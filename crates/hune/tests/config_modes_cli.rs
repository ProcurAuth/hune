//! Black-box tests of the read modes of 13.18 with the real binary.
//!
//! The configuration of each test has a `clickhouse` block whose password file and CA file do
//! not exist, and whose user comes from an environment variable that is not set. `check-config`
//! refuses such a file. The commands that read only the block `storage` accept it.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn repo(path: &str) -> PathBuf {
    Path::new(REPO).join(path)
}

fn s(path: &Path) -> String {
    path.to_str().expect("utf-8 path").to_owned()
}

/// Run the binary with an empty environment, so no variable of the test process leaks in.
fn hune<S: AsRef<OsStr>>(args: &[S]) -> Out {
    let out = Command::new(env!("CARGO_BIN_EXE_hune"))
        .args(args)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .expect("run the binary");
    Out {
        code: out.status.code().expect("an exit code"),
        stdout: String::from_utf8(out.stdout).expect("utf-8"),
        stderr: String::from_utf8(out.stderr).expect("utf-8"),
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Fx {
    _dir: tempfile::TempDir,
    root: PathBuf,
    volume: PathBuf,
    /// The file LOCAL with a good volume and a `clickhouse` block that has no file and no variable.
    config: PathBuf,
}

impl Fx {
    fn new() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let volume = root.join("volume");
        fs::create_dir_all(&volume).unwrap();
        let text = fs::read_to_string(repo("tests/fixtures/config/local.hcl"))
            .unwrap()
            .replace("/var/lib/hune/dashboards", &s(&volume))
            .replace("/run/secrets/clickhouse/password", &s(&root.join("missing/password")))
            .replace("/run/secrets/clickhouse/ca.pem", &s(&root.join("missing/ca.pem")))
            .replace("user               = \"hune\"", "user_env           = \"HUNE_TEST_NOT_SET\"");
        assert!(text.contains("user_env"), "the replacement of the user must apply");
        let config = root.join("hune.hcl");
        fs::write(&config, text).unwrap();
        Fx { _dir: dir, root, volume, config }
    }

    /// The same file with a change.
    fn variant(&self, name: &str, change: impl FnOnce(String) -> String) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, change(fs::read_to_string(&self.config).unwrap())).unwrap();
        path
    }
}

#[test]
fn check_config_refuses_the_file_and_names_the_values_that_do_not_exist() {
    let fx = Fx::new();
    let out = hune(&["check-config", "--config", &s(&fx.config)]);
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("clickhouse \"prod\".user: the environment variable HUNE_TEST_NOT_SET is not set"));
    assert!(out.stderr.contains("clickhouse \"prod\".password: cannot read the file"), "{}", out.stderr);
}

#[test]
fn export_reads_only_the_block_storage() {
    let fx = Fx::new();
    let to = fx.root.join("out");
    let out = hune(&["export", "-c", &s(&fx.config), "--to", &s(&to)]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.starts_with("folders: "), "{}", out.stdout);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    assert!(to.is_dir());
}

#[test]
fn export_still_stops_on_a_fault_of_the_block_storage() {
    let fx = Fx::new();
    let to = fx.root.join("out");
    // A rescan interval out of range: the key table still checks the block `storage`.
    let bad = fx.variant("bad-range.hcl", |t| t.replace("rescan_interval = \"5s\"", "rescan_interval = \"1ms\""));
    let out = hune(&["export", "-c", &s(&bad), "--to", &s(&to)]);
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("storage.volume.rescan_interval"), "{}", out.stderr);
    // A path that is not canonical: a check of 13.13 for the block `storage`.
    let bad = fx.variant("bad-path.hcl", |t| t.replace(&s(&fx.volume), &format!("{}/../volume", s(&fx.root))));
    let out = hune(&["export", "-c", &s(&bad), "--to", &s(&to)]);
    assert_eq!(out.code, 1, "{}", out.stderr);
    assert!(out.stderr.contains("storage.volume.path"), "{}", out.stderr);
    // The errors of the other blocks do not show in these runs.
    assert!(!out.stderr.contains("clickhouse"), "{}", out.stderr);
}

#[test]
fn the_form_rules_still_apply_to_a_value_that_export_does_not_read() {
    let fx = Fx::new();
    let to = fx.root.join("out");
    for (name, change, expected) in [
        ("two-forms.hcl", "password_file      = \"/x/y\"\n  password_env       = \"A\"", "give exactly one form"),
        ("clear.hcl", "password           = \"clear-secret\"", "a secret accepts only"),
        ("name.hcl", "password_env       = \"1BAD\"", "expected an environment variable name"),
        ("relative.hcl", "password_file      = \"relative/path\"", "expected an absolute path"),
    ] {
        let path = fx.variant(name, |t| {
            let line = t.lines().find(|l| l.trim_start().starts_with("password_file")).unwrap().to_owned();
            t.replace(&line, &format!("  {change}"))
        });
        let out = hune(&["export", "-c", &s(&path), "--to", &s(&to)]);
        assert_eq!(out.code, 1, "{name}: {}", out.stderr);
        assert!(out.stderr.contains("clickhouse \"prod\".password: "), "{name}: {}", out.stderr);
        assert!(out.stderr.contains(expected), "{name}: {}", out.stderr);
        assert!(!out.stderr.contains("clear-secret"), "{name}: {}", out.stderr);
    }
}

