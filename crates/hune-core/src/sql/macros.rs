// SPDX-License-Identifier: MIT OR Apache-2.0
//! The macros and the built-in variables: names, forms and the parse of a call.
//!
//! The dialect `clickhouse` has [`Macro`] (7.10). The dialect `postgres` has [`PgMacro`] (7.22).

use super::dialect::Dialect;
use super::interval::{Interval, TimeRange};
use super::lexer::{Token, TokenKind};

/// One of the macros of 7.10. Each name starts with `$__`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Macro {
    /// `$__timeFilter(expr)`
    TimeFilter,
    /// `$__timeFilter_ms(expr)`
    TimeFilterMs,
    /// `$__timeInterval(expr)`
    TimeInterval,
    /// `$__timeInterval_ms(expr)`
    TimeIntervalMs,
    /// `$__dateFilter(expr)`
    DateFilter,
    /// `$__dateTimeFilter(date_expr, time_expr)`
    DateTimeFilter,
    /// `$__dt(date_expr, time_expr)`: the second name of `$__dateTimeFilter` in the plugin (7.10).
    Dt,
    /// `$__conditionalAll(condition, $variable)`
    ConditionalAll,
    /// `$__fromTime`
    FromTime,
    /// `$__toTime`
    ToTime,
    /// `$__fromTime_ms`
    FromTimeMs,
    /// `$__toTime_ms`
    ToTimeMs,
    /// `$__interval_s`
    IntervalS,
    /// `$__interval_ms`
    IntervalMs,
    /// `$__from`
    From,
    /// `$__to`
    To,
    /// `$__adhocFilters`: the filters of the `adhoc` variable of the source (7.10a).
    AdhocFilters,
}

impl Macro {
    /// Finds the macro of a name. The name starts with `__` and has no `$`.
    pub fn from_name(name: &str) -> Option<Macro> {
        match name {
            "__timeFilter" => Some(Macro::TimeFilter),
            "__timeFilter_ms" => Some(Macro::TimeFilterMs),
            "__timeInterval" => Some(Macro::TimeInterval),
            "__timeInterval_ms" => Some(Macro::TimeIntervalMs),
            "__dateFilter" => Some(Macro::DateFilter),
            "__dateTimeFilter" => Some(Macro::DateTimeFilter),
            "__dt" => Some(Macro::Dt),
            "__conditionalAll" => Some(Macro::ConditionalAll),
            "__fromTime" => Some(Macro::FromTime),
            "__toTime" => Some(Macro::ToTime),
            "__fromTime_ms" => Some(Macro::FromTimeMs),
            "__toTime_ms" => Some(Macro::ToTimeMs),
            "__interval_s" => Some(Macro::IntervalS),
            "__interval_ms" => Some(Macro::IntervalMs),
            "__from" => Some(Macro::From),
            "__to" => Some(Macro::To),
            "__adhocFilters" => Some(Macro::AdhocFilters),
            _ => None,
        }
    }

    /// Gives the name of the macro with the `$` and with the `__`.
    pub fn name(self) -> &'static str {
        match self {
            Macro::TimeFilter => "$__timeFilter",
            Macro::TimeFilterMs => "$__timeFilter_ms",
            Macro::TimeInterval => "$__timeInterval",
            Macro::TimeIntervalMs => "$__timeInterval_ms",
            Macro::DateFilter => "$__dateFilter",
            Macro::DateTimeFilter => "$__dateTimeFilter",
            Macro::Dt => "$__dt",
            Macro::ConditionalAll => "$__conditionalAll",
            Macro::FromTime => "$__fromTime",
            Macro::ToTime => "$__toTime",
            Macro::FromTimeMs => "$__fromTime_ms",
            Macro::ToTimeMs => "$__toTime_ms",
            Macro::IntervalS => "$__interval_s",
            Macro::IntervalMs => "$__interval_ms",
            Macro::From => "$__from",
            Macro::To => "$__to",
            Macro::AdhocFilters => "$__adhocFilters",
        }
    }

    /// The number of arguments: 0 for a name that stands for a value, 1 or 2 for a call.
    pub fn arity(self) -> usize {
        match self {
            Macro::TimeFilter
            | Macro::TimeFilterMs
            | Macro::TimeInterval
            | Macro::TimeIntervalMs
            | Macro::DateFilter => 1,
            Macro::DateTimeFilter | Macro::Dt | Macro::ConditionalAll => 2,
            _ => 0,
        }
    }

    /// Tells if the macro takes at least one argument.
    pub fn takes_argument(self) -> bool {
        self.arity() > 0
    }

    /// Tells if the macro needs the interval of 7.4. A variable query has no interval.
    pub fn needs_interval(self) -> bool {
        matches!(self, Macro::TimeInterval | Macro::TimeIntervalMs | Macro::IntervalS | Macro::IntervalMs)
    }
}

/// `toDateTime(F)` of 7.10.
pub fn from_time_text(range: &TimeRange) -> String {
    format!("toDateTime({})", range.from_seconds())
}

/// `toDateTime(T)` of 7.10.
pub fn to_time_text(range: &TimeRange) -> String {
    format!("toDateTime({})", range.to_seconds())
}

/// `fromUnixTimestamp64Milli(F_ms)` of 7.10.
pub fn from_time_ms_text(range: &TimeRange) -> String {
    format!("fromUnixTimestamp64Milli({})", range.from_ms())
}

/// `fromUnixTimestamp64Milli(T_ms)` of 7.10.
pub fn to_time_ms_text(range: &TimeRange) -> String {
    format!("fromUnixTimestamp64Milli({})", range.to_ms())
}

/// The expansion of `$__timeFilter(expr)`: `expr >= toDateTime(F) AND expr <= toDateTime(T)`.
pub fn time_filter_text(range: &TimeRange, expr: &str) -> String {
    format!("{expr} >= toDateTime({}) AND {expr} <= toDateTime({})", range.from_seconds(), range.to_seconds())
}

/// The expansion of `$__timeFilter_ms(expr)`.
pub fn time_filter_ms_text(range: &TimeRange, expr: &str) -> String {
    format!(
        "{expr} >= fromUnixTimestamp64Milli({}) AND {expr} <= fromUnixTimestamp64Milli({})",
        range.from_ms(),
        range.to_ms()
    )
}

/// The expansion of `$__timeInterval(expr)`: `toStartOfInterval(toDateTime(expr), INTERVAL S second)`.
pub fn time_interval_text(seconds: u64, expr: &str) -> String {
    format!("toStartOfInterval(toDateTime({expr}), INTERVAL {seconds} second)")
}

/// The expansion of `$__timeInterval_ms(expr)`.
pub fn time_interval_ms_text(interval_ms: u64, expr: &str) -> String {
    format!("toStartOfInterval(toDateTime64({expr}, 3), INTERVAL {interval_ms} millisecond)")
}

