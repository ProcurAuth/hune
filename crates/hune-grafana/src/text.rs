//! The text panel of Grafana (15.6, 4.33a): the members `options.mode`, `options.content` and
//! `options.code`, and the members that a text panel does not use.
//!
//! The importer never reads Markdown. It cuts the text into lines, and it scans the lines to list the
//! first construct that Hune shows in another way (15.6, the list under the table). The scan is a text
//! scan: it can list a construct inside a code block. This is a known limit of the report.

use hune_core::limits::{MAX_TEXT_BYTES, MAX_TEXT_LINE_BYTES, MAX_TEXT_LINES};
use serde_json::Value;

use crate::item::{Ctx, Fallback, is_trivial};
use crate::out::Out;
use crate::util::as_flag;
use crate::walk::{Node, Walk};

/// The mode of the text after the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Markdown: the content stays as it is.
    Markdown,
    /// `html` or `code` of Grafana: the content becomes a code block.
    CodeBlock,
}

/// The deepest nesting of lists and quotes that the page shows (9.19b, `MD_MAX_DEPTH`).
const MAX_DEPTH: usize = 6;
/// The most columns of a table that the page shows (9.19b, `MD_MAX_COLUMNS`).
const MAX_COLUMNS: usize = 20;
/// The longest destination of a link, in bytes (9.19b, `MD_MAX_URL_BYTES`).
const MAX_DESTINATION_BYTES: usize = 2048;

/// The lines of a text after the cleaning, and what the cleaning changed.
#[derive(Debug, PartialEq, Eq)]
struct Prepared {
    lines: Vec<String>,
    /// The text had U+000D: each CR LF and each lone CR became one line end.
    had_cr: bool,
    /// The text had a control character other than U+0009 and U+000A: the importer removed it.
    removed_control: bool,
}

/// Cuts a text at U+000A (15.6). An empty text gives no line.
fn prepare(text: &str) -> Prepared {
    let had_cr = text.contains('\r');
    let unified = if had_cr { text.replace("\r\n", "\n").replace('\r', "\n") } else { text.to_owned() };
    let is_control = |c: char| (c <= '\u{1f}' && c != '\t' && c != '\n') || c == '\u{7f}';
    let removed_control = unified.chars().any(is_control);
    let clean: String = if removed_control { unified.chars().filter(|c| !is_control(*c)).collect() } else { unified };
    let lines = if clean.is_empty() { Vec::new() } else { clean.split('\n').map(str::to_owned).collect() };
    Prepared { lines, had_cr, removed_control }
}

/// Gives the fence of a code block: 3 backticks, or one more than the longest run of backticks (15.6).
fn fence_for(lines: &[String]) -> String {
    let longest = lines.iter().flat_map(|l| l.split(|c| c != '`')).map(str::len).max().unwrap_or(0);
    "`".repeat((longest + 1).max(3))
}

/// Puts the lines in a code block with no info string. An empty text stays empty.
fn code_block(lines: Vec<String>) -> Vec<String> {
    if lines.is_empty() {
        return lines;
    }
    let fence = fence_for(&lines);
    let mut out = Vec::with_capacity(lines.len() + 2);
    out.push(fence.clone());
    out.extend(lines);
    out.push(fence);
    out
}

