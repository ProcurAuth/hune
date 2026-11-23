// SPDX-License-Identifier: MIT OR Apache-2.0
//! A fake MongoDB client that scripts the answers of a server (8b.16, 16.9b).
//!
//! [`FakeMongo`] implements [`MongoClient`]. The unit tests of the logic of the source use it in place
//! of the driver, so they need no server and no network. It does what the driver module does with
//! the real wire:
//!
//! - It runs the wire guard of 8b.5 on each read, and on each command that names a database or a
//!   collection, before it records the command. A refused command is never in the record.
//! - It records each command that it "sends", in the closed table of 8b.5.
//! - It answers from a script: a queue of replies for `read` and a queue for `get_more`, each reply
//!   with an optional delay. An empty queue gives an empty batch with no cursor.
//! - It answers `hello`, `buildInfo`, `connectionStatus` and `listCollections` from values that a test
//!   can change.
//!
//! The batches come from Extended JSON text ([`Reply::docs`]), as 8b.16 says. A clone of a
//! [`FakeMongo`] shares the script and the record. [`FakeMongo::connector`] gives a
//! [`MongoConnector`] whose clients all share them.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hune_core::mongo::{
    ConnectionStatusCommand, EndSessionsCommand, HelloCommand, ListCollectionsCommand, MongoCommand, MongoValue,
    ReadPreference,
};

use crate::config::{MongoSourceConfig, MongoTopology};
use crate::mongo::erased::MongoConnector;
use crate::mongo::guard;
use crate::mongo::iface::{
    AuthInfo, AuthRole, AuthUser, Batch, ConnectOptions, CursorHandle, MongoClient, MongoError, MongoErrorClass,
    Privilege, PrivilegeResource, ReadRequest, ServerInfo,
};
use crate::mongo::probe::READ_ACTIONS;

/// One scripted answer of `read` or `get_more`.
#[derive(Debug, Clone)]
pub struct Reply {
    /// The time that the fake waits before it answers.
    pub delay: Duration,
    /// The answer.
    pub result: Result<Batch, MongoError>,
}

impl Reply {
    /// A batch with no cursor, from Extended JSON documents, one text for each document.
    ///
    /// # Panics
    ///
    /// A text is not an Extended JSON document: a test fault.
    pub fn docs(docs: &[&str]) -> Reply {
        Reply {
            delay: Duration::ZERO,
            result: Ok(Batch {
                docs: parse_docs(docs),
                cursor: None,
                reply_bytes: docs.iter().map(|d| d.len() as u64).sum(),
            }),
        }
    }

    /// A batch with the cursor `id`. The cursor names the database `analytics` and the collection
    /// `c`: the fake corrects them to those of the read, when it answers a `read`.
    ///
    /// # Panics
    ///
    /// A text is not an Extended JSON document: a test fault.
    pub fn docs_with_cursor(docs: &[&str], id: i64) -> Reply {
        let mut reply = Reply::docs(docs);
        if let Ok(batch) = &mut reply.result {
            batch.cursor = Some(CursorHandle {
                id,
                database: String::new(),
                collection: String::new(),
                read_preference: ReadPreference::Primary,
            });
        }
        reply
    }

    /// An error.
    pub fn error(error: MongoError) -> Reply {
        Reply { delay: Duration::ZERO, result: Err(error) }
    }

    /// The same reply, after `delay`.
    #[must_use]
    pub fn after(mut self, delay: Duration) -> Reply {
        self.delay = delay;
        self
    }

    /// The same reply, with the size on the wire that the batch reports.
    #[must_use]
    pub fn with_reply_bytes(mut self, bytes: u64) -> Reply {
        if let Ok(batch) = &mut self.result {
            batch.reply_bytes = bytes;
        }
        self
    }
}

fn parse_docs(docs: &[&str]) -> Vec<Vec<(String, MongoValue)>> {
    docs.iter()
        .map(|d| MongoValue::document_from_extended_json(d).expect("the test gives valid Extended JSON documents"))
        .collect()
}

