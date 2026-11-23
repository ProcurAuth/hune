// SPDX-License-Identifier: MIT OR Apache-2.0
//! The checks at save are linear and their lists have a cap (4.35, 6.13, 7.18, 7.36). Findings N1 and D1 of the
//! security review of 2026-11-16: the SQL checks found the position of each item from the start of the text, and
//! the lists of faults and warnings had no cap.
//!
//! The timing tests take the minimum of 7 runs after 2 warm-ups, with a generous limit for a debug build. Before
//! the fix, one text of 131,000 unknown placeholders took 17.4 s in a release build.

use hune_core::document::{text_warnings, text_warnings_into, validate};
use hune_core::error::ErrorCode;
use hune_core::limits::{MAX_ERRORS, MAX_WARNINGS};
use hune_core::mongo::SaveContext;
use hune_core::report::SaveReport;
use hune_core::sql::{Dialect, SqlUse, VariableDef, WarningKind, save_checks_for, save_checks_into};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// The minimum of 7 runs after 2 warm-ups (the policy of the timing tests), and the last result.
fn min_of_7<T>(mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut last = f();
    for i in 0..9 {
        let t = Instant::now();
        last = f();
        if i >= 2 {
            best = best.min(t.elapsed());
        }
    }
    (best, last)
}

/// The generous limit of one check of a text of `MAX_SQL_BYTES` in a debug build. A release build takes 3 ms.
const LIMIT: Duration = Duration::from_millis(400);

fn unknown_in_string() -> String {
    format!("SELECT '{}'", "$b".repeat(131_000))
}

#[test]
fn n1_a_text_of_131000_unknown_placeholders_is_checked_in_linear_time_with_a_cap() {
    let text = unknown_in_string();
    let (t, r) = min_of_7(|| save_checks_for(Dialect::ClickHouse, &text, SqlUse::Panel, &[]));
    assert!(t < LIMIT, "the check took {t:?}");
    assert!(r.faults.is_empty() && !r.more_faults);
    assert_eq!(r.warnings.len(), MAX_WARNINGS - 1);
    assert!(r.more_warnings);
    // The kept warnings are the first ones of the text, with their right column.
    for (i, w) in r.warnings.iter().enumerate() {
        assert_eq!((w.kind, w.line_index, w.column), (WarningKind::UnknownVariable, 0, 9 + 2 * i), "{i}");
        assert_eq!(w.message, "$b is not a variable of the document");
    }
    // The same text into a report: the list has `MAX_WARNINGS` items, and the last one says that it stops.
    let mut report = SaveReport::new();
    let pointer = "/sections/0/panels/0/data/query/sql";
    let (t, ()) = min_of_7(|| {
        report = SaveReport::new();
        save_checks_into(Dialect::ClickHouse, &text, SqlUse::Panel, &[], pointer, &mut report);
    });
    assert!(t < LIMIT, "the check took {t:?}");
    assert!(report.errors().is_empty());
    assert_eq!(report.warnings().len(), MAX_WARNINGS);
    assert_eq!(report.warnings()[0].pointer, format!("{pointer}/0"));
    assert_eq!(report.warnings()[0].message, "column 9: $b is not a variable of the document");
    assert!(report.warnings().last().unwrap().is_list_stop());
}

#[test]
fn n1_one_placeholder_on_each_line_has_the_right_line_and_a_cap() {
    let text = vec!["$b"; 87_000].join("\n");
    let (t, r) = min_of_7(|| save_checks_for(Dialect::ClickHouse, &text, SqlUse::Panel, &[]));
    assert!(t < LIMIT, "the check took {t:?}");
    // The text is not a statement: one fault at the start.
    assert_eq!(r.faults.len(), 1);
    assert_eq!(r.warnings.len(), MAX_WARNINGS - 1);
    for (i, w) in r.warnings.iter().enumerate() {
        assert_eq!((w.line_index, w.column), (i, 1));
    }
}