/// The calendar date `YYYY-MM-DD` of an epoch time in milliseconds, in UTC (7.3, D10).
///
/// The range of 7.3 starts at 0 and ends before the year 2107, so the date has four digits.
pub fn utc_date(ms: i64) -> String {
    // The algorithm of Howard Hinnant for the civil date of a day count.
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The expansion of `$__dateFilter(expr)`: `expr >= toDate('D1') AND expr <= toDate('D2')`. `D1` and
/// `D2` are the UTC dates of the start and of the end of the range.
pub fn date_filter_text(range: &TimeRange, expr: &str) -> String {
    format!("{expr} >= toDate('{}') AND {expr} <= toDate('{}')", utc_date(range.from_ms()), utc_date(range.to_ms()))
}

/// The expansion of `$__dateTimeFilter(date_expr, time_expr)`: `(date filter) AND (time filter)`.
pub fn date_time_filter_text(range: &TimeRange, date_expr: &str, time_expr: &str) -> String {
    format!("({}) AND ({})", date_filter_text(range, date_expr), time_filter_text(range, time_expr))
}

/// One of the macros of the dialect `postgres` (7.22). Each name starts with `$__`.
///
/// The set is apart from [`Macro`] on purpose. A `match` on [`Macro`] in the code of the dialect
/// `clickhouse` can then never reach a macro of the dialect `postgres`. [`lookup`] chooses the
/// set from the [`Dialect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PgMacro {
    /// `$__time(expr)`
    Time,
    /// `$__timeEpoch(expr)`
    TimeEpoch,
    /// `$__timeFilter(expr)`
    TimeFilter,
    /// `$__timeFrom`
    TimeFrom,
    /// `$__timeTo`
    TimeTo,
    /// `$__timeGroup(expr, interval)`
    TimeGroup,
    /// `$__timeGroupAlias(expr, interval)`
    TimeGroupAlias,
    /// `$__unixEpochFilter(expr)`
    UnixEpochFilter,
    /// `$__unixEpochFrom`
    UnixEpochFrom,
    /// `$__unixEpochTo`
    UnixEpochTo,
    /// `$__unixEpochNanoFilter(expr)`
    UnixEpochNanoFilter,
    /// `$__unixEpochNanoFrom`
    UnixEpochNanoFrom,
    /// `$__unixEpochNanoTo`
    UnixEpochNanoTo,
    /// `$__unixEpochGroup(expr, interval)`
    UnixEpochGroup,
    /// `$__unixEpochGroupAlias(expr, interval)`
    UnixEpochGroupAlias,
    /// `$__interval_s`
    IntervalS,
    /// `$__interval_ms`
    IntervalMs,
    /// `$__from`
    From,
    /// `$__to`
    To,
    /// `$__conditionalAll(condition, $variable)`
    ConditionalAll,
}

impl PgMacro {
    /// All macros, in the order of the table of 7.22.
    pub const ALL: [PgMacro; 20] = [
        PgMacro::Time,
        PgMacro::TimeEpoch,
        PgMacro::TimeFilter,
        PgMacro::TimeFrom,
        PgMacro::TimeTo,
        PgMacro::TimeGroup,
        PgMacro::TimeGroupAlias,
        PgMacro::UnixEpochFilter,
        PgMacro::UnixEpochFrom,
        PgMacro::UnixEpochTo,
        PgMacro::UnixEpochNanoFilter,
        PgMacro::UnixEpochNanoFrom,
        PgMacro::UnixEpochNanoTo,
        PgMacro::UnixEpochGroup,
        PgMacro::UnixEpochGroupAlias,
        PgMacro::IntervalS,
        PgMacro::IntervalMs,
        PgMacro::From,
        PgMacro::To,
        PgMacro::ConditionalAll,
    ];

    /// Finds the macro of a name. The name starts with `__` and has no `$`. The match is exact:
    /// the case of each letter counts. A name of the dialect `clickhouse` that 7.22 does not list
    /// (`__timeFilter_ms`, `__timeInterval`, `__dateFilter`, `__fromTime`) gives `None`.
    pub fn from_name(name: &str) -> Option<PgMacro> {
        PgMacro::ALL.into_iter().find(|m| m.name().strip_prefix('$') == Some(name))
    }

    /// Gives the name of the macro with the `$` and with the `__`.
    pub fn name(self) -> &'static str {
        match self {
            PgMacro::Time => "$__time",
            PgMacro::TimeEpoch => "$__timeEpoch",
            PgMacro::TimeFilter => "$__timeFilter",
            PgMacro::TimeFrom => "$__timeFrom",
            PgMacro::TimeTo => "$__timeTo",
            PgMacro::TimeGroup => "$__timeGroup",
            PgMacro::TimeGroupAlias => "$__timeGroupAlias",
            PgMacro::UnixEpochFilter => "$__unixEpochFilter",
            PgMacro::UnixEpochFrom => "$__unixEpochFrom",
            PgMacro::UnixEpochTo => "$__unixEpochTo",
            PgMacro::UnixEpochNanoFilter => "$__unixEpochNanoFilter",
            PgMacro::UnixEpochNanoFrom => "$__unixEpochNanoFrom",
            PgMacro::UnixEpochNanoTo => "$__unixEpochNanoTo",
            PgMacro::UnixEpochGroup => "$__unixEpochGroup",
            PgMacro::UnixEpochGroupAlias => "$__unixEpochGroupAlias",
            PgMacro::IntervalS => "$__interval_s",
            PgMacro::IntervalMs => "$__interval_ms",
            PgMacro::From => "$__from",
            PgMacro::To => "$__to",
            PgMacro::ConditionalAll => "$__conditionalAll",
        }
    }

    /// The number of arguments: 0 for a name that stands for a value, 1 or 2 for a call.
    pub fn arity(self) -> usize {
        match self {
            PgMacro::Time
            | PgMacro::TimeEpoch
            | PgMacro::TimeFilter
            | PgMacro::UnixEpochFilter
            | PgMacro::UnixEpochNanoFilter => 1,
            PgMacro::TimeGroup
            | PgMacro::TimeGroupAlias
            | PgMacro::UnixEpochGroup
            | PgMacro::UnixEpochGroupAlias
            | PgMacro::ConditionalAll => 2,
            _ => 0,
        }
    }

    /// Tells if the macro takes at least one argument.
    pub fn takes_argument(self) -> bool {
        self.arity() > 0
    }

    /// Tells if a macro with no argument can also be written with empty parentheses (7.22). The
    /// macros `$__interval_s`, `$__interval_ms`, `$__from` and `$__to` take no parentheses.
    pub fn accepts_parentheses(self) -> bool {
        matches!(
            self,
            PgMacro::TimeFrom
                | PgMacro::TimeTo
                | PgMacro::UnixEpochFrom
                | PgMacro::UnixEpochTo
                | PgMacro::UnixEpochNanoFrom
                | PgMacro::UnixEpochNanoTo
        )
    }

    /// Tells if the macro is a group macro: its second argument is the interval of 7.23.
    pub fn is_group(self) -> bool {
        matches!(
            self,
            PgMacro::TimeGroup | PgMacro::TimeGroupAlias | PgMacro::UnixEpochGroup | PgMacro::UnixEpochGroupAlias
        )
    }

    /// Tells if the macro needs the interval of 7.4 by its name alone. A variable query has no
    /// interval. A group macro needs it only when its argument is the built-in `$__interval`
    /// (7.23), so this function gives `false` for it.
    pub fn needs_interval(self) -> bool {
        matches!(self, PgMacro::IntervalS | PgMacro::IntervalMs)
    }

    /// The expansion of a macro with no argument (7.22). `interval` is `None` in a variable query.
    /// A macro that takes an argument, or a macro that needs the interval when there is none,
    /// gives `None`.
    pub fn expand_value(self, range: &TimeRange, interval: Option<Interval>) -> Option<String> {
        Some(match self {
            PgMacro::TimeFrom => pg_time_from_text(range),
            PgMacro::TimeTo => pg_time_to_text(range),
            PgMacro::UnixEpochFrom => range.from_seconds().to_string(),
            PgMacro::UnixEpochTo => range.to_seconds().to_string(),
            PgMacro::UnixEpochNanoFrom => nano_text(range.from_ms()),
            PgMacro::UnixEpochNanoTo => nano_text(range.to_ms()),
            PgMacro::IntervalS => interval?.seconds.to_string(),
            PgMacro::IntervalMs => interval?.ms.to_string(),
            PgMacro::From => range.from_ms().to_string(),
            PgMacro::To => range.to_ms().to_string(),
            _ => return None,
        })
    }

    /// The expansion of a call (7.22). `expr` is the text of the first argument after its own
    /// expansion. `group` is the text `G` of 7.23 and is the only thing that a group macro reads
    /// of its second argument. A macro with no argument, a group macro without `group`, and
    /// `$__conditionalAll` give `None`: the caller handles the last one.
    pub fn expand_call(self, range: &TimeRange, expr: &str, group: Option<&str>) -> Option<String> {
        Some(match self {
            PgMacro::Time => pg_time_text(expr),
            PgMacro::TimeEpoch => pg_time_epoch_text(expr),
            PgMacro::TimeFilter => pg_time_filter_text(range, expr),
            PgMacro::UnixEpochFilter => pg_unix_epoch_filter_text(range, expr),
            PgMacro::UnixEpochNanoFilter => pg_unix_epoch_nano_filter_text(range, expr),
            PgMacro::TimeGroup => pg_time_group_text(expr, group?),
            PgMacro::TimeGroupAlias => format!("{} AS \"time\"", pg_time_group_text(expr, group?)),
            PgMacro::UnixEpochGroup => pg_unix_epoch_group_text(expr, group?),
            PgMacro::UnixEpochGroupAlias => format!("{} AS \"time\"", pg_unix_epoch_group_text(expr, group?)),
            _ => return None,
        })
    }
}

