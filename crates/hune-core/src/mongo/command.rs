// SPDX-License-Identifier: MIT OR Apache-2.0
//! The commands that a MongoDB source sends (8b.5, 8b.16).
//!
//! The enum [`MongoCommand`] is closed. It has one variant for each row of the table of 8b.5 and
//! each variant holds a typed struct. No variant holds a free document, so no code path can build
//! a write, a `distinct`, a `count` or a `serverStatus`. The driver sends `hello`, `saslStart`,
//! `saslContinue`, `getMore` and `endSessions` itself ([`CommandSender::Driver`]). Hune builds the
//! other commands from the query ([`CommandSender::Hune`]).
//!
//! [`MongoCommand::to_document`] gives the command as a document with the members in the order of
//! 8b.5. The wire guard and the test proxy (MS09) read this form. The driver module encodes it.

use crate::mongo::value::{Document, MongoValue};

/// The read preference of a command (8b.4, 13.12b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadPreference {
    /// `primary`.
    Primary,
    /// `primaryPreferred`.
    PrimaryPreferred,
    /// `secondary`.
    Secondary,
    /// `secondaryPreferred`.
    SecondaryPreferred,
    /// `nearest`.
    Nearest,
}

impl ReadPreference {
    /// Gives the text of the configuration and of the member `mode`.
    pub fn as_str(self) -> &'static str {
        match self {
            ReadPreference::Primary => "primary",
            ReadPreference::PrimaryPreferred => "primaryPreferred",
            ReadPreference::Secondary => "secondary",
            ReadPreference::SecondaryPreferred => "secondaryPreferred",
            ReadPreference::Nearest => "nearest",
        }
    }

    /// Reads the text of the configuration. Another text gives `None`.
    pub fn parse(text: &str) -> Option<ReadPreference> {
        Some(match text {
            "primary" => ReadPreference::Primary,
            "primaryPreferred" => ReadPreference::PrimaryPreferred,
            "secondary" => ReadPreference::Secondary,
            "secondaryPreferred" => ReadPreference::SecondaryPreferred,
            "nearest" => ReadPreference::Nearest,
            _ => return None,
        })
    }
}

/// Who builds a command (8b.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSender {
    /// The driver sends it for its own work.
    Driver,
    /// Hune builds it.
    Hune,
}

/// The only login mechanism (8b.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaslMechanism {
    /// `SCRAM-SHA-256`.
    ScramSha256,
}

impl SaslMechanism {
    /// Gives the name on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            SaslMechanism::ScramSha256 => "SCRAM-SHA-256",
        }
    }
}

/// `hello`: the handshake and the monitor of a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelloCommand {
    /// The member `helloOk`.
    pub hello_ok: bool,
}

/// `saslStart` with the mechanism SCRAM-SHA-256.
#[derive(Clone, PartialEq, Eq)]
pub struct SaslStartCommand {
    /// The mechanism.
    pub mechanism: SaslMechanism,
    /// The first client message. It is binary and it is not a secret in itself.
    pub payload: Vec<u8>,
    /// The database of the user (`auth_database`).
    pub auth_database: String,
}

/// `saslContinue`.
#[derive(Clone, PartialEq, Eq)]
pub struct SaslContinueCommand {
    /// The id of the conversation that the server gave.
    pub conversation_id: i32,
    /// The client message.
    pub payload: Vec<u8>,
    /// The database of the user.
    pub auth_database: String,
}

impl std::fmt::Debug for SaslStartCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaslStartCommand")
            .field("mechanism", &self.mechanism)
            .field("payload", &"(hidden)")
            .field("auth_database", &self.auth_database)
            .finish()
    }
}

impl std::fmt::Debug for SaslContinueCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaslContinueCommand")
            .field("conversation_id", &self.conversation_id)
            .field("payload", &"(hidden)")
            .field("auth_database", &self.auth_database)
            .finish()
    }
}

/// `connectionStatus` with `showPrivileges: true`, on the database `admin` (8b.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionStatusCommand;

