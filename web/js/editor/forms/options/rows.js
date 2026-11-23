// The pure helpers of the option forms (10.3h, F43): the row model, the write rule, the rules of display, the states of the audit and the
// shared rows of the legend, the tooltip, the style of the thresholds and the display name. No DOM access: Node tests and cases load it.
// A table of a type imports this file only. A row is a plain object (the frozen signature of group A):
//   { id, member, kind, label, group, items, min, max, step, integer, nullable, optional, absent, default,
//     when(options) -> boolean, whenText, writes: [member], also(value, options) -> { member: value }, build(ctx), hook }
// `member` is the path from the panel: "options.x.y" or "fields.defaults.z". `kind` is one of KINDS.
// Extra fields that a row can have: `parse(text, options) -> { value } | { error }` (a text or number row with its own grammar),
// `wrap(list)` and `unwrap(member)` (a check list that stores an object, as the footer does), `none` (the text of the item "not set" of a select),
// `hint`, `existing` (an old control that the renderer places), `nullText` (the text of the item `null` of a select of fields).

export const KINDS = Object.freeze(["radio", "select", "switch", "slider", "number", "text", "checks", "list", "field", "fields", "custom"]);

// The 18 calculations with the words of Grafana 12.4.2 (10.3h, "The calculations"). The value is the member of 4.26.
export const CALCS = Object.freeze([
  ["last_not_null", "Last *"], ["first_not_null", "First *"], ["last", "Last"], ["first", "First"], ["min", "Min"], ["max", "Max"],
  ["mean", "Mean"], ["total", "Total"], ["count", "Count"], ["median", "Median"], ["std_dev", "StdDev"], ["variance", "Variance"],
  ["range", "Range"], ["diff", "Difference"], ["diff_percent", "Difference percent"], ["delta", "Delta"], ["distinct_count", "Distinct Count"],
  ["change_count", "Change Count"],
].map(Object.freeze));

export const THRESHOLDS_STYLE = Object.freeze([
  ["off", "Off"], ["line", "As lines"], ["dashed", "As lines (dashed)"], ["area", "As filled regions"], ["line_area", "As filled regions and lines"],
  ["dashed_area", "As filled regions and lines (dashed)"],
].map(Object.freeze));
export const STACKING = Object.freeze([["none", "Off"], ["normal", "Normal"], ["percent", "100%"]].map(Object.freeze));
export const SHOW_VALUES = Object.freeze([["auto", "Auto"], ["always", "Always"], ["never", "Never"]].map(Object.freeze));
export const TOOLTIP_MODES = Object.freeze([["single", "Single"], ["multi", "All"], ["hidden", "Hidden"]].map(Object.freeze));
export const TOOLTIP_SORT = Object.freeze([["none", "None"], ["asc", "Ascending"], ["desc", "Descending"]].map(Object.freeze));
export const LEGEND_MODES = Object.freeze([["list", "List"], ["table", "Table"]].map(Object.freeze));
export const LEGEND_PLACEMENTS = Object.freeze([["bottom", "Bottom"], ["right", "Right"]].map(Object.freeze));

// The width rule of a choice (10.3h, rule 3): 7.4 px for each character and 24 px for each item, 280 px at most.
export const CHOICE_MAX_PX = 280;
export const choiceWidth = (items) => items.reduce((sum, [, text]) => sum + String(text).length * 7.4 + 24, 0);

/** A row. The hook is `opt-<id>` unless the row gives its own (an old control, or a type of F31 with `heat-*`). */
export function row(id, member, kind, label, group, rest = {}) {
  return { id, member, kind, label, group, hook: `opt-${id}`, ...rest };
}

// --- the paths ------------------------------------------------------------------------------------------------------------------

export const pathOf = (member) => member.split(".");

/** The value of `member` in `panel`, or `undefined`. */
export function getMember(panel, member) {
  let at = panel;
  for (const key of pathOf(member)) {
    if (at === null || typeof at !== "object") return undefined;
    at = at[key];
  }
  return at;
}

