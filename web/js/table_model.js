// Table sort, column filter and the inspect text (9.20, FE-11). Pure module: no DOM access.
// Numbers, exact decimals and times compare with the exact compare of the copied app_results_sort.js.
import { compareNormalizedForSort, compareNumericForSort, compareTextForSort, normalizeNumericForSort } from "./vendor/chdash/app_results_sort.js";
import { NUMERIC_RE } from "./vendor/chdash/app_results_values.js";
import { parseCss, relativeLuminance } from "./colors.js";

// The value to compare, or null for a NULL (a NaN counts as NULL for the sort).
function sortKey(field, row) {
  switch (field.kind) {
    case "time":
    case "number":
      return field.nulls[row] || Number.isNaN(field.values[row]) ? null : field.values[row];
    case "number_text":
      return field.nulls[row] ? null : field.values[row];
    case "bool":
      return field.values[row] === null ? null : field.values[row] ? 1 : 0;
    default:
      return field.values[row] === null || field.values[row] === undefined ? null : field.values[row];
  }
}

function comparatorFor(field) {
  if (field.kind === "text") return compareTextForSort;
  return compareNormalizedForSort;
}

// A number key is normalised once for each row, not at each compare (E29).
const prep = (field, v) => (v === null || field.kind === "text" ? v : normalizeNumericForSort(v));

/**
 * The row order of a table. `sort` is [{index, desc}] (the first key first). `filters` is [{index, allowed: Set}]:
 * a row stays when the display text of each filtered column is allowed. NULL values come last in both directions.
 * The sort is stable. `textAt(fieldIndex, row)` gives the display text.
 */
export function rowOrder({ count, fields, sort = [], filters = [], textAt }) {
  let rows = [];
  for (let row = 0; row < count; row++) {
    let keep = true;
    for (const f of filters) {
      if (!f.allowed.has(textAt(f.index, row))) {
        keep = false;
        break;
      }
    }
    if (keep) rows.push(row);
  }
  if (!sort.length) return rows;
  const keys = sort.map((s) => ({
    desc: !!s.desc,
    compare: comparatorFor(fields[s.index]),
    values: rows.length ? null : [],
    field: fields[s.index],
  }));
  rows = rows.map((row) => ({ row, k: keys.map((k) => prep(k.field, sortKey(k.field, row))) }));
  rows.sort((a, b) => {
    for (let j = 0; j < keys.length; j++) {
      const x = a.k[j];
      const y = b.k[j];
      if (x === null && y === null) continue;
      if (x === null) return 1;
      if (y === null) return -1;
      const c = keys[j].compare(x, y);
      if (c !== 0) return keys[j].desc ? -c : c;
    }
    return a.row - b.row;
  });
  return rows.map((r) => r.row);
}

/** The distinct display texts of a column, in sorted order (numbers as numbers when all texts are numbers). */
export function distinctTexts(count, fieldIndex, textAt) {
  const set = new Set();
  for (let row = 0; row < count; row++) set.add(textAt(fieldIndex, row));
  const list = [...set];
  const numeric = list.length > 0 && list.every((t) => NUMERIC_RE.test(t));
  list.sort(numeric ? compareNumericForSort : compareTextForSort);
  return list;
}

/** The initial sort of the option `sort` (4.27): field display names to field indices. Unknown names are ignored. */
export function initialSort(sortOption, names) {
  const out = [];
  for (const key of sortOption || []) {
    const index = names.indexOf(key.field);
    if (index >= 0) out.push({ index, desc: !!key.desc });
  }
  return out;
}

const MIN_TEXT_CONTRAST = 4.5;

