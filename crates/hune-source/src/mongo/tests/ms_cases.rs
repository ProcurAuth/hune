//! Tests of the cases MS02, MS03, MS06 and MS07 of 16.9b that need no server.
//!
//! The script `tests/ci/mongosource.sh` reports a case by the name of a test: a name that starts with
//! `msNN_`. The language and the frame builder are in `hune-core`, and their own tests are there. Each
//! test here runs the same check through the public interface of `hune-core`, on the same golden files
//! of `tests/golden/`, so that the job `mongosource` names the part of the case that needs no server.
//! The tests of MS07 (the wire guard) and MS11 (the probe) are in `guard` and in `probe`, with the same
//! kind of name.

use hune_core::error::ErrorCode;
use hune_core::frame::Kind;
use hune_core::mongo::{
    FrameCaps, MongoExpandError, MongoExpandRequest, MongoFrameBuilder, MongoValue, QueryBody, SaveContext, prepare,
    save_checks,
};
use hune_core::sql::{
    RequestValues, SqlUse, TextValueType, VariableDef, check_time_range, compute_interval, resolve_values,
};
use serde_json::Value;

const FROM_MS: i64 = 1_794_211_200_000;
const TO_MS: i64 = 1_794_232_800_000;

/// The golden files of 7.31 that hold accepted texts (one or more for each name).
const ACCEPTED: [(&str, &str); 5] = [
    ("stages.json", include_str!("../../../../../tests/golden/pipeline-mongo/stages.json")),
    ("query-operators.json", include_str!("../../../../../tests/golden/pipeline-mongo/query-operators.json")),
    ("expression-operators.json", include_str!("../../../../../tests/golden/pipeline-mongo/expression-operators.json")),
    ("accumulators.json", include_str!("../../../../../tests/golden/pipeline-mongo/accumulators.json")),
    ("window-operators.json", include_str!("../../../../../tests/golden/pipeline-mongo/window-operators.json")),
];

/// The golden file of the refused names and shapes (7.32, and the names that no table lists).
const REFUSED: &str = include_str!("../../../../../tests/golden/pipeline-mongo/refused.json");

fn collections() -> Vec<String> {
    ["node_load", "hosts", "events"].iter().map(|s| (*s).to_owned()).collect()
}

fn databases() -> Vec<String> {
    vec!["metrics".to_owned()]
}

/// Runs the texts at save and at run, with no variable. It gives the faults of the save and the result of the run.
fn save_and_run(lines: &[&str]) -> (Vec<(ErrorCode, String)>, Result<(), MongoExpandError>) {
    let (c, d) = (collections(), databases());
    let ctx = SaveContext { variables: &[], collections: &c, databases: &d, usage: SqlUse::Panel };
    let faults = save_checks(lines, &ctx).faults.into_iter().map(|f| (f.code, f.message)).collect();
    let values = resolve_values(&[], &RequestValues::new()).unwrap();
    let range = check_time_range(FROM_MS, TO_MS).unwrap();
    let run = prepare(
        lines,
        &MongoExpandRequest {
            values: &values,
            range,
            interval: Some(compute_interval(&range, 1200, 1).unwrap()),
            max_query_size: 8_388_608,
            collections: &c,
            databases: &d,
        },
    );
    (faults, run.map(|_| ()))
}

fn cases_of(text: &str) -> Vec<(String, Vec<String>, Value)> {
    let doc: Value = serde_json::from_str(text).unwrap();
    doc["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let lines = case["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_owned()).collect();
            (case["id"].as_str().unwrap().to_owned(), lines, case.clone())
        })
        .collect()
}

/// MS02, the part with no server: each name of 7.31 passes the grammar, at save and at run.
#[test]
fn ms02_each_name_of_7_31_passes_the_grammar() {
    let mut total = 0;
    for (file, text) in ACCEPTED {
        for (id, lines, case) in cases_of(text) {
            if case["expect"] != "accepted" {
                continue;
            }
            total += 1;
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            let (faults, run) = save_and_run(&refs);
            assert!(faults.is_empty(), "{file} {id}: refused at save: {faults:?}");
            if let Err(e) = run {
                panic!("{file} {id}: refused at run: {e}");
            }
        }
    }
    assert!(total >= 200, "{total} accepted cases");
}

