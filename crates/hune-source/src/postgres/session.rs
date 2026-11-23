// SPDX-License-Identifier: MIT OR Apache-2.0
//! One session to a PostgreSQL server (8a.4, 8a.5).
//!
//! A [`Session`] wraps one `tokio-postgres` client and the task of its connection. The
//! [`Connector`] opens it. The whole setup is inside `connect_timeout`: the TCP connect of each
//! address, the TLS handshake, the startup, the login and the setup check R0 (8a.4). A session
//! that is closed, or that is too old or idle for too long, is not reused (8a.9).
//!
//! # TLS
//!
//! With `verify-full` the connector uses `rustls` through `tokio-postgres-rustls`. The crate gives
//! the name of the host to the check of the certificate. The source needs `tls.server_name`
//! instead, so [`FixedNameTls`] gives this one name for each connection, whatever `host` holds.
//! The trust roots are the certificates of `tls.ca_file` only: the configuration is the one of
//! the ClickHouse client (`conn::tls_client_config`). The configuration asks for channel binding
//! (`startup::connection_config`), so a server that does not use SCRAM-SHA-256 with channel
//! binding stops the connection.
//!
//! # The size of a message
//!
//! The stream of each session is wrapped in [`Bounded`](crate::postgres::bounded::Bounded). The wrapper
//! fails the stream when the server sends a message longer than `max_result_bytes` plus 1 MiB, before the
//! client crate buffers it (finding M4). For `sslmode = "disable"` this module opens the TCP connection
//! itself and gives it to `connect_raw`. The cancel request of such a session then needs its own TCP
//! connection to `host` and `port` ([`tcp_connect`]).
//!
//! # Errors
//!
//! An [`OpenError`] has a fixed English text. It never holds text of the server, because the
//! state log lines carry it (12.15). A failed certificate check, a refused login, a refused
//! startup parameter, a channel binding fault and a failed R0 are `Misconfigured`. Each other
//! fault is `Unavailable`.

use std::error::Error as StdError;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio_postgres::tls::MakeTlsConnect;
use tokio_postgres::{Client, NoTls};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::config::{PostgresSourceConfig, PostgresSslMode};
use crate::error::SourceSetupError;
use crate::postgres::bounded::{self, Bounded, BoundedTls, Oversize};
use crate::postgres::cancel::Canceller;
use crate::postgres::errors::{self, Class};
use crate::postgres::{probe, startup};

/// A TLS connector that always checks one server name, whatever host the client dials (8a.4).
#[derive(Clone)]
pub(crate) struct FixedNameTls {
    inner: MakeRustlsConnect,
    name: String,
}

impl<S> MakeTlsConnect<S> for FixedNameTls
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type Stream = <MakeRustlsConnect as MakeTlsConnect<S>>::Stream;
    type TlsConnect = <MakeRustlsConnect as MakeTlsConnect<S>>::TlsConnect;
    type Error = <MakeRustlsConnect as MakeTlsConnect<S>>::Error;

    fn make_tls_connect(&mut self, _host: &str) -> Result<Self::TlsConnect, Self::Error> {
        <MakeRustlsConnect as MakeTlsConnect<S>>::make_tls_connect(&mut self.inner, &self.name)
    }
}

/// The TLS setting of a source, built one time.
#[derive(Clone)]
pub(crate) enum Tls {
    /// `sslmode = "disable"`.
    Plain,
    /// `sslmode = "verify-full"`.
    Rustls(FixedNameTls),
}

/// The fault of a connection setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenError {
    /// A refused, reset or timed-out connection, a TLS fault other than the certificate check, or
    /// a server error that a later try can cure.
    Unavailable(String),
    /// A failed certificate check, a refused login, a refused startup parameter, a channel
    /// binding fault or a failed setup check R0.
    Misconfigured(String),
}

/// The age after which a session goes away, when it goes back to the pool (8a.9).
pub(crate) const MAX_AGE: Duration = Duration::from_secs(3600);
/// The idle time after which the pool closes a session (8a.9).
pub(crate) const MAX_IDLE: Duration = Duration::from_secs(60);

/// One open connection with its state for the pool.
pub(crate) struct Session {
    /// The client. The code of `exec` and `probe` runs its statements.
    pub(crate) client: Client,
    task: JoinHandle<()>,
    created: Instant,
    canceller: Canceller,
    /// Set when the stream failed because the server sent a message that is too long (finding M4).
    oversize: Oversize,
}

