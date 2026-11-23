// SPDX-License-Identifier: MIT OR Apache-2.0
//! The probe and the state of a PostgreSQL source (8a.12, 8.11).
//!
//! The probe opens a connection and runs the setup check R0 and the account checks R1 to R9
//! (8a.13) inside one `BEGIN READ ONLY` transaction. It ends with `ROLLBACK`. The statements are
//! the constants of `probe_sql`. The decision about each result is a pure function over the rows
//! (`check_r0`, `check_r1`, `check_names`, `check_r3` and the `advice_*` functions), so a unit
//! test can give fake rows for each check, pass and fail.
//!
//! # Verdicts
//!
//! - A failed mandatory check gives a [`Failure`]. The state is `misconfigured`. The `detail` names
//!   the check, for example `R4`, and the first names (at most five). It holds no secret and no
//!   value of the data: it holds names of the catalog only.
//! - The run stops at the first failed check, in the order R0 to R9. It reads no advice then.
//! - The advice checks A1 to A4 never change the state. [`advice_changes`] gives the
//!   advice that differs from the last probe, so the caller writes one `source.advice` line for
//!   each change. The line holds `source`, `check` ([`Advice::check`]) and `names`.
//! - A fault of the probe gives `down` for a transport fault, a timeout and a server error of the
//!   classes 08, 57P0x and 53300. Each other server error of a probe statement gives `misconfigured`
//!   (finding M2). A fault never moves a `misconfigured` source out of this state: only a probe that
//!   passes does ([`finish`], [`sticky`]).
//! - A server below the oldest supported version fails R0 (finding I7).
//!
//! The whole probe has the deadline [`probe_deadline`]. The caller of [`run_probe`] applies it.

use std::time::Duration;

use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, GenericClient};

use super::errors;
use super::probe_sql as sql;
use crate::types::{Probe, SourceState, cut_utf8};

/// The oldest supported major version, as `server_version_num` (8a.14).
pub(crate) const OLDEST_SUPPORTED_VERSION_NUM: i32 = 150_000;

/// The time that the whole probe has after the connection setup (8a.12).
const PROBE_EXTRA: Duration = Duration::from_secs(10);

/// Gives the deadline of the whole probe: `connect_timeout` plus 10 s (8a.12).
pub(crate) fn probe_deadline(connect_timeout: Duration) -> Duration {
    connect_timeout + PROBE_EXTRA
}

/// The longest name that a detail or a log line holds, in bytes.
const NAME_MAX_BYTES: usize = 140;

/// One mandatory check of 8a.12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Check {
    /// The session parameters.
    R0,
    /// The attributes of the role.
    R1,
    /// The memberships.
    R2,
    /// `CREATE` on the database and on the schemas.
    R3,
    /// Write privileges on tables, views and foreign tables.
    R4,
    /// `UPDATE` on sequences.
    R5,
    /// Write privileges on columns.
    R6,
    /// `dblink` and the privileged functions.
    R7,
    /// Foreign data wrappers and servers.
    R8,
    /// The objects that the role owns.
    R9,
}

impl Check {
    /// Gives the name of the check, as the log line `reason` writes it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Check::R0 => "R0",
            Check::R1 => "R1",
            Check::R2 => "R2",
            Check::R3 => "R3",
            Check::R4 => "R4",
            Check::R5 => "R5",
            Check::R6 => "R6",
            Check::R7 => "R7",
            Check::R8 => "R8",
            Check::R9 => "R9",
        }
    }

    /// Gives the sentence that tells what the check found, to put before the names.
    fn finding(self) -> &'static str {
        match self {
            Check::R0 => "a session parameter is wrong",
            Check::R1 => "the role has a forbidden attribute",
            Check::R2 => "the role is a member of a role that it MUST NOT be in",
            Check::R3 => "the role can create objects in",
            Check::R4 => "the role can change the table or view",
            Check::R5 => "the role can change the sequence",
            Check::R6 => "the role can write the column",
            Check::R7 => "the role can run the function",
            Check::R8 => "the role can use the foreign data wrapper or server",
            Check::R9 => "the role, or a role that it is a member of, owns",
        }
    }

    /// Gives the sentence that tells the operator how to close the finding (8a.13).
    fn fix(self) -> &'static str {
        match self {
            Check::R0 => "Connect with the startup parameters of 8a.4.",
            Check::R1 => "Remove the attribute with ALTER ROLE.",
            Check::R2 => "Revoke the membership.",
            Check::R3 => "Revoke the CREATE privilege from the role and from the roles that it is a member of.",
            Check::R4 => "Revoke INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES and TRIGGER.",
            Check::R5 => "Revoke UPDATE on the sequence.",
            Check::R6 => "Revoke INSERT, UPDATE and REFERENCES on the column.",
            Check::R7 => {
                "Revoke EXECUTE from the role and from PUBLIC, or do not install the extension in this database."
            }
            Check::R8 => "Revoke USAGE.",
            Check::R9 => "Change the owner of the object to a role that the account is not a member of.",
        }
    }
}

