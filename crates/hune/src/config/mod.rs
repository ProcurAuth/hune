// SPDX-License-Identifier: MIT OR Apache-2.0
//! The configuration of Hune (section 13): the key table, the checks and the typed `Config`.
//!
//! This module is the only code that uses the engine `hclconf` (13.2, rule 4; 16.18). It
//! parses the file one time at start into one typed structure. The rest of the code reads
//! only this structure.

mod b64;
mod ca;
mod geomap;
mod keys;
mod pubkeys;
mod render;
mod rules;
mod stage2;
mod typed;
mod url;

use std::path::{Path, PathBuf};
use std::time::Duration;

pub use hclconf::env::CapturedEnvironment;
pub use hclconf::{ConfigError, Profile, Secret};

#[allow(unused_imports)] // the start log of `serve` uses the two JSON functions
pub use render::{
    ReferenceFormat, render_print_config, render_reference, start_log_config_json, start_log_sources_json,
};
pub use url::{HttpUrl, parse_http_url};
// The block `geomap` (13.12c): the server builds the policy of 12.8 and the session member of 6.10 from it.
pub use geomap::{GeomapConfig, IMG_SRC_BASE as GEOMAP_IMG_SRC_BASE};
// The tests of the server build a block from a template.
#[cfg(test)]
pub use geomap::parse_tile_url;
// The access layer reads the SPKI blocks of the public keys (11.9, 11.10) and writes the cursor of the
// options route (11.26) with the base64 functions of this module.
pub(crate) use b64::{decode_url as base64url_decode, encode_url as base64url_encode};
pub use pubkeys::{ParsedKey, parse_public_key_set};

/// The typed configuration.
///
/// A configuration from `load` is complete. A configuration from `load_with` and a partial
/// read mode (`LoadedConfig::mode`) holds the values that the mode read and placeholders for
/// the others: an empty text for a text from `_env` or `_file`, an empty `Secret`, `None` for
/// the bytes of a CA file that stage 2 did not read, and an empty byte list in a `TlsConfig`
/// of such a file. The server MUST NEVER start from a partial configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// The block `server`.
    pub server: ServerConfig,
    /// The block `log`.
    pub log: LogConfig,
    /// The block `telemetry`.
    pub telemetry: TelemetryConfig,
    /// The block `storage`.
    pub storage: StorageConfig,
    /// The block `access`.
    pub access: AccessConfig,
    /// The blocks `clickhouse`, in file order.
    pub clickhouse: Vec<ClickHouseConfig>,
    /// The blocks `postgres "<id>"` (13.12a), in file order. The labels share one space with the
    /// blocks `clickhouse` (C15). With a partial read mode, a block whose label is not a valid
    /// source id has no entry, because that mode does not check the labels.
    pub postgres_sources: Vec<hune_source::PostgresSourceConfig>,
    /// The blocks `mongodb "<id>"` (13.12b), in file order. The labels share one space with the
    /// blocks `clickhouse` and `postgres` (C15). A build without the driver refuses a `mongodb` block
    /// at stage 2, so the list is empty there. With a partial read mode, a block whose label is not a
    /// valid source id has no entry, because that mode does not check the labels.
    pub mongo_sources: Vec<hune_source::MongoSourceConfig>,
    /// The block `geomap` (13.12c), or `None` when the file has none: then the page contacts no tile host. With a partial
    /// read mode that does not check the block, it is `None` too.
    pub geomap: Option<GeomapConfig>,
}

/// The block `server` (13.8).
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The main listener.
    pub listen: std::net::SocketAddr,
    /// The listener of the probes and the metrics.
    pub ops_listen: std::net::SocketAddr,
    /// The prefix of the routes of `listen`.
    pub base_path: String,
    /// True gives the history of a dashboard of the primary store (6.17). The key has no default.
    pub history: bool,
    /// True gives the file of the current document of a dashboard (6.14). The key has no default.
    pub export: bool,
    /// True adds the expanded SQL to the answer of a panel query (6.20). The default is false.
    pub query_inspect: bool,
    /// The most copies of one panel that repeats (4.13a). The default is `MAX_REPEAT_PER_PANEL`.
    pub repeat_max_per_panel: u32,
    /// The most copies of one section that repeats. The default is `MAX_REPEAT_PER_SECTION`.
    pub repeat_max_per_section: u32,
    /// The most copies of panels that repeat in one dashboard. The default is `MAX_REPEAT_PER_DASHBOARD`.
    pub repeat_max_per_dashboard: u32,
    /// True sends the gzip copy of a static file of a text type to a request that accepts gzip (9.3). The
    /// default is true.
    pub compress_static: bool,
    /// The most cells of the frame that one step of a transform makes in the page (4.35, 9.21a). The session gives it to
    /// the page (6.10). The default is `TRANSFORM_MAX_CELLS_DEFAULT`.
    pub transform_max_cells: u32,
    /// The list page shows its sort control and its tag filter above this number of dashboards (9.7). The session
    /// gives it to the page (6.10). The default is `LIST_FILTERS_MIN_DEFAULT`.
    pub list_filters_min_dashboards: u32,
    /// The most open connections on `listen`.
    pub max_connections: u32,
    /// The idle time after which Hune closes a kept connection.
    pub idle_timeout: Duration,
    /// The longest graceful stop.
    pub shutdown_grace: Duration,
    /// The body limit of a dashboard create, save or import.
    pub max_request_bytes: u64,
    /// The memory budget of the results in flight.
    pub max_result_memory_bytes: u64,
}

