// SPDX-License-Identifier: MIT OR Apache-2.0
// Fixture dashboards for the loading previews of the family "rows" (the table and the logs, 9.11): one panel for each rule of the council files, with
// the option variants that the corpus of the dashboards lacks. The panels read the seed table `analytics.seed` of the test server. The function takes
// the source name and the queries as arguments, so that the same variants run on the demo database.
import { doc, panel, section } from "./docs.mjs";

/** The queries of the seed table, and of the demo database (the argument `queries` of the builders). */
export const SEED = {
  source: "prod",
  table: ["SELECT node, weight, blocks, active", "FROM analytics.seed", "ORDER BY event_time"],
  logs: ["SELECT event_time AS timestamp, concat('line ', toString(blocks)) AS body, 'info' AS level", "FROM analytics.seed", "ORDER BY event_time"],
};
export const DEMO = {
  source: "demo",
  table: ["SELECT host, region, role, cpu_pct, mem_pct, disk_pct, uptime_days, state", "FROM demo.servers", "ORDER BY host"],
  logs: ["SELECT ts AS timestamp, message AS body, level", "FROM demo.logs", "ORDER BY ts DESC", "LIMIT 40"],
};

const withSource = (p, source) => {
  const swap = (q) => ({ ...q, source });
  const data = { ...p.data };
  if (data.query) data.query = swap(data.query);
  if (data.extra_queries) data.extra_queries = data.extra_queries.map(swap);
  return { ...p, data };
};

// The variants of the table. Each one has the members that decide one rule of table.md. `h` is the height of the panel in grid rows: a panel of 3 rows is
// lower than a header, a row and a footer, so its content overflows.
export const TABLE_VARIANTS = [
  { id: 1, title: "plain", h: 8, note: "H, extent, slot 0" },
  { id: 2, title: "no header", h: 8, options: { show_header: false }, note: "H = 0" },
  { id: 3, title: "wrap", h: 8, options: { wrap: true }, note: "no slot" },
  { id: 4, title: "width, footer, low", h: 3, fields: { defaults: { width: 800, footer: { reducers: ["total"] } }, overrides: [] }, note: "footer case 2" },
  { id: 5, title: "limit 1, footer", h: 8, transforms: [{ kind: "limit", count: 1, end: "first" }], fields: { defaults: { width: 150, footer: { reducers: ["total"] } }, overrides: [] }, note: "footer case 1" },
  { id: 6, title: "label, mark, filter", h: 8, options: { sort: [{ field: "Name", desc: false }] }, fields: { defaults: { display_name: "Name", align: "left", width: 160, filterable: true }, overrides: [] }, note: "header group, left" },
  { id: 7, title: "label centred", h: 8, fields: { defaults: { display_name: "Name", align: "center", width: 200 }, overrides: [] }, note: "header group, centre" },
  { id: 8, title: "gauge basic", h: 8, fields: { defaults: { width: 160, min: 0, max: 100, cell: { type: "gauge", mode: "basic" } }, overrides: [] }, note: "track" },
  { id: 9, title: "gauge lcd", h: 8, fields: { defaults: { width: 160, min: 0, max: 100, cell: { type: "gauge", mode: "lcd" } }, overrides: [] }, note: "lcd cells" },
  { id: 10, title: "two queries", h: 8, extra: true, note: "select bar" },
  { id: 11, title: "override width", h: 8, fields: { defaults: {}, overrides: [{ match: { by: "name", value: "weight" }, set: { width: 100 } }] }, note: "extent min(m)" },
  { id: 12, title: "hidden", h: 8, fields: { defaults: {}, overrides: [{ match: { by: "name", value: "node" }, set: { hidden: true } }] }, note: "no shown column" },
  { id: 13, title: "reduce fields, footer", h: 8, transforms: [{ kind: "reduce", mode: "reduce_fields", reducers: ["max"] }], fields: { defaults: { footer: { reducers: ["max"] } }, overrides: [] }, note: "B = 1, footer case 1" },
  { id: 14, title: "limit 0", h: 8, transforms: [{ kind: "limit", count: 0, end: "first" }], note: "no data" },
  { id: 15, title: "two reducers, low", h: 4, fields: { defaults: { width: 800, footer: { reducers: ["min", "max"] } }, overrides: [] }, note: "footer L = 2, names" },
  { id: 16, title: "filterable", h: 8, fields: { defaults: { filterable: true }, overrides: [] }, note: "coarse H" },
  { id: 17, title: "wrap, gauge", h: 8, options: { wrap: true }, fields: { defaults: { width: 160, min: 0, max: 100, cell: { type: "gauge", mode: "basic" } }, overrides: [] }, note: "track, top-aligned" },
  { id: 18, title: "filterable in an override", h: 8, fields: { defaults: {}, overrides: [{ match: { by: "name", value: "weight" }, set: { filterable: true } }] }, note: "coarse H is data" },
  { id: 19, title: "footers of two lengths", h: 4, fields: { defaults: { width: 150, footer: { reducers: ["min", "max"] } }, overrides: [{ match: { by: "name", value: "weight" }, set: { footer: { reducers: ["total"] } } }] }, note: "L is data" },
  { id: 20, title: "width 40", h: 8, fields: { defaults: { width: 40 }, overrides: [] }, note: "narrow columns" },
];

/** A dashboard with one table for each variant of TABLE_VARIANTS. */
export function tableVariantsDoc(uid, q = SEED) {
  const panels = TABLE_VARIANTS.map((v, i) => {
    const grid = { x: (i % 2) * 12, y: Math.floor(i / 2) * 10, w: 12, h: v.h };
    let p = panel(v.id, "table", `${v.id} ${v.title}`, q.table, { grid, options: v.options, fields: v.fields, transforms: v.transforms });
    p = withSource(p, q.source);
    if (v.extra) p.data = { ...p.data, extra_queries: [{ ref: "B", source: q.source, format: "table", sql: q.table }] };
    return p;
  });
  return doc(uid, "Previews of the tables", [section(null, panels)], { time: { from: "now-6h", to: "now", timezone: "utc", refresh: null } });
}

// The variants of the logs panel: the options that the council file runs for each rule.
export const LOGS_VARIANTS = [
  { id: 1, title: "no options", options: {} },
  { id: 2, title: "show_time", options: { show_time: true } },
  { id: 3, title: "show_time false", options: { show_time: false } },
  { id: 4, title: "wrap", options: { wrap: true } },
  { id: 5, title: "wrap and show_time", options: { wrap: true, show_time: true } },
  { id: 6, title: "oldest first, no details", options: { show_time: true, order: "oldest_first", details: false } },
];

/** A dashboard with one logs panel for each variant of LOGS_VARIANTS. */
export function logsVariantsDoc(uid, q = SEED) {
  const panels = LOGS_VARIANTS.map((v, i) => {
    const grid = { x: (i % 2) * 12, y: Math.floor(i / 2) * 10, w: 12, h: 8 };
    const base = panel(v.id, "table", `${v.id} ${v.title}`, q.logs, { grid, format: "logs" });
    return withSource({ ...base, type: "logs", fields: { defaults: {}, overrides: [] }, options: { ...v.options } }, q.source);
  });
  return doc(uid, "Previews of the logs", [section(null, panels)], { time: { from: "now-6h", to: "now", timezone: "utc", refresh: null } });
}
