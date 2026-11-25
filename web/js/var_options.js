// Options and selection of a variable (9.10, FE-02). Pure module: no DOM access.
import { MAX_PATTERN_TEXT, parseGrafanaPattern } from "./fields.js";

export const ALL = "$__all";
/** The value of the option "Auto" of an `interval` variable (4.10a). The server computes its text (7.4a): the page never does. */
export const AUTO = "$__auto";
const NUMBER_RE = /^-?[0-9]+(\.[0-9]+)?$/;

export const isNumberText = (s) => NUMBER_RE.test(String(s));

const NUM_IN_TEXT = /-?[0-9]+(\.[0-9]+)?/;
const collator = new Intl.Collator("en", { numeric: true });

function applyRegex(raw, regexText) {
  const re = parseGrafanaPattern(regexText);
  if (!re) return raw;
  const out = [];
  for (const option of raw) {
    // The pattern never matches a text longer than MAX_PATTERN_TEXT, so the option goes (9.10, T87).
    if (option.text.length > MAX_PATTERN_TEXT) continue;
    re.lastIndex = 0;
    if (!re.global) {
      const m = re.exec(option.text);
      if (!m) continue;
      out.push(fromMatch(m, option));
      continue;
    }
    let found = false;
    for (const m of option.text.matchAll(re)) {
      found = true;
      out.push(fromMatch(m, option));
    }
    if (!found) continue;
  }
  return out;
}

function fromMatch(m, option) {
  const g = m.groups || {};
  const named = g.value !== undefined || g.text !== undefined;
  const first = m[1] !== undefined ? m[1] : undefined;
  if (named) {
    const value = g.value !== undefined ? g.value : g.text;
    return { value, text: g.text !== undefined ? g.text : value };
  }
  if (first !== undefined) return { value: first, text: first };
  return { value: option.value, text: option.text };
}

function sortOptions(options, mode) {
  if (!mode || mode === "none") return options;
  const desc = mode.endsWith("_desc");
  const kind = mode.slice(0, mode.lastIndexOf("_"));
  const list = options.map((o, i) => ({ o, i }));
  const flip = (c) => (desc ? -c : c);
  list.sort((a, b) => {
    let c = 0;
    if (kind === "alpha") c = a.o.text < b.o.text ? -1 : a.o.text > b.o.text ? 1 : 0;
    else if (kind === "alpha_ci") {
      const x = a.o.text.toLowerCase();
      const y = b.o.text.toLowerCase();
      c = x < y ? -1 : x > y ? 1 : 0;
    } else if (kind === "natural") c = collator.compare(a.o.text, b.o.text);
    else if (kind === "num") {
      const x = NUM_IN_TEXT.exec(a.o.text);
      const y = NUM_IN_TEXT.exec(b.o.text);
      if (!x && !y) return a.i - b.i;
      if (!x) return 1;
      if (!y) return -1;
      c = Number(x[0]) - Number(y[0]);
    }
    return flip(c) || a.i - b.i;
  });
  return list.map((e) => e.o);
}

/** The options of a query variable from the answer of 7.14: regex, duplicates, sort (9.10). */
export function buildOptions(rawOptions, variable) {
  let options = rawOptions.map((o) => ({ value: String(o.value), text: String(o.text) }));
  if (variable.regex) options = applyRegex(options, variable.regex);
  const seen = new Set();
  options = options.filter((o) => (seen.has(o.value) ? false : (seen.add(o.value), true)));
  return sortOptions(options, variable.sort);
}

/** The option values of a variable, in the order that the bar shows. */
export function allOptionValues(variable, options) {
  return options.map((o) => o.value);
}

const encoder = new TextEncoder();

/** True for a variable that is open: `allow_custom_value` is `true` (4.8, 4.9). A person can also type a value for it. */
export const isOpen = (variable) => variable.allow_custom_value === true;

/**
 * The error of a text that a person types in an open variable (9.10): the text of the error, or "" when the text is a typed value.
 * A typed value has 1 to 1,024 bytes and no control character, and it is not `$__all`.
 */
export function typedValueError(text) {
  const bytes = encoder.encode(text).length;
  if (bytes < 1 || bytes > 1024) return "A value has 1 to 1024 bytes.";
  if (/[\u0000-\u001f\u007f]/.test(text)) return "A value has no control character.";
  return text === ALL ? "This value is reserved." : "";
}

// A value stays when it is an option, or a typed value of an open variable (9.10). The set makes the check linear in the sizes (T78).
function stays(variable, valid) {
  return (v) => valid.has(v) || (isOpen(variable) && !typedValueError(v));
}

