// The registry of the loading previews (9.11). A panel that waits for its first result shows a preview of its type, drawn only from the JSON
// document of the dashboard (common.js states the rule). This module gives the family of each panel type, builds the preview of a panel with the
// module of its family, and draws the tree of nodes. panel_frame.js uses `buildPreview` and `observePreview`.

import { el } from "../ui.js";
import { HOOKS } from "./common.js";

const SVG_NS = "http://www.w3.org/2000/svg";
const SVG_TAGS = new Set(["svg", "path", "line", "rect", "g", "use", "defs", "linearGradient", "stop"]);

// The family of each panel type. A panel type that draws no queried data (a text) has no family and no preview.
const FAMILY_OF_TYPE = {
  timeseries: "timeseries",
  xychart: "timeseries",
  barchart: "barchart",
  piechart: "piechart",
  stat: "stat",
  gauge: "gauge",
  bargauge: "bargauge",
  table: "table",
  heatmap: "heatmap",
  histogram: "histogram",
  state_timeline: "timeline",
  status_history: "timeline",
  logs: "logs",
  geomap: "geomap",
};

// The module of each family loads on demand, so a page that has no panel of a family never loads its module. Each path is a quoted literal, so the
// measure of the page budget counts the module (9.30).
const FAMILY_IMPORTS = {
  timeseries: () => import("./timeseries.js"),
  barchart: () => import("./barchart.js"),
  piechart: () => import("./piechart.js"),
  stat: () => import("./stat.js"),
  gauge: () => import("./gauge.js"),
  bargauge: () => import("./bargauge.js"),
  table: () => import("./table.js"),
  heatmap: () => import("./heatmap.js"),
  histogram: () => import("./histogram.js"),
  timeline: () => import("./timeline.js"),
  logs: () => import("./logs.js"),
  geomap: () => import("./geomap.js"),
};
const LOADED = {};
const PENDING = {};

// The promise of the module of a family. One import runs for each family. A failed import is forgotten, so the next call tries again.
export function loadFamily(family) {
  if (!PENDING[family]) {
    PENDING[family] = FAMILY_IMPORTS[family]().then((mod) => (LOADED[family] = mod), (error) => {
      delete PENDING[family];
      throw error;
    });
  }
  return PENDING[family];
}

// Loads the modules of all the families. `previewOf` needs the module of the family: a test waits for this promise before it calls `previewOf`.
export function loadPreviewFamilies() {
  return Promise.all(Object.keys(FAMILY_IMPORTS).map(loadFamily));
}

// The family of a panel type, or "" when the type has no preview.
export function skeletonKind(type) {
  return Object.prototype.hasOwnProperty.call(FAMILY_OF_TYPE, type) ? FAMILY_OF_TYPE[type] : "";
}

// The preview of a panel for a box: { family, elements, bounds, notes }, or null for a type that has none. `box` is { w, h } in px, the content
// box of the body (the size of the loaded plot). `doc` is the dashboard document and `ctx` is { rt, page }. A family that throws gives the
// neutral form: a preview never breaks a panel. It is synchronous, so it gives null while the module of the family is not loaded.
export function previewOf(panel, box, doc = null, ctx = null) {
  const family = panel ? skeletonKind(panel.type) : "";
  if (!family || !LOADED[family]) return null;
  try {
    const out = LOADED[family].build(panel, box, doc, ctx);
    return { family, elements: out.elements || [], bounds: out.bounds || {}, notes: out.notes || [] };
  } catch (error) {
    return { family, elements: [], bounds: {}, notes: ["error"] };
  }
}

// Makes the element of one node and its children. The custom properties are set with the CSSOM: the page never writes a style attribute in its
// markup (9.29). SVG nodes use the SVG namespace. A child that is a string is a text node.
export function drawNode(n) {
  if (typeof n === "string") return document.createTextNode(n);
  const e = SVG_TAGS.has(n.t) ? document.createElementNS(SVG_NS, n.t) : document.createElement(n.t);
  if (n.c) e.setAttribute("class", n.c);
  for (const [name, value] of Object.entries(n.v || {})) e.style.setProperty(name, value);
  for (const [name, value] of Object.entries(n.a || {})) e.setAttribute(name, value);
  // A node may carry a text `x`. A preview draws a text only to let the browser measure it: the CSS makes it transparent.
  if (typeof n.x === "string") e.append(document.createTextNode(n.x));
  for (const child of n.k || []) e.append(drawNode(child));
  return e;
}

