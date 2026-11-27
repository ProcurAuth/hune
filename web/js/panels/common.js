// Helpers shared by the panel types.

import { chartCore } from "../vendor/chdash/app_chart_core.js";
import { reduceField } from "../reduce.js";
import { fallbackColor, tickLabel } from "./series.js";
import { isThemedMode, markColor } from "../colors.js";
import { autoTextDecimals, formatNumber, joinParts, sigDecimals } from "../units.js";

// The names of the panel types for the label of a chart (9.26).
const TYPE_NAMES = { timeseries: "time series", barchart: "bar", xychart: "xy", piechart: "pie", stat: "stat", bargauge: "bar gauge", table: "table", state_timeline: "state timeline", status_history: "status history" };

export function chartLabel(panel) {
  return `${panel.title || "Untitled"}, ${TYPE_NAMES[panel.type] || panel.type} chart`;
}

// The engine puts role="application" on its cursor overlay. A chart has role="img" and an aria-label (9.26).
export function labelChart(chart, panel) {
  const overlay = chart.root.querySelector(".chartCore__overlay");
  if (overlay) {
    overlay.setAttribute("role", "img");
    overlay.removeAttribute("aria-roledescription");
    overlay.setAttribute("aria-label", chartLabel(panel));
  }
}

// The Float64 values of a number field for a chart: NaN for NULL.
export function numbersOf(field) {
  return field.kind === "number_text" ? field.num : field.values;
}

// True when the legend stands beside the plot (placement "right" and a host wide enough): then it takes no height.
function legendBeside(chart) {
  const legend = chart.root.querySelector(".chartCore__legend");
  const plot = chart.root.querySelector(".chartCore__plot");
  if (!legend || legend.hidden || !plot) return false;
  return legend.offsetLeft >= plot.offsetLeft + plot.offsetWidth - 1;
}

const LEGEND_MAX_SHARE = 0.35;

// The height of the first draw of a chart, before its legend exists. A legend of one row at the bottom takes 31 px (the CSS of the engine).
// A chart with no legend takes the whole host. A guess that is wrong costs one more draw, never a wrong picture: fitHeight corrects it.
// The 31 px come from the CSS of Hune (panel-chart.css and the legend rows of the engine copy): change them together.
const LEGEND_ROW = 31;
export function firstHeight(host, legend = true) {
  return Math.max(60, host.clientHeight - (legend ? LEGEND_ROW : 0));
}

// Keeps the plot height equal to the host height minus the legend. The engine draws the width by itself.
// A legend beside the plot (placement "right") takes no height: the plot uses the whole host.
// `drawn` is the height of the first draw (the option `height`): a height that is the same does not draw the chart again.
// A legend at the right narrows the plot after the first draw: the update then draws in this frame, as it did before.
export function fitHeight(host, chart, minHeight = 60, drawn = 0) {
  const legend = chart.root.querySelector(".chartCore__legend");
  const plot = chart.root.querySelector(".chartCore__plot");
  let last = host.dataset && host.dataset.legendPlacement === "right" ? 0 : drawn;
  let gone = false; // the host had no width: its frame was out of the page
  const fit = () => {
    // The legend at the bottom keeps at most 35 % of the height of the panel and scrolls beyond it (Grafana). The CSS reads --legend-max.
    if (legend && legend.style) legend.style.setProperty("--legend-max", `${Math.floor(host.clientHeight * LEGEND_MAX_SHARE)}px`);
    const legendH = legend && !legend.hidden && !legendBeside(chart) ? legend.offsetHeight : 0;
    const height = Math.max(minHeight, Math.floor(host.clientHeight - legendH));
    if (height !== last) {
      last = height;
      chart.update({ height });
    }
    // A chart that returns to the page draws before the paint, not one frame later on a blank canvas (10.2a, rule 17).
    if (host.clientWidth && gone) {
      chart.redraw();
      chart.flush();
    }
    gone = !host.clientWidth;
  };
  fit();
  const observer = new ResizeObserver(fit);
  observer.observe(host);
  if (legend) observer.observe(legend);
  if (plot) observer.observe(plot);
  // The page shows a state (dashboard.js, settle): a chart that returns draws in this task, not at the next resize callback.
  const shown = () => gone && fit();
  document.addEventListener("hune-shown", shown);
  return () => {
    observer.disconnect();
    document.removeEventListener("hune-shown", shown);
  };
}

// The y axis bounds that the document sets for a field (the data bounds are not explicit).
export function explicitBounds(field) {
  const set = (field.config && field.config.set) || {};
  return [Number.isFinite(set.min) ? set.min : null, Number.isFinite(set.max) ? set.max : null];
}

