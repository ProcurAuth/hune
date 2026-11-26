// SPDX-License-Identifier: MIT OR Apache-2.0
//! Black-box tests of the command line of the configuration: `check-config`,
//! `print-config`, `print-reference` and `version`, with the exit codes of 14.12.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/config");
const REFERENCE_DOC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/configuration-reference.md");

/// The secret files that the example files name.
const FILES: [(&str, &str, &str); 14] = [
    ("/run/secrets/clickhouse/password", "ch-pw", "chpass\n"),
    ("/run/secrets/clickhouse/ca.pem", "ch-ca.pem", "@ca-a.pem"),
    ("/run/secrets/pg/username", "pg-user", "app\n"),
    ("/run/secrets/pg/password", "pg-pw", "pgpass\n"),
    ("/run/secrets/pg-ca/ca.crt", "pg-ca.crt", "@ca-b.pem"),
    ("/run/secrets/edge/jwks.json", "jwks.json", "@jwks.json"),
    ("/run/secrets/pdp/token", "pdp-token", "pdptoken\n"),
    ("/run/secrets/pdp/ca.pem", "pdp-ca.pem", "@ca-a.pem"),
    ("/run/secrets/postgres-metrics/username", "pgm-user", "hune_ro\n"),
    ("/run/secrets/postgres-metrics/password", "pgm-pw", "pgmpass-cli\n"),
    ("/run/secrets/postgres-metrics-ca/ca.crt", "pgm-ca.crt", "@ca-a.pem"),
    ("/run/secrets/mongodb-events/username", "mge-user", "hune_ro\n"),
    ("/run/secrets/mongodb-events/password", "mge-pw", "mgepass-cli\n"),
    ("/run/secrets/mongodb-events-ca/ca.crt", "mge-ca.crt", "@ca-a.pem"),
];

/// The example block of 13.12a, for the mode `open`.
const POSTGRES_BLOCK: &str = r#"
postgres "metrics" {
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

  tls {
    ca_file     = "/run/secrets/postgres-metrics-ca/ca.crt"
    server_name = "hune-pg-ro"
  }
}
"#;

/// The example block of 13.12b, for the mode `open`.
const MONGODB_BLOCK: &str = r#"
mongodb "events" {
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

  tls {
    ca_file = "/run/secrets/mongodb-events-ca/ca.crt"
  }
}
"#;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(Path::new(FIXTURES).join(name)).unwrap()
}

struct Setup {
    dir: tempfile::TempDir,
}

impl Setup {
    fn new() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        for (_, name, content) in FILES {
            let content = match content.strip_prefix('@') {
                Some(f) => fixture(f),
                None => content.to_string(),
            };
            std::fs::write(dir.path().join(name), content).unwrap();
        }
        Setup { dir }
    }

    fn path(&self, name: &str) -> String {
        self.dir.path().join(name).display().to_string()
    }

    /// The text of an example file with the paths of the test files.
    fn config_text(&self, fixture_name: &str) -> String {
        let mut text = fixture(fixture_name);
        for (original, name, _) in FILES {
            text = text.replace(original, &self.path(name));
        }
        text
    }

    fn config(&self, fixture_name: &str) -> PathBuf {
        self.write("hune.hcl", &self.config_text(fixture_name))
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    /// The text of a block with the paths of the test files.
    fn back_paths(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (original, name, _) in FILES {
            out = out.replace(original, &self.path(name));
        }
        out
    }

    fn back(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (original, name, _) in FILES {
            out = out.replace(&self.path(name), original);
        }
        out
    }
}

fn hune(args: &[&str], env: &[(&str, &str)]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hune")).args(args).env_clear().envs(env.iter().copied()).output().unwrap()
}