// The order of the members (4.38, rule 6): the order of the tables of 4.18 and 4.26 to 4.33d. A key that the list does not know goes last.
const LEGEND_ORDER = ["show", "placement", "mode", "values", "sort_by", "sort_desc"];
const TOOLTIP_ORDER = ["mode", "sort", "hide_zeros"];
const REDUCE_ORDER = ["calc", "fields", "values"];
const DEFAULTS_ORDER = [
  "unit", "decimals", "min", "max", "color", "thresholds", "mappings", "links", "display_name", "no_value", "width", "align", "cell", "filterable",
  "hidden", "footer", "inspect", "hide_from", "axis_placement", "axis_label", "soft_min", "soft_max", "axis_grid", "draw", "line_width", "fill_opacity", "line_dash",
  "show_points", "point_size", "transform", "line_dash_array", "axis_centered_zero", "axis_width", "axis_border", "axis_color",
];
export const OPTIONS_ORDER = Object.freeze({
  table: ["show_header", "sort", "wrap"],
  timeseries: ["draw", "interpolation", "line_width", "fill_opacity", "show_points", "point_size", "span_nulls", "stacking", "y_scale", "legend", "tooltip", "thresholds_style", "gradient_mode", "log_base", "span_nulls_ms", "bar_alignment"],
  stat: ["reduce", "color_mode", "graph_mode", "text_mode", "orientation", "justify", "percent_change", "percent_change_color", "title_size", "value_size"],
  barchart: ["orientation", "x_field", "stacking", "show_values", "bar_width", "group_width", "x_tick_rotation", "legend", "tooltip", "thresholds_style", "fill_opacity", "line_width", "gradient_mode"],
  piechart: ["pie_type", "reduce", "labels", "legend", "tooltip"],
  xychart: ["x_field", "y_fields", "draw", "point_size", "line_width", "legend", "tooltip"],
  bargauge: ["reduce", "orientation", "display_mode", "show_unfilled"],
  heatmap: ["calculate", "x_buckets", "y_buckets", "y_axis", "y_reverse", "y_min", "y_max", "color", "cell_gap", "hide_le", "hide_ge", "show_legend", "tooltip"],
  histogram: ["bucket_count", "bucket_size", "bucket_offset", "combine", "stacking", "line_width", "fill_opacity", "legend", "tooltip"],
  logs: ["show_time", "wrap", "details", "order"],
  geomap: ["view", "layer", "tooltip", "show_zoom", "mouse_wheel_zoom"],
  gauge: ["reduce", "orientation", "show_threshold_labels", "show_threshold_markers", "neutral", "title_size", "value_size", "sizing", "min_width", "min_height"],
});
const NESTED_ORDER = {
  legend: LEGEND_ORDER,
  tooltip: TOOLTIP_ORDER,
  reduce: REDUCE_ORDER,
  x_buckets: ["mode", "value"],
  y_buckets: ["mode", "value", "scale"],
  color: ["mode", "scheme", "fill", "scale", "exponent", "steps", "reverse", "min", "max"],
  // The two objects of a map (4.33g).
  view: ["mode", "lat", "lon", "zoom"],
  layer: ["lat_field", "lon_field", "size", "size_min", "size_max", "size_field", "color", "color_field", "opacity"],
};

function ordered(object, order) {
  const rank = (key) => {
    const at = order.indexOf(key);
    return at < 0 ? order.length : at;
  };
  const out = {};
  for (const key of Object.keys(object).sort((a, b) => rank(a) - rank(b))) out[key] = object[key];
  return out;
}

function orderNested(options, type) {
  const out = ordered(options, OPTIONS_ORDER[type] || []);
  for (const [key, order] of Object.entries(NESTED_ORDER)) {
    if (out[key] && typeof out[key] === "object" && !Array.isArray(out[key])) out[key] = ordered(out[key], order);
  }
  return out;
}

// --- the write rule (10.3h, rule 4) ----------------------------------------------------------------------------------------------

/** True when the member leaves the panel for this value: an optional member with its `absent` value, or `undefined`. */
export function leavesPanel(row, value) {
  if (value === undefined) return true;
  if (!row.optional) return false;
  if (row.nullable && value === null) return true;
  if (row.kind === "checks" && Array.isArray(value) && value.length === 0 && !("absent" in row)) return true;
  return "absent" in row && row.absent !== undefined && value === row.absent;
}

function putMember(draft, member, value) {
  const keys = pathOf(member);
  let at = draft;
  for (const key of keys.slice(0, -1)) {
    at[key] = at[key] && typeof at[key] === "object" && !Array.isArray(at[key]) ? { ...at[key] } : {};
    at = at[key];
  }
  const last = keys.at(-1);
  if (value === undefined) delete at[last];
  else at[last] = value;
}

