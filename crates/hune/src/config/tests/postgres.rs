// SPDX-License-Identifier: MIT OR Apache-2.0
//! Case K16 of 16.5: the block `postgres "<id>"` (13.12a), the contradictions C14 to C20, the
//! warning W7, the output of `print-config` and the reference document.
//!
//! Each test starts from the example file LOCAL (mode `open`) or DISTRIBUTED (mode `authzen`) and
//! the example block of 13.12a. The secret files that the block names exist in the test directory.

use super::*;

/// The example block of 13.12a. `authzen` keeps the key `max_concurrent_per_subject`, which the
/// mode `open` forbids.
pub(super) fn block(authzen: bool) -> String {
    let mut text = r#"postgres "metrics" {
  label                      = "Metrics"
  host                       = "hune-pg-ro"
  port                       = 5432
  database                   = "analytics"
  user_file                  = "/run/secrets/postgres-metrics/username"
  password_file              = "/run/secrets/postgres-metrics/password"
  search_path                = "metrics"
  sslmode                    = "verify-full"
  connect_timeout            = "5s"
  max_execution_time         = "1m"
  max_result_rows            = 100000
  max_result_bytes           = 33554432
  max_query_size             = 1048576
  max_concurrent             = 16
  queue_wait                 = "5s"
  max_concurrent_per_subject = 6

  tls {
    ca_file     = "/run/secrets/postgres-metrics-ca/ca.crt"
    server_name = "hune-pg-ro"
  }
}
"#
    .to_string();
    if !authzen {
        text = text.replace("  max_concurrent_per_subject = 6\n", "");
    }
    text
}

/// The file LOCAL with the example block.
fn open_file(fx: &Fx) -> String {
    append(&fx.local(), &fx.substitute(&block(false)))
}

/// The file DISTRIBUTED with the example block.
fn authzen_file(fx: &Fx) -> String {
    append(&fx.distributed(), &fx.substitute(&block(true)))
}

/// The file LOCAL with the example block, without TLS: `sslmode = "disable"` and no block `tls`.
fn disabled_file(fx: &Fx) -> String {
    set(&remove_tls_of_postgres(&open_file(fx)), "postgres.sslmode", "\"disable\"")
}

/// Give the attribute `old` of the block `postgres` another name and another value, for example
/// `user_file` to `user_env`.
fn use_form(text: &str, old: &str, new: &str, rhs: &str) -> String {
    let at = locate(text, "postgres", old).unwrap_or_else(|| panic!("{old}"));
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines[at] = format!("  {new} = {rhs}");
    lines.join("\n") + "\n"
}

/// Remove the block `tls` of the block `postgres`: the last block `tls` of the file.
fn remove_tls_of_postgres(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().rposition(|l| *l == "  tls {").unwrap();
    let last = first + lines[first..].iter().position(|l| *l == "  }").unwrap();
    let mut out: Vec<&str> = lines[..first].to_vec();
    if out.last() == Some(&"") {
        out.pop();
    }
    out.extend(&lines[last + 1..]);
    out.join("\n") + "\n"
}

fn source(loaded: &LoadedConfig, index: usize) -> &hune_source::PostgresSourceConfig {
    &loaded.config.postgres_sources[index]
}

// ---------------------------------------------------------------------------------------
// The example block, the typed result and print-config.
// ---------------------------------------------------------------------------------------

#[test]
fn the_example_block_loads_in_the_mode_open_without_warning_of_its_own() {
    let fx = Fx::new();
    let loaded = fx.load(&open_file(&fx)).unwrap();
    assert!(loaded.warnings.iter().all(|w| w.key == "access.mode"), "{:?}", loaded.warnings);
    assert_eq!(loaded.config.postgres_sources.len(), 1);
    assert!(source(&loaded, 0).max_concurrent_per_subject.is_none());
}

#[test]
fn the_example_block_loads_in_the_mode_authzen_and_gives_no_warning() {
    let fx = Fx::new();
    let loaded = fx.load(&authzen_file(&fx)).unwrap();
    assert!(loaded.warnings.iter().all(|w| w.key == "server.listen"), "{:?}", loaded.warnings);
    assert_eq!(source(&loaded, 0).max_concurrent_per_subject, Some(6));
}

#[test]
fn the_typed_result_holds_every_field_of_the_block() {
    let fx = Fx::new();
    let loaded = fx.load(&authzen_file(&fx)).unwrap();
    let p = source(&loaded, 0);
    assert_eq!((p.id.as_str(), p.label.as_str()), ("metrics", "Metrics"));
    assert_eq!((p.host.as_str(), p.port, p.database.as_str()), ("hune-pg-ro", 5432, "analytics"));
    assert_eq!((p.user.as_str(), p.password.as_str()), ("hune_ro", "pgmpass"));
    assert_eq!(p.search_path, ["metrics"]);
    assert_eq!(p.sslmode, hune_source::PostgresSslMode::VerifyFull);
    let tls = p.tls.as_ref().unwrap();
    assert_eq!(tls.server_name, "hune-pg-ro");
    assert_eq!(tls.ca_pem, CA_A.as_bytes());
    assert_eq!(p.connect_timeout, Duration::from_secs(5));
    assert_eq!(p.max_execution_time, Duration::from_secs(60));
    assert_eq!((p.max_result_rows, p.max_result_bytes, p.max_query_size), (100_000, 33_554_432, 1_048_576));
    assert_eq!((p.max_concurrent, p.queue_wait), (16, Duration::from_secs(5)));
    assert_eq!(p.max_concurrent_per_subject, Some(6));
    // The ClickHouse list keeps its own field.
    assert_eq!(loaded.config.clickhouse.len(), 1);
}

