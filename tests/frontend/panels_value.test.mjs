// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the pure helpers of the stat, bar gauge and pie chart panels (9.19).
import test from "node:test";
import assert from "node:assert/strict";
import { cellInk, fitFontSize, inkOn, INK_DARK, INK_LIGHT, legibleOn, statFill, stepPercent, valueParts } from "../../web/js/panels/items.js";
import { NAMED, NAMED_LIGHT, darkenSpin, relativeLuminance } from "../../web/js/colors.js";
import { barGradient, thresholdStops } from "../../web/js/panels/bargauge.js";
import { boxInSector, drawOrder, inSector, legendOrder, percentText, placeLabel } from "../../web/js/panels/piechart.js";
import { gaugeFill, jsonCellText } from "../../web/js/panels/table.js";
import { horizontalLayout } from "../../web/js/panels/bargauge.js";
import { tileGrid, tileLayout } from "../../web/js/panels/stat.js";

const steps = (...s) => ({ mode: "absolute", steps: s.map(([value, color]) => ({ value, color })) });
const cfg = (thresholds, mode = "thresholds") => ({ color: { mode, fixed: null }, thresholds });

test("valueParts: the unit is split from the number, a mapping or a plain number is not", () => {
  const field = { kind: "number", config: { unit: "percent", decimals: 1 } };
  assert.deepEqual(valueParts(field, { raw: 45.84, text: "45.8%", mapped: false, empty: false }), { prefix: "", text: "45.8", suffix: "%" });
  assert.equal(valueParts(field, { raw: 45.84, text: "ok", mapped: true, empty: false }), null);
  assert.equal(valueParts({ kind: "number", config: { unit: "none", decimals: 0 } }, { raw: 3, text: "3", mapped: false, empty: false }), null);
  assert.equal(valueParts(field, { raw: null, text: "", mapped: false, empty: true }), null);
});

test("fitFontSize: the text never goes beyond the box", () => {
  const w100 = 400; // the text is 400 px wide at 100 px
  const s = fitFontSize(w100, 200, 300);
  assert.ok((s / 100) * w100 <= 200, "fits in width");
  const wide = fitFontSize(100, 2000, 60);
  assert.ok(wide * 1.1 <= 60, "fits in height");
  assert.equal(fitFontSize(0, 100, 100), 11);
  // Wave G: no upper limit (Grafana reaches 732 px in a view panel); the line height is a parameter.
  assert.equal(fitFontSize(50, 5000, 100, { fillW: 1, fillH: 1, lineHeight: 1.2 }), 100 / 1.2);
  assert.ok(fitFontSize(50, 5000, 5000) > 220);
  assert.equal(fitFontSize(100, 1000, 5000, { fillW: 0.96 }), 960);
});

test("statFill: the gradient and the ink of Grafana (dark and light theme)", () => {
  assert.equal(statFill("#73BF69", true).gradient, "linear-gradient(120deg, rgb(66, 154, 67), rgb(111, 183, 87))");
  assert.equal(statFill("#F2495C", true).gradient, "linear-gradient(120deg, rgb(223, 20, 16), rgb(240, 49, 96))");
  assert.equal(statFill("#ff0000", true).gradient, "linear-gradient(120deg, rgb(179, 24, 0), rgb(230, 0, 31))");
  assert.equal(statFill("#ff0000", false).gradient, "linear-gradient(120deg, rgb(255, 80, 54), rgb(255, 18, 49))");
  assert.equal(statFill("#808080", false).gradient, "linear-gradient(120deg, rgb(155, 155, 155), rgb(137, 137, 137))");
  assert.equal(statFill("#ffffff", true).gradient, "linear-gradient(120deg, rgb(217, 217, 217), rgb(242, 242, 242))");
  assert.equal(statFill("#000000", true).gradient, "linear-gradient(120deg, rgb(0, 0, 0), rgb(0, 0, 0))");
  assert.equal(statFill("var(--text)"), null);
  assert.equal(statFill("rgba(0, 0, 0, 0.5)"), null);
  assert.equal(statFill("#ff0000", true).ink, INK_LIGHT);
  assert.equal(statFill("#ff0000", false).ink, INK_LIGHT);
  assert.equal(statFill("#ffff00", true).ink, INK_DARK);
  assert.equal(statFill("#ffffff", false).ink, INK_DARK);
  assert.equal(statFill("#b4b4b4", true).ink, INK_DARK, "AA: light ink needs 3:1 on the lighter end of the fill");
  // The ink has 3:1 on the whole fill (large text, 9.26), or it is the better of the two inks (a mid blue passes with neither).
  for (const c of ["#73BF69", "#F2CC0C", "#5794F2", "#FF9830", "#F2495C", "#B877D9", "#ad46ff", "#808080", "#00ffff"]) {
    for (const dark of [true, false]) {
      const f = statFill(c, dark);
      const worst = (ink) => Math.min(...[f.from, f.to].map((end) => {
        const [l1, l2] = [relativeLuminance(ink), relativeLuminance(end)].sort((a, b) => b - a);
        return (l1 + 0.05) / (l2 + 0.05);
      }));
      const other = f.ink === INK_LIGHT ? INK_DARK : INK_LIGHT;
      assert.ok(worst(f.ink) >= 3 || worst(f.ink) >= worst(other), `${c} ${dark}`);
    }
  }
});

