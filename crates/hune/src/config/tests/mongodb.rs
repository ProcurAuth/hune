// SPDX-License-Identifier: MIT OR Apache-2.0
//! Case K18 of 16.5: the block `mongodb "<id>"` (13.12b), the contradictions C14, C15 and C22 to C27,
//! the warning W8, the refusal of a build without the driver, the output of `print-config` and the
//! reference document.
//!
//! Each test starts from the example file LOCAL (mode `open`) or DISTRIBUTED (mode `authzen`) and
//! the example block of 13.12b. The secret files that the block names exist in the test directory.
//! The test build acts as a build with the driver, except in the tests of the refusal.

use super::*;

/// The example block of 13.12b. `replica_set` gives the topology `replica_set` with its two keys.
/// `authzen` keeps the key `max_concurrent_per_subject`, which the mode `open` forbids.
pub(super) fn block(replica_set: bool, authzen: bool) -> String {
    let mut text = r#"mongodb "events" {
  label                      = "Events"
  topology                   = "direct"
  host                       = "hune-mongo-ro"
  port                       = 27017
  user_file                  = "/run/secrets/mongodb-events/username"
  password_file              = "/run/secrets/mongodb-events/password"
  auth_database              = "admin"
  databases                  = ["analytics"]
  collections                = ["node_load", "node_events"]
  read_preference            = "secondaryPreferred"
  tls_mode                   = "verify-full"
  connect_timeout            = "5s"
  max_execution_time         = "1m"
  max_result_rows            = 100000
  max_result_bytes           = 33554432
  max_query_size             = 1048576
  max_concurrent             = 16
  queue_wait                 = "5s"
  max_concurrent_per_subject = 6

  tls {
    ca_file = "/run/secrets/mongodb-events-ca/ca.crt"
  }
}
"#
    .to_string();
    if replica_set {
        text = text
            .replace("\"direct\"", "\"replica_set\"")
            .replace(
                "  host                       = \"hune-mongo-ro\"\n  port                       = 27017\n",
                "  hosts                      = [\"mongo-0:27017\", \"mongo-1:27017\", \"mongo-2:27017\"]\n  replica_set                = \"rs0\"\n",
            );
    }
    if !authzen {
        text = text.replace("  max_concurrent_per_subject = 6\n", "");
    }
    text
}

/// The file LOCAL with the example block.
fn open_file(fx: &Fx) -> String {
    append(&fx.local(), &fx.substitute(&block(false, false)))
}

/// The file DISTRIBUTED with the example block.
fn authzen_file(fx: &Fx) -> String {
    append(&fx.distributed(), &fx.substitute(&block(false, true)))
}

/// The file LOCAL with the example block of the topology `replica_set`.
fn replica_file(fx: &Fx) -> String {
    append(&fx.local(), &fx.substitute(&block(true, false)))
}

/// The file LOCAL with the example block, without TLS: `tls_mode = "disable"` and no block `tls`.
fn disabled_file(fx: &Fx) -> String {
    set(&remove_tls_of_mongodb(&open_file(fx)), "mongodb.tls_mode", "\"disable\"")
}

/// Give the attribute `old` of the block `mongodb` another name and another value, for example
/// `user_file` to `user_env`.
fn use_form(text: &str, old: &str, new: &str, rhs: &str) -> String {
    let at = locate(text, "mongodb", old).unwrap_or_else(|| panic!("{old}"));
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines[at] = format!("  {new} = {rhs}");
    lines.join("\n") + "\n"
}

/// Remove the block `tls` of the block `mongodb`: the last block `tls` of the file.
fn remove_tls_of_mongodb(text: &str) -> String {
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

/// Add an attribute line before the line `before` of the block `mongodb`.
fn add_line(text: &str, before: &str, line: &str) -> String {
    replace_once(text, before, &format!("{line}\n{before}"))
}

/// The list of items as HCL text, for example `["a", "b"]`.
fn list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| format!("\"{i}\"")).collect();
    format!("[{}]", quoted.join(", "))
}

fn names(prefix: &str, n: usize) -> Vec<String> {
    (0..n).map(|i| format!("{prefix}{i}")).collect()
}

/// The lines of `print-config` for the block `mongodb "events"`, with the paths of the example.
fn mongo_lines(fx: &Fx, loaded: &LoadedConfig) -> Vec<String> {
    fx.back(&render_print_config(loaded))
        .lines()
        .filter(|l| l.starts_with("mongodb \"events\""))
        .map(str::to_string)
        .collect()
}

// ---------------------------------------------------------------------------------------
// The example block and print-config.
// ---------------------------------------------------------------------------------------

#[test]
fn the_example_block_loads_in_the_mode_open_without_warning_of_its_own() {
    let fx = Fx::new();
    let loaded = fx.load(&open_file(&fx)).unwrap();
    assert!(loaded.warnings.iter().all(|w| w.key == "access.mode"), "{:?}", loaded.warnings);
}

#[test]
fn the_example_block_loads_in_the_mode_authzen_and_gives_no_warning() {
    let fx = Fx::new();
    let loaded = fx.load(&authzen_file(&fx)).unwrap();
    assert!(loaded.warnings.iter().all(|w| w.key == "server.listen"), "{:?}", loaded.warnings);
}

#[test]
fn print_config_gives_the_lines_of_13_20_and_hides_the_password() {
    let fx = Fx::new();
    let loaded = fx.load(&authzen_file(&fx)).unwrap();
    let expected = [
        "mongodb \"events\".label = \"Events\"  # file",
        "mongodb \"events\".topology = \"direct\"  # file",
        "mongodb \"events\".host = \"hune-mongo-ro\"  # file",
        "mongodb \"events\".port = 27017  # file",
        "mongodb \"events\".user = \"hune_ro\"  # file:/run/secrets/mongodb-events/username",
        "mongodb \"events\".password = <hidden>  # file:/run/secrets/mongodb-events/password",
        "mongodb \"events\".auth_database = \"admin\"  # file",
        "mongodb \"events\".databases = [\"analytics\"]  # file",
        "mongodb \"events\".collections = [\"node_load\", \"node_events\"]  # file",
        "mongodb \"events\".read_preference = \"secondaryPreferred\"  # file",
        "mongodb \"events\".tls_mode = \"verify-full\"  # file",
        "mongodb \"events\".tls.ca_file = \"/run/secrets/mongodb-events-ca/ca.crt\"  # file",
        "mongodb \"events\".connect_timeout = \"5s\"  # file",
        "mongodb \"events\".max_execution_time = \"1m\"  # file",
        "mongodb \"events\".max_result_rows = 100000  # file",
        "mongodb \"events\".max_result_bytes = 33554432  # file",
        "mongodb \"events\".max_query_size = 1048576  # file",
        "mongodb \"events\".max_concurrent = 16  # file",
        "mongodb \"events\".queue_wait = \"5s\"  # file",
        "mongodb \"events\".max_concurrent_per_subject = 6  # file",
    ];
    assert_eq!(mongo_lines(&fx, &loaded), expected);
    let out = render_print_config(&loaded);
    assert!(!out.contains("mgepass"));
    // The start log shows the source of each value that does not come from the file, and no secret.
    let config = start_log_config_json(&loaded);
    assert!(!config.contains("mgepass"));
    assert!(config.contains("\"mongodb \\\"events\\\".password\":\"<hidden>\""), "{config}");
    assert!(config.contains("\"mongodb \\\"events\\\".databases\":[\"analytics\"]"), "{config}");
    let sources = fx.back(&start_log_sources_json(&loaded));
    assert!(sources.contains("\"mongodb \\\"events\\\".password\":\"file:/run/secrets/mongodb-events/password\""));
    assert!(sources.contains("\"mongodb \\\"events\\\".user\":\"file:/run/secrets/mongodb-events/username\""));
}

#[test]
fn print_config_of_the_topology_replica_set_shows_the_list_of_hosts() {
    let fx = Fx::new();
    let loaded = fx.load(&replica_file(&fx)).unwrap();
    let lines = mongo_lines(&fx, &loaded);
    assert_eq!(lines[1], "mongodb \"events\".topology = \"replica_set\"  # file");
    assert_eq!(
        lines[2],
        "mongodb \"events\".hosts = [\"mongo-0:27017\", \"mongo-1:27017\", \"mongo-2:27017\"]  # file"
    );
    assert_eq!(lines[3], "mongodb \"events\".replica_set = \"rs0\"  # file");
    assert!(!lines.iter().any(|l| l.contains(".host =") || l.contains(".port =")));
}

