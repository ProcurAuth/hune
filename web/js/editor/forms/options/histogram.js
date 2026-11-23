// The option table of the type `histogram` (10.3h, F43; 4.33d, 9.19f). The page does not draw this type yet: the
// table is data, and the browser cases wait for the page. The hooks of the histogram are those of F31 (`hist-*`).

import { STACKING, displayNameRow, legendRows, row, tooltipRows } from "./rows.js";

const H = (id, hook, member, kind, label, rest = {}) => row(id, member, kind, label, "hist", { hook, ...rest });

/** The value of "Bucket size": a number above 0, or nothing (`null`: the page chooses the width, 9.19f). */
export function parseBucketSize(text) {
  const t = text.trim();
  if (t === "") return { value: null };
  const n = /^(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?$/.test(t) ? Number(t) : NaN;
  return Number.isFinite(n) && n > 0 ? { value: n } : { error: "Use a number above 0, or leave it empty." };
}

/** The value of "Bucket offset": any finite number. */
export function parseBucketOffset(text) {
  const t = text.trim();
  const n = /^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$/.test(t) ? Number(t) : NaN;
  return Number.isFinite(n) ? { value: n } : { error: "Use a number." };
}

export default {
  type: "histogram",
  groups: [["hist", "Histogram"], ["tooltip", "Tooltip"], ["legend", "Legend"]],
  rows: [
    H("bucket-count", "hist-count", "options.bucket_count", "number", "Bucket count", { min: 1, max: 1000, integer: true, default: 30, when: (o) => o.bucket_size === null, whenText: "`bucket_size` is `null`" }),
    H("bucket-size", "hist-size", "options.bucket_size", "number", "Bucket size", { min: 0, exclusiveMin: true, nullable: true, default: null, parse: parseBucketSize, hint: "Empty: the page chooses the width." }),
    H("bucket-offset", "hist-offset", "options.bucket_offset", "number", "Bucket offset", { default: 0, parse: parseBucketOffset }),
    H("combine", "hist-combine", "options.combine", "switch", "Combine series", { default: false }),
    H("stacking", "hist-stacking", "options.stacking", "radio", "Stacking", { items: STACKING, default: "none", when: (o) => o.combine === false, whenText: "`combine` is `false`" }),
    H("line-width", "hist-line-width", "options.line_width", "slider", "Line width", { min: 0, max: 10, default: 1 }),
    H("fill-opacity", "hist-fill-opacity", "options.fill_opacity", "slider", "Fill opacity", { min: 0, max: 100, default: 80 }),
    ...tooltipRows(),
    ...legendRows(),
    displayNameRow(),
  ],
};
