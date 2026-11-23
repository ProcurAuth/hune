//! Helpers of the integration tests: the synthetic export and the golden files.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use hune_core::etag::Etag;
use hune_grafana::{
    DashboardFile, FolderTable, ImportInput, ImportOptions, LinkPrefix, parse_acceptance_file, parse_folder_map,
};
use serde_json::Value;

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repository root")
}

pub fn export_dir() -> PathBuf {
    repo_root().join("tests/fixtures/grafana/export")
}

pub fn golden_dir() -> PathBuf {
    repo_root().join("tests/golden/import")
}

pub fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

pub fn load_input() -> ImportInput {
    let dir = export_dir();
    let mut names: Vec<_> = fs::read_dir(dir.join("dashboards"))
        .expect("dashboards dir")
        .map(|e| e.expect("entry").file_name().into_string().expect("name"))
        .collect();
    names.sort();
    let dashboards = names
        .iter()
        .map(|n| DashboardFile {
            name: format!("dashboards/{n}"),
            stem: n.trim_end_matches(".json").to_owned(),
            sha256: Some(Etag::of(&fs::read(dir.join("dashboards").join(n)).expect("read")).to_hex()),
            value: read_json(&dir.join("dashboards").join(n)),
        })
        .collect();
    ImportInput {
        dashboards,
        search: Some(read_json(&dir.join("search.json"))),
        health: Some(read_json(&dir.join("health.json"))),
        plugin: Some(read_json(&dir.join("plugin-clickhouse.json"))),
        public_dashboards: Some(read_json(&dir.join("public-dashboards.json"))),
        short_urls: Some(read_json(&dir.join("short-urls.json"))),
        ignored_files: vec![],
    }
}

/// The options of the main run: source map, folder map, link prefix. `accept` is the acceptance text.
pub fn options(accept: &str) -> ImportOptions {
    let dir = export_dir();
    let map = parse_folder_map(&read_json(&dir.join("folders.map.json"))).expect("folder map");
    let folders = FolderTable::new(&read_json(&dir.join("folders.json")), map).expect("folder table");
    let mut o = ImportOptions::with_source_map(&[("clickhouse", "prod")]);
    o.folders = folders;
    o.link_prefix = Some(LinkPrefix::new("/grafana/", "/dashboards/").expect("prefix"));
    o.accept = parse_acceptance_file(accept).expect("acceptance");
    o
}

/// Compares text with a golden file. With `HUNE_BLESS` set, the file is written.
pub fn check_golden(name: &str, actual: &str) {
    let path = golden_dir().join(name);
    if std::env::var_os("HUNE_BLESS").is_some() {
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
    assert_eq!(expected, actual, "golden file {name} differs (run with HUNE_BLESS=1 to write it)");
}

// ------------------------------------------------------------------ small dashboards

use hune_core::document::validate;
use hune_core::ids::Uid;
use hune_grafana::{DashboardImport, ReportItem, import_dashboard};
use serde_json::json;

pub fn ch() -> Value {
    json!({"type": "grafana-clickhouse-datasource", "uid": "clickhouse"})
}

/// A target with a ClickHouse query.
pub fn target(sql: &str) -> Value {
    json!({"datasource": ch(), "editorType": "sql", "format": 1, "rawSql": sql, "refId": "A", "queryType": "table", "pluginVersion": "4.5.1"})
}

/// A v1 panel at the place `y`, with one ClickHouse query.
pub fn panel(id: i64, ptype: &str, y: i64) -> Value {
    json!({
        "id": id, "type": ptype, "title": format!("Panel {id}"), "datasource": ch(),
        "gridPos": {"h": 4, "w": 12, "x": 0, "y": y},
        "targets": [target("SELECT 1 AS a")],
        "fieldConfig": {"defaults": {}, "overrides": []}, "options": {}
    })
}

/// A raw v1 dashboard with the uid `t1`.
pub fn dash(panels: Vec<Value>, variables: Vec<Value>) -> Value {
    json!({
        "uid": "t1", "title": "Test", "schemaVersion": 41, "time": {"from": "now-6h", "to": "now"},
        "refresh": "", "templating": {"list": variables}, "panels": panels
    })
}

pub fn base_options() -> ImportOptions {
    ImportOptions::with_source_map(&[("clickhouse", "prod"), ("old-name", "legacy")])
}

/// Imports with no acceptance.
pub fn run_plain(d: &Value) -> DashboardImport {
    import_dashboard(d, &base_options())
}

/// Imports twice: the second run accepts each item of the first. This gives the document.
pub fn run(d: &Value) -> DashboardImport {
    let first = run_plain(d);
    let mut o = base_options();
    let mut ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
    ids.sort();
    ids.dedup();
    o.accept = parse_acceptance_file(&ids.join("\n")).expect("acceptance");
    import_dashboard(d, &o)
}

/// The document of an import, parsed.
pub fn doc(d: &DashboardImport) -> Value {
    let bytes =
        d.document_bytes.as_ref().unwrap_or_else(|| panic!("no document; result {:?}; items {:#?}", d.result, d.items));
    let uid = Uid::parse(&d.uid).expect("uid");
    let valid = validate(bytes, Some(&uid)).unwrap_or_else(|e| panic!("invalid document: {e:?}"));
    assert_eq!(valid.bytes(), bytes.as_slice(), "the importer gives the canonical bytes");
    serde_json::from_slice(bytes).expect("json")
}

/// The first panel of the first section.
pub fn first_panel(d: &Value) -> &Value {
    &d["sections"][0]["panels"][0]
}

pub fn items_with<'a>(d: &'a DashboardImport, member: &str) -> Vec<&'a ReportItem> {
    d.items.iter().filter(|i| i.member == member || i.member.starts_with(&format!("{member}."))).collect()
}

pub fn one_item<'a>(d: &'a DashboardImport, member: &str) -> &'a ReportItem {
    let found = items_with(d, member);
    assert_eq!(found.len(), 1, "items for {member}: {:#?}", d.items);
    found[0]
}

pub fn no_items(d: &DashboardImport) {
    assert!(d.items.is_empty(), "unexpected items: {:#?}", d.items);
}

// ------------------------------------------------------------------ a small JSON Schema check

/// Checks a value against the subset of JSON Schema that `report.schema.json` uses. The subset
/// has `$ref` to `#/$defs`, `type`, `const`, `enum`, `required`, `properties`,
/// `additionalProperties`, `items` and `minimum`. The keyword `pattern` is not checked here.
pub fn schema_errors(root: &Value, schema: &Value, value: &Value, at: &str, out: &mut Vec<String>) {
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
    if let (Some(min), Some(n)) = (schema.get("minimum").and_then(Value::as_i64), value.as_i64())
        && n < min
    {
        out.push(format!("{at}: {n} is below {min}"));
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
