// FE-42 to FE-45 (9.19e, 9.19f): the grid of raw values, the size lists, the shape that is already bucketed, the buckets of the histogram and
// the colour scale. The vectors come from the measure of Grafana 12.4.2 (19.20g), from the rule of its code, or from the choices of Hune.
import test from "node:test";
import assert from "node:assert/strict";
import { HIST_SIZES, buildHistogram, histTicks, niceHist, shortText } from "../../web/js/bucket_model.js";
import {
  MAX_HEAT_CELLS, SCHEMES, TIME_SIZES, VALUE_SIZES, bucketRaw, buildHeatmap, heatCells, heatPalette, levelOf, logTicks, nameNumber, niceTime, niceValue, ordinalLabels,
} from "../../web/js/heat_model.js";
import { CELLS, COLOR, HIST, column, frameFields, measure, numberField, read } from "./bucket_helpers.mjs";
import { displayNames } from "../../web/js/names.js";
import { longToWide } from "../../web/js/transform.js";
import { toFields } from "./helpers.mjs";
// The session gives the cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
import "./session_cells.mjs";

const fe42 = read("FE-42-bucket-raw.json");
const fe43 = read("FE-43-bucket-sizes.json");
const fe44 = read("FE-44-bucketed.json");
const fe45 = read("FE-45-heat-colors.json");
const hdata = read("FE-44-hist-data.json").sets;

// --- FE-42 -------------------------------------------------------------------------------------------------------------------------

// The grid of a vector. A vector with `data` uses the 20,000 rows of the measure.
function gridOf(v) {
  if (v.data) {
    const d = measure();
    return bucketRaw({ times: d.times, cols: [d.v], from: v.from, to: v.to, x: v.x, y: v.y });
  }
  return bucketRaw({ times: column(v.times), cols: v.cols.map(column), from: v.from, to: v.to, x: v.x, y: v.y });
}

// The cells of a grid as `[column, row, count]`, and the columns and the rows that hold a cell.
function cellsOf(g) {
  const cells = [];
  const cols = new Set();
  const rows = new Set();
  for (let c = 0; c < g.nx; c++) {
    for (let r = 0; r < g.ny; r++) {
      const n = g.counts[c * g.ny + r];
      if (n) {
        cells.push([c, r, n]);
        cols.add(c);
        rows.add(r);
      }
    }
  }
  return { cells, cols: cols.size, rows: rows.size };
}

test("FE-42 has 5 sets, the data of the measure and 39 vectors of Grafana (17 measured, 22 by its rule)", () => {
  assert.deepEqual(Object.keys(fe42.sets), ["time_size", "value_size", "log_rows", "grids", "counts", "refusals"]);
  const all = Object.values(fe42.sets).flat();
  assert.equal(all.filter((v) => v.source.startsWith("grafana")).length, 39);
  assert.equal(all.filter((v) => v.source === "grafana-measure").length, 17);
  assert.equal(fe42.sets.counts.length, 21);
  assert.equal(measure().times.length, 20000);
});

for (const set of ["time_size", "value_size"]) {
  for (const v of fe42.sets[set]) {
    test(`FE-42 ${set}: ${v.id} (${v.source})`, () => {
      const g = gridOf(v);
      assert.ok(!g.error && !g.limit, g.error || g.limit);
      if (v.expect.sx !== undefined) assert.equal(g.sx, v.expect.sx);
      if (v.expect.sy !== undefined) assert.equal(g.sy, v.expect.sy);
    });
  }
}

for (const v of fe42.sets.grids) {
  test(`FE-42 grids: ${v.id} (${v.source}): columns, rows and cells that hold a count`, () => {
    const g = gridOf(v);
    assert.ok(!g.error && !g.limit, g.error || g.limit);
    const got = cellsOf(g);
    assert.equal(got.cells.length, v.expect.cells, "cells");
    assert.equal(got.cols, v.expect.cols_drawn, "columns");
    assert.equal(got.rows, v.expect.rows_drawn, "rows");
    // Every value of the range is in one cell, but the value of 0 or less of a log scale.
    assert.equal(got.cells.reduce((n, c) => n + c[2], 0), 20000 - g.skipped);
  });
}

