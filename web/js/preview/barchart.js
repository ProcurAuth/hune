// The loading preview of the panel type `barchart` (9.11).
// The rules are in 9.11 of docs/04 (the council file `barchart.md`). The loaded panel is drawn by web/js/panels/barchart.js (vertical, the chart
// engine) and barchart_h.js (horizontal, DOM). The preview uses ONLY the JSON members of the panel. It never reads a query.
//
// The vertical form is the frame of a chart of the engine (timeseries.js `chartFrame`): the same legend, the same rounding chain. The horizontal form
// uses the markup and the classes of the loaded `.hbar`, so its CSS gives the sizes. Neither form draws a bar, a row, a category or a value: the
// document does not give their count or their size. The legend is the least one: one entry with no name, or the head bars and one empty row.
//
// The parts: `plot` (the plot of the engine, or `.hbar__grid`) and `legend`, with the classes of timeseries.js.

import { CONTAIN, LEAST, PARTS, exact, least, most, neutral, node, partAttrs, px, resultOf, unknown } from "./common.js";
import { LINE, chartFrame, gutterOf, legendOf, rulesOf, tick0OfBars, titleProven, zeroLabel } from "./timeseries.js";

const PAD_TOP = 8;
const PAD_RIGHT = 8;
const BAND = 21;
const H_LABEL = 22; // the least width of the column of the category names: 6 px, no text, 16 px
const H_TITLE = 28;
const H_LEGEND_SHARE = 0.3;
const H_ITEM = 14 + 8 + 10; // the pill, the gap and the space after the name

const finite = (v) => typeof v === "number" && Number.isFinite(v);

/** The renderer that draws the panel: `horizontal` gives H, `auto` gives H when the box is higher than wide (a tie gives V). */
export function orientationOf(panel, box) {
  const o = panel.options && panel.options.orientation;
  if (o === "horizontal") return "horizontal";
  if (o === "auto" && box.w < box.h) return "horizontal";
  return "vertical";
}

// The vertical form: the frame of the engine. The value axis always shows (the top margin is 8) and a bar chart has no right axis.
function vertical(panel, box, rules, leg) {
  const turned = finite(panel.options.x_tick_rotation) && panel.options.x_tick_rotation !== 0;
  const tick0 = tick0OfBars(rules) ? zeroLabel(rules.D) : null;
  const left = gutterOf(rules, { sure: true, tick0 });
  const right = { value: PAD_RIGHT, exact: true };
  return chartFrame(box, rules, leg, { top: PAD_TOP, band: BAND, bandExact: !turned, left, right, pill: true, notes: ["bar chart, vertical frame"] });
}

// The band of the value axis of the horizontal form: `axis_width` of the defaults when the document fixes it, else 21 px.
function bandOf(rules) {
  const { D, touched, values } = rules;
  if (finite(D.axis_width) && !touched("axis_width")) return { value: D.axis_width, exact: true };
  const widths = values("axis_width").filter(finite);
  return widths.length ? { value: Math.min(BAND, ...widths), exact: false } : { value: BAND, exact: true };
}

// The title band: 28 px when the title is proven. An override that can set a title makes the band undecided, and the preview takes none.
function titleOf(rules) {
  if (titleProven(rules)) return { value: H_TITLE, exact: true };
  const text = typeof rules.D.axis_label === "string" && rules.D.axis_label !== "";
  return { value: 0, exact: !text && !rules.touched("axis_label") };
}

// The horizontal form. The legend of this form follows `legend.show` and `legend.placement` only (no mode, no hidden entry, no fallback).
function horizontal(panel, box, rules) {
  const lg = panel.options.legend;
  const show = lg.show === true;
  const right = show && lg.placement === "right";
  const band = bandOf(rules);
  const title = titleOf(rules);
  const W = box.w;
  const H = box.h;
  const legendW = right ? Math.min(4 + H_ITEM + 4, H_LEGEND_SHARE * W) : 0;
  const legendH = show && !right ? 8 + LINE + 4 : 0;
  const gridRight = W - legendW - PAD_RIGHT;
  const gridBottom = H - legendH - band.value - title.value;
  const edges = "ltrb";
  const grid = node("hbar__grid", {}, [], "div", partAttrs(PARTS.PLOT, `${CONTAIN}:${edges}`));
  const body = node("hbar__body", {}, [grid]);
  const plotKids = [body, node("hbar__axis", {}, [])];
  if (title.value) plotKids.push(node("hbar__axisTitle", {}, []));
  const kids = [node("hbar__plot", {}, plotKids)];
  if (show) {
    const item = node("hbar__legendItem", {}, [node("", {}, [], "i"), node("", {}, [], "span")]);
    kids.push(node("hbar__legend", {}, [item], "div", partAttrs(PARTS.LEGEND, LEAST)));
  }
  const root = node("hbar", { "--hbar-label": px(H_LABEL), "--band": px(band.value) }, kids, "div", { "data-legend-placement": right ? "right" : "bottom" });
  const bounds = {
    plotTop: exact(PAD_TOP),
    plotLeft: least(H_LABEL),
    plotRight: most(gridRight),
    plotBottom: most(gridBottom),
    legendHeight: show && !right ? least(legendH) : show ? unknown() : exact(0),
    legendWidth: right ? least(legendW) : unknown(),
    band: band.exact ? exact(band.value) : least(band.value),
    title: title.exact ? exact(title.value) : least(title.value),
  };
  return resultOf([root], bounds, ["bar chart, horizontal frame"]);
}

export function build(panel, box, doc, ctx) {
  const rules = rulesOf(panel);
  const leg = legendOf(panel, rules);
  if (!leg) return neutral("no legend member");
  if (rules.warning) return neutral("warning line");
  return orientationOf(panel, box) === "horizontal" ? horizontal(panel, box, rules) : vertical(panel, box, rules, leg);
}
