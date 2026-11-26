// SPDX-License-Identifier: MIT OR Apache-2.0
//! Unit tests of the access layer: the action document (11.18, 11.19), the PEP configuration (11.10),
//! the transport (11.12) with and without TLS, and the start of `Access` (11.6).

use super::*;
use crate::tests::files;
use hune_source::SourceKind;
use procurauth_pep::transport::{PdpTransport, TransportError};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicUsize;
use std::thread;

const GOLDEN: &str = include_str!("../../../../tests/golden/api/action-document.json");
const AUTHZEN_HCL: &str = include_str!("../../../../tests/api/authzen.hcl");
const JWKS: &str = include_str!("../../../../tests/fixtures/config/jwks.json");
const TLS_CA: &str = include_str!("../../../../tests/api/tls/ca.pem");
const TLS_CERT: &str = include_str!("../../../../tests/api/tls/server.pem");
const TLS_KEY: &str = include_str!("../../../../tests/api/tls/server.key.pem");

fn source(id: &str, label: &str, rows: u64, seconds: u64) -> SourceInfo {
    SourceInfo {
        kind: SourceKind::ClickHouse,
        id: id.to_owned(),
        label: label.to_owned(),
        max_rows: rows,
        max_seconds: seconds,
    }
}

fn inputs(audience: &str, sources: Vec<SourceInfo>) -> PepInputs {
    let keys = crate::config::parse_public_key_set(JWKS).unwrap();
    PepInputs {
        audience: audience.to_owned(),
        header: "X-Identity-Token".to_owned(),
        issuer: "https://edge.example".to_owned(),
        keys,
        max_age_s: 300,
        clock_skew_s: 60,
        pdp_url: "https://pdp.example:4182".to_owned(),
        pdp_token: procurauth_pep::config::Secret::new("test-credential"),
        plain_http: false,
        timeout: Duration::from_millis(500),
        cache_ttl: Duration::from_secs(30),
        max_connections: 4,
        history: true,
        export: true,
        writable: true,
        sources,
    }
}

fn metrics() -> Arc<Metrics> {
    Arc::new(Metrics::new("0.1.0", "test", 0))
}

// ---- The action document ------------------------------------------------------------------------

#[test]
fn the_document_has_one_resource_for_each_source_of_the_three_kinds_and_names_no_kind() {
    let mut pg = source("metrics", "Metrics", 20_000, 90);
    pg.kind = SourceKind::Postgres;
    let mut mg = source("events", "Events", 30_000, 120);
    mg.kind = SourceKind::MongoDb;
    let i = inputs("dashboards", vec![source("prod", "Production", 100_000, 60), pg, mg]);
    let doc = i.build_document().unwrap().to_value();
    assert_eq!(
        doc["resources"],
        json!([
            {"path": "source:events", "label": "Events"},
            {"path": "source:metrics", "label": "Metrics"},
            {"path": "source:prod", "label": "Production"}
        ])
    );
    // The description of the type `source` names no kind (11.33), and no resource holds a kind.
    let text = doc.to_string();
    for kind in ["ClickHouse", "PostgreSQL", "postgres", "MongoDB", "mongodb"] {
        assert!(!text.contains(kind), "{kind}");
    }
    // The ceilings take the highest limit of the sources of the three kinds.
    let limits = doc["limits"].as_array().unwrap();
    assert_eq!((limits[0]["max"].as_u64(), limits[1]["max"].as_u64()), (Some(100_000), Some(120)));
}