test("tileGrid: the orientation has the meaning of Grafana (horizontal stacks the tiles, vertical puts them side by side)", () => {
  assert.deepEqual(tileGrid(1, 300, 200, "horizontal"), { dir: "column", tileW: 300, tileH: 200 });
  assert.equal(tileGrid(3, 442, 140, "horizontal").dir, "column");
  // (140 - 2 * 3) / 3 = 44.667, rounded down to the layout unit of 1/64 px (9.19a).
  assert.equal(tileGrid(3, 442, 140, "horizontal").tileH, 44.65625);
  assert.equal(tileGrid(3, 442, 140, "vertical").dir, "row");
  assert.equal(tileGrid(3, 600, 300, "auto").dir, "row", "auto: wider than high");
  assert.equal(tileGrid(3, 300, 600, "auto").dir, "column", "auto: higher than wide");
  assert.equal(tileGrid(3, 300, 300, "auto").dir, "column", "auto: a square box is not wider than high");
  assert.ok(tileGrid(500, 400, 300, "horizontal").tileH >= 24, "the tiles do not shrink to nothing");
});

test("tileLayout: the sizes measured on Grafana 12.4.2", () => {
  const near = (a, b, eps = 0.02 * b) => assert.ok(Math.abs(a - b) <= eps, `${a} is not near ${b}`);
  // "48.5%" in a wide tile with a name (442 x 140): the value takes 30 % of the width, the name is 0.7 of the value.
  const wide = tileLayout({ w: 442, h: 140, w100: 331, t100: 400 });
  assert.equal(wide.wide, true);
  near(wide.valueSize, 37.9);
  near(wide.titleSize, 26.5);
  assert.equal(wide.center, false);
  // A value with no name fills the height (service overview: 450 x 102 gives 65 px).
  const plain = tileLayout({ w: 448, h: 102, w100: 200 });
  assert.equal(plain.valueSize, 65);
  assert.equal(plain.padding, 12);
  assert.equal(plain.center, true);
  // A tile of 100 px or less has a padding of 8 px (442 x 44.67: the value is 23.89 px, its unit 0.8 of it).
  const small = tileLayout({ w: 442, h: 44.6667, w100: 200, t100: 300 });
  assert.equal(small.padding, 8);
  near(small.valueSize, 23.89);
  near(small.unitSize, 19.11);
  near(tileLayout({ w: 442, h: 140, w100: 331 }).unitSize / tileLayout({ w: 442, h: 140, w100: 331 }).valueSize, 0.6, 0.001);
  // Stacked, with a name: the name is at most 30 px, the value takes the rest of the height (371 x 292: 193.33 px).
  const stacked = tileLayout({ w: 371, h: 292, w100: 100, t100: 560 });
  assert.equal(stacked.wide, false);
  assert.equal(stacked.titleSize, 30);
  near(stacked.valueSize, 193.33, 0.5);
  // A graph: half of the height in a wide tile, the rest under the value in a stacked tile.
  assert.equal(tileLayout({ w: 442, h: 140, w100: 450, chart: true }).chartHeight, 70);
  near(tileLayout({ w: 442, h: 140, w100: 450, chart: true }).valueSize, 37.3, 1);
  const tall = tileLayout({ w: 400, h: 689, w100: 442, chart: true });
  near(tall.valueSize, 83.9, 1.5);
  assert.equal(tall.chartHeight, Math.floor(689 - tall.valueSize * 1.2));
  // justify = center centres a tile that has a name.
  assert.equal(tileLayout({ w: 442, h: 140, w100: 331, t100: 400, justify: "center" }).center, true);
});

