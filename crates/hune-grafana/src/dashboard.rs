//! The conversion of one dashboard: the form (15.4), the dashboard and layout members of 15.6,
//! the assembly of the document and its validation.

use std::collections::{BTreeMap, BTreeSet};

use hune_core::document::{
    TimeText, is_valid_url_prefix, is_valid_zone_name, parse_duration_ms, parse_time_text, validate,
};
use hune_core::error::ValidationError;
use hune_core::ids::Uid;
use hune_core::limits::{
    MAX_DASHBOARD_LINKS, MAX_DESCRIPTION_CHARS, MAX_LINK_TAGS, MAX_TAG_CHARS, MAX_TAGS, MAX_TITLE_CHARS, MAX_URL_BYTES,
    REFRESH_CHOICES,
};
use hune_core::sql::TextValueType;
use hune_core::sql::placeholders::{self, BraceFault, Found};
use hune_core::sql::{SqlUse, VariableDef, VariableKind, WarningKind, save_checks_for};
use serde_json::Value;

use crate::mongoprep;

use crate::annotations::{AnnotationCx, AnnotationOut, PanelRef, convert_annotations};
use crate::item::{Ctx, Fallback, ReportItem, SourceUse, is_trivial};
use crate::options::{FolderResolution, ImportError, ImportOptions};
use crate::out::Out;
use crate::panel::{GridRaw, PanelCx, PanelSrc, convert_panel};
use crate::panelopts::Pt;
use crate::query::DataOut;
use crate::repeat::{
    RepeatRaw, SectionOut, check_repeats, read_v1 as read_repeat_v1, read_v2 as read_repeat_v2, repeat_vars,
    section_repeat,
};
use crate::util::{as_flag, as_int, chars};
use crate::variables::{VarKind, VarOut, apply_text_number_rule, convert_variables, data_source_names};
use crate::walk::{Node, Walk};

/// The form of an input dashboard file (15.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// A v1 dashboard: the object has `schemaVersion` and `panels`.
    V1Raw,
    /// A v1 object `{dashboard, meta}`.
    V1Wrapped,
    /// A v2 resource `dashboard.grafana.app/v2alpha1`.
    V2Alpha1,
    /// A v2 resource `dashboard.grafana.app/v2beta1`.
    V2Beta1,
}

impl Form {
    /// Tells if the form is a v2 resource.
    pub fn is_v2(self) -> bool {
        matches!(self, Form::V2Alpha1 | Form::V2Beta1)
    }

    /// The name of the form in the report: `v1`, `v2alpha1` or `v2beta1`.
    pub fn name(self) -> &'static str {
        match self {
            Form::V1Raw | Form::V1Wrapped => "v1",
            Form::V2Alpha1 => "v2alpha1",
            Form::V2Beta1 => "v2beta1",
        }
    }
}

/// The result of one dashboard (15.11). The importer sets `WouldWrite`, `Blocked` and
/// `NotImported`. The command sets the other values after the store step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardResult {
    /// Dry run: the document is new, or differs from the store.
    WouldWrite,
    /// Dry run with a store: the uid exists with other bytes, and `--replace` is absent.
    WouldRefuse,
    /// The store holds the same bytes in the same folder.
    Unchanged,
    /// The importer created the document.
    Written,
    /// The importer saved a new version over other bytes.
    Replaced,
    /// The importer moved the document.
    Moved,
    /// The importer moved the document, then saved a new version.
    MovedReplaced,
    /// The uid exists with other bytes or in another folder, or the uid is deleted.
    RefusedExists,
    /// An item needs acceptance and has none.
    Blocked,
    /// An accepted item has the fallback "the dashboard is not imported".
    NotImported,
    /// The store refused or failed the write.
    StoreError,
}

impl DashboardResult {
    /// The text of the result in the report.
    pub fn as_str(self) -> &'static str {
        match self {
            DashboardResult::WouldWrite => "would_write",
            DashboardResult::WouldRefuse => "would_refuse",
            DashboardResult::Unchanged => "unchanged",
            DashboardResult::Written => "written",
            DashboardResult::Replaced => "replaced",
            DashboardResult::Moved => "moved",
            DashboardResult::MovedReplaced => "moved_replaced",
            DashboardResult::RefusedExists => "refused_exists",
            DashboardResult::Blocked => "blocked",
            DashboardResult::NotImported => "not_imported",
            DashboardResult::StoreError => "store_error",
        }
    }
}

/// The conversion of one dashboard.
#[derive(Debug, Clone)]
pub struct DashboardImport {
    /// The uid of the dashboard in Grafana.
    pub grafana_uid: String,
    /// The uid in Hune (15.8).
    pub uid: String,
    /// The folder id in Hune. Empty when the folder has no id.
    pub folder: String,
    /// The folder title.
    pub folder_title: String,
    /// The title of the dashboard.
    pub title: String,
    /// The form of the input file.
    pub form: Option<Form>,
    /// The number of panels in the input, rows excluded.
    pub panels: u32,
    /// The number of `mapped` members.
    pub mapped: u32,
    /// The number of metadata members.
    pub metadata: u32,
    /// The canonical bytes of the document that `validate` gave. `None` when the dashboard is
    /// blocked or not imported.
    pub document_bytes: Option<Vec<u8>>,
    /// The etag of the document (4.39).
    pub etag: Option<String>,
    /// The items that are not `mapped`, in the order of their JSON pointer.
    pub items: Vec<ReportItem>,
    /// The result of the dashboard.
    pub result: DashboardResult,
    /// The errors of the validation of the document, when it failed.
    pub validation_errors: Vec<ValidationError>,
    /// True when the dashboard has a valid document once the owner accepts each item: no item
    /// has the fallback "the dashboard is not imported". The redirects use it.
    pub importable: bool,
    /// The data sources that the queries and the variables use, with the dialect that the Grafana
    /// type gives. The command compares them with the kinds of the blocks of the configuration (15.9).
    pub source_uses: Vec<SourceUse>,
}

impl DashboardImport {
    /// Tells if an item needs acceptance and has none.
    pub fn has_unaccepted(&self) -> bool {
        self.items.iter().any(|i| !i.accepted)
    }
}

/// The result of the form detection.
pub(crate) struct Detected<'a> {
    pub form: Form,
    pub top: Node<'a>,
    pub grafana_uid: String,
    pub root: Node<'a>,
    pub folder_uid: Option<String>,
    pub folder_ptr: String,
    pub uid_ptr: String,
}

/// Finds the form of a dashboard file, its uid and its folder uid (15.4).
pub(crate) fn detect<'a>(
    value: &'a Value,
    search_folders: &BTreeMap<String, String>,
) -> Result<Detected<'a>, ImportError> {
    let bad = |m: String| ImportError::new(format!("dashboard file: {m}"));
    let top = Node::root(value, "");
    if value.get("apiVersion").is_some() {
        let api = value.get("apiVersion").and_then(Value::as_str).unwrap_or("");
        let form = match api {
            "dashboard.grafana.app/v2alpha1" => Form::V2Alpha1,
            // Grafana 13 serves the stable `v2`: it has the members of `v2beta1` (15.4).
            "dashboard.grafana.app/v2beta1" | "dashboard.grafana.app/v2" => Form::V2Beta1,
            other => return Err(bad(format!("apiVersion {other:?} is not v2alpha1, v2beta1 or v2"))),
        };
        let uid = value
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .ok_or_else(|| bad("a v2 resource needs a text metadata.name".into()))?;
        let folder = value.pointer("/metadata/annotations/grafana.app~1folder").and_then(Value::as_str);
        return Ok(Detected {
            form,
            grafana_uid: uid.to_owned(),
            top: top.clone(),
            root: top,
            folder_uid: folder.filter(|f| !f.is_empty()).map(str::to_owned),
            folder_ptr: "/metadata/annotations/grafana.app~1folder".to_owned(),
            uid_ptr: "/metadata/name".to_owned(),
        });
    }
    if value.get("dashboard").is_some_and(Value::is_object) && value.get("meta").is_some_and(Value::is_object) {
        let dash = top.get("dashboard").ok_or_else(|| bad("no dashboard".into()))?;
        let uid =
            dash.v.get("uid").and_then(Value::as_str).ok_or_else(|| bad("a v1 dashboard needs a text uid".into()))?;
        let folder = value.pointer("/meta/folderUid").and_then(Value::as_str);
        return Ok(Detected {
            form: Form::V1Wrapped,
            top: top.clone(),
            grafana_uid: uid.to_owned(),
            folder_uid: folder.filter(|f| !f.is_empty()).map(str::to_owned),
            folder_ptr: "/meta/folderUid".to_owned(),
            uid_ptr: "/dashboard/uid".to_owned(),
            root: dash,
        });
    }
    if value.get("schemaVersion").is_some_and(Value::is_number) && value.get("panels").is_some_and(Value::is_array) {
        let uid =
            value.get("uid").and_then(Value::as_str).ok_or_else(|| bad("a v1 dashboard needs a text uid".into()))?;
        let folder = search_folders.get(uid).cloned().filter(|f| !f.is_empty());
        return Ok(Detected {
            form: Form::V1Raw,
            grafana_uid: uid.to_owned(),
            top: top.clone(),
            root: top,
            folder_uid: folder,
            folder_ptr: "/uid".to_owned(),
            uid_ptr: "/uid".to_owned(),
        });
    }
    Err(bad("the members fit no form: v2 resource, v1 wrapped or v1 raw (15.4)".into()))
}

