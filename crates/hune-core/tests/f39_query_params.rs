//! The query parameters of ClickHouse `{name:Type}` (F39, 7.6a, 7.18 items 10 to 16, decision ID-K330).
//!
//! The tests cover the scan (each loose form refused, the literals left as text), the save checks, the run checks
//! (the grammar of each type, the shapes, the caps, the second scan of the final text), the property PQ with seeded
//! random values, and the name scan of 7.19.

use hune_core::document::{dependency_names, referenced_names};
use hune_core::error::ErrorCode;
use hune_core::sql::{
    Dialect, ExpandError, ExpandRequest, MAX_PARAM_BYTES, MAX_PARAM_ITEMS, Prepared, RequestValues, SqlInvalidKind,
    SqlUse, Stage, TextValueType, VariableDef, check_time_range, expand_prepared, resolve_values, save_checks_for,
};

fn defs() -> Vec<VariableDef> {
    vec![
        VariableDef::text("t", TextValueType::Text),
        VariableDef::text("n", TextValueType::Number),
        VariableDef::constant("k", "{x:Identifier}"),
        VariableDef::custom("c", &["a", "b"], false, false, None),
        VariableDef::custom("cm", &["1", "2", "3"], true, false, None),
        VariableDef::custom("ci", &["a", "b"], false, true, None),
        VariableDef::custom("o", &["a"], false, false, None).with_open(true),
        VariableDef::query("q", false, false, None),
        VariableDef::query("qm", true, true, None),
        VariableDef::query("qa", true, true, Some("1,2")),
        VariableDef::custom("ca", &["a"], false, true, Some("'a'")),
        VariableDef::interval("iv", &["1m", "1h"], None),
        VariableDef::custom(
            "forge",
            &["{t:String}", "{t:Identifier}", "{zz:String}", "{ t:String}", "a${t:Identifier}"],
            false,
            false,
            None,
        ),
        VariableDef::text("zz", TextValueType::Text),
    ]
}

/// The values of a request: `(name, all, values)`.
fn req(items: &[(&str, bool, &[&str])]) -> RequestValues {
    let mut r = RequestValues::new();
    for (n, a, v) in items {
        r.insert(n, *a, v);
    }
    r
}

fn run_with(
    dialect: Dialect,
    defs: &[VariableDef],
    sql: &str,
    items: &[(&str, bool, &[&str])],
) -> Result<Prepared, ExpandError> {
    let values = resolve_values(defs, &req(items)).expect("the values of 7.7");
    expand_prepared(
        dialect,
        &ExpandRequest {
            sql,
            values: &values,
            range: check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap(),
            interval: None,
            max_query_size: 1_048_576,
        },
    )
}

fn run(sql: &str, items: &[(&str, bool, &[&str])]) -> Result<Prepared, ExpandError> {
    run_with(Dialect::ClickHouse, &defs(), sql, items)
}

/// The faults of the checks at save: the code, the column and the message.
fn save_in(dialect: Dialect, sql: &str) -> Vec<(ErrorCode, usize, String)> {
    save_checks_for(dialect, sql, SqlUse::Panel, &defs())
        .faults
        .into_iter()
        .map(|f| (f.code, f.column, f.message))
        .collect()
}

fn save(sql: &str) -> Vec<(ErrorCode, usize, String)> {
    save_in(Dialect::ClickHouse, sql)
}

fn codes(sql: &str) -> Vec<ErrorCode> {
    save(sql).into_iter().map(|f| f.0).collect()
}

fn pairs(p: &Prepared) -> Vec<(String, String)> {
    p.params.pairs().map(|(n, v)| (n.to_owned(), v.to_owned())).collect()
}

fn refused(e: &ExpandError) -> (String, String) {
    match e {
        ExpandError::ValueRefused(r) => (r.variable.clone(), r.reason.clone()),
        other => panic!("not a refused value: {other:?}"),
    }
}

