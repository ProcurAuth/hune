// Browser cases B223 and B224 (4.18, 4.28, 4.30, 9.19; wave H, items P10 and P14): the gradient modes and the cosmetic series options of a time
// series and a bar chart, against the probes of Grafana 12.4.2 of 2026-11-22 (the canvas calls of 83 panels in temporary dashboards, dpr 1 and 2,
// both themes; the values are in tests/frontend/vectors/FE-83-series-options.json):
//   opacity fill 50 of #73BF69: a gradient from the top of the plot (alpha 0.5) to its bottom (alpha 0), the plain line;
//   hue of #73BF69: #9AF3AE at the top, #719A42 at the bottom (dark) or #8AB757 (light), for the fill and the line;
//   scheme: the colours of the colour mode at the values of the scale (the bands of the thresholds, the stops of a continuous mode);
//   the dash array as it is, in device pixels; log base 2: a grid line at each power of 2 and a label at every other one on a plot of 151 to 299 px;
//   a null threshold of 150000 ms joins a gap of 2 minutes and not of 3; the bar alignment; the centred zero; the width, the border and the colour
//   of an axis.
import { doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const T0 = 1794211200;
const RANGE = { from: String(T0 * 1000), to: String((T0 + 3600) * 1000), timezone: "utc", refresh: null };
const ramp = (expr, n = 60, step = 60) => [`SELECT toDateTime(${T0} + number * ${step}, 'UTC') AS t, toFloat64(${expr}) AS v`, `FROM numbers(${n})`];
const RAMP = ramp("10 + number * 80 / 59");
const RAMP100 = ramp("number * 100 / 59");
const BARS = ramp("10 + number * 10", 6, 600);
const LOG2 = ramp("pow(2, number / 6)");
const NULLS = [`SELECT toDateTime(${T0} + number * 60, 'UTC') AS t, if(number IN (10, 20, 21, 30, 31, 32), NULL, toFloat64(20 + number % 7 * 5)) AS v`, "FROM numbers(40)"];
const POS = ramp("10 + number * 40 / 59");
const CAT = "SELECT * FROM (SELECT 'a' AS k, 10 AS v UNION ALL SELECT 'b' AS k, 25 AS v UNION ALL SELECT 'c' AS k, 40 AS v) ORDER BY k";
const CATNEG = "SELECT * FROM (SELECT 'a' AS k, -10 AS v UNION ALL SELECT 'b' AS k, 25 AS v UNION ALL SELECT 'c' AS k, 40 AS v) ORDER BY k";
const LEGEND_OFF = { show: false, placement: "bottom", mode: "list", values: [] };
const GREEN = { mode: "fixed", fixed: "#73BF69" };
const THR = { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 40, color: "#EAB839" }, { value: 70, color: "red" }] };
const fields = (defaults) => ({ defaults: { color: GREEN, ...defaults }, overrides: [] });
let at = 0;
const g = (w = 8, h = 8) => {
  const x = (at % 3) * 8;
  const y = Math.floor(at / 3) * 8;
  at += 1;
  return { x, y, w, h };
};
const ts = (id, title, sql, options, f = {}) => panel(id, "timeseries", title, sql, { grid: g(), fields: fields(f), options: { legend: LEGEND_OFF, ...options } });
const bc = (id, title, sql, options, f = {}) => panel(id, "barchart", title, sql, { grid: g(), fields: fields(f), options: { show_values: "never", legend: LEGEND_OFF, ...options } });

at = 0;
const DOC_P10 = doc("b223-gradients", "Gradient modes", [section(null, [
  ts(1, "opacity", RAMP, { fill_opacity: 50, gradient_mode: "opacity" }),
  ts(2, "hue", RAMP, { fill_opacity: 50, line_width: 2, gradient_mode: "hue" }),
  ts(3, "scheme thresholds", RAMP100, { fill_opacity: 30, gradient_mode: "scheme" }, { color: { mode: "thresholds", fixed: null }, thresholds: THR }),
  ts(4, "scheme continuous", RAMP100, { fill_opacity: 30, gradient_mode: "scheme" }, { color: { mode: "continuous_gr_yl_rd", fixed: null } }),
  ts(5, "scheme fixed", RAMP, { fill_opacity: 30, gradient_mode: "scheme" }),
  ts(6, "bars hue", BARS, { draw: "bars", fill_opacity: 61, gradient_mode: "hue" }),
  bc(7, "bar chart hue", CAT, { fill_opacity: 61, gradient_mode: "hue" }),
  bc(8, "bar chart scheme", CAT, { gradient_mode: "scheme" }, { color: { mode: "continuous_gr_yl_rd", fixed: null } }),
  bc(9, "horizontal hue", CAT, { orientation: "horizontal", fill_opacity: 61, gradient_mode: "hue" }),
])], { time: RANGE });

