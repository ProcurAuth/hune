// The loading preview of the panel type `piechart` (9.11).
// The rules of this family are in the council file `piechart.md` (scratchpad/council). The loaded panel is drawn by web/js/panels/piechart.js.
//
// The preview uses the JSON document only. The document fixes: the legend (`options.legend`), the hole (`options.pie_type`), the column count of the
// legend (`legend.mode`, `legend.values`) and the warning line (`fields.overrides`). The data decide the item count, the names and the legend size,
// so the preview takes the LEAST legend (one item) and draws no mark in it. The sizes come from the CSS of the loaded pie (the same rules in the
// section `piechart` of panel-loading.css): the browser gives the same boxes, also after a resize or a late font.
//
// Tree: the root `.skel` is a flex column. It holds the optional warning line and the holder `.skel__holder` (the place of `.panel__plot`). The holder
// holds `.skel__pie` (the place of `.pie`): the chart box `.skel__plot` with the silhouette `.skel__ring`, and the legend box `.skel__legend`.
//
// Parts that the harness measures:
//   plot    the chart box. EXACT without a legend, when the least legend is the loaded legend, else MOST (the loaded plot is the same or smaller).
//   ring    the circle of the disc or the ring. The same class as the plot (the circle follows the plot box).
//   legend  the legend box. LEAST: the loaded legend has one item or more.
//   warning the warning line above the plot. EXACT.

import { EXACT, LEAST, MOST, PARTS, exact, least, most, node, partAttrs, px, resultOf, textWidth, unknown, warningNode } from "./common.js";

// One line of the legend: 12 px x 1.5714 (`--fs-sm` x `--lh-base`).
export const LINE = 12 * 1.5714;
const GAP_RIGHT = 12; // `--space-12`
const GAP_BOTTOM = 4; // `--space-4`
const MIN_RIGHT = 72; // `min-width` of the right legend
const MAX_RIGHT = 0.52; // `max-width` of the right legend
const MAX_BOTTOM = 0.42; // `max-height` of the bottom legend
const SWATCH = 10;
const COLUMN_GAP = 8;
const MARGIN = 8; // the ring is 8 px inside the plot box
const MIN_RING = 20;

// The width of a name in the legend. The legend is DOM text, so a hidden span with the CSS of the page measures it: a canvas gives another width,
// because the page switches the optical size of its font off (base.css). A node test has no document: it uses the measurer of `textWidth`.
let ruler = null;
function nameWidth(text) {
  if (typeof document === "undefined" || !document.body) return textWidth(text);
  if (!ruler || !ruler.isConnected) {
    ruler = document.createElement("span");
    ruler.setAttribute("aria-hidden", "true");
    for (const [name, value] of Object.entries({ position: "fixed", left: "-9999px", top: "0", visibility: "hidden", "white-space": "nowrap", "font-size": "var(--fs-sm)", "font-weight": "400" })) ruler.style.setProperty(name, value);
    document.body.append(ruler);
  }
  ruler.textContent = text;
  return ruler.getBoundingClientRect().width;
}

const legendOf = (options) => options.legend || { show: true, placement: "right", mode: "list", values: [] };

// The count of value columns of the legend (E8): the value column (a table, or `values` with `value`), then the percent column.
export function extraColumns(legend) {
  const values = Array.isArray(legend.values) ? legend.values : [];
  return (values.includes("value") || legend.mode === "table" ? 1 : 0) + (values.includes("percent") ? 1 : 0);
}

// The literal name of every item (E7), or null. It holds when the items come from the fields (`reduce.values` is false), the default display name
// is a text with no template mark, and no override sets a display name. The loaded code then gives this text to every item.
export function literalName(panel) {
  const reduce = panel.options && panel.options.reduce;
  if (!reduce || reduce.values !== false) return null;
  const fields = panel.fields || {};
  const name = fields.defaults ? fields.defaults.display_name : undefined;
  if (typeof name !== "string" || name === "" || name.includes("$") || name.includes("[[")) return null;
  const overrides = Array.isArray(fields.overrides) ? fields.overrides : [];
  if (overrides.some((o) => o && o.set && Object.prototype.hasOwnProperty.call(o.set, "display_name"))) return null;
  return name;
}

