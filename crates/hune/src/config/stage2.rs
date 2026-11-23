//! Stage 2 of the checks (13.13, 13.19): the checks of the values after the key table.
//!
//! The values from `_env` and `_file` get the same checks as the values of the file. The
//! engine gives the final value of each key.

use std::collections::HashMap;

use hclconf::{Checked, ConfigError, ResolvedKey, Value};

use super::ReadMode;
use super::ca::read_ca_file;
use super::pubkeys::{ParsedKey, PublicKeyInfo, parse_public_key_set};
use super::url::{HttpUrl, parse_http_url, rule_text};
use crate::access::{PepInputs, SourceInfo};

/// What stage 2 reads and keeps for the typed configuration.
#[derive(Debug, Default)]
pub struct Stage2 {
    /// The bytes of each CA file, by the key path of the `ca_file` key.
    pub ca: HashMap<String, Vec<u8>>,
    /// The keys of `access.identity.public_keys`.
    pub public_keys: Vec<PublicKeyInfo>,
    /// The same keys with their SPKI PEM blocks, for the PEP (11.10).
    pub key_set: Vec<ParsedKey>,
    /// Each checked URL with its path, by key path.
    pub urls: HashMap<String, (HttpUrl, String)>,
    /// The parts of `geomap.tile_url` (13.12c), parsed one time. The policy of 12.8 and the session use them.
    pub geomap_tile: Option<super::geomap::TileUrl>,
}

/// The names that `access.identity.header` cannot take, in lower case (13.11).
const FORBIDDEN_HEADERS: [&str; 6] =
    ["authorization", "proxy-authorization", "cookie", "host", "content-type", "content-length"];

fn quote(text: &str) -> String {
    hclconf::value::json_string(text)
}

fn text_of(key: &ResolvedKey) -> &str {
    match &key.value {
        Value::Str(s) => s,
        _ => "",
    }
}

/// `/` or a path that starts and ends with `/`, with segments of 1 to 64 characters from
/// `A-Z a-z 0-9 . _ ~ -`, not `.` or `..` (13.8).
pub fn valid_base_path(path: &str) -> bool {
    if path == "/" {
        return true;
    }
    if path.len() > 128 || !path.starts_with('/') || !path.ends_with('/') {
        return false;
    }
    let inner = &path[1..path.len() - 1];
    inner.split('/').all(|segment| {
        !segment.is_empty()
            && segment.len() <= 64
            && segment != "."
            && segment != ".."
            && segment.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-'))
    })
}

/// The label rule of 13.12: `^[a-z0-9][a-z0-9-]{0,39}$`.
pub fn valid_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 40
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// The rule of `postgres "<id>".search_path` (13.13): 1 to 8 names separated by `,` with no blank. A
/// name matches `^[a-z_][a-z0-9_]{0,62}$`, does not start with `pg_temp` and is not repeated.
pub fn valid_search_path(text: &str) -> bool {
    let names: Vec<&str> = text.split(',').collect();
    if text.len() > 256 || names.len() > 8 {
        return false;
    }
    let valid = |name: &&str| {
        let bytes = name.as_bytes();
        !bytes.is_empty()
            && bytes.len() <= 63
            && (bytes[0].is_ascii_lowercase() || bytes[0] == b'_')
            && bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
            && !name.starts_with("pg_temp")
    };
    names.iter().all(valid) && names.iter().enumerate().all(|(i, n)| !names[..i].contains(n))
}

/// The name of a MongoDB database in `auth_database` and `databases`: `^[A-Za-z0-9_-]{1,63}$` (13.12b).
pub fn valid_mongo_database(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
}

/// The name of a replica set: `^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$` (13.12b).
pub fn valid_replica_set(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_alphanumeric()
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// The name of a collection: `^[A-Za-z0-9_][A-Za-z0-9_.-]{0,99}$`, and not a name that starts with
/// `system.` (13.12b).
pub fn valid_mongo_collection(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 100
        && (bytes[0].is_ascii_alphanumeric() || bytes[0] == b'_')
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
        && !text.starts_with("system.")
}

/// One item of `mongodb "<id>".hosts` (13.13): `host:port`. The host is a DNS name or an IPv4
/// address, or an IPv6 address in brackets. The port is a whole number from 1 to 65535 with no
/// leading zero. The result is the host, without the brackets, and the port. A text that starts with
/// `/`, or that holds `://`, `@`, `?` or a blank, has no valid form, so it gives `None`.
pub fn parse_host_port(item: &str) -> Option<(String, u16)> {
    let (host, port) = if let Some(rest) = item.strip_prefix('[') {
        let (inner, after) = rest.split_once(']')?;
        let port = after.strip_prefix(':')?;
        inner.parse::<std::net::Ipv6Addr>().ok()?;
        (inner, port)
    } else {
        let (host, port) = item.rsplit_once(':')?;
        if !(hclconf::kinds::is_dns_name(host) || host.parse::<std::net::Ipv4Addr>().is_ok()) {
            return None;
        }
        (host, port)
    };
    if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) || (port.len() > 1 && port.starts_with('0')) {
        return None;
    }
    match port.parse::<u16>() {
        Ok(n) if n >= 1 => Some((host.to_string(), n)),
        _ => None,
    }
}