#[test]
fn the_key_of_a_fault_of_a_source_names_its_kind() {
    // An id that the grammar of a path refuses gives a finding at the resource of the source.
    let mut pg = source("metrics:x/y", "Metrics", 20_000, 90);
    pg.kind = SourceKind::Postgres;
    let i = inputs("dashboards", vec![source("prod", "Production", 100_000, 60), pg]);
    let errors = i.check();
    let keys: Vec<&str> = errors.iter().map(|(k, _)| k.as_str()).collect();
    assert!(keys.contains(&"postgres \"metrics:x/y\".label"), "{errors:?}");
    let mut mg = source("events:x/y", "Events", 20_000, 90);
    mg.kind = SourceKind::MongoDb;
    let errors = inputs("dashboards", vec![source("prod", "Production", 100_000, 60), mg]).check();
    let keys: Vec<&str> = errors.iter().map(|(k, _)| k.as_str()).collect();
    assert!(keys.contains(&"mongodb \"events:x/y\".label"), "{errors:?}");
}

#[test]
fn the_document_of_11_18_for_two_sources() {
    let i = inputs(
        "dashboards",
        vec![source("preprod", "Pre-production", 50_000, 30), source("prod", "Production", 100_000, 60)],
    );
    let doc = i.build_document().unwrap().to_value();
    let want = json!({
        "server": "dashboards",
        "resource_types": [
            {"name": "folder", "parent": "service", "description": "One folder of dashboards, by its id."},
            {"name": "dashboard", "parent": "folder", "description": "One dashboard, by its uid."},
            {"name": "source", "parent": "service",
             "description": "One data source of the configuration, by the id of its block."}
        ],
        "actions": [
            {"name": "dashboard.read", "resource_types": ["service", "folder", "dashboard"],
             "description": "List and open dashboards, and run their saved queries and the queries of their variables.",
             "limits": ["max_rows", "max_duration"]},
            {"name": "dashboard.history", "resource_types": ["service", "folder", "dashboard"],
             "description": "List the versions of a dashboard of the primary store, read one version and compare two versions. With dashboard.write, restore a version."},
            {"name": "dashboard.export", "resource_types": ["service", "folder", "dashboard"],
             "description": "Download the file of the current document of a dashboard."},
            {"name": "dashboard.write", "resource_types": ["service", "folder", "dashboard"],
             "description": "Create, change, move, restore and delete dashboards. Create, rename and delete folders."},
            {"name": "source.query", "resource_types": ["service", "source"],
             "description": "Run SQL that is not saved on this source, and save dashboards whose SQL uses this source.",
             "limits": ["max_rows", "max_duration"]}
        ],
        "resources": [
            {"path": "source:preprod", "label": "Pre-production"},
            {"path": "source:prod", "label": "Production"}
        ],
        "limits": [
            {"name": "max_rows", "label": "Most rows of one query",
             "description": "The most rows that the result of one query can have.",
             "unit": "row", "kind": "ceiling", "min": 1, "max": 100_000},
            {"name": "max_duration", "label": "Longest query",
             "description": "The longest run of one query, in seconds.",
             "unit": "second", "kind": "ceiling", "min": 1, "max": 60}
        ]
    });
    assert_eq!(doc, want);
}

/// The names of the actions of the document, in order.
fn action_names(i: &PepInputs) -> Vec<String> {
    let doc = i.build_document().unwrap().to_value();
    doc["actions"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap().to_owned()).collect()
}

#[test]
fn dashboard_history_and_dashboard_export_are_published_only_when_their_key_is_true() {
    let sources = || vec![source("prod", "Production", 1000, 10)];
    let both = inputs("dashboards", sources());
    assert_eq!(
        action_names(&both),
        ["dashboard.read", "dashboard.history", "dashboard.export", "dashboard.write", "source.query"]
    );
    let mut no_history = inputs("dashboards", sources());
    no_history.history = false;
    assert_eq!(action_names(&no_history), ["dashboard.read", "dashboard.export", "dashboard.write", "source.query"]);
    let mut no_export = inputs("dashboards", sources());
    no_export.export = false;
    assert_eq!(action_names(&no_export), ["dashboard.read", "dashboard.history", "dashboard.write", "source.query"]);
    let mut neither = inputs("dashboards", sources());
    neither.history = false;
    neither.export = false;
    assert_eq!(action_names(&neither), ["dashboard.read", "dashboard.write", "source.query"]);
    // The write actions and the resources never depend on the keys: a primary store always exists.
    let doc = neither.build_document().unwrap().to_value();
    assert_eq!(doc["resources"][0]["path"], "source:prod");
    assert_eq!(doc["limits"][0]["max"], 1000);
    assert_eq!(doc["limits"][1]["max"], 10);
}

