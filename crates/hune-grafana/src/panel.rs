// SPDX-License-Identifier: MIT OR Apache-2.0
//! The mapping of one panel: the panel members of 15.6 for the v1 and the v2 forms.

use std::collections::BTreeSet;

use hune_core::limits::MAX_DESCRIPTION_CHARS;
use serde_json::Value;

use crate::dashboard::Form;
use crate::fields::{FieldCx, field_config, links};
use crate::item::{Ctx, Fallback};
use crate::options::ImportOptions;
use crate::out::Out;
use crate::panelopts::{Pt, old_heatmap_member, options_with_fields};
use crate::query::{DataOut, QueryCx, QueryOptions, QueryOut, convert_queries, read_ds};
use crate::repeat::{COPY_METADATA, PanelRepeatOut, RepeatRaw, RepeatVars};
use crate::sqlprep::Sql;
use crate::transforms::transforms;
use crate::util::{as_flag, as_int, chars};
use crate::walk::{Node, Walk};

/// The context that the panels share.
pub(crate) struct PanelCx<'a> {
    pub form: Form,
    pub opts: &'a ImportOptions,
    pub vars: &'a BTreeSet<String>,
    /// The variables that can repeat (4.13a).
    pub repeat_vars: &'a RepeatVars,
}

/// The place of a panel in the grid, as nodes of the input.
pub(crate) struct GridRaw<'a> {
    pub at: Node<'a>,
    pub x: Option<Node<'a>>,
    pub y: Option<Node<'a>>,
    pub w: Option<Node<'a>>,
    pub h: Option<Node<'a>>,
}

/// A panel of the input and its place.
pub(crate) struct PanelSrc<'a> {
    pub node: Node<'a>,
    pub grid: GridRaw<'a>,
    /// The row unit to subtract from the y of a v1 panel (15.6).
    pub y_base: i64,
    /// The kind of a v2 element: `Panel` or `LibraryPanel`.
    pub element_kind: Option<String>,
    /// The repeat member of a v2 grid item.
    pub repeat: Option<Node<'a>>,
}

/// A panel after the mapping.
pub(crate) struct PanelOut {
    pub scope: usize,
    pub id: i64,
    pub ptype: Pt,
    pub title: String,
    pub description: String,
    pub transparent: bool,
    pub grid: [i64; 4],
    pub data: DataOut,
    pub transforms: Vec<Out>,
    pub fields: Out,
    pub options: Out,
    pub links: Vec<Out>,
    /// The SQL of each query with the id of its source: the first query, then the others (4.15a).
    pub sqls: Vec<(String, Sql)>,
    pub pointer: String,
    /// The repeat of the panel (4.13a). `None`: the panel does not repeat.
    pub repeat: Option<PanelRepeatOut>,
}

impl PanelOut {
    fn grid_out(&self) -> Out {
        Out::obj([
            ("x", Out::Int(self.grid[0])),
            ("y", Out::Int(self.grid[1])),
            ("w", Out::Int(self.grid[2])),
            ("h", Out::Int(self.grid[3])),
        ])
    }

