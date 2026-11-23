//! Placeholders (7.6) and formats (7.9).
//!
//! The functions of this file read bytes. The caller gives the byte range of a run of code tokens,
//! or the content of a string, a quoted name or a heredoc. A placeholder never spans a comment or
//! a string, because the caller never gives such a range.
//!
//! The grammar of 7.6 is the same in both dialects. In the dialect `postgres`, a `$` and digits is a
//! parameter token (7.20), not a variable ([`next_for`]).

use super::dialect::Dialect;

/// A variable format (7.9). The absence of a format is `None` in [`Placeholder::format`].
///
/// The first four formats are valid in code and in a string. The others follow the Grafana formats
/// of the same name. They are valid only inside a string literal ([`Format::literal_only`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// `raw`: the value as it is, for author text and numbers only.
    Raw,
    /// `csv`: the values joined by `,`.
    Csv,
    /// `singlequote`: each value in `'...'`.
    SingleQuote,
    /// `sqlstring`: each value in `'...'` with `'` doubled.
    SqlString,
    /// `json`: a list as a JSON array. One value stays as it is.
    Json,
    /// `pipe`: the values joined by `|`.
    Pipe,
    /// `regex`: each value escaped for a regular expression. A list gives `(a|b)`.
    Regex,
    /// `glob`: a list of two values or more gives `{a,b}`.
    Glob,
    /// `lucene`: each value escaped for Lucene. A list gives `("a" OR "b")`.
    Lucene,
    /// `percentencode`: the percent-encoded text. A list gives the encoded `{a,b}`.
    PercentEncode,
    /// `queryparam`: `var-name=value`, joined by `&`.
    QueryParam,
    /// `distributed`: `a,name=b,name=c`.
    Distributed,
    /// `doublequote`: each value in `"..."`.
    DoubleQuote,
    /// `html`: the HTML-escaped text. A list is joined by `, ` first.
    Html,
    /// `text`: the text of the selection. A list is joined by ` + `. "All" gives `All`.
    Text,
}

/// The names of the formats that are valid only inside a string literal, in the order of 7.9.
pub const LITERAL_ONLY_FORMATS: [Format; 11] = [
    Format::Json,
    Format::Pipe,
    Format::Regex,
    Format::Glob,
    Format::Lucene,
    Format::PercentEncode,
    Format::QueryParam,
    Format::Distributed,
    Format::DoubleQuote,
    Format::Html,
    Format::Text,
];

impl Format {
    /// Reads the name of a format of 7.9.
    pub fn parse(name: &str) -> Option<Format> {
        match name {
            "raw" => Some(Format::Raw),
            "csv" => Some(Format::Csv),
            "singlequote" => Some(Format::SingleQuote),
            "sqlstring" => Some(Format::SqlString),
            "json" => Some(Format::Json),
            "pipe" => Some(Format::Pipe),
            "regex" => Some(Format::Regex),
            "glob" => Some(Format::Glob),
            "lucene" => Some(Format::Lucene),
            "percentencode" => Some(Format::PercentEncode),
            "queryparam" => Some(Format::QueryParam),
            "distributed" => Some(Format::Distributed),
            "doublequote" => Some(Format::DoubleQuote),
            "html" => Some(Format::Html),
            "text" => Some(Format::Text),
            _ => None,
        }
    }

    /// Gives the name of the format.
    pub fn name(self) -> &'static str {
        match self {
            Format::Raw => "raw",
            Format::Csv => "csv",
            Format::SingleQuote => "singlequote",
            Format::SqlString => "sqlstring",
            Format::Json => "json",
            Format::Pipe => "pipe",
            Format::Regex => "regex",
            Format::Glob => "glob",
            Format::Lucene => "lucene",
            Format::PercentEncode => "percentencode",
            Format::QueryParam => "queryparam",
            Format::Distributed => "distributed",
            Format::DoubleQuote => "doublequote",
            Format::Html => "html",
            Format::Text => "text",
        }
    }

    /// Tells if the format is valid only inside a string literal (D7). The text of such a format is
    /// free text: only the string escape keeps it inside the literal. Code and a quoted name refuse it.
    pub fn literal_only(self) -> bool {
        !matches!(self, Format::Raw | Format::Csv | Format::SingleQuote | Format::SqlString)
    }
}

/// A placeholder that matches the grammar of 7.6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placeholder<'a> {
    /// The name: a run of `[A-Za-z0-9_]`.
    pub name: &'a str,
    /// The format, or `None` when the placeholder has no format.
    pub format: Option<Format>,
    /// The offset of the `$`.
    pub start: usize,
    /// The offset after the last byte of the placeholder.
    pub end: usize,
    /// True for the forms `${name}` and `${name:format}`.
    pub braced: bool,
}

/// The cause of a `${` that does not match the grammar of 7.6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BraceFault {
    /// The text after `${` is not a name followed by `}` or `:format}`.
    Malformed,
    /// The format is not in the list of 7.9.
    UnknownFormat(String),
}

