// The three rows of the reduction (10.3h, F43): Show, Calculation and Fields.
// A `stat`, a `bargauge` and a `piechart` use the member `options.reduce` (4.26), so the rows are written one time.
// The key of their group is `value` ("Value options"). This module has no DOM access.

import { CALCS, row } from "./rows.js";
import { checkPattern } from "../rules.js";

const SHOW = [[false, "Calculate"], [true, "All values"]];

/**
 * Read the text of "Fields" (4.26). An empty text gives `null`: each field of type number.
 * Another text MUST be a pattern of the Grafana form (4.24). The function returns { value } or { error }.
 * It does not change the text: the draft keeps what the person wrote.
 */
export function parseFields(text) {
  if (text === "") return { value: null };
  const error = checkPattern(text, { grafana: true });
  return error ? { error } : { value: text };
}

/**
 * The rows "Show", "Calculation" and "Fields".
 * The row "Calculation" shows only while `reduce.values` is `false`: `values` = `true` gives one item for each row, with no reduction (4.26).
 * The row "Fields" is a text field with a pattern, not a select of names (decision ID-K290c).
 */
export function reduceRows() {
  return [
    row("reduce-values", "options.reduce.values", "radio", "Show", "value", { items: SHOW, default: false }),
    row("reduce-calc", "options.reduce.calc", "select", "Calculation", "value", { items: CALCS, default: "last_not_null", when: (o) => o.reduce.values === false, whenText: "`reduce.values` is `false`" }),
    row("reduce-fields", "options.reduce.fields", "text", "Fields", "value", {
      nullable: true, pattern: true, parse: parseFields, default: null, hint: "Empty: each field of type number. Or a pattern such as /^Total$/.",
    }),
  ];
}
