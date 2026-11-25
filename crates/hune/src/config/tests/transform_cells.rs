// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the key `server.transform_max_cells` (13.8, 4.35, 9.21a rule 13): an optional
//! integer with the default 10000000, the fixed bound 100000000, a value in the file only, and no coercion.

use super::*;

const KEY: &str = "server.transform_max_cells";

fn with_line(fx: &Fx, line: &str) -> String {
    replace_once(
        &fx.local(),
        "  base_path               = \"/\"\n",
        &format!("  base_path               = \"/\"\n  {line}\n"),
    )
}

fn effective(loaded: &LoadedConfig) -> &EffectiveEntry {
    loaded.effective.iter().find(|e| e.key == KEY).unwrap()
}

#[test]
fn an_absent_key_has_the_default_and_shows_the_source_default() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    assert_eq!(loaded.config.server.transform_max_cells, 10_000_000);
    assert_eq!(keys::TRANSFORM_MAX_CELLS_DEFAULT, 10_000_000);
    let entry = effective(&loaded);
    assert_eq!((entry.value.as_str(), entry.json.as_str(), entry.source.as_str()), ("10000000", "10000000", "default"));
    assert!(render_print_config(&loaded).contains("server.transform_max_cells = 10000000  # default\n"));
    let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
    assert_eq!(config[KEY], 10_000_000);
    assert!(start_log_sources_json(&loaded).contains("\"server.transform_max_cells\":\"default\""));
}

#[test]
fn a_value_in_the_file_gives_its_value_and_the_source_file() {
    let fx = Fx::new();
    // The lowest value, a value between and the bound: each one applies as it is (no clamp).
    for value in [1_u32, 2_500_000, 100_000_000] {
        let loaded = fx.load(&with_line(&fx, &format!("transform_max_cells = {value}"))).unwrap();
        assert_eq!(loaded.config.server.transform_max_cells, value);
        let entry = effective(&loaded);
        assert_eq!((entry.value.clone(), entry.source.as_str()), (value.to_string(), "file"));
        assert!(render_print_config(&loaded).contains(&format!("server.transform_max_cells = {value}  # file\n")));
        let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
        assert_eq!(config[KEY], value);
        assert!(!start_log_sources_json(&loaded).contains("transform_max_cells"));
    }
}

#[test]
fn a_value_outside_the_range_stops_the_start_and_the_error_names_the_key_the_range_and_the_value() {
    let fx = Fx::new();
    for value in ["0", "100000001", "4294967296"] {
        let text = with_line(&fx, &format!("transform_max_cells = {value}"));
        let line = line_of(&text, "transform_max_cells = ");
        assert_eq!(
            fx.errors(&text),
            [format!("{KEY}: expected an integer from 1 to 100000000, found {value} (line {line})")],
            "{value}"
        );
    }
}

#[test]
fn a_wrong_type_stops_the_start_with_no_coercion() {
    let fx = Fx::new();
    // A text, a fraction, an exponent, a sign, a zero at the start, a boolean and a list are not an integer of the range.
    for rhs in ["\"10000000\"", "10000000.0", "2.5", "1e7", "-1", "+5", "010", "true", "[5]"] {
        let errors = fx.errors(&with_line(&fx, &format!("transform_max_cells = {rhs}")));
        assert_eq!(errors.len(), 1, "{rhs}: {errors:?}");
        assert!(errors[0].starts_with(&format!("{KEY}: ")), "{}", errors[0]);
        assert!(errors[0].contains("expected an integer from 1 to 100000000"), "{}", errors[0]);
    }
    let errors = fx.errors(&with_line(&fx, "transform_max_cells = \"10000000\""));
    assert!(errors[0].contains("found \"10000000\""), "{}", errors[0]);
    let errors = fx.errors(&with_line(&fx, "transform_max_cells = 2.5"));
    assert!(errors[0].contains("found 2.5"), "{}", errors[0]);
}

