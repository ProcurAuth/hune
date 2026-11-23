//! The probe and the checks of a MongoDB source (8b.12, 8b.13, 8b.14).
//!
//! The checks M0 to M4 are pure functions of the plain structures [`ServerInfo`] and [`AuthInfo`],
//! so a unit test covers them with no server (MS11). [`run_probe`] reads the structures through the
//! client and runs the checks in the order M0, M1, M2, M3, M4. The first failure is the verdict.
//!
//! | Check | Rule |
//! |---|---|
//! | M0 | The version is a version number. `hello` gives `maxWireVersion` and a `maxBsonObjectSize` of at least 16,777,216. |
//! | M1 | One user, with the `user` and the `db` equal to the keys `user` and `auth_database`. |
//! | M2 | Each role is `read` on a database of `D`. Each database of `D` has this role. |
//! | M3 | Each privilege has a `db` of `D`, no cluster resource and no `anyResource`. Each action is in [`READ_ACTIONS`]. |
//! | M4 | In the mode `replica_set`: the set name and each announced member match the block. |
//!
//! The advice checks A1 and A2 read `listCollections` for each database of `D`. They run only when
//! all mandatory checks passed, and they never change the state.
//!
//! A fault of the probe never moves a `misconfigured` source to `down` ([`sticky`]): only a probe that
//! passes gives another state. The functions do no I/O and write no log line.

use std::time::Duration;

use crate::config::{MongoHost, MongoSourceConfig, MongoTopology};
use crate::mongo::erased::ErasedClient;
use crate::mongo::errors::{self, Group};
use crate::mongo::iface::{AuthInfo, MongoError, PrivilegeResource, ServerInfo};
use crate::types::{Probe, SourceState};

/// The time that the probe has after `connect_timeout` (8b.12).
const PROBE_EXTRA: Duration = Duration::from_secs(10);

/// Gives the deadline of the whole probe: `connect_timeout` plus 10 s (8b.12).
pub(crate) fn probe_deadline(connect_timeout: Duration) -> Duration {
    connect_timeout + PROBE_EXTRA
}

/// The size that `maxBsonObjectSize` MUST have at least (M0): 16 MiB.
pub const MIN_BSON_OBJECT_SIZE: i64 = 16_777_216;

/// The actions that the role `read` of MongoDB 7.0 and 8.0 gives (M3). A privilege with another
/// action fails the check. When a release adds an action to `read`, the action joins this list in
/// the same change as the CI image (8b.14).
pub const READ_ACTIONS: [&str; 10] = [
    "changeStream",
    "collStats",
    "dbHash",
    "dbStats",
    "find",
    "killCursors",
    "listCollections",
    "listIndexes",
    "listSearchIndexes",
    "planCacheRead",
];

/// The oldest supported release (8b.14): MongoDB 7.0.
pub(crate) const OLDEST_SUPPORTED: (u32, u32) = (7, 0);

/// The longest name that a detail or a log line holds, in bytes.
const NAME_MAX_BYTES: usize = 140;

/// The most names in a detail or in an advice.
const NAMES_MAX: usize = 5;

/// One mandatory check of 8b.12.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// The version and the limits of `hello`.
    M0,
    /// The user.
    M1,
    /// The roles.
    M2,
    /// The privileges.
    M3,
    /// The members of the replica set.
    M4,
}

impl Check {
    /// Gives the name of the check, as the log line `source.misconfigured` writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Check::M0 => "M0",
            Check::M1 => "M1",
            Check::M2 => "M2",
            Check::M3 => "M3",
            Check::M4 => "M4",
        }
    }
}

/// The first failed mandatory check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The check.
    pub check: Check,
    /// The check and its first names, for the field `reason` of the log line. It holds no secret.
    pub detail: String,
}

/// Cleans a name from the server for a detail: a control character becomes `?`, and the name is cut.
fn clean(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = if c.is_control() { '?' } else { c };
        if out.len() + c.len_utf8() > NAME_MAX_BYTES {
            break;
        }
        out.push(c);
    }
    out
}

/// Sorts the names, drops the repeats, cleans them and keeps the first five.
fn first_names(names: &[String]) -> Vec<String> {
    let mut all: Vec<String> = names.iter().map(|n| clean(n)).collect();
    all.sort();
    all.dedup();
    all.truncate(NAMES_MAX);
    all
}

/// Makes the failure of a check with its names. At most five names go in the detail.
fn fail(check: Check, names: &[String]) -> Failure {
    let shown = first_names(names);
    let mut distinct: Vec<&String> = names.iter().collect();
    distinct.sort();
    distinct.dedup();
    let mut detail = format!("{}: {}", check.as_str(), shown.join(", "));
    if distinct.len() > shown.len() {
        detail.push_str(&format!(" (and {} more)", distinct.len() - shown.len()));
    }
    Failure { check, detail }
}

