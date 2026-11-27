// The loading preview of the panel type `histogram` (9.11).
// The rules are in the council file `histogram.md`. The loaded panel is drawn by web/js/panels/histogram.js, web/js/bucket_model.js and the engine of
// the charts. The preview reads the JSON document only and draws the least form that the document proves:
//   - the plot rectangle, a guide that paints nothing: the top is 8, the right edge is the box less 8 (and the room of a right legend), the bottom
//     is the box less the least legend and the band of the x labels, the left edge is the least gutter (it can only move to the right);
//   - the label "0" as a bar of its measured width, and the grid line of the tick 0 (the two marks that every histogram has);
//   - the legend, when the document proves that it has an entry: the least legend (one entry, or the head of the table and one empty row),
//     drawn with the sizes of the engine; a name only when the document fixes it (`Count`, or one display name for every series);
//   - nothing for the bars, the other y labels and the x labels: they are data.
// A panel that can never draw a picture has an empty preview.
// Measured parts: plot, legend, grid, label (the "0"), mark (the pill of the first entry), legend-label (its name), legend-head (the head "Name").

import { clip } from "../bucket_model.js";
import { histYAxis } from "../panels/series.js";
import { BAR_HEIGHT, EXACT, HEADS, LEAST, MOST, PARTS, barNode, ceilInt, exact, least, most, neutral, node, partAttrs, px, resultOf } from "./common.js";
import { FROM_DATA, LINE, domWidth, guideNode, hidesLegend, measureText, neverDrawn, pixelRatio, possible, scrollGutter, warningValues } from "./grids.js";

const TOP = 8;
const RIGHT = 8;
const BAND = 21;
const MIN_CANVAS = 60;
const MIN_PLOT = 40;
const LABEL_GAP = 5;
const GUTTER = 9;
const SHARE = 0.35; // the legend at the bottom keeps 35 % of the height at most
const BESIDE = 0.34; // the legend at the right keeps 34 % of the width at most
const COLUMN_GAP = 8;
const MIN_PLOT_WIDTH = 228; // 220 for the plot and 8 for the gap: under it the legend moves under the plot
const ITEM_PAD = 14 + 8 + 10; // the pill, the gap and the space after the name
const ITEM_MAX = 308; // 22rem
const HEAD_ROW = 2 + LINE + 2.5; // a head of the legend table (the Hune rule)
const BODY_ROW = 2 + LINE + 2 + 1; // a row of the legend table and its rule
const TEXT_CENTRE = 8.5; // the centre of the text box of a 12 px text, from the top of its line of 18.857 px (the font of the page)

// A display name that the document writes in full: no template, no variable.
const lit = (v) => typeof v === "string" && v !== "" && !v.includes("$") && !v.includes("[[");

// The facts of the document about the names and the legend (the predicates of the council file).
export function legendFacts(panel, doc) {
  const o = panel.options || {};
  const legend = o.legend || { show: true, placement: "bottom", mode: "list", values: [] };
  const names = possible(panel, doc, "display_name", FROM_DATA);
  // A name `xMin` or `BucketMin` gives the shape `bucketed`: the names are known only when none of them is such a name.
  const NAMES = names.every((v) => lit(v) && v !== "xMin" && v !== "BucketMin");
  const ONE = NAMES && names.every((v) => v === names[0]);
  const COUNT = o.combine === true && NAMES;
  const hides = possible(panel, doc, "hide_from", {});
  const show = legend.show !== false;
  const EMPTY = show && o.combine === false && hides.every(hidesLegend);
  const ON = show && !EMPTY && (COUNT || !hides.some(hidesLegend));
  const name = COUNT ? "Count" : ONE ? names[0] : null;
  const heads = Array.isArray(legend.values) && legend.values.length ? [...new Set(legend.values.filter((v) => Object.prototype.hasOwnProperty.call(HEADS, v)))] : [];
  return { legend, show, placement: show ? legend.placement || "bottom" : "bottom", mode: legend.mode === "table" ? "table" : "list", NAMES, ONE, COUNT, EMPTY, ON, name, heads };
}

