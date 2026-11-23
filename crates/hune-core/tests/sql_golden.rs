//! Golden expansion vectors (16.3, 16.13): synthetic SQL, synthetic values, expected final text.
//!
//! Each case of `tests/golden/sql/*.json` gives the SQL lines, the variables, the request values, the
//! range, `max_data_points`, `min_interval_ms` and either the expected SQL or the expected error.

use hune_core::sql::{
    ExpandError, ExpandRequest, RequestValues, SqlInvalidKind, TextValueType, ValuesError, VariableDef,
    check_time_range, compute_interval, expand, join_lines, resolve_values,
};
use serde_json::Value;

const FILES: [(&str, &str); 5] = [
    ("spec.json", include_str!("../../../tests/golden/sql/spec.json")),
    ("macros.json", include_str!("../../../tests/golden/sql/macros.json")),
    ("variables.json", include_str!("../../../tests/golden/sql/variables.json")),
    ("contexts.json", include_str!("../../../tests/golden/sql/contexts.json")),
    ("formats.json", include_str!("../../../tests/golden/sql/formats.json")),
];

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

/// Runs a case. The result is the expansion, or the error code and the part that the case names.
fn run(case: &Value) -> Result<String, (String, String)> {
    let defs: Vec<VariableDef> = case["variables"].as_array().unwrap().iter().map(def).collect();
    let request = RequestValues::from_json(&case["request"]).expect("request");
    let lines: Vec<&str> = case["sql"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
    let sql = join_lines(&lines);
    let get = |k: &str, d: i64| case.get(k).and_then(Value::as_i64).unwrap_or(d);
    let range = check_time_range(get("from_ms", 1_794_211_200_000), get("to_ms", 1_794_232_800_000)).unwrap();
    let interval = if case.get("usage").and_then(Value::as_str) == Some("variable") {
        None
    } else {
        Some(compute_interval(&range, get("max_data_points", 1200) as u32, get("min_interval_ms", 1) as u64).unwrap())
    };
    let resolved = match resolve_values(&defs, &request) {
        Ok(r) => r,
        Err(ValuesError::Refused(r)) => return Err(("variable_value_refused".into(), r.variable)),
        Err(ValuesError::BadRequest(m)) => return Err(("bad_request".into(), m)),
    };
    let max_query_size = get("max_query_size", 262_144) as usize;
    match expand(&ExpandRequest { sql: &sql, values: &resolved, range, interval, max_query_size }) {
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

#[test]
fn golden_expansion_vectors() {
    let mut total = 0;
    for (file, text) in FILES {
        let cases: Vec<Value> = serde_json::from_str(text).unwrap();
        for case in &cases {
            total += 1;
            let name = format!("{file}: {}", case["name"].as_str().unwrap());
            let got = run(case);
            if let Some(want) = case.get("expected").and_then(Value::as_str) {
                assert_eq!(got.as_deref(), Ok(want), "{name}");
            } else {
                let err = &case["error"];
                let code = err["code"].as_str().unwrap();
                let part = err.get("kind").or_else(|| err.get("variable")).and_then(Value::as_str).unwrap_or("");
                assert_eq!(got, Err((code.to_owned(), part.to_owned())), "{name}");
            }
        }
    }
    assert!(total >= 250, "{total} cases");
}

#[test]
fn every_case_has_a_name_and_one_answer() {
    for (file, text) in FILES {
        let cases: Vec<Value> = serde_json::from_str(text).unwrap();
        for c in cases {
            assert!(c["name"].is_string(), "{file}");
            assert!(c.get("expected").is_some() != c.get("error").is_some(), "{file}: {}", c["name"]);
        }
    }
}