#[test]
fn the_forms_env_and_file_show_their_source_and_the_password_stays_hidden() {
    let fx = Fx::new();
    let text = use_form(&open_file(&fx), "user_file", "user_env", "\"MGS_USER\"");
    let text = use_form(&text, "password_file", "password_env", "\"MGS_PASSWORD\"");
    let loaded = fx.load_env(&text, &[("MGS_USER", "role_a"), ("MGS_PASSWORD", "pw-from-env")]).unwrap();
    let out = render_print_config(&loaded);
    assert!(out.contains("mongodb \"events\".user = \"role_a\"  # env:MGS_USER\n"), "{out}");
    assert!(out.contains("mongodb \"events\".password = <hidden>  # env:MGS_PASSWORD\n"), "{out}");
    assert!(!out.contains("pw-from-env"));
    assert!(!format!("{loaded:?}").contains("pw-from-env"));
    // The form V of `user`.
    let text = use_form(&open_file(&fx), "user_file", "user", "\"role_b\"");
    let loaded = fx.load(&text).unwrap();
    assert!(render_print_config(&loaded).contains("mongodb \"events\".user = \"role_b\"  # file\n"));
}

#[test]
fn the_blocks_keep_the_file_order_and_the_three_kinds_come_in_the_order_of_the_table() {
    let fx = Fx::new();
    let local = fx.local();
    let (head, ch) = local.split_once("\nclickhouse \"prod\" {").unwrap();
    let mg = |id: &str| fx.substitute(&block(false, false)).replace("mongodb \"events\"", &format!("mongodb \"{id}\""));
    let pg = fx.substitute(&postgres::block(false));
    // The file order is mongodb, postgres, clickhouse: the output order is clickhouse, postgres, mongodb.
    let text = format!("{head}\n{}\n{}\n{pg}\nclickhouse \"prod\" {{{ch}", mg("zeta"), mg("alpha"));
    let loaded = fx.load(&text).unwrap();
    let out = render_print_config(&loaded);
    let at = |needle: &str| out.find(needle).unwrap_or_else(|| panic!("{needle}"));
    assert!(at("clickhouse \"prod\".label") < at("postgres \"metrics\".label"));
    assert!(at("postgres \"metrics\".label") < at("mongodb \"zeta\".label"));
    assert!(at("mongodb \"zeta\".label") < at("mongodb \"alpha\".label"));
}

// ---------------------------------------------------------------------------------------
// The keys: forms, secrets, ranges and rules of values.
// ---------------------------------------------------------------------------------------

#[test]
fn each_key_of_the_block_that_has_no_topology_is_necessary_and_the_error_names_it() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let line = line_of(&base, "mongodb \"events\" {");
    let manual = ["mongodb.host", "mongodb.port", "mongodb.hosts", "mongodb.replica_set"];
    let mut count = 0;
    for key in SCHEMA.keys.iter().filter(|k| k.path.starts_with("mongodb.") && !k.path.starts_with("mongodb.tls.")) {
        if key.path == "mongodb.max_concurrent_per_subject" || manual.contains(&key.path) {
            continue;
        }
        let (block_path, attr) = key.path.rsplit_once('.').unwrap();
        let at = locate(&base, block_path, attr)
            .or_else(|| locate(&base, block_path, &format!("{attr}_file")))
            .unwrap_or_else(|| panic!("{}", key.path));
        let mut lines: Vec<&str> = base.lines().collect();
        lines.remove(at);
        let text = lines.join("\n") + "\n";
        let shown = key.path.replacen("mongodb", "mongodb \"events\"", 1);
        let errors = fx.errors(&text);
        // Without the topology, the keys of the topology cannot be checked: the first error is the key.
        assert!(errors.len() == 1, "{shown}: {errors:?}");
        assert!(errors[0].starts_with(&format!("{shown}: missing ")), "{}", errors[0]);
        assert!(errors[0].ends_with(&format!("(line {line})")), "{}", errors[0]);
        count += 1;
    }
    assert_eq!(count, 16);
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
    assert!(e.starts_with("mongodb \"events\".user: the file gives `user` at line"), "{e}");
    assert!(e.contains("and `user_file`; give exactly one form"), "{e}");
    // The password has no form V.
    let e = fx.one(&use_form(&base, "password_file", "password", "\"/x\""));
    assert!(e.contains("a secret accepts only `password_env` or `password_file`"), "{e}");
    // No form for the password.
    let e = fx.one(&remove(&base, "mongodb.password_file"));
    assert!(e.contains("missing secret; give `password_env` or `password_file`"), "{e}");
    // No form for the user.
    let e = fx.one(&remove(&base, "mongodb.user_file"));
    assert!(e.contains("missing attribute; give `user`, `user_env` or `user_file`"), "{e}");
    // A variable that does not exist, an empty variable, an absent file, an empty file.
    let env_form = |name: &str| use_form(&base, "password_file", "password_env", &quoted(name));
    let e = fx.one(&env_form("MGS_ABSENT"));
    assert_eq!(
        e,
        format!(
            "mongodb \"events\".password: the environment variable MGS_ABSENT is not set (line {})",
            locate(&base, "mongodb", "password_file").unwrap() + 1
        )
    );
    let e = fx.load_env(&env_form("MGS_EMPTY"), &[("MGS_EMPTY", "")]).unwrap_err();
    assert!(e[0].contains("the value of the environment variable MGS_EMPTY is empty"), "{e:?}");
    let e = fx.one(&set(&base, "mongodb.password_file", "\"/nonexistent/mongo-password\""));
    assert!(e.starts_with("mongodb \"events\".password: cannot read the file /nonexistent/mongo-password:"), "{e}");
    crate::config::test_support::write_file(fx.dir.path().join("empty"), "\n").unwrap();
    let e = fx.one(&set(&base, "mongodb.password_file", &quoted(&fx.path("empty"))));
    assert!(e.contains(&format!("the file {} is empty", fx.path("empty"))), "{e}");
    // A relative path of `_file`, and a name of variable that the rules of 13.5 refuse.
    let e = fx.one(&set(&base, "mongodb.password_file", "\"relative\""));
    assert!(e.contains("expected an absolute path of 1 to 4096 bytes"), "{e}");
    let e = fx.one(&env_form("SSL_CERT_FILE"));
    assert!(e.contains("SSL_CERT_FILE"), "{e}");
    // The other keys refuse the forms E and F.
    let e = fx.one(&add_line(&base, "  topology ", "  label_file = \"/x\""));
    assert!(e.contains("`label` accepts only a value in the file, not `_env` or `_file`"), "{e}");
    let e = fx.one(&use_form(&base, "auth_database", "auth_database_env", "\"X\""));
    assert!(e.contains("`auth_database` accepts only a value in the file, not `_env` or `_file`"), "{e}");
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
            "mongodb \"events\".password: a secret accepts only `password_env` or `password_file`, but the file gives it in clear text (value hidden) (line {})",
            line_of(&text, "password = ")
        )
    );
    assert!(!e.contains("hunter2"));
    // Also with another form.
    let text = text.replace(
        "  password = \"hunter2\"",
        &format!("  password = \"hunter2\"\n  password_file = \"{}\"", fx.path("mge-pw")),
    );
    let e = fx.one(&text);
    assert!(e.contains("in clear text") && !e.contains("hunter2"), "{e}");
}

#[test]
fn the_password_is_never_in_the_debug_output() {
    let fx = Fx::new();
    let loaded = fx.load(&open_file(&fx)).unwrap();
    assert!(!format!("{:?}", loaded.config).contains("mgepass"));
    assert!(!format!("{loaded:?}").contains("mgepass"));
}

