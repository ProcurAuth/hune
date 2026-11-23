//! The `hello` of the MongoDB driver sends no value of the process environment (13.2 rule 5, 8b.15, ID-MG25).
//!
//! The crate `mongodb` 3.10.0 of the registry puts the name of a FaaS platform and values of the variables
//! `AWS_REGION`, `AWS_EXECUTION_ENV`, `K_SERVICE`, `VERCEL`, `FUNCTIONS_WORKER_RUNTIME` and
//! `KUBERNETES_SERVICE_HOST` in the key `client.env` of the `hello` command. Hune uses a local copy of the
//! crate (`vendor/mongodb`) with no environment read. These tests start the driver in a child process with these
//! variables set, capture the `hello` on the server side, and show that the key `client.env` is absent and that
//! no value of a variable is in the message.
//!
//! - The tests `hello_*` need no MongoDB server. A stub server records the first message of each connection.
//! - The test `hello_to_a_real_server_*` is `#[ignore]` and runs with the container tests (`tests/ci/mongosource.sh`). The TLS proxy
//!   of `tests/mgs` records the `hello` between the driver and a real server.
//!
//! A test cannot set a variable in its own process (`set_var` is `unsafe`, and the crate forbids `unsafe`).
//! The parent test starts the test binary again, with the variables set, and runs one `child_*` test in it. A
//! `child_*` test does nothing when the variable `HUNE_TEST_HELLO_CHILD` is not set. Tests are not product code:
//! they can read the environment and start a process.
#![cfg(feature = "mongo-driver")]

mod mgs;

use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use bson::RawDocument;
use hune_core::ids::SourceId;
use hune_core::mongo::ReadPreference;
use hune_source::{ConnectOptions, MongoClient, MongoDriver, MongoSourceConfig, MongoTlsMode, MongoTopology};
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

/// The marker of each value that a test sets. No byte of the `hello` can hold it.
const MARKER: &str = "hune-marker";

/// The variables that the driver of the registry reads (`src/cmap/establish/handshake.rs`), with a value for each.
const ALL: [(&str, &str); 14] = [
    ("AWS_EXECUTION_ENV", "AWS_Lambda_hune-marker"),
    ("AWS_LAMBDA_RUNTIME_API", "hune-marker-api"),
    ("AWS_REGION", "hune-marker-region"),
    ("AWS_LAMBDA_FUNCTION_MEMORY_SIZE", "777"),
    ("FUNCTIONS_WORKER_RUNTIME", "hune-marker-azure"),
    ("K_SERVICE", "hune-marker-ksvc"),
    ("FUNCTION_NAME", "hune-marker-fn"),
    ("FUNCTION_MEMORY_MB", "888"),
    ("FUNCTION_TIMEOUT_SEC", "99"),
    ("FUNCTION_REGION", "hune-marker-gcp"),
    ("VERCEL", "hune-marker-vercel"),
    ("VERCEL_REGION", "hune-marker-vregion"),
    ("KUBERNETES_SERVICE_HOST", "10.9.8.7"),
    ("OTEL_RUST_INSTRUMENTATION_MONGODB_ENABLED", "true"),
];

/// The groups of variables of the scenarios. Each group makes the original driver send a different `client.env`.
fn scenarios() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    let pick = |names: &[&str]| -> Vec<(&'static str, &'static str)> {
        ALL.iter().filter(|(k, _)| names.contains(k)).copied().collect()
    };
    vec![
        ("all of them", ALL.to_vec()),
        ("aws lambda", pick(&["AWS_EXECUTION_ENV", "AWS_REGION", "AWS_LAMBDA_FUNCTION_MEMORY_SIZE"])),
        ("azure functions", pick(&["FUNCTIONS_WORKER_RUNTIME"])),
        ("gcp functions", pick(&["K_SERVICE", "FUNCTION_MEMORY_MB", "FUNCTION_TIMEOUT_SEC", "FUNCTION_REGION"])),
        ("vercel", pick(&["VERCEL", "VERCEL_REGION"])),
        ("kubernetes", pick(&["KUBERNETES_SERVICE_HOST"])),
    ]
}

