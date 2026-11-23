//! A fake ClickHouse HTTP server for the tests: it speaks enough HTTP/1.1 to test the client.
//! Tests are not product code: they can use trait objects and the file system.
#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hune_source::{
    ClickHouseSource, ClickHouseSourceConfig, EffectiveLimits, LogComment, LogKind, PreparedQuery, QueryCtx, QueryKind,
    QueueWait, ResultMemoryBudget, SubjectId, Uuid,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;

/// One request that the server received.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The index of the connection (from 0) and the number of the request on it (from 0).
    pub conn: usize,
    pub nth: usize,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    /// The URL parameters, decoded.
    pub fn params(&self) -> Vec<(String, String)> {
        let q = self.target.split_once('?').map_or("", |x| x.1);
        q.split('&')
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (k, v) = p.split_once('=').unwrap_or((p, ""));
                (k.to_owned(), percent_decode(v))
            })
            .collect()
    }

    pub fn param(&self, name: &str) -> Option<String> {
        self.params().into_iter().find(|(k, _)| k == name).map(|x| x.1)
    }

    pub fn sql(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn query_id(&self) -> String {
        self.param("query_id").unwrap_or_default()
    }
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            let h = std::str::from_utf8(&b[i + 1..i + 3]).unwrap();
            out.push(u8::from_str_radix(h, 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

/// One action of a scripted answer.
#[derive(Debug, Clone)]
pub enum Step {
    /// Write these bytes.
    Send(Vec<u8>),
    /// Wait. A close of the client during the wait is recorded.
    Sleep(Duration),
    /// Wait until the client closes the connection.
    Hang,
    /// Write these bytes again and again until a write fails (the client closed).
    Endless(Vec<u8>),
    /// Close the connection now.
    Close,
}

/// What the server counted.
#[derive(Debug, Default)]
pub struct Counters {
    pub connections: AtomicUsize,
    pub requests: AtomicUsize,
    /// The client closed while the server waited in `Sleep` or `Hang`.
    pub client_closed_during_wait: AtomicUsize,
    /// A write of `Endless` failed: the client closed.
    pub endless_write_failed: AtomicUsize,
}

pub type Handler = Arc<dyn Fn(&Recorded) -> Vec<Step> + Send + Sync>;

pub struct FakeServer {
    pub addr: SocketAddr,
    pub counters: Arc<Counters>,
    pub requests: Arc<Mutex<Vec<Recorded>>>,
    /// The SNI name of each TLS connection.
    pub snis: Arc<Mutex<Vec<Option<String>>>>,
    task: JoinHandle<()>,
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl FakeServer {
    pub async fn start(handler: Handler) -> FakeServer {
        Self::start_with(handler, None).await
    }

    pub async fn start_tls(handler: Handler, acceptor: TlsAcceptor) -> FakeServer {
        Self::start_with(handler, Some(acceptor)).await
    }

    async fn start_with(handler: Handler, tls: Option<TlsAcceptor>) -> FakeServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let counters = Arc::new(Counters::default());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let snis = Arc::new(Mutex::new(Vec::new()));
        let (c2, r2, n2) = (Arc::clone(&counters), Arc::clone(&requests), Arc::clone(&snis));
        let task = tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else { return };
                let idx = c2.connections.fetch_add(1, Ordering::SeqCst);
                let (h, c, r, tls, n) =
                    (Arc::clone(&handler), Arc::clone(&c2), Arc::clone(&r2), tls.clone(), Arc::clone(&n2));
                tokio::spawn(async move {
                    match tls {
                        Some(acceptor) => {
                            if let Ok(s) = acceptor.accept(sock).await {
                                n.lock().unwrap().push(s.get_ref().1.server_name().map(str::to_owned));
                                serve(s, idx, h, c, r).await;
                            }
                        }
                        None => serve(sock, idx, h, c, r).await,
                    }
                });
            }
        });
        FakeServer { addr, counters, requests, snis, task }
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    pub fn request(&self, i: usize) -> Recorded {
        self.requests.lock().unwrap()[i].clone()
    }

    pub fn connections(&self) -> usize {
        self.counters.connections.load(Ordering::SeqCst)
    }
}

async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
    mut s: S,
    conn: usize,
    handler: Handler,
    counters: Arc<Counters>,
    requests: Arc<Mutex<Vec<Recorded>>>,
) {
    let mut buf: Vec<u8> = Vec::new();
    let mut nth = 0;
    loop {
        // Read the head.
        let head_end = loop {
            if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break p;
            }
            let mut tmp = [0u8; 8192];
            match s.read(&mut tmp).await {
                Ok(0) | Err(_) => return,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
            }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
        let mut lines = head.split("\r\n");
        let first = lines.next().unwrap_or("");
        let mut parts = first.split(' ');
        let method = parts.next().unwrap_or("").to_owned();
        let target = parts.next().unwrap_or("").to_owned();
        let headers: Vec<(String, String)> =
            lines.filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))).collect();
        let len: usize = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        buf.drain(..head_end + 4);
        while buf.len() < len {
            let mut tmp = [0u8; 65536];
            match s.read(&mut tmp).await {
                Ok(0) | Err(_) => return,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
            }
        }
        let body: Vec<u8> = buf.drain(..len).collect();
        let rec = Recorded { method, target, headers, body, conn, nth };
        nth += 1;
        counters.requests.fetch_add(1, Ordering::SeqCst);
        requests.lock().unwrap().push(rec.clone());
        for step in handler(&rec) {
            match step {
                Step::Send(b) => {
                    if s.write_all(&b).await.is_err() {
                        return;
                    }
                    let _ = s.flush().await;
                }
                Step::Sleep(d) => {
                    let mut tmp = [0u8; 64];
                    tokio::select! {
                        _ = tokio::time::sleep(d) => {}
                        r = s.read(&mut tmp) => {
                            if matches!(r, Ok(0) | Err(_)) {
                                counters.client_closed_during_wait.fetch_add(1, Ordering::SeqCst);
                                return;
                            }
                        }
                    }
                }
                Step::Hang => {
                    let mut tmp = [0u8; 64];
                    loop {
                        match s.read(&mut tmp).await {
                            Ok(0) | Err(_) => {
                                counters.client_closed_during_wait.fetch_add(1, Ordering::SeqCst);
                                return;
                            }
                            Ok(_) => {}
                        }
                    }
                }
                Step::Endless(b) => loop {
                    if s.write_all(&b).await.is_err() {
                        counters.endless_write_failed.fetch_add(1, Ordering::SeqCst);
                        return;
                    }
                    tokio::task::yield_now().await;
                },
                Step::Close => return,
            }
        }
    }
}

