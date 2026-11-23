// The option table of the type `timeseries` (10.3h, F43; 4.28). Owner: group A. A table is data: the renderer `option_form.js` reads it.
import { SHOW_VALUES, STACKING, displayNameRow, legendRows, oldRow, row, thresholdsStyleRow, tooltipRows } from "./rows.js";

const draw = (o) => o.draw;

/** The text of `line_dash_array` (4.18, P14): "10, 20" gives [10, 20]; empty removes the member. 1 to 8 whole numbers from 0 to 100, one above 0. */
export function parseDash(text) {
  const t = text.trim();
  if (t === "") return { value: null };
  const parts = t.split(/\s*,\s*/);
  const nums = parts.map((p) => (/^(?:0|[1-9][0-9]{0,2})$/.test(p) ? Number(p) : NaN));
  if (parts.length > 8 || nums.some((n) => !(n <= 100)) || !nums.some((n) => n > 0)) return { error: "Use 1 to 8 whole numbers from 0 to 100, one above 0, as 10, 20." };
  return { value: nums };
}
// The rows of wave H, items P10 and P14 (4.18, 4.28). An optional member at its default leaves the panel.
const GRADIENT_MODES = [["none", "None"], ["opacity", "Opacity"], ["hue", "Hue"], ["scheme", "Scheme"]];
export const gradientRow = (group) => row("gradient-mode", "options.gradient_mode", "radio", "Gradient mode", group, { items: GRADIENT_MODES, optional: true, absent: "none", default: undefined });
export const axisLookRows = (centredWhen) => ({
  width: row("axis-width", "fields.defaults.axis_width", "number", "Width", "axis", { min: 0, max: 500, integer: true, nullable: true, optional: true, default: undefined, hint: "Empty: auto." }),
  color: row("axis-color", "fields.defaults.axis_color", "radio", "Color", "axis", { items: [["text", "Text"], ["series", "Series"]], optional: true, absent: "text", default: undefined }),
  border: row("axis-border", "fields.defaults.axis_border", "switch", "Show border", "axis", { optional: true, absent: false, default: undefined }),
  centred: row("axis-centered-zero", "fields.defaults.axis_centered_zero", "switch", "Centered zero", "axis", { optional: true, absent: false, default: undefined, ...centredWhen }),
});
const look = axisLookRows({ when: (o) => o.y_scale !== "log", whenText: "`y_scale` is not `log`" });

export default {
  type: "timeseries",
  groups: [["tooltip", "Tooltip"], ["legend", "Legend"], ["axis", "Axis"], ["style", "Graph styles"]],
  rows: [
    ...tooltipRows(),
    ...legendRows(),
    oldRow("axis-placement", "fields.defaults.axis_placement", "Placement", "axis", "defaults-axis_placement"),
    oldRow("axis-label", "fields.defaults.axis_label", "Label", "axis", "defaults-axis_label"),
    look.width,
    look.color,
    look.border,
    row("y-scale", "options.y_scale", "radio", "Scale", "axis", { items: [["linear", "Linear"], ["log", "Logarithmic"]], default: "linear" }),
    row("log-base", "options.log_base", "radio", "Log base", "axis", { items: [[2, "2"], [10, "10"]], optional: true, absent: 10, default: undefined, when: (o) => o.y_scale === "log", whenText: "`y_scale` is `log`" }),
    look.centred,
    oldRow("axis-soft-min", "fields.defaults.soft_min", "Soft min", "axis", "defaults-soft_min"),
    oldRow("axis-soft-max", "fields.defaults.soft_max", "Soft max", "axis", "defaults-soft_max"),
    row("draw", "options.draw", "radio", "Style", "style", { items: [["line", "Lines"], ["bars", "Bars"], ["points", "Points"]], default: "line" }),
    row("interpolation", "options.interpolation", "select", "Line interpolation", "style", { items: [["linear", "Linear"], ["smooth", "Smooth"], ["step_before", "Step before"], ["step_after", "Step after"]], default: "linear", when: (o) => draw(o) === "line", whenText: "`draw` is `line`" }),
    row("bar-alignment", "options.bar_alignment", "radio", "Bar alignment", "style", { items: [["before", "Before"], ["center", "Center"], ["after", "After"]], optional: true, absent: "center", default: undefined, when: (o) => draw(o) === "bars", whenText: "`draw` is `bars`" }),
    row("line-width", "options.line_width", "slider", "Line width", "style", { min: 0, max: 10, default: 1, when: (o) => draw(o) !== "points", whenText: "`draw` is not `points`" }),
    row("fill-opacity", "options.fill_opacity", "slider", "Fill opacity", "style", { min: 0, max: 100, default: 0, when: (o) => draw(o) !== "points", whenText: "`draw` is not `points`" }),
    gradientRow("style"),
    oldRow("line-dash", "fields.defaults.line_dash", "Line style", "style", "defaults-line_dash", { when: (o) => draw(o) === "line", whenText: "`draw` is `line`" }),
    row("dash-array", "fields.defaults.line_dash_array", "text", "Dash array", "style", { nullable: true, optional: true, default: undefined, parse: parseDash, format: (v) => (Array.isArray(v) ? v.join(", ") : ""), hint: "As 10, 20 (px). Empty: the default of the line style.", when: (o) => draw(o) === "line", whenText: "`draw` is `line`" }),
    row("span-nulls", "options.span_nulls", "switch", "Connect null values", "style", { default: false, when: (o) => draw(o) === "line", whenText: "`draw` is `line`" }),
    row("span-nulls-ms", "options.span_nulls_ms", "number", "Null threshold", "style", { min: 1, max: 1e12, integer: true, nullable: true, optional: true, default: undefined, hint: "In ms: a shorter run of nulls joins. Empty: none.", when: (o) => draw(o) === "line" && o.span_nulls !== true, whenText: "`draw` is `line` and `span_nulls` is `false`" }),
    row("show-points", "options.show_points", "radio", "Show points", "style", { items: SHOW_VALUES, default: "auto", when: (o) => draw(o) === "line", whenText: "`draw` is `line`" }),
    row("point-size", "options.point_size", "slider", "Point size", "style", { min: 1, max: 40, default: 5, when: (o) => draw(o) === "points" || (draw(o) === "line" && o.show_points !== "never"), whenText: "`draw` is `points`, or `draw` is `line` and `show_points` is not `never`" }),
    row("stacking", "options.stacking", "radio", "Stack series", "style", { items: STACKING, default: "none" }),
    oldRow("transform", "fields.defaults.transform", "Transform", "style", "defaults-transform"),
    thresholdsStyleRow(),
    displayNameRow(),
  ],
};
