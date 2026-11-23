//! Tests of the expansion of the dialect `postgres` (7.21 to 7.25): the 87 vectors of
//! `tests/golden/expansion-pg/` end to end, the escape rules, the places, the formats, the interval
//! argument and the property P.
//!
//! A vector without the member `difference` MUST give the text of Grafana after the normal form of
//! 15.14a. A vector with the member `difference` records a rule of the specification that differs from
//! Grafana. Hune MUST give the recorded result, and this result MUST differ from Grafana.

use std::fs;
use std::path::PathBuf;

use hune_core::sql::{
    Dialect, ExpandError, ExpandRequest, RequestValues, SqlInvalidKind, TextValueType, TokenKind, ValuesError,
    VariableDef, check_time_range, compute_interval, expand_for, lex_all_for, resolve_values,
};
use serde_json::Value;

const FROM: i64 = 1_794_211_200_000;
const TO: i64 = 1_794_232_800_000;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden/expansion-pg")
}

fn vectors() -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir()).expect("tests/golden/expansion-pg exists") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let value: Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        out.push((name, value));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The error in the form of the API: the code and the part.
type Failure = (String, String);

fn failure(e: &ExpandError) -> Failure {
    let part = match e {
        ExpandError::Invalid(i) => match i.kind {
            SqlInvalidKind::Lexer => "lexer",
            SqlInvalidKind::Statement => "statement",
            SqlInvalidKind::Macro => "macro",
            SqlInvalidKind::Placeholder => "placeholder",
            SqlInvalidKind::Context => "context",
            SqlInvalidKind::Parameter => "parameter",
        },
        _ => "",
    };
    (e.api_code().to_owned(), part.to_owned())
}

fn defs_of(v: &Value) -> Vec<VariableDef> {
    v["variables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let name = d["name"].as_str().unwrap();
            let multi = d["multi"].as_bool().unwrap_or(false);
            let include_all = d["include_all"].as_bool().unwrap_or(false);
            let all_value = d["all_value"].as_str();
            match d["kind"].as_str().unwrap() {
                "query" => VariableDef::query(name, multi, include_all, all_value),
                "custom" => {
                    let options: Vec<&str> =
                        d["options"].as_array().unwrap().iter().map(|o| o.as_str().unwrap()).collect();
                    VariableDef::custom(name, &options, multi, include_all, all_value)
                }
                other => panic!("kind {other}"),
            }
        })
        .collect()
}

