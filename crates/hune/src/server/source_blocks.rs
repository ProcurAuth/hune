//! The source blocks of the configuration, seen without their kind (8a.2, 11.33).
//!
//! A `clickhouse` block, a `postgres` block and a `mongodb` block share the label space (C15). The server needs the
//! same facts about both: the id, the label, the kind and the limits. This module gives them in one
//! list, so no handler reads `config.clickhouse` alone. It gives no host, no user and no secret to
//! a caller that does not need them.

use crate::config::Config;
use hune_core::sql::Dialect;
use hune_source::{MongoSourceConfig, MongoTopology, PostgresSourceConfig, SourceKind};
use std::time::Duration;

/// The facts about one source block.
#[derive(Debug, Clone)]
pub struct SourceBlock<'a> {
    /// The id of the block.
    pub id: &'a str,
    /// The label for the page.
    pub label: &'a str,
    /// The kind of the block.
    pub kind: SourceKind,
    /// The host of the server, for the span of a query.
    pub host: &'a str,
    /// The port of the server, for the span of a query.
    pub port: u16,
    /// The time limit of a query.
    pub max_execution_time: Duration,
    /// The row limit of a result.
    pub max_result_rows: u64,
    /// The byte limit of a result.
    pub max_result_bytes: u64,
    /// The size limit of the expanded SQL, or of the expanded MongoDB command in BSON bytes.
    pub max_query_size: u64,
    /// The lists `databases` and `collections` of a `mongodb` block (7.33). `None` for the other kinds.
    pub mongo: Option<MongoLists<'a>>,
    /// The key `adhoc_columns` of a `clickhouse` block (4.10c, 13.12). `None` for the other kinds and when the key is
    /// absent: then no ad hoc filter applies on the source.
    pub adhoc_columns: Option<&'a [String]>,
}

/// The lists of a `mongodb` block that the expansion of a query needs (7.33, 7.35).
#[derive(Debug, Clone, Copy)]
pub struct MongoLists<'a> {
    /// The key `databases`: the first name is the default database.
    pub databases: &'a [String],
    /// The key `collections`.
    pub collections: &'a [String],
}

impl<'a> SourceBlock<'a> {
    /// Makes a block of a test with the default limits of 13.12. It has no list of a `mongodb` block.
    #[cfg(test)]
    pub(crate) fn stub(id: &'a str, kind: SourceKind) -> SourceBlock<'a> {
        SourceBlock {
            id,
            label: id,
            kind,
            host: "127.0.0.1",
            port: 1,
            max_execution_time: Duration::from_secs(60),
            max_result_rows: 100_000,
            max_result_bytes: 33_554_432,
            max_query_size: 1_048_576,
            mongo: None,
            adhoc_columns: None,
        }
    }
}

impl SourceBlock<'_> {
    /// Gives the dialect of the SQL of this source (7.19). A `mongodb` source has no SQL dialect (7.27):
    /// it gives `None`, and its query follows 7.28 to 7.35.
    pub fn dialect(&self) -> Option<Dialect> {
        dialect_of(self.kind)
    }
}

/// Gives the dialect of the SQL of a source kind (7.19). The kind `mongodb` has none (7.27).
pub fn dialect_of(kind: SourceKind) -> Option<Dialect> {
    match kind {
        SourceKind::ClickHouse => Some(Dialect::ClickHouse),
        SourceKind::Postgres => Some(Dialect::Postgres),
        SourceKind::MongoDb => None,
    }
}

/// The blocks `mongodb`, in file order.
pub(super) fn mongo_blocks(config: &Config) -> &[MongoSourceConfig] {
    &config.mongo_sources
}

/// The host and the port of a `mongodb` block for the span of a query: the first host of the block.
fn mongo_address(topology: &MongoTopology) -> (&str, u16) {
    match topology {
        MongoTopology::Direct { host, port } => (host, *port),
        MongoTopology::ReplicaSet { hosts, .. } => hosts.first().map_or(("", 0), |h| (h.host.as_str(), h.port)),
    }
}

/// The blocks `postgres`, in file order.
pub(super) fn postgres_blocks(config: &Config) -> &[PostgresSourceConfig] {
    &config.postgres_sources
}

/// Lists the blocks of the three kinds: the `clickhouse` blocks, then the `postgres` blocks, then the
/// `mongodb` blocks, each in the order of the file.
pub fn blocks(config: &Config) -> Vec<SourceBlock<'_>> {
    let clickhouse = config.clickhouse.iter().map(|c| SourceBlock {
        id: &c.id,
        label: &c.label,
        kind: SourceKind::ClickHouse,
        host: &c.url.host,
        port: c.url.port,
        max_execution_time: c.max_execution_time,
        max_result_rows: c.max_result_rows,
        max_result_bytes: c.max_result_bytes,
        max_query_size: c.max_query_size,
        mongo: None,
        adhoc_columns: c.adhoc_columns.as_deref(),
    });
    let postgres = postgres_blocks(config).iter().map(|p| SourceBlock {
        id: p.id.as_str(),
        label: &p.label,
        kind: SourceKind::Postgres,
        host: &p.host,
        port: p.port,
        max_execution_time: p.max_execution_time,
        max_result_rows: p.max_result_rows,
        max_result_bytes: p.max_result_bytes,
        max_query_size: p.max_query_size,
        mongo: None,
        adhoc_columns: None,
    });
    let mongodb = mongo_blocks(config).iter().map(|m| {
        let (host, port) = mongo_address(&m.topology);
        SourceBlock {
            id: m.id.as_str(),
            label: &m.label,
            kind: SourceKind::MongoDb,
            host,
            port,
            max_execution_time: m.max_execution_time,
            max_result_rows: m.max_result_rows,
            max_result_bytes: m.max_result_bytes,
            max_query_size: m.max_query_size,
            mongo: Some(MongoLists { databases: &m.databases, collections: &m.collections }),
            adhoc_columns: None,
        }
    });
    clickhouse.chain(postgres).chain(mongodb).collect()
}

/// Finds one block by its id.
pub fn find<'a>(config: &'a Config, id: &str) -> Option<SourceBlock<'a>> {
    blocks(config).into_iter().find(|b| b.id == id)
}
