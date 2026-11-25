// SPDX-License-Identifier: MIT OR Apache-2.0
//! Black-box tests of the PostgreSQL backend through the real binary: `migrate-store`, `serve`,
//! `copy-store`, `export`, `import` and `restore` (5.25 to 5.29, 5.37 to 5.40).
//!
//! The tests that need a server are ignored. `tests/ci/pgwire.sh` starts PostgreSQL 15 and 18 in
//! containers, sets the variables that `pg_env` reads, and runs them.

#[path = "../src/server/tests/pg_env.rs"]
mod pg_env;

use pg_env::{FIXTURES, PgEnv, Role};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_hune");
const DASHBOARD: &str = include_str!("../../../tests/fixtures/dashboards/nodes.json");

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).stdin(Stdio::null()).output().unwrap()
}

fn run_path(command: &str, config: &Path, rest: &[&str]) -> Output {
    Command::new(BIN).arg(command).arg("-c").arg(config).args(rest).stdin(Stdio::null()).output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Owns a child process: the drop kills it, also when a test fails.
struct ChildGuard(Option<std::process::Child>);

impl ChildGuard {
    fn id(&self) -> u32 {
        self.0.as_ref().map_or(0, std::process::Child::id)
    }

    fn exited(&mut self) -> bool {
        self.0.as_mut().is_some_and(|c| c.try_wait().ok().flatten().is_some())
    }

    fn finish(mut self) -> Output {
        self.0.take().expect("the child runs").wait_with_output().unwrap()
    }

    /// Sends SIGTERM and waits for the exit.
    fn terminate(self) -> Output {
        let kill = Command::new("kill").args(["-TERM", &self.id().to_string()]).status().unwrap();
        assert!(kill.success());
        self.finish()
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

struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.body))
    }
}

fn http(addr: SocketAddr, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Option<Answer> {
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    s.write_all(req.as_bytes()).ok()?;
    let mut raw = String::new();
    s.read_to_string(&mut raw).ok()?;
    let (head, body) = raw.split_once("\r\n\r\n")?;
    let mut lines = head.split("\r\n");
    let status = lines.next()?.split(' ').nth(1)?.parse().ok()?;
    let headers =
        lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned())).collect();
    Some(Answer { status, headers, body: body.to_owned() })
}

fn key(n: u32) -> String {
    format!("0194b3c6-6f1e-7d2a-9b4c-{n:012x}")
}

/// A running `hune serve`.
struct Server {
    child: ChildGuard,
    api: SocketAddr,
    ops: SocketAddr,
}

