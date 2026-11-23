// SPDX-License-Identifier: MIT OR Apache-2.0
// The synthetic cases of the expansion vectors (16.13, case I15).
//
// Each case holds only the input of the expansion. The capture tool (vectors.mjs) sends each case to
// the pinned Grafana, reads the answer, and writes one vector file. The names of the members are the
// names of the vector files in tests/golden/expansion/.
//
// A case has these members:
//   id             Unique name. It is the name of the vector file.
//   tags           Features that the case covers. The tool counts the vectors for each tag.
//   template       The SQL text that the author writes.
//   variables      The variable definitions, in the form of tests/golden/sql/variables.json.
//                  For a "query" variable, "options" holds the rows that the options query gives.
//   values         The selection of each variable, in the form of the member "variables" of 7.7.
//   from_ms, to_ms The time range. The default is the range of 7.3.
//   max_data_points, min_interval   The panel inputs of 7.4. min_interval is a Grafana duration or null.
//
// The differences between Hune and Grafana are not in this file. They are in vector-differences.json.
// No file of this tool holds a real name, a real table or a secret. All names are synthetic.

export const GRAFANA_VERSION = "12.4.2";
export const PLUGIN = "grafana-clickhouse-datasource 4.5.1";

export const FROM = 1794211200000; // 2026-11-09T08:00:00Z
export const TO = 1794232800000; // 2026-11-09T14:00:00Z (6 hours)

const DEFAULT_SQL_TABLE = "FROM analytics.events";

/** Variable definition helpers. */
const custom = (name, options, { multi = false, all = false, all_value = null } = {}) => ({
  name,
  kind: "custom",
  options,
  multi,
  include_all: all,
  all_value,
});
const query = (name, options, { multi = false, all = false, all_value = null } = {}) => ({
  name,
  kind: "query",
  options,
  multi,
  include_all: all,
  all_value,
});
const text = (name, value_type = "text") => ({ name, kind: "text", value_type });

/** Selection helpers. */
const one = (v) => ({ all: false, values: [v] });
const many = (...v) => ({ all: false, values: v });
const ALL = (values = []) => ({ all: true, values });

export const cases = [];

function add(id, tags, template, extra = {}) {
  if (cases.some((c) => c.id === id)) throw new Error(`duplicate case id ${id}`);
  cases.push({
    id,
    tags,
    template,
    variables: [],
    values: {},
    from_ms: FROM,
    to_ms: TO,
    max_data_points: 1200,
    min_interval: null,
    ...extra,
  });
}

// ---------------------------------------------------------------------------------------------
// Macros of the plugin (7.10 and the others that the plugin 4.5.1 knows).
// ---------------------------------------------------------------------------------------------

const macro = (id, name, template, extra = {}) => add(id, [`macro:${name}`], template, extra);

macro("macro-timeFilter-basic", "$__timeFilter", `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(timestamp)`);
macro("macro-timeFilter-blanks", "$__timeFilter", `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(   timestamp\n  )`);
macro("macro-timeFilter-qualified", "$__timeFilter", `SELECT count() FROM analytics.events AS l WHERE $__timeFilter(l.timestamp)`);
macro(
  "macro-timeFilter-call-with-comma",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(toDateTime(timestamp, 'UTC'))`,
);
macro(
  "macro-timeFilter-string-with-paren",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(concat(')', toString(timestamp)))`,
);
macro(
  "macro-timeFilter-two-uses",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(timestamp) OR $__timeFilter(timestamp + 1)`,
);
macro(
  "macro-timeFilter-nested-parens",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(toStartOfHour((timestamp)))`,
);
macro(
  "macro-timeFilter-range-fractions",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(timestamp)`,
  { from_ms: FROM + 123, to_ms: TO + 987 },
);
macro(
  "macro-timeFilter-range-one-hour",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(timestamp)`,
  { from_ms: TO - 3600000 },
);
macro(
  "macro-timeFilter-lowercase-name",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timefilter(timestamp)`,
);
macro(
  "macro-timeFilter-in-string",
  "$__timeFilter",
  `SELECT 'a $__timeFilter(timestamp) b' AS s, count() ${DEFAULT_SQL_TABLE}`,
);
macro(
  "macro-timeFilter-in-line-comment",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} -- $__timeFilter(timestamp)\nWHERE 1`,
);
macro(
  "macro-timeFilter-in-block-comment",
  "$__timeFilter",
  `SELECT count() /* $__timeFilter(timestamp) */ ${DEFAULT_SQL_TABLE}`,
);
macro(
  "macro-timeFilter-two-arguments",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(timestamp, 1)`,
);
macro(
  "macro-timeFilter-no-parenthesis",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter`,
);
macro(
  "macro-timeFilter-comment-in-argument",
  "$__timeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(timestamp /* c */)`,
);

