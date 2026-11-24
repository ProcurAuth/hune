// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the key `server.list_filters_min_dashboards` (13.8, 9.7): an optional integer with the default 6,
//! the range 0 to 1000, a value in the file only, and no coercion.

use super::*;

const KEY: &str = "server.list_filters_min_dashboards";

fn with_line(fx: &Fx, line: &str) -> String {
    replace_once(
        &fx.local(),
        "  editing                 = true\n",
        &format!("  editing                 = true\n  {line}\n"),
    )
}

fn effective(loaded: &LoadedConfig) -> &EffectiveEntry {
    loaded.effective.iter().find(|e| e.key == KEY).unwrap()
}

#[test]
fn an_absent_key_has_the_default_and_shows_the_source_default() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    assert_eq!(loaded.config.server.list_filters_min_dashboards, 6);
    assert_eq!(keys::LIST_FILTERS_MIN_DEFAULT, 6);
    let entry = effective(&loaded);
    assert_eq!((entry.value.as_str(), entry.json.as_str(), entry.source.as_str()), ("6", "6", "default"));
    assert!(render_print_config(&loaded).contains("server.list_filters_min_dashboards = 6  # default\n"));
    let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
    assert_eq!(config[KEY], 6);
    assert!(start_log_sources_json(&loaded).contains("\"server.list_filters_min_dashboards\":\"default\""));
}

#[test]
fn a_value_in_the_file_gives_its_value_and_the_source_file() {
    let fx = Fx::new();
    // The lowest value, a value between and the bound: each one applies as it is (no clamp).
    for value in [0_u32, 1, 25, 1000] {
        let loaded = fx.load(&with_line(&fx, &format!("list_filters_min_dashboards = {value}"))).unwrap();
        assert_eq!(loaded.config.server.list_filters_min_dashboards, value);
        let entry = effective(&loaded);
        assert_eq!((entry.value.clone(), entry.source.as_str()), (value.to_string(), "file"));
        assert!(
            render_print_config(&loaded).contains(&format!("server.list_filters_min_dashboards = {value}  # file\n"))
        );
        let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
        assert_eq!(config[KEY], value);
        assert!(!start_log_sources_json(&loaded).contains("list_filters_min_dashboards"));
    }
}

#[test]
fn a_value_outside_the_range_stops_the_start_and_the_error_names_the_key_the_range_and_the_value() {
    let fx = Fx::new();
    for value in ["1001", "4294967296"] {
        let text = with_line(&fx, &format!("list_filters_min_dashboards = {value}"));
        let line = line_of(&text, "list_filters_min_dashboards = ");
        assert_eq!(
            fx.errors(&text),
            [format!("{KEY}: expected an integer from 0 to 1000, found {value} (line {line})")],
            "{value}"
        );
    }
}

#[test]
fn a_wrong_type_stops_the_start_with_no_coercion() {
    let fx = Fx::new();
    // A text, a fraction, an exponent, a sign, a zero at the start, a boolean and a list are not an integer of the range.
    for rhs in ["\"6\"", "6.0", "2.5", "1e1", "-1", "+5", "010", "true", "[5]"] {
        let errors = fx.errors(&with_line(&fx, &format!("list_filters_min_dashboards = {rhs}")));
        assert_eq!(errors.len(), 1, "{rhs}: {errors:?}");
        assert!(errors[0].starts_with(&format!("{KEY}: ")), "{}", errors[0]);
        assert!(errors[0].contains("expected an integer from 0 to 1000"), "{}", errors[0]);
    }
    let errors = fx.errors(&with_line(&fx, "list_filters_min_dashboards = \"6\""));
    assert!(errors[0].contains("found \"6\""), "{}", errors[0]);
    let errors = fx.errors(&with_line(&fx, "list_filters_min_dashboards = 2.5"));
    assert!(errors[0].contains("found 2.5"), "{}", errors[0]);
}

#[test]
fn the_forms_env_and_file_a_duplicate_and_a_near_name_stop_the_start() {
    let fx = Fx::new();
    for line in
        ["list_filters_min_dashboards_env = \"HUNE_FILTERS\"", "list_filters_min_dashboards_file = \"/etc/filters\""]
    {
        let errors = fx.errors(&with_line(&fx, line));
        assert_eq!(errors.len(), 1, "{line}: {errors:?}");
        assert!(errors[0].contains(KEY) && errors[0].contains("accepts only a value in the file"), "{}", errors[0]);
    }
    let twice = fx.errors(&with_line(&fx, "list_filters_min_dashboards = 5\n  list_filters_min_dashboards = 6"));
    assert!(!twice.is_empty() && twice.iter().all(|e| e.contains("list_filters_min_dashboards")), "{twice:?}");
    for near in ["list_filters_min_dashboard", "list_filter_min_dashboards", "list_filters_min"] {
        let errors = fx.errors(&with_line(&fx, &format!("{near} = 5")));
        assert_eq!(errors.len(), 1, "{near}: {errors:?}");
        assert!(errors[0].contains(near), "{}", errors[0]);
    }
}

#[test]
fn the_environment_never_gives_the_value() {
    // Rule 5: the file is the only source. A variable with a likely name changes nothing.
    let fx = Fx::new();
    let env = [
        ("HUNE_LIST_FILTERS_MIN_DASHBOARDS", "9"),
        ("LIST_FILTERS_MIN_DASHBOARDS", "9"),
        ("server.list_filters_min_dashboards", "9"),
    ];
    let loaded = fx.load_env(&fx.local(), &env).unwrap();
    assert_eq!(loaded.config.server.list_filters_min_dashboards, 6);
    assert_eq!(effective(&loaded).source, "default");
    let loaded = fx.load_env(&with_line(&fx, "list_filters_min_dashboards = 7"), &env).unwrap();
    assert_eq!(loaded.config.server.list_filters_min_dashboards, 7);
}

#[test]
fn the_key_is_in_the_table_the_reference_and_the_effective_order() {
    let key = SCHEMA.key(KEY).unwrap();
    assert!(matches!(key.kind, Kind::Integer { min: 0, max: 1000 }));
    assert_eq!(u64::from(keys::LIST_FILTERS_MIN_BOUND), 1000);
    assert!(!key.secret);
    assert_eq!(key.forms, hclconf::Forms::V);
    assert!(REFERENCE_DOC.contains(
        "| `server.list_filters_min_dashboards` | an integer from 0 to 1000 | no | V | never (the key is optional) | never | `6` |"
    ));
    let fx = Fx::new();
    let order: Vec<String> = fx.load(&fx.local()).unwrap().effective.iter().map(|e| e.key.clone()).collect();
    let position = |name: &str| order.iter().position(|k| k == name).unwrap();
    assert_eq!(position(KEY), position("server.transform_max_cells") + 1);
    assert_eq!(position("server.max_connections"), position(KEY) + 1);
    // The reference files do not hold the key: its default applies.
    for profile in [Profile::Local, Profile::Distributed] {
        let text = render_reference(ReferenceFormat::Hcl, profile);
        assert!(!text.contains("list_filters_min_dashboards ="), "{profile:?}");
        assert!(
            text.contains("the seven keys that have a default")
                && text.contains("`server.list_filters_min_dashboards`")
        );
    }
}
