//! The items of the report (15.5, 15.11) and the collector that the conversion fills.

use std::collections::{BTreeMap, BTreeSet};

use crate::options::SourceKind;
use serde_json::Value;

/// The class of a Grafana member (15.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// The document gives the same behaviour. The report counts it only.
    Mapped,
    /// The document gives the behaviour in another form or with a known difference.
    Changed,
    /// The member is not in the document, by a decision of the specification.
    Dropped,
    /// Hune cannot give the behaviour, or the member is not in the measured set.
    Unsupported,
}

impl Class {
    /// Gives the name of the class in the report.
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Mapped => "mapped",
            Class::Changed => "changed",
            Class::Dropped => "dropped",
            Class::Unsupported => "unsupported",
        }
    }
}

/// What the importer does with an `unsupported` item when the owner accepts it (15.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fallback {
    /// The member is not in the document.
    MemberRemoved,
    /// The property is not in the property set.
    PropertyRemoved,
    /// The link is not in the document.
    LinkRemoved,
    /// The panel is not in the document.
    PanelRemoved,
    /// The importer writes no document for the dashboard.
    DashboardNotImported,
    /// Another fallback, as a text.
    Other(String),
}

impl Fallback {
    /// Gives the text of the fallback for the report.
    pub fn text(&self) -> &str {
        match self {
            Fallback::MemberRemoved => "member removed",
            Fallback::PropertyRemoved => "property removed",
            Fallback::LinkRemoved => "link removed",
            Fallback::PanelRemoved => "panel removed",
            Fallback::DashboardNotImported => "the dashboard is not imported",
            Fallback::Other(t) => t,
        }
    }

    /// Makes a fallback from a text.
    pub fn other(text: &str) -> Fallback {
        Fallback::Other(text.to_owned())
    }
}

/// One item of the report: a member that is not `mapped` (15.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportItem {
    /// The item id of 15.10: `<grafana uid>:<JSON pointer>`.
    pub id: String,
    /// The class: `changed`, `dropped` or `unsupported`.
    pub class: Class,
    /// The JSON pointer (RFC 6901) of the member in the input file.
    pub pointer: String,
    /// The JSON pointer of the member in the document of Hune, when the member has one.
    pub hune_pointer: Option<String>,
    /// The id of the panel that holds the member, or `None`.
    pub panel_id: Option<i64>,
    /// The title of the panel that holds the member, or `None`.
    pub panel_title: Option<String>,
    /// The name of the member, for example `options.legend.sortBy`.
    pub member: String,
    /// The reason, in one English sentence.
    pub reason: String,
    /// The fallback of an `unsupported` item, else `None`.
    pub fallback: Option<Fallback>,
    /// The original value as compact JSON, cut to 200 characters.
    pub value: Option<String>,
    /// True when the value was cut.
    pub truncated: bool,
    /// True when the acceptance file lists the item id.
    pub accepted: bool,
}

/// The largest length of the value in an item, in characters (15.11).
pub const VALUE_CHARS: usize = 200;

/// Gives the value as compact JSON, cut at a character boundary, and the flag "cut".
pub fn compact_value(value: &Value) -> (String, bool) {
    let text = serde_json::to_string(value).unwrap_or_default();
    match text.char_indices().nth(VALUE_CHARS) {
        Some((at, _)) => (text[..at].to_owned(), true),
        None => (text, false),
    }
}

/// Tells if a value turns no feature on: `null`, `false`, `0`, `""`, `[]` and `{}` (15.5).
/// An array or an object is trivial when each of its items is trivial.
pub fn is_trivial(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.iter().all(is_trivial),
        Value::Object(o) => o.values().all(is_trivial),
    }
}

/// Tells if a member has the default value of Grafana 12 and 13 (15.5: such a value gives no behaviour).
/// The test uses the end of the pointer, so it fits the v1 and the v2 forms. It names only members
/// that a new panel of Grafana writes with this value.
pub fn is_grafana_default(pointer: &str, value: &Value) -> bool {
    use serde_json::json;
    let table: [(&str, Value); 14] = [
        ("/fieldConfig/defaults/custom/axisColorMode", json!("text")),
        ("/fieldConfig/defaults/custom/axisPlacement", json!("auto")),
        ("/fieldConfig/defaults/custom/gradientMode", json!("none")),
        ("/fieldConfig/defaults/custom/thresholdsStyle", json!({"mode": "off"})),
        ("/fieldConfig/defaults/custom/stacking/group", json!("A")),
        ("/fieldConfig/defaults/custom/lineStyle", json!({"fill": "solid"})),
        ("/fieldConfig/defaults/custom/barWidthFactor", json!(0.6)),
        ("/fieldConfig/defaults/custom/scaleDistribution", json!({"type": "linear"})),
        ("/options/tooltip/sort", json!("none")),
        ("/options/annotations", json!({"clustering": -1, "multiLane": false})),
        ("/options/percentChangeColorMode", json!("standard")),
        ("/options/wideLayout", json!(true)),
        ("/options/cellHeight", json!("sm")),
        ("/options/justifyMode", json!("auto")),
    ];
    table.iter().any(|(suffix, default)| pointer.ends_with(suffix) && default == value)
}

