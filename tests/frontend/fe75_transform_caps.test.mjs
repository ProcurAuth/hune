// FE-75: the caps and the time of the 11 more transforms (9.21a, rules 13 and 15; 4.35; T85).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { availableParallelism, loadavg } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { MAX_MERGE_STEPS, MAX_TRANSFORM_FIELDS, MAX_TRANSFORM_FRAMES, MAX_TRANSFORM_ROWS, MORE_KINDS, applyMore, checkShape } from "../../web/js/transform_more.js";
import { applyList } from "../../web/js/transform.js";
import { best, fieldOf } from "./more_helpers.mjs";
// The cap of the cells is the key `server.transform_max_cells`: the session gives it to the page (6.10). The default of the server here.
import { BOUND_CELLS, DEFAULT_CELLS as MAX_TRANSFORM_CELLS, setCells, withCells } from "./session_cells.mjs";
import { app } from "../../web/js/config.js";

const num = (name, n, f) => fieldOf({ name, kind: "number", values: Array.from({ length: n }, (_, i) => f(i)) });
const txt = (name, n, f) => fieldOf({ name, kind: "text", values: Array.from({ length: n }, (_, i) => f(i)) });
const limitAll = { kind: "limit", count: 1000000, end: "first" };

test("FE-75 the caps equal the table of 4.35 and the limits of the server", () => {
  const doc = readFileSync(new URL("../../docs/02-format-and-storage.md", import.meta.url), "utf8");
  const rust = readFileSync(new URL("../../crates/hune-core/src/limits.rs", import.meta.url), "utf8");
  const caps = { MAX_TRANSFORM_ROWS, MAX_TRANSFORM_FIELDS, MAX_TRANSFORM_FRAMES, MAX_MERGE_STEPS };
  for (const [name, value] of Object.entries(caps)) {
    const row = new RegExp(`\\| \`${name}\` \\| ([0-9,]+) \\|`).exec(doc);
    assert.ok(row, `${name} is in the table of 4.35`);
    assert.equal(Number(row[1].replaceAll(",", "")), value, name);
    const rs = new RegExp(`pub const ${name}: usize = ([0-9_]+);`).exec(rust);
    assert.ok(rs, `${name} is in limits.rs`);
    assert.equal(Number(rs[1].replaceAll("_", "")), value, `${name} in limits.rs`);
  }
  // MAX_TRANSFORM_CELLS is the key `server.transform_max_cells` (owner order of 2026-11-16). The row of 4.35 gives the key, its
  // default and its bound, and the key table of the server and the table of 13.8 hold the same two numbers. limits.rs does not hold it.
  const cells = /\| `MAX_TRANSFORM_CELLS` \| the configured value \| [^\n]*`server\.transform_max_cells` \(13\.8\): the default is ([0-9,]+)\. A file can set it from 1 to ([0-9,]+)\./.exec(doc);
  assert.ok(cells, "MAX_TRANSFORM_CELLS is in the table of 4.35, with the key, its default and its bound");
  assert.deepEqual([Number(cells[1].replaceAll(",", "")), Number(cells[2].replaceAll(",", ""))], [10000000, 100000000]);
  assert.deepEqual([MAX_TRANSFORM_CELLS, BOUND_CELLS], [10000000, 100000000]);
  const ops = readFileSync(new URL("../../docs/06-configuration-operation.md", import.meta.url), "utf8");
  assert.match(ops, /\| `server\.transform_max_cells` \| Integer, 1 to 100000000\. [^\n]*\| no \| V \| optional, default `10000000` \| never \| `10000000` \|/);
  assert.doesNotMatch(rust, /TRANSFORM_CELLS/);
});

