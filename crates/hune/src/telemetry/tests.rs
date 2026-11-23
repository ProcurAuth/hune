//! Tests of the span model, the encoding, the queue and the exporter, with a fake collector on
//! 127.0.0.1.

use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio::net::TcpListener;
use tokio::time::Instant;
use tokio_rustls::TlsAcceptor;

use super::otlp::Encoder;
use super::span::{Attr, AttrValue, SpanData, SpanKind, SpanStatus, TraceContext, hex};
use super::*;
use crate::config::{TelemetryConfig, TelemetryEnabled};

const CA_PEM: &[u8] = include_bytes!("../../../hune-source/tests/fixtures/tls/ca.pem");
const OTHER_CA_PEM: &[u8] = include_bytes!("../../../hune-source/tests/fixtures/tls/other-ca.pem");
const SERVER_PEM: &[u8] = include_bytes!("../../../hune-source/tests/fixtures/tls/server.pem");
const SERVER_KEY: &[u8] = include_bytes!("../../../hune-source/tests/fixtures/tls/server.key");

// ---------------------------------------------------------------- traceparent

#[test]
fn traceparent_valid_round_trip() {
    let text = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
    let ctx = TraceContext::parse(text).expect("valid");
    assert_eq!(hex(&ctx.trace_id), "0af7651916cd43dd8448eb211c80319c");
    assert_eq!(hex(&ctx.span_id), "b7ad6b7169203331");
    assert_eq!(ctx.flags, 1);
    assert_eq!(ctx.format(), text);
    let unsampled = TraceContext::parse("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-00").expect("valid");
    assert_eq!(unsampled.flags, 0);
}

#[test]
fn traceparent_invalid_forms_are_refused() {
    let bad = [
        "",
        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331",
        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01-",
        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01-extra",
        "01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        "ff-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        "00-0AF7651916CD43DD8448EB211C80319C-b7ad6b7169203331-01",
        "00-0af7651916cd43dd8448eb211c80319c-B7AD6B7169203331-01",
        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-0A",
        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-0g",
        "00_0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        "00-0af7651916cd43dd8448eb211c80319c_b7ad6b7169203331-01",
        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331_01",
        " 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01",
        "00-0af7651916cd43dd8448eb211c80319z-b7ad6b7169203331-01",
        "00-0af7651916cd43dd8448eb211c80319c-b7ad6b716920333-001",
    ];
    for text in bad {
        assert!(TraceContext::parse(text).is_none(), "{text:?} must be refused");
    }
}

#[test]
fn traceparent_all_zero_ids_are_refused() {
    assert!(TraceContext::parse("00-00000000000000000000000000000000-b7ad6b7169203331-01").is_none());
    assert!(TraceContext::parse("00-0af7651916cd43dd8448eb211c80319c-0000000000000000-01").is_none());
}

#[test]
fn traceparent_non_ascii_is_refused() {
    // A multi-byte character must not panic the parser.
    assert!(TraceContext::parse("00-0af7651916cd43dd8448eb211c80319c-b7ad6b71692033é1-01").is_none());
}

// ---------------------------------------------------------------- encoding

fn sample_span() -> SpanData {
    SpanData {
        trace_id: [0x0a, 0xf7, 0x65, 0x19, 0x16, 0xcd, 0x43, 0xdd, 0x84, 0x48, 0xeb, 0x21, 0x1c, 0x80, 0x31, 0x9c],
        span_id: [0xb7, 0xad, 0x6b, 0x71, 0x69, 0x20, 0x33, 0x31],
        parent: Some([1, 2, 3, 4, 5, 6, 7, 8]),
        kind: SpanKind::Server,
        name: "GET /api/v1/dashboards/{uid}".to_owned(),
        start_unix_nanos: 1_700_000_000_000_000_001,
        end_unix_nanos: 1_700_000_000_500_000_000,
        attributes: vec![
            Attr { key: "http.request.method", value: AttrValue::Str("GET".to_owned()) },
            Attr { key: "http.response.status_code", value: AttrValue::Int(500) },
            Attr { key: "hune.flag", value: AttrValue::Bool(true) },
            Attr { key: "q", value: AttrValue::Str("a\"b\\c\n".to_owned()) },
        ],
        status: SpanStatus::Error,
    }
}

