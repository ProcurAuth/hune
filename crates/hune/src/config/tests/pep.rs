// SPDX-License-Identifier: MIT OR Apache-2.0
//! Stage 2 builds the `PepConfig` and the action document in the mode `authzen` (13.13). Neither call
//! uses the network. `check-config`, `print-config` and `serve` find the same errors.

use super::*;

#[test]
fn a_good_file_passes_the_pep_checks_and_the_access_layer_builds_with_no_network() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.distributed()).expect("the file DISTRIBUTED is good");
    let access = crate::access::Access::from_config(
        &loaded.config,
        crate::log::Logger::memory(LogLevel::Debug),
        std::sync::Arc::new(crate::metrics::Metrics::new("0.1.0", "t", 0)),
    )
    .expect("the PEP builds")
    .expect("the mode is authzen");
    assert_eq!(access.pep().service_id(), "dashboards");
    // The mode `open` builds no access layer.
    let open = fx.load(&fx.local()).unwrap();
    assert!(
        crate::access::Access::from_config(
            &open.config,
            crate::log::Logger::memory(LogLevel::Debug),
            std::sync::Arc::new(crate::metrics::Metrics::new("0.1.0", "t", 0)),
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn a_pdp_token_with_a_character_outside_the_visible_ascii_stops_the_start_and_hides_the_value() {
    let fx = Fx::new();
    crate::config::test_support::write_file(fx.dir.path().join("pdp-token"), "pdp t\u{f6}ken\n").unwrap();
    let errors = fx.load(&fx.distributed()).unwrap_err();
    // Either the key table or the PEP check refuses the value. The error names the key and hides the value.
    let e = errors.iter().find(|e| e.starts_with("access.pdp.token")).unwrap_or_else(|| panic!("{errors:?}"));
    assert!(!e.contains("pdp t"), "{e}");
}

#[test]
fn the_error_of_the_pep_has_the_key_of_the_file_and_its_line() {
    let fx = Fx::new();
    crate::config::test_support::write_file(fx.dir.path().join("pdp-token"), "t\u{e9}ken\n").unwrap();
    let text = fx.distributed();
    let errors = fx.load(&text).unwrap_err();
    assert!(!errors.is_empty());
    let line = line_of(&text, "token_file");
    assert!(
        errors.iter().all(|e| e.starts_with("access.pdp.token: ") && e.ends_with(&format!("(line {line})"))),
        "{errors:?}"
    );
}

#[test]
fn the_scheme_error_stays_the_contradiction_c5_and_not_a_pep_error() {
    let fx = Fx::new();
    let text = replace_once(&fx.distributed(), "https://pdp.example:4182", "http://pdp.example:4182");
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].starts_with(
            "access.pdp.plain_http, access.pdp.url: `access.pdp.plain_http` (false) needs the scheme https"
        )
    );
}

#[test]
fn the_mode_open_runs_no_pep_check() {
    let fx = Fx::new();
    assert!(fx.load(&fx.local()).is_ok());
}
