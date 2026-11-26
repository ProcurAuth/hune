// SPDX-License-Identifier: MIT OR Apache-2.0
// Fixture dashboards for the loading preview of the charts (9.11): the time series, the xy chart and the bar chart, each with several option
// variants (legend at the bottom or at the right, as a list or a table, hidden or off; axes hidden, placed or sized; the bar chart vertical,
// horizontal and automatic). Each panel is on the seed table `analytics.seed`. The ids and the variants are in the lists TS_VARIANTS and
// BAR_VARIANTS, so a case can name the panel that it asserts.
import { SQL, doc, panel, section } from "./docs.mjs";

const lg = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const hide = (over = {}) => ({ legend: true, tooltip: false, viz: false, ...over });
const d = (defaults = {}, overrides = []) => ({ defaults, overrides });
const byName = (value, set) => ({ match: { by: "name", value }, set });
const byQuery = (value, set) => ({ match: { by: "query", value }, set });

// The time series variants: [key, title, options, fields, size { w, h } in grid units].
export const TS_VARIANTS = [
  ["list-bottom", "legend list at the bottom", { legend: lg() }, d()],
  ["table-bottom", "legend table at the bottom", { legend: lg({ mode: "table", values: ["last_not_null", "max"], sort_by: "max", sort_desc: true }) }, d({ unit: "percent" })],
  ["table-empty", "legend table with no value", { legend: lg({ mode: "table", values: [] }) }, d()],
  ["list-right", "legend list at the right", { legend: lg({ placement: "right" }) }, d()],
  ["table-right", "legend table at the right", { legend: lg({ placement: "right", mode: "table", values: ["mean", "last"] }) }, d()],
  ["hidden-bottom", "every entry hidden, bottom", { legend: lg() }, d({ hide_from: hide() })],
  ["hidden-right", "every entry hidden, right", { legend: lg({ placement: "right" }) }, d({ hide_from: hide() })],
  ["off", "legend off", { legend: lg({ show: false }) }, d()],
  ["axes-hidden", "all axes hidden", { legend: lg({ show: false }) }, d({ axis_placement: "hidden" })],
  ["axis-right", "axis at the right, with a title", { legend: lg({ show: false }) }, d({ axis_placement: "right", axis_label: "Weight" })],
  ["axis-width", "axis width 50 and a title", { legend: lg({ show: false }) }, d({ axis_width: 50, axis_label: "Weight" })],
  ["fixed-range", "fixed range from 0 to 100", { legend: lg() }, d({ min: 0, max: 100, unit: "percent" })],
  ["soft-zero", "soft ends 0 and 500, unit short", { legend: lg() }, d({ soft_min: 0, soft_max: 500, unit: "short", decimals: 0 })],
  ["undecided", "overrides by name set the axis and the unit", { legend: lg() }, d({}, [byName("v0", { axis_placement: "right", unit: "bytes" }), byName("v1", { axis_width: 30 })])],
  ["undecided-hide", "an override hides some entries", { legend: lg() }, d({}, [byName("v0", { hide_from: hide() })])],
  ["query-override", "an override by query sets the axis width", { legend: lg({ show: false }) }, d({}, [byQuery("A", { axis_width: 44 })])],
  ["short-list", "a short panel, list at the bottom", { legend: lg() }, d(), { w: 12, h: 3 }],
  ["short-table", "a short panel, table at the bottom", { legend: lg({ mode: "table", values: ["last", "max"] }) }, d(), { w: 12, h: 3 }],
  ["narrow-right-table", "a narrow panel, table at the right", { legend: lg({ placement: "right", mode: "table", values: ["mean", "max", "last"] }) }, d(), { w: 6, h: 8 }],
  ["narrow-right-list", "a narrow panel, list at the right", { legend: lg({ placement: "right" }) }, d(), { w: 6, h: 8 }],
  ["bars", "draw bars, stacked", { legend: lg(), draw: "bars", stacking: "normal", fill_opacity: 80 }, d()],
];