impl Drop for Session {
    /// The drop closes the socket at once: it stops the task that owns the connection.
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Session {
    /// Gives the time since the session was opened.
    pub(crate) fn age(&self) -> Duration {
        self.created.elapsed()
    }

    /// Tells if the connection is still open on the client side.
    pub(crate) fn is_open(&self) -> bool {
        !self.client.is_closed() && !self.task.is_finished()
    }

    /// Gives the flag that tells that the stream of this session failed because a message of the server was
    /// longer than the limit (finding M4). The query then ends with the byte limit of 8a.6.
    pub(crate) fn oversize_flag(&self) -> Oversize {
        Arc::clone(&self.oversize)
    }

    /// Gives a handle that sends the CancelRequest for this session (8a.7).
    pub(crate) fn canceller(&self) -> Canceller {
        self.canceller.clone()
    }
}

/// Makes the sessions of one source. It holds the configuration, built one time.
pub(crate) struct Connector {
    config: tokio_postgres::Config,
    tls: Tls,
    expected_search_path: String,
    connect_timeout: Duration,
    cancel_gate: Arc<Semaphore>,
    host: String,
    port: u16,
    /// The longest body of a message of the server (finding M4).
    message_limit: u64,
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connector")
            .field("tls", &matches!(self.tls, Tls::Rustls(_)))
            .field("connect_timeout", &self.connect_timeout)
            .finish_non_exhaustive()
    }
}

impl Connector {
    /// Builds the connector. It does the CPU work only: it sends nothing on the network.
    ///
    /// # Errors
    ///
    /// A key of the block that cannot go in the startup packet, or TLS settings that do not
    /// match the `sslmode`, or a CA text or a server name that does not build.
    pub(crate) fn new(cfg: &PostgresSourceConfig) -> Result<Connector, SourceSetupError> {
        let config = startup::connection_config(cfg).map_err(|e| SourceSetupError::new(e.message()))?;
        let tls = match (cfg.sslmode, &cfg.tls) {
            (PostgresSslMode::VerifyFull, Some(t)) => {
                rustls::pki_types::ServerName::try_from(t.server_name.clone()).map_err(|_| {
                    SourceSetupError::new("tls.server_name: the value is not a DNS name or an IP address")
                })?;
                let rustls_config = crate::conn::tls_client_config(t)?;
                Tls::Rustls(FixedNameTls { inner: MakeRustlsConnect::new(rustls_config), name: t.server_name.clone() })
            }
            (PostgresSslMode::VerifyFull, None) => {
                return Err(SourceSetupError::new(
                    "tls: a source with sslmode verify-full needs tls.ca_file and tls.server_name",
                ));
            }
            (PostgresSslMode::Disable, Some(_)) => {
                return Err(SourceSetupError::new("tls: a source with sslmode disable takes no tls block"));
            }
            (PostgresSslMode::Disable, None) => Tls::Plain,
        };
        Ok(Connector {
            config,
            tls,
            expected_search_path: probe::expected_search_path(&cfg.search_path),
            connect_timeout: cfg.connect_timeout,
            // One cancel request at a time: a source holds `max_concurrent + 2` sessions at most (8a.9).
            cancel_gate: Arc::new(Semaphore::new(1)),
            host: cfg.host.clone(),
            port: cfg.port,
            message_limit: bounded::message_limit(cfg.max_result_bytes),
        })
    }

    /// Opens a session within `connect_timeout`: TCP, TLS, startup, login and the check R0.
    ///
    /// # Errors
    ///
    /// The fault, with a fixed text.
    pub(crate) async fn open(&self) -> Result<Session, OpenError> {
        match tokio::time::timeout(self.connect_timeout, self.open_inner()).await {
            Ok(r) => r,
            Err(_) => Err(OpenError::Unavailable(format!(
                "The connection to the source did not complete within {} ms.",
                self.connect_timeout.as_millis()
            ))),
        }
    }

    async fn open_inner(&self) -> Result<Session, OpenError> {
        let oversize: Oversize = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (client, task) = match &self.tls {
            Tls::Plain => {
                let tcp = tcp_connect(&self.host, self.port).await?;
                let stream = Bounded::new(tcp, self.message_limit, Arc::clone(&oversize));
                let (client, connection) = self.config.connect_raw(stream, NoTls).await.map_err(|e| open_error(&e))?;
                (client, tokio::spawn(async move { drop(connection.await) }))
            }
            Tls::Rustls(tls) => {
                let bounded = BoundedTls::new(tls.clone(), self.message_limit, Arc::clone(&oversize));
                let (client, connection) = self.config.connect(bounded).await.map_err(|e| open_error(&e))?;
                (client, tokio::spawn(async move { drop(connection.await) }))
            }
        };
        let canceller = Canceller::new(
            client.cancel_token(),
            self.tls.clone(),
            (self.host.clone(), self.port),
            self.connect_timeout,
            &self.cancel_gate,
        );
        // From here a drop of the session closes the connection, also at the timeout of the setup.
        let session = Session { client, task, created: Instant::now(), canceller, oversize };
        let setup = probe::read_setup(&session.client).await.map_err(|e| open_error(&e))?;
        probe::check_r0(&setup, &self.expected_search_path).map_err(|f| OpenError::Misconfigured(f.detail))?;
        Ok(session)
    }
}