test("FE-75 the page keeps no copy of the cap of the cells: no file of web/js holds 10,000,000 outside a comment", () => {
  const files = [];
  const walk = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (e.isDirectory()) walk(join(dir, e.name));
      else if (e.name.endsWith(".js")) files.push(join(dir, e.name));
    }
  };
  walk(fileURLToPath(new URL("../../web/js", import.meta.url)));
  assert.ok(files.length > 100, `${files.length} files`);
  const copy = /(?<![\w.])(?:10[,_]?000[,_]?000|1e7)(?![\w.])/i;
  for (const f of files) {
    const code = readFileSync(f, "utf8").replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
    assert.doesNotMatch(code, copy, f);
  }
  // The test sees a copy: the forms of the number in code, and not the bound or a longer number.
  for (const text of ["const MAX = 10000000;", "cap(n, 10_000_000)", '"10,000,000 at most"', "x < 1e7"]) assert.match(text, copy, text);
  for (const text of ["100000000", "10000001", "110000000", "// 10000000", "0.10000000"]) assert.doesNotMatch(text.replace(/\/\/.*$/gm, ""), copy, text);
});

test("FE-75 the 11 kinds are the kinds of 4.25a, in the order of their sets", () => {
  assert.deepEqual(MORE_KINDS, ["organize", "calculate_field", "sort_by", "limit", "reduce", "filter_fields_by_name", "rename_by_regex", "convert_field_type", "partition_by_values", "merge", "join_by_field"]);
});

test("FE-75 MAX_TRANSFORM_ROWS: the bound passes and one above is an error", () => {
  const at = [num("x", MAX_TRANSFORM_ROWS, (i) => i)];
  assert.equal(applyMore([at], limitAll)[0][0].values.length, MAX_TRANSFORM_ROWS);
  const above = [num("x", MAX_TRANSFORM_ROWS + 1, (i) => i)];
  assert.throws(() => applyMore([above], limitAll), /The transform limit failed: 200001 rows, 200,000 at most/);
  assert.throws(() => applyMore([above], { kind: "sort_by", sort: [{ field: "x", desc: false }] }), /rows, 200,000 at most/);
});

test("FE-75 MAX_TRANSFORM_ROWS: merge and join_by_field count the rows of all frames", () => {
  const half = MAX_TRANSFORM_ROWS / 2;
  const mk = (k, n, v) => [num("k", n, (i) => i), num(v, n, (i) => i)];
  assert.equal(applyMore([mk("k", half, "a"), mk("k", half, "b")], { kind: "join_by_field", field: "k", mode: "inner" })[0][0].values.length, half);
  assert.throws(() => applyMore([mk("k", half, "a"), mk("k", half + 1, "b")], { kind: "join_by_field", field: "k", mode: "outer" }), /The transform join_by_field failed: 200001 rows, 200,000 at most/);
  assert.throws(() => applyMore([mk("k", half, "a"), mk("k", half + 1, "b")], { kind: "merge" }), /The transform merge failed: 200001 rows, 200,000 at most/);
});

test("FE-75 join_by_field inner: the product is checked before a row is built", () => {
  const same = (v, n) => [num("k", n, () => 1), num(v, n, (i) => i)];
  assert.throws(() => applyMore([same("a", 1000), same("b", 1000)], { kind: "join_by_field", field: "k", mode: "inner" }), /The transform join_by_field failed: [0-9]+ rows, 200,000 at most/);
  // 447 x 447 = 199,809 rows pass. The check runs for each row of the first frame, before the rows of that step are built.
  assert.equal(applyMore([same("a", 447), same("b", 447)], { kind: "join_by_field", field: "k", mode: "inner" })[0][0].values.length, 199809);
  assert.throws(() => applyMore([same("a", 448), same("b", 448)], { kind: "join_by_field", field: "k", mode: "inner" }), /rows, 200,000 at most/);
});

test("FE-75 MAX_TRANSFORM_FIELDS: the bound passes and one above is an error", () => {
  const frame = (n) => Array.from({ length: n }, (_, i) => num(`f${i}`, 1, () => i));
  assert.equal(applyMore([frame(MAX_TRANSFORM_FIELDS)], limitAll)[0].length, MAX_TRANSFORM_FIELDS);
  assert.throws(() => applyMore([frame(MAX_TRANSFORM_FIELDS + 1)], limitAll), /The transform limit failed: 2001 fields, 2,000 at most/);
  // reduce_fields makes one field for each field and reducer.
  assert.throws(() => applyMore([frame(1000)], { kind: "reduce", mode: "reduce_fields", reducers: ["min", "max", "mean"] }), /The transform reduce failed: 3000 fields, 2,000 at most/);
});

