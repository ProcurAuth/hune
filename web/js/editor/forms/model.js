// The data rules of the forms (10.3a, 10.3b): limits, new items, list changes, the order of the members. Pure module: no DOM access.
// Every function returns new objects and never changes its input. A new item is valid for the format (section 4).

import { framesToFields } from "../../frame.js";
import { baseDisplayNames } from "../../names.js";
import { variableNamesOfLines } from "../../sqlscan.js";
import { MORE_KIND_TEXT, isMoreKind, newMoreTransform } from "./transform_more_model.js";

// The limits of 4.35 that a form shows.
export const LIMITS = Object.freeze({
  overrides: 512,
  mappings: 256,
  links: 16,
  transforms: 16,
  filters: 32,
  names: 64,
  reducers: 8,
  dashboardLinks: 16,
  linkTags: 8,
  variables: 64,
  annotations: 8,
  options: 10000,
  defaults: 10000,
  tags: 32,
});

export const MAPPING_KINDS = Object.freeze([["value", "Value"], ["range", "Range"], ["regex", "Regex"], ["special", "Special"]]);
export const SPECIAL_MATCHES = Object.freeze([["null", "Null"], ["nan", "NaN"], ["null_nan", "Null and NaN"], ["true", "True"], ["false", "False"], ["empty", "Empty"]]);
export const MATCHER_KINDS = Object.freeze([
  ["equal", "Is equal to"], ["not_equal", "Is not equal to"], ["greater", "Is greater than"], ["greater_or_equal", "Is greater or equal"],
  ["lower", "Is lower than"], ["lower_or_equal", "Is lower or equal"], ["range", "Is between"], ["regex", "Matches regex"],
  ["is_null", "Is null"], ["is_not_null", "Is not null"],
]);
// The two kinds of format 1 and the eleven kinds of 4.25a (F28).
export const TRANSFORM_KINDS = Object.freeze([["filter_by_value", "Filter data by values"], ["grouping_to_matrix", "Grouping to matrix"], ...MORE_KIND_TEXT]);
export const EMPTY_VALUES = Object.freeze([["empty", "Empty"], ["null", "Null"], ["true", "True"], ["false", "False"]]);
export const OVERRIDE_MATCHERS = Object.freeze([["name", "Fields with name"], ["regex", "Fields with regex"], ["names", "Fields with names"]]);
export const VARIABLE_KINDS = Object.freeze([["query", "Query"], ["custom", "Custom"], ["text", "Text box"], ["interval", "Interval"], ["constant", "Constant"], ["adhoc", "Ad hoc filters"]]);

/** The 30 texts of the table of 7.4a: the values of `auto_min` (4.10a). */
export const AUTO_MIN_TEXTS = Object.freeze(["1ms", "10ms", "20ms", "50ms", "100ms", "200ms", "500ms", "1s", "2s", "5s", "10s", "15s", "20s", "30s", "1m", "2m", "5m", "10m", "15m", "20m", "30m", "1h", "2h", "3h", "6h", "12h", "1d", "7d", "30d", "1y"]);

/** The count of items that an `interval` variable has at most (4.35, `MAX_INTERVALS`). */
export const MAX_INTERVALS = 50;

const UNIT_MS = Object.freeze({ ms: 1, s: 1000, m: 60000, h: 3600000, d: 86400000, w: 604800000, y: 31536000000 });

/** The error of one interval text (4.10a): its text, or "" when the item is valid. The bound is 365 days. */
export function intervalError(text) {
  const m = /^[1-9][0-9]{0,8}(ms|s|m|h|d|w|y)$/.exec(text);
  if (!m) return `"${text.slice(0, 20)}" is not an interval. Use digits and one unit: ms, s, m, h, d, w or y.`;
  return parseInt(text, 10) * UNIT_MS[m[1]] > UNIT_MS.y ? `"${text}" is more than 365 days.` : "";
}

/** The items of the text area "Intervals": the text is split at the commas and at the line ends, and each item loses its outer spaces (10.3b). */
export const splitIntervals = (text) => text.split(/[,\r\n]+/).map((x) => x.trim()).filter((x) => x !== "");

