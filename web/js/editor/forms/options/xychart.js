// The option table of the type `xychart` (10.3h, F43; 4.32). The type has no axis group: the axes have the names of the fields (9.19).
import { displayNameRow, legendRows, row, tooltipRows } from "./rows.js";

export default {
  type: "xychart",
  groups: [["xy", "XY chart"], ["tooltip", "Tooltip"], ["legend", "Legend"]],
  rows: [
    row("x-field", "options.x_field", "field", "X field", "xy", { nullText: "First field of type number", default: null, nullable: true }),
    row("y-fields", "options.y_fields", "fields", "Y fields", "xy", { max: 32, default: [], hint: "None chosen: each other field of type number." }),
    row("draw", "options.draw", "radio", "Show", "xy", { items: [["points", "Points"], ["lines", "Lines"], ["points_and_lines", "Both"]], default: "points" }),
    row("point-size", "options.point_size", "slider", "Point size", "xy", { min: 1, max: 40, default: 5, when: (o) => o.draw !== "lines", whenText: "`draw` is not `lines`" }),
    row("line-width", "options.line_width", "slider", "Line width", "xy", { min: 0, max: 10, default: 1, when: (o) => o.draw !== "points", whenText: "`draw` is not `points`" }),
    ...tooltipRows(),
    ...legendRows(),
    displayNameRow(),
  ],
};
