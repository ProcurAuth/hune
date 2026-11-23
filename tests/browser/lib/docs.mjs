// SPDX-License-Identifier: MIT OR Apache-2.0
// Builders of synthetic dashboard documents (format 1) for the browser cases.
// All SQL reads the seed table `analytics.seed` (30 rows, 3 nodes) or runs a literal SELECT.

export const SOURCE = "prod";

const legend = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const tooltip = (mode = "single") => ({ mode });
const reduce = (over = {}) => ({ calc: "last_not_null", fields: null, values: false, ...over });

/** Default options of each panel type (the "New" column of 9.19). */
export function defaultOptions(type) {
  switch (type) {
    case "table":
      return { show_header: true, sort: [] };
    case "timeseries":
      return { draw: "line", interpolation: "linear", line_width: 1, fill_opacity: 0, show_points: "auto", point_size: 5, span_nulls: false, stacking: "none", y_scale: "linear", legend: legend(), tooltip: tooltip() };
    case "stat":
      return { reduce: reduce(), color_mode: "value", graph_mode: "none", text_mode: "auto", orientation: "auto" };
    case "barchart":
      return { orientation: "vertical", x_field: null, stacking: "none", show_values: "auto", bar_width: 97, group_width: 70, x_tick_rotation: 0, legend: legend(), tooltip: tooltip() };
    case "piechart":
      return { pie_type: "pie", reduce: reduce(), labels: [], legend: legend({ placement: "right" }), tooltip: tooltip() };
    case "xychart":
      return { x_field: null, y_fields: [], draw: "points", point_size: 5, line_width: 1, legend: legend(), tooltip: tooltip() };
    case "bargauge":
      return { reduce: reduce(), orientation: "horizontal", display_mode: "gradient", show_unfilled: true };
    default:
      throw new Error("unknown panel type " + type);
  }
}

/** A panel with a query. `o` overrides any member. */
export function panel(id, type, title, sql, o = {}) {
  const lines = Array.isArray(sql) ? sql : [sql];
  const format = o.format || (type === "timeseries" ? "time_series" : "table");
  return {
    id,
    type,
    title,
    description: o.description ?? "",
    transparent: o.transparent ?? false,
    grid: o.grid || { x: 0, y: 0, w: 12, h: 8 },
    data: o.reuse
      ? { reuse: o.reuse }
      : { query: { source: o.source || SOURCE, format, sql: lines, min_interval: o.min_interval ?? null, max_data_points: o.max_data_points ?? null } },
    transforms: o.transforms || [],
    fields: o.fields || { defaults: {}, overrides: [] },
    options: { ...defaultOptions(type), ...(o.options || {}) },
    links: o.links || [],
  };
}

export function section(title, panels, collapsed = false) {
  return { title, collapsed, panels };
}

export function doc(uid, title, sections, o = {}) {
  return {
    format_version: 3,
    uid,
    title,
    description: o.description ?? "",
    tags: o.tags || [],
    time: o.time || { from: "now-6h", to: "now", timezone: "browser", refresh: null },
    cursor_sync: o.cursor_sync || "off",
    links: o.links || [],
    variables: o.variables || [],
    sections,
  };
}

export const SQL = {
  table: ["SELECT node, weight, blocks, active", "FROM analytics.seed", "ORDER BY event_time"],
  series: [
    "SELECT $__timeInterval(event_time) AS t, node, sum(weight) AS weight",
    "FROM analytics.seed",
    "WHERE $__timeFilter(event_time)",
    "GROUP BY t, node",
    "ORDER BY t",
  ],
  bars: ["SELECT node, sum(blocks) AS blocks", "FROM analytics.seed", "GROUP BY node", "ORDER BY node"],
  xy: ["SELECT blocks AS x, toFloat64(weight) AS y", "FROM analytics.seed", "ORDER BY blocks"],
  total: ["SELECT sum(weight) AS Total, count() AS Rows", "FROM analytics.seed"],
};

/** A dashboard with the seven panel types, each on its fixture query. */
export function allTypesDoc(uid = "all-types") {
  const g = (x, y, w, h) => ({ x, y, w, h });
  return doc(uid, "All panel types", [
    section(null, [
      panel(1, "table", "Table", SQL.table, { grid: g(0, 0, 12, 8) }),
      panel(2, "timeseries", "Lines", SQL.series, { grid: g(12, 0, 12, 8), options: { draw: "line" } }),
      panel(3, "timeseries", "Bars", SQL.series, { grid: g(0, 8, 12, 8), options: { draw: "bars", fill_opacity: 80 } }),
      panel(4, "stat", "Stat", SQL.total, {
        grid: g(12, 8, 6, 4),
        options: { reduce: reduce({ calc: "last", fields: "/^Total$/" }), color_mode: "none", graph_mode: "none" },
      }),
      panel(5, "barchart", "Bar chart", SQL.bars, { grid: g(18, 8, 6, 8) }),
      panel(6, "piechart", "Pie chart", SQL.bars, { grid: g(0, 16, 8, 8), options: { reduce: reduce({ values: true }), labels: ["name", "percent"] } }),
      panel(7, "xychart", "XY chart", SQL.xy, { grid: g(8, 16, 8, 8) }),
      panel(8, "bargauge", "Bar gauge", SQL.bars, {
        grid: g(16, 16, 8, 8),
        options: { reduce: reduce({ values: true }) },
        fields: { defaults: { min: 0, max: 400 }, overrides: [] },
      }),
    ]),
  ]);
}
