//! The walk of the member tables of section 4: it reads the JSON tree into the typed model and
//! collects the errors (4.36, step 5). The rules across members are in `cross.rs`.

use super::grammar::{
    is_decimal_number, is_valid_colour, is_valid_query_ref, is_valid_unit, is_valid_url_prefix, is_valid_variable_name,
    is_valid_zone_name,
};
use super::json::Json;
use super::model::*;
use super::pattern::{PatternFault, PatternForm, check_pattern};
use super::read::{
    AnnotationMeta, Ctx, Field, Items, Obj, PanelMeta, PlacementMeta, RepeatMeta, SectionMeta, TextRule, VariableMeta,
    quoted,
};
use super::time_text::{TimeText, parse_duration_ms, parse_time_from, parse_time_shift, parse_time_text};
use crate::error::ErrorCode;
use crate::ids::{SourceId, Uid};
use crate::limits::*;
use crate::sql::{AUTO_VALUE, ItemFault, MAX_AUTO_COUNT, MIN_AUTO_COUNT, auto_min_ms, parse_interval_item};
use std::collections::HashSet;

const ANY: TextRule = TextRule::bytes(0, usize::MAX);
const TITLE: TextRule = TextRule::chars(1, MAX_TITLE_CHARS);
const VALUE: TextRule = TextRule::bytes(1, MAX_VALUE_BYTES);

/// Reads the top-level object (4.5).
pub(crate) fn document(cx: &mut Ctx, mut obj: Obj<'_>, expected_uid: Option<&Uid>) -> Option<Document> {
    obj.opt("format_version");
    let uid = obj.get(cx, "uid").and_then(|f| uid_field(cx, &f, expected_uid));
    let title = obj.text(cx, "title", TITLE);
    let description = obj.text(cx, "description", TextRule::chars(0, MAX_DESCRIPTION_CHARS));
    let tags = obj.get(cx, "tags").and_then(|f| tags(cx, &f));
    let time = obj.get(cx, "time").and_then(|f| time(cx, &f));
    let cursor_sync = obj.en::<CursorSync>(cx, "cursor_sync");
    let links = obj.get(cx, "links").and_then(|f| dashboard_links(cx, &f));
    let variables = obj.get(cx, "variables").and_then(|f| variables(cx, &f));
    // An absent member is an empty list. A member that did not read is `None`.
    let annotations = match obj.opt("annotations") {
        None => Some(Vec::new()),
        Some(f) => annotations(cx, &f),
    };
    let sections = obj.get(cx, "sections").and_then(|f| sections(cx, &f));
    obj.finish(
        cx,
        &[
            "format_version",
            "uid",
            "title",
            "description",
            "tags",
            "time",
            "cursor_sync",
            "links",
            "variables",
            "annotations",
            "sections",
        ],
    );
    Some(Document {
        uid: uid?,
        title: title?,
        description: description?,
        tags: tags?,
        time: time?,
        cursor_sync: cursor_sync?,
        links: links?,
        variables: variables?,
        annotations: annotations?,
        sections: sections?,
    })
}

fn id_text(cx: &mut Ctx, f: &Field<'_>) -> Option<String> {
    f.text(cx, ANY)
}

fn uid_field(cx: &mut Ctx, f: &Field<'_>, expected: Option<&Uid>) -> Option<Uid> {
    let text = id_text(cx, f)?;
    let Ok(uid) = Uid::parse(&text) else {
        f.fail(cx, ErrorCode::BadFormat, "must match ^[a-z0-9][a-z0-9-]{0,39}$");
        return None;
    };
    if let Some(exp) = expected
        && exp != &uid
    {
        f.fail(cx, ErrorCode::UidMismatch, &format!("must equal the location {}", quoted(exp.as_str())));
        return None;
    }
    Some(uid)
}

fn source_field(cx: &mut Ctx, f: &Field<'_>) -> Option<SourceId> {
    let text = id_text(cx, f)?;
    match SourceId::parse(&text) {
        Ok(id) => Some(id),
        Err(_) => {
            f.fail(cx, ErrorCode::BadFormat, "must match ^[a-z0-9][a-z0-9-]{0,39}$");
            None
        }
    }
}

