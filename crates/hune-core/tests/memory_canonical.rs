#![cfg(target_os = "linux")]
//! The memory of a canonical form above `MAX_DOCUMENT_BYTES` (finding D2 of the security review of 2026-11-16).
//! The writer made the whole text (27.6 MB for `lines_split`) before the check of its size. It now measures the
//! length first and allocates no text above the limit. This file holds one test, because the peak of the process
//! is global.

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

/// `lines_split` of the review (4,142,135 bytes): 6 panels, each with one query of 230,000 empty lines. Each text
/// is valid, and the canonical form has 27,604,816 bytes.
fn lines_split() -> Vec<u8> {
    let lines = vec!["\"\""; 230_000].join(",");
    let panels: Vec<String> = (0..6)
        .map(|i| {
            format!(
                "{{\"id\":{},\"type\":\"table\",\"title\":\"t\",\"description\":\"\",\"transparent\":false,\
                 \"grid\":{{\"x\":0,\"y\":{i},\"w\":24,\"h\":1}},\"data\":{{\"query\":{{\"source\":\"s\",\
                 \"format\":\"table\",\"sql\":[{lines}],\"min_interval\":null,\"max_data_points\":null}}}},\
                 \"transforms\":[],\"fields\":{{\"defaults\":{{}},\"overrides\":[]}},\
                 \"options\":{{\"show_header\":true,\"sort\":[]}},\"links\":[]}}",
                i + 1
            )
        })
        .collect();
    format!(
        "{{\"format_version\":3,\"uid\":\"v2poc\",\"title\":\"v2\",\"description\":\"\",\"tags\":[],\
         \"time\":{{\"from\":\"now-1h\",\"to\":\"now\",\"timezone\":\"utc\",\"refresh\":null}},\"cursor_sync\":\"off\",\
         \"links\":[],\"variables\":[],\"sections\":[{{\"title\":null,\"collapsed\":false,\"panels\":[{}]}}]}}",
        panels.join(",")
    )
    .into_bytes()
}

#[test]
fn d2_a_canonical_form_above_the_limit_is_refused_with_no_text() {
    let bytes = lines_split();
    assert_eq!(bytes.len(), 4_142_135);
    let base = peak_rss();
    let e = validate(&bytes, None).unwrap_err();
    let growth = peak_rss().saturating_sub(base);
    eprintln!("the peak grew by {growth} bytes");
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!((e[0].code, e[0].pointer.as_str()), (ErrorCode::TooLarge, ""));
    assert_eq!(e[0].message, "canonical document must have at most 4194304 bytes; value 27604816 bytes");
    // 198 MB of resident memory before the fix. The tree of the JSON text and the typed document stay: one value
    // of 32 bytes and one text of 24 bytes for each line.
    assert!(growth < 120 * 1024 * 1024, "the peak grew by {growth} bytes");
}
