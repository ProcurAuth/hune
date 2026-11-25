// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tests of the read modes of 13.18: `ReadMode::StoreCommands`, `ImportWrite` and `Nothing`.
//!
//! Each test starts from the example file DISTRIBUTED. The files and the variable of the blocks
//! outside `storage` do not exist, so only a mode that does not read them can load the file.

use super::*;

/// The paths of the files of the block `storage`: they exist in the test directory.
const STORAGE_FILES: [&str; 3] = ["pg-user", "pg-pw", "pg-ca.crt"];

impl Fx {
    /// The file DISTRIBUTED. The files of the block `storage` exist. The password file and the
    /// CA file of the `clickhouse` block, the CA file of the PDP and the key file do not exist,
    /// and the variable of the PDP token is not set.
    fn without_outside_files(&self) -> String {
        let mut out = DISTRIBUTED.to_string();
        for (original, name, _) in FILES {
            let target =
                if STORAGE_FILES.contains(&name) { self.path(name) } else { self.path(&format!("missing/{name}")) };
            out = out.replace(original, &target);
        }
        out.replace(
            &format!("token_file      = \"{}\"", self.path("missing/pdp-token")),
            "token_env       = \"HUNE_TEST_NOT_SET\"",
        )
    }

    fn load_mode(&self, text: &str, mode: ReadMode) -> Result<LoadedConfig, Vec<String>> {
        let file = self.dir.path().join("hune.hcl");
        crate::config::test_support::write_file(&file, text).unwrap();
        load_with(&file, &CapturedEnvironment::default(), mode)
            .map_err(|list| list.iter().map(|e| self.back(&e.to_string())).collect())
    }

    fn mode_errors(&self, text: &str, mode: ReadMode) -> Vec<String> {
        self.load_mode(text, mode).err().unwrap_or_default()
    }
}

fn entry<'a>(loaded: &'a LoadedConfig, key: &str) -> &'a EffectiveEntry {
    loaded.effective.iter().find(|e| e.key == key).unwrap_or_else(|| panic!("{key}"))
}

/// Apply a change to the text of the test (outside files missing) and to the file DISTRIBUTED
/// (all files exist). `load` reaches stage 2 only with the second text.
fn both(fx: &Fx, change: impl Fn(&str) -> String) -> (String, String) {
    (change(&fx.without_outside_files()), change(&fx.distributed()))
}

fn has_error(errors: &[String], needle: &str) -> bool {
    errors.iter().any(|e| e.contains(needle))
}

const PARTIAL: [ReadMode; 2] = [ReadMode::StoreCommands, ReadMode::ImportWrite];

#[test]
fn the_text_of_the_test_is_a_good_file_for_load_only_with_its_files() {
    let fx = Fx::new();
    let text = fx.without_outside_files();
    // `load` reads every value, so it fails on the files and the variable that do not exist.
    let errors = fx.errors(&text);
    assert!(has_error(&errors, "clickhouse \"prod\".password: cannot read the file"), "{errors:#?}");
    assert!(has_error(&errors, "access.pdp.token: the environment variable HUNE_TEST_NOT_SET is not set"));
    assert!(has_error(&errors, "access.identity.public_keys: cannot read the file"));
    // The same text with all the files and the variable loads with `load`.
    assert!(fx.load_env(&fx.distributed(), &[]).is_ok());
}

#[test]
fn a_partial_mode_loads_a_file_whose_other_blocks_have_no_files() {
    for mode in PARTIAL {
        let fx = Fx::new();
        let loaded = fx.load_mode(&fx.without_outside_files(), mode).unwrap_or_else(|e| panic!("{mode:?}: {e:#?}"));
        assert_eq!(loaded.mode, mode);
        assert!(!loaded.mode.is_complete());
        // The block `storage` is read.
        let Some(pg) = &loaded.config.storage.postgres else { panic!("postgres expected") };
        assert_eq!(pg.user, "app");
        assert_eq!(pg.password.expose(), "pgpass");
        assert!(!pg.tls.as_ref().unwrap().ca_pem.is_empty());
        assert_eq!(entry(&loaded, "storage.postgres.user").value, "\"app\"");
        assert_eq!(entry(&loaded, "storage.postgres.password").value, "<hidden>");
        // The other values from `_env` and `_file` are not read, and they keep their source.
        let password = entry(&loaded, "clickhouse \"prod\".password");
        assert_eq!(password.value, "<not read>");
        assert_eq!(password.json, "\"<not read>\"");
        assert!(password.source.starts_with("file:") && password.source.ends_with("/missing/ch-pw"), "{password:?}");
        assert_eq!(entry(&loaded, "access.pdp.token").value, "<not read>");
        assert_eq!(entry(&loaded, "access.pdp.token").source, "env:HUNE_TEST_NOT_SET");
        assert_eq!(entry(&loaded, "access.identity.public_keys").value, "<not read>");
        // The values of the file are read.
        assert_eq!(entry(&loaded, "clickhouse \"prod\".user").value, "\"hune\"");
        assert_eq!(entry(&loaded, "access.pdp.url").value, "\"https://pdp.example:4182\"");
        // The typed configuration holds placeholders. A secret never shows.
        let ch = &loaded.config.clickhouse[0];
        assert_eq!(ch.id, "prod");
        assert_eq!(ch.password.expose(), "");
        assert!(ch.tls.as_ref().unwrap().ca_pem.is_empty());
        let AccessConfig::Authzen(authzen) = &loaded.config.access else { panic!("authzen expected") };
        assert_eq!(authzen.identity.public_keys, "");
        assert_eq!(authzen.pdp.token.expose(), "");
        assert_eq!(authzen.pdp.ca_pem, None);
        assert!(authzen.pdp.ca_file.is_some());
        let shown = format!("{loaded:?}");
        assert!(!shown.contains("pgpass"), "{shown}");
        // The sources of the start log list the keys whose source is not the file.
        assert!(start_log_sources_json(&loaded).contains("\"access.pdp.token\":\"env:HUNE_TEST_NOT_SET\""));
    }
}

