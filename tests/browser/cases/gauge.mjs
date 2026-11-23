// Browser case B225 (4.33e, 9.19g; wave H, item P12, decision ID-K326): the gauge panel.
// The pure rules of the cells, the arc and the texts are the rules of the classic gauge of Grafana 12.4.2 (FE-84 holds the measures).
// This case proves that the page applies them: for each gauge, the record `data-gauge` equals the rules for the box that the page has
// and the text widths of its font. It reads the pixels of the arcs (the empty arc, the filled arc, the threshold band, the inside), the
// texts on the canvases (the values, the threshold labels, "No data"), the scroll of a column of six gauges, the titles as text nodes
// (a hostile field name), the text alternative, and the light shades of the colours. Three sizes, both themes.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { doc, section } from "../lib/docs.mjs";
import { ROOT } from "../lib/env.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const UID = "b225-gauge";
const HOSTILE = "<img src=x onerror=alert(1)>";
const THR3 = { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 50, color: "orange" }, { value: 80, color: "red" }] };
const reduce = { calc: "last_not_null", fields: null, values: false };
const gauge = (id, title, sql, grid, options = {}, defaults = {}) => ({
  id, type: "gauge", title, description: "", transparent: false, grid,
  data: { query: { source: "prod", format: "table", sql: [sql], min_interval: null, max_data_points: null } },
  transforms: [], fields: { defaults, overrides: [] }, options: { reduce, ...options }, links: [],
});
const g = (x, y, w, h) => ({ x, y, w, h });
const DOC = doc(UID, "Gauge B225", [
  section(null, [
    gauge(1, "g small", "SELECT 42 AS v", g(0, 0, 4, 5), {}, { min: 0, max: 100 }),
    gauge(2, "g medium", "SELECT 42 AS v", g(4, 0, 8, 8), { show_threshold_labels: true }, { min: 0, max: 100, thresholds: THR3 }),
    gauge(3, "g large", "SELECT 63.5 AS v", g(12, 0, 12, 12), { neutral: 30 }, { unit: "percent" }),
    gauge(4, "g grid", "SELECT 10 AS a, 35 AS b, 60 AS c, 85 AS d, 99 AS e", g(0, 12, 12, 8), {}, { min: 0, max: 100 }),
    gauge(5, "g vertical", "SELECT 10 AS a, 55 AS b, 90 AS c", g(12, 12, 12, 6), { orientation: "vertical" }, { min: 0, max: 100 }),
    gauge(6, "g column", "SELECT 10 AS a, 25 AS b, 40 AS c, 55 AS d, 70 AS e, 95 AS f", g(0, 20, 6, 8), { orientation: "horizontal" }, { min: 0, max: 100 }),
    gauge(7, "g empty", "SELECT 1 AS v WHERE 0", g(6, 20, 6, 8)),
    gauge(8, "g hostile", `SELECT 10 AS \`${HOSTILE}\`, 20 AS b`, g(12, 20, 12, 8), { show_threshold_markers: false }, { min: 0, max: 100, color: { mode: "fixed", fixed: "purple" } }),
  ]),
]);
const IDS = [1, 2, 3, 4, 5, 6, 7, 8];
const near = (a, b, tol) => Math.abs(a - b) <= tol;
const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
// A pixel of a colour: opaque (a thin band keeps a little of the edge smoothing, so its alpha is lower) and within `tol` on each channel.
const close = (px, rgb, tol = 3, alpha = 250) => px[3] > alpha && rgb.every((v, i) => Math.abs(v - px[i]) <= tol);

