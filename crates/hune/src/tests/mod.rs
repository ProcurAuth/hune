// SPDX-License-Identifier: MIT OR Apache-2.0
//! Helpers for the unit tests of the binary crate.
//!
//! The policy check of 16.18 permits file writes in test code only under a `tests/` path.
//! The unit tests in `src/` write their temporary files through this module.

pub mod files;
