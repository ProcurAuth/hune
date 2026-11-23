-- Synthetic setup of the test PostgreSQL of the `pgsource` job (16.9a).
-- All data is synthetic. No name here is a real name.
--
-- Use. Run this file with psql as a superuser on a new cluster. The script `apply.sh` does it at the first
-- start of the container. The file holds no password: each password is a psql variable.
--   psql -v ON_ERROR_STOP=1 -v ro_password=... -v rw_password=... -v bad_password=... -v legacy_password=... -f init.sql
-- The file is not idempotent: run it one time on an empty cluster.
--
-- Roles (the names are a contract with the tests):
--   metrics_owner      owner of the database, the schemas and the tables. No login.
--   metrics_writer     a plain role with INSERT on metrics.write_target. No login. Only hune_bad_noinh is a member.
--   hune_ro           the role of 8a.13, as written there. It is the role of the source.
--   hune_bad_*        deviant roles. Each one has the rights of hune_ro and one more thing that breaks one
--                      check of the probe (8a.12):
--                        hune_bad_createrole  R1  the attribute CREATEROLE
--                        hune_bad_pgread      R2  member of pg_read_all_data
--                        hune_bad_create      R3  CREATE on the schema metrics
--                        hune_bad_write       R4  INSERT on the table metrics.write_target
--                        hune_bad_seq         R5  UPDATE on the sequence metrics.audit_seq
--                        hune_bad_col         R6  INSERT on one column of metrics.col_target
--                        hune_bad_dblink      R7  EXECUTE on the functions of dblink
--                        hune_bad_fdw         R8  USAGE on the foreign server hune_fx
--                        hune_bad_signal      R7  EXECUTE on pg_terminate_backend and pg_cancel_backend (H1)
--                        hune_bad_emit        R7  EXECUTE on pg_logical_emit_message (H1)
--                        hune_bad_ts          R7  EXECUTE on ts_stat and ts_rewrite (H1)
--                        hune_bad_noinh       R4  a NOINHERIT member of the plain role metrics_writer (M3)
--                        hune_bad_owner       R9  owner of the table metrics.owned, with its own rights
--                                                  revoked except SELECT (M3)
--                      PS11 needs the six roles R1, R2, R3, R4, R7 and R8. The others are extra.
--   hune_rw           all rights on the schema metrics, for PS07. No superuser.
--   hune_md5          the contract of hune_ro, but its password is an MD5 password and pg_hba.conf asks
--                      the method md5 (PS12). apply.sh writes the pg_hba.conf lines.
--   hune_clear        the same, with the method `password` (clear text on the wire) (PS12).
--
-- Objects of the database `analytics`, schema `metrics`:
--   node_load          3 hosts, one row each minute, 720 minutes from 2026-01-01T00:00:00Z (PS01, PS02).
--   node_load_v        a view of node_load.
--   types_all          one column of each type of 7.16a, 4 rows (PS06). types_unsupported: other types.
--   big_rows           1,000,000 rows (PS04). big_value: one row with a text of 50,000,000 bytes (PS04).
--   write_target, col_target, audit_seq   objects for the write cases (PS07, PS08).
-- The schema `restricted` has one table that no test role can read. The schema `ext` holds dblink.

\set ON_ERROR_STOP on

-- The four passwords are mandatory.
\if :{?ro_password}
\else
    DO $$ BEGIN RAISE EXCEPTION 'psql variable ro_password is not set'; END $$;
\endif
\if :{?rw_password}
\else
    DO $$ BEGIN RAISE EXCEPTION 'psql variable rw_password is not set'; END $$;
\endif
\if :{?bad_password}
\else
    DO $$ BEGIN RAISE EXCEPTION 'psql variable bad_password is not set'; END $$;
\endif
\if :{?legacy_password}
\else
    DO $$ BEGIN RAISE EXCEPTION 'psql variable legacy_password is not set'; END $$;