function contrastOf(a, b) {
  const hi = Math.max(a, b);
  const lo = Math.min(a, b);
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * The text colour of a "color_text" cell on a surface. `color` is the colour of the value (9.16), `surface` the colour of
 * the panel and `ink` the text colour of the theme (all CSS colours, as `rgb(r, g, b)` or hex). A colour with a contrast
 * of 4.5 or more on the surface stays as it is. A pale colour on a light surface (yellow on white) or a dark colour on a
 * dark surface moves towards `ink` in steps of 10 %, up to the first step that reaches 4.5. A colour that cannot be read
 * (the `text` token, a colour with an alpha channel) stays as it is.
 */
export function legibleColor(color, surface, ink) {
  const c = parseCss(color);
  const s = parseCss(surface);
  const t = parseCss(ink);
  if (!c || !s || !t || c.a < 1 || s.a < 1) return color;
  const lumSurface = relativeLuminance(surface);
  if (contrastOf(relativeLuminance(color), lumSurface) >= MIN_TEXT_CONTRAST) return color;
  for (let step = 1; step <= 10; step++) {
    const k = step / 10;
    const mixed = { r: Math.round(c.r + (t.r - c.r) * k), g: Math.round(c.g + (t.g - c.g) * k), b: Math.round(c.b + (t.b - c.b) * k) };
    const css = `rgb(${mixed.r}, ${mixed.g}, ${mixed.b})`;
    if (contrastOf(relativeLuminance(css), lumSurface) >= MIN_TEXT_CONTRAST) return css;
  }
  return ink;
}

// ---- footer (9.20) ----

const FOOTER_NAMES = Object.freeze({
  last_not_null: "Last *", first_not_null: "First *", total: "Total", std_dev: "StdDev", diff: "Difference", diff_percent: "Difference percent",
  change_count: "Change Count", distinct_count: "Distinct Count",
});
const FOOTER_TEXT_CALCS = new Set(["first", "first_not_null", "last", "last_not_null", "count", "distinct_count", "change_count"]);

/** The Grafana name of a calculation in a footer line: `last_not_null` is `Last *`, `std_dev` is `StdDev`, `min` is `Min`. */
export function footerName(calc) {
  return FOOTER_NAMES[calc] || calc.charAt(0).toUpperCase() + calc.slice(1);
}

/** False for a calculation that a column of the kind `text` or `bool` does not show (its footer line is empty). */
export function footerAllowed(kind, calc) {
  return kind === "text" || kind === "bool" ? FOOTER_TEXT_CALCS.has(calc) : true;
}

/**
 * The field that holds only the rows `rows` (in this order), for a calculation over the rows that the column filters keep. The
 * result has the members that `reduce.js` reads: `kind`, `values`, `nulls` and `num`.
 */
export function subsetField(field, rows) {
  const pick = (src) => {
    if (!src) return src;
    const out = new (src.constructor === Array ? Array : src.constructor)(rows.length);
    rows.forEach((row, k) => {
      out[k] = src[row];
    });
    return out;
  };
  return { ...field, values: pick(field.values), nulls: pick(field.nulls), num: pick(field.num) };
}

/**
 * The layout of the footer cells (pure). `columns` is [{ calcs: string[] }] for the shown columns (empty `calcs`: no footer).
 * Gives { lines, names } where `lines` is the number of lines of the tallest footer, and `names` is true when the first column
 * shows the names and the others show the values only: all the columns that have a footer have the same items, and the first
 * shown column has none. Else each cell shows its names and its values.
 */
export function footerLayout(columns) {
  const withFooter = columns.filter((c) => c.calcs.length);
  const lines = Math.max(0, ...columns.map((c) => c.calcs.length));
  if (!withFooter.length) return { lines: 0, names: false };
  const same = withFooter.every((c) => c.calcs.join("\u0000") === withFooter[0].calcs.join("\u0000"));
  return { lines, names: same && columns[0].calcs.length === 0 };
}

// --- the drawer "Inspect value" (9.20) ---------------------------------------------------------------------------------------

export const INSPECT_CAP = 1048576; // UTF-16 code units
const INSPECT_DEPTH = 64;

/**
 * A JSON object or array with an indent of 2 spaces, or null. JSON.parse checks the text, and the indent keeps its tokens (digits,
 * escapes). A 65th level gives null. The work stops after `cap` units of output.
 */
export function indentJson(text, cap = INSPECT_CAP) {
  if (!/^[[{]/.test(text)) return null;
  try {
    JSON.parse(text);
  } catch {
    return null;
  }
  let out = "";
  let depth = 0;
  const line = () => `\n${"  ".repeat(depth)}`;
  for (let i = 0; i < text.length && out.length <= cap; i++) {
    const c = text[i];
    if (c === '"') {
      let j = i + 1;
      while (text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
      out += text.slice(i, j + 1);
      i = j;
    } else if (c === "{" || c === "[") {
      // An empty object or array stays "{}" or "[]".
      let j = i + 1;
      while (text[j] <= " ") j++;
      if (text[j] === "}" || text[j] === "]") {
        out += c + text[j];
        i = j;
      } else if (++depth > INSPECT_DEPTH) return null;
      else out += c + line();
    } else if (c === "}" || c === "]") {
      depth--;
      out += line() + c;
    } else if (c === ",") out += `,${line()}`;
    else if (c === ":") out += ": ";
    else if (c > " ") out += c;
  }
  return out;
}

/** The text of the drawer for a raw value: trimmed (as Grafana), indented when it is JSON, cut to `cap` units but not in a surrogate pair. */
export function inspectText(value, cap = INSPECT_CAP) {
  let text = value === null || value === undefined ? "" : String(value).trim();
  text = (text.length <= cap && indentJson(text, cap)) || text;
  const cut = text.length > cap;
  return { text: cut ? text.slice(0, /[\uD800-\uDBFF]/.test(text[cap - 1]) ? cap - 1 : cap) : text, cut };
}
