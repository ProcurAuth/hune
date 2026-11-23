// The percent change and the text sizes of a stat (4.29, 9.19, 10.3h; wave H, item P11; vector set FE-82): the text, the arrow and the
// colour of each change and each mode in both themes, and the sizes of `tileLayout`, against the measure of Grafana 12.4.2. Also the rows of
// the option form (write rule, rules of display, the number fields).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PANEL_BACKGROUND, resolveFill } from "../../web/js/colors.js";
import { newOptions } from "../../web/js/editor/draft.js";
import stat from "../../web/js/editor/forms/options/stat.js";
import { parseText, shownIds, valueOf, writeOption } from "../../web/js/editor/forms/options/rows.js";
import { legibleOn } from "../../web/js/panels/items.js";
import { tileLayout } from "../../web/js/panels/stat.js";
import { percentDir, percentHue, percentOf, percentSizes, percentText } from "../../web/js/panels/stat_percent.js";

const vectors = JSON.parse(readFileSync(new URL("./vectors/FE-82-stat-percent.json", import.meta.url), "utf8"));
// A number field of frame.js: a NULL cell has `nulls[i] = 1`; "NaN" in a vector is a NaN cell.
const field = (values) => ({
  kind: "number",
  values: Float64Array.from(values, (v) => (v === null ? 0 : v === "NaN" ? NaN : v)),
  nulls: Uint8Array.from(values, (v) => (v === null ? 1 : 0)),
});
const near = (got, want, what) => assert.ok(Math.abs(got - want) <= Math.max(0.06, Math.abs(want) * 0.005), `${what}: ${got} is not near ${want}`);
const same = (a, b) => String(a).toUpperCase() === String(b).toUpperCase();

test("FE-82: the text and the arrow of each change are those of Grafana 12.4.2", () => {
  assert.ok(vectors.texts.length >= 20);
  for (const v of vectors.texts) {
    const p = percentOf(field(v.values));
    assert.equal(percentText(p), v.text, `${v.id}: the text`);
    if (v.text !== null) assert.equal(percentDir(p), v.arrow, `${v.id}: the arrow`);
  }
  // The edges that the measure shows: a first value of 0 (infinite or NaN), -0, no usable value, and the sign of a negative first value.
  assert.equal(percentText(percentOf(field([0, -5]))), "-∞%");
  assert.equal(percentDir(percentOf(field([0, -5]))), "down");
  assert.equal(percentText(-0), "-0%");
  assert.equal(percentDir(-0), null);
  assert.equal(percentText(NaN), null);
  assert.equal(percentOf(field([])), 0);
  assert.equal(percentOf({ kind: "text", values: ["a", "b"] }), 0, "a text field gives 0, as Grafana");
  assert.equal(percentText(percentOf(field([-200, -100]))), "-50%", "the change divides by the first value with its sign");
  // 3 significant digits with the group separator: the number of the text has no rounding of its own.
  assert.equal(percentText(12.345), "12.3%");
  assert.equal(percentText(1e-7), "0.0000001%");
  assert.equal(percentText(123456), "123,000%");
});

test("FE-82: the colour of the change in each mode and each theme", () => {
  for (const v of vectors.texts) {
    if (v.text === null) continue;
    const p = percentOf(field(v.values));
    for (const theme of ["dark", "light"]) assert.ok(same(resolveFill(percentHue(p, "standard"), theme === "dark"), v[theme]), `${v.id} ${theme}`);
  }
  const modes = Object.fromEntries(vectors.modes.map((m) => [m.id, m]));
  for (const [id, values] of [["P13", [100, 120]], ["P14", [120, 100]], ["P15", [100, 100]]]) {
    const p = percentOf(field(values));
    for (const theme of ["dark", "light"]) assert.ok(same(resolveFill(percentHue(p, "inverted"), theme === "dark"), modes[id][theme].change), `${id} ${theme}`);
  }
  // `same_as_value` and a filled tile take the colour of the value; `standard` keeps green with a red value.
  for (const id of ["P16", "P17", "P19", "P20", "P21"]) for (const theme of ["dark", "light"]) assert.equal(modes[id][theme].change, modes[id][theme].value, id);
  for (const theme of ["dark", "light"]) assert.notEqual(modes.P23[theme].change, modes.P23[theme].value);
  // The contrast rule of the value text (9.19): a large change (24 px and more) keeps the colours of Grafana in both themes. A small change
  // keeps them in the dark theme; in the light theme the green moves toward the text colour, up to 4.5:1.
  for (const dark of [true, false]) {
    const panel = dark ? PANEL_BACKGROUND.dark : PANEL_BACKGROUND.light;
    for (const hue of ["green", "red"]) {
      const c = resolveFill(hue, dark);
      assert.equal(legibleOn(c, panel, 3), c, `${hue} large`);
      if (dark || hue === "red") assert.equal(legibleOn(c, panel, 4.5), c, `${hue} small`);
    }
  }
  assert.notEqual(legibleOn(resolveFill("green", false), PANEL_BACKGROUND.light, 4.5), resolveFill("green", false));
});

