// SPDX-License-Identifier: MIT OR Apache-2.0
// The measurement harness of the loading previews (9.11). It measures the preview of each panel of a dashboard, then the loaded panel, and tells
// where they differ. The browser cases and the command tests/browser/preview_measure.mjs use it.
//
// The steps:
//   1. `measureDashboard` opens the page, and holds the queries of the panels, so that each panel keeps its preview.
//   2. `previewProbe` reads the preview of each panel: its parts (the elements with `data-skel-part`), their rectangles and their tolerance class.
//   3. The queries are released. When the panels have loaded, `loadedProbe` reads the loaded panel with the extractor of its type.
//   4. `compare` matches the parts of the preview with the parts of the loaded panel and gives one row for each part.
//
// All rectangles are [x, y, w, h] in px, from the top left corner of the content box of the panel body: the corner of the preview. The probes run
// in the page: each one is a pure function with no outside name, because `page.eval` sends its source text.

// The families come from the registry of the previews, so the extractors follow the same names.
import { skeletonKind } from "../../../web/js/preview/index.js";

export const TOLERANCE_PX = 0.5;
export const CLASSES = Object.freeze({ EXACT: "exact", LEAST: "least", MOST: "most", EXTENT: "extent", DATA: "data" });
export const HOLD_QUERIES = (req) => /\/panels\/[^/]+\/query/.test(req.url);

// ---- The recorder of the texts of the canvases ---------------------------------------------------------------------------------------
// The charts draw their axis labels on a canvas, so the DOM does not know them. This script wraps `fillText`: it keeps the text, the place, the font
// and the measured width of each call, for each canvas. `clearRect` from the origin starts a new list, as a new drawing does.
const RECORDER = `(() => {
  if (window.__previewTexts) return;
  const lists = new WeakMap();
  const proto = CanvasRenderingContext2D.prototype;
  const fill = proto.fillText;
  const clear = proto.clearRect;
  proto.fillText = function (text, x, y, ...rest) {
    let list = lists.get(this.canvas);
    if (!list) lists.set(this.canvas, (list = []));
    const m = this.measureText(String(text));
    list.push({ text: String(text), x, y, align: this.textAlign, baseline: this.textBaseline, font: this.font, width: m.width, ascent: m.fontBoundingBoxAscent, descent: m.fontBoundingBoxDescent });
    return fill.call(this, text, x, y, ...rest);
  };
  proto.clearRect = function (x, y, w, h) {
    if (x === 0 && y === 0) lists.set(this.canvas, []);
    return clear.call(this, x, y, w, h);
  };
  window.__previewTexts = (canvas) => lists.get(canvas) || [];
})();`;

/** Install the recorder for the next documents of the page. Call it before `goto`. */
export async function installRecorder(page) {
  await page.send("Page.addScriptToEvaluateOnNewDocument", { source: RECORDER });
}

// ---- The probes (they run in the page) -------------------------------------------------------------------------------------------------

/** Reads the preview of one panel. `id` is the key of the panel. Gives null for an unknown panel. */
function previewProbe(id) {
  const panel = document.querySelector(`[data-testid="panel-${id}"]`);
  if (!panel) return null;
  const out = { id, type: panel.dataset.type || "", state: panel.dataset.state || "", wait: panel.dataset.wait || "" };
  const sk = panel.querySelector(".panel__skeleton");
  if (!sk) return { ...out, family: "", shown: false, parts: [] };
  const style = getComputedStyle(sk);
  const root = sk.firstElementChild;
  const o = sk.getBoundingClientRect();
  const parts = [];
  for (const e of sk.querySelectorAll("[data-skel-part]")) {
    const b = e.getBoundingClientRect();
    parts.push({ part: e.getAttribute("data-skel-part"), cls: e.getAttribute("data-skel-class") || "data", rect: [b.x - o.x, b.y - o.y, b.width, b.height] });
  }
  return {
    ...out,
    family: root ? root.getAttribute("data-skel") || "" : "",
    shown: style.display !== "none",
    box: [sk.clientWidth, sk.clientHeight],
    children: root ? root.childElementCount : 0,
    notes: root ? root.getAttribute("data-skel-notes") : null,
    aria: sk.getAttribute("aria-hidden"),
    sweep: getComputedStyle(sk, "::after").animationName,
    parts,
  };
}

