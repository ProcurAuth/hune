// More transforms (9.21a, FE-70 to FE-75). Pure module: no DOM access.
// A frame is a list of fields (9.12). Most kinds map one frame to one frame (`STEPS`). `partition_by_values` maps one frame to a list
// of frames. `merge` and `join_by_field` read the whole list of frames and give a list of one frame (9.22a, rule 2).
// The page finds a field by its current name (9.21a, rule 1). `merge` and `join_by_field` find a field by its `name`.
import { cloneField, grafanaValue, rowCount, selectRows } from "./frame.js";
import { MAX_PATTERN_TEXT, parseGrafanaPattern, regexTest } from "./fields.js";
import { displayNames } from "./names.js";
import { reduceField } from "./reduce.js";
import { rowOrder } from "./table_model.js";
import { NUMERIC_RE } from "./vendor/chdash/app_results_values.js";
import { app } from "./config.js";

// The caps of 4.35. The page checks the output of each transform. It never cuts, samples or rounds (9.21a, rule 13).
export const MAX_TRANSFORM_ROWS = 200000;
export const MAX_TRANSFORM_FIELDS = 2000;
export const MAX_TRANSFORM_FRAMES = 100;
export const MAX_MERGE_STEPS = 4000000;

// An error of a transform. It becomes the error of the panel (9.21a, rule 14).
const fail = (kind, text) => {
  throw new Error(`The transform ${kind} failed: ${text}`);
};
const over = (kind, n, what, max) => fail(kind, `${n} ${what}, ${max.toLocaleString("en-US")} at most`);
const isNum = (f) => f.kind === "number" || f.kind === "number_text" || f.kind === "time";

// The check before a step builds a frame of `fields` fields and `rows` rows (9.21a, rule 13). `lead` starts the error text.
// The cell cap is `server.transform_max_cells` from the session (6.10). The page keeps no copy, and fails closed without it.
export function checkShape(lead, fields, rows) {
  const cells = app.session?.transform_max_cells;
  if (!Number.isSafeInteger(cells) || cells < 1) throw new Error(`${lead}: the session has no valid transform_max_cells`);
  const cap = (n, what, max) => {
    if (n > max) throw new Error(`${lead}: ${n} ${what}, ${max.toLocaleString("en-US")} at most`);
  };
  cap(fields, "fields", MAX_TRANSFORM_FIELDS);
  cap(fields * rows, "cells", cells);
}

/**
 * A new field with the kind and the metadata of `model`, with n rows that are all null.
 * A member of `patch` that is `undefined` removes that member (the merge removes `ref` and `prefix`).
 */
function blank(model, n, patch = {}) {
  const f = { ...model, config: { ...model.config }, labels: null, ...patch };
  for (const k of Object.keys(patch)) if (patch[k] === undefined) delete f[k];
  delete f.fill;
  if (model.kind === "number_text") {
    Object.assign(f, { values: new Array(n).fill(null), num: new Float64Array(n).fill(NaN), nulls: new Uint8Array(n).fill(1) });
  } else if (isNum(model)) {
    f.values = new Float64Array(n).fill(NaN);
    f.nulls = new Uint8Array(n).fill(1);
    delete f.num;
  } else {
    f.values = new Array(n).fill(null);
    delete f.nulls;
    delete f.num;
  }
  return f;
}

/** A field of the kind `number` from an array of doubles. A `null` is a null cell. A `NaN` is a number that is not null (9.12). */
function numberField(name, values, config = {}) {
  const nulls = Uint8Array.from(values, (v) => (v === null ? 1 : 0));
  return { name, type: "number", kind: "number", values: Float64Array.from(values, (v) => (v === null ? NaN : v)), nulls, labels: null, config };
}

// Set the cell k of `to` from the cell i of `from`. The two fields have the same kind. transform.js uses it too.
export function copyCell(from, i, to, k) {
  to.values[k] = from.values[i];
  if (from.nulls) to.nulls[k] = from.nulls[i];
  if (from.num) to.num[k] = from.num[i];
}

/**
 * organize (9.21a, rule 2). The step removes the fields of `hide`. It puts the fields of `order` first, in the order of the array.
 * The other fields follow in the order of the frame. Then it renames the fields of `rename`. All names are the names before the step.
 */