fn stored_fault(e: &ExpandError) -> (u32, u32) {
    match e {
        ExpandError::Invalid(i) if i.kind == SqlInvalidKind::Parameter && i.stage == Stage::Stored => {
            (i.line.unwrap(), i.column.unwrap())
        }
        other => panic!("not a stored fault of 7.6a: {other:?}"),
    }
}

#[test]
fn a_strict_form_sends_the_value_in_the_url_and_the_sql_keeps_the_form() {
    let sql = "SELECT {t:String} AS t, {n:Float64} AS n, has({cm:Array(UInt64)}, 2) AS h WHERE 1 = {t:String}";
    assert!(save(sql).is_empty(), "{:?}", save(sql));
    let p =
        run(sql, &[("t", false, &["it's \\ a ] , é"]), ("n", false, &["-1.5"]), ("cm", false, &["1", "3"])]).unwrap();
    assert_eq!(p.sql, sql, "the SQL body holds no value");
    assert_eq!(
        pairs(&p),
        [("cm".into(), "[1,3]".into()), ("n".into(), "-1.5".into()), ("t".into(), "it's \\\\ a ] , é".into())],
        "the names in byte order, each name one time"
    );
    assert_eq!(
        p.params.query_string(),
        "&param_cm=%5B1%2C3%5D&param_n=-1.5&param_t=it%27s%20%5C%5C%20a%20%5D%20%2C%20%C3%A9"
    );
}

#[test]
fn each_kind_that_can_be_bound_gives_its_text() {
    let sql = "SELECT {c:String}, {o:String}, {q:Int64}, {qm:Array(String)}, {ci:Array(String)}, {k2:Date}";
    let mut d = defs();
    d.push(VariableDef::constant("k2", "2026-11-22"));
    let items: &[(&str, bool, &[&str])] = &[
        ("c", false, &["b"]),
        ("o", false, &["x'y"]),
        ("q", false, &["-9223372036854775808"]),
        ("qm", false, &["a'b", "c\\d"]),
        ("ci", true, &[]),
    ];
    let p = run_with(Dialect::ClickHouse, &d, sql, items).unwrap();
    assert_eq!(
        pairs(&p),
        [
            ("c".into(), "b".into()),
            ("ci".into(), "['a','b']".into()),
            ("k2".into(), "2026-11-22".into()),
            ("o".into(), "x'y".into()),
            ("q".into(), "-9223372036854775808".into()),
            ("qm".into(), "['a\\'b','c\\\\d']".into()),
        ]
    );
    // An Array type takes the one value of a variable without `multi`, and no value gives `[]`.
    let p = run("SELECT {ci:Array(String)}", &[("ci", false, &["a"])]).unwrap();
    assert_eq!(pairs(&p), [("ci".into(), "['a']".into())]);
    let p = run("SELECT {qm:Array(String)}", &[("qm", false, &[])]).unwrap();
    assert_eq!(pairs(&p), [("qm".into(), "[]".into())]);
}

#[test]
fn each_loose_form_is_refused_at_save_and_at_run() {
    for (sql, column) in [
        ("SELECT { t:String}", 8),
        ("SELECT {t :String}", 8),
        ("SELECT {t: String}", 8),
        ("SELECT {t:String }", 8),
        ("SELECT {t:Array( String)}", 8),
        ("SELECT {/* c */t:String}", 8),
        ("SELECT {t/* c */:String}", 8),
        ("SELECT {-- c\nt:String}", 8),
        ("SELECT 1,\n  {t # c\n:String}", 3),
        ("SELECT {`t`:String}", 8),
        ("SELECT {\"t\":String}", 8),
        ("SELECT {\u{a0}t:String}", 8),
        ("SELECT {t\u{2003}:String}", 8),
        ("SELECT {$t:String}", 8),
        ("SELECT {t$x:String}", 8),
        ("SELECT {t:String", 8),
    ] {
        let faults = save(sql);
        assert!(
            faults.iter().any(|f| f.0 == ErrorCode::BadFormat && f.1 == column && f.2.contains("exactly {name:Type}")),
            "{sql:?}: {faults:?}"
        );
        let e = run(sql, &[("t", false, &["x"])]).unwrap_err();
        assert_eq!(stored_fault(&e).1, column as u32, "{sql:?}");
        assert!(e.is_document_fault(), "{sql:?}");
    }
}

