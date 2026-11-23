//! The annotations of a dashboard (4.5b) and the rows of 15.6 for `annotations.list`.
//!
//! An item with a data source of the source map and a query becomes one annotation: a ClickHouse or a
//! PostgreSQL data source with a SQL text (`rawSql`), or a MongoDB data source with a pipeline
//! (`queryText`, `collection` and `database`), as for a panel (decisions ID-K142 and ID-K333).
//! The built-in item is metadata. An item that Hune cannot hold is `unsupported` and the importer
//! removes it. The form v1 was read on the exports of the reference Grafana and on the measure of the
//! worker of 19.20e. The form v2 follows the schema and no real export of Grafana confirms it.

use std::collections::BTreeSet;

use hune_core::document::is_valid_colour;
use hune_core::limits::{MAX_ANNOTATION_PANELS, MAX_ANNOTATIONS, MAX_TITLE_CHARS};
use hune_core::sql::{SqlUse, VariableDef, WarningKind, save_checks_for};

use crate::dashboard::Form;
use crate::item::{Ctx, Fallback, is_trivial};
use crate::mongoprep::{MongoMembers, prepare_mongo_with};
use crate::options::{ImportOptions, SourceKind};
use crate::out::Out;
use crate::query::{Ds, kind_of, read_ds};
use crate::sqlprep::{Sql, prepare};
use crate::util::{as_flag, as_int, chars};
use crate::walk::{Node, Walk};

/// The colour that Grafana draws for an absent or an unknown `iconColor` (15.6).
const MISSING_COLOUR: &str = "#808080";

/// A panel of the imported document, for the member `filter` (15.6).
#[derive(Debug, Clone, Copy)]
pub(crate) struct PanelRef {
    pub id: i64,
    /// `true` for a panel of the type `timeseries`: the only type that draws marks (9.19d).
    pub timeseries: bool,
}

/// An annotation after the mapping (4.5b).
#[derive(Debug, Clone)]
pub(crate) struct AnnotationOut {
    pub name: String,
    pub source: String,
    pub sql: Sql,
    pub color: String,
    pub enabled: bool,
    pub panels: Option<Vec<i64>>,
}

impl AnnotationOut {
    /// The members come in the order of 4.5b. `enabled` and `panels` are left out at their default.
    pub(crate) fn to_out(&self) -> Out {
        let mut m: Vec<(&str, Out)> = vec![
            ("name", Out::s(&self.name)),
            ("source", Out::s(&self.source)),
            ("sql", Out::strings(&self.sql.lines)),
            ("color", Out::s(&self.color)),
        ];
        if !self.enabled {
            m.push(("enabled", Out::Bool(false)));
        }
        if let Some(panels) = &self.panels {
            m.push(("panels", Out::Arr(panels.iter().map(|id| Out::Int(*id)).collect())));
        }
        Out::members(m)
    }
}

fn removed() -> Fallback {
    Fallback::other("item removed")
}

/// The context of the conversion of the list.
pub(crate) struct AnnotationCx<'a> {
    pub form: Form,
    pub opts: &'a ImportOptions,
    /// The names of the variables of the dashboard, for the rewrite of `[[name]]`.
    pub names: &'a BTreeSet<String>,
    /// The definitions of the variables, for the checks of 7.18.
    pub defs: &'a [VariableDef],
    /// The panels of the imported document.
    pub panels: &'a [PanelRef],
}

/// Maps the list `annotations.list` (v1) or `spec.annotations` (v2). The function keeps the order of the list.
pub(crate) fn convert_annotations(ctx: &mut Ctx, cx: &AnnotationCx<'_>, list: Option<Node<'_>>) -> Vec<AnnotationOut> {
    let Some(list) = list.filter(|l| !l.is_null()) else { return Vec::new() };
    if list.v.as_array().is_none() {
        if is_trivial(list.v) {
            ctx.meta();
        } else {
            ctx.unsupported(&list.ptr, Some(list.v), "annotations must be an array", Fallback::MemberRemoved);
        }
        return Vec::new();
    }
    let mut out: Vec<AnnotationOut> = Vec::new();
    for item in list.items() {
        let (spec, builtin) = spec_of(cx.form, &item);
        if builtin {
            ctx.meta();
            continue;
        }
        if out.len() >= MAX_ANNOTATIONS {
            let why = format!("a document has at most {MAX_ANNOTATIONS} annotations (4.35)");
            ctx.unsupported(&item.ptr, Some(item.v), &why, removed());
            continue;
        }
        if let Some(a) = convert_item(ctx, cx, &item, spec, &out) {
            out.push(a);
        }
    }
    out
}

