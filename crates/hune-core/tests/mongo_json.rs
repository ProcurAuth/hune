// SPDX-License-Identifier: MIT OR Apache-2.0
//! The strict JSON profile J of 7.28 (J1 to J6) and the limits of `limits.rs`.

mod mongo_common;

use hune_core::error::ErrorCode;
use hune_core::mongo::limits::*;
use hune_core::mongo::{JsonErrorKind, MongoValue, parse_text, to_query_json};
use mongo_common::*;

fn value(text: &str) -> MongoValue {
    parse_text(text).unwrap_or_else(|e| panic!("{text}: {e}")).value
}

fn kind(text: &str) -> JsonErrorKind {
    parse_text(text).expect_err(text).kind
}

#[test]
fn j1_text_follows_rfc_8259() {
    assert_eq!(kind("\u{feff}{}"), JsonErrorKind::BadFormat);
    for t in [
        "{} // c",
        "/* c */ {}",
        "{\"a\":1,}",
        "[1,]",
        "{} {}",
        "{}x",
        "{\"a\":1} 2",
        "{'a':1}",
        "{a:1}",
        "{\"a\":01}",
        "{\"a\":+1}",
        "{\"a\":.5}",
        "{\"a\":1.}",
        "{\"a\":NaN}",
        "{\"a\":Infinity}",
        "{\"a\":-Infinity}",
        "{\"a\":\"\t\"}",
        "{\"a\":\"\\x\"}",
        "{\"a\":\"\\u12\"}",
        "{\"a\":tru}",
        "",
        "   ",
    ] {
        assert_eq!(kind(t), JsonErrorKind::BadFormat, "{t:?}");
    }
    // The blank bytes are space, TAB, LF and CR only.
    for b in ["\u{b}", "\u{c}", "\u{a0}", "\u{2028}", "\u{3000}"] {
        assert!(parse_text(&format!("{b}{{}}")).is_err(), "{b:?}");
        assert!(parse_text(&format!("{{\"a\"{b}:1}}")).is_err(), "{b:?}");
    }
    assert!(parse_text(" \t\r\n{ \"a\" :\r\n 1 }\n").is_ok());
    // A lone surrogate is not text.
    assert!(parse_text(r#"{"a":"\ud800"}"#).is_err());
    assert!(parse_text(r#"{"a":"\udc00"}"#).is_err());
    assert!(parse_text(r#"{"a":"\ud83d\ude00"}"#).is_ok());
}

#[test]
fn j1_fault_gives_bad_format_at_save_with_the_position() {
    let r = save("{\"collection\": \"hosts\",\n \"aggregate\": [{\"$limit\": 1,}]}");
    assert_eq!(r.faults.len(), 1);
    assert_eq!(r.faults[0].code, ErrorCode::BadFormat);
    assert_eq!(r.faults[0].line_index, 1);
}

#[test]
fn j2_duplicate_member_names() {
    assert_eq!(kind(r#"{"a":1,"a":2}"#), JsonErrorKind::BadFormat);
    assert_eq!(kind(r#"{"a":{"b":1,"b":1}}"#), JsonErrorKind::BadFormat);
    assert_eq!(kind(r#"[{"a":1,"a":1}]"#), JsonErrorKind::BadFormat);
    // The same name in two objects is valid. A name that differs only in escapes is the same name.
    assert!(parse_text(r#"{"a":{"a":1}}"#).is_ok());
    assert!(parse_text(r#"{"a":1,"\u0061":2}"#).is_err());
    assert_eq!(
        save_codes(r#"{"collection":"hosts","collection":"hosts","aggregate":[{"$limit":1}]}"#),
        [ErrorCode::BadFormat]
    );
}

#[test]
fn j3_no_nul_in_a_string_or_a_key() {
    assert!(parse_text(r#"{"a":"x\u0000y"}"#).is_err());
    assert!(parse_text(r#"{"a\u0000":1}"#).is_err());
    assert!(parse_text("{\"a\":\"x\0y\"}").is_err());
    assert!(parse_text("{\"a\0\":1}").is_err());
    assert!(parse_text(r#"{"a":"\u0001"}"#).is_ok());
}

#[test]
fn j4_integers_are_int32_int64_or_an_error() {
    assert_eq!(value("0"), MongoValue::Int32(0));
    assert_eq!(value("-0"), MongoValue::Int32(0));
    assert_eq!(value("2147483647"), MongoValue::Int32(i32::MAX));
    assert_eq!(value("-2147483648"), MongoValue::Int32(i32::MIN));
    assert_eq!(value("2147483648"), MongoValue::Int64(2_147_483_648));
    assert_eq!(value("-2147483649"), MongoValue::Int64(-2_147_483_649));
    assert_eq!(value("9223372036854775807"), MongoValue::Int64(i64::MAX));
    assert_eq!(value("-9223372036854775808"), MongoValue::Int64(i64::MIN));
    assert!(parse_text("9223372036854775808").is_err());
    assert!(parse_text("-9223372036854775809").is_err());
    assert!(parse_text(&"9".repeat(400)).is_err());
}

#[test]
fn j5_fraction_or_exponent_is_a_finite_double() {
    assert_eq!(value("1.0"), MongoValue::Double(1.0));
    assert_eq!(value("-0.0").type_name(), "double");
    assert_eq!(value("1e2"), MongoValue::Double(100.0));
    assert_eq!(value("1E+2"), MongoValue::Double(100.0));
    assert_eq!(value("5e-324"), MongoValue::Double(5e-324));
    assert!(parse_text("1e999").is_err());
    assert!(parse_text("-1e999").is_err());
    assert!(parse_text("1e").is_err());
    assert!(parse_text("1e+").is_err());
    // A big integer with an exponent is a double, not an error.
    assert_eq!(value("1e30").type_name(), "double");
}

#[test]
fn j6_typed_objects() {
    assert_eq!(value(r#"{"$date":"2026-11-09T14:00:00Z"}"#), MongoValue::DateMs(1_794_232_800_000));
    assert_eq!(value(r#"{"$date":"2026-11-09T14:00:00.5Z"}"#), MongoValue::DateMs(1_794_232_800_500));
    assert_eq!(value(r#"{"$date":"2026-11-09T14:00:00.123Z"}"#), MongoValue::DateMs(1_794_232_800_123));
    assert_eq!(value(r#"{"$date":"0001-01-01T00:00:00Z"}"#), MongoValue::DateMs(-62_135_596_800_000));
    assert_eq!(value(r#"{"$date":"9999-12-31T23:59:59.999Z"}"#), MongoValue::DateMs(253_402_300_799_999));
    assert_eq!(value(r#"{"$date":{"$numberLong":"1794232800000"}}"#), MongoValue::DateMs(1_794_232_800_000));
    assert_eq!(value(r#"{"$date":{"$numberLong":"-5"}}"#), MongoValue::DateMs(-5));
    for bad in [
        r#"{"$date":"2026-11-09T14:00:00"}"#,
        r#"{"$date":"2026-11-09T14:00:00+00:00"}"#,
        r#"{"$date":"2026-11-09T14:00:00.1234Z"}"#,
        r#"{"$date":"2026-11-09T14:00:00.Z"}"#,
        r#"{"$date":"2026-11-09 14:00:00Z"}"#,
        r#"{"$date":"2026-02-29T00:00:00Z"}"#,
        r#"{"$date":"2026-13-01T00:00:00Z"}"#,
        r#"{"$date":"0000-01-01T00:00:00Z"}"#,
        r#"{"$date":"2026-11-09T24:00:00Z"}"#,
        r#"{"$date":"2026-11-09T14:00:60Z"}"#,
        r#"{"$date":1794232800000}"#,
        r#"{"$date":{"$numberLong":"12345678901234567"}}"#,
        r#"{"$date":{"$numberLong":5}}"#,
        r#"{"$date":{"$oid":"0123456789abcdef01234567"}}"#,
        r#"{"$date":{"a":1}}"#,
        r#"{"$date":null}"#,
        r#"{"$date":"2026-11-09T14:00:00Z","x":1}"#,
        r#"{"x":1,"$date":"2026-11-09T14:00:00Z"}"#,
    ] {
        assert!(parse_text(bad).is_err(), "{bad}");
    }
    assert!(matches!(value(r#"{"$oid":"0123456789ABCDEF01234567"}"#), MongoValue::ObjectId(_)));
    assert_eq!(value(r#"{"$oid":"0123456789ABCDEF01234567"}"#), value(r#"{"$oid":"0123456789abcdef01234567"}"#));
    for bad in [
        r#"{"$oid":"0123"}"#,
        r#"{"$oid":"0123456789abcdef0123456g"}"#,
        r#"{"$oid":5}"#,
        r#"{"$oid":"0123456789abcdef01234567","a":1}"#,
    ] {
        assert!(parse_text(bad).is_err(), "{bad}");
    }
    assert_eq!(value(r#"{"$numberLong":"5"}"#), MongoValue::Int64(5));
    assert_eq!(value(r#"{"$numberLong":"-9223372036854775808"}"#), MongoValue::Int64(i64::MIN));
    for bad in [
        r#"{"$numberLong":"9223372036854775808"}"#,
        r#"{"$numberLong":"+5"}"#,
        r#"{"$numberLong":"5.0"}"#,
        r#"{"$numberLong":""}"#,
        r#"{"$numberLong":"-"}"#,
        r#"{"$numberLong":5}"#,
        r#"{"$numberLong":"12345678901234567890"}"#,
    ] {
        assert!(parse_text(bad).is_err(), "{bad}");
    }
    assert_eq!(value(r#"{"$numberDecimal":"12.50"}"#).type_name(), "decimal");
    assert_eq!(value(r#"{"$numberDecimal":"-1.5e+3"}"#).type_name(), "decimal");
    assert!(parse_text(&format!(r#"{{"$numberDecimal":"{}"}}"#, "1".repeat(34))).is_ok());
    for bad in [
        r#"{"$numberDecimal":"NaN"}"#,
        r#"{"$numberDecimal":"Infinity"}"#,
        r#"{"$numberDecimal":"1."}"#,
        r#"{"$numberDecimal":".5"}"#,
        r#"{"$numberDecimal":"1e"}"#,
        r#"{"$numberDecimal":1.5}"#,
    ] {
        assert!(parse_text(bad).is_err(), "{bad}");
    }
    assert!(parse_text(&format!(r#"{{"$numberDecimal":"{}"}}"#, "1".repeat(35))).is_err());
}

#[test]
fn j6_other_forms_of_extended_json_are_refused_by_the_grammar() {
    for form in [
        r#"{"$binary":{"base64":"AA==","subType":"00"}}"#,
        r#"{"$uuid":"00000000-0000-0000-0000-000000000000"}"#,
        r#"{"$regularExpression":{"pattern":"a","options":""}}"#,
        r#"{"$timestamp":{"t":1,"i":1}}"#,
        r#"{"$code":"1"}"#,
        r#"{"$codeWScope":{}}"#,
        r#"{"$symbol":"a"}"#,
        r#"{"$minKey":1}"#,
        r#"{"$maxKey":1}"#,
        r#"{"$undefined":true}"#,
        r#"{"$dbPointer":{}}"#,
        r#"{"$numberInt":"1"}"#,
        r#"{"$numberDouble":"1"}"#,
    ] {
        for place in [
            format!(r#"{{"collection":"hosts","aggregate":[{{"$match":{{"a":{form}}}}}]}}"#),
            format!(r#"{{"collection":"hosts","aggregate":[{{"$addFields":{{"a":{form}}}}}]}}"#),
            format!(r#"{{"collection":"hosts","aggregate":[{{"$addFields":{{"a":{{"$literal":{form}}}}}}}]}}"#),
        ] {
            assert_eq!(save_codes(&place), [ErrorCode::NotPermitted], "{place}");
            let e = run_plain(&place).unwrap_err();
            assert_eq!(e.api_code(), "sql_invalid", "{place}");
        }
    }
}

#[test]
fn limits_at_the_bound_and_one_above() {
    // Depth: the top object has depth 1.
    let nest = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
    assert!(parse_text(&nest(MAX_QUERY_DEPTH)).is_ok());
    assert_eq!(kind(&nest(MAX_QUERY_DEPTH + 1)), JsonErrorKind::TooDeep);
    let objs = |n: usize| format!("{}1{}", "{\"a\":".repeat(n), "}".repeat(n));
    assert!(parse_text(&objs(MAX_QUERY_DEPTH)).is_ok());
    assert_eq!(kind(&objs(MAX_QUERY_DEPTH + 1)), JsonErrorKind::TooDeep);
    // A very deep text does not overflow the stack.
    assert_eq!(kind(&"[".repeat(100_000)), JsonErrorKind::TooDeep);
    // Key size.
    let key = |n: usize| format!("{{\"{}\":1}}", "k".repeat(n));
    assert!(parse_text(&key(MAX_QUERY_KEY_BYTES)).is_ok());
    assert_eq!(kind(&key(MAX_QUERY_KEY_BYTES + 1)), JsonErrorKind::TooLarge);
    // Two-byte characters count as bytes.
    let wide = |n: usize| format!("{{\"{}\":1}}", "\u{e9}".repeat(n));
    assert!(parse_text(&wide(128)).is_ok());
    assert_eq!(kind(&wide(129)), JsonErrorKind::TooLarge);
    // String size.
    let string = |n: usize| format!("\"{}\"", "s".repeat(n));
    assert!(parse_text(&string(MAX_QUERY_STRING_BYTES)).is_ok());
    assert_eq!(kind(&string(MAX_QUERY_STRING_BYTES + 1)), JsonErrorKind::TooLarge);
    // A string of escapes is counted after the decoding.
    assert!(parse_text(&format!("\"{}\"", "\\n".repeat(MAX_QUERY_STRING_BYTES))).is_ok());
    assert_eq!(kind(&format!("\"{}\"", "\\n".repeat(MAX_QUERY_STRING_BYTES + 1))), JsonErrorKind::TooLarge);
    // Array size.
    let array = |n: usize| format!("[{}]", vec!["0"; n].join(","));
    assert!(parse_text(&array(MAX_QUERY_ARRAY_ITEMS)).is_ok());
    assert_eq!(kind(&array(MAX_QUERY_ARRAY_ITEMS + 1)), JsonErrorKind::TooLarge);
    // Node count: the top array and its items.
    let nodes = |items: usize| {
        let row = format!("[{}]", vec!["0"; MAX_QUERY_ARRAY_ITEMS].join(","));
        let full = items / (MAX_QUERY_ARRAY_ITEMS + 1);
        format!("[{}]", vec![row; full].join(","))
    };
    // 9 rows of 10,000 items: 1 + 9 + 90,000 = 90,010 nodes.
    assert!(parse_text(&nodes(9 * (MAX_QUERY_ARRAY_ITEMS + 1))).is_ok());
    // 10 rows: 1 + 10 + 100,000 = 100,011 nodes.
    assert_eq!(kind(&nodes(10 * (MAX_QUERY_ARRAY_ITEMS + 1))), JsonErrorKind::TooLarge);
}

#[test]
fn limits_give_the_codes_of_the_save() {
    let deep = format!("{}{}", "[".repeat(33), "]".repeat(33));
    assert_eq!(save_codes(&deep), [ErrorCode::TooDeep]);
    let long =
        format!("{{\"collection\":\"hosts\",\"aggregate\":[{{\"$match\":{{\"a\":\"{}\"}}}}]}}", "x".repeat(16_385));
    assert_eq!(save_codes(&long), [ErrorCode::TooLarge]);
    let e = run_plain(&long).unwrap_err();
    assert_eq!(e.api_code(), "sql_invalid");
}

#[test]
fn the_writer_and_the_parser_agree() {
    let t = r#"{"collection":"hosts","aggregate":[{"$match":{"a":{"$gte":{"$date":"2026-11-09T14:00:00.250Z"},"$lt":{"$numberLong":"9"}},"b":[1,2.5,"x\n\u2028",null,true,{"$oid":"0123456789abcdef01234567"},{"$numberDecimal":"1.50"}]}}]}"#;
    let v = value(t);
    let again = to_query_json(&v);
    assert_eq!(value(&again), v);
    assert_eq!(to_query_json(&value(&again)), again);
    // A Date outside the ISO range uses $numberLong and reads back.
    for ms in [-62_135_596_800_001, 253_402_300_800_000, i64::from(i32::MIN), 0] {
        let d = MongoValue::DateMs(ms);
        assert_eq!(value(&to_query_json(&d)), d, "{ms}");
    }
}

#[test]
fn the_parser_never_panics_and_is_deterministic() {
    let mut x: u64 = 0x1234_5678_9abc_def0;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let alphabet: Vec<&str> = vec![
        "{",
        "}",
        "[",
        "]",
        ",",
        ":",
        "\"",
        "\\",
        "a",
        "1",
        "-",
        ".",
        "e",
        "$",
        "$date",
        "$oid",
        "$numberLong",
        "null",
        "true",
        " ",
        "\n",
        "\\u0000",
        "\\ud800",
        "\u{e9}",
        "\"$out\"",
        "\"a\":",
        "Z",
        "T",
    ];
    for _ in 0..20_000 {
        let n = (next() % 24) as usize;
        let s: String = (0..n).map(|_| alphabet[(next() % alphabet.len() as u64) as usize]).collect();
        let a = parse_text(&s);
        let b = parse_text(&s);
        assert_eq!(a.is_ok(), b.is_ok());
        if let (Ok(a), Ok(b)) = (a, b) {
            assert_eq!(a.value, b.value);
        }
    }
}
