// SPDX-License-Identifier: MIT OR Apache-2.0
//! The subcommand `hune import-grafana` (section 15).
//!
//! The command reads the exported Grafana files, calls the pure importer of `hune-grafana`,
//! prints a summary, writes the report files, and with `--write` writes the documents through
//! the `Store` trait. The importer crate does no I/O. This module reads the files and does the
//! JSON-level checks of 15.4: size, byte order mark, duplicate member, symbolic link and
//! `SHA256SUMS`.
//!
//! Exit codes (15.12): 0 success, 1 an item needs acceptance or a line is stale, 2 usage or
//! input error, 3 write refused or failed.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hune_core::document::{CanonicalDocument, validate};
use hune_core::etag::Etag;
use hune_core::ids::is_valid_id;
use hune_grafana::{
    DashboardFile, DashboardResult, FolderResult, FolderTable, ImportInput, ImportOptions, ImportResult, LinkPrefix,
    MongoLists, ReportMeta, SourceKind, import_all, parse_acceptance_file, parse_folder_map, render_nginx_map,
    render_report,
};
use hune_store::{
    AnyStore, CatalogEntry, FolderId, FolderState, FolderTitle, Message, OpenErrorKind, Outcome, SaveId, Snapshot,
    Store, StoreError, Subject, Uid, UtcMillis, VersionKind, VolumeOptions, VolumeStore, WriteMeta,
};
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

use super::output::{self, Target};
use crate::config::{self, CapturedEnvironment, LoadedConfig, StorageConfig};

/// Exit code: an item needs acceptance, or an acceptance line is stale.
const EXIT_ACCEPT: u8 = 1;
/// Exit code: usage or input error.
const EXIT_INPUT: u8 = 2;
/// Exit code: the store refused or failed a write.
const EXIT_WRITE: u8 = 3;

/// The size limit of one input file (15.4).
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
/// The most dashboard files (15.4).
const MAX_DASHBOARDS: usize = 1000;
/// The most source map options (15.2).
const MAX_SOURCE_MAPS: usize = 64;
/// The most items that the summary lists.
const MAX_LISTED_ITEMS: usize = 50;

/// The names of the input directory that the importer reads (15.4).
const KNOWN_NAMES: [&str; 8] = [
    "dashboards",
    "folders.json",
    "search.json",
    "health.json",
    "plugin-clickhouse.json",
    "public-dashboards.json",
    "short-urls.json",
    "SHA256SUMS",
];

/// A parsed command line of `hune import-grafana`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCommand {
    /// `--in`: the export directory.
    pub input: PathBuf,
    /// `--source-map`: the Grafana key and the source id, in the order of the command line. The key is a data source
    /// uid or name, a data source variable or an item of `__inputs` (15.9).
    pub source_map: Vec<(String, String)>,
    /// `--folders`: the folder map file.
    pub folders: Option<PathBuf>,
    /// `--link-prefix`: the old and the new prefix.
    pub link_prefix: Option<(String, String)>,
    /// `--accept`: the acceptance file.
    pub accept: Option<PathBuf>,
    /// `--report`: the path without extension.
    pub report: Option<PathBuf>,
    /// `--nginx-map`: the map file.
    pub nginx_map: Option<PathBuf>,
    /// `--config`: the configuration file.
    pub config: Option<PathBuf>,
    /// `--write`.
    pub write: bool,
    /// `--replace`.
    pub replace: bool,
}

/// Reads the command line after the name of the command (15.2). The message goes with the usage text.
pub fn parse(rest: &[OsString]) -> Result<ImportCommand, String> {
    let name = "import-grafana";
    let mut single: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut source_maps: Vec<String> = Vec::new();
    let (mut write, mut replace) = (false, false);
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        let text = arg.to_str().ok_or_else(|| format!("{name}: an argument is not valid UTF-8"))?;
        let (flag, inline) = match text.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_owned())),
            _ => (text, None),
        };
        let flag = if flag == "-c" { "--config" } else { flag };
        let switch = matches!(flag, "--write" | "--replace");
        let key: &'static str = match flag {
            "--in" => "--in",
            "--source-map" => "--source-map",
            "--folders" => "--folders",
            "--link-prefix" => "--link-prefix",
            "--accept" => "--accept",
            "--report" => "--report",
            "--nginx-map" => "--nginx-map",
            "--config" => "--config",
            "--write" => "--write",
            "--replace" => "--replace",
            _ if text.starts_with('-') => return Err(format!("{name}: unknown option {flag}")),
            _ => return Err(format!("{name}: unexpected argument {text}")),
        };
        if switch {
            if inline.is_some() {
                return Err(format!("{name}: the option {key} takes no value"));
            }
            let slot = if key == "--write" { &mut write } else { &mut replace };
            if *slot {
                return Err(format!("{name}: the option {key} is given two times"));
            }
            *slot = true;
            continue;
        }
        let value = match inline {
            Some(v) => v,
            None => iter
                .next()
                .ok_or_else(|| format!("{name}: the option {key} needs a value"))?
                .to_str()
                .ok_or_else(|| format!("{name}: the value of {key} is not valid UTF-8"))?
                .to_owned(),
        };
        if value.is_empty() {
            return Err(format!("{name}: the value of {key} is empty"));
        }
        if key == "--source-map" {
            source_maps.push(value);
        } else if single.insert(key, value).is_some() {
            return Err(format!("{name}: the option {key} is given two times"));
        }
    }
    let input = single.remove("--in").ok_or_else(|| format!("{name}: the option --in <dir> is necessary"))?;
    if source_maps.is_empty() || source_maps.len() > MAX_SOURCE_MAPS {
        return Err(format!(
            "{name}: --source-map is necessary, 1 to {MAX_SOURCE_MAPS} times; found {} times",
            source_maps.len()
        ));
    }
    let mut source_map: Vec<(String, String)> = Vec::new();
    for entry in source_maps {
        let (uid, id) = match entry.split_once('=') {
            Some((u, i)) if !u.is_empty() && !i.is_empty() => (u, i),
            _ => return Err(format!("{name}: --source-map must be <grafana-key>=<source-id>; value {entry:?}")),
        };
        if !is_valid_id(id) {
            return Err(format!(
                "{name}: --source-map: the source id must match ^[a-z0-9][a-z0-9-]{{0,39}}$; value {id:?}"
            ));
        }
        if source_map.iter().any(|(u, _)| u == uid) {
            return Err(format!("{name}: --source-map: the Grafana key {uid:?} is given two times"));
        }
        source_map.push((uid.to_owned(), id.to_owned()));
    }
    let link_prefix = match single.remove("--link-prefix") {
        None => None,
        Some(v) => {
            let (old, new) =
                v.split_once('=').ok_or_else(|| format!("{name}: --link-prefix must be <old>=<new>; value {v:?}"))?;
            LinkPrefix::new(old, new).map_err(|e| format!("{name}: {e}"))?;
            Some((old.to_owned(), new.to_owned()))
        }
    };
    let path = |single: &mut BTreeMap<&'static str, String>, key: &str| single.remove(key).map(PathBuf::from);
    let cmd = ImportCommand {
        input: PathBuf::from(input),
        source_map,
        folders: path(&mut single, "--folders"),
        link_prefix,
        accept: path(&mut single, "--accept"),
        report: path(&mut single, "--report"),
        nginx_map: path(&mut single, "--nginx-map"),
        config: path(&mut single, "--config"),
        write,
        replace,
    };
    if cmd.replace && !cmd.write {
        return Err(format!("{name}: the option --replace needs --write"));
    }
    if cmd.write && cmd.config.is_none() {
        return Err(format!("{name}: the option --write needs --config <path>"));
    }
    if cmd.nginx_map.is_some() && cmd.link_prefix.is_none() {
        return Err(format!("{name}: the option --nginx-map needs --link-prefix"));
    }
    Ok(cmd)
}

