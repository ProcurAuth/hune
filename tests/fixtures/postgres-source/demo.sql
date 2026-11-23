-- Synthetic demo database for a local demo of the PostgreSQL source (not a test fixture).
-- All data is synthetic and made from formulas. No name here is a real name.
--
-- Use: demo.sh runs this file. To run it by hand, connect as a superuser to a new PostgreSQL 15 to 18 server:
--   psql -v ON_ERROR_STOP=1 -v ro_password=... [-v days=2] -f demo.sql
-- The file holds no password. It makes the database `analytics`, the schema `metrics` and the role
-- `hune_ro` with the contract of 8a.13 (the example SQL of that section).
-- The times are relative to the start of the current minute: the data ends now and starts `days` days ago.
-- The file is not idempotent: run it one time on a new server.
--
-- Tables of the schema `metrics`:
--   node_load     one row each minute for 6 hosts: load, memory use and network bytes.
--   http_requests one row each minute for 3 services and 3 status classes: requests and latency.
--   orders        one row each 5 minutes: region, channel and amount (a `numeric`).
--   fleet         one row for each host: environment, region, role, up or down, and a list of tags.
-- The columns use the types of 7.16a. The time columns are `timestamptz`.

\set ON_ERROR_STOP on
\if :{?ro_password}
\else
    DO $$ BEGIN RAISE EXCEPTION 'psql variable ro_password is not set'; END $$;
\endif
\if :{?days}
\else
    \set days 2
\endif

CREATE ROLE metrics_owner NOLOGIN;
CREATE ROLE hune_ro LOGIN
    PASSWORD :'ro_password'
    NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS
    CONNECTION LIMIT 18;
ALTER ROLE hune_ro SET default_transaction_read_only = on;
ALTER ROLE hune_ro SET statement_timeout = '60s';
ALTER ROLE hune_ro SET idle_in_transaction_session_timeout = '15s';
ALTER ROLE hune_ro SET temp_file_limit = '1GB';
ALTER ROLE hune_ro SET jit = off;

CREATE DATABASE analytics OWNER metrics_owner;
\connect analytics

REVOKE USAGE ON LANGUAGE plpgsql FROM PUBLIC;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;

-- The functions that PUBLIC runs by default and that the probe refuses: the statement of 8a.13. Without it the
-- probe gives `misconfigured` with the check R7 (pg_terminate_backend, pg_cancel_backend, pg_logical_emit_message,
-- ts_stat, ts_rewrite).
SELECT format('REVOKE EXECUTE ON FUNCTION %s FROM PUBLIC', p.oid::regprocedure)
FROM pg_proc p
WHERE p.pronamespace = 'pg_catalog'::regnamespace
  AND (p.proname IN ('pg_sleep', 'pg_sleep_for', 'pg_sleep_until', 'pg_cancel_backend',
                     'pg_terminate_backend', 'pg_notify', 'pg_logical_emit_message', 'ts_stat', 'ts_rewrite')
       OR p.proname LIKE 'pg\_advisory\_%' OR p.proname LIKE 'pg\_try\_advisory\_%'
       OR p.proname LIKE 'query\_to\_xml%' OR p.proname LIKE 'cursor\_to\_xml%')
\gexec

CREATE SCHEMA metrics AUTHORIZATION metrics_owner;

SET ROLE metrics_owner;
SET search_path = metrics, pg_catalog;

CREATE TABLE fleet (
    host   text PRIMARY KEY,
    env    text NOT NULL,
    region text NOT NULL,
    role   text NOT NULL,
    up     bool NOT NULL,
    tags   text[] NOT NULL
);
INSERT INTO fleet VALUES
    ('node-a', 'prod',  'north', 'web',   true,  ARRAY['edge', 'blue']),
    ('node-b', 'prod',  'north', 'web',   true,  ARRAY['edge', 'green']),
    ('node-c', 'prod',  'south', 'db',    true,  ARRAY['storage']),
    ('node-d', 'prod',  'south', 'cache', true,  ARRAY['memory']),
    ('node-e', 'stage', 'north', 'web',   true,  ARRAY['edge']),
    ('node-f', 'stage', 'south', 'db',    false, ARRAY['storage', 'maintenance']);

