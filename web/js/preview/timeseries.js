// The loading preview of the panel types `timeseries` and `xychart` (9.11), and the frame of a chart of the engine that the bar chart shares.
// The rules are in 9.11 of docs/04 (the council file `timeseries-xychart.md`). The loaded panel is drawn by web/js/panels/timeseries.js, xychart.js and
// the chart engine web/js/vendor/chdash/app_chart_core.js. The preview uses ONLY the JSON members of the panel and the range of the request. It never
// reads a query.
//
// The classes of a part, against the loaded panel:
//   exact    the document fixes the place and the size.
//   contain  the document gives bounds: the preview rectangle holds the loaded rectangle. `contain:ltrb` names the edges that the rule claims.
//            Left: the preview edge is at the same place or to the left. Top: the same. Right and bottom: the same place or further out.
//   least    the document gives a lower bound on the size (the legend).
// The parts: `plot` (the plot of the engine), `legend`, `head` (the head row of a table legend), `head-label` (a head text, as a bar), `label-y` and `label-x` (an
// axis label of the left y axis, `label-y`, or of the x axis, `label-x`, as a bar, only where the document and the range of the request fix its place).
//
// The legend is built with the markup and the classes of the engine, so the CSS of the loaded legend sizes it (panel-chart.css names the preview
// host `skel__host` too). The preview gives each text as a bar of the measured width and no name: its legend is the least one (one entry, or the heads
// and one empty row). The plot has the decorative form of a series: one fixed line, or bars with heights that vary, in neutral tones. It claims no
// value, no count and no colour. The code of the loaded panel is the reference.

import { parseGrafanaPattern } from "../fields.js";
import { dashOf } from "../panels/series_opts.js";
import { boundedYAxis, tickLabel } from "../panels/series.js";
import { formatNumber, joinParts } from "../units.js";
import { chartCore } from "../vendor/chdash/app_chart_core.js";
import { zoneClock } from "../zone.js";
import { BAR_HEIGHT, CONTAIN, EXACT, LEAST, PARTS, barNode, fontOf, exact, least, lineNode, most, neutral, node, partAttrs, pathNode, px, resultOf, svgNode, textWidth, unknown } from "./common.js";

// ---- The sizes that the CSS of the legend gives (panel-chart.css, tokens.css) ---------------------------------------------------------
export const LINE = 12 * 1.5714; // the line of the legend: 12 px text, line height 1.5714
export const HEAD_ROW = 2 + LINE + 2.5; // the head row of a table legend: padding 2 and 2.5
export const BODY_ROW = 2 + LINE + 2 + 1; // a row of a table legend: padding 2 and 2, a rule of 1 px
export const LIST_PAD = 12; // the padding of a list legend at the bottom: 8 above, 4 under
export const CAP_SHARE = 0.35; // the legend at the bottom keeps at most 35 % of the height (common.js fitHeight)
export const MIN_PLOT = 220; // the plot keeps this room beside a right legend (common.js applyLegend)
export const LEGEND_GAP = 8;
const LIST_NEVER_FALLS = 346; // a list beside the plot: W - ceil(0.34 W) >= 228
const PAD_TOP = 8;
const PAD_RIGHT = 8;
const Y_PAD = 9; // the gutter of an axis: the widest label plus 9 px
const Y_LABEL_GAP = 5; // a y label ends 5 px before the plot (9 px with a border)
const Y_TITLE = 28;
const Y_TITLE_WIDTH = 20;
const BAND = 21;
const X_TITLE = 20;
const X_LABEL_TOP = 9; // an x label has its top 9 px under the plot

// The heads of the legend table (app_chart_core.js LEGEND_HEADS).
const HEADS = {
  total: "Total", min: "Min", max: "Max", mean: "Mean", last: "Last", last_not_null: "Last *", first: "First", first_not_null: "First *", count: "Count",
  median: "Median", std_dev: "Std Dev", variance: "Variance", range: "Range", diff: "Difference", diff_percent: "Difference %", delta: "Delta",
  distinct_count: "Distinct count", change_count: "Change count",
};

const own = (o, k) => o !== null && typeof o === "object" && Object.prototype.hasOwnProperty.call(o, k);
const finite = (v) => typeof v === "number" && Number.isFinite(v);
const num = (n) => Number(Number(n).toFixed(2));

// ---- The platform ----------------------------------------------------------------------------------------------------------------------
let gutterCache = null;
/** Tests set the width of the scroll bar gutter that the platform gives (0 with overlay scroll bars). */
export function setGutter(n) {
  gutterCache = n;
}
// The width of `scrollbar-gutter: stable` on this platform: the browser gives it. Overlay scroll bars give 0.
export function gutterWidth() {
  if (gutterCache !== null) return gutterCache;
  gutterCache = 0;
  try {
    const probe = document.createElement("div");
    for (const [k, v] of Object.entries({ position: "absolute", visibility: "hidden", overflowY: "auto", scrollbarGutter: "stable", width: "100px", height: "20px" })) probe.style[k] = v;
    document.body.append(probe);
    gutterCache = probe.offsetWidth - probe.clientWidth;
    probe.remove();
  } catch (error) {
    gutterCache = 0;
  }
  return gutterCache;
}

