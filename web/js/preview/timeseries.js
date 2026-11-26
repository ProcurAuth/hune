// The loading preview of the panel types `timeseries` and `xychart` (9.11), and the frame of a chart of the engine that the bar chart shares.
// The rules are in 9.11 of docs/04 (the council file `timeseries-xychart.md`). The loaded panel is drawn by web/js/panels/timeseries.js, xychart.js and
// the chart engine web/js/vendor/chdash/app_chart_core.js. The preview uses ONLY the JSON members of the panel. It never reads a query.
//
// The classes of a part, against the loaded panel:
//   exact    the document fixes the place and the size.
//   contain  the document gives bounds: the preview rectangle holds the loaded rectangle. `contain:ltrb` names the edges that the rule claims.
//            Left: the preview edge is at the same place or to the left. Top: the same. Right and bottom: the same place or further out.
//   least    the document gives a lower bound on the size (the legend).
// The parts: `plot` (the plot of the engine), `legend`, `head` (the head row of a table legend) and `head-label` (a head text, as a bar).
//
// The legend is built with the markup and the classes of the engine, so the CSS of the loaded legend sizes it (panel-chart.css names the preview
// host `skel__host` too). The preview gives each text as a bar of the measured width and no name: its legend is the least one (one entry, or the heads
// and one empty row). The code of the loaded panel is the reference until the owner decides the open points of the council file.

import { parseGrafanaPattern } from "../fields.js";
import { formatNumber, joinParts } from "../units.js";
import { CONTAIN, EXACT, LEAST, PARTS, exact, least, most, neutral, node, partAttrs, px, resultOf, textWidth, unknown } from "./common.js";

// ---- The sizes that the CSS of the legend gives (panel-chart.css, tokens.css) ---------------------------------------------------------
export const LINE = 12 * 1.5714; // the line of the legend: 12 px text, line height 1.5714
export const HEAD_ROW = 2 + LINE + 2.5; // the head row of a table legend: padding 2 and 2.5
export const BODY_ROW = 2 + LINE + 2 + 1; // a row of a table legend: padding 2 and 2, a rule of 1 px
const LIST_PAD = 12; // the padding of a list legend at the bottom: 8 above, 4 under
const CAP_SHARE = 0.35; // the legend at the bottom keeps at most 35 % of the height (common.js fitHeight)
const FALLBACK_TABLE_CAP = 112; // 8rem, the root font is 14 px
const MIN_PLOT = 220; // the plot keeps this room beside a right legend (common.js applyLegend)
const LEGEND_GAP = 8;
const LIST_NEVER_FALLS = 346; // a list beside the plot: W - ceil(0.34 W) >= 228
const PAD_TOP = 8;
const PAD_RIGHT = 8;
const Y_PAD = 9; // the gutter of an axis: the widest label plus 9 px
const Y_TITLE = 28;
const Y_TITLE_WIDTH = 20;
const BAND = 21;
const X_TITLE = 20;

// The heads of the legend table (app_chart_core.js LEGEND_HEADS). The engine shows these five when the document keeps none.
const HEADS = {
  total: "Total", min: "Min", max: "Max", mean: "Mean", last: "Last", last_not_null: "Last *", first: "First", first_not_null: "First *", count: "Count",
  median: "Median", std_dev: "Std Dev", variance: "Variance", range: "Range", diff: "Difference", diff_percent: "Difference %", delta: "Delta",
  distinct_count: "Distinct count", change_count: "Change count",
};
const BASE_COLUMNS = ["total", "min", "max", "mean", "last"];

const own = (o, k) => o !== null && typeof o === "object" && Object.prototype.hasOwnProperty.call(o, k);
const finite = (v) => typeof v === "number" && Number.isFinite(v);

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
  // The columns of the table: the document order, no duplicate, only the known calculations. None kept gives the five of the engine (defect D1).
  const asked = Array.isArray(lg.values) ? lg.values.filter((v, i, all) => own(HEADS, v) && all.indexOf(v) === i) : [];
  const columns = asked.length ? asked : BASE_COLUMNS;
  const sorted = lg.sort_by === "name" ? 0 : columns.indexOf(lg.sort_by) + 1 || -1;
  return { state, mode, placement, columns, heads: ["Name", ...columns.map((c) => HEADS[c])], sorted, desc: lg.sort_desc === true };
}