/// A macro of one dialect, as [`lookup`] finds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnyMacro {
    /// A macro of the dialect `clickhouse` (7.10).
    ClickHouse(Macro),
    /// A macro of the dialect `postgres` (7.22).
    Postgres(PgMacro),
}

/// Finds the macro of a name in the macro set of a dialect. The name starts with `__` and has no `$`.
pub fn lookup(dialect: Dialect, name: &str) -> Option<AnyMacro> {
    match dialect {
        Dialect::ClickHouse => Macro::from_name(name).map(AnyMacro::ClickHouse),
        Dialect::Postgres => PgMacro::from_name(name).map(AnyMacro::Postgres),
    }
}

/// `TS(t)` of 7.22: the UTC time of the epoch milliseconds `t` as `YYYY-MM-DDTHH:MM:SS[.fff]Z`. The
/// fraction is the milliseconds, without a zero at its end, and it is absent when it is 0.
pub fn pg_timestamp(ms: i64) -> String {
    let secs = ms.div_euclid(1000).rem_euclid(86_400);
    let millis = ms.rem_euclid(1000);
    let mut out = format!("{}T{:02}:{:02}:{:02}", utc_date(ms), secs / 3600, secs / 60 % 60, secs % 60);
    if millis != 0 {
        let fraction = format!("{millis:03}");
        out.push('.');
        out.push_str(fraction.trim_end_matches('0'));
    }
    out.push('Z');
    out
}

/// `N` of 7.22: epoch milliseconds times 1,000,000. The range of 7.3 keeps it in an `i64`.
fn nano_text(ms: i64) -> String {
    ms.saturating_mul(1_000_000).to_string()
}

/// The expansion of `$__time(expr)`: `expr AS "time"`.
pub fn pg_time_text(expr: &str) -> String {
    format!("{expr} AS \"time\"")
}

/// The expansion of `$__timeEpoch(expr)`: `extract(epoch from expr) as "time"`.
pub fn pg_time_epoch_text(expr: &str) -> String {
    format!("extract(epoch from {expr}) as \"time\"")
}

/// The expansion of `$__timeFilter(expr)`: `expr BETWEEN 'TS(from_ms)' AND 'TS(to_ms)'`.
pub fn pg_time_filter_text(range: &TimeRange, expr: &str) -> String {
    format!("{expr} BETWEEN {} AND {}", pg_time_from_text(range), pg_time_to_text(range))
}

/// The expansion of `$__timeFrom`: `'TS(from_ms)'`.
pub fn pg_time_from_text(range: &TimeRange) -> String {
    format!("'{}'", pg_timestamp(range.from_ms()))
}

/// The expansion of `$__timeTo`: `'TS(to_ms)'`.
pub fn pg_time_to_text(range: &TimeRange) -> String {
    format!("'{}'", pg_timestamp(range.to_ms()))
}

/// The expansion of `$__timeGroup(expr, interval)`: `floor(extract(epoch from expr)/G)*G`.
/// `group` is `G` of 7.23, as [`group_seconds_text`](super::interval::group_seconds_text) writes it.
pub fn pg_time_group_text(expr: &str, group: &str) -> String {
    format!("floor(extract(epoch from {expr})/{group})*{group}")
}

/// The expansion of `$__unixEpochFilter(expr)`: `expr >= F AND expr <= T`.
pub fn pg_unix_epoch_filter_text(range: &TimeRange, expr: &str) -> String {
    format!("{expr} >= {} AND {expr} <= {}", range.from_seconds(), range.to_seconds())
}

/// The expansion of `$__unixEpochNanoFilter(expr)`: `expr >= N1 AND expr <= N2`.
pub fn pg_unix_epoch_nano_filter_text(range: &TimeRange, expr: &str) -> String {
    format!("{expr} >= {} AND {expr} <= {}", nano_text(range.from_ms()), nano_text(range.to_ms()))
}

/// The expansion of `$__unixEpochGroup(expr, interval)`: `floor((expr)/G)*G`.
///
/// Grafana writes the parentheses around `expr`, and they are needed: without them the argument
/// `n + 1` gives `floor(n + 1/G)*G`, which is not the group of `n + 1`.
pub fn pg_unix_epoch_group_text(expr: &str, group: &str) -> String {
    format!("floor(({expr})/{group})*{group}")
}

/// The cause of a macro call that breaks 7.10.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallFault {
    /// A macro with an argument that no `(` follows directly.
    MissingParen,
    /// A macro with no argument that a `(` follows directly.
    UnexpectedParen,
    /// The `(` has no `)` that closes it.
    Unclosed,
    /// The call has an empty argument, or a number of arguments that the macro does not take.
    WrongArgumentCount,
    /// A macro of the dialect `postgres` with no argument that has more than blanks between its `(`
    /// and its `)` (7.22). The dialect `clickhouse` never gives this fault.
    UnexpectedArgument,
}

