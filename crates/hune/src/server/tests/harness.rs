// SPDX-License-Identifier: MIT OR Apache-2.0
//! The test harness: a test configuration, a live server and a small HTTP/1.1 client.

use crate::config::Secret;
use crate::config::{
    AccessConfig, ClickHouseConfig, Config, HttpUrl, LogConfig, LogLevel, ServerConfig, StorageConfig, TelemetryConfig,
    VolumeConfig,
};
use crate::log::Logger;
use crate::server::lifecycle::{self, Running, Signals};
use crate::server::limits::Timing;
use crate::server::startup;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

/// The configuration of a test: ephemeral ports, a volume at `volume`, mode `open`.
pub fn test_config(volume: &Path) -> Config {
    Config {
        server: ServerConfig {
            listen: "127.0.0.1:0".parse().unwrap(),
            ops_listen: "127.0.0.1:0".parse().unwrap(),
            base_path: "/".to_owned(),
            editing: true,
            query_inspect: false,
            repeat_max_per_panel: 24,
            repeat_max_per_section: 12,
            repeat_max_per_dashboard: 120,
            compress_static: true,
            transform_max_cells: 10_000_000,
            max_connections: 64,
            idle_timeout: Duration::from_secs(30),
            shutdown_grace: Duration::from_secs(30),
            max_request_bytes: 8_388_608,
            max_result_memory_bytes: 536_870_912,
        },
        log: LogConfig { level: LogLevel::Debug },
        telemetry: TelemetryConfig::Disabled,
        storage: StorageConfig::Volume(VolumeConfig {
            path: volume.to_owned(),
            read_only: false,
            rescan_interval: Duration::from_millis(100),
            history_keep: 100,
        }),
        access: AccessConfig::Open,
        clickhouse: vec![ClickHouseConfig {
            id: "prod".to_owned(),
            label: "Production".to_owned(),
            url: HttpUrl {
                https: false,
                host: "127.0.0.1".to_owned(),
                port: 8123,
                text: "http://127.0.0.1:8123".to_owned(),
            },
            user: "hune".to_owned(),
            password: Secret::new("secret-password".to_owned()),
            database: "analytics".to_owned(),
            tls: None,
            connect_timeout: Duration::from_secs(5),
            max_execution_time: Duration::from_secs(60),
            max_result_rows: 100_000,
            max_result_bytes: 33_554_432,
            max_query_size: 1_048_576,
            max_concurrent: 16,
            queue_wait: Duration::from_secs(5),
            max_concurrent_per_subject: None,
            adhoc_columns: None,
        }],
        postgres_sources: Vec::new(),
        mongo_sources: Vec::new(),
        geomap: None,
    }
}

/// The timing of a test: a short drain.
pub fn test_timing() -> Timing {
    Timing {
        header_timeout: Duration::from_secs(10),
        body_timeout: Duration::from_secs(30),
        drain: Duration::from_millis(50),
        probe_sources: false,
        probe_interval: Duration::from_secs(30),
    }
}

/// A live server with its volume and its log.
pub struct Harness {
    pub running: Option<Running>,
    pub logger: Arc<Logger>,
    pub dir: tempfile::TempDir,
    pub addr: SocketAddr,
    pub ops: SocketAddr,
    pub signals_tx: mpsc::UnboundedSender<&'static str>,
    pub signals_rx: Option<Signals>,
}

impl Harness {
    /// Starts a server on an empty volume. `edit` changes the configuration.
    pub async fn start(edit: impl FnOnce(&mut Config)) -> Harness {
        Harness::start_with(test_timing(), |_| {}, edit).await
    }

    /// Starts a server with a timing, a function that fills the volume before the start, and a
    /// function that changes the configuration.
    pub async fn start_with(timing: Timing, fill: impl FnOnce(&Path), edit: impl FnOnce(&mut Config)) -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        fill(&root);
        let mut config = test_config(&root);
        edit(&mut config);
        let logger = Logger::memory(LogLevel::Debug);
        let running = lifecycle::start(Arc::new(config), logger.clone(), timing, startup::open_store)
            .await
            .expect("the server starts");
        let (addr, ops) = (running.listen_addr, running.ops_addr);
        let (signals_tx, signals_rx) = mpsc::unbounded_channel();
        Harness { running: Some(running), logger, dir, addr, ops, signals_tx, signals_rx: Some(signals_rx) }
    }

    /// The shared state.
    pub fn state(&self) -> Arc<crate::server::state::AppState> {
        self.running.as_ref().unwrap().state.clone()
    }

    /// Stops the server with the signal and gives the exit code.
    pub async fn stop(&mut self) -> u8 {
        let running = self.running.take().unwrap();
        let mut rx = self.signals_rx.take().unwrap();
        running.stop("SIGTERM", &mut rx).await
    }

    /// The names of the events in the log, in order.
    pub fn events(&self) -> Vec<String> {
        self.logger
            .memory_lines()
            .iter()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["event"].as_str().unwrap().to_owned())
            .collect()
    }
}