// ---------------------------------------------------------------------------------------------
// The input directory (15.4)
// ---------------------------------------------------------------------------------------------

/// A JSON reader that fails on a duplicate member. It keeps no value.
struct NoDuplicate;

impl<'de> Deserialize<'de> for NoDuplicate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NoDuplicateVisitor)
    }
}

struct NoDuplicateVisitor;

impl<'de> Visitor<'de> for NoDuplicateVisitor {
    type Value = NoDuplicate;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON value")
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<NoDuplicate, E> {
        Ok(NoDuplicate)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<NoDuplicate, E> {
        Ok(NoDuplicate)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<NoDuplicate, E> {
        Ok(NoDuplicate)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<NoDuplicate, E> {
        Ok(NoDuplicate)
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<NoDuplicate, E> {
        Ok(NoDuplicate)
    }
    fn visit_unit<E: de::Error>(self) -> Result<NoDuplicate, E> {
        Ok(NoDuplicate)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<NoDuplicate, A::Error> {
        while seq.next_element::<NoDuplicate>()?.is_some() {}
        Ok(NoDuplicate)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<NoDuplicate, A::Error> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom(format!("duplicate member {key:?}")));
            }
            map.next_value::<NoDuplicate>()?;
        }
        Ok(NoDuplicate)
    }
}

/// Parses the bytes of one JSON file with the rules of 15.4: no byte order mark, valid UTF-8,
/// one value, no data after it, no duplicate member. The label names the file in each message.
fn parse_json(bytes: &[u8], label: &str) -> Result<Value, String> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Err(format!("{label}: the file starts with a byte order mark (15.4)"));
    }
    let text = std::str::from_utf8(bytes).map_err(|e| format!("{label}: the file is not valid UTF-8: {e}"))?;
    let value: Value = serde_json::from_str(text).map_err(|e| format!("{label}: not valid JSON: {e}"))?;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    NoDuplicate::deserialize(&mut deserializer).map_err(|e| format!("{label}: {e}"))?;
    Ok(value)
}

/// Reads one file of at most 32 MiB. A symbolic link or another type of file is an error.
fn read_limited(path: &Path, label: &str, follow_links: bool) -> Result<Vec<u8>, String> {
    let meta = if follow_links { fs::metadata(path) } else { fs::symlink_metadata(path) }
        .map_err(|e| format!("{label}: cannot read the file: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{label}: a symbolic link is not permitted (15.4)"));
    }
    if !meta.is_file() {
        return Err(format!("{label}: not a regular file"));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("{label}: the file has {} bytes; the limit is {MAX_FILE_BYTES} bytes (15.4)", meta.len()));
    }
    let file = fs::File::open(path).map_err(|e| format!("{label}: cannot read the file: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("{label}: cannot read the file: {e}"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("{label}: the file is larger than {MAX_FILE_BYTES} bytes (15.4)"));
    }
    Ok(bytes)
}

/// Reads `SHA256SUMS`: the output of `sha256sum`, one `<hex>  <path>` line for each file.
fn parse_sums(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut listed = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let fail = |why: &str| format!("SHA256SUMS, line {}: {why}; value {line:?}", n + 1);
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let bytes = line.as_bytes();
        if bytes.len() < 67 || !bytes[..64].iter().all(u8::is_ascii_hexdigit) || bytes[64] != b' ' {
            return Err(fail("expected <64 hex characters>, a space, a mode character and a path"));
        }
        if bytes[65] != b' ' && bytes[65] != b'*' {
            return Err(fail("expected a space or * before the path"));
        }
        let path = &line[66..];
        let mut name = path;
        while let Some(rest) = name.strip_prefix("./") {
            name = rest;
        }
        let parts: Vec<&str> = name.split('/').collect();
        let plain = !name.starts_with('/')
            && !name.contains('\\')
            && parts.len() <= 2
            && parts.iter().all(|p| !p.is_empty() && *p != "." && *p != "..");
        if !plain {
            return Err(fail("the path must be relative, with at most one directory level"));
        }
        if listed.insert(name.to_owned(), line[..64].to_ascii_lowercase()).is_some() {
            return Err(fail("the path is listed two times"));
        }
    }
    Ok(listed)
}

