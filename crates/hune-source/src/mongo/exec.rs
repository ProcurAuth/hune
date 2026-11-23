// SPDX-License-Identifier: MIT OR Apache-2.0
//! The run of one query (8b.5, 8b.6, 8b.7).
//!
//! The steps of 8b.5 after the admission (steps 1 and 2 belong to `source`):
//!
//! 3. The deadline starts: the effective `max_execution_time` in whole seconds plus 5 s (8b.7).
//! 4. [`build_request`] makes the [`ReadRequest`]. The database is the one of the query, or the first
//!    name of the key `databases`. For an `aggregate`, Hune adds the last stage `$limit` of `R + 1`,
//!    so the server stops after one extra row. For a `find`, the limit is the lower of the limit of
//!    the query and `R + 1`. The first batch has `min(1000, R + 1)` documents, and `maxTimeMS` is the
//!    effective `max_execution_time`. The attribution comment goes in the request (8b.11). The wire
//!    guard runs on the request, and the BSON size of the command is checked against `max_query_size`.
//! 5. The client sends the `aggregate` or the `find`, and then `getMore` until the cursor ends, or until
//!    a cap stops the query.
//! 6. Each document goes to the [`MongoFrameBuilder`] as its batch arrives. The builder counts the rows
//!    and the bytes and stops at a cap (8b.6). Hune never cuts a result.
//! 7. At the end of the cursor, the builder makes the frame (7.16c) and the client ends the session.
//!
//! # What ends a query
//!
//! A query that ends without error leaves no cursor. A query that stops with a cursor open (a cap, a
//! value that does not decode, the client deadline, or the client that left) gets one `killCursors`
//! for the cursor of its last batch. The kill is taken one at a time for each source, and the wait for
//! the turn and the kill share the time `connect_timeout` (8b.7). A lost kill is not an error. Then
//! the client ends the session and closes the link of the query (`finish`). A server error on `getMore`
//! means that the cursor is dead already, so the query sends no kill for it.
//!
//! The drop of the future of a query, while it is in flight, means that the client left (8b.7). The
//! [`Flight`] guard then starts a task that kills the cursor, ends the session and gives the permits
//! back. The permits stay in the task until the session is ended, so the source never holds more
//! links than its admission allows. A command that has no cursor yet goes on in the server until
//! `maxTimeMS`: the interface can do nothing about it.
//!
//! # No retry
//!
//! A query never gets a second send after Hune sent it. The interface gives no signal that a command
//! failed before its first byte left, so this module does not make the one retry that 8b.5 allows for
//! a reused connection: the driver module does it inside its pool.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use hune_core::frame::Frame;
use hune_core::ids::SourceId;
use hune_core::mongo::frame::{FrameCaps, MongoFrameBuilder};
use hune_core::mongo::{Document, MongoQuery, MongoValue, QueryBody, ReadPreference};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::{Instant, timeout, timeout_at};

use crate::admission::Permits;
use crate::error::{ErrorCtx, SourceError, exec_seconds};
use crate::mongo::attrib;
use crate::mongo::erased::ErasedClient;
use crate::mongo::errors;
use crate::mongo::guard;
use crate::mongo::iface::{CursorHandle, ReadOp, ReadRequest};
use crate::types::{EffectiveLimits, LogComment, Uuid};

/// The wait after the effective `max_execution_time` before the client deadline (8b.7).
pub(crate) const DEADLINE_MARGIN: Duration = Duration::from_secs(5);
/// The most documents of the first batch (8b.5).
pub(crate) const FIRST_BATCH_MAX: u64 = 1000;

/// The turn of the kills of one source (8b.7): one kill runs at a time.
pub(crate) struct Killer {
    lock: AsyncMutex<()>,
    /// The time for the wait of the turn and the kill together: `connect_timeout`.
    limit: Duration,
}

impl Killer {
    pub(crate) fn new(limit: Duration) -> Killer {
        Killer { lock: AsyncMutex::new(()), limit }
    }
}

/// The lists of the block that bound a query (7.33, 8b.5).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lists<'a> {
    pub databases: &'a [String],
    pub collections: &'a [String],
    pub read_preference: ReadPreference,
}

/// The parameters of one query.
pub(crate) struct Run<'a> {
    /// The id of the source, for the errors.
    pub source: &'a SourceId,
    /// The parsed and expanded query.
    pub query: &'a MongoQuery,
    /// The effective limits.
    pub limits: &'a EffectiveLimits,
    /// The query id.
    pub query_id: Uuid,
    /// The members of the attribution comment.
    pub log_comment: &'a LogComment,
    /// The lists of the block.
    pub lists: Lists<'a>,
    /// The time from the start to the client deadline.
    pub deadline: Duration,
}

/// Gives the time from the start to the client deadline: the whole seconds of `max_execution_time`
/// plus 5 s (8b.7).
pub(crate) fn client_deadline(limits: &EffectiveLimits) -> Duration {
    Duration::from_secs(exec_seconds(limits)) + DEADLINE_MARGIN
}

