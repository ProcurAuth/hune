// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B145 and B145b (4.33d, 9.19f, 16.10): the histogram.
// B145: the look. The numbers are those of the measure of Grafana 12.4.2 (19.20g): the buckets, the bars, the labels of the x axis and the colours.
// B145b: the caps and the time of the histogram (the part of B146 that belongs to this type).
import { doc, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const T0 = Date.UTC(2026, 10, 13);
const HOUR = 3600000;
const RANGE = { from: String(T0), to: String(T0 + 5 * HOUR), timezone: "utc", refresh: null };
const TIME = "toDateTime('2026-11-13 00:00:00','UTC') + number AS time";
// The exponential values of the measure: 40 * -ln(1 - u) with u from a hash of the row number (the same rows as 19.20g).
const EXP = (seed, scale, shift = 0) => `round(${scale}*-log(1-(cityHash64(number+${seed}) % 1000000 + 0.5)/1000000)${shift ? `+${shift}` : ""},3)`;
const A = (n) => `SELECT ${TIME}, ${EXP(0, 40)} AS a FROM numbers(${n})`;
const AB = (n) => `SELECT ${TIME}, ${EXP(0, 40)} AS a, ${EXP(7, 25, 10)} AS b FROM numbers(${n})`;

const legend = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const OPTIONS = { bucket_count: 30, bucket_size: null, bucket_offset: 0, combine: false, stacking: "none", line_width: 1, fill_opacity: 80, legend: legend(), tooltip: { mode: "single", sort: "none" } };
/** A histogram panel. `sql` is a SELECT. */
const hp = (id, title, sql, o = {}) => ({
  id, type: "histogram", title, description: "", transparent: false, grid: { x: 0, y: (id - 1) * 8, w: 24, h: 8 },
  data: { query: { source: "prod", format: "table", sql: [sql], min_interval: null, max_data_points: null } },
  transforms: [], fields: o.fields || { defaults: {}, overrides: [] }, options: { ...OPTIONS, ...(o.options || {}) }, links: [],
});
const one = (uid, panels) => doc(uid, `Histogram ${uid}`, [section(null, panels)], { time: RANGE });
const ensure = async (t, d) => {
  if ((await t.api.dashboard(d.uid)).status === 200) return;
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create ${d.uid}: ${r.status} ${r.text.slice(0, 400)}`);
};
const open = async (t, uid, { scheme = "dark", width = 1280, height = 3300 } = {}) => {
  const p = await t.page({ width, height });
  await p.emulateColorScheme(scheme);
  await p.recordCanvasText();
  // The rectangles of each canvas, with the fill, the stroke and the line width of their path (the bars of the engine).
  await p.send("Page.addScriptToEvaluateOnNewDocument", {
    source: `(() => {
      const rec = new WeakMap();
      const proto = CanvasRenderingContext2D.prototype;
      const [begin, rect, fill, stroke, clear] = [proto.beginPath, proto.rect, proto.fill, proto.stroke, proto.clearRect];
      let cur = [];
      const list = (c) => rec.get(c) || rec.set(c, []).get(c);
      proto.beginPath = function () { cur = []; return begin.call(this); };
      proto.rect = function (x, y, w, h) { cur.push([x, y, w, h]); return rect.call(this, x, y, w, h); };
      proto.fill = function (...a) { if (cur.length) list(this.canvas).push({ fs: String(this.fillStyle), rects: cur.slice(), line: 0 }); return fill.apply(this, a); };
      proto.stroke = function (...a) { const l = list(this.canvas); if (l.length && cur.length) { const g = l[l.length - 1]; if (g.rects.length === cur.length) { g.line = this.lineWidth; g.ss = String(this.strokeStyle); } } return stroke.apply(this, a); };
      proto.clearRect = function (x, y, w, h) { if (x === 0 && y === 0) rec.set(this.canvas, []); return clear.call(this, x, y, w, h); };
      window.__rects = (c) => rec.get(c) || [];
    })();`,
  });
  await p.goto(t.url(`d/${uid}?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`));
  await p.idle();
  return p;
};
const plot = (id) => `document.querySelector('[data-testid="panel-${id}"] .panel__plot')`;
const drawn = async (p, ids) => {
  await p.waitFor(`${JSON.stringify(ids)}.every((id) => { const e = document.querySelector('[data-testid="panel-' + id + '"]'); const pl = e && e.querySelector('.panel__plot'); return e && (e.dataset.state === "error" || e.dataset.state === "bucket-limit" || e.dataset.state === "empty" || (pl && pl.dataset.histogram)); })`, { what: "the histograms" });
  await p.idle();
};
const hook = (p, id) => p.eval(`JSON.parse(${plot(id)}.dataset.histogram)`);
const canvasExpr = (id) => `[...document.querySelectorAll('[data-testid="panel-${id}"] canvas')].find((c) => window.__rects(c).length)`;
/** The groups of rectangles (one for each series) and the texts of the x axis ([text, x]) and of the y axis ([text, y]) of a panel. */
const picture = async (p, id) => p.eval(`(() => { const c = ${canvasExpr(id)}; const texts = window.__canvasTexts(c); return { groups: window.__rects(c).filter((g) => g.rects.length > 1 || g.rects[0][2] > 0), x: texts.filter((x) => x.align === "center").map((x) => [x.text, x.x]), y: texts.filter((x) => x.align === "right").map((x) => [x.text, x.y]) }; })()`);
const rgb = (css) => (css.startsWith("#") ? [...hex(css), 1] : (css.match(/[\d.]+/g) || []).map(Number));
const near = (a, b, tol = 1) => a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) <= tol);
const hex = (css) => [1, 3, 5].map((i) => parseInt(css.slice(i, i + 2), 16));
/** The colour of the series `k` of the palette of the page (the tokens --qchart-1 and the next ones), as [r, g, b]. */
const palette = async (p, k) => hex(await p.eval(`getComputedStyle(document.documentElement).getPropertyValue("--qchart-${k + 1}").trim()`));

/**
 * The checks of one panel against the numbers of the measure: `want` has `size`, `nb` (the buckets), `bars` (the bars that the measure drew), `xs` (the
 * labels of the x axis, when the measure has them) and `alpha`. A bar is its slot less 1 px (the rule of the engine): the slot comes from the labels.
 */
async function lookPanel(t, p, id, want, scheme) {
  const h = await hook(p, id);
  const pic = await picture(p, id);
  const name = `${scheme}: panel ${id}`;
  t.eq([h.size, h.buckets], [want.size, want.nb], `${name}: the size and the number of buckets`);
  t.eq(h.bars.length, want.bars, `${name}: the bars of the hook`);
  t.eq(pic.groups.reduce((n, g) => n + g.rects.length, 0), want.bars, `${name}: the rectangles that the canvas drew`);
  if (want.xs) t.eq(pic.x.map((x) => x[0]), want.xs, `${name}: the labels of the x axis`);
  // The slot is the width of a bar and 1 px (the rule of the engine). The left edge of the plot comes from the first bar. The labels sit on the edges
  // of the buckets (the last label can move left to stay in the canvas).
  const first = pic.groups[0] && pic.groups[0].rects.length ? [...pic.groups[0].rects].sort((x, y) => x[0] - y[0])[0] : null;
  let geo = null;
  if (first) {
    const slot = first[2] + 1;
    const bySeries = new Map();
    for (const [k, j, c] of h.bars) (bySeries.get(k) || bySeries.set(k, []).get(k)).push([j, c]);
    const j0 = Math.min(...[...bySeries.values()].map((l) => l[0][0]));
    const left = Math.min(...pic.groups.map((g) => Math.min(...g.rects.map((r) => r[0])))) - 0.5 - j0 * slot;
    geo = { left, slot };
    pic.x.slice(0, -1).forEach(([text, x]) => t.assert(Math.abs(x - (left + ((Number(text) - h.lo) / h.size) * slot)) <= 1.5, `${name}: the label ${text} is at ${x.toFixed(1)}, expected ${(left + ((Number(text) - h.lo) / h.size) * slot).toFixed(1)}`));
    // One group for each series that has a bar. A bar of the bucket `j` spans `left + j * slot + 0.5` and is `slot - 1` wide.
    const groups = pic.groups.filter((g) => g.rects.length > 0);
    t.assert(groups.length === bySeries.size, `${name}: one group for each series (${groups.length}, ${bySeries.size})`);
    let at = 0;
    for (const [k, list] of [...bySeries].sort((a, b) => a[0] - b[0])) {
      const g = groups[at++];
      const rects = [...g.rects].sort((a, b) => a[0] - b[0]);
      list.forEach(([j], i) => {
        const [x, , w] = rects[i];
        t.assert(Math.abs(x - (left + j * slot + 0.5)) <= 1 && Math.abs(w - (slot - 1)) <= 1, `${name}: series ${k} bucket ${j}: x ${x.toFixed(2)} w ${w.toFixed(2)}, expected ${(left + j * slot + 0.5).toFixed(2)} ${(slot - 1).toFixed(2)}`);
      });
      // The heights are in the ratio of the counts (a stack has no such ratio).
      if (want.ratio !== false) {
        const top = Math.max(...list.map((x) => x[1]));
        const hmax = Math.max(...rects.map((r) => r[3]));
        list.forEach(([, c], i) => t.assert(Math.abs(rects[i][3] - (hmax * c) / top) <= 1.2, `${name}: series ${k} height ${rects[i][3].toFixed(2)} of count ${c}, expected ${((hmax * c) / top).toFixed(2)}`));
      }
      if (want.fill) {
        const [r, g2, b, a] = rgb(g.fs);
        t.assert(near([r, g2, b], await palette(p, k), 1) && Math.abs(a - want.fill) < 0.01, `${name}: the fill ${g.fs} of series ${k}`);
      }
      if (want.line !== undefined) t.assert(Math.abs(g.line - want.line) < 0.01, `${name}: the line width ${g.line}, expected ${want.line}`);
    }
  }
  return { h, pic, geo };
}

// The measured cases of 19.20g. `xs` are the labels of the x axis (every edge, or every second edge). The bars are the non-empty buckets.
const X20 = ["0", "20", "40", "60", "80", "100", "120", "140", "160", "180", "200", "220", "240", "260", "280", "300", "320", "340", "360"];
const LOOK = [
  [1, "defaults", A(5000), {}, { size: 20, nb: 18, bars: 17, xs: X20, fill: 0.8, line: 1 }],
  [2, "two fields", AB(5000), {}, { size: 20, nb: 18, bars: 29, xs: X20, fill: 0.8 }],
  [3, "combine", AB(5000), { combine: true }, { size: 20, nb: 18, bars: 17, xs: X20, fill: 0.8 }],
  [4, "bucketCount 10", A(5000), { bucket_count: 10 }, { size: 40, nb: 9, bars: 9, xs: ["0", "40", "80", "120", "160", "200", "240", "280", "320", "360"], fill: 0.8 }],
  [5, "bucketSize 25", A(5000), { bucket_size: 25 }, { size: 25, nb: 14, bars: 13, xs: ["0", "25", "50", "75", "100", "125", "150", "175", "200", "225", "250", "275", "300", "325", "350"], fill: 0.8 }],
  [6, "bucketSize 25 offset 10", A(5000), { bucket_size: 25, bucket_offset: 10 }, { size: 25, nb: 15, bars: 15, fill: 0.8 }],
  [7, "stacking normal, fill 30, line 3", AB(3000), { stacking: "normal", fill_opacity: 30, line_width: 3 }, { size: 20, nb: 18, bars: 27, xs: X20, fill: 0.3, line: 3, ratio: false }],
  [8, "line 0, fill 100", A(3000), { line_width: 0, fill_opacity: 100 }, { size: 20, nb: 18, bars: 16, fill: 1, line: 0 }],
  // The counts 54 to 92 of the negative values (19.20g) and one value of a second field: the y axis reads the lowest count of the series that show.
  [9, "a click hides a series", `SELECT ${TIME}, round(60*((cityHash64(number+1) % 1000000 + 0.5)/1000000-0.5),3) AS a, if(number = 0, 0.5, NULL) AS b FROM numbers(2000)`, {}, { size: 2, nb: 30, bars: 31, fill: 0.8 }],
];

// The data cases: the measured ones of 19.20g (negative values, two clusters, equal values, one value, null values, a bucket count of 1) and the choices of
// Hune (the buckets that the query made, hostile names, `hide_from`, the legend at the right).
const NAMES = "`<img src=x onerror=alert(1)>`";
const U = (seed, expr) => `round(${expr}*(cityHash64(number+${seed}) % 1000000 + 0.5)/1000000,2)`;
const DATA = [
  [1, "negative values", `SELECT ${TIME}, round(60*((cityHash64(number+1) % 1000000 + 0.5)/1000000-0.5),3) AS v FROM numbers(2000)`, {}, { size: 2, nb: 30, bars: 30, fill: 0.8 }],
  [2, "two clusters (gap)", `SELECT ${TIME}, if(number%2=0, ${U(1, 10)}, 90+${U(2, 10)}) AS v FROM numbers(2000)`, {}, { size: 4, nb: 25, bars: 6, fill: 0.8 }],
  [3, "all equal", `SELECT ${TIME}, toFloat64(5) AS v FROM numbers(100)`, {}, { size: 1, nb: 1, bars: 1, fill: 0.8 }],
  [4, "single value", `SELECT ${TIME}, toFloat64(5) AS v FROM numbers(1)`, {}, { size: 1, nb: 1, bars: 1, fill: 0.8 }],
  [5, "null values", `SELECT ${TIME}, if(number%5=0, NULL, ${U(1, 100)}) AS v FROM numbers(2000)`, {}, { size: 4, nb: 25, bars: 25, fill: 0.8 }],
  [6, "bucket count 1", A(3000), { bucket_count: 1 }, { size: 400, nb: 1, bars: 1, fill: 0.8 }],
  [7, "legend right, tooltip multi", AB(3000), { legend: legend({ placement: "right" }), tooltip: { mode: "multi", sort: "none" } }, { size: 20, nb: 18, bars: 27, fill: 0.8 }],
  [8, "stacking percent", AB(3000), { stacking: "percent" }, { size: 20, nb: 18, bars: 27, ratio: false }],
  [9, "buckets that the query made", "SELECT number*5 AS xMin, number*5+5 AS xMax, (number*7)%11 AS n FROM numbers(12)", {}, { size: 5, nb: 12, bars: 10, fill: 0.8 }],
  [10, "hostile names", `SELECT ${TIME}, number%5 AS ${NAMES}, number%7 AS \`<b>x</b>\` FROM numbers(200)`, { legend: legend({ mode: "table", values: ["max"] }), tooltip: { mode: "multi", sort: "none" } }, { size: 1, nb: 7, bars: 12 }],
  [11, "hide from the legend", AB(3000), {}, { size: 20, nb: 18, bars: 27 }],
  [12, "hide from the picture", AB(3000), {}, { size: 20, nb: 18, bars: 16 }],
];
const hide = (name, key) => ({ defaults: {}, overrides: [{ match: { by: "names", mode: "include", names: [name] }, set: { hide_from: { legend: false, tooltip: false, viz: false, [key]: true } } }] });

