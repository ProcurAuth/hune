// SPDX-License-Identifier: MIT OR Apache-2.0
//! The subcommands that work on a store: `export`, `import`, `copy-store`, `restore`,
//! `migrate-documents` and `migrate-store` (5.28, 5.37 to 5.40, 4.42).
//!
//! Each command reads the configuration file with the checks of section 13, opens the store
//! through `hune-store` and calls its functions. The store is the volume or PostgreSQL, as
//! the block `storage` says. The commands that write take the lock of the volume (5.16). A
//! command that only reads opens the volume read-only, so it takes no lock. A PostgreSQL store
//! gets the start checks of 5.29 and no retry: any failure stops the command with exit code 1.

mod migrate;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hune_core::ids::Uid;
use hune_store::{
    AnyStore, ImportReport, Message, PgStore, Store, StoreError, Subject, TransferError, UtcMillis, VolumeOptions,
    VolumeStore, WriteMeta, copy_store, export_to_dir, import_from_dir, migrate_store,
};

use super::{EXIT_CONFIG, EXIT_OK};
use crate::config::{self, CapturedEnvironment, LogLevel, PostgresConfig, PrimaryRef, StorageConfig};
use crate::log::Logger;
use crate::server::startup::pg_options;

/// A parsed command of this module.
#[derive(Debug, PartialEq, Eq)]
pub enum StoreCommand {
    /// `hune export -c <file> --to <dir>` (5.38).
    Export { config: PathBuf, to: PathBuf },
    /// `hune import -c <file> --from <dir>` (5.39).
    Import { config: PathBuf, from: PathBuf },
    /// `hune copy-store --from <file> --to <file>` (5.37).
    CopyStore { from: PathBuf, to: PathBuf },
    /// `hune restore -c <file> --uid <uid> [--version <n>]` (5.40).
    Restore { config: PathBuf, uid: String, version: Option<u64> },
    /// `hune migrate-documents -c <file> [--write]` (4.42).
    MigrateDocuments { config: PathBuf, write: bool },
    /// `hune migrate-store -c <file>` (5.28). The file names the owner role.
    MigrateStore { config: PathBuf },
}

/// The names of the commands of this module.
pub const NAMES: [&str; 6] = ["export", "import", "copy-store", "restore", "migrate-documents", "migrate-store"];

/// How an option takes its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Opt {
    Value,
    Switch,
}

/// Reads the options of a command. `spec` lists the long names, without `--`. `--config` also
/// has the short form `-c`. A value can follow as the next argument or after `=`.
fn parse_named(
    name: &str,
    args: &[OsString],
    spec: &[(&str, Opt)],
) -> Result<BTreeMap<String, Option<String>>, String> {
    let mut out: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let text = arg.to_str().ok_or_else(|| format!("{name}: an argument is not valid UTF-8"))?;
        let (flag, inline) = match text.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_owned())),
            _ => (text, None),
        };
        let long = if flag == "-c" { "--config" } else { flag };
        let Some((key, kind)) = spec.iter().find(|(k, _)| long.strip_prefix("--") == Some(*k)) else {
            return Err(if text.starts_with('-') {
                format!("{name}: unknown option {flag}")
            } else {
                format!("{name}: unexpected argument {text}")
            });
        };
        let value = match (kind, inline) {
            (Opt::Switch, None) => None,
            (Opt::Switch, Some(_)) => return Err(format!("{name}: the option --{key} takes no value")),
            (Opt::Value, Some(v)) => Some(v),
            (Opt::Value, None) => match iter.next() {
                Some(v) => Some(
                    v.to_str().ok_or_else(|| format!("{name}: the value of --{key} is not valid UTF-8"))?.to_owned(),
                ),
                None => return Err(format!("{name}: the option --{key} needs a value")),
            },
        };
        if value.as_deref() == Some("") {
            return Err(format!("{name}: the value of --{key} is empty"));
        }
        if out.insert((*key).to_owned(), value).is_some() {
            return Err(format!("{name}: the option --{key} is given two times"));
        }
    }
    Ok(out)
}

fn need(name: &str, map: &BTreeMap<String, Option<String>>, key: &str) -> Result<String, String> {
    map.get(key).and_then(Clone::clone).ok_or_else(|| format!("{name}: the option --{key} is necessary"))
}

