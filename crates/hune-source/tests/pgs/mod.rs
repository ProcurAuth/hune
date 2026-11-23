//! Helpers of the container tests of the PostgreSQL source (16.9a).
//!
//! The script `tests/ci/pgsource.sh` starts a disposable PostgreSQL on 127.0.0.1 and sets the
//! variables `HUNE_TEST_PGS_*`. The tests are `#[ignore]`: a plain `cargo test` skips them.
//! Tests are not product code: they can read the environment and the file system.
//!
//! The module has the proxy of 16.9a. It sits between the client and the server, records each
//! message of the frontend (the type byte and the body), and records the process id of each
//! backend. A test can drop the cancel requests, and stop and start the proxy to make a server
//! that goes away and comes back.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use hune_core::frame::{Frame, FrameValue};
use hune_source::{
    EffectiveLimits, LogComment, LogKind, PostgresSource, PostgresSourceConfig, PostgresSslMode, PreparedQuery,
    QueryCtx, QueryKind, QueueWait, ResultMemoryBudget, SourceError, SourceTls, SubjectId, Uuid,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub const REASON: &str = "needs a PostgreSQL container: run tests/ci/pgsource.sh";

pub fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: {REASON}"))
}

pub fn port() -> u16 {
    env("HUNE_TEST_PGS_PORT").parse().expect("HUNE_TEST_PGS_PORT is a port")
}

pub fn version() -> u32 {
    env("HUNE_TEST_PGS_VERSION").parse().expect("version")
}

/// The config of a source for the role `user`. `host_port` is the server, or a proxy.
pub fn config_for(user: &str, password: &str, host_port: (&str, u16)) -> PostgresSourceConfig {
    PostgresSourceConfig {
        id: hune_core::ids::SourceId::parse("pgs").expect("id"),
        label: "Test".to_owned(),
        host: host_port.0.to_owned(),
        port: host_port.1,
        database: env("HUNE_TEST_PGS_DATABASE"),
        user: user.to_owned(),
        password: password.to_owned(),
        search_path: vec![env("HUNE_TEST_PGS_SCHEMA")],
        sslmode: PostgresSslMode::Disable,
        tls: None,
        connect_timeout: Duration::from_secs(5),
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 1_000_000,
        max_result_bytes: 1_000_000,
        max_query_size: 1_000_000,
        max_concurrent: 2,
        queue_wait: Duration::from_secs(15),
        max_concurrent_per_subject: None,
    }
}

/// The config of the role `hune_ro`, on the server.
pub fn ro_config() -> PostgresSourceConfig {
    config_for(&env("HUNE_TEST_PGS_RO_USER"), &env("HUNE_TEST_PGS_RO_PASSWORD"), (&env("HUNE_TEST_PGS_HOST"), port()))
}

/// The config of the role `hune_ro`, with TLS `verify-full` and the test CA.
pub fn tls_config(ca_file: &str, server_name: &str) -> PostgresSourceConfig {
    let mut c = ro_config();
    c.sslmode = PostgresSslMode::VerifyFull;
    c.tls = Some(SourceTls {
        ca_pem: std::fs::read(ca_file).unwrap_or_else(|e| panic!("{ca_file}: {e}")),
        server_name: server_name.to_owned(),
    });
    c
}

pub fn make_source(config: PostgresSourceConfig) -> PostgresSource {
    PostgresSource::new(config, ResultMemoryBudget::new(1 << 30)).expect("the source builds")
}

pub fn limits(rows: u64, bytes: u64, secs: u64) -> EffectiveLimits {
    EffectiveLimits {
        max_execution_time: Duration::from_secs(secs),
        max_result_rows: rows,
        max_result_bytes: bytes,
        max_query_size: 1_000_000,
        rows_from_ceiling: false,
        time_from_ceiling: false,
    }
}

/// The limits of an ordinary query: a large result is possible and the time is 30 s.
pub fn wide() -> EffectiveLimits {
    limits(100_000, 1_000_000, 30)
}