for (const v of fe42.sets.log_rows) {
  test(`FE-42 log_rows: ${v.id} (${v.source}): the rows and the ticks of a plot of ${v.plot_h} px`, () => {
    const g = gridOf(v);
    assert.equal(g.ny, v.expect.ny);
    assert.equal(cellsOf(g).rows, v.expect.rows_drawn);
    assert.equal(g.skipped, 1, "the value 0 of the data is skipped (the measure of Grafana used the 19,999 values above 0)");
    if (!v.expect.ticks) return;
    const m = { shape: "raw", ...g };
    assert.deepEqual(logTicks(m, v.plot_h), v.expect.ticks);
    const base = g.log;
    v.expect.ticks.forEach((u, i) => {
      const want = Number(v.expect.tick_labels[i]);
      assert.ok(Math.abs(base ** u - want) / want < 0.01, `${base}^${u} against ${want}`);
    });
  });
}

for (const v of fe42.sets.counts) {
  test(`FE-42 counts: ${v.id} (${v.source}): ${v.why}`, () => {
    const g = gridOf(v);
    assert.ok(!g.error && !g.limit, g.error || g.limit);
    const e = v.expect;
    for (const key of ["sx", "sy", "nx", "ny", "c0", "r0"]) if (e[key] !== undefined) assert.equal(g[key], e[key], key);
    assert.deepEqual(cellsOf(g).cells, e.cells);
    assert.equal(g.counts.length, g.nx * g.ny);
  });
}

for (const v of fe42.sets.refusals) {
  test(`FE-42 refusals: ${v.id} (${v.source}): ${v.why}`, () => {
    const g = gridOf(v);
    const e = v.expect;
    if (e.error) return assert.equal(g.error, e.error);
    if (e.limit) {
      assert.equal(g.limit, e.limit);
      assert.equal(g.counts, undefined, "no array is made above a cap");
      return undefined;
    }
    assert.ok(!g.error && !g.limit, g.error || g.limit);
    assert.equal(g.skipped, e.skipped);
    assert.equal(g.ny, e.ny);
    if (e.r0 !== undefined) assert.equal(g.r0, e.r0);
    assert.deepEqual(cellsOf(g).cells, e.cells);
    if (e.note !== undefined) assert.equal(buildHeatmap({ fields: [{ name: "time", kind: "time", values: column(v.times), nulls: new Uint8Array(v.times.length), config: {} }, numberField("v", v.cols[0])], names: ["time", "v"], options: { calculate: true, x_buckets: v.x, y_buckets: v.y }, range: { from: v.from, to: v.to } }).note, e.note);
    return undefined;
  });
}

test("FE-42: a heatmap of a log scale gives the note of the values of 0 or less, and none when no value is skipped", () => {
  const d = measure();
  const fields = [{ name: "time", kind: "time", values: d.times, nulls: new Uint8Array(d.times.length), config: {} }, { name: "v", kind: "number", values: d.v, nulls: new Uint8Array(d.v.length), config: {} }];
  const run = (y) => buildHeatmap({ fields, names: ["time", "v"], options: { calculate: true, x_buckets: { mode: "count", value: null }, y_buckets: y }, range: { from: d.from, to: d.to } });
  assert.equal(run({ mode: "size", value: 1, scale: "log2" }).note, "1 value of 0 or less is not on the log scale.");
  assert.equal(run({ mode: "count", value: null, scale: "linear" }).note, "");
});

