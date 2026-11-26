// SPDX-License-Identifier: MIT OR Apache-2.0
// The loading previews of the values families (9.11): `stat`, `gauge` and `bargauge`. One test for each rule of the council files
// (scratchpad/council/stat.md, gauge.md, bargauge.md), with the member of the document that decides it, the neutral form when the document does not
// decide, and the bounds. The pure functions of the loaded gauge give the expected numbers, so the preview and the loaded panel have one source.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { DATA, EXACT, LEAST, boundHolds, setMeasurer } from "../../web/js/preview/common.js";
import { previewOf } from "../../web/js/preview/index.js";
import { gaugeLayout, configOf, chainOf, gaugeCount, ringPath, titleText } from "../../web/js/preview/gauge.js";
import { computeFieldConfig } from "../../web/js/fields.js";
import { angleOf, bandEntries, cellParts, gaugeGeometry, gridCells, stackCells } from "../../web/js/panels/gauge.js";
import { warningLines, warningSentences } from "../../web/js/preview/stat.js";

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");
// A measurer: 6 px for each character, whatever the font.
setMeasurer((text) => text.length * 6);

const reduce = (over = {}) => ({ calc: "last", fields: null, values: false, ...over });
const calc = (mode, over = {}) => ({ kind: "calculate_field", mode, alias: null, replace_fields: true, ...over });
const ONE = [calc("reduce_row", { reduce: { reducer: "total", include: [] } })];
const query = (over = {}) => ({ query: { source: "s", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null, ...over } });
const stat = (options = {}, over = {}) => ({ id: 1, type: "stat", title: "t", data: query(), transforms: [], fields: { defaults: {}, overrides: [] }, options: { reduce: reduce(), color_mode: "value", graph_mode: "none", text_mode: "auto", orientation: "auto", ...options }, ...over });
const barGauge = (options = {}, over = {}) => ({ id: 1, type: "bargauge", title: "t", data: query(), transforms: [], fields: { defaults: {}, overrides: [] }, options: { reduce: reduce(), orientation: "horizontal", display_mode: "gradient", show_unfilled: true, ...options }, ...over });
const gauge = (options = {}, defaults = { min: 0, max: 400 }, over = {}) => ({ id: 1, type: "gauge", title: "t", data: query(), transforms: ONE, fields: { defaults, overrides: [] }, options: { reduce: reduce(), ...options }, ...over });
const out = (panel, w, h, doc = { sections: [] }, extra = {}) => previewOf(panel, { w: Math.floor(w), h: Math.floor(h), ...extra }, doc, null);
// All the nodes of a result, the children included, in the order of the tree.
const flat = (r) => {
  const all = [];
  const walk = (n) => {
    all.push(n);
    (n.k || []).forEach(walk);
  };
  r.elements.forEach(walk);
  return all;
};
const parts = (r) => flat(r).map((e) => e.a && e.a["data-skel-part"]).filter(Boolean);
const part = (r, name, i = 0) => flat(r).filter((e) => e.a && e.a["data-skel-part"] === name)[i];
const num = (n, name) => parseFloat(n.v[name]);
const rectOf = (n) => [num(n, "--x"), num(n, "--y"), num(n, "--w"), num(n, "--h")];
// A px value of the tree has three decimals at most (px() of common.js).
const near = (a, b, tol = 0.002) => assert.ok(Math.abs(a - b) <= tol, `${a} is not ${b} within ${tol}`);

// ---- stat ----------------------------------------------------------------------------------------------------------------------------

test("stat E1: the preview is the box of the body, and the document adds nothing else", () => {
  const r = out(stat(), 338, 102);
  assert.deepEqual(parts(r), ["holder"]);
  assert.equal(r.elements.length, 1, "no tile, no gap, no text slot, no tint");
  assert.equal(part(r, "holder").a["data-skel-class"], EXACT);
  assert.equal(part(r, "holder").v["--y"], "0px", "with no warning line the box starts at the top of the body");
  assert.equal(r.bounds.holder.value, 102);
  assert.deepEqual(r.notes, ["box"]);
});

