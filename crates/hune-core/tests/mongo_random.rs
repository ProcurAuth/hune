// SPDX-License-Identifier: MIT OR Apache-2.0
//! Seeded random tests of the expansion for the kind `mongodb` (16.4, property R10).
//!
//! The generator makes query trees from the stages and the operators of 7.31. It puts whole-leaf and
//! embedded placeholders and macros at value places, and placeholders in keys and in name places. The
//! values of a request are hostile: `$`, `$$ROOT`, `$secret`, `.`, a JSON text with `$out`, quote marks,
//! U+2028 and long texts. A seeded generator (splitmix64) gives the same cases on each platform.
//!
//! Run one seed again: `HUNE_TEST_SEED=4242 cargo test -p hune-core --locked random_`. The variables
//! `HUNE_TEST_SEEDS` (the number of seeds, from 1) and `HUNE_TEST_CASES` (the cases of each seed) set a
//! longer run for a nightly job. The default is the seeds 1 to 64 with 5,000 cases each.
//!
//! R10 has three parts. (a) The expansion gives an error, or a tree T1 with the keys, the stage names and
//! the operator names of T0, and the same structure. (b) Each leaf that holds a free value does not start
//! with `$`. (c) A placeholder in a key or in a name place always gives an error. Also: the same inputs give
//! the same bytes, and the parser never panics.

mod mongo_common;

use hune_core::error::ErrorCode;
use hune_core::mongo::grammar::Class;
use hune_core::mongo::{DefNames, MongoExpandError, MongoValue, classify, parse_text, to_query_json};
use hune_core::sql::{RequestValues, SqlUse, VariableDef};
use mongo_common::*;