/// Gives the node that holds the members of an item, and tells if the item is the built-in item.
/// In the form v2, the members are in `spec`.
fn spec_of<'a>(form: Form, item: &Node<'a>) -> (Option<Node<'a>>, bool) {
    let spec = if form.is_v2() { item.get("spec") } else { Some(item.clone()) };
    let builtin = spec.as_ref().and_then(|s| s.get("builtIn")).and_then(|b| as_flag(b.v)).unwrap_or(false);
    (spec, builtin)
}

/// The members of the plugin query that Hune does not read. They have no behaviour in Hune.
const QUERY_META: [&str; 11] = [
    "editorType",
    "meta",
    "pluginVersion",
    "builderOptions",
    "refId",
    "rawQuery",
    "table",
    "dataset",
    "columns",
    "sql",
    "hide",
];

/// What the walk of the query of an item gives. The members of the plugin are read before the kind of the
/// source is known: [`plugin_members`] gives their rows after it.
#[derive(Default)]
struct QueryParts<'a> {
    /// The node of the plugin query, for an item about a member that is absent.
    node: Option<Node<'a>>,
    sql: Option<Node<'a>>,
    ds: Ds<'a>,
    format: Option<Node<'a>>,
    /// `editorMode` of the PostgreSQL plugin: `code` or `builder`.
    editor_mode: Option<Node<'a>>,
    /// The members of the MongoDB plugin.
    query_text: Option<Node<'a>>,
    collection: Option<Node<'a>>,
    database: Option<Node<'a>>,
    query_type: Option<Node<'a>>,
}

/// Reads the query of an item: the member `target` (v1), or `query` with its `spec` (v2).
fn read_query<'a>(ctx: &mut Ctx, form: Form, w: &mut Walk<'a>) -> QueryParts<'a> {
    let mut parts = QueryParts::default();
    if form.is_v2() {
        let Some(q) = w.take("query").filter(|q| !q.is_null()) else { return parts };
        let Some(mut qw) = Walk::new(&q) else {
            ctx.unsupported(&q.ptr, Some(q.v), "the query must be an object", removed());
            return parts;
        };
        qw.meta(ctx, &["kind", "group", "version"]);
        parts.ds = read_ds(ctx, qw.take("datasource"));
        if let Some(spec) = qw.take("spec") {
            read_plugin_query(ctx, &spec, &mut parts);
        }
        qw.finish(ctx);
    } else if let Some(t) = w.take("target").filter(|t| !t.is_null()) {
        read_plugin_query(ctx, &t, &mut parts);
    }
    parts
}

/// Reads the members of the plugin query: `rawSql`, the members of the MongoDB plugin and the metadata.
fn read_plugin_query<'a>(ctx: &mut Ctx, node: &Node<'a>, parts: &mut QueryParts<'a>) {
    let Some(mut tw) = Walk::new(node) else {
        ctx.unsupported(&node.ptr, Some(node.v), "the query must be an object", removed());
        return;
    };
    parts.node = Some(node.clone());
    parts.sql = tw.take("rawSql").filter(|n| n.str().is_some());
    if let Some(ds) = tw.take("datasource") {
        let inner = read_ds(ctx, Some(ds));
        if parts.ds.is_empty() {
            parts.ds = inner;
        }
    }
    parts.format = tw.take("format");
    parts.editor_mode = tw.take("editorMode");
    parts.query_text = tw.take("queryText");
    parts.collection = tw.take("collection");
    parts.database = tw.take("database");
    parts.query_type = tw.take("queryType");
    tw.meta(ctx, &QUERY_META);
    tw.finish(ctx);
}

/// The text of the `changed` item of a format that is not `table`.
const TABLE_ONLY: &str = "Hune runs the query of an annotation with the format table (7.37)";

