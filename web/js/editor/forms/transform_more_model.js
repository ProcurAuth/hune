// The data rules of the forms of the eleven more transforms (10.3a, 4.25a, F28). Pure module: no DOM access.
// A new transform is valid at once and changes no data. Every function returns new objects and never changes its input.
// The members come in the order of 4.38 with `kind` first. `moreFaults` is the check of 4.25a and 4.34 that the page can make before the save.

import { CALCS } from "../../reduce.js";
import { TITLES } from "../../transform_more.js";
import { parseGrafanaPattern } from "../../fields.js";
import { checkPattern, charLength, byteLength } from "./rules.js";

// The bounds of 4.25a and 4.35 that a form shows.
export const MORE_LIMITS = Object.freeze({ items: 64, sort: 8, reducers: 18, partition: 4, conversions: 16, count: 1000000, nameChars: 256, nameBytes: 1024 });

export const MORE_KIND_TEXT = Object.freeze([
  ["organize", "Organize fields"],
  ["calculate_field", "Add field from calculation"],
  ["sort_by", "Sort by"],
  ["limit", "Limit"],
  ["merge", "Merge"],
  ["reduce", "Reduce"],
  ["partition_by_values", "Partition by values"],
  ["join_by_field", "Join by field"],
  ["filter_fields_by_name", "Filter fields by name"],
  ["rename_by_regex", "Rename fields by regex"],
  ["convert_field_type", "Convert field type"],
]);

export const CALC_MODES = Object.freeze([["binary", "Binary operation"], ["reduce_row", "Reduce row"], ["unary", "Unary operation"], ["index", "Row index"]]);
export const OPERATORS = Object.freeze([["add", "+"], ["subtract", "-"], ["multiply", "*"], ["divide", "/"]]);
export const UNARY_OPERATORS = Object.freeze([["abs", "abs"], ["exp", "exp"], ["ln", "ln"], ["round", "round"], ["floor", "floor"], ["ceil", "ceil"], ["percent", "percent"]]);
export const REDUCE_MODES = Object.freeze([["series_to_rows", "Series to rows"], ["reduce_fields", "Reduce fields"]]);
export const REDUCER_ITEMS = Object.freeze(CALCS.map((c) => [c, TITLES[c]]));
export const CONVERT_TYPES = Object.freeze([["number", "Number"], ["text", "Text"], ["time", "Time"], ["boolean", "Boolean"]]);
export const JOIN_MODES = Object.freeze([["outer", "OUTER (time series or tables)"], ["inner", "INNER"]]);
export const LIMIT_ENDS = Object.freeze([["first", "First rows"], ["last", "Last rows"]]);
export const OPERAND_KINDS = Object.freeze([["field", "Field"], ["number", "Number"]]);

const isMore = (kind) => MORE_KIND_TEXT.some(([k]) => k === kind);

/** True for one of the eleven kinds of 4.25a. */
export const isMoreKind = isMore;

/** The text of the head of a transform of the eleven kinds. */
export const moreKindText = (kind) => (MORE_KIND_TEXT.find(([k]) => k === kind) || [kind, kind])[1];

/** The `calculate_field` object of a mode, with a field name `field` for the operands that need one. */
export function calcObject(mode, field = "Value") {
  switch (mode) {
    case "binary": return { left: { kind: "field", field }, right: { kind: "number", value: 1 }, operator: "add" };
    case "reduce_row": return { reducer: "total", include: [] };
    case "unary": return { operator: "abs", field };
    default: return { as_percentile: false };
  }
}

const CALC_KEY = { binary: "binary", reduce_row: "reduce", unary: "unary", index: "index" };

/** A `calculate_field` of `mode`: the alias and `replace_fields` stay, and the object of the mode comes last (4.38). */
export function calcOfMode(previous, mode, field = "Value") {
  return { kind: "calculate_field", mode, alias: previous ? previous.alias : null, replace_fields: previous ? previous.replace_fields : false, [CALC_KEY[mode]]: calcObject(mode, field) };
}

/** The member name of the object of a mode (4.25a). */
export const calcKey = (mode) => CALC_KEY[mode];

/** A new transform of one of the eleven kinds. `field` is the first field of the last data, or "Value". */
export function newMoreTransform(kind, field = "Value") {
  switch (kind) {
    case "organize": return { kind, hide: [], order: [], rename: [] };
    case "calculate_field": return calcOfMode(null, "index", field);
    case "sort_by": return { kind, sort: [{ field, desc: false }] };
    case "limit": return { kind, count: 100, end: "first" };
    case "merge": return { kind };
    case "reduce": return { kind, mode: "series_to_rows", reducers: ["max"] };
    case "partition_by_values": return { kind, fields: [field], keep_fields: false };
    case "join_by_field": return { kind, field: null, mode: "outer" };
    case "filter_fields_by_name": return { kind, include: { names: [], pattern: null }, exclude: { names: [], pattern: null } };
    case "rename_by_regex": return { kind, regex: "(.*)", rename_pattern: "$1" };
    default: return { kind: "convert_field_type", conversions: [{ field, to: "text" }] };
  }
}

// --- the checks of one control ------------------------------------------------------------------------------------------------------

/** The error of a name in a list of names at `at`, or "". */
export function nameError(list, at) {
  const n = byteLength(list[at]);
  if (n < 1 || n > MORE_LIMITS.nameBytes) return `A field name needs 1 to ${MORE_LIMITS.nameBytes} bytes.`;
  return list.indexOf(list[at]) === at ? "" : "This name is in the list twice.";
}