at = 0;
const DOC_P14 = doc("b224-series-options", "Cosmetic series options", [section(null, [
  ts(1, "dash 10 20", RAMP, { line_width: 2 }, { line_dash: "dash", line_dash_array: [10, 20] }),
  ts(2, "dot 0 20", RAMP, { line_width: 3 }, { line_dash: "dot", line_dash_array: [0, 20] }),
  ts(3, "log 2", LOG2, { y_scale: "log", log_base: 2 }),
  ts(4, "null threshold", NULLS, { span_nulls_ms: 150000 }),
  ts(5, "nulls break", NULLS, {}),
  ts(6, "bars center", BARS, { draw: "bars", fill_opacity: 50 }),
  ts(7, "bars before", BARS, { draw: "bars", fill_opacity: 50, bar_alignment: "before" }),
  ts(8, "bars after", BARS, { draw: "bars", fill_opacity: 50, bar_alignment: "after" }),
  ts(9, "centred zero", POS, {}, { axis_centered_zero: true }),
  ts(10, "axis width 80", RAMP, {}, { axis_width: 80 }),
  ts(11, "border and colour", RAMP, {}, { axis_border: true, axis_color: "series", color: { mode: "fixed", fixed: "#F2495C" } }),
  bc(12, "bar chart axis", CATNEG, {}, { axis_centered_zero: true, axis_width: 70, axis_border: true, axis_color: "series", color: { mode: "fixed", fixed: "#F2495C" } }),
])], { time: RANGE });

// The canvas calls of each canvas: fills, strokes (with the dash and the cap) and texts, with their styles; a gradient gives its line and stops.
const HOOK = `(() => {
  const rec = new WeakMap();
  const P = CanvasRenderingContext2D.prototype;
  const list = (c) => rec.get(c) || rec.set(c, []).get(c);
  const lg = P.createLinearGradient;
  P.createLinearGradient = function (...a) { const g = lg.apply(this, a); g.__g = { line: a.map((v) => Math.round(v * 100) / 100), stops: [] }; return g; };
  const acs = CanvasGradient.prototype.addColorStop;
  CanvasGradient.prototype.addColorStop = function (f, c) { if (this.__g) this.__g.stops.push([f, String(c)]); return acs.call(this, f, c); };
  const style = (v) => (v && v.__g ? v.__g : String(v));
  let cur = { rects: [], segs: [] };
  const [begin, rect, moveTo, lineTo, fill, stroke, text, clear] = [P.beginPath, P.rect, P.moveTo, P.lineTo, P.fill, P.stroke, P.fillText, P.clearRect];
  P.beginPath = function () { cur = { rects: [], segs: [] }; return begin.call(this); };
  P.rect = function (x, y, w, h) { cur.rects.push([x, y, w, h]); return rect.call(this, x, y, w, h); };
  P.moveTo = function (x, y) { if (cur.segs.length < 400) cur.segs.push(["M", x, y]); return moveTo.call(this, x, y); };
  P.lineTo = function (x, y) { if (cur.segs.length < 400) cur.segs.push(["L", x, y]); return lineTo.call(this, x, y); };
  P.fill = function (...a) { list(this.canvas).push({ k: "fill", s: style(this.fillStyle), rects: cur.rects.slice(), segs: cur.segs.length }); return fill.apply(this, a); };
  P.stroke = function (...a) { list(this.canvas).push({ k: "stroke", s: style(this.strokeStyle), lw: this.lineWidth, dash: this.getLineDash(), cap: this.lineCap, rects: cur.rects.slice(), segs: cur.segs.slice() }); return stroke.apply(this, a); };
  P.fillText = function (t, x, y, ...r) { list(this.canvas).push({ k: "text", t: String(t), x, y, s: style(this.fillStyle), align: this.textAlign }); return text.call(this, t, x, y, ...r); };
  P.clearRect = function (x, y, w, h) { if (x === 0 && y === 0) rec.set(this.canvas, []); return clear.call(this, x, y, w, h); };
  window.__ops = (c) => rec.get(c) || [];
})();`;