#[test]
fn literals_comments_heredocs_and_placeholders_leave_the_form_as_text() {
    for sql in [
        "SELECT '{t:String}'",
        "SELECT 1 AS `{t:String}`",
        "SELECT 1 -- {t:String}",
        "SELECT 1 /* {t:String} */",
        "SELECT $h${t:String}$h$",
        "SELECT {'a': 1}",
        "SELECT format('{} {}', 1, 2)",
    ] {
        assert!(save(sql).is_empty(), "{sql:?}: {:?}", save(sql));
        let p = run(sql, &[]).unwrap();
        assert_eq!(p.sql, sql);
        assert!(p.params.is_empty(), "{sql:?}");
    }
    // `${name:format}` is a placeholder of 7.6: `${t:String}` is a format fault, not a query parameter.
    let faults = save("SELECT ${t:String}");
    assert!(faults.iter().all(|f| !f.2.contains("query parameter")), "{faults:?}");
    assert!(faults.iter().any(|f| f.2.contains("not in the list of formats")), "{faults:?}");
}

#[test]
fn the_save_items_11_to_16() {
    let one = |sql: &str, code: ErrorCode, words: &str| {
        let faults = save(sql);
        assert!(faults.iter().any(|f| f.0 == code && f.2.contains(words)), "{sql:?}: {faults:?}");
    };
    // 11: a type outside the list.
    for ty in [
        "Identifier",
        "DateTime",
        "DateTime64(3)",
        "Nullable(String)",
        "LowCardinality(String)",
        "UUID",
        "Bool",
        "string",
        "Int",
    ] {
        one(&format!("SELECT {{t:{ty}}}"), ErrorCode::NotPermitted, "is not permitted");
    }
    // 12: a name that is not a variable of the document.
    one("SELECT {nope:String}", ErrorCode::BadReference, "is not a variable of the document");
    // 13: the refused variables.
    one("SELECT {iv:String}", ErrorCode::NotPermitted, "interval variable");
    one("SELECT {qa:Array(String)}", ErrorCode::NotPermitted, "all_value");
    one("SELECT {ca:Array(String)}", ErrorCode::NotPermitted, "all_value");
    one("SELECT {__from:Int64}", ErrorCode::NotPermitted, "macro");
    // 14: the shape.
    one("SELECT {t:Array(String)}", ErrorCode::NotPermitted, "gives one value");
    one("SELECT {q:Array(Int64)}", ErrorCode::NotPermitted, "gives one value");
    one("SELECT {cm:UInt64}", ErrorCode::NotPermitted, "use an Array type");
    one("SELECT {ci:String}", ErrorCode::NotPermitted, "use an Array type");
    one("SELECT {qm:String}", ErrorCode::NotPermitted, "use an Array type");
    // 15: two types, both forms, 33 names.
    one("SELECT {t:String}, {t:Int64}", ErrorCode::NotPermitted, "two types");
    one("SELECT {t:String}, '$t'", ErrorCode::NotPermitted, "same text");
    one("SELECT {t:String}, ${t}", ErrorCode::NotPermitted, "same text");
    one("SELECT 1 WHERE $__conditionalAll(x IN {qm:Array(String)}, $qm)", ErrorCode::NotPermitted, "same text");
    // A placeholder in a comment is no use, and another variable in the text form is permitted.
    assert!(save("SELECT {t:String} -- $t").is_empty());
    assert!(save("SELECT {t:String}, '$c', ${n}").is_empty(), "{:?}", save("SELECT {t:String}, '$c', ${n}"));
    // 16: a `postgres` source refuses each candidate.
    for sql in ["SELECT {t:String}", "SELECT { t : String }", "SELECT {t:Identifier}"] {
        let faults = save_in(Dialect::Postgres, sql);
        assert!(
            faults.iter().any(|f| f.0 == ErrorCode::NotPermitted && f.2.contains("ClickHouse source")),
            "{faults:?}"
        );
        let e = run_with(Dialect::Postgres, &defs(), sql, &[("t", false, &["x"])]).unwrap_err();
        assert_eq!(stored_fault(&e), (1, 8), "{sql:?}");
    }
}