/// An HTTP answer that the test client read.
#[derive(Debug)]
pub struct Resp {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Resp {
    /// The first header with a name, compared without case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    /// The body as JSON.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| panic!("body is not JSON ({e}): {}", self.text()))
    }
    /// The code of an error body.
    pub fn code(&self) -> String {
        self.json()["error"]["code"].as_str().unwrap_or_default().to_owned()
    }
}

/// A client connection: it sends the text of requests and reads the answers.
pub struct Client {
    pub stream: TcpStream,
    buf: Vec<u8>,
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

impl Client {
    pub async fn connect(addr: SocketAddr) -> Client {
        Client { stream: TcpStream::connect(addr).await.unwrap(), buf: Vec::new() }
    }

    pub async fn send(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).await.unwrap();
    }

    /// Reads one answer. `head_only` is true for the answer to a `HEAD` request.
    pub async fn read(&mut self, head_only: bool) -> Resp {
        let timeout = Duration::from_secs(10);
        loop {
            if let Some(end) = find(&self.buf, b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&self.buf[..end]).into_owned();
                let mut lines = head.split("\r\n");
                let status: u16 = lines.next().unwrap().split(' ').nth(1).unwrap().parse().unwrap();
                let headers: Vec<(String, String)> = lines
                    .map(|l| {
                        let (k, v) = l.split_once(':').unwrap();
                        (k.trim().to_owned(), v.trim().to_owned())
                    })
                    .collect();
                let len: usize = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map_or(0, |(_, v)| v.parse().unwrap());
                let need = if head_only { 0 } else { len };
                while self.buf.len() < end + 4 + need {
                    self.fill(timeout).await.expect("the answer is complete");
                }
                let body = self.buf[end + 4..end + 4 + need].to_vec();
                self.buf.drain(..end + 4 + need);
                return Resp { status, headers, body };
            }
            self.fill(timeout).await.expect("an answer comes");
        }
    }

    async fn fill(&mut self, timeout: Duration) -> Option<usize> {
        let mut chunk = [0u8; 8192];
        match tokio::time::timeout(timeout, self.stream.read(&mut chunk)).await {
            Ok(Ok(n)) if n > 0 => {
                self.buf.extend_from_slice(&chunk[..n]);
                Some(n)
            }
            _ => None,
        }
    }

    /// Waits until the server closes the connection. It gives `true` when it closed within the time.
    pub async fn closed_within(&mut self, within: Duration) -> bool {
        let mut chunk = [0u8; 1024];
        match tokio::time::timeout(within, self.stream.read(&mut chunk)).await {
            Ok(Ok(0) | Err(_)) => true,
            Ok(Ok(_)) => Box::pin(self.closed_within(within)).await,
            Err(_) => false,
        }
    }
}

/// Makes the text of a request.
pub fn request(method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut out = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n");
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    if !body.is_empty() && !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-length")) {
        out.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

/// Sends one request on a new connection with `Connection: close` and reads the answer.
pub async fn call(addr: SocketAddr, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Resp {
    let mut c = Client::connect(addr).await;
    let mut h = headers.to_vec();
    h.push(("Connection", "close"));
    c.send(&request(method, path, &h, body)).await;
    c.read(method == "HEAD").await
}

/// A `GET` on a new connection.
pub async fn get(addr: SocketAddr, path: &str) -> Resp {
    call(addr, "GET", path, &[], b"").await
}

/// Starts a server whose source `prod` is a fake ClickHouse. The probe runs at once.
pub async fn start_with_fake(
    fake: &super::fake_ch::Fake,
    fill: impl FnOnce(&Path),
    edit: impl FnOnce(&mut Config),
) -> Harness {
    let addr = fake.addr;
    let timing = Timing { probe_sources: true, ..test_timing() };
    Harness::start_with(timing, fill, |c| {
        let block = &mut c.clickhouse[0];
        block.url.host = addr.ip().to_string();
        block.url.port = addr.port();
        block.url.text = format!("http://{addr}");
        block.connect_timeout = Duration::from_secs(2);
        edit(c);
    })
    .await
}

/// Waits until the source has the state, as `/readyz` shows it.
pub async fn wait_source_state(h: &Harness, state: &str) {
    for _ in 0..200 {
        let r = get(h.ops, "/readyz").await;
        if r.json()["sources"]["prod"] == state {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the source never got the state {state}");
}