/// Gives the JSON pointer text of one key: `~` is `~0` and `/` is `~1`.
pub fn escape_key(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn unescape_key(key: &str) -> String {
    key.replace("~1", "/").replace("~0", "~")
}

struct Entry {
    scope: usize,
    item: ReportItem,
}

#[derive(Default)]
struct Scope {
    base: String,
    panel: Option<(i64, String)>,
    mapped: u32,
    metadata: u32,
    removed: bool,
}

/// The collector of one dashboard. A scope is the dashboard (scope 0) or one panel.
/// A removed panel keeps only the items that name its removal.
pub(crate) struct Ctx {
    uid: String,
    root_base: String,
    entries: Vec<Entry>,
    scopes: Vec<Scope>,
    current: usize,
    source_uses: Vec<SourceUse>,
    /// The data source variables and the items of `__inputs` of the dashboard (15.9): the name and the plugin type.
    source_names: BTreeMap<String, Option<String>>,
}

/// A data source that a query or a variable uses, with the kind that the Grafana type gives (15.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUse {
    /// The Grafana data source uid or name, as the source map has it.
    pub grafana: String,
    /// The source id that the source map gives.
    pub source: String,
    /// The kind that the type of the Grafana data source gives.
    pub kind: SourceKind,
}

impl Ctx {
    /// Makes a collector for the dashboard `uid` (the Grafana uid). `root_base` is the pointer
    /// of the dashboard object in the file: empty, or `/dashboard`.
    pub(crate) fn new(uid: &str, root_base: &str) -> Ctx {
        Ctx {
            uid: uid.to_owned(),
            root_base: root_base.to_owned(),
            entries: Vec::new(),
            scopes: vec![Scope { base: root_base.to_owned(), ..Scope::default() }],
            current: 0,
            source_uses: Vec::new(),
            source_names: BTreeMap::new(),
        }
    }

    /// Sets the data source variables and the items of `__inputs` of the dashboard (15.9).
    pub(crate) fn set_source_names(&mut self, names: BTreeMap<String, Option<String>>) {
        self.source_names = names;
    }

    /// Gives the plugin type of a data source variable or an item of `__inputs`, or `None` when the dashboard
    /// declares no such name.
    pub(crate) fn source_name(&self, name: &str) -> Option<&Option<String>> {
        self.source_names.get(name)
    }

    /// Records that a query or a variable uses a data source of a kind that its Grafana type gives.
    pub(crate) fn use_source(&mut self, grafana: &str, source: &str, kind: SourceKind) {
        let u = SourceUse { grafana: grafana.to_owned(), source: source.to_owned(), kind };
        if !self.source_uses.contains(&u) {
            self.source_uses.push(u);
        }
    }

    /// Opens a scope for a panel. `base` is the pointer of the panel object.
    pub(crate) fn begin_panel(&mut self, base: &str) -> usize {
        self.scopes.push(Scope { base: base.to_owned(), ..Scope::default() });
        self.current = self.scopes.len() - 1;
        self.current
    }

    /// Gives the id and the title of the panel of the current scope.
    pub(crate) fn set_panel(&mut self, id: Option<i64>, title: &str) {
        let scope = &mut self.scopes[self.current];
        scope.panel = id.map(|i| (i, title.to_owned()));
        if id.is_none() {
            scope.panel = Some((0, title.to_owned()));
        }
    }

    /// Goes back to the scope of the dashboard.
    pub(crate) fn end_panel(&mut self) {
        self.current = 0;
    }

    /// Enters an open scope again.
    pub(crate) fn enter(&mut self, scope: usize) {
        self.current = scope;
    }

    /// Tells if the panel of a scope is removed.
    pub(crate) fn is_removed(&self, scope: usize) -> bool {
        self.scopes[scope].removed
    }

    /// Counts one `mapped` member.
    pub(crate) fn ok(&mut self) {
        self.scopes[self.current].mapped += 1;
    }

