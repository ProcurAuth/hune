// SPDX-License-Identifier: MIT OR Apache-2.0
// The differences that the visual parity capture accepts (15.14c). Each rule names the section of 19.20 that lists the difference.
// A difference that no rule matches is a fault. A rule is `{ id, section, text, match(ctx) }`.
//
// `ctx` is the record of one difference: `{ check, panel, detail, type, id, n, fixture, title, options, custom, lines, siblings, side, section, bars }`.
// `options` and `custom` are the panel options and `fieldConfig.defaults.custom` of the Grafana file. `lines` are the text lines of the
// panel in each product. `siblings` are the differences that the same panel already has, each with its class and rule. `section` is
// `{ inSection, below, copies }` for a dashboard with a repeated section. `bars` is `{ hune, grafana_no_x }` for a heatmap or a histogram: the
// marks of Hune and the rectangles of Grafana with no finite x.

const num = (t) => Number.parseFloat(String(t).replace(/[^\d.eE+-]/g, ""));
// The two lists hold the same numbers, each within a relative `tolerance` (Grafana writes 0.00781 for 0.0078125).
const sameNumbers = (a, b, tolerance = 0) => {
  if (a.length !== b.length) return false;
  const x = [...a].map(num).sort((p, q) => p - q);
  const y = [...b].map(num).sort((p, q) => p - q);
  return x.every((v, i) => Math.abs(v - y[i]) <= tolerance * Math.abs(y[i]));
};
const siblingRule = (ctx, id) => (ctx.siblings ?? []).some((s) => s.rule === `19.20g:${id}`);
const isHeat = (ctx) => ctx.type === "heatmap";
const isHist = (ctx) => ctx.type === "histogram";
const heatChecks = new Set(["marks", "colours", "plot", "canvas-size", "texts:x-axis", "texts:y-axis", "marks:empty-buckets", "state"]);
const hune = (ctx) => (ctx.lines?.hune ?? []).join("\n");

// The offset of the header of a copy of a repeated section (19.20b): 30 px less 27.25 px in Grafana 12.4.2.
const COPY_HEADER = 2.75;
// The copies with the header of 27.25 px above the box in Grafana: the copies 2 to n before it, and its own header for a copy of the section.
const shortHeaders = (c) => (c.section?.inSection ? c.n - 1 : c.section?.below ? c.section.copies - 1 : 0);
/** True when the options of a histogram have a `bucketOffset` that is not a multiple of `bucketSize` (19.20g). */
export const offsetCuts = (o) => o?.bucketSize > 0 && Number.isFinite(o.bucketOffset) && Math.abs(o.bucketOffset / o.bucketSize - Math.round(o.bucketOffset / o.bucketSize)) > 1e-9;

