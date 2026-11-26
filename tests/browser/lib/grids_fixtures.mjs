// SPDX-License-Identifier: MIT OR Apache-2.0
// Fixture dashboards for the loading previews of the grid family (9.11): the heatmap, the histogram, the state timeline and the status history.
// Each dashboard has one panel for each option variant that a rule of the council files tests. All queries are literal SELECTs on `numbers(...)`,
// so the panels load on any ClickHouse. The range is absolute: the loaded panels have data in the same range for each run.
import { doc, panel, section } from "./docs.mjs";

export const T0 = Date.UTC(2026, 10, 13);
const HOUR = 3600000;
export const RANGE = { from: String(T0), to: String(T0 + 5 * HOUR), timezone: "utc", refresh: null };

const TIME = (step) => `toDateTime('2026-11-13 00:00:00','UTC') + number*${step} AS time`;
const EXP = (seed, scale) => `round(${scale}*-log(1-(cityHash64(number+${seed}) % 1000000 + 0.5)/1000000),3)`;
const SERIES = [`SELECT ${TIME(60)}, ${EXP(0, 40)} AS latency`, "FROM numbers(300)"];
const WIDE = [`SELECT ${TIME(60)}, number % 7 AS \`10\`, number % 5 AS \`100\``, "FROM numbers(40)"];
const VALUES = [`SELECT ${TIME(60)}, ${EXP(0, 40)} AS a`, "FROM numbers(300)"];
const TWO = [`SELECT ${TIME(60)}, ${EXP(0, 40)} AS a, ${EXP(7, 25)} AS b`, "FROM numbers(300)"];
const STATES = [`SELECT ${TIME(600)}, multiIf(number<5,0, number<9,1, 2) AS state`, "FROM numbers(30)"];

// The panels flow in rows of 24 columns: a panel has its size in `grid` (w and h), and `flow` gives it the place.
export function flow(panels) {
  let x = 0;
  let y = 0;
  let row = 0;
  return panels.map((p) => {
    const { w, h } = p.grid;
    if (x + w > 24) {
      x = 0;
      y += row;
      row = 0;
    }
    const out = { ...p, grid: { x, y, w, h } };
    x += w;
    row = Math.max(row, h);
    return out;
  });
}
const SIZE = { w: 12, h: 8 };

// A panel of a type that the helper of docs.mjs does not know: the options are given in full.
const typed = (id, type, title, sql, g, options, fields = { defaults: {}, overrides: [] }, format = "table") => ({ ...panel(id, "table", title, sql, { grid: g, format }), type, options, fields });

// ---- heatmap ---------------------------------------------------------------------------------------------------------------------
const HEAT = {
  calculate: true, x_buckets: { mode: "count", value: 60 }, y_buckets: { mode: "size", value: 2, scale: "log2" }, y_axis: "left", y_reverse: false, y_min: null, y_max: null,
  color: { mode: "scheme", scheme: "spectral", fill: "dark-orange", scale: "exponential", exponent: 0.5, steps: 64, reverse: false, min: null, max: null },
  cell_gap: 1, hide_le: 0, hide_ge: null, show_legend: true, tooltip: { mode: "single" },
};
const heat = (id, title, over = {}, fields, sql = SERIES, g = SIZE) =>
  typed(id, "heatmap", title, sql, g, { ...HEAT, ...over, color: { ...HEAT.color, ...(over.color || {}) } }, fields);
const linear = { mode: "size", value: 10, scale: "linear" };

/** The heatmaps: each panel is a case of the council rules. `calculate` with a log axis is the bounded left edge of the demo. */
export function heatmapDoc(uid = "pv-heat") {
  const ms = { defaults: { unit: "ms", decimals: 0 }, overrides: [] };
  return doc(uid, "Heatmap previews", [section(null, flow([
    heat(1, "bounded left, no ends", {}, ms),
    heat(2, "fixed axis, both ends", { y_buckets: linear, y_min: 0, y_max: 100, color: { min: 0, max: 100 } }, ms),
    heat(3, "fixed axis, reversed, percent, left end", { y_buckets: linear, y_min: 0, y_max: 50, y_reverse: true, color: { min: 1 } }, { defaults: { unit: "percent", decimals: 1 }, overrides: [] }),
    heat(4, "hidden axis, no legend", { y_axis: "hidden", show_legend: false }),
    heat(5, "hidden axis, both ends, narrow", { y_axis: "hidden", color: { min: 0, max: 1000 } }, undefined, SERIES, { w: 6, h: 8 }),
    heat(6, "E2: no whole count passes", { hide_le: 5, hide_ge: 3 }),
    heat(7, "E3: bucketed, hide_le above hide_ge", { calculate: false, hide_le: 5, hide_ge: 5 }, undefined, WIDE),
    heat(8, "E4: no field is a value field", {}, { defaults: { hide_from: { legend: false, tooltip: false, viz: true } }, overrides: [] }),
    heat(9, "short body", { y_buckets: linear, y_min: 0, y_max: 100 }, ms, SERIES, { w: 12, h: 3 }),
    heat(10, "bucketed, left axis", { calculate: false }, undefined, WIDE),
    heat(11, "fixed axis, no unit", { y_buckets: linear, y_min: 0, y_max: 100 }, undefined, SERIES, SIZE),
    heat(12, "fixed axis, unit set by a query override", { y_buckets: linear, y_min: 0, y_max: 1000 }, { defaults: { unit: "ms" }, overrides: [{ match: { by: "query", value: "A" }, set: { unit: "s", decimals: 1 } }] }, SERIES, SIZE),
  ]))], { time: RANGE });
}