#[test]
fn n1_the_list_keeps_the_items_of_the_lowest_positions_of_all_passes() {
    // The walk makes the warnings 1 and the pass of the words makes the warnings 3 later. The kept items are the
    // first ones in the text, in the order of the text, as before the cap.
    let text = format!("SELECT 1 {}", "$u FINAL ".repeat(400));
    let r = save_checks_for(Dialect::ClickHouse, &text, SqlUse::Panel, &[]);
    assert_eq!(r.warnings.len(), MAX_WARNINGS - 1);
    for (i, w) in r.warnings.iter().enumerate() {
        let (kind, column) = if i % 2 == 0 {
            (WarningKind::UnknownVariable, 10 + 9 * (i / 2))
        } else {
            (WarningKind::FinalOrSettings, 13 + 9 * (i / 2))
        };
        assert_eq!((w.kind, w.column), (kind, column), "{i}");
    }
    // A short text keeps all its items and has no flag.
    let r = save_checks_for(Dialect::ClickHouse, "SELECT $u FROM t FINAL", SqlUse::Panel, &[]);
    assert_eq!(r.warnings.len(), 2);
    assert!(!r.more_warnings && !r.more_faults);
}

#[test]
fn n1_faults_have_a_cap_and_the_last_item_is_too_many_errors() {
    let text = format!("SELECT {}", "$__nope ".repeat(30_000));
    let (t, r) = min_of_7(|| save_checks_for(Dialect::ClickHouse, &text, SqlUse::Panel, &[]));
    assert!(t < LIMIT, "the check took {t:?}");
    assert_eq!(r.faults.len(), MAX_ERRORS - 1);
    assert!(r.more_faults);
    assert_eq!(r.faults[0].column, 8);
    let mut report = SaveReport::new();
    save_checks_into(Dialect::ClickHouse, &text, SqlUse::Panel, &[], "/variables/0/sql", &mut report);
    assert_eq!(report.errors().len(), MAX_ERRORS);
    let last = report.errors().last().unwrap();
    assert_eq!((last.code, last.pointer.as_str()), (ErrorCode::TooManyErrors, ""));
    assert_eq!(report.errors()[0].pointer, "/variables/0/sql/0");
    assert!(report.errors_stopped());
}

#[test]
fn n1_nested_macro_calls_are_checked_in_linear_time() {
    // Each call read its arguments up to its `)`, so nested and open calls were quadratic: 0.6 s to 2 s for one
    // text in a release build.
    let k = 18_000;
    let cases = [
        (Dialect::ClickHouse, format!("SELECT 1 WHERE {}x{}", "$__timeFilter(".repeat(k), ")".repeat(k))),
        (Dialect::ClickHouse, format!("SELECT 1 WHERE {}", "$__timeFilter(".repeat(k))),
        (Dialect::Postgres, format!("SELECT {}x{}", "$__timeGroup(t, ".repeat(12_000), ")".repeat(12_000))),
        (Dialect::Postgres, format!("SELECT {}", "$__timeGroup(t, ".repeat(15_000))),
    ];
    for (dialect, text) in &cases {
        let (t, r) = min_of_7(|| save_checks_for(*dialect, text, SqlUse::Panel, &[]));
        assert!(t < LIMIT, "{dialect:?} {}: the check took {t:?}", &text[..40]);
        assert!(r.faults.len() < MAX_ERRORS);
    }
    // The calls keep their meaning: one good call, one call with a wrong count, one open call.
    let r = save_checks_for(
        Dialect::ClickHouse,
        "SELECT 1 WHERE $__timeFilter(t) AND $__timeFilter(a, b) AND $__timeFilter(",
        SqlUse::Panel,
        &[],
    );
    let messages: Vec<&str> = r.faults.iter().map(|f| f.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "$__timeFilter needs exactly one argument that is not blank",
            "the ( of $__timeFilter has no ) that closes it"
        ]
    );
}

