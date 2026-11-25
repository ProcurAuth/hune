// SPDX-License-Identifier: MIT OR Apache-2.0
//! `POST /api/sources/{id}/preview` (6.22, 7.17): a draft panel or the options of a draft variable.
//!
//! The server checks the draft with section 4 and 7.18. It puts the draft in a document that has
//! nothing else. It validates this document with `hune-core`. Then it maps the pointers of the
//! faults to pointers of the body.

use super::gate;
use super::query::{
    RunInfo, block, effective_limits, expand_query, failure, frame_response_with_sql, options_response, read_interval,
    read_range, read_values, run_query, variable_defs,
};
use super::support::{services, validate_document};
use crate::server::ctx::{Ctx, Request};
use crate::server::error::ApiError;
use crate::server::json::{Member, Shape, parse_and_check};
use crate::server::response::Response;
use hune_core::document::{PanelData, QueryFormat, VariableKind as DocVariableKind};
use hune_core::error::{ErrorCode, ValidationError};
use hune_source::QueryKind;
use serde_json::{Map, Value, json};

const NUMBER: Shape = Shape::Int { min: i64::MIN, max: i64::MAX };
static ANY: Shape = Shape::Any;

static MEMBERS: [Member; 9] = [
    Member { name: "uid", shape: Shape::Str { min_chars: 1, max_chars: 40 }, required: false },
    Member { name: "folder", shape: Shape::Str { min_chars: 1, max_chars: 40 }, required: false },
    Member { name: "from_ms", shape: NUMBER, required: true },
    Member { name: "to_ms", shape: NUMBER, required: true },
    Member { name: "max_data_points", shape: NUMBER, required: false },
    Member { name: "dashboard_variables", shape: Shape::Array { item: &ANY, max: 64 }, required: true },
    Member { name: "variables", shape: Shape::Any, required: true },
    Member { name: "panel", shape: Shape::Any, required: false },
    Member { name: "variable", shape: Shape::Str { min_chars: 1, max_chars: 64 }, required: false },
];
static SHAPE: Shape = Shape::Object(&MEMBERS);

/// Maps a pointer of the document that holds the draft to a pointer of the body.
pub fn body_pointer(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("/variables") {
        return format!("/dashboard_variables{rest}");
    }
    if let Some(rest) = p.strip_prefix("/sections/0/panels/0") {
        return format!("/panel{rest}");
    }
    if p.starts_with("/sections") {
        return "/panel".to_owned();
    }
    p.to_owned()
}

fn map_errors(errors: &[ValidationError]) -> Vec<ValidationError> {
    errors
        .iter()
        .map(|e| {
            let mut m = e.clone();
            m.pointer = body_pointer(&e.pointer);
            m
        })
        .collect()
}

/// Takes the member `placement` (4.7a) out of each draft variable. The draft document has one section, so a
/// placement of the real document would name a section that the draft does not have. The preview never uses it.
fn without_placements(variables: &Value) -> Value {
    let mut out = variables.clone();
    if let Some(items) = out.as_array_mut() {
        for item in items {
            if let Some(m) = item.as_object_mut() {
                m.remove("placement");
            }
        }
    }
    out
}

/// Makes the document that holds the draft: the variables of the body and the panel, if any.
fn draft_document(body: &Value) -> Vec<u8> {
    let mut sections = Map::new();
    sections.insert("title".into(), Value::Null);
    sections.insert("collapsed".into(), Value::Bool(false));
    sections.insert("panels".into(), Value::Array(body.get("panel").cloned().into_iter().collect()));
    let doc = json!({
        "format_version": hune_core::document::FORMAT_VERSION,
        "uid": "preview",
        "title": "Preview",
        "description": "",
        "tags": [],
        "time": {"from": "now-6h", "to": "now", "timezone": "utc", "refresh": null},
        "cursor_sync": "off",
        "links": [],
        "variables": without_placements(&body["dashboard_variables"]),
        "sections": [Value::Object(sections)],
    });
    serde_json::to_vec(&doc).unwrap_or_default()
}