/** The facts of a panel: the box of the root, the record, the canvases (box and texts), the titles, the description and the scroll. */
const factsOf = (p, id) => p.eval(`(() => {
  const root = document.querySelector('[data-testid="gauge-${id}"]');
  if (!root) return null;
  const r = root.getBoundingClientRect();
  const m = document.createElement("canvas").getContext("2d");
  m.font = "400 14px Inter";
  const cells = [...root.querySelectorAll('[data-testid="gauge-item-${id}"]')].map((cell) => {
    const c = cell.querySelector("canvas");
    const t = cell.querySelector(".gauge__title");
    const texts = window.__canvasTexts(c).map((x) => x.text);
    return { texts, w14: texts.length ? m.measureText(texts[texts.length - 1]).width : 0, size: [c.width, c.height], title: t.hidden ? null : t.textContent, titleFont: parseFloat(t.style.fontSize) };
  });
  const desc = document.getElementById(root.getAttribute("aria-describedby"));
  const cs = getComputedStyle(root);
  return {
    box: [Math.floor(r.width * 64 + 1e-6) / 64, Math.floor(r.height * 64 + 1e-6) / 64], record: JSON.parse(root.dataset.gauge || "[]"), cells,
    role: root.getAttribute("role"), label: root.getAttribute("aria-label"), desc: desc ? desc.textContent : null,
    overflowY: cs.overflowY, scroll: [root.scrollHeight, root.clientHeight], imgs: document.querySelector('[data-testid="panel-${id}"]').querySelectorAll("img").length,
  };
})()`);

/** The colour [r, g, b, a] of the canvas of cell `i` of panel `id` at the angle `a` (units of pi) and the radius `rr` from the record. */
const pixel = (p, id, i, a, rr) => p.eval(`(() => {
  const root = document.querySelector('[data-testid="gauge-${id}"]');
  const rec = JSON.parse(root.dataset.gauge)[${i}];
  const c = root.querySelectorAll("canvas")[${i}];
  const k = c.width / rec.canvasW;
  const x = Math.round((rec.cx + ${rr} * Math.cos(${a} * Math.PI)) * k);
  const y = Math.round((rec.cy + ${rr} * Math.sin(${a} * Math.PI)) * k);
  return [...c.getContext("2d").getImageData(x, y, 1, 1).data];
})()`);

