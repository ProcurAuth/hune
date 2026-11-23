// FE-24: the axis plan (9.19, F16), the style of a series (F17) and the axis hook of a scale.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { AXIS_PLACEMENTS, LINE_DASHES, axisPlan, axisSettings, seriesStyle, sharedRange } from "../../web/js/panels/series.js";
import { scaleAxis } from "../../web/js/panels/timeseries.js";
import { tickLabel } from "../../web/js/panels/series.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-24-axis-plan.json", import.meta.url)));

// A field with the resolved configuration that fields.js gives: `set` holds the properties that the document sets.
const fieldOf = ([unit, placement, set]) => ({ kind: "number", config: { unit, set: { ...set, ...(placement ? { axis_placement: placement } : {}) } } });

test("FE-24 the side, the scales and the grid of each case", () => {
  for (const c of v.cases) {
    const plan = axisPlan(c.series.map(fieldOf));
    assert.deepEqual(plan.sides, c.sides, c.name);
    assert.equal(plan.grid, c.grid, c.name);
    assert.equal(plan.scales.length, c.scales.length, c.name);
    c.scales.forEach((want, k) => {
      const got = plan.scales[k];
      for (const [key, value] of Object.entries(want)) assert.deepEqual(got[key], value, `${c.name}: ${key}`);
    });
    assert.equal(plan.left, plan.scales.find((s) => s.side === "left") || null);
    assert.equal(plan.right, plan.scales.find((s) => s.side === "right") || null);
  }
});

test("FE-24 a scale reads the unit and the first series", () => {
  const plan = axisPlan([fieldOf(["ms", null, {}]), fieldOf(["reqps", null, {}]), fieldOf(["bytes", null, {}])]);
  assert.equal(plan.left.unit, "ms");
  assert.equal(plan.left.first, 0);
  assert.equal(plan.right.unit, "reqps");
  assert.equal(plan.right.first, 1);
});

test("FE-24 the plan of no series is empty, and a list of settings replaces the fields", () => {
  const empty = axisPlan([]);
  assert.deepEqual(empty.scales, []);
  assert.equal(empty.grid, null);
  const plan = axisPlan([null, null], [{ unit: "a", placement: "auto" }, { unit: "b", placement: "left", label: "L" }]);
  assert.deepEqual(plan.sides, ["left", "left"]);
  assert.equal(plan.left.label, "L");
});

test("FE-24 the settings of a field", () => {
  assert.deepEqual(axisSettings({ config: { unit: "ms", set: {} } }), { unit: "ms", placement: "auto", label: null, softMin: null, softMax: null, min: null, max: null, negate: false });
  assert.equal(axisSettings({ config: { unit: "ms", set: { axis_placement: "bogus" } } }).placement, "auto");
  assert.equal(axisSettings({ config: {} }).unit, "none");
  assert.equal(axisSettings(null).placement, "auto");
  assert.deepEqual([...AXIS_PLACEMENTS], ["auto", "left", "right", "hidden"]);
});

test("FE-24 the style of a series: override, then the option of the panel, then the default", () => {
  const panel = { draw: "bars", line_width: 2, fill_opacity: 30, show_points: "never", point_size: 7 };
  const none = seriesStyle({ config: { set: {} } }, panel);
  assert.deepEqual(none, { draw: "bars", lineWidth: 2, fillOpacity: 30, dash: "solid", showPoints: "never", pointSize: 7, negate: false });
  const own = seriesStyle({ config: { set: { draw: "line", line_width: 0, fill_opacity: 0, show_points: "always", point_size: 1, line_dash: "dot", transform: "negative_y" } } }, panel);
  assert.deepEqual(own, { draw: "line", lineWidth: 0, fillOpacity: 0, dash: "dot", showPoints: "always", pointSize: 1, negate: true });
  // A value of 0 in an override is a value (it is not "not set").
  assert.equal(own.lineWidth, 0);
  assert.equal(seriesStyle({ config: { set: { transform: "none", line_dash: "bogus" } } }, panel).negate, false);
  assert.equal(seriesStyle({ config: { set: { line_dash: "bogus" } } }, panel).dash, "solid");
  // Without a panel option, the defaults of 4.28 apply.
  assert.deepEqual(seriesStyle({ config: { set: {} } }, null), { draw: "line", lineWidth: 1, fillOpacity: 0, dash: "solid", showPoints: "auto", pointSize: 5, negate: false });
});

test("FE-24 the dashes of Grafana: 10 and 10 with a butt cap, a round dot every 10 px (paper 10.8, V3)", () => {
  assert.deepEqual([...LINE_DASHES.dash.dash], [10, 10]);
  assert.equal(LINE_DASHES.dash.cap, "butt");
  assert.deepEqual([...LINE_DASHES.dot.dash], [0, 10]);
  assert.equal(LINE_DASHES.dot.cap, "round");
  assert.equal(LINE_DASHES.solid.dash, null);
});

test("FE-24 the range of all the number fields together", () => {
  const num = (values) => ({ kind: "number", values: Float64Array.from(values), nulls: new Uint8Array(values.length) });
  assert.deepEqual(sharedRange([num([0, 50]), num([10, 100]), { kind: "text", values: ["a"] }]), { min: 0, max: 100 });
  assert.deepEqual(sharedRange([num([NaN])]), { min: null, max: null });
  assert.deepEqual(sharedRange([{ kind: "number_text", num: Float64Array.of(3, NaN, 9) }]), { min: 3, max: 9 });
  assert.deepEqual(sharedRange([{ kind: "number", values: Float64Array.of(1, 99), nulls: Uint8Array.of(0, 1) }]), { min: 1, max: 1 });
});

test("FE-24 the axis hook of a scale: automatic, hard ends, soft ends, percent and log (ID-K316)", () => {
  const base = { binary: false, tickText: tickLabel, plainTick: tickLabel, percent: false, log: false };
  const ends = (set) => ({ min: null, max: null, softMin: null, softMax: null, ...set });
  const auto = scaleAxis({ ...base, ends: ends({}) });
  const a = auto.hook(0, 100, 300);
  assert.ok(a.max > 100 && a.min <= 0, "10 % of room above the data");
  assert.ok(a.ticks.length >= 3);
  // Hard ends on both sides are the ends of the axis.
  const hard = scaleAxis({ ...base, ends: ends({ min: 0, max: 200 }) }).hook(10, 20, 300, [10, 20]);
  assert.deepEqual([hard.min, hard.max], [0, 200]);
  // The soft ends apply in the hook, on the data ends of the engine (E30), not on the first two arguments.
  const soft = scaleAxis({ ...base, ends: ends({ softMax: 200 }) }).hook(5, 200, 300, [5, 95]);
  assert.deepEqual([soft.min, soft.max], [0, 200]);
  // Bars and a stack have the same automatic range: the bars do not start at 0 when the data stays far from 0.
  const far = auto.hook(0, 1010, 300, [1000, 1010]);
  assert.deepEqual([far.min, far.max], [999, 1011]);
  // A percent stack goes from 0 to 100 %. A log scale has no hook of its own.
  const pct = scaleAxis({ ...base, ends: ends({}), percent: true }).hook(0, 100, 300, [3, 100]);
  assert.deepEqual([pct.min, pct.max, pct.ticks.at(-1).label], [0, 100, "100%"]);
  assert.equal(scaleAxis({ ...base, ends: ends({}), log: true }).hook, undefined);
});
