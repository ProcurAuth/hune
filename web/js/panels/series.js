// Small helpers on arrays of numbers for the panel types. No DOM access.
import { QCHART_COUNT } from "../colors.js";

// The last finite value of an array, or null.
export function lastFinite(values) {
  for (let i = values.length - 1; i >= 0; i--) if (Number.isFinite(values[i])) return values[i];
  return null;
}

// The colour of a series when its field gives none (no threshold step, or no value): one token of the series palette (ID-K70),
// by the index of the series, so that several series stay apart in both themes. The engine reads the token from the page.
export { QCHART_COUNT };
export function fallbackColor(seriesIndex) {
  const i = Number.isFinite(seriesIndex) ? Math.trunc(seriesIndex) : 0;
  return `var(--qchart-${(((i % QCHART_COUNT) + QCHART_COUNT) % QCHART_COUNT) + 1})`;
}

const NICE_MANTISSAS = [1, 2, 2.5, 5, 10];

// The smallest step 1, 2, 2.5 or 5 times a power of ten that is at least `raw`. With `binary`, the smallest power of two.
export function niceStepAtLeast(raw, binary = false) {
  if (!(raw > 0)) return 1;
  if (binary) return 2 ** Math.ceil(Math.log2(raw) - 1e-9);
  const exp = Math.floor(Math.log10(raw));
  const base = 10 ** exp;
  for (const m of NICE_MANTISSAS) if (m * base >= raw * (1 - 1e-9)) return m * base;
  return 10 * base;
}

// Rounds `x` down to a multiple of `incr`. A small bias keeps a value that already sits on a multiple.
const roundDown = (x, incr) => Number((Math.floor(x / incr + 1e-9) * incr).toPrecision(14));
const roundUp = (x, incr) => Number((Math.ceil(x / incr - 1e-9) * incr).toPrecision(14));

/**
 * The automatic y range of a time series, as Grafana draws it (measured on the 22 panels of the stream dashboard).
 * Each side gets 10 % of the data range as room. The edge then rounds out to a tenth of the decade of that range.
 * The room never crosses 0: when the padded edge reaches 0 and the data does not cross 0, the edge is 0.
 * A flat series gets its own size (or 1 at 0) as room. The edges do NOT move to ticks.
 * `ends` (9.19 y axes rules 6 and 7, ID-K316): the hard ends `min`, `max` and the soft ends `softMin`, `softMax` of a scale, null when not set.
 * This is uPlot `rangeNum` with the scale of Grafana 12.4.2: a soft end replaces the rule of 0 on its side.
 * Returns `[min, max]`.
 */
export function autoYRange(dataMin, dataMax, ends) {
  const { min: hardMin = null, max: hardMax = null, softMin = null, softMax = null } = ends || {};
  const lo = hardMin !== null && softMin === null ? hardMin : dataMin;
  const hi = hardMax !== null && softMax === null ? hardMax : dataMax;
  let delta = hi - lo;
  const scalar = Math.max(Math.abs(lo), Math.abs(hi));
  if (delta < 1e-24 || Math.abs(Math.log10(scalar) - Math.log10(delta)) > 10) {
    delta = 0;
    if (lo === 0 || hi === 0) delta = 1e-24;
  }
  const mag = delta || scalar || 1e3;
  const incr = 10 ** Math.floor(Math.log10(mag)) / 10;
  const roomMin = mag * (delta === 0 ? (lo === 0 ? 0.1 : 1) : 0.1);
  const roomMax = mag * (delta === 0 ? (hi === 0 ? 0.1 : 1) : 0.1);
  let min = roundDown(lo - roomMin, incr);
  let max = roundUp(hi + roomMax, incr);
  if (softMin !== null ? lo >= softMin : lo >= 0 && min <= 0) min = softMin ?? 0;
  if (softMax !== null ? hi <= softMax : hi <= 0 && max >= 0) max = softMax ?? 0;
  if (hardMin !== null) min = Math.max(min, hardMin);
  if (hardMax !== null) max = Math.min(max, hardMax);
  if (!(max > min)) max = min + 1;
  return [min, max];
}

