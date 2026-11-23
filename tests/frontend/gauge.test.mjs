// FE-84: the gauge panel (9.19g, 4.33e). The vectors are the measures of the classic gauge of Grafana 12.4.2 (58 gauges of 34 panels,
// dark theme): the cells, the canvas boxes, the arcs, the text sizes and places, the threshold labels and the titles.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { angleOf, arcEntry, autoSize, bandEntries, cellParts, drawGauge, gaugeGeometry, gridCells, mix, scaleOf, stackCells } from "../../web/js/panels/gauge.js";
import { resolveFill } from "../../web/js/colors.js";

const { vectors } = JSON.parse(readFileSync(new URL("./vectors/FE-84-gauge.json", import.meta.url), "utf8"));
// The tolerances are the rounding of the measures (0.01 px) and the layout unit of the browser (1/64 px, added over a column).
const near = (a, b, tol, what) => assert.ok(Math.abs(a - b) <= tol, `${what}: ${a} vs ${b} (tolerance ${tol})`);

// The thresholds of a Grafana file in the form of the document: the first step has the value null.
const docThresholds = (t) => ({ mode: t.mode, steps: t.steps.map((s, i) => ({ value: i === 0 ? null : s.value, color: s.color })) });

// The plan of a vector: the cells, and for each gauge its parts, its scale, its entries and its geometry.
function plan(v) {
  const n = v.gauges.length;
  const o = v.options;
  const [W, H] = v.box;
  const vertical = o.orientation === "vertical";
  const manual = o.sizing === "manual";
  const min = vertical ? (manual ? o.minW : 75) : manual && o.orientation === "horizontal" ? o.minH : 75;
  const cells = o.orientation === "auto" ? gridCells(n, W, H) : stackCells(n, W, H, vertical, min);
  const title = n > 1 || !!v.defaults.displayName;
  const range = v.data.length ? { min: Math.min(...v.data), max: Math.max(...v.data) } : { min: null, max: null };
  const config = { unit: v.defaults.unit, set: { min: v.defaults.min, max: v.defaults.max } };
  const scale = v.data.length ? scaleOf(config, range) : { min: v.defaults.min ?? 0, max: v.defaults.max ?? 0 };
  const thresholds = docThresholds(v.thresholds);
  const entries = v.colorMode === "thresholds" ? bandEntries(thresholds, scale.min, scale.max, v.defaults.decimals ?? 2) : [{ value: scale.min, color: "fixed" }];
  return v.gauges.map((g, i) => {
    const parts = cellParts({ w: cells[i].w, h: cells[i].h, title, orientation: o.orientation, titleSize: o.titleSize ?? null });
    const bands = o.markers && entries.length > 1;
    const geo = gaugeGeometry({ cw: parts.canvasW, ch: parts.canvasH, sizeW: parts.sizeW, sizeH: parts.sizeH, w14: g.w14[0], labels: o.labels, markers: o.markers, bands, valueSize: o.valueSize ?? null });
    return { cell: cells[i], parts, geo, scale, entries, bands, title, thresholds };
  });
}

for (const v of vectors) {
  test(`FE-84 ${v.name}: cells, canvas box, arcs and text sizes as Grafana 12.4.2`, () => {
    const plans = plan(v);
    v.gauges.forEach((g, i) => {
      const { cell, parts, geo, bands } = plans[i];
      const tag = `${v.name}#${i}`;
      near(cell.x, g.canvas[0], 0.05, `${tag} canvas x`);
      near(cell.y + parts.top, g.canvas[1], 0.05, `${tag} canvas y`);
      near(parts.canvasW, g.canvas[2], 0.02, `${tag} canvas width`);
      near(parts.canvasH, g.canvas[3], 0.02, `${tag} canvas height`);
      near(geo.cx, g.cx, 0.01, `${tag} centre x`);
      near(geo.cy, g.cy, 0.02, `${tag} centre y`);
      near(geo.radius, g.radius, 0.02, `${tag} radius`);
      near(geo.arcW, g.arcW, 0.01, `${tag} arc width`);
      near(geo.valueSize, g.valueSize, 0.001, `${tag} value size`);
      near(geo.cy - geo.valueSize / 2, g.valueTop, 0.02, `${tag} value top`);
      // The value text has its middle at the integer part of the centre (Grafana: parseInt of the left), within the rounding of its width.
      near(Math.trunc(geo.cx), g.valueCenterX, 0.6, `${tag} value centre`);
      // The fills: the track, then the value arc (radius - 1) when it shows, the neutral mark, then the bands (radius + band width).
      const bandFills = g.fills.filter((f) => f.r > geo.radius + 0.5);
      assert.equal(bandFills.length > 0, bands, `${tag} threshold band`);
      for (const f of bandFills) {
        near(f.r, geo.radius + geo.markW, 0.01, `${tag} band outer radius`);
        near(f.r - f.r2, geo.markW - 2, 0.01, `${tag} band width`);
      }
    });
  });
}

