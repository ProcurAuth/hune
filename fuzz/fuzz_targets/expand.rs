// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fuzz target `expand` (16.16, 7.11).
//!
//! Input format (a plain byte format, so a seed is easy to write):
//!
//! - Byte 0: the template. 0 means free mode: the SQL is the text of the first section. 1 and
//!   more select a fixed SQL template of `TEMPLATES` (modulo the number of templates).
//! - Byte 1: flags. Bit 0 sets `interval` to none. Bit 1 sets "All" on the variable `c`, bit 2 on `t`.
//!   Bit 4 opens the `custom` variables `c` and `a` (`allow_custom_value`, F41): each value of them is free text.
//!   Bit 5 opens the `query` variables `l`, `q` and `t`: the expansion MUST be the same as with bit 5 clear.
//!   Bit 3 selects the dialect `postgres` (7.20 to 7.25): the lexer, the escape rules and the macros of
//!   PostgreSQL, and the templates of `TEMPLATES_PG`. With bit 3 clear the dialect is `clickhouse`.
//! - Bytes 2 and 3: `max_data_points`, big endian, with no change.
//! - Bytes 4 to 7: the start of the range in seconds. Bytes 8 and 9: the length in seconds minus 1.
//! - Bytes 10 and 11: `min_interval` in milliseconds.
//! - Bytes 12 and 13: `max_query_size`, scaled by 64, and 0 means 262144.
//! - The rest: sections that a NUL byte ends. In free mode, section 0 is the SQL, and section
//!   1 + i holds the values of the variable i. In template mode, section i holds the values of
//!   the variable i. A byte 0x01 splits the values of one variable.
//!
//! The variables are fixed: `s` (text), `n` (number), `l` (query, multi), `q` (query), `c`
//! (custom, multi, with All), `a` (custom with an `all_value`) and `t` (query, with All and an
//! `all_value`).
//!
//! Properties:
//!
//! - No input makes `resolve_values`, `check_time_range`, `compute_interval` or `expand` panic.
//! - A result is at most `max_query_size` bytes, lexes with no fault and is a valid statement.
//! - The property P of 7.11, on the templates: a free value stays in its literal. When the
//!   expansion with the hostile values gives text, the token kinds of that text are the token
//!   kinds of the expansion with harmless values (`1`) in the same places, up to the sign of a
//!   number. A hostile value that changes the token structure and passes is a bug. This holds for both
//!   dialects, with the lexer of the dialect.
#![no_main]

use libfuzzer_sys::fuzz_target;
use hune_core::sql::lexer::token_fault;
use hune_core::sql::{
    Dialect, ExpandRequest, RequestValues, TextValueType, TokenKind, VariableDef, check_time_range, compute_interval,
    expand_for, lex_all_for, resolve_values,
};

/// The templates of template mode.
const TEMPLATES: [&str; 12] = [
    "SELECT 1 WHERE a = '$s'",
    "SELECT '$s' AS x, \"col\" FROM t WHERE y = '${s:csv}' /* $s */",
    "SELECT 1 FROM t WHERE c IN ($l)",
    "SELECT 1 FROM t WHERE c IN (${l:sqlstring}) AND d = ${q:singlequote}",
    "SELECT 1 FROM t WHERE a > 0 - $n AND b = 'k'",
    "SELECT 1 FROM t WHERE a > 0-$n AND b = 'k'",
    "SELECT 1 FROM t WHERE $__timeFilter(f(${s:singlequote}))",
    "SELECT 1 FROM t WHERE x IN (${l:csv}) AND z = ${n:raw}",
    "SELECT $__timeInterval(ts), '$s' FROM t GROUP BY 1",
    "SELECT 1 FROM t WHERE a = $s AND b = '${l:csv}'",
    "SELECT 1 FROM t WHERE a IN ($l) -- $s\nAND b = '$q'",
    "SELECT 1 FROM t WHERE a = ${s:sqlstring} OR b = '$q' AND c = $n",
];

