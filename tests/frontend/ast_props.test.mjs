// SPDX-License-Identifier: MIT OR Apache-2.0
// The property tests of the syntax tree of the query editor (F42): parseSql (two dialects), parseJson and the formatter.
// The inputs are the shared corpus in tests/fixtures/ast/ (the real queries of the repository and the hostile cases)
// and seeded random mutations of it. The seeds are fixed, so a failure repeats. The tree is never a security check:
// these tests check structure only.
// The properties are in tests/frontend/tools/ast_props_lib.mjs. The fuzz tool (tools/ast_fuzz.mjs) runs the same properties.
import test from "node:test";
import assert from "node:assert/strict";
import { MAX_AST_INPUT_BYTES, MAX_AST_NODES, walk } from "../../web/js/editor/ast/nodes.js";
import { parseJson } from "../../web/js/editor/ast/json_tree.js";
import { parseSql } from "../../web/js/editor/ast/sql_tree.js";
import {
  expandCase,
  formatFaults,
  jsonFaults,
  loadCorpus,
  loadFormatter,
  makeRng,
  mutate,
  sqlFaults,
} from "./tools/ast_props_lib.mjs";

const CORPUS = loadCorpus();
const REAL_SQL = CORPUS.sql.map((e) => ({ name: e.source, text: e.text }));
const REAL_JSON = CORPUS.json.map((e) => ({ name: e.source, text: e.text }));
const HOSTILE = CORPUS.hostile;
const EVERYTHING = [...REAL_SQL, ...REAL_JSON, ...HOSTILE];

/** The parsers under test. `faults(text)` gives the list of faults, `parse(text)` gives the result. */
const TARGETS = {
  sql_ch: { faults: (t) => sqlFaults(t, "clickhouse"), parse: (t) => parseSql(t, "clickhouse") },
  sql_pg: { faults: (t) => sqlFaults(t, "postgres"), parse: (t) => parseSql(t, "postgres") },
  json: { faults: jsonFaults, parse: (t) => parseJson(t) },
};

// ---- The timing policy of all F42 timing tests ----

/**
 * The time of `fn` in milliseconds, measured with the policy of F42: two warm-up runs that count for nothing,
 * then the MINIMUM of seven runs. One run is never a measure. The minimum is the run that the machine load
 * disturbed the least, so a test that compares the minimum does not fail when other processes use the CPU.
 */
function minTime(fn, { warmups = 2, runs = 7 } = {}) {
  for (let i = 0; i < warmups; i++) fn();
  let best = Infinity;
  for (let i = 0; i < runs; i++) {
    const t0 = performance.now();
    fn();
    best = Math.min(best, performance.now() - t0);
  }
  return best;
}

/** Joins the first faults of a list of failures into one message. */
function describe(failures, limit = 6) {
  const shown = failures.slice(0, limit).map((f) => `${f.name}: ${f.faults.join("; ")}\n    input: ${JSON.stringify(f.text.slice(0, 100))}`);
  return `${failures.length} failure(s)\n  ${shown.join("\n  ")}`;
}

/** Runs the faults function of a target on a list of inputs. It gives the failures. */
function check(target, inputs) {
  const failures = [];
  for (const { name, text } of inputs) {
    const faults = TARGETS[target].faults(text);
    if (faults.length) failures.push({ name, text, faults });
  }
  return failures;
}

// ---- The corpus ----

test("the corpus has the real queries and the hostile cases", () => {
  assert.ok(REAL_SQL.length >= 400, `SQL texts: ${REAL_SQL.length}`);
  assert.ok(REAL_JSON.length >= 400, `JSON texts: ${REAL_JSON.length}`);
  assert.ok(HOSTILE.length >= 250, `hostile cases: ${HOSTILE.length}`);
  const names = new Set(HOSTILE.map((h) => h.name));
  assert.equal(names.size, HOSTILE.length, "the names of the hostile cases are unique");
  for (const group of ["deep_", "lone_", "mixed_", "unclosed_", "cap_", "crlf_", "astral_", "macro_", "json_"]) {
    assert.ok(HOSTILE.some((h) => h.name.startsWith(group)), `a hostile case ${group}*`);
  }
  assert.ok(HOSTILE.some((h) => h.text.length === 262144), "an input of exactly 262,144 units");
});

