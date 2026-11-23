// SPDX-License-Identifier: MIT OR Apache-2.0
//! Golden vectors of the kind `mongodb` (16.13): `tests/golden/pipeline-mongo/` and
//! `tests/golden/expansion-mongo/`.
//!
//! The pipeline files hold one accepted text and one refused text for each name of 7.31 and 7.32, the
//! profile J and the limits. The test runs each text through the checks at save and through the run, and
//! it requires that both agree (case MS17 without a server). The expected frame of each accepted text
//! needs a server (MS02) and is not in these files.
//!
//! The expansion files hold each macro, each format and the injection vectors of 7.35. The expected text is
//! the compact Extended JSON text of the expanded query. Each case that the files give as an error names the
//! API code, and the variable and the reason when the code is `variable_value_refused`.

use hune_core::mongo::{
    MongoExpandError, MongoExpandRequest, SaveContext, WarnKind, prepare, save_checks, to_query_json,
};
use hune_core::sql::{
    RequestValues, SqlUse, TextValueType, ValuesError, VariableDef, check_time_range, compute_interval, resolve_values,
};
use serde_json::Value;

const PIPELINE: [(&str, &str); 7] = [
    ("stages.json", include_str!("../../../tests/golden/pipeline-mongo/stages.json")),
    ("query-operators.json", include_str!("../../../tests/golden/pipeline-mongo/query-operators.json")),
    ("expression-operators.json", include_str!("../../../tests/golden/pipeline-mongo/expression-operators.json")),
    ("accumulators.json", include_str!("../../../tests/golden/pipeline-mongo/accumulators.json")),
    ("window-operators.json", include_str!("../../../tests/golden/pipeline-mongo/window-operators.json")),
    ("refused.json", include_str!("../../../tests/golden/pipeline-mongo/refused.json")),
    ("json-profile.json", include_str!("../../../tests/golden/pipeline-mongo/json-profile.json")),
];

const EXPANSION: [(&str, &str); 4] = [
    ("macros.json", include_str!("../../../tests/golden/expansion-mongo/macros.json")),
    ("formats.json", include_str!("../../../tests/golden/expansion-mongo/formats.json")),
    ("embedded.json", include_str!("../../../tests/golden/expansion-mongo/embedded.json")),
    ("injection.json", include_str!("../../../tests/golden/expansion-mongo/injection.json")),
];

fn code_text(c: hune_core::error::ErrorCode) -> &'static str {
    c.as_str()
}

