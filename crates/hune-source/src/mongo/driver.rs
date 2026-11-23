// SPDX-License-Identifier: MIT OR Apache-2.0
//! The driver of a MongoDB source (8b.2, 8b.4, 8b.5, 8b.16).
//!
//! This is the only module that names the crates `mongodb` and `bson` (8b.2, policy check of 16.18). It
//! implements [`MongoClient`] with the official driver. The rest of the crate sees the trait only.
//!
//! # What the module does
//!
//! - [`MongoDriver::connect`] builds the options of the driver in code, from the typed block. It never
//!   builds a connection string, so a value of a key cannot add an option (8b.4). The topology `direct` sets
//!   one host and the option `direct_connection`. The topology `replica_set` sets the hosts. It does not
//!   set the set name: the probe compares the name that `hello` gives with the key (check M4). The module sets `appName`, no compressor, no retry of reads or writes, the pool limits of 8b.9
//!   and the login with SCRAM-SHA-256 on `auth_database`. It makes no SRV lookup.
//! - TLS. The mode `verify-full` gives the driver the CA file of the block and nothing else: the driver
//!   then loads no other root, and it checks the host name of the address (see [`tls_options`]). The mode
//!   `disable` gives no TLS. A CA file whose bytes differ from the bytes that the start read is refused.
//! - Each read goes through the wire guard of `guard.rs` before the encode. The module builds the
//!   command document from the typed [`MongoCommand`] of the request. It never takes a document from
//!   another place. The calls `run_raw_command` and `run_raw_cursor_command` are used here and nowhere else
//!   (policy check "No command document from a caller").
//! - The driver makes the cursor commands. A cursor of `aggregate` or `find` is a raw batch cursor: the
//!   driver sends `getMore` to the server that made the cursor, with the batch size and the `comment` of the
//!   request. A `getMore` has no `maxTimeMS`: a server refuses the member for a cursor that is not tailable
//!   (code 2, `BadValue`). The client deadline of 8b.7 bounds the whole query. A kill is the drop of the
//!   cursor: the driver then sends `killCursors`.
//! - [`decode_document`] converts a BSON document of a reply to the Hune model, with no panic on any data.
//!
//! # Limits that the module records
//!
//! - The driver sends `killCursors` on the drop of a cursor to the primary of the set when the read
//!   preference allows a secondary. The server then answers `CursorNotFound`, and the cursor on the
//!   secondary ends by the idle timeout of the server. A direct connection has one server, so it is not
//!   affected.
//! - The driver reads no environment variable. The crate `mongodb` is a local copy in `vendor/mongodb`
//!   (8b.2) without the reads of the original. The metadata of `hello` thus holds no
//!   name of a platform or a container. The test `mongosource_hello` makes sure of this.

use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use bson::raw::{CString, RawArrayBuf, RawBson, RawBsonRef, RawDocument, RawDocumentBuf};
use bson::spec::BinarySubtype;
use bson::{Binary, Bson, Decimal128};
use futures_core::Stream;
use hune_core::mongo::command::{
    ConnectionStatusCommand, HelloCommand, ListCollectionsCommand, MongoCommand, ReadPreference,
};
use hune_core::mongo::{Decimal128Text, Document, MongoValue, OtherBsonType};
use mongodb::Client;
use mongodb::error::{Error, ErrorKind};
use mongodb::event::EventHandler;
use mongodb::event::cmap::CmapEvent;
use mongodb::event::sdam::SdamEvent;
use mongodb::options::{
    AuthMechanism, ClientOptions, Credential, ReadPreference as DriverReadPreference, SelectionCriteria, ServerAddress,
    Tls, TlsOptions,
};
use mongodb::raw_batch_cursor::{RawBatch, RawBatchCursor};

use crate::config::{MongoTlsMode, MongoTopology};
use crate::mongo::attrib::APP_NAME;
use crate::mongo::guard;
use crate::mongo::iface::{
    AuthInfo, AuthRole, AuthUser, Batch, CERTIFICATE_CODE_NAME, ConnectOptions, CursorHandle, MongoClient, MongoError,
    MongoErrorClass, Privilege, PrivilegeResource, ReadRequest, ServerInfo,
};

/// The deepest nesting of a document of a reply or of a command. A deeper value gives an error: the
/// server limit is lower, so a deeper value is not from a server.
const MAX_DEPTH: usize = 200;

/// The time after which the pool closes an idle connection (8b.9).
const MAX_IDLE: Duration = Duration::from_secs(60);

/// The most cursors that the driver keeps for the queries in flight. The admission of the source
/// bounds the real number far below this value.
const MAX_OPEN_CURSORS: usize = 4096;

/// The state that the driver shares with its monitor callback.
#[derive(Default)]
struct Shared {
    /// The last fault of the monitor connection of a member, until a heartbeat succeeds. A server
    /// selection that fails gives the class of this fault (a certificate, the link, the login).
    monitor_fault: Mutex<Option<MongoError>>,
    /// The number of connections that the pool made. A read that fails on a connection while this number
    /// does not change used a connection that was already in the pool (see [`MongoDriver::read`]).
    created: AtomicU64,
}

/// The client of a MongoDB source over the official driver (8b.16).
pub struct MongoDriver {
    client: Client,
    shared: Arc<Shared>,
    read_preference: ReadPreference,
    databases: Vec<String>,
    collections: Vec<String>,
    /// The raw batch cursors that are open, by server cursor id.
    cursors: Mutex<HashMap<i64, RawBatchCursor>>,
}

impl std::fmt::Debug for MongoDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MongoDriver").finish_non_exhaustive()
    }
}

// ---- the options of the driver ----

/// Gives the read preference of the driver for the one of the block.
fn driver_read_preference(rp: ReadPreference) -> DriverReadPreference {
    match rp {
        ReadPreference::Primary => DriverReadPreference::Primary,
        ReadPreference::PrimaryPreferred => DriverReadPreference::PrimaryPreferred { options: None },
        ReadPreference::Secondary => DriverReadPreference::Secondary { options: None },
        ReadPreference::SecondaryPreferred => DriverReadPreference::SecondaryPreferred { options: None },
        ReadPreference::Nearest => DriverReadPreference::Nearest { options: None },
    }
}