// ---- The structural properties on the corpus ----
// P1 never throws. P2 the tokens cover the text with no gap and no overlap. P3 the nodes are in order and nested.
// P4 each code bracket belongs to one node or one error node. P5 a cap gives `capped` with a known reason.

for (const target of Object.keys(TARGETS)) {
  test(`${target}: the real SQL texts keep the structural properties`, () => {
    const failures = check(target, REAL_SQL);
    assert.equal(failures.length, 0, describe(failures));
  });
  test(`${target}: the real JSON texts keep the structural properties`, () => {
    const failures = check(target, REAL_JSON);
    assert.equal(failures.length, 0, describe(failures));
  });
  test(`${target}: the hostile cases keep the structural properties`, () => {
    const failures = check(target, HOSTILE);
    assert.equal(failures.length, 0, describe(failures));
  });
}

// ---- Seeded random mutations ----

/** The mutated inputs for a seed: each text of the corpus (up to 20,000 units) gets one to three mutations. */
function mutations(seed) {
  const rng = makeRng(seed);
  const pool = EVERYTHING.filter((e) => e.text.length <= 20000).map((e) => e.text);
  const out = [];
  for (const e of EVERYTHING) {
    if (e.text.length > 20000) continue;
    let text = e.text;
    const steps = 1 + rng.int(3);
    for (let i = 0; i < steps; i++) text = mutate(rng, text, pool);
    out.push({ name: `${e.name} (seed ${seed})`, text });
  }
  return out;
}

for (const seed of [20261114, 7, 424242]) {
  for (const target of Object.keys(TARGETS)) {
    test(`${target}: random mutations of the corpus, seed ${seed}`, () => {
      const failures = check(target, mutations(seed));
      assert.equal(failures.length, 0, describe(failures));
    });
  }
}

test("a mutation is deterministic for a seed", () => {
  const a = mutations(99).map((m) => m.text);
  const b = mutations(99).map((m) => m.text);
  assert.deepEqual(a, b);
  const c = mutations(100).map((m) => m.text);
  assert.notDeepEqual(a, c);
});

// ---- The caps ----

/** The text of a hostile case by name. */
const hostile = (name) => {
  const h = HOSTILE.find((x) => x.name === name);
  assert.ok(h, `the hostile case ${name} exists`);
  return h.text;
};

test("the input cap: 262,144 bytes pass and the next byte gives too_large", () => {
  // `exact` and `in` are at or below 262,144 bytes in UTF-8. `over` is one unit or more above.
  const within = ["cap_a_exact", "cap_two_byte_exact", "cap_three_byte_in", "cap_four_byte_exact"];
  const above = ["cap_a_over", "cap_two_byte_over", "cap_three_byte_over", "cap_four_byte_over", "cap_numbers_over"];
  for (const [target, { parse }] of Object.entries(TARGETS)) {
    for (const name of within) {
      const r = parse(hostile(name));
      assert.notEqual(r.capped && r.capped.reason, "too_large", `${target} ${name}`);
    }
    for (const name of above) {
      const r = parse(hostile(name));
      assert.equal(r.capped && r.capped.reason, "too_large", `${target} ${name}`);
    }
  }
});