/// MS02, the part with no server: each other name fails the grammar, at save and at run.
#[test]
fn ms02_each_other_name_fails_the_grammar() {
    let mut total = 0;
    for (id, lines, case) in cases_of(REFUSED) {
        if case["expect"] == "accepted" {
            // A refused name inside a string value gives no refusal (7.32).
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            let (faults, run) = save_and_run(&refs);
            assert!(faults.is_empty() && run.is_ok(), "{id}: {faults:?} {run:?}");
            continue;
        }
        assert_eq!(case["expect"], "refused", "{id}");
        total += 1;
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let (faults, run) = save_and_run(&refs);
        let code = case["code"].as_str().unwrap();
        assert!(faults.iter().any(|(c, _)| c.as_str() == code), "{id}: expected {code} at save, got {faults:?}");
        assert!(matches!(run, Err(MongoExpandError::Invalid(_))), "{id}: expected sql_invalid at run, got {run:?}");
    }
    assert!(total >= 100, "{total} refused cases");
    // A name that no table lists, in a stage and in a filter, is refused at any depth of a pipeline.
    for name in ["$unknownThing", "$Match", "$_x", "$natural"] {
        let stage = format!(r#"{{"collection":"node_load","aggregate":[{{"{name}":{{}}}}]}}"#);
        let filter = format!(r#"{{"collection":"node_load","aggregate":[{{"$match":{{"a":{{"{name}":1}}}}}}]}}"#);
        for text in [stage, filter] {
            let (faults, run) = save_and_run(&[&text]);
            assert!(faults.iter().any(|(c, _)| *c == ErrorCode::NotPermitted), "{text}: {faults:?}");
            assert!(matches!(run, Err(MongoExpandError::Invalid(_))), "{text}");
        }
    }
}

/// MS07, the part with no server: each refused name gives a refusal at save and `sql_invalid` at run, at any depth.
#[test]
fn ms07_each_refused_name_is_refused_at_save_and_at_run() {
    let agg = |stage: &str| format!(r#"{{"collection":"node_load","aggregate":[{stage}]}}"#);
    for name in [
        "$out",
        "$merge",
        "$function",
        "$accumulator",
        "$where",
        "$code",
        "$codeWScope",
        "$currentOp",
        "$listSessions",
        "$listLocalSessions",
        "$listSampledQueries",
        "$queryStats",
        "$planCacheStats",
        "$collStats",
        "$indexStats",
        "$listCatalog",
        "$listSearchIndexes",
        "$changeStream",
        "$graphLookup",
        "$search",
        "$vectorSearch",
        "$documents",
        "$geoNear",
        "$redact",
        "$text",
        "$jsonSchema",
        "$sample",
        "$rand",
        "$meta",
        "$natural",
        "$_internal",
        "$unknownThing",
    ] {
        let texts = [
            agg(&format!(r#"{{"{name}":{{}}}}"#)),
            agg(&format!(r#"{{"$match":{{"a":{{"{name}":1}}}}}}"#)),
            agg(&format!(r#"{{"$group":{{"_id":null,"x":{{"{name}":"$a"}}}}}}"#)),
            agg(&format!(r#"{{"$facet":{{"f":[{{"{name}":{{}}}}]}}}}"#)),
            agg(&format!(r#"{{"$lookup":{{"from":"hosts","pipeline":[{{"{name}":{{}}}}],"as":"x"}}}}"#)),
            agg(&format!(r#"{{"$addFields":{{"x":{{"$literal":{{"{name}":1}}}}}}}}"#)),
            format!(r#"{{"collection":"node_load","find":{{"filter":{{"{name}":1}}}}}}"#),
        ];
        for text in texts {
            let (faults, run) = save_and_run(&[&text]);
            assert!(faults.iter().any(|(c, _)| *c == ErrorCode::NotPermitted), "{text}: {faults:?}");
            assert!(matches!(run, Err(MongoExpandError::Invalid(_))), "{text}: {run:?}");
        }
        // A refused name inside a string value gives no refusal.
        let text = agg(&format!(r#"{{"$match":{{"a":"x{name}","b":{{"$eq":"a{name}"}}}}}}"#));
        let (faults, run) = save_and_run(&[&text]);
        assert!(faults.is_empty() && run.is_ok(), "{text}: {faults:?} {run:?}");
    }
}

/// A seeded generator (splitmix64): the same cases on each platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// The pieces of a hostile value: `$`, `$$`, `.`, quote marks, backslashes, braces, U+2028 and long texts.
fn hostile_value(rng: &mut Rng) -> String {
    const PIECES: [&str; 22] = [
        "$",
        "$$",
        "$$ROOT",
        "$secret",
        ".",
        "a.b",
        "\"",
        "'",
        "\\",
        "\\\"",
        "{",
        "}",
        "{\"$out\":\"x\"}",
        "[\"$secret\"]",
        "\u{2028}",
        "\u{1f600}",
        "${host}",
        "$__timeFrom",
        "text",
        " ",
        "a$b",
        "0",
    ];
    let mut out = String::new();
    for _ in 0..1 + rng.below(6) {
        out.push_str(PIECES[rng.below(PIECES.len())]);
    }
    if rng.below(50) == 0 {
        out.push_str(&"x".repeat(800 + rng.below(100)));
    }
    out
}

/// MS03, the part with no server: a hostile value stays a value, in a whole-leaf and in an embedded placeholder.
#[test]
fn ms03_a_hostile_value_stays_a_value_in_its_leaf() {
    let defs = [VariableDef::text("host", TextValueType::Text)];
    let (c, d) = (collections(), databases());
    let range = check_time_range(FROM_MS, TO_MS).unwrap();
    let mut rng = Rng(3);
    let (mut whole_ok, mut whole_refused, mut embedded_ok) = (0, 0, 0);
    for case in 0..2000 {
        let v = hostile_value(&mut rng);
        let mut req = RequestValues::new();
        req.insert("host", false, &[&v]);
        let values = resolve_values(&defs, &req).unwrap();
        let expand = |leaf: &str| {
            let text = format!(
                r#"{{"collection":"node_load","aggregate":[{{"$match":{{"x":{leaf},"y":{{"$in":[{leaf}]}}}}}}]}}"#
            );
            prepare(
                &[&text],
                &MongoExpandRequest {
                    values: &values,
                    range,
                    interval: Some(compute_interval(&range, 1200, 1).unwrap()),
                    max_query_size: 262_144,
                    collections: &c,
                    databases: &d,
                },
            )
        };
        let shape_is_the_text = |q: &hune_core::mongo::MongoQuery| {
            let QueryBody::Aggregate(stages) = &q.body else { panic!("case {case}: not a pipeline") };
            let m = stages[0].get("$match").unwrap();
            assert_eq!(m.as_document().unwrap().len(), 2, "case {case}");
            assert_eq!(m.get("y").unwrap().as_document().unwrap().len(), 1, "case {case}");
            m.get("x").unwrap().clone()
        };
        // The whole-leaf placeholder gives the value unchanged, and a value that starts with `$` is refused.
        match expand(r#""$host""#) {
            Ok(q) => {
                assert!(!v.starts_with('$'), "case {case}: {v:?} accepted");
                assert_eq!(shape_is_the_text(&q), MongoValue::String(v.clone()), "case {case}");
                whole_ok += 1;
            }
            Err(e) => {
                assert!(v.starts_with('$'), "case {case}: {v:?}: {e:?}");
                assert!(matches!(e, MongoExpandError::ValueRefused(_)), "case {case}: {e:?}");
                whole_refused += 1;
            }
        }
        // The embedded placeholder gives a string with the value inside, whatever the value is.
        let q = expand(r#""p${host}s""#).unwrap_or_else(|e| panic!("case {case}: {v:?}: {e:?}"));
        assert_eq!(shape_is_the_text(&q), MongoValue::String(format!("p{v}s")), "case {case}");
        embedded_ok += 1;
    }
    assert!(whole_ok > 500 && whole_refused > 100 && embedded_ok == 2000, "{whole_ok} {whole_refused} {embedded_ok}");
}

fn build(docs: &[&str]) -> Result<hune_core::frame::Frame, hune_core::mongo::FrameFault> {
    let mut b = MongoFrameBuilder::new(FrameCaps { max_rows: 1000, max_bytes: 1 << 20 });
    for d in docs {
        b.push_document(&MongoValue::document_from_extended_json(d).unwrap())?;
    }
    b.finish("q1".to_owned(), 7)
}

/// MS06, the part with no server: each type of 7.16c gives its kind and its value.
#[test]
fn ms06_each_type_gives_its_kind_and_its_value() {
    let f =
        build(&[r#"{"i":1,"l":{"$numberLong":"9223372036854775807"},"d":1.5,"dec":{"$numberDecimal":"12.500000000"},
               "s":"x","b":true,"t":{"$date":{"$numberLong":"1794211200125"}},
               "o":{"$oid":"507f1f77bcf86cd799439011"},"u":{"$uuid":"123e4567-e89b-12d3-a456-426614174000"},
               "j":{"a":1},"arr":[1,"x"],"n":null}"#])
        .unwrap();
    let cols: Vec<(&str, &str, &str)> =
        f.columns.iter().map(|c| (c.name.as_str(), c.type_text.as_str(), c.kind.as_str())).collect();
    assert_eq!(
        cols,
        [
            ("i", "int", "number"),
            ("l", "long", "number_text"),
            ("d", "double", "number"),
            ("dec", "decimal", "number_text"),
            ("s", "string", "text"),
            ("b", "bool", "bool"),
            ("t", "date", "time"),
            ("o", "objectId", "text"),
            ("u", "uuid", "text"),
            ("j", "object", "text"),
            ("arr", "array", "text"),
            ("n", "null", "null"),
        ]
    );
    let v: Value = serde_json::from_str(&f.to_json()).unwrap();
    assert_eq!(
        v["values"],
        serde_json::json!([
            [1],
            ["9223372036854775807"],
            [1.5],
            ["12.500000000"],
            ["x"],
            [true],
            [1794211200.125],
            ["507f1f77bcf86cd799439011"],
            ["123e4567-e89b-12d3-a456-426614174000"],
            ["{\"a\":1}"],
            ["[1,\"x\"]"],
            [null]
        ])
    );
    // A Date before 1970 keeps its sign and its milliseconds.
    let f = build(&[r#"{"t":{"$date":{"$numberLong":"-1500"}}}"#]).unwrap();
    assert_eq!(f.columns[0].kind, Kind::Time);
    let v: Value = serde_json::from_str(&f.to_json()).unwrap();
    assert_eq!(v["values"], serde_json::json!([[-1.5]]));
    // A missing field is null.
    let f = build(&[r#"{"a":1}"#, r#"{"b":"x"}"#]).unwrap();
    let v: Value = serde_json::from_str(&f.to_json()).unwrap();
    assert_eq!(v["values"], serde_json::json!([[1, null], [null, "x"]]));
}

/// MS06, the part with no server: the mixes, the unsupported types, the empty result and the column limit.
#[test]
fn ms06_the_mixes_the_unsupported_types_and_the_limits() {
    use hune_core::mongo::FrameFault as Fault;
    // Int32 with Double passes. Int32 with String is a query error that holds no value.
    let f = build(&[r#"{"a":1}"#, r#"{"a":1.5}"#]).unwrap();
    assert_eq!(f.columns.len(), 1);
    let Fault::Query(m) = build(&[r#"{"a":1}"#, r#"{"a":"secret-value"}"#]).unwrap_err() else { panic!() };
    assert!(m.contains("\"a\"") && m.contains("int, string") && !m.contains("secret-value"), "{m}");
    // A column of each unsupported type is a query error.
    for form in [
        r#"{"$binary":{"base64":"AQID","subType":"00"}}"#,
        r#"{"$regularExpression":{"pattern":"a","options":""}}"#,
        r#"{"$timestamp":{"t":1,"i":1}}"#,
        r#"{"$code":"x"}"#,
        r#"{"$symbol":"x"}"#,
        r#"{"$minKey":1}"#,
        r#"{"$maxKey":1}"#,
        r#"{"$undefined":true}"#,
    ] {
        let e = build(&[&format!("{{\"a\":{form}}}")]).unwrap_err();
        assert!(matches!(e, Fault::Query(m) if m.contains("\"a\"")), "{form}");
    }
    // A result with no document is a frame with no column.
    let f = MongoFrameBuilder::new(FrameCaps { max_rows: 10, max_bytes: 100 }).finish("q9".to_owned(), 3).unwrap();
    assert!(f.columns.is_empty());
    assert_eq!(f.rows, 0);
    // A document with 257 fields gives the error of the column limit.
    let fields =
        |n: usize| -> Vec<(String, MongoValue)> { (0..n).map(|i| (format!("f{i}"), MongoValue::Int32(1))).collect() };
    let mut b = MongoFrameBuilder::new(FrameCaps { max_rows: 10, max_bytes: 1 << 20 });
    b.push_document(&fields(256)).unwrap();
    let mut b = MongoFrameBuilder::new(FrameCaps { max_rows: 10, max_bytes: 1 << 20 });
    let e = b.push_document(&fields(257)).unwrap_err();
    assert!(matches!(e, Fault::Query(m) if m.contains("256")));
}