#[test]
fn a_value_outside_its_range_is_refused_with_the_key_the_type_and_the_value() {
    let fx = Fx::new();
    let base = authzen_file(&fx);
    let cases = [
        ("mongodb.port", "0", "an integer from 1 to 65535"),
        ("mongodb.port", "65536", "an integer from 1 to 65535"),
        ("mongodb.max_concurrent", "257", "an integer from 1 to 256"),
        ("mongodb.max_concurrent", "0", "an integer from 1 to 256"),
        ("mongodb.max_concurrent_per_subject", "257", "an integer from 1 to 256"),
        ("mongodb.max_concurrent_per_subject", "0", "an integer from 1 to 256"),
        ("mongodb.max_result_rows", "1000000001", "an integer from 1 to 1000000000"),
        ("mongodb.max_result_rows", "0", "an integer from 1 to 1000000000"),
        ("mongodb.max_result_bytes", "1048575", "a size in bytes from 1048576 to 1073741824"),
        ("mongodb.max_result_bytes", "1073741825", "a size in bytes from 1048576 to 1073741824"),
        ("mongodb.max_query_size", "262143", "a size in bytes from 262144 to 8388608"),
        ("mongodb.max_query_size", "8388609", "a size in bytes from 262144 to 8388608"),
        ("mongodb.connect_timeout", "\"99ms\"", "a duration from 100ms to 1m"),
        ("mongodb.connect_timeout", "\"61s\"", "a duration from 100ms to 1m"),
        ("mongodb.queue_wait", "\"61s\"", "a duration from 0s to 1m"),
        ("mongodb.max_execution_time", "\"0s\"", "a duration from 1s to 1h"),
        ("mongodb.max_execution_time", "\"3601s\"", "a duration from 1s to 1h"),
        ("mongodb.tls_mode", "\"require\"", "\"verify-full\" or \"disable\""),
        ("mongodb.topology", "\"sharded\"", "\"direct\" or \"replica_set\""),
        (
            "mongodb.read_preference",
            "\"Primary\"",
            "\"primary\", \"primaryPreferred\", \"secondary\", \"secondaryPreferred\" or \"nearest\"",
        ),
    ];
    for (key, value, expected) in cases {
        let text = set(&base, key, value);
        let e = fx.one(&text);
        let shown = key.replacen("mongodb", "mongodb \"events\"", 1);
        assert!(e.starts_with(&format!("{shown}: expected {expected}, found {value}")), "{e}");
    }
    // No coercion: a text for an integer, an integer for a text, a text for a boolean list.
    let e = fx.one(&set(&base, "mongodb.port", "\"27017\""));
    assert!(e.contains("expected an integer from 1 to 65535, found \"27017\""), "{e}");
    let e = fx.one(&set(&base, "mongodb.label", "7"));
    assert!(e.starts_with("mongodb \"events\".label: expected a text"), "{e}");
    let e = fx.one(&set(&base, "mongodb.databases", "\"analytics\""));
    assert!(e.starts_with("mongodb \"events\".databases: expected a list of 1 to 8 texts"), "{e}");
    let e = fx.one(&set(&base, "mongodb.databases", "[1]"));
    assert!(e.starts_with("mongodb \"events\".databases: expected a list of 1 to 8 texts"), "{e}");
}

#[test]
fn each_numeric_and_duration_key_accepts_its_minimum_and_its_maximum() {
    let fx = Fx::new();
    let base = authzen_file(&fx);
    // `max_concurrent_per_subject` is at most `max_concurrent`, so both go together.
    let cases = [
        ("mongodb.port", "1"),
        ("mongodb.port", "65535"),
        ("mongodb.max_result_rows", "1"),
        ("mongodb.max_result_rows", "1000000000"),
        ("mongodb.max_result_bytes", "1048576"),
        ("mongodb.max_query_size", "262144"),
        ("mongodb.max_query_size", "8388608"),
        ("mongodb.max_concurrent", "256"),
        ("mongodb.connect_timeout", "\"100ms\""),
        ("mongodb.connect_timeout", "\"1m\""),
        ("mongodb.queue_wait", "\"0s\""),
        ("mongodb.queue_wait", "\"1m\""),
        ("mongodb.max_execution_time", "\"1s\""),
        ("mongodb.max_execution_time", "\"1h\""),
    ];
    for (key, value) in cases {
        let mut text = set(&base, key, value);
        if key == "mongodb.max_result_bytes" || key == "mongodb.max_result_rows" {
            text = set(&text, "server.max_result_memory_bytes", "2147483648");
        }
        assert_eq!(fx.errors(&text), Vec::<String>::new(), "{key} = {value}");
    }
    // The highest `max_result_bytes` needs a budget of twice that size (C23).
    let text = set(&base, "mongodb.max_result_bytes", "1073741824");
    let text = set(&text, "server.max_result_memory_bytes", "2147483648");
    assert_eq!(fx.errors(&text), Vec::<String>::new());
    let text = set(&base, "mongodb.max_concurrent", "1");
    let text = set(&text, "mongodb.max_concurrent_per_subject", "1");
    assert_eq!(fx.errors(&text), Vec::<String>::new());
}

#[test]
fn a_whole_number_of_seconds_is_necessary_for_max_execution_time() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let e = fx.one(&set(&base, "mongodb.max_execution_time", "\"1500ms\""));
    assert!(
        e.starts_with("mongodb \"events\".max_execution_time: expected a whole number of seconds, found \"1500ms\""),
        "{e}"
    );
    assert!(fx.errors(&set(&base, "mongodb.max_execution_time", "\"2000ms\"")).is_empty());
}

#[test]
fn the_label_text_is_1_to_60_characters() {
    let fx = Fx::new();
    let base = open_file(&fx);
    assert!(fx.errors(&set(&base, "mongodb.label", &quoted(&"é".repeat(60)))).is_empty());
    let e = fx.one(&set(&base, "mongodb.label", &quoted(&"é".repeat(61))));
    assert!(e.contains("expected a text of 1 to 60 characters"), "{e}");
}

#[test]
fn the_block_label_follows_the_rule_of_13_12() {
    let fx = Fx::new();
    let base = open_file(&fx);
    for ok in ["0", "a-b", &"a".repeat(40)] {
        let text = replace_once(&base, "mongodb \"events\" {", &format!("mongodb \"{ok}\" {{"));
        assert_eq!(fx.errors(&text), Vec::<String>::new(), "{ok}");
    }
    for bad in ["Events", "-a", "a_b", &"a".repeat(41)] {
        let text = replace_once(&base, "mongodb \"events\" {", &format!("mongodb \"{bad}\" {{"));
        let e = fx.one(&text);
        assert!(
            e.starts_with(&format!(
                "mongodb \"{bad}\": expected a label that matches ^[a-z0-9][a-z0-9-]{{0,39}}$, found"
            )),
            "{e}"
        );
    }
}

#[test]
fn the_host_is_a_dns_name_or_an_ip_address_and_never_a_socket_or_a_connection_string() {
    let fx = Fx::new();
    let base = open_file(&fx);
    for ok in ["db", "db.example.org", "10.0.0.5", "::1", "2001:db8::1", "DB.Example.ORG"] {
        assert_eq!(fx.errors(&set(&base, "mongodb.host", &quoted(ok))), Vec::<String>::new(), "{ok}");
    }
    for bad in [
        "/var/run/mongodb-27017.sock",
        "/tmp",
        "bad host",
        "db:27017",
        "mongodb://db",
        "mongodb+srv://db.example.org",
        "user@db",
        "db?tls=false",
        "db/x",
        "@abstract",
        "db ",
        " db",
        "",
    ] {
        let e = fx.one(&set(&base, "mongodb.host", &quoted(bad)));
        assert!(
            e.starts_with(&format!(
                "mongodb \"events\".host: expected a DNS name of 1 to 253 bytes, or an IP address, found {}",
                quoted(bad)
            )),
            "{e}"
        );
    }
    let e = fx.one(&set(&base, "mongodb.host", &quoted(&"a".repeat(254))));
    assert!(e.contains("1 to 253 bytes"), "{e}");
}

#[test]
fn the_name_of_the_replica_set_and_of_the_auth_database_follow_their_rules() {
    let fx = Fx::new();
    let base = replica_file(&fx);
    for ok in ["rs0", "A", "0", "a_b.c-d", &"a".repeat(64)] {
        assert_eq!(fx.errors(&set(&base, "mongodb.replica_set", &quoted(ok))), Vec::<String>::new(), "{ok}");
    }
    for bad in ["_rs", ".rs", "-rs", "r s", "rs/0", "rs,0", "rs\\\\0", "é", &"a".repeat(65)] {
        let e = fx.one(&set(&base, "mongodb.replica_set", &quoted(bad)));
        assert!(e.starts_with("mongodb \"events\".replica_set: expected a text"), "{bad}: {e}");
    }
    let e = fx.one(&set(&base, "mongodb.replica_set", "\"-rs\""));
    assert!(e.contains("expected a text that matches ^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$, found \"-rs\""), "{e}");
    let base = open_file(&fx);
    for ok in ["admin", "A_b-9", &"a".repeat(63)] {
        assert_eq!(fx.errors(&set(&base, "mongodb.auth_database", &quoted(ok))), Vec::<String>::new(), "{ok}");
    }
    for bad in ["a.b", "a b", "a$", "é", &"a".repeat(64)] {
        let e = fx.one(&set(&base, "mongodb.auth_database", &quoted(bad)));
        assert!(e.starts_with("mongodb \"events\".auth_database: expected a text"), "{bad}: {e}");
    }
    let e = fx.one(&set(&base, "mongodb.auth_database", "\"a.b\""));
    assert!(e.contains("expected a text that matches ^[A-Za-z0-9_-]{1,63}$, found \"a.b\""), "{e}");
}

