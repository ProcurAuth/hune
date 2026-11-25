// The wait of a panel (9.11): which sign a panel shows while it waits, the preview of each panel type, and the test that tells
// if a new answer holds the data that the panel already draws. Pure module: no DOM access.

import { formatNumber, joinParts } from "./units.js";

// The states of a panel that wait for a result.
export const WAITING = new Set(["idle", "queued", "loading"]);

// What the body shows when the state is a waiting state (9.11):
//   "first"  nothing is on the screen: the panel never had a result, or its last try failed. It shows the preview of its type and the dots.
//   "reload" the panel has content, or its "No data" text. It keeps them as they are and shows the dots only.
//   ""       the panel does not wait.
// `shown` tells what is on the screen: "content" (a drawn panel), "empty" (the text "No data") or "nothing".
export function waitMode(state, shown) {
  if (!WAITING.has(state)) return "";
  return shown === "content" || shown === "empty" ? "reload" : "first";
}

// The preview of each panel type. A panel type that draws no queried data (a text) has none.
const KIND_OF_TYPE = {
  timeseries: "series",
  xychart: "series",
  barchart: "bars",
  stat: "stat",
  gauge: "gauge",
  bargauge: "bar-gauge",
  table: "table",
  piechart: "pie",
  heatmap: "heatmap",
  histogram: "histogram",
  state_timeline: "timeline",
  status_history: "timeline",
  logs: "logs",
  geomap: "geomap",
};

// The kind of preview of a panel type, or "" when the type has none.
export function skeletonKind(type) {
  return Object.prototype.hasOwnProperty.call(KIND_OF_TYPE, type) ? KIND_OF_TYPE[type] : "";
}

// ---- The preview: a tree of shapes that follows the options of the panel ---------------------------------------------------------------
// The preview shows the structure of the panel before its data come: the lines or the bars of a chart, the axes, the legend. It reserves the
// places that the real panel takes, so that the plot keeps its size when the data arrive. It is a tree of nodes { t, c, v, a, k }: the tag
// (`div`, `svg` or `path`), the class, the custom properties (set with the CSSOM, never a style attribute), the attributes (an SVG path) and
// the children. panel_preview.js draws the tree. The sizes in px are those of the real panels (docs/04 9.11): they are measured on the charts.

const node = (c, v = {}, k = [], t = "div", a = {}) => ({ t, c, v, a, k });
const px = (n) => `${Math.round(n * 10) / 10}px`;
const pct = (n) => `${Math.round(n * 10) / 10}%`;
const range = (n) => Array.from({ length: n }, (_, i) => i);

// The sizes of the real charts, in px. The chart is inside the padding of the body (8 px). CHAR is the width of one character of an axis label
// (12 px text). The left gutter of a plot is the widest label and 9 px. The x labels under a plot take 21 px.
export const SIZES = {
  GUTTER_PAD: 9,
  EDGE: 8,
  TOP: 8,
  X_BAND: 21,
  CHAR: 7.6,
  // The legend of a chart (time series, bars, histogram, timeline): the height under the plot, the width beside it with its gap of 8 px.
  LEGEND_BOTTOM: 30.8,
  LEGEND_RIGHT_LIST: 141,
  LEGEND_RIGHT_BASE: 158,
  LEGEND_RIGHT_COLUMN: 58,
  // A legend in table mode: a head of 28 px and rows of 22 px. At the bottom it never takes more than 35 % of the panel (css).
  LEGEND_TABLE_HEAD: 28,
  LEGEND_TABLE_ROW: 22,
  LEGEND_TABLE_ROWS: 4,
  // The legend of a pie: one line of 18.8 px and a gap of 4 px under the plot, or a column with a gap of 12 px beside it.
  PIE_BOTTOM: 22.8,
  PIE_RIGHT_LIST: 84,
  PIE_RIGHT_BASE: 70,
  PIE_RIGHT_COLUMN: 40,
  HBAR_LABELS: 78,
  SCALE: 46,
};

const WIDTHS = [56, 72, 48, 64, 60, 52];

const optionsOf = (panel) => (panel && panel.options) || {};
const defaultsOf = (panel) => (panel && panel.fields && panel.fields.defaults) || {};