/** The text of the card after a hover on bucket `j` of a panel: the title and the rows. */
async function card(p, id, geo, j, fy = 0.5) {
  const box = await p.eval(`(() => { const o = document.querySelector('[data-testid="panel-${id}"] .chartCore__overlay'); const r = o.getBoundingClientRect(); return [r.left, r.width]; })()`);
  await p.hover(`[data-testid="panel-${id}"] .chartCore__overlay`, (geo.left + (j + 0.5) * geo.slot) / box[1], fy);
  await p.waitFor(`(() => { const e = document.querySelector('[data-testid="panel-${id}"] .chartCore__tooltip'); return e && !e.hidden && e.textContent; })()`, { what: "the card" });
  return p.eval(`(() => { const e = document.querySelector('[data-testid="panel-${id}"] .chartCore__tooltip'); return { title: e.querySelector("strong").textContent, rows: [...e.querySelectorAll(".chartCore__tipRow")].map((r) => [r.querySelector("em").textContent, r.querySelector("b").textContent]), images: e.querySelectorAll("img, script, svg").length }; })()`);
}

export const cases = [
  {
    id: "B145",
    name: "the look of a histogram: the bucket size and count, the offset, combine, two fields that overlap, stacking, the line and the fill, the labels of the x axis, the bars (x and width, 1 px) and the colours, in both themes (9.19f, 19.20g)",
    async run(t) {
      await ensure(t, one("hg145a", LOOK.map(([id, title, sql, options]) => hp(id, title, sql, { options }))));
      for (const scheme of ["dark", "light"]) {
        const p = await open(t, "hg145a", { scheme, height: 3300 });
        await drawn(p, LOOK.map((x) => x[0]));
        for (const [id, , , , want] of LOOK) await lookPanel(t, p, id, want, scheme);
        // Two fields overlap at the same place: the two groups have the same left edge for a bucket that both have.
        const two = await picture(p, 2);
        t.assert(two.groups.length === 2 && two.groups[0].rects.some((a) => two.groups[1].rects.some((b) => Math.abs(a[0] - b[0]) < 0.01 && Math.abs(a[2] - b[2]) < 0.01)), `${scheme}: the bars of two fields overlap`);
        // A stack: the second bar of a bucket starts where the first one ends. The line of each bar stays inside the bar (E26, 9.19f section 3): the
        // box loses half the line width at each end, so two boxes of a stack are one line width (3 px) apart, and their lines touch.
        // Both edges come from one value of the y scale (the top of the lower bar) in one draw, so the gap is 0 up to the float error for any scale.
        // A box of 2 px or less has the floor of 1 device pixel, so it is not part of the check.
        const st = await picture(p, 7);
        const [g0, g1] = st.groups;
        const both = g0.rects.map((a) => [a, g1.rects.find((b) => Math.abs(a[0] - b[0]) < 0.01)]).filter((x) => x[1] && x[0][3] > 2 && x[1][3] > 2);
        const gap = Math.max(...both.map(([a, b]) => Math.abs(b[1] + b[3] + g0.line - a[1])));
        t.assert(both.length > 3 && gap < 0.05, `${scheme}: the lines of stacked bars touch (line ${g0.line}, ${both.length} buckets, largest gap ${gap.toFixed(3)} px)`);
        // The y axis (9.19f section 3): from 0 to the end of the range rule for the lowest count above 0 and the highest count of the series that show.
        // With both series the counts are 1 to 92 (end 102, last tick 100). A click on the item a shows only a (Grafana: a click isolates), so the series b
        // with its count 1 is hidden and the counts are 54 to 92 (end 96, last tick 80).
        const ys = async () => (await picture(p, 9)).y.map((x) => x[0]).sort((a, b) => Number(a) - Number(b));
        t.eq((await ys()).at(-1), "100", `${scheme}: the last y tick with both series`);
        await p.click('[data-testid="panel-9"] .chartCore__legendItem[data-index="0"]');
        await p.waitFor(`(() => { const c = ${canvasExpr(9)}; const y = window.__canvasTexts(c).filter((x) => x.align === "right"); return y.some((x) => x.text === "80") && !y.some((x) => x.text === "100"); })()`, { what: "the y axis of the series that show", timeout: 5000 }).catch(() => {});
        t.eq(await ys(), ["0", "20", "40", "60", "80"], `${scheme}: the y ticks after a click hides the series b`);
      }
    },
  },
  {
    id: "B145a",
    name: "the data of a histogram: negative values, two clusters, equal values, one value, null values, a bucket count of 1, the legend at the right, a stack in percent, buckets that the query made, hostile names, hide_from, and the card (9.19f, 19.20g)",
    async run(t) {
      await ensure(t, one("hg145b", DATA.map(([id, title, sql, options]) => hp(id, title, sql, {
        options,
        fields: id === 11 ? hide("b", "legend") : id === 12 ? hide("b", "viz") : undefined,
      }))));
      for (const scheme of ["dark", "light"]) {
        const p = await open(t, "hg145b", { scheme, height: 3600 });
        await drawn(p, DATA.map((x) => x[0]));
        const geo = {};
        for (const [id, , , , want] of DATA) {
          const r = await lookPanel(t, p, id, want, scheme);
          geo[id] = r.geo;
        }
        // One bucket fills the plot: the bar is the plot less 1 px (Grafana: the whole plot).
        for (const id of [3, 4, 6]) {
          const pic = await picture(p, id);
          const W = await p.eval(`${plot(id)}.getBoundingClientRect().width`);
          t.assert(pic.groups[0].rects[0][2] > W - 100 && pic.groups[0].rects[0][2] < W, `${scheme}: panel ${id}: one bucket fills the plot, got ${pic.groups[0].rects[0][2]} of ${W}`);
        }
        // The stack in percent: the y axis shows shares of 1, as in Grafana, and the bars of a bucket fill the plot from the top to the bottom.
        const pc = await picture(p, 8);
        t.assert(pc.y.length >= 3 && pc.y[0][0] === "0" && ["1", "1.1"].includes(pc.y.at(-1)[0]), `${scheme}: the labels of a stack in percent: ${pc.y.map((y) => y[0])}`);
        // The buckets that the query made: five wide, 12 buckets, and the first bucket has no bar (count 0).
        const q = await hook(p, 9);
        t.eq([q.shape, q.lo, q.size, q.buckets], ["bucketed", 0, 5, 12], `${scheme}: the shape of the query`);
        t.eq(q.bars.map((b) => b[1]), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10], `${scheme}: the buckets with a bar`);
        // Hostile names: the legend and the card show the text; no element comes from it.
        const legendText = await p.eval(`document.querySelector('[data-testid="panel-10"] .chartCore__legend').textContent`);
        t.assert(legendText.includes("<img src=x onerror=alert(1)>") && legendText.includes("<b>x</b>"), `${scheme}: the legend writes the names as text`);
        t.eq(await p.count('[data-testid="panel-10"] img, [data-testid="panel-10"] b > b'), 0, `${scheme}: no element comes from a name`);
        const hostile = await card(p, 10, geo[10], 2);
        t.assert(hostile.images === 0 && hostile.rows.some((r) => r[0] === "<img src=x onerror=alert(1)>"), `${scheme}: the card writes the names as text`);
        // The card of a bucket: the title is the edges, the row is the series name and the count.
        const one1 = await card(p, 7, geo[7], 2);
        const bars7 = (await hook(p, 7)).bars;
        const count = (k) => bars7.find((b) => b[0] === k && b[1] === 2)[2];
        t.eq(one1.title, "40 - 60", `${scheme}: the title of the card`);
        t.eq(one1.rows.map((r) => [r[0], Number(r[1])]), [["a", count(0)], ["b", count(1)]], `${scheme}: the rows of the card (multi)`);
        // hide_from: the legend of panel 11 has one entry and the picture of panel 12 has one series. The card has the series that the field allows.
        t.eq(await p.count('[data-testid="panel-11"] .chartCore__legend [data-series], [data-testid="panel-11"] .chartCore__legendItem'), 1, `${scheme}: one entry in the legend of panel 11`);
        const nine = await card(p, 9, geo[9], 3);
        t.eq([nine.title, nine.rows.length], ["15 - 20", 1], `${scheme}: the card of buckets that the query made`);
        // The legend at the right of panel 7 stands beside the plot.
        const side = await p.eval(`(() => { const l = document.querySelector('[data-testid="panel-7"] .chartCore__legend'); const c = document.querySelector('[data-testid="panel-7"] .chartCore__plot'); return l.offsetLeft >= c.offsetLeft + c.offsetWidth - 1; })()`);
        t.assert(side, `${scheme}: the legend of panel 7 is beside the plot`);
      }
    },
  },
  {
    id: "B145b",
    name: "the caps and the time of a histogram: 3,999, 4,000 and 4,001 buckets, 500,000 and 600,000 values, the message in the state bucket-limit with no picture, and no long task (9.19f, T69)",
    timeout: 240000,
    async run(t) {
      const lim = { bucket_size: 1 };
      const N = (n) => `SELECT ${TIME}, number AS v FROM numbers(${n})`;
      // The server sends 100,000 rows at most for a query (max_result_rows): 5 fields of 100,000 rows are 500,000 values, and 6 fields are 600,000.
      const M = (k) => `SELECT ${TIME}, ${Array.from({ length: k }, (_, i) => `(number+${i * 137})%1000 AS v${i}`).join(", ")} FROM numbers(100000)`;
      const one2 = (uid, rows) => one(uid, rows.map(([id, title, sql, options], i) => ({ ...hp(id, title, sql, { options }), grid: { x: 0, y: i * 8, w: 24, h: 8 } })));
      await ensure(t, one2("hg145c", [[1, "3,999 buckets", N(3999), lim], [2, "4,000 buckets", N(4000), lim], [3, "4,001 buckets", N(4001), lim]]));
      await ensure(t, one2("hg145d", [[1, "500,000 values", M(5), {}], [2, "600,000 values", M(6), {}]]));
      const p = await open(t, "hg145c", { height: 1000 });
      await drawn(p, [1, 2, 3]);
      const states = await p.eval(`[1, 2, 3].map((id) => document.querySelector('[data-testid="panel-' + id + '"]').dataset.state)`);
      t.eq(states, ["ready", "ready", "bucket-limit"], "the states of 3,999, 4,000 and 4,001 buckets");
      t.eq([(await hook(p, 1)).buckets, (await hook(p, 2)).buckets], [3999, 4000], "the buckets that draw");
      t.eq(await p.text('[data-testid="histogram-3-note"]'), "Too many buckets: 4,001. The limit is 4,000.", "the message of 4,001 buckets");
      t.eq(await p.count('[data-testid="panel-3"] canvas'), 0, "no picture above the cap");
      t.eq(await p.exists('[data-testid="panel-3"] [data-testid$="retry"], [data-testid="panel-3-retry"]'), false, "no Retry for a message of a cap");
      // 4,000 buckets: a bar is 1 px wide at least, and the engine keeps every bar (E24).
      const wide = await picture(p, 2);
      t.eq(wide.groups[0].rects.length, 4000, "4,000 bars, none left out");
      const q = await open(t, "hg145d", { height: 700 });
      await q.send("Page.addScriptToEvaluateOnNewDocument", { source: "window.__long = 0; new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__long = Math.max(window.__long, e.duration); }).observe({ type: 'longtask', buffered: true });" });
      await q.goto(t.url(`d/hg145d?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`));
      await q.idle();
      await drawn(q, [1, 2]);
      const s2 = await q.eval(`[1, 2].map((id) => document.querySelector('[data-testid="panel-' + id + '"]').dataset.state)`);
      t.eq(s2, ["ready", "bucket-limit"], "the states of 500,000 and 600,000 values");
      t.eq(await q.text('[data-testid="histogram-2-note"]'), "Too many values to bucket: 600,000. The limit is 500,000. Bucket the values in the SQL.", "the message of 600,000 values");
      const h = await hook(q, 1);
      t.eq([h.size, h.buckets, h.bars.reduce((n, b) => n + b[2], 0)], [40, 25, 500000], "500,000 values: the size, the buckets and the counts that add up to the values");
      const long = await q.eval("window.__long");
      t.assert(long <= 800, `no task of the main thread is longer than 800 ms: ${long} ms`);
    },
  },
];