/// What [`next`] finds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found<'a> {
    /// A placeholder.
    Placeholder(Placeholder<'a>),
    /// A `${` that does not match the grammar. `start` is the offset of the `$`, and `end` is the
    /// offset after the `${`.
    BadBrace {
        /// The offset of the `$`.
        start: usize,
        /// The offset after `${`.
        end: usize,
        /// The cause.
        fault: BraceFault,
    },
}

impl Found<'_> {
    /// The offset of the first byte.
    pub fn start(&self) -> usize {
        match self {
            Found::Placeholder(p) => p.start,
            Found::BadBrace { start, .. } => *start,
        }
    }

    /// The offset after the last byte that the caller must skip.
    pub fn end(&self) -> usize {
        match self {
            Found::Placeholder(p) => p.end,
            Found::BadBrace { end, .. } => *end,
        }
    }
}

/// Tells if a byte can be in a variable name.
pub fn is_name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Finds the next placeholder or malformed `${` in `text[from..to]`.
///
/// `$name` takes the longest run of name bytes (7.6). A lone `$` is not a placeholder.
pub fn next(text: &str, from: usize, to: usize) -> Option<Found<'_>> {
    scan(text, from, to, false)
}

/// Finds the next placeholder or malformed `${` in `text[from..to]`, for the dialect.
///
/// For `clickhouse` it is [`next`]. For `postgres` the grammar is the same, except for one point: a
/// `$` and a digit start a parameter token (7.20), so the `$` and the digits that follow are not a
/// placeholder. A variable name never starts with a digit (4.7), so no variable is lost. The
/// statement check refuses the parameter (7.25). A `$` in the middle of a word, as in `a$b`, still
/// starts a placeholder, as in `clickhouse`: this reads one name more than the lexer and never one less.
pub fn next_for(dialect: Dialect, text: &str, from: usize, to: usize) -> Option<Found<'_>> {
    scan(text, from, to, dialect == Dialect::Postgres)
}

/// The scan of [`next`] and [`next_for`]. With `parameters`, a `$` and a digit start a parameter token.
fn scan(text: &str, from: usize, to: usize, parameters: bool) -> Option<Found<'_>> {
    let b = text.as_bytes();
    let to = to.min(b.len());
    let mut p = from;
    while p < to {
        if b[p] != b'$' {
            p += 1;
            continue;
        }
        if p + 1 < to && b[p + 1] == b'{' {
            return Some(braced(text, p, to));
        }
        if parameters && p + 1 < to && b[p + 1].is_ascii_digit() {
            p += 1;
            while p < to && b[p].is_ascii_digit() {
                p += 1;
            }
            continue;
        }
        let mut n = p + 1;
        while n < to && is_name_byte(b[n]) {
            n += 1;
        }
        if n > p + 1 {
            return Some(Found::Placeholder(Placeholder {
                name: text.get(p + 1..n)?,
                format: None,
                start: p,
                end: n,
                braced: false,
            }));
        }
        p += 1;
    }
    None
}

fn braced(text: &str, p: usize, to: usize) -> Found<'_> {
    let b = text.as_bytes();
    let bad = |fault| Found::BadBrace { start: p, end: p + 2, fault };
    let n0 = p + 2;
    let mut n1 = n0;
    while n1 < to && is_name_byte(b[n1]) {
        n1 += 1;
    }
    let name = match text.get(n0..n1) {
        Some(n) if !n.is_empty() && n1 < to => n,
        _ => return bad(BraceFault::Malformed),
    };
    if b[n1] == b'}' {
        return Found::Placeholder(Placeholder { name, format: None, start: p, end: n1 + 1, braced: true });
    }
    if b[n1] != b':' {
        return bad(BraceFault::Malformed);
    }
    let f0 = n1 + 1;
    let mut f1 = f0;
    while f1 < to && is_name_byte(b[f1]) {
        f1 += 1;
    }
    if f1 >= to || b[f1] != b'}' {
        return bad(BraceFault::Malformed);
    }
    let fname = text.get(f0..f1).unwrap_or("");
    match Format::parse(fname) {
        Some(format) => {
            Found::Placeholder(Placeholder { name, format: Some(format), start: p, end: f1 + 1, braced: true })
        }
        None => bad(BraceFault::UnknownFormat(fname.to_owned())),
    }
}