fn criteria(rp: ReadPreference) -> SelectionCriteria {
    SelectionCriteria::ReadPreference(driver_read_preference(rp))
}

/// Makes the TLS options of the driver. With `verify-full`, the CA file is the only source of trust
/// roots: the driver loads the roots of `webpki-roots` only when `ca_file_path` is `None`
/// (`src/runtime/tls_rustls.rs` of the crate `mongodb` 3.10.0, function `make_rustls_config`). The
/// options never allow an invalid certificate and set no client certificate.
///
/// # Errors
///
/// The mode has no CA file, or the file differs from the bytes that the start read.
pub(crate) fn tls_options(options: &ConnectOptions) -> Result<Option<Tls>, MongoError> {
    let cert_fault = |m: &str| MongoError {
        class: MongoErrorClass::Tls,
        code: None,
        code_name: Some(CERTIFICATE_CODE_NAME.to_owned()),
        message: m.to_owned(),
    };
    match options.tls_mode {
        MongoTlsMode::Disable => Ok(Some(Tls::Disabled)),
        MongoTlsMode::VerifyFull => {
            let (Some(tls), Some(path)) = (&options.tls, &options.tls_ca_file) else {
                return Err(cert_fault("The block has no CA file."));
            };
            check_ca_file(path, &tls.ca_pem).map_err(|m| cert_fault(&m))?;
            let tls_options =
                TlsOptions::builder().ca_file_path(path.clone()).allow_invalid_certificates(false).build();
            Ok(Some(Tls::Enabled(tls_options)))
        }
    }
}

/// The CA file must still hold the bytes that the start read and checked (13.2 rule 4: the file is
/// read one time). A file that changed gives an error, so the trust roots never change in silence.
fn check_ca_file(path: &Path, expected: &[u8]) -> Result<(), String> {
    if expected.is_empty() {
        return Err("The bytes of the CA file are not known.".to_owned());
    }
    match std::fs::read(path) {
        Ok(bytes) if bytes == expected => Ok(()),
        Ok(_) => Err("The CA file changed after the start.".to_owned()),
        Err(_) => Err("The CA file cannot be read.".to_owned()),
    }
}

/// Builds the options of the driver from the options of the connection (8b.4, 8b.9).
///
/// # Errors
///
/// The TLS options are not valid (see [`tls_options`]).
pub(crate) fn client_options(options: &ConnectOptions) -> Result<ClientOptions, MongoError> {
    let mut opts = ClientOptions::builder()
        .hosts(match &options.topology {
            MongoTopology::Direct { host, port } => vec![address(host, *port)],
            MongoTopology::ReplicaSet { hosts, .. } => hosts.iter().map(|h| address(&h.host, h.port)).collect(),
        })
        .app_name(APP_NAME.to_owned())
        .connect_timeout(options.connect_timeout)
        .server_selection_timeout(options.connect_timeout)
        .max_pool_size(options.max_pool_size.max(1))
        .min_pool_size(0_u32)
        .max_idle_time(MAX_IDLE)
        .retry_reads(false)
        .retry_writes(false)
        .max_adaptive_retries(0_u32)
        .selection_criteria(criteria(options.read_preference))
        .credential(
            Credential::builder()
                .username(options.user.clone())
                .password(options.password.clone())
                .source(options.auth_database.clone())
                .mechanism(AuthMechanism::ScramSha256)
                .build(),
        )
        .build();
    match &options.topology {
        MongoTopology::Direct { .. } => opts.direct_connection = Some(true),
        // The set name stays out of the options of the driver. A driver that holds the name drops a member of
        // another set in silence, and the source then shows a timeout only. The probe reads the name from
        // `hello` and compares it with the key (check M4), so the operator gets the reason.
        MongoTopology::ReplicaSet { .. } => {}
    }
    opts.tls = tls_options(options)?;
    Ok(opts)
}

fn address(host: &str, port: u16) -> ServerAddress {
    ServerAddress::Tcp { host: host.to_owned(), port: Some(port) }
}

// ---- errors ----

/// Gives the class of an I/O error: a TLS fault when the cause is an error of `rustls`.
fn io_error(io: &std::io::Error) -> MongoError {
    let tls = io.get_ref().and_then(|e| e.downcast_ref::<rustls::Error>());
    match tls {
        Some(rustls::Error::InvalidCertificate(_)) => MongoError {
            class: MongoErrorClass::Tls,
            code: None,
            code_name: Some(CERTIFICATE_CODE_NAME.to_owned()),
            message: "The certificate of the server failed the check.".to_owned(),
        },
        Some(_) => MongoError::new(MongoErrorClass::Tls, "The TLS handshake failed."),
        None => MongoError::new(MongoErrorClass::Network, "The link to the server failed."),
    }
}

/// Maps an error of the driver to the error of the interface (8b.8). The mapping reads the kind of the
/// error and the code of the server, never the message of the server.
fn map_error(error: &Error, shared: &Shared) -> MongoError {
    match &*error.kind {
        ErrorKind::Command(c) => MongoError::server(c.code, &c.code_name, &c.message),
        ErrorKind::Authentication { .. } => MongoError::new(MongoErrorClass::Auth, "The login failed."),
        ErrorKind::Io(io) => io_error(io),
        ErrorKind::InvalidTlsConfig { .. } => MongoError {
            class: MongoErrorClass::Tls,
            code: None,
            code_name: Some(CERTIFICATE_CODE_NAME.to_owned()),
            message: "The TLS options are not valid.".to_owned(),
        },
        ErrorKind::ServerSelection { .. } | ErrorKind::ConnectionPoolCleared { .. } => {
            let fault = shared.monitor_fault.lock().unwrap_or_else(PoisonError::into_inner).clone();
            fault.unwrap_or_else(|| MongoError::new(MongoErrorClass::Network, "No server answered in time."))
        }
        ErrorKind::BsonDeserialization(_)
        | ErrorKind::BsonSerialization(_)
        | ErrorKind::Bson(_)
        | ErrorKind::InvalidResponse { .. } => MongoError::new(MongoErrorClass::Decode, "The reply did not decode."),
        _ => MongoError::new(MongoErrorClass::Network, "The call to the server failed."),
    }
}

