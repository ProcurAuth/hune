//! The SQL text of a query: the lines (4.16), the rewrite of `[[name]]` (15.6) and the
//! placeholders that stand in code.

use std::collections::BTreeSet;

use hune_core::limits::MAX_SQL_BYTES;
use hune_core::sql::Dialect;
use hune_core::sql::lexer::{Token, TokenKind, lex_all_for};
use hune_core::sql::placeholders::{self, Format, Found};

use crate::item::{Ctx, Fallback};
use crate::options::SourceKind;
use crate::walk::Node;

/// A prepared SQL text.
#[derive(Debug, Clone)]
pub(crate) struct Sql {
    /// The lines of 4.16.
    pub lines: Vec<String>,
    /// The lines joined with LF.
    pub text: String,
    /// The pointer of the member in the input.
    pub pointer: String,
    /// The names of the variables that the code uses outside a literal.
    pub code_uses: BTreeSet<String>,
    /// The names of the variables that the code uses outside a literal with the format none, `raw` or `csv`.
    /// A free value in such a place must be a number (7.8).
    pub code_raw_uses: BTreeSet<String>,
    /// The kind of the source of the query (7.19). The kind `mongodb` has no SQL: its text is the object of 7.29.
    pub kind: SourceKind,
}

/// The group macros of 7.22. They take two arguments in Hune. Grafana lets a third one fill gaps.
const GROUP_MACROS: [&str; 4] = ["__timeGroup", "__timeGroupAlias", "__unixEpochGroup", "__unixEpochGroupAlias"];

/// Tells for each token run the byte range where `[[name]]` is read: code and strings. In the
/// dialect `postgres`, an `E` string is a string too (7.20).
fn text_ranges(src: &str, tokens: &[Token]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        if t.kind.is_code() {
            let mut j = i;
            while j < tokens.len() && tokens[j].kind.is_code() {
                j += 1;
            }
            ranges.push((t.start, tokens[j - 1].end));
            i = j;
            continue;
        }
        if matches!(t.kind, TokenKind::String | TokenKind::EString)
            && t.closed
            && let Some(range) = t.content_range(src)
        {
            ranges.push(range);
        }
        i += 1;
    }
    ranges
}

/// Rewrites `[[name]]` to `${name}` when `name` is a variable. Gives the new text and the names.
fn rewrite_double_brackets(dialect: Dialect, src: &str, vars: &BTreeSet<String>) -> (String, Vec<String>) {
    let tokens = lex_all_for(dialect, src);
    let mut edits: Vec<(usize, String)> = Vec::new();
    for (lo, hi) in text_ranges(src, &tokens) {
        let mut p = lo;
        while let Some((at, name)) = placeholders::next_double_bracket(src, p, hi) {
            p = at + 2 + name.len() + 2;
            if vars.contains(name) {
                edits.push((at, name.to_owned()));
            }
        }
    }
    if edits.is_empty() {
        return (src.to_owned(), Vec::new());
    }
    let mut out = String::with_capacity(src.len());
    let mut last = 0;
    let mut names = Vec::new();
    for (at, name) in edits {
        out.push_str(&src[last..at]);
        out.push_str("${");
        out.push_str(&name);
        out.push('}');
        last = at + 2 + name.len() + 2;
        if !names.contains(&name) {
            names.push(name);
        }
    }
    out.push_str(&src[last..]);
    (out, names)
}

/// Finds the variables that the code uses outside a string, a quoted name, a comment and a heredoc.
fn code_uses(dialect: Dialect, src: &str, vars: &BTreeSet<String>) -> BTreeSet<String> {
    code_uses_with(dialect, src, vars, |_| true)
}

/// Finds the variables that the code uses outside a literal with the format none, `raw` or `csv` (7.8).
fn code_raw_uses(dialect: Dialect, src: &str, vars: &BTreeSet<String>) -> BTreeSet<String> {
    code_uses_with(dialect, src, vars, |f| matches!(f, None | Some(Format::Raw | Format::Csv)))
}