// ---- The gutters of the axes ---------------------------------------------------------------------------------------------------------------
// The text of the tick 0 of the axis of the defaults (yTickLabeller): the unit and the decimals decide it.
export function zeroLabel(D) {
  const unit = D.unit === undefined || D.unit === null ? "none" : D.unit;
  const fixed = finite(D.decimals) ? D.decimals : null;
  if (fixed !== null || unit !== "none") {
    try {
      return joinParts(formatNumber(0, unit, fixed));
    } catch (error) {
      return null;
    }
  }
  return "0";
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
  const auto = tick0 === null ? Y_PAD : Math.ceil(Y_PAD + textWidth(tick0));
  if (widths.length) return { value: Math.min(...widths, finite(D.axis_width) ? Infinity : auto), exact: false };
  return { value: auto + (titled ? Y_TITLE : 0), exact: false };
}

// ---- The legend ------------------------------------------------------------------------------------------------------------------------
// The least legend: one entry with no name, or the heads (bars of the measured width) and one empty row. `pill`: the mark shows only when the first
// entry is surely on the left scale (its place is then exact).
function pillNode(pill) {
  return node(pill ? "" : "skel__nopill", {}, [], "i");
}
function itemNode(pill) {
  return node("chartCore__legendItem", {}, [pillNode(pill), node("", {}, [], "span")]);
}
function headCell(leg, i, place) {
  const w = domWidth(leg.heads[i], 500);
  // Only the first and the last text have a place that the document fixes: the name column starts at 30 px, and the last column ends at the right edge.
  const attrs = place && (i === 0 || i === leg.heads.length - 1) ? partAttrs("head-label", place) : {};
  const bar = node("skel__hb", { "--w": px(w) }, [], "span", attrs);
  const state = leg.sorted === i ? (leg.desc ? "descending" : "ascending") : "none";
  return node("", {}, [node("chartCore__legendSort", {}, [bar], "span")], "th", { scope: "col", "aria-sort": state });
}
// `claim`: the legend has a size that the document bounds (none beside the plot where a fallback can come: the legend is then at the right or under).
function legendNode(leg, { cap, lhExact, beside, claim, pill }) {
  const v = { "--legend-max": px(cap) };
  // Beside the plot, the width of the legend is data: only its size is bounded. Under the plot, its place and size are exact when its height is.
  const place = !claim ? null : lhExact && !beside ? EXACT : LEAST;
  const attrs = place ? partAttrs(PARTS.LEGEND, place) : {};
  if (leg.mode === "list") return node("chartCore__legend", v, [node("chartCore__legendList", {}, [itemNode(pill)])], "div", { "data-mode": "list", ...attrs });
  const head = node("", {}, [node("", {}, leg.heads.map((_, i) => headCell(leg, i, place)), "tr")], "thead", place ? partAttrs(PARTS.HEAD, place) : {});
  const row = node("", {}, [node("", {}, [itemNode(pill)], "th", { scope: "row" }), ...leg.columns.map(() => node("", {}, [], "td"))], "tr");
  const table = node("chartCore__legendTable", {}, [head, node("", {}, [row], "tbody")], "table", { "aria-label": "Values of the series" });
  return node("chartCore__legend", v, [node("chartCore__legendTableWrap", {}, [table])], "div", { "data-mode": "table", ...attrs });
}

// The width of the least legend beside the plot: the room that the CSS gives it (a list: 4 + 32 + 4 px; a table: the sum of the heads) and the gutter.
function leastBeside(leg) {
  const g = gutterWidth();
  if (leg.mode === "list") return 40 + g;
  return leg.heads.reduce((sum, h) => sum + 38 + domWidth(h, 500), 0) + g;
}

/**
 * The rules of the vertical chain of a chart of the engine (E3 to E13). `W` and `H` are the size of the box. `leg` is the legend of the document.
 * Returns the legend, the canvas, and what is exact: { placement, fallback, draw, lh, lhExact, canvasW, canvasH, canvasExact, cap, beside }.
 */