fn fail_text(check: Check, text: &str) -> Failure {
    Failure { check, detail: format!("{}: {text}", check.as_str()) }
}

/// A server version: `7.0.14` is `(7, 0, 14)`. A version with no patch number has the patch 0.
pub type VersionNumber = (u32, u32, u32);

/// Reads a version text: `major.minor` or `major.minor.patch`, with an optional suffix that starts
/// with `-` or `+` (for example `8.0.0-rc4`).
pub fn parse_version(text: &str) -> Option<VersionNumber> {
    let end = text.find(['-', '+']).unwrap_or(text.len());
    let mut parts = text[..end].split('.');
    let number = |p: Option<&str>| -> Option<u32> {
        let p = p?;
        if p.is_empty() || p.len() > 6 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse().ok()
    };
    let major = number(parts.next())?;
    let minor = number(parts.next())?;
    let patch = match parts.next() {
        Some(p) => number(Some(p))?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Tells if a version is at least the oldest supported release (8b.14).
pub fn version_supported(version: VersionNumber) -> bool {
    (version.0, version.1) >= OLDEST_SUPPORTED
}

/// M0: the version is a version number, and `hello` gives `maxWireVersion` and a `maxBsonObjectSize`
/// of at least 16,777,216.
///
/// # Errors
///
/// The version text is not a version number, the wire version is not above 0, or the size limit is
/// below 16 MiB.
pub fn check_m0(info: &ServerInfo) -> Result<VersionNumber, Failure> {
    let Some(version) = parse_version(&info.version) else {
        return Err(fail_text(Check::M0, "the version of the server is not a version number"));
    };
    if info.max_wire_version <= 0 {
        return Err(fail_text(Check::M0, "hello gives no maxWireVersion"));
    }
    if info.max_bson_object_size < MIN_BSON_OBJECT_SIZE {
        return Err(fail_text(Check::M0, "maxBsonObjectSize is below 16777216"));
    }
    Ok(version)
}

/// M1: one user, with the `user` and the `db` equal to the keys `user` and `auth_database`.
///
/// # Errors
///
/// The list has no user, more than one user, or a user that differs from the keys.
pub fn check_m1(auth: &AuthInfo, user: &str, auth_database: &str) -> Result<(), Failure> {
    match auth.users.as_slice() {
        [u] if u.user == user && u.db == auth_database => Ok(()),
        [] => Err(fail_text(Check::M1, "the connection has no authenticated user")),
        users => {
            let names: Vec<String> = users.iter().map(|u| format!("{}@{}", u.user, u.db)).collect();
            Err(fail(Check::M1, &names))
        }
    }
}

/// M2: each role is `read` on a database of `D`, and each database of `D` has this role.
///
/// # Errors
///
/// A role that is not `read` on a database of `D`, or a database of `D` without the role `read`.
pub fn check_m2(auth: &AuthInfo, databases: &[String]) -> Result<(), Failure> {
    let mut names: Vec<String> = auth
        .roles
        .iter()
        .filter(|r| r.role != "read" || !databases.contains(&r.db))
        .map(|r| format!("{}@{}", r.role, r.db))
        .collect();
    for db in databases {
        if !auth.roles.iter().any(|r| r.role == "read" && &r.db == db) {
            names.push(format!("missing read@{db}"));
        }
    }
    if names.is_empty() { Ok(()) } else { Err(fail(Check::M2, &names)) }
}

/// M3: each privilege has a `db` of `D`, and no cluster resource and no `anyResource`. Each action is
/// in [`READ_ACTIONS`].
///
/// # Errors
///
/// A privilege on the cluster or on a resource of another form, on a database outside `D` or on all
/// databases, or with an action that is not in the list.
pub fn check_m3(auth: &AuthInfo, databases: &[String]) -> Result<(), Failure> {
    let mut names: Vec<String> = Vec::new();
    for p in &auth.privileges {
        let place = match &p.resource {
            PrivilegeResource::Cluster => {
                names.push("cluster".to_owned());
                continue;
            }
            PrivilegeResource::Other => {
                names.push("anyResource".to_owned());
                continue;
            }
            PrivilegeResource::Namespace { db, collection } => {
                let place = format!(
                    "{}.{}",
                    if db.is_empty() { "*" } else { db },
                    if collection.is_empty() { "*" } else { collection }
                );
                if db.is_empty() || !databases.contains(db) {
                    names.push(place.clone());
                }
                place
            }
        };
        for action in &p.actions {
            if !READ_ACTIONS.contains(&action.as_str()) {
                names.push(format!("{place}:{action}"));
            }
        }
    }
    if names.is_empty() { Ok(()) } else { Err(fail(Check::M3, &names)) }
}

/// Makes the text `host:port` of a member of the block.
fn member_text(h: &MongoHost) -> String {
    format!("{}:{}", h.host, h.port)
}

/// M4: in the mode `replica_set`, `setName` equals `replica_set`, and each address that the server
/// announces is in the key `hosts`. In the mode `direct` there is no check.
///
/// The comparison of a host name ignores the case. An IPv6 address of the announcement may have
/// brackets: the comparison removes them.
///
/// # Errors
///
/// The set name differs, or the server announces an address that the block does not list.
pub fn check_m4(info: &ServerInfo, topology: &MongoTopology) -> Result<(), Failure> {
    let MongoTopology::ReplicaSet { hosts, replica_set } = topology else { return Ok(()) };
    if info.set_name.as_deref() != Some(replica_set.as_str()) {
        let found = info.set_name.as_deref().unwrap_or("none");
        return Err(fail(Check::M4, &[format!("set name {found}")]));
    }
    let listed: Vec<String> = hosts.iter().map(|h| member_text(h).to_ascii_lowercase()).collect();
    let unlisted: Vec<String> = info
        .hosts
        .iter()
        .filter(|a| !listed.contains(&a.to_ascii_lowercase().replace(['[', ']'], "")))
        .cloned()
        .collect();
    if unlisted.is_empty() { Ok(()) } else { Err(fail(Check::M4, &unlisted)) }
}

/// Runs M0 to M4 in order. Gives the version, and the first failure when there is one.
pub fn check_all(info: &ServerInfo, auth: &AuthInfo, config: &MongoSourceConfig) -> Result<VersionNumber, Failure> {
    let version = check_m0(info)?;
    check_m1(auth, &config.user, &config.auth_database)?;
    check_m2(auth, &config.databases)?;
    check_m3(auth, &config.databases)?;
    check_m4(info, &config.topology)?;
    Ok(version)
}

/// An advice check (8b.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdviceCheck {
    /// A collection of the key `collections` exists in no database of `D`.
    A1,
    /// The user can read a collection that the key `collections` does not list.
    A2,
}

impl AdviceCheck {
    /// Gives the field `check` of the log line `source.advice`.
    pub fn as_str(self) -> &'static str {
        match self {
            AdviceCheck::A1 => "A1",
            AdviceCheck::A2 => "A2",
        }
    }
}