#[test]
fn a_block_with_sslmode_disable_has_no_tls_and_the_search_path_is_a_list() {
    let fx = Fx::new();
    let text = set(&disabled_file(&fx), "postgres.search_path", "\"metrics,public,_x1\"");
    let loaded = fx.load(&text).unwrap();
    let p = source(&loaded, 0);
    assert_eq!(p.sslmode, hune_source::PostgresSslMode::Disable);
    assert!(p.tls.is_none());
    assert_eq!(p.search_path, ["metrics", "public", "_x1"]);
}

#[test]
fn print_config_gives_the_lines_of_13_20_and_hides_the_password() {
    let fx = Fx::new();
    let loaded = fx.load(&authzen_file(&fx)).unwrap();
    let out = fx.back(&render_print_config(&loaded));
    let expected = [
        "postgres \"metrics\".label = \"Metrics\"  # file",
        "postgres \"metrics\".host = \"hune-pg-ro\"  # file",
        "postgres \"metrics\".port = 5432  # file",
        "postgres \"metrics\".database = \"analytics\"  # file",
        "postgres \"metrics\".user = \"hune_ro\"  # file:/run/secrets/postgres-metrics/username",
        "postgres \"metrics\".password = <hidden>  # file:/run/secrets/postgres-metrics/password",
        "postgres \"metrics\".search_path = \"metrics\"  # file",
        "postgres \"metrics\".sslmode = \"verify-full\"  # file",
        "postgres \"metrics\".tls.ca_file = \"/run/secrets/postgres-metrics-ca/ca.crt\"  # file",
        "postgres \"metrics\".tls.server_name = \"hune-pg-ro\"  # file",
        "postgres \"metrics\".connect_timeout = \"5s\"  # file",
        "postgres \"metrics\".max_execution_time = \"1m\"  # file",
        "postgres \"metrics\".max_result_rows = 100000  # file",
        "postgres \"metrics\".max_result_bytes = 33554432  # file",
        "postgres \"metrics\".max_query_size = 1048576  # file",
        "postgres \"metrics\".max_concurrent = 16  # file",
        "postgres \"metrics\".queue_wait = \"5s\"  # file",
        "postgres \"metrics\".max_concurrent_per_subject = 6  # file",
    ];
    let lines: Vec<&str> = out.lines().filter(|l| l.starts_with("postgres \"metrics\"")).collect();
    assert_eq!(lines, expected);
    assert!(!out.contains("pgmpass"));
    // The start log shows the source of each value that does not come from the file, and no secret.
    let config = start_log_config_json(&loaded);
    assert!(!config.contains("pgmpass"));
    assert!(config.contains("\"postgres \\\"metrics\\\".password\":\"<hidden>\""), "{config}");
    let sources = fx.back(&start_log_sources_json(&loaded));
    assert!(sources.contains("\"postgres \\\"metrics\\\".password\":\"file:/run/secrets/postgres-metrics/password\""));
    assert!(sources.contains("\"postgres \\\"metrics\\\".user\":\"file:/run/secrets/postgres-metrics/username\""));
}

#[test]
fn the_forms_env_and_file_show_their_source_and_the_password_stays_hidden() {
    let fx = Fx::new();
    let text = use_form(&open_file(&fx), "user_file", "user_env", "\"PGS_USER\"");
    let text = use_form(&text, "password_file", "password_env", "\"PGS_PASSWORD\"");
    let loaded = fx.load_env(&text, &[("PGS_USER", "role_a"), ("PGS_PASSWORD", "pw-from-env")]).unwrap();
    let out = render_print_config(&loaded);
    assert!(out.contains("postgres \"metrics\".user = \"role_a\"  # env:PGS_USER\n"), "{out}");
    assert!(out.contains("postgres \"metrics\".password = <hidden>  # env:PGS_PASSWORD\n"), "{out}");
    assert!(!out.contains("pw-from-env"));
    assert_eq!(source(&loaded, 0).password, "pw-from-env");
    // The form V of `user`.
    let text = use_form(&open_file(&fx), "user_file", "user", "\"role_b\"");
    let loaded = fx.load(&text).unwrap();
    assert_eq!(source(&loaded, 0).user, "role_b");
    assert!(render_print_config(&loaded).contains("postgres \"metrics\".user = \"role_b\"  # file\n"));
}

#[test]
fn the_blocks_keep_the_file_order_and_clickhouse_comes_before_postgres_in_the_output() {
    let fx = Fx::new();
    let local = fx.local();
    let (head, ch) = local.split_once("\nclickhouse \"prod\" {").unwrap();
    let pg = |id: &str| fx.substitute(&block(false)).replace("postgres \"metrics\"", &format!("postgres \"{id}\""));
    let text = format!("{head}\n{}\n{}\nclickhouse \"prod\" {{{ch}", pg("zeta"), pg("alpha"));
    let loaded = fx.load(&text).unwrap();
    let ids: Vec<&str> = loaded.config.postgres_sources.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["zeta", "alpha"]);
    let out = render_print_config(&loaded);
    let at = |needle: &str| out.find(needle).unwrap_or_else(|| panic!("{needle}"));
    assert!(at("clickhouse \"prod\".label") < at("postgres \"zeta\".label"));
    assert!(at("postgres \"zeta\".label") < at("postgres \"alpha\".label"));
}

// ---------------------------------------------------------------------------------------
// The keys: forms, secrets, ranges and rules of values.
// ---------------------------------------------------------------------------------------