test("legibleOn and inkOn: contrast rules in both themes", () => {
  const lum = (c) => c;
  assert.notEqual(legibleOn("#FADE2A", "#ffffff", 3), "#FADE2A", "yellow is darkened on white");
  assert.equal(legibleOn("#FADE2A", "#12151a", 3), "#FADE2A", "yellow stays on dark");
  assert.equal(legibleOn("var(--text)", "#ffffff", 3), "var(--text)");
  assert.equal(inkOn("#73BF69"), "#000000");
  assert.equal(inkOn("#C4162A"), "#ffffff");
  assert.equal(inkOn("var(--text)"), "var(--bg)");
  assert.ok(lum);
});

test("stepPercent and thresholdStops: the colours are in the order of the steps, at their place on the scale", () => {
  const t = steps([null, "green"], [50, "yellow"], [70, "red"]);
  assert.equal(stepPercent(t, null, 0, 100), 0);
  assert.equal(stepPercent(t, 50, 0, 100), 50);
  assert.equal(stepPercent(t, 500, 0, 100), 100);
  assert.equal(stepPercent({ mode: "percentage", steps: [] }, 30, 0, 200), 30);
  const stops = thresholdStops(cfg(t), 0, 100);
  assert.deepEqual(stops.map((s) => s[1]), [0, 50, 70]);
  assert.deepEqual(stops.map((s) => s[0]), ["#73BF69", "#FADE2A", "#F2495C"]);
  assert.equal(thresholdStops(cfg(t, "fixed"), 0, 100), null);
});

test("thresholdStops and barGradient: the light theme draws the named colours with the light shades (X4)", () => {
  const t = steps([null, "green"], [50, "yellow"], [70, "red"]);
  assert.deepEqual(thresholdStops(cfg(t), 0, 100, true).map((s) => s[0]), ["#73BF69", "#FADE2A", "#F2495C"]);
  assert.deepEqual(thresholdStops(cfg(t), 0, 100, false).map((s) => s[0]), ["#56A64B", "#F2CC0C", "#E02F44"]);
  const g = barGradient(cfg(t), 0, 100, 80, "to right", false);
  assert.ok(g.includes("#56A64B") && g.includes("#F2CC0C") && !g.includes("#73BF69"), g);
  assert.ok(barGradient(cfg(t), 0, 100, 80, "to right").includes("#73BF69"), "the dark theme is the default");
  // A colour of the document that is not a token keeps its value in both themes.
  const lit = steps([null, "#73BF69"], [50, "#abcdef"]);
  assert.deepEqual(thresholdStops(cfg(lit), 0, 100, false).map((s) => s[0]), ["#73BF69", "#abcdef"]);
});

test("cellInk: Grafana's ink (brightness 180) with an AA step to 4.5:1 over the whole fill", () => {
  const ratio = (a, b) => {
    const [x, y] = [relativeLuminance(a), relativeLuminance(b)].sort((m, n) => n - m);
    return (x + 0.05) / (y + 0.05);
  };
  assert.equal(cellInk("#FFB357"), INK_DARK, "bright fill: dark ink as Grafana");
  assert.equal(cellInk("#F2CC0C"), INK_DARK);
  assert.equal(cellInk("#1F60C4"), INK_LIGHT, "dark fill: light ink as Grafana");
  assert.equal(cellInk("#19730E"), INK_LIGHT);
  assert.equal(cellInk("#56A64B"), INK_DARK, "Grafana's light ink is 2.84:1 here: AA moves to the dark ink");
  assert.equal(cellInk("var(--text)"), "var(--bg)");
  assert.equal(cellInk("rgba(0, 0, 0, 0.5)"), "var(--text)");
  // Every named colour of both themes: flat, 4.5:1 always. As a gradient, 4.5:1 or the best of the four inks (a mid blue passes with none).
  for (const table of [NAMED, NAMED_LIGHT]) {
    for (const c of Object.values(table)) {
      for (const ends of [[c], [c, darkenSpin(c, true)], [c, darkenSpin(c, false)]]) {
        const ink = cellInk(c, ends);
        const worst = (i) => Math.min(...ends.map((e) => ratio(i, e)));
        if (ends.length === 1) assert.ok(worst(ink) >= 4.5, `${c} ${ink} ${worst(ink)}`);
        else assert.ok(worst(ink) >= 4.5 || [INK_LIGHT, INK_DARK, "#ffffff", "#000000"].every((i) => worst(ink) >= worst(i)), `${c} ${ends.join(" ")} ${ink}`);
      }
    }
  }
});

