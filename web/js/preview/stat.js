// The loading preview of the panel types stat (9.11).
// The rules of this family are in the council file `stat.md` (scratchpad/council). The loaded panel is drawn by web/js/panels/stat.js, items.js.
// The foundation draws the NEUTRAL form: the empty body with the sweep and the dots. The family implementation replaces `build` and follows the
// rules of its council file: the preview uses only the JSON document, never a query, and draws the least proven form where the document is silent.

import { neutral } from "./common.js";

// `panel` is the panel of the document, `box` is { w, h } (the content box of the body in px: the size of the loaded `.panel__plot`, as the
// council files define it), `doc` is the dashboard document and `ctx` is { rt, page }. It returns { elements, bounds, notes }.
export function build(panel, box, doc, ctx) {
  return neutral();
}
