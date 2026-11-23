//! The queries of a panel (15.6, query members) and the data sources (15.9).

use std::collections::{BTreeMap, BTreeSet};

use hune_core::document::{is_valid_query_ref, parse_duration_ms, parse_time_from, parse_time_shift};
use hune_core::limits::MAX_QUERIES_PER_PANEL;
use hune_core::sql::Dialect;
use hune_core::sql::placeholders::is_name_byte;

use crate::dashboard::Form;
use crate::item::{Ctx, Fallback};
use crate::mongoprep::{MongoMembers, prepare_mongo};
use crate::options::{ImportOptions, SourceKind};
use crate::sqlprep::{Sql, prepare};
use crate::util::{as_flag, as_int};
use crate::walk::{Node, Walk};

/// The type of the ClickHouse data source plugin.
pub(crate) const CLICKHOUSE_TYPE: &str = "grafana-clickhouse-datasource";
/// The types of the PostgreSQL data source plugin: the plugin of Grafana 12 and the old built-in name (15.9).
pub(crate) const POSTGRES_TYPES: [&str; 2] = ["grafana-postgresql-datasource", "postgres"];
/// The type of the MongoDB data source plugin of the production Grafana.
pub(crate) const MONGO_TYPE: &str = "haohanyang-mongodb-datasource";
/// The uid of the data source that reuses the result of a panel.
const DASHBOARD_DS: &str = "-- Dashboard --";

/// The data of a panel after the mapping (4.15).
#[derive(Debug, Clone)]
pub(crate) enum DataOut {
    /// The panel runs its own SQL.
    Query {
        source: String,
        format: &'static str,
        sql: Vec<String>,
        min_interval: Option<String>,
        max_data_points: Option<i64>,
        /// The time override of the panel (4.15).
        time: TimeOverride,
        /// The ref of the first query (4.15a). `A` when the panel has one query.
        query_ref: String,
        /// `true`: the first query does not run (4.15a, rule 3).
        hide: bool,
        /// The other queries of the panel (4.15a): 0 to 7 items.
        extra: Vec<ExtraOut>,
    },
    /// The panel uses the frame of another panel.
    Reuse { panel: i64, with_transforms: bool, pointer: String },
    /// The panel has no query: a text panel (4.33a).
    None,
}

/// One item of `data.extra_queries` after the mapping (4.15a).
#[derive(Debug, Clone)]
pub(crate) struct ExtraOut {
    pub query_ref: String,
    pub source: String,
    pub format: &'static str,
    pub sql: Vec<String>,
    pub hide: bool,
}

/// The refs of the queries of a panel: the Grafana `refId` and the ref of Hune (15.6). A matcher
/// `byFrameRefID` reads it.
pub(crate) type RefMap = BTreeMap<String, String>;

/// The time override of a panel: `time_from`, `time_shift` and `hide_time_override` (4.15).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TimeOverride {
    pub from: Option<String>,
    pub shift: Option<String>,
    pub hide: bool,
}

/// A data source reference of the input.
#[derive(Clone, Default)]
pub(crate) struct Ds<'a> {
    pub uid: Option<String>,
    pub ty: Option<String>,
    pub node: Option<Node<'a>>,
}

impl Ds<'_> {
    pub(crate) fn is_empty(&self) -> bool {
        self.uid.is_none() && self.ty.is_none()
    }
}

/// Reads a data source reference: an object with `type`, `uid` or `name`, or a text.
pub(crate) fn read_ds<'a>(ctx: &mut Ctx, node: Option<Node<'a>>) -> Ds<'a> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Ds::default() };
    if let Some(text) = node.str() {
        return Ds { uid: Some(text.to_owned()), ty: None, node: Some(node.clone()) };
    }
    let Some(mut w) = Walk::new(&node) else {
        ctx.unsupported(&node.ptr, Some(node.v), "the data source must be an object or a text", Fallback::PanelRemoved);
        return Ds::default();
    };
    let ty = w.take("type").and_then(|t| t.str().map(str::to_owned));
    let uid = w.take("uid").and_then(|t| t.str().map(str::to_owned));
    let name = w.take("name").and_then(|t| t.str().map(str::to_owned));
    w.meta(ctx, &["apiVersion"]);
    w.finish(ctx);
    Ds { uid: uid.or(name), ty, node: Some(node.clone()) }
}

/// Gives the name of a variable reference in a data source member: `$name`, `${name}` or `[[name]]` (15.9).
fn variable_ref(uid: &str) -> Option<&str> {
    let name = if let Some(rest) = uid.strip_prefix("${") {
        rest.strip_suffix('}')?
    } else if let Some(rest) = uid.strip_prefix("[[") {
        rest.strip_suffix("]]")?
    } else {
        uid.strip_prefix('$')?
    };
    (!name.is_empty() && name.bytes().all(is_name_byte)).then_some(name)
}

