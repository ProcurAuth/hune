// SPDX-License-Identifier: MIT OR Apache-2.0
//! The checks at save of 7.36: the items L1 to L9, the pointer of each fault and the warnings 1 to 3.

mod mongo_common;

use hune_core::error::ErrorCode;
use hune_core::mongo::{WarnKind, save_checks};
use hune_core::sql::SqlUse;
use mongo_common::*;

const PANEL: &str = "/sections/0/panels/2/data/query/sql";

#[test]
fn a_valid_query_has_no_fault() {
    let r = save(
        r#"{"collection":"node_load","aggregate":[{"$match":{"ts":{"$gte":"$__timeFrom","$lte":"$__timeTo"},"host":{"$in":"${hosts:json}"}}},{"$limit":10}]}"#,
    );
    assert!(r.faults.is_empty(), "{:?}", r.faults);
    assert!(r.warnings.is_empty());
}

#[test]
fn l1_profile_and_limits() {
    assert_eq!(save_codes("{"), [ErrorCode::BadFormat]);
    assert_eq!(save_codes(r#"{"a":1,"a":2}"#), [ErrorCode::BadFormat]);
    assert_eq!(save_codes(&"[".repeat(33)), [ErrorCode::TooDeep]);
    assert_eq!(save_codes(&format!(r#"{{"{}":1}}"#, "k".repeat(257))), [ErrorCode::TooLarge]);
    assert_eq!(save_codes(r#"{"a":{"$date":"x"}}"#), [ErrorCode::BadFormat]);
    // A fault of J is the only fault: the text has no tree.
    assert_eq!(save_codes(r#"{"collection":"secret","aggregate":[{"$out":"x"},]}"#).len(), 1);
}

#[test]
fn l2_top_object() {
    assert_eq!(save_codes(r#"{"aggregate":[{"$limit":1}]}"#), [ErrorCode::BadFormat]);
    assert_eq!(save_codes(r#"{"collection":"hosts"}"#), [ErrorCode::BadFormat]);
    assert_eq!(save_codes(r#"{"collection":"hosts","aggregate":[],"find":{}}"#).len(), 2);
}

#[test]
fn l3_names() {
    assert_eq!(save_codes(r#"{"collection":"hosts","aggregate":[{"$out":"x"}]}"#), [ErrorCode::NotPermitted]);
    assert_eq!(
        save_codes(r#"{"collection":"hosts","aggregate":[{"$match":{"$where":"1"}}]}"#),
        [ErrorCode::NotPermitted]
    );
}

#[test]
fn l4_shapes() {
    assert_eq!(save_codes(r#"{"collection":"hosts","aggregate":[{"$limit":0}]}"#), [ErrorCode::BadFormat]);
    assert_eq!(save_codes(r#"{"collection":"hosts","aggregate":[{"$limit":1,"$skip":1}]}"#), [ErrorCode::BadFormat]);
}

#[test]
fn l5_unknown_macro() {
    assert_eq!(save_codes(r#"{"collection":"hosts","aggregate":[{"$match":{"a":"$__now"}}]}"#), [ErrorCode::BadFormat]);
    assert_eq!(
        save_codes(r#"{"collection":"hosts","aggregate":[{"$match":{"a":"${__now}"}}]}"#),
        [ErrorCode::BadFormat]
    );
}

#[test]
fn l6_placeholder_in_a_key_a_name_place_or_a_dollar_string() {
    for t in [
        r#"{"collection":"hosts","aggregate":[{"$match":{"${host}":1}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$unwind":"$host"}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$addFields":{"x":"$a_${host}"}}]}"#,
    ] {
        assert_eq!(save_codes(t), [ErrorCode::NotPermitted], "{t}");
    }
}

#[test]
fn l7_formats() {
    // A format that 7.34 does not list for the place.
    for t in [
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"${host:csv}"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"${host:nosuch}"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"x${host:number}"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"x${host:singlequote}"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"x${host:sqlstring}"}}]}"#,
        // A list with no format in a whole-leaf placeholder, and in an embedded one.
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"${hosts}"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"$hosts"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"x${hosts}"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"x${hosts:raw}"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"${hosts:number}"}}]}"#,
    ] {
        assert_eq!(save_codes(t), [ErrorCode::BadFormat], "{t}");
    }
    // A variable that is not `multi` takes a one-value format. A `multi` variable takes a list format.
    for t in [
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"${one}","b":"${host:number}","c":"x${hosts:csv}","d":"${hosts:json_number}"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"x${hosts:regex}y","b":"${envs:json}"}}]}"#,
    ] {
        assert!(save(t).faults.is_empty(), "{t}: {:?}", save(t).faults);
    }
}

#[test]
fn l8_collections_and_databases() {
    assert_eq!(save_codes(r#"{"collection":"secret","find":{}}"#), [ErrorCode::BadReference]);
    assert_eq!(save_codes(r#"{"collection":"hosts","database":"admin","find":{}}"#), [ErrorCode::BadReference]);
    assert_eq!(
        save_codes(
            r#"{"collection":"hosts","aggregate":[{"$lookup":{"from":"x","localField":"a","foreignField":"b","as":"c"}}]}"#
        ),
        [ErrorCode::BadReference]
    );
    // The check uses the lists of the call.
    let r = hune_core::mongo::save_checks(
        &[r#"{"collection":"secret","find":{}}"#],
        &hune_core::mongo::SaveContext {
            variables: &[],
            collections: &["secret".to_owned()],
            databases: &[],
            usage: SqlUse::Panel,
        },
    );
    assert!(r.faults.is_empty());
}

#[test]
fn l9_interval_macros_in_a_variable_query() {
    for m in ["$__interval_ms", "$__interval_s", "${__interval_s}"] {
        let t = format!(r#"{{"collection":"hosts","aggregate":[{{"$match":{{"a":"{m}"}}}}]}}"#);
        let r = save_with(&t, &defs(), SqlUse::Variable);
        assert_eq!(r.faults.iter().map(|f| f.code).collect::<Vec<_>>(), [ErrorCode::NotPermitted], "{m}");
        assert!(save_with(&t, &defs(), SqlUse::Panel).faults.is_empty());
    }
    let t = r#"{"collection":"hosts","aggregate":[{"$match":{"a":"$__timeFrom","b":"$__from"}}]}"#;
    assert!(save_with(t, &defs(), SqlUse::Variable).faults.is_empty());
}

#[test]
fn l9_interval_macros_in_an_annotation_query() {
    // An annotation has no interval, as a variable (4.5b rule 3, 7.36 L9). The message names the annotation.
    for m in ["$__interval_ms", "$__interval_s", "${__interval_ms}"] {
        let t = format!(r#"{{"collection":"hosts","aggregate":[{{"$match":{{"a":"{m}"}}}}]}}"#);
        let r = save_with(&t, &defs(), SqlUse::Annotation);
        assert_eq!(r.faults.iter().map(|f| f.code).collect::<Vec<_>>(), [ErrorCode::NotPermitted], "{m}");
        assert!(r.faults[0].message.ends_with("is not permitted in the query of an annotation"), "{:?}", r.faults);
    }
    let r = save_with(r#"{"collection":"hosts","aggregate":[{"$limit":"$__interval_s"}]}"#, &defs(), SqlUse::Variable);
    assert!(r.faults[0].message.ends_with("in the query of a variable"), "{:?}", r.faults);
    // The macros of the range stay valid in an annotation.
    let t = r#"{"collection":"hosts","aggregate":[{"$match":{"a":{"$gte":"$__timeFrom","$lt":"$__timeTo"}}}]}"#;
    assert!(save_with(t, &defs(), SqlUse::Annotation).faults.is_empty());
}

#[test]
fn the_pointer_and_the_position_name_the_sql_line() {
    let lines = [
        "{",
        "  \"collection\": \"hosts\",",
        "  \"aggregate\": [",
        "    {\"$limit\": 5},",
        "    {\"$out\": \"x\"}",
        "  ]",
        "}",
    ];
    let (c, d) = (collections(), databases());
    let r = save_checks(
        &lines,
        &hune_core::mongo::SaveContext { variables: &[], collections: &c, databases: &d, usage: SqlUse::Panel },
    );
    assert_eq!(r.faults.len(), 1);
    let f = &r.faults[0];
    assert_eq!(f.line_index, 4);
    assert_eq!(f.column, 6);
    assert!(f.message.contains("/aggregate/1/$out"));
    let item = f.to_validation_error(PANEL);
    assert_eq!(item.pointer, format!("{PANEL}/4"));
    assert_eq!(item.code, ErrorCode::NotPermitted);
    assert!(item.message.starts_with("column 6: /aggregate/1/$out: $out is not permitted in a MongoDB query of Hune"));
    assert_eq!(item.line, None);
    assert_eq!(item.column, None);
    // A fault of the profile gives the line of the fault.
    let bad = ["{", "  \"collection\": \"hosts\",", "  \"find\": {,}", "}"];
    let r = save_checks(
        &bad,
        &hune_core::mongo::SaveContext { variables: &[], collections: &c, databases: &d, usage: SqlUse::Panel },
    );
    assert_eq!(r.faults.len(), 1);
    assert_eq!(r.faults[0].line_index, 2);
    // The faults come in the order of their position.
    let many =
        ["{\"collection\":\"hosts\",\"aggregate\":[", "{\"$out\":1},", "{\"$merge\":1},", "{\"$limit\":0}", "]}"];
    let r = save_checks(
        &many,
        &hune_core::mongo::SaveContext { variables: &[], collections: &c, databases: &d, usage: SqlUse::Panel },
    );
    let lines: Vec<usize> = r.faults.iter().map(|f| f.line_index).collect();
    assert_eq!(lines, [1, 2, 3]);
}

#[test]
fn the_warnings_1_to_3() {
    // 1: a `${name}` whose name is not a variable. A `$name` gives no warning.
    let t = r#"{"collection":"hosts","aggregate":[{"$match":{"a":"${cpu}","b":"${cpu:json}","c":"$cpu","d":"x ${gpu} y"}}]}"#;
    assert_eq!(warning_kinds(t), [WarnKind::UnknownVariable; 3]);
    assert!(save(t).faults.is_empty());
    // 2: a `[[name]]` whose name is a variable of the document.
    let t = r#"{"collection":"hosts","aggregate":[{"$match":{"a":"[[host]]","b":"[[cpu]]"}}]}"#;
    assert_eq!(warning_kinds(t), [WarnKind::DoubleBracket]);
    // 3: a macro name in a longer string.
    let t = r#"{"collection":"hosts","aggregate":[{"$match":{"a":"from $__timeFrom to ${__timeTo}"}}]}"#;
    assert_eq!(warning_kinds(t), [WarnKind::MacroInText; 2]);
    let r = save(t);
    let (pointer, message) = r.warnings[0].to_pointer_message(PANEL);
    assert_eq!(pointer, format!("{PANEL}/0"));
    assert!(message.starts_with("column "));
    // A warning never gives a fault.
    assert!(r.faults.is_empty());
    // No warning for a placeholder that is declared.
    assert!(warning_kinds(r#"{"collection":"hosts","aggregate":[{"$match":{"a":"x${host}"}}]}"#).is_empty());
}

#[test]
fn what_the_save_accepts_the_run_accepts_and_what_the_save_refuses_the_run_refuses() {
    let texts = [
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"x"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$out":"x"}]}"#,
        r#"{"collection":"secret","find":{}}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"${host}":1}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$limit":0}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$match":{"a":"$__bad"}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$addFields":{"x":{"$add":[1,2]}}}]}"#,
        r#"{"collection":"hosts","aggregate":[{"$addFields":{"x":"$$NOW"}}]}"#,
        "{",
    ];
    for t in texts {
        let saved_ok = save(t).faults.is_empty();
        let run_ok = run(t, &request(&[("host", false, &["x"])])).is_ok();
        assert_eq!(saved_ok, run_ok, "{t}");
    }
}