    pub(crate) fn to_out(&self) -> Out {
        let data = match &self.data {
            DataOut::Query { source, format, sql, min_interval, max_data_points, time, query_ref, hide, extra } => {
                let mut members = vec![
                    ("source", Out::s(source)),
                    ("format", Out::s(format)),
                    ("sql", Out::strings(sql)),
                    ("min_interval", Out::opt_s(min_interval.as_deref())),
                    ("max_data_points", max_data_points.map_or(Out::Null, Out::Int)),
                ];
                if let Some(from) = &time.from {
                    members.push(("time_from", Out::s(from)));
                }
                if let Some(shift) = &time.shift {
                    members.push(("time_shift", Out::s(shift)));
                }
                if time.hide {
                    members.push(("hide_time_override", Out::Bool(true)));
                }
                // `ref` and `hide` are the last members of `data.query` (4.38).
                if query_ref != "A" {
                    members.push(("ref", Out::s(query_ref)));
                }
                if *hide {
                    members.push(("hide", Out::Bool(true)));
                }
                let mut data = vec![("query", Out::members(members))];
                if !extra.is_empty() {
                    let items = extra.iter().map(|e| {
                        let mut m = vec![
                            ("ref", Out::s(&e.query_ref)),
                            ("source", Out::s(&e.source)),
                            ("format", Out::s(e.format)),
                            ("sql", Out::strings(&e.sql)),
                        ];
                        if e.hide {
                            m.push(("hide", Out::Bool(true)));
                        }
                        Out::members(m)
                    });
                    data.push(("extra_queries", Out::Arr(items.collect())));
                }
                Out::members(data)
            }
            DataOut::Reuse { panel, with_transforms, .. } => Out::obj([(
                "reuse",
                Out::obj([("panel", Out::Int(*panel)), ("with_transforms", Out::Bool(*with_transforms))]),
            )]),
            // A text panel has no `data`, no `transforms` and no `fields` (4.33a, 4.38).
            DataOut::None => {
                return Out::members(vec![
                    ("id", Out::Int(self.id)),
                    ("type", Out::s(self.ptype.as_str())),
                    ("title", Out::s(&self.title)),
                    ("description", Out::s(&self.description)),
                    ("transparent", Out::Bool(self.transparent)),
                    ("grid", self.grid_out()),
                    ("options", self.options.clone()),
                    ("links", Out::Arr(self.links.clone())),
                ]);
            }
        };
        let mut members = vec![
            ("id", Out::Int(self.id)),
            ("type", Out::s(self.ptype.as_str())),
            ("title", Out::s(&self.title)),
            ("description", Out::s(&self.description)),
            ("transparent", Out::Bool(self.transparent)),
            ("grid", self.grid_out()),
            ("data", data),
            ("transforms", Out::Arr(self.transforms.clone())),
            ("fields", self.fields.clone()),
            ("options", self.options.clone()),
            ("links", Out::Arr(self.links.clone())),
        ];
        if let Some(r) = &self.repeat {
            members.push(("repeat", r.to_out()));
        }
        Out::members(members)
    }
}

/// The members that the two forms hold in different places.
struct Raw<'a> {
    id: Option<Node<'a>>,
    title: Option<Node<'a>>,
    description: Option<Node<'a>>,
    transparent: Option<Node<'a>>,
    options: Option<Node<'a>>,
    field_config: Option<Node<'a>>,
    links: Option<Node<'a>>,
    queries: Option<Node<'a>>,
    transformations: Option<Node<'a>>,
    panel_ds: Option<Node<'a>>,
    query_options: QueryOptions<'a>,
    /// The repeat members of a v1 panel. A v2 grid item gives its own (`PanelSrc::repeat`).
    repeat: Option<RepeatRaw<'a>>,
    /// `repeatPanelId` of a v1 panel: the panel is a stored copy.
    copy_of: Option<Node<'a>>,
}

fn peek_type(form: Form, node: &Node) -> Option<String> {
    if !form.is_v2() {
        return node.get("type").and_then(|t| t.str().map(str::to_owned));
    }
    let viz = node.get("vizConfig")?;
    let group = viz.get("group").and_then(|g| g.str().map(str::to_owned));
    group.or_else(|| viz.get("kind").and_then(|k| k.str().map(str::to_owned)).filter(|k| k != "VizConfig"))
}