impl CallFault {
    /// Gives a short English message for the macro.
    pub fn message(self, m: Macro) -> String {
        self.message_for(m.name(), m.arity())
    }

    /// Gives a short English message for a macro of any dialect. `name` has the `$` and the `__`.
    pub fn message_for(self, name: &str, arity: usize) -> String {
        match self {
            CallFault::MissingParen => format!("{name} needs one argument: a ( must follow the name"),
            CallFault::UnexpectedParen => format!("{name} takes no argument: no ( can follow the name"),
            CallFault::Unclosed => format!("the ( of {name} has no ) that closes it"),
            CallFault::WrongArgumentCount => match arity {
                2 => format!("{name} needs exactly two arguments, and none is blank"),
                _ => format!("{name} needs exactly one argument that is not blank"),
            },
            CallFault::UnexpectedArgument => format!("{name} takes no argument: only blanks can be between ( and )"),
        }
    }
}

/// A macro call. The token indexes refer to the token list of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call {
    /// The index of the `)` token that closes the call.
    pub close: usize,
    /// The index of the first token of the first argument after the removal of the blank tokens at the start.
    pub arg_start: usize,
    /// The index after the last token of the first argument after the removal of the blank tokens at the end.
    pub arg_end: usize,
    /// The token range of the second argument (the same trim rule), for a macro with two arguments.
    pub second: Option<(usize, usize)>,
}

/// Reads the call of the macro `m` whose name ends at the byte offset `after`.
///
/// The `(` MUST be the next byte. The engine counts only `(` and `)` punctuation tokens. A `,`
/// punctuation token at depth 0 inside the parentheses separates two arguments. `limit` is the
/// index after the last token that the call can use. A macro with no argument gives `Ok(None)`.
/// A macro with one argument has no comma. A macro with two arguments has exactly one comma.
pub fn parse_call(
    m: Macro,
    src: &str,
    tokens: &[Token],
    after: usize,
    limit: usize,
) -> Result<Option<Call>, CallFault> {
    let next_is_paren = src.as_bytes().get(after) == Some(&b'(');
    if !m.takes_argument() {
        return if next_is_paren { Err(CallFault::UnexpectedParen) } else { Ok(None) };
    }
    parse_arguments(m.arity(), src, tokens, after, limit, None).map(Some)
}

/// Runs [`parse_call`] with the parentheses of [`Parens::of`] on all the tokens. It gives the same result, with
/// no scan of the tokens after the name. The checks at save use it, so a text with many macros stays linear.
pub fn parse_call_matched(
    m: Macro,
    src: &str,
    tokens: &[Token],
    after: usize,
    parens: &Parens,
) -> Result<Option<Call>, CallFault> {
    let next_is_paren = src.as_bytes().get(after) == Some(&b'(');
    if !m.takes_argument() {
        return if next_is_paren { Err(CallFault::UnexpectedParen) } else { Ok(None) };
    }
    parse_arguments(m.arity(), src, tokens, after, tokens.len(), Some(parens)).map(Some)
}

/// Runs [`parse_pg_call`] with the parentheses of [`Parens::of`] on all the tokens. It gives the same result.
pub fn parse_pg_call_matched(
    m: PgMacro,
    src: &str,
    tokens: &[Token],
    after: usize,
    parens: &Parens,
) -> Result<Option<Call>, CallFault> {
    if m.takes_argument() {
        return parse_arguments(m.arity(), src, tokens, after, tokens.len(), Some(parens)).map(Some);
    }
    parse_pg_call(m, src, tokens, after, tokens.len())
}

/// One `(` punctuation token and what its call needs: the `)` that closes it, and its commas at depth 1. The
/// token indexes are `u32` (a text has at most `MAX_SQL_BYTES` tokens), and `NONE` marks an absent index.
#[derive(Debug, Clone, Copy)]
struct Group {
    open: u32,
    close: u32,
    first_comma: u32,
    commas: u32,
}

/// The absent token index of a [`Group`].
const NONE: u32 = u32::MAX;

impl Group {
    fn index(i: u32) -> Option<usize> {
        (i != NONE).then_some(i as usize)
    }
}

/// The `(` and `)` punctuation tokens of a whole token list, matched in one pass. Each `(` knows the `)` that
/// closes it and its commas at depth 1, as the scan of [`parse_call`] finds them. The memory is one small item
/// for each `(`.
#[derive(Debug, Clone, Default)]
pub struct Parens {
    /// The groups, in the order of their `(`.
    groups: Vec<Group>,
}

impl Parens {
    /// Matches the parentheses of `tokens`, the token list of `src`. A list of more than `u32::MAX - 1` tokens
    /// gives no group, and each call then scans as [`parse_call`] does.
    pub fn of(src: &str, tokens: &[Token]) -> Parens {
        let mut groups: Vec<Group> = Vec::new();
        if u32::try_from(tokens.len()).map_or(true, |n| n == NONE) {
            return Parens { groups };
        }
        let mut stack: Vec<u32> = Vec::new();
        for (i, t) in (0u32..).zip(tokens) {
            if t.kind != TokenKind::Punct {
                continue;
            }
            match src.as_bytes().get(t.start) {
                Some(b'(') => {
                    stack.push(u32::try_from(groups.len()).unwrap_or(NONE));
                    groups.push(Group { open: i, close: NONE, first_comma: NONE, commas: 0 });
                }
                Some(b')') => {
                    if let Some(g) = stack.pop().and_then(|g| groups.get_mut(g as usize)) {
                        g.close = i;
                    }
                }
                Some(b',') => {
                    if let Some(g) = stack.last().and_then(|&g| groups.get_mut(g as usize)) {
                        if g.first_comma == NONE {
                            g.first_comma = i;
                        }
                        g.commas = g.commas.saturating_add(1);
                    }
                }
                _ => {}
            }
        }
        Parens { groups }
    }

    /// The `)`, the first comma and the count of commas of the `(` at the token index `open`.
    fn group(&self, open: usize) -> Option<(Option<usize>, Option<usize>, usize)> {
        let open = u32::try_from(open).ok()?;
        let at = self.groups.binary_search_by_key(&open, |g| g.open).ok()?;
        let g = self.groups.get(at)?;
        Some((Group::index(g.close), Group::index(g.first_comma), g.commas as usize))
    }
}

/// Reads the call of a macro of the dialect `postgres` whose name ends at the byte offset `after`.
///
/// The rules of [`parse_call`] apply, with one change for the macros of 7.22 that take no argument
/// and that [`PgMacro::accepts_parentheses`] marks: they can be written `$__timeFrom()`, with blanks
/// between the parentheses. The call then gives `Some(Call)` with an empty argument range, and
/// `close` is the `)`. The other macros with no argument take no `(`.
pub fn parse_pg_call(
    m: PgMacro,
    src: &str,
    tokens: &[Token],
    after: usize,
    limit: usize,
) -> Result<Option<Call>, CallFault> {
    let next_is_paren = src.as_bytes().get(after) == Some(&b'(');
    if m.takes_argument() {
        return parse_arguments(m.arity(), src, tokens, after, limit, None).map(Some);
    }
    if !next_is_paren {
        return Ok(None);
    }
    if !m.accepts_parentheses() {
        return Err(CallFault::UnexpectedParen);
    }
    let limit = limit.min(tokens.len());
    let open = tokens.partition_point(|t| t.start < after);
    if open >= limit || tokens[open].start != after || tokens[open].kind != TokenKind::Punct {
        return Err(CallFault::UnexpectedParen);
    }
    for (i, t) in tokens.iter().enumerate().take(limit).skip(open + 1) {
        match t.kind {
            TokenKind::Blank => {}
            TokenKind::Punct if src.as_bytes().get(t.start) == Some(&b')') => {
                return Ok(Some(Call { close: i, arg_start: i, arg_end: i, second: None }));
            }
            _ => return Err(CallFault::UnexpectedArgument),
        }
    }
    Err(CallFault::Unclosed)
}