/// Gives the rows of the members of the plugin query once the kind of the source is known (15.6). A member of
/// the plugin of another kind is unknown here, as for a panel. The format `table` is the format of the
/// annotation query of Hune: another format is `changed`. The result is `false` when the item is removed.
fn plugin_members(ctx: &mut Ctx, kind: SourceKind, q: &QueryParts<'_>) -> bool {
    let unknown = |ctx: &mut Ctx, nodes: &[&Option<Node<'_>>]| {
        for n in nodes.iter().copied().flatten() {
            ctx.unknown_member(&n.ptr, n.v);
        }
    };
    if kind == SourceKind::MongoDb {
        unknown(ctx, &[&q.sql, &q.format, &q.editor_mode]);
        match q.query_type.as_ref().filter(|t| !t.is_null()) {
            Some(t) if t.str() == Some("table") => ctx.meta(),
            Some(t) => ctx.changed(&t.ptr, Some(t.v), TABLE_ONLY),
            None => {}
        }
        return true;
    }
    unknown(ctx, &[&q.query_text, &q.collection, &q.database]);
    if q.query_type.is_some() {
        ctx.meta();
    }
    if let Some(f) = q.format.as_ref().filter(|f| !f.is_null()) {
        if f.str() == Some("table") || as_int(f.v) == Some(1) {
            ctx.meta();
        } else {
            ctx.changed(&f.ptr, Some(f.v), TABLE_ONLY);
        }
    }
    let Some(mode) = q.editor_mode.as_ref() else { return true };
    match (kind, mode.str()) {
        // The visual builder of the PostgreSQL plugin: Hune has no builder, and the `rawSql` that it made stays.
        (SourceKind::Postgres, Some("builder"))
            if q.sql.as_ref().and_then(Node::str).is_some_and(|t| !t.trim().is_empty()) =>
        {
            let fallback = Fallback::other("the rawSql stays as the SQL text, and the builder members are removed");
            let why = "Hune has no query builder for PostgreSQL: the visual builder is not imported";
            ctx.unsupported(&mode.ptr, Some(mode.v), why, fallback);
        }
        (SourceKind::Postgres, Some("code" | "builder")) | (SourceKind::ClickHouse, _) => ctx.meta(),
        _ => {
            ctx.unsupported(&mode.ptr, Some(mode.v), "the editorMode is not code or builder", removed());
            return false;
        }
    }
    true
}

fn convert_item(
    ctx: &mut Ctx,
    cx: &AnnotationCx<'_>,
    item: &Node<'_>,
    spec: Option<Node<'_>>,
    done: &[AnnotationOut],
) -> Option<AnnotationOut> {
    let Some(spec) = spec else {
        ctx.unsupported(&item.ptr, Some(item.v), "an annotation item needs the member spec", removed());
        return None;
    };
    let Some(mut w) = Walk::new(&spec) else {
        ctx.unsupported(&item.ptr, Some(item.v), "an annotation item must be an object", removed());
        return None;
    };
    if cx.form.is_v2() {
        // The member `kind` of the item and the member `builtIn` are read above.
        if let Some(mut iw) = Walk::new(item) {
            iw.meta(ctx, &["kind"]);
            iw.take("spec");
            iw.finish(ctx);
        }
    }
    let name = w.take("name");
    let enable = w.take("enable");
    let hide = w.take("hide");
    let icon = w.take("iconColor");
    let filter = w.take("filter");
    let mappings = w.take("mappings");
    if let Some(b) = w.take("builtIn") {
        // A `builtIn` that is not true: metadata.
        let _ = b;
        ctx.meta();
    }
    let item_ds = read_ds(ctx, w.take("datasource"));
    let query = read_query(ctx, cx.form, &mut w);
    w.finish(ctx);

    // Each member is read before the first refusal, so the report names every fault of the item.
    let name = annotation_name(ctx, item, name.as_ref(), done);
    let enabled = enabled_of(ctx, enable.as_ref());
    let hide_ok = hide_of(ctx, hide.as_ref());
    let mappings_ok = mappings_of(ctx, mappings.as_ref());
    let panels = filter_of(ctx, cx, filter.as_ref());
    let source = source_of(ctx, cx, item, &item_ds, &query);
    let members_ok = source.as_ref().is_none_or(|(_, kind)| plugin_members(ctx, *kind, &query));
    let sql = match (&source, &name) {
        (Some((id, kind)), Some(n)) if members_ok => sql_of(ctx, cx, item, &query, n, id, *kind),
        _ => None,
    };
    let (name, enabled, panels, (source, _), sql) = (name?, enabled?, panels?, source?, sql?);
    if !(hide_ok && mappings_ok && members_ok) {
        return None;
    }
    // The colour is read last: an item that the importer removes has no colour to report.
    let color = colour_of(ctx, &spec, icon.as_ref());
    ctx.ok();
    // The SQL is not run at the import: the report says so for each annotation that the importer keeps (15.6).
    ctx.changed(
        &sql.pointer,
        None,
        &format!(
            "the SQL of the annotation {name:?} is not checked against the rules of 7.37: the import runs no query"
        ),
    );
    Some(AnnotationOut { name, source, sql, color, enabled, panels })
}