// The default of a variable as an array: the `default` of an `interval` is a text or null (4.10a), the others have an array (4.11).
const defaultsOf = (variable) => (typeof variable.default === "string" ? [variable.default] : variable.default || []);

function pickFrom(candidates, variable, options) {
  const valid = new Set(options.map((o) => o.value));
  if (candidates.includes(ALL)) return variable.include_all ? { all: true, values: [] } : null;
  const kept = candidates.filter(stays(variable, valid));
  if (!kept.length) return null;
  return { all: false, values: variable.multi ? [...new Set(kept)] : [kept[0]] };
}

/**
 * The selection of a variable (9.10). `urlValues` is the array of the URL values or undefined. `currentValues` is the
 * array of the current selection (with "$__all" for All) or undefined. Returns { all, values }.
 */
export function resolveSelection(variable, options, urlValues, currentValues) {
  // A constant never reads the URL or the current selection: its value is the value of the document (4.10b, rule 3).
  if (variable.kind === "constant") return { all: false, values: [variable.value] };
  if (variable.kind === "text") {
    const v = urlValues && urlValues.length ? urlValues[0] : currentValues && currentValues.length ? currentValues[0] : variable.default;
    return { all: false, values: [v === undefined ? "" : String(v)] };
  }
  const first = urlValues && urlValues.length ? urlValues : currentValues && currentValues.length ? currentValues : defaultsOf(variable);
  const picked = pickFrom(first, variable, options) || pickFrom(defaultsOf(variable), variable, options);
  if (picked) return picked;
  if (variable.include_all) return { all: true, values: [] };
  if (options.length) return { all: false, values: [options[0].value] };
  return { all: false, values: [] };
}

/**
 * The selection that the page picks for a variable when the URL has no value for it (9.5): the rules of `resolveSelection` with no URL value and no
 * current selection. `options` are the options of the variable as the page knows them now.
 */
export function defaultSelection(variable, options) {
  return resolveSelection(variable, options, undefined, undefined);
}

/** True when two selections have the same All flag and the same values in the same order. */
export function sameSelection(a, b) {
  return a.all === b.all && a.values.length === b.values.length && a.values.every((v, i) => v === b.values[i]);
}

/** The values that the page requested and drops (9.10): not an option, and not a typed value of an open variable. `$__all` stays with `include_all`. */
export function droppedValues(variable, options, requested) {
  const keep = stays(variable, new Set(options.map((o) => o.value)));
  return [...new Set(requested)].filter((v) => !(v === ALL ? variable.include_all : keep(v)));
}

/** A text of at most 40 characters: a longer text is cut and gets `...` (9.10, the notice and the row `Use`). */
export const cut = (text) => (text.length > 40 ? `${text.slice(0, 40)}...` : text);

/**
 * The text of the notice of a replaced value (9.10). `label` is the label of the variable, `dropped` the values that went,
 * `selection` the selection that the page used. A value of more than 40 characters is cut. The list has 3 values at most.
 */
export function replacedNotice(label, dropped, options, selection) {
  const list = dropped.slice(0, 3).map((v) => `"${cut(v)}"`).join(", ") + (dropped.length > 3 ? ` and ${dropped.length - 3} more` : "");
  const texts = selection.values.slice(0, 3).map((v) => (options.find((o) => o.value === v) || { text: v }).text);
  const used = selection.all ? "All" : texts.length ? texts.join(" + ") : "no value";
  return `${dropped.length > 1 ? "These values are" : "This value is"} not an option of ${label}: ${list}. The page used "${used}".`;
}

/**
 * The notice for a variable whose selection was just resolved (9.10): { name, text }, or null. `urlValues`, `currentValues` and `after` are the
 * arguments and the result of `resolveSelection`. A variable with no option has no notice: the control shows "No options" or the error.
 */
export function replacement(variable, options, urlValues, currentValues, after) {
  if (!options.length) return null;
  const requested = urlValues && urlValues.length ? urlValues : currentValues && currentValues.length ? currentValues : defaultsOf(variable);
  const dropped = droppedValues(variable, options, requested);
  return dropped.length ? { name: variable.name, text: replacedNotice(variable.label || variable.name, dropped, options, after) } : null;
}

/** The values for the URL: one for each value, or "$__all". A `constant` has none (9.5). */
export function selectionToUrl(variable, selection) {
  if (variable.kind === "constant") return [];
  if (variable.kind === "adhoc") return selection.values.length ? selection.values : [""];
  if (variable.kind === "text") return [selection.values[0] === undefined ? "" : selection.values[0]];
  return selection.all ? [ALL] : selection.values;
}

