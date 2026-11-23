// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the key `server.compress_static` (13.8, 9.3): an optional boolean with the
//! default `true`, a value in the file only, and no coercion.

use super::*;

fn with_key(fx: &Fx, line: &str) -> String {
    replace_once(
        &fx.local(),
        "  editing                 = true\n",
        &format!("  editing                 = true\n  {line}\n"),
    )
}

#[test]
fn an_absent_key_is_true_and_shows_the_source_default() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    assert!(loaded.config.server.compress_static);
    let entry = loaded.effective.iter().find(|e| e.key == "server.compress_static").unwrap();
    assert_eq!((entry.value.as_str(), entry.json.as_str(), entry.source.as_str()), ("true", "true", "default"));
    let printed = render_print_config(&loaded);
    assert!(printed.contains("server.compress_static = true  # default\n"), "{printed}");
    let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
    assert_eq!(config["server.compress_static"], true);
    assert!(start_log_sources_json(&loaded).contains("\"server.compress_static\":\"default\""));
}

#[test]
fn the_key_in_the_file_gives_its_value_and_the_source_file() {
    let fx = Fx::new();
    for (text, want) in [("true", true), ("false", false)] {
        let loaded = fx.load(&with_key(&fx, &format!("compress_static = {text}"))).unwrap();
        assert_eq!(loaded.config.server.compress_static, want);
        let printed = render_print_config(&loaded);
        assert!(printed.contains(&format!("server.compress_static = {text}  # file\n")), "{printed}");
        let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
        assert_eq!(config["server.compress_static"], want);
        assert!(!start_log_sources_json(&loaded).contains("compress_static"));
    }
}

#[test]
fn the_key_is_in_the_table_the_reference_and_the_effective_order() {
    let key = SCHEMA.key("server.compress_static").unwrap();
    assert!(matches!(key.kind, Kind::Bool));
    assert!(!key.secret);
    assert_eq!(key.forms, hclconf::Forms::V);
    assert!(REFERENCE_DOC.contains("| `server.compress_static` | a boolean |"));
    assert!(REFERENCE_DOC.contains("`server.compress_static` (default `true`)"));
    let fx = Fx::new();
    let order: Vec<String> = fx.load(&fx.local()).unwrap().effective.iter().map(|e| e.key.clone()).collect();
    let position = |keys: &[String], name: &str| keys.iter().position(|k| k == name).unwrap();
    assert_eq!(position(&order, "server.compress_static"), position(&order, "server.repeat_max_per_dashboard") + 1);
    // The reference files do not hold the key: its default applies.
    for profile in [Profile::Local, Profile::Distributed] {
        let text = render_reference(ReferenceFormat::Hcl, profile);
        assert!(!text.contains("compress_static ="), "{profile:?}");
        assert!(text.contains("`server.compress_static`"), "{profile:?}");
    }
}

#[test]
fn a_wrong_form_or_value_stops_the_start() {
    let fx = Fx::new();
    // No coercion: a text and an integer are not a boolean.
    for rhs in ["\"true\"", "\"false\"", "1", "0", "yes", "on"] {
        let errors = fx.errors(&with_key(&fx, &format!("compress_static = {rhs}")));
        assert_eq!(errors.len(), 1, "{rhs}: {errors:?}");
        assert!(errors[0].starts_with("server.compress_static: "), "{}", errors[0]);
    }
    let errors = fx.errors(&with_key(&fx, "compress_static = \"true\""));
    assert!(errors[0].contains("expected a boolean, found \"true\""), "{}", errors[0]);
    // The key accepts no `_env` and no `_file`: the value is in the file only.
    for line in ["compress_static_env = \"HUNE_GZ\"", "compress_static_file = \"/etc/gz\""] {
        let errors = fx.errors(&with_key(&fx, line));
        assert_eq!(errors.len(), 1, "{line}: {errors:?}");
        assert!(errors[0].contains("accepts only a value in the file"), "{}", errors[0]);
    }
    // A duplicate attribute stops the start.
    let twice = with_key(&fx, "compress_static = true\n  compress_static = false");
    assert!(fx.errors(&twice).iter().any(|e| e.contains("compress_static")));
    // A key with a near name is an unknown key.
    let near = fx.errors(&with_key(&fx, "compress_statics = true"));
    assert_eq!(near.len(), 1, "{near:?}");
    assert!(near[0].contains("compress_statics"), "{}", near[0]);
}