export function chainOf(W, H, leg) {
  const cap = Math.floor(H * CAP_SHARE);
  const out = { placement: leg.placement, fallback: false, draw: false, lh: 0, lhExact: true, canvasW: W, canvasExact: true, claimRight: true, cap, beside: 0 };
  if (leg.state === "off") return { ...out, placement: "bottom" };
  if (leg.state === "hidden") return { ...out, canvasW: leg.placement === "right" ? W - LEGEND_GAP : W };
  // An override can hide some entries: the legend is not certain, so the preview draws none and takes the whole height.
  if (leg.state === "maybe") {
    const right = leg.placement === "right";
    return { ...out, canvasW: right ? W - LEGEND_GAP : W, canvasExact: !right, claimRight: !right, lhExact: false };
  }
  out.draw = true;
  if (leg.placement === "bottom") {
    if (leg.mode === "list") return { ...out, lh: Math.round(LIST_PAD + Math.min(LINE, cap)), lhExact: cap <= LINE };
    return { ...out, lh: Math.round(Math.min(HEAD_ROW + BODY_ROW, cap)), lhExact: cap <= HEAD_ROW + BODY_ROW };
  }
  const lmin = leastBeside(leg);
  if (W < lmin + LEGEND_GAP + MIN_PLOT) {
    // The fallback is certain: the legend goes under the plot. A list has no cap there, so its height is data. A table keeps the cap of 8rem.
    const lh = leg.mode === "list" ? Math.round(4 + LINE + 4) : Math.round(4 + Math.min(HEAD_ROW + BODY_ROW, FALLBACK_TABLE_CAP));
    return { ...out, fallback: true, lh, lhExact: false };
  }
  // Beside the plot. The width of the legend is data: the least one gives the largest plot. A list that is wide enough never falls back.
  const never = leg.mode === "list" && W >= LIST_NEVER_FALLS;
  return { ...out, canvasW: W - LEGEND_GAP - lmin, canvasExact: false, claimRight: never, beside: lmin, lhExact: never };
}

// ---- The frame of a chart of the engine -------------------------------------------------------------------------------------------------------
/**
 * Builds the preview of a chart of the engine (a time series, an xy chart or a vertical bar chart).
 * `top` and `band` are the margins above and under the plot, with `topExact` and `bandExact`. `left` and `right` are gutters ({ value, exact }).
 * `pill`: the first legend entry is surely on the left scale.
 */
export function chartFrame(box, rules, leg, { top, topExact = true, band, bandExact = true, left, right, pill, notes }) {
  const W = box.w;
  const H = box.h;
  const c = chainOf(W, H, leg);
  const canvasH = Math.max(60, Math.floor(H - c.lh));
  const plotH = Math.max(40, canvasH - top - band);
  const allExact = left.exact && right.exact && c.lhExact && c.canvasExact && topExact && bandExact;
  const edges = `l${topExact ? "t" : ""}${c.claimRight ? "r" : ""}b`;
  const cls = allExact ? EXACT : `${CONTAIN}:${edges}`;
  const region = node("skel__r skel__plot", { "--x": px(left.value), "--y": px(top), "--r": px(right.value), "--h": px(plotH) }, [], "div", partAttrs(PARTS.PLOT, cls));
  const plotCell = node("chartCore__plot", { "--c": px(canvasH) }, [region]);
  const kids = [plotCell];
  if (c.draw) kids.push(legendNode(leg, { cap: c.cap, lhExact: c.lhExact, beside: c.beside > 0, claim: !c.beside || c.claimRight, pill }));
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
  const tick0 = tick0OfSeries(panel, rules) ? zeroLabel(D) : null;
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
  // The first entry of the list legend is on the left scale surely when no override sets the placement or hides a series.
  const pill = free && !touched("hide_from") && (placement === "auto" || placement === "left");
  return { top, topExact, left, right, pill };
}

export function build(panel, box, doc, ctx) {
  const rules = rulesOf(panel);
  const leg = legendOf(panel, rules);
  if (!leg) return neutral("no legend member");
  // The warning line above the plot changes the box of the plot, and only its layout gives its height: the preview draws nothing then.
  if (rules.warning) return neutral("warning line");
  const xy = panel.type === "xychart";
  if (xy) {
    // An xy chart: the left gutter is data (the labels come from the data), the x axis has a title (41 px band), and no axis can be on the right.
    const left = { value: Y_PAD, exact: false };
    const right = { value: PAD_RIGHT, exact: true };
    return chartFrame(box, rules, leg, { top: PAD_TOP, band: BAND + X_TITLE, left, right, pill: true, notes: ["xychart frame"] });
  }
  const g = seriesGeometry(panel, rules);
  return chartFrame(box, rules, leg, { top: g.top, topExact: g.topExact, band: BAND, left: g.left, right: g.right, pill: g.pill, notes: ["time series frame"] });
}