/// The source map for one data source uid or name (15.9).
pub(crate) enum Mapped {
    /// The map names the key. `ty` is the plugin type of a data source variable or of an item of `__inputs`.
    Source { key: String, source: String, ty: Option<String> },
    /// The uid is a variable reference that the map does not name, or that names no data source variable and no
    /// item of `__inputs` of the dashboard.
    Variable,
    /// The map does not name the uid.
    Absent,
}

impl Mapped {
    /// The key and the source id, when the map names the key.
    pub(crate) fn pair(self) -> Option<(String, String)> {
        match self {
            Mapped::Source { key, source, .. } => Some((key, source)),
            Mapped::Variable | Mapped::Absent => None,
        }
    }
}

/// The reason of a reference to a data source variable that the map does not name (15.9).
pub(crate) const UNMAPPED_VARIABLE: &str = "the data source is a variable that the source map does not name";

/// Finds the source of a data source uid or name (15.9). A reference `$name`, `${name}` or `[[name]]` to a data
/// source variable or to an item of `__inputs` takes the source that the map gives to `name`.
pub(crate) fn map_source(ctx: &Ctx, opts: &ImportOptions, uid: &str) -> Mapped {
    if uid.starts_with('$') || uid.starts_with("[[") {
        let found = variable_ref(uid).and_then(|n| Some((n, ctx.source_name(n)?, opts.source_map.get(n)?)));
        return match found {
            Some((name, ty, source)) => Mapped::Source { key: name.to_owned(), source: source.clone(), ty: ty.clone() },
            None => Mapped::Variable,
        };
    }
    match opts.source_map.get(uid) {
        Some(source) => Mapped::Source { key: uid.to_owned(), source: source.clone(), ty: None },
        None => Mapped::Absent,
    }
}

/// The kind of data source of a query.
enum Source {
    /// A source of the source map with SQL, with the dialect of its SQL.
    Sql {
        id: String,
        dialect: Dialect,
    },
    /// A source of the source map of the kind `mongodb`.
    Mongo {
        id: String,
    },
    Dashboard,
}

/// The kind that a Grafana data source type gives: `Some` for a ClickHouse, a PostgreSQL and a MongoDB type.
fn type_kind(ty: Option<&str>) -> Option<SourceKind> {
    match ty {
        Some(CLICKHOUSE_TYPE) => Some(SourceKind::ClickHouse),
        Some(MONGO_TYPE) => Some(SourceKind::MongoDb),
        Some(t) if POSTGRES_TYPES.contains(&t) => Some(SourceKind::Postgres),
        _ => None,
    }
}

/// The kind of a data source (15.9). The type decides. A reference with no type (an old text reference)
/// takes the kind of the block of the configuration, and else ClickHouse. The function tells the use of
/// the source to the collector when the type gave the kind: the command checks it against the
/// configuration.
pub(crate) fn kind_of(ctx: &mut Ctx, opts: &ImportOptions, ds: &Ds, grafana: &str, source: &str) -> SourceKind {
    match type_kind(ds.ty.as_deref()) {
        Some(k) => {
            ctx.use_source(grafana, source, k);
            k
        }
        None => opts.source_kinds.get(source).copied().unwrap_or(SourceKind::ClickHouse),
    }
}

/// Finds the kind of a data source. An item names the fault. The fallback of the item is `unusable`: the
/// panel is removed for a panel with one query, and the target is removed for a panel with several (15.6).
fn classify(ctx: &mut Ctx, opts: &ImportOptions, ds: &Ds, at: &Node, unusable: &Fallback) -> Option<Source> {
    let node = ds.node.as_ref().unwrap_or(at);
    let uid = ds.uid.as_deref();
    let ty = ds.ty.as_deref();
    let fail = |ctx: &mut Ctx, why: String| {
        ctx.unsupported(&node.ptr, Some(node.v), &why, unusable.clone());
        None
    };
    if uid == Some(DASHBOARD_DS) {
        return Some(Source::Dashboard);
    }
    if matches!(uid, Some("-- Mixed --" | "-- Grafana --")) {
        return fail(ctx, format!("the data source {} has no equal in format 1", uid.unwrap_or_default()));
    }
    let mapped = uid.map(|u| map_source(ctx, opts, u));
    if matches!(mapped, Some(Mapped::Variable)) {
        return fail(ctx, UNMAPPED_VARIABLE.to_owned());
    }
    // A reference with no type takes the plugin type of the data source variable or of the item of `__inputs`.
    let typed;
    let ds = match &mapped {
        Some(Mapped::Source { ty: Some(t), .. }) if ty.is_none() => {
            typed = Ds { ty: Some(t.clone()), ..ds.clone() };
            &typed
        }
        _ => ds,
    };
    if let Some(t) = ds.ty.as_deref()
        && type_kind(Some(t)).is_none()
    {
        return fail(ctx, format!("the data source type {t} is not supported"));
    }
    match (uid, mapped) {
        (_, Some(Mapped::Source { key, source, .. })) => {
            let kind = kind_of(ctx, opts, ds, &key, &source);
            Some(match kind.dialect() {
                Some(dialect) => Source::Sql { id: source, dialect },
                None => Source::Mongo { id: source },
            })
        }
        (Some(u), _) => fail(ctx, format!("the data source {u} is not in the source map")),
        (None, _) => fail(ctx, "the query names no data source".to_owned()),
    }
}