#[test]
fn each_key_of_the_block_is_necessary_and_the_error_names_it() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let line = line_of(&base, "postgres \"metrics\" {");
    for key in SCHEMA.keys.iter().filter(|k| k.path.starts_with("postgres.") && !k.path.starts_with("postgres.tls")) {
        if key.path == "postgres.max_concurrent_per_subject" {
            continue;
        }
        let (block_path, attr) = key.path.rsplit_once('.').unwrap();
        let at = locate(&base, block_path, attr)
            .or_else(|| locate(&base, block_path, &format!("{attr}_file")))
            .unwrap_or_else(|| panic!("{}", key.path));
        let mut lines: Vec<&str> = base.lines().collect();
        lines.remove(at);
        let text = lines.join("\n") + "\n";
        let shown = key.path.replacen("postgres", "postgres \"metrics\"", 1);
        let errors = fx.errors(&text);
        assert_eq!(errors.len(), 1, "{shown}: {errors:?}");
        assert!(errors[0].starts_with(&format!("{shown}: missing ")), "{}", errors[0]);
        assert!(errors[0].ends_with(&format!("(line {line})")), "{}", errors[0]);
    }
}

#[test]
fn the_user_accepts_three_forms_and_the_password_two() {
    let fx = Fx::new();
    let base = open_file(&fx);
    // Two forms stop the start.
    let two = base.replace(
        "  user_file                  =",
        "  user                       = \"x\"\n  user_file                  =",
    );
    let e = fx.one(&two);
    assert!(e.starts_with("postgres \"metrics\".user: the file gives `user` at line"), "{e}");
    assert!(e.contains("and `user_file`; give exactly one form"), "{e}");
    // The password has no form V.
    let e = fx.one(&use_form(&base, "password_file", "password", "\"/x\""));
    assert!(e.contains("a secret accepts only `password_env` or `password_file`"), "{e}");
    // No form for the password.
    let e = fx.one(&remove(&base, "postgres.password_file"));
    assert!(e.contains("missing secret; give `password_env` or `password_file`"), "{e}");
    // A variable that does not exist, an empty variable, an absent file.
    let env_form = |name: &str| use_form(&base, "password_file", "password_env", &quoted(name));
    let e = fx.one(&env_form("PGS_ABSENT"));
    assert_eq!(
        e,
        format!(
            "postgres \"metrics\".password: the environment variable PGS_ABSENT is not set (line {})",
            locate(&base, "postgres", "password_file").unwrap() + 1
        )
    );
    let e = fx.load_env(&env_form("PGS_EMPTY"), &[("PGS_EMPTY", "")]).unwrap_err();
    assert!(e[0].contains("the value of the environment variable PGS_EMPTY is empty"), "{e:?}");
    let e = fx.one(&set(&base, "postgres.password_file", "\"/nonexistent/pg-password\""));
    assert!(e.starts_with("postgres \"metrics\".password: cannot read the file /nonexistent/pg-password:"), "{e}");
    crate::config::test_support::write_file(fx.dir.path().join("empty"), "\n").unwrap();
    let e = fx.one(&set(&base, "postgres.password_file", &quoted(&fx.path("empty"))));
    assert!(e.contains(&format!("the file {} is empty", fx.path("empty"))), "{e}");
}

#[test]
fn a_secret_in_clear_text_stops_the_start_and_never_shows() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let text = use_form(&base, "password_file", "password", "\"hunter2\"");
    let e = fx.one(&text);
    assert_eq!(
        e,
        format!(
            "postgres \"metrics\".password: a secret accepts only `password_env` or `password_file`, but the file gives it in clear text (value hidden) (line {})",
            line_of(&text, "password = ")
        )
    );
    assert!(!e.contains("hunter2"));
    // Also with another form.
    let text = text.replace(
        "  password                   = \"hunter2\"",
        &format!(
            "  password                   = \"hunter2\"\n  password_file              = \"{}\"",
            fx.path("pgm-pw")
        ),
    );
    let e = fx.one(&text);
    assert!(e.contains("in clear text") && !e.contains("hunter2"), "{e}");
}

#[test]
fn the_password_is_never_in_the_debug_output() {
    let fx = Fx::new();
    let loaded = fx.load(&open_file(&fx)).unwrap();
    assert!(!format!("{:?}", loaded.config).contains("pgmpass"));
    assert!(!format!("{:?}", loaded).contains("pgmpass"));
}

#[test]
fn a_value_that_is_too_long_or_outside_its_range_is_refused_with_the_key_the_type_and_the_value() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let cases = [
        ("postgres.port", "0", "an integer from 1 to 65535"),
        ("postgres.port", "65536", "an integer from 1 to 65535"),
        ("postgres.max_concurrent", "257", "an integer from 1 to 256"),
        ("postgres.max_concurrent", "0", "an integer from 1 to 256"),
        ("postgres.max_result_rows", "1000000001", "an integer from 1 to 1000000000"),
        ("postgres.max_result_bytes", "1048575", "a size in bytes from 1048576 to 1073741824"),
        ("postgres.max_query_size", "16777217", "a size in bytes from 262144 to 16777216"),
        ("postgres.connect_timeout", "\"99ms\"", "a duration from 100ms to 1m"),
        ("postgres.queue_wait", "\"61s\"", "a duration from 0s to 1m"),
        ("postgres.max_execution_time", "\"0s\"", "a duration from 1s to 1h"),
        ("postgres.sslmode", "\"require\"", "\"verify-full\" or \"disable\""),
    ];
    for (key, value, expected) in cases {
        let text = set(&base, key, value);
        let e = fx.one(&text);
        let shown = key.replacen("postgres", "postgres \"metrics\"", 1);
        assert!(e.starts_with(&format!("{shown}: expected {expected}, found {value}")), "{e}");
    }
    // No coercion: a text for an integer, an integer for a text.
    let e = fx.one(&set(&base, "postgres.port", "\"5432\""));
    assert!(e.contains("expected an integer from 1 to 65535, found \"5432\""), "{e}");
    let e = fx.one(&set(&base, "postgres.label", "7"));
    assert!(e.starts_with("postgres \"metrics\".label: expected a text"), "{e}");
    // Lengths in bytes.
    assert_eq!(fx.errors(&set(&base, "postgres.database", &quoted(&"d".repeat(63)))), Vec::<String>::new());
    assert!(fx.one(&set(&base, "postgres.database", &quoted(&"d".repeat(64)))).contains("1 to 63 bytes"));
}