/// The text of a chunked body from the pieces.
pub fn chunked(pieces: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in pieces {
        out.extend_from_slice(format!("{:x}\r\n", p.len()).as_bytes());
        out.extend_from_slice(p);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"0\r\n\r\n");
    out
}

/// The head of an answer. `extra` are more header lines.
pub fn head(status: u16, extra: &[(&str, String)]) -> Vec<u8> {
    let mut h = format!("HTTP/1.1 {status} X\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\n");
    for (k, v) in extra {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    h.into_bytes()
}

/// A complete 200 answer with the right query id.
pub fn ok_steps(rec: &Recorded, body: &str) -> Vec<Step> {
    ok_steps_with(rec, &[body.as_bytes()], &[])
}

pub fn ok_steps_with(rec: &Recorded, pieces: &[&[u8]], extra: &[(&str, String)]) -> Vec<Step> {
    let mut hdr = vec![("X-ClickHouse-Query-Id", rec.query_id())];
    hdr.extend(extra.iter().map(|(k, v)| (*k, v.clone())));
    vec![Step::Send(head(200, &hdr)), Step::Send(chunked(pieces))]
}

pub fn error_steps(rec: &Recorded, status: u16, code: i64, text: &str) -> Vec<Step> {
    vec![
        Step::Send(head(
            status,
            &[("X-ClickHouse-Query-Id", rec.query_id()), ("X-ClickHouse-Exception-Code", code.to_string())],
        )),
        Step::Send(chunked(&[text.as_bytes()])),
    ]
}

pub const ONE_COLUMN: &str = "[\"n\"]\n[\"UInt32\"]\n[1]\n[2]\n[3]\n";

pub fn handler(f: impl Fn(&Recorded) -> Vec<Step> + Send + Sync + 'static) -> Handler {
    Arc::new(f)
}

pub fn config(addr: SocketAddr) -> ClickHouseSourceConfig {
    ClickHouseSourceConfig {
        id: hune_core::ids::SourceId::parse("test").unwrap(),
        label: "Test".into(),
        https: false,
        host: addr.ip().to_string(),
        port: addr.port(),
        user: "hune".into(),
        password: "p4ss-secret".into(),
        database: "analytics".into(),
        tls: None,
        connect_timeout: Duration::from_secs(2),
        max_execution_time: Duration::from_secs(60),
        max_result_rows: 100_000,
        max_result_bytes: 1 << 20,
        max_query_size: 1 << 20,
        max_concurrent: 4,
        queue_wait: Duration::from_millis(200),
        max_concurrent_per_subject: None,
    }
}

pub fn source(cfg: ClickHouseSourceConfig) -> ClickHouseSource {
    ClickHouseSource::new(cfg, ResultMemoryBudget::new(1 << 30)).unwrap()
}

/// A source that is `up` without a probe: the tests that do not test the probe call this.
pub async fn up_source(cfg: ClickHouseSourceConfig, server: &FakeServer) -> ClickHouseSource {
    let _ = server;
    let s = source(cfg);
    s.probe().await;
    s
}

pub fn limits() -> EffectiveLimits {
    EffectiveLimits {
        max_execution_time: Duration::from_secs(60),
        max_result_rows: 1000,
        max_result_bytes: 1 << 20,
        max_query_size: 1 << 20,
        rows_from_ceiling: false,
        time_from_ceiling: false,
    }
}

pub fn ctx() -> QueryCtx {
    ctx_for("-")
}

pub fn ctx_for(subject: &str) -> QueryCtx {
    QueryCtx {
        kind: QueryKind::Panel,
        query_id: Uuid::now_v7(),
        log_comment: LogComment {
            kind: LogKind::Panel,
            uid: Some("nodes".into()),
            folder: None,
            panel: Some(4),
            query_ref: None,
            variable: None,
            request_id: Some("r-5f2c".into()),
            subject: Some(subject.into()),
        },
        subject: SubjectId::new(subject),
        traceparent: None,
        queue_wait: QueueWait::new(),
    }
}

pub fn sql(text: &str) -> PreparedQuery {
    PreparedQuery::ClickHouseSql { sql: text.to_owned(), params: Default::default() }
}

/// The steps of the answers to the three probe statements, in a healthy account.
pub fn probe_answer(rec: &Recorded, version: &str, readonly: &str, grants: &[&str]) -> Option<Vec<Step>> {
    let s = rec.sql();
    if s.starts_with("SELECT version()") {
        let body = format!(
            "[\"version()\",\"getSetting('readonly')\"]\n[\"String\",\"UInt64\"]\n[\"{version}\",\"{readonly}\"]\n"
        );
        Some(ok_steps(rec, &body))
    } else if s == "SHOW GRANTS" {
        let mut body = String::from("[\"GRANTS\"]\n[\"String\"]\n");
        for g in grants {
            body.push_str(&format!("[\"{g}\"]\n"));
        }
        Some(ok_steps(rec, &body))
    } else {
        None
    }
}

/// A map of counters by key, for the tests of concurrency.
#[derive(Default)]
pub struct Tally(pub Mutex<HashMap<String, usize>>);
