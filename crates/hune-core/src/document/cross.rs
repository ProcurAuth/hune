// SPDX-License-Identifier: MIT OR Apache-2.0
//! The rules across members (4.34): unique ids and names, overlaps, reuse chains and cycles of
//! variable references. They run after the member checks, on the parts that passed.

use super::model::{PanelType, RepeatDirection};
use super::read::{Ctx, PanelMeta, PlacementMeta, RepeatMeta, VariableMeta, quoted};
use crate::error::ErrorCode;
use crate::limits::{MAX_PANELS, MAX_QUERIES};
use std::collections::HashMap;

/// Runs every rule across members and adds the errors to `cx`.
pub(crate) fn check(cx: &mut Ctx) {
    panel_count(cx);
    query_count(cx);
    unique_variable_names(cx);
    adhoc_sources(cx);
    annotation_rules(cx);
    unique_panel_ids(cx);
    overlaps(cx);
    reuse(cx);
    placements(cx);
    repeats(cx);
    extra_query_repeats(cx);
    variable_cycles(cx);
}

fn panel_count(cx: &mut Ctx) {
    if cx.panel_total > MAX_PANELS {
        let total = cx.panel_total;
        cx.push(
            "/sections",
            ErrorCode::TooLarge,
            format!("the document must have at most {MAX_PANELS} panels; value {total} panels"),
        );
    }
}

/// The cap on the queries of a document: a hidden query counts (4.15a, rule 6).
fn query_count(cx: &mut Ctx) {
    if cx.query_total > MAX_QUERIES {
        let total = cx.query_total;
        cx.push(
            "/sections",
            ErrorCode::TooLarge,
            format!("the document must have at most {MAX_QUERIES} queries; value {total} queries"),
        );
    }
}

/// A panel with `extra_queries` has no `repeat` and is not in a section with `repeat` (4.15a, rule 8).
fn extra_query_repeats(cx: &mut Ctx) {
    let mut errors = Vec::new();
    for p in &cx.panels {
        let Some(pointer) = &p.extra_pointer else { continue };
        if p.repeat.is_some() || section_repeats(&cx.sections, p.section) {
            errors.push((
                pointer.clone(),
                "extra_queries must not be in a panel that has repeat or is in a section that has one (4.15a, rule 8)"
                    .to_owned(),
            ));
        }
    }
    for (pointer, message) in errors {
        cx.push(&pointer, ErrorCode::NotPermitted, message);
    }
}

fn unique_variable_names(cx: &mut Ctx) {
    let mut seen: Vec<&str> = Vec::new();
    let mut errors = Vec::new();
    for v in &cx.variables {
        let Some(name) = v.name.as_deref() else { continue };
        if seen.contains(&name) {
            errors.push((format!("{}/name", v.pointer), format!("name must be unique; value {name:?}")));
        } else {
            seen.push(name);
        }
    }
    for (pointer, message) in errors {
        cx.push(&pointer, ErrorCode::NotUnique, message);
    }
}

/// A document has one `adhoc` variable for each source at most (4.10c, rule 1): a second one gives `not_unique` at
/// its `source`.
fn adhoc_sources(cx: &mut Ctx) {
    let mut seen: Vec<&str> = Vec::new();
    let mut errors = Vec::new();
    for v in &cx.variables {
        let Some(source) = v.adhoc_source.as_deref() else { continue };
        if seen.contains(&source) {
            errors.push((
                format!("{}/source", v.pointer),
                format!("source must be the source of one adhoc variable at most; value {}", quoted(source)),
            ));
        } else {
            seen.push(source);
        }
    }
    for (pointer, message) in errors {
        cx.push(&pointer, ErrorCode::NotUnique, message);
    }
}

/// The rules across members of the annotations (4.34): the names are unique, and each id of `panels` names
/// a panel of the document.
fn annotation_rules(cx: &mut Ctx) {
    let mut errors: Vec<(String, ErrorCode, String)> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for a in &cx.annotations {
        if let Some(name) = a.name.as_deref() {
            if seen.contains(&name) {
                errors.push((
                    format!("{}/name", a.pointer),
                    ErrorCode::NotUnique,
                    format!("name must be unique; value {}", quoted(name)),
                ));
            } else {
                seen.push(name);
            }
        }
        for (pointer, id) in &a.panels {
            if !cx.panels.iter().any(|p| p.id == Some(*id)) {
                errors.push((
                    pointer.clone(),
                    ErrorCode::BadReference,
                    format!("panel id must name a panel of the document; value {id}"),
                ));
            }
        }
    }
    for (pointer, code, message) in errors {
        cx.push(&pointer, code, message);
    }
}

fn unique_panel_ids(cx: &mut Ctx) {
    let mut seen: Vec<u32> = Vec::new();
    let mut errors = Vec::new();
    for p in &cx.panels {
        let Some(id) = p.id else { continue };
        if seen.contains(&id) {
            errors.push((format!("{}/id", p.pointer), format!("id must be unique; value {id}")));
        } else {
            seen.push(id);
        }
    }
    for (pointer, message) in errors {
        cx.push(&pointer, ErrorCode::NotUnique, message);
    }
}

fn overlaps(cx: &mut Ctx) {
    let mut errors = Vec::new();
    for (j, b) in cx.panels.iter().enumerate() {
        let Some(gb) = b.grid else { continue };
        let first =
            cx.panels.iter().take(j).find(|a| a.section == b.section && a.grid.is_some_and(|ga| ga.overlaps(&gb)));
        if let Some(a) = first {
            let with = a.id.map_or_else(|| a.pointer.clone(), |id| format!("panel {id}"));
            errors.push((
                format!("{}/grid", b.pointer),
                format!("grid must not overlap {with}; value x {} y {} w {} h {}", gb.x, gb.y, gb.w, gb.h),
            ));
        }
    }
    for (pointer, message) in errors {
        cx.push(&pointer, ErrorCode::Overlap, message);
    }
}

