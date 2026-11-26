// SPDX-License-Identifier: MIT OR Apache-2.0
// Fixture dashboards for the loading previews of the values families (9.11): `stat`, `gauge` and `bargauge`. They read no table: each query is a
// literal SELECT, so a dashboard runs on any ClickHouse source (the argument `source`). Each dashboard has panels that vary one option group, so that the harness (lib/preview_measure.mjs) compares the preview with
// the loaded panel for each variant.
//   gaugeDoc     a gauge with one new number field (the case B of the rules): the geometry is exact or bounded. Variants of the orientation, the sizing,
//                the title, the scale, the markers, the labels, the neutral mark, the value size and the colour mode.
//   gaugeNDoc    gauges whose count is data (no proof): the box only.
//   statDoc      stat panels with the options that change the layout: the box only.
//   statOneDoc   stat panels whose chain ends with one new number field (proof A): the box and the one tile.
//   reuseDoc     panels that read the data of a source panel (`data.reuse`): the count follows the chain of the source.
//   barGaugeDoc  bar gauges with the options that change the layout: the box only.
import { SOURCE, doc, panel, section } from "./docs.mjs";

const TOTAL = ["SELECT 300 AS Total, 30 AS Rows"];
const BARS = ["SELECT 'n1' AS node, 100 AS blocks", "UNION ALL SELECT 'n2', 250", "UNION ALL SELECT 'n3', 180"];
const reduce = (over = {}) => ({ calc: "last", fields: null, values: false, ...over });
const calc = (mode, object, o = {}) => ({ kind: "calculate_field", mode, alias: o.alias ?? null, replace_fields: o.replace ?? false, [{ binary: "binary", reduce_row: "reduce", unary: "unary", index: "index" }[mode]]: object });
// The chain of the case B: one new number field replaces the fields.
const ONE = [calc("reduce_row", { reducer: "total", include: ["Total", "Rows"] }, { alias: "Sum", replace: true })];

const g = (x, y, w, h) => ({ x, y, w, h });
const steps = (list) => ({ mode: "absolute", steps: list.map(([value, color]) => ({ value, color })) });

// A panel of the type `gauge` (the helper `panel` does not know it): the options of `over.options` follow the defaults of the format.
function gauge(id, title, grid, over = {}, source = SOURCE) {
  const base = panel(id, "table", title, over.sql || TOTAL, { grid, transforms: over.transforms ?? ONE, reuse: over.reuse, source });
  return {
    ...base,
    type: "gauge",
    fields: over.fields || { defaults: { min: 0, max: 400 }, overrides: [] },
    options: { reduce: reduce(over.reduce || {}), ...(over.options || {}) },
  };
}

/** A gauge with the case B of the rules (one new field): one panel for each variant. The id of a panel is its place in `GAUGE_VARIANTS`. */
export const GAUGE_VARIANTS = [
  ["auto, scale 0 to 400, default thresholds", g(0, 0, 12, 8), { options: { orientation: "auto" } }],
  ["auto, a title (display_name)", g(12, 0, 12, 8), { fields: { defaults: { min: 0, max: 400, display_name: "Total weight" }, overrides: [] } }],
  ["auto, a title and the title size 22", g(0, 8, 8, 6), { fields: { defaults: { min: 0, max: 400, display_name: "Weight" }, overrides: [] }, options: { title_size: 22 } }],
  ["auto, value size 30, labels on", g(8, 8, 8, 6), { options: { value_size: 30, show_threshold_labels: true } }],
  ["auto, labels on, markers off", g(16, 8, 8, 6), { options: { show_threshold_labels: true, show_threshold_markers: false } }],
  ["horizontal, manual, min_height 300", g(0, 14, 12, 8), { options: { orientation: "horizontal", sizing: "manual", min_height: 300 } }],
  ["vertical, manual, min_width 500", g(12, 14, 12, 8), { options: { orientation: "vertical", sizing: "manual", min_width: 500 } }],
  ["vertical, a title", g(0, 22, 6, 6), { fields: { defaults: { min: 0, max: 100, display_name: "Load" }, overrides: [] }, options: { orientation: "vertical" } }],
  ["percentage thresholds, no min and max", g(6, 22, 6, 6), { fields: { defaults: { thresholds: { mode: "percentage", steps: [{ value: null, color: "green" }, { value: 70, color: "orange" }, { value: 90, color: "red" }] } }, overrides: [] } }],
  ["unit percent, no min and max", g(12, 22, 6, 6), { fields: { defaults: { unit: "percent", decimals: 0 }, overrides: [] } }],
  ["scale from the data (no min, no max)", g(18, 22, 6, 6), { fields: { defaults: {}, overrides: [] } }],
  ["fixed colour, a neutral mark", g(0, 28, 6, 6), { fields: { defaults: { min: 0, max: 400, color: { mode: "fixed", fixed: "blue" } }, overrides: [] }, options: { neutral: 200 } }],
  ["thresholds with a neutral mark and labels", g(6, 28, 6, 6), { fields: { defaults: { min: 0, max: 600, thresholds: steps([[null, "green"], [150, "yellow"], [300, "orange"], [600, "red"]]) }, overrides: [] }, options: { neutral: 300, show_threshold_labels: true } }],
  ["a small box (grid height 3)", g(12, 28, 6, 3), { options: {} }],
  ["a tall box (grid height 12)", g(18, 28, 6, 12), { fields: { defaults: { min: 0, max: 400, display_name: "Tall" }, overrides: [] } }],
  ["an override that sets the colour (undecided)", g(0, 34, 6, 6), { fields: { defaults: { min: 0, max: 400 }, overrides: [{ match: { by: "name", value: "Sum" }, set: { color: { mode: "fixed", fixed: "red" } } }] } }],
  ["an override that sets the maximum (undecided)", g(6, 34, 6, 6), { fields: { defaults: { min: 0, max: 400 }, overrides: [{ match: { by: "name", value: "Sum" }, set: { max: 1000 } }] } }],
];

