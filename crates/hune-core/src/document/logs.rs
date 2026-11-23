// SPDX-License-Identifier: MIT OR Apache-2.0
//! The panel type `logs` (4.33f): the walk of its options, their canonical form and
//! the rule of the query format `logs`.
//!
//! Each member of the options is optional, so a new value of the type needs no new format version (4.42). The
//! page reads the frame with the rules of 7.16e. The server runs a query of the format `logs` as a query of the
//! format `table`.

use super::canonical::Out;
use super::model::{LogsOptions, LogsOrder, PanelType, QueryFormat, TextEnum};
use super::read::{Ctx, Field, Obj};
use crate::error::ErrorCode;

/// The members of the options of a `logs` panel, in the canonical order (4.33f, 4.38).
const MEMBERS: [&str; 4] = ["show_time", "wrap", "details", "order"];

/// Reads a member that can be absent: an absent member gives `Some(None)`, a member that does not read gives
/// `None` and the error is in `cx`.
fn optional<'a, T>(obj: &mut Obj<'a>, name: &str, parse: impl FnOnce(&Field<'a>) -> Option<T>) -> Option<Option<T>> {
    match obj.opt(name) {
        None => Some(None),
        Some(f) => parse(&f).map(Some),
    }
}

/// Reads the options of a `logs` panel (4.33f). An absent member has its default: `show_time` and `wrap` are
/// `false`, `details` is `true` and `order` is `newest_first`.
pub(super) fn logs_options(cx: &mut Ctx, f: &Field<'_>) -> Option<LogsOptions> {
    let mut obj = f.object(cx, "options")?;
    let show_time = optional(&mut obj, "show_time", |f| f.boolean(cx));
    let wrap = optional(&mut obj, "wrap", |f| f.boolean(cx));
    let details = optional(&mut obj, "details", |f| f.boolean(cx));
    let order = optional(&mut obj, "order", |f| f.en::<LogsOrder>(cx));
    obj.finish(cx, &MEMBERS);
    Some(LogsOptions {
        show_time: show_time?.unwrap_or(false),
        wrap: wrap?.unwrap_or(false),
        details: details?.unwrap_or(true),
        order: order?.unwrap_or(LogsOrder::NewestFirst),
    })
}

/// The canonical form of the options of a `logs` panel: a member with its default value is not written (4.38,
/// rule 6), so the options of a new panel are `{}`.
pub(super) fn logs_out(o: &LogsOptions) -> Out {
    let mut m: Vec<(&'static str, Out)> = Vec::new();
    if o.show_time {
        m.push(("show_time", Out::Bool(true)));
    }
    if o.wrap {
        m.push(("wrap", Out::Bool(true)));
    }
    if !o.details {
        m.push(("details", Out::Bool(false)));
    }
    if o.order != LogsOrder::NewestFirst {
        m.push(("order", Out::Str(o.order.as_str().to_owned())));
    }
    Out::Obj(m)
}

/// Checks the rule of the format `logs` (4.15, 4.33f): the value is permitted in a panel of the type `logs` only.
/// `pointer` is the pointer of the member `format`. A panel whose type did not read gives no error here.
pub(super) fn check_logs_format(cx: &mut Ctx, pointer: &str, format: QueryFormat, panel_type: Option<PanelType>) {
    if format == QueryFormat::Logs && panel_type.is_some_and(|t| t != PanelType::Logs) {
        cx.push(
            pointer,
            ErrorCode::NotPermitted,
            "format logs is permitted in a panel of the type logs only (4.15, 4.33f); value \"logs\"".to_owned(),
        );
    }
}