/// The members of one query that the mapping reads.
pub(crate) struct QueryRaw<'a> {
    pub node: Node<'a>,
    pub hidden: Option<Node<'a>>,
    /// The `refId` of the target (4.15a).
    pub ref_id: Option<Node<'a>>,
    pub ds: Ds<'a>,
    pub raw_sql: Option<Node<'a>>,
    pub format: Option<Node<'a>>,
    pub editor: Option<Node<'a>>,
    pub builder: Option<Node<'a>>,
    pub panel_id: Option<Node<'a>>,
    pub with_transforms: Option<Node<'a>>,
    /// The members of the PostgreSQL plugin: `editorMode`, `rawQuery`, and the members of the builder.
    pub pg_editor_mode: Option<Node<'a>>,
    pub pg_members: Vec<Node<'a>>,
    /// The members of the MongoDB plugin: `queryText`, `collection`, `database` and `queryType`.
    pub query_text: Option<Node<'a>>,
    pub collection: Option<Node<'a>>,
    pub database: Option<Node<'a>>,
    pub query_type: Option<Node<'a>>,
}

fn plugin_members<'a>(ctx: &mut Ctx, mut w: Walk<'a>, q: &mut QueryRaw<'a>) {
    q.raw_sql = w.take("rawSql");
    q.format = w.take("format");
    q.editor = w.take("editorType");
    q.builder = w.take("builderOptions");
    q.panel_id = w.take("panelId");
    q.with_transforms = w.take("withTransforms");
    q.pg_editor_mode = w.take("editorMode");
    for key in ["rawQuery", "table", "dataset", "columns", "sql"] {
        q.pg_members.extend(w.take(key));
    }
    q.query_text = w.take("queryText");
    q.collection = w.take("collection");
    q.database = w.take("database");
    q.query_type = w.take("queryType");
    w.meta(ctx, &["meta", "pluginVersion"]);
    w.finish(ctx);
}

/// Reads one query. The v1 form is the target. The v2 form is a `PanelQuery`.
pub(crate) fn read_query<'a>(ctx: &mut Ctx, form: Form, node: &Node<'a>) -> Option<QueryRaw<'a>> {
    let mut q = QueryRaw {
        node: node.clone(),
        hidden: None,
        ref_id: None,
        ds: Ds::default(),
        raw_sql: None,
        format: None,
        editor: None,
        builder: None,
        panel_id: None,
        with_transforms: None,
        pg_editor_mode: None,
        pg_members: Vec::new(),
        query_text: None,
        collection: None,
        database: None,
        query_type: None,
    };
    let Some(mut w) = Walk::new(node) else {
        ctx.unsupported(&node.ptr, Some(node.v), "a query must be an object", Fallback::PanelRemoved);
        return None;
    };
    if !form.is_v2() {
        q.hidden = w.take("hide");
        q.ds = read_ds(ctx, w.take("datasource"));
        q.ref_id = w.take("refId");
        plugin_members(ctx, w, &mut q);
        return Some(q);
    }
    w.meta(ctx, &["kind"]);
    let spec = w.take("spec");
    w.finish(ctx);
    let mut sw = spec.as_ref().and_then(Walk::new)?;
    let query = sw.take("query");
    q.hidden = sw.take("hidden");
    let panel_ds = read_ds(ctx, sw.take("datasource"));
    q.ref_id = sw.take("refId");
    sw.finish(ctx);
    let mut kind_type: Option<String> = None;
    let mut inner_ds = Ds::default();
    if let Some(query) = query
        && let Some(mut qw) = Walk::new(&query)
    {
        let kind = qw.take("kind").and_then(|k| k.str().map(str::to_owned));
        let group = qw.take("group").and_then(|k| k.str().map(str::to_owned));
        qw.meta(ctx, &["version"]);
        inner_ds = read_ds(ctx, qw.take("datasource"));
        kind_type = group.or(kind.filter(|k| k != "DataQuery" && k != "DataQueryKind"));
        if let Some(plugin) = qw.take("spec") {
            if let Some(pw) = Walk::new(&plugin) {
                plugin_members(ctx, pw, &mut q);
            } else {
                ctx.unsupported(
                    &plugin.ptr,
                    Some(plugin.v),
                    "the query spec must be an object",
                    Fallback::PanelRemoved,
                );
            }
        }
        qw.finish(ctx);
    }
    q.ds = Ds {
        uid: panel_ds.uid.clone().or(inner_ds.uid.clone()),
        ty: panel_ds.ty.clone().or(inner_ds.ty.clone()).or(kind_type),
        node: panel_ds.node.clone().or(inner_ds.node.clone()),
    };
    Some(q)
}

