// Transformations, long-to-wide and panel reuse (9.21 to 9.23, FE-12 to FE-15). Pure module: no DOM access.
import { cloneField, framesToFields, grafanaValue, rowCount, selectRows } from "./frame.js";
import { regexTest } from "./fields.js";
import { displayNames } from "./names.js";
import { combine, tagFields, visibleQueries } from "./queries.js";
import { applyMore, checkShape, copyCell } from "./transform_more.js";

const isNaNValue = (v) => typeof v === "number" ? Number.isNaN(v) : Number.isNaN(Number(v));

// The plain form of 4.24 (a match can start anywhere), or null. filterByValue compiles it one time for all the rows.
function plainPattern(text) {
  try {
    return new RegExp(text);
  } catch {
    return null;
  }
}

// The pass test of a matcher on the Grafana value v (9.21): the rules of JavaScript, as Grafana does. `re` is from plainPattern.
function passes(matcher, v, re) {
  switch (matcher.kind) {
    case "is_null":
      return v === null;
    case "is_not_null":
      return v !== null;
    case "equal":
      // eslint-disable-next-line eqeqeq
      return v == matcher.value;
    case "not_equal":
      // eslint-disable-next-line eqeqeq
      return v != matcher.value;
    case "greater":
      return !isNaNValue(v) && v > matcher.value;
    case "greater_or_equal":
      return !isNaNValue(v) && v >= matcher.value;
    case "lower":
      return !isNaNValue(v) && v < matcher.value;
    case "lower_or_equal":
      return !isNaNValue(v) && v <= matcher.value;
    case "range":
      return !isNaNValue(v) && v > matcher.from && v < matcher.to;
    case "regex":
      // A text longer than MAX_PATTERN_TEXT never passes (9.14, T87).
      return re !== null && regexTest(re, String(v));
    default:
      return false;
  }
}

/**
 * filter_by_value (9.21, FE-12). `several` is true for a panel with several visible queries: a filter that names a field that this frame
 * does not have is skipped there, so the filter of one query does not remove the rows of another query (9.22a, rule 2).
 */
export function filterByValue(fields, spec, several = false) {
  const names = displayNames(fields);
  const n = rowCount(fields);
  const resolved = (spec.filters || []).map((f) => ({ index: names.indexOf(f.field), matcher: f.matcher, re: f.matcher.kind === "regex" ? plainPattern(f.matcher.pattern) : null })).filter((f) => !several || f.index >= 0);
  if (several && !resolved.length) return fields;
  const keep = [];
  for (let row = 0; row < n; row++) {
    const test = (f) => f.index >= 0 && passes(f.matcher, grafanaValue(fields[f.index], row), f.re);
    const pass = spec.match === "all" ? resolved.every(test) : resolved.some(test);
    if (spec.mode === "include" ? pass : !pass) keep.push(row);
  }
  return selectRows(fields, keep);
}

// Builds an empty cell of a field that holds a value that is not null (grouping_to_matrix, empty_value).
function emptyCell(kind, empty) {
  if (empty === "null") return { fill: undefined, direct: null };
  const value = empty === "empty" ? "" : empty === "true";
  return { fill: value, direct: value };
}