async function themeChecks(t, scheme) {
  const G = await import(pathToFileURL(join(ROOT, "web/js/panels/gauge.js")).href);
  const C = await import(pathToFileURL(join(ROOT, "web/js/colors.js")).href);
  const dark = scheme === "dark";
  const p = await t.page({ width: 1600, height: 1400 });
  await p.emulateColorScheme(scheme);
  await p.recordCanvasText();
  await p.goto(t.url(`d/${UID}/x`));
  await p.idle();
  // A query with no row leaves the frame in the state `empty` (9.11); the gauge draws "No data" in it, as a stat does.
  await waitPanelsReady(p, IDS.filter((id) => id !== 7));
  await waitPanelsReady(p, [7], "empty");
  await p.waitFor(`${JSON.stringify(IDS)}.every((id) => { const r = document.querySelector('[data-testid="gauge-' + id + '"]'); return r && r.dataset.gauge; })`, { what: "the gauges drew" });
  await p.eval("document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))");
  const facts = {};
  for (const id of IDS) facts[id] = await factsOf(p, id);

  // 1. The record of each gauge equals the rules of 9.19g for the box of the page and the widths of its font.
  const options = Object.fromEntries(DOC.sections[0].panels.map((x) => [x.id, x.options]));
  for (const id of IDS) {
    const f = facts[id];
    t.assert(f && f.record.length === f.cells.length && f.cells.length > 0, `${scheme} ${id}: one record for each cell: ${JSON.stringify(f && f.record.length)}`);
    const o = options[id];
    const orientation = o.orientation || "auto";
    const n = f.cells.length;
    const [W, H] = f.box;
    const cells = orientation === "auto" ? G.gridCells(n, W, H) : G.stackCells(n, W, H, orientation === "vertical");
    const titled = n > 1;
    f.record.forEach((rec, i) => {
      const parts = G.cellParts({ w: cells[i].w, h: cells[i].h, title: titled, orientation });
      const markers = o.show_threshold_markers !== false;
      const geo = G.gaugeGeometry({ cw: parts.canvasW, ch: parts.canvasH, sizeW: parts.sizeW, sizeH: parts.sizeH, w14: f.cells[i].w14, labels: !!o.show_threshold_labels, markers, bands: rec.bands });
      const tag = `${scheme} panel ${id} gauge ${i}`;
      for (const [k, want] of [["x", cells[i].x], ["y", cells[i].y], ["w", cells[i].w], ["h", cells[i].h], ["canvasW", parts.canvasW], ["canvasH", parts.canvasH], ["cx", geo.cx], ["cy", geo.cy], ["radius", geo.radius], ["arcW", geo.arcW], ["markW", geo.markW], ["valueSize", geo.valueSize], ["labelSize", geo.labelSize]]) {
        t.assert(near(rec[k], want, 0.01), `${tag}: ${k} is ${rec[k]}, the rule gives ${want}`);
      }
      if (titled) t.assert(near(f.cells[i].titleFont, parts.titleSize, 0.01), `${tag}: the title size ${f.cells[i].titleFont} vs ${parts.titleSize}`);
    });
  }
  // The same box as a panel of the measure of Grafana (FE-84) gives the same gauges: the centre and the radius within 0.05 px, the
  // vertical place within 0.5 px and the value size within 3 % (the font file of Hune is not the font file of Grafana, so a text
  // width differs a little).
  const { vectors } = JSON.parse(readFileSync(join(ROOT, "tests/frontend/vectors/FE-84-gauge.json"), "utf8"));
  for (const [id, name] of [[4, "grid5"], [5, "vert3"], [6, "horiz6"]]) {
    const v = vectors.find((x) => x.name === name);
    const f = facts[id];
    if (!(near(f.box[0], v.box[0], 0.5) && near(f.box[1], v.box[1], 0.5))) {
      t.assert(false, `${scheme} panel ${id}: the box ${f.box} is not the box ${v.box} of the Grafana panel ${name}`);
      continue;
    }
    v.gauges.forEach((gr, i) => {
      const rec = f.record[i];
      const tag = `${scheme} panel ${id} gauge ${i} vs Grafana ${name}`;
      t.assert(near(rec.cx, gr.cx, 0.05) && near(rec.radius, gr.radius, 0.05), `${tag}: centre ${rec.cx} radius ${rec.radius} vs ${gr.cx} ${gr.radius}`);
      t.assert(near(rec.cy, gr.cy, 0.5) && Math.abs(rec.valueSize / gr.valueSize - 1) <= 0.03, `${tag}: y ${rec.cy} size ${rec.valueSize} vs ${gr.cy} ${gr.valueSize}`);
    });
  }
  // The three sizes: the arc width follows min(T / 5.5, 40), so the small gauge has a thinner arc than the large one.
  t.assert(facts[1].record[0].arcW < facts[2].record[0].arcW && facts[2].record[0].arcW <= facts[3].record[0].arcW, `${scheme}: the arc widths of the three sizes: ${[1, 2, 3].map((id) => facts[id].record[0].arcW)}`);

  // 2. The pixels of the arcs (panel 1: 42 of 0 to 100, thresholds green and red at 80).
  const r1 = facts[1].record[0];
  const green = hex(C.resolveFill("green", dark));
  const red = hex(C.resolveFill("red", dark));
  const valuePx = await pixel(p, 1, 0, 0.95, r1.radius - r1.arcW / 2);
  t.assert(close(valuePx, green), `${scheme}: the filled arc near the start is green ${green}: ${valuePx}`);
  const trackPx = await pixel(p, 1, 0, 2.0, r1.radius - r1.arcW / 2);
  t.assert(trackPx[3] > 250 && !close(trackPx, green, 20), `${scheme}: the empty arc after the value is the track, not the value colour: ${trackPx}`);
  // The band spans the radius + 1.5 px to the radius + band width + 0.5 px (its line of 1 px included): read its middle.
  const bandStart = await pixel(p, 1, 0, 1.2, r1.radius + 1 + r1.markW / 2);
  const bandEnd = await pixel(p, 1, 0, 2.05, r1.radius + 1 + r1.markW / 2);
  t.assert(close(bandStart, green, 6, 200) && close(bandEnd, red, 6, 200), `${scheme}: the band is green, then red after 80: ${bandStart} ${bandEnd}`);
  const inside = await pixel(p, 1, 0, 1.5, (r1.radius - r1.arcW) * 0.8);
  t.assert(inside[3] === 0, `${scheme}: no paint inside the arc above the text: ${inside}`);
  // Panel 3: the neutral value 30 of a percent field (0 to 100): the arc from 30 to 63.5, and no fill before 30.
  const r3 = facts[3].record[0];
  const before = await pixel(p, 3, 0, 0.9 + 1.2 * 0.15, r3.radius - r3.arcW / 2);
  const between = await pixel(p, 3, 0, 0.9 + 1.2 * 0.5, r3.radius - r3.arcW / 2);
  t.assert(near(r3.min, 0, 0) && near(r3.max, 100, 0), `${scheme}: the scale of a percent field is 0 to 100: ${r3.min} ${r3.max}`);
  t.assert(close(between, hex(C.resolveFill("orange", dark)), 4) || close(between, green, 4), `${scheme}: the arc between the neutral value and the value has the colour of the value: ${between}`);
  t.assert(!close(before, between, 20), `${scheme}: no filled arc between the minimum and the neutral value: ${before} vs ${between}`);
  // Panel 8: the fixed colour has no threshold band.
  t.assert(facts[8].record.every((x) => x.markW === 0 && x.bands === false), `${scheme}: no band with the markers off and a fixed colour`);
  const fixed = await pixel(p, 8, 1, 0.95, facts[8].record[1].radius - facts[8].record[1].arcW / 2);
  t.assert(close(fixed, hex(C.resolveFill("purple", dark)), 4), `${scheme}: the filled arc has the fixed colour: ${fixed}`);

  // 3. The texts on the canvases.
  t.eq(facts[1].cells[0].texts, ["42"], `${scheme}: the value text of panel 1`);
  t.eq([...facts[2].cells[0].texts].sort(), ["0", "100", "42", "50", "80"], `${scheme}: the labels of the ends and of the steps, then the value`);
  t.eq(facts[3].cells[0].texts, ["63.5%"], `${scheme}: the value with its unit`);
  t.eq(facts[7].cells.map((c) => c.texts), [["No data"]], `${scheme}: one gauge "No data" for a query with no row`);
  t.eq(facts[4].cells.map((c) => c.texts.at(-1)), ["10", "35", "60", "85", "99"], `${scheme}: one gauge for each value of the grid`);

  // 4. The cells: a grid of five, a row of three, a column of six that scrolls.
  t.eq(facts[4].record.map((x) => x.y === facts[4].record[0].y), [true, true, true, true, true], `${scheme}: five gauges in one row of the grid`);
  t.assert(facts[5].record.every((x, i) => i === 0 || x.x > facts[5].record[i - 1].x) && facts[5].record.every((x) => x.canvasW <= x.w + 0.01 && x.canvasW < x.w), `${scheme}: the vertical gauges are side by side, each one at the left of its cell`);
  t.assert(facts[6].record.every((x) => near(x.h, 75, 0.01)) && facts[6].overflowY === "auto" && facts[6].scroll[0] > facts[6].scroll[1], `${scheme}: six gauges in a column keep 75 px and the panel scrolls: ${JSON.stringify(facts[6].scroll)}`);

  // 5. The titles and the text alternative: text nodes only.
  t.eq(facts[8].cells.map((c) => c.title), [HOSTILE, "b"], `${scheme}: the hostile name is a text`);
  t.assert(facts[8].imgs === 0, `${scheme}: no image element in the panel`);
  t.assert(facts[8].desc.includes(`${HOSTILE}: 10.`), `${scheme}: the description holds the name as text: ${facts[8].desc}`);
  t.eq(facts[1].cells.map((c) => c.title), [null], `${scheme}: one value with no display name has no title`);
  t.eq([facts[1].role, facts[1].label], ["img", "g small, gauge chart"], `${scheme}: the root is an image with its name`);
  t.eq(facts[2].desc, "v: 42.", `${scheme}: the description of panel 2`);
}

export const cases = [
  {
    id: "B225",
    name: "the gauge: the cells, the arcs, the texts and the colours of the classic gauge of Grafana 12.4.2, at three sizes and in both themes (4.33e, 9.19g)",
    timeout: 200000,
    async run(t) {
      const r = await t.api.create("transit", DOC);
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      await themeChecks(t, "dark");
      await themeChecks(t, "light");
    },
  },
];