#[test]
fn a_whole_number_of_seconds_is_necessary_for_max_execution_time() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let e = fx.one(&set(&base, "postgres.max_execution_time", "\"1500ms\""));
    assert!(
        e.starts_with("postgres \"metrics\".max_execution_time: expected a whole number of seconds, found \"1500ms\""),
        "{e}"
    );
    assert!(fx.errors(&set(&base, "postgres.max_execution_time", "\"2000ms\"")).is_empty());
}

#[test]
fn the_label_text_is_1_to_60_characters() {
    let fx = Fx::new();
    let base = open_file(&fx);
    assert!(fx.errors(&set(&base, "postgres.label", &quoted(&"é".repeat(60)))).is_empty());
    let e = fx.one(&set(&base, "postgres.label", &quoted(&"é".repeat(61))));
    assert!(e.contains("expected a text of 1 to 60 characters"), "{e}");
}

#[test]
fn the_block_label_follows_the_rule_of_13_12() {
    let fx = Fx::new();
    let base = open_file(&fx);
    for ok in ["0", "a-b", &"a".repeat(40)] {
        let text = replace_once(&base, "postgres \"metrics\" {", &format!("postgres \"{ok}\" {{"));
        assert_eq!(fx.errors(&text), Vec::<String>::new(), "{ok}");
    }
    for bad in ["Metrics", "-a", "a_b", &"a".repeat(41)] {
        let text = replace_once(&base, "postgres \"metrics\" {", &format!("postgres \"{bad}\" {{"));
        let e = fx.one(&text);
        assert!(
            e.starts_with(&format!(
                "postgres \"{bad}\": expected a label that matches ^[a-z0-9][a-z0-9-]{{0,39}}$, found"
            )),
            "{e}"
        );
    }
}

#[test]
fn the_host_is_a_dns_name_or_an_ip_address_and_never_a_unix_socket() {
    let fx = Fx::new();
    let base = open_file(&fx);
    for ok in ["db", "db.example.org", "10.0.0.5", "::1", "2001:db8::1"] {
        assert_eq!(fx.errors(&set(&base, "postgres.host", &quoted(ok))), Vec::<String>::new(), "{ok}");
    }
    for bad in ["/var/run/postgresql", "/tmp", "bad host", "db:5432", "postgresql://db", "@abstract"] {
        let e = fx.one(&set(&base, "postgres.host", &quoted(bad)));
        assert!(
            e.starts_with(&format!(
                "postgres \"metrics\".host: expected a DNS name of 1 to 253 bytes, or an IP address, found {}",
                quoted(bad)
            )),
            "{e}"
        );
    }
    let e = fx.one(&set(&base, "postgres.host", &quoted(&"a".repeat(254))));
    assert!(e.contains("1 to 253 bytes"), "{e}");
}

#[test]
fn the_search_path_rule() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let eight = ["a", "b", "c", "d", "e", "f", "g", "h"].join(",");
    let long_name = "n".repeat(63);
    for ok in ["metrics", "a,b", "_x,y_1,z9", eight.as_str(), long_name.as_str(), "public"] {
        assert_eq!(fx.errors(&set(&base, "postgres.search_path", &quoted(ok))), Vec::<String>::new(), "{ok}");
    }
    let nine = ["a", "b", "c", "d", "e", "f", "g", "h", "i"].join(",");
    let too_long_name = "n".repeat(64);
    for bad in [
        "a b",
        "a, b",
        " a",
        "a,",
        ",a",
        "a,,b",
        "A",
        "1a",
        "a-b",
        "a.b",
        "\"public\"",
        "pg_temp",
        "pg_temp_3",
        "a,pg_temp_1",
        "a,a",
        nine.as_str(),
        too_long_name.as_str(),
    ] {
        let e = fx.one(&set(&base, "postgres.search_path", &quoted(&bad.replace('"', "\\\""))));
        assert!(e.starts_with("postgres \"metrics\".search_path: expected 1 to 8 names separated by"), "{bad}: {e}");
    }
    // The pure function agrees.
    assert!(stage2::valid_search_path("pg_catalog,public"));
    assert!(!stage2::valid_search_path(""));
    assert!(!stage2::valid_search_path(&"a,".repeat(129)));
}

#[test]
fn an_unknown_key_and_a_duplicate_key_stop_the_start() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let text = base.replace(
        "  port                       = 5432\n",
        "  port                       = 5432\n  sslrootcert = \"/x\"\n",
    );
    let e = fx.one(&text);
    assert!(e.starts_with("postgres \"metrics\".sslrootcert: unknown attribute"), "{e}");
    let text =
        base.replace("  port                       = 5432\n", "  port                       = 5432\n  port = 5433\n");
    let errors = fx.errors(&text);
    assert!(errors.iter().any(|e| e.contains("duplicate attribute")), "{errors:?}");
    let at = base.rfind("  tls {").unwrap();
    let text = format!("{}  options {{\n  }}\n\n{}", &base[..at], &base[at..]);
    let e = fx.one(&text);
    assert!(e.starts_with("postgres \"metrics\".options: unknown block"), "{e}");
}