\endif

-- ---------------------------------------------------------------------------------------------------
-- Roles of the cluster
-- ---------------------------------------------------------------------------------------------------

CREATE ROLE metrics_owner NOLOGIN;

-- The role of the source: the SQL of 8a.13, with the password from the variable.
CREATE ROLE hune_ro LOGIN
    PASSWORD :'ro_password'
    NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS
    CONNECTION LIMIT 18;

ALTER ROLE hune_ro SET default_transaction_read_only = on;
ALTER ROLE hune_ro SET statement_timeout = '60s';
ALTER ROLE hune_ro SET idle_in_transaction_session_timeout = '15s';
ALTER ROLE hune_ro SET temp_file_limit = '1GB';
ALTER ROLE hune_ro SET jit = off;

-- The deviant roles. The attribute of R1 is set at the creation. The other changes come with the grants below.
CREATE ROLE hune_bad_createrole LOGIN PASSWORD :'bad_password'
    NOSUPERUSER NOCREATEDB CREATEROLE NOREPLICATION NOBYPASSRLS CONNECTION LIMIT 18;
SELECT format(
    'CREATE ROLE %I LOGIN PASSWORD %L NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS CONNECTION LIMIT 18',
    r, :'bad_password')
FROM unnest(ARRAY['hune_bad_pgread', 'hune_bad_create', 'hune_bad_write', 'hune_bad_seq', 'hune_bad_col',
                  'hune_bad_dblink', 'hune_bad_fdw', 'hune_bad_signal', 'hune_bad_emit', 'hune_bad_ts',
                  'hune_bad_owner']) AS r
\gexec

-- M3: a role with the attribute NOINHERIT does not use the rights of its groups, but it can run SET ROLE to
-- them. The group is a plain role: no attribute, no name pg_*, so R2 does not refuse the membership.
CREATE ROLE metrics_writer NOLOGIN;
CREATE ROLE hune_bad_noinh LOGIN PASSWORD :'bad_password'
    NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS NOINHERIT CONNECTION LIMIT 18;
GRANT metrics_writer TO hune_bad_noinh;
ALTER ROLE hune_bad_noinh SET default_transaction_read_only = on;
ALTER ROLE hune_bad_noinh SET statement_timeout = '60s';

-- The role with all rights.
CREATE ROLE hune_rw LOGIN PASSWORD :'rw_password'
    NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS;

-- The roles with an old method of login. The MD5 hash needs password_encryption = 'md5' for one statement.
SET password_encryption = 'md5';
CREATE ROLE hune_md5 LOGIN PASSWORD :'legacy_password'
    NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS CONNECTION LIMIT 18;
RESET password_encryption;
CREATE ROLE hune_clear LOGIN PASSWORD :'legacy_password'
    NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS CONNECTION LIMIT 18;

-- Each role with the contract of hune_ro has the same session defaults as hune_ro.
SELECT format('ALTER ROLE %I SET default_transaction_read_only = on', r) FROM unnest(ARRAY[
    'hune_bad_createrole', 'hune_bad_pgread', 'hune_bad_create', 'hune_bad_write', 'hune_bad_seq',
    'hune_bad_col', 'hune_bad_dblink', 'hune_bad_fdw', 'hune_bad_signal', 'hune_bad_emit', 'hune_bad_ts',
    'hune_bad_owner', 'hune_md5', 'hune_clear']) AS r
\gexec
SELECT format('ALTER ROLE %I SET statement_timeout = ''60s''', r) FROM unnest(ARRAY[
    'hune_bad_createrole', 'hune_bad_pgread', 'hune_bad_create', 'hune_bad_write', 'hune_bad_seq',
    'hune_bad_col', 'hune_bad_dblink', 'hune_bad_fdw', 'hune_bad_signal', 'hune_bad_emit', 'hune_bad_ts',
    'hune_bad_owner', 'hune_md5', 'hune_clear']) AS r