test("stat E3 to E29: no option, no transform and no proof of one tile changes the tree (R3, R4, R5, R6)", () => {
  const base = JSON.stringify(out(stat(), 322, 86).elements);
  const options = [
    { orientation: "vertical" }, { orientation: "horizontal" }, { text_mode: "value_and_name", justify: "center" }, { text_mode: "none" }, { text_mode: "name" },
    { graph_mode: "area", reduce: reduce({ values: true }) }, { graph_mode: "area" }, { color_mode: "background" }, { color_mode: "none" }, { value_size: 40, title_size: 14 },
    { percent_change: true, percent_change_color: "inverted" }, { reduce: reduce({ values: true, fields: "/^a$/" }) },
  ];
  for (const o of options) assert.equal(JSON.stringify(out(stat(o), 322, 86).elements), base, JSON.stringify(o));
  // Proof A (one tile): the last transform is a calculate_field that replaces the fields. The fill is a colour of the document, so it is never drawn.
  const fixed = stat({ color_mode: "background" }, { transforms: ONE, fields: { defaults: { color: { mode: "fixed", fixed: "red" } }, overrides: [] } });
  assert.equal(JSON.stringify(out(fixed, 322, 86).elements), base, "no fill with proof A and a certain colour");
  assert.ok(!JSON.stringify(out(fixed, 322, 86)).includes("gradient"));
});

test("stat E1: the box follows the size of the body, and a fraction does not change the claim", () => {
  for (const [w, h] of [[322, 86], [338, 102], [170, 160], [600, 40]]) {
    const r = out(stat(), w, h, { sections: [] }, { fw: w + 0.328125, fh: h + 0.5 });
    assert.equal(r.bounds.holder.value, h);
    assert.equal(part(r, "holder").v["--y"], "0px");
  }
});

// ---- the warning line of the three families ---------------------------------------------------------------------------------------------

const withOverrides = (panel, list) => ({ ...panel, fields: { ...panel.fields, overrides: list.map((value) => ({ match: { by: "regex", value }, set: { unit: "percent" } })) } });

test("warning E2: the sentences are the ones of computeFieldConfig, one for each distinct bad pattern, in order", () => {
  const p = withOverrides(stat(), ["/(/", "/[/", "/(/", "^ok$", "/a(b/"]);
  const own = warningSentences(p);
  const loaded = computeFieldConfig([{ kind: "number", name: "x", labels: null, values: new Float64Array(0), config: {} }], p.fields, "stat").warnings;
  assert.deepEqual(own, loaded);
  assert.equal(own.length, 3);
  assert.deepEqual(warningSentences(withOverrides(stat(), ["^ok$", "/a/i"])), [], "a pattern that compiles gives no warning");
  const byName = { ...stat(), fields: { defaults: {}, overrides: [{ match: { by: "name", value: "/(/" }, set: { unit: "percent" } }] } };
  assert.deepEqual(warningSentences(byName), [], "only the matcher regex is a pattern");
});

test("warning R2: the stat holds the paragraph in the padding and the grid starts at its height", () => {
  const p = withOverrides(stat(), ["/(/"]);
  const r = out(p, 338, 102);
  const w = part(r, "warning");
  const [x, y, width, height] = rectOf(w);
  assert.deepEqual([x, y, width], [8, 8, 322], "the paragraph is in the padding, 16 px narrower than the body");
  // The padding (2 + 2) and the line boxes, 12 px at the ratio 1.5714 rounded down to 1/64 px. The measurer gives 6 px for each character.
  const lines = warningLines(warningSentences(p), 322).length;
  assert.equal(lines, 2);
  near(height, 4 + lines * (Math.floor(12 * 1.5714 * 64) / 64));
  near(num(part(r, "holder"), "--y"), height, 0.001);
  near(r.bounds.holder.value, 102 - height, 0.001);
  assert.equal(w.a["data-skel-class"], EXACT);
  assert.deepEqual(r.notes, ["box warning"]);
});

test("warning R2: the gauge and the bar gauge hold the paragraph at the top of the plot", () => {
  for (const [panel, type] of [[withOverrides(gauge(), ["/(/"]), "gauge"], [withOverrides(barGauge(), ["/(/"]), "bargauge"]]) {
    const r = out(panel, 322, 200);
    const [x, y, width, height] = rectOf(part(r, "warning"));
    assert.deepEqual([x, y, width], [0, 0, 322], type);
    near(num(part(r, "holder"), "--y"), height, 0.001);
    near(r.bounds.holder.value, 200 - height, 0.001);
  }
});

