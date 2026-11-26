// SPDX-License-Identifier: MIT OR Apache-2.0
//! The key table of Hune (13.7 to 13.12). The reference files, `print-config` and the
//! checks all come from this table.

use hclconf::{BlockDesc, Forms, KeyDesc, Kind, Ref, Schema, Term, TextRule};
use hune_core::limits::{
    MAX_REPEAT_PER_DASHBOARD, MAX_REPEAT_PER_PANEL, MAX_REPEAT_PER_SECTION, REPEAT_BOUND_PER_DASHBOARD,
    REPEAT_BOUND_PER_PANEL, REPEAT_BOUND_PER_SECTION,
};

const AUTHZEN: &[Term] = &[Term { key: "access.mode", equals: "authzen" }];
const SSL_VERIFY_FULL: &[Term] = &[Term { key: "storage.postgres.sslmode", equals: "verify-full" }];
const TELEMETRY_ON: &[Term] = &[Term { key: "telemetry.enabled", equals: "true" }];
const PLAIN_HTTP_OFF: &[Term] = &[Term { key: "access.pdp.plain_http", equals: "false" }];

const fn text(min: usize, max: usize, rule: TextRule) -> Kind {
    Kind::Text { min, max, rule }
}

const fn seconds(min: u64, max: u64) -> Kind {
    Kind::Duration { min_ms: min * 1000, max_ms: max * 1000 }
}

const MS: u64 = 1;

/// The default of `server.transform_max_cells` (4.35, 9.21a, rule 13).
pub const TRANSFORM_MAX_CELLS_DEFAULT: u32 = 10_000_000;
/// The fixed upper bound of `server.transform_max_cells`. One cell of a frame of the page takes 9 bytes or more: a
/// double and a null flag. Thus one frame at the bound takes 900 MB or more, near the memory of one browser tab.
pub const TRANSFORM_MAX_CELLS_BOUND: u32 = 100_000_000;

/// The default of `server.list_filters_min_dashboards` (9.7, rule 6 of the list page).
pub const LIST_FILTERS_MIN_DEFAULT: u32 = 6;
/// The fixed upper bound of `server.list_filters_min_dashboards`. A list of more dashboards than this never has the
/// filters hidden, so the bound only stops a typing error.
pub const LIST_FILTERS_MIN_BOUND: u32 = 1000;

/// The most `storage.volume` blocks of a file (13.10).
pub const MAX_VOLUMES: usize = 16;

/// The blocks, in table order. A parent comes before its children.
static BLOCKS: [BlockDesc; 17] = [
    BlockDesc::new("server", "server", "The two listeners and the limits of the server (section 14)."),
    BlockDesc::new("log", "log", "The log level (14.6)."),
    BlockDesc::new("telemetry", "telemetry", "The export of the spans with OTLP (14.9)."),
    BlockDesc::new("storage", "storage", "The store of the dashboards (section 5)."),
    BlockDesc::new(
        "storage.volume",
        "storage.volume",
        "One volume store: one file for each dashboard. The label is the store id. The store with `primary = true` is writable. Each other store is read-only.",
    )
    .labelled(0, MAX_VOLUMES, "main"),
    BlockDesc::new(
        "storage.postgres",
        "storage.postgres",
        "The PostgreSQL store: one row for each dashboard. The store with `primary = true` is writable. Each other store is read-only.",
    )
    .manual("no `storage.volume` block exists", "never"),
    BlockDesc::new("storage.postgres.tls", "storage.postgres.tls", "The TLS link to PostgreSQL (12.14).")
        .when(SSL_VERIFY_FULL),
    BlockDesc::new("access", "access", "The access mode (section 11)."),
    BlockDesc::new("access.identity", "access.identity", "The identity token of the edge (11.4).").when(AUTHZEN),
    BlockDesc::new("access.pdp", "access.pdp", "The link to the policy decision point (11.5).").when(AUTHZEN),
    BlockDesc::new("clickhouse", "clickhouse", "One ClickHouse server (section 8). The label is the source id.")
        .labelled(0, 64, "prod"),
    BlockDesc::new("clickhouse.tls", "clickhouse.tls", "The TLS link to one ClickHouse server (12.14).")
        .manual("the `url` of the block has the scheme `https`", "the `url` of the block has the scheme `http`"),
    BlockDesc::new(
        "postgres",
        "postgres",
        "One PostgreSQL server for panel queries (section 8a). It is not the storage backend. The label is the source id and it is unique across both kinds of source.",
    )
    .labelled(0, 64, "metrics"),
    BlockDesc::new("postgres.tls", "postgres.tls", "The TLS link to one PostgreSQL source (12.14).")
        .manual("`sslmode = \"verify-full\"` of the block", "`sslmode = \"disable\"` of the block"),
    BlockDesc::new(
        "mongodb",
        "mongodb",
        "One MongoDB server or one replica set for panel queries (section 8b). The label is the source id and it is unique across the three kinds of source.",
    )
    .labelled(0, 64, "events"),
    BlockDesc::new("mongodb.tls", "mongodb.tls", "The TLS link to one MongoDB source (12.14).")
        .manual("`tls_mode = \"verify-full\"` of the block", "`tls_mode = \"disable\"` of the block"),
    BlockDesc::new(
        "geomap",
        "geomap",
        "The tile source of the panel type `geomap` (13.12c). The block is optional. Without it, the page contacts no tile host, and a map draws its markers on a plain background.",
    )
    .manual("never (the block is optional)", "never"),
];