// The width of a text that the DOM draws: the canvas width plus the letter spacing of the page (the canvas ignores it).
let spacingCache = null;
export function setSpacing(px_) {
  spacingCache = px_;
}
function letterSpacing() {
  if (spacingCache !== null) return spacingCache;
  spacingCache = 0;
  try {
    const v = parseFloat(getComputedStyle(document.body).letterSpacing);
    spacingCache = Number.isFinite(v) ? v : 0;
  } catch (error) {
    spacingCache = 0;
  }
  return spacingCache;
}
export const domWidth = (text, weight = 400) => textWidth(text, { weight }) + [...text].length * letterSpacing();

// The width of a text on the canvas of a chart. The engine measures on its own canvas, which sits in the page: the letter spacing of the page applies
// to it, as it does to the drawing (the recorder of the harness reads the same width). A probe canvas in the page gives the same measure. Without a
// page (a test), the measurer of common.js gives it.
let probeCtx = null;
export function canvasWidth(text, weight = 400) {
  if (typeof document === "undefined" || !document.body) return textWidth(text, { weight });
  if (!probeCtx) {
    const canvas = document.createElement("canvas");
    canvas.style.setProperty("position", "absolute");
    canvas.style.setProperty("visibility", "hidden");
    canvas.style.setProperty("pointer-events", "none");
    canvas.setAttribute("width", "1");
    canvas.setAttribute("height", "1");
    canvas.setAttribute("aria-hidden", "true");
    document.body.append(canvas);
    probeCtx = canvas.getContext("2d");
  }
  probeCtx.font = fontOf(12, weight);
  return probeCtx.measureText(String(text)).width;
}

// The height of the line box of a text that a canvas draws (the ascent and the descent of the font). The recorder of the harness reads the same.
let lineCache = null;
export function setTextHeight(n) {
  lineCache = n;
}
export function textHeight() {
  if (lineCache !== null) return lineCache;
  let h = 14;
  try {
    const ctx = document.createElement("canvas").getContext("2d");
    ctx.font = `400 12px ${getComputedStyle(document.body).fontFamily}`;
    const m = ctx.measureText("0");
    if (finite(m.fontBoundingBoxAscent) && finite(m.fontBoundingBoxDescent)) h = m.fontBoundingBoxAscent + m.fontBoundingBoxDescent;
  } catch (error) {
    h = 14;
  }
  lineCache = h;
  return h;
}
/** The fonts changed (the page font arrived): the measures are read again. */
export function resetMeasures() {
  probeCtx = null;
  gutterCache = null;
  spacingCache = null;
  lineCache = null;
}

// ---- The members of the document ---------------------------------------------------------------------------------------------------------
// The ref of the only visible query, or null when the panel has more than one, a reuse, or a transform (R4b of the council file).
function soleRef(panel) {
  const d = panel.data;
  if (!d || d.reuse || !d.query || (panel.transforms || []).length) return null;
  const visible = [d.query, ...(d.extra_queries || [])].filter((q) => q.hide !== true);
  if (visible.length !== 1) return null;
  return visible[0].ref || (visible[0] === d.query ? "A" : null);
}

/**
 * What the document decides about the field rules. `D` is the defaults joined by the overrides that match every field (a matcher `query` with the ref of
 * the only visible query). A matcher `regex` that does not compile matches no field and gives a warning line. Each other override is undecided: it
 * can set any of its members for some field, so `touched(key)` is true for the keys that it sets. `values(key)` lists the values that a key can have.
 */
export function rulesOf(panel) {
  const spec = panel.fields || {};
  const D = { ...(spec.defaults || {}) };
  const undecided = [];
  const sole = soleRef(panel);
  let warning = false;
  for (const o of spec.overrides || []) {
    const m = o.match || {};
    const set = o.set || {};
    if (m.by === "regex" && parseGrafanaPattern(m.value) === null) warning = true;
    else if (m.by === "query" && sole !== null && m.value === sole) Object.assign(D, set);
    else undecided.push(set);
  }
  const touched = (key) => undecided.some((s) => own(s, key));
  const values = (key) => [D[key], ...undecided.map((s) => s[key])].filter((v) => v !== undefined && v !== null);
  // `key` has one value for every field: the defaults set it, and no undecided override sets another value.
  const uniform = (key) => undecided.every((s) => !own(s, key) || s[key] === D[key]);
  return { D, touched, values, uniform, warning };
}

/**
 * The legend of the document (the shared object `options.legend`, 4.26). `state`: `off` (show is false), `hidden` (every entry is hidden by
 * `hide_from.legend` of the defaults, and no override sets `hide_from`), `maybe` (an override can hide some entries) or `shown`.
 * `columns` are the calculations of the table: `values` of the document, in its order. A table with none has the column "Name" only (4.26).
 * Returns null when the panel has no legend member.
 */