// The legend of the options: { place, mode, items, columns } or null when the legend is off. A panel with no `legend` member shows a list at the bottom.
export function legendSpec(panel) {
  const lg = optionsOf(panel).legend;
  if (lg && lg.show === false) return null;
  const mode = lg && lg.mode === "table" ? "table" : "list";
  const place = lg && lg.placement === "right" ? "right" : "bottom";
  const values = lg && Array.isArray(lg.values) ? lg.values.length : 0;
  return { place, mode, items: mode === "table" ? 3 : 4, columns: mode === "table" ? values : 0 };
}

// The size that the legend takes from the plot, in px: the height under the plot, or the width beside it. A pie has a legend of its own look.
export function legendSize(spec, pie = false) {
  if (!spec) return 0;
  if (spec.place === "bottom") {
    if (spec.mode === "table") return SIZES.LEGEND_TABLE_HEAD + SIZES.LEGEND_TABLE_ROWS * SIZES.LEGEND_TABLE_ROW;
    return pie ? SIZES.PIE_BOTTOM : SIZES.LEGEND_BOTTOM;
  }
  const columns = Math.max(1, spec.columns);
  if (pie) return spec.mode === "table" ? SIZES.PIE_RIGHT_BASE + SIZES.PIE_RIGHT_COLUMN * columns : SIZES.PIE_RIGHT_LIST;
  return spec.mode === "table" ? SIZES.LEGEND_RIGHT_BASE + SIZES.LEGEND_RIGHT_COLUMN * columns : SIZES.LEGEND_RIGHT_LIST;
}

// The width of a y label of the unit of the panel: the text of a typical tick (500) in the unit and the decimals.
export function labelWidth(panel) {
  const d = defaultsOf(panel);
  const text = joinParts(formatNumber(500, d.unit || "none", Number.isFinite(d.decimals) ? d.decimals : null));
  return Math.ceil(text.length * SIZES.CHAR);
}

// The gutters of a plot with a value axis: { left, right, bottom, top } in px. The y axis can be hidden (`axis_placement`), have a width
// (`axis_width`) or sit at the right for a field that an override sets (the right gutter then has the room of the labels).
export function plotGutters(panel, { yAxis = true, label = labelWidth(panel) } = {}) {
  const d = defaultsOf(panel);
  const overrides = (panel && panel.fields && panel.fields.overrides) || [];
  const right = overrides.some((o) => o && o.set && o.set.axis_placement === "right");
  const hidden = !yAxis || d.axis_placement === "hidden";
  const own = Number.isFinite(d.axis_width) ? d.axis_width : label + SIZES.GUTTER_PAD;
  return {
    left: hidden ? SIZES.EDGE : own,
    right: right ? label + SIZES.GUTTER_PAD : SIZES.EDGE,
    bottom: SIZES.X_BAND,
    // With no y axis the plot starts at the top edge: the 8 px above the first label are not needed.
    top: hidden ? 0 : SIZES.TOP,
  };
}

// A smooth path through the points (a Catmull-Rom curve as cubic Bézier segments) in a box of 100 by 100.
export function smoothPath(ys, { close = false } = {}) {
  const r = (n) => Math.round(n * 10) / 10;
  const pts = ys.map((y, i) => [r((i * 100) / (ys.length - 1)), y]);
  let d = `M${pts[0][0]} ${pts[0][1]}`;
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[Math.max(0, i - 1)];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[Math.min(pts.length - 1, i + 2)];
    const c1 = [r(p1[0] + (p2[0] - p0[0]) / 6), r(p1[1] + (p2[1] - p0[1]) / 6)];
    const c2 = [r(p2[0] - (p3[0] - p1[0]) / 6), r(p2[1] - (p3[1] - p1[1]) / 6)];
    d += `C${c1.join(" ")} ${c2.join(" ")} ${p2.join(" ")}`;
  }
  return close ? `${d}L100 100L0 100Z` : d;
}

const LINES = [
  [62, 55, 58, 48, 50, 38, 44, 32, 40, 50, 56, 44, 36, 42, 34, 46],
  [80, 76, 78, 72, 74, 68, 72, 66, 70, 74, 76, 72, 68, 70, 66, 72],
  [34, 38, 30, 40, 34, 44, 36, 32, 40, 34, 30, 38, 32, 36, 28, 34],
];

const svgBox = (children, viewBox = "0 0 100 100") => node("skel__svg", {}, children, "svg", { viewBox, preserveAspectRatio: "none" });
const linePath = (ys) => node("skel__line", {}, [], "path", { d: smoothPath(ys) });
const areaPath = (ys) => node("skel__fill", {}, [], "path", { d: smoothPath(ys, { close: true }) });

