// SPDX-License-Identifier: MIT OR Apache-2.0
//! Helpers of the container tests of the MongoDB source (16.9b).
//!
//! The script `tests/ci/mongosource.sh` starts disposable MongoDB servers on 127.0.0.1 and sets the
//! variables `HUNE_TEST_MGS_*`. The tests that need a server are `#[ignore]`: a plain `cargo test` skips them.
//! Tests are not product code: they can read the environment and the file system.
//!
//! The module has the proxy of 16.9b. It sits between the client and the server, ends the TLS on both
//! sides with the test CA, records the name of each command that the client sends, and can close the
//! connections, delay the answers, or close a connection after a given command.
#![allow(dead_code)]

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use hune_core::frame::Frame;
use hune_core::ids::SourceId;
use hune_core::mongo::{
    MongoExpandRequest, MongoQuery, MongoValue, QueryBody, ReadPreference, prepare as prepare_query,
};
use hune_core::sql::{RequestValues, check_time_range, compute_interval, resolve_values};
use hune_source::{
    EffectiveLimits, LogComment, LogKind, MongoDriver, MongoHost, MongoSource, MongoSourceConfig, MongoTlsMode,
    MongoTopology, PreparedQuery, QueryCtx, QueryKind, QueueWait, ResultMemoryBudget, SourceError, SourceTls,
    SubjectId, Uuid,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_rustls::{TlsAcceptor, TlsConnector};

pub const REASON: &str = "needs a MongoDB container: run tests/ci/mongosource.sh";

/// The first instant of the data of `node_load` (2026-01-01T00:00:00Z).
pub const BASE_MS: i64 = 1_767_225_600_000;

pub fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set: {REASON}"))
}

pub fn port() -> u16 {
    env("HUNE_TEST_MGS_PORT").parse().expect("HUNE_TEST_MGS_PORT is a port")
}

pub fn port_of(name: &str) -> u16 {
    env(name).parse().unwrap_or_else(|_| panic!("{name} is a port"))
}

pub fn version() -> String {
    env("HUNE_TEST_MGS_VERSION")
}

pub fn host() -> String {
    env("HUNE_TEST_MGS_HOST")
}

pub fn database() -> String {
    env("HUNE_TEST_MGS_DATABASE")
}

pub fn ca_file() -> String {
    env("HUNE_TEST_MGS_CA_FILE")
}

/// The folder of the test CA, which holds `server.crt` and `server.key` too.
fn tls_dir() -> PathBuf {
    PathBuf::from(ca_file()).parent().expect("the CA file has a folder").to_path_buf()
}