impl Server {
    fn start(config: &Path, ports: (u16, u16)) -> Server {
        let child = ChildGuard(Some(
            Command::new(BIN)
                .args(["serve", "-c"])
                .arg(config)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ));
        let mut server = Server {
            child,
            api: format!("127.0.0.1:{}", ports.0).parse().unwrap(),
            ops: format!("127.0.0.1:{}", ports.1).parse().unwrap(),
        };
        let started = Instant::now();
        loop {
            assert!(!server.child.exited(), "the server stopped before it was ready");
            if matches!(http(server.ops, "GET", "/readyz", &[], ""), Some(a) if a.status == 200) {
                return server;
            }
            assert!(started.elapsed() < Duration::from_secs(20), "the server is not ready");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn call(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Answer {
        http(self.api, method, path, headers, body).expect("an answer")
    }

    fn stop(self) -> Output {
        self.child.terminate()
    }
}

/// The events of a log, in order.
fn events(stdout: &[u8]) -> Vec<serde_json::Value> {
    text(stdout).lines().map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{l}: {e}"))).collect()
}

fn names(list: &[serde_json::Value]) -> Vec<&str> {
    list.iter().map(|e| e["event"].as_str().unwrap()).collect()
}

/// Reads each file below `root`: the path relative to the root gives the bytes.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            // The lock of a volume is not data.
            if path.file_name().is_some_and(|n| n == ".hune") {
                continue;
            }
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.insert(path.strip_prefix(root).unwrap().display().to_string(), std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// Checks that two trees hold the same files with the same bytes. A difference shows the first byte.
fn assert_same_tree(want: &BTreeMap<String, Vec<u8>>, got: &BTreeMap<String, Vec<u8>>, what: &str) {
    assert_eq!(want.keys().collect::<Vec<_>>(), got.keys().collect::<Vec<_>>(), "{what}: the files");
    for (name, bytes) in want {
        let other = &got[name];
        if bytes != other {
            let at = bytes.iter().zip(other).position(|(a, b)| a != b).unwrap_or(bytes.len().min(other.len()));
            panic!(
                "{what}: {name} differs at byte {at}\nwant: {}\ngot:  {}",
                String::from_utf8_lossy(&bytes[at.saturating_sub(40)..bytes.len().min(at + 60)]),
                String::from_utf8_lossy(&other[at.saturating_sub(40)..other.len().min(at + 60)])
            );
        }
    }
}

/// The text of the configuration of a volume.
fn volume_config(dir: &Path, name: &str, volume: &Path) -> PathBuf {
    std::fs::write(dir.join("ch.pw"), "chpass-marker\n").unwrap();
    std::fs::copy(Path::new(FIXTURES).join("ca-a.pem"), dir.join("ch-ca.pem")).unwrap();
    let text = std::fs::read_to_string(Path::new(FIXTURES).join("local.hcl"))
        .unwrap()
        .replace("/run/secrets/clickhouse/password", &dir.join("ch.pw").display().to_string())
        .replace("/run/secrets/clickhouse/ca.pem", &dir.join("ch-ca.pem").display().to_string())
        .replace("/var/lib/hune/dashboards", &volume.display().to_string());
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// A volume with two folders and two dashboards.
fn seeded_volume(dir: &Path, name: &str) -> PathBuf {
    let root = dir.join(name);
    for (folder, title) in [("transit", "Transit desk"), ("ops", "Operations")] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::write(
            root.join(folder).join("_folder.json"),
            format!("{{\n  \"format_version\": 1,\n  \"title\": \"{title}\"\n}}\n"),
        )
        .unwrap();
    }
    std::fs::write(root.join("transit/nodes.json"), DASHBOARD).unwrap();
    let second = DASHBOARD
        .replace("\"uid\": \"nodes\"", "\"uid\": \"second\"")
        .replace("\"title\": \"Nodes\"", "\"title\": \"Second\"");
    std::fs::write(root.join("ops/second.json"), second).unwrap();
    std::fs::canonicalize(&root).unwrap()
}

fn empty_volume(dir: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir.join(name)).unwrap();
    std::fs::canonicalize(dir.join(name)).unwrap()
}

/// Makes a database, migrates it with the binary and gives its name.
fn migrated_database(env: &PgEnv, dir: &Path, label: &str) -> String {
    let db = env.fresh_database(label);
    let owner = env.write_config(dir, &format!("{label}-owner.hcl"), &db, Role::Owner, false, (18080, 19090));
    let out = run_path("migrate-store", &owner, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    db
}

const FOUR_TABLES: &str = "DO $$ BEGIN ASSERT (SELECT count(*) FROM pg_tables WHERE schemaname = 'public' \
                           AND tablename LIKE 'hune_%') = 4, 'the four tables'; END $$";
const NO_TABLE: &str = "DO $$ BEGIN ASSERT (SELECT count(*) FROM pg_tables WHERE schemaname = 'public' \
                        AND tablename LIKE 'hune_%') = 0, 'no table'; END $$";

// ---- Tests without a server ----

#[test]
fn migrate_store_needs_a_configuration_file() {
    for args in [&["migrate-store"][..], &["migrate-store", "--bogus"], &["migrate-store", "-c", "/a", "extra"]] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty());
        assert!(text(&out.stderr).contains("migrate-store --config <path>"), "the usage names the command");
    }
    let out = run(&["migrate-store", "-c", "/nonexistent/migrate.hcl"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).starts_with("cannot read the configuration file /nonexistent/migrate.hcl"));
}

#[test]
fn migrate_store_refuses_a_volume_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    let config = volume_config(&d, "volume.hcl", &empty_volume(&d, "v"));
    let out = run_path("migrate-store", &config, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(
        text(&out.stderr).contains(
            "the primary store is not the PostgreSQL store; this command needs `primary = true` on `storage.postgres`"
        ),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn the_commands_stop_with_1_when_postgres_does_not_answer() {
    // No server: the port is closed. A command does not wait, unlike `serve` (5.29, step 1).
    let env = PgEnv {
        docker: "unused".to_owned(),
        container: "unused".to_owned(),
        host: "127.0.0.1".to_owned(),
        port: free_port(),
        owner_user: "hune_owner".to_owned(),
        owner_password: "ownerpw".to_owned(),
        app_user: "hune_app".to_owned(),
        app_password: "apppw".to_owned(),
        ca_file: PathBuf::new(),
        server_name: "localhost".to_owned(),
    };
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    let app = env.write_config(&d, "app.hcl", "hune", Role::App, false, (18080, 19090));
    let owner = env.write_config(&d, "owner.hcl", "hune", Role::Owner, false, (18080, 19090));
    let started = Instant::now();
    let want = format!("storage.postgres: cannot connect to 127.0.0.1:{}", env.port);
    for (command, rest) in [("export", vec!["--to", d.join("out").to_str().unwrap()]), ("restore", vec!["--uid", "x"])]
    {
        let out = run_path(command, &app, &rest);
        assert_eq!(out.status.code(), Some(1), "{command}");
        assert!(text(&out.stderr).starts_with(&want), "{command}: {}", text(&out.stderr));
        assert!(!text(&out.stderr).contains("apppw"));
    }
    let out = run_path("migrate-store", &owner, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).starts_with("storage.postgres: cannot connect: "), "{}", text(&out.stderr));
    assert!(!text(&out.stderr).contains("ownerpw"));
    assert!(started.elapsed() < Duration::from_secs(20));
}

// ---- Tests with a server (ignored) ----

#[test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
fn pg_migrate_store_twice_is_idempotent() {
    let env = PgEnv::from_env();
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    let db = env.fresh_database("migrate_twice");
    let owner = env.write_config(&d, "migrate.hcl", &db, Role::Owner, false, (18080, 19090));
    // The two files pass `check-config` (5.25, item 6).
    let app = env.write_config(&d, "config.hcl", &db, Role::App, false, (18080, 19090));
    for file in [&owner, &app] {
        assert_eq!(run_path("check-config", file, &[]).status.code(), Some(0));
    }
    let first = run_path("migrate-store", &owner, &[]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    assert!(first.stderr.is_empty());
    let out = text(&first.stdout);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    let migrated: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(
        (migrated["event"].as_str(), migrated["level"].as_str(), migrated["step"].as_u64()),
        (Some("store.migrated"), Some("info"), Some(1))
    );
    assert!(migrated["ts"].as_str().unwrap().ends_with('Z'));
    assert_eq!(lines[1], "schema version 1");
    env.admin(&db, FOUR_TABLES);
    // The second run changes nothing and writes no `store.migrated`.
    let second = run_path("migrate-store", &owner, &[]);
    assert_eq!(second.status.code(), Some(0), "{}", text(&second.stderr));
    assert_eq!(text(&second.stdout), "schema version 1, nothing to do\n");
    env.admin(&db, FOUR_TABLES);
    // The role of `serve` starts on the migrated database.
    let ports = (free_port(), free_port());
    let app = env.write_config(&d, "serve.hcl", &db, Role::App, false, ports);
    let server = Server::start(&app, ports);
    let out = server.stop();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
fn pg_migrate_store_over_tls() {
    let env = PgEnv::from_env();
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    let db = env.fresh_database("migrate_tls");
    let owner = env.write_config(&d, "migrate.hcl", &db, Role::Owner, true, (18080, 19090));
    let out = run_path("migrate-store", &owner, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).ends_with("schema version 1\n"));
    env.admin(&db, FOUR_TABLES);
    // A wrong server name stops the command: the check of the certificate is on.
    let wrong =
        std::fs::read_to_string(&owner).unwrap().replace(&format!("\"{}\"", env.server_name), "\"other.example\"");
    let wrong_path = d.join("wrong-name.hcl");
    std::fs::write(&wrong_path, wrong).unwrap();
    let out = run_path("migrate-store", &wrong_path, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!text(&out.stderr).is_empty());
}

#[test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
fn pg_migrate_store_stops_with_1_on_a_refusal() {
    let env = PgEnv::from_env();
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    // The application role cannot make the tables: the step is rolled back.
    let db = env.fresh_database("migrate_app");
    let app = env.write_config(&d, "app.hcl", &db, Role::App, false, (18080, 19090));
    let out = run_path("migrate-store", &app, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(text(&out.stderr).starts_with("migration 1 failed: "), "{}", text(&out.stderr));
    env.admin(&db, NO_TABLE);
    // A database with a schema that is newer than the binary.
    let db = migrated_database(&env, &d, "migrate_newer");
    env.admin(&db, "UPDATE hune_schema SET version = 2");
    let owner = d.join("migrate_newer-owner.hcl");
    let out = run_path("migrate-store", &owner, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stderr), "database schema 2 is newer than this binary (1)\n");
    // A password that is not the one of the role.
    std::fs::write(d.join("migrate_newer-owner.hcl.pgpw"), "not-the-password\n").unwrap();
    let out = run_path("migrate-store", &owner, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).starts_with("storage.postgres: cannot connect: "), "{}", text(&out.stderr));
    assert!(!text(&out.stderr).contains("not-the-password"));
}

#[test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
fn pg_serve_binary_runs_the_api_and_refuses_the_owner_role() {
    let env = PgEnv::from_env();
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    let db = migrated_database(&env, &d, "serve_bin");
    let ports = (free_port(), free_port());
    let app = env.write_config(&d, "app.hcl", &db, Role::App, true, ports);
    let server = Server::start(&app, ports);
    let json = ("Content-Type", "application/json");
    let a = server.call("POST", "/api/folders", &[json], r#"{"id":"transit","title":"Transit desk"}"#);
    assert_eq!(a.status, 201, "{}", a.body);
    let body = format!("{{\"folder\":\"transit\",\"document\":{DASHBOARD}}}");
    let k1 = key(1);
    let created = server.call("POST", "/api/dashboards", &[json, ("Idempotency-Key", &k1)], &body);
    assert_eq!(created.status, 201, "{}", created.body);
    let e1 = created.header("etag").unwrap().to_owned();
    let doc2 = DASHBOARD.replace("\"title\": \"Nodes\"", "\"title\": \"Nodes v2\"");
    let (k2, k3) = (key(2), key(3));
    let save = |if_match: &str, doc: &str, id: &str| {
        let body = format!("{{\"message\":\"m\",\"document\":{doc}}}");
        server.call("PUT", "/api/dashboards/nodes", &[json, ("If-Match", if_match), ("Idempotency-Key", id)], &body)
    };
    let saved = save(&e1, &doc2, &k2);
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(saved.json()["version"], 2);
    let doc3 = DASHBOARD.replace("\"title\": \"Nodes\"", "\"title\": \"Nodes v3\"");
    let stale = save(&e1, &doc3, &k3);
    assert_eq!(stale.status, 409, "{}", stale.body);
    assert_eq!(stale.json()["error"]["code"], "conflict");
    assert_eq!(stale.json()["error"]["current_version"], 2);
    let hist = server.call("GET", "/api/dashboards/nodes/history", &[], "").json();
    assert_eq!(hist["items"].as_array().unwrap().len(), 2);
    let stopped = server.stop();
    assert_eq!(stopped.status.code(), Some(0), "{}", text(&stopped.stderr));
    let log = events(&stopped.stdout);
    let list = names(&log);
    for want in ["server.config", "store.loaded", "server.listening", "server.ready", "server.stopped"] {
        assert!(list.contains(&want), "{want} in {list:?}");
    }
    let loaded = log.iter().find(|e| e["event"] == "store.loaded").unwrap();
    assert_eq!(loaded["backend"], "postgres");
    assert!(!text(&stopped.stdout).contains("apppw"), "the log holds no password");
    // The same database with the owner role: `serve` stops with 1 and the role message.
    let owner_serve = env.write_config(&d, "owner-serve.hcl", &db, Role::Owner, false, (free_port(), free_port()));
    let out = run_path("serve", &owner_serve, &[]);
    assert_eq!(out.status.code(), Some(1));
    let log = events(&out.stdout);
    let last = log.last().unwrap();
    assert_eq!(last["event"], "server.start_failed");
    assert_eq!(last["step"], "store.open");
    let message = last["error"].as_str().unwrap();
    assert!(
        message.starts_with(&format!(
            "storage.postgres: the role {} is a superuser or owns the Hune tables",
            env.owner_user
        )),
        "{message}"
    );
    assert!(!names(&log).contains(&"server.ready"));
    // A database without a schema: the message of 5.29 step 3.
    let empty = env.fresh_database("serve_bin_empty");
    let none = env.write_config(&d, "none.hcl", &empty, Role::App, false, (free_port(), free_port()));
    let out = run_path("serve", &none, &[]);
    assert_eq!(out.status.code(), Some(1));
    let log = events(&out.stdout);
    assert_eq!(
        log.last().unwrap()["error"],
        "storage.postgres: the database has no Hune schema; run: hune migrate-store -c <file>"
    );
}

#[test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
fn pg_copy_store_volume_to_postgres_to_volume_is_byte_equal() {
    let env = PgEnv::from_env();
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    let a = seeded_volume(&d, "a");
    let cfg_a = volume_config(&d, "a.hcl", &a);
    let db = migrated_database(&env, &d, "copy_roundtrip");
    let pg = env.write_config(&d, "pg.hcl", &db, Role::App, false, (18080, 19090));
    let copy = |from: &Path, to: &Path| {
        Command::new(BIN)
            .args(["copy-store", "--from"])
            .arg(from)
            .arg("--to")
            .arg(to)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    };
    // Volume to PostgreSQL.
    let out = copy(&cfg_a, &pg);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "folders: 2\ndashboards: 2\nversions: 0\n");
    // PostgreSQL to a second volume.
    let c = empty_volume(&d, "c");
    let cfg_c = volume_config(&d, "c.hcl", &c);
    let out = copy(&pg, &cfg_c);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "folders: 2\ndashboards: 2\nversions: 0\n");
    let (before, after) = (tree(&a), tree(&c));
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        ["ops/_folder.json", "ops/second.json", "transit/_folder.json", "transit/nodes.json"]
    );
    assert_same_tree(&before, &after, "volume to PostgreSQL to volume");
    // The target is not empty now: the copy stops with 1 and changes nothing.
    let out = copy(&cfg_a, &pg);
    assert_eq!(out.status.code(), Some(1));
    assert!(!text(&out.stderr).is_empty());
    // PostgreSQL to PostgreSQL.
    let db2 = migrated_database(&env, &d, "copy_pg2pg");
    let pg2 = env.write_config(&d, "pg2.hcl", &db2, Role::App, false, (18080, 19090));
    assert_eq!(copy(&pg, &pg2).status.code(), Some(0));
    let e = empty_volume(&d, "e");
    assert_eq!(copy(&pg2, &volume_config(&d, "e.hcl", &e)).status.code(), Some(0));
    assert_same_tree(&before, &tree(&e), "PostgreSQL to PostgreSQL to volume");
    // A source database without a schema: the error names the file of the configuration.
    let bare = env.fresh_database("copy_bare");
    let bare_cfg = env.write_config(&d, "bare.hcl", &bare, Role::App, false, (18080, 19090));
    let out = copy(&bare_cfg, &cfg_c);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        text(&out.stderr),
        format!(
            "source: storage.postgres: the database has no Hune schema; run: hune migrate-store -c {}\n",
            bare_cfg.display()
        )
    );
}