fn run(v: &Value) -> Result<String, Failure> {
    let get = |k: &str| v[k].as_i64().unwrap_or_else(|| panic!("{k}"));
    let range = check_time_range(get("from_ms"), get("to_ms")).unwrap();
    let min = v["min_interval_ms"].as_u64().unwrap_or(1);
    let interval = compute_interval(&range, get("max_data_points") as u32, min).unwrap();
    assert_eq!(interval.ms, v["interval_ms"].as_u64().unwrap(), "{}: the interval of 7.4", v["id"]);
    let defs = defs_of(v);
    let mut request = RequestValues::new();
    for (name, x) in v["values"].as_object().unwrap() {
        let values: Vec<&str> = x["values"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        request.insert(name, x["all"].as_bool().unwrap(), &values);
    }
    let resolved = resolve_values(&defs, &request).map_err(|e| match e {
        ValuesError::Refused(_) => ("variable_value_refused".to_owned(), String::new()),
        ValuesError::BadRequest(m) => panic!("bad request: {m}"),
    })?;
    let sql = v["template"].as_str().unwrap();
    let req = ExpandRequest { sql, values: &resolved, range, interval: Some(interval), max_query_size: 262_144 };
    expand_for(Dialect::Postgres, &req).map_err(|e| failure(&e))
}

/// The normal form of 15.14a: each run of blanks outside literals and comments is one space.
fn normal(sql: &str) -> String {
    let mut out = String::new();
    for t in lex_all_for(Dialect::Postgres, sql) {
        if t.kind == TokenKind::Blank {
            out.push(' ');
        } else {
            out.push_str(&sql[t.start..t.end]);
        }
    }
    let mut collapsed = String::new();
    for c in out.chars() {
        if c == ' ' && collapsed.ends_with(' ') {
            continue;
        }
        collapsed.push(c);
    }
    collapsed.trim().to_owned()
}

fn hune_of(d: &Value) -> Result<String, Failure> {
    let v = &d["hune"];
    if let Some(sql) = v.get("sql").and_then(Value::as_str) {
        Ok(sql.to_owned())
    } else {
        let e = &v["error"];
        Err((e["code"].as_str().unwrap().to_owned(), e["part"].as_str().unwrap_or("").to_owned()))
    }
}

fn norm(r: &Result<String, Failure>) -> Result<String, Failure> {
    r.as_ref().map(|s| normal(s)).map_err(Clone::clone)
}

#[test]
fn the_expander_passes_each_vector() {
    let all = vectors();
    assert_eq!(all.len(), 87, "the number of vectors");
    let mut faults = Vec::new();
    for (_, v) in &all {
        let id = v["id"].as_str().unwrap();
        let got = run(v);
        match v.get("difference") {
            None => {
                let want = v["expected"].as_str().unwrap_or_else(|| panic!("{id}: no expected text"));
                match &got {
                    Ok(s) if normal(s) == normal(want) => {}
                    other => faults.push(format!("{id}: Grafana {want:?} / Hune {other:?}")),
                }
            }
            Some(d) => {
                let want = hune_of(d);
                if norm(&got) != norm(&want) {
                    faults.push(format!("{id}: Hune gives {got:?}, the difference records {want:?}"));
                }
                let grafana = v["expected"].as_str().map(|s| Ok::<String, Failure>(normal(s)));
                if grafana.as_ref() == Some(&norm(&got)) {
                    faults.push(format!("{id}: the difference is not real"));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} faults:\n{}", faults.len(), faults.join("\n"));
}

// ---- helpers for the hand-made cases ----

struct Case {
    defs: Vec<VariableDef>,
    values: RequestValues,
}

impl Case {
    fn new(defs: Vec<VariableDef>) -> Case {
        Case { defs, values: RequestValues::new() }
    }

    fn set(mut self, name: &str, all: bool, values: &[&str]) -> Case {
        self.values.insert(name, all, values);
        self
    }

    fn run_with(&self, sql: &str, with_interval: bool) -> Result<String, ExpandError> {
        let resolved = resolve_values(&self.defs, &self.values).map_err(|e| match e {
            ValuesError::Refused(r) => ExpandError::ValueRefused(r),
            ValuesError::BadRequest(m) => panic!("bad request: {m}"),
        })?;
        let range = check_time_range(FROM, TO).unwrap();
        let interval = with_interval.then(|| compute_interval(&range, 1200, 1).unwrap());
        expand_for(
            Dialect::Postgres,
            &ExpandRequest { sql, values: &resolved, range, interval, max_query_size: 262_144 },
        )
    }

    fn ok(&self, sql: &str) -> String {
        self.run_with(sql, true).unwrap_or_else(|e| panic!("{sql:?}: {e:?}"))
    }

    fn refused(&self, sql: &str) -> String {
        match self.run_with(sql, true) {
            Err(ExpandError::ValueRefused(r)) => r.variable,
            other => panic!("{sql:?}: {other:?}"),
        }
    }

    fn invalid(&self, sql: &str) -> SqlInvalidKind {
        match self.run_with(sql, true) {
            Err(ExpandError::Invalid(i)) => i.kind,
            other => panic!("{sql:?}: {other:?}"),
        }
    }
}

fn text_var() -> Vec<VariableDef> {
    vec![VariableDef::text("t", TextValueType::Text)]
}

fn one(value: &str) -> Case {
    Case::new(text_var()).set("t", false, &[value])
}

// ---- 7.21: escape rules ----

#[test]
fn standard_string_doubles_the_quote_and_keeps_the_backslash() {
    assert_eq!(one("o'x").ok("SELECT '$t'"), "SELECT 'o''x'");
    assert_eq!(one("a\\b").ok("SELECT '${t}'"), "SELECT 'a\\b'");
    assert_eq!(one("\\").ok("SELECT 'x$t'"), "SELECT 'x\\'");
    assert_eq!(one("' OR 1=1 --").ok("SELECT '$t'"), "SELECT ''' OR 1=1 --'");
}

#[test]
fn e_string_doubles_the_backslash_and_the_quote() {
    assert_eq!(one("o'x").ok("SELECT E'$t'"), "SELECT E'o''x'");
    assert_eq!(one("a\\b").ok("SELECT E'$t'"), "SELECT E'a\\\\b'");
    assert_eq!(one("\\").ok("SELECT e'x$t'"), "SELECT e'x\\\\'");
    assert_eq!(one("\\' OR 1=1 --").ok("SELECT E'$t'"), "SELECT E'\\\\'' OR 1=1 --'");
}

#[test]
fn a_free_value_that_leaves_an_open_token_after_its_range_is_refused() {
    // Finding I2: the value `a'` opens a string at its end. The text after it makes the lexer fail at a later
    // offset, outside the range of the value. The answer is `variable_value_refused`, not `sql_invalid`.
    let c = Case::new(vec![VariableDef::query("v", false, false, None)]).set("v", false, &["a'"]);
    assert_eq!(c.refused("SELECT $v, 'b'"), "v");
    assert_eq!(c.refused("SELECT $v FROM t WHERE x = 'y'"), "v");
    // A stored text that does not lex stays `sql_invalid`.
    assert_eq!(c.invalid("SELECT 'b"), SqlInvalidKind::Lexer);
}

#[test]
fn e_string_continuation_keeps_the_escape_rule_of_the_first_part() {
    // The second part of an E string is read with the escape of the E string (7.20).
    assert_eq!(one("\\").ok("SELECT E'a'\n'$t'"), "SELECT E'a'\n'\\\\'");
    assert_eq!(one("a'b").ok("SELECT 'a'\n'$t'"), "SELECT 'a'\n'a''b'");
}

#[test]
fn quoted_name_doubles_the_double_quote_for_a_custom_variable() {
    let c =
        Case::new(vec![VariableDef::custom("col", &["a\"b", "c"], false, false, None)]).set("col", false, &["a\"b"]);
    assert_eq!(c.ok("SELECT \"$col\" FROM t"), "SELECT \"a\"\"b\" FROM t");
    // A backslash in a name is an ordinary byte.
    let c = Case::new(vec![VariableDef::custom("col", &["a\\b"], false, false, None)]).set("col", false, &["a\\b"]);
    assert_eq!(c.ok("SELECT \"$col\""), "SELECT \"a\\b\"");
    // A text variable in a quoted name is a document error (7.8).
    assert_eq!(one("x").invalid("SELECT \"$t\""), SqlInvalidKind::Context);
}

#[test]
fn code_string_literal_uses_the_standard_escape() {
    let c = Case::new(vec![VariableDef::query("h", true, false, None)]).set("h", false, &["a1", "b'2", "c\\d"]);
    assert_eq!(c.ok("SELECT 1 WHERE h IN ($h)"), "SELECT 1 WHERE h IN ('a1','b''2','c\\d')");
    assert_eq!(c.ok("SELECT 1 WHERE h IN (${h:singlequote})"), "SELECT 1 WHERE h IN ('a1','b''2','c\\d')");
    assert_eq!(c.ok("SELECT 1 WHERE h IN (${h:sqlstring})"), "SELECT 1 WHERE h IN ('a1','b''2','c\\d')");
    // An empty selection is the empty string literal.
    let c = Case::new(vec![VariableDef::query("h", true, false, None)]).set("h", false, &[]);
    assert_eq!(c.ok("SELECT 1 WHERE h IN ($h)"), "SELECT 1 WHERE h IN ('')");
}

#[test]
fn prefixed_forms_and_dollar_quotes_refuse_a_placeholder() {
    let c = one("x");
    for sql in [
        "SELECT B'$t'",
        "SELECT b'$t'",
        "SELECT X'$t'",
        "SELECT N'$t'",
        "SELECT U&'$t'",
        "SELECT u&'${t}'",
        "SELECT U&\"$t\"",
        "SELECT $$ $t $$",
        "SELECT $q$ ${t} $q$",
        "SELECT $q$ ${t:csv} $q$",
    ] {
        assert_eq!(c.invalid(sql), SqlInvalidKind::Context, "{sql}");
    }
    // The text of a name that is not a variable of the document stays text.
    assert_eq!(c.ok("SELECT $$ $other $$"), "SELECT $$ $other $$");
    assert_eq!(c.ok("SELECT B'101'"), "SELECT B'101'");
}

#[test]
fn comments_keep_the_text() {
    let c = one("x");
    assert_eq!(c.ok("SELECT 1 -- $t\n"), "SELECT 1 -- $t\n");
    assert_eq!(c.ok("SELECT 1 /* a /* $t */ $t */"), "SELECT 1 /* a /* $t */ $t */");
    // The `#` of ClickHouse is no comment: the placeholder after it is read.
    assert_eq!(one("5").ok("SELECT 1 # $t"), "SELECT 1 # 5");
}

#[test]
fn a_dollar_digit_is_a_parameter_and_the_statement_check_refuses_it() {
    assert_eq!(one("x").invalid("SELECT $1"), SqlInvalidKind::Statement);
    assert_eq!(one("x").invalid("SELECT '$t' WHERE a = $2"), SqlInvalidKind::Statement);
}

// ---- 7.9: formats inside a string ----

#[test]
fn formats_of_the_last_group_in_a_postgres_string() {
    let multi = || Case::new(vec![VariableDef::query("h", true, false, None)]).set("h", false, &["a.b", "c'd", "e\\f"]);
    // The text of the format is built first, then the escape of the kind of the string applies.
    assert_eq!(multi().ok("SELECT '${h:regex}'"), "SELECT '(a\\.b|c''d|e\\\\f)'");
    assert_eq!(multi().ok("SELECT E'${h:regex}'"), "SELECT E'(a\\\\.b|c''d|e\\\\\\\\f)'");
    assert_eq!(multi().ok("SELECT '${h:pipe}'"), "SELECT 'a.b|c''d|e\\f'");
    assert_eq!(multi().ok("SELECT E'${h:pipe}'"), "SELECT E'a.b|c''d|e\\\\f'");
    assert_eq!(multi().ok("SELECT '${h:json}'"), "SELECT '[\"a.b\",\"c''d\",\"e\\\\f\"]'");
    assert_eq!(multi().ok("SELECT '${h:doublequote}'"), "SELECT '\"a.b\",\"c''d\",\"e\\f\"'");
    assert_eq!(multi().ok("SELECT '${h:csv}'"), "SELECT 'a.b,c''d,e\\f'");
}

#[test]
fn string_only_formats_are_refused_in_code_and_in_a_name() {
    let c = Case::new(vec![VariableDef::custom("v", &["a"], false, false, None)]).set("v", false, &["a"]);
    for f in [
        "json",
        "pipe",
        "regex",
        "glob",
        "lucene",
        "percentencode",
        "queryparam",
        "distributed",
        "doublequote",
        "html",
        "text",
    ] {
        assert_eq!(c.invalid(&format!("SELECT ${{v:{f}}}")), SqlInvalidKind::Context, "{f}");
        assert_eq!(c.invalid(&format!("SELECT \"${{v:{f}}}\"")), SqlInvalidKind::Context, "{f}");
        c.ok(&format!("SELECT '${{v:{f}}}'"));
        c.ok(&format!("SELECT E'${{v:{f}}}'"));
    }
}

#[test]
fn a_hostile_value_in_each_string_format_stays_in_its_literal() {
    let values = ["'", "''", "\\", "\\'", "'; DROP TABLE t; --", "$$", "\\\\'", "a\\", "\"", "/*", "--", "x'y\\z"];
    let formats = [
        "csv",
        "json",
        "pipe",
        "regex",
        "glob",
        "lucene",
        "percentencode",
        "queryparam",
        "distributed",
        "doublequote",
        "html",
        "text",
    ];
    for v in values {
        for f in formats {
            for opener in ["'", "E'", "e'", "$X"] {
                if opener == "$X" {
                    continue;
                }
                let c = Case::new(vec![VariableDef::query("h", true, false, None)]).set("h", false, &[v, "z'", v]);
                let sql = format!("SELECT {opener}a${{h:{f}}}b' AS c, 1 AS d");
                match c.run_with(&sql, true) {
                    Ok(out) => {
                        let t = lex_all_for(Dialect::Postgres, &out);
                        let sig: Vec<_> = t.iter().filter(|t| t.kind != TokenKind::Blank).collect();
                        assert_eq!(sig.len(), 8, "{sql:?} gave {out:?}");
                        let want = if opener == "'" { TokenKind::String } else { TokenKind::EString };
                        assert_eq!(sig[1].kind, want, "{out:?}");
                    }
                    Err(e) => panic!("{sql:?}: {e:?}"),
                }
            }
        }
    }
}

// ---- 7.24: the property P ----

#[test]
fn p_refuses_the_joins_of_the_specification() {
    // `E$name` with the value `\`.
    let c = Case::new(vec![VariableDef::query("name", true, false, None)]).set("name", false, &["\\"]);
    assert_eq!(c.refused("SELECT 1 WHERE n = E$name"), "name");
    // `@$v` with the value `-5`: the operator `@-` starts before the range.
    let c = Case::new(vec![VariableDef::query("v", false, false, None)]).set("v", false, &["-5"]);
    assert_eq!(c.refused("SELECT 1 WHERE x @$v"), "v");
    // A blank before the number is fine. `+-5` reads as `+`, then `-5`.
    assert_eq!(c.ok("SELECT 1 WHERE x = $v"), "SELECT 1 WHERE x = -5");
    assert_eq!(c.ok("SELECT 1 WHERE x +$v"), "SELECT 1 WHERE x +-5");
    // `--5` is a comment.
    assert_eq!(c.refused("SELECT 1 WHERE x -$v"), "v");
    // A prefix before a string list.
    let c = Case::new(vec![VariableDef::query("s", true, false, None)]).set("s", false, &["a", "b"]);
    for prefix in ["E", "B", "X", "N", "U&", "e", "b"] {
        assert_eq!(c.refused(&format!("SELECT 1 WHERE x IN ({prefix}$s)")), "s", "{prefix}");
    }
    assert_eq!(c.ok("SELECT 1 WHERE x IN ($s)"), "SELECT 1 WHERE x IN ('a','b')");
    // A number glued to a word.
    let c = Case::new(vec![VariableDef::query("v", false, false, None)]).set("v", false, &["5"]);
    assert_eq!(c.refused("SELECT a$v"), "v");
    assert_eq!(c.refused("SELECT 1$v"), "v");
    assert_eq!(c.refused("SELECT $v.5"), "v");
}

#[test]
fn p_checks_a_string_continuation_after_an_e_string() {
    // A string list after an E string and a line end would join the E string.
    let c = Case::new(vec![VariableDef::query("s", true, false, None)]).set("s", false, &["\\"]);
    assert_eq!(c.refused("SELECT E'a'\n$s"), "s");
    // After a standard string the continuation keeps the standard escape.
    assert_eq!(c.ok("SELECT 'a'\n$s"), "SELECT 'a'\n'\\'");
}

#[test]
fn an_empty_list_joins_no_tokens() {
    let c = Case::new(vec![VariableDef::query("v", true, false, None)]).set("v", false, &[]);
    assert_eq!(c.ok("SELECT 1 WHERE x IN ($v)"), "SELECT 1 WHERE x IN ('')");
    assert_eq!(c.refused("SELECT 1 WHERE x IN (a-${v:csv}-b)"), "v");
    assert_eq!(c.refused("SELECT 1 -${v:raw}- 1"), "v");
    assert_eq!(c.refused("SELECT 1 /${v:raw}* 1"), "v");
}

// ---- 7.23: the interval argument ----

fn plain() -> Case {
    Case::new(vec![VariableDef::custom("step", &["5m", "1h"], false, false, None)]).set("step", false, &["5m"])
}

#[test]
fn the_interval_argument_end_to_end() {
    let c = plain();
    assert_eq!(c.ok("SELECT $__timeGroup(ts, '5m')"), "SELECT floor(extract(epoch from ts)/300)*300");
    assert_eq!(
        c.ok("SELECT $__timeGroupAlias(ts, $__interval)"),
        "SELECT floor(extract(epoch from ts)/20)*20 AS \"time\""
    );
    assert_eq!(c.ok("SELECT $__timeGroup(ts, ${__interval})"), "SELECT floor(extract(epoch from ts)/20)*20");
    assert_eq!(c.ok("SELECT $__timeGroup(ts, '$__interval')"), "SELECT floor(extract(epoch from ts)/20)*20");
    assert_eq!(c.ok("SELECT $__timeGroup(ts, '$step')"), "SELECT floor(extract(epoch from ts)/300)*300");
    assert_eq!(c.ok("SELECT $__unixEpochGroup(n + 1, '500ms')"), "SELECT floor((n + 1)/0.5)*0.5");
    assert_eq!(c.ok("SELECT $__unixEpochGroupAlias(n, '1ms')"), "SELECT floor((n)/0.001)*0.001 AS \"time\"");
}

#[test]
fn the_interval_argument_refusals() {
    let c = plain();
    for sql in [
        "SELECT $__timeGroup(ts, 5m)",
        "SELECT $__timeGroup(ts, 300)",
        "SELECT $__timeGroup(ts, '1h30m')",
        "SELECT $__timeGroup(ts, '2y')",
        "SELECT $__timeGroup(ts, $step)",
        "SELECT $__timeGroup(ts, '5m' || '1')",
        "SELECT $__timeGroup(ts, ${__interval:csv})",
        "SELECT $__timeGroup(ts, $__interval_s)",
        "SELECT $__timeGroup(ts, $__timeFrom)",
        "SELECT $__timeGroup(ts, '5m', 0)",
        "SELECT $__timeGroup(ts, E'5m')",
        "SELECT $__timeGroup(ts, $$5m$$)",
        "SELECT $__timeGroup(ts)",
        "SELECT $__interval",
        "SELECT 1 WHERE x > $__interval",
    ] {
        assert_eq!(c.invalid(sql), SqlInvalidKind::Macro, "{sql}");
    }
}

#[test]
fn a_bad_interval_from_a_variable_is_a_refused_value() {
    for bad in ["5m'); DROP TABLE t; --", "1h30m", "2y", "x", "", "0s", "5 m", "'5m'"] {
        let c =
            Case::new(vec![VariableDef::custom("step", &["5m", bad], false, false, None)]).set("step", false, &[bad]);
        assert_eq!(c.refused("SELECT $__timeGroup(ts, '$step')"), "step", "{bad:?}");
    }
    // A text variable too.
    let c = Case::new(vec![VariableDef::text("t", TextValueType::Text)]).set("t", false, &["5m'); DROP TABLE x; --"]);
    assert_eq!(c.refused("SELECT $__timeGroup(ts, '$t')"), "t");
    let c = Case::new(vec![VariableDef::text("t", TextValueType::Text)]).set("t", false, &["15m"]);
    assert_eq!(c.ok("SELECT $__timeGroup(ts, '$t')"), "SELECT floor(extract(epoch from ts)/900)*900");
}

#[test]
fn a_variable_query_has_no_interval() {
    let c = plain();
    for sql in [
        "SELECT $__interval_s",
        "SELECT $__interval_ms",
        "SELECT $__timeGroup(ts, $__interval)",
        "SELECT $__timeGroup(ts, '$__interval')",
    ] {
        match c.run_with(sql, false) {
            Err(ExpandError::Invalid(i)) => assert_eq!(i.kind, SqlInvalidKind::Macro, "{sql}"),
            other => panic!("{sql}: {other:?}"),
        }
    }
    assert_eq!(
        c.run_with("SELECT $__timeGroup(ts, '5m'), $__timeFrom", false).unwrap(),
        "SELECT floor(extract(epoch from ts)/300)*300, '2026-11-09T08:00:00Z'"
    );
}

#[test]
fn the_builtin_interval_is_range_checked() {
    let range = check_time_range(FROM, TO).unwrap();
    let resolved = resolve_values(&[], &RequestValues::new()).unwrap();
    // I = 366 days is over the limit of 7.23.
    let mut interval = compute_interval(&range, 1200, 1).unwrap();
    interval.ms = 366 * 86_400_000;
    interval.seconds = interval.ms / 1000;
    let req = ExpandRequest {
        sql: "SELECT $__timeGroup(ts, $__interval)",
        values: &resolved,
        range,
        interval: Some(interval),
        max_query_size: 1000,
    };
    assert!(matches!(expand_for(Dialect::Postgres, &req), Err(ExpandError::Invalid(_))));
}

// ---- the statement check goes through the dialect ----

#[test]
fn the_statement_check_of_the_dialect_applies() {
    let c = none();
    assert_eq!(c.invalid("SELECT pg_sleep(10)"), SqlInvalidKind::Statement);
    assert_eq!(c.invalid("SELECT * FROM t FOR UPDATE"), SqlInvalidKind::Statement);
    assert_eq!(c.invalid("SELECT U&\"d\\0061t\\0061\""), SqlInvalidKind::Statement);
    assert_eq!(c.invalid("DO $$ BEGIN END $$"), SqlInvalidKind::Statement);
    assert_eq!(c.invalid("SELECT 1; SELECT 2"), SqlInvalidKind::Statement);
    // A text that the ClickHouse lexer reads in another way is read with the lexer of 7.20.
    assert_eq!(c.ok("SELECT 'a\\' || 'b'"), "SELECT 'a\\' || 'b'");
    assert_eq!(c.ok("SELECT 1 # 2"), "SELECT 1 # 2");
    // A value never makes a refused function: the check runs on the final text.
    let t = one("x");
    assert_eq!(t.ok("SELECT '$t'"), "SELECT 'x'");
}

fn none() -> Case {
    Case::new(vec![])
}

#[test]
fn macros_stay_text_outside_code() {
    let c = none();
    assert_eq!(
        c.ok("SELECT '$__timeFrom', \"$__timeTo\", E'$__from' -- $__to\n"),
        "SELECT '$__timeFrom', \"$__timeTo\", E'$__from' -- $__to\n"
    );
    assert_eq!(c.ok("SELECT $__timeFrom, $__timeTo()"), "SELECT '2026-11-09T08:00:00Z', '2026-11-09T14:00:00Z'");
}

#[test]
fn size_limit_applies() {
    let range = check_time_range(FROM, TO).unwrap();
    let resolved = resolve_values(&[], &RequestValues::new()).unwrap();
    let req = ExpandRequest { sql: "SELECT 1", values: &resolved, range, interval: None, max_query_size: 5 };
    assert!(matches!(
        expand_for(Dialect::Postgres, &req),
        Err(ExpandError::TooLarge { .. }) | Err(ExpandError::Invalid(_))
    ));
}
