// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fuzz target `hcl_config` (16.16, section 13).
//!
//! The target checks the input text against the real key table of Hune. The table lives in the
//! binary crate `hune`, which has no library, so the target includes the source file of the
//! table with a `#[path]` attribute. The file uses only `hclconf`.
//!
//! Property: no text makes `check_text` panic, in each read mode. The result does not change
//! between two runs. An error always has a message that is not empty. Each key of a valid result
//! belongs to the table.
//!
//! The `_file` form reads a file of the host. The target skips a text that names a path in
//! `/dev`, `/proc` or `/sys`, because a read there can block or has no end.
#![no_main]
#![allow(dead_code)]

#[path = "../../crates/hune/src/config/keys.rs"]
mod keys;

use hclconf::check::ReadMode;
use hclconf::{CapturedEnvironment, ConfigError, check_text_with};
use libfuzzer_sys::fuzz_target;

fn describe(errors: &[ConfigError]) -> Vec<String> {
    errors.iter().map(|e| format!("{}|{:?}|{}", e.key, e.line, e.message)).collect()
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if text.contains("/dev") || text.contains("/proc") || text.contains("/sys") {
        return;
    }
    let env = CapturedEnvironment::from_pairs([
        ("HUNE_A".to_owned(), "a".to_owned()),
        ("HUNE_TOKEN".to_owned(), "secret-value".to_owned()),
        ("HUNE_EMPTY".to_owned(), String::new()),
        ("HUNE_PORT".to_owned(), "8080".to_owned()),
    ]);
    let schema = &keys::SCHEMA;
    for mode in [ReadMode::All, ReadMode::None, ReadMode::Blocks(&["storage", "clickhouse"])] {
        let first = check_text_with(text, schema, &env, mode);
        let second = check_text_with(text, schema, &env, mode);
        match (first, second) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.keys().len(), b.keys().len());
                for key in a.keys() {
                    assert!(schema.key(&key.path).is_some(), "a key of the result is not in the table");
                }
            }
            (Err(a), Err(b)) => {
                assert!(!a.is_empty(), "an error result with no error");
                assert!(a.iter().all(|e| !e.message.is_empty()), "an error with an empty message");
                assert_eq!(describe(&a), describe(&b), "the errors change between two runs");
            }
            _ => panic!("the result changes between two runs"),
        }
    }
});