export function gaugeDoc(uid = "values-gauge", time = { from: "now-6h", to: "now", timezone: "utc", refresh: null }, source = SOURCE) {
  const panels = GAUGE_VARIANTS.map(([title, grid, over], i) => gauge(i + 1, title, grid, over, source));
  return doc(uid, "Preview of the gauges: one new field", [section(null, panels)], { time });
}

/** Gauges whose count the document does not fix: the box only. The case A (a pattern that does not compile) is a draft: the server refuses it. */
export const GAUGE_N_VARIANTS = [
  ["values true (the demo gauge)", g(0, 0, 12, 8), { sql: BARS, transforms: [], reduce: { values: true }, fields: { defaults: { min: 0, max: 400 }, overrides: [] } }],
  ["new panel default (no transform)", g(12, 0, 12, 8), { sql: TOTAL, transforms: [] }],
  ["a literal field name", g(0, 8, 12, 8), { sql: TOTAL, transforms: [], reduce: { fields: "/^Total$/" } }],
  ["a last transform that is not a new field", g(12, 8, 12, 8), { transforms: [calc("binary", { left: { kind: "field", field: "Total" }, right: { kind: "number", value: 2 }, operator: "divide" }, { alias: "Half", replace: true })] }],
  ["a chain that ends with a new field, but a pattern of fields", g(0, 16, 12, 8), { reduce: { fields: "/^Sum$/" } }],
];

export function gaugeNDoc(uid = "values-gauge-n", time = { from: "now-6h", to: "now", timezone: "utc", refresh: null }, source = SOURCE) {
  const panels = GAUGE_N_VARIANTS.map(([title, grid, over], i) => gauge(i + 1, title, grid, over, source));
  return doc(uid, "Preview of the gauges: the count is data", [section(null, panels)], { time });
}

// ---- stat ----------------------------------------------------------------------------------------------------------------------------------

const statOptions = (over = {}) => ({ reduce: reduce(), color_mode: "value", graph_mode: "none", text_mode: "auto", orientation: "auto", ...over });

/** The stat variants: the preview is the box of the body, for every option. */
export const STAT_VARIANTS = [
  ["auto, default", g(0, 0, 6, 4), {}],
  ["vertical", g(6, 0, 6, 4), { orientation: "vertical" }],
  ["horizontal", g(12, 0, 6, 4), { orientation: "horizontal" }],
  ["value and name, justify center", g(18, 0, 6, 4), { text_mode: "value_and_name", justify: "center" }],
  ["background colour, area graph", g(0, 4, 6, 3), { color_mode: "background", graph_mode: "area" }],
  ["value size 40, title size 14", g(6, 4, 6, 7), { value_size: 40, title_size: 14, text_mode: "value_and_name" }],
  ["percent change, text none", g(12, 4, 6, 4), { percent_change: true, text_mode: "none" }],
  ["wide and low (grid height 2)", g(18, 4, 6, 2), {}],
  ["two fields, many tiles", g(0, 11, 12, 4), { reduce: reduce({ calc: "last" }), orientation: "horizontal" }],
  ["a tall narrow box", g(12, 11, 4, 10), { orientation: "vertical" }],
];

export function statDoc(uid = "values-stat", time = { from: "now-6h", to: "now", timezone: "utc", refresh: null }, source = SOURCE) {
  const panels = STAT_VARIANTS.map(([title, grid, options], i) => panel(i + 1, "stat", title, TOTAL, { grid, options: statOptions(options), source }));
  return doc(uid, "Preview of the stat panels", [section(null, panels)], { time });
}