test("FE-75 MAX_TRANSFORM_FRAMES: the bound passes and one above is an error", () => {
  const split = (n) => [[txt("h", n, (i) => `h${i}`), num("v", n, (i) => i)]];
  assert.equal(applyMore(split(MAX_TRANSFORM_FRAMES), { kind: "partition_by_values", fields: ["h"], keep_fields: false }).length, MAX_TRANSFORM_FRAMES);
  assert.throws(() => applyMore(split(MAX_TRANSFORM_FRAMES + 1), { kind: "partition_by_values", fields: ["h"], keep_fields: false }), /The transform partition_by_values failed: 101 frames, 100 at most/);
  const list = (n) => Array.from({ length: n }, () => [num("x", 1, () => 1)]);
  assert.equal(applyMore(list(MAX_TRANSFORM_FRAMES), limitAll).length, MAX_TRANSFORM_FRAMES);
  assert.throws(() => applyMore(list(MAX_TRANSFORM_FRAMES + 1), limitAll), /The transform limit failed: 101 frames, 100 at most/);
});

// Steps of a merge: the row i of a group of equal keys with distinct values compares with the i rows before it. A group of g rows makes
// g(g-1)/2 compares. A compare of two rows is one step for each field of the result: here 3 fields (k, x and y).
function stepFrames(groups) {
  const keys = [];
  const vals = [];
  groups.forEach((g, gi) => {
    for (let i = 0; i < g; i++) {
      keys.push(`g${gi}`);
      vals.push(i);
    }
  });
  const other = [txt("k", 1, () => "z"), num("y", 1, () => 0)];
  return [[fieldOf({ name: "k", kind: "text", values: keys }), fieldOf({ name: "x", kind: "number", values: vals })], other];
}

test("FE-75 MAX_MERGE_STEPS: 3,999,999 steps pass and 4,000,002 are an error (3 fields: 3 steps for each compare)", () => {
  // 1633 x 1632 / 2 + 40 x 39 / 2 + 7 x 6 / 2 + 3 + 1 = 1,332,528 + 780 + 21 + 3 + 1 = 1,333,333 compares, 3,999,999 steps.
  // A group of 2 more rows adds 1 compare: 4,000,002 steps.
  const groups = [1633, 40, 7, 3, 2];
  const at = applyMore(stepFrames(groups), { kind: "merge" });
  assert.equal(at[0][0].values.length, 1633 + 40 + 7 + 3 + 2 + 1);
  assert.throws(() => applyMore(stepFrames([...groups, 2]), { kind: "merge" }), /The transform merge failed: too many comparisons; add a field that all frames share/);
});

// ---- the caps before a step builds a frame (review v3, finding P2, 2026-11-16) -------------------------------------------

const best7 = (fn) => {
  fn();
  fn();
  let min = Infinity;
  for (let i = 0; i < 7; i++) {
    const t0 = performance.now();
    fn();
    min = Math.min(min, performance.now() - t0);
  }
  return min;
};

// A query of R rows: time, host (100 hosts, each row has its own time) and F value fields. partition_by_values by host gives 100 frames.
function hostFrames(F, R) {
  const time = fieldOf({ name: "time", kind: "time", values: Array.from({ length: R }, (_, i) => i) });
  const host = txt("host", R, (i) => `h${i % 100}`);
  const values = Array.from({ length: F }, (_, c) => num(`v${c}`, R, () => c));
  return [{ ref: "A", fields: [time, host, ...values] }];
}
const partition = { kind: "partition_by_values", fields: ["host"], keep_fields: false };
const outerJoin = { kind: "join_by_field", field: null, mode: "outer" };

