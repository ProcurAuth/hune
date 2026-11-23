// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-46 (9.19e section 3, 9.19f section 4, T69 and T70): the caps (each one fails closed, and no array exists above it), the texts of the
// messages and of the descriptions with 30 hostile names, and the time of the model for 500,000 values.
import test from "node:test";
import assert from "node:assert/strict";
import { MAX_BUCKET_VALUES, MAX_HIST_BUCKETS, buildHistogram, histDescription } from "../../web/js/bucket_model.js";
import { MAX_HEAT_CELLS, bucketRaw, buildHeatmap, heatCells, heatDescription } from "../../web/js/heat_model.js";
import { CELLS, FakeNode, HIST, numberField, read, timeField } from "./bucket_helpers.mjs";

const fe46 = read("FE-46-bucket-caps.json");
const CALC = { calculate: true, x_buckets: { mode: "size", value: 1 }, y_buckets: { mode: "size", value: 1, scale: "linear" } };
const heat = (fields, options, range = { from: 0, to: 0 }) => buildHeatmap({ fields, names: fields.map((f) => f.name), options, range });

test("FE-46: the constants equal 9.19e and 9.19f", () => {
  assert.deepEqual({ MAX_BUCKET_VALUES, MAX_HEAT_CELLS, MAX_HIST_BUCKETS }, fe46.constants);
});

for (const v of fe46.caps.heat_raw_cells) {
  test(`FE-46 caps: ${v.id} (raw grid ${v.nx} x ${v.ny})`, () => {
    const last = v.nx - 1;
    const m = bucketRaw({ times: Float64Array.of(0, 0), cols: [Float64Array.of(0, v.ny - 1)], from: 0, to: last, x: { mode: "size", value: 1 }, y: { mode: "size", value: 1, scale: "linear" } });
    if (v.pass) {
      assert.equal(m.counts.length, v.nx * v.ny);
      assert.equal(m.counts.length <= MAX_HEAT_CELLS, true);
    } else {
      assert.equal(m.limit, v.text);
      assert.equal(m.counts, undefined);
    }
  });
}

for (const v of fe46.caps.heat_bucketed_cells) {
  test(`FE-46 caps: ${v.id} (frame of ${v.times} times and ${v.rows} rows)`, () => {
    const time = timeField(Array.from({ length: v.times }, (_, i) => i * 1000));
    const fields = [time, ...Array.from({ length: v.rows }, (_, k) => ({ name: String(k), kind: "number", values: new Float64Array(v.times).fill(1), nulls: new Uint8Array(v.times), config: {} }))];
    const m = heat(fields, { calculate: false });
    if (v.pass) {
      assert.equal(m.counts.length, v.times * v.rows);
      assert.equal(heatCells(m, CELLS).n, v.times * v.rows);
    } else {
      assert.equal(m.limit, v.text);
      assert.equal(m.counts, undefined);
    }
  });
}

for (const v of fe46.caps.heat_values) {
  test(`FE-46 caps: ${v.id} (${v.rows} rows of ${v.fields} fields)`, () => {
    const fields = [{ name: "time", kind: "time", values: new Float64Array(v.rows), nulls: new Uint8Array(v.rows), config: {} }];
    for (let k = 0; k < v.fields; k++) fields.push({ name: `f${k}`, kind: "number", values: new Float64Array(v.rows), nulls: new Uint8Array(v.rows), config: {} });
    const m = heat(fields, CALC);
    if (v.pass) {
      assert.ok(!m.limit && !m.error, m.limit || m.error);
      assert.equal(m.counts.reduce((a, c) => a + c, 0), v.rows * v.fields);
    } else {
      assert.equal(m.limit, v.text);
      assert.equal(m.counts, undefined);
    }
  });
}

for (const v of fe46.caps.hist_buckets) {
  test(`FE-46 caps: ${v.id} (values 0 to ${v.top}, size 1)`, () => {
    const m = buildHistogram({ fields: [numberField("v", [0, v.top])], names: ["v"], options: { ...HIST, bucket_size: 1 } });
    if (v.pass) assert.equal(m.nb, v.top + 1);
    else assert.deepEqual([m.limit, m.series], [v.text, undefined]);
  });
}