/// Starts this test binary again with `vars` set and runs the test `child` in it. The child must pass.
fn run_child(child: &str, scenario: &str, vars: &[(&str, &str)]) {
    let exe = std::env::current_exe().expect("the path of the test binary");
    let names: Vec<&str> = vars.iter().map(|(k, _)| *k).collect();
    let mut command = Command::new(exe);
    command.args([child, "--exact", "--nocapture", "--test-threads=1"]);
    for (k, v) in vars {
        command.env(k, v);
    }
    command.env("HUNE_TEST_HELLO_CHILD", names.join(","));
    let out = command.output().expect("the child process starts");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stdout.contains("1 passed"),
        "scenario {scenario}: the child test {child} failed\n--- stdout\n{stdout}\n--- stderr\n{stderr}"
    );
}

/// In a child, gives the names of the variables that the parent set, and checks that they are set.
fn child_variables() -> Option<Vec<String>> {
    let list = std::env::var("HUNE_TEST_HELLO_CHILD").ok()?;
    let names: Vec<String> = list.split(',').filter(|n| !n.is_empty()).map(str::to_owned).collect();
    for n in &names {
        let v = std::env::var(n).unwrap_or_else(|_| panic!("the child does not have {n}"));
        assert!(!v.is_empty(), "{n} is empty in the child");
    }
    assert!(!names.is_empty(), "the child has no variable to test");
    Some(names)
}

/// Checks one `hello` document: it holds no value of the environment, and no key `env` or `container` in `client`.
/// Gives true when the document holds the metadata of the driver. The `hello` of the handshake holds it. The
/// `hello` that the probe sends later and the `hello` of the monitor do not.
fn assert_no_platform_value(raw: &[u8], names: &[String]) -> bool {
    let doc = RawDocument::from_bytes(raw).expect("the hello is a BSON document");
    // No value of the environment is in the bytes of the message.
    let text = String::from_utf8_lossy(raw);
    assert!(!text.contains(MARKER), "a marker value is in the hello");
    for n in names {
        let value = std::env::var(n).expect("the variable is set");
        // A short value such as "1" or "99" can be in the bytes by chance: test the values with 5 characters or more.
        if value.len() >= 5 {
            assert!(!text.contains(&value), "the value of {n} is in the hello");
        }
        assert!(!text.contains(n.as_str()), "the name {n} is in the hello");
    }
    let Some(client) = doc.get("client").expect("the hello reads") else { return false };
    let client = client.as_document().expect("client is a document").to_owned();
    // The capture is the metadata of the driver: its name, its operating system and its platform are there.
    assert_eq!(client.get_document("driver").expect("driver").get_str("name").expect("name"), "mongo-rust-driver");
    assert!(client.get_document("os").is_ok(), "client.os is missing");
    assert!(client.get_str("platform").is_ok(), "client.platform is missing");
    // The key `env` carries the platform. It must be absent, and so must any key that a platform adds.
    let keys: Vec<String> = client.iter().map(|e| e.expect("element").0.as_str().to_owned()).collect();
    assert!(!keys.iter().any(|k| k == "env"), "client has the key env: {keys:?}");
    assert!(!keys.iter().any(|k| k == "container"), "client has the key container: {keys:?}");
    true
}

fn config(port: u16) -> MongoSourceConfig {
    MongoSourceConfig {
        id: SourceId::parse("hello").unwrap(),
        label: "Test".to_owned(),
        topology: MongoTopology::Direct { host: "127.0.0.1".to_owned(), port },
        user: "u".to_owned(),
        password: "p".to_owned(),
        auth_database: "admin".to_owned(),
        databases: vec!["d".to_owned()],
        collections: vec!["c".to_owned()],
        read_preference: ReadPreference::Primary,
        tls_mode: MongoTlsMode::Disable,
        tls: None,
        tls_ca_file: None,
        connect_timeout: Duration::from_secs(1),
        max_execution_time: Duration::from_secs(30),
        max_result_rows: 100,
        max_result_bytes: 1 << 20,
        max_query_size: 1 << 20,
        max_concurrent: 2,
        queue_wait: Duration::from_secs(1),
        max_concurrent_per_subject: None,
    }
}