test("FE-75 checkShape: 2,000 fields and 10,000,000 cells pass, one more is an error", () => {
  assert.equal(MAX_TRANSFORM_CELLS, 10000000);
  assert.doesNotThrow(() => checkShape("The step failed", MAX_TRANSFORM_FIELDS, MAX_TRANSFORM_CELLS / MAX_TRANSFORM_FIELDS));
  assert.doesNotThrow(() => checkShape("The step failed", 1, MAX_TRANSFORM_CELLS));
  assert.throws(() => checkShape("The step failed", MAX_TRANSFORM_FIELDS + 1, 0), { message: "The step failed: 2001 fields, 2,000 at most" });
  assert.throws(() => checkShape("The step failed", 1, MAX_TRANSFORM_CELLS + 1), { message: "The step failed: 10000001 cells, 10,000,000 at most" });
});

test("FE-75 checkShape fails closed: no session, no member, or a member that is not a positive whole number", () => {
  const closed = (lead) => ({ message: `${lead}: the session has no valid transform_max_cells` });
  const pair = () => [[num("k", 2, (i) => i), num("a", 2, (i) => i)], [num("k", 2, (i) => i + 1), num("b", 2, (i) => i)]];
  const check = (label) => {
    // Each step that makes cells fails, also a step of no cell: the page never guesses a cap.
    assert.throws(() => checkShape("The step failed", 1, 0), closed("The step failed"), label);
    assert.throws(() => checkShape("The step failed", MAX_TRANSFORM_FIELDS + 1, 0), closed("The step failed"), label);
    assert.throws(() => applyMore(pair(), { kind: "join_by_field", field: "k", mode: "outer" }), closed("The transform join_by_field failed"), label);
    assert.throws(() => applyMore(pair(), { kind: "merge" }), closed("The transform merge failed"), label);
    // A step that makes no new cell does not read the cap.
    assert.equal(applyMore(pair(), limitAll).length, 2);
  };
  // No session (the answer of GET api/session is not there), or a session that is not an object.
  try {
    for (const session of [null, undefined, "text", 5]) {
      app.session = session;
      check(String(session));
    }
  } finally {
    setCells(MAX_TRANSFORM_CELLS);
  }
  // A session with no member, or with a member that is not a positive whole number. `undefined` gives a session with no member.
  for (const bad of [undefined, null, 0, -1, 2.5, "10000000", "1e7", NaN, Infinity, true, [5], { n: 5 }, 2 ** 53]) withCells(bad, () => check(String(bad)));
  assert.doesNotThrow(() => checkShape("The step failed", 1, 0));
});

test("FE-75 a configured value applies as it is: 1,000 cells, and the bound 100,000,000", () => {
  withCells(1000, () => {
    assert.doesNotThrow(() => checkShape("The step failed", 1, 1000));
    assert.throws(() => checkShape("The step failed", 1, 1001), { message: "The step failed: 1001 cells, 1,000 at most" });
    // A join of 2 frames of 300 distinct keys: 3 fields x 600 rows = 1,800 cells.
    const frame = (from, tag) => [num("k", 300, (i) => from + i), num(tag, 300, (i) => i)];
    assert.throws(() => applyMore([frame(0, "a"), frame(300, "b")], { kind: "join_by_field", field: "k", mode: "outer" }), { message: "The transform join_by_field failed: 1800 cells, 1,000 at most" });
  });
  withCells(BOUND_CELLS, () => {
    assert.doesNotThrow(() => checkShape("The step failed", 1, MAX_TRANSFORM_CELLS + 1));
    assert.doesNotThrow(() => checkShape("The step failed", MAX_TRANSFORM_FIELDS, BOUND_CELLS / MAX_TRANSFORM_FIELDS));
    assert.throws(() => checkShape("The step failed", 1, BOUND_CELLS + 1), { message: "The step failed: 100000001 cells, 100,000,000 at most" });
  });
  // The default comes back after each run.
  assert.throws(() => checkShape("The step failed", 1, MAX_TRANSFORM_CELLS + 1), { message: "The step failed: 10000001 cells, 10,000,000 at most" });
});

