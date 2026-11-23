//! The repeat of a panel and of a row (15.6, 4.13a).
//!
//! The reading of the members of the two forms, the mapping to the member `repeat`, and the checks that
//! need all panels. The decisions that the owner can change are in one place each:
//! the rule for the variable (ID-K94) is [`variable_fault`], and the default direction is [`DEFAULT_DIRECTION`].

use std::collections::{BTreeMap, BTreeSet};

use hune_core::limits::MAX_PANELS;

use crate::item::{Ctx, Fallback, is_trivial};
use crate::out::Out;
use crate::panel::PanelOut;
use crate::query::DataOut;
use crate::util::as_int;
use crate::variables::{VarKind, VarOut};
use crate::walk::{Node, Walk};

/// The direction that an absent or `null` direction gets: Grafana gives the copies one row (15.6).
pub(crate) const DEFAULT_DIRECTION: &str = "h";

/// The fallback of a repeat that the importer removes.
fn no_repeat() -> Fallback {
    Fallback::other("no repeat")
}

/// The variables of the document: the name, and `true` when the variable can repeat (4.13a, rule 1).
pub(crate) type RepeatVars = BTreeMap<String, bool>;

/// Tells which variables can repeat. A `query` or `custom` variable with `multi` can.
pub(crate) fn repeat_vars(vars: &[VarOut]) -> RepeatVars {
    vars.iter()
        .map(|v| {
            let multi = match &v.kind {
                VarKind::Query { multi, .. } | VarKind::Custom { multi, .. } => *multi,
                VarKind::Text { .. } | VarKind::Interval { .. } | VarKind::Constant { .. } | VarKind::Adhoc { .. } => {
                    false
                }
            };
            (v.name.clone(), multi)
        })
        .collect()
}

/// The rule for the variable of a repeat (4.13a, rule 1; decision ID-K94). It gives the reason when the
/// repeat is not possible. The importer never changes the variable to make it repeat.
fn variable_fault(vars: &RepeatVars, name: &str) -> Option<String> {
    match vars.get(name) {
        Some(true) => None,
        Some(false) => Some(format!(
            "the variable {name} must be a query or custom variable with several values (multi): Hune repeats by such a variable only (4.13a)"
        )),
        None => Some(format!("the variable {name} is not a variable of the dashboard")),
    }
}

/// The members of a repeat in the input, before the mapping. The two forms fill the same fields.
pub(crate) struct RepeatRaw<'a> {
    /// The member that holds the repeat: its pointer is the pointer of the items.
    at: Node<'a>,
    /// The name of the variable.
    name: Option<Node<'a>>,
    direction: Option<Node<'a>>,
    max_per_row: Option<Node<'a>>,
}

/// Reads `repeat`, `repeatDirection` and `maxPerRow` of a v1 panel or row. A panel with no repeat has no
/// effect from the two other members, so they are metadata.
pub(crate) fn read_v1<'a>(ctx: &mut Ctx, w: &mut Walk<'a>) -> Option<RepeatRaw<'a>> {
    let repeat = w.take("repeat");
    let direction = w.take("repeatDirection");
    let max_per_row = w.take("maxPerRow");
    match repeat {
        Some(at) if !is_trivial(at.v) => Some(RepeatRaw { name: Some(at.clone()), at, direction, max_per_row }),
        other => {
            let count =
                usize::from(other.is_some()) + usize::from(direction.is_some()) + usize::from(max_per_row.is_some());
            (0..count).for_each(|_| ctx.meta());
            None
        }
    }
}

/// Reads the object `repeat` of a v2 grid item or of a v2 row: `mode` and `value`, and for a grid item
/// `direction` and `maxPerRow`. Another mode than `variable` is `unsupported`.
pub(crate) fn read_v2<'a>(ctx: &mut Ctx, at: &Node<'a>) -> Option<RepeatRaw<'a>> {
    if is_trivial(at.v) {
        ctx.meta();
        return None;
    }
    let Some(mut w) = Walk::new(at) else {
        ctx.unsupported(&at.ptr, Some(at.v), "the repeat must be an object", no_repeat());
        return None;
    };
    let mode = w.take("mode");
    let name = w.take("value");
    let direction = w.take("direction");
    let max_per_row = w.take("maxPerRow");
    w.finish(ctx);
    if mode.as_ref().and_then(Node::str) != Some("variable") {
        let node = mode.as_ref().unwrap_or(at);
        ctx.unsupported(&node.ptr, Some(node.v), "the repeat mode must be variable", no_repeat());
        return None;
    }
    Some(RepeatRaw { at: at.clone(), name, direction, max_per_row })
}