const sel = (id) => `document.querySelector('[data-testid="panel-${id}"]')`;
/** The calls of the canvas that drew the series of a panel, the plot box [left, top, w, h] and the axes of 9.29. */
const chartOf = (p, id) => p.eval(`(() => {
  const panel = ${sel(id)};
  const root = panel.querySelector(".chartCore");
  const c = [...panel.querySelectorAll("canvas")].map((x) => window.__ops(x)).sort((a, b) => b.length - a.length)[0] || [];
  return { ops: c, plot: root && root.dataset.plot ? root.dataset.plot.split(" ").map(Number) : null, axes: JSON.parse(panel.querySelector(".panel__plot").dataset.axes || "[]") };
})()`);
const rgba = (css) => (css.startsWith("#") ? [...[1, 3, 5].map((i) => parseInt(css.slice(i, i + 2), 16)), 1] : (css.match(/[\d.]+/g) || []).map(Number));
const near = (a, b, tol) => Math.abs(a - b) <= tol;
const grads = (ops, k) => ops.filter((o) => o.k === k && typeof o.s === "object").map((o) => ({ ...o.s, rects: o.rects }));
const stopColours = (gr) => gr.stops.map((s) => rgba(s[1]).slice(0, 3).join(","));

async function open(t, d, ids, theme = null) {
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
  const p = await t.page({ width: 1500, height: 1400 });
  if (theme) await p.emulateColorScheme(theme);
  await p.send("Page.addScriptToEvaluateOnNewDocument", { source: HOOK });
  await p.goto(t.url(`d/${d.uid}/x`));
  await p.idle();
  await waitPanelsReady(p, ids);
  if (theme) t.eq(await p.eval("document.documentElement.dataset.theme || 'dark'"), theme, `the page shows the ${theme} theme`);
  return p;
}
/** The expression that is true when each panel of `ids` drew its chart on a canvas. */
const drawn = (ids) => `[${ids}].every((id) => [...document.querySelectorAll('[data-testid="panel-' + id + '"] canvas')].some((c) => window.__ops(c).length > 3))`;

