// The automatic y axis of the time series panel (9.19): the Grafana range rule.
import test from "node:test";
import assert from "node:assert/strict";
import { autoYAxis, autoYRange, boundedYAxis, histYAxis, numberTicks, stepDecimals, tickLabel, yTickSpace } from "../../web/js/panels/series.js";
import { yTickLabeller } from "../../web/js/panels/common.js";

test("autoYAxis keeps room of 10 % and rounds the edges to a tenth of the decade of the range", () => {
  // The values that Grafana draws for these data (the stream dashboard, panels 1, 14).
  const a = autoYAxis(117, 259, 860);
  assert.deepEqual([a.min, a.max], [100, 280]);
  const b = autoYAxis(68, 100, 800);
  assert.deepEqual([b.min, b.max], [64, 104]);
  // Processed jobs: the data 238 to 478 gives 210 to 510. The edges are not on a tick (step 50).
  const c = autoYAxis(238, 478, 247);
  assert.deepEqual([c.min, c.max, c.step], [210, 510, 50]);
  assert.deepEqual(c.values, [250, 300, 350, 400, 450, 500]);
  // Internal errors: 1 to 4 gives 0.7 to 4.3.
  const d = autoYRange(1, 4);
  assert.deepEqual(d, [0.7, 4.3]);
});

test("autoYAxis stays at 0 when the data does not cross 0 and the room would", () => {
  const a = autoYAxis(8, 108, 830);
  assert.deepEqual([a.min, a.max, a.step], [0, 120, 5]);
  // Requests by region: 16 to 201 gives 0 to 220. Active users: 1 to 36 gives 0 to 40.
  assert.deepEqual(autoYRange(16, 201), [0, 220]);
  assert.deepEqual(autoYRange(1, 36), [0, 40]);
  // The data does not touch 0 and the room does not reach it: no zero.
  assert.ok(autoYRange(238, 478)[0] > 0);
  const b = autoYAxis(-30, -2, 400);
  assert.equal(b.max, 0);
  assert.equal(b.min, -33);
  const c = autoYAxis(0, 50, 400);
  assert.equal(c.min, 0);
  assert.deepEqual(autoYRange(0, 100), [0, 110]);
});

test("autoYRange gives a range for a constant series, as Grafana does", () => {
  // A constant 1 gives 0 to 2 (dependency codes); 0 gives 0 to 1.
  assert.deepEqual(autoYRange(1, 1), [0, 2]);
  assert.deepEqual(autoYRange(0, 0), [0, 1]);
  assert.deepEqual(autoYRange(5, 5), [0, 10]);
});

test("autoYAxis gives a range for a flat series and for negative data", () => {
  const a = autoYAxis(5, 5, 300);
  assert.ok(a.min < 5 && a.max > 5);
  const b = autoYAxis(-5, 20, 400);
  assert.ok(b.min <= -5 && b.max >= 20);
  assert.ok(b.values.includes(0));
});

test("autoYAxis keeps the ticks at least minGap pixels apart and refuses a range with no data", () => {
  const a = autoYAxis(0, 1000, 300, 30);
  const gap = (300 * a.step) / (a.max - a.min);
  assert.ok(gap >= 30 - 1e-9, String(gap));
  assert.equal(autoYAxis(NaN, 1, 300), null);
  assert.equal(autoYAxis(0, 1, 0), null);
  assert.equal(autoYAxis(Infinity, 1, 300), null);
});

test("autoYAxis with binary steps gives powers of two", () => {
  const a = autoYAxis(8.5 * 1048576, 12.8 * 1048576, 830, 30, true);
  assert.equal(Math.log2(a.step) % 1, 0);
  assert.ok(a.values.every((v) => v % a.step === 0 && v >= a.min && v <= a.max));
  assert.ok(a.min <= 8.5 * 1048576 && a.max >= 12.8 * 1048576);
});

test("stepDecimals counts the decimals of a step", () => {
  assert.equal(stepDecimals(10), 0);
  assert.equal(stepDecimals(2.5), 1);
  assert.equal(stepDecimals(0.05), 2);
  assert.equal(stepDecimals(0.0005), 4);
  assert.equal(stepDecimals(0), 0);
});