\gexec

GRANT pg_read_all_data TO hune_bad_pgread;

-- ---------------------------------------------------------------------------------------------------
-- The database
-- ---------------------------------------------------------------------------------------------------

SELECT 'CREATE DATABASE analytics OWNER metrics_owner'
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'analytics')
\gexec

\connect analytics

-- The language of A1: the last line of 8a.13.
REVOKE USAGE ON LANGUAGE plpgsql FROM PUBLIC;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;

-- The five functions that PUBLIC runs by default and that the probe refuses (R7, finding H1): the statement of
-- 8a.13, limited to these names. The probe refuses a role that can run pg_logical_emit_message,
-- pg_terminate_backend, pg_cancel_backend, ts_stat and ts_rewrite, so hune_ro needs this revoke. The deviant
-- roles get their right back below. The fixture does NOT revoke pg_sleep and the other functions of the advice
-- A3: the tests of the deadline and of the cancel request use pg_sleep.
SELECT format('REVOKE EXECUTE ON FUNCTION %s FROM PUBLIC', p.oid::regprocedure)
FROM pg_proc p
WHERE p.pronamespace = 'pg_catalog'::regnamespace
  AND p.proname IN ('pg_logical_emit_message', 'pg_terminate_backend', 'pg_cancel_backend', 'ts_stat', 'ts_rewrite')
\gexec

CREATE SCHEMA metrics AUTHORIZATION metrics_owner;
CREATE SCHEMA restricted AUTHORIZATION metrics_owner;

-- dblink lives in its own schema. Its functions go to PUBLIC by default: take that right away, so that
-- only hune_bad_dblink can call them (R7).
CREATE SCHEMA ext;
CREATE EXTENSION dblink SCHEMA ext;
SELECT format('REVOKE ALL ON FUNCTION %s FROM PUBLIC', p.oid::regprocedure)
FROM pg_proc p WHERE p.pronamespace = 'ext'::regnamespace AND p.proname LIKE 'dblink%'
\gexec
GRANT USAGE ON SCHEMA ext TO hune_bad_dblink;
SELECT format('GRANT EXECUTE ON FUNCTION %s TO hune_bad_dblink', p.oid::regprocedure)
FROM pg_proc p WHERE p.pronamespace = 'ext'::regnamespace AND p.proname LIKE 'dblink%'
\gexec

-- The foreign server of R8. No connection is made to it.
CREATE SERVER hune_fx FOREIGN DATA WRAPPER dblink_fdw OPTIONS (host '127.0.0.1', dbname 'analytics');
GRANT USAGE ON FOREIGN SERVER hune_fx TO hune_bad_fdw;

-- ---------------------------------------------------------------------------------------------------
-- The objects of the owner
-- ---------------------------------------------------------------------------------------------------

SET ROLE metrics_owner;
SET search_path = metrics, pg_catalog;

CREATE TYPE level AS ENUM ('debug', 'info', 'warn', 'error');
CREATE DOMAIN positive_int AS int4 CHECK (VALUE > 0);
CREATE DOMAIN short_code AS text CHECK (length(VALUE) <= 8);
CREATE TYPE pair AS (a int4, b text);

-- PS01, PS02: the seed table. The times are known: 2026-01-01T00:00:00Z plus i minutes.
CREATE TABLE node_load (
    created_at    timestamptz NOT NULL,
    created_plain timestamp   NOT NULL,
    epoch_s       bigint      NOT NULL,
    epoch_ns      bigint      NOT NULL,
    host          text        NOT NULL,
    env           text        NOT NULL,
    load          float8      NOT NULL
);
INSERT INTO node_load
SELECT t, t AT TIME ZONE 'UTC', extract(epoch FROM t)::bigint, extract(epoch FROM t)::bigint * 1000000000,
       h.host, h.env, (i % 60) + h.base