/// An advice check that fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advice {
    /// The check.
    pub check: AdviceCheck,
    /// The first names (at most five), cleaned and in order.
    pub names: Vec<String>,
}

impl Advice {
    /// Gives the field `check` of the log line `source.advice`.
    pub fn check(&self) -> &'static str {
        self.check.as_str()
    }
}

/// Computes the advice A1 and A2 from the collections of each database of `D`.
///
/// A1 lists the names of the key `collections` that no database of `D` holds. A2 lists the collections
/// that a database of `D` holds and the key does not list, as `database.collection`. The names that
/// start with `system.` are not advice: the role `read` has them and no query names them.
pub fn advice(found: &[(String, Vec<String>)], collections: &[String]) -> Vec<Advice> {
    let mut out = Vec::new();
    let missing: Vec<String> =
        collections.iter().filter(|c| !found.iter().any(|(_, list)| list.contains(c))).cloned().collect();
    if !missing.is_empty() {
        out.push(Advice { check: AdviceCheck::A1, names: first_names(&missing) });
    }
    let unlisted: Vec<String> = found
        .iter()
        .flat_map(|(db, list)| {
            list.iter()
                .filter(|c| !c.starts_with("system.") && !collections.contains(c))
                .map(move |c| format!("{db}.{c}"))
        })
        .collect();
    if !unlisted.is_empty() {
        out.push(Advice { check: AdviceCheck::A2, names: first_names(&unlisted) });
    }
    out
}

/// Gives the advice of `current` that is new or has other names than in `previous`. The caller writes
/// one `source.advice` line for each of them (8b.12): the lines come one time for each change, not at
/// each probe.
pub fn advice_changes<'a>(previous: &[Advice], current: &'a [Advice]) -> Vec<&'a Advice> {
    current.iter().filter(|a| !previous.contains(a)).collect()
}

/// What the run of the checks found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The version that M0 read, or `None` when M0 failed.
    pub version: Option<VersionNumber>,
    /// The version text of the server, as `buildInfo` gives it.
    pub version_text: String,
    /// The first failed mandatory check, or `None`.
    pub failure: Option<Failure>,
    /// The failed advice checks. It is empty when a mandatory check failed.
    pub advice: Vec<Advice>,
}

/// The result of one probe run, before the state is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The checks ran to the end, or to the first failure.
    Checked(Report),
    /// The probe stopped on a fault.
    Fault {
        /// The group of the error (8b.8).
        group: Group,
        /// The server code, when the error had one.
        code: Option<i32>,
    },
}