/// The rules of the placement of a variable (4.7a, 4.34). The section index names a section. The place
/// does not overlap a panel or an earlier placement of the same section.
fn placements(cx: &mut Ctx) {
    let mut errors = Vec::new();
    let placed: Vec<(&str, &PlacementMeta)> =
        cx.variables.iter().filter_map(|v| v.placement.as_ref().map(|p| (v.pointer.as_str(), p))).collect();
    for (j, (pointer, p)) in placed.iter().enumerate() {
        if let Some(count) = cx.section_count
            && usize::try_from(p.section).map_or(true, |s| s >= count)
        {
            errors.push((
                format!("{pointer}/placement/section"),
                ErrorCode::BadReference,
                format!("section must name a section of the document; value {} of {count} sections", p.section),
            ));
            continue;
        }
        if usize::try_from(p.section).is_ok_and(|s| section_repeats(&cx.sections, s)) {
            errors.push((
                format!("{pointer}/placement/section"),
                ErrorCode::NotPermitted,
                format!("section must name a section that has no repeat (4.13a); value {}", p.section),
            ));
            continue;
        }
        let in_section = |s: usize| u32::try_from(s).is_ok_and(|s| s == p.section);
        let with_panel =
            cx.panels.iter().find(|q| in_section(q.section) && q.grid.is_some_and(|g| g.overlaps(&p.grid)));
        let with = match with_panel {
            Some(q) => Some(q.id.map_or_else(|| q.pointer.clone(), |id| format!("panel {id}"))),
            None => placed
                .iter()
                .take(j)
                .find(|(_, o)| o.section == p.section && o.grid.overlaps(&p.grid))
                .map(|(other, _)| format!("the placement of {other}")),
        };
        if let Some(with) = with {
            let g = p.grid;
            errors.push((
                format!("{pointer}/placement"),
                ErrorCode::Overlap,
                format!("placement must not overlap {with}; value x {} y {} w {} h {}", g.x, g.y, g.w, g.h),
            ));
        }
    }
    for (pointer, code, message) in errors {
        cx.push(&pointer, code, message);
    }
}

/// Tells if the section with this index has a valid member `repeat` (4.13a).
fn section_repeats(sections: &[super::read::SectionMeta], index: usize) -> bool {
    sections.iter().any(|m| m.index == index && m.repeat.is_some())
}

fn reuse(cx: &mut Ctx) {
    let by_id: HashMap<u32, &PanelMeta> = cx.panels.iter().filter_map(|p| p.id.map(|id| (id, p))).collect();
    let mut errors = Vec::new();
    for p in &cx.panels {
        let Some(target) = p.reuse_target else { continue };
        let pointer = format!("{}/data/reuse/panel", p.pointer);
        let message = match by_id.get(&target) {
            _ if p.id == Some(target) => format!("panel must name another panel; value {target} is this panel"),
            None => format!("panel must name a panel of the document; value {target}"),
            Some(t) if t.is_text => {
                format!("panel must name a panel with query: a text panel has no query (4.33a); value {target}")
            }
            Some(t) if t.is_query == Some(false) => {
                format!("panel must name a panel with query, not a reuse; value {target}")
            }
            Some(t) if t.repeat.is_some() || section_repeats(&cx.sections, t.section) => {
                format!(
                    "panel must name a panel that has no repeat and is not in a section that has one (4.13a); value {target}"
                )
            }
            // A bar chart, an xy chart, a logs panel and a map draw one frame: no source with several queries (4.15, rule 6).
            Some(t)
                if t.extra_pointer.is_some()
                    && matches!(
                        p.panel_type,
                        Some(PanelType::Barchart | PanelType::Xychart | PanelType::Logs | PanelType::Geomap)
                    ) =>
            {
                format!(
                    "panel must name a panel with one query, because a bar chart, an xy chart, a logs panel and a map draw one frame (4.15, rule 6); value {target}"
                )
            }
            Some(_) => continue,
        };
        errors.push((pointer, message));
    }
    for (pointer, message) in errors {
        cx.push(&pointer, ErrorCode::BadReference, message);
    }
}

/// Gives the error of the variable of a `repeat` (4.13a, rule 1), or `None`. An unknown name is
/// `bad_reference`. A variable that cannot repeat is `not_permitted`. A variable that did not read gives no
/// false error.
fn repeat_variable_error(cx: &Ctx, r: &RepeatMeta) -> Option<(String, ErrorCode, String)> {
    let pointer = format!("{}/variable", r.pointer);
    match cx.variables.iter().find(|v| v.name.as_deref() == Some(r.variable.as_str())) {
        Some(v) if v.repeatable == Some(false) => Some((
            pointer,
            ErrorCode::NotPermitted,
            format!("variable must have the kind query or custom and multi true; value {}", quoted(&r.variable)),
        )),
        Some(_) => None,
        None if cx.variables.iter().any(|v| v.name.is_none()) => None,
        None => Some((
            pointer,
            ErrorCode::BadReference,
            format!("variable must name a variable of the document; value {}", quoted(&r.variable)),
        )),
    }
}

