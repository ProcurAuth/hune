// SPDX-License-Identifier: MIT OR Apache-2.0
// A fixture dashboard for the tests of the loading previews (9.11): one panel for each of the 14 queried panel types, all on the seed table
// `analytics.seed`. The options and the fields of the types that the helper `panel` does not know come from the demo dashboard of latency and from
// the fixtures of the cases of the timelines and of the map. The validator of the server accepts them.
import { readFileSync } from "node:fs";
import { SQL, doc, panel, section } from "./docs.mjs";

const DEMO = JSON.parse(readFileSync(new URL("../../../examples/demo/dashboards/operations/latency.json", import.meta.url), "utf8"));
const demo = (type) => DEMO.sections.flatMap((s) => s.panels).find((p) => p.type === type);
const legend = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const reduce = (over = {}) => ({ calc: "last_not_null", fields: null, values: false, ...over });

// The queries of the fixture. A panel of a type that the helper does not know keeps the format `table`.
const TIMES = "toDateTime('2026-11-13 00:00:00','UTC') + number*600 AS time";
const TIMELINE = [`SELECT ${TIMES}, multiIf(number<5,0, number<9,1, 2) AS a`, "FROM numbers(30)"];
const LOGS = ["SELECT now() - number AS timestamp, concat('line ', toString(number)) AS body, 'info' AS level", "FROM numbers(7)"];
const GEO = ["SELECT 48.85 AS lat, 2.35 AS lon, 'Paris' AS name, 10 AS value"];

// A panel of a type that `defaultOptions` of docs.mjs does not know: the options and the fields of the demo panel, the query of the fixture.
function demoBased(id, type, title, sql, grid, over = {}) {
  const src = demo(type);
  const base = panel(id, "table", title, sql, { grid, format: over.format || "table" });
  return { ...base, type, fields: over.fields || src.fields, options: { ...src.options, ...(over.options || {}) } };
}

/** The ids of the panels of `allTypesPreviewDoc`, in the order of the types. */
export const FIXTURE_TYPES = ["timeseries", "xychart", "barchart", "piechart", "stat", "gauge", "bargauge", "table", "heatmap", "histogram", "state_timeline", "status_history", "logs", "geomap"];

/** The dashboard with one panel of each queried type. The panel `i` of the list FIXTURE_TYPES has the id `i + 1`. */
export function allTypesPreviewDoc(uid = "preview-types", time = { from: "now-6h", to: "now", timezone: "utc", refresh: null }) {
  const g = (i, h = 8) => ({ x: (i % 2) * 12, y: Math.floor(i / 2) * 8, w: 12, h });
  const f = { defaults: { unit: "none", decimals: 0 }, overrides: [] };
  const map4 = { defaults: { mappings: [{ kind: "value", value: "0", text: "OK", color: "green" }, { kind: "value", value: "1", text: "WARN", color: "yellow" }, { kind: "value", value: "2", text: "FAIL", color: "red" }] }, overrides: [] };
  const panels = [
    panel(1, "timeseries", "timeseries", SQL.series, { grid: g(0), options: { legend: legend() }, fields: f }),
    panel(2, "xychart", "xychart", SQL.xy, { grid: g(1) }),
    panel(3, "barchart", "barchart", SQL.bars, { grid: g(2), options: { legend: legend({ show: false }) }, fields: f }),
    panel(4, "piechart", "piechart", SQL.bars, { grid: g(3), options: { reduce: reduce({ values: true }), labels: [] }, fields: f }),
    panel(5, "stat", "stat", SQL.total, { grid: g(4, 4), options: { graph_mode: "area" } }),
    demoBased(6, "gauge", "gauge", SQL.bars, g(5, 4), { options: { reduce: reduce({ values: true }) }, fields: { defaults: { min: 0, max: 400 }, overrides: [] } }),
    panel(7, "bargauge", "bargauge", SQL.bars, { grid: g(6, 4), options: { reduce: reduce({ values: true }) }, fields: { defaults: { min: 0, max: 400 }, overrides: [] } }),
    panel(8, "table", "table", SQL.table, { grid: g(7, 6) }),
    demoBased(9, "heatmap", "heatmap", SQL.series, g(8)),
    demoBased(10, "histogram", "histogram", SQL.bars, g(9)),
    demoBased(11, "state_timeline", "state_timeline", TIMELINE, g(10), { fields: map4 }),
    demoBased(12, "state_timeline", "status_history", TIMELINE, g(11), { fields: map4 }),
    demoBased(13, "logs", "logs", LOGS, g(12), { format: "logs", fields: { defaults: {}, overrides: [] } }),
    {
      ...panel(14, "table", "geomap", GEO, { grid: g(13, 10) }),
      type: "geomap",
      fields: { defaults: {}, overrides: [] },
      options: { view: { mode: "fixed", lat: 0, lon: 0, zoom: 1 }, layer: { lat_field: null, lon_field: null, size: 5, size_min: 2, size_max: 15, size_field: null, color: "dark-green", color_field: null, opacity: 40 }, tooltip: "details", show_zoom: true, mouse_wheel_zoom: true },
    },
  ];
  // The panel 12 is a status history: the same options as a state timeline, less its own members.
  panels[11] = { ...panels[11], type: "status_history", options: { show_values: "auto", row_height: 90, col_width: 90, line_width: 1, fill_opacity: 70, show_labels: true, legend: legend(), tooltip: { mode: "single", sort: "none" } } };
  return doc(uid, "Preview of the panel types", [section(null, panels)], { time });
}