#[test]
fn the_list_databases_has_1_to_8_names_with_no_repeat() {
    let fx = Fx::new();
    let base = open_file(&fx);
    for n in [1, 8] {
        let text = set(&base, "mongodb.databases", &list(&names("db", n)));
        assert_eq!(fx.errors(&text), Vec::<String>::new(), "{n}");
    }
    for n in [0, 9] {
        let e = fx.one(&set(&base, "mongodb.databases", &list(&names("db", n))));
        assert!(
            e.starts_with("mongodb \"events\".databases: expected a list of 1 to 8 texts, each of 1 to 63 bytes"),
            "{n}: {e}"
        );
    }
    let ok = ["a", "A", "a_b", "a-b", "0", &"d".repeat(63)];
    for item in ok {
        assert_eq!(fx.errors(&set(&base, "mongodb.databases", &format!("[\"{item}\"]"))), Vec::<String>::new());
    }
    for bad in ["a.b", "a b", "a$", "é", "a/b", "", &"d".repeat(64)] {
        let e = fx.one(&set(&base, "mongodb.databases", &format!("[\"{bad}\"]")));
        assert!(e.starts_with("mongodb \"events\".databases: expected "), "{bad}: {e}");
    }
    let e = fx.one(&set(&base, "mongodb.databases", "[\"a.b\"]"));
    assert!(e.contains("expected an item that matches ^[A-Za-z0-9_-]{1,63}$, found \"a.b\""), "{e}");
    // A repeated item: the comparison is exact and case-sensitive.
    let e = fx.one(&set(&base, "mongodb.databases", "[\"a\", \"b\", \"a\"]"));
    assert!(e.contains("expected a list with no repeated item, found \"a\" more than once"), "{e}");
    assert_eq!(fx.errors(&set(&base, "mongodb.databases", "[\"a\", \"A\"]")), Vec::<String>::new());
}

#[test]
fn the_list_collections_has_1_to_64_names_with_no_repeat() {
    let fx = Fx::new();
    let base = open_file(&fx);
    for n in [1, 64] {
        let text = set(&base, "mongodb.collections", &list(&names("c", n)));
        assert_eq!(fx.errors(&text), Vec::<String>::new(), "{n}");
    }
    for n in [0, 65] {
        let e = fx.one(&set(&base, "mongodb.collections", &list(&names("c", n))));
        assert!(
            e.starts_with("mongodb \"events\".collections: expected a list of 1 to 64 texts, each of 1 to 100 bytes"),
            "{n}: {e}"
        );
    }
    for ok in ["a", "_a", "0", "a.b", "a-b", "node_load", "system", "systemx.y", &format!("a{}", "b".repeat(99))] {
        assert_eq!(fx.errors(&set(&base, "mongodb.collections", &format!("[\"{ok}\"]"))), Vec::<String>::new(), "{ok}");
    }
    for bad in ["system.profile", "system.views", "$cmd", "a b", ".a", "-a", "a$b", "é", &"b".repeat(101)] {
        let e = fx.one(&set(&base, "mongodb.collections", &format!("[\"{bad}\"]")));
        assert!(e.starts_with("mongodb \"events\".collections: expected "), "{bad}: {e}");
    }
    let e = fx.one(&set(&base, "mongodb.collections", "[\"system.profile\"]"));
    assert!(e.contains("found \"system.profile\""), "{e}");
    let e = fx.one(&set(&base, "mongodb.collections", "[\"x\", \"x\"]"));
    assert!(e.contains("expected a list with no repeated item, found \"x\" more than once"), "{e}");
}

#[test]
fn the_list_hosts_has_2_to_7_items_of_the_form_host_and_port() {
    let fx = Fx::new();
    let base = replica_file(&fx);
    let hosts = |n: usize| list(&(0..n).map(|i| format!("mongo-{i}:27017")).collect::<Vec<_>>());
    for n in [2, 7] {
        assert_eq!(fx.errors(&set(&base, "mongodb.hosts", &hosts(n))), Vec::<String>::new(), "{n}");
    }
    for n in [0, 1, 8] {
        let e = fx.one(&set(&base, "mongodb.hosts", &hosts(n)));
        assert!(e.starts_with("mongodb \"events\".hosts: expected a list of 2 to 7 texts"), "{n}: {e}");
    }
    let with = |items: &[&str]| {
        let mut all: Vec<String> = vec!["mongo-0:27017".to_string()];
        all.extend(items.iter().map(|s| s.to_string()));
        set(&base, "mongodb.hosts", &list(&all))
    };
    for ok in
        ["db:1", "db:65535", "10.0.0.5:27017", "[::1]:27017", "[2001:db8::1]:27018", "DB.Example.ORG:27017", "a-b.c:9"]
    {
        assert_eq!(fx.errors(&with(&[ok])), Vec::<String>::new(), "{ok}");
    }
    for bad in [
        "db",
        "db:",
        ":27017",
        "db:0",
        "db:65536",
        "db:027017",
        "db:+1",
        "db:1e3",
        "db:port",
        "::1:27017",
        "[::1]",
        "[::1]27017",
        "[::1:27017",
        "[db]:27017",
        "[10.0.0.1]:27017",
        "/var/run/mongodb.sock",
        "/tmp/x:27017",
        "mongodb://db:27017",
        "mongodb+srv://db",
        "user@db:27017",
        "db:27017?tls=false",
        "db:27017/admin",
        "db :27017",
        "d_b:27017",
        "db:27017:27017",
    ] {
        let e = fx.one(&with(&[bad]));
        assert!(
            e.starts_with("mongodb \"events\".hosts: expected an item that matches host:port, with an IPv6 address in brackets and a port from 1 to 65535, found")
                || e.starts_with("mongodb \"events\".hosts: expected a list of 2 to 7 texts"),
            "{bad}: {e}"
        );
    }
    // A repeat: names without case, equal ports, equal IP addresses.
    for dup in ["mongo-0:27017", "MONGO-0:27017", "Mongo-0:27017"] {
        let e = fx.one(&with(&[dup]));
        assert!(e.contains("expected a list with no repeated item, found"), "{dup}: {e}");
    }
    assert_eq!(fx.errors(&with(&["mongo-0:27018"])), Vec::<String>::new());
    let e = fx.one(&set(&base, "mongodb.hosts", "[\"[::1]:27017\", \"[0:0:0:0:0:0:0:1]:27017\"]"));
    assert!(e.contains("no repeated item"), "{e}");
    let e = fx.one(&set(&base, "mongodb.hosts", "[\"[2001:DB8::1]:27017\", \"[2001:db8::1]:27017\"]"));
    assert!(e.contains("no repeated item"), "{e}");
    // The pure function agrees.
    assert_eq!(stage2::parse_host_port("[::1]:27017"), Some(("::1".to_string(), 27017)));
    assert_eq!(stage2::parse_host_port("Db.Example:9"), Some(("Db.Example".to_string(), 9)));
    assert_eq!(stage2::parse_host_port("db:0"), None);
}