macro("macro-timeFilter_ms-basic", "$__timeFilter_ms", `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter_ms(timestamp)`);
macro(
  "macro-timeFilter_ms-range-fractions",
  "$__timeFilter_ms",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__timeFilter_ms(timestamp)`,
  { from_ms: FROM + 123, to_ms: TO + 987 },
);

macro("macro-fromTime-basic", "$__fromTime", "SELECT $__fromTime AS f");
macro("macro-fromTime-range-fractions", "$__fromTime", "SELECT $__fromTime AS f", { from_ms: FROM + 999, to_ms: TO + 1 });
macro("macro-fromTime-followed-by-paren", "$__fromTime", "SELECT $__fromTime (1)");
macro("macro-fromTime-empty-call", "$__fromTime", "SELECT $__fromTime()");
macro("macro-fromTime-in-string", "$__fromTime", "SELECT '$__fromTime' AS s");
macro("macro-fromTime-in-comment", "$__fromTime", "SELECT 1 -- $__fromTime\n");
macro("macro-fromTime-before-comma", "$__fromTime", "SELECT $__fromTime,$__toTime");
macro("macro-fromTime_ms-basic", "$__fromTime_ms", "SELECT $__fromTime_ms AS f");
macro("macro-fromTime_ms-range-fractions", "$__fromTime_ms", "SELECT $__fromTime_ms AS f", { from_ms: FROM + 123, to_ms: TO + 987 });

macro("macro-toTime-basic", "$__toTime", "SELECT $__toTime AS t");
macro("macro-toTime-range-fractions", "$__toTime", "SELECT $__toTime AS t", { from_ms: FROM + 123, to_ms: TO + 999 });
macro("macro-toTime_ms-basic", "$__toTime_ms", "SELECT $__toTime_ms AS t");
macro("macro-toTime_ms-range-fractions", "$__toTime_ms", "SELECT $__toTime_ms AS t", { from_ms: FROM + 123, to_ms: TO + 987 });

macro("macro-timeInterval-basic", "$__timeInterval", `SELECT $__timeInterval(timestamp) AS t ${DEFAULT_SQL_TABLE} GROUP BY t`);
macro(
  "macro-timeInterval-blanks-and-comment",
  "$__timeInterval",
  `SELECT $__timeInterval( /* c */ timestamp ) AS t ${DEFAULT_SQL_TABLE} GROUP BY t`,
);
macro(
  "macro-timeInterval-nested-fromTime",
  "$__timeInterval",
  "SELECT $__timeInterval($__fromTime) AS t",
);
macro(
  "macro-timeInterval-call-with-comma",
  "$__timeInterval",
  `SELECT $__timeInterval(toDateTime(timestamp, 'UTC')) AS t ${DEFAULT_SQL_TABLE} GROUP BY t`,
);
macro(
  "macro-timeInterval-in-string",
  "$__timeInterval",
  "SELECT '$__timeInterval(t)' AS s",
);
macro("macro-timeInterval_ms-basic", "$__timeInterval_ms", `SELECT $__timeInterval_ms(timestamp) AS t ${DEFAULT_SQL_TABLE} GROUP BY t`);
macro(
  "macro-timeInterval_ms-sub-second",
  "$__timeInterval_ms",
  `SELECT $__timeInterval_ms(timestamp) AS t ${DEFAULT_SQL_TABLE} GROUP BY t`,
  { to_ms: FROM + 60000 },
);

macro("macro-interval_s-basic", "$__interval_s", "SELECT $__interval_s AS s");
macro("macro-interval_s-in-expression", "$__interval_s", "SELECT intDiv(60, $__interval_s) AS n");
macro("macro-interval_s-in-string", "$__interval_s", "SELECT '$__interval_s' AS s");
macro("macro-interval_s-in-comment", "$__interval_s", "SELECT 1 -- $__interval_s\n");

macro("macro-dateFilter-basic", "$__dateFilter", `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__dateFilter(timestamp)`);
macro(
  "macro-dateFilter-across-midnight",
  "$__dateFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__dateFilter(timestamp)`,
  { from_ms: 1794268800000 - 3600000, to_ms: 1794268800000 + 3600000 },
);
macro(
  "macro-dateTimeFilter-basic",
  "$__dateTimeFilter",
  `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__dateTimeFilter(toDate(timestamp), timestamp)`,
);
// $__dt is the second name of $__dateTimeFilter in the plugin (7.10, D08).
macro("macro-dt-basic", "$__dt", `SELECT count() ${DEFAULT_SQL_TABLE} WHERE $__dt(toDate(timestamp), timestamp)`);

// $__conditionalAll is a plugin front-end macro. Grafana reads the current value of the variable.
const envMulti = custom("env", ["prod", "dev", "test"], { multi: true, all: true });
macro(
  "macro-conditionalAll-all-selected",
  "$__conditionalAll",
  "SELECT 1 WHERE $__conditionalAll(env IN ($env), $env)",
  { variables: [envMulti], values: { env: ALL() } },
);
macro(
  "macro-conditionalAll-two-selected",
  "$__conditionalAll",
  "SELECT 1 WHERE $__conditionalAll(env IN ($env), $env)",
  { variables: [envMulti], values: { env: many("prod", "dev") } },
);
macro(
  "macro-conditionalAll-one-selected-braces",
  "$__conditionalAll",
  "SELECT 1 WHERE $__conditionalAll(env IN (${env}), ${env})",
  { variables: [envMulti], values: { env: many("prod") } },
);

