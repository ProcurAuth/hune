//! The attribution of a query (8a.11, ID-PG13).
//!
//! This module makes the block comment `/*hune {json}*/` that goes before the SQL, and the value
//! `hune/<kind>/<query_id>` of `application_name` for the transaction.
//!
//! Safety rule of the comment. PostgreSQL block comments nest, so a `/*` or a `*/` in the comment
//! could swallow the SQL or end the comment early. Each text member is percent-encoded: each byte
//! outside `A-Z`, `a-z`, `0-9`, `-`, `.`, `_` and `~` becomes `%XX`. This holds for `subject` (8a.11)
//! and, as a second barrier, for `uid`, `folder`, `variable` and `request_id`. A valid id has no
//! byte to encode, so the text of such a member does not change. No `*`, no `/`, no quote mark and
//! no backslash can occur in the comment, so the JSON needs no escape.
//!
//! The comment has 1,024 bytes at most. When it is longer, the function shortens `subject`, then
//! drops it, then drops `request_id`, `variable`, `folder` and `uid`, in this order. The comment is
//! not part of the size limit `max_query_size` (8a.5).

use std::fmt::Write as _;

use crate::types::{LOG_COMMENT_MAX_BYTES, LOG_COMMENT_SUBJECT_MAX_BYTES, LogComment, LogKind, Uuid, cut_utf8};

/// The longest value of `application_name` that PostgreSQL keeps (`NAMEDATALEN` - 1).
#[allow(dead_code, reason = "Only the tests of the module read it.")]
pub(crate) const APPLICATION_NAME_MAX_BYTES: usize = 63;

/// The text of `kind` in the comment and in `application_name`.
fn kind_text(kind: LogKind) -> &'static str {
    match kind {
        LogKind::Panel => "panel",
        LogKind::Options => "options",
        LogKind::Annotation => "annotation",
        LogKind::Preview => "preview",
        LogKind::Probe => "probe",
    }
}

/// Percent-encodes `s`: each byte outside `A-Z`, `a-z`, `0-9`, `-`, `.`, `_` and `~` becomes `%XX`.
pub(crate) fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// The members of the comment, after the encoding. `None` drops the member.
struct Members<'a> {
    c: &'a LogComment,
    query_id: String,
    subject: Option<String>,
    keep_request_id: bool,
    keep_variable: bool,
    keep_folder: bool,
    keep_uid: bool,
}

impl Members<'_> {
    fn render(&self) -> String {
        let c = self.c;
        let mut s = String::from(r#"{"app":"hune","kind":""#);
        s.push_str(kind_text(c.kind));
        s.push('"');
        let text = |s: &mut String, name: &str, v: &str| {
            let _ = write!(s, ",\"{name}\":\"{}\"", percent_encode(v));
        };
        if let (Some(v), true) = (&c.uid, self.keep_uid) {
            text(&mut s, "uid", v);
        }
        if let (Some(v), true) = (&c.folder, self.keep_folder) {
            text(&mut s, "folder", v);
        }
        if let Some(p) = c.panel {
            let name = if c.kind == LogKind::Annotation { "annotation" } else { "panel" };
            let _ = write!(s, ",\"{name}\":{p}");
        }
        if let Some(v) = &c.query_ref {
            text(&mut s, "ref", v);
        }
        if let (Some(v), true) = (&c.variable, self.keep_variable) {
            text(&mut s, "variable", v);
        }
        let _ = write!(s, ",\"query_id\":\"{}\"", self.query_id);
        if let (Some(v), true) = (&c.request_id, self.keep_request_id) {
            text(&mut s, "request_id", v);
        }
        if let Some(v) = &self.subject {
            // The subject is encoded once: the cut comes before the encoding.
            let _ = write!(s, ",\"subject\":\"{v}\"");
        }
        s.push('}');
        s
    }
}