#[test]
fn an_unknown_key_and_a_duplicate_key_stop_the_start() {
    let fx = Fx::new();
    let base = open_file(&fx);
    for (name, value) in [
        ("uri", "\"mongodb://db\""),
        ("srv", "true"),
        ("auth_mechanism", "\"PLAIN\""),
        ("read_concern", "\"majority\""),
        ("write_concern", "\"majority\""),
        ("compressors", "[\"zstd\"]"),
        ("tls_ca_file", "\"/x\""),
    ] {
        let text = add_line(&base, "  topology ", &format!("  {name} = {value}"));
        let e = fx.one(&text);
        assert!(e.starts_with(&format!("mongodb \"events\".{name}: unknown attribute")), "{e}");
    }
    let text = add_line(&base, "  port ", "  port = 27018");
    let errors = fx.errors(&text);
    assert!(errors.iter().any(|e| e.contains("duplicate attribute")), "{errors:?}");
    let at = base.rfind("  tls {").unwrap();
    let text = format!("{}  options {{\n  }}\n\n{}", &base[..at], &base[at..]);
    let e = fx.one(&text);
    assert!(e.starts_with("mongodb \"events\".options: unknown block"), "{e}");
    let text = replace_once(&base, "    ca_file = ", "    server_name = \"hune-mongo-ro\"\n    ca_file = ");
    let e = fx.one(&text);
    assert!(e.starts_with("mongodb \"events\".tls.server_name: unknown attribute"), "{e}");
}

// ---------------------------------------------------------------------------------------
// The contradictions C14, C15 and C22 to C27.
// ---------------------------------------------------------------------------------------

#[test]
fn c14_the_file_has_one_to_sixty_four_source_blocks_in_all_of_the_three_kinds() {
    let fx = Fx::new();
    let local = fx.local();
    let (head, ch) = local.split_once("\nclickhouse \"prod\" {").unwrap();
    let pg = fx.substitute(&postgres::block(false));
    let pg_body = pg.split_once("postgres \"metrics\" {").unwrap().1.to_string();
    let mg = fx.substitute(&block(false, false));
    let mg_body = mg.split_once("mongodb \"events\" {").unwrap().1.to_string();
    let many = |clickhouse: usize, postgres: usize, mongodb: usize| {
        let mut text = head.to_string();
        for i in 0..clickhouse {
            text.push_str(&format!("\nclickhouse \"c{i}\" {{{ch}"));
        }
        for i in 0..postgres {
            text.push_str(&format!("\npostgres \"p{i}\" {{{pg_body}"));
        }
        for i in 0..mongodb {
            text.push_str(&format!("\nmongodb \"m{i}\" {{{mg_body}"));
        }
        text
    };
    let all =
        "expected 1 to 64 blocks `clickhouse \"<label>\"`, `postgres \"<label>\"` or `mongodb \"<label>\"` in all";
    // 64 in all, of the three kinds.
    let loaded = fx.load(&many(22, 21, 21)).unwrap();
    assert_eq!((loaded.config.clickhouse.len(), loaded.config.postgres_sources.len()), (22, 21));
    // Only mongodb blocks: one is enough, and 64 are enough.
    assert!(fx.load(&many(0, 0, 1)).is_ok());
    assert!(fx.load(&many(0, 0, 64)).is_ok());
    // 65 in all is a contradiction; 65 of one kind is an error of the engine.
    assert_eq!(fx.one(&many(22, 22, 21)), format!("clickhouse, postgres, mongodb: {all}, found 65"));
    assert_eq!(fx.one(&many(0, 0, 65)), "mongodb: expected 0 to 64 blocks `mongodb \"<label>\"`, found 65");
    assert_eq!(fx.one(&many(0, 0, 0)), format!("clickhouse, postgres, mongodb: {all}, found 0"));
}

#[test]
fn c15_a_label_is_unique_across_the_three_kinds() {
    let fx = Fx::new();
    let base = open_file(&fx);
    // Later than a clickhouse block.
    let text = replace_once(&base, "mongodb \"events\" {", "mongodb \"prod\" {");
    let first = line_of(&text, "clickhouse \"prod\" {");
    let second = line_of(&text, "mongodb \"prod\" {");
    assert_eq!(
        fx.one(&text),
        format!(
            "mongodb \"prod\": duplicate label; the first block with this label is `clickhouse \"prod\"` at line {first} (line {second})"
        )
    );
    // Later than a postgres block.
    let text = append(&base, &fx.substitute(&postgres::block(false)));
    let text = replace_once(&text, "postgres \"metrics\" {", "postgres \"events\" {");
    let first = line_of(&text, "mongodb \"events\" {");
    let second = line_of(&text, "postgres \"events\" {");
    assert_eq!(
        fx.one(&text),
        format!(
            "postgres \"events\": duplicate label; the first block with this label is `mongodb \"events\"` at line {first} (line {second})"
        )
    );
    // The first block in the file owns the label, also when it is a mongodb block.
    let local = fx.local();
    let (head, ch) = local.split_once("\nclickhouse \"prod\" {").unwrap();
    let mg = fx.substitute(&block(false, false)).replace("mongodb \"events\"", "mongodb \"prod\"");
    let text = format!("{head}\n{mg}\nclickhouse \"prod\" {{{ch}");
    let first = line_of(&text, "mongodb \"prod\" {");
    let second = line_of(&text, "clickhouse \"prod\" {");
    assert_eq!(
        fx.one(&text),
        format!(
            "clickhouse \"prod\": duplicate label; the first block with this label is `mongodb \"prod\"` at line {first} (line {second})"
        )
    );
    // The same label inside one kind stays an error of the engine.
    let text = append(&base, &fx.substitute(&block(false, false)));
    let e = fx.one(&text);
    assert!(e.starts_with("mongodb \"events\": duplicate label; the first block with this label is at line"), "{e}");
}

#[test]
fn c22_the_block_tls_exists_only_with_verify_full() {
    let fx = Fx::new();
    let base = open_file(&fx);
    // verify-full without the block.
    let text = remove_tls_of_mongodb(&base);
    assert_eq!(
        fx.one(&text),
        format!(
            "mongodb \"events\".tls: missing block; expected when `mongodb \"events\".tls_mode` is \"verify-full\" (line {})",
            line_of(&text, "mongodb \"events\" {")
        )
    );
    // disable with the block.
    let text = set(&base, "mongodb.tls_mode", "\"disable\"");
    let tls_line = text.lines().enumerate().filter(|(_, l)| *l == "  tls {").map(|(i, _)| i + 1).last().unwrap();
    assert_eq!(
        fx.one(&text),
        format!(
            "mongodb \"events\".tls: not permitted when `mongodb \"events\".tls_mode` is \"disable\"; the key is only for `mongodb \"events\".tls_mode` = \"verify-full\" (line {tls_line})"
        )
    );
    // disable without the block is valid.
    assert!(fx.errors(&disabled_file(&fx)).is_empty());
    // The key of the block is necessary when the block exists.
    let text = remove(&base, "mongodb.tls.ca_file");
    assert!(fx.one(&text).starts_with("mongodb \"events\".tls.ca_file: missing attribute; expected an absolute path"));
}

#[test]
fn c23_the_memory_budget_is_at_least_two_times_the_result_bytes_of_each_source() {
    let fx = Fx::new();
    let base = open_file(&fx);
    // The ClickHouse block has the same limit: C11 also finds it.
    let text = set(&base, "server.max_result_memory_bytes", "33554432");
    let errors = fx.errors(&text);
    assert!(
        errors.contains(
            &"server.max_result_memory_bytes, mongodb \"events\".max_result_bytes: `server.max_result_memory_bytes` (33554432) is less than 2 x `mongodb \"events\".max_result_bytes` (33554432)".to_string()
        ),
        "{errors:#?}"
    );
    // The exact limit (2 x) is valid.
    let text = set(&text, "mongodb.max_result_bytes", "16777216");
    let text = set(&text, "clickhouse.max_result_bytes", "16777216");
    assert!(fx.errors(&text).is_empty());
    let text = set(&text, "mongodb.max_result_bytes", "16777217");
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("(33554432) is less than 2 x `mongodb \"events\".max_result_bytes` (16777217)"));
}

#[test]
fn c24_the_value_for_each_subject_is_at_most_max_concurrent() {
    let fx = Fx::new();
    let base = authzen_file(&fx);
    let text = set(&base, "mongodb.max_concurrent_per_subject", "20");
    assert_eq!(
        fx.one(&text),
        "mongodb \"events\".max_concurrent_per_subject, mongodb \"events\".max_concurrent: `mongodb \"events\".max_concurrent_per_subject` (20) is more than `mongodb \"events\".max_concurrent` (16)"
    );
    assert!(fx.errors(&set(&base, "mongodb.max_concurrent_per_subject", "16")).is_empty());
    let text = set(&set(&base, "mongodb.max_concurrent_per_subject", "256"), "mongodb.max_concurrent", "256");
    assert!(fx.errors(&text).is_empty());
}