/// The state that the clones of one fake share.
struct State {
    connect_errors: VecDeque<MongoError>,
    connect_always: Option<MongoError>,
    connects: usize,
    last_options: Option<ConnectOptions>,
    info: Result<ServerInfo, MongoError>,
    info_delay: Duration,
    auth: Result<AuthInfo, MongoError>,
    collections: HashMap<String, Result<Vec<String>, MongoError>>,
    reads: VecDeque<Reply>,
    get_mores: VecDeque<Reply>,
    guard_lists: Option<(Vec<String>, Vec<String>)>,
    commands: Vec<MongoCommand>,
    requests: Vec<ReadRequest>,
    killed: Vec<i64>,
    finished: Vec<Option<i64>>,
}

/// The fake client. A clone shares the script and the record with the original.
#[derive(Clone)]
pub struct FakeMongo {
    state: Arc<Mutex<State>>,
}

impl std::fmt::Debug for FakeMongo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FakeMongo")
    }
}

impl Default for FakeMongo {
    fn default() -> FakeMongo {
        FakeMongo::new()
    }
}

/// The default answer of `hello` and `buildInfo`: a standalone MongoDB 7.0.14.
fn default_info() -> ServerInfo {
    ServerInfo {
        version: "7.0.14".to_owned(),
        max_wire_version: 21,
        max_bson_object_size: 16_777_216,
        set_name: None,
        hosts: Vec::new(),
        is_mongos: false,
    }
}

impl FakeMongo {
    /// Makes a fake with an empty script and a standalone server of the version 7.0.14. The account
    /// is empty, so a probe against it fails the check M1: use [`FakeMongo::for_config`].
    pub fn new() -> FakeMongo {
        FakeMongo {
            state: Arc::new(Mutex::new(State {
                connect_errors: VecDeque::new(),
                connect_always: None,
                connects: 0,
                last_options: None,
                info: Ok(default_info()),
                info_delay: Duration::ZERO,
                auth: Ok(AuthInfo::default()),
                collections: HashMap::new(),
                reads: VecDeque::new(),
                get_mores: VecDeque::new(),
                guard_lists: None,
                commands: Vec::new(),
                requests: Vec::new(),
                killed: Vec::new(),
                finished: Vec::new(),
            })),
        }
    }