test("FE-75 a step that only moves fields has no cap of cells: organize passes 200,000 rows x 60 fields (12,000,000 cells)", () => {
  // The 60 fields share one array of values: organize moves the fields and copies no cell.
  const one = num("x", MAX_TRANSFORM_ROWS, (i) => i);
  const frame = Array.from({ length: 60 }, (_, c) => ({ ...one, name: `f${c}` }));
  const out = applyMore([frame], { kind: "organize", hide: ["f0"], order: ["f59"], rename: [] });
  assert.equal(out[0].length * out[0][0].values.length, 59 * MAX_TRANSFORM_ROWS);
  assert.ok(60 * MAX_TRANSFORM_ROWS > MAX_TRANSFORM_CELLS);
});

test("FE-75 join_by_field: 10,000,000 cells pass, one more row is an error before a field is built", () => {
  // Two frames with distinct keys: 100 fields (the key, 49 and 50 value fields) and one output row for each input row.
  const frame = (from, n, F, tag) => [num("k", n, (i) => from + i), ...Array.from({ length: F }, (_, c) => num(`${tag}${c}`, n, () => c))];
  const out = applyMore([frame(0, 50000, 49, "a"), frame(50000, 50000, 50, "b")], { kind: "join_by_field", field: "k", mode: "outer" });
  assert.equal(out[0].length * out[0][0].values.length, MAX_TRANSFORM_CELLS);
  assert.throws(() => applyMore([frame(0, 50000, 49, "a"), frame(50000, 50001, 50, "b")], { kind: "join_by_field", field: "k", mode: "outer" }), { message: "The transform join_by_field failed: 10000100 cells, 10,000,000 at most" });
});

test("FE-75 partition and join_by_field, 25 value fields and 40,000 rows: an error in 300 ms or less, under 50 MB of new buffers", () => {
  const lists = hostFrames(25, 40000);
  const run = () => assert.throws(() => applyList(lists, [partition, outerJoin]), { message: "The transform join_by_field failed: 2501 fields, 2,000 at most" });
  const before = process.memoryUsage().arrayBuffers;
  run();
  const grew = (process.memoryUsage().arrayBuffers - before) / 1e6;
  assert.ok(grew < 50, `${grew.toFixed(0)} MB`);
  const ms = best7(run);
  assert.ok(ms <= 300, `${ms.toFixed(0)} ms`);
});

test("FE-75 partition and join_by_field, 19 value fields and 100,000 rows: the cells are an error in 300 ms or less", () => {
  const lists = hostFrames(19, 100000);
  const run = () => assert.throws(() => applyList(lists, [partition, outerJoin]), { message: "The transform join_by_field failed: 190100000 cells, 10,000,000 at most" });
  const ms = best7(run);
  assert.ok(ms <= 300, `${ms.toFixed(0)} ms`);
});

test("FE-75 join_by_field inner: the cells are checked before a row is built", () => {
  // 447 x 447 equal keys give 199,809 rows, with 1 + 10 + 10 fields: 4,195,989 cells pass. With 30 value fields each, 61 fields are too many cells.
  const same = (v, n, F) => [num("k", n, () => 1), ...Array.from({ length: F }, (_, c) => num(`${v}${c}`, n, (i) => i))];
  assert.equal(applyMore([same("a", 447, 10), same("b", 447, 10)], { kind: "join_by_field", field: "k", mode: "inner" })[0][0].values.length, 199809);
  assert.throws(() => applyMore([same("a", 447, 30), same("b", 447, 30)], { kind: "join_by_field", field: "k", mode: "inner" }), { message: "The transform join_by_field failed: 12188349 cells, 10,000,000 at most" });
});

test("FE-75 merge with 2,000 fields: equal keys give an error in 300 ms or less", () => {
  // One shared key that is 0 everywhere. The rows of the first frame conflict in their last field only, so a compare reads all the fields.
  const R = 2800;
  const A = [num("k", R, () => 0), ...Array.from({ length: 999 }, (_, c) => num(`a${c}`, R, (i) => (c === 998 ? i : 0)))];
  const B = [num("k", 1, () => 0), ...Array.from({ length: 1000 }, (_, c) => num(`b${c}`, 1, () => 1))];
  const run = () => assert.throws(() => applyMore([A, B], { kind: "merge" }), /The transform merge failed: too many comparisons/);
  const ms = best7(run);
  assert.ok(ms <= 300, `${ms.toFixed(0)} ms`);
});