// ---- histogram -------------------------------------------------------------------------------------------------------------------
const HIST = { bucket_count: 30, bucket_size: null, bucket_offset: 0, combine: false, stacking: "none", line_width: 1, fill_opacity: 80, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single" } };
const legendOf = (over) => ({ ...HIST.legend, ...over });
const hist = (id, title, over = {}, fields, sql = VALUES, g) => typed(id, "histogram", title, sql, g || SIZE, { ...HIST, ...over }, fields);
const noLegend = { defaults: { hide_from: { legend: true, tooltip: false, viz: false } }, overrides: [] };

/** The histograms: the legend cases (off, empty, list, table, bottom, right, fallback), the names, the stacking and the case with no chart. */
export function histogramDoc(uid = "pv-hist") {
  const count = { defaults: { display_name: "n" }, overrides: [] };
  return doc(uid, "Histogram previews", [section(null, flow([
    hist(1, "bottom list", {}),
    hist(2, "legend off", { legend: legendOf({ show: false }) }, { defaults: { unit: "ms", decimals: 0, color: { mode: "fixed", fixed: "orange" } }, overrides: [] }),
    hist(3, "right list", { legend: legendOf({ placement: "right" }) }),
    hist(4, "bottom table", { legend: legendOf({ mode: "table", values: ["min", "max"] }) }),
    hist(5, "right table", { legend: legendOf({ placement: "right", mode: "table", values: ["min", "max", "mean"], sort_by: "max", sort_desc: true }) }),
    hist(6, "right list, narrow (the fallback)", { legend: legendOf({ placement: "right" }) }, undefined, VALUES, { w: 4, h: 8 }),
    hist(7, "right, every entry hidden", { legend: legendOf({ placement: "right" }) }, noLegend),
    hist(8, "COUNT, bottom list", { combine: true }, count),
    hist(9, "COUNT, percent, right list", { combine: true, stacking: "percent", legend: legendOf({ placement: "right" }) }, count),
    hist(10, "no chart", {}, { defaults: { hide_from: { legend: false, tooltip: false, viz: true } }, overrides: [] }),
    hist(11, "two series, percent", { stacking: "percent" }, undefined, TWO),
    hist(12, "short body, bottom list", {}, undefined, VALUES, { w: 12, h: 3 }),
    hist(13, "COUNT, bottom table", { combine: true, legend: legendOf({ mode: "table", values: ["max"] }) }, count),
  ]))], { time: RANGE });
}

// ---- state timeline and status history -----------------------------------------------------------------------------------------
const STATE = { merge_values: true, show_values: "never", align_values: "left", row_height: 80, line_width: 0, fill_opacity: 75, span_nulls: false, show_labels: true, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single" } };
const HISTORY = { show_values: "never", row_height: 90, col_width: 90, line_width: 1, fill_opacity: 70, show_labels: true, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single" } };
const steps = (...values) => ({ mode: "absolute", steps: values.map((v, k) => ({ value: v, color: ["green", "orange", "red"][k] })) });
const MAP = [{ kind: "value", value: "0", text: "OK", color: "green" }, { kind: "value", value: "1", text: "WARN", color: "yellow" }];
const tl = (id, type, title, over = {}, fields = { defaults: { mappings: MAP }, overrides: [] }, g) => {
  const base = type === "state_timeline" ? STATE : HISTORY;
  return typed(id, type, title, STATES, g || SIZE, { ...base, ...over, legend: { ...base.legend, ...(over.legend || {}) } }, fields);
};
const off = { legend: { show: false } };

/** The state timelines and the status histories: the legend cases A to E, the left gutter, the fallback and the panel that never draws. */
export function timelineDoc(uid = "pv-timeline") {
  const pct = { defaults: { unit: "percent", decimals: 1, color: { mode: "thresholds", fixed: null }, thresholds: steps(null, 50, 90) }, overrides: [{ match: { by: "name", value: "state" }, set: { display_name: "Status" } }] };
  const named = { defaults: { display_name: "Status", color: { mode: "thresholds", fixed: null }, thresholds: steps(null, 80) }, overrides: [] };
  return doc(uid, "Timeline previews", [section(null, flow([
    tl(1, "state_timeline", "B: mappings, no colour", {}),
    tl(2, "state_timeline", "A: legend off, no names", { show_labels: false, ...off }, { defaults: { color: { mode: "fixed", fixed: "blue" } }, overrides: [] }),
    tl(3, "state_timeline", "C: right legend, thresholds, override name", { legend: { placement: "right" } }, pct),
    tl(4, "state_timeline", "D: palette, bottom", {}, { defaults: { color: { mode: "palette", fixed: null } }, overrides: [] }),
    tl(5, "state_timeline", "C: bottom, fixed name", {}, named),
    tl(6, "status_history", "C: default thresholds", {}),
    tl(7, "status_history", "A: legend off, no names", { show_labels: false, ...off }, { defaults: { color: { mode: "fixed", fixed: "blue" } }, overrides: [] }),
    tl(8, "status_history", "C: right legend, percent", { legend: { placement: "right" } }, { defaults: { unit: "percent", decimals: 1, thresholds: steps(null, 80) }, overrides: [] }),
    tl(9, "status_history", "never drawn", {}, { defaults: { hide_from: { legend: false, tooltip: false, viz: true } }, overrides: [] }),
    tl(10, "state_timeline", "C: right legend, narrow (the fallback)", { legend: { placement: "right" } }, pct, { w: 5, h: 8 }),
    tl(11, "state_timeline", "C: short body", {}, named, { w: 12, h: 3 }),
  ]))], { time: RANGE });
}