/// A failed mandatory check. The state of the source becomes `misconfigured`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failure {
    /// The check that failed.
    pub check: Check,
    /// One English text. It names the check and the first names.
    pub detail: String,
}

impl Failure {
    fn names(check: Check, names: &[String]) -> Failure {
        let list = join_names(names);
        Failure { check, detail: format!("{}: {} {}. {}", check.as_str(), check.finding(), list, check.fix()) }
    }

    fn text(check: Check, text: String) -> Failure {
        Failure { check, detail: format!("{}: {}", check.as_str(), text) }
    }
}

/// Makes a name safe for a detail and a log line: no control character, at most 140 bytes.
pub(crate) fn clean_name(name: &str) -> String {
    let safe: String = name.chars().map(|c| if c.is_control() { '?' } else { c }).collect();
    cut_utf8(&safe, NAME_MAX_BYTES).to_owned()
}

/// Keeps the first five names, cleaned.
fn first_names(names: &[String]) -> Vec<String> {
    names.iter().take(sql::MAX_NAMES).map(|n| clean_name(n)).collect()
}

fn join_names(names: &[String]) -> String {
    first_names(names).join(", ")
}

/// The values of the row that R0 reads (8a.12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SetupRow {
    /// `server_version_num`.
    pub version_num: i32,
    /// `default_transaction_read_only`.
    pub default_transaction_read_only: String,
    /// `standard_conforming_strings`.
    pub standard_conforming_strings: String,
    /// `client_encoding`.
    pub client_encoding: String,
    /// `TimeZone`.
    pub time_zone: String,
    /// `search_path`.
    pub search_path: String,
}

/// Gives the `search_path` that R0 expects: the items of the key joined with a comma, without any
/// blank (8a.12).
pub(crate) fn expected_search_path(items: &[String]) -> String {
    strip_blanks(&items.join(","))
}

fn strip_blanks(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// R0: the server version and the session parameters. A version below the oldest supported one fails
/// (finding I7). `expected_search_path` is the value of [`expected_search_path`].
pub(crate) fn check_r0(row: &SetupRow, expected_search_path: &str) -> Result<(), Failure> {
    if row.version_num < OLDEST_SUPPORTED_VERSION_NUM {
        return Err(Failure::text(
            Check::R0,
            format!(
                "the server is PostgreSQL {} and Hune supports {} and later. Connect to a supported server.",
                version_text(row.version_num),
                version_text(OLDEST_SUPPORTED_VERSION_NUM)
            ),
        ));
    }
    let wrong = |name: &str, value: &str, expected: &str| {
        Failure::text(
            Check::R0,
            format!(
                "the session parameter {name} is \"{}\" and MUST be \"{}\". {}",
                clean_name(value),
                clean_name(expected),
                Check::R0.fix()
            ),
        )
    };
    if row.default_transaction_read_only != "on" {
        return Err(wrong("default_transaction_read_only", &row.default_transaction_read_only, "on"));
    }
    if row.standard_conforming_strings != "on" {
        return Err(wrong("standard_conforming_strings", &row.standard_conforming_strings, "on"));
    }
    if row.client_encoding != "UTF8" {
        return Err(wrong("client_encoding", &row.client_encoding, "UTF8"));
    }
    if row.time_zone != "UTC" {
        return Err(wrong("TimeZone", &row.time_zone, "UTC"));
    }
    if strip_blanks(&row.search_path) != expected_search_path {
        return Err(wrong("search_path", &row.search_path, expected_search_path));
    }
    Ok(())
}

/// The five attributes that R1 reads. A `true` is a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RoleAttributes {
    /// `rolsuper`.
    pub superuser: bool,
    /// `rolcreaterole`.
    pub createrole: bool,
    /// `rolcreatedb`.
    pub createdb: bool,
    /// `rolreplication`.
    pub replication: bool,
    /// `rolbypassrls`.
    pub bypassrls: bool,
}