#[test]
fn a_text_has_32_names_at_most() {
    let mut d = defs();
    let names: Vec<String> = (0..33).map(|i| format!("p{i:02}")).collect();
    for n in &names {
        d.push(VariableDef::text(n, TextValueType::Text));
    }
    let text = |count: usize| {
        let forms: Vec<String> = names[..count].iter().map(|n| format!("{{{n}:String}}")).collect();
        format!("SELECT {}", forms.join(", "))
    };
    let items: Vec<(&str, bool, &[&str])> = names.iter().map(|n| (n.as_str(), false, &["v"] as &[&str])).collect();
    let checks = |sql: &str| save_checks_for(Dialect::ClickHouse, sql, SqlUse::Panel, &d).faults;
    assert!(checks(&text(32)).is_empty());
    let p = run_with(Dialect::ClickHouse, &d, &text(32), &items).unwrap();
    assert_eq!(p.params.len(), 32);
    let faults = checks(&text(33));
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].code, ErrorCode::NotPermitted);
    assert!(faults[0].message.contains("32 query parameters at most"));
    let e = run_with(Dialect::ClickHouse, &d, &text(33), &items).unwrap_err();
    stored_fault(&e);
}

#[test]
fn a_text_full_of_faults_gives_a_bounded_list_in_linear_time() {
    // 20,000 loose forms in a text below `MAX_SQL_BYTES`: the save keeps the faults of the lowest positions and
    // sets its mark `more`, and the check stops early. The minimum of 7 runs after 2 warm-ups (the timing policy).
    let text = format!("SELECT {}", "{ t:String}+".repeat(20_000));
    assert!(text.len() < hune_core::limits::MAX_SQL_BYTES);
    let d = defs();
    let mut best = std::time::Duration::MAX;
    let mut last = save_checks_for(Dialect::ClickHouse, &text, SqlUse::Panel, &d);
    for i in 0..9 {
        let t = std::time::Instant::now();
        last = save_checks_for(Dialect::ClickHouse, &text, SqlUse::Panel, &d);
        if i >= 2 {
            best = best.min(t.elapsed());
        }
    }
    assert!(best < std::time::Duration::from_millis(400), "the check took {best:?}");
    assert_eq!(last.faults.len(), hune_core::limits::MAX_ERRORS - 1);
    assert!(last.more_faults);
    assert!(last.faults.iter().enumerate().all(|(i, f)| f.code == ErrorCode::BadFormat && f.column == 8 + 12 * i));
    // The run stops at the first fault, with its position.
    assert_eq!(stored_fault(&run(&text, &[("t", false, &["v"])]).unwrap_err()), (1, 8));
}