/// Opens one TCP connection to `host` and `port`, as the client crate does: each address in turn, `TCP_NODELAY`
/// and keepalive on. The caller bounds the time. The idle time of the keepalive is the one of the system
///.
///
/// # Errors
///
/// `Unavailable` with a fixed text: a refused connection, an unknown host.
pub(crate) async fn tcp_connect(host: &str, port: u16) -> Result<TcpStream, OpenError> {
    let unavailable =
        |e: &io::Error| OpenError::Unavailable(format!("The connection to the source failed: {}.", io_text(e.kind())));
    let addrs: Vec<std::net::SocketAddr> =
        tokio::net::lookup_host((host, port)).await.map_err(|e| unavailable(&e))?.collect();
    let mut last = OpenError::Unavailable("The connection to the source failed: host not found.".to_owned());
    for addr in addrs {
        let socket = if addr.is_ipv4() { TcpSocket::new_v4() } else { TcpSocket::new_v6() };
        let socket = match socket {
            Ok(s) => s,
            Err(e) => {
                last = unavailable(&e);
                continue;
            }
        };
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

/// Finds a `rustls` error in the chain of causes of an error, also inside an `io::Error`.
fn find_rustls<'a>(error: &'a (dyn StdError + 'static)) -> Option<&'a rustls::Error> {
    let mut current: Option<&(dyn StdError + 'static)> = Some(error);
    while let Some(e) = current {
        if let Some(r) = e.downcast_ref::<rustls::Error>() {
            return Some(r);
        }
        if let Some(inner) = e.downcast_ref::<io::Error>().and_then(io::Error::get_ref)
            && let Some(r) = inner.downcast_ref::<rustls::Error>()
        {
            return Some(r);
        }
        current = e.source();
    }
    None
}

/// Finds the kind of an `io::Error` in the chain of causes.
fn find_io_kind(error: &(dyn StdError + 'static)) -> Option<io::ErrorKind> {
    let mut current: Option<&(dyn StdError + 'static)> = Some(error);
    while let Some(e) = current {
        if let Some(io) = e.downcast_ref::<io::Error>() {
            return Some(io.kind());
        }
        current = e.source();
    }
    None
}

/// The reason of a failed certificate check, as a fixed text.
fn certificate_reason(error: &rustls::Error) -> &'static str {
    use rustls::CertificateError as C;
    match error {
        rustls::Error::InvalidCertificate(C::UnknownIssuer) => "the issuer is not in tls.ca_file",
        rustls::Error::InvalidCertificate(C::NotValidForName | C::NotValidForNameContext { .. }) => {
            "the certificate is not valid for tls.server_name"
        }
        rustls::Error::InvalidCertificate(C::Expired | C::ExpiredContext { .. }) => "the certificate is expired",
        rustls::Error::InvalidCertificate(C::NotValidYet | C::NotValidYetContext { .. }) => {
            "the certificate is not valid yet"
        }
        _ => "the certificate is not valid",
    }
}

/// Names an I/O fault without a path or an address.
fn io_text(kind: io::ErrorKind) -> String {
    match kind {
        io::ErrorKind::ConnectionRefused => "connection refused".to_owned(),
        io::ErrorKind::ConnectionReset => "connection reset".to_owned(),
        io::ErrorKind::TimedOut => "timed out".to_owned(),
        io::ErrorKind::NotFound => "host not found".to_owned(),
        k => format!("{k}"),
    }
}

/// Maps an error of the setup to an [`OpenError`] with a fixed text (8a.8).
pub(crate) fn open_error(error: &tokio_postgres::Error) -> OpenError {
    if let Some(state) = errors::sqlstate_of(error) {
        return match errors::classify(&state) {
            Class::Login => OpenError::Misconfigured(format!(
                "The server refused the login or the database of the account (SQLSTATE {state})."
            )),
            _ if matches!(state.as_str(), "22023" | "42704") => OpenError::Misconfigured(format!(
                "The server refused a startup parameter of the source (SQLSTATE {state})."
            )),
            _ => OpenError::Unavailable(format!("The PostgreSQL server refused the connection (SQLSTATE {state}).")),
        };
    }
    if error.as_db_error().is_some() {
        return OpenError::Unavailable("The PostgreSQL server refused the connection.".to_owned());
    }
    if let Some(r) = find_rustls(error) {
        if matches!(r, rustls::Error::InvalidCertificate(_)) {
            return OpenError::Misconfigured(format!(
                "The certificate of the source failed the check: {}.",
                certificate_reason(r)
            ));
        }
        return OpenError::Unavailable("The TLS handshake with the source failed.".to_owned());
    }
    // The text is the one of the client crate for a refused channel binding, not text of the server.
    if error.source().is_some_and(|s| s.to_string() == "server did not use channel binding") {
        return OpenError::Misconfigured(
            "The server did not use SCRAM-SHA-256 with channel binding, and the source requires it.".to_owned(),
        );
    }
    match find_io_kind(error) {
        Some(kind) => OpenError::Unavailable(format!("The connection to the source failed: {}.", io_text(kind))),
        None => OpenError::Unavailable("The connection to the source failed.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SourceTls;
    use hune_core::ids::SourceId;

    fn cfg() -> PostgresSourceConfig {
        PostgresSourceConfig {
            id: SourceId::parse("metrics").expect("id"),
            label: "Metrics".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 5432,
            database: "analytics".to_owned(),
            user: "hune_ro".to_owned(),
            password: "pw".to_owned(),
            search_path: vec!["metrics".to_owned()],
            sslmode: PostgresSslMode::Disable,
            tls: None,
            connect_timeout: Duration::from_secs(5),
            max_execution_time: Duration::from_secs(60),
            max_result_rows: 1000,
            max_result_bytes: 1_000_000,
            max_query_size: 100_000,
            max_concurrent: 4,
            queue_wait: Duration::from_secs(1),
            max_concurrent_per_subject: None,
        }
    }

    #[test]
    fn tls_settings_must_match_the_sslmode() {
        assert!(Connector::new(&cfg()).is_ok());
        let mut c = cfg();
        c.tls = Some(SourceTls { ca_pem: Vec::new(), server_name: "localhost".to_owned() });
        assert!(Connector::new(&c).unwrap_err().message().contains("takes no tls block"));
        let mut c = cfg();
        c.sslmode = PostgresSslMode::VerifyFull;
        assert!(Connector::new(&c).unwrap_err().message().contains("needs tls.ca_file"));
        // A CA text with no certificate is a setup error, not a later fault.
        c.tls = Some(SourceTls { ca_pem: b"not a pem".to_vec(), server_name: "localhost".to_owned() });
        assert!(Connector::new(&c).unwrap_err().message().contains("tls.ca_file"));
    }

    #[test]
    fn a_wrong_server_name_is_a_setup_error() {
        let mut c = cfg();
        c.sslmode = PostgresSslMode::VerifyFull;
        c.tls = Some(SourceTls { ca_pem: Vec::new(), server_name: "not a name!".to_owned() });
        assert!(Connector::new(&c).unwrap_err().message().contains("tls.server_name"));
    }

    #[test]
    fn a_host_that_is_a_path_does_not_build() {
        let mut c = cfg();
        c.host = "/var/run/postgresql".to_owned();
        assert!(Connector::new(&c).unwrap_err().message().contains("`host`"));
    }

    #[test]
    fn certificate_faults_have_fixed_texts() {
        use rustls::CertificateError as C;
        let cases = [
            (C::UnknownIssuer, "issuer"),
            (C::NotValidForName, "tls.server_name"),
            (C::Expired, "expired"),
            (C::BadSignature, "not valid"),
        ];
        for (e, word) in cases {
            let r = rustls::Error::InvalidCertificate(e);
            assert!(certificate_reason(&r).contains(word), "{r:?}");
        }
    }

    #[test]
    fn find_rustls_sees_the_error_inside_an_io_error() {
        let io = io::Error::new(
            io::ErrorKind::InvalidData,
            rustls::Error::InvalidCertificate(rustls::CertificateError::Expired),
        );
        assert!(matches!(find_rustls(&io), Some(rustls::Error::InvalidCertificate(_))));
        let plain = io::Error::from(io::ErrorKind::ConnectionRefused);
        assert!(find_rustls(&plain).is_none());
        assert_eq!(find_io_kind(&plain), Some(io::ErrorKind::ConnectionRefused));
    }
}
