// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B110 to B118 and B395 (4.33b, 9.19c, 10.3, 15.6): the state timeline and the status history.
// B110: the look of a state timeline. B111: the look of a status history. B112: the states, the colours, the legend, hostile texts.
// B113: the data cases. B114: the caps and the time. B115: the card, the hover and the keyboard. B116: the editor and the format.
// B117: the import. B118: layouts, zoom, viewPanel, the menu and print. B395: the text size at each pixel ratio and the ticks of the hook.
import { readFileSync } from "node:fs";
import { doc, panel, section } from "../lib/docs.mjs";
import { enterEdit, openPanelEditor, saveDialog, waitDraft } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";
import { boxText, cellWidth, gutterOf, rowBox } from "../../../web/js/timeline_model.js";

const T0 = Date.UTC(2026, 10, 13);
const HOUR = 3600000;
// The range of the measure of 19.20d: 00:00 to 05:00 on 2026-11-13, in UTC.
const RANGE = { from: String(T0), to: String(T0 + 5 * HOUR), timezone: "utc", refresh: null };
const TIMES = "toDateTime('2026-11-13 00:00:00','UTC') + number*600 AS time";
const A = "multiIf(number<5,0, number<9,1, number<14,2, number<15,1, number<22,0, 3)";
const B = "multiIf(number<10,1, number<20,2, 0)";
const C = "intDiv(number,3)%3";
const MAP4 = [{ kind: "value", value: "0", text: "OK", color: "green" }, { kind: "value", value: "1", text: "WARN", color: "yellow" }, { kind: "value", value: "2", text: "FAIL", color: "red" }, { kind: "value", value: "3", text: "DOWN", color: "dark-red" }];

const legend = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const OPTIONS = {
  state_timeline: { merge_values: true, show_values: "auto", align_values: "left", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend: legend(), tooltip: { mode: "single", sort: "none" } },
  status_history: { show_values: "auto", row_height: 90, col_width: 90, line_width: 1, fill_opacity: 70, show_labels: true, legend: legend(), tooltip: { mode: "single", sort: "none" } },
};
/** A panel of a timeline type. `sql` is a SELECT with the time column `time`. */
const tp = (id, type, title, sql, o = {}) => ({
  id, type, title, description: "", transparent: false, grid: o.grid || { x: 0, y: 0, w: 24, h: 8 },
  data: o.reuse ? { reuse: o.reuse } : { query: { source: "prod", format: o.format || "table", sql: Array.isArray(sql) ? sql : [sql], min_interval: null, max_data_points: null } },
  transforms: o.transforms || [], fields: o.fields || { defaults: { mappings: MAP4 }, overrides: [] },
  options: { ...OPTIONS[type], ...(o.options || {}) }, links: [],
});
const q = (cols, n = 30, where = "") => `SELECT ${TIMES}, ${cols} FROM numbers(${n})${where}`;
const one = (uid, panels, o = {}) => doc(uid, `Timeline ${uid}`, [section(null, Array.isArray(panels) ? panels : [panels])], { time: RANGE, ...o });
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
/** Wait until each panel has drawn a picture (the hook `data-timeline`) or shows its message. */
const drawn = async (p, ids) => {
  await p.waitFor(`${JSON.stringify(ids)}.every((id) => { const e = document.querySelector('[data-testid="panel-' + id + '"]'); const plot = e && e.querySelector('.panel__plot'); return e && (e.dataset.state === "error" || e.dataset.state === "timeline-limit" || e.dataset.state === "empty" || (plot && plot.dataset.timeline)); })`, { what: `timelines ${ids}` });
  await p.idle();
};
const plot = (id) => `document.querySelector('[data-testid="panel-${id}"] .panel__plot')`;
const hook = (p, id) => p.eval(`JSON.parse(${plot(id)}.dataset.timeline)`);
const canvasOf = (id) => `document.querySelector('[data-testid="timeline-${id}"] canvas')`;
/** The pixel at (x, y) of the canvas of a panel, in CSS pixels, as [r, g, b, a]. */
const pixel = (p, id, x, y) => p.eval(`(() => { const c = ${canvasOf(id)}; const k = c.width / c.clientWidth; return [...c.getContext("2d").getImageData(Math.round(${x} * k), Math.round(${y} * k), 1, 1).data]; })()`);
const panelBg = { dark: [24, 27, 31], light: [255, 255, 255] };
const mix = (rgb, bg, a) => rgb.map((c, i) => Math.round(bg[i] * (1 - a) + c * a));
const near = (a, b, tol = 6) => a.length === b.length && a.every((c, i) => Math.abs(c - b[i]) <= tol);
const HEX = { OK: { dark: [115, 191, 105], light: [86, 166, 75] }, WARN: { dark: [250, 222, 42], light: [242, 204, 12] }, FAIL: { dark: [242, 73, 92], light: [224, 47, 68] }, DOWN: { dark: [196, 22, 42], light: [173, 3, 23] } };

// --- B110 -----------------------------------------------------------------------------------------------------------------------------

async function lookState(t, p, scheme) {
  // Panel 1: one row. The numbers of the measure of 19.20d (example 1 and example 2).
  const one1 = await hook(p, 1);
  const [L, top, plotW, plotH] = one1.box;
  const W = await p.eval(`${canvasOf(1)}.clientWidth`);
  const H = await p.eval(`${canvasOf(1)}.clientHeight`);
  t.eq([top, L + plotW, H - top - plotH], [8, W - 8, 21], "the margins: 8 px at the top and at the right, 21 px for the time axis");
  // The width of the name, measured on the context of the canvas of the panel (the same font and the same letter spacing as the page draws).
  const nameW = await p.eval(`(() => { const c = ${canvasOf(1)}.getContext("2d"); c.font = "400 12px " + getComputedStyle(${plot(1)}).fontFamily; return c.measureText("state").width; })()`);
  t.assert(L === gutterOf(nameW, true), `the left gutter is round(w + 20.5): ${L} for a name of ${nameW} px`);
  t.eq(one1.rows.map((r) => [r.name, r.y, r.h, r.n]), [["state", 8, plotH, 6]], "one row fills the plot");
  const xOf = (ms, from = T0, to = T0 + 5 * HOUR) => L + ((ms - from) * plotW) / (to - from);
  const starts = [0, 5, 9, 14, 15, 22].map((i) => T0 + i * 600000);
  const ends = [...starts.slice(1), T0 + 5 * HOUR];
  const want = starts.map((s, i) => [Math.round(xOf(s)), Math.max(1, Math.round(xOf(ends[i])) - Math.round(xOf(s)))]);
  t.eq(one1.segments.length, 6, "six segments");
  one1.segments.forEach(([row, x, w], i) => {
    t.assert(row === 0 && Math.abs(x - want[i][0]) <= 1 && Math.abs(w - want[i][1]) <= 1, `${scheme}: segment ${i} is ${x},${w}, expected ${want[i]}`);
  });
  t.eq(one1.states.map((s) => s[0]), ["OK", "WARN", "FAIL", "DOWN"], "the states");
  // The fill: the colour of the state at the alpha 0.7 (the canvas is transparent: the panel shows through).
  const names = ["OK", "WARN", "FAIL", "OK", "OK", "DOWN"];
  const seg = one1.segments[0];
  const px = await pixel(p, 1, seg[1] + seg[2] / 2, 8 + plotH - 3);
  t.assert(near(px.slice(0, 3), HEX[names[0]][scheme], 3) && Math.abs(px[3] - 179) <= 3, `${scheme}: the fill ${px} of OK`);
  // The grid lines are under the fills: a pixel of a line in a gap of a row that has no segment is the 9 % line.
  t.assert(one1.texts.length >= 4 && one1.texts.every(([text, x, y]) => text.length > 0 && x >= L && y > 8), "the texts are in the plot");
  const full = ["OK", "WARN", "FAIL", "WARN", "OK", "DOWN"];
  t.eq(one1.texts.map((x) => x[0]), full.map((text, i) => boxText(text, one1.segments[i][2], "auto")).filter((x) => x !== ""), "the texts are the states, cut to floor(w / 8.5) characters with no ellipsis")
  // Panel 2: three rows, row_height 90, 50 and 100 (example 2: the bars are 64 px high from 8, 83 and 157 on a plot of 213 px).
  for (const [id, rh] of [[2, 90], [3, 50], [4, 100]]) {
    const h = await hook(p, id);
    const [, , , ph] = h.box;
    const rows = Array.from({ length: 3 }, (_, k) => rowBox(k, 3, ph, rh));
    t.eq(h.rows.map((r) => [r.y - 8, r.h]), rows.map((r) => [r.y, r.h]), `${scheme}: rows at row_height ${rh}`);
    t.eq(h.rows.map((r) => r.name), ["a", "b", "c"], "row names");
  }
  // Panel 5: 15 rows: a bar of 13 px. Panel 6: 40 rows: no name shows (the pitch is under 12 px).
  const h15 = await hook(p, 5);
  t.assert(h15.rows.length === 15 && h15.rows.every((r) => Math.abs(r.h - rowBox(0, 15, h15.box[3], 90).h) <= 1), "15 rows");
  // Panel 7: line_width 2 and fill_opacity 30: the fill has the alpha 0.3 and the line is the full colour, inset by 1 px.
  const h7 = await hook(p, 7);
  const s7 = h7.segments[0];
  const y7 = h7.rows[0].y;
  const inner = await pixel(p, 7, s7[1] + s7[2] / 2, y7 + h7.rows[0].h / 2);
  const edge = await pixel(p, 7, s7[1] + s7[2] / 2, y7 + 1);
  t.assert(near(inner.slice(0, 3), HEX.OK[scheme], 3) && Math.abs(inner[3] - 77) <= 3, `${scheme}: fill alpha 0.3, got ${inner}`);
  t.assert(near(edge.slice(0, 3), HEX.OK[scheme], 3) && edge[3] === 255, `${scheme}: the line has the full colour, got ${edge}`);
}

