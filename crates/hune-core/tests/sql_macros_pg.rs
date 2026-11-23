//! The macros of the dialect `postgres` (7.22) and the interval argument of a group macro (7.23).
//!
//! The files of `tests/golden/expansion-pg/` are vectors. Each vector holds the SQL `executedQueryString`
//! that the pinned Grafana 12.4.2 gives for a template with the core PostgreSQL data source. A vector
//! without the member `difference` MUST give the same text with Hune. A vector with the member
//! `difference` records a rule of the specification that differs from Grafana. It names the cause, the
//! section, the reason and the exact result of Hune. The test checks that Hune gives this result, and
//! that the result differs from Grafana. Thus an old difference cannot stay in a vector after a fix.
//!
//! This test runs the macro step only, with a small driver that uses the public functions of the
//! macro module and of the interval module: the same functions that the expander of the dialect uses.
//! The variable step belongs to 7.21 and has its own tests. A vector with variables gives the text that
//! the variable step writes in the member `values_sql`. The driver reads the member `all` of a variable
//! for `$__conditionalAll`.
//!
//! Two texts are equal when they are equal after the normal form of 15.14a: each run of blanks outside
//! literals and comments becomes one space, and the blanks at both ends go.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use hune_core::sql::interval::{
    GroupIntervalFault, MAX_GROUP_INTERVAL_MS, check_stored_group_interval, group_seconds, group_seconds_text,
    resolve_group_interval,
};
use hune_core::sql::macros::{CallFault, PgMacro, lookup, parse_pg_call, pg_timestamp};
use hune_core::sql::{
    Dialect, Interval, TimeRange, Token, TokenKind, check_time_range, compute_interval, lex_all, placeholders,
};
use serde_json::Value;

const CAUSES: [&str; 4] = ["escape", "interval", "format", "other"];

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

/// The error of the driver, in the form of the API: the code and the part.
type Failure = (String, String);

fn macro_fault(message: String) -> Failure {
    let _ = message;
    ("sql_invalid".to_owned(), "macro".to_owned())
}

/// The macro step of the expander, for the tests.
struct Driver<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    range: TimeRange,
    interval: Interval,
    /// The text that the variable step writes for a variable.
    variables: Vec<(String, String)>,
    /// The variables with "All" selected.
    all: Vec<String>,
}

