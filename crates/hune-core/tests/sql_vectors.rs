//! Expansion vectors from Grafana (16.13, case I15).
//!
//! Each file of `tests/golden/expansion/` is one vector that the capture tool `tools/parity/vectors.mjs`
//! made from the pinned Grafana and the ClickHouse plugin. A vector holds the template, the variable
//! definitions, the selection, the range, `max_data_points` and `min_interval_ms`. It holds the text
//! `executedQueryString` of Grafana as `expected`, and `interval_ms` of the request.
//!
//! The test runs the expander of Hune on each vector. It needs no Grafana. Two texts are equal when
//! they are equal after the normal form of 15.14a: each run of blanks outside literals and comments
//! becomes one space, and the blanks at both ends go.
//!
//! A vector without the member `difference` MUST give the same text as Grafana. A vector with the member
//! `difference` records a rule of the specification that differs from Grafana on purpose. It names the
//! cause (`escape`, `interval`, `format` or `other`, 15.14a), the section of the specification, the reason,
//! and the exact result of Hune. The test checks that Hune gives this result, and that the result
//! differs from Grafana. Thus an old difference cannot stay in a vector after a fix.

use std::fs;
use std::path::PathBuf;

use hune_core::sql::{
    ExpandError, ExpandRequest, RequestValues, SqlInvalidKind, TextValueType, TokenKind, ValuesError, VariableDef,
    check_time_range, compute_interval, expand, lex_all, resolve_values,
};
use serde_json::Value;

const CAUSES: [&str; 4] = ["escape", "interval", "format", "other"];

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/expansion")
}

fn vectors() -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir()).expect("tests/golden/expansion exists") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let value: Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        out.push((name, value));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
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

/// The result of Hune for one vector: the text and the interval, or the error code and its part.
#[derive(Debug, PartialEq)]
struct Outcome {
    sql: Result<String, (String, String)>,
    interval_ms: u64,
}

fn run(v: &Value) -> Outcome {
    let defs: Vec<VariableDef> = v["variables"].as_array().unwrap().iter().map(def).collect();
    let request = RequestValues::from_json(&v["values"]).expect("values");
    let get = |k: &str| v[k].as_i64().unwrap_or_else(|| panic!("{k}"));
    let range = check_time_range(get("from_ms"), get("to_ms")).unwrap();
    let min = v["min_interval_ms"].as_u64().unwrap_or(1);
    let interval = compute_interval(&range, get("max_data_points") as u32, min).unwrap();
    let interval_ms = interval.ms;
    let sql = match resolve_values(&defs, &request) {
        Err(ValuesError::Refused(r)) => Err(("variable_value_refused".to_owned(), r.variable)),
        Err(ValuesError::BadRequest(m)) => Err(("bad_request".to_owned(), m)),
        Ok(resolved) => {
            let sql = v["template"].as_str().unwrap();
            match expand(&ExpandRequest {
                sql,
                values: &resolved,
                range,
                interval: Some(interval),
                max_query_size: 262_144,
            }) {
                Ok(s) => Ok(s),
                Err(e) => {
                    let part = match &e {
                        ExpandError::Invalid(i) => match i.kind {
                            SqlInvalidKind::Lexer => "lexer",
                            SqlInvalidKind::Statement => "statement",
                            SqlInvalidKind::Macro => "macro",
                            SqlInvalidKind::Placeholder => "placeholder",
                            SqlInvalidKind::Context => "context",
                            SqlInvalidKind::Parameter => "parameter",
                        }
                        .to_owned(),
                        ExpandError::MissingVariable(n) => n.clone(),
                        ExpandError::ValueRefused(r) => r.variable.clone(),
                        ExpandError::TooLarge { .. } => String::new(),
                    };
                    Err((e.api_code().to_owned(), part))
                }
            }
        }
    };
    Outcome { sql, interval_ms }
}

/// The normal form of 15.14a.
fn normal(sql: &str) -> String {
    let mut out = String::new();
    for t in lex_all(sql) {
        if t.kind == TokenKind::Blank {
            out.push(' ');
        } else {
            out.push_str(t_text(sql, &t));
        }
    }
    let mut collapsed = String::new();
    for c in out.chars() {
        if c == ' ' && collapsed.ends_with(' ') {
            continue;
        }
        collapsed.push(c);
    }
    collapsed.trim().to_owned()
}

fn t_text<'a>(sql: &'a str, t: &hune_core::sql::Token) -> &'a str {
    &sql[t.start..t.end]
}

/// What the vector says about Hune, in the form of an `Outcome` sql part.
fn hune_of(d: &Value) -> Result<String, (String, String)> {
    let v = &d["hune"];
    if let Some(sql) = v.get("sql").and_then(Value::as_str) {
        Ok(sql.to_owned())
    } else {
        let e = &v["error"];
        Err((e["code"].as_str().unwrap().to_owned(), e["part"].as_str().unwrap_or("").to_owned()))
    }
}

