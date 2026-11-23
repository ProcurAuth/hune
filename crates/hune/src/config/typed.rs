// SPDX-License-Identifier: MIT OR Apache-2.0
//! Build the typed `Config` from the checked keys (13.2, rule 4: typed once).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use hclconf::{Checked, ConfigError, ResolvedKey, Secret, Value};
use hune_core::ids::SourceId;
use hune_core::limits::{MAX_REPEAT_PER_DASHBOARD, MAX_REPEAT_PER_PANEL, MAX_REPEAT_PER_SECTION};
use hune_core::mongo::ReadPreference;
use hune_source::{
    MongoHost, MongoSourceConfig, MongoTlsMode, MongoTopology, PostgresSourceConfig, PostgresSslMode, SourceTls,
};

use super::stage2::Stage2;
use super::url::parse_http_url;
use super::*;

/// Reads the keys of one scope: the top, or one `clickhouse`, `postgres` or `mongodb` block.
struct Reader<'a> {
    checked: &'a Checked,
    label: Option<&'a str>,
    /// True for a partial read mode: a value that was not read gets a placeholder.
    partial: bool,
}

fn quote(text: &str) -> String {
    hclconf::value::json_string(text)
}

fn internal(path: &str, what: &str) -> ConfigError {
    ConfigError::new(path, format!("internal error: {what}"), None)
}

impl Reader<'_> {
    fn opt(&self, path: &str) -> Option<&ResolvedKey> {
        match self.label {
            Some(label) => self.checked.get_in(label, path),
            None => self.checked.get(path),
        }
    }

    fn key(&self, path: &str) -> Result<&ResolvedKey, ConfigError> {
        self.opt(path).ok_or_else(|| internal(path, "a checked key is absent"))
    }

    fn text(&self, path: &str) -> Result<String, ConfigError> {
        match &self.key(path)?.value {
            Value::Str(s) => Ok(s.clone()),
            // The placeholder of a value that the read mode did not read (13.18).
            Value::NotRead => Ok(String::new()),
            _ => Err(internal(path, "a text was expected")),
        }
    }

    fn opt_text(&self, path: &str) -> Result<Option<String>, ConfigError> {
        self.opt(path).map(|_| self.text(path)).transpose()
    }

    fn int(&self, path: &str) -> Result<u64, ConfigError> {
        match self.key(path)?.value {
            Value::Int(n) => Ok(n),
            _ => Err(internal(path, "an integer was expected")),
        }
    }

    fn int32(&self, path: &str) -> Result<u32, ConfigError> {
        u32::try_from(self.int(path)?).map_err(|_| internal(path, "an integer of 32 bits was expected"))
    }

    fn int16(&self, path: &str) -> Result<u16, ConfigError> {
        u16::try_from(self.int(path)?).map_err(|_| internal(path, "an integer of 16 bits was expected"))
    }

    fn boolean(&self, path: &str) -> Result<bool, ConfigError> {
        match self.key(path)?.value {
            Value::Bool(b) => Ok(b),
            _ => Err(internal(path, "a boolean was expected")),
        }
    }

    /// A boolean key that can be absent: `None` when the file does not set it.
    fn opt_int32(&self, path: &str) -> Result<Option<u32>, ConfigError> {
        self.opt(path).map(|_| self.int32(path)).transpose()
    }

    fn opt_boolean(&self, path: &str) -> Result<Option<bool>, ConfigError> {
        self.opt(path).map(|_| self.boolean(path)).transpose()
    }

    fn duration(&self, path: &str) -> Result<Duration, ConfigError> {
        match self.key(path)?.value {
            Value::DurationMs(ms) => Ok(Duration::from_millis(ms)),
            _ => Err(internal(path, "a duration was expected")),
        }
    }

    fn secret(&self, path: &str) -> Result<Secret, ConfigError> {
        Ok(Secret::new(self.text(path)?))
    }

    /// The items of a list of texts. A value that the read mode did not read is an empty list.
    fn text_list(&self, path: &str) -> Result<Vec<String>, ConfigError> {
        match &self.key(path)?.value {
            Value::List(items) => items
                .iter()
                .map(|item| match item {
                    Value::Str(s) => Ok(s.clone()),
                    _ => Err(internal(path, "a list of texts was expected")),
                })
                .collect(),
            Value::NotRead => Ok(Vec::new()),
            _ => Err(internal(path, "a list of texts was expected")),
        }
    }

    fn socket(&self, path: &str) -> Result<SocketAddr, ConfigError> {
        self.text(path)?.parse().map_err(|_| internal(path, "a socket address was expected"))
    }

    fn path(&self, path: &str) -> Result<PathBuf, ConfigError> {
        Ok(PathBuf::from(self.text(path)?))
    }

    /// The CA bytes that stage 2 read for a key. A partial read mode may skip the read:
    /// the result is then `None`.
    fn ca_pem(&self, path: &str, stage: &Stage2) -> Result<Option<Vec<u8>>, ConfigError> {
        let key = self.key(path)?;
        match stage.ca.get(&key.display) {
            Some(bytes) => Ok(Some(bytes.clone())),
            None if self.partial => Ok(None),
            None => Err(internal(path, "the CA file was not read")),
        }
    }

    fn tls(&self, block: &str, stage: &Stage2) -> Result<Option<TlsConfig>, ConfigError> {
        let present = match self.label {
            Some(label) => self.checked.block_in(label, block).is_some(),
            None => self.checked.has_block(block),
        };
        if !present {
            return Ok(None);
        }
        Ok(Some(TlsConfig {
            ca_file: self.path(&format!("{block}.ca_file"))?,
            ca_pem: self.ca_pem(&format!("{block}.ca_file"), stage)?.unwrap_or_default(),
            server_name: self.text(&format!("{block}.server_name"))?,
        }))
    }
}

