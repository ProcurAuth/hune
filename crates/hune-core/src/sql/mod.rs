// SPDX-License-Identifier: MIT OR Apache-2.0
//! The ClickHouse dialect of the query engine (section 7).
//!
//! The module does no I/O. It makes the final SQL text from the stored SQL, the variable values,
//! the time range and `max_data_points`. It is the only place that writes a value into SQL.
//!
//! # Run time (7.2, steps 6 to 12)
//!
//! 1. [`check_time_range`] checks `from_ms` and `to_ms` (7.3). A fault gives `invalid_time_range`.
//! 2. [`compute_interval`] checks `max_data_points` and computes `I` and `S` (7.4).
//! 3. [`resolve_values_at`] checks the variable values (7.7). ([`resolve_values`] does the same with no time range:
//!    it refuses the value `$__auto`.) It gives [`ValuesError::BadRequest`]
//!    (`bad_request`) or [`ValuesError::Refused`] (`variable_value_refused`, with `variable` and
//!    `reason`). [`RequestValues::from_json`] reads the member `variables` of a request.
//! 4. [`join_lines`] joins the SQL lines.
//! 5. [`expand`] does the steps 9 to 12. It runs the lexer, the macros, the placeholders and the
//!    escape rules. Then it checks the property P (7.11), the statement (7.12) and the size (7.13).
//! 6. [`expand_prepared`] does the same, and it checks the query parameters `{name:Type}` of ClickHouse (7.6a,
//!    [`params`]): the forms of the stored text, a second scan of the final text, the grammar of each value and the
//!    caps. It gives the final SQL and the values that the client sends as `param_<name>`.
//!
//! The error [`ExpandError`] maps to one API code. [`ExpandError::api_code`] gives it.
//! The codes are `sql_invalid` (with `line` and `column`), `bad_request` (an absent variable
//! member), `variable_value_refused` and `sql_too_large` (with `size` and `limit`).
//!
//! A variable query (7.14) calls [`expand`] with `interval: None`.
//!
//! The ad hoc filters (7.7a, 7.10a): [`adhoc::read_member`] reads the member of an `adhoc` variable, [`adhoc::bind`]
//! checks it against the keys of the document and the configured list, and [`expand_prepared_adhoc`] expands
//! `$__adhocFilters` with the bound filters. [`save_checks_adhoc`] and [`save_checks_into_adhoc`] check the macro at save.
//!
//! # Save time (7.18)
//!
//! [`save_checks`] gives the faults (items 1 to 8 and 10 to 16) and the warnings 1 to 5. [`check_source`] is
//! item 9. [`SaveFault::to_validation_error`] makes the item of `invalid_document`. [`save_checks_into`]
//! puts the items into the bounded lists of a [`crate::report::SaveReport`] (4.35).
//!
//! # Dialects
//!
//! [`expand`] and [`save_checks`] run the dialect `clickhouse`. [`expand_for`] and [`save_checks_for`]
//! take a [`Dialect`], the dialect of the source of the panel or the variable. For `postgres` they use
//! the lexer of 7.20, the escape rules of 7.21, the macros of 7.22 and 7.23, the property P of 7.24,
//! the statement check of 7.25 and the changes of 7.26.
//!
//! # Property P
//!
//! A free value (a value of a `text` or `query` variable) stays in its literal. After the
//! expansion, the engine reads the output again and checks each inserted range against the
//! token structure. A failure gives `variable_value_refused`.
//!
//! An empty text from a list that the viewer selects needs a check too, also for author text.
//! The engine checks that the empty text joins no two tokens. This is stricter than 7.11, which
//! gives an empty range no check.

pub mod adhoc;
pub mod checks;
pub mod dialect;
pub mod escape;
pub mod expand;
pub mod formats;
pub mod interval;
pub mod interval_var;
pub mod lexer;
pub mod macros;
pub mod params;
pub mod placeholders;
pub mod rules;
pub mod statement;
pub mod values;

pub use checks::{
    AdhocSave, SaveCheckResult, SaveFault, SaveWarning, SqlUse, WarningKind, check_source, save_checks,
    save_checks_adhoc, save_checks_for, save_checks_into, save_checks_into_adhoc,
};
pub use dialect::Dialect;
pub use expand::{
    ExpandError, ExpandRequest, Prepared, SqlInvalid, SqlInvalidKind, Stage, expand, expand_for, expand_prepared,
    expand_prepared_adhoc, join_lines,
};
pub use interval::{
    DataPointsError, Interval, MAX_DATA_POINTS, MAX_TIME_MS, TimeRange, TimeRangeError, check_time_range,
    compute_interval, interval_ms,
};
pub use interval_var::{
    AUTO_TABLE, AUTO_VALUE, ItemFault, MAX_AUTO_COUNT, MAX_ITEM_MS, MIN_AUTO_COUNT, auto_min_ms, auto_text,
    largest_table_text_at_most, parse_interval_item,
};
pub use lexer::{
    LexError, LexErrorKind, Position, PositionCursor, Token, TokenKind, lex, lex_all, lex_all_for, lex_for,
    utf16_offset,
};
pub use macros::Macro;
pub use params::{MAX_PARAM_BYTES, MAX_PARAM_ITEMS, MAX_QUERY_PARAMS, ParamType, QueryParams};
pub use placeholders::Format;
pub use statement::{StatementError, StatementFault, check_statement};
pub use values::{
    AutoDef, IntervalDef, ListOptions, MAX_VALUE_BYTES, MAX_VARIABLE_VALUES, ParamShape, RequestValues, Resolved,
    ResolvedValues, Selection, TextValueType, ValueRefused, ValuesError, VariableClass, VariableDef, VariableKind,
    VariableValue, is_number_text, resolve_values, resolve_values_at,
};
