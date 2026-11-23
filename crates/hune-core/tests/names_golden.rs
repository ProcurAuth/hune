// SPDX-License-Identifier: MIT OR Apache-2.0
//! The golden test of the name scan (6.14, 7.19).
//!
//! The files `tests/golden/names/*.json` hold the names that the scan of the page found for each query
//! text and each document of the repository. The scan of the server MUST name every one of them: a missed
//! name is a failure, because a panel would keep old data. A name that only the server finds is an extra
//! name. Each extra name needs a reason in `tests/golden/names/extra.json`, and the list holds no stale item.
//! The generator is `tests/tools/golden_names.mjs`.

use hune_core::document::{DocumentNames, dependency_names, is_valid_variable_name, referenced_names, validate};
use serde_json::Value;
use std::collections::BTreeMap;

const TEXTS: [(&str, &str); 3] = [
    ("texts.json", include_str!("../../../tests/golden/names/texts.json")),
    ("curated.json", include_str!("../../../tests/golden/names/curated.json")),
    ("random.json", include_str!("../../../tests/golden/names/random.json")),
];
const DOCUMENTS: &str = include_str!("../../../tests/golden/names/documents.json");
const EXTRA: &str = include_str!("../../../tests/golden/names/extra.json");

/// The server names of a text that can name a variable: distinct, in order. A name that is not a variable
/// name (4.3) cannot be in the member `names`, so the comparison leaves it out on both sides: `$1` is one.
fn server_names(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in dependency_names(text) {
        if is_valid_variable_name(&name) && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_owned()).collect()
}

/// The formats that a reader of the page accepts (7.9, 7.34). The page reads another format in no place.
const KNOWN_FORMATS: [&str; 21] = [
    "csv",
    "singlequote",
    "sqlstring",
    "raw",
    "json",
    "pipe",
    "regex",
    "glob",
    "lucene",
    "percentencode",
    "queryparam",
    "distributed",
    "doublequote",
    "html",
    "text",
    "number",
    "date",
    "oid",
    "bool",
    "json_number",
    "json_oid",
];

/// Tells why the server finds a name that the page does not (the keys of `reasons` in `extra.json`).
/// `format`: the text has `${name:...}` with a format that the page does not read, or with no closing brace.
/// `lexer`: the exact scan of 7.19 finds the name, and the copied lexer of the page does not.
/// `loose`: only the loose scan finds the name, in a place that each lexer skips.
fn reason_of(text: &str, name: &str) -> &'static str {
    let open = format!("${{{name}:");
    let bad_format = text.match_indices(&open).any(|(at, _)| {
        let rest = &text[at + open.len()..];
        let fmt: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        !(rest[fmt.len()..].starts_with('}') && KNOWN_FORMATS.contains(&fmt.as_str()))
    });
    if bad_format {
        "format"
    } else if referenced_names(text).iter().any(|n| n == name) {
        "lexer"
    } else {
        "loose"
    }
}

/// The extra names that the server finds and the page does not: `(place, name)`, with their reason.
type Extras = BTreeMap<(String, String), String>;

fn check_texts() -> (Extras, usize) {
    let mut extras = Extras::new();
    let mut count = 0;
    let mut misses = Vec::new();
    for (file, text) in TEXTS {
        let cases: Value = serde_json::from_str(text).unwrap();
        for case in cases.as_array().unwrap() {
            let (id, text) = (case["id"].as_str().unwrap(), case["text"].as_str().unwrap());
            let page: Vec<String> = strings(&case["names"]).into_iter().filter(|n| is_valid_variable_name(n)).collect();
            let server = server_names(text);
            count += 1;
            for name in &page {
                if !server.contains(name) {
                    misses.push(format!("{file} {id}: the server misses {name:?}; text {text:?}; server {server:?}"));
                }
            }
            for name in server.iter().filter(|n| !page.contains(n)) {
                extras.insert((format!("{file}:{id}"), name.clone()), reason_of(text, name).to_owned());
            }
        }
    }
    assert!(
        misses.is_empty(),
        "{} names lost:\n{}",
        misses.len(),
        misses.iter().take(400).cloned().collect::<Vec<_>>().join("\n")
    );
    (extras, count)
}

/// The names of one list of the member `names`, from a JSON value.
fn lists(v: &Value) -> Vec<Vec<String>> {
    v.as_array().unwrap().iter().map(strings).collect()
}