/// Makes the attribution comment of 8a.11: `/*hune {json}*/`.
///
/// The members follow the order of 8.10, with `ref` after `panel` and `query_id` after `panel`, `ref` or `variable`.
pub(crate) fn comment(log: &LogComment, query_id: &Uuid) -> String {
    let mut m = Members {
        c: log,
        query_id: query_id.to_string(),
        subject: log
            .subject
            .as_deref()
            .map(|s| percent_encode(cut_utf8(s, LOG_COMMENT_SUBJECT_MAX_BYTES)))
            .filter(|s| !s.is_empty()),
        keep_request_id: true,
        keep_variable: true,
        keep_folder: true,
        keep_uid: true,
    };
    let fits = |m: &Members<'_>| m.render().len() <= LOG_COMMENT_MAX_BYTES - "/*hune */".len();
    // First shorten the subject, with whole `%XX` groups, then drop each member in turn.
    while !fits(&m) {
        match m.subject.take() {
            Some(s) if s.len() > 8 => {
                let mut cut = s.len() / 2;
                // Never cut inside a `%XX` group.
                if let Some(i) = s[..cut].rfind('%')
                    && i + 3 > cut
                {
                    cut = i;
                }
                m.subject = Some(s[..cut].to_owned());
            }
            Some(_) => {}
            None => {
                if m.keep_request_id {
                    m.keep_request_id = false;
                } else if m.keep_variable {
                    m.keep_variable = false;
                } else if m.keep_folder {
                    m.keep_folder = false;
                } else if m.keep_uid {
                    m.keep_uid = false;
                } else {
                    break;
                }
            }
        }
    }
    format!("/*hune {}*/", m.render())
}

/// Makes the text of step 6 of 8a.5: the comment, a line end, and the final SQL.
pub(crate) fn commented_sql(log: &LogComment, query_id: &Uuid, sql: &str) -> String {
    let mut text = comment(log, query_id);
    text.push('\n');
    text.push_str(sql);
    text
}

