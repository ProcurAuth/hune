//! A fake ClickHouse HTTP server for the tests: it speaks enough HTTP/1.1 and the format
//! `JSONCompactEachRowWithNamesAndTypes` to test the routes of the queries.
//! Tests are not product code: they can use trait objects.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// One request that the server received.
#[derive(Debug, Clone)]
pub struct Rec {
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Rec {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    pub fn param(&self, name: &str) -> Option<String> {
        let q = self.target.split_once('?').map_or("", |x| x.1);
        q.split('&').find_map(|p| {
            let (k, v) = p.split_once('=')?;
            (k == name).then(|| decode(v))
        })
    }
    pub fn sql(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn query_id(&self) -> String {
        self.param("query_id").unwrap_or_default()
    }
    pub fn is_probe(&self) -> bool {
        let s = self.sql();
        s.starts_with("SELECT version()") || s == "SHOW GRANTS"
    }
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            out.push(u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap(), 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// One action of a scripted answer.
#[derive(Debug, Clone)]
pub enum Step {
    Send(Vec<u8>),
    /// Wait until the client closes the connection.
    Hang,
    Sleep(Duration),
}

pub type Handler = Arc<dyn Fn(&Rec) -> Vec<Step> + Send + Sync>;

pub struct Fake {
    pub addr: SocketAddr,
    pub requests: Arc<Mutex<Vec<Rec>>>,
    /// The client closed the connection while the server waited in `Hang` or `Sleep`.
    pub client_closed: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fake {
    pub async fn start(handler: Handler) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let client_closed = Arc::new(AtomicUsize::new(0));
        let (r2, c2) = (requests.clone(), client_closed.clone());
        let task = tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else { return };
                tokio::spawn(serve(sock, handler.clone(), r2.clone(), c2.clone()));
            }
        });
        Fake { addr, requests, client_closed, task }
    }

    /// The requests that are not probes.
    pub fn queries(&self) -> Vec<Rec> {
        self.requests.lock().unwrap().iter().filter(|r| !r.is_probe()).cloned().collect()
    }

    pub fn closed(&self) -> usize {
        self.client_closed.load(Ordering::SeqCst)
    }
}

async fn serve(mut s: TcpStream, handler: Handler, requests: Arc<Mutex<Vec<Rec>>>, closed: Arc<AtomicUsize>) {
    let mut buf: Vec<u8> = Vec::new();
    loop {
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
        let target = first.split(' ').nth(1).unwrap_or("").to_owned();
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
        let rec = Rec { target, headers, body };
        requests.lock().unwrap().push(rec.clone());
        for step in handler(&rec) {
            match step {
                Step::Send(b) => {
                    if s.write_all(&b).await.is_err() {
                        return;
                    }
                }
                Step::Sleep(d) => {
                    let mut tmp = [0u8; 64];
                    tokio::select! {
                        () = tokio::time::sleep(d) => {}
                        r = s.read(&mut tmp) => {
                            if matches!(r, Ok(0) | Err(_)) {
                                closed.fetch_add(1, Ordering::SeqCst);
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
                                closed.fetch_add(1, Ordering::SeqCst);
                                return;
                            }
                            Ok(_) => {}
                        }
                    }
                }
            }
        }
    }
}

fn chunked(text: &str) -> Vec<u8> {
    let mut out = format!("{:x}\r\n", text.len()).into_bytes();
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(b"\r\n0\r\n\r\n");
    out
}

fn head(status: u16, extra: &[(&str, String)]) -> Vec<u8> {
    let mut h = format!("HTTP/1.1 {status} X\r\nTransfer-Encoding: chunked\r\nContent-Type: text/plain\r\n");
    for (k, v) in extra {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    h.into_bytes()
}

/// A complete 200 answer with the right query id and the summary header.
pub fn ok(rec: &Rec, body: &str) -> Vec<Step> {
    vec![
        Step::Send(head(
            200,
            &[
                ("X-ClickHouse-Query-Id", rec.query_id()),
                (
                    "X-ClickHouse-Summary",
                    "{\"read_rows\":\"10\",\"read_bytes\":\"100\",\"result_bytes\":\"50\"}".into(),
                ),
            ],
        )),
        Step::Send(chunked(body)),
    ]
}

/// An answer with a ClickHouse error.
pub fn error(rec: &Rec, status: u16, code: i64, text: &str) -> Vec<Step> {
    vec![
        Step::Send(head(
            status,
            &[("X-ClickHouse-Query-Id", rec.query_id()), ("X-ClickHouse-Exception-Code", code.to_string())],
        )),
        Step::Send(chunked(text)),
    ]
}

/// The body of a frame with three columns and two rows.
pub const FRAME_BODY: &str = "[\"t\",\"node\",\"weight\"]\n[\"DateTime\",\"String\",\"Decimal(38, 9)\"]\n[\"1794211200\",\"a1\",\"12.5\"]\n[\"1794211260\",null,\"3\"]\n";

/// The answers of a healthy account to the two statements of the probe (8.11).
pub fn probe_answer(rec: &Rec, version: &str, readonly: u8) -> Option<Vec<Step>> {
    let s = rec.sql();
    if s.starts_with("SELECT version()") {
        Some(ok(
            rec,
            &format!(
                "[\"version()\",\"getSetting('readonly')\"]\n[\"String\",\"UInt64\"]\n[\"{version}\",\"{readonly}\"]\n"
            ),
        ))
    } else if s == "SHOW GRANTS" {
        Some(ok(rec, "[\"GRANTS\"]\n[\"String\"]\n[\"GRANT SELECT ON analytics.* TO hune\"]\n"))
    } else {
        None
    }
}

/// A handler: a healthy probe, and `queries` for each other statement.
pub fn healthy(queries: impl Fn(&Rec) -> Vec<Step> + Send + Sync + 'static) -> Handler {
    Arc::new(move |rec| probe_answer(rec, "26.8.1.1", 2).unwrap_or_else(|| queries(rec)))
}