/**
 * Reads one loaded panel. The rectangles use the same corner as the preview. The function does the work that every type needs (the box, the warning,
 * the helpers) and calls the extractor of the family of the panel: `extractors[family](panel, h)`. `h` gives the helpers: `rel`, `add`, `all`,
 * `union`, `textRects`, `visible`, `holder` and `chart` (the part of the engine of the charts, see below).
 */
function loadedProbe(id, family, extractors) {
  const panel = document.querySelector(`[data-testid="panel-${id}"]`);
  if (!panel) return null;
  const type = panel.dataset.type || "";
  const state = panel.dataset.state || "";
  const sk = panel.querySelector(".panel__skeleton");
  const body = panel.querySelector(".panel__body");
  if (!body) return { id, type, state, parts: [] };
  const pad = sk ? parseFloat(getComputedStyle(sk).left) || 0 : 8;
  const b = body.getBoundingClientRect();
  const ox = b.x + pad;
  const oy = b.y + pad;
  const rel = (e) => {
    const r = e.getBoundingClientRect();
    return [r.x - ox, r.y - oy, r.width, r.height];
  };
  const parts = [];
  const add = (part, rect, text) => {
    if (rect && rect[2] > 0 && rect[3] > 0) parts.push(text === undefined ? { part, rect } : { part, rect, text });
  };
  const visible = (e) => !e.hidden && e.getClientRects().length > 0 && !e.closest(".visually-hidden");
  const all = (sel, within = panel) => [...within.querySelectorAll(sel)].filter(visible);
  const union = (rects) => {
    if (!rects.length) return null;
    const x = Math.min(...rects.map((r) => r[0]));
    const y = Math.min(...rects.map((r) => r[1]));
    return [x, y, Math.max(...rects.map((r) => r[0] + r[2])) - x, Math.max(...rects.map((r) => r[1] + r[3])) - y];
  };
  // The text nodes of an element, each with its rectangle. They give the labels of the types that draw their text in the DOM.
  const textRects = (root) => {
    const out = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const text = n.nodeValue.replace(/\s+/g, " ").trim();
      if (!text || !visible(n.parentElement)) continue;
      const range = document.createRange();
      range.selectNodeContents(n);
      const r = range.getBoundingClientRect();
      if (r.width > 0 && r.height > 0) out.push({ text, rect: [r.x - ox, r.y - oy, r.width, r.height] });
    }
    return out;
  };

  add("box", [0, 0, b.width - 2 * pad, b.height - 2 * pad]);
  const warn = panel.querySelector(".panel__warning");
  if (warn && visible(warn)) add("warning", rel(warn));
  const holder = panel.querySelector(".panel__plot");
  if (!holder) return { id, type, state, parts };

  // The engine of the charts (`.chartCore`): the plot is in `data-plot` (left, top, width and height in px from the top left corner of the chart).
  // The labels of the axes are drawn on a canvas, so they come from the recorder of `fillText`. A label at the left of the plot is a y label, and a
  // label under the plot is an x label. Gives true when the panel has this engine.
  const chart = () => {
    const core = holder.querySelector(".chartCore");
    if (!core) return false;
    const c = core.getBoundingClientRect();
    if (core.dataset.plot) {
      const [l, t, w, h] = core.dataset.plot.split(" ").map(Number);
      add("plot", [c.x - ox + l, c.y - oy + t, w, h]);
    }
    const legend = core.querySelector(".chartCore__legend");
    if (legend && visible(legend)) add("legend", rel(legend));
    const canvas = core.querySelector("canvas");
    if (canvas && core.dataset.plot && window.__previewTexts) {
      const [l, t, w, h] = core.dataset.plot.split(" ").map(Number);
      const k = canvas.getBoundingClientRect();
      const ys = [];
      const xs = [];
      for (const r of window.__previewTexts(canvas)) {
        const px = r.align === "right" || r.align === "end" ? r.x - r.width : r.align === "center" ? r.x - r.width / 2 : r.x;
        const top = r.baseline === "middle" ? r.y - (r.ascent + r.descent) / 2 : r.baseline === "top" || r.baseline === "hanging" ? r.y : r.y - r.ascent;
        const rect = [k.x - ox + px, k.y - oy + top, r.width, r.ascent + r.descent];
        const inside = r.x >= l - 1 && r.x <= l + w + 1;
        if (r.x < l - 1 || (r.align === "right" && r.x <= l + 1)) ys.push({ text: r.text, rect });
        else if (r.y > t + h && inside) xs.push({ text: r.text, rect });
      }
      for (const y of ys) add("label", y.rect, y.text);
      for (const x of xs) add("label", x.rect, x.text);
      add("axis-y", union(ys.map((y) => y.rect)));
      add("axis-x", union(xs.map((x) => x.rect)));
    }
    return true;
  };
  const h = { rel, add, all, union, textRects, visible, holder, chart };
  if (extractors[family]) extractors[family](panel, h);
  // A type that draws its text in the DOM gives its labels from the text nodes. A type with an axis on a canvas has them already.
  if (!parts.some((p) => p.part === "label") && !holder.querySelector(".chartCore")) for (const t of textRects(holder)) add("label", t.rect, t.text);
  return { id, type, state, parts };
}