// The legend: items with a swatch and a name bar. `swatch` is "pill" (a series), "dot" (a part of a pie) or "square" (a state).
function legendNode(spec, swatch) {
  if (spec.mode === "table") {
    const row = (cls) => node(`skel__lrow ${cls}`.trim(), {}, [
      node("skel__lcell skel__lname", {}, [node(`skel__sw skel__sw--${swatch}`), node("skel__txt", { "--w": px(52) })]),
      ...range(Math.max(0, spec.columns)).map(() => node("skel__lcell skel__lval", {}, [node("skel__txt", { "--w": px(30) })])),
    ]);
    return node("skel__legend", {}, [row("skel__lhead"), row(""), row("")]);
  }
  return node("skel__legend", {}, range(spec.items).map((i) => node("skel__item", {}, [node(`skel__sw skel__sw--${swatch}`), node("skel__txt", { "--w": px(WIDTHS[i % WIDTHS.length]) })])));
}

// The plot area of a chart: the grid, the labels of the axes and the shapes that `draw()` gives. The area is placed with the gutters in px.
function plotNode(g, { grid = true, yLabels = 5, xLabels = 6, labelPx = 0, draw }) {
  const area = node("skel__area", { "--left": px(g.left), "--right": px(g.right), "--top": px(g.top), "--bottom": px(g.bottom) }, [
    ...(grid ? [0, 25, 50, 75, 100].map((y) => node("skel__grid", { "--y": pct(y) })) : []),
    ...draw(),
  ]);
  const ys = g.left > SIZES.EDGE && yLabels > 0 && labelPx > 0
    ? [node("skel__ycol", { "--left": px(g.left), "--top": px(g.top), "--bottom": px(g.bottom) }, range(yLabels).map((i) => node("skel__ylab", { "--y": pct((i * 100) / (yLabels - 1)), "--w": px(Math.max(8, labelPx - 4 - (i % 2) * 4)) })))]
    : [];
  const xs = xLabels > 0
    ? [node("skel__xrow", { "--left": px(g.left), "--right": px(g.right), "--h": px(g.bottom) }, range(xLabels).map((i) => node("skel__xlab", { "--x": pct((i * 100) / (xLabels - 1)) })))]
    : [];
  return node("skel__chart", {}, [...ys, area, ...xs]);
}

// The frame of a preview: the plot, then the legend under it or beside it. `size` is the room of the legend in px.
function frame(kind, plot, legend, size) {
  const place = legend ? legend.place : "none";
  return node("skel", { "--legend": px(size) }, legend ? [plot, legend.node] : [plot], "div", { "data-kind": kind, "data-legend": place, ...(legend ? { "data-mode": legend.mode } : {}) });
}

// ---- The preview of each panel type ----------------------------------------------------------------------------------------------

// A time series or an xy chart: lines (or bars, `draw: "bars"`), the axes and the legend.
function seriesPreview(panel) {
  const o = optionsOf(panel);
  const g = plotGutters(panel);
  const spec = legendSpec(panel);
  const stacked = o.stacking === "normal" || o.stacking === "percent";
  const draw = () => {
    if (o.draw !== "bars") return [svgBox([areaPath(LINES[0]), ...LINES.map((ys) => linePath(ys))])];
    return [44, 52, 46, 60, 40, 56, 48, 62, 42, 54, 46, 58].map((h, i) => node(`skel__bar${stacked ? " skel__bar--stack" : ""}`, { "--x": pct(1.5 + i * 8.2), "--w": pct(5.4), "--h": pct(h) }));
  };
  return frame("series", plotNode(g, { labelPx: labelWidth(panel), draw }), spec && { ...spec, node: legendNode(spec, "pill") }, legendSize(spec));
}