/// The rules of the member `repeat` of a panel and of a section (4.13a, 4.34). The errors follow the order of
/// the document: for each section, its own repeat, then its panels.
fn repeats(cx: &mut Ctx) {
    let mut errors: Vec<(String, ErrorCode, String)> = Vec::new();
    let placements: Vec<(usize, super::model::Grid)> = cx
        .variables
        .iter()
        .filter_map(|v| v.placement.as_ref())
        .filter_map(|p| usize::try_from(p.section).ok().map(|s| (s, p.grid)))
        .collect();
    for sm in &cx.sections {
        if let Some(r) = &sm.repeat {
            errors.extend(repeat_variable_error(cx, r));
            if sm.title_null == Some(true) {
                errors.push((
                    r.pointer.clone(),
                    ErrorCode::NotPermitted,
                    "repeat must be absent when title is null: the copies need a header".to_owned(),
                ));
            }
        }
        let section_repeated = sm.repeat.is_some();
        for p in cx.panels.iter().filter(|p| p.section == sm.index) {
            if let Some(r) = &p.repeat {
                errors.extend(repeat_variable_error(cx, r));
                if p.is_query == Some(false) {
                    errors.push((
                        r.pointer.clone(),
                        ErrorCode::NotPermitted,
                        "repeat must be absent when data has reuse (4.13a)".to_owned(),
                    ));
                } else if section_repeated {
                    errors.push((
                        r.pointer.clone(),
                        ErrorCode::NotPermitted,
                        "repeat must be absent in a section that has a repeat (4.13a)".to_owned(),
                    ));
                }
                if r.direction == Some(RepeatDirection::H)
                    && let Some(g) = p.grid
                {
                    errors.extend(shared_row(cx, &placements, sm.index, p, g, r));
                }
            }
            if section_repeated && p.is_query == Some(false) {
                errors.push((
                    format!("{}/data/reuse", p.pointer),
                    ErrorCode::NotPermitted,
                    "reuse must be absent in a section that has a repeat (4.13a)".to_owned(),
                ));
            }
        }
    }
    for (pointer, code, message) in errors {
        cx.push(&pointer, code, message);
    }
}

/// Gives the error of a panel with `repeat` and direction `h` that shares a row unit with another panel or
/// with a placement of its section (4.13a, rule 5). The columns do not count: the band has the full width.
fn shared_row(
    cx: &Ctx,
    placements: &[(usize, super::model::Grid)],
    section: usize,
    panel: &PanelMeta,
    g: super::model::Grid,
    r: &RepeatMeta,
) -> Option<(String, ErrorCode, String)> {
    let rows = |o: &super::model::Grid| o.y < g.y + g.h && g.y < o.y + o.h;
    let other = cx
        .panels
        .iter()
        .filter(|q| q.section == section && !std::ptr::eq(*q, panel))
        .find(|q| q.grid.is_some_and(|o| rows(&o)))
        .map(|q| q.id.map_or_else(|| q.pointer.clone(), |id| format!("panel {id}")))
        .or_else(|| {
            placements.iter().find(|(s, o)| *s == section && rows(o)).map(|_| "a placement of the section".to_owned())
        })?;
    Some((
        format!("{}/direction", r.pointer),
        ErrorCode::Overlap,
        format!("direction h must not share a row unit with {other}; value y {} h {}", g.y, g.h),
    ))
}

/// Finds the names that a query text references with `$name`, `${name}` or `${name:format}`, and with the query
/// parameter `{name:Type}` of ClickHouse in code (7.6a).
///
/// This is the name scan of 7.19. The document does not know the dialect of the query here, so the
/// scan reads the text with three readers: the lexer rules of `clickhouse`, the lexer rules of
/// `postgres` and the JSON reader of `mongodb` (7.34). It gives the union of the answers, in the
/// order of the text. A name that only one reader finds is an extra name, and an extra name is
/// allowed. The scan never finds one name less than the run of the query. A comment, a heredoc and
/// a dollar quote hold no reference (7.6). A string and a quoted name do. The scan applies no
/// format check: the check of 7.9 and 7.34 is a rule of the run, not of the scan.
pub fn referenced_names(sql: &str) -> Vec<String> {
    let mut found = scan_clickhouse(sql);
    found.extend(scan_postgres(sql));
    found.extend(scan_json(sql));
    found.sort_by_key(|(at, _)| *at);
    found.dedup();
    found.into_iter().map(|(_, name)| name).collect()
}

/// Finds the names that a saved query names, for the member `names` of the answer of 6.14 (7.19).
///
/// This is the dependency scan. It gives the union of [`referenced_names`] and of the loose scan, which reads
/// each placeholder form of 7.6 in the text with no lexer. It reads a comment, a heredoc and a dollar quote
/// too. It finds each name that the scan of the page found before the page left this work to the server,
/// whatever the lexer of the page did with the text. It can find more names than a run needs, and an extra
/// name costs one run of a panel at most. The validation of a document does not use it.
pub fn dependency_names(sql: &str) -> Vec<String> {
    let mut found = scan_clickhouse(sql);
    found.extend(scan_postgres(sql));
    found.extend(scan_json(sql));
    found.extend(scan_loose(sql));
    found.sort_by_key(|(at, _)| *at);
    found.dedup();
    found.into_iter().map(|(_, name)| name).collect()
}

/// The loose name scan (7.19): each `$name`, `${name}` and `${name:format}` of the text. It gives the
/// offset of each `$`.
fn scan_loose(sql: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = sql.get(i..).and_then(|t| t.find('$')) {
        i = placeholder(sql, i + rel, &mut out);
    }
    out
}

