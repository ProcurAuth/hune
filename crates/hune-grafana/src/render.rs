// SPDX-License-Identifier: MIT OR Apache-2.0
//! The report (15.11) in its two forms, and the nginx map (15.17).

use std::collections::BTreeMap;

use crate::all::ImportResult;
use crate::item::ReportItem;
use crate::options::LinkPrefix;
use crate::out::Out;

/// The facts of the run that the importer does not know: the clock, the version and the options.
#[derive(Debug, Clone)]
pub struct ReportMeta {
    /// The version of the `hune` binary.
    pub hune_version: String,
    /// The time of the run, as `YYYY-MM-DDThh:mm:ssZ`.
    pub created_at: String,
    /// The input directory.
    pub input_dir: String,
    /// The source map of the run.
    pub source_map: BTreeMap<String, String>,
    /// The link prefix.
    pub link_prefix: Option<LinkPrefix>,
    /// The name of the folder map file, when the run has one.
    pub folders_file: Option<String>,
    /// The name of the acceptance file, when the run has one.
    pub accept_file: Option<String>,
    /// True with `--write`.
    pub write: bool,
    /// True with `--replace`.
    pub replace: bool,
}

/// The two forms of the report: the JSON for programs and the Markdown for a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedReport {
    /// The content of `<path>.json`.
    pub json: String,
    /// The content of `<path>.md`.
    pub markdown: String,
}

fn item_out(item: &ReportItem) -> Out {
    Out::obj([
        ("id", Out::s(&item.id)),
        ("class", Out::s(item.class.as_str())),
        ("panel_id", item.panel_id.map_or(Out::Null, Out::Int)),
        ("panel_title", Out::opt_s(item.panel_title.as_deref())),
        ("member", Out::s(&item.member)),
        ("reason", Out::s(&item.reason)),
        ("fallback", Out::opt_s(item.fallback.as_ref().map(|f| f.text()))),
        ("value", Out::opt_s(item.value.as_deref())),
        ("truncated", Out::Bool(item.truncated)),
        ("accepted", Out::Bool(item.accepted)),
    ])
}

fn count(n: u32) -> Out {
    Out::Int(i64::from(n))
}

/// Writes the report of 15.11. The JSON lists the dashboards in the order of their Grafana uid.
/// The Markdown has the six parts of 15.11.
pub fn render_report(result: &ImportResult, meta: &ReportMeta) -> RenderedReport {
    RenderedReport { json: render_json(result, meta), markdown: render_markdown(result, meta) }
}

fn render_json(result: &ImportResult, meta: &ReportMeta) -> String {
    let t = result.totals();
    let files: Vec<Out> = result
        .files
        .iter()
        .map(|f| {
            Out::obj([
                ("name", Out::s(&f.name)),
                ("sha256", Out::opt_s(f.sha256.as_deref())),
                ("form", Out::s(&f.form)),
            ])
        })
        .collect();
    let source_map = Out::Obj(meta.source_map.iter().map(|(k, v)| (k.clone(), Out::s(v))).collect());
    let link_prefix = match &meta.link_prefix {
        Some(p) => Out::obj([("old", Out::s(&p.old)), ("new", Out::s(&p.new))]),
        None => Out::Null,
    };
    let folders: Vec<Out> = result
        .folders
        .iter()
        .map(|f| {
            Out::obj([
                ("grafana_uid", Out::opt_s(f.grafana_uid.as_deref())),
                ("id", Out::s(&f.id)),
                ("title", Out::s(&f.title)),
                ("result", Out::s(f.result.as_str())),
            ])
        })
        .collect();
    let dashboards: Vec<Out> = result
        .dashboards
        .iter()
        .map(|d| {
            Out::obj([
                ("grafana_uid", Out::s(&d.grafana_uid)),
                ("uid", Out::s(&d.uid)),
                ("folder", Out::s(&d.folder)),
                ("title", Out::s(&d.title)),
                ("panels", count(d.panels)),
                ("metadata", count(d.metadata)),
                ("result", Out::s(d.result.as_str())),
                ("etag", Out::opt_s(d.etag.as_deref())),
                ("items", Out::Arr(d.items.iter().map(item_out).collect())),
            ])
        })
        .collect();
    let other: Vec<Out> = result.folder_items.iter().chain(result.short_link_items.iter()).map(item_out).collect();
    let redirects: Vec<Out> =
        result.redirects.iter().map(|r| Out::obj([("from", Out::s(&r.from)), ("to", Out::s(&r.to))])).collect();
    let report = Out::obj([
        ("report_version", Out::Int(1)),
        ("hune_version", Out::s(&meta.hune_version)),
        ("created_at", Out::s(&meta.created_at)),
        ("mode", Out::s(if meta.write { "write" } else { "dry_run" })),
        (
            "input",
            Out::obj([
                ("dir", Out::s(&meta.input_dir)),
                ("grafana_version", Out::opt_s(result.grafana_version.as_deref())),
                ("clickhouse_plugin_version", Out::opt_s(result.plugin_version.as_deref())),
                ("files", Out::Arr(files)),
                ("ignored_files", Out::strings(&result.ignored_files)),
            ]),
        ),
        (
            "options",
            Out::obj([
                ("source_map", source_map),
                ("link_prefix", link_prefix),
                ("folders_file", Out::opt_s(meta.folders_file.as_deref())),
                ("accept_file", Out::opt_s(meta.accept_file.as_deref())),
                ("write", Out::Bool(meta.write)),
                ("replace", Out::Bool(meta.replace)),
            ]),
        ),
        (
            "totals",
            Out::obj([
                ("dashboards", count(t.dashboards)),
                ("panels", count(t.panels)),
                ("mapped", count(t.mapped)),
                ("changed", count(t.changed)),
                ("dropped", count(t.dropped)),
                ("unsupported", count(t.unsupported)),
                ("metadata", count(t.metadata)),
                ("accepted", count(t.accepted)),
                ("unaccepted", count(t.unaccepted)),
                ("stale_accept_lines", count(t.stale_accept_lines)),
            ]),
        ),
        (
            "not_imported",
            Out::obj([
                ("history", Out::s("not in the export")),
                ("public_dashboards", count(result.not_imported.public_dashboards)),
                ("short_links", count(result.not_imported.short_links)),
            ]),
        ),
        ("folders", Out::Arr(folders)),
        ("dashboards", Out::Arr(dashboards)),
        ("other_items", Out::Arr(other)),
        ("redirects", Out::Arr(redirects)),
        ("exit_code", Out::Int(i64::from(result.exit_code()))),
    ]);
    report.write()
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\n', '\r'], " ")
}

