// SPDX-License-Identifier: MIT OR Apache-2.0
//! The frame of a PostgreSQL result (7.16, 7.16a, 7.16b, 8a.6).
//!
//! This module reads the column types of the Describe answer (8a.5, step 6). It refuses a type
//! that 7.16a does not list ([`classify_columns`], then `422 query_error` with `sqlstate = null`),
//! before the query runs. It then builds the frame row by row with a [`PgFrameBuilder`]. It counts
//! the rows and the bytes (the JSON text of each value plus 1) and stops at a cap (8a.6). The
//! members `read_rows` and `read_bytes` of `stats` are `null` (7.16a). `result_bytes` is the count.
//!
//! The builder reuses the `FrameBuilder` of `hune-core` (7.16). It maps each PostgreSQL type to a
//! ClickHouse type text of the same kind and value form, only to select the checks of 7.16. The
//! member `type` of each column is then set to the PostgreSQL name (7.16a). So the frame of both
//! kinds has one writer and one set of value checks.
//!
//! [`apply_time_series_rule`] is the rule of 7.16b. The server calls it for a query of a
//! `postgres` source whose format is `time_series` (7.2, step 15a), not the options query.
//!
//! The functions do no I/O and read no clock.

use hune_core::frame::{Frame, FrameBuilder, FrameStats, FrameValue, Kind};

use crate::postgres::decode::{ColumnType, DecodeError, Scalar, decode, raw_values};

/// A column of a type that 7.16a does not list. The error is `422 query_error` with
/// `sqlstate = null`, built by `errors::unsupported_type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnsupportedColumn {
    /// The column name.
    pub column: String,
    /// The name of the type: `pg_type.typname`, or the element name and `[]`.
    pub type_name: String,
}

/// Finds the entry of the closed list for each column of the Describe answer.
///
/// # Errors
///
/// The first column whose type is not in the list. The caller stops the query here.
pub(crate) fn classify_columns(
    columns: &[tokio_postgres::Column],
) -> Result<Vec<(String, ColumnType)>, UnsupportedColumn> {
    classify(columns.iter().map(|c| (c.name(), c.type_())))
}

/// As [`classify_columns`], for names and types.
pub(crate) fn classify<'a>(
    columns: impl IntoIterator<Item = (&'a str, &'a tokio_postgres::types::Type)>,
) -> Result<Vec<(String, ColumnType)>, UnsupportedColumn> {
    columns
        .into_iter()
        .map(|(name, ty)| {
            ColumnType::from_pg(ty)
                .map(|ct| (name.to_owned(), ct))
                .map_err(|u| UnsupportedColumn { column: name.to_owned(), type_name: u.name })
        })
        .collect()
}

/// The caps of one query (8a.6), from the effective limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Caps {
    /// The effective `max_result_rows`.
    pub max_rows: u64,
    /// The key `max_result_bytes`.
    pub max_bytes: u64,
}

/// The reason why a row cannot join the frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Fault {
    /// The row is above `max_result_rows`: `422 query_limit`, or `403 ceiling_exceeded`.
    Rows,
    /// The count of bytes is above `max_result_bytes`: `422 query_limit`.
    Bytes,
    /// A value does not decode, or a row has a wrong shape: `502 source_bad_response`.
    /// The text never holds a value of a row.
    BadResponse(String),
}

impl From<DecodeError> for Fault {
    fn from(e: DecodeError) -> Fault {
        Fault::BadResponse(e.message().to_owned())
    }
}

/// The ClickHouse type text that gives the kind and the value checks of a PostgreSQL type.
fn proxy_type(ty: &ColumnType) -> &'static str {
    if ty.array {
        return "String";
    }
    match ty.scalar {
        Scalar::Bool => "Bool",
        Scalar::Int2 | Scalar::Int4 | Scalar::Oid => "Int32",
        Scalar::Int8 => "Int64",
        Scalar::Float4 | Scalar::Float8 => "Float64",
        Scalar::Numeric => "Decimal(76, 0)",
        // The decode gives Unix seconds with up to 6 decimals, for the three types.
        Scalar::Date | Scalar::Timestamp | Scalar::Timestamptz => "DateTime64(6)",
        Scalar::Text
        | Scalar::Uuid
        | Scalar::Json
        | Scalar::Jsonb
        | Scalar::Inet
        | Scalar::Cidr
        | Scalar::Enum
        | Scalar::Time => "String",
    }
}