/// The name scan with the rules of the lexer of `clickhouse` (7.5). It gives the offset of each `$`.
fn scan_clickhouse(sql: &str) -> Vec<(usize, String)> {
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    let mut tags: Option<TagIndex<'_>> = None;
    while let Some(&c) = b.get(i) {
        if let Some(q) = quote {
            if c == b'\\' {
                // The escape hides the next byte from the lexer, but not a `$` from the placeholder grammar.
                i += if b.get(i + 1) == Some(&b'$') { 1 } else { 2 };
            } else if c == q {
                quote = None;
                i += 1;
            } else if c == b'$' {
                i = placeholder(sql, i, &mut out);
            } else {
                i += 1;
            }
            continue;
        }
        match c {
            b'\'' | b'"' | b'`' => {
                quote = Some(c);
                i += 1;
            }
            // A line comment of `clickhouse` starts with `--`, `//` or `#` and a space or `!`, and ends at LF only (7.5).
            b'-' if b.get(i + 1) == Some(&b'-') => i = skip_line_lf(b, i),
            b'/' if b.get(i + 1) == Some(&b'/') => i = skip_line_lf(b, i),
            b'#' if matches!(b.get(i + 1), Some(b' ' | b'!')) => i = skip_line_lf(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_block(b, i),
            b'$' => {
                i = heredoc_end(b, i, &mut tags).unwrap_or_else(|| {
                    let after = placeholder(sql, i, &mut out);
                    word(sql, after, true, &mut out)
                });
            }
            b'{' => {
                // The start of a query parameter `{name:Type}` of 7.6a names a variable too (7.19).
                if i == 0 || b[i - 1] != b'$' {
                    param_name(sql, i, &mut out);
                }
                i += 1;
            }
            b'0'..=b'9' => i = number_end(b, i),
            b'.' if b.get(i + 1).is_some_and(u8::is_ascii_digit) => i = number_end(b, i),
            c if (c.is_ascii_alphabetic() || c == b'_' || c >= 0x80) && unicode_space_length(b, i) == 0 => {
                i = word(sql, i, true, &mut out);
            }
            _ => i += unicode_space_length(b, i).max(1),
        }
    }
    out
}

/// Reads the name of `{name:` at `at` (a `{` in code), the start of a query parameter of ClickHouse (7.6a).
fn param_name(sql: &str, at: usize, out: &mut Vec<(usize, String)>) {
    let b = sql.as_bytes();
    let start = at + 1;
    if !b.get(start).is_some_and(|&c| c.is_ascii_alphabetic() || c == b'_') {
        return;
    }
    let mut end = start;
    while b.get(end).is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'_') {
        end += 1;
    }
    if b.get(end) == Some(&b':') {
        out.push((at, sql.get(start..end).unwrap_or_default().to_owned()));
    }
}

/// Gives the length of a Unicode space that the lexer of `clickhouse` reads as a blank, or 0 (7.5).
fn unicode_space_length(b: &[u8], i: usize) -> usize {
    let at = |k: usize| b.get(i + k).copied().unwrap_or(0);
    let (c0, c1, c2) = (at(0), at(1), at(2));
    match (c0, c1, c2) {
        (0xC2, 0x85 | 0xA0, _) => 2,
        (0xE1, 0xA0, 0x8E) => 3,
        (0xE2, 0x80, 0x80..=0x8D | 0xA8 | 0xA9 | 0xAF) => 3,
        (0xE2, 0x81, 0x9F | 0xA0) => 3,
        (0xE3, 0x80, 0x80) | (0xEF, 0xBB, 0xBF) => 3,
        _ => 0,
    }
}

/// Reads a word from `i` (the lexer joins letters, digits, `_`, bytes from 0x80 and `$` in one word),
/// and the placeholders that start at a `$` inside it. With `unicode_blank`, a Unicode space ends the
/// word, as in `clickhouse`. Gives the end. A `$` inside a word never starts a heredoc or a dollar quote.
fn word(sql: &str, mut i: usize, unicode_blank: bool, out: &mut Vec<(usize, String)>) -> usize {
    let b = sql.as_bytes();
    while let Some(&c) = b.get(i) {
        if c == b'$' {
            i = placeholder(sql, i, out);
        } else if c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80 {
            if unicode_blank && c >= 0x80 && unicode_space_length(b, i) > 0 {
                break;
            }
            i += 1;
        } else {
            break;
        }
    }
    i
}

/// Gives the end of a number: letters, digits, `_` and `.`, and the sign of an exponent (7.5).
fn number_end(b: &[u8], mut i: usize) -> usize {
    let hex = b.get(i) == Some(&b'0') && matches!(b.get(i + 1), Some(b'x' | b'X'));
    while let Some(&c) = b.get(i) {
        if !(c.is_ascii_alphanumeric() || c == b'_' || c == b'.') {
            break;
        }
        let exponent = if hex { matches!(c, b'p' | b'P') } else { matches!(c, b'e' | b'E') };
        i += 1;
        if exponent && matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
    }
    i
}

/// Reads the placeholder at `at` (a `$`). Gives the index after it.
fn placeholder(sql: &str, at: usize, out: &mut Vec<(usize, String)>) -> usize {
    let b = sql.as_bytes();
    let braced = b.get(at + 1) == Some(&b'{');
    let start = if braced { at + 2 } else { at + 1 };
    let mut end = start;
    while b.get(end).is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'_') {
        end += 1;
    }
    let closed = !braced || matches!(b.get(end), Some(b'}' | b':'));
    if end > start && closed {
        out.push((at, sql.get(start..end).unwrap_or_default().to_owned()));
    }
    end.max(at + 1)
}

/// Skips a line comment of `clickhouse`: it ends before the next LF (7.5). A CR does not end it.
fn skip_line_lf(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(|&c| c != b'\n') {
        i += 1;
    }
    i
}

/// Skips a line comment of `postgres`: it ends before the next LF or CR (7.20).
fn skip_line(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(|&c| c != b'\n' && c != b'\r') {
        i += 1;
    }
    i
}

