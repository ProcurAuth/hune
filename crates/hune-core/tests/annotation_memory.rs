#![cfg(target_os = "linux")]
//! The memory of the tag reader (finding 3 of the review of 2026-11-14). A cell of 10 MB must not cost a copy
//! of each item. This file holds one test, because the peak of the process is global.

use hune_core::annotation::{AnnotationReason, annotations_from_frame};
use hune_core::frame::{Frame, FrameBuilder, FrameStats};
use std::fs;

/// The peak resident memory of the process in bytes (`VmHWM`), on Linux. The crate forbids `unsafe`, so the
/// test reads the peak from the system instead of a counting allocator.
fn peak_rss() -> usize {
    let status = fs::read_to_string("/proc/self/status").expect("/proc/self/status");
    let line = status.lines().find(|l| l.starts_with("VmHWM")).expect("VmHWM");
    let kb: usize = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).expect("a number of kB");
    kb * 1024
}

fn frame(cell: &str) -> Frame {
    let row = serde_json::to_string(&serde_json::json!(["1", cell])).unwrap();
    let mut b =
        FrameBuilder::new(vec!["time".into(), "tags".into()], vec!["DateTime64(3)".into(), "String".into()]).unwrap();
    b.push_row(&row).unwrap();
    b.finish("q".into(), FrameStats::default())
}

/// The growth of the peak of the process during the call, in bytes, and its result. The peak only grows, so
/// the growth is 0 when an earlier step of the test had a higher peak: the old code added 230 MB or more.
fn measure(f: &Frame) -> (usize, Result<usize, AnnotationReason>) {
    let base = peak_rss();
    let r = annotations_from_frame(f).map(|m| m[0].tags.len()).map_err(|e| e.reason);
    (peak_rss().saturating_sub(base), r)
}

#[test]
fn a_cell_of_10_mb_has_a_small_peak_and_the_right_answer() {
    const CELL: usize = 10_000_000;
    // Only commas: no tag at all. The old code held 10 million empty strings (240 MB).
    let (peak, r) = measure(&frame(&",".repeat(CELL)));
    assert_eq!(r, Ok(0));
    assert!(peak < 20_000_000, "the peak is {peak} bytes");
    // A JSON array of 3 million empty texts (the cell is 10 MB).
    let json = format!("[{}]", vec!["\"\""; CELL / 3].join(","));
    let (peak, r) = measure(&frame(&json));
    assert_eq!(r, Ok(0));
    assert!(peak < 20_000_000, "the peak is {peak} bytes");
    // Nine distinct tags and then the commas: `too_long` after the ninth, with no copy of the rest.
    let nine = format!("a,b,c,d,e,f,g,h,i{}", ",".repeat(CELL));
    let (peak, r) = measure(&frame(&nine));
    assert_eq!(r, Err(AnnotationReason::TooLong));
    assert!(peak < 20_000_000, "the peak is {peak} bytes");
    let json = format!("[\"a\",\"b\",\"c\",\"d\",\"e\",\"f\",\"g\",\"h\",\"i\"{}]", ",\"\"".repeat(CELL / 3));
    let (peak, r) = measure(&frame(&json));
    assert_eq!(r, Err(AnnotationReason::TooLong));
    assert!(peak < 20_000_000, "the peak is {peak} bytes");
}