// The text of a tick of the value axis, as Grafana writes it: a field with `decimals` writes each tick with them (70.0%), a unit gives the
// display rule of 9.15 ("200 K", "1 Mil"), and a plain number gives the least digits ("2", "1.5"). `(v, step) => text`.
export function yTickLabeller(field) {
  const c = field ? field.config : null;
  const unit = c ? c.unit : "none";
  const fixed = c && c.decimals !== null && c.decimals !== undefined ? c.decimals : null;
  if (fixed !== null || (unit && unit !== "none")) return (v) => joinParts(formatNumber(v, unit, fixed));
  return tickLabel;
}

// The text of one value of a field in its unit and decimals (a label of a bar).
export function joinPartsOf(field, v) {
  const c = field.config;
  return Number.isFinite(v) ? joinParts(formatNumber(v, c.unit, c.decimals)) : "";
}

// The value text of a series with its unit and decimals.
export function valueFormatter(field) {
  const c = field.config;
  return (v) => (Number.isFinite(v) ? joinParts(formatNumber(v, c.unit, c.decimals)) : "NULL");
}

// A number that has only zeros after the point reads as a whole number: "69.0" gives "69", "1.00 KiB" gives "1 KiB" (Grafana).
export function trimZeros(text) {
  return text.replace(/(\d)\.0+(?!\d)/, "$1");
}

// The text of a value in a tooltip row. A field with `decimals` keeps them. Else the value has 3 significant digits in its unit
// (31 200 with the unit `short` gives "31.2 K"), so that rows that differ by a few percent do not read the same. A whole
// number has no decimals ("69", not "69.0").
export function tooltipFormatter(field) {
  const c = field.config;
  if (c.decimals !== null && c.decimals !== undefined) return valueFormatter(field);
  return (v) => (Number.isFinite(v) ? trimZeros(joinParts(formatNumber(v, c.unit, sigDecimals(v, c.unit, 3)))) : "NULL");
}

// The `formatValue(value, series)` option of the engine: the tooltip text of the series, else of the first series.
export function tooltipFormat(first) {
  const cache = new Map();
  const of = (field) => {
    if (!cache.has(field)) cache.set(field, tooltipFormatter(field));
    return cache.get(field);
  };
  return (v, s) => (s && s.field ? of(s.field)(v) : first ? of(first)(v) : String(v));
}

const LEGEND_COLUMNS = ["total", "min", "max", "mean", "last"];

// The calculations that count values: a count has no unit. The difference in percent is a percentage.
const COUNT_CALCS = new Set(["count", "distinct_count", "change_count"]);

// The number of a legend calculation of a series, NaN when it has none. The five base columns come from the engine over the visible
// range. The other calculations read the whole field (9.19, reduce.js).
export function legendCalc(series, name) {
  const field = series && series.field;
  if (!field) return NaN;
  const r = reduceField(field, name);
  return r && typeof r.num === "number" && Number.isFinite(r.num) ? r.num : NaN;
}

// Total, min, max, mean and last of the finite values of a series, NaN when it has none.
export function reduceValues(values) {
  let min = Infinity, max = -Infinity, total = 0, count = 0, last = NaN;
  for (let i = 0; i < values.length; i++) {
    const v = values[i];
    if (!Number.isFinite(v)) continue;
    if (v < min) min = v;
    if (v > max) max = v;
    total += v;
    count += 1;
    last = v;
  }
  return count ? { total, min, max, mean: total / count, last } : { total: NaN, min: NaN, max: NaN, mean: NaN, last: NaN };
}

// The `legendFormat(value, column, series)` option of the engine. One column keeps one precision for all series of the same unit:
// the largest number of decimals that the fields set, or that the display rules give to a value of the column. The digits then
// line up in the table. The total of a percentage is a sum of percentages: it shows as a plain number, with no `%`.
export function legendFormatter(series, columns = []) {
  const calc = new Map(series.map((s) => [s.id, reduceValues(s.values)]));
  const extra = columns.filter((c) => !LEGEND_COLUMNS.includes(c));
  for (const s of series) for (const c of extra) calc.get(s.id)[c] = legendCalc(s, c);
  const unitOf = (s, column) => {
    const unit = s.field.config.unit || "none";
    if (COUNT_CALCS.has(column)) return "none";
    if (column === "diff_percent") return "percent";
    return column === "total" && (unit === "percent" || unit === "percentunit") ? "none" : unit;
  };
  const fixed = (s, column) => (COUNT_CALCS.has(column) ? 0 : s.field.config.decimals !== null && s.field.config.decimals !== undefined ? s.field.config.decimals : null);
  const decimals = new Map();
  for (const column of [...LEGEND_COLUMNS, ...extra]) {
    for (const s of series) {
      const v = calc.get(s.id)[column];
      if (!Number.isFinite(v)) continue;
      const unit = unitOf(s, column);
      const d = fixed(s, column) !== null ? fixed(s, column) : autoTextDecimals(v, unit);
      const key = `${column}|${unit}`;
      decimals.set(key, Math.max(decimals.get(key) || 0, d));
    }
  }
  return (v, column, s) => {
    if (!Number.isFinite(v)) return "\u2014";
    if (!s || !s.field) return String(v);
    const unit = unitOf(s, column);
    const d = decimals.get(`${column}|${unit}`);
    return joinParts(formatNumber(v, unit, d === undefined ? fixed(s, column) : d));
  };
}

