// FE-31, FE-32 and FE-34 (9.19c): the states, the segments, the cells, the geometry, the caps, the duration text and the legend.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { cellInk } from "../../web/js/panels/items.js";
import { parseCss, relativeLuminance } from "../../web/js/colors.js";
import { computeFieldConfig } from "../../web/js/fields.js";
import { baseDisplayNames } from "../../web/js/names.js";
import { CHAR_PX, MAX_TIMELINE_SEGMENTS, MIN_LABEL_PITCH, boxText, buildTimeline, cellWidth, durationText, gutterOf, labelChars, legendItems, limitMessage, rowBox } from "../../web/js/timeline_model.js";
import { toFields } from "./helpers.mjs";

const read = (name) => JSON.parse(readFileSync(new URL(`./vectors/${name}`, import.meta.url)));
const fe31 = read("FE-31-timeline-states.json");
const fe32 = read("FE-32-timeline-geometry.json");
const fe33 = read("FE-33-timeline-caps.json");
const fe34 = read("FE-34-timeline-text.json");

const T0 = Date.UTC(2026, 10, 13);
const MIN = 60000;

// The model of a vector. `cols` have no kind for a number. `range` is in minutes from T0 (default 0 to 300).
function modelOf(v) {
  const cols = [{ name: "time", kind: "time", values: v.time }, ...v.cols.map((c) => ({ ...c, kind: c.kind || "number" }))];
  const spec = { defaults: v.defaults || {}, overrides: (v.overrides || []).map((o) => ({ match: { by: "name", value: o.match }, set: o.set })) };
  const { fields } = computeFieldConfig(toFields(cols), spec, v.type);
  const range = v.range || { from: 0, to: 300 };
  const m = buildTimeline({ kind: v.type, fields, names: baseDisplayNames(fields), options: v.options || {}, range: { from: T0 + range.from * MIN, to: T0 + range.to * MIN } });
  return m;
}

const hex = (css) => {
  const p = parseCss(css);
  return [p.r, p.g, p.b];
};

// The boxes of a row in CSS pixels at the pixel ratio 1: the canvas is 982 px wide, the plot has 8 px on the right.
function pixels(v, m, row) {
  const o = v.options;
  const left = v.expect.left;
  const plotW = 982 - 8 - left;
  const [lo, hi] = m.x;
  const xOf = (t) => ((t - lo) * plotW) / (hi - lo);
  if (v.type === "status_history") {
    const cw = cellWidth((m.step * plotW) / (hi - lo), o.col_width, o.line_width);
    const w = Math.max(1, Math.round(cw));
    return row.t.map((t) => [Math.round(xOf(t) - w / 2), w]);
  }
  return row.t.map((t, j) => {
    const a = Math.round(xOf(t));
    return [a, Math.max(1, Math.round(xOf(row.e[j])) - a)];
  });
}

const everything = Object.entries(fe31.sets).flatMap(([set, list]) => list.map((v) => [set, v]));

test("FE-31 has 5 sets and 20 vectors from the measure of Grafana", () => {
  assert.deepEqual(Object.keys(fe31.sets), ["states", "merge", "span_nulls", "gaps", "cells"]);
  assert.equal(everything.filter(([, v]) => v.source.startsWith("grafana")).length, 20);
});

