// The loading preview of the panel type `heatmap` (9.11).
// The rules are in the council file `heatmap.md`. The loaded panel is drawn by web/js/panels/heatmap.js, web/js/heat_model.js and the engine of the charts.
// The preview reads the JSON document only. It draws the least form that the document proves:
//   - the plot rectangle, a guide that paints nothing (exact with a hidden axis and with a fixed linear axis, else bounded at the left);
//   - with a fixed linear axis (case F): the y labels as bars of their measured width and the horizontal grid lines at the places of the ticks;
//   - the legend band of 46 px, the colour bar at its exact place (a ramp of two skeleton tones), and the two legend ends as bars when the document
//     fixes them (`color.min` and `color.max` are numbers);
//   - with a hidden axis and with a fixed linear axis (the left edge is exact) and a finite range of the request: the x labels as bars of their measured
//     width and the vertical grid lines at the ticks that the engine picks (the same functions, the same range, the same zone);
//   - nothing at all when the document proves that no picture can come (cases E1 to E4; E1 needs the range of the request).
// It never draws the cells, the columns, the rows, an x label with no exact left edge or with no range, or a y label outside the case F.
// Measured parts: plot, legend, scale (the colour bar), label (a y label), grid (a horizontal line), label-x (an x label), grid-x (a vertical line),
// legend-label (a legend end).

import { shortText } from "../bucket_model.js";
import { MAX_HEAT_CELLS, niceTime } from "../heat_model.js";
import { niceStepAtLeast, tickLabel } from "../panels/series.js";
import { formatNumber, joinParts } from "../units.js";
import { BAR_HEIGHT, EXACT, LEAST, MOST, PARTS, barNode, ceilInt, exact, least, most, neutral, node, partAttrs, px, resultOf } from "./common.js";
import { domWidth, guideNode, measureText, neverDrawn, pixelRatio, possible, rangeOf, textBoxHeight, timeAxis, vlineNode, warningValues } from "./grids.js";

// The sizes of the loaded heatmap (9.19e, 4.33c).
const LEGEND = 46; // the band of the legend, at the bottom of the root
const BAR_LEFT = 25;
const BAR_TOP = 10;
const BAR_HIGH = 9;
const BAR_WIDE = 300;
const LABEL_TOP = 23; // 10 + 9 + 4: the top of the row of the two legend ends
const LABEL_HIGH = 14;
const LABEL_SIZE = 11;
const TOP = 8; // the plot top with a shown y axis (the engine: PAD_TOP)
const RIGHT = 8;
const BAND = 21; // the band of the x labels
const MIN_PLOT = 40;
const MIN_CANVAS = 60;
const LABEL_GAP = 5; // the y labels end 5 px before the plot
const GUTTER = 9; // the gutter is the widest y label plus 9 px
const COUNT_MAX = 500000; // a count of the shape raw: rows x value fields at most

const isNumber = (v) => typeof v === "number" && Number.isFinite(v);

// The certain no-picture cases (the council file, E1 to E4): the loaded body ends in a message, never in a picture. Gives the name of the case or "".
// `range` is the range of the request, or null. E1 needs it: the columns of a computed grid are the steps of the time range.
export function noPicture(panel, doc, range = null) {
  const o = panel.options || {};
  if (o.calculate === true) {
    // E1: more than 250,000 columns. The loaded model answers with the limit of the cells, whatever the rows are (9.19e, section 2).
    if (range && o.x_buckets) {
      const x = o.x_buckets;
      const step = x.mode === "size" && x.value > 0 ? x.value : niceTime((range.to - range.from) / Math.max((x.value || 20) - 1, 1));
      if (Math.floor(range.to / step) - Math.floor(range.from / step) + 1 > MAX_HEAT_CELLS) return "E1";
    }
    // A count of a computed grid is a whole number from 0 to COUNT_MAX. E2: none of them passes `hide_le` and `hide_ge`.
    const a = isNumber(o.hide_le) ? Math.max(0, Math.floor(o.hide_le) + 1) : 0;
    if (a > COUNT_MAX || (isNumber(o.hide_ge) && o.hide_ge <= a)) return "E2";
  } else if (o.calculate === false && isNumber(o.hide_le) && isNumber(o.hide_ge) && o.hide_le >= o.hide_ge) {
    return "E3"; // no number is above `hide_le` and below `hide_ge`
  }
  return neverDrawn(panel, doc) ? "E4" : "";
}