fn read_raw<'a>(ctx: &mut Ctx, form: Form, node: &Node<'a>) -> Option<Raw<'a>> {
    let mut w = Walk::new(node).or_else(|| {
        ctx.unsupported(&node.ptr, Some(node.v), "a panel must be an object", Fallback::PanelRemoved);
        None
    })?;
    let mut raw = Raw {
        id: w.take("id"),
        title: w.take("title"),
        description: w.take("description"),
        transparent: w.take("transparent"),
        options: None,
        field_config: None,
        links: w.take("links"),
        queries: None,
        transformations: None,
        panel_ds: None,
        query_options: QueryOptions {
            interval: None,
            max_data_points: None,
            time_from: None,
            time_shift: None,
            hide_time_override: None,
        },
        repeat: None,
        copy_of: None,
    };
    if form.is_v2() {
        if let Some(viz) = w.take("vizConfig")
            && let Some(mut vw) = Walk::new(&viz)
        {
            vw.take("kind");
            vw.take("group");
            vw.meta(ctx, &["version"]);
            let spec = vw.take("spec");
            vw.finish(ctx);
            if let Some(mut sw) = spec.as_ref().and_then(Walk::new) {
                sw.meta(ctx, &["pluginVersion"]);
                raw.options = sw.take("options");
                raw.field_config = sw.take("fieldConfig");
                sw.finish(ctx);
            }
        }
        if let Some(data) = w.take("data")
            && let Some(mut dw) = Walk::new(&data)
        {
            dw.meta(ctx, &["kind"]);
            let spec = dw.take("spec");
            dw.finish(ctx);
            if let Some(mut sw) = spec.as_ref().and_then(Walk::new) {
                raw.queries = sw.take("queries");
                raw.transformations = sw.take("transformations");
                if let Some(qo) = sw.take("queryOptions")
                    && let Some(mut qw) = Walk::new(&qo)
                {
                    raw.query_options.interval = qw.take("interval");
                    raw.query_options.max_data_points = qw.take("maxDataPoints");
                    raw.query_options.time_from = qw.take("timeFrom");
                    raw.query_options.time_shift = qw.take("timeShift");
                    raw.query_options.hide_time_override = qw.take("hideTimeOverride");
                    qw.meta(ctx, &["cacheTimeout", "queryCachingTTL"]);
                    qw.finish(ctx);
                }
                sw.finish(ctx);
            }
        }
    } else {
        w.meta(ctx, &["pluginVersion", "cacheTimeout", "queryCachingTTL"]);
        w.take("type");
        raw.options = w.take("options");
        raw.field_config = w.take("fieldConfig");
        raw.queries = w.take("targets");
        raw.transformations = w.take("transformations");
        raw.panel_ds = w.take("datasource");
        raw.query_options.interval = w.take("interval");
        raw.query_options.max_data_points = w.take("maxDataPoints");
        raw.query_options.time_from = w.take("timeFrom");
        raw.query_options.time_shift = w.take("timeShift");
        raw.query_options.hide_time_override = w.take("hideTimeOverride");
        // The place is read by the grid code from the same node.
        w.take("gridPos");
    }
    raw.repeat = crate::repeat::read_v1(ctx, &mut w);
    if let Some(n) = w.take("repeatPanelId") {
        if crate::item::is_trivial(n.v) {
            ctx.meta();
        } else {
            raw.copy_of = Some(n);
        }
    }
    w.meta(ctx, &COPY_METADATA);
    w.finish(ctx);
    Some(raw)
}

impl Ctx {
    /// A `dropped` member whose value is `true` is listed. Another value is metadata.
    pub(crate) fn drop_member_value(&mut self, node: &Node, reason: &str) {
        if as_flag(node.v) == Some(true) || (!node.v.is_boolean() && !crate::item::is_trivial(node.v)) {
            self.dropped(&node.ptr, Some(node.v), reason);
        } else {
            self.meta();
        }
    }
}

fn grid_out(ctx: &mut Ctx, grid: &GridRaw, y_base: i64) -> Option<[i64; 4]> {
    let read = |ctx: &mut Ctx, n: &Option<Node>, name: &str, min: i64, max: i64, base: i64| -> Option<i64> {
        let Some(node) = n else {
            ctx.unsupported(
                &grid.at.ptr,
                Some(grid.at.v),
                &format!("the grid has no {name}"),
                Fallback::DashboardNotImported,
            );
            return None;
        };
        match as_int(node.v).and_then(|v| v.checked_sub(base)).filter(|v| (min..=max).contains(v)) {
            Some(v) => Some(v),
            None => {
                let reason = format!("the grid member {name} must be an integer from {min} to {max}");
                ctx.unsupported(&node.ptr, Some(node.v), &reason, Fallback::DashboardNotImported);
                None
            }
        }
    };
    let x = read(ctx, &grid.x, "x", 0, 23, 0)?;
    let y = read(ctx, &grid.y, "y", 0, 10_000, y_base)?;
    let w = read(ctx, &grid.w, "w", 1, 24, 0)?;
    let h = read(ctx, &grid.h, "h", 1, 200, 0)?;
    if x + w > 24 {
        let at = grid.w.as_ref().unwrap_or(&grid.at);
        ctx.unsupported(&at.ptr, Some(at.v), "x + w must be 24 or less", Fallback::DashboardNotImported);
        return None;
    }
    ctx.ok();
    Some([x, y, w, h])
}