export function legendOf(panel, rules) {
  const lg = panel.options && panel.options.legend;
  if (!lg || typeof lg !== "object") return null;
  const hide = rules.D.hide_from;
  const hideAll = hide && hide.legend === true;
  let state = "shown";
  if (lg.show === false) state = "off";
  else if (hideAll && !rules.touched("hide_from")) state = "hidden";
  else if (hideAll || rules.touched("hide_from")) state = "maybe";
  const mode = lg.mode === "table" ? "table" : "list";
  const placement = lg.show === false ? "bottom" : lg.placement === "right" ? "right" : "bottom";
  // The columns of the table: the document order, no duplicate, only the known calculations.
  const columns = Array.isArray(lg.values) ? lg.values.filter((v, i, all) => own(HEADS, v) && all.indexOf(v) === i) : [];
  const sorted = lg.sort_by === "name" ? 0 : columns.indexOf(lg.sort_by) + 1 || -1;
  return { state, mode, placement, columns, heads: ["Name", ...columns.map((c) => HEADS[c])], sorted, desc: lg.sort_desc === true };
}

// ---- The gutters of the axes ---------------------------------------------------------------------------------------------------------------
// The label of a tick of the axis of the defaults (yTickLabeller): the unit and the decimals decide it.
export function labellerOf(D) {
  const unit = D.unit === undefined || D.unit === null ? "none" : D.unit;
  const fixed = finite(D.decimals) ? D.decimals : null;
  if (fixed !== null || unit !== "none") return (v) => joinParts(formatNumber(v, unit, fixed));
  return tickLabel;
}

// The text of the tick 0 of the axis of the defaults.
export function zeroLabel(D) {
  try {
    return String(labellerOf(D)(0, 1));
  } catch (error) {
    return null;
  }
}

// True when the tick 0 of a linear time series axis is certain (E14): the range holds 0 whatever the data are.
export function tick0OfSeries(panel, rules) {
  const { D, touched } = rules;
  if (panel.options && panel.options.y_scale === "log") return false;
  for (const k of ["min", "max", "soft_min", "soft_max", "unit", "decimals", "transform", "axis_centered_zero"]) if (touched(k)) return false;
  if (D.transform !== undefined && D.transform !== null) return false;
  const hardMin = finite(D.min);
  const hardMax = finite(D.max);
  const sMin = finite(D.soft_min);
  const sMax = finite(D.soft_max);
  if (hardMin && D.min === 0 && !sMin) return true;
  if (hardMax && D.max === 0 && !sMax) return true;
  if (hardMin && hardMax && D.min < 0 && D.max > 0) return true;
  if (!hardMin && !hardMax && sMin && sMax && D.soft_min <= 0 && D.soft_max >= 0) return true;
  if (!hardMin && !hardMax && D.axis_centered_zero === true) return true;
  return false;
}

// True when the tick 0 of a bar chart axis is certain (B14): the bars start at 0, so only a range that excludes 0 moves it.
export function tick0OfBars(rules) {
  const { D, values, uniform } = rules;
  if (!uniform("unit") || !uniform("decimals")) return false;
  if (values("min").some((v) => v > 0) || values("soft_min").some((v) => v > 0)) return false;
  if (values("max").some((v) => v < 0) || values("soft_max").some((v) => v < 0)) return false;
  return D !== null;
}

// The title of a scale is proven when the defaults set a text and no override sets one (P1: the panel has at least one series).
export const titleProven = (rules) => typeof rules.D.axis_label === "string" && rules.D.axis_label !== "" && !rules.touched("axis_label");

/**
 * The gutter of one side. `sure`: the axis of this side shows for certain. `tick0`: the least text of the tick 0 (or null). Returns { value, exact }:
 * `exact` when the document fixes the gutter (E14 a and b), else the least gutter that the document proves.
 */
export function gutterOf(rules, { sure, tick0, none = PAD_RIGHT }) {
  const { D, touched, values } = rules;
  const widths = values("axis_width").filter(finite);
  const text = typeof D.axis_label === "string" && D.axis_label !== "";
  const titled = titleProven(rules);
  // The title is known when it is proven, or when no place can set one.
  const known = titled || (!text && !touched("axis_label"));
  if (!sure) return { value: Math.min(none, ...widths), exact: false };
  if (finite(D.axis_width) && !touched("axis_width")) return { value: D.axis_width + (titled ? Y_TITLE_WIDTH : 0), exact: known };
  const auto = tick0 === null ? Y_PAD : Math.ceil(Y_PAD + canvasWidth(tick0));
  if (widths.length) return { value: Math.min(...widths, finite(D.axis_width) ? Infinity : auto), exact: false };
  return { value: auto + (titled ? Y_TITLE : 0), exact: false };
}

