-- SPDX-License-Identifier: MIT OR Apache-2.0
--
-- Synthetic data of the Hune demo. All names and values are generic.
-- The script makes the database `demo` and a read-only user for Hune.
-- It runs once, and it makes data for the last 24 hours, relative to now().
-- The data come from hash functions, so the same run gives the same shape.
--
-- DEMO VALUE: the password of `hune_reader` is a public demo value.
-- It protects nothing. Do not use it outside this demo.

CREATE DATABASE IF NOT EXISTS demo;

-- The account of Hune follows the contract of the documentation (section 8.12):
-- a dedicated user, readonly = 2, limits with MAX, overflow modes fixed to `throw`,
-- and SELECT grants only.
CREATE SETTINGS PROFILE IF NOT EXISTS hune_demo_readonly SETTINGS
    readonly = 2,
    max_execution_time = 60 MAX 60,
    max_result_rows = 100000 MAX 100000,
    max_result_bytes = 33554432 MAX 33554432,
    max_rows_to_read = 20000000000 MAX 20000000000,
    max_memory_usage = 4294967296 MAX 4294967296,
    max_query_size = 1048576 MAX 1048576,
    result_overflow_mode = 'throw' CONST,
    timeout_overflow_mode = 'throw' CONST,
    read_overflow_mode = 'throw' CONST;

CREATE USER IF NOT EXISTS hune_reader
    IDENTIFIED WITH sha256_password BY 'hune-demo-password'
    DEFAULT DATABASE demo
    SETTINGS PROFILE 'hune_demo_readonly';

GRANT SELECT ON demo.* TO hune_reader;

-- Requests of five services on six hosts. The traffic follows a daily curve.
-- An incident (slow and failing checkout on eu-west) starts 5 hours ago and lasts 45 minutes.
-- A short slowdown of payments happens 2 hours ago.
-- The data end at the last quarter of an hour, so that no chart shows a partial bucket at its right edge.
CREATE TABLE IF NOT EXISTS demo.requests
(
    ts DateTime,
    service LowCardinality(String),
    host LowCardinality(String),
    region LowCardinality(String),
    status UInt16,
    latency_ms Float32,
    bytes UInt32
)
ENGINE = MergeTree
ORDER BY (service, ts);

INSERT INTO demo.requests
WITH
    toStartOfInterval(now(), INTERVAL 15 MINUTE) AS t_end
SELECT
    ts,
    service,
    concat('host-', toString(host_n)) AS host,
    ['eu-west', 'eu-west', 'us-east', 'us-east', 'ap-south', 'ap-south'][host_n] AS region,
    status,
    toFloat32(round(latency, 1)) AS latency_ms,
    toUInt32(base_bytes * (0.4 + 1.2 * u_bytes)) AS bytes
FROM
(
    SELECT
        ts,
        service,
        host_n,
        status_roll,
        (cityHash64(m, k, 7) % 1000000) / 1000000 AS u_bytes,
        multiIf(service = 'search', 45, service = 'catalog', 30, service = 'checkout', 120, service = 'auth', 25, 180) AS base_ms,
        multiIf(service = 'search', 9000, service = 'catalog', 24000, service = 'checkout', 3500, service = 'auth', 800, 1500) AS base_bytes,
        in_incident,
        in_slowdown,
        -- The sum of three uniform values is close to a normal law.
        ((cityHash64(m, k, 4) % 1000000) + (cityHash64(m, k, 5) % 1000000) + (cityHash64(m, k, 6) % 1000000)) / 1000000 - 1.5 AS z,
        sin((toHour(ts) + toMinute(ts) / 60 - 8) / 24 * 2 * pi()) AS daily,
        multiIf(in_incident, 0.12, 0.004) AS p_fail,
        -- Final columns
        base_ms * exp(0.45 * z) * (1 + 0.25 * (0.5 + 0.5 * daily)) * multiIf(in_incident, 3.5, in_slowdown, 2.2, 1) AS latency,
        multiIf(
            status_roll < p_fail, [500, 502, 503][1 + cityHash64(m, k, 8) % 3],
            service IN ('search', 'catalog') AND status_roll < p_fail + 0.025, 404,
            status_roll < p_fail + 0.025 + 0.03, 301,
            200) AS status
    FROM
    (
        SELECT
            m,
            k,
            t_end - toIntervalMinute(m) + toIntervalSecond(cityHash64(m, k, 1) % 60) AS ts,
            (cityHash64(m, k, 2) % 1000000) / 1000000 AS r_service,
            multiIf(r_service < 0.30, 'search', r_service < 0.55, 'catalog', r_service < 0.75, 'checkout', r_service < 0.90, 'auth', 'payments') AS service,
            toUInt8(1 + cityHash64(m, k, 3) % 6) AS host_n,
            (cityHash64(m, k, 9) % 1000000) / 1000000 AS status_roll,
            service = 'checkout' AND host_n <= 2 AND m BETWEEN 226 AND 300 AS in_incident,
            service = 'payments' AND m BETWEEN 100 AND 120 AS in_slowdown
        FROM
        (
            SELECT
                number + 1 AS m,
                toUInt32(18 + 22 * (0.5 + 0.5 * sin((toHour(t_end - toIntervalMinute(number + 1)) + toMinute(t_end - toIntervalMinute(number + 1)) / 60 - 8) / 24 * 2 * pi())) + cityHash64(number, 0) % 8) AS n_rows
            FROM numbers(1440)
        )
        ARRAY JOIN range(n_rows) AS k
    )
);

