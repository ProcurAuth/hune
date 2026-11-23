// SPDX-License-Identifier: MIT OR Apache-2.0
// The place lists of `$__adhocFilters` (7.10a, rule 4; 7.18, item 20). The unit test of `src/sql/adhoc.rs` and the
// test `tests/adhoc_filters.rs` include this file, so the two read the same lists.
//
// The lists come from the verifier v1 of the security review 3 (option A'). ClickHouse 26.8.34.13 measured the forms:
// `NOT` binds tighter than `AND`, and `?:` binds looser than `AND`.

/// The accepted places: the forms of the repository (unit, save, ClickHouse suite, browser cases, fixtures, the example
/// of 7.10a) and other plain forms where a filter only removes rows. Each MUST stay accepted.
const PLACE_ACCEPT: [&str; 35] = [
    // The unit test of `src/sql/adhoc.rs`.
    "SELECT 1 FROM t WHERE $__adhocFilters",
    "SELECT 1 FROM t where\n  $__adhocFilters",
    "SELECT 1 FROM t PREWHERE $__adhocFilters",
    "SELECT 1 FROM t WHERE x = 1 AND /* c */ $__adhocFilters",
    "SELECT 1 FROM t WHERE x = 1 and -- c\n$__adhocFilters",
    "SELECT 1 FROM t WHERE ${__adhocFilters}",
    // `tests/adhoc_filters.rs`: the templates, the macro test and the save test.
    "SELECT count() FROM t WHERE $__adhocFilters",
    "SELECT count() FROM t PREWHERE $__adhocFilters AND x = 1",
    "SELECT 1 FROM t WHERE $__timeFilter(ts) AND $__adhocFilters -- end",
    "SELECT 1 FROM t WHERE x = '$t' AND ${__adhocFilters} ORDER BY 1",
    "SELECT 1 FROM t WHERE\n/* c */ $__adhocFilters\nGROUP BY 1",
    "SELECT 1 FROM t PREWHERE ${__adhocFilters} AND x = '$t'",
    "SELECT 1 FROM t WHERE x = 1\n  AND ${__adhocFilters}",
    // The ClickHouse suite and the browser cases B218 and B219.
    "SELECT count() AS rows_kept FROM analytics.adhoc_types WHERE $__adhocFilters",
    "SELECT count() AS n\nFROM analytics.seed\nWHERE $__adhocFilters",
    // `tests/fixtures/dashboards/adhoc.json`.
    "SELECT service, host, count() AS n\nFROM analytics.events\nWHERE $__timeFilter(ts) AND env = '${env}'\n  AND $__adhocFilters\nGROUP BY service, host",
    // The server tests: panel 0 of `nodes.json` with the added line.
    "SELECT\n  $__timeInterval(ts) AS t,\n  node,\n  sum(weight) AS weight\nFROM analytics.node_weight\nWHERE $__timeFilter(ts)\n  AND node IN (${node})\n  AND $__adhocFilters\nGROUP BY t, node\nORDER BY t",
    // The example of 7.10a.
    "SELECT service, count() AS n FROM analytics.events\nWHERE $__timeFilter(ts) AND $__adhocFilters\nGROUP BY service",
    // Other plain forms.
    "SELECT id FROM t WHERE host IS NOT NULL AND $__adhocFilters",
    "SELECT id FROM t WHERE id NOT IN (5) AND $__adhocFilters",
    "SELECT id FROM t WHERE id BETWEEN 1 AND 4 AND $__adhocFilters",
    "SELECT id FROM t WHERE id NOT BETWEEN 1 AND 2 AND $__adhocFilters",
    "SELECT id FROM t WHERE NOT x AND $__adhocFilters",
    "SELECT id FROM t WHERE x::Int32 = 1 AND $__adhocFilters",
    "SELECT id FROM t WHERE x = 1 AND $__adhocFilters AND y = 2 GROUP BY a",
    "SELECT id FROM t WHERE CASE WHEN a OR b THEN 1 ELSE 0 END = 1 AND $__adhocFilters",
    "SELECT * EXCEPT (secret) FROM t WHERE $__adhocFilters",
    "SELECT id FROM t PREWHERE $__adhocFilters WHERE x = 1",
    "SELECT id FROM t WHERE $__adhocFilters LIMIT 10",
    "SELECT id FROM t WHERE $__adhocFilters SETTINGS max_threads = 1",
    "WITH 1 AS one SELECT id FROM t WHERE $__adhocFilters",
    "WITH base AS (SELECT id, host FROM t) SELECT count() FROM base WHERE $__adhocFilters",
    "SELECT a FROM t WHERE $__adhocFilters UNION ALL SELECT a FROM u",
    "SELECT a FROM u UNION ALL SELECT a FROM t WHERE $__adhocFilters",
    "SELECT t.id FROM t LEFT ANTI JOIN h ON h.id = t.id WHERE $__adhocFilters",
];