impl Outcome {
    /// Makes the outcome of an error of the client.
    pub fn from_error(error: &MongoError) -> Outcome {
        Outcome::Fault { group: errors::classify(error), code: error.code }
    }
}

/// Keeps a `misconfigured` source in this state when a probe has a fault. Only a probe that passes
/// gives another state: a fault gives no information about the account.
pub fn sticky(previous: SourceState, state: SourceState) -> SourceState {
    if previous == SourceState::Misconfigured && state == SourceState::Down {
        SourceState::Misconfigured
    } else {
        state
    }
}

fn code_text(code: Option<i32>) -> String {
    code.map_or_else(String::new, |c| format!(" (code {c})"))
}

/// Makes the [`Probe`] of an outcome (8b.12). `previous` is the state before the probe.
///
/// The detail of a fault is a fixed text. It never holds text of the server.
pub fn finish(previous: SourceState, outcome: &Outcome) -> Probe {
    match outcome {
        Outcome::Checked(report) => {
            let (state, detail) = match &report.failure {
                None => (SourceState::Up, None),
                Some(f) => (SourceState::Misconfigured, Some(f.detail.clone())),
            };
            Probe {
                state,
                previous,
                version: Some(report.version_text.clone()),
                version_supported: report.version.map(version_supported),
                detail,
            }
        }
        Outcome::Fault { group, code } => {
            let c = code_text(*code);
            let (raw, mut detail) = match group {
                Group::Misconfigured | Group::AccessDenied => (
                    SourceState::Misconfigured,
                    format!(
                        "The login or the certificate check failed{c}. The account or the link breaks the contract of the source."
                    ),
                ),
                Group::Other | Group::Memory => (
                    SourceState::Misconfigured,
                    format!("A probe command failed{c}. The account or the server breaks the contract of the source."),
                ),
                Group::Timeout | Group::Unavailable | Group::BadResponse | Group::Killed | Group::Guard => {
                    (SourceState::Down, format!("The probe did not get a good answer from the server{c}."))
                }
            };
            let state = sticky(previous, raw);
            if raw == SourceState::Down && state == SourceState::Misconfigured {
                detail.push_str(" The source stays misconfigured until a probe passes.");
            }
            Probe { state, previous, version: None, version_supported: None, detail: Some(detail) }
        }
    }
}

