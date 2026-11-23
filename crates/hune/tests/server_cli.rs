// SPDX-License-Identifier: MIT OR Apache-2.0
//! Black-box test of `hune serve` and `hune health`: the real binary, real sockets, a real SIGTERM.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/config");
const DASHBOARD: &str = include_str!("../../../tests/fixtures/dashboards/nodes.json");

/// Owns a child process: the drop kills it and waits for it, also when the test fails.
struct ChildGuard(Option<std::process::Child>);

impl ChildGuard {
    fn id(&self) -> u32 {
        self.0.as_ref().map_or(0, std::process::Child::id)
    }

    /// Waits for the end of the process and gives its output.
    fn finish(mut self) -> std::process::Output {
        self.0.take().expect("the child runs").wait_with_output().unwrap()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn http(addr: SocketAddr, method: &str, path: &str) -> Option<(u16, String)> {
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    s.write_all(format!("{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").as_bytes()).ok()?;
    let mut text = String::new();
    s.read_to_string(&mut text).ok()?;
    let status = text.split(' ').nth(1)?.parse().ok()?;
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_owned()).unwrap_or_default();
    Some((status, body))
}

fn write_config(dir: &Path, listen: u16, ops: u16) -> std::path::PathBuf {
    std::fs::write(dir.join("pw"), "chpass-marker\n").unwrap();
    std::fs::copy(Path::new(FIXTURES).join("ca-a.pem"), dir.join("ca.pem")).unwrap();
    let volume = dir.join("volume");
    std::fs::create_dir_all(volume.join("transit")).unwrap();
    std::fs::write(
        volume.join("transit/_folder.json"),
        "{\n  \"format_version\": 1,\n  \"title\": \"Transit desk\"\n}\n",
    )
    .unwrap();
    std::fs::write(volume.join("transit/nodes.json"), DASHBOARD).unwrap();
    let text = std::fs::read_to_string(Path::new(FIXTURES).join("local.hcl"))
        .unwrap()
        .replace("/run/secrets/clickhouse/password", &dir.join("pw").display().to_string())
        .replace("/run/secrets/clickhouse/ca.pem", &dir.join("ca.pem").display().to_string())
        .replace("/var/lib/hune/dashboards", &std::fs::canonicalize(&volume).unwrap().display().to_string())
        .replace("0.0.0.0:8080", &format!("127.0.0.1:{listen}"))
        .replace("0.0.0.0:9090", &format!("127.0.0.1:{ops}"))
        .replace("\"30s\"", "\"8s\"");
    let path = dir.join("hune.hcl");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn serve_answers_health_passes_and_sigterm_stops_with_0() {
    let dir = tempfile::tempdir().unwrap();
    let (listen, ops) = (free_port(), free_port());
    let config = write_config(dir.path(), listen, ops);
    let bin = env!("CARGO_BIN_EXE_hune");
    let child = ChildGuard(Some(
        Command::new(bin)
            .args(["serve", "--config"])
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    ));
    let ops_addr: SocketAddr = format!("127.0.0.1:{ops}").parse().unwrap();
    let main_addr: SocketAddr = format!("127.0.0.1:{listen}").parse().unwrap();
    let started = Instant::now();
    let mut ready = false;
    while started.elapsed() < Duration::from_secs(10) {
        if matches!(http(ops_addr, "GET", "/readyz"), Some((200, _))) {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(ready, "the server becomes ready");
    // The main listener serves while the process holds the standard output.
    let (status, body) = http(main_addr, "GET", "/api/dashboards/nodes").unwrap();
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"uid\":\"nodes\""));
    let (status, _) = http(main_addr, "GET", "/api/session").unwrap();
    assert_eq!(status, 200);
    let (status, text) = http(ops_addr, "GET", "/metrics").unwrap();
    assert_eq!(status, 200);
    assert!(text.contains("hune_http_requests_total{route=\"GET /api/session\",status=\"200\"} 1"));
    // `hune health` gives 0 and writes nothing.
    let health = Command::new(bin).args(["health", "--config"]).arg(&config).output().unwrap();
    assert_eq!(health.status.code(), Some(0));
    assert!(health.stdout.is_empty() && health.stderr.is_empty());
    // SIGTERM: a stop with the exit code 0 after the drain.
    let kill = Command::new("kill").args(["-TERM", &child.id().to_string()]).status().unwrap();
    assert!(kill.success());
    let stopped = Instant::now();
    let out = child.finish();
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(stopped.elapsed() < Duration::from_secs(14));
    // The log: one JSON object for each line, with `event`, and no secret.
    let log = String::from_utf8(out.stdout).unwrap();
    let mut events = Vec::new();
    for line in log.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}: {e}"));
        events.push(v["event"].as_str().expect("event").to_owned());
        assert!(!line.contains("chpass-marker"));
    }
    for want in [
        "server.config",
        "server.config.sources",
        "config.warning",
        "store.loaded",
        "server.listening",
        "server.ready",
        "http.request",
        "server.stopping",
        "server.stopped",
    ] {
        assert!(events.iter().any(|e| e == want), "{want} in {events:?}");
    }
    assert_eq!(events.last().map(String::as_str), Some("server.stopped"));
    // After the stop, nothing listens.
    assert!(TcpStream::connect_timeout(&ops_addr, Duration::from_millis(300)).is_err());
    let health = Command::new(bin).args(["health", "--config"]).arg(&config).output().unwrap();
    assert_eq!(health.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&health.stderr).starts_with("hune health: "));
}

#[test]
fn serve_with_a_bad_configuration_exits_with_1_before_it_binds() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.hcl");
    std::fs::write(&bad, "server {\n  unknown_key = 1\n}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_hune")).args(["serve", "-c"]).arg(&bad).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.is_empty());
    let out = Command::new(env!("CARGO_BIN_EXE_hune")).args(["serve"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn serve_stops_with_1_when_a_port_is_taken() {
    let dir = tempfile::tempdir().unwrap();
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let ops = taken.local_addr().unwrap().port();
    let config = write_config(dir.path(), free_port(), ops);
    let out = Command::new(env!("CARGO_BIN_EXE_hune")).args(["serve", "-c"]).arg(&config).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let log = String::from_utf8(out.stdout).unwrap();
    let last: serde_json::Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!(last["event"], "server.start_failed");
    assert_eq!(last["step"], "bind ops_listen");
    assert!(last["error"].as_str().unwrap().contains(&format!("127.0.0.1:{ops}")));
}