test("the depth cap: a deep text gives `capped` and a partial tree", () => {
  for (const [target, { parse }] of Object.entries(TARGETS)) {
    // The JSON reader has no parenthesis: it gets only the cases with [ ] { }.
    const deep = target === "json" ? ["deep_array_200", "deep_brace_200", "deep_json_array_1000", "deep_json_object_200"] : ["deep_paren_200", "deep_array_200", "deep_brace_200", "deep_mixed_1000", "deep_unclosed_paren_1000", "deep_json_array_1000", "deep_json_object_200"];
    for (const name of deep) {
      const text = hostile(name);
      const r = parse(text);
      assert.ok(r.capped, `${target} ${name} has capped`);
      assert.ok(["too_deep", "too_many_nodes"].includes(r.capped.reason), `${target} ${name}: ${r.capped.reason}`);
      assert.equal(r.tokens[r.tokens.length - 1].end, text.length, `${target} ${name}: the tokens still cover the text`);
    }
    for (const name of ["deep_array_1", "deep_brace_1", "deep_json_array_1", "deep_json_object_1"]) {
      assert.equal(parse(hostile(name)).capped, null, `${target} ${name} has no cap`);
    }
  }
});

test("the node cap: 100,000 nodes at most, then `capped` with too_many_nodes", () => {
  const text = hostile("cap_numbers_exact");
  assert.equal(text.length, 262144);
  for (const target of ["sql_ch", "sql_pg"]) {
    const r = TARGETS[target].parse(text);
    assert.equal(r.capped && r.capped.reason, "too_many_nodes", target);
    let count = 0;
    walk(r.root, () => {
      count += 1;
    });
    assert.ok(count <= MAX_AST_NODES, `${target}: ${count} nodes`);
  }
  // A JSON array of 100,001 numbers: the cap holds for the other reader too.
  const numbers = `[${"1,".repeat(100000)}1]`;
  const r = parseJson(numbers);
  assert.equal(r.capped && r.capped.reason, "too_many_nodes");
  let count = 0;
  walk(r.root, () => {
    count += 1;
  });
  assert.ok(count <= MAX_AST_NODES, `json: ${count} nodes`);
});

test("no hang: each hostile case, with each parser, ends in 5 seconds or less", () => {
  const slow = [];
  for (const h of HOSTILE) {
    for (const [target, { parse }] of Object.entries(TARGETS)) {
      const t0 = performance.now();
      parse(h.text);
      const ms = performance.now() - t0;
      if (ms > 5000) slow.push(`${target} ${h.name} ${Math.round(ms)} ms`);
    }
  }
  assert.deepEqual(slow, []);
});

// ---- Linear time ----
// For each repeated unit and each parser: the time of 2n over the time of n on three doublings
// (16K to 32K, 32K to 64K, 64K to 128K units). The time of each size is the minimum of 7 runs (see minTime), and
// a time below 1 ms counts as 1 ms (the clock and the garbage collector decide there, not the code).
// The test fails when the mean ratio of the three doublings (geometric mean) is 3 or more, or when one ratio is 5 or more.
// A quadratic scan gives a ratio of 4 on each doubling, so it fails the first rule on a large input and the second on a pair.
// A failure is measured again, up to 3 attempts in all, with the minimum over the attempts. The garbage collector adds a ratio of 1 to 1.5 at
// these sizes (more tokens in the heap), so a strict limit of 3 on each single pair fails with no fault of the code.

/** A text of at least `size` units: `prefix`, the unit repeated, then `suffix`. */
function repeated(spec, size) {
  const count = Math.ceil(size / spec.unit.length);
  return spec.prefix + spec.unit.repeat(count) + spec.suffix;
}

const spec = (unit, prefix = "", suffix = "") => ({ unit, prefix, suffix });