#[test]
fn the_grammar_of_each_type_refuses_before_any_byte_and_never_shows_the_value() {
    let cases: [(&str, &str, &str); 12] = [
        ("SELECT {t:Int64}", "12a", "Int64"),
        ("SELECT {t:Int64}", "9223372036854775808", "Int64"),
        ("SELECT {t:UInt64}", "-1", "UInt64"),
        ("SELECT {t:UInt64}", "18446744073709551616", "UInt64"),
        ("SELECT {t:Float64}", "1e400", "Float64"),
        ("SELECT {t:Float64}", "NaN", "Float64"),
        ("SELECT {t:Date}", "2026-02-30", "Date"),
        ("SELECT {t:Date}", "1969-12-31", "Date"),
        ("SELECT {t:Date}", "2149-06-07", "Date"),
        ("SELECT {qm:Array(Int64)}", "1;DROP", "Int64"),
        ("SELECT {qm:Array(UInt64)}", "0x10", "UInt64"),
        ("SELECT {t:Int64}", "", "Int64"),
    ];
    for (sql, value, ty) in cases {
        let var = if sql.contains("qm") { "qm" } else { "t" };
        let items: &[(&str, bool, &[&str])] = &[(var, false, &[value])];
        let (variable, reason) = refused(&run(sql, items).unwrap_err());
        assert_eq!(variable, var, "{sql} {value:?}");
        assert!(reason.contains(&format!("valid {ty}")), "{sql} {value:?}: {reason}");
        if !value.is_empty() {
            assert!(!reason.contains(value), "the reason holds no value: {reason}");
        }
    }
    // The bounds of each range pass.
    for (sql, value) in [
        ("SELECT {t:Int64}", "9223372036854775807"),
        ("SELECT {t:UInt64}", "18446744073709551615"),
        ("SELECT {t:Float64}", "0.000001"),
        ("SELECT {t:Date}", "1970-01-01"),
        ("SELECT {t:Date}", "2149-06-06"),
    ] {
        let p = run(sql, &[("t", false, &[value])]).unwrap();
        assert_eq!(pairs(&p), [("t".into(), value.into())]);
    }
    // A text variable with the number type keeps its own check of 7.7 first.
    assert!(matches!(
        resolve_values(&defs(), &req(&[("n", false, &["1e3"])])),
        Err(hune_core::sql::ValuesError::Refused(_))
    ));
}

#[test]
fn a_scalar_type_takes_exactly_one_value() {
    // A `query` variable without `multi` can have no value: a scalar type refuses it.
    let (variable, reason) = refused(&run("SELECT {q:String}", &[("q", false, &[])]).unwrap_err());
    assert_eq!((variable.as_str(), reason.as_str()), ("q", "The type String takes exactly one value."));
    // The member of the request is necessary, as for a placeholder (7.7).
    assert_eq!(run("SELECT {t:String}", &[]).unwrap_err(), ExpandError::MissingVariable("t".into()));
}

#[test]
fn the_caps_of_items_and_bytes() {
    let ok: Vec<String> = (0..MAX_PARAM_ITEMS).map(|i| (i % 10).to_string()).collect();
    let refs: Vec<&str> = ok.iter().map(String::as_str).collect();
    assert!(run("SELECT {qm:Array(UInt64)}", &[("qm", false, &refs)]).is_ok());
    let mut more = refs.clone();
    more.push("1");
    let (variable, reason) = refused(&run("SELECT {qm:Array(UInt64)}", &[("qm", false, &more)]).unwrap_err());
    assert_eq!(variable, "qm");
    assert!(
        reason.contains("1000 values at most") && reason.ends_with("use the text form for long lists."),
        "{reason}"
    );
    // The bytes. A value of 1,024 blanks takes 3,072 bytes after the encoding, and `&param_w1=` adds 10 bytes. A
    // second value of 1,004 bytes brings the pairs to 4,096 bytes exactly, and one more byte crosses the cap.
    let blanks = " ".repeat(1024);
    let mut d = defs();
    for n in ["w1", "w2"] {
        d.push(VariableDef::text(n, TextValueType::Text));
    }
    let two = "SELECT {w1:String}, {w2:String}";
    for (len, fits) in [(1004, true), (1005, false)] {
        let a = "a".repeat(len);
        let items: &[(&str, bool, &[&str])] = &[("w1", false, &[&blanks]), ("w2", false, &[&a])];
        match run_with(Dialect::ClickHouse, &d, two, items) {
            Ok(p) => {
                assert!(fits, "{len}");
                assert_eq!(p.params.query_string().len(), MAX_PARAM_BYTES);
            }
            Err(e) => {
                assert!(!fits, "{len}: {e:?}");
                let (variable, reason) = refused(&e);
                assert_eq!(variable, "w2", "the pair that crosses the cap names its variable");
                assert!(reason.contains("4096 bytes at most") && reason.ends_with("long lists."), "{reason}");
            }
        }
    }
}