fn text_of(bytes: &[u8]) -> String {
    text(bytes)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn check_config_on_a_good_file_prints_no_error_and_exits_0() {
    let s = Setup::new();
    let config = s.config("local.hcl");
    let out = hune(&["check-config", "--config", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), "");
    // The warning W1 goes on the standard error.
    assert_eq!(
        text(&out.stderr),
        "warning: access.mode: access.mode is open: Hune checks no identity and asks no PDP. A proxy that authenticates the users must guard each route of server.listen.\n"
    );
}

#[test]
fn check_config_accepts_the_short_option_and_the_distributed_file() {
    let s = Setup::new();
    let config = s.config("distributed.hcl");
    let out = hune(&["check-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "");
    assert_eq!(
        text(&out.stderr),
        "warning: server.listen: server.listen has a wildcard host: make sure that only the edge reaches this port (ProcurAuth 9.4).\n"
    );
}

#[test]
fn k10_check_config_on_a_file_with_five_errors_prints_five_errors_and_exits_1() {
    let s = Setup::new();
    let text_in = s
        .config_text("local.hcl")
        .replace("max_connections         = 1024", "max_connections         = \"1024\"")
        .replace("level = \"info\"", "level = \"loud\"")
        .replace("history_keep    = 100", "history_keep    = 0")
        .replace("queue_wait         = \"5s\"", "queue_wait         = \"2m\"")
        .replace("base_path               = \"/\"", "base_path               = \"/\"\n  colour = \"red\"");
    let config = s.write("bad.hcl", &text_in);
    let out = hune(&["check-config", "--config", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    let err = text(&out.stderr);
    let lines: Vec<&str> = err.lines().collect();
    assert_eq!(lines.len(), 5, "{err}");
    assert!(lines[0].starts_with("server.colour: unknown attribute (line "), "{err}");
    assert!(
        lines
            .iter()
            .any(|l| l
                .starts_with("server.max_connections: expected an integer from 16 to 65536, found \"1024\" (line "))
    );
    assert!(lines.iter().any(|l| {
        l.starts_with("log.level: expected \"error\", \"warn\", \"info\" or \"debug\", found \"loud\" (line ")
    }));
    assert!(lines.iter().any(|l| {
        l.starts_with("storage.volume \"main\".history_keep: expected an integer from 1 to 10000, found 0 (line ")
    }));
    assert!(lines.iter().any(|l| {
        l.starts_with("clickhouse \"prod\".queue_wait: expected a duration from 0s to 1m, found \"2m\" (line ")
    }));
}

#[test]
fn a_missing_secret_file_stops_check_config_with_the_path() {
    let s = Setup::new();
    let config = s.config("local.hcl");
    std::fs::remove_file(s.dir.path().join("ch-pw")).unwrap();
    let out = hune(&["check-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    assert!(err.contains("clickhouse \"prod\".password: cannot read the file "), "{err}");
    assert!(err.contains("ch-pw: No such file or directory"), "{err}");
}

#[test]
fn k11_print_config_prints_the_value_and_the_source_of_each_key() {
    let s = Setup::new();
    let config = s.config("local.hcl");
    let out = hune(&["print-config", "--config", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(s.back(&text(&out.stdout)), fixture("local.print-config.txt"));
    assert!(!text(&out.stdout).contains("chpass"));
    assert!(text(&out.stderr).starts_with("warning: access.mode:"));
}

#[test]
fn print_config_shows_the_key_query_inspect_with_its_default_and_its_source() {
    let s = Setup::new();
    let base = s.config_text("local.hcl");
    // The key is absent: the default `false` applies, with the source `default`.
    let config = s.write("absent.hcl", &base);
    let out = hune(&["print-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).contains("server.query_inspect = false  # default\n"), "{}", text(&out.stdout));
    // The key is in the file.
    let with = base.replace(
        "  base_path               = \"/\"\n",
        "  base_path               = \"/\"\n  query_inspect           = true\n",
    );
    let config = s.write("true.hcl", &with);
    let out = hune(&["print-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).contains("server.query_inspect = true  # file\n"), "{}", text(&out.stdout));
    // A variable that the file does not name changes nothing, and the key has no `_env` form.
    let out = hune(&["print-config", "-c", config.to_str().unwrap()], &[("HUNE_QUERY_INSPECT", "false")]);
    assert!(text(&out.stdout).contains("server.query_inspect = true  # file\n"));
    let env_form = base.replace(
        "  base_path               = \"/\"\n",
        "  base_path               = \"/\"\n  query_inspect_env       = \"HUNE_QUERY_INSPECT\"\n",
    );
    let config = s.write("env.hcl", &env_form);
    let out = hune(&["check-config", "-c", config.to_str().unwrap()], &[("HUNE_QUERY_INSPECT", "true")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("server.query_inspect:"), "{}", text(&out.stderr));
    // A wrong type and the contradiction of the mode authzen.
    let bad = base.replace(
        "  base_path               = \"/\"\n",
        "  base_path               = \"/\"\n  query_inspect           = \"true\"\n",
    );
    let config = s.write("type.hcl", &bad);
    let out = hune(&["check-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("server.query_inspect: expected a boolean, found \"true\""),
        "{}",
        text(&out.stderr)
    );
    // No primary store: the file stops the start (C10). The contradiction C21 is removed.
    let dist = s
        .config_text("distributed.hcl")
        .replace(
            "  base_path               = \"/\"\n",
            "  base_path               = \"/\"\n  query_inspect           = true\n",
        )
        .replace("    primary           = true\n", "    title             = \"Archive\"\n")
        .replace("    history_keep      = 100\n", "");
    let config = s.write("c10.hcl", &dist);
    let out = hune(&["check-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr)
            .contains("storage: the file has no primary store; give `primary = true` to `storage.postgres`"),
        "{}",
        text(&out.stderr)
    );
    assert!(!text(&out.stderr).contains("server.query_inspect, access.mode, storage"), "{}", text(&out.stderr));
}

#[test]
fn k16_print_config_of_a_postgres_block_shows_the_sources_and_hides_the_password() {
    let s = Setup::new();
    let text = format!("{}{}", s.config_text("local.hcl"), s.back_paths(POSTGRES_BLOCK));
    let config = s.write("pg.hcl", &text);
    let out = hune(&["print-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text_of(&out.stderr));
    let stdout = s.back(&text_of(&out.stdout));
    for line in [
        "postgres \"metrics\".host = \"hune-pg-ro\"  # file",
        "postgres \"metrics\".user = \"hune_ro\"  # file:/run/secrets/postgres-metrics/username",
        "postgres \"metrics\".password = <hidden>  # file:/run/secrets/postgres-metrics/password",
        "postgres \"metrics\".tls.server_name = \"hune-pg-ro\"  # file",
        "postgres \"metrics\".max_concurrent = 16  # file",
    ] {
        assert!(stdout.lines().any(|l| l == line), "{line}\n{stdout}");
    }
    assert!(!text_of(&out.stdout).contains("pgmpass-cli") && !text_of(&out.stderr).contains("pgmpass-cli"));
    // The ClickHouse blocks come before the PostgreSQL blocks.
    assert!(
        stdout.find("clickhouse \"prod\".queue_wait").unwrap() < stdout.find("postgres \"metrics\".label").unwrap()
    );
    // check-config accepts the file and a shared label stops it (C15).
    let check = hune(&["check-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(check.status.code(), Some(0), "{}", text_of(&check.stderr));
    let shared = s.write("shared.hcl", &text.replace("postgres \"metrics\" {", "postgres \"prod\" {"));
    let check = hune(&["check-config", "-c", shared.to_str().unwrap()], &[]);
    assert_eq!(check.status.code(), Some(1));
    assert!(
        text_of(&check.stderr).contains(
            "postgres \"prod\": duplicate label; the first block with this label is `clickhouse \"prod\"` at line"
        ),
        "{}",
        text_of(&check.stderr)
    );
    // A secret in clear text stops the start and the error never shows it.
    let clear = s.write("clear.hcl", &text.replace("password_file              =", "password                   ="));
    let check = hune(&["check-config", "-c", clear.to_str().unwrap()], &[]);
    assert_eq!(check.status.code(), Some(1));
    assert!(text_of(&check.stderr).contains("postgres \"metrics\".password: a secret accepts only"));
    // The value comes from the named variable and from nothing else.
    let env_form = text.replace(
        &format!("password_file              = \"{}\"", s.path("pgm-pw")),
        "password_env               = \"PGS_CLI_PASSWORD\"",
    );
    let config = s.write("env.hcl", &env_form);
    let out = hune(&["print-config", "-c", config.to_str().unwrap()], &[("PGS_CLI_PASSWORD", "from-env")]);
    assert_eq!(out.status.code(), Some(0), "{}", text_of(&out.stderr));
    assert!(text_of(&out.stdout).contains("postgres \"metrics\".password = <hidden>  # env:PGS_CLI_PASSWORD\n"));
    assert!(!text_of(&out.stdout).contains("from-env"));
    let out = hune(&["check-config", "-c", config.to_str().unwrap()], &[("PGPASSWORD", "x"), ("PGHOST", "y")]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text_of(&out.stderr).contains("the environment variable PGS_CLI_PASSWORD is not set"));
}

/// The refusal of a `mongodb` block by a build without the driver (8b.2): `check-config` and
/// `print-config` give the same error, and `print-config` writes nothing on the standard output.
#[cfg(not(feature = "mongo-driver"))]
#[test]
fn k18_a_build_without_the_driver_refuses_a_mongodb_block_in_check_config_and_print_config() {
    let s = Setup::new();
    let source = format!("{}{}", s.config_text("local.hcl"), s.back_paths(MONGODB_BLOCK));
    let config = s.write("mongo.hcl", &source);
    let line = source.lines().position(|l| l.starts_with("mongodb \"events\" {")).unwrap() + 1;
    let expected = format!("mongodb \"events\": this build has no MongoDB driver (line {line})\n");
    let check = hune(&["check-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(text(&check.stdout), "");
    assert_eq!(text(&check.stderr), expected);
    let out = hune(&["print-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert_eq!(text(&out.stderr), expected);
    // The checks of the stage still find a mistake of the block, with the refusal in the same list.
    let bad = s.write(
        "bad.hcl",
        &source.replace("auth_database              = \"admin\"", "auth_database              = \"a.b\""),
    );
    let check = hune(&["check-config", "-c", bad.to_str().unwrap()], &[]);
    assert_eq!(check.status.code(), Some(1));
    assert!(text(&check.stderr).contains("mongodb \"events\".auth_database: expected a text that matches"));
    assert!(text(&check.stderr).contains("this build has no MongoDB driver"));
    // A mistake of the key table comes first, alone (stage 1).
    let bad =
        s.write("bad1.hcl", &source.replace("port                       = 27017", "port                       = 0"));
    let check = hune(&["check-config", "-c", bad.to_str().unwrap()], &[]);
    assert_eq!(check.status.code(), Some(1));
    assert!(text(&check.stderr).contains("mongodb \"events\".port: expected an integer from 1 to 65535, found 0"));
    assert!(!text(&check.stderr).contains("MongoDB driver"));
    // A file with no `mongodb` block is valid.
    let good = hune(&["check-config", "-c", s.config("local.hcl").to_str().unwrap()], &[]);
    assert_eq!(good.status.code(), Some(0), "{}", text(&good.stderr));
}

/// With the driver, the example block of 13.12b gives the lines of 13.20 and hides the password.
#[cfg(feature = "mongo-driver")]
#[test]
fn k18_print_config_of_a_mongodb_block_shows_the_sources_and_hides_the_password() {
    let s = Setup::new();
    let text = format!("{}{}", s.config_text("local.hcl"), s.back_paths(MONGODB_BLOCK));
    let config = s.write("mongo.hcl", &text);
    let out = hune(&["print-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text_of(&out.stderr));
    let stdout = s.back(&text_of(&out.stdout));
    for line in [
        "mongodb \"events\".topology = \"direct\"  # file",
        "mongodb \"events\".user = \"hune_ro\"  # file:/run/secrets/mongodb-events/username",
        "mongodb \"events\".password = <hidden>  # file:/run/secrets/mongodb-events/password",
        "mongodb \"events\".databases = [\"analytics\"]  # file",
        "mongodb \"events\".collections = [\"node_load\", \"node_events\"]  # file",
    ] {
        assert!(stdout.lines().any(|l| l == line), "{line}\n{stdout}");
    }
    assert!(!text_of(&out.stdout).contains("mgepass-cli") && !text_of(&out.stderr).contains("mgepass-cli"));
}

#[test]
fn print_config_with_an_error_exits_1_and_prints_nothing_on_the_standard_output() {
    let s = Setup::new();
    let config = s.write("bad.hcl", "server {\n}\n");
    let out = hune(&["print-config", "-c", config.to_str().unwrap()], &[]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert!(
        text(&out.stderr)
            .contains("server.listen: missing attribute; expected an IP address and a port in canonical form (line 1)")
    );
}

#[test]
fn print_config_reads_a_value_from_the_named_variable_and_nothing_else() {
    let s = Setup::new();
    let config =
        s.config_text("local.hcl").replace("user               = \"hune\"", "user_env           = \"CH_USER\"");
    let config = s.write("env.hcl", &config);
    let args = ["print-config", "-c", config.to_str().unwrap()];
    // The variable is not set.
    let out = hune(&args, &[]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("clickhouse \"prod\".user: the environment variable CH_USER is not set (line "));
    // The variable is set.
    let out = hune(&args, &[("CH_USER", "reader")]);
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).contains("clickhouse \"prod\".user = \"reader\"  # env:CH_USER\n"));
}

#[test]
fn k14_variables_that_the_file_does_not_name_change_nothing() {
    let s = Setup::new();
    let config = s.config("distributed.hcl");
    let args = ["print-config", "-c", config.to_str().unwrap()];
    let clean = hune(&args, &[]);
    assert_eq!(clean.status.code(), Some(0));
    let bad = [
        ("RUST_LOG", "trace"),
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.2:4318"),
        ("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT", "http://127.0.0.2:4318"),
        ("OTEL_SDK_DISABLED", "true"),
        ("OTEL_SERVICE_NAME", "wrong"),
        ("OTEL_RESOURCE_ATTRIBUTES", "service.name=wrong"),
        ("OTEL_TRACES_SAMPLER", "always_off"),
        ("HTTP_PROXY", "http://127.0.0.3:3128"),
        ("HTTPS_PROXY", "http://127.0.0.3:3128"),
        ("ALL_PROXY", "http://127.0.0.3:3128"),
        ("NO_PROXY", "*"),
        ("http_proxy", "http://127.0.0.3:3128"),
        ("https_proxy", "http://127.0.0.3:3128"),
        ("SSL_CERT_FILE", "/nonexistent"),
        ("SSL_CERT_DIR", "/nonexistent"),
        ("PGHOST", "127.0.0.4"),
        ("PGPASSWORD", "wrong"),
        ("PGSSLMODE", "disable"),
        ("MONGODB_URI", "mongodb://127.0.0.4:27017/?tls=false&authMechanism=PLAIN"),
        ("MONGODB_HOST", "127.0.0.4"),
        ("MONGODB_USER", "wrong"),
        ("MONGODB_PASSWORD", "wrong"),
        ("MONGODB_TLS", "false"),
        ("MONGODB_AUTH_MECHANISM", "PLAIN"),
        ("MONGODB_READ_PREFERENCE", "primary"),
        ("MONGOSH_HOME", "/nonexistent"),
        ("TZ", "Pacific/Kiritimati"),
        ("HUNE_LOG_LEVEL", "debug"),
        ("HUNE_LISTEN", "127.0.0.1:1"),
    ];
    let with_bad = hune(&args, &bad);
    assert_eq!(with_bad.status.code(), Some(0));
    assert_eq!(with_bad.stdout, clean.stdout, "print-config is byte-equal");
    assert_eq!(with_bad.stderr, clean.stderr);
}

#[test]
fn print_reference_gives_the_two_files_and_the_document() {
    let local = hune(&["print-reference", "--format", "hcl", "--profile", "local"], &[]);
    assert_eq!(local.status.code(), Some(0));
    assert_eq!(text(&local.stdout), fixture("local.hcl"));
    assert_eq!(text(&local.stderr), "");
    let distributed = hune(&["print-reference", "--profile", "distributed", "--format", "hcl"], &[]);
    assert_eq!(text(&distributed.stdout), fixture("distributed.hcl"));
    // K13: the committed document equals the output.
    let doc = hune(&["print-reference", "--format", "markdown"], &[]);
    assert_eq!(text(&doc.stdout), std::fs::read_to_string(REFERENCE_DOC).unwrap());
}

#[test]
fn print_reference_output_passes_check_config() {
    let s = Setup::new();
    for profile in ["local", "distributed"] {
        let out = hune(&["print-reference", "--format", "hcl", "--profile", profile], &[]);
        let mut generated = text(&out.stdout);
        for (original, name, _) in FILES {
            generated = generated.replace(original, &s.path(name));
        }
        let path = s.write("generated.hcl", &generated);
        let check = hune(&["check-config", "-c", path.to_str().unwrap()], &[]);
        assert_eq!(check.status.code(), Some(0), "{profile}: {}", text(&check.stderr));
    }
}

#[test]
fn command_line_errors_exit_with_2() {
    for args in [
        &[][..],
        &["check-config"][..],
        &["print-config"][..],
        &["check-config", "--config"][..],
        &["check-config", "--bogus", "x"][..],
        &["print-reference"][..],
        &["print-reference", "--format", "hcl"][..],
        &["print-reference", "--format", "xml", "--profile", "local"][..],
        &["no-such-command"][..],
    ] {
        let out = hune(args, &[]);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&out.stdout), "", "{args:?}");
        assert!(text(&out.stderr).starts_with("hune: "), "{args:?}");
    }
}

#[test]
fn version_prints_the_version_and_reads_no_file() {
    let out = hune(&["version"], &[]);
    assert_eq!(out.status.code(), Some(0));
    let line = text(&out.stdout);
    assert!(line.starts_with("hune 0.3.1 "), "{line}");
    assert_eq!(line.lines().count(), 1);
}
