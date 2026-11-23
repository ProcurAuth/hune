// SPDX-License-Identifier: MIT OR Apache-2.0
//! Helpers of the PostgreSQL tests (5.36): the connection data from the environment, the
//! factories of backends and a TCP proxy that breaks connections.
//!
//! The tests need a PostgreSQL server. They are `#[ignore]`: `tests/ci/store-pg.sh` starts
//! PostgreSQL 15 and 18 in containers, sets the variables below and runs the ignored tests.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use hune_store::AnyStore;
use hune_store::conformance::pg::{PgBackend, PgTestConfig};
use hune_store::conformance::{Backend, Fixture, Hooks};
use hune_store::volume::testing::VolumeHooks;
use hune_store::volume::{Faults, VolumeOptions, VolumeStore};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinHandle;

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!("the variable {name} is not set: run the PostgreSQL tests with tests/ci/store-pg.sh")
    })
}

/// The connection data of the server, from the environment of the test run.
pub fn config() -> PgTestConfig {
    PgTestConfig {
        host: var("HUNE_TEST_PG_HOST"),
        port: var("HUNE_TEST_PG_PORT").parse().expect("HUNE_TEST_PG_PORT"),
        admin_user: var("HUNE_TEST_PG_ADMIN_USER"),
        admin_password: var("HUNE_TEST_PG_ADMIN_PASSWORD"),
        owner_user: var("HUNE_TEST_PG_OWNER_USER"),
        owner_password: var("HUNE_TEST_PG_OWNER_PASSWORD"),
        app_user: var("HUNE_TEST_PG_APP_USER"),
        app_password: var("HUNE_TEST_PG_APP_PASSWORD"),
    }
}

/// The PEM bytes of the CA that signed the certificate of the server.
pub fn ca_pem() -> Vec<u8> {
    std::fs::read(var("HUNE_TEST_PG_CA_FILE")).expect("HUNE_TEST_PG_CA_FILE")
}

/// The PEM bytes of a CA that did not sign the certificate of the server.
pub fn wrong_ca_pem() -> Vec<u8> {
    std::fs::read(var("HUNE_TEST_PG_WRONG_CA_FILE")).expect("HUNE_TEST_PG_WRONG_CA_FILE")
}

/// The name that the certificate of the server holds.
pub fn server_name() -> String {
    var("HUNE_TEST_PG_SERVER_NAME")
}

/// The one factory of the test binary. Its databases start with `vt<pid>`.
pub fn backend() -> &'static PgBackend {
    static B: OnceLock<PgBackend> = OnceLock::new();
    B.get_or_init(|| PgBackend::new(config(), &format!("vt{}", std::process::id())))
}

/// The factory of volume stores in temporary directories.
pub struct VolumeBackend {
    dirs: Mutex<Vec<tempfile::TempDir>>,
}

impl VolumeBackend {
    pub fn new() -> Self {
        VolumeBackend { dirs: Mutex::new(Vec::new()) }
    }
}

impl Backend for VolumeBackend {
    async fn new_store(&self, history_keep: u32) -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        self.dirs.lock().unwrap().push(dir);
        let faults = Faults::new();
        let opts = VolumeOptions {
            root: root.clone(),
            read_only: false,
            rescan_interval: std::time::Duration::from_secs(1),
            history_keep,
        };
        let store = VolumeStore::open_with_faults(opts, faults.clone()).await.expect("open");
        Fixture { store: AnyStore::Volume(store), hooks: Hooks::Volume(VolumeHooks::new(&root, faults)) }
    }
}

/// A scratch directory with an absolute canonical path.
pub fn scratch() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = std::fs::canonicalize(dir.path()).expect("canonical path");
    (dir, path)
}

pub fn is_dir(p: &Path) -> bool {
    p.is_dir()
}

/// A TCP proxy to the server. `cut` closes each open connection. `stop` closes the listener too.
pub struct Proxy {
    port: u16,
    cut: watch::Sender<u64>,
    task: JoinHandle<()>,
}

impl Proxy {
    /// Starts a proxy on 127.0.0.1. `listen_port` 0 takes a free port.
    pub async fn start(target_host: &str, target_port: u16, listen_port: u16) -> Proxy {
        let listener = TcpListener::bind(("127.0.0.1", listen_port)).await.expect("bind the proxy");
        let port = listener.local_addr().expect("proxy address").port();
        let target: SocketAddr = format!("{target_host}:{target_port}").parse().expect("target address");
        let (cut, _) = watch::channel(0u64);
        let tx = cut.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut inbound, _)) = listener.accept().await else { return };
                let mut rx = tx.subscribe();
                tokio::spawn(async move {
                    let Ok(mut outbound) = TcpStream::connect(target).await else { return };
                    tokio::select! {
                        _ = copy_bidirectional(&mut inbound, &mut outbound) => {}
                        _ = rx.changed() => {}
                    }
                });
            }
        });
        Proxy { port, cut, task }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Closes each open connection. New connections work.
    pub fn cut(&self) {
        self.cut.send_modify(|n| *n += 1);
    }

    /// Closes the listener and each open connection.
    pub fn stop(self) {
        self.task.abort();
        self.cut();
    }
}

/// A free port on 127.0.0.1 (the port is free when the function returns).
pub fn free_port() -> u16 {
    let l = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    l.local_addr().expect("address").port()
}
