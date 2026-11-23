//! The options of the type `logs` (15.6, 4.33f; decision ID-K326, item F33).
//!
//! The new logs panel of Grafana 12.4.2 has 16 options. Four of them map: `showTime`, `wrapLogMessage`,
//! `enableLogDetails` and `sortOrder`. An option at the value of a new Grafana panel is metadata. Another value
//! of an option that Hune does not have is `unsupported`, and the fallback names what the page shows.

use super::{Ctx, Fallback, Node, OptWalk, Out, Row, bool_opt, enum_opt};
use crate::util::as_flag;

/// The value table of `sortOrder` (15.6): the Grafana value, the value of `order` and the class `changed`.
const ORDER: [Row; 2] = [("Descending", "newest_first", false), ("Ascending", "oldest_first", false)];

/// The switches that Hune does not have, with the fallback of the value `true`. The value `false` is the value
/// of a new Grafana panel: it is metadata.
const OFF_SWITCHES: [(&str, &str); 7] = [
    ("showLabels", "the panel shows no unique labels"),
    ("showCommonLabels", "the panel shows no common labels"),
    ("prettifyLogMessage", "the panel shows the JSON text as it is"),
    ("enableInfiniteScrolling", "the panel shows the lines of one query"),
    ("showControls", "the panel shows no controls"),
    ("unwrappedColumns", "the panel shows no columns of displayed fields"),
    ("syntaxHighlighting", "the panel shows the body in one colour"),
];

/// The options with a text value that Hune does not have: the value of a new Grafana panel, and the fallback of
/// another value.
const TEXT_OPTIONS: [(&str, &str, &str); 4] = [
    ("dedupStrategy", "none", "the panel shows each line: Hune has no deduplication"),
    ("fontSize", "default", "the panel shows the text at the default size"),
    ("detailsMode", "inline", "the details row opens under the line (inline)"),
    ("timestampResolution", "ms", "the time shows milliseconds"),
];

/// Maps the `options` of a logs panel to the member `options` of 4.33f. A member that Grafana does not write
/// stays absent, and the canonical form leaves out a member with its default value (4.38).
pub(crate) fn logs_options(ctx: &mut Ctx, options: Option<Node>) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let show_time = w.take("showTime");
    let wrap = w.take("wrapLogMessage");
    let details = w.take("enableLogDetails");
    let order = w.take("sortOrder");
    let switches: Vec<(Node, &str)> = OFF_SWITCHES.iter().filter_map(|(k, fb)| w.take(k).map(|n| (n, *fb))).collect();
    let texts: Vec<(Node, &str, &str)> =
        TEXT_OPTIONS.iter().filter_map(|(k, d, fb)| w.take(k).map(|n| (n, *d, *fb))).collect();
    w.finish(ctx);
    for (n, fallback) in &switches {
        match as_flag(n.v) {
            Some(false) => ctx.meta(),
            _ => {
                ctx.unsupported(&n.ptr, Some(n.v), "Hune has no such option in a logs panel", Fallback::other(fallback))
            }
        }
    }
    for (n, default, fallback) in &texts {
        if n.str() == Some(*default) {
            ctx.meta();
        } else {
            ctx.unsupported(&n.ptr, Some(n.v), "Hune has no such option in a logs panel", Fallback::other(fallback));
        }
    }
    let mut members: Vec<(&str, Out)> = Vec::new();
    if show_time.is_some() {
        members.push(("show_time", bool_opt(ctx, show_time.as_ref(), false)));
    }
    if wrap.is_some() {
        members.push(("wrap", bool_opt(ctx, wrap.as_ref(), false)));
    }
    if details.is_some() {
        members.push(("details", bool_opt(ctx, details.as_ref(), true)));
    }
    if order.is_some() {
        members.push(("order", enum_opt(ctx, order.as_ref(), &ORDER, "newest_first")));
    }
    Out::members(members)
}
