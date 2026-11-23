// SPDX-License-Identifier: MIT OR Apache-2.0
//! A size limit on each message of the server (8a.6, 8a.15, finding M4).
//!
//! `tokio-postgres` reads a whole `DataRow` message into memory before it gives the row to the code.
//! A row with a field of 300 MB then costs 300 MB, and four such queries cost 1.2 GB, while the admission
//! of 8a.10 reserves `2 * max_result_bytes` for each query. The server can send a field of 1 GiB. The
//! check of the bytes in `frame_build` comes after the whole row is in memory, so it cannot protect it.
//!
//! [`Bounded`] wraps the stream below the client crate. It follows the framing of the protocol: one
//! byte for the type, four bytes for the length (the length counts itself), and the body. It reads the
//! header of each message, and it fails the stream at once when the body is longer than the limit,
//! before the client crate buffers the body. The limit is `max_result_bytes` plus [`OVERHEAD`] for the
//! row framing, the names of the columns and the notices. The limit is the same for each message type:
//! the text of an error message can hold a whole value, too.
//!
//! The failure sets a flag. The code of `exec` reads the flag: a failed stream with the flag set is the
//! byte limit of 8a.6 (`422 query_limit`), not a transport fault. The session is closed in each case.
//!
//! # Where the wrapper sits
//!
//! - `sslmode = "disable"`: the session builds the TCP stream itself, wraps it in [`Bounded`], and gives it to
//!   `Config::connect_raw`. The first byte that the server sends is the first byte of a message.
//! - `sslmode = "verify-full"`: [`BoundedTls`] wraps the TLS connector, so [`Bounded`] sits above the
//!   TLS stream and reads the plain messages. The wrapper never sees the bytes of the handshake.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_postgres::tls::{ChannelBinding, MakeTlsConnect, TlsConnect, TlsStream};

/// The bytes that a message can hold above `max_result_bytes`: the framing of the row, the names of the
/// columns, a notice (1 MiB).
pub(crate) const OVERHEAD: u64 = 1024 * 1024;

/// Gives the longest body of a message for a source with this `max_result_bytes`.
pub(crate) fn message_limit(max_result_bytes: u64) -> u64 {
    max_result_bytes.saturating_add(OVERHEAD)
}

/// The flag that tells that the stream failed because a message was too long.
pub(crate) type Oversize = Arc<AtomicBool>;

/// What the reader knows about the framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Framing {
    /// The longest body that a message can have.
    limit: u64,
    /// The bytes of the header that came so far (the type and four bytes of the length).
    header: [u8; 5],
    /// How many bytes of `header` are filled.
    header_len: usize,
    /// The bytes of the body of the current message that did not come yet.
    remaining: u64,
}

/// A message is too long: the length that its header gives, and the limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooLong {
    /// The type byte of the message.
    pub tag: u8,
    /// The length of the body, in bytes.
    pub body: u64,
}

impl Framing {
    fn new(limit: u64) -> Framing {
        Framing { limit, header: [0; 5], header_len: 0, remaining: 0 }
    }

    /// Reads the bytes that came from the server. It gives the first message that is too long.
    ///
    /// A length below 4 is not a valid message. The function does not judge it: the client crate does.
    fn feed(&mut self, mut data: &[u8]) -> Result<(), TooLong> {
        while !data.is_empty() {
            if self.remaining > 0 {
                let take = usize::try_from(self.remaining).unwrap_or(usize::MAX).min(data.len());
                data = &data[take..];
                self.remaining -= take as u64;
                continue;
            }
            let want = 5 - self.header_len;
            let take = want.min(data.len());
            self.header[self.header_len..self.header_len + take].copy_from_slice(&data[..take]);
            self.header_len += take;
            data = &data[take..];
            if self.header_len == 5 {
                self.header_len = 0;
                let length = u32::from_be_bytes([self.header[1], self.header[2], self.header[3], self.header[4]]);
                let body = u64::from(length.saturating_sub(4));
                if body > self.limit {
                    return Err(TooLong { tag: self.header[0], body });
                }
                self.remaining = body;
            }
        }
        Ok(())
    }
}

