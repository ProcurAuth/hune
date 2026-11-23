//! The names of the saved queries (decision ID-K176, 6.14, 7.19): the JSON reader, the dependency scan
//! and the member `names` of a document. The golden test of the page names is `names_golden.rs`.

use hune_core::document::{DocumentNames, dependency_names, referenced_names, validate};
use serde_json::{Value, json};

const NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/nodes.json");
const QUERIES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/queries.json");

/// The distinct names of a scan, in order of first use. The scans of the readers can find a name twice.
fn distinct(scan: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in scan {
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

fn names(v: &Value) -> DocumentNames {
    let doc = validate(&serde_json::to_vec(v).unwrap(), None).unwrap_or_else(|e| panic!("{e:?}"));
    DocumentNames::of(doc.document())
}

#[test]
fn the_json_reader_reads_each_string_with_its_escapes_decoded() {
    // The text of a MongoDB query (7.34): a placeholder in a value, in a key and behind an escape.
    let text = r#"{"collection": "c", "aggregate": [{"$match": {"host": "$host", "${key}": "\u0024esc", "n": "${n:number}"}}]}"#;
    assert_eq!(distinct(referenced_names(text)), ["match", "host", "key", "esc", "n"]);
    // A lone surrogate does not hide the name behind it, as `JSON.parse` reads it.
    assert_eq!(distinct(referenced_names(r#"{"a": "\ud800\u0024lone", "b": "\ud83d\ude00$pair"}"#)), ["lone", "pair"]);
    // A string that is not valid JSON stays as written: a bad escape, a raw line feed, no end.
    assert_eq!(distinct(referenced_names("{\"a\": \"\\x$bad\", \"b\": \"line\n$nl\"}")), ["bad", "nl"]);
    assert_eq!(distinct(referenced_names("{\"a\": \"$open")), ["open"]);
    assert_eq!(distinct(referenced_names("{\"a\": \"$closed\", \"b\": \"$cut\\")), ["closed", "cut"]);
    // An escaped quote does not end the string, and a decoded text is read again as text.
    assert_eq!(distinct(referenced_names(r#"{"a": "{\"b\": \"$inner\"}"}"#)), ["inner"]);
    assert!(referenced_names(r#"{"a": "\\u0024no"}"#).is_empty());
}

#[test]
fn the_dependency_scan_reads_each_place_that_a_lexer_skips() {
    // A comment holds no reference for the name scan of the checks, and a name for the dependency scan.
    assert!(referenced_names("select 1 -- $a\n").is_empty());
    assert_eq!(distinct(dependency_names("select 1 -- $a\n, $b /* ${c} */")), ["a", "b", "c"]);
    // A heredoc and a dollar quote too.
    assert_eq!(distinct(dependency_names("select $$ $a $$, $tag$ ${b} $tag$")), ["a", "tag", "b"]);
    // The scan never finds fewer names than the name scan.
    for text in ["select '$a', \"$b\", `$c`", "{\"x\": \"\\u0024d\"}", "select E'\\' $e'", "$f$g$"] {
        let loose = dependency_names(text);
        for name in referenced_names(text) {
            assert!(loose.contains(&name), "{text:?}: {name:?} is lost");
        }
    }
}

#[test]
fn the_names_of_the_nodes_fixture() {
    let n = names(&serde_json::from_slice(NODES).unwrap());
    // Panel 1 names the query variable. Panel 2 names the three variables of the text, in order of use.
    // The text panel has no query. The macros and the names of no variable are not in the lists.
    assert_eq!(n.panels.len(), 2);
    assert_eq!(n.panels[0], (1, vec![vec!["node".to_owned()]]));
    assert_eq!(n.panels[1].1, vec![vec!["account".to_owned(), "env".to_owned(), "node".to_owned()]]);
    assert_eq!(n.variables, vec![("node".to_owned(), Vec::<String>::new())]);
    assert!(n.annotations.is_empty());
    assert_eq!(
        serde_json::from_str::<Value>(&n.to_json()).unwrap(),
        json!({
            "panels": { "1": [["node"]], "2": [["account", "env", "node"]] },
            "variables": { "node": [] },
            "annotations": []
        })
    );
}

#[test]
fn each_query_of_a_panel_each_variable_and_each_annotation_has_its_list() {
    let mut doc: Value = serde_json::from_slice(QUERIES).unwrap();
    doc["variables"] = json!([
        { "kind": "custom", "name": "env", "label": null, "hide": "none",
          "options": [{ "text": "P", "value": "p" }], "multi": false, "include_all": false, "all_value": null,
          "default": ["p"] },
        { "kind": "text", "name": "host", "label": null, "hide": "none", "value_type": "text", "default": "" },
        { "kind": "query", "name": "zone", "label": null, "hide": "none", "source": "prod",
          "sql": ["select z from t where e = '${env}' and h = ${host}"], "multi": false, "include_all": false,
          "all_value": null, "refresh": "load", "sort": "alpha_asc", "regex": null, "default": ["a"] },
        { "kind": "query", "name": "node", "label": null, "hide": "none", "source": "prod",
          "sql": ["select n from t where z = ${zone} -- ${node} ${env}"], "multi": false, "include_all": false,
          "all_value": null, "refresh": "load", "sort": "alpha_asc", "regex": null, "default": ["a"] }
    ]);
    let data = &mut doc["sections"][0]["panels"][0]["data"];
    data["query"]["sql"] = json!(["select 1 where e = '${env}'"]);
    data["extra_queries"][0]["sql"] = json!(["select 2"]);
    data["extra_queries"][1]["sql"] = json!(["select 3 where h = ${host} and z = ${zone} and x = ${nobody}"]);
    doc["annotations"] = json!([
        { "name": "A", "source": "prod", "sql": ["select t as time where $__timeFilter(t) and z = ${zone}"],
          "color": "#1f78c1" },
        { "name": "B", "source": "prod", "sql": ["select t as time"], "color": "#1f78c1" }
    ]);
    let n = names(&doc);
    // Panel 1 has three lists, in the order of 4.15a. A name of no variable and a macro are left out.
    assert_eq!(n.panels[0], (1, vec![vec!["env".to_owned()], vec![], vec!["host".to_owned(), "zone".to_owned()]]));
    // A panel with no variable name has a list of no name, and each panel with a query has its key.
    assert_eq!(n.panels.iter().map(|p| p.0).collect::<Vec<_>>(), [1, 2, 3, 4]);
    assert!(n.panels[1].1.iter().all(Vec::is_empty));
    // A query variable has its list, with no own name. The loose scan counts the name in the comment.
    assert_eq!(
        n.variables,
        vec![
            ("zone".to_owned(), vec!["env".to_owned(), "host".to_owned()]),
            ("node".to_owned(), vec!["zone".to_owned(), "env".to_owned()]),
        ]
    );
    assert_eq!(n.annotations, vec![vec!["zone".to_owned()], vec![]]);
    // The JSON text has the three members, and the keys of the panels are decimal texts.
    let json: Value = serde_json::from_str(&n.to_json()).unwrap();
    assert_eq!(json["panels"]["1"], json!([["env"], [], ["host", "zone"]]));
    assert_eq!(json["annotations"], json!([["zone"], []]));
}

// ---- the cost of the name scan (finding 1 and 2 of the review of 2026-11-14) ----

/// A hidden query and comment lines that hold `n` unclosed `$tag$` openers. The save checks of `clickhouse` pass,
/// because the `#` line is a comment. The text has 9 bytes for each tag.
fn tag_lines(n: usize) -> Vec<String> {
    let mut out = vec!["SELECT 1 AS x".to_owned()];
    let mut line = String::from("# ");
    for k in 0..n {
        line.push_str(&format!("$t{k:05}$ "));
        if line.len() > 4000 {
            out.push(std::mem::replace(&mut line, String::from("# ")));
        }
    }
    out.push(line);
    out
}

/// The time bound of a debug run. The linear scan needs a few milliseconds. The old scan needed 20 seconds.
const BOUND: std::time::Duration = std::time::Duration::from_secs(1);

#[test]
fn the_scan_of_28000_unclosed_tags_is_linear() {
    let text = tag_lines(28_000).join("\n");
    assert!(text.len() > 250_000);
    let t = std::time::Instant::now();
    let names = dependency_names(&text);
    let refs = referenced_names(&text);
    assert!(t.elapsed() < BOUND, "the scan needs {:?}", t.elapsed());
    // No name is lost: each opener is read as a placeholder, as the scan did before.
    assert!(names.len() >= 28_000 && refs.len() >= 28_000, "{} {}", names.len(), refs.len());
    assert!(names.contains(&"t27999".to_owned()) && names.contains(&"t00000".to_owned()));
}

#[test]
fn the_scan_of_closed_and_unclosed_tags_keeps_its_answer() {
    // A closed heredoc and a closed dollar quote hide their text. An unclosed opener hides nothing.
    assert_eq!(distinct(referenced_names("select $a$ $x $a$, $b$ $y, $z")), ["b", "y", "z"]);
    // The closing tag must be the same tag: `$a$` does not close `$ab$`. Two `$a$` after it make a pair.
    assert_eq!(distinct(referenced_names("$ab$ $m $a$ $n $ab$ $o $a$ $p $a$ $q")), ["o", "q"]);
    // Many distinct openers and one closing pair that is far away.
    let mut text = String::new();
    for k in 0..4999 {
        text.push_str(&format!("$u{k}$ "));
    }
    text.push_str("$u4999$ $hidden $u4999$ ${visible}");
    let names = distinct(referenced_names(&text));
    assert!(!names.contains(&"hidden".to_owned()), "a name in a closed heredoc");
    assert!(names.contains(&"visible".to_owned()) && names.contains(&"u0".to_owned()));
}

#[test]
fn a_variable_with_28000_tags_validates_in_a_bound_time() {
    // Finding 2: validate reads the names of each query variable before any gate.
    let mut doc: Value = serde_json::from_slice(NODES).unwrap();
    doc["variables"][0]["sql"] = json!(tag_lines(28_000));
    let bytes = serde_json::to_vec(&doc).unwrap();
    let t = std::time::Instant::now();
    let r = validate(&bytes, None);
    assert!(t.elapsed() < BOUND, "validate needs {:?}", t.elapsed());
    assert!(r.is_ok(), "{r:?}");
}

#[test]
fn a_panel_with_28000_tags_gives_its_names_in_a_bound_time() {
    let mut doc: Value = serde_json::from_slice(NODES).unwrap();
    doc["sections"][0]["panels"][0]["data"]["query"]["sql"] = json!(tag_lines(28_000));
    let bytes = serde_json::to_vec(&doc).unwrap();
    let t = std::time::Instant::now();
    let parsed = validate(&bytes, None).unwrap_or_else(|e| panic!("{e:?}"));
    let n = DocumentNames::of(parsed.document());
    assert!(t.elapsed() < BOUND, "the names need {:?}", t.elapsed());
    assert!(n.panels[0].1[0].is_empty(), "the tags name no variable");
}