fn item_rows(items: &[ReportItem], out: &mut String) {
    out.push_str("| Id | Class | Panel | Member | Reason | Fallback | Accepted |\n");
    out.push_str("|---|---|---|---|---|---|---|\n");
    for i in items {
        let panel = match (&i.panel_id, &i.panel_title) {
            (Some(id), Some(t)) => format!("{id}: {t}"),
            (Some(id), None) => id.to_string(),
            _ => String::new(),
        };
        out.push_str(&format!(
            "| `{}` | {} | {} | `{}` | {} | {} | {} |\n",
            cell(&i.id),
            i.class.as_str(),
            cell(&panel),
            cell(&i.member),
            cell(&i.reason),
            cell(i.fallback.as_ref().map_or("", |f| f.text())),
            if i.accepted { "yes" } else { "no" },
        ));
    }
    out.push('\n');
}

fn render_markdown(result: &ImportResult, meta: &ReportMeta) -> String {
    let t = result.totals();
    let mut md = String::new();
    md.push_str("# Import report\n\n");
    md.push_str("## Input\n\n| Name | Value |\n|---|---|\n");
    md.push_str(&format!("| Input directory | `{}` |\n", cell(&meta.input_dir)));
    md.push_str(&format!("| Grafana version | {} |\n", result.grafana_version.as_deref().unwrap_or("unknown")));
    md.push_str(&format!(
        "| ClickHouse plugin version | {} |\n",
        result.plugin_version.as_deref().unwrap_or("unknown")
    ));
    md.push_str(&format!("| Mode | `{}` |\n\n", if meta.write { "write" } else { "dry_run" }));
    md.push_str("## Totals\n\n| Total | Count |\n|---|---|\n");
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
        md.push_str(&format!("| {name} | {n} |\n"));
    }
    md.push_str("\n## Objects that are not imported\n\n");
    md.push_str("- History: not in the export.\n");
    md.push_str(&format!("- Public dashboards: {}.\n", result.not_imported.public_dashboards));
    md.push_str(&format!("- Short links: {}.\n\n", result.not_imported.short_links));
    md.push_str("## Dashboards\n\n");
    for d in &result.dashboards {
        md.push_str(&format!("### `{}`: {}\n\n", d.grafana_uid, cell(&d.title)));
        md.push_str("| Result | Hune uid | Folder | Panels | Metadata members |\n|---|---|---|---|---|\n");
        md.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | {} |\n\n",
            d.result.as_str(),
            d.uid,
            d.folder,
            d.panels,
            d.metadata
        ));
        if d.items.is_empty() {
            md.push_str("No item needs acceptance.\n\n");
        } else {
            item_rows(&d.items, &mut md);
        }
    }
    if !result.folder_items.is_empty() || !result.short_link_items.is_empty() {
        md.push_str("### Folders and short links\n\n");
        let all: Vec<ReportItem> = result.folder_items.iter().chain(result.short_link_items.iter()).cloned().collect();
        item_rows(&all, &mut md);
    }
    md.push_str("## Redirects\n\n```nginx\n");
    md.push_str(&render_nginx_map(result));
    md.push_str("```\n\n## Exit code\n\n");
    md.push_str(&format!("Exit code: {}.\n", result.exit_code()));
    md.push_str(&format!("Reason: {}\n", result.exit_reason()));
    if !result.stale_accept_lines.is_empty() {
        md.push_str("\nStale acceptance lines:\n\n");
        for s in &result.stale_accept_lines {
            md.push_str(&format!("- `{}`\n", cell(s)));
        }
    }
    md
}

fn lines_block(name_var: &str, value_var: &str, lines: &[(String, String)], out: &mut String) {
    out.push_str(&format!("map ${name_var} ${value_var} {{\n    default \"\";\n"));
    for (k, v) in lines {
        out.push_str(&format!("    \"{k}\" \"{v}\";\n"));
    }
    out.push_str("}\n");
}

/// Writes the nginx maps of 15.17: the dashboard map and the short link map.
pub fn render_nginx_map(result: &ImportResult) -> String {
    let mut out = String::from("# Written by hune import-grafana. Include in the http context.\n");
    lines_block("hune_grafana_uid", "hune_new_uid", &result.uid_lines, &mut out);
    lines_block("hune_grafana_short", "hune_short_target", &result.short_lines, &mut out);
    out
}
