// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the keys `server.history` and `server.export` (13.8, K24): two mandatory Booleans with no default.

use super::*;

const HISTORY_LINE: &str = "  history                 = true\n";
const EXPORT_LINE: &str = "  export                  = true\n";

fn with_lines(text: &str, history: &str, export: &str) -> String {
    let text = replace_once(text, HISTORY_LINE, history);
    replace_once(&text, EXPORT_LINE, export)
}

/// A file in which one line replaces the line of `name`, and the other line stays as it is.
fn with_line(text: &str, name: &str, line: &str) -> String {
    if name == "history" { with_lines(text, line, EXPORT_LINE) } else { with_lines(text, HISTORY_LINE, line) }
}

#[test]
fn each_key_in_the_file_gives_its_value_and_the_source_file() {
    let fx = Fx::new();
    for (history, export) in [(true, true), (true, false), (false, true), (false, false)] {
        let text = with_lines(&fx.local(), &format!("  history = {history}\n"), &format!("  export = {export}\n"));
        // With `server.history = false` the primary store has no `history_keep` (C31).
        let text = if history { text } else { text.replace("    history_keep    = 100\n", "") };
        let loaded = fx.load(&text).unwrap();
        assert_eq!((loaded.config.server.history, loaded.config.server.export), (history, export));
        let printed = render_print_config(&loaded);
        assert!(printed.contains(&format!("server.history = {history}  # file\n")), "{printed}");
        assert!(printed.contains(&format!("server.export = {export}  # file\n")), "{printed}");
        let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
        assert_eq!(config["server.history"], history);
        assert_eq!(config["server.export"], export);
        assert!(!start_log_sources_json(&loaded).contains("server.history"));
    }
}

#[test]
fn a_missing_key_stops_the_start_and_names_the_key_and_the_type() {
    let fx = Fx::new();
    for key in ["history", "export"] {
        let errors = fx.errors(&with_line(&fx.local(), key, ""));
        assert_eq!(errors.len(), 1, "{key}: {errors:?}");
        assert!(
            errors[0].starts_with(&format!("server.{key}: missing attribute; expected a boolean")),
            "{}",
            errors[0]
        );
    }
    // Both missing: two errors, in the order of the table.
    let errors = fx.errors(&with_lines(&fx.local(), "", ""));
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors[0].starts_with("server.history: "), "{}", errors[0]);
    assert!(errors[1].starts_with("server.export: "), "{}", errors[1]);
}

#[test]
fn a_value_that_is_not_a_boolean_stops_the_start_with_no_coercion() {
    let fx = Fx::new();
    for key in ["history", "export"] {
        for rhs in ["\"true\"", "1", "0", "yes"] {
            let errors = fx.errors(&with_line(&fx.local(), key, &format!("  {key} = {rhs}\n")));
            assert_eq!(errors.len(), 1, "{key} {rhs}: {errors:?}");
            assert!(errors[0].starts_with(&format!("server.{key}: ")), "{}", errors[0]);
            assert!(errors[0].contains("boolean"), "{}", errors[0]);
        }
    }
}

#[test]
fn the_keys_accept_only_a_value_in_the_file_and_a_duplicate_stops_the_start() {
    let fx = Fx::new();
    for name in ["history", "export"] {
        for line in [format!("  {name}_env = \"HUNE_X\"\n"), format!("  {name}_file = \"/etc/x\"\n")] {
            let errors = fx.errors(&with_line(&fx.local(), name, &line));
            assert!(errors.iter().any(|e| e.contains("accepts only a value in the file")), "{line}: {errors:?}");
        }
        let twice = format!("  {name} = true\n  {name} = false\n");
        assert!(fx.errors(&with_line(&fx.local(), name, &twice)).iter().any(|e| e.contains(name)), "{name}");
    }
}

#[test]
fn the_keys_are_mandatory_in_the_mode_open_and_in_the_mode_authzen() {
    let fx = Fx::new();
    // The local file is in the mode `open`.
    assert!(fx.local().contains("mode = \"open\""));
    assert!(fx.errors(&with_lines(&fx.local(), "", EXPORT_LINE)).iter().any(|e| e.starts_with("server.history: ")));
    // The distributed file is in the mode `authzen`.
    let dist = fx.distributed();
    assert!(dist.contains("mode = \"authzen\""));
    for name in ["history", "export"] {
        let errors = fx.errors(&with_line(&dist, name, ""));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("missing attribute"), "{}", errors[0]);
    }
    // With a `false` value, the file loads in both modes.
    let off =
        with_lines(&dist, "  history = false\n", "  export = false\n").replace("    history_keep      = 100\n", "");
    let loaded = fx.load(&off).unwrap();
    assert!(!loaded.config.server.history && !loaded.config.server.export);
}

#[test]
fn the_keys_are_in_the_table_the_reference_and_the_effective_order() {
    for key in ["server.history", "server.export"] {
        let desc = SCHEMA.key(key).unwrap();
        assert!(matches!(desc.kind, Kind::Bool), "{key}");
        assert!(!desc.secret, "{key}");
        assert!(
            REFERENCE_DOC.contains(&format!("| `{key}` | a boolean | no | V | always | never | `true` |")),
            "{key}"
        );
    }
    let fx = Fx::new();
    let order: Vec<String> = fx.load(&fx.local()).unwrap().effective.iter().map(|e| e.key.clone()).collect();
    let position = |name: &str| order.iter().position(|k| k == name).unwrap();
    assert_eq!(position("server.history"), position("server.base_path") + 1);
    assert_eq!(position("server.export"), position("server.history") + 1);
}

#[test]
fn neither_key_has_a_default_value() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    for key in ["server.history", "server.export"] {
        let entry = loaded.effective.iter().find(|e| e.key == key).unwrap();
        assert_eq!(entry.source, "file", "{key}");
    }
    assert!(!REFERENCE_DOC.contains("`server.history` (default"));
    assert!(!REFERENCE_DOC.contains("`server.export` (default"));
}
