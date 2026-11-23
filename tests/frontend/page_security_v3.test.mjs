// SPDX-License-Identifier: MIT OR Apache-2.0
// Review v3 of the page code (findings P1 and N2 of 2026-11-16): the number patterns and the decimal compare that run on data.
// Each test failed (or ran for many seconds) before the fix. The timing tests take the minimum of 7 runs after 2 warm-ups.
import test from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { buildHeatmap, nameNumber } from "../../web/js/heat_model.js";
import { displayNames } from "../../web/js/names.js";
import { distinctTexts, rowOrder } from "../../web/js/table_model.js";
import { longToWide } from "../../web/js/transform.js";
import { applyMore } from "../../web/js/transform_more.js";
// A namespace import: the file also loads on the code before the fix (E29 added two exports), so each test can fail alone.
import * as sort from "../../web/js/vendor/chdash/app_results_sort.js";
import { NUMERIC_RE } from "../../web/js/vendor/chdash/app_results_values.js";
// The session gives the cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
import "./session_cells.mjs";

const JS = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "web", "js");

const best = (fn) => {
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

// One cell of 40,001 characters: 40,000 digits and an "x". The old patterns tried each split of the digits: about 1 s to 3 s.
const HOSTILE = `${"1".repeat(40000)}x`;

test("P1: convert_field_type gives null for a cell of 40,000 digits and an x, in 50 ms or less", () => {
  const field = { name: "code", type: "string", kind: "text", values: [HOSTILE, "12.5"], labels: null, config: {} };
  const step = { kind: "convert_field_type", conversions: [{ field: "code", to: "number" }] };
  const [[out]] = applyMore([[field]], step);
  assert.equal(out.nulls[0], 1);
  assert.equal(out.values[1], 12.5);
  const ms = best(() => applyMore([[field]], step));
  assert.ok(ms <= 50, `${ms.toFixed(1)} ms`);
});

test("P1: a heatmap row name of 40,000 digits and an x is not a number, and the model takes 50 ms or less", () => {
  assert.ok(Number.isNaN(nameNumber(HOSTILE)));
  const R = 4;
  const col = (name, kind, values) => ({ name, type: kind, kind, values, nulls: kind === "text" ? undefined : new Uint8Array(R), labels: null, config: {} });
  const long = [col("time", "time", Float64Array.of(1, 2, 1, 2)), col("le", "text", [HOSTILE, HOSTILE, "10", "10"]), col("count", "number", Float64Array.of(1, 2, 3, 4))];
  const wide = longToWide(long);
  const args = { fields: wide, names: displayNames(wide), options: { calculate: false, x_buckets: {}, y_buckets: {} }, range: { from: 0, to: 3 } };
  const model = buildHeatmap(args);
  assert.equal(model.shape, "bucketed");
  assert.deepEqual(model.labels.map((l) => l.length), [2, 40001]);
  const ms = best(() => buildHeatmap(args));
  assert.ok(ms <= 50, `${ms.toFixed(1)} ms`);
});

test("P1: the filter list of a table column with a cell of 40,000 digits and an x sorts as text, in 50 ms or less", () => {
  const texts = [HOSTILE, "2", "1"];
  const at = (fi, row) => texts[row];
  assert.deepEqual(distinctTexts(3, 0, at), ["1", HOSTILE, "2"]);
  assert.equal(NUMERIC_RE.test(HOSTILE), false);
  const ms = best(() => distinctTexts(3, 0, at));
  assert.ok(ms <= 50, `${ms.toFixed(1)} ms`);
});

test("P1: NUMERIC_RE accepts the same texts as the old form", () => {
  const OLD = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/;
  const alphabet = "1.e+x";
  const all = (n, p = "") => (n === 0 ? [p] : [p, ...[...alphabet].flatMap((c) => all(n - 1, p + c))]);
  for (const s of [...all(6), "12", "-3.5", ".5", "5.", "1e10", "+1E-3", "1.2.3", "e5", "--1", " 1"]) assert.equal(NUMERIC_RE.test(s), OLD.test(s), s);
});

// The shapes that split one run of digits in many ways: `\d+\.?\d*`, `[0-9]*\.?[0-9]+`, `\d+\d*`. With a long run of digits and a
// last character that does not match, the time of the regular expression grows with the square of the length (findings P1, N2).
const QUADRATIC = /(?:\\{1,2}d|\[0-9\])[+*](?:\\{1,2}\.\?)?(?:\\{1,2}d|\[0-9\])[+*]/;

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir).sort()) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (name.endsWith(".js")) out.push(full);
  }
  return out;
}