FROM generate_series(0, 719) AS i
CROSS JOIN LATERAL (SELECT timestamptz '2026-01-01 00:00:00+00' + i * interval '1 minute' AS t) AS tt
CROSS JOIN (VALUES ('node-a', 'prod', 0.0), ('node-b', 'prod', 100.0), ('node-c', 'stage', 200.0)) AS h(host, env, base);
CREATE INDEX node_load_created_at ON node_load (created_at);
CREATE VIEW node_load_v AS SELECT created_at, host, env, load FROM node_load;

-- PS06: one column of each supported type. The rows:
--   1 typical values; 2 maximum values and NaN; 3 minimum values and the infinities; 4 a NULL row
--   with +Infinity in the float columns.
CREATE TABLE types_all (
    id           int4 PRIMARY KEY,
    c_bool       bool,
    c_int2       int2,
    c_int4       int4,
    c_oid        oid,
    c_int8       int8,
    c_float4     float4,
    c_float8     float8,
    c_numeric    numeric,
    c_numeric_38 numeric(38, 9),
    c_text       text,
    c_varchar    varchar(20),
    c_bpchar     char(8),
    c_name       name,
    c_uuid       uuid,
    c_json       json,
    c_jsonb      jsonb,
    c_inet       inet,
    c_inet_net   inet,
    c_cidr       cidr,
    c_enum       level,
    c_domain     positive_int,
    c_domain_txt short_code,
    c_date       date,
    c_timestamptz timestamptz,
    c_timestamp  timestamp,
    c_time       time,
    a_bool       bool[],
    a_int4       int4[],
    a_int8       int8[],
    a_float8     float8[],
    a_numeric    numeric[],
    a_text       text[],
    a_date       date[],
    a_timestamptz timestamptz[],
    a_timestamp  timestamp[],
    a_enum       level[],
    a_int4_2d    int4[]
);
INSERT INTO types_all VALUES (
    1, true, 12, 123456, 16384, 4503599627370496, 0.1, 0.1, 12.50, 12.500000000,
    'plain text', 'varchar', 'ab', 'a_name', '123e4567-e89b-42d3-a456-426614174000',
    '{"a" :  1, "b": [true,null]}', '{"b": [true, null], "a": 1}', '192.168.1.5', '192.168.1.5/24', '10.1.0.0/16',
    'warn', 7, 'code', '2026-11-09', '2026-11-09 08:00:00.125+00', '2026-11-09 08:00:00.123456',
    '08:30:15.5',
    ARRAY[true, NULL, false], ARRAY[1, NULL, 3], ARRAY[1, 2]::int8[], ARRAY[1.5, 'NaN', 'Infinity']::float8[],
    ARRAY[1.10, NULL]::numeric[], ARRAY['x', NULL, 'y z'], ARRAY['2026-11-09'::date, NULL],
    ARRAY['2026-11-09 08:00:00.125+00'::timestamptz, NULL], ARRAY['2026-11-09 08:00:00'::timestamp],
    ARRAY['info', NULL]::level[], ARRAY[[1, 2], [3, 4]]
);
INSERT INTO types_all (id, c_int2, c_int4, c_int8, c_float4, c_float8, c_numeric, c_numeric_38, c_date, c_timestamptz,
                       c_timestamp)
VALUES (2, 32767, 2147483647, 9223372036854775807, 'NaN', 'NaN', 'NaN', 99999999999999999999999999999.999999999,
        'infinity', 'infinity', 'infinity');
INSERT INTO types_all (id, c_int2, c_int4, c_int8, c_float4, c_float8, c_numeric, c_numeric_38, c_date, c_timestamptz,
                       c_timestamp)
VALUES (3, -32768, -2147483648, -9223372036854775808, '-Infinity', '-Infinity', '-Infinity',
        -99999999999999999999999999999.999999999, '-infinity', '-infinity', '-infinity');