/// R1: the attributes of the role. It needs one row with five `false`.
pub(crate) fn check_r1(rows: &[RoleAttributes]) -> Result<(), Failure> {
    let [row] = rows else {
        return Err(Failure::text(
            Check::R1,
            format!("pg_roles gave {} rows for the current role, and one row is necessary.", rows.len()),
        ));
    };
    let attributes: Vec<&str> = [
        (row.superuser, "SUPERUSER"),
        (row.createrole, "CREATEROLE"),
        (row.createdb, "CREATEDB"),
        (row.replication, "REPLICATION"),
        (row.bypassrls, "BYPASSRLS"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    if attributes.is_empty() {
        Ok(())
    } else {
        Err(Failure::names(Check::R1, &attributes.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>()))
    }
}

/// The checks R2, R4 to R9 pass when the statement gives no row. Each row is a name
/// that the detail shows.
pub(crate) fn check_names(check: Check, names: &[String]) -> Result<(), Failure> {
    if names.is_empty() { Ok(()) } else { Err(Failure::names(check, names)) }
}

/// R3: the role has no `CREATE` on a schema and none on the database.
///
/// `database_create` is true when the second statement of R3 gave its row.
pub(crate) fn check_r3(schemas: &[String], database_create: bool) -> Result<(), Failure> {
    let mut names: Vec<String> = Vec::new();
    if database_create {
        names.push("the database".to_owned());
    }
    for s in schemas {
        names.push(format!("schema {s}"));
    }
    if names.is_empty() { Ok(()) } else { Err(Failure::names(Check::R3, &names)) }
}

/// One advice check of 8a.12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdviceCheck {
    /// A language that runs `DO` blocks.
    A1,
    /// The `TEMPORARY` privilege on the database.
    A2,
    /// A function that the grammar refuses and the role can run.
    A3,
    /// `temp_file_limit` is `-1`.
    A4,
}

impl AdviceCheck {
    /// Gives the name of the check, for the field `check` of the log line.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            AdviceCheck::A1 => "A1",
            AdviceCheck::A2 => "A2",
            AdviceCheck::A3 => "A3",
            AdviceCheck::A4 => "A4",
        }
    }

    /// Gives the English advice of 8a.12.
    #[allow(
        dead_code,
        reason = "The server writes the advice text from `AdviceChange::check`; the tests read this text."
    )]
    pub(crate) fn advice(self) -> &'static str {
        match self {
            AdviceCheck::A1 => "The role can run DO blocks. Revoke USAGE on the language.",
            AdviceCheck::A2 => "The role can make temporary tables, if a session is read-write.",
            AdviceCheck::A3 => "The role can call a function that the grammar refuses. Revoke EXECUTE from PUBLIC.",
            AdviceCheck::A4 => {
                "temp_file_limit is -1: a query can fill the disk with temporary files. Set temp_file_limit for the role."
            }
        }
    }
}

/// One failed advice check, with its first names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Advice {
    /// The check.
    pub check: AdviceCheck,
    /// The first names (at most five), cleaned.
    pub names: Vec<String>,
}

impl Advice {
    /// Gives the field `check` of the log line `source.advice`.
    pub(crate) fn check(&self) -> &'static str {
        self.check.as_str()
    }
}

fn advice_of(check: AdviceCheck, names: &[String]) -> Option<Advice> {
    (!names.is_empty()).then(|| Advice { check, names: first_names(names) })
}

/// A1: the languages with `DO` that the role can use. No row passes.
pub(crate) fn advice_a1(languages: &[String]) -> Option<Advice> {
    advice_of(AdviceCheck::A1, languages)
}

/// A2: the statement gives one row (`temp`) when the role has `TEMPORARY`.
pub(crate) fn advice_a2(rows: &[String]) -> Option<Advice> {
    advice_of(AdviceCheck::A2, rows)
}

/// A3: the refused functions that the role can run. No row passes.
pub(crate) fn advice_a3(functions: &[String]) -> Option<Advice> {
    advice_of(AdviceCheck::A3, functions)
}

/// A4: `temp_file_limit` has the value `-1`.
pub(crate) fn advice_a4(value: &str) -> Option<Advice> {
    (value.trim() == "-1").then(|| Advice { check: AdviceCheck::A4, names: vec!["temp_file_limit=-1".to_owned()] })
}

/// Gives the advice of `current` that is new or has other names than in `previous`. The caller
/// writes one `source.advice` line for each of them (8a.12): the lines come one time for each
/// change, not at each probe.
pub(crate) fn advice_changes<'a>(previous: &[Advice], current: &'a [Advice]) -> Vec<&'a Advice> {
    current.iter().filter(|a| !previous.contains(a)).collect()
}

