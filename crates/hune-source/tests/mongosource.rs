// SPDX-License-Identifier: MIT OR Apache-2.0
//! Container tests of the MongoDB source (16.9b, the job `mongosource`).
//!
//! Each test is `#[ignore]` and starts with its case id. The script `tests/ci/mongosource.sh` runs them on
//! MongoDB 7.0 and 8.0 with `--include-ignored`. The tests use the real driver (`MongoDriver`) and the
//! disposable servers of `tests/compose/mongosource.yaml`. Tests are not product code: they can read the
//! environment and the file system.
#![cfg(feature = "mongo-driver")]

mod mgs;

use std::time::{Duration, Instant};

use hune_core::mongo::{MongoValue, ReadPreference};
use hune_source::{
    CERTIFICATE_CODE_NAME, ConnectOptions, MongoClient, MongoDriver, MongoErrorClass, ReadOp, ReadRequest, SourceState,
};
use mgs::*;

async fn up(source: &hune_source::MongoSource) {
    let probe = source.probe().await;
    assert_eq!(probe.state, SourceState::Up, "{probe:?}");
}

// ---- MS01 ----

/// MS01: each macro of 7.34 is expanded and run on the seed data with known times.
#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms01_each_macro_gives_the_expected_documents() {
    let source = make_source(ro_config());
    up(&source).await;
    // `$__timeFrom` and `$__timeTo` are Date values with the millisecond of `from_ms` and `to_ms`: the window
    // [from, to) holds 60 documents, one for each second, and the first one has the Date of `from_ms`.
    let q = pipeline(
        "node_load",
        r#"[{"$match":{"ts":{"$gte":"$__timeFrom","$lt":"$__timeTo"}}},
            {"$group":{"_id":null,"n":{"$sum":1},"first":{"$min":"$ts"},"last":{"$max":"$ts"}}}]"#,
    );
    let frame = run(&source, &q, wide()).await.unwrap();
    assert_eq!(frame.rows, 1);
    assert_eq!(cell(&frame, "n", 0), 60);
    assert_eq!(cell(&frame, "first", 0), serde_json::json!(FROM_MS / 1000));
    assert_eq!(cell(&frame, "last", 0), serde_json::json!((TO_MS - 1000) / 1000));
    // The same with the Int64 macros: `$__from` and `$__to` are Int64 values of milliseconds, and `$toLong`
    // of a Date gives its milliseconds, so the comparison is on exact integers.
    let q = pipeline(
        "node_load",
        r#"[{"$match":{"$expr":{"$and":[{"$gte":[{"$toLong":"$ts"},"$__from"]},{"$lt":[{"$toLong":"$ts"},"$__to"]}]}}},
            {"$group":{"_id":null,"n":{"$sum":1}}}]"#,
    );
    let frame = run(&source, &q, wide()).await.unwrap();
    assert_eq!(cell(&frame, "n", 0), 60);
    // The four typed values come back with their types.
    let q = pipeline(
        "node_load",
        r#"[{"$limit":1},{"$replaceWith":{"from":"$__from","to":"$__to","tf":"$__timeFrom","tt":"$__timeTo",
            "ims":"$__interval_ms","is":"$__interval_s"}}]"#,
    );
    let frame = run(&source, &q, wide()).await.unwrap();
    assert_eq!(column_type(&frame, "from"), "long");
    assert_eq!(column_type(&frame, "to"), "long");
    assert_eq!(column_type(&frame, "tf"), "date");
    assert_eq!(column_type(&frame, "tt"), "date");
    assert_eq!(cell(&frame, "from", 0), serde_json::json!(FROM_MS.to_string()));
    assert_eq!(cell(&frame, "to", 0), serde_json::json!(TO_MS.to_string()));
    assert_eq!(cell(&frame, "tf", 0), serde_json::json!(FROM_MS / 1000));
    assert_eq!(cell(&frame, "tt", 0), serde_json::json!(TO_MS / 1000));
    // `$__interval_s` is the `binSize` of `$dateTrunc` for each step of the table of 7.4.
    let steps: [(i64, i64); 6] = [
        (BASE_MS, BASE_MS + 60_000),
        (BASE_MS, BASE_MS + 3_600_000),
        (BASE_MS, BASE_MS + 6 * 3_600_000),
        (BASE_MS, BASE_MS + 86_400_000),
        (BASE_MS, BASE_MS + 7 * 86_400_000),
        (BASE_MS, BASE_MS + 30 * 86_400_000),
    ];
    for (from, to) in steps {
        let interval = interval_of(from, to);
        let text = pipeline(
            "node_load",
            r#"[{"$match":{"_id":{"$lt":1}}},
                {"$replaceWith":{"ims":"$__interval_ms","is":"$__interval_s",
                  "bucket":{"$dateTrunc":{"date":"$ts","unit":"second","binSize":"$__interval_s"}}}}]"#,
        );
        let query = prepared_range(&text, from, to);
        let frame =
            source.query(hune_source::PreparedQuery::MongoQuery { command: query }, wide(), ctx()).await.unwrap();
        assert_eq!(cell(&frame, "ims", 0), serde_json::json!(interval.ms.to_string()), "{from} {to}");
        assert_eq!(cell(&frame, "is", 0), serde_json::json!(interval.seconds.to_string()), "{from} {to}");
        // The first document is at the base: its bucket is the base, whatever the size of the bin.
        assert_eq!(cell(&frame, "bucket", 0), serde_json::json!(BASE_MS / 1000));
    }
}

// ---- MS04 ----

/// MS04: the limits of 8b.6 and 8b.7 on the seed data.
#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms04_row_limit_is_exact() {
    let source = make_source(ro_config());
    up(&source).await;
    let q =
        |n: u32| pipeline("node_load", &format!(r#"[{{"$match":{{"_id":{{"$lt":{n}}}}}}},{{"$sort":{{"_id":1}}}}]"#));
    // A result of exactly `max_result_rows` documents passes.
    let frame = run(&source, &q(100), limits(100, 100_000_000, 30)).await.unwrap();
    assert_eq!(frame.rows, 100);
    // One more gives `422 query_limit`, and no row.
    let e = run(&source, &q(101), limits(100, 100_000_000, 30)).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_limit", 422), "{e:?}");
    assert!(e.members().iter().any(|(k, _)| *k == "limit"));
    // A result above `max_result_bytes` gives its code.
    let e = run(&source, &q(100), limits(1000, 500, 30)).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_limit", 422), "{e:?}");
    // The rows of a cursor that spans many batches: the limit counts across the batches.
    let e = run(&source, &q(5000), limits(2500, 100_000_000, 30)).await.unwrap_err();
    assert_eq!(e.code(), "query_limit");
    let frame = run(&source, &q(5000), limits(5000, 100_000_000, 30)).await.unwrap();
    assert_eq!(frame.rows, 5000);
}

#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms04_time_limit_gives_query_timeout() {
    let source = make_source(ro_config());
    up(&source).await;
    // 200 documents that each scan the 1,000,000 documents of the collection: far above one second.
    let start = Instant::now();
    let e = run(&source, &slow_query(), limits(10_000, 100_000_000, 1)).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_timeout", 504), "{e:?}");
    // The server stops at `maxTimeMS` (1 s), before the client deadline (6 s).
    assert!(start.elapsed() < Duration::from_secs(5), "{:?}", start.elapsed());
}