#[test]
fn encoding_golden_json() {
    let encoder = Encoder::new("hune", "0.1.0", "11111111-2222-4333-8444-555555555555");
    let mut client = sample_span();
    client.parent = None;
    client.kind = SpanKind::Client;
    client.name = "clickhouse.query".to_owned();
    client.attributes = Vec::new();
    client.status = SpanStatus::Unset;
    let body = String::from_utf8(encoder.encode(&[sample_span(), client])).expect("utf8");
    let expected = concat!(
        r#"{"resourceSpans":[{"resource":{"attributes":["#,
        r#"{"key":"service.name","value":{"stringValue":"hune"}},"#,
        r#"{"key":"service.version","value":{"stringValue":"0.1.0"}},"#,
        r#"{"key":"service.instance.id","value":{"stringValue":"11111111-2222-4333-8444-555555555555"}}"#,
        r#"]},"scopeSpans":[{"scope":{"name":"hune"},"spans":["#,
        r#"{"traceId":"0af7651916cd43dd8448eb211c80319c","spanId":"b7ad6b7169203331","parentSpanId":"0102030405060708","#,
        r#""name":"GET /api/v1/dashboards/{uid}","kind":2,"startTimeUnixNano":"1700000000000000001","#,
        r#""endTimeUnixNano":"1700000000500000000","attributes":["#,
        r#"{"key":"http.request.method","value":{"stringValue":"GET"}},"#,
        r#"{"key":"http.response.status_code","value":{"intValue":"500"}},"#,
        r#"{"key":"hune.flag","value":{"boolValue":true}},"#,
        r#"{"key":"q","value":{"stringValue":"a\"b\\c\n"}}"#,
        r#"],"status":{"code":2}},"#,
        r#"{"traceId":"0af7651916cd43dd8448eb211c80319c","spanId":"b7ad6b7169203331","#,
        r#""name":"clickhouse.query","kind":3,"startTimeUnixNano":"1700000000000000001","#,
        r#""endTimeUnixNano":"1700000000500000000","attributes":[],"status":{"code":0}}"#,
        r#"]}]}]}"#,
    );
    assert_eq!(body, expected);
    let value: serde_json::Value = serde_json::from_str(&body).expect("the body is JSON");
    assert_eq!(value["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array().map(Vec::len), Some(2));
}

#[test]
fn encoding_empty_batch_is_valid_json() {
    let encoder = Encoder::new("hune", "0.1.0", "id");
    let body = encoder.encode(&[]);
    let value: serde_json::Value = serde_json::from_slice(&body).expect("JSON");
    assert_eq!(value["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array().map(Vec::len), Some(0));
}

// ---------------------------------------------------------------- span builders

struct Probe {
    states: Arc<Mutex<Vec<ExportState>>>,
    dropped: Arc<AtomicU64>,
}

impl Probe {
    fn new() -> Probe {
        Probe { states: Arc::new(Mutex::new(Vec::new())), dropped: Arc::new(AtomicU64::new(0)) }
    }
    fn hooks(&self) -> (DroppedHook, StateHook) {
        let dropped = Arc::clone(&self.dropped);
        let states = Arc::clone(&self.states);
        (
            Arc::new(move |n| {
                dropped.fetch_add(n, Ordering::SeqCst);
            }),
            Arc::new(move |s| states.lock().unwrap().push(s)),
        )
    }
    fn states(&self) -> Vec<ExportState> {
        self.states.lock().unwrap().clone()
    }
}

fn enabled_cfg(endpoint: &str, ca_pem: Option<&[u8]>, interval: Duration) -> TelemetryConfig {
    TelemetryConfig::Enabled(TelemetryEnabled {
        otlp_endpoint: endpoint.to_owned(),
        ca_file: None,
        ca_pem: ca_pem.map(<[u8]>::to_vec),
        service_name: "hune-test".to_owned(),
        export_interval: interval,
    })
}

fn start_with(probe: &Probe, cfg: &TelemetryConfig) -> (Telemetry, ExporterTask) {
    let (d, s) = probe.hooks();
    Telemetry::start(cfg, d, s).expect("start")
}

fn quick_span(t: &Telemetry) {
    let mut s = t.span_server("GET /x", None);
    s.set_int("http.response.status_code", 200);
    s.end();
}

#[test]
fn disabled_handle_makes_nothing() {
    let t = Telemetry::disabled();
    assert!(!t.is_enabled());
    let mut s = t.span_server("GET /x", None);
    assert!(!s.is_recording());
    assert!(s.context().is_none());
    assert!(s.traceparent().is_none());
    s.set_str("http.route", "/x");
    s.set_error();
    s.end();
    let c = t.span_client(ClientSpan::ClickhouseQuery, None);
    assert!(!c.is_recording());
    assert_eq!(t.dropped_spans(), 0);
    assert_eq!(t.queued_spans(), 0);
}

#[tokio::test]
async fn disabled_start_and_shutdown_are_immediate() {
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &TelemetryConfig::Disabled);
    assert!(!t.is_enabled());
    task.run().await;
    let start = Instant::now();
    t.shutdown().await;
    assert!(start.elapsed() < Duration::from_millis(100));
}

#[test]
fn parent_and_child_share_the_trace() {
    let probe = Probe::new();
    let cfg = enabled_cfg("http://127.0.0.1:1", None, Duration::from_secs(3600));
    let (t, _task) = start_with(&probe, &cfg);
    let incoming = TraceContext::parse("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01").unwrap();
    let server = t.span_server("GET /x", Some(&incoming));
    let ctx = server.context().unwrap();
    assert_eq!(ctx.trace_id, incoming.trace_id);
    assert_ne!(ctx.span_id, incoming.span_id);
    let client = t.span_client(ClientSpan::StoreOperation("save"), Some(&ctx));
    let cctx = client.context().unwrap();
    assert_eq!(cctx.trace_id, ctx.trace_id);
    assert!(TraceContext::parse(&client.traceparent().unwrap()).is_some());
    client.end();
    server.end();
    let root = t.span_server("GET /y", None);
    assert_ne!(root.context().unwrap().trace_id, ctx.trace_id);
    let queue = t.shared.as_ref().unwrap().lock();
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].name, "store.save");
    assert_eq!(queue[0].kind, SpanKind::Client);
    assert_eq!(queue[0].parent, Some(ctx.span_id));
    assert_eq!(queue[1].parent, Some(incoming.span_id));
    assert_eq!(queue[1].kind, SpanKind::Server);
}

#[test]
fn attributes_follow_the_closed_list() {
    let probe = Probe::new();
    let cfg = enabled_cfg("http://127.0.0.1:1", None, Duration::from_secs(3600));
    let (t, _task) = start_with(&probe, &cfg);
    let mut s = t.span_client(ClientSpan::PdpEvaluate, None);
    s.set_str("server.address", "pdp");
    s.set_int("hune.questions", 3);
    s.set_int("hune.questions", 4);
    // Keys of other span types and of 12.15 are dropped.
    s.set_str("db.statement", "SELECT 1");
    s.set_str("enduser.id", "alice");
    s.set_str("http.route", "/x");
    s.set_error();
    s.end();
    let long = "é".repeat(300);
    let mut r = t.span_server("GET /x", None);
    r.set_str("hune.request_id", &long);
    r.end();
    let queue = t.shared.as_ref().unwrap().lock();
    assert_eq!(queue[0].name, "pdp.evaluate");
    assert_eq!(queue[0].status, SpanStatus::Error);
    assert_eq!(
        queue[0].attributes,
        vec![
            Attr { key: "server.address", value: AttrValue::Str("pdp".to_owned()) },
            Attr { key: "hune.questions", value: AttrValue::Int(4) },
        ]
    );
    match &queue[1].attributes[0].value {
        AttrValue::Str(v) => assert!(v.len() <= 256 && v.chars().all(|c| c == 'é')),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn end_before_start_gives_zero_length() {
    let probe = Probe::new();
    let cfg = enabled_cfg("http://127.0.0.1:1", None, Duration::from_secs(3600));
    let (t, _task) = start_with(&probe, &cfg);
    t.span_server("GET /x", None).end_at(0);
    let queue = t.shared.as_ref().unwrap().lock();
    assert_eq!(queue[0].end_unix_nanos, queue[0].start_unix_nanos);
}

#[test]
fn start_refuses_bad_endpoint_and_missing_ca() {
    let probe = Probe::new();
    let (d, s) = probe.hooks();
    for bad in ["ftp://x:1", "http://", "http://x/path", "http://x:0", "http://x:99999", "x:4318"] {
        let cfg = enabled_cfg(bad, None, Duration::from_secs(15));
        assert!(Telemetry::start(&cfg, Arc::clone(&d), Arc::clone(&s)).is_err(), "{bad}");
    }
    let https = enabled_cfg("https://127.0.0.1:4318", None, Duration::from_secs(15));
    let e = Telemetry::start(&https, Arc::clone(&d), Arc::clone(&s)).err().unwrap();
    assert!(e.to_string().contains("telemetry.ca_file"));
    let bad_pem = enabled_cfg("https://127.0.0.1:4318", Some(b"nothing"), Duration::from_secs(15));
    assert!(Telemetry::start(&bad_pem, d, s).is_err());
}

#[test]
fn endpoint_parse() {
    let e = client::Endpoint::parse("http://otel-collector:4318").unwrap();
    assert_eq!((e.https, e.host.as_str(), e.port), (false, "otel-collector", 4318));
    let e = client::Endpoint::parse("https://[::1]").unwrap();
    assert_eq!((e.https, e.host.as_str(), e.port), (true, "::1", 443));
}

#[test]
fn queue_full_drops_the_new_span_and_counts_it() {
    let probe = Probe::new();
    let cfg = enabled_cfg("http://127.0.0.1:1", None, Duration::from_secs(3600));
    let (t, _task) = start_with(&probe, &cfg);
    for _ in 0..(QUEUE_CAPACITY + 5) {
        quick_span(&t);
    }
    assert_eq!(t.queued_spans(), QUEUE_CAPACITY);
    assert_eq!(t.dropped_spans(), 5);
    assert_eq!(probe.dropped.load(Ordering::SeqCst), 5);
}

// ---------------------------------------------------------------- fake collector

#[derive(Debug, Clone)]
struct Received {
    method: String,
    path: String,
    content_type: String,
    user_agent: String,
    host: String,
    body: serde_json::Value,
    spans: usize,
}

struct Collector {
    port: u16,
    received: Arc<Mutex<Vec<Received>>>,
    status: Arc<AtomicU16>,
    delay_ms: Arc<AtomicU64>,
    close_at_once: Arc<AtomicBool>,
}

impl Collector {
    async fn plain() -> Collector {
        Collector::spawn(None).await
    }

    async fn tls() -> Collector {
        let certs: Vec<CertificateDer<'static>> =
            CertificateDer::pem_slice_iter(SERVER_PEM).collect::<Result<_, _>>().unwrap();
        let key = PrivateKeyDer::from_pem_slice(SERVER_KEY).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap();
        Collector::spawn(Some(Arc::new(config))).await
    }

    async fn spawn(tls: Option<Arc<rustls::ServerConfig>>) -> Collector {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let c = Collector {
            port,
            received: Arc::new(Mutex::new(Vec::new())),
            status: Arc::new(AtomicU16::new(200)),
            delay_ms: Arc::new(AtomicU64::new(0)),
            close_at_once: Arc::new(AtomicBool::new(false)),
        };
        let received = Arc::clone(&c.received);
        let status = Arc::clone(&c.status);
        let delay = Arc::clone(&c.delay_ms);
        let close = Arc::clone(&c.close_at_once);
        tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else { return };
                if close.load(Ordering::SeqCst) {
                    drop(tcp);
                    continue;
                }
                let received = Arc::clone(&received);
                let status = Arc::clone(&status);
                let delay = Arc::clone(&delay);
                let tls = tls.clone();
                tokio::spawn(async move {
                    let service = service_fn(move |req: http::Request<hyper::body::Incoming>| {
                        let received = Arc::clone(&received);
                        let status = Arc::clone(&status);
                        let delay = Arc::clone(&delay);
                        async move {
                            let header = |name: &str| {
                                req.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or("").to_owned()
                            };
                            let method = req.method().to_string();
                            let path = req.uri().path().to_owned();
                            let content_type = header("content-type");
                            let user_agent = header("user-agent");
                            let host = header("host");
                            let body = req.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();
                            let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                            let spans =
                                json["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array().map_or(0, Vec::len);
                            received.lock().unwrap().push(Received {
                                method,
                                path,
                                content_type,
                                user_agent,
                                host,
                                body: json,
                                spans,
                            });
                            let d = delay.load(Ordering::SeqCst);
                            if d > 0 {
                                tokio::time::sleep(Duration::from_millis(d)).await;
                            }
                            let code = status.load(Ordering::SeqCst);
                            let mut resp = http::Response::new(Full::new(Bytes::from_static(b"{}")));
                            *resp.status_mut() = http::StatusCode::from_u16(code).unwrap();
                            Ok::<_, std::convert::Infallible>(resp)
                        }
                    });
                    let builder = hyper::server::conn::http1::Builder::new();
                    match tls {
                        None => {
                            let _ = builder.serve_connection(TokioIo::new(tcp), service).await;
                        }
                        Some(config) => {
                            let Ok(stream) = TlsAcceptor::from(config).accept(tcp).await else {
                                return;
                            };
                            let _ = builder.serve_connection(TokioIo::new(stream), service).await;
                        }
                    }
                });
            }
        });
        c
    }

    fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn https_endpoint(&self) -> String {
        format!("https://127.0.0.1:{}", self.port)
    }

    fn requests(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }

    fn total_spans(&self) -> usize {
        self.requests().iter().map(|r| r.spans).sum()
    }
}

async fn wait_for(what: &str, limit: Duration, mut cond: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while !cond() {
        assert!(Instant::now() < end, "timeout while waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

// ---------------------------------------------------------------- exporter

#[tokio::test]
async fn export_by_interval_with_headers_and_resource() {
    let collector = Collector::plain().await;
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_millis(100)));
    let handle = tokio::spawn(task.run());
    let mut s = t.span_server("GET /x", None);
    s.set_str("http.route", "/x");
    s.end();
    quick_span(&t);
    quick_span(&t);
    wait_for("3 spans", Duration::from_secs(3), || collector.total_spans() == 3).await;
    let first = &collector.requests()[0];
    assert_eq!(first.method, "POST");
    assert_eq!(first.path, "/v1/traces");
    assert_eq!(first.content_type, "application/json");
    assert_eq!(first.user_agent, format!("hune/{}", env!("CARGO_PKG_VERSION")));
    assert_eq!(first.host, format!("127.0.0.1:{}", collector.port));
    let attrs = &first.body["resourceSpans"][0]["resource"]["attributes"];
    assert_eq!(attrs[0]["key"], "service.name");
    assert_eq!(attrs[0]["value"]["stringValue"], "hune-test");
    assert_eq!(attrs[1]["key"], "service.version");
    assert_eq!(attrs[2]["key"], "service.instance.id");
    assert_eq!(attrs[2]["value"]["stringValue"].as_str().unwrap().len(), 36);
    assert_eq!(first.body["resourceSpans"][0]["scopeSpans"][0]["scope"]["name"], "hune");
    assert_eq!(t.queued_spans(), 0);
    assert_eq!(t.dropped_spans(), 0);
    assert!(probe.states().is_empty());
    t.shutdown().await;
    handle.await.unwrap();
}

#[tokio::test]
async fn export_by_batch_size_and_limit() {
    let collector = Collector::plain().await;
    let probe = Probe::new();
    // A long interval: only the size of the queue starts the first exports.
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_millis(1500)));
    for _ in 0..1200 {
        quick_span(&t);
    }
    let handle = tokio::spawn(task.run());
    wait_for("two full batches", Duration::from_secs(1), || collector.total_spans() >= 1024).await;
    let sizes: Vec<usize> = collector.requests().iter().map(|r| r.spans).collect();
    assert_eq!(sizes, vec![512, 512]);
    assert_eq!(t.queued_spans(), 176);
    wait_for("the rest by interval", Duration::from_secs(4), || collector.total_spans() == 1200).await;
    let sizes: Vec<usize> = collector.requests().iter().map(|r| r.spans).collect();
    assert_eq!(sizes, vec![512, 512, 176]);
    t.shutdown().await;
    handle.await.unwrap();
}