/// The key that makes two hosts of a list equal: an IP address by its value, a name without case
/// (13.13).
fn host_identity(host: &str) -> String {
    match host.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.to_string(),
        Err(_) => host.to_ascii_lowercase(),
    }
}

/// Check the items of a list of `mongodb "<id>"` (13.13): each item has the form `rule_text` and
/// no item repeats. The result is the first problem.
fn check_mongo_list(key: &ResolvedKey) -> Option<String> {
    let Value::List(items) = &key.value else { return None };
    let (valid, rule): (fn(&str) -> bool, &str) = match key.path {
        "mongodb.databases" => (valid_mongo_database, "^[A-Za-z0-9_-]{1,63}$"),
        "mongodb.collections" => {
            (valid_mongo_collection, "^[A-Za-z0-9_][A-Za-z0-9_.-]{0,99}$ and not starting with system.")
        }
        _ => {
            (|t| parse_host_port(t).is_some(), "host:port, with an IPv6 address in brackets and a port from 1 to 65535")
        }
    };
    let mut seen: Vec<String> = Vec::new();
    for item in items {
        let Value::Str(text) = item else { continue };
        if !valid(text) {
            return Some(format!("expected an item that matches {rule}, found {}", quote(text)));
        }
        let identity = match parse_host_port(text).filter(|_| key.path == "mongodb.hosts") {
            Some((host, port)) => format!("{}:{port}", host_identity(&host)),
            None => text.clone(),
        };
        if seen.contains(&identity) {
            return Some(format!("expected a list with no repeated item, found {} more than once", quote(text)));
        }
        seen.push(identity);
    }
    None
}

/// Check the items of `clickhouse "<id>".adhoc_columns` (13.13, decision ID-K329): each item is a plain column name
/// `^[A-Za-z_][A-Za-z0-9_]{0,63}$` and no item repeats (exact, case-sensitive). The result is the first problem.
fn check_adhoc_columns(key: &ResolvedKey) -> Option<String> {
    let Value::List(items) = &key.value else { return None };
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for item in items {
        let Value::Str(text) = item else { continue };
        if !valid_identifier(text) {
            return Some(format!(
                "expected an item that matches ^[A-Za-z_][A-Za-z0-9_]{{0,63}}$, found {}",
                quote(text)
            ));
        }
        if !seen.insert(text.as_str()) {
            return Some(format!("expected a list with no repeated item, found {} more than once", quote(text)));
        }
    }
    None
}