// The built-in variables of Grafana. The front end interpolates them before the plugin runs.
macro("builtin-interval-ms", "$__interval_ms", "SELECT $__interval_ms AS i");
macro("builtin-interval", "$__interval", "SELECT '$__interval' AS i");
macro("builtin-from", "$__from", "SELECT $__from AS f");
macro("builtin-to", "$__to", "SELECT $__to AS t");
macro("builtin-range-s", "$__range_s", "SELECT $__range_s AS r");
macro("builtin-range-ms", "$__range_ms", "SELECT $__range_ms AS r");
macro("builtin-range", "$__range", "SELECT '$__range' AS r");
macro("unknown-macro-name", "unknown", "SELECT $__nothing AS n");

// ---------------------------------------------------------------------------------------------
// The interval rule (7.4).
// ---------------------------------------------------------------------------------------------

const INTERVAL_SQL = `SELECT $__timeInterval(timestamp) AS t, $__interval_s AS s ${DEFAULT_SQL_TABLE} WHERE $__timeFilter(timestamp) GROUP BY t`;
const interval = (id, extra, tag = "interval:range") => add(id, [tag, "macro:$__interval_s", "macro:$__timeInterval"], INTERVAL_SQL, extra);

const SEC = 1000;
const MIN = 60 * SEC;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const range = (length, from = FROM) => ({ from_ms: from, to_ms: from + length });

// Ranges, with 1200 points and no minimum.
for (const [name, len] of [
  ["5s", 5 * SEC],
  ["30s", 30 * SEC],
  ["1m", MIN],
  ["10m", 10 * MIN],
  ["1h", HOUR],
  ["6h", 6 * HOUR],
  ["24h", DAY],
  ["7d", 7 * DAY],
  ["30d", 30 * DAY],
  ["90d", 90 * DAY],
  ["1y", 365 * DAY],
  ["5y", 5 * 365 * DAY],
]) {
  interval(`interval-range-${name}`, range(len));
}

// Points of the panel, on a range of 6 hours.
for (const d of [1, 2, 10, 100, 500, 1200, 5000, 10000]) {
  interval(`interval-points-${d}`, { ...range(6 * HOUR), max_data_points: d }, "interval:max_data_points");
}

// Minimum interval of the panel, on a range of 1 hour with 1200 points (R = 3 s).
for (const m of ["1ms", "100ms", "1s", "2s", "3s", "10s", "1m", "5m", "1h", "1d"]) {
  interval(`interval-min-${m}`, { ...range(HOUR), min_interval: m }, "interval:min_interval");
}

// The examples of 7.4.
interval("interval-example-6h-1200", range(6 * HOUR), "interval:example");
interval("interval-example-7d-800", { ...range(7 * DAY), max_data_points: 800 }, "interval:example");
interval("interval-example-1h-1200-min-1m", { ...range(HOUR), min_interval: "1m" }, "interval:example");
interval("interval-example-10m-1920", { ...range(10 * MIN), max_data_points: 1920 }, "interval:example");
interval("interval-example-30d-1000", { ...range(30 * DAY), max_data_points: 1000 }, "interval:example");

// Each bound of the table of 7.4, with 100 points: R is just below the bound, and R equals the bound.
export const ROUND_BOUNDS = [
  10, 15, 35, 75, 150, 350, 750, 1500, 3500, 7500, 12500, 17500, 25000, 45000, 90000, 210000, 450000, 750000, 1050000,
  1500000, 2700000, 5400000, 9000000, 16200000, 32400000, 86400000, 604800000, 1814400000, 3628800000,
];
for (const bound of ROUND_BOUNDS) {
  interval(`interval-bound-${bound}-below`, { ...range(bound * 100 - 1), max_data_points: 100 }, "interval:bound");
  interval(`interval-bound-${bound}-equal`, { ...range(bound * 100), max_data_points: 100 }, "interval:bound");
}

// ---------------------------------------------------------------------------------------------
// Variables (7.6 to 7.9).
// ---------------------------------------------------------------------------------------------

const v = (id, tags, template, variables, values, extra = {}) => add(id, tags, template, { variables, values, ...extra });

const envC = custom("env", ["prod", "dev"]);
const envCall = custom("env", ["prod", "dev"], { all: true });
const envCallValue = custom("env", ["prod", "dev"], { all: true, all_value: "%" });
const envM = custom("env", ["prod", "dev", "test"], { multi: true });
const envMall = custom("env", ["prod", "dev", "test"], { multi: true, all: true });
const envMallValue = custom("env", ["prod", "dev", "test"], { multi: true, all: true, all_value: ".*" });