#[test]
fn the_largest_document_follows_the_bounds_of_11_19() {
    // 64 sources with labels of 60 characters of 4 bytes each.
    let label: String = "\u{1D11E}".repeat(60);
    assert_eq!(label.len(), 240);
    let sources: Vec<SourceInfo> = (0..64).map(|n| source(&format!("s{n:02}"), &label, 1_000_000_000, 3600)).collect();
    let i = inputs("a".repeat(256).as_str(), sources);
    let doc = i.build_document().expect("the document passes the checks of 5.2, 5.3 and 6.1");
    let bytes = doc.to_value().to_string();
    assert!(bytes.len() < 64 * 1024, "{} bytes", bytes.len());
    let v: Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(v["actions"].as_array().unwrap().len(), 5);
    assert_eq!(v["resource_types"].as_array().unwrap().len(), 3);
    assert_eq!(v["limits"].as_array().unwrap().len(), 2);
    assert_eq!(v["resources"].as_array().unwrap().len(), 64);
    // Each ceiling has min 1 and a max that is 1 or more, with whole numbers (P 6.1, P 7.1).
    for l in v["limits"].as_array().unwrap() {
        assert_eq!(l["kind"], "ceiling");
        assert_eq!(l["min"], 1);
        assert!(l["max"].as_u64().unwrap() >= 1);
        assert!(l["unit"].as_str().unwrap().len() <= 20);
    }
    // The resources are sorted by id, in byte order.
    let paths: Vec<&str> = v["resources"].as_array().unwrap().iter().map(|r| r["path"].as_str().unwrap()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    assert_eq!(paths, sorted);
    // The tree has the depths 1, 2 and 1, and no cycle.
    let types = v["resource_types"].as_array().unwrap();
    let parent = |n: &str| types.iter().find(|t| t["name"] == n).unwrap()["parent"].as_str().unwrap().to_owned();
    assert_eq!(
        (parent("folder").as_str(), parent("dashboard").as_str(), parent("source").as_str()),
        ("service", "folder", "service")
    );
}

#[test]
fn a_bad_audience_is_an_error_with_a_key_of_the_configuration() {
    // The length of a label is a rule of the key table, so the document accepts a longer label.
    let mut i = inputs("dashboards", vec![source("prod", "Production", 10, 10)]);
    i.audience = String::new();
    let errors = i.check();
    assert!(errors.iter().any(|(k, m)| k == "access.identity.audience" && m.starts_with("expected ")), "{errors:?}");
}

#[test]
fn the_pep_checks_map_to_the_keys_of_13_11() {
    let mut i = inputs("dashboards", vec![source("prod", "Production", 10, 10)]);
    assert!(i.check().is_empty());
    i.pdp_token = procurauth_pep::config::Secret::new("bad token with space");
    i.header = "Authorization".to_owned();
    i.max_age_s = 0;
    let errors = i.check();
    let keys: Vec<&str> = errors.iter().map(|e| e.0.as_str()).collect();
    assert!(keys.contains(&"access.pdp.token"), "{keys:?}");
    assert!(keys.contains(&"access.identity.header"), "{keys:?}");
    assert!(keys.contains(&"access.identity.max_age"), "{keys:?}");
    for (_, message) in &errors {
        assert!(message.starts_with("expected "), "{message}");
        assert!(!message.contains("bad token"), "the value of a secret never shows: {message}");
    }
    // The scheme error is the contradiction C5 of 13.14, so stage 2 does not report it.
    let mut j = inputs("dashboards", vec![source("prod", "Production", 10, 10)]);
    j.pdp_url = "http://pdp.example:4182".to_owned();
    assert!(j.check().is_empty());
    assert!(j.build_pep_config(false).unwrap().check().is_err());
}

#[test]
fn the_pep_config_fields_map_to_their_own_key_or_to_an_internal_error() {
    use super::inputs::config_key_of;
    assert_eq!(config_key_of("max_token_age_seconds"), Some("access.identity.max_age"));
    assert_eq!(config_key_of("clock_skew_seconds"), Some("access.identity.clock_skew"));
    assert_eq!(config_key_of("cache_ttl"), Some("access.pdp.cache_ttl"));
    assert_eq!(config_key_of("pdp.max_connections"), Some("access.pdp.max_connections"));
    // Hune sets these fields from constants: no key of the file gives them.
    for field in ["display_max_questions", "cache_max_entries", "pdp.idle_close_after", "pdp.max_response_bytes"] {
        assert_eq!(config_key_of(field), None, "{field}");
    }
}

#[test]
fn the_values_of_11_10_reach_the_pep_config() {
    let i = inputs("dashboards", vec![source("prod", "Production", 10, 10)]);
    let c = i.build_pep_config(false).unwrap();
    assert_eq!(c.service_id, "dashboards");
    assert_eq!(c.token_header, "X-Identity-Token");
    assert_eq!(c.max_token_age_seconds, 300);
    assert_eq!(c.clock_skew_seconds, 60);
    assert_eq!(c.cache_ttl, Duration::from_secs(30));
    assert_eq!(c.cache_max_entries, 16_384);
    assert_eq!(c.display_max_questions, 200);
    assert_eq!(c.trusted_keys.len(), 2);
    assert_eq!(c.trusted_keys.keys()[0].kid(), Some("edge-2026-11"));
    assert_eq!(c.pdp.request_timeout, Duration::from_millis(500));
    assert_eq!(c.pdp.usage_wait_margin, Duration::from_secs(1));
    assert_eq!(c.pdp.idle_close_after, Duration::from_secs(5));
    assert_eq!(c.pdp.max_connections, 4);
    assert_eq!(c.pdp.evaluations_max_bytes, 32_768);
    assert_eq!(c.pdp.evaluations_max_items, 50);
    assert_eq!(c.pdp.max_response_bytes, 262_144);
    assert_eq!(c.pdp.plain_retries, 1);
    assert_eq!(c.pdp.metadata_max_age, Duration::from_secs(3600));
    assert!(!c.pdp.allow_plain_http);
}

/// The test file of the API suite loads, and its document is the golden file (A45).
#[test]
fn a45_the_document_of_tests_api_authzen_hcl_is_the_golden_file() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    files::put(d.join("jwks.json"), JWKS);
    files::put(d.join("pdp-token"), "pdp-test-token\n");
    files::put(d.join("ch-pw"), "pw\n");
    let text = AUTHZEN_HCL
        .replace("/run/secrets/edge/jwks.json", &d.join("jwks.json").display().to_string())
        .replace("/run/secrets/pdp/token", &d.join("pdp-token").display().to_string())
        .replace("/run/secrets/clickhouse/password", &d.join("ch-pw").display().to_string());
    files::put(d.join("hune.hcl"), text);
    let loaded = crate::config::load(&d.join("hune.hcl"), &crate::config::CapturedEnvironment::default())
        .unwrap_or_else(|e| panic!("{e:?}"));
    // W3: plain_http gives a warning, and nothing else does.
    assert!(loaded.warnings.iter().any(|w| w.key == "access.pdp.plain_http"));
    let i = PepInputs::from_config(&loaded.config).unwrap().expect("mode authzen");
    assert_eq!(i.audience, "hune-test");
    assert_eq!(i.cache_ttl, Duration::from_secs(30));
    assert_eq!(i.sources.len(), 2);
    let got = i.build_document().unwrap().to_value();
    let want: Value = serde_json::from_str(GOLDEN).unwrap();
    assert_eq!(got, want, "compare JSON values, not bytes");
    // The access layer builds from this configuration and sends nothing (11.6).
    let access = Access::from_config(&loaded.config, Logger::memory(LogLevel::Debug), metrics()).unwrap().unwrap();
    assert_eq!(access.pool_size(), 4);
    assert_eq!(access.pep().service_id(), "hune-test");
    let served: Value = serde_json::from_slice(&access.actions_body()).unwrap();
    assert_eq!(served, want);
    // The mode `open` has no access layer.
    let mut open = loaded.config.clone();
    open.access = crate::config::AccessConfig::Open;
    assert!(Access::from_config(&open, Logger::memory(LogLevel::Debug), metrics()).unwrap().is_none());
}

