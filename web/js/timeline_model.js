// The model of the state timeline and the status history (9.19c, FE-31 to FE-34). Pure module: no DOM access.
import { fieldColorValue, fillOf, resolveFill } from "./colors.js";
import { makeDisplay } from "./display.js";
import { grafanaValue } from "./frame.js";
import { applyMappings } from "./mappings.js";
import { formatNumber, joinParts } from "./units.js";

/** The cap of 9.19c: a panel with more segments or cells than this value shows the note of the limit and draws nothing. */
export const MAX_TIMELINE_SEGMENTS = 10000;
/** The width of one character of a box label in CSS pixels, as Grafana 12.4.2 counts it (measured). */
export const CHAR_PX = 8.5;
/** The smallest distance in CSS pixels between two rows that lets the row names show. */
export const MIN_LABEL_PITCH = 12;

const isRow = (f) => f.kind === "number" || f.kind === "number_text" || f.kind === "text" || f.kind === "bool";
const absent = (v) => v === null || v === undefined || (typeof v === "number" && Number.isNaN(v));

// The index of the step of the thresholds that holds `v` (the rule of colors.js thresholdColor).
function stepIndex(t, v, min, max) {
  let cmp = v;
  if (t.mode === "percentage") {
    if (!(max !== min) || !Number.isFinite(min) || !Number.isFinite(max)) return 0;
    cmp = ((v - min) / (max - min)) * 100;
  }
  let at = 0;
  for (let i = 1; i < t.steps.length; i++) {
    if (t.steps[i].value !== null && t.steps[i].value !== undefined && t.steps[i].value <= cmp) at = i;
    else break;
  }
  return at;
}

/** The text of step `i`: `< v` for the base step (v is the value of the second step) and `v+` for the others. */
export function stepText(cfg, i) {
  const steps = cfg.thresholds.steps;
  const fmt = (x) => (cfg.thresholds.mode === "percentage" ? `${x}%` : joinParts(formatNumber(x, cfg.unit, cfg.decimals)));
  return i === 0 ? `< ${fmt(steps[1].value)}` : `${fmt(steps[i].value)}+`;
}

// The segments of one row. `t` has the sorted times, `s` the state of each sample (-1: absent), `to` is the end of the range.
function segmentsOf(t, s, merge, span, to) {
  const out = { t: [], e: [], s: [] };
  const idx = [];
  for (let i = 0; i < t.length; i++) if (!span || s[i] >= 0) idx.push(i);
  for (let a = 0; a < idx.length;) {
    const i = idx[a];
    let b = a + 1;
    if (s[i] >= 0 && merge) while (b < idx.length && s[idx[b]] === s[i]) b++;
    const end = b < idx.length ? t[idx[b]] : to;
    if (s[i] >= 0 && end > t[i]) {
      out.t.push(t[i]);
      out.e.push(end);
      out.s.push(s[i]);
    }
    a = b;
  }
  return out;
}

/**
 * Make the model of a frame (9.19c). `kind` is `state_timeline` or `status_history`. `fields` have a resolved config, `names` are the display
 * names, `range` is `{ from, to }` or null. Gives `{ error }`, or `{ kind, rows, states, x, step, samples, count, over }`.
 * A row has `name`, `field`, `legend`, `tooltip` and the boxes `t` (start), `e` (end) and `s` (state).
 */
