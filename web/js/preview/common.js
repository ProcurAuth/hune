// The shared helpers of the loading previews (9.11). Every family module (web/js/preview/<family>.js) builds its preview with them.
//
// The rule of a preview: it uses ONLY the JSON document of the dashboard (the members of the panel, its options, its fields, its overrides, its
// grid). It never reads the text of a query, and it has no heuristic: no typical value, no guessed count, no guessed width. Where the document
// does not fix a size, an edge or a count, the preview draws the LEAST form that the document proves, drawn empty, or nothing.
//
// A preview is a tree of nodes { t, c, v, a, k }: the tag (`div`, `svg`, `path`, `line`, `rect`), the class, the custom properties (set with the
// CSSOM, never in a style attribute: 9.29), the attributes and the children. index.js draws the tree. Everything in this module is pure,
// except `textWidth`, which needs a canvas (a test gives its own measurer with `setMeasurer`).

// ---- The hooks of the tests --------------------------------------------------------------------------------------------------------
// The test harness (tests/browser/lib/preview_measure.mjs) finds the parts of a preview by these attributes.
//   data-skel        on the root `.skel`: the key of the family ("timeseries", "table", ...).
//   data-skel-part   on each element that a test measures: one of PARTS.
//   data-skel-class  on the same element: how exact the place and the size are (EXACT, LEAST, MOST or DATA).
export const HOOKS = Object.freeze({ ROOT: "data-skel", PART: "data-skel-part", CLASS: "data-skel-class" });

// The names of the measured parts. A family uses the names that fit and may add its own, listed in its file header and in docs/04 9.11.
export const PARTS = Object.freeze({
  BOX: "box", // the body of the panel that the preview covers
  PLOT: "plot", // the plot of a chart, the area of a table body, the body of logs
  LEGEND: "legend",
  WARNING: "warning", // the warning line above a chart
  AXIS_Y: "axis-y", // the column of the y labels
  AXIS_X: "axis-x", // the row of the x labels
  LABEL: "label", // one bar that stands for a text that the document fixes
  HEAD: "head", // the head row of a table or of a legend table
  ROW: "row",
  COLUMN: "column",
  TILE: "tile", // one item of a stat, a gauge or a bar gauge
  ARC: "arc",
  TRACK: "track",
  SCALE: "scale", // a colour scale
  MARK: "mark", // a decorative form
});

// How exact a part is, against the loaded panel (the tolerance classes of the council files):
//   EXACT  the document fixes it: the same place and size, within TOLERANCE_PX.
//   LEAST  the document gives a lower bound: the preview takes the least room, so its value is at most the loaded value.
//   MOST   the document gives an upper bound: the preview value is at least the loaded value.
//   DATA   the document gives nothing: no shape stands for it, and the harness does not compare it.
export const EXACT = "exact";
export const LEAST = "least";
export const MOST = "most";
export const DATA = "data";
//   CONTAIN  the document bounds the edges of a rectangle (the plot of a chart): the preview rectangle holds the loaded one. It is written
//            `contain:<edges>`, with the letters l, t, r and b for the edges that the rule claims (left, top, right, bottom). Left and top: the
//            preview edge is at the same place or before the loaded edge, and the top is the same. Right and bottom: the same place or further out.
export const CONTAIN = "contain";

export const TOLERANCE_PX = 0.5;
// A bar that stands for a text of known width: 8 px high, rounded (the council files). Its width is the measured width of the text.
export const BAR_HEIGHT = 8;
export const BAR_RADIUS = 4;
// The size of the text of the charts and of the tables: the engine draws at 12 px (app_chart_core.js FONT_SIZE).
export const FONT_SIZE = 12;

// ---- Nodes -------------------------------------------------------------------------------------------------------------------------
export const node = (c, v = {}, k = [], t = "div", a = {}) => ({ t, c, v, a, k });

// A px value as a text. A px value is never rounded to a tenth (the engine reads integers and the council rules say where to round).
export const px = (n) => `${Number(Number(n).toFixed(3))}px`;

// The attributes of a measured part.
export const partAttrs = (part, cls = DATA) => ({ [HOOKS.PART]: part, [HOOKS.CLASS]: cls });

// A region of the box, drawn empty: the least form of a part whose edge the document proves. `r` is { x, y, w, h } in px from the top left corner
// of the preview.
export const regionNode = (r, part, cls = LEAST, c = "skel__r") => node(c, { "--x": px(r.x), "--y": px(r.y), "--w": px(r.w), "--h": px(r.h) }, [], "div", partAttrs(part, cls));

// A rounded bar that stands for a text of known width, at (x, y) from the top left corner of the preview. The height is BAR_HEIGHT.
export const barNode = (x, y, w, part = PARTS.LABEL, cls = EXACT) => node("skel__b", { "--x": px(x), "--y": px(y), "--w": px(w), "--h": px(BAR_HEIGHT) }, [], "div", partAttrs(part, cls));