#[tokio::test]
async fn export_starts_at_once_when_512_spans_wait() {
    let collector = Collector::plain().await;
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_secs(3600)));
    let handle = tokio::spawn(task.run());
    for _ in 0..511 {
        quick_span(&t);
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(collector.total_spans(), 0);
    quick_span(&t);
    wait_for("the batch", Duration::from_secs(3), || collector.total_spans() == 512).await;
    t.shutdown().await;
    handle.await.unwrap();
}

#[tokio::test]
async fn deadline_is_the_lower_of_interval_and_ten_seconds() {
    let collector = Collector::plain().await;
    collector.delay_ms.store(5000, Ordering::SeqCst);
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_millis(200)));
    let handle = tokio::spawn(task.run());
    let start = Instant::now();
    quick_span(&t);
    quick_span(&t);
    wait_for("the failure", Duration::from_secs(3), || !probe.states().is_empty()).await;
    assert!(start.elapsed() < Duration::from_millis(2500), "the deadline of 200 ms did not apply");
    assert_eq!(t.dropped_spans(), 2);
    match &probe.states()[0] {
        ExportState::Failed { error, dropped } => {
            assert!(error.contains("deadline"), "{error}");
            assert_eq!(*dropped, 2);
        }
        other => panic!("unexpected {other:?}"),
    }
    t.shutdown().await;
    handle.await.unwrap();
}

