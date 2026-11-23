// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fuzz target `document_parse` (16.16, 4.36, 4.38, 4.39, 4.41).
//!
//! Property: for any bytes, `validate` and `parse_folder_file` do not panic. A valid document
//! gives canonical bytes. `validate` on these bytes gives a valid document with the same bytes
//! and the same etag, the same uid and the same title. With the uid as the expected uid, the
//! result is the same. A valid folder file gives a title, and the canonical bytes of that title
//! parse again to the same title. The migrations (4.42) do not panic. A document that the
//! migration 2 to 3 or the chain 1 to 3 gives is a valid document of format 3 with the same
//! canonical bytes at the second pass. The chain gives the bytes of its two steps, and the
//! migration 1 to 2 gives canonical bytes of format 2 that the migration 2 to 3 accepts.
#![no_main]

use libfuzzer_sys::fuzz_target;
use hune_core::document::{
    folder_file_bytes, migrate_1_to_2, migrate_1_to_3, migrate_2_to_3, parse_folder_file, validate,
};

fuzz_target!(|data: &[u8]| {
    if let Ok(doc) = validate(data, None) {
        let again = validate(doc.bytes(), None).expect("the canonical bytes do not validate");
        assert_eq!(again.bytes(), doc.bytes(), "the canonical bytes change at the second pass");
        assert_eq!(again.etag(), doc.etag(), "the etag changes at the second pass");
        assert_eq!(again.uid(), doc.uid());
        assert_eq!(again.title(), doc.title());
        let located = validate(doc.bytes(), Some(doc.uid())).expect("the canonical bytes fail with their own uid");
        assert_eq!(located.bytes(), doc.bytes());
    }
    if let Ok(migrated) = migrate_2_to_3(data, None) {
        let again = validate(migrated.bytes(), None).expect("the migrated bytes do not validate");
        assert_eq!(again.bytes(), migrated.bytes(), "the migrated bytes change at the second pass");
        assert_eq!(again.etag(), migrated.etag());
        assert!(migrate_2_to_3(migrated.bytes(), None).is_err(), "a document of format 3 migrates again");
    }
    if let Ok(chain) = migrate_1_to_3(data, None) {
        let again = validate(chain.bytes(), None).expect("the chain bytes do not validate");
        assert_eq!(again.bytes(), chain.bytes(), "the chain bytes change at the second pass");
        let step = migrate_1_to_2(data, None).expect("the first step fails where the chain works");
        let two = migrate_2_to_3(&step, None).expect("the second step fails on the output of the first");
        assert_eq!(two.bytes(), chain.bytes(), "the chain differs from its two steps");
        assert_eq!(two.etag(), chain.etag());
    }
    if let Ok(folder) = parse_folder_file(data) {
        let bytes = folder_file_bytes(&folder.title);
        let again = parse_folder_file(&bytes).expect("the canonical folder file does not parse");
        assert_eq!(again, folder);
        assert_eq!(folder_file_bytes(&again.title), bytes);
    }
});