/// Reads the `options` of a text panel and gives the object `options` of Hune (4.33a).
///
/// `options_ptr` is the pointer that the items of an absent member use.
pub(crate) fn options_out(ctx: &mut Ctx, options: Option<Node>, options_ptr: &str) -> Out {
    let options = options.filter(|n| !n.is_null());
    let mut walk = options.as_ref().and_then(|n| {
        let w = Walk::new(n);
        if w.is_none() {
            ctx.unsupported(&n.ptr, Some(n.v), "the options must be an object", Fallback::other("empty text"));
        }
        w
    });
    let mode_node = take_member(&mut walk, "mode");
    let content_node = take_member(&mut walk, "content");
    let code_node = take_member(&mut walk, "code");
    if let Some(w) = walk {
        w.finish(ctx);
    }
    let mode = mode_out(ctx, mode_node.as_ref(), options_ptr);
    let content_ptr = content_node.as_ref().map_or_else(|| format!("{options_ptr}/content"), |n| n.ptr.clone());
    let mut lines = match &content_node {
        None => {
            ctx.changed(
                &content_ptr,
                None,
                "the content is absent: Grafana shows its default text, Hune shows an empty panel",
            );
            Vec::new()
        }
        Some(node) => match node.str() {
            Some(text) => content_lines(ctx, node, text),
            None => {
                ctx.unsupported(&node.ptr, Some(node.v), "the content must be a text", Fallback::other("empty text"));
                Vec::new()
            }
        },
    };
    if mode != Mode::Markdown {
        lines = code_block(lines);
    }
    // The limits come first: a text above a limit removes the panel, so the scan of its lines is of no use. A
    // line of up to 32 MiB then costs no scan.
    let over = limit_fault(&lines);
    if mode == Mode::Markdown && over.is_none() {
        scan_report(ctx, &content_ptr, content_node.as_ref().map(|n| n.v), &lines);
    }
    if let Some(code) = &code_node {
        code_out(ctx, code, mode);
    }
    if let Some(reason) = over {
        ctx.unsupported(&content_ptr, content_node.as_ref().map(|n| n.v), &reason, Fallback::PanelRemoved);
    }
    Out::obj([("mode", Out::s("markdown")), ("content", Out::strings(&lines))])
}

/// Takes a member of the options. A `null` member is absent.
fn take_member<'a>(walk: &mut Option<Walk<'a>>, key: &str) -> Option<Node<'a>> {
    walk.as_mut().and_then(|w| w.take(key)).filter(|n| !n.is_null())
}

/// Maps the member `options.mode` (15.6). Hune has `markdown` only.
fn mode_out(ctx: &mut Ctx, node: Option<&Node>, options_ptr: &str) -> Mode {
    let Some(node) = node else {
        let ptr = format!("{options_ptr}/mode");
        ctx.changed(&ptr, None, "the mode is absent: Grafana reads markdown, and Hune has markdown only (4.33a)");
        return Mode::Markdown;
    };
    match node.str() {
        Some("markdown") => {
            ctx.ok();
            Mode::Markdown
        }
        Some("html") => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "Hune has no HTML mode: HTML is text (4.33a, rule 4)",
                Fallback::other("the content becomes a code block"),
            );
            Mode::CodeBlock
        }
        Some("code") => {
            ctx.changed(
                &node.ptr,
                Some(node.v),
                "Hune has no code mode: the content becomes a code block with no colour",
            );
            Mode::CodeBlock
        }
        Some(_) => {
            ctx.changed(&node.ptr, Some(node.v), "Grafana reads this mode as markdown: Hune writes markdown");
            Mode::Markdown
        }
        None => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "the mode must be a text: Hune writes markdown",
                Fallback::other("the mode markdown"),
            );
            Mode::Markdown
        }
    }
}

/// Maps the member `options.content`: the lines and the items of 15.6.
fn content_lines(ctx: &mut Ctx, node: &Node, text: &str) -> Vec<String> {
    let prepared = prepare(text);
    ctx.ok();
    if prepared.had_cr {
        ctx.changed(&node.ptr, Some(node.v), "each CR LF and each lone CR of the text becomes one line end");
    }
    if prepared.removed_control {
        ctx.unsupported(
            &node.ptr,
            Some(node.v),
            "the text has a control character other than a tab and a line end (4.33a, rule 2)",
            Fallback::other("the character is removed"),
        );
    }
    prepared.lines
}

/// Maps the member `options.code`: metadata for markdown, else listed when it has a non-default value.
fn code_out(ctx: &mut Ctx, node: &Node, mode: Mode) {
    if mode == Mode::Markdown || is_default_code(node.v) {
        ctx.meta();
        return;
    }
    ctx.dropped(&node.ptr, Some(node.v), "Hune does not colour the code and shows no line numbers and no mini map");
}