#[test]
fn c25_max_concurrent_per_subject_exists_only_in_the_mode_authzen() {
    let fx = Fx::new();
    let text = append(&fx.local(), &fx.substitute(&block(false, true)));
    assert_eq!(
        fx.one(&text),
        format!(
            "mongodb \"events\".max_concurrent_per_subject: not permitted when `access.mode` is \"open\"; the key is only for `access.mode` = \"authzen\" (line {})",
            line_of(&text, "max_concurrent_per_subject")
        )
    );
    let text = remove(&authzen_file(&fx), "mongodb.max_concurrent_per_subject");
    assert_eq!(
        fx.one(&text),
        format!(
            "mongodb \"events\".max_concurrent_per_subject: missing attribute; expected an integer from 1 to 256 (line {})",
            line_of(&text, "mongodb \"events\" {")
        )
    );
}

#[test]
fn c26_the_keys_of_the_topology_direct_and_the_keys_of_the_topology_replica_set_do_not_mix() {
    let fx = Fx::new();
    let line_of_block = |text: &str| line_of(text, "mongodb \"events\" {");
    let refused = |key: &str, topology: &str, other: &str, line: usize| {
        format!(
            "mongodb \"events\".{key}: not permitted when `mongodb \"events\".topology` is \"{topology}\"; the key is only for `mongodb \"events\".topology` = \"{other}\" (line {line})"
        )
    };
    // direct with `hosts` and `replica_set`.
    let base = open_file(&fx);
    let text = add_line(&base, "  user_file ", "  hosts = [\"mongo-0:27017\", \"mongo-1:27017\"]");
    assert_eq!(fx.one(&text), refused("hosts", "direct", "replica_set", line_of(&text, "  hosts = ")));
    let text = add_line(&base, "  user_file ", "  replica_set = \"rs0\"");
    assert_eq!(fx.one(&text), refused("replica_set", "direct", "replica_set", line_of(&text, "  replica_set = ")));
    // direct without `host`, without `port`.
    for (key, expected) in
        [("host", "a DNS name of 1 to 253 bytes, or an IP address"), ("port", "an integer from 1 to 65535")]
    {
        let text = remove(&base, &format!("mongodb.{key}"));
        assert_eq!(
            fx.one(&text),
            format!("mongodb \"events\".{key}: missing attribute; expected {expected} (line {})", line_of_block(&text))
        );
    }
    // replica_set with `host` and `port`.
    let base = replica_file(&fx);
    let text = add_line(&base, "  user_file ", "  host = \"mongo-0\"");
    assert_eq!(fx.one(&text), refused("host", "replica_set", "direct", line_of(&text, "  host = ")));
    let text = add_line(&base, "  user_file ", "  port = 27017");
    assert_eq!(fx.one(&text), refused("port", "replica_set", "direct", line_of(&text, "  port = ")));
    // replica_set without `hosts`, without `replica_set`.
    let text = remove(&base, "mongodb.hosts");
    let e = fx.one(&text);
    assert!(
        e.starts_with("mongodb \"events\".hosts: missing attribute; expected a list of 2 to 7 texts")
            && e.ends_with(&format!("(line {})", line_of_block(&text))),
        "{e}"
    );
    let text = remove(&base, "mongodb.replica_set");
    assert_eq!(
        fx.one(&text),
        format!(
            "mongodb \"events\".replica_set: missing attribute; expected a text of 1 to 64 bytes with no control character (line {})",
            line_of_block(&text)
        )
    );
    // All four wrong keys give four errors, in the order of the table.
    let mixed = add_line(
        &add_line(&open_file(&fx), "  user_file ", "  hosts = [\"a:1\", \"b:1\"]"),
        "  user_file ",
        "  replica_set = \"rs0\"",
    );
    let errors = fx.errors(&mixed);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors[0].starts_with("mongodb \"events\".hosts: not permitted"));
    assert!(errors[1].starts_with("mongodb \"events\".replica_set: not permitted"));
    // The topology itself is necessary.
    let e = fx.one(&remove(&open_file(&fx), "mongodb.topology"));
    assert!(
        e.starts_with("mongodb \"events\".topology: missing attribute; expected \"direct\" or \"replica_set\""),
        "{e}"
    );
}

#[test]
fn c27_the_list_databases_holds_none_of_admin_local_and_config() {
    let fx = Fx::new();
    let base = open_file(&fx);
    for name in ["admin", "local", "config"] {
        let text = set(&base, "mongodb.databases", &format!("[\"analytics\", \"{name}\"]"));
        assert_eq!(
            fx.one(&text),
            format!(
                "mongodb \"events\".databases: expected a name that is not \"admin\", \"local\" or \"config\", found \"{name}\" (line {})",
                line_of(&text, "databases")
            )
        );
    }
    // The comparison is case-sensitive.
    assert!(fx.errors(&set(&base, "mongodb.databases", "[\"Admin\", \"LOCAL\", \"Config\"]")).is_empty());
    // The key `auth_database` can be `admin`, `local` or `config`.
    assert!(fx.errors(&set(&base, "mongodb.auth_database", "\"admin\"")).is_empty());
    assert!(fx.errors(&set(&base, "mongodb.auth_database", "\"local\"")).is_empty());
    // Two refused names give two errors.
    let e = fx.errors(&set(&base, "mongodb.databases", "[\"admin\", \"local\"]"));
    assert_eq!(e.len(), 2, "{e:?}");
}

#[test]
fn stage_2_and_3_do_not_run_when_stage_1_has_an_error() {
    let fx = Fx::new();
    let base = open_file(&fx);
    // A wrong port (stage 1) and a duplicate label (stage 3): only the first one shows.
    let text = replace_once(&base, "mongodb \"events\" {", "mongodb \"prod\" {");
    let text = set(&text, "mongodb.port", "0");
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("mongodb \"prod\".port:"));
    // A bad item of a list (stage 2) hides C27 (stage 3).
    let text = set(&base, "mongodb.databases", "[\"a.b\", \"admin\"]");
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("expected an item that matches"), "{errors:?}");
}

// ---------------------------------------------------------------------------------------
// The warning W8.
// ---------------------------------------------------------------------------------------

#[test]
fn w8_tls_mode_disable_for_a_host_that_is_not_a_loopback_address() {
    let fx = Fx::new();
    let base = disabled_file(&fx);
    let loaded = fx.load(&base).unwrap();
    let w8: Vec<&Warning> = loaded.warnings.iter().filter(|w| w.key.starts_with("mongodb")).collect();
    assert_eq!(w8.len(), 1);
    assert_eq!(w8[0].key, "mongodb \"events\".tls_mode");
    assert_eq!(w8[0].message, "mongodb \"events\".tls_mode is disable for a host that is not a loopback address.");
    assert_eq!(loaded.warnings.last().unwrap().key, "mongodb \"events\".tls_mode");
    for host in ["127.0.0.1", "127.9.9.9", "localhost", "LocalHost", "::1"] {
        let loaded = fx.load(&set(&base, "mongodb.host", &quoted(host))).unwrap();
        assert!(loaded.warnings.iter().all(|w| !w.key.starts_with("mongodb")), "{host}");
    }
    // No warning with verify-full.
    let loaded = fx.load(&open_file(&fx)).unwrap();
    assert!(loaded.warnings.iter().all(|w| !w.key.starts_with("mongodb")));
}

#[test]
fn w8_in_the_topology_replica_set_one_host_that_is_not_a_loopback_address_is_enough() {
    let fx = Fx::new();
    let base = set(&remove_tls_of_mongodb(&replica_file(&fx)), "mongodb.tls_mode", "\"disable\"");
    let count = |text: &str| {
        let loaded = fx.load(text).unwrap();
        loaded.warnings.iter().filter(|w| w.key == "mongodb \"events\".tls_mode").count()
    };
    assert_eq!(count(&base), 1);
    let all_loopback = set(&base, "mongodb.hosts", "[\"127.0.0.1:27017\", \"localhost:27018\", \"[::1]:27019\"]");
    assert_eq!(count(&all_loopback), 0);
    let one_remote = set(&base, "mongodb.hosts", "[\"127.0.0.1:27017\", \"localhost:27018\", \"mongo-2:27019\"]");
    assert_eq!(count(&one_remote), 1);
    // One warning for the block, not one for each host.
    assert_eq!(count(&set(&base, "mongodb.hosts", "[\"mongo-0:1\", \"mongo-1:1\"]")), 1);
}