/// `POST /api/sources/{id}/preview`.
pub async fn preview(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let source_id = ctx.params.get("id").unwrap_or("").to_owned();
    let body = parse_and_check(&req.body, &SHAPE, &[])?;
    let v = &body.value;
    let has = |name: &str| v.get(name).is_some();
    if has("uid") == has("folder") {
        return Err(ApiError::bad_request("The body must hold exactly one of uid and folder."));
    }
    if has("panel") == has("variable") {
        return Err(ApiError::bad_request("The body must hold exactly one of panel and variable."));
    }
    if has("panel") != has("max_data_points") {
        return Err(ApiError::bad_request("max_data_points is necessary with panel and absent with variable."));
    }
    if has("panel") && !v["panel"].is_object() {
        return Err(ApiError::bad_request_errors(
            "The body does not follow the schema.",
            &[ValidationError::new("/panel", ErrorCode::WrongType, "Expected an object.")],
        ));
    }
    // A text panel has no query (4.33a, 6.22): the answer comes before any other check of the panel.
    if v.pointer("/panel/type").and_then(Value::as_str) == Some("text") {
        return Err(ApiError::bad_request("This panel has no query."));
    }
    if v.pointer("/panel/data/reuse").is_some() {
        return Err(ApiError::bad_request("A panel that reuses the frame of another panel has no query."));
    }
    // A preview runs one query (6.22, 4.15a). The editor sends the selected query as `data.query`.
    if v.pointer("/panel/data/extra_queries").is_some() {
        return Err(ApiError::bad_request("A preview runs one query."));
    }
    // The source of the path: an unknown id gives 404 and no question (11.20, step 7).
    let source = ctx.state.source(&source_id).ok_or_else(|| ApiError::not_found("No such source."))?;
    let cfg = block(&ctx.state, &source_id).ok_or_else(ApiError::internal)?;
    // The saved dashboard or the folder of a new one.
    let services = services(&ctx)?;
    let uid = v.get("uid").and_then(Value::as_str).map(str::to_owned);
    let folder = v.get("folder").and_then(Value::as_str).map(str::to_owned);
    // The folder of the saved dashboard: the chain of the other permission (11.21).
    let mut saved_folder: Option<String> = None;
    if let Some(u) = &uid {
        let entry = hune_core::ids::Uid::parse(u).ok().and_then(|u| services.catalog.get(&u));
        match entry {
            Some(e) => saved_folder = Some(e.folder().as_str().to_owned()),
            None => return Err(gate::unknown(&ctx, "No such dashboard.").await),
        }
    }
    if let Some(f) = &folder
        && !services.catalog.folders().iter().any(|r| r.id.as_str() == f)
    {
        return Err(gate::unknown(&ctx, "No such folder.").await);
    }
    // A draft of a dashboard or of a folder of a read-only store is refused (5.42, 11.35).
    if let (Some(f), Some(u)) = (&saved_folder, &uid)
        && hune_core::ids::Uid::parse(u).is_ok_and(|u| services.store.uid_is_read_only(&u))
    {
        return Err(gate::read_only_refusal(&ctx, f, Some(u)).await);
    }
    if let Some(f) = &folder
        && hune_core::ids::FolderId::parse(f).is_ok_and(|f| services.store.folder_is_read_only(&f))
    {
        return Err(gate::read_only_refusal(&ctx, f, None).await);
    }
    // The decision comes before the validation of the draft and before the first byte to the source (11.20).
    let draft_ref = match (&saved_folder, &uid, &folder) {
        (Some(f), Some(u), _) => gate::Draft::Saved { folder: f, uid: u },
        (_, _, Some(f)) => gate::Draft::New { folder: f },
        _ => return Err(ApiError::internal()),
    };
    let grant = gate::preview(&ctx, &source_id, draft_ref).await?;
    // The draft: section 4, then 7.18, in `spawn_blocking` (6.27).
    let (doc, _warnings) = validate_document(&ctx, req.place.take(), draft_document(v), None, false)
        .await?
        .map_err(|errors| ApiError::invalid_document(&map_errors(&errors)))?;
    let defs = variable_defs(doc.document());
    let range = read_range(v)?;
    if has("panel") {
        let panel = doc.document().panels().next().map(|(_, p)| p).ok_or_else(ApiError::internal)?;
        let PanelData::Query(query) = &panel.data else { return Err(ApiError::internal()) };
        if query.source.as_str() != source_id {
            return Err(ApiError::invalid_document(&[ValidationError::new(
                "/panel/data/query/source",
                ErrorCode::BadReference,
                format!("source must be {source_id}, the source of the route; value {}", query.source),
            )]));
        }
        let interval = read_interval(v, &range, query.min_interval_ms())?;
        // The filters of the `adhoc` variable of the source apply to the draft panel too (7.7a, 7.10a).
        let (plain, adhoc_input) = super::query::adhoc_split(v, doc.document(), &ctx.state)?;
        let values = read_values(&plain, &defs, &range)?;
        let adhoc = super::query::adhoc_run(doc.document(), &ctx.state, &source_id, &adhoc_input)?;
        let expanded =
            super::query::expand_query_adhoc(&cfg, &query.sql, &values, range, Some(interval), adhoc.as_ref())?;
        let info = RunInfo {
            kind: QueryKind::Preview,
            uid: uid.as_deref(),
            folder: folder.as_deref(),
            panel: Some(panel.id),
            query_ref: None,
            variable: None,
            annotation: None,
            source: &source_id,
            time_series: query.format == QueryFormat::TimeSeries,
            action: "source.query",
        };
        let shown = ctx.state.config.server.query_inspect.then(|| expanded.text.clone());
        return match run_query(&ctx, source, &info, expanded, effective_limits(&cfg, false, &grant.ceilings)).await {
            // The preview already needs `source.query` and `dashboard.write`: the SQL needs only the key.
            Ok(frame) => Ok(frame_response_with_sql(frame, None, Some(interval.ms), shown.as_deref())),
            Err(f) => failure(f),
        };
    }
    let name = v["variable"].as_str().unwrap_or("");
    let variable = doc
        .document()
        .variable(name)
        .ok_or_else(|| ApiError::bad_request("The variable is not in dashboard_variables."))?;
    let DocVariableKind::Query { source: var_source, sql, .. } = &variable.kind else {
        return Err(ApiError::bad_request("The preview of a variable needs a variable of the kind query."));
    };
    if var_source.as_str() != source_id {
        return Err(ApiError::bad_request("The source of the variable is not the source of the route."));
    }
    let (plain, _) = super::query::adhoc_split(v, doc.document(), &ctx.state)?;
    let values = read_values(&plain, &defs, &range)?;
    let expanded = expand_query(&cfg, sql, &values, range, None)?;
    let info = RunInfo {
        kind: QueryKind::Preview,
        uid: uid.as_deref(),
        folder: folder.as_deref(),
        panel: None,
        query_ref: None,
        variable: Some(name),
        annotation: None,
        source: &source_id,
        time_series: false,
        action: "source.query",
    };
    match run_query(&ctx, source, &info, expanded, effective_limits(&cfg, true, &grant.ceilings)).await {
        Ok(frame) => Ok(options_response(&frame, None)),
        Err(f) => failure(f),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointers_map_to_the_body() {
        assert_eq!(body_pointer("/variables/2/sql/1"), "/dashboard_variables/2/sql/1");
        assert_eq!(body_pointer("/sections/0/panels/0/data/query/sql/3"), "/panel/data/query/sql/3");
        assert_eq!(body_pointer("/sections/0/panels/0"), "/panel");
        assert_eq!(body_pointer("/sections/0/panels/1/title"), "/panel");
        assert_eq!(body_pointer(""), "");
    }
}
