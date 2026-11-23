//! The variables, the macros and the expansion of 7.34 and 7.35, with the property PM.

mod mongo_common;

use hune_core::mongo::{MongoExpandError, MongoQuery, MongoValue, QueryBody, to_query_json};
use hune_core::sql::{RequestValues, TextValueType, VariableDef};
use mongo_common::*;

/// Runs `{"$match": {"x": <leaf>}}` and gives the value of `x` of the expanded tree.
fn leaf_with(leaf: &str, defs: &[VariableDef], req: &RequestValues) -> Result<MongoValue, MongoExpandError> {
    let text = format!(r#"{{"collection":"node_load","aggregate":[{{"$match":{{"x":{leaf}}}}}]}}"#);
    let q = run_with(&text, defs, req, false)?;
    Ok(x_of(&q))
}

fn x_of(q: &MongoQuery) -> MongoValue {
    let QueryBody::Aggregate(stages) = &q.body else { panic!() };
    stages[0].get("$match").unwrap().get("x").unwrap().clone()
}

fn leaf(leaf: &str, req: &RequestValues) -> Result<MongoValue, MongoExpandError> {
    leaf_with(leaf, &defs(), req)
}

fn s(t: &str) -> MongoValue {
    MongoValue::String(t.to_owned())
}

fn refused_reason(e: MongoExpandError) -> (String, String) {
    match e {
        MongoExpandError::ValueRefused(v) => (v.variable, v.reason),
        other => panic!("{other:?}"),
    }
}

const STRUCTURE: &str = "The value changes the structure of the query.";

#[test]
fn the_macros_are_typed_leaves() {
    let none = RequestValues::new();
    assert_eq!(leaf(r#""$__timeFrom""#, &none).unwrap(), MongoValue::DateMs(FROM_MS));
    assert_eq!(leaf(r#""$__timeTo""#, &none).unwrap(), MongoValue::DateMs(TO_MS));
    assert_eq!(leaf(r#""${__timeFrom}""#, &none).unwrap(), MongoValue::DateMs(FROM_MS));
    assert_eq!(leaf(r#""$__from""#, &none).unwrap(), MongoValue::Int64(FROM_MS));
    assert_eq!(leaf(r#""$__to""#, &none).unwrap(), MongoValue::Int64(TO_MS));
    // The interval of 7.4 for a range of 6 hours and 1200 points: R = 18,000 ms, so I = 20,000 ms, S = 20.
    assert_eq!(leaf(r#""$__interval_ms""#, &none).unwrap(), MongoValue::Int64(20_000));
    assert_eq!(leaf(r#""$__interval_s""#, &none).unwrap(), MongoValue::Int64(20));
    // In the argument of an operator.
    let q = run_plain(r#"{"collection":"node_load","aggregate":[{"$addFields":{"t":{"$dateTrunc":{"date":"$ts","unit":"second","binSize":"$__interval_s"}}}}]}"#).unwrap();
    assert!(to_query_json(&q.to_value()).contains(r#""binSize":{"$numberLong":"20"}"#));
    // A macro name inside a longer string stays text.
    assert_eq!(leaf(r#""at $__timeFrom""#, &none).unwrap(), s("at $__timeFrom"));
    // An inserted value is never read again: a value that holds a macro name stays text.
    let mut r = RequestValues::new();
    r.insert("host", false, &["$__timeFrom"]);
    assert_eq!(refused_reason(leaf(r#""$host""#, &r).unwrap_err()).1, STRUCTURE);
    assert_eq!(leaf(r#""a${host}""#, &r).unwrap(), s("a$__timeFrom"));
}

#[test]
fn the_interval_macros_are_an_error_in_a_variable_query() {
    let text = r#"{"collection":"node_load","aggregate":[{"$match":{"x":"$__interval_s"}}]}"#;
    let e = run_with(text, &defs(), &RequestValues::new(), true).unwrap_err();
    assert_eq!(e.api_code(), "sql_invalid");
    let text = r#"{"collection":"node_load","aggregate":[{"$match":{"x":"$__timeFrom"}}]}"#;
    assert!(run_with(text, &defs(), &RequestValues::new(), true).is_ok());
}

#[test]
fn whole_leaf_text_format() {
    let r = request(&[("host", false, &["a.b"]), ("one", false, &["x"]), ("env", false, &["$dev"])]);
    assert_eq!(leaf(r#""$host""#, &r).unwrap(), s("a.b"));
    assert_eq!(leaf(r#""${host}""#, &r).unwrap(), s("a.b"));
    assert_eq!(leaf(r#""${host:text}""#, &r).unwrap(), s("a.b"));
    // The empty selection of a `query` variable that is not `multi` is the empty text.
    let e = request(&[("one", false, &[])]);
    assert_eq!(leaf(r#""${one}""#, &e).unwrap(), s(""));
    // A value that is not a string leaf stays text: a JSON text, quote marks, braces.
    for v in [r#"{"$out":"x"}"#, r#"a"b"#, "a\\b", "{", "}", "\u{2028}", "$", "a$", "1", "-1", " $a"] {
        let r = request(&[("host", false, &[v])]);
        let expect = if v == "$" { Err(()) } else { Ok(s(v)) };
        match (leaf(r#""$host""#, &r), expect) {
            (Ok(got), Ok(want)) => assert_eq!(got, want, "{v:?}"),
            (Err(e), Err(())) => assert_eq!(refused_reason(e).1, STRUCTURE),
            (got, want) => panic!("{v:?}: {got:?} {want:?}"),
        }
    }
    // PM2: a free value that starts with `$` is refused, also `$$` and `$secret`.
    for v in ["$secret", "$$ROOT", "$$NOW", "$a.b", "$"] {
        let r = request(&[("host", false, &[v])]);
        assert_eq!(refused_reason(leaf(r#""$host""#, &r).unwrap_err()), ("host".into(), STRUCTURE.into()), "{v}");
        assert_eq!(refused_reason(leaf(r#""${host:text}""#, &r).unwrap_err()).1, STRUCTURE);
    }
    // A value of a `custom` variable is author text: it can start with `$`.
    assert_eq!(leaf(r#""$env""#, &r).unwrap(), s("$dev"));
}

#[test]
fn whole_leaf_number_format() {
    let n = |v: &str| leaf(r#""${n:number}""#, &request(&[("n", false, &[v])]));
    assert_eq!(n("12").unwrap(), MongoValue::Int32(12));
    assert_eq!(n("-12").unwrap(), MongoValue::Int32(-12));
    assert_eq!(n("-0").unwrap(), MongoValue::Int32(0));
    assert_eq!(n("007").unwrap(), MongoValue::Int32(7));
    assert_eq!(n("2147483647").unwrap(), MongoValue::Int32(i32::MAX));
    assert_eq!(n("2147483648").unwrap(), MongoValue::Int64(2_147_483_648));
    assert_eq!(n("9223372036854775807").unwrap(), MongoValue::Int64(i64::MAX));
    assert_eq!(n("9223372036854775808").unwrap(), MongoValue::Double(9_223_372_036_854_775_808.0));
    assert_eq!(n("12.5").unwrap(), MongoValue::Double(12.5));
    assert_eq!(n("12.0").unwrap(), MongoValue::Double(12.0));
    for bad in ["12abc", "", "1e3", "+1", ".5", "5.", "0x1f", " 1", "NaN", "Infinity", "$a", "1,5", "\u{661}"] {
        let e = n(bad).unwrap_err();
        assert_eq!(refused_reason(e), ("n".into(), "The value is not a number.".into()), "{bad:?}");
    }
    // The text variable accepts any text, so the format decides.
    let defs = [VariableDef::text("n", TextValueType::Number)];
    let q = leaf_with(r#""${n:number}""#, &defs, &request(&[("n", false, &["3"])])).unwrap();
    assert_eq!(q, MongoValue::Int32(3));
}

#[test]
fn whole_leaf_date_oid_bool_formats() {
    let f = |fmt: &str, v: &str| leaf(&format!(r#""${{n:{fmt}}}""#), &request(&[("n", false, &[v])]));
    assert_eq!(f("date", "2026-11-09T14:00:00Z").unwrap(), MongoValue::DateMs(TO_MS));
    assert_eq!(f("date", "2026-11-09T14:00:00.250Z").unwrap(), MongoValue::DateMs(TO_MS + 250));
    assert_eq!(f("date", "1794232800000").unwrap(), MongoValue::DateMs(TO_MS));
    assert_eq!(f("date", "0").unwrap(), MongoValue::DateMs(0));
    for bad in [
        "2026-11-09",
        "2026-11-09T14:00:00",
        "2026-11-09T14:00:00+01:00",
        "2026-02-30T00:00:00Z",
        "12345678901234567",
        "-5",
        "",
        "x",
        "$a",
    ] {
        assert_eq!(refused_reason(f("date", bad).unwrap_err()).1, "The value is not a date.", "{bad:?}");
    }
    assert!(matches!(f("oid", "0123456789ABCDEF01234567").unwrap(), MongoValue::ObjectId(_)));
    for bad in ["0123", "0123456789abcdef0123456g", "", "$0123456789abcdef0123456"] {
        assert_eq!(refused_reason(f("oid", bad).unwrap_err()).1, "The value is not an ObjectId.", "{bad:?}");
    }
    assert_eq!(f("bool", "true").unwrap(), MongoValue::Bool(true));
    assert_eq!(f("bool", "false").unwrap(), MongoValue::Bool(false));
    for bad in ["True", "1", "", "yes", "true "] {
        assert_eq!(refused_reason(f("bool", bad).unwrap_err()).1, "The value is not true or false.", "{bad:?}");
    }
}

#[test]
fn whole_leaf_list_formats() {
    let r = request(&[("hosts", false, &["a", "b'c", "$d"])]);
    // `hosts` is a `multi` query variable, so a list.
    let mut ok = RequestValues::new();
    ok.insert("hosts", false, &["a", "b'c"]);
    assert_eq!(leaf(r#""${hosts:json}""#, &ok).unwrap(), MongoValue::Array(vec![s("a"), s("b'c")]));
    // PM2 holds for each item.
    assert_eq!(refused_reason(leaf(r#""${hosts:json}""#, &r).unwrap_err()).1, STRUCTURE);
    let mut empty = RequestValues::new();
    empty.insert("hosts", false, &[]);
    assert_eq!(leaf(r#""${hosts:json}""#, &empty).unwrap(), MongoValue::Array(vec![]));
    // All.
    let mut all = RequestValues::new();
    all.insert("hosts", true, &["x", "y"]);
    assert_eq!(leaf(r#""${hosts:json}""#, &all).unwrap(), MongoValue::Array(vec![s("x"), s("y")]));
    // A list with no format, or with a format of one value, is an error.
    for fmt in
        ["$hosts", "${hosts}", "${hosts:text}", "${hosts:number}", "${hosts:date}", "${hosts:oid}", "${hosts:bool}"]
    {
        let e = leaf(&format!("\"{fmt}\""), &ok).unwrap_err();
        assert_eq!(e.api_code(), "sql_invalid", "{fmt}");
    }
    // json_number and json_oid.
    let mut nums = RequestValues::new();
    nums.insert("hosts", false, &["1", "2.5", "3000000000"]);
    assert_eq!(
        leaf(r#""${hosts:json_number}""#, &nums).unwrap(),
        MongoValue::Array(vec![MongoValue::Int32(1), MongoValue::Double(2.5), MongoValue::Int64(3_000_000_000)])
    );
    let mut bad = RequestValues::new();
    bad.insert("hosts", false, &["1", "x"]);
    assert_eq!(refused_reason(leaf(r#""${hosts:json_number}""#, &bad).unwrap_err()).1, "A value is not a number.");
    let mut oids = RequestValues::new();
    oids.insert("hosts", false, &["0123456789abcdef01234567"]);
    assert!(
        matches!(leaf(r#""${hosts:json_oid}""#, &oids).unwrap(), MongoValue::Array(a) if a.len() == 1 && matches!(a[0], MongoValue::ObjectId(_)))
    );
    assert_eq!(refused_reason(leaf(r#""${hosts:json_oid}""#, &nums).unwrap_err()).1, "A value is not an ObjectId.");
    // One value in a json format is a list of one. A `custom` list gives the options.
    let one = request(&[("host", false, &["x"])]);
    assert_eq!(leaf(r#""${host:json}""#, &one).unwrap(), MongoValue::Array(vec![s("x")]));
    let mut envs = RequestValues::new();
    envs.insert("envs", true, &[]);
    assert_eq!(leaf(r#""${envs:json}""#, &envs).unwrap(), MongoValue::Array(vec![s("prod"), s("dev")]));
}

#[test]
fn the_array_of_a_format_is_one_element_of_its_array() {
    let mut r = RequestValues::new();
    r.insert("hosts", false, &["a", "b"]);
    let q = run(r#"{"collection":"node_load","aggregate":[{"$match":{"host":{"$in":"${hosts:json}"}}}]}"#, &r).unwrap();
    assert_eq!(
        to_query_json(&q.to_value()),
        r#"{"collection":"node_load","aggregate":[{"$match":{"host":{"$in":["a","b"]}}}]}"#
    );
    // Inside another array, the array stays one element (PM1).
    let q = run(r#"{"collection":"node_load","aggregate":[{"$match":{"host":{"$in":["${hosts:json}","z"]}}}]}"#, &r)
        .unwrap();
    assert_eq!(
        to_query_json(&q.to_value()),
        r#"{"collection":"node_load","aggregate":[{"$match":{"host":{"$in":[["a","b"],"z"]}}}]}"#
    );
}

#[test]
fn embedded_placeholders_give_a_string() {
    let r = request(&[("host", false, &["a.b"]), ("one", false, &["c d"])]);
    assert_eq!(leaf(r#""x-$host-y""#, &r).unwrap(), s("x-a.b-y"));
    assert_eq!(leaf(r#""x-${host}-${one}""#, &r).unwrap(), s("x-a.b-c d"));
    assert_eq!(leaf(r#""x-${host:raw}""#, &r).unwrap(), s("x-a.b"));
    assert_eq!(leaf(r#""x-${host:csv}""#, &r).unwrap(), s("x-a.b"));
    // The spec example of 7.35.
    assert_eq!(leaf(r#""^${host:regex}""#, &r).unwrap(), s("^a\\.b"));
    assert_eq!(leaf(r#""x${host:json}""#, &r).unwrap(), s("xa.b"));
    assert_eq!(leaf(r#""x${host:pipe}""#, &r).unwrap(), s("xa.b"));
    assert_eq!(leaf(r#""x${host:html}""#, &request(&[("host", false, &["<a>"])])).unwrap(), s("x&lt;a&gt;"));
    assert_eq!(leaf(r#""x${host:doublequote}""#, &request(&[("host", false, &["a\"b"])])).unwrap(), s("x\"a\\\"b\""));
    assert_eq!(leaf(r#""x${host:percentencode}""#, &request(&[("host", false, &["a b"])])).unwrap(), s("xa%20b"));
    assert_eq!(leaf(r#""x${host:lucene}""#, &request(&[("host", false, &["a:b"])])).unwrap(), s("xa\\:b"));
    // A value that starts with `$` is text in the middle of a string: PM2 has nothing to protect.
    assert_eq!(leaf(r#""x-$host""#, &request(&[("host", false, &["$secret"])])).unwrap(), s("x-$secret"));
    // Lists.
    let mut l = RequestValues::new();
    l.insert("hosts", false, &["a.b", "c"]);
    assert_eq!(leaf(r#""x${hosts:csv}""#, &l).unwrap(), s("xa.b,c"));
    assert_eq!(leaf(r#""^${hosts:regex}$""#, &l).unwrap(), s("^(a\\.b|c)$"));
    assert_eq!(leaf(r#""x${hosts:json}x""#, &l).unwrap(), s("x[\"a.b\",\"c\"]x"));
    // A string that starts with a placeholder starts with `$`: the grammar refuses it (7.34).
    assert_eq!(leaf(r#""${hosts:json}x""#, &l).unwrap_err().api_code(), "sql_invalid");
    assert_eq!(leaf(r#""${hosts:csv}""#, &l).unwrap_err().api_code(), "sql_invalid");
    assert_eq!(leaf(r#""x${hosts:pipe}""#, &l).unwrap(), s("xa.b|c"));
    let mut e = RequestValues::new();
    e.insert("hosts", false, &[]);
    assert_eq!(leaf(r#""x${hosts:csv}""#, &e).unwrap(), s("x"));
    // All with all_value is the author text.
    let defs = [VariableDef::query("v", true, true, Some(".*"))];
    let mut all = RequestValues::new();
    all.insert("v", true, &[]);
    assert_eq!(leaf_with(r#""^${v:regex}$""#, &defs, &all).unwrap(), s("^.*$"));
    assert_eq!(leaf_with(r#""^${v:raw}$""#, &defs, &all).unwrap(), s("^.*$"));
    // A list with no format or with `raw`, and the formats that 7.34 refuses.
    for f in ["$hosts", "${hosts}", "${hosts:raw}"] {
        assert_eq!(leaf(&format!("\"x{f}\""), &l).unwrap_err().api_code(), "sql_invalid", "{f}");
    }
    for f in ["singlequote", "sqlstring", "number", "date", "oid", "bool", "json_number", "json_oid", "nosuch"] {
        let t = format!("\"x${{host:{f}}}\"");
        assert_eq!(leaf(&t, &r).unwrap_err().api_code(), "sql_invalid", "{f}");
        assert!(
            save_codes(&format!(r#"{{"collection":"node_load","aggregate":[{{"$match":{{"x":{t}}}}}]}}"#))
                .contains(&hune_core::error::ErrorCode::BadFormat),
            "{f}"
        );
    }
}

#[test]
fn an_undeclared_name_stays_text_and_a_declared_one_needs_a_request_member() {
    let none = RequestValues::new();
    assert_eq!(leaf(r#""$cpu""#, &none).unwrap(), s("$cpu"));
    assert_eq!(leaf(r#""x ${cpu} y""#, &none).unwrap(), s("x ${cpu} y"));
    assert_eq!(leaf(r#""x ${cpu:json} y""#, &none).unwrap(), s("x ${cpu:json} y"));
    assert_eq!(leaf(r#""x ${ y""#, &none).unwrap(), s("x ${ y"));
    let e = leaf(r#""$host""#, &none).unwrap_err();
    assert_eq!(e, MongoExpandError::MissingVariable("host".into()));
    assert_eq!(e.api_code(), "bad_request");
    let e = leaf(r#""x${host}""#, &none).unwrap_err();
    assert_eq!(e.api_code(), "bad_request");
    // A field path that a value completes is refused by the grammar.
    let r = request(&[("host", false, &["a"])]);
    assert_eq!(leaf(r#""$a_${host}""#, &r).unwrap_err().api_code(), "sql_invalid");
    // The reference form names the field of a variable name.
    assert_eq!(leaf(r#""$$CURRENT.host""#, &none).unwrap(), s("$$CURRENT.host"));
}

#[test]
fn hostile_values_stay_in_their_leaf() {
    let hostile = [
        "$",
        "$$ROOT",
        "$secret",
        ".",
        r#"{"$out": "x"}"#,
        r#"["$secret"]"#,
        "\"",
        "'",
        "\\",
        "{",
        "}",
        "\u{2028}",
        "\u{1f600}",
        "$__timeFrom",
        "${host}",
        "$host",
        "a$b",
        &"x".repeat(1024),
    ];
    for v in hostile {
        let r = request(&[("host", false, &[v])]);
        for form in [
            r#""$host""#,
            r#""${host}""#,
            r#""${host:text}""#,
            r#""p${host}s""#,
            r#""p${host:json}""#,
            r#""p${host:regex}""#,
        ] {
            let text = format!(
                r#"{{"collection":"node_load","aggregate":[{{"$match":{{"x":{form},"y":{{"$in":[{form}]}}}}}}]}}"#
            );
            match run(&text, &r) {
                Ok(q) => {
                    let QueryBody::Aggregate(st) = &q.body else { panic!() };
                    // The shape of the tree is the shape of the text.
                    let m = st[0].get("$match").unwrap();
                    assert_eq!(m.as_document().unwrap().len(), 2);
                    assert_eq!(m.get("y").unwrap().as_document().unwrap().len(), 1);
                    // A form that is exactly one placeholder (it starts with `$`) gives the value as it is.
                    if let MongoValue::String(x) = m.get("x").unwrap()
                        && form.starts_with("\"$")
                    {
                        assert_eq!(x, v);
                        assert!(!x.starts_with('$'));
                    }
                }
                Err(e) => {
                    assert!(matches!(e, MongoExpandError::ValueRefused(_)), "{v:?} {form}: {e:?}");
                    assert!(v.starts_with('$'));
                }
            }
        }
    }
}

#[test]
fn size_and_count_limits() {
    // A command above max_query_size gives sql_too_large.
    let values = resolved(&RequestValues::new());
    let range = hune_core::sql::check_time_range(FROM_MS, TO_MS).unwrap();
    let (c, d) = (collections(), databases());
    let text = r#"{"collection":"node_load","aggregate":[{"$match":{"a":"0123456789"}}]}"#;
    let req = |max| hune_core::mongo::MongoExpandRequest {
        values: &values,
        range,
        interval: None,
        max_query_size: max,
        collections: &c,
        databases: &d,
    };
    let q = hune_core::mongo::prepare(&[text], &req(10_000)).unwrap();
    let size = hune_core::mongo::bson_size(&q.to_value());
    assert!(hune_core::mongo::prepare(&[text], &req(size)).is_ok());
    match hune_core::mongo::prepare(&[text], &req(size - 1)).unwrap_err() {
        MongoExpandError::TooLarge { size: s, limit } => assert_eq!((s, limit), (size, size - 1)),
        other => panic!("{other:?}"),
    }
    // Values that grow the tree: 10,000 values of 1,024 bytes make about 10 MiB.
    let big = "a".repeat(1024);
    let many: Vec<&str> = vec![big.as_str(); 10_000];
    let mut r = RequestValues::new();
    r.insert("hosts", false, &many);
    let e =
        run(r#"{"collection":"node_load","aggregate":[{"$match":{"x":{"$in":"${hosts:json}"}}}]}"#, &r).unwrap_err();
    assert_eq!(e.api_code(), "sql_too_large");
    // Embedded text above the limit of a string.
    let e = run(r#"{"collection":"node_load","aggregate":[{"$match":{"x":"x${hosts:csv}"}}]}"#, &r).unwrap_err();
    assert!(matches!(e, MongoExpandError::ValueRefused(_) | MongoExpandError::TooLarge { .. }), "{e:?}");
    // A pattern above MAX_REGEX_BYTES after the expansion gives variable_value_refused.
    let long = "a".repeat(1000);
    let r = request(&[("host", false, &[long.as_str()]), ("n", false, &[long.as_str()])]);
    let e =
        run(r#"{"collection":"node_load","aggregate":[{"$match":{"x":{"$regex":"p${host}${n}"}}}]}"#, &r).unwrap_err();
    let (variable, reason) = refused_reason(e);
    assert_eq!(variable, "host");
    assert!(reason.contains("1024"), "{reason}");
    let ok = request(&[("host", false, &["a"]), ("n", false, &["b"])]);
    assert!(run(r#"{"collection":"node_load","aggregate":[{"$match":{"x":{"$regex":"p${host}${n}"}}}]}"#, &ok).is_ok());
}

#[test]
fn a_value_that_does_not_fit_its_place_is_refused_as_a_value() {
    // The type of the value is known after the expansion: $limit takes an integer.
    let q = |v: &str| {
        run(r#"{"collection":"node_load","aggregate":[{"$limit":"${n:number}"}]}"#, &request(&[("n", false, &[v])]))
    };
    assert!(q("10").is_ok());
    assert!(q("3000000").is_ok());
    assert_eq!(refused_reason(q("1.5").unwrap_err()).0, "n");
    assert_eq!(refused_reason(q("0").unwrap_err()).0, "n");
    assert_eq!(refused_reason(q("-1").unwrap_err()).0, "n");
    assert_eq!(refused_reason(q("1000000001").unwrap_err()).0, "n");
    let o = |v: &str| {
        run(
            r#"{"collection":"node_load","aggregate":[{"$match":{"x":{"$regex":"a","$options":"${n}"}}}]}"#,
            &request(&[("n", false, &[v])]),
        )
    };
    assert!(o("i").is_ok());
    assert_eq!(refused_reason(o("u").unwrap_err()).0, "n");
    let sort = |v: &str| {
        run(
            r#"{"collection":"node_load","aggregate":[{"$sort":{"a":"${n:number}"}}]}"#,
            &request(&[("n", false, &[v])]),
        )
    };
    assert!(sort("1").is_ok());
    assert!(sort("-1").is_ok());
    assert_eq!(refused_reason(sort("2").unwrap_err()).0, "n");
}

#[test]
fn the_same_inputs_give_the_same_bytes() {
    let mut r = RequestValues::new();
    r.insert("hosts", false, &["a", "b"]);
    r.insert("host", false, &["x"]);
    let t = r#"{"collection":"node_load","aggregate":[{"$match":{"ts":{"$gte":"$__timeFrom"},"h":{"$in":"${hosts:json}"},"n":"p-${host}"}}]}"#;
    let a = to_query_json(&run(t, &r).unwrap().to_value());
    let b = to_query_json(&run(t, &r).unwrap().to_value());
    assert_eq!(a, b);
    // The expanded text parses again with the profile J and gives the same tree.
    let again = hune_core::mongo::parse_text(&a).unwrap().value;
    assert_eq!(to_query_json(&again), a);
}

#[test]
fn expand_on_a_stored_query_gives_the_same_tree_as_prepare() {
    let t = r#"{"collection":"node_load","aggregate":[{"$match":{"ts":{"$gte":"$__timeFrom"},"n":"p-${host}"}}]}"#;
    let r = request(&[("host", false, &["x"])]);
    let direct = run(t, &r).unwrap();
    let stored = hune_core::mongo::parse(&[t]).unwrap();
    let values = resolved(&r);
    let range = hune_core::sql::check_time_range(FROM_MS, TO_MS).unwrap();
    let interval = Some(hune_core::sql::compute_interval(&range, 1200, 1).unwrap());
    let (c, d) = (collections(), databases());
    let via = hune_core::mongo::expand(
        &stored,
        &hune_core::mongo::MongoExpandRequest {
            values: &values,
            range,
            interval,
            max_query_size: 262_144,
            collections: &c,
            databases: &d,
        },
    )
    .unwrap();
    assert_eq!(direct, via);
    assert_eq!(hune_core::mongo::effective_database(&direct, &d), Some("metrics"));
}