fn mongo_check(text: &str) -> hune_core::mongo::MongoSaveResult {
    let c = vec!["c".to_owned()];
    let ctx = SaveContext { variables: &[], collections: &c, databases: &[], usage: SqlUse::Panel };
    hune_core::mongo::save_checks_text(text, &ctx)
}

#[test]
fn n1_mongodb_warnings_and_shape_faults_have_a_cap() {
    // 15 strings of 4,095 `${x}`: 61,425 warnings 1 of 7.36. Before the fix: 7.2 s in a release build.
    let s = "${x}".repeat(4_095);
    let members: Vec<String> = (0..15).map(|i| format!("\"f{i}\":\"{s}\"")).collect();
    let text = format!("{{\"collection\":\"c\",\"aggregate\":[{{\"$match\":{{{}}}}}]}}", members.join(","));
    let (t, r) = min_of_7(|| mongo_check(&text));
    assert!(t < LIMIT, "the check took {t:?}");
    assert_eq!(r.warnings.len(), MAX_WARNINGS - 1);
    assert!(r.more_warnings);
    let first = &r.warnings[0];
    assert_eq!(first.message, "/aggregate/0/$match/f0: ${x} is not a variable of the document");
    // 30,000 unknown members of the top object: one fault L3 each. Before the fix: 6.0 s in a release build.
    let members: Vec<String> = (0..30_000).map(|i| format!("\"k{i}\":0")).collect();
    let text = format!("{{\"collection\":\"c\",\"aggregate\":[{{\"$limit\":1}}],{}}}", members.join(","));
    let (t, r) = min_of_7(|| mongo_check(&text));
    assert!(t < LIMIT, "the check took {t:?}");
    assert_eq!(r.faults.len(), MAX_ERRORS - 1);
    assert!(r.more_faults);
    assert!(r.faults[0].message.starts_with("/k0: "), "{}", r.faults[0].message);
    // Into a report: the two lists stop with their last item.
    let c = vec!["c".to_owned()];
    let ctx = SaveContext { variables: &[], collections: &c, databases: &[], usage: SqlUse::Panel };
    let mut report = SaveReport::new();
    hune_core::mongo::save_checks_into(&[text.as_str()], &ctx, "/sections/0/panels/0/data/query/sql", &mut report);
    assert_eq!(report.errors().len(), MAX_ERRORS);
    assert_eq!(report.errors().last().unwrap().code, ErrorCode::TooManyErrors);
}

/// The document `w6_max` of the review: 200 panels, each with a description of 2,048 `$b` and two overrides of
/// 16 data links whose titles have 128 `$b`. It gave 1,228,800 warnings 6 (a peak of 451 MiB and an answer of
/// 140 MB).
pub fn w6_max() -> Vec<u8> {
    let link = json!({"title": "$b".repeat(128), "url": "/", "target_blank": false});
    let ov = json!({"match": {"by": "name", "value": "x"}, "set": {"links": vec![link; 16]}});
    let panels: Vec<Value> = (0..200)
        .map(|i| {
            json!({"id": i + 1, "type": "table", "title": "t", "description": "$b".repeat(2048), "transparent": false,
                "grid": {"x": 0, "y": i, "w": 24, "h": 1},
                "data": {"query": {"source": "s", "format": "table", "sql": ["SELECT 1"], "min_interval": null,
                                   "max_data_points": null}},
                "transforms": [], "fields": {"defaults": {}, "overrides": [ov, ov]},
                "options": {"show_header": true, "sort": []}, "links": []})
        })
        .collect();
    let doc = json!({"format_version": 3, "uid": "w6max", "title": "w6", "description": "", "tags": [],
        "time": {"from": "now-1h", "to": "now", "timezone": "utc", "refresh": null}, "cursor_sync": "off",
        "links": [], "variables": [], "sections": [{"title": null, "collapsed": false, "panels": panels}]});
    serde_json::to_vec(&doc).unwrap()
}