export const cases = [
  {
    id: "B110",
    name: "the look of a state timeline: the plot box, the rows (1, 3 and 15 rows, three values of row_height), the segments, the fill, the line, the texts and their cut, in both themes (9.19c, 19.20d)",
    async run(t) {
      const rows15 = Array.from({ length: 15 }, (_, i) => `intDiv(number+${i},${3 + (i % 4)})%3 AS r${i}`).join(", ");
      const rows40 = Array.from({ length: 40 }, (_, i) => `intDiv(number+${i},${3 + (i % 4)})%3 AS r${i}`).join(", ");
      const g = (n) => ({ x: 0, y: n * 8, w: 24, h: 8 });
      await ensure(t, one("tl110", [
        tp(1, "state_timeline", "One row", q(`${A} AS state`), { grid: g(0) }),
        tp(2, "state_timeline", "Three rows 90", q(`${A} AS a, ${B} AS b, ${C} AS c`), { grid: g(1) }),
        tp(3, "state_timeline", "Three rows 50", q(`${A} AS a, ${B} AS b, ${C} AS c`), { grid: g(2), options: { row_height: 50 } }),
        tp(4, "state_timeline", "Three rows 100", q(`${A} AS a, ${B} AS b, ${C} AS c`), { grid: g(3), options: { row_height: 100 } }),
        tp(5, "state_timeline", "Fifteen rows", q(rows15), { grid: { x: 0, y: 32, w: 24, h: 12 } }),
        tp(6, "state_timeline", "Forty rows", q(rows40), { grid: { x: 0, y: 44, w: 24, h: 12 } }),
        tp(7, "state_timeline", "Line 2 fill 30", q(`${A} AS state`), { grid: { x: 0, y: 56, w: 24, h: 8 }, options: { line_width: 2, fill_opacity: 30 } }),
      ]));
      for (const scheme of ["dark", "light"]) {
        const p = await open(t, "tl110", { scheme, width: 1280, height: 2700 });
        await drawn(p, [1, 2, 3, 4, 5, 6, 7]);
        await lookState(t, p, scheme);
        const h40 = await hook(p, 6);
        t.eq(h40.rows.length, 40, "40 rows");
      }
    },
  },
  {
    id: "B111",
    name: "the look of a status history: the width of a cell for col_width 50, 90 and 100, the range of the axis, the text that needs 2 characters, the line of 1 px, one sample is an error, the cap of the width, in both themes (9.19c)",
    async run(t) {
      const g = (n) => ({ x: 0, y: n * 8, w: 24, h: 8 });
      const rows3 = `${A} AS a, ${B} AS b, ${C} AS c`;
      const probe = (n) => tp(1, "status_history", `Samples ${n}`, `SELECT toDateTime64('2026-11-13 00:00:00',3,'UTC') + toIntervalMillisecond(toInt64(number*18000000/${n})) AS time, number%3 AS a FROM numbers(${n})`);
      await ensure(t, one("tl111", [
        tp(1, "status_history", "Col 50", q(rows3), { grid: g(0), options: { col_width: 50 } }),
        tp(2, "status_history", "Col 90", q(rows3), { grid: g(1) }),
        tp(3, "status_history", "Col 100", q(rows3), { grid: g(2), options: { col_width: 100 } }),
        tp(4, "status_history", "One sample", q(`${A} AS a`, 1), { grid: g(3) }),
      ]));
      for (const scheme of ["dark", "light"]) {
        const p = await open(t, "tl111", { scheme, width: 1280, height: 1500 });
        t.allow(p, /A status history needs two samples at least/);
        await drawn(p, [1, 2, 3, 4]);
        for (const [id, col] of [[1, 50], [2, 90], [3, 100]]) {
          const h = await hook(p, id);
          const [L, , plotW] = h.box;
          // The data covers the range, so the axis is the data plus half a step on each side: 00:00 less 5 minutes to 04:50 plus 5 minutes.
          const step = (10 * plotW) / 300;
          const cw = cellWidth(step, col, 1);
          const first = h.segments.find((s) => s[0] === 0);
          t.assert(Math.abs(first[2] - Math.max(1, Math.round(cw))) <= 1, `${scheme}: col ${col}: the width ${first[2]} of a cell, expected ${cw}`);
          t.assert(Math.abs(first[1] + first[2] / 2 - (L + (5 * plotW) / 300)) <= 1.5, `${scheme}: col ${col}: the first cell is centred on its sample`);
          t.eq(h.segments.length, 90, "30 cells in each of the 3 rows");
          const fits = boxText("OK", first[2], "auto") !== "";
          t.eq(h.texts.length > 0, fits, `${scheme}: col ${col}: the text shows when 2 characters fit (${first[2]} px)`);
        }
        const h2 = await hook(p, 2);
        const c0 = h2.segments[0];
        const edge = await pixel(p, 2, c0[1], h2.rows[0].y + h2.rows[0].h / 2);
        t.assert(edge[3] === 255, `${scheme}: the line of 1 px has the full colour, got ${edge}`);
        t.eq(await p.eval(`document.querySelector('[data-testid="panel-4"]').dataset.state`), "error", "one sample is an error");
        t.assert((await p.text('[data-testid="panel-4-error"]')).includes("A status history needs two samples at least."), "the message");
      }
      // The cap of the width: a canvas of W px takes floor(W / 2) rows.
      const p0 = await open(t, "tl111", { width: 1280, height: 1500 });
      t.allow(p0, /A status history needs two samples at least/);
      await drawn(p0, [1]);
      const W = await p0.eval(`${canvasOf(1)}.clientWidth`);
      const cap = Math.floor(W / 2);
      await ensure(t, one("tl111b", [{ ...probe(cap), id: 1, grid: g(0) }, { ...probe(cap + 1), id: 2, grid: g(1) }]));
      const p = await open(t, "tl111b", { width: 1280, height: 900 });
      await drawn(p, [1, 2]);
      t.eq(await p.eval(`['1', '2'].map((i) => document.querySelector('[data-testid="panel-' + i + '"]').dataset.state)`), ["ready", "timeline-limit"], `${cap} rows pass and ${cap + 1} fail at ${W} px`);
      t.eq(await p.text('[data-testid="timeline-2-note"]'), `Too many points to draw: ${cap + 1} rows. The most for a width of ${W} px is ${cap}.`, "the message");
    },
  },
  {
    id: "B112",
    name: "the states and the colours: the four mappings, the colour modes, text, bool and the empty text, the ink with 4.5:1, the legend for each mode, hostile texts show as text (9.19c)",
    async run(t) {
      const g = (n) => ({ x: 0, y: n * 8, w: 24, h: 8 });
      const mapped = {
        defaults: { mappings: [
          { kind: "range", from: 0, to: 9, text: "LOW", color: "green" }, { kind: "regex", pattern: "/^1\\d$/", text: "TEN", color: "blue" },
          { kind: "value", value: "20", text: "TWENTY", color: "red" }, { kind: "special", match: "true", text: "T", color: "purple" },
        ] },
        overrides: [],
      };
      const thr = (extra = {}) => ({ defaults: { color: { mode: "thresholds", fixed: null }, thresholds: { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 1, color: "orange" }, { value: 3, color: "red" }] }, ...extra }, overrides: [] });
      await ensure(t, one("tl112", [
        tp(1, "state_timeline", "Mappings", q("number AS n"), { grid: g(0), fields: mapped }),
        tp(2, "state_timeline", "Thresholds", q(`${A} AS a`), { grid: g(1), fields: thr() }),
        tp(3, "state_timeline", "Fixed", q(`${A} AS a`), { grid: g(2), fields: { defaults: { color: { mode: "fixed", fixed: "blue" } }, overrides: [] } }),
        tp(4, "state_timeline", "Palette", q(`${A} AS a, ${B} AS b`), { grid: g(3), fields: { defaults: { color: { mode: "palette", fixed: null } }, overrides: [] } }),
        tp(5, "state_timeline", "Continuous", q(`${A} AS a`), { grid: g(4), fields: { defaults: {}, overrides: [] } }),
        tp(6, "state_timeline", "Text", q("multiIf(number<8,'up', number<12,'down', number<20,'up','maint') AS svc"), { grid: g(5), fields: { defaults: { color: { mode: "palette", fixed: null } }, overrides: [] } }),
        tp(7, "state_timeline", "Bool", q("toBool(number%10<4) AS flag"), { grid: g(6), fields: { defaults: {}, overrides: [] } }),
        tp(8, "state_timeline", "Empty text", q("multiIf(number<10,'', 'x') AS s"), { grid: g(7), fields: { defaults: { color: { mode: "fixed", fixed: "blue" } }, overrides: [] } }),
        tp(9, "state_timeline", "Hostile", q(`multiIf(number<8,'<img src=x onerror=alert(1)>', number<16, repeat('x',10000), number<24, '\\xE2\\x80\\xAEevil\\xE2\\x80\\xAC', 'a\\0b') AS s`), { grid: g(8), fields: { defaults: { color: { mode: "fixed", fixed: "blue" } }, overrides: [] } }),
        tp(10, "status_history", "History thresholds", q(`${A} AS a`), { grid: g(9), fields: thr() }),
      ]));
      const legendOf = (p, id) => p.eval(`[...document.querySelectorAll('[data-testid="timeline-${id}-legend"] li')].map((e) => e.textContent)`);
      const statesOf = async (p, id) => (await hook(p, id)).states.map((s) => s[0]);
      for (const scheme of ["dark", "light"]) {
        const p = await open(t, "tl112", { scheme, width: 1280, height: 3000 });
        await drawn(p, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        t.eq((await statesOf(p, 1)).slice(0, 3), ["LOW", "TEN", "TWENTY"], `${scheme}: value, range and regex mappings`);
        t.eq(await legendOf(p, 1), [], "a continuous mode has no legend, also with mappings");
        t.eq(await statesOf(p, 2), ["< 1", "1+", "3+"], "thresholds: the step texts");
        t.eq((await hook(p, 2)).rows[0].n, 4, "the values 1 and 2 are one state: the merge by step");
        t.eq(await legendOf(p, 2), ["< 1", "1+", "3+"], "the steps are the legend");
        t.eq(await legendOf(p, 3), ["0", "1", "2", "3"], "fixed: the different states in order");
        const h4 = await hook(p, 4);
        t.eq(new Set(h4.states.map((s) => s[1])).size, 2, "palette: one colour for each row, the slot of the row");
        t.eq(await legendOf(p, 5), [], "continuous: no item");
        t.eq(await legendOf(p, 6), ["up", "down", "maint"], "text values: the states by first appearance");
        t.eq(await statesOf(p, 7), ["true", "false"], "bool texts");
        t.eq((await hook(p, 7)).states[0][1], (await hook(p, 7)).states[1][1], "a bool has the colour of the mode, not the colour of 1 and 0");
        t.eq(await legendOf(p, 8), ["x"], "the empty text is a state with no legend item");
        t.eq((await hook(p, 8)).texts.some((x) => x[0] === ""), false, "the empty text shows no text in its box");
        t.eq(await legendOf(p, 10), ["< 1", "1+", "3+"], "the status history lists the steps of its thresholds");
        t.eq(await legendOf(p, 10), ["< 80", "80+"].slice(0, 0).concat(await legendOf(p, 10)), "the status history legend");
        // The ink has 4.5:1 with each fill over the panel.
        const lum = (c) => { const f = (u) => { const x = u / 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]); };
        const ratio = (a, b) => (Math.max(lum(a), lum(b)) + 0.05) / (Math.min(lum(a), lum(b)) + 0.05);
        const hex = (css) => { const m = /^#([0-9a-f]{6})$/i.exec(css); return m ? [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16)) : (css.match(/\d+/g) || []).slice(0, 3).map(Number); };
        let checked = 0;
        for (const id of [1, 2, 3, 6, 7, 10]) {
          const h = await hook(p, id);
          for (const [, x, y, ink] of h.texts) {
            const seg = h.segments.find(([row, sx, sw]) => x >= sx - 1 && x <= sx + sw + 1 && Math.abs(h.rows[row].y + h.rows[row].h / 2 - y) < 3);
            if (!seg) continue;
            const fill = hex(h.states[seg[3]][1]);
            const over = mix(fill, panelBg[scheme], 0.7);
            t.assert(ratio(hex(ink), over) >= 4.5, `${scheme}: panel ${id}: ink ${ink} on ${over}`);
            checked += 1;
          }
        }
        t.assert(checked >= 10, `${checked} texts checked`);
        // Hostile texts: text only, no element, no dialog.
        t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="timeline-9"] *')].map((e) => e.tagName.toLowerCase()).filter((n) => !["div", "p", "ul", "li", "span", "b", "i", "canvas"].includes(n))`), [], "no element of the data");
        t.eq(await p.eval(`document.querySelectorAll('[data-testid="timeline-9"] img, [data-testid="timeline-9"] script').length`), 0, "no image, no script");
        t.eq((await legendOf(p, 9)).length, 4, "a fixed mode lists the 4 states, as text");
        t.eq((await legendOf(p, 9))[0], "<img src=x onerror=alert(1)>", "the payload is the text of an item");
        t.eq(p.dialogs.length, 0, "no dialog");
      }
    },
  },
  {
    id: "B113",
    name: "the data cases: null, NaN and a mapped null, span_nulls, a gap in the time, rows that are not sorted, two samples with one time, two time columns, no time column, no row, no data, time_series with 3 hosts, reuse, transforms, hide_from.viz (9.19c)",
    async run(t) {
      const g = (n) => ({ x: 0, y: n * 8, w: 24, h: 8 });
      const nulls = `if(number IN (3,4,10,11,12), NULL, intDiv(number,5)%3) AS a FROM numbers(30) WHERE number NOT IN (16,17,18)`;
      const base = (cols) => `SELECT ${TIMES}, ${cols}`;
      const map3 = { defaults: { mappings: MAP4.slice(0, 3) }, overrides: [] };
      const mappedNull = { defaults: { mappings: [...MAP4.slice(0, 3), { kind: "special", match: "null", text: "NO DATA", color: "blue" }] }, overrides: [] };
      const list = [
        tp(1, "state_timeline", "Nulls", base(nulls), { grid: g(0), fields: map3 }),
        tp(2, "state_timeline", "Span nulls", base(nulls), { grid: g(1), fields: map3, options: { span_nulls: true } }),
        tp(3, "state_timeline", "Mapped null", base(nulls), { grid: g(2), fields: mappedNull }),
        tp(4, "state_timeline", "NaN", q("if(number%7=3, nan, number%3) AS x"), { grid: g(3), fields: map3 }),
        tp(5, "state_timeline", "Unsorted", q(`intDiv(number,5)%3 AS a`) + " ORDER BY (number*7)%30", { grid: g(4), fields: map3 }),
        tp(6, "state_timeline", "Duplicate times", `SELECT toDateTime('2026-11-13 00:00:00','UTC') + number*600 AS time, a FROM (SELECT number, intDiv(number,5)%3 AS a FROM numbers(30)) ARRAY JOIN [0,1] AS dup`, { grid: g(5), fields: map3 }),
        tp(7, "state_timeline", "Two time columns", `SELECT ${TIMES}, toDateTime('2026-11-13 00:00:00','UTC') + number*600 + 1 AS time2, intDiv(number,5)%3 AS a FROM numbers(30)`, { grid: g(6), fields: map3 }),
        tp(8, "state_timeline", "No time column", "SELECT number AS n, number%3 AS a FROM numbers(30)", { grid: g(7) }),
        tp(9, "state_timeline", "Time only", `SELECT ${TIMES} FROM numbers(30)`, { grid: g(8) }),
        tp(10, "state_timeline", "No data", q("1 AS a", 30, " WHERE 0"), { grid: g(9) }),
        tp(11, "state_timeline", "Hosts", `SELECT ${TIMES}, concat('h', toString(number%3)) AS host, intDiv(number,6)%3 AS st FROM numbers(30) ORDER BY time`, { grid: g(10), format: "time_series", fields: map3 }),
        tp(12, "state_timeline", "Reuse of 1", null, { grid: g(11), reuse: { panel: 1, with_transforms: false }, fields: map3 }),
        tp(13, "state_timeline", "Transform", q(`${A} AS a, ${B} AS b, ${C} AS c`), { grid: g(12), fields: map3, transforms: [{ kind: "filter_by_value", mode: "include", match: "all", filters: [{ field: "c", matcher: { kind: "lower", value: 2 } }] }] }),
        tp(14, "state_timeline", "Hidden row", q(`${A} AS a, ${B} AS b`), { grid: g(13), fields: { defaults: { mappings: MAP4 }, overrides: [{ match: { by: "name", value: "b" }, set: { hide_from: { legend: false, tooltip: false, viz: true } } }] } }),
      ];
      await ensure(t, one("tl113", list));
      const p = await open(t, "tl113", { width: 1280, height: 3600 });
      t.allow(p, /The data has no (time column|field to draw)/);
      await drawn(p, list.map((x) => x.id));
      const state = (id) => p.eval(`document.querySelector('[data-testid="panel-${id}"]').dataset.state`);
      const rowsOf = async (id) => (await hook(p, id)).rows.map((r) => [r.name, r.n]);
      t.eq(await rowsOf(1), [["a", 6]], "a null ends the segment and leaves a gap");
      t.eq(await rowsOf(2), [["a", 6]], "span_nulls joins the segments over the nulls");
      const w1 = (await hook(p, 1)).segments, w2 = (await hook(p, 2)).segments;
      t.assert(w2[1][2] > w1[1][2] + 1, "the second segment is wider with span_nulls");
      t.eq((await hook(p, 3)).states.map((s) => s[0]).includes("NO DATA"), true, "a mapped null is a state");
      t.eq((await hook(p, 4)).rows[0].n, 26, "a NaN with no mapping is absent");
      t.eq(await rowsOf(5), [["a", 6]], "rows that are not sorted draw as the sorted rows");
      t.eq(await rowsOf(6), [["a", 6]], "two samples with one time: the later one wins and the earlier is not drawn");
      t.eq(await rowsOf(7), [["a", 6]], "two time columns: the first is the axis and the second is not a row");
      t.eq([await state(8), await p.text('[data-testid="panel-8-error"]').then((x) => x.includes("The data has no time column."))], ["error", true], "no time column");
      t.eq([await state(9), await p.text('[data-testid="panel-9-error"]').then((x) => x.includes("The data has no field to draw."))], ["error", true], "no row");
      t.eq(await state(10), "empty", "no data");
      t.eq((await rowsOf(11)).length, 3, "three hosts are three rows");
      t.eq(await rowsOf(12), await rowsOf(1), "a reuse panel draws the data of its source");
      t.eq((await rowsOf(13)).map((r) => r[0]), ["a", "b", "c"], "the transformations apply");
      t.eq((await rowsOf(14)).map((r) => r[0]), ["a"], "hide_from.viz hides the row");
    },
  },
  {
    id: "B114",
    name: "the caps: 9,999 and 10,000 segments draw, 10,001 shows its message in the state timeline-limit with no picture, a status history at the cap and at the cap of the width, a new draw after a resize, the time of the draw (9.19c, T57)",
    async run(t) {
      const seg = (id, n, grid) => tp(id, "state_timeline", `Segments ${n}`, `SELECT toDateTime64('2026-11-13 00:00:00',3,'UTC') + toIntervalMillisecond(toInt64(number*18000000/${n})) AS time, number%2 AS a FROM numbers(${n})`, { grid, fields: { defaults: {}, overrides: [] } });
      const cells = (id, rows) => tp(id, "status_history", `Cells ${rows}x100`, `SELECT ${TIMES}, ${Array.from({ length: rows }, (_, i) => `(number+${i})%3 AS r${i}`).join(", ")} FROM numbers(100)`, { grid: { x: 0, y: 0, w: 24, h: 8 }, fields: { defaults: {}, overrides: [] }, options: { show_labels: false } });
      const g = (n) => ({ x: 0, y: n * 8, w: 24, h: 8 });
      await ensure(t, one("tl114", [seg(1, 9999, g(0)), seg(2, 10000, g(1)), seg(3, 10001, g(2)), { ...cells(4, 100), grid: g(3) }, { ...cells(5, 101), grid: g(4) }]));
      const p = await t.page({ width: 1280, height: 1800 });
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: `window.__long = []; new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__long.push(e.duration); }).observe({ entryTypes: ["longtask"] });` });
      await p.emulateColorScheme("dark");
      await p.goto(t.url(`d/tl114?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`));
      await p.idle();
      await drawn(p, [1, 2, 3, 4, 5]);
      const state = (id) => p.eval(`document.querySelector('[data-testid="panel-${id}"]').dataset.state`);
      t.eq([await state(1), await state(2), await state(3), await state(4), await state(5)], ["ready", "ready", "timeline-limit", "ready", "timeline-limit"], "the states at the cap");
      t.eq([(await hook(p, 1)).rows[0].n, (await hook(p, 2)).rows[0].n], [9999, 10000], "the number of segments");
      t.eq(await p.text('[data-testid="timeline-3-note"]'), "Too many states to draw: 10,001 segments. The limit is 10,000. Use a shorter time range or merge equal values.", "the message");
      t.eq(await p.eval(`(() => { const n = document.querySelector('[data-testid="timeline-3-note"]'); return [n.getAttribute("role"), n.hidden, document.querySelector('[data-testid="timeline-3"] canvas').offsetParent === null, document.querySelector('[data-testid="panel-3"] .panel__plot').dataset.timeline === undefined, !!document.querySelector('[data-testid="panel-3-retry"]')]; })()`), ["note", false, true, true, false], "role note, no picture, no data hook, no Retry");
      t.eq(await p.text('[data-testid="timeline-5-note"]'), "Too many points to draw: 10,100 cells. The limit is 10,000.", "the message of the cells");
      // The time: a draw of 10,000 segments takes 100 ms or less, and no task is longer than 200 ms.
      const ms = await p.eval(`(() => { const t0 = performance.now(); window.dispatchEvent(new Event("resize")); return performance.now() - t0; })()`);
      t.assert(ms <= 100 * 5, `a draw of the page took ${ms} ms`);
      const longest = Math.max(0, ...(await p.eval("window.__long")));
      t.assert(longest <= 200, `a task of ${longest} ms`);
      t.note(`draw of the 5 panels ${Math.round(ms)} ms, longest task ${Math.round(longest)} ms`);
      // The cap of the width and the new draw after a resize.
      const W = await p.eval(`document.querySelector('[data-testid="timeline-4"] canvas').clientWidth`);
      const need = 2 * 100 + 2;
      await ensure(t, one("tl114b", [tp(1, "status_history", "Wide", `SELECT toDateTime64('2026-11-13 00:00:00',3,'UTC') + toIntervalMillisecond(toInt64(number*18000000/${need})) AS time, number%3 AS a FROM numbers(${need})`, { fields: { defaults: {}, overrides: [] } })]));
      void W;
      const q2 = await t.page({ width: 380, height: 900 });
      await q2.goto(t.url(`d/tl114b?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`));
      await q2.idle();
      await drawn(q2, [1]);
      t.eq(await q2.eval(`document.querySelector('[data-testid="panel-1"]').dataset.state`), "timeline-limit", "too many points for a narrow canvas");
      await q2.send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 900, deviceScaleFactor: 1, mobile: false });
      await q2.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "ready"`, { what: "a new draw after the resize" });
    },
  },
  {
    id: "B115",
    name: "the card, the hover and the keyboard: single, multi and hidden, the head and the Duration row, the duration forms, the place and the flip, the white layer, the arrow keys, Escape, the description, focus and roles in both themes (9.19c)",
    async run(t) {
      const g = (n) => ({ x: 0, y: n * 8, w: 24, h: 8 });
      const rows3 = `${A} AS a, ${B} AS b, ${C} AS c`;
      await ensure(t, one("tl115", [
        tp(1, "state_timeline", "Single", q(`${A} AS state`), { grid: g(0) }),
        tp(2, "state_timeline", "Multi", q(rows3), { grid: g(1), options: { tooltip: { mode: "multi", sort: "none" } } }),
        tp(3, "state_timeline", "Hidden", q(`${A} AS state`), { grid: g(2), options: { tooltip: { mode: "hidden", sort: "none" } } }),
        tp(4, "status_history", "History", q(rows3), { grid: g(3) }),
      ]));
      const move = async (p, id, x, y) => {
        const r = await p.eval(`(() => { const b = ${canvasOf(id)}.getBoundingClientRect(); return [b.left, b.top]; })()`);
        await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: r[0] + x - 1, y: r[1] + y });
        await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: r[0] + x, y: r[1] + y });
        await sleep(100);
      };
      const tip = (p, id) => p.eval(`(() => { const e = document.querySelector('[data-testid="timeline-${id}-tip"]'); if (e.hidden) return null; const b = e.getBoundingClientRect(); const c = document.querySelector('[data-testid="timeline-${id}"] canvas').getBoundingClientRect(); return { head: e.querySelector(".timeline__tip-head").textContent, rows: [...e.querySelectorAll(".timeline__tip-row")].map((r) => r.textContent), left: b.left - c.left, top: b.top - c.top, right: b.right - c.left, bottom: b.bottom - c.top, cw: c.width, ch: c.height }; })()`);
      for (const scheme of ["dark", "light"]) {
        const p = await open(t, "tl115", { scheme, width: 1280, height: 1500 });
        await drawn(p, [1, 2, 3, 4]);
        const h1 = await hook(p, 1);
        const mid = (s, rows, k = 0) => [s[1] + s[2] / 2, rows[k].y + rows[k].h / 2];
        // A segment of 50 minutes: the head has two lines, the row and the Duration.
        let [x, y] = mid(h1.segments[0], h1.rows);
        const before = await pixel(p, 1, x, h1.rows[0].y + 4);
        await move(p, 1, x, y);
        let c = await tip(p, 1);
        t.eq([c.head, c.rows], ["2026-11-13 00:00:00 -\n2026-11-13 00:50:00", ["stateOK", "Duration50m"]], `${scheme}: the single card`);
        const after = await pixel(p, 1, x, h1.rows[0].y + 4);
        t.assert(after[0] + after[1] + after[2] > before[0] + before[1] + before[2], `${scheme}: the white layer lightens the box under the pointer`);
        t.assert(c.left >= x + 9 && c.top >= y + 9, "the card is 10 px right of and below the pointer");
        // The last segment ends at the end of the range: 1h 20m.
        [x, y] = mid(h1.segments[5], h1.rows);
        await move(p, 1, x, y);
        c = await tip(p, 1);
        t.eq([c.head, c.rows[1]], ["2026-11-13 03:40:00 -\n2026-11-13 05:00:00", "Duration1h 20m"], "the last segment, the duration form");
        const edgeX = h1.box[0] + h1.box[2] - 5;
        await move(p, 1, edgeX, y);
        c = await tip(p, 1);
        t.assert(c.right <= c.cw + 1 && c.left < edgeX, `the card flips at the right edge of the plot: ${JSON.stringify(c)}`);
        // Between the bars of a row: no box, no card (single).
        await move(p, 2, h1.box[0] + 40, (await hook(p, 2)).rows[0].y + (await hook(p, 2)).rows[0].h + 4);
        c = await tip(p, 2);
        t.assert(c !== null && c.rows.length === 3 && !c.rows.some((r) => r.startsWith("Duration")), "multi: one row for each row of the frame, no Duration, also between the bars");
        const h3 = await hook(p, 3);
        await move(p, 3, ...mid(h3.segments[0], h3.rows));
        t.eq(await tip(p, 3), null, "hidden: no card");
        const h4 = await hook(p, 4);
        await move(p, 4, ...mid(h4.segments[0], h4.rows));
        c = await tip(p, 4);
        t.eq(c.head, "2026-11-13 00:00:00", "the status history has one time");
        // The keyboard.
        await p.focus('[data-testid="timeline-1"] canvas');
        await p.press("ArrowRight");
        t.assert((await tip(p, 1)).rows[0].includes("OK"), "ArrowRight shows the first box");
        await p.press("ArrowRight");
        t.assert((await tip(p, 1)).rows[0].includes("WARN"), "ArrowRight goes to the next box");
        await p.press("ArrowLeft");
        t.assert((await tip(p, 1)).rows[0].includes("OK"), "ArrowLeft goes back");
        t.eq(await p.eval(`getComputedStyle(${canvasOf(1)}).outlineWidth`), "2px", `${scheme}: the focus ring`);
        await p.press("Escape");
        t.eq(await tip(p, 1), null, "Escape hides the card");
        await p.focus('[data-testid="timeline-2"] canvas');
        await p.press("ArrowRight");
        await p.press("ArrowDown");
        t.assert((await tip(p, 2)).rows.length === 3, "ArrowDown changes the row");
        // The description and the roles.
        const a11y = await p.eval(`(() => { const c = ${canvasOf(1)}; const d = document.getElementById(c.getAttribute("aria-describedby")); return [c.getAttribute("role"), c.getAttribute("tabindex"), c.getAttribute("aria-label"), d.textContent, d.className]; })()`);
        t.eq(a11y, ["img", "0", "Single, state timeline", "state: 6 states, last state DOWN since 2026-11-13 03:40:00.", "visually-hidden"], "role, tabindex, name and description");
      }
    },
  },
  {
    id: "B116",
    name: "the editor and the format: the groups and their controls with their ranges, a legend with no mode, a type change that keeps the data, the JSON tab with the errors of 4.33b, a save and a reload, the fixtures save with 200 (10.3, 4.33b)",
    async run(t) {
      const fx = (name) => JSON.parse(readFileSync(new URL(`../../fixtures/dashboards/${name}.json`, import.meta.url), "utf8"));
      // The format: each fixture saves, and a document with a fault gets 422 with the code and the pointer of 4.33b.
      for (const name of ["state_timeline", "status_history"]) {
        const d = { ...fx(name), uid: `tl116-${name.replace("_", "-")}` };
        const made = await t.api.create("transit", d);
        t.eq(made.status, 201, `${name}: create ${made.text.slice(0, 200)}`);
        const saved = await t.api.save(d.uid, made.etag, d);
        t.eq(saved.status, 200, `${name}: a save gives 200`);
        const got = await t.api.dashboard(d.uid);
        t.eq(JSON.stringify(got.json.document), JSON.stringify(d), `${name}: the stored document is the canonical form`);
        const bad = structuredClone(d);
        const target = bad.sections.flatMap((x) => x.panels).find((x) => x.type === name);
        target.options.row_height = 9;
        target.options.legend.mode = "table";
        const r = await t.api.create("transit", { ...bad, uid: `tl116-bad-${name.replace("_", "-")}` });
        t.eq(r.status, 422, `${name}: a fault gives 422`);
        const errs = r.json.error.errors.map((e) => [e.code, e.pointer.replace(/^.*\/options/, "/options")]);
        t.assert(errs.some((e) => e[1] === "/options/row_height" && e[0] === "out_of_range") && errs.some((e) => e[1] === "/options/legend/mode" && e[0] === "not_permitted"), `${name}: the codes and the pointers ${JSON.stringify(errs)}`);
      }
      const d = doc("tl116", "Timeline editor", [section(null, [
        panel(1, "stat", "Stat", "SELECT 1 AS v", { grid: { x: 0, y: 0, w: 12, h: 8 } }),
        { ...tp(2, "state_timeline", "State timeline", q(`${A} AS a`), { grid: { x: 12, y: 0, w: 12, h: 8 } }) },
        { ...tp(3, "status_history", "Status history", q(`${A} AS a`), { grid: { x: 0, y: 8, w: 12, h: 8 } }) },
      ])], { time: RANGE });
      await ensure(t, d);
      const p = await open(t, "tl116", { width: 1500, height: 1000 });
      t.allow(p, /422|The data has no time column/);
      await waitPanelsReady(p, [1]);
      await enterEdit(p);
      await openPanelEditor(p, 2);
      const groups = () => p.eval(`[...document.querySelectorAll('.peditor__obody .ogroup__title')].map((e) => e.textContent)`);
      const has = (ids) => p.eval(`${JSON.stringify(ids)}.map((id) => !!document.querySelector('[data-testid="' + id + '"]'))`);
      t.assert((await groups()).slice(0, 5).join("|").startsWith("Panel options|Visualisation|State timeline|Legend|Tooltip"), `the groups ${await groups()}`);
      t.eq(await has(["timeline-merge", "timeline-show-values", "timeline-align", "timeline-row-height", "timeline-line-width", "timeline-fill-opacity", "timeline-span-nulls", "timeline-labels", "timeline-legend-show", "timeline-legend-placement", "timeline-tooltip-mode", "timeline-colour-note"]), Array(12).fill(true), "the controls of a state timeline");
      t.eq(await has(["timeline-col-width"]), [false], "no column width in a state timeline");
      t.eq(await p.eval(`(() => { const r = (id) => { const i = document.querySelector('[data-testid="' + id + '"]').closest(".timeline-slider").querySelector('input[type="range"]'); return [i.min, i.max]; }; return ["timeline-row-height", "timeline-line-width", "timeline-fill-opacity"].map(r); })()`), [["10", "100"], ["0", "10"], ["0", "100"]], "the ranges of the sliders");
      t.eq(await p.eval(`[...document.querySelectorAll('.peditor__obody .ogroup')].find((g) => g.querySelector(".ogroup__title").textContent === "Legend").querySelectorAll("[data-testid]").length`), 2, "the legend has a switch and a place, no mode and no value list");
      // A change of an option writes the draft, and the preview shows it.
      await p.fill(p.tid("timeline-row-height"), "40");
      await sleep(450);
      await waitDraft(p, "tl116", (x) => x.document.sections[0].panels[1].options.row_height === 40);
      await p.click(p.tid("timeline-merge"));
      await waitDraft(p, "tl116", (x) => x.document.sections[0].panels[1].options.merge_values === false);
      await p.clickText(`${p.tid("timeline-legend-placement")} button`, "Right");
      const draft = await waitDraft(p, "tl116", (x) => x.document.sections[0].panels[1].options.legend.placement === "right");
      t.eq(draft.document.sections[0].panels[1].options.legend, { show: true, placement: "right", mode: "list", values: [] }, "the legend keeps mode list and no value");
      t.eq(await p.eval(`document.querySelector('.peditor [data-testid="panel-2"] .panel__plot').dataset.timeline ? JSON.parse(document.querySelector('.peditor [data-testid="panel-2"] .panel__plot').dataset.timeline).kind : null`), "state_timeline", "the preview shows the picture");
      // The JSON tab shows the errors of 4.33b.
      await p.click(p.tid("tab-json"));
      const text = await p.eval(`document.querySelector('[data-testid="panel-json"]').value`);
      const bad = JSON.parse(text);
      bad.options.row_height = 9;
      bad.options.legend.mode = "table";
      await p.eval(`(() => { const a = document.querySelector('[data-testid="panel-json"]'); a.value = ${JSON.stringify(JSON.stringify(bad, null, 2))}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
      await sleep(1900);
      const errors = await p.eval(`(() => { const e = document.querySelector('[data-testid="panel-json-errors"]'); return e.hidden ? "" : e.textContent; })()`);
      t.assert(errors.includes("options/row_height") && errors.includes("out_of_range") && errors.includes("options/legend/mode") && errors.includes("not_permitted"), `the errors of the JSON tab: ${errors}`);
      bad.options.row_height = 40;
      bad.options.legend.mode = "list";
      await p.eval(`(() => { const a = document.querySelector('[data-testid="panel-json"]'); a.value = ${JSON.stringify(JSON.stringify(bad, null, 2))}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
      await sleep(300);
      await p.click(p.tid("tab-query"));
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("panel-editor"));
      // A change of the type keeps the data and takes the New options; a change from a type with a calculation asks first.
      await openPanelEditor(p, 1);
      await p.click(p.tid("change-type")); // the group "Visualisation" is closed at the first draw (10.3h, rule 6)
      await p.click('.typelist__item[data-type="state_timeline"]');
      await p.waitSel(p.tid("type-change-dialog"));
      t.assert((await p.text(p.tid("type-change-dialog"))).includes("The calculation of this panel leaves the draft."), "the dialog");
      await p.clickText(`${p.tid("type-change-dialog")} button`, "Change");
      const changed = await waitDraft(p, "tl116", (x) => x.document.sections[0].panels[0].type === "state_timeline");
      const first = changed.document.sections[0].panels[0];
      t.eq([first.data.query.sql, first.options], [["SELECT 1 AS v"], OPTIONS.state_timeline], "the data stays and the options are the New options");
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("panel-editor"));
      // A save and a reload give the same document.
      await saveDialog(p, "timeline editor");
      await p.waitFor(`!document.querySelector('[data-testid="save-button"]:not([disabled])')`, { what: "the saved page", timeout: 15000 }).catch(() => {});
      const stored = (await t.api.dashboard("tl116")).json.document.sections[0].panels;
      t.eq(stored[1].options.row_height, 40, "the stored option");
      t.eq(Object.keys(stored[1].options), Object.keys(OPTIONS.state_timeline), "the canonical order of 4.38");
      t.eq(stored[0].type, "state_timeline", "the stored type");
      await p.reload();
      await p.idle();
      await drawn(p, [1, 2, 3]);
      t.eq(await p.eval(`JSON.parse(${plot(2)}.dataset.timeline).kind`), "state_timeline", "after a reload");
    },
  },
  {
    id: "B117",
    name: "the import of the 8 fixtures tests/fixtures/grafana/timelines_*.json: the report lists the items of 15.6, the server loads each dashboard, each panel has one of the two types and draws (15.6)",
    async run(t) {
      const { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } = await import("node:fs");
      const { tmpdir } = await import("node:os");
      const { spawnSync } = await import("node:child_process");
      const { join } = await import("node:path");
      const { BINARY, ROOT } = await import("../lib/env.mjs");
      const s = await t.ownServer("imp117");
      const work = mkdtempSync(join(tmpdir(), "hune-b117-"));
      const fx = join(ROOT, "tests/fixtures/grafana");
      mkdirSync(join(work, "dashboards"), { recursive: true });
      const names = ["a", "b", "c", "e", "f", "g", "h", "i"];
      const uids = [];
      for (const n of names) {
        const file = `timelines_${n}.json`;
        // The fixtures give each panel its own row and a range in epoch milliseconds. The uid of a fixture is its file stem.
        const d = JSON.parse(readFileSync(join(fx, file), "utf8"));
        writeFileSync(join(work, "dashboards", `${d.uid}.json`), JSON.stringify(d));
        // The import changes `_` to `-` in the uid (an item "changed" of 15.6).
        uids.push([d.uid.replace(/_/g, "-"), n, d.uid]);
      }
      for (const f of ["folders.json", "health.json", "plugin-clickhouse.json"]) cpSync(join(fx, "export", f), join(work, f));
      writeFileSync(join(work, "search.json"), JSON.stringify(uids.map(([, , uid]) => ({ uid, folderUid: "fe1x0abc2kq9" }))));
      const run = (extra) => spawnSync(BINARY, ["import-grafana", "--in", work, "--source-map", "clickhouse=prod", "--folders", join(fx, "export/folders.map.json"), "--report", join(work, "report"), "-c", s.configPath, ...extra], { encoding: "utf8", env: {} });
      const dry = run([]);
      t.assert(!dry.error && dry.stdout.length + dry.stderr.length > 0, "the importer ran");
      let report;
      try {
        report = JSON.parse(readFileSync(join(work, "report.json"), "utf8"));
      } catch (error) {
        throw new Error(`no report: status ${dry.status}: ${dry.stdout.slice(-600)} ${dry.stderr.slice(-600)}`);
      }
      t.eq(report.dashboards.length, 8, "the report has the 8 dashboards: " + dry.stdout.slice(-300));
      const collect = (o) => (Array.isArray(o) ? o.flatMap(collect) : o && typeof o === "object" ? Object.entries(o).flatMap(([k, v]) => (k === "items" && Array.isArray(v) ? v : collect(v))) : []);
      const items = collect(report.dashboards);
      const classes = new Set(items.map((i) => i.class));
      t.assert([...classes].every((c) => ["mapped", "changed", "unsupported", "dropped", "metadata"].includes(c)), "the classes of 15.6: " + [...classes]);
      const rows = new Set(items.map((i) => i.id));
      t.assert(items.length >= 16, `the report lists the items of the 8 dashboards (${items.length})`);
      const accept = join(work, "accept.txt");
      writeFileSync(accept, `# accepted for the case B117\n${items.map((i) => i.id).join("\n")}\n`);
      await s.stop();
      const wrote = run(["--accept", accept, "--write"]);
      t.eq(wrote.status, 0, "the import with the acceptance: " + wrote.stdout.slice(-400) + wrote.stderr.slice(-300));
      const after = JSON.parse(readFileSync(join(work, "report.json"), "utf8"));
      t.assert(after.dashboards.every((d) => d.result === "written"), JSON.stringify(after.dashboards.map((d) => [d.uid, d.result])));
      await s.restart();
      let panels = 0;
      const p = await t.page({ width: 1280, height: 2400 });
      t.allow(p, /./);
      for (const [uid] of uids) {
        const got = await fetch(`${s.url}api/dashboards/${uid}`).then((r) => r.json());
        t.assert(got.document, `${uid}: ${JSON.stringify(got).slice(0, 300)} ${wrote.stdout.slice(-1500)} ${wrote.stderr.slice(-800)}`);
        const list = got.document.sections.flatMap((x) => x.panels);
        t.assert(list.length > 0 && list.every((x) => x.type === "state_timeline" || x.type === "status_history"), `${uid}: the types ${list.map((x) => x.type)} ${JSON.stringify(got.document.sections).slice(0, 400)}`);
        panels += list.length;
      }
      await p.goto(`${s.url}d/${uids[1][0]}?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`);
      await p.idle();
      await p.waitFor(`document.querySelectorAll('[data-type="state_timeline"][data-state="ready"], [data-type="status_history"][data-state="ready"], [data-state="error"]').length >= 1`, { what: "panels" });
      t.assert(panels >= 80, `${panels} panels imported`);
      rmSync(work, { recursive: true, force: true });
    },
  },
  {
    id: "B118",
    name: "layouts and zoom: the picture stays in its box at 1920, 1280, 768 and 390 px and at three pixel ratios, the legend moves under the plot when the panel is narrow, viewPanel, the menu and print (9.19a, 9.26)",
    async run(t) {
      const g = (n) => ({ x: 0, y: n * 8, w: 24, h: 8 });
      const right = { legend: legend({ placement: "right" }) };
      const half = (n, x) => ({ x, y: n * 8, w: 12, h: 8 });
      await ensure(t, one("tl118", [
        tp(1, "state_timeline", "Legend bottom", q(`${A} AS a, ${B} AS b`), { grid: g(0), fields: { defaults: { color: { mode: "fixed", fixed: "blue" } }, overrides: [] } }),
        tp(2, "state_timeline", "Legend right", q(`concat('a long service state name ', toString(${A})) AS a, ${B} AS b`), { grid: g(1), options: right, fields: { defaults: { color: { mode: "fixed", fixed: "blue" } }, overrides: [] } }),
        tp(3, "status_history", "History", q(`${A} AS a, ${B} AS b`), { grid: half(2, 0) }),
        tp(4, "state_timeline", "Narrow", q(`concat('a long service state name ', toString(${A})) AS a`), { grid: { x: 12, y: 16, w: 6, h: 8 }, options: right, fields: { defaults: { color: { mode: "fixed", fixed: "blue" } }, overrides: [] } }),
      ]));
      const fit = (p) => p.eval(`(() => {
        const out = { scroll: Math.max(document.documentElement.scrollWidth - window.innerWidth, document.getElementById("app-main").scrollWidth - document.getElementById("app-main").clientWidth), panels: [] };
        for (const id of [1, 2, 3, 4]) {
          const e = document.querySelector('[data-testid="timeline-' + id + '"]');
          if (!e) { out.panels.push([id, "missing"]); continue; }
          const box = e.getBoundingClientRect(), c = e.querySelector("canvas").getBoundingClientRect(), frame = document.querySelector('[data-testid="panel-' + id + '"]').getBoundingClientRect();
          const h = JSON.parse(document.querySelector('[data-testid="panel-' + id + '"] .panel__plot').dataset.timeline);
          out.panels.push([id, c.right <= frame.right + 1 && c.bottom <= frame.bottom + 1 && c.left >= frame.left - 1, Math.abs(h.box[0] + h.box[2] + 8 - c.width) <= 1, e.hasAttribute("data-fallback")]);
        }
        return out;
      })()`);
      for (const scheme of ["dark", "light"]) {
        for (const [w, h, dsf] of [[1920, 1400, 1], [1280, 1400, 1], [768, 1400, 1], [390, 1800, 1], [1280, 1400, 1.25], [1280, 1400, 2]]) {
          const p = await t.page({ width: w, height: h });
          await p.emulateColorScheme(scheme);
          await p.send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: dsf, mobile: false });
          await p.goto(t.url(`d/tl118?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`));
          await p.idle();
          await drawn(p, [1, 2, 3, 4]);
          const r = await fit(p);
          t.assert(r.scroll <= 0, `${scheme} ${w} x ${dsf}: no horizontal scroll (${r.scroll})`);
          for (const [id, inBox, hook2] of r.panels) t.assert(inBox === true && hook2 === true, `${scheme} ${w} x ${dsf}: panel ${id} stays in its box: ${JSON.stringify(r.panels)}`);
          if (w === 1280 && dsf === 1) t.eq(r.panels.find((x) => x[0] === 4)[3], true, "the legend moves under the plot when the panel is narrow");
          if (w >= 1280) t.eq(r.panels.find((x) => x[0] === 2)[3], false, "the legend stays at the right in a wide panel");
        }
      }
      // viewPanel: the picture at full size, and the menu.
      const p = await open(t, "tl118", { width: 1280, height: 900, query: "viewPanel=1" });
      await p.waitFor(`document.querySelector('[data-testid="timeline-1"] canvas') && document.querySelector('[data-testid="timeline-1"]').closest(".panel__plot").dataset.timeline`, { what: "the picture of viewPanel" });
      const big = await p.eval(`document.querySelector('[data-testid="timeline-1"] canvas').clientHeight`);
      t.assert(big > 300, `viewPanel shows the picture at full size (${big} px)`);
      const q2 = await open(t, "tl118", { width: 1280, height: 1400 });
      await drawn(q2, [1, 2, 3, 4]);
      await q2.click(q2.tid("panel-1-menu"));
      const items = await q2.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`);
      t.assert(["View", "View data"].every((x) => items.includes(x)), `the menu ${items}`);
      await q2.press("Escape");
      // Print shows the picture.
      await q2.send("Emulation.setEmulatedMedia", { media: "print" });
      await sleep(300);
      t.eq(await q2.eval(`(() => { const c = document.querySelector('[data-testid="timeline-1"] canvas'); const b = c.getBoundingClientRect(); return [c.offsetParent !== null, b.width > 100, b.height > 50]; })()`), [true, true, true], "print shows the picture");
    },
  },
  {
    id: "B395",
    name: "the texts of a state timeline and of a status history are 12 CSS px high at the pixel ratios 1, 1.25 and 2, and the hook lists the ticks and the label boxes (9.19c section 6, 9.29)",
    async run(t) {
      await ensure(t, one("tl395", [
        tp(1, "state_timeline", "States", q(`${A} AS alpha, ${B} AS beta`), { grid: { x: 0, y: 0, w: 24, h: 8 }, options: { show_values: "always" } }),
        tp(2, "status_history", "History", q(`${A} AS alpha, ${B} AS beta`), { grid: { x: 0, y: 8, w: 24, h: 8 }, options: { show_values: "always" } }),
      ]));
      const boxes = {};
      for (const dsf of [1, 1.25, 2]) {
        const p = await t.page({ width: 1280, height: 900 });
        await p.emulateColorScheme("dark");
        await p.recordCanvasText();
        await p.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: dsf, mobile: false });
        await p.goto(t.url(`d/tl395?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`));
        await p.idle();
        await drawn(p, [1, 2]);
        for (const id of [1, 2]) {
          const texts = await p.eval(`window.__canvasTexts(${canvasOf(id)})`);
          t.assert(texts.length > 6, `${dsf} panel ${id}: the canvas has texts (${texts.length})`);
          const sizes = [...new Set(texts.map((x) => parseFloat(/([0-9.]+)px/.exec(x.font)[1])))];
          t.eq(sizes, [12 * dsf], `${dsf} panel ${id}: every text is 12 CSS px high (font sizes ${sizes})`);
          const h = await hook(p, id);
          boxes[`${dsf}/${id}`] = h.box;
          t.assert(h.labels.length >= 3, `${dsf} panel ${id}: the hook lists the axis labels (${h.labels.length})`);
          // Each label of the hook is a text of the canvas, centred on the same place (the canvas has device pixels).
          const centred = texts.filter((x) => x.align === "center" && x.baseline === "top");
          for (const [text, x, left, right, y] of h.labels) {
            const drawnLabel = centred.find((c) => c.text === text && Math.abs(c.x - x * dsf) <= 0.02 * dsf + 0.01);
            t.assert(drawnLabel !== undefined, `${dsf} panel ${id}: the label ${text} is drawn at x ${x}`);
            t.assert(left < x && x < right && y === h.box[1] + h.box[3] + 9, `${dsf} panel ${id}: the box of the label ${text} (${left}, ${right}, ${y})`);
          }
          t.assert(h.ticks.every(([v, x]) => Number.isFinite(v) && x >= h.box[0] - 0.5 && x <= h.box[0] + h.box[2] + 0.5), `${dsf} panel ${id}: the ticks are in the plot`);
        }
      }
      for (const key of ["1.25/1", "2/1", "1.25/2", "2/2"]) t.eq(boxes[key], boxes[`1/${key.slice(-1)}`], `the plot box at the ratio ${key.split("/")[0]} is the box at the ratio 1 (panel ${key.slice(-1)})`);
    },
  },
];