#[test]
fn load_still_reads_everything() {
    let fx = Fx::new();
    let text = fx.without_outside_files();
    let all = fx.mode_errors(&text, ReadMode::All);
    assert_eq!(all, fx.errors(&text));
    assert!(!all.is_empty());
    let loaded = fx.load_mode(&fx.distributed(), ReadMode::All).unwrap();
    assert!(loaded.mode.is_complete());
    assert_eq!(loaded.config.clickhouse[0].password.expose(), "chpass");
    assert_eq!(entry(&loaded, "clickhouse \"prod\".password").value, "<hidden>");
}

#[test]
fn a_failure_inside_storage_still_fails_in_each_partial_mode() {
    for mode in PARTIAL {
        let fx = Fx::new();
        let good = fx.without_outside_files();
        // A password file of `storage` that does not exist.
        let text = good.replace(&fx.path("pg-pw"), &fx.path("missing/pg-pw"));
        let errors = fx.mode_errors(&text, mode);
        assert!(has_error(&errors, "storage.postgres.password: cannot read the file"), "{mode:?}: {errors:#?}");
        // A variable of `storage` that is not set.
        let text = good.replace(
            &format!("user_file         = \"{}\"", fx.path("pg-user")),
            "user_env          = \"HUNE_TEST_NOT_SET\"",
        );
        let errors = fx.mode_errors(&text, mode);
        assert_eq!(
            errors,
            vec![format!(
                "storage.postgres.user: the environment variable HUNE_TEST_NOT_SET is not set (line {})",
                line_of(&text, "user_env")
            )]
        );
        // The check of 13.13 of the CA file of `storage`.
        let text = good.replace(&fx.path("pg-ca.crt"), &fx.path("missing/pg-ca.crt"));
        let errors = fx.mode_errors(&text, mode);
        assert!(has_error(&errors, "storage.postgres.tls.ca_file"), "{mode:?}: {errors:#?}");
        // The key table still checks the block `storage`.
        let text = good.replace("max_connections   = 8", "max_connections   = 0");
        assert!(has_error(&fx.mode_errors(&text, mode), "storage.postgres.max_connections"));
    }
}

#[test]
fn a_partial_mode_skips_the_checks_of_13_13_outside_storage() {
    for mode in PARTIAL {
        let fx = Fx::new();
        let good = fx.without_outside_files();
        // The CA file of the `clickhouse` block does not exist (the files of `missing` do not exist).
        assert!(good.contains(&fx.path("missing/ch-ca.pem")));
        assert!(fx.load_mode(&good, mode).is_ok());
        // A bad value of 13.13 outside `storage`: `load` refuses it, the partial mode does not.
        let changes: [(&str, &str, &str); 2] = [
            ("base_path               = \"/\"", "base_path               = \"no-slash\"", "server.base_path"),
            ("url             = \"https://pdp.example:4182\"", "url             = \"not a url\"", "access.pdp.url"),
        ];
        for (from, to, key) in &changes {
            let (partial, full) = both(&fx, |t| t.replace(from, to));
            assert!(fx.load_mode(&partial, mode).is_ok(), "{mode:?} {key}");
            assert!(has_error(&fx.errors(&full), key), "{key}");
        }
        let (partial, _) =
            both(&fx, |t| t.replace("database           = \"analytics\"", "database           = \"1-bad\""));
        assert!(fx.load_mode(&partial, mode).is_ok(), "{mode:?}");
        // A telemetry block is not a check of `storage`: the full mode accepts it (13.9), the partial mode too.
        let (partial, full) = both(&fx, |t| {
            t.replace(
                "telemetry {\n  enabled = false\n}",
                "telemetry {\n  enabled = true\n  otlp_endpoint = \"http://otel:4318\"\n  service_name = \"hune\"\n  export_interval = \"15s\"\n}",
            )
        });
        assert!(fx.errors(&full).is_empty(), "{:?}", fx.errors(&full));
        assert!(fx.load_mode(&partial, mode).is_ok(), "{mode:?}");
        let (partial, _) = both(&fx, |t| {
            t.replace(
                "telemetry {\n  enabled = false\n}",
                "telemetry {\n  enabled = true\n  otlp_endpoint_env = \"HUNE_TEST_NOT_SET\"\n  service_name = \"hune\"\n  export_interval = \"15s\"\n}",
            )
        });
        let loaded = fx.load_mode(&partial, mode).unwrap();
        let TelemetryConfig::Enabled(t) = &loaded.config.telemetry else { panic!("enabled expected") };
        assert_eq!(t.otlp_endpoint, "");
        assert_eq!(entry(&loaded, "telemetry.otlp_endpoint").value, "<not read>");
    }
}