/// Builds the frame of one query, row by row, and counts rows and bytes (8a.6).
#[derive(Debug)]
pub(crate) struct PgFrameBuilder {
    types: Vec<ColumnType>,
    inner: FrameBuilder,
    caps: Caps,
    rows: u64,
    bytes: u64,
    line: String,
}

impl PgFrameBuilder {
    /// Makes a builder for the columns of the Describe answer.
    ///
    /// # Errors
    ///
    /// The answer has no column.
    pub(crate) fn new(columns: Vec<(String, ColumnType)>, caps: Caps) -> Result<PgFrameBuilder, Fault> {
        let names: Vec<String> = columns.iter().map(|(n, _)| n.clone()).collect();
        let types: Vec<ColumnType> = columns.into_iter().map(|(_, t)| t).collect();
        let proxies = types.iter().map(|t| proxy_type(t).to_owned()).collect();
        let inner =
            FrameBuilder::new(names.clone(), proxies).map_err(|e| Fault::BadResponse(e.message().to_owned()))?;
        Ok(PgFrameBuilder { types, inner, caps, rows: 0, bytes: 0, line: String::new() })
    }

    /// Gives the number of rows that the builder holds.
    pub(crate) fn rows(&self) -> u64 {
        self.rows
    }

    /// Gives the count of bytes of 8a.6: for each value, its JSON length plus 1.
    #[allow(dead_code, reason = "The tests read the count; the frame carries it in `stats`.")]
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Decodes one row of the binary form and adds it to the frame.
    ///
    /// The row cap is exact: the row after `max_rows` rows gives [`Fault::Rows`], before it is
    /// decoded. The byte count is checked after each value, so the row stops at the first value
    /// that brings the count above `max_result_bytes` (the count after the row would be above it
    /// too). A builder that gave a fault MUST NOT be used again.
    ///
    /// # Errors
    ///
    /// A cap, a value that does not decode, or a row of a wrong shape.
    pub(crate) fn push_row(&mut self, values: &[Option<&[u8]>]) -> Result<(), Fault> {
        if self.rows >= self.caps.max_rows {
            return Err(Fault::Rows);
        }
        if values.len() != self.types.len() {
            return Err(Fault::BadResponse(format!(
                "A row has {} values and the description has {} columns.",
                values.len(),
                self.types.len()
            )));
        }
        let mut line = std::mem::take(&mut self.line);
        line.clear();
        line.push('[');
        let mut bytes = self.bytes;
        for (i, (ty, raw)) in self.types.iter().zip(values).enumerate() {
            let v = decode(ty, *raw)?;
            bytes = bytes.saturating_add(v.json_len() as u64 + 1);
            if bytes > self.caps.max_bytes {
                self.line = line;
                return Err(Fault::Bytes);
            }
            if i > 0 {
                line.push(',');
            }
            v.write_json(&mut line);
        }
        line.push(']');
        let pushed = self.inner.push_row(&line);
        self.line = line;
        pushed.map_err(|e| Fault::BadResponse(e.message().to_owned()))?;
        self.rows += 1;
        self.bytes = bytes;
        Ok(())
    }

    /// Decodes one `tokio-postgres` row and adds it. See [`PgFrameBuilder::push_row`].
    ///
    /// # Errors
    ///
    /// As `push_row`, or a row that the client cannot read.
    pub(crate) fn push_pg_row(&mut self, row: &tokio_postgres::Row) -> Result<(), Fault> {
        let raw = raw_values(row)?;
        self.push_row(&raw)
    }

    /// Ends the frame. The member `type` of each column is the PostgreSQL name (7.16a).
    pub(crate) fn finish(self, query_id: String, elapsed_ms: u64) -> Frame {
        let stats = FrameStats { elapsed_ms, read_rows: None, read_bytes: None, result_bytes: Some(self.bytes) };
        let mut frame = self.inner.finish(query_id, stats);
        for (col, ty) in frame.columns.iter_mut().zip(&self.types) {
            col.type_text = ty.name.clone();
        }
        frame
    }
}