/// Runs the probe on a client (8b.12): `hello` and `buildInfo`, `connectionStatus`, the checks M0 to
/// M4, and then the advice with `listCollections` when no mandatory check failed.
///
/// # Errors
///
/// A command of the probe failed. The caller maps the error with [`Outcome::from_error`].
pub(crate) async fn run_probe(client: &dyn ErasedClient, config: &MongoSourceConfig) -> Result<Report, MongoError> {
    let info = client.server_info().await?;
    let version_text = info.version.clone();
    let version = match check_m0(&info) {
        Ok(v) => Some(v),
        Err(failure) => {
            return Ok(Report { version: None, version_text, failure: Some(failure), advice: Vec::new() });
        }
    };
    let auth = client.connection_status().await?;
    if let Err(failure) = check_all(&info, &auth, config) {
        return Ok(Report { version, version_text, failure: Some(failure), advice: Vec::new() });
    }
    let mut found = Vec::with_capacity(config.databases.len());
    for db in &config.databases {
        found.push((db.clone(), client.list_collections(db).await?));
    }
    Ok(Report { version, version_text, failure: None, advice: advice(&found, &config.collections) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MongoTlsMode;
    use crate::mongo::iface::{AuthRole, AuthUser, MongoErrorClass, Privilege};
    use hune_core::ids::SourceId;
    use hune_core::mongo::ReadPreference;

    fn config(topology: MongoTopology) -> MongoSourceConfig {
        MongoSourceConfig {
            id: SourceId::parse("events").unwrap(),
            label: "Events".to_owned(),
            topology,
            user: "hune_ro".to_owned(),
            password: "p".to_owned(),
            auth_database: "admin".to_owned(),
            databases: vec!["analytics".to_owned()],
            collections: vec!["node_load".to_owned(), "node_events".to_owned()],
            read_preference: ReadPreference::Primary,
            tls_mode: MongoTlsMode::Disable,
            tls: None,
            tls_ca_file: None,
            connect_timeout: Duration::from_secs(5),
            max_execution_time: Duration::from_secs(60),
            max_result_rows: 100,
            max_result_bytes: 1 << 20,
            max_query_size: 1 << 20,
            max_concurrent: 4,
            queue_wait: Duration::from_secs(1),
            max_concurrent_per_subject: None,
        }
    }

    fn direct() -> MongoTopology {
        MongoTopology::Direct { host: "h".to_owned(), port: 27017 }
    }

    fn replica(hosts: &[(&str, u16)], name: &str) -> MongoTopology {
        MongoTopology::ReplicaSet {
            hosts: hosts.iter().map(|(h, p)| MongoHost { host: (*h).to_owned(), port: *p }).collect(),
            replica_set: name.to_owned(),
        }
    }

    fn info() -> ServerInfo {
        ServerInfo {
            version: "7.0.14".to_owned(),
            max_wire_version: 21,
            max_bson_object_size: 16_777_216,
            set_name: None,
            hosts: vec![],
            is_mongos: false,
        }
    }

    fn privilege(db: &str, collection: &str, actions: &[&str]) -> Privilege {
        Privilege {
            resource: PrivilegeResource::Namespace { db: db.to_owned(), collection: collection.to_owned() },
            actions: actions.iter().map(|a| (*a).to_owned()).collect(),
        }
    }

    /// The account of 8b.13: the role `read` on `analytics`.
    fn good_auth() -> AuthInfo {
        AuthInfo {
            users: vec![AuthUser { user: "hune_ro".to_owned(), db: "admin".to_owned() }],
            roles: vec![AuthRole { role: "read".to_owned(), db: "analytics".to_owned() }],
            privileges: vec![
                privilege("analytics", "", &READ_ACTIONS),
                privilege("analytics", "system.js", &["find", "killCursors"]),
            ],
        }
    }

    fn failure_of(r: Result<impl std::fmt::Debug, Failure>) -> Failure {
        r.unwrap_err()
    }

    // ---- M0 ----

    #[test]
    fn m0_reads_the_version_and_the_limits() {
        assert_eq!(check_m0(&info()), Ok((7, 0, 14)));
        for (text, ok) in [
            ("7.0.14", Some((7, 0, 14))),
            ("8.0.0-rc4", Some((8, 0, 0))),
            ("7.0", Some((7, 0, 0))),
            ("6.0.1+ent", Some((6, 0, 1))),
            ("", None),
            ("7", None),
            ("7.x.1", None),
            ("seven", None),
            ("7.0.1.2", None),
            ("7..1", None),
            ("-7.0.1", None),
            ("7.0.1x", None),
        ] {
            assert_eq!(parse_version(text), ok, "{text}");
        }
        let mut i = info();
        i.version = "mongo".to_owned();
        let f = failure_of(check_m0(&i));
        assert_eq!(f.check, Check::M0);
        assert!(f.detail.starts_with("M0: "), "{}", f.detail);
        let mut i = info();
        i.max_wire_version = 0;
        assert_eq!(failure_of(check_m0(&i)).check, Check::M0);
        let mut i = info();
        i.max_bson_object_size = 16_777_215;
        assert_eq!(failure_of(check_m0(&i)).check, Check::M0);
        i.max_bson_object_size = 33_554_432;
        assert!(check_m0(&i).is_ok());
    }

    #[test]
    fn ms11_the_oldest_supported_version_is_7_0() {
        assert!(version_supported((7, 0, 0)));
        assert!(version_supported((8, 0, 3)));
        assert!(!version_supported((6, 0, 14)));
        assert!(!version_supported((4, 4, 29)));
    }

    // ---- M1 ----

    #[test]
    fn ms11_m1_wants_one_user_equal_to_the_keys() {
        assert!(check_m1(&good_auth(), "hune_ro", "admin").is_ok());
        let f = failure_of(check_m1(&AuthInfo::default(), "hune_ro", "admin"));
        assert_eq!(f.check, Check::M1);
        let mut a = good_auth();
        a.users[0].db = "analytics".to_owned();
        let f = failure_of(check_m1(&a, "hune_ro", "admin"));
        assert_eq!(f.detail, "M1: hune_ro@analytics");
        let mut a = good_auth();
        a.users[0].user = "root".to_owned();
        assert_eq!(failure_of(check_m1(&a, "hune_ro", "admin")).detail, "M1: root@admin");
        let mut a = good_auth();
        a.users.push(AuthUser { user: "other".to_owned(), db: "admin".to_owned() });
        let f = failure_of(check_m1(&a, "hune_ro", "admin"));
        assert_eq!(f.detail, "M1: hune_ro@admin, other@admin");
    }

    // ---- M2 ----

    fn roles(list: &[(&str, &str)]) -> AuthInfo {
        AuthInfo {
            roles: list.iter().map(|(r, d)| AuthRole { role: (*r).to_owned(), db: (*d).to_owned() }).collect(),
            ..good_auth()
        }
    }

    #[test]
    fn ms11_m2_accepts_only_read_on_the_listed_databases() {
        let d = vec!["analytics".to_owned()];
        assert!(check_m2(&good_auth(), &d).is_ok());
        for refused in [
            "readWrite",
            "dbAdmin",
            "userAdmin",
            "root",
            "readAnyDatabase",
            "clusterAdmin",
            "backup",
            "restore",
            "clusterMonitor",
            "customRole",
        ] {
            let a = roles(&[("read", "analytics"), (refused, "analytics")]);
            let f = failure_of(check_m2(&a, &d));
            assert_eq!(f.check, Check::M2, "{refused}");
            assert!(f.detail.contains(refused), "{}", f.detail);
        }
        // The role `read` on a database outside D, and on admin.
        let f = failure_of(check_m2(&roles(&[("read", "analytics"), ("read", "other")]), &d));
        assert_eq!(f.detail, "M2: read@other");
        assert!(check_m2(&roles(&[("read", "analytics"), ("read", "admin")]), &d).is_err());
        // A database of D without the role.
        let f = failure_of(check_m2(&roles(&[]), &d));
        assert_eq!(f.detail, "M2: missing read@analytics");
        let two = vec!["analytics".to_owned(), "logs".to_owned()];
        let f = failure_of(check_m2(&roles(&[("read", "analytics")]), &two));
        assert_eq!(f.detail, "M2: missing read@logs");
        assert!(check_m2(&roles(&[("read", "analytics"), ("read", "logs")]), &two).is_ok());
    }

    // ---- M3 ----

    #[test]
    fn ms11_m3_accepts_the_read_actions_on_the_listed_databases_only() {
        let d = vec!["analytics".to_owned()];
        assert!(check_m3(&good_auth(), &d).is_ok());
        assert!(check_m3(&AuthInfo::default(), &d).is_ok());
        // Each action of the list passes by itself.
        for action in READ_ACTIONS {
            let a = AuthInfo { privileges: vec![privilege("analytics", "c", &[action])], ..good_auth() };
            assert!(check_m3(&a, &d).is_ok(), "{action}");
        }
    }

    #[test]
    fn ms11_m3_refuses_a_cluster_resource_and_any_resource() {
        let d = vec!["analytics".to_owned()];
        let a = AuthInfo {
            privileges: vec![Privilege { resource: PrivilegeResource::Cluster, actions: vec!["find".to_owned()] }],
            ..good_auth()
        };
        assert_eq!(failure_of(check_m3(&a, &d)).detail, "M3: cluster");
        let a = AuthInfo {
            privileges: vec![Privilege { resource: PrivilegeResource::Other, actions: vec!["find".to_owned()] }],
            ..good_auth()
        };
        assert_eq!(failure_of(check_m3(&a, &d)).detail, "M3: anyResource");
        // A resource with an empty db is any database.
        let a = AuthInfo { privileges: vec![privilege("", "", &["find"])], ..good_auth() };
        assert_eq!(failure_of(check_m3(&a, &d)).detail, "M3: *.*");
        let a = AuthInfo { privileges: vec![privilege("", "c", &["find"])], ..good_auth() };
        assert!(check_m3(&a, &d).is_err());
        // A database outside D.
        let a = AuthInfo { privileges: vec![privilege("other", "c", &["find"])], ..good_auth() };
        assert_eq!(failure_of(check_m3(&a, &d)).detail, "M3: other.c");
        let a = AuthInfo { privileges: vec![privilege("admin", "", &["find"])], ..good_auth() };
        assert!(check_m3(&a, &d).is_err());
    }

    #[test]
    fn ms11_m3_refuses_each_write_action_and_each_action_that_the_list_lacks() {
        let d = vec!["analytics".to_owned()];
        for action in [
            "insert",
            "update",
            "remove",
            "createCollection",
            "dropCollection",
            "createIndex",
            "createUser",
            "grantRole",
            "applyOps",
            "internal",
            "anyAction",
            "inprog",
            "killop",
            "serverStatus",
            "viewRole",
            "compact",
            "convertToCapped",
            "dropDatabase",
            "FIND",
        ] {
            let a = AuthInfo {
                privileges: vec![privilege("analytics", "c", &["find", action, "collStats"])],
                ..good_auth()
            };
            let f = failure_of(check_m3(&a, &d));
            assert_eq!(f.check, Check::M3, "{action}");
            assert_eq!(f.detail, format!("M3: analytics.c:{action}"));
        }
    }

    #[test]
    fn ms11_a_failure_holds_at_most_five_names_in_order() {
        let d = vec!["analytics".to_owned()];
        let a = AuthInfo {
            privileges: (0..9).map(|i| privilege("analytics", &format!("c{i}"), &["insert"])).collect(),
            ..good_auth()
        };
        let f = failure_of(check_m3(&a, &d));
        assert_eq!(
            f.detail,
            "M3: analytics.c0:insert, analytics.c1:insert, analytics.c2:insert, analytics.c3:insert, analytics.c4:insert (and 4 more)"
        );
        // A name from the server is cleaned and cut.
        let a = AuthInfo {
            privileges: vec![privilege("analytics", &format!("a\nb{}", "x".repeat(300)), &["insert"])],
            ..good_auth()
        };
        let f = failure_of(check_m3(&a, &d));
        assert!(!f.detail.contains('\n'));
        assert!(f.detail.len() < 200, "{}", f.detail.len());
    }

    // ---- M4 ----

    #[test]
    fn m4_checks_the_replica_set_and_the_direct_mode_has_no_check() {
        let mut i = info();
        i.set_name = Some("other".to_owned());
        i.hosts = vec!["stranger:27017".to_owned()];
        assert!(check_m4(&i, &direct()).is_ok());
        let t = replica(&[("m1", 27017), ("m2", 27017)], "rs0");
        // The set name differs.
        let f = failure_of(check_m4(&i, &t));
        assert_eq!(f.detail, "M4: set name other");
        let mut none = info();
        none.set_name = None;
        assert_eq!(failure_of(check_m4(&none, &t)).detail, "M4: set name none");
        // The members.
        let mut ok = info();
        ok.set_name = Some("rs0".to_owned());
        ok.hosts = vec!["m1:27017".to_owned(), "M2:27017".to_owned()];
        assert!(check_m4(&ok, &t).is_ok());
        // A subset is fine: the block may list members that the server does not announce now.
        ok.hosts = vec!["m1:27017".to_owned()];
        assert!(check_m4(&ok, &t).is_ok());
        ok.hosts = vec!["m1:27017".to_owned(), "m3:27017".to_owned()];
        assert_eq!(failure_of(check_m4(&ok, &t)).detail, "M4: m3:27017");
        // A port counts.
        ok.hosts = vec!["m1:27018".to_owned()];
        assert!(check_m4(&ok, &t).is_err());
        // An IPv6 address with brackets.
        let t6 = replica(&[("::1", 27017), ("fe80::2", 27017)], "rs0");
        let mut v6 = info();
        v6.set_name = Some("rs0".to_owned());
        v6.hosts = vec!["[::1]:27017".to_owned()];
        // The block has no brackets (13.12b) and the announcement has: `::1:27017` is not `[::1]:27017`.
        assert!(check_m4(&v6, &t6).is_ok());
    }

    #[test]
    fn ms11_the_checks_run_in_order_and_the_first_failure_wins() {
        let cfg = config(direct());
        assert_eq!(check_all(&info(), &good_auth(), &cfg), Ok((7, 0, 14)));
        let mut bad = good_auth();
        bad.users[0].user = "x".to_owned();
        bad.roles[0].role = "readWrite".to_owned();
        bad.privileges[0].actions.push("insert".to_owned());
        assert_eq!(failure_of(check_all(&info(), &bad, &cfg)).check, Check::M1);
        bad.users[0].user = "hune_ro".to_owned();
        assert_eq!(failure_of(check_all(&info(), &bad, &cfg)).check, Check::M2);
        bad.roles[0].role = "read".to_owned();
        assert_eq!(failure_of(check_all(&info(), &bad, &cfg)).check, Check::M3);
        let mut i = info();
        i.version = "x".to_owned();
        assert_eq!(failure_of(check_all(&i, &bad, &cfg)).check, Check::M0);
        let rs = config(replica(&[("m1", 1), ("m2", 1)], "rs0"));
        assert_eq!(failure_of(check_all(&info(), &good_auth(), &rs)).check, Check::M4);
    }

    // ---- advice ----

    fn found(list: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        list.iter().map(|(d, c)| ((*d).to_owned(), c.iter().map(|x| (*x).to_owned()).collect())).collect()
    }

    #[test]
    fn ms11_a1_lists_a_collection_that_exists_nowhere() {
        let coll = vec!["node_load".to_owned(), "node_events".to_owned()];
        assert!(advice(&found(&[("analytics", &["node_load", "node_events"])]), &coll).is_empty());
        let a = advice(&found(&[("analytics", &["node_load"])]), &coll);
        assert_eq!(a, vec![Advice { check: AdviceCheck::A1, names: vec!["node_events".to_owned()] }]);
        // A name that one of two databases holds is not missing.
        let a = advice(&found(&[("analytics", &["node_load"]), ("logs", &["node_events"])]), &coll);
        assert!(a.is_empty());
    }

    #[test]
    fn ms11_a2_lists_a_readable_collection_that_the_key_does_not_list() {
        let coll = vec!["node_load".to_owned()];
        let a = advice(&found(&[("analytics", &["node_load", "secret", "system.views"])]), &coll);
        assert_eq!(a, vec![Advice { check: AdviceCheck::A2, names: vec!["analytics.secret".to_owned()] }]);
        // Both checks, A1 first.
        let a = advice(&found(&[("analytics", &["zzz"])]), &coll);
        assert_eq!(a.iter().map(Advice::check).collect::<Vec<_>>(), ["A1", "A2"]);
        // At most five names, in order.
        let many: Vec<String> = (0..9).map(|i| format!("c{i}")).collect();
        let many_refs: Vec<&str> = many.iter().map(String::as_str).collect();
        let a = advice(&found(&[("analytics", &many_refs)]), &coll);
        let a2 = a.iter().find(|x| x.check == AdviceCheck::A2).unwrap();
        assert_eq!(a2.names.len(), 5);
        assert_eq!(a2.names[0], "analytics.c0");
    }

    #[test]
    fn ms11_advice_comes_one_time_for_each_change() {
        let a1 = Advice { check: AdviceCheck::A1, names: vec!["x".to_owned()] };
        let a2 = Advice { check: AdviceCheck::A2, names: vec!["analytics.y".to_owned()] };
        assert_eq!(advice_changes(&[], &[a1.clone(), a2.clone()]).len(), 2);
        assert!(advice_changes(&[a1.clone(), a2.clone()], &[a1.clone(), a2.clone()]).is_empty());
        let changed = Advice { check: AdviceCheck::A1, names: vec!["x".to_owned(), "z".to_owned()] };
        assert_eq!(advice_changes(&[a1.clone(), a2.clone()], &[changed.clone(), a2.clone()]), vec![&changed]);
        assert!(advice_changes(std::slice::from_ref(&a1), &[]).is_empty());
    }

    // ---- the verdict ----

    fn report(failure: Option<Failure>) -> Report {
        Report { version: Some((7, 0, 14)), version_text: "7.0.14".to_owned(), failure, advice: vec![] }
    }

    #[test]
    fn ms11_finish_gives_up_misconfigured_or_unknown_by_the_checks() {
        let p = finish(SourceState::Unknown, &Outcome::Checked(report(None)));
        assert_eq!(
            (p.state, p.previous, p.version.as_deref(), p.version_supported),
            (SourceState::Up, SourceState::Unknown, Some("7.0.14"), Some(true))
        );
        assert!(p.detail.is_none() && p.changed());
        let f = fail_text(Check::M3, "x");
        let p = finish(SourceState::Up, &Outcome::Checked(report(Some(f))));
        assert_eq!((p.state, p.detail.as_deref()), (SourceState::Misconfigured, Some("M3: x")));
        let mut old = report(None);
        old.version = Some((6, 0, 1));
        old.version_text = "6.0.1".to_owned();
        let p = finish(SourceState::Unknown, &Outcome::Checked(old));
        assert_eq!((p.state, p.version_supported), (SourceState::Up, Some(false)));
    }

    #[test]
    fn a_fault_gives_down_or_misconfigured_by_its_group_and_a_misconfigured_source_stays() {
        let fault = |e: MongoError| Outcome::from_error(&e);
        let p = finish(SourceState::Up, &fault(MongoError::new(MongoErrorClass::Network, "reset")));
        assert_eq!(p.state, SourceState::Down);
        let p = finish(SourceState::Unknown, &fault(MongoError::new(MongoErrorClass::Timeout, "slow")));
        assert_eq!(p.state, SourceState::Down);
        let p = finish(SourceState::Up, &fault(MongoError::new(MongoErrorClass::Auth, "bad pw hunter2")));
        assert_eq!(p.state, SourceState::Misconfigured);
        assert!(!p.detail.as_deref().unwrap().contains("hunter2"));
        let p = finish(SourceState::Up, &fault(MongoError::server(13, "Unauthorized", "not authorized")));
        assert_eq!(p.state, SourceState::Misconfigured);
        assert!(p.detail.as_deref().unwrap().contains("code 13"));
        let p = finish(SourceState::Up, &fault(MongoError::server(59, "CommandNotFound", "no such command")));
        assert_eq!(p.state, SourceState::Misconfigured);
        // A fault of the link never moves a misconfigured source to down.
        let p = finish(SourceState::Misconfigured, &fault(MongoError::new(MongoErrorClass::Network, "reset")));
        assert_eq!(p.state, SourceState::Misconfigured);
        assert!(!p.changed());
        assert!(p.detail.as_deref().unwrap().contains("stays misconfigured"));
        let p = finish(SourceState::Misconfigured, &fault(MongoError::server(91, "ShutdownInProgress", "x")));
        assert_eq!(p.state, SourceState::Misconfigured);
        // A down source with a fault stays down.
        let p = finish(SourceState::Down, &fault(MongoError::new(MongoErrorClass::Network, "x")));
        assert_eq!((p.state, p.changed()), (SourceState::Down, false));
        assert_eq!(sticky(SourceState::Misconfigured, SourceState::Up), SourceState::Up);
    }

    #[test]
    fn the_deadline_is_connect_timeout_plus_10_seconds() {
        assert_eq!(probe_deadline(Duration::from_secs(5)), Duration::from_secs(15));
    }
}