fn default_refresh_intervals() -> Value {
    serde_json::json!(["5s", "10s", "30s", "1m", "5m", "15m", "30m", "1h", "2h", "1d"])
}

fn default_time_options() -> Value {
    serde_json::json!(["5m", "15m", "1h", "6h", "12h", "24h", "2d", "7d", "30d"])
}

/// Tells if the v1 object `timepicker` holds only the Grafana defaults.
fn is_default_timepicker(v: &Value) -> bool {
    match v {
        Value::Object(m) => m.iter().all(|(k, val)| match k.as_str() {
            "refresh_intervals" => is_trivial(val) || *val == default_refresh_intervals(),
            "time_options" => is_trivial(val) || *val == default_time_options(),
            "enable" => *val == Value::Bool(true) || is_trivial(val),
            _ => is_trivial(val),
        }),
        other => is_trivial(other),
    }
}

/// The nodes of the header members. The two forms fill them from different places.
#[derive(Default)]
struct HeaderNodes<'a> {
    title: Option<Node<'a>>,
    description: Option<Node<'a>>,
    tags: Option<Node<'a>>,
    from: Option<Node<'a>>,
    to: Option<Node<'a>>,
    timezone: Option<Node<'a>>,
    refresh: Option<Node<'a>>,
    cursor: Option<Node<'a>>,
    links: Option<Node<'a>>,
    /// The list of the annotations (15.6): `annotations.list` (v1) or `spec.annotations` (v2).
    annotations: Option<Node<'a>>,
    /// The member `__inputs` of a v1 export for another instance (15.9).
    inputs: Option<Node<'a>>,
}

struct Header {
    title: String,
    description: String,
    tags: Vec<String>,
    from: String,
    to: String,
    timezone: String,
    refresh: Option<String>,
    cursor_sync: &'static str,
}

fn not_imported(ctx: &mut Ctx, node: &Node, why: &str) {
    ctx.unsupported(&node.ptr, Some(node.v), why, Fallback::DashboardNotImported);
}

fn header(ctx: &mut Ctx, n: &HeaderNodes) -> Option<Header> {
    let title = match n.title.as_ref().and_then(|t| t.str().map(|s| (t, s))) {
        Some((_, t)) if (1..=256).contains(&chars(t)) => {
            ctx.ok();
            t.to_owned()
        }
        Some((node, _)) => {
            not_imported(ctx, node, "the title must have 1 to 256 characters");
            return None;
        }
        None => {
            let ptr = n.title.as_ref().map_or_else(String::new, |t| t.ptr.clone());
            ctx.unsupported(&ptr, None, "the dashboard has no title", Fallback::DashboardNotImported);
            return None;
        }
    };
    let description = match n.description.as_ref().filter(|d| !d.is_null()) {
        None => String::new(),
        Some(d) => match d.str() {
            Some(t) if chars(t) <= MAX_DESCRIPTION_CHARS => {
                ctx.ok();
                t.to_owned()
            }
            _ => {
                not_imported(ctx, d, "the description must be a text of at most 4096 characters");
                return None;
            }
        },
    };
    let tags = tags(ctx, n.tags.as_ref())?;
    let (from, to) = time_range(ctx, n.from.as_ref(), n.to.as_ref());
    let timezone = timezone(ctx, n.timezone.as_ref());
    let refresh = refresh(ctx, n.refresh.as_ref());
    let cursor_sync = cursor_sync(ctx, n.cursor.as_ref());
    Some(Header { title, description, tags, from, to, timezone, refresh, cursor_sync })
}

fn tags(ctx: &mut Ctx, node: Option<&Node>) -> Option<Vec<String>> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Some(Vec::new()) };
    if node.v.as_array().is_none() {
        not_imported(ctx, node, "tags must be an array");
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for item in node.items() {
        let Some(t) = item.str().filter(|t| (1..=MAX_TAG_CHARS).contains(&chars(t))) else {
            not_imported(ctx, &item, "a tag must be a text of 1 to 64 characters");
            return None;
        };
        if out.iter().any(|o| o == t) {
            ctx.changed(&item.ptr, Some(item.v), "two equal tags are changed to one tag");
        } else {
            ctx.ok();
            out.push(t.to_owned());
        }
    }
    if out.len() > MAX_TAGS {
        not_imported(ctx, node, "a dashboard has at most 32 tags");
        return None;
    }
    Some(out)
}

fn time_range(ctx: &mut Ctx, from: Option<&Node>, to: Option<&Node>) -> (String, String) {
    let one = |ctx: &mut Ctx, n: Option<&Node>, default: &str| -> (String, bool) {
        let Some(n) = n.filter(|n| !n.is_null()) else { return (default.to_owned(), false) };
        match n.str().filter(|t| parse_time_text(t).is_some()) {
            Some(t) => {
                ctx.ok();
                (t.to_owned(), true)
            }
            None => {
                let fb = Fallback::other(default);
                ctx.unsupported(&n.ptr, Some(n.v), "the time text does not match the grammar of 4.4", fb);
                (default.to_owned(), false)
            }
        }
    };
    let (f, _) = one(ctx, from, "now-6h");
    let (t, _) = one(ctx, to, "now");
    if let (Some(TimeText::Absolute(a)), Some(TimeText::Absolute(b))) = (parse_time_text(&f), parse_time_text(&t))
        && a >= b
    {
        let at = from.or(to);
        let ptr = at.map(|n| n.ptr.clone()).unwrap_or_default();
        ctx.unsupported(&ptr, at.map(|n| n.v), "from must be less than to", Fallback::other("now-6h and now"));
        return ("now-6h".to_owned(), "now".to_owned());
    }
    (f, t)
}

/// Maps the time zone (15.6): `browser`, `utc` or a zone name of 4.6. The importer does not check that
/// the zone exists. The page does (9.9).
fn timezone(ctx: &mut Ctx, node: Option<&Node>) -> String {
    let Some(n) = node.filter(|n| !n.is_null()) else { return "browser".to_owned() };
    match n.str() {
        Some("") | Some("browser") => {
            ctx.ok();
            "browser".to_owned()
        }
        Some("utc") | Some("UTC") => {
            ctx.ok();
            "utc".to_owned()
        }
        Some("default") => {
            ctx.changed(&n.ptr, Some(n.v), "the time zone default of Grafana is the time zone of the browser");
            "browser".to_owned()
        }
        Some(name) if is_valid_zone_name(name) => {
            ctx.ok();
            name.to_owned()
        }
        _ => {
            ctx.unsupported(&n.ptr, Some(n.v), "this time zone is not a zone name of 4.6", Fallback::other("browser"));
            "browser".to_owned()
        }
    }
}

fn refresh(ctx: &mut Ctx, node: Option<&Node>) -> Option<String> {
    let n = node.filter(|n| !n.is_null() && n.v != &Value::Bool(false))?;
    let Some(text) = n.str() else {
        ctx.unsupported(&n.ptr, Some(n.v), "refresh must be a text", Fallback::other("null"));
        return None;
    };
    if text.is_empty() {
        ctx.ok();
        return None;
    }
    let Some(ms) = parse_duration_ms(text) else {
        ctx.unsupported(&n.ptr, Some(n.v), "refresh is not a duration", Fallback::other("null"));
        return None;
    };
    if REFRESH_CHOICES.contains(&text) {
        ctx.ok();
        return Some(text.to_owned());
    }
    let choice =
        REFRESH_CHOICES.iter().find(|c| parse_duration_ms(c).is_some_and(|cm| cm >= ms)).copied().unwrap_or("1d");
    ctx.changed(
        &n.ptr,
        Some(n.v),
        &format!("refresh {text} is not a choice of 4.6: the next larger choice is {choice}"),
    );
    Some(choice.to_owned())
}

fn cursor_sync(ctx: &mut Ctx, node: Option<&Node>) -> &'static str {
    let Some(n) = node.filter(|n| !n.is_null()) else { return "off" };
    match (n.str(), as_int(n.v)) {
        (Some("Off"), _) | (_, Some(0)) => {
            ctx.ok();
            "off"
        }
        (Some("Crosshair"), _) | (_, Some(1)) => {
            ctx.ok();
            "crosshair"
        }
        (Some("Tooltip"), _) | (_, Some(2)) => {
            ctx.changed(&n.ptr, Some(n.v), "the shared tooltip becomes the shared crosshair");
            "crosshair"
        }
        _ => {
            ctx.unsupported(&n.ptr, Some(n.v), "this value has no equal in format 1", Fallback::other("off"));
            "off"
        }
    }
}