test("warning: the lines wrap by the measured width, the first line holds the icon, and each line is a bar of its width", () => {
  const sentences = ["The override pattern /( is not a valid regular expression."];
  const wide = warningLines(sentences, 1000);
  assert.equal(wide.length, 1);
  assert.equal(wide[0].x, 14, "the first bar starts at the right edge of the icon of 14 px");
  const text = sentences[0];
  near(wide[0].w, 6 + text.length * 6 - 0, 6.001);
  const narrow = warningLines(sentences, 120);
  assert.ok(narrow.length > 1);
  for (const l of narrow.slice(1)) assert.equal(l.x, 0);
  for (const l of narrow) assert.ok(l.x + l.w <= 120 + 1e-9 || l.w <= 6 * 20, "a line stays in the width, unless one word is wider");
  // A word that is wider than the line stays on its own line and overflows.
  const long = warningLines(["x".repeat(100)], 100);
  assert.equal(long.length, 2, "the icon stays alone on the first line");
  assert.deepEqual(warningLines([], 100), []);
  // The bars are in the tree, one for each line with text.
  const r = out(withOverrides(stat(), ["/(/"]), 122, 102);
  const bars = r.elements.filter((e) => e.c === "skel__b");
  assert.equal(bars.length, warningLines(warningSentences(withOverrides(stat(), ["/(/"])), 106).length);
  for (const b of bars) assert.equal(b.a["data-skel-part"], "label");
});

// ---- bar gauge --------------------------------------------------------------------------------------------------------------------------

test("bargauge E1, R1, R3: the preview is the box, whatever the option, the reduce and the transforms (O1: no item is proven)", () => {
  const base = JSON.stringify(out(barGauge(), 446, 86).elements);
  const r = out(barGauge(), 446, 86);
  assert.deepEqual(parts(r), ["holder"]);
  assert.equal(r.elements.length, 1);
  assert.equal(r.bounds.holder.value, 86);
  for (const orientation of ["horizontal", "vertical"]) {
    for (const display_mode of ["basic", "gradient", "lcd"]) {
      for (const show_unfilled of [true, false]) {
        for (const reduction of [reduce(), reduce({ values: true }), reduce({ fields: "/^a$/" }), reduce({ fields: "/(/" })]) {
          for (const transforms of [[], ONE, [{ kind: "limit", count: 0, end: "first" }]]) {
            const p = barGauge({ orientation, display_mode, show_unfilled, reduce: reduction }, { transforms });
            assert.equal(JSON.stringify(out(p, 446, 86).elements), base, `${orientation} ${display_mode} ${show_unfilled} ${JSON.stringify(reduction)}`);
          }
        }
      }
    }
  }
});

// ---- gauge: the number of gauges --------------------------------------------------------------------------------------------------------

const countOf = (panel, doc = { sections: [] }) => gaugeCount(panel, doc);

test("gauge E4: case A, a field pattern that does not compile, gives no gauge", () => {
  assert.equal(countOf(gauge({ reduce: reduce({ fields: "/(/" }) })), 0);
  assert.equal(countOf(gauge({ reduce: reduce({ fields: "/[a-/" }) })), 0);
  assert.equal(countOf(gauge({ reduce: reduce({ fields: "^a$" }) })), null, "a pattern that compiles never fixes the number");
  assert.equal(countOf(gauge({ reduce: reduce({ fields: "" }) })), 1, "an empty text is the default");
  const r = out(gauge({ reduce: reduce({ fields: "/(/" }) }), 300, 200);
  assert.deepEqual(parts(r), ["holder"]);
  assert.deepEqual(r.notes, ["no gauge"]);
});

test("gauge E4: case B, a last transform that replaces the fields with one new number field, gives one gauge", () => {
  assert.equal(countOf(gauge()), 1);
  assert.equal(countOf(gauge({}, {}, { transforms: [calc("index", { index: { as_percentile: false } })] })), 1);
  assert.equal(countOf(gauge({}, {}, { transforms: [{ kind: "limit", count: 3, end: "first" }, calc("reduce_row")] })), 1, "an earlier transform does not matter");
  assert.equal(countOf(gauge({}, {}, { transforms: [calc("reduce_row"), { kind: "limit", count: 3, end: "first" }] })), null, "the last transform decides");
  assert.equal(countOf(gauge({}, {}, { transforms: [calc("reduce_row", { replace_fields: false })] })), null, "the fields stay");
  assert.equal(countOf(gauge({}, {}, { transforms: [calc("binary", { alias: "x" })] })), null, "binary can return the fields as they are");
  assert.equal(countOf(gauge({}, {}, { transforms: [calc("unary")] })), null);
  assert.equal(countOf(gauge({}, {}, { transforms: [] })), null, "the default of a new panel has no proof");
  assert.equal(countOf(gauge({}, {}, { transforms: [{ kind: "partition_by_values", fields: ["a"], keep_fields: false }, ...ONE] })), null, "a partition gives one frame for each value");
  assert.equal(countOf(gauge({ reduce: reduce({ values: true }) })), null, "one gauge for each row");
  assert.equal(countOf(gauge({ reduce: reduce({ fields: "/^Sum$/" }) })), null, "a pattern of fields can select the time field");
  assert.equal(countOf(gauge({ reduce: reduce({ fields: "Sum" }) })), null, "a literal name does not fix the number: the field can be missing");
});

