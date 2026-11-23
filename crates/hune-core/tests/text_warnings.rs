//! Warning 6 of 7.18: a placeholder in a display text that names no variable of the document.

use hune_core::document::{TextWarning, text_warnings, validate};
use serde_json::{Value, json};

const ALL_TYPES: &[u8] = include_bytes!("../../../tests/fixtures/dashboards/all_types.json");

fn warnings_of(v: &Value) -> Vec<TextWarning> {
    let c = validate(&serde_json::to_vec(v).unwrap(), None).expect("valid document");
    text_warnings(c.document())
}

fn base() -> Value {
    serde_json::from_slice(ALL_TYPES).unwrap()
}

fn w(pointer: &str, message: &str) -> TextWarning {
    TextWarning { pointer: pointer.to_owned(), message: message.to_owned() }
}

#[test]
fn the_base_document_has_no_warning() {
    assert!(warnings_of(&base()).is_empty());
}

#[test]
fn known_names_and_builtins_give_no_warning() {
    let mut v = base();
    v["sections"][0]["panels"][0]["title"] = json!("Rows of $q and ${c} and ${t:text} and $__from");
    v["sections"][1]["title"] = json!("Section ${q:csv} ${__interval}");
    assert!(warnings_of(&v).is_empty());
}

#[test]
fn a_panel_title_and_description() {
    let mut v = base();
    v["sections"][0]["panels"][1]["title"] = json!("Latency of $service in ${env}");
    v["sections"][0]["panels"][1]["description"] = json!("Known $q, unknown ${gone:csv}");
    assert_eq!(
        warnings_of(&v),
        vec![
            w("/sections/0/panels/1/title", "$service is not a variable of the document"),
            w("/sections/0/panels/1/title", "${env} is not a variable of the document"),
            w("/sections/0/panels/1/description", "${gone:csv} is not a variable of the document"),
        ]
    );
}

#[test]
fn a_section_title() {
    let mut v = base();
    v["sections"][1]["title"] = json!("Hosts $host");
    assert_eq!(warnings_of(&v), vec![w("/sections/1/title", "$host is not a variable of the document")]);
}

#[test]
fn a_panel_link_and_data_links() {
    let mut v = base();
    v["sections"][0]["panels"][0]["links"][0]["title"] = json!("Docs of $team");
    v["sections"][0]["panels"][0]["fields"]["defaults"]["links"][0]["title"] = json!("Data ${row}");
    v["sections"][0]["panels"][0]["fields"]["overrides"][0]["set"]["links"] =
        json!([{"title": "Over $q $z", "url": "/d/x", "target_blank": false}]);
    assert_eq!(
        warnings_of(&v),
        vec![
            w("/sections/0/panels/0/links/0/title", "$team is not a variable of the document"),
            w("/sections/0/panels/0/fields/defaults/links/0/title", "${row} is not a variable of the document"),
            w("/sections/0/panels/0/fields/overrides/0/set/links/0/title", "$z is not a variable of the document"),
        ]
    );
}

#[test]
fn a_malformed_brace_and_a_double_bracket_give_no_warning() {
    let mut v = base();
    v["sections"][1]["title"] = json!("a ${ b [[q]] [[zz]] $ 5");
    assert!(warnings_of(&v).is_empty());
}