export const RULES = [
  // --- The repeat (19.20b) ---
  {
    id: "copy-header",
    section: "19.20b",
    text: "Grafana gives the copies 2 to n of a repeated section a header of 27.25 px (a title of 14 px, weight 400), and Hune gives each copy the header of 30 px of 9.8. A box of a copy, or of a panel below the section, is 2.75 px lower in Hune for each such copy above it.",
    match: (c) => {
      if (c.check !== "box" || !Array.isArray(c.detail?.shift)) return false;
      const k = shortHeaders(c);
      const [dx, , dw, dh] = c.detail.delta;
      return k >= 1 && dx <= 1 && dw <= 1 && dh <= 1 && Math.abs(c.detail.shift[1] - COPY_HEADER * k) <= 0.5;
    },
  },
  // --- Several queries (19.20f) ---
  {
    id: "name-prefix-on-failure",
    section: "19.20f",
    text: "Hune keeps the prefix `A a` when a query fails. Grafana drops the prefix of a time series when only one frame answers.",
    match: (c) => c.check === "names:legend" && c.detail.grafana.length > 0 && c.detail.grafana.length === c.detail.hune.length && c.detail.grafana.every((n, i) => /^[A-H] /.test(c.detail.hune[i]) && c.detail.hune[i].slice(2) === n),
  },
  // --- Heatmap (19.20g) ---
  {
    id: "unit-of-names",
    section: "19.20g",
    text: "A unit of the names of a frame that is already bucketed is not read.",
    match: (c) => isHeat(c) && c.check === "texts:y-axis" && c.detail.only_grafana.length > 0 && c.detail.only_grafana.every((t) => /^-?[\d.]+ [A-Za-z%]+$/.test(t)) && sameNumbers(c.detail.only_grafana, c.detail.only_hune),
  },
  {
    id: "unit-of-names-layout",
    section: "19.20g",
    text: "The wider labels of the unit move the plot (the same difference as the unit of the names).",
    match: (c) => isHeat(c) && ["plot", "marks", "canvas-size"].includes(c.check) && siblingRule(c, "unit-of-names"),
  },
  {
    id: "ybucketscale",
    section: "19.20g",
    text: "A `yBucketScale` of a frame that is already bucketed is not read.",
    match: (c) => isHeat(c) && heatChecks.has(c.check) && !!c.options?.rowsFrame?.yBucketScale,
  },
  {
    id: "log-label-form",
    section: "19.20g",
    text: "The labels of a log axis have the short form of the engine (`0.001`). Grafana writes `0.00100` and `0.100`.",
    match: (c) => isHeat(c) && c.check.startsWith("texts:") && c.detail.only_grafana.length > 0 && sameNumbers(c.detail.only_grafana, c.detail.only_hune, 0.01),
  },
  {
    id: "log-label-layout",
    section: "19.20g",
    text: "The short labels of a log axis move the plot (the same difference as the form of the labels).",
    match: (c) => isHeat(c) && ["plot", "marks", "colours"].includes(c.check) && siblingRule(c, "log-label-form"),
  },
  {
    id: "log-note",
    section: "19.20g",
    text: "Hune shows a note with the count of the values of 0 or less that a log scale skips. The note takes room under the plot.",
    match: (c) => isHeat(c) && ["plot", "marks", "colours", "canvas-size"].includes(c.check) && /of 0 or less (is|are) not on the log scale/.test(hune(c)),
  },
  {
    id: "axis-places",
    section: "19.20g",
    text: "The value axis has the places `left` and `hidden`. Hune has no right axis, no `axisWidth` and no `axisLabel` for the heatmap.",
    match: (c) => isHeat(c) && heatChecks.has(c.check) && (c.options?.yAxis?.axisPlacement === "right" || c.options?.yAxis?.axisWidth !== undefined || !!c.options?.yAxis?.axisLabel),
  },
  {
    id: "layout-ge-labels",
    section: "19.20g",
    text: "The labels of a frame that is already bucketed stay at the middle of the row. Grafana puts them at an edge with the layout `ge`.",
    match: (c) => isHeat(c) && c.check.startsWith("texts:") && c.options?.rowsFrame?.layout === "ge",
  },
  {
    id: "tiny-cells",
    section: "19.20g",
    text: "A cell is 0.5 px at least in Hune, and Grafana has 1 device pixel. A cell of 3 px or less has no gap in Hune.",
    match: (c) =>
      isHeat(c) &&
      ((c.check === "marks" && [...(c.detail.only_grafana ?? []), ...(c.detail.only_hune ?? [])].length > 0 && [...c.detail.only_grafana, ...c.detail.only_hune].every((m) => m.box[2] <= 3 && m.box[3] <= 3)) ||
        (c.check === "colours" && (c.detail.examples ?? []).length > 0 && c.detail.examples.every((e) => e.box[2] <= 3 && e.box[3] <= 3))),
  },
  {
    id: "query-limit",
    section: "19.20g",
    text: "A source of Hune has `max_result_rows` (100,000 in the reference profile). Grafana has no such limit and shows no data above about 100,000 rows with this data source.",
    match: (c) => (isHeat(c) || isHist(c)) && c.check === "state" && /query_limit|TOO_MANY_ROWS/.test(hune(c)),
  },
  {
    id: "one-time-row",
    section: "19.20g",
    text: "A frame with one time row gives an error in Hune. Grafana draws nothing.",
    match: (c) => isHeat(c) && c.check === "state" && /needs two time rows/.test(hune(c)),
  },
  // --- Histogram (19.20g) ---
  {
    id: "bucket-cap",
    section: "19.20g",
    text: "Hune has a cap of 4,000 buckets. Grafana has none.",
    match: (c) => isHist(c) && c.check === "state" && c.detail.hune === "bucket-limit",
  },
  {
    id: "empty-buckets",
    section: "19.20g",
    text: "The number of buckets is the number of the span, empty buckets included. Grafana draws only the buckets that have a count.",
    match: (c) => isHist(c) && c.check === "marks:empty-buckets",
  },
  {
    id: "palette",
    section: "19.20g",
    text: "The colours use the tokens and the shades of the theme (9.25): the series colours of the engine are not the palette of Grafana.",
    match: (c) => isHist(c) && c.check === "colours" && (c.detail.examples ?? []).length > 0 && c.detail.examples.every((e) => /, ?([\d.]+)\)$/.exec(e.grafana)?.[1] === /, ?([\d.]+)\)$/.exec(e.hune)?.[1]),
  },
  {
    id: "no-gradient-mode",
    section: "19.20g",
    text: "The histogram has no gradient mode.",
    match: (c) => isHist(c) && ["marks", "colours", "texts:y-axis", "texts:x-axis", "plot", "canvas-size"].includes(c.check) && !!c.custom?.gradientMode && c.custom.gradientMode !== "none",
  },
  {
    id: "one-bucket",
    section: "19.20g",
    text: "One bucket (all the values equal, one value, or a bucket count of 1): Grafana draws no bar (its canvas gets a rectangle with an x of NaN) and no x label, a fault of Grafana. Hune draws the bar and the labels of its two edges.",
    match: (c) =>
      isHist(c) &&
      (c.bars?.grafana_no_x ?? 0) > 0 &&
      ((c.check === "marks" && (c.detail.only_grafana ?? []).every((m) => !Number.isFinite(m.box[0]))) || (c.check === "texts:x-axis" && c.detail.only_grafana.length === 0)),
  },
  {
    id: "bucket-offset",
    section: "19.20g",
    text: "A bucket offset that is not a multiple of the bucket size: the x axis of Hune starts at the low edge of the first bucket and its labels are the bucket edges (9.19f). Grafana starts the axis later, cuts a part of the first bucket and puts its labels on no bar edge. The bars pair by their x value.",
    match: (c) => isHist(c) && ["texts:x-axis", "marks"].includes(c.check) && offsetCuts(c.options),
  },
  {
    id: "percent-axis",
    section: "19.20g",
    text: "With `stacking` equal to `percent`, the engine shows percent, and Grafana shows a fraction.",
    match: (c) => isHist(c) && c.custom?.stacking?.mode === "percent" && ["texts:y-axis", "plot"].includes(c.check),
  },
];

/** The class of one difference: `{ class: "documented", rule }` or `{ class: "fault", rule: null }`. */
export function classifyDifference(ctx) {
  for (const r of RULES) if (r.match(ctx)) return { class: "documented", rule: `${r.section}:${r.id}` };
  return { class: "fault", rule: null };
}