#[tokio::test]
async fn non_2xx_is_counted_logged_once_and_not_resent() {
    let collector = Collector::plain().await;
    collector.status.store(503, Ordering::SeqCst);
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_millis(60)));
    let handle = tokio::spawn(task.run());
    for i in 1..=3usize {
        quick_span(&t);
        wait_for("an export", Duration::from_secs(3), || collector.requests().len() == i).await;
        wait_for("the count", Duration::from_secs(3), || t.dropped_spans() == i as u64).await;
    }
    assert_eq!(probe.states().len(), 1, "one line for the change of state");
    match &probe.states()[0] {
        ExportState::Failed { error, dropped } => {
            assert_eq!(error, "status 503");
            assert_eq!(*dropped, 1);
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(probe.dropped.load(Ordering::SeqCst), 3);
    // The collector recovers: one line for the change, and no old span comes again.
    collector.status.store(200, Ordering::SeqCst);
    quick_span(&t);
    wait_for("the recovery", Duration::from_secs(3), || probe.states().len() == 2).await;
    assert_eq!(probe.states()[1], ExportState::Recovered);
    tokio::time::sleep(Duration::from_millis(250)).await;
    let sizes: Vec<usize> = collector.requests().iter().map(|r| r.spans).collect();
    assert_eq!(sizes, vec![1, 1, 1, 1], "each span goes out one time");
    assert_eq!(t.dropped_spans(), 3);
    // A new failure is a new change of state.
    collector.status.store(500, Ordering::SeqCst);
    quick_span(&t);
    wait_for("the second failure", Duration::from_secs(3), || probe.states().len() == 3).await;
    assert!(matches!(probe.states()[2], ExportState::Failed { .. }));
    t.shutdown().await;
    handle.await.unwrap();
}

#[tokio::test]
async fn closed_connection_is_a_failure() {
    let collector = Collector::plain().await;
    collector.close_at_once.store(true, Ordering::SeqCst);
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_millis(60)));
    let handle = tokio::spawn(task.run());
    quick_span(&t);
    quick_span(&t);
    wait_for("the failure", Duration::from_secs(3), || t.dropped_spans() == 2).await;
    quick_span(&t);
    wait_for("the next failure", Duration::from_secs(3), || t.dropped_spans() == 3).await;
    assert_eq!(probe.states().len(), 1);
    assert!(matches!(&probe.states()[0], ExportState::Failed { dropped: 2, .. }));
    t.shutdown().await;
    handle.await.unwrap();
}

