// SPDX-License-Identifier: MIT OR Apache-2.0
//! The startup packet of a connection (8a.4).
//!
//! This module builds the `tokio-postgres` configuration from the keys of the block only: no `PG*`
//! variable, no password file, no service file, no Unix socket, no second host. It sends the
//! startup parameters of the table of 8a.4 in the `options` parameter, as `-c name=value` items.
//! The crate itself sends `client_encoding = UTF8`.
//!
//! Each value is a constant, a key, or a value that this module checks again (the binary checked
//! it when it read the file). A value that holds a blank, a quote mark or a backslash is refused,
//! so no value can add an item to `options`. An error holds the key name and the rule, never the
//! password.
//!
//! Not in this module: the TLS connector and the check of the name (they need the resolved
//! address, see the session), the cancel request, and the total time of the setup. The
//! `connect_timeout` of the configuration bounds only the TCP connect of one address, so the
//! caller MUST wrap the whole setup in a timeout of the same value (8a.4).

use std::time::Duration;

use tokio_postgres::Config;
use tokio_postgres::config::{ChannelBinding, LoadBalanceHosts, SslMode, TargetSessionAttrs};

use crate::config::{PostgresSourceConfig, PostgresSslMode};

/// The value of `application_name` at the start. Each query sets a longer name (8a.11).
pub(crate) const APPLICATION_NAME: &str = "hune";
/// The value of `lock_timeout` at the start, in milliseconds.
pub(crate) const LOCK_TIMEOUT_MS: u64 = 5_000;
/// The value of `idle_in_transaction_session_timeout`, in milliseconds.
pub(crate) const IDLE_IN_TRANSACTION_TIMEOUT_MS: u64 = 15_000;
/// The value of `client_connection_check_interval`, in milliseconds.
pub(crate) const CLIENT_CONNECTION_CHECK_INTERVAL_MS: u64 = 1_000;
/// The idle time of TCP keepalive (8a.4).
pub(crate) const KEEPALIVE_IDLE: Duration = Duration::from_secs(30);

/// A key of the block that cannot go in the startup packet. The text names the key and the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartupError {
    message: String,
}

impl StartupError {
    fn new(key: &str, rule: &str) -> StartupError {
        StartupError { message: format!("The key `{key}` {rule}.") }
    }

    /// Gives the message.
    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Tells if `name` matches `^[a-z_][a-z0-9_]{0,62}$` and does not start with `pg_temp` (13.12a).
fn is_search_path_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 63
        && (b[0].is_ascii_lowercase() || b[0] == b'_')
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
        && !name.starts_with("pg_temp")
}

/// Gives the value of the session `search_path`: 1 to 8 names separated by `,` (8a.4).
///
/// # Errors
///
/// The list is empty, it has more than 8 names, or a name breaks the rule of 13.12a.
pub(crate) fn search_path_value(names: &[String]) -> Result<String, StartupError> {
    if names.is_empty() || names.len() > 8 {
        return Err(StartupError::new("search_path", "must have 1 to 8 names"));
    }
    if let Some(bad) = names.iter().position(|n| !is_search_path_name(n)) {
        return Err(StartupError::new(
            "search_path",
            &format!("has a name that does not match ^[a-z_][a-z0-9_]{{0,62}}$ (name {})", bad + 1),
        ));
    }
    Ok(names.join(","))
}