/**
 * The y axis of a histogram, as Grafana 12.4.2 draws it (9.19f, section 3): from 0 to the end of `autoYRange` for the lowest count above 0
 * (`low`) and the highest count (`high`). Two ticks are 30 px apart at least, or 15 px on a plot of 150 px or less. Gives `{ min, max, step, ticks }`.
 */
export function histYAxis(low, high, plotH) {
  if (!(high > 0 && plotH > 0)) return null;
  const max = autoYRange(Math.min(low, high), high)[1];
  const step = niceStepAtLeast((yTickSpace(plotH) * max) / plotH);
  const ticks = [];
  for (let k = 0; k * step <= max * (1 + 1e-9); k++) ticks.push({ v: Number((k * step).toPrecision(12)), label: tickLabel(k * step, step) });
  return { min: 0, max, step, ticks };
}

/**
 * The least distance in pixels between two y ticks, as Grafana 12.4.2 keeps it (`calculateSpace`): 30 px, or 15 px on a plot of
 * 150 px or less. The measure found it on 115 time series panels with plots of 19 to 817 px (9.19, decision ID-K311).
 */
export function yTickSpace(plotHeight) {
  return plotHeight <= 150 ? 15 : 30;
}

/**
 * The automatic y axis of a time series: the range of autoYRange and the ticks of Grafana. The step is the smallest
 * step 1, 2, 2.5 or 5 times a power of ten whose ticks are at least `minGap` pixels apart. With `binary`
 * (the unit `bytes`), the steps are powers of two, so that the ticks fall on 512 KiB, 1 MiB, 2 MiB.
 * The ticks are the multiples of the step inside the range. Returns `{ min, max, step, values }`, or null when the data
 * has no finite range.
 */
export function autoYAxis(dataMin, dataMax, plotHeight, minGap = 30, binary = false) {
  if (!Number.isFinite(dataMin) || !Number.isFinite(dataMax) || !(plotHeight > 0)) return null;
  const [min, max] = autoYRange(dataMin, dataMax);
  const step = niceStepAtLeast((minGap * (max - min)) / plotHeight, binary);
  const values = [];
  for (let k = Math.ceil(min / step - 1e-9); k * step <= max + step * 1e-9 && values.length < 200; k++) {
    const v = k * step;
    values.push(Math.abs(v) < step * 1e-9 ? 0 : Number(v.toPrecision(12)));
  }
  return { min, max, step, values };
}

/**
 * The y axis of a field that sets `min`, `max` or both (Grafana): the set edges are the edges of the axis, and an edge that is not set is the one of
 * the automatic range of the data. The ticks are those of `autoYAxis`. Returns `{ min, max, step, values }`, or null when the edges leave no range.
 */
export function boundedYAxis(lo, hi, dataMin, dataMax, plotHeight, binary = false) {
  if (!(plotHeight > 0)) return null;
  const auto = Number.isFinite(dataMin) && Number.isFinite(dataMax) ? autoYRange(dataMin, dataMax) : [0, 1];
  const min = Number.isFinite(lo) ? lo : auto[0];
  const max = Number.isFinite(hi) ? hi : auto[1];
  if (!(max > min)) return null;
  const step = niceStepAtLeast((yTickSpace(plotHeight) * (max - min)) / plotHeight, binary);
  const values = [];
  for (let k = Math.ceil(min / step - 1e-9); k * step <= max + step * 1e-9 && values.length < 200; k++) {
    const v = k * step;
    values.push(Math.abs(v) < step * 1e-9 ? 0 : Number(v.toPrecision(12)));
  }
  return { min, max, step, values };
}

/** The decimals that print the ticks of a step without noise: 2.5 gives 1, 0.05 gives 2, 10 gives 0. */
export function stepDecimals(step) {
  if (!(step > 0)) return 0;
  let d = 0;
  while (d < 12 && Math.abs(step * 10 ** d - Math.round(step * 10 ** d)) > 1e-9 * Math.max(1, step * 10 ** d)) d++;
  return d;
}