// ---- The legend ------------------------------------------------------------------------------------------------------------------------
// The least legend: one entry with no name, or the heads (bars of the measured width) and one empty row. `pill`: the mark shows only when the first
// entry is surely on the left scale (its place is then exact). `kind` is `chart` (the markup of the engine) or `hbar` (the horizontal bar chart).
function pillNode(pill) {
  return node(pill ? "" : "skel__nopill", {}, [], "i");
}
function itemNode(pill, kind) {
  return node(kind === "hbar" ? "hbar__legendItem" : "chartCore__legendItem", {}, [pillNode(pill), node("", {}, [], "span")]);
}
function headCell(leg, i, place) {
  const w = domWidth(leg.heads[i], 500);
  // Only the first and the last text have a place that the document fixes: the name column starts at 30 px, and the last column ends at the right edge.
  const attrs = place && (i === 0 || i === leg.heads.length - 1) ? partAttrs("head-label", place) : {};
  const bar = node("skel__hb", { "--w": px(w) }, [], "span", attrs);
  const state = leg.sorted === i ? (leg.desc ? "descending" : "ascending") : "none";
  return node("", {}, [node("chartCore__legendSort", {}, [bar], "span")], "th", { scope: "col", "aria-sort": state });
}
/**
 * The node of the least legend. `place` is the class of the legend part and of the first and last head text (null: no measured part).
 * `kind` `hbar` gives the horizontal bar chart markup: the legend is the element `hbar__legend`, with its own items.
 */
export function legendNode(leg, { cap, place, pill, kind = "chart" }) {
  const v = { "--legend-max": px(cap) };
  const attrs = place ? partAttrs(PARTS.LEGEND, place) : {};
  const hbar = kind === "hbar";
  if (leg.mode === "list") {
    const items = [itemNode(pill, kind)];
    if (hbar) return node("hbar__legend", v, items, "div", { "data-mode": "list", ...attrs });
    return node("chartCore__legend", v, [node("chartCore__legendList", {}, items)], "div", { "data-mode": "list", ...attrs });
  }
  const head = node("", {}, [node("", {}, leg.heads.map((_, i) => headCell(leg, i, place)), "tr")], "thead", place ? partAttrs(PARTS.HEAD, place) : {});
  const row = node("", {}, [node("", {}, [itemNode(pill, kind)], "th", { scope: "row" }), ...leg.columns.map(() => node("", {}, [], "td"))], "tr");
  const table = node("chartCore__legendTable", {}, [head, node("", {}, [row], "tbody")], "table", { "aria-label": "Values of the series" });
  return node(hbar ? "hbar__legend" : "chartCore__legend", v, [node("chartCore__legendTableWrap", {}, [table])], "div", { "data-mode": "table", ...attrs });
}

// The width of the least legend beside the plot: the room that the CSS gives it (a list: 4 + 32 + 4 px; a table: the sum of the heads) and the gutter.
export function leastBeside(leg) {
  const g = gutterWidth();
  if (leg.mode === "list") return 40 + g;
  return leg.heads.reduce((sum, h) => sum + 38 + domWidth(h, 500), 0) + g;
}

/**
 * The rules of the vertical chain of a chart of the engine (E3 to E13). `W` and `H` are the size of the box. `leg` is the legend of the document.
 * Returns the legend, the canvas, and what is exact: { placement, fallback, draw, lh, lhExact, lhMax, canvasW, canvasExact, claimRight, cap, beside }.
 * `lhMax` is the most height that the legend can take under the plot (null when the document gives no bound).
 */
export function chainOf(W, H, leg) {
  const cap = Math.floor(H * CAP_SHARE);
  const out = { placement: leg.placement, fallback: false, draw: false, lh: 0, lhExact: true, lhMax: 0, canvasW: W, canvasExact: true, claimRight: true, cap, beside: 0 };
  if (leg.state === "off") return { ...out, placement: "bottom" };
  // Every entry hidden: the engine hides the legend, and the host gets no gap (9.19).
  if (leg.state === "hidden") return { ...out, placement: "bottom" };
  // An override can hide some entries: the legend is not certain, so the preview draws none and takes the whole height.
  if (leg.state === "maybe") {
    const right = leg.placement === "right";
    return { ...out, canvasExact: !right, claimRight: !right, lhExact: false, lhMax: null };
  }
  out.draw = true;
  if (leg.placement === "bottom") {
    if (leg.mode === "list") return { ...out, lh: Math.round(LIST_PAD + Math.min(LINE, cap)), lhExact: cap <= LINE, lhMax: LIST_PAD + cap };
    return { ...out, lh: Math.round(Math.min(HEAD_ROW + BODY_ROW, cap)), lhExact: cap <= HEAD_ROW + BODY_ROW, lhMax: cap };
  }
  const lmin = leastBeside(leg);
  if (W < lmin + LEGEND_GAP + MIN_PLOT) {
    // The fallback is certain: the legend goes under the plot, with the cap of 35 % (9.19). Its height is data.
    const lh = leg.mode === "list" ? Math.round(8 + Math.min(LINE, cap)) : Math.round(4 + Math.min(HEAD_ROW + BODY_ROW, cap));
    return { ...out, fallback: true, lh, lhExact: false, lhMax: null };
  }
  // Beside the plot. The width of the legend is data: the least one gives the largest plot. A list that is wide enough never falls back.
  const never = leg.mode === "list" && W >= LIST_NEVER_FALLS;
  return { ...out, canvasW: W - LEGEND_GAP - lmin, canvasExact: false, claimRight: never, beside: lmin, lhExact: never, lhMax: never ? 0 : null };
}