test("gauge E4: the queries of the owner of the data must be one visible query", () => {
  const two = gauge({}, {}, { data: { ...query(), extra_queries: [{ ref: "B", source: "s", format: "table", sql: ["SELECT 2"] }] } });
  assert.equal(countOf(two), null, "two visible queries give two frames");
  const hidden = gauge({}, {}, { data: { ...query(), extra_queries: [{ ref: "B", source: "s", format: "table", sql: ["SELECT 2"], hide: true }] } });
  assert.equal(countOf(hidden), 1, "a hidden query does not run");
  const none = gauge({}, {}, { data: query({ hide: true }) });
  assert.equal(countOf(none), null, "no visible query, no frame");
});

test("gauge E4: a reuse reads the queries of its source, and the transforms of the source with with_transforms", () => {
  const source = { id: 7, type: "table", data: query(), transforms: ONE };
  const doc = { sections: [{ panels: [source] }] };
  const reuse = (withTransforms, transforms = []) => gauge({}, {}, { data: { reuse: { panel: 7, with_transforms: withTransforms } }, transforms });
  assert.equal(countOf(reuse(true), doc), 1, "the transforms of the source come first");
  assert.equal(countOf(reuse(false), doc), null, "the transforms of the source do not apply");
  assert.equal(countOf(reuse(false, ONE), doc), 1, "the own transforms apply");
  assert.equal(countOf(reuse(true, [{ kind: "limit", count: 1, end: "first" }]), doc), null, "the last transform of the whole chain decides");
  assert.equal(countOf(reuse(true), { sections: [] }), null, "a source that is not in the document proves nothing");
  const hiddenSource = { ...source, data: query({ hide: true }) };
  assert.equal(countOf(reuse(true), { sections: [{ panels: [hiddenSource] }] }), null);
  assert.deepEqual(chainOf(reuse(true), doc).refs, ["A"]);
  assert.equal(chainOf(reuse(true), doc).owner, source);
});

test("gauge R3: a gauge whose number is data shows the box only, for every option", () => {
  for (const options of [{}, { orientation: "vertical" }, { value_size: 30, show_threshold_labels: true }, { reduce: reduce({ values: true }) }]) {
    const r = out(gauge(options, {}, { transforms: [] }), 682, 238);
    assert.deepEqual(parts(r), ["holder"], JSON.stringify(options));
    assert.deepEqual(r.notes, ["box"]);
  }
});

// ---- gauge: the cell, the title, the canvas -------------------------------------------------------------------------------------------

test("gauge E5: the cell of one gauge follows the orientation and the manual sizes", () => {
  const cellOf = (options) => rectOf(part(out(gauge(options), 400, 200), "tile"));
  assert.deepEqual(cellOf({}), [0, 0, 400, 200], "auto");
  assert.deepEqual(cellOf({ orientation: "horizontal" }), [0, 0, 400, 200], "horizontal, the box is higher than 75 px");
  assert.deepEqual(cellOf({ orientation: "horizontal", sizing: "manual", min_height: 300 }), [0, 0, 400, 300], "a manual minimum height");
  assert.deepEqual(cellOf({ orientation: "horizontal", sizing: "auto", min_height: 300 }), [0, 0, 400, 200], "sizing auto reads no minimum");
  assert.deepEqual(cellOf({ orientation: "vertical", sizing: "manual", min_width: 500 }), [0, 0, 500, 200], "a manual minimum width");
  assert.deepEqual(cellOf({ orientation: "vertical", sizing: "manual", min_height: 500 }), [0, 0, 400, 200], "vertical reads no minimum height");
  assert.deepEqual(rectOf(part(out(gauge({ orientation: "horizontal" }), 400, 60), "tile")), [0, 0, 400, 75], "the least height is 75 px");
  // The cell is the cell of the loaded code.
  assert.deepEqual(gridCells(1, 400, 200)[0], { x: 0, y: 0, w: 400, h: 200 });
  assert.deepEqual(stackCells(1, 400, 60, false)[0], { x: 0, y: 0, w: 400, h: 75 });
});