/// Reads the name: 1 to 256 characters. A name that an earlier annotation has gets a number: `<name> 2`.
fn annotation_name(ctx: &mut Ctx, item: &Node, name: Option<&Node>, done: &[AnnotationOut]) -> Option<String> {
    let Some(node) = name else {
        ctx.unsupported(&item.child_ptr("name"), None, "an annotation needs a name (4.5b)", removed());
        return None;
    };
    let Some(text) = node.str().filter(|t| (1..=MAX_TITLE_CHARS).contains(&chars(t))) else {
        ctx.unsupported(&node.ptr, Some(node.v), "the name must be a text of 1 to 256 characters", removed());
        return None;
    };
    if text.chars().any(|c| c.is_control()) {
        ctx.unsupported(&node.ptr, Some(node.v), "the name has a control character", removed());
        return None;
    }
    if !done.iter().any(|a| a.name == text) {
        ctx.ok();
        return Some(text.to_owned());
    }
    let mut n = 2u32;
    let new = loop {
        let suffix = format!(" {n}");
        let keep = MAX_TITLE_CHARS.saturating_sub(suffix.chars().count());
        let base: String = text.chars().take(keep).collect();
        let candidate = format!("{base}{suffix}");
        if !done.iter().any(|a| a.name == candidate) {
            break candidate;
        }
        n += 1;
    };
    ctx.changed(&node.ptr, Some(node.v), &format!("the name exists already: the new name is {new:?} (4.5b)"));
    Some(new)
}

/// Reads `enable`: `false` gives `enabled` = `false`. An absent member is `true`.
fn enabled_of(ctx: &mut Ctx, node: Option<&Node>) -> Option<bool> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Some(true) };
    match as_flag(node.v) {
        Some(b) => {
            ctx.ok();
            Some(b)
        }
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "enable must be true or false", removed());
            None
        }
    }
}

/// Reads `hide`: `true` is metadata. `false` is listed, because the page has no toggle (ID-K157).
fn hide_of(ctx: &mut Ctx, node: Option<&Node>) -> bool {
    let Some(node) = node.filter(|n| !n.is_null()) else { return true };
    match as_flag(node.v) {
        Some(true) => {
            ctx.meta();
            true
        }
        Some(false) => {
            ctx.dropped(
                &node.ptr,
                Some(node.v),
                "Hune has no toggle for an annotation: enabled is a member of the document",
            );
            true
        }
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "hide must be true or false", removed());
            false
        }
    }
}

/// Reads `iconColor`: a colour of 4.19 maps. An absent, empty or other value is `changed` to `#808080`.
fn colour_of(ctx: &mut Ctx, item: &Node, node: Option<&Node>) -> String {
    match node {
        Some(n) if n.str().is_some_and(is_valid_colour) => {
            ctx.ok();
            n.str().unwrap_or(MISSING_COLOUR).to_owned()
        }
        Some(n) => {
            let reason = format!("the colour is not a form of 4.19: Grafana draws {MISSING_COLOUR}, and so does Hune");
            ctx.changed(&n.ptr, Some(n.v), &reason);
            MISSING_COLOUR.to_owned()
        }
        None => {
            let reason = format!("the colour is absent: Grafana draws {MISSING_COLOUR}, and so does Hune");
            ctx.changed(&item.child_ptr("iconColor"), None, &reason);
            MISSING_COLOUR.to_owned()
        }
    }
}

