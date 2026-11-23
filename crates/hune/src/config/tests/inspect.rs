// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the key `server.query_inspect` (13.8): the only key with a default.

use super::*;

fn with_key(fx: &Fx, line: &str) -> String {
    replace_once(
        &fx.local(),
        "  editing                 = true\n",
        &format!("  editing                 = true\n  {line}\n"),
    )
}

#[test]
fn an_absent_key_is_false_and_shows_the_source_default() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    assert!(!loaded.config.server.query_inspect);
    let entry = loaded.effective.iter().find(|e| e.key == "server.query_inspect").unwrap();
    assert_eq!((entry.value.as_str(), entry.json.as_str(), entry.source.as_str()), ("false", "false", "default"));
    let printed = render_print_config(&loaded);
    assert!(printed.contains("server.query_inspect = false  # default\n"));
}

#[test]
fn the_key_in_the_file_gives_its_value_and_the_source_file() {
    let fx = Fx::new();
    for (text, want) in [("true", true), ("false", false)] {
        let loaded = fx.load(&with_key(&fx, &format!("query_inspect = {text}"))).unwrap();
        assert_eq!(loaded.config.server.query_inspect, want);
        let printed = render_print_config(&loaded);
        assert!(printed.contains(&format!("server.query_inspect = {text}  # file\n")), "{printed}");
        let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
        assert_eq!(config["server.query_inspect"], want);
        assert!(!start_log_sources_json(&loaded).contains("query_inspect"));
    }
}

#[test]
fn the_key_is_in_the_table_the_reference_and_the_effective_order() {
    let key = SCHEMA.key("server.query_inspect").unwrap();
    assert!(matches!(key.kind, Kind::Bool));
    assert!(!key.secret);
    assert!(REFERENCE_DOC.contains("| `server.query_inspect` | a boolean |"));
    let fx = Fx::new();
    let order: Vec<String> = fx.load(&fx.local()).unwrap().effective.iter().map(|e| e.key.clone()).collect();
    let position = |keys: &[String], name: &str| keys.iter().position(|k| k == name).unwrap();
    assert_eq!(position(&order, "server.query_inspect"), position(&order, "server.editing") + 1);
}

#[test]
fn a_wrong_form_or_value_stops_the_start() {
    let fx = Fx::new();
    // No coercion: a text and an integer are not a boolean.
    for rhs in ["\"true\"", "1", "0", "yes"] {
        let errors = fx.errors(&with_key(&fx, &format!("query_inspect = {rhs}")));
        assert_eq!(errors.len(), 1, "{rhs}: {errors:?}");
        assert!(errors[0].contains("server.query_inspect"), "{}", errors[0]);
    }
    // The key accepts no `_env` and no `_file`: the key accepts only a value in the file.
    for line in ["query_inspect_env = \"HUNE_QI\"", "query_inspect_file = \"/etc/qi\""] {
        let errors = fx.errors(&with_key(&fx, line));
        assert_eq!(errors.len(), 1, "{line}: {errors:?}");
        assert!(errors[0].contains("accepts only a value in the file"), "{}", errors[0]);
    }
    // A duplicate attribute stops the start.
    let twice = with_key(&fx, "query_inspect = true\n  query_inspect = false");
    assert!(fx.errors(&twice).iter().any(|e| e.contains("query_inspect")));
}

#[test]
fn c21_the_key_in_the_mode_authzen_needs_editing() {
    let fx = Fx::new();
    let text = replace_once(
        &fx.distributed(),
        "  editing                 = true\n",
        "  editing                 = false\n  query_inspect           = true\n",
    );
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("server.query_inspect, server.editing, access.mode: "), "{}", errors[0]);
    // editing true, or the key false, or the mode open: no contradiction.
    let ok = replace_once(
        &fx.distributed(),
        "  editing                 = true\n",
        "  editing                 = true\n  query_inspect           = true\n",
    );
    assert!(fx.errors(&ok).is_empty());
    let off = replace_once(
        &fx.distributed(),
        "  editing                 = true\n",
        "  editing                 = false\n  query_inspect           = false\n",
    );
    assert!(fx.errors(&off).is_empty());
    let open = replace_once(
        &fx.local(),
        "  editing                 = true\n",
        "  editing                 = false\n  query_inspect           = true\n",
    );
    assert!(fx.errors(&set(&open, "storage.volume.read_only", "true")).is_empty());
}