test("gauge E7, E8, R7: the title box exists iff display_name is set, and its numbers are those of cellParts", () => {
  const untitled = out(gauge(), 161, 238);
  assert.equal(part(untitled, "title"), undefined, "no display_name, no title for one gauge");
  const titled = out(gauge({}, { min: 0, max: 100, display_name: "CPU" }), 161, 238);
  const [x, y, w, h] = rectOf(part(titled, "title"));
  // The measure of Grafana (FE-84): a cell of 161 x 238 has the title size 16.1, the top 182.86 and the height 25.28.
  near(y, 182.85, 0.01);
  near(h, 25.3, 0.02);
  assert.equal(x, 0);
  assert.equal(w, 161);
  const cell = cellParts({ w: 161, h: 238, title: true, orientation: "auto", titleSize: null });
  near(y, cell.titleY);
  near(h, cell.titleH);
  assert.equal(part(titled, "title").a["data-skel-class"], EXACT);
  // An override that sets display_name does not turn the title on.
  const overridden = out({ ...gauge(), fields: { defaults: { min: 0, max: 100 }, overrides: [{ match: { by: "name", value: "Sum" }, set: { display_name: "x" } }] } }, 161, 238);
  assert.equal(part(overridden, "title"), undefined);
  // title_size sets the size of the text, and the gauge height still uses the automatic size.
  const sized = out(gauge({ title_size: 30 }, { min: 0, max: 100, display_name: "CPU" }), 161, 238);
  const c30 = cellParts({ w: 161, h: 238, title: true, orientation: "auto", titleSize: 30 });
  near(rectOf(part(sized, "title"))[3], c30.titleH);
  assert.equal(c30.titleSize, 30);
  // The canvas: the same height as the loaded canvas, at the left of the cell.
  const [cx, cy, cw, ch] = rectOf(part(titled, "plot"));
  near(cy, cell.top);
  near(ch, cell.canvasH);
  assert.equal(cx, 0);
  assert.equal(cw, 161);
});

test("gauge E8 and E10: a vertical gauge has a canvas as wide as it is high, at the left of the cell", () => {
  const p = gauge({ orientation: "vertical", sizing: "manual", min_width: 300 }, { min: 0, max: 100, display_name: "CPU" });
  const r = out(p, 400, 200);
  const cell = stackCells(1, 400, 200, true, 300)[0];
  const cp = cellParts({ w: cell.w, h: cell.h, title: true, orientation: "vertical", titleSize: null });
  const [x, , w, h] = rectOf(part(r, "plot"));
  assert.equal(x, 0, "the canvas is not centred");
  near(w, cp.canvasW);
  near(h, cp.canvasH);
  near(rectOf(part(r, "title"))[2], cp.titleW);
});

test("gauge E9: the title text is a bar of the measured width when the document fixes the text, else the box stays empty", () => {
  const bars = (defaults, over = {}) => out({ ...gauge({}, defaults), ...over }, 400, 300).elements.filter((e) => e.c === "skel__b");
  const base = { min: 0, max: 100 };
  const one = bars({ ...base, display_name: "Total weight" });
  assert.equal(one.length, 1);
  assert.equal(part(out(gauge({}, { ...base, display_name: "Total weight" }), 400, 300), "label").a["data-skel-class"], EXACT);
  const size = cellParts({ w: 400, h: 300, title: true, orientation: "auto", titleSize: null }).titleSize;
  near(num(one[0], "--w"), "Total weight".length * 6);
  assert.ok(size > 0);
  assert.equal(num(one[0], "--h"), 8);
  assert.equal(bars({ ...base, display_name: "${__field.name}" }).length, 0, "the field gives its value");
  assert.equal(bars({ ...base, display_name: "[[__field.labels.host]]" }).length, 0);
  assert.equal(bars({ ...base, display_name: "${__series.name}" }).length, 0, "empty after the series tokens: the name of the field is data");
  assert.equal(bars({ ...base, display_name: "$name" }).length, 1, "a bare name stays as written");
  assert.equal(titleText(configOf(gauge({}, { display_name: "a ${__series.name}b" }), ["A"]).config, { display_name: true }), "a b");
  const over = (match) => ({ fields: { defaults: { ...base, display_name: "Fixed" }, overrides: [{ match, set: { display_name: "Other" } }] } });
  assert.equal(bars(null, over({ by: "name", value: "Sum" })).length, 0, "an override that can match sets the name: unknown");
  // A pattern that does not compile also gives the warning line: its bars come first, and the bar of the title is the last one.
  const warned = bars(null, over({ by: "regex", value: "/(/" }));
  assert.equal(warned.length, 1 + warningLines(warningSentences({ fields: over({ by: "regex", value: "/(/" }).fields }), 400).length, "a pattern that does not compile matches nothing");
  assert.equal(num(warned[warned.length - 1], "--w"), "Fixed".length * 6);
  assert.equal(bars(null, over({ by: "query", value: "Z" })).length, 1, "a ref that no visible query has matches nothing");
  assert.equal(bars(null, over({ by: "query", value: "A" })).length, 0, "the ref of the query matches the field");
});

// ---- gauge: the arc ---------------------------------------------------------------------------------------------------------------------