/// A plain server that records the command document of the first message of each connection and closes.
async fn stub() -> (u16, Arc<Mutex<Vec<Vec<u8>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let record = Arc::clone(&record);
            tokio::spawn(async move {
                let mut header = [0_u8; 16];
                if socket.read_exact(&mut header).await.is_err() {
                    return;
                }
                let len =
                    usize::try_from(i32::from_le_bytes([header[0], header[1], header[2], header[3]])).unwrap_or(0);
                let opcode = i32::from_le_bytes([header[12], header[13], header[14], header[15]]);
                if !(16..=1 << 20).contains(&len) {
                    return;
                }
                let mut body = vec![0_u8; len - 16];
                if socket.read_exact(&mut body).await.is_err() {
                    return;
                }
                if let Some(doc) = mgs::command_document(opcode, &body) {
                    record.lock().unwrap_or_else(PoisonError::into_inner).push(doc.to_vec());
                }
                // The connection closes: the driver fails, and the test has the messages.
            });
        }
    });
    (port, seen)
}

/// Child of `hello_with_platform_variables_sends_none`: the driver connects to the stub, and the stub shows the
/// `hello`.
#[tokio::test]
async fn child_stub_hello() {
    let Some(names) = child_variables() else { return };
    let (port, seen) = stub().await;
    // The connection fails (the stub closes it). The call returns after the connect timeout of one second.
    let _ = MongoDriver::connect(ConnectOptions::from_config(&config(port))).await.map(|_| ());
    let docs = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert!(!docs.is_empty(), "the stub did not see a message");
    let hellos: Vec<&Vec<u8>> = docs
        .iter()
        .filter(|d| {
            let name = d.get(5..).and_then(|k| k.iter().position(|b| *b == 0).map(|e| &k[..e])).unwrap_or(&[]);
            matches!(name, b"hello" | b"isMaster" | b"ismaster")
        })
        .collect();
    assert!(!hellos.is_empty(), "the stub did not see a hello");
    let with_client = hellos.into_iter().filter(|h| assert_no_platform_value(h, &names)).count();
    assert!(with_client > 0, "no hello holds the metadata of the driver");
}

/// ID-MG25: with the variables of each platform set, the `hello` that a stub server receives holds no value of them.
#[test]
fn hello_with_platform_variables_sends_none() {
    for (scenario, vars) in scenarios() {
        run_child("child_stub_hello", scenario, &vars);
    }
}

/// Child of `hello_to_a_real_server_sends_no_platform_value`.
#[tokio::test]
async fn child_container_hello() {
    let Some(names) = child_variables() else { return };
    let proxy = mgs::Proxy::start(mgs::port()).await;
    let mut config = mgs::ro_config();
    config.topology = proxy.topology();
    let source = mgs::make_source(config);
    let probe = source.probe().await;
    assert_eq!(probe.state, hune_source::SourceState::Up, "{probe:?}");
    let docs = proxy.hello_documents();
    assert!(!docs.is_empty(), "the proxy did not see a hello");
    let with_client = docs.iter().filter(|d| assert_no_platform_value(d, &names)).count();
    assert!(with_client > 0, "no hello holds the metadata of the driver");
}

/// ID-MG25: the same with a real MongoDB server: the proxy of the container tests records the `hello` between the
/// driver and the server, with the variables of every platform set.
#[test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
fn hello_to_a_real_server_sends_no_platform_value() {
    let (scenario, vars) = scenarios().into_iter().next().expect("a scenario");
    run_child("child_container_hello", scenario, &vars);
}