fn code_uses_with(
    dialect: Dialect,
    src: &str,
    vars: &BTreeSet<String>,
    format_matches: impl Fn(Option<Format>) -> bool,
) -> BTreeSet<String> {
    let tokens = lex_all_for(dialect, src);
    let mut uses = BTreeSet::new();
    let mut i = 0;
    while i < tokens.len() {
        if !tokens[i].kind.is_code() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < tokens.len() && tokens[j].kind.is_code() {
            j += 1;
        }
        let (lo, hi) = (tokens[i].start, tokens[j - 1].end);
        let mut p = lo;
        while let Some(found) = placeholders::next_for(dialect, src, p, hi) {
            p = found.end();
            if let Found::Placeholder(ph) = found
                && !ph.name.starts_with("__")
                && vars.contains(ph.name)
                && format_matches(ph.format)
            {
                uses.insert(ph.name.to_owned());
            }
        }
        i = j;
    }
    uses
}

/// One `(` punctuation token: its offset, the offsets of its first two commas at depth 1, and the offset of the
/// `)` that closes it.
#[derive(Debug, Clone, Copy)]
struct Group {
    open: usize,
    commas: [Option<usize>; 2],
    close: Option<usize>,
}

/// The `(` and `)` punctuation tokens of a whole token list, matched in one pass, as `Parens` of `hune-core`
/// does (ID-K313). The engine counts only the `(`, `)` and `,` punctuation tokens. The groups are in the order
/// of their `(`, so a call finds its group with a binary search. Thus a text with many calls stays linear.
struct Groups(Vec<Group>);

impl Groups {
    /// Matches the parentheses of `tokens`, the token list of `src`. A `)` with no open `(` closes nothing.
    fn of(src: &str, tokens: &[Token]) -> Groups {
        let mut groups: Vec<Group> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for t in tokens.iter().filter(|t| t.kind == TokenKind::Punct) {
            match src.as_bytes().get(t.start) {
                Some(b'(') => {
                    stack.push(groups.len());
                    groups.push(Group { open: t.start, commas: [None, None], close: None });
                }
                Some(b')') => {
                    if let Some(g) = stack.pop() {
                        groups[g].close = Some(t.start);
                    }
                }
                Some(b',') => {
                    if let Some(&g) = stack.last()
                        && let Some(free) = groups[g].commas.iter_mut().find(|c| c.is_none())
                    {
                        *free = Some(t.start);
                    }
                }
                _ => {}
            }
        }
        Groups(groups)
    }

    /// Finds the third argument of the call whose name ends at `after` (7.22). The `(` must be the next
    /// byte. The result is the byte range from the second `,` to the `)` that closes the call. A call with
    /// two arguments or less, or with no `)`, gives `None`.
    fn third_argument(&self, after: usize) -> Option<(usize, usize)> {
        let at = self.0.binary_search_by_key(&after, |g| g.open).ok()?;
        let g = self.0[at];
        Some((g.commas[1]?, g.close?))
    }
}

/// Removes the third argument of each group macro of the dialect `postgres` (15.6). It gives the
/// new text and the number of calls that it changed. Grafana uses the third argument to fill the
/// gaps of a series, and Hune does not fill gaps (7.22). The scan reads the code only, so a macro
/// name in a string, a quoted name or a comment stays as it is.
fn strip_third_arguments(src: &str) -> (String, usize) {
    let tokens = lex_all_for(Dialect::Postgres, src);
    let mut groups: Option<Groups> = None;
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if !tokens[i].kind.is_code() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < tokens.len() && tokens[j].kind.is_code() {
            j += 1;
        }
        let (lo, hi) = (tokens[i].start, tokens[j - 1].end);
        let mut p = lo;
        while let Some(found) = placeholders::next_for(Dialect::Postgres, src, p, hi) {
            p = found.end();
            if let Found::Placeholder(ph) = found
                && !ph.braced
                && ph.format.is_none()
                && GROUP_MACROS.contains(&ph.name)
                && let Some(cut) = groups.get_or_insert_with(|| Groups::of(src, &tokens)).third_argument(ph.end)
            {
                cuts.push(cut);
            }
        }
        i = j;
    }
    if cuts.is_empty() {
        return (src.to_owned(), 0);
    }
    // The cuts nest or are apart. A cut inside an earlier cut goes away with it.
    cuts.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut out = String::with_capacity(src.len());
    let (mut last, mut count) = (0, 0);
    for (lo, hi) in cuts {
        if lo < last {
            continue;
        }
        out.push_str(&src[last..lo]);
        last = hi;
        count += 1;
    }
    out.push_str(&src[last..]);
    (out, count)
}