/// A section before the mapping of its header.
struct SectionSrc<'a> {
    row: bool,
    title: Option<Node<'a>>,
    hide_header: bool,
    collapsed: Option<Node<'a>>,
    row_ptr: String,
    panels: Vec<PanelSrc<'a>>,
    /// The repeat of the row, before the mapping (4.13a).
    repeat: Option<RepeatRaw<'a>>,
    /// The `gridPos.y` of a v1 row: the value and its pointer.
    row_y: Option<(i64, String)>,
}

impl<'a> SectionSrc<'a> {
    fn plain() -> SectionSrc<'a> {
        SectionSrc {
            row: false,
            title: None,
            hide_header: false,
            collapsed: None,
            row_ptr: String::new(),
            panels: Vec::new(),
            repeat: None,
            row_y: None,
        }
    }

    /// The exception to the v1 `y` rule (15.6): when the smallest `y` of the panels of a row is the `y` of the row,
    /// Grafana draws these panels on the first line of the section. Each panel of the row then gets `y` minus the
    /// `y` of the row. The row has one `changed` item.
    fn first_line_rule(&mut self, ctx: &mut Ctx) {
        let Some((row_y, ptr)) = &self.row_y else { return };
        let smallest = self.panels.iter().filter_map(|p| p.grid.y.as_ref().and_then(|y| as_int(y.v))).min();
        if smallest != Some(*row_y) {
            return;
        }
        for p in &mut self.panels {
            p.y_base = *row_y;
        }
        let why = "the first panels of the row have the y of the row: Grafana draws them on the first line of the section, so each panel of the row gets y minus the y of the row";
        ctx.changed(ptr, Some(&Value::from(*row_y)), why);
    }
}

fn grid_v1<'a>(ctx: &mut Ctx, panel: &Node<'a>) -> GridRaw<'a> {
    let at = panel.get("gridPos").unwrap_or_else(|| panel.clone());
    let mut grid = GridRaw { at: at.clone(), x: None, y: None, w: None, h: None };
    if let Some(mut w) = Walk::new(&at) {
        grid.x = w.take("x");
        grid.y = w.take("y");
        grid.w = w.take("w");
        grid.h = w.take("h");
        w.meta(ctx, &["static"]);
        w.finish(ctx);
    }
    grid
}

fn layout_v1<'a>(ctx: &mut Ctx, panels: Option<Node<'a>>) -> Vec<SectionSrc<'a>> {
    let mut sections: Vec<SectionSrc<'a>> = Vec::new();
    let mut current = SectionSrc::plain();
    let mut y_base = 0_i64;
    let Some(panels) = panels.filter(|p| !p.is_null()) else { return vec![current] };
    if panels.v.as_array().is_none() {
        not_imported(ctx, &panels, "panels must be an array");
        return vec![current];
    }
    for node in panels.items() {
        let is_row = node.get("type").and_then(|t| t.str().map(str::to_owned)).as_deref() == Some("row");
        if !is_row {
            let grid = grid_v1(ctx, &node);
            current.panels.push(PanelSrc { node, grid, y_base, element_kind: None, repeat: None });
            continue;
        }
        if current.row || !current.panels.is_empty() {
            sections.push(std::mem::replace(&mut current, SectionSrc::plain()));
        }
        let Some(mut w) = Walk::new(&node) else { continue };
        w.take("type");
        let title = w.take("title");
        let collapsed = w.take("collapsed");
        let inner = w.take("panels");
        let grid_pos = w.take("gridPos");
        w.meta(ctx, &["id", "datasource"]);
        let repeat = read_repeat_v1(ctx, &mut w);
        w.meta(ctx, &crate::repeat::COPY_METADATA);
        w.finish(ctx);
        let row_y = grid_pos.as_ref().and_then(|g| g.get("y")).and_then(|y| as_int(y.v));
        if grid_pos.is_some() {
            ctx.ok();
        }
        let Some(row_y) = row_y else {
            not_imported(ctx, &node, "a row needs gridPos.y");
            current = SectionSrc::plain();
            current.row = true;
            continue;
        };
        // A hostile gridPos.y at the end of the range of integers must not overflow.
        let Some(next_base) = row_y.checked_add(1) else {
            not_imported(ctx, &node, "the gridPos.y of a row is too large");
            current = SectionSrc::plain();
            current.row = true;
            continue;
        };
        y_base = next_base;
        current = SectionSrc {
            row: true,
            title,
            hide_header: false,
            collapsed,
            row_ptr: node.ptr.clone(),
            panels: Vec::new(),
            repeat,
            row_y: Some((row_y, format!("{}/gridPos/y", node.ptr))),
        };
        if let Some(inner) = inner.filter(|i| i.v.as_array().is_some_and(|a| !a.is_empty())) {
            for p in inner.items() {
                let grid = grid_v1(ctx, &p);
                current.panels.push(PanelSrc { node: p, grid, y_base, element_kind: None, repeat: None });
            }
        }
    }
    sections.push(current);
    for s in &mut sections {
        s.first_line_rule(ctx);
    }
    sections
}

/// Reads a v2 `GridLayout` into panel sources.
fn grid_items_v2<'a>(
    ctx: &mut Ctx,
    layout: &Node<'a>,
    elements: &Node<'a>,
    used: &mut BTreeSet<String>,
) -> Vec<PanelSrc<'a>> {
    let mut out = Vec::new();
    let Some(mut lw) = Walk::new(layout) else { return out };
    lw.meta(ctx, &["kind"]);
    let spec = lw.take("spec");
    lw.finish(ctx);
    let Some(mut sw) = spec.as_ref().and_then(Walk::new) else { return out };
    let items = sw.take("items");
    sw.finish(ctx);
    for item in items.map(|i| i.items()).unwrap_or_default() {
        let Some(mut iw) = Walk::new(&item) else { continue };
        iw.meta(ctx, &["kind"]);
        let ispec = iw.take("spec");
        iw.finish(ctx);
        let Some(mut pw) = ispec.as_ref().and_then(Walk::new) else { continue };
        let x = pw.take("x");
        let y = pw.take("y");
        let w = pw.take("width");
        let h = pw.take("height");
        let element = pw.take("element");
        let repeat = pw.take("repeat");
        pw.finish(ctx);
        let Some(element) = element else {
            not_imported(ctx, &item, "a grid item needs an element reference");
            continue;
        };
        let name = element.get("name").and_then(|n| n.str().map(str::to_owned));
        let Some(name) = name else {
            not_imported(ctx, &element, "an element reference needs a name");
            continue;
        };
        if let Some(mut ew) = Walk::new(&element) {
            ew.meta(ctx, &["kind"]);
            ew.take("name");
            ew.finish(ctx);
        }
        let Some(el) = elements.get(&name) else {
            not_imported(ctx, &element, &format!("the element {name} is not in elements"));
            continue;
        };
        used.insert(name);
        let Some(mut ew) = Walk::new(&el) else { continue };
        let kind = ew.take("kind").and_then(|k| k.str().map(str::to_owned));
        let spec = ew.take("spec");
        ew.finish(ctx);
        let Some(spec) = spec else { continue };
        let at = ispec.clone().unwrap_or_else(|| item.clone());
        out.push(PanelSrc {
            node: spec,
            grid: GridRaw { at, x, y, w, h },
            y_base: 0,
            element_kind: Some(kind.unwrap_or_else(|| "Panel".to_owned())),
            repeat,
        });
    }
    out
}