test("FE-75 merge: more than 10,000,000 cells (fields times the rows of all frames) are an error before the first row", () => {
  // All 2,000 fields are in both frames, so each row has its own key: 2,000 fields x 5,600 rows.
  const frame = (off) => Array.from({ length: 2000 }, (_, c) => num(`c${c}`, 2800, (i) => (c === 1999 ? off + i : 0)));
  const frames = [frame(0), frame(1e6)];
  const run = () => assert.throws(() => applyMore(frames, { kind: "merge" }), { message: "The transform merge failed: 11200000 cells, 10,000,000 at most" });
  const ms = best7(run);
  assert.ok(ms <= 300, `${ms.toFixed(0)} ms`);
});

// 9.21a, rule 15: the test MUST run on a machine with a low load. A load of more than half of the cores skips it (the worker measured 520 ms to 960 ms
// at a load average of 29). The orchestrator runs the whole suite again on a quiet machine.
const busy = loadavg()[0] > availableParallelism() / 2;
test("FE-75 time: each step takes 300 ms or less for 200,000 rows (the best of 7 runs; run on a machine with a low load)", { skip: busy ? `load average ${loadavg()[0].toFixed(1)}: run again on a quiet machine` : false }, () => {
  const N = 100000;
  const A = [num("k", N, (i) => i), num("a", N, (i) => i * 2)];
  const B = [num("k", N, (i) => N - i), num("b", N, (i) => i * 3)];
  const S1 = [txt("k", N, (i) => `k${i}`), num("a", N, (i) => i)];
  const S2 = [txt("k", N, (i) => `k${N - i}`), num("b", N, (i) => i)];
  const big = 200000;
  // The inputs are made before the timer starts: the test measures the transform only.
  const five = [0, 1, 2, 3, 4].map((j) => num(`f${j}`, big, (i) => i + j));
  const sortIn = [num("x", big, (i) => (i * 7919) % 200003)];
  const textIn = [txt("s", big, (i) => String(i))];
  const xy = [num("x", big, (i) => i), num("y", big, (i) => 2 * i)];
  const hosts = [txt("h", big, (i) => `h${i % 100}`), num("v", big, (i) => i)];
  const calc = (reduce) => ({ kind: "calculate_field", mode: reduce ? "reduce_row" : "binary", alias: null, replace_fields: false, ...(reduce ? { reduce: { reducer: "mean", include: [] } } : { binary: { left: { kind: "field", field: "x" }, right: { kind: "field", field: "y" }, operator: "add" } }) });
  const steps = {
    "join outer": () => applyMore([A, B], { kind: "join_by_field", field: "k", mode: "outer" }),
    "join inner": () => applyMore([A, B], { kind: "join_by_field", field: "k", mode: "inner" }),
    "join outer text": () => applyMore([S1, S2], { kind: "join_by_field", field: "k", mode: "outer" }),
    "merge": () => applyMore([S1, S2], { kind: "merge" }),
    "sort_by": () => applyMore([sortIn], { kind: "sort_by", sort: [{ field: "x", desc: false }] }),
    "convert text to number": () => applyMore([textIn], { kind: "convert_field_type", conversions: [{ field: "s", to: "number" }] }),
    "calculate binary": () => applyMore([xy], calc(false)),
    "calculate reduce_row": () => applyMore([five], calc(true)),
    "reduce": () => applyMore([five], { kind: "reduce", mode: "series_to_rows", reducers: ["min", "max", "mean", "total", "last"] }),
    "partition": () => applyMore([hosts], { kind: "partition_by_values", fields: ["h"], keep_fields: false }),
  };
  for (const [name, fn] of Object.entries(steps)) {
    const { ms } = best(fn);
    assert.ok(ms <= 300, `${name}: ${ms.toFixed(0)} ms`);
  }
});