    /// Counts one metadata member.
    pub(crate) fn meta(&mut self) {
        self.scopes[self.current].metadata += 1;
    }

    /// The member name of a pointer, relative to the base of the scope.
    fn member_name(&self, pointer: &str) -> String {
        let base = &self.scopes[self.current].base;
        let rel = pointer.strip_prefix(base.as_str()).or_else(|| pointer.strip_prefix(self.root_base.as_str()));
        let rel = rel.unwrap_or(pointer).trim_start_matches('/');
        let mut parts: Vec<String> = rel.split('/').filter(|p| !p.is_empty()).map(unescape_key).collect();
        for pair in [["vizConfig", "spec"], ["data", "spec"]] {
            if parts.len() > 2 && parts[0] == pair[0] && parts[1] == pair[1] {
                parts.drain(0..2);
            }
        }
        parts.join(".")
    }

    /// Adds an item. A fallback `PanelRemoved` removes the panel of the scope.
    pub(crate) fn item(
        &mut self,
        class: Class,
        pointer: &str,
        value: Option<&Value>,
        reason: &str,
        fallback: Option<Fallback>,
    ) {
        if matches!(fallback, Some(Fallback::PanelRemoved)) && self.current != 0 {
            self.scopes[self.current].removed = true;
        }
        let (value, truncated) = match value {
            Some(v) => {
                let (t, c) = compact_value(v);
                (Some(t), c)
            }
            None => (None, false),
        };
        let panel = self.scopes[self.current].panel.clone();
        let item = ReportItem {
            id: format!("{}:{}", self.uid, pointer),
            class,
            pointer: pointer.to_owned(),
            hune_pointer: None,
            panel_id: panel.as_ref().map(|p| p.0).filter(|id| *id != 0),
            panel_title: panel.map(|p| p.1),
            member: self.member_name(pointer),
            reason: reason.to_owned(),
            fallback,
            value,
            truncated,
            accepted: false,
        };
        self.entries.push(Entry { scope: self.current, item });
    }

    /// Adds an `unsupported` item with a fallback.
    pub(crate) fn unsupported(&mut self, pointer: &str, value: Option<&Value>, reason: &str, fallback: Fallback) {
        self.item(Class::Unsupported, pointer, value, reason, Some(fallback));
    }

    /// Adds a `changed` item.
    pub(crate) fn changed(&mut self, pointer: &str, value: Option<&Value>, reason: &str) {
        self.item(Class::Changed, pointer, value, reason, None);
    }

    /// Adds a `dropped` item that is not metadata.
    pub(crate) fn dropped(&mut self, pointer: &str, value: Option<&Value>, reason: &str) {
        self.item(Class::Dropped, pointer, value, reason, None);
    }

    /// A member that no row names (15.5): `unsupported` with the reason "unknown member",
    /// or metadata when the value is trivial.
    pub(crate) fn unknown_member(&mut self, pointer: &str, value: &Value) {
        if is_trivial(value) || is_grafana_default(pointer, value) {
            self.meta();
        } else {
            self.unsupported(pointer, Some(value), "unknown member", Fallback::MemberRemoved);
        }
    }

    /// Tells if an item with the fallback "the dashboard is not imported" exists.
    pub(crate) fn blocks_dashboard(&self) -> bool {
        self.entries
            .iter()
            .any(|e| matches!(e.item.fallback, Some(Fallback::DashboardNotImported)) && !self.scopes[e.scope].removed)
    }

    /// Ends the collection. It drops the items and the counts of the removed panels, except
    /// the items that name the removal. It sets `accepted` from the acceptance set.
    pub(crate) fn finish(self, accepted: &BTreeSet<String>) -> Collected {
        let mut mapped = 0;
        let mut metadata = 0;
        for scope in &self.scopes {
            if !scope.removed {
                mapped += scope.mapped;
                metadata += scope.metadata;
            }
        }
        let mut items: Vec<ReportItem> = self
            .entries
            .into_iter()
            .filter(|e| !self.scopes[e.scope].removed || matches!(e.item.fallback, Some(Fallback::PanelRemoved)))
            .map(|e| {
                let mut item = e.item;
                item.accepted = accepted.contains(&item.id);
                item
            })
            .collect();
        items.sort_by(|a, b| a.pointer.cmp(&b.pointer));
        Collected { items, mapped, metadata, source_uses: self.source_uses }
    }
}

/// The result of a collection.
pub(crate) struct Collected {
    pub items: Vec<ReportItem>,
    pub mapped: u32,
    pub metadata: u32,
    pub source_uses: Vec<SourceUse>,
}