function organize(fields, o) {
  const names = displayNames(fields);
  const hide = new Set(o.hide);
  const rank = new Map(o.order.map((n, i) => [n, i]));
  const rename = new Map(o.rename.map((r) => [r.field, r.name]));
  const first = (e) => (rank.has(e.name) ? rank.get(e.name) : 1e9 + e.i);
  const out = fields
    .map((f, i) => ({ f, i, name: names[i] }))
    .filter((e) => !hide.has(e.name))
    .sort((a, b) => first(a) - first(b));
  return out.map((e) => (rename.has(e.name) ? cloneField(e.f, { config: { ...e.f.config, display_name: rename.get(e.name) } }) : e.f));
}

/** sort_by (9.21a, rule 4). The sort is the stable multi-key sort of the table (9.20). A key that is not a field does nothing. */
function sortBy(fields, o) {
  const names = displayNames(fields);
  const sort = o.sort.map((s) => ({ index: names.indexOf(s.field), desc: s.desc })).filter((s) => s.index >= 0);
  if (!sort.length) return fields;
  return selectRows(fields, rowOrder({ count: rowCount(fields), fields, sort, textAt: () => "" }));
}

/** limit (9.21a, rule 5). It keeps the first `count` rows, or the last `count` rows with `end` = `last`. */
function limit(fields, o) {
  const n = rowCount(fields);
  const k = Math.min(o.count, n);
  return selectRows(fields, Array.from({ length: k }, (_, i) => (o.end === "last" ? n - k + i : i)));
}

const OPS = { add: (a, b) => a + b, subtract: (a, b) => a - b, multiply: (a, b) => a * b, divide: (a, b) => a / b };
const SYMBOL = { add: "+", subtract: "-", multiply: "*", divide: "/" };
const UNARY = { abs: Math.abs, exp: Math.exp, ln: Math.log, round: Math.round, floor: Math.floor, ceil: Math.ceil };
// The title of each calculation of 9.19: the default name of a `reduce_row` field and of a column of `reduce`.
export const TITLES = {
  last_not_null: "Last *", last: "Last", first: "First", first_not_null: "First *", min: "Min", max: "Max", mean: "Mean", total: "Total",
  count: "Count", median: "Median", std_dev: "StdDev", variance: "Variance", range: "Range", diff: "Difference", diff_percent: "Difference percent",
  delta: "Delta", distinct_count: "Distinct Count", change_count: "Change Count",
};

// The doubles of a field for an arithmetic. A null cell counts as 0 and a `bool` as 0 or 1, as JavaScript does in Grafana (ID-K243).
function doubles(f, kind) {
  if (f.kind === "bool") return f.values.map((v) => (v ? 1 : 0));
  if (!isNum(f)) fail(kind, `the field ${f.name} is not a number`);
  return f.values.map((v, i) => (f.nulls[i] ? 0 : f.kind === "number_text" ? f.num[i] : v));
}

/**
 * calculate_field (9.21a, rule 3). It adds one field of the kind `number`, or none when a name is not a field.
 * `replace_fields` keeps the first time field and the new field only.
 */
function calculateField(fields, o) {
  const names = displayNames(fields);
  const n = rowCount(fields);
  const at = (name) => fields[names.indexOf(name)];
  const operand = (x) => (x.kind === "number" ? new Array(n).fill(x.value) : at(x.field) && doubles(at(x.field), "calculate_field"));
  let out = null;
  let title = "";
  if (o.mode === "binary") {
    const { left, operator, right } = o.binary;
    const a = operand(left);
    const b = operand(right);
    if (a && b) out = Array.from(a, (v, i) => OPS[operator](v, b[i]));
    title = `${left.field ?? left.value} ${SYMBOL[operator]} ${right.field ?? right.value}`;
  } else if (o.mode === "unary") {
    const f = at(o.unary.field);
    const a = f && doubles(f, "calculate_field");
    if (a && o.unary.operator === "percent") {
      // The sum has the finite values only. A sum of 0 divides by 1 (9.21a, rule 3).
      const sum = a.reduce((s, v) => s + (Number.isFinite(v) ? v : 0), 0);
      out = Array.from(a, (v) => v / (sum || 1));
    } else if (a) out = Array.from(a, UNARY[o.unary.operator]);
    title = `${o.unary.operator}(${o.unary.field})`;
  } else if (o.mode === "index") {
    out = Array.from({ length: n }, (_, i) => (o.index.as_percentile ? i / n : i));
    title = "Row";
  } else {
    // reduce_row: the row is a field of one cell for each source field, so the calculations of 9.19 apply with no change.
    // One field object serves all rows: the calculation reads its cells only.
    const use = fields.filter((f, i) => (f.kind === "number" || f.kind === "number_text") && (!o.reduce.include.length || o.reduce.include.includes(names[i])));
    const cells = numberField("row", new Array(use.length).fill(0));
    out = Array.from({ length: n }, (_, row) => {
      use.forEach((f, j) => {
        cells.nulls[j] = f.nulls[row];
        cells.values[j] = f.kind === "number_text" ? f.num[row] : f.values[row];
      });
      return reduceField(cells, o.reduce.reducer).num;
    });
    title = TITLES[o.reduce.reducer];
  }
  if (!out) return fields;
  const added = numberField(o.alias || title, out, o.mode === "index" && o.index.as_percentile ? { unit: "percentunit" } : {});
  if (!o.replace_fields) return [...fields, added];
  const time = fields.find((f) => f.kind === "time");
  return time ? [time, added] : [added];
}

