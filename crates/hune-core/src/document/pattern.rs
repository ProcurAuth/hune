// SPDX-License-Identifier: MIT OR Apache-2.0
//! The common subset of regular expressions (4.24).
//!
//! The server only checks a pattern. The browser runs it with `new RegExp(body, flags)`.

use crate::limits::MAX_PATTERN_BYTES;

/// The two forms of a pattern (4.24).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternForm {
    /// The text is the pattern. A match can start at any position.
    Plain,
    /// The form of the Grafana function `stringToJsRegex`: `/<body>/<flags>`, or a plain text
    /// that is an exact match.
    Grafana,
}

/// The kind of a pattern fault. The validator maps it to an error code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternFault {
    /// The pattern is shorter than 1 byte or longer than `MAX_PATTERN_BYTES`.
    Length,
    /// The pattern is not in the common subset.
    Syntax(String),
}

/// Checks a pattern against the rules of 4.24.
///
/// In the Grafana form, a text that starts with `/` has a body up to its last `/`. The flags
/// after it are 0 to 4 letters from `g`, `i`, `m` and `s`, with no repeat. A text that does not
/// start with `/` is the body.
pub fn check_pattern(text: &str, form: PatternForm) -> Result<(), PatternFault> {
    if text.is_empty() || text.len() > MAX_PATTERN_BYTES {
        return Err(PatternFault::Length);
    }
    let body = match form {
        PatternForm::Plain => text,
        PatternForm::Grafana => match text.strip_prefix('/') {
            None => text,
            Some(rest) => {
                let Some(last) = rest.rfind('/') else {
                    return Err(syntax("a pattern that starts with / needs a second / before the flags"));
                };
                let (body, flags) = (&rest[..last], &rest[last + 1..]);
                check_flags(flags)?;
                body
            }
        },
    };
    check_body(body)
}

fn syntax(message: &str) -> PatternFault {
    PatternFault::Syntax(message.to_owned())
}

fn check_flags(flags: &str) -> Result<(), PatternFault> {
    if flags.len() > 4 {
        return Err(syntax("the flags must have at most 4 letters"));
    }
    let mut seen = [false; 4];
    for c in flags.chars() {
        let i = match c {
            'g' => 0,
            'i' => 1,
            'm' => 2,
            's' => 3,
            _ => {
                return Err(PatternFault::Syntax(format!(
                    "the flag {c} is not permitted; permitted flags: g, i, m, s"
                )));
            }
        };
        if std::mem::replace(&mut seen[i], true) {
            return Err(PatternFault::Syntax(format!("the flag {c} occurs two times")));
        }
    }
    Ok(())
}

