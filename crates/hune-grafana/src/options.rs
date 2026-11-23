// SPDX-License-Identifier: MIT OR Apache-2.0
//! The options of the importer: the source map (15.9), the folder map (15.7), the link prefix,
//! the acceptance list (15.10) and the uid rule (15.8).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use hune_core::etag::Etag;
use hune_core::ids::is_valid_id;
use hune_core::sql::Dialect;
use serde_json::Value;

/// An error of usage or of the input. The command exits with code 2 (15.12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    /// The message: it names the file, the member and the value.
    pub message: String,
}

impl ImportError {
    pub(crate) fn new(message: impl Into<String>) -> ImportError {
        ImportError { message: message.into() }
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ImportError {}

/// The option `--link-prefix <old>=<new>`. Both parts start and end with `/` (15.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPrefix {
    /// The prefix of the Grafana links, for example `/grafana/`.
    pub old: String,
    /// The prefix of the Hune links, for example `/dashboards/`.
    pub new: String,
}

impl LinkPrefix {
    /// Checks the two parts: each one starts and ends with `/`.
    pub fn new(old: &str, new: &str) -> Result<LinkPrefix, ImportError> {
        for (name, part) in [("old", old), ("new", new)] {
            if !(part.starts_with('/') && part.ends_with('/')) {
                return Err(ImportError::new(format!(
                    "--link-prefix: the {name} part must start and end with /; value {part:?}"
                )));
            }
        }
        Ok(LinkPrefix { old: old.to_owned(), new: new.to_owned() })
    }
}

/// One entry of the folder map file (15.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderMapEntry {
    /// The folder uid of `folders.json`.
    pub grafana_uid: String,
    /// The folder id of Hune.
    pub id: String,
    /// The title, or `None` to take the Grafana title.
    pub title: Option<String>,
}

/// Reads the folder map file (15.7). The schema is closed.
pub fn parse_folder_map(value: &Value) -> Result<Vec<FolderMapEntry>, ImportError> {
    let bad = |m: String| ImportError::new(format!("folder map: {m}"));
    let top = value.as_object().ok_or_else(|| bad("the file must be a JSON object".into()))?;
    if let Some(extra) = top.keys().find(|k| k.as_str() != "folders") {
        return Err(bad(format!("member {extra} is not permitted; permitted members: folders")));
    }
    let list = top
        .get("folders")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("member folders is necessary and must be an array".into()))?;
    let mut out: Vec<FolderMapEntry> = Vec::new();
    for (i, entry) in list.iter().enumerate() {
        let o = entry.as_object().ok_or_else(|| bad(format!("folders/{i} must be an object")))?;
        if let Some(extra) = o.keys().find(|k| !["grafana_uid", "id", "title"].contains(&k.as_str())) {
            return Err(bad(format!(
                "folders/{i}: member {extra} is not permitted; permitted members: grafana_uid, id, title"
            )));
        }
        let text = |key: &str| -> Result<Option<String>, ImportError> {
            match o.get(key) {
                None => Ok(None),
                Some(Value::String(s)) => Ok(Some(s.clone())),
                Some(other) => Err(bad(format!("folders/{i}/{key} must be a text; value {other}"))),
            }
        };
        let grafana_uid = text("grafana_uid")?.ok_or_else(|| bad(format!("folders/{i}: grafana_uid is necessary")))?;
        let id = text("id")?.ok_or_else(|| bad(format!("folders/{i}: id is necessary")))?;
        let title = text("title")?;
        if !is_valid_id(&id) {
            return Err(bad(format!("folders/{i}/id must match ^[a-z0-9][a-z0-9-]{{0,39}}$; value {id:?}")));
        }
        if id == "general" {
            return Err(bad(format!("folders/{i}/id: the value general is reserved for the root; value {id:?}")));
        }
        if let Some(t) = &title
            && !(1..=256).contains(&t.chars().count())
        {
            return Err(bad(format!("folders/{i}/title must have 1 to 256 characters; value {t:?}")));
        }
        if out.iter().any(|e| e.grafana_uid == grafana_uid) {
            return Err(bad(format!("folders/{i}/grafana_uid is not unique; value {grafana_uid:?}")));
        }
        out.push(FolderMapEntry { grafana_uid, id, title });
    }
    Ok(out)
}

/// A folder of the answer of `GET /api/folders`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrafanaFolder {
    /// The index of the folder in `folders.json`.
    pub index: usize,
    /// The folder uid.
    pub uid: String,
    /// The Grafana title.
    pub title: String,
    /// The parent folder uid, when the folder has a parent.
    pub parent_uid: Option<String>,
    /// The folder object of the file.
    pub raw: Value,
}

