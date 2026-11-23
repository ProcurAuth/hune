// The option table of the type `heatmap` (10.3h, F43; 4.33c, 9.19e). The page does not draw this type yet: the
// table is data, and the pure rules (the grammar of the buckets, the writes of a mode and of a scale) are tested in Node (FE-78).
// The hooks are those of F31 (`heat-*`). The rows "Exponent", "Color" and "Scale" write members that the format needs in each mode.

import { displayNameRow, row, tooltipRows } from "./rows.js";

const BUCKET_MODES = [["size", "Size"], ["count", "Count"]];
const SCALES = [["linear", "Linear"], ["log2", "Log base 2"], ["log10", "Log base 10"]];
const SCHEMES = [
  ["oranges", "Oranges"], ["blues", "Blues"], ["greens", "Greens"], ["greys", "Greys"], ["purples", "Purples"], ["reds", "Reds"],
  ["yl_or_rd", "YlOrRd"], ["rd_yl_gn", "RdYlGn"], ["spectral", "Spectral"], ["viridis", "Viridis"],
];

export const X_SIZE_MAX = 31_536_000_000;
export const X_SIZE_DEFAULT = 600_000; // 10m: the value that the choice "Size" writes
export const Y_SIZE_DEFAULT = 1;
const UNITS = [["d", 86_400_000], ["h", 3_600_000], ["m", 60_000], ["s", 1000], ["ms", 1]];

/** A duration text of 4.4 (one count and one unit) in milliseconds, or undefined. */
export function parseDuration(text) {
  const m = /^([1-9][0-9]{0,8})(ms|s|m|h|d)$/.exec(text.trim());
  if (!m) return undefined;
  return Number(m[1]) * UNITS.find(([u]) => u === m[2])[1];
}

/** The duration text of a number of milliseconds: the largest unit that divides it exactly, as in 4.4. */
export function formatDuration(ms) {
  for (const [unit, size] of UNITS) if (ms % size === 0 && ms / size <= 999_999_999) return `${ms / size}${unit}`;
  return `${ms}ms`;
}

const isInt = (v, min, max) => Number.isInteger(v) && v >= min && v <= max;

// --- the grammar of the bucket values ---------------------------------------------------------------------------------------------

const WHOLE = /^[0-9]+$/;

/** The value of "X Bucket value": `count` takes an integer from 1 to 1,000 or nothing; `size` takes a duration from 1 ms to 365 d. */
export function parseXValue(text, options) {
  const t = text.trim();
  if (options.x_buckets.mode === "count") {
    if (t === "") return { value: null };
    const n = WHOLE.test(t) ? Number(t) : NaN;
    return isInt(n, 1, 1000) ? { value: n } : { error: "Use a whole number from 1 to 1000, or leave it empty." };
  }
  const ms = parseDuration(t);
  return ms !== undefined && ms <= X_SIZE_MAX ? { value: ms } : { error: "Use a duration from 1ms to 365d, such as 10m." };
}

export const formatXValue = (value, options) => (value === null || value === undefined ? "" : options.x_buckets.mode === "size" ? formatDuration(value) : String(value));

