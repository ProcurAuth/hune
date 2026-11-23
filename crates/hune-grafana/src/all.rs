// SPDX-License-Identifier: MIT OR Apache-2.0
//! The import of the whole input directory (15.4): the folders, the dashboards, the acceptance
//! check, the redirects (15.17), the totals and the exit code (15.12).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::dashboard::{DashboardImport, DashboardResult, Form, detect, import_dashboard};
use crate::item::{Class, Ctx, Fallback, ReportItem, SourceUse};
use crate::options::{FolderResolution, ImportError, ImportOptions, ROOT_FOLDER_ID, ROOT_FOLDER_TITLE, assign_uids};

/// One dashboard file of the input directory.
#[derive(Debug, Clone)]
pub struct DashboardFile {
    /// The name in the directory, for example `dashboards/abc123.json`.
    pub name: String,
    /// The file stem. It must equal the uid of the dashboard (15.4).
    pub stem: String,
    /// The SHA-256 of the file in hex, when the command computed it.
    pub sha256: Option<String>,
    /// The JSON value of the file.
    pub value: Value,
}

/// The input directory of 15.4 as values.
#[derive(Debug, Clone, Default)]
pub struct ImportInput {
    /// The dashboard files, in any order.
    pub dashboards: Vec<DashboardFile>,
    /// `search.json`. It is necessary when a raw v1 file exists.
    pub search: Option<Value>,
    /// `health.json`: the report records its member `version`.
    pub health: Option<Value>,
    /// `plugin-clickhouse.json`: the report records the plugin version.
    pub plugin: Option<Value>,
    /// `public-dashboards.json`.
    pub public_dashboards: Option<Value>,
    /// `short-urls.json`: a list of `{uid, path}`.
    pub short_urls: Option<Value>,
    /// The names that the importer ignores (15.4).
    pub ignored_files: Vec<String>,
}

/// The result of a folder in the report (15.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderResult {
    /// The store holds the folder with this title.
    Exists,
    /// Dry run: the folder is new.
    WouldCreate,
    /// The importer created the folder.
    Created,
    /// Dry run: the folder has another title.
    WouldRename,
    /// The importer renamed the folder.
    Renamed,
    /// The folder has another title and `--replace` is absent.
    RefusedTitle,
}

impl FolderResult {
    /// The text of the result in the report.
    pub fn as_str(self) -> &'static str {
        match self {
            FolderResult::Exists => "exists",
            FolderResult::WouldCreate => "would_create",
            FolderResult::Created => "created",
            FolderResult::WouldRename => "would_rename",
            FolderResult::Renamed => "renamed",
            FolderResult::RefusedTitle => "refused_title",
        }
    }
}

/// A folder of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderReport {
    /// The Grafana folder uid, or `None` for the root.
    pub grafana_uid: Option<String>,
    /// The folder id of Hune.
    pub id: String,
    /// The folder title.
    pub title: String,
    /// The result. The importer sets `WouldCreate`. The command sets the others.
    pub result: FolderResult,
}

/// One redirect (15.17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    /// The old path.
    pub from: String,
    /// The new path.
    pub to: String,
}

/// A file of the input and its form, for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFileInfo {
    /// The name, for example `dashboards/abc123.json`.
    pub name: String,
    /// The SHA-256 in hex.
    pub sha256: Option<String>,
    /// The form: `v1`, `v2alpha1` or `v2beta1`.
    pub form: String,
}

/// The totals of the report (15.11).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Totals {
    /// The number of dashboards.
    pub dashboards: u32,
    /// The number of panels.
    pub panels: u32,
    /// The number of `mapped` members.
    pub mapped: u32,
    /// The number of `changed` items.
    pub changed: u32,
    /// The number of `dropped` items that are not metadata.
    pub dropped: u32,
    /// The number of `unsupported` items.
    pub unsupported: u32,
    /// The number of metadata members.
    pub metadata: u32,
    /// The number of accepted items.
    pub accepted: u32,
    /// The number of items without acceptance.
    pub unaccepted: u32,
    /// The number of acceptance lines that match no item.
    pub stale_accept_lines: u32,
}

/// The objects that are not imported (15.6, 15.11).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotImported {
    /// The number of public dashboards.
    pub public_dashboards: u32,
    /// The number of short links.
    pub short_links: u32,
}

