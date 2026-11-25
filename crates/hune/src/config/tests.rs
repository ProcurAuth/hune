// SPDX-License-Identifier: MIT OR Apache-2.0
//! Unit tests of the configuration: the nine rules of 13.2, the error forms of 13.14, the
//! contradictions C1 to C13, the warnings W1 to W6, the stage 2 checks and the outputs.

use std::collections::HashMap;

use hclconf::{Kind, Value};
use tempfile::TempDir;

use super::keys::SCHEMA;
use super::rules;
use super::stage2::Stage2;
use super::url::parse_http_url;
use super::*;

const LOCAL: &str = include_str!("../../../../tests/fixtures/config/local.hcl");
const DISTRIBUTED: &str = include_str!("../../../../tests/fixtures/config/distributed.hcl");
const LOCAL_PRINT: &str = include_str!("../../../../tests/fixtures/config/local.print-config.txt");
const CA_A: &str = include_str!("../../../../tests/fixtures/config/ca-a.pem");
const CA_B: &str = include_str!("../../../../tests/fixtures/config/ca-b.pem");
const JWKS: &str = include_str!("../../../../tests/fixtures/config/jwks.json");
const ED_PEM: &str = include_str!("../../../../tests/fixtures/config/ed25519.pub.pem");
const REFERENCE_DOC: &str = include_str!("../../../../docs/configuration-reference.md");

/// The local file with its volume as a read-only store: no primary store, a title, no history.
fn stores_read_only_main(text: &str) -> String {
    let t = replace_once(text, "    primary         = true\n", "    title           = \"Main\"\n");
    replace_once(&t, "    history_keep    = 100\n", "")
}

/// The distributed file with its PostgreSQL store as a read-only store.
fn stores_read_only_postgres(text: &str) -> String {
    let t = replace_once(text, "    primary           = true\n", "    title             = \"Archive\"\n");
    replace_once(&t, "    history_keep      = 100\n", "")
}

/// The secret files that the example files name, and the test files that replace them.
const FILES: [(&str, &str, &str); 14] = [
    ("/run/secrets/clickhouse/password", "ch-pw", "chpass\n"),
    ("/run/secrets/clickhouse/ca.pem", "ch-ca.pem", CA_A),
    ("/run/secrets/pg/username", "pg-user", "app\n"),
    ("/run/secrets/pg/password", "pg-pw", "pgpass\r\n"),
    ("/run/secrets/pg-ca/ca.crt", "pg-ca.crt", CA_B),
    ("/run/secrets/edge/jwks.json", "jwks.json", JWKS),
    ("/run/secrets/pdp/token", "pdp-token", "pdptoken\n"),
    ("/run/secrets/pdp/ca.pem", "pdp-ca.pem", CA_A),
    ("/run/secrets/postgres-metrics/username", "pgm-user", "hune_ro\n"),
    ("/run/secrets/postgres-metrics/password", "pgm-pw", "pgmpass\n"),
    ("/run/secrets/postgres-metrics-ca/ca.crt", "pgm-ca.crt", CA_A),
    ("/run/secrets/mongodb-events/username", "mge-user", "hune_ro\n"),
    ("/run/secrets/mongodb-events/password", "mge-pw", "mgepass\n"),
    ("/run/secrets/mongodb-events-ca/ca.crt", "mge-ca.crt", CA_A),
];

struct Fx {
    dir: TempDir,
}

impl Fx {
    fn new() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        for (_, name, content) in FILES {
            crate::config::test_support::write_file(dir.path().join(name), content).unwrap();
        }
        Fx { dir }
    }

    fn path(&self, name: &str) -> String {
        self.dir.path().join(name).display().to_string()
    }

    /// Replace the paths of the example files by the paths of the test files.
    fn substitute(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (original, name, _) in FILES {
            out = out.replace(original, &self.path(name));
        }
        out
    }

    /// The reverse of `substitute`, for the outputs.
    fn back(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (original, name, _) in FILES {
            out = out.replace(&self.path(name), original);
        }
        out
    }

    fn local(&self) -> String {
        self.substitute(LOCAL)
    }

    fn distributed(&self) -> String {
        self.substitute(DISTRIBUTED)
    }

    fn load(&self, text: &str) -> Result<LoadedConfig, Vec<String>> {
        self.load_env(text, &[])
    }

    fn load_env(&self, text: &str, env: &[(&str, &str)]) -> Result<LoadedConfig, Vec<String>> {
        let file = self.dir.path().join("hune.hcl");
        crate::config::test_support::write_file(&file, text).unwrap();
        let env = CapturedEnvironment::from_pairs(env.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        load(&file, &env).map_err(|list| list.iter().map(|e| self.back(&e.to_string())).collect())
    }

    fn errors(&self, text: &str) -> Vec<String> {
        match self.load(text) {
            Ok(_) => Vec::new(),
            Err(e) => e,
        }
    }

    fn one(&self, text: &str) -> String {
        let e = self.errors(text);
        assert_eq!(e.len(), 1, "{e:#?}");
        e[0].clone()
    }
}

/// The line number of the first line that holds `needle`, from 1.
fn line_of(text: &str, needle: &str) -> usize {
    text.lines().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("{needle}")) + 1
}

fn replace_once(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "{from:?} must occur once");
    text.replace(from, to)
}

/// Find the line of an attribute `attr` in the block `block_path` (for example
/// `storage.postgres.tls`; a labelled block counts by its type).
fn locate(text: &str, block_path: &str, attr: &str) -> Option<usize> {
    let mut stack: Vec<String> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim();
        if t.ends_with('{') {
            stack.push(t.split_whitespace().next().unwrap_or("").to_string());
        } else if t == "}" {
            stack.pop();
        } else if stack.join(".") == block_path
            && let Some((name, _)) = t.split_once('=')
            && name.trim() == attr
        {
            return Some(i);
        }
    }
    None
}

/// Set an attribute: `key` is the block path and the attribute name as written.
fn set(text: &str, key: &str, rhs: &str) -> String {
    let (block, attr) = key.rsplit_once('.').unwrap();
    let at = locate(text, block, attr)
        .or_else(|| locate(text, block, &format!("{attr}_file")))
        .or_else(|| locate(text, block, &format!("{attr}_env")))
        .unwrap_or_else(|| panic!("{key}"));
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let indent: String = lines[at].chars().take_while(|c| *c == ' ').collect();
    lines[at] = format!("{indent}{attr} = {rhs}");
    lines.join("\n") + "\n"
}

/// Give the attribute of a key another name, for example `password` to `password_env`.
fn rename(text: &str, key: &str, new_name: &str) -> String {
    let (block, attr) = key.rsplit_once('.').unwrap();
    let at = locate(text, block, attr).unwrap_or_else(|| panic!("{key}"));
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let (_, rhs) = lines[at].split_once('=').unwrap();
    let indent: String = lines[at].chars().take_while(|c| *c == ' ').collect();
    lines[at] = format!("{indent}{new_name} ={rhs}");
    lines.join("\n") + "\n"
}

/// Remove the block that starts with the line `start` (for example `  tls {`) and its
/// closing line.
fn remove_block(text: &str, start: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines.iter().position(|l| *l == start).unwrap_or_else(|| panic!("{start}"));
    let indent = start.len() - start.trim_start().len();
    let close = format!("{}}}", " ".repeat(indent));
    let last = first + lines[first..].iter().position(|l| *l == close).unwrap();
    let mut out: Vec<&str> = lines[..first].to_vec();
    // Also remove the empty line before the block.
    if out.last() == Some(&"") {
        out.pop();
    }
    out.extend(&lines[last + 1..]);
    out.join("\n") + "\n"
}

fn remove(text: &str, key: &str) -> String {
    let (block, attr) = key.rsplit_once('.').unwrap();
    let at = locate(text, block, attr).unwrap_or_else(|| panic!("{key}"));
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines.remove(at);
    lines.join("\n") + "\n"
}

fn quoted(s: &str) -> String {
    format!("\"{s}\"")
}

/// Add a block at the end of the file.
fn append(text: &str, block: &str) -> String {
    format!("{text}\n{block}\n")
}

fn remove_tls_block_of_clickhouse(text: &str) -> String {
    remove_block(text, "  tls {")
}

// ---------------------------------------------------------------------------------------
// The example files, the outputs and the generated documents.
// ---------------------------------------------------------------------------------------

#[test]
fn local_file_loads_with_the_warning_w1() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    assert_eq!(loaded.warnings.len(), 1);
    assert_eq!(loaded.warnings[0].key, "access.mode");
    assert_eq!(
        loaded.warnings[0].message,
        "access.mode is open: Hune checks no identity and asks no PDP. A proxy that authenticates the users must guard each route of server.listen."
    );
}

#[test]
fn distributed_file_loads_with_the_warning_w2() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.distributed()).unwrap();
    assert_eq!(loaded.warnings.len(), 1);
    assert_eq!(loaded.warnings[0].key, "server.listen");
    assert_eq!(
        loaded.warnings[0].message,
        "server.listen has a wildcard host: make sure that only the edge reaches this port (ProcurAuth 9.4)."
    );
}

#[test]
fn print_reference_gives_the_files_of_13_16_and_13_17_byte_for_byte() {
    assert_eq!(render_reference(ReferenceFormat::Hcl, Profile::Local), LOCAL);
    assert_eq!(render_reference(ReferenceFormat::Hcl, Profile::Distributed), DISTRIBUTED);
}

#[test]
fn k15_the_reference_files_pass_the_checks() {
    let fx = Fx::new();
    for profile in [Profile::Local, Profile::Distributed] {
        let text = fx.substitute(&render_reference(ReferenceFormat::Hcl, profile));
        assert_eq!(fx.errors(&text), Vec::<String>::new(), "{profile:?}");
    }
}

#[test]
fn k13_the_reference_document_equals_the_output() {
    let generated = render_reference(ReferenceFormat::Markdown, Profile::Local);
    assert_eq!(generated, REFERENCE_DOC);
    assert_eq!(generated, render_reference(ReferenceFormat::Markdown, Profile::Distributed));
}

#[test]
fn the_reference_document_has_a_row_for_each_key() {
    for key in SCHEMA.keys {
        let shown = if key.path.starts_with("storage.volume.") {
            key.path.replacen("storage.volume", "storage.volume \"<id>\"", 1)
        } else if key.path.starts_with("clickhouse.") {
            key.path.replacen("clickhouse", "clickhouse \"<id>\"", 1)
        } else if key.path.starts_with("postgres.") {
            key.path.replacen("postgres", "postgres \"<id>\"", 1)
        } else if key.path.starts_with("mongodb.") {
            key.path.replacen("mongodb", "mongodb \"<id>\"", 1)
        } else {
            key.path.to_string()
        };
        assert!(REFERENCE_DOC.contains(&format!("| `{shown}` |")), "{shown}");
        assert!(REFERENCE_DOC.contains(key.description.split('|').next().unwrap()), "{shown}");
    }
}