pub fn ctx_for(subject: &str) -> QueryCtx {
    QueryCtx {
        kind: QueryKind::Panel,
        query_id: Uuid::now_v7(),
        log_comment: LogComment {
            kind: LogKind::Panel,
            uid: Some("nodes".to_owned()),
            folder: None,
            panel: Some(4),
            query_ref: None,
            variable: None,
            request_id: Some("r-5f2c".to_owned()),
            subject: Some(subject.to_owned()),
        },
        subject: SubjectId::new(subject),
        traceparent: None,
        queue_wait: QueueWait::new(),
    }
}

pub fn ctx() -> QueryCtx {
    ctx_for("user:alice")
}

pub async fn run(source: &PostgresSource, sql: &str, limits: EffectiveLimits) -> Result<Frame, SourceError> {
    source.query(PreparedQuery::PostgresSql { sql: sql.to_owned() }, limits, ctx()).await
}

/// The value of one cell as text: `null` for NULL.
pub fn cell(frame: &Frame, col: usize, row: usize) -> String {
    match frame.columns[col].value(row).expect("the cell exists") {
        FrameValue::Null => "null".to_owned(),
        FrameValue::Bool(b) => b.to_string(),
        FrameValue::Number(s) | FrameValue::Text(s) => s.to_owned(),
    }
}

/// An admin client of the test server. It is not a client of the source.
pub async fn admin() -> tokio_postgres::Client {
    let mut c = tokio_postgres::Config::new();
    c.host(env("HUNE_TEST_PGS_HOST"))
        .port(port())
        .user(env("HUNE_TEST_PGS_ADMIN_USER"))
        .password(env("HUNE_TEST_PGS_ADMIN_PASSWORD"))
        .dbname(env("HUNE_TEST_PGS_DATABASE"))
        .application_name("hune-test-admin");
    let (client, connection) = c.connect(tokio_postgres::NoTls).await.expect("the admin connects");
    tokio::spawn(async move { drop(connection.await) });
    client
}

/// The number of sessions of the server that have this `application_name`.
pub async fn sessions_named(admin: &tokio_postgres::Client, name: &str) -> i64 {
    admin
        .query_one("SELECT count(*) FROM pg_stat_activity WHERE application_name = $1", &[&name])
        .await
        .expect("pg_stat_activity")
        .get(0)
}

