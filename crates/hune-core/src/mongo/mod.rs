// SPDX-License-Identifier: MIT OR Apache-2.0
//! The MongoDB query model (sections 7.27 to 7.36 and 8b.16). The module does no I/O and uses no
//! MongoDB crate (8b.2).
//!
//! # Modules and the sections that each one implements
//!
//! | Module | Sections | Job |
//! |---|---|---|
//! | [`value`] | 8b.16, 7.28 | The Hune value model [`MongoValue`] and its Extended JSON v2 constructor. |
//! | [`tree`] | 7.27 to 7.29 | The parsed top object: [`MongoQuery`], [`QueryBody`], [`FindSpec`]. |
//! | [`command`] | 8b.5, 8b.16 | The closed enum [`MongoCommand`] of the commands that the source sends. |
//! | [`limits`] | 7.28 | The constants of the parser. |
//! | [`json`] | 7.28 | The strict parser of the profile J (J1 to J6) and the writer of a tree. |
//! | [`grammar`] | 7.29 to 7.34 | The top object, the allowlists, the contexts Q, E, A and W, the string leaves. |
//! | [`expand`] | 7.34, 7.35 | The macros, the placeholders, the typed insertion and the property PM. |
//! | [`check`] | 7.36 | The checks at save L1 to L9 and the warnings. |
//!
//! # Use
//!
//! - Run time: [`prepare`] takes the SQL lines and a [`MongoExpandRequest`]. It gives the expanded
//!   [`MongoQuery`] or a [`MongoExpandError`] (`sql_invalid`, `variable_value_refused`, `bad_request`,
//!   `sql_too_large`). [`expand`] does the steps 3 to 7 on a stored [`MongoQuery`] from [`parse`].
//! - Save: [`save_checks`] takes the SQL lines and a [`SaveContext`]. It gives the faults of 7.36 and the
//!   warnings. [`save_checks_into`] puts them into the bounded lists of a [`crate::report::SaveReport`] (4.35).
//!
//! # Status
//!
//! This is the first skeleton. The parser (7.28), the grammar (7.30 to 7.33), the variables
//! (7.34, 7.35) and the save checks (7.36) come in the next waves and build on these types.

pub mod check;
pub mod command;
pub mod expand;
pub mod frame;
pub mod grammar;
pub mod json;
pub mod limits;
pub mod tree;
pub mod value;

pub use check::{MongoSaveResult, MongoSaveWarning, SaveContext, save_checks, save_checks_into, save_checks_text};
pub use command::{
    AggregateCommand, CommandSender, ConnectionStatusCommand, EndSessionsCommand, FindCommand, GetMoreCommand,
    HelloCommand, KillCursorsCommand, ListCollectionsCommand, MongoCommand, ReadPreference, SaslContinueCommand,
    SaslMechanism, SaslStartCommand,
};
pub use expand::{
    InvalidKind, MongoExpandError, MongoExpandRequest, MongoInvalid, ParsedQuery, bson_size, effective_database,
    expand, join_lines, parse, parse_lines, prepare,
};
pub use frame::{ColumnType, Fault as FrameFault, FrameCaps, MAX_FRAME_COLUMNS, MongoFrameBuilder, write_relaxed_json};
pub use grammar::{
    Check, DefNames, Fault, Item, MAX_FAULTS, Macro, NoVariables, Phase, Report, ResolvedNames, VariableNames,
    WarnKind, Warning, check_top, check_top_limited, classify, refusal_message, refusal_reason, shape_top,
    shape_top_limited,
};
pub use json::{JsonError, JsonErrorKind, Parsed, Seg, Span, parse_text, to_query_json};
pub use tree::{FindSpec, MongoQuery, QueryBody, TOP_MEMBERS};
pub use value::{Decimal128Text, Document, EjsonError, MongoValue, OtherBsonType};