/// The log level (13.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Errors only.
    Error,
    /// Warnings and errors.
    Warn,
    /// Information, warnings and errors.
    Info,
    /// All levels.
    Debug,
}

/// The block `log`.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// The lowest level that Hune writes.
    pub level: LogLevel,
}

/// The block `telemetry`.
#[derive(Debug, Clone)]
pub enum TelemetryConfig {
    /// `enabled = false`.
    Disabled,
    /// `enabled = true`.
    Enabled(TelemetryEnabled),
}

/// The keys of an enabled `telemetry` block.
#[derive(Debug, Clone)]
pub struct TelemetryEnabled {
    /// The OTLP endpoint.
    pub otlp_endpoint: String,
    /// The CA file of an `https` endpoint.
    pub ca_file: Option<PathBuf>,
    /// The bytes of the CA file, read and checked at stage 2.
    pub ca_pem: Option<Vec<u8>>,
    /// The resource attribute `service.name`.
    pub service_name: String,
    /// The period of the export.
    pub export_interval: Duration,
}

/// The block `storage` (13.10): the stores of the dashboards.
///
/// Exactly one store has `primary = true`, or the key `read_only` is `true` and no store is the primary store. The
/// primary store is the only store that accepts writes and has a history. Each other store is read-only.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// `storage.read_only`: true when the file says that no store is the primary store.
    pub read_only: bool,
    /// The blocks `storage.volume "<id>"`, in file order.
    pub volumes: Vec<VolumeConfig>,
    /// The block `storage.postgres`, or `None`.
    pub postgres: Option<PostgresConfig>,
}

/// The id of the PostgreSQL store. A volume cannot have this label.
pub const POSTGRES_STORE_ID: &str = "postgres";

/// The primary store of a configuration.
#[derive(Debug, Clone, Copy)]
pub enum PrimaryRef<'a> {
    /// A volume store.
    Volume(&'a VolumeConfig),
    /// The PostgreSQL store.
    Postgres(&'a PostgresConfig),
}

impl StorageConfig {
    /// The primary store, or `None` when no store has `primary = true`.
    pub fn primary(&self) -> Option<PrimaryRef<'_>> {
        if let Some(v) = self.volumes.iter().find(|v| v.primary) {
            return Some(PrimaryRef::Volume(v));
        }
        self.postgres.as_ref().filter(|p| p.primary).map(PrimaryRef::Postgres)
    }

    /// The id of the primary store: the label of the volume, or `postgres`.
    pub fn primary_id(&self) -> Option<&str> {
        match self.primary()? {
            PrimaryRef::Volume(v) => Some(&v.id),
            PrimaryRef::Postgres(_) => Some(POSTGRES_STORE_ID),
        }
    }
}

/// The block `storage.volume "<id>"` (13.10).
#[derive(Debug, Clone)]
pub struct VolumeConfig {
    /// The label of the block: the id of the store.
    pub id: String,
    /// The name of the root folder in the page. `None` for the primary store.
    pub title: Option<String>,
    /// True for the primary store.
    pub primary: bool,
    /// The directory of the dashboards.
    pub path: PathBuf,
    /// The period of the scan for changes made outside Hune.
    pub rescan_interval: Duration,
    /// The versions kept for each dashboard. `Some` for the primary store only.
    pub history_keep: Option<u32>,
}

/// The value of `storage.postgres.sslmode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    /// Check the certificate and the name.
    VerifyFull,
    /// No TLS.
    Disable,
}

