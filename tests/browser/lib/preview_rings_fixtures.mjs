// SPDX-License-Identifier: MIT OR Apache-2.0
// Fixture dashboards for the loading previews of the pie chart and of the map (9.11): one panel for each option variant that changes a place or a
// size of the preview. They use the seed table `analytics.seed` and literal SELECT queries only. The ids of the panels follow the order of the lists
// PIE_VARIANTS and MAP_VARIANTS (the panel `i` has the id `i + 1`).
import { SQL, doc, panel, section } from "./docs.mjs";

const legend = (over = {}) => ({ show: true, placement: "right", mode: "list", values: [], ...over });
const reduce = (over = {}) => ({ calc: "last_not_null", fields: null, values: false, ...over });
// Forty items with long names: a legend that wraps at the bottom and passes the width at the right.
const MANY = ["SELECT concat('service number ', toString(number)) AS name, toFloat64(number + 1) AS amount", "FROM numbers(40)"];
const ONE = ["SELECT 'only' AS name, 1.0 AS amount"];

/** The variants of the pie chart. `grid.w` and `grid.h` are in the units of the dashboard grid. */
export const PIE_VARIANTS = [
  { name: "right list", options: { legend: legend() }, grid: { w: 8, h: 8 } },
  { name: "right table value percent", options: { legend: legend({ mode: "table", values: ["value", "percent"] }) }, grid: { w: 8, h: 8 } },
  { name: "right list percent", options: { legend: legend({ values: ["percent"] }) }, grid: { w: 8, h: 8 } },
  { name: "bottom list", options: { legend: legend({ placement: "bottom" }) }, grid: { w: 8, h: 8 } },
  { name: "bottom table", options: { legend: legend({ placement: "bottom", mode: "table", values: ["value", "percent"] }) }, grid: { w: 8, h: 8 } },
  { name: "legend hidden", options: { legend: legend({ show: false }) }, grid: { w: 8, h: 8 } },
  { name: "donut right table", options: { pie_type: "donut", legend: legend({ mode: "table", values: ["value", "percent"] }) }, grid: { w: 8, h: 8 } },
  { name: "donut bottom low", options: { pie_type: "donut", legend: legend({ placement: "bottom" }) }, grid: { w: 8, h: 2 } },
  { name: "right narrow", options: { legend: legend({ mode: "table", values: ["value"] }) }, grid: { w: 2, h: 8 } },
  { name: "literal name right", options: { reduce: reduce({ calc: "last" }), legend: legend() }, grid: { w: 8, h: 8 }, sql: SQL.total, fields: { defaults: { display_name: "Total weight in the period" }, overrides: [] } },
  { name: "many items right", options: { reduce: reduce({ values: true }), legend: legend({ mode: "table", values: ["value", "percent"] }) }, grid: { w: 8, h: 8 }, sql: MANY },
  { name: "many items bottom", options: { reduce: reduce({ values: true }), legend: legend({ placement: "bottom" }) }, grid: { w: 8, h: 8 }, sql: MANY },
  { name: "one item bottom", options: { reduce: reduce({ values: true }), legend: legend({ placement: "bottom" }) }, grid: { w: 8, h: 8 }, sql: ONE },
  { name: "no reduce rows right", options: { reduce: reduce({ values: true }), legend: legend() }, grid: { w: 12, h: 12 } },
];

/** A dashboard with one pie chart for each variant. */
export function pieDoc(uid = "pv-pies", variants = PIE_VARIANTS) {
  let x = 0;
  let y = 0;
  let rowH = 0;
  const panels = variants.map((v, i) => {
    if (x + v.grid.w > 24) {
      x = 0;
      y += rowH;
      rowH = 0;
    }
    const grid = { x, y, w: v.grid.w, h: v.grid.h };
    x += v.grid.w;
    rowH = Math.max(rowH, v.grid.h);
    const options = { pie_type: "pie", reduce: reduce({ values: true }), labels: ["percent"], tooltip: { mode: "single" }, ...v.options };
    return panel(i + 1, "piechart", v.name, v.sql || SQL.bars, { grid, options, fields: v.fields || { defaults: {}, overrides: [] } });
  });
  return doc(uid, "Preview of the pie charts", [section(null, panels)], { time: { from: "now-6h", to: "now", timezone: "utc", refresh: null } });
}

const MAP_ROWS = ["SELECT 48.85 AS lat, 2.35 AS lon, 'Paris' AS name, 10 AS value UNION ALL SELECT 40.7, -74.0, 'New York', 30"];
const mapOptions = (over = {}) => ({
  view: { mode: "fixed", lat: 0, lon: 0, zoom: 1 },
  layer: { lat_field: null, lon_field: null, size: 5, size_min: 2, size_max: 15, size_field: null, color: "dark-green", color_field: null, opacity: 40 },
  tooltip: "details",
  show_zoom: true,
  mouse_wheel_zoom: true,
  ...over,
});

/** The variants of the map. */
export const MAP_VARIANTS = [
  { name: "zoom fixed", options: mapOptions(), grid: { w: 12, h: 10 } },
  { name: "no zoom", options: mapOptions({ show_zoom: false }), grid: { w: 12, h: 10 } },
  { name: "zoom fit", options: mapOptions({ view: { mode: "fit", lat: 0, lon: 0, zoom: 6 } }), grid: { w: 12, h: 10 } },
  { name: "low map", options: mapOptions(), grid: { w: 12, h: 2 } },
  { name: "narrow map", options: mapOptions({ mouse_wheel_zoom: false }), grid: { w: 3, h: 10 } },
  { name: "transparent", options: mapOptions(), grid: { w: 12, h: 10 }, transparent: true },
];

/** A dashboard with one map for each variant. */
export function mapDoc(uid = "pv-maps", variants = MAP_VARIANTS) {
  const panels = variants.map((v, i) => {
    const base = panel(i + 1, "table", v.name, MAP_ROWS, { grid: { x: (i % 2) * 12, y: Math.floor(i / 2) * 10, w: v.grid.w, h: v.grid.h }, transparent: v.transparent });
    return { ...base, type: "geomap", fields: { defaults: {}, overrides: [] }, options: v.options };
  });
  return doc(uid, "Preview of the maps", [section(null, panels)], { time: { from: "now-6h", to: "now", timezone: "utc", refresh: null } });
}