test("FE-84: the angles of the value arcs and of the band parts are the angles of Grafana", () => {
  let arcs = 0;
  for (const v of vectors) {
    const plans = plan(v);
    v.gauges.forEach((g, i) => {
      const { geo, entries, scale } = plans[i];
      const o = v.options;
      let { min, max } = scale;
      // One gauge of one value: the raw value (a unit can change the text); else the text of the gauge.
      let value = v.gauges.length === 1 && v.data.length === 1 ? v.data[0] : Number(g.text.replace(/[^0-9.-]/g, ""));
      if (plans[i].thresholds.mode === "percentage") [value, min, max] = [((value - min) / (max - min)) * 100, 0, 100];
      if (o.markers) [min, max] = [parseFloat(min.toFixed(v.defaults.decimals ?? 2)), parseFloat(max.toFixed(v.defaults.decimals ?? 2))];
      const valueFill = g.fills.find((f) => Math.abs(f.r - (geo.radius - 1)) < 0.01);
      const neutral = v.defaults.neutral ?? null;
      if (valueFill && neutral === null && valueFill.a2 !== null) {
        near(angleOf(value, min, max), valueFill.a2, 0.0002, `${v.name}#${i} value angle`);
        // The colour of the arc: the entry that holds the value (the bands of Grafana, dark theme).
        if (v.colorMode === "thresholds") assert.equal(resolveFill(arcEntry(entries, value).color, true).toLowerCase(), valueFill.color, `${v.name}#${i} arc colour`);
        arcs++;
      }
      if (valueFill && neutral !== null) {
        const [a1, a2] = value < neutral ? [angleOf(value, min, max), angleOf(neutral, min, max)] : [angleOf(neutral, min, max), angleOf(value, min, max)];
        near(a1, valueFill.a1, 0.0002, `${v.name} neutral a1`);
        near(a2, valueFill.a2, 0.0002, `${v.name} neutral a2`);
        arcs++;
      }
      const bandFills = g.fills.filter((f) => f.r > geo.radius + 0.5 && f.a1 !== null && f.a2 !== null);
      for (const f of bandFills) {
        const at = entries.findIndex((e) => Math.abs(angleOf(e.value, min, max) - f.a2) < 0.0002);
        assert.ok(at >= 0, `${v.name}#${i} band end ${f.a2}`);
        if (v.colorMode === "thresholds") assert.equal(resolveFill(entries[at].color, true).toLowerCase(), f.color, `${v.name}#${i} band colour`);
      }
    });
  }
  assert.ok(arcs >= 25, `${arcs} value arcs compared`);
});

test("FE-84: the threshold labels have the place, the size and the text of Grafana", () => {
  let count = 0;
  for (const v of vectors.filter((x) => x.labels.length)) {
    const [{ geo, scale, entries }] = plan(v);
    const d = v.defaults.decimals ?? 2;
    let { min, max } = scale;
    if (v.thresholds.mode === "percentage") [min, max] = [0, 100];
    if (v.options.markers) [min, max] = [parseFloat(min.toFixed(d)), parseFloat(max.toFixed(d))];
    // The labels that drawGauge writes, in the order of its calls, with their centres.
    const calls = [];
    const ctx = fakeContext(calls);
    drawGauge(ctx, geo, { min, max, entries, value: 0, neutral: v.defaults.neutral ?? null, labels: true, bands: false, colors: { track: "#000", arc: "#111", value: "#222", label: "#333" }, label: "x", family: "Inter" });
    const drawn = calls.filter((c) => c.text !== "x");
    assert.equal(drawn.length, v.labels.length, `${v.name} label count`);
    for (const l of v.labels) {
      const c = drawn.find((x) => x.text === l.text && Math.abs(x.x - l.center[0]) < 0.6 && Math.abs(x.y - l.center[1]) < 0.6);
      assert.ok(c, `${v.name} label ${l.text} at ${l.center} (drawn: ${JSON.stringify(drawn)})`);
      near(c.size, l.size, 0.001, `${v.name} label size`);
      count++;
    }
  }
  assert.ok(count >= 15, `${count} labels compared`);
});