fn layout_v2<'a>(ctx: &mut Ctx, layout: Option<Node<'a>>, elements: Option<Node<'a>>) -> Vec<SectionSrc<'a>> {
    static EMPTY: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let empty = EMPTY.get_or_init(|| Value::Object(serde_json::Map::new()));
    let elements_node = elements.clone().unwrap_or_else(|| Node { v: empty, ptr: String::new() });
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut sections = Vec::new();
    if let Some(layout) = layout.filter(|l| !l.is_null()) {
        let kind = layout.get("kind").and_then(|k| k.str().map(str::to_owned)).unwrap_or_default();
        match kind.as_str() {
            "GridLayout" => {
                let mut s = SectionSrc::plain();
                s.panels = grid_items_v2(ctx, &layout, &elements_node, &mut used);
                sections.push(s);
            }
            "RowsLayout" => {
                if let Some(mut lw) = Walk::new(&layout) {
                    lw.meta(ctx, &["kind"]);
                    let spec = lw.take("spec");
                    lw.finish(ctx);
                    if let Some(mut sw) = spec.as_ref().and_then(Walk::new) {
                        let rows = sw.take("rows");
                        sw.finish(ctx);
                        for row in rows.map(|r| r.items()).unwrap_or_default() {
                            let Some(mut rw) = Walk::new(&row) else { continue };
                            rw.meta(ctx, &["kind"]);
                            let rspec = rw.take("spec");
                            rw.finish(ctx);
                            let Some(mut xw) = rspec.as_ref().and_then(Walk::new) else { continue };
                            let title = xw.take("title");
                            let collapse = xw.take("collapse");
                            let hide = xw.take("hideHeader");
                            let inner = xw.take("layout");
                            let repeat = xw.take("repeat").and_then(|r| read_repeat_v2(ctx, &r));
                            xw.finish(ctx);
                            let mut hide_header = false;
                            if let Some(h) = &hide {
                                match as_flag(h.v) {
                                    Some(true) => hide_header = true,
                                    _ => ctx.meta(),
                                }
                            }
                            let mut s = SectionSrc {
                                row: true,
                                title,
                                hide_header,
                                collapsed: collapse,
                                row_ptr: row.ptr.clone(),
                                panels: Vec::new(),
                                repeat,
                                row_y: None,
                            };
                            if let Some(inner) = inner {
                                let inner_kind = inner.get("kind").and_then(|k| k.str().map(str::to_owned));
                                if inner_kind.as_deref() == Some("GridLayout") {
                                    s.panels = grid_items_v2(ctx, &inner, &elements_node, &mut used);
                                } else {
                                    not_imported(ctx, &inner, "a row holds a GridLayout only");
                                }
                            }
                            sections.push(s);
                        }
                    }
                }
            }
            other => {
                let at = layout.get("kind").unwrap_or_else(|| layout.clone());
                not_imported(ctx, &at, &format!("the layout kind {other:?} has no equal in format 1"));
            }
        }
    }
    if let Some(el) = &elements {
        for key in el.v.as_object().map(|m| m.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() {
            if !used.contains(&key)
                && let Some(node) = el.get(&key)
            {
                ctx.unsupported(
                    &node.ptr,
                    Some(node.v),
                    "the element is not in the layout",
                    Fallback::other("element removed"),
                );
            }
        }
    }
    if sections.is_empty() {
        sections.push(SectionSrc::plain());
    }
    sections
}

/// Maps the header of a section (15.6, layout members).
fn section_header(ctx: &mut Ctx, index: usize, s: &SectionSrc) -> Option<(Option<String>, bool)> {
    if !s.row {
        return Some((None, false));
    }
    let title_text = s.title.as_ref().and_then(Node::str).unwrap_or("").to_owned();
    if chars(&title_text) > 256 {
        let at = s.title.as_ref()?;
        not_imported(ctx, at, "the title must have at most 256 characters");
        return None;
    }
    let title = if s.hide_header {
        if index == 0 {
            ctx.ok();
            None
        } else {
            let ptr = format!("{}/spec/hideHeader", s.row_ptr);
            ctx.unsupported(
                &ptr,
                None,
                "only the first section can have no header (4.12)",
                Fallback::other("the header shows"),
            );
            Some(if title_text.is_empty() { "Row".to_owned() } else { title_text })
        }
    } else if title_text.is_empty() {
        if index == 0 {
            ctx.ok();
            None
        } else {
            let at = s.title.as_ref().map_or_else(|| s.row_ptr.clone(), |t| t.ptr.clone());
            ctx.changed(&at, None, "an empty title becomes the text Row");
            Some("Row".to_owned())
        }
    } else {
        ctx.ok();
        Some(title_text)
    };
    let collapsed = s.collapsed.as_ref().and_then(|c| as_flag(c.v)).unwrap_or(false);
    match (&title, &s.collapsed) {
        (None, Some(at)) if collapsed => {
            ctx.changed(&at.ptr, Some(at.v), "a section with no header is not collapsed (4.12)");
            Some((title, false))
        }
        _ => Some((title, collapsed)),
    }
}

fn failed(grafana_uid: &str, folder: &str, items: Vec<ReportItem>, form: Option<Form>) -> DashboardImport {
    DashboardImport {
        grafana_uid: grafana_uid.to_owned(),
        uid: String::new(),
        folder: folder.to_owned(),
        folder_title: String::new(),
        title: String::new(),
        form,
        panels: 0,
        mapped: 0,
        metadata: 0,
        document_bytes: None,
        etag: None,
        items,
        result: DashboardResult::Blocked,
        validation_errors: Vec::new(),
        importable: false,
        source_uses: Vec::new(),
    }
}

/// Converts one Grafana dashboard: a v1 dashboard, a v1 `{dashboard, meta}` object or a v2 resource.
///
/// The conversion applies the fallback of each `unsupported` item. A dashboard with an item
/// that needs acceptance and has none is `Blocked` and has no document. A dashboard with an
/// accepted item whose fallback is "the dashboard is not imported" is `NotImported` and has no
/// document. Else `document_bytes` holds the canonical bytes that `validate` gave.
pub fn import_dashboard(grafana: &Value, opts: &ImportOptions) -> DashboardImport {
    match detect(grafana, &opts.search_folders) {
        Ok(det) => convert(&det, opts),
        Err(e) => {
            let mut ctx = Ctx::new("?", "");
            ctx.unsupported("", None, &e.message, Fallback::DashboardNotImported);
            let items = ctx.finish(&opts.accepted_set()).items;
            failed("?", "", items, None)
        }
    }
}

#[derive(Default)]
struct Totals {
    panels: u32,
}

fn convert(det: &Detected, opts: &ImportOptions) -> DashboardImport {
    let root_base = if det.form == Form::V1Wrapped { "/dashboard" } else { "" };
    let mut ctx = Ctx::new(&det.grafana_uid, root_base);
    let accepted = opts.accepted_set();
    let hune_uid = opts.hune_uid(&det.grafana_uid);
    let form = det.form;
    let root = &det.root;

    // The uid and the folder.
    if hune_uid != det.grafana_uid {
        let old = Value::String(det.grafana_uid.clone());
        ctx.changed(
            &det.uid_ptr,
            Some(&old),
            &format!("the uid does not fit the rule of 4.3: the new uid is {hune_uid}"),
        );
    } else {
        ctx.ok();
    }
    if opts.link_prefix.is_some()
        && !det.grafana_uid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        ctx.changed(
            &det.uid_ptr,
            None,
            "the uid has a character outside [A-Za-z0-9_-]: the nginx map has no line for it",
        );
    }
    let resolution = opts.folders.resolve(det.folder_uid.as_deref());
    let (folder_id, folder_title, folder_blocked) = match &resolution {
        FolderResolution::Found(f) => {
            ctx.ok();
            (f.id.clone(), f.title.clone(), false)
        }
        FolderResolution::NoId { grafana_uid } => {
            let v = Value::String(grafana_uid.clone());
            ctx.unsupported(
                &det.folder_ptr,
                Some(&v),
                "the folder uid does not fit the rule of 4.3 and the folder map has no entry (15.7)",
                Fallback::DashboardNotImported,
            );
            (String::new(), String::new(), false)
        }
        FolderResolution::HasParent { grafana_uid } => {
            let id = format!(
                "folder:{grafana_uid}:/{}/parentUid",
                opts.folders.folders.iter().find(|f| &f.uid == grafana_uid).map_or(0, |f| f.index)
            );
            (String::new(), String::new(), !accepted.contains(&id) || true)
        }
    };
    let parent_folder_accepted = match &resolution {
        FolderResolution::HasParent { grafana_uid } => {
            let index = opts.folders.folders.iter().find(|f| &f.uid == grafana_uid).map_or(0, |f| f.index);
            Some(accepted.contains(&format!("folder:{grafana_uid}:/{index}/parentUid")))
        }
        _ => None,
    };
    let _ = folder_blocked;

    // The object `{dashboard, meta}`: the object `meta` is metadata.
    if form == Form::V1Wrapped
        && let Some(mut w) = Walk::new(&det.top)
    {
        w.take("dashboard");
        if let Some(meta) = w.take("meta") {
            for _ in meta.v.as_object().into_iter().flatten() {
                ctx.meta();
            }
        }
        w.finish(&mut ctx);
    }

    // Header and sources of the members.
    let mut nodes = HeaderNodes::default();
    let (variables_node, mut sections_src) = match form {
        Form::V1Raw | Form::V1Wrapped => read_v1(&mut ctx, root, &mut nodes),
        _ => read_v2(&mut ctx, root, &mut nodes),
    };
    let hdr = header(&mut ctx, &nodes);
    let dash_links = dashboard_links(&mut ctx, opts, nodes.links.as_ref());

    // Names of the variables, for the rewrite of `[[name]]`.
    let names: BTreeSet<String> = variable_names(form, variables_node.as_ref());
    // The data source variables and the items of `__inputs`: a key of the source map can name them (15.9).
    let mut source_names: BTreeMap<String, Option<String>> = inputs(&mut ctx, opts, nodes.inputs.as_ref());
    source_names.extend(data_source_names(form, variables_node.as_ref()));
    ctx.set_source_names(source_names);
    let mut vars = convert_variables(&mut ctx, form, opts, variables_node, &names);

    // Panels and sections.
    let repeatable = repeat_vars(&vars);
    let pc = PanelCx { form, opts, vars: &names, repeat_vars: &repeatable };
    let mut totals = Totals::default();
    let mut sections_out: Vec<SectionOut> = Vec::new();
    for (i, s) in sections_src.iter_mut().enumerate() {
        let header = section_header(&mut ctx, i, s);
        let header_ok = header.is_some();
        let mut panels = Vec::new();
        for p in &s.panels {
            totals.panels += 1;
            if let Some(out) = convert_panel(&mut ctx, &pc, p) {
                panels.push(out);
            }
        }
        let (title, collapsed) = header.unwrap_or((None, false));
        let repeat = match (header_ok, &s.repeat) {
            (true, Some(raw)) => section_repeat(&mut ctx, &repeatable, raw, title.is_some()),
            _ => None,
        };
        sections_out.push(SectionOut { title, collapsed, panels, repeat });
    }
    let checked_uses = post_checks(&mut ctx, opts, &mut vars, &mut sections_out);

    // The annotations (4.5b) come after the panels: the member `filter` names panels of the document.
    let panel_refs: Vec<PanelRef> = sections_out
        .iter()
        .flat_map(|s| s.panels.iter().map(|p| PanelRef { id: p.id, timeseries: p.ptype == Pt::Timeseries }))
        .collect();
    let defs = variable_defs_closed(&vars);
    let acx = AnnotationCx { form, opts, names: &names, defs: &defs, panels: &panel_refs };
    let mut annotations = convert_annotations(&mut ctx, &acx, nodes.annotations.take());
    annotations.retain(|a| !names_source_variable(&mut ctx, &a.sql, "SQL of the annotation", item_removed()));
    let annotation_queries: Vec<(crate::sqlprep::Sql, SqlUse)> =
        annotations.iter().map(|a| (a.sql.clone(), SqlUse::Annotation)).collect();
    restrict_open_custom(&mut ctx, &mut vars, &annotation_queries);
    // A text variable that the SQL of an annotation uses in code follows the rule of 15.6 and 19.11 too.
    let annotation_uses: BTreeSet<String> = annotations
        .iter()
        .flat_map(|a| a.sql.code_uses.iter().cloned())
        .filter(|n| !checked_uses.contains(n))
        .collect();
    apply_text_number_rule(&mut ctx, &mut vars, &annotation_uses);

    let blocked_by_not_imported = ctx.blocks_dashboard();
    let mut validation_errors = Vec::new();
    let mut document: Option<(Vec<u8>, String)> = None;
    let title = hdr.as_ref().map(|h| h.title.clone()).unwrap_or_default();
    if let (Some(h), false) = (&hdr, blocked_by_not_imported) {
        let mut members: Vec<(&str, Out)> = vec![
            ("format_version", Out::Int(i64::from(hune_core::document::FORMAT_VERSION))),
            ("uid", Out::s(&hune_uid)),
            ("title", Out::s(&h.title)),
            ("description", Out::s(&h.description)),
            ("tags", Out::strings(&h.tags)),
            (
                "time",
                Out::obj([
                    ("from", Out::s(&h.from)),
                    ("to", Out::s(&h.to)),
                    ("timezone", Out::s(&h.timezone)),
                    ("refresh", Out::opt_s(h.refresh.as_deref())),
                ]),
            ),
            ("cursor_sync", Out::s(h.cursor_sync)),
            ("links", Out::Arr(dash_links.clone())),
            ("variables", Out::Arr(vars.iter().map(VarOut::to_out).collect())),
        ];
        if !annotations.is_empty() {
            members.push(("annotations", Out::Arr(annotations.iter().map(AnnotationOut::to_out).collect())));
        }
        members.push(("sections", Out::Arr(sections_out.iter().map(SectionOut::to_out).collect())));
        let out = Out::members(members);
        let text = out.write();
        match Uid::parse(&hune_uid) {
            Err(e) => {
                ctx.unsupported(
                    &det.uid_ptr,
                    None,
                    &format!("the uid is not valid: {e}"),
                    Fallback::DashboardNotImported,
                );
            }
            Ok(uid) => match validate(text.as_bytes(), Some(&uid)) {
                Ok(doc) => document = Some((doc.bytes().to_vec(), doc.etag().to_hex())),
                Err(errors) => {
                    let first: Vec<String> =
                        errors.iter().take(3).map(|e| format!("{} {}: {}", e.pointer, e.code, e.message)).collect();
                    let more =
                        if errors.len() > 3 { format!(" ({} errors in all)", errors.len()) } else { String::new() };
                    ctx.unsupported(
                        "",
                        None,
                        &format!("the document is not valid: {}{more}", first.join("; ")),
                        Fallback::DashboardNotImported,
                    );
                    validation_errors = errors;
                }
            },
        }
    }
    let mut collected = ctx.finish(&accepted);
    let mut places: BTreeMap<i64, String> = BTreeMap::new();
    for (si, section) in sections_out.iter().enumerate() {
        for (pi, p) in section.panels.iter().enumerate() {
            places.entry(p.id).or_insert_with(|| format!("/sections/{si}/panels/{pi}"));
        }
    }
    for item in &mut collected.items {
        item.hune_pointer = match item.panel_id {
            Some(id) => places.get(&id).cloned(),
            None if item.pointer == det.uid_ptr => Some("/uid".to_owned()),
            None => None,
        };
    }
    let not_imported_item = collected.items.iter().any(|i| matches!(i.fallback, Some(Fallback::DashboardNotImported)));
    let all_accepted = collected.items.iter().all(|i| i.accepted);
    let result = match parent_folder_accepted {
        Some(true) => DashboardResult::NotImported,
        Some(false) => DashboardResult::Blocked,
        None if !all_accepted => DashboardResult::Blocked,
        None if not_imported_item || document.is_none() => DashboardResult::NotImported,
        None => DashboardResult::WouldWrite,
    };
    let importable = document.is_some() && !not_imported_item && parent_folder_accepted.is_none();
    let (document_bytes, etag) = match (result, document) {
        (DashboardResult::WouldWrite, Some((b, e))) => (Some(b), Some(e)),
        _ => (None, None),
    };
    DashboardImport {
        grafana_uid: det.grafana_uid.clone(),
        uid: hune_uid,
        folder: folder_id,
        folder_title,
        title,
        form: Some(form),
        panels: totals.panels,
        mapped: collected.mapped,
        metadata: collected.metadata,
        document_bytes,
        etag,
        items: collected.items,
        result,
        validation_errors,
        importable,
        source_uses: collected.source_uses,
    }
}

