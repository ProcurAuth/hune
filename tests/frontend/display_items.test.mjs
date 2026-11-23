// Tests of the display of a cell (9.14 to 9.17, 9.20) and of the items of stat, bar gauge and pie (9.19).
import test from "node:test";
import assert from "node:assert/strict";
import { QCHART, fillOf } from "../../web/js/colors.js";
import { makeDisplay } from "../../web/js/display.js";
import { computeFieldConfig } from "../../web/js/fields.js";
import { baseDisplayNames, displayNames } from "../../web/js/names.js";
import { reduceItems } from "../../web/js/panels/items.js";
import { toFields } from "./helpers.mjs";

function configured(cols, spec, type = "table") {
  return computeFieldConfig(toFields(cols), spec, type).fields;
}

test("display: mapping text wins, then no_value, then the formatted value", () => {
  const [f] = configured([{ name: "s", kind: "number", values: [1, 0, null, 12.5] }], {
    defaults: { no_value: "n/a", unit: "suffix: pts", mappings: [{ kind: "value", value: "1", text: "Active", color: "green" }, { kind: "value", value: "0", text: null, color: "red" }] },
    overrides: [],
  });
  const d = makeDisplay(f);
  assert.deepEqual([d(0).text, d(0).color], ["Active", "#73BF69"]);
  assert.deepEqual([d(1).text, d(1).color], ["0 pts", "#F2495C"]);
  assert.equal(d(2).text, "n/a");
  assert.equal(d(2).empty, true);
  assert.equal(d(3).text, "12.5 pts");
});

test("display: threshold colour of a number, none for null; exact decimals keep their text", () => {
  const [f] = configured([{ name: "v", kind: "number_text", values: ["12345678901234567.123456789", null] }], { defaults: { unit: "none", decimals: 2 }, overrides: [] });
  const d = makeDisplay(f);
  assert.equal(d(0).text, "12345678901234567.12");
  assert.equal(d(1).text, "");
  assert.equal(d(1).color, null);
  const [g] = configured([{ name: "v", kind: "number", values: [10, 90] }], { defaults: {}, overrides: [] });
  const dg = makeDisplay(g);
  assert.equal(dg(0).color, "#73BF69");
  assert.equal(dg(1).color, "#F2495C");
});

test("display: time field and text field", () => {
  const f = configured([{ name: "t", kind: "time", values: [1794211200] }, { name: "x", kind: "text", values: ["abc"] }], { defaults: {}, overrides: [] });
  assert.equal(makeDisplay(f[0], { zone: "utc" })(0).text, "2026-11-09 08:00:00", "one row has no step: no milliseconds");
  assert.equal(makeDisplay(f[1], { zone: "utc" })(0).text, "abc");
});

test("display: a time field shows the milliseconds when its mean step is below 20 s (measured on Grafana 12.4.2)", () => {
  const text = (values, zone = "utc") => {
    const [f] = configured([{ name: "t", kind: "time", values }], { defaults: {}, overrides: [] });
    return makeDisplay(f, { zone })(0).text;
  };
  const t0 = 1794211200;
  assert.equal(text([t0 + 0.613, t0 + 1.681]), "2026-11-09 08:00:00.613", "two close values");
  assert.equal(text([t0, t0, t0]), "2026-11-09 08:00:00.000", "equal values: step 0");
  assert.equal(text([t0, t0 + 19.999, t0 + 39.998, t0 + 59.997]), "2026-11-09 08:00:00.000", "step 19.999 s");
  assert.equal(text([t0, t0 + 20, t0 + 40, t0 + 60]), "2026-11-09 08:00:00", "step 20 s");
  assert.equal(text([t0 + 60, t0 + 40, t0 + 20, t0]), "2026-11-09 08:01:00", "the step is the size of (last - first) / (rows - 1)");
  assert.equal(text([t0, t0 + 60, t0 + 20, t0 + 40]), "2026-11-09 08:00:00.000", "the order of the rows counts, not the spread");
  assert.equal(text([t0, t0 + 1, t0 + 2, t0 + 3, t0 + 4, t0 + 104]), "2026-11-09 08:00:00", "the mean, not the median");
  assert.equal(text([t0 + 0.5, t0 + 3600.5]), "2026-11-09 08:00:00", "no millisecond part, whatever the value");
});