for (const [set, v] of everything) {
  test(`FE-31 ${set}: ${v.id} (${v.source})`, () => {
    const m = modelOf(v);
    assert.ok(!m.error, m.error);
    const e = v.expect;
    if (e.rows) {
      assert.equal(m.rows.length, e.rows.length, "rows");
      m.rows.forEach((row, k) => {
        const px = pixels(v, m, row);
        assert.equal(px.length, e.rows[k].length, `${v.id} row ${k}: the number of boxes`);
        px.forEach(([x, w], j) => {
          assert.ok(Math.abs(x - e.rows[k][j][0]) <= 1, `${v.id} row ${k} box ${j}: x ${x} against ${e.rows[k][j][0]}`);
          assert.ok(Math.abs(w - e.rows[k][j][1]) <= 1, `${v.id} row ${k} box ${j}: width ${w} against ${e.rows[k][j][1]}`);
          const got = hex(m.states[row.s[j]].dark);
          const want = e.rows[k][j][2];
          // The gradient of Grafana differs from the page by 3 levels at most.
          assert.ok(got.every((c, i) => Math.abs(c - want[i]) <= 3), `${v.id} row ${k} box ${j}: colour ${got} against ${want}`);
        });
      });
    }
    if (e.texts) {
      m.rows.forEach((row, k) => {
        const px = pixels(v, m, row);
        const got = px.map(([, w], j) => boxText(m.states[row.s[j]].text, w, v.options.show_values || "auto")).filter((t) => t !== "");
        assert.deepEqual(got, e.texts[k], `${v.id} row ${k}: the texts`);
      });
    }
    if (e.boxes) {
      assert.equal(m.rows.length, e.boxes.length);
      m.rows.forEach((row, k) => {
        const got = row.t.map((t, j) => [m.states[row.s[j]].text, (t - T0) / MIN, (row.e[j] - T0) / MIN]);
        assert.deepEqual(got, e.boxes[k], `${v.id} row ${k}`);
      });
    }
    if (e.colors) for (const [text, color] of Object.entries(e.colors)) assert.equal(m.states.find((s) => s.text === text).dark, color, text);
    if (e.x) assert.deepEqual(m.x.map((t) => (t - T0) / MIN), e.x);
  });
}

test("FE-31 the model reads the data by the rules of 9.19c section 1", () => {
  const model = (cols, type = "state_timeline", spec = {}) => {
    const { fields } = computeFieldConfig(toFields(cols), spec, type);
    return buildTimeline({ kind: type, fields, names: baseDisplayNames(fields), options: { merge_values: true }, range: { from: 0, to: 10000 } });
  };
  const time = { name: "t", kind: "time", values: [0, 1, 2] };
  assert.equal(model([{ name: "a", kind: "number", values: [1, 2, 3] }]).error, "The data has no time column.");
  assert.equal(model([time]).error, "The data has no field to draw.");
  assert.equal(model([time, { name: "t2", kind: "time", values: [0, 1, 2] }]).error, "The data has no field to draw.");
  assert.equal(model([time, { name: "n", kind: "null", values: [null, null, null] }]).error, "The data has no field to draw.");
  assert.equal(model([{ name: "t", kind: "time", values: [0] }, { name: "a", kind: "number", values: [1] }], "status_history").error, "A status history needs two samples at least.");
  // The first time field is the axis, a sample with a null time is dropped, the other kinds are rows.
  const m = model([{ name: "t", kind: "time", values: [2, null, 0, 1] }, { name: "a", kind: "number", values: [3, 9, 1, 2] }, { name: "s", kind: "text", values: ["x", "y", "z", "w"] }, { name: "b", kind: "bool", values: [true, true, false, false] }]);
  assert.deepEqual(m.rows.map((r) => r.name), ["a", "s", "b"]);
  assert.deepEqual(m.rows[0].t, [0, 1000, 2000]);
  assert.equal(m.samples, 3);
  // A row with hide_from.viz is not drawn.
  const hidden = model([time, { name: "a", kind: "number", values: [1, 2, 3] }, { name: "b", kind: "number", values: [1, 2, 3] }], "state_timeline", { overrides: [{ match: { by: "name", value: "b" }, set: { hide_from: { legend: false, tooltip: false, viz: true } } }] });
  assert.deepEqual(hidden.rows.map((r) => r.name), ["a"]);
});

test("FE-31 a state is the text and the colour in both themes, and the key merges equal states", () => {
  const { fields } = computeFieldConfig(toFields([{ name: "t", kind: "time", values: [0, 1, 2, 3] }, { name: "a", kind: "number", values: [1, 2, 5, 9] }]), { defaults: { mappings: [{ kind: "range", from: 0, to: 5, text: "LOW", color: "green" }] } }, "state_timeline");
  const m = buildTimeline({ kind: "state_timeline", fields, names: ["a", "a"], options: { merge_values: true }, range: { from: 0, to: 10000 } });
  assert.equal(m.states[0].text, "LOW");
  assert.equal(m.states[0].dark, "#73BF69");
  assert.equal(m.states[0].light, "#56A64B");
  assert.equal(m.rows[0].t.length, 2, "three samples of one state merge: LOW then the unmapped 9");
});