/**
 * reduce (9.21a, rule 8). The step reduces each field that is not of the kind `time`, and a text field too.
 * `series_to_rows` gives the text field `Field` and one column for each reducer. `reduce_fields` gives one cell for each field and reducer.
 */
function reduce(fields, o) {
  const names = displayNames(fields);
  const use = fields.map((f, i) => ({ f, name: names[i] })).filter((e) => e.f.kind !== "time");
  const cells = (e, calc) => reduceField(e.f, calc).value;
  const make = (name, vals, extra = {}) => {
    // A column has the kind `number` when all its cells are numbers or null. Else it is text.
    const number = vals.every((v) => v === null || typeof v === "number");
    if (number) return { ...numberField(name, vals), ...extra };
    return { name, type: "string", kind: "text", values: vals.map((v) => (v === null ? null : String(v))), labels: null, config: {}, ...extra };
  };
  if (o.mode === "reduce_fields") {
    return use.flatMap((e) => o.reducers.map((calc) => make(e.name, [cells(e, calc)], { labels: { reducer: TITLES[calc] } })));
  }
  const first = { name: "Field", type: "string", kind: "text", values: use.map((e) => e.name), labels: null, config: {} };
  return [first, ...o.reducers.map((calc) => make(TITLES[calc], use.map((e) => cells(e, calc))))];
}

/**
 * partition_by_values (9.21a, rule 9). One new frame comes out for each list of values of the key fields, in the order of first
 * appearance. A value is its text form. Each field of a new frame gets the key labels. A key that is not a field does nothing.
 */
function partition(fields, o) {
  const names = displayNames(fields);
  const keys = o.fields.map((n) => names.indexOf(n)).filter((i) => i >= 0);
  if (!keys.length) return [fields];
  const kept = fields.filter((f, i) => o.keep_fields || !keys.includes(i));
  const groups = new Map();
  for (let row = 0, n = rowCount(fields); row < n; row++) {
    const values = keys.map((i) => String(grafanaValue(fields[i], row)));
    const id = JSON.stringify(values);
    if (!groups.has(id)) groups.set(id, { values, rows: [] });
    groups.get(id).rows.push(row);
    if (groups.size > MAX_TRANSFORM_FRAMES) over("partition_by_values", groups.size, "frames", MAX_TRANSFORM_FRAMES);
  }
  return [...groups.values()].map((g) => {
    const labels = Object.fromEntries(keys.map((i, j) => [fields[i].name, g.values[j]]));
    return selectRows(kept, g.rows).map((f) => cloneField(f, { labels: { ...f.labels, ...labels } }));
  });
}

// The pattern of 4.24 as a regular expression. A pattern that the document check lets pass compiles. A fault is an error of the panel.
function patternOf(kind, text) {
  const re = parseGrafanaPattern(text);
  if (!re) fail(kind, "the pattern is not valid");
  return re;
}

/**
 * filter_fields_by_name (9.21a, rule 10). A field passes the inclusion when the inclusion is empty, or when its current name is in
 * `names` or the pattern matches it. A field that passes stays, unless the exclusion matches it in the same way. A pattern never
 * matches a name longer than MAX_PATTERN_TEXT (regexTest, T87).
 */