// ---- The transport -------------------------------------------------------------------------------

fn link(plain: bool, ca: Option<&[u8]>, timeout: Duration, max: usize) -> Arc<PdpLink> {
    Arc::new(PdpLink::new(plain, ca, timeout, max, 262_144, Duration::from_secs(5), metrics()).unwrap())
}

fn read_request(stream: &mut impl Read) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        buf.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&buf).to_ascii_lowercase();
    let len: usize = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
        .unwrap_or(0);
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).ok()?;
    buf.extend_from_slice(&body);
    Some(buf)
}

/// A plain server: `script` gets the request text and the number of the connection, and gives the
/// bytes of the answer. `None` closes the connection with no byte.
fn serve_plain<F>(script: F) -> (String, Arc<AtomicUsize>, Arc<std::sync::Mutex<Vec<String>>>)
where
    F: Fn(&str, usize) -> Option<Vec<u8>> + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let accepted = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (a, s) = (accepted.clone(), seen.clone());
    let script = Arc::new(script);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let n = a.fetch_add(1, Ordering::SeqCst);
            let (script, s) = (script.clone(), s.clone());
            thread::spawn(move || {
                while let Some(req) = read_request(&mut stream) {
                    let text = String::from_utf8_lossy(&req).into_owned();
                    s.lock().unwrap().push(text.clone());
                    match script(&text, n) {
                        Some(bytes) => {
                            if stream.write_all(&bytes).is_err() {
                                return;
                            }
                        }
                        None => return,
                    }
                }
            });
        }
    });
    (url, accepted, seen)
}