#[tokio::test]
async fn refused_connection_is_a_failure() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let probe = Probe::new();
    let (t, task) =
        start_with(&probe, &enabled_cfg(&format!("http://127.0.0.1:{port}"), None, Duration::from_millis(60)));
    let handle = tokio::spawn(task.run());
    quick_span(&t);
    wait_for("the failure", Duration::from_secs(3), || t.dropped_spans() == 1).await;
    match &probe.states()[0] {
        ExportState::Failed { error, .. } => assert!(error.starts_with("connect:"), "{error}"),
        other => panic!("unexpected {other:?}"),
    }
    t.shutdown().await;
    handle.await.unwrap();
}

#[tokio::test]
async fn shutdown_exports_the_queue_one_last_time() {
    let collector = Collector::plain().await;
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_secs(3600)));
    let handle = tokio::spawn(task.run());
    for _ in 0..3 {
        quick_span(&t);
    }
    tokio::task::yield_now().await;
    assert_eq!(collector.total_spans(), 0);
    t.shutdown().await;
    assert_eq!(collector.total_spans(), 3);
    handle.await.unwrap();
    // A span after the stop is dropped and counted.
    quick_span(&t);
    assert_eq!(t.dropped_spans(), 1);
    assert_eq!(t.queued_spans(), 0);
}

