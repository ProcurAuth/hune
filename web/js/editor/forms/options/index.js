// The registry of the option tables (10.3h, rule 1, F43): one table for each type that has an option form. Pure data: no DOM access.
import barchart from "./barchart.js";
import bargauge from "./bargauge.js";
import gauge from "./gauge.js";
import geomap from "./geomap.js";
import heatmap from "./heatmap.js";
import histogram from "./histogram.js";
import logs from "./logs.js";
import piechart from "./piechart.js";
import stat from "./stat.js";
import table from "./table.js";
import timeseries from "./timeseries.js";
import xychart from "./xychart.js";

export const TABLES = Object.freeze({ timeseries, barchart, xychart, stat, bargauge, piechart, table, heatmap, histogram, gauge, logs, geomap });

/** The table of a type, or `null` for a type with no table (the two timeline types and `text`). */
export const tableOf = (type) => TABLES[type] || null;

// The members of a type that have no control, with the cause (10.3h, "The members with no control"). A member here is not an error of the audit.
export const NO_CONTROL = Object.freeze({
  text: Object.freeze({ "options.mode": "The member has one value, `markdown` (4.33a).", "options.content": "The group \"Text\" has the field \"Content\" (10.3)." }),
  piechart: Object.freeze({ "options.tooltip.sort": "A pie chart shows one slice at a time, so a sort has no row to order.", "options.tooltip.hide_zeros": "A slice of 0 has no area under the pointer." }),
  table: Object.freeze({ "fields.defaults.hidden": "A default that hides each column. The override form sets it for one column." }),
  timeseries: Object.freeze({ "fields.defaults.hide_from": "A default that hides each series. The override form sets it for one series." }),
  barchart: Object.freeze({ "fields.defaults.hide_from": "A default that hides each series. The override form sets it for one series." }),
  xychart: Object.freeze({ "fields.defaults.hide_from": "A default that hides each series. The override form sets it for one series." }),
  histogram: Object.freeze({ "fields.defaults.hide_from": "A default that hides each series. The override form sets it for one series." }),
  state_timeline: Object.freeze({ "fields.defaults.hide_from": "A default that hides each series. The override form sets it for one series.", "options.legend.mode": "The legend of a timeline has no mode (4.33b).", "options.legend.values": "The legend of a timeline has no values (4.33b).", "options.tooltip.sort": "The card of a timeline has no sort (4.33b)." }),
  status_history: Object.freeze({ "fields.defaults.hide_from": "A default that hides each series. The override form sets it for one series.", "options.legend.mode": "The legend of a timeline has no mode (4.33b).", "options.legend.values": "The legend of a timeline has no values (4.33b).", "options.tooltip.sort": "The card of a timeline has no sort (4.33b)." }),
});

// The optional members of 4.26 that a type has (rule F5 of the audit): each one has a row or an entry of NO_CONTROL.
export const OPTIONAL_MEMBERS = Object.freeze({
  timeseries: ["options.thresholds_style", "options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc", "options.gradient_mode", "options.log_base", "options.span_nulls_ms", "options.bar_alignment"],
  barchart: ["options.thresholds_style", "options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc", "options.gradient_mode"],
  xychart: ["options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc"],
  stat: ["options.justify", "options.percent_change", "options.percent_change_color", "options.title_size", "options.value_size"],
  bargauge: [],
  piechart: ["options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc"],
  table: ["options.wrap"],
  heatmap: [],
  histogram: ["options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc"],
  logs: ["options.show_time", "options.wrap", "options.details", "options.order"],
  geomap: [],
  gauge: ["options.orientation", "options.show_threshold_labels", "options.show_threshold_markers", "options.neutral", "options.title_size", "options.value_size", "options.sizing", "options.min_width", "options.min_height"],
});
