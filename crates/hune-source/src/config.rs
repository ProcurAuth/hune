//! The configuration of one ClickHouse source. The binary builds it from its own types.
//!
//! The crate does not depend on the configuration types of the binary (3.3, rule 3).

use std::fmt;
use std::time::Duration;

use hune_core::ids::SourceId;
use hune_core::mongo::ReadPreference;

/// The TLS settings of an `https` source (8.6).
#[derive(Clone, PartialEq, Eq)]
pub struct SourceTls {
    /// The PEM bytes of the CA certificates. They are the only trust roots.
    pub ca_pem: Vec<u8>,
    /// The name that the certificate MUST hold. Hune also sends it in SNI.
    pub server_name: String,
}

impl fmt::Debug for SourceTls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceTls")
            .field("ca_pem_bytes", &self.ca_pem.len())
            .field("server_name", &self.server_name)
            .finish()
    }
}

/// The settings of one `clickhouse "<id>"` block (8.2, 13.12).
#[derive(Clone)]
pub struct ClickHouseSourceConfig {
    /// The id of the block.
    pub id: SourceId,
    /// The label for the page.
    pub label: String,
    /// True for the scheme `https`.
    pub https: bool,
    /// The host of the URL.
    pub host: String,
    /// The port of the URL.
    pub port: u16,
    /// The user name.
    pub user: String,
    /// The password. The crate never logs it and never shows it in an error.
    pub password: String,
    /// The default database of the queries.
    pub database: String,
    /// The TLS settings. They are necessary for `https` and forbidden for `http`.
    pub tls: Option<SourceTls>,
    /// The time to connect and to do the TLS handshake.
    pub connect_timeout: Duration,
    /// The time limit of a query. The probe does not use it.
    pub max_execution_time: Duration,
    /// The row limit of a result. The probe uses it.
    pub max_result_rows: u64,
    /// The byte limit of a result.
    pub max_result_bytes: u64,
    /// The size limit of the SQL.
    pub max_query_size: u64,
    /// The most queries at one time on the source.
    pub max_concurrent: u32,
    /// The longest wait for a place (8.8).
    pub queue_wait: Duration,
    /// The most queries at one time for one subject. `None` in mode `open`.
    pub max_concurrent_per_subject: Option<u32>,
}

impl fmt::Debug for ClickHouseSourceConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClickHouseSourceConfig")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("https", &self.https)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("password", &"(hidden)")
            .field("database", &self.database)
            .field("tls", &self.tls)
            .field("connect_timeout", &self.connect_timeout)
            .field("max_execution_time", &self.max_execution_time)
            .field("max_result_rows", &self.max_result_rows)
            .field("max_result_bytes", &self.max_result_bytes)
            .field("max_query_size", &self.max_query_size)
            .field("max_concurrent", &self.max_concurrent)
            .field("queue_wait", &self.queue_wait)
            .field("max_concurrent_per_subject", &self.max_concurrent_per_subject)
            .finish()
    }
}

/// The `sslmode` of a `postgres "<id>"` block (13.12a, 8a.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PostgresSslMode {
    /// TLS 1.2 or 1.3, with the check of the certificate and of the name.
    VerifyFull,
    /// No TLS.
    Disable,
}

impl PostgresSslMode {
    /// Gives the mode as the text of the configuration.
    pub fn as_str(self) -> &'static str {
        match self {
            PostgresSslMode::VerifyFull => "verify-full",
            PostgresSslMode::Disable => "disable",
        }
    }
}

/// The settings of one `postgres "<id>"` block (8a.2, 13.12a).
///
/// The binary builds this value from its own configuration types. The crate does not read the file.
/// The binary has checked each range of 13.12a before it builds the value, so the crate does not
/// check them again.
#[derive(Clone)]
pub struct PostgresSourceConfig {
    /// The id of the block.
    pub id: SourceId,
    /// The label for the page.
    pub label: String,
    /// The one host that Hune connects to (8a.4). A DNS name or an IP address.
    pub host: String,
    /// The port of the host.
    pub port: u16,
    /// The database of the connection.
    pub database: String,
    /// The name of the read-only role (8a.13).
    pub user: String,
    /// The password. The crate never logs it and never shows it in an error.
    pub password: String,
    /// The names of the session `search_path`: 1 to 8 names (8a.4).
    pub search_path: Vec<String>,
    /// The mode of TLS.
    pub sslmode: PostgresSslMode,
    /// The TLS settings. They are necessary for `verify-full` and forbidden for `disable`.
    pub tls: Option<SourceTls>,
    /// The time for the connection, TLS, login and setup check (8a.4).
    pub connect_timeout: Duration,
    /// The time limit of a query, in whole seconds.
    pub max_execution_time: Duration,
    /// The row limit of a result.
    pub max_result_rows: u64,
    /// The byte limit of a result.
    pub max_result_bytes: u64,
    /// The size limit of the SQL.
    pub max_query_size: u64,
    /// The most queries at one time on the source, and the size of its pool (8a.9).
    pub max_concurrent: u32,
    /// The longest wait for a place (8a.10).
    pub queue_wait: Duration,
    /// The most queries at one time for one subject. `None` in mode `open`.
    pub max_concurrent_per_subject: Option<u32>,
}