/** The error of a new name or an alias (1 to 256 characters), or "". */
export function newNameError(text, what = "The name") {
  const n = charLength(text);
  return n >= 1 && n <= MORE_LIMITS.nameChars ? "" : `${what} needs 1 to ${MORE_LIMITS.nameChars} characters.`;
}

/** The error of the two operands of a binary operation: two numbers are not permitted (4.34), or "". */
export const operandsError = (b) => (b.left.kind === "number" && b.right.kind === "number" ? "One operand must be a field." : "");

/** The error of a pattern of 4.24 in the Grafana form, or "". An empty text is no pattern when `optional`. */
export function patternError(text, optional = false) {
  if (optional && text === "") return "";
  return checkPattern(text, { grafana: true });
}

// --- the faults of a transform --------------------------------------------------------------------------------------------------------

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/**
 * The faults of one transform of the eleven kinds that a form can make or that the JSON tab can hold: [{ pointer, code, message }].
 * `at` is the pointer of the transform. The codes and the pointers are those of 4.37, so a test compares them with the server.
 * A fault of type (a text where a number goes) is not here: the server and the JSON tab report it.
 */
export function moreFaults(t, at = "") {
  const out = [];
  const add = (pointer, code, message) => out.push({ pointer: `${at}${pointer}`, code, message });
  const count = (key, min, max) => {
    const list = t[key];
    if (!Array.isArray(list)) return false;
    if (list.length > max) add(`/${key}`, "too_large", `At most ${max} items.`);
    else if (list.length < min) add(`/${key}`, "out_of_range", `At least ${min} items.`);
    return true;
  };
  const names = (list, base, min = 0, max = MORE_LIMITS.items) => {
    if (!Array.isArray(list)) return;
    if (list.length > max) add(base, "too_large", `At most ${max} names.`);
    else if (list.length < min) add(base, "out_of_range", `At least ${min} names.`);
    list.forEach((n, i) => {
      if (typeof n === "string" && (byteLength(n) < 1 || byteLength(n) > MORE_LIMITS.nameBytes)) add(`${base}/${i}`, "out_of_range", "A field name needs 1 to 1024 bytes.");
      else if (list.indexOf(n) !== i) add(`${base}/${i}`, "not_unique", "This name is in the list twice.");
    });
  };
  const unique = (list, base, keyOf) => list.forEach((x, i) => {
    if (list.findIndex((y) => same(keyOf(y), keyOf(x))) !== i) add(`${base}/${i}`, "not_unique", "This item is in the list twice.");
  });
  const pattern = (text, base) => {
    if (typeof text === "string" && (byteLength(text) < 1 || byteLength(text) > MORE_LIMITS.nameBytes || parseGrafanaPattern(text) === null || patternError(text) !== "")) add(base, "bad_format", "The pattern is not valid.");
  };
  switch (t.kind) {
    case "organize":
      names(t.hide, "/hide");
      names(t.order, "/order");
      if (Array.isArray(t.rename)) {
        if (t.rename.length > MORE_LIMITS.items) add("/rename", "too_large", `At most ${MORE_LIMITS.items} items.`);
        t.rename.forEach((r, i) => {
          if (typeof r.name === "string" && (charLength(r.name) < 1 || charLength(r.name) > MORE_LIMITS.nameChars)) add(`/rename/${i}/name`, "out_of_range", "A new name needs 1 to 256 characters.");
        });
        unique(t.rename, "/rename", (r) => r.field);
      }
      break;
    case "calculate_field": {
      if (typeof t.alias === "string" && (charLength(t.alias) < 1 || charLength(t.alias) > MORE_LIMITS.nameChars)) add("/alias", "out_of_range", "An alias needs 1 to 256 characters.");
      const o = t[CALC_KEY[t.mode]];
      if (t.mode === "binary" && o && o.left && o.right && operandsError(o)) add("/binary", "not_permitted", "One operand must be a field.");
      if (t.mode === "reduce_row" && o) names(o.include, "/reduce/include");
      break;
    }
    case "sort_by":
      if (count("sort", 1, MORE_LIMITS.sort)) unique(t.sort, "/sort", (k) => [k.field, k.desc]);
      break;
    case "limit":
      if (Number.isInteger(t.count) && (t.count < 0 || t.count > MORE_LIMITS.count)) add("/count", "out_of_range", `A count from 0 to ${MORE_LIMITS.count}.`);
      break;
    case "reduce":
      if (count("reducers", 1, MORE_LIMITS.reducers)) unique(t.reducers, "/reducers", (r) => r);
      break;
    case "partition_by_values":
      names(t.fields, "/fields", 1, MORE_LIMITS.partition);
      break;
    case "filter_fields_by_name":
      for (const side of ["include", "exclude"]) {
        if (!t[side]) continue;
        names(t[side].names, `/${side}/names`);
        pattern(t[side].pattern, `/${side}/pattern`);
      }
      break;
    case "rename_by_regex":
      pattern(t.regex, "/regex");
      if (typeof t.rename_pattern === "string" && charLength(t.rename_pattern) > MORE_LIMITS.nameChars) add("/rename_pattern", "out_of_range", "A replacement has 256 characters at most.");
      break;
    case "convert_field_type":
      if (count("conversions", 1, MORE_LIMITS.conversions)) unique(t.conversions, "/conversions", (c) => c.field);
      break;
    default:
      break;
  }
  return out;
}