/**
 * A new panel with `value` written to the member of `row`. Pure: `panel` is not changed. An optional member with its `absent` value leaves the
 * panel. A row can name more members that change with it in `also(value, options)` (a map of member to value, `undefined` removes). The members
 * keep the order of 4.38. A row of `fields.defaults` removes an empty `defaults` object never: the member `thresholds` stays (4.18).
 */
export function writeOption(panel, row, value) {
  const next = structuredClone(panel);
  const stored = leavesPanel(row, value) ? undefined : row.kind === "checks" && row.wrap ? row.wrap(value) : value;
  putMember(next, row.member, stored);
  const extra = row.also ? row.also(value, next.options) : null;
  if (extra) for (const [member, v] of Object.entries(extra)) putMember(next, member, v);
  if (next.options) next.options = orderNested(next.options, next.type);
  if (next.fields && next.fields.defaults) next.fields.defaults = ordered(next.fields.defaults, DEFAULTS_ORDER);
  return next;
}

/** The members that a write of `row` can change: its own and the `writes` list. */
export const writtenMembers = (row) => [row.member, ...(row.writes || [])];

// --- the value of a control ------------------------------------------------------------------------------------------------------

/** The value that the control of `row` shows for `panel`: the member, or the `absent` value, or `undefined`. */
export function valueOf(panel, row) {
  const raw = getMember(panel, row.member);
  if (raw === undefined) return "absent" in row ? row.absent : row.default;
  return row.kind === "checks" && row.unwrap ? row.unwrap(raw) : raw;
}

/** True when `value` is one of the values of the items of a row. */
export const hasItem = (row, value) => (row.items || []).some(([v]) => v === value);

/** The error under a choice or a select for a value that the row does not know (10.3h, rule 5), or "". */
export const unknownValueError = (row, value) => (value === undefined || value === null && row.nullable || hasItem(row, value) ? "" : `The value ${JSON.stringify(value)} is not valid. Choose a value.`);

const numberText = (n) => (n === null || n === undefined ? "" : String(n));
export { numberText };

/**
 * Reads the text of a number or text row. It returns { value } for a valid text, or { error }. An empty text gives `null` for a nullable row and
 * an error else. A text row with a `parse` function uses it. Nothing here changes a value to make it fit (10.3h, rule 4).
 */
export function parseText(row, text, options = {}) {
  if (row.parse) return row.parse(text, options);
  const t = text.trim();
  if (row.kind === "text") {
    if (t === "" && row.nullable) return { value: null };
    const n = [...text].length;
    const min = row.min ?? 0;
    const max = row.max ?? 4096;
    if (n < min || n > max) return { error: `The text needs ${min} to ${max} characters.` };
    return { value: text };
  }
  if (t === "") return row.nullable ? { value: null } : { error: `${row.label} needs a number.` };
  if (!/^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$/.test(t)) return { error: "Use a number." };
  const n = Number(t);
  if (!Number.isFinite(n)) return { error: "Use a number." };
  if (row.integer && !Number.isInteger(n)) return { error: "Use a whole number." };
  const range = rangeText(row);
  if (row.min !== undefined && (row.exclusiveMin ? n <= row.min : n < row.min)) return { error: `Use a number ${range}.` };
  if (row.max !== undefined && n > row.max) return { error: `Use a number ${range}.` };
  return { value: n };
}

/** The words of a range: "from 1 to 10", "above 0". */
export function rangeText(row) {
  if (row.exclusiveMin && row.max === undefined) return `above ${row.min}`;
  if (row.min !== undefined && row.max !== undefined) return `from ${row.min} to ${row.max}`;
  if (row.min !== undefined) return `of ${row.min} or more`;
  if (row.max !== undefined) return `of ${row.max} or less`;
  return "";
}

/** The error of a value for a row of the kind slider, or "". A slider holds an integer in its range. */
export function checkSlider(row, n) {
  if (!Number.isInteger(n) || n < row.min || n > row.max) return `Use a whole number from ${row.min} to ${row.max}.`;
  return "";
}

/** The error of a list of values for a check list (the limit `max` and no duplicate), or "". */
export function checkChecks(row, list) {
  if (list.length > (row.max ?? Infinity)) return `Choose ${row.max} at most.`;
  if (new Set(list).size !== list.length) return "Choose each item once.";
  return "";
}

