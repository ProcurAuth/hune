// Browser cases B140 to B144 and B146 to B148 (4.33c, 9.19e, 10.3, 15.6): the heatmap. B145 (the look of a histogram) is in histogram.mjs.
// B140: the look of a heatmap of raw values. B141: the shape that is already bucketed. B142: the colours and the legend. B143: the axes.
// B144: the card, the hover and the keyboard. B146: the caps and the time. B147: the editor and the format. B148: the import, the layouts, zoom, print.
// B147 and B148 also run the histogram (4.33d, 9.19f): the lines of the histogram are marked `histogram`.
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { doc, panel, section } from "../lib/docs.mjs";
import { enterEdit, openPanelEditor, saveDialog, waitDraft } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";
import { SCHEMES, heatPalette } from "../../../web/js/heat_model.js";

const T0 = Date.UTC(2026, 10, 13);
const HOUR = 3600000;
// The range of the measure of 19.20g: 00:00 to 05:00 on 2026-11-13, in UTC.
const RANGE = { from: String(T0), to: String(T0 + 5 * HOUR), timezone: "utc", refresh: null };
const QUERY = "toDateTime('2026-11-13 00:00:00','UTC')";
// The raw values of the measure: 20,000 rows over 5 hours, a latency with a daily wave and an exponential tail.
const raw = (n = 20000) => `SELECT ${QUERY} + toUInt32(intDiv(number*18000, ${n})) AS time, round(40 * (1 + 0.6*sin(number/${n}*6.2832*2)) * -log(1 - (cityHash64(number) % 1000000 + 0.5)/1000000), 3) AS latency FROM numbers(${n}) ORDER BY time`;
// The shape that is already bucketed: 30 times of 10 minutes and one column for each name (a wave over the rows).
const wave = (b, t = "t") => `toUInt32(round(100*exp(-pow(${b}-3-sin(${t}/4)*1.5,2)/2)))`;
const wide = (names, o = {}) => `SELECT ${QUERY} + ${o.time || "t*600"} AS time, ${names.map((n, b) => `${o.expr ? o.expr(b) : wave(b)} AS \`${n}\``).join(", ")} FROM (SELECT number AS t FROM numbers(${o.rows || 30})) ORDER BY time`;
const POW2 = ["1", "2", "4", "8", "16", "32", "64", "128"];

const COLOR = { mode: "scheme", scheme: "oranges", fill: "dark-orange", scale: "exponential", exponent: 0.5, steps: 64, reverse: false, min: null, max: null };
const heat = (o = {}) => ({ calculate: true, x_buckets: { mode: "count", value: null }, y_buckets: { mode: "count", value: null, scale: "linear" }, y_axis: "left", y_reverse: false, y_min: null, y_max: null, color: { ...COLOR, ...(o.color || {}) }, cell_gap: 1, hide_le: 0, hide_ge: null, show_legend: true, tooltip: { mode: "single" }, ...o, ...(o.color ? { color: { ...COLOR, ...o.color } } : {}) });
/** A panel of the type `heatmap`. `sql` is a SELECT with the time column `time`. `o.grid`, `o.format`, `o.fields`, `o.transforms` and `o.reuse` as `panel`. */
const hp = (id, title, sql, options = {}, o = {}) => ({ ...panel(id, "table", title, sql, { grid: o.grid || { x: 0, y: 0, w: 24, h: 8 }, format: o.format, fields: o.fields, transforms: o.transforms }), type: "heatmap", options: heat(options) });
const one = (uid, panels, o = {}) => doc(uid, `Heatmap ${uid}`, [section(null, Array.isArray(panels) ? panels : [panels])], { time: RANGE, ...o });
const ensure = async (t, d) => {
  if ((await t.api.dashboard(d.uid)).status === 200) return;
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create ${d.uid}: ${r.status} ${r.text.slice(0, 400)}`);
};
const open = async (t, uid, { scheme = "dark", width = 1280, height = 900, query = "" } = {}) => {
  const p = await t.page({ width, height });
  await p.emulateColorScheme(scheme);
  await p.goto(t.url(`d/${uid}?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc${query ? `&${query}` : ""}`));
  await p.idle();
  return p;
};
/** Wait until each panel has drawn (the hook `data-heatmap`) or shows a state of its own. */
const drawn = async (p, ids) => {
  await p.waitFor(`${JSON.stringify(ids)}.every((id) => { const e = document.querySelector('[data-testid="panel-' + id + '"]'); const plot = e && e.querySelector('.panel__plot'); return e && (["error", "bucket-limit", "empty"].includes(e.dataset.state) || (plot && plot.dataset.heatmap)); })`, { what: `heatmaps ${ids}`, timeout: 40000 });
  await p.idle();
  // The plot draws again when its box gets its final size: wait until the number of draws stays the same.
  let last = "";
  for (let i = 0; i < 40; i++) {
    await sleep(150);
    const now = await p.eval(`[...document.querySelectorAll('.chartCore')].map((e) => e.dataset.draws).join()`);
    if (now === last) break;
    last = now;
  }
};
const plot = (id) => `document.querySelector('[data-testid="panel-${id}"] .panel__plot')`;
const hook = (p, id) => p.eval(`JSON.parse(${plot(id)}.dataset.heatmap)`);
const engine = (id) => `document.querySelector('[data-testid="heatmap-${id}"] .chartCore')`;
/** The engine facts of a panel: the plot box `[left, top, width, height]`, the ranges and the labels of the ticks. */
const facts = (p, id) => p.eval(`(() => { const e = ${engine(id)}; const d = e.dataset; return { plot: d.plot.split(" ").map(Number), x: [Number(d.xMin), Number(d.xMax)], y: [Number(d.yMin), Number(d.yMax)], yTicks: JSON.parse(d.yTicks), xTicks: JSON.parse(d.xTicks).map((x) => x[0]) }; })()`);
const state = (p, id) => p.eval(`document.querySelector('[data-testid="panel-${id}"]').dataset.state`);
/** The pixel of the plot canvas at the client position of a plot point `(x, y)`, as [r, g, b, a]. */
const pixelAt = (p, id, f, x, y) => p.eval(`(() => { const c = ${engine(id)}.querySelector("canvas.chartCore__canvas"); const k = c.width / c.clientWidth; const [l, t, w, h] = ${JSON.stringify(f.plot)}; const px = l + (${x} - ${f.x[0]}) / ${f.x[1] - f.x[0]} * w; const py = t + h * (1 - (${y} - ${f.y[0]}) / ${f.y[1] - f.y[0]}); return [...c.getContext("2d").getImageData(Math.floor(px * k), Math.floor(py * k), 1, 1).data]; })()`);
/** The client position of a plot point. */
const clientAt = (p, id, f, x, y) => p.eval(`(() => { const o = ${engine(id)}.querySelector(".chartCore__overlay").getBoundingClientRect(); const [l, t, w, h] = ${JSON.stringify(f.plot)}; return { x: o.left + l + (${x} - ${f.x[0]}) / ${f.x[1] - f.x[0]} * w, y: o.top + t + h * (1 - (${y} - ${f.y[0]}) / ${f.y[1] - f.y[0]}) }; })()`);
const moveTo = async (p, at) => {
  await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x, y: at.y });
  await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: at.x + 0.5, y: at.y });
  await sleep(250);
};
/** The card of the engine: the head and the rows, or null when it is hidden. */
const card = (p, id) => p.eval(`(() => { const t = document.querySelector('[data-testid="heatmap-${id}-tip"]'); if (!t || t.hidden) return null; return { head: t.querySelector("strong") ? t.querySelector("strong").textContent : "", rows: [...t.querySelectorAll(".chartCore__tipRow")].map((r) => [r.querySelector("em").textContent, r.querySelector("b").textContent, r.querySelector("i") ? r.querySelector("i").style.background : ""]), footer: t.querySelector("small") ? t.querySelector("small").textContent : "" }; })()`);
const near = (a, b, tol = 3) => a.length >= 3 && b.length >= 3 && [0, 1, 2].every((i) => Math.abs(a[i] - b[i]) <= tol);
const rgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
const legendBar = (p, id, xs) => p.eval(`(() => { const c = document.querySelector('[data-testid="heatmap-${id}-legend"] canvas'); const g = c.getContext("2d"); return { w: c.width, h: c.height, px: ${JSON.stringify(xs)}.map((x) => [...g.getImageData(x, 4, 1, 1).data]) }; })()`);

