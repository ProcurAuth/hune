// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fuzz target `annotation_frame` (16.3, 7.37).
//!
//! The input is the text of an answer of ClickHouse in the format of 8.4, as for `frame_decode`: line 1 holds
//! the column names, line 2 holds the type texts, and each next line is one row. The target builds the frame
//! and gives it to `annotations_from_frame`.
//!
//! Property: no input makes the function panic. A fault names a row that exists, or no row. A result obeys
//! 7.37: one mark for each row, in the order of the time, a time in range, an end above the time, and the limits
//! and the character rules for each title, text and tag. The result is the same in two runs.
#![no_main]

use hune_core::annotation::{MAX_MARK_MS, annotations_from_frame};
use hune_core::frame::{FrameBuilder, FrameStats};
use hune_core::limits::{
    MAX_ANNOTATION_TAG_CHARS, MAX_ANNOTATION_TAGS, MAX_ANNOTATION_TEXT_BYTES, MAX_ANNOTATION_TITLE_CHARS,
};
use libfuzzer_sys::fuzz_target;

/// The character rules of 7.37, rule 9.
fn clean(s: &str) -> bool {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\t' | '\n' => {}
            '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => return false,
            '<' if chars.peek().is_some_and(|n| n.is_ascii_alphabetic() || *n == '/' || *n == '!') => return false,
            _ => {}
        }
    }
    true
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let mut lines = text.split('\n');
    let (Some(names), Some(types)) = (lines.next(), lines.next()) else {
        return;
    };
    let Ok(mut builder) = FrameBuilder::from_header_lines(names, types) else {
        return;
    };
    for line in lines {
        if line.is_empty() {
            continue;
        }
        // A builder that gave an error MUST NOT be used again (8.4): stop here.
        if builder.push_row(line).is_err() {
            return;
        }
    }
    let frame = builder.finish("q".to_owned(), FrameStats::default());
    let first = annotations_from_frame(&frame);
    assert_eq!(first, annotations_from_frame(&frame), "the function is not deterministic");
    let marks = match first {
        Ok(marks) => marks,
        Err(fault) => {
            if let Some(row) = fault.row {
                assert!((1..=frame.rows).contains(&row), "the fault names the row {row} of {} rows", frame.rows);
            }
            return;
        }
    };
    assert_eq!(marks.len(), frame.rows, "one mark for each row");
    for pair in marks.windows(2) {
        assert!(pair[0].time_ms <= pair[1].time_ms, "the marks are not in the order of the time");
    }
    for m in &marks {
        assert!((0..=MAX_MARK_MS).contains(&m.time_ms));
        if let Some(end) = m.end_ms {
            assert!(end > m.time_ms && end <= MAX_MARK_MS, "the end is not above the time or not in range");
        }
        assert!(m.title.chars().count() <= MAX_ANNOTATION_TITLE_CHARS && clean(&m.title));
        assert!(m.text.len() <= MAX_ANNOTATION_TEXT_BYTES && clean(&m.text));
        assert!(m.tags.len() <= MAX_ANNOTATION_TAGS);
        for (i, tag) in m.tags.iter().enumerate() {
            assert!(!tag.is_empty() && tag.trim() == tag, "a tag is empty or not trimmed");
            assert!(tag.chars().count() <= MAX_ANNOTATION_TAG_CHARS && clean(tag));
            assert!(!m.tags[..i].contains(tag), "two equal tags");
        }
    }
});