/// Tells if `options.code` has the values `plaintext`, `false` and `false` (an absent member counts).
fn is_default_code(v: &Value) -> bool {
    let Some(map) = v.as_object() else { return is_trivial(v) };
    let flag_false = |key: &str| map.get(key).is_none_or(|x| as_flag(x) == Some(false));
    map.keys().all(|k| matches!(k.as_str(), "language" | "showLineNumbers" | "showMiniMap"))
        && map.get("language").is_none_or(|l| l.as_str() == Some("plaintext"))
        && flag_false("showLineNumbers")
        && flag_false("showMiniMap")
}

/// Gives the reason when a text is above a limit of 4.35. The panel is removed (15.6).
fn limit_fault(lines: &[String]) -> Option<String> {
    let total = lines.iter().map(String::len).sum::<usize>() + lines.len().saturating_sub(1);
    if lines.len() > MAX_TEXT_LINES {
        Some(format!("the text has {} lines: Hune accepts {MAX_TEXT_LINES} lines at most (4.35)", lines.len()))
    } else if lines.iter().any(|l| l.len() > MAX_TEXT_LINE_BYTES) {
        Some(format!("a line of the text has more than {MAX_TEXT_LINE_BYTES} bytes (4.35)"))
    } else if total > MAX_TEXT_BYTES {
        Some(format!("the text has {total} bytes: Hune accepts {MAX_TEXT_BYTES} bytes at most (4.35)"))
    } else {
        None
    }
}

// --------------------------------------------------------------- the members that a text panel does not use

/// A member of a text panel that holds a query or a field option (15.6): metadata when it is empty or
/// the default of Grafana, else a `dropped` item.
pub(crate) fn drop_query_member(ctx: &mut Ctx, node: Option<&Node>) {
    let Some(node) = node else { return };
    if is_trivial(node.v) || is_default_targets(node.v) || is_default_datasource(node.v) {
        ctx.meta();
    } else {
        ctx.dropped(&node.ptr, Some(node.v), "a text panel has no query and no field (4.33a)");
    }
}

/// `[{"refId": "A"}]`: the targets that a new panel of Grafana holds.
fn is_default_targets(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|items| items.iter().all(|i| i.as_object().is_some_and(|m| m.keys().all(|k| k == "refId"))))
}

/// The data source of Grafana for the annotations: it has no query.
fn is_default_datasource(v: &Value) -> bool {
    v.as_object().is_some_and(|m| {
        m.get("type").and_then(Value::as_str) == Some("datasource")
            && m.get("uid").and_then(Value::as_str) == Some("grafana")
            && m.len() == 2
    })
}

// ------------------------------------------------------------------------------------------ the scan

/// Lists the first construct of 15.6 that Hune shows in another way, and a nesting or a table that the
/// page refuses. The first is `changed`. The second removes the panel.
fn scan_report(ctx: &mut Ctx, ptr: &str, value: Option<&Value>, lines: &[String]) {
    let scan = scan(lines);
    if let Some((line, what)) = scan.refused {
        let reason = format!("line {line}: {what}");
        ctx.unsupported(ptr, value, &reason, Fallback::PanelRemoved);
    }
    if let Some((line, what)) = scan.shown_differently {
        let reason = format!("line {line}: {what}");
        ctx.changed(ptr, value, &reason);
    }
}

