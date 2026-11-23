-- Fixture of the probe test (8a.12, 16.9a): synthetic roles and databases.
-- Run as the administrator of a disposable server: psql -v pw=<password> -f roles.sql
-- Each role probe_* has the password `pw`. The names tell the check that must fail.
\set ON_ERROR_STOP on

CREATE ROLE grp_createdb NOLOGIN CREATEDB;
CREATE ROLE probe_readers NOLOGIN;
-- Plain groups for the memberships of finding M3. Each one holds one right that the probe refuses.
CREATE ROLE grp_writer NOLOGIN;
CREATE ROLE grp_creator NOLOGIN;
CREATE ROLE grp_seq NOLOGIN;
CREATE ROLE grp_col NOLOGIN;
CREATE ROLE grp_exec NOLOGIN;
CREATE ROLE grp_fdw NOLOGIN;
CREATE ROLE grp_outer NOLOGIN;
CREATE ROLE grp_owner NOLOGIN;

SELECT format('CREATE ROLE %I LOGIN PASSWORD %L %s', r, :'pw', a)
FROM (VALUES
    ('probe_ok', ''),
    ('probe_hard', ''),
    ('probe_super', 'SUPERUSER'),
    ('probe_createrole', 'CREATEROLE'),
    ('probe_createdb', 'CREATEDB'),
    ('probe_replication', 'REPLICATION'),
    ('probe_bypassrls', 'BYPASSRLS'),
    ('probe_member_pg_read_all_data', ''),
    ('probe_member_pg_execute', ''),
    ('probe_member_noinherit', 'NOINHERIT'),
    ('probe_member_attr', ''),
    ('probe_schema_create', ''),
    ('probe_db_create', ''),
    ('probe_t_insert', ''),
    ('probe_t_update', ''),
    ('probe_t_delete', ''),
    ('probe_t_truncate', ''),
    ('probe_t_references', ''),
    ('probe_t_trigger', ''),
    ('probe_view_update', ''),
    ('probe_seq_update', ''),
    ('probe_col_insert', ''),
    ('probe_col_update', ''),
    ('probe_col_references', ''),
    ('probe_readfile', ''),
    ('probe_fdw', ''),
    ('probe_srv', ''),
    -- H1: one explicit grant on each function that runs text or signals.
    ('probe_exec_term', ''),
    ('probe_exec_cancel', ''),
    ('probe_exec_emit', ''),
    ('probe_exec_tsstat', ''),
    ('probe_exec_tsrewrite', ''),
    -- M3: a membership with NOINHERIT (the member can run SET ROLE), one role for each check R3 to R8.
    ('probe_noinh_r3', 'NOINHERIT'),
    ('probe_noinh_r4', 'NOINHERIT'),
    ('probe_noinh_r5', 'NOINHERIT'),
    ('probe_noinh_r6', 'NOINHERIT'),
    ('probe_noinh_r7', 'NOINHERIT'),
    ('probe_noinh_r8', 'NOINHERIT'),
    -- M3: a plain inherited membership, and an indirect one.
    ('probe_inherit_r4', ''),
    ('probe_indirect_r4', 'NOINHERIT'),
    -- M3: an owner (R9): of a table, of a table through a group, of a schema, a function, a type and a large object.
    ('probe_owner_table', ''),
    ('probe_owner_member', 'NOINHERIT'),
    ('probe_owner_schema', ''),
    ('probe_owner_function', ''),
    ('probe_owner_type', ''),
    ('probe_owner_lo', '')
) AS v(r, a)
\gexec

-- A harmless group: a member of it passes R2. Each login role reads through it.
SELECT format('GRANT probe_readers TO %I', rolname) FROM pg_roles WHERE rolname LIKE 'probe\_%' AND rolcanlogin \gexec

GRANT pg_read_all_data TO probe_member_pg_read_all_data;
GRANT pg_execute_server_program TO probe_member_pg_execute;
GRANT pg_monitor TO probe_member_noinherit;
GRANT grp_createdb TO probe_member_attr;

GRANT grp_writer TO probe_noinh_r4, probe_inherit_r4;
GRANT grp_writer TO grp_outer;
GRANT grp_outer TO probe_indirect_r4;
GRANT grp_creator TO probe_noinh_r3;
GRANT grp_seq TO probe_noinh_r5;
GRANT grp_col TO probe_noinh_r6;
GRANT grp_exec TO probe_noinh_r7;
GRANT grp_fdw TO probe_noinh_r8;
GRANT grp_owner TO probe_owner_member;

CREATE DATABASE probe_db;
CREATE DATABASE probe_hard;
CREATE DATABASE probe_dblink;
CREATE DATABASE probe_fdw;
-- probe_public: the default install. PUBLIC still runs the functions that the probe refuses (R7, finding H1).
CREATE DATABASE probe_public;