/// Reads the command line of one command of this module. `name` is in `NAMES`.
pub fn parse(name: &str, rest: &[OsString]) -> Result<StoreCommand, String> {
    use Opt::{Switch, Value};
    Ok(match name {
        "export" => {
            let m = parse_named(name, rest, &[("config", Value), ("to", Value)])?;
            StoreCommand::Export { config: need(name, &m, "config")?.into(), to: need(name, &m, "to")?.into() }
        }
        "import" => {
            let m = parse_named(name, rest, &[("config", Value), ("from", Value)])?;
            StoreCommand::Import { config: need(name, &m, "config")?.into(), from: need(name, &m, "from")?.into() }
        }
        "copy-store" => {
            let m = parse_named(name, rest, &[("from", Value), ("to", Value)])?;
            StoreCommand::CopyStore { from: need(name, &m, "from")?.into(), to: need(name, &m, "to")?.into() }
        }
        "restore" => {
            let m = parse_named(name, rest, &[("config", Value), ("uid", Value), ("version", Value)])?;
            let version = match m.get("version").and_then(Clone::clone) {
                None => None,
                Some(v) => match v.parse::<u64>() {
                    Ok(n) if n >= 1 && !v.starts_with('0') && !v.starts_with('+') => Some(n),
                    _ => return Err(format!("{name}: --version must be a whole number from 1; value {v:?}")),
                },
            };
            StoreCommand::Restore { config: need(name, &m, "config")?.into(), uid: need(name, &m, "uid")?, version }
        }
        "migrate-documents" => {
            let m = parse_named(name, rest, &[("config", Value), ("write", Switch)])?;
            StoreCommand::MigrateDocuments { config: need(name, &m, "config")?.into(), write: m.contains_key("write") }
        }
        "migrate-store" => {
            let m = parse_named(name, rest, &[("config", Value)])?;
            StoreCommand::MigrateStore { config: need(name, &m, "config")?.into() }
        }
        other => return Err(format!("unknown command {other:?}")),
    })
}

/// Loads a configuration file with the read mode of the store commands (13.18): it reads only
/// the `_env` and `_file` values of the block `storage`. Each error goes to the standard error.
fn load(path: &Path, env: &CapturedEnvironment, err: &mut dyn Write) -> Option<config::LoadedConfig> {
    match config::load_with(path, env, config::ReadMode::StoreCommands) {
        Ok(loaded) => Some(loaded),
        Err(errors) => {
            for e in &errors {
                let _ = writeln!(err, "{e}");
            }
            None
        }
    }
}

/// Opens the PostgreSQL store of a configuration with the start checks of 5.29. The role check
/// of step 6 is for `serve` only. The text `<file>` of an error becomes `config_path`. The
/// function does not wait: a temporary error is an error here.
pub(super) async fn open_postgres(pg: &PostgresConfig, config_path: &Path) -> Result<AnyStore, String> {
    let options = pg_options(pg, false)?;
    PgStore::open(options)
        .await
        .map(AnyStore::Postgres)
        .map_err(|e| e.with_config_file(&config_path.display().to_string()).message)
}

/// The text of the error of a command that needs the primary store when the file has none (5.42).
pub(super) const NO_PRIMARY: &str =
    "the file has no primary store; the command works on the primary store, so give `primary = true` to one store";

/// Opens the primary store of a configuration: a command works on this store only (5.42). `read_only` opens a volume
/// without the lock and with no write. `config_path` is the file that gave `storage`: it names the file in the
/// errors of PostgreSQL.
async fn open(storage: &StorageConfig, read_only: bool, config_path: &Path) -> Result<AnyStore, String> {
    match storage.primary() {
        None => Err(NO_PRIMARY.to_owned()),
        Some(PrimaryRef::Volume(v)) => {
            let options = VolumeOptions {
                root: v.path.clone(),
                read_only,
                rescan_interval: v.rescan_interval,
                history_keep: v.history_keep.unwrap_or(crate::server::startup::NO_HISTORY_KEEP),
            };
            VolumeStore::open(options)
                .await
                .map(AnyStore::from)
                .map_err(|e| format!("storage.volume \"{}\": {e}", v.id))
        }
        Some(PrimaryRef::Postgres(pg)) => open_postgres(pg, config_path).await,
    }
}

fn now() -> UtcMillis {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
    UtcMillis::from_millis(ms)
}

fn print_report(out: &mut dyn Write, r: &ImportReport) {
    let _ = writeln!(out, "folders: {}", r.folders);
    let _ = writeln!(out, "dashboards: {}", r.dashboards);
    let _ = writeln!(out, "versions: {}", r.versions);
}

fn fail(err: &mut dyn Write, message: impl std::fmt::Display) -> u8 {
    let _ = writeln!(err, "{message}");
    EXIT_CONFIG
}

fn transfer_failed(err: &mut dyn Write, e: &TransferError) -> u8 {
    fail(err, e)
}

/// Runs a command of this module and gives the exit code.
pub fn run(cmd: StoreCommand, env: &CapturedEnvironment, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => return fail(err, format!("hune: cannot build the runtime: {e}")),
    };
    runtime.block_on(run_async(cmd, env, out, err))
}