INSERT INTO types_all (id, c_float4, c_float8) VALUES (4, 'Infinity', 'Infinity');

-- PS06: a column of each type that 7.16a does not list. The row is not NULL.
CREATE TABLE types_unsupported (
    id         int4 PRIMARY KEY,
    c_bytea    bytea,
    c_interval interval,
    c_money    money,
    c_xml      xml,
    c_int4range int4range,
    c_tsrange  tsrange,
    c_point    point,
    c_composite pair,
    c_macaddr  macaddr,
    c_bit      bit(4),
    c_tsvector tsvector,
    a_jsonb    jsonb[]
);
INSERT INTO types_unsupported VALUES (
    1, '\xdeadbeef', '1 day 02:03:04', 12.34, '<a>1</a>', '[1,10)', '[2026-01-01,2026-01-02)', '(1,2)', ROW(1, 'x'),
    '08:00:2b:01:02:03', B'1010', 'a fat cat'::tsvector, ARRAY['{"k": 1}']::jsonb[]
);

-- PS04: a table of 1,000,000 rows.
CREATE TABLE big_rows (
    id int4 PRIMARY KEY,
    ts timestamptz NOT NULL,
    v  float8 NOT NULL
);
INSERT INTO big_rows
SELECT i, timestamptz '2026-01-01 00:00:00+00' + i * interval '1 second', (i % 1000)::float8
FROM generate_series(1, 1000000) AS i;

-- PS04: one row with a value of 50,000,000 bytes. The text is md5 hex, so it does not compress to nothing.
CREATE TABLE big_value (
    id int4 PRIMARY KEY,
    v  text NOT NULL
);
ALTER TABLE big_value ALTER COLUMN v SET STORAGE EXTERNAL;
INSERT INTO big_value VALUES (1, repeat(md5('hune-fixture'), 1562500));

-- PS07, PS08: objects for the write cases.
CREATE TABLE write_target (
    id   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    note text NOT NULL DEFAULT ''
);
CREATE TABLE col_target (
    id   int4 PRIMARY KEY,
    note text,
    flag bool
);
INSERT INTO col_target VALUES (1, 'row', false);
CREATE SEQUENCE audit_seq;

-- A table that hune_ro can not read: no grant, and no USAGE on its schema.
CREATE TABLE restricted.hidden (id int4 PRIMARY KEY, note text);
INSERT INTO restricted.hidden VALUES (1, 'synthetic');

-- The marker. The healthcheck of the compose project waits for this table: it is the last object.
CREATE TABLE fixture_marker (version int4 NOT NULL);
INSERT INTO fixture_marker VALUES (1);

RESET ROLE;
RESET search_path;
ANALYZE metrics.node_load;
ANALYZE metrics.big_rows;

-- ---------------------------------------------------------------------------------------------------
-- The contract of 8a.13, as written there
-- ---------------------------------------------------------------------------------------------------

GRANT CONNECT ON DATABASE analytics TO hune_ro;
GRANT USAGE ON SCHEMA metrics TO hune_ro;
GRANT SELECT ON ALL TABLES IN SCHEMA metrics TO hune_ro;
ALTER DEFAULT PRIVILEGES FOR ROLE metrics_owner IN SCHEMA metrics GRANT SELECT ON TABLES TO hune_ro;

-- The same grants for the roles that must differ from hune_ro in one point only.
SELECT format('GRANT CONNECT ON DATABASE analytics TO %I', r) FROM unnest(ARRAY[
    'hune_bad_createrole', 'hune_bad_pgread', 'hune_bad_create', 'hune_bad_write', 'hune_bad_seq',
    'hune_bad_col', 'hune_bad_dblink', 'hune_bad_fdw', 'hune_bad_signal', 'hune_bad_emit', 'hune_bad_ts',
    'hune_bad_owner', 'hune_bad_noinh', 'hune_md5', 'hune_clear']) AS r
