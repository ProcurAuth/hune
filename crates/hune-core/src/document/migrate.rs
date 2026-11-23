//! The migrations of the document format (4.42).
//!
//! Each migration is a pure function. It reads the bytes of a document of an older format,
//! changes the JSON tree by the rules of the format change, and then runs the validation of the
//! current format (4.36) on the result. Thus a document that migrates has passed every check,
//! and the result holds the canonical bytes.
//!
//! - Format 2 adds the panel member `transparent` (4.13). [`migrate_1_to_2`] adds it.
//! - Format 3 adds the top-level member `links` (4.5a). [`migrate_2_to_3`] adds it.
//! - [`migrate_1_to_3`] applies both steps in one call, and gives one result.
//!
//! The errors of the steps make one list with the cap of 4.36, step 7. The list has `MAX_ERRORS` items at
//! most. When the list stops, its last item is `too_many_errors`.

use super::json::{Json, Num};
use super::{CanonicalDocument, FORMAT_VERSION, canonical, parse_object, validate_tree};
use crate::error::{ErrorCode, ValidationError};
use crate::ids::Uid;
use crate::report::SaveReport;

/// Sets the member `format_version` of the top-level object.
fn set_version(tree: &mut Json, version: u32) {
    if let Json::Obj(top) = tree {
        for (key, value) in top.iter_mut() {
            if key == "format_version" {
                *value = Json::Num(Num::integer(i64::from(version)));
            }
        }
    }
}

/// Step 1 to 2 on the tree: adds `transparent` with the value `false` to each panel. A panel
/// that already has the member gives `unknown_member`: format 1 has no such member. The step
/// stops when the list of errors stops (4.36, step 7).
fn step_1_to_2(tree: &mut Json, report: &mut SaveReport) {
    let Json::Obj(top) = tree else { return };
    let Some((_, Json::Arr(sections))) = top.iter_mut().find(|(k, _)| k == "sections") else { return };
    for (si, section) in sections.iter_mut().enumerate() {
        let Json::Obj(members) = section else { continue };
        let Some((_, Json::Arr(panels))) = members.iter_mut().find(|(k, _)| k == "panels") else { continue };
        for (pi, panel) in panels.iter_mut().enumerate() {
            let Json::Obj(items) = panel else { continue };
            if items.iter().any(|(k, _)| k == "transparent") {
                report.error_with(|| {
                    ValidationError::new(
                        format!("/sections/{si}/panels/{pi}/transparent"),
                        ErrorCode::UnknownMember,
                        "member transparent is not permitted in a panel of format 1",
                    )
                });
                if report.errors_stopped() {
                    return;
                }
                continue;
            }
            items.push(("transparent".to_owned(), Json::Bool(false)));
        }
    }
}

/// Step 2 to 3 on the tree: adds `links` with the value `[]` to the top-level object. A document
/// that already has `links` gives `unknown_member`: formats 1 and 2 have no such member.
/// `from` is the format of the input, for the message.
fn step_2_to_3(tree: &mut Json, from: u32, report: &mut SaveReport) {
    let Json::Obj(top) = tree else { return };
    if top.iter().any(|(k, _)| k == "links") {
        report.error_with(|| {
            ValidationError::new(
                "/links",
                ErrorCode::UnknownMember,
                format!("member links is not permitted in a document of format {from}"),
            )
        });
        return;
    }
    top.push(("links".to_owned(), Json::Arr(Vec::new())));
}

/// Runs the steps that `steps` names on the tree of `bytes`, which has the format `from`, and
/// validates the result in format 3.
fn run(bytes: &[u8], expected_uid: Option<&Uid>, from: u32) -> Result<CanonicalDocument, Vec<ValidationError>> {
    let mut tree = parse_object(bytes, from).map_err(|e| vec![e])?;
    let mut report = SaveReport::new();
    if from == 1 {
        step_1_to_2(&mut tree, &mut report);
    }
    step_2_to_3(&mut tree, from, &mut report);
    let (errors, _) = report.into_parts();
    if !errors.is_empty() {
        return Err(errors);
    }
    set_version(&mut tree, FORMAT_VERSION);
    validate_tree(tree, expected_uid)
}

/// Migrates the bytes of a document of format 2 to a document of format 3 (4.42, steps 1 to 5).
///
/// The input is any valid JSON layout of a format 2 document. `expected_uid` is the location of
/// the document (4.5): `None` skips that check. The result is the canonical document of format
/// 3, or the errors. A document that is not valid in format 2 gives the errors of format 3 at the
/// same pointers. The rules of format 3 only add members and values to the rules of format 2.
/// The migration does not check for these new optional members. A document that is not format 2
/// gives the error `format_version`. A document that has `links` gives `unknown_member`.
pub fn migrate_2_to_3(bytes: &[u8], expected_uid: Option<&Uid>) -> Result<CanonicalDocument, Vec<ValidationError>> {
    run(bytes, expected_uid, 2)
}

/// Migrates the bytes of a document of format 1 to a document of format 3 in one call (4.42,
/// items 8 and 9). It applies the first change (`transparent`) and then the second (`links`).
///
/// The result is the canonical document of format 3. It has the same bytes as
/// [`migrate_2_to_3`] gives for the output of [`migrate_1_to_2`]. A document that is not valid
/// gives the errors of format 3 at the pointers of the input. A document that is not format 1
/// gives the error `format_version`.
pub fn migrate_1_to_3(bytes: &[u8], expected_uid: Option<&Uid>) -> Result<CanonicalDocument, Vec<ValidationError>> {
    run(bytes, expected_uid, 1)
}

/// Migrates the bytes of a document of format 1 to the canonical bytes of format 2 (4.42).
///
/// The result is the first step of the chain. It holds no member of format 3, so it is not a
/// [`CanonicalDocument`] and no store accepts it. The migration also runs the validation, with
/// the rules of format 3 that include the rules of format 2. A document of format 1 that has
/// `links` gives `unknown_member`, as in [`migrate_1_to_3`].
pub fn migrate_1_to_2(bytes: &[u8], expected_uid: Option<&Uid>) -> Result<Vec<u8>, Vec<ValidationError>> {
    let document = migrate_1_to_3(bytes, expected_uid)?;
    let out = canonical::to_format_2(canonical::document_out(document.document()));
    Ok(canonical::write(&out))
}
