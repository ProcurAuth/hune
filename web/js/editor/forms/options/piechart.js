// The option table of the type `piechart` (10.3h, F43, 4.31).
// It has the groups "Value options", "Pie chart", "Tooltip" and "Legend", and the row "Display name" of "Standard options". It has no DOM access.
// A pie chart shows one slice under the pointer. So `tooltip.sort` and `tooltip.hide_zeros` have no row (NO_CONTROL).
// The choice "All" of the tooltip equals "Single" and stays in the form (decision ID-K296).

import { displayNameRow, legendRows, row, tooltipRows } from "./rows.js";
import { reduceRows } from "./reduce_rows.js";

const r = (id, member, kind, label, rest) => row(id, `options.${member}`, kind, label, "pie", rest);

export default {
  type: "piechart",
  groups: [["value", "Value options"], ["pie", "Pie chart"], ["tooltip", "Tooltip"], ["legend", "Legend"]],
  rows: [
    ...reduceRows(),
    r("pie-type", "pie_type", "radio", "Pie chart type", { items: [["pie", "Pie"], ["donut", "Donut"]], default: "pie" }),
    r("labels", "labels", "checks", "Labels", { items: [["name", "Name"], ["value", "Value"], ["percent", "Percent"]], max: 3, default: [] }),
    ...tooltipRows({ sort: false, hideZeros: false }),
    ...legendRows({ pie: true, placement: "right" }),
    displayNameRow(),
  ],
};