#[test]
fn print_config_of_the_local_file_is_the_output_of_13_20() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    assert_eq!(fx.back(&render_print_config(&loaded)), LOCAL_PRINT);
}

#[test]
fn print_config_of_the_distributed_file_shows_the_other_sources() {
    let fx = Fx::new();
    let out = fx.back(&render_print_config(&fx.load(&fx.distributed()).unwrap()));
    for line in [
        "storage.postgres.user = \"app\"  # file:/run/secrets/pg/username",
        "storage.postgres.password = <hidden>  # file:/run/secrets/pg/password",
        "access.pdp.token = <hidden>  # file:/run/secrets/pdp/token",
        "access.identity.public_keys = [\"Ed25519 kid=edge-2026-11\", \"ES256 kid=edge-2026-05\"]  # file:/run/secrets/edge/jwks.json",
        "access.identity.max_age = \"5m\"  # file",
        "access.pdp.timeout = \"500ms\"  # file",
        "clickhouse \"prod\".max_concurrent_per_subject = 6  # file",
        "storage.postgres.tls.server_name = \"hune-pg-rw\"  # file",
    ] {
        assert!(out.lines().any(|l| l == line), "{line}\n{out}");
    }
    assert!(!out.contains("pgpass") && !out.contains("pdptoken") && !out.contains("chpass"));
}

#[test]
fn print_config_lists_keys_in_the_order_of_the_key_table() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.distributed()).unwrap();
    let position = |p: &str| SCHEMA.keys.iter().position(|k| k.path == p).unwrap();
    let mut last = 0;
    for entry in &loaded.effective {
        let path = entry.key.clone();
        let schema_path = if path.starts_with("clickhouse \"prod\"") {
            path.replacen("clickhouse \"prod\"", "clickhouse", 1)
        } else if path.starts_with("storage.volume \"main\"") {
            path.replacen("storage.volume \"main\"", "storage.volume", 1)
        } else {
            path
        };
        let at = position(&schema_path);
        assert!(at >= last, "{}", entry.key);
        last = at;
    }
}

#[test]
fn the_typed_config_holds_the_values_of_the_local_file() {
    let fx = Fx::new();
    let c = fx.load(&fx.local()).unwrap().config;
    assert_eq!(c.server.listen.to_string(), "0.0.0.0:8080");
    assert_eq!(c.server.ops_listen.port(), 9090);
    assert_eq!(c.server.base_path, "/");
    assert!(c.storage.editing());
    assert_eq!(c.server.max_connections, 1024);
    assert_eq!(c.server.idle_timeout, Duration::from_secs(75));
    assert_eq!(c.server.shutdown_grace, Duration::from_secs(30));
    assert_eq!(c.server.max_request_bytes, 8_388_608);
    assert_eq!(c.server.max_result_memory_bytes, 536_870_912);
    assert_eq!(c.log.level, LogLevel::Info);
    assert!(matches!(c.telemetry, TelemetryConfig::Disabled));
    assert!(matches!(c.access, AccessConfig::Open));
    assert!(c.storage.postgres.is_none());
    assert_eq!(c.storage.volumes.len(), 1);
    let v = &c.storage.volumes[0];
    assert_eq!((v.id.as_str(), v.primary, v.title.as_deref()), ("main", true, None));
    assert_eq!(v.path, std::path::PathBuf::from("/var/lib/hune/dashboards"));
    assert_eq!(v.rescan_interval, Duration::from_secs(5));
    assert_eq!(v.history_keep, Some(100));
    assert_eq!(c.clickhouse.len(), 1);
    let ch = &c.clickhouse[0];
    assert_eq!((ch.id.as_str(), ch.label.as_str()), ("prod", "Production"));
    assert_eq!((ch.url.https, ch.url.host.as_str(), ch.url.port), (true, "clickhouse", 8443));
    assert_eq!(ch.url.text, "https://clickhouse:8443");
    assert_eq!(ch.user, "hune");
    assert_eq!(ch.password.expose(), "chpass");
    assert_eq!(ch.database, "analytics");
    let tls = ch.tls.as_ref().unwrap();
    assert_eq!(tls.server_name, "clickhouse");
    assert_eq!(tls.ca_pem, CA_A.as_bytes());
    assert_eq!(ch.connect_timeout, Duration::from_secs(5));
    assert_eq!(ch.max_execution_time, Duration::from_secs(60));
    assert_eq!(ch.max_result_rows, 100_000);
    assert_eq!(ch.max_result_bytes, 33_554_432);
    assert_eq!(ch.max_query_size, 1_048_576);
    assert_eq!(ch.max_concurrent, 16);
    assert_eq!(ch.queue_wait, Duration::from_secs(5));
    assert_eq!(ch.max_concurrent_per_subject, None);
}

#[test]
fn the_typed_config_holds_the_values_of_the_distributed_file() {
    let fx = Fx::new();
    let c = fx.load(&fx.distributed()).unwrap().config;
    assert!(c.storage.volumes.is_empty());
    let Some(p) = &c.storage.postgres else { panic!("postgres expected") };
    assert!(p.primary && p.title.is_none());
    assert_eq!((p.host.as_str(), p.port, p.database.as_str(), p.user.as_str()), ("hune-pg-rw", 5432, "hune", "app"));
    assert_eq!(p.password.expose(), "pgpass", "one line end \\r\\n is removed");
    assert_eq!(p.sslmode, SslMode::VerifyFull);
    assert_eq!(p.tls.as_ref().unwrap().ca_pem, CA_B.as_bytes());
    assert_eq!(p.tls.as_ref().unwrap().ca_file, std::path::PathBuf::from(fx.path("pg-ca.crt")));
    assert_eq!((p.max_connections, p.history_keep), (8, Some(100)));
    assert_eq!(p.statement_timeout, Duration::from_secs(10));
    assert_eq!(p.poll_interval, Duration::from_secs(5));
    let AccessConfig::Authzen(a) = &c.access else { panic!("authzen expected") };
    assert_eq!(a.identity.header, "X-Identity-Token");
    assert_eq!(a.identity.issuer, "https://edge.example");
    assert_eq!(a.identity.audience, "dashboards");
    assert_eq!(a.identity.public_keys, JWKS.strip_suffix('\n').unwrap_or(JWKS));
    assert_eq!(a.identity.max_age, Duration::from_secs(300));
    assert_eq!(a.identity.clock_skew, Duration::from_secs(60));
    assert_eq!(a.pdp.url, "https://pdp.example:4182");
    assert_eq!(a.pdp.token.expose(), "pdptoken");
    assert!(!a.pdp.plain_http);
    assert_eq!(a.pdp.ca_pem.as_deref(), Some(CA_A.as_bytes()));
    assert_eq!(a.pdp.timeout, Duration::from_millis(500));
    assert_eq!(a.pdp.cache_ttl, Duration::from_secs(30));
    assert_eq!(a.pdp.max_connections, 4);
    assert_eq!(c.clickhouse[0].max_concurrent_per_subject, Some(6));
}

#[test]
fn secrets_never_show_in_debug_output() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.distributed()).unwrap();
    let text = format!("{:?}", loaded);
    for secret in ["chpass", "pgpass", "pdptoken"] {
        assert!(!text.contains(secret), "{secret}");
    }
    assert!(text.contains("(value hidden)"));
}

#[test]
fn start_log_objects_follow_13_22() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&loaded)).unwrap();
    assert_eq!(config["server.listen"], "0.0.0.0:8080");
    assert_eq!(config["server.max_connections"], 1024);
    assert_eq!(config["storage.volume \"main\".primary"], true);
    assert_eq!(config["server.idle_timeout"], "75s");
    assert_eq!(config["clickhouse \"prod\".password"], "<hidden>");
    assert_eq!(config.as_object().unwrap().len(), loaded.effective.len());
    let sources = fx.back(&start_log_sources_json(&loaded));
    assert_eq!(
        sources,
        r#"{"server.query_inspect":"default","server.repeat_max_per_panel":"default","server.repeat_max_per_section":"default","server.repeat_max_per_dashboard":"default","server.compress_static":"default","server.transform_max_cells":"default","server.list_filters_min_dashboards":"default","clickhouse \"prod\".password":"file:/run/secrets/clickhouse/password"}"#
    );
    // A list is a JSON array.
    let dist = fx.load(&fx.distributed()).unwrap();
    let config: serde_json::Value = serde_json::from_str(&start_log_config_json(&dist)).unwrap();
    assert_eq!(config["access.identity.public_keys"][1], "ES256 kid=edge-2026-05");
}

// ---------------------------------------------------------------------------------------
// K01: the range of each key.
// ---------------------------------------------------------------------------------------

fn base_text_for(fx: &Fx, path: &str) -> String {
    let distributed = path.starts_with("storage.postgres")
        || path.starts_with("access.identity")
        || path.starts_with("access.pdp")
        || path == "clickhouse.max_concurrent_per_subject"
        || path == "postgres.max_concurrent_per_subject"
        || path == "mongodb.max_concurrent_per_subject";
    let mut text = if distributed { fx.distributed() } else { fx.local() };
    // A title is only in a read-only store: the reference files have none.
    if path == "storage.volume.title" {
        text = stores_read_only_main(&text);
    }
    if path == "storage.postgres.title" {
        text = stores_read_only_postgres(&text);
    }
    if path.starts_with("postgres.") {
        let authzen = path == "postgres.max_concurrent_per_subject";
        text = append(&if authzen { fx.distributed() } else { fx.local() }, &fx.substitute(&postgres::block(authzen)));
    }
    if path.starts_with("mongodb.") {
        let authzen = path == "mongodb.max_concurrent_per_subject";
        let replica_set = matches!(path, "mongodb.hosts" | "mongodb.replica_set");
        text = append(
            &if authzen { fx.distributed() } else { fx.local() },
            &fx.substitute(&mongodb::block(replica_set, authzen)),
        );
    }
    // The reference files do not set the optional keys that have a default.
    if let Some(attr) = path.strip_prefix("server.").filter(|a| {
        matches!(*a, "query_inspect" | "compress_static" | "transform_max_cells" | "list_filters_min_dashboards")
            || a.starts_with("repeat_max_per_")
    }) {
        let initial = match attr {
            "query_inspect" => "false",
            "compress_static" => "true",
            _ => "1",
        };
        text = replace_once(
            &text,
            "  base_path               = \"/\"\n",
            &format!("  base_path               = \"/\"\n  {attr:<23} = {initial}\n"),
        );
    }
    if path.starts_with("telemetry.") {
        text = replace_once(
            &text,
            "telemetry {\n  enabled = false\n}",
            "telemetry {\n  enabled = true\n  otlp_endpoint = \"http://otel-collector:4318\"\n  service_name = \"hune\"\n  export_interval = \"15s\"\n}",
        );
    }
    // The block `geomap` (13.12c) is optional, and the reference files hold none.
    if path.starts_with("geomap.") {
        text = append(
            &text,
            "geomap {\n  tile_url    = \"https://tiles.example.org/osm/{z}/{x}/{y}.png\"\n  attribution = \"Map tiles: example.org\"\n  max_zoom    = 19\n}",
        );
    }
    text
}