impl Driver<'_> {
    fn slice(&self, a: usize, b: usize) -> &str {
        &self.src[a..b]
    }

    fn variable(&self, name: &str) -> Option<&str> {
        self.variables.iter().find(|(n, _)| n == name).map(|(_, t)| t.as_str())
    }

    fn expand(&self, lo: usize, hi: usize, out: &mut String) -> Result<(), Failure> {
        let mut i = lo;
        while i < hi {
            let t = self.tokens[i];
            if t.kind.is_code() {
                let mut j = i;
                while j < hi && self.tokens[j].kind.is_code() {
                    j += 1;
                }
                i = self.code_run(i, j, hi, out)?;
            } else if matches!(t.kind, TokenKind::String) {
                // A macro name in a string stays text (7.22). A variable is written (7.21).
                out.push_str(&self.write_variables(t.start, t.end));
                i += 1;
            } else {
                out.push_str(self.slice(t.start, t.end));
                i += 1;
            }
        }
        Ok(())
    }

    /// The text of the second argument of a group macro, with the variables written and the built-in
    /// `$__interval` left as it is (7.23).
    fn group_argument(&self, lo: usize, hi: usize) -> String {
        self.write_variables(self.tokens[lo].start, self.tokens[hi - 1].end)
    }

    /// The text of `start..end` with the variables written and each name that starts with `__` as it is.
    fn write_variables(&self, start: usize, end: usize) -> String {
        let mut out = String::new();
        let mut p = start;
        while let Some(found) = placeholders::next_for(Dialect::Postgres, self.src, p, end) {
            out.push_str(self.slice(p, found.start()));
            p = found.end();
            match found {
                placeholders::Found::Placeholder(ph)
                    if !ph.name.starts_with("__") && self.variable(ph.name).is_some() =>
                {
                    out.push_str(self.variable(ph.name).unwrap_or(""));
                }
                other => out.push_str(self.slice(other.start(), other.end())),
            }
        }
        out.push_str(self.slice(p, end));
        out
    }

    fn code_run(&self, i0: usize, j: usize, hi: usize, out: &mut String) -> Result<usize, Failure> {
        let run_hi = self.tokens[j - 1].end;
        let mut p = self.tokens[i0].start;
        loop {
            let Some(found) = placeholders::next_for(Dialect::Postgres, self.src, p, run_hi) else {
                out.push_str(self.slice(p, run_hi));
                return Ok(j);
            };
            out.push_str(self.slice(p, found.start()));
            let placeholders::Found::Placeholder(ph) = found else {
                return Err(macro_fault("bad brace".to_owned()));
            };
            p = ph.end;
            if ph.name.starts_with("__") {
                let Some(hune_core::sql::macros::AnyMacro::Postgres(m)) = lookup(Dialect::Postgres, ph.name) else {
                    return Err(macro_fault(format!("${} is not a macro of Hune", ph.name)));
                };
                let call = parse_pg_call(m, self.src, &self.tokens, ph.end, hi)
                    .map_err(|f: CallFault| macro_fault(f.message_for(m.name(), m.arity())))?;
                let Some(c) = call else {
                    let text = m
                        .expand_value(&self.range, Some(self.interval))
                        .ok_or_else(|| macro_fault(format!("{} needs an argument", m.name())))?;
                    out.push_str(&text);
                    continue;
                };
                if !m.takes_argument() {
                    out.push_str(&m.expand_value(&self.range, Some(self.interval)).unwrap());
                    return Ok(c.close + 1);
                }
                let mut arg = String::new();
                self.expand(c.arg_start, c.arg_end, &mut arg)?;
                let text = if m == PgMacro::ConditionalAll {
                    let (lo, hi) = c.second.unwrap();
                    let name = self.slice(self.tokens[lo].start, self.tokens[hi - 1].end).trim_start_matches('$');
                    if self.all.iter().any(|n| n == name.trim_matches(|c| c == '{' || c == '}')) {
                        "1=1".to_owned()
                    } else {
                        arg
                    }
                } else if m.is_group() {
                    let (lo, hi) = c.second.unwrap();
                    let g = group_seconds(&self.group_argument(lo, hi), Some(self.interval.ms))
                        .map_err(|e| macro_fault(e.message))?;
                    m.expand_call(&self.range, &arg, Some(&g)).unwrap()
                } else {
                    m.expand_call(&self.range, &arg, None).unwrap()
                };
                out.push_str(&text);
                return Ok(c.close + 1);
            }
            match self.variable(ph.name) {
                Some(text) => out.push_str(text),
                None => out.push_str(self.slice(ph.start, ph.end)),
            }
        }
    }
}

/// The result of Hune for one vector: the text, or the error code and its part.
fn run(v: &Value) -> Result<String, Failure> {
    let get = |k: &str| v[k].as_i64().unwrap_or_else(|| panic!("{k}"));
    let range = check_time_range(get("from_ms"), get("to_ms")).unwrap();
    let min = v["min_interval_ms"].as_u64().unwrap_or(1);
    let interval = compute_interval(&range, get("max_data_points") as u32, min).unwrap();
    assert_eq!(
        interval.ms,
        v["interval_ms"].as_u64().unwrap(),
        "{}: the interval of 7.4 differs from the interval of the capture",
        v["id"]
    );
    let variables = v
        .get("values_sql")
        .and_then(Value::as_object)
        .map(|o| o.iter().map(|(k, t)| (k.clone(), t.as_str().unwrap().to_owned())).collect())
        .unwrap_or_default();
    let all =
        v["values"].as_object().unwrap().iter().filter(|(_, x)| x["all"] == true).map(|(k, _)| k.clone()).collect();
    let sql = v["template"].as_str().unwrap();
    let driver = Driver { src: sql, tokens: lex_all(sql), range, interval, variables, all };
    let mut out = String::new();
    driver.expand(0, driver.tokens.len(), &mut out)?;
    Ok(out)
}