fn ok_body(body: &str) -> Vec<u8> {
    format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len())
        .into_bytes()
}

#[test]
fn the_transport_sends_the_headers_of_11_12_and_reuses_the_connection() {
    let (url, accepted, seen) = serve_plain(|_, _| Some(ok_body("{\"decision\":true}")));
    let t = link(true, None, Duration::from_secs(2), 2);
    let headers = vec![("Authorization".to_owned(), "Bearer secret-1".to_owned())];
    let info = CallInfo {
        request_id: "req-9".into(),
        traceparent: Some("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".into()),
    };
    let (r, report) = call::with_call(info, || {
        let a = t.post_json(&format!("{url}/access/v1/evaluation"), &headers, b"{\"x\":1}", Duration::from_secs(2));
        let b = t.post_json(&format!("{url}/access/v1/evaluation"), &headers, b"{\"x\":2}", Duration::from_secs(2));
        (a, b)
    });
    assert_eq!(r.0.unwrap().status, 200);
    assert_eq!(r.1.unwrap().body, b"{\"decision\":true}");
    assert_eq!(report.requests, 2);
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "one connection serves two requests");
    assert_eq!(t.open_connections(), 1);
    let first = seen.lock().unwrap()[0].clone();
    let lower = first.to_ascii_lowercase();
    assert!(first.starts_with("POST /access/v1/evaluation HTTP/1.1\r\n"), "{first}");
    assert!(lower.contains("authorization: bearer secret-1\r\n"));
    assert!(lower.contains("content-type: application/json\r\n"));
    assert!(lower.contains("accept: application/json\r\n"));
    assert!(lower.contains("content-length: 7\r\n"));
    assert!(lower.contains("host: 127.0.0.1:"));
    assert!(lower.contains("x-request-id: req-9\r\n"));
    assert!(lower.contains("traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01\r\n"));
    assert!(t.supports_scheme("http") && !t.supports_scheme("https") && !t.supports_scheme("ftp"));
}