fn check_documents() -> (Extras, usize) {
    let mut extras = Extras::new();
    let mut count = 0;
    let mut misses = Vec::new();
    let entries: Value = serde_json::from_str(DOCUMENTS).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
    for entry in entries.as_array().unwrap() {
        let (file, pointer) = (entry["file"].as_str().unwrap(), entry["pointer"].as_str().unwrap());
        let tree: Value = serde_json::from_slice(&std::fs::read(root.join(file)).unwrap()).unwrap();
        let doc = tree.pointer(pointer).unwrap();
        let bytes = serde_json::to_vec(doc).unwrap();
        let valid = validate(&bytes, None).unwrap_or_else(|e| panic!("{file}{pointer} must be valid: {e:?}"));
        let ours = DocumentNames::of(valid.document());
        let want = &entry["names"];
        let place = format!("{file}{pointer}");
        count += 1;
        let mut compare = |what: String, page: &[String], server: &[String]| {
            for name in page.iter().filter(|n| !server.contains(n)) {
                misses.push(format!("{place} {what}: the server misses {name:?}; server {server:?}"));
            }
            for name in server.iter().filter(|n| !page.contains(n)) {
                extras.insert((format!("documents.json:{place} {what}"), name.clone()), "document".to_owned());
            }
        };
        // Panels: the keys are the same, and each panel has one list for each query.
        let page_panels = want["panels"].as_object().unwrap();
        let ids: Vec<String> = ours.panels.iter().map(|(id, _)| id.to_string()).collect();
        let page_ids: Vec<&String> = page_panels.keys().collect();
        assert_eq!(page_ids.len(), ids.len(), "{place}: the panels with a query differ");
        for (id, queries) in &ours.panels {
            let page = lists(&page_panels[&id.to_string()]);
            assert_eq!(page.len(), queries.len(), "{place}: the queries of panel {id} differ");
            for (k, (p, s)) in page.iter().zip(queries).enumerate() {
                compare(format!("panel {id} query {k}"), p, s);
            }
        }
        let page_vars = want["variables"].as_object().unwrap();
        assert_eq!(page_vars.len(), ours.variables.len(), "{place}: the query variables differ");
        for (name, names) in &ours.variables {
            compare(format!("variable {name}"), &strings(&page_vars[name]), names);
        }
        let page_ann = lists(&want["annotations"]);
        assert_eq!(page_ann.len(), ours.annotations.len(), "{place}: the annotations differ");
        for (k, (p, s)) in page_ann.iter().zip(&ours.annotations).enumerate() {
            compare(format!("annotation {k}"), p, s);
        }
    }
    assert!(misses.is_empty(), "{} names lost:\n{}", misses.len(), misses.join("\n"));
    (extras, count)
}

/// The listed extra names: `(place, name)` with the reason that the file gives for each item.
fn listed() -> Extras {
    let file: Value = serde_json::from_str(EXTRA).unwrap();
    let reasons = file["reasons"].as_object().unwrap();
    let mut out = Extras::new();
    for item in file["extras"].as_array().unwrap() {
        let reason = item["reason"].as_str().unwrap();
        assert!(reasons.contains_key(reason), "unknown reason {reason:?}");
        for name in strings(&item["names"]) {
            let key = (item["place"].as_str().unwrap().to_owned(), name);
            assert!(out.insert(key, reason.to_owned()).is_none(), "duplicate item {item}");
        }
    }
    out
}

#[test]
fn the_server_names_every_name_that_the_page_found() {
    let (mut extras, texts) = check_texts();
    let (more, documents) = check_documents();
    extras.extend(more);
    // The size of the corpus: a generator that finds nothing must not pass.
    assert!(texts > 4_000 && documents >= 30, "texts {texts}, documents {documents}");
    // To write the list again, run this test with K176_EXTRAS_OUT=<path>: the file gets the items of `extras`.
    if let Ok(path) = std::env::var("K176_EXTRAS_OUT") {
        let mut by_place: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
        for ((place, name), reason) in &extras {
            by_place.entry((place.as_str(), reason.as_str())).or_default().push(name.as_str());
        }
        let items: Vec<Value> = by_place
            .iter()
            .map(|((place, reason), names)| serde_json::json!({ "place": place, "names": names, "reason": reason }))
            .collect();
        let text = items.iter().map(Value::to_string).collect::<Vec<_>>().join(",\n");
        std::fs::write(path, format!("[\n{text}\n]\n")).unwrap();
    }
    let listed = listed();
    let wrong: Vec<_> = extras.iter().filter(|(k, r)| listed.get(*k).is_some_and(|l| l != *r)).take(20).collect();
    let unlisted: Vec<_> = extras.keys().filter(|k| !listed.contains_key(*k)).take(20).collect();
    let stale: Vec<_> = listed.keys().filter(|k| !extras.contains_key(*k)).take(20).collect();
    assert!(wrong.is_empty(), "the reason differs, for example {wrong:?}");
    assert!(unlisted.is_empty(), "extra names have no reason, for example {unlisted:?}");
    assert!(stale.is_empty(), "listed extra names are gone, for example {stale:?}");
}