/// The members of `queryOptions` that give the member `query` (4.15).
pub(crate) struct QueryOptions<'a> {
    pub interval: Option<Node<'a>>,
    pub max_data_points: Option<Node<'a>>,
    pub time_from: Option<Node<'a>>,
    pub time_shift: Option<Node<'a>>,
    pub hide_time_override: Option<Node<'a>>,
}

/// Maps `timeFrom` (15.6): `<count><unit>` gives `now-<count><unit>`, and a text of the grammar of
/// 4.15 stays. An empty text gives no member.
fn time_from(ctx: &mut Ctx, node: Option<&Node>) -> Option<String> {
    let node = node.filter(|n| !n.is_null())?;
    let fallback = || Fallback::MemberRemoved;
    let Some(text) = node.str() else {
        ctx.unsupported(&node.ptr, Some(node.v), "timeFrom must be a text", fallback());
        return None;
    };
    if text.is_empty() {
        ctx.ok();
        return None;
    }
    let candidate = if parse_time_shift(text).is_some() { format!("now-{text}") } else { text.to_owned() };
    if parse_time_from(&candidate).is_some() {
        ctx.ok();
        Some(candidate)
    } else {
        let why = "timeFrom must be <count><unit>, now-<count><unit>[/<unit>] or now/<unit> (4.15)";
        ctx.unsupported(&node.ptr, Some(node.v), why, fallback());
        None
    }
}

/// Maps `timeShift` (15.6): `<count><unit>`. An empty text gives no member.
fn time_shift(ctx: &mut Ctx, node: Option<&Node>) -> Option<String> {
    let node = node.filter(|n| !n.is_null())?;
    let Some(text) = node.str() else {
        ctx.unsupported(&node.ptr, Some(node.v), "timeShift must be a text", Fallback::MemberRemoved);
        return None;
    };
    if text.is_empty() {
        ctx.ok();
        return None;
    }
    if parse_time_shift(text).is_some() {
        ctx.ok();
        Some(text.to_owned())
    } else {
        let why = "timeShift must be <count><unit>: no sign, no /, no now and no variable (4.15)";
        ctx.unsupported(&node.ptr, Some(node.v), why, Fallback::MemberRemoved);
        None
    }
}

/// Maps `hideTimeOverride` (15.6): `true` maps. `false` and an absent member are metadata.
fn hide_time_override(ctx: &mut Ctx, node: Option<&Node>) -> bool {
    let Some(node) = node.filter(|n| !n.is_null()) else { return false };
    match &node.v {
        serde_json::Value::Bool(true) => {
            ctx.ok();
            true
        }
        serde_json::Value::Bool(false) => {
            ctx.meta();
            false
        }
        _ => {
            ctx.unsupported(&node.ptr, Some(node.v), "hideTimeOverride must be true or false", Fallback::MemberRemoved);
            false
        }
    }
}

/// Maps the three members of the time override of a panel with its own query.
fn time_override(ctx: &mut Ctx, options: &QueryOptions) -> TimeOverride {
    TimeOverride {
        from: time_from(ctx, options.time_from.as_ref()),
        shift: time_shift(ctx, options.time_shift.as_ref()),
        hide: hide_time_override(ctx, options.hide_time_override.as_ref()),
    }
}

/// A panel with the data source `-- Dashboard --` has no time member (4.15): each member that
/// turns a feature on is `unsupported`. A trivial value is metadata.
fn no_time_override(ctx: &mut Ctx, options: &QueryOptions) {
    for node in [&options.time_from, &options.time_shift, &options.hide_time_override].into_iter().flatten() {
        if crate::item::is_trivial(node.v) {
            ctx.meta();
        } else {
            let why = "a panel that reuses another panel has no time override (4.15)";
            ctx.unsupported(&node.ptr, Some(node.v), why, Fallback::MemberRemoved);
        }
    }
}

fn interval(ctx: &mut Ctx, node: Option<&Node>) -> Option<String> {
    let node = node.filter(|n| !n.is_null())?;
    let Some(text) = node.str() else {
        ctx.unsupported(&node.ptr, Some(node.v), "the interval must be a text", Fallback::other("null"));
        return None;
    };
    if text.is_empty() {
        ctx.ok();
        return None;
    }
    let bare = text.strip_prefix('>').unwrap_or(text);
    match parse_duration_ms(bare).filter(|ms| (1..=86_400_000).contains(ms)) {
        Some(_) => {
            ctx.ok();
            Some(bare.to_owned())
        }
        None => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "the interval is not a duration from 1ms to 1d",
                Fallback::other("null"),
            );
            None
        }
    }
}