// A bar chart: a vertical one is a chart with bars and category labels, a horizontal one is rows with a label column, bars and a value axis.
function barsPreview(panel) {
  const o = optionsOf(panel);
  const spec = legendSpec(panel);
  const stacked = o.stacking === "normal" || o.stacking === "percent";
  const legend = spec && { ...spec, node: legendNode(spec, "pill") };
  if (o.orientation === "horizontal") {
    const widths = [86, 70, 52, 38, 26];
    const rows = widths.map((w, i) => node("skel__hrow", { "--i": String(i), "--n": String(widths.length) }, [
      node("skel__hname", {}, [node("skel__txt", { "--w": px(34 + ((i * 11) % 30)) })]),
      node("skel__htrack", {}, [
        node(`skel__hbar${stacked ? " skel__hbar--stack" : ""}`, { "--w": pct(w) }),
        ...(o.show_values === "never" ? [] : [node("skel__hval", { "--x": pct(w) })]),
      ]),
    ]));
    const area = node("skel__hbars", { "--label": px(SIZES.HBAR_LABELS), "--top": px(SIZES.TOP), "--bottom": px(SIZES.X_BAND), "--right": px(SIZES.EDGE) }, [
      node("skel__vgrids", { "--label": px(SIZES.HBAR_LABELS) }, range(6).map((i) => node("skel__vgrid", { "--x": pct(i * 20) }))),
      ...rows,
    ]);
    const axis = node("skel__xrow", { "--left": px(SIZES.HBAR_LABELS), "--right": px(SIZES.EDGE), "--h": px(SIZES.X_BAND) }, range(6).map((i) => node("skel__xlab", { "--x": pct(i * 20) })));
    return frame("bars", node("skel__chart", {}, [area, axis]), legend, legendSize(spec));
  }
  const g = plotGutters(panel);
  const draw = () => [44, 64, 52, 74, 40, 60, 48].map((h, i) => node(`skel__bar${stacked ? " skel__bar--stack" : ""}`, { "--x": pct(6 + i * 13.2), "--w": pct(8.4), "--h": pct(h) }));
  return frame("bars", plotNode(g, { labelPx: labelWidth(panel), draw }), legend, legendSize(spec));
}

// A pie chart: a ring (donut) or a disc (pie) in the middle of the plot, and the legend under it or beside it.
function piePreview(panel) {
  const spec = legendSpec(panel);
  const disc = optionsOf(panel).pie_type === "pie";
  const plot = node("skel__chart skel__chart--pie", {}, [node(`skel__pie${disc ? " skel__pie--disc" : ""}`)]);
  return frame("pie", plot, spec && { ...spec, node: legendNode(spec, "dot") }, legendSize(spec, true));
}

// A stat: the value and a caption, and a line under them when the graph mode is on.
function statPreview(panel) {
  const o = optionsOf(panel);
  const graph = o.graph_mode === "area" || o.graph_mode === "line";
  const spark = [70, 62, 66, 52, 58, 44, 50, 40, 46, 56, 62, 50, 42, 48, 40, 44];
  const k = [
    node("skel__value", { "--w": "40%", "--y": graph ? "22%" : "34%", "--h": graph ? "24%" : "30%" }),
    node("skel__caption", { "--w": "24%", "--y": graph ? "52%" : "70%" }),
  ];
  if (graph) k.push(node("skel__spark", {}, [svgBox([...(o.graph_mode === "area" ? [areaPath(spark)] : []), linePath(spark)])]));
  return node("skel", {}, k, "div", { "data-kind": "stat", "data-legend": "none" });
}

// A gauge: an arc open at its foot with a value. Several gauges when each value of the data has one (`reduce.values`), with a name under each.
function gaugePreview(panel) {
  const many = !!(optionsOf(panel).reduce && optionsOf(panel).reduce.values === true);
  const gauge = (fill) => node("skel__gauge", { "--fill": String(fill) }, [
    node("skel__svg", {}, [
      node("skel__track", {}, [], "path", { d: "M14 78A40 40 0 1 1 86 78" }),
      node("skel__arc", {}, [], "path", { d: "M14 78A40 40 0 1 1 86 78", pathLength: "100" }),
    ], "svg", { viewBox: "0 0 100 100" }),
    node("skel__value", { "--w": "36%", "--y": "46%", "--h": "14%" }),
    ...(many ? [node("skel__caption", { "--w": "40%", "--y": "88%" })] : []),
  ]);
  return node("skel", { "--n": many ? "3" : "1" }, (many ? [60, 45, 78] : [60]).map(gauge), "div", { "data-kind": "gauge", "data-legend": "none" });
}