\gexec
SELECT format('GRANT USAGE ON SCHEMA metrics TO %I', r) FROM unnest(ARRAY[
    'hune_bad_createrole', 'hune_bad_pgread', 'hune_bad_create', 'hune_bad_write', 'hune_bad_seq',
    'hune_bad_col', 'hune_bad_dblink', 'hune_bad_fdw', 'hune_bad_signal', 'hune_bad_emit', 'hune_bad_ts',
    'hune_bad_owner', 'hune_bad_noinh', 'hune_md5', 'hune_clear']) AS r
\gexec
SELECT format('GRANT SELECT ON ALL TABLES IN SCHEMA metrics TO %I', r) FROM unnest(ARRAY[
    'hune_bad_createrole', 'hune_bad_pgread', 'hune_bad_create', 'hune_bad_write', 'hune_bad_seq',
    'hune_bad_col', 'hune_bad_dblink', 'hune_bad_fdw', 'hune_bad_signal', 'hune_bad_emit', 'hune_bad_ts',
    'hune_bad_owner', 'hune_bad_noinh', 'hune_md5', 'hune_clear']) AS r
\gexec

-- The one extra right of each deviant role.
GRANT CREATE ON SCHEMA metrics TO hune_bad_create;                       -- R3
GRANT INSERT ON metrics.write_target TO hune_bad_write;                  -- R4
GRANT UPDATE ON SEQUENCE metrics.audit_seq TO hune_bad_seq;              -- R5
GRANT INSERT (note) ON metrics.col_target TO hune_bad_col;               -- R6
-- R1 (CREATEROLE), R2 (pg_read_all_data), R7 (dblink) and R8 (foreign server) are set above.

-- R7, finding H1: one explicit grant for each group of functions that the probe refuses.
SELECT format('GRANT EXECUTE ON FUNCTION %s TO hune_bad_signal', p.oid::regprocedure)
FROM pg_proc p WHERE p.pronamespace = 'pg_catalog'::regnamespace
  AND p.proname IN ('pg_terminate_backend', 'pg_cancel_backend')
\gexec
SELECT format('GRANT EXECUTE ON FUNCTION %s TO hune_bad_emit', p.oid::regprocedure)
FROM pg_proc p WHERE p.pronamespace = 'pg_catalog'::regnamespace AND p.proname = 'pg_logical_emit_message'
\gexec
SELECT format('GRANT EXECUTE ON FUNCTION %s TO hune_bad_ts', p.oid::regprocedure)
FROM pg_proc p WHERE p.pronamespace = 'pg_catalog'::regnamespace AND p.proname IN ('ts_stat', 'ts_rewrite')
\gexec

-- R4 by a membership with NOINHERIT, finding M3: the right is the right of the group.
GRANT INSERT ON metrics.write_target TO metrics_writer;

-- R9, finding M3: an owner can grant itself each right again, so the probe refuses it at once. The table has
-- no right for its owner except SELECT: R4, R5 and R6 see nothing.
CREATE TABLE metrics.owned (id int4);
ALTER TABLE metrics.owned OWNER TO hune_bad_owner;
SET ROLE hune_bad_owner;
REVOKE ALL ON metrics.owned FROM hune_bad_owner;
GRANT SELECT ON metrics.owned TO hune_bad_owner;
RESET ROLE;
GRANT SELECT ON metrics.owned TO hune_ro;

-- ---------------------------------------------------------------------------------------------------
-- The role with all rights (PS07)
-- ---------------------------------------------------------------------------------------------------

GRANT CONNECT, TEMPORARY, CREATE ON DATABASE analytics TO hune_rw;
GRANT ALL ON SCHEMA metrics TO hune_rw;
GRANT ALL ON ALL TABLES IN SCHEMA metrics TO hune_rw;
GRANT ALL ON ALL SEQUENCES IN SCHEMA metrics TO hune_rw;
GRANT ALL ON SCHEMA public TO hune_rw;
