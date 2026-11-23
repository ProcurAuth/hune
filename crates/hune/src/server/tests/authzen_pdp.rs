//! A fake PDP and an edge for the tests of the mode `authzen`: an in-process AuthZEN server on
//! `127.0.0.1`, and identity tokens signed with a test key. Tests are not product code: they can
//! use trait objects.

use crate::config::{AccessConfig, AuthzenConfig, Config, IdentityConfig, PdpConfig, Secret};
use procurauth_core::jws::{SigningKey, sign_compact};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

pub const AUDIENCE: &str = "hune-test";
pub const ISSUER: &str = "https://edge.example";
pub const HEADER: &str = "X-Identity-Token";
pub const PDP_TOKEN: &str = "pdp-test-credential";

/// One question that the fake PDP got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Q {
    pub sub: String,
    pub action: String,
    /// The chain and the node: `service:hune-test/folder:transit/dashboard:nodes`.
    pub resource: String,
    pub subtree: bool,
    pub usage: bool,
}

impl Q {
    /// The question without the service prefix: `folder:transit/dashboard:nodes`.
    pub fn short(&self) -> String {
        self.resource.split_once('/').map_or(String::new(), |(_, r)| r.to_owned())
    }
}

/// The answer of the fake PDP to one question.
#[derive(Debug, Clone)]
pub enum Ans {
    Yes(Vec<(&'static str, Value)>),
    No(Option<&'static str>),
}

impl Ans {
    pub fn yes() -> Ans {
        Ans::Yes(Vec::new())
    }
    fn to_value(&self) -> Value {
        match self {
            Ans::Yes(limits) if limits.is_empty() => json!({"decision": true}),
            Ans::Yes(limits) => {
                let map: BTreeMap<&str, &Value> = limits.iter().map(|(k, v)| (*k, v)).collect();
                json!({"decision": true, "context": {"limits": map}})
            }
            Ans::No(None) => json!({"decision": false}),
            Ans::No(Some(r)) => json!({"decision": false, "context": {"reason": r}}),
        }
    }
}

/// The failure mode of the whole server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    None,
    /// HTTP 503 on each question.
    Status503,
    /// HTTP 401 on each question.
    Status401,
    /// A 200 answer that is not JSON.
    BadJson,
    /// Wait this long before the answer.
    Delay(Duration),
}

pub type Rules = Arc<dyn Fn(&Q) -> Ans + Send + Sync>;

pub struct Pdp {
    pub addr: SocketAddr,
    pub questions: Arc<Mutex<Vec<Q>>>,
    pub http_requests: Arc<AtomicUsize>,
    pub bearer_seen: Arc<Mutex<Vec<String>>>,
    pub request_ids: Arc<Mutex<Vec<String>>>,
    /// The values of the header `traceparent` of the requests.
    pub traceparents: Arc<Mutex<Vec<String>>>,
    rules: Arc<Mutex<Rules>>,
    fault: Arc<Mutex<Fault>>,
    task: JoinHandle<()>,
}

impl Drop for Pdp {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn text(v: Option<&Value>) -> &str {
    v.and_then(Value::as_str).unwrap_or("")
}

fn chain(resource: &Value) -> String {
    let mut parts: Vec<String> = resource
        .pointer("/properties/ancestors")
        .and_then(Value::as_array)
        .map(|l| l.iter().map(|a| format!("{}:{}", text(a.get("type")), text(a.get("id")))).collect())
        .unwrap_or_default();
    parts.push(format!("{}:{}", text(resource.get("type")), text(resource.get("id"))));
    parts.join("/")
}

fn questions_of(body: &Value) -> (Vec<Q>, bool) {
    let sub = text(body.pointer("/subject/id")).to_owned();
    let subtree = text(body.pointer("/context/match")) == "subtree";
    let make = |item: &Value| Q {
        sub: sub.clone(),
        action: text(item.pointer("/action/name")).to_owned(),
        resource: chain(item.get("resource").unwrap_or(&Value::Null)),
        subtree,
        usage: body.pointer("/context/usage").is_some(),
    };
    match body.get("evaluations").and_then(Value::as_array) {
        Some(list) => (list.iter().map(make).collect(), true),
        None => (vec![make(body)], false),
    }
}

impl Pdp {
    /// Starts a PDP. `rules` answers each question.
    pub async fn start(rules: impl Fn(&Q) -> Ans + Send + Sync + 'static) -> Pdp {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let questions = Arc::new(Mutex::new(Vec::new()));
        let http_requests = Arc::new(AtomicUsize::new(0));
        let bearer_seen = Arc::new(Mutex::new(Vec::new()));
        let request_ids = Arc::new(Mutex::new(Vec::new()));
        let traceparents = Arc::new(Mutex::new(Vec::new()));
        let rules: Arc<Mutex<Rules>> = Arc::new(Mutex::new(Arc::new(rules)));
        let fault = Arc::new(Mutex::new(Fault::None));
        let state = (
            questions.clone(),
            http_requests.clone(),
            bearer_seen.clone(),
            request_ids.clone(),
            traceparents.clone(),
            rules.clone(),
            fault.clone(),
        );
        let task = tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else { return };
                tokio::spawn(serve(sock, state.clone()));
            }
        });
        Pdp { addr, questions, http_requests, bearer_seen, request_ids, traceparents, rules, fault, task }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn set_fault(&self, f: Fault) {
        *self.fault.lock().unwrap() = f;
    }

    pub fn set_rules(&self, rules: impl Fn(&Q) -> Ans + Send + Sync + 'static) {
        *self.rules.lock().unwrap() = Arc::new(rules);
    }

    /// The questions so far.
    pub fn asked(&self) -> Vec<Q> {
        self.questions.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.questions.lock().unwrap().clear();
        self.http_requests.store(0, Ordering::SeqCst);
    }

    pub fn requests(&self) -> usize {
        self.http_requests.load(Ordering::SeqCst)
    }
}

type State = (
    Arc<Mutex<Vec<Q>>>,
    Arc<AtomicUsize>,
    Arc<Mutex<Vec<String>>>,
    Arc<Mutex<Vec<String>>>,
    Arc<Mutex<Vec<String>>>,
    Arc<Mutex<Rules>>,
    Arc<Mutex<Fault>>,
);

async fn serve(mut s: TcpStream, st: State) {
    let (questions, requests, bearer, ids, traceparents, rules, fault) = st;
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
        let first = lines.next().unwrap_or("").to_owned();
        let headers: Vec<(String, String)> = lines
            .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned())))
            .collect();
        let len: usize =
            headers.iter().find(|(k, _)| k == "content-length").and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
        buf.drain(..head_end + 4);
        while buf.len() < len {
            let mut tmp = [0u8; 8192];
            match s.read(&mut tmp).await {
                Ok(0) | Err(_) => return,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
            }
        }
        let body: Vec<u8> = buf.drain(..len).collect();
        let path = first.split(' ').nth(1).unwrap_or("").to_owned();
        let send = |status: u16, body: &str| {
            format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
        };
        let reply = if path.starts_with("/.well-known/") {
            send(404, "{}")
        } else {
            requests.fetch_add(1, Ordering::SeqCst);
            if let Some((_, v)) = headers.iter().find(|(k, _)| k == "authorization") {
                bearer.lock().unwrap().push(v.clone());
            }
            if let Some((_, v)) = headers.iter().find(|(k, _)| k == "x-request-id") {
                ids.lock().unwrap().push(v.clone());
            }
            if let Some((_, v)) = headers.iter().find(|(k, _)| k == "traceparent") {
                traceparents.lock().unwrap().push(v.clone());
            }
            let f = *fault.lock().unwrap();
            match f {
                Fault::Delay(d) => tokio::time::sleep(d).await,
                Fault::Status503 => {
                    if s.write_all(send(503, "{}").as_bytes()).await.is_err() {
                        return;
                    }
                    continue;
                }
                Fault::Status401 => {
                    if s.write_all(send(401, "{}").as_bytes()).await.is_err() {
                        return;
                    }
                    continue;
                }
                Fault::BadJson => {
                    if s.write_all(send(200, "not json").as_bytes()).await.is_err() {
                        return;
                    }
                    continue;
                }
                Fault::None => {}
            }
            let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let (qs, batch) = questions_of(&value);
            let rule = rules.lock().unwrap().clone();
            let answers: Vec<Value> = qs.iter().map(|q| rule(q).to_value()).collect();
            questions.lock().unwrap().extend(qs);
            let out = if batch { json!({"evaluations": answers}) } else { answers[0].clone() };
            send(200, &out.to_string())
        };
        if s.write_all(reply.as_bytes()).await.is_err() {
            return;
        }
    }
}