test("FE-42: the counts of a grid add up to the number of values for 7 runs of random data (the grid never loses a value)", () => {
  let s = 12345;
  const rnd = () => (s = (s * 1664525 + 1013904223) >>> 0) / 4294967296;
  for (let k = 0; k < 7; k++) {
    const n = 2000;
    const times = Float64Array.from({ length: n }, () => Math.floor(rnd() * 3600e3));
    const v = Float64Array.from({ length: n }, () => rnd() * 1000 - 200 * rnd());
    const g = bucketRaw({ times, cols: [v], from: 0, to: 3600e3, x: { mode: "count", value: 30 }, y: { mode: "count", value: 20, scale: "linear" } });
    assert.equal(g.counts.reduce((a, c) => a + c, 0), n);
  }
});

// --- FE-43 -------------------------------------------------------------------------------------------------------------------------

test("FE-43: the lists T, N and Z", () => {
  assert.deepEqual([...TIME_SIZES], fe43.lists.T);
  assert.equal(VALUE_SIZES.length, fe43.lists.N_count);
  assert.equal(HIST_SIZES.length, fe43.lists.Z_count);
  assert.deepEqual([TIME_SIZES[0], TIME_SIZES.at(-1), TIME_SIZES.length], fe43.ends.T);
  assert.deepEqual([VALUE_SIZES[0], VALUE_SIZES.at(-1), VALUE_SIZES.length], fe43.ends.N);
  assert.deepEqual([HIST_SIZES[0], HIST_SIZES.at(-1), HIST_SIZES.length], fe43.ends.Z);
  for (const list of [TIME_SIZES, VALUE_SIZES, HIST_SIZES]) assert.ok(list.every((n, i) => i === 0 || n > list[i - 1]), "ascending");
});

test("FE-43: 120 ranges for the time size, 120 values of Y for the value size and 120 pairs for the size of a histogram", () => {
  assert.equal(fe43.sets.time.length, 120);
  assert.equal(fe43.sets.value.length, 120);
  assert.equal(fe43.sets.hist.length, 120);
  for (const v of fe43.sets.time) assert.equal(niceTime(v.x), v.expect, v.id);
  for (const v of fe43.sets.value) assert.equal(niceValue(v.y), v.expect, v.id);
  for (const v of fe43.sets.hist) assert.equal(niceHist(v.y, v.h), v.expect, v.id);
});

test("FE-43: a number from 1 to below 10 is in the list N only when it is whole", () => {
  for (const v of fe43.sets.whole) assert.equal(niceValue(v.y), v.expect, v.id);
  for (const n of VALUE_SIZES) if (n >= 1 && n < 10) assert.equal(n, Math.trunc(n));
  assert.ok(VALUE_SIZES.includes(0.15) && VALUE_SIZES.includes(15) && !VALUE_SIZES.includes(1.5));
});

test("FE-43: the lowest and the highest number of each list, equal values and one value", () => {
  assert.equal(niceTime(0.5), 1);
  assert.equal(niceTime(1e13), 315360e6);
  assert.equal(niceValue(0), 1e-16);
  assert.equal(niceValue(3e16), 9.5e15);
  assert.equal(niceHist(5e9, 1), 0);
  const eq = buildHistogram({ fields: [numberField("v", [3, 3, 3])], names: ["v"], options: HIST });
  const one = buildHistogram({ fields: [numberField("v", [3])], names: ["v"], options: HIST });
  assert.equal(eq.size, 1);
  assert.equal(one.size, 1);
});

// --- FE-44 -------------------------------------------------------------------------------------------------------------------------

const frameModel = (v) => {
  const { fields, names } = frameFields(v.frame, v.kind, v.hide);
  return buildHeatmap({ fields, names, options: { calculate: false }, range: { from: 0, to: 1 } });
};