for (const v of fe46.caps.hist_bucketed_rows) {
  test(`FE-46 caps: ${v.id} (the buckets that a query made)`, () => {
    const lo = Array.from({ length: v.rows }, (_, i) => i);
    const m = buildHistogram({ fields: [numberField("xMin", lo), numberField("n", lo)], names: ["xMin", "n"], options: HIST });
    if (v.pass) assert.equal(m.nb, v.rows);
    else assert.deepEqual([m.limit, m.series], [v.text, undefined]);
  });
}

for (const v of fe46.caps.hist_values) {
  test(`FE-46 caps: ${v.id} (${v.n} values)`, () => {
    const m = buildHistogram({ fields: [{ name: "v", kind: "number", values: new Float64Array(v.n), nulls: new Uint8Array(v.n), config: {} }], names: ["v"], options: HIST });
    if (v.pass) assert.equal(m.series[0].counts[0], v.n);
    else assert.deepEqual([m.limit, m.series], [v.text, undefined]);
  });
}

test("FE-46: the messages of the errors", () => {
  const m = fe46.messages;
  assert.equal(heat([numberField("a", [1, 2])], { calculate: false }).error, m.error_no_time);
  assert.equal(heat([timeField([0, 1])], { calculate: false }).error, m.error_no_number);
  assert.equal(heat([timeField([0]), numberField("a", [1])], { calculate: false }).error, m.error_two_rows);
  assert.equal(heat([timeField([0]), numberField("a", [null])], CALC).error, m.error_no_value);
  assert.equal(buildHistogram({ fields: [numberField("xMin", [0, 1]), numberField("n", [1, 1])], names: ["xMin", "n"], options: HIST }).size, 1);
  assert.equal(buildHistogram({ fields: [numberField("xMin", [3]), numberField("n", [1])], names: ["xMin", "n"], options: HIST }).error, m.error_width);
  assert.equal(buildHistogram({ fields: [numberField("v", [0, 1e12])], names: ["v"], options: { ...HIST, bucket_count: 1 } }).error, m.error_fit);
  assert.equal(buildHistogram({ fields: [{ name: "t", kind: "text", values: ["a"], config: {} }], names: ["t"], options: HIST }).error, "The data has no number column.");
});

// --- the texts -----------------------------------------------------------------------------------------------------------------------

const fmt = (v) => String(v);
const clipped = (s) => (s.length <= 120 ? s : `${Array.from(s.slice(0, 240)).slice(0, 120).join("")}…`);

// The description as the panel writes it: a text node of a hidden paragraph. A hostile text never makes an element.
function paragraph(text) {
  const p = new FakeNode("p");
  p.textContent = text;
  return p;
}

test("FE-46: the description of a heatmap with 30 hostile row names is one string and makes no element", () => {
  assert.equal(fe46.hostile.length, 30);
  for (const name of fe46.hostile) {
    const m = heat([timeField([0, 60000]), numberField(name, [5, 7]), numberField("other", [1, 2])], { calculate: false });
    assert.deepEqual(m.labels, [name, "other"], "the names pass unchanged");
    const text = heatDescription(m, heatCells(m, CELLS), fmt, "utc");
    assert.equal(typeof text, "string");
    assert.ok(text.includes(clipped(name)), `the description names the row: ${JSON.stringify(name.slice(0, 20))}`);
    const p = paragraph(text);
    assert.equal(p.children.length, text === "" ? 0 : 1);
    assert.ok(p.children.every((c) => c.tag === "#text"));
    assert.equal(p.textContent, text);
  }
});

test("FE-46: the description of a histogram with 30 hostile series names is one string and makes no element", () => {
  for (const name of fe46.hostile) {
    const m = buildHistogram({ fields: [numberField(name, [1, 2, 2, 3])], names: [name], options: { ...HIST, bucket_size: 1 } });
    assert.equal(m.series[0].name, name);
    const text = histDescription(m, fmt);
    assert.ok(text.includes(clipped(name)));
    assert.ok(paragraph(text).children.every((c) => c.tag === "#text"));
  }
});

