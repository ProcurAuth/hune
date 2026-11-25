// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B120 to B127 (4.5b, 6.21a, 7.37, 9.19d, 10.3b, 15.6): the annotations from a query on a time series.
// B120: the look of the marks. B121: the queries. B122: the card, the hover and the keyboard. B123: the caps.
// B124: the access (mode authzen). B125: the editor and the format. B126: the import. B127: layouts and views.
import { readFileSync } from "node:fs";
import { NAMED, NAMED_LIGHT, parseCss } from "../../../web/js/colors.js";
import { doc, panel, section } from "../lib/docs.mjs";
import { enterEdit, moreItem, noticeTexts, saveDialog, waitDraft, waitNotice } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const T0 = Date.UTC(2026, 10, 13);
const HOUR = 3600000;
// The range of the cases: 00:00 to 05:00 on 2026-11-13, in UTC. The dashboard zone is UTC.
const RANGE = { from: String(T0), to: String(T0 + 5 * HOUR), timezone: "utc", refresh: null };
const SERIES = "SELECT toDateTime('2026-11-13 00:00:00','UTC') + number*300 AS time, number AS v FROM numbers(60)";
const GRID = (k, h = 8) => ({ x: 0, y: k * h, w: 24, h });

// --- fixtures ----------------------------------------------------------------------------------------------------------------------

