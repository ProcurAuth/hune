// SPDX-License-Identifier: MIT OR Apache-2.0
// The closed list of the audit B153 for stat, bargauge and piechart (10.3h, rule 14).
// An entry covers one member and the members under it, for the types that it names. A member with `=` and a value covers that value only.
// `cls` is `design` (the control cannot change the picture by design) or `data` (the data of the audit does not show the effect).
// A stale entry (its control changes the preview) makes B153 fail. A dead entry (no control matches it) makes B153 fail too.
// So a worker removes an entry when its control starts to work. The entries `grid` and `repeat` (all types) are in the list of group A.
const SMALL = ["stat", "bargauge", "piechart", "gauge"];

export const EXEMPT = [
  { pointer: "fields.defaults.links", types: SMALL, cls: "design", cause: "a data link shows on a click only, and the audit does not click" },
  { pointer: "fields.defaults.mappings", types: SMALL, cls: "data", cause: "a new mapping matches no value of the data of the audit" },
  { pointer: "fields.overrides", types: SMALL, cls: "data", cause: "a new override has one property for a field that the data does not use" },
  { pointer: "fields.defaults.unit", types: ["piechart"], cls: "data", cause: "the pie of the audit has no label and no legend value that shows a unit" },
  { pointer: "fields.defaults.decimals", types: ["piechart"], cls: "data", cause: "the pie of the audit has no label and no legend value that shows decimals" },
  { pointer: "fields.defaults.min", types: ["piechart", "stat"], cls: "data", cause: "no scale, gauge or percent threshold of these types uses the limit with this data" },
  { pointer: "fields.defaults.max", types: ["piechart", "stat"], cls: "data", cause: "no scale, gauge or percent threshold of these types uses the limit with this data" },
  { pointer: "fields.defaults.no_value", types: ["bargauge", "piechart", "gauge"], cls: "data", cause: "no value of the data of these types is null in the preview" },
  { pointer: "fields.defaults.thresholds=percentage", types: ["stat"], cls: "data", cause: "the percentage needs a minimum and a maximum; the audit sets none" },
  { pointer: "options.orientation=vertical", types: ["stat"], cls: "data", cause: "the mode Auto already puts the items side by side at the size of the audit panel" },
  // The gauge: the manual size with the default least sizes of 75 px gives the cells of the automatic size (9.19g).
  { pointer: "options.sizing", types: ["gauge"], cls: "design", cause: "the manual size with its default least sizes of 75 px gives the cells of the automatic size" },
  { pointer: "options.tooltip.mode=multi", types: ["piechart"], cls: "design", cause: "a pie has one slice under the pointer, so All equals Single" },
];

// The point of the hover probe (10.3h, rule 14) of the types that have a tooltip, as fractions of the box of the element `sel` of the preview.
// The point is on a slice of the pie and outside the hole of a donut (the radius of the hole is 0.55 of the radius of the pie), so the card
// shows for both pie types. A stat and a bar gauge have no tooltip.
export const HOVER = { piechart: { sel: ".pie__canvas", x: 0.5, y: 0.15 } };
