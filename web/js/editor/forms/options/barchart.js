// The option table of the type `barchart` (10.3h, F43; 4.30). Owner: group A.
import { SHOW_VALUES, STACKING, displayNameRow, legendRows, oldRow, row, thresholdsStyleRow, tooltipRows } from "./rows.js";
import { axisLookRows, gradientRow } from "./timeseries.js";

// Wave H, item P14: a bar chart has no log scale, so "Centered zero" always shows.
const look = axisLookRows({});

export default {
  type: "barchart",
  groups: [["bars", "Bar chart"], ["tooltip", "Tooltip"], ["legend", "Legend"], ["axis", "Axis"]],
  rows: [
    row("x-field", "options.x_field", "field", "X Axis", "bars", { nullText: "First string or time field", default: null, nullable: true }),
    row("orientation", "options.orientation", "radio", "Orientation", "bars", { items: [["auto", "Auto"], ["horizontal", "Horizontal"], ["vertical", "Vertical"]], default: "auto" }),
    row("x-tick-rotation", "options.x_tick_rotation", "slider", "Rotate x-axis tick labels", "bars", { min: -90, max: 90, default: 0, when: (o) => o.orientation !== "horizontal", whenText: "`orientation` is not `horizontal`" }),
    row("show-values", "options.show_values", "radio", "Show values", "bars", { items: SHOW_VALUES, default: "auto", when: (o) => o.stacking === "none", whenText: "`stacking` is `none`" }),
    row("stacking", "options.stacking", "radio", "Stacking", "bars", { items: STACKING, default: "none" }),
    row("group-width", "options.group_width", "slider", "Group width", "bars", { min: 10, max: 100, default: 70, when: (o) => o.stacking === "none", whenText: "`stacking` is `none`" }),
    row("bar-width", "options.bar_width", "slider", "Bar width", "bars", { min: 10, max: 100, default: 97 }),
    // Wave H, item P03 (4.30): optional members; the default value removes the member.
    row("line-width", "options.line_width", "slider", "Line width", "bars", { min: 0, max: 10, optional: true, absent: 1, default: undefined }),
    row("fill-opacity", "options.fill_opacity", "slider", "Fill opacity", "bars", { min: 0, max: 100, optional: true, absent: 80, default: undefined }),
    gradientRow("bars"),
    ...tooltipRows(),
    ...legendRows(),
    oldRow("axis-label", "fields.defaults.axis_label", "Label", "axis", "defaults-axis_label"),
    look.width,
    // `axis_grid` (4.18): Grafana has Auto, On and Off; one value axis makes On equal to Auto, so the switch has two states (10.3h).
    row("axis-grid", "fields.defaults.axis_grid", "switch", "Show grid lines", "axis", { optional: true, absent: true, default: undefined }),
    look.color,
    look.border,
    look.centred,
    oldRow("axis-soft-min", "fields.defaults.soft_min", "Soft min", "axis", "defaults-soft_min"),
    oldRow("axis-soft-max", "fields.defaults.soft_max", "Soft max", "axis", "defaults-soft_max"),
    thresholdsStyleRow(),
    displayNameRow(),
  ],
};