/// Reads the files of the input directory and checks `SHA256SUMS` before each parse.
struct Reader<'a> {
    dir: &'a Path,
    listed: BTreeMap<String, String>,
    checked: BTreeSet<String>,
}

impl Reader<'_> {
    /// Reads a file and gives its bytes and its SHA-256. A listed file must match its digest.
    fn read(&mut self, name: &str) -> Result<(Vec<u8>, String), String> {
        let bytes = read_limited(&self.dir.join(name), name, false)?;
        let sha = Etag::of(&bytes).to_hex();
        if let Some(want) = self.listed.get(name) {
            if *want != sha {
                return Err(format!("{name}: the SHA-256 {sha} differs from the line of SHA256SUMS ({want}) (15.4)"));
            }
            self.checked.insert(name.to_owned());
        }
        Ok((bytes, sha))
    }

    fn json(&mut self, name: &str) -> Result<(Value, String), String> {
        let (bytes, sha) = self.read(name)?;
        Ok((parse_json(&bytes, name)?, sha))
    }
}

/// The input directory as values, with what the store step needs.
struct InputRead {
    input: ImportInput,
    folders_json: Value,
    /// The file name and the SHA-256 of each dashboard file, by Grafana uid (the file stem).
    sources: BTreeMap<String, (String, String)>,
}

fn list_dir(dir: &Path, label: &str) -> Result<Vec<(String, fs::FileType)>, String> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("{label}: cannot read the directory: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{label}: cannot read the directory: {e}"))?;
        let name =
            entry.file_name().into_string().map_err(|n| format!("{label}: the name {n:?} is not valid UTF-8"))?;
        let kind = entry.file_type().map_err(|e| format!("{label}/{name}: cannot read the file type: {e}"))?;
        if kind.is_symlink() {
            return Err(format!("{label}/{name}: a symbolic link is not permitted (15.4)"));
        }
        out.push((name, kind));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn read_input(dir: &Path) -> Result<InputRead, String> {
    let shown = dir.display().to_string();
    if !fs::metadata(dir).is_ok_and(|m| m.is_dir()) {
        return Err(format!("--in: {shown} is not a directory"));
    }
    let top = list_dir(dir, &shown)?;
    let mut ignored: Vec<String> = Vec::new();
    for (name, kind) in &top {
        if !KNOWN_NAMES.contains(&name.as_str()) {
            ignored.push(name.clone());
        } else if (name == "dashboards") != kind.is_dir() || (name != "dashboards" && !kind.is_file()) {
            return Err(format!(
                "{shown}/{name}: expected {}",
                if name == "dashboards" { "a directory" } else { "a file" }
            ));
        }
    }
    let has = |name: &str| top.iter().any(|(n, _)| n == name);
    for need in ["dashboards", "folders.json", "health.json", "plugin-clickhouse.json"] {
        if !has(need) {
            return Err(format!("{shown}: the necessary name {need} is absent (15.4)"));
        }
    }
    let mut files: Vec<String> = Vec::new();
    for (name, kind) in list_dir(&dir.join("dashboards"), &format!("{shown}/dashboards"))? {
        if kind.is_file() && name.ends_with(".json") {
            files.push(name);
        } else {
            ignored.push(format!("dashboards/{name}"));
        }
    }
    if files.is_empty() || files.len() > MAX_DASHBOARDS {
        return Err(format!("{shown}/dashboards: expected 1 to {MAX_DASHBOARDS} files *.json; found {}", files.len()));
    }
    let listed = if has("SHA256SUMS") {
        let bytes = read_limited(&dir.join("SHA256SUMS"), "SHA256SUMS", false)?;
        let text = String::from_utf8(bytes).map_err(|_| "SHA256SUMS: the file is not valid UTF-8".to_owned())?;
        parse_sums(&text)?
    } else {
        BTreeMap::new()
    };
    let mut reader = Reader { dir, listed, checked: BTreeSet::new() };
    let optional = |reader: &mut Reader<'_>, name: &str| -> Result<Option<Value>, String> {
        if has(name) { Ok(Some(reader.json(name)?.0)) } else { Ok(None) }
    };
    let (folders_json, _) = reader.json("folders.json")?;
    let health = optional(&mut reader, "health.json")?;
    let plugin = optional(&mut reader, "plugin-clickhouse.json")?;
    let search = optional(&mut reader, "search.json")?;
    let public_dashboards = optional(&mut reader, "public-dashboards.json")?;
    let short_urls = optional(&mut reader, "short-urls.json")?;
    let mut dashboards = Vec::new();
    let mut sources = BTreeMap::new();
    for file in &files {
        let name = format!("dashboards/{file}");
        let (value, sha) = reader.json(&name)?;
        let stem = file.trim_end_matches(".json").to_owned();
        sources.insert(stem.clone(), (name.clone(), sha.clone()));
        dashboards.push(DashboardFile { name, stem, sha256: Some(sha), value });
    }
    // A listed file that the importer does not read is checked too.
    let rest: Vec<String> = reader.listed.keys().filter(|k| !reader.checked.contains(*k)).cloned().collect();
    for name in rest {
        reader.read(&name).map_err(|e| format!("{e} (listed in SHA256SUMS)"))?;
    }
    ignored.sort();
    Ok(InputRead {
        input: ImportInput {
            dashboards,
            search,
            health,
            plugin,
            public_dashboards,
            short_urls,
            ignored_files: ignored,
        },
        folders_json,
        sources,
    })
}

