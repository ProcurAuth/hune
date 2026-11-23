//! The narrow client interface of a MongoDB source (8b.16).
//!
//! The trait [`MongoClient`] is the only way from the logic of the source to a MongoDB server. It
//! has one method for each read of the table of 8b.5. It has no method that writes data and no
//! method that takes a command document from a caller. [`ReadOp`] is a closed enum, so a caller
//! cannot name a command. The logic of the source is generic over the trait. The driver module
//! (behind the cargo feature `mongo-driver`, 8b.2) implements it. The unit tests implement it with
//! a fake client that returns scripted batches, errors and delays.
//!
//! The probe of 8b.12 reads [`ServerInfo`] and [`AuthInfo`]: two plain structures. The role checks
//! are pure functions of them.

use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use hune_core::mongo::command::{
    AggregateCommand, FindCommand, GetMoreCommand, KillCursorsCommand, MongoCommand, ReadPreference,
};
use hune_core::mongo::{Document, MongoValue};

use crate::config::{MongoSourceConfig, MongoTlsMode, MongoTopology, SourceTls};

/// The options of a connection. The driver module converts them to the options of the driver and
/// sets nothing that 8b.4 does not name.
#[derive(Clone)]
pub struct ConnectOptions {
    /// The hosts (8b.4).
    pub topology: MongoTopology,
    /// The user name.
    pub user: String,
    /// The password. It never goes to a log.
    pub password: String,
    /// The database that holds the user.
    pub auth_database: String,
    /// The mode of TLS.
    pub tls_mode: MongoTlsMode,
    /// The TLS settings.
    pub tls: Option<SourceTls>,
    /// The path of the CA file of `tls` (the driver reads its trust roots from a file only).
    pub tls_ca_file: Option<PathBuf>,
    /// The databases of the block. The wire guard of the driver module reads them (8b.5).
    pub databases: Vec<String>,
    /// The collections of the block. The wire guard of the driver module reads them (8b.5).
    pub collections: Vec<String>,
    /// The most connections of the pool: `max_concurrent` plus one for the probe (8b.9).
    pub max_pool_size: u32,
    /// The time for the connection, TLS, login and server selection.
    pub connect_timeout: Duration,
    /// The read preference of each command.
    pub read_preference: ReadPreference,
}

impl ConnectOptions {
    /// Takes the options from the settings of a block.
    pub fn from_config(config: &MongoSourceConfig) -> ConnectOptions {
        ConnectOptions {
            topology: config.topology.clone(),
            user: config.user.clone(),
            password: config.password.clone(),
            auth_database: config.auth_database.clone(),
            tls_mode: config.tls_mode,
            tls: config.tls.clone(),
            tls_ca_file: config.tls_ca_file.clone(),
            databases: config.databases.clone(),
            collections: config.collections.clone(),
            max_pool_size: config.max_concurrent.saturating_add(1),
            connect_timeout: config.connect_timeout,
            read_preference: config.read_preference,
        }
    }
}

impl fmt::Debug for ConnectOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectOptions")
            .field("topology", &self.topology)
            .field("user", &self.user)
            .field("password", &"(hidden)")
            .field("auth_database", &self.auth_database)
            .field("tls_mode", &self.tls_mode)
            .field("tls", &self.tls)
            .field("tls_ca_file", &self.tls_ca_file)
            .field("databases", &self.databases)
            .field("collections", &self.collections)
            .field("max_pool_size", &self.max_pool_size)
            .field("connect_timeout", &self.connect_timeout)
            .field("read_preference", &self.read_preference)
            .finish()
    }
}

/// The answer of `hello` and `buildInfo` (8b.12, checks M0 and M4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    /// The server version text, for example `7.0.14`.
    pub version: String,
    /// The highest wire version that the server speaks.
    pub max_wire_version: i32,
    /// The size limit of one BSON document (`maxBsonObjectSize` of `hello`). The check M0 needs
    /// at least 16,777,216.
    pub max_bson_object_size: i64,
    /// The replica set name, when the server is a member of a set.
    pub set_name: Option<String>,
    /// The members that the server announces (`hosts`, `passives`, `arbiters`), as `host:port`.
    pub hosts: Vec<String>,
    /// True when the server is a `mongos` router.
    pub is_mongos: bool,
}