// The digits of the time in the tooltip head: milliseconds only when the step of the x axis is under one second.
export function timeFractionDigits(xs) {
  const diffs = [];
  for (let i = 1; i < xs.length && diffs.length < 200; i++) {
    const d = xs[i] - xs[i - 1];
    if (d > 0) diffs.push(d);
  }
  if (!diffs.length) return 0;
  diffs.sort((a, b) => a - b);
  return diffs[diffs.length >> 1] < 1000 ? 3 : 0;
}

// The fill under the lines when several series overlap (no stack). Grafana keeps the configured opacity for each series, so the
// fills add up where the lines overlap: the function returns the opacity as it is (kept as a hook of the panels).
export function overlapFill(opacity) {
  return opacity;
}

export const CALC_COLUMNS = new Set([...LEGEND_COLUMNS, "last_not_null", "first", "first_not_null", "count", "median", "std_dev", "variance", "range", "diff", "diff_percent", "delta", "distinct_count", "change_count"]);

// The heads of the legend table (the names of Grafana, as the copied engine writes them: `LEGEND_HEADS` of app_chart_core.js).
export const LEGEND_HEADS = Object.freeze({
  total: "Total", min: "Min", max: "Max", mean: "Mean", last: "Last",
  last_not_null: "Last *", first: "First", first_not_null: "First *", count: "Count", median: "Median", std_dev: "Std Dev",
  variance: "Variance", range: "Range", diff: "Difference", diff_percent: "Difference %", delta: "Delta",
  distinct_count: "Distinct count", change_count: "Change count",
});

// The flags of a series from the property `hide_from` of its field (9.19, F03): `viz` is not drawn (the engine hides it, and the legend
// shows it as off), `legend` has no entry in the legend, `tooltip` has no row and no dot. Returns the fields to merge into the series.
export function hideFlags(field) {
  const h = (field && field.config && field.config.hide_from) || {};
  return { legendHidden: h.legend === true, tooltipHidden: h.tooltip === true, vizHidden: h.viz === true };
}

// The ids of the series that the page does not draw (`hide_from.viz`), for the option `hidden` of the engine.
export function hiddenIds(series) {
  return new Set(series.filter((s) => s.vizHidden).map((s) => s.id));
}

// The engine options of the shared objects legend and tooltip (4.26, 9.19).
export function legendOptions(options) {
  const legend = options.legend || { show: true, placement: "bottom", mode: "list", values: [] };
  const tooltip = options.tooltip ? options.tooltip.mode : "single";
  return {
    legend: legend.show ? "always" : false,
    // The columns keep the order of the document, as Grafana does. Every calculation of the format is a column (9.19).
    legendColumns: legend.values && legend.values.length ? legend.values.filter((v) => CALC_COLUMNS.has(v)) : undefined,
    legendSort: legend.sort_by ? { by: legend.sort_by, desc: legend.sort_desc === true } : null,
    legendCalc,
    tooltip: tooltip === "hidden" ? "none" : tooltip === "multi" ? "all" : "single",
    tooltipSort: options.tooltip && (options.tooltip.sort === "asc" || options.tooltip.sort === "desc") ? options.tooltip.sort : undefined,
    tooltipHideZeros: !!(options.tooltip && options.tooltip.hide_zeros),
    mode: legend.mode,
    placement: legend.placement,
  };
}

// The room that the legend at the right leaves to the plot. Under it, the legend moves under the plot (data-legend-fallback).
const MIN_PLOT_WIDTH = 220;
const LEGEND_GAP = 8;