#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms04_command_size_limit_gives_sql_too_large() {
    let source = make_source(ro_config());
    up(&source).await;
    let text = pipeline("node_load", r#"[{"$match":{"node":{"$in":["a","b","c","d","e","f","g","h","i","j"]}}}]"#);
    let mut small = wide();
    small.max_query_size = 120;
    let query = prepared(&text);
    let e = source.query(hune_source::PreparedQuery::MongoQuery { command: query }, small, ctx()).await.unwrap_err();
    // The size is checked on the BSON size of the command, before the send.
    assert!(matches!(e.code(), "sql_too_large" | "query_error"), "{e:?}");
    assert_eq!(e.http_status(), 422);
}

#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms04_a_large_group_gives_query_memory_limit() {
    let source = make_source(ro_config());
    up(&source).await;
    let docs: u64 = env("HUNE_TEST_MGS_NODE_LOAD_DOCS").parse().unwrap();
    // A `$push` of each document, with `allowDiskUse: false`: the stage needs more than the 100 MiB of the
    // server (code 292).
    let q = pipeline("node_load", r#"[{"$group":{"_id":null,"all":{"$push":"$$ROOT"}}}]"#);
    let e = run(&source, &q, limits(10, 100_000_000, 30)).await.unwrap_err();
    if docs >= 1_000_000 {
        assert_eq!((e.code(), e.http_status()), ("query_memory_limit", 422), "{e:?}");
    }
}

#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms04_a_large_document_gives_query_limit_and_no_second_document() {
    let proxy = Proxy::start(port()).await;
    let mut config = ro_config();
    config.topology = proxy.topology();
    let source = make_source(config);
    up(&source).await;
    proxy.clear();
    // The document `big` has 10 MiB (`HUNE_TEST_MGS_BIG_BYTES`). With `max_result_bytes` of 1 MiB the first
    // reply stops the query.
    let big: usize = env("HUNE_TEST_MGS_BIG_BYTES").parse().unwrap();
    let q = pipeline("big", r#"[{"$match":{}}]"#);
    let e = run(&source, &q, limits(10, 1 << 20, 30)).await.unwrap_err();
    if big > (1 << 20) {
        assert_eq!((e.code(), e.http_status()), ("query_limit", 422), "{e:?}");
    }
    // The client reads no second document: one `aggregate`, and no `getMore`.
    assert_eq!(proxy.count("aggregate"), 1);
    assert_eq!(proxy.count("getMore"), 0);
    proxy.assert_only_closed_commands();
}

// ---- MS06 ----