    /// Makes a fake with the account of 8b.13 for a block: one user equal to the keys, the role `read`
    /// on each database, the privileges of that role, and each collection of the key in each database.
    /// For a replica set it announces the members of the block. The wire guard uses the lists of the
    /// block.
    pub fn for_config(config: &MongoSourceConfig) -> FakeMongo {
        let fake = FakeMongo::new();
        {
            let mut s = fake.lock();
            s.auth = Ok(AuthInfo {
                users: vec![AuthUser { user: config.user.clone(), db: config.auth_database.clone() }],
                roles: config.databases.iter().map(|db| AuthRole { role: "read".to_owned(), db: db.clone() }).collect(),
                privileges: config
                    .databases
                    .iter()
                    .map(|db| Privilege {
                        resource: PrivilegeResource::Namespace { db: db.clone(), collection: String::new() },
                        actions: READ_ACTIONS.iter().map(|a| (*a).to_owned()).collect(),
                    })
                    .collect(),
            });
            for db in &config.databases {
                s.collections.insert(db.clone(), Ok(config.collections.clone()));
            }
            if let MongoTopology::ReplicaSet { hosts, replica_set } = &config.topology {
                let mut info = default_info();
                info.set_name = Some(replica_set.clone());
                info.hosts = hosts.iter().map(|h| format!("{}:{}", h.host, h.port)).collect();
                s.info = Ok(info);
            }
            s.guard_lists = Some((config.databases.clone(), config.collections.clone()));
        }
        fake
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Gives a connector whose clients are clones of this fake. Each connect counts, and fails when
    /// a failure is scripted ([`FakeMongo::fail_next_connect`], [`FakeMongo::fail_every_connect`]).
    pub fn connector(&self) -> MongoConnector {
        let fake = self.clone();
        MongoConnector::from_fn(move |options| {
            let fake = fake.clone();
            async move {
                {
                    let mut s = fake.lock();
                    s.connects += 1;
                    s.last_options = Some(options);
                    if let Some(e) = s.connect_errors.pop_front() {
                        return Err(e);
                    }
                    if let Some(e) = &s.connect_always {
                        return Err(e.clone());
                    }
                }
                Ok(fake)
            }
        })
    }

    // ---- the script ----

    /// Makes the next connect fail with `error`.
    pub fn fail_next_connect(&self, error: MongoError) {
        self.lock().connect_errors.push_back(error);
    }

    /// Makes each connect fail with `error`, until `None` clears it.
    pub fn fail_every_connect(&self, error: Option<MongoError>) {
        self.lock().connect_always = error;
    }

    /// Sets the answer of `hello` and `buildInfo`.
    pub fn set_server_info(&self, info: ServerInfo) {
        self.lock().info = Ok(info);
    }

    /// Edits the answer of `hello` and `buildInfo`.
    pub fn edit_server_info(&self, edit: impl FnOnce(&mut ServerInfo)) {
        if let Ok(info) = &mut self.lock().info {
            edit(info);
        }
    }

    /// Makes `hello` and `buildInfo` wait before they answer, so two probes can overlap.
    pub fn delay_server_info(&self, delay: Duration) {
        self.lock().info_delay = delay;
    }

    /// Makes `hello` and `buildInfo` fail.
    pub fn fail_server_info(&self, error: MongoError) {
        self.lock().info = Err(error);
    }

    /// Sets the answer of `connectionStatus`.
    pub fn set_auth(&self, auth: AuthInfo) {
        self.lock().auth = Ok(auth);
    }

    /// Edits the answer of `connectionStatus`.
    pub fn edit_auth(&self, edit: impl FnOnce(&mut AuthInfo)) {
        if let Ok(auth) = &mut self.lock().auth {
            edit(auth);
        }
    }

    /// Makes `connectionStatus` fail.
    pub fn fail_auth(&self, error: MongoError) {
        self.lock().auth = Err(error);
    }

    /// Sets the collections that `listCollections` gives for a database.
    pub fn set_collections(&self, database: &str, names: &[&str]) {
        self.lock().collections.insert(database.to_owned(), Ok(names.iter().map(|n| (*n).to_owned()).collect()));
    }

    /// Makes `listCollections` fail for a database.
    pub fn fail_collections(&self, database: &str, error: MongoError) {
        self.lock().collections.insert(database.to_owned(), Err(error));
    }

    /// Adds a reply for the next `read`.
    pub fn push_read(&self, reply: Reply) {
        self.lock().reads.push_back(reply);
    }

    /// Adds a reply for the next `get_more`.
    pub fn push_get_more(&self, reply: Reply) {
        self.lock().get_mores.push_back(reply);
    }

    /// Sets the lists that the wire guard of the fake uses. A fake from [`FakeMongo::new`] has none,
    /// and the guard then checks the refused keys only.
    pub fn set_guard_lists(&self, databases: &[&str], collections: &[&str]) {
        self.lock().guard_lists = Some((
            databases.iter().map(|d| (*d).to_owned()).collect(),
            collections.iter().map(|c| (*c).to_owned()).collect(),
        ));
    }

    // ---- the record ----

    /// Gives each command that the fake "sent", in order. A command that the guard refused is not in it.
    pub fn commands(&self) -> Vec<MongoCommand> {
        self.lock().commands.clone()
    }

    /// Gives the names of the commands that the fake "sent", in order.
    pub fn command_names(&self) -> Vec<&'static str> {
        self.lock().commands.iter().map(MongoCommand::name).collect()
    }

    /// Gives each read request that passed the guard, in order.
    pub fn requests(&self) -> Vec<ReadRequest> {
        self.lock().requests.clone()
    }

    /// Gives the ids of the cursors that got a `killCursors`, in order.
    pub fn killed(&self) -> Vec<i64> {
        self.lock().killed.clone()
    }