test("FE-82: the sizes of the value, the name and the change are those of Grafana 12.4.2", () => {
  assert.ok(vectors.sizes.length >= 18);
  for (const v of vectors.sizes) {
    const lay = tileLayout({ w: v.w, h: v.h, w100: v.w100, t100: v.t100, chart: v.chart, pct: true, titleFix: v.titleFix, valueFix: v.valueFix, p100: v.p100 });
    if (v.valueSize !== null) near(lay.valueSize, v.valueSize, `${v.id}: the value`);
    if (v.t100 > 0) near(lay.titleSize, v.titleSize, `${v.id}: the name`);
    for (const k of ["size", "icon", "gap", "lift"]) near(lay.pct[k], v.pct[k], `${v.id}: the change ${k}`);
  }
  // With no change, the value takes the whole height, as before the item (Grafana: 128.33 px in a stacked tile of 384 x 178).
  near(tileLayout({ w: 384, h: 178, w100: 168.15 }).valueSize, 128.333, "no change");
  assert.equal(tileLayout({ w: 384, h: 178, w100: 168.15 }).pct, null);
  // A tile with a graph keeps the whole height for its value (Grafana scales only the tiles with no graph).
  assert.equal(tileLayout({ w: 778, h: 64, w100: 170, chart: true, pct: true }).valueSize, tileLayout({ w: 778, h: 64, w100: 170, chart: true }).valueSize);
  assert.deepEqual(percentSizes(30), { size: 12, icon: 10, gap: 4, lift: 0 });
});

test("FE-82: the option rows: write rule, rules of display and the number fields", () => {
  const row = (id) => stat.rows.find((r) => r.id === id);
  const panel = { id: 1, type: "stat", title: "t", options: newOptions("stat"), fields: { defaults: {}, overrides: [] }, links: [] };
  const shown = (options) => shownIds(stat, options);
  assert.ok(shown(panel.options).includes("percent-change") && !shown(panel.options).includes("percent-change-color"));
  assert.ok(shown(panel.options).includes("title-size") && shown(panel.options).includes("value-size"));
  const on = writeOption(panel, row("percent-change"), true);
  assert.equal(on.options.percent_change, true);
  assert.ok(shown(on.options).includes("percent-change-color"));
  assert.ok(!shown({ ...on.options, reduce: { ...on.options.reduce, values: true } }).includes("percent-change"));
  assert.ok(!shown({ ...on.options, reduce: { ...on.options.reduce, values: true } }).includes("percent-change-color"));
  for (const mode of ["value", "name", "none"]) assert.ok(!shown({ ...panel.options, text_mode: mode }).includes("title-size"), mode);
  for (const mode of ["auto", "value_and_name"]) assert.ok(shown({ ...panel.options, text_mode: mode }).includes("title-size"), mode);
  // The defaults leave the panel: off, standard and an empty size.
  assert.equal(writeOption(on, row("percent-change"), false).options.percent_change, undefined);
  const inverted = writeOption(on, row("percent-change-color"), "inverted");
  assert.equal(inverted.options.percent_change_color, "inverted");
  assert.equal(writeOption(inverted, row("percent-change-color"), "standard").options.percent_change_color, undefined);
  assert.equal(valueOf(panel, row("percent-change-color")), "standard");
  const sized = writeOption(writeOption(inverted, row("value-size"), 40), row("title-size"), 20);
  assert.deepEqual(Object.keys(sized.options).slice(-4), ["percent_change", "percent_change_color", "title_size", "value_size"], "the order of 4.29");
  assert.equal(writeOption(sized, row("title-size"), parseText(row("title-size"), "").value).options.title_size, undefined);
  for (const [text, error] of [["0", "from 1 to 200"], ["201", "from 1 to 200"], ["12.5", "whole number"], ["x", "Use a number"]]) {
    assert.ok((parseText(row("value-size"), text).error || "").includes(error), `${text}: ${JSON.stringify(parseText(row("value-size"), text))}`);
  }
  assert.deepEqual(parseText(row("value-size"), "200"), { value: 200 });
});