/// The key of the test edge.
pub fn edge_key() -> SigningKey {
    SigningKey::from_ed25519_seed(&[7u8; 32])
}

/// The JWK Set of the test edge, with the `kid` `edge-1`.
pub fn jwks() -> String {
    edge_key().public_jwks(Some("edge-1")).to_string()
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A good identity token for `sub`.
pub fn token(sub: &str) -> String {
    token_with(sub, |_| {})
}

/// An identity token whose claims a test changes.
pub fn token_with(sub: &str, change: impl FnOnce(&mut Value)) -> String {
    let t = now();
    let mut claims = json!({"iss": ISSUER, "sub": sub, "aud": AUDIENCE, "iat": t, "exp": t + 120});
    change(&mut claims);
    sign_compact(&json!({"alg": "Ed25519", "typ": "JWT", "kid": "edge-1"}), &claims, &edge_key())
}

/// Puts the mode `authzen` in the configuration: the PDP at `url`, a second source `conf`.
pub fn authzen(c: &mut Config, pdp_url: &str, cache_ttl: Duration) {
    c.access = AccessConfig::Authzen(AuthzenConfig {
        identity: IdentityConfig {
            header: HEADER.to_owned(),
            issuer: ISSUER.to_owned(),
            audience: AUDIENCE.to_owned(),
            public_keys: jwks(),
            max_age: Duration::from_secs(300),
            clock_skew: Duration::from_secs(60),
        },
        pdp: PdpConfig {
            url: pdp_url.to_owned(),
            token: Secret::new(PDP_TOKEN.to_owned()),
            plain_http: true,
            ca_file: None,
            ca_pem: None,
            timeout: Duration::from_millis(1500),
            cache_ttl,
            max_connections: 4,
        },
    });
    let mut conf = c.clickhouse[0].clone();
    conf.id = "conf".to_owned();
    conf.label = "Conformance".to_owned();
    c.clickhouse[0].max_concurrent_per_subject = Some(4);
    conf.max_concurrent_per_subject = Some(4);
    c.clickhouse.push(conf);
}