/**
 * The label of a y tick of a plain number axis, as Grafana writes it: the least digits that show the value, with no
 * trailing zero. The ticks 2, 1.5, 1, 0.5, 0 of step 0.5 read "2", "1.5", "1", "0.5", "0".
 */
export function tickLabel(v, step) {
  if (!Number.isFinite(v)) return "";
  return String(Number(v.toFixed(Math.min(stepDecimals(step), 12))));
}

/**
 * The ticks of a number axis, as Grafana draws them: the smallest step 1, 2, 2.5 or 5 times a power of ten whose ticks keep the width of their
 * labels and `room` px apart. `labelOf(value, step)` writes a label. `measure(text)` gives its width in px. The ticks are the multiples of the step
 * inside `[lo, hi]`. Returns `[{ v, label }]`.
 */
export function numberTicks(lo, hi, plotWidth, labelOf, measure, room = 11.5) {
  if (!(hi > lo) || !(plotWidth > 0)) return [];
  const make = (step) => {
    const out = [];
    for (let k = Math.ceil(lo / step - 1e-9); k * step <= hi + step * 1e-9 && out.length < 400; k++) {
      const v = Math.abs(k * step) < step * 1e-9 ? 0 : Number((k * step).toPrecision(12));
      out.push({ v, label: labelOf(v, step) });
    }
    return out;
  };
  let step = niceStepAtLeast(((hi - lo) / plotWidth) * 8);
  for (let guard = 0; guard < 40; guard++) {
    const ticks = make(step);
    const widest = ticks.reduce((m, t) => Math.max(m, measure(t.label)), 0);
    if ((plotWidth * step) / (hi - lo) >= widest + room || ticks.length <= 1) return ticks;
    step = niceStepAtLeast(step * 1.0001 * 1.2);
  }
  return make(step);
}

// ---- The y axes and the style of a series (9.19, wave 2: F16, F17) ----

export const AXIS_PLACEMENTS = Object.freeze(["auto", "left", "right", "hidden"]);

// The dash of a series (9.19, paper 10.8 V3): `dash` is 10 px and 10 px with a butt cap, `dot` is a round dot every 10 px.
export const LINE_DASHES = Object.freeze({
  solid: { dash: null, cap: null },
  dash: { dash: Object.freeze([10, 10]), cap: "butt" },
  dot: { dash: Object.freeze([0, 10]), cap: "round" },
});

const finiteOrNull = (v) => (Number.isFinite(v) ? v : null);

/**
 * The axis settings of a field (9.14): `{ unit, placement, label, softMin, softMax, min, max }`. `min` and `max` are only the ends that a
 * property sets (the ends of the data do not count). A member that is not set is null, and the placement is `auto`.
 */
export function axisSettings(field) {
  const c = (field && field.config) || {};
  const set = c.set || {};
  return {
    unit: c.unit || "none",
    placement: AXIS_PLACEMENTS.includes(set.axis_placement) ? set.axis_placement : "auto",
    label: typeof set.axis_label === "string" && set.axis_label !== "" ? set.axis_label : null,
    softMin: finiteOrNull(set.soft_min),
    softMax: finiteOrNull(set.soft_max),
    min: finiteOrNull(set.min),
    max: finiteOrNull(set.max),
    negate: set.transform === "negative_y",
  };
}

// The end of a scale that a series with `negative_y` sets: the series draws -v, so its ends swap and change sign (the axis shows the drawn values).
const flip = (v) => (v === null ? null : v === 0 ? 0 : -v);
function drawnEnds(c) {
  if (!c.negate) return c;
  return { ...c, min: flip(c.max), max: flip(c.min), softMin: flip(c.softMax), softMax: flip(c.softMin) };
}