/// The block `storage.postgres` (13.10).
#[derive(Debug, Clone)]
pub struct PostgresConfig {
    /// The name of the root folder in the page. `None` for the primary store.
    pub title: Option<String>,
    /// True for the primary store.
    pub primary: bool,
    /// The host.
    pub host: String,
    /// The port.
    pub port: u16,
    /// The database.
    pub database: String,
    /// The role of `serve`.
    pub user: String,
    /// The password of the role.
    pub password: Secret,
    /// The TLS mode.
    pub sslmode: SslMode,
    /// The block `tls`, present with `verify-full`.
    pub tls: Option<TlsConfig>,
    /// The size of the connection pool.
    pub max_connections: u32,
    /// The longest wait for a connection.
    pub connect_timeout: Duration,
    /// The longest statement.
    pub statement_timeout: Duration,
    /// The period of the poll of the catalog.
    pub poll_interval: Duration,
    /// The versions kept for each dashboard. `Some` for the primary store only.
    pub history_keep: Option<u32>,
}

/// A block `tls`.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// The CA file.
    pub ca_file: PathBuf,
    /// The bytes of the CA file, read and checked at stage 2.
    pub ca_pem: Vec<u8>,
    /// The name that the certificate must hold.
    pub server_name: String,
}

/// The block `access`.
// The value is built one time at start, so the size difference of the variants has no cost.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum AccessConfig {
    /// The mode `open`.
    Open,
    /// The mode `authzen`.
    Authzen(AuthzenConfig),
}

/// The blocks of the mode `authzen`.
#[derive(Debug, Clone)]
pub struct AuthzenConfig {
    /// The block `identity`.
    pub identity: IdentityConfig,
    /// The block `pdp`.
    pub pdp: PdpConfig,
}

/// The block `access.identity` (13.11).
#[derive(Debug, Clone)]
pub struct IdentityConfig {
    /// The name of the header that holds the token.
    pub header: String,
    /// The value of the claim `iss`.
    pub issuer: String,
    /// The service id: the claim `aud`.
    pub audience: String,
    /// The text of the public keys: PEM or a JWK Set.
    pub public_keys: String,
    /// The longest life of a token.
    pub max_age: Duration,
    /// The skew that applies to `iat` and `exp`.
    pub clock_skew: Duration,
}

/// The block `access.pdp` (13.11).
#[derive(Debug, Clone)]
pub struct PdpConfig {
    /// The base URL of the PDP.
    pub url: String,
    /// The credential of Hune at the PDP.
    pub token: Secret,
    /// True accepts an `http` URL.
    pub plain_http: bool,
    /// The CA file, present with `plain_http = false`.
    pub ca_file: Option<PathBuf>,
    /// The bytes of the CA file, read and checked at stage 2.
    pub ca_pem: Option<Vec<u8>>,
    /// The longest wait for the PDP.
    pub timeout: Duration,
    /// The longest life of a kept answer.
    pub cache_ttl: Duration,
    /// The most connections to the PDP.
    pub max_connections: u32,
}

/// A block `clickhouse "<id>"` (13.12).
#[derive(Debug, Clone)]
pub struct ClickHouseConfig {
    /// The label of the block: the source id.
    pub id: String,
    /// The label in the page.
    pub label: String,
    /// The HTTP interface.
    pub url: HttpUrl,
    /// The ClickHouse account.
    pub user: String,
    /// The password of the account.
    pub password: Secret,
    /// The default database of the queries.
    pub database: String,
    /// The block `tls`, present with an `https` URL.
    pub tls: Option<TlsConfig>,
    /// The longest wait for a connection.
    pub connect_timeout: Duration,
    /// The longest run of a query.
    pub max_execution_time: Duration,
    /// The most rows of a result.
    pub max_result_rows: u64,
    /// The most bytes of a result.
    pub max_result_bytes: u64,
    /// The largest expanded SQL.
    pub max_query_size: u64,
    /// The most queries at one time.
    pub max_concurrent: u32,
    /// The longest wait for a place.
    pub queue_wait: Duration,
    /// The most queries at one time for one subject (mode `authzen` only).
    pub max_concurrent_per_subject: Option<u32>,
    /// The columns that an ad hoc filter of this source can name (4.10c, 13.12). `None`: the key is absent, and no ad hoc
    /// filter applies on this source.
    pub adhoc_columns: Option<Vec<String>>,
}

/// A warning (13.15). It never stops the start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The key of the warning.
    pub key: String,
    /// The message.
    pub message: String,
}