/// The folder that holds a dashboard in Hune.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderInfo {
    /// The Grafana folder uid, or `None` for the root.
    pub grafana_uid: Option<String>,
    /// The folder id of Hune.
    pub id: String,
    /// The folder title.
    pub title: String,
}

/// The result of the search for the folder of a dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderResolution {
    /// The folder is known.
    Found(FolderInfo),
    /// The folder has a parent: its dashboards are not imported (15.6).
    HasParent {
        /// The folder uid.
        grafana_uid: String,
    },
    /// The folder uid does not fit the id rule and the folder map has no entry (15.7).
    NoId {
        /// The folder uid.
        grafana_uid: String,
    },
}

/// The Grafana folders and the folder map: the table that gives the folder id of each folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderTable {
    /// The folders of `folders.json`, in file order.
    pub folders: Vec<GrafanaFolder>,
    map: BTreeMap<String, FolderMapEntry>,
}

/// The root folder of Grafana in Hune (D9).
pub const ROOT_FOLDER_ID: &str = "general";
/// The title of the root folder (15.6).
pub const ROOT_FOLDER_TITLE: &str = "General";

impl FolderTable {
    /// Reads `folders.json` and the folder map. Two folders that map to one id give an error (15.7).
    pub fn new(folders_json: &Value, map: Vec<FolderMapEntry>) -> Result<FolderTable, ImportError> {
        let list = folders_json
            .as_array()
            .ok_or_else(|| ImportError::new("folders.json: the answer of GET /api/folders must be an array"))?;
        let mut folders = Vec::new();
        for (index, f) in list.iter().enumerate() {
            let uid = f.get("uid").and_then(Value::as_str).ok_or_else(|| {
                ImportError::new(format!("folders.json: item {index} needs a text member uid; value {f}"))
            })?;
            let title = f.get("title").and_then(Value::as_str).unwrap_or(uid).to_owned();
            let parent_uid = f.get("parentUid").and_then(Value::as_str).filter(|p| !p.is_empty()).map(str::to_owned);
            folders.push(GrafanaFolder { index, uid: uid.to_owned(), title, parent_uid, raw: f.clone() });
        }
        let map: BTreeMap<String, FolderMapEntry> = map.into_iter().map(|e| (e.grafana_uid.clone(), e)).collect();
        let table = FolderTable { folders, map };
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        for f in &table.folders {
            if let Some(id) = table.id_of(&f.uid)
                && let Some(other) = seen.insert(id.clone(), f.uid.clone())
            {
                return Err(ImportError::new(format!(
                    "folders: the Grafana folders {other} and {} map to the same id {id}",
                    f.uid
                )));
            }
        }
        Ok(table)
    }

    /// The folder id of a Grafana folder uid: the map entry, else the uid when it fits the rule.
    fn id_of(&self, uid: &str) -> Option<String> {
        if let Some(e) = self.map.get(uid) {
            return Some(e.id.clone());
        }
        (is_valid_id(uid) && uid != ROOT_FOLDER_ID).then(|| uid.to_owned())
    }

    /// Finds the folder of a dashboard from its Grafana folder uid. An empty or absent uid is the root.
    pub fn resolve(&self, folder_uid: Option<&str>) -> FolderResolution {
        let uid = match folder_uid {
            None | Some("") => return FolderResolution::Found(root_folder()),
            Some(u) => u,
        };
        let listed = self.folders.iter().find(|f| f.uid == uid);
        if listed.is_none() && uid == ROOT_FOLDER_ID {
            return FolderResolution::Found(root_folder());
        }
        if listed.is_some_and(|f| f.parent_uid.is_some()) {
            return FolderResolution::HasParent { grafana_uid: uid.to_owned() };
        }
        let Some(id) = self.id_of(uid) else {
            return FolderResolution::NoId { grafana_uid: uid.to_owned() };
        };
        let title = self
            .map
            .get(uid)
            .and_then(|e| e.title.clone())
            .or_else(|| listed.map(|f| f.title.clone()))
            .unwrap_or_else(|| uid.to_owned());
        FolderResolution::Found(FolderInfo { grafana_uid: Some(uid.to_owned()), id, title })
    }
}

fn root_folder() -> FolderInfo {
    FolderInfo { grafana_uid: None, id: ROOT_FOLDER_ID.to_owned(), title: ROOT_FOLDER_TITLE.to_owned() }
}

/// The accepted losses (15.10): the item ids of the acceptance file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Acceptance {
    /// The item ids, in file order without duplicates.
    pub ids: Vec<String>,
}

impl Acceptance {
    /// Tells if an item id is accepted.
    pub fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|i| i == id)
    }
}