/// An integer of the model: an Int32 when it fits, else an Int64.
fn int(n: u64) -> MongoValue {
    i32::try_from(n).map_or_else(|_| MongoValue::Int64(i64::try_from(n).unwrap_or(i64::MAX)), MongoValue::Int32)
}

/// Gives the BSON size of a document: 4 bytes of length, each member, and the final 0. The function of
/// `hune-core` does the count, as the engine does for the check of 7.35, step 7.
pub(crate) fn bson_document_size(d: &Document) -> u64 {
    hune_core::mongo::bson_size(&MongoValue::Document(d.clone())) as u64
}

/// A fault of Hune that stops the query before it sends: `500 internal_error`. The text is fixed.
fn internal(message: &str) -> SourceError {
    errors::internal(message)
}

/// Makes the request of a query (steps 4 of the module comment). The request passed the wire guard
/// and the check of `max_query_size`.
///
/// # Errors
///
/// A database or a collection outside the lists, or a refusal of the guard: `internal_error`. A
/// command above `max_query_size`: `query_error` with `mongo_code` `null`.
pub(crate) fn build_request(run: &Run<'_>) -> Result<ReadRequest, SourceError> {
    let ectx = ErrorCtx { source: run.source, query_id: run.query_id, limits: run.limits };
    let rows = run.limits.max_result_rows;
    let overflow = rows.saturating_add(1);
    // The default database is the first name of the key `databases` (7.29).
    let database = match &run.query.database {
        Some(d) => d.clone(),
        None => {
            run.lists.databases.first().cloned().ok_or_else(|| internal("The source has no database in its list."))?
        }
    };
    let collection = run.query.collection.clone();
    let op = match &run.query.body {
        QueryBody::Aggregate(stages) => {
            let mut pipeline = stages.clone();
            pipeline.push(MongoValue::Document(vec![("$limit".to_owned(), int(overflow))]));
            ReadOp::Aggregate { collection, pipeline }
        }
        QueryBody::Find(f) => ReadOp::Find {
            collection,
            filter: MongoValue::Document(f.filter.clone()),
            projection: f.projection.clone().map(MongoValue::Document),
            sort: f.sort.clone().map(MongoValue::Document),
            skip: f.skip,
            limit: f.limit.map_or(overflow, |l| l.min(overflow)),
        },
    };
    let request = ReadRequest {
        database,
        op,
        batch_size: u32::try_from(overflow.min(FIRST_BATCH_MAX)).unwrap_or(1000),
        max_time_ms: exec_seconds(run.limits).saturating_mul(1000),
        read_preference: run.lists.read_preference,
        comment: attrib::comment_document(run.log_comment, &run.query_id),
    };
    // The second barrier: the wire guard. The driver module runs it again before the encode.
    guard::check_read_request(&request, run.lists.databases, run.lists.collections)
        .map_err(|e| errors::map_error(&ectx, &e, false))?;
    // The BSON size of the command (7.35, step 7). The engine checks it first.
    if let Some(doc) = request.to_command().to_document()
        && bson_document_size(&doc) > run.limits.max_query_size
    {
        return Err(errors::query_error(
            &ectx,
            None,
            &format!("The command is larger than the limit of {} bytes.", run.limits.max_query_size),
        ));
    }
    Ok(request)
}

/// The reason why the work of a query stopped.
struct Fail(SourceError);

/// Reads the batches and makes the frame (steps 5 to 7). The slot holds the cursor of the last batch,
/// so the caller can kill it after a stop.
async fn work(
    client: &dyn ErasedClient,
    run: &Run<'_>,
    ectx: &ErrorCtx<'_>,
    slot: &Mutex<Option<CursorHandle>>,
    sent: &AtomicBool,
    started: std::time::Instant,
) -> Result<Frame, Fail> {
    let set_slot = |c: Option<CursorHandle>| *slot.lock().unwrap_or_else(PoisonError::into_inner) = c;
    let request = build_request(run).map_err(Fail)?;
    // From here on, a command can be on the wire: a stop needs the cleanup.
    sent.store(true, Ordering::Release);
    let caps = FrameCaps { max_rows: run.limits.max_result_rows, max_bytes: run.limits.max_result_bytes };
    let mut builder = MongoFrameBuilder::new(caps);
    let mut batch = client.read(request).await.map_err(|e| Fail(errors::map_error(ectx, &e, false)))?;
    loop {
        // The cursor goes in the slot before the documents are read: a stop in the loop kills it.
        set_slot(batch.cursor.clone());
        for doc in &batch.docs {
            builder.push_document(doc).map_err(|f| Fail(errors::map_frame_fault(ectx, f)))?;
        }
        let Some(mut cursor) = batch.cursor.take() else { break };
        batch = match client.get_more(&mut cursor).await {
            Ok(b) => b,
            Err(e) => {
                // A server error on `getMore` means that the server closed the cursor.
                if e.class == crate::mongo::MongoErrorClass::Server {
                    set_slot(None);
                }
                return Err(Fail(errors::map_error(ectx, &e, false)));
            }
        };
    }
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    builder.finish(run.query_id.to_string(), elapsed_ms).map_err(|f| Fail(errors::map_frame_fault(ectx, f)))
}