const arcOf = (r) => part(r, "arc");

test("gauge E13, R9: the arc is the loaded geometry of a text of no width, and exact when the text cannot move it", () => {
  // FE-84 fixed: 499 x 238, no title, no band. The radius is 174.18 and cy is 179.18 with the arc width 40.
  const noBand = gauge({ show_threshold_markers: false });
  const lay = gaugeLayout(noBand, { w: 499, h: 238 }, ["A"], configOf(noBand, ["A"]).config, configOf(noBand, ["A"]).decided);
  near(lay.g.radius, 174.18, 0.01);
  near(lay.g.cy, 179.18, 0.01);
  assert.equal(lay.g.arcW, 40);
  assert.equal(lay.exactArc, true, "the text cannot move the arc here");
  const r = out(noBand, 499, 238);
  assert.equal(arcOf(r).a["data-skel-class"], EXACT);
  assert.equal(r.bounds.radius.kind, EXACT);
  // The geometry is the geometry of gauge.js for the same box.
  const parts2 = cellParts({ w: 499, h: 238, title: false, orientation: "auto", titleSize: null });
  const g = gaugeGeometry({ cw: parts2.canvasW, ch: parts2.canvasH, sizeW: parts2.sizeW, sizeH: parts2.sizeH, w14: 0, labels: false, markers: false, bands: false });
  near(r.bounds.radius.value, g.radius, 1e-9);
  near(r.bounds.cy.value, g.cy, 1e-9);
});

test("gauge E12 and E14: a band that the scale cannot prove, or a text that can move the arc, give a lower bound", () => {
  // FE-84 big: the data give the scale. The room of the band is reserved, so the radius is 166.18 (8 px less than 174.18).
  const big = gauge({}, {});
  const r = out(big, 499, 238);
  assert.equal(r.bounds.radius.kind, LEAST);
  near(r.bounds.radius.value, 166.18, 0.01);
  assert.equal(arcOf(r).a["data-skel-class"], LEAST);
  assert.equal(part(r, "scale"), undefined, "no band ring for a scale from the data");
  // The loaded radius is never smaller: a fixed value text of any width gives a radius between the two bounds.
  const cfg = configOf(big, ["A"]);
  const lay = gaugeLayout(big, { w: 499, h: 238 }, ["A"], cfg.config, cfg.decided);
  const cp = cellParts({ w: 499, h: 238, title: false, orientation: "auto", titleSize: null });
  for (const w14 of [0, 10, 14, 40, 100, 1e9]) {
    const loaded = gaugeGeometry({ cw: cp.canvasW, ch: cp.canvasH, sizeW: cp.sizeW, sizeH: cp.sizeH, w14, labels: false, markers: true, bands: true });
    assert.ok(boundHolds(r.bounds.radius, loaded.radius), `w14 ${w14}`);
  }
  assert.ok(lay.g.radius > 0);
  // value_size fixes the text size: the arc is exact (a band that is decided too).
  const sized = out(gauge({ value_size: 30, show_threshold_markers: false }), 499, 238);
  assert.equal(sized.bounds.radius.kind, EXACT);
  // Labels with a text that can be large: the label size moves the arc, so the arc is a bound.
  const labels = out(gauge({ show_threshold_labels: true, show_threshold_markers: false }), 499, 238);
  assert.equal(labels.bounds.radius.kind, LEAST);
});