/// Tells if `host` is a DNS name or an IP address. A path, a list and a blank are refused.
fn is_plain_host(host: &str) -> bool {
    if host.contains(':') {
        // Only an IPv6 address has a colon: `host:port` is not a host.
        return host.parse::<std::net::Ipv6Addr>().is_ok();
    }
    !host.is_empty()
        && host.len() <= 253
        && host.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// The startup parameters of the table of 8a.4, in the order of the table, without
/// `client_encoding` and `application_name` (the client sends them as separate parameters).
///
/// # Errors
///
/// `search_path` breaks its rule.
pub(crate) fn startup_params(cfg: &PostgresSourceConfig) -> Result<Vec<(&'static str, String)>, StartupError> {
    let statement_timeout = u64::try_from(cfg.max_execution_time.as_millis()).unwrap_or(u64::MAX);
    Ok(vec![
        ("default_transaction_read_only", "on".to_owned()),
        ("standard_conforming_strings", "on".to_owned()),
        ("TimeZone", "UTC".to_owned()),
        ("DateStyle", "ISO,YMD".to_owned()),
        ("search_path", search_path_value(&cfg.search_path)?),
        ("statement_timeout", statement_timeout.to_string()),
        ("lock_timeout", LOCK_TIMEOUT_MS.to_string()),
        ("idle_in_transaction_session_timeout", IDLE_IN_TRANSACTION_TIMEOUT_MS.to_string()),
        ("client_connection_check_interval", CLIENT_CONNECTION_CHECK_INTERVAL_MS.to_string()),
    ])
}

/// The value of the `options` parameter: one `-c name=value` item for each startup parameter.
///
/// # Errors
///
/// `search_path` breaks its rule, or a value holds a blank, a quote mark or a backslash.
pub(crate) fn options_text(cfg: &PostgresSourceConfig) -> Result<String, StartupError> {
    let mut out = String::new();
    for (name, value) in startup_params(cfg)? {
        if value.is_empty() || value.bytes().any(|b| b.is_ascii_whitespace() || matches!(b, b'\'' | b'"' | b'\\' | 0)) {
            return Err(StartupError::new(name, "has a value with a blank, a quote mark or a backslash"));
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str("-c ");
        out.push_str(name);
        out.push('=');
        out.push_str(&value);
    }
    Ok(out)
}

/// The values that the setup check R0 reads back (8a.12): each one MUST equal the startup value.
///
/// The check also reads `server_version_num`, which is not a startup parameter.
///
/// # Errors
///
/// `search_path` breaks its rule.
#[allow(dead_code, reason = "The container test of the startup reads it; R0 compares the values by `probe::check_r0`.")]
pub(crate) fn expected_session_values(cfg: &PostgresSourceConfig) -> Result<Vec<(&'static str, String)>, StartupError> {
    Ok(vec![
        ("default_transaction_read_only", "on".to_owned()),
        ("standard_conforming_strings", "on".to_owned()),
        ("client_encoding", "UTF8".to_owned()),
        ("TimeZone", "UTC".to_owned()),
        ("search_path", search_path_value(&cfg.search_path)?),
    ])
}

/// Builds the `tokio-postgres` configuration of a connection (8a.4).
///
/// The configuration has one host, no Unix socket, TCP keepalive with an idle time of 30 s, and
/// the startup parameters of [`startup_params`]. With `verify-full` it requires TLS and channel
/// binding. The caller adds the TLS connector (see the module comment).
///
/// # Errors
///
/// The host is not a DNS name or an IP address, a name is empty, or `search_path` breaks its rule.
pub(crate) fn connection_config(cfg: &PostgresSourceConfig) -> Result<Config, StartupError> {
    if !is_plain_host(&cfg.host) {
        return Err(StartupError::new("host", "must be one DNS name or one IP address, with no path and no list"));
    }
    if cfg.database.is_empty() || cfg.database.contains('\0') {
        return Err(StartupError::new("database", "must be a text with no NUL byte"));
    }
    if cfg.user.is_empty() || cfg.user.contains('\0') {
        return Err(StartupError::new("user", "must be a text with no NUL byte"));
    }
    if cfg.password.contains('\0') {
        return Err(StartupError::new("password", "must be a text with no NUL byte"));
    }
    let mut c = Config::new();
    c.host(cfg.host.as_str());
    c.port(cfg.port);
    c.user(cfg.user.as_str());
    c.password(cfg.password.as_bytes());
    c.dbname(cfg.database.as_str());
    c.application_name(APPLICATION_NAME);
    c.options(options_text(cfg)?);
    c.connect_timeout(cfg.connect_timeout);
    c.keepalives(true);
    c.keepalives_idle(KEEPALIVE_IDLE);
    c.target_session_attrs(TargetSessionAttrs::Any);
    c.load_balance_hosts(LoadBalanceHosts::Disable);
    match cfg.sslmode {
        PostgresSslMode::VerifyFull => {
            c.ssl_mode(SslMode::Require);
            c.channel_binding(ChannelBinding::Require);
        }
        PostgresSslMode::Disable => {
            c.ssl_mode(SslMode::Disable);
        }
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SourceTls;
    use hune_core::ids::SourceId;

    fn cfg() -> PostgresSourceConfig {
        PostgresSourceConfig {
            id: SourceId::parse("metrics").expect("valid id"),
            label: "Metrics".to_owned(),
            host: "pg.internal".to_owned(),
            port: 5432,
            database: "analytics".to_owned(),
            user: "hune_ro".to_owned(),
            password: "s3cret".to_owned(),
            search_path: vec!["metrics".to_owned(), "public".to_owned()],
            sslmode: PostgresSslMode::VerifyFull,
            tls: Some(SourceTls { ca_pem: Vec::new(), server_name: "pg.internal".to_owned() }),
            connect_timeout: Duration::from_secs(5),
            max_execution_time: Duration::from_secs(60),
            max_result_rows: 100_000,
            max_result_bytes: 33_554_432,
            max_query_size: 1_048_576,
            max_concurrent: 16,
            queue_wait: Duration::from_secs(5),
            max_concurrent_per_subject: None,
        }
    }

    #[test]
    fn options_have_the_table_of_8a4() {
        assert_eq!(
            options_text(&cfg()).unwrap(),
            "-c default_transaction_read_only=on -c standard_conforming_strings=on -c TimeZone=UTC \
-c DateStyle=ISO,YMD -c search_path=metrics,public -c statement_timeout=60000 -c lock_timeout=5000 \
-c idle_in_transaction_session_timeout=15000 -c client_connection_check_interval=1000"
        );
        let names: Vec<_> = startup_params(&cfg()).unwrap().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names.len(), 9);
        // The two parameters that the client sends apart are not in the options.
        assert!(!options_text(&cfg()).unwrap().contains("client_encoding"));
        assert!(!options_text(&cfg()).unwrap().contains("application_name"));
    }

    #[test]
    fn statement_timeout_follows_the_key() {
        let mut c = cfg();
        c.max_execution_time = Duration::from_secs(3);
        assert!(options_text(&c).unwrap().contains("-c statement_timeout=3000 "));
    }

    #[test]
    fn config_is_closed() {
        let c = connection_config(&cfg()).unwrap();
        assert_eq!(c.get_hosts().len(), 1);
        assert!(matches!(&c.get_hosts()[0], tokio_postgres::config::Host::Tcp(h) if h == "pg.internal"));
        assert_eq!(c.get_ports(), &[5432]);
        assert_eq!(c.get_user(), Some("hune_ro"));
        assert_eq!(c.get_dbname(), Some("analytics"));
        assert_eq!(c.get_application_name(), Some("hune"));
        assert_eq!(c.get_ssl_mode(), SslMode::Require);
        assert_eq!(c.get_channel_binding(), ChannelBinding::Require);
        assert_eq!(c.get_connect_timeout(), Some(&Duration::from_secs(5)));
        assert!(c.get_keepalives());
        assert_eq!(c.get_keepalives_idle(), Duration::from_secs(30));
        assert_eq!(c.get_target_session_attrs(), TargetSessionAttrs::Any);
        assert!(c.get_options().unwrap().contains("default_transaction_read_only=on"));
        let mut d = cfg();
        d.sslmode = PostgresSslMode::Disable;
        d.tls = None;
        let c = connection_config(&d).unwrap();
        assert_eq!(c.get_ssl_mode(), SslMode::Disable);
    }

    #[test]
    fn host_never_opens_a_socket_path_or_a_list() {
        for host in ["/var/run/postgresql", "a,b", "h:5432", "", "host name", "h/../x", "[::1]", "a@b"] {
            let mut c = cfg();
            c.host = host.to_owned();
            let e = connection_config(&c).unwrap_err();
            assert!(e.message().contains("`host`"), "{host}");
        }
        for host in ["127.0.0.1", "::1", "pg-1.example.org", "db_1"] {
            let mut c = cfg();
            c.host = host.to_owned();
            connection_config(&c).unwrap();
        }
    }

    #[test]
    fn search_path_rule() {
        let names = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(search_path_value(&names(&["a", "_b", "c9"])).unwrap(), "a,_b,c9");
        for bad in [
            names(&[]),
            names(&["a b"]),
            names(&["A"]),
            names(&["9a"]),
            names(&["a;b"]),
            names(&["a'b"]),
            names(&["a\\b"]),
            names(&["pg_temp"]),
            names(&["pg_temp_3"]),
            names(&["\"x\""]),
            names(&[""]),
            names(&["a", "b", "c", "d", "e", "f", "g", "h", "i"]),
            vec!["a".repeat(64)],
        ] {
            assert!(search_path_value(&bad).is_err(), "{bad:?}");
        }
        assert!(search_path_value(&["a".repeat(63)]).is_ok());
        let mut c = cfg();
        c.search_path = names(&["x y"]);
        assert!(connection_config(&c).is_err());
    }

    #[test]
    fn error_never_holds_the_password() {
        let mut c = cfg();
        c.host = "bad host".into();
        let e = connection_config(&c).unwrap_err();
        assert!(!e.message().contains("s3cret"));
        let mut c = cfg();
        c.password = "a\0b".into();
        let e = connection_config(&c).unwrap_err();
        assert_eq!(e.message(), "The key `password` must be a text with no NUL byte.");
    }

    #[test]
    fn expected_values_for_r0() {
        let v = expected_session_values(&cfg()).unwrap();
        assert!(v.contains(&("default_transaction_read_only", "on".to_owned())));
        assert!(v.contains(&("client_encoding", "UTF8".to_owned())));
        assert!(v.contains(&("search_path", "metrics,public".to_owned())));
    }
}

#[cfg(test)]
#[path = "startup/tests/container_tests.rs"]
mod container_tests;