/// Waits until the number of sessions with this name is `want`. It gives the wait.
pub async fn wait_sessions(
    admin: &tokio_postgres::Client,
    name: &str,
    want: i64,
    within: Duration,
) -> Result<Duration, i64> {
    let start = Instant::now();
    loop {
        let n = sessions_named(admin, name).await;
        if n == want {
            return Ok(start.elapsed());
        }
        if start.elapsed() > within {
            return Err(n);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// The `application_name` of a panel query (8a.11).
pub fn app_name(query_id: &Uuid) -> String {
    format!("hune/panel/{query_id}")
}

/// One message of the frontend. `tag` is 0 for the first packet of a connection (the startup packet,
/// the cancel request, the SSL request): it has no type byte.
#[derive(Debug, Clone)]
pub struct Msg {
    pub conn: usize,
    pub tag: u8,
    pub body: Vec<u8>,
}

impl Msg {
    /// The text of a `Q` message, or the query of a `P` message.
    pub fn text(&self) -> String {
        match self.tag {
            b'Q' => cstr(&self.body).0,
            b'P' => {
                let (_name, rest) = cstr(&self.body);
                cstr(rest.as_slice()).0
            }
            _ => String::new(),
        }
    }
}

/// Reads one text that ends with a NUL byte. It gives the text and the rest.
fn cstr(bytes: &[u8]) -> (String, Vec<u8>) {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    (String::from_utf8_lossy(&bytes[..end]).into_owned(), bytes.get(end + 1..).unwrap_or(&[]).to_vec())
}

const CANCEL_CODE: u32 = 80_877_102;
const SSL_CODE: u32 = 80_877_103;

struct Shared {
    log: Mutex<Vec<Msg>>,
    pids: Mutex<Vec<i32>>,
    drop_cancel: AtomicBool,
    kill_next_simple: AtomicBool,
    conns: AtomicUsize,
}

/// A proxy on 127.0.0.1 in front of the test server.
pub struct Proxy {
    port: u16,
    upstream: SocketAddr,
    shared: Arc<Shared>,
    generation: watch::Sender<u64>,
    accept: Mutex<Option<JoinHandle<()>>>,
}

impl Proxy {
    /// Starts a proxy to the test server.
    pub async fn start() -> Proxy {
        let upstream: SocketAddr = format!("{}:{}", env("HUNE_TEST_PGS_HOST"), port()).parse().expect("address");
        let (generation, _) = watch::channel(0);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let proxy = Proxy {
            port,
            upstream,
            shared: Arc::new(Shared {
                log: Mutex::new(Vec::new()),
                pids: Mutex::new(Vec::new()),
                drop_cancel: AtomicBool::new(false),
                kill_next_simple: AtomicBool::new(false),
                conns: AtomicUsize::new(0),
            }),
            generation,
            accept: Mutex::new(None),
        };
        proxy.spawn_accept(listener);
        proxy
    }

    fn spawn_accept(&self, listener: TcpListener) {
        let shared = Arc::clone(&self.shared);
        let upstream = self.upstream;
        let generation = self.generation.clone();
        let handle = tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let conn = shared.conns.fetch_add(1, Ordering::SeqCst);
                let rx = generation.subscribe();
                tokio::spawn(handle(client, upstream, Arc::clone(&shared), conn, rx));
            }
        });
        *self.accept.lock().unwrap_or_else(PoisonError::into_inner) = Some(handle);
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Makes the proxy drop each cancel request (it closes the connection of the request).
    pub fn drop_cancels(&self, on: bool) {
        self.shared.drop_cancel.store(on, Ordering::SeqCst);
    }

    /// The next message of the simple protocol (the `BEGIN` of a query) ends its connection before the
    /// server sees it: the server went away between two queries.
    pub fn kill_next_simple_query(&self) {
        self.shared.kill_next_simple.store(true, Ordering::SeqCst);
    }

    /// The server goes away: the port refuses the connections and the open ones close.
    pub fn stop(&self) {
        if let Some(h) = self.accept.lock().unwrap_or_else(PoisonError::into_inner).take() {
            h.abort();
        }
        self.generation.send_modify(|g| *g += 1);
    }

    /// The server comes back on the same port.
    pub async fn restart(&self) {
        let start = Instant::now();
        let listener = loop {
            match TcpListener::bind(("127.0.0.1", self.port)).await {
                Ok(l) => break l,
                Err(e) if start.elapsed() < Duration::from_secs(10) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    drop(e);
                }
                Err(e) => panic!("the proxy cannot bind again: {e}"),
            }
        };
        self.spawn_accept(listener);
    }

    pub fn messages(&self) -> Vec<Msg> {
        self.shared.log.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// The text of each `Q` message (the simple protocol).
    pub fn simple_queries(&self) -> Vec<String> {
        self.messages().iter().filter(|m| m.tag == b'Q').map(Msg::text).collect()
    }

    /// The query of each `P` message (Parse).
    pub fn parses(&self) -> Vec<String> {
        self.messages().iter().filter(|m| m.tag == b'P').map(Msg::text).collect()
    }

    /// The number of `E` messages (Execute) of the frontend.
    pub fn executes(&self) -> usize {
        self.messages().iter().filter(|m| m.tag == b'E').count()
    }

    /// The number of cancel requests that arrived.
    pub fn cancels(&self) -> usize {
        self.messages().iter().filter(|m| m.tag == 0 && packet_code(&m.body) == Some(CANCEL_CODE)).count()
    }

    /// The number of connections that the proxy accepted.
    pub fn connections(&self) -> usize {
        self.shared.conns.load(Ordering::SeqCst)
    }

    /// The process ids of the backends that the proxy saw.
    pub fn pids(&self) -> Vec<i32> {
        self.shared.pids.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop();
    }
}

fn packet_code(body: &[u8]) -> Option<u32> {
    body.get(..4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

async fn read_packet(stream: &mut (impl AsyncReadExt + Unpin)) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await?;
    let len = u32::from_be_bytes(head) as usize;
    let mut body = vec![0u8; len.saturating_sub(4)];
    stream.read_exact(&mut body).await?;
    Ok((head.to_vec(), body))
}

/// Joins the type byte, the length and the body, to send them in one write.
fn frame_bytes(tag: u8, head: [u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + body.len());
    out.push(tag);
    out.extend_from_slice(&head);
    out.extend_from_slice(body);
    out
}

/// Reads one typed message: the type byte, the 4 length bytes and the body.
async fn read_typed(stream: &mut (impl AsyncReadExt + Unpin)) -> std::io::Result<(u8, [u8; 4], Vec<u8>)> {
    let mut tag = [0u8; 1];
    stream.read_exact(&mut tag).await?;
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await?;
    let len = u32::from_be_bytes(head) as usize;
    let mut body = vec![0u8; len.saturating_sub(4)];
    stream.read_exact(&mut body).await?;
    Ok((tag[0], head, body))
}

async fn handle(
    client: TcpStream,
    upstream: SocketAddr,
    shared: Arc<Shared>,
    conn: usize,
    mut stop: watch::Receiver<u64>,
) {
    let _ = pipe(client, upstream, &shared, conn, &mut stop).await;
}

async fn pipe(
    mut client: TcpStream,
    upstream: SocketAddr,
    shared: &Arc<Shared>,
    conn: usize,
    stop: &mut watch::Receiver<u64>,
) -> std::io::Result<()> {
    let (head, body) = read_packet(&mut client).await?;
    let code = packet_code(&body);
    shared.log.lock().unwrap_or_else(PoisonError::into_inner).push(Msg { conn, tag: 0, body: body.clone() });
    if code == Some(CANCEL_CODE) && shared.drop_cancel.load(Ordering::SeqCst) {
        return Ok(());
    }
    let mut server = TcpStream::connect(upstream).await?;
    // No Nagle delay: the proxy must not change the timing of the protocol.
    server.set_nodelay(true)?;
    client.set_nodelay(true)?;
    server.write_all(&head).await?;
    server.write_all(&body).await?;
    if code == Some(SSL_CODE) {
        // The tests that use the proxy do not use TLS. Copy the bytes if one does.
        let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
        return Ok(());
    }
    let (mut cr, mut cw) = client.into_split();
    let (mut sr, mut sw) = server.into_split();
    let to_client = async {
        loop {
            let (tag, head, body) = read_typed(&mut sr).await?;
            if tag == b'K' && body.len() >= 8 {
                let pid = i32::from_be_bytes([body[0], body[1], body[2], body[3]]);
                shared.pids.lock().unwrap_or_else(PoisonError::into_inner).push(pid);
            }
            cw.write_all(&frame_bytes(tag, head, &body)).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), std::io::Error>(())
    };
    let to_server = async {
        loop {
            let (tag, head, body) = read_typed(&mut cr).await?;
            shared.log.lock().unwrap_or_else(PoisonError::into_inner).push(Msg { conn, tag, body: body.clone() });
            if tag == b'Q' && shared.kill_next_simple.swap(false, Ordering::SeqCst) {
                return Ok(());
            }
            sw.write_all(&frame_bytes(tag, head, &body)).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), std::io::Error>(())
    };
    tokio::select! {
        r = to_client => r,
        r = to_server => r,
        _ = stop.changed() => Ok(()),
    }
}

/// A generator of numbers for the tests that need random input. It is not for security.
pub struct Xorshift(pub u64);

impl Xorshift {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}
