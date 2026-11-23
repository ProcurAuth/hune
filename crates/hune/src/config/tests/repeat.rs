// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the keys `server.repeat_max_per_panel`, `server.repeat_max_per_section` and
//! `server.repeat_max_per_dashboard` (13.8, 4.35): optional keys with a default, a hard range and
//! the contradictions C28 and C29.

use super::*;

const KEYS: [(&str, &str, u64, u64); 3] = [
    ("server.repeat_max_per_panel", "repeat_max_per_panel", 24, 200),
    ("server.repeat_max_per_section", "repeat_max_per_section", 12, 50),
    ("server.repeat_max_per_dashboard", "repeat_max_per_dashboard", 120, 200),
];

fn with_lines(fx: &Fx, lines: &[String]) -> String {
    let add: String = lines.iter().map(|l| format!("  {l}\n")).collect();
    replace_once(&fx.local(), "  editing                 = true\n", &format!("  editing                 = true\n{add}"))
}

fn with(fx: &Fx, pairs: &[(&str, u64)]) -> String {
    let lines: Vec<String> = pairs.iter().map(|(k, v)| format!("{k} = {v}")).collect();
    with_lines(fx, &lines)
}

#[test]
fn absent_keys_have_the_defaults_and_show_the_source_default() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    let s = &loaded.config.server;
    assert_eq!((s.repeat_max_per_panel, s.repeat_max_per_section, s.repeat_max_per_dashboard), (24, 12, 120));
    let printed = render_print_config(&loaded);
    for (key, _, default, _) in KEYS {
        let entry = loaded.effective.iter().find(|e| e.key == key).unwrap();
        assert_eq!(
            (entry.value.as_str(), entry.json.as_str(), entry.source.as_str()),
            (&*default.to_string(), &*default.to_string(), "default")
        );
        assert!(printed.contains(&format!("{key} = {default}  # default\n")), "{printed}");
    }
    // The defaults are the constants of `hune-core`.
    assert_eq!(
        (
            hune_core::limits::MAX_REPEAT_PER_PANEL,
            hune_core::limits::MAX_REPEAT_PER_SECTION,
            hune_core::limits::MAX_REPEAT_PER_DASHBOARD
        ),
        (24, 12, 120)
    );
    let sources = start_log_sources_json(&loaded);
    for (key, ..) in KEYS {
        assert!(sources.contains(&format!("\"{key}\":\"default\"")), "{sources}");
    }
}

#[test]
fn the_keys_in_the_file_give_their_values_and_the_source_file() {
    let fx = Fx::new();
    let loaded = fx
        .load(&with(
            &fx,
            &[("repeat_max_per_panel", 10), ("repeat_max_per_section", 5), ("repeat_max_per_dashboard", 60)],
        ))
        .unwrap();
    let s = &loaded.config.server;
    assert_eq!((s.repeat_max_per_panel, s.repeat_max_per_section, s.repeat_max_per_dashboard), (10, 5, 60));
    let printed = render_print_config(&loaded);
    assert!(printed.contains("server.repeat_max_per_panel = 10  # file\n"), "{printed}");
    assert!(printed.contains("server.repeat_max_per_section = 5  # file\n"));
    assert!(printed.contains("server.repeat_max_per_dashboard = 60  # file\n"));
    let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
    assert_eq!(config["server.repeat_max_per_dashboard"], 60);
    assert!(!start_log_sources_json(&loaded).contains("repeat_max"));
}

#[test]
fn the_keys_are_in_the_table_the_reference_and_the_effective_order() {
    let fx = Fx::new();
    let order: Vec<String> = fx.load(&fx.local()).unwrap().effective.iter().map(|e| e.key.clone()).collect();
    let position = |name: &str| order.iter().position(|k| k == name).unwrap();
    assert_eq!(position("server.query_inspect"), position("server.editing") + 1);
    let mut previous = "server.query_inspect";
    for (key, _, default, bound) in KEYS {
        assert_eq!(position(key), position(previous) + 1, "{key}");
        previous = key;
        let desc = SCHEMA.key(key).unwrap();
        assert!(matches!(desc.kind, Kind::Integer { min: 1, max } if max == bound), "{key}");
        assert!(!desc.secret);
        let row = format!("| `{key}` | an integer from 1 to {bound} |");
        assert!(REFERENCE_DOC.contains(&row), "{row}");
        assert!(REFERENCE_DOC.contains(&format!("| never (the key is optional) | never | `{default}` |")), "{key}");
    }
}