// SVG helpers. The SVG fills the box, in px: the viewBox is the box.
export const svgNode = (w, h, k = [], c = "skel__svg") => node(c, {}, k, "svg", { viewBox: `0 0 ${Number(w)} ${Number(h)}`, preserveAspectRatio: "none" });
export const pathNode = (d, c = "skel__path") => node(c, {}, [], "path", { d });
export const lineNode = (x1, y1, x2, y2, c = "skel__line") => node(c, {}, [], "line", { x1: String(x1), y1: String(y1), x2: String(x2), y2: String(y2) });

// ---- Numbers -----------------------------------------------------------------------------------------------------------------------
// The engine of the charts reads integers (offsetHeight, floor, ceil). The preview rounds in the same way.
export const floorInt = (n) => Math.floor(n);
export const ceilInt = (n) => Math.ceil(n);
export const clampInt = (n, lo, hi) => Math.min(hi, Math.max(lo, Math.floor(n)));

// ---- Rectangles --------------------------------------------------------------------------------------------------------------------
export const rect = (x, y, w, h) => ({ x, y, w, h });
export const rectRight = (r) => r.x + r.w;
export const rectBottom = (r) => r.y + r.h;
// The rectangle `r` less the margins (top, right, bottom, left). A width or a height is never negative.
export const inset = (r, top = 0, right = 0, bottom = 0, left = 0) => rect(r.x + left, r.y + top, Math.max(0, r.w - left - right), Math.max(0, r.h - top - bottom));
export function intersect(a, b) {
  const x = Math.max(a.x, b.x);
  const y = Math.max(a.y, b.y);
  const r = Math.min(rectRight(a), rectRight(b));
  const bt = Math.min(rectBottom(a), rectBottom(b));
  return r > x && bt > y ? rect(x, y, r - x, bt - y) : null;
}
export function union(a, b) {
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return rect(x, y, Math.max(rectRight(a), rectRight(b)) - x, Math.max(rectBottom(a), rectBottom(b)) - y);
}

// The box of a preview: the body of the panel inside its padding. `w` and `h` are the size of the body, `pad` is `--panel-pad` (8 px).
export const contentBox = (w, h, pad = 8) => rect(0, 0, Math.max(0, w - 2 * pad), Math.max(0, h - 2 * pad));

// ---- Bounds ------------------------------------------------------------------------------------------------------------------------
// A bound says what the document proves about a value. `value` is the value that the preview draws.
export const exact = (value) => ({ kind: EXACT, value });
export const least = (value) => ({ kind: LEAST, value });
export const most = (value) => ({ kind: MOST, value });
export const unknown = () => ({ kind: DATA, value: null });

// True when the value `loaded` of the loaded panel agrees with a bound of the preview. EXACT: the same within `tol`. LEAST: the preview value is
// at most the loaded value (the preview takes the least room). MOST: the preview value is at least the loaded value. DATA: nothing to check.
export function boundHolds(bound, loaded, tol = TOLERANCE_PX) {
  if (!bound || bound.kind === DATA || bound.value === null || bound.value === undefined) return true;
  if (!Number.isFinite(loaded)) return false;
  if (bound.kind === EXACT) return Math.abs(bound.value - loaded) <= tol;
  if (bound.kind === LEAST) return bound.value <= loaded + tol;
  if (bound.kind === MOST) return bound.value >= loaded - tol;
  return false;
}

// ---- The width of a text -----------------------------------------------------------------------------------------------------------
// The width of a text is measured with the font of the loaded panel: `400 12px <the font family of the page>` (app_chart_core.js). A canvas
// measures it. A test sets its own measurer with `setMeasurer(fn)`, where fn(text, font) gives a width.
let measurer = null;
let canvasContext = null;
let familyCache = null;

export function setMeasurer(fn) {
  measurer = fn;
  familyCache = null;
}

// The font family of the panels: the computed family of the body of the page.
export function fontFamily() {
  if (familyCache) return familyCache;
  try {
    familyCache = getComputedStyle(document.body).fontFamily || "system-ui, sans-serif";
  } catch (error) {
    familyCache = "system-ui, sans-serif";
  }
  return familyCache;
}

// The CSS font shorthand of a size and a weight.
export const fontOf = (size = FONT_SIZE, weight = 400, family = fontFamily()) => `${weight} ${size}px ${family}`;

// The width in px of `text` at `size` px and `weight`. The result is not rounded: the rule of each part says where to round.
export function textWidth(text, { size = FONT_SIZE, weight = 400, family } = {}) {
  const font = fontOf(size, weight, family);
  if (measurer) return measurer(String(text), font);
  if (!canvasContext) canvasContext = document.createElement("canvas").getContext("2d");
  canvasContext.font = font;
  return canvasContext.measureText(String(text)).width;
}

// ---- The result of a family --------------------------------------------------------------------------------------------------------
// A family module exports `build(panel, box, doc, ctx)` and returns { elements, bounds, notes }:
//   elements  the nodes of the preview, in px from the top left corner of the box
//   bounds    { name: bound } what the preview claims, for the tests (see exact, least, most, unknown)
//   notes     short texts for the tests and the reader: which rule gave the form ("neutral" when nothing is drawn)
export const resultOf = (elements = [], bounds = {}, notes = []) => ({ elements, bounds, notes });

// The neutral form: the empty body with the sweep and the dots, and nothing else. The preview claims nothing.
export const neutral = (note = "neutral") => resultOf([], {}, [note]);