/// The name of the variable of a repeat, when it can repeat (4.13a, rule 1).
fn variable_name(ctx: &mut Ctx, vars: &RepeatVars, raw: &RepeatRaw) -> Option<String> {
    let node = raw.name.as_ref().unwrap_or(&raw.at);
    let Some(name) = node.str().filter(|n| !n.is_empty()) else {
        ctx.unsupported(&node.ptr, Some(node.v), "the repeat must name a variable", no_repeat());
        return None;
    };
    if let Some(reason) = variable_fault(vars, name) {
        ctx.unsupported(&node.ptr, Some(node.v), &reason, no_repeat());
        return None;
    }
    ctx.ok();
    Some(name.to_owned())
}

/// A repeat of a panel after the mapping.
#[derive(Debug, Clone)]
pub(crate) struct PanelRepeatOut {
    pub variable: String,
    pub direction: &'static str,
    pub max_per_row: Option<i64>,
    /// The pointer of the member in the input, for the items of the checks across panels.
    pub pointer: String,
}

impl PanelRepeatOut {
    pub(crate) fn to_out(&self) -> Out {
        let mut m = vec![("variable", Out::s(&self.variable)), ("direction", Out::s(self.direction))];
        if let Some(n) = self.max_per_row {
            m.push(("max_per_row", Out::Int(n)));
        }
        Out::members(m)
    }
}

/// A text panel does not repeat (15.6, 4.33a rule 6). The repeat is `unsupported` with the fallback "no repeat".
/// The direction and `maxPerRow` have no effect without a repeat: they are metadata.
pub(crate) fn refuse_text_repeat(ctx: &mut Ctx, raw: &RepeatRaw) {
    ctx.unsupported(&raw.at.ptr, Some(raw.at.v), "a text panel does not repeat (4.33a, rule 6)", no_repeat());
    for _ in [&raw.direction, &raw.max_per_row].into_iter().flatten() {
        ctx.meta();
    }
}

/// Maps the repeat of a panel (15.6). A fault removes the repeat and keeps the panel.
pub(crate) fn panel_repeat(ctx: &mut Ctx, vars: &RepeatVars, raw: &RepeatRaw) -> Option<PanelRepeatOut> {
    let variable = variable_name(ctx, vars, raw)?;
    let direction = match raw.direction.as_ref().filter(|d| !d.is_null()) {
        None => {
            let at = raw.direction.as_ref().unwrap_or(&raw.at);
            let reason = format!("the repeat has no direction: Hune uses {DEFAULT_DIRECTION} (4.13a)");
            ctx.changed(&at.ptr, Some(at.v), &reason);
            DEFAULT_DIRECTION
        }
        Some(d) => match d.str() {
            Some("h") => {
                ctx.ok();
                "h"
            }
            Some("v") => {
                ctx.ok();
                "v"
            }
            _ => {
                ctx.unsupported(&d.ptr, Some(d.v), "the direction must be h or v", no_repeat());
                if raw.max_per_row.is_some() {
                    ctx.meta();
                }
                return None;
            }
        },
    };
    let max_per_row = match raw.max_per_row.as_ref().filter(|m| !m.is_null()) {
        None => None,
        // Grafana ignores `maxPerRow` with the direction v.
        Some(_) if direction == "v" => {
            ctx.meta();
            None
        }
        Some(m) => match as_int(m.v).filter(|n| (1..=24).contains(n)) {
            Some(n) => {
                ctx.ok();
                Some(n)
            }
            None => {
                ctx.unsupported(
                    &m.ptr,
                    Some(m.v),
                    "maxPerRow must be a whole number from 1 to 24",
                    Fallback::MemberRemoved,
                );
                None
            }
        },
    };
    Some(PanelRepeatOut { variable, direction, max_per_row, pointer: raw.at.ptr.clone() })
}

/// A repeat of a section after the mapping.
#[derive(Debug, Clone)]
pub(crate) struct SectionRepeatOut {
    pub variable: String,
    pub pointer: String,
}

impl SectionRepeatOut {
    pub(crate) fn to_out(&self) -> Out {
        Out::obj([("variable", Out::s(&self.variable))])
    }
}

/// Maps the repeat of a row (15.6). A row with no header cannot repeat (4.13a, rule 7). Grafana ignores the
/// direction and `maxPerRow` of a row.
pub(crate) fn section_repeat(
    ctx: &mut Ctx,
    vars: &RepeatVars,
    raw: &RepeatRaw,
    has_header: bool,
) -> Option<SectionRepeatOut> {
    for _ in [&raw.direction, &raw.max_per_row].into_iter().flatten() {
        ctx.meta();
    }
    if !has_header {
        let reason = "a section with no header cannot repeat: the copies need a title (4.13a, rule 7)";
        ctx.unsupported(&raw.at.ptr, Some(raw.at.v), reason, no_repeat());
        return None;
    }
    let variable = variable_name(ctx, vars, raw)?;
    Some(SectionRepeatOut { variable, pointer: raw.at.ptr.clone() })
}

