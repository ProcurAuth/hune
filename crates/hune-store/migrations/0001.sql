-- Migration 1 of the Hune schema (5.24).
CREATE TABLE hune_schema (
    id         boolean     PRIMARY KEY DEFAULT true CHECK (id),
    version    integer     NOT NULL CHECK (version >= 1),
    updated_at timestamptz NOT NULL
);

CREATE TABLE hune_folders (
    id         text        PRIMARY KEY CHECK (id ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    title      text        NOT NULL CHECK (char_length(title) BETWEEN 1 AND 256),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

CREATE TABLE hune_dashboards (
    uid            text        PRIMARY KEY CHECK (uid ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    folder_id      text        NOT NULL REFERENCES hune_folders (id) ON DELETE RESTRICT,
    version        bigint      NOT NULL CHECK (version >= 1),
    etag           text        NOT NULL CHECK (etag ~ '^[0-9a-f]{64}$'),
    format_version integer     NOT NULL,
    title          text        NOT NULL,
    document       text        NOT NULL CHECK (octet_length(document) <= 4194304),
    updated_at     timestamptz NOT NULL,
    updated_by     text        NOT NULL CHECK (char_length(updated_by) BETWEEN 1 AND 256)
);

CREATE TABLE hune_versions (
    uid        text        NOT NULL CHECK (uid ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    version    bigint      NOT NULL CHECK (version >= 1),
    kind       text        NOT NULL CHECK (kind IN ('create', 'save', 'restore', 'move',
                                                    'delete', 'import', 'external', 'migrate')),
    folder_id  text        NOT NULL CHECK (folder_id ~ '^[a-z0-9][a-z0-9-]{0,39}$'),
    document   text        CHECK (octet_length(document) <= 4194304),
    etag       text        CHECK (etag ~ '^[0-9a-f]{64}$'),
    saved_at   timestamptz NOT NULL,
    saved_by   text        NOT NULL CHECK (char_length(saved_by) BETWEEN 1 AND 256),
    message    text        NOT NULL CHECK (char_length(message) <= 500),
    save_id    uuid,
    PRIMARY KEY (uid, version),
    UNIQUE (uid, save_id),
    CHECK ((kind IN ('delete', 'external')) OR document IS NOT NULL),
    CHECK (kind <> 'delete' OR document IS NULL),
    CHECK ((kind = 'delete') = (etag IS NULL))
);

INSERT INTO hune_schema (id, version, updated_at) VALUES (true, 1, now());

-- The default privileges of 5.25 give the application role INSERT, UPDATE and DELETE on
-- each new table. Only migrate-store writes hune_schema, so remove these rights from each
-- role other than the owner. With the role names of 5.25, this is:
-- REVOKE INSERT, UPDATE, DELETE ON hune_schema FROM hune_app;
DO $$
DECLARE r record;
BEGIN
    FOR r IN SELECT DISTINCT grantee FROM information_schema.role_table_grants
              WHERE table_schema = 'public' AND table_name = 'hune_schema'
                AND privilege_type IN ('INSERT', 'UPDATE', 'DELETE')
                AND grantee <> current_user AND grantee <> 'PUBLIC'
    LOOP
        EXECUTE format('REVOKE INSERT, UPDATE, DELETE ON hune_schema FROM %I', r.grantee);
    END LOOP;
END $$;