// Custom, single value.
v("var-custom-single-string", ["var:custom-single"], "SELECT 1 WHERE e = '$env'", [envC], { env: one("dev") });
v("var-custom-single-code", ["var:custom-single"], "SELECT 1 WHERE e = $env", [envC], { env: one("dev") });
v("var-custom-single-braces", ["var:custom-single"], "SELECT 1 WHERE e = '${env}'", [envC], { env: one("prod") });
v("var-custom-single-braces-suffix", ["var:custom-single"], "SELECT 1 WHERE e = '${env}_name'", [envC], { env: one("prod") });
v("var-custom-single-longest-name", ["var:custom-single", "edge:longest-name"], "SELECT 1 WHERE e = '$env_name'", [envC, custom("env_name", ["x1", "x2"])], {
  env: one("prod"),
  env_name: one("x2"),
});
v("var-custom-single-in-quoted-name", ["var:custom-single"], 'SELECT "$env" FROM t', [custom("env", ["a", "b"])], { env: one("a") });
v("var-custom-single-in-backtick-name", ["var:custom-single"], "SELECT `$env` FROM t", [custom("env", ["a", "b"])], { env: one("a") });
v("var-custom-single-in-line-comment", ["var:custom-single", "edge:comment"], "SELECT 1 -- $env\n", [envC], { env: one("dev") });
v("var-custom-single-in-block-comment", ["var:custom-single", "edge:comment"], "SELECT 1 /* $env */", [envC], { env: one("dev") });
v("var-custom-single-two-uses", ["var:custom-single"], "SELECT '$env', '$env'", [envC], { env: one("dev") });
v("var-custom-single-old-syntax", ["var:custom-single", "edge:old-syntax"], "SELECT '[[env]]'", [envC], { env: one("dev") });
v("var-custom-single-unknown-name-stays", ["edge:unknown-name"], "SELECT '$nope', $nope", [envC], { env: one("dev") });
v("var-custom-single-dollar-end", ["edge:dollar"], "SELECT '$' AS d, '$env$'", [envC], { env: one("dev") });
v("var-custom-single-space-in-value", ["var:custom-single"], "SELECT '$env'", [custom("env", ["a b", "c"])], { env: one("a b") });
v("var-custom-single-unicode-value", ["var:custom-single"], "SELECT '$env'", [custom("env", ["café", "x"])], { env: one("café") });
v("var-custom-single-number-code", ["var:custom-single"], "SELECT * FROM numbers(10) LIMIT $env", [custom("env", ["5", "10"])], { env: one("5") });

// Custom, single value, every format.
for (const f of ["raw", "csv", "singlequote", "sqlstring"]) {
  v(`var-custom-single-format-${f}`, ["var:custom-single", `format:${f}`], `SELECT 1 WHERE e = \${env:${f}}`, [envC], { env: one("dev") });
}
for (const f of ["json", "doublequote", "pipe", "regex", "glob", "lucene", "percentencode", "text", "queryparam", "html", "distributed", "date"]) {
  v(`var-custom-single-format-${f}`, ["var:custom-single", `format:${f}`], `SELECT 1 WHERE e = \${env:${f}}`, [envC], { env: one("dev") });
}

// The formats that Hune admits only inside a string literal (D7), for a single value and for a list.
// The front end of Grafana formats the value without a look at the context, so the text inside the
// quote marks is the text of the same format in code.
const LITERAL_FORMATS = ["json", "doublequote", "pipe", "regex", "glob", "lucene", "percentencode", "text", "queryparam", "html", "distributed"];
for (const f of LITERAL_FORMATS) {
  v(`var-custom-single-in-string-format-${f}`, ["var:custom-single", `format:${f}`, "edge:in-string"], `SELECT 1 WHERE e = '\${env:${f}}'`, [envC], { env: one("dev") });
}