// ---------------------------------------------------------------------------------------
// The contradictions C14 to C20.
// ---------------------------------------------------------------------------------------

#[test]
fn c14_the_file_has_one_to_sixty_four_source_blocks_in_all() {
    let fx = Fx::new();
    let local = fx.local();
    let (head, ch) = local.split_once("\nclickhouse \"prod\" {").unwrap();
    let pg = fx.substitute(&block(false));
    let pg_body = pg.split_once("postgres \"metrics\" {").unwrap().1.to_string();
    let many = |clickhouse: usize, postgres: usize| {
        let mut text = head.to_string();
        for i in 0..clickhouse {
            text.push_str(&format!("\nclickhouse \"c{i}\" {{{ch}"));
        }
        for i in 0..postgres {
            text.push_str(&format!("\npostgres \"p{i}\" {{{pg_body}"));
        }
        text
    };
    // 64 in all, of both kinds.
    let loaded = fx.load(&many(32, 32)).unwrap();
    assert_eq!((loaded.config.clickhouse.len(), loaded.config.postgres_sources.len()), (32, 32));
    let loaded = fx.load(&many(0, 64)).unwrap();
    assert_eq!((loaded.config.clickhouse.len(), loaded.config.postgres_sources.len()), (0, 64));
    // Only postgres blocks: one is enough.
    assert_eq!(fx.load(&many(0, 1)).unwrap().config.postgres_sources.len(), 1);
    // 65 in all is a contradiction; 65 of one kind is an error of the engine.
    assert_eq!(
        fx.one(&many(32, 33)),
        "clickhouse, postgres, mongodb: expected 1 to 64 blocks `clickhouse \"<label>\"`, `postgres \"<label>\"` or `mongodb \"<label>\"` in all, found 65"
    );
    assert_eq!(fx.one(&many(0, 65)), "postgres: expected 0 to 64 blocks `postgres \"<label>\"`, found 65");
    assert_eq!(fx.one(&many(65, 0)), "clickhouse: expected 0 to 64 blocks `clickhouse \"<label>\"`, found 65");
    assert_eq!(
        fx.one(&many(0, 0)),
        "clickhouse, postgres, mongodb: expected 1 to 64 blocks `clickhouse \"<label>\"`, `postgres \"<label>\"` or `mongodb \"<label>\"` in all, found 0"
    );
}

#[test]
fn c15_a_label_is_unique_across_both_kinds() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let text = replace_once(&base, "postgres \"metrics\" {", "postgres \"prod\" {");
    let first = line_of(&text, "clickhouse \"prod\" {");
    let second = line_of(&text, "postgres \"prod\" {");
    assert_eq!(
        fx.one(&text),
        format!(
            "postgres \"prod\": duplicate label; the first block with this label is `clickhouse \"prod\"` at line {first} (line {second})"
        )
    );
    // The first block in the file owns the label, also when it is a postgres block.
    let local = fx.local();
    let (head, ch) = local.split_once("\nclickhouse \"prod\" {").unwrap();
    let pg = fx.substitute(&block(false)).replace("postgres \"metrics\"", "postgres \"prod\"");
    let text = format!("{head}\n{pg}\nclickhouse \"prod\" {{{ch}");
    let first = line_of(&text, "postgres \"prod\" {");
    let second = line_of(&text, "clickhouse \"prod\" {");
    assert_eq!(
        fx.one(&text),
        format!(
            "clickhouse \"prod\": duplicate label; the first block with this label is `postgres \"prod\"` at line {first} (line {second})"
        )
    );
    // The same label inside one kind stays an error of the engine.
    let text = append(&base, &fx.substitute(&block(false)));
    let e = fx.one(&text);
    assert!(e.starts_with("postgres \"metrics\": duplicate label; the first block with this label is at line"), "{e}");
}

#[test]
fn c16_the_block_tls_exists_only_with_verify_full() {
    let fx = Fx::new();
    let base = open_file(&fx);
    // verify-full without the block.
    let text = remove_tls_of_postgres(&base);
    assert_eq!(
        fx.one(&text),
        format!(
            "postgres \"metrics\".tls: missing block; expected when `postgres \"metrics\".sslmode` is \"verify-full\" (line {})",
            line_of(&text, "postgres \"metrics\" {")
        )
    );
    // disable with the block.
    let text = set(&base, "postgres.sslmode", "\"disable\"");
    let e = fx.one(&text);
    let tls_line = text.lines().enumerate().filter(|(_, l)| *l == "  tls {").map(|(i, _)| i + 1).last().unwrap();
    assert_eq!(
        e,
        format!(
            "postgres \"metrics\".tls: not permitted when `postgres \"metrics\".sslmode` is \"disable\"; the key is only for `postgres \"metrics\".sslmode` = \"verify-full\" (line {tls_line})"
        )
    );
    // disable without the block is valid, with the warning W7 for a host that is not a loopback address.
    assert!(fx.errors(&disabled_file(&fx)).is_empty());
    // The two keys of the block are necessary when the block exists.
    let text = remove(&base, "postgres.tls.server_name");
    assert!(fx.one(&text).starts_with("postgres \"metrics\".tls.server_name: missing attribute; expected a DNS name"));
    let text = remove(&base, "postgres.tls.ca_file");
    assert!(
        fx.one(&text).starts_with("postgres \"metrics\".tls.ca_file: missing attribute; expected an absolute path")
    );
}