fn max_points(ctx: &mut Ctx, node: Option<&Node>) -> Option<i64> {
    let node = node.filter(|n| !n.is_null())?;
    match as_int(node.v).filter(|v| (1..=10_000).contains(v)) {
        Some(v) => {
            ctx.ok();
            Some(v)
        }
        None => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "maxDataPoints must be an integer from 1 to 10000",
                Fallback::DashboardNotImported,
            );
            None
        }
    }
}

fn format_of(ctx: &mut Ctx, q: &QueryRaw, logs: bool) -> Option<&'static str> {
    match q.format.as_ref().filter(|f| !f.is_null()) {
        None => {
            ctx.changed(&q.node.child_ptr("format"), None, "the format is absent: the plugin default is table");
            Some("table")
        }
        Some(f) => match as_int(f.v) {
            Some(1) => {
                ctx.ok();
                Some("table")
            }
            Some(0) => {
                ctx.ok();
                Some("time_series")
            }
            // The format 2 is the logs format of the plugin: a logs panel keeps it as `logs` (4.33f). Grafana draws it as
            // the format 1 on another panel (15.6).
            Some(2) if logs => {
                ctx.ok();
                Some("logs")
            }
            Some(2) => {
                let why = "the logs format 2 of the plugin becomes table: Grafana draws it as the format 1 on a panel that is not a logs panel";
                ctx.changed(&f.ptr, Some(f.v), why);
                Some("table")
            }
            _ => {
                ctx.unsupported(&f.ptr, Some(f.v), "this format is not in the measured set", Fallback::PanelRemoved);
                None
            }
        },
    }
}

/// The format of a PostgreSQL query: a text, `table` or `time_series` (15.6).
fn format_of_pg(ctx: &mut Ctx, q: &QueryRaw) -> Option<&'static str> {
    match q.format.as_ref().filter(|f| !f.is_null()) {
        None => {
            ctx.changed(&q.node.child_ptr("format"), None, "the format is absent: the plugin default is table");
            Some("table")
        }
        Some(f) => match f.str() {
            Some("table") => {
                ctx.ok();
                Some("table")
            }
            Some("time_series") => {
                ctx.ok();
                Some("time_series")
            }
            _ => {
                ctx.unsupported(
                    &f.ptr,
                    Some(f.v),
                    "the format of a PostgreSQL query must be the text table or time_series",
                    Fallback::PanelRemoved,
                );
                None
            }
        },
    }
}

/// The members of the MongoDB plugin in a SQL query. `queryType` is metadata for a SQL query (15.6).
/// The three other members are unknown here (15.5).
fn sql_plugin_members(ctx: &mut Ctx, q: &QueryRaw) {
    if q.query_type.is_some() {
        ctx.meta();
    }
    for n in q.query_text.iter().chain(&q.collection).chain(&q.database) {
        ctx.unknown_member(&n.ptr, n.v);
    }
}

/// The members of a MongoDB query (15.6). The members of the SQL plugins are unknown here. The member
/// `queryType` gives the format.
fn mongo_members(ctx: &mut Ctx, q: &QueryRaw) {
    let sql_members =
        [&q.raw_sql, &q.format, &q.editor, &q.builder, &q.pg_editor_mode, &q.panel_id, &q.with_transforms];
    for n in sql_members.into_iter().flatten().chain(&q.pg_members) {
        ctx.unknown_member(&n.ptr, n.v);
    }
}

/// The format of a MongoDB query: `table` or `timeseries` (15.6). An absent `queryType` is `changed` to
/// `table`.
fn format_of_mongo(ctx: &mut Ctx, q: &QueryRaw) -> Option<&'static str> {
    match q.query_type.as_ref().filter(|f| !f.is_null()) {
        None => {
            ctx.changed(&q.node.child_ptr("queryType"), None, "the queryType is absent: the format is table");
            Some("table")
        }
        Some(f) => match f.str() {
            Some("table") => {
                ctx.ok();
                Some("table")
            }
            Some("timeseries") => {
                ctx.ok();
                Some("time_series")
            }
            _ => {
                ctx.unsupported(
                    &f.ptr,
                    Some(f.v),
                    "the queryType of a MongoDB query must be the text table or timeseries",
                    Fallback::PanelRemoved,
                );
                None
            }
        },
    }
}