/// True when the error is an I/O error of a link that the other side closed or reset: the form of a failure of
/// a connection that stayed idle in the pool. A timeout, a TLS fault and a server selection are not of this form.
fn is_closed_link(error: &Error) -> bool {
    use std::io::ErrorKind as Io;
    match &*error.kind {
        ErrorKind::Io(io) => {
            // A fault of `rustls` (a certificate, an alert) is not a closed link. The close of the peer with no
            // `close_notify` is an I/O error of the kind `UnexpectedEof` with a text.
            io.get_ref().is_none_or(|inner| inner.downcast_ref::<rustls::Error>().is_none())
                && matches!(
                    io.kind(),
                    Io::UnexpectedEof | Io::ConnectionReset | Io::ConnectionAborted | Io::BrokenPipe | Io::NotConnected
                )
        }
        _ => false,
    }
}

fn decode_error(message: &str) -> MongoError {
    MongoError::new(MongoErrorClass::Decode, message)
}

fn guard_error(message: &str) -> MongoError {
    MongoError::new(MongoErrorClass::Guard, message)
}

// ---- values: BSON to the Hune model ----

/// Converts the bytes of a BSON document to the value model of 8b.16.
///
/// Int64 and Decimal128 keep their exact value, a Date keeps its milliseconds, an ObjectId and a UUID (the
/// binary subtype 4 with 16 bytes) keep their bytes, and each other type gives `Other` with the type
/// only. No input makes the function panic: a document that is not valid BSON, or that is nested deeper
/// than [`MAX_DEPTH`], gives an error of the class `Decode`.
///
/// # Errors
///
/// The bytes are not a valid BSON document.
pub fn decode_document(bytes: &[u8]) -> Result<Document, MongoError> {
    let doc = RawDocument::from_bytes(bytes).map_err(|_| decode_error("The reply holds an invalid document."))?;
    members(doc, 0)
}

fn members(doc: &RawDocument, depth: usize) -> Result<Document, MongoError> {
    if depth >= MAX_DEPTH {
        return Err(decode_error("The reply holds a document that is nested too deep."));
    }
    let mut out = Vec::new();
    for item in doc.iter() {
        let (key, value) = item.map_err(|_| decode_error("The reply holds an invalid document."))?;
        out.push((key.as_str().to_owned(), value_of(value, depth + 1)?));
    }
    Ok(out)
}

fn value_of(value: RawBsonRef<'_>, depth: usize) -> Result<MongoValue, MongoError> {
    Ok(match value {
        RawBsonRef::Null => MongoValue::Null,
        RawBsonRef::Boolean(b) => MongoValue::Bool(b),
        RawBsonRef::Int32(n) => MongoValue::Int32(n),
        RawBsonRef::Int64(n) => MongoValue::Int64(n),
        RawBsonRef::Double(n) => MongoValue::Double(n),
        RawBsonRef::Decimal128(d) => match Decimal128Text::from_server(d.to_string()) {
            Some(text) => MongoValue::Decimal128(text),
            None => return Err(decode_error("The reply holds an invalid Decimal128.")),
        },
        RawBsonRef::String(s) => MongoValue::String(s.to_owned()),
        RawBsonRef::DateTime(d) => MongoValue::DateMs(d.timestamp_millis()),
        RawBsonRef::ObjectId(o) => MongoValue::ObjectId(o.bytes()),
        RawBsonRef::Binary(b) => match (b.subtype, <[u8; 16]>::try_from(b.bytes)) {
            (BinarySubtype::Uuid, Ok(bytes)) => MongoValue::Uuid(bytes),
            _ => MongoValue::Other(OtherBsonType::Binary),
        },
        RawBsonRef::Array(array) => {
            if depth >= MAX_DEPTH {
                return Err(decode_error("The reply holds an array that is nested too deep."));
            }
            let mut items = Vec::new();
            for item in array {
                let item = item.map_err(|_| decode_error("The reply holds an invalid array."))?;
                items.push(value_of(item, depth + 1)?);
            }
            MongoValue::Array(items)
        }
        RawBsonRef::Document(doc) => MongoValue::Document(members(doc, depth)?),
        RawBsonRef::RegularExpression(_) => MongoValue::Other(OtherBsonType::Regex),
        RawBsonRef::Timestamp(_) => MongoValue::Other(OtherBsonType::Timestamp),
        RawBsonRef::JavaScriptCode(_) | RawBsonRef::JavaScriptCodeWithScope(_) => {
            MongoValue::Other(OtherBsonType::Code)
        }
        RawBsonRef::Symbol(_) => MongoValue::Other(OtherBsonType::Symbol),
        RawBsonRef::MinKey => MongoValue::Other(OtherBsonType::MinKey),
        RawBsonRef::MaxKey => MongoValue::Other(OtherBsonType::MaxKey),
        RawBsonRef::DbPointer(_) => MongoValue::Other(OtherBsonType::DbPointer),
        RawBsonRef::Undefined => MongoValue::Other(OtherBsonType::Undefined),
    })
}

// ---- values: the Hune model to BSON ----