/// Skips a block comment. It nests (7.5). An open comment ends at the end of the text.
fn skip_block(b: &[u8], mut i: usize) -> usize {
    let mut depth = 0usize;
    while i < b.len() {
        if b.get(i) == Some(&b'/') && b.get(i + 1) == Some(&b'*') {
            depth += 1;
            i += 2;
        } else if b.get(i) == Some(&b'*') && b.get(i + 1) == Some(&b'/') {
            depth = depth.saturating_sub(1);
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    i
}

/// The index of the closed-tag candidates of a text: for each tag of the form `$name$`, the sorted offsets of
/// its occurrences.
///
/// The search for the closing tag of a heredoc or of a dollar quote reads this index. A search in the text for each
/// opener costs time in the square of the size of the text when most openers have no closing tag. The index
/// costs one pass over the text, and each search costs a binary search.
struct TagIndex<'a> {
    offsets: HashMap<&'a [u8], Vec<usize>>,
}

impl<'a> TagIndex<'a> {
    /// Reads the text once. `tag_len` gives the length of the tag that starts at an offset, or 0.
    ///
    /// An occurrence of a tag in the text starts at a `$`, and the maximal name after this `$` is the name of the
    /// tag. So each occurrence is the tag that `tag_len` reads at its offset.
    fn build(b: &'a [u8], tag_len: fn(&[u8], usize) -> usize) -> Self {
        let mut offsets: HashMap<&'a [u8], Vec<usize>> = HashMap::new();
        for (at, _) in b.iter().enumerate().filter(|(_, c)| **c == b'$') {
            let len = tag_len(b, at);
            if let Some(tag) = b.get(at..at + len).filter(|_| len > 0) {
                offsets.entry(tag).or_default().push(at);
            }
        }
        Self { offsets }
    }

    /// Gives the first offset of `tag` that is not less than `from`.
    fn first_from(&self, tag: &[u8], from: usize) -> Option<usize> {
        let list = self.offsets.get(tag)?;
        list.get(list.partition_point(|&p| p < from)).copied()
    }
}

/// Gives the length of the tag `$name$` of `clickhouse` that starts at `at` (a `$`), or 0. The name can start with a
/// digit (7.5).
fn heredoc_tag_len(b: &[u8], at: usize) -> usize {
    let mut e = at + 1;
    while b.get(e).is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'_') {
        e += 1;
    }
    if b.get(e) == Some(&b'$') { e - at + 1 } else { 0 }
}

/// Gives the end of a closed heredoc `$tag$ ... $tag$` that starts at `at`. `tags` holds the index of the
/// text, built at the first call.
fn heredoc_end<'a>(b: &'a [u8], at: usize, tags: &mut Option<TagIndex<'a>>) -> Option<usize> {
    let len = heredoc_tag_len(b, at);
    if len == 0 {
        return None;
    }
    let tag = b.get(at..at + len)?;
    let index = tags.get_or_insert_with(|| TagIndex::build(b, heredoc_tag_len));
    index.first_from(tag, at + len).map(|found| found + len)
}

// ---- the name scan with the rules of the lexer of `postgres` (7.20) ----

/// Tells if a byte starts a word of PostgreSQL: an ASCII letter, `_` or a byte from 0x80.
fn pg_word_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}

/// The name scan with the rules of the lexer of `postgres` (7.20). It gives the offset of each `$`.
///
/// The scan reads a string, an `E` string and a quoted name for placeholders. It skips the other
/// strings, the `U&` names, the dollar quotes and the comments: a placeholder there is a document
/// error or no reference. A `$` inside a word is a placeholder start, as in the engine.
fn scan_postgres(sql: &str) -> Vec<(usize, String)> {
    let b = sql.as_bytes();
    let mut out = Vec::new();
    let mut tags: Option<TagIndex<'_>> = None;
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        match c {
            b'-' if b.get(i + 1) == Some(&b'-') => i = skip_line(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => i = skip_block(b, i),
            b'\'' => i = pg_string(sql, i, false, true, &mut out),
            b'"' => i = pg_name(sql, i, true, &mut out),
            b'$' => {
                let tag = pg_dollar_tag(b, i);
                i = if tag > 0 {
                    pg_dollar_end(b, i, tag, &mut tags).unwrap_or_else(|| placeholder(sql, i, &mut out))
                } else if b.get(i + 1).is_some_and(u8::is_ascii_digit) {
                    // A parameter token: `$` and digits. It names no variable. What follows is a new token.
                    let mut e = i + 1;
                    while b.get(e).is_some_and(u8::is_ascii_digit) {
                        e += 1;
                    }
                    e
                } else {
                    let after = placeholder(sql, i, &mut out);
                    word(sql, after, false, &mut out)
                };
            }
            b'0'..=b'9' => i = number_end(b, i),
            b'.' if b.get(i + 1).is_some_and(u8::is_ascii_digit) => i = number_end(b, i),
            c if pg_word_start(c) => {
                i = match (c, b.get(i + 1), b.get(i + 2)) {
                    (b'E' | b'e', Some(b'\''), _) => pg_string(sql, i + 1, true, true, &mut out),
                    (b'B' | b'b' | b'X' | b'x' | b'N' | b'n', Some(b'\''), _) => {
                        pg_string(sql, i + 1, false, false, &mut out)
                    }
                    (b'U' | b'u', Some(b'&'), Some(b'\'')) => pg_string(sql, i + 2, false, false, &mut out),
                    (b'U' | b'u', Some(b'&'), Some(b'"')) => pg_name(sql, i + 2, false, &mut out),
                    _ => word(sql, i, false, &mut out),
                };
            }
            _ => i += 1,
        }
    }
    out
}

/// Gives the length of the dollar quote tag that starts at `at`, or 0 (7.20).
fn pg_dollar_tag(b: &[u8], at: usize) -> usize {
    let mut e = at + 1;
    if b.get(e).is_some_and(|&c| pg_word_start(c)) {
        while b.get(e).is_some_and(|&c| pg_word_start(c) || c.is_ascii_digit()) {
            e += 1;
        }
    }
    if b.get(e) == Some(&b'$') { e - at + 1 } else { 0 }
}

