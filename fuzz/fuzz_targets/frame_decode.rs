//! Fuzz target `frame_decode` (16.16, 8.4, 7.16).
//!
//! The input is the text of an answer of ClickHouse in the format of 8.4: line 1 holds the column
//! names, line 2 holds the type texts, and each next line is one row.
//!
//! Property: no input makes the builder panic. When the header and all rows pass, the frame gives
//! a valid JSON text (a number out of the range of a double is valid). When serde_json reads it,
//! the members `columns` and `values` have the same length, each column of `values` has `rows`
//! items, and the member `rows` is the number of rows that the builder took.
#![no_main]

use libfuzzer_sys::fuzz_target;
use hune_core::frame::{FrameBuilder, FrameStats};

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
    let columns = builder.column_count();
    let mut rows = 0usize;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        // A builder that gave an error MUST NOT be used again (8.4): stop here.
        if builder.push_row(line).is_err() {
            return;
        }
        rows += 1;
        assert_eq!(builder.rows(), rows);
    }
    let frame = builder.finish("q".to_owned(), FrameStats::default());
    assert_eq!(frame.rows, rows);
    assert_eq!(frame.columns.len(), columns);
    let json = frame.to_json();
    // The frame writes the exact digits of a number. A number such as `21e308` is valid JSON, but
    // serde_json refuses to read it as a double. The grammar check below does not read the value.
    serde_json::from_str::<serde::de::IgnoredAny>(&json).expect("the frame JSON is not valid JSON");
    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(e) if e.to_string().starts_with("number out of range") => return,
        Err(e) => panic!("the frame JSON does not parse: {e}"),
    };
    let cols = value["columns"].as_array().expect("columns is not an array");
    let vals = value["values"].as_array().expect("values is not an array");
    assert_eq!(cols.len(), columns);
    assert_eq!(vals.len(), columns);
    assert_eq!(value["rows"].as_u64(), Some(rows as u64));
    for column in vals {
        assert_eq!(column.as_array().expect("a column of values is not an array").len(), rows);
    }
});
