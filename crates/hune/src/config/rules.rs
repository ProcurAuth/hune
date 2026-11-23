// SPDX-License-Identifier: MIT OR Apache-2.0
//! The contradiction checks C5 to C12, C14 to C18, C20 to C24 and C26 to C29 (stage 3 of 13.19) and the
//! warnings W1 to W9 (13.15).
//!
//! The engine finds C1 to C4, C6, C13, C19 and C25 as mode errors in stage 1.

use std::net::{IpAddr, SocketAddr};

use hclconf::{Checked, ConfigError, Value};
use hune_core::limits::{MAX_REPEAT_PER_DASHBOARD, MAX_REPEAT_PER_PANEL, MAX_REPEAT_PER_SECTION};

use super::Warning;
use super::keys::SCHEMA;
use super::stage2::{Stage2, parse_host_port};
use super::url::is_loopback_host;

fn text<'a>(c: &'a Checked, path: &str) -> Option<&'a str> {
    match &c.get(path)?.value {
        Value::Str(s) => Some(s),
        _ => None,
    }
}

fn boolean(c: &Checked, path: &str) -> Option<bool> {
    match c.get(path)?.value {
        Value::Bool(b) => Some(b),
        _ => None,
    }
}

fn int_in(c: &Checked, label: &str, path: &str) -> Option<u64> {
    match c.get_in(label, path)?.value {
        Value::Int(n) => Some(n),
        _ => None,
    }
}

fn text_in<'a>(c: &'a Checked, label: &str, path: &str) -> Option<&'a str> {
    match &c.get_in(label, path)?.value {
        Value::Str(s) => Some(s),
        _ => None,
    }
}

/// The form of a host that C20 compares. The check never resolves a name (13.13: no network), so it
/// folds only the spellings that name one server for sure: a letter case, one trailing dot, the brackets of
/// an IPv6 address, an IPv4 address in the IPv6 form (`::ffff:127.0.0.1`), and the loopback names. Each
/// loopback address (`127.0.0.0/8`, `::1`) and the name `localhost` give the same key.
fn host_key(host: &str) -> String {
    let lower = host.to_ascii_lowercase();
    let bare = lower.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(&lower);
    let bare = bare.strip_suffix('.').unwrap_or(bare);
    if bare == "localhost" {
        return "loopback".to_owned();
    }
    match bare.parse::<IpAddr>() {
        Ok(ip) => {
            let ip = ip.to_canonical();
            if ip.is_loopback() { "loopback".to_owned() } else { ip.to_string() }
        }
        Err(_) => bare.to_owned(),
    }
}

/// True when two hosts name the same server: equal names without case and without a trailing dot, equal
/// IP addresses, or two loopback spellings (see [`host_key`]).
fn same_host(a: &str, b: &str) -> bool {
    host_key(a) == host_key(b)
}

fn quote(text: &str) -> String {
    hclconf::value::json_string(text)
}

fn contradiction(keys: &str, message: String) -> ConfigError {
    ConfigError::new(keys, message, None)
}

/// True when two hosts overlap: they are equal, or one is a wildcard of the same family.
fn hosts_overlap(a: &SocketAddr, b: &SocketAddr) -> bool {
    if a.is_ipv4() != b.is_ipv4() {
        return false;
    }
    a.ip() == b.ip() || a.ip().is_unspecified() || b.ip().is_unspecified()
}

