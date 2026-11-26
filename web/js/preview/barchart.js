// The loading preview of the panel type `barchart` (9.11).
// The rules are in 9.11 of docs/04 (the council file `barchart.md`). The loaded panel is drawn by web/js/panels/barchart.js (vertical, the chart
// engine) and barchart_h.js (horizontal, DOM). The preview uses ONLY the JSON members of the panel. It never reads a query.
//
// The vertical form is the frame of a chart of the engine (timeseries.js `chartFrame`): the same legend, the same rounding chain. The horizontal form
// uses the markup and the classes of the loaded `.hbar`, so its CSS gives the sizes. Both forms follow the shared legend object of 4.26: a list or a
// table, hidden entries, the cap of 35 % of the height and the fallback under the plot. Neither form draws a category or a value: the document gives
// no name and no number. The plot has decorative bars with heights that vary, in neutral tones. They claim no count and no value.
//
// The parts: `plot` (the plot of the engine, or `.hbar__grid`), `legend`, `head` and `head-label` (a table legend), and `tick` (the label "100%" of a
// percent stack of the horizontal form, as a bar).

import { CONTAIN, EXACT, LEAST, PARTS, exact, least, most, neutral, node, partAttrs, px, resultOf, svgNode, unknown } from "./common.js";
import { BODY_ROW, CAP_SHARE, HEAD_ROW, LINE, LIST_PAD, chartFrame, domWidth, gutterOf, gutterWidth, leastBeside, legendNode, legendOf, marksOf, rulesOf, tick0OfBars, titleProven, zeroLabel } from "./timeseries.js";

const PAD_TOP = 8;
const PAD_RIGHT = 8;
const BAND = 21;
const H_LABEL = 22; // the least width of the column of the category names: 6 px, no text, 16 px
const H_TITLE = 28;
const H_LEGEND_SHARE = 0.3;
const H_MIN_PLOT = 220;
const H_GAP = 8;
const H_ROW = 28; // the decorative bars of the horizontal form: a row of 28 px with a bar of 18 px
const H_BAR = 18;
const H_LENGTHS = [78, 54, 66, 40, 58, 30, 70, 48];
const H_TICK_TOP = 7; // the label "100%" has its line 7 px under the top of the band (11 px with a border)
const H_TICK_LINE = 14;

const finite = (v) => typeof v === "number" && Number.isFinite(v);

/** The renderer that draws the panel: `horizontal` gives H, `auto` gives H when the box is higher than wide (a tie gives V). */
export function orientationOf(panel, box) {
  const o = panel.options && panel.options.orientation;
  if (o === "horizontal") return "horizontal";
  if (o === "auto" && box.w < box.h) return "horizontal";
  return "vertical";
}

// Every series is hidden from the plot (`hide_from.viz`) and no override shows one: the plot draws no bar.
const barsHidden = (rules) => rules.D.hide_from && rules.D.hide_from.viz === true && !rules.touched("hide_from");