for (const v of fe44.sets.heatmap) {
  test(`FE-44 heatmap: ${v.id} (${v.source})`, () => {
    const m = frameModel(v);
    const e = v.expect;
    if (e.error) return assert.equal(m.error, e.error);
    assert.ok(!m.error && !m.limit, m.error || m.limit);
    assert.equal(m.shape, "bucketed");
    if (e.nx !== undefined) assert.equal(m.nx, e.nx);
    if (e.ny !== undefined) assert.equal(m.ny, e.ny);
    if (e.labels) assert.deepEqual(m.labels, e.labels);
    if (e.step !== undefined) assert.equal(m.step, e.step);
    if (e.counts) assert.deepEqual(Array.from({ length: m.nx }, (_, c) => Array.from({ length: m.ny }, (__, r) => (Number.isNaN(m.counts[c * m.ny + r]) ? null : m.counts[c * m.ny + r]))), e.counts);
    const c = heatCells(m, CELLS);
    if (e.drawn !== undefined) assert.equal(c.n, e.drawn, "drawn cells");
    if (e.positions !== undefined) assert.equal(m.nx * m.ny, e.positions);
    if (e.min !== undefined) assert.equal(c.min, e.min);
    if (e.max !== undefined) assert.equal(c.max, e.max);
    return undefined;
  });
}

test("FE-44 heatmap: the cells of a frame that is already bucketed have the end of the time bucket and an ordinal row", () => {
  const m = frameModel({ frame: { names: ["a", "b"], time: [600000, 1200000, 1800000], cols: [[1, 2, 3], [4, 5, 6]] } });
  const c = heatCells(m, CELLS);
  assert.equal(c.n, 6);
  assert.deepEqual([c.x0[0], c.x1[0], c.y0[0], c.y1[0]], [0, 600000, -0.5, 0.5]);
  assert.deepEqual([c.x0[5], c.x1[5], c.y0[5], c.y1[5]], [1200000, 1800000, 0.5, 1.5]);
  assert.equal(c.level[0], 1, "the lowest count has the first level (index plus 1)");
  assert.equal(c.level[5], 64, "the highest count has the last level");
});

// A long frame of 3 times and 10 names, through the shared long-to-wide path (9.22). `name` is the name of the value field.
const longHeat = (name, les) => {
  const t = [];
  const le = [];
  const v = [];
  for (let k = 0; k < 3; k++) les.forEach((x, b) => { t.push(60000 * (k + 1)); le.push(x); v.push(k + b + 1); });
  const wide = longToWide(toFields([{ name: "time", kind: "time", values: t }, { name: "le", kind: "text", values: le }, { name, kind: "number", values: v }]));
  return { wide, names: displayNames(wide), model: buildHeatmap({ fields: wide, names: displayNames(wide), options: { calculate: false }, range: { from: 0, to: 1 } }) };
};

test("FE-44 heatmap (ID-K195c): a long frame drops the prefix of the value field and the rows sort as numbers", () => {
  const les = Array.from({ length: 10 }, (_, i) => String(i + 1));
  const { wide, names, model } = longHeat("count", les);
  assert.deepEqual(names.slice(1), ["count 1", "count 10", "count 2", "count 3", "count 4", "count 5", "count 6", "count 7", "count 8", "count 9"], "the other panels keep the 9.22 naming");
  assert.equal(wide.length, 11);
  assert.deepEqual(model.labels, les, "10 comes after 9");
  assert.equal(model.counts[0 * 10 + 9], 10, "the row 10 has the values of its own field");
});

test("FE-44 heatmap (ID-K195c): +Inf is the last row, and names that do not share the prefix keep it", () => {
  assert.deepEqual(longHeat("count", ["+Inf", "5", "1"]).model.labels, ["1", "5", "+Inf"]);
  const real = ["count 1", "count 2"];
  const fields = [{ name: "time", kind: "time", values: Float64Array.of(1, 2), nulls: new Uint8Array(2), config: {} }, ...real.map((n) => numberField(n, [1, 2]))];
  assert.deepEqual(buildHeatmap({ fields, names: ["time", ...real], options: { calculate: false }, range: { from: 0, to: 1 } }).labels, real, "a wide frame with real names keeps them");
  const { wide } = longHeat("count", ["1", "2"]);
  const names = displayNames(wide);
  names[2] = "other 2";
  assert.deepEqual(buildHeatmap({ fields: wide, names, options: { calculate: false }, range: { from: 0, to: 1 } }).labels, ["count 1", "other 2"], "one name without the prefix keeps all the names");
});

