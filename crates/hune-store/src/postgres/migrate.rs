//! `hune migrate-store` (5.28): the schema migrations, under an advisory lock, for the owner role.

use std::fmt;

use super::conn::{self, Direct, describe};
use super::{PgOptions, sql};

/// One schema migration: the SQL of one version step.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// The schema version after this step.
    pub version: i32,
    /// The SQL. It ends with the update of `hune_schema` (5.24, item 6).
    pub sql: &'static str,
}

/// The migrations of this binary, in the order of the versions. The SQL of each step is a file
/// of `hune-store/migrations/`.
pub const MIGRATIONS: &[Migration] = &[Migration { version: 1, sql: include_str!("../../migrations/0001.sql") }];

/// The schema version of this binary (M in 5.28 and 5.29).
pub const SCHEMA_VERSION: i32 = MIGRATIONS[MIGRATIONS.len() - 1].version;

/// The result of a migration run that succeeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrateReport {
    /// The version of the database before the run (0 for an empty database).
    pub from: i32,
    /// The version of the database after the run.
    pub to: i32,
    /// The steps that the run applied, in order.
    pub applied: Vec<i32>,
}

impl fmt::Display for MigrateReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.applied.is_empty() {
            write!(f, "schema version {}, nothing to do", self.to)
        } else {
            write!(f, "schema version {}", self.to)
        }
    }
}

/// The error of a migration run. Each one gives the exit code 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrateError {
    /// The session cannot be opened.
    Connect(String),
    /// The database encoding is not `UTF8`.
    Encoding(String),
    /// Another `migrate-store` holds the advisory lock.
    LockHeld,
    /// The database has a schema version above the binary.
    Newer {
        /// The version of the database.
        database: i32,
        /// The version of this binary.
        binary: i32,
    },
    /// A step failed and was rolled back. The schema stays at version `version - 1`.
    Step {
        /// The version of the failed step.
        version: i32,
        /// The PostgreSQL error.
        message: String,
    },
    /// Another database error.
    Database(String),
}

impl fmt::Display for MigrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MigrateError::Connect(m) => write!(f, "storage.postgres: cannot connect: {m}"),
            MigrateError::Encoding(e) => write!(f, "storage.postgres: database encoding is {e}; Hune needs UTF8"),
            MigrateError::LockHeld => f.write_str("another hune migrate-store holds the lock"),
            MigrateError::Newer { database, binary } => {
                write!(f, "database schema {database} is newer than this binary ({binary})")
            }
            MigrateError::Step { version, message } => {
                write!(f, "migration {version} failed: {message}; the schema stays at version {}", version - 1)
            }
            MigrateError::Database(m) => write!(f, "storage.postgres: {m}"),
        }
    }
}

impl std::error::Error for MigrateError {}

fn dberr(e: tokio_postgres::Error) -> MigrateError {
    MigrateError::Database(describe(&e))
}

async fn unlock(d: &Direct) {
    drop(d.client.query_opt(sql::P29, &[]).await);
}

/// Applies the schema migrations of this binary (5.28). The options name the owner role.
pub async fn migrate_store(opts: &PgOptions) -> Result<MigrateReport, MigrateError> {
    migrate_store_with(opts, MIGRATIONS).await
}

/// Like [`migrate_store`] with a list of migrations. For the tests of 5.36 (SP-02, SP-06).
#[doc(hidden)]
pub async fn migrate_store_with(opts: &PgOptions, migrations: &[Migration]) -> Result<MigrateReport, MigrateError> {
    let tls = conn::build_tls(&opts.sslmode).map_err(|e| MigrateError::Connect(e.message))?;
    let direct = conn::connect_direct(opts, &tls).await.map_err(|e| MigrateError::Connect(describe(&e)))?;
    let c = &direct.client;
    // Step 3: the encoding.
    let encoding: String = match c.query_opt(sql::P1, &[]).await.map_err(dberr)? {
        Some(r) => r.try_get(0).map_err(dberr)?,
        None => String::new(),
    };
    if !encoding.eq_ignore_ascii_case("UTF8") {
        return Err(MigrateError::Encoding(encoding));
    }
    // Step 4: the lock.
    let locked: bool = match c.query_opt(sql::P28, &[]).await.map_err(dberr)? {
        Some(r) => r.try_get(0).map_err(dberr)?,
        None => false,
    };
    if !locked {
        return Err(MigrateError::LockHeld);
    }
    let result = run_steps(&direct, migrations).await;
    unlock(&direct).await;
    result
}

async fn run_steps(d: &Direct, migrations: &[Migration]) -> Result<MigrateReport, MigrateError> {
    let c = &d.client;
    // Step 5: the version of the database.
    let has_table: bool = match c.query_opt(sql::P2, &[]).await.map_err(dberr)? {
        Some(r) => r.try_get(0).map_err(dberr)?,
        None => false,
    };
    let from: i32 = if has_table {
        match c.query_opt(sql::P3, &[]).await.map_err(dberr)? {
            Some(r) => r.try_get(0).map_err(dberr)?,
            None => 0,
        }
    } else {
        0
    };
    // Step 6: the version of the binary.
    let binary = migrations.last().map_or(0, |m| m.version);
    if from > binary {
        return Err(MigrateError::Newer { database: from, binary });
    }
    // Steps 7 and 8.
    let mut applied = Vec::new();
    for k in (from + 1)..=binary {
        let Some(step) = migrations.iter().find(|m| m.version == k) else {
            return Err(MigrateError::Database(format!("the binary has no migration {k}")));
        };
        let outcome = async {
            c.batch_execute("BEGIN").await?;
            c.batch_execute(step.sql).await?;
            c.batch_execute("COMMIT").await
        }
        .await;
        if let Err(e) = outcome {
            drop(c.batch_execute("ROLLBACK").await);
            return Err(MigrateError::Step { version: k, message: describe(&e) });
        }
        applied.push(k);
    }
    Ok(MigrateReport { from, to: from.max(binary), applied })
}