/// Gives the end of a closed dollar quote that starts at `at`, or `None` when it is not closed. `tags` holds the
/// index of the text, built at the first call.
fn pg_dollar_end<'a>(b: &'a [u8], at: usize, tag: usize, tags: &mut Option<TagIndex<'a>>) -> Option<usize> {
    let open = b.get(at..at + tag)?;
    let index = tags.get_or_insert_with(|| TagIndex::build(b, pg_dollar_tag));
    index.first_from(open, at + tag).map(|found| found + tag)
}

/// Reads a quoted name from its opening quote. The quote is doubled, and a backslash is ordinary.
/// With `read`, the placeholders in it go to `out`. Gives the end.
fn pg_name(sql: &str, open: usize, read: bool, out: &mut Vec<(usize, String)>) -> usize {
    let b = sql.as_bytes();
    let mut i = open + 1;
    while let Some(&c) = b.get(i) {
        if c == b'"' {
            if b.get(i + 1) == Some(&b'"') {
                i += 2;
            } else {
                return i + 1;
            }
        } else if c == b'$' && read {
            i = placeholder(sql, i, out);
        } else {
            i += 1;
        }
    }
    i
}

/// Finds the opening quote of a continuation of a string (7.20): a line end between two quotes, with
/// blanks and line comments around it. Gives the offset of the opening quote.
fn pg_continuation(b: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while let Some(&c) = b.get(i) {
        match c {
            b' ' | b'\t' | 0x0C => i += 1,
            b'-' if b.get(i + 1) == Some(&b'-') => i = skip_line(b, i),
            _ => break,
        }
    }
    if !matches!(b.get(i), Some(b'\n' | b'\r')) {
        return None;
    }
    i += 1;
    while let Some(&c) = b.get(i) {
        match c {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0C | 0x0B => i += 1,
            b'-' if b.get(i + 1) == Some(&b'-') => {
                i = skip_line(b, i);
                if !matches!(b.get(i), Some(b'\n' | b'\r')) {
                    return None;
                }
                i += 1;
            }
            _ => break,
        }
    }
    (b.get(i) == Some(&b'\'')).then_some(i)
}

/// Reads a string from its opening quote. `escapes` is true for an `E` string. With `read`, the
/// placeholders in it go to `out`. A continuation joins the next part. Gives the end.
fn pg_string(sql: &str, open: usize, escapes: bool, read: bool, out: &mut Vec<(usize, String)>) -> usize {
    let b = sql.as_bytes();
    let mut i = open + 1;
    while let Some(&c) = b.get(i) {
        if escapes && c == b'\\' {
            // The escape hides the next byte from the lexer, but not a `$` from the placeholder grammar.
            i += if b.get(i + 1) == Some(&b'$') { 1 } else { 2 };
        } else if c == b'\'' {
            if b.get(i + 1) == Some(&b'\'') {
                i += 2;
            } else if let Some(next_open) = pg_continuation(b, i + 1) {
                i = next_open + 1;
            } else {
                return i + 1;
            }
        } else if c == b'$' && read {
            i = placeholder(sql, i, out);
        } else {
            i += 1;
        }
    }
    i.min(b.len())
}

// ---- the name scan with the rules of the JSON reader of `mongodb` (7.19, 7.34) ----

/// Gives the end of the string-like run that starts at the quote `open`: any bytes except a quote and
/// a backslash, or a backslash and the next character, then an optional closing quote.
fn json_run_end(sql: &str, open: usize) -> usize {
    let b = sql.as_bytes();
    let mut i = open + 1;
    while let Some(&c) = b.get(i) {
        match c {
            b'"' => return i + 1,
            b'\\' => {
                // The backslash takes the next character. A backslash at the end of the text takes nothing.
                let Some(next) = sql.get(i + 1..).and_then(|t| t.chars().next()) else { return i };
                i += 1 + next.len_utf8();
            }
            _ => i += 1,
        }
    }
    i
}

/// Decodes the escapes of a closed JSON string with its two quote marks, as `JSON.parse` does. A lone
/// surrogate becomes U+FFFD. Gives `None` when the text is not a valid JSON string.
fn json_decode(run: &str) -> Option<String> {
    let inner = run.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    let hex4 = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..4 {
            v = v * 16 + chars.next()?.to_digit(16)?;
        }
        Some(v)
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => return None,
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let first = hex4(&mut chars)?;
                    let mut unit = first;
                    if (0xD800..0xDC00).contains(&first) {
                        // A high surrogate joins a low surrogate that follows as `\uXXXX`.
                        let mut ahead = chars.clone();
                        if ahead.next() == Some('\\')
                            && ahead.next() == Some('u')
                            && let Some(low) = hex4(&mut ahead).filter(|l| (0xDC00..0xE000).contains(l))
                        {
                            unit = 0x10000 + ((first - 0xD800) << 10) + (low - 0xDC00);
                            chars = ahead;
                        }
                    }
                    out.push(char::from_u32(unit).unwrap_or('\u{FFFD}'));
                }
                _ => return None,
            },
            c if c < ' ' => return None,
            c => out.push(c),
        }
    }
    Some(out)
}

/// The name scan with the rules of the JSON reader (7.19, 7.34). It reads each string of the text, keys
/// included, with its escapes decoded. A string that is not valid JSON stays as written. A string with no
/// end reads to the end of the text. The offset of a name is the start of its string plus its place in
/// the decoded string, so the order is stable.
fn scan_json(sql: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = sql.get(i..).and_then(|t| t.find('"')) {
        let open = i + rel;
        let end = json_run_end(sql, open);
        let run = sql.get(open..end).unwrap_or_default();
        let (text, base) = match json_decode(run) {
            Some(decoded) => (decoded, open + 1),
            None => (run.to_owned(), open),
        };
        let mut found = Vec::new();
        let mut k = 0;
        while let Some(rel) = text.get(k..).and_then(|t| t.find('$')) {
            k = placeholder(&text, k + rel, &mut found);
        }
        out.extend(found.into_iter().map(|(at, name)| (base + at, name)));
        i = end.max(open + 1);
    }
    out
}