/// One user of `connectionStatus` (`authInfo.authenticatedUsers`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUser {
    /// The user name.
    pub user: String,
    /// The database of the user.
    pub db: String,
}

/// One role of `connectionStatus` (`authInfo.authenticatedUserRoles`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRole {
    /// The role name.
    pub role: String,
    /// The database of the role.
    pub db: String,
}

/// The resource of a privilege (`authInfo.authenticatedUserPrivileges[].resource`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivilegeResource {
    /// The whole cluster.
    Cluster,
    /// A database or a collection. An empty name means all of them.
    Namespace {
        /// The database. Empty means any database.
        db: String,
        /// The collection. Empty means any collection of the database.
        collection: String,
    },
    /// Another resource form that the model does not read.
    Other,
}

/// One privilege of `connectionStatus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Privilege {
    /// What the privilege covers.
    pub resource: PrivilegeResource,
    /// The actions, for example `find`.
    pub actions: Vec<String>,
}

/// The answer of `connectionStatus` with `showPrivileges: true` (8b.12, checks M1 to M3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuthInfo {
    /// The users that the connection logged in.
    pub users: Vec<AuthUser>,
    /// The roles of these users.
    pub roles: Vec<AuthRole>,
    /// The privileges of these roles.
    pub privileges: Vec<Privilege>,
}

/// The class of a [`MongoError`]. The error map of 8b.8 reads it with the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MongoErrorClass {
    /// The server answered with an error code.
    Server,
    /// The link failed: connect, read, write or server selection.
    Network,
    /// The TLS handshake or the check of the certificate failed.
    Tls,
    /// The login failed.
    Auth,
    /// The deadline of the client passed.
    Timeout,
    /// A reply could not be decoded to the value model.
    Decode,
    /// The wire guard refused the request (8b.5). It is an internal error.
    Guard,
}

/// The `code_name` that a `Tls` error carries when the check of the certificate failed (8b.4, 8b.8).
/// The error map gives `source_misconfigured` for it. A `Tls` error with another `code_name`, or with
/// none, gives `source_unavailable`.
pub const CERTIFICATE_CODE_NAME: &str = "CertificateCheck";

/// The error of the client interface (8b.16).
///
/// The message is the first line of the server message. It can hold a value of the data (8b.15), so
/// the logic of the source never logs it.
#[derive(Clone, PartialEq, Eq)]
pub struct MongoError {
    /// The class.
    pub class: MongoErrorClass,
    /// The server error code, when the server gave one.
    pub code: Option<i32>,
    /// The server code name, for example `MaxTimeMSExpired`.
    pub code_name: Option<String>,
    /// The first line of the message.
    pub message: String,
}

impl MongoError {
    /// Makes an error with no server code. The message is cut to its first line.
    pub fn new(class: MongoErrorClass, message: &str) -> MongoError {
        MongoError { class, code: None, code_name: None, message: crate::error::first_line(message) }
    }

    /// Makes an error of the class `Server`. The message is cut to its first line.
    pub fn server(code: i32, code_name: &str, message: &str) -> MongoError {
        MongoError {
            class: MongoErrorClass::Server,
            code: Some(code),
            code_name: Some(code_name.to_owned()),
            message: crate::error::first_line(message),
        }
    }
}

impl fmt::Debug for MongoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The message can hold a value of the data: it stays out of the debug text.
        f.debug_struct("MongoError")
            .field("class", &self.class)
            .field("code", &self.code)
            .field("code_name", &self.code_name)
            .finish_non_exhaustive()
    }
}

/// One read that Hune can send. The enum is closed: no other command exists (8b.5).
#[derive(Debug, Clone, PartialEq)]
pub enum ReadOp {
    /// An `aggregate`. The pipeline holds the last stage `$limit` of `R + 1` rows.
    Aggregate {
        /// The collection.
        collection: String,
        /// The stages.
        pipeline: Vec<MongoValue>,
    },
    /// A `find`.
    Find {
        /// The collection.
        collection: String,
        /// The filter.
        filter: MongoValue,
        /// The projection.
        projection: Option<MongoValue>,
        /// The sort.
        sort: Option<MongoValue>,
        /// The number of documents to skip.
        skip: u64,
        /// The lower of the query limit and `R + 1`.
        limit: u64,
    },
}

