// Frame to fields (9.12, FE-03). Pure module: no DOM access.
// A frame is the JSON of 7.16. A field has name, type, kind, values, labels, config, and for some kinds nulls and num.

/** Map a JSON number cell of the kind `number` to a double. */
function numberOf(cell) {
  if (cell === "nan") return NaN;
  if (cell === "inf") return Infinity;
  if (cell === "-inf") return -Infinity;
  return Number(cell);
}

/** Make one field from a column description and its value array. */
function makeField(column, raw) {
  const kind = column.kind;
  const n = raw.length;
  const field = { name: column.name, type: column.type, kind, values: null, labels: null, config: {} };
  if (kind === "time" || kind === "number") {
    const values = new Float64Array(n);
    const nulls = new Uint8Array(n);
    for (let i = 0; i < n; i++) {
      const cell = raw[i];
      if (cell === null || cell === undefined) {
        values[i] = NaN;
        nulls[i] = 1;
      } else if (kind === "time") {
        values[i] = Number(cell) * 1000;
      } else {
        values[i] = numberOf(cell);
      }
    }
    field.values = values;
    field.nulls = nulls;
  } else if (kind === "number_text") {
    const values = new Array(n);
    const num = new Float64Array(n);
    const nulls = new Uint8Array(n);
    for (let i = 0; i < n; i++) {
      const cell = raw[i];
      if (cell === null || cell === undefined) {
        values[i] = null;
        num[i] = NaN;
        nulls[i] = 1;
      } else {
        values[i] = String(cell);
        num[i] = parseFloat(values[i]);
      }
    }
    field.values = values;
    field.num = num;
    field.nulls = nulls;
  } else if (kind === "bool") {
    field.values = raw.map((c) => (c === null || c === undefined ? null : Boolean(c)));
  } else if (kind === "null") {
    field.values = raw.map(() => null);
  } else {
    // text: the server always sends a JSON string or null
    field.values = raw.map((c) => (c === null || c === undefined ? null : typeof c === "string" ? c : String(c)));
  }
  return field;
}

/** Turn a frame (7.16) into an array of fields. */
export function framesToFields(frame) {
  const columns = frame.columns || [];
  const values = frame.values || [];
  return columns.map((column, i) => makeField(column, values[i] || []));
}

// A field made by grouping_to_matrix can hold an empty cell that is not null: `fill` is its value ("", true or false).
function fillOf(field) {
  return field.fill === undefined ? null : field.fill;
}

/** The Grafana value of the cell i of a field (9.12): number, text, boolean or null. */
export function grafanaValue(field, i) {
  switch (field.kind) {
    case "time":
    case "number":
      return field.nulls[i] ? fillOf(field) : field.values[i];
    case "number_text":
      return field.nulls[i] ? fillOf(field) : field.num[i];
    case "text":
    case "bool":
      return field.values[i] === undefined ? null : field.values[i];
    default:
      return null;
  }
}

/** The text form of a Grafana value: `String(value)`. */
export function valueText(v) {
  return String(v);
}

/** The number of rows of a field list (the length of the first field). */
export function rowCount(fields) {
  if (!fields.length) return 0;
  return fields[0].values.length;
}

/** Copy a field with a patch of members. Typed arrays stay as they are (shared). */
export function cloneField(field, patch = {}) {
  return { ...field, config: { ...field.config }, labels: field.labels ? { ...field.labels } : null, ...patch };
}

function pick(arr, indices) {
  if (arr instanceof Float64Array || arr instanceof Uint8Array) {
    const out = new arr.constructor(indices.length);
    for (let k = 0; k < indices.length; k++) out[k] = arr[indices[k]];
    return out;
  }
  const out = new Array(indices.length);
  for (let k = 0; k < indices.length; k++) out[k] = arr[indices[k]];
  return out;
}

/** Keep the rows at `indices`, in this order, in each field. */
export function selectRows(fields, indices) {
  return fields.map((f) => {
    const next = cloneField(f);
    next.values = pick(f.values, indices);
    if (f.nulls) next.nulls = pick(f.nulls, indices);
    if (f.num) next.num = pick(f.num, indices);
    return next;
  });
}