fn stage1(text: &str) -> Vec<String> {
    match hclconf::check_text(text, &SCHEMA, &CapturedEnvironment::default()) {
        Ok(_) => Vec::new(),
        Err(e) => e.iter().map(ToString::to_string).collect(),
    }
}

#[test]
fn k01_each_numeric_key_accepts_its_minimum_and_maximum_and_refuses_the_values_outside() {
    let fx = Fx::new();
    let mut checked_keys = 0;
    for key in SCHEMA.keys {
        let (good, bad): (Vec<String>, Vec<String>) = match key.kind {
            Kind::Integer { min, max } | Kind::Size { min, max } => {
                let mut bad = vec![(max + 1).to_string()];
                if min > 0 {
                    bad.push((min - 1).to_string());
                }
                (vec![min.to_string(), max.to_string()], bad)
            }
            Kind::Duration { min_ms, max_ms } => {
                let mut bad = vec![quoted(&format!("{}ms", max_ms + 1))];
                if min_ms > 0 {
                    bad.push(quoted(&format!("{}ms", min_ms - 1)));
                }
                (
                    vec![
                        quoted(&hclconf::value::format_duration(min_ms)),
                        quoted(&hclconf::value::format_duration(max_ms)),
                    ],
                    bad,
                )
            }
            _ => continue,
        };
        checked_keys += 1;
        let base = base_text_for(&fx, key.path);
        assert_eq!(stage1(&base), Vec::<String>::new(), "{}", key.path);
        let attr_key = key.path.to_string();
        for value in good {
            let text = set(&base, &attr_key, &value);
            assert_eq!(stage1(&text), Vec::<String>::new(), "{} = {value}", key.path);
        }
        for value in bad {
            let text = set(&base, &attr_key, &value);
            let errors = stage1(&text);
            assert_eq!(errors.len(), 1, "{} = {value}: {errors:?}", key.path);
            let shown = if key.path.starts_with("storage.volume.") {
                key.path.replacen("storage.volume", "storage.volume \"main\"", 1)
            } else if key.path.starts_with("clickhouse.") {
                key.path.replacen("clickhouse", "clickhouse \"prod\"", 1)
            } else if key.path.starts_with("postgres.") {
                key.path.replacen("postgres", "postgres \"metrics\"", 1)
            } else if key.path.starts_with("mongodb.") {
                key.path.replacen("mongodb", "mongodb \"events\"", 1)
            } else {
                key.path.to_string()
            };
            let found = value.clone();
            let (block, attr) = key.path.rsplit_once('.').unwrap();
            let line = locate(&text, block, attr).unwrap() + 1;
            let expected = format!("{shown}: expected {}, found {found} (line {line})", key.kind.expected());
            assert_eq!(errors[0], expected);
        }
    }
    assert!(checked_keys >= 27, "{checked_keys}");
}

#[test]
fn k01_text_lengths_are_checked_for_each_key_with_a_length() {
    let fx = Fx::new();
    for key in SCHEMA.keys {
        let Kind::Text { min, max, rule } = key.kind else { continue };
        if key.forms == hclconf::Forms::Ef
            || !matches!(
                rule,
                hclconf::TextRule::Any
                    | hclconf::TextRule::NoControl
                    | hclconf::TextRule::NoControlNoSpace
                    | hclconf::TextRule::PrintableAscii
            )
        {
            continue;
        }
        let base = base_text_for(&fx, key.path);
        let at_max = set(&base, key.path, &quoted(&"a".repeat(max)));
        assert_eq!(stage1(&at_max), Vec::<String>::new(), "{} at max", key.path);
        let above = set(&base, key.path, &quoted(&"a".repeat(max + 1)));
        assert_eq!(stage1(&above).len(), 1, "{} above max", key.path);
        let at_min = set(&base, key.path, &quoted(&"a".repeat(min)));
        assert_eq!(stage1(&at_min), Vec::<String>::new(), "{} at min", key.path);
        if min > 0 {
            let below = set(&base, key.path, &quoted(&"a".repeat(min - 1)));
            assert_eq!(stage1(&below).len(), 1, "{} below min", key.path);
        }
    }
}

#[test]
fn k02_a_wrong_type_stops_the_start_with_no_coercion() {
    let fx = Fx::new();
    let local = fx.local();
    let cases: [(&str, &str, &str); 8] = [
        ("server.max_connections", "\"1024\"", "expected an integer from 16 to 65536, found \"1024\""),
        ("server.max_connections", "1024.0", "expected an integer from 16 to 65536, found 1024.0"),
        ("server.max_connections", "1e3", "expected an integer from 16 to 65536, found 1e3"),
        ("server.max_connections", "-1", "expected an integer from 16 to 65536, found -1"),
        ("server.max_connections", "01024", "expected an integer from 16 to 65536, found 01024"),
        ("telemetry.enabled", "\"true\"", "expected a boolean, found \"true\""),
        ("telemetry.enabled", "1", "expected a boolean, found 1"),
        ("server.max_request_bytes", "\"32MiB\"", "expected a size in bytes from 4259840 to 67108864, found \"32MiB\""),
    ];
    for (key, value, message) in cases {
        let text = set(&local, key, value);
        let line = line_of(&text, &format!("{} ", key.rsplit('.').next().unwrap()));
        assert_eq!(fx.one(&text), format!("{key}: {message} (line {line})"));
    }
}

#[test]
fn k09_a_duration_with_a_fraction_a_sign_two_units_or_no_unit_stops_the_start() {
    let fx = Fx::new();
    for bad in ["1.5s", "-1s", "+5s", "1m30s", "1m 30s", "30", "\"1d\"", "05s", "30 s", "5us", "5ns"] {
        let value = if bad.starts_with('"') { bad.to_string() } else { quoted(bad) };
        let errors = fx.errors(&set(&fx.local(), "server.idle_timeout", &value));
        assert_eq!(errors.len(), 1, "{bad}");
        assert!(
            errors[0].starts_with("server.idle_timeout: expected a duration from 1s to 10m, found "),
            "{}",
            errors[0]
        );
    }
    // A number with no quotes is refused too.
    let e = fx.one(&set(&fx.local(), "server.idle_timeout", "30"));
    assert!(e.contains("found 30 (line"), "{e}");
}

#[test]
fn whole_second_durations_refuse_the_milliseconds() {
    let fx = Fx::new();
    let line = |t: &str, k: &str| line_of(t, k);
    // max_age and clock_skew (13.4)
    let text = set(&fx.distributed(), "access.identity.max_age", "\"1500ms\"");
    assert_eq!(
        fx.one(&text),
        format!(
            "access.identity.max_age: expected a whole number of seconds, found \"1500ms\" (line {})",
            line(&text, "max_age")
        )
    );
    assert!(fx.errors(&set(&fx.distributed(), "access.identity.max_age", "\"2000ms\"")).is_empty());
    let text = set(&fx.distributed(), "access.identity.clock_skew", "\"1500ms\"");
    assert!(fx.one(&text).contains("access.identity.clock_skew: expected a whole number of seconds"));
    let text = set(&fx.local(), "clickhouse.max_execution_time", "\"1500ms\"");
    assert_eq!(
        fx.one(&text),
        format!(
            "clickhouse \"prod\".max_execution_time: expected a whole number of seconds, found \"1500ms\" (line {})",
            line(&text, "max_execution_time")
        )
    );
    assert!(fx.errors(&set(&fx.local(), "clickhouse.max_execution_time", "\"2000ms\"")).is_empty());
    // Other durations accept the milliseconds.
    assert!(fx.errors(&set(&fx.local(), "clickhouse.connect_timeout", "\"1500ms\"")).is_empty());
}

#[test]
fn durations_of_the_file_show_in_canonical_form() {
    let fx = Fx::new();
    let text = set(&set(&fx.local(), "server.idle_timeout", "\"60s\""), "clickhouse.connect_timeout", "\"1500ms\"");
    let out = render_print_config(&fx.load(&text).unwrap());
    assert!(out.contains("server.idle_timeout = \"1m\"  # file\n"));
    assert!(out.contains("clickhouse \"prod\".connect_timeout = \"1500ms\"  # file\n"));
}

// ---------------------------------------------------------------------------------------
// K03 to K08: the closed schema, the forms, the secrets, the literals.
// ---------------------------------------------------------------------------------------

#[test]
fn k03_unknown_and_duplicate_things_stop_the_start() {
    let fx = Fx::new();
    let local = fx.local();
    let text = replace_once(
        &local,
        "  base_path               = \"/\"\n",
        "  base_path               = \"/\"\n  colour = \"red\"\n",
    );
    assert_eq!(fx.one(&text), format!("server.colour: unknown attribute (line {})", line_of(&text, "colour")));
    let text = append(&local, "extra {\n}\n");
    assert_eq!(fx.one(&text), format!("extra: unknown block (line {})", line_of(&text, "extra {")));
    let text = replace_once(&local, "log {\n  level = \"info\"\n}", "log {\n  level = \"info\"\n  level = \"warn\"\n}");
    assert_eq!(
        fx.one(&text),
        format!(
            "log.level: duplicate attribute; the first one is at line {} (line {})",
            line_of(&text, "level"),
            line_of(&text, "level") + 1
        )
    );
    let text = append(&local, "log {\n  level = \"warn\"\n}\n");
    let second = text.lines().collect::<Vec<_>>().iter().rposition(|l| *l == "log {").unwrap() + 1;
    assert_eq!(
        fx.one(&text),
        format!("log: duplicate block; the first one is at line {} (line {second})", line_of(&local, "log {"))
    );
    let dup = append(
        &local,
        &local.split("clickhouse \"prod\" {").nth(1).map(|b| format!("clickhouse \"prod\" {{{b}")).unwrap(),
    );
    let e = fx.one(&dup);
    assert!(e.starts_with("clickhouse \"prod\": duplicate label; the first block with this label is at line "), "{e}");
}