// Applies the per-panel legend mode and placement (9.24: the placement is CSS, the mode is an engine call).
// The legend at the right keeps all its columns: when the host is too narrow for the legend and a plot of MIN_PLOT_WIDTH, it moves under the plot.
export function applyLegend(host, chart, legend) {
  // A legend that is off has no placement: the grid of the placement "right" would keep its gap and narrow the plot (10.3h, rule 15).
  host.dataset.legendPlacement = legend.show === false ? "bottom" : legend.placement || "bottom";
  if (legend.mode === "list" || legend.mode === "table") chart.setLegendMode(legend.mode);
  // A table with no value column shows the column "Name" only (4.26). The engine falls back to its five columns: the CSS hides them.
  const columns = (legend.values || []).filter((v) => CALC_COLUMNS.has(v));
  if (legend.mode === "table" && !columns.length) host.dataset.legendColumns = "none";
  else delete host.dataset.legendColumns;
  const legendEl = chart.root.querySelector(".chartCore__legend");
  if (!legendEl) return;
  let busy = false;
  // A legend that the engine hides (every entry has `hide_from.legend`) takes no room: the placement "bottom" keeps no grid gap (9.19).
  const sync = () => {
    if (legend.show === false) return;
    host.dataset.legendPlacement = legendEl.hidden ? "bottom" : legend.placement || "bottom";
  };
  const place = () => {
    if (busy || legend.placement !== "right") return;
    busy = true;
    sync();
    // Measure the legend beside the plot, then decide.
    const had = host.hasAttribute("data-legend-fallback");
    if (had) host.removeAttribute("data-legend-fallback");
    const need = legendEl.hidden ? 0 : legendEl.offsetWidth + MIN_PLOT_WIDTH + LEGEND_GAP;
    const narrow = need > 0 && host.clientWidth < need;
    if (narrow) host.setAttribute("data-legend-fallback", "");
    busy = false;
  };
  place();
  const mutations = new MutationObserver(place);
  mutations.observe(legendEl, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-mode", "hidden"] });
  const resizes = new ResizeObserver(place);
  resizes.observe(host);
  const destroy = chart.destroy;
  chart.destroy = () => {
    mutations.disconnect();
    resizes.disconnect();
    return destroy();
  };
}

// True when the page shows the dark theme.
export const isDarkTheme = () => typeof document === "undefined" || document.documentElement.dataset.theme !== "light";

/**
 * The colour of a series for the chart (9.16): `{ color, colorFor }`. `color` is the CSS colour of a mark in the theme of the page.
 * `palette` and the continuous modes depend on the theme beyond the shade, so they also give `colorFor(dark)`: `createChart` calls it
 * when the theme changes. `ctx` has `index` (the index among the number fields) and `shared` (`{ min, max }` of all the
 * number fields together: the scale of a continuous mode when the field sets no end).
 */
export function chartColor(field, value, { index, shared }) {
  const cfg = field.config;
  const set = cfg.set || {};
  const continuous = cfg.color && cfg.color.mode && cfg.color.mode.startsWith("continuous_");
  const make = (dark) => {
    const min = continuous && shared && !Number.isFinite(set.min) && Number.isFinite(shared.min) ? shared.min : cfg.min;
    const max = continuous && shared && !Number.isFinite(set.max) && Number.isFinite(shared.max) ? shared.max : cfg.max;
    return markColor(cfg, value, { min, max, seriesIndex: index, dark }) || fallbackColor(index);
  };
  return { color: make(isDarkTheme()), colorFor: cfg.color && isThemedMode(cfg.color.mode) ? make : undefined };
}

// Makes the chart. The engine watches the attribute data-theme. The event "hune-theme" (a change that comes from the
// theme control, another tab or the system) also makes it read the colours of the page again and draw the canvas again.
export function createChart(host, options) {
  // The legend mode is the option of the document (9.19). The engine has no button of the mode (E41).
  const chart = chartCore.create(host, options);
  const onTheme = () => {
    // A series whose colour depends on the theme (`colorFor`) gets its colour for the new theme before the engine reads the page again.
    const dark = isDarkTheme();
    for (const s of options.series || []) if (typeof s.colorFor === "function") s.color = s.colorFor(dark);
    chart.themeChanged();
  };
  document.addEventListener("hune-theme", onTheme);
  const destroy = chart.destroy;
  chart.destroy = () => {
    document.removeEventListener("hune-theme", onTheme);
    return destroy();
  };
  return chart;
}

// Reads a CSS colour from the page for the canvas (a canvas cannot use var()).
export function canvasColor(host, css) {
  if (typeof css === "string" && css.startsWith("var(")) {
    const name = css.slice(4, -1).trim();
    return getComputedStyle(host).getPropertyValue(name).trim() || "#888";
  }
  return css;
}
