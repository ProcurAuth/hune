//! The attribution of a query (8b.11).
//!
//! Each command of a query has the member `comment`: a document. MongoDB shows it in the log, in
//! `currentOp` and in the profiler. An operator finds the queries of Hune with
//! `db.currentOp({"command.comment.app": "hune"})`. The driver option `appName` is [`APP_NAME`].
//!
//! The members have this order: `app`, `kind`, `uid` or `folder`, `panel` (then `ref`) or `variable`, `query_id`,
//! `request_id` and `subject`. A value is a text, or an integer for `panel`. The document has 1,024
//! bytes at most, as the compact JSON text counts them. When it is longer, [`comment_document`]
//! shortens `subject`. Then it drops `subject`, `request_id`, `variable`, `folder` and `uid`, in this
//! order, until it fits.

use hune_core::mongo::frame::write_relaxed_json;
use hune_core::mongo::{Document, MongoValue};

use crate::types::{LOG_COMMENT_MAX_BYTES, LOG_COMMENT_SUBJECT_MAX_BYTES, LogComment, LogKind, Uuid, cut_utf8};

/// The value of the driver option `appName` and of the member `app` (8b.4, 8b.11).
pub const APP_NAME: &str = "hune";

fn kind_text(kind: LogKind) -> &'static str {
    match kind {
        LogKind::Panel => "panel",
        LogKind::Options => "options",
        LogKind::Annotation => "annotation",
        LogKind::Preview => "preview",
        LogKind::Probe => "probe",
    }
}

fn text(s: &str) -> MongoValue {
    MongoValue::String(s.to_owned())
}

/// Which optional members the document keeps.
#[derive(Clone, Copy)]
struct Keep {
    subject: usize,
    request_id: bool,
    variable: bool,
    folder: bool,
    uid: bool,
}

fn build(c: &LogComment, query_id: &Uuid, keep: Keep) -> Document {
    let mut d: Document = vec![("app".to_owned(), text(APP_NAME)), ("kind".to_owned(), text(kind_text(c.kind)))];
    if let (Some(v), true) = (&c.uid, keep.uid) {
        d.push(("uid".to_owned(), text(v)));
    }
    if let (Some(v), true) = (&c.folder, keep.folder) {
        d.push(("folder".to_owned(), text(v)));
    }
    if let Some(p) = c.panel {
        let name = if c.kind == LogKind::Annotation { "annotation" } else { "panel" };
        d.push((name.to_owned(), i32::try_from(p).map_or(MongoValue::Int64(i64::from(p)), MongoValue::Int32)));
    }
    if let Some(v) = &c.query_ref {
        d.push(("ref".to_owned(), text(v)));
    }
    if let (Some(v), true) = (&c.variable, keep.variable) {
        d.push(("variable".to_owned(), text(v)));
    }
    d.push(("query_id".to_owned(), text(&query_id.to_string())));
    if let (Some(v), true) = (&c.request_id, keep.request_id) {
        d.push(("request_id".to_owned(), text(v)));
    }
    if let (Some(v), true) = (&c.subject, keep.subject > 0) {
        d.push(("subject".to_owned(), text(cut_utf8(v, keep.subject))));
    }
    d
}

/// Gives the length of the compact JSON text of a document.
fn json_len(d: &Document) -> usize {
    let mut out = String::new();
    // The members are texts and integers: the writer cannot fail.
    let _ = write_relaxed_json(&mut out, &MongoValue::Document(d.clone()));
    out.len()
}

