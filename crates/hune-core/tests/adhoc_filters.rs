//! The ad hoc filters (F36, decision ID-K329): the variable kind `adhoc` in the document (4.10c), the names of 6.14, the
//! macro `$__adhocFilters` at run time (7.10a, 7.11) and at save (7.18, items 17 to 21).
//!
//! The seeded test makes hostile values from the pieces of 7.11 and runs them through the macro. For each value the
//! expansion gives an error or the property P holds, and each literal decodes to its value. A second test shows that
//! the identifier text never comes from the request.

use hune_core::document::{DocumentNames, VariableKind, validate};
use hune_core::error::{ErrorCode, ValidationError};
use hune_core::sql::adhoc::{AdhocRun, PLACE_MESSAGE, RequestFilter, bind, place_is_permitted};
use hune_core::sql::{
    AdhocSave, Dialect, ExpandError, ExpandRequest, RequestValues, SqlInvalidKind, SqlUse, TextValueType, TokenKind,
    VariableDef, check_time_range, compute_interval, expand_prepared_adhoc, lex, resolve_values, save_checks_adhoc,
};
use serde_json::{Value, json};

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/adhoc.json");
const FROM: i64 = 1_794_211_200_000;
const TO: i64 = 1_794_232_800_000;

fn base() -> Value {
    serde_json::from_slice(FIXTURE).unwrap()
}

fn errors_of(v: &Value) -> Vec<ValidationError> {
    validate(&serde_json::to_vec(v).unwrap(), None).err().unwrap_or_default()
}

fn one_error(v: &Value) -> (ErrorCode, String) {
    let e = errors_of(v);
    assert_eq!(e.len(), 1, "{e:#?}");
    (e[0].code, e[0].pointer.clone())
}

// ---- the document ----

