// The option table of the type `table` (10.3h, F43; 4.27, 4.18).
// The members under `fields.defaults` are the members of 4.18 that apply to each column. The form value of the footer is a list of
// calculations and the member is `{ reducers }` (`wrap` and `unwrap` of rows.js). The sort list has its builder in `forms/sort_list.js` (DOM code).
// The cell type is an old control (10.3a): the renderer places it.

import { checkBytes } from "../rules.js";
import { CALCS, displayNameRow, row } from "./rows.js";

export const MAX_SORT_KEYS = 8;
export const MAX_FOOTER = 8;
const ALIGN = [["auto", "Auto"], ["left", "Left"], ["center", "Center"], ["right", "Right"]];

// A sort key is { field, desc } (4.27). A new key sorts by the first field of the last result, or by "Value".
export const newSortKey = (hints = []) => ({ field: hints[0] || "Value", desc: false });

/** The error of the name of a sort key, or "". */
export const checkSortField = (text) => checkBytes(text, 1, 1024, "The field name");

export const FOOTER = Object.freeze({
  wrap: (list) => ({ reducers: [...list] }),
  unwrap: (member) => (member ? [...member.reducers] : []),
});

const T = (id, member, kind, label, rest) => row(id, member, kind, label, "table", rest);

export default {
  type: "table",
  groups: [["table", "Table"], ["footer", "Table footer"], ["cells", "Cell options"]],
  rows: [
    T("show-header", "options.show_header", "switch", "Show table header", { default: true }),
    T("sort", "options.sort", "list", "Sort by", { max: MAX_SORT_KEYS, default: [] }),
    T("col-width", "fields.defaults.width", "number", "Column width", { min: 10, max: 4000, integer: true, nullable: true, optional: true, default: undefined }),
    T("col-align", "fields.defaults.align", "radio", "Column alignment", { items: ALIGN, optional: true, default: undefined }),
    T("wrap", "options.wrap", "switch", "Wrap text", { optional: true, absent: false, default: undefined }),
    T("filterable", "fields.defaults.filterable", "switch", "Column filter", { optional: true, absent: false, default: undefined }),
    row("footer", "fields.defaults.footer", "checks", "Calculation", "footer", { items: CALCS, max: MAX_FOOTER, optional: true, default: undefined, wrap: FOOTER.wrap, unwrap: FOOTER.unwrap }),
    row("cell-type", "fields.defaults.cell", "custom", "Cell type", "cells", { optional: true, default: undefined, hook: "defaults-cell-type" }),
    row("inspect", "fields.defaults.inspect", "switch", "Cell value inspect", "cells", { optional: true, absent: false, default: undefined }),
    displayNameRow(),
  ],
};