#[test]
fn each_mongodb_block_gets_its_own_warning_in_file_order_after_the_postgres_warnings() {
    let fx = Fx::new();
    let base = disabled_file(&fx);
    let second = base.split_once("\nmongodb \"events\" {").unwrap().1;
    let text = format!("{base}\nmongodb \"other\" {{{second}");
    let pg = fx.substitute(&postgres::block(false));
    let pg = set(&remove_tls_of_mongodb(&pg), "postgres.sslmode", "\"disable\"");
    let text = format!("{text}\n{pg}");
    let loaded = fx.load(&text).unwrap();
    let keys: Vec<&str> = loaded
        .warnings
        .iter()
        .filter(|w| w.key.starts_with("mongodb") || w.key.starts_with("postgres"))
        .map(|w| w.key.as_str())
        .collect();
    assert_eq!(keys, ["postgres \"metrics\".sslmode", "mongodb \"events\".tls_mode", "mongodb \"other\".tls_mode"]);
}

// ---------------------------------------------------------------------------------------
// The CA file, the build without the driver and the read modes.
// ---------------------------------------------------------------------------------------

#[test]
fn the_ca_file_of_the_block_gets_the_checks_of_13_13() {
    let fx = Fx::new();
    let base = open_file(&fx);
    crate::config::test_support::write_file(fx.dir.path().join("bad.pem"), "not a certificate\n").unwrap();
    let e = fx.one(&set(&base, "mongodb.tls.ca_file", &quoted(&fx.path("bad.pem"))));
    assert!(e.starts_with("mongodb \"events\".tls.ca_file:"), "{e}");
    let e = fx.one(&set(&base, "mongodb.tls.ca_file", "\"relative/ca.pem\""));
    assert!(e.starts_with("mongodb \"events\".tls.ca_file: expected an absolute path"), "{e}");
    let e = fx.one(&set(&base, "mongodb.tls.ca_file", "\"/nonexistent/ca.pem\""));
    assert!(e.starts_with("mongodb \"events\".tls.ca_file:"), "{e}");
}

#[test]
fn a_build_without_the_driver_refuses_each_mongodb_block_and_names_the_missing_driver() {
    let fx = Fx::new();
    let base = open_file(&fx);
    crate::config::test_support::set_mongo_driver(false);
    let result = fx.errors(&base);
    crate::config::test_support::set_mongo_driver(true);
    assert_eq!(
        result,
        [format!(
            "mongodb \"events\": this build has no MongoDB driver (line {})",
            line_of(&base, "mongodb \"events\" {")
        )]
    );
    // One error for each block, in file order.
    let second = base.split_once("\nmongodb \"events\" {").unwrap().1;
    let text = format!("{base}\nmongodb \"other\" {{{second}");
    crate::config::test_support::set_mongo_driver(false);
    let result = fx.errors(&text);
    crate::config::test_support::set_mongo_driver(true);
    assert_eq!(result.len(), 2, "{result:?}");
    assert!(result[0].starts_with("mongodb \"events\": this build has no MongoDB driver"));
    assert!(result[1].starts_with("mongodb \"other\": this build has no MongoDB driver"));
    // A file with no mongodb block is valid in each build.
    crate::config::test_support::set_mongo_driver(false);
    let result = fx.errors(&fx.local());
    crate::config::test_support::set_mongo_driver(true);
    assert!(result.is_empty());
}

#[test]
fn a_build_without_the_driver_still_runs_the_checks_of_the_stage_and_loads_the_modes_that_need_no_driver() {
    let fx = Fx::new();
    let file = fx.dir.path().join("hune.hcl");
    let env = CapturedEnvironment::default();
    crate::config::test_support::write_file(&file, open_file(&fx)).unwrap();
    crate::config::test_support::set_mongo_driver(false);
    let all = load_with(&file, &env, ReadMode::All).is_err();
    let nothing = load_with(&file, &env, ReadMode::Nothing).is_err();
    let store = load_with(&file, &env, ReadMode::StoreCommands).is_ok();
    let import = load_with(&file, &env, ReadMode::ImportWrite).is_ok();
    // A bad value of the block gives its own error with the refusal, in one list.
    crate::config::test_support::write_file(&file, set(&open_file(&fx), "mongodb.auth_database", "\"a.b\"")).unwrap();
    let mixed = load_with(&file, &env, ReadMode::All);
    crate::config::test_support::set_mongo_driver(true);
    assert!(all && nothing && store && import);
    assert_eq!(mixed.unwrap_err().len(), 2);
}

#[test]
fn the_labels_of_the_blocks_are_checked_by_the_import_mode_and_not_by_the_store_modes() {
    let fx = Fx::new();
    let text = replace_once(&open_file(&fx), "mongodb \"events\" {", "mongodb \"Events\" {");
    let file = fx.dir.path().join("hune.hcl");
    crate::config::test_support::write_file(&file, &text).unwrap();
    let env = CapturedEnvironment::default();
    let errors = load_with(&file, &env, ReadMode::ImportWrite).unwrap_err();
    assert!(errors.iter().any(|e| e.to_string().starts_with("mongodb \"Events\": expected a label")), "{errors:?}");
    // The store modes ignore the sources.
    let loaded = load_with(&file, &env, ReadMode::StoreCommands).unwrap();
    assert!(!loaded.mode.is_complete());
    // The import mode reads the label and the kind, and no value of the source.
    crate::config::test_support::write_file(&file, open_file(&fx)).unwrap();
    let loaded = load_with(&file, &env, ReadMode::ImportWrite).unwrap();
    let entry = loaded.effective.iter().find(|e| e.key == "mongodb \"events\".password").unwrap();
    assert_eq!(entry.value, "<not read>");
    let entry = loaded.effective.iter().find(|e| e.key == "mongodb \"events\".user").unwrap();
    assert_eq!(entry.value, "<not read>");
}

// ---------------------------------------------------------------------------------------
// The key table and the generated documents.
// ---------------------------------------------------------------------------------------

#[test]
fn the_reference_files_hold_no_mongodb_block() {
    for profile in [Profile::Local, Profile::Distributed] {
        let text = render_reference(ReferenceFormat::Hcl, profile);
        assert!(!text.contains("mongodb"), "{profile:?}");
    }
}

#[test]
fn the_reference_document_has_the_tables_of_13_12b() {
    for needle in [
        "| `mongodb \"<id>\"` | 0 to 64 | always | never |",
        "| `mongodb \"<id>\".tls` | 0 or 1 | `tls_mode = \"verify-full\"` of the block | `tls_mode = \"disable\"` of the block |",
        "## Block `mongodb \"<id>\"`",
        "## Block `mongodb \"<id>\".tls`",
        "| `mongodb \"<id>\".topology` | \"direct\" or \"replica_set\" | no | V | always | never | `\"direct\"` |",
        "| `mongodb \"<id>\".host` | a DNS name of 1 to 253 bytes, or an IP address | no | V | `topology = \"direct\"` | `topology = \"replica_set\"` | `\"hune-mongo-ro\"` |",
        "| `mongodb \"<id>\".port` | an integer from 1 to 65535 | no | V | `topology = \"direct\"` | `topology = \"replica_set\"` | `27017` |",
        "| `mongodb \"<id>\".hosts` | a list of 2 to 7 texts, each of 1 to 262 bytes with no space and no control character | no | V | `topology = \"replica_set\"` | `topology = \"direct\"` | `[\"mongo-0:27017\", \"mongo-1:27017\", \"mongo-2:27017\"]` |",
        "| `mongodb \"<id>\".replica_set` | a text of 1 to 64 bytes with no control character | no | V | `topology = \"replica_set\"` | `topology = \"direct\"` | `\"rs0\"` |",
        "| `mongodb \"<id>\".user` | a text of 1 to 128 bytes with no control character | no | V, E, F | always | never | `user_file = \"/run/secrets/mongodb-events/username\"` |",
        "| `mongodb \"<id>\".password` | a text of 1 to 1024 bytes | yes | E, F | always | never | `password_file = \"/run/secrets/mongodb-events/password\"` |",
        "| `mongodb \"<id>\".databases` | a list of 1 to 8 texts, each of 1 to 63 bytes with no control character | no | V | always | never | `[\"analytics\"]` |",
        "| `mongodb \"<id>\".collections` | a list of 1 to 64 texts, each of 1 to 100 bytes with no control character | no | V | always | never | `[\"node_load\", \"node_events\"]` |",
        "| `mongodb \"<id>\".read_preference` | \"primary\", \"primaryPreferred\", \"secondary\", \"secondaryPreferred\" or \"nearest\" | no | V | always | never | `\"secondaryPreferred\"` |",
        "| `mongodb \"<id>\".tls_mode` | \"verify-full\" or \"disable\" | no | V | always | never | `\"verify-full\"` |",
        "| `mongodb \"<id>\".tls.ca_file` | an absolute path of 1 to 4096 bytes | no | V | `tls_mode = \"verify-full\"` | `tls_mode = \"disable\"` | `\"/run/secrets/mongodb-events-ca/ca.crt\"` |",
        "| `mongodb \"<id>\".max_query_size` | a size in bytes from 262144 to 8388608 | no | V | always | never | `1048576` |",
        "| `mongodb \"<id>\".max_concurrent` | an integer from 1 to 256 | no | V | always | never | `16` |",
        "| `mongodb \"<id>\".max_concurrent_per_subject` | an integer from 1 to 256 | no | V | `access.mode = \"authzen\"` | `access.mode = \"open\"` | `6` |",
    ] {
        // A row of a key ends with its description: the needle is the start of the row.
        assert!(REFERENCE_DOC.contains(needle.trim_end_matches(" |")), "{needle}");
    }
}

