// SPDX-License-Identifier: MIT OR Apache-2.0
//! The folder routes (6.11).

use super::gate;
use super::support::{StoreOp, format_faults, map_store_error, read_folder_id, services};
use crate::server::ctx::{Ctx, Request};
use crate::server::error::ApiError;
use crate::server::json::{Member, Shape, parse_and_check_with};
use crate::server::response::{Response, empty_response, json_response};
use http::StatusCode;
use hune_core::error::{ErrorCode, ValidationError};
use hune_store::{FolderId, FolderTitle, Store};
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct FolderAccess {
    write: bool,
}

#[derive(Serialize)]
struct FolderItem<'a> {
    id: &'a str,
    title: &'a str,
    access: FolderAccess,
    /// True when the store of the folder is read-only (5.42). Then `access.write` is false.
    read_only: bool,
    /// The id of the store of the folder.
    store: String,
}

#[derive(Serialize)]
struct ListAccess {
    create: bool,
}

/// The place of the dashboards without a folder (6.11). It is not a folder: it has no id and no title.
#[derive(Serialize)]
struct RootItem {
    access: FolderAccess,
    /// True when no primary store exists: nothing can be created at the root.
    read_only: bool,
    /// The id of the primary store, or null when no primary store exists.
    store: Option<String>,
}

#[derive(Serialize)]
struct FolderList<'a> {
    access: ListAccess,
    root: RootItem,
    items: Vec<FolderItem<'a>>,
}

#[derive(Serialize)]
struct FolderAnswer<'a> {
    id: &'a str,
    title: &'a str,
}

static CREATE_MEMBERS: [Member; 2] = [
    Member { name: "id", shape: Shape::Str { min_chars: 1, max_chars: 40 }, required: true },
    Member { name: "title", shape: Shape::Str { min_chars: 1, max_chars: 256 }, required: true },
];
static CREATE_SHAPE: Shape = Shape::Object(&CREATE_MEMBERS);
static RENAME_MEMBERS: [Member; 1] =
    [Member { name: "title", shape: Shape::Str { min_chars: 1, max_chars: 256 }, required: true }];
static RENAME_SHAPE: Shape = Shape::Object(&RENAME_MEMBERS);

fn read_title(value: &Value) -> Result<FolderTitle, ApiError> {
    let text = value.get("title").and_then(Value::as_str).unwrap_or("");
    FolderTitle::parse(text).map_err(|e| {
        ApiError::bad_request_errors(
            "The body does not follow the schema.",
            &[ValidationError::new("/title", ErrorCode::BadFormat, e.0)],
        )
    })
}

fn log_folder_changed(ctx: &Ctx, action: &str, folder: &str) {
    ctx.state.logger.folder_changed(Some(&ctx.request_id), action, folder, &ctx.subject);
}

/// `GET /api/folders`: the folders in byte order of the id. Mode `open` shows each folder.
pub async fn list(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let services = services(&ctx)?;
    let folders = services.catalog.folders();
    let mut ids: Vec<String> = folders.iter().map(|f| f.id.as_str().to_owned()).collect();
    // The node of the dashboards without a folder is the last question of the display filter (ACCESS.md).
    ids.push(hune_core::ids::ROOT_FOLDER_TEXT.to_owned());
    let primary = services.store.primary_id().map(str::to_owned);
    let root_item = |write: bool| RootItem {
        access: FolderAccess { write: write && primary.is_some() },
        read_only: primary.is_none(),
        store: primary.clone(),
    };
    let facts = |f: &hune_store::FolderRecord| match services.store.store_of_folder(&f.id) {
        Some(info) => (info.id.clone(), !info.primary),
        None => (services.store.primary_id().unwrap_or_default().to_owned(), false),
    };
    let in_read_only_store =
        |folder: &str| FolderId::parse(folder).is_ok_and(|f| services.store.folder_is_read_only(&f));
    let body = match gate::filter_folders(&ctx, &ids, &in_read_only_store).await? {
        // Mode `authzen`: the display filter keeps some folders and gives the flags (11.25).
        Some(filter) => FolderList {
            access: ListAccess { create: filter.create },
            root: root_item(filter.folders.last().copied().flatten().unwrap_or(false)),
            items: folders
                .iter()
                .zip(&filter.folders)
                .filter_map(|(f, flag)| {
                    flag.map(|write| {
                        let (store, read_only) = facts(f);
                        FolderItem {
                            id: f.id.as_str(),
                            title: f.title.as_str(),
                            access: FolderAccess { write: write && !read_only },
                            read_only,
                            store,
                        }
                    })
                })
                .collect(),
        },
        None => {
            let write = true;
            FolderList {
                access: ListAccess { create: write && primary.is_some() },
                root: root_item(write),
                items: folders
                    .iter()
                    .map(|f| {
                        let (store, read_only) = facts(f);
                        FolderItem {
                            id: f.id.as_str(),
                            title: f.title.as_str(),
                            access: FolderAccess { write: write && !read_only },
                            read_only,
                            store,
                        }
                    })
                    .collect(),
            }
        }
    };
    Ok(json_response(StatusCode::OK, &body))
}