// A bar gauge: tracks with a fill, in rows or in columns (`orientation`), one per value when `reduce.values` is on.
function barGaugePreview(panel) {
  const o = optionsOf(panel);
  const many = !!(o.reduce && o.reduce.values === true);
  const fills = many ? [62, 38, 78, 50] : [62];
  const lcd = o.display_mode === "lcd";
  const items = fills.map((f, i) => node(`skel__bg${lcd ? " skel__bg--lcd" : ""}`, { "--fill": pct(f), "--i": String(i) }, [
    node("skel__bgname", {}, [node("skel__txt", { "--w": px(40 + (i % 2) * 14) })]),
    node("skel__bgtrack", {}, [node("skel__bgfill")]),
  ]));
  return node("skel", { "--n": String(fills.length) }, items, "div", { "data-kind": "bar-gauge", "data-legend": "none", "data-orient": o.orientation === "vertical" ? "vertical" : "horizontal" });
}

// A table: a header row and rows. The number of columns follows the fields that the document names (at least 3, at most 8).
function tablePreview(panel) {
  const overrides = (panel.fields && panel.fields.overrides) || [];
  const named = new Set(overrides.filter((x) => x && x.match && x.match.by === "name").map((x) => x.match.value));
  const cols = Math.min(8, Math.max(3, named.size));
  const footer = overrides.some((x) => x && x.set && x.set.footer);
  const rows = range(10).map((r) => node("skel__row", {}, range(cols).map((c) => node("skel__td", {}, [node("skel__cell", { "--w": pct(46 + ((r * 7 + c * 13) % 40)) })]))));
  return node("skel", { "--cols": String(cols) }, [
    node("skel__thead", {}, range(cols).map((c) => node("skel__th", {}, [node("skel__txt", { "--w": px(28 + ((c * 9) % 22)) })]))),
    node("skel__rows", {}, rows),
    ...(footer ? [node("skel__tfoot", {}, range(cols).map((c) => node("skel__td", {}, c === 0 ? [node("skel__txt", { "--w": px(28) })] : [])))] : []),
  ], "div", { "data-kind": "table", "data-legend": "none" });
}

// A heatmap: a grid of cells in a plot with a y axis and an x axis, and a color scale under it when the legend shows.
function heatmapPreview(panel) {
  const o = optionsOf(panel);
  const g = plotGutters(panel, { yAxis: o.y_axis !== "hidden" });
  const draw = () => range(5).flatMap((r) => range(10).map((c) => node("skel__hcell", { "--c": pct(c * 10), "--r": pct(r * 20), "--d": String((c + r) % 6) })));
  const scale = o.show_legend === true ? { place: "bottom", mode: "scale", node: node("skel__scale", { "--gutter": px(g.left) }, [node("skel__ramp"), node("skel__ends", {}, [node("skel__txt", { "--w": px(14) }), node("skel__txt", { "--w": px(20) })])]) } : null;
  return frame("heatmap", plotNode(g, { labelPx: labelWidth(panel), draw }), scale, scale ? SIZES.SCALE : 0);
}

// A histogram: bars in the shape of a bell, with the axes and the legend when it shows.
function histogramPreview(panel) {
  const spec = legendSpec(panel);
  const counts = Math.ceil(4 * SIZES.CHAR); // the y axis counts the values of a bucket: no unit, a count of four digits
  const g = plotGutters(panel, { label: counts });
  const draw = () => [12, 22, 38, 56, 72, 60, 40, 24, 14, 8].map((h, i) => node("skel__bar skel__bar--flush", { "--x": pct(i * 10), "--w": pct(9), "--h": pct(h) }));
  return frame("histogram", plotNode(g, { labelPx: counts, draw }), spec && { ...spec, node: legendNode(spec, "pill") }, legendSize(spec));
}

// A state timeline: rows of segments with the names of the rows at the left, and an x axis.
function timelinePreview() {
  const rows = [
    [[0, 30], [34, 22], [60, 40]],
    [[0, 16], [20, 44], [68, 32]],
    [[0, 38], [42, 18], [64, 36]],
    [[0, 24], [28, 30], [62, 38]],
  ];
  const lane = (row, r) => node("skel__lane", { "--i": String(r), "--n": String(rows.length) }, [
    node("skel__hname", {}, [node("skel__txt", { "--w": px(32 + ((r * 13) % 24)) })]),
    node("skel__ltrack", {}, row.map(([x, w], i) => node("skel__seg", { "--x": pct(x), "--w": pct(w - 2), "--d": String(r + i) }))),
  ]);
  const area = node("skel__hbars", { "--label": px(SIZES.HBAR_LABELS), "--top": px(SIZES.TOP), "--bottom": px(SIZES.X_BAND), "--right": px(SIZES.EDGE) }, rows.map(lane));
  const axis = node("skel__xrow", { "--left": px(SIZES.HBAR_LABELS), "--right": px(SIZES.EDGE), "--h": px(SIZES.X_BAND) }, range(6).map((i) => node("skel__xlab", { "--x": pct(i * 20) })));
  // The legend of a timeline lists the states that the data hold: the preview cannot know them, so it reserves no place for it.
  return frame("timeline", node("skel__chart", {}, [area, axis]), null, 0);
}