fn to_raw(value: &MongoValue, depth: usize) -> Result<RawBson, MongoError> {
    if depth >= MAX_DEPTH {
        return Err(guard_error("The command is nested too deep."));
    }
    Ok(match value {
        MongoValue::Null => RawBson::Null,
        MongoValue::Bool(b) => RawBson::Boolean(*b),
        MongoValue::Int32(n) => RawBson::Int32(*n),
        MongoValue::Int64(n) => RawBson::Int64(*n),
        MongoValue::Double(n) => RawBson::Double(*n),
        MongoValue::Decimal128(text) => RawBson::Decimal128(
            text.as_str().parse::<Decimal128>().map_err(|_| guard_error("The command holds an invalid Decimal128."))?,
        ),
        MongoValue::String(s) => RawBson::String(s.clone()),
        MongoValue::DateMs(ms) => RawBson::DateTime(bson::DateTime::from_millis(*ms)),
        MongoValue::ObjectId(bytes) => RawBson::ObjectId(bson::oid::ObjectId::from_bytes(*bytes)),
        MongoValue::Uuid(bytes) => RawBson::Binary(Binary { subtype: BinarySubtype::Uuid, bytes: bytes.to_vec() }),
        MongoValue::Array(items) => {
            let mut array = RawArrayBuf::new();
            for item in items {
                array.push(to_raw(item, depth + 1)?);
            }
            RawBson::Array(array)
        }
        MongoValue::Document(members) => RawBson::Document(to_raw_document(members, depth + 1)?),
        MongoValue::Other(_) => return Err(guard_error("The command holds a value that the model does not read.")),
    })
}

/// Builds a document and keeps the order of the members. A member that comes twice stays twice.
fn to_raw_document(members: &[(String, MongoValue)], depth: usize) -> Result<RawDocumentBuf, MongoError> {
    let mut doc = RawDocumentBuf::new();
    for (key, value) in members {
        let key = CString::try_from(key.as_str()).map_err(|_| guard_error("A key of the command holds a NUL byte."))?;
        doc.append(key, to_raw(value, depth)?);
    }
    Ok(doc)
}

/// The command document of a typed command, and its database. The driver adds `$db` and the read
/// preference itself, so the two members that the model holds for them stay out of the document.
fn command_document(command: &MongoCommand) -> Result<(String, RawDocumentBuf), MongoError> {
    let members = command.to_document().ok_or_else(|| guard_error("The command has no document form."))?;
    let database = match members.iter().find(|(k, _)| k == "$db").map(|(_, v)| v) {
        Some(MongoValue::String(db)) => db.clone(),
        _ => return Err(guard_error("The command has no database.")),
    };
    let kept: Vec<(String, MongoValue)> =
        members.into_iter().filter(|(k, _)| k != "$db" && k != "$readPreference").collect();
    Ok((database, to_raw_document(&kept, 0)?))
}

// ---- replies ----

