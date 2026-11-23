// FE-23: the colour modes of wave 2 (9.16, F26): the series palette and the 10 continuous modes. The gradients are measured on Grafana 12.4.2.
// Since ID-K74 (owner decision of 2026-11-13) there is one series palette, the 18 colours of ClickHouse Dash, with a dark and a light variant.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  CONTINUOUS_MODES, QCHART, continuousFraction, fieldColor, fieldColorValue, fillOf, gradientColor, isContinuousMode, isThemedMode, markColor, qchartColor,
} from "../../web/js/colors.js";
import { makeDisplay } from "../../web/js/display.js";
import { COLOR_MODES, COLOR_MODE_LABELS } from "../../web/js/editor/props.js";
import { chartColor } from "../../web/js/panels/common.js";
import { sharedRange } from "../../web/js/panels/series.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-23-color-modes.json", import.meta.url)));

test("FE-23 the series palette is the 18 colours of ClickHouse Dash in each theme, and the vector file agrees (ID-K74)", () => {
  for (const theme of ["dark", "light"]) {
    assert.equal(QCHART[theme].length, 18, theme);
    assert.deepEqual([...QCHART[theme]], v[`palette_${theme}`], theme);
  }
  assert.deepEqual(QCHART.other, v.palette_other);
});

test("FE-23 the mode palette gives slot i mod 18 in the theme of the page, and there is no other palette mode", () => {
  const config = { color: { mode: "palette", fixed: null } };
  for (const dark of [true, false]) {
    for (const i of [0, 1, 17, 18, 19, 40]) {
      assert.equal(fieldColorValue(config, 1, { seriesIndex: i, dark }), QCHART[dark ? "dark" : "light"][i % 18], `${i} ${dark}`);
      assert.equal(fieldColorValue(config, 1, { seriesIndex: i, dark }), qchartColor(i, dark));
    }
  }
  // The name of the series has no effect: the removed mode palette_by_name is not a colour mode any more (it falls back to thresholds).
  assert.equal(fieldColorValue(config, 1, { seriesIndex: 2, name: "web-1" }), fieldColorValue(config, 1, { seriesIndex: 2, name: "db" }));
  assert.ok(!COLOR_MODES.includes("palette_by_name"));
  assert.equal(COLOR_MODES.filter((m) => m.startsWith("palette")).length, 1);
  assert.equal(COLOR_MODE_LABELS.palette, "Series palette");
  assert.equal(Object.keys(COLOR_MODE_LABELS).length, COLOR_MODES.length);
});

test("FE-23 each continuous mode follows the measure of Grafana to 1 level, in both themes", () => {
  assert.equal(Object.keys(CONTINUOUS_MODES).length, 10);
  for (const g of v.gradients) {
    assert.ok(isContinuousMode(g.mode) && isThemedMode(g.mode));
    for (const [pct, hex] of Object.entries(g.measured)) {
      const got = gradientColor(g.mode, Number(pct) / 100, g.theme === "dark");
      const err = [1, 3, 5].map((i) => Math.abs(parseInt(got.slice(i, i + 2), 16) - parseInt(hex.slice(i, i + 2), 16)));
      assert.ok(Math.max(...err) <= 1, `${g.mode} ${g.theme} ${pct}: ${got} against ${hex}`);
    }
  }
  // The vectors of the specification.
  assert.deepEqual([0, 0.25, 0.5, 0.75, 1].map((f) => gradientColor("continuous_gr_yl_rd", f, true)), ["#73BF69", "#B4CB4C", "#E2C03D", "#F39045", "#F2495C"]);
  assert.deepEqual([0, 0.25, 0.5, 0.75, 1].map((f) => gradientColor("continuous_blues", f, false)), ["#FFFFFF", "#C4D3EB", "#89A8D8", "#4D7CC4", "#1250B0"]);
});

test("FE-23 the spline is not the linear mix, and two stops are linear", () => {
  // With 3 stops the middle colour is the spline value, not the middle stop (green 73BF69, yellow FADE2A, red F2495C).
  assert.notEqual(gradientColor("continuous_gr_yl_rd", 0.5, true), "#FADE2A");
  // With 2 stops the result is linear to 1 level.
  const mid = gradientColor("continuous_yl_rd", 0.5, true);
  assert.deepEqual([1, 3, 5].map((i) => parseInt(mid.slice(i, i + 2), 16)), [Math.round((0xFF + 0xC4) / 2), Math.round((0xF8 + 0x16) / 2), Math.round((0x99 + 0x2A) / 2)]);
  // The four modes of one hue start at the panel background.
  assert.equal(gradientColor("continuous_reds", 0, true), "#181B1F");
  assert.equal(gradientColor("continuous_purples", 0, false), "#FFFFFF");
});