    /// Gives one entry for each `finish`: the id of the cursor that it got, or `None`.
    pub fn finished(&self) -> Vec<Option<i64>> {
        self.lock().finished.clone()
    }

    /// Gives the number of connects.
    pub fn connects(&self) -> usize {
        self.lock().connects
    }

    /// Gives the options of the last connect.
    pub fn last_connect_options(&self) -> Option<ConnectOptions> {
        self.lock().last_options.clone()
    }

    /// Gives the number of replies that wait in the queues of `read` and `get_more`.
    pub fn pending_replies(&self) -> (usize, usize) {
        let s = self.lock();
        (s.reads.len(), s.get_mores.len())
    }

    // ---- the wire ----

    /// Runs the guard on a command and records it. A refusal is an error of the class `Guard`.
    fn send(&self, command: MongoCommand) -> Result<(), MongoError> {
        let mut s = self.lock();
        if let Some((databases, collections)) = &s.guard_lists {
            guard::check_command(&command, databases, collections)
                .map_err(|r| MongoError::new(MongoErrorClass::Guard, r.reason()))?;
        } else {
            if let Some(doc) = command.to_document() {
                guard::check_value(&MongoValue::Document(doc), None)
                    .map_err(|r| MongoError::new(MongoErrorClass::Guard, r.reason()))?;
            }
        }
        s.commands.push(command);
        Ok(())
    }

    async fn answer(&self, mut reply: Reply, request: Option<&ReadRequest>) -> Result<Batch, MongoError> {
        if !reply.delay.is_zero() {
            tokio::time::sleep(reply.delay).await;
        }
        if let (Ok(batch), Some(r)) = (&mut reply.result, request)
            && let Some(cursor) = &mut batch.cursor
        {
            cursor.database.clone_from(&r.database);
            cursor.collection = r.collection().to_owned();
            cursor.read_preference = r.read_preference;
        }
        reply.result
    }
}

impl MongoClient for FakeMongo {
    /// Makes a fake with an empty script. The tests use [`FakeMongo::connector`] instead, so that
    /// the clients share one script.
    async fn connect(options: ConnectOptions) -> Result<FakeMongo, MongoError> {
        let fake = FakeMongo::new();
        fake.lock().last_options = Some(options);
        fake.lock().connects = 1;
        Ok(fake)
    }

    async fn server_info(&self) -> Result<ServerInfo, MongoError> {
        self.send(MongoCommand::Hello(HelloCommand { hello_ok: true }))?;
        self.send(MongoCommand::BuildInfo)?;
        let delay = self.lock().info_delay;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        self.lock().info.clone()
    }

    async fn connection_status(&self) -> Result<AuthInfo, MongoError> {
        self.send(MongoCommand::ConnectionStatus(ConnectionStatusCommand))?;
        self.lock().auth.clone()
    }

    async fn list_collections(&self, database: &str) -> Result<Vec<String>, MongoError> {
        self.send(MongoCommand::ListCollections(ListCollectionsCommand { database: database.to_owned() }))?;
        self.lock().collections.get(database).cloned().unwrap_or_else(|| Ok(Vec::new()))
    }

    async fn read(&self, request: ReadRequest) -> Result<Batch, MongoError> {
        self.send(request.to_command())?;
        let reply = {
            let mut s = self.lock();
            s.requests.push(request.clone());
            s.reads.pop_front()
        };
        match reply {
            Some(reply) => self.answer(reply, Some(&request)).await,
            None => Ok(Batch::default()),
        }
    }

    async fn get_more(&self, cursor: &mut CursorHandle) -> Result<Batch, MongoError> {
        self.send(cursor.get_more_command(1000, 1000, MongoValue::Null))?;
        let reply = self.lock().get_mores.pop_front();
        match reply {
            Some(mut reply) => {
                // The next cursor handle keeps the database and the collection of this one.
                if let Ok(batch) = &mut reply.result
                    && let Some(next) = &mut batch.cursor
                {
                    next.database.clone_from(&cursor.database);
                    next.collection.clone_from(&cursor.collection);
                    next.read_preference = cursor.read_preference;
                }
                self.answer(reply, None).await
            }
            None => Ok(Batch::default()),
        }
    }

