// The loading preview of the panel types `state_timeline` and `status_history` (9.11).
// The rules are in the council file `state-timeline.md`. The loaded panel is drawn by web/js/panels/timeline.js, web/js/timeline_model.js and
// web/css/panel-timeline.css. The preview reads the JSON document only. It uses the markup and the classes of the loaded panel for the legend, so the
// browser gives the same boxes, and it draws the least form that the document proves:
//   - the legend, when the document fixes it (case C: a thresholds legend, with its items as bars of the measured width of their texts);
//   - no legend room when the document proves that there is none (cases A, B and E), and no legend room in the undecided case D (the plot is then
//     bounded: it can only get smaller);
//   - the rows envelope: one flat tint on the clip box of the plot, [L, 8, W - 8 - L, H - 29], with L = 8 without row names, the exact gutter for one fixed
//     display name, else the least gutter 21 (it can only move to the right);
//   - the time axis of a state timeline, when the plot box is exact (no legend room or a legend at the bottom, and an exact left gutter) and the range of
//     the request is finite: the x labels as bars of their measured width and the vertical grid lines at the ticks that the engine picks (case E15);
//   - nothing for the rows, the segments, the names and the texts (they are data). The time axis of a status history is data too.
// A panel that can never draw a picture has an empty preview.
// Measured parts: plot (the canvas), legend, legend-label (the text of an item), rows (the clip box of the plot), label-x (an x label), grid-x (a vertical line).

import { gutterOf, stepText } from "../timeline_model.js";
import { interpolateDisplayName } from "../names.js";
import { EXACT, MOST, PARTS, exact, least, most, neutral, node, partAttrs, px, resultOf, unknown } from "./common.js";
import { FROM_DATA, domWidth, hidesLegend, measureText, neverDrawn, pixelRatio, possible, rangeOf, textBoxHeight, timeAxis, warningValues } from "./grids.js";

const PAD = 8;
const AXIS = 21;
const LEAST_GUTTER = 21; // gutterOf(0, true): the gutter of the row names when every name measures 0 px
const BESIDE = 0.34; // the legend at the right keeps 34 % of the width at most
const FALLBACK_ROOM = 228; // the plot needs 228 px beside the legend, else the legend moves under it
const ITEM = 14 + 8 + 10; // the pill, the gap and the space after the text

const isThresholds = (c) => (c && c.mode ? c.mode : "thresholds") === "thresholds";
const builtinColor = (type) => ({ mode: type === "state_timeline" ? "continuous_gr_yl_rd" : "thresholds", fixed: null });
const builtinThresholds = () => ({ mode: "absolute", steps: [{ value: null, color: "green" }, { value: 80, color: "red" }] });
const same = (list) => list.every((v) => JSON.stringify(v) === JSON.stringify(list[0]));

/**
 * The legend case of the council file: "A" (legend off), "B" (no entry can come, E included), "C" (a thresholds legend: `items` are its texts)
 * or "D" (the states give the entries: data). Cases A and B have no legend room and C has an exact legend.
 */
export function legendCase(panel, doc) {
  const o = panel.options || {};
  const legend = o.legend || { show: true, placement: "bottom" };
  if (legend.show === false) return { kind: "A", items: [] };
  const colors = possible(panel, doc, "color", builtinColor(panel.type));
  const thresholds = possible(panel, doc, "thresholds", builtinThresholds());
  const hides = possible(panel, doc, "hide_from", {});
  const steps = (t) => !!t && Array.isArray(t.steps) && t.steps.length >= 2;
  // No entry for every possible config of the first row: a gradient has none, and so has a mode that lists the states while the legend is hidden.
  const none = colors.every((c) => (c && c.mode ? c.mode : "thresholds").startsWith("continuous_") || thresholds.every((t) => !(isThresholds(c) && steps(t)) && hides.every(hidesLegend)));
  if (none) return { kind: "B", items: [] };
  const units = possible(panel, doc, "unit", "none");
  const decimals = possible(panel, doc, "decimals", null);
  if (colors.length === 1 && thresholds.length === 1 && isThresholds(colors[0]) && steps(thresholds[0]) && (thresholds[0].mode === "percentage" || (same(units) && same(decimals)))) {
    const cfg = { thresholds: thresholds[0], unit: units[0] === FROM_DATA ? "none" : units[0], decimals: decimals[0] === FROM_DATA ? null : decimals[0] };
    return { kind: "C", items: thresholds[0].steps.map((s, k) => stepText(cfg, k)) };
  }
  return { kind: "D", items: [] };
}

/**
 * The left gutter of the plot: `{ left, exact }`. 8 without row names. For one fixed display name that the document writes in full: the gutter of
 * that name (the text is measured on a canvas, as the loaded panel does). Else the least gutter, 21: a row name can measure 0 px.
 */