/// The result of the scan. A line number starts at 1.
#[derive(Debug, Default, PartialEq, Eq)]
struct Scan {
    /// The first construct that Hune shows in another way.
    shown_differently: Option<(usize, &'static str)>,
    /// The first nesting or table above a limit of the page.
    refused: Option<(usize, &'static str)>,
}

fn scan(lines: &[String]) -> Scan {
    let mut out = Scan::default();
    let mut nest = Nest::default();
    for (i, line) in lines.iter().enumerate() {
        let prev = if i > 0 { lines.get(i - 1).map(String::as_str) } else { None };
        if out.shown_differently.is_none() {
            out.shown_differently = construct(line, prev).map(|what| (i + 1, what));
        }
        if out.refused.is_none() {
            if nest.depth_of(line) > MAX_DEPTH {
                out.refused = Some((i + 1, "a list or a quote nested more than 6 levels"));
            } else if lines.get(i + 1).is_some_and(|next| table_columns(line, next) > MAX_COLUMNS) {
                out.refused = Some((i + 1, "a table with more than 20 columns"));
            }
        }
        if out.shown_differently.is_some() && out.refused.is_some() {
            break;
        }
    }
    out
}

/// Gives the first construct of the list of 15.6 that a line has, in the order of the list.
fn construct(line: &str, prev: Option<&str>) -> Option<&'static str> {
    if has_html(line) {
        Some("HTML shows as text")
    } else if line.contains("![") {
        Some("an image: no image shows, the page shows ! and a link")
    } else if has_entity(line) {
        Some("an entity shows as written")
    } else if line.contains("~~") {
        Some("the marks of a strike show")
    } else if is_task_item(line) {
        Some("the marks of a task show")
    } else if is_setext_underline(line, prev) {
        Some("a line of = or - under a line of text shows as text or as a rule")
    } else if is_indented_code(line, prev) {
        Some("an indented code block shows as text")
    } else if is_reference(line) {
        Some("a link reference, a reference link or a footnote shows as written")
    } else if let Some(what) = link_fault(line) {
        Some(what)
    } else {
        placeholder_fault(line)
    }
}

fn has_html(line: &str) -> bool {
    let b = line.as_bytes();
    b.iter()
        .enumerate()
        .any(|(i, c)| *c == b'<' && b.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic() || *n == b'/' || *n == b'!'))
}

/// `&` then letters, digits or `#`, then `;`. The first character is a letter or `#`.
fn has_entity(line: &str) -> bool {
    let b = line.as_bytes();
    (0..b.len()).filter(|i| b[*i] == b'&').any(|i| {
        let rest = &b[i + 1..];
        let run = rest.iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'#').count();
        run > 0 && (rest[0].is_ascii_alphabetic() || rest[0] == b'#') && rest.get(run) == Some(&b';')
    })
}

