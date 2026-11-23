// SPDX-License-Identifier: MIT OR Apache-2.0
//! The canonical writer (4.38): one byte sequence for each document.

use super::model::*;
use super::{FOLDER_FORMAT_VERSION, FORMAT_VERSION};

/// A JSON value ready to write. An object keeps its members in the order to write.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Out {
    Null,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(String),
    Arr(Vec<Out>),
    Obj(Vec<(&'static str, Out)>),
}

/// Writes a value in the canonical layout, with one U+000A at the end (4.38, rules 1 to 11).
///
/// The writer measures the length first, then fills one buffer of this exact length.
pub(crate) fn write(out: &Out) -> Vec<u8> {
    write_sized(out, length(out))
}

/// Gives the length in bytes of the canonical text of a value, with no buffer. The validation compares it with
/// `MAX_DOCUMENT_BYTES` before it writes (4.36, step 8), so a document that is too large allocates no text.
pub(crate) fn length(out: &Out) -> usize {
    let mut counter = Counter(0);
    write_value(out, 0, &mut counter);
    counter.0.saturating_add(1)
}

/// Writes a value whose canonical text has `len` bytes, which [`length`] gives.
pub(crate) fn write_sized(out: &Out, len: usize) -> Vec<u8> {
    let mut buf = String::with_capacity(len);
    write_value(out, 0, &mut buf);
    buf.push('\n');
    debug_assert_eq!(buf.len(), len);
    buf.into_bytes()
}

/// The target of the writer: a text, or a count of bytes. One writer serves both, so the length that
/// [`length`] gives is the length of the text.
trait Sink {
    fn put(&mut self, s: &str);
    fn put_char(&mut self, c: char);
}

impl Sink for String {
    fn put(&mut self, s: &str) {
        self.push_str(s);
    }

    fn put_char(&mut self, c: char) {
        self.push(c);
    }
}

/// A sink that counts the bytes and keeps no text.
struct Counter(usize);

impl Sink for Counter {
    fn put(&mut self, s: &str) {
        self.0 = self.0.saturating_add(s.len());
    }

    fn put_char(&mut self, c: char) {
        self.0 = self.0.saturating_add(c.len_utf8());
    }
}

fn indent(level: usize, buf: &mut impl Sink) {
    for _ in 0..level {
        buf.put("  ");
    }
}

fn write_value(out: &Out, level: usize, buf: &mut impl Sink) {
    match out {
        Out::Null => buf.put("null"),
        Out::Bool(b) => buf.put(if *b { "true" } else { "false" }),
        Out::Int(i) => buf.put(&i.to_string()),
        Out::Num(f) => buf.put(&number_text(*f)),
        Out::Str(s) => write_string(s, buf),
        Out::Arr(items) if items.is_empty() => buf.put("[]"),
        Out::Arr(items) => {
            buf.put("[\n");
            for (i, item) in items.iter().enumerate() {
                indent(level + 1, buf);
                write_value(item, level + 1, buf);
                buf.put(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            indent(level, buf);
            buf.put_char(']');
        }
        Out::Obj(members) if members.is_empty() => buf.put("{}"),
        Out::Obj(members) => {
            buf.put("{\n");
            for (i, (name, value)) in members.iter().enumerate() {
                indent(level + 1, buf);
                write_string(name, buf);
                buf.put(": ");
                write_value(value, level + 1, buf);
                buf.put(if i + 1 < members.len() { ",\n" } else { "\n" });
            }
            indent(level, buf);
            buf.put_char('}');
        }
    }
}

/// Writes a string with the escapes of rule 7.
fn write_string(s: &str, buf: &mut impl Sink) {
    buf.put_char('"');
    if !s.bytes().any(|b| b == b'"' || b == b'\\' || b <= 0x1f) {
        // No character needs an escape: the text goes as it is.
        buf.put(s);
        buf.put_char('"');
        return;
    }
    for c in s.chars() {
        match c {
            '"' => buf.put("\\\""),
            '\\' => buf.put("\\\\"),
            '\u{8}' => buf.put("\\b"),
            '\u{c}' => buf.put("\\f"),
            '\n' => buf.put("\\n"),
            '\r' => buf.put("\\r"),
            '\t' => buf.put("\\t"),
            c if c <= '\u{1f}' => buf.put(&format!("\\u{:04x}", c as u32)),
            c => buf.put_char(c),
        }
    }
    buf.put_char('"');
}

/// Gives the text of a finite double in the form of RFC 8785, section 3.2.2.3 (4.38, rule 9).
///
/// The digits and the exponent come from `ryu`. The value `-0` gives `0`. A value that is not
/// finite gives `0`: the validation refuses it before the writer runs.
pub(crate) fn number_text(value: f64) -> String {
    if !value.is_finite() || value == 0.0 {
        return "0".to_owned();
    }
    let mut buffer = ryu::Buffer::new();
    let raw = buffer.format_finite(value.abs());
    let (mantissa, exponent) = match raw.split_once('e') {
        Some((m, e)) => (m, e.parse::<i32>().unwrap_or(0)),
        None => (raw, 0),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let all: String = format!("{int_part}{frac_part}");
    let leading = all.len() - all.trim_start_matches('0').len();
    let digits = all.trim_start_matches('0').trim_end_matches('0');
    // The value is 0.<digits> times 10^n, in the notation of ECMAScript: n is the position of
    // the decimal point after the first digit.
    let n = i32::try_from(int_part.len()).unwrap_or(0) + exponent - i32::try_from(leading).unwrap_or(0);
    let k = i32::try_from(digits.len()).unwrap_or(0);
    let sign = if value < 0.0 { "-" } else { "" };
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat(usize::try_from(n - k).unwrap_or(0)))
    } else if 0 < n && n <= 21 {
        let (a, b) = digits.split_at(usize::try_from(n).unwrap_or(0));
        format!("{a}.{b}")
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat(usize::try_from(-n).unwrap_or(0)))
    } else {
        let e = n - 1;
        let esign = if e < 0 { '-' } else { '+' };
        let (first, rest) = digits.split_at(1);
        let mant = if rest.is_empty() { first.to_owned() } else { format!("{first}.{rest}") };
        format!("{mant}e{esign}{}", e.abs())
    };
    format!("{sign}{body}")
}

// ------------------------------------------------------------ model to Out

fn s(v: &str) -> Out {
    Out::Str(v.to_owned())
}

fn opt_s(v: &Option<String>) -> Out {
    v.as_deref().map_or(Out::Null, s)
}

fn opt_n(v: Option<f64>) -> Out {
    v.map_or(Out::Null, Out::Num)
}

fn e<T: TextEnum>(v: T) -> Out {
    Out::Str(v.as_str().to_owned())
}

fn list<T>(items: &[T], f: impl Fn(&T) -> Out) -> Out {
    Out::Arr(items.iter().map(f).collect())
}

fn lines(v: &[String]) -> Out {
    list(v, |x| s(x))
}

/// Converts a document to the value that the writer writes.
pub(crate) fn document_out(d: &Document) -> Out {
    let mut members = vec![
        ("format_version", Out::Int(i64::from(FORMAT_VERSION))),
        ("uid", s(d.uid.as_str())),
        ("title", s(&d.title)),
        ("description", s(&d.description)),
        ("tags", lines(&d.tags)),
        (
            "time",
            Out::Obj(vec![
                ("from", s(&d.time.from)),
                ("to", s(&d.time.to)),
                ("timezone", s(d.time.timezone.as_str())),
                ("refresh", opt_s(&d.time.refresh)),
            ]),
        ),
        ("cursor_sync", e(d.cursor_sync)),
        ("links", list(&d.links, dashboard_link)),
        ("variables", list(&d.variables, variable)),
    ];
    // The member `annotations` comes after `variables`. The writer leaves it out when it is empty (4.38).
    if !d.annotations.is_empty() {
        members.push(("annotations", list(&d.annotations, annotation)));
    }
    members.push(("sections", list(&d.sections, section)));
    Out::Obj(members)
}

/// The members of an annotation come in the order of 4.5b. `enabled` and `panels` are left out at their default.
fn annotation(a: &Annotation) -> Out {
    let mut m =
        vec![("name", s(&a.name)), ("source", s(a.source.as_str())), ("sql", lines(&a.sql)), ("color", s(&a.color))];
    if !a.enabled {
        m.push(("enabled", Out::Bool(false)));
    }
    if let Some(panels) = &a.panels {
        m.push(("panels", list(panels, |id| Out::Int(i64::from(*id)))));
    }
    Out::Obj(m)
}

/// Converts the value of a document of format 3 to the value of the same document in format 2.
///
/// A document that the migration 1 to 2 makes has no member of format 3. Its format 2 form
/// differs from its format 3 form by the version number and by the member `links` only (4.42).
pub(crate) fn to_format_2(out: Out) -> Out {
    let Out::Obj(members) = out else { return out };
    Out::Obj(
        members
            .into_iter()
            .filter(|(name, _)| *name != "links")
            .map(|(name, value)| if name == "format_version" { (name, Out::Int(2)) } else { (name, value) })
            .collect(),
    )
}

/// Converts a folder file to the value that the writer writes (4.41).
pub(crate) fn folder_out(title: &str) -> Out {
    Out::Obj(vec![("format_version", Out::Int(i64::from(FOLDER_FORMAT_VERSION))), ("title", s(title))])
}

fn dashboard_link(l: &DashboardLink) -> Out {
    match l {
        DashboardLink::Link { title, url, target_blank, include_vars, keep_time } => Out::Obj(vec![
            ("kind", e(l.kind())),
            ("title", s(title)),
            ("url", s(url)),
            ("target_blank", Out::Bool(*target_blank)),
            ("include_vars", Out::Bool(*include_vars)),
            ("keep_time", Out::Bool(*keep_time)),
        ]),
        DashboardLink::Dashboards { title, tags, as_dropdown, target_blank, include_vars, keep_time } => {
            Out::Obj(vec![
                ("kind", e(l.kind())),
                ("title", s(title)),
                ("tags", lines(tags)),
                ("as_dropdown", Out::Bool(*as_dropdown)),
                ("target_blank", Out::Bool(*target_blank)),
                ("include_vars", Out::Bool(*include_vars)),
                ("keep_time", Out::Bool(*keep_time)),
            ])
        }
    }
}

fn variable(v: &Variable) -> Out {
    let mut m = vec![("kind", e(v.kind.tag())), ("name", s(&v.name)), ("label", opt_s(&v.label)), ("hide", e(v.hide))];
    match &v.kind {
        VariableKind::Query {
            source,
            sql,
            multi,
            include_all,
            all_value,
            refresh,
            sort,
            regex,
            default,
            allow_custom_value,
        } => {
            m.push(("source", s(source.as_str())));
            m.push(("sql", lines(sql)));
            m.push(("multi", Out::Bool(*multi)));
            m.push(("include_all", Out::Bool(*include_all)));
            m.push(("all_value", opt_s(all_value)));
            m.push(("refresh", e(*refresh)));
            m.push(("sort", e(*sort)));
            m.push(("regex", opt_s(regex)));
            m.push(("default", lines(default)));
            push_open(&mut m, *allow_custom_value);
        }
        VariableKind::Custom { options, multi, include_all, all_value, default, allow_custom_value } => {
            m.push(("options", list(options, |o| Out::Obj(vec![("text", s(&o.text)), ("value", s(&o.value))]))));
            m.push(("multi", Out::Bool(*multi)));
            m.push(("include_all", Out::Bool(*include_all)));
            m.push(("all_value", opt_s(all_value)));
            m.push(("default", lines(default)));
            push_open(&mut m, *allow_custom_value);
        }
        VariableKind::Text { value_type, default } => {
            m.push(("value_type", e(*value_type)));
            m.push(("default", s(default)));
        }
        VariableKind::Interval { intervals, auto, auto_count, auto_min, default } => {
            m.push(("intervals", lines(intervals)));
            m.push(("auto", Out::Bool(*auto)));
            m.push(("auto_count", Out::Int(i64::from(*auto_count))));
            m.push(("auto_min", s(auto_min)));
            m.push(("default", opt_s(default)));
        }
        VariableKind::Constant { value } => m.push(("value", s(value))),
        VariableKind::Adhoc { source, keys } => {
            m.push(("source", s(source.as_str())));
            m.push(("keys", lines(keys)));
        }
    }
    if let Some(p) = &v.placement {
        let mut pm = vec![
            ("section", Out::Int(i64::from(p.section))),
            ("x", Out::Int(i64::from(p.grid.x))),
            ("y", Out::Int(i64::from(p.grid.y))),
            ("w", Out::Int(i64::from(p.grid.w))),
            ("h", Out::Int(i64::from(p.grid.h))),
        ];
        if let Some(t) = &p.title {
            pm.push(("title", s(t)));
        }
        m.push(("placement", Out::Obj(pm)));
    }
    Out::Obj(m)
}

/// Writes `allow_custom_value` after `default`. The writer leaves out `false` (4.38).
fn push_open(m: &mut Vec<(&'static str, Out)>, open: bool) {
    if open {
        m.push(("allow_custom_value", Out::Bool(true)));
    }
}

fn section(v: &Section) -> Out {
    let mut m =
        vec![("title", opt_s(&v.title)), ("collapsed", Out::Bool(v.collapsed)), ("panels", list(&v.panels, panel))];
    if let Some(r) = &v.repeat {
        m.push(("repeat", Out::Obj(vec![("variable", s(&r.variable))])));
    }
    Out::Obj(m)
}

/// The member `repeat` of a panel. `max_per_row` is written as the document gives it (4.13a, rule 4).
fn panel_repeat(r: &PanelRepeat) -> Out {
    let mut m = vec![("variable", s(&r.variable)), ("direction", e(r.direction))];
    if let Some(n) = r.max_per_row {
        m.push(("max_per_row", Out::Int(i64::from(n))));
    }
    Out::Obj(m)
}

fn panel(p: &Panel) -> Out {
    let mut m = panel_members(p);
    if let Some(r) = &p.repeat {
        m.push(("repeat", panel_repeat(r)));
    }
    Out::Obj(m)
}

fn panel_members(p: &Panel) -> Vec<(&'static str, Out)> {
    let head = panel_head(p);
    let tail = [("options", options(&p.options)), ("links", list(&p.links, link))];
    if p.panel_type == PanelType::Text {
        // A text panel has no `data`, no `transforms` and no `fields` (4.38).
        return head.into_iter().chain(tail).collect();
    }
    let query = [
        ("data", data(&p.data)),
        ("transforms", list(&p.transforms, transform)),
        (
            "fields",
            Out::Obj(vec![
                ("defaults", property_set(&p.fields.defaults)),
                ("overrides", list(&p.fields.overrides, field_override)),
            ]),
        ),
    ];
    head.into_iter().chain(query).chain(tail).collect()
}

/// The members of a panel that come before `data`: they are the same for each type (4.38).
fn panel_head(p: &Panel) -> Vec<(&'static str, Out)> {
    vec![
        ("id", Out::Int(i64::from(p.id))),
        ("type", e(p.panel_type)),
        ("title", s(&p.title)),
        ("description", s(&p.description)),
        ("transparent", Out::Bool(p.transparent)),
        (
            "grid",
            Out::Obj(vec![
                ("x", Out::Int(i64::from(p.grid.x))),
                ("y", Out::Int(i64::from(p.grid.y))),
                ("w", Out::Int(i64::from(p.grid.w))),
                ("h", Out::Int(i64::from(p.grid.h))),
            ]),
        ),
    ]
}

fn data(d: &PanelData) -> Out {
    match d {
        PanelData::Query(q) => {
            let mut m = vec![
                ("source", s(q.source.as_str())),
                ("format", e(q.format)),
                ("sql", lines(&q.sql)),
                ("min_interval", opt_s(&q.min_interval)),
                ("max_data_points", q.max_data_points.map_or(Out::Null, |v| Out::Int(i64::from(v)))),
            ];
            if let Some(v) = &q.time_from {
                m.push(("time_from", s(v)));
            }
            if let Some(v) = &q.time_shift {
                m.push(("time_shift", s(v)));
            }
            if q.hide_time_override {
                m.push(("hide_time_override", Out::Bool(true)));
            }
            // `ref` and `hide` are the last members of `data.query`, after `hide_time_override` (4.38).
            if q.query_ref != DEFAULT_QUERY_REF {
                m.push(("ref", s(&q.query_ref)));
            }
            if q.hide {
                m.push(("hide", Out::Bool(true)));
            }
            let mut members = vec![("query", Out::Obj(m))];
            if !q.extra_queries.is_empty() {
                members.push(("extra_queries", list(&q.extra_queries, extra_query)));
            }
            Out::Obj(members)
        }
        // A text panel has no member `data`: `panel_members` never calls this function for it.
        PanelData::None => Out::Obj(vec![]),
        PanelData::Reuse { panel, with_transforms } => Out::Obj(vec![(
            "reuse",
            Out::Obj(vec![("panel", Out::Int(i64::from(*panel))), ("with_transforms", Out::Bool(*with_transforms))]),
        )]),
    }
}

fn link(l: &Link) -> Out {
    Out::Obj(vec![("title", s(&l.title)), ("url", s(&l.url)), ("target_blank", Out::Bool(l.target_blank))])
}

fn field_override(o: &Override) -> Out {
    Out::Obj(vec![("match", matcher(&o.matcher)), ("set", property_set(&o.set))])
}

/// The members of an item of `extra_queries` come in the order `ref`, `source`, `format`, `sql`, `hide` (4.38).
fn extra_query(q: &ExtraQuery) -> Out {
    let mut m = vec![
        ("ref", s(&q.query_ref)),
        ("source", s(q.source.as_str())),
        ("format", e(q.format)),
        ("sql", lines(&q.sql)),
    ];
    if q.hide {
        m.push(("hide", Out::Bool(true)));
    }
    Out::Obj(m)
}

fn matcher(m: &FieldMatcher) -> Out {
    match m {
        FieldMatcher::Name(v) | FieldMatcher::Regex(v) | FieldMatcher::Query(v) => {
            Out::Obj(vec![("by", e(m.by())), ("value", s(v))])
        }
        FieldMatcher::Names { names, mode } => {
            Out::Obj(vec![("by", e(m.by())), ("names", lines(names)), ("mode", e(*mode))])
        }
    }
}

fn property_set(p: &PropertySet) -> Out {
    let mut m: Vec<(&'static str, Out)> = Vec::new();
    if let Some(v) = &p.unit {
        m.push(("unit", s(v)));
    }
    if let Some(v) = p.decimals {
        m.push(("decimals", Out::Int(i64::from(v))));
    }
    if let Some(v) = p.min {
        m.push(("min", Out::Num(v)));
    }
    if let Some(v) = p.max {
        m.push(("max", Out::Num(v)));
    }
    if let Some(v) = &p.color {
        m.push(("color", Out::Obj(vec![("mode", e(v.mode)), ("fixed", opt_s(&v.fixed))])));
    }
    if let Some(v) = &p.thresholds {
        m.push((
            "thresholds",
            Out::Obj(vec![
                ("mode", e(v.mode)),
                ("steps", list(&v.steps, |st| Out::Obj(vec![("value", opt_n(st.value)), ("color", s(&st.color))]))),
            ]),
        ));
    }
    if let Some(v) = &p.mappings {
        m.push(("mappings", list(v, mapping)));
    }
    if let Some(v) = &p.links {
        m.push(("links", list(v, link)));
    }
    if let Some(v) = &p.display_name {
        m.push(("display_name", s(v)));
    }
    if let Some(v) = &p.no_value {
        m.push(("no_value", s(v)));
    }
    if let Some(v) = p.width {
        m.push(("width", Out::Int(i64::from(v))));
    }
    if let Some(v) = p.align {
        m.push(("align", e(v)));
    }
    if let Some(v) = &p.cell {
        m.push(("cell", Out::Obj(vec![("type", e(v.cell_type)), ("mode", v.mode.map_or(Out::Null, e))])));
    }
    if let Some(v) = p.filterable {
        m.push(("filterable", Out::Bool(v)));
    }
    if let Some(v) = p.hidden {
        m.push(("hidden", Out::Bool(v)));
    }
    if let Some(v) = &p.footer {
        m.push(("footer", Out::Obj(vec![("reducers", list(&v.reducers, |c| e(*c)))])));
    }
    if let Some(v) = p.inspect {
        m.push(("inspect", Out::Bool(v)));
    }
    if let Some(v) = &p.hide_from {
        m.push((
            "hide_from",
            Out::Obj(vec![
                ("legend", Out::Bool(v.legend)),
                ("tooltip", Out::Bool(v.tooltip)),
                ("viz", Out::Bool(v.viz)),
            ]),
        ));
    }
    if let Some(v) = p.axis_placement {
        m.push(("axis_placement", e(v)));
    }
    if let Some(v) = &p.axis_label {
        m.push(("axis_label", s(v)));
    }
    if let Some(v) = p.soft_min {
        m.push(("soft_min", Out::Num(v)));
    }
    if let Some(v) = p.soft_max {
        m.push(("soft_max", Out::Num(v)));
    }
    if let Some(v) = p.axis_grid {
        m.push(("axis_grid", Out::Bool(v)));
    }
    if let Some(v) = p.draw {
        m.push(("draw", e(v)));
    }
    if let Some(v) = p.line_width {
        m.push(("line_width", Out::Int(i64::from(v))));
    }
    if let Some(v) = p.fill_opacity {
        m.push(("fill_opacity", Out::Int(i64::from(v))));
    }
    if let Some(v) = p.line_dash {
        m.push(("line_dash", e(v)));
    }
    if let Some(v) = p.show_points {
        m.push(("show_points", e(v)));
    }
    if let Some(v) = p.point_size {
        m.push(("point_size", Out::Int(i64::from(v))));
    }
    if let Some(v) = p.transform {
        m.push(("transform", e(v)));
    }
    if let Some(v) = &p.line_dash_array {
        m.push(("line_dash_array", list(v, |d| Out::Int(i64::from(*d)))));
    }
    if let Some(v) = p.axis_centered_zero {
        m.push(("axis_centered_zero", Out::Bool(v)));
    }
    if let Some(v) = p.axis_width {
        m.push(("axis_width", Out::Int(i64::from(v))));
    }
    if let Some(v) = p.axis_border {
        m.push(("axis_border", Out::Bool(v)));
    }
    if let Some(v) = p.axis_color {
        m.push(("axis_color", e(v)));
    }
    Out::Obj(m)
}

fn mapping(m: &Mapping) -> Out {
    let (head, text, color): (Vec<(&'static str, Out)>, &Option<String>, &Option<String>) = match m {
        Mapping::Value { value, text, color } => (vec![("kind", s("value")), ("value", s(value))], text, color),
        Mapping::Range { from, to, text, color } => {
            (vec![("kind", s("range")), ("from", opt_n(*from)), ("to", opt_n(*to))], text, color)
        }
        Mapping::Regex { pattern, text, color } => (vec![("kind", s("regex")), ("pattern", s(pattern))], text, color),
        Mapping::Special { matches, text, color } => {
            (vec![("kind", s("special")), ("match", e(*matches))], text, color)
        }
    };
    let mut members = head;
    members.push(("text", opt_s(text)));
    members.push(("color", opt_s(color)));
    Out::Obj(members)
}

fn scalar(v: &Scalar) -> Out {
    match v {
        Scalar::Number(n) => Out::Num(*n),
        Scalar::Text(t) => s(t),
    }
}

fn transform(t: &Transform) -> Out {
    match t {
        Transform::FilterByValue { mode, match_mode, filters } => Out::Obj(vec![
            ("kind", s("filter_by_value")),
            ("mode", e(*mode)),
            ("match", e(*match_mode)),
            (
                "filters",
                list(filters, |f| Out::Obj(vec![("field", s(&f.field)), ("matcher", filter_matcher(&f.matcher))])),
            ),
        ]),
        Transform::GroupingToMatrix { column_field, row_field, value_field, empty_value } => Out::Obj(vec![
            ("kind", s("grouping_to_matrix")),
            ("column_field", s(column_field)),
            ("row_field", s(row_field)),
            ("value_field", s(value_field)),
            ("empty_value", e(*empty_value)),
        ]),
        Transform::Organize { hide, order, rename } => Out::Obj(vec![
            ("kind", s("organize")),
            ("hide", lines(hide)),
            ("order", lines(order)),
            ("rename", list(rename, |r| Out::Obj(vec![("field", s(&r.field)), ("name", s(&r.name))]))),
        ]),
        Transform::CalculateField { alias, replace_fields, calc } => {
            let (key, object) = calc_object(calc);
            Out::Obj(vec![
                ("kind", s("calculate_field")),
                ("mode", e(calc.mode())),
                ("alias", opt_s(alias)),
                ("replace_fields", Out::Bool(*replace_fields)),
                (key, object),
            ])
        }
        Transform::SortBy { sort } => Out::Obj(vec![
            ("kind", s("sort_by")),
            ("sort", list(sort, |k| Out::Obj(vec![("field", s(&k.field)), ("desc", Out::Bool(k.desc))]))),
        ]),
        Transform::Limit { count, end } => {
            Out::Obj(vec![("kind", s("limit")), ("count", Out::Int(i64::from(*count))), ("end", e(*end))])
        }
        Transform::Merge => Out::Obj(vec![("kind", s("merge"))]),
        Transform::Reduce { mode, reducers } => {
            Out::Obj(vec![("kind", s("reduce")), ("mode", e(*mode)), ("reducers", list(reducers, |r| e(*r)))])
        }
        Transform::PartitionByValues { fields, keep_fields } => Out::Obj(vec![
            ("kind", s("partition_by_values")),
            ("fields", lines(fields)),
            ("keep_fields", Out::Bool(*keep_fields)),
        ]),
        Transform::JoinByField { field, mode } => {
            Out::Obj(vec![("kind", s("join_by_field")), ("field", opt_s(field)), ("mode", e(*mode))])
        }
        Transform::FilterFieldsByName { include, exclude } => Out::Obj(vec![
            ("kind", s("filter_fields_by_name")),
            ("include", name_filter(include)),
            ("exclude", name_filter(exclude)),
        ]),
        Transform::RenameByRegex { regex, rename_pattern } => {
            Out::Obj(vec![("kind", s("rename_by_regex")), ("regex", s(regex)), ("rename_pattern", s(rename_pattern))])
        }
        Transform::ConvertFieldType { conversions } => Out::Obj(vec![
            ("kind", s("convert_field_type")),
            ("conversions", list(conversions, |c| Out::Obj(vec![("field", s(&c.field)), ("to", e(c.to))]))),
        ]),
    }
}

fn name_filter(f: &NameFilter) -> Out {
    Out::Obj(vec![("names", lines(&f.names)), ("pattern", opt_s(&f.pattern))])
}

/// The member name and the object of the mode of a `calculate_field` transform.
fn calc_object(calc: &Calc) -> (&'static str, Out) {
    let operand = |o: &Operand| match o {
        Operand::Field(name) => Out::Obj(vec![("kind", s("field")), ("field", s(name))]),
        Operand::Number(v) => Out::Obj(vec![("kind", s("number")), ("value", Out::Num(*v))]),
    };
    match calc {
        Calc::Binary { left, right, operator } => {
            ("binary", Out::Obj(vec![("left", operand(left)), ("right", operand(right)), ("operator", e(*operator))]))
        }
        Calc::ReduceRow { reducer, include } => {
            ("reduce", Out::Obj(vec![("reducer", e(*reducer)), ("include", lines(include))]))
        }
        Calc::Unary { operator, field } => ("unary", Out::Obj(vec![("operator", e(*operator)), ("field", s(field))])),
        Calc::Index { as_percentile } => ("index", Out::Obj(vec![("as_percentile", Out::Bool(*as_percentile))])),
    }
}

fn filter_matcher(m: &FilterMatcher) -> Out {
    let one = |kind: &str, v: Out| Out::Obj(vec![("kind", s(kind)), ("value", v)]);
    match m {
        FilterMatcher::Equal(v) => one("equal", scalar(v)),
        FilterMatcher::NotEqual(v) => one("not_equal", scalar(v)),
        FilterMatcher::Greater(v) => one("greater", Out::Num(*v)),
        FilterMatcher::GreaterOrEqual(v) => one("greater_or_equal", Out::Num(*v)),
        FilterMatcher::Lower(v) => one("lower", Out::Num(*v)),
        FilterMatcher::LowerOrEqual(v) => one("lower_or_equal", Out::Num(*v)),
        FilterMatcher::Range { from, to } => {
            Out::Obj(vec![("kind", s("range")), ("from", Out::Num(*from)), ("to", Out::Num(*to))])
        }
        FilterMatcher::Regex(p) => Out::Obj(vec![("kind", s("regex")), ("pattern", s(p))]),
        FilterMatcher::IsNull => Out::Obj(vec![("kind", s("is_null"))]),
        FilterMatcher::IsNotNull => Out::Obj(vec![("kind", s("is_not_null"))]),
    }
}

/// Writes `legend`. The members `sort_by` and `sort_desc` are optional: the writer leaves out a
/// member that has its default value (4.38, rule 6).
fn legend(l: &Legend) -> Out {
    let mut m = vec![
        ("show", Out::Bool(l.show)),
        ("placement", e(l.placement)),
        ("mode", e(l.mode)),
        ("values", list(&l.values, |v| e(*v))),
    ];
    if let Some(v) = l.sort_by {
        m.push(("sort_by", e(v)));
    }
    if l.sort_desc {
        m.push(("sort_desc", Out::Bool(true)));
    }
    Out::Obj(m)
}

/// Writes `tooltip`. The members `sort` and `hide_zeros` are optional (see [`legend`]).
fn tooltip(t: &Tooltip) -> Out {
    let mut m = vec![("mode", e(t.mode))];
    if t.sort != TooltipSort::None {
        m.push(("sort", e(t.sort)));
    }
    if t.hide_zeros {
        m.push(("hide_zeros", Out::Bool(true)));
    }
    Out::Obj(m)
}

/// Adds `thresholds_style` when it is not the default `off`.
/// The optional option `gradient_mode` (4.28, 4.30): left out at `none`.
fn gradient(m: &mut Vec<(&'static str, Out)>, v: GradientMode) {
    if v != GradientMode::None {
        m.push(("gradient_mode", e(v)));
    }
}

fn thresholds_style(m: &mut Vec<(&'static str, Out)>, v: ThresholdsStyle) {
    if v != ThresholdsStyle::Off {
        m.push(("thresholds_style", e(v)));
    }
}

fn reduce(r: &Reduce) -> Out {
    Out::Obj(vec![("calc", e(r.calc)), ("fields", opt_s(&r.fields)), ("values", Out::Bool(r.values))])
}

fn options(o: &PanelOptions) -> Out {
    match o {
        PanelOptions::Table(t) => {
            let mut m = vec![
                ("show_header", Out::Bool(t.show_header)),
                ("sort", list(&t.sort, |k| Out::Obj(vec![("field", s(&k.field)), ("desc", Out::Bool(k.desc))]))),
            ];
            if t.wrap {
                m.push(("wrap", Out::Bool(true)));
            }
            Out::Obj(m)
        }
        PanelOptions::Timeseries(t) => {
            let mut m = vec![
                ("draw", e(t.draw)),
                ("interpolation", e(t.interpolation)),
                ("line_width", Out::Int(i64::from(t.line_width))),
                ("fill_opacity", Out::Int(i64::from(t.fill_opacity))),
                ("show_points", e(t.show_points)),
                ("point_size", Out::Int(i64::from(t.point_size))),
                ("span_nulls", Out::Bool(t.span_nulls)),
                ("stacking", e(t.stacking)),
                ("y_scale", e(t.y_scale)),
                ("legend", legend(&t.legend)),
                ("tooltip", tooltip(&t.tooltip)),
            ];
            thresholds_style(&mut m, t.thresholds_style);
            // The optional members of the later options (P10, P14) are left out when they have their default (4.38, rule 6).
            gradient(&mut m, t.gradient_mode);
            if t.log_base != LOG_BASE {
                m.push(("log_base", Out::Int(i64::from(t.log_base))));
            }
            if let Some(v) = t.span_nulls_ms {
                m.push(("span_nulls_ms", Out::Int(i64::try_from(v).unwrap_or(MAX_SPAN_NULLS_MS))));
            }
            if t.bar_alignment != BarAlignment::Center {
                m.push(("bar_alignment", e(t.bar_alignment)));
            }
            Out::Obj(m)
        }
        PanelOptions::Stat(t) => {
            let mut m = vec![
                ("reduce", reduce(&t.reduce)),
                ("color_mode", e(t.color_mode)),
                ("graph_mode", e(t.graph_mode)),
                ("text_mode", e(t.text_mode)),
                ("orientation", e(t.orientation)),
            ];
            if t.justify != StatJustify::Auto {
                m.push(("justify", e(t.justify)));
            }
            // The optional members (P11) (4.29), left out with their defaults (4.38, rule 6).
            if t.percent_change {
                m.push(("percent_change", Out::Bool(true)));
            }
            if t.percent_change_color != PercentChangeColor::Standard {
                m.push(("percent_change_color", e(t.percent_change_color)));
            }
            for (key, size) in [("title_size", t.title_size), ("value_size", t.value_size)] {
                if let Some(v) = size {
                    m.push((key, Out::Int(i64::from(v))));
                }
            }
            Out::Obj(m)
        }
        PanelOptions::Barchart(t) => {
            let mut m = vec![
                ("orientation", e(t.orientation)),
                ("x_field", opt_s(&t.x_field)),
                ("stacking", e(t.stacking)),
                ("show_values", e(t.show_values)),
                ("bar_width", Out::Int(i64::from(t.bar_width))),
                ("group_width", Out::Int(i64::from(t.group_width))),
                ("x_tick_rotation", Out::Int(i64::from(t.x_tick_rotation))),
                ("legend", legend(&t.legend)),
                ("tooltip", tooltip(&t.tooltip)),
            ];
            thresholds_style(&mut m, t.thresholds_style);
            // The optional members of 4.30 are left out when they have their default (4.38, rule 6).
            if t.fill_opacity != BAR_FILL_OPACITY {
                m.push(("fill_opacity", Out::Int(i64::from(t.fill_opacity))));
            }
            if t.line_width != BAR_LINE_WIDTH {
                m.push(("line_width", Out::Int(i64::from(t.line_width))));
            }
            gradient(&mut m, t.gradient_mode);
            Out::Obj(m)
        }
        PanelOptions::Piechart(t) => Out::Obj(vec![
            ("pie_type", e(t.pie_type)),
            ("reduce", reduce(&t.reduce)),
            ("labels", list(&t.labels, |v| e(*v))),
            ("legend", legend(&t.legend)),
            ("tooltip", tooltip(&t.tooltip)),
        ]),
        PanelOptions::Xychart(t) => Out::Obj(vec![
            ("x_field", opt_s(&t.x_field)),
            ("y_fields", lines(&t.y_fields)),
            ("draw", e(t.draw)),
            ("point_size", Out::Int(i64::from(t.point_size))),
            ("line_width", Out::Int(i64::from(t.line_width))),
            ("legend", legend(&t.legend)),
            ("tooltip", tooltip(&t.tooltip)),
        ]),
        PanelOptions::Bargauge(t) => Out::Obj(vec![
            ("reduce", reduce(&t.reduce)),
            ("orientation", e(t.orientation)),
            ("display_mode", e(t.display_mode)),
            ("show_unfilled", Out::Bool(t.show_unfilled)),
        ]),
        PanelOptions::Text(t) => Out::Obj(vec![("mode", e(t.mode)), ("content", lines(&t.content))]),
        PanelOptions::StateTimeline(t) => Out::Obj(vec![
            ("merge_values", Out::Bool(t.merge_values)),
            ("show_values", e(t.show_values)),
            ("align_values", e(t.align_values)),
            ("row_height", Out::Int(i64::from(t.row_height))),
            ("line_width", Out::Int(i64::from(t.line_width))),
            ("fill_opacity", Out::Int(i64::from(t.fill_opacity))),
            ("span_nulls", Out::Bool(t.span_nulls)),
            ("show_labels", Out::Bool(t.show_labels)),
            ("legend", legend(&t.legend)),
            ("tooltip", tooltip(&t.tooltip)),
        ]),
        PanelOptions::StatusHistory(t) => Out::Obj(vec![
            ("show_values", e(t.show_values)),
            ("row_height", Out::Int(i64::from(t.row_height))),
            ("col_width", Out::Int(i64::from(t.col_width))),
            ("line_width", Out::Int(i64::from(t.line_width))),
            ("fill_opacity", Out::Int(i64::from(t.fill_opacity))),
            ("show_labels", Out::Bool(t.show_labels)),
            ("legend", legend(&t.legend)),
            ("tooltip", tooltip(&t.tooltip)),
        ]),
        PanelOptions::Heatmap(t) => Out::Obj(vec![
            ("calculate", Out::Bool(t.calculate)),
            (
                "x_buckets",
                Out::Obj(vec![("mode", e(t.x_buckets.mode)), ("value", t.x_buckets.value.map_or(Out::Null, Out::Int))]),
            ),
            (
                "y_buckets",
                Out::Obj(vec![
                    ("mode", e(t.y_buckets.mode)),
                    ("value", opt_n(t.y_buckets.value)),
                    ("scale", e(t.y_buckets.scale)),
                ]),
            ),
            ("y_axis", e(t.y_axis)),
            ("y_reverse", Out::Bool(t.y_reverse)),
            ("y_min", opt_n(t.y_min)),
            ("y_max", opt_n(t.y_max)),
            (
                "color",
                Out::Obj(vec![
                    ("mode", e(t.color.mode)),
                    ("scheme", e(t.color.scheme)),
                    ("fill", s(&t.color.fill)),
                    ("scale", e(t.color.scale)),
                    ("exponent", Out::Num(t.color.exponent)),
                    ("steps", Out::Int(i64::from(t.color.steps))),
                    ("reverse", Out::Bool(t.color.reverse)),
                    ("min", opt_n(t.color.min)),
                    ("max", opt_n(t.color.max)),
                ]),
            ),
            ("cell_gap", Out::Int(i64::from(t.cell_gap))),
            ("hide_le", opt_n(t.hide_le)),
            ("hide_ge", opt_n(t.hide_ge)),
            ("show_legend", Out::Bool(t.show_legend)),
            ("tooltip", tooltip(&t.tooltip)),
        ]),
        PanelOptions::Histogram(t) => Out::Obj(vec![
            ("bucket_count", Out::Int(i64::from(t.bucket_count))),
            ("bucket_size", opt_n(t.bucket_size)),
            ("bucket_offset", Out::Num(t.bucket_offset)),
            ("combine", Out::Bool(t.combine)),
            ("stacking", e(t.stacking)),
            ("line_width", Out::Int(i64::from(t.line_width))),
            ("fill_opacity", Out::Int(i64::from(t.fill_opacity))),
            ("legend", legend(&t.legend)),
            ("tooltip", tooltip(&t.tooltip)),
        ]),
        PanelOptions::Logs(t) => super::logs::logs_out(t),
        PanelOptions::Gauge(t) => gauge(t),
        PanelOptions::Geomap(t) => super::geomap::canonical(t),
    }
}

/// The options of a `gauge` (4.33e): `reduce`, then each optional member that does not have its default (4.38, rule 6).
fn gauge(t: &GaugeOptions) -> Out {
    let mut m = vec![("reduce", reduce(&t.reduce))];
    if t.orientation != StatOrientation::Auto {
        m.push(("orientation", e(t.orientation)));
    }
    if t.show_threshold_labels {
        m.push(("show_threshold_labels", Out::Bool(true)));
    }
    if !t.show_threshold_markers {
        m.push(("show_threshold_markers", Out::Bool(false)));
    }
    if let Some(v) = t.neutral {
        m.push(("neutral", Out::Num(v)));
    }
    if let Some(v) = t.title_size {
        m.push(("title_size", Out::Int(i64::from(v))));
    }
    if let Some(v) = t.value_size {
        m.push(("value_size", Out::Int(i64::from(v))));
    }
    if t.sizing != GaugeSizing::Auto {
        m.push(("sizing", e(t.sizing)));
    }
    if t.min_width != GAUGE_MIN_SIZE {
        m.push(("min_width", Out::Int(i64::from(t.min_width))));
    }
    if t.min_height != GAUGE_MIN_SIZE {
        m.push(("min_height", Out::Int(i64::from(t.min_height))));
    }
    Out::Obj(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_table_of_4_38() {
        let table: [(&str, &str); 7] = [
            ("1.50", "1.5"),
            ("1e3", "1000"),
            ("-0", "0"),
            ("0.000001", "0.000001"),
            ("1e-7", "1e-7"),
            ("1e21", "1e+21"),
            ("123456789012345678901234567890", "1.2345678901234568e+29"),
        ];
        for (input, want) in table {
            let v: f64 = input.parse().unwrap();
            assert_eq!(number_text(v), want, "{input}");
        }
    }

    #[test]
    fn more_numbers() {
        let table: [(f64, &str); 14] = [
            (0.0, "0"),
            (1.0, "1"),
            (-1.0, "-1"),
            (100.0, "100"),
            (0.5, "0.5"),
            (-0.25, "-0.25"),
            (123.456, "123.456"),
            (1e20, "100000000000000000000"),
            (1.5e20, "150000000000000000000"),
            (1.5e-6, "0.0000015"),
            (1.5e-7, "1.5e-7"),
            (-1e21, "-1e+21"),
            (f64::MAX, "1.7976931348623157e+308"),
            (5e-324, "5e-324"),
        ];
        for (v, want) in table {
            assert_eq!(number_text(v), want);
        }
    }

    #[test]
    fn string_escapes() {
        let mut b = String::new();
        write_string("a\"b\\c/d\u{8}\u{c}\n\r\t\u{0}\u{1f}é\u{7f}", &mut b);
        assert_eq!(b, "\"a\\\"b\\\\c/d\\b\\f\\n\\r\\t\\u0000\\u001fé\u{7f}\"");
    }

    #[test]
    fn layout() {
        let v = Out::Obj(vec![
            ("a", Out::Arr(vec![])),
            ("b", Out::Obj(vec![])),
            ("c", Out::Arr(vec![Out::Int(1), Out::Null, Out::Bool(true)])),
        ]);
        let want = "{\n  \"a\": [],\n  \"b\": {},\n  \"c\": [\n    1,\n    null,\n    true\n  ]\n}\n";
        assert_eq!(String::from_utf8(write(&v)).unwrap(), want);
    }
}
