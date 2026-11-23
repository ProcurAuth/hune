// SPDX-License-Identifier: MIT OR Apache-2.0
//! Connections (8.6, 8.7): TCP, TLS with the configured CA, and the pool of idle connections.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use hyper::client::conn::http1::{self, SendRequest};
use hyper_util::rt::TokioIo;
use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpSocket, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_rustls::TlsConnector;

use crate::config::{ClickHouseSourceConfig, SourceTls};
use crate::error::SourceSetupError;

/// The time that Hune keeps an idle connection (8.7).
pub(crate) const IDLE_TIMEOUT: Duration = Duration::from_secs(5);

/// The error of a connection attempt, with the class of 8.5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConnectError {
    /// A refused, reset or timed-out connection, or a TLS fault other than the certificate check.
    Unavailable(String),
    /// A failed certificate check.
    Misconfigured(String),
}

/// A plain or a TLS stream.
enum Io {
    Plain(TcpStream),
    Tls(Box<tokio_rustls::client::TlsStream<TcpStream>>),
}

impl AsyncRead for Io {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Io::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Io::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Io {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Io::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Io::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Io::Plain(s) => Pin::new(s).poll_flush(cx),
            Io::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Io::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Io::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

/// One HTTP/1.1 connection. The drop closes the socket: it stops the task that drives the
/// connection, and the task owns the socket.
pub(crate) struct Conn {
    pub(crate) sender: SendRequest<Full<Bytes>>,
    driver: JoinHandle<()>,
    idle_since: Instant,
}

impl Drop for Conn {
    fn drop(&mut self) {
        self.driver.abort();
    }
}

impl Conn {
    fn is_stale(&self, idle_timeout: Duration) -> bool {
        self.sender.is_closed() || self.idle_since.elapsed() >= idle_timeout
    }
}

/// Makes the connections of one source.
pub(crate) struct Connector {
    host: String,
    port: u16,
    tls: Option<(TlsConnector, ServerName<'static>)>,
    connect_timeout: Duration,
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connector")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("tls", &self.tls.is_some())
            .finish()
    }
}

/// Builds the rustls client configuration with the `ring` provider and TLS 1.2 and 1.3.
/// The CA certificates of the PEM bytes are the only trust roots (8.6, R6).
/// The function reads no file and no system store.
pub(crate) fn tls_client_config(tls: &SourceTls) -> Result<rustls::ClientConfig, SourceSetupError> {
    let mut roots = rustls::RootCertStore::empty();
    let mut count = 0usize;
    for item in CertificateDer::pem_slice_iter(&tls.ca_pem) {
        let cert = item.map_err(|e| SourceSetupError::new(format!("tls.ca_file: the PEM text is not valid: {e}")))?;
        roots.add(cert).map_err(|e| SourceSetupError::new(format!("tls.ca_file: a certificate is not valid: {e}")))?;
        count += 1;
    }
    if count == 0 {
        return Err(SourceSetupError::new("tls.ca_file: the PEM text holds no CERTIFICATE block"));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .map_err(|e| SourceSetupError::new(format!("tls: the protocol versions are not supported: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config)
}

impl Connector {
    pub(crate) fn new(cfg: &ClickHouseSourceConfig) -> Result<Connector, SourceSetupError> {
        let tls = match (&cfg.tls, cfg.https) {
            (Some(t), true) => {
                let name = ServerName::try_from(t.server_name.clone()).map_err(|_| {
                    SourceSetupError::new("tls.server_name: the value is not a DNS name or an IP address")
                })?;
                Some((TlsConnector::from(Arc::new(tls_client_config(t)?)), name))
            }
            (None, true) => {
                return Err(SourceSetupError::new("tls: an https source needs tls.ca_file and tls.server_name"));
            }
            (Some(_), false) => {
                return Err(SourceSetupError::new("tls: an http source takes no tls block"));
            }
            (None, false) => None,
        };
        Ok(Connector { host: cfg.host.clone(), port: cfg.port, tls, connect_timeout: cfg.connect_timeout })
    }

    /// Opens a connection within `connect_timeout`: TCP connect and TLS handshake.
    pub(crate) async fn connect(&self) -> Result<Conn, ConnectError> {
        match tokio::time::timeout(self.connect_timeout, self.connect_inner()).await {
            Ok(r) => r,
            Err(_) => Err(ConnectError::Unavailable(format!(
                "The connection to the source did not complete within {} ms.",
                self.connect_timeout.as_millis()
            ))),
        }
    }

    async fn tcp(&self) -> Result<TcpStream, ConnectError> {
        let unavailable =
            |e: &io::Error| ConnectError::Unavailable(format!("The connection to the source failed: {}.", io_kind(e)));
        let addrs: Vec<SocketAddr> =
            tokio::net::lookup_host((self.host.as_str(), self.port)).await.map_err(|e| unavailable(&e))?.collect();
        let mut last = ConnectError::Unavailable("The host of the source has no address.".to_owned());
        for addr in addrs {
            let socket = if addr.is_ipv4() { TcpSocket::new_v4() } else { TcpSocket::new_v6() };
            let socket = match socket {
                Ok(s) => s,
                Err(e) => {
                    last = unavailable(&e);
                    continue;
                }
            };
            // The idle time of 30 s of 8.7 needs `TCP_KEEPIDLE`: see the report of the crate.
            let _ = socket.set_keepalive(true);
            match socket.connect(addr).await {
                Ok(stream) => {
                    let _ = stream.set_nodelay(true);
                    return Ok(stream);
                }
                Err(e) => last = unavailable(&e),
            }
        }
        Err(last)
    }

    async fn connect_inner(&self) -> Result<Conn, ConnectError> {
        let tcp = self.tcp().await?;
        let io = match &self.tls {
            None => Io::Plain(tcp),
            Some((connector, name)) => match connector.connect(name.clone(), tcp).await {
                Ok(s) => Io::Tls(Box::new(s)),
                Err(e) => return Err(classify_tls_error(&e)),
            },
        };
        let (sender, connection) = http1::handshake::<_, Full<Bytes>>(TokioIo::new(io))
            .await
            .map_err(|_| ConnectError::Unavailable("The HTTP handshake with the source failed.".to_owned()))?;
        let driver = tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(Conn { sender, driver, idle_since: Instant::now() })
    }
}

/// Names an I/O error without a path or an address.
fn io_kind(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::ConnectionRefused => "connection refused".to_owned(),
        io::ErrorKind::ConnectionReset => "connection reset".to_owned(),
        io::ErrorKind::TimedOut => "timed out".to_owned(),
        io::ErrorKind::NotFound => "host not found".to_owned(),
        k => format!("{k}"),
    }
}

/// Splits the TLS errors: a failed certificate check is a misconfiguration (8.5).
pub(crate) fn classify_tls_error(e: &io::Error) -> ConnectError {
    let inner = e.get_ref().and_then(|i| i.downcast_ref::<rustls::Error>());
    match inner {
        Some(err @ rustls::Error::InvalidCertificate(_)) => {
            ConnectError::Misconfigured(format!("The certificate of the source failed the check: {err}."))
        }
        Some(err) => ConnectError::Unavailable(format!("The TLS handshake with the source failed: {err}.")),
        None => ConnectError::Unavailable(format!("The TLS handshake with the source failed: {}.", io_kind(e))),
    }
}

#[derive(Default)]
struct PoolState {
    idle: Vec<Conn>,
    reaper_running: bool,
}

struct PoolInner {
    state: Mutex<PoolState>,
    max_idle: usize,
    idle_timeout: Duration,
}

/// The pool of idle connections of one source (8.7).
#[derive(Clone)]
pub(crate) struct Pool {
    inner: Arc<PoolInner>,
}

impl std::fmt::Debug for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool").field("idle", &self.idle_count()).finish()
    }
}

impl Pool {
    pub(crate) fn new(max_idle: usize, idle_timeout: Duration) -> Pool {
        Pool { inner: Arc::new(PoolInner { state: Mutex::new(PoolState::default()), max_idle, idle_timeout }) }
    }

    pub(crate) fn idle_count(&self) -> usize {
        self.inner.state.lock().unwrap_or_else(PoisonError::into_inner).idle.len()
    }

    /// Takes the most recent idle connection that is still good. It closes the stale ones.
    pub(crate) fn take(&self) -> Option<Conn> {
        let mut st = self.inner.state.lock().unwrap_or_else(PoisonError::into_inner);
        while let Some(c) = st.idle.pop() {
            if !c.is_stale(self.inner.idle_timeout) {
                return Some(c);
            }
        }
        None
    }

    /// Gives a connection back. The pool keeps at most `max_idle` of them.
    pub(crate) fn put(&self, mut conn: Conn) {
        if conn.sender.is_closed() {
            return;
        }
        conn.idle_since = Instant::now();
        let start_reaper = {
            let mut st = self.inner.state.lock().unwrap_or_else(PoisonError::into_inner);
            st.idle.push(conn);
            if st.idle.len() > self.inner.max_idle {
                st.idle.remove(0);
            }
            !std::mem::replace(&mut st.reaper_running, true)
        };
        if start_reaper {
            let weak = Arc::downgrade(&self.inner);
            tokio::spawn(reap(weak));
        }
    }
}

/// Closes the idle connections that are too old. It ends when the pool has no idle connection.
async fn reap(pool: Weak<PoolInner>) {
    loop {
        let Some(inner) = pool.upgrade() else { return };
        let tick = (inner.idle_timeout / 4).clamp(Duration::from_millis(10), Duration::from_millis(500));
        drop(inner);
        tokio::time::sleep(tick).await;
        let Some(inner) = pool.upgrade() else { return };
        let mut st = inner.state.lock().unwrap_or_else(PoisonError::into_inner);
        let timeout = inner.idle_timeout;
        st.idle.retain(|c| !c.is_stale(timeout));
        if st.idle.is_empty() {
            st.reaper_running = false;
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hune_core::ids::SourceId;

    /// A listener that accepts connections and keeps them open.
    async fn listener() -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let h = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((s, _)) = l.accept().await {
                held.push(s);
            }
        });
        (addr, h)
    }

    fn connector(addr: SocketAddr) -> Connector {
        let cfg = ClickHouseSourceConfig {
            id: SourceId::parse("t").unwrap(),
            label: "t".into(),
            https: false,
            host: addr.ip().to_string(),
            port: addr.port(),
            user: "u".into(),
            password: "p".into(),
            database: "d".into(),
            tls: None,
            connect_timeout: Duration::from_secs(1),
            max_execution_time: Duration::from_secs(1),
            max_result_rows: 1,
            max_result_bytes: 1,
            max_query_size: 1,
            max_concurrent: 1,
            queue_wait: Duration::ZERO,
            max_concurrent_per_subject: None,
        };
        Connector::new(&cfg).unwrap()
    }

    #[tokio::test]
    async fn an_idle_connection_closes_after_the_idle_time() {
        let (addr, _h) = listener().await;
        let c = connector(addr);
        let pool = Pool::new(2, Duration::from_millis(200));
        pool.put(c.connect().await.unwrap());
        assert_eq!(pool.idle_count(), 1);
        assert!(pool.take().is_some());
        pool.put(c.connect().await.unwrap());
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(pool.idle_count(), 0, "the reaper closed it");
        assert!(pool.take().is_none());
    }

    #[tokio::test]
    async fn the_pool_keeps_at_most_max_idle_connections() {
        let (addr, _h) = listener().await;
        let c = connector(addr);
        let pool = Pool::new(2, Duration::from_secs(5));
        for _ in 0..4 {
            pool.put(c.connect().await.unwrap());
        }
        assert_eq!(pool.idle_count(), 2);
    }

    #[tokio::test]
    async fn take_skips_a_stale_connection() {
        let (addr, _h) = listener().await;
        let c = connector(addr);
        let pool = Pool::new(2, Duration::from_millis(50));
        pool.put(c.connect().await.unwrap());
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(pool.take().is_none());
    }

    #[tokio::test]
    async fn a_dropped_connection_closes_its_socket() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        let c = connector(addr);
        let conn = c.connect().await.unwrap();
        let (mut peer, _) = l.accept().await.unwrap();
        drop(conn);
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 1];
        let n = tokio::time::timeout(Duration::from_secs(1), peer.read(&mut buf)).await.unwrap().unwrap();
        assert_eq!(n, 0, "the peer sees the close");
    }

    #[test]
    fn a_tls_error_that_is_not_the_certificate_is_unavailable() {
        let e = io::Error::other(rustls::Error::NoCertificatesPresented);
        assert!(matches!(classify_tls_error(&e), ConnectError::Unavailable(_)));
        let e = io::Error::new(
            io::ErrorKind::InvalidData,
            rustls::Error::InvalidCertificate(rustls::CertificateError::Expired),
        );
        assert!(matches!(classify_tls_error(&e), ConnectError::Misconfigured(_)));
        let e = io::Error::from(io::ErrorKind::UnexpectedEof);
        assert!(matches!(classify_tls_error(&e), ConnectError::Unavailable(_)));
    }
}