impl fmt::Debug for PostgresSourceConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostgresSourceConfig")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("user", &self.user)
            .field("password", &"(hidden)")
            .field("search_path", &self.search_path)
            .field("sslmode", &self.sslmode)
            .field("tls", &self.tls)
            .field("connect_timeout", &self.connect_timeout)
            .field("max_execution_time", &self.max_execution_time)
            .field("max_result_rows", &self.max_result_rows)
            .field("max_result_bytes", &self.max_result_bytes)
            .field("max_query_size", &self.max_query_size)
            .field("max_concurrent", &self.max_concurrent)
            .field("queue_wait", &self.queue_wait)
            .field("max_concurrent_per_subject", &self.max_concurrent_per_subject)
            .finish()
    }
}

/// The topology of a `mongodb "<id>"` block (13.12b, 8b.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MongoTopology {
    /// One host, no discovery (`topology = "direct"`).
    Direct {
        /// The host: a DNS name or an IP address.
        host: String,
        /// The port.
        port: u16,
    },
    /// The complete list of the members of a replica set (`topology = "replica_set"`).
    ReplicaSet {
        /// The members, 2 to 7. An IPv6 address has no brackets here.
        hosts: Vec<MongoHost>,
        /// The name of the replica set.
        replica_set: String,
    },
}

/// One member of the key `hosts` (13.12b): the text `host:port`, split.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MongoHost {
    /// The host: a DNS name or an IP address.
    pub host: String,
    /// The port.
    pub port: u16,
}

/// The `tls_mode` of a `mongodb "<id>"` block (13.12b, 8b.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MongoTlsMode {
    /// TLS 1.2 or 1.3, with the check of the certificate and of the name.
    VerifyFull,
    /// No TLS.
    Disable,
}

impl MongoTlsMode {
    /// Gives the mode as the text of the configuration.
    pub fn as_str(self) -> &'static str {
        match self {
            MongoTlsMode::VerifyFull => "verify-full",
            MongoTlsMode::Disable => "disable",
        }
    }
}

/// The settings of one `mongodb "<id>"` block (8b.2, 13.12b).
///
/// The binary builds this value from its own configuration types. The crate does not read the file.
/// The binary has checked each range of 13.12b before it builds the value. The file parser does not
/// build this type yet: a build without the driver refuses a `mongodb` block at start (8b.2).
#[derive(Clone)]
pub struct MongoSourceConfig {
    /// The id of the block.
    pub id: SourceId,
    /// The label for the page.
    pub label: String,
    /// The hosts that Hune connects to (8b.4).
    pub topology: MongoTopology,
    /// The name of the read-only user (8b.13).
    pub user: String,
    /// The password. The crate never logs it and never shows it in an error.
    pub password: String,
    /// The database that holds the user.
    pub auth_database: String,
    /// The databases that a query can read: 1 to 8 names (7.33). The first one is the default.
    pub databases: Vec<String>,
    /// The collections that a query can name: 1 to 64 names (7.33).
    pub collections: Vec<String>,
    /// The read preference of each command (8b.5).
    pub read_preference: ReadPreference,
    /// The mode of TLS.
    pub tls_mode: MongoTlsMode,
    /// The TLS settings. They are necessary for `verify-full` and forbidden for `disable`.
    pub tls: Option<SourceTls>,
    /// The path of the CA file of `tls`. The driver reads trust roots from a file only, so it needs the path.
    /// The bytes of the file are in `tls.ca_pem`: the driver refuses a file that no more holds these bytes.
    pub tls_ca_file: Option<std::path::PathBuf>,
    /// The time for the connection, TLS, login and server selection (8b.4).
    pub connect_timeout: Duration,
    /// The time limit of a query, in whole seconds.
    pub max_execution_time: Duration,
    /// The row limit of a result.
    pub max_result_rows: u64,
    /// The byte limit of a result.
    pub max_result_bytes: u64,
    /// The size limit of the expanded command, in BSON bytes.
    pub max_query_size: u64,
    /// The most queries at one time on the source.
    pub max_concurrent: u32,
    /// The longest wait for a place (8b.9).
    pub queue_wait: Duration,
    /// The most queries at one time for one subject. `None` in mode `open`.
    pub max_concurrent_per_subject: Option<u32>,
}

impl fmt::Debug for MongoSourceConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MongoSourceConfig")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("topology", &self.topology)
            .field("user", &self.user)
            .field("password", &"(hidden)")
            .field("auth_database", &self.auth_database)
            .field("databases", &self.databases)
            .field("collections", &self.collections)
            .field("read_preference", &self.read_preference)
            .field("tls_mode", &self.tls_mode)
            .field("tls", &self.tls)
            .field("tls_ca_file", &self.tls_ca_file)
            .field("connect_timeout", &self.connect_timeout)
            .field("max_execution_time", &self.max_execution_time)
            .field("max_result_rows", &self.max_result_rows)
            .field("max_result_bytes", &self.max_result_bytes)
            .field("max_query_size", &self.max_query_size)
            .field("max_concurrent", &self.max_concurrent)
            .field("queue_wait", &self.queue_wait)
            .field("max_concurrent_per_subject", &self.max_concurrent_per_subject)
            .finish()
    }
}