function filterFields(fields, o) {
  const names = displayNames(fields);
  const some = (m) => m.names.length > 0 || m.pattern !== null;
  const re = (m) => (m.pattern === null ? null : patternOf("filter_fields_by_name", m.pattern));
  const include = re(o.include);
  const exclude = re(o.exclude);
  const test = (m, r, name) => m.names.includes(name) || (r !== null && regexTest(r, name));
  return fields.filter((f, i) => (!some(o.include) || test(o.include, include, names[i])) && !(some(o.exclude) && test(o.exclude, exclude, names[i])));
}

/**
 * rename_by_regex (9.21a, rule 11). The step calls the JavaScript `replace` on the current name of each field. Only the first match
 * changes, unless the pattern has the flag `g`. A result that is empty or equal to the name keeps the name. A name longer than
 * MAX_PATTERN_TEXT keeps the name (9.14, T87).
 */
function renameByRegex(fields, o) {
  const re = patternOf("rename_by_regex", o.regex);
  const names = displayNames(fields);
  return fields.map((f, i) => {
    const next = names[i].length > MAX_PATTERN_TEXT ? "" : names[i].replace(re, o.rename_pattern);
    return next === "" || next === names[i] ? f : cloneField(f, { config: { ...f.config, display_name: next } });
  });
}

const ISO_RE = /^[0-9]{4}-[0-9]{2}-[0-9]{2}(T[0-9]{2}:[0-9]{2}(:[0-9]{2}(\.[0-9]{1,3})?)?(Z|[+-][0-9]{2}:[0-9]{2}))?$/;
const KIND_OF = { number: "number", text: "text", time: "time", boolean: "bool" };

// An ISO 8601 text in UTC with the letter Z. A time that the Date cannot show gives null.
const isoOf = (ms) => (Number.isFinite(ms) && Math.abs(ms) <= 8.64e15 ? new Date(ms).toISOString() : null);

/**
 * The converted value of the Grafana value `v` (9.12) of a field of the kind `from`: a number, a text, a boolean or null (9.21a, rule 12).
 * The conversion to the same kind never comes here.
 */
function convertValue(v, from, to) {
  if (v === null || v === undefined) return null;
  const x = typeof v === "number" ? v : NaN;
  if (to === "text") return from === "time" ? isoOf(v) : String(v);
  if (to === "time") {
    if (from === "number" || from === "number_text") return Number.isNaN(x) ? null : x;
    const ms = from === "text" && ISO_RE.test(v) ? Date.parse(v) : NaN;
    return Number.isNaN(ms) ? null : ms;
  }
  if (to === "number") {
    if (from === "bool") return v ? 1 : 0;
    const y = from === "text" ? (NUMERIC_RE.test(v.trim()) ? Number(v) : NaN) : x;
    return Number.isNaN(y) ? null : y;
  }
  if (from === "text") return v === "true" || v === "1" ? true : v === "false" || v === "0" ? false : null;
  return from === "time" || Number.isNaN(x) ? null : x !== 0;
}

/** convert_field_type (9.21a, rule 12). Each conversion applies to the field of that current name. A null stays null. */
function convertType(fields, o) {
  const names = displayNames(fields);
  return fields.map((f, i) => {
    const c = o.conversions.find((x) => x.field === names[i]);
    const kind = c && KIND_OF[c.to];
    if (!c || f.kind === kind) return f;
    const vals = Array.from(f.values, (_, row) => convertValue(grafanaValue(f, row), f.kind, c.to));
    const out = cloneField(f, { kind, type: c.to === "text" ? "string" : c.to });
    delete out.num;
    delete out.fill;
    if (kind === "number" || kind === "time") {
      out.values = Float64Array.from(vals, (v) => (v === null ? NaN : v));
      out.nulls = Uint8Array.from(vals, (v) => (v === null ? 1 : 0));
    } else {
      out.values = vals;
      delete out.nulls;
    }
    return out;
  });
}

// ---- merge and join (list transforms) ---------------------------------------------------------------------------------

/**
 * merge (9.21a, rule 6). The rows of all frames join into one frame. A field that all frames have is a key. A row joins the first
 * earlier row of the same key when no field has two different values. A null in the earlier row never conflicts.
 * The fields match by `name`. The merged fields have no `ref` and no `prefix`.
 */