/// Gives the version text of `server_version_num`: 150004 gives `15.4`.
pub(crate) fn version_text(version_num: i32) -> String {
    format!("{}.{}", version_num / 10_000, version_num % 100)
}

/// What the run of the checks found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Report {
    /// `server_version_num`, read by R0.
    pub version_num: i32,
    /// The first failed mandatory check, or `None`.
    pub failure: Option<Failure>,
    /// The failed advice checks. It is empty when a mandatory check failed.
    pub advice: Vec<Advice>,
}

/// The result of one probe run, before the state is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The checks ran to the end, or to the first failure.
    Checked(Report),
    /// The probe stopped on a fault. `sqlstate` is `None` for a transport fault or a timeout.
    Fault {
        /// The SQLSTATE of the server error.
        sqlstate: Option<String>,
    },
}

impl Outcome {
    /// Makes the outcome of an error of `tokio-postgres`.
    pub(crate) fn from_error(error: &tokio_postgres::Error) -> Outcome {
        Outcome::Fault { sqlstate: errors::sqlstate_of(error) }
    }
}

/// Tells if a server error of a probe statement can be a fault of the server or of the link, and not
/// a wrong account or a wrong setup: the class 08, the codes 57P0x and 53300. Each other SQLSTATE of
/// a probe statement fails closed and gives `misconfigured` (finding M2).
fn fault_is_transient(sqlstate: &str) -> bool {
    sqlstate.starts_with("08") || sqlstate.starts_with("57P0") || sqlstate == "53300"
}

/// Keeps a `misconfigured` source in this state when a probe has a fault. Only a probe that
/// passes gives another state (finding M2): a fault gives no information about the account.
pub(crate) fn sticky(previous: SourceState, state: SourceState) -> SourceState {
    if previous == SourceState::Misconfigured && state == SourceState::Down {
        SourceState::Misconfigured
    } else {
        state
    }
}

/// Makes the [`Probe`] of an outcome (8a.12). `previous` is the state before the probe.
pub(crate) fn finish(previous: SourceState, outcome: &Outcome) -> Probe {
    match outcome {
        Outcome::Checked(report) => {
            let (state, detail) = match &report.failure {
                None => (SourceState::Up, None),
                Some(f) => (SourceState::Misconfigured, Some(f.detail.clone())),
            };
            Probe {
                state,
                previous,
                version: Some(version_text(report.version_num)),
                version_supported: Some(report.version_num >= OLDEST_SUPPORTED_VERSION_NUM),
                detail,
            }
        }
        Outcome::Fault { sqlstate } => {
            let (raw, mut detail) = match sqlstate.as_deref() {
                Some(s) if !fault_is_transient(s) => (
                    SourceState::Misconfigured,
                    format!(
                        "The probe statement failed (SQLSTATE {s}). The account or the server breaks the contract of the source."
                    ),
                ),
                Some(s) => (SourceState::Down, format!("The probe failed (SQLSTATE {s}).")),
                None => (SourceState::Down, "The probe did not get an answer from the server.".to_owned()),
            };
            let state = sticky(previous, raw);
            if raw == SourceState::Down && state == SourceState::Misconfigured {
                detail.push_str(" The source stays misconfigured until a probe passes.");
            }
            Probe { state, previous, version: None, version_supported: None, detail: Some(detail) }
        }
    }
}

async fn names<C: GenericClient + Sync>(
    client: &C,
    statement: &'static str,
    params: &[&(dyn ToSql + Sync)],
) -> Result<Vec<String>, tokio_postgres::Error> {
    let rows = client.query(statement, params).await?;
    rows.iter().map(|row| row.try_get::<_, String>(0)).collect()
}

/// Reads the setup row of R0.
pub(crate) async fn read_setup<C: GenericClient + Sync>(client: &C) -> Result<SetupRow, tokio_postgres::Error> {
    let row = client.query_one(sql::R0, &[]).await?;
    Ok(SetupRow {
        version_num: row.try_get(0)?,
        default_transaction_read_only: row.try_get(1)?,
        standard_conforming_strings: row.try_get(2)?,
        client_encoding: row.try_get(3)?,
        time_zone: row.try_get(4)?,
        search_path: row.try_get(5)?,
    })
}

