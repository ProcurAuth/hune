#![cfg(target_os = "linux")]
//! The memory of the validation of a long array of SQL lines (finding D2 of the security review of 2026-11-16).
//! Each item had a field with two texts, and each line was copied before the check of the joined size. The joined
//! size now comes first, and an item makes its pointer only for a message. This file holds one test, because the
//! peak of the process is global.

use hune_core::document::validate;
use hune_core::error::ErrorCode;
use std::fs;

/// The peak resident memory of the process in bytes (`VmHWM`), on Linux.
fn peak_rss() -> usize {
    let status = fs::read_to_string("/proc/self/status").expect("/proc/self/status");
    let line = status.lines().find(|l| l.starts_with("VmHWM")).expect("VmHWM");
    let kb: usize = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).expect("a number of kB");
    kb * 1024
}

/// A document with one panel whose query has `n` empty lines, written as text with no JSON value in between.
fn lines_doc(n: usize) -> Vec<u8> {
    let lines = vec!["\"\""; n].join(",");
    format!(
        "{{\"format_version\":3,\"uid\":\"v2poc\",\"title\":\"v2\",\"description\":\"\",\"tags\":[],\
         \"time\":{{\"from\":\"now-1h\",\"to\":\"now\",\"timezone\":\"utc\",\"refresh\":null}},\"cursor_sync\":\"off\",\
         \"links\":[],\"variables\":[],\"sections\":[{{\"title\":null,\"collapsed\":false,\"panels\":[\
         {{\"id\":1,\"type\":\"table\",\"title\":\"t\",\"description\":\"\",\"transparent\":false,\
         \"grid\":{{\"x\":0,\"y\":0,\"w\":24,\"h\":1}},\"data\":{{\"query\":{{\"source\":\"s\",\"format\":\"table\",\
         \"sql\":[{lines}],\"min_interval\":null,\"max_data_points\":null}}}},\"transforms\":[],\
         \"fields\":{{\"defaults\":{{}},\"overrides\":[]}},\"options\":{{\"show_header\":true,\"sort\":[]}},\"links\":[]}}]}}]}}"
    )
    .into_bytes()
}

/// The growth of the peak of the process during the validation, and the one error.
fn measure(bytes: &[u8]) -> (usize, ErrorCode, String) {
    let base = peak_rss();
    let e = validate(bytes, None).unwrap_err();
    let growth = peak_rss().saturating_sub(base);
    eprintln!("the peak grew by {growth} bytes");
    assert_eq!(e.len(), 1, "{e:?}");
    (growth, e[0].code, e[0].pointer.clone())
}

#[test]
fn d2_an_sql_text_of_many_lines_is_too_large_with_no_copy_of_each_line() {
    // `lines_300k` of the review (900,560 bytes): 74 MB of resident memory before the fix. The tree of the JSON
    // text holds one value of 32 bytes for each item; the walk adds nothing for each item.
    let bytes = lines_doc(300_000);
    assert_eq!(bytes.len(), 900_560);
    let (growth, code, pointer) = measure(&bytes);
    assert_eq!((code, pointer.as_str()), (ErrorCode::TooLarge, "/sections/0/panels/0/data/query/sql"));
    assert!(growth < 24 * 1024 * 1024, "the peak grew by {growth} bytes");
    // `lines_one` of the review (4,170,560 bytes): 325 MB of resident memory before the fix.
    let bytes = lines_doc(1_390_000);
    assert_eq!(bytes.len(), 4_170_560);
    let (growth, code, pointer) = measure(&bytes);
    assert_eq!((code, pointer.as_str()), (ErrorCode::TooLarge, "/sections/0/panels/0/data/query/sql"));
    assert!(growth < 96 * 1024 * 1024, "the peak grew by {growth} bytes");
}
