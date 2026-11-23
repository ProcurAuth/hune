// Reduce calculations (9.19, FE-10). Pure module: no DOM access.
import { parseGrafanaPattern, regexTest } from "./fields.js";
import { compareNumericForSort } from "./vendor/chdash/app_results_sort.js";

export const CALCS = Object.freeze([
  "last_not_null", "last", "first", "first_not_null", "min", "max", "mean", "total", "count",
  "median", "std_dev", "variance", "range", "diff", "diff_percent", "delta", "distinct_count", "change_count",
]);

// The cell i as { isNull, num, exact }: isNull for NULL; num is the double (maybe NaN); exact is the exact text or null.
function cell(field, i) {
  switch (field.kind) {
    case "number":
    case "time":
      return { isNull: field.nulls[i] === 1, num: field.values[i], exact: null };
    case "number_text":
      return { isNull: field.nulls[i] === 1, num: field.num[i], exact: field.values[i] };
    default:
      return { isNull: field.values[i] === null || field.values[i] === undefined, num: NaN, exact: null };
  }
}

const isNumericKind = (field) => field.kind === "number" || field.kind === "number_text" || field.kind === "time";

const usable = (c) => !c.isNull && !Number.isNaN(c.num);

function pickResult(field, i) {
  if (i < 0) return { value: null, num: null };
  const c = cell(field, i);
  if (c.isNull) return { value: null, num: null };
  if (!isNumericKind(field)) return { value: field.values[i], num: null };
  const num = c.num;
  return { value: c.exact !== null ? c.exact : num, num };
}

/**
 * Reduce a field with one calculation. `value` is the Grafana value (the exact text for a number_text field and a
 * calculation that selects one value), or null. `num` is the double, or null.
 */
export function reduceField(field, calc) {
  const n = field.values.length;
  switch (calc) {
    case "count":
      return { value: n, num: n };
    case "first":
      return pickResult(field, n ? 0 : -1);
    case "last":
      return pickResult(field, n - 1);
    case "first_not_null": {
      if (!isNumericKind(field)) {
        for (let i = 0; i < n; i++) if (!cell(field, i).isNull) return pickResult(field, i);
        return pickResult(field, -1);
      }
      for (let i = 0; i < n; i++) if (usable(cell(field, i))) return pickResult(field, i);
      return pickResult(field, -1);
    }
    case "last_not_null": {
      if (!isNumericKind(field)) {
        for (let i = n - 1; i >= 0; i--) if (!cell(field, i).isNull) return pickResult(field, i);
        return pickResult(field, -1);
      }
      for (let i = n - 1; i >= 0; i--) if (usable(cell(field, i))) return pickResult(field, i);
      return pickResult(field, -1);
    }
    case "min":
    case "max": {
      if (!isNumericKind(field)) return { value: null, num: null };
      let best = -1;
      for (let i = 0; i < n; i++) {
        const c = cell(field, i);
        if (!usable(c)) continue;
        if (best < 0) {
          best = i;
          continue;
        }
        const b = cell(field, best);
        const left = c.exact !== null ? c.exact : c.num;
        const right = b.exact !== null ? b.exact : b.num;
        const cmp = compareNumericForSort(left, right);
        if (calc === "min" ? cmp < 0 : cmp > 0) best = i;
      }
      return pickResult(field, best);
    }
    case "mean":
    case "total": {
      if (!isNumericKind(field)) return calc === "total" ? { value: 0, num: 0 } : { value: null, num: null };
      let sum = 0;
      let count = 0;
      for (let i = 0; i < n; i++) {
        const c = cell(field, i);
        if (!usable(c)) continue;
        sum += c.num;
        count += 1;
      }
      if (calc === "total") return { value: sum, num: sum };
      return count ? { value: sum / count, num: sum / count } : { value: null, num: null };
    }
    case "median":
    case "variance":
    case "std_dev":
    case "range":
    case "diff":
    case "diff_percent":
    case "delta":
      return reduceStat(field, calc, n);
    case "distinct_count":
      return wholeNumber(distinctCount(field, n));
    case "change_count":
      return wholeNumber(changeCount(field, n));
    default:
      return { value: null, num: null };
  }
}

