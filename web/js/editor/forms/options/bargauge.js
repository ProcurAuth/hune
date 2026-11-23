// The option table of the type `bargauge` (10.3h, F43, 4.33).
// It has the groups "Value options" and "Bar gauge", and the row "Display name" of "Standard options". It has no DOM access.
// The row "Show unfilled area" shows for each display mode. The bar gauge of Hune draws the unfilled track in the mode `lcd` too (19.20m).

import { displayNameRow, row } from "./rows.js";
import { reduceRows } from "./reduce_rows.js";

const r = (id, member, kind, label, rest) => row(id, `options.${member}`, kind, label, "gauge", rest);

export default {
  type: "bargauge",
  groups: [["value", "Value options"], ["gauge", "Bar gauge"]],
  rows: [
    ...reduceRows(),
    r("orientation", "orientation", "radio", "Orientation", { items: [["horizontal", "Horizontal"], ["vertical", "Vertical"]], default: "horizontal" }),
    r("display-mode", "display_mode", "radio", "Display mode", { items: [["gradient", "Gradient"], ["lcd", "Retro LCD"], ["basic", "Basic"]], default: "gradient" }),
    r("show-unfilled", "show_unfilled", "switch", "Show unfilled area", { default: true }),
    displayNameRow(),
  ],
};