#[test]
fn c17_the_memory_budget_is_at_least_two_times_the_result_bytes_of_each_source() {
    let fx = Fx::new();
    let base = open_file(&fx);
    // The ClickHouse block has the same limit: C11 also finds it.
    let text = set(&base, "server.max_result_memory_bytes", "33554432");
    let errors = fx.errors(&text);
    assert!(
        errors.contains(
            &"server.max_result_memory_bytes, postgres \"metrics\".max_result_bytes: `server.max_result_memory_bytes` (33554432) is less than 2 x `postgres \"metrics\".max_result_bytes` (33554432)".to_string()
        ),
        "{errors:#?}"
    );
    // A postgres block alone, at the exact limit (2 x), is valid.
    let text = set(&text, "postgres.max_result_bytes", "16777216");
    let text = set(&text, "clickhouse.max_result_bytes", "16777216");
    assert!(fx.errors(&text).is_empty());
    let text = set(&text, "postgres.max_result_bytes", "16777217");
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("(33554432) is less than 2 x `postgres \"metrics\".max_result_bytes` (16777217)"));
}

#[test]
fn c18_the_value_for_each_subject_is_at_most_max_concurrent() {
    let fx = Fx::new();
    let base = authzen_file(&fx);
    let text = set(&base, "postgres.max_concurrent_per_subject", "20");
    assert_eq!(
        fx.one(&text),
        "postgres \"metrics\".max_concurrent_per_subject, postgres \"metrics\".max_concurrent: `postgres \"metrics\".max_concurrent_per_subject` (20) is more than `postgres \"metrics\".max_concurrent` (16)"
    );
    assert!(fx.errors(&set(&base, "postgres.max_concurrent_per_subject", "16")).is_empty());
    let text = set(&set(&base, "postgres.max_concurrent_per_subject", "256"), "postgres.max_concurrent", "256");
    assert!(fx.errors(&text).is_empty());
}

#[test]
fn c19_max_concurrent_per_subject_exists_only_in_the_mode_authzen() {
    let fx = Fx::new();
    let text = append(&fx.local(), &fx.substitute(&block(true)));
    assert_eq!(
        fx.one(&text),
        format!(
            "postgres \"metrics\".max_concurrent_per_subject: not permitted when `access.mode` is \"open\"; the key is only for `access.mode` = \"authzen\" (line {})",
            line_of(&text, "max_concurrent_per_subject")
        )
    );
    let text = remove(&authzen_file(&fx), "postgres.max_concurrent_per_subject");
    assert_eq!(
        fx.one(&text),
        format!(
            "postgres \"metrics\".max_concurrent_per_subject: missing attribute; expected an integer from 1 to 256 (line {})",
            line_of(&text, "postgres \"metrics\" {")
        )
    );
}

/// The file DISTRIBUTED with a source on the host, the port and the user of `storage.postgres`.
fn same_server_file(fx: &Fx) -> String {
    let text = authzen_file(fx);
    let text = set(&text, "postgres.host", "\"hune-pg-rw\"");
    use_form(&text, "user_file", "user", "\"app\"")
}

#[test]
fn c20_a_source_needs_its_own_role_on_the_server_of_the_store() {
    let fx = Fx::new();
    let text = same_server_file(&fx);
    assert_eq!(
        fx.one(&text),
        "postgres \"metrics\".user, storage.postgres.user: `postgres \"metrics\".user` (\"app\") equals `storage.postgres.user` on the same host and port; a source needs its own read-only role"
    );
    // The host of a name has no case.
    let upper = set(&text, "postgres.host", "\"HUNE-PG-RW\"");
    assert!(fx.one(&upper).contains("equals `storage.postgres.user` on the same host and port"));
    // A role from a file counts too: the file of the store gives `app`.
    let from_file = use_form(&text, "user", "user_file", &quoted(&fx.path("pg-user")));
    assert!(fx.one(&from_file).contains("equals `storage.postgres.user`"));
    // Another role, another port, another host or the volume backend: no contradiction.
    assert!(fx.errors(&set(&text, "postgres.user", "\"hune_ro\"")).is_empty());
    assert!(fx.errors(&set(&text, "postgres.port", "5433")).is_empty());
    assert!(fx.errors(&set(&text, "postgres.host", "\"hune-pg-other\"")).is_empty());
    let volume = set(&open_file(&fx), "postgres.host", "\"hune-pg-rw\"");
    let volume = use_form(&volume, "user_file", "user", "\"app\"");
    assert!(fx.errors(&volume).is_empty());
}

#[test]
fn c20_two_names_of_one_ip_address_are_the_same_host() {
    let fx = Fx::new();
    let text = same_server_file(&fx);
    let text = set(&text, "postgres.host", "\"10.0.0.7\"");
    let text = set(&text, "storage.postgres.host", "\"10.0.0.7\"");
    // The store has the TLS name `hune-pg-rw`, so the host is an address here.
    assert!(fx.one(&text).contains("equals `storage.postgres.user`"));
    let text = set(&text, "postgres.host", "\"::1\"");
    let text = set(&text, "storage.postgres.host", "\"0:0:0:0:0:0:0:1\"");
    assert!(fx.one(&text).contains("equals `storage.postgres.user`"));
}