/// Reads the arguments of a call with `arity` 1 or 2. The `(` MUST be the next byte at `after`. With `parens`
/// (made on all the tokens, and `limit` equal to their count), the `)` and the commas come from it.
fn parse_arguments(
    arity: usize,
    src: &str,
    tokens: &[Token],
    after: usize,
    limit: usize,
    parens: Option<&Parens>,
) -> Result<Call, CallFault> {
    if src.as_bytes().get(after) != Some(&b'(') {
        return Err(CallFault::MissingParen);
    }
    let limit = limit.min(tokens.len());
    let open = tokens.partition_point(|t| t.start < after);
    if open >= limit || tokens[open].start != after || tokens[open].kind != TokenKind::Punct {
        return Err(CallFault::MissingParen);
    }
    // The `)` of the call, its first comma at depth 1 and the count of these commas.
    let (close, first_comma, comma_count) = match parens.and_then(|p| p.group(open)) {
        Some(found) => found,
        None => {
            let mut depth = 1usize;
            let (mut first_comma, mut comma_count) = (None, 0usize);
            let mut close = None;
            for (i, t) in tokens.iter().enumerate().take(limit).skip(open + 1) {
                if t.kind != TokenKind::Punct {
                    continue;
                }
                match src.as_bytes().get(t.start) {
                    Some(b'(') => depth += 1,
                    Some(b')') => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(i);
                            break;
                        }
                    }
                    Some(b',') if depth == 1 => {
                        first_comma = first_comma.or(Some(i));
                        comma_count += 1;
                    }
                    _ => {}
                }
            }
            (close, first_comma, comma_count)
        }
    };
    let close = close.ok_or(CallFault::Unclosed)?;
    let trim = |mut lo: usize, mut hi: usize| {
        while lo < hi && tokens[lo].kind == TokenKind::Blank {
            lo += 1;
        }
        while hi > lo && tokens[hi - 1].kind == TokenKind::Blank {
            hi -= 1;
        }
        (lo, hi)
    };
    if comma_count + 1 != arity {
        return Err(CallFault::WrongArgumentCount);
    }
    let first_end = first_comma.unwrap_or(close);
    let (arg_start, arg_end) = trim(open + 1, first_end);
    if arg_start == arg_end {
        return Err(CallFault::WrongArgumentCount);
    }
    let second = if let (2, Some(comma)) = (arity, first_comma) {
        let (lo, hi) = trim(comma + 1, close);
        if lo == hi {
            return Err(CallFault::WrongArgumentCount);
        }
        Some((lo, hi))
    } else {
        None
    };
    Ok(Call { close, arg_start, arg_end, second })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sql::interval::check_time_range;
    use crate::sql::lexer::lex_all;

    fn call(src: &str, m: Macro) -> Result<Option<Call>, CallFault> {
        let tokens = lex_all(src);
        let name_end = src.find('(').unwrap_or(src.len().min(m.name().len()));
        let name_end = if src.starts_with(m.name()) { m.name().len() } else { name_end };
        parse_call(m, src, &tokens, name_end, tokens.len())
    }

    #[test]
    fn the_matched_parentheses_give_the_result_of_the_scan() {
        // Texts of random parts: parentheses, commas, brackets, blanks, and parentheses in strings and comments.
        const PARTS: [&str; 12] = ["(", ")", ",", " ", "a", "'(,)'", "/* ( */", "[", "]", "-- ( ,\n", "f(", "$x"];
        let mut seed = 0x2545_f491_u64;
        for _ in 0..3000 {
            let mut src = String::new();
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            for k in 0..(seed % 24) {
                let r = seed.rotate_left(u32::try_from(k * 5).unwrap_or(0)) % PARTS.len() as u64;
                src.push_str(PARTS[usize::try_from(r).unwrap_or(0)]);
            }
            let tokens = lex_all(&src);
            let parens = Parens::of(&src, &tokens);
            for t in &tokens {
                for arity in [1, 2] {
                    let scan = super::parse_arguments(arity, &src, &tokens, t.start, tokens.len(), None);
                    let matched = super::parse_arguments(arity, &src, &tokens, t.start, tokens.len(), Some(&parens));
                    assert_eq!(scan, matched, "{src:?} at {}", t.start);
                }
            }
        }
    }

    #[test]
    fn names() {
        for (n, m) in [
            ("__timeFilter", Macro::TimeFilter),
            ("__timeInterval", Macro::TimeInterval),
            ("__fromTime", Macro::FromTime),
            ("__toTime", Macro::ToTime),
            ("__interval_s", Macro::IntervalS),
            ("__timeFilter_ms", Macro::TimeFilterMs),
            ("__timeInterval_ms", Macro::TimeIntervalMs),
            ("__dateFilter", Macro::DateFilter),
            ("__dateTimeFilter", Macro::DateTimeFilter),
            ("__dt", Macro::Dt),
            ("__conditionalAll", Macro::ConditionalAll),
            ("__fromTime_ms", Macro::FromTimeMs),
            ("__toTime_ms", Macro::ToTimeMs),
            ("__interval_ms", Macro::IntervalMs),
            ("__from", Macro::From),
            ("__to", Macro::To),
        ] {
            assert_eq!(Macro::from_name(n), Some(m));
            assert_eq!(m.name(), format!("${n}"));
        }
        for n in [
            "timeFilter",
            "__TimeFilter",
            "__range",
            "__range_s",
            "__range_ms",
            "__interval",
            "__conditionalTest",
            "__timezone",
            "",
        ] {
            assert_eq!(Macro::from_name(n), None, "{n}");
        }
        assert!(Macro::TimeFilter.takes_argument() && !Macro::ToTime.takes_argument());
        assert_eq!(Macro::DateTimeFilter.arity(), 2);
        assert_eq!(Macro::Dt.arity(), 2);
        assert!(!Macro::Dt.needs_interval());
        assert_eq!(Macro::from_name("__DT"), None);
        assert_eq!(Macro::ConditionalAll.arity(), 2);
        assert_eq!(Macro::DateFilter.arity(), 1);
        assert_eq!(Macro::From.arity(), 0);
        for m in [Macro::IntervalS, Macro::IntervalMs, Macro::TimeInterval, Macro::TimeIntervalMs] {
            assert!(m.needs_interval(), "{m:?}");
        }
        for m in [Macro::TimeFilter, Macro::From, Macro::To, Macro::DateFilter, Macro::ConditionalAll] {
            assert!(!m.needs_interval(), "{m:?}");
        }
    }

    #[test]
    fn dates_in_utc() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(86_399_999), "1970-01-01");
        assert_eq!(utc_date(86_400_000), "1970-01-02");
        assert_eq!(utc_date(1_794_211_200_000), "2026-11-09");
        assert_eq!(utc_date(1_794_265_200_000), "2026-11-09");
        assert_eq!(utc_date(1_794_272_400_000), "2026-11-10");
        // 2024 is a leap year.
        assert_eq!(utc_date(1_709_164_800_000), "2024-02-29");
        assert_eq!(utc_date(1_709_251_200_000), "2024-03-01");
        assert_eq!(utc_date(4_294_967_295_999), "2106-02-07");
    }

    #[test]
    fn exact_texts_of_the_millisecond_family() {
        let r = check_time_range(1_794_211_200_123, 1_794_232_800_987).unwrap();
        assert_eq!(from_time_ms_text(&r), "fromUnixTimestamp64Milli(1794211200123)");
        assert_eq!(to_time_ms_text(&r), "fromUnixTimestamp64Milli(1794232800987)");
        assert_eq!(
            time_filter_ms_text(&r, "t"),
            "t >= fromUnixTimestamp64Milli(1794211200123) AND t <= fromUnixTimestamp64Milli(1794232800987)"
        );
        assert_eq!(time_interval_ms_text(50, "t"), "toStartOfInterval(toDateTime64(t, 3), INTERVAL 50 millisecond)");
        assert_eq!(date_filter_text(&r, "d"), "d >= toDate('2026-11-09') AND d <= toDate('2026-11-09')");
        assert_eq!(
            date_time_filter_text(&r, "d", "t"),
            "(d >= toDate('2026-11-09') AND d <= toDate('2026-11-09')) AND \
             (t >= toDateTime(1794211200) AND t <= toDateTime(1794232800))"
        );
    }

    #[test]
    fn exact_texts() {
        let r = check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap();
        assert_eq!(from_time_text(&r), "toDateTime(1794211200)");
        assert_eq!(to_time_text(&r), "toDateTime(1794232800)");
        assert_eq!(
            time_filter_text(&r, "event_time"),
            "event_time >= toDateTime(1794211200) AND event_time <= toDateTime(1794232800)"
        );
        assert_eq!(
            time_interval_text(20, "event_time"),
            "toStartOfInterval(toDateTime(event_time), INTERVAL 20 second)"
        );
    }

    #[test]
    fn parse_arguments() {
        let c = call("$__timeFilter( a.b )", Macro::TimeFilter).unwrap().unwrap();
        let tokens = lex_all("$__timeFilter( a.b )");
        assert_eq!(tokens[c.arg_start].start, 15);
        assert_eq!(tokens[c.arg_end - 1].end, 18);
        assert_eq!(c.close, tokens.len() - 1);
        // Parentheses of a string or a comment do not count. Nested calls and commas inside count by depth.
        assert!(call("$__timeFilter(f(')', a, b) /* ) */)", Macro::TimeFilter).unwrap().is_some());
        assert!(call("$__timeFilter(f(a, b))", Macro::TimeFilter).unwrap().is_some());
    }

    #[test]
    fn faults() {
        assert_eq!(call("$__timeFilter", Macro::TimeFilter), Err(CallFault::MissingParen));
        assert_eq!(call("$__timeFilter (a)", Macro::TimeFilter), Err(CallFault::MissingParen));
        assert_eq!(call("$__timeFilter()", Macro::TimeFilter), Err(CallFault::WrongArgumentCount));
        assert_eq!(call("$__timeFilter(  )", Macro::TimeFilter), Err(CallFault::WrongArgumentCount));
        assert_eq!(call("$__timeFilter(a, b)", Macro::TimeFilter), Err(CallFault::WrongArgumentCount));
        assert_eq!(call("$__dateTimeFilter(a)", Macro::DateTimeFilter), Err(CallFault::WrongArgumentCount));
        assert_eq!(call("$__dateTimeFilter(a, b, c)", Macro::DateTimeFilter), Err(CallFault::WrongArgumentCount));
        assert_eq!(call("$__dateTimeFilter(a, )", Macro::DateTimeFilter), Err(CallFault::WrongArgumentCount));
        assert_eq!(call("$__dateTimeFilter( , b)", Macro::DateTimeFilter), Err(CallFault::WrongArgumentCount));
        assert_eq!(call("$__conditionalAll(a)", Macro::ConditionalAll), Err(CallFault::WrongArgumentCount));
        assert_eq!(call("$__timeFilter(a", Macro::TimeFilter), Err(CallFault::Unclosed));
        assert_eq!(call("$__timeFilter(a ')' ", Macro::TimeFilter), Err(CallFault::Unclosed));
        assert_eq!(call("$__fromTime()", Macro::FromTime), Err(CallFault::UnexpectedParen));
        assert_eq!(call("$__fromTime ()", Macro::FromTime), Ok(None));
        assert_eq!(call("$__toTime", Macro::ToTime), Ok(None));
        assert_eq!(call("$__from()", Macro::From), Err(CallFault::UnexpectedParen));
        assert_eq!(call("$__interval_ms()", Macro::IntervalMs), Err(CallFault::UnexpectedParen));
    }

    #[test]
    fn two_arguments() {
        let src = "$__dateTimeFilter( toDate(t, 'x'),  t )";
        let c = call(src, Macro::DateTimeFilter).unwrap().unwrap();
        let tokens = lex_all(src);
        let (lo, hi) = c.second.unwrap();
        assert_eq!(tokens[c.arg_start].start, 19);
        assert_eq!(tokens[lo].start, 36);
        assert_eq!(tokens[hi - 1].end, 37);
        assert_eq!(c.close, tokens.len() - 1);
        let one = call("$__timeFilter(a)", Macro::TimeFilter).unwrap().unwrap();
        assert_eq!(one.second, None);
    }

    fn pg_call(src: &str, m: PgMacro) -> Result<Option<Call>, CallFault> {
        let tokens = lex_all(src);
        let name_end = src.find('(').unwrap_or(src.len());
        let name_end = if src.starts_with(m.name()) { m.name().len() } else { name_end };
        parse_pg_call(m, src, &tokens, name_end, tokens.len())
    }

    #[test]
    fn pg_names_and_arities() {
        let table = [
            ("__time", PgMacro::Time, 1),
            ("__timeEpoch", PgMacro::TimeEpoch, 1),
            ("__timeFilter", PgMacro::TimeFilter, 1),
            ("__timeFrom", PgMacro::TimeFrom, 0),
            ("__timeTo", PgMacro::TimeTo, 0),
            ("__timeGroup", PgMacro::TimeGroup, 2),
            ("__timeGroupAlias", PgMacro::TimeGroupAlias, 2),
            ("__unixEpochFilter", PgMacro::UnixEpochFilter, 1),
            ("__unixEpochFrom", PgMacro::UnixEpochFrom, 0),
            ("__unixEpochTo", PgMacro::UnixEpochTo, 0),
            ("__unixEpochNanoFilter", PgMacro::UnixEpochNanoFilter, 1),
            ("__unixEpochNanoFrom", PgMacro::UnixEpochNanoFrom, 0),
            ("__unixEpochNanoTo", PgMacro::UnixEpochNanoTo, 0),
            ("__unixEpochGroup", PgMacro::UnixEpochGroup, 2),
            ("__unixEpochGroupAlias", PgMacro::UnixEpochGroupAlias, 2),
            ("__interval_s", PgMacro::IntervalS, 0),
            ("__interval_ms", PgMacro::IntervalMs, 0),
            ("__from", PgMacro::From, 0),
            ("__to", PgMacro::To, 0),
            ("__conditionalAll", PgMacro::ConditionalAll, 2),
        ];
        assert_eq!(table.len(), PgMacro::ALL.len());
        for (n, m, arity) in table {
            assert_eq!(PgMacro::from_name(n), Some(m), "{n}");
            assert_eq!(m.name(), format!("${n}"));
            assert_eq!(m.arity(), arity, "{n}");
            assert_eq!(m.takes_argument(), arity > 0);
            assert_eq!(lookup(Dialect::Postgres, n), Some(AnyMacro::Postgres(m)));
        }
        // A name of the dialect clickhouse that 7.22 does not list, and near misses.
        for n in [
            "__timeFilter_ms",
            "__timeInterval",
            "__timeInterval_ms",
            "__dateFilter",
            "__dateTimeFilter",
            "__fromTime",
            "__toTime",
            "__fromTime_ms",
            "__toTime_ms",
            "__interval",
            "__range",
            "__timefilter",
            "__TimeFilter",
            "__timeGroup_ms",
            "__unixEpochNano",
            "timeFilter",
            "$__timeFilter",
            "",
        ] {
            assert_eq!(PgMacro::from_name(n), None, "{n}");
            assert_eq!(lookup(Dialect::Postgres, n), None, "{n}");
        }
        // Each dialect has its own set.
        assert_eq!(lookup(Dialect::ClickHouse, "__timeFilter_ms"), Some(AnyMacro::ClickHouse(Macro::TimeFilterMs)));
        assert_eq!(lookup(Dialect::ClickHouse, "__timeGroup"), None);
        assert_eq!(lookup(Dialect::ClickHouse, "__unixEpochFrom"), None);
        assert_eq!(lookup(Dialect::ClickHouse, "__time"), None);
        assert_eq!(Macro::from_name("__timeGroup"), None);
        assert_eq!(Macro::from_name("__unixEpochGroup"), None);
        for m in PgMacro::ALL {
            assert_eq!(m.is_group(), matches!(m.arity(), 2) && m != PgMacro::ConditionalAll, "{m:?}");
            assert_eq!(m.needs_interval(), matches!(m, PgMacro::IntervalS | PgMacro::IntervalMs));
            // Only a macro with no argument can take empty parentheses, and never a built-in value.
            if m.accepts_parentheses() {
                assert!(!m.takes_argument(), "{m:?}");
            }
        }
        for m in [PgMacro::IntervalS, PgMacro::IntervalMs, PgMacro::From, PgMacro::To] {
            assert!(!m.accepts_parentheses(), "{m:?}");
        }
    }

    #[test]
    fn pg_timestamps() {
        assert_eq!(pg_timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(pg_timestamp(1), "1970-01-01T00:00:00.001Z");
        assert_eq!(pg_timestamp(10), "1970-01-01T00:00:00.01Z");
        assert_eq!(pg_timestamp(100), "1970-01-01T00:00:00.1Z");
        assert_eq!(pg_timestamp(120), "1970-01-01T00:00:00.12Z");
        assert_eq!(pg_timestamp(999), "1970-01-01T00:00:00.999Z");
        assert_eq!(pg_timestamp(1000), "1970-01-01T00:00:01Z");
        assert_eq!(pg_timestamp(86_399_999), "1970-01-01T23:59:59.999Z");
        assert_eq!(pg_timestamp(86_400_000), "1970-01-02T00:00:00Z");
        assert_eq!(pg_timestamp(1_794_211_200_000), "2026-11-09T08:00:00Z");
        assert_eq!(pg_timestamp(1_794_211_200_125), "2026-11-09T08:00:00.125Z");
        assert_eq!(pg_timestamp(1_794_232_800_999), "2026-11-09T14:00:00.999Z");
        assert_eq!(pg_timestamp(1_709_164_800_000), "2024-02-29T00:00:00Z");
        assert_eq!(pg_timestamp(1_709_251_199_999), "2024-02-29T23:59:59.999Z");
        assert_eq!(pg_timestamp(4_294_967_295_999), "2106-02-07T06:28:15.999Z");
    }

    #[test]
    fn pg_exact_texts() {
        let r = check_time_range(1_794_211_200_125, 1_794_232_800_999).unwrap();
        assert_eq!(pg_time_text("ts"), "ts AS \"time\"");
        assert_eq!(pg_time_epoch_text("ts"), "extract(epoch from ts) as \"time\"");
        assert_eq!(
            pg_time_filter_text(&r, "ts"),
            "ts BETWEEN '2026-11-09T08:00:00.125Z' AND '2026-11-09T14:00:00.999Z'"
        );
        assert_eq!(pg_time_from_text(&r), "'2026-11-09T08:00:00.125Z'");
        assert_eq!(pg_time_to_text(&r), "'2026-11-09T14:00:00.999Z'");
        assert_eq!(pg_unix_epoch_filter_text(&r, "n"), "n >= 1794211200 AND n <= 1794232800");
        assert_eq!(pg_unix_epoch_nano_filter_text(&r, "n"), "n >= 1794211200125000000 AND n <= 1794232800999000000");
        assert_eq!(pg_time_group_text("ts", "300"), "floor(extract(epoch from ts)/300)*300");
        assert_eq!(pg_unix_epoch_group_text("n + 1", "0.5"), "floor((n + 1)/0.5)*0.5");
        // The largest range of 7.3 stays in 64 bits.
        let max = check_time_range(0, crate::sql::interval::MAX_TIME_MS).unwrap();
        assert_eq!(PgMacro::UnixEpochNanoTo.expand_value(&max, None).unwrap(), "4294967295999000000");
    }

    #[test]
    fn pg_expansions() {
        let r = check_time_range(1_794_211_200_000, 1_794_232_800_000).unwrap();
        let i = crate::sql::interval::compute_interval(&r, 1200, 1).unwrap();
        let value = |m: PgMacro| m.expand_value(&r, Some(i)).unwrap();
        assert_eq!(value(PgMacro::TimeFrom), "'2026-11-09T08:00:00Z'");
        assert_eq!(value(PgMacro::TimeTo), "'2026-11-09T14:00:00Z'");
        assert_eq!(value(PgMacro::UnixEpochFrom), "1794211200");
        assert_eq!(value(PgMacro::UnixEpochTo), "1794232800");
        assert_eq!(value(PgMacro::UnixEpochNanoFrom), "1794211200000000000");
        assert_eq!(value(PgMacro::UnixEpochNanoTo), "1794232800000000000");
        assert_eq!(value(PgMacro::IntervalS), "20");
        assert_eq!(value(PgMacro::IntervalMs), "20000");
        assert_eq!(value(PgMacro::From), "1794211200000");
        assert_eq!(value(PgMacro::To), "1794232800000");
        // A variable query has no interval.
        assert_eq!(PgMacro::IntervalS.expand_value(&r, None), None);
        assert_eq!(PgMacro::IntervalMs.expand_value(&r, None), None);
        assert!(PgMacro::TimeFrom.expand_value(&r, None).is_some());
        // A macro with an argument has no value.
        assert_eq!(PgMacro::TimeFilter.expand_value(&r, Some(i)), None);
        let call = |m: PgMacro, g: Option<&str>| m.expand_call(&r, "x", g);
        assert_eq!(call(PgMacro::Time, None).unwrap(), "x AS \"time\"");
        assert_eq!(
            call(PgMacro::TimeGroupAlias, Some("20")).unwrap(),
            "floor(extract(epoch from x)/20)*20 AS \"time\""
        );
        assert_eq!(call(PgMacro::UnixEpochGroupAlias, Some("20")).unwrap(), "floor((x)/20)*20 AS \"time\"");
        // A group macro needs G. A macro with no argument and `$__conditionalAll` give nothing.
        assert_eq!(call(PgMacro::TimeGroup, None), None);
        assert_eq!(call(PgMacro::UnixEpochGroupAlias, None), None);
        assert_eq!(call(PgMacro::TimeFrom, None), None);
        assert_eq!(call(PgMacro::ConditionalAll, None), None);
        // Each macro of the table gives a text, as a value, as a call or as the caller.
        for m in PgMacro::ALL {
            let any = m.expand_value(&r, Some(i)).is_some()
                || m.expand_call(&r, "x", Some("1")).is_some()
                || m == PgMacro::ConditionalAll;
            assert!(any, "{m:?}");
        }
    }

    #[test]
    fn pg_parse_zero_argument_forms() {
        // The macros with no argument can take empty parentheses, with blanks inside.
        for src in ["$__timeFrom()", "$__timeFrom( )", "$__timeFrom(\n\t )"] {
            let c = pg_call(src, PgMacro::TimeFrom).unwrap().unwrap();
            let tokens = lex_all(src);
            assert_eq!(c.close, tokens.len() - 1, "{src:?}");
            assert_eq!((c.arg_start, c.arg_end, c.second), (c.close, c.close, None));
        }
        for m in [
            PgMacro::TimeTo,
            PgMacro::UnixEpochFrom,
            PgMacro::UnixEpochTo,
            PgMacro::UnixEpochNanoFrom,
            PgMacro::UnixEpochNanoTo,
        ] {
            let src = format!("{}()", m.name());
            assert!(pg_call(&src, m).unwrap().is_some(), "{src}");
            assert_eq!(pg_call(m.name(), m), Ok(None), "{}", m.name());
            // A blank before the ( is not a call.
            assert_eq!(pg_call(&format!("{} ()", m.name()), m), Ok(None));
        }
        // The built-in values take no parentheses.
        for m in [PgMacro::IntervalS, PgMacro::IntervalMs, PgMacro::From, PgMacro::To] {
            assert_eq!(pg_call(&format!("{}()", m.name()), m), Err(CallFault::UnexpectedParen), "{m:?}");
            assert_eq!(pg_call(m.name(), m), Ok(None));
        }
        // Something other than blanks between the parentheses is a fault. No argument is ignored.
        assert_eq!(pg_call("$__timeFrom(1)", PgMacro::TimeFrom), Err(CallFault::UnexpectedArgument));
        assert_eq!(pg_call("$__timeFrom( 1 )", PgMacro::TimeFrom), Err(CallFault::UnexpectedArgument));
        assert_eq!(pg_call("$__timeFrom(/* c */)", PgMacro::TimeFrom), Err(CallFault::UnexpectedArgument));
        assert_eq!(pg_call("$__timeFrom(')')", PgMacro::TimeFrom), Err(CallFault::UnexpectedArgument));
        assert_eq!(pg_call("$__timeFrom((", PgMacro::TimeFrom), Err(CallFault::UnexpectedArgument));
        assert_eq!(pg_call("$__timeFrom(", PgMacro::TimeFrom), Err(CallFault::Unclosed));
        assert_eq!(pg_call("$__timeFrom(  ", PgMacro::TimeFrom), Err(CallFault::Unclosed));
        for fault in [CallFault::UnexpectedArgument, CallFault::UnexpectedParen, CallFault::Unclosed] {
            assert!(fault.message_for("$__timeFrom", 0).contains("$__timeFrom"));
        }
    }

    #[test]
    fn pg_parse_arguments() {
        let c = pg_call("$__timeFilter( a.b )", PgMacro::TimeFilter).unwrap().unwrap();
        let tokens = lex_all("$__timeFilter( a.b )");
        assert_eq!(tokens[c.arg_start].start, 15);
        assert_eq!(tokens[c.arg_end - 1].end, 18);
        // Nested calls count by depth, and a comma at depth 1 splits the group arguments.
        assert!(pg_call("$__timeFilter(date_trunc('day', ts))", PgMacro::TimeFilter).unwrap().is_some());
        let src = "$__timeGroup(coalesce(ts, now()), '5m')";
        let c = pg_call(src, PgMacro::TimeGroup).unwrap().unwrap();
        let tokens = lex_all(src);
        let (lo, hi) = c.second.unwrap();
        assert_eq!(&src[tokens[c.arg_start].start..tokens[c.arg_end - 1].end], "coalesce(ts, now())");
        assert_eq!(&src[tokens[lo].start..tokens[hi - 1].end], "'5m'");
        // A third argument (the fill value of Grafana) is a fault, also for each group macro.
        for m in [PgMacro::TimeGroup, PgMacro::TimeGroupAlias, PgMacro::UnixEpochGroup, PgMacro::UnixEpochGroupAlias] {
            let ok = format!("{}(x, '5m')", m.name());
            assert!(pg_call(&ok, m).unwrap().is_some(), "{ok}");
            for bad in [
                format!("{}(x, '5m', 0)", m.name()),
                format!("{}(x, '5m', NULL)", m.name()),
                format!("{}(x, '5m', previous)", m.name()),
                format!("{}(x)", m.name()),
                format!("{}(x, )", m.name()),
                format!("{}( , '5m')", m.name()),
                format!("{}()", m.name()),
            ] {
                assert_eq!(pg_call(&bad, m), Err(CallFault::WrongArgumentCount), "{bad}");
            }
            assert_eq!(pg_call(m.name(), m), Err(CallFault::MissingParen));
            assert_eq!(pg_call(&format!("{} (x, '5m')", m.name()), m), Err(CallFault::MissingParen));
        }
        for m in [
            PgMacro::Time,
            PgMacro::TimeEpoch,
            PgMacro::TimeFilter,
            PgMacro::UnixEpochFilter,
            PgMacro::UnixEpochNanoFilter,
        ] {
            assert_eq!(pg_call(&format!("{}()", m.name()), m), Err(CallFault::WrongArgumentCount), "{m:?}");
            assert_eq!(pg_call(&format!("{}(a, b)", m.name()), m), Err(CallFault::WrongArgumentCount), "{m:?}");
            assert_eq!(pg_call(m.name(), m), Err(CallFault::MissingParen), "{m:?}");
        }
        // The word of the dialect clickhouse stays valid in its own parse.
        assert_eq!(call("$__fromTime()", Macro::FromTime), Err(CallFault::UnexpectedParen));
    }
}