/// Run the contradiction checks. They run only when stages 1 and 2 have no error.
pub fn contradictions(c: &Checked, stage: &Stage2) -> Vec<ConfigError> {
    let mut errors = Vec::new();
    source_structure(c, &mut errors);

    // C5: access.pdp.plain_http and the scheme of access.pdp.url.
    if let (Some(plain), Some((url, _))) = (boolean(c, "access.pdp.plain_http"), stage.urls.get("access.pdp.url")) {
        let scheme = if url.https { "https" } else { "http" };
        if !plain && !url.https {
            errors.push(contradiction(
                "access.pdp.plain_http, access.pdp.url",
                format!(
                    "`access.pdp.plain_http` (false) needs the scheme https, but `access.pdp.url` has the scheme {}",
                    quote(scheme)
                ),
            ));
        } else if plain && url.https {
            errors.push(contradiction(
                "access.pdp.plain_http, access.pdp.url",
                format!(
                    "`access.pdp.plain_http` (true) needs the scheme http, but `access.pdp.url` has the scheme {}",
                    quote(scheme)
                ),
            ));
        }
    }

    // C7: the url of a clickhouse block and its block tls.
    for label in c.labels("clickhouse") {
        let url_key = format!("clickhouse {}.url", quote(label));
        let tls_key = format!("clickhouse {}.tls", quote(label));
        let Some((url, _)) = stage.urls.get(&url_key) else { continue };
        let has_tls = c.block_in(label, "clickhouse.tls").is_some();
        let keys = format!("{url_key}, {tls_key}");
        if url.https && !has_tls {
            errors.push(contradiction(&keys, format!("`{url_key}` ({}) needs the block tls", quote(&url.text))));
        } else if !url.https && has_tls {
            errors.push(contradiction(
                &keys,
                format!("`{url_key}` ({}) has the scheme http and forbids the block tls", quote(&url.text)),
            ));
        }
    }

    // C8: telemetry.otlp_endpoint and telemetry.ca_file.
    if let Some((url, _)) = stage.urls.get("telemetry.otlp_endpoint") {
        let ca = c.get("telemetry.ca_file");
        if url.https && ca.is_none() {
            errors.push(contradiction(
                "telemetry.otlp_endpoint, telemetry.ca_file",
                format!(
                    "`telemetry.otlp_endpoint` ({}) has the scheme https and needs `telemetry.ca_file`",
                    quote(&url.text)
                ),
            ));
        } else if let (false, Some(ca)) = (url.https, ca) {
            errors.push(contradiction(
                "telemetry.otlp_endpoint, telemetry.ca_file",
                format!(
                    "`telemetry.otlp_endpoint` ({}) has the scheme http and forbids `telemetry.ca_file` ({})",
                    quote(&url.text),
                    ca.value.print()
                ),
            ));
        }
    }

    // C9: server.listen and server.ops_listen.
    if let (Some(listen), Some(ops)) = (text(c, "server.listen"), text(c, "server.ops_listen"))
        && let (Ok(a), Ok(b)) = (listen.parse::<SocketAddr>(), ops.parse::<SocketAddr>())
        && a.port() == b.port()
        && hosts_overlap(&a, &b)
    {
        errors.push(contradiction(
            "server.listen, server.ops_listen",
            format!(
                "`server.listen` ({}) and `server.ops_listen` ({}) use the same port on overlapping hosts",
                quote(listen),
                quote(ops)
            ),
        ));
    }

    // C10: storage.volume.read_only and server.editing.
    if let (Some(true), Some(true)) = (boolean(c, "storage.volume.read_only"), boolean(c, "server.editing")) {
        errors.push(contradiction(
            "storage.volume.read_only, server.editing",
            "`storage.volume.read_only` (true) needs `server.editing` = false; found `server.editing` (true)"
                .to_string(),
        ));
    }

    // C21: server.query_inspect and the right to see the SQL in the mode authzen. The right needs
    // `source.query` and `dashboard.write`, and `server.editing` = false does not publish them (11.18).
    if let (Some(true), Some(false), Some("authzen")) =
        (boolean(c, "server.query_inspect"), boolean(c, "server.editing"), text(c, "access.mode"))
    {
        errors.push(contradiction(
            "server.query_inspect, server.editing, access.mode",
            "`server.query_inspect` (true) in the mode `authzen` needs `server.editing` = true, because the right to see the SQL is `source.query` with `dashboard.write`; found `server.editing` (false)"
                .to_string(),
        ));
    }

    // C28 and C29: the repeat limits of the dashboard. A key that is absent has its default (13.7).
    let repeat = |key: &str, default: usize| match c.get(key).map(|k| &k.value) {
        Some(Value::Int(n)) => *n,
        _ => default as u64,
    };
    let per_dashboard = repeat("server.repeat_max_per_dashboard", MAX_REPEAT_PER_DASHBOARD);
    // C28 compares the key per panel, and C29 the key per section.
    for (key, default) in [
        ("server.repeat_max_per_panel", MAX_REPEAT_PER_PANEL),
        ("server.repeat_max_per_section", MAX_REPEAT_PER_SECTION),
    ] {
        let per = repeat(key, default);
        if per > per_dashboard {
            errors.push(contradiction(
                &format!("{key}, server.repeat_max_per_dashboard"),
                format!(
                    "`{key}` ({per}) is more than `server.repeat_max_per_dashboard` ({per_dashboard}): a repeat of one panel or one section cannot have more copies than the dashboard"
                ),
            ));
        }
    }

    // C11 and C12: the limits of each source.
    let memory = match c.get("server.max_result_memory_bytes").map(|k| &k.value) {
        Some(Value::Int(n)) => Some(*n),
        _ => None,
    };
    for label in c.labels("clickhouse") {
        let name = |key: &str| format!("clickhouse {}.{key}", quote(label));
        if let (Some(memory), Some(bytes)) = (memory, int_in(c, label, "clickhouse.max_result_bytes"))
            && memory < bytes.saturating_mul(2)
        {
            errors.push(contradiction(
                &format!("server.max_result_memory_bytes, {}", name("max_result_bytes")),
                format!(
                    "`server.max_result_memory_bytes` ({memory}) is less than 2 x `{}` ({bytes})",
                    name("max_result_bytes")
                ),
            ));
        }
        if let (Some(per), Some(all)) =
            (int_in(c, label, "clickhouse.max_concurrent_per_subject"), int_in(c, label, "clickhouse.max_concurrent"))
            && per > all
        {
            errors.push(contradiction(
                &format!("{}, {}", name("max_concurrent_per_subject"), name("max_concurrent")),
                format!(
                    "`{}` ({per}) is more than `{}` ({all})",
                    name("max_concurrent_per_subject"),
                    name("max_concurrent")
                ),
            ));
        }
    }
    postgres_limits(c, memory, &mut errors);
    mongodb_limits(c, memory, &mut errors);
    errors
}

