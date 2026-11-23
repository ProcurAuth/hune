//! The fixed texts that Hune sends itself (8a.5, 8a.3 rule 8, ID-PG8).
//!
//! This is the only module that may name a function of the simple protocol (`simple_query`,
//! `batch_execute`, `copy_in`, `copy_out`). A policy check greps for these names in the other
//! files (16.18). The module holds the constants: `BEGIN READ ONLY`, the one `set_config`
//! statement, `ROLLBACK` and `DISCARD ALL`. The probe statements live in `probe_sql`.
//!
//! No function of this module takes the text of a caller. The functions that send a text take a
//! client and numbers, or a kind and a query id. The values of the `set_config` statement go as
//! bound parameters (extended protocol), never as part of the text.

#![allow(dead_code)] // The client of the next wave calls these items.

use std::time::Duration;

use tokio_postgres::Client;

use crate::postgres::attrib;
use crate::types::{LogKind, Uuid};

/// The start of each query and of each probe (8a.5).
pub(crate) const BEGIN_READ_ONLY: &str = "BEGIN READ ONLY";

/// The values of one query: three text parameters (8a.5). The third value is `application_name`.
pub(crate) const SET_CONFIG: &str = "SELECT set_config('statement_timeout', $1, true), \
set_config('lock_timeout', $2, true), set_config('application_name', $3, true)";

/// The end of each query and of each probe (8a.5).
pub(crate) const ROLLBACK: &str = "ROLLBACK";

/// The reset of a connection that goes back to the pool (8a.5).
pub(crate) const DISCARD_ALL: &str = "DISCARD ALL";

/// The most that `lock_timeout` takes, in milliseconds (8a.5).
pub(crate) const LOCK_TIMEOUT_MAX_MS: u64 = 5_000;

/// The three values of the `set_config` statement, as text (8a.5, step 5).
///
/// `statement_timeout` is the effective `max_execution_time` in milliseconds. `lock_timeout` is
/// the lower of 5000 and that value. `application_name` is `hune/<kind>/<query_id>` (8a.11).
pub(crate) fn query_values(max_execution_time: Duration, kind: LogKind, query_id: &Uuid) -> [String; 3] {
    let ms = u64::try_from(max_execution_time.as_millis()).unwrap_or(u64::MAX);
    [ms.to_string(), ms.min(LOCK_TIMEOUT_MAX_MS).to_string(), attrib::application_name(kind, query_id)]
}

/// Sends `BEGIN READ ONLY`.
///
/// # Errors
///
/// The error of the client: a transport fault, or an error of the server.
pub(crate) async fn begin_read_only(client: &Client) -> Result<(), tokio_postgres::Error> {
    client.batch_execute(BEGIN_READ_ONLY).await
}

/// Sends the `set_config` statement for one query. The values are bound parameters.
///
/// # Errors
///
/// The error of the client.
pub(crate) async fn set_query_values(
    client: &Client,
    max_execution_time: Duration,
    kind: LogKind,
    query_id: &Uuid,
) -> Result<(), tokio_postgres::Error> {
    let [timeout, lock, app] = query_values(max_execution_time, kind, query_id);
    client.execute(SET_CONFIG, &[&timeout, &lock, &app]).await.map(|_| ())
}

/// Sends `ROLLBACK`.
///
/// # Errors
///
/// The error of the client.
pub(crate) async fn rollback(client: &Client) -> Result<(), tokio_postgres::Error> {
    client.batch_execute(ROLLBACK).await
}

/// Sends `DISCARD ALL`.
///
/// # Errors
///
/// The error of the client.
pub(crate) async fn discard_all(client: &Client) -> Result<(), tokio_postgres::Error> {
    client.batch_execute(DISCARD_ALL).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qid() -> Uuid {
        Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").unwrap()
    }

    #[test]
    fn constants_are_the_texts_of_8a5() {
        assert_eq!(BEGIN_READ_ONLY, "BEGIN READ ONLY");
        assert_eq!(ROLLBACK, "ROLLBACK");
        assert_eq!(DISCARD_ALL, "DISCARD ALL");
        assert_eq!(
            SET_CONFIG,
            "SELECT set_config('statement_timeout', $1, true), set_config('lock_timeout', $2, true), set_config('application_name', $3, true)"
        );
    }

    #[test]
    fn query_values_follow_8a5() {
        let v = query_values(Duration::from_secs(30), LogKind::Panel, &qid());
        assert_eq!(v[0], "30000");
        assert_eq!(v[1], "5000");
        assert_eq!(v[2], "hune/panel/019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d");
        // A short limit gives a lower lock timeout.
        let v = query_values(Duration::from_secs(2), LogKind::Options, &qid());
        assert_eq!((v[0].as_str(), v[1].as_str()), ("2000", "2000"));
        let v = query_values(Duration::from_millis(5000), LogKind::Preview, &qid());
        assert_eq!((v[0].as_str(), v[1].as_str()), ("5000", "5000"));
        let v = query_values(Duration::from_millis(5001), LogKind::Preview, &qid());
        assert_eq!((v[0].as_str(), v[1].as_str()), ("5001", "5000"));
    }

    #[test]
    fn set_config_is_closed() {
        // The statement has three parameters and no other text that varies.
        assert_eq!(SET_CONFIG.matches('$').count(), 3);
        assert!(!SET_CONFIG.contains(';'));
        // `local` is true in each call: the values end with the transaction.
        assert_eq!(SET_CONFIG.matches(", true)").count(), 3);
    }
}