// ---- The decorative form of a series -------------------------------------------------------------------------------------------------------
// The marks claim nothing: one fixed line, or bars with heights that vary. Only the style of the document changes them (the interpolation, the
// line width and dash, the fill, the points). Every mark has a neutral tone. The points are in percent of the plot, from the top left corner.
const POINTS = [[0, 64], [9, 50], [18, 58], [28, 36], [38, 46], [48, 28], [58, 40], [68, 24], [79, 38], [89, 30], [100, 42]];
const BAR_HEIGHTS = [38, 62, 46, 74, 54, 30, 68, 50, 80, 42, 58, 34];
export const BAR_PERIOD = 14; // px between two decorative bars
const BAR_WIDTH = 10;

/** The path of the decorative line for an interpolation of the document: `linear`, `smooth`, `step_before` or `step_after`. */
export function linePath(mode, pts = POINTS) {
  const at = (p) => `${num(p[0])} ${num(p[1])}`;
  let d = `M${at(pts[0])}`;
  for (let i = 1; i < pts.length; i++) {
    const a = pts[i - 1];
    const b = pts[i];
    if (mode === "step_before") d += ` L${num(a[0])} ${num(b[1])} L${at(b)}`;
    else if (mode === "step_after") d += ` L${num(b[0])} ${num(a[1])} L${at(b)}`;
    else if (mode === "smooth") {
      // Catmull-Rom to Bezier: the curve passes through every point.
      const p0 = pts[i - 2] || a;
      const p3 = pts[i + 1] || b;
      d += ` C${num(a[0] + (b[0] - p0[0]) / 6)} ${num(a[1] + (b[1] - p0[1]) / 6)} ${num(b[0] - (p3[0] - a[0]) / 6)} ${num(b[1] - (p3[1] - a[1]) / 6)} ${at(b)}`;
    } else d += ` L${at(b)}`;
  }
  return d;
}

/** The decorative bars for a plot of `plotW` px: a fixed period, heights that vary. The viewBox is in px of the period and 100 for the height. */
export function barMarks(plotW) {
  const n = Math.max(2, Math.floor(plotW / BAR_PERIOD));
  const rects = [];
  for (let i = 0; i < n; i++) {
    const h = BAR_HEIGHTS[i % BAR_HEIGHTS.length];
    rects.push(node("skel__bar", {}, [], "rect", { x: String(i * BAR_PERIOD + (BAR_PERIOD - BAR_WIDTH) / 2), y: String(100 - h), width: String(BAR_WIDTH), height: String(h) }));
  }
  return svgNode(n * BAR_PERIOD, 100, rects, "skel__svg skel__bars");
}

/** The style of the series of a time series (7 of the council file, section 3): the members of the defaults, else the options. Null: nothing draws. */
export function markStyle(panel, rules) {
  const { D, touched } = rules;
  const o = panel.options || {};
  // An override can change `draw` for some series: the form is not certain, so nothing draws.
  if (panel.type !== "xychart" && touched("draw")) return null;
  // Every series is hidden from the plot (`hide_from.viz`) and no override shows one.
  if (D.hide_from && D.hide_from.viz === true && !touched("hide_from")) return null;
  const pick = (own_, opt, fallback) => (own_ !== undefined && own_ !== null ? own_ : opt !== undefined && opt !== null ? opt : fallback);
  if (panel.type === "xychart") {
    const lines = o.draw === "lines" || o.draw === "points_and_lines";
    const dots = o.draw !== "lines";
    return { kind: "xy", line: lines, dots, pointSize: pick(null, o.point_size, 5), lineWidth: pick(null, o.line_width, 1), interpolation: "linear", fill: 0, gradient: "none", dash: null };
  }
  const draw = pick(D.draw, o.draw, "line");
  const lineWidth = pick(D.line_width, o.line_width, 1);
  const show = pick(D.show_points, o.show_points, "auto");
  const dash = dashOf(D).dash;
  return {
    kind: draw === "bars" ? "bars" : draw === "points" ? "points" : "line",
    line: draw === "line",
    dots: draw === "points" || (draw === "line" && show === "always"),
    pointSize: pick(D.point_size, o.point_size, 5),
    lineWidth,
    interpolation: o.interpolation || "linear",
    fill: draw === "line" ? pick(D.fill_opacity, o.fill_opacity, 0) : 0,
    gradient: o.gradient_mode || "none",
    dash,
  };
}