/// The members of a ClickHouse query: the editor type and the builder. The members of the PostgreSQL
/// plugin are unknown members here.
fn clickhouse_members(ctx: &mut Ctx, q: &QueryRaw) {
    for n in q.pg_editor_mode.iter().chain(&q.pg_members) {
        ctx.unknown_member(&n.ptr, n.v);
    }
    sql_plugin_members(ctx, q);
    let builder = q.editor.as_ref().and_then(Node::str) == Some("builder");
    match &q.editor {
        Some(e) if builder => {
            ctx.changed(&e.ptr, Some(e.v), "the SQL that the builder made stays: the builder form is removed");
        }
        Some(_) => ctx.meta(),
        None => {}
    }
    if q.builder.is_some() && !builder {
        ctx.meta();
    }
}

/// The members of a PostgreSQL query (15.6). The visual builder is `unsupported`: Hune has no builder
/// for PostgreSQL. A non-empty `rawSql` stays, and the builder members go. Else the panel is removed.
/// The members of a code query are metadata. The members of the ClickHouse plugin are unknown here.
/// The result is `None` when the panel is removed.
fn postgres_members(ctx: &mut Ctx, q: &QueryRaw) -> Option<()> {
    for n in q.editor.iter().chain(&q.builder) {
        ctx.unknown_member(&n.ptr, n.v);
    }
    sql_plugin_members(ctx, q);
    let mode = q.pg_editor_mode.as_ref();
    match mode.and_then(Node::str) {
        Some("builder") => {
            let e = mode?;
            let has_sql = q.raw_sql.as_ref().and_then(Node::str).is_some_and(|t| !t.trim().is_empty());
            let fallback = if has_sql {
                Fallback::other("the rawSql stays as the SQL text, and the builder members are removed")
            } else {
                Fallback::PanelRemoved
            };
            ctx.unsupported(
                &e.ptr,
                Some(e.v),
                "Hune has no query builder for PostgreSQL: the visual builder is not imported",
                fallback,
            );
            if !has_sql {
                return None;
            }
        }
        Some("code") | None => {
            if mode.is_some() {
                ctx.meta();
            }
            for _ in &q.pg_members {
                ctx.meta();
            }
        }
        Some(_) => {
            let e = mode?;
            ctx.unsupported(&e.ptr, Some(e.v), "the editorMode is not code or builder", Fallback::PanelRemoved);
            return None;
        }
    }
    Some(())
}

/// The result of the mapping of the queries of a panel.
pub(crate) struct QueryOut {
    pub data: DataOut,
    /// The SQL of each query, with the source id of the query: the first query, then the others.
    pub sqls: Vec<(String, Sql)>,
    /// The refs of the queries, for the matchers `byFrameRefID` (15.6).
    pub refs: RefMap,
}

/// One target after the mapping, before the refs are known.
struct Built<'a> {
    source: String,
    format: &'static str,
    sql: Sql,
    hide: bool,
    raw: QueryRaw<'a>,
}

/// The context that the queries of a dashboard share.
pub(crate) struct QueryCx<'a> {
    pub form: Form,
    pub opts: &'a ImportOptions,
    pub vars: &'a BTreeSet<String>,
    /// True for a panel of the type `logs`: the format 2 of the plugin gives `logs` (4.33f, 15.6).
    pub logs: bool,
}

/// Tells if the data source of a panel only names the targets that have their own data source: `-- Mixed --`
/// or `-- Grafana --` (15.6).
fn is_container_ds(ds: &Ds) -> bool {
    matches!(ds.uid.as_deref(), Some("-- Mixed --" | "-- Grafana --"))
}

/// Maps the `hide` member of a target (15.6): `true` (or 1) maps to `hide`. `false` (or 0), `null` and an
/// absent member give none. Another value is `unsupported`, as for an annotation. Grafana hides a target
/// for most such values, so the fallback keeps the query hidden: it does not run.
fn hide_of(ctx: &mut Ctx, q: &QueryRaw) -> bool {
    let Some(h) = q.hidden.as_ref().filter(|h| !h.is_null()) else {
        if q.hidden.is_some() {
            ctx.meta();
        }
        return false;
    };
    match as_flag(h.v) {
        Some(true) => {
            ctx.ok();
            true
        }
        Some(false) => {
            ctx.meta();
            false
        }
        None => {
            let fallback = Fallback::other("the query is hidden: hide = true");
            ctx.unsupported(&h.ptr, Some(h.v), "hide must be true, false, 0 or 1", fallback);
            true
        }
    }
}