/** The stat variants with proof A (the last transform replaces the fields with one new number field): the grid has one tile. */
export const STAT_ONE_VARIANTS = [
  ["auto", g(0, 0, 6, 4), {}],
  ["vertical", g(6, 0, 6, 4), { orientation: "vertical" }],
  ["horizontal, value and name", g(12, 0, 6, 4), { orientation: "horizontal", text_mode: "value_and_name" }],
  ["background fixed colour, area graph", g(18, 0, 6, 4), { color_mode: "background", graph_mode: "area" }, { fields: { defaults: { color: { mode: "fixed", fixed: "purple" } }, overrides: [] } }],
  ["value size 40, change", g(0, 4, 6, 7), { value_size: 40, percent_change: true }],
  ["wide and low (grid height 2)", g(6, 4, 6, 2), { text_mode: "none" }],
  ["a tall narrow box", g(12, 4, 4, 10), { orientation: "vertical" }],
  ["values true: one tile for each row, no proof", g(16, 4, 8, 4), { reduce: reduce({ values: true }) }],
  ["a pattern of fields: no proof", g(0, 11, 6, 4), { reduce: reduce({ fields: "/^Sum$/" }) }],
  ["no transform: no proof", g(6, 11, 6, 4), {}, { transforms: [] }],
];

export function statOneDoc(uid = "values-stat-one", time = { from: "now-6h", to: "now", timezone: "utc", refresh: null }, source = SOURCE) {
  const panels = STAT_ONE_VARIANTS.map(([title, grid, options, extra = {}], i) => panel(i + 1, "stat", title, TOTAL, { grid, options: statOptions(options), source, transforms: ONE, ...extra }));
  return doc(uid, "Preview of the stat panels: one tile", [section(null, panels)], { time });
}

/**
 * Panels that read the data of a source table (the panel 100, with the chain of the case B). The ids 1 to 6 are the stat, the gauge and the bar
 * gauge with the transforms of the source (`with_transforms` true), and the same without (false).
 */
export const REUSE_PANELS = [
  ["stat, the chain of the source", g(0, 0, 8, 4), "stat", true],
  ["stat, own chain only", g(8, 0, 8, 4), "stat", false],
  ["gauge, the chain of the source", g(0, 4, 8, 6), "gauge", true],
  ["gauge, own chain only", g(8, 4, 8, 6), "gauge", false],
  ["gauge, a vertical cell with a title, the chain of the source", g(16, 4, 8, 6), "gauge", true, { fields: { defaults: { min: 0, max: 400, display_name: "Source" }, overrides: [] }, options: { orientation: "vertical" } }],
  ["bar gauge, the chain of the source", g(16, 0, 8, 4), "bargauge", true],
];

export function reuseDoc(uid = "values-reuse", time = { from: "now-6h", to: "now", timezone: "utc", refresh: null }, source = SOURCE) {
  const sourcePanel = panel(100, "table", "Source", TOTAL, { grid: g(0, 10, 24, 5), transforms: ONE, source });
  const panels = REUSE_PANELS.map(([title, grid, type, withTransforms, extra = {}], i) => {
    const reuse = { panel: 100, with_transforms: withTransforms };
    if (type === "gauge") return gauge(i + 1, title, grid, { ...extra, reuse, transforms: [] }, source);
    if (type === "stat") return panel(i + 1, "stat", title, TOTAL, { grid, options: statOptions(), reuse, source });
    return panel(i + 1, "bargauge", title, BARS, { grid, options: { orientation: "horizontal", display_mode: "gradient", show_unfilled: true }, reuse, source });
  });
  return doc(uid, "Preview of the panels that reuse data", [section(null, [...panels, sourcePanel])], { time });
}

// ---- bar gauge -------------------------------------------------------------------------------------------------------------------------

/** The bar gauge variants: the preview is the box, for every orientation and display mode. */
export const BARGAUGE_VARIANTS = [
  ["horizontal gradient", g(0, 0, 8, 4), { orientation: "horizontal", display_mode: "gradient" }],
  ["horizontal basic, no unfilled", g(8, 0, 8, 4), { orientation: "horizontal", display_mode: "basic", show_unfilled: false }],
  ["horizontal lcd", g(16, 0, 8, 7), { orientation: "horizontal", display_mode: "lcd" }],
  ["vertical gradient", g(0, 4, 8, 7), { orientation: "vertical", display_mode: "gradient" }],
  ["vertical lcd", g(8, 4, 8, 3), { orientation: "vertical", display_mode: "lcd" }],
  ["vertical basic", g(16, 7, 8, 4), { orientation: "vertical", display_mode: "basic" }],
  ["values true", g(0, 11, 12, 6), { reduce: reduce({ values: true }), orientation: "horizontal", display_mode: "gradient" }],
  ["a narrow box", g(12, 11, 4, 6), { orientation: "vertical", display_mode: "lcd", reduce: reduce({ values: true }) }],
];

export function barGaugeDoc(uid = "values-bargauge", time = { from: "now-6h", to: "now", timezone: "utc", refresh: null }, source = SOURCE) {
  const panels = BARGAUGE_VARIANTS.map(([title, grid, options], i) =>
    panel(i + 1, "bargauge", title, BARS, { grid, options, source, fields: { defaults: { min: 0, max: 400 }, overrides: [] } }),
  );
  return doc(uid, "Preview of the bar gauges", [section(null, panels)], { time });
}