/// The places that the rule of the token before the macro accepted, and option A' refuses. In the first 14, a filter
/// adds rows that the query with no filter does not show (ClickHouse 26.8.34.13). In the next 5, the filter does not
/// apply to each row. The last 3 only narrow, but the closed rule refuses them: move the macro to the outer `WHERE`.
const PLACE_WIDEN: [&str; 22] = [
    "SELECT * FROM t WHERE NOT (restricted = 1 AND $__adhocFilters)",
    "SELECT * FROM t WHERE id NOT IN (SELECT id FROM hidden WHERE $__adhocFilters)",
    "SELECT * FROM t WHERE NOT EXISTS (SELECT 1 FROM hidden h WHERE h.id = t.id AND $__adhocFilters)",
    "SELECT * FROM t WHERE $__adhocFilters = 0",
    "SELECT * FROM t WHERE x = 1 AND $__adhocFilters IS NOT TRUE",
    "SELECT * FROM t WHERE level BETWEEN 0 AND $__adhocFilters",
    "SELECT t.id FROM t LEFT ANTI JOIN hidden h ON h.id = t.id AND $__adhocFilters",
    "SELECT id FROM t EXCEPT SELECT id FROM t WHERE $__adhocFilters",
    "SELECT id FROM t WHERE CASE WHEN restricted = 1 AND $__adhocFilters THEN 0 ELSE 1 END = 1",
    "SELECT countIf(restricted = 0 OR NOT (1 AND $__adhocFilters)) FROM t",
    "WITH f AS (SELECT id FROM hidden WHERE $__adhocFilters) SELECT * FROM t WHERE id NOT IN f",
    "SELECT * FROM t ORDER BY 1 AND $__adhocFilters DESC LIMIT 10",
    "SELECT id FROM t WHERE x AND $__adhocFilters ? 0 : 1",
    "SELECT id FROM t WHERE x AND $__adhocFilters AS g",
    "SELECT * FROM t WHERE a = 1 OR b = 2 AND $__adhocFilters",
    "SELECT * FROM t WHERE x = 1 AND $__adhocFilters OR y = 2",
    "SELECT * FROM t WHERE $__adhocFilters AND x = 1 OR y = 2",
    "SELECT t.id FROM t LEFT JOIN h ON h.id = t.id AND $__adhocFilters",
    "SELECT h, count() FROM t GROUP BY h HAVING count() > 0 AND $__adhocFilters",
    "SELECT a, count() FROM (SELECT a FROM t WHERE $__adhocFilters) GROUP BY a",
    "WITH base AS (SELECT a FROM t WHERE $__adhocFilters) SELECT count() FROM base",
    "SELECT * FROM t WHERE id IN (SELECT id FROM u WHERE $__adhocFilters)",
];

/// The places that the rule of the token before the macro refused too. Each MUST stay refused.
const PLACE_REFUSED: [&str; 10] = [
    "SELECT $__adhocFilters",
    "SELECT 1 FROM t WHERE x OR $__adhocFilters",
    "SELECT 1 FROM t WHERE NOT $__adhocFilters",
    "SELECT 1 FROM t WHERE (x) AND($__adhocFilters)",
    "SELECT 1 FROM t WHERE AND$__adhocFilters",
    "SELECT 1 FROM t HAVING $__adhocFilters",
    "SELECT 1 FROM t WHERE x = 'AND' $__adhocFilters",
    "SELECT 1 FROM t WHERE x = `AND` $__adhocFilters",
    "$__adhocFilters",
    "SELECT $__adhocFilters FROM t",
];

/// The byte range of the first macro of a text: `$__adhocFilters` or `${__adhocFilters}`.
fn macro_range(sql: &str) -> (usize, usize) {
    let plain = sql.find("$__adhocFilters").map(|at| (at, at + "$__adhocFilters".len()));
    let braced = sql.find("${__adhocFilters}").map(|at| (at, at + "${__adhocFilters}".len()));
    match (plain, braced) {
        (Some(p), Some(b)) => p.min(b),
        (p, b) => p.or(b).expect("a text of the lists has the macro"),
    }
}