/// The length of the list marker at the start of a line (after the spaces) and the spaces that follow it.
/// `None`: the line has no list marker.
fn marker(line: &str) -> Option<(usize, usize)> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    let b = rest.as_bytes();
    let width = match b.first()? {
        b'-' | b'*' | b'+' => 1,
        c if c.is_ascii_digit() => {
            let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
            if digits > 9 || !matches!(b.get(digits), Some(b'.' | b')')) {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    let spaces = rest[width..].len() - rest[width..].trim_start_matches(' ').len();
    (spaces > 0 || rest.len() == width).then_some((indent, width + spaces.min(4)))
}

fn is_task_item(line: &str) -> bool {
    let Some((indent, width)) = marker(line) else { return false };
    let rest = line[indent + width..].trim_start_matches(' ');
    ["[ ]", "[x]", "[X]"].iter().any(|t| rest.starts_with(t))
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn is_setext_underline(line: &str, prev: Option<&str>) -> bool {
    let Some(prev) = prev else { return false };
    let spaces = line.len() - line.trim_start_matches(' ').len();
    let body = line[spaces..].trim_end_matches([' ', '\t']);
    let prev_trim = prev.trim_start();
    // A heading, a fence or a table row before the line gives no setext heading.
    let prev_other = prev_trim.starts_with('#')
        || prev_trim.starts_with("```")
        || prev_trim.starts_with("~~~")
        || prev.contains('|');
    spaces <= 3
        && !body.is_empty()
        && (body.bytes().all(|c| c == b'=') || body.bytes().all(|c| c == b'-'))
        && !is_blank(prev)
        && !prev_other
}

fn is_indented_code(line: &str, prev: Option<&str>) -> bool {
    prev.is_some_and(is_blank) && (line.starts_with("    ") || line.starts_with('\t')) && !is_blank(line)
}

/// `[name]:` at the start of a line, `[text][name]` and `[^name]`.
fn is_reference(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    let definition = line.len() - trimmed.len() <= 3
        && trimmed.strip_prefix('[').and_then(|r| r.find("]:")).is_some_and(|close| close > 0);
    let reference = line.find("][").is_some_and(|at| line[..at].contains('[') && line[at + 2..].contains(']'));
    let footnote = line.find("[^").is_some_and(|at| line[at + 2..].contains(']'));
    definition || reference || footnote
}

/// Tells if a destination follows the rule of 9.19b (Links, rules 1 and 2). The scheme is not case sensitive.
fn destination_ok(dest: &str) -> bool {
    if dest.is_empty() || dest.len() > MAX_DESTINATION_BYTES {
        return false;
    }
    if !dest.bytes().all(|c| (0x21..=0x7e).contains(&c) && c != b'(' && c != b')') {
        return false;
    }
    if dest.starts_with("//") || dest.starts_with("/\\") {
        return false;
    }
    let lower = dest.to_ascii_lowercase();
    ["/", "?", "#", "http://", "https://"].iter().any(|p| lower.starts_with(p))
}

/// The next byte of a kind at or after a position, for queries with a position that never goes back. One pass
/// over the text serves all the queries: it keeps the last hit, and a miss ends the search for good.
struct NextByte<'a> {
    bytes: &'a [u8],
    target: u8,
    hit: Option<usize>,
    missed: bool,
}

impl<'a> NextByte<'a> {
    fn new(text: &'a str, target: u8) -> Self {
        Self { bytes: text.as_bytes(), target, hit: None, missed: false }
    }

    /// Gives the offset of the first byte at or after `from`. `from` never decreases between two calls.
    fn at_or_after(&mut self, from: usize) -> Option<usize> {
        if self.missed {
            return None;
        }
        if let Some(hit) = self.hit.filter(|h| *h >= from) {
            return Some(hit);
        }
        match self.bytes.get(from..).and_then(|t| t.iter().position(|b| *b == self.target)) {
            Some(rel) => {
                self.hit = Some(from + rel);
                self.hit
            }
            None => {
                self.missed = true;
                None
            }
        }
    }
}

/// A link whose destination breaks the rule of 9.19b, and a bare `www.` address.
fn link_fault(line: &str) -> Option<&'static str> {
    let first_open = line.find('[');
    let mut closes = NextByte::new(line, b')');
    let mut from = 0;
    while let Some(at) = line[from..].find("](") {
        let open = from + at + 2;
        from = open;
        // A `[` before the `](` makes it a link.
        if !first_open.is_some_and(|b| b < open - 2) {
            continue;
        }
        let Some(close) = closes.at_or_after(open) else { break };
        if !destination_ok(&line[open..close]) {
            return Some("a link destination that Hune does not accept: the link shows as text");
        }
    }
    has_www(line).then_some("a www. address: Hune makes no link for it")
}

/// `www.` at the start of a word and followed by a letter or a digit.
fn has_www(line: &str) -> bool {
    let b = line.as_bytes();
    (0..b.len()).any(|i| {
        b[i..].starts_with(b"www.")
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'.' || b[i - 1] == b'/'))
            && b.get(i + 4).is_some_and(u8::is_ascii_alphanumeric)
    })
}

