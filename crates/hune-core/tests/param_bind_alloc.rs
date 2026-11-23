#![cfg(target_os = "linux")]
// SPDX-License-Identifier: MIT OR Apache-2.0
//! The memory of `params::bind` before the cap of 4,096 bytes refuses a parameter (finding 3 of the security review 3).
//!
//! One `Array(String)` of 255 values of 1,024 `'` fits the request body of 6.20 (262,144 bytes). Before the fix, bind
//! built the escaped text (about 523 KB) and its percent encoding (about 1.6 MB) before the check of the cap. It now adds
//! the bytes of the values first, and it refuses the pair before it builds any text.
//!
//! The workspace forbids unsafe code, so the test cannot install a counting global allocator. It reads the peak resident
//! memory of the process (`VmHWM`) instead, as `memory_canonical.rs` does. It sets the peak back to the resident memory
//! with `/proc/self/clear_refs` (Linux 4.0 and later), calls bind one time, and compares the peak with the resident memory
//! before the call. The kernel counts the resident pages of each CPU in batches, so the bound has room for one batch
//! (176 KiB on 22 CPUs). Only the first call shows the growth: after it, the allocator keeps the freed pages. This file
//! holds one test, because the peak of the process is global.

use hune_core::sql::params::{BindError, ParamType, bind};
use hune_core::sql::{MAX_PARAM_BYTES, RequestValues, VariableDef, resolve_values};
use std::collections::BTreeMap;

/// A value in bytes of `/proc/self/status`, for example `VmRSS` or `VmHWM`.
fn status_bytes(field: &str) -> usize {
    let status = std::fs::read_to_string("/proc/self/status").expect("/proc/self/status");
    let line = status.lines().find(|l| l.starts_with(field)).unwrap_or_else(|| panic!("{field}"));
    let kb: usize = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).expect("a number of kB");
    kb * 1024
}

#[test]
fn a_parameter_above_the_cap_is_refused_before_its_text_takes_memory() {
    let value = "'".repeat(1024);
    let items: Vec<&str> = (0..255).map(|_| value.as_str()).collect();
    let defs = vec![VariableDef::query("p", true, false, None)];
    let mut req = RequestValues::new();
    req.insert("p", false, &items);
    let values = resolve_values(&defs, &req).unwrap();
    let mut pairs = BTreeMap::new();
    pairs.insert("p".to_owned(), ParamType::ArrayString);
    // `5` sets the peak of the process back to its resident memory now.
    std::fs::write("/proc/self/clear_refs", "5").expect("/proc/self/clear_refs");
    let before = status_bytes("VmRSS");
    let result = bind(&pairs, &values);
    let peak = status_bytes("VmHWM");
    match result {
        Err(BindError::Refused(r)) => {
            assert_eq!(r.variable, "p");
            assert!(r.reason.contains(&format!("{MAX_PARAM_BYTES} bytes")), "{}", r.reason);
        }
        other => panic!("the cap must refuse the parameter: {other:?}"),
    }
    let grown = peak.saturating_sub(before);
    println!("bind of 255 x 1,024 ': the peak resident memory grows by {grown} bytes");
    // Before the fix, the growth was about 2 MB.
    assert!(grown < 512 * 1024, "the peak grows by {grown} bytes");
}