fn http_url(r: &Reader<'_>, path: &str) -> Result<HttpUrl, ConfigError> {
    let text = r.text(path)?;
    parse_http_url(&text, false).map(|(u, _)| u).ok_or_else(|| internal(path, "a checked URL does not parse"))
}

/// A default of a repeat key as the type of the field. The constants are below 2^32.
fn repeat_default(limit: usize) -> u32 {
    u32::try_from(limit).unwrap_or(u32::MAX)
}

/// Build the typed configuration. The input has passed stages 1 to 3. With `partial`, a value
/// that the read mode did not read gets a placeholder (see `Config`).
pub fn build(checked: &Checked, stage: &Stage2, partial: bool) -> Result<Config, Vec<ConfigError>> {
    build_inner(checked, stage, partial).map_err(|e| vec![e])
}

fn build_inner(checked: &Checked, stage: &Stage2, partial: bool) -> Result<Config, ConfigError> {
    let r = Reader { checked, label: None, partial };

    let server = ServerConfig {
        listen: r.socket("server.listen")?,
        ops_listen: r.socket("server.ops_listen")?,
        base_path: r.text("server.base_path")?,
        editing: r.boolean("server.editing")?,
        query_inspect: r.opt_boolean("server.query_inspect")?.unwrap_or(false),
        repeat_max_per_panel: r
            .opt_int32("server.repeat_max_per_panel")?
            .unwrap_or(repeat_default(MAX_REPEAT_PER_PANEL)),
        repeat_max_per_section: r
            .opt_int32("server.repeat_max_per_section")?
            .unwrap_or(repeat_default(MAX_REPEAT_PER_SECTION)),
        repeat_max_per_dashboard: r
            .opt_int32("server.repeat_max_per_dashboard")?
            .unwrap_or(repeat_default(MAX_REPEAT_PER_DASHBOARD)),
        compress_static: r.opt_boolean("server.compress_static")?.unwrap_or(true),
        transform_max_cells: r
            .opt_int32("server.transform_max_cells")?
            .unwrap_or(super::keys::TRANSFORM_MAX_CELLS_DEFAULT),
        max_connections: r.int32("server.max_connections")?,
        idle_timeout: r.duration("server.idle_timeout")?,
        shutdown_grace: r.duration("server.shutdown_grace")?,
        max_request_bytes: r.int("server.max_request_bytes")?,
        max_result_memory_bytes: r.int("server.max_result_memory_bytes")?,
    };

    let level = match r.text("log.level")?.as_str() {
        "error" => LogLevel::Error,
        "warn" => LogLevel::Warn,
        "info" => LogLevel::Info,
        "debug" => LogLevel::Debug,
        _ => return Err(internal("log.level", "a log level was expected")),
    };

    let telemetry = if r.boolean("telemetry.enabled")? {
        TelemetryConfig::Enabled(TelemetryEnabled {
            otlp_endpoint: r.text("telemetry.otlp_endpoint")?,
            ca_file: r.opt("telemetry.ca_file").map(|_| r.path("telemetry.ca_file")).transpose()?,
            ca_pem: r.opt("telemetry.ca_file").map(|_| r.ca_pem("telemetry.ca_file", stage)).transpose()?.flatten(),
            service_name: r.text("telemetry.service_name")?,
            export_interval: r.duration("telemetry.export_interval")?,
        })
    } else {
        TelemetryConfig::Disabled
    };

    let storage = match r.text("storage.backend")?.as_str() {
        "volume" => StorageConfig::Volume(VolumeConfig {
            path: r.path("storage.volume.path")?,
            read_only: r.boolean("storage.volume.read_only")?,
            rescan_interval: r.duration("storage.volume.rescan_interval")?,
            history_keep: r.int32("storage.volume.history_keep")?,
        }),
        "postgres" => {
            let sslmode = match r.text("storage.postgres.sslmode")?.as_str() {
                "verify-full" => SslMode::VerifyFull,
                "disable" => SslMode::Disable,
                _ => return Err(internal("storage.postgres.sslmode", "an sslmode was expected")),
            };
            StorageConfig::Postgres(PostgresConfig {
                host: r.text("storage.postgres.host")?,
                port: r.int16("storage.postgres.port")?,
                database: r.text("storage.postgres.database")?,
                user: r.text("storage.postgres.user")?,
                password: r.secret("storage.postgres.password")?,
                sslmode,
                tls: r.tls("storage.postgres.tls", stage)?,
                max_connections: r.int32("storage.postgres.max_connections")?,
                connect_timeout: r.duration("storage.postgres.connect_timeout")?,
                statement_timeout: r.duration("storage.postgres.statement_timeout")?,
                poll_interval: r.duration("storage.postgres.poll_interval")?,
                history_keep: r.int32("storage.postgres.history_keep")?,
            })
        }
        _ => return Err(internal("storage.backend", "a backend was expected")),
    };

    let authzen = r.text("access.mode")? == "authzen";
    let access = if authzen {
        let pdp_url = r.text("access.pdp.url")?;
        AccessConfig::Authzen(AuthzenConfig {
            identity: IdentityConfig {
                header: r.text("access.identity.header")?,
                issuer: r.text("access.identity.issuer")?,
                audience: r.text("access.identity.audience")?,
                public_keys: r.text("access.identity.public_keys")?,
                max_age: r.duration("access.identity.max_age")?,
                clock_skew: r.duration("access.identity.clock_skew")?,
            },
            pdp: PdpConfig {
                url: pdp_url,
                token: r.secret("access.pdp.token")?,
                plain_http: r.boolean("access.pdp.plain_http")?,
                ca_file: r.opt("access.pdp.ca_file").map(|_| r.path("access.pdp.ca_file")).transpose()?,
                ca_pem: r
                    .opt("access.pdp.ca_file")
                    .map(|_| r.ca_pem("access.pdp.ca_file", stage))
                    .transpose()?
                    .flatten(),
                timeout: r.duration("access.pdp.timeout")?,
                cache_ttl: r.duration("access.pdp.cache_ttl")?,
                max_connections: r.int32("access.pdp.max_connections")?,
            },
        })
    } else {
        AccessConfig::Open
    };

    let mut clickhouse = Vec::new();
    for label in checked.labels("clickhouse") {
        let c = Reader { checked, label: Some(label), partial };
        clickhouse.push(ClickHouseConfig {
            id: label.to_string(),
            label: c.text("clickhouse.label")?,
            url: http_url(&c, "clickhouse.url")?,
            user: c.text("clickhouse.user")?,
            password: c.secret("clickhouse.password")?,
            database: c.text("clickhouse.database")?,
            tls: c.tls("clickhouse.tls", stage)?,
            connect_timeout: c.duration("clickhouse.connect_timeout")?,
            max_execution_time: c.duration("clickhouse.max_execution_time")?,
            max_result_rows: c.int("clickhouse.max_result_rows")?,
            max_result_bytes: c.int("clickhouse.max_result_bytes")?,
            max_query_size: c.int("clickhouse.max_query_size")?,
            max_concurrent: c.int32("clickhouse.max_concurrent")?,
            queue_wait: c.duration("clickhouse.queue_wait")?,
            max_concurrent_per_subject: c
                .opt("clickhouse.max_concurrent_per_subject")
                .map(|_| c.int32("clickhouse.max_concurrent_per_subject"))
                .transpose()?,
            adhoc_columns: c
                .opt("clickhouse.adhoc_columns")
                .map(|_| c.text_list("clickhouse.adhoc_columns"))
                .transpose()?,
        });
    }

    let mut postgres_sources = Vec::new();
    for label in checked.labels("postgres") {
        let p = Reader { checked, label: Some(label), partial };
        let id = match SourceId::parse(label) {
            Ok(id) => id,
            // A partial read mode does not check the labels (13.18), and the server never starts from it.
            Err(_) if partial => continue,
            Err(_) => {
                return Err(internal(&format!("postgres {}", quote(label)), "a checked label is not a source id"));
            }
        };
        let sslmode = match p.text("postgres.sslmode")?.as_str() {
            "verify-full" => PostgresSslMode::VerifyFull,
            "disable" => PostgresSslMode::Disable,
            _ => return Err(internal("postgres.sslmode", "an sslmode was expected")),
        };
        let search_path = p.text("postgres.search_path")?.split(',').map(str::to_owned).collect();
        postgres_sources.push(PostgresSourceConfig {
            id,
            label: p.text("postgres.label")?,
            host: p.text("postgres.host")?,
            port: p.int16("postgres.port")?,
            database: p.text("postgres.database")?,
            user: p.text("postgres.user")?,
            password: p.secret("postgres.password")?.expose().to_owned(),
            search_path,
            sslmode,
            tls: p.tls("postgres.tls", stage)?.map(|t| SourceTls { ca_pem: t.ca_pem, server_name: t.server_name }),
            connect_timeout: p.duration("postgres.connect_timeout")?,
            max_execution_time: p.duration("postgres.max_execution_time")?,
            max_result_rows: p.int("postgres.max_result_rows")?,
            max_result_bytes: p.int("postgres.max_result_bytes")?,
            max_query_size: p.int("postgres.max_query_size")?,
            max_concurrent: p.int32("postgres.max_concurrent")?,
            queue_wait: p.duration("postgres.queue_wait")?,
            max_concurrent_per_subject: p
                .opt("postgres.max_concurrent_per_subject")
                .map(|_| p.int32("postgres.max_concurrent_per_subject"))
                .transpose()?,
        });
    }

    let mut mongo_sources = Vec::new();
    for label in checked.labels("mongodb") {
        let m = Reader { checked, label: Some(label), partial };
        let shown = format!("mongodb {}", quote(label));
        let id = match SourceId::parse(label) {
            Ok(id) => id,
            // A partial read mode does not check the labels (13.18), and the server never starts from it.
            Err(_) if partial => continue,
            Err(_) => return Err(internal(&shown, "a checked label is not a source id")),
        };
        let topology = match m.text("mongodb.topology")?.as_str() {
            "direct" => MongoTopology::Direct { host: m.text("mongodb.host")?, port: m.int16("mongodb.port")? },
            "replica_set" => {
                let mut hosts = Vec::new();
                for item in m.text_list("mongodb.hosts")? {
                    match stage2::parse_host_port(&item) {
                        Some((host, port)) => hosts.push(MongoHost { host, port }),
                        // A partial read mode does not check the items of a list (13.18).
                        None if partial => {}
                        None => return Err(internal("mongodb.hosts", "a checked item is not host:port")),
                    }
                }
                MongoTopology::ReplicaSet { hosts, replica_set: m.text("mongodb.replica_set")? }
            }
            _ => return Err(internal("mongodb.topology", "a topology was expected")),
        };
        let read_preference = match m.text("mongodb.read_preference")?.as_str() {
            "primary" => ReadPreference::Primary,
            "primaryPreferred" => ReadPreference::PrimaryPreferred,
            "secondary" => ReadPreference::Secondary,
            "secondaryPreferred" => ReadPreference::SecondaryPreferred,
            "nearest" => ReadPreference::Nearest,
            _ => return Err(internal("mongodb.read_preference", "a read preference was expected")),
        };
        let tls_mode = match m.text("mongodb.tls_mode")?.as_str() {
            "verify-full" => MongoTlsMode::VerifyFull,
            "disable" => MongoTlsMode::Disable,
            _ => return Err(internal("mongodb.tls_mode", "a tls mode was expected")),
        };
        // The key table has no `tls.server_name` for this kind: the certificate must hold the host
        // name of each address (8b.4), so the name of `SourceTls` stays empty.
        let tls = match m.checked.block_in(label, "mongodb.tls") {
            Some(_) => Some(SourceTls {
                ca_pem: m.ca_pem("mongodb.tls.ca_file", stage)?.unwrap_or_default(),
                server_name: String::new(),
            }),
            None => None,
        };
        // The driver reads the trust roots from a file only (8b.4), so it needs the path as well.
        let tls_ca_file = match tls {
            Some(_) => Some(m.path("mongodb.tls.ca_file")?),
            None => None,
        };
        mongo_sources.push(MongoSourceConfig {
            id,
            label: m.text("mongodb.label")?,
            topology,
            user: m.text("mongodb.user")?,
            password: m.secret("mongodb.password")?.expose().to_owned(),
            auth_database: m.text("mongodb.auth_database")?,
            databases: m.text_list("mongodb.databases")?,
            collections: m.text_list("mongodb.collections")?,
            read_preference,
            tls_mode,
            tls,
            tls_ca_file,
            connect_timeout: m.duration("mongodb.connect_timeout")?,
            max_execution_time: m.duration("mongodb.max_execution_time")?,
            max_result_rows: m.int("mongodb.max_result_rows")?,
            max_result_bytes: m.int("mongodb.max_result_bytes")?,
            max_query_size: m.int("mongodb.max_query_size")?,
            max_concurrent: m.int32("mongodb.max_concurrent")?,
            queue_wait: m.duration("mongodb.queue_wait")?,
            max_concurrent_per_subject: m
                .opt("mongodb.max_concurrent_per_subject")
                .map(|_| m.int32("mongodb.max_concurrent_per_subject"))
                .transpose()?,
        });
    }

    // The block `geomap` (13.12c): the parts that stage 2 parsed one time. A partial read mode that did not check the
    // block gives no block, and the server never starts from it.
    let geomap = match (checked.has_block("geomap"), stage.geomap_tile.clone()) {
        (true, Some(tile_url)) => Some(GeomapConfig {
            tile_url,
            attribution: r.text("geomap.attribution")?,
            max_zoom: u8::try_from(r.int("geomap.max_zoom")?)
                .map_err(|_| internal("geomap.max_zoom", "a zoom of 0 to 22 was expected"))?,
        }),
        (true, None) if !partial => return Err(internal("geomap.tile_url", "a checked template was not parsed")),
        _ => None,
    };

    Ok(Config {
        server,
        log: LogConfig { level },
        telemetry,
        storage,
        access,
        clickhouse,
        postgres_sources,
        mongo_sources,
        geomap,
    })
}
