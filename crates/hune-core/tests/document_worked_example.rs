// SPDX-License-Identifier: MIT OR Apache-2.0
//! The worked example of 4.40 and the folder file of 4.41.

use hune_core::document::{FolderFile, folder_file_bytes, parse_folder_file, validate};
use hune_core::ids::Uid;

const NODES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/nodes.json");
const ETAG: &str = "50b23d1cbce77a9fdcc19931973a8ba41299a08a497b713b8b6e45c4b992d501";

fn uid() -> Uid {
    Uid::parse("nodes").unwrap()
}

#[test]
fn worked_example_is_canonical_and_has_the_spec_etag() {
    let doc = validate(NODES, Some(&uid())).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(NODES.len(), 8397);
    assert_eq!(doc.bytes(), NODES);
    assert_eq!(doc.etag().to_hex(), ETAG);
    assert_eq!(doc.uid().as_str(), "nodes");
    assert_eq!(doc.title(), "Nodes");
    assert_eq!(doc.format_version(), 3);
}

#[test]
fn folder_file_example_round_trips() {
    let bytes = folder_file_bytes("Transit desk");
    assert_eq!(bytes.len(), 53);
    assert_eq!(bytes, b"{\n  \"format_version\": 1,\n  \"title\": \"Transit desk\"\n}\n");
    assert_eq!(parse_folder_file(&bytes).unwrap(), FolderFile { title: "Transit desk".to_owned() });
}