function histOf(v) {
  let cols;
  let names;
  if (v.inline) {
    cols = v.inline;
    names = cols.map((_, i) => String.fromCharCode(97 + i));
  } else {
    cols = v.series.map((s) => {
      const base = s.endsWith("3000") ? s.slice(0, -4) : s;
      const list = hdata[base].map((x) => (x === null ? null : x / 1000));
      return s.endsWith("3000") ? list.slice(0, 3000) : list;
    });
    names = v.series.map((s) => (s.startsWith("f") && s !== "f6" ? "v" : s === "f6" ? "v" : s.toLowerCase().replace("3000", "")));
  }
  return buildHistogram({ fields: cols.map((l, i) => numberField(names[i], l)), names, options: v.options });
}

const barsOf = (m) => m.series.reduce((n, s) => n + [...s.counts].filter((c) => c > 0).length, 0);

for (const v of [...fe44.sets.histogram, ...fe44.sets.hist_rules]) {
  test(`FE-44 histogram: ${v.id} (${v.source})`, () => {
    const m = histOf(v);
    const e = v.expect;
    if (e.error) return assert.equal(m.error, e.error);
    if (e.limit) return assert.equal(m.limit, e.limit);
    assert.ok(!m.error && !m.limit, m.error || m.limit);
    assert.equal(m.shape, "values");
    assert.equal(m.size, e.size, "size");
    assert.equal(m.nb, e.nb, "buckets");
    if (e.lo !== undefined) assert.equal(m.lows[0], e.lo, "low edge");
    if (e.bars !== undefined) assert.equal(barsOf(m), e.bars, "bars");
    if (e.totals) assert.deepEqual(m.series.map((s) => s.counts.reduce((a, c) => a + c, 0)), e.totals, "totals");
    if (e.names) assert.deepEqual(m.series.map((s) => s.name), e.names);
    if (e.counts) assert.deepEqual(m.series.map((s) => [...s.counts]), e.counts);
    if (e.lows) assert.deepEqual([...m.lows], e.lows);
    if (e.highs) assert.deepEqual([...m.highs], e.highs);
    // The edges are contiguous: the high edge of a bucket is the low edge of the next one.
    for (let j = 1; j < m.nb; j++) assert.equal(m.highs[j - 1], m.lows[j]);
    return undefined;
  });
}

for (const v of fe44.sets.hist_bucketed) {
  test(`FE-44 hist_bucketed: ${v.id} (${v.source}): ${v.why}`, () => {
    const fields = v.cols.map((l, i) => numberField(v.names[i], l));
    const m = buildHistogram({ fields, names: v.names, options: HIST });
    const e = v.expect;
    if (e.error) return assert.equal(m.error, e.error);
    assert.ok(!m.error && !m.limit, m.error || m.limit);
    assert.equal(m.shape, "bucketed");
    assert.equal(m.size, e.size);
    assert.equal(m.nb, e.nb);
    if (e.lows) assert.deepEqual([...m.lows], e.lows);
    if (e.highs) assert.deepEqual([...m.highs], e.highs);
    assert.deepEqual(m.series.map((s) => Array.from(s.counts, (c) => (Number.isNaN(c) ? null : c))), e.counts);
    if (e.names) assert.deepEqual(m.series.map((s) => s.name), e.names);
    return undefined;
  });
}

