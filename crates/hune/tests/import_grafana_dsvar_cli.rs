//! Black-box tests of a data source variable and of `__inputs` in `hune import-grafana` (15.9, case I36 of 16.13).
//!
//! Each test runs the real binary on the fixture `tests/fixtures/grafana/datasource_variable.json`. A key of
//! `--source-map` names the data source variable `datasource` and the item `DS_CLICKHOUSE` of `__inputs`.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

const POSTGRES_BLOCK: &str = r#"
postgres "analytics" {
  label              = "Analytics"
  host               = "127.0.0.1"
  port               = 1
  database           = "analytics"
  user               = "hune_ro"
  password_file      = "@PW@"
  search_path        = "metrics"
  sslmode            = "disable"
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
    root: PathBuf,
    export: PathBuf,
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
        config_text.push_str(&POSTGRES_BLOCK.replace("@PW@", &s(&root.join("pw"))));
        let config = root.join("hune.hcl");
        fs::write(&config, config_text).unwrap();
        let export = root.join("export");
        fs::create_dir_all(export.join("dashboards")).unwrap();
        fs::copy(
            repo("tests/fixtures/grafana/datasource_variable.json"),
            export.join("dashboards").join("ds-variable.json"),
        )
        .unwrap();
        fs::write(
            export.join("search.json"),
            r#"[{"id": 1, "uid": "ds-variable", "title": "Data source variable", "type": "dash-db"}]"#,
        )
        .unwrap();
        fs::write(export.join("folders.json"), "[]").unwrap();
        fs::write(export.join("health.json"), r#"{"commit":"0","database":"ok","version":"12.4.2"}"#).unwrap();
        fs::write(
            export.join("plugin-clickhouse.json"),
            r#"{"name":"ClickHouse","type":"datasource","id":"grafana-clickhouse-datasource","info":{"version":"4.5.1"},"enabled":true}"#,
        )
        .unwrap();
        Fx { _dir: dir, root, export, config }
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

#[test]
fn a_map_that_names_the_variable_and_the_input_gives_a_document() {
    let fx = Fx::new();
    let maps = ["datasource=prod", "DS_CLICKHOUSE=prod"];
    // Two `changed` items need acceptance: the variable and the item of `__inputs`.
    let out = fx.run(&maps, true, &[]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("changed: 2\n") && out.stdout.contains("unsupported: 0\n"), "{}", out.stdout);
    let accept = fx.root.join("accept.txt");
    fs::write(&accept, "ds-variable:/__inputs/0\nds-variable:/templating/list/0\n").unwrap();
    let out = fx.run(&maps, true, &["--accept", &s(&accept)]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("dashboard results: would_write=1\n"), "{}", out.stdout);
}

#[test]
fn the_kind_check_applies_to_the_source_of_a_variable() {
    let fx = Fx::new();
    // The variable is a ClickHouse data source variable, and `analytics` is a postgres block.
    let out = fx.run(&["datasource=analytics", "DS_CLICKHOUSE=prod"], true, &[]);
    assert_eq!(out.code, 2, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("--source-map datasource=analytics"), "{}", out.stderr);
    assert!(
        out.stderr.contains("gives a clickhouse block") && out.stderr.contains("has a postgres block"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_variable_that_the_map_does_not_name_is_listed_with_its_reason() {
    let fx = Fx::new();
    let out = fx.run(&["DS_CLICKHOUSE=prod"], false, &[]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("does not name the data source variable \"datasource\""), "{}", out.stdout);
    assert!(out.stdout.contains("the data source is a variable that the source map does not name"), "{}", out.stdout);
}
