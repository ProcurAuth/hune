//! The checks at save for a MongoDB source (7.36, items L1 to L9) and the warnings 1 to 3.
//!
//! The function [`save_checks`] gives data. The caller makes the items of `invalid_document` from it with
//! [`crate::sql::SaveFault::to_validation_error`]. The pointer of an item names the SQL line (7.28). The
//! message gives the JSON pointer inside the query text and the column in the line. The caller runs the
//! item 9 of 7.18 (an unknown source id) with `crate::sql::check_source`. The warning 4 of 7.36 (a
//! placeholder in a display text) is a check of the page texts, not of the query.

use crate::error::ErrorCode;
use crate::limits::{MAX_ERRORS, MAX_WARNINGS};
use crate::mongo::expand::join_lines;
use crate::mongo::grammar::{
    Check, DefNames, Fault, Phase, WarnKind, Warning as MongoWarning, check_top_limited, shape_top_limited,
};
use crate::mongo::json::{JsonErrorKind, parse_text, pointer};
use crate::report::{SaveReport, Warning};
use crate::sql::lexer::{Position, PositionCursor};
use crate::sql::values::VariableDef;
use crate::sql::{SaveFault, SqlUse};

/// The input of [`save_checks`]: the variables of the document, and the lists of the source.
#[derive(Debug, Clone, Copy)]
pub struct SaveContext<'a> {
    /// All variables of the document.
    pub variables: &'a [VariableDef],
    /// The key `collections` of the source, with the configuration of this moment (7.33).
    pub collections: &'a [String],
    /// The key `databases` of the source.
    pub databases: &'a [String],
    /// A panel or a `query` variable. A variable query takes no interval macro (L9).
    pub usage: SqlUse,
}

/// One warning of 7.36. A warning never stops a save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MongoSaveWarning {
    /// The kind: warning 1, 2 or 3.
    pub kind: WarnKind,
    /// The index of the SQL line, from 0.
    pub line_index: usize,
    /// The column in the line, in characters, from 1.
    pub column: usize,
    /// The English message, without the column.
    pub message: String,
}

impl MongoSaveWarning {
    /// Gives the pointer and the message of the member `warnings` (6.13).
    pub fn to_pointer_message(&self, sql_pointer: &str) -> (String, String) {
        (format!("{sql_pointer}/{}", self.line_index), format!("column {}: {}", self.column, self.message))
    }
}

/// The result of [`save_checks`]. Each list holds at most `MAX_ERRORS` minus 1 or `MAX_WARNINGS` minus 1 items
/// (4.35).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MongoSaveResult {
    /// The faults, in the order of their position in the text.
    pub faults: Vec<SaveFault>,
    /// The warnings, in the order of their position in the text.
    pub warnings: Vec<MongoSaveWarning>,
    /// True when the text has more faults than the list holds.
    pub more_faults: bool,
    /// True when the text has more warnings than the list holds.
    pub more_warnings: bool,
}

/// Runs the checks L1 to L9 and the warnings 1 to 3 of 7.36 on the SQL lines of a panel or a variable.
///
/// A fault of the profile J (L1) stops the text, so it is the only fault. Else the checks of the top
/// object (L2) and of the grammar (L3 to L9) run together.
pub fn save_checks<S: AsRef<str>>(lines: &[S], ctx: &SaveContext<'_>) -> MongoSaveResult {
    save_checks_text(&join_lines(lines), ctx)
}

/// Runs [`save_checks`] on the joined text.
pub fn save_checks_text(text: &str, ctx: &SaveContext<'_>) -> MongoSaveResult {
    check_bounded(text, ctx, MAX_ERRORS - 1, MAX_WARNINGS - 1)
}

/// Runs [`save_checks`] and puts the items into the lists of `report` (6.13, 7.36). `sql_pointer` is the
/// pointer of the member `sql`.
///
/// The walk keeps only the items that the lists can take, the first ones of the walk. When the text has more
/// items than the room of a list, the list stops with its last item.
pub fn save_checks_into<S: AsRef<str>>(lines: &[S], ctx: &SaveContext<'_>, sql_pointer: &str, report: &mut SaveReport) {
    let result = check_bounded(&join_lines(lines), ctx, report.error_room(), report.warning_room());
    for f in &result.faults {
        report.push_error(f.to_validation_error(sql_pointer));
    }
    if result.more_faults {
        report.stop_errors();
    }
    for w in &result.warnings {
        let (pointer, message) = w.to_pointer_message(sql_pointer);
        report.push_warning(Warning { pointer, message });
    }
    if result.more_warnings {
        report.stop_warnings();
    }
}

/// Runs the checks and keeps at most `fault_room` faults and `warning_room` warnings.
fn check_bounded(text: &str, ctx: &SaveContext<'_>, fault_room: usize, warning_room: usize) -> MongoSaveResult {
    let parsed = match parse_text(text) {
        Ok(p) => p,
        Err(e) => {
            let code = match e.kind {
                JsonErrorKind::BadFormat => ErrorCode::BadFormat,
                JsonErrorKind::TooDeep => ErrorCode::TooDeep,
                JsonErrorKind::TooLarge => ErrorCode::TooLarge,
            };
            let p = Position::at(text, e.offset);
            let fault =
                SaveFault { code, line_index: p.line as usize - 1, column: p.column as usize, message: e.message };
            let (faults, more_faults) = if fault_room > 0 { (vec![fault], false) } else { (Vec::new(), true) };
            return MongoSaveResult { faults, warnings: Vec::new(), more_faults, more_warnings: false };
        }
    };
    // The lists keep at most the room of each list, so the search of the offsets below stays bounded.
    let mut found: Vec<(usize, Fault)> = Vec::new();
    let mut more_faults = false;
    if let Err((faults, more)) = shape_top_limited(&parsed.value, fault_room) {
        more_faults = more;
        for f in faults {
            found.push((parsed.span.locate(&parsed.value, &f.path, f.at_key), f));
        }
    }
    let names = DefNames(ctx.variables);
    let check = Check {
        phase: Phase::Stored,
        vars: &names,
        collections: ctx.collections,
        databases: ctx.databases,
        usage: ctx.usage,
    };
    let report = check_top_limited(&parsed.value, &check, fault_room.saturating_sub(found.len()), warning_room);
    more_faults |= report.more_faults;
    for f in report.faults {
        found.push((parsed.span.locate(&parsed.value, &f.path, f.at_key), f));
    }
    found.sort_by_key(|(o, _)| *o);
    let mut cursor = PositionCursor::new(text);
    let faults = found
        .iter()
        .map(|(o, f)| {
            let p = cursor.at(*o);
            let pointer = f.pointer();
            let message = if pointer.is_empty() { f.message.clone() } else { format!("{pointer}: {}", f.message) };
            SaveFault { code: f.code, line_index: p.line as usize - 1, column: p.column as usize, message }
        })
        .collect();
    let mut located: Vec<(usize, &MongoWarning)> =
        report.warnings.iter().map(|w| (parsed.span.locate(&parsed.value, &w.path, false), w)).collect();
    located.sort_by_key(|(o, _)| *o);
    let mut cursor = PositionCursor::new(text);
    let warnings = located
        .into_iter()
        .map(|(offset, w)| {
            let p = cursor.at(offset);
            let message = format!("{}: {}", pointer(&w.path), w.message);
            MongoSaveWarning { kind: w.kind, line_index: p.line as usize - 1, column: p.column as usize, message }
        })
        .collect();
    MongoSaveResult { faults, warnings, more_faults, more_warnings: report.more_warnings }
}