const wholeNumber = (n) => ({ value: n, num: n });
const NONE = Object.freeze({ value: null, num: null });
const some = (x) => ({ value: x, num: x });

/** The usable doubles of a field (not null, not NaN), in row order. */
function usableNumbers(field, n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    const c = cell(field, i);
    if (usable(c)) out.push(c.num);
  }
  return out;
}

// The calculations that use the doubles only (9.19): median, variance, std_dev, range, diff, diff_percent, delta.
function reduceStat(field, calc, n) {
  if (!isNumericKind(field)) return NONE;
  if (calc === "delta") return field.kind === "number" ? some(deltaOf(usableNumbers(field, n))) : NONE;
  const xs = usableNumbers(field, n);
  switch (calc) {
    case "median": {
      if (!xs.length) return NONE;
      const sorted = xs.slice().sort((a, b) => a - b);
      const mid = sorted.length >> 1;
      return some(sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2);
    }
    case "variance":
    case "std_dev": {
      if (n === 0) return NONE;
      let v = 0;
      if (xs.length) {
        const mean = xs.reduce((a, b) => a + b, 0) / xs.length;
        v = xs.reduce((a, x) => a + (x - mean) * (x - mean), 0) / xs.length;
      }
      return some(calc === "variance" ? v : Math.sqrt(v));
    }
    case "range":
      return xs.length ? some(xs.reduce((a, b) => Math.max(a, b)) - xs.reduce((a, b) => Math.min(a, b))) : NONE;
    case "diff":
      return xs.length ? some(xs[xs.length - 1] - xs[0]) : NONE;
    default: // diff_percent
      return xs.length ? some(((xs[xs.length - 1] - xs[0]) / xs[0]) * 100) : NONE;
  }
}

/** The `delta` rule of 9.19: the sum of the rises, a fall (counter reset) removes the first rise after it. */
export function deltaOf(xs) {
  let delta = 0;
  let up = true;
  for (let k = 1; k < xs.length; k++) {
    const w = xs[k] - xs[k - 1];
    if (xs[k - 1] > xs[k]) up = false;
    else if (up) {
      delta += w;
      up = true;
    } else up = true;
  }
  return delta;
}

// The key of a cell for a compare of equal values: the exact text, the text, or the double (all NaN are one key).
function equalKey(field, i) {
  const c = cell(field, i);
  if (c.isNull) return null;
  if (c.exact !== null) return `n:${c.exact}`;
  if (isNumericKind(field)) return Number.isNaN(c.num) ? "NaN" : `n:${c.num}`;
  return `s:${String(field.values[i])}`;
}

function distinctCount(field, n) {
  const seen = new Set();
  for (let i = 0; i < n; i++) {
    const k = equalKey(field, i);
    if (k !== null) seen.add(k);
  }
  return seen.size;
}

// The values, not null, that differ from the value before them (the last value that is not null; the first one has no value before it).
function changeCount(field, n) {
  let count = 0;
  let previous = null;
  for (let i = 0; i < n; i++) {
    const k = equalKey(field, i);
    if (k === null) continue;
    if (previous !== null && k !== previous) count += 1;
    previous = k;
  }
  return count;
}

/** The indices of the fields that a reduce option keeps (9.19). `reduceFields` is null or a Grafana-form pattern. */
export function selectReduceFields(fields, baseNames, reduceFields) {
  const out = [];
  if (reduceFields === null || reduceFields === undefined || reduceFields === "") {
    fields.forEach((f, i) => {
      if (f.kind === "number" || f.kind === "number_text") out.push(i);
    });
    return out;
  }
  const re = parseGrafanaPattern(reduceFields);
  if (!re) return out;
  fields.forEach((f, i) => {
    if (regexTest(re, baseNames[i])) out.push(i);
  });
  return out;
}