-- probe_db: the default install, plus one right for each failing role.
\connect probe_db
\ir objects.sql
\ir revoke.sql
GRANT INSERT ON metrics.t TO probe_t_insert;
GRANT UPDATE ON metrics.t TO probe_t_update;
GRANT DELETE ON metrics.t TO probe_t_delete;
GRANT TRUNCATE ON metrics.t TO probe_t_truncate;
GRANT REFERENCES ON metrics.t TO probe_t_references;
GRANT TRIGGER ON metrics.t TO probe_t_trigger;
GRANT UPDATE ON metrics.v TO probe_view_update;
GRANT UPDATE ON SEQUENCE metrics.seq TO probe_seq_update;
GRANT INSERT (v) ON metrics.t TO probe_col_insert;
GRANT UPDATE (v) ON metrics.t TO probe_col_update;
GRANT REFERENCES (id) ON metrics.t TO probe_col_references;
GRANT CREATE ON SCHEMA metrics TO probe_schema_create;
GRANT CREATE ON DATABASE probe_db TO probe_db_create;
GRANT EXECUTE ON FUNCTION pg_read_file(text) TO probe_readfile;

-- H1: one explicit grant for each function of R7 that PUBLIC ran before the revoke.
SELECT format('GRANT EXECUTE ON FUNCTION %s TO %I', p.oid::regprocedure, g.role)
FROM pg_proc p
JOIN (VALUES ('pg_terminate_backend', 'probe_exec_term'), ('pg_cancel_backend', 'probe_exec_cancel'),
             ('pg_logical_emit_message', 'probe_exec_emit'), ('ts_stat', 'probe_exec_tsstat'),
             ('ts_rewrite', 'probe_exec_tsrewrite')) AS g(fn, role) ON g.fn = p.proname
WHERE p.pronamespace = 'pg_catalog'::regnamespace
\gexec

-- M3: the right of each group. The members use it through SET ROLE (NOINHERIT), through the inheritance
-- (probe_inherit_r4) or through a group of a group (probe_indirect_r4).
GRANT INSERT ON metrics.t TO grp_writer;
GRANT CREATE ON SCHEMA metrics TO grp_creator;
GRANT UPDATE ON SEQUENCE metrics.seq TO grp_seq;
GRANT INSERT (v) ON metrics.t TO grp_col;
GRANT EXECUTE ON FUNCTION pg_read_file(text) TO grp_exec;

-- M3, R9: each object has no right for its owner except SELECT or USAGE, so R3 to R8 see nothing and only
-- the check of the owner refuses the role.
CREATE TABLE metrics.owned (id int);
ALTER TABLE metrics.owned OWNER TO probe_owner_table;
SET ROLE probe_owner_table;
REVOKE ALL ON metrics.owned FROM probe_owner_table;
GRANT SELECT ON metrics.owned TO probe_owner_table;
RESET ROLE;
CREATE TABLE metrics.owned_group (id int);
ALTER TABLE metrics.owned_group OWNER TO grp_owner;
REVOKE ALL ON metrics.owned_group FROM grp_owner;
GRANT SELECT ON metrics.owned_group TO grp_owner;
CREATE SCHEMA own_schema AUTHORIZATION probe_owner_schema;
SET ROLE probe_owner_schema;
REVOKE ALL ON SCHEMA own_schema FROM probe_owner_schema;
RESET ROLE;
CREATE FUNCTION metrics.own_fn() RETURNS int LANGUAGE sql AS 'SELECT 1';
ALTER FUNCTION metrics.own_fn() OWNER TO probe_owner_function;
CREATE TYPE metrics.own_type AS ENUM ('a');
ALTER TYPE metrics.own_type OWNER TO probe_owner_type;
SET ROLE probe_owner_lo;
SELECT lo_create(424242);
RESET ROLE;

-- probe_hard: the closure of the advice A1 to A4 (8a.13).
\connect probe_hard
\ir objects.sql
\ir revoke.sql
REVOKE TEMPORARY ON DATABASE probe_hard FROM PUBLIC;
REVOKE USAGE ON LANGUAGE plpgsql FROM PUBLIC;
SELECT format('REVOKE EXECUTE ON FUNCTION %s FROM PUBLIC', p.oid::regprocedure)
FROM pg_proc p
WHERE p.pronamespace = 'pg_catalog'::regnamespace
  AND (p.proname IN ('pg_sleep', 'pg_sleep_for', 'pg_sleep_until', 'pg_cancel_backend',
                     'pg_terminate_backend', 'pg_notify', 'pg_logical_emit_message', 'ts_stat', 'ts_rewrite')
       OR p.proname LIKE 'pg\_advisory\_%' OR p.proname LIKE 'pg\_try\_advisory\_%'
       OR p.proname LIKE 'query\_to\_xml%' OR p.proname LIKE 'cursor\_to\_xml%')
\gexec
ALTER ROLE probe_hard SET temp_file_limit = '1GB';

-- probe_dblink: the extension makes its functions callable by PUBLIC (R7).
\connect probe_dblink
\ir objects.sql
\ir revoke.sql
CREATE EXTENSION dblink;

-- probe_fdw: a wrapper and a server, with USAGE for two roles (R8).
\connect probe_fdw
\ir objects.sql
\ir revoke.sql
CREATE EXTENSION postgres_fdw;
CREATE SERVER probe_remote FOREIGN DATA WRAPPER postgres_fdw OPTIONS (host '127.0.0.1', dbname 'none');
GRANT USAGE ON FOREIGN DATA WRAPPER postgres_fdw TO probe_fdw;
GRANT USAGE ON FOREIGN SERVER probe_remote TO probe_srv;
GRANT USAGE ON FOREIGN SERVER probe_remote TO grp_fdw;

-- probe_public: objects.sql only. No revoke: PUBLIC runs pg_terminate_backend and the other four functions.
\connect probe_public
\ir objects.sql