fn variable_cycles(cx: &mut Ctx) {
    let vars: Vec<&VariableMeta> = cx.variables.iter().filter(|v| v.sql.is_some()).collect();
    let index: HashMap<&str, usize> =
        vars.iter().enumerate().filter_map(|(i, v)| v.name.as_deref().map(|n| (n, i))).collect();
    let edges: Vec<Vec<usize>> = vars
        .iter()
        .map(|v| {
            let names = referenced_names(v.sql.as_deref().unwrap_or_default());
            let mut e: Vec<usize> = names.iter().filter_map(|n| index.get(n.as_str()).copied()).collect();
            e.sort_unstable();
            e.dedup();
            e
        })
        .collect();
    // reach[i][j]: j is reachable from i by one edge or more. At most 64 variables.
    let n = vars.len();
    let mut reach = vec![vec![false; n]; n];
    for (i, row) in reach.iter_mut().enumerate() {
        let mut stack: Vec<usize> = edges.get(i).cloned().unwrap_or_default();
        while let Some(j) = stack.pop() {
            if !std::mem::replace(&mut row[j], true) {
                stack.extend(edges.get(j).into_iter().flatten().copied());
            }
        }
    }
    let mut reported = vec![false; n];
    let mut errors = Vec::new();
    for i in 0..n {
        if reported[i] || !reach[i][i] {
            continue;
        }
        let members: Vec<usize> = (0..n).filter(|&j| reach[i][j] && reach[j][i]).collect();
        for &j in &members {
            reported[j] = true;
        }
        let names: Vec<&str> = members.iter().filter_map(|&j| vars.get(j).and_then(|v| v.name.as_deref())).collect();
        let message = if members.len() == 1 {
            format!("sql must not reference its own variable; value ${{{}}}", names.join(""))
        } else {
            format!("sql must not be in a cycle of variable references; variables {}", names.join(", "))
        };
        if let Some(v) = vars.get(i) {
            errors.push((format!("{}/sql", v.pointer), message));
        }
    }
    for (pointer, message) in errors {
        cx.push(&pointer, ErrorCode::BadReference, message);
    }
}

#[cfg(test)]
mod tests {
    use super::{referenced_names, scan_clickhouse, scan_postgres};
    use crate::sql::Dialect;
    use crate::sql::lexer::{TokenKind, lex_all_for};
    use crate::sql::placeholders::{self, Found};

    #[test]
    fn placeholders() {
        let names = referenced_names("SELECT $a, ${b}, ${c:csv}, '$d', \"$e\" FROM t WHERE x = $__timeFilter(ts)");
        assert_eq!(names, ["a", "b", "c", "d", "e", "__timeFilter"]);
    }

    #[test]
    fn comments_and_heredocs_hold_no_reference() {
        let sql = "SELECT 1 -- $a\n/* $b /* $c */ $d */ , $$ $e $$, $tag$ $f $tag$, # $g\n, $h";
        // The `#` line is a comment for `clickhouse` and code for `postgres`: the union names `g` too.
        assert_eq!(referenced_names(sql), ["g", "h"]);
        assert_eq!(scan_clickhouse(sql).len(), 1);
        let sql = "SELECT 1 -- $a\n/* $b /* $c */ $d */ , $$ $e $$, $tag$ $f $tag$, $h";
        assert_eq!(referenced_names(sql), ["h"]);
    }

    #[test]
    fn a_backslash_does_not_hide_a_placeholder() {
        // The placeholder grammar reads bytes. A backslash before the `$` hides nothing.
        assert_eq!(referenced_names(r"select '\$a', E'\${b}', `\$c`"), ["a", "b", "c"]);
    }

    #[test]
    fn longest_run_names_the_variable() {
        assert_eq!(referenced_names("$env_name ${env}_name"), ["env_name", "env"]);
    }

    #[test]
    fn postgres_rules_of_the_name_scan() {
        // A backslash is ordinary in a string: the string ends at `\'`, and `$a` is code.
        assert_eq!(referenced_names(r"select '\' || $a || '\'"), ["a"]);
        // An E string: the backslash escapes the quote, so the text after it is still in the string. Both
        // lexers name `b`, because the placeholders in a string count.
        assert_eq!(referenced_names(r"select E'\' $b'"), ["b"]);
        // A `#` is not a comment for `postgres`.
        assert_eq!(referenced_names("select 1 # $a\n"), ["a"]);
        // A prefixed string, a `U&` name and a dollar quote hold no reference.
        assert!(scan_postgres("select B'$a', X'$b', N'$c', U&'$d', U&\"$e\", $$ $f $$, $t$ $g $t$").is_empty());
        // A string, an E string and a quoted name hold references, with the doubled quote.
        assert_eq!(
            scan_postgres("select 'it''s $a', E'it\\'s $b', \"q\"\"$c\"")
                .iter()
                .map(|x| x.1.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        // A `$` in a word starts a placeholder, as in the engine.
        assert_eq!(referenced_names("select a$b, a$$c"), ["b", "c"]);
        // A parameter names the variable `1` for `clickhouse`, which no document declares.
        assert_eq!(referenced_names("select $1"), ["1"]);
        assert!(scan_postgres("select $1, $22").is_empty());
        let names: Vec<String> = scan_postgres("select $1x$a$U$a$").into_iter().map(|x| x.1).collect();
        assert_eq!(names, ["a", "U", "a"]);
        // The continuation of a string: the second part is an E string too.
        assert_eq!(
            scan_postgres("select E'a'\n'\\' $x ' $y").iter().map(|x| x.1.as_str()).collect::<Vec<_>>(),
            ["x", "y"]
        );
        // A prefix is read only at the start of a token: `xE'..'` is a word and a standard string.
        assert_eq!(scan_postgres(r"select xE'\' $z").iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), ["z"]);
        // A number that holds an `E` does not start a prefix.
        assert_eq!(scan_postgres(r"select 1E'\' $z'").iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), ["z"]);
    }