test("FE-44 histogram: 13 vectors of the measure, and the values of a hidden field and of a number_text field", () => {
  assert.ok(fe44.sets.histogram.filter((v) => v.source === "grafana-measure").length >= 13);
  const hid = buildHistogram({ fields: [numberField("a", [1, 2]), numberField("b", [5, 9], { hide_from: { viz: true } })], names: ["a", "b"], options: { ...HIST, bucket_size: 1 } });
  assert.deepEqual(hid.series.map((s) => s.name), ["a"]);
  const flags = buildHistogram({ fields: [numberField("a", [1, 2], { hide_from: { legend: true } }), numberField("b", [1, 2], { hide_from: { tooltip: true } })], names: ["a", "b"], options: { ...HIST, bucket_size: 1 } });
  assert.deepEqual(flags.series.map((s) => [s.legend, s.tooltip]), [[false, true], [true, false]]);
  const text = buildHistogram({ fields: [{ name: "n", kind: "number_text", values: ["1", "2", "x", null, ""], num: Float64Array.of(1, 2, NaN, NaN, NaN), nulls: new Uint8Array(5), config: {} }], names: ["n"], options: { ...HIST, bucket_size: 1 } });
  assert.equal(text.series[0].counts.reduce((a, c) => a + c, 0), 2, "a text that is not a finite number is skipped");
});

test("FE-44: the row names that are numbers (decimal text, an exponent, +Inf and -Inf)", () => {
  for (const [name, n] of [["1", 1], ["+1", 1], ["-1.5", -1.5], [".5", 0.5], ["5.", 5], ["1e3", 1000], ["1E-3", 0.001], ["+Inf", Infinity], ["-Inf", -Infinity]]) assert.equal(nameNumber(name), n, name);
  for (const name of ["", " 1", "1 ", "Inf", "Infinity", "NaN", "0x10", "1_0", "1,5", "e3", "--1", "+-1"]) assert.ok(Number.isNaN(nameNumber(name)), name);
});

test("FE-44: the labels of an ordinal axis and the edges of a histogram that show", () => {
  assert.deepEqual(ordinalLabels(8, 50), [0, 7]);
  assert.deepEqual(ordinalLabels(1, 50), [0]);
  assert.deepEqual(ordinalLabels(4, 165), [0, 1, 2, 3], "4 rows of 165 px: (165 - 15) / 4 is 37");
  assert.deepEqual(ordinalLabels(30, 165), [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28], "30 rows: 15 labels (10 px each) is the first count that has 10 px or more");
  assert.deepEqual(ordinalLabels(100, 165), [0, 8, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96]);
  const m = histOf(fe44.sets.histogram[0]);
  assert.deepEqual(histTicks(m, 934), Array.from({ length: 19 }, (_, i) => i * 20), "bars of 51.9 px: every edge (the measure of Grafana)");
  assert.deepEqual(histTicks(m, 600), Array.from({ length: 10 }, (_, i) => i * 40), "bars of 33 px: every second edge");
});

// --- FE-45 -------------------------------------------------------------------------------------------------------------------------

const near = (a, b, tol) => [1, 3, 5].every((k) => Math.abs(parseInt(a.slice(k, k + 2), 16) - parseInt(b.slice(k, k + 2), 16)) <= tol);