#[test]
fn the_transport_reads_chunked_answers_and_does_not_follow_a_redirect() {
    let (url, _, _) = serve_plain(|req, _| {
        if req.starts_with("GET /chunked") {
            Some(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n".to_vec(),
            )
        } else {
            Some(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/elsewhere\r\nContent-Length: 0\r\n\r\n".to_vec())
        }
    });
    let t = link(true, None, Duration::from_secs(2), 2);
    let r = t.get_json(&format!("{url}/chunked"), &[], Duration::from_secs(2)).unwrap();
    assert_eq!(r.body, b"hello world");
    let r = t.get_json(&format!("{url}/moved"), &[], Duration::from_secs(2)).unwrap();
    assert_eq!(r.status, 302, "a status 3xx is an answer with that status");
}

#[test]
fn the_failures_have_the_kinds_of_11_12() {
    // An answer above 262144 bytes: TooLarge.
    let (url, _, _) = serve_plain(|req, _| {
        if req.starts_with("GET /big") {
            Some(format!("HTTP/1.1 200 OK\r\nContent-Length: 262145\r\n\r\n{}", "x".repeat(262_145)).into_bytes())
        } else if req.starts_with("GET /junk") {
            Some(b"SMTP 220 hello\r\n\r\n".to_vec())
        } else if req.starts_with("GET /close") {
            None
        } else {
            std::thread::sleep(Duration::from_millis(600));
            Some(ok_body("{}"))
        }
    });
    let t = link(true, None, Duration::from_secs(2), 2);
    assert_eq!(t.get_json(&format!("{url}/big"), &[], Duration::from_secs(2)).unwrap_err(), TransportError::TooLarge);
    assert!(matches!(
        t.get_json(&format!("{url}/junk"), &[], Duration::from_secs(2)).unwrap_err(),
        TransportError::Malformed(_)
    ));
    // The connection fails after the first byte of the request: Lost.
    assert!(matches!(
        t.get_json(&format!("{url}/close"), &[], Duration::from_secs(2)).unwrap_err(),
        TransportError::Lost(_)
    ));
    // The deadline ends after the first byte of the request: Timeout.
    assert_eq!(
        t.get_json(&format!("{url}/slow"), &[], Duration::from_millis(150)).unwrap_err(),
        TransportError::Timeout
    );
    // No TCP connection: NotSent.
    let dead = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_url = format!("http://{}", dead.local_addr().unwrap());
    drop(dead);
    assert!(matches!(
        t.get_json(&format!("{dead_url}/x"), &[], Duration::from_secs(1)).unwrap_err(),
        TransportError::NotSent(_)
    ));
    // A scheme that the link does not support: Unsupported. An http URL needs `plain_http`.
    let strict = link(false, Some(TLS_CA.as_bytes()), Duration::from_secs(1), 1);
    assert!(matches!(
        strict.get_json(&format!("{url}/x"), &[], Duration::from_secs(1)).unwrap_err(),
        TransportError::Unsupported(_)
    ));
    assert!(strict.supports_scheme("https") && !strict.supports_scheme("http"));
    // A TLS link needs the CA certificates.
    assert!(PdpLink::new(false, None, Duration::from_secs(1), 1, 1, Duration::from_secs(1), metrics()).is_err());
}

#[test]
fn the_pool_never_opens_more_connections_than_its_bound() {
    let (url, accepted, _) = serve_plain(|_, _| {
        std::thread::sleep(Duration::from_millis(80));
        Some(ok_body("{}"))
    });
    let t = link(true, None, Duration::from_secs(3), 2);
    let mut handles = Vec::new();
    for _ in 0..6 {
        let (t, url) = (t.clone(), url.clone());
        handles
            .push(thread::spawn(move || t.get_json(&format!("{url}/x"), &[], Duration::from_secs(3)).unwrap().status));
    }
    for h in handles {
        assert_eq!(h.join().unwrap(), 200);
    }
    assert!(accepted.load(Ordering::SeqCst) <= 2, "{} connections", accepted.load(Ordering::SeqCst));
    assert!(t.open_connections() <= 2);
}

fn tls_server(delay_close: bool) -> (u16, Arc<AtomicUsize>) {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    let certs: Vec<CertificateDer<'static>> =
        CertificateDer::pem_slice_iter(TLS_CERT.as_bytes()).map(|c| c.unwrap()).collect();
    let key = PrivateKeyDer::from_pem_slice(TLS_KEY.as_bytes()).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = Arc::new(
        rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let a = accepted.clone();
    thread::spawn(move || {
        for tcp in listener.incoming() {
            let Ok(tcp) = tcp else { return };
            a.fetch_add(1, Ordering::SeqCst);
            let config = config.clone();
            thread::spawn(move || {
                let conn = rustls::ServerConnection::new(config).unwrap();
                let mut stream = rustls::StreamOwned::new(conn, tcp);
                while read_request(&mut stream).is_some() {
                    if stream.write_all(&ok_body("{\"decision\":true}")).is_err() || stream.flush().is_err() {
                        return;
                    }
                    if delay_close {
                        return;
                    }
                }
            });
        }
    });
    (port, accepted)
}

#[test]
fn an_https_link_checks_the_certificate_against_the_ca_of_the_file_only() {
    let (port, accepted) = tls_server(false);
    let url = format!("https://localhost:{port}/access/v1/evaluation");
    let t = link(false, Some(TLS_CA.as_bytes()), Duration::from_secs(3), 2);
    let r = t.post_json(&url, &[], b"{}", Duration::from_secs(3)).expect("a good certificate");
    assert_eq!((r.status, r.body.as_slice()), (200, b"{\"decision\":true}".as_slice()));
    let r = t.post_json(&url, &[], b"{}", Duration::from_secs(3)).unwrap();
    assert_eq!(r.status, 200);
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "the TLS connection is reused");
    // The host is the server name: the certificate holds `localhost` and `127.0.0.1`.
    let by_ip = format!("https://127.0.0.1:{port}/x");
    assert_eq!(t.post_json(&by_ip, &[], b"{}", Duration::from_secs(3)).unwrap().status, 200);
    // Another CA: the handshake fails before the first byte of the request (NotSent).
    let other = include_str!("../../../../tests/fixtures/config/ca-a.pem");
    let wrong = link(false, Some(other.as_bytes()), Duration::from_secs(3), 2);
    assert!(matches!(
        wrong.post_json(&url, &[], b"{}", Duration::from_secs(3)).unwrap_err(),
        TransportError::NotSent(_)
    ));
    // A plain port that speaks no TLS: NotSent too.
    let (plain_url, _, _) = serve_plain(|_, _| Some(ok_body("{}")));
    let https_to_plain = plain_url.replace("http://", "https://");
    assert!(t.get_json(&https_to_plain, &[], Duration::from_millis(800)).is_err());
}

#[test]
fn the_text_form_of_a_chain_is_the_form_of_11_16() {
    let t = Tree::new("dashboards");
    assert_eq!(node_text(&t.dashboard("transit", "score")), "service:dashboards/folder:transit/dashboard:score");
}

#[test]
fn a_file_with_no_primary_store_publishes_neither_write_nor_source_query() {
    let mut i = inputs("hune", Vec::new());
    i.history = false;
    i.writable = false;
    let doc = i.build_document().expect("the document builds").to_value();
    let json = doc.to_string();
    assert!(json.contains("dashboard.read") && json.contains("dashboard.export"), "{json}");
    assert!(!json.contains("dashboard.write") && !json.contains("source.query"), "{json}");
}