const itemWidth = (name) => (name === null ? 0 : Math.min(domWidth(clip(name), { size: 12 }), ITEM_MAX - ITEM_PAD));

// The grid line of the tick 0 and of the other ticks: 1 / dpr px high, at the top edge round(y dpr) / dpr of the line that the engine draws.
const gridNode = (x, y, w, cls) => {
  const dpr = pixelRatio();
  return node("skel__hl", { "--x": px(x), "--y": px(Math.round(y * dpr) / dpr), "--w": px(w), "--h": px(1 / dpr) }, [], "div", partAttrs("grid", cls));
};

// `panel` is the panel of the document, `box` is { w, h }: the size of the loaded `.panel__plot`.
export function build(panel, box, doc, ctx) {
  const o = panel.options || {};
  if (warningValues(panel).length) return neutral("warning");
  if (neverDrawn(panel, doc)) return resultOf([], {}, ["no-chart"]);
  const W = box.w;
  const H = box.h;
  const f = legendFacts(panel, doc);
  const gutter = scrollGutter();
  const cap = Math.floor(SHARE * H);
  const elements = [];
  const bounds = {};
  const notes = [];

  // ---- The legend: its place decides the canvas. `lh` is the height under the plot, `lw` the width beside it (the least that the document proves).
  let lh = 0; // offsetHeight under the plot, an integer
  let lhRaw = 0; // the height of the rect
  let lw = 0; // the width beside the plot
  let beside = false;
  let table = false;
  let legendExact = false; // the size of the legend is exact (a legend of one entry with a known name)
  if (f.ON) {
    table = f.mode === "table";
    const name = table ? null : f.name;
    if (f.placement === "right") {
      const need = table ? f.heads.reduce((a, h) => a + 38 + domWidth(HEADS[h], { size: 12, weight: 500 }), 38 + domWidth("Name", { size: 12, weight: 500 })) + gutter : Math.min(40 + gutter + itemWidth(name), BESIDE * W);
      if (W < Math.round(need) + MIN_PLOT_WIDTH) {
        // Certain fallback: the legend goes under the plot, the host keeps `data-legend-placement="right"`.
        // Under the plot the legend keeps the cap of 35 % of the height, as at the bottom (the list and the table wrap scroll).
        lhRaw = table ? 4 + Math.min(HEAD_ROW + BODY_ROW, cap) : 8 + Math.min(LINE, cap);
        lh = Math.round(lhRaw);
        notes.push("legend:fallback");
      } else {
        beside = true;
        lw = need;
        notes.push("legend:right");
      }
      legendExact = !table && f.COUNT;
    } else {
      lhRaw = table ? Math.min(HEAD_ROW + BODY_ROW, cap) : 12 + Math.min(LINE, cap);
      lh = Math.round(lhRaw);
      legendExact = !table && f.COUNT;
      notes.push("legend:bottom");
    }
  } else {
    notes.push(f.show ? (f.EMPTY ? "legend:empty" : "legend:unknown") : "legend:off");
  }

  // ---- The canvas and the plot.
  const canvasW = beside ? W - COLUMN_GAP - lw : W;
  const canvasH = Math.max(MIN_CANVAS, Math.floor(H - lh));
  const plotH = Math.max(MIN_PLOT, canvasH - TOP - BAND);
  const base = TOP + plotH;
  // The y labels: the label "0" is always there. With `percent` and one series named Count, every share is 1 and the labels follow from the height.
  let ticks = [{ v: 0, y: base, text: "0" }];
  const exactLabels = f.COUNT && o.stacking === "percent";
  if (exactLabels) {
    const ax = histYAxis(1, 1, plotH);
    if (ax) ticks = ax.ticks.map((t) => ({ v: t.v, y: TOP + plotH * (1 - t.v / ax.max), text: t.label }));
  }
  const labelW = Math.max(0, ...ticks.map((t) => measureText(t.text)));
  const left = ceilInt(labelW + GUTTER);
  const plotW = Math.max(MIN_PLOT, canvasW - left - RIGHT);
  const exactX = exactLabels && (!beside || f.COUNT || f.EMPTY || lw === 0);
  const exactY = lh === 0 || legendExact;
  elements.push(guideNode({ x: left, y: TOP, w: plotW, h: plotH }, PARTS.PLOT, exactX && exactY ? EXACT : MOST, "skel__g skel__plot"));
  bounds.plotTop = exact(TOP);
  bounds.plotLeft = exactLabels ? exact(left) : least(left);
  bounds.plotRight = exactX ? exact(canvasW - RIGHT) : most(canvasW - RIGHT);
  bounds.plotBottom = exactY ? exact(base) : most(base);
  bounds.plotHeight = exactY ? exact(plotH) : most(plotH);
  bounds.plotWidth = exactX ? exact(plotW) : most(plotW);

  // The grid lines and the labels of the ticks, in the order of the axis. A label ends 5 px before the plot.
  for (const t of ticks) {
    const w = measureText(t.text);
    elements.push(barNode(left - LABEL_GAP - w, t.y - BAR_HEIGHT / 2, w, PARTS.LABEL, exactLabels ? EXACT : LEAST));
  }
  for (const t of ticks) elements.push(gridNode(left, t.y, plotW, exactX ? EXACT : MOST));
  bounds.yTicks = exactLabels ? exact(ticks.length) : least(1);

  // ---- The legend marks. The rect of a legend that is not exact is a lower bound.
  if (f.ON) {
    const cls = legendExact ? EXACT : LEAST;
    const nameW = itemWidth(f.name);
    if (beside) {
      const x0 = W - lw;
      elements.push(guideNode({ x: x0, y: 0, w: lw, h: H }, PARTS.LEGEND, cls, "skel__g skel__legend"));
      const pad = 4; // the list has 4 px on each side and 4 above, the table has 4 above only
      if (table) elements.push(barNode(x0 + 30, pad + 2 + TEXT_CENTRE - BAR_HEIGHT / 2, domWidth("Name", { size: 12, weight: 500 }), "legend-head", LEAST));
      else {
        elements.push(node("skel__pill", { "--x": px(x0 + pad), "--y": px(pad + (LINE - 4) / 2) }, [], "div", partAttrs(PARTS.MARK, LEAST)));
        if (f.name !== null) elements.push(barNode(x0 + pad + 22, pad + TEXT_CENTRE - BAR_HEIGHT / 2, nameW, "legend-label", LEAST));
      }
    } else if (lh > 0) {
      const y0 = canvasH;
      const fall = notes.includes("legend:fallback");
      const pad = fall ? 4 : 8;
      elements.push(guideNode({ x: 0, y: y0, w: W, h: lhRaw }, PARTS.LEGEND, cls, "skel__g skel__legend"));
      if (table) {
        const head = fall ? 4 : 0;
        // The first head has a gutter of 30 px for the sort arrow. The fallback and a phone (600 px or less) take it away: the cell has no padding.
        const phone = fall || (typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia("(max-width: 600px)").matches);
        elements.push(barNode(phone ? 0 : 30, y0 + head + 2 + TEXT_CENTRE - BAR_HEIGHT / 2, domWidth("Name", { size: 12, weight: 500 }), "legend-head", LEAST));
      } else {
        elements.push(node("skel__pill", { "--x": px(4), "--y": px(y0 + pad + (LINE - 4) / 2) }, [], "div", partAttrs(PARTS.MARK, LEAST)));
        if (f.name !== null) elements.push(barNode(26, y0 + pad + TEXT_CENTRE - BAR_HEIGHT / 2, nameW, "legend-label", LEAST));
      }
    }
    bounds.legend = legendExact ? exact(lhRaw || lw) : least(lhRaw || lw);
  }
  return resultOf(elements, bounds, notes);
}