#[test]
fn a_form_of_the_final_text_must_be_a_pair_of_the_stored_text() {
    // A constant in code puts author text in the SQL: `{x:Identifier}` would name a column of the viewer's choice.
    let sql = "SELECT $k, {t:String}";
    assert!(save(sql).is_empty(), "{:?}", save(sql));
    let e = run(sql, &[("t", false, &["v"])]).unwrap_err();
    assert!(
        matches!(&e, ExpandError::Invalid(i) if i.kind == SqlInvalidKind::Parameter && i.stage == Stage::Expanded && i.line.is_none()),
        "{e:?}"
    );
    assert!(!e.is_document_fault());
    // A closed `custom` value can make a new form, a form with another type, or a loose form.
    for value in ["{t:Identifier}", "{zz:String}", "{ t:String}"] {
        let e = run(
            "SELECT $forge, {t:String}",
            &[("forge", false, &[value]), ("t", false, &["v"]), ("zz", false, &["w"])],
        )
        .unwrap_err();
        assert!(matches!(&e, ExpandError::Invalid(i) if i.stage == Stage::Expanded), "{value}: {e:?}");
    }
    // A `{` right after `$` counts in the final text: ClickHouse 26.8 reads `a${t:Identifier}` as the name `a$` and
    // the alias `{t:Identifier}`, so `param_t` would give the alias.
    let e = run("SELECT {t:String} AS v, $forge", &[("forge", false, &["a${t:Identifier}"]), ("t", false, &["v"])])
        .unwrap_err();
    assert!(
        matches!(&e, ExpandError::Invalid(i) if i.kind == SqlInvalidKind::Parameter && i.stage == Stage::Expanded),
        "{e:?}"
    );
    // The same pair as the stored text is the same parameter.
    let p = run("SELECT $forge, {t:String}", &[("forge", false, &["{t:String}"]), ("t", false, &["v"])]).unwrap();
    assert_eq!(p.sql, "SELECT {t:String}, {t:String}");
    assert_eq!(pairs(&p), [("t".into(), "v".into())]);
}

#[test]
fn only_the_names_of_the_final_text_go_to_the_request() {
    let sql = "SELECT 1 WHERE $__conditionalAll(x = {q:Int64}, $qm)";
    assert!(save(sql).is_empty(), "{:?}", save(sql));
    let all = run(sql, &[("qm", true, &["a"]), ("q", false, &["not a number"])]).unwrap();
    assert_eq!(all.sql, "SELECT 1 WHERE 1=1");
    assert!(all.params.is_empty(), "the removed form sends nothing and checks nothing");
    let e = run(sql, &[("qm", false, &["a"]), ("q", false, &["not a number"])]).unwrap_err();
    assert_eq!(refused(&e).0, "q");
    let some = run(sql, &[("qm", false, &["a"]), ("q", false, &["2"])]).unwrap();
    assert_eq!(some.sql, "SELECT 1 WHERE x = {q:Int64}");
    assert_eq!(pairs(&some), [("q".into(), "2".into())]);
    // A macro that copies its argument copies the form, and the name goes one time.
    let p = run("SELECT 1 WHERE $__timeFilter(toDateTime({t:String}))", &[("t", false, &["2026-01-01"])]).unwrap();
    assert_eq!(p.sql.matches("{t:String}").count(), 2);
    assert_eq!(p.params.len(), 1);
}

#[test]
fn save_and_run_agree_on_each_document_fault() {
    for sql in [
        "SELECT {t:String}",
        "SELECT {t: String}",
        "SELECT {t:Identifier}",
        "SELECT {nope:String}",
        "SELECT {iv:String}",
        "SELECT {qa:Array(String)}",
        "SELECT {t:Array(String)}",
        "SELECT {t:String}, {t:Int64}",
        "SELECT {t:String}, '$t'",
        "SELECT {__to:Int64}",
        "SELECT '{t:String}'",
    ] {
        let saved = codes(sql);
        let ran = run(sql, &[("t", false, &["x"]), ("iv", false, &["1m"]), ("qa", true, &[])]);
        assert_eq!(
            !saved.is_empty(),
            ran.as_ref().is_err_and(ExpandError::is_document_fault),
            "{sql:?}: {saved:?} {ran:?}"
        );
    }
}

