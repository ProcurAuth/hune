-- Synthetic table of the expansion vectors and the parity capture (16.13). All data is fictitious.
-- The compose project runs this file at the first start of ClickHouse (docker-entrypoint-initdb.d).
CREATE TABLE IF NOT EXISTS analytics.events
(
    `timestamp` DateTime64(3, 'UTC'),
    -- event.* : kind and result of the event.
    `event` Tuple(
        name LowCardinality(String),
        outcome LowCardinality(Nullable(String)),
        reason LowCardinality(Nullable(String))),
    -- system.* : server that wrote the event.
    `system` Tuple(
        region LowCardinality(String),
        server LowCardinality(String),
        version LowCardinality(String)),
    -- metrics.* : size and duration of the work of the event.
    `metrics` Tuple(
        bytes Nullable(UInt64),
        duration_us Nullable(UInt64)),
    -- attributes : free fields of the event (Dynamic sub-paths).
    `attributes` JSON
)
ENGINE = MergeTree
PARTITION BY toYYYYMMDD(timestamp)
ORDER BY timestamp;

-- Ten rows so that each query of a vector can run. The other columns hold their default values.
INSERT INTO analytics.events (timestamp)
SELECT toDateTime64('2026-11-09 08:00:00', 3, 'UTC') + number FROM numbers(10);
