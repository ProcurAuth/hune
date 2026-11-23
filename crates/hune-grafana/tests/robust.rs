// SPDX-License-Identifier: MIT OR Apache-2.0
//! The importer never panics on input JSON: each value of a fixture is replaced by other values
//! of another type, and the import runs again.

mod common;

use common::*;
use hune_grafana::{import_all, import_dashboard};
use serde_json::{Value, json};

/// The JSON pointers of all values below `v`.
fn pointers(v: &Value, at: &str, out: &mut Vec<String>) {
    out.push(at.to_owned());
    match v {
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                pointers(x, &format!("{at}/{i}"), out);
            }
        }
        Value::Object(o) => {
            for (k, x) in o {
                pointers(x, &format!("{at}/{}", k.replace('~', "~0").replace('/', "~1")), out);
            }
        }
        _ => {}
    }
}

fn junk(original: &Value) -> Vec<Value> {
    let mut v = vec![
        json!(null),
        json!(-1),
        json!(1.5e300),
        json!("zz"),
        json!(true),
        json!([]),
        json!({}),
        json!([null, 1, "x"]),
        json!({"a": [1]}),
    ];
    // The same type with an odd content.
    match original {
        Value::String(_) => v.extend([json!(""), json!("${"), json!("$__timeFilter("), json!("/(/"), json!("\u{0}")]),
        Value::Number(_) => v.extend([json!(0), json!(99999999999_i64), json!(0.5)]),
        _ => {}
    }
    v
}

fn run_mutations(file: &str) {
    let base = read_json(&export_dir().join("dashboards").join(file));
    let mut ps = Vec::new();
    pointers(&base, "", &mut ps);
    let opts =
        options(&std::fs::read_to_string(repo_root().join("tests/fixtures/grafana/accept.txt")).expect("accept"));
    let mut runs = 0;
    // A fixed sample: each second value, with two of the replacements. The run stays short.
    for (n, p) in ps.iter().enumerate().filter(|(n, _)| n % 2 == 0) {
        if p.is_empty() {
            continue;
        }
        let original = base.pointer(p).cloned().unwrap_or(Value::Null);
        let candidates = junk(&original);
        for j in [candidates[n % candidates.len()].clone(), candidates[(n * 7 + 3) % candidates.len()].clone()] {
            let mut mutated = base.clone();
            if let Some(slot) = mutated.pointer_mut(p) {
                *slot = j;
            }
            let _ = import_dashboard(&mutated, &opts);
            runs += 1;
        }
    }
    assert!(runs > 500, "{file}: {runs} runs");
}

#[test]
fn no_panic_on_a_v1_raw_dashboard() {
    run_mutations("monitor.json");
}

#[test]
fn no_panic_on_a_v1_wrapped_dashboard() {
    run_mutations("monitor-wrapped.json");
}

#[test]
fn no_panic_on_a_v2alpha1_dashboard() {
    run_mutations("monitor-v2a.json");
}

#[test]
fn no_panic_on_a_v2beta1_dashboard() {
    run_mutations("monitor-v2b.json");
}

#[test]
fn no_panic_on_odd_whole_files() {
    let opts = options("");
    for v in [
        json!(null),
        json!(1),
        json!("x"),
        json!([]),
        json!({}),
        json!({"dashboard": 1, "meta": 2}),
        json!({"apiVersion": 3}),
        json!({"schemaVersion": 1, "panels": "x"}),
    ] {
        let r = import_dashboard(&v, &opts);
        assert!(r.document_bytes.is_none());
        let mut input = load_input();
        input.dashboards[0].value = v;
        assert!(import_all(&input, &opts).is_err());
    }
}