/// `listCollections` with `nameOnly: true` and `authorizedCollections: true` (8b.12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListCollectionsCommand {
    /// The database.
    pub database: String,
}

/// `aggregate` (8b.5). The pipeline holds the last stage `$limit` of `R + 1` rows already.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateCommand {
    /// The database (`$db`).
    pub database: String,
    /// The collection.
    pub collection: String,
    /// The stages, the trailing `$limit` included.
    pub pipeline: Vec<MongoValue>,
    /// The member `cursor.batchSize`.
    pub batch_size: u32,
    /// The member `maxTimeMS`.
    pub max_time_ms: u64,
    /// The attribution document (8b.11).
    pub comment: MongoValue,
    /// The member `$readPreference`.
    pub read_preference: ReadPreference,
}

/// `find` (8b.5).
#[derive(Debug, Clone, PartialEq)]
pub struct FindCommand {
    /// The database (`$db`).
    pub database: String,
    /// The collection.
    pub collection: String,
    /// The filter.
    pub filter: MongoValue,
    /// The projection.
    pub projection: Option<MongoValue>,
    /// The sort.
    pub sort: Option<MongoValue>,
    /// The member `skip`. The command has no member when the value is 0.
    pub skip: u64,
    /// The member `limit`: the lower of the query limit and `R + 1`.
    pub limit: u64,
    /// The member `batchSize`.
    pub batch_size: u32,
    /// The member `maxTimeMS`.
    pub max_time_ms: u64,
    /// The attribution document (8b.11).
    pub comment: MongoValue,
    /// The member `$readPreference`.
    pub read_preference: ReadPreference,
}

/// `getMore` for a cursor that a read gave. The driver sends it for Hune.
#[derive(Debug, Clone, PartialEq)]
pub struct GetMoreCommand {
    /// The database (`$db`).
    pub database: String,
    /// The collection of the cursor.
    pub collection: String,
    /// The id of the cursor.
    pub cursor_id: i64,
    /// The member `batchSize`.
    pub batch_size: u32,
    /// The member `maxTimeMS`.
    pub max_time_ms: u64,
    /// The attribution document (8b.11).
    pub comment: MongoValue,
    /// The member `$readPreference`.
    pub read_preference: ReadPreference,
}

/// `killCursors` (8b.7).
#[derive(Debug, Clone, PartialEq)]
pub struct KillCursorsCommand {
    /// The database (`$db`).
    pub database: String,
    /// The collection of the cursors.
    pub collection: String,
    /// The ids of the cursors.
    pub cursor_ids: Vec<i64>,
    /// The member `$readPreference`.
    pub read_preference: ReadPreference,
}

/// `endSessions`, on the database `admin`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndSessionsCommand {
    /// The ids of the sessions: UUIDs.
    pub session_ids: Vec<[u8; 16]>,
}

/// One command of the closed table of 8b.5.
#[derive(Debug, Clone, PartialEq)]
pub enum MongoCommand {
    /// `hello`.
    Hello(HelloCommand),
    /// `saslStart`.
    SaslStart(SaslStartCommand),
    /// `saslContinue`.
    SaslContinue(SaslContinueCommand),
    /// `buildInfo`, on the database `admin`.
    BuildInfo,
    /// `connectionStatus`.
    ConnectionStatus(ConnectionStatusCommand),
    /// `listCollections`.
    ListCollections(ListCollectionsCommand),
    /// `aggregate`.
    Aggregate(AggregateCommand),
    /// `find`.
    Find(FindCommand),
    /// `getMore`.
    GetMore(GetMoreCommand),
    /// `killCursors`.
    KillCursors(KillCursorsCommand),
    /// `endSessions`.
    EndSessions(EndSessionsCommand),
}

/// The names of the commands of the table of 8b.5, in the order of the table. `isMaster` is the
/// legacy form of `hello`, and the driver sends it only for a server that does not know `hello`.
pub const COMMAND_NAMES: [&str; 11] = [
    "hello",
    "saslStart",
    "saslContinue",
    "buildInfo",
    "connectionStatus",
    "listCollections",
    "aggregate",
    "find",
    "getMore",
    "killCursors",
    "endSessions",
];