// The numbers of the preview, as the CSS gives them (E4 to E6, E12), for a pie box of `box`. The class tells how the loaded plot compares: EXACT, or
// MOST (the loaded plot is the same or smaller). A warning line above the plot makes the pie box lower than the preview box: the caller then
// keeps the widths and drops the heights, which the browser gives.
export function geometry({ legend, box, name = null }) {
  const W = box.w;
  const H = box.h;
  const extra = extraColumns(legend);
  let plot;
  let cls;
  let legendRect = null;
  if (!legend.show) {
    plot = { x: 0, y: 0, w: W, h: H };
    cls = EXACT;
  } else if (legend.placement === "bottom") {
    const line = Math.min(LINE, MAX_BOTTOM * H);
    plot = { x: 0, y: 0, w: W, h: Math.max(0, H - GAP_BOTTOM - line) };
    legendRect = { x: 0, y: H - line, w: W, h: line };
    // The cap of 42 % binds for every line count when it is under one line: then the legend is as high as the cap whatever the data.
    cls = MAX_BOTTOM * H <= LINE ? EXACT : MOST;
  } else {
    const cap = Math.max(MIN_RIGHT, MAX_RIGHT * W);
    const content = name === null ? 0 : SWATCH + COLUMN_GAP * (1 + extra) + nameWidth(name);
    const lw = Math.min(cap, Math.max(MIN_RIGHT, content));
    plot = { x: 0, y: 0, w: Math.max(0, W - GAP_RIGHT - lw), h: H };
    legendRect = { x: W - lw, y: (H - LINE) / 2, w: lw, h: LINE };
    // Exact when the width of the legend is fixed whatever the data: the cap is not above the floor, or the name fixes the content and no value column adds to it.
    cls = MAX_RIGHT * W <= MIN_RIGHT || (name !== null && extra === 0) ? EXACT : MOST;
  }
  const d = Math.max(MIN_RING, Math.min(plot.w, plot.h) - 2 * MARGIN);
  return { plot, legend: legendRect, cls, d };
}

export function build(panel, box, doc, ctx) {
  const o = panel.options || {};
  const legend = legendOf(o);
  const warning = warningNode(panel);
  const name = legend.show && legend.placement !== "bottom" ? literalName(panel) : null;
  const g = geometry({ legend, box, name });
  // The cap of 42 % and the cap of 52 % do not depend on the height above the plot, so the class holds with a warning line too.
  const cls = g.cls;
  const donut = o.pie_type === "donut";
  const notes = ["piechart", legend.show ? `legend ${legend.placement === "bottom" ? "bottom" : "right"}` : "legend off", donut ? "donut" : "pie"];
  if (warning) notes.push("warning");
  if (name !== null) notes.push("literal name");

  const ring = node(`skel__ring${donut ? " skel__ring--donut" : ""}`, {}, [], "div", partAttrs("ring", cls));
  const plot = node("skel__plot", {}, [ring], "div", partAttrs(PARTS.PLOT, cls));
  const kids = [plot];
  if (legend.show) {
    if (legend.placement === "bottom") kids.push(node("skel__legend skel__legend--bottom", {}, [], "div", partAttrs(PARTS.LEGEND, LEAST)));
    else {
      // The least row of the loaded grid, hidden: a swatch cell, a name cell (the width of the literal name, else empty) and the empty value cells.
      const cells = [node("skel__sw"), node("skel__name", name === null ? {} : { "--w": px(nameWidth(name)) })];
      for (let i = 0; i < extraColumns(legend); i++) cells.push(node("skel__val"));
      kids.push(node(`skel__legend skel__legend--right skel__legend--c${extraColumns(legend)}`, {}, cells, "div", partAttrs(PARTS.LEGEND, LEAST)));
    }
  }
  const pie = node(`skel__pie skel__pie--${legend.show && legend.placement === "bottom" ? "bottom" : "right"}`, {}, kids);
  const holder = node("skel__holder", {}, [pie]);

  const bound = cls === EXACT ? exact : most;
  const bounds = { plotW: bound(g.plot.w), plotH: warning ? unknown() : bound(g.plot.h), ringD: warning ? unknown() : bound(g.d) };
  if (g.legend) {
    bounds.legendW = least(g.legend.w);
    bounds.legendH = warning ? unknown() : least(g.legend.h);
  }
  return resultOf(warning ? [warning, holder] : [holder], bounds, notes);
}