/// Kills a cursor and ends the session, each within the time of the kill (8b.7). Nothing fails.
async fn cleanup(client: &dyn ErasedClient, killer: &Killer, cursor: Option<CursorHandle>) {
    if let Some(c) = cursor {
        // One kill runs at a time for each source. The wait for the turn and the kill share the time.
        let _ = timeout(killer.limit, async {
            let _turn = killer.lock.lock().await;
            client.kill_cursor(c).await;
        })
        .await;
    }
    let _ = timeout(killer.limit, client.finish(None)).await;
}

/// The client, the cursor slot and the permits of a query in flight. The drop of the guard, while it is
/// armed, means that the client left (8b.7): a task kills the cursor, ends the session and gives the
/// permits back.
pub(crate) struct Flight {
    client: Arc<dyn ErasedClient>,
    killer: Arc<Killer>,
    slot: Arc<Mutex<Option<CursorHandle>>>,
    permits: Option<Permits>,
    /// True when the query reached the point where a command can be on the wire.
    sent: Arc<AtomicBool>,
}

impl Flight {
    pub(crate) fn new(client: Arc<dyn ErasedClient>, killer: Arc<Killer>, permits: Permits) -> Flight {
        Flight {
            client,
            killer,
            slot: Arc::new(Mutex::new(None)),
            permits: Some(permits),
            sent: Arc::new(AtomicBool::new(false)),
        }
    }

    fn take_cursor(&self) -> Option<CursorHandle> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    /// Disarms the guard, after the cleanup ran to the end. The permits go back with the drop.
    fn disarm(mut self) {
        drop(self.permits.take());
    }
}

impl Drop for Flight {
    fn drop(&mut self) {
        // An armed guard: the future of the query was dropped.
        let Some(permits) = self.permits.take() else { return };
        let cursor = self.take_cursor();
        let client = Arc::clone(&self.client);
        let killer = Arc::clone(&self.killer);
        let sent = self.sent.load(Ordering::Acquire);
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    if sent {
                        cleanup(&*client, &killer, cursor).await;
                    }
                    drop(permits);
                });
            }
            // No runtime: the close of the link is the only means left.
            Err(_) => drop(permits),
        }
    }
}