fn tags(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<String>> {
    let items = f.items(cx, 0, MAX_TAGS, ErrorCode::TooLarge)?;
    let parsed: Vec<Option<String>> = items.iter().map(|i| i.text(cx, TextRule::chars(1, MAX_TAG_CHARS))).collect();
    unique(cx, &items, &parsed);
    parsed.into_iter().collect()
}

/// Adds `not_unique` for each parsed item that equals an item before it.
pub(super) fn unique<T: PartialEq>(cx: &mut Ctx, items: &Items<'_>, parsed: &[Option<T>]) {
    for (i, p) in parsed.iter().enumerate() {
        let Some(p) = p else { continue };
        let before = parsed.iter().take(i).flatten().any(|q| q == p);
        if before && let Some(item) = items.get(i) {
            item.fail(cx, ErrorCode::NotUnique, "must be unique");
        }
    }
}

fn time_text_field(cx: &mut Ctx, f: &Field<'_>) -> Option<(String, TimeText)> {
    let text = f.text(cx, ANY)?;
    match parse_time_text(&text) {
        Some(t) => Some((text, t)),
        None => {
            f.fail(cx, ErrorCode::BadFormat, "must be a time text (4.4)");
            None
        }
    }
}

fn time(cx: &mut Ctx, f: &Field<'_>) -> Option<TimeSettings> {
    let mut obj = f.object(cx, "time")?;
    let from = obj.get(cx, "from").and_then(|f| time_text_field(cx, &f));
    let to_field = obj.get(cx, "to");
    let to = to_field.as_ref().and_then(|f| time_text_field(cx, f));
    let timezone = obj.get(cx, "timezone").and_then(|f| timezone(cx, &f));
    let refresh = obj.get(cx, "refresh").and_then(|f| {
        f.nullable(|f| {
            let text = f.text(cx, ANY)?;
            if REFRESH_CHOICES.contains(&text.as_str()) {
                Some(text)
            } else {
                f.fail(cx, ErrorCode::BadFormat, &format!("must be null or one of {}", REFRESH_CHOICES.join(", ")));
                None
            }
        })
    });
    obj.finish(cx, &["from", "to", "timezone", "refresh"]);
    if let (Some((_, TimeText::Absolute(a))), Some((_, TimeText::Absolute(b))), Some(tf)) = (&from, &to, &to_field)
        && a >= b
    {
        tf.fail(cx, ErrorCode::OutOfRange, "must be greater than from when both are absolute");
    }
    Some(TimeSettings { from: from?.0, to: to?.0, timezone: timezone?, refresh: refresh? })
}

fn timezone(cx: &mut Ctx, f: &Field<'_>) -> Option<Timezone> {
    let text = f.text(cx, ANY)?;
    match text.as_str() {
        "browser" => Some(Timezone::Browser),
        "utc" => Some(Timezone::Utc),
        _ if is_valid_zone_name(&text) => Some(Timezone::Named(text)),
        _ => {
            f.fail(cx, ErrorCode::BadFormat, "must be browser, utc or a zone name such as Europe/Paris (4.6)");
            None
        }
    }
}

// ---------------------------------------------------------- dashboard links

fn dashboard_links(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<DashboardLink>> {
    let items = f.items(cx, 0, MAX_DASHBOARD_LINKS, ErrorCode::TooLarge)?;
    collect(items.iter().map(|i| dashboard_link(cx, &i)).collect())
}

fn dashboard_link(cx: &mut Ctx, f: &Field<'_>) -> Option<DashboardLink> {
    let mut obj = f.object(cx, "dashboard link")?;
    let Some(kind) = obj.en::<DashboardLinkKind>(cx, "kind") else {
        obj.skip_all();
        return None;
    };
    match kind {
        DashboardLinkKind::Link => {
            let title = obj.text(cx, "title", TITLE);
            let url = obj.get(cx, "url").and_then(|f| url_template(cx, &f));
            let target_blank = obj.boolean(cx, "target_blank");
            let include_vars = obj.boolean(cx, "include_vars");
            let keep_time = obj.boolean(cx, "keep_time");
            obj.finish(cx, &["kind", "title", "url", "target_blank", "include_vars", "keep_time"]);
            Some(DashboardLink::Link {
                title: title?,
                url: url?,
                target_blank: target_blank?,
                include_vars: include_vars?,
                keep_time: keep_time?,
            })
        }
        DashboardLinkKind::Dashboards => {
            let title_field = obj.get(cx, "title");
            let title = title_field.as_ref().and_then(|f| f.text(cx, TextRule::chars(0, MAX_TITLE_CHARS)));
            let tags = obj.get(cx, "tags").and_then(|f| {
                let items = f.items(cx, 0, MAX_LINK_TAGS, ErrorCode::TooLarge)?;
                let parsed: Vec<Option<String>> =
                    items.iter().map(|i| i.text(cx, TextRule::chars(1, MAX_TAG_CHARS))).collect();
                unique(cx, &items, &parsed);
                collect(parsed)
            });
            let as_dropdown = obj.boolean(cx, "as_dropdown");
            let target_blank = obj.boolean(cx, "target_blank");
            let include_vars = obj.boolean(cx, "include_vars");
            let keep_time = obj.boolean(cx, "keep_time");
            obj.finish(cx, &["kind", "title", "tags", "as_dropdown", "target_blank", "include_vars", "keep_time"]);
            if let (Some(t), Some(true), Some(tf)) = (&title, as_dropdown, &title_field)
                && t.is_empty()
            {
                tf.fail(cx, ErrorCode::OutOfRange, "must be 1 to 256 characters with as_dropdown true");
                return None;
            }
            Some(DashboardLink::Dashboards {
                title: title?,
                tags: tags?,
                as_dropdown: as_dropdown?,
                target_blank: target_blank?,
                include_vars: include_vars?,
                keep_time: keep_time?,
            })
        }
    }
}

// ---------------------------------------------------------------- annotations

/// Reads the member `annotations` (4.5b): 0 to `MAX_ANNOTATIONS` items. An empty array is valid.
fn annotations(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<Annotation>> {
    let items = f.items(cx, 0, MAX_ANNOTATIONS, ErrorCode::TooLarge)?;
    let mut out = Vec::new();
    let mut ok = true;
    for item in &items {
        if cx.stopped() {
            return None;
        }
        match annotation(cx, &item) {
            Some(a) => out.push(a),
            None => ok = false,
        }
    }
    ok.then_some(out)
}

fn annotation(cx: &mut Ctx, f: &Field<'_>) -> Option<Annotation> {
    let mut obj = f.object(cx, "annotation")?;
    let name_field = obj.get(cx, "name");
    let name = name_field.as_ref().and_then(|n| n.text(cx, TITLE));
    let source = obj.get(cx, "source").and_then(|f| source_field(cx, &f));
    let sql = obj.get(cx, "sql").and_then(|f| sql_lines(cx, &f));
    let color = obj.get(cx, "color").and_then(|f| colour(cx, &f));
    let enabled = obj.opt("enabled").map_or(Some(true), |f| f.boolean(cx));
    let mut panel_ids: Vec<(String, u32)> = Vec::new();
    let panels = match obj.opt("panels") {
        None => Some(None),
        Some(f) => annotation_panels(cx, &f, &mut panel_ids).map(Some),
    };
    obj.finish(cx, &["name", "source", "sql", "color", "enabled", "panels"]);
    cx.annotations.push(AnnotationMeta { pointer: f.ptr().into_owned(), name: name.clone(), panels: panel_ids });
    Some(Annotation { name: name?, source: source?, sql: sql?, color: color?, enabled: enabled?, panels: panels? })
}

/// Reads the member `panels` of an annotation: 1 to `MAX_ANNOTATION_PANELS` unique panel ids. `ids` gets each
/// id that read, with its pointer, for the check of the references (4.34).
fn annotation_panels(cx: &mut Ctx, f: &Field<'_>, ids: &mut Vec<(String, u32)>) -> Option<Vec<u32>> {
    let items = f.items(cx, 1, MAX_ANNOTATION_PANELS, ErrorCode::TooLarge)?;
    let parsed: Vec<Option<u32>> =
        items.iter().map(|i| i.int(cx, 1, 2_147_483_647).and_then(|v| u32::try_from(v).ok())).collect();
    unique(cx, &items, &parsed);
    for (item, id) in items.iter().zip(&parsed) {
        if let Some(id) = id {
            ids.push((item.ptr().into_owned(), *id));
        }
    }
    parsed.into_iter().collect()
}

// ---------------------------------------------------------------- variables

fn variables(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<Variable>> {
    let items = f.items(cx, 0, MAX_VARIABLES, ErrorCode::TooLarge)?;
    let mut out = Vec::new();
    let mut ok = true;
    for item in &items {
        if cx.stopped() {
            return None;
        }
        match variable(cx, &item) {
            Some(v) => out.push(v),
            None => ok = false,
        }
    }
    ok.then_some(out)
}

fn variable(cx: &mut Ctx, f: &Field<'_>) -> Option<Variable> {
    let mut obj = f.object(cx, "variable")?;
    let tag = obj.en::<VariableTag>(cx, "kind");
    let name = obj.get(cx, "name").and_then(|n| {
        let text = n.text(cx, ANY)?;
        if is_valid_variable_name(&text) {
            Some(text)
        } else {
            n.fail(cx, ErrorCode::BadFormat, "must match ^[A-Za-z_][A-Za-z0-9_]{0,63}$ and not start with __");
            None
        }
    });
    let label = obj.get(cx, "label").and_then(|l| l.nullable(|l| l.text(cx, TITLE)));
    let hide = obj.en::<Hide>(cx, "hide");
    let mut meta = VariableMeta {
        pointer: f.ptr().into_owned(),
        name: name.clone(),
        sql: None,
        placement: None,
        repeatable: None,
        adhoc_source: None,
    };
    let Some(tag) = tag else {
        obj.skip_all();
        cx.variables.push(meta);
        return None;
    };
    let placement: Option<Option<Placement>>;
    let kind = match tag {
        VariableTag::Query => {
            let source = obj.get(cx, "source").and_then(|f| source_field(cx, &f));
            let sql = obj.get(cx, "sql").and_then(|f| sql_lines(cx, &f));
            let multi = obj.boolean(cx, "multi");
            meta.repeatable = multi;
            let include_all = obj.boolean(cx, "include_all");
            let all_value = obj.get(cx, "all_value").and_then(|f| all_value(cx, &f, include_all));
            let refresh = obj.en::<VariableRefresh>(cx, "refresh");
            let sort = obj.en::<VariableSort>(cx, "sort");
            let regex = obj.get(cx, "regex").and_then(|f| f.nullable(|f| pattern(cx, f, PatternForm::Grafana)));
            let default = obj.get(cx, "default").and_then(|f| defaults(cx, &f, multi, include_all, None));
            let allow_custom_value = optional(&mut obj, "allow_custom_value", |f| f.boolean(cx));
            placement = take_placement(cx, &mut obj, hide, &mut meta);
            obj.finish(
                cx,
                &[
                    "kind",
                    "name",
                    "label",
                    "hide",
                    "source",
                    "sql",
                    "multi",
                    "include_all",
                    "all_value",
                    "refresh",
                    "sort",
                    "regex",
                    "default",
                    "allow_custom_value",
                    "placement",
                ],
            );
            meta.sql = sql.as_ref().map(|l| l.join("\n"));
            (|| {
                Some(VariableKind::Query {
                    source: source?,
                    sql: sql?,
                    multi: multi?,
                    include_all: include_all?,
                    all_value: all_value?,
                    refresh: refresh?,
                    sort: sort?,
                    regex: regex?,
                    default: default?,
                    allow_custom_value: allow_custom_value?.unwrap_or(false),
                })
            })()
        }
        VariableTag::Custom => {
            let options = obj.get(cx, "options").and_then(|f| custom_options(cx, &f));
            let multi = obj.boolean(cx, "multi");
            meta.repeatable = multi;
            let include_all = obj.boolean(cx, "include_all");
            let all_value = obj.get(cx, "all_value").and_then(|f| all_value(cx, &f, include_all));
            // The member is read before `default`: the rule of the default depends on it (4.11).
            let allow_custom_value = optional(&mut obj, "allow_custom_value", |f| f.boolean(cx));
            let option_rule = match allow_custom_value {
                Some(Some(false) | None) => Some(options.as_deref()),
                _ => None,
            };
            let default = obj.get(cx, "default").and_then(|f| defaults(cx, &f, multi, include_all, option_rule));
            placement = take_placement(cx, &mut obj, hide, &mut meta);
            obj.finish(
                cx,
                &[
                    "kind",
                    "name",
                    "label",
                    "hide",
                    "options",
                    "multi",
                    "include_all",
                    "all_value",
                    "default",
                    "allow_custom_value",
                    "placement",
                ],
            );
            (|| {
                Some(VariableKind::Custom {
                    options: options?,
                    multi: multi?,
                    include_all: include_all?,
                    all_value: all_value?,
                    default: default?,
                    allow_custom_value: allow_custom_value?.unwrap_or(false),
                })
            })()
        }
        VariableTag::Text => {
            meta.repeatable = Some(false);
            let value_type = obj.en::<ValueType>(cx, "value_type");
            let default_field = obj.get(cx, "default");
            let default = default_field.as_ref().and_then(|f| f.text(cx, TextRule::bytes(0, MAX_VALUE_BYTES)));
            placement = take_placement(cx, &mut obj, hide, &mut meta);
            obj.finish(cx, &["kind", "name", "label", "hide", "value_type", "default", "placement"]);
            if let (Some(ValueType::Number), Some(d), Some(df)) = (value_type, &default, &default_field)
                && !is_decimal_number(d)
            {
                df.fail(cx, ErrorCode::BadFormat, "must match ^-?[0-9]+(\\.[0-9]+)?$ with value_type number");
                cx.variables.push(meta);
                return None;
            }
            (|| Some(VariableKind::Text { value_type: value_type?, default: default? }))()
        }
        VariableTag::Interval => {
            meta.repeatable = Some(false);
            let intervals = obj.get(cx, "intervals").and_then(|f| interval_items(cx, &f));
            let auto = obj.boolean(cx, "auto");
            let auto_count = obj
                .int(cx, "auto_count", i64::from(MIN_AUTO_COUNT), i64::from(MAX_AUTO_COUNT))
                .and_then(|v| u32::try_from(v).ok());
            let auto_min = obj.get(cx, "auto_min").and_then(|f| auto_min_field(cx, &f));
            let default = obj.get(cx, "default").and_then(|f| interval_default(cx, &f, intervals.as_deref(), auto));
            placement = take_placement(cx, &mut obj, hide, &mut meta);
            obj.finish(
                cx,
                &[
                    "kind",
                    "name",
                    "label",
                    "hide",
                    "intervals",
                    "auto",
                    "auto_count",
                    "auto_min",
                    "default",
                    "placement",
                ],
            );
            (|| {
                Some(VariableKind::Interval {
                    intervals: intervals?,
                    auto: auto?,
                    auto_count: auto_count?,
                    auto_min: auto_min?,
                    default: default?,
                })
            })()
        }
        VariableTag::Constant => {
            meta.repeatable = Some(false);
            let value = obj.get(cx, "value").and_then(|f| f.text(cx, TextRule::bytes(0, MAX_VALUE_BYTES)));
            placement = take_placement(cx, &mut obj, hide, &mut meta);
            obj.finish(cx, &["kind", "name", "label", "hide", "value", "placement"]);
            // The bar never shows a constant, so `hide` MUST be `variable` (4.10b, rule 1).
            if let Some(h) = hide
                && h != Hide::Variable
            {
                let message = format!("hide must be variable for the kind constant; value {}", quoted(h.as_str()));
                cx.push(&format!("{}/hide", f.ptr()), ErrorCode::NotPermitted, message);
                cx.variables.push(meta);
                return None;
            }
            value.map(|value| VariableKind::Constant { value })
        }
        VariableTag::Adhoc => {
            // The filters of a viewer come from the address and the bar: the document holds none (4.10c).
            meta.repeatable = Some(false);
            let source = obj.get(cx, "source").and_then(|f| source_field(cx, &f));
            meta.adhoc_source = source.as_ref().map(|s| s.as_str().to_owned());
            let keys = obj.get(cx, "keys").and_then(|f| adhoc_keys(cx, &f));
            // The control is always in the variables bar (4.10c, rule 4).
            if let Some(p) = obj.opt("placement") {
                p.fail(
                    cx,
                    ErrorCode::NotPermitted,
                    "must be absent for the kind adhoc: its control is in the variables bar",
                );
            }
            placement = Some(None);
            obj.finish(cx, &["kind", "name", "label", "hide", "source", "keys", "placement"]);
            (|| Some(VariableKind::Adhoc { source: source?, keys: keys? }))()
        }
    };
    cx.variables.push(meta);
    Some(Variable { name: name?, label: label?, hide: hide?, kind: kind?, placement: placement? })
}

/// Reads the optional member `placement` of a variable (4.7a). It gives `Some(None)` when the member is
/// absent, and `None` when it is not valid. The check across members needs the place, so `meta` keeps it.
fn take_placement(
    cx: &mut Ctx,
    obj: &mut Obj<'_>,
    hide: Option<Hide>,
    meta: &mut VariableMeta,
) -> Option<Option<Placement>> {
    let Some(f) = obj.opt("placement") else { return Some(None) };
    let placement = placement(cx, &f)?;
    if hide == Some(Hide::Variable) {
        f.fail(cx, ErrorCode::NotPermitted, "must be absent when hide is variable");
        return None;
    }
    meta.placement = Some(PlacementMeta { section: placement.section, grid: placement.grid });
    Some(Some(placement))
}

fn placement(cx: &mut Ctx, f: &Field<'_>) -> Option<Placement> {
    let mut obj = f.object(cx, "placement")?;
    let section = obj.int(cx, "section", 0, 2_147_483_647).and_then(|v| u32::try_from(v).ok());
    let grid = grid_members(cx, &mut obj);
    let title = optional(&mut obj, "title", |f| f.text(cx, TITLE));
    obj.finish(cx, &["section", "x", "y", "w", "h", "title"]);
    Some(Placement { section: section?, grid: grid?, title: title? })
}

fn all_value(cx: &mut Ctx, f: &Field<'_>, include_all: Option<bool>) -> Option<Option<String>> {
    let v = f.nullable(|f| f.text(cx, VALUE))?;
    if include_all == Some(false) && v.is_some() {
        f.fail(cx, ErrorCode::NotPermitted, "must be null when include_all is false");
        return None;
    }
    Some(v)
}

fn custom_options(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<CustomOption>> {
    let items = f.items(cx, 1, MAX_VARIABLE_VALUES, ErrorCode::TooLarge)?;
    let mut out = Vec::new();
    let mut values: Vec<Option<String>> = Vec::new();
    let mut value_fields: Vec<Option<Field<'_>>> = Vec::new();
    let mut ok = true;
    for item in &items {
        let Some(mut obj) = item.object(cx, "option") else {
            ok = false;
            values.push(None);
            value_fields.push(None);
            continue;
        };
        let text = obj.text(cx, "text", TITLE);
        let value_field = obj.get(cx, "value");
        let value = value_field.as_ref().and_then(|f| f.text(cx, VALUE));
        obj.finish(cx, &["text", "value"]);
        values.push(value.clone());
        value_fields.push(value_field);
        match (text, value) {
            (Some(text), Some(value)) => out.push(CustomOption { text, value }),
            _ => ok = false,
        }
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for (i, v) in values.iter().enumerate() {
        let Some(v) = v else { continue };
        if !seen.insert(v.as_str())
            && let Some(Some(vf)) = value_fields.get(i)
        {
            ok = false;
            vf.fail(cx, ErrorCode::NotUnique, "must be unique among the options");
        }
    }
    ok.then_some(out)
}

/// Reads the member `keys` of an `adhoc` variable (4.10c): 1 to `MAX_ADHOC_KEYS` unique plain column names. A name
/// that breaks the grammar is `bad_format`. The checks at save compare the keys with the configured list (7.18).
fn adhoc_keys(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<String>> {
    let items = f.items(cx, 1, MAX_ADHOC_KEYS, ErrorCode::TooLarge)?;
    let mut out: Vec<Option<String>> = Vec::with_capacity(items.len());
    for item in &items {
        out.push(item.text(cx, ANY).and_then(|text| {
            if crate::sql::adhoc::is_column_name(&text) {
                Some(text)
            } else {
                item.fail(cx, ErrorCode::BadFormat, "must match ^[A-Za-z_][A-Za-z0-9_]{0,63}$");
                None
            }
        }));
    }
    let mut ok = out.iter().all(Option::is_some);
    let mut seen: HashSet<&str> = HashSet::new();
    for (item, text) in items.iter().zip(&out) {
        if let Some(text) = text
            && !seen.insert(text.as_str())
        {
            item.fail(cx, ErrorCode::NotUnique, "must be unique among the keys");
            ok = false;
        }
    }
    if ok { out.into_iter().collect() } else { None }
}

/// Reads the member `intervals` of an `interval` variable (4.10a): 1 to `MAX_INTERVALS` unique items. An item
/// that breaks the grammar is `bad_format`. An item of more than 365 days is `out_of_range`.
fn interval_items(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<String>> {
    let items = f.items(cx, 1, MAX_INTERVALS, ErrorCode::TooLarge)?;
    let mut out: Vec<Option<String>> = Vec::with_capacity(items.len());
    for item in &items {
        out.push(item.text(cx, ANY).and_then(|text| match parse_interval_item(&text) {
            Ok(_) => Some(text),
            Err(ItemFault::Format) => {
                item.fail(cx, ErrorCode::BadFormat, "must match ^[1-9][0-9]{0,8}(ms|s|m|h|d|w|y)$");
                None
            }
            Err(ItemFault::Range) => {
                item.fail(cx, ErrorCode::OutOfRange, "must be 365 days or less");
                None
            }
        }));
    }
    let mut ok = out.iter().all(Option::is_some);
    let mut seen: HashSet<&str> = HashSet::new();
    for (item, text) in items.iter().zip(&out) {
        if let Some(text) = text
            && !seen.insert(text.as_str())
        {
            item.fail(cx, ErrorCode::NotUnique, "must be unique among the items");
            ok = false;
        }
    }
    if ok { out.into_iter().collect() } else { None }
}

/// Reads `auto_min` (4.10a): one of the 30 texts of the table of 7.4a.
fn auto_min_field(cx: &mut Ctx, f: &Field<'_>) -> Option<String> {
    let text = f.text(cx, ANY)?;
    if auto_min_ms(&text).is_some() {
        Some(text)
    } else {
        f.fail(cx, ErrorCode::BadFormat, "must be one of the 30 texts of the table of 7.4a (1ms to 1y)");
        None
    }
}

/// Reads the member `default` of an `interval` variable (4.10a): `null`, an item, or `$__auto` with `auto` true.
/// `intervals` is `None` when the items did not read: then the rule has no data, and it gives no false error.
fn interval_default(
    cx: &mut Ctx,
    f: &Field<'_>,
    intervals: Option<&[String]>,
    auto: Option<bool>,
) -> Option<Option<String>> {
    let value = f.nullable(|f| f.text(cx, VALUE))?;
    let (Some(text), Some(items), Some(auto)) = (&value, intervals, auto) else { return Some(value) };
    let is_item = items.iter().any(|i| i == text);
    if is_item || (auto && text == AUTO_VALUE) {
        Some(value)
    } else {
        f.fail(cx, ErrorCode::NotPermitted, "must be an item of intervals, or $__auto when auto is true");
        None
    }
}

/// Reads `default` (4.11). `options` is `Some` for a `custom` variable: the inner value is
/// `None` when the options did not pass.
fn defaults(
    cx: &mut Ctx,
    f: &Field<'_>,
    multi: Option<bool>,
    include_all: Option<bool>,
    options: Option<Option<&[CustomOption]>>,
) -> Option<Vec<String>> {
    let items = f.items(cx, 0, MAX_VARIABLE_VALUES, ErrorCode::TooLarge)?;
    let parsed: Vec<Option<String>> = items.iter().map(|i| i.text(cx, VALUE)).collect();
    let mut ok = parsed.iter().all(Option::is_some);
    let option_values: Option<HashSet<&str>> =
        options.flatten().map(|opts| opts.iter().map(|o| o.value.as_str()).collect());
    if multi == Some(false)
        && let Some(second) = items.get(1)
    {
        second.fail(cx, ErrorCode::NotPermitted, "must not exist: with multi false, default has 0 or 1 item");
        ok = false;
    }
    for (i, p) in parsed.iter().enumerate() {
        let (Some(p), Some(item)) = (p, items.get(i)) else { continue };
        if p == "$__all" {
            if include_all == Some(false) {
                item.fail(cx, ErrorCode::NotPermitted, "$__all needs include_all true");
                ok = false;
            } else if items.len() > 1 && multi != Some(false) {
                item.fail(cx, ErrorCode::NotPermitted, "$__all must be the only item");
                ok = false;
            }
        } else if option_values.as_ref().is_some_and(|set| !set.contains(p.as_str())) {
            item.fail(cx, ErrorCode::NotPermitted, "must be the value of one option");
            ok = false;
        }
    }
    if ok { parsed.into_iter().collect() } else { None }
}

// ------------------------------------------------------------------- SQL

/// Reads the lines of an SQL text (4.16). The joined size comes first, from the lengths of the texts: a text
/// above `MAX_SQL_BYTES` gives `too_large` before any item is read or copied. An item that is not a text counts
/// 0 bytes in this sum. Then each line is checked with no copy, and the lines are copied only when all pass.
fn sql_lines(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<String>> {
    if let Json::Arr(a) = f.json
        && !a.is_empty()
    {
        let total =
            a.iter().map(|j| if let Json::Str(s) = j { s.len() } else { 0 }).fold(a.len() - 1, usize::saturating_add);
        if total > MAX_SQL_BYTES {
            let name = f.name();
            let message = format!("{name} must have at most {MAX_SQL_BYTES} bytes when joined; value {total} bytes");
            cx.push(&f.ptr(), ErrorCode::TooLarge, message);
            return None;
        }
    }
    let items = f.items(cx, 1, usize::MAX, ErrorCode::TooLarge)?;
    let rule = TextRule { min: 0, max: usize::MAX, bytes: true, allow_tab: true };
    let mut ok = true;
    for item in &items {
        let Json::Str(s) = item.json else {
            item.fail(cx, ErrorCode::WrongType, "must be text");
            ok = false;
            continue;
        };
        if s.contains(['\n', '\r']) {
            item.fail(cx, ErrorCode::BadFormat, "must not contain U+000A or U+000D");
            ok = false;
            continue;
        }
        ok &= item.check_text(cx, rule).is_some();
    }
    if !ok {
        return None;
    }
    let mut lines = Vec::with_capacity(items.len());
    for item in &items {
        if let Json::Str(s) = item.json {
            lines.push(s.clone());
        }
    }
    Some(lines)
}

// -------------------------------------------------------------- patterns

pub(super) fn pattern(cx: &mut Ctx, f: &Field<'_>, form: PatternForm) -> Option<String> {
    let text = f.text(cx, TextRule::bytes(1, MAX_PATTERN_BYTES))?;
    match check_pattern(&text, form) {
        Ok(()) => Some(text),
        Err(PatternFault::Length) => {
            f.fail(cx, ErrorCode::OutOfRange, "must be 1 to 1024 bytes");
            None
        }
        Err(PatternFault::Syntax(reason)) => {
            f.fail(
                cx,
                ErrorCode::BadFormat,
                &format!("must be in the common regular expression subset (4.24): {reason}"),
            );
            None
        }
    }
}

fn colour(cx: &mut Ctx, f: &Field<'_>) -> Option<String> {
    let text = f.text(cx, ANY)?;
    if is_valid_colour(&text) {
        Some(text)
    } else {
        f.fail(cx, ErrorCode::BadFormat, "must be a colour (4.19)");
        None
    }
}

// -------------------------------------------------------------- sections

fn sections(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<Section>> {
    let items = f.items(cx, 1, MAX_SECTIONS, ErrorCode::TooLarge)?;
    cx.section_count = Some(items.len());
    let mut out = Vec::new();
    let mut ok = true;
    for (i, item) in items.iter().enumerate() {
        if cx.stopped() {
            return None;
        }
        match section(cx, &item, i) {
            Some(s) => out.push(s),
            None => ok = false,
        }
    }
    ok.then_some(out)
}

fn section(cx: &mut Ctx, f: &Field<'_>, index: usize) -> Option<Section> {
    let mut obj = f.object(cx, "section")?;
    let title_field = obj.get(cx, "title");
    let title = title_field.as_ref().and_then(|t| t.nullable(|t| t.text(cx, TITLE)));
    let collapsed_field = obj.get(cx, "collapsed");
    let collapsed = collapsed_field.as_ref().and_then(|c| c.boolean(cx));
    let mut panels = Vec::new();
    let mut panels_ok = false;
    if let Some(pf) = obj.get(cx, "panels")
        && let Some(items) = pf.items(cx, 0, MAX_PANELS, ErrorCode::TooLarge)
    {
        cx.panel_total += items.len();
        panels_ok = true;
        for item in &items {
            if cx.stopped() {
                break;
            }
            match panel(cx, &item, index) {
                Some(p) => panels.push(p),
                None => panels_ok = false,
            }
        }
    }
    let mut repeat_meta = None;
    let repeat = optional(&mut obj, "repeat", |f| {
        let r = section_repeat(cx, f)?;
        repeat_meta = Some(RepeatMeta { pointer: f.ptr().into_owned(), variable: r.variable.clone(), direction: None });
        Some(r)
    });
    obj.finish(cx, &["title", "collapsed", "panels", "repeat"]);
    cx.sections.push(SectionMeta { index, title_null: title.as_ref().map(Option::is_none), repeat: repeat_meta });
    let mut ok = true;
    if let (Some(None), Some(tf)) = (&title, &title_field) {
        if index > 0 {
            tf.fail(cx, ErrorCode::NotPermitted, "can be null in the first section only");
            ok = false;
        }
        if collapsed == Some(true)
            && let Some(cf) = &collapsed_field
        {
            cf.fail(cx, ErrorCode::NotPermitted, "must be false when title is null");
            ok = false;
        }
    }
    if !ok || !panels_ok {
        return None;
    }
    Some(Section { title: title?, collapsed: collapsed?, panels, repeat: repeat? })
}

/// Reads the member `repeat` of a section (4.13a): the name of a variable. The rules across members check
/// that the variable exists and can repeat (4.34).
fn section_repeat(cx: &mut Ctx, f: &Field<'_>) -> Option<SectionRepeat> {
    let mut obj = f.object(cx, "repeat")?;
    let variable = obj.get(cx, "variable").and_then(|f| f.text(cx, ANY));
    obj.finish(cx, &["variable"]);
    Some(SectionRepeat { variable: variable? })
}

/// The largest value of `max_per_row` (4.13a). It is the width of the grid in columns.
const MAX_PER_ROW: i64 = 24;

/// Reads the member `repeat` of a panel (4.13a). `max_per_row` is valid with the direction `h` only.
fn panel_repeat(cx: &mut Ctx, f: &Field<'_>) -> Option<PanelRepeat> {
    let mut obj = f.object(cx, "repeat")?;
    let variable = obj.get(cx, "variable").and_then(|f| f.text(cx, ANY));
    let direction = obj.en::<RepeatDirection>(cx, "direction");
    let per_row_field = obj.opt("max_per_row");
    let max_per_row = match &per_row_field {
        None => Some(None),
        Some(pf) => pf.int(cx, 1, MAX_PER_ROW).and_then(|v| u32::try_from(v).ok()).map(Some),
    };
    obj.finish(cx, &["variable", "direction", "max_per_row"]);
    if let (Some(RepeatDirection::V), Some(Some(_)), Some(pf)) = (direction, max_per_row, &per_row_field) {
        pf.fail(cx, ErrorCode::NotPermitted, "must be absent when direction is v");
        return None;
    }
    Some(PanelRepeat { variable: variable?, direction: direction?, max_per_row: max_per_row? })
}

// ---------------------------------------------------------------- panels

fn panel(cx: &mut Ctx, f: &Field<'_>, section: usize) -> Option<Panel> {
    let mut obj = f.object(cx, "panel")?;
    let id = obj.int(cx, "id", 1, 2_147_483_647).and_then(|v| u32::try_from(v).ok());
    let panel_type = obj.en::<PanelType>(cx, "type");
    let title = obj.text(cx, "title", TextRule::chars(0, MAX_TITLE_CHARS));
    let description = obj.text(cx, "description", TextRule::chars(0, MAX_DESCRIPTION_CHARS));
    let transparent = obj.boolean(cx, "transparent");
    let grid = obj.get(cx, "grid").and_then(|f| grid(cx, &f));
    let is_text = panel_type == Some(PanelType::Text);
    let mut facts = DataFacts::default();
    // A text panel has no query, no transforms and no field options: the three members are refused (4.33a).
    let (data, transforms, fields) = if is_text {
        for name in ["data", "transforms", "fields"] {
            refuse_in_text_panel(cx, &mut obj, name);
        }
        (Some(PanelData::None), Some(Vec::new()), Some(Fields::default()))
    } else {
        let data = obj.get(cx, "data").and_then(|f| data(cx, &f, panel_type, &mut facts));
        let transforms = obj.get(cx, "transforms").and_then(|f| {
            let items = f.items(cx, 0, MAX_TRANSFORMS, ErrorCode::TooLarge)?;
            collect(items.iter().map(|i| transform(cx, &i)).collect())
        });
        let fields = obj.get(cx, "fields").and_then(|f| fields(cx, &f, panel_type));
        (data, transforms, fields)
    };
    let options = obj.get(cx, "options").and_then(|f| options(cx, &f, panel_type?));
    let links = obj.get(cx, "links").and_then(|f| links(cx, &f, 0));
    let mut repeat_meta = None;
    let repeat = if is_text {
        refuse_in_text_panel(cx, &mut obj, "repeat");
        Some(None)
    } else {
        optional(&mut obj, "repeat", |f| {
            let r = panel_repeat(cx, f)?;
            repeat_meta = Some(RepeatMeta {
                pointer: f.ptr().into_owned(),
                variable: r.variable.clone(),
                direction: Some(r.direction),
            });
            Some(r)
        })
    };
    obj.finish(
        cx,
        &[
            "id",
            "type",
            "title",
            "description",
            "transparent",
            "grid",
            "data",
            "transforms",
            "fields",
            "options",
            "links",
            "repeat",
        ],
    );
    cx.query_total += facts.queries;
    cx.panels.push(PanelMeta {
        pointer: f.ptr().into_owned(),
        section,
        panel_type,
        extra_pointer: facts.extra_pointer,
        id,
        grid,
        is_query: data.as_ref().and_then(|d| match d {
            PanelData::Query(_) => Some(true),
            PanelData::Reuse { .. } => Some(false),
            PanelData::None => None,
        }),
        is_text,
        reuse_target: match &data {
            Some(PanelData::Reuse { panel, .. }) => Some(*panel),
            _ => None,
        },
        repeat: repeat_meta,
    });
    Some(Panel {
        id: id?,
        panel_type: panel_type?,
        title: title?,
        description: description?,
        transparent: transparent?,
        grid: grid?,
        data: data?,
        transforms: transforms?,
        fields: fields?,
        options: options?,
        links: links?,
        repeat: repeat?,
    })
}

/// Adds `not_permitted` when a text panel has the member `name` (4.33a, 4.34). The page never reads the value.
fn refuse_in_text_panel(cx: &mut Ctx, obj: &mut Obj<'_>, name: &str) {
    if let Some(f) = obj.opt(name) {
        f.fail(cx, ErrorCode::NotPermitted, "is not permitted in a text panel: a text panel has no query (4.33a)");
    }
}

/// Gives the vector when every item parsed.
fn collect<T>(items: Vec<Option<T>>) -> Option<Vec<T>> {
    items.into_iter().collect()
}

fn grid(cx: &mut Ctx, f: &Field<'_>) -> Option<Grid> {
    let mut obj = f.object(cx, "grid")?;
    let grid = grid_members(cx, &mut obj);
    obj.finish(cx, &["x", "y", "w", "h"]);
    grid
}

/// Reads the members `x`, `y`, `w` and `h` of 4.14 from an object. The caller calls `finish`.
fn grid_members(cx: &mut Ctx, obj: &mut Obj<'_>) -> Option<Grid> {
    let int = |cx: &mut Ctx, obj: &mut Obj<'_>, name: &str, min: i64, max: i64| {
        obj.int(cx, name, min, max).and_then(|v| u32::try_from(v).ok())
    };
    let x = int(cx, obj, "x", 0, 23);
    let y = int(cx, obj, "y", 0, 10_000);
    let w_field = obj.get(cx, "w");
    let w = w_field.as_ref().and_then(|f| f.int(cx, 1, 24)).and_then(|v| u32::try_from(v).ok());
    let h = int(cx, obj, "h", 1, 200);
    if let (Some(x), Some(w), Some(wf)) = (x, w, &w_field)
        && x + w > 24
    {
        cx.push(&wf.ptr(), ErrorCode::OutOfRange, format!("x + w must be 24 or less; value {}", x + w));
        return None;
    }
    Some(Grid { x: x?, y: y?, w: w?, h: h? })
}

/// What the walk of `data` tells the rules across members (4.15a, 4.34).
#[derive(Default)]
struct DataFacts {
    /// The pointer of the member `extra_queries`, when the panel has it.
    extra_pointer: Option<String>,
    /// The number of queries of the panel: `data.query` and the items of `extra_queries` that the array holds.
    queries: usize,
}

fn data(cx: &mut Ctx, f: &Field<'_>, panel_type: Option<PanelType>, facts: &mut DataFacts) -> Option<PanelData> {
    let mut obj = f.object(cx, "data")?;
    let query = obj.opt("query");
    let reuse = obj.opt("reuse");
    let extra = obj.opt("extra_queries");
    facts.extra_pointer = extra.as_ref().map(|e| e.ptr().into_owned());
    let result = match (query, reuse) {
        (None, None) => {
            cx.push(&f.ptr(), ErrorCode::MissingMember, "member query or reuse is necessary in data".to_owned());
            None
        }
        (Some(_), Some(r)) => {
            r.fail(cx, ErrorCode::NotPermitted, "must not be with query: data has one member");
            None
        }
        (Some(q), None) => {
            let listed = extra.as_ref().map_or(0, |e| match e.json {
                Json::Arr(a) => a.len(),
                _ => 0,
            });
            facts.queries = 1 + listed;
            let read = query_with_extras(cx, &q, extra.as_ref(), panel_type);
            if let Some(d) = &read {
                super::logs::check_logs_format(cx, &format!("{}/format", q.ptr()), d.format, panel_type);
            }
            read.map(PanelData::Query)
        }
        (None, Some(r)) => {
            let read = reuse_data(cx, &r);
            if let Some(e) = &extra {
                e.fail(cx, ErrorCode::NotPermitted, "must not be with reuse: a reuse has no query of its own (4.15a)");
                None
            } else {
                read
            }
        }
    };
    obj.finish(cx, &["query", "reuse", "extra_queries"]);
    result
}

/// Reads `data.query` and the member `extra_queries` (4.15a). The refs are unique in the panel.
fn query_with_extras(
    cx: &mut Ctx,
    q: &Field<'_>,
    extra: Option<&Field<'_>>,
    panel_type: Option<PanelType>,
) -> Option<QueryData> {
    let mut first_ref = DEFAULT_QUERY_REF.to_owned();
    let first = query_data(cx, q, &mut first_ref);
    let extras = match extra {
        None => Some(Vec::new()),
        Some(e) => extra_queries(cx, e, panel_type, &first_ref),
    };
    let mut first = first?;
    first.extra_queries = extras?;
    Some(first)
}

/// Reads the member `extra_queries` (4.15a): 1 to `MAX_QUERIES_PER_PANEL` minus 1 items. A `barchart` and an
/// `xychart` have none. `first_ref` is the ref of `data.query`.
fn extra_queries(
    cx: &mut Ctx,
    f: &Field<'_>,
    panel_type: Option<PanelType>,
    first_ref: &str,
) -> Option<Vec<ExtraQuery>> {
    let items = f.items(cx, 1, MAX_QUERIES_PER_PANEL - 1, ErrorCode::OutOfRange)?;
    let mut ok = true;
    if matches!(panel_type, Some(PanelType::Barchart | PanelType::Xychart | PanelType::Logs | PanelType::Geomap)) {
        f.fail(cx, ErrorCode::NotPermitted, "is not permitted in a panel that draws one frame only (4.15a, rule 7)");
        ok = false;
    }
    let mut seen: Vec<String> = vec![first_ref.to_owned()];
    let mut out = Vec::new();
    for item in &items {
        if cx.stopped() {
            return None;
        }
        let read = extra_query(cx, &item);
        // The format `logs` is for a logs panel only, and a logs panel has no extra query (4.33f).
        if let Some(q) = &read {
            super::logs::check_logs_format(cx, &format!("{}/format", item.ptr()), q.format, panel_type);
        }
        match read {
            Some(q) if !seen.contains(&q.query_ref) => {
                seen.push(q.query_ref.clone());
                out.push(q);
            }
            Some(q) => {
                cx.push(
                    &format!("{}/ref", item.ptr()),
                    ErrorCode::NotUnique,
                    format!("ref must be unique in the panel; value {}", quoted(&q.query_ref)),
                );
                ok = false;
            }
            None => ok = false,
        }
    }
    ok.then_some(out)
}

/// Reads one item of `extra_queries` (4.15a). The ref is necessary in an item.
fn extra_query(cx: &mut Ctx, f: &Field<'_>) -> Option<ExtraQuery> {
    let mut obj = f.object(cx, "extra query")?;
    let query_ref = obj.get(cx, "ref").and_then(|f| query_ref(cx, &f));
    let source = obj.get(cx, "source").and_then(|f| source_field(cx, &f));
    let format = obj.en::<QueryFormat>(cx, "format");
    let sql = obj.get(cx, "sql").and_then(|f| sql_lines(cx, &f));
    let hide = optional(&mut obj, "hide", |f| f.boolean(cx));
    obj.finish(cx, &["ref", "source", "format", "sql", "hide"]);
    Some(ExtraQuery {
        query_ref: query_ref?,
        source: source?,
        format: format?,
        sql: sql?,
        hide: hide?.unwrap_or(false),
    })
}

/// Reads the ref of a query (4.3): `^[A-Za-z][A-Za-z0-9_]{0,15}$`.
fn query_ref(cx: &mut Ctx, f: &Field<'_>) -> Option<String> {
    let text = f.text(cx, ANY)?;
    if is_valid_query_ref(&text) {
        Some(text)
    } else {
        f.fail(cx, ErrorCode::BadFormat, "must match ^[A-Za-z][A-Za-z0-9_]{0,15}$ (4.3)");
        None
    }
}

/// Reads `data.query`. `first_ref` gets the ref of the query: `A` when the member is absent or did not read.
fn query_data(cx: &mut Ctx, f: &Field<'_>, first_ref: &mut String) -> Option<QueryData> {
    let mut obj = f.object(cx, "query")?;
    let source = obj.get(cx, "source").and_then(|f| source_field(cx, &f));
    let format = obj.en::<QueryFormat>(cx, "format");
    let sql = obj.get(cx, "sql").and_then(|f| sql_lines(cx, &f));
    let min_interval = obj.get(cx, "min_interval").and_then(|f| f.nullable(|f| min_interval(cx, f)));
    let max_data_points = obj
        .get(cx, "max_data_points")
        .and_then(|f| f.nullable(|f| f.int(cx, 1, 10_000).and_then(|v| u32::try_from(v).ok())));
    let time_from = optional(&mut obj, "time_from", |f| {
        time_override(cx, f, "must be now-<count><unit>[/<unit>] or now/<unit> (4.15)", |t| {
            parse_time_from(t).is_some()
        })
    });
    let time_shift = optional(&mut obj, "time_shift", |f| {
        time_override(cx, f, "must be <count><unit> (4.15)", |t| parse_time_shift(t).is_some())
    });
    let hide_time_override = optional(&mut obj, "hide_time_override", |f| f.boolean(cx));
    let query_ref = optional(&mut obj, "ref", |f| query_ref(cx, f));
    if let Some(Some(r)) = &query_ref {
        first_ref.clone_from(r);
    }
    let hide = optional(&mut obj, "hide", |f| f.boolean(cx));
    obj.finish(
        cx,
        &[
            "source",
            "format",
            "sql",
            "min_interval",
            "max_data_points",
            "time_from",
            "time_shift",
            "hide_time_override",
            "ref",
            "hide",
        ],
    );
    Some(QueryData {
        source: source?,
        format: format?,
        sql: sql?,
        min_interval: min_interval?,
        max_data_points: max_data_points?,
        time_from: time_from?,
        time_shift: time_shift?,
        hide_time_override: hide_time_override?.unwrap_or(false),
        query_ref: query_ref?.unwrap_or_else(|| DEFAULT_QUERY_REF.to_owned()),
        hide: hide?.unwrap_or(false),
        extra_queries: Vec::new(),
    })
}

/// Reads the text of `time_from` or `time_shift`: a text that `valid` accepts.
fn time_override(cx: &mut Ctx, f: &Field<'_>, rule: &str, valid: impl FnOnce(&str) -> bool) -> Option<String> {
    let text = f.text(cx, ANY)?;
    if valid(&text) {
        Some(text)
    } else {
        f.fail(cx, ErrorCode::BadFormat, rule);
        None
    }
}

fn min_interval(cx: &mut Ctx, f: &Field<'_>) -> Option<String> {
    let text = f.text(cx, ANY)?;
    match parse_duration_ms(&text) {
        None => {
            f.fail(cx, ErrorCode::BadFormat, "must be a duration (4.4)");
            None
        }
        Some(ms) if !(1..=86_400_000).contains(&ms) => {
            f.fail(cx, ErrorCode::OutOfRange, "must be 1ms to 1d");
            None
        }
        Some(_) => Some(text),
    }
}

fn reuse_data(cx: &mut Ctx, f: &Field<'_>) -> Option<PanelData> {
    let mut obj = f.object(cx, "reuse")?;
    let panel = obj.int(cx, "panel", 1, 2_147_483_647).and_then(|v| u32::try_from(v).ok());
    let with_transforms = obj.boolean(cx, "with_transforms");
    obj.finish(cx, &["panel", "with_transforms"]);
    Some(PanelData::Reuse { panel: panel?, with_transforms: with_transforms? })
}

// ----------------------------------------------------------------- links

/// Reads an array of links with `min` items at least and `MAX_LINKS` at most.
fn links(cx: &mut Ctx, f: &Field<'_>, min: usize) -> Option<Vec<Link>> {
    let items = f.items(cx, min, MAX_LINKS, ErrorCode::TooLarge)?;
    collect(items.iter().map(|i| link(cx, &i)).collect())
}

fn link(cx: &mut Ctx, f: &Field<'_>) -> Option<Link> {
    let mut obj = f.object(cx, "link")?;
    let title = obj.text(cx, "title", TITLE);
    let url = obj.get(cx, "url").and_then(|f| url_template(cx, &f));
    let target_blank = obj.boolean(cx, "target_blank");
    obj.finish(cx, &["title", "url", "target_blank"]);
    Some(Link { title: title?, url: url?, target_blank: target_blank? })
}

/// Reads a URL template (4.23): 1 to 2048 bytes, and a permitted prefix.
fn url_template(cx: &mut Ctx, f: &Field<'_>) -> Option<String> {
    let url = f.text(cx, TextRule::bytes(1, MAX_URL_BYTES))?;
    if is_valid_url_prefix(&url) {
        return Some(url);
    }
    f.fail(
        cx,
        ErrorCode::BadFormat,
        "must start with /, ?, #, http:// or https:// before its first ${ (and not with // or /\\)",
    );
    None
}

// ---------------------------------------------------------------- fields

fn fields(cx: &mut Ctx, f: &Field<'_>, panel_type: Option<PanelType>) -> Option<Fields> {
    let mut obj = f.object(cx, "fields")?;
    let defaults = obj.get(cx, "defaults").and_then(|f| {
        property_set(cx, &f, "defaults", PropertyContext { panel_type, in_defaults: true, non_empty: false })
    });
    let overrides = obj.get(cx, "overrides").and_then(|f| {
        let items = f.items(cx, 0, MAX_OVERRIDES, ErrorCode::TooLarge)?;
        collect(items.iter().map(|i| field_override(cx, &i, panel_type)).collect())
    });
    obj.finish(cx, &["defaults", "overrides"]);
    Some(Fields { defaults: defaults?, overrides: overrides? })
}

fn field_override(cx: &mut Ctx, f: &Field<'_>, panel_type: Option<PanelType>) -> Option<Override> {
    let mut obj = f.object(cx, "override")?;
    let matcher = obj.get(cx, "match").and_then(|f| field_matcher(cx, &f));
    let set = obj
        .get(cx, "set")
        .and_then(|f| property_set(cx, &f, "set", PropertyContext { panel_type, in_defaults: false, non_empty: true }));
    obj.finish(cx, &["match", "set"]);
    Some(Override { matcher: matcher?, set: set? })
}

fn field_matcher(cx: &mut Ctx, f: &Field<'_>) -> Option<FieldMatcher> {
    let mut obj = f.object(cx, "match")?;
    let by = obj.en::<MatchBy>(cx, "by");
    match by {
        Some(MatchBy::Names) => {
            let names = obj.get(cx, "names").and_then(|f| {
                let items = f.items(cx, 1, MAX_MATCHER_NAMES, ErrorCode::TooLarge)?;
                collect(items.iter().map(|i| i.text(cx, VALUE)).collect())
            });
            let mode = obj.en::<NamesMode>(cx, "mode");
            obj.finish(cx, &["by", "names", "mode"]);
            Some(FieldMatcher::Names { names: names?, mode: mode? })
        }
        Some(kind) => {
            let value = obj.get(cx, "value").and_then(|f| match kind {
                MatchBy::Regex => pattern(cx, &f, PatternForm::Grafana),
                MatchBy::Query => query_ref(cx, &f),
                _ => f.text(cx, VALUE),
            });
            obj.finish(cx, &["by", "value"]);
            let value = value?;
            Some(match kind {
                MatchBy::Regex => FieldMatcher::Regex(value),
                MatchBy::Query => FieldMatcher::Query(value),
                _ => FieldMatcher::Name(value),
            })
        }
        None => {
            // The kind is not known: no other member can be checked, and none is unknown.
            obj.skip_all();
            None
        }
    }
}

#[derive(Clone, Copy)]
struct PropertyContext {
    /// `None`: the type is unknown. Then no check of the type applies.
    panel_type: Option<PanelType>,
    in_defaults: bool,
    non_empty: bool,
}

/// Where a member of a property set is permitted (4.18, rules 1 to 6).
#[derive(Clone, Copy)]
struct Place {
    /// The panel types that permit the member. An empty list means all types.
    types: &'static [PanelType],
    /// `true`: the member is permitted in `fields.defaults` only.
    defaults_only: bool,
    /// `true`: the member is permitted in an override `set` only.
    set_only: bool,
}

const EVERYWHERE: Place = Place { types: &[], defaults_only: false, set_only: false };
const TABLE: Place = Place { types: &[PanelType::Table], defaults_only: false, set_only: false };
const DEFAULTS_ONLY: Place = Place { types: &[], defaults_only: true, set_only: false };
const TIMESERIES: Place = Place { types: &[PanelType::Timeseries], defaults_only: false, set_only: false };
const TIMESERIES_SET: Place = Place { types: &[PanelType::Timeseries], defaults_only: false, set_only: true };
const AXIS: Place =
    Place { types: &[PanelType::Timeseries, PanelType::Barchart], defaults_only: false, set_only: false };
const BARCHART: Place = Place { types: &[PanelType::Barchart], defaults_only: false, set_only: false };

impl Place {
    /// The text of the rule for the type of the panel.
    fn type_rule(self) -> String {
        let names: Vec<&str> = self.types.iter().map(|t| TextEnum::as_str(*t)).collect();
        format!("is permitted in a panel of type {} only", names.join(" or "))
    }
}

/// Takes one member of a property set and checks where it is permitted (4.18, rules 1 to 6).
fn take_property<'a>(
    cx: &mut Ctx,
    obj: &mut Obj<'a>,
    name: &str,
    pc: PropertyContext,
    place: Place,
    ok: &mut bool,
) -> Option<Field<'a>> {
    let f = obj.opt(name)?;
    let rule = if pc.panel_type.is_some_and(|t| !place.types.is_empty() && !place.types.contains(&t)) {
        Some(place.type_rule())
    } else if place.defaults_only && !pc.in_defaults {
        Some("is permitted in fields.defaults only".to_owned())
    } else if place.set_only && pc.in_defaults {
        Some("is permitted in an override set only".to_owned())
    } else {
        None
    };
    if let Some(rule) = rule {
        f.fail(cx, ErrorCode::NotPermitted, &rule);
        *ok = false;
        return None;
    }
    if f.is_null() {
        f.fail(cx, ErrorCode::WrongType, "must not be null");
        *ok = false;
        return None;
    }
    Some(f)
}

fn property_set(cx: &mut Ctx, f: &Field<'_>, label: &'static str, pc: PropertyContext) -> Option<PropertySet> {
    let mut obj = f.object(cx, label)?;
    let mut ok = true;
    let mut ps = PropertySet::default();
    if pc.non_empty && obj.len() == 0 {
        cx.push(&f.ptr(), ErrorCode::MissingMember, "set must have 1 member or more".to_owned());
        ok = false;
    }
    macro_rules! prop {
        ($name:literal, $place:expr, $f:ident => $parse:expr, $slot:expr) => {
            if let Some($f) = take_property(cx, &mut obj, $name, pc, $place, &mut ok) {
                match $parse {
                    Some(v) => $slot = Some(v),
                    None => ok = false,
                }
            }
        };
    }
    prop!("unit", EVERYWHERE, f => unit(cx, &f), ps.unit);
    prop!("decimals", EVERYWHERE, f => f.int(cx, 0, 20).and_then(|v| u8::try_from(v).ok()), ps.decimals);
    prop!("min", EVERYWHERE, f => f.num(cx), ps.min);
    prop!("max", EVERYWHERE, f => f.num(cx), ps.max);
    prop!("color", EVERYWHERE, f => color_spec(cx, &f), ps.color);
    prop!("thresholds", EVERYWHERE, f => thresholds(cx, &f), ps.thresholds);
    prop!("mappings", EVERYWHERE, f => mappings(cx, &f), ps.mappings);
    prop!("links", EVERYWHERE, f => links(cx, &f, 1), ps.links);
    prop!("display_name", EVERYWHERE, f => f.text(cx, TITLE), ps.display_name);
    prop!("no_value", DEFAULTS_ONLY, f => f.text(cx, TITLE), ps.no_value);
    prop!("width", TABLE, f => f.int(cx, 10, 4000).and_then(|v| u32::try_from(v).ok()), ps.width);
    prop!("align", TABLE, f => f.en::<Align>(cx), ps.align);
    prop!("cell", TABLE, f => cell(cx, &f), ps.cell);
    prop!("filterable", TABLE, f => f.boolean(cx), ps.filterable);
    prop!("hidden", TABLE, f => f.boolean(cx), ps.hidden);
    prop!("footer", TABLE, f => footer(cx, &f), ps.footer);
    prop!("inspect", TABLE, f => f.boolean(cx), ps.inspect);
    prop!("hide_from", EVERYWHERE, f => hide_from(cx, &f), ps.hide_from);
    prop!("axis_placement", TIMESERIES, f => f.en::<AxisPlacement>(cx), ps.axis_placement);
    prop!("axis_label", AXIS, f => f.text(cx, TITLE), ps.axis_label);
    prop!("soft_min", AXIS, f => f.num(cx), ps.soft_min);
    prop!("soft_max", AXIS, f => f.num(cx), ps.soft_max);
    prop!("axis_grid", BARCHART, f => f.boolean(cx), ps.axis_grid);
    prop!("draw", TIMESERIES_SET, f => f.en::<TimeseriesDraw>(cx), ps.draw);
    prop!("line_width", TIMESERIES_SET, f => f.int(cx, 0, 10).and_then(|v| u8::try_from(v).ok()), ps.line_width);
    prop!("fill_opacity", TIMESERIES_SET, f => f.int(cx, 0, 100).and_then(|v| u8::try_from(v).ok()), ps.fill_opacity);
    prop!("line_dash", TIMESERIES, f => f.en::<LineDash>(cx), ps.line_dash);
    prop!("show_points", TIMESERIES_SET, f => f.en::<ShowPoints>(cx), ps.show_points);
    prop!("point_size", TIMESERIES_SET, f => f.int(cx, 1, 40).and_then(|v| u8::try_from(v).ok()), ps.point_size);
    prop!("transform", TIMESERIES, f => f.en::<SeriesTransform>(cx), ps.transform);
    // The members of wave H, item P14 (4.18).
    prop!("line_dash_array", TIMESERIES, f => dash_array(cx, &f), ps.line_dash_array);
    prop!("axis_centered_zero", AXIS, f => f.boolean(cx), ps.axis_centered_zero);
    prop!("axis_width", AXIS, f => f.int(cx, 0, 500).and_then(|v| u16::try_from(v).ok()), ps.axis_width);
    prop!("axis_border", AXIS, f => f.boolean(cx), ps.axis_border);
    prop!("axis_color", AXIS, f => f.en::<AxisColor>(cx), ps.axis_color);
    obj.finish(
        cx,
        &[
            "unit",
            "decimals",
            "min",
            "max",
            "color",
            "thresholds",
            "mappings",
            "links",
            "display_name",
            "no_value",
            "width",
            "align",
            "cell",
            "filterable",
            "hidden",
            "footer",
            "inspect",
            "hide_from",
            "axis_placement",
            "axis_label",
            "soft_min",
            "soft_max",
            "axis_grid",
            "draw",
            "line_width",
            "fill_opacity",
            "line_dash",
            "show_points",
            "point_size",
            "transform",
            "line_dash_array",
            "axis_centered_zero",
            "axis_width",
            "axis_border",
            "axis_color",
        ],
    );
    if let (Some(min), Some(max)) = (ps.min, ps.max)
        && min >= max
    {
        cx.push(
            &format!("{}/max", f.ptr()),
            ErrorCode::OutOfRange,
            format!("max must be greater than min; value {max}"),
        );
        ok = false;
    }
    if let (Some(min), Some(max)) = (ps.soft_min, ps.soft_max)
        && min >= max
    {
        cx.push(
            &format!("{}/soft_max", f.ptr()),
            ErrorCode::OutOfRange,
            format!("soft_max must be greater than soft_min; value {max}"),
        );
        ok = false;
    }
    ok.then_some(ps)
}

fn unit(cx: &mut Ctx, f: &Field<'_>) -> Option<String> {
    let text = f.text(cx, ANY)?;
    if is_valid_unit(&text) {
        Some(text)
    } else {
        f.fail(cx, ErrorCode::BadFormat, "must be a unit (4.20)");
        None
    }
}

fn cell(cx: &mut Ctx, f: &Field<'_>) -> Option<Cell> {
    let mut obj = f.object(cx, "cell")?;
    let cell_type = obj.en::<CellType>(cx, "type");
    let mode_field = obj.get(cx, "mode");
    let mode = mode_field.as_ref().and_then(|f| f.nullable(|f| f.en::<CellMode>(cx)));
    obj.finish(cx, &["type", "mode"]);
    if let (Some(t), Some(m), Some(mf)) = (cell_type, mode, &mode_field) {
        let (wants, rule): (bool, &str) = match t {
            CellType::ColorBackground => (true, "must be gradient or basic with color_background"),
            CellType::Gauge => (true, "must be basic, gradient or lcd with gauge"),
            _ => (false, "must be null without color_background or gauge"),
        };
        if wants != m.is_some() {
            mf.fail(cx, ErrorCode::NotPermitted, rule);
            return None;
        }
        if t == CellType::ColorBackground && m == Some(CellMode::Lcd) {
            mf.fail(cx, ErrorCode::BadFormat, rule);
            return None;
        }
    }
    Some(Cell { cell_type: cell_type?, mode: mode? })
}

fn footer(cx: &mut Ctx, f: &Field<'_>) -> Option<Footer> {
    let mut obj = f.object(cx, "footer")?;
    let reducers = obj.get(cx, "reducers").and_then(|f| enum_list(cx, &f, 1, MAX_FOOTER_REDUCERS, ReduceCalc::ALL));
    obj.finish(cx, &["reducers"]);
    Some(Footer { reducers: reducers? })
}

fn hide_from(cx: &mut Ctx, f: &Field<'_>) -> Option<HideFrom> {
    let mut obj = f.object(cx, "hide_from")?;
    let legend = obj.boolean(cx, "legend");
    let tooltip = obj.boolean(cx, "tooltip");
    let viz = obj.boolean(cx, "viz");
    obj.finish(cx, &["legend", "tooltip", "viz"]);
    Some(HideFrom { legend: legend?, tooltip: tooltip?, viz: viz? })
}

fn color_spec(cx: &mut Ctx, f: &Field<'_>) -> Option<ColorSpec> {
    let mut obj = f.object(cx, "color")?;
    let mode = obj.get(cx, "mode").and_then(|f| {
        // The value `palette_by_name` was removed (4.19): the one series palette is the mode `palette`.
        if matches!(f.json, Json::Str(s) if s == "palette_by_name") {
            f.fail(
                cx,
                ErrorCode::BadFormat,
                "palette_by_name is removed: the mode palette is the one series palette (value palette_by_name)",
            );
            return None;
        }
        f.en::<ColorMode>(cx)
    });
    let fixed_field = obj.get(cx, "fixed");
    let fixed = fixed_field.as_ref().and_then(|f| f.nullable(|f| colour(cx, f)));
    obj.finish(cx, &["mode", "fixed"]);
    if let (Some(m), Some(fx), Some(ff)) = (mode, &fixed, &fixed_field) {
        let wants = m == ColorMode::Fixed;
        if wants != fx.is_some() {
            let rule = if wants { "must be a colour with mode fixed" } else { "must be null without mode fixed" };
            ff.fail(cx, ErrorCode::NotPermitted, rule);
            return None;
        }
    }
    Some(ColorSpec { mode: mode?, fixed: fixed? })
}

fn thresholds(cx: &mut Ctx, f: &Field<'_>) -> Option<Thresholds> {
    let mut obj = f.object(cx, "thresholds")?;
    let mode = obj.en::<ThresholdMode>(cx, "mode");
    let steps = obj.get(cx, "steps").and_then(|f| {
        let items = f.items(cx, 1, MAX_THRESHOLD_STEPS, ErrorCode::TooLarge)?;
        let mut previous: Option<f64> = None;
        let mut out = Vec::new();
        let mut ok = true;
        for (i, item) in items.iter().enumerate() {
            match threshold_step(cx, &item, i, &mut previous) {
                Some(s) => out.push(s),
                None => ok = false,
            }
        }
        ok.then_some(out)
    });
    obj.finish(cx, &["mode", "steps"]);
    Some(Thresholds { mode: mode?, steps: steps? })
}

fn threshold_step(cx: &mut Ctx, f: &Field<'_>, index: usize, previous: &mut Option<f64>) -> Option<ThresholdStep> {
    let mut obj = f.object(cx, "step")?;
    let value_field = obj.get(cx, "value");
    let value = value_field.as_ref().and_then(|v| v.nullable(|v| v.num(cx)));
    let color = obj.get(cx, "color").and_then(|f| colour(cx, &f));
    obj.finish(cx, &["value", "color"]);
    let mut ok = true;
    if let (Some(v), Some(vf)) = (value, &value_field) {
        match (index, v) {
            (0, Some(_)) => {
                vf.fail(cx, ErrorCode::NotPermitted, "must be null in the first step");
                ok = false;
            }
            (0, None) => {}
            (_, None) => {
                vf.fail(cx, ErrorCode::NotPermitted, "must be a number in a step after the first");
                ok = false;
            }
            (_, Some(n)) => {
                if let Some(p) = *previous
                    && n <= p
                {
                    vf.fail(
                        cx,
                        ErrorCode::OutOfRange,
                        &format!("must be greater than the value of the step before ({p})"),
                    );
                    ok = false;
                }
                *previous = Some(n);
            }
        }
    }
    if !ok {
        return None;
    }
    Some(ThresholdStep { value: value?, color: color? })
}

fn mappings(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<Mapping>> {
    let items = f.items(cx, 1, MAX_MAPPINGS, ErrorCode::TooLarge)?;
    collect(items.iter().map(|i| mapping(cx, &i)).collect())
}

fn mapping(cx: &mut Ctx, f: &Field<'_>) -> Option<Mapping> {
    let mut obj = f.object(cx, "mapping")?;
    let Some(tag) = obj.en::<MappingTag>(cx, "kind") else {
        obj.skip_all();
        return None;
    };
    let text_color = |cx: &mut Ctx, obj: &mut Obj<'_>| {
        let text = obj.get(cx, "text").and_then(|f| f.nullable(|f| f.text(cx, TITLE)));
        let color = obj.get(cx, "color").and_then(|f| f.nullable(|f| colour(cx, f)));
        (text, color)
    };
    let nullable_num =
        |cx: &mut Ctx, obj: &mut Obj<'_>, name: &str| obj.get(cx, name).and_then(|f| f.nullable(|f| f.num(cx)));
    let (shape, permitted): (Option<MappingShape>, &[&str]) = match tag {
        MappingTag::Value => {
            let value = obj.text(cx, "value", TextRule::bytes(0, MAX_VALUE_BYTES));
            (value.map(MappingShape::Value), &["kind", "value", "text", "color"])
        }
        MappingTag::Range => {
            let from = nullable_num(cx, &mut obj, "from");
            let to = nullable_num(cx, &mut obj, "to");
            let shape = from.zip(to).map(|(from, to)| MappingShape::Range(from, to));
            (shape, &["kind", "from", "to", "text", "color"])
        }
        MappingTag::Regex => {
            let p = obj.get(cx, "pattern").and_then(|f| pattern(cx, &f, PatternForm::Grafana));
            (p.map(MappingShape::Regex), &["kind", "pattern", "text", "color"])
        }
        MappingTag::Special => {
            let m = obj.en::<SpecialMatch>(cx, "match");
            (m.map(MappingShape::Special), &["kind", "match", "text", "color"])
        }
    };
    let (text, color) = text_color(cx, &mut obj);
    obj.finish(cx, permitted);
    let (shape, text, color) = (shape?, text?, color?);
    if text.is_none() && color.is_none() {
        cx.push(&f.ptr(), ErrorCode::MissingMember, "a mapping must have text or color; both are null".to_owned());
        return None;
    }
    if let MappingShape::Range(from, to) = &shape {
        match (from, to) {
            (None, None) => {
                cx.push(
                    &f.ptr(),
                    ErrorCode::MissingMember,
                    "a range mapping must have from or to; both are null".to_owned(),
                );
                return None;
            }
            (Some(a), Some(b)) if a > b => {
                cx.push(&f.ptr(), ErrorCode::OutOfRange, format!("from must be at most to; value {a} and {b}"));
                return None;
            }
            _ => {}
        }
    }
    Some(match shape {
        MappingShape::Value(value) => Mapping::Value { value, text, color },
        MappingShape::Range(from, to) => Mapping::Range { from, to, text, color },
        MappingShape::Regex(pattern) => Mapping::Regex { pattern, text, color },
        MappingShape::Special(matches) => Mapping::Special { matches, text, color },
    })
}

enum MappingShape {
    Value(String),
    Range(Option<f64>, Option<f64>),
    Regex(String),
    Special(SpecialMatch),
}

// ------------------------------------------------------------- transforms

fn transform(cx: &mut Ctx, f: &Field<'_>) -> Option<Transform> {
    let mut obj = f.object(cx, "transform")?;
    let Some(tag) = obj.en::<TransformTag>(cx, "kind") else {
        obj.skip_all();
        return None;
    };
    match tag {
        TransformTag::FilterByValue => {
            let mode = obj.en::<FilterMode>(cx, "mode");
            let match_mode = obj.en::<FilterMatch>(cx, "match");
            let filters = obj.get(cx, "filters").and_then(|f| {
                let items = f.items(cx, 1, MAX_FILTERS, ErrorCode::TooLarge)?;
                collect(items.iter().map(|i| filter(cx, &i)).collect())
            });
            obj.finish(cx, &["kind", "mode", "match", "filters"]);
            Some(Transform::FilterByValue { mode: mode?, match_mode: match_mode?, filters: filters? })
        }
        TransformTag::GroupingToMatrix => {
            let column_field = obj.text(cx, "column_field", VALUE);
            let row_field = obj.text(cx, "row_field", VALUE);
            let value_field = obj.text(cx, "value_field", VALUE);
            let empty_value = obj.en::<EmptyValue>(cx, "empty_value");
            obj.finish(cx, &["kind", "column_field", "row_field", "value_field", "empty_value"]);
            Some(Transform::GroupingToMatrix {
                column_field: column_field?,
                row_field: row_field?,
                value_field: value_field?,
                empty_value: empty_value?,
            })
        }
        // The eleven kinds of 4.25a have their own walk.
        more_kind => super::walk_transforms::more(cx, obj, more_kind),
    }
}

fn filter(cx: &mut Ctx, f: &Field<'_>) -> Option<Filter> {
    let mut obj = f.object(cx, "filter")?;
    let field = obj.text(cx, "field", VALUE);
    let matcher = obj.get(cx, "matcher").and_then(|f| filter_matcher(cx, &f));
    obj.finish(cx, &["field", "matcher"]);
    Some(Filter { field: field?, matcher: matcher? })
}

fn scalar(cx: &mut Ctx, f: &Field<'_>) -> Option<Scalar> {
    match f.json {
        Json::Num(_) => f.num(cx).map(Scalar::Number),
        Json::Str(_) => f.text(cx, TextRule::bytes(0, MAX_VALUE_BYTES)).map(Scalar::Text),
        _ => {
            f.fail(cx, ErrorCode::WrongType, "must be a number or text");
            None
        }
    }
}

fn filter_matcher(cx: &mut Ctx, f: &Field<'_>) -> Option<FilterMatcher> {
    let mut obj = f.object(cx, "matcher")?;
    let Some(tag) = obj.en::<MatcherTag>(cx, "kind") else {
        obj.skip_all();
        return None;
    };
    match tag {
        MatcherTag::Equal | MatcherTag::NotEqual => {
            let v = obj.get(cx, "value").and_then(|f| scalar(cx, &f));
            obj.finish(cx, &["kind", "value"]);
            if tag == MatcherTag::Equal { v.map(FilterMatcher::Equal) } else { v.map(FilterMatcher::NotEqual) }
        }
        MatcherTag::Greater | MatcherTag::GreaterOrEqual | MatcherTag::Lower | MatcherTag::LowerOrEqual => {
            let v = obj.num(cx, "value");
            obj.finish(cx, &["kind", "value"]);
            v.map(|v| match tag {
                MatcherTag::Greater => FilterMatcher::Greater(v),
                MatcherTag::GreaterOrEqual => FilterMatcher::GreaterOrEqual(v),
                MatcherTag::Lower => FilterMatcher::Lower(v),
                _ => FilterMatcher::LowerOrEqual(v),
            })
        }
        MatcherTag::Range => {
            let from = obj.num(cx, "from");
            let to_field = obj.get(cx, "to");
            let to = to_field.as_ref().and_then(|f| f.num(cx));
            obj.finish(cx, &["kind", "from", "to"]);
            match (from, to, &to_field) {
                (Some(a), Some(b), Some(tf)) if a > b => {
                    tf.fail(cx, ErrorCode::OutOfRange, &format!("must be at least from ({a})"));
                    None
                }
                (Some(from), Some(to), _) => Some(FilterMatcher::Range { from, to }),
                _ => None,
            }
        }
        MatcherTag::Regex => {
            let p = obj.get(cx, "pattern").and_then(|f| pattern(cx, &f, PatternForm::Plain));
            obj.finish(cx, &["kind", "pattern"]);
            p.map(FilterMatcher::Regex)
        }
        MatcherTag::IsNull => {
            obj.finish(cx, &["kind"]);
            Some(FilterMatcher::IsNull)
        }
        MatcherTag::IsNotNull => {
            obj.finish(cx, &["kind"]);
            Some(FilterMatcher::IsNotNull)
        }
    }
}

// ---------------------------------------------------------------- options

fn options(cx: &mut Ctx, f: &Field<'_>, panel_type: PanelType) -> Option<PanelOptions> {
    match panel_type {
        PanelType::Table => table_options(cx, f).map(PanelOptions::Table),
        PanelType::Timeseries => timeseries_options(cx, f).map(PanelOptions::Timeseries),
        PanelType::Stat => stat_options(cx, f).map(PanelOptions::Stat),
        PanelType::Barchart => barchart_options(cx, f).map(PanelOptions::Barchart),
        PanelType::Piechart => piechart_options(cx, f).map(PanelOptions::Piechart),
        PanelType::Xychart => xychart_options(cx, f).map(PanelOptions::Xychart),
        PanelType::Bargauge => bargauge_options(cx, f).map(PanelOptions::Bargauge),
        PanelType::Text => text_options(cx, f).map(PanelOptions::Text),
        PanelType::StateTimeline => state_timeline_options(cx, f).map(PanelOptions::StateTimeline),
        PanelType::StatusHistory => status_history_options(cx, f).map(PanelOptions::StatusHistory),
        PanelType::Heatmap => heatmap_options(cx, f).map(PanelOptions::Heatmap),
        PanelType::Histogram => histogram_options(cx, f).map(PanelOptions::Histogram),
        PanelType::Logs => super::logs::logs_options(cx, f).map(PanelOptions::Logs),
        PanelType::Gauge => gauge_options(cx, f).map(PanelOptions::Gauge),
        PanelType::Geomap => super::geomap::options(cx, f).map(PanelOptions::Geomap),
    }
}

/// Reads the `options` of a text panel (4.33a): the mode `markdown` and the lines of the content.
fn text_options(cx: &mut Ctx, f: &Field<'_>) -> Option<TextOptions> {
    let mut obj = f.object(cx, "options")?;
    let mode = obj.en::<TextPanelMode>(cx, "mode");
    let content = obj.get(cx, "content").and_then(|f| text_lines(cx, &f));
    obj.finish(cx, &["mode", "content"]);
    Some(TextOptions { mode: mode?, content: content? })
}

/// Reads the lines of a text panel (4.33a, rule 2 and 3). The form is the form of `sql_lines` (4.16) with 0
/// to `MAX_TEXT_LINES` lines. A limit gives `too_large`. A text with no Markdown check: only the lines.
fn text_lines(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<String>> {
    let items = f.items(cx, 0, MAX_TEXT_LINES, ErrorCode::TooLarge)?;
    let rule = TextRule { min: 0, max: usize::MAX, bytes: true, allow_tab: true };
    let mut lines = Vec::with_capacity(items.len());
    let mut ok = true;
    for item in &items {
        let Json::Str(s) = item.json else {
            item.fail(cx, ErrorCode::WrongType, "must be text");
            ok = false;
            continue;
        };
        if s.contains(['\n', '\r']) {
            item.fail(cx, ErrorCode::BadFormat, "must not contain U+000A or U+000D");
            ok = false;
            continue;
        }
        if s.len() > MAX_TEXT_LINE_BYTES {
            item.fail(cx, ErrorCode::TooLarge, &format!("must have at most {MAX_TEXT_LINE_BYTES} bytes"));
            ok = false;
            continue;
        }
        match item.text(cx, rule) {
            Some(line) => lines.push(line),
            None => ok = false,
        }
    }
    if !ok {
        return None;
    }
    let total: usize = lines.iter().map(String::len).sum::<usize>() + lines.len().saturating_sub(1);
    if total > MAX_TEXT_BYTES {
        cx.push(
            &f.ptr(),
            ErrorCode::TooLarge,
            format!("{} must have at most {MAX_TEXT_BYTES} bytes when joined; value {total} bytes", f.name()),
        );
        return None;
    }
    Some(lines)
}

fn small(cx: &mut Ctx, obj: &mut Obj<'_>, name: &str, min: i64, max: i64) -> Option<u8> {
    obj.int(cx, name, min, max).and_then(|v| u8::try_from(v).ok())
}

/// Reads an array of enum values: unique items, each in `allowed`.
fn enum_list<E: TextEnum + PartialEq>(
    cx: &mut Ctx,
    f: &Field<'_>,
    min: usize,
    max: usize,
    allowed: &[E],
) -> Option<Vec<E>> {
    let items = f.items(cx, min, max, ErrorCode::OutOfRange)?;
    let parsed: Vec<Option<E>> = items
        .iter()
        .map(|item| {
            let v = item.en::<E>(cx)?;
            if allowed.contains(&v) {
                Some(v)
            } else {
                let names: Vec<&str> = allowed.iter().map(|a| a.as_str()).collect();
                item.fail(cx, ErrorCode::BadFormat, &format!("must be one of {}", names.join(", ")));
                None
            }
        })
        .collect();
    unique(cx, &items, &parsed);
    let mut seen: Vec<E> = Vec::new();
    for v in parsed.iter().flatten() {
        if seen.contains(v) {
            return None;
        }
        seen.push(*v);
    }
    parsed.into_iter().collect()
}

/// Reads a member that can be absent. An absent member gives `Some(None)`: the caller uses the
/// default. A member that is present and not valid gives `None`, and the error is in `cx`.
fn optional<'a, T>(obj: &mut Obj<'a>, name: &str, parse: impl FnOnce(&Field<'a>) -> Option<T>) -> Option<Option<T>> {
    match obj.opt(name) {
        None => Some(None),
        Some(f) => parse(&f).map(Some),
    }
}

/// The values of `legend.values` outside a `piechart`: each value of `ReduceCalc`.
const CALC_LEGEND_VALUES: [LegendValue; 18] = [
    LegendValue::Total,
    LegendValue::Min,
    LegendValue::Max,
    LegendValue::Mean,
    LegendValue::Last,
    LegendValue::LastNotNull,
    LegendValue::First,
    LegendValue::FirstNotNull,
    LegendValue::Count,
    LegendValue::Median,
    LegendValue::StdDev,
    LegendValue::Variance,
    LegendValue::Range,
    LegendValue::Diff,
    LegendValue::DiffPercent,
    LegendValue::Delta,
    LegendValue::DistinctCount,
    LegendValue::ChangeCount,
];

/// The values of `legend.sort_by` outside a `piechart`: `name` and each value of `ReduceCalc`.
const CALC_SORT_BY: [LegendSortBy; 19] = [
    LegendSortBy::Name,
    LegendSortBy::Total,
    LegendSortBy::Min,
    LegendSortBy::Max,
    LegendSortBy::Mean,
    LegendSortBy::Last,
    LegendSortBy::LastNotNull,
    LegendSortBy::First,
    LegendSortBy::FirstNotNull,
    LegendSortBy::Count,
    LegendSortBy::Median,
    LegendSortBy::StdDev,
    LegendSortBy::Variance,
    LegendSortBy::Range,
    LegendSortBy::Diff,
    LegendSortBy::DiffPercent,
    LegendSortBy::Delta,
    LegendSortBy::DistinctCount,
    LegendSortBy::ChangeCount,
];

fn legend_sort_by(cx: &mut Ctx, f: &Field<'_>, pie: bool) -> Option<LegendSortBy> {
    let allowed: &[LegendSortBy] =
        if pie { &[LegendSortBy::Name, LegendSortBy::Value, LegendSortBy::Percent] } else { &CALC_SORT_BY };
    let v = f.en::<LegendSortBy>(cx)?;
    if allowed.contains(&v) {
        Some(v)
    } else {
        let names: Vec<&str> = allowed.iter().map(|a| a.as_str()).collect();
        f.fail(cx, ErrorCode::BadFormat, &format!("must be one of {}", names.join(", ")));
        None
    }
}

fn legend(cx: &mut Ctx, f: &Field<'_>, pie: bool) -> Option<Legend> {
    let mut obj = f.object(cx, "legend")?;
    let show = obj.boolean(cx, "show");
    let placement = obj.en::<LegendPlacement>(cx, "placement");
    let mode = obj.en::<LegendMode>(cx, "mode");
    let values = obj.get(cx, "values").and_then(|f| {
        if pie {
            enum_list(cx, &f, 0, 2, &[LegendValue::Value, LegendValue::Percent])
        } else {
            enum_list(cx, &f, 0, 5, &CALC_LEGEND_VALUES)
        }
    });
    let sort_by = optional(&mut obj, "sort_by", |f| legend_sort_by(cx, f, pie));
    let sort_desc = optional(&mut obj, "sort_desc", |f| f.boolean(cx));
    obj.finish(cx, &["show", "placement", "mode", "values", "sort_by", "sort_desc"]);
    Some(Legend {
        show: show?,
        placement: placement?,
        mode: mode?,
        values: values?,
        sort_by: sort_by?,
        sort_desc: sort_desc?.unwrap_or(false),
    })
}

fn tooltip(cx: &mut Ctx, f: &Field<'_>) -> Option<Tooltip> {
    let mut obj = f.object(cx, "tooltip")?;
    let mode = obj.en::<TooltipMode>(cx, "mode");
    let sort = optional(&mut obj, "sort", |f| f.en::<TooltipSort>(cx));
    let hide_zeros = optional(&mut obj, "hide_zeros", |f| f.boolean(cx));
    obj.finish(cx, &["mode", "sort", "hide_zeros"]);
    Some(Tooltip { mode: mode?, sort: sort?.unwrap_or(TooltipSort::None), hide_zeros: hide_zeros?.unwrap_or(false) })
}

fn reduce(cx: &mut Ctx, f: &Field<'_>) -> Option<Reduce> {
    let mut obj = f.object(cx, "reduce")?;
    let calc = obj.en::<ReduceCalc>(cx, "calc");
    let fields = obj.get(cx, "fields").and_then(|f| f.nullable(|f| pattern(cx, f, PatternForm::Grafana)));
    let values = obj.boolean(cx, "values");
    obj.finish(cx, &["calc", "fields", "values"]);
    Some(Reduce { calc: calc?, fields: fields?, values: values? })
}

fn table_options(cx: &mut Ctx, f: &Field<'_>) -> Option<TableOptions> {
    let mut obj = f.object(cx, "options")?;
    let show_header = obj.boolean(cx, "show_header");
    let sort = obj.get(cx, "sort").and_then(|f| {
        let items = f.items(cx, 0, MAX_SORT_KEYS, ErrorCode::TooLarge)?;
        collect(
            items
                .iter()
                .map(|i| {
                    let mut o = i.object(cx, "sort key")?;
                    let field = o.text(cx, "field", VALUE);
                    let desc = o.boolean(cx, "desc");
                    o.finish(cx, &["field", "desc"]);
                    Some(SortKey { field: field?, desc: desc? })
                })
                .collect(),
        )
    });
    let wrap = optional(&mut obj, "wrap", |f| f.boolean(cx));
    obj.finish(cx, &["show_header", "sort", "wrap"]);
    Some(TableOptions { show_header: show_header?, sort: sort?, wrap: wrap?.unwrap_or(false) })
}

/// The property `line_dash_array` (4.18, P14): 1 to 8 integers from 0 to 100, one of them above 0. The count is checked before the items.
fn dash_array(cx: &mut Ctx, f: &Field<'_>) -> Option<Vec<u8>> {
    let items = f.items(cx, 1, 8, ErrorCode::TooLarge)?;
    let parsed: Vec<Option<u8>> = items.iter().map(|i| i.int(cx, 0, 100).and_then(|v| u8::try_from(v).ok())).collect();
    let out: Option<Vec<u8>> = parsed.into_iter().collect();
    if out.as_ref().is_some_and(|v| v.iter().all(|d| *d == 0)) {
        f.fail(cx, ErrorCode::OutOfRange, "must have one item above 0");
        return None;
    }
    out
}

fn timeseries_options(cx: &mut Ctx, f: &Field<'_>) -> Option<TimeseriesOptions> {
    let mut obj = f.object(cx, "options")?;
    let draw = obj.en::<TimeseriesDraw>(cx, "draw");
    let interpolation = obj.en::<Interpolation>(cx, "interpolation");
    let line_width = small(cx, &mut obj, "line_width", 0, 10);
    let fill_opacity = small(cx, &mut obj, "fill_opacity", 0, 100);
    let show_points = obj.en::<ShowPoints>(cx, "show_points");
    let point_size = small(cx, &mut obj, "point_size", 1, 40);
    let span_nulls = obj.boolean(cx, "span_nulls");
    let stacking = obj.en::<Stacking>(cx, "stacking");
    let y_scale = obj.en::<YScale>(cx, "y_scale");
    let legend = obj.get(cx, "legend").and_then(|f| legend(cx, &f, false));
    let tooltip = obj.get(cx, "tooltip").and_then(|f| tooltip(cx, &f));
    let thresholds_style = optional(&mut obj, "thresholds_style", |f| f.en::<ThresholdsStyle>(cx));
    // The optional members of wave H, items P10 and P14 (4.28): an absent member has its default.
    let gradient_mode = optional(&mut obj, "gradient_mode", |f| f.en::<GradientMode>(cx));
    let log_base = optional(&mut obj, "log_base", |f| match f.int(cx, 2, 10)? {
        v @ (2 | 10) => u8::try_from(v).ok(),
        _ => {
            f.fail(cx, ErrorCode::OutOfRange, "must be 2 or 10");
            None
        }
    });
    let span_nulls_ms =
        optional(&mut obj, "span_nulls_ms", |f| f.int(cx, 1, MAX_SPAN_NULLS_MS).and_then(|v| u64::try_from(v).ok()));
    let bar_alignment = optional(&mut obj, "bar_alignment", |f| f.en::<BarAlignment>(cx));
    obj.finish(
        cx,
        &[
            "draw",
            "interpolation",
            "line_width",
            "fill_opacity",
            "show_points",
            "point_size",
            "span_nulls",
            "stacking",
            "y_scale",
            "legend",
            "tooltip",
            "thresholds_style",
            "gradient_mode",
            "log_base",
            "span_nulls_ms",
            "bar_alignment",
        ],
    );
    Some(TimeseriesOptions {
        draw: draw?,
        interpolation: interpolation?,
        line_width: line_width?,
        fill_opacity: fill_opacity?,
        show_points: show_points?,
        point_size: point_size?,
        span_nulls: span_nulls?,
        stacking: stacking?,
        y_scale: y_scale?,
        legend: legend?,
        tooltip: tooltip?,
        thresholds_style: thresholds_style?.unwrap_or(ThresholdsStyle::Off),
        gradient_mode: gradient_mode?.unwrap_or(GradientMode::None),
        log_base: log_base?.unwrap_or(LOG_BASE),
        span_nulls_ms: span_nulls_ms?,
        bar_alignment: bar_alignment?.unwrap_or(BarAlignment::Center),
    })
}

fn stat_options(cx: &mut Ctx, f: &Field<'_>) -> Option<StatOptions> {
    let mut obj = f.object(cx, "options")?;
    let reduce = obj.get(cx, "reduce").and_then(|f| reduce(cx, &f));
    let color_mode = obj.en::<StatColorMode>(cx, "color_mode");
    let graph_mode = obj.en::<GraphMode>(cx, "graph_mode");
    let text_mode = obj.en::<TextMode>(cx, "text_mode");
    let orientation = obj.en::<StatOrientation>(cx, "orientation");
    let justify = optional(&mut obj, "justify", |f| f.en::<StatJustify>(cx));
    // The optional members of wave H, item P11 (4.29): an absent member has the Grafana default.
    let percent_change = optional(&mut obj, "percent_change", |f| f.boolean(cx));
    let percent_change_color = optional(&mut obj, "percent_change_color", |f| f.en::<PercentChangeColor>(cx));
    let title_size = optional(&mut obj, "title_size", |f| f.int(cx, 1, 200).and_then(|v| u8::try_from(v).ok()));
    let value_size = optional(&mut obj, "value_size", |f| f.int(cx, 1, 200).and_then(|v| u8::try_from(v).ok()));
    obj.finish(
        cx,
        &[
            "reduce",
            "color_mode",
            "graph_mode",
            "text_mode",
            "orientation",
            "justify",
            "percent_change",
            "percent_change_color",
            "title_size",
            "value_size",
        ],
    );
    Some(StatOptions {
        reduce: reduce?,
        color_mode: color_mode?,
        graph_mode: graph_mode?,
        text_mode: text_mode?,
        orientation: orientation?,
        justify: justify?.unwrap_or(StatJustify::Auto),
        percent_change: percent_change?.unwrap_or(false),
        percent_change_color: percent_change_color?.unwrap_or(PercentChangeColor::Standard),
        title_size: title_size?,
        value_size: value_size?,
    })
}

fn barchart_options(cx: &mut Ctx, f: &Field<'_>) -> Option<BarchartOptions> {
    let mut obj = f.object(cx, "options")?;
    let orientation = obj.en::<BarchartOrientation>(cx, "orientation");
    let x_field = obj.get(cx, "x_field").and_then(|f| f.nullable(|f| f.text(cx, VALUE)));
    let stacking = obj.en::<Stacking>(cx, "stacking");
    let show_values = obj.en::<ShowValues>(cx, "show_values");
    let bar_width = small(cx, &mut obj, "bar_width", 10, 100);
    let group_width = small(cx, &mut obj, "group_width", 10, 100);
    let x_tick_rotation = obj.int(cx, "x_tick_rotation", -90, 90).and_then(|v| i8::try_from(v).ok());
    let legend = obj.get(cx, "legend").and_then(|f| legend(cx, &f, false));
    let tooltip = obj.get(cx, "tooltip").and_then(|f| tooltip(cx, &f));
    let thresholds_style = optional(&mut obj, "thresholds_style", |f| f.en::<ThresholdsStyle>(cx));
    // The optional members of wave H, item P03 (4.30): an absent member has the Grafana default.
    let fill_opacity = optional(&mut obj, "fill_opacity", |f| f.int(cx, 0, 100).and_then(|v| u8::try_from(v).ok()));
    let line_width = optional(&mut obj, "line_width", |f| f.int(cx, 0, 10).and_then(|v| u8::try_from(v).ok()));
    let gradient_mode = optional(&mut obj, "gradient_mode", |f| f.en::<GradientMode>(cx));
    obj.finish(
        cx,
        &[
            "orientation",
            "x_field",
            "stacking",
            "show_values",
            "bar_width",
            "group_width",
            "x_tick_rotation",
            "legend",
            "tooltip",
            "thresholds_style",
            "fill_opacity",
            "line_width",
            "gradient_mode",
        ],
    );
    Some(BarchartOptions {
        orientation: orientation?,
        x_field: x_field?,
        stacking: stacking?,
        show_values: show_values?,
        bar_width: bar_width?,
        group_width: group_width?,
        x_tick_rotation: x_tick_rotation?,
        legend: legend?,
        tooltip: tooltip?,
        thresholds_style: thresholds_style?.unwrap_or(ThresholdsStyle::Off),
        fill_opacity: fill_opacity?.unwrap_or(BAR_FILL_OPACITY),
        line_width: line_width?.unwrap_or(BAR_LINE_WIDTH),
        gradient_mode: gradient_mode?.unwrap_or(GradientMode::None),
    })
}

fn piechart_options(cx: &mut Ctx, f: &Field<'_>) -> Option<PiechartOptions> {
    let mut obj = f.object(cx, "options")?;
    let pie_type = obj.en::<PieType>(cx, "pie_type");
    let reduce = obj.get(cx, "reduce").and_then(|f| reduce(cx, &f));
    let labels = obj
        .get(cx, "labels")
        .and_then(|f| enum_list(cx, &f, 0, 3, &[PieLabel::Name, PieLabel::Value, PieLabel::Percent]));
    let legend = obj.get(cx, "legend").and_then(|f| legend(cx, &f, true));
    let tooltip = obj.get(cx, "tooltip").and_then(|f| tooltip(cx, &f));
    obj.finish(cx, &["pie_type", "reduce", "labels", "legend", "tooltip"]);
    Some(PiechartOptions { pie_type: pie_type?, reduce: reduce?, labels: labels?, legend: legend?, tooltip: tooltip? })
}

fn xychart_options(cx: &mut Ctx, f: &Field<'_>) -> Option<XychartOptions> {
    let mut obj = f.object(cx, "options")?;
    let x_field = obj.get(cx, "x_field").and_then(|f| f.nullable(|f| f.text(cx, VALUE)));
    let y_fields = obj.get(cx, "y_fields").and_then(|f| {
        let items = f.items(cx, 0, MAX_Y_FIELDS, ErrorCode::TooLarge)?;
        collect(items.iter().map(|i| i.text(cx, VALUE)).collect())
    });
    let draw = obj.en::<XyDraw>(cx, "draw");
    let point_size = small(cx, &mut obj, "point_size", 1, 40);
    let line_width = small(cx, &mut obj, "line_width", 0, 10);
    let legend = obj.get(cx, "legend").and_then(|f| legend(cx, &f, false));
    let tooltip = obj.get(cx, "tooltip").and_then(|f| tooltip(cx, &f));
    obj.finish(cx, &["x_field", "y_fields", "draw", "point_size", "line_width", "legend", "tooltip"]);
    Some(XychartOptions {
        x_field: x_field?,
        y_fields: y_fields?,
        draw: draw?,
        point_size: point_size?,
        line_width: line_width?,
        legend: legend?,
        tooltip: tooltip?,
    })
}

fn bargauge_options(cx: &mut Ctx, f: &Field<'_>) -> Option<BargaugeOptions> {
    let mut obj = f.object(cx, "options")?;
    let reduce = obj.get(cx, "reduce").and_then(|f| reduce(cx, &f));
    let orientation = obj.en::<Orientation>(cx, "orientation");
    let display_mode = obj.en::<BarGaugeMode>(cx, "display_mode");
    let show_unfilled = obj.boolean(cx, "show_unfilled");
    obj.finish(cx, &["reduce", "orientation", "display_mode", "show_unfilled"]);
    Some(BargaugeOptions {
        reduce: reduce?,
        orientation: orientation?,
        display_mode: display_mode?,
        show_unfilled: show_unfilled?,
    })
}

/// Reads the `options` of a `gauge` (4.33e). Each member but `reduce` is optional: an absent member has its default.
fn gauge_options(cx: &mut Ctx, f: &Field<'_>) -> Option<GaugeOptions> {
    let mut obj = f.object(cx, "options")?;
    let reduce = obj.get(cx, "reduce").and_then(|f| reduce(cx, &f));
    let orientation = optional(&mut obj, "orientation", |f| f.en::<StatOrientation>(cx));
    let labels = optional(&mut obj, "show_threshold_labels", |f| f.boolean(cx));
    let markers = optional(&mut obj, "show_threshold_markers", |f| f.boolean(cx));
    let neutral = optional(&mut obj, "neutral", |f| f.num(cx));
    let text_size =
        |cx: &mut Ctx, f: &Field<'_>| f.int(cx, 1, i64::from(GAUGE_MAX_TEXT_SIZE)).and_then(|v| u8::try_from(v).ok());
    let title_size = optional(&mut obj, "title_size", |f| text_size(cx, f));
    let value_size = optional(&mut obj, "value_size", |f| text_size(cx, f));
    let sizing = optional(&mut obj, "sizing", |f| f.en::<GaugeSizing>(cx));
    let min_size =
        |cx: &mut Ctx, f: &Field<'_>| f.int(cx, 0, i64::from(GAUGE_MAX_MIN_SIZE)).and_then(|v| u16::try_from(v).ok());
    let min_width = optional(&mut obj, "min_width", |f| min_size(cx, f));
    let min_height = optional(&mut obj, "min_height", |f| min_size(cx, f));
    obj.finish(
        cx,
        &[
            "reduce",
            "orientation",
            "show_threshold_labels",
            "show_threshold_markers",
            "neutral",
            "title_size",
            "value_size",
            "sizing",
            "min_width",
            "min_height",
        ],
    );
    Some(GaugeOptions {
        reduce: reduce?,
        orientation: orientation?.unwrap_or(StatOrientation::Auto),
        show_threshold_labels: labels?.unwrap_or(false),
        show_threshold_markers: markers?.unwrap_or(true),
        neutral: neutral?,
        title_size: title_size?,
        value_size: value_size?,
        sizing: sizing?.unwrap_or(GaugeSizing::Auto),
        min_width: min_width?.unwrap_or(GAUGE_MIN_SIZE),
        min_height: min_height?.unwrap_or(GAUGE_MIN_SIZE),
    })
}

// ------------------------------------------------- state timeline and status history (4.33b)

/// Reads the `legend` of a `state_timeline` or a `status_history` (4.33b, rule 2). The members `show` and
/// `placement` are as in 4.26. The mode MUST be `list`, `values` MUST be empty, and `sort_by` and `sort_desc`
/// MUST be absent: the error is `not_permitted` at the member. The page never reads a refused value.
fn timeline_legend(cx: &mut Ctx, f: &Field<'_>) -> Option<Legend> {
    let mut obj = f.object(cx, "legend")?;
    let show = obj.boolean(cx, "show");
    let placement = obj.en::<LegendPlacement>(cx, "placement");
    let mut ok = true;
    let mode = obj.get(cx, "mode").and_then(|m| {
        let v = m.en::<LegendMode>(cx)?;
        if v != LegendMode::List {
            m.fail(
                cx,
                ErrorCode::NotPermitted,
                "must be list: the legend of this panel type has no table mode (4.33b)",
            );
            ok = false;
        }
        Some(v)
    });
    let values = obj.get(cx, "values").and_then(|v| {
        let items = enum_list(cx, &v, 0, 5, &CALC_LEGEND_VALUES)?;
        if !items.is_empty() {
            v.fail(
                cx,
                ErrorCode::NotPermitted,
                "must be an empty array: the legend of this panel type has no values (4.33b)",
            );
            ok = false;
        }
        Some(items)
    });
    for name in ["sort_by", "sort_desc"] {
        if let Some(m) = obj.opt(name) {
            m.fail(cx, ErrorCode::NotPermitted, "must be absent: the legend of this panel type has no sort (4.33b)");
            ok = false;
        }
    }
    obj.finish(cx, &["show", "placement", "mode", "values", "sort_by", "sort_desc"]);
    if !ok {
        return None;
    }
    Some(Legend { show: show?, placement: placement?, mode: mode?, values: values?, sort_by: None, sort_desc: false })
}

/// Reads the `tooltip` of a `state_timeline` or a `status_history` (4.33b, rule 3). The member `sort` MUST be
/// absent or `none`. The member `hide_zeros` MUST be absent or `false`. Else the error is `not_permitted`.
fn timeline_tooltip(cx: &mut Ctx, f: &Field<'_>) -> Option<Tooltip> {
    reduced_tooltip(cx, f, "4.33b")
}

/// Reads a `tooltip` that has no sort and no hide-zeros: the one of a timeline (4.33b, rule 3) and the one of
/// a `heatmap` (4.33c, rule 4). `section` is the section of the rule, for the message.
fn reduced_tooltip(cx: &mut Ctx, f: &Field<'_>, section: &str) -> Option<Tooltip> {
    let mut obj = f.object(cx, "tooltip")?;
    let mode = obj.en::<TooltipMode>(cx, "mode");
    let mut ok = true;
    if let Some(m) = obj.opt("sort") {
        match m.en::<TooltipSort>(cx) {
            Some(TooltipSort::None) => {}
            Some(_) => {
                m.fail(
                    cx,
                    ErrorCode::NotPermitted,
                    &format!("must be none or absent: the tooltip of this panel type has no sort ({section})"),
                );
                ok = false;
            }
            None => ok = false,
        }
    }
    if let Some(m) = obj.opt("hide_zeros") {
        match m.boolean(cx) {
            Some(false) => {}
            Some(true) => {
                m.fail(
                    cx,
                    ErrorCode::NotPermitted,
                    &format!("must be false or absent: the tooltip of this panel type has no hide-zeros ({section})"),
                );
                ok = false;
            }
            None => ok = false,
        }
    }
    obj.finish(cx, &["mode", "sort", "hide_zeros"]);
    if !ok {
        return None;
    }
    Some(Tooltip { mode: mode?, sort: TooltipSort::None, hide_zeros: false })
}

fn state_timeline_options(cx: &mut Ctx, f: &Field<'_>) -> Option<StateTimelineOptions> {
    let mut obj = f.object(cx, "options")?;
    let merge_values = obj.boolean(cx, "merge_values");
    let show_values = obj.en::<ShowValues>(cx, "show_values");
    let align_values = obj.en::<AlignValues>(cx, "align_values");
    let row_height = small(cx, &mut obj, "row_height", 10, 100);
    let line_width = small(cx, &mut obj, "line_width", 0, 10);
    let fill_opacity = small(cx, &mut obj, "fill_opacity", 0, 100);
    let span_nulls = obj.boolean(cx, "span_nulls");
    let show_labels = obj.boolean(cx, "show_labels");
    let legend = obj.get(cx, "legend").and_then(|f| timeline_legend(cx, &f));
    let tooltip = obj.get(cx, "tooltip").and_then(|f| timeline_tooltip(cx, &f));
    obj.finish(
        cx,
        &[
            "merge_values",
            "show_values",
            "align_values",
            "row_height",
            "line_width",
            "fill_opacity",
            "span_nulls",
            "show_labels",
            "legend",
            "tooltip",
        ],
    );
    Some(StateTimelineOptions {
        merge_values: merge_values?,
        show_values: show_values?,
        align_values: align_values?,
        row_height: row_height?,
        line_width: line_width?,
        fill_opacity: fill_opacity?,
        span_nulls: span_nulls?,
        show_labels: show_labels?,
        legend: legend?,
        tooltip: tooltip?,
    })
}

fn status_history_options(cx: &mut Ctx, f: &Field<'_>) -> Option<StatusHistoryOptions> {
    let mut obj = f.object(cx, "options")?;
    let show_values = obj.en::<ShowValues>(cx, "show_values");
    let row_height = small(cx, &mut obj, "row_height", 10, 100);
    let col_width = small(cx, &mut obj, "col_width", 10, 100);
    let line_width = small(cx, &mut obj, "line_width", 0, 10);
    let fill_opacity = small(cx, &mut obj, "fill_opacity", 0, 100);
    let show_labels = obj.boolean(cx, "show_labels");
    let legend = obj.get(cx, "legend").and_then(|f| timeline_legend(cx, &f));
    let tooltip = obj.get(cx, "tooltip").and_then(|f| timeline_tooltip(cx, &f));
    obj.finish(
        cx,
        &["show_values", "row_height", "col_width", "line_width", "fill_opacity", "show_labels", "legend", "tooltip"],
    );
    Some(StatusHistoryOptions {
        show_values: show_values?,
        row_height: row_height?,
        col_width: col_width?,
        line_width: line_width?,
        fill_opacity: fill_opacity?,
        show_labels: show_labels?,
        legend: legend?,
        tooltip: tooltip?,
    })
}

// ------------------------------------------------------------ heatmap and histogram (4.33c, 4.33d)

/// Reads an integer member of a bucket object (4.33c). A value of another type is `wrong_type`. A number that is
/// not an integer, or that is out of `min..=max`, is `out_of_range` (4.34).
fn bucket_int(cx: &mut Ctx, f: &Field<'_>, min: i64, max: i64) -> Option<i64> {
    let Json::Num(n) = f.json else {
        // A `null` is a value of the right kind that the mode does not permit.
        let code = if f.is_null() { ErrorCode::OutOfRange } else { ErrorCode::WrongType };
        f.fail(cx, code, &format!("must be an integer from {min} to {max}"));
        return None;
    };
    match n.as_i64() {
        Some(v) if (min..=max).contains(&v) => Some(v),
        _ => {
            f.fail(cx, ErrorCode::OutOfRange, &format!("must be an integer from {min} to {max}"));
            None
        }
    }
}

/// Reads `x_buckets` of a `heatmap` (4.33c). The value of the mode `count` is `null` or 1 to 1,000. The value of
/// the mode `size` is 1 to 31,536,000,000 milliseconds.
fn heat_x_buckets(cx: &mut Ctx, f: &Field<'_>) -> Option<HeatXBuckets> {
    let mut obj = f.object(cx, "x_buckets")?;
    let mode = obj.en::<BucketMode>(cx, "mode");
    let value_field = obj.get(cx, "value");
    obj.finish(cx, &["mode", "value"]);
    let mode = mode?;
    let value_field = value_field?;
    let value = match mode {
        BucketMode::Count => value_field.nullable(|f| bucket_int(cx, f, 1, 1000))?,
        BucketMode::Size => Some(bucket_int(cx, &value_field, 1, 31_536_000_000)?),
    };
    Some(HeatXBuckets { mode, value })
}

/// Reads `y_buckets` of a `heatmap` (4.33c). It gives the buckets and the scale. The scale is `Some` when the
/// member `scale` read, also when another member did not. A log scale MUST have the mode `size`. The value of a
/// log scale is the split: `null` or 1 to 16. The value of a linear scale is `null` or 1 to 1,000 for `count`,
/// and a number above 0 for `size`.
fn heat_y_buckets(cx: &mut Ctx, f: &Field<'_>) -> (Option<HeatYBuckets>, Option<HeatScale>) {
    let Some(mut obj) = f.object(cx, "y_buckets") else { return (None, None) };
    let mode_field = obj.get(cx, "mode");
    let mode = mode_field.as_ref().and_then(|m| m.en::<BucketMode>(cx));
    let value_field = obj.get(cx, "value");
    let scale = obj.en::<HeatScale>(cx, "scale");
    obj.finish(cx, &["mode", "value", "scale"]);
    let mut ok = true;
    if let (Some(BucketMode::Count), Some(HeatScale::Log2 | HeatScale::Log10), Some(m)) = (mode, scale, &mode_field) {
        m.fail(cx, ErrorCode::NotPermitted, "must be size: a log scale has no mode count (4.33c, rule 2)");
        ok = false;
    }
    let value = match (&value_field, scale, mode) {
        (Some(v), Some(HeatScale::Log2 | HeatScale::Log10), _) => {
            v.nullable(|f| bucket_int(cx, f, 1, 16).map(|n| n as f64))
        }
        (Some(v), Some(HeatScale::Linear), Some(BucketMode::Count)) => {
            v.nullable(|f| bucket_int(cx, f, 1, 1000).map(|n| n as f64))
        }
        (Some(v), Some(HeatScale::Linear), Some(BucketMode::Size)) if v.is_null() => {
            v.fail(cx, ErrorCode::OutOfRange, "must be a number above 0 with the mode size");
            None
        }
        (Some(v), Some(HeatScale::Linear), Some(BucketMode::Size)) => v.num(cx).and_then(|n| {
            if n > 0.0 {
                Some(Some(n))
            } else {
                v.fail(cx, ErrorCode::OutOfRange, "must be a number above 0");
                None
            }
        }),
        _ => None,
    };
    if value.is_none() {
        ok = false;
    }
    let buckets = match (ok, mode, value, scale) {
        (true, Some(mode), Some(value), Some(scale)) => Some(HeatYBuckets { mode, value, scale }),
        _ => None,
    };
    (buckets, scale)
}

/// Reads a member that is a number or `null`.
fn nullable_number(cx: &mut Ctx, f: &Field<'_>) -> Option<Option<f64>> {
    f.nullable(|f| f.num(cx))
}

/// Reads `color` of a `heatmap` (4.33c). All members are necessary. `exponent` is 0.1 to 2 with one decimal at
/// most. When `min` and `max` are numbers, `min` MUST be lower than `max`.
fn heat_color(cx: &mut Ctx, f: &Field<'_>) -> Option<HeatColor> {
    let mut obj = f.object(cx, "color")?;
    let mode = obj.en::<HeatColorMode>(cx, "mode");
    let scheme = obj.en::<HeatScheme>(cx, "scheme");
    let fill = obj.get(cx, "fill").and_then(|f| colour(cx, &f));
    let scale = obj.en::<HeatColorScale>(cx, "scale");
    let exponent = obj.get(cx, "exponent").and_then(|f| {
        let v = f.num(cx)?;
        let tenths = v * 10.0;
        if (0.1..=2.0).contains(&v) && (tenths - tenths.round()).abs() < 1e-9 {
            Some(v)
        } else {
            f.fail(cx, ErrorCode::OutOfRange, "must be 0.1 to 2 with one decimal at most");
            None
        }
    });
    let steps = obj.int(cx, "steps", 2, 128).and_then(|v| u8::try_from(v).ok());
    let reverse = obj.boolean(cx, "reverse");
    let min = obj.get(cx, "min").and_then(|f| nullable_number(cx, &f));
    let max_field = obj.get(cx, "max");
    let max = max_field.as_ref().and_then(|f| nullable_number(cx, f));
    obj.finish(cx, &["mode", "scheme", "fill", "scale", "exponent", "steps", "reverse", "min", "max"]);
    let mut ok = true;
    if let (Some(Some(lo)), Some(Some(hi)), Some(m)) = (min, max, &max_field)
        && lo >= hi
    {
        m.fail(cx, ErrorCode::OutOfRange, &format!("must be greater than min ({lo}) (4.33c, rule 3)"));
        ok = false;
    }
    if !ok {
        return None;
    }
    Some(HeatColor {
        mode: mode?,
        scheme: scheme?,
        fill: fill?,
        scale: scale?,
        exponent: exponent?,
        steps: steps?,
        reverse: reverse?,
        min: min?,
        max: max?,
    })
}

/// Reads the `options` of a `heatmap` (4.33c). A log scale refuses `y_min` and `y_max` at or below 0.
fn heatmap_options(cx: &mut Ctx, f: &Field<'_>) -> Option<HeatmapOptions> {
    let mut obj = f.object(cx, "options")?;
    let calculate = obj.boolean(cx, "calculate");
    let x_buckets = obj.get(cx, "x_buckets").and_then(|f| heat_x_buckets(cx, &f));
    let (y_buckets, scale) = match obj.get(cx, "y_buckets") {
        Some(f) => heat_y_buckets(cx, &f),
        None => (None, None),
    };
    let y_axis = obj.en::<HeatYAxis>(cx, "y_axis");
    let y_reverse = obj.boolean(cx, "y_reverse");
    let y_min_field = obj.get(cx, "y_min");
    let y_min = y_min_field.as_ref().and_then(|f| nullable_number(cx, f));
    let y_max_field = obj.get(cx, "y_max");
    let y_max = y_max_field.as_ref().and_then(|f| nullable_number(cx, f));
    let color = obj.get(cx, "color").and_then(|f| heat_color(cx, &f));
    let cell_gap = small(cx, &mut obj, "cell_gap", 0, 25);
    let hide_le = obj.get(cx, "hide_le").and_then(|f| nullable_number(cx, &f));
    let hide_ge = obj.get(cx, "hide_ge").and_then(|f| nullable_number(cx, &f));
    let show_legend = obj.boolean(cx, "show_legend");
    let tooltip = obj.get(cx, "tooltip").and_then(|f| reduced_tooltip(cx, &f, "4.33c"));
    obj.finish(
        cx,
        &[
            "calculate",
            "x_buckets",
            "y_buckets",
            "y_axis",
            "y_reverse",
            "y_min",
            "y_max",
            "color",
            "cell_gap",
            "hide_le",
            "hide_ge",
            "show_legend",
            "tooltip",
        ],
    );
    let mut ok = true;
    if matches!(scale, Some(HeatScale::Log2 | HeatScale::Log10)) {
        for (field, value) in [(&y_min_field, y_min), (&y_max_field, y_max)] {
            if let (Some(field), Some(Some(v))) = (field, value)
                && v <= 0.0
            {
                field.fail(cx, ErrorCode::OutOfRange, "must be above 0 with a log scale (4.34)");
                ok = false;
            }
        }
    }
    if !ok {
        return None;
    }
    Some(HeatmapOptions {
        calculate: calculate?,
        x_buckets: x_buckets?,
        y_buckets: y_buckets?,
        y_axis: y_axis?,
        y_reverse: y_reverse?,
        y_min: y_min?,
        y_max: y_max?,
        color: color?,
        cell_gap: cell_gap?,
        hide_le: hide_le?,
        hide_ge: hide_ge?,
        show_legend: show_legend?,
        tooltip: tooltip?,
    })
}

/// Reads the `options` of a `histogram` (4.33d). The legend and the tooltip have all their members (4.26).
fn histogram_options(cx: &mut Ctx, f: &Field<'_>) -> Option<HistogramOptions> {
    let mut obj = f.object(cx, "options")?;
    let bucket_count = obj.int(cx, "bucket_count", 1, 1000).and_then(|v| u16::try_from(v).ok());
    let bucket_size = obj.get(cx, "bucket_size").and_then(|f| {
        f.nullable(|f| {
            let v = f.num(cx)?;
            if v > 0.0 {
                Some(v)
            } else {
                f.fail(cx, ErrorCode::OutOfRange, "must be a number above 0 or null");
                None
            }
        })
    });
    let bucket_offset = obj.num(cx, "bucket_offset");
    let combine = obj.boolean(cx, "combine");
    let stacking = obj.en::<Stacking>(cx, "stacking");
    let line_width = small(cx, &mut obj, "line_width", 0, 10);
    let fill_opacity = small(cx, &mut obj, "fill_opacity", 0, 100);
    let legend = obj.get(cx, "legend").and_then(|f| legend(cx, &f, false));
    let tooltip = obj.get(cx, "tooltip").and_then(|f| tooltip(cx, &f));
    obj.finish(
        cx,
        &[
            "bucket_count",
            "bucket_size",
            "bucket_offset",
            "combine",
            "stacking",
            "line_width",
            "fill_opacity",
            "legend",
            "tooltip",
        ],
    );
    Some(HistogramOptions {
        bucket_count: bucket_count?,
        bucket_size: bucket_size?,
        bucket_offset: bucket_offset?,
        combine: combine?,
        stacking: stacking?,
        line_width: line_width?,
        fill_opacity: fill_opacity?,
        legend: legend?,
        tooltip: tooltip?,
    })
}