#[tokio::test]
async fn shutdown_exports_several_batches() {
    let collector = Collector::plain().await;
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_secs(3600)));
    for _ in 0..1300 {
        quick_span(&t);
    }
    let handle = tokio::spawn(task.run());
    t.shutdown().await;
    handle.await.unwrap();
    assert_eq!(collector.total_spans(), 1300);
    assert_eq!(t.dropped_spans(), 0);
}

#[tokio::test]
async fn shutdown_is_bounded_with_a_silent_collector() {
    let collector = Collector::plain().await;
    collector.delay_ms.store(30_000, Ordering::SeqCst);
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_secs(3600)));
    let handle = tokio::spawn(task.run());
    quick_span(&t);
    quick_span(&t);
    let start = Instant::now();
    t.shutdown().await;
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(1500) && elapsed < Duration::from_millis(2600), "{elapsed:?}");
    handle.await.unwrap();
    assert_eq!(t.dropped_spans(), 2);
}

#[tokio::test]
async fn shutdown_interrupts_an_export_in_flight() {
    let collector = Collector::plain().await;
    collector.delay_ms.store(30_000, Ordering::SeqCst);
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_millis(100)));
    let handle = tokio::spawn(task.run());
    quick_span(&t);
    wait_for("the request", Duration::from_secs(3), || !collector.requests().is_empty()).await;
    collector.delay_ms.store(0, Ordering::SeqCst);
    let start = Instant::now();
    t.shutdown().await;
    assert!(start.elapsed() < Duration::from_millis(2600));
    handle.await.unwrap();
    // The first request got no answer. The final export sends the span again, because the
    // interrupted export did not fail.
    assert_eq!(collector.requests().len(), 2);
    assert_eq!(t.dropped_spans(), 0);
}