/// Reads `mappings`: Hune reads the columns by their names only (7.37). A non-trivial value is `unsupported`.
fn mappings_of(ctx: &mut Ctx, node: Option<&Node>) -> bool {
    let Some(node) = node else { return true };
    if is_trivial(node.v) {
        ctx.meta();
        return true;
    }
    ctx.unsupported(
        &node.ptr,
        Some(node.v),
        "Hune reads the columns of an annotation by their names only (7.37)",
        removed(),
    );
    false
}

/// Reads `filter` (15.6). The result is `Some(None)` for no member, `Some(Some(ids))` for a list of ids of the
/// imported time series, and `None` for a refusal.
fn filter_of(ctx: &mut Ctx, cx: &AnnotationCx<'_>, node: Option<&Node>) -> Option<Option<Vec<i64>>> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Some(None) };
    let Some(mut fw) = Walk::new(node) else {
        ctx.unsupported(&node.ptr, Some(node.v), "the filter must be an object", removed());
        return None;
    };
    let exclude = fw.take("exclude");
    let ids = fw.take("ids");
    fw.finish(ctx);
    let exclude = match exclude.as_ref().filter(|e| !e.is_null()) {
        None => false,
        Some(e) => match as_flag(e.v) {
            Some(b) => b,
            None => {
                ctx.unsupported(&e.ptr, Some(e.v), "exclude must be true or false", removed());
                return None;
            }
        },
    };
    // A list above the limit of the document names more ids than the document has panels. The check comes before
    // the loop, so a hostile list of 200,000 ids costs no work.
    if let Some(list) = ids.as_ref().and_then(|n| n.v.as_array()).filter(|a| a.len() > MAX_ANNOTATION_PANELS) {
        let reason =
            format!("the filter lists {} ids: Hune accepts {MAX_ANNOTATION_PANELS} at most (4.5b)", list.len());
        ctx.unsupported(&node.ptr, Some(node.v), &reason, removed());
        return None;
    }
    let mut listed: Vec<(i64, String)> = Vec::new();
    for id in ids.as_ref().map(Node::items).unwrap_or_default() {
        match as_int(id.v) {
            Some(n) if !listed.iter().any(|(l, _)| *l == n) => listed.push((n, id.ptr.clone())),
            Some(_) => ctx.changed(&id.ptr, Some(id.v), "an id that occurs two times leaves the list one time"),
            None => ctx.changed(&id.ptr, Some(id.v), "an item that is not a whole number leaves the list"),
        }
    }
    let timeseries = |id: i64| cx.panels.iter().any(|p| p.id == id && p.timeseries);
    let panels: Vec<i64> = if exclude {
        let kept: Vec<i64> = cx
            .panels
            .iter()
            .filter(|p| p.timeseries && !listed.iter().any(|(l, _)| *l == p.id))
            .map(|p| p.id)
            .collect();
        ctx.changed(
            &node.ptr,
            Some(node.v),
            "the filter excludes panels: the list names each time series that it does not exclude. A panel that a person adds later is not in the list",
        );
        kept
    } else {
        let mut kept = Vec::new();
        for (id, at) in &listed {
            if timeseries(*id) {
                kept.push(*id);
            } else {
                let reason = format!("the id {id} is not a time series of the document: it leaves the list");
                ctx.changed(at, None, &reason);
            }
        }
        if !kept.is_empty() {
            ctx.ok();
        }
        kept
    };
    if panels.is_empty() || panels.len() > MAX_ANNOTATION_PANELS {
        ctx.unsupported(
            &node.ptr,
            Some(node.v),
            "the filter names no time series of the document: the annotation would draw nowhere",
            removed(),
        );
        return None;
    }
    Some(Some(panels))
}