/// The result of the conversion of the input directory.
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// The dashboards, in the order of their Grafana uid.
    pub dashboards: Vec<DashboardImport>,
    /// The folders that hold a dashboard, in the order of their id.
    pub folders: Vec<FolderReport>,
    /// The items of the folders: a folder with a parent (15.6). The item id starts with `folder:`.
    pub folder_items: Vec<ReportItem>,
    /// The items of the short links (15.17). The item id starts with `short-url:`.
    pub short_link_items: Vec<ReportItem>,
    /// The acceptance lines that match no item (15.10).
    pub stale_accept_lines: Vec<String>,
    /// The redirects of 15.17: the dashboards, then the short links. Empty without a link prefix.
    pub redirects: Vec<Redirect>,
    /// The lines of the map of the dashboards: Grafana uid and Hune uid.
    pub uid_lines: Vec<(String, String)>,
    /// The lines of the map of the short links: short uid and target.
    pub short_lines: Vec<(String, String)>,
    /// The objects that are not imported.
    pub not_imported: NotImported,
    /// The Grafana version of `health.json`.
    pub grafana_version: Option<String>,
    /// The version of the ClickHouse plugin.
    pub plugin_version: Option<String>,
    /// The dashboard files and their forms.
    pub files: Vec<InputFileInfo>,
    /// The ignored names.
    pub ignored_files: Vec<String>,
}

impl ImportResult {
    /// The data sources that the queries and the variables of the run use, once each, with the
    /// dialect that the Grafana type gives (15.9).
    pub fn source_uses(&self) -> Vec<SourceUse> {
        let mut out: Vec<SourceUse> = Vec::new();
        for u in self.dashboards.iter().flat_map(|d| d.source_uses.iter()) {
            if !out.contains(u) {
                out.push(u.clone());
            }
        }
        out
    }

    /// All items of the run: dashboards, folders and short links.
    pub fn all_items(&self) -> impl Iterator<Item = &ReportItem> {
        self.dashboards
            .iter()
            .flat_map(|d| d.items.iter())
            .chain(self.folder_items.iter())
            .chain(self.short_link_items.iter())
    }

    /// The totals of 15.11.
    pub fn totals(&self) -> Totals {
        let mut t = Totals {
            dashboards: u32::try_from(self.dashboards.len()).unwrap_or(u32::MAX),
            stale_accept_lines: u32::try_from(self.stale_accept_lines.len()).unwrap_or(u32::MAX),
            ..Totals::default()
        };
        for d in &self.dashboards {
            t.panels += d.panels;
            t.mapped += d.mapped;
            t.metadata += d.metadata;
        }
        for item in self.all_items() {
            match item.class {
                Class::Mapped => {}
                Class::Changed => t.changed += 1,
                Class::Dropped => t.dropped += 1,
                Class::Unsupported => t.unsupported += 1,
            }
            if item.accepted {
                t.accepted += 1;
            } else {
                t.unaccepted += 1;
            }
        }
        t
    }

    /// The exit code of 15.12: 0, 1 or 3. The codes 2 come from [`ImportError`].
    ///
    /// The code is 1 when an item has no acceptance or an acceptance line is stale. The code is 3
    /// when the store step refused or failed a write. The command sets the results of the store
    /// step before it calls this function.
    pub fn exit_code(&self) -> u8 {
        let t = self.totals();
        if t.unaccepted > 0 || t.stale_accept_lines > 0 {
            return 1;
        }
        let refused = self
            .dashboards
            .iter()
            .any(|d| matches!(d.result, DashboardResult::RefusedExists | DashboardResult::StoreError))
            || self.folders.iter().any(|f| f.result == FolderResult::RefusedTitle);
        if refused { 3 } else { 0 }
    }

    /// The reason of the exit code, in one English sentence.
    pub fn exit_reason(&self) -> String {
        let t = self.totals();
        match self.exit_code() {
            0 => "Each item that needs acceptance is accepted, no acceptance line is stale, and each write succeeded."
                .to_owned(),
            1 => format!(
                "{} item(s) need acceptance and have none, and {} acceptance line(s) are stale.",
                t.unaccepted, t.stale_accept_lines
            ),
            _ => "The store refused or failed a write.".to_owned(),
        }
    }
}

fn percent_encode_target(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '$' => out.push_str("%24"),
            '"' => out.push_str("%22"),
            '\\' => out.push_str("%5C"),
            ' ' => out.push_str("%20"),
            ';' => out.push_str("%3B"),
            '{' => out.push_str("%7B"),
            '}' => out.push_str("%7D"),
            c => out.push(c),
        }
    }
    out
}