// The bar chart variants: [key, title, options, fields, size].
export const BAR_VARIANTS = [
  ["list-bottom", "vertical, list at the bottom", { orientation: "vertical", legend: lg() }, d()],
  ["table-bottom", "vertical, table at the bottom", { orientation: "vertical", legend: lg({ mode: "table", values: ["max"] }) }, d()],
  ["table-empty", "vertical, table with no value", { orientation: "vertical", legend: lg({ mode: "table", values: [] }) }, d()],
  ["list-right", "vertical, list at the right", { orientation: "vertical", legend: lg({ placement: "right" }) }, d()],
  ["table-right", "vertical, table at the right", { orientation: "vertical", legend: lg({ placement: "right", mode: "table", values: ["max", "last"] }) }, d()],
  ["off", "vertical, legend off", { orientation: "vertical", legend: lg({ show: false }) }, d()],
  ["hidden", "vertical, every entry hidden", { orientation: "vertical", legend: lg() }, d({ hide_from: hide() })],
  ["axis", "vertical, axis width 40 and a title", { orientation: "vertical", legend: lg({ show: false }) }, d({ axis_width: 40, axis_label: "Weight" })],
  ["title", "vertical, soft ends and a title", { orientation: "vertical", legend: lg() }, d({ axis_label: "Weight", soft_min: 0, soft_max: 500, unit: "short" })],
  ["turned", "vertical, labels turned by -90", { orientation: "vertical", x_tick_rotation: -90, legend: lg() }, d()],
  ["h-bottom", "horizontal, legend at the bottom", { orientation: "horizontal", legend: lg() }, d()],
  ["h-right", "horizontal, legend at the right", { orientation: "horizontal", legend: lg({ placement: "right" }) }, d()],
  ["h-off", "horizontal, legend off, with a title", { orientation: "horizontal", legend: lg({ show: false }) }, d({ axis_label: "Weight" })],
  ["h-band", "horizontal, axis width 30", { orientation: "horizontal", legend: lg() }, d({ axis_width: 30 })],
  ["h-percent", "horizontal, percent stack", { orientation: "horizontal", stacking: "percent", legend: lg({ show: false }) }, d()],
  ["auto-tall", "auto, higher than wide", { orientation: "auto", legend: lg() }, d(), { w: 6, h: 14 }],
  ["auto-wide", "auto, wider than high", { orientation: "auto", legend: lg() }, d(), { w: 12, h: 6 }],
  ["short-list", "vertical, a short panel", { orientation: "vertical", legend: lg() }, d(), { w: 12, h: 3 }],
];

const XY_VARIANTS = [
  ["xy-list", "xy chart, list at the bottom", { legend: lg() }, d()],
  ["xy-table", "xy chart, table at the bottom", { legend: lg({ mode: "table", values: ["last"] }) }, d()],
  ["xy-right", "xy chart, list at the right", { legend: lg({ placement: "right" }) }, d()],
  ["xy-off", "xy chart, legend off", { legend: lg({ show: false }) }, d()],
];

// Puts panels in rows of 24 columns, `w` and `h` columns and rows each.
function flow(items) {
  let x = 0;
  let y = 0;
  let rowH = 0;
  return items.map((it) => {
    const w = it.w || 12;
    const h = it.h || 8;
    if (x + w > 24) {
      x = 0;
      y += rowH;
      rowH = 0;
    }
    const grid = { x, y, w, h };
    x += w;
    rowH = Math.max(rowH, h);
    return grid;
  });
}

const build = (type, variants, query, start, extra = {}) => {
  const grids = flow(variants.map((v) => v[4] || {}));
  return variants.map((v, i) => panel(start + i, type, v[1], query, { grid: grids[i], options: { ...extra, ...v[2] }, fields: v[3] }));
};

/** A dashboard of time series, one for each variant of TS_VARIANTS: the panel `i` of the list has the id `i + 1`. */
export function timeSeriesVariantsDoc(uid = "pv-ts") {
  return doc(uid, "Preview of the time series", [section(null, build("timeseries", TS_VARIANTS, SQL.series, 1))], { time: { from: "now-6h", to: "now", timezone: "utc", refresh: null } });
}

/** A dashboard of bar charts and xy charts: the bar chart `i` has the id `i + 1`, and the xy chart `j` the id `BAR_VARIANTS.length + j + 1`. */
export function barVariantsDoc(uid = "pv-bar") {
  const bars = build("barchart", BAR_VARIANTS, SQL.bars, 1);
  const xy = build("xychart", XY_VARIANTS, SQL.xy, BAR_VARIANTS.length + 1, { draw: "points" });
  // The xy panels come after the bar panels, so their grid starts below the last row of the bars.
  const rows = Math.max(...bars.map((p) => p.grid.y + p.grid.h));
  const placed = xy.map((p) => ({ ...p, grid: { ...p.grid, y: p.grid.y + rows } }));
  return doc(uid, "Preview of the bar charts", [section(null, [...bars, ...placed])], { time: { from: "now-6h", to: "now", timezone: "utc", refresh: null } });
}
