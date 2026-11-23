//! The value checks and the conversions of 7.16 (second table and the three changes).

use hune_core::frame::{FrameBuilder, FrameStats, FrameValue, parse_date};

/// Builds a one-column frame with the rows given as raw JSON values.
fn one_column(ty: &str, rows: &[&str]) -> Result<hune_core::frame::Frame, String> {
    let mut b = FrameBuilder::new(vec!["c".into()], vec![ty.into()]).map_err(|e| e.to_string())?;
    for r in rows {
        b.push_row(&format!("[{r}]")).map_err(|e| e.to_string())?;
    }
    Ok(b.finish("q".into(), FrameStats::default()))
}

fn values(ty: &str, rows: &[&str]) -> Vec<String> {
    let f = one_column(ty, rows).unwrap();
    f.columns[0]
        .values()
        .map(|v| match v {
            FrameValue::Null => "null".to_string(),
            FrameValue::Bool(b) => b.to_string(),
            FrameValue::Number(n) => format!("#{n}"),
            FrameValue::Text(t) => format!("'{t}'"),
        })
        .collect()
}

fn refused(ty: &str, row: &str) -> String {
    one_column(ty, &[row]).expect_err(&format!("{ty} must refuse {row}"))
}

#[test]
fn number_accepts_numbers_specials_and_null() {
    assert_eq!(values("UInt32", &["42", "0", "null"]), ["#42", "#0", "null"]);
    assert_eq!(values("Int8", &["-5"]), ["#-5"]);
    assert_eq!(values("Float64", &["1.5", "-0.25", "1e-7", "2.5E+3"]), ["#1.5", "#-0.25", "#1e-7", "#2.5E+3"]);
    assert_eq!(values("Float32", &[r#""nan""#, r#""inf""#, r#""-inf""#]), ["'nan'", "'inf'", "'-inf'"]);
    assert_eq!(values("Nullable(BFloat16)", &["0.5", "null"]), ["#0.5", "null"]);
}

#[test]
fn number_refuses_other_forms() {
    for bad in [r#""12""#, r#""NaN""#, r#""Inf""#, "true", "[1]", "{}"] {
        refused("UInt32", bad);
    }
}

#[test]
fn number_text_accepts_exact_texts() {
    assert_eq!(
        values("UInt64", &[r#""18446744073709551615""#, r#""0""#, "null"]),
        ["'18446744073709551615'", "'0'", "null"]
    );
    assert_eq!(
        values("Int128", &[r#""-170141183460469231731687303715884105728""#]),
        ["'-170141183460469231731687303715884105728'"]
    );
    assert_eq!(
        values("Decimal(38, 9)", &[r#""12.500000000""#, r#""-3.000000000""#]),
        ["'12.500000000'", "'-3.000000000'"]
    );
    assert_eq!(values("Decimal(10, 0)", &[r#""7""#]), ["'7'"]);
    assert_eq!(values("Decimal(10, 2)", &[r#""007.10""#]), ["'007.10'"]);
}

#[test]
fn number_text_refuses_numbers_and_bad_texts() {
    for bad in ["12", r#""""#, r#""-""#, r#""1.5""#, r#""abc""#, r#""1e5""#, r#"" 1""#, r#""+1""#, "true"] {
        refused("UInt64", bad);
    }
    for bad in ["1.5", r#""1.""#, r#"".5""#, r#""1.2.3""#, r#""1,5""#, r#""--1""#, r#""""#] {
        refused("Decimal(10, 2)", bad);
    }
}

#[test]
fn time_text_and_number_give_the_same_digits() {
    assert_eq!(values("DateTime", &[r#""1794211200""#, "1794211201", "null"]), ["#1794211200", "#1794211201", "null"]);
    assert_eq!(values("DateTime('UTC')", &[r#""-86400""#]), ["#-86400"]);
    assert_eq!(values("DateTime64(3)", &[r#""1794211200.125""#, "1794211200.5"]), ["#1794211200.125", "#1794211200.5"]);
    assert_eq!(values("DateTime64(9)", &[r#""1794211200.123456789""#]), ["#1794211200.123456789"]);
    assert_eq!(values("DateTime64(3)", &[r#""0.001""#]), ["#0.001"]);
}

#[test]
fn time_refuses_bad_forms() {
    for bad in [
        r#""2026-11-09 00:00:00""#,
        r#""1794211200.1234567890""#,
        r#""1794211200.""#,
        r#"".5""#,
        "1.5e9",
        r#""abc""#,
        r#""""#,
        "true",
        // A leading zero would make the JSON output invalid.
        r#""0123""#,
    ] {
        refused("DateTime", bad);
        refused("DateTime64(3)", bad);
    }
}

#[test]
fn date_gives_unix_seconds() {
    assert_eq!(values("Date", &[r#""2026-11-09""#]), ["#1794182400"]);
    assert_eq!(values("Date", &[r#""1970-01-01""#]), ["#0"]);
    assert_eq!(values("Date", &[r#""1970-01-02""#, "null"]), ["#86400", "null"]);
    assert_eq!(values("Date32", &[r#""1900-01-01""#]), ["#-2208988800"]);
    assert_eq!(values("Date32", &[r#""2299-12-31""#]), ["#10413705600"]);
    assert_eq!(values("Nullable(Date)", &[r#""2024-02-29""#]), ["#1709164800"]);
    assert_eq!(parse_date("2026-11-09"), Some(1_794_182_400));
}

#[test]
fn date_refuses_unreal_dates_and_other_forms() {
    for bad in [
        r#""2026-02-30""#,
        r#""2025-02-29""#,
        r#""2026-13-01""#,
        r#""2026-00-10""#,
        r#""2026-01-00""#,
        r#""2026-1-1""#,
        r#""26-01-01""#,
        r#""2026/01/01""#,
        r#""2026-01-01 00:00:00""#,
        r#""abcd-ef-gh""#,
        "20260101",
        "true",
    ] {
        refused("Date", bad);
        refused("Date32", bad);
    }
    assert_eq!(parse_date("2100-02-29"), None);
    assert_eq!(parse_date("2000-02-29"), Some(951_782_400));
}

#[test]
fn bool_accepts_booleans_only() {
    assert_eq!(values("Bool", &["true", "false", "null"]), ["true", "false", "null"]);
    for bad in [r#""true""#, "1", "0", "[true]"] {
        refused("Bool", bad);
    }
}

#[test]
fn text_takes_strings_and_compact_json() {
    assert_eq!(values("String", &[r#""a1""#, "null", r#""""#]), ["'a1'", "null", "''"]);
    assert_eq!(values("LowCardinality(String)", &[r#""x\ny\"z""#]), ["'x\ny\"z'"]);
    assert_eq!(values("String", &[r#""é\u0000""#]), ["'é\u{0}'"]);
    assert_eq!(values("Array(UInt8)", &["[1,2,3]", "[]"]), ["'[1,2,3]'", "'[]'"]);
    assert_eq!(values("Map(String, UInt8)", &[r#"{"b":1,"a":2}"#]), [r#"'{"b":1,"a":2}'"#]);
    assert_eq!(values("Tuple(a UInt8, b String)", &[r#"[1,"x"]"#]), [r#"'[1,"x"]'"#]);
    assert_eq!(values("JSON", &["42", "true"]), ["'42'", "'true'"]);
    // A string inside an array is kept with its quotes. White space outside strings goes away.
    assert_eq!(values("Array(String)", &[r#"[ "a b" , 1 ]"#]), [r#"'["a b",1]'"#]);
    assert_eq!(values("Array(String)", &[r#"["x, y","]"]"#]), [r#"'["x, y","]"]'"#]);
}

#[test]
fn null_kind_takes_null_only() {
    assert_eq!(values("Nothing", &["null", "null"]), ["null", "null"]);
    refused("Nothing", "1");
    refused("Nothing", r#""a""#);
}

#[test]
fn a_value_error_names_the_column_and_never_the_value() {
    let e = refused("UInt64", r#""secret-row-data""#);
    assert!(e.contains("column 1"), "{e}");
    assert!(e.contains("UInt64"), "{e}");
    assert!(!e.contains("secret-row-data"), "{e}");
}

#[test]
fn row_shape_errors() {
    let mut b = FrameBuilder::new(vec!["a".into(), "b".into()], vec!["UInt8".into(), "String".into()]).unwrap();
    assert!(b.push_row(r#"[1,"x"]"#).is_ok());
    assert!(b.push_row(r#"[1]"#).is_err());
    assert!(b.push_row(r#"[1,"x",3]"#).is_err());
    assert!(b.push_row(r#"[]"#).is_err());
    assert!(b.push_row(r#"{"a":1}"#).is_err());
    assert!(b.push_row(r#"1,2"#).is_err());
    assert!(b.push_row(r#"[1,"x"] x"#).is_err());
    assert!(b.push_row("").is_err());
    assert!(b.push_row("[1,").is_err());
    assert_eq!(b.rows(), 1);
}

#[test]
fn row_with_spaces_between_values() {
    let mut b = FrameBuilder::new(vec!["a".into(), "b".into()], vec!["UInt8".into(), "String".into()]).unwrap();
    b.push_row(r#" [ 1 , "x" ] "#).unwrap();
    let f = b.finish("q".into(), FrameStats::default());
    assert_eq!(f.columns[0].value(0), Some(FrameValue::Number("1")));
    assert_eq!(f.columns[1].value(0), Some(FrameValue::Text("x")));
}

#[test]
fn header_errors() {
    assert!(FrameBuilder::new(vec![], vec![]).is_err());
    assert!(FrameBuilder::new(vec!["a".into()], vec![]).is_err());
    assert!(FrameBuilder::from_header_lines(r#"["a","b"]"#, r#"["UInt8","String"]"#).is_ok());
    assert!(FrameBuilder::from_header_lines(r#"["a","b"]"#, r#"["UInt8"]"#).is_err());
    assert!(FrameBuilder::from_header_lines(r#"[1]"#, r#"["UInt8"]"#).is_err());
    assert!(FrameBuilder::from_header_lines(r#"["a"]"#, r#"{"x":1}"#).is_err());
    assert!(FrameBuilder::from_header_lines("", "").is_err());
    assert!(FrameBuilder::from_header_lines("[]", "[]").is_err());
}

#[test]
fn nulls_are_kept_in_every_column_kind() {
    let mut b = FrameBuilder::new(
        vec!["s".into(), "n".into(), "b".into(), "z".into()],
        vec!["String".into(), "UInt8".into(), "Bool".into(), "Nothing".into()],
    )
    .unwrap();
    b.push_row(r#"["a",1,true,null]"#).unwrap();
    b.push_row(r#"[null,null,null,null]"#).unwrap();
    b.push_row(r#"["",0,false,null]"#).unwrap();
    let f = b.finish("q".into(), FrameStats::default());
    let col = |i: usize| f.columns[i].values().collect::<Vec<_>>();
    assert_eq!(col(0), [FrameValue::Text("a"), FrameValue::Null, FrameValue::Text("")]);
    assert_eq!(col(1), [FrameValue::Number("1"), FrameValue::Null, FrameValue::Number("0")]);
    assert_eq!(col(2), [FrameValue::Bool(true), FrameValue::Null, FrameValue::Bool(false)]);
    assert_eq!(col(3), [FrameValue::Null, FrameValue::Null, FrameValue::Null]);
    assert_eq!(f.rows, 3);
    assert_eq!(f.columns[0].value(3), None);
}

#[test]
fn an_empty_result_has_columns_and_no_row() {
    let b = FrameBuilder::new(vec!["a".into()], vec!["UInt8".into()]).unwrap();
    let f = b.finish("q".into(), FrameStats::default());
    assert_eq!(f.rows, 0);
    assert!(f.columns[0].is_empty());
}