-- Server errors with a message. The rows come from the requests with a status of 500 or more.
CREATE TABLE IF NOT EXISTS demo.errors
(
    ts DateTime,
    service LowCardinality(String),
    host LowCardinality(String),
    code UInt16,
    message String
)
ENGINE = MergeTree
ORDER BY (service, ts);

INSERT INTO demo.errors
SELECT
    ts,
    service,
    host,
    status AS code,
    [
        'upstream connection timed out',
        'database connection pool exhausted',
        'payment gateway returned an error',
        'cache backend not reachable',
        'request queue is full'
    ][1 + cityHash64(ts, host, service) % 5] AS message
FROM demo.requests
WHERE status >= 500;

-- Deployments: the marks of the annotations.
CREATE TABLE IF NOT EXISTS demo.deployments
(
    ts DateTime,
    service LowCardinality(String),
    version String,
    kind LowCardinality(String)
)
ENGINE = MergeTree
ORDER BY ts;

INSERT INTO demo.deployments
SELECT now() - toIntervalMinute(minutes_ago), service, version, kind
FROM
(
    SELECT * FROM VALUES('minutes_ago UInt32, service String, version String, kind String',
        (1290, 'search', 'v2.14.0', 'deploy'),
        (1010, 'catalog', 'v5.2.1', 'deploy'),
        (760, 'auth', 'v1.9.4', 'deploy'),
        (540, 'payments', 'v3.0.2', 'deploy'),
        (312, 'checkout', 'v4.8.0', 'deploy'),
        (262, 'checkout', 'v4.7.3', 'rollback'),
        (190, 'search', 'v2.14.1', 'deploy'),
        (64, 'catalog', 'v5.2.2', 'deploy'),
        (22, 'auth', 'v1.9.5', 'deploy'))
);

-- Events with a start and an end: incidents and maintenance windows.
CREATE TABLE IF NOT EXISTS demo.events
(
    ts DateTime,
    ts_end DateTime,
    title String,
    text String,
    tags String
)
ENGINE = MergeTree
ORDER BY ts;

INSERT INTO demo.events
SELECT now() - toIntervalMinute(minutes_ago), now() - toIntervalMinute(minutes_ago - duration), title, text, tags
FROM
(
    SELECT * FROM VALUES('minutes_ago UInt32, duration UInt32, title String, text String, tags String',
        (300, 75, 'Incident', 'Slow and failing checkout in eu-west', 'incident,checkout'),
        (120, 20, 'Slowdown', 'Payments gateway latency', 'incident,payments'),
        (800, 30, 'Maintenance', 'Database failover test in us-east', 'maintenance,us-east'))
);

-- A small inventory of servers: the table panel with cell colours, links and thresholds.
CREATE TABLE IF NOT EXISTS demo.servers
(
    host LowCardinality(String),
    region LowCardinality(String),
    role LowCardinality(String),
    cpu_pct Float32,
    mem_pct Float32,
    disk_pct Float32,
    uptime_days UInt16,
    state LowCardinality(String)
)
ENGINE = MergeTree
ORDER BY host;

INSERT INTO demo.servers VALUES
    ('host-1', 'eu-west', 'web', 71.4, 63.0, 41.2, 128, 'degraded'),
    ('host-2', 'eu-west', 'web', 64.8, 58.5, 39.9, 128, 'degraded'),
    ('host-3', 'us-east', 'web', 38.2, 49.1, 44.7, 212, 'ok'),
    ('host-4', 'us-east', 'cache', 22.6, 81.7, 36.0, 212, 'ok'),
    ('host-5', 'ap-south', 'web', 45.9, 52.3, 78.4, 64, 'ok'),
    ('host-6', 'ap-south', 'db', 83.5, 88.2, 91.6, 301, 'critical');

-- Application logs. A burst of errors follows the incident of checkout.
CREATE TABLE IF NOT EXISTS demo.logs
(
    ts DateTime,
    level LowCardinality(String),
    service LowCardinality(String),
    host LowCardinality(String),
    message String
)
ENGINE = MergeTree
ORDER BY (service, ts);

INSERT INTO demo.logs
SELECT
    ts,
    level,
    service,
    host,
    multiIf(
        level = 'error', [
            'upstream connect error: connection timed out after 5000 ms',
            'database connection pool exhausted (max 50)',
            'payment gateway returned HTTP 502',
            'cache backend not reachable: connection refused'
        ][1 + cityHash64(n, 11) % 4],
        level = 'warn', [
            'slow query detected: 1450 ms',
            'retrying request (attempt 2 of 3)',
            'queue depth above 80 percent',
            'certificate expires in 14 days'
        ][1 + cityHash64(n, 12) % 4],
        level = 'debug', concat('cache lookup key=item:', toString(cityHash64(n, 13) % 5000), ' hit=true'),
        concat('GET /api/v1/items/', toString(cityHash64(n, 14) % 9000), ' completed in ', toString(8 + cityHash64(n, 15) % 120), ' ms')
    ) AS message
FROM
(
    SELECT
        number AS n,
        now() - toIntervalSecond(cityHash64(number, 10) % 86400) AS ts,
        multiIf(
            service = 'checkout' AND ts BETWEEN now() - toIntervalMinute(300) AND now() - toIntervalMinute(225), if(cityHash64(number, 16) % 100 < 45, 'error', 'warn'),
            cityHash64(number, 17) % 100 < 70, 'info',
            cityHash64(number, 17) % 100 < 85, 'warn',
            cityHash64(number, 17) % 100 < 94, 'error',
            'debug') AS level,
        ['search', 'catalog', 'checkout', 'auth', 'payments'][1 + cityHash64(number, 18) % 5] AS service,
        concat('host-', toString(1 + cityHash64(number, 19) % 6)) AS host
    FROM numbers(4000)
);