test("P1: no file of web/js holds a number pattern of a quadratic shape", () => {
  for (const bad of [String.raw`/^\d+\.?\d*$/`, String.raw`/^[0-9]*\.?[0-9]+$/`, String.raw`"\\d+\\.?\\d*"`, String.raw`/\d+\d*/`]) assert.ok(QUADRATIC.test(bad), bad);
  for (const good of [String.raw`/^\d+(?:\.\d*)?$/`, String.raw`/^[0-9]+\.[0-9]+$/`, String.raw`/^[+-]?\d*(?:\.\d*)?$/`]) assert.ok(!QUADRATIC.test(good), good);
  const found = [];
  for (const f of walk(JS)) {
    readFileSync(f, "utf8").split("\n").forEach((line, i) => {
      if (QUADRATIC.test(line)) found.push(`${relative(JS, f)}:${i + 1}`);
    });
  }
  assert.deepEqual(found, []);
});

// N2: a PostgreSQL numeric text has a scale of up to 16,383 digits (decode.rs). 16 significant digits after the zeros make the sort
// compare the texts exactly. The old pattern took about 0.9 s for one compare of two such texts.
const numeric = (k) => `0.${"0".repeat(16383 - 16)}${String(1234567890123456 + k)}`;

test("N2: one compare of two PostgreSQL numeric texts of 16,385 characters takes 5 ms or less, with the exact order", () => {
  assert.equal(sort.compareNumericForSort(numeric(1), numeric(2)), -1);
  assert.equal(sort.compareNumericForSort(`-${numeric(1)}`, `-${numeric(2)}`), 1);
  assert.equal(sort.compareNumericForSort(`${numeric(1)}000`, numeric(1)), 0);
  const ms = best(() => sort.compareNumericForSort(numeric(1), numeric(2)));
  assert.ok(ms <= 5, `${ms.toFixed(2)} ms`);
});

test("N2: a table sort of 32 PostgreSQL numeric cells of 16,385 characters takes 50 ms or less, and the order is exact", () => {
  const R = 32;
  const vals = Array.from({ length: R }, (_, i) => numeric((i * 7919) % R));
  const f = { name: "amount", type: "numeric", kind: "number_text", values: vals, num: Float64Array.from(vals, Number), nulls: new Uint8Array(R), labels: null, config: {} };
  const args = { count: R, fields: [f], sort: [{ index: 0, desc: false }], textAt: () => "" };
  const order = rowOrder(args);
  for (let k = 1; k < R; k++) assert.equal(sort.compareNumericForSort(vals[order[k - 1]], vals[order[k]]), -1);
  const ms = best(() => rowOrder(args));
  assert.ok(ms <= 50, `${ms.toFixed(1)} ms`);
});

test("N2: the two steps of compareNumericForSort give its result", () => {
  const vals = [null, "", "nan", "-inf", "inf", "0", "-0", "1.5", "-1.50", "12345678901234567890", "0.1234567890123456789", "-0.1234567890123456789", "1.000000000000000000001", "abc", "1e5", 2.5, NaN, Infinity];
  for (const a of vals) for (const b of vals) assert.equal(sort.compareNormalizedForSort(sort.normalizeNumericForSort(a), sort.normalizeNumericForSort(b)), sort.compareNumericForSort(a, b), `${a} ${b}`);
});