fn is_nginx_safe(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Gives the target of a short link: the new path of 15.17, or `None` for another path.
fn short_target(path: &str, new_prefix: &str, hune_uids: &BTreeMap<String, String>) -> Option<String> {
    let path = path.trim_start_matches('/');
    let rest = path.strip_prefix("d/")?;
    let (head, query) = match rest.split_once('?') {
        Some((h, q)) => (h, Some(q)),
        None => (rest, None),
    };
    let (uid, slug) = match head.split_once('/') {
        Some((u, s)) => (u, Some(s)),
        None => (head, None),
    };
    let hune = hune_uids.get(uid)?;
    let mut target = format!("{new_prefix}d/{hune}");
    if let Some(s) = slug {
        target.push('/');
        target.push_str(s);
    }
    if let Some(q) = query {
        target.push('?');
        target.push_str(q);
    }
    Some(percent_encode_target(&target))
}

fn version_of(value: &Option<Value>, paths: &[&str]) -> Option<String> {
    let v = value.as_ref()?;
    paths.iter().find_map(|p| v.pointer(p).and_then(Value::as_str)).map(str::to_owned)
}

/// Converts every dashboard of the input directory (15.4) and gives the data of the report.
///
/// An error is an input error: the command exits with code 2. Three cases stop the run. A file
/// stem differs from the uid. A form fits no row. A raw v1 file has no `search.json`.
pub fn import_all(input: &ImportInput, opts: &ImportOptions) -> Result<ImportResult, ImportError> {
    if input.dashboards.is_empty() || input.dashboards.len() > 1000 {
        return Err(ImportError::new("the input has 1 to 1000 dashboard files"));
    }
    // The search answer gives the folder of a raw v1 dashboard.
    let mut search_folders: BTreeMap<String, String> = BTreeMap::new();
    if let Some(Value::Array(list)) = &input.search {
        for entry in list {
            if let (Some(uid), Some(folder)) =
                (entry.get("uid").and_then(Value::as_str), entry.get("folderUid").and_then(Value::as_str))
            {
                search_folders.insert(uid.to_owned(), folder.to_owned());
            }
        }
    }
    let mut files: Vec<&DashboardFile> = input.dashboards.iter().collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let mut grafana_uids: Vec<String> = Vec::new();
    let mut infos = Vec::new();
    for f in &files {
        let det =
            detect(&f.value, &search_folders).map_err(|e| ImportError::new(format!("{}: {}", f.name, e.message)))?;
        if det.form == Form::V1Raw && input.search.is_none() {
            return Err(ImportError::new(format!("{}: a raw v1 file needs search.json (15.4)", f.name)));
        }
        if det.grafana_uid != f.stem {
            return Err(ImportError::new(format!(
                "{}: the file stem {:?} differs from the uid {:?} (15.4)",
                f.name, f.stem, det.grafana_uid
            )));
        }
        if grafana_uids.contains(&det.grafana_uid) {
            return Err(ImportError::new(format!("{}: the uid {:?} occurs in two files", f.name, det.grafana_uid)));
        }
        grafana_uids.push(det.grafana_uid.clone());
        infos.push(InputFileInfo { name: f.name.clone(), sha256: f.sha256.clone(), form: det.form.name().to_owned() });
    }
    let mut run = opts.clone();
    run.uid_map = assign_uids(&grafana_uids);
    run.search_folders = search_folders;
    let mut order: Vec<&DashboardFile> = files;
    order.sort_by(|a, b| a.stem.cmp(&b.stem));
    let dashboards: Vec<DashboardImport> = order.iter().map(|f| import_dashboard(&f.value, &run)).collect();

    // Folders.
    let accepted = run.accepted_set();
    let mut folders: BTreeMap<String, FolderReport> = BTreeMap::new();
    let mut folder_items: Vec<ReportItem> = Vec::new();
    let mut parent_seen: BTreeSet<String> = BTreeSet::new();
    for (f, d) in order.iter().zip(&dashboards) {
        let det = detect(&f.value, &run.search_folders)?;
        match run.folders.resolve(det.folder_uid.as_deref()) {
            FolderResolution::Found(info) => {
                folders.entry(info.id.clone()).or_insert(FolderReport {
                    grafana_uid: info.grafana_uid.clone(),
                    id: info.id.clone(),
                    title: info.title.clone(),
                    result: FolderResult::WouldCreate,
                });
            }
            FolderResolution::HasParent { grafana_uid } => {
                if parent_seen.insert(grafana_uid.clone()) {
                    let index = run.folders.folders.iter().find(|x| x.uid == grafana_uid).map_or(0, |x| x.index);
                    let raw = run.folders.folders.iter().find(|x| x.uid == grafana_uid).map(|x| x.raw.clone());
                    let pointer = format!("/{index}/parentUid");
                    let value = raw.as_ref().and_then(|r| r.get("parentUid").cloned());
                    let mut ctx = Ctx::new(&format!("folder:{grafana_uid}"), "");
                    ctx.unsupported(
                        &pointer,
                        value.as_ref(),
                        "Folders have one level: a folder with a parent is not imported",
                        Fallback::other("its dashboards are not imported"),
                    );
                    folder_items.extend(ctx.finish(&accepted).items);
                }
            }
            FolderResolution::NoId { .. } => {}
        }
        let _ = d;
    }
    // Short links and redirects.
    let hune_uids: BTreeMap<String, String> =
        dashboards.iter().filter(|d| d.importable).map(|d| (d.grafana_uid.clone(), d.uid.clone())).collect();
    let mut redirects = Vec::new();
    let mut uid_lines = Vec::new();
    let mut short_lines = Vec::new();
    let mut short_link_items = Vec::new();
    let mut short_count = 0u32;
    if let Some(Value::Array(list)) = &input.short_urls {
        short_count = u32::try_from(list.len()).unwrap_or(u32::MAX);
    }
    if let Some(prefix) = &run.link_prefix {
        for (g, v) in &hune_uids {
            if is_nginx_safe(g) {
                redirects.push(Redirect { from: format!("{}d/{g}", prefix.old), to: format!("{}d/{v}", prefix.new) });
                uid_lines.push((g.clone(), v.clone()));
            }
        }
        if let Some(Value::Array(list)) = &input.short_urls {
            for (i, entry) in list.iter().enumerate() {
                let uid = entry.get("uid").and_then(Value::as_str).unwrap_or("");
                let path = entry.get("path").and_then(Value::as_str).unwrap_or("");
                let mut ctx = Ctx::new(&format!("short-url:{uid}"), "");
                if !is_nginx_safe(uid) {
                    ctx.changed(
                        &format!("/{i}/uid"),
                        Some(&Value::String(uid.to_owned())),
                        "the short uid has a character outside [A-Za-z0-9_-]: the nginx map has no line for it",
                    );
                    short_link_items.extend(ctx.finish(&accepted).items);
                    continue;
                }
                let target = match short_target(path, &prefix.new, &hune_uids) {
                    Some(t) => t,
                    None => {
                        ctx.changed(
                            &format!("/{i}/path"),
                            Some(&Value::String(path.to_owned())),
                            "the path is not a link to an imported dashboard: the redirect goes to the start page",
                        );
                        short_link_items.extend(ctx.finish(&accepted).items);
                        percent_encode_target(&prefix.new)
                    }
                };
                redirects.push(Redirect { from: format!("{}goto/{uid}", prefix.old), to: target.clone() });
                short_lines.push((uid.to_owned(), target));
            }
        }
    }
    uid_lines.sort();
    short_lines.sort();
    // Acceptance: a line that matches no item is stale.
    let ids: BTreeSet<String> = dashboards
        .iter()
        .flat_map(|d| d.items.iter())
        .chain(folder_items.iter())
        .chain(short_link_items.iter())
        .map(|i| i.id.clone())
        .collect();
    let stale: Vec<String> = run.accept.ids.iter().filter(|id| !ids.contains(*id)).cloned().collect();
    let public = match &input.public_dashboards {
        Some(Value::Array(a)) => a.len(),
        Some(v) => v
            .get("publicDashboards")
            .and_then(Value::as_array)
            .map_or_else(|| v.get("totalCount").and_then(Value::as_u64).unwrap_or(0) as usize, Vec::len),
        None => 0,
    };
    let _ = (ROOT_FOLDER_ID, ROOT_FOLDER_TITLE);
    Ok(ImportResult {
        dashboards,
        folders: folders.into_values().collect(),
        folder_items,
        short_link_items,
        stale_accept_lines: stale,
        redirects,
        uid_lines,
        short_lines,
        not_imported: NotImported {
            public_dashboards: u32::try_from(public).unwrap_or(u32::MAX),
            short_links: short_count,
        },
        grafana_version: version_of(&input.health, &["/version"]),
        plugin_version: version_of(&input.plugin, &["/info/version", "/version"]),
        files: infos,
        ignored_files: input.ignored_files.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_target_forms() {
        let mut uids = BTreeMap::new();
        uids.insert("abc123".to_owned(), "abc123".to_owned());
        assert_eq!(
            short_target("d/abc123/nodes?from=now-7d&to=now&var-v=$__all", "/dashboards/", &uids).as_deref(),
            Some("/dashboards/d/abc123/nodes?from=now-7d&to=now&var-v=%24__all")
        );
        assert_eq!(short_target("d/abc123", "/dashboards/", &uids).as_deref(), Some("/dashboards/d/abc123"));
        assert!(short_target("explore?x=1", "/dashboards/", &uids).is_none());
        assert!(short_target("d/other", "/dashboards/", &uids).is_none());
    }
}