test("gauge E12: the room of the band is reserved only where the document leaves it open", () => {
  const room = (panel, w = 499, h = 238) => {
    const cfg = configOf(panel, ["A"]);
    return gaugeLayout(panel, { w, h }, ["A"], cfg.config, cfg.decided).room;
  };
  const thresholds = { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 80, color: "red" }] };
  assert.deepEqual(room(gauge({ show_threshold_markers: false })), { bands: false, exact: true }, "markers off");
  assert.deepEqual(room(gauge({}, { min: 0, max: 100, color: { mode: "fixed", fixed: "blue" } })), { bands: false, exact: true }, "another colour mode");
  const fixed = room(gauge({}, { min: 0, max: 100, thresholds }));
  assert.equal(fixed.bands, true);
  assert.equal(fixed.exact, true);
  assert.deepEqual(fixed.entries.map((e) => e.value), [0, 80, 100]);
  const dataScale = room(gauge({}, { thresholds }));
  assert.deepEqual([dataScale.bands, dataScale.exact], [true, false], "the scale comes from the data: one entry is possible");
  const undecided = (set) => ({ ...gauge({}, { min: 0, max: 100, thresholds }), fields: { defaults: { min: 0, max: 100, thresholds }, overrides: [{ match: { by: "name", value: "Sum" }, set }] } });
  assert.deepEqual([room(undecided({ color: { mode: "fixed", fixed: "red" } })).bands, room(undecided({ color: { mode: "fixed", fixed: "red" } })).exact], [true, false], "an override of the colour: reserve the room of the defaults");
  const noBandDefaults = { ...gauge({}, { min: 0, max: 100, color: { mode: "fixed", fixed: "blue" } }), fields: { defaults: { min: 0, max: 100, color: { mode: "fixed", fixed: "blue" } }, overrides: [{ match: { by: "name", value: "Sum" }, set: { color: { mode: "thresholds", fixed: null } } }] } };
  assert.deepEqual([room(noBandDefaults).bands, room(noBandDefaults).exact], [true, false], "an override can add a band, so the room is reserved");
  const excluded = { ...gauge({}, { min: 0, max: 100, color: { mode: "fixed", fixed: "blue" } }), fields: { defaults: { min: 0, max: 100, color: { mode: "fixed", fixed: "blue" } }, overrides: [{ match: { by: "regex", value: "/(/" }, set: { color: { mode: "thresholds", fixed: null } } }] } };
  assert.deepEqual(room(excluded), { bands: false, exact: true }, "a pattern that does not compile matches no field");
  // With a single threshold step and a scale of one point, the loaded band has one entry and the room is zero: the reserve is still a lower bound.
  const point = room(gauge({}, { min: 80, max: 80, thresholds }));
  assert.equal(point.exact, false, "min = max gives no scale: the document does not decide");
});

test("gauge E16: the scale is fixed by percentage mode, by both ends, or by a percent unit, and by nothing else", () => {
  const scaleOf2 = (defaults, options = {}, overrides = []) => {
    const p = { ...gauge(options, defaults), fields: { defaults, overrides } };
    const cfg = configOf(p, ["A"]);
    return gaugeLayout(p, { w: 400, h: 300 }, ["A"], cfg.config, cfg.decided).scale;
  };
  assert.deepEqual(scaleOf2({ min: 0, max: 400 }), { min: 0, max: 400 });
  assert.equal(scaleOf2({}), null, "no member: the range of the data");
  assert.equal(scaleOf2({ min: 0 }), null, "one end");
  assert.deepEqual(scaleOf2({ unit: "percent" }), { min: 0, max: 100 });
  assert.deepEqual(scaleOf2({ unit: "percentunit" }), { min: 0, max: 1 });
  assert.deepEqual(scaleOf2({ unit: "percent", min: 10 }), { min: 10, max: 100 });
  assert.deepEqual(scaleOf2({ thresholds: { mode: "percentage", steps: [{ value: null, color: "green" }] } }), { min: 0, max: 100 }, "percentage mode");
  assert.equal(scaleOf2({ min: 5, max: 5 }), null, "an empty scale draws no band");
  // With the markers the ends are rounded to the decimals (2 when absent).
  assert.deepEqual(scaleOf2({ min: 0.123456, max: 10 }), { min: 0.12, max: 10 });
  assert.deepEqual(scaleOf2({ min: 0.123456, max: 10, decimals: 0 }), { min: 0, max: 10 });
  assert.deepEqual(scaleOf2({ min: 0.123456, max: 10 }, { show_threshold_markers: false }), { min: 0.123456, max: 10 }, "no rounding without the markers");
  const over = (set) => scaleOf2({ min: 0, max: 100 }, {}, [{ match: { by: "name", value: "Sum" }, set }]);
  assert.equal(over({ max: 500 }), null, "an override of the maximum: unknown");
  assert.equal(over({ unit: "percent" }), null, "an override of the unit: unknown");
  assert.equal(over({ decimals: 0 }), null, "an override of the decimals changes the rounding of the ends");
  assert.deepEqual(over({ links: [] }), { min: 0, max: 100 }, "a member that the scale does not read does not matter");
});