-- The start of the current minute, and the number of minutes of the data.
CREATE TEMPORARY TABLE demo_axis AS
SELECT i, date_trunc('minute', now()) - (i * interval '1 minute') AS t
FROM generate_series(0, (:days) * 1440) AS i;

CREATE TABLE node_load (
    created_at timestamptz NOT NULL,
    host       text        NOT NULL,
    env        text        NOT NULL,
    load       float8      NOT NULL,
    mem_used   float8      NOT NULL,
    net_bytes  int8        NOT NULL
);
INSERT INTO node_load
SELECT a.t, f.host, f.env,
       round((1.5 + 1.2 * sin(a.i / 90.0 + k.n) + 0.4 * sin(a.i / 7.0 + k.n * 2)
              + (abs(hashtext(f.host || a.i::text)) % 100) / 400.0)::numeric, 3)::float8,
       round((55 + 20 * sin(a.i / 360.0 + k.n) + (abs(hashtext(a.i::text || f.host)) % 100) / 25.0)::numeric, 2)::float8,
       (2000000 + 1500000 * (1 + sin(a.i / 45.0 + k.n)) + abs(hashtext(f.host || '-net-' || a.i::text)) % 500000)::int8
FROM demo_axis a
CROSS JOIN (SELECT host, env, row_number() OVER (ORDER BY host) AS n FROM fleet) AS k
JOIN fleet f ON f.host = k.host
ORDER BY a.t, f.host;
CREATE INDEX node_load_created_at ON node_load (created_at);

CREATE TABLE http_requests (
    created_at timestamptz NOT NULL,
    service    text        NOT NULL,
    status     int4        NOT NULL,
    requests   int4        NOT NULL,
    latency_ms float8      NOT NULL
);
INSERT INTO http_requests
SELECT a.t, s.service, c.status,
       greatest(0, round(s.base * c.share * (1 + 0.6 * sin(a.i / 120.0 + s.n))
                         + (abs(hashtext(s.service || c.status::text || a.i::text)) % 10))::int4),
       round((s.lat * c.slow + (abs(hashtext(a.i::text || s.service)) % 200) / 10.0)::numeric, 1)::float8
FROM demo_axis a
CROSS JOIN (VALUES ('api', 400, 20.0, 1), ('web', 250, 35.0, 2), ('search', 120, 60.0, 3)) AS s(service, base, lat, n)
CROSS JOIN (VALUES (200, 0.93, 1.0), (404, 0.05, 0.8), (500, 0.02, 3.0)) AS c(status, share, slow);
CREATE INDEX http_requests_created_at ON http_requests (created_at);

CREATE TABLE orders (
    created_at timestamptz   NOT NULL,
    region     text          NOT NULL,
    channel    text          NOT NULL,
    amount     numeric(12, 2) NOT NULL
);
INSERT INTO orders
SELECT a.t, r.region, ch.channel,
       round((20 + (abs(hashtext(a.i::text || r.region || ch.channel)) % 18000) / 100.0)::numeric, 2)
FROM demo_axis a
CROSS JOIN (VALUES ('north'), ('south'), ('east')) AS r(region)
CROSS JOIN (VALUES ('web'), ('mobile')) AS ch(channel)
WHERE a.i % 5 = 0 AND abs(hashtext(a.i::text || r.region || ch.channel || 'o')) % 3 <> 0;
CREATE INDEX orders_created_at ON orders (created_at);

RESET ROLE;
RESET search_path;
ANALYZE metrics.node_load;
ANALYZE metrics.http_requests;
ANALYZE metrics.orders;

-- The grants of 8a.13.
GRANT CONNECT ON DATABASE analytics TO hune_ro;
GRANT USAGE ON SCHEMA metrics TO hune_ro;
GRANT SELECT ON ALL TABLES IN SCHEMA metrics TO hune_ro;
ALTER DEFAULT PRIVILEGES FOR ROLE metrics_owner IN SCHEMA metrics GRANT SELECT ON TABLES TO hune_ro;
