// SPDX-License-Identifier: MIT OR Apache-2.0
//! The cancel request (8a.7).
//!
//! A [`Canceller`] sends a CancelRequest on a new connection to the same host and port, with the
//! same TLS rules as the session (the name check of `tls.server_name` included). It never calls
//! `pg_cancel_backend` or `pg_terminate_backend` (8a.7). The request needs no privilege and no
//! SQL.
//!
//! One cancel request runs at a time for one source, so a source holds `max_concurrent + 2`
//! connections at most (8a.9). The wait for the turn and the request itself share one time limit:
//! `connect_timeout`. A lost request is not an error: the caller closes the connection of the
//! session anyway, and `client_connection_check_interval = 1000` makes the server stop the query
//! (8a.7). The caller closes the connection after [`Canceller::send`].
//!
//! The query leaves `pg_stat_activity` within 2 s after the cancel (the test PS05).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio_postgres::{CancelToken, NoTls};

use crate::postgres::session::{OpenError, Tls, tcp_connect};

/// The capability to cancel the query of one session.
#[derive(Clone)]
pub(crate) struct Canceller {
    token: CancelToken,
    tls: Tls,
    /// The host and the port of the block. A session without TLS opens its stream itself, so the client
    /// crate has no address for the cancel request: this module opens the connection (finding M4).
    target: (String, u16),
    limit: Duration,
    gate: Arc<Semaphore>,
}

impl Canceller {
    pub(crate) fn new(
        token: CancelToken,
        tls: Tls,
        target: (String, u16),
        limit: Duration,
        gate: &Arc<Semaphore>,
    ) -> Canceller {
        Canceller { token, tls, target, limit, gate: Arc::clone(gate) }
    }

    /// Sends one CancelRequest. It gives `true` when the server closed the cancel connection after
    /// the request, and `false` for a fault or a timeout. The server never says if the cancel worked.
    pub(crate) async fn send(&self) -> bool {
        let run = async {
            let Ok(_turn) = self.gate.acquire().await else { return false };
            match &self.tls {
                Tls::Plain => match tcp_connect(&self.target.0, self.target.1).await {
                    Ok(stream) => self.token.cancel_query_raw(stream, NoTls).await.is_ok(),
                    Err(OpenError::Unavailable(_) | OpenError::Misconfigured(_)) => false,
                },
                Tls::Rustls(tls) => self.token.cancel_query(tls.clone()).await.is_ok(),
            }
        };
        tokio::time::timeout(self.limit, run).await.unwrap_or(false)
    }
}
