// The cosmetic options of a series (4.18, 4.28, 9.19), as Grafana 12.4.2 draws them. Pure module: no DOM access.

import { niceStepAtLeast, yTickSpace } from "./series.js";

/** `bar_alignment` as the engine option `barAlign` (E44). */
export const BAR_ALIGN = Object.freeze({ before: -1, center: 0, after: 1 });

/** The dash `{ dash, cap }` of a property set: `dash` and `dot` draw `line_dash_array`, else 10, 10 and 0, 10; an array with no `line_dash` draws dashes. */
export function dashOf(set) {
  const style = set ? set.line_dash : undefined;
  const own = set && Array.isArray(set.line_dash_array) && set.line_dash_array.length ? set.line_dash_array : null;
  if (style === "solid" || (style === undefined && !own)) return { dash: null, cap: null };
  return style === "dot" ? { dash: own || [0, 10], cap: "round" } : { dash: own || [10, 10], cap: "butt" };
}

/** The data ends of `axis_centered_zero`: [-m, m], where m is the larger of |lo| and |hi|, or 80. */
export function centredEnds(lo, hi) {
  const m = Math.max(Math.abs(lo), Math.abs(hi)) || 80;
  return Number.isFinite(m) ? [-m, m] : [lo, hi];
}

/** The null flags with `span_nulls_ms`: a run of nulls joins (flag 0) when the values around it are less than `ms` apart; an end run stays a break. */
export function joinShortGaps(xs, nulls, ms) {
  if (!nulls || !(ms > 0)) return nulls;
  const out = Uint8Array.from(nulls);
  let prev = null;
  let start = -1;
  for (let i = 0; i < out.length; i++) {
    if (nulls[i]) {
      if (start < 0 && prev !== null) start = i;
      continue;
    }
    if (start >= 0 && xs[i] - prev < ms) out.fill(0, start, i);
    start = -1;
    prev = xs[i];
  }
  return out;
}

/**
 * The y axis of a log scale of base 2 (uPlot `rangeLog` and `log2AxisValsFilt`): from the power of 2 at or below `lo` (the lowest value above 0) to
 * the one at or above `hi`, a hard end rounded to a power of 2 replaces a data end, one value v gives v/2 to 2v, no value gives 1 to 2, and `whole`
 * (decimals 0) starts at 1. Each power has a grid line; every k-th power from the top has a label, so two labels are 30 px apart (15 px on a plot of
 * 150 px or less). The label decimals follow the linear step. Returns `{ min, max, step, ticks }`; a tick with no label has "".
 */
export function log2Axis(lo, hi, ends, plotH, label, whole = false) {
  const pow = (e) => 2 ** Math.max(-1074, Math.min(1023, e));
  let a = ends && ends.min > 0 ? pow(Math.floor(Math.log2(ends.min))) : lo > 0 ? lo : null;
  let b = ends && ends.max > 0 ? pow(Math.ceil(Math.log2(ends.max))) : hi > 0 ? hi : null;
  if (a === null || b === null || !(b >= a)) [a, b] = [1, 2];
  if (a === b) [a, b] = [a / 2, b * 2];
  const min = whole ? Math.max(1, pow(Math.floor(Math.log2(a)))) : pow(Math.floor(Math.log2(a)));
  const max = Math.max(pow(Math.ceil(Math.log2(b))), min * 2);
  const [e0, e1] = [Math.round(Math.log2(min)), Math.round(Math.log2(max))];
  const space = yTickSpace(plotH);
  const per = plotH / (e1 - e0);
  const keep = per < space ? Math.ceil(space / per) : 1;
  const step = niceStepAtLeast((space * (max - min)) / Math.max(1, plotH));
  const ticks = [];
  // Above 400 powers the grid keeps the labelled ones, so a draw stays short.
  for (let e = e1; e >= e0; e--) {
    const shown = (e1 - e) % keep === 0;
    if (shown || e1 - e0 < 400) ticks.unshift({ v: 2 ** e, label: shown ? label(2 ** e, step) : "" });
  }
  return { min, max, step, ticks };
}
