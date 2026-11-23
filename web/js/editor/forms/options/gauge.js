// The option table of the type `gauge` (10.3h, 4.33e, 9.19g).
// It has the groups "Value options", "Gauge" and "Text size", and the row "Display name" of "Standard options". It has no DOM access.
// The labels are the labels of the classic gauge of Grafana 12.4.2. Each member but `reduce` is optional: a value equal to `absent` removes it.

import { displayNameRow, row } from "./rows.js";
import { reduceRows } from "./reduce_rows.js";

const r = (id, member, kind, label, rest, group = "gauge") => row(id, `options.${member}`, kind, label, group, rest);
const size = (id, member, label) => r(id, member, "number", label, { min: 1, max: 200, integer: true, nullable: true, optional: true, default: undefined, hint: "Empty: auto." }, "text");
// A least size shows where it acts: with the manual size and the orientation that reads it (9.19g).
const minSize = (id, member, label, orientation) =>
  r(id, member, "slider", label, { min: 0, max: 600, step: 1, optional: true, absent: 75, default: undefined, when: (o) => o.sizing === "manual" && o.orientation === orientation, whenText: `\`sizing\` is \`manual\` and \`orientation\` is \`${orientation}\`` });

export default {
  type: "gauge",
  groups: [["value", "Value options"], ["gauge", "Gauge"], ["text", "Text size"]],
  rows: [
    ...reduceRows(),
    r("orientation", "orientation", "radio", "Orientation", { items: [["auto", "Auto"], ["horizontal", "Horizontal"], ["vertical", "Vertical"]], optional: true, absent: "auto", default: undefined }),
    r("threshold-labels", "show_threshold_labels", "switch", "Show threshold labels", { optional: true, absent: false, default: undefined }),
    r("threshold-markers", "show_threshold_markers", "switch", "Show threshold markers", { optional: true, absent: true, default: undefined }),
    r("sizing", "sizing", "radio", "Gauge size", { items: [["auto", "Auto"], ["manual", "Manual"]], optional: true, absent: "auto", default: undefined, when: (o) => o.orientation === "horizontal" || o.orientation === "vertical", whenText: "`orientation` is `horizontal` or `vertical`" }),
    minSize("min-width", "min_width", "Min width", "vertical"),
    minSize("min-height", "min_height", "Min height", "horizontal"),
    r("neutral", "neutral", "number", "Neutral", { nullable: true, optional: true, default: undefined, hint: "Empty: the minimum is the neutral point." }),
    size("title-size", "title_size", "Title"),
    size("value-size", "value_size", "Value"),
    displayNameRow(),
  ],
};