/// A placeholder that Hune reads in another way: a format other than `text`, `csv`, `pipe` and `glob`,
/// `[[name]]`, a name that starts with `__`, and a placeholder in a destination.
fn placeholder_fault(line: &str) -> Option<&'static str> {
    if let Some(at) = line.find("[[")
        && line[at + 2..].contains("]]")
    {
        return Some("a placeholder [[name]] shows as written");
    }
    let first_open = line.find('[');
    let mut closes = NextByte::new(line, b')');
    let mut from = 0;
    while let Some(at) = line[from..].find("](") {
        let open = from + at + 2;
        from = open;
        if !first_open.is_some_and(|b| b < open - 2) {
            continue;
        }
        let Some(close) = closes.at_or_after(open) else { break };
        if line[open..close].contains('$') {
            return Some("a placeholder in a link destination stays as written");
        }
    }
    let b = line.as_bytes();
    let mut braces = NextByte::new(line, b'}');
    let mut colons = NextByte::new(line, b':');
    for (i, c) in b.iter().enumerate() {
        if *c != b'$' {
            continue;
        }
        let (name, format) = match b.get(i + 1) {
            Some(b'{') => {
                // A `${` with no `}` after it ends the check of the braces, and the next `$` is read as a name.
                let Some(end) = braces.at_or_after(i + 2) else { continue };
                // The first `:` before the `}`: the name ends there and the format follows.
                match colons.at_or_after(i + 2).filter(|c| *c < end) {
                    Some(colon) => (&line[i + 2..colon], Some(&line[colon + 1..end])),
                    None => (&line[i + 2..end], None),
                }
            }
            Some(n) if n.is_ascii_alphanumeric() || *n == b'_' => {
                let len = b[i + 1..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'_').count();
                (&line[i + 1..i + 1 + len], None)
            }
            _ => continue,
        };
        if name.starts_with("__") {
            return Some("a placeholder with a name that starts with __ stays as written");
        }
        if format.is_some_and(|f| !matches!(f, "text" | "csv" | "pipe" | "glob")) {
            return Some("a placeholder with a format that Hune does not read stays as written");
        }
    }
    None
}

/// The nesting of lists and quotes that a text scan can see: the quote marks of a line, and the lists by
/// the width of their markers.
#[derive(Default)]
struct Nest {
    /// The column where the text of each open list item starts.
    items: Vec<usize>,
}

impl Nest {
    /// Gives the depth of a line: its quote marks and the open items. A blank line has depth 0.
    fn depth_of(&mut self, line: &str) -> usize {
        if is_blank(line) {
            return 0;
        }
        let expanded = line.replace('\t', "    ");
        let column = |rest: &str| expanded.len() - rest.len();
        let mut rest = expanded.trim_start_matches(' ');
        // A line with less indent than the text of an item closes the item.
        while self.items.last().is_some_and(|start| *start > column(rest)) {
            self.items.pop();
        }
        let mut quotes = 0;
        loop {
            rest = rest.trim_start_matches(' ');
            if let Some(after) = rest.strip_prefix('>') {
                quotes += 1;
                rest = after;
            } else if let Some((_, width)) = marker(rest) {
                rest = &rest[width.min(rest.len())..];
                self.items.push(column(rest));
            } else {
                break;
            }
        }
        quotes + self.items.len()
    }
}