/// One read request (8b.16). The driver module runs the wire guard on it before the encode.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadRequest {
    /// The database.
    pub database: String,
    /// The read.
    pub op: ReadOp,
    /// The first batch size: `min(1000, R + 1)`.
    pub batch_size: u32,
    /// The member `maxTimeMS`.
    pub max_time_ms: u64,
    /// The read preference.
    pub read_preference: ReadPreference,
    /// The attribution document (8b.11).
    pub comment: MongoValue,
}

impl ReadRequest {
    /// Gives the collection of the read.
    pub fn collection(&self) -> &str {
        match &self.op {
            ReadOp::Aggregate { collection, .. } | ReadOp::Find { collection, .. } => collection,
        }
    }

    /// Gives the typed command of the request: the only way to make an `aggregate` or a `find`.
    pub fn to_command(&self) -> MongoCommand {
        match &self.op {
            ReadOp::Aggregate { collection, pipeline } => MongoCommand::Aggregate(AggregateCommand {
                database: self.database.clone(),
                collection: collection.clone(),
                pipeline: pipeline.clone(),
                batch_size: self.batch_size,
                max_time_ms: self.max_time_ms,
                comment: self.comment.clone(),
                read_preference: self.read_preference,
            }),
            ReadOp::Find { collection, filter, projection, sort, skip, limit } => MongoCommand::Find(FindCommand {
                database: self.database.clone(),
                collection: collection.clone(),
                filter: filter.clone(),
                projection: projection.clone(),
                sort: sort.clone(),
                skip: *skip,
                limit: *limit,
                batch_size: self.batch_size,
                max_time_ms: self.max_time_ms,
                comment: self.comment.clone(),
                read_preference: self.read_preference,
            }),
        }
    }
}

/// An open cursor of a read (8b.7). It names the cursor for `getMore` and `killCursors`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorHandle {
    /// The id of the cursor. It is never 0: the server closes a cursor with the id 0.
    pub id: i64,
    /// The database of the cursor.
    pub database: String,
    /// The collection of the cursor.
    pub collection: String,
    /// The read preference of the read.
    pub read_preference: ReadPreference,
}

impl CursorHandle {
    /// Gives the typed `getMore` of the cursor.
    pub fn get_more_command(&self, batch_size: u32, max_time_ms: u64, comment: MongoValue) -> MongoCommand {
        MongoCommand::GetMore(GetMoreCommand {
            database: self.database.clone(),
            collection: self.collection.clone(),
            cursor_id: self.id,
            batch_size,
            max_time_ms,
            comment,
            read_preference: self.read_preference,
        })
    }

    /// Gives the typed `killCursors` of the cursor.
    pub fn kill_command(&self) -> MongoCommand {
        MongoCommand::KillCursors(KillCursorsCommand {
            database: self.database.clone(),
            collection: self.collection.clone(),
            cursor_ids: vec![self.id],
            read_preference: self.read_preference,
        })
    }
}

/// One batch of documents (8b.16).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Batch {
    /// The documents, each one as the ordered members of the value model.
    pub docs: Vec<Document>,
    /// The cursor, when the server has more documents.
    pub cursor: Option<CursorHandle>,
    /// The size of the reply on the wire, in bytes. The logic counts it for the caps of 8b.6.
    pub reply_bytes: u64,
}

/// The narrow client of a MongoDB source (8b.5). No method writes data.
pub trait MongoClient: Send + Sync {
    /// Connects, checks TLS and logs in with SCRAM-SHA-256 (8b.4).
    fn connect(options: ConnectOptions) -> impl Future<Output = Result<Self, MongoError>> + Send
    where
        Self: Sized;

    /// `hello` and `buildInfo` (8b.12, M0 and M4).
    fn server_info(&self) -> impl Future<Output = Result<ServerInfo, MongoError>> + Send;