/// The members of a v1 panel or row that are metadata when a copy is stored: they name the original.
pub(crate) const COPY_METADATA: [&str; 3] = ["repeatIteration", "repeatedByRow", "scopedVars"];

/// A section after the mapping.
pub(crate) struct SectionOut {
    pub title: Option<String>,
    pub collapsed: bool,
    pub panels: Vec<PanelOut>,
    pub repeat: Option<SectionRepeatOut>,
}

impl SectionOut {
    pub(crate) fn to_out(&self) -> Out {
        let mut m = vec![
            ("title", Out::opt_s(self.title.as_deref())),
            ("collapsed", Out::Bool(self.collapsed)),
            ("panels", Out::Arr(self.panels.iter().map(PanelOut::to_out).collect())),
        ];
        if let Some(r) = &self.repeat {
            m.push(("repeat", r.to_out()));
        }
        Out::members(m)
    }
}

/// Tells if a panel is a reuse panel.
fn is_reuse(p: &PanelOut) -> bool {
    matches!(p.data, DataOut::Reuse { .. })
}

/// Tells if a panel has several queries (4.15a). The importer removes its repeat (decision ID-K165).
fn has_extra_queries(p: &PanelOut) -> bool {
    matches!(&p.data, DataOut::Query { extra, .. } if !extra.is_empty())
}

/// Removes the repeat of a panel and adds the item.
fn drop_panel_repeat(ctx: &mut Ctx, p: &mut PanelOut, reason: &str) {
    if let Some(r) = p.repeat.take() {
        ctx.enter(p.scope);
        ctx.unsupported(&r.pointer, None, reason, Fallback::other("no repeat on the panel"));
        ctx.end_panel();
    }
}

/// Removes the repeat of a section and adds the item.
fn drop_section_repeat(ctx: &mut Ctx, s: &mut SectionOut, reason: &str) {
    if let Some(r) = s.repeat.take() {
        ctx.unsupported(&r.pointer, None, reason, no_repeat());
    }
}