#[test]
fn rule1_a_missing_block_and_a_misplaced_attribute() {
    let fx = Fx::new();
    let text = LOCAL.replace("log {\n  level = \"info\"\n}\n\n", "");
    assert_eq!(fx.one(&fx.substitute(&text)), "log: missing block");
    let e = fx.errors("listen = \"0.0.0.0:8080\"\n");
    assert!(e.contains(&"listen: unknown attribute (line 1)".to_string()), "{e:?}");
}

#[test]
fn syntax_error_gives_one_error() {
    let fx = Fx::new();
    let e = fx.errors(&format!("{}\nbad {{\n", fx.local()));
    assert_eq!(e.len(), 1);
    assert!(e[0].starts_with("the configuration file is not valid HCL at line "), "{}", e[0]);
}

#[test]
fn k04_the_three_forms_of_an_attribute() {
    let fx = Fx::new();
    let local = fx.local();
    // Form V.
    assert!(
        fx.errors(&replace_once(&local, "user               = \"hune\"", "user               = \"alice\"")).is_empty()
    );
    // Form E.
    let text = rename(&set(&local, "clickhouse.user", "\"CH_USER\""), "clickhouse.user", "user_env");
    let loaded = fx.load_env(&text, &[("CH_USER", "alice")]).unwrap();
    assert_eq!(loaded.config.clickhouse[0].user, "alice");
    let out = render_print_config(&loaded);
    assert!(out.contains("clickhouse \"prod\".user = \"alice\"  # env:CH_USER\n"), "{out}");
    let sources: serde_json::Value = serde_json::from_str(&start_log_sources_json(&loaded)).unwrap();
    assert_eq!(sources["clickhouse \"prod\".user"], "env:CH_USER");
    // Form F.
    crate::config::test_support::write_file(fx.dir.path().join("user-file"), "bob\n").unwrap();
    let text = rename(&set(&local, "clickhouse.user", &quoted(&fx.path("user-file"))), "clickhouse.user", "user_file");
    assert_eq!(fx.load(&text).unwrap().config.clickhouse[0].user, "bob");
}

#[test]
fn k04_two_forms_none_and_bad_sources_name_the_attribute() {
    let fx = Fx::new();
    let local = fx.local();
    let text = replace_once(
        &local,
        "user               = \"hune\"",
        "user               = \"hune\"\n  user_env = \"CH_USER\"",
    );
    let user_line = line_of(&text, "user               =");
    assert_eq!(
        fx.one(&text),
        format!(
            "clickhouse \"prod\".user: the file gives `user` at line {user_line} and `user_env`; give exactly one form (line {})",
            user_line + 1
        )
    );
    let text = remove(&local, "clickhouse.user");
    assert_eq!(
        fx.one(&text),
        format!(
            "clickhouse \"prod\".user: missing attribute; give `user`, `user_env` or `user_file` (a text of 1 to 256 bytes with no control character) (line {})",
            line_of(&text, "clickhouse \"prod\" {")
        )
    );
    let text = remove(&local, "clickhouse.password_file");
    assert_eq!(
        fx.one(&text).replace(&format!(" (line {})", line_of(&text, "clickhouse \"prod\" {")), ""),
        "clickhouse \"prod\".password: missing secret; give `password_env` or `password_file`"
    );
    // A variable that is not set, an empty variable, a value with edge spaces.
    let env_text = rename(&set(&local, "clickhouse.password", "\"CH_PW\""), "clickhouse.password", "password_env");
    let line = line_of(&env_text, "password_env");
    assert_eq!(
        fx.one(&env_text),
        format!("clickhouse \"prod\".password: the environment variable CH_PW is not set (line {line})")
    );
    let e = fx.load_env(&env_text, &[("CH_PW", "")]).unwrap_err();
    assert_eq!(
        e[0],
        format!("clickhouse \"prod\".password: the value of the environment variable CH_PW is empty (line {line})")
    );
    for bad in [" pw", "pw ", "pw\n", "\tpw"] {
        let e = fx.load_env(&env_text, &[("CH_PW", bad)]).unwrap_err();
        assert_eq!(
            e[0],
            format!(
                "clickhouse \"prod\".password: the value of the environment variable CH_PW starts or ends with a space or a line end (line {line})"
            )
        );
    }
    assert_eq!(fx.load_env(&env_text, &[("CH_PW", "pw")]).unwrap().config.clickhouse[0].password.expose(), "pw");
    // The names that a file cannot name.
    for name in ["SSL_CERT_FILE", "SSL_CERT_DIR"] {
        let t = env_text.replace("CH_PW", name);
        let e = fx.load_env(&t, &[(name, "x")]).unwrap_err();
        assert!(e[0].contains("other than SSL_CERT_FILE and SSL_CERT_DIR"), "{}", e[0]);
    }
    // A file that cannot be read, and an empty file.
    let text = set(&local, "clickhouse.password_file", "\"/nonexistent/pw\"");
    assert_eq!(
        fx.one(&text),
        format!(
            "clickhouse \"prod\".password: cannot read the file /nonexistent/pw: No such file or directory (line {})",
            line_of(&text, "password_file")
        )
    );
    crate::config::test_support::write_file(fx.dir.path().join("empty"), "").unwrap();
    let text = set(&local, "clickhouse.password_file", &quoted(&fx.path("empty")));
    assert!(
        fx.one(&text).contains("clickhouse \"prod\".password: the file /run") || fx.one(&text).contains("is empty")
    );
}

#[test]
fn k04_file_content_rules_with_the_real_keys() {
    let fx = Fx::new();
    let local = fx.local();
    let with_content = |content: &[u8]| {
        crate::config::test_support::write_file(fx.dir.path().join("pw"), content).unwrap();
        let text = set(&local, "clickhouse.password_file", &quoted(&fx.path("pw")));
        fx.load(&text).map(|l| l.config.clickhouse[0].password.expose().to_string())
    };
    assert_eq!(with_content(b"pw").unwrap(), "pw");
    assert_eq!(with_content(b"pw\n").unwrap(), "pw");
    assert_eq!(with_content(b"pw\r\n").unwrap(), "pw");
    assert_eq!(with_content(b"pw\n\n").unwrap(), "pw\n", "one last line end only");
    assert!(with_content(b"").unwrap_err()[0].ends_with("is empty (line 36)") || with_content(b"").is_err());
    assert!(with_content(b"\xef\xbb\xbfpw").unwrap_err()[0].contains("byte order mark"));
    assert!(with_content(b"\xff\xfe").unwrap_err()[0].contains("not UTF-8"));
    assert!(with_content(&vec![b'a'; 65_537]).unwrap_err()[0].contains("more than 65536 bytes"));
    assert!(
        with_content(&vec![b'a'; 1025]).unwrap_err()[0]
            .contains("expected a text of 1 to 1024 bytes, found (value hidden)")
    );
}

#[test]
fn k04_a_key_with_only_the_form_v_refuses_env_and_file() {
    let fx = Fx::new();
    let text = rename(&fx.local(), "server.listen", "listen_env");
    assert_eq!(
        fx.one(&text),
        format!(
            "server.listen: `listen` accepts only a value in the file, not `_env` or `_file` (line {})",
            line_of(&text, "listen_env")
        )
    );
    let text = rename(&fx.local(), "server.listen", "listen_file");
    assert!(fx.one(&text).contains("`listen` accepts only a value in the file"));
}

#[test]
fn k05_a_secret_in_clear_text_stops_the_start_and_never_shows() {
    let fx = Fx::new();
    let local = fx.local();
    let text = set(&local, "clickhouse.password", "\"hunter2\"");
    let e = fx.one(&text);
    assert_eq!(
        e,
        format!(
            "clickhouse \"prod\".password: a secret accepts only `password_env` or `password_file`, but the file gives it in clear text (value hidden) (line {})",
            line_of(&text, "password = ")
        )
    );
    assert!(!e.contains("hunter2"));
    // Also together with another form.
    let text = text.replace(
        "  password = \"hunter2\"",
        &format!("  password = \"hunter2\"\n  password_file = \"{}\"", fx.path("ch-pw")),
    );
    let e = fx.one(&text);
    assert!(e.contains("in clear text") && !e.contains("hunter2"), "{e}");
    // The other two secrets.
    let dist = fx.distributed();
    let text = set(&dist, "storage.postgres.password", "\"s3cret\"");
    assert!(fx.errors(&text).iter().any(|e| e.starts_with("storage.postgres.password: a secret accepts only")));
    let text = set(&dist, "access.pdp.token", "\"s3cret\"");
    assert!(fx.errors(&text).iter().any(|e| e.starts_with("access.pdp.token: a secret accepts only")));
    // A secret that is too long for its key is never shown.
    crate::config::test_support::write_file(fx.dir.path().join("long"), "z".repeat(1025)).unwrap();
    let text = set(&local, "clickhouse.password_file", &quoted(&fx.path("long")));
    assert!(!fx.one(&text).contains("zzzz"));
}

#[test]
fn k08_expressions_functions_templates_and_heredocs_stop_the_start() {
    let fx = Fx::new();
    let local = fx.local();
    let cases = [
        ("server.max_connections", "1 + 2", "found expression: 1 + 2; write a literal value"),
        ("server.max_connections", "max(1, 2)", "found function call: max(1, 2); write a literal value"),
        ("server.max_connections", "var.n", "found variable: var.n; write a literal value"),
        ("server.base_path", "\"${var.x}\"", "found template: \"${var.x}\"; write a literal value"),
        ("server.base_path", "\"%{ if x }a%{ endif }\"", "found template"),
        ("server.base_path", "null", "found null: null; write a literal value"),
        ("server.listen", "{ a = 1 }", "found object: { a = 1 }; write a literal value"),
        ("log.level", "<<EOT\ninfo\nEOT", "found heredoc: <<EOT; write a literal value"),
    ];
    for (key, value, found) in cases {
        let text = set(&local, key, value);
        let e = fx.one(&text);
        assert!(e.starts_with(&format!("{key}: expected ")), "{e}");
        assert!(e.contains(found), "{e}");
    }
}

#[test]
fn rule2_an_unquoted_text_with_operator_characters_is_a_syntax_error() {
    let fx = Fx::new();
    let e = fx.one(&set(&fx.local(), "clickhouse.url", "https://clickhouse:8443"));
    assert!(e.starts_with("the configuration file is not valid HCL at line "), "{e}");
}

// ---------------------------------------------------------------------------------------
// K06 and K07: the modes of the engine (C1 to C4, C6, C13) and the contradictions.
// ---------------------------------------------------------------------------------------

const IDENTITY_BLOCK: &str = "  identity {\n    header = \"X-Identity-Token\"\n  }\n";