/// `POST /api/folders`: creates a folder. The answer is `201` with `{id, title}`.
pub async fn create(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let body = parse_and_check_with(&req.body, &CREATE_SHAPE, &[], |v| format_faults(v, &["id"], true, false))?;
    let id = read_folder_id(body.value["id"].as_str().unwrap_or(""), "/id")?;
    let title = read_title(&body.value)?;
    gate::service_write(&ctx).await?;
    let services = services(&ctx)?;
    let op = StoreOp::begin(&ctx, "create_folder");
    let result = services.store.create_folder(&id, &title).await;
    op.end(&ctx.state, "create_folder", &result);
    result.map_err(|e| map_store_error(&e))?;
    services.catalog.refresh_folders(&services.store).await;
    log_folder_changed(&ctx, "create", id.as_str());
    Ok(json_response(StatusCode::CREATED, &FolderAnswer { id: id.as_str(), title: title.as_str() }))
}

/// Reads the folder id of the path. A text that is not an id names no folder: `404 not_found`.
fn path_folder(ctx: &Ctx) -> Result<FolderId, ApiError> {
    FolderId::parse(ctx.params.get("id").unwrap_or("")).map_err(|_| ApiError::not_found("No such folder."))
}

/// Reads the folder id of the path and asks the use question (11.21). In the mode `authzen`, an id that
/// the catalog does not hold follows the rule of 11.23. In the mode `open`, it is the answer as before.
async fn path_folder_gated(ctx: &Ctx) -> Result<FolderId, ApiError> {
    let id = match path_folder(ctx) {
        Ok(id) => id,
        Err(_) => return Err(gate::unknown(ctx, "No such folder.").await),
    };
    if ctx.state.access.is_some() {
        let services = services(ctx)?;
        if !services.catalog.folders().iter().any(|f| f.id == id) {
            return Err(gate::unknown(ctx, "No such folder.").await);
        }
        if services.store.folder_is_read_only(&id) {
            return Err(gate::read_only_refusal(ctx, id.as_str(), None).await);
        }
        gate::folder_write_with_sources(ctx, id.as_str(), &[]).await?;
    }
    Ok(id)
}

/// `PUT /api/folders/{id}`: renames a folder. The last rename wins.
pub async fn rename(ctx: Ctx, req: Request) -> Result<Response, ApiError> {
    let body = parse_and_check_with(&req.body, &RENAME_SHAPE, &[], |v| format_faults(v, &[], true, false))?;
    let title = read_title(&body.value)?;
    let id = path_folder_gated(&ctx).await?;
    let services = services(&ctx)?;
    let op = StoreOp::begin(&ctx, "rename_folder");
    let result = services.store.rename_folder(&id, &title).await;
    op.end(&ctx.state, "rename_folder", &result);
    result.map_err(|e| map_store_error(&e))?;
    services.catalog.refresh_folders(&services.store).await;
    log_folder_changed(&ctx, "rename", id.as_str());
    Ok(json_response(StatusCode::OK, &FolderAnswer { id: id.as_str(), title: title.as_str() }))
}

/// `DELETE /api/folders/{id}`: deletes an empty folder. The answer is `204`.
pub async fn delete(ctx: Ctx, _req: Request) -> Result<Response, ApiError> {
    let id = path_folder_gated(&ctx).await?;
    let services = services(&ctx)?;
    let op = StoreOp::begin(&ctx, "delete_folder");
    let result = services.store.delete_folder(&id).await;
    op.end(&ctx.state, "delete_folder", &result);
    result.map_err(|e| map_store_error(&e))?;
    services.catalog.refresh_folders(&services.store).await;
    log_folder_changed(&ctx, "delete", id.as_str());
    Ok(empty_response(StatusCode::NO_CONTENT))
}