#[test]
fn the_labels_of_the_clickhouse_blocks_are_checked_by_import_write_only() {
    let fx = Fx::new();
    let text = fx.without_outside_files().replace("clickhouse \"prod\"", "clickhouse \"Prod\"");
    assert!(fx.load_mode(&text, ReadMode::StoreCommands).is_ok());
    let errors = fx.mode_errors(&text, ReadMode::ImportWrite);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].starts_with("clickhouse \"Prod\": expected a label that matches"), "{errors:#?}");
    let full = fx.distributed().replace("clickhouse \"prod\"", "clickhouse \"Prod\"");
    assert!(has_error(&fx.errors(&full), "clickhouse \"Prod\": expected a label"));
    // A good label loads, and the typed configuration holds the labels.
    let good = fx.load_mode(&fx.without_outside_files(), ReadMode::ImportWrite).unwrap();
    assert_eq!(good.config.clickhouse.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["prod"]);
}

#[test]
fn the_form_rules_still_apply_to_the_values_that_are_not_read() {
    let fx = Fx::new();
    let good = fx.without_outside_files();
    let password = format!("password_file              = \"{}\"", fx.path("missing/ch-pw"));
    assert!(good.contains(&password));
    let cases: [(&str, &str); 7] = [
        ("password           = \"clear\"", "a secret accepts only `password_env` or `password_file`"),
        ("password_env       = \"A\"\n  password_file      = \"/x/y\"", "the file gives `password_env` at line"),
        ("password_env       = \"1BAD\"", "expected an environment variable name that matches"),
        ("password_env       = \"A-B\"", "expected an environment variable name that matches"),
        ("password_env       = \"SSL_CERT_FILE\"", "other than SSL_CERT_FILE and SSL_CERT_DIR"),
        ("password_file      = \"relative/path\"", "expected an absolute path of 1 to 4096 bytes"),
        ("password_file      = 5", "expected an absolute path of 1 to 4096 bytes"),
    ];
    for mode in [ReadMode::StoreCommands, ReadMode::ImportWrite, ReadMode::Nothing] {
        for (replacement, expected) in cases {
            let text = good.replace(&password, replacement);
            let errors = fx.mode_errors(&text, mode);
            assert!(
                errors.iter().any(|e| e.starts_with("clickhouse \"prod\".password: ") && e.contains(expected)),
                "{mode:?} {replacement}: {errors:#?}"
            );
            assert!(!errors.iter().any(|e| e.contains("clear\"")), "no value in the message");
        }
        // A necessary secret that no form gives.
        let errors = fx.mode_errors(&good.replace(&format!("  {password}\n"), ""), mode);
        assert!(has_error(&errors, "clickhouse \"prod\".password: missing secret"), "{mode:?}: {errors:#?}");
        // A key that refuses the indirection.
        let errors =
            fx.mode_errors(&good.replace("max_concurrent             = 16", "max_concurrent_env = \"A\""), mode);
        assert!(has_error(&errors, "accepts only a value in the file"), "{mode:?}: {errors:#?}");
        // The other keys of the key table are checked in each mode.
        let errors =
            fx.mode_errors(&good.replace("max_concurrent             = 16", "max_concurrent             = 0"), mode);
        assert!(has_error(&errors, "clickhouse \"prod\".max_concurrent"), "{mode:?}: {errors:#?}");
    }
}