/** grouping_to_matrix (9.21, FE-13). */
export function groupingToMatrix(fields, spec) {
  const names = displayNames(fields);
  const ci = names.indexOf(spec.column_field);
  const ri = names.indexOf(spec.row_field);
  const vi = names.indexOf(spec.value_field);
  if (ci < 0 || ri < 0 || vi < 0) return fields;
  const n = rowCount(fields);
  const colField = fields[ci];
  const rowField = fields[ri];
  const valField = fields[vi];
  // Distinct keys by SameValueZero (a Map does this), in the order of the first row.
  const colKeys = new Map();
  const rowKeys = new Map();
  const cells = new Map();
  for (let i = 0; i < n; i++) {
    const c = grafanaValue(colField, i);
    const r = grafanaValue(rowField, i);
    if (!colKeys.has(c)) colKeys.set(c, i);
    if (!rowKeys.has(r)) rowKeys.set(r, i);
    cells.set(JSON.stringify([String(c), String(r)]), i);
  }
  // The caps of 9.21a, rule 13, before the page builds a field.
  checkShape("The transform grouping_to_matrix failed", colKeys.size + 1, rowKeys.size);
  const first = cloneField(selectRows([rowField], [...rowKeys.values()])[0], {
    name: `${spec.row_field}\\${spec.column_field}`,
    labels: null,
  });
  const out = [first];
  const empty = emptyCell(valField.kind, spec.empty_value);
  for (const [c] of colKeys) {
    const idx = [];
    const missing = [];
    for (const [r] of rowKeys) {
      const at = cells.get(JSON.stringify([String(c), String(r)]));
      idx.push(at === undefined ? 0 : at);
      missing.push(at === undefined);
    }
    let f = valField.values.length ? selectRows([valField], idx)[0] : cloneField(valField);
    f = cloneField(f, { name: String(c), labels: null });
    missing.forEach((m, k) => {
      if (!m) return;
      if (f.kind === "number" || f.kind === "time" || f.kind === "number_text") {
        f.nulls[k] = 1;
        if (f.kind === "number_text") {
          f.values[k] = null;
          f.num[k] = NaN;
        } else f.values[k] = NaN;
        if (empty.fill !== undefined) f.fill = empty.fill;
      } else {
        f.values[k] = empty.direct;
      }
    });
    out.push(f);
  }
  return out;
}

// A new empty field of the kind and the metadata of `model`, with n rows set to null.
function blankField(model, n, patch) {
  const f = { ...model, config: { ...model.config }, labels: null, ...patch };
  delete f.fill;
  if (model.kind === "number" || model.kind === "time") {
    f.values = new Float64Array(n).fill(NaN);
    f.nulls = new Uint8Array(n).fill(1);
    delete f.num;
  } else if (model.kind === "number_text") {
    f.values = new Array(n).fill(null);
    f.num = new Float64Array(n).fill(NaN);
    f.nulls = new Uint8Array(n).fill(1);
  } else {
    f.values = new Array(n).fill(null);
    delete f.nulls;
    delete f.num;
  }
  return f;
}

/** Long-to-wide (9.22, FE-14). Throws an Error when a time series query has no time column. */
export function longToWide(fields) {
  const timeIndex = fields.findIndex((f) => f.kind === "time");
  if (timeIndex < 0) throw new Error("A time series query needs a time column.");
  const timeField = fields[timeIndex];
  const labelFields = fields.filter((f) => f.kind === "text");
  const valueFields = fields.filter((f, i) => f.kind !== "text" && i !== timeIndex);
  if (!valueFields.length) return fields;
  const n = timeField.values.length;
  const rows = [];
  for (let i = 0; i < n; i++) if (!timeField.nulls[i]) rows.push(i);
  rows.sort((a, b) => timeField.values[a] - timeField.values[b] || a - b);
  const times = [];
  const timeAt = new Map();
  const sets = new Map();
  const cell = new Map();
  for (const i of rows) {
    const t = timeField.values[i];
    if (!timeAt.has(t)) {
      timeAt.set(t, times.length);
      times.push(t);
    }
    const labels = labelFields.map((f) => (f.values[i] === null ? "" : f.values[i]));
    const key = JSON.stringify(labels);
    if (!sets.has(key)) sets.set(key, labels);
    cell.set(`${key}|${timeAt.get(t)}`, i);
  }
  // The caps of 9.21a, rule 13, before the page builds a field.
  checkShape("The long-to-wide failed", sets.size * valueFields.length + 1, times.length);
  // The series go in the order of their label values, compared by code unit, one label field after the other (as Grafana does).
  // The order of the first row (9.22 step 6) is not used.
  const ordered = [...sets].sort(([, a], [, b]) => {
    for (let j = 0; j < a.length; j++) if (a[j] !== b[j]) return a[j] < b[j] ? -1 : 1;
    return 0;
  });
  const out = [];
  const tf = blankField(timeField, times.length, { name: timeField.name });
  times.forEach((t, k) => {
    tf.values[k] = t;
    tf.nulls[k] = 0;
  });
  out.push(tf);
  for (const [key, labels] of ordered) {
    for (const vf of valueFields) {
      const f = blankField(vf, times.length, { name: vf.name });
      f.labels = {};
      labelFields.forEach((lf, j) => {
        f.labels[lf.name] = labels[j];
      });
      for (let k = 0; k < times.length; k++) {
        const at = cell.get(`${key}|${k}`);
        if (at !== undefined) copyCell(vf, at, f, k);
      }
      out.push(f);
    }
  }
  return out;
}