/// Maps one panel. Gives `None` when the panel is removed. The collector then holds the items.
pub(crate) fn convert_panel(ctx: &mut Ctx, pc: &PanelCx, src: &PanelSrc) -> Option<PanelOut> {
    let node = &src.node;
    let scope = ctx.begin_panel(&node.ptr);
    let id_peek = node.get("id").and_then(|i| as_int(i.v));
    let title_peek = node.get("title").and_then(|t| t.str().map(str::to_owned)).unwrap_or_default();
    ctx.set_panel(id_peek, &title_peek);
    let result = convert_inner(ctx, pc, src, scope);
    ctx.end_panel();
    result
}

fn convert_inner(ctx: &mut Ctx, pc: &PanelCx, src: &PanelSrc, scope: usize) -> Option<PanelOut> {
    let node = &src.node;
    let form = pc.form;
    let library = if form.is_v2() {
        src.element_kind.as_deref() == Some("LibraryPanel")
    } else {
        node.get("libraryPanel").is_some_and(|l| !crate::item::is_trivial(l.v))
    };
    if library {
        let at = node.get("libraryPanel").unwrap_or_else(|| node.clone());
        ctx.unsupported(&at.ptr, Some(at.v), "library panels are out of scope", Fallback::PanelRemoved);
        return None;
    }
    let ptype = match peek_type(form, node) {
        Some(t) => match Pt::parse(&t) {
            Some(p) => p,
            None => {
                let at = node.get("type").unwrap_or_else(|| node.clone());
                ctx.unsupported(
                    &at.ptr,
                    Some(at.v),
                    &format!("the panel type {t} is not a type of 4.13"),
                    Fallback::PanelRemoved,
                );
                return None;
            }
        },
        None => {
            ctx.unsupported(&node.ptr, Some(node.v), "the panel has no type", Fallback::PanelRemoved);
            return None;
        }
    };
    // The heatmap of Grafana 7 to 9 has panel members that the heatmap of Grafana 10 and later does not have (15.6).
    if ptype == Pt::Heatmap && !form.is_v2() && old_heatmap_member(ctx, node) {
        return None;
    }
    let raw = read_raw(ctx, form, node)?;
    if let Some(copy) = &raw.copy_of {
        let reason = "the panel is a stored copy of a repeated panel: the page makes the copies (4.13a)";
        ctx.dropped(&copy.ptr, Some(copy.v), reason);
        return None;
    }
    let v2_repeat = if form.is_v2() { src.repeat.as_ref().and_then(|n| crate::repeat::read_v2(ctx, n)) } else { None };
    let repeat_raw = v2_repeat.as_ref().or(raw.repeat.as_ref());
    let repeat = if ptype == Pt::Text {
        // A text panel does not repeat (4.33a, rule 6).
        if let Some(r) = repeat_raw {
            crate::repeat::refuse_text_repeat(ctx, r);
        }
        None
    } else {
        repeat_raw.and_then(|r| crate::repeat::panel_repeat(ctx, pc.repeat_vars, r))
    };
    // id
    let id = match raw.id.as_ref().and_then(|i| as_int(i.v)).filter(|i| (1..=2_147_483_647).contains(i)) {
        Some(i) => {
            ctx.ok();
            i
        }
        None => {
            let at = raw.id.clone().unwrap_or_else(|| node.clone());
            ctx.unsupported(
                &at.ptr,
                Some(at.v),
                "the panel id must be an integer from 1 to 2147483647",
                Fallback::DashboardNotImported,
            );
            return None;
        }
    };
    let title = text_member(ctx, raw.title.as_ref(), 256, "title");
    let description = text_member(ctx, raw.description.as_ref(), MAX_DESCRIPTION_CHARS, "description");
    let transparent = transparent_member(ctx, raw.transparent.as_ref());
    ctx.set_panel(Some(id), &title);
    let grid = grid_out(ctx, &src.grid, src.y_base)?;
    if ptype == Pt::Text {
        // A text panel (15.6, 4.33a): the options, the links, and the members that it does not use.
        let options_ptr =
            if form.is_v2() { format!("{}/vizConfig/spec/options", node.ptr) } else { format!("{}/options", node.ptr) };
        let options = crate::text::options_out(ctx, raw.options.clone(), &options_ptr);
        let q = &raw.query_options;
        for member in [
            &raw.queries,
            &raw.transformations,
            &raw.panel_ds,
            &raw.field_config,
            &q.interval,
            &q.max_data_points,
            &q.time_from,
            &q.time_shift,
            &q.hide_time_override,
        ] {
            crate::text::drop_query_member(ctx, member.as_ref());
        }
        let panel_links = match raw.links.as_ref().filter(|l| !l.is_null()) {
            Some(l) => links(ctx, pc.opts, l),
            None => Vec::new(),
        };
        return Some(PanelOut {
            scope,
            id,
            ptype,
            title,
            description,
            transparent,
            grid,
            data: DataOut::None,
            transforms: Vec::new(),
            fields: Out::Null,
            options,
            links: panel_links,
            sqls: Vec::new(),
            pointer: node.ptr.clone(),
            repeat: None,
        });
    }
    let panel_ds = read_ds(ctx, raw.panel_ds.clone());
    let has_query = !is_reuse(ctx, &raw, form);
    let tr = transforms(ctx, form, raw.transformations.clone(), has_query);
    // The queries come before the field configuration: a matcher `byFrameRefID` needs the refs (15.6).
    let qc = QueryCx { form, opts: pc.opts, vars: pc.vars, logs: ptype == Pt::Logs };
    let single_frame = matches!(ptype, Pt::Barchart | Pt::Xychart | Pt::Logs | Pt::Geomap);
    let QueryOut { data, sqls, refs } =
        convert_queries(ctx, &qc, raw.queries.clone(), &panel_ds, &raw.query_options, tr.format, single_frame)?;
    // The field configuration first: it gives the values of the options.
    let fcx = FieldCx { opts: pc.opts, ptype, refs: &refs };
    let (fields, customs) = field_config(&fcx, ctx, raw.field_config.clone());
    let (options, fields) = options_with_fields(ctx, ptype, raw.options.clone(), &customs, fields);
    let panel_links = match raw.links.as_ref().filter(|l| !l.is_null()) {
        Some(l) => links(ctx, pc.opts, l),
        None => Vec::new(),
    };
    Some(PanelOut {
        scope,
        id,
        ptype,
        title,
        description,
        transparent,
        grid,
        data,
        transforms: tr.items,
        fields,
        options,
        links: panel_links,
        sqls,
        pointer: node.ptr.clone(),
        repeat,
    })
}

