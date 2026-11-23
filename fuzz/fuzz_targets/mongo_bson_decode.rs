//! Fuzz target `mongo_bson_decode` (16.16, 8b.16, 7.28, 7.16c).
//!
//! The first byte of the input selects a reader, and the rest is its input.
//!
//! - 0: the conversion of the driver module, from the bytes of a BSON document of a reply to the value model.
//! - 1: the lenient reader of Extended JSON for server data (`MongoValue::from_extended_json`).
//! - 2: the strict parser of the JSON profile of 7.28 for the text of an author (`parse`).
//!
//! Property: no input makes a reader panic. A document that the conversion accepts goes through the frame builder
//! of 7.16c with no panic, and the conversion of the same bytes gives the same value two times.
#![no_main]

use libfuzzer_sys::fuzz_target;
use hune_core::mongo::{FrameCaps, MongoFrameBuilder, MongoValue, parse};
use hune_source::decode_mongo_document;

fuzz_target!(|data: &[u8]| {
    let Some((mode, rest)) = data.split_first() else {
        return;
    };
    match mode % 3 {
        0 => {
            if let Ok(doc) = decode_mongo_document(rest) {
                let again = decode_mongo_document(rest).expect("the second conversion fails");
                // `Double(NaN) != Double(NaN)` under `PartialEq`: compare the text form, where NaN equals NaN.
                assert_eq!(format!("{doc:?}"), format!("{again:?}"), "the conversion is not deterministic");
                let mut builder = MongoFrameBuilder::new(FrameCaps { max_rows: 10, max_bytes: 1 << 20 });
                if builder.push_document(&doc).is_ok() {
                    let _ = builder.finish("q".to_owned(), 1);
                }
            }
        }
        1 => {
            if let Ok(text) = std::str::from_utf8(rest) {
                let _ = MongoValue::from_extended_json(text);
                let _ = MongoValue::document_from_extended_json(text);
            }
        }
        _ => {
            if let Ok(text) = std::str::from_utf8(rest) {
                let _ = parse(&[text]);
            }
        }
    }
});
