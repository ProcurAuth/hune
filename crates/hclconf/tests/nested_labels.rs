// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of a labelled block that sits inside a block (13.4): `store { volume "a" { } }`.

use hclconf::{check_text, BlockDesc, CapturedEnvironment, KeyDesc, Kind, Profile, Ref, Schema, TextRule, Value};

static BLOCKS: [BlockDesc; 3] = [
    BlockDesc::new("store", "store", "The stores."),
    BlockDesc::new("store.volume", "store.volume", "One volume.").labelled(0, 3, "main"),
    BlockDesc::new("store.db", "store.db", "The database.").manual("a database", "no database"),
];

static KEYS: [KeyDesc; 4] = [
    KeyDesc::new("store.volume.path", Kind::Path, "The path.").refs(Ref::Str("/data"), Ref::Absent),
    KeyDesc::new("store.volume.title", Kind::Text { min: 1, max: 20, rule: TextRule::NoControl }, "The title.")
        .refs(Ref::Str("Main"), Ref::Absent),
    KeyDesc::new("store.db.host", Kind::Text { min: 1, max: 20, rule: TextRule::NoControl }, "The host.")
        .refs(Ref::Absent, Ref::Str("db")),
    KeyDesc::new("store.db.port", Kind::Integer { min: 1, max: 65535 }, "The port.").refs(Ref::Absent, Ref::Int(5432)),
];

fn schema() -> Schema {
    Schema { blocks: &BLOCKS, keys: &KEYS }
}

fn env() -> CapturedEnvironment {
    CapturedEnvironment::from_pairs(std::iter::empty::<(String, String)>())
}

const GOOD: &str = r#"
store {
  db {
    host = "h"
    port = 5
  }

  volume "b" {
    title = "Bee"
    path  = "/b"
  }

  volume "a" {
    title = "Ay"
    path  = "/a"
  }
}
"#;

fn errors(text: &str) -> Vec<String> {
    match check_text(text, &schema(), &env()) {
        Ok(_) => Vec::new(),
        Err(list) => list.iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn a_labelled_block_can_have_a_parent() {
    schema().validate().unwrap();
}

#[test]
fn a_labelled_block_inside_a_labelled_block_is_refused() {
    static B: [BlockDesc; 3] = [
        BlockDesc::new("a", "a", "A.").labelled(0, 2, "x"),
        BlockDesc::new("a.b", "a.b", "B.").labelled(0, 2, "y"),
        BlockDesc::new("c", "c", "C."),
    ];
    static K: [KeyDesc; 1] = [KeyDesc::new("a.b.k", Kind::Bool, "K.")];
    let err = Schema { blocks: &B, keys: &K }.validate().unwrap_err();
    assert!(err.contains("inside another labelled block"), "{err}");
}

#[test]
fn the_labels_and_the_keys_of_each_block_are_found_in_file_order() {
    let checked = check_text(GOOD, &schema(), &env()).unwrap();
    assert_eq!(checked.labels("store.volume"), vec!["b", "a"]);
    assert_eq!(checked.get_in("a", "store.volume.path").map(|k| k.value.clone()), Some(Value::Str("/a".into())));
    assert_eq!(checked.get_in("b", "store.volume.title").map(|k| k.value.clone()), Some(Value::Str("Bee".into())));
    // A key of a block with no label has no label, and a key of a labelled block is not found without it.
    assert!(checked.get("store.db.host").is_some());
    assert!(checked.get("store.volume.path").is_none());
}

#[test]
fn the_keys_of_one_labelled_block_stay_together_in_the_order_of_the_table() {
    let checked = check_text(GOOD, &schema(), &env()).unwrap();
    let shown: Vec<&str> = checked.keys().iter().map(|k| k.display.as_str()).collect();
    assert_eq!(
        shown,
        vec![
            "store.volume \"b\".path",
            "store.volume \"b\".title",
            "store.volume \"a\".path",
            "store.volume \"a\".title",
            "store.db.host",
            "store.db.port",
        ]
    );
}

#[test]
fn a_duplicate_label_and_a_wrong_count_name_the_block_with_its_parent() {
    let text = "store {\n  volume \"a\" {\n    title = \"t\"\n    path = \"/a\"\n  }\n  volume \"a\" {\n    title = \"t\"\n    path = \"/b\"\n  }\n}\n";
    let e = errors(text);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(
        e[0].starts_with("store.volume \"a\": duplicate label; the first block with this label is at line 2"),
        "{}",
        e[0]
    );

    let mut many = String::from("store {\n");
    for n in 0..4 {
        many.push_str(&format!("  volume \"v{n}\" {{\n    title = \"t\"\n    path = \"/{n}\"\n  }}\n"));
    }
    many.push_str("}\n");
    let e = errors(&many);
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(e[0].contains("expected 0 to 3 blocks `store.volume \"<label>\"`, found 4"), "{}", e[0]);
}

#[test]
fn a_missing_key_of_a_labelled_block_names_its_label() {
    let e = errors("store {\n  volume \"a\" {\n    path = \"/a\"\n  }\n}\n");
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(e[0].starts_with("store.volume \"a\".title: missing attribute"), "{}", e[0]);
}

#[test]
fn the_reference_writes_the_labelled_block_inside_its_parent() {
    let local = hclconf::render_hcl(&schema(), Profile::Local, &[]);
    assert!(
        local.contains("store {\n  volume \"main\" {\n    path  = \"/data\"\n    title = \"Main\"\n  }\n}\n"),
        "{local}"
    );
    let markdown = hclconf::render_markdown(
        &schema(),
        &hclconf::MarkdownSpec { title: "T", intro: &[], profile_names: ["local", "distributed"] },
    );
    assert!(markdown.contains("| `store.volume \"<id>\".path` |"), "{markdown}");
    assert!(markdown.contains("## Block `store.volume \"<id>\"`"), "{markdown}");
}