#[test]
fn c1_the_blocks_identity_and_pdp_exist_only_in_the_mode_authzen() {
    let fx = Fx::new();
    let local = fx.local();
    let text = replace_once(&local, "  mode = \"open\"\n", &format!("  mode = \"open\"\n\n{IDENTITY_BLOCK}"));
    assert_eq!(
        fx.one(&text),
        format!(
            "access.identity: not permitted when `access.mode` is \"open\"; the key is only for `access.mode` = \"authzen\" (line {})",
            line_of(&text, "identity {")
        )
    );
    let text =
        replace_once(&local, "  mode = \"open\"\n", "  mode = \"open\"\n\n  pdp {\n    plain_http = false\n  }\n");
    assert!(fx.one(&text).starts_with("access.pdp: not permitted when `access.mode` is \"open\""));
    // K07: a necessary block that is absent.
    let dist = fx.distributed();
    let (before, rest) = dist.split_once("  identity {").unwrap();
    let (_, after) = rest.split_once("  pdp {").unwrap();
    let text = format!("{before}  pdp {{{after}");
    assert_eq!(
        fx.one(&text),
        format!(
            "access.identity: missing block; expected when `access.mode` is \"authzen\" (line {})",
            line_of(&text, "access {")
        )
    );
    // A necessary key of the block that is absent.
    let text = remove(&dist, "access.identity.issuer");
    assert_eq!(
        fx.one(&text),
        format!(
            "access.identity.issuer: missing attribute; expected a text of 1 to 1024 bytes with no control character (line {})",
            line_of(&text, "identity {")
        )
    );
}

#[test]
fn c3_the_keys_of_telemetry_exist_only_with_enabled_true() {
    let fx = Fx::new();
    let local = fx.local();
    let text = replace_once(
        &local,
        "  enabled = false\n",
        "  enabled = false\n  otlp_endpoint = \"http://otel-collector:4318\"\n  service_name = \"hune\"\n  export_interval = \"15s\"\n  ca_file = \"/x\"\n",
    );
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 4, "{errors:?}");
    assert!(errors[0].starts_with(
        "telemetry.otlp_endpoint: not permitted when `telemetry.enabled` is false; the key is only for `telemetry.enabled` = true (line "
    ));
    assert!(errors[3].starts_with("telemetry.ca_file: not permitted when `telemetry.enabled` is false"));
    // enabled = true needs the three keys.
    let text = replace_once(&local, "  enabled = false\n", "  enabled = true\n");
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors[0].starts_with(
        "telemetry.otlp_endpoint: missing attribute; give `otlp_endpoint`, `otlp_endpoint_env` or `otlp_endpoint_file`"
    ));
    assert!(
        errors[1].starts_with(
            "telemetry.service_name: missing attribute; expected a text of 1 to 64 bytes of printable ASCII"
        )
    );
    assert!(errors[2].starts_with("telemetry.export_interval: missing attribute; expected a duration from 1s to 5m"));
}

#[test]
fn c4_the_block_tls_of_postgres_exists_only_with_verify_full() {
    let fx = Fx::new();
    let dist = fx.distributed();
    let text = replace_once(&dist, "sslmode           = \"verify-full\"", "sslmode           = \"disable\"");
    assert_eq!(
        fx.one(&text),
        format!(
            "storage.postgres.tls: not permitted when `storage.postgres.sslmode` is \"disable\"; the key is only for `storage.postgres.sslmode` = \"verify-full\" (line {})",
            line_of(&text, "    tls {")
        )
    );
    let (before, rest) = dist.split_once("\n    tls {").unwrap();
    let (_, after) = rest.split_once("    }\n").unwrap();
    let text = format!("{before}{after}");
    assert!(fx.one(&text).starts_with(
        "storage.postgres.tls: missing block; expected when `storage.postgres.sslmode` is \"verify-full\""
    ));
    // sslmode = disable with no block is valid, with the warning W5.
    let text = replace_once(
        &format!("{before}{after}"),
        "sslmode           = \"verify-full\"",
        "sslmode           = \"disable\"",
    );
    let loaded = fx.load(&text).unwrap();
    assert!(loaded.warnings.iter().any(|w| w.key == "storage.postgres.sslmode"));
}

#[test]
fn c5_plain_http_and_the_scheme_of_the_pdp_url() {
    let fx = Fx::new();
    let dist = fx.distributed();
    let text = set(&dist, "access.pdp.url", "\"http://pdp.example:4182\"");
    assert_eq!(
        fx.one(&text),
        "access.pdp.plain_http, access.pdp.url: `access.pdp.plain_http` (false) needs the scheme https, but `access.pdp.url` has the scheme \"http\""
    );
    let text = remove(&set(&dist, "access.pdp.plain_http", "true"), "access.pdp.ca_file");
    assert_eq!(
        fx.one(&text),
        "access.pdp.plain_http, access.pdp.url: `access.pdp.plain_http` (true) needs the scheme http, but `access.pdp.url` has the scheme \"https\""
    );
    // Both agree: plain_http true with an http URL and no CA file, with the warning W3.
    let text = remove(
        &set(&set(&dist, "access.pdp.plain_http", "true"), "access.pdp.url", "\"http://pdp.example:4182\""),
        "access.pdp.ca_file",
    );
    let loaded = fx.load(&text).unwrap();
    assert!(loaded.warnings.iter().any(|w| w.key == "access.pdp.plain_http"
        && w.message == "access.pdp.plain_http is true: the PDP token and the questions go without TLS."));
    let AccessConfig::Authzen(a) = loaded.config.access else { panic!() };
    assert!(a.pdp.plain_http && a.pdp.ca_file.is_none() && a.pdp.ca_pem.is_none());
}

#[test]
fn c6_ca_file_exists_only_with_plain_http_false() {
    let fx = Fx::new();
    let dist = fx.distributed();
    let text = set(&set(&dist, "access.pdp.plain_http", "true"), "access.pdp.url", "\"http://pdp.example:4182\"");
    assert_eq!(
        fx.one(&text),
        format!(
            "access.pdp.ca_file: not permitted when `access.pdp.plain_http` is true; the key is only for `access.pdp.plain_http` = false (line {})",
            line_of(&text, "ca_file         =")
        )
    );
    let text = remove(&dist, "access.pdp.ca_file");
    assert_eq!(
        fx.one(&text),
        format!(
            "access.pdp.ca_file: missing attribute; expected an absolute path of 1 to 4096 bytes (line {})",
            line_of(&text, "pdp {")
        )
    );
}

#[test]
fn c7_an_https_url_needs_the_block_tls_and_an_http_url_forbids_it() {
    let fx = Fx::new();
    let local = fx.local();
    let text = remove_tls_block_of_clickhouse(&local);
    assert_eq!(
        fx.one(&text),
        "clickhouse \"prod\".url, clickhouse \"prod\".tls: `clickhouse \"prod\".url` (\"https://clickhouse:8443\") needs the block tls"
    );
    let text = set(&local, "clickhouse.url", "\"http://clickhouse:8123\"");
    assert_eq!(
        fx.one(&text),
        "clickhouse \"prod\".url, clickhouse \"prod\".tls: `clickhouse \"prod\".url` (\"http://clickhouse:8123\") has the scheme http and forbids the block tls"
    );
    // http without tls is valid, with the warning W4.
    let text = set(&remove_tls_block_of_clickhouse(&local), "clickhouse.url", "\"http://clickhouse:8123\"");
    let loaded = fx.load(&text).unwrap();
    assert!(loaded.warnings.iter().any(|w| w.key == "clickhouse \"prod\".url"
        && w.message == "clickhouse \"prod\".url has no TLS to a host that is not a loopback address."));
    assert!(loaded.config.clickhouse[0].tls.is_none());
}

#[test]
fn c8_the_scheme_of_the_otlp_endpoint_and_its_ca_file() {
    // Stage 2 refuses `telemetry.enabled = true` in this build, so the check runs on the
    // result of stage 1 here.
    let fx = Fx::new();
    let text = replace_once(
        &fx.local(),
        "telemetry {\n  enabled = false\n}",
        "telemetry {\n  enabled = true\n  otlp_endpoint = \"http://otel-collector:4318\"\n  ca_file = \"/run/ca.pem\"\n  service_name = \"hune\"\n  export_interval = \"15s\"\n}",
    );
    let checked = hclconf::check_text(&text, &SCHEMA, &CapturedEnvironment::default()).unwrap();
    let mut stage = Stage2::default();
    stage
        .urls
        .insert("telemetry.otlp_endpoint".to_string(), parse_http_url("http://otel-collector:4318", false).unwrap());
    let errors: Vec<String> = rules::contradictions(&checked, &stage).iter().map(ToString::to_string).collect();
    assert_eq!(
        errors,
        vec![
            "telemetry.otlp_endpoint, telemetry.ca_file: `telemetry.otlp_endpoint` (\"http://otel-collector:4318\") has the scheme http and forbids `telemetry.ca_file` (\"/run/ca.pem\")"
        ]
    );
    // An https endpoint with no ca_file.
    let text = text.replace("  ca_file = \"/run/ca.pem\"\n", "").replace("http://", "https://");
    let checked = hclconf::check_text(&text, &SCHEMA, &CapturedEnvironment::default()).unwrap();
    let mut stage = Stage2::default();
    stage
        .urls
        .insert("telemetry.otlp_endpoint".to_string(), parse_http_url("https://otel-collector:4318", false).unwrap());
    let errors: Vec<String> = rules::contradictions(&checked, &stage).iter().map(ToString::to_string).collect();
    assert_eq!(
        errors,
        vec![
            "telemetry.otlp_endpoint, telemetry.ca_file: `telemetry.otlp_endpoint` (\"https://otel-collector:4318\") has the scheme https and needs `telemetry.ca_file`"
        ]
    );
    // http without ca_file is valid, and W6 warns for a host that is not a loopback address.
    let text = text.replace("https://", "http://");
    let checked = hclconf::check_text(&text, &SCHEMA, &CapturedEnvironment::default()).unwrap();
    let mut stage = Stage2::default();
    stage
        .urls
        .insert("telemetry.otlp_endpoint".to_string(), parse_http_url("http://otel-collector:4318", false).unwrap());
    assert!(rules::contradictions(&checked, &stage).is_empty());
    let warnings = rules::warnings(&checked, &stage);
    assert!(warnings.contains(&Warning {
        key: "telemetry.otlp_endpoint".into(),
        message: "telemetry.otlp_endpoint has no TLS to a host that is not a loopback address.".into()
    }));
    // W6 stays silent for a loopback host.
    let mut stage = Stage2::default();
    stage.urls.insert("telemetry.otlp_endpoint".to_string(), parse_http_url("http://127.0.0.1:4318", false).unwrap());
    assert!(!rules::warnings(&checked, &stage).iter().any(|w| w.key == "telemetry.otlp_endpoint"));
}