function merge(frames) {
  const list = frames.filter((fs) => fs.length);
  if (list.length < 2) return frames;
  const total = list.reduce((s, fs) => s + rowCount(fs), 0);
  if (total > MAX_TRANSFORM_ROWS) over("merge", total, "rows", MAX_TRANSFORM_ROWS);
  const common = [...new Set(list[0].map((f) => f.name))].filter((n) => list.every((fs) => fs.some((f) => f.name === n)));
  if (!common.length) return frames;
  const model = new Map();
  for (const fs of list) {
    for (const f of fs) {
      if (model.has(f.name) && model.get(f.name).kind !== f.kind) fail("merge", `the field ${f.name} has two types`);
      if (!model.has(f.name)) model.set(f.name, f);
    }
  }
  const names = [...model.keys()];
  // The result has `total` rows at most, and the page keeps one value of each field for each row.
  checkShape("The transform merge failed", names.length, total);
  const keyAt = common.map((c) => names.indexOf(c));
  const rows = [];
  const groups = new Map();
  let steps = 0;
  for (const fs of list) {
    const at = names.map((n) => fs.find((f) => f.name === n));
    for (let r = 0, n = rowCount(fs); r < n; r++) {
      // A field that the frame lacks is `undefined`, and it never conflicts. A null in the new row conflicts with a value.
      const vals = at.map((f) => (f ? grafanaValue(f, r) : undefined));
      const key = JSON.stringify(keyAt.map((j) => vals[j]));
      const same = groups.get(key) || groups.set(key, []).get(key);
      // A compare of two rows is one step for each field.
      const hit = same.find((e) => {
        steps += names.length;
        return vals.every((v, j) => v === undefined || e.vals[j] === undefined || e.vals[j] === null || e.vals[j] === v);
      });
      if (steps > MAX_MERGE_STEPS) fail("merge", "too many comparisons; add a field that all frames share");
      const into = hit || { vals: vals.map(() => undefined), src: [] };
      vals.forEach((v, j) => {
        if (v === undefined) return;
        into.vals[j] = v;
        into.src[j] = [at[j], r];
      });
      if (!hit) {
        same.push(into);
        rows.push(into);
      }
    }
  }
  const out = names.map((name) => blank(model.get(name), rows.length, { name, ref: undefined, prefix: undefined }));
  rows.forEach((e, k) => out.forEach((f, j) => e.src[j] && copyCell(e.src[j][0], e.src[j][1], f, k)));
  return [out];
}

// The order of two keys of a sorted join: a null or a NaN comes first.
const keyOrder = (a, b) => {
  const x = a === null || a !== a ? -Infinity : a;
  const y = b === null || b !== b ? -Infinity : b;
  return x < y ? -1 : x > y ? 1 : 0;
};

/**
 * join_by_field (9.21a, rule 7). One frame comes from all frames, on the key field. The key field of a frame is its first field of the
 * kind `time` when `field` is `null`, else its field of that `name`. `outer` keeps all keys. `inner` keeps the keys of all frames.
 * The page checks the count of output fields, rows and cells before it builds a row (rule 13).
 */