#[test]
fn a_value_outside_the_range_stops_the_start_and_the_error_names_the_key_the_range_and_the_value() {
    let fx = Fx::new();
    for (key, attr, _, bound) in KEYS {
        for value in [0, bound + 1, 100_000] {
            let errors = fx.errors(&with(&fx, &[(attr, value)]));
            assert_eq!(errors.len(), 1, "{key} {value}: {errors:?}");
            assert!(errors[0].contains(key), "{}", errors[0]);
            assert!(errors[0].contains(&format!("1 to {bound}")), "{}", errors[0]);
            assert!(errors[0].contains(&value.to_string()), "{}", errors[0]);
        }
        // The lowest value and the bound are valid for the key alone: no clamp.
        for value in [1, bound] {
            let errors = fx.errors(&with(&fx, &[(attr, value)]));
            let contradiction = errors.iter().all(|e| e.contains("repeat_max_per_dashboard"));
            assert!(errors.is_empty() || contradiction, "{key} {value}: {errors:?}");
        }
    }
}

#[test]
fn a_wrong_form_stops_the_start() {
    let fx = Fx::new();
    for (key, attr, _, _) in KEYS {
        // No coercion: a text, a boolean, a fraction and a negative number are not an integer of the range.
        for rhs in ["\"24\"", "true", "2.5", "-1"] {
            let errors = fx.errors(&with_lines(&fx, &[format!("{attr} = {rhs}")]));
            assert_eq!(errors.len(), 1, "{key} {rhs}: {errors:?}");
            assert!(errors[0].contains(key), "{}", errors[0]);
        }
        for line in [format!("{attr}_env = \"HUNE_REPEAT\""), format!("{attr}_file = \"/etc/repeat\"")] {
            let errors = fx.errors(&with_lines(&fx, std::slice::from_ref(&line)));
            assert_eq!(errors.len(), 1, "{line}: {errors:?}");
            assert!(errors[0].contains("accepts only a value in the file"), "{}", errors[0]);
        }
        let twice = with_lines(&fx, &[format!("{attr} = 5"), format!("{attr} = 6")]);
        assert!(fx.errors(&twice).iter().any(|e| e.contains(attr)));
    }
}

#[test]
fn an_unknown_key_in_the_block_stops_the_start() {
    let fx = Fx::new();
    for attr in ["repeat_max_per_row", "repeat_max", "repeat_max_per_panels"] {
        let errors = fx.errors(&with_lines(&fx, &[format!("{attr} = 4")]));
        assert_eq!(errors.len(), 1, "{attr}: {errors:?}");
        assert!(errors[0].contains(attr), "{}", errors[0]);
    }
}

#[test]
fn c28_the_limit_of_a_panel_is_not_more_than_the_limit_of_the_dashboard() {
    let fx = Fx::new();
    let e = fx.errors(&with(&fx, &[("repeat_max_per_panel", 30), ("repeat_max_per_dashboard", 20)]));
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].starts_with("server.repeat_max_per_panel, server.repeat_max_per_dashboard: "), "{}", e[0]);
    assert!(e[0].contains("(30)") && e[0].contains("(20)"), "{}", e[0]);
    // The default of an absent key counts: the default 24 is more than 10.
    let e = fx.errors(&with(&fx, &[("repeat_max_per_dashboard", 10)]));
    assert!(
        e.iter()
            .any(|m| m.starts_with("server.repeat_max_per_panel, server.repeat_max_per_dashboard: ")
                && m.contains("(24)")),
        "{e:?}"
    );
    // Equal values are valid.
    assert!(
        fx.errors(&with(
            &fx,
            &[("repeat_max_per_panel", 12), ("repeat_max_per_section", 12), ("repeat_max_per_dashboard", 12)]
        ))
        .is_empty()
    );
}

#[test]
fn c29_the_limit_of_a_section_is_not_more_than_the_limit_of_the_dashboard() {
    let fx = Fx::new();
    let e = fx.errors(&with(
        &fx,
        &[("repeat_max_per_section", 9), ("repeat_max_per_panel", 5), ("repeat_max_per_dashboard", 8)],
    ));
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].starts_with("server.repeat_max_per_section, server.repeat_max_per_dashboard: "), "{}", e[0]);
    // Both contradictions together give two errors.
    let e = fx.errors(&with(
        &fx,
        &[("repeat_max_per_panel", 30), ("repeat_max_per_section", 20), ("repeat_max_per_dashboard", 10)],
    ));
    assert_eq!(e.len(), 2, "{e:?}");
    // The largest values together are valid.
    assert!(
        fx.errors(&with(
            &fx,
            &[("repeat_max_per_panel", 200), ("repeat_max_per_section", 50), ("repeat_max_per_dashboard", 200)]
        ))
        .is_empty()
    );
}
