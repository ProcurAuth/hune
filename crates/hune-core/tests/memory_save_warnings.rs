#![cfg(target_os = "linux")]
//! The memory of the warnings at save (finding D1 of the security review of 2026-11-16). The document `w6_max`
//! gave 1,228,800 warnings 6: a peak of 451 MiB (counting allocator) and an answer of 140 MB. The walk now makes a
//! warning only when the list has room. This file holds one test, because the peak of the process is global.

use hune_core::document::{text_warnings_into, validate};
use hune_core::limits::MAX_WARNINGS;
use hune_core::report::SaveReport;
use hune_core::sql::{Dialect, SqlUse, save_checks_into};
use serde_json::{Value, json};
use std::fs;

/// The peak resident memory of the process in bytes (`VmHWM`), on Linux. The workspace forbids `unsafe`, so the
/// test reads the peak from the system instead of a counting allocator.
fn peak_rss() -> usize {
    let status = fs::read_to_string("/proc/self/status").expect("/proc/self/status");
    let line = status.lines().find(|l| l.starts_with("VmHWM")).expect("VmHWM");
    let kb: usize = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).expect("a number of kB");
    kb * 1024
}

/// The document `w6_max` of the review (2,826,627 bytes): 200 panels, each with a description of 2,048 `$b` and
/// two overrides of 16 data links whose titles have 128 `$b`.
fn w6_max() -> Vec<u8> {
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
fn d1_the_warnings_of_w6_max_add_almost_no_memory() {
    let bytes = w6_max();
    assert_eq!(bytes.len(), 2_826_627);
    let doc = validate(&bytes, None).expect("valid");
    let base = peak_rss();
    let mut report = SaveReport::new();
    for text in doc.document().sql_texts() {
        save_checks_into(Dialect::ClickHouse, &text.text(), SqlUse::Panel, &[], &text.pointer, &mut report);
    }
    text_warnings_into(doc.document(), &mut report);
    let (errors, warnings) = report.into_parts();
    let answer = serde_json::to_vec(&json!({"errors": errors, "warnings": warnings})).unwrap();
    let growth = peak_rss().saturating_sub(base);
    eprintln!("the peak grew by {growth} bytes");
    assert_eq!(warnings.len(), MAX_WARNINGS);
    assert!(warnings.last().unwrap().is_list_stop());
    assert!(answer.len() < 64 * 1024, "{} bytes", answer.len());
    // Less than the input: the old code added 400 MB or more.
    assert!(growth < bytes.len(), "the peak grew by {growth} bytes");
}