/// C14, C15, C16, C22, C26 and C27: the count of source blocks, the shared label space, the block
/// `tls` of a `postgres` block and the rules of a `mongodb` block.
fn source_structure(c: &Checked, errors: &mut Vec<ConfigError>) {
    // C14: 1 to 64 source blocks of the three kinds.
    let total = c.labels("clickhouse").len() + c.labels("postgres").len() + c.labels("mongodb").len();
    if !(1..=64).contains(&total) {
        errors.push(contradiction(
            "clickhouse, postgres, mongodb",
            format!(
                "expected 1 to 64 blocks `clickhouse \"<label>\"`, `postgres \"<label>\"` or `mongodb \"<label>\"` in all, found {total}"
            ),
        ));
    }
    // C15: a label is unique across the three kinds. The first block in the file owns the label.
    let mut blocks: Vec<_> = c
        .blocks()
        .iter()
        .filter(|b| matches!(b.path, "clickhouse" | "postgres" | "mongodb") && b.label.is_some())
        .collect();
    blocks.sort_by_key(|b| b.line);
    for (i, block) in blocks.iter().enumerate() {
        if let Some(first) = blocks[..i].iter().find(|f| f.label == block.label) {
            errors.push(ConfigError::at(
                block.display.clone(),
                format!(
                    "duplicate label; the first block with this label is `{}` at line {}",
                    first.display, first.line
                ),
                block.line,
            ));
        }
    }
    // C16: the block tls exists only with `verify-full`.
    for block in c.blocks().iter().filter(|b| b.path == "postgres") {
        let Some(label) = block.label.as_deref() else { continue };
        let Some(mode) = text_in(c, label, "postgres.sslmode") else { continue };
        let mode_key = format!("postgres {}.sslmode", quote(label));
        let tls_key = format!("postgres {}.tls", quote(label));
        match (mode, c.block_in(label, "postgres.tls")) {
            ("verify-full", None) => errors.push(ConfigError::at(
                tls_key,
                format!("missing block; expected when `{mode_key}` is \"verify-full\""),
                block.line,
            )),
            ("disable", Some(tls)) => errors.push(ConfigError::at(
                tls_key,
                format!(
                    "not permitted when `{mode_key}` is \"disable\"; the key is only for `{mode_key}` = \"verify-full\""
                ),
                tls.line,
            )),
            _ => {}
        }
    }
    mongodb_structure(c, errors);
}