/**
 * The scales and the axes of a chart (9.19, decision F16, paper 10.8 V1 and V2). `fields` are the fields of the series in field order.
 * `configs` are their axis settings (default: `axisSettings` of each field).
 * - `U_L` is the unit of the first series whose placement is `auto` or `left`. `left` and `right` are fixed. A series with `auto` or `hidden`
 *   goes left when its unit equals `U_L`, else right.
 * - A scale has its first series, the `label`, the hard ends `min` and `max`, and the soft ends of its first series that set each of them.
 *   The ends that a series with `negative_y` sets swap and change sign, because the axis shows the drawn values (paper 10.8, V3).
 *   It shows its axis unless every series of the scale is `hidden`.
 * - The grid follows the scale of the first series, in field order, whose axis shows (`grid`: "left", "right" or null).
 * Returns `{ sides, scales, left, right, grid }`. `sides[i]` is "left" or "right". `scales` has the left scale first. A scale with no series is absent.
 */
export function axisPlan(fields, configs) {
  const cfgs = fields.map((f, i) => drawnEnds((configs && configs[i]) || axisSettings(f)));
  const leader = cfgs.find((c) => c.placement === "auto" || c.placement === "left");
  const unitLeft = leader ? leader.unit : null;
  const sides = cfgs.map((c) => {
    if (c.placement === "left" || c.placement === "right") return c.placement;
    return leader && c.unit === unitLeft ? "left" : "right";
  });
  const build = (side) => {
    const series = [];
    sides.forEach((s, i) => { if (s === side) series.push(i); });
    if (!series.length) return null;
    const firstWith = (key) => {
      for (const i of series) if (cfgs[i][key] !== null && cfgs[i][key] !== undefined) return cfgs[i][key];
      return null;
    };
    return {
      side,
      series,
      first: series[0],
      unit: cfgs[series[0]].unit,
      label: firstWith("label"),
      min: firstWith("min"),
      max: firstWith("max"),
      softMin: firstWith("softMin"),
      softMax: firstWith("softMax"),
      shown: series.some((i) => cfgs[i].placement !== "hidden"),
    };
  };
  const left = build("left");
  const right = build("right");
  const shownFirst = cfgs.findIndex((c, i) => c.placement !== "hidden" && (sides[i] === "left" ? left : right).shown);
  return { sides, scales: [left, right].filter(Boolean), left, right, grid: shownFirst < 0 ? null : sides[shownFirst] };
}

/**
 * The style of a series (9.14, 9.19, decision F17): the value of the last override that sets a member, else the option of the panel, else the
 * default. Returns `{ draw, lineWidth, fillOpacity, dash, showPoints, pointSize, negate }`. `dash` is `solid`, `dash` or `dot`.
 */
export function seriesStyle(field, options) {
  const set = (field && field.config && field.config.set) || {};
  const o = options || {};
  const pick = (own, panel, fallback) => (own !== undefined && own !== null ? own : panel !== undefined && panel !== null ? panel : fallback);
  return {
    draw: pick(set.draw, o.draw, "line"),
    lineWidth: pick(set.line_width, o.line_width, 1),
    fillOpacity: pick(set.fill_opacity, o.fill_opacity, 0),
    dash: Object.hasOwn(LINE_DASHES, set.line_dash) ? set.line_dash : "solid",
    showPoints: pick(set.show_points, o.show_points, "auto"),
    pointSize: pick(set.point_size, o.point_size, 5),
    negate: set.transform === "negative_y",
  };
}

/** The lowest and the highest finite value of all the number fields together, or `{ min: null, max: null }`: the scale of a continuous colour mode (9.16). */
export function sharedRange(fields) {
  let min = Infinity;
  let max = -Infinity;
  for (const f of fields) {
    const src = f.kind === "number" ? f.values : f.kind === "number_text" ? f.num : null;
    if (!src) continue;
    for (let i = 0; i < src.length; i++) {
      const v = src[i];
      if (Number.isFinite(v) && !(f.nulls && f.nulls[i])) {
        if (v < min) min = v;
        if (v > max) max = v;
      }
    }
  }
  return min <= max ? { min, max } : { min: null, max: null };
}