/// Maps the targets of a panel to the queries of 4.15a, and gives the refs (15.6). Called with the kept
/// targets in order. A panel with one query has the ref `A` and no `refId` of its own (4.42). A panel with
/// several queries keeps each `refId` of the grammar of 4.3 that no other target has. Another `refId` is
/// `changed` to the first free letter of `A` to `Z`.
fn assign_refs(ctx: &mut Ctx, kept: &[Built<'_>]) -> (Vec<String>, RefMap) {
    let mut refs = RefMap::new();
    let ids: Vec<Option<&str>> = kept.iter().map(|b| b.raw.ref_id.as_ref().and_then(Node::str)).collect();
    if kept.len() == 1 {
        if let Some(id) = ids[0] {
            refs.insert(id.to_owned(), "A".to_owned());
        }
        if kept[0].raw.ref_id.is_some() {
            ctx.meta();
        }
        return (vec!["A".to_owned()], refs);
    }
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut assigned: Vec<Option<String>> = Vec::new();
    for id in &ids {
        match id {
            Some(t) if is_valid_query_ref(t) && claimed.insert((*t).to_owned()) => assigned.push(Some((*t).to_owned())),
            _ => assigned.push(None),
        }
    }
    let mut out = Vec::new();
    for (i, slot) in assigned.into_iter().enumerate() {
        let b = &kept[i];
        let query_ref = match slot {
            Some(r) => {
                ctx.ok();
                r
            }
            None => {
                let free =
                    ('A'..='Z').map(String::from).find(|l| !claimed.contains(l)).unwrap_or_else(|| "Z".to_owned());
                claimed.insert(free.clone());
                let at = b.raw.ref_id.clone();
                let (ptr, value) = match &at {
                    Some(n) => (n.ptr.clone(), Some(n.v)),
                    None => (b.raw.node.child_ptr("refId"), None),
                };
                let why = format!(
                    "the refId is absent, breaks the grammar of 4.3, or another target has it: the query gets the ref {free}"
                );
                ctx.changed(&ptr, value, &why);
                free
            }
        };
        if let Some(id) = ids[i] {
            refs.entry(id.to_owned()).or_insert_with(|| query_ref.clone());
        }
        out.push(query_ref);
    }
    (out, refs)
}

/// Maps one target of a panel with its own SQL or MongoDB query: the members of the plugin, the SQL text
/// and the format. A fault removes the panel.
fn build_target<'a>(
    ctx: &mut Ctx,
    qc: &QueryCx,
    q: QueryRaw<'a>,
    source: Source,
    override_format: Option<&'static str>,
) -> Option<Built<'a>> {
    let vars = qc.vars;
    let hide = hide_of(ctx, &q);
    match source {
        Source::Mongo { id: source_id } => {
            mongo_members(ctx, &q);
            let members = MongoMembers {
                query_text: q.query_text.clone(),
                collection: q.collection.clone(),
                database: q.database.clone(),
                query: q.node.clone(),
            };
            let sql = prepare_mongo(ctx, &members, vars)?;
            let format = match override_format {
                Some(f) => f,
                None => format_of_mongo(ctx, &q)?,
            };
            Some(Built { source: source_id, format, sql, hide, raw: q })
        }
        Source::Dashboard => None,
        Source::Sql { id: source_id, dialect } => {
            match dialect {
                Dialect::ClickHouse => clickhouse_members(ctx, &q),
                Dialect::Postgres => postgres_members(ctx, &q)?,
            }
            let Some(raw) = q.raw_sql.as_ref() else {
                ctx.unsupported(&q.node.ptr, Some(q.node.v), "the query has no rawSql", Fallback::PanelRemoved);
                return None;
            };
            let Some(text) = raw.str() else {
                ctx.unsupported(&raw.ptr, Some(raw.v), "rawSql must be a text", Fallback::PanelRemoved);
                return None;
            };
            let sql = prepare(ctx, raw, text, vars, Fallback::PanelRemoved, dialect)?;
            let format = match (override_format, dialect) {
                (Some(f), _) => f,
                (None, Dialect::ClickHouse) => format_of(ctx, &q, qc.logs)?,
                (None, Dialect::Postgres) => format_of_pg(ctx, &q)?,
            };
            Some(Built { source: source_id, format, sql, hide, raw: q })
        }
    }
}