// The extractors of the families: one for each family, in the same names as the registry of web/js/preview/index.js. Each one adds the parts that its
// panels have. An extractor is an arrow function with no outside name, because the harness sends its source text to the page. The family forks
// add the parts of their own family here, and nothing else.
export const EXTRACTORS = {
  // `timeseries` and `xychart`: the engine of the charts.
  timeseries: (panel, h) => {
    h.chart();
  },
  // `barchart`: the engine of the charts for the vertical form, and the elements `hbar__*` for the horizontal form.
  barchart: (panel, h) => {
    if (h.chart()) return;
    const grid = h.holder.querySelector(".hbar__grid");
    if (grid) h.add("plot", h.rel(grid));
    const legend = h.holder.querySelector(".hbar__legend");
    if (legend && h.visible(legend)) h.add("legend", h.rel(legend));
    for (const e of h.all(".hbar__row", h.holder)) h.add("row", h.rel(e));
  },
  piechart: (panel, h) => {
    const plot = h.holder.querySelector(".pie__plot");
    if (plot) h.add("plot", h.rel(plot));
    const legend = h.holder.querySelector(".pie__legend");
    if (legend && h.visible(legend)) h.add("legend", h.rel(legend));
  },
  stat: (panel, h) => {
    for (const e of h.all(".stat__item", h.holder)) h.add("tile", h.rel(e));
  },
  gauge: (panel, h) => {
    for (const e of h.all(".gauge__item", h.holder)) h.add("tile", h.rel(e));
    const canvas = h.holder.querySelector(".gauge canvas");
    if (canvas) h.add("plot", h.rel(canvas));
  },
  bargauge: (panel, h) => {
    for (const e of h.all(".bargauge__item", h.holder)) h.add("tile", h.rel(e));
    for (const e of h.all(".bargauge__track", h.holder)) h.add("track", h.rel(e));
  },
  // `table`: the view, the select bar, the header, the rows, the footer, and the shapes of column 0. A text that the preview draws as a bar has the rectangle of
  // a bar here too: the width of the text, 8 px high, centred on the text. The footer cells are sticky, so the footer is the union of its cells.
  table: (panel, h) => {
    const view = h.holder.querySelector(".vtable__scroll");
    if (view) h.add("plot", h.rel(view));
    // The bar of a text: the place and the width of the text, 8 px high, centred on the box of its element (not on the glyphs, whose centre depends on the font).
    const bar = (root) => {
      const t = root ? h.textRects(root)[0] : null;
      const r = root ? h.rel(root) : null;
      return t ? [t.rect[0], r[1] + r[3] / 2 - 4, t.rect[2], 8] : null;
    };
    const frames = panel.querySelector(".panel__frames");
    if (frames && h.visible(frames)) {
      h.add("frames", h.rel(frames));
      const select = frames.querySelector("select");
      if (select) h.add("select", h.rel(select));
    }
    const head = h.holder.querySelector(".dataTable thead");
    if (head) h.add("head", h.rel(head));
    for (const e of h.all(".dataTable thead th", h.holder)) h.add("column", h.rel(e));
    const th = h.holder.querySelector(".dataTable thead th");
    if (th) {
      h.add("head-label", bar(th.querySelector(".vtable__label")));
      const group = th.querySelector(".dataTable__sort");
      if (group && th.hasAttribute("aria-sort") && th.getAttribute("aria-sort") !== "none") {
        const g = h.rel(group);
        h.add("mark", [g[0] + g[2] - 18, g[1] + (g[3] - 18) / 2, 18, 18]);
      }
      const filter = th.querySelector(".vtable__filter");
      if (filter) h.add("filter", h.rel(filter));
    }
    for (const e of h.all(".dataTable tbody tr[data-row]", h.holder)) h.add("row", h.rel(e));
    const foots = h.all(".dataTable tfoot td", h.holder).map((e) => h.rel(e));
    h.add("foot", h.union(foots));
    for (const e of h.all(".dataTable tfoot td:first-child .vtable__foot-name", h.holder)) h.add("foot-name", bar(e));
    const first = h.holder.querySelector(".dataTable tbody tr[data-row] td");
    if (first) {
      for (const e of h.all(".cell-gauge__bar:not(.cell-gauge__bar--lcd)", first)) h.add("track", h.rel(e));
      for (const e of h.all(".cell-gauge__cell", first)) h.add("cell", h.rel(e));
    }
  },
  heatmap: (panel, h) => {
    h.chart();
    const legend = h.holder.querySelector(".heat__legend");
    if (legend && h.visible(legend)) h.add("legend", h.rel(legend));
    for (const e of h.all(".heat__bar", h.holder)) h.add("scale", h.rel(e));
  },
  histogram: (panel, h) => {
    h.chart();
  },
  // `state_timeline` and `status_history`: a canvas and a legend of their own.
  timeline: (panel, h) => {
    const canvas = h.holder.querySelector(".timeline__canvas");
    if (canvas) h.add("plot", h.rel(canvas));
    const legend = h.holder.querySelector(".timeline__legend");
    if (legend && h.visible(legend)) h.add("legend", h.rel(legend));
  },
  // `logs`: the list, the lines, and the bars of the first line. The time bar is the time box of the first line and the track is the region from the level
  // cell to the end of the line. Each one is 8 px high, centred in the text band of 22 px at the top of the line (the line may be higher with `wrap`).
  logs: (panel, h) => {
    const list = h.holder.querySelector(".logs");
    if (list) h.add("plot", h.rel(list));
    for (const e of h.all(".logs__line", h.holder)) h.add("row", h.rel(e));
    const line = h.all(".logs__line", h.holder)[0];
    if (!line) return;
    const ln = h.rel(line);
    const time = line.querySelector(".logs__time");
    if (time) {
      const t = h.rel(time);
      h.add("time", [t[0], ln[1] + 7, t[2], 8]);
    }
    const level = line.querySelector(".logs__level");
    if (level) {
      const lv = h.rel(level);
      h.add("track", [lv[0], ln[1] + 7, ln[0] + ln[2] - lv[0], 8]);
    }
  },
  geomap: (panel, h) => {
    const canvas = h.holder.querySelector(".geo__canvas");
    if (canvas) h.add("plot", h.rel(canvas));
  },
};

