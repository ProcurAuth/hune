// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B216 (4.18, 4.30, 9.19): the fill, the line, the grid lines and the orientation `auto` of a bar chart, against
// the probe of Grafana 12.4.2 of 2026-11-22 (the canvas calls of 16 bar charts, the data `a` = 10 and `b` = 5, the colour #FF0000):
//   fillOpacity 61: the fill rgba(255, 0, 0, 0.61) and a line #ff0000 of 1 px; fillOpacity 0 and lineWidth 4: no fill, a line of 4 px;
//   fillOpacity 100 and lineWidth 0: the fill #ff0000 and no line; the box of a bar loses half the line at each side;
//   axisGridShow false: no grid line, the labels stay; orientation auto: horizontal bars in a plot narrower than it is high.
import { doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const UID = "b216-bar-p03";
const SQL = "SELECT * FROM (SELECT 'a' AS k, 10 AS v UNION ALL SELECT 'b' AS k, 5 AS v) ORDER BY k";
const RED = { defaults: { color: { mode: "fixed", fixed: "#FF0000" } }, overrides: [] };
const NO_GRID = { defaults: { color: { mode: "fixed", fixed: "#FF0000" }, axis_grid: false }, overrides: [] };
const LEGEND_OFF = { show: false, placement: "bottom", mode: "list", values: [] };

const bar = (id, title, options, o = {}) => panel(id, "barchart", title, SQL, { grid: o.grid || { x: 0, y: (id - 1) * 8, w: 12, h: 8 }, fields: o.fields || RED, options: { show_values: "never", legend: LEGEND_OFF, ...options } });

const DOC = doc(UID, "Bar chart fill, line, grid and auto", [
  section(null, [
    bar(1, "fill 61", { fill_opacity: 61 }),
    bar(2, "fill 0 line 4", { fill_opacity: 0, line_width: 4 }),
    bar(3, "fill 100 line 0", { fill_opacity: 100, line_width: 0 }),
    bar(4, "grid off", {}, { fields: NO_GRID }),
    bar(5, "auto narrow", { orientation: "auto" }, { grid: { x: 12, y: 0, w: 4, h: 12 } }),
    bar(6, "auto wide", { orientation: "auto" }, { grid: { x: 12, y: 12, w: 12, h: 6 } }),
    bar(7, "horizontal fill 61 line 3", { orientation: "horizontal", fill_opacity: 61, line_width: 3 }, { grid: { x: 12, y: 18, w: 12, h: 8 } }),
    bar(8, "horizontal grid off", { orientation: "horizontal" }, { grid: { x: 12, y: 26, w: 12, h: 8 }, fields: NO_GRID }),
  ]),
]);

// The paths of each canvas: { fs, ss, lw, rects, segs } for each fill or stroke, with the rects and the line segments of the path.
const HOOK = `(() => {
  const rec = new WeakMap();
  const proto = CanvasRenderingContext2D.prototype;
  const [begin, rect, moveTo, lineTo, fill, stroke, clear] = [proto.beginPath, proto.rect, proto.moveTo, proto.lineTo, proto.fill, proto.stroke, proto.clearRect];
  let cur = { rects: [], segs: [] };
  const list = (c) => rec.get(c) || rec.set(c, []).get(c);
  proto.beginPath = function () { cur = { rects: [], segs: [] }; return begin.call(this); };
  proto.rect = function (x, y, w, h) { cur.rects.push([x, y, w, h]); return rect.call(this, x, y, w, h); };
  proto.moveTo = function (x, y) { cur.segs.push(["M", x, y]); return moveTo.call(this, x, y); };
  proto.lineTo = function (x, y) { cur.segs.push(["L", x, y]); return lineTo.call(this, x, y); };
  proto.fill = function (...a) { list(this.canvas).push({ k: "fill", fs: String(this.fillStyle), rects: cur.rects.slice() }); return fill.apply(this, a); };
  proto.stroke = function (...a) { list(this.canvas).push({ k: "stroke", ss: String(this.strokeStyle), lw: this.lineWidth, rects: cur.rects.slice(), segs: cur.segs.slice(0, 200) }); return stroke.apply(this, a); };
  proto.clearRect = function (x, y, w, h) { if (x === 0 && y === 0) rec.set(this.canvas, []); return clear.call(this, x, y, w, h); };
  window.__paths = (c) => rec.get(c) || [];
})();`;

const panelSel = (id) => `document.querySelector('[data-testid="panel-${id}"]')`;
/** The paths of the canvas of a panel that drew the bars, and the plot box [left, top, w, h] of the engine. */
const canvasOf = (p, id) => p.eval(`(() => {
  const panel = ${panelSel(id)};
  const c = [...panel.querySelectorAll("canvas")].find((x) => window.__paths(x).some((o) => o.rects.length === 2));
  const root = panel.querySelector(".chartCore");
  return { paths: c ? window.__paths(c) : [], plot: root && root.dataset.plot ? root.dataset.plot.split(" ").map(Number) : null, texts: c ? window.__canvasTexts(c).map((t) => t.text) : [] };
})()`);
const bars = (paths, k) => paths.filter((o) => o.k === k && o.rects.length === 2 && o.rects.every((r) => r[2] > 20));
const rgba = (css) => (css.startsWith("#") ? [...[1, 3, 5].map((i) => parseInt(css.slice(i, i + 2), 16)), 1] : (css.match(/[\d.]+/g) || []).map(Number));
const near = (a, b, tol) => Math.abs(a - b) <= tol;
/** The alpha of a CSS colour of the computed style: `rgba(...)`, `rgb(...)` or `color(srgb r g b / a)`. */
const alphaOf = (css) => {
  const slash = /\/\s*([\d.]+)\)/.exec(css);
  if (slash) return Number(slash[1]);
  const n = (css.match(/[\d.]+/g) || []).map(Number);
  return n.length === 4 ? n[3] : 1;
};

