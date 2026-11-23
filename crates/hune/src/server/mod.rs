// SPDX-License-Identifier: MIT OR Apache-2.0
//! The HTTP server of `hune serve`: the router, the pipeline of 6.28, the handlers, the ops
//! listener, the start sequence and the graceful stop.
//!
//! The command line calls `run_serve`, `build_runtime` and `run_health`.
#![allow(dead_code)]

pub mod build_info;
mod conn;
mod ctx;
mod encoding;
pub mod error;
mod handlers;
mod headers;
mod health_client;
mod json;
pub(crate) mod lifecycle;
pub(crate) mod limits;
mod ops;
mod pipeline;
mod query;
mod response;
pub(crate) mod route;
pub(crate) mod source_blocks;
mod sources;
mod spans;
pub(crate) mod startup;
pub(crate) mod state;

#[cfg(test)]
mod tests;

use crate::config::{AccessConfig, LoadedConfig};
use crate::log::Logger;
use std::sync::Arc;
use tokio::sync::mpsc;

pub use health_client::run_health;

/// The errors of the save checks of 7.18 for a document (see `handlers::save_check_errors`).
pub(crate) fn save_check_errors(
    doc: &hune_core::document::Document,
    config: &crate::config::Config,
) -> Vec<hune_core::error::ValidationError> {
    handlers::save_check_errors(doc, config)
}

/// Builds the `tokio` runtime of the server (3.6): `worker_threads` is the available
/// parallelism, and `max_blocking_threads` is 16 plus the PDP connections in mode `authzen`.
pub fn build_runtime(config: &crate::config::Config) -> std::io::Result<tokio::runtime::Runtime> {
    let workers = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let pdp = match &config.access {
        AccessConfig::Open => 0,
        AccessConfig::Authzen(a) => a.pdp.max_connections as usize,
    };
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .max_blocking_threads(16 + pdp)
        .enable_all()
        .build()
}

/// Sends the name of each SIGTERM and SIGINT to the channel.
fn forward_signals() -> std::io::Result<mpsc::UnboundedReceiver<&'static str>> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let name = tokio::select! {
                _ = term.recv() => "SIGTERM",
                _ = int.recv() => "SIGINT",
            };
            if tx.send(name).is_err() {
                return;
            }
        }
    });
    Ok(rx)
}

/// Runs `hune serve` (14.12): writes the start log of 13.22, opens the listeners, serves until
/// SIGTERM or SIGINT and stops (14.13). It gives the exit code: 0 or 1.
///
/// Call it inside the runtime of `build_runtime`.
pub async fn run_serve(loaded: LoadedConfig) -> u8 {
    let logger = Logger::stdout(loaded.config.log.level);
    logger.start_log(&loaded.effective, &loaded.warnings);
    let signals = match forward_signals() {
        Ok(s) => s,
        Err(e) => {
            logger.server_start_failed("signals", &format!("cannot install the signal handlers: {e}"));
            return 1;
        }
    };
    lifecycle::run(Arc::new(loaded.config), logger, limits::Timing::default(), startup::open_store, signals).await
}
