//! Black-box tests of the kind of the source in `hune import-grafana` for MongoDB (15.9, case I23 of 16.13).
//!
//! Each test runs the real binary on the synthetic export of `tests/fixtures/grafana/export-mongo`. The
//! configuration has the block `clickhouse "prod"` and the block `mongodb "events"`.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

const MONGODB_BLOCK: &str = r#"
mongodb "events" {
  label              = "Events"
  topology           = "direct"
  host               = "127.0.0.1"
  port               = 1
  user               = "hune_ro"
  password_file      = "@PW@"
  auth_database      = "admin"
  databases          = ["analytics"]
  collections        = ["orders", "node_load"]
  read_preference    = "primary"
  tls_mode           = "disable"
  connect_timeout    = "5s"
  max_execution_time = "1m"
  max_result_rows    = 100000
  max_result_bytes   = 33554432
  max_query_size     = 1048576
  max_concurrent     = 16
  queue_wait         = "5s"
}
"#;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn hune<S: AsRef<OsStr>>(args: &[S]) -> Out {
    let out =
        Command::new(env!("CARGO_BIN_EXE_hune")).args(args).stdin(Stdio::null()).output().expect("run the binary");
    Out {
        code: out.status.code().expect("an exit code"),
        stdout: String::from_utf8(out.stdout).expect("utf-8"),
        stderr: String::from_utf8(out.stderr).expect("utf-8"),
    }
}

fn repo(path: &str) -> PathBuf {
    Path::new(REPO).join(path)
}

fn s(p: &Path) -> String {
    p.to_str().expect("utf-8 path").to_owned()
}

struct Fx {
    _dir: tempfile::TempDir,
    export: PathBuf,
    volume: PathBuf,
    config: PathBuf,
}

impl Fx {
    fn new() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let volume = root.join("volume");
        fs::create_dir_all(&volume).unwrap();
        fs::write(root.join("pw"), "pass-marker\n").unwrap();
        fs::copy(repo("tests/fixtures/config/ca-a.pem"), root.join("ca.pem")).unwrap();
        let mut config_text = fs::read_to_string(repo("tests/fixtures/config/local.hcl"))
            .unwrap()
            .replace("/run/secrets/clickhouse/password", &s(&root.join("pw")))
            .replace("/run/secrets/clickhouse/ca.pem", &s(&root.join("ca.pem")))
            .replace("/var/lib/hune/dashboards", &s(&volume));
        config_text.push_str(&MONGODB_BLOCK.replace("@PW@", &s(&root.join("pw"))));
        let config = root.join("hune.hcl");
        fs::write(&config, config_text).unwrap();
        let export = root.join("export");
        fs::create_dir_all(export.join("dashboards")).unwrap();
        for entry in fs::read_dir(repo("tests/fixtures/grafana/export-mongo/dashboards")).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), export.join("dashboards").join(entry.file_name())).unwrap();
        }
        fs::copy(repo("tests/fixtures/grafana/export-mongo/search.json"), export.join("search.json")).unwrap();
        fs::write(export.join("folders.json"), "[]").unwrap();
        fs::write(export.join("health.json"), r#"{"commit":"0","database":"ok","version":"12.4.2"}"#).unwrap();
        fs::write(
            export.join("plugin-clickhouse.json"),
            r#"{"name":"ClickHouse","type":"datasource","id":"grafana-clickhouse-datasource","info":{"version":"4.5.1"},"enabled":true}"#,
        )
        .unwrap();
        Fx { _dir: dir, export, volume, config }
    }

    fn run(&self, maps: &[&str], with_config: bool, extra: &[&str]) -> Out {
        let mut args: Vec<String> = vec!["import-grafana".into(), "--in".into(), s(&self.export)];
        for m in maps {
            args.push("--source-map".into());
            args.push((*m).to_owned());
        }
        if with_config {
            args.push("-c".into());
            args.push(s(&self.config));
        }
        args.extend(extra.iter().map(|a| (*a).to_owned()));
        hune(&args)
    }
}

const MAP: [&str; 2] = ["mongo=events", "clickhouse=prod"];

#[test]
fn a_map_without_a_configuration_takes_the_kind_from_the_type() {
    let fx = Fx::new();
    let out = fx.run(&MAP, false, &[]);
    // The run has losses that no file accepts: exit code 1, and no usage error.
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("MongoDB panels") || out.stdout.contains("mongo-main"), "{}", out.stdout);
}

#[test]
fn a_map_with_the_right_kinds_runs() {
    let fx = Fx::new();
    let out = fx.run(&MAP, true, &[]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

#[test]
fn a_mongodb_type_on_a_clickhouse_block_gives_exit_code_2() {
    let fx = Fx::new();
    let out = fx.run(&["mongo=prod", "clickhouse=prod"], true, &[]);
    assert_eq!(out.code, 2, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("--source-map mongo=prod"), "{}", out.stderr);
    assert!(out.stderr.contains("gives a mongodb block"), "{}", out.stderr);
    assert!(out.stderr.contains("has a clickhouse block"), "{}", out.stderr);
}

#[test]
fn a_clickhouse_type_on_a_mongodb_block_gives_exit_code_2() {
    let fx = Fx::new();
    let out = fx.run(&["mongo=events", "clickhouse=events"], true, &[]);
    assert_eq!(out.code, 2, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("--source-map clickhouse=events"), "{}", out.stderr);
    assert!(out.stderr.contains("gives a clickhouse block"), "{}", out.stderr);
    assert!(out.stderr.contains("has a mongodb block"), "{}", out.stderr);
}

#[test]
fn a_wrong_kind_writes_nothing() {
    let fx = Fx::new();
    let out = fx.run(&["mongo=prod", "clickhouse=prod"], true, &["--write"]);
    assert_eq!(out.code, 2, "{}{}", out.stdout, out.stderr);
    let written: Vec<_> = fs::read_dir(&fx.volume).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(written.iter().all(|n| n.to_str().is_some_and(|n| n.starts_with('.'))), "{written:?}");
}

#[test]
fn a_source_id_that_no_block_names_gives_exit_code_2() {
    let fx = Fx::new();
    let out = fx.run(&["mongo=nothere", "clickhouse=prod"], true, &[]);
    assert_eq!(out.code, 2, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("no block mongodb \"nothere\""), "{}", out.stderr);
}

#[test]
fn the_lists_of_the_block_decide_the_collections_only_with_a_configuration() {
    let fx = Fx::new();
    let report = |with_config: bool| {
        let base = fx.export.parent().unwrap().join(if with_config { "with" } else { "without" });
        let out = fx.run(&MAP, with_config, &["--report", &s(&base)]);
        assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
        fs::read_to_string(format!("{}.md", base.display())).unwrap()
    };
    // `$lookup` of the collection `secrets`: a fault with the lists of the block (7.33), and no fault without.
    assert!(report(true).contains("the collection is not in the key collections of the source"));
    assert!(!report(false).contains("the collection is not in the key collections of the source"));
}