export function gutter(panel, doc) {
  const o = panel.options || {};
  if (o.show_labels === false) return { left: 8, exact: true, why: "no names" };
  const names = possible(panel, doc, "display_name", FROM_DATA);
  const one = names.length > 0 && same(names) ? names[0] : null;
  if (typeof one === "string" && one !== "" && !/\$\{__field|\[\[__field/.test(one)) {
    const text = interpolateDisplayName(one, {}, "", "");
    if (text !== "") return { left: gutterOf(measureText(text), true), exact: true, why: "fixed name" };
  }
  return { left: LEAST_GUTTER, exact: false, why: "names are data" };
}

// `panel` is the panel of the document, `box` is { w, h }: the size of the loaded `.panel__plot`.
export function build(panel, box, doc, ctx) {
  const o = panel.options || {};
  if (warningValues(panel).length) return neutral("warning");
  if (neverDrawn(panel, doc)) return resultOf([], {}, ["never-drawn"]);
  const W = box.w;
  const H = box.h;
  const placement = (o.legend && o.legend.placement) || "bottom";
  const lc = legendCase(panel, doc);
  const g = gutter(panel, doc);
  const notes = [`legend:${lc.kind}`, `gutter:${g.why}`];
  const bounds = {};

  // The canvas box: the host less the legend of case C. The browser lays it out: the preview uses the classes of the loaded panel.
  const kids = [];
  let fallback = false;
  const sizeExact = lc.kind !== "D";
  const plot = node("timeline__main", {}, [], "div", partAttrs(PARTS.PLOT, sizeExact ? EXACT : MOST));
  plot.k.push(node("skel__rows", { "--x": px(g.left) }, [], "div", partAttrs("rows", g.exact && sizeExact ? EXACT : MOST)));
  kids.push(plot);
  // The time axis of a state timeline (case E15). The canvas width is the box width when the legend is off, empty or at the bottom, and the left gutter
  // must be exact. The height is not computed: the label bars sit on the bottom edge of the canvas box, which the browser lays out.
  const range = panel.type === "state_timeline" && g.exact && (lc.kind === "A" || lc.kind === "B" || (lc.kind === "C" && placement !== "right")) ? rangeOf(ctx) : null;
  if (range) {
    const plotW = Math.max(1, W - PAD - g.left);
    const axis = timeAxis({ from: range.from, to: range.to, left: g.left, plotW, width: W, zone: ctx && ctx.page ? ctx.page.zone : undefined });
    const dpr = pixelRatio();
    // The label has its top 9 px under the plot, that is 12 px above the canvas bottom. The bar is centred on the text box of the engine.
    const below = 12 - textBoxHeight(12) / 2 - 4;
    for (const l of axis.labels) plot.k.push(node("skel__xl", { "--x": px(l.x - l.w / 2), "--w": px(l.w), "--b": px(below) }, [], "div", partAttrs("label-x", EXACT)));
    for (const l of axis.lines) plot.k.push(node("skel__vl", { "--x": px(Math.round(l.x * dpr) / dpr), "--w": px(1 / dpr) }, [], "div", partAttrs("grid-x", EXACT)));
    notes.push("axis-x");
  }
  if (lc.kind === "C") {
    const widths = lc.items.map((t) => domWidth(t));
    if (placement === "right") {
      // The legend beside the plot is as wide as its widest item, at most 34 % of the width. Under 228 px for the plot, it moves under the plot.
      const wide = Math.min(2 * 4 + ITEM + Math.max(...widths), BESIDE * W);
      fallback = W < Math.round(wide) + FALLBACK_ROOM;
    }
    const items = lc.items.map((t, k) => node("timeline__legend-item", {}, [node("", {}, [], "i"), node("skel__t", { "--w": px(widths[k]) }, [], "span", partAttrs("legend-label", EXACT))], "li"));
    kids.push(node("timeline__legend", { "--legend-max": px(Math.floor(H * 0.35)) }, items, "ul", partAttrs(PARTS.LEGEND, EXACT)));
    notes.push(fallback ? "legend:fallback" : `legend:${placement}`);
  }
  const attrs = fallback ? { "data-fallback": "" } : {};
  const root = node(`timeline timeline--${placement}`, {}, kids, "div", attrs);

  // The bounds for the unit tests: the sizes that the preview can state without a layout.
  bounds.gutter = g.exact ? exact(g.left) : least(g.left);
  bounds.plotTop = exact(PAD);
  bounds.plotRightMargin = exact(PAD);
  bounds.axisBand = exact(AXIS);
  if (lc.kind === "A" || lc.kind === "B") {
    bounds.canvasWidth = exact(W);
    bounds.canvasHeight = exact(H);
  } else if (lc.kind === "D") {
    bounds.canvasWidth = most(W);
    bounds.canvasHeight = most(H);
  } else bounds.canvasWidth = unknown();
  return resultOf([root], bounds, notes);
}