for (const v of [...fe45.sets.schemes, ...fe45.sets.steps]) {
  test(`FE-45 schemes: ${v.id}: ${v.steps} colours, a difference of ${v.tolerance} at most in a channel`, () => {
    const got = heatPalette({ ...COLOR, mode: "scheme", scheme: v.scheme, steps: v.steps, reverse: v.reverse === true }, v.theme === "dark");
    assert.equal(got.length, v.steps);
    got.forEach((c, i) => assert.ok(/^#[0-9a-f]{6}$/.test(c) && near(c, v.expect[i], v.tolerance), `colour ${i}: ${c} against ${v.expect[i]}`));
  });
}

test("FE-45: the 10 schemes, their rules of the inversion, and the colours of oranges that the measure shows", () => {
  assert.deepEqual(Object.keys(SCHEMES), ["oranges", "blues", "greens", "greys", "purples", "reds", "yl_or_rd", "rd_yl_gn", "spectral", "viridis"]);
  const dark = heatPalette(COLOR, true);
  const light = heatPalette(COLOR, false);
  assert.deepEqual([dark[0], dark[63]], ["#7f2704", "#fff5eb"], "the dark theme goes from the lowest count to the lightest colour");
  assert.deepEqual([light[0], light[1], light[2], light[3], light[63]], ["#fff5eb", "#fff3e7", "#fff1e4", "#ffefe0", "#7f2704"]);
  assert.deepEqual(heatPalette({ ...COLOR, reverse: true }, true), [...dark].reverse());
});

for (const v of fe45.sets.measure) {
  test(`FE-45 measure: ${v.id}: the colour of each of the 126 cells of example 1`, () => {
    const d = measure();
    const g = bucketRaw({ times: d.times, cols: [d.v], from: d.from, to: d.to, x: { mode: "count", value: null }, y: { mode: "count", value: null, scale: "linear" } });
    const c = heatCells({ shape: "raw", ...g }, CELLS);
    assert.equal(c.n, v.cells);
    assert.deepEqual([c.min, c.max], [1, 960], "the legend of the measure shows 1 and 960");
    const palette = heatPalette(COLOR, v.theme === "dark");
    const got = {};
    for (let i = 0; i < c.n; i++) got[palette[c.level[i] - 1]] = (got[palette[c.level[i] - 1]] || 0) + 1;
    // The measure of the light theme lists its first 6 colours only: the counts of these colours must agree.
    if (v.partial) for (const [colour, n] of Object.entries(v.colours)) assert.equal(got[colour], n, colour);
    else assert.deepEqual(got, v.colours);
  });
}

for (const v of fe45.sets.levels) {
  test(`FE-45 levels: ${v.id}: min ${v.min}, max ${v.max}, ${v.steps} steps`, () => {
    assert.deepEqual(v.values.map((x) => levelOf(x, v.min, v.max, v.steps)), v.expect);
  });
}

test("FE-45: the range of the colours after hide_le and hide_ge (101 and 263 for the data of the measure; 19 cells)", () => {
  const d = measure();
  const g = bucketRaw({ times: d.times, cols: [d.v], from: d.from, to: d.to, x: { mode: "count", value: null }, y: { mode: "count", value: null, scale: "linear" } });
  const m = { shape: "raw", ...g };
  const c = heatCells(m, { ...CELLS, hide_le: 100, hide_ge: 400 });
  assert.deepEqual([c.min, c.max, c.n], [101, 263, 19]);
  const fixed = heatCells(m, { ...CELLS, color: { ...COLOR, min: 50, max: 100 } });
  assert.deepEqual([fixed.min, fixed.max], [50, 100]);
  assert.equal(Math.max(...fixed.level), 64, "a count above max has the last level");
  assert.equal(heatCells(m, { ...CELLS, hide_le: null }).n >= 126, true, "hide_le null draws the empty cells too");
  assert.equal(shortText(960), "960");
  assert.equal(shortText(4910), "4.91 K");
});

for (const v of fe45.sets.opacity) {
  test(`FE-45 opacity: ${v.id}`, () => {
    const got = heatPalette({ ...COLOR, mode: "opacity", scale: v.scale, exponent: v.exponent, steps: v.steps, fill: v.fill }, v.theme === "dark");
    assert.deepEqual(got, v.expect);
  });
}

test("FE-45: the mode opacity has no inversion and no reverse; a colour that is not a hex is read (rgba, token, text)", () => {
  const base = { ...COLOR, mode: "opacity", scale: "linear", steps: 3 };
  assert.deepEqual(heatPalette({ ...base, reverse: true }, true), heatPalette({ ...base, reverse: false }, true));
  assert.deepEqual(heatPalette({ ...base, fill: "rgba(1, 2, 3, 0.5)" }, true), ["#01020300", "#01020380", "#010203ff"]);
  assert.deepEqual(heatPalette({ ...base, fill: "#ff0000" }, false), ["#ff000000", "#ff000080", "#ff0000ff"]);
  assert.deepEqual(heatPalette({ ...base, fill: "text" }, true)[2], "#ccccdcff");
  assert.deepEqual(heatPalette({ ...base, fill: "text" }, false)[2], "#24292eff");
});

test("FE-45: the cap on the cells is a constant of 9.19e", () => {
  assert.equal(MAX_HEAT_CELLS, 250000);
});