fn variable_names(form: Form, list: Option<&Node>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for item in list.map(Node::items).unwrap_or_default() {
        let name = if form.is_v2() { item.get("spec").and_then(|s| s.get("name")) } else { item.get("name") };
        if let Some(n) = name.and_then(|n| n.str().map(str::to_owned)) {
            out.insert(n);
        }
    }
    out
}

fn read_v1<'a>(ctx: &mut Ctx, root: &Node<'a>, h: &mut HeaderNodes<'a>) -> (Option<Node<'a>>, Vec<SectionSrc<'a>>) {
    let Some(mut w) = Walk::new(root) else { return (None, vec![SectionSrc::plain()]) };
    w.take("uid");
    h.title = w.take("title");
    h.description = w.take("description");
    h.tags = w.take("tags");
    if let Some(t) = w.take("time")
        && let Some(mut tw) = Walk::new(&t)
    {
        h.from = tw.take("from");
        h.to = tw.take("to");
        tw.finish(ctx);
    }
    h.timezone = w.take("timezone");
    h.refresh = w.take("refresh");
    h.cursor = w.take("graphTooltip");
    if let Some(tp) = w.take("timepicker") {
        if is_default_timepicker(tp.v) {
            ctx.meta();
        } else {
            ctx.dropped(&tp.ptr, Some(tp.v), "Hune has no such setting");
        }
    }
    default_member(ctx, w.take("fiscalYearStartMonth"), is_trivial);
    default_member(ctx, w.take("weekStart"), is_trivial);
    h.links = w.take("links");
    if let Some(a) = w.take("annotations")
        && let Some(mut aw) = Walk::new(&a)
    {
        h.annotations = aw.take("list");
        aw.finish(ctx);
    }
    h.inputs = w.take("__inputs");
    w.meta(
        ctx,
        &[
            "editable",
            "liveNow",
            "id",
            "version",
            "schemaVersion",
            "iteration",
            "gnetId",
            "__requires",
            "__elements",
            "preload",
        ],
    );
    let mut variables = None;
    if let Some(t) = w.take("templating")
        && let Some(mut tw) = Walk::new(&t)
    {
        variables = tw.take("list");
        tw.finish(ctx);
    }
    let panels = w.take("panels");
    w.finish(ctx);
    let sections = layout_v1(ctx, panels);
    (variables, sections)
}