// --- FE-32 ------------------------------------------------------------------------------------------------------------------

test("FE-32 the rows: 160 vectors for 1 to 40 rows and 4 values of row_height", () => {
  assert.equal(fe32.rows.length, 160);
  for (const v of fe32.rows) {
    const got = Array.from({ length: v.n }, (_, k) => rowBox(k, v.n, v.plot_height, v.row_height)).map((b) => [b.y, b.h]);
    // A top that is exactly at a half pixel may round either way: the float sum of the page decides (the same as the code of Grafana).
    const half = new Set(v.half || []);
    got.forEach(([y, h], k) => {
      assert.equal(h, v.boxes[k][1], `${v.n} rows at ${v.row_height}: height of row ${k}`);
      assert.ok(half.has(k) ? Math.abs(y - v.boxes[k][0]) <= 1 : y === v.boxes[k][0], `${v.n} rows at ${v.row_height}: top of row ${k}`);
    });
  }
  for (const v of fe32.measured) {
    const got = Array.from({ length: v.n }, (_, k) => rowBox(k, v.n, v.plot_height, v.row_height));
    if (v.boxes) assert.deepEqual(got.map((b) => [b.y, b.h]), v.boxes, v.source);
    else assert.ok(got.every((b) => b.h === v.h), v.source);
  }
  // The first bar touches the top and the last bar touches the bottom of the plot.
  for (const n of [2, 3, 15, 40]) {
    assert.equal(rowBox(0, n, 213, 90).y, 0);
    const last = rowBox(n - 1, n, 213, 90);
    assert.ok(Math.abs(last.y + last.h - 213) <= 1, `${n}`);
  }
});

test("FE-32 the left gutter, the width of a cell and the characters that fit", () => {
  for (const v of fe32.gutter) assert.equal(gutterOf(v.width, v.show), v.expect, JSON.stringify(v));
  for (const v of fe32.cells) assert.ok(Math.abs(cellWidth(v.step_px, v.col_width, v.line_width) - v.expect) < 1e-9, JSON.stringify(v));
  assert.equal(fe32.chars.length, 60);
  for (const v of fe32.chars) assert.equal(labelChars(v.width), v.expect, `${v.width}`);
  for (const v of fe32.charsMeasured) assert.equal(boxText(v.text, v.width, v.mode || "auto"), v.expect, JSON.stringify(v));
  assert.equal(CHAR_PX, 8.5);
  assert.equal(MIN_LABEL_PITCH, 12);
});

test("FE-32 the ink on each fill has 4.5:1 over the panel in both themes", () => {
  const lum = (css) => relativeLuminance(css);
  const ratio = (a, b) => (Math.max(lum(a), lum(b)) + 0.05) / (Math.min(lum(a), lum(b)) + 0.05);
  let checked = 0;
  for (const dark of [true, false]) {
    const bg = parseCss(dark ? fe32.panel.dark : fe32.panel.light);
    for (const fill of fe32.ink) {
      const p = parseCss(fill);
      const mix = (f, b) => Math.round(b * 0.3 + f * 0.7);
      const shown = `rgb(${mix(p.r, bg.r)}, ${mix(p.g, bg.g)}, ${mix(p.b, bg.b)})`;
      assert.ok(ratio(cellInk(shown), shown) >= 4.5, `${fill} in the ${dark ? "dark" : "light"} theme`);
      checked += 1;
    }
  }
  assert.equal(checked, 80);
});

// --- FE-33 (model part) -----------------------------------------------------------------------------------------------------

