// SPDX-License-Identifier: MIT OR Apache-2.0
//! `hune migrate-documents` (4.42): the migration of the stored documents from format 1 or
//! format 2 to format 3.
//!
//! The command works in two phases. The plan reads each current document and migrates it in
//! memory. A document of format 1 goes through the chain 1 to 2 to 3 (`migrate_1_to_3`) and a
//! document of format 2 through `migrate_2_to_3`. Both run the validation of format 3. Only when
//! every document passes does `--write` call `Store::save` with the kind `migrate`, one document
//! at a time. A document that needs two steps gets one history version. A dry run stops after
//! the plan. The command never rewrites a history version (4.42, item 10), a folder file or a
//! metadata file: their schema did not change (4.42, item 11).
//!
//! Only a document of format 1 or 2 can fail the command: its migration must pass the validation
//! of format 3 (4.42, item 8). A stored document that is not valid for another reason (a later
//! format, a syntax error, a folder without a valid file) is not the concern of this command. It
//! gets a warning and stays as it is.

use std::io::Write;
use std::path::Path;

use hune_core::document::{CanonicalDocument, FORMAT_VERSION, migrate_1_to_3, migrate_2_to_3};
use hune_core::error::ErrorCode;
use hune_store::{AnyStore, Catalog, CatalogEntry, Etag, FolderId, Message, Store, Subject, VersionKind, WriteMeta};

use super::{EXIT_CONFIG, EXIT_OK, fail, now, open};
use crate::config;

/// The oldest format that the command reads. The newer formats that it reads are before
/// [`FORMAT_VERSION`].
const OLDEST: u32 = 1;

/// One document that the command migrates.
struct Planned {
    place: String,
    /// The format of the stored document: 1 or 2.
    from: u32,
    folder: FolderId,
    /// The etag of the stored bytes (4.39, item 3): the `If-Match` of the save.
    etag: Etag,
    /// The migrated document, validated.
    doc: CanonicalDocument,
}

/// The result of the plan.
struct Plan {
    migrate: Vec<Planned>,
    current: usize,
    /// The errors of the documents of format 1 that do not migrate. They stop the command (4.42, item 8).
    problems: Vec<String>,
    /// The documents that this command does not migrate and that are not valid: another format, a
    /// syntax error, a folder without its file. They stay as they are, as the server shows them.
    skipped: Vec<String>,
    /// The number of documents that are not valid and not migrated.
    skipped_documents: usize,
}

/// Gives the format of an entry that has exactly the error `not loadable: format N`, with N from
/// the oldest format to the format before the current one. A document of an older format has
/// this one error only, because the format check comes first (4.36).
fn old_format(errors: &[hune_core::error::ValidationError]) -> Option<u32> {
    let [e] = errors else { return None };
    if e.code != ErrorCode::FormatVersion {
        return None;
    }
    (OLDEST..FORMAT_VERSION).find(|n| e.message == format!("not loadable: format {n}"))
}