// The element `panel__skeleton` of a panel, or null for a panel type that has no preview. It is out of the flow and hidden from assistive
// technology. The CSS shows it for a first load only. It starts empty: `observePreview` fills it when the box has a size. The import of the
// module of the family starts here, so the module is there when the preview shows. A failed import leaves the panel with no preview.
export function buildPreview(panel) {
  const family = panel ? skeletonKind(panel.type) : "";
  if (!family) return null;
  loadFamily(family).catch(() => {});
  const root = el("div", { class: `skel skel--${family}`, [HOOKS.ROOT]: family });
  return el("div", { class: "panel__skeleton", "aria-hidden": "true", dataset: { kind: family } }, root);
}

// Fills the preview when its box has a size, and again when the window changes size. The panel calls `measure` when the preview shows. The preview is hidden (display none) when the panel does not
// wait for a first load, so a panel that never waits builds nothing. The callback `done(result)` is for the tests. `ctx` is { rt, page, key }: `key` is
// the key of the panel instance, for the range of the request (`rt.rangeOf(key)`). Returns a function that stops the observation.
export function observePreview(skeleton, panel, doc, ctx, done = null) {
  if (!skeleton) return () => {};
  let last = "";
  let waiting = false;
  let stopped = false;
  const family = skeletonKind(panel.type);
  // The module of the family is not there yet: the preview draws when it comes, with the size that the box has then.
  const paintWhenLoaded = () => {
    if (waiting) return;
    waiting = true;
    const end = () => {
      waiting = false;
    };
    loadFamily(family).then(() => {
      end();
      if (!stopped) paint();
    }, end);
  };
  const paint = () => {
    const root = skeleton.firstElementChild;
    if (!root) return;
    const w = skeleton.clientWidth;
    const h = skeleton.clientHeight;
    if (!w || !h || `${w}x${h}` === last) return;
    if (!LOADED[family]) return paintWhenLoaded();
    last = `${w}x${h}`;
    // `fw` and `fh` are the size with its fraction, rounded down to the layout unit (1/64 px): a gauge reads the real size of the box.
    const r = skeleton.getBoundingClientRect();
    const result = previewOf(panel, { w, h, fw: Math.floor(r.width * 64 + 1e-6) / 64, fh: Math.floor(r.height * 64 + 1e-6) / 64 }, doc, ctx);
    if (!result) return;
    root.replaceChildren(...result.elements.map(drawNode));
    root.dataset.skelNotes = result.notes.join(" ");
    skeleton._preview = result;
    if (done) done(result);
  };
  // The page reads the size when the preview shows and when the window changes size. It has no ResizeObserver here: an observer on the page of a big
  // text that a person types forces a layout of the page at each key (B176).
  const measure = () => paint();
  const host = typeof window !== "undefined" && typeof window.addEventListener === "function" ? window : null;
  if (host) host.addEventListener("resize", measure);
  // A text that the document fixes is measured with the font of the page. A font that loads after the first paint changes the width, so the measures are read
  // again and the preview builds again when a font has loaded (9.11).
  const fonts = typeof document !== "undefined" ? document.fonts : null;
  const again = () => {
    if (LOADED.timeseries) LOADED.timeseries.resetMeasures();
    last = "";
    paint();
  };
  if (fonts && typeof fonts.addEventListener === "function") fonts.addEventListener("loadingdone", again);
  const stop = () => {
    stopped = true;
    if (host) host.removeEventListener("resize", measure);
    if (fonts && typeof fonts.removeEventListener === "function") fonts.removeEventListener("loadingdone", again);
  };
  stop.measure = measure;
  return stop;
}