/// The keys that exist only in one topology of a `mongodb` block (C26): the topology, then the keys.
const MONGO_DIRECT_KEYS: [&str; 2] = ["mongodb.host", "mongodb.port"];
const MONGO_REPLICA_SET_KEYS: [&str; 2] = ["mongodb.hosts", "mongodb.replica_set"];

/// C22, C26 and C27 for each `mongodb` block, in file order.
fn mongodb_structure(c: &Checked, errors: &mut Vec<ConfigError>) {
    for block in c.blocks().iter().filter(|b| b.path == "mongodb") {
        let Some(label) = block.label.as_deref() else { continue };
        let name = |key: &str| format!("mongodb {}.{key}", quote(label));
        // C22: the block tls exists only with `verify-full`.
        if let Some(mode) = text_in(c, label, "mongodb.tls_mode") {
            let mode_key = name("tls_mode");
            let tls_key = name("tls");
            match (mode, c.block_in(label, "mongodb.tls")) {
                ("verify-full", None) => errors.push(ConfigError::at(
                    tls_key,
                    format!("missing block; expected when `{mode_key}` is \"verify-full\""),
                    block.line,
                )),
                ("disable", Some(tls)) => errors.push(ConfigError::at(
                    tls_key,
                    format!(
                        "not permitted when `{mode_key}` is \"disable\"; the key is only for `{mode_key}` = \"verify-full\""
                    ),
                    tls.line,
                )),
                _ => {}
            }
        }
        // C26: the keys of the topology that the block names, and no key of the other topology.
        if let Some(topology) = text_in(c, label, "mongodb.topology") {
            let topology_key = name("topology");
            // `other` is the topology that the refused keys are for.
            let (needed, refused, other) = if topology == "direct" {
                (MONGO_DIRECT_KEYS, MONGO_REPLICA_SET_KEYS, "replica_set")
            } else {
                (MONGO_REPLICA_SET_KEYS, MONGO_DIRECT_KEYS, "direct")
            };
            for path in needed {
                if c.get_in(label, path).is_none() {
                    let expected = SCHEMA.key(path).map(|k| k.kind.expected()).unwrap_or_default();
                    let attr = path.strip_prefix("mongodb.").unwrap_or(path);
                    errors.push(ConfigError::at(
                        name(attr),
                        format!("missing attribute; expected {expected}"),
                        block.line,
                    ));
                }
            }
            for path in refused {
                if let Some(key) = c.get_in(label, path) {
                    errors.push(ConfigError::at(
                        key.display.clone(),
                        format!(
                            "not permitted when `{topology_key}` is {}; the key is only for `{topology_key}` = {}",
                            quote(topology),
                            quote(other),
                        ),
                        key.line,
                    ));
                }
            }
        }
        // C27: `databases` holds none of `admin`, `local` and `config`.
        if let Some(key) = c.get_in(label, "mongodb.databases")
            && let Value::List(items) = &key.value
        {
            for item in items {
                if let Value::Str(db) = item
                    && matches!(db.as_str(), "admin" | "local" | "config")
                {
                    errors.push(ConfigError::at(
                        key.display.clone(),
                        format!("expected a name that is not \"admin\", \"local\" or \"config\", found {}", quote(db)),
                        key.line,
                    ));
                }
            }
        }
    }
}

/// C23 and C24: the limits of each `mongodb` block.
fn mongodb_limits(c: &Checked, memory: Option<u64>, errors: &mut Vec<ConfigError>) {
    for label in c.labels("mongodb") {
        let name = |key: &str| format!("mongodb {}.{key}", quote(label));
        if let (Some(memory), Some(bytes)) = (memory, int_in(c, label, "mongodb.max_result_bytes"))
            && memory < bytes.saturating_mul(2)
        {
            errors.push(contradiction(
                &format!("server.max_result_memory_bytes, {}", name("max_result_bytes")),
                format!(
                    "`server.max_result_memory_bytes` ({memory}) is less than 2 x `{}` ({bytes})",
                    name("max_result_bytes")
                ),
            ));
        }
        if let (Some(per), Some(all)) =
            (int_in(c, label, "mongodb.max_concurrent_per_subject"), int_in(c, label, "mongodb.max_concurrent"))
            && per > all
        {
            errors.push(contradiction(
                &format!("{}, {}", name("max_concurrent_per_subject"), name("max_concurrent")),
                format!(
                    "`{}` ({per}) is more than `{}` ({all})",
                    name("max_concurrent_per_subject"),
                    name("max_concurrent")
                ),
            ));
        }
    }
}