// ---- The comparison --------------------------------------------------------------------------------------------------------------------

const near = (a, b, tol) => Math.abs(a - b) <= tol;

/** Compares a rectangle of the preview with the rectangle of the loaded panel by the tolerance class. Gives { verdict, delta }. */
export function compareRect(cls, preview, loaded, tol = TOLERANCE_PX) {
  const delta = preview.map((v, i) => Number((v - loaded[i]).toFixed(2)));
  if (cls === CLASSES.EXACT) return { verdict: delta.every((d) => Math.abs(d) <= tol) ? "ok" : "fail", delta };
  // A one-sided bound is a bound on the size. The preview takes at most (least) or at least (most) the room of the loaded panel.
  if (cls === CLASSES.LEAST) return { verdict: preview[2] <= loaded[2] + tol && preview[3] <= loaded[3] + tol ? "ok" : "fail", delta };
  // An extent is a shape whose right edge is a lower bound (a table is at least as wide as the document proves): left, top and height are exact, the width is a least bound.
  if (cls === CLASSES.EXTENT) return { verdict: delta[0] <= tol && delta[0] >= -tol && Math.abs(delta[1]) <= tol && Math.abs(delta[3]) <= tol && preview[2] <= loaded[2] + tol ? "ok" : "fail", delta };
  if (cls === CLASSES.MOST) return { verdict: preview[2] >= loaded[2] - tol && preview[3] >= loaded[3] - tol ? "ok" : "fail", delta };
  return { verdict: "data", delta };
}