/// The checks across panels (15.6, 4.13a). Hune has one level of repeat, no repeat of a reuse panel, no reuse
/// of a repeated panel, and its own rows for a panel with the direction `h`. The checks run in this order:
/// the sections first, then the panels. A removed repeat is an `unsupported` item.
///
/// The cap of `MAX_PANELS` (4.13, rule 2) applies first. Above it, the dashboard is not imported, and the checks
/// do not run: the check of the rows compares the panels of a section in pairs.
pub(crate) fn check_repeats(ctx: &mut Ctx, sections: &mut [SectionOut]) {
    let total: usize = sections.iter().map(|s| s.panels.len()).sum();
    if total > MAX_PANELS {
        let reason = format!("the dashboard has {total} panels: a document has {MAX_PANELS} panels at most (4.13)");
        ctx.unsupported("", None, &reason, Fallback::DashboardNotImported);
        return;
    }
    // The sections: a reuse panel in the section, or a reuse panel that names a panel of the section.
    let reuse_sources: BTreeSet<i64> = sections
        .iter()
        .flat_map(|s| s.panels.iter())
        .filter_map(|p| match &p.data {
            DataOut::Reuse { panel, .. } => Some(*panel),
            DataOut::Query { .. } | DataOut::None => None,
        })
        .collect();
    for s in sections.iter_mut() {
        if s.repeat.is_none() {
            continue;
        }
        if s.panels.iter().any(is_reuse) {
            drop_section_repeat(
                ctx,
                s,
                "the section holds a reuse panel: Hune has no repeat of a section with one (4.13a, rule 7)",
            );
        } else if s.panels.iter().any(|p| reuse_sources.contains(&p.id)) {
            drop_section_repeat(
                ctx,
                s,
                "a reuse panel names a panel of the section: Hune has no such repeat (4.13a, rule 8)",
            );
        } else if s.panels.iter().any(has_extra_queries) {
            drop_section_repeat(
                ctx,
                s,
                "the section holds a panel with several queries: Hune has one query for each copy (4.15a, rule 8)",
            );
        }
    }
    // The panels.
    for s in sections.iter_mut() {
        let in_repeated_section = s.repeat.is_some();
        for p in s.panels.iter_mut() {
            if p.repeat.is_none() {
                continue;
            }
            if is_reuse(p) {
                drop_panel_repeat(
                    ctx,
                    p,
                    "the panel reuses a frame: Hune has no repeat of a reuse panel (4.13a, rule 8)",
                );
            } else if has_extra_queries(p) {
                drop_panel_repeat(
                    ctx,
                    p,
                    "the panel has several queries: Hune has one query for each copy, so the repeat is removed and the queries stay (4.15a, rule 8)",
                );
            } else if in_repeated_section {
                drop_panel_repeat(ctx, p, "the row has a repeat: Hune has one level of repeat (4.13a, rule 7)");
            } else if reuse_sources.contains(&p.id) {
                drop_panel_repeat(
                    ctx,
                    p,
                    "a reuse panel names this panel: Hune has no reuse of a repeated panel (4.13a, rule 8)",
                );
            }
        }
    }
    // A panel with the direction h needs its own rows: no other panel shares a row unit with it.
    for s in sections.iter_mut() {
        let rows: Vec<(i64, i64, i64)> = s.panels.iter().map(|p| (p.id, p.grid[1], p.grid[1] + p.grid[3])).collect();
        for p in s.panels.iter_mut() {
            let h_repeat = p.repeat.as_ref().is_some_and(|r| r.direction == "h");
            if !h_repeat {
                continue;
            }
            let (top, bottom) = (p.grid[1], p.grid[1] + p.grid[3]);
            if rows.iter().any(|(id, t, b)| *id != p.id && *t < bottom && top < *b) {
                drop_panel_repeat(
                    ctx,
                    p,
                    "no repeat: the panel shares a row unit with another panel, and Hune needs free rows for the direction h (4.13a, rule 5)",
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::panelopts::Pt;
    use crate::query::TimeOverride;

    fn panel(id: i64, y: i64, data: DataOut, repeat: Option<&'static str>) -> PanelOut {
        PanelOut {
            scope: 0,
            id,
            ptype: Pt::Stat,
            title: String::new(),
            description: String::new(),
            transparent: false,
            grid: [0, y, 12, 1],
            data,
            transforms: Vec::new(),
            fields: Out::Null,
            options: Out::Null,
            links: Vec::new(),
            sqls: Vec::new(),
            pointer: String::new(),
            repeat: repeat.map(|d| PanelRepeatOut {
                variable: "host".to_owned(),
                direction: d,
                max_per_row: None,
                pointer: format!("/panels/{id}/repeat"),
            }),
        }
    }

    fn query() -> DataOut {
        DataOut::Query {
            source: "prod".to_owned(),
            format: "table",
            sql: vec!["SELECT 1".to_owned()],
            min_interval: None,
            max_data_points: None,
            time: TimeOverride::default(),
            query_ref: "A".to_owned(),
            hide: false,
            extra: Vec::new(),
        }
    }

    fn reuse(source: i64) -> DataOut {
        DataOut::Reuse { panel: source, with_transforms: false, pointer: String::new() }
    }

    /// A hostile list: `n` panels with the direction h, each in its own row unit, in a section with a repeat and in
    /// a section with no repeat, and `n` reuse panels that name panel 1.
    fn hostile(n: i64) -> Vec<SectionOut> {
        let mut repeated = vec![panel(1, 0, query(), None)];
        let mut plain = Vec::new();
        for i in 0..n {
            repeated.push(panel(10 + i, 1 + i, query(), None));
            plain.push(panel(1_000_000 + i, i, query(), Some("h")));
        }
        let reuses = (0..n).map(|i| panel(2_000_000 + i, i, reuse(1), None)).collect();
        let section = |panels, repeat: bool| SectionOut {
            title: Some("S".to_owned()),
            collapsed: false,
            panels,
            repeat: repeat
                .then(|| SectionRepeatOut { variable: "host".to_owned(), pointer: "/panels/0/repeat".to_owned() }),
        };
        vec![section(repeated, true), section(plain, false), section(reuses, false)]
    }

    #[test]
    fn d7_the_checks_across_panels_stay_bounded_for_a_hostile_list() {
        // D7 (a) of the security review of 2026-11-16: the checks compared each panel with each reuse panel and
        // each other panel of its section. The cap of 4.13 (rule 2) now comes first. Minimum of 7 runs after 2
        // warm-ups, with a generous limit for a debug build.
        let mut best = Duration::MAX;
        let mut items = 0;
        for run in 0..9 {
            let mut sections = hostile(20_000);
            let mut ctx = Ctx::new("t", "");
            let t = Instant::now();
            check_repeats(&mut ctx, &mut sections);
            let elapsed = t.elapsed();
            if run >= 2 {
                best = best.min(elapsed);
            }
            items = ctx.finish(&Default::default()).items.len();
        }
        assert!(best < Duration::from_millis(100), "the checks took {best:?}");
        assert_eq!(items, 1);
    }
}
