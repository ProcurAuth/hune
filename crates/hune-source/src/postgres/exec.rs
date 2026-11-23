//! The run of one query (8a.5, 8a.6, 8a.7, ID-PG9).
//!
//! The steps of 8a.5 after the admission and the session (steps 1 and 2 belong to `source`):
//!
//! 3. The deadline starts: the effective `max_execution_time` in whole seconds plus 5 s (8a.7).
//! 4. `START TRANSACTION READ ONLY` (see below).
//! 5. The `set_config` statement with the bound values of `fixed::set_query_values`.
//! 6. Parse and Describe of the comment and the SQL. `classify_columns` refuses a type that 7.16a
//!    does not list (`422 query_error`, `sqlstate` null) before the portal runs.
//! 7. Bind, with no parameter: one portal.
//! 8. Execute again and again. Each Execute asks for `min(1000, R + 1)` rows. `R` is what the
//!    effective `max_result_rows` still permits, so the row after the last permitted row is the
//!    overflow and the server never sends more than `R + 1` rows.
//! 9. Each row is read as it comes (one row at a time), decoded and counted by the
//!    `PgFrameBuilder`. A row above a cap stops the query. Hune never cuts a result.
//! 10. `ROLLBACK`, then `DISCARD ALL` within 2 s. Then `clear_type_cache`, because `DISCARD ALL`
//!     removes the statements that the client crate cached for the lookup of types.
//!
//! # The size of a message
//!
//! The stream of the session fails when the server sends one message longer than `max_result_bytes` plus
//! 1 MiB (`bounded`, finding M4). The client crate then has not buffered the body. The failed stream is
//! the byte limit of 8a.6 (`422 query_limit`), and the session is closed after a cancel request.
//!
//! # What closes the session
//!
//! The caller reuses a session in one case only: an ErrorResponse of the server, or a stop by
//! Hune before the portal runs, and the end of the portal, with `ROLLBACK` and `DISCARD ALL`
//! both done within 2 s. Each other case closes it: a cap, the deadline, a transport fault, a
//! value that does not decode. For a cap and the deadline this module first sends the cancel
//! request, so the server stops the work (8a.6, 8a.7). The drop of the whole future (the client
//! left) is the job of [`InFlight`].
//!
//! # Where the text of 8a.5 differs
//!
//! The client crate has no way to open the portal after `BEGIN READ ONLY` of the simple protocol.
//! `Transaction` is the only type that can bind a portal, and it opens its transaction with
//! `START TRANSACTION READ ONLY`. This text has the same effect as `BEGIN READ ONLY`.
//!
//! # No second statement
//!
//! The SQL goes in one Parse message. The server refuses a text that holds two statements with
//! the SQLSTATE 42601. Nothing in this module sends the text of a caller by the simple protocol.

use std::future::poll_fn;
use std::pin::pin;
use std::sync::atomic::Ordering;
use std::time::Duration;

use futures_core::Stream;
use hune_core::frame::Frame;
use hune_core::ids::SourceId;
use tokio::time::{Instant, timeout_at};
use tokio_postgres::Transaction;

use crate::admission::Permits;
use crate::error::{ErrorCtx, SourceError, exec_seconds};
use crate::postgres::bounded::Oversize;
use crate::postgres::cancel::Canceller;
use crate::postgres::errors::{self, Class};
use crate::postgres::frame_build::{Caps, Fault, PgFrameBuilder, classify_columns};
use crate::postgres::session::Session;
use crate::postgres::{attrib, fixed};
use crate::types::{EffectiveLimits, LogComment, LogKind, Uuid};

/// The wait after the effective `max_execution_time` before the client deadline (8a.7).
pub(crate) const DEADLINE_MARGIN: Duration = Duration::from_secs(5);
/// The time that `ROLLBACK` and `DISCARD ALL` have together (8a.5).
pub(crate) const CLEANUP_LIMIT: Duration = Duration::from_secs(2);
/// The most rows that one Execute asks for (8a.5).
pub(crate) const FETCH_ROWS: u64 = 1000;