#[test]
fn golden_pipeline_vectors() {
    let collections: Vec<String> = ["node_load", "hosts", "events"].iter().map(|s| (*s).to_owned()).collect();
    let databases = vec!["metrics".to_owned()];
    let mut total = 0;
    let (mut accepted, mut refused) = (0, 0);
    for (file, text) in PIPELINE {
        let doc: Value = serde_json::from_str(text).unwrap_or_else(|e| panic!("{file}: {e}"));
        for case in doc["cases"].as_array().unwrap() {
            total += 1;
            let id = case["id"].as_str().unwrap();
            let lines: Vec<&str> = case["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
            let ctx =
                SaveContext { variables: &[], collections: &collections, databases: &databases, usage: SqlUse::Panel };
            let saved = save_checks(&lines, &ctx);
            let values = resolve_values(&[], &RequestValues::new()).unwrap();
            let range = check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap();
            let run = prepare(
                &lines,
                &MongoExpandRequest {
                    values: &values,
                    range,
                    interval: Some(compute_interval(&range, 1200, 1).unwrap()),
                    max_query_size: 8_388_608,
                    collections: &collections,
                    databases: &databases,
                },
            );
            match case["expect"].as_str().unwrap() {
                "accepted" => {
                    accepted += 1;
                    assert!(saved.faults.is_empty(), "{file} {id}: refused at save: {:?}", saved.faults);
                    if let Err(e) = run {
                        panic!("{file} {id}: refused at run: {e}");
                    }
                }
                "refused" => {
                    refused += 1;
                    let code = case["code"].as_str().unwrap();
                    assert!(
                        saved.faults.iter().any(|f| code_text(f.code) == code),
                        "{file} {id}: expected {code} at save, got {:?}",
                        saved.faults
                    );
                    if let Some(c) = case.get("contains").and_then(Value::as_str) {
                        assert!(
                            saved.faults.iter().any(|f| f.message.contains(c)),
                            "{file} {id}: no fault names {c}: {:?}",
                            saved.faults
                        );
                    }
                    match run {
                        Err(MongoExpandError::Invalid(i)) => {
                            if let Some(c) = case.get("contains").and_then(Value::as_str) {
                                assert!(i.message.contains(c) || i.pointer.contains(c), "{file} {id}: {i}");
                            }
                        }
                        other => panic!("{file} {id}: expected sql_invalid at run, got {other:?}"),
                    }
                }
                other => panic!("{file} {id}: expect {other}"),
            }
        }
    }
    eprintln!("golden pipeline: {total} cases, {accepted} accepted, {refused} refused");
    assert!(total >= 900, "{total}");
    assert!(accepted >= 250 && refused >= 500);
}

fn def(v: &Value) -> VariableDef {
    let name = v["name"].as_str().unwrap();
    let all_value = v.get("all_value").and_then(Value::as_str);
    let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    match v["kind"].as_str().unwrap() {
        "query" => VariableDef::query(name, flag("multi"), flag("include_all"), all_value),
        "custom" => {
            let options: Vec<&str> = v["options"].as_array().unwrap().iter().map(|o| o.as_str().unwrap()).collect();
            VariableDef::custom(name, &options, flag("multi"), flag("include_all"), all_value)
        }
        "text" => {
            let t = if v["value_type"] == "number" { TextValueType::Number } else { TextValueType::Text };
            VariableDef::text(name, t)
        }
        other => panic!("kind {other}"),
    }
}

/// Runs a case of the expansion files. It gives the compact text of the expanded query, or the error.
fn run_expansion(case: &Value) -> Result<String, (String, String, String)> {
    let defs: Vec<VariableDef> = case["variables"].as_array().unwrap().iter().map(def).collect();
    let request = RequestValues::from_json(&case["values"]).expect("values");
    let lines: Vec<&str> = case["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
    let get = |k: &str, d: i64| case.get(k).and_then(Value::as_i64).unwrap_or(d);
    let range = check_time_range(get("from_ms", 1_794_211_200_000), get("to_ms", 1_794_232_800_000)).unwrap();
    let interval = if case.get("usage").and_then(Value::as_str) == Some("variable") {
        None
    } else {
        Some(compute_interval(&range, get("max_data_points", 1200) as u32, get("min_interval_ms", 1) as u64).unwrap())
    };
    let values = match resolve_values(&defs, &request) {
        Ok(v) => v,
        Err(ValuesError::Refused(r)) => return Err(("variable_value_refused".into(), r.variable, r.reason)),
        Err(ValuesError::BadRequest(m)) => return Err(("bad_request".into(), m, String::new())),
    };
    let collections: Vec<String> = ["node_load", "hosts", "events"].iter().map(|s| (*s).to_owned()).collect();
    let databases = vec!["metrics".to_owned()];
    let res = prepare(
        &lines,
        &MongoExpandRequest {
            values: &values,
            range,
            interval,
            max_query_size: get("max_query_size", 262_144) as usize,
            collections: &collections,
            databases: &databases,
        },
    );
    match res {
        Ok(q) => Ok(to_query_json(&q.to_value())),
        Err(e) => {
            let (variable, reason) = match &e {
                MongoExpandError::ValueRefused(r) => (r.variable.clone(), r.reason.clone()),
                MongoExpandError::MissingVariable(n) => (n.clone(), String::new()),
                _ => (String::new(), String::new()),
            };
            Err((e.api_code().to_owned(), variable, reason))
        }
    }
}

#[test]
fn golden_expansion_vectors() {
    let mut total = 0;
    let mut tags = std::collections::BTreeSet::new();
    for (file, text) in EXPANSION {
        let doc: Value = serde_json::from_str(text).unwrap_or_else(|e| panic!("{file}: {e}"));
        for case in doc["cases"].as_array().unwrap() {
            total += 1;
            let id = case["id"].as_str().unwrap();
            for t in case["tags"].as_array().unwrap() {
                tags.insert(t.as_str().unwrap().to_owned());
            }
            let got = run_expansion(case);
            match (case.get("expected"), case.get("error")) {
                (Some(want), None) => {
                    let want = want.as_str().unwrap();
                    match got {
                        Ok(text) => assert_eq!(text, want, "{file} {id}"),
                        Err(e) => panic!("{file} {id}: error {e:?}"),
                    }
                }
                (None, Some(err)) => {
                    let code = err["code"].as_str().unwrap();
                    match got {
                        Ok(text) => panic!("{file} {id}: expected {code}, got {text}"),
                        Err((c, variable, reason)) => {
                            assert_eq!(c, code, "{file} {id}: {variable} {reason}");
                            if let Some(v) = err.get("variable").and_then(Value::as_str) {
                                assert_eq!(variable, v, "{file} {id}");
                            }
                            if let Some(r) = err.get("reason").and_then(Value::as_str) {
                                assert_eq!(reason, r, "{file} {id}");
                            }
                        }
                    }
                }
                _ => panic!("{file} {id}: a case has one of expected and error"),
            }
        }
    }
    // The files cover each macro and each format.
    for m in ["$__timeFrom", "$__timeTo", "$__from", "$__to", "$__interval_ms", "$__interval_s"] {
        assert!(tags.contains(&format!("macro:{m}")), "{m}");
    }
    for f in ["text", "number", "date", "oid", "bool", "json", "json_number", "json_oid"] {
        assert!(tags.contains(&format!("whole:{f}")), "{f}");
    }
    for f in [
        "raw",
        "csv",
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
    ] {
        assert!(tags.contains(&format!("embedded:{f}")), "{f}");
    }
    eprintln!("golden expansion: {total} cases");
    assert!(total >= 100, "{total}");
}

#[test]
fn the_save_warnings_of_the_vectors_are_not_faults() {
    // The warning kinds exist for the page: a warning is never a fault.
    let ctx =
        SaveContext { variables: &[], collections: &["node_load".to_owned()], databases: &[], usage: SqlUse::Panel };
    let r = save_checks(&[r#"{"collection":"node_load","find":{"filter":{"a":"${x}"}}}"#], &ctx);
    assert!(r.faults.is_empty());
    assert_eq!(r.warnings.iter().map(|w| w.kind).collect::<Vec<_>>(), [WarnKind::UnknownVariable]);
}
