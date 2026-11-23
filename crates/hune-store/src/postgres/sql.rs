//! The statements of 5.26. The backend sends only these statements and the transaction
//! control of 5.27. The names P1 to P30 are the names of 5.26.
//!
//! A statement can differ from the text of 5.26 in these ways:
//! - It selects more columns, so that one statement gives what a later check needs.
//! - It joins the head version, so that `Current` comes from one snapshot (P5, P7, P8).
//! - It converts the save id with `$n::text::uuid`, because the client has no UUID type.

/// P1: encoding of the database.
pub(super) const P1: &str = "SHOW server_encoding";
/// P2: the schema table exists.
pub(super) const P2: &str = "SELECT to_regclass('public.hune_schema') IS NOT NULL";
/// P3: schema version.
pub(super) const P3: &str = "SELECT version FROM hune_schema WHERE id";
/// P4: all folders.
pub(super) const P4: &str = r#"SELECT id, title FROM hune_folders ORDER BY id COLLATE "C""#;
/// P5: all current dashboards, with the head version of each.
pub(super) const P5: &str = r#"SELECT d.uid, d.folder_id, d.version, d.etag, d.document, d.updated_at, d.updated_by,
       h.version AS h_version, h.etag AS h_etag
  FROM hune_dashboards d
  LEFT JOIN LATERAL (SELECT v.version, v.etag FROM hune_versions v
                      WHERE v.uid = d.uid ORDER BY v.version DESC LIMIT 1) h ON true
 ORDER BY d.uid COLLATE "C""#;
/// P6: marks of the current dashboards.
pub(super) const P6: &str = r#"SELECT uid, folder_id, etag FROM hune_dashboards ORDER BY uid COLLATE "C""#;
/// P7: changed dashboards, at most 1000 uids in `$1`.
pub(super) const P7: &str = r#"SELECT d.uid, d.folder_id, d.version, d.etag, d.document, d.updated_at, d.updated_by,
       h.version AS h_version, h.etag AS h_etag
  FROM hune_dashboards d
  LEFT JOIN LATERAL (SELECT v.version, v.etag FROM hune_versions v
                      WHERE v.uid = d.uid ORDER BY v.version DESC LIMIT 1) h ON true
 WHERE d.uid = ANY($1::text[]) ORDER BY d.uid COLLATE "C""#;
/// P8: one current dashboard, with its head version.
pub(super) const P8: &str = r#"SELECT d.uid, d.folder_id, d.version, d.etag, d.document, d.updated_at, d.updated_by,
       h.version AS h_version, h.etag AS h_etag
  FROM hune_dashboards d
  LEFT JOIN LATERAL (SELECT v.version, v.etag FROM hune_versions v
                      WHERE v.uid = d.uid ORDER BY v.version DESC LIMIT 1) h ON true
 WHERE d.uid = $1"#;
/// P9: lock of one current dashboard.
pub(super) const P9: &str = "SELECT folder_id, version, etag, document, updated_at, updated_by
  FROM hune_dashboards WHERE uid = $1 FOR UPDATE";
/// P10: version with this save id.
pub(super) const P10: &str = "SELECT version, kind, folder_id, etag FROM hune_versions
 WHERE uid = $1 AND save_id = $2::text::uuid";
/// P11: head version.
pub(super) const P11: &str = "SELECT version, etag, folder_id FROM hune_versions
 WHERE uid = $1 ORDER BY version DESC LIMIT 1";
/// P12: lock of the creates.
pub(super) const P12: &str = "SELECT pg_advisory_xact_lock(8532464671393841154)";
/// P13: counts.
pub(super) const P13: &str = "SELECT (SELECT count(*) FROM hune_dashboards), (SELECT count(*) FROM hune_folders)";
/// P14: the folder exists.
pub(super) const P14: &str = "SELECT 1 FROM hune_folders WHERE id = $1 FOR SHARE";
/// P15: insert a version.
pub(super) const P15: &str = "INSERT INTO hune_versions (uid, version, kind, folder_id, document, etag,
                            saved_at, saved_by, message, save_id)
  VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::text::uuid)";