export function buildTimeline({ kind, fields, names, options, zone = "browser", range = null }) {
  const ti = fields.findIndex((f) => f.kind === "time");
  if (ti < 0) return { error: "The data has no time column." };
  const picked = [];
  fields.forEach((f, i) => {
    if (i !== ti && isRow(f) && !(f.config.hide_from && f.config.hide_from.viz)) picked.push(i);
  });
  if (!picked.length) return { error: "The data has no field to draw." };
  const time = fields[ti];
  let order = [];
  let sorted = true;
  for (let i = 0; i < time.values.length; i++) {
    if (time.nulls[i]) continue;
    if (order.length && time.values[i] < time.values[order[order.length - 1]]) sorted = false;
    order.push(i);
  }
  if (!sorted) order = order.sort((a, b) => time.values[a] - time.values[b] || a - b);
  const n = order.length;
  const t = order.map((i) => time.values[i]);
  const history = kind === "status_history";
  if (history && n < 2) return { error: "A status history needs two samples at least." };
  const step = n > 1 ? t[1] - t[0] || 1 : 0;
  const t0 = t[0];
  const tn = t[n - 1];
  const to = range ? range.to : n > 1 ? tn + (tn - t0) / (n - 1) : tn + 60000;
  const x = history
    ? [range && !(range.from <= t0) ? range.from : t0 - step / 2, range && !(range.to >= tn) ? range.to : tn + step / 2]
    : [range ? range.from : t0, to];
  const states = [];
  const known = new Map();
  const rows = [];
  let count = 0;
  for (const fi of picked) {
    const field = fields[fi];
    const cfg = field.config;
    const k = rows.length;
    const display = makeDisplay(field, { zone, seriesIndex: k });
    const thresholds = !history && cfg.color && cfg.color.mode === "thresholds" && cfg.thresholds && cfg.thresholds.steps.length >= 2;
    const colorIn = (d, v, dark) => {
      const own = fillOf(d, dark);
      if (own !== null) return own;
      const c = fieldColorValue(cfg, typeof v === "number" ? v : NaN, { min: cfg.min, max: cfg.max, seriesIndex: k, dark });
      return resolveFill(c !== null ? c : cfg.thresholds.steps[0].color, dark);
    };
    const make = (v, exact) => {
      const mapping = applyMappings(cfg.mappings, v);
      if (absent(v) && !mapping) return -1;
      const d = display.valueDisplay(v, exact);
      let text = d.text;
      if (thresholds && !mapping && typeof v === "number" && !Number.isNaN(v)) text = stepText(cfg, stepIndex(cfg.thresholds, v, cfg.min, cfg.max));
      const dark = colorIn(d, v, true);
      const light = colorIn(d, v, false);
      const key = `${text}\u0000${dark}\u0000${light}`;
      let id = known.get(key);
      if (id === undefined) {
        id = states.length;
        states.push({ text, dark, light });
        known.set(key, id);
      }
      return id;
    };
    const cache = new Map();
    const s = new Array(n);
    for (let j = 0; j < n; j++) {
      const v = grafanaValue(field, order[j]);
      const key = field.kind === "number_text" && v !== null ? field.values[order[j]] : v;
      let id = cache.get(key);
      if (id === undefined) {
        id = make(v, field.kind === "number_text" && v !== null ? key : null);
        cache.set(key, id);
      }
      s[j] = id;
    }
    const row = { name: names[fi], field, legend: !(cfg.hide_from && cfg.hide_from.legend), tooltip: !(cfg.hide_from && cfg.hide_from.tooltip) };
    if (history) {
      Object.assign(row, { t: [], e: [], s: [] });
      for (let j = 0; j < n; j++) {
        if (s[j] < 0) continue;
        row.t.push(t[j]);
        row.e.push(t[j] + step);
        row.s.push(s[j]);
      }
    } else Object.assign(row, segmentsOf(t, s, options.merge_values !== false, options.span_nulls === true, to));
    count += row.t.length;
    rows.push(row);
  }
  return { kind, rows, states, x, step, samples: n, times: t, count, over: count > MAX_TIMELINE_SEGMENTS };
}

const plain = (n) => n.toLocaleString("en-US");

/** The message of a model that breaks a cap (9.19c, section 5), or "". `width` is the width of the canvas in CSS pixels. */
export function limitMessage(m, width) {
  if (m.over) {
    return m.kind === "status_history"
      ? `Too many points to draw: ${plain(m.count)} cells. The limit is ${plain(MAX_TIMELINE_SEGMENTS)}.`
      : `Too many states to draw: ${plain(m.count)} segments. The limit is ${plain(MAX_TIMELINE_SEGMENTS)}. Use a shorter time range or merge equal values.`;
  }
  const most = Math.floor(width / 2);
  return m.kind === "status_history" && m.samples > most ? `Too many points to draw: ${plain(m.samples)} rows. The most for a width of ${width} px is ${most}.` : "";
}

