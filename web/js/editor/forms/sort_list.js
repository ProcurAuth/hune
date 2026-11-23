// The builder of the sort list of a table (10.3h, F43; 4.27): 0 to 8 keys, each with a field name and the switch "Descending".
// DOM code, so it is outside `options/`: the row `sort` of `options/table.js` has the data, and the renderer calls this builder for that row.

import { el } from "../../ui.js";
import { listEditor, switchField, textField } from "./controls.js";
import { MAX_SORT_KEYS, checkSortField, newSortKey } from "./options/table.js";

/**
 * The control of the row `sort`. `ctx` is the context of the renderer: `scope` (the debounce scope), `row`, `hints()` (the field names of the last
 * result), `get()` (the array now) and `set(array)` (the write rule of 10.3h). It returns { node }. The hooks are `opt-sort`, `opt-sort-add`,
 * `opt-sort-<i>`, `opt-sort-<i>-field`, `opt-sort-<i>-desc`, `opt-sort-<i>-up`, `opt-sort-<i>-down` and `opt-sort-<i>-remove`.
 */
export function buildSortList(ctx) {
  const { scope, get, set, hints, row } = ctx;
  const list = listEditor(scope, {
    testid: "opt-sort",
    itemTestid: (i) => `opt-sort-${i}`,
    get: () => get() || [],
    set,
    limit: MAX_SORT_KEYS,
    limitWhat: "sort keys",
    emptyText: "No sort. The rows keep the order of the data.",
    title: (key, i) => `${i + 1}. ${key.field}`,
    add: { label: "Add sort", testid: "opt-sort-add", make: () => newSortKey(hints()) },
    renderItem(key, i, item) {
      const names = el("datalist", { id: `${item.testid}-names` }, ...hints().map((n) => el("option", { value: n })));
      const name = textField(scope, {
        label: "Field name",
        value: key.field,
        list: `${item.testid}-names`,
        check: checkSortField,
        apply: (text) => item.update({ field: text }),
        testid: `${item.testid}-field`,
        hint: "A display name of the last result.",
      });
      const desc = switchField({ label: "Descending", checked: key.desc, testid: `${item.testid}-desc`, apply: (on) => item.update({ desc: on }) });
      return [name.node, names, desc.node];
    },
  });
  return { node: el("fieldset", { class: "field" }, el("legend", { class: "field__label", text: row.label }), list) };
}