/// Tells if the first query of the panel reuses a frame, with no item.
fn is_reuse(ctx: &mut Ctx, raw: &Raw, form: Form) -> bool {
    let _ = ctx;
    let Some(first) = raw.queries.as_ref().and_then(|q| q.at(0)) else { return false };
    let spec = if form.is_v2() { first.get("spec") } else { Some(first.clone()) };
    let Some(spec) = spec else { return false };
    let uid = |n: &Node| {
        n.get("datasource").and_then(|d| match d.v {
            Value::String(s) => Some(s.clone()),
            _ => d.get("uid").or_else(|| d.get("name")).and_then(|u| u.str().map(str::to_owned)),
        })
    };
    let mut found = uid(&spec);
    if found.is_none() {
        found = spec.get("query").and_then(|q| uid(&q));
    }
    found.as_deref() == Some("-- Dashboard --")
}

/// The member `transparent` (4.13): `true` or `false` map as they are (Grafana also writes 1 and 0).
/// An absent or `null` member gives the Grafana default `false`. Another value is `unsupported`.
fn transparent_member(ctx: &mut Ctx, node: Option<&Node>) -> bool {
    match node.filter(|n| !n.is_null()) {
        None => false,
        Some(n) => match as_flag(n.v) {
            Some(flag) => {
                ctx.ok();
                flag
            }
            None => {
                ctx.unsupported(
                    &n.ptr,
                    Some(n.v),
                    "the panel member transparent must be true or false",
                    Fallback::other("false"),
                );
                false
            }
        },
    }
}

fn text_member(ctx: &mut Ctx, node: Option<&Node>, max: usize, name: &str) -> String {
    match node.filter(|n| !n.is_null()) {
        None => String::new(),
        Some(n) => match n.str() {
            Some(t) if chars(t) <= max => {
                ctx.ok();
                t.to_owned()
            }
            _ => {
                let reason = format!("the {name} must be a text of at most {max} characters");
                ctx.unsupported(&n.ptr, Some(n.v), &reason, Fallback::DashboardNotImported);
                String::new()
            }
        },
    }
}