test("tickLabel writes the least digits of a tick, with no trailing zero", () => {
  // Grafana: 2, 1.5, 1, 0.5, 0 on a step of 0.5; 250 on a step of 50; 0.15 on a step of 0.05.
  assert.deepEqual([2, 1.5, 1, 0.5, 0].map((v) => tickLabel(v, 0.5)), ["2", "1.5", "1", "0.5", "0"]);
  assert.equal(tickLabel(250, 50), "250");
  assert.equal(tickLabel(0.15, 0.05), "0.15");
  assert.equal(tickLabel(0.30000000000000004, 0.1), "0.3");
  assert.equal(tickLabel(-0.5, 0.5), "-0.5");
  assert.equal(tickLabel(-0, 1), "0");
  assert.equal(tickLabel(NaN, 1), "");
});

test("barAxis draws the bars from 0 with room above the highest value", async () => {
  const { barAxis } = await import("../../web/js/panels/barchart_h.js");
  // The stream dashboard, panel 6: 19549 is the highest value. The axis ends 10 % above it (21503.9), where the room ends,
  // and the last tick is the last multiple of the step inside it (Grafana, wave G).
  const a = barAxis([19549, 4358, 8936], 1400);
  assert.equal(a.min, 0);
  assert.ok(Math.abs(a.max - 19549 * 1.1) < 1e-6);
  assert.equal(a.step, 1000);
  assert.equal(a.values[a.values.length - 1], 21000);
  assert.ok(a.values.every((v) => v <= a.max));
  const b = barAxis([-5, 10], 600);
  assert.ok(b.min <= -5 && b.max >= 10);
  const c = barAxis([0, 0], 600);
  assert.deepEqual([c.min, c.max], [0, 1]);
  const d = barAxis([NaN], 600);
  assert.deepEqual([d.min, d.max], [0, 1]);
});

test("yTickSpace is 30 px, or 15 px on a plot of 150 px or less (Grafana 12.4.2 calculateSpace, decision ID-K311)", () => {
  assert.deepEqual([19, 57, 150, 151, 171, 817, 1207].map(yTickSpace), [15, 15, 15, 30, 30, 30, 30]);
  // The steps that Grafana chose for the data 16.7 to 74.8 (range 10 to 80): 10 at a plot of 227 and 327 px, 5 at 467 and 607, 2.5 at 907.
  const steps = [134, 227, 327, 467, 607, 907].map((h) => autoYAxis(16.7, 74.8, h, yTickSpace(h)).step);
  assert.deepEqual(steps, [10, 10, 10, 5, 5, 2.5]);
});

test("the y steps of a time series are those that Grafana 12.4.2 drew on the panels of h 4 to 24 (measure of 2026-11-16, decision ID-K311)", () => {
  // The data of the panels: 61 values sin(n / 6) * a + b. The plot heights of both products for h 4, 5, 6, 7, 8, 10, 12, 16, 20 and 24.
  const data = (a, b) => Array.from({ length: 61 }, (_, n) => Math.sin(n / 6) * a + b);
  const plots = [57, 95, 133, 171, 209, 285, 361, 513, 665, 817];
  const cases = [
    [data(0.45, 0.5), [0.5, 0.2, 0.2, 0.2, 0.2, 0.2, 0.1, 0.1, 0.05, 0.05]],
    [data(1200, 1250), [1000, 500, 500, 500, 500, 500, 250, 200, 200, 100]],
    [data(65, 15), [50, 50, 20, 50, 25, 20, 20, 10, 10, 10]],
    [data(5, 1005), [5, 2, 2, 2.5, 2, 2, 1, 1, 1, 0.5]],
  ];
  for (const [values, want] of cases) {
    const lo = Math.min(...values);
    const hi = Math.max(...values);
    assert.deepEqual(plots.map((h) => autoYAxis(lo, hi, h, yTickSpace(h)).step), want, `data ${lo} to ${hi}`);
  }
  // The field sets min 0 and max 100: 0, 50, 100 at 57 px, the step 20 up to 285 px, 10 at 361 and 513 px, 5 at 665 and 817 px.
  const hard = data(45, 50);
  assert.deepEqual(plots.map((h) => boundedYAxis(0, 100, Math.min(...hard), Math.max(...hard), h).step), [50, 20, 20, 20, 20, 20, 10, 10, 5, 5]);
  // The ticks of the data -50 to 80 on a plot of 133 px: -60 to 100 in steps of 20 (the old rule gave steps of 25).
  const neg = data(65, 15);
  assert.deepEqual(autoYAxis(Math.min(...neg), Math.max(...neg), 133, yTickSpace(133)).values, [-60, -40, -20, 0, 20, 40, 60, 80, 100]);
});

