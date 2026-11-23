//! The grammar of 7.29 to 7.33: the top object, G1 to G10, the allowlists, the refused names.

mod mongo_common;

use hune_core::error::ErrorCode;
use hune_core::mongo::{Item, Phase, check_top, parse_text, refusal_message};
use mongo_common::*;

fn agg(stage: &str) -> String {
    format!(r#"{{"collection":"node_load","aggregate":[{stage}]}}"#)
}

fn ok(text: &str) {
    let r = save(text);
    assert!(r.faults.is_empty(), "{text}: {:?}", r.faults);
    if let Err(e) = run_plain(text) {
        panic!("{text}: {e}");
    }
}

/// The text is refused at save with `code`, and at run with `sql_invalid`.
fn refused(text: &str, code: ErrorCode) {
    let r = save(text);
    assert!(!r.faults.is_empty(), "{text}: accepted at save");
    assert!(r.faults.iter().any(|f| f.code == code), "{text}: {:?}", r.faults);
    match run_plain(text) {
        Err(e) => assert_eq!(e.api_code(), "sql_invalid", "{text}: {e}"),
        Ok(_) => panic!("{text}: accepted at run"),
    }
}

#[test]
fn the_top_object_of_7_29() {
    ok(r#"{"collection":"node_load","aggregate":[{"$limit":1}]}"#);
    ok(r#"{"collection":"node_load","database":"metrics","aggregate":[{"$limit":1}]}"#);
    ok(r#"{"collection":"node_load","find":{}}"#);
    ok(
        r#"{"collection":"node_load","find":{"filter":{"a":1},"projection":{"a":1,"_id":0},"sort":{"a":-1},"skip":5,"limit":10}}"#,
    );
    for t in [
        r#"{"aggregate":[{"$limit":1}]}"#,
        r#"{"collection":"node_load"}"#,
        r#"{"collection":"node_load","aggregate":[],"find":{}}"#,
        r#"{"collection":"node_load","aggregate":[{"$limit":1}],"find":{}}"#,
        r#"{"collection":"node_load","aggregate":[]}"#,
        r#"{"collection":"node_load","aggregate":{"$limit":1}}"#,
        r#"{"collection":"node_load","find":[]}"#,
        r#"{"collection":5,"find":{}}"#,
        r#"{"collection":"node_load","database":5,"find":{}}"#,
        r#"[]"#,
        r#""x""#,
        r#"{"collection":"node_load","find":{"skip":-1}}"#,
        r#"{"collection":"node_load","find":{"skip":1000000001}}"#,
        r#"{"collection":"node_load","find":{"limit":0}}"#,
        r#"{"collection":"node_load","find":{"limit":1.5}}"#,
        r#"{"collection":"node_load","find":{"limit":"${n:number}"}}"#,
        r#"{"collection":"node_load","find":{"sort":{}}}"#,
        r#"{"collection":"node_load","find":{"sort":{"a":2}}}"#,
        r#"{"collection":"node_load","find":{"projection":{"a":1,"b":0}}}"#,
        r#"{"collection":"node_load","find":{"projection":{"a":2}}}"#,
        r#"{"collection":"node_load","find":{"filter":[]}}"#,
    ] {
        let r = save(t);
        assert!(!r.faults.is_empty(), "{t}");
        assert_eq!(run_plain(t).unwrap_err().api_code(), "sql_invalid", "{t}");
    }
    // The find limits are in range at the bound.
    ok(r#"{"collection":"node_load","find":{"skip":0,"limit":1000000000}}"#);
    ok(r#"{"collection":"node_load","find":{"skip":1000000000,"limit":1}}"#);
    // 32 sort keys pass and 33 do not.
    let sort = |n: usize| {
        let keys: Vec<String> = (0..n).map(|i| format!("\"k{i}\":1")).collect();
        format!(r#"{{"collection":"node_load","find":{{"sort":{{{}}}}}}}"#, keys.join(","))
    };
    ok(&sort(32));
    assert!(!save(&sort(33)).faults.is_empty());
}

#[test]
fn unknown_members_are_refused_with_the_message_of_7_32() {
    for name in [
        "allowDiskUse",
        "hint",
        "collation",
        "let",
        "readConcern",
        "maxTimeMS",
        "writeConcern",
        "explain",
        "mapReduce",
        "comment",
        "x",
    ] {
        let t = format!(r#"{{"collection":"node_load","find":{{}},"{name}":1}}"#);
        let r = save(&t);
        assert_eq!(r.faults.len(), 1, "{name}");
        assert_eq!(r.faults[0].code, ErrorCode::NotPermitted);
        assert!(
            r.faults[0].message.contains(&format!("{name} is not permitted in a MongoDB query of Hune")),
            "{}",
            r.faults[0].message
        );
        assert_eq!(run_plain(&t).unwrap_err().api_code(), "sql_invalid");
    }
    // A member of find that 7.29 does not list.
    let r = save(r#"{"collection":"node_load","find":{"hint":{"a":1}}}"#);
    assert_eq!(r.faults[0].code, ErrorCode::NotPermitted);
}

#[test]
fn collections_and_databases_are_an_exact_allowlist() {
    refused(r#"{"collection":"secret","find":{}}"#, ErrorCode::BadReference);
    refused(r#"{"collection":"Node_Load","find":{}}"#, ErrorCode::BadReference);
    refused(r#"{"collection":"node_load ","find":{}}"#, ErrorCode::BadReference);
    refused(r#"{"collection":"*","find":{}}"#, ErrorCode::BadReference);
    refused(r#"{"collection":"node_load","database":"admin","find":{}}"#, ErrorCode::BadReference);
    refused(r#"{"collection":"node_load","database":"Metrics","find":{}}"#, ErrorCode::BadReference);
    ok(r#"{"collection":"events","database":"metrics","find":{}}"#);
    // $lookup and $unionWith: the list of the collections, in the database of the query.
    ok(&agg(r#"{"$lookup":{"from":"hosts","localField":"h","foreignField":"_id","as":"x"}}"#));
    ok(&agg(
        r#"{"$lookup":{"from":"hosts","let":{"h":"$h"},"pipeline":[{"$match":{"$expr":{"$eq":["$_id","$$h"]}}}],"as":"x"}}"#,
    ));
    ok(&agg(r#"{"$unionWith":"events"}"#));
    ok(&agg(r#"{"$unionWith":{"coll":"events","pipeline":[{"$limit":1}]}}"#));
    refused(
        &agg(r#"{"$lookup":{"from":"secret","localField":"h","foreignField":"_id","as":"x"}}"#),
        ErrorCode::BadReference,
    );
    refused(
        &agg(r#"{"$lookup":{"from":{"db":"admin","coll":"hosts"},"localField":"h","foreignField":"_id","as":"x"}}"#),
        ErrorCode::NotPermitted,
    );
    refused(&agg(r#"{"$lookup":{"localField":"h","foreignField":"_id","as":"x"}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$lookup":{"from":"hosts","pipeline":[],"as":"x"}}"#), ErrorCode::BadFormat);
    refused(
        &agg(r#"{"$lookup":{"from":"hosts","localField":"h","foreignField":"_id","as":"x","let":{}}}"#),
        ErrorCode::BadFormat,
    );
    refused(&agg(r#"{"$lookup":{"from":"hosts","localField":"h","as":"x"}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$unionWith":"secret"}"#), ErrorCode::BadReference);
    refused(&agg(r#"{"$unionWith":{"coll":"secret"}}"#), ErrorCode::BadReference);
    refused(&agg(r#"{"$unionWith":{"pipeline":[{"$limit":1}]}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$unionWith":{"coll":{"db":"a","coll":"b"}}}"#), ErrorCode::NotPermitted);
    refused(
        &agg(
            r#"{"$unionWith":{"coll":"events","pipeline":[{"$lookup":{"from":"secret","localField":"a","foreignField":"b","as":"c"}}]}}"#,
        ),
        ErrorCode::BadReference,
    );
    refused(
        &agg(
            r#"{"$graphLookup":{"from":"hosts","startWith":"$a","connectFromField":"a","connectToField":"b","as":"c"}}"#,
        ),
        ErrorCode::NotPermitted,
    );
}

#[test]
fn g1_every_dollar_key_is_in_a_table() {
    for name in [
        "$out",
        "$merge",
        "$function",
        "$accumulator",
        "$where",
        "$code",
        "$codeWScope",
        "$currentOp",
        "$listSessions",
        "$listLocalSessions",
        "$listSampledQueries",
        "$queryStats",
        "$planCacheStats",
        "$collStats",
        "$indexStats",
        "$listCatalog",
        "$listSearchIndexes",
        "$changeStream",
        "$changeStreamSplitLargeEvent",
        "$graphLookup",
        "$search",
        "$searchMeta",
        "$vectorSearch",
        "$documents",
        "$geoNear",
        "$redact",
        "$text",
        "$jsonSchema",
        "$sample",
        "$sampleRate",
        "$rand",
        "$meta",
        "$comment",
        "$natural",
        "$_internal",
        "$_x",
        "$_",
        "$unknownThing",
        "$Match",
    ] {
        // As a stage.
        refused(&agg(&format!(r#"{{"{name}":{{}}}}"#)), ErrorCode::NotPermitted);
        // As an operator of a filter, of an expression and of an accumulator.
        refused(&agg(&format!(r#"{{"$match":{{"a":{{"{name}":1}}}}}}"#)), ErrorCode::NotPermitted);
        refused(&agg(&format!(r#"{{"$match":{{"{name}":1}}}}"#)), ErrorCode::NotPermitted);
        refused(&agg(&format!(r#"{{"$addFields":{{"x":{{"{name}":1}}}}}}"#)), ErrorCode::NotPermitted);
        refused(&agg(&format!(r#"{{"$group":{{"_id":null,"x":{{"{name}":"$a"}}}}}}"#)), ErrorCode::NotPermitted);
        // At depth: in an array, in a nested expression, in $expr, in $facet, in $lookup, in $unionWith.
        refused(
            &agg(&format!(r#"{{"$addFields":{{"x":{{"$add":[1,{{"$multiply":[2,{{"{name}":3}}]}}]}}}}}}"#)),
            ErrorCode::NotPermitted,
        );
        refused(
            &agg(&format!(r#"{{"$match":{{"$expr":{{"$and":[true,{{"{name}":1}}]}}}}}}"#)),
            ErrorCode::NotPermitted,
        );
        refused(&agg(&format!(r#"{{"$facet":{{"f":[{{"{name}":{{}}}}]}}}}"#)), ErrorCode::NotPermitted);
        refused(
            &agg(&format!(r#"{{"$lookup":{{"from":"hosts","pipeline":[{{"{name}":{{}}}}],"as":"x"}}}}"#)),
            ErrorCode::NotPermitted,
        );
        refused(
            &agg(&format!(r#"{{"$unionWith":{{"coll":"hosts","pipeline":[{{"{name}":{{}}}}]}}}}"#)),
            ErrorCode::NotPermitted,
        );
        // In a literal object that no operator holds.
        refused(&agg(&format!(r#"{{"$match":{{"a":{{"b":{{"{name}":1}}}}}}}}"#)), ErrorCode::NotPermitted);
        // Inside $literal, where a key never starts with $.
        refused(&agg(&format!(r#"{{"$addFields":{{"x":{{"$literal":{{"{name}":1}}}}}}}}"#)), ErrorCode::NotPermitted);
        // As a find filter.
        refused(
            &format!(r#"{{"collection":"node_load","find":{{"filter":{{"{name}":1}}}}}}"#),
            ErrorCode::NotPermitted,
        );
    }
}

#[test]
fn the_message_of_a_refused_name_is_exact() {
    let r = save(&agg(r#"{"$out":"x"}"#));
    assert_eq!(r.faults.len(), 1);
    assert!(r.faults[0].message.contains("/aggregate/0/$out: $out is not permitted in a MongoDB query of Hune"));
    assert_eq!(refusal_message("$unknownThing"), "$unknownThing is not permitted in a MongoDB query of Hune");
    assert!(refusal_message("$out").starts_with("$out is not permitted in a MongoDB query of Hune: "));
    // A refused name in a string value gives no refusal.
    ok(&agg(r#"{"$match":{"a":"$out","b":"out","c":{"$eq":"$function"}}}"#.replace("$out\"", "x$out\"").as_str()));
    ok(&agg(
        r#"{"$addFields":{"x":{"$concat":["$out text","$where"]}}}"#.replace("\"$where\"", "\"a$where\"").as_str(),
    ));
}

#[test]
fn variables_g7() {
    ok(&agg(
        r#"{"$addFields":{"x":"$$ROOT","y":"$$CURRENT.a.b","z":"$$REMOVE","w":"$$this","v":"$$value.a","u":"$$x1"}}"#,
    ));
    for name in
        ["NOW", "CLUSTER_TIME", "USER_ROLES", "SEARCH_META", "DESCEND", "PRUNE", "KEEP", "Root", "X", "9", "", "_a"]
    {
        let t = agg(&format!(r#"{{"$addFields":{{"x":"$${name}"}}}}"#));
        refused(&t, ErrorCode::NotPermitted);
    }
    refused(&agg(r#"{"$addFields":{"x":"$$ROOT..a"}}"#), ErrorCode::BadFormat);
    // The names of 7.32 are refused in a filter literal too.
    refused(&agg(r#"{"$match":{"a":"$$NOW"}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$match":{"$expr":{"$gt":["$ts","$$NOW"]}}}"#), ErrorCode::NotPermitted);
}

#[test]
fn field_names_and_paths_g2_g8() {
    ok(&agg(r#"{"$match":{"a":1,"a.b":2,"a.0.c":3,"_id":4,"é":5,"a b":6}}"#));
    ok(&agg(r#"{"$addFields":{"x":"$a.b.c","y":"$_id","z":"$a b"}}"#));
    for key in ["", "a$", "a$b", "a..b", ".a", "a.", "a\\u0001", "a\\u007f", "a\\u0085"] {
        let t = agg(&format!(r#"{{"$match":{{"{key}":1}}}}"#));
        assert!(!save(&t).faults.is_empty(), "{key:?}");
        assert_eq!(run_plain(&t).unwrap_err().api_code(), "sql_invalid", "{key:?}");
    }
    let long = "k".repeat(257);
    assert!(!save(&agg(&format!(r#"{{"$match":{{"{long}":1}}}}"#))).faults.is_empty());
    // Field paths.
    for path in ["$", "$.a", "$a.", "$a..b", "$a.$b", "$a\\u0001"] {
        let t = agg(&format!(r#"{{"$addFields":{{"x":"{path}"}}}}"#));
        assert!(!save(&t).faults.is_empty(), "{path:?}");
    }
    // A key of an output object follows G2.
    refused(&agg(r#"{"$addFields":{"a$b":1}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$group":{"_id":null,"a.":{"$sum":1}}}"#), ErrorCode::BadFormat);
}

#[test]
fn g3_expression_objects() {
    ok(&agg(r#"{"$addFields":{"x":{"$add":[1,2]},"y":{"a":1,"b":{"c":"$d"}},"z":[1,{"$abs":-1}]}}"#));
    refused(&agg(r#"{"$addFields":{"x":{"$add":[1,2],"y":3}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$addFields":{"x":{"y":3,"$add":[1,2]}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$addFields":{"x":{"$add":[1,2],"$abs":3}}}"#), ErrorCode::BadFormat);
}

#[test]
fn g4_g5_filters() {
    ok(&agg(
        r#"{"$match":{"a":{"$gt":1,"$lt":5},"b":{"$in":[1,2]},"c":{"$not":{"$gt":1}},"$or":[{"a":1},{"b":2}],"$and":[{"a":1}],"$nor":[{"a":1}],"$expr":{"$gt":["$a",1]}}}"#,
    ));
    ok(&agg(
        r#"{"$match":{"a":{"$exists":true},"b":{"$type":"string"},"c":{"$size":2},"d":{"$all":[1,2]},"e":{"$elemMatch":{"x":1,"y":{"$gt":2}}},"f":{"$mod":[2,0]},"g":{"$regex":"^a","$options":"i"}}}"#,
    ));
    ok(&agg(r#"{"$match":{"a":{"b":1},"c":[1,{"d":2}],"e":null}}"#));
    ok(&agg(r#"{"$match":{"a":{"$elemMatch":{"$gt":1,"$lt":3}}}}"#));
    refused(&agg(r#"{"$match":{"a":{"$gt":1,"b":2}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$match":{"a":{"b":1,"$gt":2}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$match":{"a":{"$sum":1}}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$match":{"a":{"$add":[1,2]}}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$match":{"$gt":1}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$match":{"$and":[]}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$match":{"$and":{}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$match":{"$or":[1]}}"#), ErrorCode::BadFormat);
    let many = vec![r#"{"a":1}"#; 33].join(",");
    refused(&agg(&format!(r#"{{"$match":{{"$or":[{many}]}}}}"#)), ErrorCode::BadFormat);
    let many = vec![r#"{"a":1}"#; 32].join(",");
    ok(&agg(&format!(r#"{{"$match":{{"$or":[{many}]}}}}"#)));
    refused(&agg(r#"{"$match":{"a":{"$in":{"x":1}}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$match":{"a":{"$in":[{"$gt":1}]}}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$match":{"a":{"$eq":{"$gt":1}}}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$match":{"$expr":{"$function":{}}}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$match":5}"#), ErrorCode::BadFormat);
}

#[test]
fn regex_options_and_size() {
    ok(&agg(r#"{"$match":{"a":{"$regex":"x","$options":"imsx"}}}"#));
    ok(&agg(r#"{"$addFields":{"m":{"$regexMatch":{"input":"$a","regex":"x","options":"i"}}}}"#));
    for opt in ["u", "l", "g", "ii ", "n", "imsxu", " "] {
        refused(&agg(&format!(r#"{{"$match":{{"a":{{"$regex":"x","$options":"{opt}"}}}}}}"#)), ErrorCode::BadFormat);
        refused(
            &agg(&format!(
                r#"{{"$addFields":{{"m":{{"$regexFind":{{"input":"$a","regex":"x","options":"{opt}"}}}}}}}}"#
            )),
            ErrorCode::BadFormat,
        );
    }
    let p = |n: usize| format!(r#"{{"$match":{{"a":{{"$regex":"{}"}}}}}}"#, "a".repeat(n));
    ok(&agg(&p(1024)));
    refused(&agg(&p(1025)), ErrorCode::TooLarge);
    let q = |n: usize| {
        format!(r#"{{"$addFields":{{"m":{{"$regexMatch":{{"input":"$a","regex":"{}"}}}}}}}}"#, "a".repeat(n))
    };
    ok(&agg(&q(1024)));
    refused(&agg(&q(1025)), ErrorCode::TooLarge);
    refused(&agg(r#"{"$match":{"a":{"$regex":5}}}"#), ErrorCode::BadFormat);
}

#[test]
fn g6_stage_shapes() {
    refused(&agg(r#"{}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$limit":1,"$skip":2}"#), ErrorCode::BadFormat);
    refused(&agg(r#"5"#), ErrorCode::BadFormat);
    refused(&agg(r#"[]"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$limit":0}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$limit":1000000001}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$limit":1.5}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$limit":"x"}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$skip":-1}"#), ErrorCode::BadFormat);
    ok(&agg(r#"{"$limit":1000000000}"#));
    ok(&agg(r#"{"$skip":0}"#));
    ok(&agg(r#"{"$limit":3000000}"#));
    refused(&agg(r#"{"$count":"a.b"}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$count":"$a"}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$count":""}"#), ErrorCode::BadFormat);
    ok(&agg(r#"{"$count":"n"}"#));
    refused(&agg(r#"{"$group":{"x":{"$sum":1}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$group":{"_id":null,"x":{"$sum":1,"$avg":1}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$group":{"_id":null,"x":{"$abs":1}}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$group":{"_id":null,"x":1}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$unset":[]}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$unset":["a",5]}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$unset":"$a"}"#), ErrorCode::BadFormat);
    ok(&agg(r#"{"$unset":["a","b.c"]}"#));
    ok(&agg(r#"{"$unset":"a"}"#));
    let many: Vec<String> = (0..65).map(|i| format!("\"f{i}\"")).collect();
    refused(&agg(&format!(r#"{{"$unset":[{}]}}"#, many.join(","))), ErrorCode::BadFormat);
    refused(&agg(r#"{"$sort":{}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$sort":{"a":0}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$sort":{"a":{"$meta":"textScore"}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$unwind":"a"}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$unwind":{}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$unwind":{"path":"$a","foo":1}}"#), ErrorCode::BadFormat);
    ok(&agg(r#"{"$unwind":{"path":"$a","includeArrayIndex":"i","preserveNullAndEmptyArrays":true}}"#));
    ok(&agg(r#"{"$unwind":"$a.b"}"#));
    refused(&agg(r#"{"$replaceRoot":{}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$replaceRoot":{"newRoot":"$a","x":1}}"#), ErrorCode::BadFormat);
    ok(&agg(r#"{"$replaceRoot":{"newRoot":"$a"}}"#));
    ok(&agg(r#"{"$replaceRoot":{"newRoot":{"x":"$a"}}}"#));
    ok(&agg(r#"{"$replaceWith":"$a"}"#));
    ok(&agg(r#"{"$sortByCount":"$a"}"#));
}

#[test]
fn bucket_facet_window_densify_fill() {
    ok(&agg(r#"{"$bucket":{"groupBy":"$a","boundaries":[0,10,20],"default":"other","output":{"n":{"$sum":1}}}}"#));
    refused(&agg(r#"{"$bucket":{"groupBy":"$a","boundaries":[0]}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$bucket":{"boundaries":[0,1]}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$bucket":{"groupBy":"$a","boundaries":[0,1],"x":1}}"#), ErrorCode::BadFormat);
    let many = vec!["0"; 1001].join(",");
    refused(&agg(&format!(r#"{{"$bucket":{{"groupBy":"$a","boundaries":[{many}]}}}}"#)), ErrorCode::BadFormat);
    ok(&agg(r#"{"$bucketAuto":{"groupBy":"$a","buckets":5,"output":{"n":{"$sum":1}}}}"#));
    refused(&agg(r#"{"$bucketAuto":{"groupBy":"$a","buckets":0}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$bucketAuto":{"groupBy":"$a","buckets":1001}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$bucketAuto":{"groupBy":"$a","buckets":5,"granularity":"R5"}}"#), ErrorCode::BadFormat);
    ok(&agg(r#"{"$facet":{"a":[{"$match":{"x":1}}],"b":[{"$count":"n"}]}}"#));
    refused(&agg(r#"{"$facet":{}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$facet":{"a":{"$match":{}}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$facet":{"a":[]}}"#), ErrorCode::BadFormat);
    let members: Vec<String> = (0..17).map(|i| format!(r#""f{i}":[{{"$count":"n"}}]"#)).collect();
    refused(&agg(&format!(r#"{{"$facet":{{{}}}}}"#, members.join(","))), ErrorCode::BadFormat);
    // $facet is not permitted in a nested pipeline.
    refused(&agg(r#"{"$facet":{"a":[{"$facet":{"b":[{"$count":"n"}]}}]}}"#), ErrorCode::NotPermitted);
    refused(
        &agg(r#"{"$lookup":{"from":"hosts","pipeline":[{"$facet":{"b":[{"$count":"n"}]}}],"as":"x"}}"#),
        ErrorCode::NotPermitted,
    );
    refused(
        &agg(r#"{"$unionWith":{"coll":"hosts","pipeline":[{"$facet":{"b":[{"$count":"n"}]}}]}}"#),
        ErrorCode::NotPermitted,
    );
    ok(&agg(
        r#"{"$setWindowFields":{"partitionBy":"$h","sortBy":{"ts":1},"output":{"avg":{"$avg":"$v","window":{"documents":["unbounded","current"]}},"r":{"$rank":{}},"d":{"$derivative":{"input":"$v","unit":"hour"},"window":{"range":[-1,0],"unit":"hour"}}}}}"#,
    ));
    refused(&agg(r#"{"$setWindowFields":{"sortBy":{"ts":1}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$setWindowFields":{"output":{"a":{"$sum":"$v","$avg":"$v"}}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$setWindowFields":{"output":{"a":{"window":{}}}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$setWindowFields":{"output":{"a":{"$abs":1}}}}"#), ErrorCode::NotPermitted);
    refused(&agg(r#"{"$setWindowFields":{"output":{"a":{"$sum":1}},"x":1}}"#), ErrorCode::BadFormat);
    ok(&agg(
        r#"{"$densify":{"field":"ts","partitionByFields":["h"],"range":{"step":1,"unit":"hour","bounds":"full"}}}"#,
    ));
    refused(&agg(r#"{"$densify":{"field":"ts"}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$densify":{"range":{}}}"#), ErrorCode::BadFormat);
    ok(&agg(
        r#"{"$fill":{"partitionByFields":["h"],"sortBy":{"ts":1},"output":{"v":{"method":"linear"},"w":{"value":0}}}}"#,
    ));
    refused(&agg(r#"{"$fill":{"sortBy":{"ts":1}}}"#), ErrorCode::BadFormat);
}

#[test]
fn g9_nested_pipelines_and_limits() {
    let nest = |depth: usize| {
        let mut inner = r#"{"$limit":1}"#.to_owned();
        for _ in 0..depth {
            inner = format!(r#"{{"$unionWith":{{"coll":"hosts","pipeline":[{inner}]}}}}"#);
        }
        agg(&inner)
    };
    ok(&nest(1));
    ok(&nest(3));
    refused(&nest(4), ErrorCode::TooDeep);
    // $lookup is not permitted at the third level of nested pipelines.
    let level3 = agg(
        r#"{"$unionWith":{"coll":"hosts","pipeline":[{"$unionWith":{"coll":"hosts","pipeline":[{"$unionWith":{"coll":"hosts","pipeline":[{"$lookup":{"from":"hosts","localField":"a","foreignField":"b","as":"c"}}]}}]}}]}}"#,
    );
    refused(&level3, ErrorCode::NotPermitted);
    let level2 = agg(
        r#"{"$unionWith":{"coll":"hosts","pipeline":[{"$unionWith":{"coll":"hosts","pipeline":[{"$lookup":{"from":"hosts","localField":"a","foreignField":"b","as":"c"}}]}}]}}"#,
    );
    ok(&level2);
    // Stages at all depths: 64 pass, 65 do not.
    let stages =
        |n: usize| format!(r#"{{"collection":"node_load","aggregate":[{}]}}"#, vec![r#"{"$limit":1}"#; n].join(","));
    ok(&stages(64));
    refused(&stages(65), ErrorCode::TooLarge);
    let nested = |inner: usize| {
        let p = vec![r#"{"$limit":1}"#; inner].join(",");
        format!(r#"{{"collection":"node_load","aggregate":[{{"$unionWith":{{"coll":"hosts","pipeline":[{p}]}}}}]}}"#)
    };
    ok(&nested(63));
    refused(&nested(64), ErrorCode::TooLarge);
}

#[test]
fn name_places_hold_no_placeholder() {
    // `host` is a variable of the document: a string `$host` is a placeholder.
    for t in [
        r#"{"collection":"${host}","find":{}}"#,
        r#"{"collection":"node_load","database":"$host","find":{}}"#,
        r#"{"collection":"node_load","aggregate":[{"$lookup":{"from":"${host}","localField":"a","foreignField":"b","as":"c"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$lookup":{"from":"hosts","localField":"$host","foreignField":"b","as":"c"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$lookup":{"from":"hosts","localField":"a","foreignField":"${host}","as":"c"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$lookup":{"from":"hosts","localField":"a","foreignField":"b","as":"${host}"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$unionWith":"${host}"}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$unionWith":{"coll":"$host"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$unwind":"$host"}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$unwind":{"path":"$host"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$unwind":{"path":"$a","includeArrayIndex":"${host}"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$count":"${host}"}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$unset":"${host}"}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$unset":["a","${host}"]}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$replaceRoot":{"newRoot":"$host"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":{"$getField":"${host}"}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":{"$getField":{"field":"${host}","input":"$$ROOT"}}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":{"$setField":{"field":"${host}","input":"$$ROOT","value":1}}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":{"$unsetField":{"field":"${host}","input":"$$ROOT"}}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":{"$map":{"input":"$a","as":"${host}","in":1}}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$densify":{"field":"${host}","range":{"step":1,"bounds":"full"}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$densify":{"field":"t","partitionByFields":["${host}"],"range":{"step":1,"bounds":"full"}}}]}"#,
        // A macro in a name place.
        r#"{"collection":"node_load","aggregate":[{"$unwind":"$__timeFrom"}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$count":"$__from"}]}"#,
    ] {
        let r = save(t);
        assert!(
            r.faults.iter().any(|f| f.code == ErrorCode::NotPermitted || f.code == ErrorCode::BadReference),
            "{t}: {:?}",
            r.faults
        );
        let mut rq = request(&[("host", false, &["x"])]);
        rq.insert("hosts", false, &["a"]);
        assert_eq!(run(t, &rq).unwrap_err().api_code(), "sql_invalid", "{t}");
    }
    // The same places hold plain text.
    ok(
        r#"{"collection":"node_load","aggregate":[{"$unwind":{"path":"$a.b","includeArrayIndex":"i"}},{"$count":"n"}]}"#,
    );
    // A variable that the document does not declare is text: `$cpu` is a field path.
    ok(r#"{"collection":"node_load","aggregate":[{"$unwind":"$cpu"}]}"#);
    // A placeholder in a key.
    for t in [
        r#"{"collection":"node_load","aggregate":[{"$match":{"${host}":1}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$match":{"$host":1}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$match":{"a_${host}":1}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"${host}":1}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$project":{"x":{"${host}":1}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"${host}":{}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$group":{"_id":null,"${host}":{"$sum":1}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$group":{"_id":null,"x":{"${host}":1}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$sort":{"${host}":1}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$match":{"a":{"${host}":1}}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$match":{"a":{"b":{"${host}":1}}}}]}"#,
        r#"{"collection":"node_load","find":{"sort":{"${host}":1}}}"#,
        r#"{"collection":"node_load","find":{"projection":{"${host}":1}}}"#,
    ] {
        let r = save(t);
        assert!(r.faults.iter().any(|f| f.code == ErrorCode::NotPermitted), "{t}: {:?}", r.faults);
        let rq = request(&[("host", false, &["x"])]);
        assert_eq!(run(t, &rq).unwrap_err().api_code(), "sql_invalid", "{t}");
    }
}

#[test]
fn stored_string_that_starts_with_dollar_holds_no_placeholder() {
    for t in [
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":"$a_${host}"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":"${host}_a"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":"$host.a"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":"$$ROOT.${host}"}}]}"#,
        r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":"${host}${n}"}}]}"#,
    ] {
        let r = save(t);
        assert!(r.faults.iter().any(|f| f.code == ErrorCode::NotPermitted), "{t}: {:?}", r.faults);
        assert_eq!(
            run(t, &request(&[("host", false, &["x"]), ("n", false, &["y"])])).unwrap_err().api_code(),
            "sql_invalid",
            "{t}"
        );
    }
    // To name the field of a field called like a variable, write the reference.
    ok(r#"{"collection":"node_load","aggregate":[{"$addFields":{"x":"$$CURRENT.host"}}]}"#);
}

#[test]
fn macros_g8() {
    ok(&agg(
        r#"{"$match":{"ts":{"$gte":"$__timeFrom","$lte":"${__timeTo}"},"f":"$__from","t":"$__to","i":"$__interval_ms","s":"$__interval_s"}}"#,
    ));
    for s in [
        "$__timeFromX",
        "$__foo",
        "$__",
        "$__timeFrom ",
        "$__timeFrom.x",
        "${__timeFrom:json}",
        "${__foo}",
        "$__interval",
        "$__timeInterval",
        "$__range",
        "$__TIMEFROM",
    ] {
        let t = agg(&format!(r#"{{"$match":{{"a":"{s}"}}}}"#));
        refused(&t, ErrorCode::BadFormat);
    }
    // A macro name in a longer string stays text and gives a warning.
    let t = agg(r#"{"$match":{"a":"x $__timeFrom y"}}"#);
    ok(&t);
    assert_eq!(warning_kinds(&t), [hune_core::mongo::WarnKind::MacroInText]);
}

#[test]
fn e_operators_with_a_name_place() {
    ok(&agg(
        r#"{"$addFields":{"x":{"$getField":"a"},"y":{"$getField":{"field":"a","input":"$b"}},"z":{"$setField":{"field":"a","input":"$$ROOT","value":1}}}}"#,
    ));
    refused(&agg(r#"{"$addFields":{"x":{"$getField":{"field":5}}}}"#), ErrorCode::BadFormat);
    refused(&agg(r#"{"$addFields":{"x":{"$getField":{"$literal":"a"}}}}"#), ErrorCode::BadFormat);
    ok(&agg(r#"{"$addFields":{"x":{"$filter":{"input":"$a","as":"v","cond":{"$gt":["$$v",1]}}}}}"#));
    ok(&agg(r#"{"$addFields":{"x":{"$let":{"vars":{"a":1},"in":"$$a"}}}}"#));
}

#[test]
fn accumulators_and_the_tables() {
    ok(&agg(
        r#"{"$group":{"_id":"$h","a":{"$sum":"$v"},"b":{"$avg":"$v"},"c":{"$min":"$v"},"d":{"$max":"$v"},"e":{"$count":{}},"f":{"$first":"$v"},"g":{"$last":"$v"},"h":{"$push":"$v"},"i":{"$addToSet":"$v"},"j":{"$stdDevPop":"$v"},"k":{"$stdDevSamp":"$v"},"l":{"$top":{"sortBy":{"t":1},"output":"$v"}},"m":{"$bottom":{"sortBy":{"t":1},"output":"$v"}},"n":{"$topN":{"n":2,"sortBy":{"t":1},"output":"$v"}},"o":{"$bottomN":{"n":2,"sortBy":{"t":1},"output":"$v"}},"p":{"$firstN":{"input":"$v","n":2}},"q":{"$lastN":{"input":"$v","n":2}},"r":{"$maxN":{"input":"$v","n":2}},"s":{"$minN":{"input":"$v","n":2}},"t":{"$median":{"input":"$v","method":"approximate"}},"u":{"$percentile":{"input":"$v","p":[0.5],"method":"approximate"}},"v":{"$mergeObjects":"$o"}}}"#,
    ));
    // The accumulators `$sum`, `$avg`, `$min`, `$max` are not expression operators in 7.31.
    for op in
        ["$sum", "$avg", "$min", "$max", "$push", "$addToSet", "$stdDevPop", "$count", "$median", "$percentile", "$top"]
    {
        refused(&agg(&format!(r#"{{"$addFields":{{"x":{{"{op}":"$a"}}}}}}"#)), ErrorCode::NotPermitted);
    }
}

#[test]
fn the_guard_check_runs_on_a_tree_with_no_placeholder() {
    let t = parse_text(&agg(r#"{"$match":{"a":"$$NOW"}}"#)).unwrap().value;
    let (c, d) = (collections(), databases());
    let check = hune_core::mongo::Check {
        phase: Phase::Expanded,
        vars: &hune_core::mongo::NoVariables,
        collections: &c,
        databases: &d,
        usage: hune_core::sql::SqlUse::Panel,
    };
    let r = check_top(&t, &check);
    assert_eq!(r.faults.len(), 1);
    assert_eq!(r.faults[0].item, Item::L3);
}