/// A ClickHouse identifier: `^[A-Za-z_][A-Za-z0-9_]{0,63}$` (13.12).
pub fn valid_identifier(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        && bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// A header name of RFC 9110: 1 to 64 token characters.
pub fn valid_header_token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 64
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

fn canonical_path(path: &str) -> bool {
    path == "/"
        || (path.starts_with('/')
            && !path.ends_with('/')
            && path[1..].split('/').all(|s| !s.is_empty() && s != "." && s != ".."))
}

/// Run the checks of stage 2 that the read mode asks for (13.18). The list holds every error,
/// in the order of the file.
///
/// A key that the mode does not check gets no check. If it is a URL that the file gives, stage 2
/// still parses it without a message, so the contradiction checks that need the scheme can run.
pub fn run(checked: &Checked, mode: ReadMode) -> Result<Stage2, Vec<ConfigError>> {
    let mut errors: Vec<ConfigError> = Vec::new();
    let mut stage = Stage2::default();
    for key in checked.keys() {
        if !mode.checks_key(key) {
            if key.is_read()
                && matches!(key.path, "telemetry.otlp_endpoint" | "clickhouse.url" | "access.pdp.url")
                && let Some(parsed) = parse_http_url(text_of(key), key.path == "access.pdp.url")
            {
                stage.urls.insert(key.display.clone(), parsed);
            }
            continue;
        }
        let mut problem: Option<String> = None;
        let mut fail = |message: String| problem = Some(message);
        let text = text_of(key);
        match key.path {
            "server.base_path" => {
                if !valid_base_path(text) {
                    fail(format!(
                        "expected \"/\" or a path that starts and ends with \"/\", with segments of 1 to 64 characters from A-Z a-z 0-9 . _ ~ -, not \".\" or \"..\", found {}",
                        quote(text)
                    ));
                }
            }
            "storage.volume.path" => {
                if !canonical_path(text) {
                    fail(format!(
                        "expected an absolute canonical path with no \".\", \"..\" or empty segment and no last \"/\", found {}",
                        quote(text)
                    ));
                }
            }
            "access.identity.header" => {
                if !valid_header_token(text) {
                    fail(format!(
                        "expected a header name of 1 to 64 token characters (RFC 9110), found {}",
                        quote(text)
                    ));
                } else if FORBIDDEN_HEADERS.contains(&text.to_ascii_lowercase().as_str()) {
                    fail(format!(
                        "expected a header name other than Authorization, Proxy-Authorization, Cookie, Host, Content-Type and Content-Length, found {}",
                        quote(text)
                    ));
                }
            }
            "access.identity.public_keys" => match parse_public_key_set(text) {
                Ok(keys) => {
                    stage.public_keys = keys.iter().map(|k| k.info.clone()).collect();
                    stage.key_set = keys;
                }
                Err(message) => fail(message),
            },
            "access.identity.max_age"
            | "access.identity.clock_skew"
            | "clickhouse.max_execution_time"
            | "postgres.max_execution_time"
            | "mongodb.max_execution_time" => {
                if let Value::DurationMs(ms) = key.value
                    && ms % 1000 != 0
                {
                    fail(format!(
                        "expected a whole number of seconds, found {}",
                        quote(&hclconf::value::format_duration(ms))
                    ));
                }
            }
            "telemetry.otlp_endpoint" | "clickhouse.url" | "access.pdp.url" => {
                let allow_path = key.path == "access.pdp.url";
                match parse_http_url(text, allow_path) {
                    Some(parsed) => {
                        stage.urls.insert(key.display.clone(), parsed);
                    }
                    None => fail(format!("expected {}, found {}", rule_text(allow_path), quote(text))),
                }
            }
            "clickhouse.label" | "postgres.label" | "mongodb.label" => {
                let chars = text.chars().count();
                if !(1..=60).contains(&chars) {
                    fail(format!(
                        "expected a text of 1 to 60 characters with no control character, found {}",
                        quote(text)
                    ));
                }
            }
            "clickhouse.database" => {
                if !valid_identifier(text) {
                    fail(format!(
                        "expected a ClickHouse identifier that matches ^[A-Za-z_][A-Za-z0-9_]{{0,63}}$, found {}",
                        quote(text)
                    ));
                }
            }
            "postgres.search_path" => {
                if !valid_search_path(text) {
                    fail(format!(
                        "expected 1 to 8 names separated by \",\" with no blank, each one matching ^[a-z_][a-z0-9_]{{0,62}}$, none starting with pg_temp and none repeated, found {}",
                        quote(text)
                    ));
                }
            }
            "mongodb.auth_database" => {
                if !valid_mongo_database(text) {
                    fail(format!("expected a text that matches ^[A-Za-z0-9_-]{{1,63}}$, found {}", quote(text)));
                }
            }
            "mongodb.replica_set" => {
                if !valid_replica_set(text) {
                    fail(format!(
                        "expected a text that matches ^[A-Za-z0-9][A-Za-z0-9_.-]{{0,63}}$, found {}",
                        quote(text)
                    ));
                }
            }
            "mongodb.databases" | "mongodb.collections" | "mongodb.hosts" => {
                if let Some(message) = check_mongo_list(key) {
                    fail(message);
                }
            }
            "clickhouse.adhoc_columns" => {
                if let Some(message) = check_adhoc_columns(key) {
                    fail(message);
                }
            }
            "geomap.tile_url" => match super::geomap::parse_tile_url(text) {
                Ok(tile) => stage.geomap_tile = Some(tile),
                Err(reason) => {
                    fail(format!("expected {} ({reason}), found {}", super::geomap::TILE_URL_RULE, quote(text)))
                }
            },
            "geomap.attribution" => {
                if let Err(reason) = super::geomap::check_attribution(text) {
                    fail(format!("expected {} ({reason}), found {}", super::geomap::ATTRIBUTION_RULE, quote(text)));
                }
            }
            "telemetry.ca_file"
            | "access.pdp.ca_file"
            | "storage.postgres.tls.ca_file"
            | "clickhouse.tls.ca_file"
            | "postgres.tls.ca_file"
            | "mongodb.tls.ca_file" => match read_ca_file(text) {
                Ok(bytes) => {
                    stage.ca.insert(key.display.clone(), bytes);
                }
                Err(message) => fail(message),
            },
            _ => {}
        }
        if let Some(message) = problem {
            errors.push(ConfigError::at(key.display.clone(), message, key.line));
        }
    }
    // A build without the MongoDB driver refuses each `mongodb` block (8b.2). A mode that reads only
    // the labels and the kinds of the blocks needs no driver.
    if mode.checks_driver() && !super::has_mongo_driver() {
        for block in checked.blocks().iter().filter(|b| b.path == "mongodb") {
            errors.push(ConfigError::at(block.display.clone(), "this build has no MongoDB driver", block.line));
        }
    }
    let label_blocks = checked
        .blocks()
        .iter()
        .filter(|b| matches!(b.path, "clickhouse" | "postgres" | "mongodb") && mode.checks_labels());
    for block in label_blocks {
        if let Some(label) = &block.label
            && !valid_label(label)
        {
            errors.push(ConfigError::at(
                block.display.clone(),
                format!("expected a label that matches ^[a-z0-9][a-z0-9-]{{0,39}}$, found {}", quote(label)),
                block.line,
            ));
        }
    }
    if errors.is_empty() && mode == ReadMode::All {
        errors.extend(pep_errors(checked, &stage));
    }
    if errors.is_empty() {
        Ok(stage)
    } else {
        errors.sort_by_key(|e| e.line.unwrap_or(0));
        Err(hclconf::error::limit_errors(errors))
    }
}

fn text_key(checked: &Checked, path: &str) -> Option<String> {
    match &checked.get(path)?.value {
        Value::Str(s) => Some(s.clone()),
        _ => None,
    }
}

fn duration_key(checked: &Checked, path: &str) -> Option<std::time::Duration> {
    match checked.get(path)?.value {
        Value::DurationMs(ms) => Some(std::time::Duration::from_millis(ms)),
        _ => None,
    }
}

fn int_key(checked: &Checked, path: &str) -> Option<u64> {
    match checked.get(path)?.value {
        Value::Int(n) => Some(n),
        _ => None,
    }
}

/// The values that the PEP needs, read from the checked keys. It gives `None` in the mode `open`.
fn pep_inputs(checked: &Checked, stage: &Stage2) -> Option<PepInputs> {
    if text_key(checked, "access.mode")? != "authzen" {
        return None;
    }
    let mut sources = Vec::new();
    for label in checked.labels("clickhouse") {
        let get_int = |p: &str| match checked.get_in(label, p)?.value {
            Value::Int(n) => Some(n),
            _ => None,
        };
        let seconds = match checked.get_in(label, "clickhouse.max_execution_time")?.value {
            Value::DurationMs(ms) => ms / 1000,
            _ => return None,
        };
        let id_label = match &checked.get_in(label, "clickhouse.label")?.value {
            Value::Str(s) => s.clone(),
            _ => return None,
        };
        sources.push(SourceInfo {
            kind: hune_source::SourceKind::ClickHouse,
            id: label.to_string(),
            label: id_label,
            max_rows: get_int("clickhouse.max_result_rows")?,
            max_seconds: seconds,
        });
    }
    for label in checked.labels("postgres") {
        let get_int = |p: &str| match checked.get_in(label, p)?.value {
            Value::Int(n) => Some(n),
            _ => None,
        };
        let seconds = match checked.get_in(label, "postgres.max_execution_time")?.value {
            Value::DurationMs(ms) => ms / 1000,
            _ => return None,
        };
        let id_label = match &checked.get_in(label, "postgres.label")?.value {
            Value::Str(s) => s.clone(),
            _ => return None,
        };
        sources.push(SourceInfo {
            kind: hune_source::SourceKind::Postgres,
            id: label.to_string(),
            label: id_label,
            max_rows: get_int("postgres.max_result_rows")?,
            max_seconds: seconds,
        });
    }
    for label in checked.labels("mongodb") {
        let get_int = |p: &str| match checked.get_in(label, p)?.value {
            Value::Int(n) => Some(n),
            _ => None,
        };
        let seconds = match checked.get_in(label, "mongodb.max_execution_time")?.value {
            Value::DurationMs(ms) => ms / 1000,
            _ => return None,
        };
        let id_label = match &checked.get_in(label, "mongodb.label")?.value {
            Value::Str(s) => s.clone(),
            _ => return None,
        };
        sources.push(SourceInfo {
            kind: hune_source::SourceKind::MongoDb,
            id: label.to_string(),
            label: id_label,
            max_rows: get_int("mongodb.max_result_rows")?,
            max_seconds: seconds,
        });
    }
    Some(PepInputs {
        audience: text_key(checked, "access.identity.audience")?,
        header: text_key(checked, "access.identity.header")?,
        issuer: text_key(checked, "access.identity.issuer")?,
        keys: stage.key_set.clone(),
        max_age_s: duration_key(checked, "access.identity.max_age")?.as_secs(),
        clock_skew_s: duration_key(checked, "access.identity.clock_skew")?.as_secs(),
        pdp_url: text_key(checked, "access.pdp.url")?,
        pdp_token: procurauth_pep::config::Secret::new(text_key(checked, "access.pdp.token")?),
        plain_http: matches!(checked.get("access.pdp.plain_http")?.value, Value::Bool(true)),
        timeout: duration_key(checked, "access.pdp.timeout")?,
        cache_ttl: duration_key(checked, "access.pdp.cache_ttl")?,
        max_connections: usize::try_from(int_key(checked, "access.pdp.max_connections")?).ok()?,
        editing: matches!(checked.get("server.editing")?.value, Value::Bool(true)),
        sources,
    })
}

/// In the mode `authzen`, builds the `PepConfig` and the action document and checks them (13.13).
/// Neither call uses the network. Each error has the key of 13.11 that gives the field (13.14).
fn pep_errors(checked: &Checked, stage: &Stage2) -> Vec<ConfigError> {
    // C15 (stage 3) reports a label that two blocks share. The action document would find it first.
    let mut ids: Vec<&str> = checked.labels("clickhouse");
    ids.extend(checked.labels("postgres"));
    ids.extend(checked.labels("mongodb"));
    if (1..ids.len()).any(|i| ids[..i].contains(&ids[i])) {
        return Vec::new();
    }
    let Some(inputs) = pep_inputs(checked, stage) else { return Vec::new() };
    inputs
        .check()
        .into_iter()
        .map(|(key, message)| {
            // The key of a source error is `<kind> "<id>".label`: find the line of that key.
            let source_key = ["clickhouse", "postgres", "mongodb"].iter().find_map(|kind| {
                let label = key.strip_prefix(&format!("{kind} \""))?.split_once('"')?.0;
                checked.get_in(label, &format!("{kind}.label"))
            });
            let (display, line) = match source_key {
                Some(k) => (k.display.clone(), Some(k.line)),
                None => match checked.get(&key) {
                    Some(k) => (k.display.clone(), Some(k.line)),
                    None => (key.clone(), None),
                },
            };
            ConfigError::new(display, message, line)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_path_rule() {
        for ok in ["/", "/dashboards/", "/a/b/", "/a.b_c~d-e/", &format!("/{}/", "a".repeat(64))] {
            assert!(valid_base_path(ok), "{ok}");
        }
        for bad in [
            "",
            "dashboards/",
            "/dashboards",
            "//",
            "/a//b/",
            "/./",
            "/../",
            "/a b/",
            "/a%20/",
            "/é/",
            &format!("/{}/", "a".repeat(65)),
            &format!("/{}/", "a/".repeat(64)),
        ] {
            assert!(!valid_base_path(bad), "{bad}");
        }
    }

    #[test]
    fn label_rule() {
        for ok in ["prod", "0", "a-b", "a".repeat(40).as_str()] {
            assert!(valid_label(ok), "{ok}");
        }
        for bad in ["", "Prod", "-a", "a_b", "a b", "a".repeat(41).as_str(), "é"] {
            assert!(!valid_label(bad), "{bad}");
        }
    }

    #[test]
    fn identifier_and_header_rules() {
        assert!(valid_identifier("analytics"));
        assert!(valid_identifier("_a1"));
        assert!(!valid_identifier("1a"));
        assert!(!valid_identifier("a-b"));
        assert!(!valid_identifier(&"a".repeat(65)));
        assert!(valid_header_token("X-Identity-Token"));
        assert!(!valid_header_token("X Identity"));
        assert!(!valid_header_token("X:Identity"));
        assert!(!valid_header_token(&"a".repeat(65)));
    }

    #[test]
    fn canonical_paths() {
        assert!(canonical_path("/var/lib/hune"));
        assert!(canonical_path("/"));
        for bad in ["/var/", "/var//lib", "/var/./lib", "/var/../lib", "var"] {
            assert!(!canonical_path(bad), "{bad}");
        }
    }
}
