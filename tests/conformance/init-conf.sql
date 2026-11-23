-- Synthetic data of the series U (11.32): the table that the panel of the fixture reads.
-- The user `hune` and the profile come from tests/fixtures/clickhouse/init.sql (8.12).

CREATE DATABASE IF NOT EXISTS analytics;

CREATE TABLE IF NOT EXISTS analytics.conf_rows (n UInt32) ENGINE = MergeTree ORDER BY n;

INSERT INTO analytics.conf_rows SELECT number + 1 FROM numbers(10);

GRANT SELECT ON analytics.conf_rows TO hune;
