-- Migration 2 of the Hune schema: a dashboard can have no folder.
ALTER TABLE hune_dashboards ALTER COLUMN folder_id DROP NOT NULL;
ALTER TABLE hune_versions ALTER COLUMN folder_id DROP NOT NULL;
UPDATE hune_schema SET version = 2, updated_at = now() WHERE id;