/// Makes the comment document of a query: the members of 8b.11, shortened to 1,024 bytes.
pub fn comment_document(comment: &LogComment, query_id: &Uuid) -> MongoValue {
    let mut keep = Keep {
        subject: comment.subject.as_deref().map_or(0, |s| cut_utf8(s, LOG_COMMENT_SUBJECT_MAX_BYTES).len()),
        request_id: true,
        variable: true,
        folder: true,
        uid: true,
    };
    loop {
        let d = build(comment, query_id, keep);
        if json_len(&d) <= LOG_COMMENT_MAX_BYTES {
            return MongoValue::Document(d);
        }
        // Shorten `subject`, then drop it. Then drop the others, in the order of 8b.11.
        if keep.subject > 1 {
            keep.subject /= 2;
        } else if keep.subject == 1 {
            keep.subject = 0;
        } else if keep.request_id {
            keep.request_id = false;
        } else if keep.variable {
            keep.variable = false;
        } else if keep.folder {
            keep.folder = false;
        } else if keep.uid {
            keep.uid = false;
        } else {
            return MongoValue::Document(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_id() -> Uuid {
        Uuid::parse("019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d").unwrap()
    }

    fn full() -> LogComment {
        LogComment {
            kind: LogKind::Panel,
            uid: Some("node-load".to_owned()),
            folder: None,
            panel: Some(3),
            query_ref: None,
            variable: None,
            request_id: Some("req-1".to_owned()),
            subject: Some("alice".to_owned()),
        }
    }

    fn json(v: &MongoValue) -> String {
        let mut out = String::new();
        write_relaxed_json(&mut out, v).unwrap();
        out
    }

    #[test]
    fn the_ref_of_a_query_comes_after_the_panel() {
        let mut c = full();
        c.query_ref = Some("C".to_owned());
        let v = comment_document(&c, &query_id());
        let text = json(&v);
        let panel = text.find("\"panel\"").unwrap();
        let query_ref = text.find("\"ref\":\"C\"").unwrap();
        let query = text.find("\"query_id\"").unwrap();
        assert!(panel < query_ref && query_ref < query, "{text}");
    }

    #[test]
    fn the_members_have_the_order_of_8b11() {
        let v = comment_document(&full(), &query_id());
        assert_eq!(
            json(&v),
            r#"{"app":"hune","kind":"panel","uid":"node-load","panel":3,"query_id":"019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d","request_id":"req-1","subject":"alice"}"#
        );
        let preview = LogComment {
            kind: LogKind::Preview,
            uid: None,
            folder: Some("ops".to_owned()),
            panel: None,
            query_ref: None,
            variable: Some("host".to_owned()),
            request_id: Some("r".to_owned()),
            subject: None,
        };
        assert_eq!(
            json(&comment_document(&preview, &query_id())),
            r#"{"app":"hune","kind":"preview","folder":"ops","variable":"host","query_id":"019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d","request_id":"r"}"#
        );
        // The app name is the one of the driver option.
        assert_eq!(APP_NAME, "hune");
    }

    #[test]
    fn an_annotation_query_names_its_index_in_place_of_the_panel() {
        // 8b.11: the kind `annotation` and the integer member `annotation` (decisions ID-K142 and ID-K333).
        let mut c = full();
        c.kind = LogKind::Annotation;
        c.panel = Some(5);
        let v = comment_document(&c, &query_id());
        assert_eq!(
            json(&v),
            r#"{"app":"hune","kind":"annotation","uid":"node-load","annotation":5,"query_id":"019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d","request_id":"req-1","subject":"alice"}"#
        );
        assert_eq!(v.get("annotation"), Some(&MongoValue::Int32(5)));
        assert!(v.get("panel").is_none());
    }

    #[test]
    fn the_comment_of_a_probe_has_the_app_the_kind_and_the_query_id() {
        let v = comment_document(&LogComment::probe(), &query_id());
        assert_eq!(json(&v), r#"{"app":"hune","kind":"probe","query_id":"019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d"}"#);
        assert_eq!(v.get("app"), Some(&MongoValue::String("hune".to_owned())));
    }

    #[test]
    fn the_panel_is_an_integer_and_the_other_values_are_texts() {
        let v = comment_document(&full(), &query_id());
        assert_eq!(v.get("panel"), Some(&MongoValue::Int32(3)));
        for key in ["app", "kind", "uid", "query_id", "request_id", "subject"] {
            assert!(matches!(v.get(key), Some(MongoValue::String(_))), "{key}");
        }
    }

    #[test]
    fn a_long_subject_is_shortened_and_then_the_members_are_dropped_in_order() {
        let mut c = full();
        c.subject = Some("s".repeat(5000));
        let v = comment_document(&c, &query_id());
        assert!(json(&v).len() <= 1024);
        let Some(MongoValue::String(s)) = v.get("subject") else { panic!("the subject stays, shortened") };
        assert!(!s.is_empty() && s.len() <= 256);
        // The other members stay.
        assert!(v.get("request_id").is_some() && v.get("uid").is_some());
        // A long request id forces the subject out first, then the request id.
        let mut c = full();
        c.request_id = Some("r".repeat(2000));
        let v = comment_document(&c, &query_id());
        assert!(json(&v).len() <= 1024);
        assert!(v.get("subject").is_none() && v.get("request_id").is_none());
        assert!(v.get("uid").is_some());
        // A long variable goes after the request id.
        let mut c = full();
        c.uid = None;
        c.panel = None;
        c.variable = Some("v".repeat(2000));
        let v = comment_document(&c, &query_id());
        assert!(v.get("variable").is_none() && v.get("subject").is_none());
        // A long uid goes last. The members `app`, `kind`, `panel` and `query_id` stay.
        let mut c = full();
        c.uid = Some("u".repeat(3000));
        let v = comment_document(&c, &query_id());
        assert!(json(&v).len() <= 1024);
        assert!(v.get("uid").is_none());
        for key in ["app", "kind", "panel", "query_id"] {
            assert!(v.get(key).is_some(), "{key}");
        }
    }

    #[test]
    fn the_comment_fits_1024_bytes_for_many_subjects() {
        // A small deterministic generator: texts of many lengths, with multi-byte characters.
        let mut x: u64 = 0x1234_5678_9abc_def0;
        for _ in 0..1000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let len = (x % 3000) as usize;
            let ch = ["a", "é", "水", "\"", "\\", "😀"][(x >> 20) as usize % 6];
            let mut c = full();
            c.subject = Some(ch.repeat(len));
            c.request_id = Some(ch.repeat((x >> 40) as usize % 800));
            let v = comment_document(&c, &query_id());
            assert!(json(&v).len() <= 1024, "{}", json(&v).len());
            assert!(v.get("app").is_some() && v.get("query_id").is_some());
        }
    }
}