/** The nodes of the decorative marks in a plot of `plotW` px (the estimate: the plot is at most this wide). */
export function marksOf(style, plotW, id) {
  if (!style) return [];
  if (style.kind === "bars") return [barMarks(plotW)];
  const kids = [];
  const d = linePath(style.interpolation);
  if (style.fill > 0) {
    const gid = `skel-fade-${id}`;
    const fade = style.gradient === "opacity";
    const area = `${d} L100 100 L0 100 Z`;
    if (fade) {
      kids.push(node("", {}, [node("", {}, [node("skel__stop", {}, [], "stop", { offset: "0", "stop-opacity": String(num(style.fill / 100)) }), node("skel__stop", {}, [], "stop", { offset: "1", "stop-opacity": "0" })], "linearGradient", { id: gid, x1: "0", y1: "0", x2: "0", y2: "1" })], "defs"));
    }
    // The fill that fades has its paint in the attribute (a CSS fill would win over it), the flat fill has its tone in the CSS.
    kids.push(node(fade ? "skel__fade" : "skel__area", {}, [], "path", { d: area, ...(fade ? { fill: `url(#${gid})` } : { "fill-opacity": String(num(style.fill / 100)) }) }));
  }
  if (style.line && style.lineWidth > 0) {
    const a = style.dash ? { "stroke-dasharray": style.dash.join(" ") } : {};
    const p = pathNode(d, "skel__path skel__series");
    p.v = { "--lw": px(style.lineWidth) };
    p.a = { ...p.a, ...a };
    kids.push(p);
  }
  const svg = svgNode(100, 100, kids, "skel__svg skel__marks");
  const out = kids.length ? [svg] : [];
  if (style.dots) for (const [x, y] of POINTS) out.push(node("skel__dot", { "--x": `${x}%`, "--y": `${y}%`, "--d": px(style.pointSize) }));
  return out;
}

// ---- The axes that the document fixes --------------------------------------------------------------------------------------------------
/**
 * The y axis of a time series whose range the document fixes (E17 case i): a linear scale with the hard ends `min` and `max`, no soft end, one scale on
 * the left, and no override that can change one of these. The range is then [min, max] for any data. Returns { axis, labels } for the plot height `P`
 * (the same code as the loaded panel: `boundedYAxis` and the labeller of the unit), or null.
 */
export function fixedAxis(panel, rules, P) {
  const { D, touched } = rules;
  if (panel.type !== "timeseries" || !finite(P)) return null;
  const o = panel.options || {};
  if (o.y_scale === "log" || o.stacking === "percent") return null;
  for (const k of ["min", "max", "soft_min", "soft_max", "unit", "decimals", "transform", "axis_placement", "axis_centered_zero", "axis_border"]) if (touched(k)) return null;
  if (!finite(D.min) || !finite(D.max) || !(D.max > D.min)) return null;
  if (finite(D.soft_min) || finite(D.soft_max)) return null;
  if (D.transform !== undefined && D.transform !== null) return null;
  if (D.axis_placement !== undefined && D.axis_placement !== null && D.axis_placement !== "auto" && D.axis_placement !== "left") return null;
  const axis = boundedYAxis(D.min, D.max, D.min, D.max, P, D.unit === "bytes");
  if (!axis) return null;
  const label = labellerOf(D);
  return { axis, labels: axis.values.map((v) => String(label(v, axis.step))), border: D.axis_border === true };
}

// The gutter of the fixed axis for a plot height `P`: the widest label plus 9 px, and the title room.
const fixedGutter = (fixed, titled) => Math.ceil(Y_PAD + Math.max(0, ...fixed.labels.map((t) => canvasWidth(t)))) + (titled ? Y_TITLE : 0);

/**
 * The left gutter when the document fixes the y axis. `P` is the plot height of the preview. `pExact`: P is the height of the loaded plot. `range`: the
 * least and the greatest plot height that the document allows (R9: the gutter is used only when it is the same for every integer P of the range).
 * Returns { value, exact, fixed } or null when the axis is not fixed.
 */
export function fixedLeft(panel, rules, P, pExact, range) {
  const first = fixedAxis(panel, rules, P);
  if (!first) return null;
  const { D, touched } = rules;
  const text = typeof D.axis_label === "string" && D.axis_label !== "";
  const titled = titleProven(rules);
  const known = titled || (!text && !touched("axis_label"));
  // A width of the document wins over the labels (E14 b).
  if (finite(D.axis_width) && !touched("axis_width")) return { value: D.axis_width + (titled ? Y_TITLE_WIDTH : 0), exact: known, fixed: pExact && known && !titled ? first : null };
  if (touched("axis_width") || !known) return null;
  const at = (p) => {
    const f = fixedAxis(panel, rules, p);
    return f ? fixedGutter(f, titled) : null;
  };
  const value = fixedGutter(first, titled);
  if (pExact) return { value, exact: true, fixed: titled ? null : first };
  if (range && range[0] < range[1]) {
    for (let p = range[0]; p <= range[1]; p++) if (at(p) !== value) return null;
    return { value, exact: true, fixed: null };
  }
  return null;
}

// ---- The x labels from the range of the request -----------------------------------------------------------------------------------------------
/**
 * The x labels of a time series (E19, R12): the same ticks and layout as the engine, over the range of the request (the page asks `rt.rangeOf(key)`).
 * `box`: { left, top, plotW, plotH, canvasW }. Returns { labels: [{ x, w, text }], ticks: [px] } or null when the range is not known.
 */