// Custom, multi.
v("var-custom-multi-default", ["var:custom-multi"], "SELECT 1 WHERE e IN ($env)", [envM], { env: many("prod", "dev") });
v("var-custom-multi-one-value", ["var:custom-multi"], "SELECT 1 WHERE e IN ($env)", [envM], { env: many("prod") });
v("var-custom-multi-three-values-braces", ["var:custom-multi"], "SELECT 1 WHERE e IN (${env})", [envM], { env: many("prod", "dev", "test") });
v("var-custom-multi-in-string", ["var:custom-multi"], "SELECT 1 WHERE e = '$env'", [envM], { env: many("prod", "dev") });
v("var-custom-multi-in-string-csv", ["var:custom-multi", "format:csv"], "SELECT 1 WHERE e = '${env:csv}'", [envM], { env: many("prod", "dev") });
for (const f of ["raw", "csv", "singlequote", "sqlstring"]) {
  v(`var-custom-multi-format-${f}`, ["var:custom-multi", `format:${f}`], `SELECT 1 WHERE e IN (\${env:${f}})`, [envM], { env: many("prod", "dev") });
}
for (const f of ["json", "doublequote", "pipe", "regex", "glob", "lucene", "percentencode", "text", "queryparam", "html", "distributed"]) {
  v(`var-custom-multi-format-${f}`, ["var:custom-multi", `format:${f}`], `SELECT 1 WHERE e IN (\${env:${f}})`, [envM], { env: many("prod", "dev") });
}
for (const f of LITERAL_FORMATS) {
  v(`var-custom-multi-in-string-format-${f}`, ["var:custom-multi", `format:${f}`, "edge:in-string"], `SELECT 1 WHERE e = '\${env:${f}}'`, [envM], { env: many("prod", "dev") });
}
v("var-custom-multi-numbers-default", ["var:custom-multi", "edge:numbers"], "SELECT 1 WHERE n IN ($n)", [custom("n", ["1", "2", "3"], { multi: true })], {
  n: many("1", "2"),
});
v("var-custom-multi-numbers-csv", ["var:custom-multi", "format:csv", "edge:numbers"], "SELECT 1 WHERE n IN (${n:csv})", [custom("n", ["1", "2", "3"], { multi: true })], {
  n: many("1", "2", "3"),
});
v("var-custom-multi-numbers-raw", ["var:custom-multi", "format:raw", "edge:numbers"], "SELECT 1 WHERE n IN (${n:raw})", [custom("n", ["1", "-2", "3.5"], { multi: true })], {
  n: many("1", "-2", "3.5"),
});

// Custom, include All.
v("var-custom-all-single-all-selected", ["var:custom-all"], "SELECT 1 WHERE e IN ($env)", [envCall], { env: ALL() });
v("var-custom-all-single-one-selected", ["var:custom-all"], "SELECT 1 WHERE e = $env", [envCall], { env: one("dev") });
v("var-custom-all-single-one-selected-in-string", ["var:custom-all"], "SELECT 1 WHERE e = '$env'", [envCall], { env: one("dev") });
v("var-custom-all-single-all-in-string", ["var:custom-all"], "SELECT 1 WHERE e = '$env'", [envCall], { env: ALL() });
v("var-custom-all-single-all-csv", ["var:custom-all", "format:csv"], "SELECT 1 WHERE e IN (${env:csv})", [envCall], { env: ALL() });
v("var-custom-all-single-all-singlequote", ["var:custom-all", "format:singlequote"], "SELECT 1 WHERE e IN (${env:singlequote})", [envCall], { env: ALL() });
v("var-custom-all-single-all-sqlstring", ["var:custom-all", "format:sqlstring"], "SELECT 1 WHERE e IN (${env:sqlstring})", [envCall], { env: ALL() });
v("var-custom-all-single-all-raw", ["var:custom-all", "format:raw"], "SELECT 1 WHERE e IN (${env:raw})", [envCall], { env: ALL() });
v("var-custom-all-value-single-all", ["var:custom-all-value"], "SELECT 1 WHERE e LIKE '$env'", [envCallValue], { env: ALL() });
v("var-custom-all-value-single-all-code", ["var:custom-all-value"], "SELECT 1 WHERE $env", [custom("env", ["a = 1", "b = 2"], { all: true, all_value: "1 = 1" })], { env: ALL() });
v("var-custom-all-value-single-one-selected", ["var:custom-all-value"], "SELECT 1 WHERE e LIKE '$env'", [envCallValue], { env: one("dev") });
v("var-custom-all-value-single-all-csv", ["var:custom-all-value", "format:csv"], "SELECT 1 WHERE e LIKE '${env:csv}'", [envCallValue], { env: ALL() });
v("var-custom-all-value-single-all-singlequote", ["var:custom-all-value", "format:singlequote"], "SELECT 1 WHERE e LIKE ${env:singlequote}", [envCallValue], { env: ALL() });
v("var-custom-all-multi-all", ["var:custom-all"], "SELECT 1 WHERE e IN ($env)", [envMall], { env: ALL() });
v("var-custom-all-multi-all-csv", ["var:custom-all", "format:csv"], "SELECT 1 WHERE e IN (${env:csv})", [envMall], { env: ALL() });
v("var-custom-all-multi-all-singlequote", ["var:custom-all", "format:singlequote"], "SELECT 1 WHERE e IN (${env:singlequote})", [envMall], { env: ALL() });
v("var-custom-all-multi-all-sqlstring", ["var:custom-all", "format:sqlstring"], "SELECT 1 WHERE e IN (${env:sqlstring})", [envMall], { env: ALL() });
v("var-custom-all-multi-some", ["var:custom-all"], "SELECT 1 WHERE e IN ($env)", [envMall], { env: many("dev", "test") });
v("var-custom-all-value-multi-all", ["var:custom-all-value"], "SELECT 1 WHERE match(e, '$env')", [envMallValue], { env: ALL() });
v("var-custom-all-value-multi-all-in-code", ["var:custom-all-value"], "SELECT 1 WHERE e IN ($env)", [custom("env", ["prod", "dev"], { multi: true, all: true, all_value: "'prod','dev'" })], { env: ALL() });
v("var-custom-all-value-multi-some", ["var:custom-all-value"], "SELECT 1 WHERE e IN ($env)", [envMallValue], { env: many("prod", "dev") });