/// The templates of template mode for the dialect `postgres`: the places of 7.21 to 7.24 (E strings, prefixes,
/// dollar quotes, operators that end in a sign, string continuation after a line end).
const TEMPLATES_PG: [&str; 14] = [
    "SELECT 1 WHERE a = '$s'",
    "SELECT '$s' AS x, \"col\" FROM t WHERE y = E'${s:csv}' /* $s */",
    "SELECT 1 FROM t WHERE c IN ($l)",
    "SELECT 1 FROM t WHERE c IN (${l:sqlstring}) AND d = ${q:singlequote}",
    "SELECT 1 FROM t WHERE a > 0 - $n AND b = 'k'",
    "SELECT 1 FROM t WHERE a @$n AND b = 'k'",
    "SELECT 1 FROM t WHERE $__timeFilter(f(${s:singlequote}))",
    "SELECT 1 FROM t WHERE x IN (${l:csv}) AND z = ${n:raw}",
    "SELECT $__timeGroup(ts, '$__interval'), '$s' FROM t GROUP BY 1",
    "SELECT 1 FROM t WHERE a = $s AND b = $$ $q $$",
    "SELECT 1 FROM t WHERE a IN ($l) -- $s\nAND b = '$q'",
    "SELECT 1 FROM t WHERE a = ${s:sqlstring} OR b = U&'$q' AND c = $n",
    "SELECT E'a'\n$l, 1 WHERE n = E$s AND x IN (B$l)",
    "SELECT 1 WHERE x = $__unixEpochGroup(1, '5m') AND y = '$q'::text AND z = $n$s",
];

/// The names of the variables, in the order of the sections.
const NAMES: [&str; 7] = ["s", "n", "l", "q", "c", "a", "t"];

fn defs(open_custom: bool, open_query: bool) -> Vec<VariableDef> {
    vec![
        VariableDef::text("s", TextValueType::Text),
        VariableDef::text("n", TextValueType::Number),
        VariableDef::query("l", true, false, None).with_open(open_query),
        VariableDef::query("q", false, false, None).with_open(open_query),
        VariableDef::custom("c", &["a", "b'c", "d\\e"], true, true, None).with_open(open_custom),
        VariableDef::custom("a", &["x", "y"], false, true, Some("1=1")).with_open(open_custom),
        VariableDef::query("t", false, true, Some("2=2")).with_open(open_query),
    ]
}

/// The variables with the `query` variables closed.
fn defs_plain(open_custom: bool) -> Vec<VariableDef> {
    defs(open_custom, false)
}

fn request(sections: &[&[u8]], all: [bool; 7]) -> Option<RequestValues> {
    let mut out = RequestValues::new();
    for (i, name) in NAMES.iter().enumerate() {
        let Some(raw) = sections.get(i) else {
            continue;
        };
        let mut values: Vec<String> = Vec::new();
        for part in raw.split(|b| *b == 1) {
            values.push(String::from_utf8(part.to_vec()).ok()?);
        }
        if raw.is_empty() {
            values.clear();
        }
        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
        out.insert(name, all[i], &refs);
    }
    Some(out)
}

