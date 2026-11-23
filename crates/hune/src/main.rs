// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `hune` binary: the server and the command line tools (3.1).
//!
//! `main` captures and clears the environment first (3.5, step 1; R2).
#![forbid(unsafe_code)]

mod access;
mod cli;
mod gzip;
mod log;
mod metrics;
mod server;
mod telemetry;
#[cfg(test)]
mod tests;
mod web_assets;
// The typed configuration has members that the server reads from later milestones.
#[allow(dead_code)]
mod config;

use std::process::ExitCode;

fn main() -> ExitCode {
    let env = config::CapturedEnvironment::capture_and_clear();
    let code = cli::run(std::env::args_os().skip(1), &env);
    ExitCode::from(code)
}
