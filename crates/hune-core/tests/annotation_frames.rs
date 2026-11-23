// SPDX-License-Identifier: MIT OR Apache-2.0
//! The marks of an annotation query (7.37): the golden vectors of `tests/golden/annotations/`, the hostile
//! texts, the order of the marks and the fixed messages. The function is pure: it reads a frame only.

use hune_core::annotation::{AnnotationFault, AnnotationReason, Mark, annotations_from_frame};
use hune_core::frame::{Frame, FrameBuilder, FrameStats};
use serde_json::{Value, json};

const FRAMES: &str = include_str!("../../../tests/golden/annotations/frames.json");
const TEXTS: &str = include_str!("../../../tests/golden/annotations/texts.json");

fn frame(columns: &[(String, String)], rows: &[Value]) -> Frame {
    let mut b =
        FrameBuilder::new(columns.iter().map(|c| c.0.clone()).collect(), columns.iter().map(|c| c.1.clone()).collect())
            .expect("header");
    for row in rows {
        b.push_row(&serde_json::to_string(row).unwrap()).unwrap_or_else(|e| panic!("row {row}: {e}"));
    }
    b.finish("q".into(), FrameStats::default())
}

fn columns_of(case: &Value) -> Vec<(String, String)> {
    case["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["name"].as_str().unwrap().to_owned(), c["type"].as_str().unwrap().to_owned()))
        .collect()
}

fn marks_json(marks: &[Mark]) -> Value {
    Value::Array(marks.iter().map(|m| json!([m.time_ms, m.end_ms, m.title, m.text, m.tags])).collect::<Vec<Value>>())
}

fn reason_of(text: &str) -> AnnotationReason {
    *AnnotationReason::ALL.iter().find(|r| r.as_str() == text).unwrap_or_else(|| panic!("reason {text}"))
}

#[test]
fn there_are_66_vectors_and_each_one_gives_its_marks_or_its_fault() {
    let doc: Value = serde_json::from_str(FRAMES).unwrap();
    let cases = doc["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 66);
    let mut names: Vec<&str> = cases.iter().map(|c| c["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), cases.len(), "the names are unique");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let f = frame(&columns_of(case), case["rows"].as_array().unwrap());
        let got = annotations_from_frame(&f);
        if let Some(e) = case.get("error") {
            let want = AnnotationFault {
                reason: reason_of(e["reason"].as_str().unwrap()),
                row: e["row"].as_u64().map(|r| r as usize),
            };
            assert_eq!(got.unwrap_err(), want, "{name}");
        } else {
            let marks = got.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(marks_json(&marks), case["marks"], "{name}");
        }
    }
}

fn one_text_frame(column: &str, kind: &str, cell: Value) -> Frame {
    frame(
        &[("time".to_owned(), "DateTime64(3)".to_owned()), (column.to_owned(), kind.to_owned())],
        &[json!(["1794211800.000", cell])],
    )
}

#[test]
fn hostile_texts_42_give_bad_chars_in_a_title_a_text_and_a_tag() {
    let doc: Value = serde_json::from_str(TEXTS).unwrap();
    let hostile = doc["hostile"].as_array().unwrap();
    assert_eq!(hostile.len(), 42);
    for h in hostile {
        let (name, text) = (h["name"].as_str().unwrap(), h["text"].as_str().unwrap());
        for column in ["title", "text"] {
            let f = one_text_frame(column, "String", json!(text));
            let e = annotations_from_frame(&f).unwrap_err();
            assert_eq!((e.reason, e.row), (AnnotationReason::BadChars, Some(1)), "{name} in {column}");
        }
        // A tag: the text as one tag in a JSON array, and as a text that the function splits.
        let f = one_text_frame("tags", "Array(String)", json!([text]));
        let e = annotations_from_frame(&f).unwrap_err();
        assert_eq!(e.reason, AnnotationReason::BadChars, "{name} in a tag (array)");
        let f = one_text_frame("tags", "String", json!(format!("ok,{text}")));
        let e = annotations_from_frame(&f).unwrap_err();
        assert_eq!(e.reason, AnnotationReason::BadChars, "{name} in a tag (split)");
    }
}

#[test]
fn safe_texts_12_with_a_less_than_sign_an_ampersand_quotes_and_javascript_pass_unchanged() {
    let doc: Value = serde_json::from_str(TEXTS).unwrap();
    let safe = doc["safe"].as_array().unwrap();
    assert_eq!(safe.len(), 12);
    for s in safe {
        let (name, text) = (s["name"].as_str().unwrap(), s["text"].as_str().unwrap());
        for column in ["title", "text"] {
            let marks = annotations_from_frame(&one_text_frame(column, "String", json!(text))).unwrap();
            let got = if column == "title" { &marks[0].title } else { &marks[0].text };
            assert_eq!(got, text, "{name} in {column}");
        }
    }
}

#[test]
fn a_tag_with_a_line_break_character_at_its_edge_is_refused_and_not_trimmed_away() {
    for c in ['\r', '\u{b}', '\u{c}', '\u{85}', '\u{1c}'] {
        let f = one_text_frame("tags", "String", json!(format!("a,{c}")));
        assert_eq!(annotations_from_frame(&f).unwrap_err().reason, AnnotationReason::BadChars, "{c:?}");
    }
    // A tab and a line feed are allowed, and the trim removes them at the edges.
    let f = one_text_frame("tags", "String", json!("\ta\n,b"));
    assert_eq!(annotations_from_frame(&f).unwrap()[0].tags, ["a", "b"]);
}

#[test]
fn no_message_holds_a_cell_value() {
    // The message is a fixed sentence for each reason: it has no part that depends on the frame.
    let secret = "SECRET-CELL-<b>";
    let f = one_text_frame("title", "String", json!(secret));
    let e = annotations_from_frame(&f).unwrap_err();
    assert!(!e.reason.message().contains("SECRET"));
    assert!(!format!("{e:?}").contains("SECRET"));
    let all: Vec<&str> = AnnotationReason::ALL.iter().map(|r| r.message()).collect();
    assert_eq!(all.len(), 7);
    for m in &all {
        assert!(m.ends_with('.') && m.len() < 120, "{m}");
    }
    let mut unique = all.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 7);
    let names: Vec<&str> = AnnotationReason::ALL.iter().map(|r| r.as_str()).collect();
    assert_eq!(names, ["no_time", "bad_time", "bad_end", "bad_text", "bad_chars", "too_long", "duplicate_column"]);
}

#[test]
fn the_marks_come_in_the_order_of_the_time_then_of_the_row() {
    let rows: Vec<Value> =
        (0..200).map(|i| json!([format!("{}.000", 1_794_211_800 + (i * 7919) % 13), format!("r{i}")])).collect();
    let f = frame(&[("time".into(), "DateTime64(3)".into()), ("title".into(), "String".into())], &rows);
    let marks = annotations_from_frame(&f).unwrap();
    assert_eq!(marks.len(), 200, "the function drops nothing and merges nothing");
    let row_of = |m: &Mark| m.title[1..].parse::<usize>().unwrap();
    for pair in marks.windows(2) {
        let key = |m: &Mark| (m.time_ms, row_of(m));
        assert!(key(&pair[0]) < key(&pair[1]), "{:?} then {:?}", key(&pair[0]), key(&pair[1]));
    }
}

#[test]
fn a_frame_with_no_column_and_no_row_gives_no_mark_and_with_rows_has_no_time() {
    // Rule 13 of 7.37: the empty result of a `mongodb` source has no column (7.16c). It is no
    // fault. A frame with no column and rows (no source gives one) keeps the reason `no_time` of rule 3.
    let mut f = Frame {
        etag: None,
        query_id: "q".into(),
        interval_ms: None,
        columns: Vec::new(),
        rows: 0,
        stats: FrameStats::default(),
    };
    assert_eq!(annotations_from_frame(&f), Ok(Vec::new()));
    f.rows = 2;
    let e = annotations_from_frame(&f).unwrap_err();
    assert_eq!((e.reason, e.row), (AnnotationReason::NoTime, None));
}

#[test]
fn zero_width_characters_the_separators_the_byte_order_mark_and_a_processing_instruction_are_refused() {
    // Finding 5 of the review of 2026-11-14: they hide or change a text as the bidirectional controls do.
    let mut hostile: Vec<String> = ('\u{200b}'..='\u{200f}').map(|c| format!("a{c}b")).collect();
    hostile.extend(['\u{61c}', '\u{2028}', '\u{2029}', '\u{feff}'].map(|c| format!("a{c}b")));
    hostile.push("<?php echo 1 ?>".to_owned());
    hostile.push("a <? b".to_owned());
    for text in &hostile {
        for column in ["title", "text"] {
            let f = one_text_frame(column, "String", json!(text));
            let e = annotations_from_frame(&f).unwrap_err();
            assert_eq!((e.reason, e.row), (AnnotationReason::BadChars, Some(1)), "{text:?} in {column}");
        }
        let f = one_text_frame("tags", "Array(String)", json!([text]));
        assert_eq!(annotations_from_frame(&f).unwrap_err().reason, AnnotationReason::BadChars, "{text:?} (array)");
        let f = one_text_frame("tags", "String", json!(format!("ok,{text}")));
        assert_eq!(annotations_from_frame(&f).unwrap_err().reason, AnnotationReason::BadChars, "{text:?} (split)");
    }
    // A `<` before a space or at the end, and the neighbours of the ranges, still pass.
    for ok in ["a < b", "a<", "x\u{200a}y", "x\u{2027}y", "x\u{2030}y", "x\u{fefe}y", "x\u{61d}y"] {
        let marks = annotations_from_frame(&one_text_frame("title", "String", json!(ok))).unwrap();
        assert_eq!(marks[0].title, ok);
    }
}

#[test]
fn the_tag_forms_keep_their_answers_with_the_one_pass_reader() {
    let tags = |kind: &str, cell: Value| {
        annotations_from_frame(&one_text_frame("tags", kind, cell)).map(|m| m[0].tags.clone()).map_err(|e| e.reason)
    };
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
    assert_eq!(tags("Array(String)", json!(["b", " a ", "b", ""])), Ok(s(&["b", "a"])));
    assert_eq!(tags("String", json!("b, a ,b,,")), Ok(s(&["b", "a"])));
    // A text that starts with `[` and is not an array of texts is a list with commas, as before.
    assert_eq!(tags("String", json!("[a,b]")), Ok(s(&["[a", "b]"])));
    assert_eq!(tags("String", json!("[1,2]")), Ok(s(&["[1", "2]"])));
    assert_eq!(tags("String", json!("[\"a\",\"b\"] x")), Ok(s(&["[\"a\"", "\"b\"] x"])));
    // The limit: 8 tags are allowed, the ninth distinct tag is `too_long`, and an equal tag is no new tag.
    let eight: Vec<String> = (0..8).map(|i| format!("t{i}")).collect();
    assert_eq!(tags("Array(String)", json!(eight)).map(|t| t.len()), Ok(8));
    let mut nine = eight.clone();
    nine.push("t8".to_owned());
    assert_eq!(tags("Array(String)", json!(nine)), Err(AnnotationReason::TooLong));
    assert_eq!(tags("String", json!(nine.join(","))), Err(AnnotationReason::TooLong));
    let mut dup = eight;
    dup.push("t0".to_owned());
    assert_eq!(tags("Array(String)", json!(dup)).map(|t| t.len()), Ok(8));
    // The first fault in the order of the items wins.
    assert_eq!(tags("Array(String)", json!(["a\u{7}", "b"])), Err(AnnotationReason::BadChars));
    assert_eq!(tags("Array(String)", json!(["x".repeat(65), "a\u{7}"])), Err(AnnotationReason::TooLong));
    // A JSON text that is invalid after the limit is read as a list with commas, as before.
    let bad_tail = format!("[{}, 1]", (0..9).map(|i| format!("\"t{i}\"")).collect::<Vec<_>>().join(","));
    assert_eq!(tags("String", json!(bad_tail)), Err(AnnotationReason::TooLong));
}