#[test]
fn c20_folds_the_spellings_of_one_host() {
    // Finding L3: the check never resolves a name, but it folds the spellings that name one server for sure.
    let fx = Fx::new();
    let base = same_server_file(&fx);
    for (source_host, store_host) in [
        ("localhost", "127.0.0.1"),
        ("LOCALHOST", "localhost"),
        ("127.0.0.1", "::1"),
        ("::ffff:127.0.0.1", "127.0.0.1"),
        ("::ffff:127.0.0.1", "::1"),
        ("127.0.0.2", "localhost"),
        ("db.internal", "DB.INTERNAL"),
        ("::ffff:10.0.0.7", "10.0.0.7"),
    ] {
        let text = set(&base, "postgres.host", &quoted(source_host));
        let text = set(&text, "storage.postgres.host", &quoted(store_host));
        let message = fx.errors(&text).iter().map(ToString::to_string).collect::<Vec<_>>().join(" | ");
        assert!(
            message.contains("equals `storage.postgres.user`"),
            "{source_host} and {store_host} are one server: {message}"
        );
    }
    // Two other servers stay two servers: no resolution, no guess.
    for (source_host, store_host) in [("localhost", "10.0.0.7"), ("db.internal", "db.internal.example"), ("::1", "::2")]
    {
        let text = set(&base, "postgres.host", &quoted(source_host));
        let text = set(&text, "storage.postgres.host", &quoted(store_host));
        assert!(fx.errors(&text).is_empty(), "{source_host} and {store_host}");
    }
}

#[test]
fn c20_is_skipped_by_a_read_mode_that_does_not_read_the_users() {
    let fx = Fx::new();
    // The role of the source is in the file. The role of the store is in a file that this mode does not read.
    let text = same_server_file(&fx);
    let file = fx.dir.path().join("hune.hcl");
    crate::config::test_support::write_file(&file, &text).unwrap();
    let env = CapturedEnvironment::default();
    assert!(load_with(&file, &env, ReadMode::All).is_err());
    let loaded = load_with(&file, &env, ReadMode::Nothing).unwrap();
    assert_eq!(loaded.config.postgres_sources[0].user, "app");
    // The mode of the store commands reads the user of the store: the check runs and finds C20.
    let errors = load_with(&file, &env, ReadMode::StoreCommands).unwrap_err();
    assert!(errors.iter().any(|e| e.to_string().contains("equals `storage.postgres.user`")), "{errors:?}");
}

// ---------------------------------------------------------------------------------------
// The warning W7 and the stage 2 checks.
// ---------------------------------------------------------------------------------------

#[test]
fn w7_sslmode_disable_for_a_host_that_is_not_a_loopback_address() {
    let fx = Fx::new();
    let base = disabled_file(&fx);
    let loaded = fx.load(&base).unwrap();
    let w7: Vec<&Warning> = loaded.warnings.iter().filter(|w| w.key.starts_with("postgres")).collect();
    assert_eq!(w7.len(), 1);
    assert_eq!(w7[0].key, "postgres \"metrics\".sslmode");
    assert_eq!(w7[0].message, "postgres \"metrics\".sslmode is disable for a host that is not a loopback address.");
    // The warning comes after the other warnings.
    assert_eq!(loaded.warnings.last().unwrap().key, "postgres \"metrics\".sslmode");
    for host in ["127.0.0.1", "127.9.9.9", "localhost", "::1"] {
        let loaded = fx.load(&set(&base, "postgres.host", &quoted(host))).unwrap();
        assert!(loaded.warnings.iter().all(|w| !w.key.starts_with("postgres")), "{host}");
    }
    // No warning with verify-full.
    let loaded = fx.load(&open_file(&fx)).unwrap();
    assert!(loaded.warnings.iter().all(|w| !w.key.starts_with("postgres")));
}

#[test]
fn each_postgres_block_gets_its_own_warning_in_file_order() {
    let fx = Fx::new();
    let base = disabled_file(&fx);
    let second = base.split_once("\npostgres \"metrics\" {").unwrap().1;
    let text = format!("{base}\npostgres \"other\" {{{second}");
    let loaded = fx.load(&text).unwrap();
    let keys: Vec<&str> =
        loaded.warnings.iter().filter(|w| w.key.starts_with("postgres")).map(|w| w.key.as_str()).collect();
    assert_eq!(keys, ["postgres \"metrics\".sslmode", "postgres \"other\".sslmode"]);
}

#[test]
fn the_ca_file_of_the_block_gets_the_checks_of_13_13() {
    let fx = Fx::new();
    let base = open_file(&fx);
    crate::config::test_support::write_file(fx.dir.path().join("bad.pem"), "not a certificate\n").unwrap();
    let e = fx.one(&set(&base, "postgres.tls.ca_file", &quoted(&fx.path("bad.pem"))));
    assert!(e.starts_with("postgres \"metrics\".tls.ca_file:"), "{e}");
    let e = fx.one(&set(&base, "postgres.tls.ca_file", "\"relative/ca.pem\""));
    assert!(e.starts_with("postgres \"metrics\".tls.ca_file: expected an absolute path"), "{e}");
    let e = fx.one(&set(&base, "postgres.tls.ca_file", "\"/nonexistent/ca.pem\""));
    assert!(e.starts_with("postgres \"metrics\".tls.ca_file:"), "{e}");
}

#[test]
fn stage_2_and_3_do_not_run_when_stage_1_has_an_error() {
    let fx = Fx::new();
    let base = open_file(&fx);
    // A wrong port (stage 1) and a duplicate label (stage 3): only the first one shows.
    let text = replace_once(&base, "postgres \"metrics\" {", "postgres \"prod\" {");
    let text = set(&text, "postgres.port", "0");
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("postgres \"prod\".port:"));
}