/** Apply the transforms of the two older kinds to one frame in array order (9.21). A field is found by its current name (9.21a, rule 1). */
export function applyTransforms(fields, transforms, several = false) {
  let current = fields;
  for (const t of transforms || []) {
    if (t.kind === "filter_by_value") current = filterByValue(current, t, several);
    else if (t.kind === "grouping_to_matrix") current = groupingToMatrix(current, t);
  }
  return current;
}

/**
 * Apply the transforms of a panel in array order to the list of frames (9.21a, 9.22a rule 2). `lists` is [{ ref, fields }].
 * A frame transform maps each frame alone. `merge` and `join_by_field` read the whole list and give one frame. `partition_by_values` gives
 * more frames. A kind that this page does not know is an error of the panel. A frame keeps the ref of its first field, else the ref of its list.
 */
export function applyList(lists, transforms, several = false) {
  let current = lists;
  for (const t of transforms || []) {
    if (t.kind === "filter_by_value" || t.kind === "grouping_to_matrix") {
      current = current.map((l) => ({ ...l, fields: applyTransforms(l.fields, [t], several) }));
      continue;
    }
    const out = applyMore(current.map((l) => l.fields), t);
    current = out.map((fields, i) => ({ ref: fields.length && fields[0].ref !== undefined ? fields[0].ref : (current[i] || current[0]).ref, fields }));
  }
  return current;
}

function wideOf(fields, query) {
  if (query && query.format === "time_series" && fields.some((f) => f.kind === "text")) return longToWide(fields);
  return fields;
}

/**
 * The fields of a panel before the field configuration (9.22, 9.22a, 9.23, FE-15). `items` is [{ ref, frame }]: the answers in the order
 * of the queries. For a reuse panel they are the answers of the SOURCE panel, and `sourcePanel` is the source panel.
 * Each frame goes through its long-to-wide. The transforms apply to the list of frames (`applyList`). Then the frames combine by the type of
 * the panel (`choice` is the ref that a table shows). It returns { lists, fields }: `lists` is [{ ref, fields }] before the combination.
 */
export function preparePanelFrames(panel, items, sourcePanel = null, choice = null) {
  const data = panel.data || {};
  if (data.reuse && !sourcePanel) throw new Error("The source panel is not in the dashboard.");
  const owner = data.reuse ? sourcePanel : panel;
  const queries = visibleQueries(owner);
  const many = queries.length > 1;
  const own = items.map(({ ref, frame }) => {
    const fields = wideOf(framesToFields(frame), queries.find((q) => q.ref === ref) || (owner.data && owner.data.query));
    return { ref, fields: ref !== null ? tagFields(fields, ref, many) : fields };
  });
  // The transforms of the source panel come first for a reuse panel, then its own (9.23). A list transform sees all frames of both.
  const all = [...(data.reuse && data.reuse.with_transforms ? sourcePanel.transforms : []), ...(panel.transforms || [])];
  let lists = applyList(own, all, many);
  // A frame of `partition_by_values` has the option text `<ref> <label values of its first field>` in the select of a table (9.22a, rule 9).
  if (all.some((t) => t.kind === "partition_by_values")) {
    lists = lists.map((l) => ({ ...l, ref: [l.ref === null ? "A" : l.ref, ...Object.values((l.fields[0] && l.fields[0].labels) || {})].join(" ") }));
  }
  return { lists, fields: combine(panel.type, lists, choice) };
}

/** The fields of a panel for one frame (9.22, 9.23, FE-15): the case of one query. */
export function preparePanelFields(panel, frame, sourcePanel = null) {
  return preparePanelFrames(panel, [{ ref: null, frame }], sourcePanel).fields;
}