export const cases = [
  {
    id: "B223",
    name: "the gradient modes opacity, hue and scheme of a time series and a bar chart have the stops and the alphas of Grafana 12.4.2, in both themes (4.28, 4.30, 9.19; P10)",
    timeout: 240000,
    async run(t) {
      for (const theme of ["dark", "light"]) {
        const d = { ...DOC_P10, uid: `b223-gradients-${theme}` };
        const p = await open(t, d, [1, 2, 3, 4, 5, 6, 7, 8, 9], theme);
        await p.waitFor(drawn([1, 2, 3, 4, 6, 7, 8]), { what: "the charts" });
        const dark = theme === "dark";
        // 1. opacity: one fill gradient over the plot, from the alpha 0.5 at the top to 0 at the bottom; the line keeps the plain colour.
        const c1 = await chartOf(p, 1);
        const f1 = grads(c1.ops, "fill");
        t.eq(f1.length, 1, `${theme} 1: one gradient fill`);
        t.eq(JSON.stringify(f1[0].line), JSON.stringify([0, c1.plot[1], 0, c1.plot[1] + c1.plot[3]]), `${theme} 1: the gradient runs from the top to the bottom of the plot`);
        t.eq(JSON.stringify(f1[0].stops.map((s) => rgba(s[1]))), JSON.stringify([[115, 191, 105, 0.5], [115, 191, 105, 0]]), `${theme} 1: the stops of Grafana`);
        t.eq(grads(c1.ops, "stroke").length, 0, `${theme} 1: the line is plain`);
        // 2. hue: the fill and the line, with the two colours of the theme.
        const c2 = await chartOf(p, 2);
        const low = dark ? "113,154,66" : "138,183,87";
        t.eq(JSON.stringify(stopColours(grads(c2.ops, "fill")[0])), JSON.stringify(["154,243,174", low]), `${theme} 2: the hue fill`);
        t.eq(JSON.stringify(stopColours(grads(c2.ops, "stroke")[0])), JSON.stringify(["154,243,174", low]), `${theme} 2: the hue line`);
        t.eq(rgba(grads(c2.ops, "fill")[0].stops[0][1])[3], 0.5, `${theme} 2: the fill alpha`);
        // 3. scheme with thresholds: hard bands green, yellow, red at 40 and 70 of the scale 0..110, from the bottom.
        const c3 = await chartOf(p, 3);
        const s3 = grads(c3.ops, "fill")[0];
        const green = dark ? "115,191,105" : "86,166,75";
        const red = dark ? "242,73,92" : "224,47,68";
        t.eq(JSON.stringify(stopColours(s3)), JSON.stringify([green, green, "234,184,57", "234,184,57", red]), `${theme} 3: the bands of the thresholds`);
        t.assert(near(s3.stops[1][0], 40 / 70, 0.02), `${theme} 3: the step 40 is at 40/70 of the gradient (${s3.stops[1][0]})`);
        t.assert(near(s3.line[1], c3.plot[1] + c3.plot[3], 1), `${theme} 3: the gradient starts at the bottom of the scale`);
        t.eq(JSON.stringify(stopColours(grads(c3.ops, "stroke")[0])), JSON.stringify([green, green, "234,184,57", "234,184,57", red]), `${theme} 3: the line too`);
        // 4. scheme with a continuous mode: the three colours of the theme at 0, 0.5 and 1 from the lowest to the highest value.
        const c4 = await chartOf(p, 4);
        const s4 = grads(c4.ops, "fill")[0];
        t.eq(JSON.stringify(stopColours(s4)), JSON.stringify([green, dark ? "250,222,42" : "242,204,12", red]), `${theme} 4: the stops of Green-Yellow-Red`);
        t.eq(JSON.stringify(s4.stops.map((s) => s[0])), JSON.stringify([0, 0.5, 1]), `${theme} 4: the offsets`);
        // 5. scheme with a fixed colour: no gradient.
        const c5 = await chartOf(p, 5);
        t.eq(grads(c5.ops, "fill").length + grads(c5.ops, "stroke").length, 0, `${theme} 5: the plain colours`);
        // 6 and 7. Bars: one gradient over the plot for all the bars, fill and line.
        for (const id of [6, 7]) {
          const c = await chartOf(p, id);
          const fill = grads(c.ops, "fill").find((o) => o.rects.length);
          t.assert(fill, `${theme} ${id}: a gradient fill of the bars`);
          t.eq(JSON.stringify(stopColours(fill)), JSON.stringify(["154,243,174", low]), `${theme} ${id}: the hue of the bars`);
          t.eq(rgba(fill.stops[0][1])[3], 0.61, `${theme} ${id}: the fill alpha 0.61`);
          t.eq(JSON.stringify(fill.line), JSON.stringify([0, c.plot[1], 0, c.plot[1] + c.plot[3]]), `${theme} ${id}: over the whole plot`);
        }
        // 8. Bar chart scheme: from the value 10 to the value 40.
        const c8 = await chartOf(p, 8);
        t.eq(JSON.stringify(stopColours(grads(c8.ops, "fill").find((o) => o.rects.length))), JSON.stringify([green, dark ? "250,222,42" : "242,204,12", red]), `${theme} 8: the scheme of the bars`);
        // 9. Horizontal bars: one CSS gradient over the track, from the bottom colour at the left to the top colour at the right.
        const bg = await p.eval(`getComputedStyle(${sel(9)}.querySelector(".hbar__bar:not([hidden])")).backgroundImage`);
        t.assert(bg.startsWith("linear-gradient(90deg"), `${theme} 9: a CSS gradient: ${bg.slice(0, 80)}`);
        const cols = (bg.match(/rgba?\([^)]*\)/g) || []).map((c) => rgba(c).slice(0, 3).join(","));
        t.eq(JSON.stringify([cols[0], cols.at(-1)]), JSON.stringify([low, "154,243,174"]), `${theme} 9: the hue of the horizontal bars`);
        await p.close();
      }
    },
  },
  {
    id: "B224",
    name: "the dash array, log base 2, the null threshold, the bar alignment, the centred zero and the width, the border and the colour of an axis have the drawing of Grafana 12.4.2 (4.18, 4.28, 9.19; P14)",
    timeout: 240000,
    async run(t) {
      const p = await open(t, DOC_P14, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
      await p.waitFor(drawn([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]), { what: "the charts" });
      const series = (ops, colour) => ops.filter((o) => o.k === "stroke" && o.segs.length > 6 && typeof o.s === "string" && rgba(o.s).slice(0, 3).join(",") === colour);
      // 1 and 2. The dash array as it is (dpr 1) and the caps.
      const d1 = series((await chartOf(p, 1)).ops, "115,191,105")[0];
      t.eq([JSON.stringify(d1.dash), d1.cap, d1.lw], ["[10,20]", "butt", 2], "1: dash 10, 20 with a butt cap");
      const d2 = series((await chartOf(p, 2)).ops, "115,191,105")[0];
      t.eq([JSON.stringify(d2.dash), d2.cap, d2.lw], ["[0,20]", "round", 3], "2: dot 0, 20 with a round cap");
      // 3. log 2: 1 to 1024, a grid line at each power of 2 and a label at every other one from the top.
      const c3 = await chartOf(p, 3);
      t.assert(c3.plot[3] > 150 && c3.plot[3] < 300, `3: the plot height gives every other label (${c3.plot[3]})`);
      const labels = c3.ops.filter((o) => o.k === "text" && o.align === "right" && o.t !== "").map((o) => o.t);
      t.eq(JSON.stringify(labels), JSON.stringify(["1", "4", "16", "64", "256", "1024"]), "3: the labels of Grafana");
      t.eq(JSON.stringify([c3.axes[0].min, c3.axes[0].max]), JSON.stringify([1, 1024]), "3: the range");
      // 4 and 5. A null threshold of 150000 ms joins the gap of 2 minutes only: 3 runs, where the breaks give 4.
      const runs = (c) => series(c.ops, "115,191,105").reduce((n, s) => n + s.segs.filter((x) => x[0] === "M").length, 0);
      t.eq(runs(await chartOf(p, 4)), 3, "4: 150000 ms joins the run of one null");
      t.eq(runs(await chartOf(p, 5)), 4, "5: with no threshold each run of nulls breaks the line");
      // 6 to 8. Bar alignment: the bar ends half its line before its time, or starts half its line after it.
      const firstBar = async (id) => (await chartOf(p, id)).ops.find((o) => o.k === "fill" && o.rects.length).rects[1];
      const center = await firstBar(6);
      const before = await firstBar(7);
      const after = await firstBar(8);
      const mid = center[0] + center[2] / 2;
      t.assert(near(before[0] + before[2], mid - 0.5, 0.01), `7: before ends at the time less half the line (${before} ${mid})`);
      t.assert(near(after[0], mid + 0.5, 0.01), `8: after starts at the time plus half the line (${after} ${mid})`);
      // 9. Centred zero: the data 10 to 50 gives -60 to 60.
      const c9 = await chartOf(p, 9);
      t.eq(JSON.stringify([c9.axes[0].min, c9.axes[0].max]), JSON.stringify([-60, 60]), "9: the range of Grafana");
      // 10. Axis width 80: the plot starts 80 px from the left of the canvas.
      t.eq((await chartOf(p, 10)).plot[0], 80, "10: the gutter is the width");
      // 11. Border and colour: the labels, the line on the edge of the plot and the ticks of 4 px have the colour of the series.
      const c11 = await chartOf(p, 11);
      const yLabels = c11.ops.filter((o) => o.k === "text" && o.align === "right");
      t.assert(yLabels.length >= 2 && yLabels.every((o) => rgba(o.s).slice(0, 3).join(",") === "242,73,92"), "11: the labels have the colour of the series");
      t.assert(yLabels.every((o) => near(o.x, c11.plot[0] - 9, 0.01)), "11: the labels end 9 px before the plot");
      const edge = c11.ops.filter((o) => o.k === "stroke" && typeof o.s === "string" && rgba(o.s).slice(0, 3).join(",") === "242,73,92" && o.lw === 1).flatMap((o) => o.segs);
      t.assert(edge.some((s, i) => s[0] === "M" && edge[i + 1] && near(edge[i + 1][2] - s[2], c11.plot[3], 1)), "11: a line along the plot");
      t.eq(edge.filter((s, i) => s[0] === "M" && edge[i + 1] && near(s[1] - edge[i + 1][1], 4, 0.01)).length, yLabels.length, "11: a tick of 4 px at each label");
      // 12. Bar chart: the axis -48 to 48, the gutter of 70 px and the colour of the series.
      const c12 = await chartOf(p, 12);
      t.eq(JSON.stringify([c12.axes[0].min, c12.axes[0].max]), JSON.stringify([-48, 48]), "12: the centred value axis of Grafana");
      t.eq(c12.plot[0], 70, "12: the gutter is the width");
      t.assert(c12.ops.filter((o) => o.k === "text" && o.align === "right").every((o) => rgba(o.s).slice(0, 3).join(",") === "242,73,92"), "12: the labels have the colour of the series");
    },
  },
];