/// Gives the count of cells of a table header, when `delimiter` is the delimiter row of a table with the same
/// count of cells. Else 0.
fn table_columns(header: &str, delimiter: &str) -> usize {
    if !header.contains('|') || !delimiter.contains('-') {
        return 0;
    }
    let cells = |s: &str| -> Vec<String> {
        let t = s.trim();
        let t = t.strip_prefix('|').unwrap_or(t);
        let t = t.strip_suffix('|').unwrap_or(t);
        t.split('|').map(|c| c.trim().to_owned()).collect()
    };
    let (head, delim) = (cells(header), cells(delimiter));
    let is_delimiter_cell = |c: &String| {
        let core = c.trim_start_matches(':').trim_end_matches(':');
        !core.is_empty() && core.bytes().all(|b| b == b'-')
    };
    if head.len() == delim.len() && delim.iter().all(is_delimiter_cell) { head.len() } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_owned).collect()
    }

    #[test]
    fn the_link_and_placeholder_scans_are_one_pass() {
        // Review of 2026-11-14, finding F2: each `](` searched the rest of the line for `)`.
        let bound = std::time::Duration::from_secs(1);
        let link = format!("[{}", "](".repeat(200_000));
        let t = std::time::Instant::now();
        assert_eq!(link_fault(&link), None);
        assert_eq!(placeholder_fault(&link), None);
        let braces = "${".repeat(200_000);
        assert_eq!(placeholder_fault(&braces), None);
        let colons = format!("{}}}", "${".repeat(100_000));
        assert_eq!(placeholder_fault(&colons), None);
        assert!(t.elapsed() < bound, "{:?}", t.elapsed());
        // The answers do not change.
        assert!(link_fault("[a](ftp://x)").is_some());
        assert!(link_fault("[a](/x) [b](https://y/z)").is_none());
        assert!(link_fault("x](ftp://x) [a](/ok)").is_none());
        assert!(placeholder_fault("[a](/x?v=$v)").is_some());
        assert!(placeholder_fault("${a:json} ${b}").is_some());
        assert!(placeholder_fault("${a:csv} ${b} $c").is_none());
        assert!(placeholder_fault("${ok} ${__x}").is_some());
        assert!(placeholder_fault("${open $__x").is_some());
    }

    #[test]
    fn the_text_is_cut_at_the_line_end() {
        assert_eq!(prepare("").lines, Vec::<String>::new());
        assert_eq!(prepare("a").lines, ["a"]);
        assert_eq!(prepare("a\nb\n").lines, ["a", "b", ""]);
        assert!(!prepare("a\nb").had_cr);
    }

    #[test]
    fn each_cr_lf_and_lone_cr_is_one_line_end() {
        let p = prepare("a\r\nb\rc\nd");
        assert_eq!(p.lines, ["a", "b", "c", "d"]);
        assert!(p.had_cr && !p.removed_control);
    }

    #[test]
    fn a_control_character_is_removed_and_a_tab_stays() {
        let p = prepare("a\u{0}b\u{1}c\u{7f}d\te\u{b}\u{c}");
        assert_eq!(p.lines, ["abcd\te"]);
        assert!(p.removed_control);
        assert!(!prepare("tab\there").removed_control);
    }

    #[test]
    fn the_fence_is_three_backticks_or_one_more_than_the_longest_run() {
        for (run, want) in [(0, 3), (1, 3), (2, 3), (3, 4), (4, 5), (5, 6)] {
            let l = vec![format!("a{}b", "`".repeat(run)), "x".to_owned()];
            assert_eq!(fence_for(&l), "`".repeat(want), "run of {run}");
        }
        // The longest run counts over all the lines, and a run is not cut by a line end.
        assert_eq!(fence_for(&["``".into(), "``".into()]), "```");
        assert_eq!(fence_for(&["```".into(), "`````".into()]), "``````");
    }

    #[test]
    fn a_code_block_has_the_fence_and_no_info_string() {
        let block = code_block(vec!["<b>x</b>".into(), "```".into()]);
        assert_eq!(block, ["````", "<b>x</b>", "```", "````"]);
        assert!(code_block(Vec::new()).is_empty());
    }

    #[test]
    fn each_construct_of_the_list_is_found() {
        let cases: [(&str, &str); 14] = [
            ("a <b>x</b>", "HTML"),
            ("a <!-- c --> b", "HTML"),
            ("![alt](http://a/i.png)", "image"),
            ("fish &amp; chips", "entity"),
            ("a &#169; b", "entity"),
            ("~~old~~", "strike"),
            ("- [ ] todo", "task"),
            ("1. [x] done", "task"),
            ("[name]: http://a", "reference"),
            ("see [text][name]", "reference"),
            ("a footnote[^1]", "reference"),
            ("[x](javascript:alert(1))", "link destination"),
            ("[x](../up)", "link destination"),
            ("[x](http://a \"title\")", "link destination"),
        ];
        for (line, what) in cases {
            let found = construct(line, None).unwrap_or_else(|| panic!("{line}: no construct"));
            assert!(found.contains(what), "{line}: {found}");
        }
    }

    #[test]
    fn the_destinations_that_the_page_accepts_are_not_listed() {
        for line in [
            "[docs](https://wiki.example/a?b=1&c=2)",
            "[x](/d/other-uid)",
            "[x](#top)",
            "[x](?x=1)",
            "[x](HTTP://A.example)",
            "plain text with a $region and ${region:csv} and ${region:text}",
            "see https://a.example/x, or more",
            "# Title",
            "- item",
            "> quote",
            "a | b",
        ] {
            assert_eq!(construct(line, None), None, "{line}");
        }
    }

    #[test]
    fn the_destinations_that_the_page_refuses_are_listed() {
        for dest in [
            "javascript:x",
            "JaVaScRiPt:x",
            " javascript:x",
            "data:text/html,x",
            "mailto:a@b.c",
            "ftp://h/f",
            "//evil.example",
            "/\\evil.example",
            "../up",
            "www.example.com",
            "http://a b",
            "http://a/(b)",
            "http://a/\u{e9}",
            "<http://a>",
        ] {
            let line = format!("[x]({dest})");
            assert!(construct(&line, None).is_some(), "{line}");
        }
        let long = format!("[x](http://a/{})", "a".repeat(2048));
        assert!(construct(&long, None).is_some());
        let ok = format!("[x](http://a/{})", "a".repeat(2048 - 9));
        assert!(construct(&ok, None).is_none());
    }

    #[test]
    fn a_setext_line_and_an_indented_code_line_need_their_context() {
        assert!(construct("====", Some("Title")).is_some());
        assert!(construct("---", Some("Title")).is_some());
        assert!(construct("---", Some("")).is_none(), "a rule after a blank line");
        assert!(construct("---", None).is_none());
        assert!(construct("    code", Some("")).is_some());
        assert!(construct("\tcode", Some("")).is_some());
        assert!(construct("    code", Some("text")).is_none(), "no blank line before");
        assert!(construct("    ", Some("")).is_none());
    }

    #[test]
    fn the_placeholders_of_the_list_are_found() {
        for line in [
            "${region:json}",
            "${region:raw}",
            "$__from and $__to",
            "${__dashboard}",
            "[[region]]",
            "[x](https://a.example/$region)",
            "[x](https://a.example/${region})",
        ] {
            assert!(construct(line, None).is_some(), "{line}");
        }
    }

    #[test]
    fn the_scan_gives_the_first_construct_and_its_line() {
        let text = lines("# Title\n\nplain\n~~strike~~\n<b>later</b>");
        let s = scan(&text);
        assert_eq!(s.shown_differently.map(|(l, _)| l), Some(4));
        assert_eq!(s.refused, None);
        assert_eq!(scan(&lines("# Title\n\n- a\n- b\n\n| a | b |\n|---|---|\n| 1 | 2 |")), Scan::default());
    }

    #[test]
    fn a_nesting_of_7_levels_and_a_table_of_21_columns_are_refused() {
        let quotes = |n: usize| vec![format!("{}x", "> ".repeat(n))];
        assert_eq!(scan(&quotes(6)).refused, None);
        assert_eq!(scan(&quotes(7)).refused.map(|(l, _)| l), Some(1));
        let list = |n: usize| (0..n).map(|i| format!("{}- x", "  ".repeat(i))).collect::<Vec<_>>();
        assert_eq!(scan(&list(6)).refused, None);
        assert_eq!(scan(&list(7)).refused.map(|(l, _)| l), Some(7));
        let table = |n: usize| {
            let row = |c: &str| format!("|{}", format!("{c}|").repeat(n));
            vec![row(" a "), row("---"), row(" 1 ")]
        };
        assert_eq!(scan(&table(20)).refused, None);
        assert_eq!(scan(&table(21)).refused.map(|(l, _)| l), Some(1));
        // A quote in a list item counts both.
        let mixed = vec!["- > - > - > - x".to_owned()];
        assert_eq!(scan(&mixed).refused.map(|(l, _)| l), Some(1));
    }

    #[test]
    fn the_default_code_options_are_metadata() {
        assert!(is_default_code(
            &serde_json::json!({"language": "plaintext", "showLineNumbers": false, "showMiniMap": false})
        ));
        assert!(is_default_code(&serde_json::json!({})));
        assert!(!is_default_code(&serde_json::json!({"language": "go"})));
        assert!(!is_default_code(&serde_json::json!({"showLineNumbers": true})));
        assert!(!is_default_code(&serde_json::json!({"other": 1})));
    }
}