/// MS06: each type of 7.16c gives its kind and its value, on the document `types`.
#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms06_each_type_gives_its_kind_and_value_from_the_server() {
    let source = make_source(ro_config());
    up(&source).await;
    let q = pipeline(
        "types",
        r#"[{"$match":{"_id":1}},{"$project":{"_id":0,"t_null":1,"t_bool":1,"t_int32":1,"t_int64_max":1,
            "t_int64_min":1,"t_double":1,"t_double_nan":1,"t_double_inf":1,"t_double_ninf":1,"t_decimal":1,
            "t_string":1,"t_date_ms":1,"t_date_old":1,"t_objectid":1,"t_uuid":1,"t_doc":1,"t_array":1}}]"#,
    );
    let frame = run(&source, &q, wide()).await.unwrap();
    assert_eq!(frame.rows, 1);
    let check = |name: &str, ty: &str, value: serde_json::Value| {
        assert_eq!(column_type(&frame, name), ty, "{name}");
        assert_eq!(cell(&frame, name, 0), value, "{name}");
    };
    check("t_null", "null", serde_json::Value::Null);
    check("t_bool", "bool", serde_json::json!(true));
    check("t_int32", "int", serde_json::json!(2_147_483_647));
    check("t_int64_max", "long", serde_json::json!("9223372036854775807"));
    check("t_int64_min", "long", serde_json::json!("-9223372036854775808"));
    check("t_double", "double", serde_json::json!(1.5));
    check("t_double_nan", "double", serde_json::json!("nan"));
    check("t_double_inf", "double", serde_json::json!("inf"));
    check("t_double_ninf", "double", serde_json::json!("-inf"));
    check("t_decimal", "decimal", serde_json::json!("12345.6700"));
    check("t_string", "string", serde_json::json!("synthetic text"));
    check("t_date_ms", "date", serde_json::json!(1_767_225_600.123_f64));
    check("t_date_old", "date", serde_json::json!(-86_400.123_f64));
    check("t_objectid", "objectId", serde_json::json!("0123456789abcdef01234567"));
    check("t_uuid", "uuid", serde_json::json!("01234567-89ab-4def-8123-456789abcdef"));
    check("t_doc", "object", serde_json::json!(r#"{"a":1,"b":{"c":"deep"}}"#));
    check("t_array", "array", serde_json::json!(r#"[1,"two",{"three":3},[4]]"#));
    // A missing field gives `null`: the document 5 has the field `present` and not the field `t_bool`.
    let q = pipeline(
        "types",
        r#"[{"$match":{"_id":{"$in":[1,5]}}},{"$sort":{"_id":1}},{"$project":{"_id":1,"present":1}}]"#,
    );
    let frame = run(&source, &q, wide()).await.unwrap();
    assert_eq!(cell(&frame, "present", 0), serde_json::Value::Null);
    assert_eq!(cell(&frame, "present", 1), serde_json::json!(1));
    // A column that mixes Int32 and Double passes. A mix of Int32 and String gives `query_error`, with
    // `mongo_code` `null`.
    let q = pipeline(
        "types",
        r#"[{"$match":{"_id":{"$in":[2,3]}}},{"$sort":{"_id":1}},{"$project":{"_id":0,"mixed_num":1}}]"#,
    );
    let frame = run(&source, &q, wide()).await.unwrap();
    assert_eq!(column_type(&frame, "mixed_num"), "double");
    let q = pipeline(
        "types",
        r#"[{"$match":{"_id":{"$in":[2,3]}}},{"$sort":{"_id":1}},{"$project":{"_id":0,"mixed_text":1}}]"#,
    );
    let e = run(&source, &q, wide()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_error", 422), "{e:?}");
    assert!(e.members().iter().any(|(k, v)| *k == "mongo_code" && matches!(v, hune_source::Member::Null)), "{e:?}");
    // A column of each unsupported type gives the same error.
    for field in ["t_regex", "t_timestamp", "t_minkey", "t_maxkey", "t_code", "t_binary"] {
        let q = pipeline("types", &format!(r#"[{{"$match":{{"_id":1}}}},{{"$project":{{"_id":0,"{field}":1}}}}]"#));
        let e = run(&source, &q, wide()).await.unwrap_err();
        assert_eq!((e.code(), e.http_status()), ("query_error", 422), "{field}: {e:?}");
    }
    // A result with no document gives a frame with no column.
    let q = pipeline("types", r#"[{"$match":{"_id":999}}]"#);
    let frame = run(&source, &q, wide()).await.unwrap();
    assert_eq!((frame.rows, frame.columns.len()), (0, 0));
    // A document with 257 fields gives the error of the column limit.
    let q = pipeline("types", r#"[{"$match":{"_id":4}}]"#);
    let e = run(&source, &q, wide()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_error", 422), "{e:?}");
}

// ---- MS08 ----

/// MS08: the user side alone. A plain client logs in as `hune_ro`, and each statement of 8b.13 fails with
/// the code 13. A read of the collection `secret` works, because the role reads it.
#[test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
fn ms08_the_role_alone_refuses_each_statement_of_8b13() {
    let statements = [
        r#"db_("analytics").node_load.insertOne({ x: 1 })"#,
        r#"db_("analytics").node_load.updateOne({}, { $set: { x: 1 } })"#,
        r#"db_("analytics").node_load.deleteMany({})"#,
        r#"db_("analytics").node_load.drop()"#,
        r#"db_("analytics").createCollection("x")"#,
        r#"db_("analytics").node_load.createIndex({ x: 1 })"#,
        r#"db_("admin").createUser({ user: "x", pwd: "y", roles: [] })"#,
        r#"db_("analytics").node_load.aggregate([{ $out: "x" }]).toArray()"#,
        r#"db_("analytics").node_load.aggregate([{ $merge: { into: "x" } }]).toArray()"#,
        r#"db_("admin").aggregate([{ $currentOp: { allUsers: true } }]).toArray()"#,
        r#"db_("admin").runCommand({ serverStatus: 1 })"#,
        r#"db_("admin").runCommand({ getParameter: 1, javascriptEnabled: 1 })"#,
        r#"db_("other").stuff.find({}).toArray()"#,
    ];
    let user = env("HUNE_TEST_MGS_RO_USER");
    let password = env("HUNE_TEST_MGS_RO_PASSWORD");
    for statement in statements {
        let script = format!(
            "try {{ const r = {statement}; if (r && r.ok === 0) {{ print('CODE=' + r.code); }} else {{ print('NOERROR'); }} }} \
             catch (e) {{ print('CODE=' + e.code); }}"
        );
        let out = mongosh(&env("HUNE_TEST_MGS_CONTAINER"), port(), &user, &password, "admin", &script);
        assert!(out.lines().any(|l| l.trim() == "CODE=13"), "{statement} did not fail with the code 13: {out}");
    }
    // The role reads the collection `secret`: the list of the block (7.33) is what refuses it.
    let out = mongosh(
        &env("HUNE_TEST_MGS_CONTAINER"),
        port(),
        &user,
        &password,
        "admin",
        r#"print('N=' + db_("analytics").secret.countDocuments({}));"#,
    );
    assert!(out.lines().any(|l| l.trim() == "N=3"), "{out}");
}

// ---- MS09 ----

fn doc(members: Vec<(&str, MongoValue)>) -> MongoValue {
    MongoValue::Document(members.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn aggregate_request(stages: Vec<MongoValue>) -> ReadRequest {
    ReadRequest {
        database: database(),
        op: ReadOp::Aggregate { collection: "node_load".to_owned(), pipeline: stages },
        batch_size: 101,
        max_time_ms: 30_000,
        read_preference: ReadPreference::Primary,
        comment: doc(vec![
            ("app", MongoValue::String("hune".to_owned())),
            ("kind", MongoValue::String("panel".to_owned())),
        ]),
    }
}

/// MS09: the connection side alone. The client connects as `hune_rw`, a user that can write, and the wire
/// guard still refuses each write key. The proxy shows no command for a refused request.
#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms09_the_wire_guard_refuses_each_write_key_for_a_user_that_can_write() {
    let proxy = Proxy::start(port()).await;
    let config = config_for(&env("HUNE_TEST_MGS_RW_USER"), &env("HUNE_TEST_MGS_RW_PASSWORD"), proxy.topology(), true);
    let driver = MongoDriver::connect(ConnectOptions::from_config(&config)).await.expect("the login works");
    // The probe refuses this user (M2). The test uses the client alone, as the state `up` by hand.
    let match_all = || doc(vec![("$match", doc(vec![]))]);
    let refused: [(&str, MongoValue); 4] = [
        ("$out", doc(vec![("$out", MongoValue::String("copy".to_owned()))])),
        ("$merge", doc(vec![("$merge", doc(vec![("into", MongoValue::String("copy".to_owned()))]))])),
        (
            "$function",
            doc(vec![(
                "$addFields",
                doc(vec![(
                    "a",
                    doc(vec![(
                        "$function",
                        doc(vec![
                            ("body", MongoValue::String("function() { return 1; }".to_owned())),
                            ("args", MongoValue::Array(vec![])),
                            ("lang", MongoValue::String("js".to_owned())),
                        ]),
                    )]),
                )]),
            )]),
        ),
        ("$where", doc(vec![("$match", doc(vec![("$where", MongoValue::String("1".to_owned()))]))])),
    ];
    proxy.clear();
    for (name, stage) in refused {
        let placements: [Vec<MongoValue>; 3] = [
            // At the first level.
            vec![stage.clone()],
            // At the end of a pipeline.
            vec![match_all(), match_all(), stage.clone()],
            // In a `$facet`.
            vec![doc(vec![("$facet", doc(vec![("f", MongoValue::Array(vec![match_all(), stage.clone()]))]))])],
        ];
        for stages in placements {
            let e = driver.read(aggregate_request(stages)).await.unwrap_err();
            assert_eq!(e.class, MongoErrorClass::Guard, "{name}: {e:?}");
        }
    }
    assert_eq!(proxy.count("aggregate"), 0, "{:?}", proxy.commands());
    assert_eq!(proxy.count("find"), 0);
    // A `find` with a refused key is refused too.
    let find = ReadRequest {
        op: ReadOp::Find {
            collection: "node_load".to_owned(),
            filter: doc(vec![("$where", MongoValue::String("1".to_owned()))]),
            projection: None,
            sort: None,
            skip: 0,
            limit: 5,
        },
        ..aggregate_request(vec![])
    };
    assert_eq!(driver.read(find).await.unwrap_err().class, MongoErrorClass::Guard);
    assert_eq!(proxy.count("find"), 0);
    // A request for a collection that the list does not name is refused before the send.
    let secret = ReadRequest {
        op: ReadOp::Aggregate { collection: "secret".to_owned(), pipeline: vec![match_all()] },
        ..aggregate_request(vec![])
    };
    assert_eq!(driver.read(secret).await.unwrap_err().class, MongoErrorClass::Guard);
    assert_eq!(proxy.count("aggregate"), 0);
    // A good read, a cursor, a kill and the probe commands: the proxy sees only the closed list of 8b.5.
    let batch = driver
        .read(ReadRequest { batch_size: 10, ..aggregate_request(vec![doc(vec![("$limit", MongoValue::Int32(1000))])]) })
        .await
        .unwrap();
    assert_eq!(batch.docs.len(), 10);
    let mut cursor = batch.cursor.expect("a cursor");
    let next = driver.get_more(&mut cursor).await.unwrap();
    assert_eq!(next.docs.len(), 10);
    driver.kill_cursor(cursor).await;
    driver.server_info().await.unwrap();
    driver.connection_status().await.unwrap();
    driver.list_collections(&database()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let names = proxy.commands();
    for expected in ["aggregate", "getMore", "killCursors", "buildInfo", "connectionStatus", "listCollections"] {
        assert!(names.iter().any(|n| n == expected), "{expected} missing in {names:?}");
    }
    proxy.assert_only_closed_commands();
    drop(driver);
    tokio::time::sleep(Duration::from_millis(300)).await;
    proxy.assert_only_closed_commands();
}

/// MS09: with `read_preference = "secondary"` on the replica set, a pipeline with the write stage `$out`
/// that a plain client sends fails on the server for the read-only user (the code 13).
///
/// The test does not send the stage with the user `hune_rw`: since MongoDB 5.0 a secondary forwards the
/// write of `$out` and `$merge` to the primary, so a replica does not refuse them (8b.13).
#[test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
fn ms09_a_plain_client_cannot_write_through_a_secondary() {
    let container = container_of(&format!("{}-rs", env("HUNE_TEST_MGS_SERVICE")));
    let (p1, p2) = (port_of("HUNE_TEST_MGS_RS_PORT"), port_of("HUNE_TEST_MGS_RS_PORT2"));
    let uri = format!(
        "mongodb://localhost:{p1},localhost:{p2}/?replicaSet=rs0&readPreference=secondary&tls=true&tlsCAFile=%2Ftls%2Fca.pem"
    );
    let script = r#"
        try { conn.getDB("analytics").node_load.aggregate([{ $out: "copy" }]).toArray(); print("NOERROR"); }
        catch (e) { print("CODE=" + e.code); }
        try { conn.getDB("analytics").node_load.aggregate([{ $merge: { into: "copy" } }]).toArray(); print("NOERROR"); }
        catch (e) { print("CODE=" + e.code); }
    "#;
    let out = mongosh_uri(
        &container,
        &uri,
        &env("HUNE_TEST_MGS_RO_USER"),
        &env("HUNE_TEST_MGS_RO_PASSWORD"),
        "admin",
        script,
    );
    assert!(!out.contains("NOERROR"), "{out}");
    assert_eq!(out.matches("CODE=13").count(), 2, "{out}");
}

/// MS09: no call of a write function of the driver exists in the code (16.18).
#[test]
fn ms09_no_write_call_of_the_driver_in_the_code() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/mongo");
    let forbidden = [
        ".insert_one(",
        ".insert_many(",
        ".update_one(",
        ".update_many(",
        ".replace_one(",
        ".delete_one(",
        ".delete_many(",
        ".create_collection(",
        ".create_index(",
        ".create_indexes(",
        ".drop_index(",
        ".find_one_and_",
        ".bulk_write(",
    ];
    let mut files = vec![root.join("driver.rs")];
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "rs") && !files.contains(&path) {
            files.push(path);
        }
    }
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        // The test modules may name the calls in strings (a test of the guard). The code before `mod tests` may not.
        let code = text.split("#[cfg(test)]").next().unwrap_or(&text);
        for f in forbidden {
            assert!(!code.contains(f), "{} calls {f}", file.display());
        }
    }
}

// ---- MS10 ----

async fn probe_state_of(config: hune_source::MongoSourceConfig) -> hune_source::Probe {
    make_source(config).probe().await
}

/// MS10: the test CA succeeds, and a name that the certificate lacks, another CA, and a server that offers a
/// mechanism other than SCRAM-SHA-256 each stop the connection.
#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms10_the_link_checks_the_certificate_the_name_and_the_mechanism() {
    // The test CA and the right name.
    let probe = probe_state_of(ro_config()).await;
    assert_eq!(probe.state, SourceState::Up, "{probe:?}");
    // The certificate holds the name `localhost` only: a connection to the IP address fails the name check.
    let mut config = ro_config();
    config.topology = direct(&env("HUNE_TEST_MGS_IP"), port());
    let probe = probe_state_of(config).await;
    assert_eq!(probe.state, SourceState::Misconfigured, "{probe:?}");
    // The client reads the CA of the block only: a certificate of another CA fails.
    let wrong = env("HUNE_TEST_MGS_WRONG_CA_FILE");
    let mut config = ro_config();
    config.tls = Some(hune_source::SourceTls { ca_pem: std::fs::read(&wrong).unwrap(), server_name: String::new() });
    config.tls_ca_file = Some(wrong.clone().into());
    let probe = probe_state_of(config).await;
    assert_eq!(probe.state, SourceState::Misconfigured, "{probe:?}");
    // The error class of a failed certificate is the one that 8b.8 maps to `source_misconfigured`.
    let mut config = ro_config();
    config.tls = Some(hune_source::SourceTls { ca_pem: std::fs::read(&wrong).unwrap(), server_name: String::new() });
    config.tls_ca_file = Some(wrong.into());
    let e = MongoDriver::connect(ConnectOptions::from_config(&config)).await.map(|_| ()).unwrap_err();
    assert_eq!((e.class, e.code_name.as_deref()), (MongoErrorClass::Tls, Some(CERTIFICATE_CODE_NAME)), "{e:?}");
    // A server that offers only SCRAM-SHA-1: Hune offers no other mechanism, so the login fails.
    let config = config_for(
        &env("HUNE_TEST_MGS_SHA1_USER"),
        &env("HUNE_TEST_MGS_LEGACY_PASSWORD"),
        direct(&host(), port_of("HUNE_TEST_MGS_SHA1_PORT")),
        true,
    );
    let probe = probe_state_of(config).await;
    assert_eq!(probe.state, SourceState::Misconfigured, "{probe:?}");
    // A server that offers only PLAIN: the same.
    let config = config_for(
        &env("HUNE_TEST_MGS_PLAIN_USER"),
        &env("HUNE_TEST_MGS_LEGACY_PASSWORD"),
        direct(&host(), port_of("HUNE_TEST_MGS_PLAIN_PORT")),
        true,
    );
    let probe = probe_state_of(config).await;
    assert_eq!(probe.state, SourceState::Misconfigured, "{probe:?}");
}

/// MS10: `tls_mode = "disable"` works on the loopback interface. The proxy has no TLS on its client side.
#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms10_tls_mode_disable_works_on_the_loopback_interface() {
    let proxy = Proxy::start_plain(port()).await;
    let mut config = ro_config();
    config.topology = proxy.topology();
    config.tls_mode = hune_source::MongoTlsMode::Disable;
    config.tls = None;
    config.tls_ca_file = None;
    let source = make_source(config);
    up(&source).await;
    let frame = run(&source, &pipeline("node_load", r#"[{"$limit":2}]"#), wide()).await.unwrap();
    assert_eq!(frame.rows, 2);
    assert!(proxy.connections() >= 1);
}

/// The text of the entry of the package `mongodb` in `Cargo.lock`.
fn mongodb_lock_entry() -> String {
    let lock = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"))
        .expect("Cargo.lock");
    let start = lock.find("name = \"mongodb\"\n").expect("the package mongodb is in the lock");
    let rest = &lock[start..];
    rest.split("\n[[package]]").next().unwrap_or(rest).to_owned()
}

/// MS10: no SRV and no TXT lookup, and no system CA store. The driver has no resolver and no source of
/// roots other than the CA file of the block: the lock file shows no crate for them in the closure of
/// `mongodb`.
#[test]
fn ms10_the_driver_has_no_resolver_and_no_system_store() {
    let entry = mongodb_lock_entry();
    for name in ["hickory", "trust-dns", "rustls-native-certs", "openssl-probe", "native-tls", "openssl", "schannel"] {
        assert!(!entry.contains(&format!("\"{name}")), "the driver depends on {name}:\n{entry}");
    }
}

/// MS10: the environment variables of 13.24 change nothing. The test starts the inner test again, with the
/// variables set to values that would help or harm a client that reads them.
#[test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
fn ms10_the_environment_changes_nothing() {
    let exe = std::env::current_exe().expect("test binary");
    let right = ca_file();
    let out = std::process::Command::new(exe)
        .args(["--exact", "ms10_inner_environment", "--include-ignored", "--nocapture"])
        .env("SSL_CERT_FILE", &right)
        .env("SSL_CERT_DIR", std::path::Path::new(&right).parent().unwrap())
        .env("MONGODB_URI", "mongodb+srv://elsewhere.invalid/?tls=false")
        .env("OTEL_RUST_INSTRUMENTATION_MONGODB_ENABLED", "true")
        .env("OTEL_SDK_DISABLED", "false")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "socks5://127.0.0.1:9")
        .env("RUST_LOG", "trace")
        .output()
        .expect("the test binary runs");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "{text}");
    assert!(text.contains("1 passed"), "{text}");
}

#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms10_inner_environment() {
    // With the right CA in the block, the probe passes. With the wrong CA, it fails, although `SSL_CERT_FILE`
    // names the right one when the outer test sets it.
    let probe = probe_state_of(ro_config()).await;
    assert_eq!(probe.state, SourceState::Up, "{probe:?}");
    let wrong = env("HUNE_TEST_MGS_WRONG_CA_FILE");
    let mut config = ro_config();
    config.tls = Some(hune_source::SourceTls { ca_pem: std::fs::read(&wrong).unwrap(), server_name: String::new() });
    config.tls_ca_file = Some(wrong.into());
    let probe = probe_state_of(config).await;
    assert_eq!(probe.state, SourceState::Misconfigured, "{probe:?}");
}

// ---- MS11 ----

/// MS11: the probe. The user `hune_ro` gives `up`. Each deviant user gives `misconfigured`, and the reason
/// names its check. The advice A1 and A2 each give one change.
#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms11_the_probe_checks_each_user_of_the_fixture() {
    let probe = probe_state_of(ro_config()).await;
    assert_eq!(probe.state, SourceState::Up, "{probe:?}");
    assert_eq!(probe.version_supported, Some(true));
    assert!(probe.version.as_deref().is_some_and(|v| v.starts_with(&version())), "{probe:?}");
    let password = env("HUNE_TEST_MGS_BAD_PASSWORD");
    for item in env("HUNE_TEST_MGS_BAD_USERS").split_whitespace() {
        let mut parts = item.split(':');
        let (user, check, auth_db) = (parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap());
        let mut config = config_for(user, &password, direct(&host(), port()), true);
        if check == "M1" {
            // A user of another database cannot log in with the key `auth_database`: the login fails first, so
            // the state is `misconfigured` and the check M1 stays a second line of defence. The same user passes
            // with its own database in the key (the role `read` on `analytics` only).
            let own = probe_state_of(config_for(user, &password, direct(&host(), port()), true)).await;
            assert_eq!(own.state, SourceState::Misconfigured, "{user}: {own:?}");
            config.auth_database = auth_db.to_owned();
            let probe = probe_state_of(config).await;
            assert_eq!(probe.state, SourceState::Up, "{user}: {probe:?}");
            continue;
        }
        config.auth_database = auth_db.to_owned();
        let probe = probe_state_of(config).await;
        assert_eq!(probe.state, SourceState::Misconfigured, "{user}: {probe:?}");
        let detail = probe.detail.unwrap_or_default();
        // A user of M3 holds a custom role, so it fails M2 first (8b.13: M2 runs before M3).
        let first = if check == "M3" { "M2" } else { check };
        assert!(detail.contains(first), "{user} must fail {first}: {detail}");
    }
}

#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms11_the_advice_a1_and_a2_each_write_one_change() {
    // A1: the key lists a collection that exists nowhere. A2: the user reads a collection that the key lacks
    // (`secret`).
    let mut config = ro_config();
    config.collections.push("ghost".to_owned());
    let source = make_source(config);
    up(&source).await;
    let changes = source.take_advice_changes();
    let a1: Vec<_> = changes.iter().filter(|c| c.check == "A1").collect();
    let a2: Vec<_> = changes.iter().filter(|c| c.check == "A2").collect();
    assert_eq!(a1.len(), 1, "{changes:?}");
    assert_eq!(a2.len(), 1, "{changes:?}");
    assert!(a1[0].names.iter().any(|n| n.contains("ghost")), "{changes:?}");
    assert!(a2[0].names.iter().any(|n| n.contains("secret")), "{changes:?}");
    // The same advice at the next probe gives no new change.
    source.probe().await;
    assert!(source.take_advice_changes().is_empty());
    assert!(source.take_version_unsupported().is_empty());
}

// ---- MS12 ----

/// MS12: the replica set of two members. The right list gives `up`. A list that lacks a member, and a set
/// name that differs, each give `misconfigured` with the check M4.
#[tokio::test]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms12_the_replica_set_list_and_name_are_checked() {
    let (p1, p2) = (port_of("HUNE_TEST_MGS_RS_PORT"), port_of("HUNE_TEST_MGS_RS_PORT2"));
    let name = env("HUNE_TEST_MGS_RS_NAME");
    let ro = (env("HUNE_TEST_MGS_RO_USER"), env("HUNE_TEST_MGS_RO_PASSWORD"));
    let block = |hosts: &[(&str, u16)], set: &str| config_for(&ro.0, &ro.1, rs_topology(hosts, set), true);
    // The right list and the right name.
    let source = make_source(block(&[("localhost", p1), ("localhost", p2)], &name));
    let probe = source.probe().await;
    assert_eq!(probe.state, SourceState::Up, "{probe:?}");
    let frame = run(&source, &pipeline("node_load", r#"[{"$limit":3}]"#), wide()).await.unwrap();
    assert_eq!(frame.rows, 3);
    // A list that lacks the second member.
    let probe = make_source(block(&[("localhost", p1)], &name)).probe().await;
    assert_eq!(probe.state, SourceState::Misconfigured, "{probe:?}");
    assert!(probe.detail.unwrap_or_default().contains("M4"));
    // A set name that differs.
    let probe = make_source(block(&[("localhost", p1), ("localhost", p2)], "rsother")).probe().await;
    assert_eq!(probe.state, SourceState::Misconfigured, "{probe:?}");
    assert!(probe.detail.unwrap_or_default().contains("M4"));
}

// ---- MS05 ----

/// The query that streams the whole collection `node_load`: many batches, with a cursor.
fn streaming_query() -> String {
    pipeline("node_load", r#"[{"$match":{}}]"#)
}

/// A query that runs long on the server: 200 documents that each scan the whole collection. The join uses a
/// variable of the outer document, so the server cannot keep one result for all of them.
fn slow_query() -> String {
    pipeline(
        "node_load",
        r#"[{"$limit":200},{"$lookup":{"from":"node_load","let":{"c":"$cpu"},
            "pipeline":[{"$match":{"$expr":{"$eq":["$cpu","$$c"]}}},{"$count":"n"}],"as":"x"}}]"#,
    )
}

fn streaming_limits() -> hune_source::EffectiveLimits {
    limits(2_000_000, 1 << 30, 30)
}

async fn wait_for(proxy: &Proxy, name: &str, count: usize, within: Duration) {
    let start = Instant::now();
    while proxy.count(name) < count {
        assert!(start.elapsed() < within, "the proxy saw {:?}", proxy.commands());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Starts a watch of `currentOp` for the query id. The watch runs while the query runs.
async fn start_watch(query_id: String) -> OpWatch {
    tokio::task::spawn_blocking(move || watch_ops(&query_id)).await.unwrap()
}

/// The time from `from` until the watch sees no operation of the query. The watch polls every 50 ms.
async fn gone_within(watch: OpWatch, from: u128) -> Result<Duration, String> {
    tokio::task::spawn_blocking(move || watch.wait_gone(from, Duration::from_secs(30))).await.unwrap()
}

/// MS05: a query with a cursor, then the client leaves. `currentOp` shows no operation of the query within 2 s,
/// and the proxy shows one `killCursors`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms05_a_client_that_leaves_kills_the_cursor() {
    let proxy = Proxy::start(port()).await;
    let mut config = ro_config();
    config.topology = proxy.topology();
    let source = make_source(config);
    up(&source).await;
    proxy.clear();
    let context = ctx();
    let query_id = context.query_id.to_string();
    let watch = start_watch(query_id.clone()).await;
    let query = hune_source::PreparedQuery::MongoQuery { command: prepared(&streaming_query()) };
    let task = {
        let source = source.clone();
        tokio::spawn(async move { source.query(query, streaming_limits(), context).await })
    };
    wait_for(&proxy, "getMore", 2, Duration::from_secs(20)).await;
    let watch = tokio::task::spawn_blocking(move || {
        watch.wait_seen(Duration::from_secs(20)).expect("the query is in currentOp while it runs");
        watch
    })
    .await
    .unwrap();
    // The client leaves: the future of the query drops.
    let left = epoch_ms();
    task.abort();
    let _ = task.await;
    let wait = gone_within(watch, left).await.expect("no operation within 2 s");
    assert!(wait <= Duration::from_secs(2), "{wait:?}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(proxy.count("killCursors"), 1, "{:?}", proxy.commands());
    proxy.assert_only_closed_commands();
}

/// MS05: a query with no cursor yet leaves `currentOp` within `maxTimeMS` plus 2 s. The client cannot kill it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms05_a_command_with_no_cursor_leaves_currentop_within_max_time_plus_2s() {
    let source = make_source(ro_config());
    up(&source).await;
    let context = ctx();
    let query_id = context.query_id.to_string();
    let query = hune_source::PreparedQuery::MongoQuery { command: prepared(&slow_query()) };
    let max_time = Duration::from_secs(3);
    let watch = start_watch(query_id).await;
    let task = {
        let source = source.clone();
        tokio::spawn(async move { source.query(query, limits(10_000, 1 << 30, 3), context).await })
    };
    let watch = tokio::task::spawn_blocking(move || {
        watch.wait_seen(Duration::from_secs(20)).expect("the command is in currentOp while it runs");
        watch
    })
    .await
    .unwrap();
    let left = epoch_ms();
    task.abort();
    let _ = task.await;
    // The server stops the command at `maxTimeMS`, counted from its start.
    let within = max_time + Duration::from_secs(2);
    let wait = gone_within(watch, left).await.expect("the command leaves currentOp");
    assert!(wait <= within, "{wait:?}");
}

/// MS05: the same at the client deadline. The proxy delays the answers, so the deadline of 8b.7 passes. The
/// source answers `query_timeout`, kills the cursor once, and `currentOp` shows nothing within 2 s.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms05_the_client_deadline_kills_the_cursor() {
    let proxy = Proxy::start(port()).await;
    let mut config = ro_config();
    config.topology = proxy.topology();
    let source = make_source(config);
    up(&source).await;
    proxy.clear();
    let context = ctx();
    let query_id = context.query_id.to_string();
    let watch = start_watch(query_id).await;
    let query = hune_source::PreparedQuery::MongoQuery { command: prepared(&streaming_query()) };
    let task = {
        let source = source.clone();
        // The deadline is the whole seconds of `max_execution_time` plus 5 s: 6 s.
        tokio::spawn(async move { source.query(query, limits(2_000_000, 1 << 30, 1), context).await })
    };
    wait_for(&proxy, "getMore", 1, Duration::from_secs(20)).await;
    proxy.delay_replies(Duration::from_secs(12));
    let start = Instant::now();
    let result = task.await.unwrap();
    let e = result.unwrap_err();
    assert_eq!((e.code(), e.http_status()), ("query_timeout", 504), "{e:?}");
    assert!(start.elapsed() < Duration::from_secs(9), "{:?}", start.elapsed());
    proxy.delay_replies(Duration::ZERO);
    let wait = gone_within(watch, epoch_ms()).await.expect("no operation within 2 s");
    assert!(wait <= Duration::from_secs(2), "{wait:?}");
    assert_eq!(proxy.count("killCursors"), 1, "{:?}", proxy.commands());
}

// ---- MS14 ----

/// MS14: the pool. A connection that the pool made while the read ran gives no second send. A pooled
/// connection that the other side closed gives one send again and a success. After 70 s without a query, the
/// next query succeeds on the first attempt.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms14_the_pool_retries_a_closed_link_one_time_and_only_for_a_pooled_connection() {
    let proxy = Proxy::start(port()).await;
    let mut config = ro_config();
    config.topology = proxy.topology();
    let source = make_source(config);
    up(&source).await;
    let quick = pipeline("node_load", r#"[{"$limit":3}]"#);
    let find = find_query("node_load", r#"{"filter":{"_id":{"$lt":3}}}"#);

    // 1. A connection that the pool makes for this read, and that the proxy closes after the command went out:
    //    `503 source_unavailable`, and the proxy shows no second command. The pool has one connection, and a
    //    slow query holds it, so the `find` runs on a new connection.
    proxy.clear();
    let slow = {
        let source = source.clone();
        let query = hune_source::PreparedQuery::MongoQuery { command: prepared(&slow_query()) };
        tokio::spawn(async move { source.query(query, limits(10_000, 1 << 30, 3), ctx()).await })
    };
    wait_for(&proxy, "aggregate", 1, Duration::from_secs(10)).await;
    proxy.close_after("find");
    let e = run(&source, &find, wide()).await.unwrap_err();
    assert_eq!((e.code(), e.http_status(), e.retryable()), ("source_unavailable", 503, true), "{e:?}");
    assert_eq!(proxy.count("find"), 1, "no second command: {:?}", proxy.commands());
    slow.abort();
    let _ = slow.await;
    proxy.assert_only_closed_commands();

    // 2. A pooled connection that the other side closed before the command: one retry and a success. The
    //    source goes back to `up` first, with a probe, so that the pool holds a connection.
    let probe = source.probe().await;
    assert_eq!(probe.state, SourceState::Up, "{probe:?}");
    run(&source, &quick, wide()).await.unwrap();
    proxy.clear();
    let before = proxy.connections();
    proxy.close_pool_connections();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let frame = run(&source, &quick, wide()).await.expect("one retry gives a success");
    assert_eq!(frame.rows, 3);
    assert!(proxy.connections() > before, "the retry opened a new connection");
    assert_eq!(proxy.count("aggregate"), 1, "{:?}", proxy.commands());

    // 3. After 70 s without a query, the pool has closed its idle connection (60 s). The next query succeeds
    //    on the first attempt: one `aggregate` on a new connection.
    tokio::time::sleep(Duration::from_secs(70)).await;
    proxy.clear();
    let before = proxy.connections();
    let frame = run(&source, &quick, wide()).await.expect("the first attempt succeeds");
    assert_eq!(frame.rows, 3);
    assert_eq!(proxy.count("aggregate"), 1, "{:?}", proxy.commands());
    assert!(proxy.connections() > before, "the idle connection was closed");
    proxy.assert_only_closed_commands();
}

// ---- MS15 ----

/// MS15: attribution. The profiler holds the `comment` of 8b.11 with the `query_id` and the `uid`. For 1,000
/// random subjects, the comment fits 1,024 bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms15_the_comment_names_the_query_and_fits_1024_bytes() {
    let source = make_source(ro_config());
    up(&source).await;
    // A profile collection that holds the whole run.
    let reset = tokio::task::spawn_blocking(|| {
        root_shell(
            r#"const a = db_('analytics'); a.setProfilingLevel(0); a.system.profile.drop();
               a.createCollection('system.profile', {capped: true, size: 256 * 1024 * 1024});
               a.setProfilingLevel(2); print('PROFILE=ON');"#,
        )
    })
    .await
    .unwrap();
    assert!(reset.contains("PROFILE=ON"), "{reset}");
    let first = ctx();
    let first_id = first.query_id.to_string();
    run_with(&source, &pipeline("node_load", r#"[{"$limit":2}]"#), wide(), first).await.unwrap();
    let mut random = Lcg(0x5eed_1234);
    for i in 0..1000 {
        // The subject has 0 to 700 bytes, with text of 1, 2 and 3 bytes for each character.
        let len = usize::try_from(random.next() % 240).unwrap();
        let subject: String = (0..len)
            .map(|_| match random.next() % 4 {
                0 => 'a',
                1 => 'é',
                2 => '漢',
                _ => char::from(b'0' + u8::try_from(random.next() % 10).unwrap()),
            })
            .collect();
        let mut context = ctx_for(&format!("user:{i}"));
        context.log_comment.subject = Some(subject);
        context.log_comment.request_id = Some(format!("r-{:x}", random.next()));
        run_with(&source, &pipeline("node_load", r#"[{"$limit":1}]"#), wide(), context).await.unwrap();
    }
    let report = tokio::task::spawn_blocking(move || {
        root_shell(&format!(
            r#"const a = db_('analytics');
               const mine = a.system.profile.find({{'command.comment.app': 'hune', 'command.aggregate': 'node_load'}}).toArray();
               const first = mine.find(e => e.command.comment.query_id === '{first_id}');
               const sizes = a.system.profile.aggregate([{{$match: {{'command.comment.app': 'hune'}}}},
                 {{$project: {{n: {{$bsonSize: '$command.comment'}}}}}},
                 {{$group: {{_id: null, max: {{$max: '$n'}}, c: {{$sum: 1}}}}}}]).toArray()[0];
               print('FIRST_UID=' + (first ? first.command.comment.uid : 'none'));
               print('FIRST_KIND=' + (first ? first.command.comment.kind : 'none'));
               print('ENTRIES=' + sizes.c); print('MAX=' + sizes.max);
               a.setProfilingLevel(0);"#
        ))
    })
    .await
    .unwrap();
    let get = |key: &str| {
        report
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .unwrap_or_else(|| panic!("{key} in {report}"))
            .trim()
            .to_owned()
    };
    assert_eq!(get("FIRST_UID="), "nodes");
    assert_eq!(get("FIRST_KIND="), "panel");
    assert!(get("ENTRIES=").parse::<u32>().unwrap() >= 1001, "{report}");
    assert!(get("MAX=").parse::<u32>().unwrap() <= 1024, "{report}");
}

// ---- MS18 ----

/// MS18: the server without `--noscripting`. A plain client runs `$where` and the server permits it. Hune
/// refuses the same text by the grammar, and the proxy shows no command. This case records the known limit
/// of 8b.15.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms18_a_server_with_scripting_on_permits_where_and_hune_refuses_it() {
    let service = format!("{}-js", env("HUNE_TEST_MGS_SERVICE"));
    let container = container_of(&service);
    let js_port = port_of("HUNE_TEST_MGS_JS_PORT");
    let user = env("HUNE_TEST_MGS_RO_USER");
    let password = env("HUNE_TEST_MGS_RO_PASSWORD");
    let script = r#"try { print('N=' + db_('analytics').node_load.find({ $where: 'this._id == 1' }).toArray().length); }
                    catch (e) { print('CODE=' + e.code); }"#;
    let (c, u, p) = (container.clone(), user.clone(), password.clone());
    let out = tokio::task::spawn_blocking(move || mongosh(&c, js_port, &u, &p, "admin", script)).await.unwrap();
    assert!(out.lines().any(|l| l.trim() == "N=1"), "the server must permit $where: {out}");
    // The main server runs with `--noscripting`: the same statement fails there.
    let main = env("HUNE_TEST_MGS_CONTAINER");
    let (u, p) = (user.clone(), password.clone());
    let out = tokio::task::spawn_blocking(move || mongosh(&main, port(), &u, &p, "admin", script)).await.unwrap();
    assert!(out.contains("CODE="), "the main server must refuse $where: {out}");

    // Hune refuses the same text: the grammar at the expansion, and the wire guard if a tree reached it.
    let proxy = Proxy::start(js_port).await;
    let mut config = config_for(&user, &password, proxy.topology(), true);
    config.collections = collections();
    let source = make_source(config);
    up(&source).await;
    proxy.clear();
    let text = find_query("node_load", r#"{"filter":{"$where":"this._id == 1"}}"#);
    let refused = try_prepared(&text, FROM_MS, TO_MS);
    assert!(matches!(refused, Err(hune_core::mongo::MongoExpandError::Invalid(_))), "{refused:?}");
    let text = pipeline(
        "node_load",
        r#"[{"$match":{"$expr":{"$function":{"body":"function() { return true; }","args":[],"lang":"js"}}}}]"#,
    );
    assert!(try_prepared(&text, FROM_MS, TO_MS).is_err());
    assert!(proxy.commands().iter().all(|c| !matches!(c.as_str(), "aggregate" | "find")), "{:?}", proxy.commands());
    // A normal query on the same server works.
    let frame = run(&source, &pipeline("node_load", r#"[{"$limit":2}]"#), wide()).await.unwrap();
    assert_eq!(frame.rows, 2);
}

// ---- MS02, MS03, MS07: the parts that need a server ----

const REFUSED: &str = include_str!("../../../tests/golden/pipeline-mongo/refused.json");

/// The accepted texts of the golden files of 7.31. One text names the database `metrics`, which no block lists.
fn golden_accepted() -> Vec<(String, String)> {
    let files = [
        include_str!("../../../tests/golden/pipeline-mongo/stages.json"),
        include_str!("../../../tests/golden/pipeline-mongo/query-operators.json"),
        include_str!("../../../tests/golden/pipeline-mongo/expression-operators.json"),
        include_str!("../../../tests/golden/pipeline-mongo/accumulators.json"),
        include_str!("../../../tests/golden/pipeline-mongo/window-operators.json"),
    ];
    let mut out = Vec::new();
    for text in files {
        let doc: serde_json::Value = serde_json::from_str(text).unwrap();
        for case in doc["cases"].as_array().unwrap() {
            if case["expect"] != "accepted" {
                continue;
            }
            let lines: Vec<&str> = case["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
            let text = lines.join("\n");
            if text.contains("\"database\"") {
                continue;
            }
            out.push((case["id"].as_str().unwrap().to_owned(), text));
        }
    }
    out
}

/// MS02: each stage and each operator of 7.31 that the grammar accepts runs on the seed data of the server. The
/// server of 7.0 and the one of 8.0 both know each name of the table (no code 168 and no code 40324).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms02_each_accepted_name_of_7_31_runs_on_the_server() {
    let mut config = ro_config();
    config.topology = direct(&host(), port_of("HUNE_TEST_MGS_RS_PORT"));
    let source = make_source(config);
    up(&source).await;
    let texts = golden_accepted();
    assert!(texts.len() >= 200, "{} accepted texts", texts.len());
    let (mut frames, mut server_refusals) = (0, 0);
    let mut bad = Vec::new();
    for (id, text) in &texts {
        match run(&source, text, wide()).await {
            Ok(_) => frames += 1,
            Err(e) if e.code() == "query_error" => {
                let code = e.members().into_iter().find_map(|(k, v)| match (k, v) {
                    ("mongo_code", hune_source::Member::Int(n)) => Some(n),
                    ("mongo_code", hune_source::Member::Uint(n)) => i64::try_from(n).ok(),
                    _ => None,
                });
                // The server does not know the operator: the table of 7.31 is wrong for this release.
                if matches!(code, Some(168 | 40324)) {
                    bad.push(format!("{id}: {code:?} {}", e.message()));
                }
                server_refusals += 1;
            }
            Err(e) => bad.push(format!("{id}: {} {}", e.code(), e.message())),
        }
    }
    assert!(bad.is_empty(), "{} texts failed:\n{}", bad.len(), bad.join("\n"));
    // The seed data is small and typed: most texts run to a frame, and the server refuses the rest for the types.
    assert!(frames >= 150, "{frames} frames and {server_refusals} refusals of the server");
}

/// MS03: for 2,000 random values, a query that holds the value in a whole-leaf placeholder and in an embedded
/// placeholder returns the value, unchanged, as a document. The values hold `$`, `$$`, `.`, quote marks,
/// backslashes, `{`, `}`, U+2028 and long texts. A value that starts with `$` is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms03_a_random_value_comes_back_unchanged_from_the_server() {
    use hune_core::mongo::MongoExpandError;
    use hune_core::sql::{RequestValues, TextValueType, VariableDef, resolve_values};
    let mut config = ro_config();
    config.topology = direct(&host(), port_of("HUNE_TEST_MGS_RS_PORT"));
    let source = make_source(config);
    up(&source).await;
    const PIECES: [&str; 22] = [
        "$",
        "$$",
        "$$ROOT",
        "$secret",
        ".",
        "a.b",
        "\"",
        "'",
        "\\",
        "\\\"",
        "{",
        "}",
        "{\"$out\":\"x\"}",
        "[\"$secret\"]",
        "\u{2028}",
        "\u{1f600}",
        "${host}",
        "$__timeFrom",
        "text",
        " ",
        "a$b",
        "0",
    ];
    let defs = [VariableDef::text("host", TextValueType::Text)];
    let mut random = Lcg(7);
    let (mut whole, mut refused) = (0, 0);
    for case in 0..2000 {
        let mut value = String::new();
        for _ in 0..1 + random.next() % 6 {
            value.push_str(PIECES[usize::try_from(random.next()).unwrap() % PIECES.len()]);
        }
        if random.next().is_multiple_of(50) {
            value.push_str(&"x".repeat(800 + usize::try_from(random.next() % 100).unwrap()));
        }
        let mut request = RequestValues::new();
        request.insert("host", false, &[&value]);
        let values = resolve_values(&defs, &request).unwrap();
        let text = pipeline(
            "node_load",
            r#"[{"$limit":1},{"$replaceWith":{"whole":"$host","embedded":"p${host}s","n":{"$literal":1}}}]"#,
        );
        match prepare_with(&text, &values) {
            Ok(query) => {
                assert!(!value.starts_with('$'), "case {case}: {value:?} accepted");
                let frame = source
                    .query(hune_source::PreparedQuery::MongoQuery { command: query }, wide(), ctx())
                    .await
                    .unwrap_or_else(|e| panic!("case {case}: {value:?}: {e:?}"));
                assert_eq!(cell(&frame, "whole", 0), serde_json::json!(value), "case {case}");
                assert_eq!(cell(&frame, "embedded", 0), serde_json::json!(format!("p{value}s")), "case {case}");
                whole += 1;
            }
            Err(e) => {
                assert!(value.starts_with('$'), "case {case}: {value:?}: {e:?}");
                assert!(matches!(e, MongoExpandError::ValueRefused(_)), "case {case}: {e:?}");
                refused += 1;
            }
        }
    }
    assert!(whole > 500 && refused > 100, "{whole} {refused}");
}

/// MS07: each refused text gives a refusal at the expansion, and the proxy shows no `aggregate` and no `find` for
/// any of them. A refused name inside a string value gives no refusal and runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a MongoDB container: run tests/ci/mongosource.sh"]
async fn ms07_no_refused_text_reaches_the_server() {
    let proxy = Proxy::start(port_of("HUNE_TEST_MGS_RS_PORT")).await;
    let mut config = ro_config();
    config.topology = proxy.topology();
    let source = make_source(config);
    up(&source).await;
    proxy.clear();
    let doc: serde_json::Value = serde_json::from_str(REFUSED).unwrap();
    let (mut refused, mut accepted) = (0, 0);
    for case in doc["cases"].as_array().unwrap() {
        let lines: Vec<&str> = case["lines"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
        let text = lines.join("\n");
        let id = case["id"].as_str().unwrap();
        if case["expect"] == "accepted" {
            let query = try_prepared(&text, FROM_MS, TO_MS).unwrap_or_else(|e| panic!("{id}: {e:?}"));
            // The server may refuse the text for its own reasons (a variable that it does not know): that is
            // `query_error`. Any other answer is a fault.
            let answer = source.query(hune_source::PreparedQuery::MongoQuery { command: query }, wide(), ctx()).await;
            if let Err(e) = answer {
                assert_eq!(e.code(), "query_error", "{id}: {e:?}");
            }
            accepted += 1;
            continue;
        }
        let before = proxy.count("aggregate") + proxy.count("find");
        // The engine refuses the text: no tree exists, so nothing goes to the source.
        assert!(
            matches!(try_prepared(&text, FROM_MS, TO_MS), Err(hune_core::mongo::MongoExpandError::Invalid(_))),
            "{id}: the text is not refused"
        );
        assert_eq!(proxy.count("aggregate") + proxy.count("find"), before, "{id}");
        refused += 1;
    }
    assert!(refused >= 400, "{refused} refused texts");
    // The proxy saw one command for each accepted text, and nothing for a refused one.
    assert_eq!(proxy.count("aggregate") + proxy.count("find"), accepted, "{:?}", proxy.commands());
    proxy.assert_only_closed_commands();
}