/// C17, C18 and C20: the limits of each `postgres` block and its role.
fn postgres_limits(c: &Checked, memory: Option<u64>, errors: &mut Vec<ConfigError>) {
    let store_postgres = text(c, "storage.backend") == Some("postgres");
    for label in c.labels("postgres") {
        let name = |key: &str| format!("postgres {}.{key}", quote(label));
        if let (Some(memory), Some(bytes)) = (memory, int_in(c, label, "postgres.max_result_bytes"))
            && memory < bytes.saturating_mul(2)
        {
            errors.push(contradiction(
                &format!("server.max_result_memory_bytes, {}", name("max_result_bytes")),
                format!(
                    "`server.max_result_memory_bytes` ({memory}) is less than 2 x `{}` ({bytes})",
                    name("max_result_bytes")
                ),
            ));
        }
        if let (Some(per), Some(all)) =
            (int_in(c, label, "postgres.max_concurrent_per_subject"), int_in(c, label, "postgres.max_concurrent"))
            && per > all
        {
            errors.push(contradiction(
                &format!("{}, {}", name("max_concurrent_per_subject"), name("max_concurrent")),
                format!(
                    "`{}` ({per}) is more than `{}` ({all})",
                    name("max_concurrent_per_subject"),
                    name("max_concurrent")
                ),
            ));
        }
        // C20: a source needs its own role. A read mode that does not read a user value skips it.
        if store_postgres
            && let (Some(host), Some(port), Some(user)) = (
                text_in(c, label, "postgres.host"),
                int_in(c, label, "postgres.port"),
                text_in(c, label, "postgres.user"),
            )
            && let (Some(s_host), Some(s_port), Some(s_user)) = (
                text(c, "storage.postgres.host"),
                match c.get("storage.postgres.port").map(|k| &k.value) {
                    Some(Value::Int(n)) => Some(*n),
                    _ => None,
                },
                text(c, "storage.postgres.user"),
            )
            && port == s_port
            && same_host(host, s_host)
            && user == s_user
        {
            errors.push(contradiction(
                &format!("{}, storage.postgres.user", name("user")),
                format!(
                    "`{}` ({}) equals `storage.postgres.user` on the same host and port; a source needs its own read-only role",
                    name("user"),
                    quote(user)
                ),
            ));
        }
    }
}

fn warning(key: &str, message: String) -> Warning {
    Warning { key: key.to_string(), message }
}