function join(frames, o) {
  if (frames.length < 2) return frames;
  const keyOf = (fs) => {
    const f = o.field === null ? fs.find((x) => x.kind === "time") : fs.find((x) => x.name === o.field);
    if (!f) fail("join_by_field", `a frame has no ${o.field === null ? "time field" : `field ${o.field}`}`);
    return f;
  };
  const keys = frames.map(keyOf);
  if (keys.some((k) => k.kind !== keys[0].kind)) fail("join_by_field", "the key fields have two types");
  const total = frames.reduce((s, fs) => s + rowCount(fs), 0);
  if (total > MAX_TRANSFORM_ROWS) over("join_by_field", total, "rows", MAX_TRANSFORM_ROWS);
  const lead = "The transform join_by_field failed";
  const width = frames.reduce((s, fs) => s + fs.length - 1, 1);
  checkShape(lead, width, 0);
  // The index of a key field: key -> the last row of that key (`outer`), or key -> all rows of that key (`inner`, frames after the first).
  const inner = o.mode === "inner";
  const at = keys.map((k, j) => {
    const m = new Map();
    if (inner && j === 0) return m;
    for (let r = 0, n = k.values.length; r < n; r++) {
      const v = grafanaValue(k, r);
      if (!inner) m.set(v, r);
      else if (m.has(v)) m.get(v).push(r);
      else m.set(v, [r]);
    }
    return m;
  });
  // One output row is a choice of one source row, or -1 for none, in each frame. `src[j][k]` is the row of frame j in the output row k.
  // `outer`: the keys in order, and the last row of a key wins. `inner`: the rows of the first frame in order, each with all the
  // matching rows of the other frames. The first frame is the slowest index, so equal keys multiply.
  const src = keys.map(() => []);
  let count = 0;
  // `inner` walks the rows of the first frame two times: the first walk counts the output rows, the second walk builds them.
  const add = (lists, build) => {
    const size = lists.reduce((s, l) => s * l.length, 1);
    if (!build) {
      count += size;
      if (count > MAX_TRANSFORM_ROWS) over("join_by_field", count, "rows", MAX_TRANSFORM_ROWS);
      return;
    }
    const pos = lists.map(() => 0);
    for (let c = 0; c < size; c++) {
      lists.forEach((l, j) => src[j].push(l[pos[j]]));
      for (let j = lists.length - 1; j >= 0 && ++pos[j] === lists[j].length; j--) pos[j] = 0;
    }
  };
  const walk = (build) => {
    for (let r = 0; r < keys[0].values.length; r++) {
      const v = grafanaValue(keys[0], r);
      const rest = at.slice(1).map((m) => m.get(v));
      if (rest.every((l) => l)) add([[r], ...rest], build);
    }
  };
  let order = [];
  if (inner) walk(false);
  else {
    const ids = new Set();
    for (const m of at) for (const id of m.keys()) ids.add(id);
    // A number key and a time key sort up. A text key and a bool key keep the order of first appearance.
    order = keys[0].kind !== "text" && keys[0].kind !== "bool" ? [...ids].sort(keyOrder) : ids;
    // The count of keys is at most the count of rows, which the check above holds under `MAX_TRANSFORM_ROWS`.
    count = ids.size;
  }
  checkShape(lead, width, count);
  if (inner) walk(true);
  for (const id of order) at.forEach((m, j) => src[j].push(m.has(id) ? m.get(id) : -1));
  // The key field comes from the first frame that has a row. It has no `ref` and no `prefix`.
  const keyField = blank(keys[0], count, { ref: undefined, prefix: undefined });
  for (let k = 0; k < count; k++) {
    const j = src.findIndex((s) => s[k] >= 0);
    copyCell(keys[j], src[j][k], keyField, k);
  }
  const out = [keyField];
  frames.forEach((fs, j) => {
    for (const f of fs) {
      if (f === keys[j]) continue;
      const g = blank(f, count, { labels: f.labels });
      for (let k = 0; k < count; k++) if (src[j][k] >= 0) copyCell(f, src[j][k], g, k);
      out.push(g);
    }
  });
  return [out];
}

const STEPS = {
  organize,
  calculate_field: calculateField,
  sort_by: sortBy,
  limit,
  reduce,
  filter_fields_by_name: filterFields,
  rename_by_regex: renameByRegex,
  convert_field_type: convertType,
};
const LISTS = { merge, join_by_field: join };

/** The 11 kinds of this module (4.25a), in the order of their sets (9.30). */
export const MORE_KINDS = [...Object.keys(STEPS), "partition_by_values", ...Object.keys(LISTS)];

/**
 * Apply one transform of this module to a list of frames (9.21a). It returns the new list. A frame transform maps each frame alone.
 * The check of the caps follows each transform: a frame has `MAX_TRANSFORM_ROWS` rows and `MAX_TRANSFORM_FIELDS` fields at most.
 * The list has `MAX_TRANSFORM_FRAMES` frames at most. A higher count is an error and the page makes no cut (rule 13).
 */
export function applyMore(frames, t) {
  let out;
  if (LISTS[t.kind]) out = LISTS[t.kind](frames, t);
  else if (t.kind === "partition_by_values") out = frames.flatMap((fs) => partition(fs, t));
  else if (STEPS[t.kind]) out = frames.map((fs) => STEPS[t.kind](fs, t));
  else throw new Error(`The transform ${t.kind} is not available in this version of the page.`);
  if (out.length > MAX_TRANSFORM_FRAMES) over(t.kind, out.length, "frames", MAX_TRANSFORM_FRAMES);
  for (const fs of out) {
    if (fs.length > MAX_TRANSFORM_FIELDS) over(t.kind, fs.length, "fields", MAX_TRANSFORM_FIELDS);
    if (rowCount(fs) > MAX_TRANSFORM_ROWS) over(t.kind, rowCount(fs), "rows", MAX_TRANSFORM_ROWS);
  }
  return out;
}