test("items: reduce with calc, the pattern on fields, and values = true", () => {
  const fields = configured([{ name: "a", kind: "number", values: [1, 2, 3] }, { name: "Total", kind: "number", values: [10, 20, 30] }, { name: "k", kind: "text", values: ["x", "y", "z"] }], { defaults: { unit: "percent" }, overrides: [] }, "stat");
  const base = { fields, names: displayNames(fields), baseNames: baseDisplayNames(fields), zone: "utc" };
  const two = reduceItems(base, { calc: "last_not_null", fields: null, values: false });
  assert.deepEqual(two.map((i) => [i.name, i.text]), [["a", "3%"], ["Total", "30%"]]);
  const one = reduceItems(base, { calc: "total", fields: "/^Total$/", values: false });
  assert.deepEqual(one.map((i) => [i.name, i.text]), [["Total", "60%"]]);
  const rows = reduceItems(base, { calc: "last", fields: "/^Total$/", values: true });
  assert.deepEqual(rows.map((i) => [i.name, i.text]), [["x Total", "10%"], ["y Total", "20%"], ["z Total", "30%"]], "two numeric fields: the name of the field follows the row name (Grafana)");
  const solo = configured([{ name: "Total", kind: "number", values: [10, 20] }, { name: "k", kind: "text", values: ["x", "y"] }], { defaults: {}, overrides: [] }, "stat");
  const names = reduceItems({ fields: solo, names: displayNames(solo), baseNames: baseDisplayNames(solo), zone: "utc" }, { calc: "last", fields: null, values: true });
  assert.deepEqual(names.map((i) => i.name), ["x", "y"], "one numeric field: the row name only");
});

test("items: a calculation can force the unit (9.19)", () => {
  const fields = configured([{ name: "a", kind: "number", values: [1, 2, 4] }], { defaults: { unit: "bytes" }, overrides: [] }, "stat");
  const base = { fields, names: displayNames(fields), baseNames: baseDisplayNames(fields), zone: "utc" };
  const text = (calc) => reduceItems(base, { calc, fields: null, values: false })[0].text;
  assert.equal(text("count"), "3");
  assert.equal(text("distinct_count"), "3");
  assert.equal(text("diff_percent"), "300%");
  assert.equal(text("range"), "3 B");
  assert.equal(text("median"), "2 B");
});

test("display and fillOf: the stored token stays, the light theme draws its own shade (X4)", () => {
  const [f] = configured([{ name: "s", kind: "number", values: [10, 60, 90] }], {
    defaults: {
      thresholds: { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 50, color: "semi-dark-yellow" }, { value: 80, color: "#abcdef" }] },
      mappings: [{ kind: "value", value: "90", text: "Hot", color: "dark-red" }],
    },
    overrides: [],
  }, "stat");
  const d = makeDisplay(f);
  assert.equal(d(0).color, "#73BF69", "the dark hex stays in the display result");
  assert.equal(d(0).mark, "var(--viz-green)");
  assert.deepEqual([d(0), d(1)].map((x) => fillOf(x, true)), ["#73BF69", "#F2CC0C"]);
  assert.deepEqual([d(0), d(1)].map((x) => fillOf(x, false)), ["#56A64B", "#E0B400"]);
  assert.equal(fillOf(d(2), false), "#AD0317", "a mapping colour is a token too");
  assert.equal(fillOf(d(2), true), "#C4162A");
  const [g] = configured([{ name: "s", kind: "number", values: [90] }], { defaults: { thresholds: { mode: "absolute", steps: [{ value: null, color: "#abcdef" }] } }, overrides: [] }, "stat");
  assert.equal(fillOf(makeDisplay(g)(0), false), "#abcdef", "a hex of the document is never changed");
  const items = reduceItems({ fields: [f], names: ["s"], baseNames: ["s"], zone: "utc" }, { values: true, fields: "", calc: "lastNotNull" });
  assert.deepEqual(items.map((i) => fillOf(i.display, false)), ["#56A64B", "#E0B400", "#AD0317"]);
});

test("ID-K70 the panels that draw series default to the palette of ClickHouse Dash, the panels of one value keep thresholds", () => {
  const cols = [{ name: "a", kind: "number", values: [1, 2] }];
  const modeOf = (type) => configured(cols, { defaults: {}, overrides: [] }, type)[0].config.color.mode;
  for (const type of ["timeseries", "barchart", "piechart", "xychart"]) assert.equal(modeOf(type), "palette", type);
  for (const type of ["stat", "bargauge", "gauge", "table"]) assert.equal(modeOf(type), "thresholds", type);
  // An explicit colour mode wins over the default of the type: the palette on a stat, a fixed colour on a time series.
  const [stat] = configured(cols, { defaults: { color: { mode: "palette", fixed: null } }, overrides: [] }, "stat");
  const shown = makeDisplay(stat, { seriesIndex: 3 })(0);
  assert.equal(shown.color, QCHART.dark[3]);
  assert.equal(fillOf(shown, true), QCHART.dark[3]);
  assert.equal(fillOf(shown, false), QCHART.light[3], "the light theme draws the light colour of the same slot");
  const [fixed] = configured(cols, { defaults: { color: { mode: "fixed", fixed: "purple" } }, overrides: [] }, "timeseries");
  assert.equal(makeDisplay(fixed, { seriesIndex: 3 })(0).color, "#B877D9");
  // A mapping colour and an override keep their own colour.
  const [mapped] = configured(cols, { defaults: { mappings: [{ kind: "value", value: "1", text: "One", color: "red" }] }, overrides: [] }, "timeseries");
  assert.equal(makeDisplay(mapped, { seriesIndex: 3 })(0).color, "#F2495C");
});