    // ---- the scan agrees with the lexers ----

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    const SOUP: &[&str] = &[
        "'", "''", "\"", "`", "\\", "$", "$$", "$a$", "$1", "$a", "$b", "${c}", "${d:csv}", "-", "--", "#", "# ", "/",
        "*", "/*", "*/", "+", "=", ";", ",", ".", "(", ")", " ", "\n", "\r", "\t", "\u{a0}", "\u{e9}", "a", "E", "e",
        "B", "X", "N", "U", "U&", "1", "0x", "1e", "E'", "b'", "U&'", "U&\"", "x", "_",
        // The forms of the measure of ClickHouse 26.8.34.13 (7.5): `//`, `#!`, a digit-led tag, a FF in a comment.
        "//", "#!", "$1$", "\x0c",
    ];

    /// The content of a string, an `E` string or a quoted name that has no `U&` prefix (7.20).
    fn literal_range(sql: &str, t: &crate::sql::lexer::Token) -> Option<(usize, usize)> {
        let readable = match t.kind {
            TokenKind::String | TokenKind::EString => true,
            TokenKind::QuotedName => !sql[t.start..t.end].starts_with(['U', 'u']),
            _ => false,
        };
        if readable { t.content_range(sql) } else { None }
    }

    /// The names that the placeholder grammar finds when the lexer of the dialect gives the ranges (7.6, 7.20):
    /// the code tokens, the strings, the `E` strings and the quoted names that have no `U&` prefix.
    fn names_by_lexer(dialect: Dialect, sql: &str) -> Option<Vec<String>> {
        let tokens = lex_all_for(dialect, sql);
        if tokens.iter().any(|t| !t.closed || t.kind == TokenKind::Unknown) {
            return None;
        }
        let mut out = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let t = tokens[i];
            let (from, to) = if t.kind.is_code() {
                let mut j = i;
                while j + 1 < tokens.len() && tokens[j + 1].kind.is_code() {
                    j += 1;
                }
                i = j;
                (t.start, tokens[j].end)
            } else if let Some(range) = literal_range(sql, &t) {
                range
            } else {
                i += 1;
                continue;
            };
            i += 1;
            let mut p = from;
            while let Some(found) = placeholders::next_for(dialect, sql, p, to) {
                if let Found::Placeholder(ph) = &found {
                    out.push(ph.name.to_owned());
                }
                p = found.end();
            }
        }
        Some(out)
    }

    /// The search of the closing tag as the scan did it before the index: the first occurrence of the tag.
    fn naive_end(b: &[u8], from: usize, tag: &[u8]) -> Option<usize> {
        let rest = b.get(from..)?;
        rest.windows(tag.len()).position(|w| w == tag).map(|pos| from + pos + tag.len())
    }

    #[test]
    fn the_tag_index_agrees_with_the_search_of_the_text() {
        const TAGS: &[&str] =
            &["$", "$$", "$a$", "$ab$", "$a1$", "$_$", "$1$", "$\u{e9}$", "$a", "a", "b", " ", "\n", "'"];
        let mut rng = Rng(0x7A65);
        let mut checked = 0usize;
        for _ in 0..20_000 {
            let n = 1 + rng.below(24);
            let sql: String = (0..n).map(|_| TAGS[rng.below(TAGS.len())]).collect();
            let b = sql.as_bytes();
            let (mut ch, mut pg) = (None, None);
            for at in (0..b.len()).filter(|&i| b[i] == b'$') {
                let len = super::heredoc_tag_len(b, at);
                // A tag of `clickhouse` can start with a digit (7.5).
                let want = if len > 0 { naive_end(b, at + len, &b[at..at + len]) } else { None };
                assert_eq!(super::heredoc_end(b, at, &mut ch), want, "clickhouse at {at} in {sql:?}");
                let len = super::pg_dollar_tag(b, at);
                let want = if len > 0 { naive_end(b, at + len, &b[at..at + len]) } else { None };
                if len > 0 {
                    assert_eq!(super::pg_dollar_end(b, at, len, &mut pg), want, "postgres at {at} in {sql:?}");
                }
                checked += 1;
            }
        }
        assert!(checked > 20_000, "{checked}");
    }

    #[test]
    fn the_scan_never_names_less_than_the_lexers() {
        let cases: usize = 100_000;
        let mut checked = [0usize; 2];
        let mut rng = Rng(0xA11CE);
        for _ in 0..cases {
            let n = 1 + rng.below(20);
            let sql: String = (0..n).map(|_| SOUP[rng.below(SOUP.len())]).collect();
            let scan: Vec<String> = referenced_names(&sql);
            for (k, dialect) in [Dialect::ClickHouse, Dialect::Postgres].into_iter().enumerate() {
                let Some(names) = names_by_lexer(dialect, &sql) else { continue };
                checked[k] += 1;
                for name in &names {
                    assert!(scan.contains(name), "{dialect:?}: the scan misses {name:?} in {sql:?}: {scan:?}");
                }
            }
        }
        assert!(checked[0] > cases / 20 && checked[1] > cases / 20, "{checked:?}");
    }
}