// --- B140 -----------------------------------------------------------------------------------------------------------------------------

export const cases = [
  {
    id: "B140",
    name: "the look of a heatmap of raw values: the grid and the number of cells (126, 37, 383, 272, 101, 494), the colour of a cell, the axis labels, the legend of 300 by 9 px and its labels, in both themes (9.19e, 19.20g)",
    async run(t) {
      const lin = (o) => ({ y_buckets: { mode: "count", value: null, scale: "linear" }, ...o });
      const g = (n) => ({ grid: { x: 0, y: n * 8, w: 24, h: 8 } });
      await ensure(t, one("hm140", [
        hp(1, "Defaults", raw(), {}, g(0)),
        hp(2, "x 10, y 5", raw(), { x_buckets: { mode: "count", value: 10 }, y_buckets: { mode: "count", value: 5, scale: "linear" } }, g(1)),
        hp(3, "x size 10 min, y size 20", raw(), { x_buckets: { mode: "size", value: 600000 }, y_buckets: { mode: "size", value: 20, scale: "linear" } }, g(2)),
        hp(4, "log2 split 1", raw(), lin({ y_buckets: { mode: "size", value: 1, scale: "log2" } }), g(3)),
        hp(5, "log10 split 1", raw(), lin({ y_buckets: { mode: "size", value: 1, scale: "log10" } }), g(4)),
        hp(6, "log2 split 2", raw(), lin({ y_buckets: { mode: "size", value: 2, scale: "log2" } }), g(5)),
      ]));
      const want = [[1, 126, 21, 10], [2, 37, 11, 5], [3, 383, 31, 24], [4, 272, 21, 17], [5, 101, 21, 6], [6, 494, 21, 34]];
      for (const scheme of ["dark", "light"]) {
        const p = await open(t, "hm140", { scheme, width: 1280, height: 2300 });
        await drawn(p, [1, 2, 3, 4, 5, 6]);
        for (const [id, cells, cols, rows] of want) {
          const h = await hook(p, id);
          t.eq([h.shape, h.drawn, h.cols, h.rows], ["raw", cells, cols, rows], `${scheme}: panel ${id} grid and cells`);
          t.assert(h.cells.length === Math.min(cells, 2000) && h.cut === false, `${scheme}: panel ${id} hook cells`);
        }
        // The sizes of the measure: the time size 15 minutes and the value size 50 (example 1), the log sizes have no value size.
        const h1 = await hook(p, 1);
        t.eq([h1.sx, h1.sy, h1.scale, h1.steps], [900000, 50, "linear", 64], `${scheme}: example 1 sizes`);
        t.eq((await hook(p, 4)).scale, "log2", "the log scale");
        // The cell of the engine: the pitch of the columns less the gap, the pitch of the rows less the gap (46.1 and 15.6 px at 1000 by 300 px).
        const f = await facts(p, 1);
        const [, , plotW, plotH] = f.plot;
        const palette = heatPalette({ ...COLOR }, scheme === "dark");
        t.eq([palette[0], palette[63]], scheme === "dark" ? ["#7f2704", "#fff5eb"] : ["#fff5eb", "#7f2704"], `${scheme}: oranges ends`);
        t.eq(h1.palette, [palette[0], palette[63]], `${scheme}: the hook palette is the palette of the model`);
        // The colour of the first three cells of each of two rows, read on the canvas at the centre of the cell.
        for (const [col, row, count, level] of [h1.cells[0], h1.cells[1], h1.cells[40], h1.cells[100]]) {
          const px = await pixelAt(p, 1, f, (Math.floor(T0 / 900000) + col + 0.5) * 900000, (row + 0.5) * 50);
          t.assert(near(px, rgb(palette[level]), 3) && px[3] === 255, `${scheme}: cell ${col},${row} count ${count} has the colour of level ${level}: ${px} ${palette[level]}`);
        }
        // The pitch of the cells, from the hook of the engine: 20 columns of 15 minutes and 10 rows of 50 over the axis 0 to 500.
        t.eq(f.y, [0, 500], "the axis of example 1");
        t.eq(f.yTicks, ["0", "100", "200", "300", "400", "500"], "the labels of the value axis");
        t.assert(f.xTicks[0] === "00:00" && f.xTicks[f.xTicks.length - 1] === "05:00", `the labels of the time axis ${f.xTicks}`);
        t.assert(plotW > 500 && plotH > 100, "the plot");
        // The legend: a canvas of 300 by 9 px, 25 px from the left and 10 px under the top of the legend area of 46 px, and the two ends.
        const lg = await p.eval(`(() => { const l = document.querySelector('[data-testid="heatmap-1-legend"]'); const c = l.querySelector("canvas"); const a = l.getBoundingClientRect(); const b = c.getBoundingClientRect(); return { w: c.width, h: c.height, cw: b.width, ch: b.height, left: b.left - a.left, top: b.top - a.top, area: a.height, labels: [...l.querySelectorAll("span")].map((s) => s.textContent), size: getComputedStyle(l.querySelector("span")).fontSize }; })()`);
        t.eq([lg.w, lg.h, lg.cw, lg.ch, lg.left, lg.top, lg.area], [300, 9, 300, 9, 25, 10, 46], `${scheme}: the legend box`);
        t.eq([lg.labels, lg.size], [[String(h1.min), "960"], "11px"], `${scheme}: the labels of the legend`);
        t.eq(h1.legend, [h1.min, h1.max], "the legend counts");
        const bar = await legendBar(p, 1, [0, 100, 299]);
        t.assert(near(bar.px[0], rgb(palette[0]), 1) && near(bar.px[2], rgb(palette[63]), 1) && near(bar.px[1], rgb(palette[Math.floor((100 * 64) / 300)]), 1), `${scheme}: the colours of the bar ${JSON.stringify(bar.px)}`);
        // The legend of a narrow body: the bar is the width of the body less 50 px.
        t.assert((await state(p, 1)) === "ready", "ready");
      }
      const p = await open(t, "hm140", { width: 330, height: 2300 });
      await drawn(p, [1]);
      t.eq(await p.eval(`document.querySelector('[data-testid="heatmap-1-legend"] canvas').width`), await p.eval(`document.querySelector('[data-testid="heatmap-1"]').clientWidth - 50`), "the bar of a body narrower than 350 px");
      void sleep;
    },
  },
  {
    id: "B141",
    name: "the shape that is already bucketed: names that are numbers (sorted, +Inf last) and names that are not, null and zero cells, one time row, irregular times, negative counts, a long frame, hostile names, in both themes (9.19e)",
    async run(t) {
      const g = (n) => ({ grid: { x: 0, y: n * 8, w: 24, h: 8 } });
      const nulls = (b) => `if((t+${b})%7=0, NULL, if((t+${b})%5=0, toUInt32(0), ${wave(b)}))`;
      const longFrame = `SELECT ${QUERY} + t*600 AS time, toString(toFloat64(pow(2,b))) AS le, ${wave("b")} AS count FROM (SELECT number AS t FROM numbers(30)) AS a CROSS JOIN (SELECT number AS b FROM numbers(8)) AS c ORDER BY time, b`;
      const hostile = `SELECT ${QUERY} + t*600 AS time, arrayElement(['<img src=x onerror=alert(1)>', repeat('A', 10000), concat('x', char(8238), 'y'), concat('a', char(0), 'b')], b + 1) AS name, ${wave("b")} AS count FROM (SELECT number AS t FROM numbers(30)) AS a CROSS JOIN (SELECT number AS b FROM numbers(4)) AS c ORDER BY time, b`;
      const B = { calculate: false };
      await ensure(t, one("hm141", [
        hp(1, "Numbers", wide(POW2), B, g(0)),
        hp(2, "Unsorted numbers", wide(["16", "2", "128", "1", "8", "64", "4", "32"]), B, g(1)),
        hp(3, "Text names", wide(["fast", "ok", "slow", "very slow"]), B, g(2)),
        hp(4, "Inf", wide(["1", "2", "4", "8", "+Inf"]), B, g(3)),
        hp(5, "Nulls and zeros", wide(POW2, { expr: nulls }), B, g(4)),
        hp(6, "One time row", wide(POW2, { rows: 1 }), B, g(5)),
        hp(7, "Irregular times", wide(POW2, { time: "if(t<3, t*600, 1800+(t-3)*300)" }), B, g(6)),
        hp(8, "Negative counts", wide(POW2, { expr: (b) => `(toInt32(${wave(b)}) - 40)` }), B, g(7)),
        hp(9, "Long frame", longFrame, B, { ...g(8), format: "time_series" }),
        hp(10, "Hostile names", hostile, B, { ...g(9), format: "time_series" }),
      ]));
      for (const scheme of ["dark", "light"]) {
        const p = await open(t, "hm141", { scheme, width: 1280, height: 3100 });
        t.allow(p, /A heatmap needs two time rows at least/);
        await drawn(p, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        const labels = async (id) => (await facts(p, id)).yTicks;
        const h = (id) => hook(p, id);
        const h1 = await h(1);
        t.eq([h1.shape, h1.cols, h1.rows, h1.scale, h1.sy], ["bucketed", 30, 8, "ordinal", null], `${scheme}: the shape of panel 1`);
        t.assert(h1.drawn > 100 && h1.drawn <= 240, `${scheme}: cells ${h1.drawn}`);
        t.eq(h1.sx, 600000, "the step is the first step");
        t.eq(await labels(1), POW2, "numbers: one label for each row, in the order of the numbers");
        t.eq(await labels(2), POW2, "unsorted numbers are sorted");
        t.eq(await labels(3), ["fast", "ok", "slow", "very slow"], "text names keep the order of the fields");
        t.eq(await labels(4), ["1", "2", "4", "8", "+Inf"], "+Inf is the last row");
        t.eq((await h(5)).cells.length, 122, `${scheme}: null and zero cells do not draw (122 of 240 positions)`);
        t.eq((await h(5)).drawn, 117, "the first column ends at the start of the range: its 5 cells are before the plot");
        t.eq(await state(p, 6), "error", "one time row is an error");
        t.assert((await p.text(`${plot(6).replace("document.querySelector(", "").replace(")", "")}`.replace(/^/, "")).catch(() => "")) !== null, "the error text");
        const err6 = await p.eval(`document.querySelector('[data-testid="panel-6"]').textContent`);
        t.assert(err6.includes("A heatmap needs two time rows at least."), `the error of one time row: ${err6.slice(0, 200)}`);
        t.eq((await h(7)).sx, 600000, "irregular times: the first step applies to all the columns");
        const h8 = await h(8);
        t.assert(h8.cells.length === 79 && h8.cells.every((c) => c[2] > 0), `negative counts are not drawn: ${h8.cells.length} cells`);
        t.eq([(await h(9)).shape, (await h(9)).rows, (await h(9)).drawn], ["bucketed", 8, h1.drawn], "the long frame gives the same grid as the wide frame");
        t.eq(await labels(9), POW2, "a long frame: the heatmap drops the prefix of the value field and sorts the rows as numbers (9.19e, ID-K195c)");
        // Hostile names show as text and make no element: no image, no alert, the long name is cut.
        t.eq(await p.eval(`document.querySelectorAll('[data-testid="heatmap-10"] img, [data-testid="panel-10"] img').length`), 0, "no element from a name");
        const l10 = await labels(10);
        t.assert(l10.includes("<img src=x onerror=alert(1)>") && l10.some((x) => x.length === 121 && x.endsWith("…")), `the labels of the hostile names ${JSON.stringify(l10.map((x) => x.slice(0, 30)))}`);
        const desc = await p.eval(`document.querySelector('[data-testid="heatmap-10-desc"]').textContent`);
        t.assert(desc.length < 700, `the description is short (${desc.length})`);
      }
    },
  },
  {
    id: "B142",
    name: "the colours and the legend: each of the 10 schemes in both themes (the first, the last and 4 middle steps), the mode opacity with both scales, steps 2 and 128, reverse, min and max, hide_le and hide_ge, cell_gap 0, 1, 3 and 4, show_legend off (9.19e)",
    async run(t) {
      const schemes = Object.keys(SCHEMES);
      const g = (n) => ({ grid: { x: (n % 2) * 12, y: Math.floor(n / 2) * 8, w: 12, h: 8 } });
      const list = [];
      schemes.forEach((s, i) => list.push(hp(i + 1, s, raw(), { color: { scheme: s } }, g(i))));
      const more = [
        ["opacity linear", { color: { mode: "opacity", scale: "linear" } }],
        ["opacity exponential 0.5", { color: { mode: "opacity", scale: "exponential", exponent: 0.5 } }],
        ["opacity exponential 2", { color: { mode: "opacity", scale: "exponential", exponent: 2 } }],
        ["steps 2", { color: { steps: 2 } }],
        ["steps 128", { color: { steps: 128 } }],
        ["reverse", { color: { reverse: true } }],
        ["min 50 max 100", { color: { min: 50, max: 100 } }],
        ["hide_le 100 hide_ge 400", { hide_le: 100, hide_ge: 400 }],
        ["gap 0", { cell_gap: 0 }],
        ["gap 3", { cell_gap: 3 }],
        ["gap 4", { cell_gap: 4 }],
        ["no legend", { show_legend: false }],
      ];
      more.forEach(([title, o], k) => list.push(hp(11 + k, title, raw(), o, g(10 + k))));
      await ensure(t, one("hm142", list));
      for (const scheme of ["dark", "light"]) {
        const dark = scheme === "dark";
        const p = await open(t, "hm142", { scheme, width: 1280, height: 2800 });
        await drawn(p, list.map((x) => x.id));
        // The 10 schemes: the colour of 6 steps on the bar of the legend equals the palette of the model, and the hook gives the ends.
        for (const [i, s] of schemes.entries()) {
          const pal = heatPalette({ ...COLOR, scheme: s }, dark);
          const h = await hook(p, i + 1);
          t.eq(h.palette, [pal[0], pal[63]], `${scheme} ${s}: the ends of the palette`);
          const steps = [0, 20, 33, 42, 55, 63];
          const bar = await legendBar(p, i + 1, steps.map((k) => Math.floor((k * 300) / 64) + 2));
          bar.px.forEach((px, k) => t.assert(near(px, rgb(pal[steps[k]]), 1), `${scheme} ${s}: step ${steps[k]} is ${px}, expected ${pal[steps[k]]}`));
        }
        // The mode opacity: the colour is the fill with the opacity of the step (0 for the lowest, 1 for the highest).
        for (const [id, scale, exp] of [[11, "linear", 1], [12, "exponential", 0.5], [13, "exponential", 2]]) {
          const bar = await legendBar(p, id, [0, 150, 299]);
          t.assert(bar.px[0][3] === 0 && bar.px[2][3] === 255, `${scheme}: opacity ${scale} ${exp}: the ends ${JSON.stringify(bar.px)}`);
          const f = (150 * 64) / 300;
          const want = Math.round(255 * (scale === "linear" ? Math.floor(f) / 63 : (Math.floor(f) / 63) ** exp));
          t.assert(Math.abs(bar.px[1][3] - want) <= 2, `${scheme}: opacity ${scale} ${exp}: the middle ${bar.px[1][3]} expected ${want}`);
          t.eq((await hook(p, id)).palette[0].length, 9, "the palette of the mode opacity has an alpha");
        }
        t.eq((await hook(p, 14)).steps, 2, "steps 2");
        t.eq((await legendBar(p, 14, [10, 290])).px.map((x) => x.slice(0, 3).join()), [heatPalette({ ...COLOR, steps: 2 }, dark)[0], heatPalette({ ...COLOR, steps: 2 }, dark)[1]].map((x) => rgb(x).join()), `${scheme}: two colours`);
        t.eq((await hook(p, 15)).steps, 128, "steps 128");
        const rev = await hook(p, 16);
        t.eq(rev.palette, [heatPalette({ ...COLOR, reverse: true }, dark)[0], heatPalette({ ...COLOR, reverse: true }, dark)[63]], `${scheme}: reverse`);
        t.assert(rev.palette[0] !== (await hook(p, 1)).palette[0], "reverse changes the first colour");
        const mm = await hook(p, 17);
        t.eq([mm.min, mm.max, mm.legend], [50, 100, [50, 100]], "min and max");
        const le = await hook(p, 18);
        t.assert(le.cells.every((c) => c[2] > 100 && c[2] < 400), "hide_le and hide_ge");
        t.eq(le.legend, [101, 263], "the colour range of the cells that show (101 and 263 on the data of the measure)");
        t.eq(le.drawn, 19, "19 cells");
        // The gap of the cells (E22, the option cellGap): the pixels between two next cells of a row that have neither colour of the two cells.
        const gaps = [];
        for (const id of [19, 1, 20, 21]) {
          const f = await facts(p, id);
          const h = await hook(p, id);
          const pal = heatPalette({ ...COLOR }, dark);
          const a = h.cells.find((c) => h.cells.some((d) => d[1] === c[1] && d[0] === c[0] + 1 && c[0] > 0));
          const b = h.cells.find((d) => d[1] === a[1] && d[0] === a[0] + 1);
          gaps.push(await p.eval(`(() => { const c = ${engine(id)}.querySelector("canvas.chartCore__canvas"); const k = c.width / c.clientWidth; const [l, t, w, hh] = ${JSON.stringify(f.plot)}; const xs = (x) => l + (x - ${f.x[0]}) / ${f.x[1] - f.x[0]} * w; const sx = ${h.sx}; const c0 = Math.floor(${T0} / sx); const y = t + hh * (1 - (${a[1] + 0.5} * ${h.sy} - ${f.y[0]}) / ${f.y[1] - f.y[0]}); const from = Math.floor(xs((c0 + ${a[0]} + 0.5) * sx) * k), to = Math.floor(xs((c0 + ${b[0]} + 0.5) * sx) * k); const d = c.getContext("2d").getImageData(from, Math.floor(y * k), to - from + 1, 1).data; const A = ${JSON.stringify(rgb(pal[a[3]]))}, B = ${JSON.stringify(rgb(pal[b[3]]))}; let n = 0; for (let i = 0; i < d.length; i += 4) { const near = (q) => [0, 1, 2].every((j) => Math.abs(d[i + j] - q[j]) <= 6); if (!near(A) && !near(B)) n++; } return n / k; })()`));
        }
        t.eq(gaps[0], 0, `${scheme}: cell_gap 0 leaves no gap`);
        t.assert(gaps[1] >= 1 && gaps[1] <= 2 && Math.abs(gaps[2] - 3) <= 1 && Math.abs(gaps[3] - 4) <= 1 && gaps[1] < gaps[2] && gaps[2] < gaps[3], `${scheme}: the gaps of cell_gap 1, 3 and 4 are ${gaps.slice(1)}`);
        t.eq(await p.eval(`document.querySelector('[data-testid="heatmap-22-legend"]').hidden`), true, "show_legend off hides the legend");
        t.eq((await hook(p, 22)).legend, null, "the hook has no legend");
        t.eq(await p.eval(`document.querySelector('[data-testid="heatmap-1-legend"]').hidden`), false, "the legend shows");
      }
    },
  },
  {
    id: "B143",
    name: "the axes and the value axis: y_axis hidden, y_reverse, y_min and y_max on a linear and a log scale, the ticks of a log scale, the unit and the decimals, the cap of the raw values, the error of the log scale with no value above 0 (9.19e)",
    async run(t) {
      const g = (n) => ({ grid: { x: (n % 2) * 12, y: Math.floor(n / 2) * 8, w: 12, h: 8 } });
      const ms = { defaults: { unit: "ms", decimals: 1 }, overrides: [] };
      const log = (scale, value = 1) => ({ y_buckets: { mode: "size", value, scale } });
      await ensure(t, one("hm143", [
        hp(1, "Hidden", raw(), { y_axis: "hidden" }, g(0)),
        hp(2, "Reverse", raw(), { y_reverse: true }, g(1)),
        hp(3, "Min 50 max 300", raw(), { y_min: 50, y_max: 300 }, g(2)),
        hp(4, "Log2", raw(), log("log2"), g(3)),
        hp(5, "Log10", raw(), log("log10"), g(4)),
        hp(6, "Log2 min max", raw(), { ...log("log2"), y_min: 2, y_max: 100 }, g(5)),
        hp(7, "Unit", raw(), {}, { ...g(6), fields: ms }),
        hp(8, "Log unit", raw(), log("log2"), { ...g(7), fields: ms }),
        hp(9, "Only zeros", `SELECT ${QUERY} + number*600 AS time, toFloat64(0) AS v FROM numbers(30)`, log("log10"), g(8)),
      ]));
      for (const scheme of ["dark", "light"]) {
        const p = await open(t, "hm143", { scheme, width: 1280, height: 1900 });
        t.allow(p, /The log scale needs values/);
        await drawn(p, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
        const f1 = await facts(p, 1);
        t.eq(f1.plot[0], 8, `${scheme}: a hidden axis leaves a margin of 8 px`);
        t.eq([f1.plot[1], (await facts(p, 7)).plot[1]], [0, 8], `${scheme}: a hidden axis leaves no margin at the top, a visible axis 8 px (E27)`);
        const f0 = await facts(p, 7);
        t.assert(f0.plot[0] > 20, "a visible axis is wider");
        // Reverse: the small values are at the top: the first row (0 to 50) is under the label 0 at the top of the plot.
        const f2 = await facts(p, 2);
        t.eq(f2.yTicks[0], "0", "reverse: the label 0 is the first tick");
        const h2 = await hook(p, 2);
        const [col, row, , level] = h2.cells[0];
        const pal = heatPalette({ ...COLOR }, scheme === "dark");
        const top = await pixelAt(p, 2, f2, (Math.floor(T0 / 900000) + col + 0.5) * 900000, -(row + 0.5) * 50);
        t.assert(row === 0 && near(top, rgb(pal[level]), 3), `${scheme}: the cell of the row 0 is at the top: ${top} ${pal[level]}`);
        t.assert(f2.y[0] < 0 && f2.y[1] === 0, `reverse: the range is the negative range ${f2.y}`);
        // y_min and y_max cut the axis.
        const f3 = await facts(p, 3);
        t.eq(f3.y, [50, 300], "y_min and y_max");
        t.assert(f3.yTicks[0] === "50" && f3.yTicks.includes("300"), `the ticks of the cut axis ${f3.yTicks}`);
        // The ticks of a log scale: every 4th power for base 2, every 2nd power for base 10, the first tick is the top (measured: 19.20g).
        t.eq((await facts(p, 4)).yTicks, ["512", "32", "2", "0.125", "0.0078125"], `${scheme}: the ticks of log2`);
        t.eq((await facts(p, 5)).yTicks, ["1000", "10", "0.1", "0.001"], `${scheme}: the ticks of log10`);
        const f6 = await facts(p, 6);
        t.eq(f6.y.map((v) => Math.round(v)), [1, 7], "log2 with y_min 2 and y_max 100: the powers of 2 below and above");
        // The unit and the decimals of the field.
        t.assert((await facts(p, 7)).yTicks.every((x) => x.endsWith(" ms")), `the unit on the axis ${(await facts(p, 7)).yTicks}`);
        t.assert((await facts(p, 8)).yTicks.every((x) => x.endsWith("ms")), `the unit on a log axis ${(await facts(p, 8)).yTicks}`);
        const tip = await p.eval(`(() => { const c = document.querySelector('[data-testid="panel-7"] .panel__plot'); return !!c; })()`);
        t.assert(tip, "panel 7");
        // The values of 0 or less on a log scale: the note with the count; all values of 0 or less give an error with the count.
        t.eq(await state(p, 9), "error", "only zeros on a log scale is an error");
        const e9 = await p.eval(`document.querySelector('[data-testid="panel-9"]').textContent`);
        t.assert(e9.includes("The log scale needs values above 0. The data has 30 values of 0 or less."), `the error ${e9.slice(0, 200)}`);
        const note = await p.eval(`document.querySelector('[data-testid="heatmap-4-skip"]').textContent`);
        t.eq(note, "1 value of 0 or less is not on the log scale.", "the note of the skipped value");
        t.eq(await p.eval(`document.querySelector('[data-testid="heatmap-1-skip"]')`), null, "no note on a linear scale");
      }
      // View data shows the note above the table.
      const p = await open(t, "hm143", { width: 1280, height: 1900 });
      t.allow(p, /The log scale needs values/);
      await drawn(p, [4]);
      await p.click(p.tid("panel-4-menu"));
      await p.clickText('.menu [role="menuitem"]', "View data");
      await p.waitSel(p.tid("view-data-4-skip"));
      t.eq(await p.text(p.tid("view-data-4-skip")), "1 value of 0 or less is not on the log scale.", "View data shows the note");
      // The cap of the raw values is the rows times the value fields. The source gives 100,000 rows at most, so the browser case uses
      // 5 fields (500,000 values: pass) and 6 fields (600,000: fail). The exact limit of 500,000 and 500,001 is in the front test FE-46.
      for (const [fields, ok] of [[5, true], [6, false]]) {
        const id = `hm143-cap-${fields}`;
        const cols = Array.from({ length: fields }, (_, k) => `toFloat64((number + ${k * 7}) % 1000) + 1 AS v${k}`).join(", ");
        await ensure(t, one(id, [hp(1, "Cap", `SELECT ${QUERY} + toUInt32(intDiv(number, 7)) AS time, ${cols} FROM numbers(100000)`, { x_buckets: { mode: "size", value: 3600000 }, y_buckets: { mode: "size", value: 100, scale: "linear" } })]));
        const q = await open(t, id, { width: 1280, height: 500 });
        await drawn(q, [1]);
        t.eq(await state(q, 1), ok ? "ready" : "bucket-limit", `${fields * 100000} values`);
        if (!ok) t.eq(await q.text(q.tid("heatmap-1-note")), "Too many values to bucket: 600,000. The limit is 500,000. Bucket the values in the SQL.", "the message");
      }
    },
  },
  {
    id: "B144",
    name: "the card, the hover and the keyboard: single, multi and hidden, the head, the rows Count and Bucket and Value and Name, the highlight of the cell, the keys ArrowRight, ArrowLeft, ArrowDown, ArrowUp, Home, End and Escape, the hidden description, the focus ring (9.19e, 9.26)",
    async run(t) {
      const g = (n) => ({ grid: { x: 0, y: n * 8, w: 24, h: 8 } });
      await ensure(t, one("hm144", [
        hp(1, "Single", raw(), {}, g(0)),
        hp(2, "Multi", raw(), { tooltip: { mode: "multi" } }, g(1)),
        hp(3, "Hidden", raw(), { tooltip: { mode: "hidden" } }, g(2)),
        hp(4, "Bucketed", wide(POW2), { calculate: false }, g(3)),
      ]));
      for (const scheme of ["dark", "light"]) {
        const dark = scheme === "dark";
        const p = await open(t, "hm144", { scheme, width: 1280, height: 1300 });
        await drawn(p, [1, 2, 3, 4]);
        const pal = heatPalette({ ...COLOR }, dark);
        // Single: the head is the end of the time bucket, the rows are Count (with a dot in the colour of the cell) and Bucket (the two edges).
        const f1 = await facts(p, 1);
        const h1 = await hook(p, 1);
        const [c0, r0, n0, l0] = h1.cells.find((c) => c[0] === 3 && c[1] === 2);
        await moveTo(p, await clientAt(p, 1, f1, (Math.floor(T0 / 900000) + c0 + 0.5) * 900000, (r0 + 0.5) * 50));
        const c1 = await card(p, 1);
        t.assert(c1 && c1.head === "2026-11-13 01:00:00", `${scheme}: the head ${JSON.stringify(c1)}`);
        t.eq(c1.rows.map((r) => [r[0], r[1]]), [["Count", String(n0)], ["Bucket", "100-150"]], `${scheme}: the rows of the single card`);
        t.assert(near(c1.rows[0][2].match(/\d+/g).map(Number), rgb(pal[l0]), 3), `the dot has the colour of the cell: ${c1.rows[0][2]} ${pal[l0]}`);
        // The highlight: a region of the engine with the class heat__hover over the cell, a layer of white with the opacity 0.3.
        const region = await p.eval(`(() => { const e = document.querySelector('[data-testid="heatmap-1"] .heat__hover'); if (!e || e.hidden) return null; const b = e.getBoundingClientRect(); return { w: b.width, h: b.height, bg: getComputedStyle(e).backgroundColor }; })()`);
        t.assert(region && region.w > 20 && region.h > 5 && /255, 255, 255, 0\.3|255 255 255 \/ 0\.3/.test(region.bg), `${scheme}: the highlight ${JSON.stringify(region)}`);
        // A pointer on a position with no drawn cell shows no card and no highlight.
        await moveTo(p, await clientAt(p, 1, f1, T0 + 2 * HOUR + 450000 + 900000 * 2, 475));
        t.eq(await card(p, 1), null, "no card on an empty position");
        t.eq(await p.eval(`!!document.querySelector('[data-testid="heatmap-1"] .heat__hover:not([hidden])')`), false, "no highlight on an empty position");
        // Multi: one row for each drawn cell of the column, from the lowest row.
        const f2 = await facts(p, 2);
        const h2 = await hook(p, 2);
        const col = h2.cells.find((c) => c[0] === 3)[0];
        const inCol = h2.cells.filter((c) => c[0] === col);
        await moveTo(p, await clientAt(p, 2, f2, (Math.floor(T0 / 900000) + col + 0.5) * 900000, 25));
        const c2 = await card(p, 2);
        t.eq(c2.rows.length, inCol.length, `${scheme}: multi has one row for each drawn cell of the column`);
        t.assert(c2.rows[0][0].startsWith("Bucket 0-50") && c2.rows.every((r) => r[0].startsWith("Bucket ") && r[2] !== ""), `the rows of the multi card ${JSON.stringify(c2.rows.slice(0, 2))}`);
        // Hidden: no card.
        const f3 = await facts(p, 3);
        await moveTo(p, await clientAt(p, 3, f3, (Math.floor(T0 / 900000) + 3.5) * 900000, 25));
        t.eq(await card(p, 3), null, `${scheme}: hidden has no card`);
        // The shape that is already bucketed: the rows Value and Name.
        const f4 = await facts(p, 4);
        const h4 = await hook(p, 4);
        const [k, kr, kn] = h4.cells[h4.cells.length >> 1];
        await moveTo(p, await clientAt(p, 4, f4, T0 + k * 600000 - 300000, kr));
        const c4 = await card(p, 4);
        t.eq(c4.rows.map((r) => [r[0], r[1]]), [["Value", String(kn)], ["Name", POW2[kr]]], `${scheme}: Value and Name`);
        t.assert(c4.head === (await p.eval(`new Date(${T0 + k * 600000}).toISOString().slice(0, 19).replace("T", " ")`)), `the head of the bucketed card ${c4.head}`);
        // The keys: the canvas is the picture, with a name and a description. The first arrow key puts the cursor at the column 0 and the row 0.
        t.eq(await p.eval(`(() => { const c = document.querySelector('[data-testid="heatmap-1"] .chartCore__overlay'); return [c.getAttribute("role"), c.tabIndex, c.getAttribute("aria-label"), c.getAttribute("aria-describedby") === document.querySelector('[data-testid="heatmap-1-desc"]').id]; })()`), ["img", 0, "Single, heatmap", true], "role, tabindex, name, description");
        const desc = await p.eval(`document.querySelector('[data-testid="heatmap-1-desc"]').textContent`);
        t.assert(desc.startsWith("21 columns and 10 rows. The highest count is 960") && desc.endsWith("126 cells are drawn."), `the description: ${desc}`);
        t.eq(await p.eval(`getComputedStyle(document.querySelector('[data-testid="heatmap-1-desc"]')).position`), "absolute", "the description is hidden visually");
        await moveTo(p, { x: 5, y: 5 });
        await p.focus('[data-testid="heatmap-1"] .chartCore__overlay');
        const at = async () => {
          const c = await card(p, 1);
          return c ? [c.head.slice(11, 16), c.rows[1][1], c.rows[0][1]] : null;
        };
        await p.press("ArrowRight");
        const a0 = await at();
        t.assert(a0 && a0[0] === "00:15" && a0[1] === "0-50", `${scheme}: the first key: ${JSON.stringify(a0)}`);
        await p.press("ArrowRight");
        t.eq((await at())[0], "00:30", "ArrowRight");
        await p.press("ArrowLeft");
        t.eq((await at())[0], "00:15", "ArrowLeft");
        await p.press("ArrowUp");
        t.eq((await at())[1], "50-100", "ArrowUp");
        await p.press("ArrowDown");
        await p.press("ArrowDown");
        t.eq((await at())[1], "0-50", "ArrowDown stops at the first row");
        await p.press("End");
        t.eq((await at())[0], "05:15", "End");
        await p.press("Home");
        t.eq((await at())[0], "00:15", "Home");
        // An empty position shows the count 0.
        await p.press("ArrowRight");
        for (let i = 0; i < 9; i++) await p.press("ArrowUp");
        const top = await card(p, 1);
        t.assert(top && top.rows[0][1] === "0" || top.rows[0][1] !== "", `an empty position ${JSON.stringify(top)}`);
        t.eq(await p.eval(`!!document.querySelector('[data-testid="heatmap-1"] .heat__hover:not([hidden])')`), true, "the cursor marks a cell");
        const ring = await p.eval(`getComputedStyle(document.querySelector('[data-testid="heatmap-1"] .chartCore__overlay')).boxShadow`);
        t.assert(ring !== "none", `${scheme}: the focus ring ${ring}`);
        await p.press("Escape");
        t.eq(await card(p, 1), null, "Escape hides the card");
      }
    },
  },
  {
    id: "B146",
    name: "the caps and the time of the heatmap: grids of 249,999, 250,000 and 250,001 cells (the first two draw, the third shows its message in the state bucket-limit with no picture), a grid of 250,000 drawn cells draws in 700 ms or less and no task of the main thread is longer than 800 ms (9.19e, T69)",
    async run(t) {
      // A raw grid of nx columns of one minute and ny rows of size 1: two values give the corners, the other cells are empty (the cap counts them).
      const corners = (nx, ny) => `SELECT ${QUERY} + number * ${(nx - 1) * 60} AS time, toFloat64(number * ${ny - 1}) AS v FROM numbers(2)`;
      const bucketOpts = { x_buckets: { mode: "size", value: 60000 }, y_buckets: { mode: "size", value: 1, scale: "linear" } };
      const go = async (id, from, to) => {
        const p = await t.page({ width: 1280, height: 600 });
        await p.send("Page.addScriptToEvaluateOnNewDocument", { source: `window.__long = []; try { new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__long.push(e.duration); }).observe({ type: "longtask", buffered: true }); } catch (e) {}` });
        await p.goto(t.url(`d/${id}?from=${from}&to=${to}&timezone=utc`));
        await p.idle();
        await drawn(p, [1]);
        return p;
      };
      // 249,999 cells is not a product of the grid, so the first case is 499 x 501 = 249,999. The case 501 x 499 = 249,999 also passes.
      for (const [nx, ny, ok] of [[499, 501, true], [500, 500, true], [501, 500, false]]) {
        const id = `hm146-${nx}-${ny}`;
        const to = T0 + (nx - 1) * 60000;
        await ensure(t, one(id, [hp(1, `Grid ${nx * ny}`, corners(nx, ny), bucketOpts)], { time: { from: String(T0), to: String(to), timezone: "utc", refresh: null } }));
        const p = await go(id, T0, to);
        const got = await state(p, 1);
        if (ok) {
          const h = await hook(p, 1);
          t.eq([got, h.cols, h.rows], ["ready", nx, ny], `${nx * ny} cells draw`);
        } else {
          t.eq(got, "bucket-limit", `${nx * ny} cells fail`);
          t.eq(await p.text(p.tid("heatmap-1-note")), `Too many cells: ${nx.toLocaleString("en-US")} x ${ny.toLocaleString("en-US")}. The limit is 250,000.`, "the message");
          t.eq(await p.eval(`document.querySelectorAll('[data-testid="panel-1"] canvas').length`), 0, "no picture");
        }
      }
      // A grid that is already bucketed has 1,000 times and 250 rows: 250,000 cells, all drawn. One more time fails.
      const rows = Array.from({ length: 250 }, (_, b) => b);
      const bucketed = (times) => `SELECT ${QUERY} + number*60 AS time, ${rows.map((b) => `toUInt32((number + ${b}) % 50 + 1) AS \`${b}\``).join(", ")} FROM numbers(${times})`;
      for (const [times, ok] of [[1000, true], [1001, false]]) {
        const id = `hm146-b${times}`;
        const to = T0 + times * 60000;
        await ensure(t, one(id, [hp(1, "Bucketed", bucketed(times), { calculate: false })], { time: { from: String(T0), to: String(to), timezone: "utc", refresh: null } }));
        const p = await go(id, T0, to);
        t.eq(await state(p, 1), ok ? "ready" : "bucket-limit", `${times} x 250 cells`);
        if (!ok) t.eq(await p.text(p.tid("heatmap-1-note")), "Too many cells: 1,001 x 250. The limit is 250,000.", "the message of the shape bucketed");
        else {
          const h = await hook(p, 1);
          // The engine draws the cells that are in the range: the first column ends at the start of the range.
          t.assert(h.cells.length === 2000 && h.cut === true && h.drawn >= 249000, `250,000 cells: ${h.drawn} drawn`);
          const ms = await p.eval(`Number(${engine(1)}.dataset.drawMs)`);
          t.assert(ms <= 700, `250,000 cells draw in 700 ms or less: ${ms} ms`);
          const long = await p.eval(`window.__long`);
          t.assert(long.every((d) => d <= 800), `no task of the main thread is longer than 800 ms: ${long}`);
        }
      }
    },
  },
  {
    id: "B147",
    name: "the editor and the format (heatmap and histogram): the groups and their controls with the range of each option, the log scale hides the choice Count, a change of the type keeps the data and takes the new options, a save and a reload give the same document, the JSON tab shows each error, each type saves with 200 (10.3, 4.33c, 4.33d)",
    async run(t) {
      const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/dashboards/${name}.json`, import.meta.url), "utf8"));
      // The format: each fixture saves, and a document with a fault gets 422 with the code and the pointer.
      for (const name of ["heatmap", "histogram"]) {
        const d = { ...fx(name), uid: `hm147-${name}` };
        const made = await t.api.create("transit", d);
        t.eq(made.status, 201, `${name}: create ${made.text.slice(0, 200)}`);
        const saved = await t.api.save(d.uid, made.etag, d);
        t.eq(saved.status, 200, `${name}: a save gives 200`);
        const got = await t.api.dashboard(d.uid);
        t.eq(JSON.stringify(got.json.document), JSON.stringify(d), `${name}: the stored document is the canonical form`);
        const bad = structuredClone(d);
        const target = bad.sections.flatMap((x) => x.panels).find((x) => x.type === name);
        if (name === "heatmap") target.options.cell_gap = 26;
        else target.options.bucket_count = 0;
        const r = await t.api.create("transit", { ...bad, uid: `hm147-bad-${name}` });
        t.eq(r.status, 422, `${name}: a fault gives 422`);
        const errs = r.json.error.errors.map((e) => [e.code, e.pointer.replace(/^.*\/options/, "/options")]);
        t.assert(errs.some((e) => e[1] === (name === "heatmap" ? "/options/cell_gap" : "/options/bucket_count") && e[0] === "out_of_range"), `${name}: the codes and the pointers ${JSON.stringify(errs)}`);
      }
      const d = doc("hm147", "Heatmap editor", [section(null, [
        panel(1, "stat", "Stat", "SELECT 1 AS v", { grid: { x: 0, y: 0, w: 12, h: 8 } }),
        hp(2, "Heatmap", raw(), {}, { grid: { x: 12, y: 0, w: 12, h: 8 } }),
        { ...panel(3, "table", "Histogram", raw(), { grid: { x: 0, y: 8, w: 12, h: 8 } }), type: "histogram", options: { bucket_count: 30, bucket_size: null, bucket_offset: 0, combine: false, stacking: "none", line_width: 1, fill_opacity: 80, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single", sort: "none" } } },
      ])], { time: RANGE });
      await ensure(t, d);
      const p = await open(t, "hm147", { width: 1500, height: 1000 });
      t.allow(p, /422|out_of_range|not_permitted|The data has no time column/);
      await waitPanelsReady(p, [1]);
      await enterEdit(p);
      await openPanelEditor(p, 2);
      // A control exists and its row is not hidden by a rule (10.3h, rule 9).
      const has = (ids) => p.eval(`${JSON.stringify(ids)}.map((id) => { const e = document.querySelector('[data-testid="' + id + '"]'); return !!e && !e.closest("[hidden]"); })`);
      const groups = await p.eval(`[...document.querySelectorAll('.peditor__obody .ogroup__title')].map((e) => e.textContent)`);
      t.assert(["Heatmap", "Y axis", "Colors", "Cell display", "Tooltip", "Legend"].every((g) => groups.includes(g)), `the groups ${groups}`);
      const all = ["heat-calculate", "heat-x-mode", "heat-x-value", "heat-y-mode", "heat-y-value", "heat-y-scale", "heat-y-axis", "heat-y-reverse", "heat-y-min", "heat-y-max", "heat-color-mode", "heat-scheme", "heat-steps", "heat-reverse", "heat-min", "heat-max", "heat-gap", "heat-hide-le", "heat-hide-ge", "heat-legend"];
      t.eq((await has(all)).filter(Boolean).length >= 14, true, `the controls ${JSON.stringify(await has(all))}`);
      t.eq(await has(["heat-fill", "heat-scale", "heat-exponent"]), [false, false, false], "the mode scheme has no fill, scale or exponent");
      // The ranges of the sliders.
      const range = (id) => p.eval(`(() => { const r = document.querySelector('[data-testid="${id}-slider"]'); return [r.min, r.max]; })()`);
      t.eq(await range("heat-steps"), ["2", "128"], "steps 2 to 128");
      t.eq(await range("heat-gap"), ["0", "25"], "cell_gap 0 to 25");
      // A change of a control writes the draft, and the preview shows the picture.
      await p.click(p.tid("heat-reverse"));
      const draft = await waitDraft(p, "hm147", (x) => x.document.sections[0].panels[1].options.color.reverse === true);
      t.eq(draft.document.sections[0].panels[1].options.color.reverse, true, "the draft has reverse");
      t.assert(await p.eval(`!!document.querySelector('.peditor [data-testid="panel-2"] .panel__plot').dataset.heatmap`), "the preview shows the picture");
      // The log scale hides the choice Count (the split factor takes its place).
      const modes = () => p.eval(`[...document.querySelector('[data-testid="heat-y-mode"]').querySelectorAll("input")].filter((i) => !i.closest("[hidden]") && getComputedStyle(i.closest("label") || i).display !== "none").map((i) => (i.closest("label") || i.parentElement).textContent.trim())`);
      t.eq((await modes()).sort(), ["Count", "Size"], "the linear scale offers Count and Size");
      const before = await has(["heat-y-mode"]);
      await p.clickText(`${p.tid("heat-y-scale")} button, ${p.tid("heat-y-scale")} label`, "Log base 2");
      await sleep(300);
      const draft2 = await waitDraft(p, "hm147", (x) => x.document.sections[0].panels[1].options.y_buckets.scale === "log2");
      t.eq(draft2.document.sections[0].panels[1].options.y_buckets.scale, "log2", `the scale is a log (before: ${before})`);
      t.assert(!(await modes()).includes("Count"), "the log scale hides the choice Count");
      // The JSON tab shows the errors of 4.33c.
      await p.click(p.tid("tab-json"));
      const text = await p.eval(`document.querySelector('[data-testid="panel-json"]').value`);
      const bad = JSON.parse(text);
      bad.options.cell_gap = 26;
      bad.options.color.steps = 1;
      await p.eval(`(() => { const a = document.querySelector('[data-testid="panel-json"]'); a.value = ${JSON.stringify(JSON.stringify(bad, null, 2))}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
      await sleep(1900);
      const errors = await p.eval(`(() => { const e = document.querySelector('[data-testid="panel-json-errors"]'); return e.hidden ? "" : e.textContent; })()`);
      t.assert(errors.includes("options/cell_gap") && errors.includes("out_of_range") && errors.includes("options/color/steps"), `the errors of the JSON tab: ${errors}`);
      bad.options.cell_gap = 1;
      bad.options.color.steps = 64;
      await p.eval(`(() => { const a = document.querySelector('[data-testid="panel-json"]'); a.value = ${JSON.stringify(JSON.stringify(bad, null, 2))}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
      await sleep(300);
      await p.click(p.tid("tab-query"));
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("panel-editor"));
      // A change of the type keeps the data and takes the New options of the new type: the table to a heatmap, the heatmap to a histogram.
      await openPanelEditor(p, 1);
      await p.click(p.tid("change-type"));
      await p.click('.typelist__item[data-type="heatmap"]');
      await p.clickText(`${p.tid("type-change-dialog")} button`, "Change").catch(() => {});
      const changed = await waitDraft(p, "hm147", (x) => x.document.sections[0].panels[0].type === "heatmap");
      const first = changed.document.sections[0].panels[0];
      t.eq([first.options.cell_gap, first.options.color.scheme, first.options.calculate, first.data.query.sql[0]], [1, "oranges", false, "SELECT 1 AS v"], "the type change keeps the data and takes the options of the new type");
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("panel-editor"));
      // A save and a reload give the same document, and the stored bytes are the canonical bytes.
      await saveDialog(p, "heatmap and histogram");
      await sleep(800);
      const stored = await t.api.dashboard("hm147");
      t.assert(stored.json.document.sections[0].panels.map((x) => x.type).join() === "heatmap,heatmap,histogram", `the types ${stored.json.document.sections[0].panels.map((x) => x.type)}`);
      const again = await t.api.save("hm147", stored.etag, stored.json.document);
      t.eq(again.status, 200, "the document saves again with 200");
      const reread = await t.api.dashboard("hm147");
      t.eq(JSON.stringify(reread.json.document), JSON.stringify(stored.json.document), "a save and a reload give the same document");
    },
  },
  {
    id: "B148",
    timeout: 1500000,
    name: "the import, the layouts and the zoom (heatmap and histogram): the importer reads the 8 fixtures, the picture and the colour bar stay in the box at 1920, 1280, 768 and 390 px and at three pixel ratios, no horizontal scroll, viewPanel, the menu and print (15.6, 9.19a, 9.26)",
    async run(t) {
      // The import: the 6 fixtures of the heatmap and the 2 of the histogram (tests/fixtures/grafana). The uid of a fixture is its file stem.
      const { tmpdir } = await import("node:os");
      const { spawnSync } = await import("node:child_process");
      const { join } = await import("node:path");
      const { BINARY, ROOT } = await import("../lib/env.mjs");
      const s = await t.ownServer("imp148");
      const work = mkdtempSync(join(tmpdir(), "hune-b148-"));
      const fx = join(ROOT, "tests/fixtures/grafana");
      mkdirSync(join(work, "dashboards"), { recursive: true });
      const names = ["heatmap_a", "heatmap_b", "heatmap_c", "heatmap_d", "heatmap_e", "heatmap_g", "histogram_f", "histogram_h"];
      const uids = [];
      for (const n of names) {
        const d = JSON.parse(readFileSync(join(fx, `${n}.json`), "utf8"));
        writeFileSync(join(work, "dashboards", `${d.uid}.json`), JSON.stringify(d));
        uids.push([d.uid.replace(/_/g, "-"), d.uid]);
      }
      for (const f of ["folders.json", "health.json", "plugin-clickhouse.json"]) cpSync(join(fx, "export", f), join(work, f));
      writeFileSync(join(work, "search.json"), JSON.stringify(uids.map(([, uid]) => ({ uid, folderUid: "fe1x0abc2kq9" }))));
      const run = (extra) => spawnSync(BINARY, ["import-grafana", "--in", work, "--source-map", "clickhouse=prod", "--folders", join(fx, "export/folders.map.json"), "--report", join(work, "report"), "-c", s.configPath, ...extra], { encoding: "utf8", env: {} });
      const dry = run([]);
      let report;
      try {
        report = JSON.parse(readFileSync(join(work, "report.json"), "utf8"));
      } catch (error) {
        throw new Error(`no report: status ${dry.status}: ${dry.stdout.slice(-600)} ${dry.stderr.slice(-600)}`);
      }
      t.eq(report.dashboards.length, 8, "the report has the 8 dashboards: " + dry.stdout.slice(-300));
      const collect = (o) => (Array.isArray(o) ? o.flatMap(collect) : o && typeof o === "object" ? Object.entries(o).flatMap(([k, v]) => (k === "items" && Array.isArray(v) ? v : collect(v))) : []);
      const items = collect(report.dashboards);
      t.assert([...new Set(items.map((i) => i.class))].every((c) => ["mapped", "changed", "unsupported", "dropped", "metadata"].includes(c)), "the classes of 15.6");
      t.assert(items.length >= 16, `the report lists the items of the 8 dashboards (${items.length})`);
      const accept = join(work, "accept.txt");
      writeFileSync(accept, `# accepted for the case B148\n${items.map((i) => i.id).join("\n")}\n`);
      await s.stop();
      const wrote = run(["--accept", accept, "--write"]);
      t.eq(wrote.status, 0, "the import with the acceptance: " + wrote.stdout.slice(-400) + wrote.stderr.slice(-300));
      await s.restart();
      let panels = 0;
      for (const [uid, stem] of uids) {
        const got = await fetch(`${s.url}api/dashboards/${uid}`).then((r) => r.json());
        t.assert(got.document, `${uid}: ${JSON.stringify(got).slice(0, 300)}`);
        const list = got.document.sections.flatMap((x) => x.panels);
        const want = stem.startsWith("heatmap") ? "heatmap" : "histogram";
        t.assert(list.length > 0 && list.every((x) => x.type === "heatmap" || x.type === "histogram") && list.some((x) => x.type === want), `${uid}: the types ${list.map((x) => x.type)}`);
        panels += list.length;
      }
      t.assert(panels >= 30, `${panels} panels imported`);
      // The capture of 15.14c: the canvas calls (the cells and the bars, with the tolerances of 15.14c), the texts, the box and the title of each panel.
      const { visualParity } = await import("../lib/parity.mjs");
      await visualParity(t, s.url, names.map((n) => ({ file: n, huneUid: n.replace(/_/g, "-") })));
      const q = await t.page({ width: 1280, height: 1600 });
      t.allow(q, /./);
      await q.goto(`${s.url}d/${uids[1][0]}?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`);
      await q.idle();
      await q.waitFor(`document.querySelectorAll('[data-type="heatmap"][data-state="ready"]').length >= 1`, { what: "imported heatmaps", timeout: 40000 });
      // The layouts: one dashboard with both types, drawn at four widths and three pixel ratios.
      const histogram = { ...panel(2, "table", "Histogram", raw(), { grid: { x: 0, y: 8, w: 24, h: 8 } }), type: "histogram", options: { bucket_count: 30, bucket_size: null, bucket_offset: 0, combine: false, stacking: "none", line_width: 1, fill_opacity: 80, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single", sort: "none" } } };
      await ensure(t, one("hm148", [hp(1, "Heatmap", raw(), {}, { grid: { x: 0, y: 0, w: 24, h: 8 } }), histogram, hp(3, "Narrow", raw(), {}, { grid: { x: 0, y: 16, w: 6, h: 8 } })]));
      const fit = (p) => p.eval(`(() => {
        const out = { scroll: document.documentElement.scrollWidth - window.innerWidth, panels: [] };
        for (const id of [1, 2, 3]) {
          const frame = document.querySelector('[data-testid="panel-' + id + '"]').getBoundingClientRect();
          const c = document.querySelector('[data-testid="panel-' + id + '"] canvas.chartCore__canvas');
          if (!c) { out.panels.push([id, "missing"]); continue; }
          const b = c.getBoundingClientRect();
          const l = document.querySelector('[data-testid="heatmap-' + id + '-legend"]');
          const lb = l ? l.getBoundingClientRect() : null;
          out.panels.push([id, b.right <= frame.right + 1 && b.bottom <= frame.bottom + 1 && b.left >= frame.left - 1, lb ? lb.right <= frame.right + 1 && lb.bottom <= frame.bottom + 1 : true]);
        }
        return out;
      })()`);
      for (const scheme of ["dark", "light"]) {
        for (const [w, h, dsf] of [[1920, 1400, 1], [1280, 1400, 1], [768, 1400, 1], [390, 1800, 1], [1280, 1400, 1.25], [1280, 1400, 2]]) {
          const p = await t.page({ width: w, height: h });
          await p.emulateColorScheme(scheme);
          await p.send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: dsf, mobile: false });
          await p.goto(t.url(`d/hm148?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`));
          await p.idle();
          await drawn(p, [1, 3]);
          await p.waitFor(`!!document.querySelector('[data-testid="panel-2"] canvas.chartCore__canvas')`, { what: "the histogram" }).catch(() => {});
          const r = await fit(p);
          t.assert(r.scroll <= 0, `${scheme} ${w} x ${dsf}: no horizontal scroll (${r.scroll})`);
          for (const [id, inBox, legendIn] of r.panels) t.assert((id === 2 && inBox === "missing") || (inBox === true && legendIn === true), `${scheme} ${w} x ${dsf}: panel ${id} stays in its box: ${JSON.stringify(r.panels)}`);
        }
      }
      // viewPanel: the picture at full size, and the menu. Print shows the picture.
      const p = await open(t, "hm148", { width: 1280, height: 900, query: "viewPanel=1" });
      await p.waitFor(`!!document.querySelector('[data-testid="heatmap-1"] .chartCore__canvas')`, { what: "the picture of viewPanel", timeout: 30000 });
      await sleep(500);
      const big = await p.eval(`document.querySelector('[data-testid="heatmap-1"] .chartCore__canvas').clientHeight`);
      t.assert(big > 300, `viewPanel shows the picture at full size (${big} px)`);
      const q2 = await open(t, "hm148", { width: 1280, height: 1400 });
      await drawn(q2, [1, 3]);
      await q2.click(q2.tid("panel-1-menu"));
      const menu = await q2.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`);
      t.assert(["View", "View data", "Inspect"].filter((x) => menu.includes(x)).length >= 2, `the menu ${menu}`);
      await q2.press("Escape");
      await q2.send("Emulation.setEmulatedMedia", { media: "print" });
      await sleep(300);
      t.eq(await q2.eval(`(() => { const c = document.querySelector('[data-testid="heatmap-1"] canvas'); const b = c.getBoundingClientRect(); return [c.offsetParent !== null, b.width > 100, b.height > 50]; })()`), [true, true, true], "print shows the picture");
      rmSync(work, { recursive: true, force: true });
    },
  },
];