/// Reads `__inputs` (15.6, 15.9). It gives the name and the plugin id of each item with `type` = `datasource`. An
/// item that the source map names leaves the document with one `changed` item. The member is metadata.
fn inputs(ctx: &mut Ctx, opts: &ImportOptions, node: Option<&Node>) -> BTreeMap<String, Option<String>> {
    let mut out = BTreeMap::new();
    let Some(node) = node else { return out };
    ctx.meta();
    for item in node.items() {
        let text = |key: &str| item.get(key).and_then(|n| n.str().map(str::to_owned));
        let Some(name) = text("name").filter(|_| text("type").as_deref() == Some("datasource")) else { continue };
        if let Some(source) = opts.source_map.get(&name) {
            let why = format!(
                "the input leaves the document: each reference gets the source {source} of the source map (15.9)"
            );
            ctx.changed(&item.ptr, Some(item.v), &why);
        }
        out.insert(name, text("pluginId"));
    }
    out
}

/// Tells if a SQL text names a data source variable or an item of `__inputs` (15.9). The importer cannot give it a
/// value, so the text is `unsupported` with the fallback `fallback`. One pass of the placeholder scan of 7.6 reads
/// the text, and each name is a lookup: the work is linear in the text.
fn names_source_variable(ctx: &mut Ctx, sql: &crate::sqlprep::Sql, what: &str, fallback: Fallback) -> bool {
    let text = sql.text.as_str();
    let (mut at, mut found) = (0, None);
    while let Some(next) = placeholders::next(text, at, text.len()) {
        at = next.end();
        // A placeholder with a format outside 7.9 still names its variable, as in `mongoprep::names_variable`.
        let name = match &next {
            Found::Placeholder(p) => p.name,
            Found::BadBrace { start, fault: BraceFault::UnknownFormat(f), .. } if !f.is_empty() => {
                let rest = &text[start + 2..];
                &rest[..rest.bytes().position(|b| !placeholders::is_name_byte(b)).unwrap_or(rest.len())]
            }
            Found::BadBrace { .. } => continue,
        };
        if ctx.source_name(name).is_some() {
            found = Some(name.to_owned());
            break;
        }
    }
    let Some(name) = found else { return false };
    let why = format!("{what} names the data source variable {name}: the importer cannot give it a value (15.9)");
    ctx.unsupported(&sql.pointer, None, &why, fallback);
    true
}

fn default_member(ctx: &mut Ctx, node: Option<Node>, is_default: impl Fn(&Value) -> bool) {
    if let Some(n) = node {
        if is_default(n.v) {
            ctx.meta();
        } else {
            ctx.dropped(&n.ptr, Some(n.v), "Hune has no such setting");
        }
    }
}

/// The fallback of the rows of the dashboard links (15.6).
fn item_removed() -> Fallback {
    Fallback::other("item removed")
}

/// Maps `spec.links` (15.6, 4.5a). An item that Hune cannot hold is removed.
fn dashboard_links(ctx: &mut Ctx, opts: &ImportOptions, node: Option<&Node>) -> Vec<Out> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Vec::new() };
    if node.v.as_array().is_none() {
        if is_trivial(node.v) {
            ctx.meta();
        } else {
            ctx.unsupported(&node.ptr, Some(node.v), "links must be an array", Fallback::MemberRemoved);
        }
        return Vec::new();
    }
    let mut out = Vec::new();
    for item in node.items() {
        if out.len() >= MAX_DASHBOARD_LINKS {
            let why = format!("a dashboard has at most {MAX_DASHBOARD_LINKS} links");
            ctx.unsupported(&item.ptr, Some(item.v), &why, item_removed());
            continue;
        }
        if let Some(link) = dashboard_link(ctx, opts, &item) {
            out.push(link);
        }
    }
    out
}

/// Reads a flag of a link item. An absent flag is `false`. A value that is not a flag removes the item.
fn link_flag(ctx: &mut Ctx, node: Option<&Node>) -> Option<bool> {
    let Some(n) = node.filter(|n| !n.is_null()) else { return Some(false) };
    match as_flag(n.v) {
        Some(b) => Some(b),
        None => {
            ctx.unsupported(&n.ptr, Some(n.v), "the flag must be true or false", item_removed());
            None
        }
    }
}

/// Reads the text of a link title. An empty text gives `empty_title`, with the class `changed`.
fn link_title(
    ctx: &mut Ctx,
    item: &Node,
    node: Option<&Node>,
    replacement: &str,
    replace_empty: bool,
) -> Option<String> {
    let text = match node.filter(|n| !n.is_null()) {
        None => String::new(),
        Some(n) => match n.str() {
            Some(t) => t.to_owned(),
            None => {
                ctx.unsupported(&n.ptr, Some(n.v), "the title must be a text", item_removed());
                return None;
            }
        },
    };
    if chars(&text) > MAX_TITLE_CHARS {
        let at = node.cloned().unwrap_or_else(|| item.clone());
        ctx.unsupported(&at.ptr, Some(at.v), "the title must have at most 256 characters", item_removed());
        return None;
    }
    if text.is_empty() && replace_empty {
        let ptr = node.map_or_else(|| item.child_ptr("title"), |n| n.ptr.clone());
        ctx.changed(&ptr, node.map(|n| n.v), &format!("an empty title is changed to {replacement}"));
        return Some(replacement.to_owned());
    }
    Some(text)
}

/// Maps one link item. Gives `None` when the item is removed.
fn dashboard_link(ctx: &mut Ctx, opts: &ImportOptions, item: &Node) -> Option<Out> {
    let Some(mut w) = Walk::new(item) else {
        ctx.unsupported(&item.ptr, Some(item.v), "a dashboard link must be an object", item_removed());
        return None;
    };
    let kind = w.take("type");
    let title = w.take("title");
    let url = w.take("url");
    let tags = w.take("tags");
    let as_dropdown = w.take("asDropdown");
    let target_blank = w.take("targetBlank");
    let include_vars = w.take("includeVars");
    let keep_time = w.take("keepTime");
    let tooltip = w.take("tooltip");
    let icon = w.take("icon");
    let rest = w.unread();
    let kind_text = kind.as_ref().and_then(|k| k.str());
    if !matches!(kind_text, Some("link" | "dashboards")) {
        let at = kind.clone().unwrap_or_else(|| item.clone());
        ctx.unsupported(&at.ptr, Some(at.v), "the type of a dashboard link is link or dashboards", item_removed());
        return None;
    }
    let is_link = kind_text == Some("link");
    let mut keep = true;
    // The members of the other type and the other members have no place in the item.
    let mut other: Vec<&Node> = rest.iter().collect();
    other.extend(
        if is_link { [tags.as_ref(), as_dropdown.as_ref()] } else { [url.as_ref(), None] }.into_iter().flatten(),
    );
    for n in other {
        if is_trivial(n.v) {
            ctx.meta();
        } else {
            ctx.unsupported(&n.ptr, Some(n.v), "this member has no equal in a link of this type", item_removed());
            keep = false;
        }
    }
    // The tooltip and the icon have no equal: a default value is metadata.
    let default_icon = if is_link { "external link" } else { "dashboard" };
    if let Some(n) = &tooltip {
        if is_trivial(n.v) {
            ctx.meta();
        } else {
            ctx.dropped(&n.ptr, Some(n.v), "Hune has no tooltip for a link");
        }
    }
    if let Some(n) = &icon {
        if is_trivial(n.v) || n.str() == Some(default_icon) {
            ctx.meta();
        } else {
            ctx.dropped(&n.ptr, Some(n.v), "Hune has no icon for a link");
        }
    }
    let blank = link_flag(ctx, target_blank.as_ref());
    let vars = link_flag(ctx, include_vars.as_ref());
    let time = link_flag(ctx, keep_time.as_ref());
    let (blank, vars, time) = (blank?, vars?, time?);
    if is_link {
        let title = link_title(ctx, item, title.as_ref(), "Link", true)?;
        let Some(url) = url.as_ref().filter(|u| !u.is_null()) else {
            ctx.unsupported(&item.ptr, Some(item.v), "a link needs a url", item_removed());
            return None;
        };
        let Some(text) = url.str() else {
            ctx.unsupported(&url.ptr, Some(url.v), "the url must be a text", item_removed());
            return None;
        };
        let text = crate::fields::rewrite_link(ctx, opts, url, text);
        let fault = if text.is_empty() || text.len() > MAX_URL_BYTES {
            Some("the url must have 1 to 2048 bytes")
        } else if !is_valid_url_prefix(&text) {
            Some("the prefix of the url breaks the rule of 4.23")
        } else {
            None
        };
        if let Some(why) = fault {
            ctx.unsupported(&url.ptr, Some(url.v), why, item_removed());
            return None;
        }
        if !keep {
            return None;
        }
        ctx.ok();
        return Some(Out::obj([
            ("kind", Out::s("link")),
            ("title", Out::Str(title)),
            ("url", Out::Str(text)),
            ("target_blank", Out::Bool(blank)),
            ("include_vars", Out::Bool(vars)),
            ("keep_time", Out::Bool(time)),
        ]));
    }
    let dropdown = link_flag(ctx, as_dropdown.as_ref());
    let title = link_title(ctx, item, title.as_ref(), "Dashboards", dropdown == Some(true))?;
    let dropdown = dropdown?;
    let tag_list = link_tags(ctx, tags.as_ref())?;
    if !keep {
        return None;
    }
    ctx.ok();
    Some(Out::obj([
        ("kind", Out::s("dashboards")),
        ("title", Out::Str(title)),
        ("tags", Out::strings(&tag_list)),
        ("as_dropdown", Out::Bool(dropdown)),
        ("target_blank", Out::Bool(blank)),
        ("include_vars", Out::Bool(vars)),
        ("keep_time", Out::Bool(time)),
    ]))
}

