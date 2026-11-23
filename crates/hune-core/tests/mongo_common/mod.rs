// SPDX-License-Identifier: MIT OR Apache-2.0
//! Helpers that the MongoDB tests share: a source with known lists, and short calls of the engine.
#![allow(dead_code)]

use hune_core::error::ErrorCode;
use hune_core::mongo::{
    MongoExpandError, MongoExpandRequest, MongoQuery, MongoSaveResult, SaveContext, WarnKind, prepare, save_checks,
};
use hune_core::sql::{
    RequestValues, ResolvedValues, SqlUse, TextValueType, VariableDef, check_time_range, compute_interval,
    resolve_values,
};

pub const COLLECTIONS: [&str; 3] = ["node_load", "hosts", "events"];
pub const DATABASES: [&str; 1] = ["metrics"];
pub const FROM_MS: i64 = 1_794_211_200_000;
pub const TO_MS: i64 = 1_794_232_800_000;

pub fn collections() -> Vec<String> {
    COLLECTIONS.iter().map(|s| (*s).to_owned()).collect()
}

pub fn databases() -> Vec<String> {
    DATABASES.iter().map(|s| (*s).to_owned()).collect()
}

/// The variables that the tests use.
pub fn defs() -> Vec<VariableDef> {
    vec![
        VariableDef::text("host", TextValueType::Text),
        VariableDef::text("n", TextValueType::Text),
        VariableDef::query("hosts", true, true, None),
        VariableDef::query("one", false, false, None),
        VariableDef::custom("env", &["prod", "$dev", "a.b"], false, false, None),
        VariableDef::custom("envs", &["prod", "dev"], true, true, None),
    ]
}

pub fn save(text: &str) -> MongoSaveResult {
    save_with(text, &defs(), SqlUse::Panel)
}

pub fn save_with(text: &str, variables: &[VariableDef], usage: SqlUse) -> MongoSaveResult {
    let (c, d) = (collections(), databases());
    save_checks(&[text], &SaveContext { variables, collections: &c, databases: &d, usage })
}

/// The codes of the faults of a save, in order.
pub fn save_codes(text: &str) -> Vec<ErrorCode> {
    save(text).faults.iter().map(|f| f.code).collect()
}

pub fn warning_kinds(text: &str) -> Vec<WarnKind> {
    save(text).warnings.iter().map(|w| w.kind).collect()
}

pub fn request(items: &[(&str, bool, &[&str])]) -> RequestValues {
    let mut r = RequestValues::new();
    for (n, a, v) in items {
        r.insert(n, *a, v);
    }
    r
}

pub fn resolved(req: &RequestValues) -> ResolvedValues {
    resolve_values(&defs(), req).expect("request values")
}

/// Runs the whole expansion on one text, for a panel (an interval) or a variable (no interval).
pub fn run_with(
    text: &str,
    defs: &[VariableDef],
    req: &RequestValues,
    variable: bool,
) -> Result<MongoQuery, MongoExpandError> {
    let values = resolve_values(defs, req).expect("request values");
    let range = check_time_range(FROM_MS, TO_MS).unwrap();
    let interval = if variable { None } else { Some(compute_interval(&range, 1200, 1).unwrap()) };
    let (c, d) = (collections(), databases());
    prepare(
        &[text],
        &MongoExpandRequest {
            values: &values,
            range,
            interval,
            max_query_size: 262_144,
            collections: &c,
            databases: &d,
        },
    )
}

pub fn run(text: &str, req: &RequestValues) -> Result<MongoQuery, MongoExpandError> {
    run_with(text, &defs(), req, false)
}

pub fn run_plain(text: &str) -> Result<MongoQuery, MongoExpandError> {
    run(text, &RequestValues::new())
}