#[test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
fn pg_export_import_and_restore() {
    let env = PgEnv::from_env();
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    let a = seeded_volume(&d, "a");
    let want = tree(&a);
    let cfg_a = volume_config(&d, "a.hcl", &a);
    let db1 = migrated_database(&env, &d, "exim1");
    let pg1 = env.write_config(&d, "pg1.hcl", &db1, Role::App, false, (18080, 19090));
    let out = Command::new(BIN).args(["copy-store", "--from"]).arg(&cfg_a).arg("--to").arg(&pg1).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    // Export from PostgreSQL.
    let export_dir = d.join("exported");
    let out = run_path("export", &pg1, &["--to", export_dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "folders: 2\ndashboards: 2\nversions: 0\n");
    assert_same_tree(&want, &tree(&export_dir), "export");
    // Import into the empty store of a second database, and into a store that is not empty.
    let db2 = migrated_database(&env, &d, "exim2");
    let pg2 = env.write_config(&d, "pg2.hcl", &db2, Role::App, false, (18080, 19090));
    let out = run_path("import", &pg2, &["--from", export_dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("dashboards: 2"));
    let again = run_path("import", &pg2, &["--from", export_dir.to_str().unwrap()]);
    assert_eq!(again.status.code(), Some(1));
    let second_export = d.join("exported2");
    assert_eq!(run_path("export", &pg2, &["--to", second_export.to_str().unwrap()]).status.code(), Some(0));
    assert_same_tree(&want, &tree(&second_export), "export after import");
    // Restore: the dashboard exists, so the command refuses.
    let out = run_path("restore", &pg1, &["--uid", "nodes"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stderr), "the dashboard exists; restore a version in the editor\n");
    // Delete it through `serve`, then restore it.
    let ports = (free_port(), free_port());
    let serve_cfg = env.write_config(&d, "serve1.hcl", &db1, Role::App, false, ports);
    let server = Server::start(&serve_cfg, ports);
    let got = server.call("GET", "/api/dashboards/nodes", &[], "");
    assert_eq!(got.status, 200);
    let etag = got.header("etag").unwrap().to_owned();
    let id = key(9);
    let del = server.call("DELETE", "/api/dashboards/nodes", &[("If-Match", &etag), ("Idempotency-Key", &id)], "");
    assert_eq!(del.status, 200, "{}", del.body);
    assert_eq!(server.stop().status.code(), Some(0));
    let out = run_path("restore", &pg1, &["--uid", "nodes"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).starts_with("version: 3\netag: "), "{}", text(&out.stdout));
    // A uid that never existed, and an unknown version.
    assert_eq!(run_path("restore", &pg1, &["--uid", "never-seen"]).status.code(), Some(1));
    // The restored store exports the same bytes again.
    let third = d.join("exported3");
    assert_eq!(run_path("export", &pg1, &["--to", third.to_str().unwrap()]).status.code(), Some(0));
    let mut exported = tree(&third);
    // The versions of the history come with the export: 1 (the import), 2 (the tombstone) and 3 (the restore).
    let history: Vec<String> = exported.keys().filter(|k| k.starts_with(".history/")).cloned().collect();
    assert_eq!(
        history,
        [
            ".history/nodes/1.json",
            ".history/nodes/1.meta.json",
            ".history/nodes/2.meta.json",
            ".history/nodes/3.json",
            ".history/nodes/3.meta.json"
        ]
    );
    assert_eq!(exported[".history/nodes/3.json"], want["transit/nodes.json"], "the restored version");
    exported.retain(|k, _| !k.starts_with(".history/"));
    assert_same_tree(&want, &exported, "export after restore");
    // `migrate-documents` and a read command work on PostgreSQL.
    let out = run_path("migrate-documents", &pg1, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("transit/nodes: format 2, no migration needed"));
    // Only `serve` checks the role (5.29, step 6): `export` works with the owner role too.
    let owner = env.write_config(&d, "owner1.hcl", &db1, Role::Owner, false, (18080, 19090));
    assert_eq!(run_path("export", &owner, &["--to", d.join("by-owner").to_str().unwrap()]).status.code(), Some(0));
}

/// `migrate-documents` on PostgreSQL (4.42): a row of format 1 is not loadable, the dry run reports it, and
/// `--write` saves one version of the kind `migrate` with the canonical document of format 2.
#[test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
fn pg_migrate_documents_rewrites_the_rows_of_format_1() {
    let env = PgEnv::from_env();
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    let a = seeded_volume(&d, "a");
    let cfg_a = volume_config(&d, "a.hcl", &a);
    let db = migrated_database(&env, &d, "mig1");
    let pg = env.write_config(&d, "pg.hcl", &db, Role::App, false, (18080, 19090));
    let out = Command::new(BIN).args(["copy-store", "--from"]).arg(&cfg_a).arg("--to").arg(&pg).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    // Put the document `second` back in format 1, as a store written by the earlier binary holds it.
    let old = include_str!("../../../tests/fixtures/dashboards/format1/nodes.json")
        .replace("\"uid\": \"nodes\"", "\"uid\": \"second\"")
        .replace("\"title\": \"Nodes\"", "\"title\": \"Second\"");
    let etag = hune_store::Etag::of(old.as_bytes()).to_hex();
    env.admin(
        &db,
        &format!(
            "UPDATE hune_dashboards SET document = $doc${old}$doc$, etag = '{etag}', format_version = 1 WHERE uid = 'second'"
        ),
    );
    // The dry run reports the row and writes nothing.
    let out = run_path("migrate-documents", &pg, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let dry = text(&out.stdout);
    assert!(dry.contains("ops/second: format 1, migrates to format 2"), "{dry}");
    assert!(dry.contains("transit/nodes: format 2, no migration needed"), "{dry}");
    assert!(dry.contains("(dry run); nothing is written"), "{dry}");
    env.admin(
        &db,
        &format!("DO $$ BEGIN ASSERT (SELECT etag FROM hune_dashboards WHERE uid = 'second') = '{etag}'; END $$"),
    );
    // `--write` migrates it.
    let out = run_path("migrate-documents", &pg, &["--write"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("ops/second: migrated to format 2, version "), "{}", text(&out.stdout));
    let export_dir = d.join("exported");
    let out = run_path("export", &pg, &["--to", export_dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let got = tree(&export_dir);
    let want = DASHBOARD
        .replace("\"uid\": \"nodes\"", "\"uid\": \"second\"")
        .replace("\"title\": \"Nodes\"", "\"title\": \"Second\"");
    assert_eq!(text(&got["ops/second.json"]), want);
    let metas: Vec<String> = got
        .iter()
        .filter(|(k, _)| k.starts_with(".history/second/") && k.ends_with(".meta.json"))
        .map(|(_, v)| text(v))
        .collect();
    let last = metas.last().expect("a history version");
    assert!(
        last.contains("\"kind\": \"migrate\"")
            && last.contains("\"saved_by\": \"-\"")
            && last.contains("\"message\": \"format 1 to 2\""),
        "{last}"
    );
    // The other document got no version, and a second run has nothing to do.
    assert!(!got.keys().any(|k| k.starts_with(".history/nodes/")));
    let out = run_path("migrate-documents", &pg, &["--write"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("0 migrated, 2 already at format 2"), "{}", text(&out.stdout));
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
#[ignore = "needs PostgreSQL: tests/ci/pgwire.sh"]
fn pg_import_grafana_writes_the_same_documents_as_the_volume() {
    let env = PgEnv::from_env();
    let dir = tempfile::tempdir().unwrap();
    let d = std::fs::canonicalize(dir.path()).unwrap();
    let fixtures = Path::new(FIXTURES).join("..");
    let export = d.join("export");
    copy_dir(&fixtures.join("grafana/export"), &export);
    let folders = d.join("folders.map.json");
    std::fs::rename(export.join("folders.map.json"), &folders).unwrap();
    let accept = fixtures.join("grafana/accept.txt");
    let import = |config: &Path, write: bool| {
        let mut cmd = Command::new(BIN);
        cmd.arg("import-grafana").arg("--in").arg(&export).args(["--source-map", "clickhouse=prod", "--folders"]);
        cmd.arg(&folders).arg("--accept").arg(&accept).args(["--link-prefix", "/grafana/=/dashboards/", "-c"]);
        cmd.arg(config);
        if write {
            cmd.arg("--write");
        }
        cmd.stdin(Stdio::null()).output().unwrap()
    };
    // The volume gives the reference.
    let volume = empty_volume(&d, "v");
    let volume_cfg = volume_config(&d, "v.hcl", &volume);
    let out = import(&volume_cfg, true);
    assert_eq!(out.status.code(), Some(0), "{}{}", text(&out.stdout), text(&out.stderr));
    let mut want = tree(&volume);
    want.retain(|k, _| !k.starts_with(".history/"));
    assert!(want.keys().any(|k| k.ends_with(".json") && !k.contains("_folder")), "the import wrote documents");
    // PostgreSQL: a dry run reads only, the write makes the same documents.
    let db = migrated_database(&env, &d, "grafana");
    let pg = env.write_config(&d, "pg.hcl", &db, Role::App, false, (18080, 19090));
    let dry = import(&pg, false);
    assert_eq!(dry.status.code(), Some(0), "{}{}", text(&dry.stdout), text(&dry.stderr));
    let empty_export = d.join("empty-export");
    assert_eq!(run_path("export", &pg, &["--to", empty_export.to_str().unwrap()]).status.code(), Some(0));
    assert!(tree(&empty_export).is_empty(), "the dry run wrote nothing");
    let out = import(&pg, true);
    assert_eq!(out.status.code(), Some(0), "{}{}", text(&out.stdout), text(&out.stderr));
    let exported = d.join("exported");
    assert_eq!(run_path("export", &pg, &["--to", exported.to_str().unwrap()]).status.code(), Some(0));
    let mut got = tree(&exported);
    got.retain(|k, _| !k.starts_with(".history/"));
    assert_same_tree(&want, &got, "import-grafana on PostgreSQL");
    // A second write changes nothing (the plan finds each dashboard unchanged).
    let again = import(&pg, true);
    assert_eq!(again.status.code(), Some(0), "{}{}", text(&again.stdout), text(&again.stderr));
}