#[test]
fn hune_matches_grafana_on_each_vector() {
    let all = vectors();
    let mut faults = Vec::new();
    for (file, v) in &all {
        let got = run(v);
        let expected = &v["expected"];
        let diff = v.get("difference");
        let id = v["id"].as_str().unwrap();
        if file != &format!("{id}.json") {
            faults.push(format!("{file}: the id {id} differs from the file name"));
            continue;
        }
        match diff {
            None => {
                let Some(want) = expected.as_str() else {
                    faults.push(format!("{id}: Grafana gave no SQL and the vector has no difference"));
                    continue;
                };
                match &got.sql {
                    Ok(s) if normal(s) == normal(want) => {}
                    other => faults.push(format!("{id}: Grafana {want:?} / Hune {other:?}")),
                }
                let grafana_interval = v["interval_ms"].as_u64().unwrap();
                if got.interval_ms != grafana_interval {
                    faults.push(format!("{id}: interval Grafana {grafana_interval} / Hune {}", got.interval_ms));
                }
            }
            Some(d) => {
                let cause = d["cause"].as_str().unwrap_or("");
                if !CAUSES.contains(&cause) {
                    faults.push(format!("{id}: cause {cause:?} is not one of {CAUSES:?}"));
                }
                for k in ["section", "reason"] {
                    if d[k].as_str().is_none_or(str::is_empty) {
                        faults.push(format!("{id}: the difference has no {k}"));
                    }
                }
                let want = hune_of(d);
                let normalise =
                    |r: &Result<String, (String, String)>| r.as_ref().map(|s| normal(s)).map_err(Clone::clone);
                if normalise(&got.sql) != normalise(&want) {
                    faults.push(format!("{id}: Hune gives {:?}, the difference records {want:?}", got.sql));
                }
                // The difference MUST be real.
                let grafana = expected.as_str().map(|s| Ok::<String, (String, String)>(normal(s)));
                let interval_differs = got.interval_ms != v["interval_ms"].as_u64().unwrap();
                let text_differs = grafana.as_ref().is_none_or(|g| g != &normalise(&got.sql));
                if !text_differs && !interval_differs {
                    faults.push(format!("{id}: the recorded difference does not exist any more"));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} faults of {} vectors:\n{}", faults.len(), all.len(), faults.join("\n"));
}

#[test]
fn the_vector_files_hold_the_versions_and_the_inputs() {
    let all = vectors();
    assert!(all.len() >= 150, "{} vectors", all.len());
    for (file, v) in &all {
        assert_eq!(v["grafana"], "12.4.2", "{file}");
        assert_eq!(v["plugin"], "grafana-clickhouse-datasource 4.5.1", "{file}");
        for k in ["id", "template", "variables", "values", "from_ms", "to_ms", "max_data_points", "interval_ms"] {
            assert!(v.get(k).is_some(), "{file}: no member {k}");
        }
        assert!(v.get("min_interval_ms").is_some(), "{file}: no member min_interval_ms");
    }
}

#[test]
fn the_interval_of_each_vector_equals_the_interval_of_grafana_unless_a_difference_says_so() {
    for (file, v) in vectors() {
        let range = check_time_range(v["from_ms"].as_i64().unwrap(), v["to_ms"].as_i64().unwrap()).unwrap();
        let min = v["min_interval_ms"].as_u64().unwrap_or(1);
        let got = compute_interval(&range, v["max_data_points"].as_u64().unwrap() as u32, min).unwrap().ms;
        let want = v["interval_ms"].as_u64().unwrap();
        if v.get("difference").is_none() {
            assert_eq!(got, want, "{file}");
        }
    }
}

/// Prints the result of Hune for each vector that differs from Grafana and has no recorded difference.
/// The maintainer uses it to write `tools/parity/vector-differences.json`:
/// `cargo test -p hune-core --test sql_vectors -- --ignored --nocapture`.
#[test]
#[ignore = "helper that prints; it does not check"]
fn print_unrecorded_differences() {
    for (_, v) in vectors() {
        if v.get("difference").is_some() {
            continue;
        }
        let got = run(&v);
        let same = match (&got.sql, v["expected"].as_str()) {
            (Ok(s), Some(want)) => normal(s) == normal(want),
            _ => false,
        };
        if same && got.interval_ms == v["interval_ms"].as_u64().unwrap() {
            continue;
        }
        let hune = match got.sql {
            Ok(sql) => serde_json::json!({ "sql": sql }),
            Err((code, part)) => serde_json::json!({ "error": { "code": code, "part": part } }),
        };
        println!("DIFF {}", serde_json::json!({ "id": v["id"], "hune": hune, "interval_ms": got.interval_ms }));
    }
}
