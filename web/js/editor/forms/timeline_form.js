// The controls of the options of a state timeline and a status history (4.33b, 9.19c, 10.3): one control for each option. A control writes
// the draft through `set(patch)` for a valid value only. The legend has no mode and no values, and the card has no sort (4.33b, rules 2 and 3).

import { el } from "../../ui.js";
import { segmentedField } from "../form.js";
import { sliderField, switchField } from "./controls.js";

const SHOW = [["auto", "Auto"], ["always", "Always"], ["never", "Never"]];
const ALIGN = [["left", "Left"], ["center", "Center"], ["right", "Right"]];
const PLACE = [["bottom", "Bottom"], ["right", "Right"]];
const TIP = [["single", "Single"], ["multi", "All"], ["hidden", "Hidden"]];

// A number field with a slider (an integer from `min` to `max`). The slider writes at each move, at most once in each frame, and on release when the
// panel has more than 5,000 points (10.3h, rule 7). The field writes 300 ms after the last key.
// The root of one control: `data-option` has the member that it writes (9.29, 10.3h rule 12).
const own = (member, node) => el("div", { class: "orow", dataset: { option: `options.${member}` } }, node);

function slider(scope, { label, value, min, max, set, testid, heavy }) {
  const f = sliderField(scope, { label, value, min, max, apply: set, testid, rangeTestid: null, heavy, hint: `${min} to ${max}` });
  return el("div", { class: "timeline-slider" }, f.node);
}

function choice(label, items, value, testid, set) {
  const f = segmentedField(label, items.map(([v, text]) => ({ value: v, label: text })), value, set);
  f.seg.el.dataset.testid = testid;
  return f.node;
}

/**
 * The nodes of the three groups of the options pane: `{ main, legend, tooltip }`. `type` is the panel type, `get()` gives the options now,
 * `set(patch)` merges members into them.
 */
export function timelineControls(scope, { type, get, set, heavy = () => false }) {
  const o = get();
  const history = type === "status_history";
  const main = [];
  if (!history) main.push(own("merge_values", switchField({ label: "Merge equal values", checked: o.merge_values, testid: "timeline-merge", apply: (v) => set({ merge_values: v }) }).node));
  main.push(own("show_values", choice("Show values", SHOW, o.show_values, "timeline-show-values", (v) => set({ show_values: v }))));
  if (!history) main.push(own("align_values", choice("Align values", ALIGN, o.align_values, "timeline-align", (v) => set({ align_values: v }))));
  main.push(own("row_height", slider(scope, { label: "Row height", value: o.row_height, min: 10, max: 100, testid: "timeline-row-height", heavy, set: (n) => set({ row_height: n }) })));
  if (history) main.push(own("col_width", slider(scope, { label: "Column width", value: o.col_width, min: 10, max: 100, testid: "timeline-col-width", heavy, set: (n) => set({ col_width: n }) })));
  main.push(own("line_width", slider(scope, { label: "Line width", value: o.line_width, min: 0, max: 10, testid: "timeline-line-width", heavy, set: (n) => set({ line_width: n }) })));
  main.push(own("fill_opacity", slider(scope, { label: "Fill opacity", value: o.fill_opacity, min: 0, max: 100, testid: "timeline-fill-opacity", heavy, set: (n) => set({ fill_opacity: n }) })));
  if (!history) main.push(own("span_nulls", switchField({ label: "Connect over missing values", checked: o.span_nulls, testid: "timeline-span-nulls", apply: (v) => set({ span_nulls: v }) }).node));
  main.push(own("show_labels", switchField({ label: "Show row names", checked: o.show_labels, testid: "timeline-labels", apply: (v) => set({ show_labels: v }) }).node));
  const legend = o.legend;
  const putLegend = (patch) => set({ legend: { ...get().legend, ...patch } });
  return {
    main,
    legend: [
      own("legend.show", switchField({ label: "Show legend", checked: legend.show, testid: "timeline-legend-show", apply: (v) => putLegend({ show: v }) }).node),
      own("legend.placement", choice("Placement", PLACE, legend.placement, "timeline-legend-placement", (v) => putLegend({ placement: v }))),
    ],
    tooltip: [own("tooltip.mode", choice("Tooltip mode", TIP, o.tooltip.mode, "timeline-tooltip-mode", (v) => set({ tooltip: { ...get().tooltip, mode: v } })))],
  };
}

/** The note under the colour mode of these types. */
export const colourNote = () => el("p", { class: "field__hint", dataset: { testid: "timeline-colour-note" }, text: "A value mapping gives the colour of a state. The colour mode gives the colour of the other values." });