/// One line of the effective configuration (13.20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveEntry {
    /// The key path, for example `clickhouse "prod".url`.
    pub key: String,
    /// The value in the form of `print-config`: a text in double quotes, `<hidden>` for a
    /// secret.
    pub value: String,
    /// The value as a JSON text, for the start log (13.22). A secret is the JSON string
    /// `"<hidden>"`.
    pub json: String,
    /// The source: `file`, `env:VAR`, `file:/path` or `default`.
    pub source: String,
}

/// Which `_env` and `_file` values a load reads, and which checks of 13.13 it runs (13.18).
///
/// Each mode runs the key table (stage 1) and the contradiction checks of 13.14, except the
/// checks of the values that it does not read. A value that a mode does not read shows as
/// `<not read>` in `LoadedConfig::effective`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadMode {
    /// Read each `_env` and `_file` value, and run each check of 13.13. This is the mode of
    /// `serve`, `check-config` and `print-config`.
    All,
    /// The mode of `migrate-store`, `migrate-documents`, `copy-store`, `export`, `import` and
    /// `restore`. Read only the `_env` and `_file` values of the block `storage`. Run the
    /// checks of 13.13 for the block `storage` only.
    StoreCommands,
    /// The mode of `import-grafana`. It is the mode `StoreCommands` and also the check of the
    /// labels of the `clickhouse` and `postgres` blocks (13.13). The labels are in the file, so
    /// this mode reads no `_env` or `_file` value of these blocks.
    ImportWrite,
    /// Read no `_env` and no `_file` value. Run each check of 13.13 for the values that are
    /// in the file.
    Nothing,
}

impl ReadMode {
    /// True when the load is complete: it reads each value. Only a complete load can start
    /// the server.
    pub fn is_complete(self) -> bool {
        self == ReadMode::All
    }

    /// The read mode of the engine.
    fn engine(self) -> hclconf::ReadMode<'static> {
        match self {
            ReadMode::All => hclconf::ReadMode::All,
            ReadMode::StoreCommands | ReadMode::ImportWrite => hclconf::ReadMode::Blocks(&["storage"]),
            ReadMode::Nothing => hclconf::ReadMode::None,
        }
    }

    /// True when stage 2 checks the value of this key. It never checks a value that the mode
    /// did not read.
    fn checks_key(self, key: &hclconf::ResolvedKey) -> bool {
        key.is_read()
            && match self {
                ReadMode::All | ReadMode::Nothing => true,
                ReadMode::StoreCommands | ReadMode::ImportWrite => key.path.starts_with("storage."),
            }
    }

    /// True when stage 2 checks the labels of the `clickhouse`, `postgres` and `mongodb` blocks.
    fn checks_labels(self) -> bool {
        self != ReadMode::StoreCommands
    }

    /// True when stage 2 refuses a `mongodb` block in a build without the driver (8b.2). The modes
    /// that read only the block `storage`, and the labels and kinds of the source blocks, need none.
    fn checks_driver(self) -> bool {
        matches!(self, ReadMode::All | ReadMode::Nothing)
    }
}

/// True when this build has the MongoDB driver (8b.2). A build without the cargo feature
/// `mongo-driver` has none. A unit test sets the answer for its own thread, and its default is true.
fn has_mongo_driver() -> bool {
    #[cfg(test)]
    {
        test_support::mongo_driver()
    }
    #[cfg(not(test))]
    {
        cfg!(feature = "mongo-driver")
    }
}

/// The result of a successful load.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    /// The read mode of the load. The server starts only from the mode `ReadMode::All`.
    pub mode: ReadMode,
    /// The typed configuration. With a partial mode, it holds placeholders (see `Config`).
    pub config: Config,
    /// The warnings, in the order of 13.15.
    pub warnings: Vec<Warning>,
    /// The effective configuration, in the order of the key table.
    pub effective: Vec<EffectiveEntry>,
}

/// The text that stands for a secret in `print-config` (13.20).
const HIDDEN_VALUE: &str = "<hidden>";