/// A stream that fails when the server sends a message that is too long.
pub(crate) struct Bounded<S> {
    inner: S,
    framing: Framing,
    oversize: Oversize,
    failed: bool,
}

impl<S> Bounded<S> {
    /// Wraps a stream. `limit` is the longest body of a message. `oversize` is set when the stream fails.
    pub(crate) fn new(inner: S, limit: u64, oversize: Oversize) -> Bounded<S> {
        Bounded { inner, framing: Framing::new(limit), oversize, failed: false }
    }
}

fn too_long_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "a message of the server is longer than the limit")
}

impl<S: AsyncRead + Unpin> AsyncRead for Bounded<S> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if self.failed {
            return Poll::Ready(Err(too_long_error()));
        }
        let before = buf.filled().len();
        ready!(Pin::new(&mut self.inner).poll_read(cx, buf))?;
        let this = &mut *self;
        if let Err(TooLong { .. }) = this.framing.feed(&buf.filled()[before..]) {
            this.failed = true;
            this.oversize.store(true, Ordering::Release);
            // The bytes that came stay out of the client crate: it must not buffer the body.
            buf.set_filled(before);
            return Poll::Ready(Err(too_long_error()));
        }
        Poll::Ready(Ok(()))
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Bounded<S> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, data: &[u8]) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, data)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl<S: TlsStream + Unpin> TlsStream for Bounded<S> {
    fn channel_binding(&self) -> ChannelBinding {
        self.inner.channel_binding()
    }
}

/// A TLS connector whose streams are [`Bounded`].
#[derive(Clone)]
pub(crate) struct BoundedTls<M> {
    inner: M,
    limit: u64,
    oversize: Oversize,
}

impl<M> BoundedTls<M> {
    /// Wraps a TLS connector for one connection.
    pub(crate) fn new(inner: M, limit: u64, oversize: Oversize) -> BoundedTls<M> {
        BoundedTls { inner, limit, oversize }
    }
}

impl<S, M> MakeTlsConnect<S> for BoundedTls<M>
where
    M: MakeTlsConnect<S>,
    <M::TlsConnect as TlsConnect<S>>::Future: Unpin,
{
    type Stream = Bounded<M::Stream>;
    type TlsConnect = BoundedConnect<M::TlsConnect>;
    type Error = M::Error;

    fn make_tls_connect(&mut self, domain: &str) -> Result<Self::TlsConnect, Self::Error> {
        Ok(BoundedConnect {
            inner: self.inner.make_tls_connect(domain)?,
            limit: self.limit,
            oversize: Arc::clone(&self.oversize),
        })
    }
}

/// The connector that [`BoundedTls`] makes.
pub(crate) struct BoundedConnect<C> {
    inner: C,
    limit: u64,
    oversize: Oversize,
}

impl<S, C> TlsConnect<S> for BoundedConnect<C>
where
    C: TlsConnect<S>,
    C::Future: Unpin,
{
    type Stream = Bounded<C::Stream>;
    type Error = C::Error;
    type Future = BoundedFuture<C::Future>;

    fn connect(self, stream: S) -> Self::Future {
        BoundedFuture { inner: self.inner.connect(stream), limit: self.limit, oversize: self.oversize }
    }
}

/// The future of the TLS handshake. Its output is a [`Bounded`] stream.
pub(crate) struct BoundedFuture<F> {
    inner: F,
    limit: u64,
    oversize: Oversize,
}

