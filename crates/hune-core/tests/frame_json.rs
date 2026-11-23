// SPDX-License-Identifier: MIT OR Apache-2.0
//! The JSON output of a frame (7.16).

use hune_core::frame::{Frame, FrameBuilder, FrameStats, write_json_string};

/// The example of 7.16.
fn spec_example() -> Frame {
    let mut b = FrameBuilder::from_header_lines(
        r#"["t","node","weight","blocks","active"]"#,
        r#"["DateTime","LowCardinality(String)","Decimal(38, 9)","UInt32","Bool"]"#,
    )
    .unwrap();
    b.push_row(r#"["1794211200","a1","12.500000000",42,true]"#).unwrap();
    b.push_row(r#"["1794211220",null,"3.000000000",7,false]"#).unwrap();
    let mut f = b.finish(
        "019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d".into(),
        FrameStats { elapsed_ms: 81, read_rows: Some(120_000), read_bytes: Some(9_000_000), result_bytes: Some(512) },
    );
    f.etag = Some("1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f".into());
    f.interval_ms = Some(20_000);
    f
}

#[test]
fn the_example_of_7_16() {
    let json = spec_example().to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let expected: serde_json::Value = serde_json::from_str(
        r#"{
  "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f",
  "query_id": "019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d",
  "interval_ms": 20000,
  "columns": [
    { "name": "t", "type": "DateTime", "kind": "time" },
    { "name": "node", "type": "LowCardinality(String)", "kind": "text" },
    { "name": "weight", "type": "Decimal(38, 9)", "kind": "number_text" },
    { "name": "blocks", "type": "UInt32", "kind": "number" },
    { "name": "active", "type": "Bool", "kind": "bool" }
  ],
  "values": [
    [1794211200, 1794211220],
    ["a1", null],
    ["12.500000000", "3.000000000"],
    [42, 7],
    [true, false]
  ],
  "rows": 2,
  "stats": { "elapsed_ms": 81, "read_rows": 120000, "read_bytes": 9000000, "result_bytes": 512 }
}"#,
    )
    .unwrap();
    assert_eq!(parsed, expected);
}

#[test]
fn the_member_order_is_the_order_of_7_16() {
    let json = spec_example().to_json();
    let at = |k: &str| json.find(&format!("\"{k}\":")).unwrap();
    let order = ["etag", "query_id", "interval_ms", "columns", "values", "rows", "stats"].map(at);
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{json}");
}

#[test]
fn time_digits_are_written_as_they_are() {
    let mut b = FrameBuilder::new(vec!["t".into()], vec!["DateTime64(3)".into()]).unwrap();
    b.push_row(r#"["1794211200.125"]"#).unwrap();
    b.push_row(r#"["1794211200.500"]"#).unwrap();
    b.push_row(r#"["-1.000"]"#).unwrap();
    let json = b.finish("q".into(), FrameStats::default()).to_json();
    assert!(json.contains(r#""values":[[1794211200.125,1794211200.500,-1.000]]"#), "{json}");
}

#[test]
fn large_integers_and_decimals_stay_text() {
    let mut b =
        FrameBuilder::new(vec!["a".into(), "d".into()], vec!["UInt64".into(), "Decimal(38, 9)".into()]).unwrap();
    b.push_row(r#"["18446744073709551615","99999999999999999999999999999.999999999"]"#).unwrap();
    let json = b.finish("q".into(), FrameStats::default()).to_json();
    assert!(json.contains(r#"[["18446744073709551615"],["99999999999999999999999999999.999999999"]]"#), "{json}");
}

#[test]
fn nan_and_infinities_are_text_numbers_are_raw() {
    let mut b = FrameBuilder::new(vec!["f".into()], vec!["Float64".into()]).unwrap();
    for r in [r#"["nan"]"#, r#"["inf"]"#, r#"["-inf"]"#, "[1.5]", "[null]", "[-0]"] {
        b.push_row(r).unwrap();
    }
    let json = b.finish("q".into(), FrameStats::default()).to_json();
    assert!(json.contains(r#""values":[["nan","inf","-inf",1.5,null,-0]]"#), "{json}");
}

#[test]
fn stats_are_null_when_the_header_lacks_them() {
    let b = FrameBuilder::new(vec!["a".into()], vec!["UInt8".into()]).unwrap();
    let f =
        b.finish("q".into(), FrameStats { elapsed_ms: 5, read_rows: None, read_bytes: Some(3), result_bytes: None });
    let json = f.to_json();
    assert!(json.contains(r#""stats":{"elapsed_ms":5,"read_rows":null,"read_bytes":3,"result_bytes":null}"#), "{json}");
    // No etag and no interval: the members are absent.
    assert!(!json.contains("etag"));
    assert!(!json.contains("interval_ms"));
    assert!(json.starts_with(r#"{"query_id":"q","#), "{json}");
}

#[test]
fn text_is_escaped_and_the_output_parses_back() {
    let mut b =
        FrameBuilder::new(vec!["na\"me\n".into(), "a\\b".into()], vec!["String".into(), "Array(String)".into()])
            .unwrap();
    b.push_row(r#"["x\"y\\z\n\t\u0001 é","[\"q\"]"]"#).unwrap();
    let f = b.finish("q".into(), FrameStats::default());
    let json = f.to_json();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["columns"][0]["name"], "na\"me\n");
    assert_eq!(v["columns"][1]["name"], "a\\b");
    assert_eq!(v["values"][0][0], "x\"y\\z\n\t\u{1}\u{2028}é");
    assert_eq!(v["values"][1][0], "[\"q\"]");
}

#[test]
fn zero_rows_give_empty_arrays() {
    let b = FrameBuilder::new(vec!["a".into(), "b".into()], vec!["UInt8".into(), "Nothing".into()]).unwrap();
    let json = b.finish("q".into(), FrameStats::default()).to_json();
    assert!(json.contains(r#""values":[[],[]],"rows":0"#), "{json}");
}

#[test]
fn json_string_writer() {
    let mut s = String::new();
    write_json_string(&mut s, "a\u{1f}\u{7f}\"\\/");
    assert_eq!(s, "\"a\\u001f\u{7f}\\\"\\\\/\"");
}

#[test]
fn frame_memory_stays_near_the_size_of_the_rows() {
    let mut b = FrameBuilder::new(vec!["n".into(), "s".into()], vec!["UInt32".into(), "String".into()]).unwrap();
    let mut wire = 0usize;
    for i in 0..20_000u32 {
        let line = format!(r#"[{i},"value-{i}"]"#);
        wire += line.len() + 1;
        b.push_row(&line).unwrap();
    }
    let f = b.finish("q".into(), FrameStats::default());
    assert_eq!(f.rows, 20_000);
    assert!(f.approx_bytes() <= wire + wire / 2, "frame {} bytes for {} wire bytes", f.approx_bytes(), wire);
}
