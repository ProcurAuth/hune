// The registry of the loading previews (9.11). A panel that waits for its first result shows a preview of its type, drawn only from the JSON
// document of the dashboard (common.js states the rule). This module gives the family of each panel type, builds the preview of a panel with the
// module of its family, and draws the tree of nodes. panel_frame.js uses `buildPreview` and `observePreview`.

import { el } from "../ui.js";
import { HOOKS } from "./common.js";
import * as timeseries from "./timeseries.js";
import * as barchart from "./barchart.js";
import * as piechart from "./piechart.js";
import * as stat from "./stat.js";
import * as gauge from "./gauge.js";
import * as bargauge from "./bargauge.js";
import * as table from "./table.js";
import * as heatmap from "./heatmap.js";
import * as histogram from "./histogram.js";
import * as timeline from "./timeline.js";
import * as logs from "./logs.js";
import * as geomap from "./geomap.js";

const SVG_NS = "http://www.w3.org/2000/svg";
const SVG_TAGS = new Set(["svg", "path", "line", "rect", "g", "use"]);

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

const MODULES = { timeseries, barchart, piechart, stat, gauge, bargauge, table, heatmap, histogram, timeline, logs, geomap };

// The family of a panel type, or "" when the type has no preview.
export function skeletonKind(type) {
  return Object.prototype.hasOwnProperty.call(FAMILY_OF_TYPE, type) ? FAMILY_OF_TYPE[type] : "";
}

// The preview of a panel for a box: { family, elements, bounds, notes }, or null for a type that has none. `box` is { w, h } in px, the content
// box of the body (the size of the loaded plot). `doc` is the dashboard document and `ctx` is { rt, page }. A family that throws gives the
// neutral form: a preview never breaks a panel.
export function previewOf(panel, box, doc = null, ctx = null) {
  const family = panel ? skeletonKind(panel.type) : "";
  if (!family) return null;
  try {
    const out = MODULES[family].build(panel, box, doc, ctx);
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
  for (const child of n.k || []) e.append(drawNode(child));
  return e;
}

// The element `panel__skeleton` of a panel, or null for a panel type that has no preview. It is out of the flow and hidden from assistive
// technology. The CSS shows it for a first load only. It starts empty: `observePreview` fills it when the box has a size.
export function buildPreview(panel) {
  const family = panel ? skeletonKind(panel.type) : "";
  if (!family) return null;
  const root = el("div", { class: `skel skel--${family}`, [HOOKS.ROOT]: family });
  return el("div", { class: "panel__skeleton", "aria-hidden": "true", dataset: { kind: family } }, root);
}

// Fills the preview when its box has a size, and again when the size changes. The preview is hidden (display none) when the panel does not
// wait for a first load, so a panel that never waits builds nothing. The callback `done(result)` is for the tests. Returns a function that stops the
// observation.
export function observePreview(skeleton, panel, doc, ctx, done = null) {
  if (!skeleton || typeof ResizeObserver !== "function") return () => {};
  let last = "";
  const paint = () => {
    const root = skeleton.firstElementChild;
    if (!root) return;
    const w = skeleton.clientWidth;
    const h = skeleton.clientHeight;
    if (!w || !h || `${w}x${h}` === last) return;
    last = `${w}x${h}`;
    const result = previewOf(panel, { w, h }, doc, ctx);
    if (!result) return;
    root.replaceChildren(...result.elements.map(drawNode));
    root.dataset.skelNotes = result.notes.join(" ");
    skeleton._preview = result;
    if (done) done(result);
  };
  const observer = new ResizeObserver(paint);
  observer.observe(skeleton);
  // A text that the document fixes is measured with the font of the page. A font that loads after the first paint changes the width, so the preview
  // builds again when a font has loaded.
  const fonts = typeof document !== "undefined" ? document.fonts : null;
  const again = () => {
    last = "";
    paint();
  };
  if (fonts && typeof fonts.addEventListener === "function") fonts.addEventListener("loadingdone", again);
  return () => {
    observer.disconnect();
    if (fonts && typeof fonts.removeEventListener === "function") fonts.removeEventListener("loadingdone", again);
  };
}