/// Prepares the SQL text of the member `node` (a text). The class of each fault follows 15.6.
///
/// `unusable` is the fallback when the text is empty. The other faults give the fallback "the
/// dashboard is not imported". A text with a carriage return is `changed`. The `[[name]]` of a
/// variable is `changed` to `${name}`. `dialect` is the dialect of the source of the query (7.19).
/// In the dialect `postgres`, the third argument of a group macro is `unsupported`, and the text
/// loses it (15.6).
pub(crate) fn prepare(
    ctx: &mut Ctx,
    node: &Node,
    raw: &str,
    vars: &BTreeSet<String>,
    unusable: Fallback,
    dialect: Dialect,
) -> Option<Sql> {
    let mut text = raw.to_owned();
    if text.contains('\r') {
        text = text.replace("\r\n", "\n").replace('\r', "\n");
        ctx.changed(&node.ptr, Some(node.v), "the text has a carriage return: each \\r\\n and \\r is a line end");
    }
    if text.trim().is_empty() {
        ctx.unsupported(&node.ptr, Some(node.v), "the SQL text is empty", unusable);
        return None;
    }
    let (rewritten, names) = rewrite_double_brackets(dialect, &text, vars);
    if !names.is_empty() {
        ctx.changed(
            &node.ptr,
            Some(node.v),
            &format!("the old form [[name]] is rewritten to ${{name}} for: {}", names.join(", ")),
        );
        text = rewritten;
    }
    if let Some(bad) = text.chars().find(|c| (*c < ' ' && *c != '\n' && *c != '\t') || *c == '\u{7f}') {
        ctx.unsupported(
            &node.ptr,
            Some(node.v),
            &format!("the text has the control character U+{:04X}", bad as u32),
            Fallback::DashboardNotImported,
        );
        return None;
    }
    if text.len() > MAX_SQL_BYTES {
        ctx.unsupported(
            &node.ptr,
            None,
            &format!("the SQL text has {} bytes: the limit is {MAX_SQL_BYTES}", text.len()),
            Fallback::DashboardNotImported,
        );
        return None;
    }
    if dialect == Dialect::Postgres {
        let (stripped, count) = strip_third_arguments(&text);
        if count > 0 {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                &format!(
                    "{count} group macro(s) with a third argument: Hune does not fill the gaps of a series (7.22)"
                ),
                Fallback::other("the third argument is removed, and the panel shows the gaps"),
            );
            text = stripped;
        }
    }
    let lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let uses = code_uses(dialect, &text, vars);
    let raw_uses = code_raw_uses(dialect, &text, vars);
    ctx.ok();
    Some(Sql { lines, text, pointer: node.ptr.clone(), code_uses: uses, code_raw_uses: raw_uses, kind: dialect.into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn rewrites_double_brackets_in_code_and_strings_only() {
        let (t, n) =
            rewrite_double_brackets(Dialect::ClickHouse, "SELECT '[[a]]', [[a]], [[zz]] -- [[a]]\n", &vars(&["a"]));
        assert_eq!(t, "SELECT '${a}', ${a}, [[zz]] -- [[a]]\n");
        assert_eq!(n, vec!["a".to_owned()]);
    }

    #[test]
    fn finds_uses_in_code_only() {
        let u = code_uses(
            Dialect::ClickHouse,
            "SELECT $n, '$s', ${m:csv} -- $c\n FROM t WHERE x > $__from",
            &vars(&["n", "s", "m", "c"]),
        );
        assert_eq!(u, vars(&["n", "m"]));
    }

    #[test]
    fn strips_the_third_argument_of_group_macros_in_code_only() {
        let src = "SELECT $__timeGroupAlias(ts, '1h', 0), $__unixEpochGroup(t,$__interval,NULL), '$__timeGroup(a,b,c)' FROM x -- $__timeGroup(a,b,c)\n";
        let (out, n) = strip_third_arguments(src);
        assert_eq!(n, 2);
        assert_eq!(
            out,
            "SELECT $__timeGroupAlias(ts, '1h'), $__unixEpochGroup(t,$__interval), '$__timeGroup(a,b,c)' FROM x -- $__timeGroup(a,b,c)\n"
        );
    }

    #[test]
    fn a_nested_third_argument_goes_with_the_outer_one() {
        let (out, n) = strip_third_arguments("$__timeGroup(a, '1m', $__timeGroup(b, '1s', 0))");
        assert_eq!((out.as_str(), n), ("$__timeGroup(a, '1m')", 1));
        let (out, n) = strip_third_arguments("$__timeGroup($__timeGroup(b, '1s', 0), '1m', 7)");
        assert_eq!((out.as_str(), n), ("$__timeGroup($__timeGroup(b, '1s'), '1m')", 2));
    }

    #[test]
    fn d7_many_unclosed_group_macros_take_a_linear_time() {
        // D7 (b) of the security review of 2026-11-16: each `$__timeGroup(` with no `)` read the tokens to the end
        // of the text. Minimum of 7 runs after 2 warm-ups, with a generous limit for a debug build.
        let unit = "$__timeGroup(";
        let src = unit.repeat(MAX_SQL_BYTES / unit.len());
        let mut best = std::time::Duration::MAX;
        let mut out = (String::new(), 0);
        for run in 0..9 {
            let t = std::time::Instant::now();
            out = strip_third_arguments(&src);
            if run >= 2 {
                best = best.min(t.elapsed());
            }
        }
        assert!(best < std::time::Duration::from_millis(250), "the strip took {best:?}");
        assert_eq!(out, (src.clone(), 0));
        // The same text with each call closed, and a third argument in the last one.
        let src = format!("{}a, b, c{}", unit.repeat(5_000), ")".repeat(5_000));
        let (out, n) = strip_third_arguments(&src);
        assert_eq!(n, 1);
        assert_eq!(out, format!("{}a, b{}", unit.repeat(5_000), ")".repeat(5_000)));
    }

    /// The scan of each call to the end of the text, as before the table of [`Groups`]: the reference of the test.
    fn reference_third_argument(src: &str, tokens: &[Token], after: usize) -> Option<(usize, usize)> {
        if src.as_bytes().get(after) != Some(&b'(') {
            return None;
        }
        let open = tokens.partition_point(|t| t.start < after);
        if tokens.get(open).is_none_or(|t| t.start != after || t.kind != TokenKind::Punct) {
            return None;
        }
        let (mut depth, mut commas) = (1usize, Vec::new());
        for t in tokens[open + 1..].iter().filter(|t| t.kind == TokenKind::Punct) {
            match src.as_bytes().get(t.start) {
                Some(b'(') => depth += 1,
                Some(b')') => {
                    depth -= 1;
                    if depth == 0 {
                        return (commas.len() >= 2).then(|| (commas[1], t.start));
                    }
                }
                Some(b',') if depth == 1 => commas.push(t.start),
                _ => {}
            }
        }
        None
    }

    #[test]
    fn the_table_of_groups_gives_the_result_of_the_scan_to_the_end() {
        let parts = ["(", ")", ",", " ", "a", "'x,)'", "-- ,)\n", "$__timeGroup", "$__timeGroup(", "1"];
        let mut seed: u64 = 0x5eed_d7b0;
        for _ in 0..3_000 {
            let mut src = String::new();
            for _ in 0..(seed % 40) {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                src.push_str(parts[(seed >> 33) as usize % parts.len()]);
            }
            let tokens = lex_all_for(Dialect::Postgres, &src);
            let groups = Groups::of(&src, &tokens);
            for after in 0..=src.len() {
                assert_eq!(
                    groups.third_argument(after),
                    reference_third_argument(&src, &tokens, after),
                    "{src:?} {after}"
                );
            }
        }
    }

    #[test]
    fn two_arguments_and_other_macros_stay() {
        let src = "$__timeGroup(a, '1m') $__timeFilter(a) $__timeGroupAlias (a,'1m',2) $__timeGroupX(a,b,c)";
        let (out, n) = strip_third_arguments(src);
        assert_eq!(n, 0);
        assert_eq!(out, src);
    }
}
