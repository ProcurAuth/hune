// SPDX-License-Identifier: MIT OR Apache-2.0
//! The percent change and the text sizes of a `stat` (15.6, 4.29).
//!
//! The Grafana defaults give no behaviour and stay metadata: `showPercentChange` = `false`, `percentChangeColorMode` =
//! `standard`, and a `text` object whose members are all trivial (`{}`, a size of 0 or `null`: Grafana then fits the size).

use crate::item::is_trivial;

use super::{Ctx, Fallback, Node, OptWalk, Out, as_int, default_fallback};

/// Adds the members `percent_change`, `percent_change_color`, `title_size` and `value_size` for the Grafana options
/// `showPercentChange`, `percentChangeColorMode` and `text`. Each one can be absent.
pub(super) fn percent_and_text(
    ctx: &mut Ctx,
    members: &mut Vec<(&'static str, Out)>,
    show: Option<&Node>,
    color: Option<&Node>,
    text: Option<Node>,
) {
    if let Some(n) = show {
        match n.v.as_bool() {
            Some(true) => {
                ctx.ok();
                members.push(("percent_change", Out::Bool(true)));
            }
            Some(false) => ctx.meta(),
            None => ctx.unsupported(&n.ptr, Some(n.v), "the value must be true or false", Fallback::MemberRemoved),
        }
    }
    if let Some(n) = color {
        match n.str() {
            Some("standard") => ctx.meta(),
            Some(v @ ("inverted" | "same_as_value")) => {
                ctx.ok();
                members.push(("percent_change_color", Out::s(v)));
            }
            _ => ctx.unsupported(&n.ptr, Some(n.v), "no value table of 15.6 names this value", default_fallback()),
        }
    }
    if text.as_ref().is_some_and(|n| is_trivial(n.v)) {
        ctx.meta();
        return;
    }
    let mut w = OptWalk::new(ctx, text, "text");
    let title = w.take("titleSize");
    let value = w.take("valueSize");
    let percent = w.take("percentSize");
    w.finish(ctx);
    for (node, member) in [(title, "title_size"), (value, "value_size")] {
        let Some(n) = node else { continue };
        match as_int(n.v) {
            // Grafana reads a size of 0 as "no size": the page fits the text.
            Some(0) => ctx.meta(),
            Some(v @ 1..=200) => {
                ctx.ok();
                members.push((member, Out::Int(v)));
            }
            _ => ctx.unsupported(
                &n.ptr,
                Some(n.v),
                "the value must be an integer from 1 to 200",
                Fallback::MemberRemoved,
            ),
        }
    }
    if let Some(n) = percent {
        let why = "Hune has no size option for the percent change: its size follows the value (9.19)";
        ctx.unsupported(&n.ptr, Some(n.v), why, Fallback::MemberRemoved);
    }
}