// --- the rules of display (10.3h, rule 8) -----------------------------------------------------------------------------------------

/** The rows that the rules of display show for `options`. A row with no `when` always shows. */
export const shownRows = (table, options) => table.rows.filter((r) => !r.when || r.when(options));

/** The ids of the shown rows. */
export const shownIds = (table, options) => shownRows(table, options).map((r) => r.id);

/**
 * The words of the status line (10.3h, rule 9) for a change that shows and hides rows. `before` and `after` are lists of row ids. The line is
 * empty when no row changes. A list of more than 3 labels gives the count.
 */
export function statusText(table, before, after) {
  const label = (id) => (table.rows.find((r) => r.id === id) || { label: id }).label;
  const shown = after.filter((id) => !before.includes(id));
  const hidden = before.filter((id) => !after.includes(id));
  const part = (word, ids) => (ids.length === 0 ? "" : ids.length > 3 ? `${word}: ${ids.length} options.` : `${word}: ${ids.map(label).join(", ")}.`);
  return [part("Shown", shown), part("Hidden", hidden)].filter(Boolean).join(" ");
}

// --- the states of the audit (10.3h, rule 14) ----------------------------------------------------------------------------------------

function setPath(object, member, value) {
  const keys = pathOf(member).slice(1);
  let at = object;
  for (const key of keys.slice(0, -1)) at = at[key] ??= {};
  if (value === undefined) delete at[keys.at(-1)];
  else at[keys.at(-1)] = value;
}

// The one-change moves of a state: each other item of a choice or select, both values of a switch, and the name "name" for `legend.sort_by`.
function moves(table) {
  const out = [];
  for (const r of table.rows) {
    if (!r.member.startsWith("options.")) continue;
    let values = [];
    if ((r.kind === "radio" || r.kind === "select") && r.items) values = r.items.map(([v]) => v);
    else if (r.kind === "switch") values = [true, false];
    for (const v of values) out.push([r.member, v]);
  }
  return out;
}

/**
 * The states of the audit for one table: the options of a new panel, then one state for each row that no earlier state shows. A state is made with
 * one change of a choice, a switch or a select, or with two changes when one is not enough (10.3h, rule 14).
 * It returns [{ name, options, changes, shown }]. Pure: it reads `newOptions` and the tables.
 */
export function statesOf(table, newOptions) {
  const base = structuredClone(newOptions);
  const all = moves(table);
  const apply = (options, changes) => {
    const copy = structuredClone(options);
    for (const [member, value] of changes) setPath(copy, member, value);
    return copy;
  };
  const states = [{ name: "new panel", options: base, changes: [], shown: shownIds(table, base) }];
  const covered = new Set(states[0].shown);
  for (const r of table.rows) {
    if (covered.has(r.id)) continue;
    let found = null;
    for (const c of all) {
      const options = apply(base, [c]);
      if (shownIds(table, options).includes(r.id)) {
        found = { changes: [c], options };
        break;
      }
    }
    if (!found) {
      outer: for (const c of all) {
        for (const d of all) {
          const options = apply(base, [c, d]);
          if (shownIds(table, options).includes(r.id)) {
            found = { changes: [c, d], options };
            break outer;
          }
        }
      }
    }
    if (!found) continue;
    const shown = shownIds(table, found.options);
    states.push({ name: found.changes.map(([m, v]) => `${m}=${v}`).join(" + "), options: found.options, changes: found.changes, shown });
    for (const id of shown) covered.add(id);
  }
  // The legend as a table with a sort is a state of its own (rule 14), so `sort_desc` has a state.
  const sortDesc = table.rows.find((r) => r.id === "legend-sort-desc");
  if (sortDesc && !states.some((s) => s.shown.includes("legend-sort-desc"))) {
    const changes = [["options.legend.mode", "table"], ["options.legend.sort_by", "name"]];
    const options = apply(base, changes);
    states.push({ name: changes.map(([m, v]) => `${m}=${v}`).join(" + "), options, changes, shown: shownIds(table, options) });
  }
  return states;
}