// ---------------------------------------------------------------------------------------------
// The store step (15.13)
// ---------------------------------------------------------------------------------------------

/// What the command does for one folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderAct {
    None,
    Create,
    Rename,
}

/// What the command does for one dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Act {
    None,
    Create,
    Save { if_match: Etag, folder: String },
    Move { if_match: Etag, from: String },
    MoveSave { if_match: Etag, from: String },
}

/// Gives the `save_id` of one transition: a UUID version 8 from a SHA-256 (15.13).
fn save_id(transition: &str) -> SaveId {
    let hash = Etag::of(transition.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    SaveId::from_bytes(bytes)
}

fn now() -> UtcMillis {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
    UtcMillis::from_millis(ms)
}

/// The metadata of a write: author `import-grafana`, message `import-grafana <file> <sha12>`.
fn write_meta(file: &str, sha: &str, transition: &str) -> WriteMeta {
    let short = &sha[..sha.len().min(12)];
    let message = Message::parse(&format!("import-grafana {file} {short}"))
        .or_else(|_| Message::parse(&format!("import-grafana {short}")))
        .unwrap_or_default();
    WriteMeta { saved_by: Subject::import_grafana(), saved_at: now(), message, save_id: Some(save_id(transition)) }
}

/// Compares the result of the conversion with the store and sets the results (15.13, step 3).
///
/// `write` selects the refusal result: `refused_exists` with `--write`, `would_refuse` in a dry run.
fn plan(
    result: &mut ImportResult,
    snapshot: &Snapshot,
    deleted: &BTreeSet<String>,
    write: bool,
    replace: bool,
) -> Result<(Vec<FolderAct>, Vec<Act>), String> {
    let mut stored: BTreeMap<&str, &FolderState> = BTreeMap::new();
    for f in &snapshot.folders {
        stored.insert(f.id().as_str(), f);
    }
    let mut folder_acts = Vec::new();
    for f in &mut result.folders {
        match stored.get(f.id.as_str()) {
            None => {
                f.result = FolderResult::WouldCreate;
                folder_acts.push(FolderAct::Create);
            }
            Some(FolderState::Valid(r)) if r.title.as_str() == f.title => {
                f.result = FolderResult::Exists;
                folder_acts.push(FolderAct::None);
            }
            Some(FolderState::Valid(_)) => {
                f.result = if write && !replace { FolderResult::RefusedTitle } else { FolderResult::WouldRename };
                folder_acts.push(FolderAct::Rename);
            }
            Some(FolderState::Invalid { id, errors }) => {
                let why = errors.first().map_or_else(String::new, ToString::to_string);
                return Err(format!("the folder {id} of the store is not valid: {why}"));
            }
        }
    }
    let refuse = if write { DashboardResult::RefusedExists } else { DashboardResult::WouldRefuse };
    let mut acts = Vec::new();
    for d in &mut result.dashboards {
        if d.document_bytes.is_none() || d.result != DashboardResult::WouldWrite {
            acts.push(Act::None);
            continue;
        }
        let entries: Vec<&CatalogEntry> =
            snapshot.entries.iter().filter(|e| e.uid().as_str() == d.uid).map(|e| &**e).collect();
        let act = match entries.as_slice() {
            [] if deleted.contains(&d.uid) => {
                d.result = refuse;
                Act::None
            }
            [] => Act::Create,
            [entry] => {
                let same_bytes = Some(entry.etag().to_hex()) == d.etag;
                let same_folder = entry.folder().as_str() == d.folder;
                let from = entry.folder().as_str().to_owned();
                let if_match = *entry.etag();
                match (same_bytes, same_folder) {
                    (true, true) => {
                        d.result = DashboardResult::Unchanged;
                        Act::None
                    }
                    _ if !replace => {
                        d.result = refuse;
                        Act::None
                    }
                    (true, false) => Act::Move { if_match, from },
                    (false, true) => Act::Save { if_match, folder: from },
                    (false, false) => Act::MoveSave { if_match, from },
                }
            }
            _ => {
                d.result = refuse;
                Act::None
            }
        };
        acts.push(act);
    }
    Ok((folder_acts, acts))
}

/// The one result that a store call gives for a dashboard, or the reason it failed.
type WriteResult = Result<DashboardResult, (DashboardResult, String)>;

fn failed(error: &StoreError) -> (DashboardResult, String) {
    let result = if matches!(error, StoreError::Exists { .. }) {
        DashboardResult::RefusedExists
    } else {
        DashboardResult::StoreError
    };
    (result, error.to_string())
}

/// Writes one dashboard with the action of the plan (15.13, step 5).
async fn write_dashboard(
    store: &AnyStore,
    doc: &CanonicalDocument,
    act: &Act,
    folder: &str,
    source: &(String, String),
) -> WriteResult {
    let uid = doc.uid().clone();
    let (file, sha) = (&source.0, &source.1);
    let to_folder = FolderId::parse(folder).map_err(|e| (DashboardResult::StoreError, e.to_string()))?;
    let new_etag = doc.etag().to_hex();
    match act {
        Act::None => Ok(DashboardResult::Unchanged),
        Act::Create => {
            let meta = write_meta(file, sha, &format!("import-grafana:{uid}:none:{new_etag}"));
            let saved = store.create(&to_folder, doc, VersionKind::Import, &meta).await.map_err(|e| failed(&e))?;
            Ok(if saved.outcome == Outcome::Unchanged { DashboardResult::Unchanged } else { DashboardResult::Written })
        }
        Act::Save { if_match, folder: expected } => {
            let expected = FolderId::parse(expected).map_err(|e| (DashboardResult::StoreError, e.to_string()))?;
            let meta = write_meta(file, sha, &format!("import-grafana:{uid}:{}:{new_etag}", if_match.to_hex()));
            let saved =
                store.save(doc, if_match, &expected, VersionKind::Import, &meta).await.map_err(|e| failed(&e))?;
            Ok(if saved.outcome == Outcome::Unchanged { DashboardResult::Unchanged } else { DashboardResult::Replaced })
        }
        Act::Move { if_match, from } | Act::MoveSave { if_match, from } => {
            let from = FolderId::parse(from).map_err(|e| (DashboardResult::StoreError, e.to_string()))?;
            let transition = format!("import-grafana:{uid}:{}:move:{folder}", if_match.to_hex());
            let meta = write_meta(file, sha, &transition);
            store.move_to(&uid, &to_folder, if_match, &from, &meta).await.map_err(|e| failed(&e))?;
            if matches!(act, Act::Move { .. }) {
                return Ok(DashboardResult::Moved);
            }
            let meta = write_meta(file, sha, &format!("import-grafana:{uid}:{}:{new_etag}", if_match.to_hex()));
            let saved =
                store.save(doc, if_match, &to_folder, VersionKind::Import, &meta).await.map_err(|e| failed(&e))?;
            Ok(if saved.outcome == Outcome::Unchanged {
                DashboardResult::Moved
            } else {
                DashboardResult::MovedReplaced
            })
        }
    }
}

/// How the store step ends.
enum StoreEnd {
    /// The results are set. The report and the exit code follow the results.
    Done,
    /// The store step failed before it had results. There is no report. The value is the exit code.
    Abort(u8),
}

/// Opens the volume of the configuration. A writer takes the lock (5.16).
/// A PostgreSQL store gets the start checks of 5.29. `config_path` names the file in its errors.
async fn open_volume(
    loaded: &LoadedConfig,
    config_path: Option<&std::path::Path>,
    write: bool,
    err: &mut dyn Write,
) -> Result<AnyStore, u8> {
    let fail_code = if write { EXIT_WRITE } else { EXIT_INPUT };
    let volume = match &loaded.config.storage {
        StorageConfig::Volume(v) => v,
        StorageConfig::Postgres(pg) => {
            let path = config_path.unwrap_or_else(|| std::path::Path::new("<file>"));
            return match super::store_cmds::open_postgres(pg, path).await {
                Ok(store) => Ok(store),
                Err(e) => {
                    let _ = writeln!(err, "hune import-grafana: {e}");
                    Err(fail_code)
                }
            };
        }
    };
    if write && volume.read_only {
        let _ = writeln!(
            err,
            "hune import-grafana: storage.volume.read_only = true: --write cannot write into a read-only volume"
        );
        return Err(EXIT_WRITE);
    }
    let options = VolumeOptions {
        root: volume.path.clone(),
        read_only: !write,
        rescan_interval: volume.rescan_interval,
        history_keep: volume.history_keep,
    };
    match VolumeStore::open(options).await {
        Ok(store) => Ok(AnyStore::from(store)),
        Err(e) if e.kind == OpenErrorKind::LockHeld => {
            let _ = writeln!(
                err,
                "hune import-grafana: the volume lock is held by another process: {e}. Stop `hune serve` and run the command again (3.14)."
            );
            Err(EXIT_WRITE)
        }
        Err(e) => {
            let _ = writeln!(err, "hune import-grafana: storage.volume: {e}");
            Err(fail_code)
        }
    }
}

/// The steps 2 to 5 of 15.13. A dry run does the reads only.
async fn store_step(
    cmd: &ImportCommand,
    loaded: &LoadedConfig,
    result: &mut ImportResult,
    sources: &BTreeMap<String, (String, String)>,
    err: &mut dyn Write,
) -> StoreEnd {
    let abort = if cmd.write { EXIT_WRITE } else { EXIT_INPUT };
    let store = match open_volume(loaded, cmd.config.as_deref(), cmd.write, err).await {
        Ok(s) => s,
        Err(code) => return StoreEnd::Abort(code),
    };
    let snapshot = match store.load().await {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "hune import-grafana: the read of the store failed: {e}");
            return StoreEnd::Abort(abort);
        }
    };
    // A uid without a current document can have versions: the uid is deleted (15.13).
    let mut deleted = BTreeSet::new();
    for d in &result.dashboards {
        let absent = !snapshot.entries.iter().any(|e| e.uid().as_str() == d.uid);
        if d.document_bytes.is_none() || !absent {
            continue;
        }
        let Ok(uid) = Uid::parse(&d.uid) else { continue };
        match store.history(&uid, None, 1).await {
            Ok(list) if !list.is_empty() => {
                deleted.insert(d.uid.clone());
            }
            Ok(_) | Err(StoreError::NotFound) => {}
            Err(e) => {
                let _ = writeln!(err, "hune import-grafana: the read of the history of {} failed: {e}", d.uid);
                return StoreEnd::Abort(abort);
            }
        }
    }
    let (folder_acts, acts) = match plan(result, &snapshot, &deleted, cmd.write, cmd.replace) {
        Ok(p) => p,
        Err(message) => {
            let _ = writeln!(err, "hune import-grafana: {message}");
            return StoreEnd::Abort(abort);
        }
    };
    if !cmd.write || result.exit_code() == EXIT_WRITE {
        return StoreEnd::Done;
    }
    write_all(&store, cmd, loaded, result, (&folder_acts, &acts), sources, err).await;
    StoreEnd::Done
}