test("FE-84: the titles have the size and the box of Grafana", () => {
  let count = 0;
  for (const v of vectors.filter((x) => x.titles.length)) {
    const plans = plan(v);
    v.titles.forEach((t, i) => {
      const { cell, parts } = plans[i];
      near(parts.titleSize, t.size, 0.001, `${v.name} title size`);
      near(cell.y + parts.titleY, t.y, 0.03, `${v.name} title y`);
      near(parts.titleH, t.h, 0.03, `${v.name} title height`);
      near(cell.x, t.x, 0.02, `${v.name} title x`);
      near(parts.titleW, t.w, 0.02, `${v.name} title width`);
      count++;
    });
  }
  assert.ok(count >= 20, `${count} titles compared`);
});

// A context that records the texts of drawGauge with the centre of each one (the translation) and its size.
function fakeContext(calls) {
  let at = [0, 0];
  let font = "";
  return {
    save() {}, restore() { at = [0, 0]; }, beginPath() {}, arc() {}, lineTo() {}, closePath() {}, stroke() {}, fill() {}, rotate() {},
    translate(x, y) { at = [x, y]; },
    set font(f) { font = f; }, get font() { return font; },
    measureText() { return { fontBoundingBoxAscent: 10, fontBoundingBoxDescent: 3 }; },
    fillText(text) { calls.push({ text, x: at[0], y: at[1], size: parseFloat(font.split(" ")[1]) }); },
  };
}

test("FE-84: the pure rules of the scale, the entries and the grid", () => {
  // An empty scale (one value, no min and no max) gives no angle: Grafana draws the track and the text only.
  assert.ok(Number.isNaN(angleOf(42, 42, 42)));
  assert.equal(angleOf(-20, 0, 100), 0.9);
  assert.equal(angleOf(150, 0, 100), 2.1);
  // The scale: the document, else the percent units, else the range of all the number fields.
  assert.deepEqual(scaleOf({ unit: "percent", set: {} }, { min: 3, max: 9 }), { min: 0, max: 100 });
  assert.deepEqual(scaleOf({ unit: "percentunit", set: { max: 2 } }, { min: 3, max: 9 }), { min: 0, max: 2 });
  assert.deepEqual(scaleOf({ unit: "none", set: { min: 1 } }, { min: 3, max: 9 }), { min: 1, max: 9 });
  assert.deepEqual(scaleOf({ unit: "none", set: {} }, { min: null, max: null }), { min: 0, max: 0 });
  // The entries of the band: each entry has the colour of the part that ends at its value.
  const t = { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 100, color: "#EAB839" }, { value: 180, color: "red" }] };
  assert.deepEqual(bandEntries(t, 10, 210, 2), [{ value: 10, color: "green" }, { value: 100, color: "green" }, { value: 180, color: "#EAB839" }, { value: 210, color: "red" }]);
  const two = { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 80, color: "red" }] };
  assert.deepEqual(bandEntries(two, 42, 42, 2), [{ value: 42, color: "green" }, { value: 42, color: "green" }, { value: 80, color: "green" }]);
  assert.deepEqual(bandEntries(two, 20, 80, 2), [{ value: 20, color: "green" }, { value: 80, color: "green" }]);
  // A value on a step has the colour of the part below in the arc (Grafana), and the colour of the step in the text.
  assert.equal(arcEntry(bandEntries(two, 20, 80, 2), 80).color, "green");
  assert.equal(arcEntry(bandEntries(two, 0, 100, 2), 90).color, "red");
  // The grid of the orientation auto: 4 cells in 762 x 238 make one row, 5 in 762 x 238 one row too.
  assert.deepEqual(gridCells(4, 762, 238).map((c) => [c.x, c.w]), [[0, 184.5], [192.5, 184.5], [385, 184.5], [577.5, 184.5]]);
  // A column of 6 cells in 238 px: each cell keeps 75 px, so the panel scrolls.
  const col = stackCells(6, 368, 238, false);
  assert.equal(col[0].h, 75);
  assert.equal(col[5].y, 5 * 83);
  assert.deepEqual(autoSize(377, 238, true), { titleSize: 20, gaugeH: 208 });
  assert.equal(mix("#ffffff", "#000000", 0.06), "rgb(15, 15, 15)");
  assert.equal(mix("var(--text)", "#000000", 0.06), "#000000");
});