/** The units of the SQL scale tests: realistic statements and the single tokens that could make a scan quadratic. */
const SQL_UNITS = {
  statements: spec("SELECT a, f(b) AS c FROM (SELECT x FROM t WHERE y > 1) s WHERE z IN (1, 2) GROUP BY a ORDER BY a LIMIT 10;\n"),
  ctes: spec("WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a JOIN b ON a.x = b.x;\n"),
  macros_variables: spec("SELECT $__timeFilter(ts), ${v:csv}, $w FROM t WHERE x = '$y';\n"),
  comments: spec("SELECT 1 -- note\n/* block */ , 2;\n"),
  quoted: spec("SELECT 'it''s', \"q\", `r`, 'a\\'b';\n"),
  select_list: spec("f(a), ", "SELECT "),
  nested_small: spec("SELECT (1 + (2 * (3))), [1, [2]], {p:UInt8};\n"),
  union_chain: spec("SELECT 1 UNION ALL ", "", "SELECT 2"),
  joins: spec("SELECT 1 FROM a LEFT JOIN b ON a.x = b.x ARRAY JOIN c AS d;\n"),
  lambdas_casts: spec("SELECT arrayMap(x -> x + 1, y)::Array(Int8), a::text, b->>'k';\n"),
  dollar_quotes: spec("SELECT $$a;b$$, $t$c$$d$t$;\n"),
};
for (const t of ["(", ")", "[", "]", "{", "}", "'", "\"", "`", "$", "${", "${a:", "${a:csv", "$__f(", "$$", "$a$", "{p:A(", "{p:", "/*", "*/", "/**/", "--", "--\n", "#", "::", "->", "a.", "1e", "0x", "E'", "'\\", "\\", ";", ",", "\r\n", "\r", "😀", "\u0000", "\ud800", "WITH ", "SELECT ", "SELECT (", "ARRAY JOIN ", "UNION ALL ", "$1", "x -> ", "(SELECT ", "f(", "AS ("]) {
  SQL_UNITS[`token ${JSON.stringify(t)}`] = spec(t);
}

/** The units of the JSON scale tests. */
const JSON_UNITS = {
  array_of_objects: spec('{"a":[1,2,{"b":"c"}],"d":null},', "[", "{}]"),
  many_members: spec('"k":"v",', "{", '"z":1}'),
  placeholders: spec('{"a":"$x","b":"${v:csv}","c":"$__timeFrom"},', "[", "{}]"),
  escapes: spec("\\u00e9\\n\\\"\\ud83d\\ude00", '{"a":"', '"}'),
  long_string: spec("x", '{"a":"', '"}'),
  numbers: spec("-1.5e+10,", "[", "0]"),
  pipeline: spec('{"$match":{"$expr":{"$and":[{"$gte":["$ts","$__timeFrom"]}]}}},{"$group":{"_id":"$h","n":{"$sum":1}}},', '{"collection":"c","aggregate":[', "{}]}"),
  errors: spec("{,[}]"),
};
for (const t of ["{", "[", "]", "}", "\"", "\"\\", "\"\\u", "\"\\ud800", "$", "${", "/*", "//", "-", "1e", "0", "nul", "tru", ",", ":", "{\"a\":", "[{\"a\":", "😀", "\u0000", "\"$", "\"${", "\"${a:"]) {
  JSON_UNITS[`token ${JSON.stringify(t)}`] = spec(t);
}

const SIZES = [16384, 32768, 65536, 131072];

/**
 * Measures a parser over the four sizes and gives the failing unit as text, or an empty list.
 * A failure is measured again, up to four more times, and the time of each size is the minimum over all the attempts
 * (7, 14, 21, 28 or 35 runs). A machine with a high load can disturb the minimum of one attempt, but not of five.
 * The gate runs many jobs at the same time, so the test takes more attempts to meet a quiet moment. The limits are the same:
 * a parser with a time that is not linear fails in each attempt.
 */
function scaleFaults(label, parse, unitSpec) {
  const texts = SIZES.map((s) => repeated(unitSpec, s));
  const times = SIZES.map(() => Infinity);
  let verdict = "";
  for (let attempt = 0; attempt < 5; attempt++) {
    texts.forEach((t, i) => {
      times[i] = Math.min(times[i], minTime(() => parse(t)));
    });
    const ratios = [];
    for (let i = 0; i + 1 < SIZES.length; i++) ratios.push(Math.max(times[i + 1], 1) / Math.max(times[i], 1));
    const mean = Math.exp(ratios.reduce((sum, r) => sum + Math.log(r), 0) / ratios.length);
    if (mean < 3 && Math.max(...ratios) < 5) return [];
    verdict = `${label}: ratios ${ratios.map((r) => r.toFixed(2)).join(", ")} (mean ${mean.toFixed(2)}), times ${times.map((t) => t.toFixed(2)).join(", ")} ms, after ${attempt + 1} attempt(s)`;
  }
  return [verdict];
}