#[test]
fn the_other_store_commands_use_the_same_mode() {
    let fx = Fx::new();
    let out = hune(&["migrate-documents", "-c", &s(&fx.config)]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let from = fx.root.join("empty-export");
    fs::create_dir_all(&from).unwrap();
    let out = hune(&["export", "-c", &s(&fx.config), "--to", &s(&fx.root.join("o2"))]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let other = fx.variant("target.hcl", |t| t.replace(&s(&fx.volume), &s(&fx.root.join("volume2"))));
    fs::create_dir_all(fx.root.join("volume2")).unwrap();
    let out = hune(&["copy-store", "--from", &s(&fx.config), "--to", &s(&other)]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let out = hune(&["restore", "-c", &s(&fx.config), "--uid", "nothing-here"]);
    // The command passes the checks of the file and stops later, on the dashboard that does not exist.
    assert!(!out.stderr.contains("environment variable") && !out.stderr.contains("cannot read the file"));
}

/// The work place of `import-grafana`: the export of the fixtures and its option files.
struct Import {
    fx: Fx,
    export: PathBuf,
    folders: PathBuf,
    accept: PathBuf,
}

impl Import {
    fn new() -> Import {
        let fx = Fx::new();
        let export = fx.root.join("export");
        copy_dir(&repo("tests/fixtures/grafana/export"), &export);
        let folders = fx.root.join("folders.map.json");
        fs::rename(export.join("folders.map.json"), &folders).unwrap();
        let accept = fx.root.join("accept.txt");
        fs::copy(repo("tests/fixtures/grafana/accept.txt"), &accept).unwrap();
        Import { fx, export, folders, accept }
    }

    fn run(&self, config: &Path, extra: &[&str], source: &str) -> Out {
        let mut args: Vec<String> = vec![
            "import-grafana".into(),
            "--in".into(),
            s(&self.export),
            "--source-map".into(),
            format!("clickhouse={source}"),
            "--folders".into(),
            s(&self.folders),
            "--accept".into(),
            s(&self.accept),
            "--link-prefix".into(),
            "/grafana/=/dashboards/".into(),
            "-c".into(),
            s(config),
        ];
        args.extend(extra.iter().map(|a| (*a).to_owned()));
        hune(&args)
    }
}

#[test]
fn import_grafana_write_reads_only_the_block_storage_and_the_labels() {
    let im = Import::new();
    // The dry run with `-c` uses the same mode.
    let dry = im.run(&im.fx.config, &[], "prod");
    assert_eq!(dry.code, 0, "{}", dry.stderr);
    assert!(dry.stdout.contains("dry run"), "{}", dry.stdout);
    let written = im.run(&im.fx.config, &["--write"], "prod");
    assert_eq!(written.code, 0, "{}", written.stderr);
    assert!(written.stdout.contains("import-grafana: write"), "{}", written.stdout);
    assert!(fs::read_dir(&im.fx.volume).unwrap().count() > 0, "the volume holds the dashboards");
}

#[test]
fn import_grafana_write_uses_the_labels_of_the_clickhouse_blocks() {
    let im = Import::new();
    // The label that the map names must exist in the file (15.9).
    let out = im.run(&im.fx.config, &["--write"], "other");
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(out.stderr.contains("the configuration has no block clickhouse \"other\""), "{}", out.stderr);
    // A label that breaks the rule of 13.13 stops the command.
    let bad = im.fx.variant("bad-label.hcl", |t| t.replace("clickhouse \"prod\"", "clickhouse \"Prod\""));
    let out = im.run(&bad, &["--write"], "prod");
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(out.stderr.contains("clickhouse \"Prod\": expected a label that matches"), "{}", out.stderr);
    // The form rules apply to the values that the command does not read.
    let clear = im.fx.variant("clear.hcl", |t| {
        let line = t.lines().find(|l| l.trim_start().starts_with("password_file")).unwrap().to_owned();
        t.replace(&line, "  password = \"clear-secret\"")
    });
    let out = im.run(&clear, &["--write"], "prod");
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(out.stderr.contains("a secret accepts only"), "{}", out.stderr);
    assert!(!out.stderr.contains("clear-secret"));
    // A fault in `storage` stops the command.
    let bad = im.fx.variant("bad-storage.hcl", |t| t.replace("history_keep    = 100", "history_keep    = 0"));
    let out = im.run(&bad, &["--write"], "prod");
    assert_eq!(out.code, 2, "{}", out.stderr);
    assert!(out.stderr.contains("storage.volume.history_keep"), "{}", out.stderr);
}