// The vertical form: the frame of the engine. The value axis always shows (the top margin is 8) and a bar chart has no right axis.
function vertical(panel, box, rules, leg) {
  const turned = finite(panel.options.x_tick_rotation) && panel.options.x_tick_rotation !== 0;
  const tick0 = tick0OfBars(rules) ? zeroLabel(rules.D) : null;
  const left = gutterOf(rules, { sure: true, tick0 });
  const right = { value: PAD_RIGHT, exact: true };
  const style = barsHidden(rules) ? null : { kind: "bars" };
  return chartFrame(box, rules, leg, { top: PAD_TOP, band: BAND, bandExact: !turned, left, right, pill: true, marks: (plotW) => marksOf(style, plotW, panel.id), notes: ["bar chart, vertical frame"] });
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

/**
 * The legend of the horizontal form (9.19): the same shared object as the vertical form. Returns the size that the legend takes: { draw, fallback, w, h,
 * hExact, never }. `w` is the least width beside the plot and `h` the least height under it. `never`: the legend cannot fall back under the plot.
 */
export function hLegendOf(leg, W, H) {
  const cap = Math.floor(H * CAP_SHARE);
  const out = { draw: false, fallback: false, w: 0, h: 0, hExact: true, never: true, cap };
  if (leg.state !== "shown") return out;
  out.draw = true;
  const table = leg.mode === "table";
  const tableH = Math.min(HEAD_ROW + BODY_ROW, cap);
  if (leg.placement === "bottom") return { ...out, h: table ? tableH : LIST_PAD + Math.min(LINE, cap), hExact: cap <= (table ? HEAD_ROW + BODY_ROW : LINE) };
  // A list has at most 30 % of the width; a table has no cap. The least width: the room of one empty entry, or the heads.
  const lmin = table ? leastBeside(leg) : Math.min(40 + gutterWidth(), H_LEGEND_SHARE * W);
  if (W < Math.floor(lmin) + H_MIN_PLOT + H_GAP) {
    return { ...out, fallback: true, h: table ? tableH : Math.min(8 + LINE, cap + LIST_PAD), hExact: false, never: false };
  }
  return { ...out, w: lmin, h: 0, hExact: false, never: !table && W - Math.ceil(H_LEGEND_SHARE * W) >= H_MIN_PLOT + H_GAP };
}

// The decorative bars of the horizontal form: rows with bars of different lengths, in percent of the track.
function hMarks(gridH) {
  const n = Math.max(1, Math.min(10, Math.floor(gridH / H_ROW)));
  const rects = [];
  for (let i = 0; i < n; i++) rects.push(node("skel__bar", {}, [], "rect", { x: "0", y: String(i * H_ROW + (H_ROW - H_BAR) / 2), width: String(H_LENGTHS[i % H_LENGTHS.length]), height: String(H_BAR) }));
  return svgNode(100, n * H_ROW, rects, "skel__svg skel__bars");
}

// The horizontal form. The legend follows the shared object of the document (9.19).
function horizontal(panel, box, rules, leg) {
  const band = bandOf(rules);
  const title = titleOf(rules);
  const W = box.w;
  const H = box.h;
  const hl = hLegendOf(leg, W, H);
  const gridRight = W - hl.w - PAD_RIGHT;
  const gridBottom = H - hl.h - band.value - title.value;
  const bottomExact = hl.hExact && band.exact && title.exact;
  // A legend beside the plot that can fall back under it moves the right edge out and the bottom edge up: the right edge is not claimed then.
  const claimRight = !(hl.draw && hl.w > 0 && !hl.never);
  const edges = `lt${claimRight ? "r" : ""}b`;
  // A panel so short that the plot has no height has no plot to measure: the loaded panel draws no box for it either.
  const gridH = gridBottom - PAD_TOP;
  const grid = node("hbar__grid", {}, barsHidden(rules) || gridH <= 0 ? [] : [hMarks(gridH)], "div", gridH > 0 ? partAttrs(PARTS.PLOT, `${CONTAIN}:${edges}`) : {});
  const body = node("hbar__body", {}, [grid]);
  const axisKids = [];
  // The label "100%" of a percent stack is at the right end of the axis: exact when the legend takes no room beside the plot and the band has a place.
  const percent = panel.options.stacking === "percent" && hl.w === 0 && bottomExact && !rules.touched("axis_border");
  if (percent) {
    const t = (rules.D.axis_border === true ? 4 : 0) + H_TICK_TOP + (H_TICK_LINE - 8) / 2 - 0.25;
    axisKids.push(node("skel__tick", { "--w": px(domWidth("100%")), "--t": px(t) }, [], "div", partAttrs("tick", EXACT)));
  }
  const plotKids = [body, node("hbar__axis", {}, axisKids)];
  if (title.value) plotKids.push(node("hbar__axisTitle", {}, []));
  const kids = [node("hbar__plot", {}, plotKids)];
  if (hl.draw) {
    // Beside the plot, the legend takes the whole height and may fall back: only a legend that cannot move has a measured part.
    const place = hl.w > 0 && !hl.never ? null : hl.hExact && hl.w === 0 ? EXACT : LEAST;
    kids.push(legendNode(leg, { cap: hl.cap, place, pill: true, kind: "hbar" }));
  }
  const hostA = { "data-legend-placement": leg.placement };
  if (hl.fallback) hostA["data-legend-fallback"] = "";
  const root = node("hbar", { "--hbar-label": px(H_LABEL), "--band": px(band.value) }, kids, "div", hostA);
  const bounds = {
    plotTop: exact(PAD_TOP),
    plotLeft: least(H_LABEL),
    plotRight: claimRight ? most(gridRight) : unknown(),
    plotBottom: most(gridBottom),
    legendHeight: hl.draw ? (hl.hExact ? exact(hl.h) : least(hl.h)) : leg.state === "maybe" ? least(0) : exact(0),
    legendWidth: hl.w > 0 ? least(hl.w) : unknown(),
    band: band.exact ? exact(band.value) : least(band.value),
    title: title.exact ? exact(title.value) : least(title.value),
  };
  return resultOf([node("skel__host", {}, [root])], bounds, ["bar chart, horizontal frame"]);
}

export function build(panel, box, doc, ctx) {
  const rules = rulesOf(panel);
  const leg = legendOf(panel, rules);
  if (!leg) return neutral("no legend member");
  if (rules.warning) return neutral("warning line");
  return orientationOf(panel, box) === "horizontal" ? horizontal(panel, box, rules, leg) : vertical(panel, box, rules, leg);
}