/// Runs the steps 3 to 7 on `client`, then the cleanup of 8b.7, and gives the frame or the error.
///
/// The future is safe to drop: [`Flight`] kills the cursor, ends the session and gives the permits
/// back. The client deadline gives `query_timeout`, or `ceiling_exceeded` when the time limit came
/// from a ceiling.
pub(crate) async fn run(
    client: Arc<dyn ErasedClient>,
    killer: Arc<Killer>,
    permits: Permits,
    run: &Run<'_>,
) -> Result<Frame, SourceError> {
    let ectx = ErrorCtx { source: run.source, query_id: run.query_id, limits: run.limits };
    let flight = Flight::new(client, killer, permits);
    // Step 3.
    let started = std::time::Instant::now();
    let deadline = Instant::now() + run.deadline;
    let worked = timeout_at(deadline, work(&*flight.client, run, &ectx, &flight.slot, &flight.sent, started)).await;
    let result = match worked {
        Ok(Ok(frame)) => Ok(frame),
        Ok(Err(Fail(e))) => Err(e),
        Err(_) => Err(ectx.timeout()),
    };
    // After a good end, the slot is empty. After a stop, it holds the cursor that stays open.
    // A query that stopped before the first send has nothing to end.
    if flight.sent.load(Ordering::Acquire) {
        let cursor = flight.take_cursor();
        cleanup(&*flight.client, &flight.killer, cursor).await;
    }
    flight.disarm();
    result
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use hune_core::mongo::FindSpec;

    use super::*;
    use crate::admission::Admission;
    use crate::budget::ResultMemoryBudget;
    use crate::mongo::fake::{FakeMongo, Reply};
    use crate::mongo::iface::{MongoError, MongoErrorClass};
    use crate::types::LogKind;

    struct Fx {
        fake: FakeMongo,
        source: SourceId,
        databases: Vec<String>,
        collections: Vec<String>,
        limits: EffectiveLimits,
        killer: Arc<Killer>,
        admission: Admission,
        comment: LogComment,
    }

    fn fx() -> Fx {
        let fake = FakeMongo::new();
        fake.set_guard_lists(&["analytics"], &["c", "d"]);
        Fx {
            fake,
            source: SourceId::parse("events").unwrap(),
            databases: vec!["analytics".to_owned(), "logs".to_owned()],
            collections: vec!["c".to_owned(), "d".to_owned()],
            limits: EffectiveLimits {
                max_execution_time: Duration::from_secs(60),
                max_result_rows: 3,
                max_result_bytes: 1 << 20,
                max_query_size: 1 << 20,
                rows_from_ceiling: false,
                time_from_ceiling: false,
            },
            killer: Arc::new(Killer::new(Duration::from_secs(2))),
            admission: Admission::new(4, None, Duration::from_secs(1), ResultMemoryBudget::new(1 << 30), 100),
            comment: LogComment { kind: LogKind::Panel, panel: Some(2), ..LogComment::probe() },
        }
    }

    fn aggregate(stages: &[&str]) -> MongoQuery {
        MongoQuery {
            collection: "c".to_owned(),
            database: None,
            body: QueryBody::Aggregate(stages.iter().map(|s| MongoValue::from_extended_json(s).unwrap()).collect()),
        }
    }

    impl Fx {
        fn run_of<'a>(&'a self, query: &'a MongoQuery) -> Run<'a> {
            Run {
                source: &self.source,
                query,
                limits: &self.limits,
                query_id: Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").unwrap(),
                log_comment: &self.comment,
                lists: Lists {
                    databases: &self.databases,
                    collections: &self.collections,
                    read_preference: ReadPreference::SecondaryPreferred,
                },
                deadline: Duration::from_secs(65),
            }
        }

        async fn go(&self, query: &MongoQuery) -> Result<Frame, SourceError> {
            let permits = self.admission.acquire("-", self.admission.deadline()).await.unwrap();
            let client: Arc<dyn ErasedClient> = Arc::new(self.fake.clone());
            run(client, Arc::clone(&self.killer), permits, &self.run_of(query)).await
        }
    }

    fn json(f: &Frame) -> serde_json::Value {
        serde_json::from_str(&f.to_json()).unwrap()
    }

    // ---- the request ----

    #[test]
    fn the_request_adds_the_limit_of_r_plus_one_and_the_options() {
        let f = fx();
        let q = aggregate(&[r#"{"$match":{"a":1}}"#]);
        let r = build_request(&f.run_of(&q)).unwrap();
        assert_eq!(r.database, "analytics");
        assert_eq!(r.batch_size, 4);
        assert_eq!(r.max_time_ms, 60_000);
        assert_eq!(r.read_preference, ReadPreference::SecondaryPreferred);
        let ReadOp::Aggregate { collection, pipeline } = &r.op else { panic!("an aggregate") };
        assert_eq!(collection, "c");
        assert_eq!(pipeline.len(), 2);
        assert_eq!(pipeline[1], MongoValue::from_extended_json(r#"{"$limit":4}"#).unwrap());
        // The comment is the attribution document.
        assert_eq!(r.comment.get("app"), Some(&MongoValue::String("hune".to_owned())));
        assert_eq!(r.comment.get("panel"), Some(&MongoValue::Int32(2)));
        assert!(r.comment.get("query_id").is_some());
        // The command has the members of 8b.5, in order.
        let cmd = r.to_command().to_document().unwrap();
        let keys: Vec<&str> = cmd.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
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
    }

    #[test]
    fn the_first_batch_is_the_lower_of_1000_and_r_plus_one() {
        let mut f = fx();
        let q = aggregate(&[]);
        f.limits.max_result_rows = 100_000;
        assert_eq!(build_request(&f.run_of(&q)).unwrap().batch_size, 1000);
        f.limits.max_result_rows = 999;
        assert_eq!(build_request(&f.run_of(&q)).unwrap().batch_size, 1000);
        f.limits.max_result_rows = 998;
        assert_eq!(build_request(&f.run_of(&q)).unwrap().batch_size, 999);
        f.limits.max_result_rows = 1;
        assert_eq!(build_request(&f.run_of(&q)).unwrap().batch_size, 2);
        // A limit above 2^31 is an Int64 in the stage.
        f.limits.max_result_rows = 3_000_000_000;
        let r = build_request(&f.run_of(&q)).unwrap();
        let ReadOp::Aggregate { pipeline, .. } = &r.op else { panic!() };
        assert_eq!(pipeline[0].get("$limit"), Some(&MongoValue::Int64(3_000_000_001)));
    }

    #[test]
    fn max_time_ms_is_the_whole_seconds_of_the_effective_limit() {
        let mut f = fx();
        let q = aggregate(&[]);
        f.limits.max_execution_time = Duration::from_millis(1500);
        assert_eq!(build_request(&f.run_of(&q)).unwrap().max_time_ms, 2000);
        assert_eq!(client_deadline(&f.limits), Duration::from_secs(7));
        f.limits.max_execution_time = Duration::from_secs(30);
        assert_eq!(build_request(&f.run_of(&q)).unwrap().max_time_ms, 30_000);
        assert_eq!(client_deadline(&f.limits), Duration::from_secs(35));
    }

    #[test]
    fn a_find_has_the_lower_of_the_query_limit_and_r_plus_one() {
        let f = fx();
        let find = |limit: Option<u64>| MongoQuery {
            collection: "c".to_owned(),
            database: Some("logs".to_owned()),
            body: QueryBody::Find(FindSpec {
                filter: MongoValue::document_from_extended_json(r#"{"a":1}"#).unwrap(),
                projection: Some(MongoValue::document_from_extended_json(r#"{"a":1}"#).unwrap()),
                sort: Some(MongoValue::document_from_extended_json(r#"{"a":-1}"#).unwrap()),
                skip: 7,
                limit,
            }),
        };
        let limit_of = |q: &MongoQuery| {
            let r = build_request(&f.run_of(q)).unwrap();
            let ReadOp::Find { limit, skip, .. } = r.op else { panic!("a find") };
            assert_eq!(skip, 7);
            assert_eq!(r.database, "logs");
            limit
        };
        // R is 3: R + 1 is 4.
        assert_eq!(limit_of(&find(None)), 4);
        assert_eq!(limit_of(&find(Some(2))), 2);
        assert_eq!(limit_of(&find(Some(3))), 3);
        assert_eq!(limit_of(&find(Some(4))), 4);
        assert_eq!(limit_of(&find(Some(1_000_000))), 4);
    }

    #[test]
    fn the_default_database_is_the_first_of_the_list() {
        let f = fx();
        let q = aggregate(&[]);
        assert_eq!(build_request(&f.run_of(&q)).unwrap().database, "analytics");
        let mut q = aggregate(&[]);
        q.database = Some("logs".to_owned());
        assert_eq!(build_request(&f.run_of(&q)).unwrap().database, "logs");
    }

    #[test]
    fn a_name_outside_the_lists_and_a_refused_key_are_an_internal_error_before_any_send() {
        let f = fx();
        let mut q = aggregate(&[]);
        q.database = Some("other".to_owned());
        let e = build_request(&f.run_of(&q)).unwrap_err();
        assert_eq!((e.code(), e.http_status()), ("internal_error", 500));
        let mut q = aggregate(&[]);
        q.collection = "secret".to_owned();
        assert_eq!(build_request(&f.run_of(&q)).unwrap_err().code(), "internal_error");
        for stage in [r#"{"$out":"x"}"#, r#"{"$merge":{"into":"x"}}"#, r#"{"$function":{}}"#, r#"{"$where":"1"}"#] {
            let q = aggregate(&[stage]);
            let e = build_request(&f.run_of(&q)).unwrap_err();
            assert_eq!(e.code(), "internal_error", "{stage}");
            assert!(!e.message().contains('$'), "{}", e.message());
        }
        let q = aggregate(&[r#"{"$lookup":{"from":"secret","as":"x"}}"#]);
        assert_eq!(build_request(&f.run_of(&q)).unwrap_err().code(), "internal_error");
        let q = aggregate(&[r#"{"$lookup":{"from":"d","as":"x"}}"#]);
        assert!(build_request(&f.run_of(&q)).is_ok());
    }

    #[test]
    fn the_command_above_max_query_size_is_a_query_error_with_a_null_code() {
        let mut f = fx();
        let q = aggregate(&[r#"{"$match":{"a":"hello"}}"#]);
        let size = bson_document_size(&build_request(&f.run_of(&q)).unwrap().to_command().to_document().unwrap());
        f.limits.max_query_size = size;
        assert!(build_request(&f.run_of(&q)).is_ok());
        f.limits.max_query_size = size - 1;
        let e = build_request(&f.run_of(&q)).unwrap_err();
        assert_eq!((e.code(), e.http_status(), e.mongo_code()), ("query_error", 422, Some(None)));
    }

    #[test]
    fn the_bson_size_follows_the_format() {
        // {} is 5 bytes. {"a": 1} is 5 + 1 + 2 + 4 = 12.
        assert_eq!(bson_document_size(&vec![]), 5);
        assert_eq!(bson_document_size(&vec![("a".to_owned(), MongoValue::Int32(1))]), 12);
        // {"s": "ab"}: 5 + (1 + 2 + 4 + 3) = 15.
        assert_eq!(bson_document_size(&vec![("s".to_owned(), MongoValue::String("ab".to_owned()))]), 15);
        // {"x": [1, 2]}: the array is 5 + 2 * (1 + 2 + 4) = 19. The document is 5 + (1 + 2 + 19) = 27.
        let arr = MongoValue::Array(vec![MongoValue::Int32(1), MongoValue::Int32(2)]);
        assert_eq!(bson_document_size(&vec![("x".to_owned(), arr)]), 27);
        assert_eq!(bson_document_size(&vec![("n".to_owned(), MongoValue::Null)]), 8);
    }

    // ---- the run ----

    #[tokio::test]
    async fn one_batch_gives_a_frame_and_ends_the_session() {
        let f = fx();
        f.fake.push_read(Reply::docs(&[r#"{"n":1}"#, r#"{"n":2}"#]));
        let frame = f.go(&aggregate(&[r#"{"$match":{}}"#])).await.unwrap();
        assert_eq!(frame.rows, 2);
        assert_eq!(json(&frame)["values"], serde_json::json!([[1, 2]]));
        assert_eq!(frame.query_id, "019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d");
        assert_eq!((frame.stats.read_rows, frame.stats.read_bytes), (None, None));
        assert!(frame.stats.result_bytes.is_some());
        assert_eq!(f.fake.command_names(), ["aggregate", "endSessions"]);
        assert!(f.fake.killed().is_empty());
        assert_eq!(f.fake.finished(), vec![None]);
        assert_eq!(f.admission.stats().in_flight, 0);
    }

    #[tokio::test]
    async fn getmore_runs_until_the_cursor_ends() {
        let mut f = fx();
        f.limits.max_result_rows = 100;
        f.fake.push_read(Reply::docs_with_cursor(&[r#"{"n":1}"#], 9));
        f.fake.push_get_more(Reply::docs_with_cursor(&[r#"{"n":2}"#], 9));
        f.fake.push_get_more(Reply::docs(&[r#"{"n":3}"#]));
        let frame = f.go(&aggregate(&[])).await.unwrap();
        assert_eq!(json(&frame)["values"], serde_json::json!([[1, 2, 3]]));
        assert_eq!(f.fake.command_names(), ["aggregate", "getMore", "getMore", "endSessions"]);
        assert!(f.fake.killed().is_empty(), "the cursor ended: no kill");
    }

    #[tokio::test]
    async fn a_result_of_exactly_r_rows_passes_and_one_more_is_the_overflow() {
        let f = fx();
        f.fake.push_read(Reply::docs(&[r#"{"n":1}"#, r#"{"n":2}"#, r#"{"n":3}"#]));
        assert_eq!(f.go(&aggregate(&[])).await.unwrap().rows, 3);
        // R is 3: the server returns R + 1 documents at most.
        f.fake.push_read(Reply::docs(&[r#"{"n":1}"#, r#"{"n":2}"#, r#"{"n":3}"#, r#"{"n":4}"#]));
        let e = f.go(&aggregate(&[])).await.unwrap_err();
        assert_eq!(
            (e.code(), e.http_status(), e.limit_name(), e.limit_value()),
            ("query_limit", 422, Some("max_result_rows"), Some(3))
        );
    }

    #[tokio::test]
    async fn the_row_overflow_in_a_later_batch_kills_the_cursor_and_ends_the_session() {
        let f = fx();
        f.fake.push_read(Reply::docs_with_cursor(&[r#"{"n":1}"#, r#"{"n":2}"#], 11));
        f.fake.push_get_more(Reply::docs_with_cursor(&[r#"{"n":3}"#, r#"{"n":4}"#], 11));
        let e = f.go(&aggregate(&[])).await.unwrap_err();
        assert_eq!(e.code(), "query_limit");
        assert_eq!(f.fake.killed(), vec![11]);
        assert_eq!(f.fake.command_names(), ["aggregate", "getMore", "killCursors", "endSessions"]);
        // The overflow with a ceiling is `ceiling_exceeded`.
        let mut g = fx();
        g.limits.rows_from_ceiling = true;
        g.fake.push_read(Reply::docs(&[r#"{"n":1}"#, r#"{"n":2}"#, r#"{"n":3}"#, r#"{"n":4}"#]));
        let e = g.go(&aggregate(&[])).await.unwrap_err();
        assert_eq!(
            (e.code(), e.http_status(), e.ceiling(), e.ceiling_limit()),
            ("ceiling_exceeded", 403, Some("max_rows"), Some(3))
        );
    }

    #[tokio::test]
    async fn the_byte_cap_stops_at_the_first_document_above_it_and_the_client_reads_no_second_document() {
        let mut f = fx();
        f.limits.max_result_rows = 100;
        f.limits.max_result_bytes = 1000;
        let big = format!(r#"{{"s":"{}"}}"#, "x".repeat(5000));
        f.fake.push_read(Reply::docs_with_cursor(&[&big, r#"{"s":"y"}"#], 5));
        f.fake.push_get_more(Reply::docs(&[r#"{"s":"z"}"#]));
        let e = f.go(&aggregate(&[])).await.unwrap_err();
        assert_eq!(
            (e.code(), e.http_status(), e.limit_name(), e.limit_value()),
            ("query_limit", 422, Some("max_result_bytes"), Some(1000))
        );
        assert_eq!(f.fake.command_names(), ["aggregate", "killCursors", "endSessions"]);
        assert_eq!(f.fake.pending_replies(), (0, 1), "no getMore went out");
        assert_eq!(f.fake.killed(), vec![5]);
        // The byte cap never becomes a ceiling.
        let mut g = fx();
        g.limits.rows_from_ceiling = true;
        g.limits.max_result_rows = 100;
        g.limits.max_result_bytes = 10;
        g.fake.push_read(Reply::docs(&[r#"{"s":"xxxxxxxxxxxxxxxxxxxx"}"#]));
        assert_eq!(g.go(&aggregate(&[])).await.unwrap_err().code(), "query_limit");
    }

    #[tokio::test]
    async fn an_empty_result_is_a_frame_with_no_column() {
        let f = fx();
        f.fake.push_read(Reply::docs(&[]));
        let frame = f.go(&aggregate(&[])).await.unwrap();
        assert!(frame.columns.is_empty());
        assert_eq!(frame.rows, 0);
        let v = json(&frame);
        assert_eq!((&v["columns"], &v["values"]), (&serde_json::json!([]), &serde_json::json!([])));
    }

    #[tokio::test]
    async fn a_column_with_a_mix_that_the_table_refuses_is_a_query_error_and_kills_the_cursor() {
        let mut f = fx();
        f.limits.max_result_rows = 100;
        f.fake.push_read(Reply::docs_with_cursor(&[r#"{"a":1}"#, r#"{"a":"x"}"#], 21));
        let e = f.go(&aggregate(&[])).await.unwrap_err();
        assert_eq!((e.code(), e.http_status(), e.mongo_code()), ("query_error", 422, Some(None)));
        assert!(e.message().contains("\"a\""));
        assert_eq!(f.fake.killed(), vec![21]);
    }

    #[tokio::test]
    async fn a_server_error_on_the_first_read_is_mapped_by_its_code() {
        for (code, expected) in [
            (50, "query_timeout"),
            (292, "query_memory_limit"),
            (13, "source_access_denied"),
            (18, "source_misconfigured"),
            (91, "source_unavailable"),
            (168, "query_error"),
        ] {
            let f = fx();
            f.fake.push_read(Reply::error(MongoError::server(code, "Name", "text")));
            assert_eq!(f.go(&aggregate(&[])).await.unwrap_err().code(), expected, "{code}");
            assert!(f.fake.killed().is_empty());
            assert_eq!(f.fake.finished().len(), 1, "the session ends after an error");
        }
        let f = fx();
        f.fake.push_read(Reply::error(MongoError::new(MongoErrorClass::Network, "reset")));
        let e = f.go(&aggregate(&[])).await.unwrap_err();
        assert_eq!((e.code(), e.retryable()), ("source_unavailable", true));
        assert_eq!(f.fake.command_names(), ["aggregate", "endSessions"], "no second send");
    }

    #[tokio::test]
    async fn a_server_error_on_getmore_sends_no_kill_but_another_fault_does() {
        let f = fx();
        f.fake.push_read(Reply::docs_with_cursor(&[r#"{"n":1}"#], 31));
        f.fake.push_get_more(Reply::error(MongoError::server(43, "CursorNotFound", "gone")));
        let e = f.go(&aggregate(&[])).await.unwrap_err();
        assert_eq!((e.code(), e.mongo_code()), ("query_error", Some(Some(43))));
        assert!(f.fake.killed().is_empty());
        let f = fx();
        f.fake.push_read(Reply::docs_with_cursor(&[r#"{"n":1}"#], 32));
        f.fake.push_get_more(Reply::error(MongoError::new(MongoErrorClass::Network, "reset")));
        assert_eq!(f.go(&aggregate(&[])).await.unwrap_err().code(), "source_unavailable");
        assert_eq!(f.fake.killed(), vec![32]);
    }

    #[tokio::test]
    async fn the_client_deadline_kills_the_cursor_and_gives_query_timeout() {
        let mut f = fx();
        f.limits.max_result_rows = 100;
        f.fake.push_read(Reply::docs_with_cursor(&[r#"{"n":1}"#], 41));
        f.fake.push_get_more(Reply::docs(&[r#"{"n":2}"#]).after(Duration::from_secs(10)));
        let q = aggregate(&[]);
        let mut r = f.run_of(&q);
        r.deadline = Duration::from_millis(150);
        let permits = f.admission.acquire("-", f.admission.deadline()).await.unwrap();
        let client: Arc<dyn ErasedClient> = Arc::new(f.fake.clone());
        let start = std::time::Instant::now();
        let e = run(client, Arc::clone(&f.killer), permits, &r).await.unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(3));
        assert_eq!((e.code(), e.http_status()), ("query_timeout", 504));
        assert_eq!(f.fake.killed(), vec![41]);
        assert_eq!(f.fake.command_names(), ["aggregate", "getMore", "killCursors", "endSessions"]);
        assert_eq!(f.admission.stats().in_flight, 0);
        // With a ceiling, the code is `ceiling_exceeded`.
        let mut g = fx();
        g.limits.time_from_ceiling = true;
        g.fake.push_read(Reply::docs(&[]).after(Duration::from_secs(10)));
        let q = aggregate(&[]);
        let mut r = g.run_of(&q);
        r.deadline = Duration::from_millis(100);
        let permits = g.admission.acquire("-", g.admission.deadline()).await.unwrap();
        let client: Arc<dyn ErasedClient> = Arc::new(g.fake.clone());
        let e = run(client, Arc::clone(&g.killer), permits, &r).await.unwrap_err();
        assert_eq!((e.code(), e.ceiling(), e.ceiling_limit()), ("ceiling_exceeded", Some("max_duration"), Some(60)));
        // No cursor yet: no kill, and the session still ends.
        assert!(g.fake.killed().is_empty());
        assert_eq!(g.fake.finished().len(), 1);
    }

    #[tokio::test]
    async fn a_client_that_leaves_gets_a_kill_the_end_of_the_session_and_the_permits_back() {
        let mut f = fx();
        f.limits.max_result_rows = 100;
        f.fake.push_read(Reply::docs_with_cursor(&[r#"{"n":1}"#], 51));
        f.fake.push_get_more(Reply::docs(&[]).after(Duration::from_secs(30)));
        let in_flight_before = f.admission.stats().in_flight;
        let permits = f.admission.acquire("-", f.admission.deadline()).await.unwrap();
        assert_eq!(f.admission.stats().in_flight, in_flight_before + 1);
        let q = aggregate(&[]);
        let client: Arc<dyn ErasedClient> = Arc::new(f.fake.clone());
        let killer = Arc::clone(&f.killer);
        let r = f.run_of(&q);
        // Poll the query until the getMore is pending, then drop the future: the client left.
        let fut = run(client, killer, permits, &r);
        let mut fut = Box::pin(fut);
        let waiting = tokio::time::timeout(Duration::from_millis(100), &mut fut).await;
        assert!(waiting.is_err(), "the query waits for the getMore");
        drop(fut);
        for _ in 0..100 {
            if f.admission.stats().in_flight == in_flight_before && !f.fake.finished().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(f.fake.killed(), vec![51]);
        assert_eq!(f.fake.finished(), vec![None]);
        assert_eq!(f.admission.stats().in_flight, in_flight_before, "the permits are back");
        assert_eq!(f.fake.command_names(), ["aggregate", "getMore", "killCursors", "endSessions"]);
    }

    #[tokio::test]
    async fn a_client_that_leaves_before_the_first_reply_ends_the_session_and_sends_no_kill() {
        let f = fx();
        f.fake.push_read(Reply::docs(&[]).after(Duration::from_secs(30)));
        let permits = f.admission.acquire("-", f.admission.deadline()).await.unwrap();
        let q = aggregate(&[]);
        let client: Arc<dyn ErasedClient> = Arc::new(f.fake.clone());
        let r = f.run_of(&q);
        let mut fut = Box::pin(run(client, Arc::clone(&f.killer), permits, &r));
        assert!(tokio::time::timeout(Duration::from_millis(50), &mut fut).await.is_err());
        drop(fut);
        for _ in 0..100 {
            if !f.fake.finished().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(f.fake.killed().is_empty());
        assert_eq!(f.fake.finished(), vec![None]);
        assert_eq!(f.admission.stats().in_flight, 0);
    }

    #[tokio::test]
    async fn one_kill_runs_at_a_time_for_each_source() {
        // A client whose kill takes some time and counts the kills that overlap.
        struct Slow {
            active: AtomicUsize,
            overlap: AtomicUsize,
            kills: AtomicUsize,
        }
        impl crate::mongo::MongoClient for Slow {
            async fn connect(_: crate::mongo::ConnectOptions) -> Result<Slow, MongoError> {
                Ok(Slow { active: AtomicUsize::new(0), overlap: AtomicUsize::new(0), kills: AtomicUsize::new(0) })
            }
            async fn server_info(&self) -> Result<crate::mongo::ServerInfo, MongoError> {
                Err(MongoError::new(MongoErrorClass::Network, "x"))
            }
            async fn connection_status(&self) -> Result<crate::mongo::AuthInfo, MongoError> {
                Err(MongoError::new(MongoErrorClass::Network, "x"))
            }
            async fn list_collections(&self, _: &str) -> Result<Vec<String>, MongoError> {
                Ok(vec![])
            }
            async fn read(&self, _: ReadRequest) -> Result<crate::mongo::Batch, MongoError> {
                Ok(crate::mongo::Batch::default())
            }
            async fn get_more(&self, _: &mut CursorHandle) -> Result<crate::mongo::Batch, MongoError> {
                Ok(crate::mongo::Batch::default())
            }
            async fn kill_cursor(&self, _: CursorHandle) {
                if self.active.fetch_add(1, Ordering::SeqCst) > 0 {
                    self.overlap.fetch_add(1, Ordering::SeqCst);
                }
                tokio::time::sleep(Duration::from_millis(40)).await;
                self.active.fetch_sub(1, Ordering::SeqCst);
                self.kills.fetch_add(1, Ordering::SeqCst);
            }
            async fn finish(&self, _: Option<CursorHandle>) {}
        }
        let slow =
            Arc::new(Slow { active: AtomicUsize::new(0), overlap: AtomicUsize::new(0), kills: AtomicUsize::new(0) });
        let killer = Arc::new(Killer::new(Duration::from_secs(2)));
        let handle = |id: i64| CursorHandle {
            id,
            database: "analytics".to_owned(),
            collection: "c".to_owned(),
            read_preference: ReadPreference::Primary,
        };
        let mut tasks = Vec::new();
        for id in 1..=4 {
            let (client, killer) = (Arc::clone(&slow), Arc::clone(&killer));
            tasks.push(tokio::spawn(async move {
                let client: Arc<dyn ErasedClient> = client;
                cleanup(&*client, &killer, Some(handle(id))).await;
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        assert_eq!(slow.kills.load(Ordering::SeqCst), 4);
        assert_eq!(slow.overlap.load(Ordering::SeqCst), 0, "two kills ran at the same time");
        // A kill that waits for the turn longer than the limit is dropped, and nothing fails.
        let short = Arc::new(Killer::new(Duration::from_millis(20)));
        let client: Arc<dyn ErasedClient> = slow.clone();
        cleanup(&*client, &short, Some(handle(9))).await;
        assert_eq!(slow.kills.load(Ordering::SeqCst), 4, "the kill passed the time limit");
    }

    #[tokio::test]
    async fn a_frame_keeps_no_connection_to_the_permits() {
        let f = fx();
        f.fake.push_read(Reply::docs(&[r#"{"n":1}"#]));
        let frame = f.go(&aggregate(&[])).await.unwrap();
        assert_eq!(f.admission.stats().in_flight, 0);
        assert_eq!(frame.columns[0].name, "n");
    }
}