/// The parameters of one query.
pub(crate) struct Run<'a> {
    /// The id of the source, for the errors.
    pub source: &'a SourceId,
    /// The final SQL, without the attribution comment.
    pub sql: &'a str,
    /// The effective limits.
    pub limits: &'a EffectiveLimits,
    /// The kind of the query, for `application_name`.
    pub kind: LogKind,
    /// The query id.
    pub query_id: Uuid,
    /// The members of the attribution comment.
    pub log_comment: &'a LogComment,
}

/// The end of one run.
pub(crate) struct Outcome {
    /// The frame, or the error of 8a.8.
    pub result: Result<Frame, SourceError>,
    /// True when the session is clean and can go back to the pool.
    pub keep: bool,
    /// True when the start of the transaction failed by a transport fault: a reused session
    /// that gives this can be replaced by a new one, one time (8a.5, step 4).
    pub begin_fault: bool,
}

impl Outcome {
    fn closed(result: Result<Frame, SourceError>) -> Outcome {
        Outcome { result, keep: false, begin_fault: false }
    }
}

/// The reason why the work of a query stopped.
enum Fail {
    /// An ErrorResponse of the server, or a stop by Hune before the portal runs. The transaction
    /// can be rolled back and the session reused.
    Clean(SourceError),
    /// A cap, a value that does not decode, or a transport fault. The session is closed after a
    /// cancel request.
    Stop(SourceError),
}

/// Maps an error of the client crate. An ErrorResponse leaves the session clean. Another fault does not.
fn server_fail(error: &tokio_postgres::Error, ectx: &ErrorCtx<'_>) -> Fail {
    let mapped = errors::map_error(ectx, error, false);
    if error.as_db_error().is_some() { Fail::Clean(mapped) } else { Fail::Stop(mapped) }
}

/// Maps an error of the client crate inside the work of a query. A stream that failed because the server sent
/// a message longer than the limit (`bounded`) is the byte limit of 8a.6, and the session is closed
/// (finding M4). Each other error goes through [`server_fail`].
fn work_fail(error: &tokio_postgres::Error, ectx: &ErrorCtx<'_>, oversize: &Oversize) -> Fail {
    if oversize.load(Ordering::Acquire) {
        return Fail::Stop(ectx.bytes());
    }
    server_fail(error, ectx)
}

/// Tells if an error at the start of the transaction is a transport fault: the connection is
/// closed or broken, or the server ended it (class 08, 57P01 to 57P03, 53300, and so on).
pub(crate) fn is_transport(error: &tokio_postgres::Error) -> bool {
    error.is_closed()
        || match error.as_db_error() {
            None => true,
            Some(db) => errors::classify(db.code().code()) == Class::Unavailable,
        }
}

fn row_fault(fault: Fault, ectx: &ErrorCtx<'_>) -> Fail {
    Fail::Stop(match fault {
        Fault::Rows => ectx.rows(),
        Fault::Bytes => ectx.bytes(),
        Fault::BadResponse(message) => ectx.bad_response(message),
    })
}

/// The number of rows that one Execute asks for: `min(1000, R + 1)`, with `R` the rows that the limit
/// still permits (8a.5, step 8).
fn fetch_size(max_rows: u64, done_rows: u64) -> i32 {
    let ask = max_rows.saturating_sub(done_rows).saturating_add(1).min(FETCH_ROWS);
    i32::try_from(ask).unwrap_or(1000)
}

/// The statement has no result column and the server ran it with no error.
fn no_column(ectx: &ErrorCtx<'_>) -> SourceError {
    SourceError::QuerySqlError {
        source: ectx.source.clone(),
        sqlstate: None,
        query_id: ectx.query_id,
        message: "The statement gives no column.".to_owned(),
    }
}

