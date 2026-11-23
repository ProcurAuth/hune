//! Small readers of JSON values.

use serde_json::Value;

/// Reads a number that is an integer. A float with no fraction (`2.0`) counts as an integer.
pub(crate) fn as_int(v: &Value) -> Option<i64> {
    if let Some(i) = v.as_i64() {
        return Some(i);
    }
    let f = v.as_f64()?;
    (f.fract() == 0.0 && f.abs() < 9.0e15).then_some(f as i64)
}

/// Reads a finite number.
pub(crate) fn as_num(v: &Value) -> Option<f64> {
    v.as_f64().filter(|f| f.is_finite())
}

/// Reads a boolean that Grafana can write as `true`, `false`, `1` or `0`.
pub(crate) fn as_flag(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => match n.as_i64() {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        },
        _ => None,
    }
}

/// The count of characters of a text.
pub(crate) fn chars(s: &str) -> usize {
    s.chars().count()
}