/** The error of the whole text area "Intervals" (4.10a): a count of 1 to 50, valid items, no repeated item. */
export function intervalsError(items) {
  if (items.length < 1 || items.length > MAX_INTERVALS) return `Use 1 to ${MAX_INTERVALS} intervals.`;
  const bad = items.map(intervalError).find((e) => e !== "");
  if (bad) return bad;
  const repeated = items.find((x, i) => items.indexOf(x) !== i);
  return repeated === undefined ? "" : `"${repeated}" is in the list twice.`;
}
export const SORTS = Object.freeze([
  ["none", "Disabled"], ["alpha_asc", "Alphabetical (asc)"], ["alpha_desc", "Alphabetical (desc)"], ["num_asc", "Numerical (asc)"], ["num_desc", "Numerical (desc)"],
  ["alpha_ci_asc", "Alphabetical, case-insensitive (asc)"], ["alpha_ci_desc", "Alphabetical, case-insensitive (desc)"], ["natural_asc", "Natural (asc)"], ["natural_desc", "Natural (desc)"],
]);

// --- lists ---------------------------------------------------------------------------------------------------------------------

/** A copy of `items` with the item `from` moved by `dir` (-1 or 1). Null when it cannot move. */
export function moveItem(items, from, dir) {
  const to = from + dir;
  if (from < 0 || from >= items.length || to < 0 || to >= items.length) return null;
  const next = items.slice();
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

export const removeItem = (items, at) => items.filter((_, i) => i !== at);
export const replaceItem = (items, at, item) => items.map((x, i) => (i === at ? item : x));

/** True when the list is at its limit. */
export const atLimit = (items, limit) => items.length >= limit;

/** The tooltip text of an "Add" button that is at its limit (4.35). */
export const limitText = (limit, what) => `A limit of ${limit} ${what} applies.`;

// --- new items -----------------------------------------------------------------------------------------------------------------

/** A new override: it matches one field by name, and it sets one property (4.17: `set` has one member or more). */
export function newOverride(fieldName = "Value") {
  return { match: { by: "name", value: fieldName }, set: { unit: "short" } };
}

/** The matcher of an override after a change of its kind. It keeps the text when the new kind takes one. */
export function matcherOf(by, previous) {
  const text = previous.by === "names" ? (previous.names[0] || "Value") : previous.value;
  if (by === "names") return { by, names: [text || "Value"], mode: "include" };
  return { by, value: text || "Value" };
}

export function newMapping(kind) {
  switch (kind) {
    case "range": return { kind, from: 0, to: 100, text: "Text", color: null };
    case "regex": return { kind, pattern: ".+", text: "Text", color: null };
    case "special": return { kind, match: "null", text: "No data", color: null };
    default: return { kind: "value", value: "1", text: "Text", color: null };
  }
}

/** A mapping after a change of its kind: the text and the colour stay. */
export function mappingOfKind(kind, previous) {
  return { ...newMapping(kind), text: previous.text, color: previous.color };
}

// A new link is valid and complete: a person edits the title and the URL, and no empty item goes into the document.
export const newLink = () => ({ title: "Link", url: "https://example.org/", target_blank: false });

/** The matcher of a filter after a change of its kind. */
export function newFilterMatcher(kind, previous = null) {
  const num = previous && typeof previous.value === "number" ? previous.value : 0;
  switch (kind) {
    case "equal":
    case "not_equal": return { kind, value: previous && previous.value !== undefined ? previous.value : 0 };
    case "greater":
    case "greater_or_equal":
    case "lower":
    case "lower_or_equal": return { kind, value: num };
    case "range": return { kind, from: 0, to: 100 };
    case "regex": return { kind, pattern: ".+" };
    default: return { kind };
  }
}

export const newFilter = (field = "Value") => ({ field, matcher: { kind: "greater", value: 0 } });

/** A new transform of `kind`. `field` is the first field of the last data (10.3a): the kinds of 4.25a use it. */
export function newTransform(kind, field = "Value") {
  if (isMoreKind(kind)) return newMoreTransform(kind, field);
  if (kind === "grouping_to_matrix") return { kind, column_field: "Column", row_field: "Row", value_field: "Value", empty_value: "empty" };
  return { kind: "filter_by_value", mode: "include", match: "all", filters: [newFilter()] };
}

export function newDashboardLink(kind) {
  if (kind === "dashboards") return { kind, title: "", tags: [], as_dropdown: false, target_blank: false, include_vars: false, keep_time: false };
  return { kind: "link", title: "Link", url: "https://", target_blank: false, include_vars: false, keep_time: false };
}

/** A name that no other variable has: `var1`, `var2`, ... */
export function uniqueVariableName(variables, base = "var") {
  const used = new Set(variables.map((v) => v.name));
  for (let i = 1; ; i++) if (!used.has(`${base}${i}`)) return `${base}${i}`;
}

/** A new variable with the members of its kind in the canonical order (4.7 to 4.10). */
export function newVariable(kind, name, source = "") {
  const head = { kind, name, label: null, hide: "none" };
  if (kind === "custom") return { ...head, options: [{ text: "One", value: "1" }], multi: false, include_all: false, all_value: null, default: [] };
  if (kind === "text") return { ...head, value_type: "text", default: "" };
  if (kind === "interval") return { ...head, intervals: ["1m", "10m", "1h", "1d"], auto: false, auto_count: 30, auto_min: "10s", default: null };
  if (kind === "constant") return { ...head, hide: "variable", value: "" };
  // The keys are the plain column names of the configured list adhoc_columns of the source (4.10c): the person adds them.
  if (kind === "adhoc") return { ...head, source, keys: [] };
  return { ...head, kind: "query", source, sql: ["SELECT 1"], multi: false, include_all: false, all_value: null, refresh: "load", sort: "none", regex: null, default: [] };
}

/**
 * A copy of the variable `v` with `allow_custom_value` set (4.8, 4.9, 4.38). The member follows `default` and comes before `placement`.
 * `false` leaves the member out: a closed variable has no such member. A `text` variable never gets it.
 */
export function setAllowCustom(v, on) {
  const out = {};
  for (const [key, value] of Object.entries(v)) {
    if (key === "allow_custom_value" || key === "placement") continue;
    out[key] = value;
    if (key === "default" && on && v.kind !== "text") out.allow_custom_value = true;
  }
  if (v.placement) out.placement = v.placement;
  return out;
}

// --- annotations (10.3b, 4.5b) -------------------------------------------------------------------------------------------------

export const ANNOTATION_ADD_LIMIT = "A dashboard has 8 annotations at most.";

/** A new annotation: the colour `red`, `enabled`, all the time series (10.3b). `sql` gives a valid query that the person replaces. */
export const newAnnotation = (name, source) => ({ name, source, sql: ["SELECT now() AS time, 'Event' AS title"], color: "red", enabled: true });

/** A name that no other annotation has: `Annotation 1`, `Annotation 2`, ... */
export function uniqueAnnotationName(annotations) {
  const used = new Set(annotations.map((a) => a.name));
  for (let i = 1; ; i++) if (!used.has(`Annotation ${i}`)) return `Annotation ${i}`;
}

/** A copy of `doc` with the array `annotations` after `variables` (4.38). An empty array leaves the member out. */
export function withAnnotations(doc, annotations) {
  const out = {};
  for (const [key, value] of Object.entries(doc)) {
    if (key === "annotations") continue;
    out[key] = value;
    if (key === "variables" && annotations.length) out.annotations = annotations;
  }
  if (annotations.length && !("variables" in doc)) out.annotations = annotations;
  return out;
}

/** The annotations whose SQL names the variable `name` (the scan of 9.10): the names. A rename does not change the SQL (10.3b). */
export const staleAnnotations = (doc, name) => (doc.annotations || []).filter((a) => variableNamesOfLines(a.sql).includes(name)).map((a) => a.name);

const INTERVAL_MACRO = /\$\{?__(?:timeInterval(?:_ms)?|interval_(?:s|ms))\b/;

/**
 * The faults of the annotation `i` that the page can see before the save (4.5b, 4.34, 7.18): [{ pointer, code, message }].
 * The server checks the document again at the save. `sources` is the list of `GET api/sources`.
 */
export function annotationFaults(doc, i, sources) {
  const a = doc.annotations[i];
  const at = `/annotations/${i}`;
  const out = [];
  if (doc.annotations.some((b, k) => k !== i && b.name === a.name)) out.push({ pointer: `${at}/name`, code: "not_unique", message: "This name exists." });
  if (!sources.some((x) => x.id === a.source)) out.push({ pointer: `${at}/source`, code: "bad_reference", message: `The source ${a.source} is not available to you.` });
  a.sql.forEach((line, n) => {
    if (INTERVAL_MACRO.test(line)) out.push({ pointer: `${at}/sql/${n}`, code: "not_permitted", message: "An annotation has no interval: this macro is not permitted." });
  });
  if (a.panels) {
    const ids = new Set(allPanelIds(doc));
    a.panels.forEach((id, k) => {
      if (!ids.has(id)) out.push({ pointer: `${at}/panels/${k}`, code: "bad_reference", message: `The panel ${id} is not in the dashboard.` });
    });
  }
  return out;
}

const allPanelIds = (doc) => doc.sections.flatMap((s) => s.panels.map((p) => p.id));

// --- members in the canonical order -----------------------------------------------------------------------------------------

const QUERY_ORDER = ["source", "format", "sql", "min_interval", "max_data_points", "time_from", "time_shift", "hide_time_override", "ref", "hide"];

/** A copy of `data.query` with the member `key` set (a value) or removed (`undefined`), in the canonical order of 4.15. */
export function setQueryMember(query, key, value) {
  const next = { ...query };
  if (value === undefined) delete next[key];
  else next[key] = value;
  const out = {};
  for (const k of QUERY_ORDER) if (Object.hasOwn(next, k)) out[k] = next[k];
  for (const k of Object.keys(next)) if (!Object.hasOwn(out, k)) out[k] = next[k];
  return out;
}

/** The names of the fields of a frame, for the lists of suggestions (10.3a): the column names and the base display names, with no repeat. */
export function fieldNameHints(frame) {
  if (!frame || !Array.isArray(frame.columns)) return [];
  const names = frame.columns.map((c) => c.name);
  try {
    names.push(...baseDisplayNames(framesToFields(frame)));
  } catch (error) {
    // A frame that the page cannot read gives the column names only.
  }
  return [...new Set(names.filter((n) => typeof n === "string" && n !== ""))];
}

// --- variables ---------------------------------------------------------------------------------------------------------------

/** The panels (and sections) whose text names `name` after a rename: `[{ id, title }]`. The scan is the name scan of 9.10 on the SQL. */
export function stalePanels(doc, name) {
  const out = [];
  for (const section of doc.sections) {
    for (const p of section.panels) {
      if (p.data && p.data.query && variableNamesOfLines(p.data.query.sql).includes(name)) out.push({ id: p.id, title: p.title });
    }
  }
  return out;
}

/** The variables whose SQL names `name` (a query variable): the names. */
export function staleVariables(variables, name, except) {
  return variables.filter((v) => v.kind === "query" && v.name !== except && variableNamesOfLines(v.sql).includes(name)).map((v) => v.name);
}

/** The name of the first variable, in array order, that is in a cycle of references (4.34), or null. A variable that names itself is a cycle. */
export function variableCycle(variables) {
  const byName = new Map(variables.map((v) => [v.name, v]));
  const edges = new Map();
  for (const v of variables) {
    edges.set(v.name, v.kind === "query" ? variableNamesOfLines(v.sql).filter((n) => byName.has(n)) : []);
  }
  const reaches = (from, target, seen = new Set()) => {
    for (const next of edges.get(from) || []) {
      if (next === target) return true;
      if (seen.has(next)) continue;
      seen.add(next);
      if (reaches(next, target, seen)) return true;
    }
    return false;
  };
  for (const v of variables) if (reaches(v.name, v.name)) return v.name;
  return null;
}

/** The indexes of the options that repeat the value of an option before them (4.9). */
export function duplicateValueIndexes(options) {
  const seen = new Set();
  const out = new Set();
  options.forEach((o, i) => {
    if (seen.has(o.value)) out.add(i);
    seen.add(o.value);
  });
  return out;
}