/// Runs the steps 5 to 9 inside the transaction and gives the frame.
async fn work(
    tx: &Transaction<'_>,
    run: &Run<'_>,
    ectx: &ErrorCtx<'_>,
    started: std::time::Instant,
    oversize: &Oversize,
) -> Result<Frame, Fail> {
    // Step 5.
    fixed::set_query_values(tx.client(), Duration::from_secs(exec_seconds(run.limits)), run.kind, &run.query_id)
        .await
        .map_err(|e| work_fail(&e, ectx, oversize))?;

    // Step 6.
    let text = attrib::commented_sql(run.log_comment, &run.query_id, run.sql);
    let statement = tx.prepare(&text).await.map_err(|e| work_fail(&e, ectx, oversize))?;
    let columns = classify_columns(statement.columns())
        .map_err(|u| Fail::Clean(errors::unsupported_type(ectx, &u.column, &u.type_name)))?;
    // A statement with a parameter cannot run: the source sends none. `bind` would panic.
    if !statement.params().is_empty() {
        return Err(Fail::Clean(SourceError::QuerySqlError {
            source: ectx.source.clone(),
            sqlstate: None,
            query_id: ectx.query_id,
            message: "The statement has parameters, and the source sends none.".to_owned(),
        }));
    }
    let mut builder = if columns.is_empty() {
        None
    } else {
        let caps = Caps { max_rows: run.limits.max_result_rows, max_bytes: run.limits.max_result_bytes };
        Some(PgFrameBuilder::new(columns, caps).map_err(|f| row_fault(f, ectx))?)
    };

    // Step 7.
    let portal = tx.bind(&statement, &[]).await.map_err(|e| work_fail(&e, ectx, oversize))?;

    // Steps 8 and 9.
    loop {
        let done_rows = builder.as_ref().map_or(0, PgFrameBuilder::rows);
        let ask = fetch_size(run.limits.max_result_rows, done_rows);
        let stream = tx.query_portal_raw(&portal, ask).await.map_err(|e| work_fail(&e, ectx, oversize))?;
        let mut stream = pin!(stream);
        let mut got = 0u64;
        while let Some(item) = poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
            let row = item.map_err(|e| work_fail(&e, ectx, oversize))?;
            got += 1;
            // A statement with no column gives no row: a row here is a protocol fault.
            let Some(b) = builder.as_mut() else {
                return Err(Fail::Stop(ectx.bad_response("A statement with no column gave a row.")));
            };
            b.push_pg_row(&row).map_err(|f| row_fault(f, ectx))?;
        }
        // CommandComplete ends the portal. A suspended portal has no command tag. An Execute
        // that gives no row ends the loop in each case.
        if stream.rows_affected().is_some() || got == 0 {
            break;
        }
    }
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    match builder {
        Some(b) => Ok(b.finish(run.query_id.to_string(), elapsed_ms)),
        None => Err(Fail::Clean(no_column(ectx))),
    }
}

/// What the steps gave, before the cleanup and the cancel.
enum Step {
    Done(Result<Frame, SourceError>, bool),
    Stopped(SourceError),
    BeginFault(SourceError),
}

async fn steps(session: &mut Session, run: &Run<'_>, ectx: &ErrorCtx<'_>, started: std::time::Instant) -> Step {
    let oversize = session.oversize_flag();
    // Step 4.
    let tx = match session.client.build_transaction().read_only(true).start().await {
        Ok(tx) => tx,
        Err(e) => {
            let mapped = errors::map_error(ectx, &e, false);
            return if is_transport(&e) { Step::BeginFault(mapped) } else { Step::Stopped(mapped) };
        }
    };
    let worked = work(&tx, run, ectx, started, &oversize).await;
    let (result, clean) = match worked {
        Ok(frame) => (Ok(frame), true),
        Err(Fail::Clean(e)) => (Err(e), true),
        Err(Fail::Stop(e)) => {
            // The drop of `tx` sends a ROLLBACK that nobody waits for. The caller cancels and closes.
            drop(tx);
            return Step::Stopped(e);
        }
    };
    // Step 10. `ROLLBACK` then `DISCARD ALL`, within 2 s together.
    let limit = Instant::now() + CLEANUP_LIMIT;
    let rolled = matches!(timeout_at(limit, tx.rollback()).await, Ok(Ok(())));
    let discarded = rolled && matches!(timeout_at(limit, fixed::discard_all(&session.client)).await, Ok(Ok(())));
    if discarded {
        // `DISCARD ALL` removed the statements that the client crate cached for the lookup of types.
        session.client.clear_type_cache();
    }
    Step::Done(result, clean && discarded)
}