impl<F, T, E> Future for BoundedFuture<F>
where
    F: Future<Output = Result<T, E>> + Unpin,
{
    type Output = Result<Bounded<T>, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let stream = ready!(Pin::new(&mut self.inner).poll(cx))?;
        let (limit, oversize) = (self.limit, Arc::clone(&self.oversize));
        Poll::Ready(Ok(Bounded::new(stream, limit, oversize)))
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    /// One message: the type, the length (body plus 4) and a body of `body` bytes.
    fn message(tag: u8, body: usize) -> Vec<u8> {
        let mut m = vec![tag];
        m.extend_from_slice(&u32::try_from(body + 4).unwrap().to_be_bytes());
        m.resize(5 + body, b'x');
        m
    }

    #[test]
    fn the_limit_adds_one_mebibyte() {
        assert_eq!(message_limit(0), 1 << 20);
        assert_eq!(message_limit(33_554_432), 33_554_432 + (1 << 20));
        assert_eq!(message_limit(u64::MAX), u64::MAX);
    }

    #[test]
    fn a_message_up_to_the_limit_passes_in_one_piece_or_in_single_bytes() {
        let mut stream = Vec::new();
        stream.extend(message(b'D', 100));
        stream.extend(message(b'C', 0));
        stream.extend(message(b'D', 1000));
        let mut whole = Framing::new(1000);
        assert_eq!(whole.feed(&stream), Ok(()));
        let mut bytes = Framing::new(1000);
        for b in &stream {
            assert_eq!(bytes.feed(std::slice::from_ref(b)), Ok(()));
        }
        assert_eq!(whole, bytes);
    }

    #[test]
    fn a_message_above_the_limit_fails_at_its_header() {
        let mut f = Framing::new(1000);
        let long = message(b'D', 1001);
        // Only the header is needed: the body never has to arrive.
        assert_eq!(f.feed(&long[..5]), Err(TooLong { tag: b'D', body: 1001 }));
        // The header can come in pieces.
        let mut f = Framing::new(1000);
        assert_eq!(f.feed(&long[..2]), Ok(()));
        assert_eq!(f.feed(&long[2..5]), Err(TooLong { tag: b'D', body: 1001 }));
        // The same limit holds for another type, and after other messages.
        let mut f = Framing::new(1000);
        let mut stream = message(b'T', 10);
        stream.extend(message(b'E', 5000));
        assert_eq!(f.feed(&stream), Err(TooLong { tag: b'E', body: 5000 }));
    }

    #[test]
    fn a_huge_length_does_not_overflow() {
        let mut f = Framing::new(1 << 20);
        let header = [b'D', 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(f.feed(&header), Err(TooLong { tag: b'D', body: u64::from(u32::MAX) - 4 }));
        // A length below 4 is not judged here.
        let mut f = Framing::new(10);
        assert_eq!(f.feed(&[b'D', 0, 0, 0, 0]), Ok(()));
    }

    #[tokio::test]
    async fn the_stream_fails_before_it_gives_the_body_and_sets_the_flag() {
        let (mut server, client) = tokio::io::duplex(64 * 1024);
        let flag: Oversize = Arc::new(AtomicBool::new(false));
        let mut bounded = Bounded::new(client, 100, Arc::clone(&flag));
        server.write_all(&message(b'D', 50)).await.unwrap();
        let mut got = vec![0u8; 55];
        bounded.read_exact(&mut got).await.unwrap();
        assert!(!flag.load(Ordering::Acquire));
        // A long message: the reader gets an error, never the body.
        server.write_all(&message(b'D', 5000)[..2000]).await.unwrap();
        let mut sink = vec![0u8; 4096];
        let e = bounded.read(&mut sink).await.unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert!(flag.load(Ordering::Acquire));
        // The stream stays failed.
        assert!(bounded.read(&mut sink).await.is_err());
    }

    #[tokio::test]
    async fn the_writes_pass_through() {
        let (mut server, client) = tokio::io::duplex(1024);
        let mut bounded = Bounded::new(client, 10, Arc::new(AtomicBool::new(false)));
        bounded.write_all(b"hello").await.unwrap();
        bounded.flush().await.unwrap();
        let mut got = [0u8; 5];
        server.read_exact(&mut got).await.unwrap();
        assert_eq!(&got, b"hello");
    }
}