/// Gives the `application_name` of a query: `hune/<kind>/<query_id>` (8a.11).
///
/// The probe has no query id and uses `hune/probe`.
pub(crate) fn application_name(kind: LogKind, query_id: &Uuid) -> String {
    match kind {
        LogKind::Probe => "hune/probe".to_owned(),
        k => format!("hune/{}/{}", kind_text(k), query_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qid() -> Uuid {
        Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").unwrap()
    }

    fn panel(subject: Option<&str>) -> LogComment {
        LogComment {
            kind: LogKind::Panel,
            uid: Some("nodes".into()),
            folder: None,
            panel: Some(4),
            query_ref: None,
            variable: None,
            request_id: Some("r-5f2c".into()),
            subject: subject.map(str::to_owned),
        }
    }

    /// The comment is one block comment. Nested comments are a trap: count the marks.
    fn closes_only_at_the_end(c: &str) -> bool {
        c.starts_with("/*hune {")
            && c.ends_with("}*/")
            && !c[2..c.len() - 2].contains("/*")
            && !c[2..c.len() - 2].contains("*/")
            && !c[2..c.len() - 2].contains('*')
            && !c[2..c.len() - 2].contains('/')
    }

    #[test]
    fn spec_example() {
        let c = comment(&panel(Some("user:alice")), &qid());
        assert_eq!(
            c,
            r#"/*hune {"app":"hune","kind":"panel","uid":"nodes","panel":4,"query_id":"019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d","request_id":"r-5f2c","subject":"user%3Aalice"}*/"#
        );
        // The JSON is valid.
        let json = &c["/*hune ".len()..c.len() - 2];
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(v["subject"], "user%3Aalice");
    }

    #[test]
    fn an_annotation_query_names_its_index_in_place_of_the_panel() {
        // 8a.11: the kind `annotation` and the member `annotation` (decisions ID-K142 and ID-K333).
        let mut l = panel(None);
        l.kind = LogKind::Annotation;
        l.panel = Some(7);
        let c = comment(&l, &qid());
        assert_eq!(
            c,
            r#"/*hune {"app":"hune","kind":"annotation","uid":"nodes","annotation":7,"query_id":"019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d","request_id":"r-5f2c"}*/"#
        );
        let name = application_name(LogKind::Annotation, &qid());
        assert_eq!(name, "hune/annotation/019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d");
        assert!(name.len() <= APPLICATION_NAME_MAX_BYTES);
    }

    #[test]
    fn the_ref_of_a_query_comes_after_the_panel() {
        let mut l = panel(None);
        l.query_ref = Some("B".into());
        let c = comment(&l, &qid());
        assert_eq!(
            c,
            r#"/*hune {"app":"hune","kind":"panel","uid":"nodes","panel":4,"ref":"B","query_id":"019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d","request_id":"r-5f2c"}*/"#
        );
    }

    #[test]
    fn order_and_variants() {
        let mut l = panel(None);
        l.panel = None;
        l.uid = None;
        l.folder = Some("ops".into());
        l.variable = Some("node".into());
        let c = comment(&l, &qid());
        let f = c.find("\"folder\"").unwrap();
        let v = c.find("\"variable\"").unwrap();
        let q = c.find("\"query_id\"").unwrap();
        let r = c.find("\"request_id\"").unwrap();
        assert!(f < v && v < q && q < r);
        assert!(!c.contains("subject"));
        let probe = comment(&LogComment::probe(), &qid());
        assert_eq!(
            probe,
            r#"/*hune {"app":"hune","kind":"probe","query_id":"019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d"}*/"#
        );
    }

    #[test]
    fn percent_encoding_rule() {
        assert_eq!(percent_encode("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(percent_encode("a b/c*d\"e\\f%"), "a%20b%2Fc%2Ad%22e%5Cf%25");
        assert_eq!(percent_encode("é"), "%C3%A9");
        assert_eq!(percent_encode("\0\n"), "%00%0A");
    }

    #[test]
    fn hostile_members_never_open_or_close_the_comment() {
        let mut l = panel(Some("*/ DROP /* x"));
        l.uid = Some("a*/b".into());
        l.request_id = Some("/*".into());
        l.variable = Some("*/".into());
        l.folder = Some("*/*".into());
        let c = comment(&l, &qid());
        assert!(closes_only_at_the_end(&c), "{c}");
        let json = &c["/*hune ".len()..c.len() - 2];
        serde_json::from_str::<serde_json::Value>(json).unwrap();
    }

    #[test]
    fn random_subjects_never_break_the_comment() {
        // A small xorshift: the test needs no random crate.
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let alphabet: Vec<char> = "*/\\\"%\n\0 ab9-é漢😀{}:,".chars().collect();
        for _ in 0..2000 {
            let n = (next() % 300) as usize;
            let s: String = (0..n).map(|_| alphabet[(next() as usize) % alphabet.len()]).collect();
            let c = comment(&panel(Some(&s)), &qid());
            assert!(closes_only_at_the_end(&c), "{c}");
            assert!(c.len() <= LOG_COMMENT_MAX_BYTES);
            let json = &c["/*hune ".len()..c.len() - 2];
            serde_json::from_str::<serde_json::Value>(json).unwrap();
        }
    }

    #[test]
    fn size_limit() {
        let long = "é*".repeat(1000);
        let c = comment(&panel(Some(&long)), &qid());
        assert!(c.len() <= LOG_COMMENT_MAX_BYTES, "{}", c.len());
        assert!(closes_only_at_the_end(&c));
        assert!(c.contains("\"subject\""));
        // Members that are too long drop out; the comment stays valid.
        let mut l = panel(Some("s"));
        l.uid = Some("u".repeat(2000));
        l.request_id = Some("r".repeat(2000));
        let c = comment(&l, &qid());
        assert!(c.len() <= LOG_COMMENT_MAX_BYTES);
        assert!(!c.contains("\"uid\""));
        assert!(c.contains("\"query_id\""));
        let json = &c["/*hune ".len()..c.len() - 2];
        serde_json::from_str::<serde_json::Value>(json).unwrap();
    }

    #[test]
    fn commented_sql_has_the_comment_first() {
        let t = commented_sql(&panel(None), &qid(), "SELECT 1");
        assert!(t.starts_with("/*hune {"));
        assert!(t.ends_with("}*/\nSELECT 1"));
    }

    #[test]
    fn application_names() {
        assert_eq!(application_name(LogKind::Panel, &qid()), "hune/panel/019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d");
        assert_eq!(application_name(LogKind::Probe, &qid()), "hune/probe");
        for k in [LogKind::Panel, LogKind::Options, LogKind::Preview, LogKind::Probe] {
            let n = application_name(k, &qid());
            assert!(n.len() <= APPLICATION_NAME_MAX_BYTES);
            assert!(n.is_ascii());
        }
        assert_eq!(application_name(LogKind::Options, &qid()).len(), "hune/options/".len() + 36);
    }
}