#[tokio::test]
async fn https_with_the_configured_ca() {
    let collector = Collector::tls().await;
    let probe = Probe::new();
    let cfg = enabled_cfg(&collector.https_endpoint(), Some(CA_PEM), Duration::from_millis(100));
    let (t, task) = start_with(&probe, &cfg);
    let handle = tokio::spawn(task.run());
    quick_span(&t);
    quick_span(&t);
    wait_for("the spans", Duration::from_secs(5), || collector.total_spans() == 2).await;
    assert_eq!(collector.requests()[0].path, "/v1/traces");
    assert!(probe.states().is_empty());
    t.shutdown().await;
    handle.await.unwrap();
}

#[tokio::test]
async fn https_with_another_ca_fails() {
    let collector = Collector::tls().await;
    let probe = Probe::new();
    let cfg = enabled_cfg(&collector.https_endpoint(), Some(OTHER_CA_PEM), Duration::from_millis(100));
    let (t, task) = start_with(&probe, &cfg);
    let handle = tokio::spawn(task.run());
    quick_span(&t);
    wait_for("the failure", Duration::from_secs(5), || t.dropped_spans() == 1).await;
    match &probe.states()[0] {
        ExportState::Failed { error, .. } => assert!(error.starts_with("tls:"), "{error}"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(collector.total_spans(), 0);
    t.shutdown().await;
    handle.await.unwrap();
}

#[tokio::test]
async fn plain_client_against_a_tls_collector_fails() {
    let collector = Collector::tls().await;
    let probe = Probe::new();
    let (t, task) = start_with(&probe, &enabled_cfg(&collector.endpoint(), None, Duration::from_millis(100)));
    let handle = tokio::spawn(task.run());
    quick_span(&t);
    wait_for("the failure", Duration::from_secs(5), || t.dropped_spans() == 1).await;
    t.shutdown().await;
    handle.await.unwrap();
}

#[test]
fn the_span_of_a_mongodb_query_has_the_keys_of_a_query_span() {
    use super::span::SpanType;
    assert_eq!(SpanType::MongoQuery.allowed_keys(), SpanType::ClickhouseQuery.allowed_keys());
    assert_eq!(SpanType::MongoQuery.kind(), SpanKind::Client);
    let t = Telemetry::disabled();
    assert!(!t.span_client(ClientSpan::MongoQuery, None).is_recording());
}