function capModel(v) {
  const time = { name: "t", kind: "time", values: Array.from({ length: v.samples }, (_, i) => i) };
  const cols = [time, ...Array.from({ length: v.rows }, (_, r) => ({ name: `r${r}`, kind: "number", values: Array.from({ length: v.samples }, (_, i) => (i + r) % 2) }))];
  const { fields } = computeFieldConfig(toFields(cols), {}, v.kind);
  const names = baseDisplayNames(fields);
  const t0 = Date.now();
  const m = buildTimeline({ kind: v.kind, fields, names, options: { merge_values: true }, range: { from: 0, to: v.samples * 1000 } });
  return { m, ms: Date.now() - t0 };
}

test("FE-33 the cap: 9,999 and 10,000 pass, 10,001 fails closed with its message", () => {
  assert.equal(MAX_TIMELINE_SEGMENTS, 10000);
  for (const v of fe33.caps) {
    const { m } = capModel(v);
    assert.equal(m.count, v.count, v.name);
    assert.equal(m.over, v.over, v.name);
    assert.equal(limitMessage(m, 5000), v.over ? v.message : "", v.name);
    if (v.over) assert.equal(m.rows.reduce((n, r) => n + r.t.length, 0), v.count, "the page never cuts the data to fit the cap");
  }
});

test("FE-33 the rule of half the width of a status history: 491 pass and 492 fail at 982 px", () => {
  for (const v of fe33.width) {
    const { m } = capModel({ kind: "status_history", rows: 1, samples: v.samples });
    assert.equal(limitMessage(m, v.width), v.message, JSON.stringify(v));
  }
  const { m } = capModel({ kind: "state_timeline", rows: 1, samples: 600 });
  assert.equal(limitMessage(m, 100), "", "a state timeline has no rule of the width");
});

test("FE-33 the model of 10,000 samples builds in 50 ms and the model of 100,000 samples in 200 ms (16.10a)", () => {
  const best = (v) => Math.min(...[0, 1, 2].map(() => capModel(v).ms));
  assert.ok(best({ kind: "state_timeline", rows: 1, samples: 10000 }) < 50, "10,000 samples");
  assert.ok(best({ kind: "state_timeline", rows: 1, samples: 100000 }) < 200, "100,000 samples");
});

// --- FE-34 ------------------------------------------------------------------------------------------------------------------

test("FE-34 the duration text: 60 durations from 0 ms to 3 years", () => {
  assert.equal(fe34.durations.length, 60);
  for (const [ms, text] of fe34.durations) assert.equal(durationText(ms), text, `${ms} ms`);
  assert.equal(durationText(NaN), "");
  assert.equal(durationText(-1), "");
});

test("FE-34 the legend for each colour mode", () => {
  for (const v of fe34.legend) {
    const cols = [{ name: "time", kind: "time", values: v.time }, ...v.cols.map((c) => ({ ...c, kind: c.kind || "number" }))];
    const spec = { defaults: v.defaults, overrides: (v.overrides || []).map((o) => ({ match: { by: "name", value: o.match }, set: o.set })) };
    const { fields } = computeFieldConfig(toFields(cols), spec, v.type);
    const m = buildTimeline({ kind: v.type, fields, names: baseDisplayNames(fields), options: { merge_values: true }, range: { from: T0, to: T0 + 300 * MIN } });
    assert.deepEqual(legendItems(m).map((i) => i.text), v.expect, v.name);
  }
});

test("FE-34 an item of the legend has its colour in both themes", () => {
  const v = fe34.legend[0];
  const cols = [{ name: "time", kind: "time", values: v.time }, ...v.cols.map((c) => ({ ...c, kind: "number" }))];
  const { fields } = computeFieldConfig(toFields(cols), { defaults: v.defaults }, v.type);
  const items = legendItems(buildTimeline({ kind: v.type, fields, names: ["a"], options: {}, range: { from: T0, to: T0 + 300 * MIN } }));
  assert.deepEqual(items.map((i) => i.dark), ["#73BF69", "#FF9830", "#F2495C"]);
  assert.deepEqual(items.map((i) => i.light), ["#56A64B", "#FF780A", "#E02F44"]);
});