// Query variables: the options of the variable query are the rows.
const hosts = ["a1", "b2", "c3"];
const qM = query("host", hosts, { multi: true });
const qS = query("host", hosts);
v("var-query-single-string", ["var:query-single", "edge:free-text"], "SELECT 1 WHERE h = '$host'", [qS], { host: one("a1") });
v("var-query-single-code-text", ["var:query-single", "edge:free-text"], "SELECT 1 WHERE h = $host", [qS], { host: one("a1") });
v("var-query-single-code-number", ["var:query-single", "edge:numbers"], "SELECT 1 WHERE n = $host", [query("host", ["10", "20"])], { host: one("10") });
v("var-query-single-code-negative-number", ["var:query-single", "edge:numbers"], "SELECT 1 WHERE n = $host", [query("host", ["-10", "20"])], { host: one("-10") });
v("var-query-single-code-decimal", ["var:query-single", "edge:numbers"], "SELECT 1 WHERE n = $host", [query("host", ["1.5", "2"])], { host: one("1.5") });
v("var-query-single-code-quoted-by-format", ["var:query-single", "format:singlequote"], "SELECT 1 WHERE h = ${host:singlequote}", [qS], { host: one("a1") });
v("var-query-single-code-sqlstring", ["var:query-single", "format:sqlstring"], "SELECT 1 WHERE h = ${host:sqlstring}", [qS], { host: one("a1") });
v("var-query-single-code-csv", ["var:query-single", "format:csv"], "SELECT 1 WHERE h = ${host:csv}", [qS], { host: one("a1") });
v("var-query-single-code-raw", ["var:query-single", "format:raw"], "SELECT 1 WHERE h = ${host:raw}", [qS], { host: one("a1") });
v("var-query-single-empty-selection", ["var:query-single", "edge:empty-selection"], "SELECT 1 WHERE h IN ($host)", [query("host", [])], { host: { all: false, values: [] } });
v("var-query-single-empty-selection-string", ["var:query-single", "edge:empty-selection"], "SELECT 1 WHERE h = '$host'", [query("host", [])], { host: { all: false, values: [] } });
v("var-query-single-all", ["var:query-all"], "SELECT 1 WHERE h IN ($host)", [query("host", hosts, { all: true })], { host: ALL(hosts) });
v("var-query-single-all-value", ["var:query-all-value"], "SELECT 1 WHERE h IN ($host)", [query("host", hosts, { all: true, all_value: "'x'" })], { host: ALL() });
v("var-query-multi-default", ["var:query-multi"], "SELECT 1 WHERE h IN ($host)", [qM], { host: many("a1", "b2") });
v("var-query-multi-one-value", ["var:query-multi"], "SELECT 1 WHERE h IN ($host)", [qM], { host: many("a1") });
v("var-query-multi-in-string-csv", ["var:query-multi", "format:csv"], "SELECT 1 WHERE h = '${host:csv}'", [qM], { host: many("a1", "b2") });
for (const f of ["raw", "csv", "singlequote", "sqlstring"]) {
  v(`var-query-multi-format-${f}`, ["var:query-multi", `format:${f}`, "edge:free-text"], `SELECT 1 WHERE h IN (\${host:${f}})`, [qM], { host: many("a1", "b2") });
}
v("var-query-multi-numbers-csv", ["var:query-multi", "format:csv", "edge:numbers"], "SELECT 1 WHERE n IN (${n:csv})", [query("n", ["1", "2", "3"], { multi: true })], {
  n: many("1", "2"),
});
v("var-query-multi-numbers-raw", ["var:query-multi", "format:raw", "edge:numbers"], "SELECT 1 WHERE n IN (${n:raw})", [query("n", ["1", "2", "3"], { multi: true })], {
  n: many("1", "3"),
});
v("var-query-multi-empty-selection", ["var:query-multi", "edge:empty-selection"], "SELECT 1 WHERE h IN ($host)", [query("host", [], { multi: true })], { host: { all: false, values: [] } });
v("var-query-multi-empty-selection-csv", ["var:query-multi", "edge:empty-selection", "format:csv"], "SELECT 1 WHERE h IN (${host:csv})", [query("host", [], { multi: true })], {
  host: { all: false, values: [] },
});
v("var-query-multi-empty-selection-singlequote", ["var:query-multi", "edge:empty-selection", "format:singlequote"], "SELECT 1 WHERE h IN (${host:singlequote})", [query("host", [], { multi: true })], {
  host: { all: false, values: [] },
});
v("var-query-multi-empty-selection-sqlstring", ["var:query-multi", "edge:empty-selection", "format:sqlstring"], "SELECT 1 WHERE h IN (${host:sqlstring})", [query("host", [], { multi: true })], {
  host: { all: false, values: [] },
});
v("var-query-multi-empty-selection-raw", ["var:query-multi", "edge:empty-selection", "format:raw"], "SELECT 1 WHERE h IN (${host:raw})", [query("host", [], { multi: true })], {
  host: { all: false, values: [] },
});
v("var-query-multi-empty-selection-string", ["var:query-multi", "edge:empty-selection"], "SELECT 1 WHERE h = '$host'", [query("host", [], { multi: true })], { host: { all: false, values: [] } });
v("var-query-multi-all", ["var:query-all"], "SELECT 1 WHERE h IN ($host)", [query("host", hosts, { multi: true, all: true })], { host: ALL(hosts) });
v("var-query-multi-all-csv", ["var:query-all", "format:csv"], "SELECT 1 WHERE h IN (${host:csv})", [query("host", hosts, { multi: true, all: true })], { host: ALL(hosts) });
v("var-query-multi-all-singlequote", ["var:query-all", "format:singlequote"], "SELECT 1 WHERE h IN (${host:singlequote})", [query("host", hosts, { multi: true, all: true })], { host: ALL(hosts) });
v("var-query-multi-all-value", ["var:query-all-value"], "SELECT 1 WHERE h IN ($host)", [query("host", hosts, { multi: true, all: true, all_value: "'a1','b2','c3'" })], {
  host: ALL(),
});
v("var-query-multi-all-value-in-string", ["var:query-all-value"], "SELECT 1 WHERE match(h, '$host')", [query("host", hosts, { multi: true, all: true, all_value: ".+" })], {
  host: ALL(),
});
v("var-query-multi-all-value-some", ["var:query-all-value"], "SELECT 1 WHERE h IN ($host)", [query("host", hosts, { multi: true, all: true, all_value: ".+" })], {
  host: many("a1"),
});