/** The ids of the rows of `table` that no state shows. A test uses it: a rule that is never true is a mistake. */
export function rowsWithoutState(table, newOptions) {
  const seen = new Set(statesOf(table, newOptions).flatMap((s) => s.shown));
  return table.rows.filter((r) => !seen.has(r.id)).map((r) => r.id);
}

// --- the shared rows --------------------------------------------------------------------------------------------------------------

const isMulti = (o) => o.tooltip.mode === "multi";

/** The rows of the group "Tooltip". `modes` is the list of the choice, `sort` and `hideZeros` turn the two optional rows on and off. */
export function tooltipRows({ modes = TOOLTIP_MODES, sort = true, hideZeros = true } = {}) {
  return [
    row("tooltip-mode", "options.tooltip.mode", "radio", "Tooltip mode", "tooltip", { items: modes, default: "single" }),
    ...(sort ? [row("tooltip-sort", "options.tooltip.sort", "radio", "Values sort order", "tooltip", { items: TOOLTIP_SORT, optional: true, absent: "none", default: undefined, when: isMulti, whenText: "`tooltip.mode` is `multi`" })] : []),
    ...(hideZeros ? [row("tooltip-hide-zeros", "options.tooltip.hide_zeros", "switch", "Hide zeros", "tooltip", { optional: true, absent: false, default: undefined, when: isMulti, whenText: "`tooltip.mode` is `multi`" })] : []),
  ];
}

const legendShown = (o) => o.legend.show === true;
const legendTable = (o) => o.legend.show === true && o.legend.mode === "table";

/** The rows of the group "Legend". `pie`: the values and the sort of a pie chart. `placement`: the default of a new panel. */
export function legendRows({ pie = false, placement = "bottom" } = {}) {
  return [
    row("legend-show", "options.legend.show", "switch", "Visibility", "legend", { default: true }),
    row("legend-mode", "options.legend.mode", "radio", "Mode", "legend", { items: LEGEND_MODES, default: "list", when: legendShown, whenText: "`legend.show` is `true`" }),
    row("legend-placement", "options.legend.placement", "radio", "Placement", "legend", { items: LEGEND_PLACEMENTS, default: placement, when: legendShown, whenText: "`legend.show` is `true`" }),
    pie
      ? row("legend-values", "options.legend.values", "checks", "Legend values", "legend", { items: [["value", "Value"], ["percent", "Percent"]], max: 2, default: [], when: legendTable, whenText: "`legend.mode` is `table`" })
      : row("legend-values", "options.legend.values", "checks", "Values", "legend", { items: CALCS, max: 5, default: [], when: legendTable, whenText: "`legend.mode` is `table`" }),
    row("legend-sort-by", "options.legend.sort_by", "select", "Sort by", "legend", {
      items: pie ? [["name", "Name"], ["value", "Value"], ["percent", "Percent"]] : [["name", "Name"], ...CALCS],
      none: "None",
      optional: true,
      absent: undefined,
      default: undefined,
      when: legendTable,
      whenText: "`legend.mode` is `table`",
      writes: ["options.legend.sort_desc"],
      // The choice "None" removes `sort_by` and `sort_desc` (10.3h, rule 4).
      also: (value) => (value === undefined ? { "options.legend.sort_desc": undefined } : null),
    }),
    row("legend-sort-desc", "options.legend.sort_desc", "switch", "Sort descending", "legend", { optional: true, absent: false, default: undefined, when: (o) => legendTable(o) && o.legend.sort_by !== undefined, whenText: "`legend.mode` is `table` and `legend.sort_by` is set" }),
  ];
}

/** The row "Show thresholds" of the group "Thresholds" (`timeseries` and `barchart`). */
export const thresholdsStyleRow = () => row("thresholds-style", "options.thresholds_style", "select", "Show thresholds", "thresholds", { items: THRESHOLDS_STYLE, optional: true, absent: "off", default: undefined });

/** The row "Display name" of the group "Standard options" (each type). Empty removes the member. */
export const displayNameRow = () => row("display-name", "fields.defaults.display_name", "text", "Display name", "standard", { min: 1, max: 256, nullable: true, optional: true, default: undefined, hint: "Empty: the name of the field." });

/** A row for a control that already exists (10.3a): the renderer places it with its own hook and does not build it again. */
export const oldRow = (id, member, label, group, hook, rest = {}) => row(id, member, "custom", label, group, { existing: true, optional: true, default: undefined, hook, ...rest });