/// The token kinds, with each `-` that a number follows with no blank left out.
fn shape(dialect: Dialect, text: &str) -> Vec<TokenKind> {
    let tokens = lex_all_for(dialect, text);
    let mut out = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let minus = matches!(t.kind, TokenKind::Punct | TokenKind::Operator) && &text[t.start..t.end] == "-";
        let before_number = tokens.get(i + 1).is_some_and(|n| n.kind == TokenKind::Number);
        if minus && before_number {
            continue;
        }
        out.push(t.kind);
    }
    out
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 14 {
        return;
    }
    let template = data[0] as usize;
    let flags = data[1];
    let dialect = if flags & 8 != 0 { Dialect::Postgres } else { Dialect::ClickHouse };
    let max_data_points = u32::from(u16::from_be_bytes([data[2], data[3]]));
    let from_s = i64::from(u32::from_be_bytes([data[4], data[5], data[6], data[7]]));
    let length_s = i64::from(u16::from_be_bytes([data[8], data[9]])) + 1;
    let min_interval = u64::from(u16::from_be_bytes([data[10], data[11]]));
    let size = usize::from(u16::from_be_bytes([data[12], data[13]]));
    let max_query_size = if size == 0 { 262_144 } else { size * 64 };
    let sections: Vec<&[u8]> = data[14..].split(|b| *b == 0).collect();

    let Ok(range) = check_time_range(from_s * 1000, (from_s + length_s) * 1000) else {
        return;
    };
    let interval = if flags & 1 != 0 { None } else { compute_interval(&range, max_data_points, min_interval).ok() };
    let all = [false, false, false, false, flags & 2 != 0, false, flags & 4 != 0];
    let defs = defs(flags & 16 != 0, flags & 32 != 0);

    let (sql, value_sections): (String, &[&[u8]]) = if template == 0 {
        let Ok(sql) = String::from_utf8(sections[0].to_vec()) else {
            return;
        };
        (sql, &sections[1..])
    } else {
        let text = match dialect {
            Dialect::ClickHouse => TEMPLATES[template % TEMPLATES.len()],
            Dialect::Postgres => TEMPLATES_PG[template % TEMPLATES_PG.len()],
        };
        (text.to_owned(), &sections[..])
    };
    let Some(values) = request(value_sections, all) else {
        return;
    };
    let Ok(resolved) = resolve_values(&defs, &values) else {
        return;
    };
    let run = |sql: &str, resolved: &_| {
        expand_for(dialect, &ExpandRequest { sql, values: resolved, range, interval, max_query_size })
    };
    let result = run(&sql, &resolved);
    if flags & 32 != 0 {
        // F41: the member `allow_custom_value` of a `query` variable changes nothing for the engine.
        let plain = defs_plain(flags & 16 != 0);
        let same = resolve_values(&plain, &values).ok().map(|r| run(&sql, &r));
        assert_eq!(same, Some(result.clone()), "allow_custom_value changed a query variable");
    }
    let Ok(out) = result else {
        return;
    };

    // Post-conditions of a valid expansion (7.2 steps 11 and 12, 7.13).
    assert!(out.len() <= max_query_size, "the result is larger than max_query_size");
    let tokens = lex_all_for(dialect, &out);
    assert!(tokens.iter().all(|t| token_fault(t).is_none()), "the result has a lexer fault: {out:?}");

    // The property P (7.11), by comparison with harmless values in the same places.
    if template != 0 {
        let mut harmless_sections: Vec<Vec<u8>> = Vec::new();
        for (i, raw) in value_sections.iter().enumerate() {
            let count = if raw.is_empty() { 0 } else { raw.split(|b| *b == 1).count() };
            let one = if i < 4 { b"1".to_vec() } else { Vec::new() };
            let joined: Vec<Vec<u8>> = (0..count).map(|_| one.clone()).collect();
            harmless_sections.push(joined.join(&1u8));
        }
        // A list of one empty text is not an empty list: keep the count with a non-empty text.
        let refs: Vec<&[u8]> = harmless_sections.iter().map(Vec::as_slice).collect();
        // Only the free variables s, n, l and q take part: the other variables keep their values.
        let mut mixed: Vec<&[u8]> = Vec::new();
        for (i, raw) in value_sections.iter().enumerate() {
            mixed.push(if i < 4 { refs[i] } else { raw });
        }
        let Some(harmless) = request(&mixed, all) else {
            return;
        };
        let Ok(harmless_resolved) = resolve_values(&defs, &harmless) else {
            return;
        };
        if let Ok(reference) = run(&sql, &harmless_resolved) {
            assert_eq!(
                shape(dialect, &out),
                shape(dialect, &reference),
                "a free value changed the token structure: sql {sql:?}, output {out:?}, reference {reference:?}"
            );
        }
    }
});
