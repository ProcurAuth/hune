//! The options of a Grafana `gauge` (15.6, 4.33e): the classic gauge of Grafana 12.4.2 (the flag `newGauge` is off).
//!
//! Grafana reads `minVizWidth` and `minVizHeight` only with `sizing` = `manual` (the function `calculateGaugeSize`): with `auto`
//! they are metadata, as the sizes of a bar gauge are (H08). The member `neutral` is in `fieldConfig.defaults.custom` in Grafana and
//! is a panel option in Hune. A member that no row names is an item of the report.

use crate::fields::Customs;
use crate::item::{Ctx, Fallback};
use crate::out::Out;
use crate::util::{as_int, as_num};
use crate::walk::{Node, Walk};

use super::{OptWalk, bool_opt, custom, enum_opt, reduce, same};

/// The largest text size (CSS pixels) of `title_size` and `value_size` (4.33e).
const MAX_TEXT: i64 = 200;
/// The largest least size (CSS pixels) of `min_width` and `min_height` (4.33e): the end of the slider of Grafana.
const MAX_MIN: i64 = 600;

/// Maps the `options` of a Grafana gauge, and `custom.neutral` of the defaults, to the member `options` of 4.33e.
pub(super) fn gauge_options(ctx: &mut Ctx, options: Option<Node>, customs: &Customs) -> Out {
    let mut w = OptWalk::new(ctx, options, "options");
    let reduce_node = w.take("reduceOptions");
    let orient = w.take("orientation");
    let labels = w.take("showThresholdLabels");
    let markers = w.take("showThresholdMarkers");
    let sizing = w.take("sizing");
    let min_w = w.take("minVizWidth");
    let min_h = w.take("minVizHeight");
    let text = w.take("text");
    w.finish(ctx);
    let mut members = vec![
        ("reduce", reduce(ctx, reduce_node)),
        ("orientation", enum_opt(ctx, orient.as_ref(), &same(&["auto", "horizontal", "vertical"]), "auto")),
        ("show_threshold_labels", bool_opt(ctx, labels.as_ref(), false)),
        ("show_threshold_markers", bool_opt(ctx, markers.as_ref(), true)),
    ];
    if let Some(n) = custom(customs, "neutral") {
        match as_num(n.v) {
            Some(v) => {
                ctx.ok();
                members.push(("neutral", Out::Num(v)));
            }
            None => ctx.unsupported(&n.ptr, Some(n.v), "the neutral value must be a number", Fallback::MemberRemoved),
        }
    }
    text_sizes(ctx, text, &mut members);
    let manual = match sizing.as_ref().map(|s| (s, s.str())) {
        None => false,
        Some((_, Some("auto"))) => {
            ctx.ok();
            false
        }
        Some((_, Some("manual"))) => {
            ctx.ok();
            members.push(("sizing", Out::s("manual")));
            true
        }
        Some((s, _)) => {
            ctx.unsupported(&s.ptr, Some(s.v), "no value table of 15.6 names this value", Fallback::other("auto"));
            false
        }
    };
    for (key, node) in [("min_width", min_w), ("min_height", min_h)] {
        let Some(node) = node.filter(|n| !n.is_null()) else { continue };
        if !manual {
            // Grafana keeps 75 px with `auto` (15.6): the member has no effect.
            ctx.meta();
            continue;
        }
        match as_int(node.v).filter(|v| (0..=MAX_MIN).contains(v)) {
            Some(v) => {
                ctx.ok();
                members.push((key, Out::Int(v)));
            }
            None => {
                let reason = format!("the value must be an integer from 0 to {MAX_MIN}");
                ctx.unsupported(&node.ptr, Some(node.v), &reason, Fallback::other("75"));
            }
        }
    }
    Out::members(members)
}

/// Maps `options.text`: `titleSize` and `valueSize` give `title_size` and `value_size` (integers from 1 to 200). A value that is not
/// such an integer is `unsupported`, and the member is removed (the size is automatic). Another member of `text` is an item.
fn text_sizes<'a>(ctx: &mut Ctx, text: Option<Node<'a>>, members: &mut Vec<(&'static str, Out)>) {
    let Some(text) = text.filter(|t| !t.is_null()) else { return };
    let Some(mut w) = Walk::new(&text) else {
        ctx.unsupported(&text.ptr, Some(text.v), "text must be an object", Fallback::MemberRemoved);
        return;
    };
    let title = w.take("titleSize");
    let value = w.take("valueSize");
    w.finish(ctx);
    for (key, node) in [("title_size", title), ("value_size", value)] {
        let Some(node) = node.filter(|n| !n.is_null()) else { continue };
        match as_int(node.v).filter(|v| (1..=MAX_TEXT).contains(v)) {
            Some(v) => {
                ctx.ok();
                members.push((key, Out::Int(v)));
            }
            None => {
                let reason = format!("the value must be an integer from 1 to {MAX_TEXT}");
                ctx.unsupported(&node.ptr, Some(node.v), &reason, Fallback::MemberRemoved);
            }
        }
    }
}