export function xAxisOf(range, zone, g) {
  if (!range || range.error || !finite(range.from_ms) || !finite(range.to_ms) || !(range.to_ms > range.from_ms)) return null;
  const measure = (t) => canvasWidth(t);
  const clock = zoneClock(zone);
  const ticks = chartCore.timeTicks(range.from_ms, range.to_ms, g.plotW, measure, clock);
  const k = g.plotW / (range.to_ms - range.from_ms);
  const xOf = (v) => g.left + (v - range.from_ms) * k;
  const shown = chartCore.layoutXLabels(ticks, xOf, g.left, g.left + g.plotW, g.canvasW, measure, measure, 0);
  const lines = ticks.map((t) => xOf(t.v)).filter((x) => x >= g.left - 0.5 && x <= g.left + g.plotW + 0.5);
  return { labels: shown.map((t) => ({ x: t.x, w: measure(t.label), text: t.label })), ticks: lines };
}

// A label as a bar of the measured width. `left` is the left edge of the text and `cy` the centre of its line. The part is `label-y` or `label-x`: the
// harness matches the parts of a name by their order, and the y labels come first.
const labelBar = (part, left, cy, w) => barNode(left, cy - BAR_HEIGHT / 2, w, part, EXACT);

// ---- The frame of a chart of the engine -------------------------------------------------------------------------------------------------------
/**
 * Builds the preview of a chart of the engine (a time series, an xy chart or a vertical bar chart).
 * `top` and `band` are the margins above and under the plot, with `topExact` and `bandExact`. `left` is a gutter ({ value, exact }) or a function
 * `(P, pExact, range)` that gives one (with `fixed`: the y axis of the document). `right` is a gutter. `pill`: the first legend entry is surely on
 * the left scale. `marks(plotW, plotH)` gives the decorative marks of the plot. `x(g)` gives the x axis (E19) when the whole plot is exact.
 */
export function chartFrame(box, rules, leg, { top, topExact = true, band, bandExact = true, left: leftArg, right, pill, notes, marks = null, x = null }) {
  const W = box.w;
  const H = box.h;
  const c = chainOf(W, H, leg);
  const canvasH = Math.max(60, Math.floor(H - c.lh));
  const plotH = Math.max(40, canvasH - top - band);
  const pExact = c.lhExact && bandExact && topExact;
  let range = null;
  if (!pExact && c.lhMax !== null && bandExact && topExact) range = [Math.max(40, Math.max(60, Math.floor(H - c.lhMax)) - top - band), plotH];
  const left = typeof leftArg === "function" ? leftArg(plotH, pExact, range) : leftArg;
  const allExact = left.exact && right.exact && c.lhExact && c.canvasExact && topExact && bandExact;
  const edges = `l${topExact ? "t" : ""}${c.claimRight ? "r" : ""}b`;
  const cls = allExact ? EXACT : `${CONTAIN}:${edges}`;
  const plotW = Math.max(40, c.canvasW - left.value - right.value);
  const region = node("skel__r skel__plot", { "--x": px(left.value), "--y": px(top), "--r": px(right.value), "--h": px(plotH) }, marks ? marks(plotW, plotH) : [], "div", partAttrs(PARTS.PLOT, cls));
  const plotKids = [region];
  // The y labels need the left gutter and the plot height. The x labels and the grid need the whole plot.
  const axis = left.fixed && pExact && left.exact ? left.fixed : null;
  const g = { left: left.value, top, plotW, plotH, canvasW: c.canvasW, canvasH };
  const xa = allExact && x ? x(g) : null;
  if (axis || xa) {
    const lines = [];
    if (allExact && axis) {
      const k = plotH / (axis.axis.max - axis.axis.min);
      for (const v of axis.axis.values) lines.push(lineNode(left.value, num(top + plotH - (v - axis.axis.min) * k), left.value + plotW, num(top + plotH - (v - axis.axis.min) * k)));
    }
    if (xa) for (const px_ of xa.ticks) lines.push(lineNode(num(px_), top, num(px_), top + plotH));
    if (lines.length) plotKids.push(svgNode(c.canvasW, canvasH, lines));
  }
  if (axis) {
    const k = plotH / (axis.axis.max - axis.axis.min);
    const end = left.value - (axis.border ? 9 : Y_LABEL_GAP);
    axis.axis.values.forEach((v, i) => {
      const w = canvasWidth(axis.labels[i]);
      if (axis.labels[i] !== "") plotKids.push(labelBar("label-y", end - w, top + plotH - (v - axis.axis.min) * k, w));
    });
  }
  if (xa) for (const l of xa.labels) plotKids.push(labelBar("label-x", l.x - l.w / 2, top + plotH + X_LABEL_TOP + textHeight() / 2, l.w));
  const plotCell = node("chartCore__plot", { "--c": px(canvasH) }, plotKids);
  const kids = [plotCell];
  // Beside the plot, the width of the legend is data: only its size is bounded. Under the plot, its place and size are exact when its height is.
  const claim = !c.beside || c.claimRight;
  if (c.draw) kids.push(legendNode(leg, { cap: c.cap, place: !claim ? null : c.lhExact && !c.beside ? EXACT : LEAST, pill }));
  const hostA = { "data-legend-placement": c.placement };
  if (c.fallback) hostA["data-legend-fallback"] = "";
  const host = node("skel__host", {}, [node("chartCore", {}, kids)], "div", hostA);
  const bounds = {
    plotTop: topExact ? exact(top) : unknown(),
    plotLeft: left.exact ? exact(left.value) : least(left.value),
    plotRight: !c.claimRight ? unknown() : c.canvasExact && right.exact ? exact(c.canvasW - right.value) : most(c.canvasW - right.value),
    plotBottom: c.lhExact && bandExact && topExact ? exact(top + plotH) : most(top + plotH),
    plotHeight: c.lhExact && bandExact ? exact(plotH) : most(plotH),
    legendHeight: c.draw ? (c.lhExact ? exact(c.lh) : least(c.lh)) : leg.state === "maybe" ? least(0) : exact(0),
    band: bandExact ? exact(band) : least(band),
    gutterLeft: left.exact ? exact(left.value) : least(left.value),
    gutterRight: right.exact ? exact(right.value) : least(right.value),
  };
  return resultOf([host], bounds, notes);
}