/// The text of the column "Necessary when" of a key of `postgres "<id>".tls`.
const TLS_NECESSARY: &str = "`sslmode = \"verify-full\"`";
/// The text of the column "Forbidden when" of a key of `postgres "<id>".tls`.
const TLS_FORBIDDEN: &str = "`sslmode = \"disable\"`";

/// The text of the column "Necessary when" of a key of `mongodb "<id>".tls`.
const MONGO_TLS_NECESSARY: &str = "`tls_mode = \"verify-full\"`";
/// The text of the column "Forbidden when" of a key of `mongodb "<id>".tls`.
const MONGO_TLS_FORBIDDEN: &str = "`tls_mode = \"disable\"`";
/// The texts of the columns of the keys of the topology `direct` of a `mongodb` block.
const MONGO_DIRECT: &str = "`topology = \"direct\"`";
/// The texts of the columns of the keys of the topology `replica_set` of a `mongodb` block.
const MONGO_REPLICA_SET: &str = "`topology = \"replica_set\"`";

/// The keys, in table order.
static KEYS: [KeyDesc; 116] = [
    // 13.8: server
    KeyDesc::new(
        "server.listen",
        Kind::SocketAddr,
        "The main listener (14.2). Give the canonical form of an IP address and a port.",
    )
    .both(Ref::Str("0.0.0.0:8080")),
    KeyDesc::new(
        "server.ops_listen",
        Kind::SocketAddr,
        "The listener of the probes and the metrics (14.2). Its port is not the port of `server.listen`.",
    )
    .both(Ref::Str("0.0.0.0:9090")),
    KeyDesc::new(
        "server.base_path",
        text(1, 128, TextRule::NoControl),
        "The prefix of the routes of `server.listen`. Give `/`, or a path that starts and ends with `/`. A segment has 1 to 64 characters from `A-Z a-z 0-9 . _ ~ -`, and is not `.` or `..`.",
    )
    .both(Ref::Str("/")),
    KeyDesc::new(
        "server.history",
        Kind::Bool,
        "The value `true` gives the history of a dashboard of the primary store: the list of versions, one version, the diff of two versions and the restore (6.17, 6.18). The value `false` refuses these routes with `403 history_disabled`, and the page shows no History entry. The key has no default: the file always says it. In the mode `authzen`, the value `true` is the ceiling, and the PDP can only restrict it with the action `dashboard.history` (11.18).",
    )
    .both(Ref::Bool(true)),
    KeyDesc::new(
        "server.export",
        Kind::Bool,
        "The value `true` gives the file of the current document of a dashboard: `download=true` on the read of a dashboard (6.14). The value `false` refuses these requests with `403 export_disabled`, and the page shows no Export or Download entry. The key has no default: the file always says it. In the mode `authzen`, the value `true` is the ceiling, and the PDP can only restrict it with the action `dashboard.export` (11.18).",
    )
    .both(Ref::Bool(true)),
    KeyDesc::new(
        "server.query_inspect",
        Kind::Bool,
        "The value `true` adds the expanded SQL to the answer of a panel query, for the menu item Inspect (6.20). The key is optional. The default is `false`.",
    )
    .manual("never (the key is optional)", "never")
    .doc_ref(Ref::Bool(false)),
    KeyDesc::new(
        "server.repeat_max_per_panel",
        Kind::Integer { min: 1, max: REPEAT_BOUND_PER_PANEL as u64 },
        "The most copies of one panel that repeats by a variable (4.13a). Above it, the page shows a message and no copy. The key is optional. The default is 24. The upper bound is the limit of panels of one document (4.35).",
    )
    .manual("never (the key is optional)", "never")
    .doc_ref(Ref::Int(MAX_REPEAT_PER_PANEL as u64)),
    KeyDesc::new(
        "server.repeat_max_per_section",
        Kind::Integer { min: 1, max: REPEAT_BOUND_PER_SECTION as u64 },
        "The most copies of one section that repeats by a variable (4.13a). The key is optional. The default is 12. The upper bound is the limit of sections of one document (4.35).",
    )
    .manual("never (the key is optional)", "never")
    .doc_ref(Ref::Int(MAX_REPEAT_PER_SECTION as u64)),
    KeyDesc::new(
        "server.repeat_max_per_dashboard",
        Kind::Integer { min: 1, max: REPEAT_BOUND_PER_DASHBOARD as u64 },
        "The most copies of panels that repeat in one dashboard (4.13a). The key is optional. The default is 120. The upper bound is the limit of panels of one document (4.35). Contradictions C28 and C29 compare it with the two keys above.",
    )
    .manual("never (the key is optional)", "never")
    .doc_ref(Ref::Int(MAX_REPEAT_PER_DASHBOARD as u64)),
    KeyDesc::new(
        "server.compress_static",
        Kind::Bool,
        "The value `true` sends the gzip copy of a static file of a text type when the request accepts gzip (9.3). Hune makes the copies one time at start. An API answer and the shell are never compressed. The key is optional. The default is `true`.",
    )
    .manual("never (the key is optional)", "never")
    .doc_ref(Ref::Bool(true)),
    KeyDesc::new(
        "server.transform_max_cells",
        Kind::Integer { min: 1, max: TRANSFORM_MAX_CELLS_BOUND as u64 },
        "The most cells (fields times rows) of the frame that one step of a transform makes in the page (4.35, 9.21a, rule 13). Above it, the panel shows an error and no frame. The session gives the value to the page (6.10). The key is optional. The default is 10000000. The upper bound is fixed. One cell takes 9 bytes or more: a double and a null flag. Thus 100000000 cells take 900 MB or more, near the memory of one browser tab.",
    )
    .manual("never (the key is optional)", "never")
    .doc_ref(Ref::Int(TRANSFORM_MAX_CELLS_DEFAULT as u64)),
    KeyDesc::new(
        "server.list_filters_min_dashboards",
        Kind::Integer { min: 0, max: LIST_FILTERS_MIN_BOUND as u64 },
        "The list page shows the sort control and the tag filter only when the user sees more dashboards than this number (9.7). With this number or fewer, the page hides them. The value 0 shows them for any list that is not empty. The session gives the value to the page (6.10). The key is optional. The default is 6. The upper bound is fixed at 1000.",
    )
    .manual("never (the key is optional)", "never")
    .doc_ref(Ref::Int(LIST_FILTERS_MIN_DEFAULT as u64)),
    KeyDesc::new(
        "server.max_connections",
        Kind::Integer { min: 16, max: 65536 },
        "The most open connections on `server.listen`.",
    )
    .both(Ref::Int(1024)),
    KeyDesc::new(
        "server.idle_timeout",
        seconds(1, 600),
        "Hune closes a kept connection that is idle for this time.",
    )
    .both(Ref::Str("75s")),
    KeyDesc::new("server.shutdown_grace", seconds(5, 600), "The longest graceful stop (14.13).")
        .both(Ref::Str("30s")),
    KeyDesc::new(
        "server.max_request_bytes",
        Kind::Size { min: 4_259_840, max: 67_108_864 },
        "The body limit of a dashboard create, save or import (12.16). The lowest value is the document limit of section 4 plus 65536.",
    )
    .both(Ref::Int(8_388_608)),
    KeyDesc::new(
        "server.max_result_memory_bytes",
        Kind::Size { min: 16_777_216, max: 68_719_476_736 },
        "The memory budget of the results in flight (12.16).",
    )
    .both(Ref::Int(536_870_912)),
    // 13.9: log and telemetry
    KeyDesc::new(
        "log.level",
        Kind::Enum(&["error", "warn", "info", "debug"]),
        "The lowest level that Hune writes (14.6).",
    )
    .both(Ref::Str("info")),
    KeyDesc::new(
        "telemetry.enabled",
        Kind::Bool,
        "The value `true` exports the spans with OTLP (14.9).",
    )
    .both(Ref::Bool(false)),
    KeyDesc::new(
        "telemetry.otlp_endpoint",
        text(1, 2048, TextRule::NoControl),
        "A URL with the scheme `http` or `https`, a host and an optional port. It has no path, query, fragment or user info. Hune sends to `<endpoint>/v1/traces`.",
    )
    .forms(Forms::Vef)
    .when(TELEMETRY_ON)
    .doc_ref(Ref::Str("http://otel-collector:4318")),
    KeyDesc::new("telemetry.ca_file", Kind::Path, "A PEM file of CA certificates (13.13).")
        .when_optional(TELEMETRY_ON, "`telemetry.enabled = true` and an `https` endpoint")
        .forbidden("`telemetry.enabled = false`, or an `http` endpoint"),
    KeyDesc::new(
        "telemetry.service_name",
        text(1, 64, TextRule::PrintableAscii),
        "The resource attribute `service.name`.",
    )
    .when(TELEMETRY_ON)
    .doc_ref(Ref::Str("hune")),
    KeyDesc::new("telemetry.export_interval", seconds(1, 300), "The period of the export.")
        .when(TELEMETRY_ON)
        .doc_ref(Ref::Str("15s")),
    // 13.10: storage
    KeyDesc::new(
        "storage.volume.path",
        Kind::Path,
        "The directory of the dashboards. The path is canonical: it has no `.` or `..` segment and no repeated or last `/` (section 5).",
    )
    .refs(Ref::Str("/var/lib/hune/dashboards"), Ref::Absent),
    KeyDesc::new(
        "storage.volume.title",
        text(1, 128, TextRule::NoControl),
        "The name of the root folder of the store in the page. A read-only store needs it, and the primary store has none (section 5).",
    )
    .manual("the store is not the primary", "the store is the primary")
    .doc_ref(Ref::Str("Lab")),
    KeyDesc::new(
        "storage.volume.primary",
        Kind::Bool,
        "The value `true` makes this store the primary store: the only store that accepts writes. At most one store has it. The key is optional (section 5).",
    )
    .manual("never (the key is optional)", "another store has `primary = true`")
    .refs(Ref::Bool(true), Ref::Absent),
    KeyDesc::new(
        "storage.volume.rescan_interval",
        seconds(1, 3600),
        "The period of the scan for changes made outside Hune.",
    )
    .refs(Ref::Str("5s"), Ref::Absent),
    KeyDesc::new(
        "storage.volume.history_keep",
        Kind::Integer { min: 1, max: 10_000 },
        "The versions kept for each dashboard. Only the primary store has history.",
    )
    .manual("the store is the primary", "the store is not the primary")
    .refs(Ref::Int(100), Ref::Absent),
    KeyDesc::new("storage.postgres.host", text(1, 253, TextRule::HostOrIp), "The host of the PostgreSQL server.")
        .refs(Ref::Absent, Ref::Str("hune-pg-rw")),
    KeyDesc::new("storage.postgres.port", Kind::Integer { min: 1, max: 65535 }, "The port of the PostgreSQL server.")
        .refs(Ref::Absent, Ref::Int(5432)),
    KeyDesc::new("storage.postgres.database", text(1, 63, TextRule::NoControl), "The name of the database.")
        .refs(Ref::Absent, Ref::Str("hune")),
    KeyDesc::new("storage.postgres.user", text(1, 63, TextRule::NoControl), "The name of the role of `serve`.")
        .forms(Forms::Vef)
        .refs(Ref::Absent, Ref::File("/run/secrets/pg/username")),
    KeyDesc::new("storage.postgres.password", text(1, 1024, TextRule::Any), "The password of the role of `serve`.")
        .secret()
        .refs(Ref::Absent, Ref::File("/run/secrets/pg/password")),
    KeyDesc::new(
        "storage.postgres.sslmode",
        Kind::Enum(&["verify-full", "disable"]),
        "The value `verify-full` checks the certificate and the name (12.14).",
    )
    .refs(Ref::Absent, Ref::Str("verify-full")),
    KeyDesc::new("storage.postgres.tls.ca_file", Kind::Path, "A PEM file of CA certificates (13.13).")
        .refs(Ref::Absent, Ref::Str("/run/secrets/pg-ca/ca.crt")),
    KeyDesc::new(
        "storage.postgres.tls.server_name",
        text(1, 253, TextRule::DnsName),
        "The name that the certificate must hold.",
    )
    .refs(Ref::Absent, Ref::Str("hune-pg-rw")),
    KeyDesc::new(
        "storage.postgres.max_connections",
        Kind::Integer { min: 2, max: 64 },
        "The size of the connection pool.",
    )
    .refs(Ref::Absent, Ref::Int(8)),
    KeyDesc::new("storage.postgres.connect_timeout", seconds(1, 60), "The longest wait for a connection.")
        .refs(Ref::Absent, Ref::Str("5s")),
    KeyDesc::new(
        "storage.postgres.statement_timeout",
        Kind::Duration { min_ms: 100 * MS, max_ms: 60_000 },
        "The longest statement. Hune also uses it as `idle_in_transaction_session_timeout` (section 5).",
    )
    .refs(Ref::Absent, Ref::Str("10s")),
    KeyDesc::new(
        "storage.postgres.poll_interval",
        seconds(1, 300),
        "The period of the poll of the catalog (section 5).",
    )
    .refs(Ref::Absent, Ref::Str("5s")),
    KeyDesc::new(
        "storage.postgres.title",
        text(1, 128, TextRule::NoControl),
        "The name of the root folder of the store in the page. A read-only store needs it, and the primary store has none (section 5).",
    )
    .manual("the store is not the primary", "the store is the primary")
    .doc_ref(Ref::Str("Archive")),
    KeyDesc::new(
        "storage.postgres.primary",
        Kind::Bool,
        "The value `true` makes this store the primary store: the only store that accepts writes. At most one store has it. The key is optional (section 5).",
    )
    .manual("never (the key is optional)", "another store has `primary = true`")
    .refs(Ref::Absent, Ref::Bool(true)),
    KeyDesc::new(
        "storage.postgres.history_keep",
        Kind::Integer { min: 1, max: 10_000 },
        "The versions kept for each dashboard. Only the primary store has history.",
    )
    .manual("the store is the primary", "the store is not the primary")
    .refs(Ref::Absent, Ref::Int(100)),
    // 13.11: access
    KeyDesc::new("access.mode", Kind::Enum(&["open", "authzen"]), "The access mode (11.2).")
        .refs(Ref::Str("open"), Ref::Str("authzen")),
    KeyDesc::new(
        "access.identity.header",
        text(1, 64, TextRule::NoControl),
        "The name of the header that holds the token: 1 to 64 token characters (RFC 9110). It is not `Authorization`, `Proxy-Authorization`, `Cookie`, `Host`, `Content-Type` or `Content-Length`, in any case.",
    )
    .refs(Ref::Absent, Ref::Str("X-Identity-Token")),
    KeyDesc::new(
        "access.identity.issuer",
        text(1, 1024, TextRule::NoControl),
        "The value that the claim `iss` must have.",
    )
    .refs(Ref::Absent, Ref::Str("https://edge.example")),
    KeyDesc::new(
        "access.identity.audience",
        text(1, 256, TextRule::NoControl),
        "The service id (11.14). It is an id of P 4.1: 1 to 256 bytes of UTF-8.",
    )
    .refs(Ref::Absent, Ref::Str("dashboards")),
    KeyDesc::new(
        "access.identity.public_keys",
        text(1, 65_536, TextRule::Any),
        "The public keys of the edge: PEM text or a JWK Set with 1 to 16 keys (11.9).",
    )
    .forms(Forms::Vef)
    .refs(Ref::Absent, Ref::File("/run/secrets/edge/jwks.json")),
    KeyDesc::new(
        "access.identity.max_age",
        seconds(1, 86_400),
        "The longest life of a token. A whole number of seconds.",
    )
    .refs(Ref::Absent, Ref::Str("5m")),
    KeyDesc::new(
        "access.identity.clock_skew",
        seconds(0, 300),
        "The skew that applies to `iat` and `exp`. A whole number of seconds.",
    )
    .refs(Ref::Absent, Ref::Str("1m")),
    KeyDesc::new(
        "access.pdp.url",
        text(1, 2048, TextRule::NoControl),
        "A URL with the scheme `http` or `https`, a host, an optional port and an optional path. It has no user info, query or fragment.",
    )
    .forms(Forms::Vef)
    .refs(Ref::Absent, Ref::Str("https://pdp.example:4182")),
    KeyDesc::new(
        "access.pdp.token",
        text(1, 4096, TextRule::NoControlNoSpace),
        "The credential of Hune at the PDP. Hune sends it in `Authorization: Bearer`.",
    )
    .secret()
    .refs(Ref::Absent, Ref::File("/run/secrets/pdp/token")),
    KeyDesc::new("access.pdp.ca_file", Kind::Path, "A PEM file of CA certificates (13.13).")
        .when(PLAIN_HTTP_OFF)
        .necessary("`access.pdp.plain_http = false`")
        .forbidden("`access.pdp.plain_http = true`, or `access.mode = \"open\"`")
        .refs(Ref::Absent, Ref::Str("/run/secrets/pdp/ca.pem")),
    KeyDesc::new(
        "access.pdp.plain_http",
        Kind::Bool,
        "The value `true` accepts an `http` URL. It must agree with the scheme of `access.pdp.url`.",
    )
    .refs(Ref::Absent, Ref::Bool(false)),
    KeyDesc::new("access.pdp.timeout", Kind::Duration { min_ms: 10 * MS, max_ms: 60_000 }, "The longest wait for the PDP.")
        .refs(Ref::Absent, Ref::Str("500ms")),
    KeyDesc::new("access.pdp.cache_ttl", seconds(0, 3600), "The longest life of a kept answer.")
        .refs(Ref::Absent, Ref::Str("30s")),
    KeyDesc::new("access.pdp.max_connections", Kind::Integer { min: 1, max: 32 }, "The most connections to the PDP.")
        .refs(Ref::Absent, Ref::Int(4)),
    // 13.12: clickhouse
    KeyDesc::new(
        "clickhouse.label",
        text(1, 240, TextRule::NoControl),
        "The label in the page and in the action document: 1 to 60 characters.",
    )
    .both(Ref::Str("Production")),
    KeyDesc::new(
        "clickhouse.url",
        text(1, 2048, TextRule::NoControl),
        "The HTTP interface of ClickHouse: a URL with the scheme `http` or `https`, a host and an optional port. It has no path, query, fragment or user info.",
    )
    .both(Ref::Str("https://clickhouse:8443")),
    KeyDesc::new("clickhouse.user", text(1, 256, TextRule::NoControl), "The name of the ClickHouse account.")
        .forms(Forms::Vef)
        .both(Ref::Str("hune")),
    KeyDesc::new("clickhouse.password", text(1, 1024, TextRule::Any), "The password of the ClickHouse account.")
        .secret()
        .both(Ref::File("/run/secrets/clickhouse/password")),
    KeyDesc::new(
        "clickhouse.database",
        text(1, 64, TextRule::NoControl),
        "The default database of the queries: a ClickHouse identifier that matches `^[A-Za-z_][A-Za-z0-9_]{0,63}$`.",
    )
    .both(Ref::Str("analytics")),
    KeyDesc::new("clickhouse.tls.ca_file", Kind::Path, "A PEM file of CA certificates (13.13).")
        .both(Ref::Str("/run/secrets/clickhouse/ca.pem")),
    KeyDesc::new(
        "clickhouse.tls.server_name",
        text(1, 253, TextRule::DnsName),
        "The name that the certificate must hold.",
    )
    .both(Ref::Str("clickhouse")),
    KeyDesc::new(
        "clickhouse.connect_timeout",
        Kind::Duration { min_ms: 100 * MS, max_ms: 60_000 },
        "The longest wait for a connection.",
    )
    .both(Ref::Str("5s")),
    KeyDesc::new(
        "clickhouse.max_execution_time",
        seconds(1, 3600),
        "The longest run of a query (11.24, section 8). A whole number of seconds.",
    )
    .both(Ref::Str("1m")),
    KeyDesc::new(
        "clickhouse.max_result_rows",
        Kind::Integer { min: 1, max: 1_000_000_000 },
        "The most rows of a result.",
    )
    .both(Ref::Int(100_000)),
    KeyDesc::new(
        "clickhouse.max_result_bytes",
        Kind::Size { min: 1_048_576, max: 1_073_741_824 },
        "The most bytes of a result.",
    )
    .both(Ref::Int(33_554_432)),
    KeyDesc::new(
        "clickhouse.max_query_size",
        Kind::Size { min: 262_144, max: 16_777_216 },
        "The largest expanded SQL. Hune also sends it as the setting `max_query_size` (section 7).",
    )
    .both(Ref::Int(1_048_576)),
    KeyDesc::new(
        "clickhouse.max_concurrent",
        Kind::Integer { min: 1, max: 1024 },
        "The most queries at one time on this source.",
    )
    .both(Ref::Int(16)),
    KeyDesc::new("clickhouse.queue_wait", seconds(0, 60), "The longest wait for a place (12.16).")
        .both(Ref::Str("5s")),
    KeyDesc::new(
        "clickhouse.max_concurrent_per_subject",
        Kind::Integer { min: 1, max: 1024 },
        "The most queries at one time for one subject on this source. It is at most `max_concurrent`.",
    )
    .when(AUTHZEN)
    .refs(Ref::Absent, Ref::Int(6)),
    // The ad hoc filters. The key is optional and has no default: absent, no ad hoc filter applies on
    // the source. The reference files hold no such key (`doc_ref` only), so no filter is on by default.
    KeyDesc::new(
        "clickhouse.adhoc_columns",
        Kind::TextList { min_items: 1, max_items: 256, item_min: 1, item_max: 64, rule: TextRule::NoControlNoSpace },
        "The columns that an ad hoc filter of this source can name (4.10c, 7.7a). Each item matches `^[A-Za-z_][A-Za-z0-9_]{0,63}$`, and no item is repeated. The key is optional, with no default: absent, no ad hoc filter applies on this source. Each listed column is exposed to each viewer of a dashboard of the source whose SQL has `$__adhocFilters` (12.9).",
    )
    .manual("never (the key is optional)", "never")
    .doc_ref(Ref::StrList(&["service", "host", "env"])),
    // 13.12a: postgres. The reference files hold no `postgres` block, so each reference value
    // is a value of the reference document only (`doc_ref`).
    KeyDesc::new(
        "postgres.label",
        text(1, 240, TextRule::NoControl),
        "The label in the page and in the action document: 1 to 60 characters.",
    )
    .doc_ref(Ref::Str("Metrics")),
    KeyDesc::new(
        "postgres.host",
        text(1, 253, TextRule::HostOrIp),
        "The one host that Hune connects to (8a.4): a DNS name or an IP address. A Unix socket path is not valid.",
    )
    .doc_ref(Ref::Str("hune-pg-ro")),
    KeyDesc::new("postgres.port", Kind::Integer { min: 1, max: 65535 }, "The port of the host.")
        .doc_ref(Ref::Int(5432)),
    KeyDesc::new(
        "postgres.database",
        text(1, 63, TextRule::NoControl),
        "The database of the connection.",
    )
    .doc_ref(Ref::Str("analytics")),
    KeyDesc::new(
        "postgres.user",
        text(1, 63, TextRule::NoControl),
        "The name of the read-only role (8a.13). The role must not be the role of the store on the same server.",
    )
    .forms(Forms::Vef)
    .doc_ref(Ref::File("/run/secrets/postgres-metrics/username")),
    KeyDesc::new("postgres.password", text(1, 1024, TextRule::Any), "The password of the read-only role.")
        .secret()
        .doc_ref(Ref::File("/run/secrets/postgres-metrics/password")),
    KeyDesc::new(
        "postgres.search_path",
        text(1, 256, TextRule::NoControl),
        "1 to 8 names separated by `,` with no blank. A name matches `^[a-z_][a-z0-9_]{0,62}$`, does not start with `pg_temp` and is not repeated. Hune sends it as the session value (8a.4).",
    )
    .doc_ref(Ref::Str("metrics")),
    KeyDesc::new(
        "postgres.sslmode",
        Kind::Enum(&["verify-full", "disable"]),
        "The value `verify-full` checks the certificate and the name (12.14).",
    )
    .doc_ref(Ref::Str("verify-full")),
    KeyDesc::new("postgres.tls.ca_file", Kind::Path, "A PEM file of CA certificates (13.13).")
        .necessary(TLS_NECESSARY)
        .forbidden(TLS_FORBIDDEN)
        .doc_ref(Ref::Str("/run/secrets/postgres-metrics-ca/ca.crt")),
    KeyDesc::new(
        "postgres.tls.server_name",
        text(1, 253, TextRule::DnsName),
        "The name that the certificate must hold.",
    )
    .necessary(TLS_NECESSARY)
    .forbidden(TLS_FORBIDDEN)
    .doc_ref(Ref::Str("hune-pg-ro")),
    KeyDesc::new(
        "postgres.connect_timeout",
        Kind::Duration { min_ms: 100 * MS, max_ms: 60_000 },
        "The longest time for the connection, the TLS handshake, the login and the setup check (8a.4).",
    )
    .doc_ref(Ref::Str("5s")),
    KeyDesc::new(
        "postgres.max_execution_time",
        seconds(1, 3600),
        "The longest run of a query (11.24, section 8a). A whole number of seconds.",
    )
    .doc_ref(Ref::Str("1m")),
    KeyDesc::new(
        "postgres.max_result_rows",
        Kind::Integer { min: 1, max: 1_000_000_000 },
        "The most rows of a result.",
    )
    .doc_ref(Ref::Int(100_000)),
    KeyDesc::new(
        "postgres.max_result_bytes",
        Kind::Size { min: 1_048_576, max: 1_073_741_824 },
        "The most bytes of a result.",
    )
    .doc_ref(Ref::Int(33_554_432)),
    KeyDesc::new(
        "postgres.max_query_size",
        Kind::Size { min: 262_144, max: 16_777_216 },
        "The largest expanded SQL (section 7). Hune checks it itself.",
    )
    .doc_ref(Ref::Int(1_048_576)),
    KeyDesc::new(
        "postgres.max_concurrent",
        Kind::Integer { min: 1, max: 256 },
        "The most queries at one time on this source, and the size of its pool (8a.9).",
    )
    .doc_ref(Ref::Int(16)),
    KeyDesc::new("postgres.queue_wait", seconds(0, 60), "The longest wait for a place (12.16).")
        .doc_ref(Ref::Str("5s")),
    KeyDesc::new(
        "postgres.max_concurrent_per_subject",
        Kind::Integer { min: 1, max: 256 },
        "The most queries at one time for one subject on this source. It is at most `max_concurrent`.",
    )
    .when(AUTHZEN)
    .doc_ref(Ref::Int(6)),
    // 13.12b: mongodb. The reference files hold no `mongodb` block, so each reference value is a
    // value of the reference document only (`doc_ref`). The keys of one topology are `manual`:
    // the engine does not decide on a key of a labelled block, so the contradiction C26 does.
    KeyDesc::new(
        "mongodb.label",
        text(1, 240, TextRule::NoControl),
        "The label in the page and in the action document: 1 to 60 characters.",
    )
    .doc_ref(Ref::Str("Events")),
    KeyDesc::new(
        "mongodb.topology",
        Kind::Enum(&["direct", "replica_set"]),
        "The value `direct` is one host, with no discovery. The value `replica_set` is the complete list of the members (8b.4).",
    )
    .doc_ref(Ref::Str("direct")),
    KeyDesc::new(
        "mongodb.host",
        text(1, 253, TextRule::HostOrIp),
        "The one host that Hune connects to (8b.4): a DNS name or an IP address. A Unix socket path, a connection string and the scheme `mongodb+srv` are not valid.",
    )
    .manual(MONGO_DIRECT, MONGO_REPLICA_SET)
    .doc_ref(Ref::Str("hune-mongo-ro")),
    KeyDesc::new("mongodb.port", Kind::Integer { min: 1, max: 65535 }, "The port of the host.")
        .manual(MONGO_DIRECT, MONGO_REPLICA_SET)
        .doc_ref(Ref::Int(27017)),
    KeyDesc::new(
        "mongodb.hosts",
        Kind::TextList { min_items: 2, max_items: 7, item_min: 1, item_max: 262, rule: TextRule::NoControlNoSpace },
        "The members of the replica set, all of them (8b.4). Each item is `host:port`, with an IPv6 address in brackets and a port from 1 to 65535. No item is repeated.",
    )
    .manual(MONGO_REPLICA_SET, MONGO_DIRECT)
    .doc_ref(Ref::StrList(&["mongo-0:27017", "mongo-1:27017", "mongo-2:27017"])),
    KeyDesc::new(
        "mongodb.replica_set",
        text(1, 64, TextRule::NoControl),
        "The name of the replica set. It matches `^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$`.",
    )
    .manual(MONGO_REPLICA_SET, MONGO_DIRECT)
    .doc_ref(Ref::Str("rs0")),
    KeyDesc::new(
        "mongodb.user",
        text(1, 128, TextRule::NoControl),
        "The name of the read-only user (8b.13).",
    )
    .forms(Forms::Vef)
    .doc_ref(Ref::File("/run/secrets/mongodb-events/username")),
    KeyDesc::new("mongodb.password", text(1, 1024, TextRule::Any), "The password of the read-only user.")
        .secret()
        .doc_ref(Ref::File("/run/secrets/mongodb-events/password")),
    KeyDesc::new(
        "mongodb.auth_database",
        text(1, 63, TextRule::NoControl),
        "The database that holds the user. It matches `^[A-Za-z0-9_-]{1,63}$`. It can be `admin`.",
    )
    .doc_ref(Ref::Str("admin")),
    KeyDesc::new(
        "mongodb.databases",
        Kind::TextList { min_items: 1, max_items: 8, item_min: 1, item_max: 63, rule: TextRule::NoControl },
        "The databases that a query can read (7.33). Each item matches `^[A-Za-z0-9_-]{1,63}$` and is not `admin`, `local` or `config`. No item is repeated.",
    )
    .doc_ref(Ref::StrList(&["analytics"])),
    KeyDesc::new(
        "mongodb.collections",
        Kind::TextList { min_items: 1, max_items: 64, item_min: 1, item_max: 100, rule: TextRule::NoControl },
        "The collections that a query can name (7.33). Each item matches `^[A-Za-z0-9_][A-Za-z0-9_.-]{0,99}$` and does not start with `system.`. No item is repeated.",
    )
    .doc_ref(Ref::StrList(&["node_load", "node_events"])),
    KeyDesc::new(
        "mongodb.read_preference",
        Kind::Enum(&["primary", "primaryPreferred", "secondary", "secondaryPreferred", "nearest"]),
        "The read preference of each command (8b.5).",
    )
    .doc_ref(Ref::Str("secondaryPreferred")),
    KeyDesc::new(
        "mongodb.tls_mode",
        Kind::Enum(&["verify-full", "disable"]),
        "The value `verify-full` checks the certificate and the host name (12.14).",
    )
    .doc_ref(Ref::Str("verify-full")),
    KeyDesc::new("mongodb.tls.ca_file", Kind::Path, "A PEM file of CA certificates (13.13).")
        .necessary(MONGO_TLS_NECESSARY)
        .forbidden(MONGO_TLS_FORBIDDEN)
        .doc_ref(Ref::Str("/run/secrets/mongodb-events-ca/ca.crt")),
    KeyDesc::new(
        "mongodb.connect_timeout",
        Kind::Duration { min_ms: 100 * MS, max_ms: 60_000 },
        "The longest time for the connection, the TLS handshake, the login and the server selection (8b.4).",
    )
    .doc_ref(Ref::Str("5s")),
    KeyDesc::new(
        "mongodb.max_execution_time",
        seconds(1, 3600),
        "The longest run of a query (11.24, section 8b). A whole number of seconds.",
    )
    .doc_ref(Ref::Str("1m")),
    KeyDesc::new(
        "mongodb.max_result_rows",
        Kind::Integer { min: 1, max: 1_000_000_000 },
        "The most rows of a result.",
    )
    .doc_ref(Ref::Int(100_000)),
    KeyDesc::new(
        "mongodb.max_result_bytes",
        Kind::Size { min: 1_048_576, max: 1_073_741_824 },
        "The most bytes of a result.",
    )
    .doc_ref(Ref::Int(33_554_432)),
    KeyDesc::new(
        "mongodb.max_query_size",
        Kind::Size { min: 262_144, max: 8_388_608 },
        "The largest BSON size of the expanded command (7.35). Hune checks it itself.",
    )
    .doc_ref(Ref::Int(1_048_576)),
    KeyDesc::new(
        "mongodb.max_concurrent",
        Kind::Integer { min: 1, max: 256 },
        "The most queries at one time on this source (8b.9).",
    )
    .doc_ref(Ref::Int(16)),
    KeyDesc::new("mongodb.queue_wait", seconds(0, 60), "The longest wait for a place (12.16).")
        .doc_ref(Ref::Str("5s")),
    KeyDesc::new(
        "mongodb.max_concurrent_per_subject",
        Kind::Integer { min: 1, max: 256 },
        "The most queries at one time for one subject on this source. It is at most `max_concurrent`.",
    )
    .when(AUTHZEN)
    .doc_ref(Ref::Int(6)),
    // 13.12c: geomap. The block is optional. In the block each key is necessary, and each key accepts
    // the value form only: the values are public. The reference files hold no block `geomap` (`doc_ref` only).
    KeyDesc::new(
        "geomap.tile_url",
        text(16, 512, TextRule::PrintableAscii),
        "The template of the URL of a tile (13.12c). It has `https://`, a lower-case DNS name with a dot, an optional port and a path. The path holds `{z}`, `{x}` and `{y}` once each. An optional fixed query can follow. The page loads each tile as an image. The directive `img-src` gets the host and the path up to the last `/` before the first placeholder (12.8).",
    )
    .necessary(GEOMAP_NECESSARY)
    .doc_ref(Ref::Str("https://tiles.example.org/osm/{z}/{x}/{y}.png")),
    KeyDesc::new(
        "geomap.attribution",
        text(1, 800, TextRule::NoControl),
        "The attribution of the tiles: 1 to 200 characters, with no bidirectional control, no zero-width character and no line or paragraph separator. The page shows it as text, with no link.",
    )
    .necessary(GEOMAP_NECESSARY)
    .doc_ref(Ref::Str("Map tiles: example.org")),
    KeyDesc::new(
        "geomap.max_zoom",
        Kind::Integer { min: 0, max: super::geomap::MAX_ZOOM_BOUND },
        "The highest zoom of a map. The lowest zoom is 0.",
    )
    .necessary(GEOMAP_NECESSARY)
    .doc_ref(Ref::Int(19)),
];