/// Checks the body: the lexical rules of the subset, then the syntax of the Rust `regex` crate.
fn check_body(body: &str) -> Result<(), PatternFault> {
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    let mut in_class = false;
    while let Some(&c) = chars.get(i) {
        match c {
            '\\' => {
                i += escape(&chars, i)?;
                continue;
            }
            '[' if in_class => {
                return Err(syntax("a class must not hold [ without a backslash (no POSIX class, no nested class)"));
            }
            '[' => {
                in_class = true;
                i += 1;
                if chars.get(i) == Some(&'^') {
                    i += 1;
                }
                if chars.get(i) == Some(&']') {
                    return Err(syntax("a class must not start with ]; write \\] for the character"));
                }
                continue;
            }
            ']' if in_class => in_class = false,
            '&' | '-' | '~' if in_class && chars.get(i + 1) == Some(&c) => {
                return Err(PatternFault::Syntax(format!("a class must not hold {c}{c}")));
            }
            '(' if !in_class => {
                i += group(&chars, i)?;
                continue;
            }
            '{' if !in_class => {
                i += repetition(&chars, i)?;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    if in_class {
        return Err(syntax("a class has no closing ]"));
    }
    regex_syntax::Parser::new().parse(body).map(|_| ()).map_err(|e| PatternFault::Syntax(first_line(&e.to_string())))
}

fn first_line(s: &str) -> String {
    s.lines().rfind(|l| l.contains("error")).or_else(|| s.lines().next()).unwrap_or("").trim().to_owned()
}

/// Checks the escape at `chars[at]` (a backslash). Gives its length in characters.
fn escape(chars: &[char], at: usize) -> Result<usize, PatternFault> {
    match chars.get(at + 1) {
        None => Err(syntax("the pattern ends with a backslash")),
        Some('d' | 'D' | 'w' | 'W' | 'b' | 'B' | 't' | 'n' | 'r' | 'f' | 'v') => Ok(2),
        Some('x') => {
            let ok = chars.get(at + 2..at + 4).is_some_and(|h| h.iter().all(char::is_ascii_hexdigit));
            if ok { Ok(4) } else { Err(syntax("the escape \\x needs two hex digits")) }
        }
        // The engines differ on \< and \>: Rust reads word boundaries, JavaScript reads text.
        Some('<' | '>') => Err(syntax("the escape \\< and \\> are not permitted")),
        Some(c) if c.is_ascii_punctuation() => Ok(2),
        Some(c) => Err(PatternFault::Syntax(format!("the escape \\{c} is not permitted"))),
    }
}

/// Checks the group that starts at `chars[at]`. Gives the number of characters that it reads.
fn group(chars: &[char], at: usize) -> Result<usize, PatternFault> {
    if chars.get(at + 1) != Some(&'?') {
        return Ok(1);
    }
    match chars.get(at + 2) {
        Some(':') => Ok(3),
        Some('<') => {
            let rest = chars.get(at + 3..).unwrap_or_default();
            let end = rest.iter().position(|&c| c == '>');
            let Some(end) = end else {
                return Err(syntax("a group name has no closing >"));
            };
            let name = &rest[..end];
            let valid = !name.is_empty()
                && !name[0].is_ascii_digit()
                && name.iter().all(|c| c.is_ascii_alphanumeric() || *c == '_');
            if valid {
                Ok(3 + end + 1)
            } else {
                Err(syntax("a group name has ASCII letters, digits and _, and does not start with a digit"))
            }
        }
        _ => Err(syntax("a group (? must be (?: or (?<name>")),
    }
}

/// Checks the repetition that starts at `chars[at]` (an open brace). Gives its length.
fn repetition(chars: &[char], at: usize) -> Result<usize, PatternFault> {
    let bad = || syntax("a repetition in braces must be {m}, {m,} or {m,n}");
    let mut i = at + 1;
    let digits = |i: &mut usize| {
        let start = *i;
        while chars.get(*i).is_some_and(char::is_ascii_digit) {
            *i += 1;
        }
        *i - start
    };
    if digits(&mut i) == 0 {
        return Err(bad());
    }
    if chars.get(i) == Some(&',') {
        i += 1;
        digits(&mut i);
    }
    if chars.get(i) != Some(&'}') {
        return Err(bad());
    }
    Ok(i + 1 - at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(t: &str, f: PatternForm) {
        assert_eq!(check_pattern(t, f), Ok(()), "{t}");
    }

    fn bad(t: &str, f: PatternForm) {
        assert!(matches!(check_pattern(t, f), Err(PatternFault::Syntax(_))), "{t}");
    }

    #[test]
    fn accepted() {
        use PatternForm::*;
        for t in [
            "/^Total$/",
            "/^ratio/",
            "/.*/",
            "/a|b/gi",
            "/a|b/gims",
            "Total",
            "^a(?:b|c)+$",
            "(?<year>\\d{4})-(?<m_1>\\d\\d)",
            "[a-z0-9_.-]+",
            "[\\]\\[]",
            "\\x41\\.\\$\\/",
            "[ \\t\\n\\r\\f\\v]",
            "a{2}b{2,}c{2,5}",
            "[^abc]",
            "é+",
            "a}b]c",
            "//",
        ] {
            ok(t, Grafana);
        }
        ok("a(b)c", Plain);
        ok("/not a flag form/", Plain);
    }

    #[test]
    fn refused_escapes() {
        for t in
            ["\\A", "a\\z", "\\pL", "\\P{L}", "\\s", "\\S", "\\u0041", "\\1", "\\0", "\\Q", "\\<", "\\>", "a\\", "\\x4"]
        {
            bad(t, PatternForm::Plain);
        }
    }

    #[test]
    fn refused_classes() {
        for t in ["[[:alpha:]]", "[a&&b]", "[a--b]", "[a~~b]", "[a[b]]", "[]a]", "[^]a]", "[a", "[\\b]"] {
            bad(t, PatternForm::Plain);
        }
    }

    #[test]
    fn refused_groups() {
        for t in [
            "(?i)a",
            "(?=a)",
            "(?!a)",
            "(?<=a)",
            "(?<!a)",
            "(?P<n>a)",
            "(?<1a>a)",
            "(?<>a)",
            "(?<a-b>a)",
            "(?<a",
            "(a",
            "a)",
        ] {
            bad(t, PatternForm::Plain);
        }
    }

    #[test]
    fn refused_braces() {
        for t in ["a{", "a{,3}", "a{x}", "a{2", "a{2,x}", "{2}", "a{3,2}"] {
            bad(t, PatternForm::Plain);
        }
    }

    #[test]
    fn refused_grafana_form() {
        use PatternForm::Grafana;
        for t in ["/abc", "/", "/a/x", "/a/gg", "/a/gimsg", "/a/G", "/(/"] {
            bad(t, Grafana);
        }
    }

    #[test]
    fn length_rule() {
        assert_eq!(check_pattern("", PatternForm::Plain), Err(PatternFault::Length));
        assert_eq!(check_pattern(&"a".repeat(1025), PatternForm::Plain), Err(PatternFault::Length));
        assert_eq!(check_pattern(&"a".repeat(1024), PatternForm::Plain), Ok(()));
    }

    #[test]
    fn plain_text_is_the_body() {
        // A plain text that starts with a letter is the body of the exact match.
        bad("(", PatternForm::Grafana);
        ok("a/b", PatternForm::Grafana);
    }
}