#[test]
fn the_block_of_13_12b_is_in_the_key_table_in_the_order_of_the_spec() {
    let paths: Vec<&str> = SCHEMA.keys.iter().filter(|k| k.path.starts_with("mongodb.")).map(|k| k.path).collect();
    assert_eq!(
        paths,
        [
            "mongodb.label",
            "mongodb.topology",
            "mongodb.host",
            "mongodb.port",
            "mongodb.hosts",
            "mongodb.replica_set",
            "mongodb.user",
            "mongodb.password",
            "mongodb.auth_database",
            "mongodb.databases",
            "mongodb.collections",
            "mongodb.read_preference",
            "mongodb.tls_mode",
            "mongodb.tls.ca_file",
            "mongodb.connect_timeout",
            "mongodb.max_execution_time",
            "mongodb.max_result_rows",
            "mongodb.max_result_bytes",
            "mongodb.max_query_size",
            "mongodb.max_concurrent",
            "mongodb.queue_wait",
            "mongodb.max_concurrent_per_subject",
        ]
    );
    let secrets: Vec<&str> =
        SCHEMA.keys.iter().filter(|k| k.secret && k.path.starts_with("mongodb.")).map(|k| k.path).collect();
    assert_eq!(secrets, ["mongodb.password"]);
    // Only `user` and `password` accept `_env` and `_file`.
    let forms: Vec<&str> = SCHEMA
        .keys
        .iter()
        .filter(|k| k.path.starts_with("mongodb.") && k.forms != hclconf::Forms::V)
        .map(|k| k.path)
        .collect();
    assert_eq!(forms, ["mongodb.user", "mongodb.password"]);
    // No key has a default value: each reference value is a value of the document only.
    for key in SCHEMA.keys.iter().filter(|k| k.path.starts_with("mongodb.")) {
        assert_ne!(key.doc_ref, hclconf::Ref::Absent, "{}", key.path);
        assert_eq!(key.refs, [hclconf::Ref::Absent, hclconf::Ref::Absent], "{}", key.path);
    }
}

#[test]
fn the_example_blocks_pass_the_checks_with_the_reference_values_of_the_document() {
    let fx = Fx::new();
    assert!(fx.errors(&authzen_file(&fx)).is_empty());
    assert!(fx.errors(&replica_file(&fx)).is_empty());
}

// ---------------------------------------------------------------------------------------
// The typed result.
// ---------------------------------------------------------------------------------------

fn source(loaded: &LoadedConfig, index: usize) -> &hune_source::MongoSourceConfig {
    &loaded.config.mongo_sources[index]
}

#[test]
fn the_typed_result_holds_every_field_of_the_direct_block() {
    use hune_source::{MongoTlsMode, MongoTopology};
    let fx = Fx::new();
    let loaded = fx.load(&authzen_file(&fx)).unwrap();
    assert_eq!(loaded.config.mongo_sources.len(), 1);
    let m = source(&loaded, 0);
    assert_eq!((m.id.as_str(), m.label.as_str()), ("events", "Events"));
    assert_eq!(m.topology, MongoTopology::Direct { host: "hune-mongo-ro".to_string(), port: 27017 });
    assert_eq!((m.user.as_str(), m.password.as_str()), ("hune_ro", "mgepass"));
    assert_eq!(m.auth_database, "admin");
    assert_eq!(m.databases, ["analytics"]);
    assert_eq!(m.collections, ["node_load", "node_events"]);
    assert_eq!(m.read_preference, hune_core::mongo::ReadPreference::SecondaryPreferred);
    assert_eq!(m.tls_mode, MongoTlsMode::VerifyFull);
    assert_eq!(m.tls.as_ref().unwrap().ca_pem, CA_A.as_bytes());
    assert_eq!(m.connect_timeout, Duration::from_secs(5));
    assert_eq!(m.max_execution_time, Duration::from_secs(60));
    assert_eq!((m.max_result_rows, m.max_result_bytes, m.max_query_size), (100_000, 33_554_432, 1_048_576));
    assert_eq!((m.max_concurrent, m.queue_wait), (16, Duration::from_secs(5)));
    assert_eq!(m.max_concurrent_per_subject, Some(6));
    // The other lists keep their own field.
    assert_eq!((loaded.config.clickhouse.len(), loaded.config.postgres_sources.len()), (1, 0));
}

#[test]
fn the_typed_result_of_the_replica_set_has_the_members_split_and_the_mode_open_has_no_limit_per_subject() {
    use hune_source::{MongoHost, MongoTopology};
    let fx = Fx::new();
    let text = set(&replica_file(&fx), "mongodb.hosts", "[\"mongo-0:27017\", \"[2001:db8::1]:27018\", \"10.0.0.7:9\"]");
    let loaded = fx.load(&text).unwrap();
    let m = source(&loaded, 0);
    let host = |h: &str, port: u16| MongoHost { host: h.to_string(), port };
    assert_eq!(
        m.topology,
        MongoTopology::ReplicaSet {
            hosts: vec![host("mongo-0", 27017), host("2001:db8::1", 27018), host("10.0.0.7", 9)],
            replica_set: "rs0".to_string()
        }
    );
    assert_eq!(m.max_concurrent_per_subject, None);
}

#[test]
fn a_block_with_tls_mode_disable_has_no_tls_and_each_read_preference_is_typed() {
    use hune_core::mongo::ReadPreference as R;
    use hune_source::MongoTlsMode;
    let fx = Fx::new();
    let loaded = fx.load(&disabled_file(&fx)).unwrap();
    assert_eq!(source(&loaded, 0).tls_mode, MongoTlsMode::Disable);
    assert!(source(&loaded, 0).tls.is_none());
    for (text, expected) in [
        ("primary", R::Primary),
        ("primaryPreferred", R::PrimaryPreferred),
        ("secondary", R::Secondary),
        ("secondaryPreferred", R::SecondaryPreferred),
        ("nearest", R::Nearest),
    ] {
        let loaded = fx.load(&set(&open_file(&fx), "mongodb.read_preference", &quoted(text))).unwrap();
        assert_eq!(source(&loaded, 0).read_preference, expected, "{text}");
    }
}

#[test]
fn the_blocks_keep_the_file_order_in_the_typed_result() {
    let fx = Fx::new();
    let base = open_file(&fx);
    let second = base.split_once("\nmongodb \"events\" {").unwrap().1;
    let text = format!("{base}\nmongodb \"alpha\" {{{second}");
    let loaded = fx.load(&text).unwrap();
    let ids: Vec<&str> = loaded.config.mongo_sources.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["events", "alpha"]);
}

#[test]
fn a_partial_read_mode_gives_placeholders_for_the_values_that_it_did_not_read() {
    let fx = Fx::new();
    let file = fx.dir.path().join("hune.hcl");
    let env = CapturedEnvironment::default();
    crate::config::test_support::write_file(&file, open_file(&fx)).unwrap();
    let loaded = load_with(&file, &env, ReadMode::ImportWrite).unwrap();
    let m = source(&loaded, 0);
    assert_eq!((m.id.as_str(), m.user.as_str(), m.password.as_str()), ("events", "", ""));
    assert_eq!(m.databases, ["analytics"]);
}