/// Runs R0 to R9 and then A1 to A4 on `client`. The caller makes sure that `client` is in the
/// read-only transaction ([`run_probe`] does it). The function stops at the first failed
/// mandatory check.
///
/// # Errors
///
/// Gives the error of `tokio-postgres` when a statement fails: the probe has then a fault, not a
/// verdict.
pub(crate) async fn run_checks<C: GenericClient + Sync>(
    client: &C,
    search_path: &[String],
) -> Result<Report, tokio_postgres::Error> {
    let setup = read_setup(client).await?;
    let version_num = setup.version_num;
    let report = |failure: Failure| Report { version_num, failure: Some(failure), advice: Vec::new() };

    if let Err(f) = check_r0(&setup, &expected_search_path(search_path)) {
        return Ok(report(f));
    }

    let rows = client.query(sql::R1, &[]).await?;
    let attributes = rows
        .iter()
        .map(|row| {
            Ok(RoleAttributes {
                superuser: row.try_get(0)?,
                createrole: row.try_get(1)?,
                createdb: row.try_get(2)?,
                replication: row.try_get(3)?,
                bypassrls: row.try_get(4)?,
            })
        })
        .collect::<Result<Vec<_>, tokio_postgres::Error>>()?;
    if let Err(f) = check_r1(&attributes) {
        return Ok(report(f));
    }

    if let Err(f) = check_names(Check::R2, &names(client, sql::R2, &[]).await?) {
        return Ok(report(f));
    }

    let schemas = names(client, sql::R3_SCHEMAS, &[]).await?;
    let database_create = !names(client, sql::R3_DATABASE, &[]).await?.is_empty();
    if let Err(f) = check_r3(&schemas, database_create) {
        return Ok(report(f));
    }

    for (check, statement) in [(Check::R4, sql::R4), (Check::R5, sql::R5), (Check::R6, sql::R6)] {
        if let Err(f) = check_names(check, &names(client, statement, &[]).await?) {
            return Ok(report(f));
        }
    }

    let functions: Vec<&str> = sql::R7_FUNCTIONS.to_vec();
    let anywhere: Vec<&str> = sql::R7_ANYWHERE.to_vec();
    if let Err(f) = check_names(Check::R7, &names(client, sql::R7, &[&functions, &anywhere]).await?) {
        return Ok(report(f));
    }
    for (check, statement) in [(Check::R8, sql::R8), (Check::R9, sql::R9)] {
        if let Err(f) = check_names(check, &names(client, statement, &[]).await?) {
            return Ok(report(f));
        }
    }

    let mut advice = Vec::new();
    advice.extend(advice_a1(&names(client, sql::A1, &[]).await?));
    advice.extend(advice_a2(&names(client, sql::A2, &[]).await?));
    advice.extend(advice_a3(&names(client, sql::A3, &[]).await?));
    let limit = names(client, sql::A4, &[]).await?;
    advice.extend(limit.first().and_then(|v| advice_a4(v)));
    Ok(Report { version_num, failure: None, advice })
}

/// Runs the probe on `client` inside one `BEGIN READ ONLY` transaction and ends with `ROLLBACK`
/// (8a.12). The transaction never commits: the function drops its effects in each case.
///
/// # Errors
///
/// Gives the error of `tokio-postgres` for a fault of the connection or a refused statement.
pub(crate) async fn run_probe(client: &mut Client, search_path: &[String]) -> Result<Report, tokio_postgres::Error> {
    let tx = client.build_transaction().read_only(true).start().await?;
    let result = run_checks(&tx, search_path).await;
    let rolled_back = tx.rollback().await;
    let report = result?;
    rolled_back?;
    Ok(report)
}

