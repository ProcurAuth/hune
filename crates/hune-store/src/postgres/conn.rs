//! Connection settings, TLS and the pool of the PostgreSQL backend (5.23).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{Hook, HookError, Manager, ManagerConfig, Pool, PoolError, RecyclingMethod, Runtime};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_postgres::config::{SslMode, TargetSessionAttrs};
use tokio_postgres::tls::MakeTlsConnect;
use tokio_postgres::{Config, NoTls};
use tokio_postgres_rustls::MakeRustlsConnect;

use super::{PgOpenError, PgOpenErrorKind};

/// The TLS mode of the connection (5.23, items 3 and 4).
#[derive(Clone, PartialEq, Eq)]
pub enum PgSslMode {
    /// Plain TCP.
    Disable,
    /// TLS 1.2 or 1.3. The root store holds only the certificates of `ca_pem`. The server
    /// certificate MUST hold `server_name`.
    VerifyFull {
        /// The content of the PEM file `storage.postgres.tls.ca_file`.
        ca_pem: Vec<u8>,
        /// The name that the certificate must hold (`storage.postgres.tls.server_name`).
        server_name: String,
    },
}

impl fmt::Debug for PgSslMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PgSslMode::Disable => f.write_str("Disable"),
            PgSslMode::VerifyFull { ca_pem, server_name } => {
                write!(f, "VerifyFull {{ ca_pem: {} bytes, server_name: {server_name:?} }}", ca_pem.len())
            }
        }
    }
}

/// The options of the PostgreSQL backend (13.10, block `storage.postgres`).
#[derive(Clone)]
pub struct PgOptions {
    /// `storage.postgres.host`.
    pub host: String,
    /// `storage.postgres.port`.
    pub port: u16,
    /// `storage.postgres.database`.
    pub database: String,
    /// `storage.postgres.user`.
    pub user: String,
    /// The password, read from `password_env` or `password_file` by the caller. `None` sends none.
    pub password: Option<String>,
    /// `storage.postgres.sslmode` with the CA bytes and the server name.
    pub sslmode: PgSslMode,
    /// `storage.postgres.max_connections`: the size of the pool.
    pub max_connections: usize,
    /// `storage.postgres.connect_timeout`.
    pub connect_timeout: Duration,
    /// `storage.postgres.statement_timeout`. It also bounds the wait for a pooled connection.
    pub statement_timeout: Duration,
    /// `storage.postgres.poll_interval`.
    pub poll_interval: Duration,
    /// `storage.postgres.history_keep`.
    pub history_keep: u32,
    /// `true` for `serve`: the start checks include step 6 of 5.29 (the role).
    pub check_role: bool,
}

impl fmt::Debug for PgOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgOptions")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "<hidden>"))
            .field("sslmode", &self.sslmode)
            .field("max_connections", &self.max_connections)
            .field("connect_timeout", &self.connect_timeout)
            .field("statement_timeout", &self.statement_timeout)
            .field("poll_interval", &self.poll_interval)
            .field("history_keep", &self.history_keep)
            .field("check_role", &self.check_role)
            .finish()
    }
}

/// A TLS connector that always checks one server name, whatever host the client dials.
/// So `host` can be an address or a service name while the certificate holds `server_name`.
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

    fn make_tls_connect(&mut self, _domain: &str) -> Result<Self::TlsConnect, Self::Error> {
        <MakeRustlsConnect as MakeTlsConnect<S>>::make_tls_connect(&mut self.inner, &self.name)
    }
}

/// The TLS setting of a backend, built one time.
#[derive(Clone)]
pub(crate) enum Tls {
    None,
    Rustls(FixedNameTls),
}

fn config_error(message: String) -> PgOpenError {
    PgOpenError { kind: PgOpenErrorKind::Permanent, message }
}