#[test]
fn d1_the_warnings_6_of_a_large_document_stop_at_the_cap() {
    let bytes = w6_max();
    let doc = validate(&bytes, None).expect("valid");
    let (t, w) = min_of_7(|| text_warnings(doc.document()));
    assert!(t < Duration::from_millis(50), "the warnings took {t:?}");
    assert_eq!(w.len(), MAX_WARNINGS);
    assert_eq!(w[0].pointer, "/sections/0/panels/0/description");
    assert_eq!(w[0].message, "$b is not a variable of the document");
    assert!(w.last().unwrap().is_list_stop());
    assert_eq!(w.last().unwrap().message, "the list stops here; it has 200 warnings at most");
    // The answer of a write holds the list: it stays small.
    let answer = serde_json::to_vec(&json!({"valid": true, "warnings": w.iter().map(|x| json!({"pointer": x.pointer, "message": x.message})).collect::<Vec<_>>()})).unwrap();
    assert!(answer.len() < 64 * 1024, "{} bytes", answer.len());
}

/// The document `q15_unknown` of the review with warnings 6 too: 15 panels, each with one query of 131,072 `$b`
/// and a title with an unknown name.
fn q15() -> Vec<u8> {
    let panels: Vec<Value> = (0..15)
        .map(|i| {
            json!({"id": i + 1, "type": "table", "title": "t $nope", "description": "", "transparent": false,
                "grid": {"x": 0, "y": i, "w": 24, "h": 1},
                "data": {"query": {"source": "s", "format": "table", "sql": ["$b".repeat(131_072)], "min_interval": null,
                                   "max_data_points": null}},
                "transforms": [], "fields": {"defaults": {}, "overrides": []},
                "options": {"show_header": true, "sort": []}, "links": []})
        })
        .collect();
    let doc = json!({"format_version": 3, "uid": "q15", "title": "q", "description": "", "tags": [],
        "time": {"from": "now-1h", "to": "now", "timezone": "utc", "refresh": null}, "cursor_sync": "off",
        "links": [], "variables": [], "sections": [{"title": null, "collapsed": false, "panels": panels}]});
    serde_json::to_vec(&doc).unwrap()
}

#[test]
fn d1_one_report_for_all_the_texts_of_a_document_keeps_one_cap() {
    let bytes = q15();
    let doc = validate(&bytes, None).expect("valid");
    let defs: Vec<VariableDef> = Vec::new();
    let run = || {
        let mut report = SaveReport::new();
        for text in doc.document().sql_texts() {
            save_checks_into(Dialect::ClickHouse, &text.text(), SqlUse::Panel, &defs, &text.pointer, &mut report);
        }
        text_warnings_into(doc.document(), &mut report);
        report
    };
    let (t, report) = min_of_7(run);
    assert!(t < Duration::from_secs(6), "the checks took {t:?}");
    // Each text is not a statement: 15 faults. The warnings stop at the cap in the first text.
    assert_eq!(report.errors().len(), 15);
    assert_eq!(report.warnings().len(), MAX_WARNINGS);
    assert!(report.warnings()[..MAX_WARNINGS - 1].iter().all(|w| w.pointer == "/sections/0/panels/0/data/query/sql/0"));
    assert!(report.warnings().last().unwrap().is_list_stop());
    let (errors, warnings) = report.into_parts();
    let answer = serde_json::to_vec(&json!({"errors": errors, "warnings": warnings})).unwrap();
    assert!(answer.len() < 64 * 1024, "{} bytes", answer.len());
}

#[test]
fn d1_a_long_name_shows_200_characters_in_a_warning() {
    let name = "b".repeat(5000);
    let text = format!("SELECT '${name}'");
    let r = save_checks_for(Dialect::ClickHouse, &text, SqlUse::Panel, &[]);
    assert_eq!(r.warnings.len(), 1);
    let shown = r.warnings[0].message.strip_suffix(" is not a variable of the document").unwrap();
    assert_eq!(shown.chars().count(), 200);
    assert!(shown.starts_with("$bbb") && shown.ends_with("..."));
}
