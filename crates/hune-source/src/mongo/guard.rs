// SPDX-License-Identifier: MIT OR Apache-2.0
//! The wire guard (8b.5, 8b.3 rule 3).
//!
//! The guard is the last check before a command goes to the encoder. The driver module calls
//! [`check_read_request`] on each [`ReadRequest`], and the source calls it too, as a second barrier.
//! The guard does not depend on the grammar of 7.31: a fault of the grammar cannot send a write.
//!
//! The guard refuses a command that has one of these faults:
//!
//! - A key `$out`, `$merge`, `$function`, `$accumulator`, `$where` or `$code` at any depth of the
//!   command document. The check walks the whole document, with the comment and the options. It
//!   reads keys only: a string value that holds one of these names gives no refusal.
//! - A value of a BSON type that the model does not read (`MongoValue::Other`), JavaScript code
//!   included. Such a value has no form on the wire.
//! - A key `$graphLookup`. The stage reads a collection that no list names.
//! - A database or a collection that the lists of the block do not hold.
//! - A `$lookup` whose `from` is not a name of the list `collections`, and a `$unionWith` whose
//!   collection is not a name of the list. A `from` that is an object is refused too.
//! - A command whose name is not in the closed list of 8b.5, or is in the list of the names that Hune
//!   never sends.
//!
//! A refusal names the fault and never the query text or a value. The caller maps it to a
//! `MongoError` of the class `Guard`, and the source answers `500 internal_error`.

use hune_core::mongo::command::{COMMAND_NAMES, REFUSED_COMMAND_NAMES};
use hune_core::mongo::{MongoCommand, MongoValue};

use crate::mongo::iface::{MongoError, MongoErrorClass, ReadRequest};

/// The keys that no command document holds at any depth (8b.5).
pub const REFUSED_KEYS: [&str; 6] = ["$out", "$merge", "$function", "$accumulator", "$where", "$code"];

/// The stage that the guard refuses in addition: it reads a collection that no list names.
const REFUSED_STAGE: &str = "$graphLookup";

/// The reason of a refusal. The text names a key, a stage or a name of the lists. It holds no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    reason: String,
}

impl Refusal {
    fn new(reason: impl Into<String>) -> Refusal {
        Refusal { reason: reason.into() }
    }