#[test]
fn c9_the_two_listeners_use_different_ports_or_hosts_that_do_not_overlap() {
    let fx = Fx::new();
    let local = fx.local();
    let with = |listen: &str, ops: &str| {
        set(&set(&local, "server.listen", &quoted(listen)), "server.ops_listen", &quoted(ops))
    };
    assert_eq!(
        fx.one(&with("0.0.0.0:8080", "127.0.0.1:8080")),
        "server.listen, server.ops_listen: `server.listen` (\"0.0.0.0:8080\") and `server.ops_listen` (\"127.0.0.1:8080\") use the same port on overlapping hosts"
    );
    for (a, b) in [
        ("127.0.0.1:8080", "127.0.0.1:8080"),
        ("[::]:8080", "[::1]:8080"),
        ("[::1]:8080", "[::]:8080"),
        ("0.0.0.0:80", "0.0.0.0:80"),
    ] {
        assert!(fx.one(&with(a, b)).contains("use the same port on overlapping hosts"), "{a} {b}");
    }
    for (a, b) in [
        ("127.0.0.1:8080", "127.0.0.2:8080"),
        ("0.0.0.0:8080", "[::]:8080"),
        ("0.0.0.0:8080", "0.0.0.0:9090"),
        ("127.0.0.1:8080", "[::1]:8080"),
    ] {
        assert_eq!(fx.errors(&with(a, b)), Vec::<String>::new(), "{a} {b}");
    }
}

#[test]
fn c11_the_memory_budget_is_at_least_two_times_the_result_bytes_of_each_source() {
    let fx = Fx::new();
    let local = fx.local();
    let text = set(&local, "server.max_result_memory_bytes", "33554432");
    assert_eq!(
        fx.one(&text),
        "server.max_result_memory_bytes, clickhouse \"prod\".max_result_bytes: `server.max_result_memory_bytes` (33554432) is less than 2 x `clickhouse \"prod\".max_result_bytes` (33554432)"
    );
    assert!(fx.errors(&set(&local, "server.max_result_memory_bytes", "67108864")).is_empty());
    // The check runs for each source.
    let second =
        local.split_once("\nclickhouse \"prod\" {").map(|(_, b)| format!("\nclickhouse \"second\" {{{b}")).unwrap();
    let text = set(&format!("{local}{second}"), "server.max_result_memory_bytes", "33554432");
    assert_eq!(fx.errors(&text).len(), 2);
}

#[test]
fn c12_the_value_for_each_subject_is_at_most_max_concurrent() {
    let fx = Fx::new();
    let text = set(&fx.distributed(), "clickhouse.max_concurrent_per_subject", "20");
    assert_eq!(
        fx.one(&text),
        "clickhouse \"prod\".max_concurrent_per_subject, clickhouse \"prod\".max_concurrent: `clickhouse \"prod\".max_concurrent_per_subject` (20) is more than `clickhouse \"prod\".max_concurrent` (16)"
    );
    assert!(fx.errors(&set(&fx.distributed(), "clickhouse.max_concurrent_per_subject", "16")).is_empty());
}

#[test]
fn c13_max_concurrent_per_subject_exists_only_in_the_mode_authzen() {
    let fx = Fx::new();
    let local = fx.local();
    let text = replace_once(
        &local,
        "  queue_wait         = \"5s\"\n",
        "  queue_wait         = \"5s\"\n  max_concurrent_per_subject = 4\n",
    );
    assert_eq!(
        fx.one(&text),
        format!(
            "clickhouse \"prod\".max_concurrent_per_subject: not permitted when `access.mode` is \"open\"; the key is only for `access.mode` = \"authzen\" (line {})",
            line_of(&text, "max_concurrent_per_subject")
        )
    );
    let text = remove(&fx.distributed(), "clickhouse.max_concurrent_per_subject");
    assert_eq!(
        fx.one(&text),
        format!(
            "clickhouse \"prod\".max_concurrent_per_subject: missing attribute; expected an integer from 1 to 1024 (line {})",
            line_of(&text, "clickhouse \"prod\" {")
        )
    );
}

#[test]
fn k06_a_contradiction_error_names_both_keys_and_both_values() {
    let fx = Fx::new();
    let e = fx.one(&set(&fx.local(), "server.ops_listen", "\"0.0.0.0:8080\""));
    let (keys, rest) = e.split_once(": ").unwrap();
    assert_eq!(keys.split(", ").count(), 2);
    assert_eq!(rest.matches("(\"0.0.0.0:8080\")").count(), 2, "{e}");
    assert!(!e.contains("(line"));
}

// ---------------------------------------------------------------------------------------
// Stage 2: the checks of 13.13.
// ---------------------------------------------------------------------------------------

#[test]
fn telemetry_enabled_true_is_accepted() {
    let fx = Fx::new();
    let text = replace_once(
        &fx.local(),
        "telemetry {\n  enabled = false\n}",
        "telemetry {\n  enabled = true\n  otlp_endpoint = \"http://otel-collector:4318\"\n  service_name = \"hune\"\n  export_interval = \"15s\"\n}",
    );
    // A build from 0.7.0 has the OTLP exporter and accepts `enabled = true` (13.9).
    assert!(fx.errors(&text).is_empty(), "{:?}", fx.errors(&text));
    let loaded = fx.load(&text).unwrap();
    let TelemetryConfig::Enabled(t) = &loaded.config.telemetry else { panic!("enabled expected") };
    assert_eq!((t.otlp_endpoint.as_str(), t.service_name.as_str()), ("http://otel-collector:4318", "hune"));
    // `enabled = false` is valid (13.9).
    assert!(fx.errors(&fx.local()).is_empty());
}

#[test]
fn base_path_rule() {
    let fx = Fx::new();
    let local = fx.local();
    for ok in ["/", "/dashboards/", "/a/b/"] {
        assert!(fx.errors(&set(&local, "server.base_path", &quoted(ok))).is_empty(), "{ok}");
    }
    for bad in ["dashboards/", "/dashboards", "/./", "/a//b/", "/a b/"] {
        let text = set(&local, "server.base_path", &quoted(bad));
        let e = fx.one(&text);
        assert!(e.starts_with("server.base_path: expected \"/\" or a path that starts and ends with \"/\""), "{e}");
        assert!(e.contains(&format!("found \"{bad}\" (line {})", line_of(&text, "base_path"))), "{e}");
    }
}

#[test]
fn label_rule() {
    let fx = Fx::new();
    let text = replace_once(&fx.local(), "clickhouse \"prod\" {", "clickhouse \"Prod\" {");
    assert_eq!(
        fx.one(&text),
        format!(
            "clickhouse \"Prod\": expected a label that matches ^[a-z0-9][a-z0-9-]{{0,39}}$, found \"Prod\" (line {})",
            line_of(&text, "clickhouse \"Prod\"")
        )
    );
    for bad in ["a_b", "-a", "", &"a".repeat(41)] {
        let text = replace_once(&fx.local(), "clickhouse \"prod\" {", &format!("clickhouse \"{bad}\" {{"));
        assert!(fx.one(&text).contains("expected a label that matches"), "{bad}");
    }
    for ok in ["0", "a-b", &"a".repeat(40)] {
        let text = replace_once(&fx.local(), "clickhouse \"prod\" {", &format!("clickhouse \"{ok}\" {{"));
        assert!(fx.errors(&text).is_empty(), "{ok}");
    }
}

#[test]
fn the_limit_of_clickhouse_blocks_is_one_to_sixty_four() {
    let fx = Fx::new();
    let local = fx.local();
    let block = local.split_once("\nclickhouse \"prod\" {").map(|(_, b)| b.to_string()).unwrap();
    let head = local.split_once("\nclickhouse \"prod\" {").unwrap().0.to_string();
    let many = |n: usize| {
        let mut text = head.clone();
        for i in 0..n {
            text.push_str(&format!("\nclickhouse \"s{i}\" {{{block}"));
        }
        text
    };
    let loaded = fx.load(&many(64)).unwrap();
    assert_eq!(loaded.config.clickhouse.len(), 64);
    assert_eq!(loaded.config.clickhouse[0].id, "s0");
    assert_eq!(loaded.config.clickhouse[63].id, "s63");
    assert_eq!(fx.one(&many(65)), "clickhouse: expected 0 to 64 blocks `clickhouse \"<label>\"`, found 65");
    // C14: no source block of any kind.
    assert_eq!(
        fx.one(&head),
        "clickhouse, postgres, mongodb: expected 1 to 64 blocks `clickhouse \"<label>\"`, `postgres \"<label>\"` or `mongodb \"<label>\"` in all, found 0"
    );
}

#[test]
fn clickhouse_blocks_keep_the_file_order() {
    let fx = Fx::new();
    let local = fx.local();
    let (head, block) = local.split_once("\nclickhouse \"prod\" {").unwrap();
    let text = format!("{head}\nclickhouse \"zeta\" {{{block}\nclickhouse \"alpha\" {{{block}");
    let loaded = fx.load(&text).unwrap();
    let ids: Vec<&str> = loaded.config.clickhouse.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["zeta", "alpha"]);
    let out = render_print_config(&loaded);
    assert!(out.find("clickhouse \"zeta\".label").unwrap() < out.find("clickhouse \"alpha\".label").unwrap());
}

#[test]
fn clickhouse_label_text_and_database_rules() {
    let fx = Fx::new();
    let local = fx.local();
    let ok = "é".repeat(60);
    assert!(fx.errors(&set(&local, "clickhouse.label", &quoted(&ok))).is_empty(), "60 characters");
    let bad = "é".repeat(61);
    assert!(fx.one(&set(&local, "clickhouse.label", &quoted(&bad))).contains("expected a text of 1 to 60 characters"));
    for bad in ["1db", "a-b", "a b"] {
        let e = fx.one(&set(&local, "clickhouse.database", &quoted(bad)));
        assert!(e.contains("expected a ClickHouse identifier that matches ^[A-Za-z_][A-Za-z0-9_]{0,63}$"), "{e}");
    }
    assert!(fx.errors(&set(&local, "clickhouse.database", "\"_A1\"")).is_empty());
}