#[test]
fn the_fixture_is_valid_and_canonical() {
    let doc = validate(FIXTURE, None).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(std::str::from_utf8(doc.bytes()).unwrap(), std::str::from_utf8(FIXTURE).unwrap());
    let kinds: Vec<String> = doc.document().variables().iter().map(|v| v.kind.tag().to_string()).collect();
    assert_eq!(kinds, ["adhoc", "custom", "adhoc"]);
    match &doc.document().variables()[0].kind {
        VariableKind::Adhoc { source, keys } => {
            assert_eq!(source.as_str(), "prod");
            assert_eq!(keys, &["service", "host", "env"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn each_rule_of_the_kind_gives_its_error() {
    let set = |f: &dyn Fn(&mut Value)| {
        let mut v = base();
        f(&mut v);
        one_error(&v)
    };
    let keys = |n: usize| Value::from((0..n).map(|i| format!("k{i}")).collect::<Vec<_>>());
    assert_eq!(set(&|v| v["variables"][0]["keys"] = json!([])), (ErrorCode::OutOfRange, "/variables/0/keys".into()));
    assert_eq!(set(&|v| v["variables"][0]["keys"] = keys(65)), (ErrorCode::TooLarge, "/variables/0/keys".into()));
    assert!(
        errors_of(&{
            let mut v = base();
            v["variables"][0]["keys"] = keys(64);
            v
        })
        .is_empty()
    );
    for bad in ["a.b", "1a", "", "a b", "m['k']", "é", &"a".repeat(65)] {
        let r = set(&|v| v["variables"][0]["keys"][1] = json!(bad));
        assert_eq!(r, (ErrorCode::BadFormat, "/variables/0/keys/1".into()), "{bad:?}");
    }
    assert_eq!(
        set(&|v| v["variables"][0]["keys"][2] = json!("service")),
        (ErrorCode::NotUnique, "/variables/0/keys/2".into())
    );
    assert_eq!(set(&|v| v["variables"][0]["keys"][0] = json!(1)), (ErrorCode::WrongType, "/variables/0/keys/0".into()));
    assert_eq!(set(&|v| v["variables"][0]["source"] = json!("Bad id")).0, ErrorCode::BadFormat);
    for member in ["default", "filters", "multi", "options", "operators"] {
        let r = set(&|v| v["variables"][0][member] = json!([]));
        assert_eq!(r, (ErrorCode::UnknownMember, format!("/variables/0/{member}")), "{member}");
    }
    let r = set(&|v| v["variables"][0]["placement"] = json!({"section": 0, "x": 0, "y": 20, "w": 4, "h": 2}));
    assert_eq!(r, (ErrorCode::NotPermitted, "/variables/0/placement".into()));
    let r = set(&|v| v["variables"][0].as_object_mut().unwrap().remove("keys").map(|_| ()).unwrap_or(()));
    assert_eq!(r, (ErrorCode::MissingMember, "/variables/0".into()));
    // At most one adhoc variable for each source (4.10c, rule 1).
    let r = set(&|v| v["variables"][2]["source"] = json!("prod"));
    assert_eq!(r, (ErrorCode::NotUnique, "/variables/2/source".into()));
    // A repeat cannot name it.
    let mut v = base();
    v["sections"][0]["panels"][1]["repeat"] = json!({"variable": "filters", "direction": "h", "max_per_row": 4});
    let e = errors_of(&v);
    assert!(
        e.iter().any(|x| x.code == ErrorCode::NotPermitted && x.pointer == "/sections/0/panels/1/repeat/variable"),
        "{e:#?}"
    );
}

#[test]
fn the_names_of_6_14_hold_the_adhoc_variable_for_each_query_with_the_macro_on_its_source() {
    let doc = validate(FIXTURE, None).unwrap();
    let names = DocumentNames::of(doc.document());
    assert_eq!(names.panels, vec![(1, vec![vec!["env".to_owned(), "filters".to_owned()]]), (2, vec![vec![]])]);
    // A query on another source names the macro: the variable of that source is not in its list.
    let mut v = base();
    v["sections"][0]["panels"][1]["data"]["query"]["source"] = json!("ch2");
    v["sections"][0]["panels"][1]["data"]["query"]["sql"] = json!(["SELECT 1 FROM t WHERE $__adhocFilters"]);
    let doc = validate(&serde_json::to_vec(&v).unwrap(), None).unwrap();
    assert_eq!(DocumentNames::of(doc.document()).panels[1].1, vec![vec!["other".to_owned()]]);
    assert!(doc.names_json().contains("\"2\":[[\"other\"]]"), "{}", doc.names_json());
}

// ---- the run ----

fn defs() -> Vec<VariableDef> {
    vec![VariableDef::text("t", TextValueType::Text)]
}

fn run_sql(sql: &str, run: Option<&AdhocRun<'_>>, dialect: Dialect) -> Result<String, ExpandError> {
    let mut values = RequestValues::new();
    values.insert("t", false, &["x'y"]);
    let resolved = resolve_values(&defs(), &values).unwrap();
    let range = check_time_range(FROM, TO).unwrap();
    let interval = Some(compute_interval(&range, 1200, 1).unwrap());
    let req = ExpandRequest { sql, values: &resolved, range, interval, max_query_size: 1_048_576 };
    expand_prepared_adhoc(dialect, &req, run).map(|p| p.sql)
}

fn rf(key: &str, op: &str, value: &str) -> RequestFilter {
    RequestFilter { key: key.into(), op: op.into(), value: value.into() }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn the_macro_gives_one_one_or_the_filters() {
    let keys = strings(&["host", "env"]);
    let sql = "SELECT count() FROM t WHERE $__adhocFilters";
    let empty = AdhocRun::empty("filters");
    assert_eq!(run_sql(sql, Some(&empty), Dialect::ClickHouse).unwrap(), "SELECT count() FROM t WHERE 1=1");
    let run = bind("filters", &keys, &keys, &[rf("host", "=", "a'b"), rf("env", "!=", "")]).unwrap();
    assert_eq!(
        run_sql(sql, Some(&run), Dialect::ClickHouse).unwrap(),
        "SELECT count() FROM t WHERE (`host` = 'a\\'b' AND `env` != '')"
    );
    let braced = "SELECT 1 FROM t PREWHERE ${__adhocFilters} AND x = '$t'";
    assert_eq!(
        run_sql(braced, Some(&run), Dialect::ClickHouse).unwrap(),
        "SELECT 1 FROM t PREWHERE (`host` = 'a\\'b' AND `env` != '') AND x = 'x\\'y'"
    );
    // In a string, a quoted name or a comment the name stays text, as any macro (7.10).
    let text = "SELECT '$__adhocFilters' AS a FROM t WHERE 1 -- $__adhocFilters";
    assert_eq!(run_sql(text, Some(&run), Dialect::ClickHouse).unwrap(), text);
}

#[test]
fn the_macro_is_refused_out_of_its_place() {
    let keys = strings(&["host"]);
    let run = bind("filters", &keys, &keys, &[rf("host", "=", "a")]).unwrap();
    let invalid = |sql: &str, run: Option<&AdhocRun<'_>>, dialect: Dialect| match run_sql(sql, run, dialect) {
        Err(ExpandError::Invalid(i)) => (i.kind, i.message),
        other => panic!("{sql}: {other:?}"),
    };
    // No adhoc variable on the source of the query: a variable query, an annotation or a panel of another source.
    let (kind, message) = invalid("SELECT 1 FROM t WHERE $__adhocFilters", None, Dialect::ClickHouse);
    assert_eq!(kind, SqlInvalidKind::Macro);
    assert!(message.contains("adhoc variable"), "{message}");
    for sql in [
        "SELECT 1 FROM t WHERE x OR $__adhocFilters",
        "SELECT $__adhocFilters FROM t",
        "SELECT 1 FROM t WHERE NOT $__adhocFilters",
        "SELECT 1 FROM t WHERE AND$__adhocFilters",
    ] {
        let (kind, message) = invalid(sql, Some(&run), Dialect::ClickHouse);
        assert_eq!(kind, SqlInvalidKind::Macro, "{sql}");
        assert_eq!(message, PLACE_MESSAGE, "{sql}");
    }
    // Inside the argument of another macro.
    let (_, message) =
        invalid("SELECT 1 FROM t WHERE $__timeFilter(ts AND $__adhocFilters)", Some(&run), Dialect::ClickHouse);
    assert!(message.contains("inside another macro"), "{message}");
    // A format and an argument.
    assert_eq!(
        invalid("SELECT 1 FROM t WHERE ${__adhocFilters:csv}", Some(&run), Dialect::ClickHouse).0,
        SqlInvalidKind::Macro
    );
    assert_eq!(
        invalid("SELECT 1 FROM t WHERE $__adhocFilters()", Some(&run), Dialect::ClickHouse).0,
        SqlInvalidKind::Macro
    );
    // PostgreSQL has no such macro.
    assert_eq!(
        invalid("SELECT 1 FROM t WHERE $__adhocFilters", Some(&run), Dialect::Postgres).0,
        SqlInvalidKind::Macro
    );
}

// ---- the property P with seeded hostile values ----

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// The hostile pieces of 7.11 (the generator `random_sql` of ClickHouse Dash): quote marks, backslashes, comment
/// marks, `$`, placeholders and macros, Unicode spaces, and control characters that the checks of 7.7 refuse.
const PIECES: &[&str] = &[
    "'",
    "\"",
    "`",
    "\\",
    "\\'",
    "''",
    "--",
    "/*",
    "*/",
    "#",
    "#!",
    "$",
    "${t}",
    "$t",
    "$__fromTime",
    "$__adhocFilters",
    "$$",
    "$x$",
    ";",
    ")",
    "(",
    ",",
    " OR 1=1",
    "\u{a0}",
    "\u{2028}",
    "\u{feff}",
    "\u{3000}",
    "é",
    "\u{1f600}",
    "a",
    "1",
    "%",
    "\n",
    "\t",
    "\u{0}",
    "\u{7f}",
    " AND ",
    "SETTINGS",
    "FORMAT",
];

const TEMPLATES: &[&str] = &[
    "SELECT count() FROM t WHERE $__adhocFilters",
    "SELECT count() FROM t PREWHERE $__adhocFilters AND x = 1",
    "SELECT 1 FROM t WHERE $__timeFilter(ts) AND $__adhocFilters -- end",
    "SELECT 1 FROM t WHERE x = '$t' AND ${__adhocFilters} ORDER BY 1",
    "SELECT 1 FROM t WHERE\n/* c */ $__adhocFilters\nGROUP BY 1",
];

/// Reads the content of a string token of 7.5: a backslash escapes the next character, a doubled `'` is one quote.
fn decode(token: &str) -> String {
    let inner: Vec<char> = token[1..token.len() - 1].chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < inner.len() {
        match inner[i] {
            '\\' if i + 1 < inner.len() => {
                out.push(inner[i + 1]);
                i += 2;
            }
            '\'' if i + 1 < inner.len() && inner[i + 1] == '\'' => {
                out.push('\'');
                i += 2;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

#[test]
fn random_hostile_values_stay_in_their_literals() {
    let keys = strings(&["host", "env", "region"]);
    let seeds: u64 = std::env::var("HUNE_TEST_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(64);
    let (mut ok, mut refused) = (0usize, 0usize);
    for seed in 1..=seeds {
        let mut rng = Rng(seed);
        for _ in 0..200 {
            let count = 1 + rng.below(16);
            let filters: Vec<RequestFilter> = (0..count)
                .map(|_| {
                    let pieces = rng.below(8);
                    let value: String = (0..pieces).map(|_| PIECES[rng.below(PIECES.len())]).collect();
                    let op = if rng.below(2) == 0 { "=" } else { "!=" };
                    rf(&keys[rng.below(keys.len())], op, &value)
                })
                .collect();
            let template = TEMPLATES[rng.below(TEMPLATES.len())];
            let run = match bind("filters", &keys, &keys, &filters) {
                Ok(r) => r,
                Err(e) => {
                    // Only a value with a control character is refused here.
                    assert!(e.reason.contains("control"), "seed {seed}: {}", e.reason);
                    assert!(filters.iter().any(|f| f.value.chars().any(|c| c < ' ' || c == '\u{7f}')));
                    refused += 1;
                    continue;
                }
            };
            let sql = run_sql(template, Some(&run), Dialect::ClickHouse)
                .unwrap_or_else(|e| panic!("seed {seed}: {template} {filters:?}: {e:?}"));
            // The structure: the text of the template around `(` filters `)`, and each literal decodes to its value.
            let (text, _) = run.render();
            let at = sql.find(&text).unwrap_or_else(|| panic!("seed {seed}: {sql}"));
            let tokens = lex(&sql).unwrap();
            let inside: Vec<_> = tokens.iter().filter(|t| t.start >= at && t.end <= at + text.len()).collect();
            let mut values = Vec::new();
            let mut expect = vec![TokenKind::Punct];
            for (i, f) in filters.iter().enumerate() {
                if i > 0 {
                    expect.extend([TokenKind::Blank, TokenKind::Word, TokenKind::Blank]);
                }
                expect.extend([TokenKind::QuotedName, TokenKind::Blank]);
                expect.extend(std::iter::repeat_n(TokenKind::Punct, f.op.len()));
                expect.extend([TokenKind::Blank, TokenKind::String]);
                values.push(f.value.clone());
            }
            expect.push(TokenKind::Punct);
            let kinds: Vec<TokenKind> = inside.iter().map(|t| t.kind).collect();
            assert_eq!(kinds, expect, "seed {seed}: {sql}");
            let decoded: Vec<String> =
                inside.iter().filter(|t| t.kind == TokenKind::String).map(|t| decode(t.text(&sql))).collect();
            assert_eq!(decoded, values, "seed {seed}: {sql}");
            assert!(inside.iter().all(|t| t.closed));
            ok += 1;
        }
    }
    eprintln!("{ok} runs kept P, {refused} runs refused a value");
    assert!(ok > 1000 && refused > 100, "{ok} {refused}");
}

#[test]
fn no_byte_of_the_request_key_reaches_the_sql() {
    // The configured list and the request have the same key text in different memory. The identifier of the run points
    // into the configured list; a key that differs in any byte is refused, so nothing else of the request is compared.
    let configured = strings(&["host"]);
    let doc_keys = strings(&["host"]);
    let request = vec![rf("host", "=", "a")];
    let run = bind("filters", &doc_keys, &configured, &request).unwrap();
    let column = run.columns()[0];
    let span = configured[0].as_ptr() as usize..configured[0].as_ptr() as usize + configured[0].len();
    assert!(span.contains(&(column.as_ptr() as usize)));
    let key_span = request[0].key.as_ptr() as usize..request[0].key.as_ptr() as usize + request[0].key.len();
    assert!(!key_span.contains(&(column.as_ptr() as usize)));
    for near in ["Host", "host ", " host", "hos", "hostt", "h\u{200b}ost", "ho\u{0}st", "\u{445}ost", "host\u{301}"] {
        assert!(bind("filters", &doc_keys, &configured, &[rf(near, "=", "a")]).is_err(), "{near:?}");
    }
}

// ---- the checks at save ----

fn save(sql: &str, usage: SqlUse, adhoc: AdhocSave<'_>, dialect: Dialect) -> Vec<(ErrorCode, String)> {
    save_checks_adhoc(dialect, sql, usage, &defs(), adhoc).faults.into_iter().map(|f| (f.code, f.message)).collect()
}

const ON: AdhocSave<'static> = AdhocSave { names: &["filters"], on_source: true };

#[test]
fn the_checks_at_save_refuse_the_macro_out_of_its_place() {
    assert!(save("SELECT 1 FROM t WHERE $__adhocFilters", SqlUse::Panel, ON, Dialect::ClickHouse).is_empty());
    assert!(
        save("SELECT 1 FROM t WHERE x = 1\n  AND ${__adhocFilters}", SqlUse::Panel, ON, Dialect::ClickHouse).is_empty()
    );
    let one = |sql: &str, usage: SqlUse, adhoc: AdhocSave<'_>, dialect: Dialect| {
        let f = save(sql, usage, adhoc, dialect);
        assert_eq!(f.len(), 1, "{sql}: {f:?}");
        f.into_iter().next().unwrap()
    };
    let sql = "SELECT 1 FROM t WHERE $__adhocFilters";
    for usage in [SqlUse::Variable, SqlUse::Annotation] {
        let (code, message) = one(sql, usage, ON, Dialect::ClickHouse);
        assert_eq!(code, ErrorCode::NotPermitted, "{message}");
    }
    let (code, message) = one(sql, SqlUse::Panel, AdhocSave::default(), Dialect::ClickHouse);
    assert_eq!(code, ErrorCode::NotPermitted);
    assert!(message.contains("adhoc variable"), "{message}");
    let (code, _) = one(sql, SqlUse::Panel, ON, Dialect::Postgres);
    assert_eq!(code, ErrorCode::NotPermitted);
    let (code, message) = one("SELECT 1 FROM t WHERE x OR $__adhocFilters", SqlUse::Panel, ON, Dialect::ClickHouse);
    assert_eq!(code, ErrorCode::NotPermitted);
    assert_eq!(message, PLACE_MESSAGE);
    let (code, message) =
        one("SELECT 1 FROM t WHERE $__timeFilter(ts AND $__adhocFilters)", SqlUse::Panel, ON, Dialect::ClickHouse);
    assert_eq!(code, ErrorCode::NotPermitted);
    assert!(message.contains("inside another macro"), "{message}");
    assert_eq!(
        one("SELECT 1 FROM t WHERE $__adhocFilters(x)", SqlUse::Panel, ON, Dialect::ClickHouse).0,
        ErrorCode::BadFormat
    );
    assert_eq!(
        one("SELECT 1 FROM t WHERE ${__adhocFilters:raw}", SqlUse::Panel, ON, Dialect::ClickHouse).0,
        ErrorCode::BadFormat
    );
    // A placeholder of the adhoc variable itself (D3): only the macro reaches the filters.
    let (code, message) = one("SELECT 1 FROM t WHERE x = '$filters'", SqlUse::Panel, ON, Dialect::ClickHouse);
    assert_eq!(code, ErrorCode::NotPermitted);
    assert!(message.contains("$__adhocFilters"), "{message}");
    // The save and the run agree: each refusal of the save is a refusal of the run.
    let keys = strings(&["host"]);
    let run = bind("filters", &keys, &keys, &[]).unwrap();
    for sql in [
        "SELECT 1 FROM t WHERE x OR $__adhocFilters",
        "SELECT 1 FROM t WHERE $__timeFilter(ts AND $__adhocFilters)",
        "SELECT 1 FROM t WHERE $__adhocFilters(x)",
    ] {
        assert!(!save(sql, SqlUse::Panel, ON, Dialect::ClickHouse).is_empty(), "{sql}");
        assert!(matches!(run_sql(sql, Some(&run), Dialect::ClickHouse), Err(ExpandError::Invalid(_))), "{sql}");
    }
}

// ---- the place rule (7.10a, rule 4): option A' of the security review 3 ----

include!("adhoc_common/places.rs");

/// The variables of the texts of the lists: a text variable, and the variables of the fixtures.
fn place_defs() -> Vec<VariableDef> {
    vec![
        VariableDef::text("t", TextValueType::Text),
        VariableDef::text("env", TextValueType::Text),
        VariableDef::query("node", true, false, None),
    ]
}

fn place_run(sql: &str, run: &AdhocRun<'_>) -> Result<String, ExpandError> {
    let mut values = RequestValues::new();
    values.insert("t", false, &["x"]);
    values.insert("env", false, &["prod"]);
    values.insert("node", false, &["v1", "v2"]);
    let resolved = resolve_values(&place_defs(), &values).unwrap();
    let range = check_time_range(FROM, TO).unwrap();
    let interval = Some(compute_interval(&range, 1200, 1).unwrap());
    let req = ExpandRequest { sql, values: &resolved, range, interval, max_query_size: 1_048_576 };
    expand_prepared_adhoc(Dialect::ClickHouse, &req, Some(run)).map(|p| p.sql)
}

fn place_save(sql: &str) -> Vec<(ErrorCode, String)> {
    save_checks_adhoc(Dialect::ClickHouse, sql, SqlUse::Panel, &place_defs(), ON)
        .faults
        .into_iter()
        .map(|f| (f.code, f.message))
        .collect()
}

#[test]
fn the_save_and_the_run_agree_on_each_place() {
    let keys = strings(&["host"]);
    let none = AdhocRun::empty("filters");
    let one = bind("filters", &keys, &keys, &[rf("host", "=", "web-1")]).unwrap();
    let rule = |sql: &str| {
        let (start, end) = macro_range(sql);
        lex(sql).is_ok_and(|tokens| place_is_permitted(sql, &tokens, start, end))
    };
    for sql in PLACE_ACCEPT {
        assert!(rule(sql), "the rule accepts {sql}");
        assert!(place_save(sql).is_empty(), "the save accepts {sql}: {:?}", place_save(sql));
        let (empty, filtered) = (place_run(sql, &none).unwrap(), place_run(sql, &one).unwrap());
        assert!(empty.contains("1=1"), "{empty}");
        assert!(filtered.contains("(`host` = 'web-1')"), "{filtered}");
    }
    for sql in PLACE_WIDEN {
        assert!(!rule(sql), "the rule refuses {sql}");
        let faults = place_save(sql);
        assert!(
            faults.contains(&(ErrorCode::NotPermitted, PLACE_MESSAGE.to_owned())),
            "the save refuses {sql}: {faults:?}"
        );
        for run in [&none, &one] {
            match place_run(sql, run) {
                Err(ExpandError::Invalid(i)) => {
                    assert_eq!((i.kind, i.message.as_str()), (SqlInvalidKind::Macro, PLACE_MESSAGE), "{sql}")
                }
                other => panic!("the run refuses {sql}: {other:?}"),
            }
        }
    }
    for sql in PLACE_REFUSED {
        assert!(!rule(sql), "the rule refuses {sql}");
        assert!(!place_save(sql).is_empty(), "the save refuses {sql}");
        for run in [&none, &one] {
            assert!(matches!(place_run(sql, run), Err(ExpandError::Invalid(_))), "the run refuses {sql}");
        }
    }
}

/// A text of `MAX_SQL_BYTES` with about 13,000 macros: the place rule reads the text once, so the save and the run
/// stay linear. A search back and forth for each macro would cost the square of the count.
#[test]
fn the_place_rule_is_linear_in_the_count_of_macros() {
    let head = "SELECT count() FROM t WHERE ";
    let term = "$__adhocFilters AND ";
    let count = (hune_core::limits::MAX_SQL_BYTES - head.len() - 16) / term.len();
    let sql = format!("{head}{}$__adhocFilters", term.repeat(count));
    assert!(sql.len() <= hune_core::limits::MAX_SQL_BYTES && count > 13_000, "{} bytes, {count} macros", sql.len());
    let none = AdhocRun::empty("filters");
    let mut best = std::time::Duration::MAX;
    for i in 0..9 {
        let t = std::time::Instant::now();
        assert!(place_save(&sql).is_empty());
        let out = place_run(&sql, &none).unwrap();
        if i >= 2 {
            best = best.min(t.elapsed());
        }
        assert_eq!(out.matches("1=1").count(), count + 1);
    }
    println!(
        "{count} macros in {} bytes: the save and the run take {best:?} (minimum of 7 after 2 warm-ups)",
        sql.len()
    );
    assert!(best < std::time::Duration::from_millis(1500), "{best:?}");
}

#[test]
fn the_expansion_is_linear_in_the_count_of_filters() {
    let keys: Vec<String> = (0..64).map(|i| format!("k{i}")).collect();
    let value = "'\\".repeat(512);
    let filters: Vec<RequestFilter> = (0..16).map(|i| rf(&keys[i], "!=", &value)).collect();
    let run = bind("filters", &keys, &keys, &filters).unwrap();
    let sql = "SELECT 1 FROM t WHERE $__adhocFilters";
    let mut best = std::time::Duration::MAX;
    for i in 0..9 {
        let t = std::time::Instant::now();
        let out = run_sql(sql, Some(&run), Dialect::ClickHouse).unwrap();
        if i >= 2 {
            best = best.min(t.elapsed());
        }
        assert!(out.len() > 16 * 2048);
    }
    assert!(best < std::time::Duration::from_millis(20), "{best:?}");
}

#[test]
fn the_limits_of_4_35_have_their_rows() {
    let doc = include_str!("../../../docs/02-format-and-storage.md");
    use hune_core::limits::{MAX_ADHOC_FILTERS, MAX_ADHOC_KEYS};
    assert!(
        doc.contains(&format!("| `MAX_ADHOC_KEYS` | {MAX_ADHOC_KEYS} |")),
        "docs/02 4.35 has no row MAX_ADHOC_KEYS"
    );
    assert!(
        doc.contains(&format!("| `MAX_ADHOC_FILTERS` | {MAX_ADHOC_FILTERS} |")),
        "docs/02 4.35 has no row MAX_ADHOC_FILTERS"
    );
    assert_eq!((MAX_ADHOC_KEYS, MAX_ADHOC_FILTERS, hune_core::sql::adhoc::MAX_ADHOC_COLUMNS), (64, 16, 256));
}