// The case of the y axis (the council file, section 0): "H" hidden, "F" fixed linear axis with a fixed text of the labels, else "".
// `unit` and `decimals` are the single possible values of the config of a value field. They are null when the data can change them.
export function axisCase(panel, doc) {
  const o = panel.options || {};
  if (o.y_axis === "hidden") return { kind: "H" };
  if (o.y_axis !== "left" || o.calculate !== true) return { kind: "" };
  const scale = o.y_buckets && o.y_buckets.scale;
  if (scale !== "linear" || !isNumber(o.y_min) || !isNumber(o.y_max) || !(o.y_max - o.y_min > 0) || !Number.isFinite(o.y_max - o.y_min)) return { kind: "" };
  const one = (list) => (list.every((v) => JSON.stringify(v) === JSON.stringify(list[0])) ? { v: list[0] } : null);
  const unit = one(possible(panel, doc, "unit", "none"));
  const decimals = one(possible(panel, doc, "decimals", null));
  if (!unit || !decimals) return { kind: "" };
  return { kind: "F", unit: unit.v, decimals: decimals.v };
}

// The text of a y tick (yTickLabeller of the loaded panel): a field with `decimals` or a unit writes the value with them, else the least digits.
const tickText = (unit, decimals) => {
  const fixed = decimals !== null && decimals !== undefined ? decimals : null;
  if (fixed !== null || (unit && unit !== "none")) return (v) => joinParts(formatNumber(v, unit, fixed));
  return tickLabel;
};

// The ticks of the fixed linear axis: `[{ v, y, text }]` for a plot of the height `h`. `y` is from the plot top.
export function fixedTicks(o, h, unit, decimals) {
  const lo = o.y_min;
  const hi = o.y_max;
  const step = niceStepAtLeast((30 * (hi - lo)) / h);
  const text = tickText(unit, decimals);
  const out = [];
  for (let k = Math.ceil(lo / step - 1e-9); k * step <= hi + step * 1e-9 && out.length < 200; k++) {
    const v = Number((k * step).toPrecision(12));
    const along = ((v - lo) * h) / (hi - lo);
    out.push({ v, y: o.y_reverse === true ? along : h - along, text: text(v, step) });
  }
  return out;
}

const gridNode = (x, y, w) => {
  const dpr = pixelRatio();
  // The engine draws a line of 1 / dpr px at the centre crisp(y) = (round(y dpr) + 0.5) / dpr: its top edge is round(y dpr) / dpr.
  return node("skel__hl", { "--x": px(x), "--y": px(Math.round(y * dpr) / dpr), "--w": px(w), "--h": px(1 / dpr) }, [], "div", partAttrs("grid", EXACT));
};