/// The normal form of 15.14a.
fn normal(sql: &str) -> String {
    let mut out = String::new();
    for t in lex_all(sql) {
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

fn normalise(r: &Result<String, Failure>) -> Result<String, Failure> {
    r.as_ref().map(|s| normal(s)).map_err(Clone::clone)
}

#[test]
fn hune_matches_grafana_on_each_vector() {
    let all = vectors();
    assert!(all.len() >= 80, "the vectors are missing: {}", all.len());
    let mut faults = Vec::new();
    for (file, v) in &all {
        let id = v["id"].as_str().unwrap();
        if file != &format!("{id}.json") {
            faults.push(format!("{file}: the id {id} differs from the file name"));
            continue;
        }
        if v["grafana"] != "12.4.2" {
            faults.push(format!("{id}: the vector is not from the pinned Grafana"));
        }
        if v["tags"].as_array().is_none_or(Vec::is_empty) {
            faults.push(format!("{id}: no tag"));
        }
        let got = run(v);
        let expected = &v["expected"];
        match v.get("difference") {
            None => {
                let Some(want) = expected.as_str() else {
                    faults.push(format!("{id}: Grafana gave no SQL and the vector has no difference"));
                    continue;
                };
                match &got {
                    Ok(s) if normal(s) == normal(want) => {}
                    other => faults.push(format!("{id}: Grafana {want:?} / Hune {other:?}")),
                }
            }
            Some(d) => {
                let cause = d["cause"].as_str().unwrap_or("");
                if !CAUSES.contains(&cause) {
                    faults.push(format!("{id}: cause {cause:?} is not one of {CAUSES:?}"));
                }
                for k in ["section", "reason"] {
                    if d[k].as_str().is_none_or(str::is_empty) {
                        faults.push(format!("{id}: the difference has no {k}"));
                    }
                }
                let want = hune_of(d);
                if normalise(&got) != normalise(&want) {
                    faults.push(format!("{id}: Hune gives {got:?}, the difference records {want:?}"));
                }
                // The difference MUST be real.
                let grafana = expected.as_str().map(|s| Ok::<String, Failure>(normal(s)));
                if grafana.as_ref() == Some(&normalise(&got)) {
                    faults.push(format!("{id}: the difference is not real: Hune gives the text of Grafana"));
                }
            }
        }
    }
    assert!(faults.is_empty(), "{} faults:\n{}", faults.len(), faults.join("\n"));
}

#[test]
fn each_macro_and_each_unit_has_a_vector() {
    let all = vectors();
    let tags: BTreeSet<String> = all
        .iter()
        .flat_map(|(_, v)| v["tags"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_owned()))
        .collect();
    for m in PgMacro::ALL {
        let macro_tag = format!("macro:{}", m.name());
        let builtin_tag = format!("builtin:{}", m.name());
        assert!(tags.contains(&macro_tag) || tags.contains(&builtin_tag), "no vector for {}", m.name());
    }
    // Each unit of 7.23.
    let units: BTreeSet<&str> = ["ms", "s", "m", "h", "d", "w", "M", "y"].into_iter().collect();
    let found: BTreeSet<&str> = tags
        .iter()
        .filter_map(|t| t.strip_prefix("interval:"))
        .map(|t| t.trim_start_matches(|c: char| c.is_ascii_digit()))
        .filter_map(|u| units.get(u).copied())
        .collect();
    assert_eq!(found, units, "a unit has no vector");
    assert!(tags.contains("worked:7.22"));
    // The vectors of the places and of the unknown names.
    for id in [
        "pg-macro-in-string",
        "pg-macro-in-block-comment",
        "pg-macro-in-line-comment",
        "pg-macro-in-quoted-name",
        "pg-macro-timeGroup-fill-value",
        "pg-macro-unixEpochGroup-fill-value",
        "pg-unknown-macro-timeFilter_ms",
        "pg-unknown-macro-timeInterval",
        "pg-unknown-macro-dateFilter",
        "pg-unknown-macro-fromTime",
    ] {
        assert!(all.iter().any(|(_, v)| v["id"] == id), "no vector {id}");
    }
}

/// The worked example of 7.22 is the vector `pg-worked-7-22`. The text here is a copy of the text of
/// the specification, so a change of the specification changes this test.
#[test]
fn worked_example_of_7_22() {
    const FINAL_SQL: &str = "SELECT floor(extract(epoch from created_at)/20)*20 AS \"time\", host, avg(load) AS load\n\
FROM metrics.node_load\n\
WHERE created_at BETWEEN '2026-11-09T08:00:00Z' AND '2026-11-09T14:00:00Z' AND host IN ('a1','b''2') AND env = 'prod'\n\
GROUP BY 1, host\n\
ORDER BY 1";
    let all = vectors();
    let (_, v) = all.iter().find(|(_, v)| v["id"] == "pg-worked-7-22").expect("the vector exists");
    assert!(v.get("difference").is_none());
    assert_eq!(v["expected"], FINAL_SQL, "Grafana");
    assert_eq!(run(v).unwrap(), FINAL_SQL, "Hune");
    assert_eq!(v["interval_ms"], 20_000);
    assert_eq!(v["max_data_points"], 1200);
}

// --- The interval argument (7.23) ---

#[test]
fn interval_table_of_7_23() {
    for (arg, g) in [
        ("'5m'", "300"),
        ("'1h'", "3600"),
        ("'1d'", "86400"),
        ("'500ms'", "0.5"),
        ("'1ms'", "0.001"),
        ("'1w'", "604800"),
        ("'1M'", "2592000"),
        ("'1y'", "31536000"),
        ("'365d'", "31536000"),
        ("'1500ms'", "1.5"),
        ("'250ms'", "0.25"),
        ("'17ms'", "0.017"),
    ] {
        assert_eq!(group_seconds(arg, Some(20_000)).unwrap(), g, "{arg}");
    }
    // The built-in stands for I.
    for arg in ["$__interval", "${__interval}", "'$__interval'"] {
        for (ms, g) in [(1, "0.001"), (20_000, "20"), (60_000, "60"), (3_600_000, "3600"), (31_536_000_000, "31536000")]
        {
            assert_eq!(group_seconds(arg, Some(ms)).unwrap(), g, "{arg} {ms}");
        }
    }
}

#[test]
fn interval_refusals_cover_the_forms_that_grafana_reads() {
    for refused in [
        "'1h30m'",
        "'1.5s'",
        "'05m'",
        "'-5m'",
        "'0s'",
        "'2y'",
        "' 5m '",
        "''",
        "'10x'",
        "300",
        "5m",
        "\"5m\"",
        "'5m', 0",
        "'5m' OR 1=1",
        "$__interval_s",
        "$__interval_ms",
        "'366d'",
    ] {
        assert!(group_seconds(refused, Some(20_000)).is_err(), "{refused}");
    }
}

/// The text G holds digits and at most one point with 1 to 3 decimals. This holds for each text
/// that the engine accepts, so a free value never changes the structure of the statement (7.23).
fn is_safe_seconds(g: &str) -> bool {
    let (whole, frac) = match g.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (g, None),
    };
    !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && frac.is_none_or(|f| (1..=3).contains(&f.len()) && f.bytes().all(|b| b.is_ascii_digit()) && !f.ends_with('0'))
}

#[test]
fn accepted_interval_never_carries_free_text() {
    // A deterministic generator: no new crate.
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let alphabet: Vec<char> = "'\"0123456789msMhdwy.$_{}()-+ ,;\\/*%eExXnNEBU&\u{a0}\n\t[]a".chars().collect();
    let mut accepted = 0usize;
    for round in 0..200_000 {
        let len = (next() % 9) as usize;
        let mut text: String = (0..len).map(|_| alphabet[(next() as usize) % alphabet.len()]).collect();
        // Half of the texts have the quote marks, so the generator reaches the grammar.
        if round % 2 == 0 {
            text = format!("'{text}'");
        }
        if let Ok(g) = group_seconds(&text, Some(20_000)) {
            accepted += 1;
            assert!(is_safe_seconds(&g), "{text:?} gave {g:?}");
            let ms = resolve_group_interval(&text, Some(20_000)).unwrap();
            assert!((1..=MAX_GROUP_INTERVAL_MS).contains(&ms), "{text:?}");
            assert_eq!(g, group_seconds_text(ms));
        }
        // The check at save never accepts a text that has no valid shape.
        if check_stored_group_interval(&text).is_ok() {
            let shape_ok = text == "$__interval"
                || text == "${__interval}"
                || (text.len() >= 2 && text.starts_with('\'') && text.ends_with('\''));
            assert!(shape_ok, "{text:?}");
        }
    }
    assert!(accepted > 0, "the generator reached no valid text");
}

#[test]
fn every_whole_interval_at_the_limits() {
    for (arg, ok) in [
        ("'1ms'", true),
        ("'0ms'", false),
        ("'31536000000ms'", true),
        ("'31536000001ms'", false),
        ("'31536000s'", true),
        ("'31536001s'", false),
        ("'525600m'", true),
        ("'525601m'", false),
        ("'8760h'", true),
        ("'8761h'", false),
        ("'365d'", true),
        ("'366d'", false),
        ("'52w'", true),
        ("'53w'", false),
        ("'12M'", true),
        ("'13M'", false),
        ("'1y'", true),
        ("'2y'", false),
        ("'18446744073709551615ms'", false),
        ("'18446744073709551616ms'", false),
        ("'99999999999999999999999999d'", false),
    ] {
        assert_eq!(group_seconds(arg, Some(20_000)).is_ok(), ok, "{arg}");
    }
}

#[test]
fn injection_in_the_interval_is_refused() {
    for text in [
        "'5m'); DROP TABLE t; --'",
        "'5m') OR (1=1'",
        "'5m'; SELECT pg_sleep(10)",
        "'5m'--",
        "'5m'/*",
        "'5m' || (SELECT 1)",
        "$__interval; SELECT 1",
        "$__interval) OR (1=1",
        "'1'||'0m'",
        "'$__interval' ",
        "$$5m$$",
        "$a$5m$a$",
        "E'5m'",
        "U&'5m'",
        "'5m\\'",
        "'5\u{0}m'",
    ] {
        let err = group_seconds(text, Some(20_000));
        assert!(err.is_err(), "{text:?} gave {err:?}");
    }
    // A text with a placeholder passes the check at save, and the run time check refuses it after the
    // variable gave its value.
    assert!(check_stored_group_interval("'${step}'").is_ok());
    let e = group_seconds("'5m'); DROP TABLE t; --'", Some(1000)).unwrap_err();
    assert_eq!(e.fault, GroupIntervalFault::Grammar);
    let e = group_seconds("5m'); DROP TABLE t; --", Some(1000)).unwrap_err();
    assert_eq!(e.fault, GroupIntervalFault::Shape);
}

#[test]
fn variable_query_has_no_builtin_interval() {
    for arg in ["$__interval", "${__interval}", "'$__interval'"] {
        let e = group_seconds(arg, None).unwrap_err();
        assert_eq!(e.fault, GroupIntervalFault::NoInterval, "{arg}");
    }
    assert_eq!(group_seconds("'5m'", None).unwrap(), "300");
    // `$__interval_s` and `$__interval_ms` have no value without the interval.
    let range = check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap();
    assert!(PgMacro::IntervalS.needs_interval() && PgMacro::IntervalS.expand_value(&range, None).is_none());
    assert!(PgMacro::IntervalMs.needs_interval() && PgMacro::IntervalMs.expand_value(&range, None).is_none());
    assert!(PgMacro::TimeFrom.expand_value(&range, None).is_some());
}

#[test]
fn timestamps_have_the_form_of_7_22() {
    // TS(t): the fraction has no zero at its end and is absent when it is 0.
    for (ms, text) in [
        (1_794_211_200_000, "2026-11-09T08:00:00Z"),
        (1_794_211_200_001, "2026-11-09T08:00:00.001Z"),
        (1_794_211_200_010, "2026-11-09T08:00:00.01Z"),
        (1_794_211_200_100, "2026-11-09T08:00:00.1Z"),
        (1_794_211_200_999, "2026-11-09T08:00:00.999Z"),
    ] {
        assert_eq!(pg_timestamp(ms), text);
    }
    // For each millisecond of one second: the text parses back to the same value.
    for k in 0..1000_i64 {
        let t = pg_timestamp(1_794_211_200_000 + k);
        let (head, rest) = t.split_at(19);
        assert_eq!(head, "2026-11-09T08:00:00");
        let fraction = rest.trim_end_matches('Z').trim_start_matches('.');
        let back: i64 = format!("{fraction:0<3}").parse().unwrap();
        assert_eq!(back, k, "{t}");
        assert!(!fraction.ends_with('0'), "{t}");
    }
}