// Text variables.
const t = text("t");
const tn = text("n", "number");
v("var-text-string", ["var:text"], "SELECT 1 WHERE s = '$t'", [t], { t: one("hello") });
v("var-text-code-text", ["var:text", "edge:free-text"], "SELECT 1 WHERE s = $t", [t], { t: one("hello") });
v("var-text-code-number", ["var:text", "edge:numbers"], "SELECT * FROM numbers(100) LIMIT $n", [tn], { n: one("25") });
v("var-text-code-negative-number", ["var:text", "edge:numbers"], "SELECT 1 WHERE x > $n", [tn], { n: one("-3") });
v("var-text-code-decimal", ["var:text", "edge:numbers"], "SELECT 1 WHERE x > $n", [tn], { n: one("0.25") });
v("var-text-empty-string", ["var:text", "edge:empty-value"], "SELECT 1 WHERE s = '$t'", [t], { t: one("") });
v("var-text-empty-code", ["var:text", "edge:empty-value"], "SELECT 1 WHERE s = $t", [t], { t: one("") });
v("var-text-format-csv", ["var:text", "format:csv"], "SELECT 1 WHERE s = '${t:csv}'", [t], { t: one("hello") });
v("var-text-format-raw", ["var:text", "format:raw"], "SELECT 1 WHERE s = '${t:raw}'", [t], { t: one("hello") });
v("var-text-format-singlequote", ["var:text", "format:singlequote"], "SELECT 1 WHERE s = ${t:singlequote}", [t], { t: one("hello") });
v("var-text-format-sqlstring", ["var:text", "format:sqlstring"], "SELECT 1 WHERE s = ${t:sqlstring}", [t], { t: one("hello") });
v("var-text-in-comment", ["var:text", "edge:comment"], "SELECT 1 /* $t */", [t], { t: one("hello") });

