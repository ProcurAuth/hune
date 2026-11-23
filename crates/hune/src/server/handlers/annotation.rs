// SPDX-License-Identifier: MIT OR Apache-2.0
//! The annotation route (6.21a, 7.37): the stored query of one annotation gives time marks.
//!
//! The route has the same guarantees as a panel query. The body has no SQL. The server reads the SQL from the
//! catalog, expands it with the engine of 7.2, checks the property P and takes the admission and the limits of
//! section 8. It turns the frame into marks with `annotations_from_frame` and refuses what it cannot draw.
//! A `postgres` source uses its dialect and the property P of 7.24, and a `mongodb` source uses the pipeline
//! grammar and the property PM of 7.35. The frame rules of 7.37 are the same for the three kinds.

use super::dashboards::path_uid;
use super::query::{
    RunInfo, block, checked_document, effective_limits_capped, etag_faults, expand_query, failure, read_etag,
    read_grant, read_range, read_values, run_query, variable_defs,
};
use crate::server::ctx::{Ctx, Request};
use crate::server::error::{ApiError, Code};
use crate::server::json::{Member, Shape, parse_and_check_with};
use crate::server::response::{Response, json_response};
use http::StatusCode;
use hune_core::annotation::{Mark, annotations_from_frame};
use hune_core::limits::{MAX_ANNOTATION_ROWS, MAX_ANNOTATIONS};
use hune_source::{QueryKind, SourceKind};
use serde::Serialize;
use serde_json::Value;

const ANY: Shape = Shape::Any;
const NUMBER: Shape = Shape::Int { min: i64::MIN, max: i64::MAX };

/// The members of the body (6.21a). The body has no `max_data_points`, no SQL and no other member.
static ANNOTATION_MEMBERS: [Member; 4] = [
    Member { name: "etag", shape: Shape::Str { min_chars: 1, max_chars: 128 }, required: true },
    Member { name: "from_ms", shape: NUMBER, required: true },
    Member { name: "to_ms", shape: NUMBER, required: true },
    Member { name: "variables", shape: ANY, required: true },
];
static ANNOTATION_SHAPE: Shape = Shape::Object(&ANNOTATION_MEMBERS);

/// Reads the index of the path: a whole number from 0 to `MAX_ANNOTATIONS` minus 1, with no sign and no
/// leading zero. Any other text names no annotation.
fn path_index(ctx: &Ctx) -> Option<usize> {
    let text = ctx.params.get("index")?;
    let canonical =
        !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) && (text == "0" || !text.starts_with('0'));
    canonical.then(|| text.parse::<usize>().ok()).flatten().filter(|i| *i < MAX_ANNOTATIONS)
}

fn not_found() -> ApiError {
    ApiError::not_found("No such annotation.")
}

/// Tells if an annotation can run on a source of this kind (4.5b). The match
/// has no default arm, so a kind that 8.2 adds later is not valid for an annotation until its own decision
/// (8.14). The save (7.18) and the route (6.21a) use this function.
pub(crate) fn annotation_kind(kind: SourceKind) -> bool {
    match kind {
        SourceKind::ClickHouse | SourceKind::Postgres | SourceKind::MongoDb => true,
    }
}

#[derive(Serialize)]
struct MarkItem<'a>(i64, Option<i64>, &'a str, &'a str, &'a [String]);

#[derive(Serialize)]
struct Answer<'a> {
    etag: &'a str,
    query_id: &'a str,
    rows: usize,
    annotations: Vec<MarkItem<'a>>,
}

/// Makes the body of the answer of 6.21a from the marks.
fn answer_body<'a>(etag: &'a str, query_id: &'a str, marks: &'a [Mark]) -> Answer<'a> {
    Answer {
        etag,
        query_id,
        rows: marks.len(),
        annotations: marks
            .iter()
            .map(|m| MarkItem(m.time_ms, m.end_ms, m.title.as_str(), m.text.as_str(), m.tags.as_slice()))
            .collect(),
    }
}