test("barGradient: positions are in percent of the bar, never decrease, and the first colour starts at 0", () => {
  const t = steps([null, "green"], [50, "yellow"], [60, "orange"], [70, "red"]);
  const g = barGradient(cfg(t), 0, 100, 80, "to right");
  assert.match(g, /^linear-gradient\(to right, #73BF69 0%/);
  const pos = [...g.matchAll(/ (-?[0-9.]+)%/g)].map((m) => Number(m[1]));
  for (let i = 1; i < pos.length; i++) assert.ok(pos[i] >= pos[i - 1], `non decreasing at ${i}: ${pos}`);
  // the red step (70 of 100) is at 87.5 percent of a bar that is 80 percent full
  assert.ok(g.includes("#F2495C 87.50%"));
  assert.equal(barGradient(cfg(t), 0, 100, 0, "to right"), null);
  // a bar that stops before the first step has one colour: no gradient, so no sliver of the next colour
  assert.equal(barGradient(cfg(t), 0, 100, 45.7, "to right"), null);
  // a bar past the first step ends with the colour of its step
  const past = barGradient(cfg(t), 0, 100, 55, "to right");
  assert.ok(past.includes("#FADE2A") && !past.includes("#FF9830") && !past.includes("#F2495C"));
  // steps outside the scale stay in order
  const out = barGradient(cfg(steps([null, "green"], [-20, "yellow"], [500, "red"])), 0, 100, 100, "to right");
  const p2 = [...out.matchAll(/ (-?[0-9.]+)%/g)].map((m) => Number(m[1]));
  for (let i = 1; i < p2.length; i++) assert.ok(p2[i] >= p2[i - 1]);
});

test("pie sectors: a point is in or out", () => {
  assert.equal(inSector(10, 0, 0, Math.PI / 2, 5, 20), true);
  assert.equal(inSector(-10, 0, 0, Math.PI / 2, 5, 20), false);
  assert.equal(inSector(2, 0, 0, Math.PI / 2, 5, 20), false);
  assert.equal(inSector(0, 10, 0, Math.PI * 2, 5, 20), true);
  assert.equal(boxInSector(30, 0, 10, 10, -0.3, 0.6, 10, 60), true);
  assert.equal(boxInSector(30, 0, 10, 80, -0.3, 0.6, 10, 60), false);
});

test("placeLabel: a label shows only when its whole box is in the slice", () => {
  const measure = (t) => t.length * 7;
  const big = placeLabel({ parts: ["host-1", "33.3%"], a0: -Math.PI / 2, sweep: (Math.PI * 2) / 3, inner: 0, radius: 100, measure });
  assert.ok(big && big.lines.length >= 1);
  assert.ok(Math.hypot(big.x, big.y) < 100);
  const tiny = placeLabel({ parts: ["host-1", "0.4%"], a0: 0, sweep: 0.02, inner: 0, radius: 100, measure });
  assert.equal(tiny, null);
  // too small a pie: no label
  assert.equal(placeLabel({ parts: ["33%"], a0: 0, sweep: 2, inner: 20, radius: 30, measure }), null);
  // a slice that is too narrow for the whole text drops the name first
  const mid = placeLabel({ parts: ["a-long-host-name", "9.0%"], a0: 0, sweep: 0.5, inner: 0, radius: 100, measure });
  if (mid) assert.ok(mid.lines.join(" ").includes("9.0%"));
});

test("pie labels: a donut slice finds a place with a smaller font, and a label keeps its distance from the slice edge", () => {
  const measure = (t) => t.length * 7;
  // 33.4 percent slices of a donut of 100 px: the percent fits at one of the sizes
  const sweep = (Math.PI * 2) / 3;
  for (const a0 of [-Math.PI / 2, -Math.PI / 2 + sweep, -Math.PI / 2 + 2 * sweep]) {
    const p = placeLabel({ parts: ["33.4%"], a0, sweep, inner: 55, radius: 100, measure });
    assert.ok(p, "label found");
    assert.ok(p.size <= 12 && p.size >= 10);
    assert.equal(boxInSector(p.x, p.y, measure("33.4%") * (p.size / 12) + 6, p.lineH, a0, sweep, 58, 97, 4), true);
  }
  // margin: a point 1 px from the edge ray is out with a 4 px margin
  assert.equal(inSector(50, 1, 0, 1, 10, 100, 4), false);
  assert.equal(inSector(50, 1, 0, 1, 10, 100, 0), true);
});

test("drawOrder: the largest slice first, a tie keeps the data order (Grafana)", () => {
  const slices = [{ name: "web-1", amount: 172 }, { name: "web-2", amount: 178 }, { name: "web-3", amount: 176 }, { name: "web-4", amount: 176 }];
  assert.deepEqual(drawOrder(slices).map((s) => s.name), ["web-2", "web-3", "web-4", "web-1"]);
});

test("legendOrder: the sort of a legend table by name, value or percent (F20)", () => {
  const slices = [{ name: "b", amount: 2, value: 2 }, { name: "A", amount: 5, value: 5 }, { name: "c", amount: 0, value: null }, { name: "d", amount: 3, value: 3 }];
  assert.equal(legendOrder(slices, undefined, false), slices);
  assert.deepEqual(legendOrder(slices, "name", false).map((s) => s.name), ["A", "b", "c", "d"]);
  assert.deepEqual(legendOrder(slices, "name", true).map((s) => s.name), ["d", "c", "b", "A"]);
  assert.deepEqual(legendOrder(slices, "value", false).map((s) => s.name), ["b", "d", "A", "c"], "a null value goes last");
  assert.deepEqual(legendOrder(slices, "value", true).map((s) => s.name), ["A", "d", "b", "c"], "a null value goes last in both directions");
  assert.deepEqual(legendOrder(slices, "percent", true).map((s) => s.name), ["A", "d", "b", "c"]);
});

test("percentText: the decimals of the field, none when the field sets none", () => {
  assert.equal(percentText(1, 3, null), "33%");
  assert.equal(percentText(1, 3, 1), "33.3%");
  assert.equal(percentText(2, 3, 0), "67%");
  assert.equal(percentText(1, 0, 1), "");
});

test("gaugeFill and jsonCellText (F04)", () => {
  assert.equal(gaugeFill(50, 0, 100), 0.5);
  assert.equal(gaugeFill(150, 0, 100), 1);
  assert.equal(gaugeFill(-5, 0, 100), 0);
  assert.equal(gaugeFill(null, 0, 100), 0);
  assert.equal(gaugeFill(5, 10, 10), 0, "max not above min: an empty bar");
  assert.equal(gaugeFill(5, NaN, 10), 0);
  assert.equal(jsonCellText('{ "a": 1,  "b": [1, 2] }'), '{"a":1,"b":[1,2]}');
  assert.equal(jsonCellText("not json"), "not json");
  assert.equal(jsonCellText("12"), "12");
  assert.equal(jsonCellText(""), "");
});

test("horizontalLayout: the sizes measured on Grafana 12.4.2", () => {
  const near = (a, b, eps = 0.02 * b) => assert.ok(Math.abs(a - b) <= eps, `${a} is not near ${b}`);
  // An item of 291 px: the name above, 50 px high (font 33.33 px); the bar takes the rest.
  const tall = horizontalLayout({ w: 1804, h: 291, w100: 316, nameW100: 750 });
  assert.equal(tall.place, "top");
  near(tall.nameBoxH, 50, 0.01);
  near(tall.nameSize, 33.33, 0.01);
  near(tall.barH, 241, 0.01);
  near(tall.valueSize, 40.05);
  assert.equal(tall.valueMaxW, 150);
  near(tall.scaleLen, 1804 - 150, 0.01);
  // An item of 44.33 px: the name above, a title of 0.45 of the item.
  const mid = horizontalLayout({ w: 1804, h: 44.33, w100: 316, nameW100: 750 });
  assert.equal(mid.place, "top");
  near(mid.nameBoxH, 19.95, 0.01);
  near(mid.valueSize, 24.38, 0.02);
  // An item of 37.66 px: the name at the left, 0.4 of the item high, right of the name 10 px of padding.
  const left = horizontalLayout({ w: 884, h: 17.67, w100: 316, nameW100: 750 });
  assert.equal(left.place, "left");
  assert.equal(left.nameSize, 12);
  near(left.nameW, (750 * 12) / 100 + 10, 0.01);
  near(left.valueSize, 17.67, 0.01);
  near(left.scaleLen, 884 - left.nameW - 150, 0.01);
});