test("a field with decimals writes each tick with them, as Grafana does (70.0%)", () => {
  const percent = yTickLabeller({ config: { unit: "percent", decimals: 1 } });
  assert.deepEqual([10, 70, 80].map((v) => percent(v, 10)), ["10.0%", "70.0%", "80.0%"]);
  const plain = yTickLabeller({ config: { unit: "none", decimals: null } });
  assert.equal(plain(2, 0.5), "2");
  assert.equal(plain(1.5, 0.5), "1.5");
  const short = yTickLabeller({ config: { unit: "short", decimals: null } });
  assert.equal(short(200000, 200000), "200 K");
  assert.equal(short(1000000, 200000), "1 Mil");
  const fixed = yTickLabeller({ config: { unit: "none", decimals: 2 } });
  assert.equal(fixed(5, 5), "5.00");
});

test("numberTicks keeps the width of a label and 11.5 px between two ticks", () => {
  const measure = (t) => t.length * 7;
  const label = (v) => `${v.toFixed(1)}%`;
  const ticks = numberTicks(9, 81, 530, label, measure);
  assert.deepEqual(ticks.map((t) => t.v), [10, 20, 30, 40, 50, 60, 70, 80]);
  assert.equal(ticks[0].label, "10.0%");
  const narrow = numberTicks(9, 81, 200, label, measure);
  assert.ok(narrow.length < ticks.length);
  assert.deepEqual(numberTicks(1, 1, 200, label, measure), []);
});

test("boundedYAxis keeps the edges that the field sets, and the ticks keep the rule of the height (Grafana: 0 to 100 at 87 px gives 20, 40, 60, 80)", () => {
  const a = boundedYAxis(0, 100, 16, 75, 87);
  assert.deepEqual([a.min, a.max, a.step], [0, 100, 20]);
  assert.deepEqual(a.values, [0, 20, 40, 60, 80, 100]);
  // An edge that is not set is the edge of the automatic range.
  const b = boundedYAxis(0, null, 16, 75, 150);
  assert.equal(b.min, 0);
  assert.ok(b.max > 75);
  assert.equal(boundedYAxis(5, 5, 0, 1, 100), null);
  assert.equal(boundedYAxis(0, 10, 0, 1, 0), null);
});

test("histYAxis: the y axis of a histogram is the one of Grafana 12.4.2 (9.19f, section 3), measured at 16 plot heights", () => {
  // The fixture histogram_h, panel 1: 5,000 values in 17 buckets of 20 with the counts 1 to 1960. autoYRange(1, 1960) ends at 2200.
  // The steps of Grafana for each plot height (px): 15 px between ticks on a plot of 150 px or less, else 30 px.
  const steps = [[19, 2000], [26, 2000], [57, 1000], [63, 1000], [95, 500], [133, 250], [139, 250], [171, 500], [177, 500], [253, 500], [285, 250], [329, 250], [361, 200], [481, 200]];
  for (const [h, step] of steps) {
    const a = histYAxis(1, 1960, h);
    assert.deepEqual([a.min, a.max, a.step], [0, 2200, step], `plot of ${h} px`);
  }
  assert.deepEqual(histYAxis(1, 1960, 26).ticks.map((t) => t.label), ["0", "2000"]);
  assert.deepEqual(histYAxis(1, 1960, 481).ticks.at(-1), { v: 2200, label: "2200" });
  // The fixture histogram_f, panel 1: the counts 54 to 92 end at 96. A plot of 26 px shows only 0 (a step of 100 is above the end).
  assert.deepEqual(histYAxis(54, 92, 26).ticks.map((t) => t.v), [0]);
  assert.deepEqual([histYAxis(54, 92, 63).step, histYAxis(54, 92, 177).step, histYAxis(54, 92, 481).step], [25, 20, 10]);
  assert.equal(histYAxis(54, 92, 177).max, 96);
  // One bucket: the lowest and the highest count are equal, so the axis ends at 2 times the count, as in Grafana.
  assert.equal(histYAxis(3000, 3000, 177).max, 6000);
  assert.equal(histYAxis(1, 1, 177).max, 2);
  // No count above 0: no axis (the engine keeps its own).
  assert.equal(histYAxis(Infinity, 0, 177), null);
  assert.equal(histYAxis(1, 5, 0), null);
});