/// Maps the queries of a panel (15.6, 4.15a). `panel_ds` is the data source of the panel. `override_format` is
/// the format that a transformation sets. `single_frame` is true for a `barchart` and an `xychart`: they keep
/// the first query only. A fault removes the panel (the item carries the fallback). A target whose data source
/// has no row is removed when the panel has several targets.
pub(crate) fn convert_queries(
    ctx: &mut Ctx,
    qc: &QueryCx,
    queries: Option<Node>,
    panel_ds: &Ds,
    options: &QueryOptions,
    override_format: Option<&'static str>,
    single_frame: bool,
) -> Option<QueryOut> {
    let mut list = queries.as_ref().map(Node::items).unwrap_or_default();
    if list.is_empty() {
        let ptr = queries.as_ref().map(|n| n.ptr.clone()).unwrap_or_default();
        ctx.unsupported(&ptr, None, "the panel has no query", Fallback::PanelRemoved);
        return None;
    }
    let removed = || Fallback::other("the item is removed");
    for extra in list.iter().skip(MAX_QUERIES_PER_PANEL) {
        let why = "a panel has 8 queries at most (4.15a, rule 5)";
        ctx.unsupported(&extra.ptr, Some(extra.v), why, removed());
    }
    list.truncate(MAX_QUERIES_PER_PANEL);
    if single_frame {
        for extra in list.iter().skip(1) {
            let why = "a bar chart, an xy chart, a logs panel and a map draw one frame: Hune keeps the first query (4.15a, rule 7)";
            ctx.unsupported(&extra.ptr, Some(extra.v), why, removed());
        }
        list.truncate(1);
    }
    let multi = list.len() > 1;
    let (form, opts) = (qc.form, qc.opts);
    // With several targets, a target whose data source has no row is removed (15.6).
    let unusable = if multi { Fallback::other("the target is removed") } else { Fallback::PanelRemoved };
    let container = is_container_ds(panel_ds);
    let mut kept: Vec<Built<'_>> = Vec::new();
    for (i, node) in list.iter().enumerate() {
        let q = read_query(ctx, form, node)?;
        let ds = if !q.ds.is_empty() {
            q.ds.clone()
        } else if container {
            Ds::default()
        } else {
            panel_ds.clone()
        };
        let Some(source) = classify(ctx, opts, &ds, &q.node, &unusable) else {
            if multi {
                continue;
            }
            return None;
        };
        if let Source::Dashboard = source {
            if i == 0 {
                return reuse_out(ctx, &q, options, &list);
            }
            let why = "a target of a panel with several targets cannot reuse another panel: a query is a source and an SQL text (4.15a, rule 9)";
            ctx.unsupported(&q.node.ptr, Some(q.node.v), why, Fallback::other("the target is removed"));
            continue;
        }
        kept.push(build_target(ctx, qc, q, source, override_format)?);
    }
    if kept.is_empty() {
        let ptr = queries.as_ref().map(|n| n.ptr.clone()).unwrap_or_default();
        ctx.unsupported(&ptr, None, "the panel has no usable query", Fallback::PanelRemoved);
        return None;
    }
    let (query_refs, refs) = assign_refs(ctx, &kept);
    let min_interval = interval(ctx, options.interval.as_ref());
    let max_data_points = max_points(ctx, options.max_data_points.as_ref());
    let time = time_override(ctx, options);
    let mut sqls = Vec::new();
    let mut extra = Vec::new();
    let mut first: Option<(String, &'static str, Vec<String>, bool)> = None;
    for (b, query_ref) in kept.into_iter().zip(query_refs.iter()) {
        sqls.push((b.source.clone(), b.sql.clone()));
        if first.is_none() {
            first = Some((b.source, b.format, b.sql.lines, b.hide));
        } else {
            extra.push(ExtraOut {
                query_ref: query_ref.clone(),
                source: b.source,
                format: b.format,
                sql: b.sql.lines,
                hide: b.hide,
            });
        }
    }
    let (source, format, sql, hide) = first?;
    Some(QueryOut {
        data: DataOut::Query {
            source,
            format,
            sql,
            min_interval,
            max_data_points,
            time,
            query_ref: query_refs.first().cloned().unwrap_or_else(|| "A".to_owned()),
            hide,
            extra,
        },
        sqls,
        refs,
    })
}

/// A panel whose first target has the data source `-- Dashboard --` reuses the frame of another panel (4.15). It
/// has one query: the targets after the first are removed (15.6).
fn reuse_out(ctx: &mut Ctx, q: &QueryRaw, options: &QueryOptions, list: &[Node]) -> Option<QueryOut> {
    for extra in list.iter().skip(1) {
        let why = "a panel that reuses another panel has one query (4.15, 4.15a)";
        ctx.unsupported(
            &extra.ptr,
            Some(extra.v),
            why,
            Fallback::other("the targets other than the first are removed"),
        );
    }
    let Some(id) = q.panel_id.as_ref().and_then(|p| as_int(p.v)).filter(|i| *i >= 1) else {
        ctx.unsupported(
            &q.node.ptr,
            Some(q.node.v),
            "the reuse needs the panelId of another panel",
            Fallback::PanelRemoved,
        );
        return None;
    };
    let with = q.with_transforms.as_ref().and_then(|w| as_flag(w.v)).unwrap_or(false);
    no_time_override(ctx, options);
    if q.ref_id.is_some() {
        ctx.meta();
    }
    if let Some(h) = &q.hidden {
        if as_flag(h.v) == Some(true) {
            ctx.changed(
                &h.ptr,
                Some(h.v),
                "a panel that reuses another panel has no hidden query: the panel shows the frame",
            );
        } else {
            ctx.meta();
        }
    }
    ctx.ok();
    let pointer = q.panel_id.as_ref().map_or_else(|| q.node.ptr.clone(), |p| p.ptr.clone());
    Some(QueryOut {
        data: DataOut::Reuse { panel: id, with_transforms: with, pointer },
        sqls: Vec::new(),
        refs: RefMap::new(),
    })
}