struct Rng(u64, bool);

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

    fn percent(&mut self, p: usize) -> bool {
        self.below(100) < p
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const HOSTILE: &[&str] = &[
    "$",
    "$$ROOT",
    "$secret",
    "$a.b",
    ".",
    r#"{"$out": "x"}"#,
    r#"["$secret", {"$where": "1"}]"#,
    "\"",
    "'",
    "\\",
    "\\\"",
    "{",
    "}",
    "\u{2028}",
    "\u{1f600}",
    "a$b",
    "$__timeFrom",
    "${host}",
    "$host",
    "text",
    "a.b",
    "0123456789abcdef01234567",
    "2026-11-09T14:00:00Z",
    "true",
    "12",
    "-3.5",
    "3000000000",
    "12abc",
    "",
    " ",
    "^$",
    "a|b",
];

fn hostile(rng: &mut Rng) -> String {
    match rng.below(12) {
        0 => "x".repeat(1 + rng.below(1024)),
        1 => format!("{}{}", rng.pick(&["$", "$$", "x", "a."]), "y".repeat(rng.below(40))),
        _ => (*rng.pick(HOSTILE)).to_owned(),
    }
}

fn make_request(rng: &mut Rng) -> RequestValues {
    let mut r = RequestValues::new();
    if rng.percent(92) {
        let v = hostile(rng);
        r.insert("host", false, &[v.as_str()]);
    }
    if rng.percent(92) {
        let v = if rng.percent(60) {
            (*rng.pick(&["12", "-3", "1.5", "3000000000", "0", "1", "-1"])).to_owned()
        } else {
            hostile(rng)
        };
        r.insert("n", false, &[v.as_str()]);
    }
    if rng.percent(92) {
        let all = rng.percent(15);
        let items: Vec<String> = if all { Vec::new() } else { (0..rng.below(5)).map(|_| hostile(rng)).collect() };
        let refs: Vec<&str> = items.iter().map(String::as_str).collect();
        // With `all` and no `all_value`, the values are the options that the page shows.
        if all {
            let shown: Vec<String> = (0..rng.below(4)).map(|_| hostile(rng)).collect();
            let refs: Vec<&str> = shown.iter().map(String::as_str).collect();
            r.insert("hosts", true, &refs);
        } else {
            r.insert("hosts", false, &refs);
        }
    }
    if rng.percent(92) {
        if rng.percent(75) {
            let v = hostile(rng);
            r.insert("one", false, &[v.as_str()]);
        } else {
            r.insert("one", false, &[]);
        }
    }
    if rng.percent(92) {
        r.insert("env", false, &[rng.pick(&["prod", "$dev", "a.b"])]);
    }
    if rng.percent(92) {
        if rng.percent(30) {
            r.insert("envs", true, &[]);
        } else {
            let mut items = Vec::new();
            for o in ["prod", "dev"] {
                if rng.percent(50) {
                    items.push(o);
                }
            }
            r.insert("envs", false, &items);
        }
    }
    r
}

// ---- the generator of templates ----

/// A whole-leaf or an embedded form, a macro or a plain string, as the JSON text of one string.
fn leaf(rng: &mut Rng) -> String {
    const CALM_WHOLE: &[&str] = &[
        "$host",
        "${host}",
        "${host:text}",
        "${n:number}",
        "${hosts:json}",
        "${hosts:json_number}",
        "${one}",
        "${env}",
        "${envs:json}",
        "${host:json}",
        "$__timeFrom",
        "$__timeTo",
        "$__from",
        "$__to",
        "$__interval_ms",
        "$__interval_s",
    ];
    const CALM_EMBEDDED: &[&str] = &[
        "x${host}y",
        "^${host:regex}$",
        "a${hosts:csv}",
        "p-$one-s",
        "x${hosts:regex}",
        "x${host:json}",
        "x${env:raw}",
        "q${n}",
        "x${host:html}",
        "x${envs:csv}",
        "x ${nosuch} y",
        "x $__timeFrom y",
    ];
    if rng.1 {
        return match rng.below(10) {
            0..=5 => format!("\"{}\"", rng.pick(CALM_WHOLE)),
            6..=7 => format!("\"{}\"", rng.pick(CALM_EMBEDDED)),
            8 => rng.pick(&["1", "2.5", "true", "null", "-4"]).to_string(),
            _ => r#"{"$date":"2026-11-09T14:00:00Z"}"#.to_owned(),
        };
    }
    const WHOLE: &[&str] = &[
        "$host",
        "${host}",
        "${host:text}",
        "${n:number}",
        "${n:date}",
        "${n:oid}",
        "${n:bool}",
        "${hosts:json}",
        "${hosts:json_number}",
        "${hosts:json_oid}",
        "${one}",
        "${env}",
        "${envs:json}",
        "${one:number}",
        "${host:json}",
        "$hosts",
        "${hosts}",
        "${host:csv}",
        "$__timeFrom",
        "$__timeTo",
        "$__from",
        "$__to",
        "$__interval_ms",
        "$__interval_s",
    ];
    const EMBEDDED: &[&str] = &[
        "x${host}y",
        "^${host:regex}$",
        "a${hosts:csv}",
        "p-$one-s",
        "${hosts:json}x",
        "x${hosts:regex}",
        "x${host:json}",
        "x${env:raw}",
        "q${n}",
        "x${hosts}",
        "x${host:html}",
        "x${host:pipe}",
        "x${envs:csv}",
        "x ${nosuch} y",
        "x $__timeFrom y",
    ];
    const PLAIN: &[&str] = &["abc", "$field", "$a.b", "$$ROOT", "$$this", "x y", "$cpu", "", "1"];
    match rng.below(10) {
        0..=4 => format!("\"{}\"", rng.pick(WHOLE)),
        5..=6 => format!("\"{}\"", rng.pick(EMBEDDED)),
        7 => format!("\"{}\"", rng.pick(PLAIN)),
        8 => rng.pick(&["1", "2.5", "true", "null", "-4"]).to_string(),
        _ => r#"{"$date":"2026-11-09T14:00:00Z"}"#.to_owned(),
    }
}

fn expr(rng: &mut Rng, depth: usize) -> String {
    if depth == 0 || rng.percent(35) {
        return leaf(rng);
    }
    let sub = |rng: &mut Rng| expr(rng, depth - 1);
    match rng.below(9) {
        0 => format!(r#"{{"$concat":[{},{}]}}"#, sub(rng), sub(rng)),
        1 => format!(r#"{{"$add":[{},{}]}}"#, sub(rng), sub(rng)),
        2 => format!(r#"{{"$cond":[{},{},{}]}}"#, sub(rng), sub(rng), sub(rng)),
        3 => format!(r#"{{"$in":[{},{}]}}"#, sub(rng), leaf(rng)),
        4 => format!(r#"{{"$dateTrunc":{{"date":"$ts","unit":"second","binSize":{}}}}}"#, sub(rng)),
        5 => format!(r#"{{"$regexMatch":{{"input":{},"regex":{}}}}}"#, sub(rng), leaf(rng)),
        6 => format!(r#"{{"k":{},"j":[{}]}}"#, sub(rng), sub(rng)),
        7 => format!(r#"{{"$literal":{}}}"#, leaf(rng)),
        _ => format!(r#"{{"$ifNull":[{},{}]}}"#, sub(rng), sub(rng)),
    }
}

fn filter(rng: &mut Rng) -> String {
    let op = *rng.pick(&["$eq", "$ne", "$gt", "$gte", "$lt", "$lte"]);
    match rng.below(7) {
        0 => format!(r#"{{"a":{}}}"#, leaf(rng)),
        1 => format!(r#"{{"ts":{{"$gte":{},"$lte":{}}}}}"#, leaf(rng), leaf(rng)),
        2 if rng.1 => r#"{"h":{"$in":"${hosts:json}"}}"#.to_owned(),
        2 => format!(r#"{{"h":{{"$in":{}}}}}"#, leaf(rng)),
        3 => format!(r#"{{"h":{{"$nin":[{},{}]}}}}"#, leaf(rng), leaf(rng)),
        4 if rng.1 => r#"{"s":{"$regex":"x${host:regex}","$options":"i"}}"#.to_owned(),
        4 => format!(r#"{{"s":{{"$regex":{},"$options":{}}}}}"#, leaf(rng), leaf(rng)),
        5 => format!(r#"{{"$or":[{{"a":{{"{op}":{}}}}},{{"b":{}}}]}}"#, leaf(rng), leaf(rng)),
        _ => format!(r#"{{"$expr":{{"{op}":[{},{}]}}}}"#, expr(rng, 1), expr(rng, 1)),
    }
}

fn stage(rng: &mut Rng, nested: bool) -> String {
    match rng.below(if nested { 10 } else { 13 }) {
        0..=2 => format!(r#"{{"$match":{}}}"#, filter(rng)),
        3 => format!(r#"{{"$addFields":{{"x":{},"y":{}}}}}"#, expr(rng, 2), expr(rng, 2)),
        4 => format!(r#"{{"$project":{{"_id":0,"x":{},"y":1}}}}"#, expr(rng, 2)),
        5 => format!(
            r#"{{"$group":{{"_id":{},"s":{{"$sum":{}}},"f":{{"$first":{}}}}}}}"#,
            expr(rng, 1),
            expr(rng, 1),
            leaf(rng)
        ),
        6 if rng.1 => format!(r#"{{"$limit":{}}}"#, rng.pick(&["5", "\"${n:number}\"", "\"$__interval_s\""])),
        6 => format!(r#"{{"$limit":{}}}"#, leaf(rng)),
        7 => format!(r#"{{"$sort":{{"a":{}}}}}"#, rng.pick(&["1", "-1", "\"${n:number}\"", "2"])),
        8 => format!(r#"{{"$replaceWith":{}}}"#, expr(rng, 2)),
        9 if rng.1 => format!(r#"{{"$skip":{}}}"#, rng.pick(&["5", "\"${n:number}\""])),
        9 => format!(r#"{{"$skip":{}}}"#, leaf(rng)),
        10 => format!(r#"{{"$facet":{{"f":[{}]}}}}"#, stage(rng, true)),
        11 => format!(
            r#"{{"$lookup":{{"from":"hosts","let":{{"v":{}}},"pipeline":[{}],"as":"j"}}}}"#,
            leaf(rng),
            stage(rng, true)
        ),
        _ => format!(r#"{{"$unionWith":{{"coll":"events","pipeline":[{}]}}}}"#, stage(rng, true)),
    }
}

fn template(rng: &mut Rng) -> String {
    let n = 1 + rng.below(4);
    let stages: Vec<String> = (0..n).map(|_| stage(rng, false)).collect();
    if rng.percent(12) {
        let f = if rng.percent(50) { filter(rng) } else { "{}".to_owned() };
        return format!(r#"{{"collection":"node_load","find":{{"filter":{f},"limit":5}}}}"#);
    }
    format!(r#"{{"collection":"node_load","aggregate":[{}]}}"#, stages.join(","))
}

/// A template with one placeholder in a key or in a name place.
fn bad_template(rng: &mut Rng) -> String {
    let ph = *rng.pick(&["${host}", "$host", "a_${host}", "${hosts:json}", "${n:number}", "${one}", "$__timeFrom"]);
    let (pre, post) = (stage(rng, false), stage(rng, false));
    let mid = match rng.below(20) {
        0 => format!(r#"{{"$match":{{"{ph}":1}}}}"#),
        1 => format!(r#"{{"$match":{{"a":{{"{ph}":1}}}}}}"#),
        2 => format!(r#"{{"$addFields":{{"{ph}":1}}}}"#),
        3 => format!(r#"{{"$addFields":{{"x":{{"{ph}":1}}}}}}"#),
        4 => format!(r#"{{"$group":{{"_id":null,"{ph}":{{"$sum":1}}}}}}"#),
        5 => format!(r#"{{"$unwind":"{ph}"}}"#),
        6 => format!(r#"{{"$unwind":{{"path":"$a","includeArrayIndex":"{ph}"}}}}"#),
        7 => format!(r#"{{"$count":"{ph}"}}"#),
        8 => format!(r#"{{"$unset":["a","{ph}"]}}"#),
        9 => format!(r#"{{"$replaceRoot":{{"newRoot":"{ph}"}}}}"#),
        10 => format!(r#"{{"$lookup":{{"from":"{ph}","localField":"a","foreignField":"b","as":"c"}}}}"#),
        11 => format!(r#"{{"$lookup":{{"from":"hosts","localField":"{ph}","foreignField":"b","as":"c"}}}}"#),
        12 => format!(r#"{{"$lookup":{{"from":"hosts","localField":"a","foreignField":"{ph}","as":"c"}}}}"#),
        13 => format!(r#"{{"$lookup":{{"from":"hosts","localField":"a","foreignField":"b","as":"{ph}"}}}}"#),
        14 => format!(r#"{{"$unionWith":"{ph}"}}"#),
        15 => format!(r#"{{"$addFields":{{"x":{{"$getField":"{ph}"}}}}}}"#),
        16 => format!(r#"{{"$addFields":{{"x":{{"$setField":{{"field":"{ph}","input":"$$ROOT","value":1}}}}}}}}"#),
        17 => format!(r#"{{"$sort":{{"{ph}":1}}}}"#),
        18 => format!(r#"{{"{ph}":{{}}}}"#),
        _ => format!(r#"{{"$project":{{"x":{{"$map":{{"input":"$a","as":"{ph}","in":1}}}}}}}}"#),
    };
    let body = format!(r#"{{"collection":"node_load","aggregate":[{pre},{mid},{post}]}}"#);
    if rng.percent(10) {
        return format!(r#"{{"collection":"{ph}","find":{{}}}}"#);
    }
    body
}

// ---- the checks ----

/// The variables that hold free text when each variable is closed, and when each `custom` variable is open (F41).
const FREE_CLOSED: &[&str] = &["host", "n", "hosts", "one"];
const FREE_OPEN: &[&str] = &["host", "n", "hosts", "one", "env", "envs"];

/// The variables of the tests with the member `allow_custom_value` set on the `query` and the `custom` variables.
fn defs_open(open_query: bool, open_custom: bool) -> Vec<VariableDef> {
    defs()
        .into_iter()
        .map(|d| match d.name.as_str() {
            "hosts" | "one" => d.with_open(open_query),
            "env" | "envs" => d.with_open(open_custom),
            _ => d,
        })
        .collect()
}

#[derive(Default)]
struct Stats {
    cases: usize,
    ok: usize,
    refused: usize,
    structure: usize,
    invalid: usize,
    bad_request: usize,
    skipped: usize,
    open_ok: usize,
}

/// R10 (a): T1 has the keys and the structure of T0. A leaf of T0 that is a string can become any
/// typed leaf. Also R10 (b): a leaf that holds a free value does not start with `$`.
fn compare(t0: &MongoValue, t1: &MongoValue, path: &str, defs: &DefNames<'_>, free: &[&str], seed: u64, case: usize) {
    let fail = |m: &str| panic!("seed {seed} case {case}: {m} at {path}");
    match (t0, t1) {
        (MongoValue::Document(a), MongoValue::Document(b)) => {
            if a.len() != b.len() {
                fail("the count of members changed");
            }
            for ((k1, v1), (k2, v2)) in a.iter().zip(b) {
                if k1 != k2 {
                    fail("a key changed");
                }
                compare(v1, v2, &format!("{path}/{k1}"), defs, free, seed, case);
            }
        }
        (MongoValue::Array(a), MongoValue::Array(b)) => {
            if a.len() != b.len() {
                fail("the length of an array changed");
            }
            for (i, (v1, v2)) in a.iter().zip(b).enumerate() {
                compare(v1, v2, &format!("{path}/{i}"), defs, free, seed, case);
            }
        }
        (MongoValue::String(s), after) => {
            if matches!(after, MongoValue::Document(_)) {
                fail("a string became an object");
            }
            let mut warns = Vec::new();
            let class = classify(s, defs, true, &mut warns);
            let free_name = |name: &str| free.contains(&name);
            let no_dollar = |v: &MongoValue| match v {
                MongoValue::String(x) => !x.starts_with('$'),
                _ => true,
            };
            match class {
                Class::Whole { name, .. } if free_name(name) => match after {
                    MongoValue::Array(items) => {
                        if !items.iter().all(no_dollar) {
                            fail("a free value in a list starts with $");
                        }
                    }
                    other => {
                        if !no_dollar(other) {
                            fail("a free value starts with $");
                        }
                    }
                },
                Class::Embedded(parts) => {
                    if parts
                        .iter()
                        .any(|p| matches!(p, hune_core::mongo::grammar::Part::Var { name, .. } if free_name(name)))
                        && !no_dollar(after)
                    {
                        fail("a text with a free value starts with $");
                    }
                }
                Class::Text | Class::Path | Class::VarRef if after != t0 => {
                    fail("a text that holds no placeholder changed");
                }
                _ => {}
            }
        }
        (a, b) => {
            if a != b {
                fail("a leaf that is not a string changed");
            }
        }
    }
}

fn run_case(rng: &mut Rng, seed: u64, case: usize, stats: &mut Stats) {
    stats.cases += 1;
    let defs_v = defs();
    let names = DefNames(&defs_v);
    let req = make_request(rng);
    if rng.percent(20) {
        // R10 (c): a placeholder in a key or in a name place always gives an error.
        let text = bad_template(rng);
        let r = save(&text);
        assert!(!r.faults.is_empty(), "seed {seed} case {case}: accepted at save: {text}");
        match run(&text, &req) {
            Err(MongoExpandError::Invalid(_)) | Err(MongoExpandError::MissingVariable(_)) => {}
            other => panic!("seed {seed} case {case}: {text}: {other:?}"),
        }
        return;
    }
    let text = template(rng);
    // The parser never panics and gives the same tree for the same text.
    let t0 = match parse_text(&text) {
        Ok(p) => p.value,
        Err(e) => panic!("seed {seed} case {case}: the generator made a bad text: {e}: {text}"),
    };
    let first = run(&text, &req);
    if case.is_multiple_of(4) {
        let second = run(&text, &req);
        assert_eq!(first, second, "seed {seed} case {case}: not deterministic: {text}");
    }
    let outcome = first.clone();
    // F41, 16.4: the member of a `query` variable changes nothing, at run and at save.
    for open in [false, true] {
        let defs_q = defs_open(open, false);
        assert_eq!(
            run_with(&text, &defs_q, &req, false),
            outcome,
            "seed {seed} case {case}: query flag {open}: {text}"
        );
        assert_eq!(
            save_with(&text, &defs_q, SqlUse::Panel),
            save(&text),
            "seed {seed} case {case}: query flag {open} at save: {text}"
        );
    }
    // F41, 16.4: with each `custom` variable open, the same values are free text. PM holds for them too.
    let defs_c = defs_open(false, true);
    let open_run = run_with(&text, &defs_c, &req, false);
    if let Ok(q) = &open_run {
        stats.open_ok += 1;
        let t1 = q.to_value();
        if let (MongoValue::Document(top0), Some(top1)) = (&t0, t1.as_document()) {
            for (k, v0) in top0 {
                if let Some(v1) = top1.iter().find(|(k1, _)| k1 == k).map(|(_, v)| v)
                    && k != "find"
                {
                    compare(v0, v1, &format!("/{k}"), &DefNames(&defs_c), FREE_OPEN, seed, case);
                }
            }
        }
    }
    match first {
        Ok(q) => {
            stats.ok += 1;
            let t1 = q.to_value();
            // The stored top object and the expanded one.
            let top0 = match &t0 {
                MongoValue::Document(d) => d.clone(),
                _ => unreachable!(),
            };
            // `to_value` omits an empty `filter` and a zero `skip`: compare member by member.
            let top1 = t1.as_document().unwrap();
            for (k, v0) in &top0 {
                let v1 = top1.iter().find(|(k1, _)| k1 == k).map(|(_, v)| v);
                match (k.as_str(), v1) {
                    ("find", Some(f1)) => {
                        let f0 = v0.as_document().unwrap();
                        for (fk, fv0) in f0 {
                            if let Some(fv1) = f1.get(fk) {
                                compare(fv0, fv1, &format!("/find/{fk}"), &names, FREE_CLOSED, seed, case);
                            }
                        }
                    }
                    (_, Some(v1)) => compare(v0, v1, &format!("/{k}"), &names, FREE_CLOSED, seed, case),
                    (_, None) => panic!("seed {seed} case {case}: a member is lost: {k}"),
                }
            }
            // The output text parses again with the profile J to the same tree.
            let out = to_query_json(&t1);
            let again = parse_text(&out).unwrap_or_else(|e| panic!("seed {seed} case {case}: {e}: {out}")).value;
            assert_eq!(again, t1, "seed {seed} case {case}");
            // The saved check accepts the stored text when the run accepts it and the save has no list fault.
        }
        Err(MongoExpandError::ValueRefused(v)) => {
            stats.refused += 1;
            if v.reason == "The value changes the structure of the query." {
                stats.structure += 1;
            }
        }
        Err(MongoExpandError::Invalid(_)) => stats.invalid += 1,
        Err(MongoExpandError::MissingVariable(_)) => stats.bad_request += 1,
        Err(MongoExpandError::TooLarge { .. }) => stats.skipped += 1,
    }
    // A text that the save accepts has no fault of the document at run (only a fault of a value or a list).
    let saved = save(&text);
    if saved.faults.is_empty() {
        if let Err(MongoExpandError::Invalid(i)) = outcome {
            // A list with no format depends on the selection, so the run can refuse what the save accepts.
            // The guard of step 6 must never fire for a text that the save accepts.
            assert_ne!(i.kind, hune_core::mongo::InvalidKind::Guard, "seed {seed} case {case}: {text}: {i}");
            assert!(
                matches!(i.kind, hune_core::mongo::InvalidKind::Placeholder),
                "seed {seed} case {case}: {text}: {i}"
            );
        }
    } else {
        assert!(
            saved.faults.iter().all(|f| matches!(
                f.code,
                ErrorCode::BadFormat
                    | ErrorCode::NotPermitted
                    | ErrorCode::BadReference
                    | ErrorCode::TooLarge
                    | ErrorCode::TooDeep
            )),
            "seed {seed} case {case}"
        );
        assert!(
            outcome.is_err() || saved.faults.iter().all(|f| f.code == ErrorCode::BadFormat),
            "seed {seed} case {case}: {text}"
        );
    }
}

fn env_number(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

#[test]
fn random_r10_seeds() {
    let cases = env_number("HUNE_TEST_CASES").unwrap_or(5000) as usize;
    let seeds: Vec<u64> = match env_number("HUNE_TEST_SEED") {
        Some(s) => vec![s],
        None => (1..=env_number("HUNE_TEST_SEEDS").unwrap_or(64)).collect(),
    };
    let mut stats = Stats::default();
    for seed in seeds {
        let mut rng = Rng(seed, false);
        for case in 0..cases {
            // Half of the cases use only the forms that fit their place: the others find the faults.
            rng.1 = case.is_multiple_of(2);
            run_case(&mut rng, seed, case, &mut stats);
        }
    }
    eprintln!(
        "random R10: {} cases, {} expanded, {} refused ({} by PM), {} invalid, {} bad_request, {} too large, {} with open custom variables",
        stats.cases,
        stats.ok,
        stats.refused,
        stats.structure,
        stats.invalid,
        stats.bad_request,
        stats.skipped,
        stats.open_ok
    );
    assert!(stats.open_ok * 100 >= stats.cases * 10, "too few expansions with open custom variables");
    assert!(stats.ok * 100 >= stats.cases * 15, "too few expansions: {} of {}", stats.ok, stats.cases);
    if env_number("HUNE_TEST_SEED").is_none() {
        assert!(stats.structure > 0, "PM never fired: the test does not reach it");
    }
    assert!(stats.refused * 100 >= stats.cases * 3, "too few refusals: {} of {}", stats.refused, stats.cases);
}

#[test]
fn random_parser_and_checks_never_panic_on_hostile_text() {
    let mut rng = Rng(777, false);
    let pieces: &[&str] = &[
        "{",
        "}",
        "[",
        "]",
        ",",
        ":",
        "\"collection\"",
        "\"hosts\"",
        "\"aggregate\"",
        "\"find\"",
        "\"$match\"",
        "\"$out\"",
        "\"$limit\"",
        "1",
        "-1",
        "null",
        "\"${host}\"",
        "\"$host\"",
        "\"$$ROOT\"",
        "\"$__timeFrom\"",
        "\"$\"",
        "\"\"",
        "{\"$date\":\"x\"}",
        "\"a.b\"",
    ];
    for _ in 0..30_000 {
        let n = rng.below(30);
        let s: String = (0..n).map(|_| *rng.pick(pieces)).collect();
        let _ = save(&s);
        let _ = run(&s, &request(&[("host", false, &["x"])]));
    }
}