/// The steps 4 and 5 of 15.13: the folders, then the dashboards in the order of the Grafana uid.
/// The first store error stops the run.
async fn write_all(
    store: &AnyStore,
    cmd: &ImportCommand,
    loaded: &LoadedConfig,
    result: &mut ImportResult,
    (folder_acts, acts): (&[FolderAct], &[Act]),
    sources: &BTreeMap<String, (String, String)>,
    err: &mut dyn Write,
) {
    for (i, act) in folder_acts.iter().enumerate() {
        if *act == FolderAct::None || (*act == FolderAct::Rename && !cmd.replace) {
            continue;
        }
        let (id, title) = (result.folders[i].id.clone(), result.folders[i].title.clone());
        let call = match (FolderId::parse(&id), FolderTitle::parse(&title)) {
            (Ok(fid), Ok(ftitle)) => match act {
                FolderAct::Create => store.create_folder(&fid, &ftitle).await.map(|()| FolderResult::Created),
                _ => store.rename_folder(&fid, &ftitle).await.map(|()| FolderResult::Renamed),
            },
            (Err(e), _) => Err(StoreError::Unavailable(e.to_string())),
            (_, Err(e)) => Err(StoreError::Unavailable(e.to_string())),
        };
        match call {
            Ok(done) => result.folders[i].result = done,
            Err(e) => {
                let _ = writeln!(err, "hune import-grafana: the folder {id} failed: {e}");
                for d in result.dashboards.iter_mut().filter(|d| d.folder == id && d.document_bytes.is_some()) {
                    d.result = DashboardResult::StoreError;
                }
                return;
            }
        }
    }
    for (i, act) in acts.iter().enumerate() {
        let d = &result.dashboards[i];
        if *act == Act::None {
            continue;
        }
        let (Some(bytes), Some(source)) = (d.document_bytes.as_ref(), sources.get(&d.grafana_uid)) else { continue };
        // The document passes the validation and the save checks of 7.18, as a save of the server does.
        let checked = Uid::parse(&d.uid).map_err(|e| vec![e.to_string()]).and_then(|uid| {
            let doc = validate(bytes, Some(&uid))
                .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<String>>())?;
            let errors = crate::server::save_check_errors(doc.document(), &loaded.config);
            if errors.is_empty() { Ok(doc) } else { Err(errors.iter().map(ToString::to_string).collect()) }
        });
        let outcome = match checked {
            Ok(doc) => write_dashboard(store, &doc, act, &d.folder, source).await,
            Err(messages) => Err((DashboardResult::StoreError, messages.join("; "))),
        };
        let uid = d.uid.clone();
        match outcome {
            Ok(done) => result.dashboards[i].result = done,
            Err((done, message)) => {
                let _ = writeln!(err, "hune import-grafana: the write of {uid} failed: {message}");
                result.dashboards[i].result = done;
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The command
// ---------------------------------------------------------------------------------------------

fn fail(err: &mut dyn Write, message: impl std::fmt::Display) -> u8 {
    let _ = writeln!(err, "hune import-grafana: {message}");
    EXIT_INPUT
}

/// Reads an option file of at most 32 MiB. The path is the value of the option.
fn read_option_file(path: &Path, option: &str) -> Result<Vec<u8>, String> {
    read_limited(path, &format!("{option} {}", path.display()), true)
}

fn file_name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn created_at() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let text = UtcMillis::from_millis(secs.saturating_mul(1000)).to_rfc3339();
    // The form of the report has no fraction: `YYYY-MM-DDThh:mm:ssZ`.
    format!("{}Z", text.get(..19).unwrap_or(&text))
}

/// Replaces each control character by `?`, so a name cannot change the terminal.
fn clean(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { '?' } else { c }).collect()
}

/// The summary of 15.2: the totals, the results and the reason of the exit code.
fn summary(result: &ImportResult, meta: &ReportMeta, code: u8) -> String {
    let t = result.totals();
    let mut s = String::new();
    s.push_str(&format!("import-grafana: {}\n", if meta.write { "write" } else { "dry run" }));
    s.push_str(&format!("input: {}\n", clean(&meta.input_dir)));
    for (name, n) in [
        ("dashboards", t.dashboards),
        ("panels", t.panels),
        ("mapped", t.mapped),
        ("changed", t.changed),
        ("dropped", t.dropped),
        ("unsupported", t.unsupported),
        ("metadata", t.metadata),
        ("accepted", t.accepted),
        ("unaccepted", t.unaccepted),
        ("stale_accept_lines", t.stale_accept_lines),
    ] {
        s.push_str(&format!("{name}: {n}\n"));
    }
    let mut results: BTreeMap<&str, u32> = BTreeMap::new();
    for d in &result.dashboards {
        *results.entry(d.result.as_str()).or_default() += 1;
    }
    s.push_str("dashboard results:");
    for (name, n) in &results {
        s.push_str(&format!(" {name}={n}"));
    }
    s.push('\n');
    let mut folders: BTreeMap<&str, u32> = BTreeMap::new();
    for f in &result.folders {
        *folders.entry(f.result.as_str()).or_default() += 1;
    }
    s.push_str("folder results:");
    for (name, n) in &folders {
        s.push_str(&format!(" {name}={n}"));
    }
    s.push('\n');
    s.push_str(&format!(
        "not imported: public_dashboards={} short_links={}\n",
        result.not_imported.public_dashboards, result.not_imported.short_links
    ));
    let missing: Vec<_> = result.all_items().filter(|i| !i.accepted).collect();
    if !missing.is_empty() {
        s.push_str(&format!("items without acceptance (first {}):\n", MAX_LISTED_ITEMS.min(missing.len())));
        for item in missing.iter().take(MAX_LISTED_ITEMS) {
            s.push_str(&format!("  {} [{}] {}\n", clean(&item.id), item.class.as_str(), clean(&item.reason)));
        }
    }
    if !result.stale_accept_lines.is_empty() {
        s.push_str("stale acceptance lines:\n");
        for line in result.stale_accept_lines.iter().take(MAX_LISTED_ITEMS) {
            s.push_str(&format!("  {}\n", clean(line)));
        }
    }
    s.push_str(&format!("exit code: {code}\n"));
    s.push_str(&format!("reason: {}\n", result.exit_reason()));
    s
}

/// Finds a source id whose block has another kind than the type of the Grafana data source gives (15.9).
/// `kinds` is empty without `-c`: then no block exists to compare, and the type gives the dialect.
fn source_kind_conflict(result: &hune_grafana::ImportResult, kinds: &BTreeMap<String, SourceKind>) -> Option<String> {
    for u in result.source_uses() {
        if let Some(block) = kinds.get(&u.source)
            && *block != u.kind
        {
            return Some(format!(
                "--source-map {}={}: the type of the Grafana data source gives a {} block, but the configuration has a {} block \"{}\" (15.9)",
                u.grafana,
                u.source,
                u.kind.as_str(),
                block.as_str(),
                u.source
            ));
        }
    }
    None
}

/// Runs `hune import-grafana` and gives the exit code (15.12).
pub fn run(cmd: ImportCommand, env: &CapturedEnvironment, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    // The output paths are checked first: a bad path stops the command before any work (15.2).
    let report_targets = match &cmd.report {
        Some(base) => match output::check_report_targets(base) {
            Ok(t) => Some(t),
            Err(e) => return fail(err, e),
        },
        None => None,
    };
    let nginx_target: Option<Target> = match &cmd.nginx_map {
        Some(path) => match output::check_target("--nginx-map", path) {
            Ok(t) => Some(t),
            Err(e) => return fail(err, e),
        },
        None => None,
    };
    let loaded = match &cmd.config {
        // 13.18: it reads only the values of the block `storage` and uses the labels of the
        // `clickhouse` blocks.
        Some(path) => match config::load_with(path, env, config::ReadMode::ImportWrite) {
            Ok(l) => Some(l),
            Err(errors) => {
                for e in &errors {
                    let _ = writeln!(err, "{e}");
                }
                return EXIT_INPUT;
            }
        },
        None => None,
    };
    // 15.9: each source id of the map names a block of the configuration. The kind of the block is
    // the kind of the source. The importer compares it with the type of the Grafana data source
    // after the conversion. The lists of a `mongodb` block serve the checks of 7.33.
    let mut source_kinds: BTreeMap<String, SourceKind> = BTreeMap::new();
    let mut mongo_lists: BTreeMap<String, MongoLists> = BTreeMap::new();
    // The list `adhoc_columns` of each `clickhouse` block gives the keys of an `adhoc` variable (4.10c, 15.6).
    let mut adhoc_columns: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(l) = &loaded {
        for c in &l.config.clickhouse {
            source_kinds.insert(c.id.to_string(), SourceKind::ClickHouse);
            if let Some(list) = &c.adhoc_columns {
                adhoc_columns.insert(c.id.to_string(), list.clone());
            }
        }
        for p in &l.config.postgres_sources {
            source_kinds.insert(p.id.to_string(), SourceKind::Postgres);
        }
        for m in &l.config.mongo_sources {
            source_kinds.insert(m.id.to_string(), SourceKind::MongoDb);
            mongo_lists.insert(
                m.id.to_string(),
                MongoLists { databases: m.databases.clone(), collections: m.collections.clone() },
            );
        }
        for (grafana, source) in &cmd.source_map {
            if !source_kinds.contains_key(source) {
                return fail(
                    err,
                    format!(
                        "--source-map {grafana}={source}: the configuration has no block clickhouse \"{source}\" and no block postgres \"{source}\" and no block mongodb \"{source}\" (15.9)"
                    ),
                );
            }
        }
    }
    let read = match read_input(&cmd.input) {
        Ok(r) => r,
        Err(e) => return fail(err, e),
    };
    let mut options = ImportOptions {
        source_map: cmd.source_map.iter().cloned().collect(),
        source_kinds,
        mongo_lists,
        adhoc_columns,
        ..ImportOptions::default()
    };
    let folder_map = match &cmd.folders {
        Some(path) => {
            let parsed = read_option_file(path, "--folders")
                .and_then(|b| parse_json(&b, &format!("--folders {}", path.display())))
                .and_then(|v| parse_folder_map(&v).map_err(|e| e.to_string()));
            match parsed {
                Ok(m) => m,
                Err(e) => return fail(err, e),
            }
        }
        None => Vec::new(),
    };
    options.folders = match FolderTable::new(&read.folders_json, folder_map) {
        Ok(t) => t,
        Err(e) => return fail(err, e),
    };
    if let Some((old, new)) = &cmd.link_prefix {
        options.link_prefix = match LinkPrefix::new(old, new) {
            Ok(p) => Some(p),
            Err(e) => return fail(err, e),
        };
    }
    if let Some(path) = &cmd.accept {
        let parsed = read_option_file(path, "--accept")
            .and_then(|b| {
                String::from_utf8(b).map_err(|_| format!("--accept {}: the file is not valid UTF-8", path.display()))
            })
            .and_then(|t| parse_acceptance_file(&t).map_err(|e| e.to_string()));
        match parsed {
            Ok(a) => options.accept = a,
            Err(e) => return fail(err, e),
        }
    }
    let mut result = match import_all(&read.input, &options) {
        Ok(r) => r,
        Err(e) => return fail(err, e),
    };
    // 15.9: the type of the Grafana data source gives the kind of the block. A block of another kind
    // stops the command with exit code 2, before any write.
    if let Some(e) = source_kind_conflict(&result, &options.source_kinds) {
        return fail(err, e);
    }
    let meta = ReportMeta {
        hune_version: env!("CARGO_PKG_VERSION").to_owned(),
        created_at: created_at(),
        input_dir: fs::canonicalize(&cmd.input).unwrap_or_else(|_| cmd.input.clone()).display().to_string(),
        source_map: options.source_map.clone(),
        link_prefix: options.link_prefix.clone(),
        folders_file: cmd.folders.as_deref().map(file_name_of),
        accept_file: cmd.accept.as_deref().map(file_name_of),
        write: cmd.write,
        replace: cmd.replace,
    };
    // Step 1 of 15.13: on exit code 1, the command writes no document.
    let stop_before_store = cmd.write && result.exit_code() == EXIT_ACCEPT;
    if let (Some(loaded), false) = (&loaded, stop_before_store) {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(r) => r,
            Err(e) => return fail(err, format!("cannot build the runtime: {e}")),
        };
        if let StoreEnd::Abort(code) = runtime.block_on(store_step(&cmd, loaded, &mut result, &read.sources, err)) {
            return code;
        }
    }
    let mut code = result.exit_code();
    let _ = out.write_all(summary(&result, &meta, code).as_bytes());
    if let Some((json, md)) = &report_targets {
        let rendered = render_report(&result, &meta);
        if let Err(e) = output::write_report(json, md, &rendered.json, &rendered.markdown) {
            let _ = writeln!(err, "hune import-grafana: {e}");
            code = if code == 0 { EXIT_INPUT } else { code };
        }
    }
    if let Some(target) = &nginx_target
        && let Err(e) = output::write_nginx_map(target, &render_nginx_map(&result))
    {
        let _ = writeln!(err, "hune import-grafana: {e}");
        code = if code == 0 { EXIT_INPUT } else { code };
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_a_full_command_line() {
        let cmd = parse(&args(&[
            "--in",
            "/d",
            "--source-map",
            "clickhouse=prod",
            "--source-map=old=legacy",
            "--folders",
            "/f.json",
            "--link-prefix",
            "/grafana/=/dashboards/",
            "--accept",
            "/a.txt",
            "--report",
            "/r",
            "--nginx-map",
            "/m",
            "-c",
            "/c.hcl",
            "--write",
            "--replace",
        ]))
        .unwrap();
        assert_eq!(cmd.source_map, vec![("clickhouse".into(), "prod".into()), ("old".into(), "legacy".into())]);
        assert_eq!(cmd.link_prefix, Some(("/grafana/".into(), "/dashboards/".into())));
        assert!(cmd.write && cmd.replace);
        assert_eq!(cmd.config, Some(PathBuf::from("/c.hcl")));
    }

    #[test]
    fn refuses_a_bad_command_line() {
        let base = ["--in", "/d", "--source-map", "a=b"];
        let with = |extra: &[&str]| {
            let mut list: Vec<&str> = base.to_vec();
            list.extend_from_slice(extra);
            parse(&args(&list))
        };
        assert!(with(&[]).is_ok());
        for extra in [
            &["--in", "/e"][..],
            &["--bogus"],
            &["stray"],
            &["--source-map", "a=c"],
            &["--source-map", "x"],
            &["--source-map", "x=Bad_Id"],
            &["--link-prefix", "grafana=/x/"],
            &["--link-prefix", "/a/=/b/", "--link-prefix", "/a/=/b/"],
            &["--write"],
            &["--replace"],
            &["--replace", "--write", "-c", "/c"],
            &["--nginx-map", "/m"],
            &["--report"],
            &["--report", ""],
            &["--write=yes", "-c", "/c"],
            &["--write", "--write", "-c", "/c"],
        ] {
            let outcome = with(extra);
            if extra == ["--replace", "--write", "-c", "/c"] {
                assert!(outcome.is_ok());
            } else {
                assert!(outcome.is_err(), "{extra:?}");
            }
        }
        assert!(parse(&args(&["--source-map", "a=b"])).is_err());
        assert!(parse(&args(&["--in", "/d"])).is_err());
    }

    #[test]
    fn json_rules_of_the_input() {
        assert!(parse_json(b"{\"a\":1,\"b\":[1,2,{\"c\":null}]}", "x").is_ok());
        assert!(parse_json(b"{\"a\":1,\"a\":2}", "x").unwrap_err().contains("duplicate member"));
        assert!(parse_json(b"{\"a\":{\"b\":1,\"b\":1}}", "x").unwrap_err().contains("duplicate member"));
        assert!(parse_json(b"{\"a\":1,\"\\u0061\":2}", "x").unwrap_err().contains("duplicate member"));
        assert!(parse_json(b"[{\"a\":1},{\"a\":1}]", "x").is_ok());
        assert!(parse_json(b"\xEF\xBB\xBF{}", "x").unwrap_err().contains("byte order mark"));
        assert!(parse_json(b"{} 1", "x").is_err());
        assert!(parse_json(b"{\"a\":\"\xFF\"}", "x").is_err());
        let deep = "[".repeat(10_000);
        assert!(parse_json(deep.as_bytes(), "x").is_err());
    }

    #[test]
    fn the_sums_file_lines() {
        let hex = "a".repeat(64);
        let ok = parse_sums(&format!("{hex}  ./health.json\n{hex} *dashboards/x.json\n\n# note\n")).unwrap();
        assert_eq!(ok.len(), 2);
        assert!(ok.contains_key("health.json") && ok.contains_key("dashboards/x.json"));
        for bad in [
            format!("{hex} health.json"),
            format!("{hex}  ../x.json"),
            format!("{hex}  /etc/x"),
            format!("{hex}  a/b/c.json"),
            format!("{hex}  x.json\n{hex}  x.json"),
            "xyz  x.json".to_owned(),
        ] {
            assert!(parse_sums(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_save_id_is_a_uuid_version_8() {
        let id = save_id("import-grafana:x:none:abc").to_string();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "8");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
        assert_eq!(id, save_id("import-grafana:x:none:abc").to_string());
        assert_ne!(id, save_id("import-grafana:x:none:abd").to_string());
    }
}
