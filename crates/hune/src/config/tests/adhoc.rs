// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the key `clickhouse "<id>".adhoc_columns` (13.12, 13.13): optional with no default, a closed
//! list of plain column names, no duplicate, the value form only, the reference and `print-config`.

use super::*;

fn with_key(fx: &Fx, line: &str) -> String {
    replace_once(
        &fx.local(),
        "  database           = \"analytics\"\n",
        &format!("  database           = \"analytics\"\n  {line}\n"),
    )
}

#[test]
fn an_absent_key_gives_no_list_and_no_line() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    assert_eq!(loaded.config.clickhouse[0].adhoc_columns, None);
    assert!(!render_print_config(&loaded).contains("adhoc_columns"));
    assert!(!start_log_config_json(&loaded).contains("adhoc_columns"));
}

#[test]
fn the_list_in_the_file_gives_its_value_in_print_config_and_the_start_log() {
    let fx = Fx::new();
    let loaded = fx.load(&with_key(&fx, "adhoc_columns = [\"service\", \"host\", \"_env2\"]")).unwrap();
    assert_eq!(
        loaded.config.clickhouse[0].adhoc_columns.as_deref(),
        Some(&["service".to_owned(), "host".to_owned(), "_env2".to_owned()][..])
    );
    let printed = render_print_config(&loaded);
    assert!(
        printed.contains("clickhouse \"prod\".adhoc_columns = [\"service\", \"host\", \"_env2\"]  # file\n"),
        "{printed}"
    );
    let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
    assert_eq!(config["clickhouse \"prod\".adhoc_columns"], serde_json::json!(["service", "host", "_env2"]));
    // The maximum: 256 items of 64 bytes.
    let items: Vec<String> = (0..256).map(|i| format!("\"c{i:0>63}\"")).collect();
    assert!(fx.load(&with_key(&fx, &format!("adhoc_columns = [{}]", items.join(", ")))).is_ok());
}

#[test]
fn a_bad_item_a_duplicate_a_wrong_count_or_form_stops_the_start() {
    let fx = Fx::new();
    for (rhs, want) in [
        ("[\"a.b\"]", "expected an item that matches ^[A-Za-z_][A-Za-z0-9_]{0,63}$, found \"a.b\""),
        ("[\"1a\"]", "found \"1a\""),
        ("[\"host\", \"m['k']\"]", "found \"m['k']\""),
        ("[\"é\"]", "found \"é\""),
        ("[\"host\", \"env\", \"host\"]", "expected a list with no repeated item, found \"host\" more than once"),
        ("[\"Host\", \"host\", \"Host\"]", "found \"Host\" more than once"),
    ] {
        let errors = fx.errors(&with_key(&fx, &format!("adhoc_columns = {rhs}")));
        assert_eq!(errors.len(), 1, "{rhs}: {errors:?}");
        assert!(
            errors[0].contains("clickhouse \"prod\".adhoc_columns") && errors[0].contains(want),
            "{rhs}: {}",
            errors[0]
        );
    }
    // Two items that differ in case are two columns.
    assert!(fx.load(&with_key(&fx, "adhoc_columns = [\"Host\", \"host\"]")).is_ok());
    // The count and the length of stage 1: no item, an empty item, 257 items, an item of 65 bytes.
    let many: Vec<String> = (0..257).map(|i| format!("\"c{i}\"")).collect();
    for rhs in
        ["[]".to_owned(), "[\"\"]".to_owned(), format!("[{}]", many.join(", ")), format!("[\"{}\"]", "a".repeat(65))]
    {
        let errors = fx.errors(&with_key(&fx, &format!("adhoc_columns = {rhs}")));
        assert_eq!(errors.len(), 1, "{rhs}: {errors:?}");
        assert!(errors[0].contains("adhoc_columns"), "{}", errors[0]);
    }
    // No coercion: a text is not a list. The value form only: no `_env`, no `_file`. A duplicate attribute stops too.
    for line in [
        "adhoc_columns = \"host\"",
        "adhoc_columns_env = \"HUNE_COLUMNS\"",
        "adhoc_columns_file = \"/etc/columns\"",
        "adhoc_columns = [\"host\"]\n  adhoc_columns = [\"env\"]",
    ] {
        let errors = fx.errors(&with_key(&fx, line));
        assert!(!errors.is_empty(), "{line}");
        assert!(errors.iter().any(|e| e.contains("adhoc_columns")), "{line}: {errors:?}");
    }
}

#[test]
fn the_key_is_in_the_table_and_the_reference() {
    let key = SCHEMA.key("clickhouse.adhoc_columns").unwrap();
    assert!(matches!(key.kind, Kind::TextList { min_items: 1, max_items: 256, item_min: 1, item_max: 64, .. }));
    assert!(!key.secret);
    assert!(REFERENCE_DOC.contains("| `clickhouse \"<id>\".adhoc_columns` |"), "the generated reference has a row");
    // The reference files hold no list: no ad hoc filter is on by default.
    let fx = Fx::new();
    assert!(!fx.local().contains("adhoc_columns") && !fx.distributed().contains("adhoc_columns"));
}