/// Reads every folder and every current document and migrates the old ones in memory.
async fn plan(store: &AnyStore, out: &mut dyn Write) -> Result<Plan, String> {
    let catalog = Catalog::load(store).await.map_err(|e| format!("the load of the store failed: {e}"))?;
    let mut entries = catalog.entries();
    entries.sort_by(|a, b| (a.folder().as_str(), a.uid().as_str()).cmp(&(b.folder().as_str(), b.uid().as_str())));
    let mut plan =
        Plan { migrate: Vec::new(), current: 0, problems: Vec::new(), skipped: Vec::new(), skipped_documents: 0 };
    for entry in &entries {
        let place = format!("{}/{}", entry.folder(), entry.uid());
        match &**entry {
            CatalogEntry::Valid { .. } => {
                plan.current += 1;
                let _ = writeln!(out, "{place}: format {FORMAT_VERSION}, no migration needed");
            }
            CatalogEntry::Invalid { uid, folder, errors, .. } if old_format(errors).is_some() => {
                let from = old_format(errors).unwrap_or(OLDEST);
                let (bytes, etag) = store
                    .raw_current(uid, folder)
                    .await
                    .map_err(|e| format!("{place}: the stored bytes are not readable: {e}"))?;
                let migrated =
                    if from == 1 { migrate_1_to_3(&bytes, Some(uid)) } else { migrate_2_to_3(&bytes, Some(uid)) };
                match migrated {
                    Ok(doc) => {
                        let _ = writeln!(out, "{place}: format {from}, migrates to format {FORMAT_VERSION}");
                        plan.migrate.push(Planned { place, from, folder: folder.clone(), etag, doc });
                    }
                    Err(errors) => {
                        for e in &errors {
                            plan.problems.push(format!("{place}: {e}"));
                        }
                    }
                }
            }
            CatalogEntry::Invalid { errors, .. } => {
                plan.skipped_documents += 1;
                for e in errors {
                    plan.skipped.push(format!("{place}: {e}"));
                }
            }
        }
    }
    for (id, errors) in catalog.invalid_folders() {
        for e in errors {
            plan.skipped.push(format!("folder {id}: {e}"));
        }
    }
    Ok(plan)
}

/// `hune migrate-documents -c <file> [--write]` (4.42, items 5 to 13).
pub(super) async fn run(
    loaded: &config::LoadedConfig,
    config_path: &Path,
    write: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    // A dry run opens a volume read-only, so it takes no lock. `--write` takes the lock (4.42, item 13).
    let store = match open(&loaded.config.storage, !write, config_path).await {
        Ok(s) => s,
        Err(e) => return fail(err, e),
    };
    let plan = match plan(&store, out).await {
        Ok(p) => p,
        Err(e) => return fail(err, e),
    };
    for line in &plan.skipped {
        let _ = writeln!(err, "warning: {line}; this document is not migrated");
    }
    if !plan.problems.is_empty() {
        for line in &plan.problems {
            let _ = writeln!(err, "{line}");
        }
        let _ = writeln!(err, "nothing is written");
        return EXIT_CONFIG;
    }
    let total = plan.migrate.len() + plan.current + plan.skipped_documents;
    let not_migrated = if plan.skipped_documents == 0 {
        String::new()
    } else {
        format!(", {} not valid and not migrated", plan.skipped_documents)
    };
    if !write {
        let _ = writeln!(
            out,
            "{total} documents: {} to migrate, {} at format {FORMAT_VERSION}{not_migrated} (dry run); nothing is written",
            plan.migrate.len(),
            plan.current
        );
        return EXIT_OK;
    }
    for (done, p) in plan.migrate.iter().enumerate() {
        // The message names the stored format and the current format: `format 1 to 3` for a chain.
        let message = Message::parse(&format!("format {} to {FORMAT_VERSION}", p.from)).unwrap_or_default();
        let meta = WriteMeta { saved_by: Subject::anonymous(), saved_at: now(), message, save_id: None };
        match store.save(&p.doc, &p.etag, &p.folder, VersionKind::Migrate, &meta).await {
            Ok(saved) => {
                let _ = writeln!(
                    out,
                    "{}: migrated to format {FORMAT_VERSION}, version {}, etag {}",
                    p.place,
                    saved.version,
                    saved.etag.map_or_else(String::new, |e| e.to_hex())
                );
            }
            Err(e) => {
                let _ = writeln!(err, "{}: {e}", p.place);
                let _ = writeln!(
                    err,
                    "stopped after {done} of {} documents; run the command again after the cause is removed",
                    plan.migrate.len()
                );
                return EXIT_CONFIG;
            }
        }
    }
    let _ = writeln!(
        out,
        "{total} documents: {} migrated, {} already at format {FORMAT_VERSION}{not_migrated}",
        plan.migrate.len(),
        plan.current
    );
    EXIT_OK
}