async fn run_async(cmd: StoreCommand, env: &CapturedEnvironment, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match cmd {
        StoreCommand::Export { config, to } => {
            let Some(loaded) = load(&config, env, err) else { return EXIT_CONFIG };
            // A volume source is read without the lock (5.38, item 3).
            let store = match open(&loaded.config.storage, true, &config).await {
                Ok(s) => s,
                Err(e) => return fail(err, e),
            };
            match export_to_dir(&store, &to).await {
                Ok(report) => {
                    print_report(out, &report);
                    EXIT_OK
                }
                Err(e) => transfer_failed(err, &e),
            }
        }
        StoreCommand::Import { config, from } => {
            let Some(loaded) = load(&config, env, err) else { return EXIT_CONFIG };
            let store = match open(&loaded.config.storage, false, &config).await {
                Ok(s) => s,
                Err(e) => return fail(err, e),
            };
            match import_from_dir(&from, &store).await {
                Ok(report) => {
                    print_report(out, &report);
                    EXIT_OK
                }
                Err(e) => transfer_failed(err, &e),
            }
        }
        StoreCommand::CopyStore { from, to } => {
            let Some(source_cfg) = load(&from, env, err) else { return EXIT_CONFIG };
            let Some(target_cfg) = load(&to, env, err) else { return EXIT_CONFIG };
            let source = match open(&source_cfg.config.storage, true, &from).await {
                Ok(s) => s,
                Err(e) => return fail(err, format!("source: {e}")),
            };
            let target = match open(&target_cfg.config.storage, false, &to).await {
                Ok(s) => s,
                Err(e) => return fail(err, format!("target: {e}")),
            };
            match copy_store(&source, &target).await {
                Ok(report) => {
                    print_report(out, &report);
                    EXIT_OK
                }
                Err(e) => transfer_failed(err, &e),
            }
        }
        StoreCommand::Restore { config, uid, version } => {
            let Some(loaded) = load(&config, env, err) else { return EXIT_CONFIG };
            restore(&loaded, &config, &uid, version, out, err).await
        }
        StoreCommand::MigrateDocuments { config, write } => {
            let Some(loaded) = load(&config, env, err) else { return EXIT_CONFIG };
            migrate::run(&loaded, &config, write, out, err).await
        }
        StoreCommand::MigrateStore { config } => migrate_store_command(&config, env, out, err).await,
    }
}

/// `hune restore` (5.40).
async fn restore(
    loaded: &config::LoadedConfig,
    config_path: &Path,
    uid_text: &str,
    version: Option<u64>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let uid = match Uid::parse(uid_text) {
        Ok(u) => u,
        Err(e) => return fail(err, format!("--uid: {e}")),
    };
    let store = match open(&loaded.config.storage, false, config_path).await {
        Ok(s) => s,
        Err(e) => return fail(err, e),
    };
    match store.get(&uid).await {
        Err(StoreError::NotFound) => {}
        Err(StoreError::Unavailable(e)) => return fail(err, format!("the store does not answer: {e}")),
        Ok(_) | Err(_) => return fail(err, "the dashboard exists; restore a version in the editor"),
    }
    let number = match version {
        Some(n) => n,
        None => match highest_version_with_document(&store, &uid).await {
            Ok(Some(n)) => n,
            Ok(None) => return fail(err, format!("the dashboard {uid} has no version with a document")),
            Err(e) => return fail(err, e),
        },
    };
    let message = Message::parse(&format!("restore of version {number} by hune restore")).unwrap_or_default();
    let meta = WriteMeta { saved_by: Subject::anonymous(), saved_at: now(), message, save_id: None };
    match store.restore(&uid, number, None, None, &meta).await {
        Ok(saved) => {
            let _ = writeln!(out, "version: {}", saved.version);
            let _ = writeln!(out, "etag: {}", saved.etag.map_or_else(String::new, |e| e.to_hex()));
            EXIT_OK
        }
        Err(e) => fail(err, e),
    }
}

/// Finds the highest version of the uid that has a document (5.40, item 3).
async fn highest_version_with_document(store: &AnyStore, uid: &Uid) -> Result<Option<u64>, StoreError> {
    let mut before = None;
    loop {
        let page = store.history(uid, before, hune_store::MAX_HISTORY_PAGE).await?;
        if let Some(found) = page.iter().find(|m| m.etag.is_some() && m.size.is_some()) {
            return Ok(Some(found.version));
        }
        match page.last() {
            Some(last) if page.len() == hune_store::MAX_HISTORY_PAGE as usize => before = Some(last.version),
            _ => return Ok(None),
        }
    }
}