// ---- The time series and the xy chart ---------------------------------------------------------------------------------------------------------
/**
 * The geometry of a time series from the members of the document (E11 to E15). Returns the margins and the gutters.
 */
export function seriesGeometry(panel, rules) {
  const { D, touched } = rules;
  const placement = D.axis_placement === undefined || D.axis_placement === null ? "auto" : D.axis_placement;
  const free = !touched("axis_placement");
  const tick0 = tick0OfSeries(panel, rules) ? zeroLabel(rules.D) : null;
  // The top margin: 0 when no axis shows (all hidden), else 8. An undecided override that sets the placement makes it data.
  const top = placement === "hidden" ? 0 : PAD_TOP;
  const topExact = free;
  let left;
  let right;
  if (!free) {
    left = gutterOf(rules, { sure: false, tick0 });
    right = gutterOf(rules, { sure: false, tick0 });
  } else if (placement === "right") {
    left = { value: PAD_RIGHT, exact: true };
    right = gutterOf(rules, { sure: true, tick0 });
  } else if (placement === "hidden") {
    left = { value: PAD_RIGHT, exact: true };
    right = { value: PAD_RIGHT, exact: true };
  } else {
    left = gutterOf(rules, { sure: true, tick0 });
    // A second unit puts a series on the right scale (auto), so an override of the unit makes the right gutter undecided.
    right = placement === "left" || !touched("unit") ? { value: PAD_RIGHT, exact: true } : gutterOf(rules, { sure: false, tick0 });
  }
  // The first entry is on the left scale surely when no override sets the placement or hides a series, and the defaults do not put it on the right.
  // With every axis hidden, every series goes on the left scale (9.19).
  const pill = free && !touched("hide_from") && placement !== "right";
  const fixable = free && (placement === "auto" || placement === "left") && right.exact;
  return { top, topExact, left, right, pill, fixable };
}

export function build(panel, box, doc, ctx) {
  const rules = rulesOf(panel);
  const leg = legendOf(panel, rules);
  if (!leg) return neutral("no legend member");
  // The warning line above the plot changes the box of the plot, and only its layout gives its height: the preview draws nothing then.
  if (rules.warning) return neutral("warning line");
  const style = markStyle(panel, rules);
  const marks = (plotW) => marksOf(style, plotW, panel.id);
  if (panel.type === "xychart") {
    // An xy chart: the left gutter is data (the labels come from the data), the x axis has a title (41 px band), and no axis can be on the right.
    const left = { value: Y_PAD, exact: false };
    const right = { value: PAD_RIGHT, exact: true };
    return chartFrame(box, rules, leg, { top: PAD_TOP, band: BAND + X_TITLE, left, right, pill: true, marks, notes: ["xychart frame"] });
  }
  const g = seriesGeometry(panel, rules);
  const left = g.fixable ? (P, pExact, range) => fixedLeft(panel, rules, P, pExact, range) || g.left : g.left;
  return chartFrame(box, rules, leg, { top: g.top, topExact: g.topExact, band: BAND, left, right: g.right, pill: g.pill, marks, x: xAxisFor(panel, rules, ctx), notes: ["time series frame"] });
}

// The x axis of a time series when the document and the range fix it. The range comes from the runtime (`rt.rangeOf(key)`), and the zone from the
// page. A bar series keeps the span of the data (9.19): the form of the series must be certain and not `bars`.
function xAxisFor(panel, rules, ctx) {
  if (!ctx || !ctx.rt || typeof ctx.rt.rangeOf !== "function" || ctx.key === undefined || ctx.key === null) return null;
  if (rules.touched("draw") || (rules.D.draw || (panel.options && panel.options.draw)) === "bars") return null;
  return (g) => xAxisOf(ctx.rt.rangeOf(ctx.key), ctx.page ? ctx.page.zone : "browser", g);
}
