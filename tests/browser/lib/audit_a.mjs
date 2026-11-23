// SPDX-License-Identifier: MIT OR Apache-2.0
// The closed list of the audit B153 for the types of group A and for the entries that all types share (10.3h rule 14).
// An entry: { pointer, types, cls, cause }. `pointer` is a member from the panel ("options.x.y" or "fields.defaults.z"),
// or a member and a value ("options.tooltip.mode=multi"). `types` null: all types. `cls` is "design" (the control cannot change the picture by
// design) or "data" (the data of the audit does not show the effect). `cause` has 20 characters or more. An entry covers its member and the members
// under it. A stale entry (the preview changes) and a dead entry (no control matches it) fail the audit. A new entry needs a reviewed decision.
// The lists of the three groups merge by member, class and cause: the types add up (`mergeExempt` of audit_rules.mjs). The list has 30 entries at most.
const TIMELINES = ["state_timeline", "status_history"];

const DATA_TYPES = ["timeseries", "barchart", "xychart"];
const LINK_CAUSE = "a data link shows on a click only, and the audit does not click";
const MAPPING_CAUSE = "a new mapping matches no value of the data of the audit";
const OVERRIDE_CAUSE = "a new override has one property for a field that the data does not use";

export const EXEMPT = [
  { pointer: "grid.x", types: null, cls: "design", cause: "the preview fills its area; the place shows in the grid of the view" },
  { pointer: "grid.y", types: null, cls: "design", cause: "the preview fills its area; the place shows in the grid of the view" },
  { pointer: "grid.w", types: null, cls: "design", cause: "the preview fills its area; the width shows in the grid of the view" },
  { pointer: "repeat.direction", types: null, cls: "design", cause: "the preview is one panel (10.3); the copies show in the view" },
  { pointer: "repeat.max_per_row", types: null, cls: "design", cause: "the preview is one panel (10.3); the copies show in the view" },
  { pointer: "fields.defaults.links", types: [...DATA_TYPES, ...TIMELINES], cls: "design", cause: LINK_CAUSE },
  { pointer: "fields.defaults.mappings", types: DATA_TYPES, cls: "data", cause: MAPPING_CAUSE },
  { pointer: "fields.overrides", types: [...DATA_TYPES, ...TIMELINES], cls: "data", cause: OVERRIDE_CAUSE },
  { pointer: "fields.defaults.no_value", types: [...DATA_TYPES, ...TIMELINES], cls: "data", cause: "no value of the data of these types is null in the preview" },
  { pointer: "fields.defaults.min", types: ["xychart", ...TIMELINES], cls: "data", cause: "no scale, gauge or percent threshold of these types uses the limit with this data" },
  { pointer: "fields.defaults.max", types: ["xychart", ...TIMELINES], cls: "data", cause: "no scale, gauge or percent threshold of these types uses the limit with this data" },
  { pointer: "options.show_values=always", types: ["barchart", ...TIMELINES], cls: "data", cause: "the mode Auto already shows each text that fits with this data" },
  { pointer: "options.tooltip.hide_zeros", types: DATA_TYPES, cls: "data", cause: "the audit data has two series, a null and no zero (10.3h, rule 14)" },
  // Items P03: the default `auto` of a new bar chart draws vertical bars in the preview, which is wider than it is high (9.19).
  { pointer: "options.orientation=vertical", types: ["barchart"], cls: "data", cause: "the preview is wider than it is high, so the default Auto already draws vertical bars" },
  // Items P10: a new time series has fill_opacity 0, so the opacity gradient has no fill to paint (10.3h).
  { pointer: "options.gradient_mode=opacity", types: ["timeseries"], cls: "data", cause: "a new time series has fill_opacity 0, so the opacity gradient has no fill to paint" },
];

// The point of the hover probe (10.3h, rule 14) for the types that have a tooltip: { sel, x, y }. `sel` is the element of the preview (default: the
// plot, `.panel__plot`); `x` and `y` are fractions of its box (from the left and from the top). A type with no entry has no hover probe.
export const HOVER = {
  timeseries: { x: 0.5, y: 0.5 },
  barchart: { x: 0.5, y: 0.5 },
  xychart: { x: 0.5, y: 0.5 },
  state_timeline: { x: 0.5, y: 0.2 },
  status_history: { x: 0.5, y: 0.2 },
};