/// Reads the tags of a link of the type `dashboards` (4.5a). Two equal tags give one tag.
fn link_tags(ctx: &mut Ctx, node: Option<&Node>) -> Option<Vec<String>> {
    let Some(node) = node.filter(|n| !n.is_null()) else { return Some(Vec::new()) };
    if node.v.as_array().is_none() {
        ctx.unsupported(&node.ptr, Some(node.v), "the tags must be an array", item_removed());
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for tag in node.items() {
        let Some(t) = tag.str().filter(|t| (1..=MAX_TAG_CHARS).contains(&chars(t))) else {
            ctx.unsupported(&tag.ptr, Some(tag.v), "a tag must be a text of 1 to 64 characters", item_removed());
            return None;
        };
        if out.iter().any(|o| o == t) {
            ctx.changed(&tag.ptr, Some(tag.v), "two equal tags are changed to one tag");
        } else {
            out.push(t.to_owned());
        }
    }
    if out.len() > MAX_LINK_TAGS {
        let why = format!("a link has at most {MAX_LINK_TAGS} tags");
        ctx.unsupported(&node.ptr, Some(node.v), &why, item_removed());
        return None;
    }
    Some(out)
}

fn read_v2<'a>(ctx: &mut Ctx, root: &Node<'a>, h: &mut HeaderNodes<'a>) -> (Option<Node<'a>>, Vec<SectionSrc<'a>>) {
    let Some(mut w) = Walk::new(root) else { return (None, vec![SectionSrc::plain()]) };
    w.meta(ctx, &["apiVersion", "kind", "status"]);
    if let Some(m) = w.take("metadata")
        && let Some(mut mw) = Walk::new(&m)
    {
        mw.take("name");
        if let Some(a) = mw.take("annotations")
            && let Some(mut aw) = Walk::new(&a)
        {
            aw.take("grafana.app/folder");
            let n = a.v.as_object().map_or(0, |o| o.len());
            for _ in 0..n.saturating_sub(usize::from(a.v.get("grafana.app/folder").is_some())) {
                ctx.meta();
            }
            aw.finish(ctx);
        }
        let rest: Vec<String> = m.v.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
        for key in rest {
            if key != "name" && key != "annotations" {
                mw.meta(ctx, &[key.as_str()]);
            }
        }
        mw.finish(ctx);
    }
    let spec = w.take("spec");
    w.finish(ctx);
    let Some(mut sw) = spec.as_ref().and_then(Walk::new) else { return (None, vec![SectionSrc::plain()]) };
    h.title = sw.take("title");
    h.description = sw.take("description");
    h.tags = sw.take("tags");
    if let Some(t) = sw.take("timeSettings")
        && let Some(mut tw) = Walk::new(&t)
    {
        h.from = tw.take("from");
        h.to = tw.take("to");
        h.timezone = tw.take("timezone");
        h.refresh = tw.take("autoRefresh");
        if let Some(i) = tw.take("autoRefreshIntervals") {
            default_member(ctx, Some(i), |v| is_trivial(v) || *v == default_refresh_intervals());
        }
        default_member(ctx, tw.take("hideTimepicker"), is_trivial);
        default_member(ctx, tw.take("fiscalYearStartMonth"), is_trivial);
        default_member(ctx, tw.take("weekStart"), is_trivial);
        default_member(ctx, tw.take("nowDelay"), is_trivial);
        tw.finish(ctx);
    }
    h.cursor = sw.take("cursorSync");
    h.links = sw.take("links");
    h.annotations = sw.take("annotations");
    sw.meta(ctx, &["editable", "liveNow", "preload"]);
    let variables = sw.take("variables");
    let elements = sw.take("elements");
    let layout = sw.take("layout");
    sw.finish(ctx);
    let sections = layout_v2(ctx, layout, elements);
    (variables, sections)
}

/// Tells if a warning of 7.18 names a text that Grafana replaces inside a string and that Hune keeps
/// as text: a macro (warning 4) or a format that Hune has not (warning 5). The importer lists it as
/// `changed`, because the query runs with another text than in Grafana.
fn literal_warning(kind: WarningKind) -> bool {
    matches!(kind, WarningKind::MacroInLiteral | WarningKind::UnknownFormatInString)
}

/// The checks of 7.36 on the query of a panel of a `mongodb` source. A fault removes the panel. A macro name
/// in a longer string is a `changed` item, as for SQL. The lists of the block come from the options.
fn check_mongo(ctx: &mut Ctx, opts: &ImportOptions, source: &str, sql: &crate::sqlprep::Sql, defs: &[VariableDef]) {
    let lists = opts.mongo_lists.get(source);
    let checked = mongoprep::check(sql, defs, SqlUse::Panel, lists);
    for reason in checked.faults {
        ctx.unsupported(&sql.pointer, None, &format!("MongoDB query: {reason}"), Fallback::PanelRemoved);
    }
    for reason in checked.changes {
        ctx.changed(&sql.pointer, None, &format!("MongoDB query: {reason}"));
    }
}

/// The definitions of the variables for the checks of 7.18.
fn variable_defs(vars: &[VarOut]) -> Vec<VariableDef> {
    vars.iter()
        .filter_map(|v| {
            Some(match &v.kind {
                VarKind::Query { multi, include_all, all_value, allow_custom_value, .. } => {
                    VariableDef::query(&v.name, *multi, *include_all, all_value.as_deref())
                        .with_open(*allow_custom_value)
                }
                VarKind::Custom { options, multi, include_all, all_value, allow_custom_value, .. } => {
                    let values: Vec<&str> = options.iter().map(|(_, v)| v.as_str()).collect();
                    VariableDef::custom(&v.name, &values, *multi, *include_all, all_value.as_deref())
                        .with_open(*allow_custom_value)
                }
                VarKind::Text { number, .. } => {
                    VariableDef::text(&v.name, if *number { TextValueType::Number } else { TextValueType::Text })
                }
                VarKind::Interval { intervals, auto, auto_count, auto_min, .. } => {
                    let items: Vec<&str> = intervals.iter().map(String::as_str).collect();
                    VariableDef::interval(&v.name, &items, auto.then_some((*auto_count, auto_min.as_str())))
                }
                VarKind::Constant { value } => VariableDef::constant(&v.name, value),
                // An `adhoc` variable has no value of 7.7: only `$__adhocFilters` reads its filters (4.10c).
                VarKind::Adhoc { .. } => return None,
            })
        })
        .collect()
}

/// The definitions of the variables with each `custom` variable closed. The checks of the annotations use them:
/// the check of the places of an open `custom` variable (15.6) runs after the annotations.
fn variable_defs_closed(vars: &[VarOut]) -> Vec<VariableDef> {
    variable_defs(vars)
        .into_iter()
        .map(|d| if matches!(d.kind, VariableKind::Custom { .. }) { d.with_open(false) } else { d })
        .collect()
}

