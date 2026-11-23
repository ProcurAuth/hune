// The option table of the type `stat` (10.3h, F43, 4.29).
// It has the groups "Value options", "Stat styles" and "Text size", and the row "Display name" of "Standard options". It has no DOM access.

import { displayNameRow, row } from "./rows.js";
import { reduceRows } from "./reduce_rows.js";

const r = (id, member, kind, label, rest, group = "styles") => row(id, `options.${member}`, kind, label, group, rest);
// The rows of wave H, item P11. A row shows only where it can change the picture (10.3h, rule 14): the change needs a calculation, its colour
// needs the change, and the name size needs a name.
const calculated = (o) => !(o.reduce && o.reduce.values);
const size = (id, member, label, rest) => r(id, member, "number", label, { min: 1, max: 200, integer: true, nullable: true, optional: true, default: undefined, hint: "Empty: auto.", ...rest }, "text");

export default {
  type: "stat",
  groups: [["value", "Value options"], ["styles", "Stat styles"], ["text", "Text size"]],
  rows: [
    ...reduceRows(),
    r("orientation", "orientation", "radio", "Orientation", { items: [["auto", "Auto"], ["horizontal", "Horizontal"], ["vertical", "Vertical"]], default: "auto" }),
    r("text-mode", "text_mode", "select", "Text mode", { items: [["auto", "Auto"], ["value", "Value"], ["value_and_name", "Value and name"], ["name", "Name"], ["none", "None"]], default: "auto" }),
    r("color-mode", "color_mode", "select", "Color mode", { items: [["none", "None"], ["value", "Value"], ["background", "Background"]], default: "value" }),
    r("graph-mode", "graph_mode", "radio", "Graph mode", { items: [["none", "None"], ["area", "Area"]], default: "area" }),
    r("justify", "justify", "radio", "Text alignment", { items: [["auto", "Auto"], ["center", "Center"]], optional: true, absent: "auto", default: undefined }),
    r("percent-change", "percent_change", "switch", "Show percent change", { optional: true, absent: false, default: undefined, when: calculated, whenText: "`reduce.values` is `false`" }),
    r("percent-change-color", "percent_change_color", "select", "Percent change color mode", {
      items: [["standard", "Standard"], ["inverted", "Inverted"], ["same_as_value", "Same as Value"]], optional: true, absent: "standard", default: undefined,
      when: (o) => calculated(o) && o.percent_change === true, whenText: "`reduce.values` is `false` and `percent_change` is `true`",
    }),
    size("title-size", "title_size", "Title", { when: (o) => o.text_mode === "auto" || o.text_mode === "value_and_name", whenText: "`text_mode` is `auto` or `value_and_name`" }),
    size("value-size", "value_size", "Value"),
    displayNameRow(),
  ],
};