// `panel` is the panel of the document, `box` is { w, h }: the size of the loaded `.heat` root (the body less its padding).
export function build(panel, box, doc, ctx) {
  const o = panel.options || {};
  if (warningValues(panel).length) return neutral("warning");
  const range = rangeOf(ctx);
  const none = noPicture(panel, doc, range);
  if (none) return resultOf([], {}, [`no-picture:${none}`]);

  const W = box.w;
  const H = box.h;
  const Wc = Math.floor(W);
  const legendOn = o.show_legend === true;
  const band = legendOn ? LEGEND : 0;
  const canvasH = Math.max(MIN_CANVAS, H - band);
  const hidden = o.y_axis === "hidden";
  const top = hidden ? 0 : TOP;
  const plotH = Math.max(MIN_PLOT, canvasH - top - BAND);
  const elements = [];
  const bounds = {};
  const notes = [];

  // The plot. The right edge is always Wc - 8. The left edge is fixed with a hidden axis (8) and with a fixed linear axis, else it is 9 or more.
  const axis = axisCase(panel, doc);
  let left;
  let ticks = [];
  if (axis.kind === "H") {
    left = 8;
    notes.push("axis:hidden");
  } else if (axis.kind === "F") {
    ticks = fixedTicks(o, plotH, axis.unit, axis.decimals).filter((t) => t.text !== "");
    left = ceilInt(Math.max(0, ...ticks.map((t) => measureText(t.text))) + GUTTER);
    notes.push("axis:fixed");
  } else {
    left = GUTTER;
    notes.push("axis:bounded");
  }
  const sure = axis.kind !== "";
  const plotW = Math.max(MIN_PLOT, Wc - left - RIGHT);
  elements.push(guideNode({ x: left, y: top, w: plotW, h: plotH }, PARTS.PLOT, sure ? EXACT : MOST, "skel__g skel__plot"));
  bounds.plotTop = exact(top);
  bounds.plotHeight = exact(plotH);
  bounds.plotRight = exact(Wc - RIGHT);
  bounds.plotLeft = sure ? exact(left) : least(left);
  bounds.plotWidth = sure ? exact(plotW) : most(plotW);

  // The y labels and the horizontal grid of the fixed axis. A label is a bar of its measured width that ends 5 px before the plot.
  if (axis.kind === "F") {
    for (const t of ticks) {
      const w = measureText(t.text);
      elements.push(barNode(left - LABEL_GAP - w, top + t.y - BAR_HEIGHT / 2, w));
    }
    for (const t of ticks) elements.push(gridNode(left, top + t.y, plotW));
    bounds.yTicks = exact(ticks.length);
  }

  // The x axis: the range of the request fixes the ticks, and the left edge fixes the plot width. The labels are 9 px under the plot, and the grid
  // lines run over its height. A bar is centred on the text box that the engine draws with `textBaseline` top.
  if (sure && range) {
    const axisX = timeAxis({ from: range.from, to: range.to, left, plotW, width: Wc, zone: ctx && ctx.page ? ctx.page.zone : undefined });
    const centre = top + plotH + 9 + textBoxHeight(12) / 2;
    for (const l of axisX.labels) elements.push(barNode(l.x - l.w / 2, centre - BAR_HEIGHT / 2, l.w, "label-x", EXACT));
    for (const l of axisX.lines) elements.push(vlineNode(l.x, top, plotH));
    bounds.xLabels = exact(axisX.labels.length);
    notes.push("axis-x");
  }

  // The legend: the band of 46 px at the bottom, the colour bar and the two ends.
  if (legendOn) {
    const y0 = H - LEGEND;
    const wr = Math.round(W);
    const bar = wr >= 350 ? BAR_WIDE : Math.max(10, wr - 50);
    const lo = isNumber(o.color && o.color.min) ? shortText(o.color.min) : null;
    const hi = isNumber(o.color && o.color.max) ? shortText(o.color.max) : null;
    const wl = lo === null ? 0 : domWidth(lo, { size: LABEL_SIZE });
    const wh = hi === null ? 0 : domWidth(hi, { size: LABEL_SIZE });
    const both = lo !== null && hi !== null;
    // The legend shrinks to its content: the padding, then the wider of the bar and the two ends. It never passes the root.
    const content = both ? Math.max(bar, wl + wh) : bar;
    const wide = Math.min(W, BAR_LEFT + content);
    elements.push(guideNode({ x: 0, y: y0, w: wide, h: LEGEND }, PARTS.LEGEND, both ? EXACT : LEAST, "skel__g skel__legend"));
    elements.push(node("skel__scale", { "--x": px(BAR_LEFT), "--y": px(y0 + BAR_TOP), "--w": px(bar), "--h": px(BAR_HIGH) }, [], "div", partAttrs(PARTS.SCALE, EXACT)));
    const labelY = y0 + LABEL_TOP + (LABEL_HIGH - BAR_HEIGHT) / 2;
    if (lo !== null) elements.push(barNode(BAR_LEFT, labelY, wl, "legend-label", EXACT));
    if (both) elements.push(barNode(wide - wh, labelY, wh, "legend-label", EXACT));
    bounds.legendHeight = exact(LEGEND);
    bounds.barWidth = exact(bar);
    bounds.legendWidth = both ? exact(wide) : least(wide);
    notes.push(both ? "legend:ends" : lo !== null ? "legend:left-end" : "legend");
  }
  return resultOf(elements, bounds, notes);
}