test("gauge E17 and E18: the band ring and the neutral mark are drawn for a fixed scale only, with the angles of angleOf", () => {
  const fixed = gauge({ neutral: 200 }, { min: 0, max: 400 });
  const r = out(fixed, 400, 300);
  assert.ok(part(r, "scale"), "a fixed scale and a band");
  assert.equal(part(r, "scale").a["data-skel-class"], EXACT, "the text cannot move the arc here, so the band is exact");
  assert.equal(part(out(gauge({ show_threshold_labels: true }, { min: 0, max: 400 }), 400, 300), "scale").a["data-skel-class"], LEAST, "a label size that the text can move: a bound");
  const canvas = part(r, "plot");
  const paths = canvas.k[0].k;
  assert.equal(paths.length, 3, "the empty arc, the band and the neutral mark");
  const d = 400 / 600;
  const mark = paths[2];
  assert.ok(mark.a.d.startsWith("M"));
  assert.ok(angleOf(200 - d, 0, 400) < angleOf(200 + d, 0, 400));
  assert.equal(paths[2].a["data-skel-part"], undefined, "no hook on a shape that no test measures");
  // A scale from the data draws no band ring and no neutral mark.
  const data = out(gauge({ neutral: 200 }, {}), 400, 300);
  assert.equal(part(data, "scale"), undefined);
  assert.equal(part(data, "plot").k[0].k.length, 1);
  // A neutral value outside the scale draws no mark.
  assert.equal(part(out(gauge({ neutral: 900 }, { min: 0, max: 400 }), 400, 300), "plot").k[0].k.length, 2);
  // Markers off: no band.
  assert.equal(part(out(gauge({ show_threshold_markers: false }, { min: 0, max: 400 }), 400, 300), "scale"), undefined);
  // The ring path starts and ends at the angles of the loaded arc.
  const path = ringPath(100, 100, 50, 10, angleOf(-Infinity, 0, 1), angleOf(Infinity, 0, 1));
  assert.match(path, /^M[-0-9. ]+A50 50 0 1 1 [-0-9. ]+L[-0-9. ]+A40 40 0 1 0 [-0-9. ]+Z$/);
  // bandEntries is the function of the loaded gauge.
  assert.deepEqual(bandEntries({ mode: "absolute", steps: [{ value: null, color: "green" }, { value: 80, color: "red" }] }, 0, 100, 2).map((e) => e.value), [0, 80, 100]);
});

test("gauge E15: a canvas that is too small draws no ring", () => {
  const r = out(gauge({}, { min: 0, max: 400 }), 400, 8);
  assert.equal(part(r, "arc"), undefined);
  assert.ok(part(r, "plot"), "the canvas box stays");
});

test("gauge R14: no colour of the document, no text of a value and no threshold label is drawn", () => {
  const p = gauge({ show_threshold_labels: true, neutral: 100 }, { min: 0, max: 400, color: { mode: "fixed", fixed: "purple" }, unit: "percent", mappings: [{ kind: "value", value: "1", text: "ONE", color: "red" }], display_name: "Name" });
  const r = out(p, 500, 300);
  const text = JSON.stringify(r);
  assert.ok(!/purple|red|ONE|percent/.test(text));
  // The only bar is the one of the title. No node has a style, a fill or a stroke.
  assert.equal(r.elements.filter((e) => e.c === "skel__b").length, 1);
  for (const n of flat(r)) {
    assert.ok(!("style" in n.a) && !("fill" in n.a) && !("stroke" in n.a));
    for (const [name, value] of Object.entries(n.v)) {
      assert.match(name, /^--[a-z-]+$/);
      assert.match(String(value), /^-?[0-9.]+(px|%)?$/);
    }
  }
});

test("hooks: a family claims the parts of its file header, each with a tolerance class", () => {
  const allowed = new Set(["holder", "warning", "label", "tile", "title", "plot", "arc", "scale"]);
  const classes = new Set([EXACT, LEAST, DATA]);
  const panels = [stat(), barGauge(), gauge({ neutral: 100 }, { min: 0, max: 400, display_name: "x" }), withOverrides(stat(), ["/(/"])];
  for (const p of panels) {
    const r = out(p, 400, 300);
    for (const e of r.elements) {
      if (!e.a || !e.a["data-skel-part"]) continue;
      assert.ok(allowed.has(e.a["data-skel-part"]), e.a["data-skel-part"]);
      assert.ok(classes.has(e.a["data-skel-class"]), e.a["data-skel-class"]);
    }
  }
});

test("R16: the gauge preview uses one source for the numbers and copies no constant of the loaded gauge", () => {
  const js = read("web/js/preview/gauge.js").replace(/\/\/.*$/gm, "");
  assert.match(js, /from "\.\.\/panels\/gauge\.js"/);
  for (const name of ["gridCells", "stackCells", "cellParts", "gaugeGeometry", "angleOf", "bandEntries", "scaleOf"]) assert.ok(js.includes(name), name);
  assert.ok(!/\b(5\.5|1\.7|22 \/ 14|75)\b/.test(js), "a constant of gauge.js is copied");
  for (const name of ["stat", "gauge", "bargauge"]) {
    const family = read(`web/js/preview/${name}.js`).replace(/\/\/.*$/gm, "");
    assert.ok(!/rangeOf|\.queries\b|\.sql\b|\.expr\b/.test(family), `${name} reads a query or the range of the request`);
  }
});