/// Tells if `s` matches `^-?[0-9]+(\.[0-9]{1,9})?$` and its integer part has no leading zero.
fn is_time_text(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|c| c.is_ascii_digit());
    digits(int) && (int.len() == 1 || !int.starts_with('0')) && frac.is_none_or(|f| digits(f) && f.len() <= 9)
}

/// Applies the rule of 7.16b to the frame of a `time_series` query. Gives true when it changed
/// the frame.
///
/// The rule applies when the frame has no column of the kind `time`, the first column is named
/// `time` (lower case) and has the kind `number` or `number_text`, and each value is `null`, a
/// finite JSON number, or a text that matches `^-?[0-9]+(\.[0-9]{1,9})?$`. The kind of the
/// column then becomes `time`. The values and the member `type` do not change. Else the frame
/// does not change, and the page shows the error of 9.22.
pub fn apply_time_series_rule(frame: &mut Frame) -> bool {
    if frame.columns.iter().any(|c| c.kind == Kind::Time) {
        return false;
    }
    let Some(first) = frame.columns.first() else {
        return false;
    };
    if first.name != "time" || !matches!(first.kind, Kind::Number | Kind::NumberText) {
        return false;
    }
    let ok = first.values().all(|v| match (first.kind, v) {
        (_, FrameValue::Null) => true,
        // A `number` value that is not a digit text is `nan`, `inf` or `-inf`: `value` gives Text.
        (Kind::Number, FrameValue::Number(_)) => true,
        (Kind::NumberText, FrameValue::Text(s)) => is_time_text(s),
        _ => false,
    });
    if ok {
        frame.columns[0].kind = Kind::Time;
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_postgres::types::{Kind as PgKind, Type};

    fn caps(rows: u64, bytes: u64) -> Caps {
        Caps { max_rows: rows, max_bytes: bytes }
    }

    fn builder(cols: &[(&str, Type)], caps: Caps) -> PgFrameBuilder {
        let classified = classify(cols.iter().map(|(n, t)| (*n, t))).expect("listed");
        PgFrameBuilder::new(classified, caps).expect("builder")
    }

    fn numeric(ndigits: u16, weight: i16, sign: u16, dscale: u16, groups: &[u16]) -> Vec<u8> {
        let mut v = Vec::new();
        for x in [ndigits, weight as u16, sign, dscale] {
            v.extend_from_slice(&x.to_be_bytes());
        }
        for g in groups {
            v.extend_from_slice(&g.to_be_bytes());
        }
        v
    }

    fn us_2000(unix_us: i64) -> Vec<u8> {
        (unix_us - 946_684_800_000_000).to_be_bytes().to_vec()
    }

    #[test]
    fn frame_of_each_kind() {
        let mut b = builder(
            &[
                ("t", Type::TIMESTAMPTZ),
                ("label", Type::TEXT),
                ("weight", Type::NUMERIC),
                ("blocks", Type::INT4),
                ("big", Type::INT8),
                ("ratio", Type::FLOAT8),
                ("active", Type::BOOL),
                ("day", Type::DATE),
            ],
            caps(10, 10_000),
        );
        let row1: Vec<Option<Vec<u8>>> = vec![
            Some(us_2000(1_794_211_200_125_000)),
            Some(b"a1".to_vec()),
            Some(numeric(2, 0, 0, 9, &[12, 5000])),
            Some(42i32.to_be_bytes().to_vec()),
            Some(i64::MAX.to_be_bytes().to_vec()),
            Some(f64::NAN.to_be_bytes().to_vec()),
            Some(vec![1]),
            Some(((1_794_182_400i64 / 86_400 - 10_957) as i32).to_be_bytes().to_vec()),
        ];
        let row2: Vec<Option<Vec<u8>>> = vec![None; 8];
        for r in [&row1, &row2] {
            let refs: Vec<Option<&[u8]>> = r.iter().map(|o| o.as_deref()).collect();
            b.push_row(&refs).unwrap();
        }
        assert_eq!(b.rows(), 2);
        let f = b.finish("q1".into(), 7);
        let kinds: Vec<_> = f.columns.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, ["time", "text", "number_text", "number", "number_text", "number", "bool", "time"]);
        let types: Vec<_> = f.columns.iter().map(|c| c.type_text.as_str()).collect();
        assert_eq!(types, ["timestamptz", "text", "numeric", "int4", "int8", "float8", "bool", "date"]);
        let json = f.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            v["values"],
            serde_json::json!([
                [1794211200.125, null],
                ["a1", null],
                ["12.500000000", null],
                [42, null],
                ["9223372036854775807", null],
                ["nan", null],
                [true, null],
                [1794182400, null]
            ])
        );
        assert_eq!(v["rows"], 2);
        assert_eq!(v["stats"]["read_rows"], serde_json::Value::Null);
        assert_eq!(v["stats"]["read_bytes"], serde_json::Value::Null);
        assert_eq!(v["stats"]["elapsed_ms"], 7);
        // The text of the number is exact: no loss through the JSON value.
        assert!(json.contains("[1794211200.125,null]"));
        assert!(json.contains(r#""kind":"time""#));
        assert_eq!(v["stats"]["result_bytes"], f.stats.result_bytes.unwrap());
    }

    #[test]
    fn the_unsupported_column_is_named_before_the_run() {
        let err = classify([("id", &Type::INT4), ("payload", &Type::BYTEA)]).unwrap_err();
        assert_eq!(err, UnsupportedColumn { column: "payload".into(), type_name: "bytea".into() });
        let err = classify([("tags", &Type::JSONB_ARRAY)]).unwrap_err();
        assert_eq!(err.type_name, "jsonb[]");
        for ty in [Type::INTERVAL, Type::MONEY, Type::XML, Type::INT4_RANGE, Type::POINT] {
            assert!(classify([("c", &ty)]).is_err());
        }
        let comp = Type::new("pair".into(), 30_000, PgKind::Composite(vec![]), "public".into());
        assert_eq!(classify([("c", &comp)]).unwrap_err().type_name, "pair");
    }

    #[test]
    fn row_cap_is_exact() {
        let mut b = builder(&[("n", Type::INT4)], caps(3, 1_000_000));
        let one = 1i32.to_be_bytes();
        for _ in 0..3 {
            b.push_row(&[Some(&one)]).unwrap();
        }
        // The fourth row is the overflow.
        assert_eq!(b.push_row(&[Some(&one)]), Err(Fault::Rows));
        assert_eq!(b.rows(), 3);
        // A cap of 0 refuses the first row.
        let mut z = builder(&[("n", Type::INT4)], caps(0, 100));
        assert_eq!(z.push_row(&[Some(&one)]), Err(Fault::Rows));
    }

    #[test]
    fn byte_cap_counts_json_length_plus_one() {
        // Each row: "abcd" is 6 bytes plus 1, `42` is 2 bytes plus 1: 10 bytes.
        let two = 42i32.to_be_bytes();
        let mut b = builder(&[("s", Type::TEXT), ("n", Type::INT4)], caps(100, 20));
        b.push_row(&[Some(b"abcd"), Some(&two)]).unwrap();
        assert_eq!(b.bytes(), 10);
        // The count of 20 is not above the cap.
        b.push_row(&[Some(b"abcd"), Some(&two)]).unwrap();
        assert_eq!(b.bytes(), 20);
        assert_eq!(b.push_row(&[Some(b"x"), Some(&two)]), Err(Fault::Bytes));
        assert_eq!(b.rows(), 2);
        // NULL counts 4 + 1, `true` 4 + 1, `false` 5 + 1.
        let mut n = builder(&[("a", Type::TEXT), ("b", Type::BOOL), ("c", Type::BOOL)], caps(10, 1000));
        n.push_row(&[None, Some(&[1]), Some(&[0])]).unwrap();
        assert_eq!(n.bytes(), 5 + 5 + 6);
        // Escapes count: a quote mark is 2 bytes in JSON.
        let mut e = builder(&[("s", Type::TEXT)], caps(10, 1000));
        e.push_row(&[Some(b"\"\n")]).unwrap();
        assert_eq!(e.bytes(), 2 + 4 + 1);
        assert_eq!(e.finish("q".into(), 0).stats.result_bytes, Some(7));
    }

    #[test]
    fn a_large_value_stops_the_row_at_once() {
        let mut b = builder(&[("s", Type::TEXT), ("t", Type::TEXT)], caps(10, 50));
        let big = vec![b'x'; 10_000];
        assert_eq!(b.push_row(&[Some(&big), Some(b"a")]), Err(Fault::Bytes));
        assert_eq!(b.rows(), 0);
        assert_eq!(b.bytes(), 0);
    }

    #[test]
    fn bad_values_and_bad_rows_are_bad_responses() {
        let mut b = builder(&[("n", Type::INT4), ("m", Type::INT4)], caps(10, 1000));
        let one = 1i32.to_be_bytes();
        let Err(Fault::BadResponse(m)) = b.push_row(&[Some(&one)]) else { panic!("shape") };
        assert!(m.contains("1 values") && m.contains("2 columns"));
        let Err(Fault::BadResponse(m)) = b.push_row(&[Some(&one), Some(&[1, 2, 3])]) else { panic!("value") };
        assert_eq!(m, "A value of the type int4 does not decode: the length is wrong.");
        // A NaN numeric is null, not an error.
        let mut b = builder(&[("x", Type::NUMERIC)], caps(10, 1000));
        b.push_row(&[Some(&numeric(0, 0, 0xC000, 0, &[]))]).unwrap();
        let f = b.finish("q".into(), 0);
        assert!(f.to_json().contains(r#""values":[[null]]"#));
        // No column.
        assert!(PgFrameBuilder::new(vec![], caps(1, 1)).is_err());
    }

    #[test]
    fn arrays_and_enums_are_text() {
        let color = Type::new("color".into(), 30_001, PgKind::Enum(vec!["red".into()]), "public".into());
        let mut b = builder(&[("a", Type::INT4_ARRAY), ("c", color)], caps(10, 1000));
        let mut arr = Vec::new();
        arr.extend_from_slice(&1i32.to_be_bytes());
        arr.extend_from_slice(&0i32.to_be_bytes());
        arr.extend_from_slice(&23u32.to_be_bytes());
        arr.extend_from_slice(&1i32.to_be_bytes());
        arr.extend_from_slice(&1i32.to_be_bytes());
        arr.extend_from_slice(&4i32.to_be_bytes());
        arr.extend_from_slice(&7i32.to_be_bytes());
        b.push_row(&[Some(&arr), Some(b"red")]).unwrap();
        let f = b.finish("q".into(), 0);
        assert_eq!(f.columns[0].type_text, "int4[]");
        assert_eq!(f.columns[0].kind, Kind::Text);
        assert_eq!(f.columns[1].type_text, "color");
        let v: serde_json::Value = serde_json::from_str(&f.to_json()).unwrap();
        assert_eq!(v["values"], serde_json::json!([["[7]"], ["red"]]));
    }

    /// A frame with one column named `name` of the type `ty`, and the given raw values.
    fn frame_of(name: &str, ty: Type, raws: &[Option<Vec<u8>>]) -> Frame {
        let mut b = builder(&[(name, ty)], caps(1000, 1_000_000));
        for r in raws {
            b.push_row(&[r.as_deref()]).unwrap();
        }
        b.finish("q".into(), 0)
    }

    #[test]
    fn time_rule_changes_a_number_text_column() {
        // `numeric` gives a text; `int8` gives a text.
        let mut f = frame_of(
            "time",
            Type::INT8,
            &[Some(1_794_211_200i64.to_be_bytes().to_vec()), None, Some(0i64.to_be_bytes().to_vec())],
        );
        assert!(apply_time_series_rule(&mut f));
        assert_eq!(f.columns[0].kind, Kind::Time);
        assert_eq!(f.columns[0].type_text, "int8");
        assert!(f.to_json().contains(r#""values":[[1794211200,null,0]]"#));
        let mut g = frame_of("time", Type::NUMERIC, &[Some(numeric(3, 2, 0, 3, &[17, 9421, 1200]))]);
        // 17 9421 1200 . = 17 94211200.000 with weight 2: 17*10^8 + ... check the text first.
        let text = g.to_json();
        assert!(text.contains("\"kind\":\"number_text\""));
        assert!(apply_time_series_rule(&mut g));
        assert!(g.to_json().contains("\"kind\":\"time\""));
    }

    #[test]
    fn time_rule_changes_a_number_column() {
        let mut f = frame_of("time", Type::FLOAT8, &[Some(1_794_211_200.5f64.to_be_bytes().to_vec())]);
        assert!(apply_time_series_rule(&mut f));
        assert!(f.to_json().contains(r#""values":[[1794211200.5]]"#));
        let mut i = frame_of("time", Type::INT4, &[Some(100i32.to_be_bytes().to_vec()), None]);
        assert!(apply_time_series_rule(&mut i));
    }

    #[test]
    fn time_rule_refuses_what_it_must() {
        // A name in another case, or another name.
        for name in ["Time", "TIME", "t", "time "] {
            let mut f = frame_of(name, Type::INT4, &[Some(1i32.to_be_bytes().to_vec())]);
            assert!(!apply_time_series_rule(&mut f), "{name}");
            assert_eq!(f.columns[0].kind, Kind::Number);
        }
        // A NaN or an infinity.
        let mut f = frame_of("time", Type::FLOAT8, &[Some(f64::NAN.to_be_bytes().to_vec())]);
        assert!(!apply_time_series_rule(&mut f));
        let mut f = frame_of("time", Type::FLOAT8, &[Some(f64::INFINITY.to_be_bytes().to_vec())]);
        assert!(!apply_time_series_rule(&mut f));
        // A text column, a bool column.
        let mut f = frame_of("time", Type::TEXT, &[Some(b"123".to_vec())]);
        assert!(!apply_time_series_rule(&mut f));
        assert_eq!(f.columns[0].kind, Kind::Text);
        // A numeric with more than 9 decimals.
        let mut f = frame_of("time", Type::NUMERIC, &[Some(numeric(3, 0, 0, 10, &[1, 0, 0]))]);
        assert!(!apply_time_series_rule(&mut f));
        // A column that is already of the kind time: the frame has one, no change.
        let mut f = frame_of("time", Type::TIMESTAMPTZ, &[Some(us_2000(1_000_000))]);
        assert!(!apply_time_series_rule(&mut f));
        // Another column of the kind time.
        let mut b = builder(&[("time", Type::INT4), ("ts", Type::TIMESTAMPTZ)], caps(10, 1000));
        b.push_row(&[Some(&1i32.to_be_bytes()), Some(&us_2000(0))]).unwrap();
        let mut f = b.finish("q".into(), 0);
        assert!(!apply_time_series_rule(&mut f));
        assert_eq!(f.columns[0].kind, Kind::Number);
        // The time column must be the first.
        let mut b = builder(&[("v", Type::INT4), ("time", Type::INT4)], caps(10, 1000));
        b.push_row(&[Some(&1i32.to_be_bytes()), Some(&2i32.to_be_bytes())]).unwrap();
        let mut f = b.finish("q".into(), 0);
        assert!(!apply_time_series_rule(&mut f));
    }

    #[test]
    fn time_text_rule() {
        for ok in ["0", "-1", "12", "1794211200.125", "1.123456789", "-0.5"] {
            assert!(is_time_text(ok), "{ok}");
        }
        for bad in ["", "-", "1.", ".5", "1.1234567890", "01", "1e9", "+1", "1,5", "nan", "1.2.3", " 1"] {
            assert!(!is_time_text(bad), "{bad}");
        }
    }

    // ---- The frame of an annotation query on a `postgres` source (7.37) ----

    /// The binary form of a one-dimension `text[]` (element type 25). `None` is a NULL element.
    fn text_array(items: &[Option<&str>]) -> Vec<u8> {
        let mut v = Vec::new();
        for x in [1i32, i32::from(items.iter().any(Option::is_none))] {
            v.extend_from_slice(&x.to_be_bytes());
        }
        v.extend_from_slice(&25u32.to_be_bytes());
        v.extend_from_slice(&(items.len() as i32).to_be_bytes());
        v.extend_from_slice(&1i32.to_be_bytes());
        for item in items {
            match item {
                Some(s) => {
                    v.extend_from_slice(&(s.len() as i32).to_be_bytes());
                    v.extend_from_slice(s.as_bytes());
                }
                None => v.extend_from_slice(&(-1i32).to_be_bytes()),
            }
        }
        v
    }

    /// One mark as a tuple: the time, the end, the title, the text and the tags.
    type MarkRow<'a> = (i64, Option<i64>, &'a str, &'a str, Vec<String>);

    /// The reason and the row of a fault, or the time of the first mark.
    type Outcome = Result<i64, (&'static str, Option<usize>)>;

    /// The marks of the frame, or the reason and the row of the fault.
    fn marks_of(f: &Frame) -> Result<Vec<hune_core::annotation::Mark>, (&'static str, Option<usize>)> {
        hune_core::annotation::annotations_from_frame(f).map_err(|e| (e.reason.as_str(), e.row))
    }

    #[test]
    fn annotation_marks_from_timestamptz_text_and_text_arrays() {
        let mut b = builder(
            &[
                ("time", Type::TIMESTAMPTZ),
                ("timeEnd", Type::TIMESTAMPTZ),
                ("title", Type::TEXT),
                ("text", Type::VARCHAR),
                ("tags", Type::TEXT_ARRAY),
            ],
            caps(10, 10_000),
        );
        let rows: [[Option<Vec<u8>>; 5]; 3] = [
            // The microseconds below the millisecond are cut (rule 4). A NULL end is a point.
            [
                Some(us_2000(1_794_211_200_125_456)),
                None,
                Some(b"Deploy".to_vec()),
                Some(b"v1.2".to_vec()),
                Some(text_array(&[Some("prod"), Some(" deploy "), Some("prod")])),
            ],
            // A region, a NULL text and NULL tags.
            [
                Some(us_2000(1_794_214_800_000_000)),
                Some(us_2000(1_794_216_600_500_000)),
                Some(b"Maintenance".to_vec()),
                None,
                None,
            ],
            // The end `infinity` is NULL in the frame (7.16a): the mark is a point.
            [
                Some(us_2000(1_794_211_100_000_000)),
                Some(i64::MAX.to_be_bytes().to_vec()),
                None,
                None,
                Some(text_array(&[])),
            ],
        ];
        for r in &rows {
            let refs: Vec<Option<&[u8]>> = r.iter().map(|o| o.as_deref()).collect();
            b.push_row(&refs).unwrap();
        }
        let f = b.finish("q".into(), 0);
        let marks = marks_of(&f).unwrap();
        let got: Vec<MarkRow<'_>> =
            marks.iter().map(|m| (m.time_ms, m.end_ms, m.title.as_str(), m.text.as_str(), m.tags.clone())).collect();
        assert_eq!(
            got,
            vec![
                (1_794_211_100_000, None, "", "", vec![]),
                (1_794_211_200_125, None, "Deploy", "v1.2", vec!["prod".to_owned(), "deploy".to_owned()]),
                (1_794_214_800_000, Some(1_794_216_600_500), "Maintenance", "", vec![]),
            ]
        );
        // An array with a NULL element is not an array of texts: its JSON text is split at each comma (rule 7).
        let f = {
            let mut b = builder(&[("time", Type::INT8), ("tags", Type::TEXT_ARRAY)], caps(10, 1000));
            b.push_row(&[Some(&5i64.to_be_bytes()), Some(&text_array(&[Some("a"), None]))]).unwrap();
            b.finish("q".into(), 0)
        };
        assert_eq!(marks_of(&f).unwrap()[0].tags, ["[\"a\"", "null]"]);
    }

    #[test]
    fn annotation_numbers_are_whole_epoch_milliseconds_and_other_types_are_refused() {
        // int2, int4, int8, numeric and float8 give `number` or `number_text`: whole epoch milliseconds (rule 4).
        let cases: Vec<(Type, Vec<u8>, Outcome)> = vec![
            (Type::INT4, 5_000i32.to_be_bytes().to_vec(), Ok(5_000)),
            (Type::INT8, 1_794_211_200_125i64.to_be_bytes().to_vec(), Ok(1_794_211_200_125)),
            (Type::NUMERIC, numeric(4, 3, 0, 0, &[1, 7942, 1120, 125]), Ok(1_794_211_200_125)),
            (Type::NUMERIC, numeric(4, 3, 0, 3, &[1, 7942, 1120, 125]), Ok(1_794_211_200_125)),
            (Type::FLOAT8, 1_794_211_200_125f64.to_be_bytes().to_vec(), Ok(1_794_211_200_125)),
            (Type::DATE, ((1_794_182_400i64 / 86_400 - 10_957) as i32).to_be_bytes().to_vec(), Ok(1_794_182_400_000)),
            (Type::NUMERIC, numeric(2, 0, 0, 1, &[1500, 5000]), Err(("bad_time", Some(1)))),
            (Type::FLOAT8, f64::NAN.to_be_bytes().to_vec(), Err(("bad_time", Some(1)))),
            (Type::INT8, (-1i64).to_be_bytes().to_vec(), Err(("bad_time", Some(1)))),
            (Type::TIMESTAMPTZ, i64::MIN.to_be_bytes().to_vec(), Err(("bad_time", Some(1)))),
            // A `time` of day and a `bool` are not a time: the fault is of the column.
            (Type::TIME, 3_600_000_000i64.to_be_bytes().to_vec(), Err(("bad_time", None))),
            (Type::BOOL, vec![1], Err(("bad_time", None))),
        ];
        for (ty, raw, want) in cases {
            let name = ty.name().to_owned();
            let f = frame_of("time", ty, &[Some(raw)]);
            assert_eq!(marks_of(&f).map(|m| m[0].time_ms), want, "{name}");
        }
        // A number or a bool is not a title. A `uuid`, a `json` and an enum are texts (7.16a).
        let mut b = builder(&[("time", Type::INT4), ("title", Type::INT8)], caps(10, 1000));
        b.push_row(&[Some(&1i32.to_be_bytes()), Some(&1i64.to_be_bytes())]).unwrap();
        assert_eq!(marks_of(&b.finish("q".into(), 0)), Err(("bad_text", None)));
        let mut b = builder(&[("time", Type::INT4), ("title", Type::UUID), ("text", Type::JSON)], caps(10, 1000));
        let id = [0x12u8, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
        b.push_row(&[Some(&1i32.to_be_bytes()), Some(&id), Some(br#"{"a": 1}"#)]).unwrap();
        let m = &marks_of(&b.finish("q".into(), 0)).unwrap()[0];
        assert_eq!((m.title.as_str(), m.text.as_str()), ("12345678-9abc-def0-1234-56789abcdef0", r#"{"a": 1}"#));
    }

    #[test]
    fn annotation_hostile_texts_from_postgres_are_refused() {
        for (text, reason) in [
            ("<img src=x onerror=alert(1)>", "bad_chars"),
            ("a\u{202e}b", "bad_chars"),
            ("a\u{7}b", "bad_chars"),
            ("</title>", "bad_chars"),
        ] {
            let mut b = builder(&[("time", Type::INT4), ("text", Type::TEXT)], caps(10, 10_000));
            b.push_row(&[Some(&1i32.to_be_bytes()), Some(b"ok")]).unwrap();
            b.push_row(&[Some(&2i32.to_be_bytes()), Some(text.as_bytes())]).unwrap();
            assert_eq!(marks_of(&b.finish("q".into(), 0)), Err((reason, Some(2))), "{text:?}");
        }
        let mut b = builder(&[("time", Type::INT4), ("tags", Type::TEXT_ARRAY)], caps(10, 10_000));
        let nine: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
        let items: Vec<Option<&str>> = nine.iter().map(|s| Some(s.as_str())).collect();
        b.push_row(&[Some(&1i32.to_be_bytes()), Some(&text_array(&items))]).unwrap();
        assert_eq!(marks_of(&b.finish("q".into(), 0)), Err(("too_long", Some(1))));
    }
}