/** The member of `variables` of a query body (6.20, 7.7). */
export function requestValue(variable, options, selection) {
  if (variable.kind === "text") return { all: false, values: [selection.values[0] === undefined ? "" : selection.values[0]] };
  if (selection.all) {
    if (variable.kind === "query" && (variable.all_value === null || variable.all_value === undefined)) {
      return { all: true, values: allOptionValues(variable, options) };
    }
    return { all: true, values: [] };
  }
  return { all: false, values: [...selection.values] };
}

/** The options of a variable: its own list for `custom` and `interval` ("Auto" first, 4.10a), else the loaded options. */
export function optionsOf(variable, loaded) {
  if (variable.kind === "custom") return variable.options.map((o) => ({ value: o.value, text: o.text }));
  if (variable.kind === "interval") return [...(variable.auto ? [{ value: AUTO, text: "Auto" }] : []), ...variable.intervals.map((v) => ({ value: v, text: v }))];
  return loaded || [];
}

// --- Variables in texts (9.13, F13) ---------------------------------------------------------------------------------------
// `${name}`, `${name:format}` and `$name`. A display text shows the option texts by default (see placeholderText); SQL is not affected. The name is the longest run of letters, digits and "_" (7.6).
const PLACEHOLDER = /\$\{([A-Za-z0-9_]+)(?::([A-Za-z0-9_]+))?\}|\$([A-Za-z0-9_]+)/g;

/**
 * What a placeholder needs to know about a variable: { all, allValue, values, texts }. `values` and `texts` are the effective
 * lists: for "All" they hold every option, else the selection. `loaded` is the loaded options of a query variable.
 */
export function placeholderInfo(variable, selection, loaded) {
  if (variable.kind === "text") {
    const v = selection.values[0] === undefined ? "" : String(selection.values[0]);
    return { all: false, allValue: null, values: [v], texts: [v] };
  }
  const options = optionsOf(variable, loaded);
  if (selection.all) {
    return { all: true, allValue: variable.all_value === undefined ? null : variable.all_value, values: options.map((o) => o.value), texts: options.map((o) => o.text) };
  }
  const byValue = new Map(options.map((o) => [o.value, o.text]));
  return { all: false, allValue: null, values: [...selection.values], texts: selection.values.map((v) => (byValue.has(v) ? byValue.get(v) : v)) };
}

/** The text of one placeholder, or null when the format is not one of 9.13 (the placeholder then stays as it is written). */
function placeholderText(info, format) {
  const useAll = info.all && info.allValue !== null;
  switch (format) {
    // A display text (title, description, section title, link title) shows the option TEXT, also without a format, as Grafana 12.4.2
    // does: `${env}` gives "Production" for the value "prod", a list gives "5 minutes + 15 minutes", "All" gives "All".
    case "":
    case "text":
      return info.all ? "All" : info.texts.join(" + ");
    // The format `glob` of the values (the format that a SQL text uses by default) stays available by name.
    case "glob":
      if (useAll) return String(info.allValue);
      if (info.values.length === 0) return "";
      return info.values.length === 1 ? info.values[0] : `{${info.values.join(",")}}`;
    case "csv":
      return useAll ? String(info.allValue) : info.values.join(",");
    case "pipe":
      return useAll ? String(info.allValue) : info.values.join("|");
    default:
      return null;
  }
}

/**
 * Replace the placeholders of a title, a description or a link text (9.13). `lookup(name)` gives the info of a variable
 * (see `placeholderInfo`) or a falsy value. A name that is not a variable, a name that starts with `__` and a format that 9.13
 * does not list stay as written. The result is text: the caller writes it with `textContent`.
 */
export function interpolateText(text, lookup) {
  const out = expandText(text, lookup, 65536);
  return out === null ? text: out; // an expansion above 65,536 characters stays as written
}

/** The same replacement with a budget: gives null as soon as the result has more than `max` characters (the budget stops it, never a RangeError). */
export function expandText(text, lookup, max) {
  if (typeof text !== "string" || !text.includes("$")) return text;
  let used = text.length;
  const out = text.replace(PLACEHOLDER, (whole, braced, format, bare) => {
    if (used > max) return "";
    const name = braced !== undefined ? braced : bare;
    const info = name.startsWith("__") ? null : lookup(name);
    const piece = info ? placeholderText(info, format === undefined ? "" : format) : null;
    if (piece === null) return whole;
    used += piece.length - whole.length;
    return piece;
  });
  return used > max ? null : out;
}
