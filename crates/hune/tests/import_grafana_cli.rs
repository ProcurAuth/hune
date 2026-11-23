//! Black-box tests of `hune import-grafana` (15, cases I03 to I13, I16 to I19 and I34 of 16.13).
//!
//! Each test runs the real binary on the synthetic export of `tests/fixtures/grafana/export`,
//! with a temporary volume. A test reads the volume through `hune-store` only to check the
//! history. Each child process is waited for by `Command::output`.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use hune_core::document::validate;
use hune_core::etag::Etag;
use hune_core::ids::Uid;
use hune_store::{AnyStore, FolderId, Store, VersionKind, VersionMeta, VolumeOptions, VolumeStore};
use serde_json::Value;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const GOLDEN_DIR_NAME: &str = "/private/grafana-export-20261120T090000Z";

fn repo(path: &str) -> PathBuf {
    Path::new(REPO).join(path)
}

fn text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn golden(name: &str) -> String {
    text(&repo(&format!("tests/golden/import/{name}")))
}

/// The result of one run of the binary.
#[derive(Debug)]
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

/// Every file below a directory, except the lock file: the relative path and the bytes.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, out);
            } else if path.strip_prefix(root).unwrap() != Path::new(".hune/lock") {
                // The lock file holds the process that took the lock: it differs in each run.
                out.insert(path.strip_prefix(root).unwrap().display().to_string(), fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// Checks that two trees are equal and names the files that differ.
fn assert_same(before: &BTreeMap<String, Vec<u8>>, after: &BTreeMap<String, Vec<u8>>, why: &str) {
    let mut diff: Vec<&String> =
        before.keys().chain(after.keys()).filter(|k| before.get(*k) != after.get(*k)).collect();
    diff.sort();
    diff.dedup();
    assert!(diff.is_empty(), "{why}: these files differ: {diff:?}");
}

fn s(p: &Path) -> String {
    p.to_str().expect("utf-8 path").to_owned()
}

/// A temporary work place: a volume, its configuration, a copy of the export and the option files.
struct Fx {
    _dir: tempfile::TempDir,
    root: PathBuf,
    volume: PathBuf,
    config: PathBuf,
    export: PathBuf,
    folders: PathBuf,
    accept: PathBuf,
    counter: std::cell::Cell<u32>,
}

impl Fx {
    fn new() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let volume = root.join("volume");
        fs::create_dir_all(&volume).unwrap();
        fs::write(root.join("pw"), "chpass-marker\n").unwrap();
        fs::copy(repo("tests/fixtures/config/ca-a.pem"), root.join("ca.pem")).unwrap();
        let config_text = text(&repo("tests/fixtures/config/local.hcl"))
            .replace("/run/secrets/clickhouse/password", &s(&root.join("pw")))
            .replace("/run/secrets/clickhouse/ca.pem", &s(&root.join("ca.pem")))
            .replace("/var/lib/hune/dashboards", &s(&volume));
        let config = root.join("hune.hcl");
        fs::write(&config, config_text).unwrap();
        let export = root.join("export");
        copy_dir(&repo("tests/fixtures/grafana/export"), &export);
        let folders = root.join("folders.map.json");
        fs::rename(export.join("folders.map.json"), &folders).unwrap();
        let accept = root.join("accept.txt");
        fs::copy(repo("tests/fixtures/grafana/accept.txt"), &accept).unwrap();
        Fx { _dir: dir, root, volume, config, export, folders, accept, counter: std::cell::Cell::new(0) }
    }

    /// The options that every run has: the export, the maps and the acceptance file.
    fn base(&self, input: &Path) -> Vec<String> {
        vec![
            "import-grafana".into(),
            "--in".into(),
            s(input),
            "--source-map".into(),
            "clickhouse=prod".into(),
            "--folders".into(),
            s(&self.folders),
            "--accept".into(),
            s(&self.accept),
            "--link-prefix".into(),
            "/grafana/=/dashboards/".into(),
        ]
    }

    fn run(&self, extra: &[&str]) -> Out {
        let mut args = self.base(&self.export);
        args.extend(extra.iter().map(|a| (*a).to_owned()));
        hune(&args)
    }

    /// A run with the configuration: a dry run with a read-only store.
    fn run_c(&self, extra: &[&str]) -> Out {
        let mut list = vec!["-c", self.config.to_str().unwrap()];
        list.extend_from_slice(extra);
        self.run(&list)
    }

    fn write(&self, extra: &[&str]) -> Out {
        let mut list = vec!["--write"];
        list.extend_from_slice(extra);
        self.run_c(&list)
    }

    /// A copy of the export that the closure changes.
    fn variant(&self, change: impl FnOnce(&Path)) -> PathBuf {
        self.counter.set(self.counter.get() + 1);
        let dir = self.root.join(format!("variant-{}", self.counter.get()));
        copy_dir(&self.export, &dir);
        change(&dir);
        dir
    }

    /// A copy of the export with the dashboard `prod-main` only.
    fn mini(&self, title: &str) -> PathBuf {
        self.variant(|dir| {
            for entry in fs::read_dir(dir.join("dashboards")).unwrap() {
                let path = entry.unwrap().path();
                if path.file_name().unwrap() != "prod-main.json" {
                    fs::remove_file(path).unwrap();
                }
            }
            let file = dir.join("dashboards/prod-main.json");
            let changed = text(&file).replacen("\"title\": \"Prod main\"", &format!("\"title\": \"{title}\""), 1);
            fs::write(file, changed).unwrap();
        })
    }

    fn report(&self, name: &str) -> Value {
        serde_json::from_str(&text(&self.root.join(format!("{name}.json")))).unwrap()
    }

    fn doc_path(&self, folder: &str, uid: &str) -> PathBuf {
        self.volume.join(folder).join(format!("{uid}.json"))
    }
}

/// The result of each dashboard of a report, by Hune uid.
fn results(report: &Value) -> BTreeMap<String, String> {
    report["dashboards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (d["uid"].as_str().unwrap().to_owned(), d["result"].as_str().unwrap().to_owned()))
        .collect()
}

fn folder_results(report: &Value) -> BTreeMap<String, String> {
    report["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["id"].as_str().unwrap().to_owned(), f["result"].as_str().unwrap().to_owned()))
        .collect()
}

fn count(map: &BTreeMap<String, String>, result: &str) -> usize {
    map.values().filter(|v| v.as_str() == result).count()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
}

fn options(volume: &Path, read_only: bool) -> VolumeOptions {
    VolumeOptions { root: volume.to_owned(), read_only, rescan_interval: Duration::from_secs(5), history_keep: 100 }
}

/// The history of a uid, newest first. The store is read-only: it takes no lock.
fn history(volume: &Path, uid: &str) -> Vec<VersionMeta> {
    runtime().block_on(async {
        let store: AnyStore = VolumeStore::open(options(volume, true)).await.unwrap().into();
        store.history(&Uid::parse(uid).unwrap(), None, 100).await.unwrap()
    })
}

/// Deletes a uid through the store. The store is dropped before the function returns.
fn store_delete(volume: &Path, uid: &str) {
    runtime().block_on(async {
        let store: AnyStore = VolumeStore::open(options(volume, false)).await.unwrap().into();
        let uid = Uid::parse(uid).unwrap();
        let current = store.get(&uid).await.unwrap();
        let meta = hune_store::WriteMeta {
            saved_by: hune_store::Subject::anonymous(),
            saved_at: hune_store::UtcMillis::from_millis(1_700_000_000_000),
            message: hune_store::Message::default(),
            save_id: None,
        };
        store.delete(&uid, &current.etag, &current.folder, &meta).await.unwrap();
    });
}

/// Moves a uid to another folder through the store.
fn store_move(volume: &Path, uid: &str, folder: &str) {
    runtime().block_on(async {
        let store: AnyStore = VolumeStore::open(options(volume, false)).await.unwrap().into();
        let uid = Uid::parse(uid).unwrap();
        let current = store.get(&uid).await.unwrap();
        let meta = hune_store::WriteMeta {
            saved_by: hune_store::Subject::anonymous(),
            saved_at: hune_store::UtcMillis::from_millis(1_700_000_000_000),
            message: hune_store::Message::default(),
            save_id: None,
        };
        store.move_to(&uid, &FolderId::parse(folder).unwrap(), &current.etag, &current.folder, &meta).await.unwrap();
    });
}

fn folder_file(title: &str) -> String {
    format!("{{\n  \"format_version\": 1,\n  \"title\": \"{title}\"\n}}\n")
}

// ------------------------------------------------------------------ a small JSON Schema check

/// Checks a value against the subset of JSON Schema that `report.schema.json` uses.
fn schema_errors(root: &Value, schema: &Value, value: &Value, at: &str, out: &mut Vec<String>) {
    if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
        let name = r.strip_prefix("#/$defs/").expect("local ref");
        return schema_errors(root, &root["$defs"][name], value, at, out);
    }
    if let Some(c) = schema.get("const")
        && c != value
    {
        out.push(format!("{at}: expected {c}, found {value}"));
    }
    if let Some(Value::Array(e)) = schema.get("enum")
        && !e.contains(value)
    {
        out.push(format!("{at}: {value} is not in {e:?}"));
    }
    if let Some(t) = schema.get("type") {
        let types: Vec<&str> = match t {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            _ => vec![],
        };
        let ok = types.iter().any(|t| match *t {
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            _ => false,
        });
        if !ok {
            out.push(format!("{at}: type {types:?} expected, found {value}"));
            return;
        }
    }
    if let Some(obj) = value.as_object() {
        if let Some(Value::Array(req)) = schema.get("required") {
            for r in req.iter().filter_map(Value::as_str) {
                if !obj.contains_key(r) {
                    out.push(format!("{at}: member {r} is absent"));
                }
            }
        }
        let props = schema.get("properties").and_then(Value::as_object);
        for (k, v) in obj {
            match props.and_then(|p| p.get(k)) {
                Some(s) => schema_errors(root, s, v, &format!("{at}/{k}"), out),
                None => match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => out.push(format!("{at}: member {k} is not permitted")),
                    Some(s @ Value::Object(_)) => schema_errors(root, s, v, &format!("{at}/{k}"), out),
                    _ => {}
                },
            }
        }
    }
    if let (Some(items), Some(arr)) = (schema.get("items"), value.as_array()) {
        for (i, v) in arr.iter().enumerate() {
            schema_errors(root, items, v, &format!("{at}/{i}"), out);
        }
    }
}

// ------------------------------------------------------------------ tests

/// Replaces the line `"created_at"` of a report by the line of the golden report.
fn fix_report(actual: &str, dir: &Path) -> String {
    let golden_json = golden("report.json");
    let golden_line = golden_json.lines().find(|l| l.contains("\"created_at\"")).unwrap();
    actual
        .lines()
        .map(|l| if l.contains("\"created_at\"") { golden_line } else { l })
        .collect::<Vec<_>>()
        .join("\n")
        .replace(&s(dir), GOLDEN_DIR_NAME)
        + "\n"
}

#[test]
fn the_dry_run_reports_equal_the_golden_files() {
    // I03, I10, I11: the report files and the nginx map, with the binary.
    let fx = Fx::new();
    let base = fx.root.join("rep");
    let map = fx.root.join("nginx.map");
    let out = fx.run(&["--report", base.to_str().unwrap(), "--nginx-map", map.to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert!(out.stdout.starts_with("import-grafana: dry run\n"), "{}", out.stdout);
    assert!(out.stdout.contains("exit code: 0\n"), "{}", out.stdout);
    assert!(out.stdout.contains("dashboards: 10\n") && out.stdout.contains("unaccepted: 0\n"), "{}", out.stdout);
    assert_eq!(out.stderr, "");
    let json = text(&fx.root.join("rep.json"));
    assert_eq!(fix_report(&json, &fx.export), golden("report.json"));
    let md = text(&fx.root.join("rep.md")).replace(&s(&fx.export), GOLDEN_DIR_NAME);
    assert_eq!(md, golden("report.md"));
    assert_eq!(text(&map), golden("nginx.map"));
    assert!(text(&map).contains("%24__all"), "the dollar sign is encoded");
    // The JSON follows its schema.
    let schema: Value = serde_json::from_str(&golden("report.schema.json")).unwrap();
    let report: Value = serde_json::from_str(&json).unwrap();
    let mut errors = Vec::new();
    schema_errors(&schema, &schema, &report, "", &mut errors);
    assert!(errors.is_empty(), "{errors:#?}");
    assert_eq!(report["mode"], "dry_run");
    assert_eq!(report["exit_code"], 0);
    // The report files are private (15.18).
    for name in ["rep.json", "rep.md"] {
        assert_eq!(fs::metadata(fx.root.join(name)).unwrap().permissions().mode() & 0o777, 0o600);
    }
    // MongoDB: a data source that the map does not name is `unsupported` and removes the panel (15.6). The
    // cut of the value to 200 characters is tested in `crates/hune-grafana/tests/rows.rs` (I10).
    let mongo: Vec<&Value> = report["dashboards"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["items"].as_array().unwrap().iter())
        .filter(|i| i["reason"].as_str().unwrap().contains("mongo1"))
        .collect();
    assert_eq!(mongo.len(), 4);
    for item in mongo {
        assert_eq!(item["class"], "unsupported");
        assert_eq!(item["fallback"], "panel removed");
        assert!(item["value"].as_str().unwrap().chars().count() <= 200);
    }
    // The run is deterministic: the second run gives the same output (I14 in one binary).
    let again = fx.run(&["--report", base.to_str().unwrap()]);
    assert_eq!(again.stdout, out.stdout);
    assert_eq!(fix_report(&text(&fx.root.join("rep.json")), &fx.export), golden("report.json"));
    // Nothing else is in the directory of the report: no temporary file is left.
    let leftovers: Vec<_> = fs::read_dir(&fx.root)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn exit_code_1_for_a_missing_acceptance_and_for_a_stale_line() {
    // I04, I10: the owner must accept each loss.
    let fx = Fx::new();
    let no_accept = {
        let mut args = fx.base(&fx.export);
        let at = args.iter().position(|a| a == "--accept").unwrap();
        args.drain(at..at + 2);
        args.push("--report".into());
        args.push(s(&fx.root.join("rep")));
        hune(&args)
    };
    assert_eq!(no_accept.code, 1, "{}", no_accept.stderr);
    assert!(no_accept.stdout.contains("items without acceptance (first 50):"), "{}", no_accept.stdout);
    assert!(no_accept.stdout.contains("exit code: 1\n"));
    let report = fx.report("rep");
    assert_eq!(report["exit_code"], 1);
    assert!(report["totals"]["unaccepted"].as_u64().unwrap() > 0);
    let r = results(&report);
    assert_eq!(r["monitor"], "blocked");
    assert_eq!(r["prod-main"], "would_write");
    // A stale line.
    fs::write(&fx.accept, format!("{}monitor:/no/such/member\n", text(&fx.accept))).unwrap();
    let stale = fx.run(&[]);
    assert_eq!(stale.code, 1, "{}", stale.stderr);
    assert!(stale.stdout.contains("monitor:/no/such/member"), "{}", stale.stdout);
    assert!(stale.stdout.contains("stale_accept_lines: 1\n"));
    // `--write` with exit code 1 writes nothing: the store is not touched.
    let written = fx.write(&[]);
    assert_eq!(written.code, 1);
    assert!(tree(&fx.volume).is_empty(), "{:?}", tree(&fx.volume).keys());
    // A source that the map does not name is `unsupported`, so the run needs acceptance (I10).
    let mut args = fx.base(&fx.export);
    let at = args.iter().position(|a| a == "clickhouse=prod").unwrap();
    args[at] = "other=prod".into();
    let unmapped = hune(&args);
    assert_eq!(unmapped.code, 1);
    assert!(unmapped.stdout.contains("unsupported"), "{}", unmapped.stdout);
}

#[test]
fn exit_code_2_for_a_bad_command_line() {
    // I04: each error of 15.2.
    let fx = Fx::new();
    let e = s(&fx.export);
    let cases: Vec<Vec<&str>> = vec![
        vec!["import-grafana"],
        vec!["import-grafana", "--in", &e],
        vec!["import-grafana", "--source-map", "clickhouse=prod"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=prod", "--bogus"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=prod", "stray"],
        vec!["import-grafana", "--in", &e, "--in", &e, "--source-map", "clickhouse=prod"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=prod", "--source-map", "clickhouse=other"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=Bad_Id"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=prod", "--link-prefix", "grafana=/x/"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=prod", "--write"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=prod", "--replace"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=prod", "--nginx-map", "/tmp/never-written"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=prod", "--report"],
        vec!["import-grafana", "--in", &e, "--source-map", "clickhouse=prod", "--write=yes"],
    ];
    for case in cases {
        let out = hune(&case);
        assert_eq!(out.code, 2, "{case:?}\n{}", out.stderr);
        assert!(out.stderr.starts_with("hune: import-grafana: "), "{}", out.stderr);
        assert!(out.stdout.is_empty(), "{case:?}");
    }
}

fn input_error(fx: &Fx, change: impl FnOnce(&Path), needle: &str) {
    let dir = fx.variant(change);
    let out = hune(&fx.base(&dir));
    assert_eq!(out.code, 2, "{needle}\n{}\n{}", out.stdout, out.stderr);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    assert!(out.stderr.contains(needle), "{needle} in {}", out.stderr);
}

#[test]
fn exit_code_2_for_a_bad_input_directory() {
    // I04: each error of 15.4.
    let fx = Fx::new();
    input_error(
        &fx,
        |d| {
            let f = d.join("health.json");
            fs::write(&f, [b"\xEF\xBB\xBF".as_slice(), &fs::read(&f).unwrap()].concat()).unwrap();
        },
        "byte order mark",
    );
    input_error(
        &fx,
        |d| fs::write(d.join("health.json"), "{\"version\":\"1\",\"version\":\"2\"}").unwrap(),
        "duplicate member",
    );
    input_error(&fx, |d| fs::write(d.join("health.json"), "{} []").unwrap(), "health.json");
    input_error(&fx, |d| fs::write(d.join("health.json"), "{\"a\":").unwrap(), "not valid JSON");
    input_error(&fx, |d| symlink("../health.json", d.join("dashboards/link.json")).unwrap(), "symbolic link");
    input_error(&fx, |d| symlink("health.json", d.join("other.json")).unwrap(), "symbolic link");
    input_error(
        &fx,
        |d| {
            let f = fs::File::create(d.join("health.json")).unwrap();
            f.set_len(33 * 1024 * 1024).unwrap();
        },
        "limit",
    );
    input_error(
        &fx,
        |d| fs::write(d.join("dashboards/prod-main.json"), "{\"apiVersion\":\"dashboard.grafana.app/v9\"}").unwrap(),
        "prod-main.json",
    );
    input_error(
        &fx,
        |d| fs::rename(d.join("dashboards/prod-main.json"), d.join("dashboards/prod-other.json")).unwrap(),
        "stem",
    );
    input_error(&fx, |d| fs::remove_file(d.join("folders.json")).unwrap(), "folders.json");
    input_error(&fx, |d| fs::remove_file(d.join("plugin-clickhouse.json")).unwrap(), "plugin-clickhouse.json");
    input_error(
        &fx,
        |d| {
            for e in fs::read_dir(d.join("dashboards")).unwrap() {
                fs::remove_file(e.unwrap().path()).unwrap();
            }
        },
        "expected 1 to 1000",
    );
    input_error(&fx, |d| fs::remove_file(d.join("search.json")).unwrap(), "search.json");
    let missing = fx.root.join("no-such-dir");
    let out = hune(&fx.base(&missing));
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("not a directory"), "{}", out.stderr);
}

/// The lines of `sha256sum` for the files of the export.
fn sums(dir: &Path) -> String {
    let mut lines = Vec::new();
    for name in ["folders.json", "health.json", "plugin-clickhouse.json", "search.json", "public-dashboards.json"] {
        lines.push(format!("{}  ./{name}", Etag::of(&fs::read(dir.join(name)).unwrap()).to_hex()));
    }
    for e in fs::read_dir(dir.join("dashboards")).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_str().unwrap().to_owned();
        lines.push(format!("{}  dashboards/{name}", Etag::of(&fs::read(&p).unwrap()).to_hex()));
    }
    lines.sort();
    lines.join("\n") + "\n"
}

#[test]
fn sha256sums_is_checked_before_the_files_are_read() {
    // I04: a listed file that differs from its digest stops the command with exit code 2.
    let fx = Fx::new();
    let good = fx.variant(|d| fs::write(d.join("SHA256SUMS"), sums(d)).unwrap());
    let out = hune(&fx.base(&good));
    assert_eq!(out.code, 0, "{}", out.stderr);
    input_error(
        &fx,
        |d| {
            fs::write(d.join("SHA256SUMS"), sums(d)).unwrap();
            fs::write(d.join("health.json"), "{\"version\": \"13.0.0\"}").unwrap();
        },
        "differs from the line of SHA256SUMS",
    );
    input_error(
        &fx,
        |d| {
            fs::write(d.join("SHA256SUMS"), sums(d)).unwrap();
            let f = d.join("dashboards/plain-raw.json");
            fs::write(&f, text(&f).replacen("Plain raw dashboard", "Plain raw dashboard 2", 1)).unwrap();
        },
        "plain-raw.json",
    );
    input_error(
        &fx,
        |d| fs::write(d.join("SHA256SUMS"), format!("{}  ./missing.json\n", "0".repeat(64))).unwrap(),
        "missing.json",
    );
    input_error(&fx, |d| fs::write(d.join("SHA256SUMS"), "not a digest line\n").unwrap(), "SHA256SUMS, line 1");
}

#[test]
fn exit_code_2_for_a_bad_option_file_or_configuration() {
    // I04, I09: the folder map, the acceptance file, the source map and the configuration.
    let fx = Fx::new();
    let case = |args: &[&str]| {
        let out = hune(args);
        assert_eq!(out.code, 2, "{args:?}\n{}\n{}", out.stdout, out.stderr);
        out.stderr
    };
    let with_folders = |content: &str| {
        let path = fx.root.join("bad-folders.json");
        fs::write(&path, content).unwrap();
        let mut args = fx.base(&fx.export);
        let at = args.iter().position(|a| a == "--folders").unwrap();
        args[at + 1] = s(&path);
        hune(&args)
    };
    let out = with_folders("{\"folders\": [], \"zzz\": 1}");
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("folder map"), "{}", out.stderr);
    // Two Grafana folders that map to one id (I09).
    let out = with_folders(
        "{\"folders\": [{\"grafana_uid\": \"fe1x0abc2kq9\", \"id\": \"one\"}, {\"grafana_uid\": \"ae7r4kd0pq1z\", \"id\": \"one\"}]}",
    );
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("map to the same id"), "{}", out.stderr);
    assert_eq!(with_folders("{\"folders\": [{\"grafana_uid\": \"x\", \"id\": \"general\"}]}").code, 2);
    assert_eq!(with_folders("{ not json").code, 2);
    // A duplicate acceptance line.
    fs::write(&fx.accept, format!("{}{}", text(&fx.accept), text(&fx.accept).lines().nth(1).unwrap())).unwrap();
    let err = case(&fx.base(&fx.export).iter().map(String::as_str).collect::<Vec<_>>());
    assert!(err.contains("duplicate item id"), "{err}");
    fs::copy(repo("tests/fixtures/grafana/accept.txt"), &fx.accept).unwrap();
    // With a configuration, the source id must name a `clickhouse` block.
    let mut args = fx.base(&fx.export);
    let at = args.iter().position(|a| a == "clickhouse=prod").unwrap();
    args[at] = "clickhouse=other".into();
    args.extend(["-c".into(), s(&fx.config)]);
    let err = case(&args.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(err.contains("no block clickhouse \"other\""), "{err}");
    // A configuration that is not valid.
    let bad = fx.root.join("bad.hcl");
    fs::write(&bad, "server {\n  unknown_key = 1\n}\n").unwrap();
    let mut args = fx.base(&fx.export);
    args.extend(["-c".into(), s(&bad)]);
    case(&args.iter().map(String::as_str).collect::<Vec<_>>());
    let mut args = fx.base(&fx.export);
    args.extend(["-c".into(), s(&fx.root.join("missing.hcl"))]);
    case(&args.iter().map(String::as_str).collect::<Vec<_>>());
}

#[test]
fn outputs_inside_a_git_checkout_or_without_a_parent_give_exit_code_2() {
    // I13 and 15.2.
    let fx = Fx::new();
    fs::create_dir_all(fx.root.join("repo/.git")).unwrap();
    fs::create_dir_all(fx.root.join("repo/sub/deeper")).unwrap();
    fs::create_dir_all(fx.root.join("worktree")).unwrap();
    fs::write(fx.root.join("worktree/.git"), "gitdir: /elsewhere\n").unwrap();
    for (option, path) in [
        ("--report", "repo/rep"),
        ("--report", "repo/sub/deeper/rep"),
        ("--nginx-map", "repo/map"),
        ("--nginx-map", "repo/sub/map"),
        ("--report", "worktree/rep"),
        ("--report", "no-parent/rep"),
        ("--nginx-map", "no-parent/map"),
    ] {
        let target = fx.root.join(path);
        let out = fx.run(&[option, target.to_str().unwrap()]);
        assert_eq!(out.code, 2, "{option} {path}\n{}", out.stderr);
        assert!(out.stdout.is_empty(), "no work before the check");
        assert!(out.stderr.contains("git checkout") || out.stderr.contains("parent directory"), "{}", out.stderr);
    }
    let entries: Vec<_> = tree(&fx.root.join("repo")).into_keys().collect();
    assert!(entries.is_empty(), "{entries:?}");
    // A path outside a checkout works, and a file that exists is replaced.
    let target = fx.root.join("map");
    fs::write(&target, "old\n").unwrap();
    let out = fx.run(&["--nginx-map", target.to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(text(&target), golden("nginx.map"));
}

#[test]
fn dry_run_with_a_configuration_writes_nothing_and_compares_with_the_store() {
    // I05.
    let fx = Fx::new();
    // An empty store: each dashboard is new and each folder is new.
    let before = tree(&fx.volume);
    let rep = fx.root.join("rep");
    let out = fx.run_c(&["--report", rep.to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_same(&before, &tree(&fx.volume), "a dry run writes nothing");
    let report = fx.report("rep");
    assert_eq!(count(&results(&report), "would_write"), 8);
    assert_eq!(count(&folder_results(&report), "would_create"), 3);
    // A store that holds only `prod-main`: unchanged, would_write, would_create and exists.
    let mini = fx.mini("Prod main");
    assert_eq!(
        hune(
            &[&[
                "import-grafana",
                "--in",
                &s(&mini),
                "--source-map",
                "clickhouse=prod",
                "-c",
                &s(&fx.config),
                "--write"
            ][..]]
            .concat()
        )
        .code,
        0
    );
    let before = tree(&fx.volume);
    let out = fx.run_c(&["--report", rep.to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert_same(&before, &tree(&fx.volume), "a dry run writes nothing");
    let report = fx.report("rep");
    assert_eq!(report["mode"], "dry_run");
    let r = results(&report);
    assert_eq!(r["prod-main"], "unchanged");
    assert_eq!(r["monitor"], "would_write");
    let f = folder_results(&report);
    assert_eq!(f["general"], "exists");
    assert_eq!(f["transit"], "would_create");
    // The same dashboard with other bytes and a folder with another title.
    fs::write(
        fx.doc_path("general", "prod-main"),
        text(&fx.doc_path("general", "prod-main")).replacen("\"Prod main\"", "\"Prod main 2\"", 1),
    )
    .unwrap();
    fs::write(fx.volume.join("general/_folder.json"), folder_file("Renamed")).unwrap();
    let before = tree(&fx.volume);
    let out = fx.run_c(&["--report", rep.to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert_same(&before, &tree(&fx.volume), "the tree");
    let report = fx.report("rep");
    assert_eq!(results(&report)["prod-main"], "would_refuse");
    assert_eq!(folder_results(&report)["general"], "would_rename");
    // A deleted uid is refused (15.13).
    let fx2 = Fx::new();
    let mini = fx2.mini("Prod main");
    let args =
        ["import-grafana", "--in", &s(&mini), "--source-map", "clickhouse=prod", "-c", &s(&fx2.config), "--write"];
    assert_eq!(hune(&args).code, 0);
    store_delete(&fx2.volume, "prod-main");
    let out = hune(&args[..args.len() - 1]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("would_refuse=1"), "{}", out.stdout);
    // `--write` gives exit code 3, with or without `--replace`, and writes nothing (15.13, step 3).
    let before = tree(&fx2.volume);
    for extra in [&["--write"][..], &["--write", "--replace"]] {
        let mut list = args[..args.len() - 1].to_vec();
        list.extend_from_slice(extra);
        let out = hune(&list);
        assert_eq!(out.code, 3, "{}\n{}", out.stdout, out.stderr);
        assert!(out.stdout.contains("refused_exists=1"), "{}", out.stdout);
    }
    assert_same(&before, &tree(&fx2.volume), "a deleted uid stays deleted");
}

#[test]
fn write_is_idempotent_and_the_documents_equal_the_golden_files() {
    // I06, I07, I08, I09, I12.
    let fx = Fx::new();
    let rep = fx.root.join("rep");
    let out = fx.write(&["--report", rep.to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    let report = fx.report("rep");
    assert_eq!(report["mode"], "write");
    assert_eq!(report["exit_code"], 0);
    let r = results(&report);
    assert_eq!(count(&r, "written"), 8, "{r:?}");
    assert_eq!(count(&r, "not_imported"), 2);
    assert_eq!(count(&folder_results(&report), "created"), 3);
    // Folders and uids (I08, I09): the root is `general`, the map applies, a changed uid is a new file.
    let place = [
        ("general", "g-301f976e365b83e3"),
        ("general", "plain-raw"),
        ("general", "prod-main"),
        ("general", "monitor-v2b"),
        ("transit", "monitor"),
        ("transit", "monitor-v2a"),
        ("transit", "monitor-wrapped"),
        ("nodes", "mixed-case-uid"),
    ];
    for (folder, uid) in place {
        let bytes = fs::read(fx.doc_path(folder, uid)).unwrap_or_else(|e| panic!("{folder}/{uid}: {e}"));
        // I12: the document is valid, canonical, and equal to its golden file.
        let valid = validate(&bytes, Some(&Uid::parse(uid).unwrap())).expect("a valid document");
        assert_eq!(valid.bytes(), bytes.as_slice());
        assert_eq!(String::from_utf8(bytes).unwrap(), golden(&format!("documents/{uid}.json")));
        let versions = history(&fx.volume, uid);
        assert_eq!(versions.len(), 1, "{uid}");
        assert_eq!(versions[0].kind, VersionKind::Import);
        assert_eq!(versions[0].saved_by.as_str(), "import-grafana");
        assert!(versions[0].message.as_str().starts_with("import-grafana dashboards/"), "{:?}", versions[0].message);
        assert_eq!(versions[0].message.as_str().split(' ').count(), 3);
        assert!(versions[0].save_id.is_some());
    }
    assert!(!fx.doc_path("general", "nested-case").exists() && !fx.doc_path("general", "bad-folder").exists());
    assert!(text(&fx.volume.join("transit/_folder.json")).contains("\"Transit desk\""));
    assert!(text(&fx.volume.join("nodes/_folder.json")).contains("\"Nodes\""));
    assert!(text(&fx.volume.join("general/_folder.json")).contains("\"General\""));
    // I06: the second run writes nothing.
    let before = tree(&fx.volume);
    let out = fx.write(&["--report", rep.to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert_same(&before, &tree(&fx.volume), "the volume tree is byte-equal");
    let report = fx.report("rep");
    let r = results(&report);
    assert_eq!(count(&r, "unchanged"), 8, "{r:?}");
    assert_eq!(count(&folder_results(&report), "exists"), 3);
    for (_, uid) in place {
        assert_eq!(history(&fx.volume, uid).len(), 1, "no history version for {uid}");
    }
    // I07: another document and `--replace`. Without `--replace`, exit code 3 and no write.
    let path = fx.doc_path("transit", "monitor");
    fs::write(&path, text(&path).replacen("\"Node monitor\"", "\"Node monitor 2\"", 1)).unwrap();
    let before = tree(&fx.volume);
    let out = fx.write(&["--report", rep.to_str().unwrap()]);
    assert_eq!(out.code, 3, "{}\n{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("exit code: 3\n"));
    assert_same(&before, &tree(&fx.volume), "step 3 writes nothing");
    let report = fx.report("rep");
    assert_eq!(report["exit_code"], 3);
    assert_eq!(results(&report)["monitor"], "refused_exists");
    let out = fx.write(&["--replace", "--report", rep.to_str().unwrap()]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    let r = results(&fx.report("rep"));
    assert_eq!(r["monitor"], "replaced");
    assert_eq!(count(&r, "unchanged"), 7, "{r:?}");
    assert_eq!(text(&path), golden("documents/monitor.json"));
    let versions = history(&fx.volume, "monitor");
    assert_eq!(versions[0].kind, VersionKind::Import);
    assert_eq!(versions[0].saved_by.as_str(), "import-grafana");
    for (_, uid) in place.iter().filter(|(_, u)| *u != "monitor") {
        assert_eq!(history(&fx.volume, uid).len(), 1, "an equal dashboard gets no version: {uid}");
    }
}

#[test]
fn a_b_a_gives_a_new_version_and_a_new_save_id() {
    // I16.
    let fx = Fx::new();
    let a = fx.mini("Prod main");
    let b = fx.mini("Prod main B");
    let run = |dir: &Path, extra: &[&str]| {
        let mut args = vec!["import-grafana", "--in", dir.to_str().unwrap(), "--source-map", "clickhouse=prod", "-c"];
        let config = s(&fx.config);
        args.push(&config);
        args.push("--write");
        args.extend_from_slice(extra);
        hune(&args)
    };
    let out = run(&a, &[]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let doc_a = fs::read(fx.doc_path("general", "prod-main")).unwrap();
    let out = run(&b, &["--replace"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("replaced=1"), "{}", out.stdout);
    assert_ne!(fs::read(fx.doc_path("general", "prod-main")).unwrap(), doc_a);
    let out = run(&a, &["--replace"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("replaced=1"), "{}", out.stdout);
    assert_eq!(fs::read(fx.doc_path("general", "prod-main")).unwrap(), doc_a, "the bytes of A are in the store");
    let versions = history(&fx.volume, "prod-main");
    assert_eq!(versions.len(), 3);
    assert!(versions.iter().all(|v| v.kind == VersionKind::Import && v.saved_by.as_str() == "import-grafana"));
    assert_ne!(versions[0].save_id, versions[2].save_id, "the second A has a new save_id");
    assert_ne!(versions[0].save_id, versions[1].save_id);
    let id = versions[0].save_id.unwrap().to_string();
    assert_eq!(&id[14..15], "8", "UUID version 8: {id}");
    // A repeated run is `unchanged` and writes no version.
    let out = run(&a, &["--replace"]);
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("unchanged=1"), "{}", out.stdout);
    assert_eq!(history(&fx.volume, "prod-main").len(), 3);
}

#[test]
fn another_folder_or_title_needs_replace() {
    // I18.
    let fx = Fx::new();
    assert_eq!(fx.write(&[]).code, 0);
    let rep = fx.root.join("rep");
    let rep_arg = rep.to_str().unwrap();
    // Same bytes, other folder: `moved` with `--replace`; exit code 3 and no write without it.
    store_move(&fx.volume, "plain-raw", "nodes");
    assert!(fx.doc_path("nodes", "plain-raw").exists());
    let before = tree(&fx.volume);
    let out = fx.write(&["--report", rep_arg]);
    assert_eq!(out.code, 3, "{}", out.stderr);
    assert_same(&before, &tree(&fx.volume), "the tree");
    assert_eq!(results(&fx.report("rep"))["plain-raw"], "refused_exists");
    let out = fx.write(&["--replace", "--report", rep_arg]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(results(&fx.report("rep"))["plain-raw"], "moved");
    assert!(fx.doc_path("general", "plain-raw").exists() && !fx.doc_path("nodes", "plain-raw").exists());
    let versions = history(&fx.volume, "plain-raw");
    assert_eq!(versions[0].kind, VersionKind::Move);
    assert_eq!(versions[0].saved_by.as_str(), "import-grafana");
    // Other bytes and other folder: `moved_replaced`.
    store_move(&fx.volume, "plain-raw", "nodes");
    let path = fx.doc_path("nodes", "plain-raw");
    fs::write(&path, text(&path).replacen("\"Plain raw dashboard\"", "\"Plain raw dashboard 2\"", 1)).unwrap();
    let out = fx.write(&["--replace", "--report", rep_arg]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(results(&fx.report("rep"))["plain-raw"], "moved_replaced");
    assert_eq!(text(&fx.doc_path("general", "plain-raw")), golden("documents/plain-raw.json"));
    let versions = history(&fx.volume, "plain-raw");
    assert_eq!(versions[0].kind, VersionKind::Import);
    assert_eq!(versions[1].kind, VersionKind::Move);
    // A folder with another title.
    fs::write(fx.volume.join("transit/_folder.json"), folder_file("Other title")).unwrap();
    let before = tree(&fx.volume);
    let out = fx.write(&["--report", rep_arg]);
    assert_eq!(out.code, 3, "{}", out.stderr);
    assert_same(&before, &tree(&fx.volume), "the tree");
    assert_eq!(folder_results(&fx.report("rep"))["transit"], "refused_title");
    let out = fx.write(&["--replace", "--report", rep_arg]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(folder_results(&fx.report("rep"))["transit"], "renamed");
    assert!(text(&fx.volume.join("transit/_folder.json")).contains("\"Transit desk\""));
}

#[test]
fn a_current_file_that_is_not_valid_is_saved_over_with_the_raw_etag() {
    // I19.
    let fx = Fx::new();
    let mini = fx.mini("Prod main");
    let args = |extra: &[&str]| {
        let mut list: Vec<String> =
            ["import-grafana", "--in", &s(&mini), "--source-map", "clickhouse=prod", "-c", &s(&fx.config), "--write"]
                .iter()
                .map(|a| (*a).to_owned())
                .collect();
        list.extend(extra.iter().map(|a| (*a).to_owned()));
        list
    };
    assert_eq!(hune(&args(&[])).code, 0);
    let path = fx.doc_path("general", "prod-main");
    let good = fs::read(&path).unwrap();
    fs::write(&path, "{ not json").unwrap();
    // Without `--replace`: other bytes, exit code 3, nothing written.
    let out = hune(&args(&[]));
    assert_eq!(out.code, 3, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(fs::read(&path).unwrap(), b"{ not json");
    let out = hune(&args(&["--replace"]));
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("replaced=1"), "{}", out.stdout);
    assert_eq!(fs::read(&path).unwrap(), good);
    let versions = history(&fx.volume, "prod-main");
    let kinds: Vec<VersionKind> = versions.iter().map(|v| v.kind).collect();
    assert_eq!(kinds, vec![VersionKind::Import, VersionKind::External, VersionKind::Import]);
    assert_eq!(versions[1].size, None, "the external version has no bytes");
}

#[test]
fn a_volume_of_format_2_gets_the_documents_of_format_3_with_replace() {
    // I34: the importer writes format 3 (I32). A document of format 2 in the volume is not valid in this
    // binary, so it is "other bytes": `--write` gives exit code 3, and `--write --replace` saves over it.
    let fx = Fx::new();
    let mini = fx.mini("Prod main");
    let args = |extra: &[&str]| {
        let mut list: Vec<String> =
            ["import-grafana", "--in", &s(&mini), "--source-map", "clickhouse=prod", "-c", &s(&fx.config), "--write"]
                .iter()
                .map(|a| (*a).to_owned())
                .collect();
        list.extend(extra.iter().map(|a| (*a).to_owned()));
        list
    };
    assert_eq!(hune(&args(&[])).code, 0);
    let path = fx.doc_path("general", "prod-main");
    let format3 = fs::read(&path).unwrap();
    let text3 = String::from_utf8(format3.clone()).unwrap();
    assert!(text3.starts_with("{\n  \"format_version\": 3,\n"), "{text3}");
    assert!(text3.contains("  \"links\": [],\n"), "{text3}");
    // The same document in format 2: the version number, and no member `links` (4.42).
    let format2 =
        text3.replacen("\"format_version\": 3", "\"format_version\": 2", 1).replacen("  \"links\": [],\n", "", 1);
    fs::write(&path, &format2).unwrap();
    // The second run in format 3 is idempotent on a volume of format 3 (I34), and here it finds other bytes.
    let out = hune(&args(&[]));
    assert_eq!(out.code, 3, "{}\n{}", out.stdout, out.stderr);
    assert_eq!(fs::read_to_string(&path).unwrap(), format2);
    let out = hune(&args(&["--replace"]));
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("replaced=1"), "{}", out.stdout);
    assert_eq!(fs::read(&path).unwrap(), format3);
    let kinds: Vec<VersionKind> = history(&fx.volume, "prod-main").iter().map(|v| v.kind).collect();
    assert_eq!(kinds, vec![VersionKind::Import, VersionKind::External, VersionKind::Import]);
    // After that, a run is idempotent.
    let before = tree(&fx.volume);
    let out = hune(&args(&[]));
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("unchanged=1"), "{}", out.stdout);
    assert_same(&before, &tree(&fx.volume), "the volume tree is byte-equal");
}

#[test]
fn a_held_volume_lock_gives_exit_code_3_and_a_dry_run_works() {
    // I04, 3.14.
    let fx = Fx::new();
    let holder = runtime().block_on(VolumeStore::open(options(&fx.volume, false))).unwrap();
    let before = tree(&fx.volume);
    let out = fx.write(&[]);
    assert_eq!(out.code, 3, "{}", out.stderr);
    assert!(out.stderr.contains("lock") && out.stderr.contains("hune serve"), "{}", out.stderr);
    assert!(out.stdout.is_empty());
    assert_same(&before, &tree(&fx.volume), "the tree");
    // A dry run takes no lock.
    let out = fx.run_c(&[]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    drop(holder);
    assert_eq!(fx.write(&[]).code, 0);
}

#[test]
fn a_read_only_volume_refuses_write_and_a_missing_volume_stops_the_run() {
    // 13.10 and 15.13: the store is the judge of a write.
    let fx = Fx::new();
    let read_only = fx.root.join("ro.hcl");
    fs::write(
        &read_only,
        text(&fx.config)
            .replace("read_only       = false", "read_only       = true")
            .replace("editing                 = true", "editing                 = false"),
    )
    .unwrap();
    let mut args = fx.base(&fx.export);
    args.extend(["-c".into(), s(&read_only), "--write".into()]);
    let out = hune(&args);
    assert_eq!(out.code, 3, "{}", out.stderr);
    assert!(out.stderr.contains("read_only"), "{}", out.stderr);
    let gone = fx.root.join("gone.hcl");
    fs::write(&gone, text(&fx.config).replace(&s(&fx.volume), &s(&fx.root.join("no-volume")))).unwrap();
    let mut args = fx.base(&fx.export);
    args.extend(["-c".into(), s(&gone)]);
    let out = hune(&args);
    assert_ne!(out.code, 0, "{}", out.stdout);
    assert!(out.stdout.is_empty() && !out.stderr.is_empty());
}