const key = (p, i) => `${p}#${i}`;
const indexed = (parts) => {
  const seen = new Map();
  return parts.map((p) => {
    const i = seen.get(p.part) || 0;
    seen.set(p.part, i + 1);
    return { ...p, index: i };
  });
};

/**
 * Matches the parts of one panel by name and by order, and gives one row for each part. A part of the preview with no loaded part has the verdict
 * `no-loaded`. A part of the loaded panel that the preview does not claim has the verdict `unclaimed` and shows the target of the families.
 */
export function compare(preview, loaded, tol = TOLERANCE_PX) {
  const rows = [];
  const base = { id: preview ? preview.id : loaded.id, type: preview ? preview.type : loaded.type, family: preview ? preview.family : "" };
  const p = indexed(preview ? preview.parts : []);
  const l = indexed(loaded ? loaded.parts : []);
  const loadedBy = new Map(l.map((x) => [key(x.part, x.index), x]));
  const used = new Set();
  for (const x of p) {
    const k = key(x.part, x.index);
    const y = loadedBy.get(k);
    if (y) used.add(k);
    if (!y) rows.push({ ...base, part: x.part, index: x.index, cls: x.cls, preview: x.rect, loaded: null, delta: null, verdict: x.cls === CLASSES.DATA ? "data" : "no-loaded" });
    else rows.push({ ...base, part: x.part, index: x.index, cls: x.cls, preview: x.rect, loaded: y.rect, text: y.text, ...compareRect(x.cls, x.rect, y.rect, tol) });
  }
  for (const y of l) if (!used.has(key(y.part, y.index))) rows.push({ ...base, part: y.part, index: y.index, cls: null, preview: null, loaded: y.rect, text: y.text, delta: null, verdict: "unclaimed" });
  return rows;
}

/** The count of each verdict in a list of rows. */
export function summarize(rows) {
  const out = { ok: 0, fail: 0, data: 0, "no-loaded": 0, unclaimed: 0 };
  for (const r of rows) out[r.verdict] = (out[r.verdict] || 0) + 1;
  return out;
}