/// Builds the rustls client configuration of R6. It uses the ring provider and TLS 1.2 and 1.3.
/// The root store holds only the certificates of the configured PEM bytes. It never reads the system roots.
pub(crate) fn build_tls(mode: &PgSslMode) -> Result<Tls, PgOpenError> {
    let PgSslMode::VerifyFull { ca_pem, server_name } = mode else { return Ok(Tls::None) };
    let mut roots = rustls::RootCertStore::empty();
    let mut count = 0usize;
    for item in CertificateDer::pem_slice_iter(ca_pem) {
        let cert =
            item.map_err(|e| config_error(format!("storage.postgres.tls.ca_file: not a PEM certificate: {e}")))?;
        roots
            .add(cert)
            .map_err(|e| config_error(format!("storage.postgres.tls.ca_file: a certificate is not usable: {e}")))?;
        count += 1;
    }
    if count == 0 {
        return Err(config_error("storage.postgres.tls.ca_file: the file holds no certificate".to_owned()));
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .map_err(|e| config_error(format!("storage.postgres: TLS versions: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Tls::Rustls(FixedNameTls { inner: MakeRustlsConnect::new(config), name: server_name.clone() }))
}

/// Builds the `tokio-postgres` configuration of 5.23.
///
/// The settings: `application_name` `hune`, `target_session_attrs` `read-write`, the connect
/// timeout, TCP keepalive (idle 10 s, interval 5 s, 3 probes) and `tcp_user_timeout` 10 s.
pub(crate) fn build_config(opts: &PgOptions) -> Config {
    let mut c = Config::new();
    c.host(&opts.host);
    c.port(opts.port);
    c.dbname(&opts.database);
    c.user(&opts.user);
    if let Some(p) = &opts.password {
        c.password(p);
    }
    c.application_name("hune");
    c.connect_timeout(opts.connect_timeout);
    c.keepalives(true);
    c.keepalives_idle(Duration::from_secs(10));
    c.keepalives_interval(Duration::from_secs(5));
    c.keepalives_retries(3);
    c.tcp_user_timeout(Duration::from_secs(10));
    c.target_session_attrs(TargetSessionAttrs::ReadWrite);
    c.ssl_mode(match opts.sslmode {
        PgSslMode::Disable => SslMode::Disable,
        PgSslMode::VerifyFull { .. } => SslMode::Require,
    });
    c
}

/// The batch that each new connection receives (5.23, item 1).
pub(crate) fn session_batch(opts: &PgOptions) -> String {
    let ms = opts.statement_timeout.as_millis().max(1);
    format!(
        "SET statement_timeout = {ms}; SET idle_in_transaction_session_timeout = {ms}; \
         SET search_path = public; SET timezone = 'UTC'"
    )
}

/// Builds the pool. It sends nothing on the network.
pub(crate) fn build_pool(opts: &PgOptions, tls: &Tls) -> Result<Pool, PgOpenError> {
    let cfg = build_config(opts);
    let mgr_cfg = ManagerConfig { recycling_method: RecyclingMethod::Fast };
    let manager = match tls {
        Tls::None => Manager::from_config(cfg, NoTls, mgr_cfg),
        Tls::Rustls(t) => Manager::from_config(cfg, t.clone(), mgr_cfg),
    };
    let batch = session_batch(opts);
    Pool::builder(manager)
        .max_size(opts.max_connections.max(1))
        .wait_timeout(Some(opts.statement_timeout))
        .create_timeout(Some(opts.connect_timeout))
        .runtime(Runtime::Tokio1)
        .post_create(Hook::async_fn(move |client, _| {
            let batch = batch.clone();
            Box::pin(async move { client.batch_execute(&batch).await.map_err(HookError::Backend) })
        }))
        .build()
        .map_err(|e| config_error(format!("storage.postgres: the pool cannot be built: {e}")))
}

/// One direct session, outside the pool (5.28, step 2). The connection task stops on drop.
pub(crate) struct Direct {
    pub client: tokio_postgres::Client,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Direct {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Opens a direct session and sends the session batch.
pub(crate) async fn connect_direct(opts: &PgOptions, tls: &Tls) -> Result<Direct, tokio_postgres::Error> {
    let cfg = build_config(opts);
    let (client, task) = match tls {
        Tls::None => {
            let (client, conn) = cfg.connect(NoTls).await?;
            (client, tokio::spawn(async move { drop(conn.await) }))
        }
        Tls::Rustls(t) => {
            let (client, conn) = cfg.connect(t.clone()).await?;
            (client, tokio::spawn(async move { drop(conn.await) }))
        }
    };
    let direct = Direct { client, task };
    direct.client.batch_execute(&session_batch(opts)).await?;
    Ok(direct)
}

/// Tells if an error leaves the connection unusable (5.23, table of errors). This is the case
/// for an I/O error, a timeout, a TLS error, a closed connection, SQLSTATE class 08, `57P01`,
/// `57P02`, `57P03` and `25006`. An error without SQLSTATE is of this kind.
pub(crate) fn is_broken(e: &tokio_postgres::Error) -> bool {
    match e.code() {
        Some(c) => {
            let s = c.code();
            s.starts_with("08") || matches!(s, "57P01" | "57P02" | "57P03" | "25006")
        }
        None => true,
    }
}

/// Tells if the error chain holds a rustls error (a failed certificate check, for example).
fn is_tls_failure(e: &tokio_postgres::Error) -> bool {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(e);
    while let Some(err) = cur {
        if err.downcast_ref::<rustls::Error>().is_some() {
            return true;
        }
        if let Some(io) = err.downcast_ref::<std::io::Error>()
            && io.get_ref().is_some_and(|inner| inner.downcast_ref::<rustls::Error>().is_some())
        {
            return true;
        }
        cur = err.source();
    }
    let text = e.to_string();
    text.contains("TLS") || text.contains("certificate")
}

/// Classifies a connect error as temporary or permanent (5.29, step 1).
pub(crate) fn classify_connect(e: &tokio_postgres::Error) -> PgOpenErrorKind {
    if let Some(c) = e.code() {
        let s = c.code();
        if s.starts_with("28") || s == "3D000" {
            return PgOpenErrorKind::Permanent;
        }
        return PgOpenErrorKind::Temporary;
    }
    if is_tls_failure(e) { PgOpenErrorKind::Permanent } else { PgOpenErrorKind::Temporary }
}

/// The text of a database error with its SQLSTATE, for the log.
pub(crate) fn describe(e: &tokio_postgres::Error) -> String {
    use std::error::Error as _;
    match e.as_db_error() {
        Some(db) => format!("{} (SQLSTATE {})", db.message(), db.code().code()),
        None => match e.source() {
            Some(s) => format!("{e}: {s}"),
            None => e.to_string(),
        },
    }
}

/// Classifies the error of the pool: the cause of a failed connection.
pub(crate) fn classify_pool(e: &PoolError) -> PgOpenErrorKind {
    match e {
        PoolError::Backend(pe) => classify_connect(pe),
        PoolError::PostCreateHook(HookError::Backend(pe)) => classify_connect(pe),
        _ => PgOpenErrorKind::Temporary,
    }
}

/// The text of a pool error, for the log.
pub(crate) fn describe_pool(e: &PoolError) -> String {
    match e {
        PoolError::Backend(pe) => describe(pe),
        PoolError::PostCreateHook(HookError::Backend(pe)) => describe(pe),
        PoolError::Timeout(t) => format!("timeout while waiting for a connection ({t:?})"),
        other => other.to_string(),
    }
}