/** The value of "Y Bucket value": a log scale takes an integer from 1 to 16 or nothing; `count` an integer from 1 to 1,000 or nothing; `size` a number above 0. */
export function parseYValue(text, options) {
  const t = text.trim();
  const { mode, scale } = options.y_buckets;
  if (scale !== "linear") {
    if (t === "") return { value: null };
    const n = WHOLE.test(t) ? Number(t) : NaN;
    return isInt(n, 1, 16) ? { value: n } : { error: "Use a whole number from 1 to 16, or leave it empty." };
  }
  if (mode === "count") {
    if (t === "") return { value: null };
    const n = WHOLE.test(t) ? Number(t) : NaN;
    return isInt(n, 1, 1000) ? { value: n } : { error: "Use a whole number from 1 to 1000, or leave it empty." };
  }
  const n = /^(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?$/.test(t) ? Number(t) : NaN;
  return Number.isFinite(n) && n > 0 ? { value: n } : { error: "Use a number above 0." };
}

// A number that may be empty (`null`). `check(n, options)` gives the error text or "".
function parseNumberOrNull(check) {
  return (text, options) => {
    const t = text.trim();
    if (t === "") return { value: null };
    if (!/^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$/.test(t)) return { error: "Use a number, or leave it empty." };
    const n = Number(t);
    if (!Number.isFinite(n)) return { error: "Use a number, or leave it empty." };
    const error = check(n, options);
    return error ? { error } : { value: n };
  };
}

const logAbove0 = (n, o) => (o.y_buckets.scale !== "linear" && n <= 0 ? "A log scale needs a value above 0." : "");
const colorMinMax = (key) => (n, o) => {
  const other = o.color[key === "min" ? "max" : "min"];
  if (typeof other !== "number") return "";
  return key === "min" ? (n < other ? "" : "Start must be lower than end.") : (other < n ? "" : "End must be higher than start.");
};

// --- the writes of a mode and of a scale -------------------------------------------------------------------------------------------

// A mode switch of the X buckets changes the unit of the value, so the value goes back to the new default (`null` for a count, 10m for a size).
const xModeWrites = (mode) => ({ "options.x_buckets.value": mode === "size" ? X_SIZE_DEFAULT : null });

// A mode switch of the Y buckets keeps a value that the new mode permits. Else it writes the default of the mode.
function yModeWrites(mode, o) {
  const v = o.y_buckets.value;
  if (mode === "size") return { "options.y_buckets.value": typeof v === "number" && v > 0 ? v : Y_SIZE_DEFAULT };
  return { "options.y_buckets.value": isInt(v, 1, 1000) ? v : null };
}

/**
 * The members that a change of the scale of the Y buckets writes (4.33c, rule 2). A log scale needs the mode `size` and a split from 1 to 16 or
 * nothing. A linear scale with the mode `size` needs a number above 0, so `null` becomes 1.
 */
export function yScaleWrites(scale, o) {
  const v = o.y_buckets.value;
  if (scale !== "linear") return { "options.y_buckets.mode": "size", "options.y_buckets.value": isInt(v, 1, 16) ? v : null };
  const mode = o.y_buckets.mode;
  if (mode === "size") return { "options.y_buckets.value": typeof v === "number" && v > 0 ? v : Y_SIZE_DEFAULT };
  return { "options.y_buckets.value": isInt(v, 1, 1000) ? v : null };
}

// --- the table ------------------------------------------------------------------------------------------------------------------------

const calc = (o) => o.calculate === true;
const H = (id, hook, member, kind, label, group, rest = {}) => row(id, member, kind, label, group, { hook, ...rest });

export default {
  type: "heatmap",
  groups: [["heat", "Heatmap"], ["yaxis", "Y axis"], ["colors", "Colors"], ["cells", "Cell display"], ["tooltip", "Tooltip"], ["legend", "Legend"]],
  rows: [
    H("calculate", "heat-calculate", "options.calculate", "radio", "Calculate from data", "heat", { items: [[true, "Yes"], [false, "No"]], default: false }),
    H("x-mode", "heat-x-mode", "options.x_buckets.mode", "radio", "X Bucket", "heat", { items: BUCKET_MODES, default: "count", when: calc, whenText: "`calculate` is `true`", writes: ["options.x_buckets.value"], also: (v) => xModeWrites(v) }),
    H("x-value", "heat-x-value", "options.x_buckets.value", "text", "X Bucket value", "heat", {
      nullable: true, default: null, when: calc, whenText: "`calculate` is `true`", parse: parseXValue, format: formatXValue, hint: "Count: 1 to 1,000. Size: a duration such as 10m.",
    }),
    H("y-mode", "heat-y-mode", "options.y_buckets.mode", "radio", "Y Bucket", "heat", {
      items: BUCKET_MODES, default: "count", when: (o) => calc(o) && o.y_buckets.scale === "linear", whenText: "`calculate` is `true` and `y_buckets.scale` is `linear`", writes: ["options.y_buckets.value"],
      also: (v, o) => yModeWrites(v, o),
    }),
    H("y-scale", "heat-y-scale", "options.y_buckets.scale", "radio", "Y Bucket scale", "heat", {
      items: SCALES, default: "linear", when: calc, whenText: "`calculate` is `true`", writes: ["options.y_buckets.mode", "options.y_buckets.value"], also: (v, o) => yScaleWrites(v, o),
    }),
    H("y-value", "heat-y-value", "options.y_buckets.value", "number", "Y Bucket value", "heat", {
      nullable: true, default: null, when: calc, whenText: "`calculate` is `true`", parse: parseYValue, hint: "Count: 1 to 1,000. Size: a number above 0. Log scale: 1 to 16.",
    }),
    H("y-axis", "heat-y-axis", "options.y_axis", "radio", "Placement", "yaxis", { items: [["left", "Left"], ["hidden", "Hidden"]], default: "left" }),
    H("y-reverse", "heat-y-reverse", "options.y_reverse", "switch", "Reverse", "yaxis", { default: false }),
    H("y-min", "heat-y-min", "options.y_min", "number", "Min value", "yaxis", { nullable: true, default: null, when: calc, whenText: "`calculate` is `true`", parse: parseNumberOrNull(logAbove0) }),
    H("y-max", "heat-y-max", "options.y_max", "number", "Max value", "yaxis", { nullable: true, default: null, when: calc, whenText: "`calculate` is `true`", parse: parseNumberOrNull(logAbove0) }),
    H("color-mode", "heat-color-mode", "options.color.mode", "radio", "Mode", "colors", { items: [["scheme", "Scheme"], ["opacity", "Opacity"]], default: "scheme" }),
    H("color-scheme", "heat-scheme", "options.color.scheme", "select", "Scheme", "colors", { items: SCHEMES, default: "oranges", when: (o) => o.color.mode === "scheme", whenText: "`color.mode` is `scheme`" }),
    H("color-fill", "heat-fill", "options.color.fill", "custom", "Color", "colors", { default: "dark-orange", colour: true, when: (o) => o.color.mode === "opacity", whenText: "`color.mode` is `opacity`" }),
    H("color-scale", "heat-scale", "options.color.scale", "radio", "Scale", "colors", { items: [["exponential", "Exponential"], ["linear", "Linear"]], default: "exponential", when: (o) => o.color.mode === "opacity", whenText: "`color.mode` is `opacity`" }),
    H("color-exponent", "heat-exponent", "options.color.exponent", "number", "Exponent", "colors", {
      min: 0.1, max: 2, step: 0.1, default: 0.5, when: (o) => o.color.mode === "opacity" && o.color.scale === "exponential", whenText: "`color.mode` is `opacity` and `color.scale` is `exponential`", parse: parseExponent,
    }),
    H("color-steps", "heat-steps", "options.color.steps", "slider", "Steps", "colors", { min: 2, max: 128, default: 64 }),
    H("color-reverse", "heat-reverse", "options.color.reverse", "switch", "Reverse", "colors", { default: false }),
    H("color-min", "heat-min", "options.color.min", "number", "Start color scale from value", "colors", { nullable: true, default: null, parse: parseNumberOrNull(colorMinMax("min")) }),
    H("color-max", "heat-max", "options.color.max", "number", "End color scale at value", "colors", { nullable: true, default: null, parse: parseNumberOrNull(colorMinMax("max")) }),
    H("cell-gap", "heat-gap", "options.cell_gap", "slider", "Cell gap", "cells", { min: 0, max: 25, default: 1 }),
    H("hide-le", "heat-hide-le", "options.hide_le", "number", "Hide cells with values <=", "cells", { nullable: true, default: 0, parse: parseNumberOrNull(() => "") }),
    H("hide-ge", "heat-hide-ge", "options.hide_ge", "number", "Hide cells with values >=", "cells", { nullable: true, default: null, parse: parseNumberOrNull(() => "") }),
    ...tooltipRows({ sort: false, hideZeros: false }),
    H("show-legend", "heat-legend", "options.show_legend", "switch", "Show legend", "legend", { default: true }),
    displayNameRow(),
  ],
};

// The exponent is 0.1 to 2 with one decimal at most (4.33c).
function parseExponent(text) {
  const t = text.trim();
  const n = /^(?:[0-9]+(?:\.[0-9]+)?|\.[0-9]+)$/.test(t) ? Number(t) : NaN;
  const tenths = n * 10;
  return n >= 0.1 && n <= 2 && Math.abs(tenths - Math.round(tenths)) < 1e-9 ? { value: n } : { error: "Use a number from 0.1 to 2 with one decimal." };
}