const fmt = (r) => (r ? `[${r.map((n) => Number(n.toFixed(1))).join(", ")}]` : "-");

/** The rows as a text table, one line for each row. `all` keeps the lines of the unclaimed parts and of the labels. */
export function formatTable(rows, { all = false } = {}) {
  const shown = rows.filter((r) => all || (r.verdict !== "unclaimed" && r.verdict !== "data") || r.part !== "label");
  const head = ["panel", "type", "part", "#", "class", "verdict", "preview", "loaded", "delta"];
  const lines = shown.map((r) => [String(r.id), r.type, r.part, String(r.index), r.cls || "-", r.verdict, fmt(r.preview), fmt(r.loaded), r.delta ? fmt(r.delta) : "-"]);
  const width = head.map((h, i) => Math.max(h.length, ...lines.map((l) => l[i].length)));
  const row = (cells) => cells.map((c, i) => c.padEnd(width[i])).join("  ").trimEnd();
  return [row(head), row(width.map((w) => "-".repeat(w))), ...lines.map(row)].join("\n");
}

// ---- The run ---------------------------------------------------------------------------------------------------------------------------

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const call = (fn, arg) => `(${fn.toString()})(${JSON.stringify(arg)})`;

/** Reads the preview of one panel in the page. */
export const readPreview = (page, id) => page.eval(call(previewProbe, id));
/** Reads one loaded panel in the page, with the extractor of its family. */
export function readLoaded(page, id, type) {
  const extractors = `{ ${Object.entries(EXTRACTORS).map(([family, fn]) => `${JSON.stringify(family)}: ${fn.toString()}`).join(", ")} }`;
  return page.eval(`(${loadedProbe.toString()})(${JSON.stringify(id)}, ${JSON.stringify(skeletonKind(type))}, ${extractors})`);
}

/**
 * Measures the previews and the loaded panels of a dashboard page. `page` is a fresh page of the test client (tests/browser/lib/cdp.mjs). `url` is the
 * address of the dashboard. Options: `ids` (the panels to measure, default all), `width`, `height` (the viewport: a tall one shows every panel, because
 * the page loads the panels near the viewport only), `settle` (ms after the previews show), `drawSettle` (ms after the loaded panels are ready), `tol`,
 * `coarse` (a touch screen).
 * Gives { previews, loaded, rows, summary }.
 */
export async function measureDashboard(page, url, { ids = null, width = 1440, height = 4000, settle = 600, drawSettle = 900, tol = TOLERANCE_PX, timeout = 60000, coarse = false } = {}) {
  await page.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: coarse });
  // `coarse` emulates a touch screen: the media query `(pointer: coarse)` is true.
  if (coarse) await page.send("Emulation.setTouchEmulationEnabled", { enabled: true, maxTouchPoints: 1 });
  await installRecorder(page);
  const hold = page.holdWhen(HOLD_QUERIES);
  await page.goto(url);
  const panelIds = () => page.eval(`[...document.querySelectorAll('[data-testid^="panel-"][data-type]')].map((e) => e.getAttribute("data-testid").slice(6))`);
  await page.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-type]').length > 0`, { what: "the panels", timeout });
  await sleep(settle);
  const all = ids || (await panelIds());
  const previews = {};
  for (const id of all) previews[id] = await readPreview(page, id);
  hold.release();
  const idle = `${JSON.stringify(all)}.every((id) => { const p = document.querySelector('[data-testid="panel-' + id + '"]'); return !p || !["idle", "queued", "loading"].includes(p.dataset.state); })`;
  await page.waitFor(idle, { what: "the loaded panels", timeout });
  await sleep(drawSettle);
  const loaded = {};
  const rows = [];
  for (const id of all) {
    loaded[id] = await readLoaded(page, id, previews[id] ? previews[id].type : "");
    rows.push(...compare(previews[id], loaded[id], tol));
  }
  return { previews, loaded, rows, summary: summarize(rows) };
}