/// Reads the acceptance file (15.10). A duplicate item id gives an error.
pub fn parse_acceptance_file(text: &str) -> Result<Acceptance, ImportError> {
    let mut ids: Vec<String> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = n + 1;
        let content = match raw.find(" #").or_else(|| raw.find("\t#")) {
            Some(at) => &raw[..at],
            None => raw,
        };
        let content = content.trim();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        if !content.contains(':') {
            return Err(ImportError::new(format!(
                "acceptance file, line {line}: an item id is <grafana uid>:<JSON pointer>; value {content:?}"
            )));
        }
        if ids.iter().any(|i| i == content) {
            return Err(ImportError::new(format!("acceptance file, line {line}: duplicate item id {content:?}")));
        }
        ids.push(content.to_owned());
    }
    Ok(Acceptance { ids })
}

/// The kind of a source block (15.9): the kind that the type of a Grafana data source gives, or the kind
/// of a block of the configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceKind {
    /// A `clickhouse` block.
    ClickHouse,
    /// A `postgres` block.
    Postgres,
    /// A `mongodb` block.
    MongoDb,
}

impl SourceKind {
    /// Gives the name of the kind as the configuration writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::ClickHouse => Dialect::ClickHouse.as_str(),
            SourceKind::Postgres => Dialect::Postgres.as_str(),
            SourceKind::MongoDb => "mongodb",
        }
    }

    /// Gives the SQL dialect of the kind (7.19). The kind `mongodb` has none.
    pub fn dialect(self) -> Option<Dialect> {
        match self {
            SourceKind::ClickHouse => Some(Dialect::ClickHouse),
            SourceKind::Postgres => Some(Dialect::Postgres),
            SourceKind::MongoDb => None,
        }
    }
}

impl From<Dialect> for SourceKind {
    fn from(d: Dialect) -> SourceKind {
        match d {
            Dialect::ClickHouse => SourceKind::ClickHouse,
            Dialect::Postgres => SourceKind::Postgres,
        }
    }
}

/// The lists of a `mongodb` block that the checks of the importer use (7.33).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MongoLists {
    /// The key `databases` of the block.
    pub databases: Vec<String>,
    /// The key `collections` of the block.
    pub collections: Vec<String>,
}

/// The options of one import.
#[derive(Debug, Clone, Default)]
pub struct ImportOptions {
    /// The source map (15.9): a Grafana data source uid or name to a source id.
    pub source_map: BTreeMap<String, String>,
    /// The kind of each source id that the configuration declares (15.9). Empty without `-c`. A query
    /// whose data source has no type takes its kind from here, and ClickHouse when the id is absent.
    pub source_kinds: BTreeMap<String, SourceKind>,
    /// The lists of each `mongodb` block of the configuration (7.33). Empty without `-c`: the checks of
    /// the collection and of the database then wait for the save.
    pub mongo_lists: BTreeMap<String, MongoLists>,
    /// The folders (15.7).
    pub folders: FolderTable,
    /// The link prefix (15.17).
    pub link_prefix: Option<LinkPrefix>,
    /// The accepted losses (15.10).
    pub accept: Acceptance,
    /// The Hune uid of each Grafana uid of the run ([`assign_uids`]). A uid that is not here
    /// follows the uid rule alone (15.8).
    pub uid_map: BTreeMap<String, String>,
    /// The folder uid of each raw v1 dashboard, from `search.json` (15.4).
    pub search_folders: BTreeMap<String, String>,
    /// The list `adhoc_columns` of each `clickhouse` block of the configuration that has one (4.10c, 15.6). Empty without
    /// `-c`: the keys of an `adhoc` variable then wait for the checks at save.
    pub adhoc_columns: BTreeMap<String, Vec<String>>,
}

impl ImportOptions {
    /// Makes the options with a source map. Other members are empty.
    pub fn with_source_map(pairs: &[(&str, &str)]) -> ImportOptions {
        ImportOptions {
            source_map: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
            ..ImportOptions::default()
        }
    }

    pub(crate) fn accepted_set(&self) -> BTreeSet<String> {
        self.accept.ids.iter().cloned().collect()
    }

    pub(crate) fn hune_uid(&self, grafana_uid: &str) -> String {
        match self.uid_map.get(grafana_uid) {
            Some(u) => u.clone(),
            None => uid_rule(grafana_uid, &BTreeSet::new()),
        }
    }
}

/// Gives the first three steps of the uid rule (15.8) as text. The result can be empty or invalid.
fn clean_uid(old: &str) -> String {
    let lower: String = old.chars().map(|c| c.to_ascii_lowercase()).collect();
    let replaced: String =
        lower.chars().map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() { c } else { '-' }).collect();
    let mut squeezed = String::new();
    for c in replaced.chars() {
        if c == '-' && squeezed.ends_with('-') {
            continue;
        }
        squeezed.push(c);
    }
    let trimmed = squeezed.trim_matches('-');
    trimmed.chars().take(40).collect()
}