#[test]
fn the_forms_env_and_file_a_duplicate_and_a_near_name_stop_the_start() {
    let fx = Fx::new();
    for line in ["transform_max_cells_env = \"HUNE_CELLS\"", "transform_max_cells_file = \"/etc/cells\""] {
        let errors = fx.errors(&with_line(&fx, line));
        assert_eq!(errors.len(), 1, "{line}: {errors:?}");
        assert!(errors[0].contains(KEY) && errors[0].contains("accepts only a value in the file"), "{}", errors[0]);
    }
    let twice = fx.errors(&with_line(&fx, "transform_max_cells = 5\n  transform_max_cells = 6"));
    assert!(!twice.is_empty() && twice.iter().all(|e| e.contains("transform_max_cells")), "{twice:?}");
    for near in ["transform_max_cell", "max_transform_cells", "transform_max_cells_count"] {
        let errors = fx.errors(&with_line(&fx, &format!("{near} = 5")));
        assert_eq!(errors.len(), 1, "{near}: {errors:?}");
        assert!(errors[0].contains(near), "{}", errors[0]);
    }
}

#[test]
fn the_environment_never_gives_the_value() {
    // Rule 5: the file is the only source. A variable with a likely name changes nothing.
    let fx = Fx::new();
    let env = [("HUNE_TRANSFORM_MAX_CELLS", "5"), ("TRANSFORM_MAX_CELLS", "5"), ("server.transform_max_cells", "5")];
    let loaded = fx.load_env(&fx.local(), &env).unwrap();
    assert_eq!(loaded.config.server.transform_max_cells, 10_000_000);
    assert_eq!(effective(&loaded).source, "default");
    let loaded = fx.load_env(&with_line(&fx, "transform_max_cells = 7"), &env).unwrap();
    assert_eq!(loaded.config.server.transform_max_cells, 7);
}

#[test]
fn the_key_is_in_the_table_the_reference_and_the_effective_order() {
    let key = SCHEMA.key(KEY).unwrap();
    assert!(matches!(key.kind, Kind::Integer { min: 1, max: 100_000_000 }));
    assert_eq!(u64::from(keys::TRANSFORM_MAX_CELLS_BOUND), 100_000_000);
    assert!(!key.secret);
    assert_eq!(key.forms, hclconf::Forms::V);
    assert!(REFERENCE_DOC.contains(
        "| `server.transform_max_cells` | an integer from 1 to 100000000 | no | V | never (the key is optional) | never | `10000000` |"
    ));
    // The reason of the bound is in the key table, so it is in the reference.
    assert!(key.description.contains("9 bytes") && key.description.contains("100000000"));
    let fx = Fx::new();
    let order: Vec<String> = fx.load(&fx.local()).unwrap().effective.iter().map(|e| e.key.clone()).collect();
    let position = |name: &str| order.iter().position(|k| k == name).unwrap();
    assert_eq!(position(KEY), position("server.compress_static") + 1);
    assert_eq!(position("server.list_filters_min_dashboards"), position(KEY) + 1);
    // The reference files do not hold the key: its default applies.
    for profile in [Profile::Local, Profile::Distributed] {
        let text = render_reference(ReferenceFormat::Hcl, profile);
        assert!(!text.contains("transform_max_cells ="), "{profile:?}");
        assert!(text.contains("the seven keys that have a default") && text.contains("`server.transform_max_cells`"));
    }
}

#[test]
fn the_texts_of_the_defaults_name_each_key_with_a_default() {
    // The intro of the reference and the head of the reference files list the keys of `defaulted_keys`, with the right count.
    assert!(REFERENCE_DOC.contains("Seven keys have a default value and can be absent: "));
    assert!(REFERENCE_DOC.contains("except for the seven keys that have a default value"));
    let hcl = render_reference(ReferenceFormat::Hcl, Profile::Local);
    for (key, default, _) in defaulted_keys() {
        let shown = if matches!(SCHEMA.key(key).unwrap().kind, Kind::Bool) { format!("`{default}`") } else { default };
        assert!(REFERENCE_DOC.contains(&format!("`{key}` (default {shown})")), "{key}");
        assert!(hcl.lines().nth(1).unwrap().contains(&format!("`{key}`")), "{key}");
    }
    assert_eq!(defaulted_keys().len(), 7);
}
