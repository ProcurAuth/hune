// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of TLS (8.6): the configured CA is the only trust root, the name is checked.

mod common;

use std::sync::Arc;

use common::*;
use hune_source::{ClickHouseSource, ResultMemoryBudget, SourceState, SourceTls};
use tokio_rustls::TlsAcceptor;

const CA: &[u8] = include_bytes!("fixtures/tls/ca.pem");
const OTHER_CA: &[u8] = include_bytes!("fixtures/tls/other-ca.pem");
const CERT: &[u8] = include_bytes!("fixtures/tls/server.pem");
const KEY: &[u8] = include_bytes!("fixtures/tls/server.key");

fn acceptor(versions: &[&'static rustls::SupportedProtocolVersion]) -> TlsAcceptor {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(CERT).map(|c| c.unwrap()).collect();
    let key = PrivateKeyDer::from_pem_slice(KEY).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(versions)
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    TlsAcceptor::from(Arc::new(cfg))
}

fn healthy() -> Handler {
    handler(|rec| {
        probe_answer(rec, "26.8.1.1", "2", &["GRANT SELECT ON analytics.* TO hune"])
            .unwrap_or_else(|| ok_steps(rec, ONE_COLUMN))
    })
}

fn tls_config(addr: std::net::SocketAddr, ca: &[u8], name: &str) -> hune_source::ClickHouseSourceConfig {
    let mut cfg = config(addr);
    cfg.https = true;
    cfg.tls = Some(SourceTls { ca_pem: ca.to_vec(), server_name: name.to_owned() });
    cfg
}

#[tokio::test]
async fn the_test_ca_and_the_right_name_succeed() {
    let server = FakeServer::start_tls(healthy(), acceptor(&[&rustls::version::TLS13, &rustls::version::TLS12])).await;
    let src = source(tls_config(server.addr, CA, "clickhouse-test"));
    let f = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap();
    assert_eq!(f.rows, 3);
    assert_eq!(src.state(), SourceState::Up);
    // The name of the configuration goes in SNI, not the address.
    assert_eq!(server.snis.lock().unwrap()[0].as_deref(), Some("clickhouse-test"));
    // The connection of the probe was used again over TLS.
    assert_eq!(server.connections(), 1);
}

#[tokio::test]
async fn an_ip_address_as_name_is_checked_against_the_san() {
    let server = FakeServer::start_tls(healthy(), acceptor(&[&rustls::version::TLS13])).await;
    let src = source(tls_config(server.addr, CA, "127.0.0.1"));
    assert_eq!(src.probe().await.state, SourceState::Up);
}

#[tokio::test]
async fn tls_12_alone_works() {
    let server = FakeServer::start_tls(healthy(), acceptor(&[&rustls::version::TLS12])).await;
    let src = source(tls_config(server.addr, CA, "clickhouse-test"));
    assert_eq!(src.probe().await.state, SourceState::Up);
}

#[tokio::test]
async fn a_wrong_name_stops_the_connection() {
    let server = FakeServer::start_tls(healthy(), acceptor(&[&rustls::version::TLS13])).await;
    let src = source(tls_config(server.addr, CA, "wrong-name"));
    let e = src.query(sql("SELECT 1"), limits(), ctx()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_misconfigured", 502, false));
    assert!(e.message().contains("certificate"), "{}", e.message());
    assert_eq!(src.state(), SourceState::Misconfigured);
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn another_ca_stops_the_connection_and_no_other_root_is_tried() {
    let server = FakeServer::start_tls(healthy(), acceptor(&[&rustls::version::TLS13])).await;
    let src = source(tls_config(server.addr, OTHER_CA, "clickhouse-test"));
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Misconfigured);
    assert!(p.detail.unwrap().contains("certificate"));
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn a_plain_client_on_a_tls_port_is_unavailable_not_misconfigured() {
    let server = FakeServer::start_tls(healthy(), acceptor(&[&rustls::version::TLS13])).await;
    let src = source(config(server.addr));
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Down);
}

#[tokio::test]
async fn a_server_that_does_not_speak_tls_is_unavailable_after_the_connect_timeout() {
    // A plain server waits for an HTTP request and never answers the TLS hello.
    let server = FakeServer::start(handler(|_| vec![Step::Close])).await;
    let mut cfg = tls_config(server.addr, CA, "clickhouse-test");
    cfg.connect_timeout = std::time::Duration::from_millis(300);
    let src = source(cfg);
    let t = std::time::Instant::now();
    let p = src.probe().await;
    assert_eq!(p.state, SourceState::Down);
    assert!(p.detail.unwrap().contains("source_unavailable"));
    assert!(t.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn setup_errors() {
    let b = || ResultMemoryBudget::new(1 << 30);
    let addr: std::net::SocketAddr = "127.0.0.1:8443".parse().unwrap();
    // https without a tls block: no system store is ever used.
    let mut cfg = config(addr);
    cfg.https = true;
    let e = ClickHouseSource::new(cfg, b()).unwrap_err();
    assert!(e.message().contains("tls"), "{e}");
    // http with a tls block.
    let mut cfg = config(addr);
    cfg.tls = Some(SourceTls { ca_pem: CA.to_vec(), server_name: "x".into() });
    assert!(ClickHouseSource::new(cfg, b()).is_err());
    // A PEM text with no certificate, and a broken one.
    for pem in [&b""[..], b"hello", b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n"] {
        let e = ClickHouseSource::new(tls_config(addr, pem, "x"), b()).unwrap_err();
        assert!(e.message().starts_with("tls.ca_file"), "{e}");
    }
    // A name that is not a DNS name.
    let e = ClickHouseSource::new(tls_config(addr, CA, "not a name!"), b()).unwrap_err();
    assert!(e.message().contains("server_name"), "{e}");
    // A good one.
    assert!(ClickHouseSource::new(tls_config(addr, CA, "clickhouse-test"), b()).is_ok());
}

#[test]
fn other_setup_errors() {
    let b = || ResultMemoryBudget::new(1 << 30);
    let addr: std::net::SocketAddr = "127.0.0.1:8123".parse().unwrap();
    type Mutation = Box<dyn Fn(&mut hune_source::ClickHouseSourceConfig)>;
    let cases: Vec<(Mutation, &str)> = vec![
        (Box::new(|c| c.host.clear()), "host"),
        (Box::new(|c| c.port = 0), "port"),
        (Box::new(|c| c.user.clear()), "user"),
        (Box::new(|c| c.user = "a\nb".into()), "user"),
        (Box::new(|c| c.password = "a\r\nX: y".into()), "password"),
        (Box::new(|c| c.max_concurrent = 0), "max_concurrent"),
        (Box::new(|c| c.max_concurrent_per_subject = Some(0)), "max_concurrent_per_subject"),
        (Box::new(|c| c.max_result_rows = 0), "max_result_rows"),
        (Box::new(|c| c.max_result_bytes = 0), "max_result_bytes"),
        (Box::new(|c| c.max_result_bytes = 1 << 32), "max_result_bytes"),
        (Box::new(|c| c.max_query_size = 0), "max_query_size"),
        (Box::new(|c| c.max_execution_time = std::time::Duration::ZERO), "max_execution_time"),
        (Box::new(|c| c.connect_timeout = std::time::Duration::ZERO), "connect_timeout"),
    ];
    for (mutate, key) in cases {
        let mut cfg = config(addr);
        mutate(&mut cfg);
        let e = ClickHouseSource::new(cfg, b()).unwrap_err();
        assert!(e.message().starts_with(key), "{key}: {e}");
        assert!(!e.message().contains("p4ss-secret"));
    }
}