/// Runs the steps 3 to 10 on `session`.
///
/// At the deadline, at a cap and at a transport fault the function sends the cancel request, and the
/// caller closes the session. The future is safe to drop: [`InFlight`] cancels and closes then.
pub(crate) async fn run(session: &mut Session, run: &Run<'_>) -> Outcome {
    let ectx = ErrorCtx { source: run.source, query_id: run.query_id, limits: run.limits };
    let canceller: Canceller = session.canceller();
    // Step 3.
    let started = std::time::Instant::now();
    let deadline = Instant::now() + Duration::from_secs(exec_seconds(run.limits)) + DEADLINE_MARGIN;
    match timeout_at(deadline, steps(session, run, &ectx, started)).await {
        Ok(Step::Done(result, keep)) => Outcome { result, keep, begin_fault: false },
        Ok(Step::BeginFault(e)) => Outcome { result: Err(e), keep: false, begin_fault: true },
        Ok(Step::Stopped(e)) => {
            canceller.send().await;
            Outcome::closed(Err(e))
        }
        Err(_) => {
            canceller.send().await;
            Outcome::closed(Err(ectx.timeout()))
        }
    }
}

/// The session and the permits of a query in flight. The drop of the guard, while it is armed, means
/// that the caller left (8a.7): a task sends the cancel request, closes the session and gives the
/// permits back. The permits stay in the task until the session is closed, so the source never holds
/// more sessions than its admission allows.
pub(crate) struct InFlight {
    session: Option<Session>,
    permits: Option<Permits>,
}

impl InFlight {
    pub(crate) fn new(session: Session, permits: Permits) -> InFlight {
        InFlight { session: Some(session), permits: Some(permits) }
    }

    /// Gives the session for the run.
    ///
    /// # Panics
    ///
    /// The guard was finished before. The code never does this.
    pub(crate) fn session_mut(&mut self) -> &mut Session {
        self.session.as_mut().expect("the guard holds its session until it is finished")
    }

    /// Disarms the guard and gives the session and the permits back to the caller.
    pub(crate) fn finish(mut self) -> (Session, Permits) {
        let session = self.session.take().expect("the guard holds its session until it is finished");
        let permits = self.permits.take().expect("the guard holds its permits until it is finished");
        (session, permits)
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        let (Some(session), permits) = (self.session.take(), self.permits.take()) else { return };
        let canceller = session.canceller();
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    canceller.send().await;
                    drop(session);
                    drop(permits);
                });
            }
            // No runtime: the close of the connection is the only means left.
            Err(_) => drop((session, permits)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constants_are_the_values_of_8a5_and_8a7() {
        assert_eq!(DEADLINE_MARGIN, Duration::from_secs(5));
        assert_eq!(CLEANUP_LIMIT, Duration::from_secs(2));
        assert_eq!(FETCH_ROWS, 1000);
    }

    fn ask(max_rows: u64, done: u64) -> i32 {
        fetch_size(max_rows, done)
    }

    #[test]
    fn the_fetch_size_asks_for_one_row_more_than_the_limit_permits() {
        assert_eq!(ask(100_000, 0), 1000);
        assert_eq!(ask(5, 0), 6);
        assert_eq!(ask(1500, 1000), 501);
        assert_eq!(ask(1000, 1000), 1);
        assert_eq!(ask(u64::MAX, 0), 1000);
    }
}