export const cases = [
  {
    id: "B216",
    name: "the fill, the line, the grid lines and the orientation auto of a bar chart have the values of Grafana 12.4.2 (4.18, 4.30, 9.19; P03)",
    timeout: 180000,
    async run(t) {
      const r = await t.api.create("transit", DOC);
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      const p = await t.page({ width: 1280, height: 1400 });
      await p.recordCanvasText();
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: HOOK });
      await p.goto(t.url(`d/${UID}/x`));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 7, 8]);
      await p.waitFor(`[1, 2, 3, 4].every((id) => [...document.querySelectorAll('[data-testid="panel-' + id + '"] canvas')].some((c) => window.__paths(c).some((o) => o.rects.length === 2)))`, { what: "the bars on the canvas" });

      // 1. fill_opacity 61: the fill alpha 0.61 and a line of 1 px in the plain colour (Grafana: rgba(255, 0, 0, 0.61), #ff0000 width 1).
      const c1 = await canvasOf(p, 1);
      const f1 = bars(c1.paths, "fill");
      const s1 = bars(c1.paths, "stroke");
      t.eq(f1.length, 1, "panel 1: one fill of the two bars");
      t.eq(JSON.stringify(rgba(f1[0].fs)), JSON.stringify([255, 0, 0, 0.61]), "panel 1: the fill of Grafana for fillOpacity 61");
      t.eq(s1.length, 1, "panel 1: one stroke of the two bars");
      t.eq(JSON.stringify(rgba(s1[0].ss)), JSON.stringify([255, 0, 0, 1]), "panel 1: the line has the plain colour");
      t.eq(s1[0].lw, 1, "panel 1: the line width of Grafana");

      // 2 and 3. No fill call at 0, no stroke call at 0, and the box of a bar loses half the line at each side (Grafana: 22, 17, 352, 190 at 0 and
      // 24, 19, 348, 186 at 4).
      const c2 = await canvasOf(p, 2);
      const c3 = await canvasOf(p, 3);
      t.eq(bars(c2.paths, "fill").length, 0, "panel 2: fill_opacity 0 gives no fill");
      const s2 = bars(c2.paths, "stroke");
      t.eq(s2.length, 1, "panel 2: one stroke");
      t.eq(s2[0].lw, 4, "panel 2: the line of 4 px");
      t.eq(bars(c3.paths, "stroke").length, 0, "panel 3: line_width 0 gives no stroke");
      const f3 = bars(c3.paths, "fill");
      t.eq(JSON.stringify(rgba(f3[0].fs)), JSON.stringify([255, 0, 0, 1]), "panel 3: fill_opacity 100 gives the plain colour");
      const inner = s2[0].rects;
      const outer = f3[0].rects;
      const insets = inner.map((b, i) => [b[0] - outer[i][0], b[1] - outer[i][1], outer[i][2] - b[2], outer[i][3] - b[3]].map((v) => Math.round(v * 100) / 100));
      t.eq(JSON.stringify(insets), JSON.stringify([[2, 2, 4, 4], [2, 2, 4, 4]]), "the box of a bar at line 4 is the box at line 0 less 2 px at each side");

      // 4. axis_grid false: no grid line across the plot; the labels of the value axis stay (Grafana: Off keeps the ticks and their labels).
      const across = (c) => c.paths.filter((o) => o.k === "stroke").flatMap((o) => o.segs.filter((s, i) => s[0] === "M" && o.segs[i + 1] && near(s[1], c.plot[0], 1) && near(o.segs[i + 1][1], c.plot[0] + c.plot[2], 1) && o.segs[i + 1][2] === s[2])).length;
      const c4 = await canvasOf(p, 4);
      t.assert(across(c1) >= 2, `panel 1: the grid lines of the value axis (${across(c1)})`);
      t.eq(across(c4), 0, "panel 4: no grid line");
      t.eq(JSON.stringify(c4.texts.filter((x) => /^\d+$/.test(x))), JSON.stringify(c1.texts.filter((x) => /^\d+$/.test(x))), "panel 4: the labels of the value axis stay");

      // 5 and 6. orientation auto: horizontal in a plot narrower than it is high, vertical in a wide plot (Grafana: width < height).
      const shape = (id) => p.eval(`(() => { const e = ${panelSel(id)}; const pl = e.querySelector(".panel__plot"); const b = pl.getBoundingClientRect(); return { w: b.width, h: b.height, hbar: !!pl.querySelector(".hbar"), canvas: !!pl.querySelector(".chartCore canvas") }; })()`);
      const s5 = await shape(5);
      const s6 = await shape(6);
      t.assert(s5.w < s5.h, `panel 5 is narrower than it is high: ${s5.w} x ${s5.h}`);
      t.eq([s5.hbar, s5.canvas], [true, false], "panel 5: auto draws horizontal bars");
      t.assert(s6.w >= s6.h, `panel 6 is wider than it is high: ${s6.w} x ${s6.h}`);
      t.eq([s6.hbar, s6.canvas], [false, true], "panel 6: auto draws vertical bars");
      // A wider page makes panel 5 wider than it is high: the chart draws again with vertical bars, and back.
      await p.send("Emulation.setDeviceMetricsOverride", { width: 3600, height: 1400, deviceScaleFactor: 1, mobile: false });
      await p.waitFor(`(() => { const pl = ${panelSel(5)}.querySelector(".panel__plot"); const b = pl.getBoundingClientRect(); return b.width >= b.height && !!pl.querySelector(".chartCore canvas") && !pl.querySelector(".hbar"); })()`, { what: "panel 5 with vertical bars in a wide page" });
      await p.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 1400, deviceScaleFactor: 1, mobile: false });
      await p.waitFor(`(() => { const pl = ${panelSel(5)}.querySelector(".panel__plot"); return !!pl.querySelector(".hbar") && !pl.querySelector(".chartCore"); })()`, { what: "panel 5 with horizontal bars again" });

      // 7. The horizontal bars: the fill at 61 % and a line of 3 px inside the box (CSS border-box), in the plain colour.
      const h7 = await p.eval(`(() => { const b = ${panelSel(7)}.querySelector(".hbar__bar:not([hidden])"); const cs = getComputedStyle(b); return { bg: cs.backgroundColor, border: cs.borderTopWidth, color: cs.borderTopColor, sizing: cs.boxSizing }; })()`);
      t.assert(near(alphaOf(h7.bg), 0.61, 0.005), `panel 7: the fill alpha is 0.61 (${h7.bg})`);
      t.eq([h7.border, h7.sizing], ["3px", "border-box"], "panel 7: a line of 3 px inside the box");
      t.eq(alphaOf(h7.color), 1, "panel 7: the line has the plain colour");
      // 8. Grid off in the horizontal bars: no grid line, and the ticks stay.
      const grid = (id) => p.eval(`(() => { const e = ${panelSel(id)}; return { lines: e.querySelectorAll(".hbar__line").length, ticks: e.querySelectorAll(".hbar__tick").length }; })()`);
      const g7 = await grid(7);
      const g8 = await grid(8);
      t.assert(g7.lines >= 2, `panel 7: grid lines (${g7.lines})`);
      t.eq(g8.lines, 0, "panel 8: no grid line");
      t.assert(g8.ticks >= 2, `panel 8: the ticks stay (${g8.ticks})`);
    },
  },
];