#[test]
fn url_rules() {
    let fx = Fx::new();
    let local = fx.local();
    for ok in ["https://clickhouse:8443", "http://127.0.0.1:8123", "https://ch.example", "http://[::1]:8123"] {
        let text = set(&local, "clickhouse.url", &quoted(ok));
        let text = if ok.starts_with("http:") { remove_tls_block_of_clickhouse(&text) } else { text };
        assert!(fx.errors(&text).is_empty(), "{ok}: {:?}", fx.errors(&text));
    }
    for bad in [
        "ftp://ch",
        "clickhouse:8443",
        "https://user@ch",
        "https://ch/path",
        "https://ch?x=1",
        "https://ch#f",
        "https://CH",
        "https://ch:0",
        "https://ch:99999",
        "https://ch:",
    ] {
        let text = set(&local, "clickhouse.url", &quoted(bad));
        let e = fx.one(&text);
        assert!(e.starts_with("clickhouse \"prod\".url: expected an absolute URL in canonical form with the scheme http or https, a host and an optional port"), "{bad}: {e}");
        assert!(e.contains(&format!("found \"{bad}\" (line {})", line_of(&text, "url "))), "{bad}: {e}");
    }
    // The PDP URL can have a path; the others cannot.
    let dist = fx.distributed();
    assert!(fx.errors(&set(&dist, "access.pdp.url", "\"https://pdp.example:4182/base/path\"")).is_empty());
    assert!(
        fx.one(&set(&dist, "access.pdp.url", "\"https://u:p@pdp.example\""))
            .contains("with no user info, query or fragment")
    );
    assert!(
        fx.one(&set(&dist, "access.pdp.url", "\"https://pdp.example/x?y=1\"")).starts_with("access.pdp.url: expected")
    );
}

#[test]
fn values_from_env_and_file_get_the_same_checks() {
    let fx = Fx::new();
    let dist = fx.distributed();
    let text = rename(&set(&dist, "access.pdp.url", "\"PDP_URL\""), "access.pdp.url", "url_env");
    let e = fx.load_env(&text, &[("PDP_URL", "ftp://pdp")]).unwrap_err();
    assert!(e[0].starts_with("access.pdp.url: expected an absolute URL"), "{e:?}");
    // The error shows the value of a non-secret key.
    assert!(e[0].contains("found \"ftp://pdp\""), "{e:?}");
    let loaded = fx.load_env(&text, &[("PDP_URL", "https://pdp.example:4182/base")]).unwrap();
    let AccessConfig::Authzen(a) = &loaded.config.access else { panic!() };
    assert_eq!(a.pdp.url, "https://pdp.example:4182/base");
    assert!(render_print_config(&loaded).contains("access.pdp.url = \"https://pdp.example:4182/base\"  # env:PDP_URL"));
    // Form F, for a value of the same key.
    crate::config::test_support::write_file(fx.dir.path().join("url-file"), "http://pdp.example:4182\n").unwrap();
    let text = rename(&set(&dist, "access.pdp.url", &quoted(&fx.path("url-file"))), "access.pdp.url", "url_file");
    let e = fx.errors(&text);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(
        e[0].starts_with("access.pdp.plain_http, access.pdp.url:"),
        "the value of the file is checked by C5: {e:?}"
    );
}

#[test]
fn identity_header_rules() {
    let fx = Fx::new();
    let dist = fx.distributed();
    for ok in ["X-Identity-Token", "x-id", "A", "X_Y.z~"] {
        assert!(fx.errors(&set(&dist, "access.identity.header", &quoted(ok))).is_empty(), "{ok}");
    }
    for bad in ["Bad Header", "X:Y", "X/Y", "é"] {
        let e = fx.one(&set(&dist, "access.identity.header", &quoted(bad)));
        assert!(e.contains("expected a header name of 1 to 64 token characters (RFC 9110)"), "{bad}: {e}");
    }
    for forbidden in ["Authorization", "proxy-authorization", "COOKIE", "Host", "content-type", "Content-Length"] {
        let e = fx.one(&set(&dist, "access.identity.header", &quoted(forbidden)));
        assert!(e.contains("expected a header name other than Authorization, Proxy-Authorization, Cookie, Host, Content-Type and Content-Length"), "{e}");
    }
}

#[test]
fn identity_audience_issuer_and_pdp_token_rules() {
    let fx = Fx::new();
    let dist = fx.distributed();
    assert_eq!(fx.errors(&set(&dist, "access.identity.audience", "\"\"")).len(), 1);
    assert!(fx.errors(&set(&dist, "access.identity.audience", &quoted(&"é".repeat(128)))).is_empty(), "256 bytes");
    assert_eq!(fx.errors(&set(&dist, "access.identity.audience", &quoted(&"é".repeat(129)))).len(), 1);
    assert_eq!(fx.errors(&set(&dist, "access.identity.audience", "\"a\\u0001b\"")).len(), 1, "control character");
    assert_eq!(fx.errors(&set(&dist, "access.identity.issuer", "\"a\\nb\"")).len(), 1);
    crate::config::test_support::write_file(fx.dir.path().join("tok"), "has space").unwrap();
    let text = set(&dist, "access.pdp.token_file", &quoted(&fx.path("tok")));
    assert_eq!(
        fx.one(&text).split(" (line").next().unwrap(),
        "access.pdp.token: expected a text of 1 to 4096 bytes with no space and no control character, found (value hidden)"
    );
}

#[test]
fn volume_path_must_be_canonical_and_absolute() {
    let fx = Fx::new();
    let local = fx.local();
    assert!(fx.errors(&set(&local, "storage.volume.path", "\"/var/lib/hune\"")).is_empty());
    for bad in ["/var/lib/", "/var//lib", "/var/../lib", "/var/./lib"] {
        let e = fx.one(&set(&local, "storage.volume.path", &quoted(bad)));
        assert!(e.contains("expected an absolute canonical path"), "{bad}: {e}");
        assert!(e.starts_with("storage.volume \"main\".path: "), "{bad}: {e}");
    }
    let e = fx.one(&set(&local, "storage.volume.path", "\"relative\""));
    assert!(
        e.starts_with("storage.volume \"main\".path: expected an absolute path of 1 to 4096 bytes, found \"relative\""),
        "{e}"
    );
}

#[test]
fn postgres_host_name_and_server_name_rules() {
    let fx = Fx::new();
    let dist = fx.distributed();
    for ok in ["10.0.0.5", "db.example", "::1"] {
        assert!(fx.errors(&set(&dist, "storage.postgres.host", &quoted(ok))).is_empty(), "{ok}");
    }
    for bad in ["bad host", "-x", "a_b"] {
        assert_eq!(fx.errors(&set(&dist, "storage.postgres.host", &quoted(bad))).len(), 1, "{bad}");
    }
    assert_eq!(fx.errors(&set(&dist, "storage.postgres.tls.server_name", "\"10.0.0.5_x\"")).len(), 1);
}

// ---- CA files ----

fn ca_text(fx: &Fx, content: &[u8]) -> String {
    crate::config::test_support::write_file(fx.dir.path().join("ca-test.pem"), content).unwrap();
    set(&fx.local(), "clickhouse.tls.ca_file", &quoted(&fx.path("ca-test.pem")))
}

