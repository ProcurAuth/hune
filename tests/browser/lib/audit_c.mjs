// The closed list of the audit B153 for the types of group C: `table`, `heatmap` and `histogram` (10.3h rule 14, F43). Owner: the worker of group C.
// An entry: { pointer, types, cls, cause }. `pointer` is a member from the panel ("options.x.y" or "fields.defaults.z"), or a member and a value
// ("options.tooltip.mode=multi"). `cls` is "design" (the control cannot change the picture by design) or "data" (the data of the audit does not
// show the effect). `cause` has 20 characters or more. An entry covers its member and the members under it. A stale entry (the preview does change)
// and a dead entry (no control matches it) fail the audit. A new entry needs a decision row of the owner.
//
// The page draws `heatmap` and `histogram` since F31 (2026-11-15). The audit of that day found the entries written for them before F31 dead (no
// control of these types matches them): they are removed (the orchestrator, after the report of f43-cases).
export const EXEMPT = [
  { pointer: "fields.overrides", types: ["table"], cls: "design", cause: "a new override has one default property that no column of the data uses" },
  { pointer: "fields.defaults.thresholds", types: ["table"], cls: "design", cause: "the cell type or the colours of the type do not use the thresholds (4.27, 4.33c, 4.33d)" },
  { pointer: "fields.defaults.align=auto", types: ["table"], cls: "design", cause: "the value Auto is the picture of an absent member (4.18)" },
  { pointer: "fields.defaults.min", types: ["table"], cls: "design", cause: "no gauge cell or scale of the type uses the limit with this data (4.33c, rule 5)" },
  { pointer: "fields.defaults.max", types: ["table"], cls: "design", cause: "the same cause as the minimum" },
  { pointer: "fields.defaults.color", types: ["table"], cls: "design", cause: "a table colours a cell only with a cell type that uses the colour" },
];

// The place of the pointer for the hover probe, as fractions of the plot (x from the left, y from the top). A type with no entry has no tooltip.
export const HOVER = {
  histogram: { x: 0.5, y: 0.8 },
  heatmap: { x: 0.5, y: 0.5 },
};