fn as_i64(value: &Bson) -> Option<i64> {
    match value {
        Bson::Int32(n) => Some(i64::from(*n)),
        Bson::Int64(n) => Some(*n),
        #[allow(clippy::cast_possible_truncation)]
        Bson::Double(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => Some(*n as i64),
        _ => None,
    }
}

fn text_list(doc: &bson::Document, key: &str) -> Vec<String> {
    match doc.get(key) {
        Some(Bson::Array(items)) => items
            .iter()
            .filter_map(|v| match v {
                Bson::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Reads the cursor id of a reply (`cursor.id`). The id 0 means that the cursor is closed.
fn reply_cursor_id(reply: &RawDocument) -> Result<i64, MongoError> {
    let cursor = reply.get_document("cursor").map_err(|_| decode_error("The reply has no cursor."))?;
    match cursor.get("id") {
        Ok(Some(RawBsonRef::Int64(id))) => Ok(id),
        Ok(Some(RawBsonRef::Int32(id))) => Ok(i64::from(id)),
        _ => Err(decode_error("The cursor of the reply has no id.")),
    }
}

/// Converts the batch of a reply, and tells the cursor id.
fn batch_of(reply: &RawBatch) -> Result<(Vec<Document>, i64, u64), MongoError> {
    let raw = reply.as_raw_document();
    let id = reply_cursor_id(raw)?;
    let array = reply.doc_slices().map_err(|_| decode_error("The reply has no batch."))?;
    let mut docs = Vec::new();
    for item in array {
        let item = item.map_err(|_| decode_error("The batch holds an invalid element."))?;
        let RawBsonRef::Document(doc) = item else {
            return Err(decode_error("The batch holds a value that is not a document."));
        };
        docs.push(members(doc, 0)?);
    }
    Ok((docs, id, raw.as_bytes().len() as u64))
}

/// Pulls the next batch of a raw batch cursor.
async fn next_batch(cursor: &mut RawBatchCursor) -> Option<Result<RawBatch, Error>> {
    std::future::poll_fn(|cx| Pin::new(&mut *cursor).poll_next(cx)).await
}

impl MongoDriver {
    fn lock_cursors(&self) -> std::sync::MutexGuard<'_, HashMap<i64, RawBatchCursor>> {
        self.cursors.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn map(&self, error: &Error) -> MongoError {
        map_error(error, &self.shared)
    }

    /// Runs one command of the closed list on a database that is not a read of a query. The wire guard
    /// checks the command first.
    async fn run_admin(&self, command: MongoCommand) -> Result<bson::Document, MongoError> {
        guard::check_command(&command, &self.databases, &self.collections).map_err(|r| guard_error(r.reason()))?;
        let (database, document) = command_document(&command)?;
        self.client
            .database(&database)
            .run_raw_command(document)
            .selection_criteria(criteria(self.read_preference))
            .await
            .map_err(|e| self.map(&e))
    }

    /// Sends the `aggregate` or the `find` of a request and reads the first batch. The flag of the error is
    /// true when the link failed in the way of a connection that the other side closed.
    async fn read_once(&self, request: &ReadRequest) -> Result<Batch, (MongoError, bool)> {
        let plain = |e: MongoError| (e, false);
        let command = request.to_command();
        let (database, document) = command_document(&command).map_err(plain)?;
        let comment: Bson = to_raw(&request.comment, 0)
            .map_err(plain)?
            .try_into()
            .map_err(|_| plain(guard_error("The comment of the command is not valid.")))?;
        let driver_error = |e: &Error| (self.map(e), is_closed_link(e));
        let mut cursor = self
            .client
            .database(&database)
            .run_raw_cursor_command(document)
            .selection_criteria(criteria(request.read_preference))
            .batch_size(request.batch_size)
            .comment(comment)
            .batch()
            .await
            .map_err(|e| driver_error(&e))?;
        let reply = match next_batch(&mut cursor).await {
            Some(Ok(reply)) => reply,
            Some(Err(e)) => return Err(driver_error(&e)),
            None => return Ok(Batch::default()),
        };
        let (docs, id, reply_bytes) = batch_of(&reply).map_err(plain)?;
        let cursor = if id == 0 {
            None
        } else {
            Some(
                self.keep_cursor(cursor, id, &request.database, request.collection(), request.read_preference)
                    .map_err(plain)?,
            )
        };
        Ok(Batch { docs, cursor, reply_bytes })
    }

    /// Makes the handle of a cursor from the reply, and keeps the cursor of the driver.
    fn keep_cursor(
        &self,
        cursor: RawBatchCursor,
        id: i64,
        database: &str,
        collection: &str,
        read_preference: ReadPreference,
    ) -> Result<CursorHandle, MongoError> {
        let mut map = self.lock_cursors();
        if map.len() >= MAX_OPEN_CURSORS {
            // The cursor drops, so the driver kills it. The limit is far above the admission limit.
            return Err(MongoError::new(MongoErrorClass::Network, "Too many cursors are open."));
        }
        map.insert(id, cursor);
        Ok(CursorHandle { id, database: database.to_owned(), collection: collection.to_owned(), read_preference })
    }
}

impl MongoClient for MongoDriver {
    async fn connect(options: ConnectOptions) -> Result<MongoDriver, MongoError> {
        let shared = Arc::new(Shared::default());
        let mut opts = client_options(&options)?;
        let monitor = Arc::clone(&shared);
        opts.sdam_event_handler = Some(EventHandler::callback(move |event: SdamEvent| match event {
            SdamEvent::ServerHeartbeatFailed(e) => {
                let fault = map_error(&e.failure, &Shared::default());
                *monitor.monitor_fault.lock().unwrap_or_else(PoisonError::into_inner) = Some(fault);
            }
            SdamEvent::ServerHeartbeatSucceeded(_) => {
                *monitor.monitor_fault.lock().unwrap_or_else(PoisonError::into_inner) = None;
            }
            _ => {}
        }));
        let pool = Arc::clone(&shared);
        opts.cmap_event_handler = Some(EventHandler::callback(move |event: CmapEvent| {
            if matches!(event, CmapEvent::ConnectionCreated(_)) {
                pool.created.fetch_add(1, Ordering::AcqRel);
            }
        }));
        let client = Client::with_options(opts).map_err(|e| map_error(&e, &shared))?;
        let driver = MongoDriver {
            client,
            shared,
            read_preference: options.read_preference,
            databases: options.databases.clone(),
            collections: options.collections.clone(),
            cursors: Mutex::new(HashMap::new()),
        };
        // The first command makes the connection, the TLS handshake and the login (8b.4). The time is
        // bounded by the server selection timeout and by the one here.
        let limit = options.connect_timeout + Duration::from_secs(1);
        match tokio::time::timeout(limit, driver.server_info()).await {
            Ok(Ok(_)) => Ok(driver),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                let fault = driver.shared.monitor_fault.lock().unwrap_or_else(PoisonError::into_inner).clone();
                Err(fault.unwrap_or_else(|| MongoError::new(MongoErrorClass::Network, "The connection timed out.")))
            }
        }
    }

    async fn server_info(&self) -> Result<ServerInfo, MongoError> {
        let hello = self.run_admin(MongoCommand::Hello(HelloCommand { hello_ok: true })).await?;
        let build = self.run_admin(MongoCommand::BuildInfo).await?;
        let version = match build.get("version") {
            Some(Bson::String(v)) => v.clone(),
            _ => return Err(decode_error("The reply of buildInfo has no version.")),
        };
        let mut hosts = text_list(&hello, "hosts");
        hosts.extend(text_list(&hello, "passives"));
        hosts.extend(text_list(&hello, "arbiters"));
        Ok(ServerInfo {
            version,
            max_wire_version: hello
                .get("maxWireVersion")
                .and_then(as_i64)
                .and_then(|n| i32::try_from(n).ok())
                .unwrap_or(0),
            max_bson_object_size: hello.get("maxBsonObjectSize").and_then(as_i64).unwrap_or(0),
            set_name: match hello.get("setName") {
                Some(Bson::String(s)) => Some(s.clone()),
                _ => None,
            },
            hosts,
            is_mongos: matches!(hello.get("msg"), Some(Bson::String(m)) if m == "isdbgrid"),
        })
    }

    async fn connection_status(&self) -> Result<AuthInfo, MongoError> {
        let reply = self.run_admin(MongoCommand::ConnectionStatus(ConnectionStatusCommand)).await?;
        let Some(Bson::Document(info)) = reply.get("authInfo") else {
            return Err(decode_error("The reply of connectionStatus has no authInfo."));
        };
        let pairs = |key: &str, first: &str| -> Vec<(String, String)> {
            match info.get(key) {
                Some(Bson::Array(items)) => items
                    .iter()
                    .filter_map(|v| match v {
                        Bson::Document(d) => match (d.get(first), d.get("db")) {
                            (Some(Bson::String(a)), Some(Bson::String(b))) => Some((a.clone(), b.clone())),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            }
        };
        let users = pairs("authenticatedUsers", "user").into_iter().map(|(user, db)| AuthUser { user, db }).collect();
        let roles =
            pairs("authenticatedUserRoles", "role").into_iter().map(|(role, db)| AuthRole { role, db }).collect();
        let mut privileges = Vec::new();
        if let Some(Bson::Array(items)) = info.get("authenticatedUserPrivileges") {
            for item in items {
                let Bson::Document(p) = item else { continue };
                let resource = match p.get("resource") {
                    Some(Bson::Document(r)) if r.get("cluster") == Some(&Bson::Boolean(true)) => {
                        PrivilegeResource::Cluster
                    }
                    Some(Bson::Document(r)) => match (r.get("db"), r.get("collection")) {
                        (Some(Bson::String(db)), Some(Bson::String(c))) => {
                            PrivilegeResource::Namespace { db: db.clone(), collection: c.clone() }
                        }
                        _ => PrivilegeResource::Other,
                    },
                    _ => PrivilegeResource::Other,
                };
                privileges.push(Privilege { resource, actions: text_list(p, "actions") });
            }
        }
        Ok(AuthInfo { users, roles, privileges })
    }

    async fn list_collections(&self, database: &str) -> Result<Vec<String>, MongoError> {
        let command = MongoCommand::ListCollections(ListCollectionsCommand { database: database.to_owned() });
        guard::check_command(&command, &self.databases, &self.collections).map_err(|r| guard_error(r.reason()))?;
        let (database, document) = command_document(&command)?;
        let mut cursor = self
            .client
            .database(&database)
            .run_raw_cursor_command(document)
            .selection_criteria(criteria(self.read_preference))
            .await
            .map_err(|e| self.map(&e))?;
        let mut names = Vec::new();
        while cursor.advance().await.map_err(|e| self.map(&e))? {
            let item = decode_document(cursor.current().as_bytes())?;
            if let Some((_, MongoValue::String(name))) = item.iter().find(|(k, _)| k == "name") {
                names.push(name.clone());
            }
        }
        Ok(names)
    }

    async fn read(&self, request: ReadRequest) -> Result<Batch, MongoError> {
        // The wire guard runs before the encode (8b.5).
        guard::check_read_request(&request, &self.databases, &self.collections)?;
        let created = self.shared.created.load(Ordering::Acquire);
        match self.read_once(&request).await {
            Err((_, true)) if self.shared.created.load(Ordering::Acquire) == created => {
                // The connection came from the pool and it was closed on the other side (8b.5). The failure
                // has the form of a closed link, and the pool made no connection for this read. The driver
                // cleared the pool, so one more send uses a new connection. It is the only one.
                self.read_once(&request).await.map_err(|(e, _)| e)
            }
            Err((e, _)) => Err(e),
            Ok(batch) => Ok(batch),
        }
    }

    async fn get_more(&self, cursor: &mut CursorHandle) -> Result<Batch, MongoError> {
        // The wire guard reads the typed command that stands for the `getMore` of the driver.
        let typed = cursor.get_more_command(1, 1, MongoValue::Null);
        guard::check_command(&typed, &self.databases, &self.collections).map_err(|r| guard_error(r.reason()))?;
        // The cursor leaves the table while the call runs. A call that is dropped drops the cursor, and
        // the driver then kills it on the server (8b.7).
        let taken = self.lock_cursors().remove(&cursor.id);
        let Some(mut raw) = taken else {
            return Err(MongoError::server(43, "CursorNotFound", "The cursor is not open."));
        };
        let reply = match next_batch(&mut raw).await {
            Some(Ok(reply)) => reply,
            Some(Err(e)) => return Err(self.map(&e)),
            None => return Ok(Batch::default()),
        };
        let (docs, id, reply_bytes) = batch_of(&reply)?;
        let next = if id == 0 {
            None
        } else {
            Some(self.keep_cursor(raw, id, &cursor.database, &cursor.collection, cursor.read_preference)?)
        };
        if let Some(n) = &next {
            cursor.id = n.id;
        }
        Ok(Batch { docs, cursor: next, reply_bytes })
    }

    async fn kill_cursor(&self, cursor: CursorHandle) {
        if guard::check_command(&cursor.kill_command(), &self.databases, &self.collections).is_err() {
            return;
        }
        // The drop of the raw batch cursor makes the driver send `killCursors` (8b.7). A lost kill is
        // not an error. The yield lets the task of the driver start before the caller goes on.
        let taken = self.lock_cursors().remove(&cursor.id);
        drop(taken);
        tokio::task::yield_now().await;
    }

    async fn finish(&self, cursor: Option<CursorHandle>) {
        if let Some(cursor) = cursor {
            self.kill_cursor(cursor).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use bson::{Bson, doc};
    use hune_core::mongo::command::ReadPreference;

    use super::*;
    use crate::config::{MongoHost, SourceTls};

    fn options(topology: MongoTopology) -> ConnectOptions {
        ConnectOptions {
            topology,
            user: "hune_ro".to_owned(),
            password: "pw-marker-not-logged".to_owned(),
            auth_database: "admin".to_owned(),
            tls_mode: MongoTlsMode::Disable,
            tls: None,
            tls_ca_file: None,
            databases: vec!["analytics".to_owned()],
            collections: vec!["node_load".to_owned()],
            max_pool_size: 5,
            connect_timeout: Duration::from_secs(3),
            read_preference: ReadPreference::SecondaryPreferred,
        }
    }

    fn direct() -> MongoTopology {
        MongoTopology::Direct { host: "db.example".to_owned(), port: 27017 }
    }

    #[test]
    fn options_of_a_direct_block_are_built_in_code() {
        let o = client_options(&options(direct())).unwrap();
        assert_eq!(o.hosts, vec![ServerAddress::Tcp { host: "db.example".to_owned(), port: Some(27017) }]);
        assert_eq!(o.direct_connection, Some(true));
        assert_eq!(o.repl_set_name, None);
        assert_eq!(o.app_name.as_deref(), Some("hune"));
        assert_eq!(o.retry_reads, Some(false));
        assert_eq!(o.retry_writes, Some(false));
        assert_eq!(o.max_pool_size, Some(5));
        assert_eq!(o.min_pool_size, Some(0));
        assert_eq!(o.max_idle_time, Some(Duration::from_secs(60)));
        assert_eq!(o.connect_timeout, Some(Duration::from_secs(3)));
        assert_eq!(o.server_selection_timeout, Some(Duration::from_secs(3)));
        assert_eq!(o.tls, Some(Tls::Disabled));
        assert_eq!(
            o.selection_criteria,
            Some(SelectionCriteria::ReadPreference(DriverReadPreference::SecondaryPreferred { options: None }))
        );
        let c = o.credential.unwrap();
        assert_eq!(c.mechanism, Some(AuthMechanism::ScramSha256));
        assert_eq!(c.source.as_deref(), Some("admin"));
        assert_eq!(c.username.as_deref(), Some("hune_ro"));
        // The options never hold a connection string or a SRV name.
        assert!(o.srv_service_name.is_none() && o.srv_max_hosts.is_none());
    }

    #[test]
    fn options_of_a_replica_set_list_the_members_and_leave_the_set_name_to_the_probe() {
        let topology = MongoTopology::ReplicaSet {
            hosts: vec![
                MongoHost { host: "a.example".to_owned(), port: 27017 },
                MongoHost { host: "b.example".to_owned(), port: 27018 },
            ],
            replica_set: "rs0".to_owned(),
        };
        let o = client_options(&options(topology)).unwrap();
        assert_eq!(o.hosts.len(), 2);
        // The probe checks the set name (M4). The driver gets no name, so a member of another set gives the
        // reason `M4` and not a timeout.
        assert_eq!(o.repl_set_name, None);
        assert_ne!(o.direct_connection, Some(true));
    }

    #[test]
    fn the_password_stays_out_of_the_debug_text_of_the_client() {
        let o = options(direct());
        assert!(!format!("{o:?}").contains("pw-marker-not-logged"));
        // The options of the driver keep the password in the credential only, and `Debug` of it hides it.
        let built = client_options(&o).unwrap();
        assert!(!format!("{:?}", built.credential).contains("pw-marker-not-logged"));
    }

    #[test]
    fn verify_full_never_runs_without_a_ca_file() {
        // A mode with no CA file is refused: the driver would then use the roots of the Mozilla list. The
        // tests of `tests/mongo_tls.rs` run the CA file, the changed file and the other CA with a TLS server.
        let mut o = options(direct());
        o.tls_mode = MongoTlsMode::VerifyFull;
        let e = tls_options(&o).unwrap_err();
        assert_eq!((e.class, e.code_name.as_deref()), (MongoErrorClass::Tls, Some(CERTIFICATE_CODE_NAME)));
        o.tls = Some(SourceTls { ca_pem: b"x".to_vec(), server_name: String::new() });
        assert!(tls_options(&o).is_err(), "the bytes need a path");
        o.tls_ca_file = Some("/nonexistent/ca.pem".into());
        assert!(tls_options(&o).is_err(), "an unreadable file is refused");
        o.tls_mode = MongoTlsMode::Disable;
        assert_eq!(tls_options(&o).unwrap(), Some(Tls::Disabled));
    }

    fn raw(doc: &bson::Document) -> Vec<u8> {
        bson::serialize_to_vec(doc).unwrap()
    }

    #[test]
    fn each_bson_type_gives_its_value_of_the_model() {
        let uuid = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let d = doc! {
            "null": Bson::Null, "bool": true, "i32": i32::MAX, "i64": i64::MAX, "i64min": i64::MIN, "f": 1.5,
            "nan": f64::NAN, "dec": "12345.6700".parse::<Decimal128>().unwrap(),
            "dec_nan": "NaN".parse::<Decimal128>().unwrap(),
            "s": "text", "date": bson::DateTime::from_millis(1_767_225_600_123),
            "old": bson::DateTime::from_millis(-86_400_123),
            "oid": bson::oid::ObjectId::from_bytes([7; 12]),
            "uuid": Binary { subtype: BinarySubtype::Uuid, bytes: uuid.to_vec() },
            "bin": Binary { subtype: BinarySubtype::Generic, bytes: vec![1, 2] },
            "olduuid": Binary { subtype: BinarySubtype::UuidOld, bytes: uuid.to_vec() },
            "shortuuid": Binary { subtype: BinarySubtype::Uuid, bytes: vec![1, 2] },
            "re": Bson::RegularExpression(bson::Regex { pattern: "a".try_into().unwrap(), options: "".try_into().unwrap() }),
            "ts": Bson::Timestamp(bson::Timestamp { time: 1, increment: 2 }),
            "js": Bson::JavaScriptCode("1".to_owned()),
            "min": Bson::MinKey, "max": Bson::MaxKey,
            "arr": [1, "two", { "three": 3 }, [4]], "sub": { "a": 1, "b": { "c": "deep" } },
        };
        let m = decode_document(&raw(&d)).unwrap();
        let get = |k: &str| m.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(get("null"), MongoValue::Null);
        assert_eq!(get("i32"), MongoValue::Int32(i32::MAX));
        assert_eq!(get("i64"), MongoValue::Int64(i64::MAX));
        assert_eq!(get("i64min"), MongoValue::Int64(i64::MIN));
        assert_eq!(get("f"), MongoValue::Double(1.5));
        assert!(matches!(get("nan"), MongoValue::Double(x) if x.is_nan()));
        assert_eq!(get("dec"), MongoValue::Decimal128(Decimal128Text::from_server("12345.6700").unwrap()));
        assert_eq!(get("dec_nan"), MongoValue::Decimal128(Decimal128Text::from_server("NaN").unwrap()));
        assert_eq!(get("date"), MongoValue::DateMs(1_767_225_600_123));
        assert_eq!(get("old"), MongoValue::DateMs(-86_400_123));
        assert_eq!(get("oid"), MongoValue::ObjectId([7; 12]));
        assert_eq!(get("uuid"), MongoValue::Uuid(uuid));
        assert_eq!(get("bin"), MongoValue::Other(OtherBsonType::Binary));
        assert_eq!(get("olduuid"), MongoValue::Other(OtherBsonType::Binary));
        assert_eq!(get("shortuuid"), MongoValue::Other(OtherBsonType::Binary));
        assert_eq!(get("re"), MongoValue::Other(OtherBsonType::Regex));
        assert_eq!(get("ts"), MongoValue::Other(OtherBsonType::Timestamp));
        assert_eq!(get("js"), MongoValue::Other(OtherBsonType::Code));
        assert_eq!(get("min"), MongoValue::Other(OtherBsonType::MinKey));
        assert_eq!(get("max"), MongoValue::Other(OtherBsonType::MaxKey));
        let MongoValue::Array(items) = get("arr") else { panic!("array") };
        assert_eq!(items.len(), 4);
        assert_eq!(items[0], MongoValue::Int32(1));
        let MongoValue::Document(sub) = get("sub") else { panic!("document") };
        assert_eq!(sub[0], ("a".to_owned(), MongoValue::Int32(1)));
        // The order of the members is the order of the server.
        let keys: Vec<&str> = m.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(&keys[..3], ["null", "bool", "i32"]);
    }

    #[test]
    fn data_that_is_not_bson_gives_an_error_and_never_a_panic() {
        for bytes in [&[][..], &[0, 0, 0][..], &[5, 0, 0, 0, 1][..], &[0xff; 40][..], &b"not bson at all"[..]] {
            assert!(decode_document(bytes).is_err(), "{bytes:?}");
        }
        // A string with a length that goes past the end of the document.
        let mut bad = raw(&doc! { "a": "xyz" });
        bad[8] = 0xff;
        assert!(decode_document(&bad).is_err());
        // A document nested deeper than the limit gives an error of the class Decode.
        let mut value = Bson::Int32(1);
        for _ in 0..150 {
            value = Bson::Document(doc! { "d": value });
        }
        let mut deep = doc! { "d": value };
        let ok = decode_document(&raw(&deep));
        assert!(ok.is_ok(), "{ok:?}");
        for _ in 0..80 {
            deep = doc! { "d": Bson::Document(deep) };
        }
        let e = decode_document(&raw(&deep)).unwrap_err();
        assert_eq!(e.class, MongoErrorClass::Decode);
    }

    #[test]
    fn the_model_goes_to_bson_with_its_order_and_its_types() {
        let members = vec![
            ("aggregate".to_owned(), MongoValue::String("c".to_owned())),
            ("n".to_owned(), MongoValue::Int64(5)),
            ("d".to_owned(), MongoValue::Decimal128(Decimal128Text::from_server("1.50").unwrap())),
            ("t".to_owned(), MongoValue::DateMs(1_000)),
            ("u".to_owned(), MongoValue::Uuid([9; 16])),
            ("o".to_owned(), MongoValue::ObjectId([3; 12])),
            ("dup".to_owned(), MongoValue::Int32(1)),
            ("dup".to_owned(), MongoValue::Int32(2)),
        ];
        let doc = to_raw_document(&members, 0).unwrap();
        let back = decode_document(doc.as_bytes()).unwrap();
        assert_eq!(back, members, "order, duplicates and exact types stay");
        let bad_key = vec![("a\0b".to_owned(), MongoValue::Null)];
        assert_eq!(to_raw_document(&bad_key, 0).unwrap_err().class, MongoErrorClass::Guard);
        let other = vec![("a".to_owned(), MongoValue::Other(OtherBsonType::Code))];
        assert_eq!(to_raw_document(&other, 0).unwrap_err().class, MongoErrorClass::Guard);
    }

    #[test]
    fn a_command_document_has_no_db_and_no_read_preference_member() {
        let request = ReadRequest {
            database: "analytics".to_owned(),
            op: crate::mongo::ReadOp::Aggregate {
                collection: "node_load".to_owned(),
                pipeline: vec![MongoValue::Document(vec![("$limit".to_owned(), MongoValue::Int32(5))])],
            },
            batch_size: 5,
            max_time_ms: 1000,
            read_preference: ReadPreference::Secondary,
            comment: MongoValue::Document(vec![("app".to_owned(), MongoValue::String("hune".to_owned()))]),
        };
        let (db, doc) = command_document(&request.to_command()).unwrap();
        assert_eq!(db, "analytics");
        let keys: Vec<String> = decode_document(doc.as_bytes()).unwrap().into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["aggregate", "pipeline", "cursor", "allowDiskUse", "maxTimeMS", "comment", "readConcern"]);
    }

    #[test]
    fn server_errors_keep_their_code_and_the_login_fault_has_its_class() {
        let shared = Shared::default();
        let fault: mongodb::error::CommandError = bson::deserialize_from_document(
            doc! { "code": 50, "codeName": "MaxTimeMSExpired", "errmsg": "operation exceeded time limit\nsecond line" },
        )
        .unwrap();
        let command = Error::from(ErrorKind::Command(fault));
        let mapped = map_error(&command, &shared);
        assert_eq!((mapped.class, mapped.code), (MongoErrorClass::Server, Some(50)));
        assert_eq!(mapped.message, "operation exceeded time limit");
        let io = Error::from(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset"));
        assert_eq!(map_error(&io, &shared).class, MongoErrorClass::Network);
        let tls = Error::from(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer),
        ));
        let m = map_error(&tls, &shared);
        assert_eq!((m.class, m.code_name.as_deref()), (MongoErrorClass::Tls, Some(CERTIFICATE_CODE_NAME)));
        let alert = Error::from(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            rustls::Error::AlertReceived(rustls::AlertDescription::HandshakeFailure),
        ));
        let m = map_error(&alert, &shared);
        assert_eq!((m.class, m.code_name.clone()), (MongoErrorClass::Tls, None));
        // A server selection that fails gives the last fault of the monitor.
        *shared.monitor_fault.lock().unwrap() = Some(m);
        // The kinds `Authentication` and `ServerSelection` cannot be made outside the driver: the cases MS10
        // and MS13 run them against a server.
        assert_eq!(shared.monitor_fault.lock().unwrap().as_ref().map(|e| e.class), Some(MongoErrorClass::Tls));
    }
}