    /// `connectionStatus` with `showPrivileges: true` (8b.12, M1 to M3).
    fn connection_status(&self) -> impl Future<Output = Result<AuthInfo, MongoError>> + Send;

    /// `listCollections`, names only (8b.12, A1 and A2).
    fn list_collections(&self, database: &str) -> impl Future<Output = Result<Vec<String>, MongoError>> + Send;

    /// `aggregate` or `find`, with `maxTimeMS`, after the wire guard (8b.5). Gives the first batch.
    fn read(&self, request: ReadRequest) -> impl Future<Output = Result<Batch, MongoError>> + Send;

    /// `getMore` for a cursor that `read` gave.
    fn get_more(&self, cursor: &mut CursorHandle) -> impl Future<Output = Result<Batch, MongoError>> + Send;

    /// `killCursors` for a cursor (8b.7). It never fails the query.
    fn kill_cursor(&self, cursor: CursorHandle) -> impl Future<Output = ()> + Send;

    /// Ends the session of a query (`endSessions`) and closes the link of the query.
    fn finish(&self, cursor: Option<CursorHandle>) -> impl Future<Output = ()> + Send;
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use hune_core::ids::SourceId;

    use super::*;

    /// A scripted client, to show that the trait is enough for the logic and for the tests.
    struct Fake {
        batches: Mutex<VecDeque<Result<Batch, MongoError>>>,
        killed: Mutex<Vec<i64>>,
        seen: Mutex<Vec<MongoCommand>>,
    }

    impl MongoClient for Fake {
        async fn connect(options: ConnectOptions) -> Result<Fake, MongoError> {
            if options.user.is_empty() {
                return Err(MongoError::new(MongoErrorClass::Auth, "no user"));
            }
            Ok(Fake { batches: Mutex::default(), killed: Mutex::default(), seen: Mutex::default() })
        }

        async fn server_info(&self) -> Result<ServerInfo, MongoError> {
            Ok(ServerInfo {
                version: "7.0.14".to_owned(),
                max_wire_version: 21,
                max_bson_object_size: 16_777_216,
                set_name: None,
                hosts: vec![],
                is_mongos: false,
            })
        }

        async fn connection_status(&self) -> Result<AuthInfo, MongoError> {
            Ok(AuthInfo::default())
        }

        async fn list_collections(&self, _database: &str) -> Result<Vec<String>, MongoError> {
            Ok(vec!["a".to_owned()])
        }

        async fn read(&self, request: ReadRequest) -> Result<Batch, MongoError> {
            self.seen.lock().unwrap().push(request.to_command());
            self.batches.lock().unwrap().pop_front().unwrap_or_else(|| Ok(Batch::default()))
        }

        async fn get_more(&self, cursor: &mut CursorHandle) -> Result<Batch, MongoError> {
            self.seen.lock().unwrap().push(cursor.get_more_command(10, 1000, MongoValue::Null));
            self.batches.lock().unwrap().pop_front().unwrap_or_else(|| Ok(Batch::default()))
        }

        async fn kill_cursor(&self, cursor: CursorHandle) {
            self.killed.lock().unwrap().push(cursor.id);
        }

        async fn finish(&self, _cursor: Option<CursorHandle>) {}
    }

    fn config() -> MongoSourceConfig {
        MongoSourceConfig {
            id: SourceId::parse("events").unwrap(),
            label: "Events".to_owned(),
            topology: MongoTopology::Direct { host: "h".to_owned(), port: 27017 },
            user: "u".to_owned(),
            password: "secret-password".to_owned(),
            auth_database: "admin".to_owned(),
            databases: vec!["analytics".to_owned()],
            collections: vec!["c".to_owned()],
            read_preference: ReadPreference::Primary,
            tls_mode: MongoTlsMode::Disable,
            tls: None,
            tls_ca_file: None,
            connect_timeout: Duration::from_secs(5),
            max_execution_time: Duration::from_secs(60),
            max_result_rows: 100,
            max_result_bytes: 1 << 20,
            max_query_size: 1 << 20,
            max_concurrent: 4,
            queue_wait: Duration::from_secs(1),
            max_concurrent_per_subject: None,
        }
    }