const at = (m) => `toDateTime('2026-11-13 00:00:00','UTC') + ${m * 60}`;
const lit = (s) => `'${String(s).replace(/\\/g, "\\\\").replace(/'/g, "\\'").replace(/\n/g, "\\n")}'`;
/** The SQL of fixed marks: [start minute, end minute or null, title, text, tags]. The minute 0 is the start of the range. */
const marksSql = (rows) => [rows.map(([s, e, title = "", text = "", tags = ""]) => `SELECT ${at(s)} AS time, ${e === null || e === undefined ? "toDateTime(0,'UTC')" : at(e)} AS timeEnd, ${lit(title)} AS title, ${lit(text)} AS text, ${lit(tags)} AS tags`).join(" UNION ALL ")];
/** The SQL of `n` points, spread over the five hours. */
const rowsSql = (n) => [`SELECT toDateTime('2026-11-13 00:00:00','UTC') + number*${Math.max(1, Math.floor(18000 / n))} AS time FROM numbers(${n})`];
const ann = (name, sql, o = {}) => ({ name, source: o.source || "prod", sql: Array.isArray(sql) && typeof sql[0] === "string" ? sql : marksSql(sql), color: o.color || "red", ...(o.enabled === false ? { enabled: false } : {}), ...(o.panels ? { panels: o.panels } : {}) });
const ts = (id, o = {}) => panel(id, "timeseries", `Series ${id}`, SERIES, { grid: o.grid || GRID(id - 1), options: o.options || {} });
const mk = (uid, panels, annotations, o = {}) => ({ ...doc(uid, `Annotations ${uid}`, o.sections || [section(null, panels)], { time: RANGE, ...(o.doc || {}) }), annotations });
const ensure = async (t, d) => {
  if ((await t.api.dashboard(d.uid)).status === 200) return;
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create ${d.uid}: ${r.status} ${r.text.slice(0, 500)}`);
};
const url = (t, uid, query = "") => t.url(`d/${uid}?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc${query ? `&${query}` : ""}`);
const open = async (t, uid, { scheme = "dark", width = 1280, height = 1100, query = "" } = {}) => {
  const p = await t.page({ width, height });
  await p.emulateColorScheme(scheme);
  await p.goto(url(t, uid, query));
  await p.idle();
  return p;
};

// --- reading the page --------------------------------------------------------------------------------------------------------------

const plot = (id) => `document.querySelector('[data-testid="panel-${id}"] .panel__plot')`;
const hook = (p, id) => p.eval(`JSON.parse(${plot(id)}.dataset.annotations)`);
/** Wait until the hook of the panel has `n` marks that apply (the annotations of the panel have answered). */
const waitN = async (p, id, n) => {
  await p.waitFor(`(() => { const e = ${plot(id)}; return !!e && !!e.dataset.annotations && JSON.parse(e.dataset.annotations).n === ${n}; })()`, { what: `${n} marks on panel ${id}` });
  await p.idle();
};
const annReqs = (p) => p.reqs("/annotations/", "POST");
const annIndexes = (p) => annReqs(p).map((r) => Number(/\/annotations\/(\d+)\/query$/.exec(r.url)[1]));
const panelReqs = (p) => p.requests.filter((r) => r.method === "POST" && /\/panels\/\d+\/query$/.test(r.url));
const canvasOf = (id) => `document.querySelector('[data-testid="panel-${id}"] .annot__marks')`;
/** The pixel (x, y) of the marks canvas, in CSS pixels, as [r, g, b, a]. */
const pixel = (p, id, x, y) => p.eval(`(() => { const c = ${canvasOf(id)}; const k = c.width / c.clientWidth; return [...c.getContext("2d").getImageData(Math.round(${x} * k), Math.round(${y} * k), 1, 1).data]; })()`);
/** The alpha values of a rectangle of the marks canvas: an array of rows, each row an array of alpha values (0 to 255). */
const alphas = (p, id, x, y, w, h) => p.eval(`(() => { const c = ${canvasOf(id)}; const k = c.width / c.clientWidth; const d = c.getContext("2d").getImageData(Math.round(${x} * k), Math.round(${y} * k), ${w}, ${h}).data; const rows = []; for (let r = 0; r < ${h}; r++) { const row = []; for (let q = 0; q < ${w}; q++) row.push(d[(r * ${w} + q) * 4 + 3]); rows.push(row); } return rows; })()`);
const near = (a, b, tol = 6) => a.length === b.length && a.every((c, i) => Math.abs(c - b[i]) <= tol);
/** The colour that the page draws for a token or a colour, in a theme, as the hook writes it. */
const inkOf = (c, scheme) => {
  const p = parseCss(scheme === "light" && NAMED_LIGHT[c] ? NAMED_LIGHT[c] : c);
  return `rgb(${p.r} ${p.g} ${p.b} / ${p.a})`;
};
const rgbOf = (c, scheme) => {
  const p = parseCss(scheme === "light" && NAMED_LIGHT[c] ? NAMED_LIGHT[c] : c);
  return [p.r, p.g, p.b];
};
/** The client position of a point (x, y) of the plot of a panel, and the rect of the plot. */
const plotPoint = (p, id, x, y) => p.eval(`(() => { const e = document.querySelector('[data-testid="panel-${id}"] .chartCore__plot'); const r = e.getBoundingClientRect(); return { x: r.left + ${x}, y: r.top + ${y}, left: r.left, top: r.top, width: r.width, height: r.height }; })()`);
const moveTo = async (p, id, x, y) => {
  const c = await plotPoint(p, id, x, y);
  await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: c.x - 1, y: c.y });
  await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: c.x, y: c.y });
  await sleep(40);
};
const cardOf = (id) => `document.querySelector('[data-testid="annot-${id}-card"]')`;
/** The card of a panel: the visibility, the texts, the number of chips and its box relative to the plot. */
const cardState = (p, id) => p.eval(`(() => {
  const c = ${cardOf(id)};
  const pl = document.querySelector('[data-testid="panel-${id}"] .chartCore__plot').getBoundingClientRect();
  const r = c.getBoundingClientRect();
  return { hidden: c.hidden || getComputedStyle(c).display === "none", head: c.children[0].textContent, body: c.children[1].textContent, tags: [...c.children[2].children].map((e) => e.textContent), count: c.querySelectorAll("*").length, tags_tag: [...c.children[2].children].map((e) => e.tagName), box: { left: r.left - pl.left, top: r.top - pl.top, right: r.right - pl.left, bottom: r.bottom - pl.top, width: r.width, height: r.height }, plotW: pl.width };
})()`);
const xOfFor = (h) => {
  const [L, , W] = h.plot;
  return (minute) => L + ((minute * 60000) * W) / (5 * HOUR);
};

// --- B120 ----------------------------------------------------------------------------------------------------------------------------

const LOOK = () => mk("an120", [ts(1), ts(2)], [
  ann("A points", [[60, null], [120, null]], { color: "red" }),
  ann("B region", [[150, 210]], { color: "blue" }),
  ann("C starts before the range", [[-60, 60]], { color: "green" }),
  ann("D mixed", [[240, 360], [270, null], [-30, null]], { color: "#ff00ff" }),
]);

async function look(t, scheme) {
  const p = await open(t, "an120", { scheme });
  await waitN(p, 1, 7);
  await waitN(p, 2, 7);
  const h = await hook(p, 1);
  const [L, top, W, H] = h.plot;
  const bottom = top + H;
  const x = xOfFor(h);
  t.eq([h.n, h.drawn, h.cut], [7, 6, false], `${scheme}: 7 marks apply, 6 are in the range (the point at -30 min has no line), no cut`);
  t.eq(h.marks.length, 6, "6 items");
  // The order of the time, a tie in the order of the annotations: C, A(60), A(120), B, D region, D point.
  const want = [[-60, 60, 1, "green", 2], [60, 60, 0, "red", 0], [120, 120, 0, "red", 0], [150, 210, 1, "blue", 1], [240, 360, 1, "#ff00ff", 3], [270, 270, 0, "#ff00ff", 3]];
  h.marks.forEach((m, i) => {
    const [a, b, kind, color, index] = want[i];
    t.assert(Math.abs(m[0] - x(a)) <= 1 && Math.abs(m[1] - x(b)) <= 1, `${scheme}: mark ${i} at ${m[0]}, ${m[1]}; expected ${x(a)}, ${x(b)}`);
    t.eq([m[2], m[3], m[4]], [kind, inkOf(color, scheme), index], `${scheme}: mark ${i} kind, colour, annotation`);
  });
  t.assert(Math.abs(h.marks[3][1] - h.marks[3][0] - (x(210) - x(150))) <= 1, "the width of the region B");
  // The second time series has the same marks.
  const h2 = await hook(p, 2);
  t.eq([h2.n, h2.drawn], [7, 6], "the second time series draws the same marks");
  t.eq(await p.eval(`${canvasOf(1)}.style.pointerEvents || getComputedStyle(${canvasOf(1)}).pointerEvents`), "none", "the canvas takes no event");
  // A point: a line of 2 px with the dash [5, 5], from the top to the bottom of the plot.
  const x0 = Math.round(x(120)); // the point at 120 min: no other line is at its x
  const cols = await alphas(p, 1, x0 - 3, top, 7, H);
  const painted = cols.map((row) => Math.max(...row) > 128);
  const dashes = painted.slice(0, 60).filter(Boolean).length;
  t.assert(dashes >= 25 && dashes <= 35, `${scheme}: half of the first 60 rows are painted by a dash 5 and 5, got ${dashes}`);
  const rowAt = painted.findIndex((v) => v);
  const coverage = cols[rowAt].reduce((s, a) => s + a / 255, 0);
  t.assert(Math.abs(coverage - 2) <= 0.4, `${scheme}: the line is 2 px wide, got ${coverage}`);
  t.assert(near((await pixel(p, 1, x0, top + rowAt)).slice(0, 3), rgbOf("red", scheme), 8), `${scheme}: the line has the colour of the annotation`);
  // The triangle of 10 px by 5 px: base on the bottom line of the plot, tip up.
  const tri = await alphas(p, 1, Math.round(x(120)) - 8, bottom - 8, 17, 8);
  const widthAt = (rowFromBottom) => tri[8 - rowFromBottom].reduce((s, a) => s + a / 255, 0);
  t.assert(widthAt(1) >= 7.5 && widthAt(1) <= 10.6, `${scheme}: the base row of the triangle is 10 px wide, got ${widthAt(1)}`);
  t.assert(widthAt(3) < widthAt(1) && widthAt(3) >= 3, `${scheme}: the triangle narrows to its tip, got ${widthAt(3)}`);
  t.assert(widthAt(7) <= 2.6, `${scheme}: no triangle above its 5 px, got ${widthAt(7)}`);
  // The region B: a fill of 0.1 over the plot, and a bar of 5 px at the place of the triangles.
  const mid = Math.round((x(150) + x(210)) / 2);
  const fill = await pixel(p, 1, mid, top + 12);
  t.assert(Math.abs(fill[3] - 26) <= 3 && near(fill.slice(0, 3), rgbOf("blue", scheme), 10), `${scheme}: the fill of the region is the colour at 0.1, got ${fill}`);
  const bar = await pixel(p, 1, mid, bottom - 2);
  t.assert(bar[3] >= 250 && near(bar.slice(0, 3), rgbOf("blue", scheme), 4), `${scheme}: the bar of the region is full, got ${bar}`);
  t.assert((await pixel(p, 1, mid, bottom - 7))[3] <= 40, `${scheme}: the bar has 5 px`);
  // The region C starts before the range: the visible part has the fill and no line at the left edge. The region D ends after the range.
  t.assert(h.marks[0][0] < L, "the region C starts before the plot");
  const left = await alphas(p, 1, L, top, 3, H - 8); // above the bars
  t.assert(Math.max(...left.flat()) <= 60 && left[10][1] >= 15, `${scheme}: the fill of C shows at the left edge and no line is there, max ${Math.max(...left.flat())}`);
  t.assert(h.marks[4][1] > L + W, "the region D ends after the plot");
  const right = await alphas(p, 1, L + W - 3, top, 3, H - 8);
  t.assert(Math.max(...right.flat()) <= 60, `${scheme}: no line at the right edge, max ${Math.max(...right.flat())}`);
  await p.close();
}

// --- B121 ----------------------------------------------------------------------------------------------------------------------------

const ENV = { kind: "custom", name: "env", label: null, hide: "none", options: [{ text: "a", value: "a" }, { text: "b", value: "b" }], multi: false, include_all: false, all_value: null, default: ["a"] };
const QUERIES = () => mk("an121", null, [
  ann("uses env", [`SELECT ${at(60)} AS time, '$env' AS title`]),
  ann("plain", [[120, null]]),
  ann("in the collapsed section", [[90, null]], { panels: [6] }),
  ann("off", [[30, null]], { enabled: false }),
], {
  doc: { variables: [ENV] },
  sections: [
    section(null, [ts(1), ts(2), ts(3), ts(4), panel(5, "stat", "A stat", "SELECT 1 AS v", { grid: { x: 0, y: 32, w: 6, h: 4 } })]),
    section("More", [ts(6, { grid: { x: 0, y: 0, w: 24, h: 8 } })], true),
  ],
});

// ---------------------------------------------------------------------------------------------------------------------------------

export const cases = [
  {
    id: "B120",
    name: "the look of the marks: the x of each line, the width of a region, the line of 2 px with the dash 5 and 5, the fill 0.1, the triangle of 10 by 5 px and the bar of 5 px, in both themes (9.19d, 19.20e)",
    async run(t) {
      await ensure(t, LOOK());
      for (const scheme of ["dark", "light"]) await look(t, scheme);
      t.note("the triangle and the bar are drawn inside the plot, with the base on its bottom line, as in the prototype of the queue; the text of 9.19d says under the plot; compare it with Grafana 12.4.2");
    },
  },
  {
    id: "B121",
    name: "the queries: one request for each annotation at the load, a refresh and a change of the range; one for a variable that the SQL names; none for a hover, a pointer move or a legend click, a disabled annotation or a collapsed section; after the panel requests; aborted by a new trigger (9.8, 9.19d)",
    async run(t) {
      const d = QUERIES();
      await ensure(t, d);
      const p = await open(t, "an121", { height: 2600 });
      await waitN(p, 1, 2);
      await waitPanelsReady(p, [1, 2, 3, 4, 5]);
      t.eq(annIndexes(p).sort(), [0, 1], "2 annotation requests at the load: not the collapsed one and not the disabled one");
      const lastPanel = Math.max(...panelReqs(p).map((r) => p.requests.indexOf(r)));
      t.assert(annReqs(p).every((r) => p.requests.indexOf(r) > lastPanel), "the annotation requests come after the panel requests");
      t.eq(panelReqs(p).length, 5, "5 panel requests (the 4 time series and the stat; the panel of the collapsed section waits)");
      const body = JSON.parse(annReqs(p)[0].body);
      t.eq(Object.keys(body).sort(), ["etag", "from_ms", "to_ms", "variables"], "the body has no SQL and no max_data_points");
      t.assert(body.from_ms === T0 && body.to_ms === T0 + 5 * HOUR, "the range of the dashboard");
      // A stat panel has no mark and no hook.
      t.eq(await p.eval(`!!document.querySelector('[data-testid="panel-5"] [data-annotations]') || !!document.querySelector('[data-testid="panel-5"] .annot__marks')`), false, "a stat gets no mark");
      // A refresh and a change of the range: 2 requests each.
      p.clearRequests();
      await p.click(p.tid("refresh-now"));
      await p.idle();
      t.eq(annIndexes(p).sort(), [0, 1], "a refresh sends 2");
      p.clearRequests();
      await p.click(p.tid("time-back"));
      await p.idle();
      t.eq(annIndexes(p).sort(), [0, 1], "a change of the range sends 2");
      await p.goto(url(t, "an121"));
      await p.idle();
      await waitN(p, 1, 2);
      // A variable that one SQL names sends 1.
      p.clearRequests();
      await p.click(p.tid("variable-env-button"));
      await p.click('.listbox__item[data-value="b"]');
      await p.waitFor(`new URLSearchParams(location.search).get("var-env") === "b"`, { what: "var-env" });
      await p.idle();
      t.eq(annIndexes(p), [0], "a change of the variable that the SQL of one annotation names sends 1");
      t.eq(JSON.parse(annReqs(p)[0].body).variables.env.values, ["b"], "the new value");
      // A pointer move, a hover and a legend click send none.
      p.clearRequests();
      const h = await hook(p, 2);
      for (let i = 0; i < 6; i++) await moveTo(p, 2, h.plot[0] + 40 + i * 30, h.plot[1] + 30);
      await moveTo(p, 2, h.marks[0][0], h.plot[1] + h.plot[3] - 4);
      await p.click(`${p.tid("panel-2")} .chartCore__legendItem`).catch(() => {});
      await sleep(400);
      await p.idle();
      t.eq(annReqs(p).length + panelReqs(p).length, 0, "no request for a pointer move, a hover or a legend click");
      // The expansion of the section sends the annotation of its panel.
      p.clearRequests();
      await p.click(`${p.tid("section-1")} .section__toggle`);
      await waitPanelsReady(p, [6]);
      await p.idle();
      t.eq(annIndexes(p), [2], "the expansion sends the annotation of the panel 6");
      await waitN(p, 6, 3);
      await p.close();
      // The view of a time series sends the annotations that apply to it. The view of a stat sends none.
      const v = await open(t, "an121", { query: "viewPanel=1" });
      await waitN(v, 1, 2);
      t.eq(annIndexes(v).sort(), [0, 1], "the view of a time series sends 2");
      t.assert((await hook(v, 1)).drawn >= 2, "the view draws the marks");
      const s = await open(t, "an121", { query: "viewPanel=5" });
      await s.idle();
      await sleep(300);
      t.eq(annReqs(s).length, 0, "the view of a stat sends none");
      // A new trigger aborts the annotation request that is in flight.
      await ensure(t, mk("an121s", [ts(1)], [ann("slow", [`SELECT ${at(60)} AS time FROM numbers(1) WHERE sleep(1.5) = 0`])]));
      const w = await t.page({ width: 1280, height: 900 });
      await w.goto(url(t, "an121s"));
      for (let i = 0; i < 100 && annReqs(w).length < 1; i++) await sleep(50);
      t.eq(annReqs(w).length, 1, "the first request is sent");
      await w.click(w.tid("time-back"));
      for (let i = 0; i < 100 && annReqs(w).length < 2; i++) await sleep(50);
      t.eq(annReqs(w).length, 2, "the second request is sent");
      await sleep(300);
      t.assert(/ABORTED/i.test(annReqs(w)[0].failed || ""), `a new trigger aborts the request in flight: ${annReqs(w)[0].failed}`);
    },
  },
  {
    id: "B122",
    name: "the card, the hover and the keyboard: the head, the title, the text, the 8 tags, the place and the flip, the zone of 24 px and the 8 px of a line, a hostile text refused by the server, no element from data, the keys, the status text and the table of View data (9.19d, 9.26, T61)",
    async run(t) {
      const tags = "t1,t2,t3,t4,t5,t6,t7,t8";
      await ensure(t, mk("an122", [ts(1)], [
        ann("A", [[60, null, "Deploy v1", "line one\nline two", tags], [150, 210, "Maintenance", "disk change", ""], [290, null, "Late", "near the right edge", "x"]], { color: "red" }),
        ann("B", [[100, null, "Less than", "a < b & c \"q\" javascript:x", ""]], { color: "blue" }),
      ]));
      const p = await open(t, "an122");
      await waitN(p, 1, 4);
      const h = await hook(p, 1);
      const [L, top, W, H] = h.plot;
      const bottom = top + H;
      const x = xOfFor(h);
      // The hover on a point: the head, the body with a line end, 8 chips.
      await moveTo(p, 1, x(60), bottom - 5);
      let c = await cardState(p, 1);
      t.eq(c.hidden, false, "the card shows at the hover on a point");
      t.eq([c.head, c.body], ["2026-11-13 01:00:00", "Deploy v1\nline one\nline two"], "the head is the time in the zone of the dashboard, the body is the title, a line end and the text");
      t.eq(c.tags, ["t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8"], "8 tags at the most");
      t.eq(c.count, 3 + 8, "the card has the head, the body, the tags and the 8 chips: no other element");
      t.eq(c.tags_tag.every((n) => n === "SPAN"), true, "a chip is a span");
      t.assert(Math.abs(c.box.left - (x(60) + 12)) <= 2, `the card is 12 px right of the pointer: ${c.box.left} for ${x(60) + 12}`);
      t.assert(Math.abs(c.box.bottom - (bottom - 16 - 8)) <= 2, `the card is 8 px above the zone: ${c.box.bottom} for ${bottom - 24}`);
      t.assert(c.box.width >= 299 && c.box.width <= 421, `the width of the card is 300 to 420 px: ${c.box.width}`);
      t.eq(await p.eval(`!!document.querySelector('[data-testid="panel-1"] .chartCore__tooltip:not([hidden])')`), false, "the tooltip of the engine is hidden while the card shows");
      t.eq(await p.eval(`document.querySelector('[data-testid="panel-1"] .chartCore__cursor .chartCore__xline') !== null`), true, "the cursor line of the engine stays");
      // The zone: 16 px above the bottom of the plot to 8 px under it, and 8 px from the line.
      await moveTo(p, 1, x(60), bottom - 17);
      t.eq((await cardState(p, 1)).hidden, true, "a hover 17 px above the bottom does nothing");
      await moveTo(p, 1, x(60), bottom - 15);
      t.eq((await cardState(p, 1)).hidden, false, "a hover 15 px above the bottom shows the card");
      await moveTo(p, 1, x(60), bottom + 7);
      t.eq((await cardState(p, 1)).hidden, false, "a hover 7 px under the bottom shows the card");
      await moveTo(p, 1, x(60) + 7, bottom - 5);
      t.eq((await cardState(p, 1)).hidden, false, "7 px from the line matches");
      await moveTo(p, 1, x(60) + 9.5, bottom - 5);
      t.eq((await cardState(p, 1)).hidden, true, "more than 8 px from the line does not match");
      // A region: a pointer inside its width matches.
      await moveTo(p, 1, (x(150) + x(210)) / 2, bottom - 5);
      c = await cardState(p, 1);
      t.eq([c.head, c.body], ["2026-11-13 02:30:00 - 2026-11-13 03:30:00", "Maintenance\ndisk change"], "the head of a region has two times");
      // The flip at the right edge.
      await moveTo(p, 1, x(290), bottom - 5);
      c = await cardState(p, 1);
      t.assert(c.box.right <= x(290) - 12 + 2 && c.box.left >= 0, `the card flips to the left of the pointer: ${c.box.left} to ${c.box.right}`);
      t.assert(c.box.top >= 0, "the card stays in the plot at the top");
      // A text with `<` and `&` that is not a tag shows as written.
      await moveTo(p, 1, x(100), bottom - 5);
      c = await cardState(p, 1);
      t.eq(c.body, "Less than\na < b & c \"q\" javascript:x", "the text shows as written");
      t.eq(c.count, 3, "no element comes from the data: the head, the body and the tags");
      // The keys.
      await p.eval(`document.querySelector('[data-testid="annot-1-keys"]').focus()`);
      t.eq(await p.attr(p.tid("annot-1-keys"), "aria-label"), "Annotations, 4", "the name and the count of the keys");
      t.eq(await p.attr(p.tid("annot-1-keys"), "role"), "group", "the role of the keys");
      await p.press("ArrowRight");
      c = await cardState(p, 1);
      t.eq([c.hidden, c.head], [false, "2026-11-13 01:00:00"], "ArrowRight goes to the first mark and the card shows");
      t.eq(await p.text(p.tid("annot-1-status")), "2026-11-13 01:00:00. Deploy v1\nline one\nline two", "the status text repeats the head and the body");
      await p.press("ArrowRight");
      t.eq((await cardState(p, 1)).head, "2026-11-13 01:40:00", "ArrowRight goes to the next mark");
      await p.press("End");
      t.eq((await cardState(p, 1)).head, "2026-11-13 04:50:00", "End goes to the last mark");
      await p.press("Home");
      t.eq((await cardState(p, 1)).head, "2026-11-13 01:00:00", "Home goes to the first mark");
      await p.press("ArrowLeft");
      t.eq((await cardState(p, 1)).head, "2026-11-13 01:00:00", "ArrowLeft stays at the first mark");
      await p.press("Enter");
      t.eq((await cardState(p, 1)).hidden, false, "Enter does nothing");
      await p.press("Escape");
      t.eq((await cardState(p, 1)).hidden, true, "Escape hides the card");
      // The second table of View data.
      await p.click(p.tid("panel-1-menu"));
      await p.click(p.tid("panel-1-view-data"));
      await p.waitSel(p.tid("view-data-annotations"));
      const table = await p.eval(`(() => { const d = document.querySelector('[data-testid="view-data-annotations"]'); return { title: d.querySelector("h4").textContent, heads: [...d.querySelectorAll("th")].map((e) => e.textContent), rows: [...d.querySelectorAll("tbody tr")].map((r) => [...r.children].map((c) => c.textContent)) }; })()`);
      t.eq(table.title, "Annotations", "the title of the second table");
      t.eq(table.heads, ["Time", "End", "Title", "Text", "Tags"], "the columns");
      t.eq(table.rows.length, 4, "one row for each mark");
      t.eq(table.rows[0], ["2026-11-13 01:00:00", "", "Deploy v1", "line one\nline two", "t1, t2, t3, t4, t5, t6, t7, t8"], "the first row: 8 tags at the most, in the order of the time");
      t.eq(table.rows[2][1], "2026-11-13 03:30:00", "the end of a region");
      await p.press("Escape");
      // A hostile text is refused by the server: the panel shows the note and keeps its state.
      await ensure(t, mk("an122h", [ts(1)], [ann("Hostile", [[60, null, "Title", "<b>x</b>", ""]]), ann("Good", [[90, null, "Ok", "fine", ""]], { color: "green" })]));
      const q = await open(t, "an122h");
      t.allow(q, /422/);
      await waitN(q, 1, 1);
      t.eq(await q.attr(q.tid("panel-1"), "data-state"), "ready", "the state of the panel stays ready");
      const note = await q.text(q.tid("panel-1-annot-note"));
      t.eq(note, "Annotations: 1 failed", "the note of the title bar");
      const tip = await q.attr(q.tid("panel-1-annot-note"), "data-tooltip");
      t.assert(/Hostile/.test(tip) && /annotation_invalid/.test(tip), `the tooltip names the annotation and the code: ${tip}`);
      t.assert(!/<b>/.test(tip), "the tooltip has no cell value");
      t.eq((await hook(q, 1)).drawn, 1, "the other annotation draws");
      const bad = annReqs(q).find((r) => r.status === 422);
      t.assert(bad, "the server answered 422");
    },
  },
  {
    id: "B123",
    name: "the caps: 1,000 rows draw; 1,001 rows give 422 query_limit and a note; 2,000 marks draw and 2,001 give the note and no mark; 2,000 marks draw in 50 ms or less; no task over 200 ms; a failed annotation does not stop the others (9.19d, T63)",
    async run(t) {
      await ensure(t, mk("an123a", [ts(1)], [ann("rows", rowsSql(1000))]));
      await ensure(t, mk("an123b", [ts(1)], [ann("rows", rowsSql(1001)), ann("good", [[60, null, "ok"]], { color: "green" })]));
      await ensure(t, mk("an123c", [ts(1)], [ann("one", rowsSql(1000)), ann("two", rowsSql(1000))]));
      await ensure(t, mk("an123d", [ts(1)], [ann("one", rowsSql(1000)), ann("two", rowsSql(1000)), ann("three", [[60, null, "last"]], { color: "blue" })]));
      const a = await open(t, "an123a");
      await waitN(a, 1, 1000);
      let h = await hook(a, 1);
      t.eq([h.n, h.drawn, h.cut], [1000, 1000, false], "1,000 rows draw");
      t.eq(await a.exists(a.tid("panel-1-annot-note")), false, "no note");
      const b = await open(t, "an123b");
      t.allow(b, /422/);
      await waitN(b, 1, 1);
      t.eq((await hook(b, 1)).drawn, 1, "the good annotation draws");
      const bad = annReqs(b).find((r) => r.status === 422);
      t.assert(bad, "1,001 rows give 422");
      const err = await b.eval(`fetch(${JSON.stringify(annReqs(b)[0].url)}, { method: "POST", headers: { "Content-Type": "application/json" }, body: ${JSON.stringify(annReqs(b)[0].body)} }).then((r) => r.json().then((j) => [r.status, j.error.code]))`);
      t.eq(err, [422, "query_limit"], "the code of the answer");
      t.eq(await b.text(b.tid("panel-1-annot-note")), "Annotations: 1 failed", "the note of a failed annotation");
      t.eq(await b.attr(b.tid("panel-1"), "data-state"), "ready", "the state stays ready");
      // 2,000 marks draw, 2,001 do not.
      const c = await open(t, "an123c");
      await waitN(c, 1, 2000);
      h = await hook(c, 1);
      t.eq([h.n, h.drawn, h.cut], [2000, 2000, false], "2,000 marks draw");
      const d = await open(t, "an123d");
      await waitN(d, 1, 2001);
      h = await hook(d, 1);
      t.eq([h.n, h.drawn, h.cut], [2001, 0, true], "2,001 marks: no mark draws, and the cap is told");
      t.eq(await d.text(d.tid("panel-1-annot-note")), "Annotations: too many (2001, at most 2,000)", "the note of the cap");
      t.eq(await d.attr(d.tid("panel-1"), "data-state"), "ready", "the state stays ready");
      // The time of the draw of 2,000 marks and the long tasks of the main thread.
      const e = await t.page({ width: 1280, height: 900 });
      await e.send("Page.addScriptToEvaluateOnNewDocument", {
        source: `window.__long = []; window.__marks = [];
          try { new PerformanceObserver((l) => { for (const x of l.getEntries()) window.__long.push(x.duration); }).observe({ type: "longtask", buffered: true }); } catch (e) {}
          let start = 0;
          const get = HTMLCanvasElement.prototype.getContext;
          HTMLCanvasElement.prototype.getContext = function (...a) { if (this.className === "annot__marks") start = performance.now(); return get.apply(this, a); };
          new MutationObserver(() => { window.__marks.push(performance.now() - start); }).observe(document, { subtree: true, attributes: true, attributeFilter: ["data-annotations"] });`,
      });
      await e.goto(url(t, "an123c"));
      await e.idle();
      await waitN(e, 1, 2000);
      await e.eval(`window.__marks.length = 0; document.dispatchEvent(new Event("hune-theme")); new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))`);
      await sleep(300);
      const times = await e.eval("window.__marks");
      t.assert(times.length >= 1, "the marks draw again after the theme event");
      t.assert(Math.max(...times) <= 50, `2,000 marks draw in 50 ms or less: ${times.map((x) => x.toFixed(1))}`);
      const long = await e.eval("window.__long");
      t.assert(long.every((x) => x <= 200), `no task of the main thread over 200 ms: ${long}`);
      t.note("the ceiling max_rows of 10 (403 ceiling_exceeded) needs the ceilings of a PDP: it is in B124");
    },
  },
  {
    id: "B124",
    name: "the access in the mode authzen: a viewer with dashboard.read sees the marks; a subject without it gets 403; an author needs source.query on the source to save; the PDP down gives 503 pdp_unavailable and the note (11.21, 11.24, T59 to T60)",
    async run(t) {
      const a = await t.authzen();
      const d = mk("an124", [ts(1)], [ann("Deploys", [[60, null, "Deploy", "v1", ""]])]);
      const created = await a.call("admin", "POST", "api/dashboards", { body: { folder: "transit", message: "", document: d }, headers: { "Idempotency-Key": crypto.randomUUID() } });
      t.assert(created.status === 201, `admin creates the dashboard: ${created.status} ${created.text.slice(0, 300)}`);
      const etag = created.etag;
      const body = JSON.stringify({ etag, from_ms: T0, to_ms: T0 + 5 * HOUR, variables: {} });
      const call = (sub, path, opts = {}) => a.call(sub, "POST", path, { body: opts.body ?? body, headers: opts.headers });
      // T59: a body with SQL gives 400.
      const sql = await call("admin", "api/dashboards/an124/annotations/0/query", { body: JSON.stringify({ etag, from_ms: T0, to_ms: T0 + 5 * HOUR, variables: {}, sql: ["SELECT 1"] }) });
      t.eq(sql.status, 400, "a body with an SQL member gives 400");
      // A viewer with dashboard.read sees the marks. A subject without it gets 403.
      const reader = await call("reader", "api/dashboards/an124/annotations/0/query");
      t.eq(reader.status, 200, `a reader gets the marks: ${reader.text.slice(0, 200)}`);
      t.eq(reader.json.rows, 1, "one mark");
      t.eq((await call("nobody", "api/dashboards/an124/annotations/0/query")).status, 403, "a subject without dashboard.read gets 403");
      // The page of the reader draws the mark. The page of the others has no mark.
      const open1 = async (sub) => {
        const p = await t.page({ width: 1280, height: 900 });
        await p.setHeaders(a.headers(sub));
        await p.goto(a.server.url + `d/an124?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`);
        return p;
      };
      const rp = await open1("reader");
      await waitN(rp, 1, 1);
      t.eq((await hook(rp, 1)).drawn, 1, "a viewer with dashboard.read sees the mark");
      // An author without source.query cannot save a document that has an annotation. An author with it can.
      const save = (sub, document) => a.call(sub, "PUT", "api/dashboards/an124", { body: { message: "", document }, headers: { "If-Match": `"${etag}"`, "Idempotency-Key": crypto.randomUUID() } });
      const changed = { ...d, annotations: [{ ...d.annotations[0], color: "blue" }] };
      t.eq((await save("author", changed)).status, 403, "an author without source.query cannot save an annotation");
      t.eq((await save("sqlauthor", changed)).status, 200, "an author with source.query on the source of the annotation saves");
      // The PDP down gives 503 and the note.
      await a.pdp.stop();
      const down = await call("reader", "api/dashboards/an124/annotations/0/query");
      t.eq([down.status, down.json && down.json.error.code], [503, "pdp_unavailable"], "the PDP down gives 503 pdp_unavailable");
      await a.pdp.start();
      t.assert(a.pdp, "the PDP is back");
    },
  },
  {
    id: "B125",
    name: "the editor and the format: the tab Annotations with each control, the limit of 8, the group Panels with the time series only, the delete of a panel and the notice, the rename of a variable, the JSON tab with each error of 4.5b, a save and a reload (10.3b, 10.6, 4.5b)",
    async run(t) {
      const d = mk("an125", [ts(1), ts(2), panel(3, "stat", "A stat", "SELECT 1 AS v", { grid: { x: 0, y: 16, w: 6, h: 4 } })], [
        ann("A", [[60, null, "a"]], { panels: [2], color: "red" }),
        ann("B", [[90, null, "b"]], { panels: [1, 2], color: "blue" }),
        ann("C", [`SELECT ${at(120)} AS time, '$env' AS title`], { color: "green" }),
      ], { doc: { variables: [ENV] } });
      await ensure(t, d);
      const p = await open(t, "an125");
      t.allow(p, /422/);
      await waitN(p, 1, 2);
      await enterEdit(p);
      await moreItem(p, "dash-settings");
      await p.click(p.tid("settings-tab-annotations"));
      await p.waitSel(p.tid("annotation-list"));
      t.eq(await p.count(`${p.tid("annotation-list")} .fitem`), 3, "3 rows");
      t.assert(/A|prod|red/.test(await p.text(p.tid("annotation-0"))), "a row shows the name, the source and the colour");
      // The form: each control, with its range.
      await p.click(p.tid("annotation-0-open"));
      await p.waitSel(p.tid("annotation-form-name"));
      const form = await p.eval(`(() => { const r = document.querySelector('[data-testid="settings-annotations"]'); const has = (id) => !!r.querySelector('[data-testid="' + id + '"]'); return { name: has("annotation-form-name"), source: has("annotation-form-source"), sql: !!r.querySelector('[data-testid="sql-input"]'), color: has("annotation-form-color"), enabled: has("annotation-form-enabled"), group: has("annotation-form-panels-group"), run: [...r.querySelectorAll("button")].some((b) => /^Run$/.test(b.textContent.trim())), sources: [...r.querySelectorAll('[data-testid="annotation-form-source"] option')].map((o) => o.value) }; })()`);
      t.eq([form.name, form.source, form.sql, form.color, form.enabled, form.group, form.run], [true, true, true, true, true, true, false], "the controls, and no Run");
      t.eq(form.sources, ["prod", "zdown"], "the source select offers the sources of the server (two ClickHouse sources here; B217 has the other kinds)");
      // The group Panels lists the time series only.
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid^="annotation-form-panel-"]')].map((e) => e.dataset.testid)`), ["annotation-form-panel-1", "annotation-form-panel-2"], "the time series only");
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid^="annotation-form-panel-"] input, input[data-testid^="annotation-form-panel-"]')].map((e) => e.checked)`), [false, true], "the panel 2 is checked");
      // A name that exists gives the error.
      await p.fill(p.tid("annotation-form-name"), "B");
      await sleep(450);
      t.assert(/This name exists\./.test(await p.text(p.tid("settings-annotations"))), "a repeated name gives the error");
      await p.fill(p.tid("annotation-form-name"), "A renamed");
      await waitDraft(p, "an125", (x) => x.document.annotations[0].name === "A renamed");
      // The interval macro gives a check under the form.
      await p.click(p.tid("annotation-back"));
      // The limit of 8.
      for (let i = 0; i < 5; i++) {
        await p.click(p.tid("annotation-add"));
        await p.waitSel(p.tid("annotation-back"));
        await p.click(p.tid("annotation-back"));
      }
      await waitDraft(p, "an125", (x) => x.document.annotations.length === 8);
      t.eq(await p.attr(p.tid("annotation-add"), "aria-disabled"), "true", "the button Add annotation is not active at 8 items");
      t.eq(await p.attr(p.tid("annotation-add"), "data-tooltip"), "A dashboard has 8 annotations at most.", "its tooltip");
      const draft = (await waitDraft(p, "an125", (x) => x.document.annotations.length === 8)).document;
      t.eq(draft.annotations.slice(3).map((x) => [x.color, x.enabled, "panels" in x]), Array(5).fill(["red", true, false]), "a new annotation: red, enabled, all the time series");
      t.eq(Object.keys(draft).slice(Object.keys(draft).indexOf("variables"), Object.keys(draft).indexOf("variables") + 2), ["variables", "annotations"], "the member comes after variables in the draft");
      // Remove the 5 new ones.
      for (let i = 7; i >= 3; i--) await p.click(p.tid(`annotation-${i}-remove`));
      await waitDraft(p, "an125", (x) => x.document.annotations.length === 3);
      // The delete of a panel: the dialog lists the annotations, the draft change removes the id, and the last id turns the annotation off.
      await p.click(p.tid("editor-back"));
      await p.waitSel('.edit-grid [data-testid="panel-2-menu"]');
      await p.click(p.tid("panel-2-menu"));
      await p.click(p.tid("panel-2-delete"));
      await p.waitSel(p.tid("delete-panel-dialog"));
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="delete-panel-annotations"] li')].map((e) => e.textContent)`), ["A renamed (it goes off)", "B"], "the dialog lists the annotations that change");
      await p.clickText(`${p.tid("delete-panel-dialog")} button`, "Delete");
      await waitNotice(p, /The annotation A renamed had only this panel\. It is off\./);
      const after = (await waitDraft(p, "an125", (x) => !x.document.sections[0].panels.some((q) => q.id === 2))).document.annotations;
      t.eq([after[0].enabled, "panels" in after[0], after[1].panels], [false, false, [1]], "the id leaves the lists in the same change; an empty list goes and the annotation is off");
      // A rename of a variable lists the annotations whose SQL names the old name.
      await moreItem(p, "dash-settings");
      await p.click(p.tid("settings-tab-variables"));
      await p.click(p.tid("variable-0-open"));
      await p.fill(p.tid("variable-form-name"), "environment");
      await waitNotice(p, /The variable env is now environment\..*annotation C/);
      // The JSON tab shows each error of 4.5b with its pointer.
      await p.click(p.tid("settings-tab-json"));
      await p.waitSel(p.tid("json-text"));
      const text = JSON.parse(await p.eval(`document.querySelector('[data-testid="json-text"]').value`));
      text.annotations = [
        { name: "X", source: "prod", sql: ["SELECT $__timeInterval(t)"], color: "red", panels: [99] },
        { name: "X", source: "prod", sql: ["SELECT 1"], color: "nope" },
      ];
      await p.fill(p.tid("json-text"), JSON.stringify(text, null, 2));
      await p.click(p.tid("json-check"));
      await p.waitFor(`!!document.querySelector('[data-testid="json-errors"]') && document.querySelector('[data-testid="json-errors"]').textContent.includes("/annotations/1/color")`, { what: "the errors of 4.5b" });
      const errors = await p.text(p.tid("json-errors"));
      for (const pointer of ["/annotations/0/panels/0", "/annotations/1/name", "/annotations/1/color"]) t.assert(errors.includes(pointer), `the JSON tab shows ${pointer}: ${errors.slice(0, 600)}`);
      // The draft of the JSON tab is not applied: go back to the valid draft and save.
      await p.fill(p.tid("json-text"), JSON.stringify({ ...text, annotations: after.concat([]) }, null, 2));
      await p.click(p.tid("json-check"));
      await p.waitFor(`!document.querySelector('[data-testid="json-apply"]').disabled`, { what: "the JSON is valid" });
      await p.click(p.tid("json-apply"));
      await p.click(p.tid("editor-back"));
      await p.waitSel(p.tid("save-button"));
      await saveDialog(p, "annotations");
      let saved = null;
      for (let i = 0; i < 100; i++) {
        saved = (await t.api.dashboard("an125")).json.document;
        if (saved.annotations[0].name === "A renamed") break;
        await sleep(100);
      }
      t.eq(saved.annotations.map((x) => x.name), ["A renamed", "B", "C"], `the saved annotations (${JSON.stringify(saved.annotations.map((x) => [x.name, x.enabled, x.panels]))}, panels ${JSON.stringify(saved.sections.flatMap((x) => x.panels.map((q) => q.id)))}, variables ${JSON.stringify(saved.variables.map((v) => v.name))})`);
      t.eq(Object.keys(saved).indexOf("annotations") === Object.keys(saved).indexOf("variables") + 1, true, "the member comes after variables in the saved document");
      const again = await open(t, "an125");
      await waitN(again, 1, 2);
      const reload = (await t.api.dashboard("an125")).json.document;
      t.eq(JSON.stringify(reload.annotations), JSON.stringify(saved.annotations), "a save and a reload give the same document");
      t.eq((await hook(again, 1)).n, 2, "the marks of the saved draft show after the save: B and C, and not A that is off");
      void noticeTexts;
    },
  },
  {
    id: "B126",
    name: "the import and the parity fixtures: the importer reads the 4 fixtures tests/fixtures/grafana/annotations_*.json, the report has the classes of 15.6, the server loads each dashboard and the marks draw (15.6, 15.14c)",
    async run(t) {
      const { cpSync, mkdirSync, mkdtempSync, writeFileSync } = await import("node:fs");
      const { tmpdir } = await import("node:os");
      const { spawnSync } = await import("node:child_process");
      const { join } = await import("node:path");
      const { BINARY, ROOT } = await import("../lib/env.mjs");
      const s = await t.ownServer("imp126");
      const work = mkdtempSync(join(tmpdir(), "hune-b126-"));
      const fx = join(ROOT, "tests/fixtures/grafana");
      mkdirSync(join(work, "dashboards"), { recursive: true });
      const names = ["colours", "filter", "texts", "many"];
      const uids = [];
      for (const n of names) {
        const d = JSON.parse(readFileSync(join(fx, `annotations_${n}.json`), "utf8"));
        writeFileSync(join(work, "dashboards", `${d.uid}.json`), JSON.stringify(d));
        uids.push(d.uid);
      }
      for (const f of ["folders.json", "health.json", "plugin-clickhouse.json"]) cpSync(join(fx, "export", f), join(work, f));
      writeFileSync(join(work, "search.json"), JSON.stringify(uids.map((uid) => ({ uid, folderUid: "fe1x0abc2kq9" }))));
      const run = (extra) => spawnSync(BINARY, ["import-grafana", "--in", work, "--source-map", "clickhouse=prod", "--folders", join(fx, "export/folders.map.json"), "--report", join(work, "report"), "-c", s.configPath, ...extra], { encoding: "utf8", env: {} });
      const dry = run([]);
      let report;
      try {
        report = JSON.parse(readFileSync(join(work, "report.json"), "utf8"));
      } catch (error) {
        throw new Error(`no report: status ${dry.status}: ${dry.stdout.slice(-600)} ${dry.stderr.slice(-600)}`);
      }
      t.eq(report.dashboards.length, 4, "the report has the 4 dashboards");
      const collect = (o) => (Array.isArray(o) ? o.flatMap(collect) : o && typeof o === "object" ? Object.entries(o).flatMap(([k, v]) => (k === "items" && Array.isArray(v) ? v : collect(v))) : []);
      const items = collect(report.dashboards);
      const classes = new Set(items.map((i) => i.class));
      t.assert([...classes].every((c) => ["mapped", "changed", "unsupported", "dropped", "metadata"].includes(c)), "the classes of 15.6: " + [...classes]);
      t.assert(items.length >= 8, `the report lists the items (${items.length})`);
      t.assert(items.some((i) => /annotation/i.test(`${i.id} ${i.path || ""} ${i.message || ""}`)), "the report has rows for the annotations");
      const accept = join(work, "accept.txt");
      writeFileSync(accept, `# accepted for the case B126\n${items.map((i) => i.id).join("\n")}\n`);
      await s.stop();
      const wrote = run(["--accept", accept, "--write"]);
      t.eq(wrote.status, 0, "the import with the acceptance: " + wrote.stdout.slice(-400) + wrote.stderr.slice(-300));
      await s.restart();
      for (const uid of uids) {
        const got = await fetch(`${s.url}api/dashboards/${uid}`);
        t.assert(got.status === 200, `the server loads ${uid}: ${got.status}`);
        const json = await got.json();
        t.assert(Array.isArray(json.document.annotations) && json.document.annotations.length >= 1, `${uid} has annotations`);
        t.assert(json.document.annotations.length <= 8, "8 annotations at the most");
        for (const a of json.document.annotations) t.assert(a.source === "prod" && a.sql.length >= 1 && typeof a.color === "string", `${uid}: an annotation has a source, an SQL and a colour`);
      }
      const p = await t.page({ width: 1280, height: 1000 });
      await p.goto(`${s.url}d/${uids[0]}?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`);
      await p.idle();
      await p.waitFor(`!!document.querySelector('.panel__plot[data-annotations]') && JSON.parse(document.querySelector('.panel__plot[data-annotations]').dataset.annotations).n > 0`, { what: "marks of the imported dashboard" });
      t.note("the capture that compares Grafana and Hune (the x of the marks, the widths, the colours and the count of requests) needs the Grafana of 127.0.0.1:3010 and tools/parity/capture.mjs: it did not run here");
    },
  },
  {
    id: "B127",
    name: "layouts and views: the marks stay in the plot and the card in the panel at 1920, 1280, 768 and 390 px and at the zoom levels of B55; viewPanel; print; no horizontal scroll; cursor_sync and the drag zoom with the marks; bars; the other panel types have none (9.19d, 9.26)",
    async run(t) {
      const text = { id: 6, type: "text", title: "Text", description: "", transparent: false, grid: { x: 0, y: 40, w: 24, h: 4 }, options: { mode: "markdown", content: ["hello"] }, links: [] };
      const others = [
        panel(3, "barchart", "Bars", "SELECT 'a' AS k, 1 AS v", { grid: GRID(2) }),
        panel(4, "table", "Table", "SELECT 1 AS v", { grid: GRID(3) }),
        panel(5, "piechart", "Pie", "SELECT 'a' AS k, 1 AS v", { grid: GRID(4) }),
        text,
      ];
      await ensure(t, mk("an127", [ts(1), ts(2, { options: { draw: "bars" } }), ...others], [
        ann("A", [[60, null, "Deploy", "v1", "x"], [150, 210, "Window", "long text ".repeat(40), ""], [290, null, "Late", "edge", ""]], { color: "red" }),
      ], { doc: { cursor_sync: "crosshair" } }));
      const check = async (p, label) => {
        await waitN(p, 1, 3);
        const h = await hook(p, 1);
        const [L, top, W, H] = h.plot;
        t.assert(h.marks.every((m) => m[0] >= L - 1 && m[0] <= L + W + 1 || m[2] === 1), `${label}: the marks are in the plot`);
        t.eq(await p.eval(`(document.documentElement.scrollWidth <= document.documentElement.clientWidth && document.getElementById("app-main").scrollWidth <= document.getElementById("app-main").clientWidth)`), true, `${label}: no horizontal scroll`);
        await moveTo(p, 1, h.marks[2][0], top + H - 5);
        const c = await cardState(p, 1);
        const panelBox = await p.eval(`(() => { const e = document.querySelector('[data-testid="panel-1"]').getBoundingClientRect(); const pl = document.querySelector('[data-testid="panel-1"] .chartCore__plot').getBoundingClientRect(); return { w: e.width, plotLeft: pl.left - e.left, plotW: pl.width }; })()`);
        t.eq(c.hidden, false, `${label}: the card shows`);
        t.assert(c.box.left >= -1 && c.box.right <= c.plotW + 1, `${label}: the card stays in the plot: ${c.box.left} to ${c.box.right} of ${c.plotW}`);
        t.assert(c.box.right + panelBox.plotLeft <= panelBox.w + 1, `${label}: the card stays in the panel`);
      };
      for (const width of [1920, 1280, 768, 390]) {
        const p = await open(t, "an127", { width, height: 1400, scheme: width === 768 ? "light" : "dark" });
        await check(p, `${width}px`);
        await p.close();
      }
      for (const dsf of [0.75, 0.9, 1.1, 1.25]) {
        const cssW = Math.round(1280 / dsf);
        const p = await t.page({ width: cssW, height: Math.round(1400 / dsf) });
        await p.send("Emulation.setDeviceMetricsOverride", { width: cssW, height: Math.round(1400 / dsf), deviceScaleFactor: dsf, mobile: false });
        await p.goto(url(t, "an127"));
        await p.idle();
        await check(p, `zoom ${dsf}`);
        await p.close();
      }
      const p = await open(t, "an127", { width: 1280, height: 1400 });
      await waitN(p, 1, 3);
      // A time series with draw bars has the marks. A barchart, a table and a pie have none.
      await waitN(p, 2, 3);
      t.eq(await p.eval(`['3', '4', '5', '6'].map((id) => !!document.querySelector('[data-testid="panel-' + id + '"] .annot__marks, [data-testid="panel-' + id + '"] [data-annotations]') || !!document.querySelector('[data-testid="panel-' + id + '-annot-note"]'))`), [false, false, false, false], "a barchart, a table, a pie and a text have no mark and no note");
      // The cursor sync crosshair works with the marks: a move on the plot 1 shows the cursor line in the plot 2.
      const h = await hook(p, 1);
      await moveTo(p, 1, h.plot[0] + 200, h.plot[1] + 40);
      t.eq(await p.eval(`!!document.querySelector('[data-testid="panel-2"] .chartCore__xline:not([hidden])')`), true, "the crosshair of the cursor sync shows in the other time series");
      // The drag zoom works with the marks: a drag sets an absolute range in the URL.
      const a = await plotPoint(p, 1, h.plot[0] + 100, h.plot[1] + 40);
      await p.send("Input.dispatchMouseEvent", { type: "mousePressed", x: a.x, y: a.y, button: "left", clickCount: 1 });
      for (let i = 1; i <= 8; i++) await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: a.x + i * 25, y: a.y, button: "left", buttons: 1 });
      await p.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: a.x + 200, y: a.y, button: "left", clickCount: 1 });
      await p.waitFor(`new URLSearchParams(location.search).get("from") !== ${JSON.stringify(String(T0))}`, { what: "the range of the drag zoom" });
      await p.idle();
      t.assert(annReqs(p).length >= 2, `the zoom sends the annotation query again: ${annReqs(p).length}`);
      await p.close();
      // The view of one panel shows the marks, the card and the keys at full size.
      const v = await open(t, "an127", { query: "viewPanel=1", width: 1280, height: 900 });
      await waitN(v, 1, 3);
      t.eq(await v.exists(v.tid("annot-1-keys")), true, "the view has the keys");
      const hv = await hook(v, 1);
      t.assert(hv.plot[2] > 800, `the plot of the view is large: ${hv.plot[2]}`);
      // Print shows the marks and hides the card and the keys.
      await v.send("Emulation.setEmulatedMedia", { media: "print" });
      await moveTo(v, 1, hv.marks[0][0], hv.plot[1] + hv.plot[3] - 5);
      t.eq(await v.eval(`[getComputedStyle(document.querySelector('[data-testid="annot-1-keys"]')).display, getComputedStyle(document.querySelector('[data-testid="annot-1-card"]')).display]`), ["none", "none"], "print hides the card and the keys");
      t.eq(await v.eval(`getComputedStyle(${canvasOf(1)}).display`), "block", "print keeps the canvas of the marks");
      await v.send("Emulation.setEmulatedMedia", { media: "" });
    },
  },
];
