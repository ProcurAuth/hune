-- Synthetic setup of the test ClickHouse of the `source` job (16.8, 16.9).
-- The profile and the user are the ones of 8.12, with the test database `analytics`.
-- All passwords here are throwaway test values. They protect nothing.

CREATE DATABASE IF NOT EXISTS analytics;

CREATE SETTINGS PROFILE IF NOT EXISTS hune_readonly SETTINGS
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

CREATE USER IF NOT EXISTS hune
    IDENTIFIED WITH sha256_password BY 'hune-test-password'
    DEFAULT DATABASE analytics
    SETTINGS PROFILE 'hune_readonly';

GRANT SELECT ON analytics.* TO hune;

-- C06: accounts with readonly = 0 and readonly = 1.
CREATE SETTINGS PROFILE IF NOT EXISTS hune_readonly_0 SETTINGS readonly = 0;
CREATE SETTINGS PROFILE IF NOT EXISTS hune_readonly_1 SETTINGS readonly = 1;
CREATE USER IF NOT EXISTS hune_ro0 IDENTIFIED WITH sha256_password BY 'hune-test-password'
    DEFAULT DATABASE analytics SETTINGS PROFILE 'hune_readonly_0';
CREATE USER IF NOT EXISTS hune_ro1 IDENTIFIED WITH sha256_password BY 'hune-test-password'
    DEFAULT DATABASE analytics SETTINGS PROFILE 'hune_readonly_1';
GRANT SELECT ON analytics.* TO hune_ro0;
GRANT SELECT ON analytics.* TO hune_ro1;

-- C09: an account with a grant that the contract of 8.12 forbids.
CREATE USER IF NOT EXISTS hune_extra IDENTIFIED WITH sha256_password BY 'hune-test-password'
    DEFAULT DATABASE analytics SETTINGS PROFILE 'hune_readonly';
GRANT SELECT ON analytics.* TO hune_extra;
GRANT URL ON *.* TO hune_extra;

-- Access denied (497): an account that reads one table only.
CREATE USER IF NOT EXISTS hune_narrow IDENTIFIED WITH sha256_password BY 'hune-test-password'
    DEFAULT DATABASE analytics SETTINGS PROFILE 'hune_readonly';
GRANT SELECT ON analytics.seed TO hune_narrow;

-- C12 and the settings checks: an account whose profile has a lower MAX than the source keys.
CREATE SETTINGS PROFILE IF NOT EXISTS hune_low_max SETTINGS
    readonly = 2,
    max_execution_time = 60 MAX 60,
    max_result_rows = 50 MAX 50,
    max_result_bytes = 33554432 MAX 33554432,
    max_query_size = 1048576 MAX 1048576,
    result_overflow_mode = 'throw' CONST,
    timeout_overflow_mode = 'throw' CONST,
    read_overflow_mode = 'throw' CONST;
CREATE USER IF NOT EXISTS hune_lowmax IDENTIFIED WITH sha256_password BY 'hune-test-password'
    DEFAULT DATABASE analytics SETTINGS PROFILE 'hune_low_max';
GRANT SELECT ON analytics.* TO hune_lowmax;

-- Seed data with known times (C01, C02).
CREATE TABLE IF NOT EXISTS analytics.seed
(
    event_time DateTime,
    node LowCardinality(String),
    weight Decimal(38, 9),
    blocks UInt32,
    active Bool
)
ENGINE = MergeTree
ORDER BY event_time;

INSERT INTO analytics.seed
SELECT
    toDateTime(1794211200) + number * 20,
    concat('v', toString(number % 3)),
    toDecimal128(number, 9) + toDecimal128('0.5', 9),
    toUInt32(number),
    number % 2 = 0
FROM numbers(30);

CREATE TABLE IF NOT EXISTS analytics.secret (x UInt8) ENGINE = Memory;
INSERT INTO analytics.secret VALUES (1);

-- C07: a table with a SimpleAggregateFunction column.
CREATE TABLE IF NOT EXISTS analytics.types_extra
(
    k UInt8,
    total SimpleAggregateFunction(sum, UInt64),
    peak SimpleAggregateFunction(max, Nullable(Float64))
)
ENGINE = AggregatingMergeTree
ORDER BY k;
INSERT INTO analytics.types_extra VALUES (1, 18446744073709551615, 2.5), (2, 7, NULL);