#[test]
fn ca_file_checks_of_13_13() {
    let fx = Fx::new();
    let ok = ca_text(&fx, CA_A.as_bytes());
    assert!(fx.errors(&ok).is_empty());
    assert!(fx.errors(&ca_text(&fx, format!("{CA_A}\n{CA_B}").as_bytes())).is_empty(), "two certificates");
    assert!(fx.errors(&ca_text(&fx, CA_A.repeat(64).as_bytes())).is_empty(), "64 certificates");

    let key = "clickhouse \"prod\".tls.ca_file";
    let line = |t: &str| line_of(t, "ca_file ");
    let text = ca_text(&fx, CA_A.repeat(65).as_bytes());
    assert_eq!(
        fx.one(&text),
        format!("{key}: the CA file /run/x is not valid: 65 certificates; the maximum is 64 (line {})", line(&text))
            .replace("/run/x", &fx.back(&fx.path("ca-test.pem")))
    );
    let text = ca_text(&fx, b"");
    assert!(fx.one(&text).contains("is not valid: no PEM block CERTIFICATE; expected 1 to 64"));
    let text = ca_text(&fx, format!("comment\n{CA_A}").as_bytes());
    assert!(fx.one(&text).contains("is not valid: text outside the PEM blocks at line 1"));
    let text = ca_text(&fx, b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n");
    assert!(fx.one(&text).contains("is not valid: block 1 is not an X.509 certificate"));
    let text = ca_text(&fx, b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n");
    assert!(fx.one(&text).contains("a PEM block that is not CERTIFICATE"));
    let text = ca_text(&fx, &vec![b'#'; 1_048_577]);
    assert!(fx.one(&text).contains("the file has more than 1048576 bytes"));
    let text = set(&fx.local(), "clickhouse.tls.ca_file", "\"/nonexistent/ca.pem\"");
    assert_eq!(
        fx.one(&text),
        format!("{key}: cannot read the file /nonexistent/ca.pem: No such file or directory (line {})", line(&text))
    );
    let text = set(&fx.local(), "clickhouse.tls.ca_file", "\"relative.pem\"");
    assert!(fx.one(&text).contains("expected an absolute path of 1 to 4096 bytes, found \"relative.pem\""));
    // A symbolic link is not followed (13.6).
    let link = fx.dir.path().join("ca-link.pem");
    std::os::unix::fs::symlink(fx.dir.path().join("ch-ca.pem"), &link).unwrap();
    let text = set(&fx.local(), "clickhouse.tls.ca_file", &quoted(&link.display().to_string()));
    assert!(fx.one(&text).contains("the path is a symbolic link"));
    let text = set(&fx.local(), "clickhouse.tls.ca_file", &quoted(&fx.dir.path().display().to_string()));
    assert!(fx.one(&text).contains("the path is not a regular file"));
}

#[test]
fn each_ca_file_key_is_checked() {
    let fx = Fx::new();
    let dist = fx.distributed();
    for key in ["storage.postgres.tls.ca_file", "access.pdp.ca_file", "clickhouse.tls.ca_file"] {
        let text = set(&dist, key, "\"/nonexistent/ca.pem\"");
        assert!(fx.errors(&text).iter().any(|e| e.contains("cannot read the file /nonexistent/ca.pem")), "{key}");
    }
}

// ---- Public keys ----

#[test]
fn public_keys_checks_of_11_9() {
    let fx = Fx::new();
    let dist = fx.distributed();
    let with_file = |content: &str| {
        crate::config::test_support::write_file(fx.dir.path().join("keys"), content).unwrap();
        set(&dist, "access.identity.public_keys_file", &quoted(&fx.path("keys")))
    };
    // A PEM text, with the thumbprint as kid.
    let loaded = fx.load(&with_file(ED_PEM)).unwrap();
    assert!(render_print_config(&loaded).contains(
        "access.identity.public_keys = [\"Ed25519 kid=mlz7DJFMSylgSGNaOTfcl_fJaLUDgRSufeOX_Ac06GU\"]  # file:"
    ));
    // A bad text.
    let e = fx.one(&with_file("not a key"));
    assert!(e.starts_with("access.identity.public_keys: expected only empty lines outside the PEM blocks"), "{e}");
    let e = fx.one(&with_file(r#"{"keys":[]}"#));
    assert!(e.starts_with("access.identity.public_keys: expected 1 to 16 public keys, found 0"), "{e}");
    let e = fx.one(&with_file(
        r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"fpThXDcUJ4ljJD9Giq0dZX3Qb46N4UeeJyrQSXy7y5Q","d":"AAAA"}]}"#,
    ));
    assert!(e.contains("the private member \"d\""), "{e}");
    // The value from `_env` and the form V get the same checks.
    let env_text = rename(
        &set(&dist, "access.identity.public_keys", "\"KEYS\""),
        "access.identity.public_keys",
        "public_keys_env",
    );
    assert!(
        fx.load_env(&env_text, &[("KEYS", "x y")]).unwrap_err()[0].starts_with("access.identity.public_keys: expected")
    );
    let compact = ED_PEM.replace('\n', "\\n");
    let v_text = set(&dist, "access.identity.public_keys", &format!("\"{compact}\""));
    let loaded = fx.load(&v_text).unwrap();
    assert!(render_print_config(&loaded).contains("access.identity.public_keys = [\"Ed25519 kid=mlz7DJ"));
    assert!(render_print_config(&loaded).contains("  # file\n"));
}

// ---- The warnings ----

#[test]
fn w4_a_loopback_host_gives_no_warning_and_w5_too() {
    let fx = Fx::new();
    let text = set(&remove_tls_block_of_clickhouse(&fx.local()), "clickhouse.url", "\"http://localhost:8123\"");
    assert!(fx.load(&text).unwrap().warnings.iter().all(|w| w.key == "access.mode"));
    let dist = fx.distributed();
    let (before, rest) = dist.split_once("\n    tls {").unwrap();
    let (_, after) = rest.split_once("    }\n").unwrap();
    let base = format!("{before}{after}");
    let base = replace_once(&base, "sslmode           = \"verify-full\"", "sslmode           = \"disable\"");
    for host in ["127.0.0.1", "localhost", "::1"] {
        let loaded = fx.load(&set(&base, "storage.postgres.host", &quoted(host))).unwrap();
        assert!(loaded.warnings.iter().all(|w| w.key != "storage.postgres.sslmode"), "{host}");
    }
    let loaded = fx.load(&base).unwrap();
    assert!(
        loaded
            .warnings
            .iter()
            .any(|w| w.message == "storage.postgres.sslmode is disable for a host that is not a loopback address.")
    );
}

#[test]
fn w2_is_silent_for_a_listen_host_that_is_not_a_wildcard() {
    let fx = Fx::new();
    let text = set(&fx.distributed(), "server.listen", "\"10.0.0.7:8080\"");
    assert!(fx.load(&text).unwrap().warnings.is_empty());
    let text = set(&fx.distributed(), "server.listen", "\"[::]:8080\"");
    assert_eq!(fx.load(&text).unwrap().warnings.len(), 1);
    // W2 is only for the mode authzen: the mode open gives only W1.
    let text = set(&fx.local(), "server.listen", "\"10.0.0.7:8080\"");
    assert_eq!(fx.load(&text).unwrap().warnings.len(), 1);
}

#[test]
fn warnings_come_in_the_order_of_13_15() {
    let fx = Fx::new();
    let text = set(&remove_tls_block_of_clickhouse(&fx.local()), "clickhouse.url", "\"http://clickhouse:8123\"");
    let keys: Vec<String> = fx.load(&text).unwrap().warnings.iter().map(|w| w.key.clone()).collect();
    assert_eq!(keys, ["access.mode", "clickhouse \"prod\".url"]);
}

// ---- Stages, limits and files ----

#[test]
fn a_stage_runs_only_when_the_stage_before_it_has_no_error() {
    let fx = Fx::new();
    let local = fx.local();
    // Stage 1 error, stage 2 error and a contradiction: only stage 1 shows.
    let text = set(
        &set(&set(&local, "server.max_connections", "1"), "server.base_path", "\"x\""),
        "server.ops_listen",
        "\"0.0.0.0:8080\"",
    );
    let e = fx.errors(&text);
    assert_eq!(e.len(), 1);
    assert!(e[0].starts_with("server.max_connections:"));
    // Stage 2 error and a contradiction: only stage 2 shows.
    let text = set(&set(&local, "server.base_path", "\"x\""), "server.ops_listen", "\"0.0.0.0:8080\"");
    let e = fx.errors(&text);
    assert_eq!(e.len(), 1);
    assert!(e[0].starts_with("server.base_path:"));
}

#[test]
fn every_error_of_a_stage_is_in_the_list_in_file_order() {
    let fx = Fx::new();
    let text = set(
        &set(
            &set(&set(&fx.local(), "server.max_connections", "1"), "log.level", "\"loud\""),
            "storage.volume.history_keep",
            "0",
        ),
        "clickhouse.max_concurrent",
        "\"x\"",
    );
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 4, "{errors:?}");
    let lines: Vec<usize> =
        errors.iter().map(|e| e.rsplit("(line ").next().unwrap().trim_end_matches(')').parse().unwrap()).collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted);
}

#[test]
fn the_list_has_at_most_one_hundred_errors() {
    let fx = Fx::new();
    let junk: String = (0..150).map(|i| format!("  junk{i} = 1\n")).collect();
    let text = replace_once(&fx.local(), "log {\n", &format!("log {{\n{junk}"));
    let errors = fx.errors(&text);
    assert_eq!(errors.len(), 101);
    assert_eq!(errors[100], "and 50 more errors");
}

#[test]
fn the_configuration_file_limits() {
    let fx = Fx::new();
    let big = format!("{}\n# {}\n", fx.local(), "x".repeat(1_048_576));
    let e = fx.errors(&big);
    assert_eq!(e, vec![format!("the configuration file has {} bytes; the maximum is 1048576 bytes", big.len())]);
    let e = fx.errors(&format!("\u{feff}{}", fx.local()));
    assert_eq!(e, vec!["the configuration file starts with a byte order mark"]);
    let file = fx.dir.path().join("bad-utf8.hcl");
    crate::config::test_support::write_file(&file, b"\xff\xfe").unwrap();
    let e = load(&file, &CapturedEnvironment::default()).unwrap_err();
    assert_eq!(e[0].to_string(), "the configuration file is not UTF-8");
    let e = load(&fx.dir.path().join("none.hcl"), &CapturedEnvironment::default()).unwrap_err();
    assert!(e[0].to_string().starts_with("cannot read the configuration file "));
}

#[test]
fn the_nesting_limit_of_32_applies_to_the_file() {
    let fx = Fx::new();
    let deep = format!("{}{}", "a {".repeat(33), "}".repeat(33));
    let e = fx.errors(&format!("{}\n{deep}\n", fx.local()));
    assert_eq!(e.len(), 1);
    assert!(e[0].contains("nesting deeper than 32"));
}

#[test]
fn an_integer_in_the_file_is_at_most_the_largest_u64() {
    let fx = Fx::new();
    let text = set(&fx.local(), "server.max_request_bytes", "18446744073709551615");
    assert!(fx.one(&text).starts_with(
        "server.max_request_bytes: expected a size in bytes from 4259840 to 67108864, found 18446744073709551615"
    ));
    let text = set(&fx.local(), "server.max_request_bytes", "18446744073709551616");
    assert!(fx.one(&text).contains("found 18446744073709551616"));
}

#[test]
fn a_value_is_never_changed() {
    // No clamping: the value in the file is the value that applies.
    let fx = Fx::new();
    let text = set(&fx.local(), "server.max_connections", "16");
    assert_eq!(fx.load(&text).unwrap().config.server.max_connections, 16);
    let text = set(&fx.local(), "server.max_connections", "15");
    assert!(fx.errors(&text).len() == 1);
}

#[test]
fn print_config_escapes_text_like_json() {
    let fx = Fx::new();
    let text = set(&fx.local(), "clickhouse.label", "\"a\\\"b\\\\c\\u00e9\"");
    let out = render_print_config(&fx.load(&text).unwrap());
    assert!(out.contains("clickhouse \"prod\".label = \"a\\\"b\\\\cé\"  # file\n"), "{out}");
}

#[test]
fn the_effective_entries_have_the_three_members() {
    let fx = Fx::new();
    let loaded = fx.load(&fx.local()).unwrap();
    let hidden = loaded.effective.iter().find(|e| e.key == "clickhouse \"prod\".password").unwrap();
    assert_eq!(hidden.value, "<hidden>");
    assert_eq!(hidden.json, "\"<hidden>\"");
    assert_eq!(fx.back(&hidden.source), "file:/run/secrets/clickhouse/password");
    let primary = loaded.effective.iter().find(|e| e.key == "storage.volume \"main\".primary").unwrap();
    assert_eq!((primary.value.as_str(), primary.json.as_str(), primary.source.as_str()), ("true", "true", "file"));
}

#[test]
fn k01_every_enum_and_boolean_key_refuses_a_wrong_form() {
    let fx = Fx::new();
    let mut seen = HashMap::new();
    for key in SCHEMA.keys {
        let bad = match key.kind {
            Kind::Enum(values) => quoted(&values[0].to_uppercase()),
            Kind::Bool => "\"true\"".to_string(),
            _ => continue,
        };
        let base = base_text_for(&fx, key.path);
        let errors = stage1(&set(&base, key.path, &bad));
        assert!(
            errors.iter().any(|e| e.contains(&format!("expected {}", key.kind.expected()))),
            "{}: {errors:?}",
            key.path
        );
        seen.insert(key.path, ());
    }
    assert!(seen.len() >= 8);
}

#[test]
fn the_schema_values_have_the_kinds_of_the_typed_config() {
    // The reference values of the table are values of the right kind.
    let fx = Fx::new();
    for profile in [Profile::Local, Profile::Distributed] {
        let text = fx.substitute(&render_reference(ReferenceFormat::Hcl, profile));
        let checked = hclconf::check_text(&text, &SCHEMA, &CapturedEnvironment::default()).unwrap();
        for key in checked.keys() {
            assert!(!matches!(&key.value, Value::List(_)), "{}", key.display);
        }
    }
}

mod adhoc;
mod geomap;
mod inspect;
mod list_filters;
mod modes;
mod mongodb;
mod pep;
mod postgres;
mod repeat;
mod static_gzip;
mod stores;
mod transform_cells;