test("FE-46: the texts of the descriptions", () => {
  const m = heat([timeField([0, 60000]), numberField("a", [5, 0]), numberField("b", [1, 9])], { calculate: false });
  const c = heatCells(m, CELLS);
  assert.equal(heatDescription(m, c, fmt, "utc"), "2 columns and 2 rows. The highest count is 9, at 1970-01-01 00:01:00, in the row b. 3 cells are drawn.");
  const none = heat([timeField([0, 60000]), numberField("a", [0, 0])], { calculate: false });
  assert.equal(heatDescription(none, heatCells(none, CELLS), fmt, "utc"), "2 columns and 1 rows. No cell is drawn.");
  const h = buildHistogram({ fields: [numberField("a", [1, 2, 2, 3, 3, 3])], names: ["a"], options: { ...HIST, bucket_size: 1 } });
  assert.equal(histDescription(h, fmt), "3 buckets of the size 1. a: the highest count is 3, in the bucket 3 - 4.");
  const raw = bucketRaw({ times: Float64Array.of(0, 0, 0), cols: [Float64Array.of(1, 2, 2)], from: 0, to: 0, x: { mode: "size", value: 1 }, y: { mode: "size", value: 1, scale: "linear" } });
  const rm = { shape: "raw", ...raw };
  assert.equal(heatDescription(rm, heatCells(rm, CELLS), fmt, "utc"), "1 columns and 2 rows. The highest count is 2, at 1970-01-01 00:00:00, in the bucket 2-3. 2 cells are drawn.");
});

test("FE-46: a field name never makes a property of an object (no map keyed by a name)", () => {
  const names = ["__proto__", "constructor", "toString", "hasOwnProperty"];
  const m = heat([timeField([0, 60000]), ...names.map((n) => numberField(n, [1, 2]))], { calculate: false });
  assert.deepEqual(m.labels, names);
  assert.equal(Object.keys(m).includes("__proto__"), false);
  assert.equal({}.polluted, undefined);
});

// --- the time (9.19e section 10, 9.19f section 6): the lowest of 7 runs on the reference machine ---------------------------------------------

const best = (fn) => {
  let low = Infinity;
  for (let k = 0; k < 7; k++) {
    const t0 = performance.now();
    fn();
    low = Math.min(low, performance.now() - t0);
  }
  return low;
};

test(`FE-46: the grid of 500,000 values takes ${fe46.time.heat_budget_ms} ms or less, with 20,000 cells (the lowest of 7 runs)`, () => {
  let s = 7;
  const rnd = () => (s = (s * 1664525 + 1013904223) >>> 0) / 4294967296;
  const n = fe46.time.heat_values;
  const times = Float64Array.from({ length: n }, (_, i) => Math.floor((i * 18000e3) / n));
  const v = Float64Array.from({ length: n }, () => 40 * -Math.log(1 - rnd()) + 0.001);
  const run = () => bucketRaw({ times, cols: [v], from: 0, to: 18000e3, x: { mode: "count", value: 200 }, y: { mode: "count", value: 100, scale: "linear" } });
  const g = run();
  assert.ok(g.nx * g.ny >= 5000 && g.nx * g.ny <= MAX_HEAT_CELLS, `${g.nx} x ${g.ny}`);
  const ms = best(run);
  assert.ok(ms <= fe46.time.heat_budget_ms, `${ms.toFixed(1)} ms`);
  const log = best(() => bucketRaw({ times, cols: [v], from: 0, to: 18000e3, x: { mode: "count", value: 200 }, y: { mode: "size", value: 4, scale: "log2" } }));
  assert.ok(log <= fe46.time.heat_budget_ms, `log: ${log.toFixed(1)} ms`);
});

test(`FE-46: the buckets of 500,000 values take ${fe46.time.hist_budget_ms} ms or less (the lowest of 7 runs)`, () => {
  let s = 11;
  const rnd = () => (s = (s * 1664525 + 1013904223) >>> 0) / 4294967296;
  const n = fe46.time.hist_values;
  const a = numberField("a", []);
  a.values = Float64Array.from({ length: n / 2 }, () => 40 * -Math.log(1 - rnd()));
  const b = numberField("b", []);
  b.values = Float64Array.from({ length: n / 2 }, () => 25 * -Math.log(1 - rnd()));
  const run = () => buildHistogram({ fields: [a, b], names: ["a", "b"], options: HIST });
  assert.ok(run().nb >= 10);
  const ms = best(run);
  assert.ok(ms <= fe46.time.hist_budget_ms, `${ms.toFixed(1)} ms`);
});