#[test]
fn the_labels_of_the_blocks_are_checked_by_the_import_mode_and_not_by_the_store_modes() {
    let fx = Fx::new();
    let text = replace_once(&open_file(&fx), "postgres \"metrics\" {", "postgres \"Metrics\" {");
    let file = fx.dir.path().join("hune.hcl");
    crate::config::test_support::write_file(&file, &text).unwrap();
    let env = CapturedEnvironment::default();
    let errors = load_with(&file, &env, ReadMode::ImportWrite).unwrap_err();
    assert!(errors.iter().any(|e| e.to_string().starts_with("postgres \"Metrics\": expected a label")), "{errors:?}");
    // The store modes ignore the sources: the partial configuration has no entry for the block.
    let loaded = load_with(&file, &env, ReadMode::StoreCommands).unwrap();
    assert!(loaded.config.postgres_sources.is_empty());
    assert!(!loaded.mode.is_complete());
    // The import mode reads the label and the kind, and no value of the source.
    let good = open_file(&fx);
    crate::config::test_support::write_file(&file, &good).unwrap();
    let loaded = load_with(&file, &env, ReadMode::ImportWrite).unwrap();
    assert_eq!(loaded.config.postgres_sources[0].id.as_str(), "metrics");
    assert_eq!(loaded.config.postgres_sources[0].user, "");
    assert_eq!(loaded.config.postgres_sources[0].password, "");
    let entry = loaded.effective.iter().find(|e| e.key == "postgres \"metrics\".password").unwrap();
    assert_eq!(entry.value, "<not read>");
}

// ---------------------------------------------------------------------------------------
// The key table and the generated documents.
// ---------------------------------------------------------------------------------------

#[test]
fn the_reference_files_hold_no_postgres_block() {
    for profile in [Profile::Local, Profile::Distributed] {
        let text = render_reference(ReferenceFormat::Hcl, profile);
        assert!(!text.contains("postgres \"") && !text.contains("postgres {\n  label"), "{profile:?}");
    }
}

#[test]
fn the_reference_document_has_the_tables_of_13_12a() {
    for needle in [
        "| `postgres \"<id>\"` | 0 to 64 | always | never |",
        "| `postgres \"<id>\".tls` | 0 or 1 | `sslmode = \"verify-full\"` of the block | `sslmode = \"disable\"` of the block |",
        "## Block `postgres \"<id>\"`",
        "## Block `postgres \"<id>\".tls`",
        "| `postgres \"<id>\".host` | a DNS name of 1 to 253 bytes, or an IP address | no | V | always | never | `\"hune-pg-ro\"` |",
        "| `postgres \"<id>\".user` | a text of 1 to 63 bytes with no control character | no | V, E, F | always | never | `user_file = \"/run/secrets/postgres-metrics/username\"` |",
        "| `postgres \"<id>\".password` | a text of 1 to 1024 bytes | yes | E, F | always | never | `password_file = \"/run/secrets/postgres-metrics/password\"` |",
        "| `postgres \"<id>\".max_concurrent` | an integer from 1 to 256 | no | V | always | never | `16` |",
        "| `postgres \"<id>\".max_concurrent_per_subject` | an integer from 1 to 256 | no | V | `access.mode = \"authzen\"` | `access.mode = \"open\"` | `6` |",
        "| `postgres \"<id>\".tls.ca_file` | an absolute path of 1 to 4096 bytes | no | V | `sslmode = \"verify-full\"` | `sslmode = \"disable\"` | `\"/run/secrets/postgres-metrics-ca/ca.crt\"` |",
        "| `clickhouse \"<id>\"` | 0 to 64 | always | never |",
    ] {
        // A row of a key ends with its description: the needle is the start of the row.
        assert!(REFERENCE_DOC.contains(needle.trim_end_matches(" |")), "{needle}");
    }
    assert!(!REFERENCE_DOC.contains("No key has a default value"));
}

#[test]
fn the_block_of_13_12a_is_in_the_key_table_in_the_order_of_the_spec() {
    let paths: Vec<&str> = SCHEMA.keys.iter().filter(|k| k.path.starts_with("postgres.")).map(|k| k.path).collect();
    assert_eq!(
        paths,
        [
            "postgres.label",
            "postgres.host",
            "postgres.port",
            "postgres.database",
            "postgres.user",
            "postgres.password",
            "postgres.search_path",
            "postgres.sslmode",
            "postgres.tls.ca_file",
            "postgres.tls.server_name",
            "postgres.connect_timeout",
            "postgres.max_execution_time",
            "postgres.max_result_rows",
            "postgres.max_result_bytes",
            "postgres.max_query_size",
            "postgres.max_concurrent",
            "postgres.queue_wait",
            "postgres.max_concurrent_per_subject",
        ]
    );
    let secrets: Vec<&str> =
        SCHEMA.keys.iter().filter(|k| k.secret && k.path.starts_with("postgres.")).map(|k| k.path).collect();
    assert_eq!(secrets, ["postgres.password"]);
}

#[test]
fn the_example_block_passes_the_checks_with_the_reference_values_of_the_document() {
    // Each reference value of the document is a valid value: the example block of 13.12a uses them.
    let fx = Fx::new();
    for key in SCHEMA.keys.iter().filter(|k| k.path.starts_with("postgres.")) {
        assert_ne!(key.doc_ref, hclconf::Ref::Absent, "{}", key.path);
    }
    assert!(fx.errors(&authzen_file(&fx)).is_empty());
}