/// Makes closed each `custom` variable that Grafana opens, when a free value would be refused in one of the
/// queries (15.6, ID-K206): a placeholder in a quoted name, a placeholder in code with the format none, `raw` or
/// `csv`, or (MongoDB) an option value that starts with `$`. Each change has a `changed` item. A query uses the
/// variable when a placeholder of 7.6 names it, in each form. The scan is the placeholder scan of `hune-core`.
fn restrict_open_custom(ctx: &mut Ctx, vars: &mut [VarOut], queries: &[(crate::sqlprep::Sql, SqlUse)]) {
    for i in 0..vars.len() {
        let name = vars[i].name.clone();
        let VarKind::Custom { options, allow_custom_value: true, .. } = &vars[i].kind else { continue };
        let dollar_option = options.iter().any(|(_, v)| v.starts_with('$'));
        let mut reason: Option<&str> = None;
        for (sql, usage) in queries {
            if !sql.text.contains(name.as_str()) {
                continue;
            }
            reason = match sql.kind.dialect() {
                None if dollar_option && mongoprep::names_variable(&sql.text, &name) => Some(
                    "an option value starts with $ and a MongoDB query uses the variable: the engine refuses a free value that starts with $ (7.35, PM2)",
                ),
                None => None,
                Some(_) if sql.code_raw_uses.contains(&name) => Some(
                    "the variable stands in code with the format none, raw or csv: a free value there must be a number (7.8)",
                ),
                Some(dialect) => {
                    let open = variable_defs(vars);
                    let closed: Vec<VariableDef> = open
                        .iter()
                        .enumerate()
                        .map(|(k, d)| if k == i { d.clone().with_open(false) } else { d.clone() })
                        .collect();
                    let more = save_checks_for(dialect, &sql.text, *usage, &open).faults.len()
                        > save_checks_for(dialect, &sql.text, *usage, &closed).faults.len();
                    more.then_some("the variable stands in a quoted name: a free value cannot stand there (7.8)")
                }
            };
            if reason.is_some() {
                break;
            }
        }
        let Some(reason) = reason else { continue };
        let Some((open_ptr, current_ptr)) = vars[i].open_pointers.clone() else { continue };
        let reason = format!("{reason}; Hune keeps the variable closed, so the server keeps its option check");
        ctx.changed(&open_ptr, Some(&serde_json::Value::Bool(true)), &reason);
        if let VarKind::Custom { options, default, allow_custom_value, .. } = &mut vars[i].kind {
            *allow_custom_value = false;
            // A typed default is valid only for an open variable (4.11): the closed rule applies again.
            if default.iter().any(|d| d != "$__all" && !options.iter().any(|(_, v)| v == d)) {
                let why = "a default value is not the value of an option";
                ctx.unsupported(&current_ptr, None, why, Fallback::other("an empty array"));
                default.clear();
            }
        }
    }
}

/// The checks that need all panels: reuse (4.15), unique ids (4.3) and the SQL checks of 7.18. It gives the
/// names of the variables that the code of the variables and of the panels uses.
fn post_checks(
    ctx: &mut Ctx,
    opts: &ImportOptions,
    vars: &mut [VarOut],
    sections: &mut [SectionOut],
) -> BTreeSet<String> {
    // An open `custom` variable (15.6) stays open only when a free value is valid in each query that names it.
    let mut queries: Vec<(crate::sqlprep::Sql, SqlUse)> = Vec::new();
    for v in vars.iter() {
        if let VarKind::Query { sql, .. } = &v.kind {
            queries.push((sql.clone(), SqlUse::Variable));
        }
    }
    for section in sections.iter() {
        for p in &section.panels {
            if !ctx.is_removed(p.scope) {
                queries.extend(p.sqls.iter().map(|(_, sql)| (sql.clone(), SqlUse::Panel)));
            }
        }
    }
    restrict_open_custom(ctx, vars, &queries);
    // The SQL checks of 7.18 run on each panel and each query variable.
    let defs = variable_defs(vars);
    for v in vars.iter() {
        if let VarKind::Query { sql, .. } = &v.kind {
            // A query variable is a SQL text: the mapping refuses a MongoDB variable.
            let Some(dialect) = sql.kind.dialect() else { continue };
            let what = format!("the SQL of variable {}", v.name);
            names_source_variable(ctx, sql, &what, Fallback::DashboardNotImported);
            let checked = save_checks_for(dialect, &sql.text, SqlUse::Variable, &defs);
            for fault in checked.faults {
                let reason = format!(
                    "SQL of variable {}: line {}, column {}: {}",
                    v.name,
                    fault.line_index + 1,
                    fault.column,
                    fault.message
                );
                ctx.unsupported(&sql.pointer, None, &reason, Fallback::DashboardNotImported);
            }
            for warning in checked.warnings.iter().filter(|w| literal_warning(w.kind)) {
                let reason = format!(
                    "SQL of variable {}: line {}, column {}: {}",
                    v.name,
                    warning.line_index + 1,
                    warning.column,
                    warning.message
                );
                ctx.changed(&sql.pointer, None, &reason);
            }
        }
    }
    let mut query_ids: BTreeSet<i64> = BTreeSet::new();
    for section in sections.iter() {
        for p in &section.panels {
            for (source, sql) in &p.sqls {
                ctx.enter(p.scope);
                names_source_variable(ctx, sql, "the SQL", Fallback::PanelRemoved);
                let Some(dialect) = sql.kind.dialect() else {
                    check_mongo(ctx, opts, source, sql, &defs);
                    ctx.end_panel();
                    continue;
                };
                let checked = save_checks_for(dialect, &sql.text, SqlUse::Panel, &defs);
                for fault in checked.faults {
                    let reason =
                        format!("SQL: line {}, column {}: {}", fault.line_index + 1, fault.column, fault.message);
                    ctx.unsupported(&sql.pointer, None, &reason, Fallback::PanelRemoved);
                }
                // A query on the source of an `adhoc` variable (I39, decision ID-K329): Grafana adds the filters with a
                // setting; Hune never changes the SQL, so the query needs `AND $__adhocFilters` in its WHERE clause.
                if let Some(v) =
                    vars.iter().find(|v| matches!(&v.kind, VarKind::Adhoc { source: s, .. } if s == source))
                {
                    let reason = format!(
                        "the filters of the adhoc variable {} apply to this query only after you add AND $__adhocFilters to its WHERE clause (7.10a)",
                        v.name
                    );
                    ctx.changed(&sql.pointer, None, &reason);
                }
                for warning in checked.warnings.iter().filter(|w| literal_warning(w.kind)) {
                    let reason =
                        format!("SQL: line {}, column {}: {}", warning.line_index + 1, warning.column, warning.message);
                    ctx.changed(&sql.pointer, None, &reason);
                }
                ctx.end_panel();
            }
        }
    }
    for section in sections.iter() {
        for p in &section.panels {
            if matches!(p.data, DataOut::Query { .. }) && !ctx.is_removed(p.scope) {
                query_ids.insert(p.id);
            }
        }
    }
    // Reuse: the source panel has a query of its own.
    for section in sections.iter() {
        for p in &section.panels {
            if let DataOut::Reuse { panel, pointer, .. } = &p.data
                && !query_ids.contains(panel)
                && !ctx.is_removed(p.scope)
            {
                ctx.enter(p.scope);
                ctx.unsupported(
                    pointer,
                    None,
                    &format!("the source panel {panel} is absent, removed, or reuses a frame itself"),
                    Fallback::PanelRemoved,
                );
                ctx.end_panel();
            }
        }
    }
    // A bar chart and an xy chart draw one frame: they cannot reuse a panel with several queries (4.15, rule 6).
    let multi_ids: BTreeSet<i64> = sections
        .iter()
        .flat_map(|s| s.panels.iter())
        .filter(|p| matches!(&p.data, DataOut::Query { extra, .. } if !extra.is_empty()) && !ctx.is_removed(p.scope))
        .map(|p| p.id)
        .collect();
    for section in sections.iter() {
        for p in &section.panels {
            if let DataOut::Reuse { panel, pointer, .. } = &p.data
                && matches!(
                    p.ptype,
                    crate::panelopts::Pt::Barchart
                        | crate::panelopts::Pt::Xychart
                        | crate::panelopts::Pt::Logs
                        | crate::panelopts::Pt::Geomap
                )
                && multi_ids.contains(panel)
                && !ctx.is_removed(p.scope)
            {
                ctx.enter(p.scope);
                ctx.unsupported(
                    pointer,
                    None,
                    &format!(
                        "the source panel {panel} has several queries, and a bar chart, an xy chart, a logs panel and a map draw one frame (4.15, rule 6)"
                    ),
                    Fallback::PanelRemoved,
                );
                ctx.end_panel();
            }
        }
    }
    // Unique panel ids.
    let mut seen: BTreeSet<i64> = BTreeSet::new();
    for section in sections.iter() {
        for p in &section.panels {
            if ctx.is_removed(p.scope) {
                continue;
            }
            if !seen.insert(p.id) {
                ctx.enter(p.scope);
                let ptr = format!("{}/id", p.pointer);
                ctx.unsupported(&ptr, None, "two panels have the same id", Fallback::DashboardNotImported);
                ctx.end_panel();
            }
        }
    }
    // Text variables used in code.
    let mut uses: BTreeSet<String> = BTreeSet::new();
    for v in vars.iter() {
        if let VarKind::Query { sql, .. } = &v.kind {
            uses.extend(sql.code_uses.iter().cloned());
        }
    }
    for section in sections.iter() {
        for p in &section.panels {
            if !ctx.is_removed(p.scope) {
                for (_, sql) in &p.sqls {
                    uses.extend(sql.code_uses.iter().cloned());
                }
            }
        }
    }
    apply_text_number_rule(ctx, vars, &uses);
    // A removed panel leaves the sections.
    for section in sections.iter_mut() {
        section.panels.retain(|p| !ctx.is_removed(p.scope));
    }
    check_repeats(ctx, sections);
    uses
}