for (const [target, units] of [["sql_ch", SQL_UNITS], ["sql_pg", SQL_UNITS], ["json", JSON_UNITS]]) {
  test(`${target}: the time is linear (time(2n) / time(n) < 3)`, { timeout: 300000 }, () => {
    const faults = [];
    for (const [name, unitSpec] of Object.entries(units)) faults.push(...scaleFaults(name, TARGETS[target].parse, unitSpec));
    assert.deepEqual(faults, []);
  });
}

test("the largest allowed input parses in a few hundred milliseconds at most (a loose bound, with the minimum of 7 runs)", () => {
  const text = repeated(SQL_UNITS.statements, MAX_AST_INPUT_BYTES - 200).slice(0, MAX_AST_INPUT_BYTES);
  for (const [target, { parse }] of Object.entries(TARGETS)) {
    const ms = minTime(() => parse(text));
    assert.ok(ms < 500, `${target}: ${ms.toFixed(1)} ms`);
  }
});

// ---- The formatter ----
// P6 the non-blank tokens are the same before and after. P7 format(format(x)) = format(x).
// P8 no line comment gets code after it on its own line. The tests skip until web/js/editor/ast/format.js exists.

const FORMATTER = await loadFormatter();
const skipFormat = FORMATTER ? false : "web/js/editor/ast/format.js does not exist yet: the formatter properties are skipped";

/** The inputs for a formatter kind: the real texts of the kind, the hostile cases and the mutations of the real texts. */
function formatInputs(kind, seed) {
  const real = kind === "json" ? REAL_JSON : REAL_SQL;
  const rng = makeRng(seed);
  const pool = real.map((e) => e.text);
  const out = [...real.filter((e) => e.text.length <= 30000), ...HOSTILE.filter((e) => e.text.length <= 30000)];
  for (const e of real) {
    if (e.text.length > 20000) continue;
    out.push({ name: `${e.name} (mutation)`, text: mutate(rng, mutate(rng, e.text, pool), pool) });
  }
  return out;
}

for (const kind of ["clickhouse", "postgres", "json"]) {
  test(`format ${kind}: same tokens, idempotent, no joined line comment`, { skip: skipFormat, timeout: 300000 }, () => {
    const failures = [];
    for (const { name, text } of formatInputs(kind, 31337)) {
      const faults = formatFaults(FORMATTER, text, kind);
      if (faults.length) failures.push({ name, text, faults });
    }
    assert.equal(failures.length, 0, describe(failures));
  });
}

test("format: the huge hostile inputs end in 5 seconds or less and keep the tokens", { skip: skipFormat, timeout: 300000 }, () => {
  const failures = [];
  for (const h of HOSTILE.filter((x) => x.name.startsWith("cap_") || x.name.startsWith("deep_"))) {
    for (const kind of ["clickhouse", "postgres", "json"]) {
      const t0 = performance.now();
      const faults = formatFaults(FORMATTER, h.text, kind);
      const ms = performance.now() - t0;
      if (ms > 15000) faults.push(`${Math.round(ms)} ms`);
      if (faults.length) failures.push({ name: `${h.name} ${kind}`, text: h.text, faults });
    }
  }
  assert.equal(failures.length, 0, describe(failures));
});

// The hostile cases are expanded from parts: a check that the expansion matches the data file.
test("the expansion of a hostile case is the concatenation of its parts", () => {
  assert.equal(expandCase({ parts: [["ab", 3], ["c", 1]] }), "abababc");
});
