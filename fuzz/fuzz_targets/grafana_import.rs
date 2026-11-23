//! Fuzz target `grafana_import` (16.16, section 15).
//!
//! The input is the text of a Grafana dashboard file. The target parses it as JSON, then runs the
//! importer two times: with no acceptance, and with the acceptance of each item of the first run.
//!
//! Property: no input makes the importer panic. When a run gives the bytes of a document, these
//! bytes validate again to the same bytes and the same etag (15.2: the bytes are the output of
//! `validate`). A run that gives no document has no etag.
#![no_main]

use libfuzzer_sys::fuzz_target;
use hune_core::document::validate;
use hune_grafana::{Acceptance, DashboardImport, ImportOptions, import_dashboard};

fn check(result: &DashboardImport) {
    match &result.document_bytes {
        Some(bytes) => {
            let doc = validate(bytes, None).expect("the importer gave bytes that do not validate");
            assert_eq!(doc.bytes(), bytes.as_slice(), "the importer bytes are not canonical");
            assert_eq!(result.etag.as_deref(), Some(doc.etag().to_hex().as_str()), "the etag of the report differs");
        }
        None => assert!(result.etag.is_none(), "an etag without a document"),
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
        return;
    };
    let mut options = ImportOptions::with_source_map(&[("ch-main", "prod"), ("ClickHouse", "prod"), ("clickhouse", "prod"), ("conf-ds", "conf")]);
    let first = import_dashboard(&value, &options);
    check(&first);
    options.accept = Acceptance { ids: first.items.iter().map(|i| i.id.clone()).collect() };
    let second = import_dashboard(&value, &options);
    check(&second);
});