    fn request() -> ReadRequest {
        ReadRequest {
            database: "analytics".to_owned(),
            op: ReadOp::Aggregate {
                collection: "c".to_owned(),
                pipeline: vec![MongoValue::from_extended_json(r#"{"$limit":101}"#).unwrap()],
            },
            batch_size: 101,
            max_time_ms: 60_000,
            read_preference: ReadPreference::Primary,
            comment: MongoValue::Null,
        }
    }

    #[tokio::test]
    async fn a_fake_client_gives_scripted_batches_and_commands_are_typed() {
        let fake = Fake::connect(ConnectOptions::from_config(&config())).await.unwrap();
        let cursor = CursorHandle {
            id: 9,
            database: "analytics".to_owned(),
            collection: "c".to_owned(),
            read_preference: ReadPreference::Primary,
        };
        let doc =
            MongoValue::document_from_extended_json(r#"{"_id":{"$oid":"507f1f77bcf86cd799439011"},"n":1}"#).unwrap();
        fake.batches.lock().unwrap().extend([
            Ok(Batch { docs: vec![doc.clone()], cursor: Some(cursor.clone()), reply_bytes: 64 }),
            Err(MongoError::server(50, "MaxTimeMSExpired", "operation exceeded time limit\nsecond line")),
        ]);
        let first = fake.read(request()).await.unwrap();
        assert_eq!(first.docs, vec![doc]);
        let mut c = first.cursor.unwrap();
        let e = fake.get_more(&mut c).await.unwrap_err();
        assert_eq!((e.class, e.code), (MongoErrorClass::Server, Some(50)));
        assert_eq!(e.message, "operation exceeded time limit");
        fake.kill_cursor(c).await;
        assert_eq!(*fake.killed.lock().unwrap(), vec![9]);
        let names: Vec<&str> = fake.seen.lock().unwrap().iter().map(MongoCommand::name).collect();
        assert_eq!(names, ["aggregate", "getMore"]);
        assert_eq!(fake.server_info().await.unwrap().version, "7.0.14");
        assert_eq!(fake.connection_status().await.unwrap(), AuthInfo::default());
        assert_eq!(fake.list_collections("analytics").await.unwrap(), ["a"]);
        fake.finish(None).await;
    }

    #[tokio::test]
    async fn connect_error_has_a_class() {
        let mut c = config();
        c.user.clear();
        let e = Fake::connect(ConnectOptions::from_config(&c)).await.err().unwrap();
        assert_eq!(e.class, MongoErrorClass::Auth);
    }

    #[test]
    fn requests_become_typed_commands() {
        let r = request();
        assert_eq!(r.collection(), "c");
        assert_eq!(r.to_command().name(), "aggregate");
        let find = ReadRequest {
            op: ReadOp::Find {
                collection: "c".to_owned(),
                filter: MongoValue::Document(vec![]),
                projection: None,
                sort: None,
                skip: 0,
                limit: 5,
            },
            ..r
        };
        assert_eq!(find.to_command().name(), "find");
        assert!(find.to_command().is_query_read());
    }

    #[test]
    fn cursor_commands_name_the_cursor() {
        let c = CursorHandle {
            id: 3,
            database: "d".to_owned(),
            collection: "c".to_owned(),
            read_preference: ReadPreference::Nearest,
        };
        let kill = c.kill_command().to_document().unwrap();
        assert_eq!(kill[0].0, "killCursors");
        assert_eq!(MongoValue::Document(kill).get("cursors"), Some(&MongoValue::Array(vec![MongoValue::Int64(3)])));
        assert_eq!(c.get_more_command(5, 10, MongoValue::Null).name(), "getMore");
    }

    #[test]
    fn debug_hides_the_password_and_the_server_message() {
        let o = ConnectOptions::from_config(&config());
        let t = format!("{o:?}");
        assert!(t.contains("(hidden)") && !t.contains("secret-password"));
        let t = format!("{:?}", MongoError::server(13, "Unauthorized", "value 12345 is secret"));
        assert!(!t.contains("12345"));
    }
}