#[test]
fn the_contradiction_checks_run_for_the_values_that_are_read() {
    for mode in PARTIAL {
        let fx = Fx::new();
        let good = fx.without_outside_files();
        // C9: only keys of the file.
        let text =
            good.replace("ops_listen              = \"0.0.0.0:9090\"", "ops_listen              = \"0.0.0.0:8080\"");
        let errors = fx.mode_errors(&text, mode);
        assert!(has_error(&errors, "server.listen, server.ops_listen"), "{mode:?}: {errors:#?}");
        // C12 and C11.
        let text = good.replace("max_concurrent_per_subject = 6", "max_concurrent_per_subject = 60");
        assert!(has_error(&fx.mode_errors(&text, mode), "max_concurrent_per_subject"), "{mode:?}");
        // C7: the URL of the `clickhouse` block is in the file, so the check runs. `https` needs `tls`.
        let text = good.replace(
            "url                        = \"https://clickhouse:8443\"",
            "url                        = \"http://clickhouse:8123\"",
        );
        let errors = fx.mode_errors(&text, mode);
        assert!(has_error(&errors, "forbids the block tls"), "{mode:?}: {errors:#?}");
        // C5: the URL of the PDP is in the file, so the check runs. `plain_http = true` forbids `ca_file`.
        let ca = format!("    ca_file         = \"{}\"\n", fx.path("missing/pdp-ca.pem"));
        assert!(good.contains(&ca));
        let text = good.replace(&ca, "").replace("plain_http      = false", "plain_http      = true");
        assert!(has_error(&fx.mode_errors(&text, mode), "access.pdp.plain_http, access.pdp.url"), "{mode:?}");
        // C5 does not run when the URL is a value that the mode does not read.
        let text =
            text.replace("url             = \"https://pdp.example:4182\"", "url_env         = \"HUNE_TEST_NOT_SET\"");
        let loaded = fx.load_mode(&text, mode).unwrap_or_else(|e| panic!("{mode:?}: {e:#?}"));
        assert_eq!(entry(&loaded, "access.pdp.url").value, "<not read>");
        let full = fx
            .distributed()
            .replace("url             = \"https://pdp.example:4182\"", "url_env         = \"HUNE_TEST_NOT_SET\"");
        assert!(has_error(&fx.errors(&full), "access.pdp.url: the environment variable"));
    }
}

#[test]
fn the_mode_nothing_reads_no_value_and_checks_the_values_of_the_file() {
    let fx = Fx::new();
    // The files of `storage` do not exist either.
    let mut text = fx.without_outside_files();
    for name in STORAGE_FILES.iter().filter(|n| **n != "pg-ca.crt") {
        text = text.replace(&fx.path(name), &fx.path(&format!("missing/{name}")));
    }
    // The CA files are values of the file (a key of the form V). The mode reads and checks them.
    let errors = fx.mode_errors(&text, ReadMode::Nothing);
    assert!(has_error(&errors, "clickhouse \"prod\".tls.ca_file"), "{errors:#?}");
    assert!(has_error(&errors, "access.pdp.ca_file"), "{errors:#?}");
    assert!(!has_error(&errors, "password") && !has_error(&errors, "storage.postgres.user"), "{errors:#?}");
    // With good CA files, the mode loads the file and reads no value.
    let text = text
        .replace(&fx.path("missing/ch-ca.pem"), &fx.path("ch-ca.pem"))
        .replace(&fx.path("missing/pdp-ca.pem"), &fx.path("pdp-ca.pem"));
    let loaded = fx.load_mode(&text, ReadMode::Nothing).unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(entry(&loaded, "storage.postgres.user").value, "<not read>");
    assert_eq!(entry(&loaded, "storage.postgres.password").value, "<not read>");
    assert_eq!(entry(&loaded, "storage.postgres.password").source, format!("file:{}", fx.path("missing/pg-pw")));
    let Some(pg) = &loaded.config.storage.postgres else { panic!("postgres expected") };
    assert_eq!(pg.user, "");
    assert_eq!(pg.password.expose(), "");
    // A bad value of the file is an error in this mode.
    let bad = text.replace("base_path               = \"/\"", "base_path               = \"x\"");
    assert!(has_error(&fx.mode_errors(&bad, ReadMode::Nothing), "server.base_path"));
}

#[test]
fn the_mode_local_reads_its_volume_and_no_clickhouse_file() {
    let fx = Fx::new();
    let text = fx
        .local()
        .replace(&fx.path("ch-pw"), &fx.path("missing/ch-pw"))
        .replace(&fx.path("ch-ca.pem"), &fx.path("missing/ch-ca.pem"));
    for mode in PARTIAL {
        let loaded = fx.load_mode(&text, mode).unwrap_or_else(|e| panic!("{mode:?}: {e:#?}"));
        let v = &loaded.config.storage.volumes[0];
        assert_eq!(v.path.to_str(), Some("/var/lib/hune/dashboards"));
    }
    assert!(fx.errors(&text).iter().any(|e| e.contains("clickhouse \"prod\".password")));
}