/// Make the warnings of 13.15, in the order W1 to W9 and, for W4, W7 and W8, in file order.
pub fn warnings(c: &Checked, stage: &Stage2) -> Vec<Warning> {
    let mut out = Vec::new();
    let authzen = text(c, "access.mode") == Some("authzen");
    if text(c, "access.mode") == Some("open") {
        out.push(warning(
            "access.mode",
            "access.mode is open: Hune checks no identity and asks no PDP. A proxy that authenticates the users must guard each route of server.listen.".to_string(),
        ));
    }
    if authzen {
        let wildcard = text(c, "server.listen")
            .and_then(|t| t.parse::<SocketAddr>().ok())
            .is_some_and(|a| a.ip().is_unspecified());
        if wildcard {
            out.push(warning(
                "server.listen",
                "server.listen has a wildcard host: make sure that only the edge reaches this port (ProcurAuth 9.4)."
                    .to_string(),
            ));
        }
    }
    if boolean(c, "access.pdp.plain_http") == Some(true) {
        out.push(warning(
            "access.pdp.plain_http",
            "access.pdp.plain_http is true: the PDP token and the questions go without TLS.".to_string(),
        ));
    }
    for label in c.labels("clickhouse") {
        let key = format!("clickhouse {}.url", quote(label));
        if let Some((url, _)) = stage.urls.get(&key)
            && !url.https
            && !is_loopback_host(&url.host)
        {
            out.push(warning(&key, format!("{key} has no TLS to a host that is not a loopback address.")));
        }
    }
    if text(c, "storage.postgres.sslmode") == Some("disable")
        && let Some(host) = text(c, "storage.postgres.host")
        && !is_loopback_host(host)
    {
        out.push(warning(
            "storage.postgres.sslmode",
            "storage.postgres.sslmode is disable for a host that is not a loopback address.".to_string(),
        ));
    }
    if let Some((url, _)) = stage.urls.get("telemetry.otlp_endpoint")
        && !url.https
        && !is_loopback_host(&url.host)
    {
        out.push(warning(
            "telemetry.otlp_endpoint",
            "telemetry.otlp_endpoint has no TLS to a host that is not a loopback address.".to_string(),
        ));
    }
    // W7: a `postgres` block without TLS to a host that is not a loopback address.
    for label in c.labels("postgres") {
        let key = format!("postgres {}.sslmode", quote(label));
        if text_in(c, label, "postgres.sslmode") == Some("disable")
            && let Some(host) = text_in(c, label, "postgres.host")
            && !is_loopback_host(host)
        {
            out.push(warning(&key, format!("{key} is disable for a host that is not a loopback address.")));
        }
    }
    // W8: a `mongodb` block without TLS to a host that is not a loopback address. In the topology
    // `replica_set`, one such host in `hosts` is enough. A host name is compared without case.
    for label in c.labels("mongodb") {
        let key = format!("mongodb {}.tls_mode", quote(label));
        if text_in(c, label, "mongodb.tls_mode") != Some("disable") {
            continue;
        }
        let remote = match text_in(c, label, "mongodb.topology") {
            Some("direct") => {
                text_in(c, label, "mongodb.host").is_some_and(|h| !is_loopback_host(&h.to_ascii_lowercase()))
            }
            Some("replica_set") => match c.get_in(label, "mongodb.hosts").map(|k| &k.value) {
                Some(Value::List(items)) => items.iter().any(|item| match item {
                    Value::Str(text) => {
                        parse_host_port(text).is_some_and(|(h, _)| !is_loopback_host(&h.to_ascii_lowercase()))
                    }
                    _ => false,
                }),
                _ => false,
            },
            _ => false,
        };
        if remote {
            out.push(warning(&key, format!("{key} is disable for a host that is not a loopback address.")));
        }
    }
    // W9: the block `geomap` (13.12c). Each viewer contacts the tile host.
    if let Some(tile) = &stage.geomap_tile {
        out.push(warning("geomap.tile_url", super::geomap::w9_message(tile)));
    }
    out
}

#[cfg(test)]
mod host_tests {
    use super::*;

    #[test]
    fn same_host_folds_the_spellings_of_one_server() {
        // Finding L3. Stage 2 refuses a trailing dot and a bracket in `host`, but the check folds them anyway.
        for (a, b) in [
            ("localhost", "127.0.0.1"),
            ("LocalHost.", "localhost"),
            ("localhost", "::1"),
            ("127.0.0.1", "::ffff:127.0.0.1"),
            ("[::1]", "::1"),
            ("[::ffff:127.0.0.1]", "127.0.0.1"),
            ("127.1.2.3", "localhost"),
            ("db.internal.", "DB.Internal"),
            ("10.0.0.7", "::ffff:10.0.0.7"),
            ("0:0:0:0:0:0:0:1", "::1"),
        ] {
            assert!(same_host(a, b), "{a} and {b}");
            assert!(same_host(b, a), "{b} and {a}");
        }
        for (a, b) in [
            ("localhost", "10.0.0.7"),
            ("localhost.example", "localhost"),
            ("db.internal", "db.internal.example"),
            ("::1", "::2"),
            ("10.0.0.7", "10.0.0.8"),
            ("", "localhost"),
        ] {
            assert!(!same_host(a, b), "{a} and {b}");
        }
    }
}