/// P16: insert a current dashboard.
pub(super) const P16: &str = "INSERT INTO hune_dashboards (uid, folder_id, version, etag, format_version, title,
                              document, updated_at, updated_by)
  VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (uid) DO NOTHING";
/// P17: update a current dashboard.
pub(super) const P17: &str = "UPDATE hune_dashboards SET folder_id = $2, version = $3, etag = $4, format_version = $5,
       title = $6, document = $7, updated_at = $8, updated_by = $9 WHERE uid = $1";
/// P18: remove a current dashboard.
pub(super) const P18: &str = "DELETE FROM hune_dashboards WHERE uid = $1";
/// P19: retention.
pub(super) const P19: &str = "DELETE FROM hune_versions WHERE uid = $1 AND version <= $2";
/// P20: one page of history.
pub(super) const P20: &str = "SELECT version, kind, folder_id, etag, saved_at, saved_by, message, save_id::text,
       octet_length(document)
  FROM hune_versions WHERE uid = $1 AND version < $2 ORDER BY version DESC LIMIT $3";
/// P21: one version.
pub(super) const P21: &str = "SELECT kind, folder_id, document, etag, saved_at, saved_by, message, save_id::text
  FROM hune_versions WHERE uid = $1 AND version = $2";
/// P22: create a folder.
pub(super) const P22: &str = "INSERT INTO hune_folders (id, title, created_at, updated_at) VALUES ($1, $2, $3, $3)
  ON CONFLICT (id) DO NOTHING";
/// P23: rename a folder. The same title leaves `updated_at` as it is.
pub(super) const P23: &str = "UPDATE hune_folders SET title = $2,
       updated_at = CASE WHEN title = $2 THEN updated_at ELSE $3 END WHERE id = $1";
/// P24: delete a folder.
pub(super) const P24: &str = "DELETE FROM hune_folders WHERE id = $1";
/// P25: all uids with a current dashboard or a version.
pub(super) const P25: &str = r#"SELECT uid FROM (SELECT uid FROM hune_versions UNION SELECT uid FROM hune_dashboards) u
 ORDER BY uid COLLATE "C""#;
/// P26: all versions of one uid.
pub(super) const P26: &str =
    "SELECT version, kind, folder_id, document, etag, saved_at, saved_by, message, save_id::text
  FROM hune_versions WHERE uid = $1 ORDER BY version";
/// P27: the store is empty.
pub(super) const P27: &str = "SELECT (SELECT count(*) FROM hune_folders) + (SELECT count(*) FROM hune_dashboards)
     + (SELECT count(*) FROM hune_versions)";
/// P28: lock of the migrations.
pub(super) const P28: &str = "SELECT pg_try_advisory_lock(8532464671393841153)";
/// P29: release of the lock of the migrations.
pub(super) const P29: &str = "SELECT pg_advisory_unlock(8532464671393841153)";
/// P30: rights of the role.
pub(super) const P30: &str = "SELECT r.rolsuper,
       EXISTS (SELECT 1 FROM pg_tables t
                WHERE t.schemaname = 'public'
                  AND t.tablename IN ('hune_schema', 'hune_folders', 'hune_dashboards', 'hune_versions')
                  AND pg_has_role(current_user, t.tableowner, 'MEMBER'))
  FROM pg_roles r WHERE r.rolname = current_user";
/// The folder of the current dashboard of one uid. Used for the `Exists` answer of a `create`.
pub(super) const P_FOLDER_OF: &str = "SELECT folder_id FROM hune_dashboards WHERE uid = $1";
/// The uid has a current dashboard or a version. Used for an empty page of `history`.
pub(super) const P_UID_KNOWN: &str = "SELECT EXISTS (SELECT 1 FROM hune_dashboards WHERE uid = $1)
    OR EXISTS (SELECT 1 FROM hune_versions WHERE uid = $1)";