    async fn kill_cursor(&self, cursor: CursorHandle) {
        let id = cursor.id;
        if self.send(cursor.kill_command()).is_ok() {
            self.lock().killed.push(id);
        }
    }

    async fn finish(&self, cursor: Option<CursorHandle>) {
        let id = cursor.map(|c| c.id);
        // The driver ends the sessions of the query: the fake has no session id.
        let _ = self.send(MongoCommand::EndSessions(EndSessionsCommand { session_ids: Vec::new() }));
        self.lock().finished.push(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MongoTlsMode;
    use crate::mongo::iface::ReadOp;
    use hune_core::ids::SourceId;

    fn config() -> MongoSourceConfig {
        MongoSourceConfig {
            id: SourceId::parse("events").unwrap(),
            label: "Events".to_owned(),
            topology: MongoTopology::Direct { host: "h".to_owned(), port: 27017 },
            user: "hune_ro".to_owned(),
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

    fn request(stage: &str) -> ReadRequest {
        ReadRequest {
            database: "analytics".to_owned(),
            op: ReadOp::Aggregate {
                collection: "c".to_owned(),
                pipeline: vec![MongoValue::from_extended_json(stage).unwrap()],
            },
            batch_size: 101,
            max_time_ms: 60_000,
            read_preference: ReadPreference::Primary,
            comment: MongoValue::Null,
        }
    }

    #[tokio::test]
    async fn the_script_gives_batches_errors_and_cursors_and_the_record_shows_the_commands() {
        let fake = FakeMongo::for_config(&config());
        fake.push_read(Reply::docs_with_cursor(&[r#"{"n":1}"#, r#"{"n":2}"#], 77));
        fake.push_get_more(Reply::docs(&[r#"{"n":3}"#]));
        fake.push_get_more(Reply::error(MongoError::server(43, "CursorNotFound", "gone\nmore")));
        let first = fake.read(request(r#"{"$match":{}}"#)).await.unwrap();
        assert_eq!(first.docs.len(), 2);
        let mut cursor = first.cursor.unwrap();
        // The fake gives the cursor the database and the collection of the read.
        assert_eq!((cursor.id, cursor.database.as_str(), cursor.collection.as_str()), (77, "analytics", "c"));
        assert_eq!(fake.get_more(&mut cursor).await.unwrap().docs.len(), 1);
        let e = fake.get_more(&mut cursor).await.unwrap_err();
        assert_eq!((e.class, e.code, e.message.as_str()), (MongoErrorClass::Server, Some(43), "gone"));
        // An empty queue gives an empty batch.
        let empty = fake.get_more(&mut cursor).await.unwrap();
        assert!(empty.docs.is_empty() && empty.cursor.is_none());
        fake.kill_cursor(cursor).await;
        fake.finish(None).await;
        assert_eq!(fake.killed(), vec![77]);
        assert_eq!(fake.finished(), vec![None]);
        assert_eq!(fake.command_names(), ["aggregate", "getMore", "getMore", "getMore", "killCursors", "endSessions"]);
        assert_eq!(fake.requests().len(), 1);
        assert_eq!(fake.pending_replies(), (0, 0));
    }

    #[tokio::test]
    async fn the_probe_answers_come_from_values_that_a_test_can_change() {
        let fake = FakeMongo::for_config(&config());
        let info = fake.server_info().await.unwrap();
        assert_eq!((info.version.as_str(), info.set_name), ("7.0.14", None));
        let auth = fake.connection_status().await.unwrap();
        assert_eq!((auth.users.len(), auth.roles.len(), auth.privileges.len()), (1, 1, 1));
        assert_eq!(fake.list_collections("analytics").await.unwrap(), ["c"]);
        assert!(fake.list_collections("unknown").await.is_err(), "the guard refuses a database outside the list");
        fake.set_guard_lists(&["analytics", "unknown"], &["c"]);
        assert!(fake.list_collections("unknown").await.unwrap().is_empty());
        fake.edit_server_info(|i| i.version = "6.0.1".to_owned());
        assert_eq!(fake.server_info().await.unwrap().version, "6.0.1");
        fake.fail_auth(MongoError::server(13, "Unauthorized", "x"));
        assert_eq!(fake.connection_status().await.unwrap_err().code, Some(13));
        fake.set_collections("analytics", &["c", "d"]);
        assert_eq!(fake.list_collections("analytics").await.unwrap(), ["c", "d"]);
        fake.fail_collections("analytics", MongoError::new(MongoErrorClass::Network, "x"));
        assert!(fake.list_collections("analytics").await.is_err());
        assert_eq!(fake.command_names().iter().filter(|n| **n == "hello").count(), 2);
    }

    #[tokio::test]
    async fn the_guard_runs_before_the_record() {
        let fake = FakeMongo::for_config(&config());
        for stage in [r#"{"$out":"x"}"#, r#"{"$merge":{"into":"x"}}"#, r#"{"$function":{}}"#, r#"{"$where":"1"}"#] {
            let e = fake.read(request(stage)).await.unwrap_err();
            assert_eq!(e.class, MongoErrorClass::Guard, "{stage}");
        }
        assert!(fake.commands().is_empty());
        assert!(fake.requests().is_empty());
        // A fake with no lists still refuses the keys.
        let bare = FakeMongo::new();
        assert!(bare.read(request(r#"{"$out":"x"}"#)).await.is_err());
        assert!(bare.read(request(r#"{"$match":{}}"#)).await.is_ok());
        assert_eq!(bare.command_names(), ["aggregate"]);
    }

    #[tokio::test]
    async fn a_reply_can_wait() {
        let fake = FakeMongo::new();
        fake.push_read(Reply::docs(&[]).after(Duration::from_millis(60)));
        let start = std::time::Instant::now();
        fake.read(request(r#"{"$match":{}}"#)).await.unwrap();
        assert!(start.elapsed() >= Duration::from_millis(55));
    }

    #[tokio::test]
    async fn the_connector_shares_the_script_and_counts_the_connects() {
        let fake = FakeMongo::for_config(&config());
        let connector = fake.connector();
        let options = ConnectOptions::from_config(&config());
        let a = connector.connect(options.clone()).await.unwrap();
        assert_eq!(fake.connects(), 1);
        a.server_info().await.unwrap();
        assert_eq!(fake.command_names(), ["hello", "buildInfo"]);
        fake.fail_next_connect(MongoError::new(MongoErrorClass::Network, "refused"));
        assert!(connector.connect(options.clone()).await.is_err());
        assert!(connector.connect(options.clone()).await.is_ok());
        fake.fail_every_connect(Some(MongoError::new(MongoErrorClass::Auth, "bad")));
        assert!(connector.connect(options.clone()).await.is_err());
        fake.fail_every_connect(None);
        assert!(connector.connect(options).await.is_ok());
        assert_eq!(fake.connects(), 5);
        assert_eq!(fake.last_connect_options().unwrap().user, "hune_ro");
        // The client of the trait makes a fake of its own.
        let own = FakeMongo::connect(ConnectOptions::from_config(&config())).await.unwrap();
        assert_eq!(own.connects(), 1);
        assert!(own.commands().is_empty());
    }

    #[test]
    fn the_replica_set_account_announces_the_members_of_the_block() {
        let mut c = config();
        c.topology = MongoTopology::ReplicaSet {
            hosts: vec![
                crate::config::MongoHost { host: "m1".to_owned(), port: 27017 },
                crate::config::MongoHost { host: "m2".to_owned(), port: 27018 },
            ],
            replica_set: "rs0".to_owned(),
        };
        let fake = FakeMongo::for_config(&c);
        let info = fake.lock().info.clone().unwrap();
        assert_eq!(info.set_name.as_deref(), Some("rs0"));
        assert_eq!(info.hosts, ["m1:27017", "m2:27018"]);
    }
}