/** The top and the height of row `k` of `n` in a plot of height `h` with `rowHeight` in percent (9.19c, section 6). */
export function rowBox(k, n, h, rowHeight) {
  const f = n > 1 ? rowHeight / 100 : 1;
  return { y: Math.round(k * ((f * h) / n + (n > 1 ? ((1 - f) * h) / (n - 1) : 0))), h: Math.round((f * h) / n) };
}

/** The left gutter: `round(w + 20.5)` for the widest row name of width `w`, or 8 when the names do not show. */
export const gutterOf = (w, show) => (show ? Math.round(w + 20.5) : 8);

/** The width of a cell of a status history in CSS pixels, before the minimum of one device pixel. */
export const cellWidth = (stepPx, colWidth, lineWidth) => (stepPx * colWidth) / 100 - lineWidth;

/** The number of characters that fit a width. */
export const labelChars = (w) => Math.floor(w / CHAR_PX);

/** The text of a box: cut at `chars` characters with no ellipsis, never inside a pair of surrogates. `""` when it does not show. */
export function boxText(text, w, mode) {
  const chars = labelChars(w);
  if (mode === "never" || text === "" || chars < (mode === "always" ? 1 : 2)) return "";
  return text.length <= chars ? text : Array.from(text.slice(0, chars * 2)).slice(0, chars).join("");
}

/** The index of the box of a row that holds the time `at` (start <= at < end), or -1. The starts are sorted. */
export function boxAt(row, at) {
  let lo = 0;
  let hi = row.t.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (row.t[mid] <= at) lo = mid + 1;
    else hi = mid - 1;
  }
  return hi >= 0 && at < row.e[hi] ? hi : -1;
}

/** The duration text of a segment: the first form that fits (`3d 4h`, `1h 30m`, `50m`, `250ms`). 0 gives `0`. */
export function durationText(ms) {
  if (!(ms >= 0)) return "";
  let s = Math.floor(ms / 1000);
  let m = Math.floor(s / 60);
  s %= 60;
  let h = Math.floor(m / 60);
  m %= 60;
  let d = Math.floor(h / 24);
  h %= 24;
  const y = Math.floor(d / 365);
  if (y > 0) d %= 365;
  const mo = Math.floor(d / 30);
  if (mo > 0) d %= 30;
  const w = Math.floor(d / 7);
  if (w > 0) d %= 7;
  const r = Math.round((ms % 1000) * 1000) / 1000;
  const forms = y > 0 ? [[y, "y"], [mo, "mo"], [w, "w"], [d, "d"]] : mo > 0 ? [[mo, "mo"], [w, "w"], [d, "d"]] : w > 0 ? [[w, "w"], [d, "d"]]
    : d > 0 ? [[d, "d"], [h, "h"]] : h > 0 ? [[h, "h"], [m, "m"]] : m > 0 ? [[m, "m"], [s, "s"]] : s > 0 ? [[s, "s"], [r, "ms"]] : [[r, "ms"]];
  return forms.filter(([n]) => n > 0).map(([n, u]) => `${n}${u}`).join(" ") || "0";
}

/** The items of the legend (9.19c, section 9): `{ text, dark, light }`. The first row decides: steps, none for a gradient, else the states. */
export function legendItems(m) {
  const first = m.rows[0];
  if (!first) return [];
  const cfg = first.field.config;
  const mode = cfg.color ? cfg.color.mode : "thresholds";
  if (mode.startsWith("continuous_")) return [];
  if (mode === "thresholds" && cfg.thresholds.steps.length >= 2) {
    return cfg.thresholds.steps.map((st, i) => ({ text: stepText(cfg, i), dark: resolveFill(st.color, true), light: resolveFill(st.color, false) }));
  }
  const seen = new Set();
  const out = [];
  for (const row of m.rows) {
    if (!row.legend) continue;
    for (const id of row.s) {
      const st = m.states[id];
      if (st.text === "" || seen.has(st.text)) continue;
      seen.add(st.text);
      out.push(st);
    }
  }
  return out;
}