/// The command names that Hune never sends (8b.5). A policy check and the wire guard use the list.
pub const REFUSED_COMMAND_NAMES: [&str; 18] = [
    "insert",
    "update",
    "delete",
    "findAndModify",
    "create",
    "drop",
    "createIndexes",
    "createUser",
    "eval",
    "mapReduce",
    "distinct",
    "count",
    "explain",
    "currentOp",
    "killOp",
    "serverStatus",
    "getParameter",
    "dropDatabase",
];

fn s(text: &str) -> MongoValue {
    MongoValue::String(text.to_owned())
}

fn i64v(n: u64) -> MongoValue {
    MongoValue::Int64(i64::try_from(n).unwrap_or(i64::MAX))
}

fn i32v(n: u32) -> MongoValue {
    i32::try_from(n).map_or(MongoValue::Int64(i64::from(n)), MongoValue::Int32)
}

fn read_concern() -> (String, MongoValue) {
    ("readConcern".to_owned(), MongoValue::Document(vec![("level".to_owned(), s("local"))]))
}

fn read_pref(p: ReadPreference) -> (String, MongoValue) {
    ("$readPreference".to_owned(), MongoValue::Document(vec![("mode".to_owned(), s(p.as_str()))]))
}

fn member(key: &str, v: MongoValue) -> (String, MongoValue) {
    (key.to_owned(), v)
}