/// `hune migrate-store` (5.28). The file names the owner role. The command writes one
/// `store.migrated` line for each step that it applied, and then the report.
async fn migrate_store_command(
    config: &Path,
    env: &CapturedEnvironment,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let Some(loaded) = load(config, env, err) else { return EXIT_CONFIG };
    let Some(PrimaryRef::Postgres(pg)) = loaded.config.storage.primary() else {
        return fail(
            err,
            "migrate-store: the primary store is not the PostgreSQL store; this command needs `primary = true` on `storage.postgres`",
        );
    };
    let options = match pg_options(pg, false) {
        Ok(o) => o,
        Err(e) => return fail(err, e),
    };
    match migrate_store(&options).await {
        Ok(report) => {
            let log = Logger::memory(LogLevel::Info);
            for step in &report.applied {
                log.event(LogLevel::Info, "store.migrated").field("step", *step).emit();
            }
            for line in log.memory_lines() {
                let _ = writeln!(out, "{line}");
            }
            let _ = writeln!(out, "{report}");
            EXIT_OK
        }
        Err(e) => fail(err, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::run_with;
    use std::path::Path;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures");

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn run_text(list: &[&str]) -> (u8, String, String) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_with(args(list), &CapturedEnvironment::default(), &mut out, &mut err);
        (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
    }

    /// Writes a configuration file for a volume.
    /// A configuration of the volume `volume`. With `read_only`, the volume is a read-only store and the file has no
    /// primary store.
    fn config_for(dir: &Path, name: &str, volume: &Path, read_only: bool) -> PathBuf {
        crate::tests::files::put(dir.join("pw"), "pw\n");
        crate::tests::files::duplicate(Path::new(FIXTURES).join("config/ca-a.pem"), dir.join("ca.pem"));
        let mut text = std::fs::read_to_string(Path::new(FIXTURES).join("config/local.hcl"))
            .unwrap()
            .replace("/run/secrets/clickhouse/password", &dir.join("pw").display().to_string())
            .replace("/run/secrets/clickhouse/ca.pem", &dir.join("ca.pem").display().to_string())
            .replace("/var/lib/hune/dashboards", &volume.display().to_string());
        if read_only {
            text = text
                .replace("    primary         = true\n", "    title           = \"Main\"\n")
                .replace("    history_keep    = 100\n", "");
        }
        let path = dir.join(name);
        crate::tests::files::put(&path, text);
        path
    }

    fn seed(volume: &Path) {
        crate::tests::files::mkdirs(volume.join("transit"));
        crate::tests::files::put(
            volume.join("transit/_folder.json"),
            "{\n  \"format_version\": 1,\n  \"title\": \"Transit desk\"\n}\n",
        );
        crate::tests::files::put(
            volume.join("transit/nodes.json"),
            std::fs::read_to_string(Path::new(FIXTURES).join("dashboards/nodes.json")).unwrap(),
        );
    }

    fn volume(dir: &Path, name: &str) -> PathBuf {
        let v = dir.join(name);
        crate::tests::files::mkdirs(&v);
        std::fs::canonicalize(v).unwrap()
    }

    #[test]
    fn parses_the_commands() {
        let p = |name: &str, list: &[&str]| parse(name, &args(list));
        assert_eq!(
            p("export", &["-c", "/a", "--to", "/d"]),
            Ok(StoreCommand::Export { config: "/a".into(), to: "/d".into() })
        );
        assert_eq!(
            p("import", &["--config=/a", "--from", "/d"]),
            Ok(StoreCommand::Import { config: "/a".into(), from: "/d".into() })
        );
        assert_eq!(
            p("copy-store", &["--from", "/a", "--to=/b"]),
            Ok(StoreCommand::CopyStore { from: "/a".into(), to: "/b".into() })
        );
        assert_eq!(
            p("restore", &["-c", "/a", "--uid", "x", "--version", "7"]),
            Ok(StoreCommand::Restore { config: "/a".into(), uid: "x".into(), version: Some(7) })
        );
        assert_eq!(
            p("restore", &["-c", "/a", "--uid", "x"]),
            Ok(StoreCommand::Restore { config: "/a".into(), uid: "x".into(), version: None })
        );
        assert_eq!(
            p("migrate-documents", &["-c", "/a", "--write"]),
            Ok(StoreCommand::MigrateDocuments { config: "/a".into(), write: true })
        );
        assert_eq!(
            p("migrate-documents", &["-c", "/a"]),
            Ok(StoreCommand::MigrateDocuments { config: "/a".into(), write: false })
        );
        assert_eq!(p("migrate-store", &["-c", "/m"]), Ok(StoreCommand::MigrateStore { config: "/m".into() }));
        assert_eq!(p("migrate-store", &["--config=/m"]), Ok(StoreCommand::MigrateStore { config: "/m".into() }));
    }

    #[test]
    fn refuses_a_bad_command_line() {
        let cases: &[(&str, &[&str])] = &[
            ("export", &[]),
            ("export", &["-c", "/a"]),
            ("export", &["--to", "/d"]),
            ("export", &["-c", "/a", "--to", "/d", "extra"]),
            ("export", &["-c", "/a", "--to", "/d", "--to", "/e"]),
            ("export", &["-c", "/a", "--to"]),
            ("export", &["-c", "/a", "--to", ""]),
            ("export", &["-c", "/a", "--from", "/d"]),
            ("import", &["-c", "/a"]),
            ("copy-store", &["--from", "/a"]),
            ("copy-store", &["-c", "/a", "--from", "/a", "--to", "/b"]),
            ("restore", &["-c", "/a"]),
            ("restore", &["-c", "/a", "--uid", "x", "--version", "0"]),
            ("restore", &["-c", "/a", "--uid", "x", "--version", "x"]),
            ("restore", &["-c", "/a", "--uid", "x", "--version", "01"]),
            ("migrate-documents", &["-c", "/a", "--write=yes"]),
            ("migrate-documents", &["--write"]),
            ("migrate-store", &[]),
            ("migrate-store", &["-c", "/a", "--write"]),
            ("migrate-store", &["-c", "/a", "-c", "/b"]),
            ("migrate-store", &["-c", "/a", "extra"]),
        ];
        for (name, list) in cases {
            assert!(parse(name, &args(list)).is_err(), "{name} {list:?}");
        }
        for case in [&["export"][..], &["copy-store", "--to", "/b"][..]] {
            let (code, out, err) = run_text(case);
            assert_eq!((code, out.as_str()), (2, ""), "{case:?}");
            assert!(err.starts_with("hune: ") && err.contains("usage: hune"), "{err}");
        }
    }

    #[test]
    fn export_import_and_copy_store_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let d = std::fs::canonicalize(dir.path()).unwrap();
        let a = volume(&d, "a");
        seed(&a);
        let cfg_a = config_for(&d, "a.hcl", &a, false);
        let export_dir = d.join("exported");
        let (code, out, err) =
            run_text(&["export", "-c", cfg_a.to_str().unwrap(), "--to", export_dir.to_str().unwrap()]);
        assert_eq!((code, err.as_str()), (0, ""), "{err}");
        assert!(out.contains("folders: 1") && out.contains("dashboards: 1"), "{out}");
        assert_eq!(
            std::fs::read(export_dir.join("transit/nodes.json")).unwrap(),
            std::fs::read(a.join("transit/nodes.json")).unwrap()
        );
        assert!(export_dir.join("transit/_folder.json").exists());
        // The export of a directory that is not empty stops with 1.
        let (code, _, err) = run_text(&["export", "-c", cfg_a.to_str().unwrap(), "--to", export_dir.to_str().unwrap()]);
        assert_eq!(code, 1, "{err}");
        // Import into an empty volume, then a copy into another one.
        let b = volume(&d, "b");
        let cfg_b = config_for(&d, "b.hcl", &b, false);
        let (code, out, err) =
            run_text(&["import", "-c", cfg_b.to_str().unwrap(), "--from", export_dir.to_str().unwrap()]);
        assert_eq!((code, err.as_str()), (0, ""), "{err}");
        assert!(out.contains("dashboards: 1"), "{out}");
        assert_eq!(
            std::fs::read(b.join("transit/nodes.json")).unwrap(),
            std::fs::read(a.join("transit/nodes.json")).unwrap()
        );
        // The target is not empty now: import and copy stop with 1.
        let (code, _, _) = run_text(&["import", "-c", cfg_b.to_str().unwrap(), "--from", export_dir.to_str().unwrap()]);
        assert_eq!(code, 1);
        let c = volume(&d, "c");
        let cfg_c = config_for(&d, "c.hcl", &c, false);
        let (code, out, err) =
            run_text(&["copy-store", "--from", cfg_a.to_str().unwrap(), "--to", cfg_c.to_str().unwrap()]);
        assert_eq!((code, err.as_str()), (0, ""), "{err}");
        assert!(out.contains("folders: 1"), "{out}");
        assert_eq!(
            std::fs::read(c.join("transit/nodes.json")).unwrap(),
            std::fs::read(a.join("transit/nodes.json")).unwrap()
        );
        let (code, _, _) =
            run_text(&["copy-store", "--from", cfg_a.to_str().unwrap(), "--to", cfg_c.to_str().unwrap()]);
        assert_eq!(code, 1, "the target is not empty");
    }

    #[test]
    fn an_invalid_source_stops_before_any_write() {
        let dir = tempfile::tempdir().unwrap();
        let d = std::fs::canonicalize(dir.path()).unwrap();
        let a = volume(&d, "a");
        seed(&a);
        crate::tests::files::put(a.join("transit/broken.json"), b"{ not json");
        let cfg_a = config_for(&d, "a.hcl", &a, false);
        let export_dir = d.join("exported");
        let (code, _, err) = run_text(&["export", "-c", cfg_a.to_str().unwrap(), "--to", export_dir.to_str().unwrap()]);
        assert_eq!(code, 1);
        assert!(err.contains("broken"), "{err}");
        assert!(!export_dir.join("transit").exists(), "nothing is written");
    }

    #[test]
    fn restore_brings_a_deleted_dashboard_back() {
        let dir = tempfile::tempdir().unwrap();
        let d = std::fs::canonicalize(dir.path()).unwrap();
        let a = volume(&d, "a");
        seed(&a);
        let cfg = config_for(&d, "a.hcl", &a, false);
        let cfg = cfg.to_str().unwrap();
        // The dashboard exists: stop with 1.
        let (code, _, err) = run_text(&["restore", "-c", cfg, "--uid", "nodes"]);
        assert_eq!(code, 1);
        assert!(err.contains("the dashboard exists; restore a version in the editor"), "{err}");
        // Delete it through the store, then restore the highest version with a document.
        let deleted = delete_through_the_store(&a);
        assert!(deleted);
        let (code, out, err) = run_text(&["restore", "-c", cfg, "--uid", "nodes"]);
        assert_eq!((code, err.as_str()), (0, ""), "{err}");
        assert!(out.starts_with("version: 3\netag: "), "{out}");
        assert_eq!(
            std::fs::read(a.join("transit/nodes.json")).unwrap(),
            std::fs::read(Path::new(FIXTURES).join("dashboards/nodes.json")).unwrap()
        );
        // A uid that never existed, and a bad uid.
        let (code, _, _) = run_text(&["restore", "-c", cfg, "--uid", "never-seen"]);
        assert_eq!(code, 1);
        let (code, _, err) = run_text(&["restore", "-c", cfg, "--uid", "Bad_Uid"]);
        assert_eq!(code, 1);
        assert!(err.starts_with("--uid:"), "{err}");
        // A version that does not exist.
        delete_through_the_store(&a);
        let (code, _, _) = run_text(&["restore", "-c", cfg, "--uid", "nodes", "--version", "99"]);
        assert_eq!(code, 1);
        let (code, out, _) = run_text(&["restore", "-c", cfg, "--uid", "nodes", "--version", "1"]);
        assert_eq!(code, 0, "{out}");
    }

    fn delete_through_the_store(root: &Path) -> bool {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let store: AnyStore = VolumeStore::open(VolumeOptions {
                root: root.to_owned(),
                read_only: false,
                rescan_interval: std::time::Duration::from_secs(5),
                history_keep: 100,
            })
            .await
            .unwrap()
            .into();
            let uid = Uid::parse("nodes").unwrap();
            let current = store.get(&uid).await.unwrap();
            let meta = WriteMeta {
                saved_by: Subject::anonymous(),
                saved_at: now(),
                message: Message::default(),
                save_id: None,
            };
            store.delete(&uid, &current.etag, &current.folder, &meta).await.is_ok()
        })
    }

    #[test]
    fn restore_stops_when_serve_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let d = std::fs::canonicalize(dir.path()).unwrap();
        let a = volume(&d, "a");
        seed(&a);
        let cfg = config_for(&d, "a.hcl", &a, false);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let holder = rt.block_on(VolumeStore::open(VolumeOptions {
            root: a.clone(),
            read_only: false,
            rescan_interval: std::time::Duration::from_secs(5),
            history_keep: 100,
        }));
        assert!(holder.is_ok());
        let (code, _, err) = run_text(&["restore", "-c", cfg.to_str().unwrap(), "--uid", "nodes"]);
        assert_eq!(code, 1);
        assert!(err.contains("lock"), "{err}");
        // A read-only command takes no lock: the export works while the lock is held.
        let (code, _, err) = run_text(&["export", "-c", cfg.to_str().unwrap(), "--to", d.join("e").to_str().unwrap()]);
        assert_eq!(code, 0, "{err}");
        drop(holder);
    }

    fn format1_text(uid: &str) -> String {
        std::fs::read_to_string(Path::new(FIXTURES).join("dashboards/format1/nodes.json"))
            .unwrap()
            .replace("\"uid\": \"nodes\"", &format!("\"uid\": \"{uid}\""))
    }

    fn format2_text(uid: &str) -> String {
        std::fs::read_to_string(Path::new(FIXTURES).join("dashboards/format2/nodes.json"))
            .unwrap()
            .replace("\"uid\": \"nodes\"", &format!("\"uid\": \"{uid}\""))
    }

    /// Puts a document of format 1 in `transit` with one history version that holds the same bytes.
    fn seed_format1(volume: &Path, uid: &str) -> String {
        seed_text(volume, uid, format1_text(uid))
    }

    /// Puts a document of format 2 in `transit` with one history version that holds the same bytes.
    fn seed_format2(volume: &Path, uid: &str) -> String {
        seed_text(volume, uid, format2_text(uid))
    }

    fn seed_text(volume: &Path, uid: &str, text: String) -> String {
        crate::tests::files::put(volume.join(format!("transit/{uid}.json")), &text);
        let hist = volume.join(format!(".history/{uid}"));
        crate::tests::files::mkdirs(&hist);
        crate::tests::files::put(hist.join("1.json"), &text);
        let etag = hune_store::Etag::of(text.as_bytes()).to_hex();
        crate::tests::files::put(
            hist.join("1.meta.json"),
            format!(
                "{{\n  \"format_version\": 1,\n  \"uid\": \"{uid}\",\n  \"version\": 1,\n  \"kind\": \"save\",\n  \"folder\": \"transit\",\n  \"etag\": \"{etag}\",\n  \"saved_at\": \"2026-11-02T09:00:00.000Z\",\n  \"saved_by\": \"user:bob\",\n  \"message\": \"old\",\n  \"save_id\": null\n}}\n"
            ),
        );
        text
    }

    fn open_volume(root: &Path) -> AnyStore {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(VolumeStore::open(VolumeOptions {
            root: root.to_owned(),
            read_only: true,
            rescan_interval: std::time::Duration::from_secs(5),
            history_keep: 100,
        }))
        .unwrap()
        .into()
    }

    /// Waits until nobody holds the lock of the volume at `root`, for 5 s at most. A task of a store that blocks a thread can keep the
    /// volume for a short time after the store is dropped, so a command that runs at once after the drop could find the lock still taken.
    fn wait_until_unlocked(root: &Path) {
        // The file is open for reading only: a lock of flock needs no write right.
        let file = std::fs::File::open(root.join(".hune/lock")).unwrap();
        for _ in 0..100 {
            if file.try_lock().is_ok() {
                file.unlock().unwrap();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("the volume stays locked for 5 s after the store was dropped");
    }

    fn history_of(root: &Path, uid: &str) -> Vec<hune_store::VersionMeta> {
        let store = open_volume(root);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(store.history(&Uid::parse(uid).unwrap(), None, 100)).unwrap()
    }

    #[test]
    fn migrate_documents_dry_run_reports_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let d = std::fs::canonicalize(dir.path()).unwrap();
        let a = volume(&d, "a");
        seed(&a);
        let old = seed_format1(&a, "old");
        let mid = seed_format2(&a, "mid");
        let cfg = config_for(&d, "a.hcl", &a, false);
        let cfg = cfg.to_str().unwrap();
        let (code, out, err) = run_text(&["migrate-documents", "-c", cfg]);
        assert_eq!((code, err.as_str()), (0, ""), "{err}");
        assert!(out.contains("transit/old: format 1, migrates to format 3"), "{out}");
        assert!(out.contains("transit/mid: format 2, migrates to format 3"), "{out}");
        assert!(out.contains("transit/nodes: format 3, no migration needed"), "{out}");
        assert!(out.contains("3 documents: 2 to migrate, 1 at format 3 (dry run); nothing is written"), "{out}");
        assert_eq!(std::fs::read_to_string(a.join("transit/old.json")).unwrap(), old);
        assert_eq!(std::fs::read_to_string(a.join("transit/mid.json")).unwrap(), mid);
        assert_eq!(history_of(&a, "old").len(), 1);
        assert_eq!(history_of(&a, "mid").len(), 1);
        // The dry run takes no lock: it works while another process holds it.
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let holder = rt.block_on(VolumeStore::open(VolumeOptions {
            root: a.clone(),
            read_only: false,
            rescan_interval: std::time::Duration::from_secs(5),
            history_keep: 100,
        }));
        assert!(holder.is_ok());
        let (code, _, err) = run_text(&["migrate-documents", "-c", cfg]);
        assert_eq!((code, err.as_str()), (0, ""), "{err}");
        // `--write` takes the lock (4.42, item 13): it stops while serve holds it.
        let (code, _, err) = run_text(&["migrate-documents", "-c", cfg, "--write"]);
        assert_eq!(code, 1);
        assert!(err.contains("lock"), "{err}");
        drop(holder);
        assert_eq!(std::fs::read_to_string(a.join("transit/old.json")).unwrap(), old);
        assert_eq!(std::fs::read_to_string(a.join("transit/mid.json")).unwrap(), mid);
    }

    #[test]
    fn migrate_documents_write_saves_one_migrate_version_for_each_document() {
        let dir = tempfile::tempdir().unwrap();
        let d = std::fs::canonicalize(dir.path()).unwrap();
        let a = volume(&d, "a");
        seed(&a);
        let old = seed_format1(&a, "old");
        seed_format2(&a, "mid");
        let cfg = config_for(&d, "a.hcl", &a, false);
        let cfg = cfg.to_str().unwrap();
        let (code, out, err) = run_text(&["migrate-documents", "-c", cfg, "--write"]);
        assert_eq!((code, err.as_str()), (0, ""), "{err}");
        assert!(out.contains("transit/old: migrated to format 3, version 2, etag "), "{out}");
        assert!(out.contains("transit/mid: migrated to format 3, version 2, etag "), "{out}");
        assert!(out.contains("3 documents: 2 migrated, 1 already at format 3"), "{out}");
        // The files are the canonical document of format 3: the same bytes as the golden file. The
        // chain 1 to 2 to 3 gives the same bytes as the step 2 to 3.
        let golden = std::fs::read_to_string(Path::new(FIXTURES).join("dashboards/nodes.json")).unwrap();
        let expected = golden.replace("\"uid\": \"nodes\"", "\"uid\": \"old\"");
        assert_eq!(std::fs::read_to_string(a.join("transit/old.json")).unwrap(), expected);
        let expected_mid = golden.replace("\"uid\": \"nodes\"", "\"uid\": \"mid\"");
        assert_eq!(std::fs::read_to_string(a.join("transit/mid.json")).unwrap(), expected_mid);
        // One new version: kind migrate, saved by "-", the message "format 1 to 3" for the chain
        // and "format 2 to 3" for one step (4.42, item 9).
        let h = history_of(&a, "old");
        assert_eq!(h.len(), 2, "{h:?}");
        assert_eq!((h[0].version, h[0].kind), (2, hune_store::VersionKind::Migrate));
        assert_eq!((h[0].saved_by.as_str(), h[0].message.as_str()), ("-", "format 1 to 3"));
        assert_eq!(h[0].etag.unwrap(), hune_store::Etag::of(expected.as_bytes()));
        let h = history_of(&a, "mid");
        assert_eq!(h.len(), 2, "{h:?}");
        assert_eq!((h[0].saved_by.as_str(), h[0].message.as_str()), ("-", "format 2 to 3"));
        assert_eq!(h[0].etag.unwrap(), hune_store::Etag::of(expected_mid.as_bytes()));
        // The older version keeps its bytes and its format (item 10), and a restore refuses it.
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert_eq!(std::fs::read_to_string(a.join(".history/old/1.json")).unwrap(), old);
        let store: AnyStore = rt
            .block_on(VolumeStore::open(VolumeOptions {
                root: a.clone(),
                read_only: false,
                rescan_interval: std::time::Duration::from_secs(5),
                history_keep: 100,
            }))
            .unwrap()
            .into();
        let uid = Uid::parse("old").unwrap();
        let v1 = rt.block_on(store.version(&uid, 1)).unwrap();
        assert_eq!(v1.bytes.as_deref(), Some(old.as_bytes()));
        let current = rt.block_on(store.get(&uid)).unwrap();
        let meta =
            WriteMeta { saved_by: Subject::anonymous(), saved_at: now(), message: Message::default(), save_id: None };
        let r = rt.block_on(store.restore(&uid, 1, Some(&current.etag), Some(&current.folder), &meta));
        assert_eq!(r, Err(StoreError::NotRestorable));
        drop(store);
        wait_until_unlocked(&a);
        // The document that was format 3 got no write.
        assert_eq!(history_of(&a, "nodes").len(), 0);
        // A second run has nothing to migrate and writes nothing.
        let (code, out, err) = run_text(&["migrate-documents", "-c", cfg, "--write"]);
        assert_eq!((code, err.as_str()), (0, ""), "{err}");
        assert!(out.contains("3 documents: 0 migrated, 3 already at format 3"), "{out}");
        assert_eq!(history_of(&a, "old").len(), 2);
        assert_eq!(history_of(&a, "mid").len(), 2);
    }

    #[test]
    fn migrate_documents_writes_nothing_when_a_document_of_an_older_format_fails() {
        let dir = tempfile::tempdir().unwrap();
        let d = std::fs::canonicalize(dir.path()).unwrap();
        let a = volume(&d, "a");
        seed(&a);
        let good = seed_format1(&a, "good");
        // A format 1 document with a bad member does not migrate (4.42, item 8).
        let bad = format1_text("bad").replace("\"title\": \"Nodes\"", "\"title\": 5");
        crate::tests::files::put(a.join("transit/bad.json"), &bad);
        // A format 2 document with a bad member does not migrate either.
        let bad2 = format2_text("bad2").replace("\"title\": \"Nodes\"", "\"title\": 6");
        crate::tests::files::put(a.join("transit/bad2.json"), &bad2);
        let cfg = config_for(&d, "a.hcl", &a, false);
        let cfg = cfg.to_str().unwrap();
        for extra in [&[][..], &["--write"][..]] {
            let mut list = vec!["migrate-documents", "-c", cfg];
            list.extend_from_slice(extra);
            let (code, _, err) = run_text(&list);
            assert_eq!(code, 1, "{err}");
            assert!(err.contains("transit/bad: /title wrong_type"), "{err}");
            assert!(err.contains("transit/bad2: /title wrong_type"), "{err}");
            assert!(err.ends_with("nothing is written\n"), "{err}");
            assert_eq!(std::fs::read_to_string(a.join("transit/good.json")).unwrap(), good);
            assert_eq!(std::fs::read_to_string(a.join("transit/bad.json")).unwrap(), bad);
            assert_eq!(history_of(&a, "good").len(), 1);
        }
    }

    #[test]
    fn migrate_documents_warns_about_the_documents_that_it_does_not_migrate() {
        let dir = tempfile::tempdir().unwrap();
        let d = std::fs::canonicalize(dir.path()).unwrap();
        let a = volume(&d, "a");
        seed(&a);
        seed_format1(&a, "good");
        // A document of format 4, a broken file and a document of format 3 that is not valid.
        let future = format1_text("future").replace("\"format_version\": 1", "\"format_version\": 4");
        crate::tests::files::put(a.join("transit/future.json"), &future);
        crate::tests::files::put(a.join("transit/broken.json"), b"{ not json");
        let invalid = std::fs::read_to_string(Path::new(FIXTURES).join("dashboards/nodes.json"))
            .unwrap()
            .replace("\"uid\": \"nodes\"", "\"uid\": \"flawed\"")
            .replace("\"title\": \"Nodes\"", "\"title\": 5");
        crate::tests::files::put(a.join("transit/flawed.json"), &invalid);
        let cfg = config_for(&d, "a.hcl", &a, false);
        let cfg = cfg.to_str().unwrap();
        let (code, out, err) = run_text(&["migrate-documents", "-c", cfg, "--write"]);
        assert_eq!((code, out.contains("transit/good: migrated to format 3")), (0, true), "{out}{err}");
        assert!(out.contains("5 documents: 1 migrated, 1 already at format 3, 3 not valid and not migrated"), "{out}");
        assert!(
            err.contains("warning: transit/future: /format_version format_version: not loadable: format 4"),
            "{err}"
        );
        assert!(err.contains("warning: transit/broken:"), "{err}");
        assert!(err.contains("warning: transit/flawed: /title wrong_type"), "{err}");
        // The title of the document and the title of its last panel have the same text: two errors for `flawed`.
        assert_eq!(err.matches("this document is not migrated").count(), 4, "{err}");
        // The documents that were skipped stay as they are.
        assert_eq!(std::fs::read_to_string(a.join("transit/future.json")).unwrap(), future);
        assert_eq!(std::fs::read_to_string(a.join("transit/flawed.json")).unwrap(), invalid);
        assert!(history_of(&a, "flawed").is_empty());
    }

    #[test]
    fn a_command_needs_the_primary_store_so_a_file_with_read_only_stores_only_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let d = std::fs::canonicalize(dir.path()).unwrap();
        let a = volume(&d, "a");
        seed(&a);
        let old = seed_format1(&a, "old");
        let cfg = config_for(&d, "a.hcl", &a, true);
        let cfg = cfg.to_str().unwrap();
        let (code, _, err) = run_text(&["migrate-documents", "-c", cfg, "--write"]);
        assert_eq!(code, 1);
        assert!(err.contains("the file has no primary store"), "{err}");
        // A dry run works on the primary store too, so it is refused as well.
        let (code, _, err) = run_text(&["migrate-documents", "-c", cfg]);
        assert_eq!(code, 1);
        assert!(err.contains("the file has no primary store"), "{err}");
        assert_eq!(std::fs::read_to_string(a.join("transit/old.json")).unwrap(), old);
    }
}