// Escapes: quote marks and backslashes in the values.
const q1 = text("t");
v("escape-text-quote-in-string", ["escape:quote"], "SELECT 1 WHERE s = '$t'", [q1], { t: one("a'b") });
v("escape-text-backslash-in-string", ["escape:backslash"], "SELECT 1 WHERE s = '$t'", [q1], { t: one("a\\b") });
v("escape-text-backslash-quote-in-string", ["escape:backslash", "escape:quote"], "SELECT 1 WHERE s = '$t'", [q1], { t: one("a\\'b") });
v("escape-text-double-quote-in-string", ["escape:double-quote"], "SELECT 1 WHERE s = '$t'", [q1], { t: one('a"b') });
v("escape-text-quote-closes-string", ["escape:quote", "edge:injection"], "SELECT 1 WHERE s = '$t'", [q1], { t: one("' OR 1=1 --") });
v("escape-text-comment-mark-in-string", ["escape:comment"], "SELECT 1 WHERE s = '$t' AND r = 1", [q1], { t: one("a -- b") });
v("escape-text-dollar-in-string", ["escape:dollar"], "SELECT 1 WHERE s = '$t'", [q1], { t: one("a$b") });
v("escape-text-macro-name-in-value", ["escape:macro-in-value", "edge:reread"], "SELECT 1 WHERE s = '$t'", [q1], { t: one("$__fromTime") });
v("escape-text-variable-name-in-value", ["escape:variable-in-value", "edge:reread"], "SELECT 1 WHERE s = '$t' AND r = '$u'", [q1, text("u")], { t: one("$u"), u: one("x") });
v("escape-text-quote-in-quoted-name", ["escape:quote"], 'SELECT "$t" FROM x', [q1], { t: one("a'b") });
v("escape-custom-single-quote-in-string", ["escape:quote"], "SELECT 1 WHERE e = '$env'", [custom("env", ["a'b", "c"])], { env: one("a'b") });
v("escape-custom-single-backslash-in-string", ["escape:backslash"], "SELECT 1 WHERE e = '$env'", [custom("env", ["a\\b", "c"])], { env: one("a\\b") });
v("escape-custom-single-quote-code", ["escape:quote"], "SELECT 1 WHERE e = $env", [custom("env", ["'a'", "'b'"])], { env: one("'a'") });
v("escape-query-multi-quote-default", ["escape:quote", "var:query-multi"], "SELECT 1 WHERE h IN ($host)", [query("host", ["a1", "b'2"], { multi: true })], {
  host: many("a1", "b'2"),
});
v("escape-query-multi-quote-singlequote", ["escape:quote", "format:singlequote"], "SELECT 1 WHERE h IN (${host:singlequote})", [query("host", ["a1", "b'2"], { multi: true })], {
  host: many("a1", "b'2"),
});
v("escape-query-multi-quote-sqlstring", ["escape:quote", "format:sqlstring"], "SELECT 1 WHERE h IN (${host:sqlstring})", [query("host", ["a1", "b'2"], { multi: true })], {
  host: many("a1", "b'2"),
});
v("escape-query-multi-quote-csv", ["escape:quote", "format:csv"], "SELECT 1 WHERE h IN (${host:csv})", [query("host", ["a1", "b'2"], { multi: true })], {
  host: many("a1", "b'2"),
});
v("escape-query-multi-backslash-default", ["escape:backslash", "var:query-multi"], "SELECT 1 WHERE h IN ($host)", [query("host", ["a1", "b\\2"], { multi: true })], {
  host: many("a1", "b\\2"),
});
v("escape-query-multi-backslash-singlequote", ["escape:backslash", "format:singlequote"], "SELECT 1 WHERE h IN (${host:singlequote})", [query("host", ["a1", "b\\2"], { multi: true })], {
  host: many("a1", "b\\2"),
});
v("escape-query-multi-backslash-sqlstring", ["escape:backslash", "format:sqlstring"], "SELECT 1 WHERE h IN (${host:sqlstring})", [query("host", ["a1", "b\\2"], { multi: true })], {
  host: many("a1", "b\\2"),
});
v("escape-query-multi-quote-all", ["escape:quote", "var:query-all"], "SELECT 1 WHERE h IN ($host)", [query("host", ["a1", "b'2"], { multi: true, all: true })], {
  host: ALL(["a1", "b'2"]),
});
v("escape-query-multi-quote-in-string-csv", ["escape:quote", "format:csv"], "SELECT 1 WHERE h = '${host:csv}'", [query("host", ["a1", "b'2"], { multi: true })], {
  host: many("a1", "b'2"),
});
v("escape-query-single-quote-string", ["escape:quote", "var:query-single"], "SELECT 1 WHERE h = '$host'", [query("host", ["a'b", "c"])], { host: one("a'b") });
v("escape-query-single-quote-singlequote", ["escape:quote", "format:singlequote"], "SELECT 1 WHERE h = ${host:singlequote}", [query("host", ["a'b", "c"])], { host: one("a'b") });
v("escape-query-single-quote-sqlstring", ["escape:quote", "format:sqlstring"], "SELECT 1 WHERE h = ${host:sqlstring}", [query("host", ["a'b", "c"])], { host: one("a'b") });

// Variables inside macro arguments and in the same text as macros.
v("var-in-macro-argument", ["var:custom-single", "macro:$__timeFilter"], "SELECT count() FROM analytics.events WHERE $__timeFilter($col)", [custom("col", ["timestamp"])], {
  col: one("timestamp"),
});
v("var-in-macro-argument-braces", ["var:custom-single", "macro:$__timeInterval"], "SELECT $__timeInterval(${col}) AS t FROM analytics.events GROUP BY t", [custom("col", ["timestamp"])], {
  col: one("timestamp"),
});
v(
  "var-with-macros-and-lists",
  ["var:query-multi", "var:custom-single", "macro:$__timeFilter", "macro:$__timeInterval"],
  "SELECT $__timeInterval(timestamp) AS t, count() FROM analytics.events WHERE $__timeFilter(timestamp) AND event.name IN ($host) AND system.region = '$env' GROUP BY t",
  [qM, envC],
  { host: many("a1", "b2"), env: one("prod") },
);
v("var-multi-line-template", ["var:custom-single", "edge:lines"], "SELECT\n  1\nFROM numbers(1)\nWHERE e = '$env'\n  AND x = 1", [envC], { env: one("dev") });