/// `POST /api/dashboards/{uid}/annotations/{index}/query` (6.21a, 7.37).
pub async fn annotation_query(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let uid = path_uid(&ctx)?;
    let body = parse_and_check_with(&req.body, &ANNOTATION_SHAPE, &[], etag_faults)?;
    let etag = read_etag(&body.value)?;
    let grant = read_grant(&ctx, &uid).await?;
    let doc = checked_document(&ctx, &uid, &etag).await?;
    let index = path_index(&ctx).ok_or_else(not_found)?;
    let annotation = doc.document().annotations().get(index).ok_or_else(not_found)?;
    if !annotation.enabled {
        return Err(ApiError::bad_request("This annotation is disabled."));
    }
    let source_id = annotation.source.as_str();
    let cfg = block(&ctx.state, source_id)
        .ok_or_else(|| ApiError::not_found("The source of the annotation is not in the configuration."))?;
    // The three kinds of 8.2 run annotations. The expansion and the source follow
    // the kind: 7.5 to 7.13, 7.24 and 8a for `postgres`, 7.35 and 8b for `mongodb`.
    if !annotation_kind(cfg.kind) {
        return Err(ApiError::bad_request("An annotation cannot run on a source of this kind."));
    }
    let source = ctx
        .state
        .source(source_id)
        .ok_or_else(|| ApiError::not_found("The source of the annotation is not in the configuration."))?;
    let range = read_range(&body.value)?;
    // The filters of an `adhoc` variable are checked on this route too (7.7a); an annotation has no macro.
    let (plain, _) = super::query::adhoc_split(&body.value, doc.document(), &ctx.state)?;
    let values = read_values(&plain, &variable_defs(doc.document()), &range)?;
    // No panel and no interval (7.37): the macros that need the interval are refused at the expansion.
    let expanded = expand_query(&cfg, &annotation.sql, &values, range, None)?;
    let info = RunInfo {
        kind: QueryKind::Annotation,
        uid: Some(uid.as_str()),
        folder: None,
        panel: None,
        query_ref: None,
        variable: None,
        annotation: Some(u32::try_from(index).map_err(|_| ApiError::internal())?),
        source: source_id,
        // The rule of 7.16b applies to the format `time_series` of a panel only. An annotation frame has the
        // format `table` (4.5b, rule 2): a number in its `time` column is epoch milliseconds (7.37, rule 4).
        time_series: false,
        action: "dashboard.read",
    };
    let limits = effective_limits_capped(&cfg, Some(MAX_ANNOTATION_ROWS as u64), &grant.ceilings);
    let frame = match run_query(&ctx, source, &info, expanded, limits).await {
        Ok(frame) => frame,
        Err(f) => return failure(f),
    };
    // The source stops a longer result before this point. This check keeps the cap if a source gives more.
    if frame.rows > MAX_ANNOTATION_ROWS {
        return Err(ApiError::new(Code::QueryLimit, "The result has more rows than an annotation query permits.")
            .with("limit", "max_result_rows")
            .with("value", MAX_ANNOTATION_ROWS as u64));
    }
    match annotations_from_frame(&frame) {
        Ok(marks) => Ok(json_response(StatusCode::OK, &answer_body(&etag.to_hex(), &frame.query_id, &marks))),
        Err(fault) => Err(ApiError::new(Code::AnnotationInvalid, fault.reason.message())
            .with("reason", fault.reason.as_str())
            .with("row", fault.row.map_or(Value::Null, |r| Value::from(r as u64)))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_kinds_of_source_run_annotations() {
        for kind in [SourceKind::ClickHouse, SourceKind::Postgres, SourceKind::MongoDb] {
            assert!(annotation_kind(kind), "{kind:?}");
        }
    }

    #[test]
    fn the_answer_has_five_items_for_each_mark() {
        let marks = vec![
            Mark { time_ms: 5, end_ms: None, title: String::new(), text: "t".into(), tags: vec!["a".into()] },
            Mark { time_ms: 6, end_ms: Some(9), title: "T".into(), text: String::new(), tags: Vec::new() },
        ];
        let v = serde_json::to_value(answer_body("e", "q", &marks)).expect("json");
        assert_eq!(v["rows"], 2);
        assert_eq!(v["annotations"][0], serde_json::json!([5, null, "", "t", ["a"]]));
        assert_eq!(v["annotations"][1], serde_json::json!([6, 9, "T", "", []]));
    }
}