#[cfg(test)]
#[path = "probe/tests/container_tests.rs"]
mod container_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|i| (*i).to_owned()).collect()
    }

    fn good_setup() -> SetupRow {
        SetupRow {
            version_num: 180_001,
            default_transaction_read_only: "on".into(),
            standard_conforming_strings: "on".into(),
            client_encoding: "UTF8".into(),
            time_zone: "UTC".into(),
            search_path: "metrics".into(),
        }
    }

    // R0.

    #[test]
    fn r0_passes_on_the_table_values() {
        assert_eq!(check_r0(&good_setup(), "metrics"), Ok(()));
    }

    #[test]
    fn r0_fails_for_each_wrong_value() {
        type Edit = fn(&mut SetupRow);
        let cases: [(Edit, &str); 5] = [
            (|r| r.default_transaction_read_only = "off".into(), "default_transaction_read_only"),
            (|r| r.standard_conforming_strings = "off".into(), "standard_conforming_strings"),
            (|r| r.client_encoding = "LATIN1".into(), "client_encoding"),
            (|r| r.time_zone = "Europe/Paris".into(), "TimeZone"),
            (|r| r.search_path = "public".into(), "search_path"),
        ];
        for (edit, name) in cases {
            let mut row = good_setup();
            edit(&mut row);
            let f = check_r0(&row, "metrics").unwrap_err();
            assert_eq!(f.check, Check::R0);
            assert!(f.detail.starts_with("R0: "), "{}", f.detail);
            assert!(f.detail.contains(name), "{}", f.detail);
        }
    }

    #[test]
    fn r0_is_exact_for_the_case_of_the_values() {
        for (field, value) in [("ro", "ON"), ("enc", "utf8"), ("tz", "utc")] {
            let mut row = good_setup();
            match field {
                "ro" => row.default_transaction_read_only = value.into(),
                "enc" => row.client_encoding = value.into(),
                _ => row.time_zone = value.into(),
            }
            assert!(check_r0(&row, "metrics").is_err(), "{field}");
        }
    }

    #[test]
    fn r0_compares_the_search_path_without_blanks() {
        let items = s(&["metrics", "public"]);
        let expected = expected_search_path(&items);
        assert_eq!(expected, "metrics,public");
        let mut row = good_setup();
        row.search_path = "metrics, public".into();
        assert_eq!(check_r0(&row, &expected), Ok(()));
        row.search_path = "public, metrics".into();
        assert!(check_r0(&row, &expected).is_err());
        assert_eq!(expected_search_path(&s(&[" a ", "b "])), "a,b");
    }

    // R1.

    fn plain() -> RoleAttributes {
        RoleAttributes { superuser: false, createrole: false, createdb: false, replication: false, bypassrls: false }
    }

    #[test]
    fn r1_passes_with_five_false() {
        assert_eq!(check_r1(&[plain()]), Ok(()));
    }

    #[test]
    fn r1_fails_for_each_attribute() {
        let cases = [
            (RoleAttributes { superuser: true, ..plain() }, "SUPERUSER"),
            (RoleAttributes { createrole: true, ..plain() }, "CREATEROLE"),
            (RoleAttributes { createdb: true, ..plain() }, "CREATEDB"),
            (RoleAttributes { replication: true, ..plain() }, "REPLICATION"),
            (RoleAttributes { bypassrls: true, ..plain() }, "BYPASSRLS"),
        ];
        for (row, name) in cases {
            let f = check_r1(&[row]).unwrap_err();
            assert_eq!(f.check, Check::R1);
            assert!(f.detail.starts_with("R1: ") && f.detail.contains(name), "{}", f.detail);
        }
        let f = check_r1(&[RoleAttributes { superuser: true, createdb: true, ..plain() }]).unwrap_err();
        assert!(f.detail.contains("SUPERUSER, CREATEDB"), "{}", f.detail);
    }

    #[test]
    fn r1_fails_without_one_row() {
        assert_eq!(check_r1(&[]).unwrap_err().check, Check::R1);
        assert_eq!(check_r1(&[plain(), plain()]).unwrap_err().check, Check::R1);
    }

    // The checks that list names: R2, R4 to R8.

    #[test]
    fn name_checks_pass_with_no_row() {
        for check in [Check::R2, Check::R4, Check::R5, Check::R6, Check::R7, Check::R8, Check::R9] {
            assert_eq!(check_names(check, &[]), Ok(()), "{check:?}");
        }
    }

    #[test]
    fn name_checks_fail_with_a_row_and_name_the_check_and_the_object() {
        let cases = [
            (Check::R2, "pg_read_all_data", "R2"),
            (Check::R4, "metrics.events", "R4"),
            (Check::R5, "metrics.events_id_seq", "R5"),
            (Check::R6, "metrics.events.value", "R6"),
            (Check::R7, "dblink_connect(text)", "R7"),
            (Check::R8, "postgres_fdw", "R8"),
            (Check::R9, "table metrics.owned", "R9"),
        ];
        for (check, name, tag) in cases {
            let f = check_names(check, &s(&[name])).unwrap_err();
            assert_eq!(f.check, check);
            assert!(f.detail.starts_with(&format!("{tag}: ")), "{}", f.detail);
            assert!(f.detail.contains(name), "{}", f.detail);
        }
    }

    #[test]
    fn a_detail_holds_five_names_at_most_and_no_control_character() {
        let many = s(&["a", "b", "c", "d", "e", "f", "g"]);
        let f = check_names(Check::R4, &many).unwrap_err();
        assert!(f.detail.contains("a, b, c, d, e") && !f.detail.contains(", f"), "{}", f.detail);
        let f = check_names(Check::R4, &s(&["bad\nname\u{1b}[31m"])).unwrap_err();
        assert!(!f.detail.contains('\n') && !f.detail.contains('\u{1b}'), "{:?}", f.detail);
        let long = "x".repeat(1000);
        let f = check_names(Check::R4, &[long]).unwrap_err();
        assert!(f.detail.len() < 400);
    }

    // R3.

    #[test]
    fn r3_passes_with_no_create_right() {
        assert_eq!(check_r3(&[], false), Ok(()));
    }

    #[test]
    fn r3_fails_for_a_schema_and_for_the_database() {
        let f = check_r3(&s(&["metrics"]), false).unwrap_err();
        assert_eq!(f.check, Check::R3);
        assert!(f.detail.contains("schema metrics"), "{}", f.detail);
        let f = check_r3(&[], true).unwrap_err();
        assert!(f.detail.starts_with("R3: ") && f.detail.contains("the database"), "{}", f.detail);
        let f = check_r3(&s(&["public"]), true).unwrap_err();
        assert!(f.detail.contains("the database, schema public"), "{}", f.detail);
    }

    // Advice.

    #[test]
    fn advice_checks_pass_on_empty_results() {
        assert_eq!(advice_a1(&[]), None);
        assert_eq!(advice_a2(&[]), None);
        assert_eq!(advice_a3(&[]), None);
        assert_eq!(advice_a4("1GB"), None);
        assert_eq!(advice_a4("1048576kB"), None);
        assert_eq!(advice_a4("0"), None);
    }

    #[test]
    fn advice_checks_fire_and_hold_the_first_names() {
        let a = advice_a1(&s(&["plpgsql"])).unwrap();
        assert_eq!((a.check(), a.names.as_slice()), ("A1", &["plpgsql".to_owned()][..]));
        assert_eq!(advice_a2(&s(&["temp"])).unwrap().check(), "A2");
        let many: Vec<String> = (0..9).map(|i| format!("pg_sleep({i})")).collect();
        let a = advice_a3(&many).unwrap();
        assert_eq!((a.check(), a.names.len()), ("A3", 5));
        let a = advice_a4("-1").unwrap();
        assert_eq!(a.check(), "A4");
        assert_eq!(advice_a4(" -1 ").unwrap().check, AdviceCheck::A4);
        for c in [AdviceCheck::A1, AdviceCheck::A2, AdviceCheck::A3, AdviceCheck::A4] {
            assert!(c.advice().len() > 20);
        }
    }

    #[test]
    fn advice_lines_come_one_time_for_each_change() {
        let a1 = advice_a1(&s(&["plpgsql"])).unwrap();
        let a2 = advice_a2(&s(&["temp"])).unwrap();
        // First probe: both are new.
        assert_eq!(advice_changes(&[], &[a1.clone(), a2.clone()]), vec![&a1, &a2]);
        // Same result: no line.
        assert!(advice_changes(&[a1.clone(), a2.clone()], &[a1.clone(), a2.clone()]).is_empty());
        // One closed: no line for it, none for the other.
        assert!(advice_changes(&[a1.clone(), a2.clone()], std::slice::from_ref(&a1)).is_empty());
        // Other names: one line.
        let a1b = advice_a1(&s(&["plpgsql", "plperl"])).unwrap();
        assert_eq!(advice_changes(std::slice::from_ref(&a1), std::slice::from_ref(&a1b)), vec![&a1b]);
        // An advice that comes back after it left is a change.
        assert_eq!(advice_changes(&[], std::slice::from_ref(&a1)), vec![&a1]);
    }

    // Versions and states.

    #[test]
    fn version_text_and_support() {
        assert_eq!(version_text(150_004), "15.4");
        assert_eq!(version_text(180_001), "18.1");
        assert_eq!(version_text(140_012), "14.12");
    }

    fn report(version_num: i32, failure: Option<Failure>) -> Outcome {
        Outcome::Checked(Report { version_num, failure, advice: Vec::new() })
    }

    #[test]
    fn a_passed_probe_is_up() {
        let p = finish(SourceState::Unknown, &report(180_001, None));
        assert_eq!(p.state, SourceState::Up);
        assert_eq!(p.previous, SourceState::Unknown);
        assert!(p.changed());
        assert_eq!((p.version.as_deref(), p.version_supported, p.detail), (Some("18.1"), Some(true), None));
        assert!(!finish(SourceState::Up, &report(180_001, None)).changed());
    }

    #[test]
    fn an_old_version_is_up_and_flagged() {
        let p = finish(SourceState::Up, &report(140_012, None));
        assert_eq!((p.state, p.version_supported), (SourceState::Up, Some(false)));
    }

    #[test]
    fn a_failed_check_is_misconfigured_and_names_the_check() {
        let f = check_names(Check::R4, &s(&["metrics.events"])).unwrap_err();
        let p = finish(SourceState::Up, &report(150_004, Some(f)));
        assert_eq!(p.state, SourceState::Misconfigured);
        let d = p.detail.unwrap();
        assert!(d.starts_with("R4: ") && d.contains("metrics.events"), "{d}");
        assert_eq!(p.version.as_deref(), Some("15.4"));
    }

    #[test]
    fn a_login_fault_or_a_refused_parameter_is_misconfigured() {
        for st in ["28000", "28P01", "3D000", "22023", "42704"] {
            let p = finish(SourceState::Up, &Outcome::Fault { sqlstate: Some(st.into()) });
            assert_eq!(p.state, SourceState::Misconfigured, "{st}");
            assert!(p.detail.unwrap().contains(st));
            assert_eq!((p.version, p.version_supported), (None, None));
        }
    }

    #[test]
    fn a_transport_fault_or_a_server_fault_class_is_down() {
        for st in ["08006", "08001", "57P01", "57P02", "57P03", "53300"] {
            let p = finish(SourceState::Up, &Outcome::Fault { sqlstate: Some(st.into()) });
            assert_eq!(p.state, SourceState::Down, "{st}");
            assert!(p.detail.unwrap().contains(st));
        }
        let p = finish(SourceState::Unknown, &Outcome::Fault { sqlstate: None });
        assert_eq!(p.state, SourceState::Down);
        assert!(p.detail.is_some());
    }

    #[test]
    fn another_statement_error_of_the_probe_is_misconfigured() {
        // Finding M2: the probe fails closed. A permission error on the catalog, a timeout, an internal error
        // and each other SQLSTATE give `misconfigured`, not `down`.
        for st in ["42501", "XX000", "57014", "55P03", "42P01", "53200", "40001", "0A000"] {
            let p = finish(SourceState::Up, &Outcome::Fault { sqlstate: Some(st.into()) });
            assert_eq!(p.state, SourceState::Misconfigured, "{st}");
            assert!(p.detail.unwrap().contains(st), "{st}");
        }
    }

    #[test]
    fn a_fault_never_moves_a_misconfigured_source_out_of_the_state() {
        // Finding M2: a probe of a misconfigured source that meets a fault, or a timeout, gives
        // `misconfigured`. Only a probe that passes gives `up`.
        for st in [None, Some("08006"), Some("57P01"), Some("53300")] {
            let p = finish(SourceState::Misconfigured, &Outcome::Fault { sqlstate: st.map(str::to_owned) });
            assert_eq!(p.state, SourceState::Misconfigured, "{st:?}");
            assert_eq!(p.previous, SourceState::Misconfigured);
        }
        assert_eq!(sticky(SourceState::Misconfigured, SourceState::Down), SourceState::Misconfigured);
        assert_eq!(sticky(SourceState::Up, SourceState::Down), SourceState::Down);
        assert_eq!(sticky(SourceState::Down, SourceState::Down), SourceState::Down);
        assert_eq!(sticky(SourceState::Misconfigured, SourceState::Up), SourceState::Up);
        let ok = finish(SourceState::Misconfigured, &report(180_001, None));
        assert_eq!(ok.state, SourceState::Up);
    }

    #[test]
    fn a_server_below_the_oldest_supported_version_fails_r0() {
        // Finding I7: the safer default. The version is checked first, whatever the other values are.
        let mut row = good_setup();
        row.version_num = 140_012;
        let f = check_r0(&row, "metrics").unwrap_err();
        assert_eq!(f.check, Check::R0);
        assert!(
            f.detail.starts_with("R0: ") && f.detail.contains("14.12") && f.detail.contains("15.0"),
            "{}",
            f.detail
        );
        row.version_num = 150_000;
        assert_eq!(check_r0(&row, "metrics"), Ok(()));
        row.version_num = 90_624;
        assert!(check_r0(&row, "metrics").is_err());
    }

    #[test]
    fn the_probe_deadline_is_the_connect_timeout_plus_ten_seconds() {
        assert_eq!(probe_deadline(Duration::from_secs(5)), Duration::from_secs(15));
    }

    #[test]
    fn clean_name_cuts_at_a_character_boundary() {
        let name = "é".repeat(200);
        let cleaned = clean_name(&name);
        assert!(cleaned.len() <= NAME_MAX_BYTES && cleaned.chars().all(|c| c == 'é'));
        assert_eq!(clean_name("a\tb"), "a?b");
    }
}