/// Gives the replacement uid of step 4: `g-` and the first 16 hex characters of the SHA-256 of the old uid.
fn hashed_uid(old: &str) -> String {
    let hex = Etag::of(old.as_bytes()).to_hex();
    format!("g-{}", &hex[..16])
}

/// Applies the uid rule of 15.8 to one uid. `taken` holds the uids of the other imported dashboards.
pub fn uid_rule(old: &str, taken: &BTreeSet<String>) -> String {
    if is_valid_id(old) {
        return old.to_owned();
    }
    let cleaned = clean_uid(old);
    if cleaned.is_empty() || !is_valid_id(&cleaned) || taken.contains(&cleaned) { hashed_uid(old) } else { cleaned }
}

/// Gives the Hune uid of each Grafana uid of a run (15.8). A uid that fits the rule keeps its
/// text. The other uids follow the steps in the order of the Grafana uid. The result is the
/// same at each run.
pub fn assign_uids(grafana_uids: &[String]) -> BTreeMap<String, String> {
    let mut sorted: Vec<&String> = grafana_uids.iter().collect();
    sorted.sort();
    sorted.dedup();
    let mut taken: BTreeSet<String> = sorted.iter().filter(|u| is_valid_id(u)).map(|u| (*u).clone()).collect();
    let mut out = BTreeMap::new();
    for old in sorted {
        if is_valid_id(old) {
            out.insert(old.clone(), old.clone());
            continue;
        }
        let new = uid_rule(old, &taken);
        taken.insert(new.clone());
        out.insert(old.clone(), new);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn uid_rule_steps() {
        assert_eq!(uid_rule("Nodes_Prod", &BTreeSet::new()), "nodes-prod");
        assert_eq!(uid_rule("abc123", &BTreeSet::new()), "abc123");
        assert_eq!(uid_rule("--A  B--", &BTreeSet::new()), "a-b");
        let long = "A".repeat(60);
        assert_eq!(uid_rule(&long, &BTreeSet::new()), "a".repeat(40));
        let empty = uid_rule("___", &BTreeSet::new());
        assert!(empty.starts_with("g-") && empty.len() == 18);
    }

    #[test]
    fn uid_collision_gives_the_hashed_form() {
        let uids = vec!["Prod_Main".to_owned(), "prod-main".to_owned(), "Prod-Main".to_owned()];
        let m = assign_uids(&uids);
        assert_eq!(m["prod-main"], "prod-main");
        assert!(m["Prod_Main"].starts_with("g-"));
        assert!(m["Prod-Main"].starts_with("g-"));
        assert_ne!(m["Prod_Main"], m["Prod-Main"]);
        assert_eq!(m, assign_uids(&uids));
    }

    #[test]
    fn acceptance_file_rules() {
        let a = parse_acceptance_file("# head\n\nabc:/a/b   # why\nx:/y\n").unwrap();
        assert_eq!(a.ids, vec!["abc:/a/b".to_owned(), "x:/y".to_owned()]);
        assert!(parse_acceptance_file("a:/b\na:/b\n").unwrap_err().message.contains("duplicate"));
        assert!(parse_acceptance_file("no-colon\n").is_err());
    }

    #[test]
    fn folder_map_and_table() {
        let map = parse_folder_map(&json!({"folders": [{"grafana_uid": "fe1", "id": "transit", "title": "Transit"}]}))
            .unwrap();
        let folders = json!([{"uid": "fe1", "title": "T"}, {"uid": "Bad_Uid", "title": "B"}, {"uid": "kid", "title": "K", "parentUid": "fe1"}]);
        let t = FolderTable::new(&folders, map).unwrap();
        assert!(matches!(t.resolve(None), FolderResolution::Found(f) if f.id == "general" && f.title == "General"));
        assert!(
            matches!(t.resolve(Some("fe1")), FolderResolution::Found(f) if f.id == "transit" && f.title == "Transit")
        );
        assert!(matches!(t.resolve(Some("Bad_Uid")), FolderResolution::NoId { .. }));
        assert!(matches!(t.resolve(Some("kid")), FolderResolution::HasParent { .. }));
        let clash = FolderTable::new(
            &json!([{"uid": "a", "title": "A"}, {"uid": "b", "title": "B"}]),
            vec![FolderMapEntry { grafana_uid: "b".into(), id: "a".into(), title: None }],
        );
        assert!(clash.is_err());
        assert!(parse_folder_map(&json!({"folders": [{"grafana_uid": "x", "id": "general"}]})).is_err());
        assert!(parse_folder_map(&json!({"folders": [], "zzz": 1})).is_err());
    }
}