/// Finds the source of the annotation (15.9): the data source of the item or of its query, and its kind. A
/// ClickHouse, a PostgreSQL and a MongoDB source are valid (4.5b, decisions ID-K142 and ID-K333).
fn source_of(
    ctx: &mut Ctx,
    cx: &AnnotationCx<'_>,
    item: &Node,
    item_ds: &Ds<'_>,
    query: &QueryParts<'_>,
) -> Option<(String, SourceKind)> {
    let ds = if item_ds.is_empty() { &query.ds } else { item_ds };
    let at = ds.node.as_ref().unwrap_or(item);
    // A reference to a data source variable or to an item of `__inputs` takes the source of the map (15.9).
    let Some((grafana_uid, source)) = ds.uid.as_deref().and_then(|u| crate::query::map_source(ctx, cx.opts, u).pair())
    else {
        ctx.unsupported(&at.ptr, Some(at.v), "the data source of the annotation is not in the source map", removed());
        return None;
    };
    // The match has no default arm: a kind that a later version adds needs its own row of 15.6.
    let kind = match kind_of(ctx, cx.opts, ds, &grafana_uid, &source) {
        k @ (SourceKind::ClickHouse | SourceKind::Postgres | SourceKind::MongoDb) => k,
    };
    ctx.ok();
    Some((source, kind))
}

/// Prepares the query of an annotation and runs the checks of its kind: 7.18 for SQL (with the dialect of the
/// source), 7.36 for a MongoDB pipeline. A fault removes the item.
fn sql_of(
    ctx: &mut Ctx,
    cx: &AnnotationCx<'_>,
    item: &Node,
    query: &QueryParts<'_>,
    name: &str,
    source: &str,
    kind: SourceKind,
) -> Option<Sql> {
    let Some(dialect) = kind.dialect() else { return mongo_of(ctx, cx, item, query, name, source) };
    let Some(node) = query.sql.as_ref() else {
        ctx.unsupported(&item.child_ptr("target"), None, "the annotation has no SQL text (rawSql)", removed());
        return None;
    };
    let text = node.str()?;
    let sql = prepare(ctx, node, text, cx.names, removed(), dialect)?;
    let checked = save_checks_for(dialect, &sql.text, SqlUse::Annotation, cx.defs);
    let mut fault = false;
    for f in &checked.faults {
        fault = true;
        let reason =
            format!("SQL of the annotation {name:?}: line {}, column {}: {}", f.line_index + 1, f.column, f.message);
        ctx.unsupported(&sql.pointer, None, &reason, removed());
    }
    for w in checked
        .warnings
        .iter()
        .filter(|w| matches!(w.kind, WarningKind::MacroInLiteral | WarningKind::UnknownFormatInString))
    {
        let reason =
            format!("SQL of the annotation {name:?}: line {}, column {}: {}", w.line_index + 1, w.column, w.message);
        ctx.changed(&sql.pointer, None, &reason);
    }
    (!fault).then_some(sql)
}

/// Prepares the pipeline of an annotation on a MongoDB source (7.29) from `queryText`, `collection` and `database`,
/// and runs the checks of 7.36 with the use `annotation` (L9). The lists of the block come from the options, as for
/// a panel. A fault removes the item.
fn mongo_of(
    ctx: &mut Ctx,
    cx: &AnnotationCx<'_>,
    item: &Node,
    query: &QueryParts<'_>,
    name: &str,
    source: &str,
) -> Option<Sql> {
    let members = MongoMembers {
        query_text: query.query_text.clone(),
        collection: query.collection.clone(),
        database: query.database.clone(),
        query: query.node.clone().unwrap_or_else(|| item.clone()),
    };
    let sql = prepare_mongo_with(ctx, &members, cx.names, &removed())?;
    let checked = crate::mongoprep::check(&sql, cx.defs, SqlUse::Annotation, cx.opts.mongo_lists.get(source));
    for reason in &checked.faults {
        ctx.unsupported(&sql.pointer, None, &format!("MongoDB query of the annotation {name:?}: {reason}"), removed());
    }
    for reason in &checked.changes {
        ctx.changed(&sql.pointer, None, &format!("MongoDB query of the annotation {name:?}: {reason}"));
    }
    checked.faults.is_empty().then_some(sql)
}