/// The text of the column "Necessary when" of a key of the block `geomap`.
const GEOMAP_NECESSARY: &str = "the block `geomap` exists";

/// The key table of Hune.
pub static SCHEMA: Schema = Schema { blocks: &BLOCKS, keys: &KEYS };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_consistent() {
        SCHEMA.validate().unwrap();
    }

    #[test]
    fn secrets_accept_only_e_and_f() {
        for key in SCHEMA.keys {
            if key.secret {
                assert_eq!(key.forms, Forms::Ef, "{}", key.path);
            }
        }
        let secrets: Vec<&str> = SCHEMA.keys.iter().filter(|k| k.secret).map(|k| k.path).collect();
        assert_eq!(
            secrets,
            [
                "storage.postgres.password",
                "access.pdp.token",
                "clickhouse.password",
                "postgres.password",
                "mongodb.password"
            ]
        );
    }

    #[test]
    fn keys_with_indirection_are_those_of_13_5() {
        let forms: Vec<&str> = SCHEMA.keys.iter().filter(|k| k.forms != Forms::V).map(|k| k.path).collect();
        assert_eq!(
            forms,
            [
                "telemetry.otlp_endpoint",
                "storage.postgres.user",
                "storage.postgres.password",
                "access.identity.public_keys",
                "access.pdp.url",
                "access.pdp.token",
                "clickhouse.user",
                "clickhouse.password",
                "postgres.user",
                "postgres.password",
                "mongodb.user",
                "mongodb.password",
            ]
        );
    }
}