/// Finds the next `[[name]]` in `text[from..to]`. It gives the offset of the first `[` and the name.
///
/// The form is not a placeholder: it stays text (7.6). Warning 2 of 7.18 uses it.
pub fn next_double_bracket(text: &str, from: usize, to: usize) -> Option<(usize, &str)> {
    let b = text.as_bytes();
    let to = to.min(b.len());
    let mut p = from;
    while p + 1 < to {
        if b[p] == b'[' && b[p + 1] == b'[' {
            let mut n = p + 2;
            while n < to && is_name_byte(b[n]) {
                n += 1;
            }
            if n > p + 2 && n + 1 < to && b[n] == b']' && b[n + 1] == b']' {
                return Some((p, text.get(p + 2..n)?));
            }
        }
        p += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ph(text: &str) -> Placeholder<'_> {
        match next(text, 0, text.len()) {
            Some(Found::Placeholder(p)) => p,
            other => panic!("{text:?}: {other:?}"),
        }
    }

    #[test]
    fn dollar_name_takes_the_longest_run() {
        let p = ph("x $env_name y");
        assert_eq!((p.name, p.start, p.end, p.braced), ("env_name", 2, 11, false));
        let p = ph("${env}_name");
        assert_eq!((p.name, p.end, p.braced), ("env", 6, true));
        assert_eq!(ph("$a1_B2-").name, "a1_B2");
    }

    #[test]
    fn formats() {
        assert_eq!(ph("${a:csv}").format, Some(Format::Csv));
        assert_eq!(ph("${a:raw}").format, Some(Format::Raw));
        assert_eq!(ph("${a:singlequote}").format, Some(Format::SingleQuote));
        assert_eq!(ph("${a:sqlstring}").format, Some(Format::SqlString));
        assert_eq!(ph("${a}").format, None);
        for f in LITERAL_ONLY_FORMATS {
            assert_eq!(ph(&format!("${{a:{}}}", f.name())).format, Some(f));
            assert!(f.literal_only());
            assert_eq!(Format::parse(f.name()), Some(f));
        }
        for f in [Format::Raw, Format::Csv, Format::SingleQuote, Format::SqlString] {
            assert!(!f.literal_only());
        }
        // The formats of Grafana that Hune does not know stay a fault.
        for name in ["date", "join", "uriencode", "customqueryparam", "JSON", "Csv"] {
            let t = format!("${{a:{name}}}");
            match next(&t, 0, t.len()) {
                Some(Found::BadBrace { fault: BraceFault::UnknownFormat(f), start: 0, end: 2 }) => assert_eq!(f, name),
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn malformed_braces() {
        for t in ["${", "${}", "${a", "${a:", "${a:csv", "${a b}", "${a-b}", "${:csv}", "${a:}"] {
            assert!(matches!(next(t, 0, t.len()), Some(Found::BadBrace { .. })), "{t:?}");
        }
    }

    #[test]
    fn lone_dollar_is_text() {
        for t in ["$", "$ a", "a$", "$(x)", "$-1", "$$"] {
            assert_eq!(next(t, 0, t.len()), None, "{t:?}");
        }
        // A `$` that follows a `$` can start a placeholder.
        assert_eq!(ph("$$a").name, "a");
    }

    #[test]
    fn range_bounds_are_respected() {
        let t = "$a $b";
        assert_eq!(next(t, 0, 2).map(|f| f.end()), Some(2));
        assert_eq!(next(t, 0, 1), None);
        assert_eq!(next(t, 2, 5).map(|f| f.start()), Some(3));
        let t = "${a}";
        assert!(matches!(next(t, 0, 3), Some(Found::BadBrace { .. })));
    }

    #[test]
    fn double_brackets() {
        assert_eq!(next_double_bracket("x [[a1]] y", 0, 10), Some((2, "a1")));
        assert_eq!(next_double_bracket("[[a]", 0, 4), None);
        assert_eq!(next_double_bracket("[[]]", 0, 4), None);
        assert_eq!(next_double_bracket("[[a b]]", 0, 7), None);
    }

    #[test]
    fn postgres_parameter_is_not_a_placeholder() {
        fn pg(t: &str) -> Option<Found<'_>> {
            next_for(Dialect::Postgres, t, 0, t.len())
        }
        fn ch(t: &str) -> Option<Found<'_>> {
            next_for(Dialect::ClickHouse, t, 0, t.len())
        }
        // `$1` is a parameter token (7.20): no placeholder, with the digits skipped.
        assert_eq!(pg("$1"), None);
        assert_eq!(pg("$12 + 1"), None);
        assert_eq!(pg("$1abc"), None);
        assert!(matches!(pg("$1 $a"), Some(Found::Placeholder(p)) if p.name == "a" && p.start == 3));
        assert!(matches!(pg("$1${a}"), Some(Found::Placeholder(p)) if p.name == "a" && p.braced));
        assert!(matches!(pg("$1$a"), Some(Found::Placeholder(p)) if p.name == "a" && p.start == 2));
        // The ClickHouse path is `next`: `$1` names the variable `1`.
        assert!(matches!(ch("$1"), Some(Found::Placeholder(p)) if p.name == "1"));
        // Other forms are the same in both dialects.
        for t in ["$a", "${a}", "${a:csv}", "x $env_name y", "${", "${a:date}", "$", "a$b", "$$a", "$_x1"] {
            assert_eq!(pg(t), ch(t), "{t:?}");
        }
        // A range that ends inside the digits.
        assert_eq!(next_for(Dialect::Postgres, "$1a$b", 0, 2), None);
        assert!(matches!(next_for(Dialect::Postgres, "a$b", 0, 3), Some(Found::Placeholder(_))));
    }

    #[test]
    fn postgres_scan_is_linear() {
        // A long run of lone `$` and digits must not take a quadratic time.
        let t = "$ ".repeat(200_000) + &"$1".repeat(100_000);
        assert_eq!(next_for(Dialect::Postgres, &t, 0, t.len()), None);
    }
}