test("FE-23 the fraction: limits, null, NaN and the infinities", () => {
  assert.equal(continuousFraction(50, 0, 100), 0.5);
  assert.equal(continuousFraction(-10, 0, 100), 0);
  assert.equal(continuousFraction(110, 0, 100), 1);
  assert.equal(continuousFraction(null, 0, 100), 0);
  assert.equal(continuousFraction(NaN, 0, 100), 0);
  assert.equal(continuousFraction(-Infinity, 0, 100), 0);
  assert.equal(continuousFraction(Infinity, 0, 100), 1);
  assert.equal(continuousFraction(5, 5, 5), 0, "max equals min");
  assert.equal(continuousFraction(5, undefined, 5), 0);
  // The colour of a value that is null, NaN or -Infinity is the first stop, and +Infinity is the last stop.
  const config = { color: { mode: "continuous_gr_yl_rd", fixed: null } };
  for (const bad of [null, NaN, -Infinity]) assert.equal(fieldColorValue(config, bad, { min: 0, max: 10 }), "#73BF69");
  assert.equal(fieldColorValue(config, Infinity, { min: 0, max: 10 }), "#F2495C");
  assert.equal(fieldColorValue(config, 0, { min: 0, max: 10, dark: false }), "#56A64B");
});

test("FE-23 a continuous mode gives a hex, markColor and fieldColor resolve it", () => {
  const config = { color: { mode: "continuous_bl_pu", fixed: null } };
  assert.equal(fieldColor(config, 0, { min: 0, max: 1 }), "#5794F2");
  assert.equal(markColor(config, 1, { min: 0, max: 1 }), "#B877D9");
  assert.equal(markColor(config, 1, { min: 0, max: 1, dark: false }), "#A352CC");
});

test("FE-23 the display of a cell has its colour in both themes, and a null value has the first stop", () => {
  const field = { name: "x", kind: "number", values: Float64Array.of(0, 5, 10), nulls: Uint8Array.of(0, 0, 0), config: { unit: "none", decimals: null, min: 0, max: 10, mappings: [], no_value: null, color: { mode: "continuous_gr_yl_rd", fixed: null }, thresholds: { mode: "absolute", steps: [] }, set: {} } };
  const display = makeDisplay(field);
  const first = display(0);
  assert.equal(first.color, "#73BF69");
  assert.equal(first.light, "#56A64B");
  assert.equal(fillOf(first, true), "#73BF69");
  assert.equal(fillOf(first, false), "#56A64B");
  assert.equal(display(2).color, "#F2495C");
  // A null value has the colour of the first stop (9.16).
  const empty = display.valueDisplay(null);
  assert.equal(empty.color, "#73BF69");
  assert.equal(empty.light, "#56A64B");
  // The mode `thresholds` has no `light` member, and a null value has no colour.
  const plain = makeDisplay({ ...field, config: { ...field.config, color: { mode: "thresholds", fixed: null }, thresholds: { mode: "absolute", steps: [{ value: null, color: "green" }] } } });
  assert.equal(plain(0).light, null);
  assert.equal(plain.valueDisplay(null).color, null);
  // A mapping colour wins over the colour mode.
  const mapped = makeDisplay({ ...field, config: { ...field.config, mappings: [{ kind: "value", value: "5", text: "five", color: "red" }] } })(1);
  assert.equal(mapped.color, "#F2495C");
  assert.equal(mapped.light, null);
});

test("FE-23 the display range: an end that the field sets wins over the range of all fields", () => {
  const mk = (set) => ({ name: "x", kind: "number", values: Float64Array.of(5), nulls: Uint8Array.of(0), config: { unit: "none", decimals: null, min: 5, max: 5, mappings: [], no_value: null, color: { mode: "continuous_gr_yl_rd", fixed: null }, thresholds: { mode: "absolute", steps: [] }, set } });
  const shared = makeDisplay(mk({}), { range: { min: 0, max: 10 } })(0);
  assert.equal(shared.color, gradientColor("continuous_gr_yl_rd", 0.5, true));
  const own = makeDisplay(mk({ min: 5, max: 10 }), { range: { min: 0, max: 10 } })(0);
  assert.equal(own.color, "#73BF69");
});

test("FE-23 the colour of a chart series, with the theme of the page and the range of all the fields", () => {
  const f = (name, values, color, set = {}) => ({ name, kind: "number", values: Float64Array.from(values), nulls: new Uint8Array(values.length), config: { unit: "none", decimals: null, min: Math.min(...values), max: Math.max(...values), color, thresholds: { mode: "absolute", steps: [] }, set } });
  const fields = [f("a", [0, 50], { mode: "continuous_gr_yl_rd", fixed: null }), f("b", [0, 100], { mode: "palette", fixed: null })];
  const shared = sharedRange(fields);
  assert.deepEqual(shared, { min: 0, max: 100 });
  const c = chartColor(fields[0], 50, { index: 0, shared });
  assert.equal(c.color, gradientColor("continuous_gr_yl_rd", 0.5, true), "the range of the two fields, not the range of the field alone");
  assert.equal(c.colorFor(false), gradientColor("continuous_gr_yl_rd", 0.5, false));
  // The mode `palette` also follows the theme (the series palette of ID-K70 has a hex for each theme), so it has a colorFor.
  const pal = chartColor(fields[1], 100, { index: 1, shared });
  assert.equal(pal.color, QCHART.dark[1]);
  assert.equal(pal.colorFor(false), QCHART.light[1]);
  assert.equal(pal.colorFor(true), QCHART.dark[1]);
  // A mode with a fixed colour has no colorFor.
  assert.equal(chartColor({ ...fields[1], config: { ...fields[1].config, color: { mode: "fixed", fixed: "red" } } }, 100, { index: 1, shared }).colorFor, undefined);
});
