// SPDX-License-Identifier: MIT OR Apache-2.0
//! The trust of the MongoDB driver (8b.4, case MS10): the CA file of the block is the only trust root.
//!
//! The tests need no MongoDB server. A server that only speaks TLS stands in: it accepts a connection, ends the
//! handshake with a test certificate, and closes. The driver then fails with a TLS error or, when the handshake
//! works, with a link error after the handshake. The files of the certificates are in `tests/fixtures/tls`.
//!
//! How the driver loads its roots (the crate `mongodb` 3.10.0, read in `~/.cargo/registry`):
//! `src/runtime/tls_rustls.rs`, function `make_rustls_config`. When `ca_file_path` is set, the store holds the
//! certificates of this file and nothing else. The roots of the crate `webpki-roots` load only in the branch where
//! `ca_file_path` is `None`. The driver module of Hune never builds options with `ca_file_path` unset (it refuses
//! a `verify-full` block with no CA file), and the test below shows that a certificate of another CA fails.
#![cfg(feature = "mongo-driver")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hune_core::ids::SourceId;
use hune_core::mongo::ReadPreference;
use hune_source::{
    CERTIFICATE_CODE_NAME, ConnectOptions, MongoClient, MongoDriver, MongoErrorClass, MongoSourceConfig, MongoTlsMode,
    MongoTopology, SourceTls,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

const CA: &[u8] = include_bytes!("fixtures/tls/ca.pem");
const OTHER_CA: &[u8] = include_bytes!("fixtures/tls/other-ca.pem");
const CERT: &[u8] = include_bytes!("fixtures/tls/server.pem");
const KEY: &[u8] = include_bytes!("fixtures/tls/server.key");

fn acceptor() -> TlsAcceptor {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(CERT).map(|c| c.unwrap()).collect();
    let key = PrivateKeyDer::from_pem_slice(KEY).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    TlsAcceptor::from(Arc::new(config))
}

/// A TLS server that ends the handshake and closes the connection. It gives the port.
async fn tls_stub() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let acceptor = acceptor();
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else { return };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                // The handshake may fail: the client closes when it refuses the certificate.
                drop(acceptor.accept(socket).await);
            });
        }
    });
    port
}

/// Writes a CA file in a folder of this test and gives its path.
fn ca_file(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hune-mongo-tls-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn options(host: &str, port: u16, ca: &[u8], name: &str) -> ConnectOptions {
    let path = ca_file(name, ca);
    let config = MongoSourceConfig {
        id: SourceId::parse("tls").unwrap(),
        label: "Test".to_owned(),
        topology: MongoTopology::Direct { host: host.to_owned(), port },
        user: "u".to_owned(),
        password: "p".to_owned(),
        auth_database: "admin".to_owned(),
        databases: vec!["d".to_owned()],
        collections: vec!["c".to_owned()],
        read_preference: ReadPreference::Primary,
        tls_mode: MongoTlsMode::VerifyFull,
        tls: Some(SourceTls { ca_pem: ca.to_vec(), server_name: String::new() }),
        tls_ca_file: Some(path),
        connect_timeout: Duration::from_secs(3),
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 100,
        max_result_bytes: 1 << 20,
        max_query_size: 1 << 20,
        max_concurrent: 2,
        queue_wait: Duration::from_secs(1),
        max_concurrent_per_subject: None,
    };
    ConnectOptions::from_config(&config)
}

/// MS10: a server certificate that another CA signed fails, although the test CA is in the file of another block.
/// The client trusts the CA file only.
#[tokio::test]
async fn ms10_a_certificate_of_another_ca_fails_the_check() {
    let port = tls_stub().await;
    let e = MongoDriver::connect(options("127.0.0.1", port, OTHER_CA, "other-ca.pem")).await.map(|_| ()).unwrap_err();
    assert_eq!((e.class, e.code_name.as_deref()), (MongoErrorClass::Tls, Some(CERTIFICATE_CODE_NAME)), "{e:?}");
}

/// MS10: with the right CA, the handshake passes. The error then comes from the protocol after the handshake (the
/// stub closes the link), and it is not a TLS error. This shows that the failure above is the check of the
/// certificate and not another fault.
#[tokio::test]
async fn ms10_the_right_ca_passes_the_handshake() {
    let port = tls_stub().await;
    let e = MongoDriver::connect(options("127.0.0.1", port, CA, "ca.pem")).await.map(|_| ()).unwrap_err();
    assert_ne!(e.class, MongoErrorClass::Tls, "{e:?}");
}

/// MS10: the name check. The certificate holds `clickhouse-test` and `127.0.0.1`. A connection to `localhost` has a
/// name that the certificate lacks, so it fails, with the right CA.
#[tokio::test]
async fn ms10_a_name_that_the_certificate_lacks_fails_the_check() {
    let port = tls_stub().await;
    let e = MongoDriver::connect(options("localhost", port, CA, "ca2.pem")).await.map(|_| ()).unwrap_err();
    assert_eq!((e.class, e.code_name.as_deref()), (MongoErrorClass::Tls, Some(CERTIFICATE_CODE_NAME)), "{e:?}");
}

/// MS10: the CA file is the one that the start read. A file that changed gives the same refusal, and no
/// connection is tried.
#[tokio::test]
async fn ms10_a_ca_file_that_changed_is_refused_before_any_connection() {
    let port = tls_stub().await;
    let mut o = options("127.0.0.1", port, CA, "ca3.pem");
    std::fs::write(o.tls_ca_file.as_ref().unwrap(), OTHER_CA).unwrap();
    let e = MongoDriver::connect(o.clone()).await.map(|_| ()).unwrap_err();
    assert_eq!((e.class, e.code_name.as_deref()), (MongoErrorClass::Tls, Some(CERTIFICATE_CODE_NAME)), "{e:?}");
    // No file: the same.
    o.tls_ca_file = None;
    let e = MongoDriver::connect(o).await.map(|_| ()).unwrap_err();
    assert_eq!(e.class, MongoErrorClass::Tls);
}