// The option form of the gauge (10.3h): the write rule of the optional members and the rules of display.
test("FE-84: the form of the gauge writes the optional members and removes a default", async () => {
  const { default: table } = await import("../../web/js/editor/forms/options/gauge.js");
  const R = await import("../../web/js/editor/forms/options/rows.js");
  const row = (id) => table.rows.find((r) => r.id === id);
  const panel = { id: 1, type: "gauge", title: "t", options: { reduce: { calc: "last_not_null", fields: null, values: false } }, fields: { defaults: {}, overrides: [] }, links: [] };
  const write = (p, id, v) => R.writeOption(p, row(id), v);
  assert.deepEqual(table.rows.map((r) => r.id), ["reduce-values", "reduce-calc", "reduce-fields", "orientation", "threshold-labels", "threshold-markers", "sizing", "min-width", "min-height", "neutral", "title-size", "value-size", "display-name"]);
  // A value that is not the default is written, in the order of 4.33e; the default removes the member.
  let p = write(write(write(panel, "threshold-markers", false), "orientation", "vertical"), "neutral", 30);
  assert.equal(JSON.stringify(p.options), JSON.stringify({ reduce: panel.options.reduce, orientation: "vertical", show_threshold_markers: false, neutral: 30 }));
  p = write(write(write(p, "threshold-markers", true), "orientation", "auto"), "neutral", null);
  assert.deepEqual(p.options, panel.options);
  assert.deepEqual(write(panel, "min-width", 75).options, panel.options);
  // The rules of display: the size choice with a row or a column, the least sizes with the manual size and their orientation.
  assert.ok(!R.shownIds(table, panel.options).includes("sizing"));
  assert.ok(R.shownIds(table, { ...panel.options, orientation: "horizontal" }).includes("sizing"));
  assert.deepEqual(R.shownIds(table, { ...panel.options, orientation: "vertical", sizing: "manual" }).filter((id) => id.startsWith("min-")), ["min-width"]);
  assert.deepEqual(R.shownIds(table, { ...panel.options, orientation: "horizontal", sizing: "manual" }).filter((id) => id.startsWith("min-")), ["min-height"]);
  // The ranges: the text sizes 1 to 200 (whole numbers), the least sizes 0 to 600, any finite neutral value.
  assert.ok(R.parseText(row("title-size"), "0").error && R.parseText(row("value-size"), "201").error && R.parseText(row("title-size"), "12.5").error);
  assert.deepEqual(R.parseText(row("title-size"), ""), { value: null });
  assert.deepEqual(R.parseText(row("neutral"), "-2.5"), { value: -2.5 });
  assert.equal(R.checkSlider(row("min-height"), 601), "Use a whole number from 0 to 600.");
  assert.equal(R.checkSlider(row("min-height"), 0), "");
});

// Security review 3, finding F3 (9.19g, rule 1.1): with `reduce.values`, the gauge reads only the 25 rows that it shows. The reduction stops
// at 25 items in reduceItems: a frame of 100,000 rows gives 25 cells, and the label column is read at the rows 0 to 24 only.
test("FE-84: a gauge with reduce.values reads 25 rows of a frame of 100,000 rows", async () => {
  const { FakeNode, installFakeDom } = await import("./fake_dom.mjs");
  installFakeDom();
  globalThis.window = { devicePixelRatio: 1 };
  globalThis.getComputedStyle = () => ({ getPropertyValue: () => "" });
  globalThis.ResizeObserver = class { observe() {} disconnect() {} };
  FakeNode.prototype.getContext = () => ({ measureText: () => ({ width: 10 }) });
  const { framesToFields } = await import("../../web/js/frame.js");
  const { computeFieldConfig } = await import("../../web/js/fields.js");
  const { render } = await import("../../web/js/panels/gauge.js");
  const rows = 100000;
  const answer = { columns: [{ name: "host", type: "String", kind: "text" }, { name: "v", type: "Float64", kind: "number" }], values: [Array.from({ length: rows }, (_, r) => `host-${r}`), Array.from({ length: rows }, (_, r) => r * 1.5)] };
  const { fields } = computeFieldConfig(framesToFields(answer), { defaults: {}, overrides: [] }, "gauge");
  // The rows of the label column that the panel reads.
  const read = new Set();
  const labels = fields[0].values;
  fields[0].values = new Proxy(labels, { get: (t, k) => { if (typeof k === "string" && /^[0-9]+$/.test(k)) read.add(Number(k)); return Reflect.get(t, k); } });
  const host = new FakeNode("div");
  host.getBoundingClientRect = () => ({ left: 0, top: 0, width: 0, height: 0 }); // no box: no layout, the cells only
  const panel = { id: 9, type: "gauge", title: "Rows", fields: { defaults: {}, overrides: [] }, options: { reduce: { calc: "last_not_null", fields: null, values: true } } };
  const names = fields.map((f) => f.name);
  const handle = render(host, { panel, fields, names, baseNames: names, zone: "utc" });
  const cells = host.querySelectorAll(".gauge__item");
  assert.equal(cells.length, 25);
  assert.deepEqual(cells.map((c) => c.querySelector(".gauge__title").textContent), Array.from({ length: 25 }, (_, r) => `host-${r}`));
  assert.ok(read.size <= 25 && Math.max(...read) === 24, `rows read: ${read.size}, the last ${Math.max(...read)}`);
  handle.destroy();
});