/// The collections of the test blocks. `secret` is not in the list (7.33). The golden texts join `hosts` and
/// `events`, which the server does not hold: the probe then gives the advice A1.
pub fn collections() -> Vec<String> {
    ["node_load", "node_events", "types", "big", "orders", "fixture_marker", "hosts", "events"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

/// The settings of a block for the user `user`. `topology` is the server, or a proxy.
pub fn config_for(user: &str, password: &str, topology: MongoTopology, tls: bool) -> MongoSourceConfig {
    let ca = ca_file();
    MongoSourceConfig {
        id: SourceId::parse("mgs").expect("id"),
        label: "Test".to_owned(),
        topology,
        user: user.to_owned(),
        password: password.to_owned(),
        auth_database: env("HUNE_TEST_MGS_AUTH_DATABASE"),
        databases: vec![database()],
        collections: collections(),
        read_preference: ReadPreference::Primary,
        tls_mode: if tls { MongoTlsMode::VerifyFull } else { MongoTlsMode::Disable },
        tls: tls.then(|| SourceTls {
            ca_pem: std::fs::read(&ca).unwrap_or_else(|e| panic!("{ca}: {e}")),
            server_name: String::new(),
        }),
        tls_ca_file: tls.then(|| PathBuf::from(&ca)),
        connect_timeout: Duration::from_secs(5),
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 1_000_000,
        max_result_bytes: 1 << 30,
        max_query_size: 1_000_000,
        max_concurrent: 4,
        queue_wait: Duration::from_secs(15),
        max_concurrent_per_subject: None,
    }
}

pub fn direct(host: &str, port: u16) -> MongoTopology {
    MongoTopology::Direct { host: host.to_owned(), port }
}

/// The settings of the user `hune_ro` on the main server.
pub fn ro_config() -> MongoSourceConfig {
    config_for(&env("HUNE_TEST_MGS_RO_USER"), &env("HUNE_TEST_MGS_RO_PASSWORD"), direct(&host(), port()), true)
}

pub fn make_source(config: MongoSourceConfig) -> MongoSource {
    MongoSource::with_driver::<MongoDriver>(config, ResultMemoryBudget::new(1 << 32)).expect("the source builds")
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

/// The limits of an ordinary query.
pub fn wide() -> EffectiveLimits {
    limits(100_000, 100_000_000, 30)
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

/// The time range of the macro tests: 60 s of `node_load`, from 10 s after the base.
pub const FROM_MS: i64 = BASE_MS + 10_000;
pub const TO_MS: i64 = BASE_MS + 70_000;

/// Expands the text of a query as the engine does (7.35) for the range above.
pub fn prepared(text: &str) -> MongoQuery {
    prepared_range(text, FROM_MS, TO_MS)
}

/// The interval of 7.4 for a range and 1200 pixels.
pub fn interval_of(from_ms: i64, to_ms: i64) -> hune_core::sql::Interval {
    let range = check_time_range(from_ms, to_ms).expect("range");
    compute_interval(&range, 1200, 1).expect("interval")
}

/// Expands the text of a query for the range `from_ms` to `to_ms`.
pub fn prepared_range(text: &str, from_ms: i64, to_ms: i64) -> MongoQuery {
    try_prepared(text, from_ms, to_ms).unwrap_or_else(|e| panic!("the query is refused: {e:?}: {text}"))
}

/// Expands the text of a query, and gives the refusal of the engine when there is one.
pub fn try_prepared(text: &str, from_ms: i64, to_ms: i64) -> Result<MongoQuery, hune_core::mongo::MongoExpandError> {
    let collections = collections();
    let databases = vec![database()];
    let values = resolve_values(&[], &RequestValues::new()).expect("no value");
    let range = check_time_range(from_ms, to_ms).expect("range");
    let interval = compute_interval(&range, 1200, 1).expect("interval");
    prepare_query(
        &[text],
        &MongoExpandRequest {
            values: &values,
            range,
            interval: Some(interval),
            max_query_size: 8_388_608,
            collections: &collections,
            databases: &databases,
        },
    )
}

/// Expands the text of a query with the values of a request (7.35).
pub fn prepare_with(
    text: &str,
    values: &hune_core::sql::ResolvedValues,
) -> Result<MongoQuery, hune_core::mongo::MongoExpandError> {
    let collections = collections();
    let databases = vec![database()];
    let range = check_time_range(FROM_MS, TO_MS).expect("range");
    let interval = compute_interval(&range, 1200, 1).expect("interval");
    prepare_query(
        &[text],
        &MongoExpandRequest {
            values,
            range,
            interval: Some(interval),
            max_query_size: 262_144,
            collections: &collections,
            databases: &databases,
        },
    )
}

/// The aggregate query of a collection with the stages of a JSON array text.
pub fn pipeline(collection: &str, stages: &str) -> String {
    format!("{{\"collection\":\"{collection}\",\"aggregate\":{stages}}}")
}

pub async fn run(source: &MongoSource, text: &str, limits: EffectiveLimits) -> Result<Frame, SourceError> {
    source.query(PreparedQuery::MongoQuery { command: prepared(text) }, limits, ctx()).await
}

pub async fn run_with(
    source: &MongoSource,
    text: &str,
    limits: EffectiveLimits,
    ctx: QueryCtx,
) -> Result<Frame, SourceError> {
    source.query(PreparedQuery::MongoQuery { command: prepared(text) }, limits, ctx).await
}

/// The JSON of a frame (7.16).
pub fn json(frame: &Frame) -> serde_json::Value {
    let mut text = String::new();
    frame.write_json(&mut text);
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("the frame is not JSON: {e}: {text}"))
}

/// A cell of a frame as JSON: `frame.values[column][row]`.
pub fn cell(frame: &Frame, column: &str, row: usize) -> serde_json::Value {
    let doc = json(frame);
    let index = doc["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .position(|c| c["name"] == column)
        .unwrap_or_else(|| panic!("no column {column}: {doc}"));
    doc["values"][index][row].clone()
}

/// The type text of a column.
pub fn column_type(frame: &Frame, column: &str) -> String {
    let doc = json(frame);
    doc["columns"]
        .as_array()
        .expect("columns")
        .iter()
        .find(|c| c["name"] == column)
        .unwrap_or_else(|| panic!("no column {column}: {doc}"))["type"]
        .as_str()
        .expect("type text")
        .to_owned()
}

pub fn find_query(collection: &str, filter: &str) -> String {
    format!("{{\"collection\":\"{collection}\",\"find\":{filter}}}")
}

/// Gives a value of the model from Extended JSON text.
pub fn value(text: &str) -> MongoValue {
    MongoValue::from_extended_json(text).unwrap_or_else(|e| panic!("{e:?}: {text}"))
}

pub fn body_is_aggregate(q: &MongoQuery) -> bool {
    matches!(q.body, QueryBody::Aggregate(_))
}

// ---- replica set ----

pub fn rs_topology(hosts: &[(&str, u16)], name: &str) -> MongoTopology {
    MongoTopology::ReplicaSet {
        hosts: hosts.iter().map(|(h, p)| MongoHost { host: (*h).to_owned(), port: *p }).collect(),
        replica_set: name.to_owned(),
    }
}

// ---- mongosh in the container ----

/// Runs a JavaScript text with `mongosh` in a container, logged in as `user`. The script sees `db`, the
/// database `admin` of the server at `port`. The password goes in the environment of the process. The
/// output is the standard output of `mongosh`.
pub fn mongosh(container: &str, port: u16, user: &str, password: &str, auth_db: &str, script: &str) -> String {
    let docker = env("HUNE_TEST_MGS_DOCKER");
    let program = format!(
        "const conn = new Mongo('mongodb://localhost:{port}/?tls=true&tlsCAFile=%2Ftls%2Fca.pem&directConnection=true'); \
         const admin = conn.getDB('{auth_db}'); admin.auth(process.env.MGS_U, process.env.MGS_P); \
         function db_(n) {{ return conn.getDB(n); }} \
         {script}"
    );
    let out = Command::new(docker)
        .args(["exec", "-i", "-e", &format!("MGS_U={user}"), "-e", &format!("MGS_P={password}")])
        .args([container, "mongosh", "--quiet", "--norc", "--nodb", "--eval", &program])
        .output()
        .expect("docker runs");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    text
}

/// Runs a script with `mongosh` in a container, with a connection string (a plain client: no option of the
/// driver of Hune). The script sees `conn` and `admin` (the database of the login).
pub fn mongosh_uri(container: &str, uri: &str, user: &str, password: &str, auth_db: &str, script: &str) -> String {
    let docker = env("HUNE_TEST_MGS_DOCKER");
    let program = format!(
        "const conn = new Mongo('{uri}'); const admin = conn.getDB('{auth_db}'); \
         admin.auth(process.env.MGS_U, process.env.MGS_P); {script}"
    );
    let out = Command::new(docker)
        .args(["exec", "-i", "-e", &format!("MGS_U={user}"), "-e", &format!("MGS_P={password}")])
        .args([container, "mongosh", "--quiet", "--norc", "--nodb", "--eval", &program])
        .output()
        .expect("docker runs");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    text
}

/// Runs a script as the root user on the main server.
pub fn root_shell(script: &str) -> String {
    mongosh(
        &env("HUNE_TEST_MGS_CONTAINER"),
        port(),
        &env("HUNE_TEST_MGS_ADMIN_USER"),
        &env("HUNE_TEST_MGS_ADMIN_PASSWORD"),
        "admin",
        script,
    )
}

/// The number of operations of the server that hold the query id in their `comment` (`currentOp`).
pub fn current_ops_for(query_id: &str) -> usize {
    let out = root_shell(&format!(
        "const r = admin.aggregate([{{$currentOp: {{allUsers: true, idleCursors: true}}}}, {{$match: {{$or: [ \
         {{'command.comment.query_id': '{query_id}'}}, {{'cursor.originatingCommand.comment.query_id': '{query_id}'}} ]}}}}]).toArray(); \
         print('COUNT=' + r.length);"
    ));
    out.lines()
        .find_map(|l| l.strip_prefix("COUNT="))
        .unwrap_or_else(|| panic!("currentOp gave no count: {out}"))
        .trim()
        .parse()
        .expect("a count")
}

/// Waits until no operation holds the query id, for at most `within`. It gives the wait.
pub fn wait_no_op(query_id: &str, within: Duration) -> Result<Duration, String> {
    let start = Instant::now();
    loop {
        if current_ops_for(query_id) == 0 {
            return Ok(start.elapsed());
        }
        if start.elapsed() > within {
            return Err(format!("the operation of {query_id} is still in currentOp after {within:?}"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The time of the clock of the host, in milliseconds since the Unix epoch. `OpWatch` and the tests use it as
/// the one clock: `mongosh` runs on the same host.
pub fn epoch_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("the clock is after 1970").as_millis()
}

/// A `mongosh` process that polls `currentOp` every 50 ms for one query id, from the moment of its start. It
/// lets a test measure when an operation leaves `currentOp`, with no start of `mongosh` inside the measure.
/// A start of `mongosh` takes more than 2 s on a slow host, and the cases of MS05 allow 2 s.
pub struct OpWatch {
    child: Child,
    lines: mpsc::Receiver<String>,
}

/// Starts the watch and waits until it is logged in. Start it before the query: the watch then sees the
/// operation from its first moment.
pub fn watch_ops(query_id: &str) -> OpWatch {
    let docker = env("HUNE_TEST_MGS_DOCKER");
    let script = format!(
        "const conn = new Mongo('mongodb://localhost:{port}/?tls=true&tlsCAFile=%2Ftls%2Fca.pem&directConnection=true'); \
         const admin = conn.getDB('admin'); admin.auth(process.env.MGS_U, process.env.MGS_P); \
         print('READY'); let seen = false; let last = -1; const t0 = Date.now(); \
         while (Date.now() - t0 < 120000) {{ \
           const n = admin.aggregate([{{$currentOp: {{allUsers: true, idleCursors: true}}}}, {{$match: {{$or: [ \
             {{'command.comment.query_id': '{query_id}'}}, {{'cursor.originatingCommand.comment.query_id': '{query_id}'}} ]}}}}]).toArray().length; \
           if (n > 0) seen = true; \
           if (seen && n !== last) {{ print('OPS ' + Date.now() + ' ' + n); last = n; }} \
           if (seen && n === 0) break; \
           sleep(50); }}",
        port = port()
    );
    let mut child = Command::new(docker)
        .args(["exec", "-i", "-e", &format!("MGS_U={}", env("HUNE_TEST_MGS_ADMIN_USER"))])
        .args(["-e", &format!("MGS_P={}", env("HUNE_TEST_MGS_ADMIN_PASSWORD"))])
        .args([&env("HUNE_TEST_MGS_CONTAINER"), "mongosh", "--quiet", "--norc", "--nodb", "--eval", &script])
        .stdout(Stdio::piped())
        .spawn()
        .expect("docker runs");
    let out = child.stdout.take().expect("a pipe");
    let (tx, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let watch = OpWatch { child, lines };
    let first = watch.line(Duration::from_secs(60)).expect("the watch of currentOp starts");
    assert_eq!(first, "READY", "the watch of currentOp gave: {first}");
    watch
}

impl OpWatch {
    fn line(&self, within: Duration) -> Result<String, String> {
        self.lines.recv_timeout(within).map_err(|_| format!("the watch of currentOp gave no line in {within:?}"))
    }

    /// Reads lines until one has the form `OPS <time> <count>`. It gives the time and the count.
    fn change(&self, within: Duration) -> Result<(u128, usize), String> {
        let start = Instant::now();
        loop {
            let left = within.checked_sub(start.elapsed()).ok_or("the watch of currentOp saw no change")?;
            let line = self.line(left)?;
            let mut part = line.split_whitespace();
            if part.next() == Some("OPS") {
                let time = part.next().and_then(|t| t.parse().ok()).ok_or_else(|| format!("bad line: {line}"))?;
                let count = part.next().and_then(|t| t.parse().ok()).ok_or_else(|| format!("bad line: {line}"))?;
                return Ok((time, count));
            }
        }
    }

    /// Waits until the watch sees the operation in `currentOp`.
    pub fn wait_seen(&self, within: Duration) -> Result<(), String> {
        match self.change(within)? {
            (_, 0) => Err("the operation left currentOp before the test measured".to_owned()),
            _ => Ok(()),
        }
    }

    /// Waits until the operation leaves `currentOp`. It gives the time from `from` (a value of `epoch_ms`) to
    /// the first poll with no operation. The first poll after the event is at most 50 ms late.
    pub fn wait_gone(&self, from: u128, within: Duration) -> Result<Duration, String> {
        loop {
            if let (time, 0) = self.change(within)? {
                return Ok(Duration::from_millis(time.saturating_sub(from) as u64));
            }
        }
    }
}

impl Drop for OpWatch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---- the proxy ----

/// The names of the commands that 8b.5 allows. The monitor of the driver sends `hello` or `isMaster`.
pub const ALLOWED_COMMANDS: [&str; 13] = [
    "hello",
    "isMaster",
    "ismaster",
    "saslStart",
    "saslContinue",
    "buildInfo",
    "connectionStatus",
    "listCollections",
    "aggregate",
    "find",
    "getMore",
    "killCursors",
    "endSessions",
];

#[derive(Default)]
struct ProxyState {
    commands: Mutex<Vec<String>>,
    /// The raw BSON document of each `hello` (or legacy `isMaster`) that a client sent.
    hellos: Mutex<Vec<Vec<u8>>>,
    connections: AtomicUsize,
    /// The name of a command after which the proxy closes the connection, once.
    close_after: Mutex<Option<String>>,
    /// The name of a command that the proxy does not forward: it closes the connection instead, once.
    close_before: Mutex<Option<String>>,
    /// The delay of each message of the server, in milliseconds.
    reply_delay_ms: AtomicU64,
    stopped: AtomicBool,
}

/// What a connection carries: the monitor of the driver sends `hello` only.
fn is_monitor_command(name: &str) -> bool {
    matches!(name, "hello" | "isMaster" | "ismaster")
}

/// A TLS proxy in front of a MongoDB server. It ends the TLS with the test certificate and opens a new TLS
/// connection to the server with the test CA.
pub struct Proxy {
    pub addr: SocketAddr,
    state: Arc<ProxyState>,
    epoch: watch::Sender<u64>,
    pool_epoch: watch::Sender<u64>,
}

impl Proxy {
    /// Starts a proxy to `localhost:upstream_port`. The client side has TLS with the test certificate.
    pub async fn start(upstream_port: u16) -> Proxy {
        Proxy::start_with(upstream_port, false).await
    }

    /// Starts a proxy whose client side has no TLS (the mode `disable`) and whose server side has TLS.
    pub async fn start_plain(upstream_port: u16) -> Proxy {
        Proxy::start_with(upstream_port, true).await
    }

    async fn start_with(upstream_port: u16, plain_client: bool) -> Proxy {
        let dir = tls_dir();
        let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(dir.join("server.crt"))
            .expect("server.crt")
            .map(|c| c.expect("certificate"))
            .collect();
        let key = PrivateKeyDer::from_pem_file(dir.join("server.key")).expect("server.key");
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server_cfg = rustls::ServerConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("certificate and key");
        let acceptor = TlsAcceptor::from(Arc::new(server_cfg));
        let mut roots = rustls::RootCertStore::empty();
        for c in CertificateDer::pem_file_iter(ca_file()).expect("ca.pem") {
            roots.add(c.expect("CA certificate")).expect("root");
        }
        let client_cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(client_cfg));

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let state = Arc::new(ProxyState::default());
        let (epoch, epoch_rx) = watch::channel(0_u64);
        let (pool_epoch, pool_epoch_rx) = watch::channel(0_u64);
        let st = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                let Ok((client, _)) = listener.accept().await else { return };
                if st.stopped.load(Ordering::Acquire) {
                    drop(client);
                    continue;
                }
                st.connections.fetch_add(1, Ordering::AcqRel);
                tokio::spawn(connection(
                    client,
                    plain_client,
                    acceptor.clone(),
                    connector.clone(),
                    upstream_port,
                    Arc::clone(&st),
                    epoch_rx.clone(),
                    pool_epoch_rx.clone(),
                ));
            }
        });
        Proxy { addr, state, epoch, pool_epoch }
    }

    pub fn host(&self) -> String {
        "localhost".to_owned()
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    pub fn topology(&self) -> MongoTopology {
        direct("localhost", self.port())
    }

    /// The names of the commands that the clients sent, in order.
    pub fn commands(&self) -> Vec<String> {
        self.state.commands.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn clear(&self) {
        self.state.commands.lock().unwrap_or_else(PoisonError::into_inner).clear();
        self.state.hellos.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }

    /// The command documents (raw BSON) of the `hello` messages that the clients sent, in order.
    /// The document of a `hello` holds the metadata of the driver in its key `client`.
    pub fn hello_documents(&self) -> Vec<Vec<u8>> {
        self.state.hellos.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// The number of the commands with this name.
    pub fn count(&self, name: &str) -> usize {
        self.commands().iter().filter(|c| c.as_str() == name).count()
    }

    /// The number of the client connections that the proxy accepted.
    pub fn connections(&self) -> usize {
        self.state.connections.load(Ordering::Acquire)
    }

    /// Closes each open connection now (a server or a middlebox that closes idle links).
    pub fn close_all(&self) {
        self.epoch.send_modify(|e| *e += 1);
    }

    /// Closes the connections that carried a command other than `hello`: the connections of the pool. The
    /// monitor of the driver stays, so the driver does not see the close (an idle link that a middlebox closes).
    pub fn close_pool_connections(&self) {
        self.pool_epoch.send_modify(|e| *e += 1);
    }

    /// After the next command with this name, closes the connection (once).
    pub fn close_after(&self, name: &str) {
        *self.state.close_after.lock().unwrap_or_else(PoisonError::into_inner) = Some(name.to_owned());
    }

    pub fn delay_replies(&self, delay: Duration) {
        self.state.reply_delay_ms.store(u64::try_from(delay.as_millis()).unwrap_or(u64::MAX), Ordering::Release);
    }

    /// Stops the proxy from accepting: a new connection closes at once.
    pub fn refuse_new(&self, on: bool) {
        self.state.stopped.store(on, Ordering::Release);
    }

    /// Fails when a client sent a command that 8b.5 does not allow.
    pub fn assert_only_closed_commands(&self) {
        for c in self.commands() {
            assert!(ALLOWED_COMMANDS.contains(&c.as_str()), "the proxy saw the command {c}");
        }
    }
}

trait Duplex: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> Duplex for T {}

#[allow(clippy::too_many_arguments)]
async fn connection(
    client: TcpStream,
    plain_client: bool,
    acceptor: TlsAcceptor,
    connector: TlsConnector,
    upstream_port: u16,
    state: Arc<ProxyState>,
    mut epoch: watch::Receiver<u64>,
    mut pool_epoch: watch::Receiver<u64>,
) {
    let _ = client.set_nodelay(true);
    let client: Box<dyn Duplex> = if plain_client {
        Box::new(client)
    } else {
        let Ok(client) = acceptor.accept(client).await else { return };
        Box::new(client)
    };
    let Ok(server) = TcpStream::connect(("127.0.0.1", upstream_port)).await else { return };
    let _ = server.set_nodelay(true);
    let Ok(name) = ServerName::try_from("localhost") else { return };
    let Ok(server) = connector.connect(name, server).await else { return };
    let (mut client_read, mut client_write) = tokio::io::split(client);
    let (mut server_read, mut server_write) = tokio::io::split(server);
    let up_state = Arc::clone(&state);
    let carried = Arc::new(AtomicBool::new(false));
    let carried_up = Arc::clone(&carried);
    let up = async move {
        // client to server: one message at a time, to read the name of the command.
        loop {
            let mut header = [0_u8; 16];
            if client_read.read_exact(&mut header).await.is_err() {
                return;
            }
            let len = i32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            let opcode = i32::from_le_bytes([header[12], header[13], header[14], header[15]]);
            let Ok(total) = usize::try_from(len) else { return };
            if !(16..=48_000_000 + 16_384).contains(&total) {
                return;
            }
            let mut body = vec![0_u8; total - 16];
            if client_read.read_exact(&mut body).await.is_err() {
                return;
            }
            let name = command_name(opcode, &body);
            if let Some(n) = &name {
                if !is_monitor_command(n) {
                    carried_up.store(true, Ordering::Release);
                } else if let Some(doc) = command_document(opcode, &body) {
                    up_state.hellos.lock().unwrap_or_else(PoisonError::into_inner).push(doc.to_vec());
                }
                up_state.commands.lock().unwrap_or_else(PoisonError::into_inner).push(n.clone());
                let mut before = up_state.close_before.lock().unwrap_or_else(PoisonError::into_inner);
                if before.as_deref() == Some(n.as_str()) {
                    *before = None;
                    return;
                }
            }
            if server_write.write_all(&header).await.is_err() || server_write.write_all(&body).await.is_err() {
                return;
            }
            let _ = server_write.flush().await;
            if let Some(n) = name {
                let mut after = up_state.close_after.lock().unwrap_or_else(PoisonError::into_inner);
                if after.as_deref() == Some(n.as_str()) {
                    *after = None;
                    return;
                }
            }
        }
    };
    let down_state = Arc::clone(&state);
    let down = async move {
        let mut buf = vec![0_u8; 65_536];
        loop {
            let n = match server_read.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            let delay = down_state.reply_delay_ms.load(Ordering::Acquire);
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            if client_write.write_all(&buf[..n]).await.is_err() {
                return;
            }
            let _ = client_write.flush().await;
        }
    };
    let pool_close = async move {
        loop {
            if pool_epoch.changed().await.is_err() {
                return;
            }
            if carried.load(Ordering::Acquire) {
                return;
            }
        }
    };
    tokio::select! {
        () = up => {}
        () = down => {}
        () = pool_close => {}
        _ = epoch.changed() => {}
    }
}

/// The command document of a message of the client, as raw BSON: the section of the kind 0 of an OP_MSG, or the
/// query document of an OP_QUERY (the legacy form that the first `hello` of a driver can take).
pub fn command_document(opcode: i32, body: &[u8]) -> Option<&[u8]> {
    match opcode {
        // OP_MSG: flags (4), then sections. The section of the kind 0 holds the command.
        2013 => {
            let mut at = 4;
            loop {
                let kind = *body.get(at)?;
                at += 1;
                if kind == 0 {
                    break Some(&body[at..]);
                }
                // A section of the kind 1 has a size that includes its own 4 bytes.
                let size = i32::from_le_bytes(body.get(at..at + 4)?.try_into().ok()?);
                at += usize::try_from(size).ok()?;
            }
        }
        // OP_QUERY: flags (4), a cstring, skip (4), return (4), then the document.
        2004 => {
            let end = body.get(4..)?.iter().position(|b| *b == 0)? + 4;
            body.get(end + 1 + 8..)
        }
        _ => None,
    }
}

/// The name of the command of a message of the client: the first key of the command document.
pub fn command_name(opcode: i32, body: &[u8]) -> Option<String> {
    let doc = command_document(opcode, body)?;
    // The first element: type (1), key as a cstring.
    let key = doc.get(5..)?;
    let end = key.iter().position(|b| *b == 0)?;
    Some(String::from_utf8_lossy(&key[..end]).into_owned())
}

// ---- docker ----

/// The id of the container of a compose service of the run.
pub fn container_of(service: &str) -> String {
    let docker = env("HUNE_TEST_MGS_DOCKER");
    let project = env("HUNE_TEST_MGS_COMPOSE_PROJECT");
    let out = Command::new(docker)
        .args(["ps", "-aq", "--filter"])
        .arg(format!("label=com.docker.compose.project={project}"))
        .arg("--filter")
        .arg(format!("label=com.docker.compose.service={service}"))
        .output()
        .expect("docker runs");
    String::from_utf8_lossy(&out.stdout).lines().next().expect("the container of the service").trim().to_owned()
}

pub fn docker(args: &[&str]) -> bool {
    Command::new(env("HUNE_TEST_MGS_DOCKER")).args(args).status().expect("docker runs").success()
}

/// A small pseudo-random generator for the tests (no `rand` crate). The sequence is the same for a seed.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}