#[test]
fn the_name_scan_reads_the_name_of_a_query_parameter() {
    let sql = "SELECT {a:String}, '{b:String}', ${c:csv}, {_d1:Array(UInt64)} -- {e:String}\n, x{f:Int64}";
    assert_eq!(referenced_names(sql), ["a", "c", "_d1", "f"]);
    let deps = dependency_names(sql);
    for n in ["a", "c", "_d1", "f"] {
        assert!(deps.iter().any(|d| d == n), "{n}: {deps:?}");
    }
    // `{1:2}` and a map literal name no variable.
    assert!(referenced_names("SELECT {1:2}, {'k': v}").is_empty());
}

// ---- the property PQ (seeded random values) -------------------------------------------------------

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

const PIECES: &[&str] = &[
    "'",
    "\"",
    "`",
    "\\",
    "\\'",
    "$",
    "${t}",
    "$__fromTime",
    "{",
    "}",
    "{t:String}",
    "{x:Identifier}",
    "]",
    "[",
    ",",
    "--",
    "/*",
    "*/",
    "#",
    ";",
    " ",
    "\u{a0}",
    "\u{2028}",
    "é",
    "😀",
    "0",
    "-1",
    "1.5",
    "%",
    "&",
    "=",
    "+",
    "?",
    "a",
    "Z",
    "\t",
];

fn hostile(rng: &mut Rng) -> String {
    let len = rng.below(12);
    (0..len).map(|_| PIECES[rng.below(PIECES.len())]).collect()
}

/// Reads the escaped text of a `String` back: the inverse of the escapes of 7.6a.
fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

#[test]
fn pq_the_final_sql_is_the_same_for_all_values_of_the_variables_of_query_parameters() {
    let seed: u64 = std::env::var("HUNE_TEST_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(39);
    let templates = [
        "SELECT {t:String} AS a, {o:String} AS b, $c AS c WHERE x IN {qm:Array(String)} AND '${c}' != ''",
        "SELECT * FROM t WHERE $__timeFilter(ts) AND name = {t:String} AND tag IN {qm:Array(String)} -- {zz:String}",
        "SELECT {t:String}, $__conditionalAll(y = {o:String}, $c), '{o:String}'",
    ];
    let mut rng = Rng(seed);
    let mut checked = 0;
    for template in templates {
        assert!(save(template).is_empty(), "{template}: {:?}", save(template));
        let mut first: Option<String> = None;
        for _ in 0..400 {
            let t = hostile(&mut rng);
            let o = hostile(&mut rng);
            let list: Vec<String> = (0..rng.below(4)).map(|_| hostile(&mut rng)).collect();
            let list_refs: Vec<&str> = list.iter().map(String::as_str).collect();
            let mut r = RequestValues::new();
            r.insert("t", false, &[t.as_str()]);
            r.insert("o", false, &[o.as_str()]);
            r.insert("qm", false, &list_refs);
            r.insert("c", false, &["a"]);
            // A value with a control character or an empty open value fails 7.7 first: no SQL, no request.
            let Ok(values) = resolve_values(&defs(), &r) else { continue };
            let req = ExpandRequest {
                sql: template,
                values: &values,
                range: check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap(),
                interval: None,
                max_query_size: 1_048_576,
            };
            let p =
                expand_prepared(Dialect::ClickHouse, &req).unwrap_or_else(|e| panic!("{template} {t:?} {o:?}: {e:?}"));
            match &first {
                None => first = Some(p.sql.clone()),
                Some(f) => assert_eq!(&p.sql, f, "PQ: the SQL changed with the values {t:?} {o:?} {list:?}"),
            }
            // Each text reads back to its value, and the encoded pairs hold only the bytes of 8.3.
            for (name, text) in p.params.pairs() {
                match name {
                    "t" => assert_eq!(unescape(text), t),
                    "o" => assert_eq!(unescape(text), o),
                    _ => {}
                }
            }
            let q = p.params.query_string();
            assert!(q.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._~%&=".contains(&b)), "{q}");
            checked += 1;
        }
    }
    assert!(checked > 300, "{checked} cases ran");
}