impl MongoCommand {
    /// Gives the name of the command on the wire. It is the first key of the command document.
    pub fn name(&self) -> &'static str {
        match self {
            MongoCommand::Hello(_) => "hello",
            MongoCommand::SaslStart(_) => "saslStart",
            MongoCommand::SaslContinue(_) => "saslContinue",
            MongoCommand::BuildInfo => "buildInfo",
            MongoCommand::ConnectionStatus(_) => "connectionStatus",
            MongoCommand::ListCollections(_) => "listCollections",
            MongoCommand::Aggregate(_) => "aggregate",
            MongoCommand::Find(_) => "find",
            MongoCommand::GetMore(_) => "getMore",
            MongoCommand::KillCursors(_) => "killCursors",
            MongoCommand::EndSessions(_) => "endSessions",
        }
    }

    /// Tells who builds the command (8b.5).
    pub fn sender(&self) -> CommandSender {
        match self {
            MongoCommand::Hello(_)
            | MongoCommand::SaslStart(_)
            | MongoCommand::SaslContinue(_)
            | MongoCommand::GetMore(_)
            | MongoCommand::EndSessions(_) => CommandSender::Driver,
            MongoCommand::BuildInfo
            | MongoCommand::ConnectionStatus(_)
            | MongoCommand::ListCollections(_)
            | MongoCommand::Aggregate(_)
            | MongoCommand::Find(_)
            | MongoCommand::KillCursors(_) => CommandSender::Hune,
        }
    }

    /// Tells if the command reads a collection of a query: `aggregate` or `find`. The wire guard
    /// checks the database and the collection of such a command against the lists of the block.
    pub fn is_query_read(&self) -> bool {
        matches!(self, MongoCommand::Aggregate(_) | MongoCommand::Find(_))
    }

    /// Gives the database of the command (`$db`).
    pub fn database(&self) -> &str {
        match self {
            MongoCommand::Aggregate(c) => &c.database,
            MongoCommand::Find(c) => &c.database,
            MongoCommand::GetMore(c) => &c.database,
            MongoCommand::KillCursors(c) => &c.database,
            MongoCommand::ListCollections(c) => &c.database,
            MongoCommand::SaslStart(c) => &c.auth_database,
            MongoCommand::SaslContinue(c) => &c.auth_database,
            MongoCommand::Hello(_)
            | MongoCommand::BuildInfo
            | MongoCommand::ConnectionStatus(_)
            | MongoCommand::EndSessions(_) => "admin",
        }
    }

    /// Gives the command as a document, with the members in the order of 8b.5 (`aggregate`, then
    /// `pipeline`, and so on). The read concern is always `local`, and `allowDiskUse` is always
    /// `false`. No write concern exists. The session and cluster time members are the work of
    /// the driver.
    ///
    /// `saslStart` and `saslContinue` give `None`: their payload is binary, and the model has no
    /// binary value. The driver encodes them.
    pub fn to_document(&self) -> Option<Document> {
        let db = |d: &str| member("$db", s(d));
        Some(match self {
            MongoCommand::Hello(c) => {
                vec![
                    member("hello", MongoValue::Int32(1)),
                    member("helloOk", MongoValue::Bool(c.hello_ok)),
                    db("admin"),
                ]
            }
            MongoCommand::SaslStart(_) | MongoCommand::SaslContinue(_) => return None,
            MongoCommand::BuildInfo => vec![member("buildInfo", MongoValue::Int32(1)), db("admin")],
            MongoCommand::ConnectionStatus(_) => {
                vec![
                    member("connectionStatus", MongoValue::Int32(1)),
                    member("showPrivileges", MongoValue::Bool(true)),
                    db("admin"),
                ]
            }
            MongoCommand::ListCollections(c) => vec![
                member("listCollections", MongoValue::Int32(1)),
                member("nameOnly", MongoValue::Bool(true)),
                member("authorizedCollections", MongoValue::Bool(true)),
                db(&c.database),
            ],
            MongoCommand::Aggregate(c) => vec![
                member("aggregate", s(&c.collection)),
                member("pipeline", MongoValue::Array(c.pipeline.clone())),
                member("cursor", MongoValue::Document(vec![member("batchSize", i32v(c.batch_size))])),
                member("allowDiskUse", MongoValue::Bool(false)),
                member("maxTimeMS", i64v(c.max_time_ms)),
                member("comment", c.comment.clone()),
                read_concern(),
                db(&c.database),
                read_pref(c.read_preference),
            ],
            MongoCommand::Find(c) => {
                let mut d = vec![member("find", s(&c.collection)), member("filter", c.filter.clone())];
                if let Some(p) = &c.projection {
                    d.push(member("projection", p.clone()));
                }
                if let Some(x) = &c.sort {
                    d.push(member("sort", x.clone()));
                }
                if c.skip > 0 {
                    d.push(member("skip", i64v(c.skip)));
                }
                d.extend([
                    member("limit", i64v(c.limit)),
                    member("batchSize", i32v(c.batch_size)),
                    member("allowDiskUse", MongoValue::Bool(false)),
                    member("maxTimeMS", i64v(c.max_time_ms)),
                    member("comment", c.comment.clone()),
                    read_concern(),
                    db(&c.database),
                    read_pref(c.read_preference),
                ]);
                d
            }
            MongoCommand::GetMore(c) => vec![
                member("getMore", MongoValue::Int64(c.cursor_id)),
                member("collection", s(&c.collection)),
                member("batchSize", i32v(c.batch_size)),
                member("maxTimeMS", i64v(c.max_time_ms)),
                member("comment", c.comment.clone()),
                db(&c.database),
                read_pref(c.read_preference),
            ],
            MongoCommand::KillCursors(c) => vec![
                member("killCursors", s(&c.collection)),
                member("cursors", MongoValue::Array(c.cursor_ids.iter().map(|&i| MongoValue::Int64(i)).collect())),
                db(&c.database),
                read_pref(c.read_preference),
            ],
            MongoCommand::EndSessions(c) => vec![
                member(
                    "endSessions",
                    MongoValue::Array(
                        c.session_ids
                            .iter()
                            .map(|&id| MongoValue::Document(vec![member("id", MongoValue::Uuid(id))]))
                            .collect(),
                    ),
                ),
                db("admin"),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment() -> MongoValue {
        MongoValue::document_from_extended_json(r#"{"v":1}"#).map(MongoValue::Document).unwrap()
    }

    fn aggregate() -> MongoCommand {
        MongoCommand::Aggregate(AggregateCommand {
            database: "analytics".to_owned(),
            collection: "node_load".to_owned(),
            pipeline: vec![MongoValue::from_extended_json(r#"{"$limit":11}"#).unwrap()],
            batch_size: 11,
            max_time_ms: 60_000,
            comment: comment(),
            read_preference: ReadPreference::SecondaryPreferred,
        })
    }

    fn keys(d: &Document) -> Vec<&str> {
        d.iter().map(|(k, _)| k.as_str()).collect()
    }

    #[test]
    fn aggregate_has_the_members_of_8b5_in_order() {
        let d = aggregate().to_document().unwrap();
        assert_eq!(
            keys(&d),
            [
                "aggregate",
                "pipeline",
                "cursor",
                "allowDiskUse",
                "maxTimeMS",
                "comment",
                "readConcern",
                "$db",
                "$readPreference"
            ]
        );
        let v = MongoValue::Document(d);
        assert_eq!(v.get("allowDiskUse"), Some(&MongoValue::Bool(false)));
        assert_eq!(v.get("readConcern").and_then(|r| r.get("level")), Some(&MongoValue::String("local".to_owned())));
        assert_eq!(v.get("cursor").and_then(|r| r.get("batchSize")), Some(&MongoValue::Int32(11)));
        assert_eq!(v.get("$db"), Some(&MongoValue::String("analytics".to_owned())));
        assert_eq!(
            v.get("$readPreference").and_then(|r| r.get("mode")),
            Some(&MongoValue::String("secondaryPreferred".to_owned()))
        );
    }

    #[test]
    fn find_has_the_members_of_8b5_and_no_skip_at_zero() {
        let mut c = FindCommand {
            database: "d".to_owned(),
            collection: "c".to_owned(),
            filter: MongoValue::Document(vec![]),
            projection: None,
            sort: None,
            skip: 0,
            limit: 10,
            batch_size: 10,
            max_time_ms: 1000,
            comment: comment(),
            read_preference: ReadPreference::Primary,
        };
        let d = MongoCommand::Find(c.clone()).to_document().unwrap();
        assert_eq!(
            keys(&d),
            [
                "find",
                "filter",
                "limit",
                "batchSize",
                "allowDiskUse",
                "maxTimeMS",
                "comment",
                "readConcern",
                "$db",
                "$readPreference"
            ]
        );
        c.skip = 3;
        c.sort = Some(MongoValue::Document(vec![("a".to_owned(), MongoValue::Int32(-1))]));
        c.projection = Some(MongoValue::Document(vec![("a".to_owned(), MongoValue::Int32(1))]));
        let d = MongoCommand::Find(c).to_document().unwrap();
        assert_eq!(&keys(&d)[..6], ["find", "filter", "projection", "sort", "skip", "limit"]);
    }

    #[test]
    fn names_and_senders_follow_the_table_of_8b5() {
        let cmds = [
            (MongoCommand::Hello(HelloCommand { hello_ok: true }), "hello", CommandSender::Driver),
            (
                MongoCommand::SaslStart(SaslStartCommand {
                    mechanism: SaslMechanism::ScramSha256,
                    payload: vec![1],
                    auth_database: "admin".to_owned(),
                }),
                "saslStart",
                CommandSender::Driver,
            ),
            (
                MongoCommand::SaslContinue(SaslContinueCommand {
                    conversation_id: 1,
                    payload: vec![],
                    auth_database: "admin".to_owned(),
                }),
                "saslContinue",
                CommandSender::Driver,
            ),
            (MongoCommand::BuildInfo, "buildInfo", CommandSender::Hune),
            (MongoCommand::ConnectionStatus(ConnectionStatusCommand), "connectionStatus", CommandSender::Hune),
            (
                MongoCommand::ListCollections(ListCollectionsCommand { database: "d".to_owned() }),
                "listCollections",
                CommandSender::Hune,
            ),
            (aggregate(), "aggregate", CommandSender::Hune),
            (
                MongoCommand::GetMore(GetMoreCommand {
                    database: "d".to_owned(),
                    collection: "c".to_owned(),
                    cursor_id: 7,
                    batch_size: 1000,
                    max_time_ms: 10,
                    comment: MongoValue::Null,
                    read_preference: ReadPreference::Nearest,
                }),
                "getMore",
                CommandSender::Driver,
            ),
            (
                MongoCommand::KillCursors(KillCursorsCommand {
                    database: "d".to_owned(),
                    collection: "c".to_owned(),
                    cursor_ids: vec![7],
                    read_preference: ReadPreference::Nearest,
                }),
                "killCursors",
                CommandSender::Hune,
            ),
            (
                MongoCommand::EndSessions(EndSessionsCommand { session_ids: vec![[1; 16]] }),
                "endSessions",
                CommandSender::Driver,
            ),
        ];
        for (c, name, sender) in &cmds {
            assert_eq!(c.name(), *name);
            assert_eq!(c.sender(), *sender, "{name}");
            assert!(COMMAND_NAMES.contains(name));
            if let Some(d) = c.to_document() {
                assert_eq!(d[0].0, *name, "the first key is the command name");
                assert_eq!(MongoValue::Document(d).get("$db"), Some(&MongoValue::String(c.database().to_owned())));
            } else {
                assert!(matches!(c, MongoCommand::SaslStart(_) | MongoCommand::SaslContinue(_)));
            }
        }
        // `find` is the last name of the table that the loop does not cover.
        assert!(COMMAND_NAMES.contains(&"find"));
        assert_eq!(COMMAND_NAMES.len(), 11);
    }

    #[test]
    fn the_table_holds_no_refused_name() {
        for n in REFUSED_COMMAND_NAMES {
            assert!(!COMMAND_NAMES.contains(&n), "{n}");
        }
        for n in ["insert", "update", "delete", "mapReduce", "distinct", "count", "explain", "serverStatus"] {
            assert!(REFUSED_COMMAND_NAMES.contains(&n), "{n}");
        }
    }

    #[test]
    fn probe_commands_read_the_database_admin() {
        let s = MongoCommand::ConnectionStatus(ConnectionStatusCommand).to_document().unwrap();
        let v = MongoValue::Document(s);
        assert_eq!(v.get("showPrivileges"), Some(&MongoValue::Bool(true)));
        assert_eq!(v.get("$db"), Some(&MongoValue::String("admin".to_owned())));
        let l = MongoCommand::ListCollections(ListCollectionsCommand { database: "analytics".to_owned() })
            .to_document()
            .unwrap();
        let v = MongoValue::Document(l);
        assert_eq!(v.get("nameOnly"), Some(&MongoValue::Bool(true)));
        assert_eq!(v.get("authorizedCollections"), Some(&MongoValue::Bool(true)));
    }

    #[test]
    fn sasl_payload_is_hidden_in_debug() {
        let c = SaslStartCommand {
            mechanism: SaslMechanism::ScramSha256,
            payload: vec![42, 43],
            auth_database: "admin".to_owned(),
        };
        let t = format!("{c:?}");
        assert!(t.contains("(hidden)") && !t.contains("42"));
    }

    #[test]
    fn only_the_two_reads_are_query_reads() {
        assert!(aggregate().is_query_read());
        assert!(!MongoCommand::BuildInfo.is_query_read());
    }

    #[test]
    fn read_preference_round_trips() {
        for p in [
            ReadPreference::Primary,
            ReadPreference::PrimaryPreferred,
            ReadPreference::Secondary,
            ReadPreference::SecondaryPreferred,
            ReadPreference::Nearest,
        ] {
            assert_eq!(ReadPreference::parse(p.as_str()), Some(p));
        }
        assert_eq!(ReadPreference::parse("Primary"), None);
        assert_eq!(SaslMechanism::ScramSha256.as_str(), "SCRAM-SHA-256");
    }
}