/// The keys that have a default value (13.7): each can be absent, and the default applies. The effective
/// configuration shows the default with the source `default`. A row is the key, the default and the key that
/// the entry follows in the key table. The defaults of the repeat keys are the constants of `hune-core`. The defaults of
/// `server.transform_max_cells` and `server.list_filters_min_dashboards` are constants of the key table, because the server never builds a frame.
fn defaulted_keys() -> [(&'static str, String, &'static str); 7] {
    [
        ("server.query_inspect", "false".to_string(), "server.export"),
        ("server.repeat_max_per_panel", hune_core::limits::MAX_REPEAT_PER_PANEL.to_string(), "server.query_inspect"),
        (
            "server.repeat_max_per_section",
            hune_core::limits::MAX_REPEAT_PER_SECTION.to_string(),
            "server.repeat_max_per_panel",
        ),
        (
            "server.repeat_max_per_dashboard",
            hune_core::limits::MAX_REPEAT_PER_DASHBOARD.to_string(),
            "server.repeat_max_per_section",
        ),
        ("server.compress_static", "true".to_string(), "server.repeat_max_per_dashboard"),
        ("server.transform_max_cells", keys::TRANSFORM_MAX_CELLS_DEFAULT.to_string(), "server.compress_static"),
        (
            "server.list_filters_min_dashboards",
            keys::LIST_FILTERS_MIN_DEFAULT.to_string(),
            "server.transform_max_cells",
        ),
    ]
}
const DEFAULT_SOURCE: &str = "default";

fn effective_entries(checked: &hclconf::Checked, stage: &stage2::Stage2) -> Vec<EffectiveEntry> {
    let mut entries = effective_of_keys(checked, stage);
    for (key, value, after) in defaulted_keys() {
        if checked.get(key).is_none() {
            // The entry goes after the key before it in the key table.
            let at = entries.iter().position(|e| e.key == after).map_or(entries.len(), |i| i + 1);
            entries.insert(
                at,
                EffectiveEntry { key: key.to_string(), json: value.clone(), value, source: DEFAULT_SOURCE.to_string() },
            );
        }
    }
    // The derived line of 13.20: the value of `img-src` with the tile source, after the last key of the block `geomap`.
    // It is not a key: the file cannot set it.
    if let Some(tile) = &stage.geomap_tile {
        let text = hclconf::value::json_string(&format!("{} {}", geomap::IMG_SRC_BASE, tile.csp_source()));
        let at = entries.iter().position(|e| e.key == "geomap.max_zoom").map_or(entries.len(), |i| i + 1);
        entries.insert(
            at,
            EffectiveEntry {
                key: "geomap.img_src".to_string(),
                value: text.clone(),
                json: text,
                source: "derived".to_string(),
            },
        );
    }
    entries
}

fn effective_of_keys(checked: &hclconf::Checked, stage: &stage2::Stage2) -> Vec<EffectiveEntry> {
    checked
        .keys()
        .iter()
        .map(|key| {
            let (value, json) = if !key.is_read() {
                let text = key.value.print();
                (text.clone(), hclconf::value::json_string(&text))
            } else if key.secret {
                (HIDDEN_VALUE.to_string(), format!("\"{HIDDEN_VALUE}\""))
            } else if key.path == "access.identity.public_keys" {
                let items: Vec<String> =
                    stage.public_keys.iter().map(|k| hclconf::value::json_string(&k.display())).collect();
                let list = format!("[{}]", items.join(", "));
                (list.clone(), list)
            } else {
                let printed = key.value.print();
                (printed.clone(), printed)
            };
            EffectiveEntry { key: key.display.clone(), value, json, source: key.source.text() }
        })
        .collect()
}

/// Read the file, check it in three stages (13.19) and build the typed configuration.
///
/// Stage 1 checks the file against the key table, the forms and the reads of `_env` and
/// `_file`. Stage 2 checks the values (13.13). Stage 3 checks the contradictions. A stage
/// runs only when the stage before it has no error. The error list is the list of the first
/// stage that fails.
pub fn load(path: &Path, env: &CapturedEnvironment) -> Result<LoadedConfig, Vec<ConfigError>> {
    load_with(path, env, ReadMode::All)
}

/// Like `load`, with a read mode (13.18). A mode other than `ReadMode::All` reads only some
/// `_env` and `_file` values and runs only some checks of 13.13: see `ReadMode`. The typed
/// `Config` of such a load has placeholders for the values that it did not read. The server
/// MUST NEVER start from it.
pub fn load_with(path: &Path, env: &CapturedEnvironment, mode: ReadMode) -> Result<LoadedConfig, Vec<ConfigError>> {
    let checked = hclconf::check_file_with(path, &keys::SCHEMA, env, mode.engine())?;
    let stage = stage2::run(&checked, mode)?;
    let contradictions = rules::contradictions(&checked, &stage);
    if !contradictions.is_empty() {
        return Err(hclconf::error::limit_errors(contradictions));
    }
    let warnings = rules::warnings(&checked, &stage);
    let config = typed::build(&checked, &stage, !mode.is_complete())?;
    let effective = effective_entries(&checked, &stage);
    Ok(LoadedConfig { mode, config, warnings, effective })
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