    /// Gives the reason.
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

/// Walks a value at any depth and refuses a document that has a refused key, or a join that leaves
/// the list of the collections.
///
/// The walk uses a stack, so a deep value cannot overflow the call stack.
///
/// # Errors
///
/// A refused key or stage, or a join to a name that is not in `collections`.
pub fn check_value(value: &MongoValue, collections: Option<&[String]>) -> Result<(), Refusal> {
    let mut stack = vec![value];
    while let Some(v) = stack.pop() {
        match v {
            MongoValue::Array(items) => stack.extend(items.iter()),
            // A type that the model does not read has no wire form. JavaScript code is one of them.
            MongoValue::Other(t) => {
                return Err(Refusal::new(format!("The command holds a value of the type {}.", t.as_str())));
            }
            MongoValue::Document(members) => {
                for (key, item) in members {
                    if REFUSED_KEYS.contains(&key.as_str()) {
                        return Err(Refusal::new(format!("The command holds the key {key}.")));
                    }
                    match key.as_str() {
                        REFUSED_STAGE => return Err(Refusal::new(format!("The command holds the stage {key}."))),
                        "$lookup" if collections.is_some() => check_join(key, item, "from", collections)?,
                        "$unionWith" if collections.is_some() => check_union(item, collections)?,
                        _ => {}
                    }
                    stack.push(item);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `$lookup`: the member `from` is a string of the list of the collections.
fn check_join(stage: &str, body: &MongoValue, member: &str, collections: Option<&[String]>) -> Result<(), Refusal> {
    let collections = collections.unwrap_or_default();
    // A value that is not a document is not a stage: the server refuses it.
    let Some(members) = body.as_document() else { return Ok(()) };
    match members.iter().find(|(k, _)| k == member).map(|(_, v)| v) {
        Some(MongoValue::String(name)) if collections.iter().any(|c| c == name) => Ok(()),
        Some(MongoValue::String(_)) => {
            Err(Refusal::new(format!("The stage {stage} names a collection that is not listed.")))
        }
        Some(_) => Err(Refusal::new(format!("The member {member} of the stage {stage} is not a collection name."))),
        None => Err(Refusal::new(format!("The stage {stage} has no member {member}."))),
    }
}

/// `$unionWith`: a collection name, or an object with the member `coll`.
fn check_union(body: &MongoValue, collections: Option<&[String]>) -> Result<(), Refusal> {
    let names = collections.unwrap_or_default();
    match body {
        MongoValue::String(name) if names.iter().any(|c| c == name) => Ok(()),
        MongoValue::String(_) => Err(Refusal::new("The stage $unionWith names a collection that is not listed.")),
        MongoValue::Document(_) => check_join("$unionWith", body, "coll", collections),
        _ => Err(Refusal::new("The stage $unionWith has no collection name.")),
    }
}

/// Checks a typed command: the name, the database and the collection against the lists, and the whole
/// command document against the refused keys.
///
/// # Errors
///
/// The command breaks one of the rules of the module comment.
pub fn check_command(command: &MongoCommand, databases: &[String], collections: &[String]) -> Result<(), Refusal> {
    let name = command.name();
    if REFUSED_COMMAND_NAMES.contains(&name) || !COMMAND_NAMES.contains(&name) {
        return Err(Refusal::new(format!("The command {name} is not in the list of 8b.5.")));
    }
    if command.is_query_read() || matches!(command, MongoCommand::GetMore(_) | MongoCommand::KillCursors(_)) {
        let db = command.database();
        if !databases.iter().any(|d| d == db) {
            return Err(Refusal::new("The database is not in the list databases."));
        }
        let collection = match command {
            MongoCommand::Aggregate(c) => Some(c.collection.as_str()),
            MongoCommand::Find(c) => Some(c.collection.as_str()),
            MongoCommand::GetMore(c) => Some(c.collection.as_str()),
            MongoCommand::KillCursors(c) => Some(c.collection.as_str()),
            _ => None,
        };
        if let Some(c) = collection
            && !collections.iter().any(|n| n == c)
        {
            return Err(Refusal::new("The collection is not in the list collections."));
        }
    }
    if let MongoCommand::ListCollections(c) = command
        && !databases.iter().any(|d| d == &c.database)
    {
        return Err(Refusal::new("The database is not in the list databases."));
    }
    // The command with no document form (the login) has no key to check: the driver encodes it.
    if let Some(doc) = command.to_document() {
        if doc.first().map(|(k, _)| k.as_str()) != Some(name) {
            return Err(Refusal::new(format!("The first key of the command {name} is not its name.")));
        }
        check_value(&MongoValue::Document(doc), Some(collections))?;
    }
    Ok(())
}

/// The check of the driver module: it runs on a [`ReadRequest`] before the encode.
///
/// # Errors
///
/// A `MongoError` of the class `Guard` when [`check_command`] refuses the command of the request.
pub fn check_read_request(
    request: &ReadRequest,
    databases: &[String],
    collections: &[String],
) -> Result<(), MongoError> {
    check_command(&request.to_command(), databases, collections)
        .map_err(|r| MongoError::new(MongoErrorClass::Guard, r.reason()))
}

#[cfg(test)]
mod tests {
    use hune_core::mongo::ReadPreference;

    use super::*;
    use crate::mongo::iface::ReadOp;

    fn v(text: &str) -> MongoValue {
        MongoValue::from_extended_json(text).unwrap()
    }

    fn lists() -> (Vec<String>, Vec<String>) {
        (vec!["analytics".to_owned()], vec!["c".to_owned(), "d".to_owned()])
    }

    fn pipeline_request(stages: &[&str]) -> ReadRequest {
        ReadRequest {
            database: "analytics".to_owned(),
            op: ReadOp::Aggregate { collection: "c".to_owned(), pipeline: stages.iter().map(|s| v(s)).collect() },
            batch_size: 101,
            max_time_ms: 60_000,
            read_preference: ReadPreference::Primary,
            comment: v(r#"{"app":"hune","kind":"panel"}"#),
        }
    }

    /// A stage with one key and a text value. Extended JSON text would read `{"$code": ...}` as the type
    /// JavaScript code, so the tests build the document.
    fn stage(key: &str) -> MongoValue {
        MongoValue::Document(vec![(key.to_owned(), MongoValue::String("x".to_owned()))])
    }

    fn request_with(stages: Vec<MongoValue>) -> ReadRequest {
        ReadRequest { op: ReadOp::Aggregate { collection: "c".to_owned(), pipeline: stages }, ..pipeline_request(&[]) }
    }

    fn run(request: &ReadRequest) -> Result<(), MongoError> {
        let (dbs, colls) = lists();
        check_read_request(request, &dbs, &colls)
    }

    #[test]
    fn a_plain_read_passes() {
        assert!(run(&pipeline_request(&[r#"{"$match":{"a":1}}"#, r#"{"$limit":101}"#])).is_ok());
    }

    #[test]
    fn ms07_each_refused_key_is_refused_at_the_first_level() {
        for key in REFUSED_KEYS {
            let e = run(&request_with(vec![stage(key)])).unwrap_err();
            assert_eq!(e.class, MongoErrorClass::Guard, "{key}");
            assert!(e.message.contains(key), "{}", e.message);
        }
    }

    #[test]
    fn ms07_each_refused_key_is_refused_at_the_end_of_a_pipeline_and_in_a_facet() {
        let doc = |members: Vec<(&str, MongoValue)>| {
            MongoValue::Document(members.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
        };
        let arr = MongoValue::Array;
        for key in REFUSED_KEYS {
            let at_end = vec![v(r#"{"$match":{}}"#), stage(key)];
            assert!(run(&request_with(at_end)).is_err(), "{key}");
            let facet = doc(vec![("$facet", doc(vec![("f", arr(vec![v(r#"{"$match":{}}"#), stage(key)]))]))]);
            assert!(run(&request_with(vec![facet])).is_err(), "{key}");
            let deep = doc(vec![(
                "$match",
                doc(vec![("$and", arr(vec![doc(vec![("a", doc(vec![("$or", arr(vec![stage(key)]))]))])]))]),
            )]);
            assert!(run(&request_with(vec![deep])).is_err(), "{key}");
        }
    }

    #[test]
    fn ms07_a_refused_name_in_a_string_value_is_not_a_refusal() {
        for key in REFUSED_KEYS {
            let stage = format!("{{\"$match\":{{\"name\":\"{key}\",\"list\":[\"{key}\"]}}}}");
            assert!(run(&pipeline_request(&[&stage])).is_ok(), "{key}");
        }
    }

    #[test]
    fn ms07_a_refused_key_in_the_comment_or_a_find_is_refused() {
        let mut r = pipeline_request(&[r#"{"$match":{}}"#]);
        r.comment = v(r#"{"app":"hune","x":{"$function":1}}"#);
        assert!(run(&r).is_err());
        // The type JavaScript code is refused as a value, and as a key.
        r.comment = v(r#"{"app":"hune","x":{"$code":"1"}}"#);
        assert!(run(&r).unwrap_err().message.contains("javascript"));
        for (filter, projection, sort) in [
            (r#"{"$where":"1"}"#, None, None),
            (r#"{}"#, Some(r#"{"a":{"$function":{}}}"#), None),
            (r#"{}"#, None, Some(r#"{"$accumulator":1}"#)),
            (r#"{"a":{"$code":"x"}}"#, None, None),
        ] {
            let r = ReadRequest {
                op: ReadOp::Find {
                    collection: "c".to_owned(),
                    filter: v(filter),
                    projection: projection.map(v),
                    sort: sort.map(v),
                    skip: 0,
                    limit: 5,
                },
                ..pipeline_request(&[])
            };
            assert!(run(&r).is_err(), "{filter}");
        }
        let ok = ReadRequest {
            op: ReadOp::Find {
                collection: "c".to_owned(),
                filter: v(r#"{"a":1}"#),
                projection: Some(v(r#"{"a":1}"#)),
                sort: Some(v(r#"{"a":-1}"#)),
                skip: 2,
                limit: 5,
            },
            ..pipeline_request(&[])
        };
        assert!(run(&ok).is_ok());
    }

    #[test]
    fn a_database_or_a_collection_outside_the_lists_is_refused() {
        let mut r = pipeline_request(&[r#"{"$match":{}}"#]);
        r.database = "other".to_owned();
        assert!(run(&r).unwrap_err().message.contains("database"));
        let mut r = pipeline_request(&[r#"{"$match":{}}"#]);
        r.op = ReadOp::Aggregate { collection: "secret".to_owned(), pipeline: vec![] };
        assert!(run(&r).unwrap_err().message.contains("collection"));
        // The comparison is exact and case-sensitive.
        r.op = ReadOp::Aggregate { collection: "C".to_owned(), pipeline: vec![] };
        assert!(run(&r).is_err());
    }

    #[test]
    fn joins_stay_inside_the_list_of_the_collections() {
        let ok_lookup = r#"{"$lookup":{"from":"d","localField":"a","foreignField":"b","as":"x"}}"#;
        assert!(run(&pipeline_request(&[ok_lookup])).is_ok());
        for bad in [
            r#"{"$lookup":{"from":"secret","localField":"a","foreignField":"b","as":"x"}}"#,
            r#"{"$lookup":{"from":{"db":"other","coll":"d"},"as":"x"}}"#,
            r#"{"$lookup":{"pipeline":[],"as":"x"}}"#,
            r#"{"$unionWith":"secret"}"#,
            r#"{"$unionWith":{"coll":"secret","pipeline":[]}}"#,
            r#"{"$unionWith":{"pipeline":[]}}"#,
            r#"{"$unionWith":5}"#,
            r#"{"$graphLookup":{"from":"d","startWith":"$a","connectFromField":"a","connectToField":"b","as":"x"}}"#,
        ] {
            assert!(run(&pipeline_request(&[bad])).is_err(), "{bad}");
        }
        assert!(run(&pipeline_request(&[r#"{"$unionWith":"d"}"#])).is_ok());
        assert!(run(&pipeline_request(&[r#"{"$unionWith":{"coll":"d","pipeline":[{"$match":{}}]}}"#])).is_ok());
        // A join inside a facet is checked too.
        let nested = r#"{"$facet":{"f":[{"$lookup":{"from":"secret","as":"x"}}]}}"#;
        assert!(run(&pipeline_request(&[nested])).is_err());
    }

    #[test]
    fn the_refusal_holds_no_value_of_the_query() {
        let e = run(&pipeline_request(&[r#"{"$match":{"token":"hunter2","$where":"hunter2"}}"#])).unwrap_err();
        assert!(!e.message.contains("hunter2"));
        let e = run(&pipeline_request(&[r#"{"$lookup":{"from":"hunter2","as":"x"}}"#])).unwrap_err();
        assert!(!e.message.contains("hunter2"));
    }

    #[test]
    fn a_deep_value_does_not_overflow_the_stack() {
        let mut value = v(r#"{"$where":1}"#);
        for _ in 0..50_000 {
            value = MongoValue::Array(vec![value]);
        }
        assert!(check_value(&value, None).is_err());
        // Drop the value with a loop: the derived drop of a deep value is recursive.
        let mut cur = value;
        while let MongoValue::Array(mut items) = cur {
            match items.pop() {
                Some(next) => cur = next,
                None => break,
            }
        }
    }

    #[test]
    fn typed_commands_of_the_table_pass_and_a_refused_command_name_cannot_be_made() {
        let (dbs, colls) = lists();
        for cmd in [
            MongoCommand::BuildInfo,
            MongoCommand::ConnectionStatus(hune_core::mongo::ConnectionStatusCommand),
            MongoCommand::ListCollections(hune_core::mongo::ListCollectionsCommand {
                database: "analytics".to_owned(),
            }),
            MongoCommand::Hello(hune_core::mongo::HelloCommand { hello_ok: true }),
        ] {
            assert!(check_command(&cmd, &dbs, &colls).is_ok(), "{}", cmd.name());
        }
        let list_other =
            MongoCommand::ListCollections(hune_core::mongo::ListCollectionsCommand { database: "other".to_owned() });
        assert!(check_command(&list_other, &dbs, &colls).is_err());
        for name in REFUSED_COMMAND_NAMES {
            assert!(!COMMAND_NAMES.contains(&name), "{name}");
        }
        // The cursor commands name a database and a collection of the lists.
        let cursor = crate::mongo::CursorHandle {
            id: 4,
            database: "analytics".to_owned(),
            collection: "c".to_owned(),
            read_preference: ReadPreference::Primary,
        };
        assert!(check_command(&cursor.kill_command(), &dbs, &colls).is_ok());
        assert!(check_command(&cursor.get_more_command(10, 1000, MongoValue::Null), &dbs, &colls).is_ok());
        let other = crate::mongo::CursorHandle { collection: "secret".to_owned(), ..cursor };
        assert!(check_command(&other.kill_command(), &dbs, &colls).is_err());
        assert!(check_command(&other.get_more_command(10, 1000, MongoValue::Null), &dbs, &colls).is_err());
    }
}