// Logs: rows with a time, a level and a message, like the real rows. The time shows when the option says so.
function logsPreview(panel) {
  const time = optionsOf(panel).show_time !== false;
  const rows = [70, 58, 76, 50, 64, 44, 68, 56, 72, 48].map((w) => node("skel__log", {}, [
    ...(time ? [node("skel__ltime")] : []),
    node("skel__llevel"),
    node("skel__lmsg", { "--w": pct(w) }),
  ]));
  return node("skel", {}, rows, "div", { "data-kind": "logs", "data-legend": "none" });
}

// A map: two shapes of land and some markers.
function geomapPreview() {
  const blob = (x, y, w, h, d) => node("skel__blob", { "--x": pct(x), "--y": pct(y), "--w": pct(w), "--h": pct(h), "--d": String(d) });
  const pin = (x, y, d) => node("skel__pin", { "--x": pct(x), "--y": pct(y), "--d": String(d) });
  const pins = [[30, 40], [48, 58], [62, 44], [70, 66], [38, 70]].map(([x, y], i) => pin(x, y, i));
  return node("skel", {}, [blob(14, 16, 46, 62, 0), blob(44, 28, 40, 54, 1), ...pins], "div", { "data-kind": "geomap", "data-legend": "none" });
}

const BUILDERS = {
  series: seriesPreview,
  bars: barsPreview,
  pie: piePreview,
  stat: statPreview,
  gauge: gaugePreview,
  "bar-gauge": barGaugePreview,
  table: tablePreview,
  heatmap: heatmapPreview,
  histogram: histogramPreview,
  timeline: timelinePreview,
  logs: logsPreview,
  geomap: geomapPreview,
};

// The preview of a panel: its tree of nodes, or null for a panel type that draws no queried data (a text).
export function previewOf(panel) {
  const kind = panel ? skeletonKind(panel.type) : "";
  return kind ? BUILDERS[kind](panel) : null;
}

const COLUMN_CAP = 500000;

// True when two answers hold the data that a panel draws: the same columns, the same interval and the same values in the same order.
// `a` and `b` are the lists { ref, frame } of the answers. The members `query_id`, `stats`, `sql` and `etag` change with each request and
// are not data: the panel does not draw them. A frame of more than 500,000 cells counts as different, so the test stays cheap.
export function sameFrames(a, b) {
  if (!a || !b || a.length !== b.length) return false;
  let cells = 0;
  for (let i = 0; i < a.length; i++) {
    if (a[i].ref !== b[i].ref) return false;
    const x = a[i].frame;
    const y = b[i].frame;
    if (x === y) continue;
    if (!x || !y) return false;
    if (x.interval_ms !== y.interval_ms || x.rows !== y.rows) return false;
    if (JSON.stringify(x.columns) !== JSON.stringify(y.columns)) return false;
    const vx = x.values || [];
    const vy = y.values || [];
    if (vx.length !== vy.length) return false;
    for (let c = 0; c < vx.length; c++) {
      const p = vx[c];
      const q = vy[c];
      if (p === q) continue;
      if (!Array.isArray(p) || !Array.isArray(q) || p.length !== q.length) return false;
      cells += p.length;
      if (cells > COLUMN_CAP) return false;
      for (let r = 0; r < p.length; r++) if (p[r] !== q[r]) return false;
    }
  }
  return true;
}

// The range of a panel as a text, for the test of a redraw: a chart draws its x axis over the range, so a new range needs a new drawing
// even when the data are the same.
export const rangeKey = (r) => (r && !r.error && Number.isFinite(r.from_ms) && Number.isFinite(r.to_ms) ? `${r.from_ms}:${r.to_ms}` : "");
