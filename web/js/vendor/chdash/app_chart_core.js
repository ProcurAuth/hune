// Shared canvas chart engine (uPlot-style). It uses columnar Float64Array data and per-pixel min/max decimation.
// It has two stacked canvases. The plot canvas is redrawn on data, size, zoom and theme changes.
// The cursor overlay canvas is redrawn on every pointer move. The engine also has a DOM tooltip and legend.
// One engine serves every chart of the app. The Query result chart uses it first.
// The other charts move to it one by one. See each consumer for its migration notes.
//
//   const chart = ns.chartCore.create(hostEl, {
//     xKind: "time" | "number" | "index" | "category",
//     xs: Float64Array,                 // ascending. A category axis uses 0..n-1.
//     categories: ["a", "b"],          // labels of the category axis
//     series: [{ id, label, color, values: Float64Array, nulls: Uint8Array | null, group }],
//     type: "line" | "area" | "bar" | "points",
//     stack: false,                     // area and bar: cumulative per group
//   });
//   chart.setData({ ... }), chart.update({ type, stack }), chart.setZoom(lo, hi),
//   chart.resetZoom(), chart.setHidden(ids), chart.destroy().
//
// values[i] is NaN when the series has no value at xs[i]. nulls[i] = 1 marks a NULL.
// The line breaks at a NULL. A NaN without the flag is a missing row.
// The line connects across a missing row. nulls === null means that every NaN breaks the line.
//
// The legend is the list of the series under the plot. The "Values" mode shows a table of Total, Min, Max, Mean and Last.
// The legend is the same on every page. hune: E41, the host sets the mode (setLegendMode); the engine keeps no preference.
// A series can have `weights`, one per point: the calls behind an average, or the requests behind a rate.
// Its Mean is then weighted. A chart whose values do not add up omits the Total (`legendColumns`).
// hune: E41. No legend that filters (`legendClick`), no totals legend and no button of the mode: Hune never asks for them.
// hune: E40. No alignment groups (`alignKey`) and no aligned y axes: Hune never sets alignKey.

import { dom } from "./app_dom.js";
// hune: E36. No import of app_format.js: the engine read only format.range, for a time axis of buckets (bucketMs). Hune never sets bucketMs.
// hune: E37. No import of app_palette.js: the engine read only its WebAssembly colour kernel (palette.batch). v1 has no kernel.
// hune: E41. No import of app_state.js: the engine read it only for the preference of the legend mode.
import { uiState } from "./app_ui_state.js";
import { util } from "./app_util.js";

const { $ } = dom;

const DEFAULT_HEIGHT = 300;
// hune: the axis text is 12 px, as in the Grafana look of the dashboard (9.24, wave G).
const FONT_SIZE = 12;
const Y_TICK_SPACE = 34;
// hune: 11.5 px of room around a label of the x axis (Grafana, measured); the engine had 22.
const X_TICK_GAP = 11.5;
const X_LABEL_GAP = 6;
const X_CONTEXT_GAP = 10;
// hune: the plot starts 8 px under the top edge.
const PAD_TOP = 8;
// hune: E19. The engine annotations are removed: Hune never sets the option `annotations` (engine-patches.md).
// hune: the plot ends 8 px before the right edge.
const PAD_RIGHT = 8;
// hune: E13, F16. A y axis takes the width of its widest label plus 9 px (rounded up). The labels end 5 px before the plot, or start 5 px after it.
// A title of an axis takes 28 px. A side with no axis keeps the margin of 8 px (Grafana 12.4.2, measured: paper 10.8, V2).
const Y_AXIS_PAD = 9;
const Y_LABEL_GAP = 5;
const Y_TITLE_W = 28;
const Y_TITLE_CENTRE = 12;
// hune: the x axis has no tick marks.
const TICK_LEN = 0;
const DRAG_MIN_PX = 4;
const POINTS_AUTO_SPACING = 12;
const MAX_CATEGORY_CHARS = 18;
// hune: E31. The longest start of an axis title that fitTitle measures.
const TITLE_MAX_CHARS = 256;

// Work counters of every chart on the page. They are test and profiling hooks.
// Use ns.chartCore.counters() and resetCounters(). The counters only increase.
const COUNTER_NAMES = ["setData", "draws", "drawMs", "layoutReads", "traces", "decimations", "tracedPoints",
  "extentScans", "stackBuilds", "legendBuilds", "legendUpdates", "allocBytes", "summarised", "textMeasures"];
const counters = {};
function resetCounters() { for (const name of COUNTER_NAMES) counters[name] = 0; }
resetCounters();

// --- Numbers ----------------------------------------------------------------

const COMPACT_UNITS = [[1e12, "T"], [1e9, "B"], [1e6, "M"], [1e3, "K"]];

function compactUnitFor(maxAbs) {
  for (const [factor, suffix] of COMPACT_UNITS) if (maxAbs >= (factor === 1e3 ? 1e4 : factor)) return { factor, suffix };
  return { factor: 1, suffix: "" };
}

// Decimals that write the step exactly: 2.5 -> 1, 0.25 -> 2, 500 -> 0.
function decimalsFor(step) {
  const abs = Math.abs(step);
  if (!(abs > 0) || !Number.isFinite(abs)) return 0;
  for (let d = 0; d < 10; d++) {
    const scaled = abs * 10 ** d;
    if (Math.abs(scaled - Math.round(scaled)) <= 1e-7 * Math.max(1, scaled)) return d;
  }
  return 10;
}

function groupThousands(intText) {
  return intText.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

// Ticks share one unit and one decimal count: 0, 2.5K, 5K, 7.5K.
function formatTick(value, step, unit) {
  if (value === 0) return "0";
  const scaled = value / unit.factor;
  const text = scaled.toFixed(decimalsFor(step / unit.factor));
  const [whole, frac] = text.split(".");
  const out = `${unit.suffix ? whole : groupThousands(whole)}${frac ? `.${frac}` : ""}`;
  return `${/^-0(?:\.0*)?$/.test(out) ? "0" : out}${unit.suffix}`;
}

// Exact value. Every digit that JavaScript round-trips is shown, grouped (12,345.678901).
function formatExact(value) {
  if (!Number.isFinite(value)) return "NULL";
  const abs = Math.abs(value);
  if (abs !== 0 && (abs < 1e-6 || abs >= 1e21)) return String(value);
  const text = String(value);
  const neg = text[0] === "-";
  const [whole, frac] = (neg ? text.slice(1) : text).split(".");
  return `${neg ? "-" : ""}${groupThousands(whole)}${frac ? `.${frac}` : ""}`;
}

// Readable value. Integers are grouped. Fractions have 12 significant digits.
function formatValue(value) {
  if (!Number.isFinite(value)) return "NULL";
  if (Number.isInteger(value)) return formatExact(value);
  const abs = Math.abs(value);
  if (abs !== 0 && (abs < 1e-6 || abs >= 1e15)) return value.toPrecision(6);
  return formatExact(Number(value.toPrecision(12)));
}

// The "nice" steps of a decade: 1, 2, 2.5 and 5 (then 10, the next decade's 1).
const NICE_MULTIPLES = [1, 2, 2.5, 5];

function niceStep(rawStep) {
  if (!(rawStep > 0) || !Number.isFinite(rawStep)) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(rawStep));
  const normalized = rawStep / magnitude;
  return (NICE_MULTIPLES.find((multiple) => normalized <= multiple) || 10) * magnitude;
}

// Round the tick values inside [lo, hi]. The domain itself is not snapped.
function linearTicks(lo, hi, targetCount, integer = false) {
  let step = niceStep((hi - lo) / Math.max(1, targetCount));
  if (integer) step = Math.max(1, Math.round(step));
  const values = [];
  const first = Math.ceil(lo / step - 1e-9);
  for (let k = first; k * step <= hi + step * 1e-9 && values.length < 100; k++) {
    const v = k * step;
    values.push(Math.abs(v) < step * 1e-9 ? 0 : Number(v.toPrecision(12)));
  }
  return { step, values };
}

// --- Time ---------------------------------------------------------------------------------------------------------
// hune: the ticks and the texts of a time axis follow a zone (opts.zone), not only the zone of the browser (milestone F, F10; engine
// edit E7 of docs/engine-patches.md). A zone is a clock: { parts(ms) -> {y, mo (1 to 12), d, h, mi, s, ms},
// epoch(y, mo, d, h, mi, s) -> ms }. The fields of epoch may overflow (day 0, month 13). Without a zone the clock is the one of the browser.

const LOCAL_CLOCK = {
  parts(ms) {
    const d = new Date(ms);
    return { y: d.getFullYear(), mo: d.getMonth() + 1, d: d.getDate(), h: d.getHours(), mi: d.getMinutes(), s: d.getSeconds(), ms: d.getMilliseconds() };
  },
  epoch(y, mo, d, h = 0, mi = 0, s = 0) {
    return new Date(y, mo - 1, d, h, mi, s).getTime();
  },
};

const SECOND_MS = 1000, MINUTE_MS = 60000, HOUR_MS = 3600000, DAY_MS = 86400000;
// hune: the steps of Grafana: 2 minutes is not a step, and 4 and 8 hours are (measured on Grafana 12.4.2).
const SUB_DAY_STEPS_MS = [
  1, 2, 5, 10, 20, 50, 100, 200, 500,
  SECOND_MS, 2 * SECOND_MS, 5 * SECOND_MS, 10 * SECOND_MS, 15 * SECOND_MS, 30 * SECOND_MS,
  MINUTE_MS, 5 * MINUTE_MS, 10 * MINUTE_MS, 15 * MINUTE_MS, 30 * MINUTE_MS,
  HOUR_MS, 2 * HOUR_MS, 3 * HOUR_MS, 4 * HOUR_MS, 6 * HOUR_MS, 8 * HOUR_MS, 12 * HOUR_MS,
];
const DAY_STEPS = [1, 2, 3, 4, 5, 6, 7, 14];
const MONTH_STEPS = [1, 2, 3, 6];
const YEAR_STEPS = [1, 2, 5, 10, 20, 50, 100, 1000];
const MONTH_NAMES = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const pad2 = (v) => String(v).padStart(2, "0");
const pad3 = (v) => String(v).padStart(3, "0");

function midnightOf(ms, K) {
  const p = K.parts(ms);
  return K.epoch(p.y, p.mo, p.d);
}

// hune: E42. No dateText: only the option xDateOnly used it.

// Example: 2026-09-30 14:05:00.000. A fractionDigits value of 6 adds the microseconds.
function formatInstant(ms, fractionDigits = 3, K = LOCAL_CLOCK) {
  if (!Number.isFinite(ms)) return "—";
  const whole = Math.floor(ms);
  const p = K.parts(whole);
  let text = `${p.y}-${pad2(p.mo)}-${pad2(p.d)} ${pad2(p.h)}:${pad2(p.mi)}:${pad2(p.s)}`;
  if (fractionDigits > 0) {
    text += `.${pad3(p.ms)}`;
    if (fractionDigits > 3) text += String(Math.min(999, Math.round((ms - whole) * 1000))).padStart(3, "0").slice(0, fractionDigits - 3);
  }
  return text;
}

function formatDuration(ms) {
  if (!(ms >= 0)) return "";
  if (ms < 1) return `${formatValue(Number((ms * 1000).toPrecision(4)))} µs`;
  if (ms < SECOND_MS) return `${formatValue(Number(ms.toPrecision(4)))} ms`;
  const parts = [];
  let rest = Math.round(ms);
  const units = [[DAY_MS, "d"], [HOUR_MS, "h"], [MINUTE_MS, "m"], [SECOND_MS, "s"]];
  for (const [size, suffix] of units) {
    if (rest >= size || (suffix === "s" && !parts.length)) {
      const count = suffix === "s" ? rest / size : Math.floor(rest / size);
      parts.push(suffix === "s" && !Number.isInteger(count) && parts.length === 0 ? `${count.toFixed(3).replace(/0+$/, "")}s` : `${Math.floor(count)}${suffix}`);
      rest -= Math.floor(count) * size;
    }
    if (parts.length === 2) break;
  }
  return parts.join(" ");
}

function utcOffsetText(ms) {
  const minutes = -new Date(Number.isFinite(ms) ? ms : Date.now()).getTimezoneOffset();
  if (minutes === 0) return "UTC";
  const sign = minutes < 0 ? "-" : "+";
  const abs = Math.abs(minutes);
  return `UTC${sign}${pad2(Math.floor(abs / 60))}:${pad2(abs % 60)}`;
}

function subDayTicks(startMs, endMs, stepMs, K) {
  const ticks = [];
  let day = midnightOf(startMs, K);
  for (let guard = 0; day <= endMs && guard < 4000; guard++) {
    const midnight = day;
    const p = K.parts(midnight);
    const next = K.epoch(p.y, p.mo, p.d + 1);
    day = next;
    if (stepMs >= HOUR_MS) {
      // Wall-clock hours, such as 00:00, 06:00 and 12:00, even on DST days.
      for (let hour = 0; hour < 24; hour += stepMs / HOUR_MS) {
        const t = K.epoch(p.y, p.mo, p.d, hour);
        if (t >= startMs && t <= endMs && (!ticks.length || ticks[ticks.length - 1] < t)) ticks.push(t);
      }
      continue;
    }
    const first = midnight + Math.ceil(Math.max(0, startMs - midnight) / stepMs) * stepMs;
    for (let t = first; t < next && t <= endMs && ticks.length < 400; t += stepMs) ticks.push(t);
  }
  return ticks;
}

// hune: the day ticks start at the first midnight after the start of the range, then every `everyDays` days (Grafana).
function dayTicks(startMs, endMs, everyDays, K) {
  const ticks = [];
  let t = midnightOf(startMs, K);
  if (t < startMs) {
    const p = K.parts(t);
    t = K.epoch(p.y, p.mo, p.d + 1);
  }
  for (let guard = 0; t <= endMs && guard < 5000; guard++) {
    ticks.push(t);
    const p = K.parts(t);
    t = K.epoch(p.y, p.mo, p.d + everyDays);
  }
  return ticks;
}

function monthTicks(startMs, endMs, everyMonths, K) {
  const ticks = [];
  const start = K.parts(startMs);
  let y = start.y, mo = start.mo;
  for (let guard = 0; guard < 5000; guard++) {
    const t = K.epoch(y, mo, 1);
    if (t > endMs) break;
    if (t >= startMs && (y * 12 + mo - 1) % everyMonths === 0) ticks.push(t);
    mo += 1;
    if (mo > 12) { mo = 1; y += 1; }
  }
  return ticks;
}

function yearTicks(startMs, endMs, everyYears, K) {
  const ticks = [];
  const first = K.parts(startMs).y;
  for (let y = first; y <= first + 100000 && ticks.length < 200; y++) {
    const t = K.epoch(y, 1, 1);
    if (t > endMs) break;
    if (t >= startMs && y % everyYears === 0) ticks.push(t);
  }
  return ticks;
}

// hune: a step of one minute writes the seconds (12:27:00), as Grafana does. A range over one day writes the date before the clock (11/10 04:00).
function clockText(ms, unit, K, dated) {
  const p = K.parts(ms);
  const hm = `${pad2(p.h)}:${pad2(p.mi)}`;
  const date = dated ? `${pad2(p.mo)}/${pad2(p.d)} ` : "";
  if (unit === "minute") return date + hm;
  const hms = `${hm}:${pad2(p.s)}`;
  return unit === "second" ? date + hms : `${date}${hms}.${pad3(p.ms)}`;
}

// The day text of a day tick, and of a tick at midnight: the month and the day, "11/10" (Grafana).
const dayLabel = (ms, K = LOCAL_CLOCK) => {
  const p = K.parts(ms);
  return `${pad2(p.mo)}/${pad2(p.d)}`;
};

// Calendar-aligned ticks. The function uses the smallest step whose labels stay apart, like Grafana: the room of a label is its width
// (in the format of its step) and 11.5 px. Measured on Grafana 12.4.2 (steps of 5 minutes at 48.9 px, but not at 45.1 px).
// Each tick is { v, label, date, year }: the label and its context.
// The date is the day under clock times. The year is the year under clock times, days and months.
// layoutXLabels decides where the context line shows.
function timeTicks(startMs, endMs, plotWidthPx, measure, K = LOCAL_CLOCK) {
  const span = Math.max(1e-3, endMs - startMs);
  const width = Math.max(60, plotWidthPx);
  // hune: a range of more than 24 hours writes the date before the clock; a shorter one writes the clock only, also at midnight.
  const dated = span > DAY_MS;
  const fits = (count, sample) => count <= 80 && width / Math.max(1, count) >= measure(sample) + X_TICK_GAP;
  const make = (ticks, labelOf) => ticks.map((t) => ({ v: t, label: labelOf(t), date: "", year: "" }));
  for (const step of SUB_DAY_STEPS_MS) {
    const unit = step < SECOND_MS ? "milli" : step <= MINUTE_MS ? "second" : "minute";
    const base = unit === "milli" ? "00:00:00.000" : unit === "second" ? "00:00:00" : "00:00";
    const sample = dated && unit !== "milli" ? `00/00 ${base}` : base;
    if (!fits(span / step, sample)) continue;
    return make(subDayTicks(startMs, endMs, step, K), (t) => clockText(t, unit, K, dated && unit !== "milli"));
  }
  for (const days of DAY_STEPS) {
    if (!fits(span / (days * DAY_MS), "00/00")) continue;
    return make(dayTicks(startMs, endMs, days, K), (t) => dayLabel(t, K));
  }
  const monthMs = 30.44 * DAY_MS;
  for (const months of MONTH_STEPS) {
    if (!fits(span / (months * monthMs), "Sep")) continue;
    return make(monthTicks(startMs, endMs, months, K), (t) => MONTH_NAMES[K.parts(t).mo - 1]);
  }
  for (const years of YEAR_STEPS) {
    if (!fits(span / (years * 365.25 * DAY_MS), "2026") && years !== YEAR_STEPS[YEAR_STEPS.length - 1]) continue;
    return yearTicks(startMs, endMs, years, K).map((t) => ({ v: t, label: String(K.parts(t).y), date: "", year: "" }));
  }
  return [];
}

// hune: E25. The box of a label of width w px turned by deg degrees (not 0) about its anchor. A positive angle turns the text clockwise
// and puts its start at the anchor. A negative angle turns it counter-clockwise and puts its end at the anchor (Grafana). The text is FONT_SIZE px high.
// h: the height of the box under the anchor line. a, b: the px of the box before and after the anchor. step: the room that the label
// takes for a neighbour with the same angle: its width, or less when the labels lie side by side (a line of text is FONT_SIZE px thick).
function turnedBox(w, deg) {
  const r = Math.abs(deg) * Math.PI / 180, s = Math.sin(r), c = Math.cos(r), k = FONT_SIZE / 2 * s;
  return { h: w * s + FONT_SIZE * c, a: deg > 0 ? k : w * c + k, b: deg > 0 ? w * c + k : k, step: Math.min(w * c + FONT_SIZE * s, FONT_SIZE / s) };
}

// The x labels to draw, from left to right. The function is pure: the tests run it on made-up ticks.
// A tick label shows where it keeps X_LABEL_GAP px from the one before. Otherwise, the label is skipped.
// Under each label, on a second line, its context shows: "Oct 3" under clock times, "2026" under days or months.
// The first label shows its context. The context also shows where it changes, and the year shows only where the year changes.
// For example: "Oct 2 2026", then "Oct 3" at 00:00.
// A context that is closer than X_CONTEXT_GAP px to the context shown before is skipped too. No label reads under the previous date.
// The function then waits for the next one. The exception: the first label's context only dates the axis, so it yields to the change.
//   ticks [{ v, label, date, year }]; xOf(v) -> px; [lo, hi] the plot's px
//   span; width the canvas width (labels stay inside it); measure(text) and
//   measureContext(text) the widths at the label and the context fonts.
//   -> [{ v, label, x, left, right, context, cx, cleft, cright }]
// hune: E25. rot: the rotation of the labels in degrees (default 0, the picture of the engine).
function layoutXLabels(ticks, xOf, lo, hi, width, measure, measureContext, rot = 0) {
  const out = [];
  const centre = (x, w) => Math.max(w / 2 + 1, Math.min(width - w / 2 - 1, x));
  let lastRight = -Infinity;
  let shown = null; // the label whose context was shown last
  let first = null; // the first context shown, while it may give way
  for (const t of ticks) {
    const x = xOf(t.v);
    if (!(x >= lo - 0.5 && x <= hi + 0.5)) continue;
    const w = measure(t.label);
    // hune: E25. A turned label (rot degrees) is not centred on its tick: it spans box.a px before the anchor and box.b px after it.
    // Its neighbours keep box.step apart, and the anchor stays where the whole label is inside the canvas.
    const box = rot ? turnedBox(w, rot) : { a: w / 2, b: w / 2, step: w };
    const lx = Math.max(box.a + 1, Math.min(width - box.b - 1, x));
    if (lx - box.step / 2 < lastRight + X_LABEL_GAP) continue;
    const item = { v: t.v, label: t.label, x: lx, left: lx - box.a, right: lx + box.b, context: "", cx: 0, cleft: 0, cright: 0 };
    const date = t.date || "", year = t.year || "";
    if ((!date && !year) || (shown && date === shown.date && year === shown.year)) {
      out.push(item);
      lastRight = lx + box.step / 2;
      continue;
    }
    let text = contextText(date, year, shown ? shown.year : "");
    let cw = measureContext(text), cx = centre(x, cw);
    if (shown && cx - cw / 2 < shown.item.cright + X_CONTEXT_GAP) {
      if (shown.item !== first) continue;
      first.context = "";
      first.cx = first.cleft = first.cright = 0;
      text = contextText(date, year, "");
      cw = measureContext(text);
      cx = centre(x, cw);
    }
    out.push(item);
    lastRight = lx + box.step / 2;
    first = shown ? null : item;
    item.context = text;
    item.cx = cx;
    item.cleft = cx - cw / 2;
    item.cright = cx + cw / 2;
    shown = { item, date, year };
  }
  return out;
}

function contextText(date, year, shownYear) {
  if (!date) return year;
  return year && year !== shownYear ? `${date} ${year}` : date;
}

// hune: E40. The aligned y axes (alignedAxis, its steps and its units) are removed: only a chart with an `alignKey` used them.

// --- Colours ----------------------------------------------------------------

function parseColor(text) {
  const value = String(text || "").trim();
  let m = /^rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:\s*[,/]\s*([\d.]+%?))?\s*\)$/.exec(value);
  if (m) {
    const a = m[4] == null ? 1 : m[4].endsWith("%") ? Number(m[4].slice(0, -1)) / 100 : Number(m[4]);
    return { r: Number(m[1]), g: Number(m[2]), b: Number(m[3]), a };
  }
  m = /^color\(srgb\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*\/\s*([\d.]+))?\s*\)$/.exec(value);
  if (m) return { r: Number(m[1]) * 255, g: Number(m[2]) * 255, b: Number(m[3]) * 255, a: m[4] == null ? 1 : Number(m[4]) };
  m = /^#([0-9a-f]{6})$/i.exec(value);
  if (m) {
    const n = parseInt(m[1], 16);
    return { r: n >> 16, g: (n >> 8) & 255, b: n & 255, a: 1 };
  }
  return { r: 128, g: 128, b: 128, a: 1 };
}

// parseColor of many texts.
// hune: E37. No WebAssembly kernel (palette.batch) and no rgbaBatch: each text goes to parseColor.
function parseColors(texts) {
  return Array.from(texts).map(parseColor);
}

const rgba = (c, alpha = 1) => `rgba(${Math.round(c.r)}, ${Math.round(c.g)}, ${Math.round(c.b)}, ${+(c.a * alpha).toFixed(3)})`;
const luminance = (c) => (0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b) / 255;

// --- Shared state: theme changes and the cursor sync groups -----------------

const live = new Set();
const syncGroups = new Map();
let themeWatch = null;

function watchTheme() {
  if (themeWatch) return;
  const invalidate = () => { for (const chart of live) chart.themeChanged(); };
  const observer = new MutationObserver(invalidate);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  const media = window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;
  if (media && media.addEventListener) media.addEventListener("change", invalidate);
  // Axis labels that were measured before a web font loaded use the fallback face.
  if (document.fonts && document.fonts.addEventListener) document.fonts.addEventListener("loadingdone", invalidate);
  themeWatch = { observer, media, invalidate };
}

function lowerBound(xs, value, lo = 0, hi = xs.length) {
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (xs[mid] < value) lo = mid + 1; else hi = mid;
  }
  return lo;
}

function upperBound(xs, value, lo = 0, hi = xs.length) {
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (xs[mid] <= value) lo = mid + 1; else hi = mid;
  }
  return lo;
}

// --- Block summaries of long series --------------------------------------------
//
// A series with BLOCK_MIN_POINTS values or more is summarised per BLOCK points.
// The summary holds the min, max, sum and count of the finite values. It also says whether all values are finite.
// A summary grows with the series when rows are only appended (setData({ append: true })). The blocks already summed stay.
// Extents, decimation and the legend values read whole blocks where a range covers them.
// So, a draw costs O(n / BLOCK + columns * BLOCK) instead of O(n).
const BLOCK = 64;
const BLOCK_SHIFT = 6;
const BLOCK_MIN_POINTS = 1 << 14;

function newSummary() {
  return { values: null, n: 0, mins: new Float64Array(0), maxs: new Float64Array(0), sums: new Float64Array(0), counts: new Uint32Array(0), clean: new Uint8Array(0) };
}

// Summarise the values from block sum.n to n. The last, partial block is summed again.
function extendSummary(sum, values, n) {
  const blocks = (n + BLOCK - 1) >> BLOCK_SHIFT;
  if (sum.mins.length < blocks) {
    const cap = Math.max(64, blocks * 2);
    const grow = (Type, a) => { const next = new Type(cap); next.set(a); return next; };
    sum.mins = grow(Float64Array, sum.mins);
    sum.maxs = grow(Float64Array, sum.maxs);
    sum.sums = grow(Float64Array, sum.sums);
    sum.counts = grow(Uint32Array, sum.counts);
    sum.clean = grow(Uint8Array, sum.clean);
    counters.allocBytes += cap * 29;
  }
  for (let b = sum.n >> BLOCK_SHIFT; b < blocks; b++) {
    const a = b << BLOCK_SHIFT, e = Math.min(n, a + BLOCK);
    let min = Infinity, max = -Infinity, total = 0, count = 0;
    for (let i = a; i < e; i++) {
      const v = values[i];
      if (v !== v) continue;
      if (v < min) min = v;
      if (v > max) max = v;
      total += v;
      count++;
    }
    sum.mins[b] = min;
    sum.maxs[b] = max;
    sum.sums[b] = total;
    sum.counts[b] = count;
    sum.clean[b] = count === e - a && Number.isFinite(total) ? 1 : 0;
  }
  counters.summarised += Math.max(0, n - ((sum.n >> BLOCK_SHIFT) << BLOCK_SHIFT));
  sum.values = values;
  sum.n = n;
  return sum;
}

// Min / max / sum / count of the finite values of [v0, v1).
function summaryRange(sum, values, v0, v1) {
  let min = Infinity, max = -Infinity, total = 0, count = 0;
  let i = v0;
  const scan = (end) => {
    for (; i < end; i++) {
      const v = values[i];
      if (v !== v) continue;
      if (v < min) min = v;
      if (v > max) max = v;
      total += v;
      count++;
    }
  };
  const firstFull = (v0 + BLOCK - 1) >> BLOCK_SHIFT, lastFull = v1 >> BLOCK_SHIFT;
  if (firstFull >= lastFull) {
    scan(v1);
  } else {
    scan(firstFull << BLOCK_SHIFT);
    for (let b = firstFull; b < lastFull; b++) {
      if (sum.mins[b] < min) min = sum.mins[b];
      if (sum.maxs[b] > max) max = sum.maxs[b];
      total += sum.sums[b];
      count += sum.counts[b];
    }
    i = lastFull << BLOCK_SHIFT;
    scan(v1);
    counters.extentScans += (firstFull << BLOCK_SHIFT) - v0 + v1 - (lastFull << BLOCK_SHIFT) + (lastFull - firstFull);
    return { min, max, total, count };
  }
  counters.extentScans += v1 - v0;
  return { min, max, total, count };
}

// util.escapeHtml is the only escaper. null prints as "".
const esc = (value) => util.escapeHtml(value == null ? "" : value);

// hune: E42. No bridgeGaps: Hune never calls it.

// --- Extensions (module level) ---------------------------------------------------


// The function makes 1-2-5 ticks per decade inside [lo, hi]. lo must be above 0.
// It keeps only the powers of ten when there are more than 7 ticks. It makes linear ticks when the range spans less than 8x.
function logTicks(lo, hi) {
  if (!(lo > 0) || !(hi > lo)) return [];
  if (hi / lo < 8) return linearTicks(lo, hi, 4).values.filter((v) => v > 0);
  const ticks = [];
  for (let k = Math.floor(Math.log10(lo)); k <= Math.ceil(Math.log10(hi)); k++) {
    for (const m of [1, 2, 5]) {
      const v = Number((m * 10 ** k).toPrecision(12));
      if (v >= lo && v <= hi) ticks.push(v);
    }
  }
  return ticks.length > 7 ? ticks.filter((v) => Math.abs(Math.log10(v) - Math.round(Math.log10(v))) < 1e-9) : ticks;
}

// --- Chart ------------------------------------------------------------------

// The step of an ascending time axis whose points sit on one regular spacing, such as a row per hour.
// The step is the median spacing. The function takes it when at least half of the spacings equal it, within 1 %.
// It returns 0 for an irregular axis, such as event times or months, and for fewer than 4 points.
function regularStep(xs) {
  const n = xs ? xs.length : 0;
  if (n < 4) return 0;
  const stride = Math.max(1, Math.floor((n - 1) / 4096));
  const deltas = [];
  for (let i = 1; i < n; i += stride) {
    const d = xs[i] - xs[i - 1];
    if (d > 0) deltas.push(d);
  }
  if (deltas.length < 3) return 0;
  deltas.sort((a, b) => a - b);
  const step = deltas[deltas.length >> 1];
  let regular = 0;
  for (const d of deltas) if (Math.abs(d - step) <= step * 0.01) regular++;
  return regular * 2 >= deltas.length ? step : 0;
}

function create(host, initial = {}) {
  const root = document.createElement("div");
  root.className = "chartCore";
  root.innerHTML = `
      <div class="chartCore__plot">
        <canvas class="chartCore__canvas" aria-hidden="true"></canvas>
        <div class="chartCore__cursor" aria-hidden="true">
          <i class="chartCore__select" hidden></i>
          <i class="chartCore__xline" hidden></i>
          <i class="chartCore__yline" hidden></i>
          <span class="chartCore__badge chartCore__selectBadge" hidden></span>
          <span class="chartCore__badge chartCore__xbadge" hidden></span>
          <span class="chartCore__badge chartCore__ybadge" hidden></span>
        </div>
        <div class="chartCore__tooltip" role="tooltip" hidden></div>
        <div class="chartCore__overlay" tabindex="0" role="application" aria-roledescription="chart" aria-label="Chart cursor: Left / Right move it point by point (Shift: 10), Home / End jump to the ends, drag to zoom"></div>
        <i class="chartCore__probe" aria-hidden="true"></i>
      </div>
      <div class="chartCore__legend" role="group" aria-label="Series"></div>`;
  host.appendChild(root);
  const plotEl = $(".chartCore__plot", root);
  // The plot width is the width that the resize observer reports. It is the content box, floored.
  // clientWidth rounds the width. A 523.5 px plot drawn at clientWidth (524) is redrawn at 523 px.
  // The engine redraws it at 523 px as soon as the observer reports the width.
  const plotWidth = () => { counters.layoutReads++; return Math.floor(plotEl.getBoundingClientRect().width); };
  const baseCanvas = $("canvas", root);
  const overCanvas = $(".chartCore__overlay", root);
  const cursorEl = $(".chartCore__cursor", root);
  const selectEl = $(".chartCore__select", cursorEl);
  const xLine = $(".chartCore__xline", cursorEl);
  const yLine = $(".chartCore__yline", cursorEl);
  const selectBadge = $(".chartCore__selectBadge", cursorEl);
  const xBadge = $(".chartCore__xbadge", cursorEl);
  const yBadge = $(".chartCore__ybadge", cursorEl);
  const cursorDots = [];
  const tooltipEl = $(".chartCore__tooltip", root);
  const probe = $(".chartCore__probe", root);
  const legendEl = $(".chartCore__legend", root);
  const baseCtx = baseCanvas.getContext("2d");

  const opts = {
    height: DEFAULT_HEIGHT,
    xKind: "number",
    xs: new Float64Array(0),
    categories: [],
    series: [],
    type: "line",
    stack: false,
    xLabel: "",
    xFractionDigits: 3,
    tooltip: "all",
    legend: true,
    syncKey: "",
    onZoom: null,
    ...initial,
  };
  let hidden = new Set(initial.hidden || []);
  let highlight = null;
  let legendMode = "list"; // hune: E41
  let zoom = null; // [lo, hi] in x units
  let theme = null;
  let layout = null;
  let stacks = null; // Map id -> { top, base } for the visible series
  let cursor = null; // { px, py } css px in the plot canvas
  let cursorIndex = -1;
  let syncedX = null;
  let drag = null;
  let sizeW = 0;
  let dpr = window.devicePixelRatio || 1;
  let drawRaf = 0;
  const overlayFrame = util.rafOnce(() => drawOverlay());
  let released = false;
  let pendingDraw = false;
  let destroyed = false;
  let tooltipKey = "";
  let tooltipSize = { w: 0, h: 0 };
  let stats = { points: 0, series: 0, ms: 0 };
  // Line series of the last draw that marked each value. This applies to few values.
  let markedSeries = 0;
  // Series drawn with the light wash under their line. See stats().washed.
  let washedSeries = 0;
  let observedWidth = 0;
  let widthObserved = false;
  let refreshCursor = false;
  // The y axis of the visible range is kept until the data, the zoom, the hidden series or the theme changes (axisStamp).
  // hune: the clock of the time axis, the cursor time and the head of the tooltip (E7, F10): the option `zone`, else the browser.
  const clockOf = () => (opts.zone && typeof opts.zone.parts === "function" && typeof opts.zone.epoch === "function" ? opts.zone : LOCAL_CLOCK);
  let axisStamp = 1;
  let axisCache = null;

  // hune: E40. No alignment group: no `member`, knownWidth or settleGroup.

  plotEl.style.height = `${opts.height}px`;

  // --- theme -------------------------------------------------------------

  function computedColor(value) {
    probe.style.color = "";
    probe.style.color = value;
    return getComputedStyle(probe).color;
  }

  function color(value) {
    return parseColor(computedColor(value));
  }

  function readTheme() {
    const style = getComputedStyle(root);
    // Forced colors (Windows high contrast): the axes, labels and grid take the system palette.
    // The series keep their colors. The probe is excluded from the forcing.
    const forced = typeof window.matchMedia === "function" && window.matchMedia("(forced-colors: active)").matches;
    const text = color(forced ? "CanvasText" : "var(--text)");
    const muted = color(forced ? "GrayText" : "var(--muted)");
    const panel = color(forced ? "Canvas" : "var(--panel)");
    const dark = luminance(panel) < 0.5;
    const family = style.fontFamily || "system-ui, sans-serif";
    const seriesColors = new Map();
    // Many series (Metrics): the engine reads the colors from the page, then parses them in one batch.
    const parsed = parseColors(opts.series.map((s) => computedColor(s.color || "var(--qchart-1)")));
    opts.series.forEach((s, i) => seriesColors.set(s.id, parsed[i]));
    return {
      dark,
      text,
      muted,
      panel,
      // hune: the axis labels have the regular weight.
      font: `400 ${FONT_SIZE}px ${family}`,
      fontBold: `600 ${FONT_SIZE}px ${family}`,
      // Grafana: hairline grids at 9% of the text color. The grid uses the theme tokens.
      // The grid has no color of its own.
      grid: rgba(text, forced ? 0.3 : 0.09),
      axis: rgba(muted, 0.55),
      // hune: the axis labels have the full text colour, not the muted colour.
      label: rgba(text, 1),
      seriesColors,
    };
  }

  function seriesColor(s) {
    if (!theme.seriesColors.has(s.id)) theme.seriesColors.set(s.id, color(s.color || "var(--qchart-1)"));
    return theme.seriesColors.get(s.id);
  }

  // --- data ----------------------------------------------------------------

  const visibleSeries = () => opts.series.filter((s) => !hidden.has(s.id));
  const isStacked = () => opts.stack && (opts.type === "area" || opts.type === "bar");

  // hune: E13, F16. A chart has the left scale `y` and, when the option `y2` is an object, the right scale `y2`. `opts.scales` maps the id of a series
  // to "y" or "y2" (a series that it does not name is on `y`). Without `y2`, every series is on `y` and the chart is the chart of one scale.
  // The options of the right scale are in `opts.y2`: yAxis, yBounds, ySoft, yTickFormat, yTitle, yScale, yUnit, yHeadroom, formatY, shown.
  const hasY2 = () => !!opts.y2 && typeof opts.y2 === "object";
  const scaleOf = (s) => (hasY2() && opts.scales && opts.scales[s.id] === "y2" ? "y2" : "y");
  const viewFor = (L, s) => (L.y2 && scaleOf(s) === "y2" ? L.y2 : L);
  const leftShown = () => opts.yShown !== false;
  const rightShown = () => hasY2() && opts.y2.shown !== false;
  // The options that a scale reads. The left scale reads the options of the chart. The right scale has none of them but its own.
  function scaleOpts(key) {
    if (key !== "y2") return opts;
    const o2 = opts.y2 || {};
    return { ...opts, yScale: o2.yScale, yAxis: o2.yAxis, yBounds: o2.yBounds, ySoft: o2.ySoft, yTickFormat: o2.yTickFormat, yTitle: o2.yTitle,
      yUnit: o2.yUnit, yHeadroom: o2.yHeadroom, formatY: o2.formatY }; // hune: E40
  }
  // The scale whose grid lines the chart draws: the option `gridScale` ("y", "y2" or "none"), else the left scale when its axis shows, else the right one.
  function gridKey() {
    const g = opts.gridScale;
    if (g === "none") return null;
    if (g === "y2" && hasY2()) return "y2";
    if (g === "y") return "y";
    return leftShown() ? "y" : rightShown() ? "y2" : null;
  }
  // hune: E14, F17. A series with `negate` draws -v. The values of the series, the legend and the tooltip stay the data (paper 10.8, V3).
  const negatedValues = new WeakMap();
  function drawnValues(s) {
    if (!s.negate || s.derived) return s.values;
    let neg = negatedValues.get(s.values);
    if (!neg || neg.length !== s.values.length) {
      neg = new Float64Array(s.values.length);
      for (let i = 0; i < neg.length; i++) neg[i] = -s.values[i];
      negatedValues.set(s.values, neg);
    }
    return neg;
  }
  const negSeries = (s) => ({ ...s, values: drawnValues(s), derived: true });
  const isBarSeries = (s) => (s.type || opts.type) === "bar";

  // Cumulative tops and bases of the visible series. NULL counts as 0.
  // Bars stack positive values up and negative values down. Areas only add up.
  function computeStacks() {
    if (!isStacked()) { stacks = null; return; }
    counters.stackBuilds++;
    const n = opts.xs.length;
    const state = new Map();
    stacks = new Map();
    // stackMode "percent": each x shows the shares of its group, out of 100.
    const totals = new Map();
    // hune: E32. `stackSign: "series"` (Grafana 12.4.2): a series stacks down from 0, in a group of its own, when more of its values are below 0
    // than above 0 (about 100 samples, from its first to its last value; `negate` turns the side). Its values add as they are, and a percent is
    // of the signed total of its group. A NULL is at the top of its group when a series of the group has a value there, else it is not drawn.
    const bySign = opts.stackSign === "series";
    const sideOf = (s) => {
      const a = s.values;
      let i = 0, j = a.length - 1, neg = 0, pos = 0;
      while (i <= j && !(a[i] === a[i])) i++;
      while (j >= i && !(a[j] === a[j])) j--;
      for (const d = Math.max(1, Math.floor((j - i + 1) / 100)); i <= j; i += d) if (a[i] < 0 || Object.is(a[i], -0)) neg++; else if (a[i] > 0) pos++;
      return (neg > pos) !== !!s.negate ? -1 : 1;
    };
    const sides = new Map(bySign ? visibleSeries().map((s) => [s.id, sideOf(s)]) : []);
    const has = new Map();
    // hune: E13. A series stacks only with the series of its own scale.
    // hune: E33. With `stackSign`, a series of another draw (`type`: bars, points or a line) stacks in a group of its own (Grafana 12.4.2).
    const stackKey = (s) => (s.group == null ? "" : String(s.group)) + (scaleOf(s) === "y2" ? "\u0001y2" : "") + (sides.get(s.id) < 0 ? "\u0002" : "") + (bySign && s.type ? "\u0003" + s.type : "");
    if (opts.stackMode === "percent" || bySign) {
      for (const s of visibleSeries()) {
        if (ownMark(s)) continue;
        const key = stackKey(s);
        if (!totals.has(key)) { totals.set(key, new Float64Array(n)); has.set(key, new Uint8Array(n)); }
        const sum = totals.get(key);
        const dvs = drawnValues(s);
        for (let i = 0; i < n; i++) { const v = dvs[i]; if (v === v) { sum[i] += bySign ? v : Math.abs(v); has.get(key)[i] = 1; } } // hune: E32
      }
    }
    for (const s of visibleSeries()) {
      if (ownMark(s)) continue;
      const key = stackKey(s);
      if (!state.has(key)) state.set(key, { pos: new Float64Array(n), neg: opts.type === "bar" ? new Float64Array(n) : null });
      const st = state.get(key);
      const top = new Float64Array(n);
      const base = new Float64Array(n);
      counters.allocBytes += 16 * n;
      const values = drawnValues(s);
      const sum = opts.stackMode === "percent" ? totals.get(key) : null; // hune: E32
      // hune: E32. In percent, a top is the sum below it divided by the signed total of its group, times 100. Thus the last top is 100.
      const share = (a, i) => (sum ? (sum[i] ? ((sides.get(s.id) * a) / sum[i]) * 100 : 0) : a);
      for (let i = 0; i < n; i++) {
        const v = values[i];
        if (bySign) {
          base[i] = share(st.pos[i], i);
          if (v === v) st.pos[i] += v;
          top[i] = v === v || has.get(key)[i] ? share(st.pos[i], i) : NaN;
          continue;
        }
        const value = v === v ? (sum ? (sum[i] > 0 ? v / sum[i] * 100 : 0) : v) : 0;
        if (st.neg && value < 0) {
          base[i] = st.neg[i];
          top[i] = st.neg[i] + value;
          st.neg[i] = top[i];
        } else {
          base[i] = st.pos[i];
          top[i] = st.pos[i] + value;
          st.pos[i] = top[i];
        }
      }
      stacks.set(s.id, { top, base });
    }
  }

  function fullDomain() {
    const fixed = fixedDomain();
    if (fixed) return fixed;
    const xs = opts.xs;
    const n = xs.length;
    if (opts.xKind === "category") return [-0.5, Math.max(0.5, n - 0.5)];
    let lo = n ? xs[0] : 0;
    let hi = n ? xs[n - 1] : 1;
    if (hi === lo) {
      const pad = opts.xKind === "time" ? MINUTE_MS : Math.max(1, Math.abs(lo) * 0.1);
      lo -= pad;
      hi += pad;
    } else if (opts.type === "bar" && n > 1) {
      // Bars are centred on their x: half a bar fits inside each edge.
      const gap = minGap(0, n);
      const half = Math.max(gap, (hi - lo) / 400) / 2;
      lo -= half;
      hi += half;
    }
    return [lo, hi];
  }

  function minGap(i0, i1) {
    const xs = opts.xs;
    let gap = Infinity;
    for (let i = i0 + 1; i < i1; i++) {
      const g = xs[i] - xs[i - 1];
      if (g > 0 && g < gap) gap = g;
    }
    return Number.isFinite(gap) ? gap : 1;
  }

  // --- layout ----------------------------------------------------------------

  // Label widths at the axis font, and at the bold font of the date line. The engine caches them.
  // Tick labels repeat on redraws. So, a draw measures only new texts.
  const widths = new Map();
  const boldWidths = new Map();
  function measureIn(cache, font, text) {
    let w = cache.get(text);
    if (w === undefined) {
      baseCtx.font = font;
      w = baseCtx.measureText(text).width;
      counters.textMeasures++;
      if (cache.size > 2000) cache.clear();
      cache.set(text, w);
    }
    return w;
  }
  const measure = (text) => measureIn(widths, theme.font, text);
  const measureBold = (text) => measureIn(boldWidths, theme.fontBold, text);

  // Block summaries of the long series, by series id. See extendSummary.
  // setData({ append: true }) lets each summary grow with its series.
  // Any other setData call starts the summaries again on their next use.
  const summaries = new Map();
  function summaryOf(s) {
    const values = s.values;
    const n = values ? values.length : 0;
    if (n < BLOCK_MIN_POINTS || s.xs || s.derived) return null;
    let sum = summaries.get(s.id);
    if (sum && sum.values === values) return sum;
    if (!sum || sum.adopt !== values || n < sum.n) {
      sum = newSummary();
      summaries.set(s.id, sum);
    }
    sum.adopt = null;
    return extendSummary(sum, values, n);
  }

  // Min and max of a value array over [v0, v1). The engine keeps them while the array and the range stay the same.
  // Redraws on hover-free changes, resizes and theme changes use the kept values.
  const extents = new WeakMap();
  function extent(a, v0, v1) {
    const cached = extents.get(a);
    if (cached && cached.v0 === v0 && cached.v1 === v1) return cached;
    let min = Infinity, max = -Infinity;
    counters.extentScans += Math.max(0, v1 - v0);
    for (let i = v0; i < v1; i++) {
      const v = a[i];
      if (v < min) min = v;
      if (v > max) max = v;
    }
    const out = { v0, v1, min, max };
    extents.set(a, out);
    return out;
  }

  // The visible range is the x domain: the zoom, or the whole domain. It also gives the indexes of its points.
  // A line has one point beyond each edge, so that the line leaves the plot.
  function viewOf() {
    const xs = opts.xs;
    const n = xs.length;
    const [fullLo, fullHi] = fullDomain();
    let [xLo, xHi] = zoom || [fullLo, fullHi];
    if (!(xHi > xLo)) [xLo, xHi] = [fullLo, fullHi];
    let i0 = opts.xKind === "category" ? Math.max(0, Math.ceil(xLo)) : lowerBound(xs, xLo);
    let i1 = opts.xKind === "category" ? Math.min(n, Math.floor(xHi) + 1) : upperBound(xs, xHi);
    const v0 = i0, v1 = i1;
    // hune: bars on a time axis draw the bars next to the range too: a bar whose centre is outside the plot shows its part inside (E10, V item, Grafana).
    if (opts.xKind !== "category" && (opts.type !== "bar" || opts.xKind === "time")) { i0 = Math.max(0, i0 - 1); i1 = Math.min(n, i1 + 1); }
    return { xLo, xHi, i0, i1, v0, v1 };
  }

  // The y axis of a visible range has a domain, ticks with their labels, and the width of the widest label.
  // It reads the data, the height of the plot and the font. It never reads the width of the chart.
  // The chart pads its range and keeps the ticks inside it. hune: E40, no aligned axis.
  function yAxisOf(view, key = "y") {
    const so = scaleOpts(key);
    const { xLo, xHi, v0, v1 } = view;
    // Y domain over the visible points of the series of this scale.
    let yMin = Infinity, yMax = -Infinity;
    // hune: E30. The data ends for a yAxis hook (raw): the tops of a stack, no base and no soft end.
    let rMin = Infinity, rMax = -Infinity;
    const vis = visibleSeries().filter((s) => scaleOf(s) === key);
    for (const s of vis) {
      const st = stacks && stacks.get(s.id);
      const [a0, a1] = s.xs ? ownRange(s, xLo, xHi) : [v0, v1];
      const sum = st || s.negate ? null : summaryOf(s);
      for (const a of st ? [st.top, st.base] : [drawnValues(s)]) {
        const e = sum ? summaryRange(sum, a, a0, a1) : extent(a, a0, a1);
        if (e.min < yMin) yMin = e.min;
        if (e.max > yMax) yMax = e.max;
        if (!st || a === st.top) { rMin = Math.min(rMin, e.min); rMax = Math.max(rMax, e.max); } // hune: E30
      }
    }
    const cellsY = key === "y" ? cellsExtent(xLo, xHi) : null;
    if (cellsY) { yMin = Math.min(yMin, cellsY[0]); yMax = Math.max(yMax, cellsY[1]); }
    if (!(yMin <= yMax)) { yMin = 0; yMax = 1; }
    // hune: E15, F16. A soft end moves the end of the data, before the automatic rule: `ySoft` is [soft_min, soft_max], an end that is null is not set.
    if (Array.isArray(so.ySoft)) {
      if (Number.isFinite(so.ySoft[0]) && so.ySoft[0] < yMin) yMin = so.ySoft[0];
      if (Number.isFinite(so.ySoft[1]) && so.ySoft[1] > yMax) yMax = so.ySoft[1];
    }
    const dataMin = yMin, dataMax = yMax;
    // hune: one row of x labels (no second row with the date), and a band of 21 px under the plot.
    // hune: a title under the x axis (xTitle) takes 20 px more (Grafana, E11).
    const bottom = 21;
    // hune: E25. Turned x labels (opts.xTickRotation) take the height of the tallest one, where a flat label takes FONT_SIZE px of the 21 px.
    const turn = rotation();
    let more = 0;
    if (turn) {
      for (let i = 0; i < Math.min(400, opts.xs.length); i++) more = Math.max(more, turnedBox(measure(short(i)), turn).h - FONT_SIZE);
    }
    const band = bottom + more + (opts.xTitle ? 20 : 0);
    const padTop = PAD_TOP; // hune: E19, no gutter for the labels of engine annotations.
    // hune: E27. With no y axis on either side, the plot starts at the top of the canvas (Grafana).
    const top = leftShown() || rightShown() ? padTop : 0;
    const plotH = Math.max(40, opts.height - top - band);
    // hune: E13. A bar series of a chart that draws in series order (opts.drawOrder) also starts its bars at 0.
    const pinned = isStacked() || opts.type === "bar" || (opts.drawOrder === "series" && vis.some((s) => isBarSeries(s)));
    if (pinned) { yMin = Math.min(0, yMin); yMax = Math.max(0, yMax); }
    else if (yMin >= 0 && yMax - yMin > 0.25 * yMax) yMin = 0;
    else if (yMax <= 0 && yMax - yMin > 0.25 * -yMin) yMax = 0;
    if (yMax === yMin) {
      const pad = yMin === 0 ? 1 : Math.abs(yMin) * 0.1;
      yMin -= pad; yMax += pad;
    } else {
      const pad = (yMax - yMin) * 0.06;
      // yHeadroom: more room above the largest value. Dots of a sparse series never sit on the top edge of the plot.
      const head = so.yHeadroom > 0 ? (yMax - yMin) * so.yHeadroom : pad;
      if (yMax !== 0 || !pinned) yMax += head;
      if (yMin !== 0) yMin -= pad;
    }

    // yBounds [min, max]: a fixed edge. null uses the edge of the data. A pair that leaves no range is ignored.
    if (Array.isArray(so.yBounds)) {
      const lo = Number.isFinite(so.yBounds[0]) ? so.yBounds[0] : yMin;
      const hi = Number.isFinite(so.yBounds[1]) ? so.yBounds[1] : yMax;
      if (hi > lo) { yMin = lo; yMax = hi; }
    }

    const yAx = customYAxis(dataMin, dataMax, plotH, so, rMin <= rMax ? [rMin, rMax] : null); // hune: E30
    if (yAx) { yMin = yAx.min; yMax = yAx.max; }
    const yt = yAx ? { step: yAx.step || 0, values: yAx.ticks.map((t) => t.v) } : linearTicks(yMin, yMax, Math.max(2, Math.floor(plotH / Y_TICK_SPACE)));
    const yUnit = yUnitFor(Math.max(Math.abs(yMin), Math.abs(yMax)), so);
    const yLabels = yAx ? yAx.ticks.map((t) => t.label) : yt.values.map((v) => yTickLabel(v, yt.step, yUnit, so));
    let labelW = 0;
    for (const l of yLabels) labelW = Math.max(labelW, measure(l));
    return { yMin, yMax, plotH, padTop: top, bottom: band, ticks: yt, labels: yLabels, labelW, key }; // hune: E27
  }

  // The visible range and its y axis are kept while nothing that they read changes (axisStamp).
  // A draw and the layout readers share one computation.
  function axisNow() {
    if (axisCache && axisCache.stamp === axisStamp) return axisCache;
    if (!theme) theme = readTheme();
    if (stacks === undefined || stacks === null) computeStacks();
    const view = viewOf();
    axisCache = { stamp: axisStamp, view, axis: yAxisOf(view, "y"), axis2: hasY2() ? yAxisOf(view, "y2") : null };
    return axisCache;
  }

  // The gutter at the left of the plot (px) has room for the widest y label.
  // hune: the gutter is the widest label plus 9 px (E13: Grafana, paper 10.8 V2), and a title at the left of the y axis (yTitle) takes 28 px more (E11, E15).
  // A left axis that does not show (opts.yShown === false) takes the margin of 8 px.
  // hune: E46. `yWidth` (the property `axis_width`) is the gutter in px; a title adds 20 px (Grafana: the size of the axis, then the label).
  const ownGutter = (axis) => (leftShown() ? (Number.isFinite(opts.yWidth) ? opts.yWidth + (opts.yTitle ? 20 : 0) : Math.ceil(axis.labelW + Y_AXIS_PAD + (opts.yTitle ? Y_TITLE_W : 0))) : PAD_RIGHT);
  // hune: E13. The gutter at the right of the plot: the widest label of the right scale plus 9 px, and the title. It replaces the margin of 8 px.
  const rightGutter = (axis2) => (rightShown() && axis2 ? (Number.isFinite(opts.y2.yWidth) ? opts.y2.yWidth + (opts.y2.yTitle ? 20 : 0) : Math.ceil(axis2.labelW + Y_AXIS_PAD + (opts.y2.yTitle ? Y_TITLE_W : 0))) : PAD_RIGHT); // hune: E46

  function computeLayout(width) {
    const { view, axis, axis2 } = axisNow();
    const { xLo, xHi, i0, i1, v0, v1 } = view;
    const { yMin, yMax, plotH, padTop, bottom, ticks: yt, labels: yLabels } = axis;
    const left = ownGutter(axis); // hune: E40
    const right = rightGutter(axis2);
    const plotW = Math.max(40, width - left - right);
    const ky = plotH / (yMax - yMin);
    const kx = plotW / (xHi - xLo);
    const L = {
      width, height: opts.height, left, right, top: padTop, plotW, plotH, bottom,
      xLo, xHi, yMin, yMax, kx, ky, i0, i1, v0, v1,
      yTicks: yt.values.map((v, k) => ({ v, label: yLabels[k] })), yStep: yt.step,
      xOf: (x) => left + (x - xLo) * kx,
      yOf: (y) => padTop + plotH - (y - yMin) * ky,
      xAt: (px) => xLo + (px - left) / kx,
      yAt: (py) => yMin + (padTop + plotH - py) / ky,
    };
    if (opts.yScale === "log") applyLogScale(L);
    // hune: E13. The view of the right scale: the layout of the left scale with the range, the ticks and the functions of the right scale. A series of
    // the right scale draws and reads its dots on this view (viewFor).
    if (axis2) {
      const ky2 = plotH / (axis2.yMax - axis2.yMin);
      const V = Object.assign({}, L, {
        yMin: axis2.yMin, yMax: axis2.yMax, ky: ky2,
        yTicks: axis2.ticks.values.map((v, k) => ({ v, label: axis2.labels[k] })), yStep: axis2.ticks.step,
        yOf: (y) => padTop + plotH - (y - axis2.yMin) * ky2,
        yAt: (py) => axis2.yMin + (padTop + plotH - py) / ky2,
        scaleKey: "y2", logY: false, logView: null,
      });
      if (scaleOpts("y2").yScale === "log") applyLogScale(V);
      L.y2 = V;
    }
    L.xTicks = xTicks(L);
    L.xLabels = layoutXLabels(L.xTicks, L.xOf, left, left + plotW, width, measure, measureBold, rotation());
    return L;
  }

  // hune: E25. The rotation of the x labels, in degrees: only a category axis turns its labels. 0 is the picture of the engine.
  function rotation() {
    return opts.xKind === "category" && Number.isFinite(opts.xTickRotation) ? opts.xTickRotation : 0;
  }

  // The text of a category label: at most MAX_CATEGORY_CHARS characters. hune: E25. A turned label also keeps the plot at 40 px or more
  // (and the title of the axis): a longer label ends with an ellipsis.
  function short(i) {
    const c = String(opts.categories[i] ?? ""), turn = rotation();
    const t = c.length > MAX_CATEGORY_CHARS ? `${c.slice(0, MAX_CATEGORY_CHARS - 1)}\u2026` : c;
    if (!turn) return t;
    const r = Math.abs(turn) * Math.PI / 180;
    return fitTitle(t, (opts.height - PAD_TOP - 49 - (opts.xTitle ? 20 : 0) - FONT_SIZE * Math.cos(r)) / Math.sin(r));
  }

  function xTicks(L) {
    const { xLo, xHi, plotW } = L;
    if (opts.xKind === "time") {
      return timeTicks(xLo, xHi, plotW, measure, clockOf()); // hune: E42, no xDateOnly
    }
    if (opts.xKind === "category") {
      const n = opts.xs.length;
      const labels = [];
      const lo = Math.max(0, Math.ceil(xLo)), hi = Math.min(n - 1, Math.floor(xHi));
      const band = plotW / Math.max(1, xHi - xLo);
      let widest = 0;
      for (let i = lo; i <= hi && i - lo < 400; i++) widest = Math.max(widest, measure(short(i)));
      const every = Math.max(1, Math.ceil(((rotation() ? turnedBox(widest, rotation()).step : widest) + 10) / band));
      for (let i = lo; i <= hi; i += every) labels.push({ v: i, label: short(i) });
      return labels;
    }
    // hune: a host that knows the unit of the x field gives its own ticks (E11): xTicksFn(lo, hi, plotW, measure) -> [{ v, label }].
    if (typeof opts.xTicksFn === "function") return opts.xTicksFn(xLo, xHi, plotW, measure) || [];
    const sample = formatValue(Math.max(Math.abs(xLo), Math.abs(xHi)));
    const count = Math.max(2, Math.min(14, Math.floor(plotW / (measure(sample) + 36))));
    const t = linearTicks(xLo, xHi, count, opts.xKind === "index");
    const unit = compactUnitFor(Math.max(Math.abs(xLo), Math.abs(xHi)));
    return t.values.map((v) => ({ v, label: formatTick(v, t.step, unit) }));
  }

  // --- canvases --------------------------------------------------------------

  function sizeCanvas(canvas, ctx, w, h) {
    const bw = Math.round(w * dpr), bh = Math.round(h * dpr);
    if (canvas.width !== bw || canvas.height !== bh) {
      canvas.width = bw;
      canvas.height = bh;
      canvas.style.width = `${w}px`;
      canvas.style.height = `${h}px`;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  // Snap a CSS coordinate to the device pixel grid. So, 1px lines stay crisp.
  const crisp = (v) => (Math.round(v * dpr) + 0.5) / dpr;

  // measure: a draw that is asked for now (flush) measures the plot.
  // It does this when the resize observer last saw the plot hidden. The plot may just have been shown.
  function draw(measure = false) {
    drawRaf = 0;
    if (destroyed) return;
    // The width that the resize observer saw last. A measure forces a layout for each chart.
    // Several charts redraw in one frame.
    const width = widthObserved && !(measure && !observedWidth) ? observedWidth : (observedWidth = plotWidth());
    if (!width) { pendingDraw = true; return; } // hidden: the resize observer draws on show
    pendingDraw = false;
    const t0 = performance.now();
    counters.draws++;
    released = false;
    sizeW = width;
    dpr = window.devicePixelRatio || 1;
    if (!theme) theme = readTheme();
    if (stacks === undefined || stacks === null) computeStacks();
    layout = computeLayout(width);
    sizeCanvas(baseCanvas, baseCtx, width, opts.height);
    const ctx = baseCtx;
    ctx.clearRect(0, 0, width, opts.height);
    drawAxes(ctx, layout);
    ctx.save();
    ctx.beginPath();
    ctx.rect(layout.left, layout.top - 1, layout.plotW, layout.plotH + 2);
    ctx.clip();
    let points = drawCells(ctx, layout);
    // hune: a hook of the host draws under the series, inside the plot (E8, F12: the lines and the bands of the thresholds). It reads
    // the layout (left, top, plotW, plotH, yMin, yMax, yOf) and draws nothing for the style "off".
    if (typeof opts.drawUnder === "function") opts.drawUnder(ctx, layout, { dpr });
    markedSeries = 0;
    washedSeries = 0;
    const all = visibleSeries();
    const own = all.filter(ownMark);
    const vis = own.length ? all.filter((s) => !own.includes(s)) : all;
    const perSeries = {};
    if (opts.drawOrder === "series") {
      // hune: E14, F17. The bars and the lines of one chart draw in the order of the series: the bars are one cluster, each bar series in its turn.
      const bars = vis.filter(isBarSeries);
      for (const s of vis) {
        if (isBarSeries(s)) { points = Math.max(points, drawBars(ctx, layout, bars, perSeries, [s])); continue; }
        const drawn = drawSeries(ctx, viewFor(layout, s), s);
        perSeries[s.label] = { points: drawn, runs: runs.length };
        points = Math.max(points, drawn);
      }
    } else if (opts.type === "bar") points = Math.max(points, drawBars(ctx, layout, vis, perSeries));
    else {
      for (const s of vis) {
        const drawn = drawSeries(ctx, viewFor(layout, s), s);
        perSeries[s.label] = { points: drawn, runs: runs.length };
        points = Math.max(points, drawn);
      }
    }
    points = Math.max(points, drawOwnMarks(ctx, layout, own, perSeries));
    ctx.restore();
    stats = { points, series: all.length, perSeries, marked: markedSeries, washed: washedSeries, ms: performance.now() - t0 };
    counters.drawMs += stats.ms;
    publish();
    publishExtras();
    drawOverlay();
    renderLegend();
    // hune: E17, no exemplar markers to place.
    if (refreshCursor) {
      refreshCursor = false;
      if (cursor) moveCursor(cursor);
    }
  }

  function publish() {
    const L = layout;
    root.dataset.pointsDrawn = String(stats.points);
    root.dataset.seriesDrawn = String(stats.series);
    root.dataset.pointsMarked = String(stats.marked || 0);
    root.dataset.seriesWashed = String(stats.washed || 0);
    root.dataset.xMin = String(L.xLo);
    root.dataset.xMax = String(L.xHi);
    root.dataset.interpolation = interpolation();
    root.dataset.gaps = String(gapInfo().count);
    root.dataset.yMin = String(L.yMin);
    root.dataset.yMax = String(L.yMax);
    // hune: E13. The range of the right scale.
    if (L.y2) { root.dataset.y2Min = String(L.y2.yMin); root.dataset.y2Max = String(L.y2.yMax); } else { delete root.dataset.y2Min; delete root.dataset.y2Max; }
    root.dataset.zoomed = String(!!zoom);
    root.dataset.drawMs = stats.ms.toFixed(2);
    root.dataset.type = opts.type;
    // Test and debug hooks. They show what each series drew and where the plot is.
    root.dataset.seriesStats = JSON.stringify(stats.perSeries);
    root.dataset.draws = String((Number(root.dataset.draws) || 0) + 1);
    root.dataset.plot = `${L.left} ${L.top} ${L.plotW} ${L.plotH}`;
    // The y step.
    root.dataset.yStep = String(L.yStep);
    if (typeof opts.onDraw === "function") opts.onDraw({ ...stats, xLo: L.xLo, xHi: L.xHi, zoomed: !!zoom, yMin: L.yMin, yMax: L.yMax, y2Min: L.y2 ? L.y2.yMin : null, y2Max: L.y2 ? L.y2.yMax : null });
  }

  // hune: E15. A title longer than the room of its axis ends with an ellipsis. hune: E31. A binary search in the first TITLE_MAX_CHARS characters.
  function fitTitle(text, maxW) {
    const all = String(text);
    const t = all.slice(0, TITLE_MAX_CHARS);
    if (t.length === all.length && measure(t) <= maxW) return t;
    let lo = 1, hi = t.length;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (measure(`${t.slice(0, mid)}\u2026`) <= maxW) lo = mid;
      else hi = mid - 1;
    }
    return `${t.slice(0, lo)}\u2026`;
  }

  // hune: E46. The colour of a y axis (the property `axis_color`): a CSS colour, or a function (ctx, L) that gives a CSS colour or a canvas style
  // (a gradient); else the colour of the labels.
  function axisPaint(ctx, L, c) {
    const v = typeof c === "function" ? c(ctx, L) : c;
    return typeof v === "string" && v ? rgba(cssColor(v), 1) : v || theme.label;
  }
  // hune: E46. The border of a y axis (the property `axis_border`): a line of 1 device px on the edge of the plot from its top to its bottom, and a tick
  // of 4 px (`out` is -4 at the left, 4 at the right) at each tick of the scale, in the colour of the axis (Grafana 12.4.2).
  function axisBorder(ctx, L, x, out, c) {
    const cx = crisp(x);
    ctx.save();
    ctx.lineWidth = 1 / dpr;
    ctx.strokeStyle = axisPaint(ctx, L, c);
    ctx.beginPath();
    ctx.moveTo(cx, L.top);
    ctx.lineTo(cx, L.top + L.plotH);
    for (const t of L.yTicks) {
      const y = crisp(L.yOf(t.v));
      ctx.moveTo(cx, y);
      ctx.lineTo(cx + out, y);
    }
    ctx.stroke();
    ctx.restore();
  }

  function drawAxes(ctx, L) {
    const { left, top, plotW, plotH } = L;
    // hune: E13. The grid lines are those of one scale (gridKey). The ticks of the other scale have their labels only.
    const gk = gridKey();
    const gv = gk === "y2" && L.y2 ? L.y2 : L;
    ctx.lineWidth = 1 / dpr;
    ctx.strokeStyle = theme.grid;
    ctx.beginPath();
    if (gk) for (const t of gv.yTicks) {
      const y = crisp(gv.yOf(t.v));
      ctx.moveTo(left, y);
      ctx.lineTo(left + plotW, y);
    }
    // hune: `xGrid: false` draws no vertical grid line (the bar chart of Grafana has none) (E10).
    if (opts.xGrid !== false) for (const t of L.xTicks) {
      const x = crisp(L.xOf(t.v));
      if (x < left - 0.5 || x > left + plotW + 0.5) continue;
      ctx.moveTo(x, top);
      ctx.lineTo(x, top + plotH);
    }
    ctx.stroke();
    // hune: no axis baseline, no zero line and no tick marks. The grid lines are the only lines (Grafana look).
    ctx.font = theme.font;
    ctx.fillStyle = theme.label;
    ctx.textBaseline = "middle";
    // hune: the y labels of the left axis end 5 px before the plot, and those of the right axis start 5 px after it (E13); an axis that does not show has none.
    if (leftShown()) {
      ctx.textAlign = "right";
      ctx.fillStyle = axisPaint(ctx, L, opts.yColor); // hune: E46
      for (const t of L.yTicks) if (t.label !== "") ctx.fillText(t.label, left - Y_LABEL_GAP - (opts.yBorder ? 4 : 0), L.yOf(t.v));
      if (opts.yBorder) axisBorder(ctx, L, left, -4, opts.yColor);
    }
    if (rightShown() && L.y2) {
      ctx.textAlign = "left";
      ctx.fillStyle = axisPaint(ctx, L.y2, opts.y2.yColor); // hune: E46
      for (const t of L.y2.yTicks) if (t.label !== "") ctx.fillText(t.label, left + plotW + Y_LABEL_GAP + (opts.y2.yBorder ? 4 : 0), L.y2.yOf(t.v));
      if (opts.y2.yBorder) axisBorder(ctx, L.y2, left + plotW, 4, opts.y2.yColor);
    }
    ctx.fillStyle = theme.label; // hune: E46
    ctx.textBaseline = "top";
    ctx.textAlign = "center";
    const y1 = top + plotH + TICK_LEN + 9;
    const turn = rotation();
    if (!turn) for (const t of L.xLabels) ctx.fillText(t.label, t.x, y1);
    else {
      // hune: E25. Each label turns about its anchor (x, y1 + half its height), so the top of its box is at y1. A positive angle turns it
      // clockwise and starts it at the anchor, a negative angle turns it counter-clockwise and ends it there (Grafana `xTickLabelRotation`).
      ctx.save();
      ctx.textBaseline = "middle";
      ctx.textAlign = turn > 0 ? "left" : "right";
      for (const t of L.xLabels) {
        ctx.save();
        ctx.translate(t.x, y1 + FONT_SIZE / 2 * Math.cos(turn * Math.PI / 180));
        ctx.rotate(turn * Math.PI / 180);
        ctx.fillText(t.label, 0, 0);
        ctx.restore();
      }
      ctx.restore();
    }
    // hune: the titles of the axes (E11): the name of the x field under the labels, and the name of the y field turned, at the left.
    if (opts.xTitle) {
      ctx.textBaseline = "alphabetic";
      ctx.fillText(String(opts.xTitle), left + plotW / 2, top + plotH + L.bottom - 1);
    }
    // hune: E15. A title of a y axis has the font of the labels. Its centre is 12 px from the outer edge. The left title turns by -90 degrees, the right title by 90.
    const titleAt = (text, x, angle) => {
      ctx.save();
      ctx.translate(x, top + plotH / 2);
      ctx.rotate(angle);
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(fitTitle(text, plotH), 0, 0);
      ctx.restore();
    };
    if (opts.yTitle && leftShown()) titleAt(opts.yTitle, Y_TITLE_CENTRE, -Math.PI / 2);
    if (rightShown() && opts.y2.yTitle) titleAt(opts.y2.yTitle, L.width - Y_TITLE_CENTRE, Math.PI / 2);
  }

  function alphaFor(s) {
    return highlight && highlight !== s.id ? 0.18 : 1;
  }

  // Vertices of the series that is drawn. The engine reuses them across series and draws:
  // vx and vy are the line. vb is the stack base under it. runs gives the index where each
  // unbroken run starts. Lines break at NULLs.
  let vx = new Float64Array(4096), vy = new Float64Array(4096), vb = new Float64Array(4096);
  let vCount = 0;
  let decimated = false;
  const runs = [];
  function vertex(x, y, b) {
    if (vCount === vx.length) {
      const grow = (a) => { const next = new Float64Array(a.length * 2); counters.allocBytes += 8 * next.length; next.set(a); return next; };
      vx = grow(vx); vy = grow(vy); vb = grow(vb);
    }
    vx[vCount] = x; vy[vCount] = y; vb[vCount] = b;
    vCount++;
  }

  // A time axis has one regular step, such as a row per hour. Rows can be missing.
  // A gap wider than GAP_FACTOR steps breaks the line. So, a stop of the data does not read as a trend. This is the same break as a NULL.
  // The break is Infinity (no break) for the other axes, bars, a series of few points, and an irregular spacing (event times, months).
  // It is also Infinity when opts.connectGaps is set. The step is the median spacing. The engine takes it when most spacings equal it.
  const GAP_FACTOR = 1.5;
  let gapCache = { xs: null, n: -1, limit: Infinity, count: 0 };
  function gapInfo() {
    const xs = opts.xs;
    const n = xs ? xs.length : 0;
    if (opts.xKind !== "time" || opts.type === "bar" || n < 4) return { limit: Infinity, count: 0 };
    if (gapCache.xs === xs && gapCache.n === n) return gapCache;
    let limit = Infinity;
    let count = 0;
    const step = regularStep(xs);
    if (step) {
      limit = step * GAP_FACTOR;
      for (let i = 1; i < n; i++) if (xs[i] - xs[i - 1] > limit) count++;
    }
    gapCache = { xs, n, limit, count };
    return gapCache;
  }
  // The step times 1.5 when the line has gaps to break at. Otherwise it is Infinity, and the block summaries stay in use.
  const gapLimit = () => {
    if (opts.connectGaps) return Infinity;
    const info = gapInfo();
    return info.count ? info.limit : Infinity;
  };

  // One line goes through the visible points, as vertices. When there are more than two points per
  // device-pixel column, each column keeps its first, lowest, highest and last value. This makes one vertical stroke (base: false).
  // So every peak survives at any zoom. Otherwise, the column keeps its highest point and the matching base (base: an array, for stacked areas).
  // A draw costs O(points) arithmetic, but only O(width) canvas calls.
  function trace(L, ys, nulls, base, sum = null) {
    const xs = opts.xs;
    const { i0, i1, left, plotW, kx, ky, xLo, yMin } = L;
    const category = opts.xKind === "category";
    const bottom = L.top + L.plotH;
    const breakAll = !nulls;
    const gl = category ? Infinity : gapLimit();
    vCount = 0;
    runs.length = 0;
    decimated = i1 - i0 > plotW * 2;
    counters.traces++;
    if (decimated) counters.decimations++;
    if (decimated && !base) { traceColumns(L, ys, nulls, sum); return; }
    counters.tracedPoints += Math.max(0, i1 - i0);
    let pen = false;
    const start = () => { if (!pen) { runs.push(vCount); pen = true; } };
    if (!decimated) {
      for (let i = i0; i < i1; i++) {
        const v = ys[i];
        if (v !== v) {
          if (breakAll || nulls[i]) pen = false;
          continue;
        }
        if (i > i0 && xs[i] - xs[i - 1] > gl) pen = false;
        start();
        vertex(left + ((category ? i : xs[i]) - xLo) * kx, bottom - (v - yMin) * ky, base ? bottom - (base[i] - yMin) * ky : 0);
      }
      return;
    }
    let col = -Infinity;
    let fy = 0, lo = 0, hi = 0, ly = 0, cx = 0, by = 0;
    let has = false;
    const flush = () => {
      if (!has) return;
      start();
      if (base) vertex(cx, lo, by);
      else {
        vertex(cx, fy, 0);
        if (lo !== fy) vertex(cx, lo, 0);
        if (hi !== lo) vertex(cx, hi, 0);
        if (ly !== hi) vertex(cx, ly, 0);
      }
      has = false;
    };
    for (let i = i0; i < i1; i++) {
      const v = ys[i];
      if (v !== v) {
        if (breakAll || nulls[i]) { flush(); pen = false; }
        continue;
      }
      if (i > i0 && xs[i] - xs[i - 1] > gl) { flush(); pen = false; }
      const x = left + ((category ? i : xs[i]) - xLo) * kx;
      const c = Math.floor(x * dpr);
      const y = bottom - (v - yMin) * ky;
      if (c !== col) {
        flush();
        col = c;
        cx = (c + 0.5) / dpr;
        fy = lo = hi = ly = y;
        if (base) by = bottom - (base[i] - yMin) * ky;
        has = true;
      } else {
        if (y < lo) { lo = y; if (base) by = bottom - (base[i] - yMin) * ky; }
        if (y > hi) hi = y;
        ly = y;
      }
    }
    flush();
  }

  // A dense line has one entry per device-pixel column (colCount of them).
  // An entry holds its centre (css px), its first and last value, and its extent.
  // The extent is the highest and lowest value, and the step from the previous column. Values are in plot px.
  // colBrk marks a column after a NULL break, or the first column.
  let colCount = 0;
  let colX = new Float64Array(2048), colFy = new Float64Array(2048), colLy = new Float64Array(2048);
  let colTop = new Float64Array(2048), colBot = new Float64Array(2048), colDev = new Float64Array(2048);
  let colBrk = new Uint8Array(2048);
  // hune: the extremes of a column in plot px (colMn is the smallest y, colMx the largest) and the order in which they occur
  // (colLoFirst is 1 when the smallest y comes first). The line goes through first, the two extremes in that order, last (uPlot's linear path).
  let colMn = new Float64Array(2048), colMx = new Float64Array(2048), colLoFirst = new Uint8Array(2048);
  function column(dev, fy, lo, hi, ly, brk, prevLast, loFirst) {
    if (colCount === colX.length) {
      const grow = (a) => { const next = new a.constructor(a.length * 2); counters.allocBytes += next.byteLength; next.set(a); return next; };
      colX = grow(colX); colFy = grow(colFy); colLy = grow(colLy); colTop = grow(colTop); colBot = grow(colBot); colDev = grow(colDev); colBrk = grow(colBrk);
      colMn = grow(colMn); colMx = grow(colMx); colLoFirst = grow(colLoFirst);
    }
    const k = colCount++;
    colDev[k] = dev;
    colX[k] = (dev + 0.5) / dpr;
    colFy[k] = fy;
    colLy[k] = ly;
    colTop[k] = !brk && prevLast < lo ? prevLast : lo;
    colBot[k] = !brk && prevLast > hi ? prevLast : hi;
    colBrk[k] = brk ? 1 : 0;
    colMn[k] = lo;
    colMx[k] = hi;
    colLoFirst[k] = loFirst ? 1 : 0;
  }

  // Per-column first, lowest, highest and last value of the visible points.
  // With a block summary (sum), a whole block of finite values that falls in one column is read from the summary.
  // The cost is O(n / BLOCK + columns * BLOCK).
  function traceColumns(L, ys, nulls, sum) {
    const xs = opts.xs;
    const { i0, i1, left, plotW, kx, ky, xLo, yMin } = L;
    const category = opts.xKind === "category";
    const bottom = L.top + L.plotH;
    const breakAll = !nulls;
    const gl = category ? Infinity : gapLimit();
    const xDev = (i) => Math.floor((left + ((category ? i : xs[i]) - xLo) * kx) * dpr);
    // A block summary does not know the gaps between its points. A series with gaps is read point by point.
    const useBlocks = !!sum && gl === Infinity && i1 - i0 > plotW * dpr * BLOCK * 2;
    colCount = 0;
    let col = -Infinity, fy = 0, lo = 0, hi = 0, ly = 0;
    let loSeq = 0, hiSeq = 0, seq = 0; // hune: the order of the extremes inside a column
    let has = false;
    let brk = true;
    let prevLast = NaN;
    let read = 0;
    const flush = () => {
      if (!has) return;
      column(col, fy, lo, hi, ly, brk, prevLast, loSeq <= hiSeq);
      brk = false;
      prevLast = ly;
      has = false;
    };
    let i = i0;
    while (i < i1) {
      if (useBlocks && (i & (BLOCK - 1)) === 0 && i + BLOCK <= i1 && sum.clean[i >> BLOCK_SHIFT]) {
        const e = i + BLOCK;
        const c = xDev(i);
        if (c === xDev(e - 1)) {
          const b = i >> BLOCK_SHIFT;
          const top = bottom - (sum.maxs[b] - yMin) * ky;
          const low = bottom - (sum.mins[b] - yMin) * ky;
          if (c !== col || !has) {
            flush();
            col = c;
            fy = bottom - (ys[i] - yMin) * ky;
            lo = top;
            hi = low;
            loSeq = 0; hiSeq = 1; seq = 2;
            has = true;
          } else {
            if (top < lo) { lo = top; loSeq = seq++; }
            if (low > hi) { hi = low; hiSeq = seq++; }
          }
          ly = bottom - (ys[e - 1] - yMin) * ky;
          read += 3;
          i = e;
          continue;
        }
      }
      const v = ys[i];
      read++;
      if (v !== v) {
        if (breakAll || nulls[i]) { flush(); brk = true; }
        i++;
        continue;
      }
      if (i > i0 && xs[i] - xs[i - 1] > gl) { flush(); brk = true; }
      const c = xDev(i);
      const y = bottom - (v - yMin) * ky;
      if (c !== col || !has) {
        flush();
        col = c;
        fy = lo = hi = ly = y;
        loSeq = hiSeq = 0; seq = 1;
        has = true;
      } else {
        if (y < lo) { lo = y; loSeq = seq; }
        if (y > hi) { hi = y; hiSeq = seq; }
        seq++;
        ly = y;
      }
      i++;
    }
    flush();
    counters.tracedPoints += read;
  }

  // A dense line is made by traceColumns().
  // hune: one continuous polyline goes through every column, as the linear path of uPlot
  // (Grafana) does: to the first value of the column, to its two extremes in the order in which they occur, to its last value,
  // then to the next column. A NULL gap (colBrk) starts a new run. The fill under the line follows the same vertices.
  // Adds the vertices of the columns a to b-1 (one run) to the current path; the pen moves to the first vertex.
  function columnsPath(ctx, a, b) {
    let px = NaN, py = NaN;
    let n = 0;
    const to = (x, y) => {
      if (x === px && y === py) return;
      if (n === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
      px = x;
      py = y;
      n++;
    };
    for (let k = a; k < b; k++) {
      const x = colX[k];
      to(x, colFy[k]);
      const e1 = colLoFirst[k] ? colMn[k] : colMx[k];
      const e2 = colLoFirst[k] ? colMx[k] : colMn[k];
      if (e1 !== py) to(x, e1);
      if (e2 !== py) to(x, e2);
      if (colLy[k] !== py) to(x, colLy[k]);
    }
    return n;
  }

  function drawColumns(ctx, L, s, c, alpha, wash) {
    const n = colCount;
    const lw = seriesWidth(s, 1.5);
    if (wash) {
      washedSeries++;
      // The wash under a single series, along the polyline.
      const baseY = Math.min(L.top + L.plotH, Math.max(L.top, L.yOf(Math.max(L.yMin, Math.min(0, L.yMax)))));
      ctx.beginPath();
      for (let a = 0; a < n;) {
        let b = a + 1;
        while (b < n && !colBrk[b]) b++;
        if (b - a > 1) {
          columnsPath(ctx, a, b);
          ctx.lineTo(colX[b - 1], baseY);
          ctx.lineTo(colX[a], baseY);
          ctx.closePath();
        }
        a = b;
      }
      ctx.fillStyle = paintOf(s, "fill", ctx, L, c, wash === "auto" ? 0 : wash * alpha) || washStyle(ctx, L, c, alpha, wash); // hune: E43
      ctx.fill();
    }
    ctx.beginPath();
    runs.length = 0;
    let vertices = 0;
    for (let a = 0; a < n;) {
      let b = a + 1;
      while (b < n && !colBrk[b]) b++;
      runs.push(vertices);
      const m = columnsPath(ctx, a, b);
      // A run of one column with one value is a lone value: a mark of the width of the line.
      if (m === 1) ctx.lineTo(colX[a] + 0.01, colFy[a]);
      vertices += m;
      a = b;
    }
    if (vertices && lw > 0) {
      ctx.strokeStyle = paintOf(s, "line", ctx, L, c, alpha) || rgba(c, alpha); // hune: E43
      ctx.lineWidth = lw;
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      strokeDashed(ctx, s);
    }
    return vertices;
  }

  const runEnd = (r) => (r + 1 < runs.length ? runs[r + 1] : vCount);

  // Line interpolation (opts.interpolation): "linear" (the default), "smooth" (a monotone cubic with no
  // overshoot past the data), "stepBefore" (the value of a point applies back to the previous one), and
  // "stepAfter" (the value of a point applies until the next one).
  const interpolation = () => (opts.interpolation === "smooth" || opts.interpolation === "stepBefore" || opts.interpolation === "stepAfter" ? opts.interpolation : "linear");
  let secants = new Float64Array(0);
  let tangents = new Float64Array(0);

  // The engine adds the vertices a to b-1 of X and Y to the path, forward or backward. An area's base comes back this way.
  // With `join`, the pen is already where the first vertex is. Otherwise, the pen moves there.
  function runTo(ctx, X, Y, a, b, back, join) {
    const first = back ? b - 1 : a;
    if (join) ctx.lineTo(X[first], Y[first]); else ctx.moveTo(X[first], Y[first]);
    if (b - a < 2) return;
    const mode = interpolation();
    if (mode === "smooth") { smoothRun(ctx, X, Y, a, b, back); return; }
    const step = back ? -1 : 1;
    const last = back ? a : b - 1;
    for (let k = first; k !== last; k += step) {
      const next = k + step;
      const lo = Math.min(k, next), hi = Math.max(k, next);
      if (mode === "stepBefore") ctx.lineTo(X[lo], Y[hi]);
      else if (mode === "stepAfter") ctx.lineTo(X[hi], Y[lo]);
      ctx.lineTo(X[next], Y[next]);
    }
  }

  // Fritsch-Butland tangents keep the direction of the data. So, the curve never swings past a value.
  function smoothRun(ctx, X, Y, a, b, back) {
    const n = b - a;
    if (secants.length < n) { secants = new Float64Array(Math.max(n, 2 * secants.length)); tangents = new Float64Array(secants.length); }
    for (let j = 0; j < n - 1; j++) {
      const dx = X[a + j + 1] - X[a + j];
      secants[j] = dx > 0 ? (Y[a + j + 1] - Y[a + j]) / dx : 0;
    }
    tangents[0] = secants[0];
    tangents[n - 1] = secants[n - 2];
    for (let j = 1; j < n - 1; j++) {
      const d0 = secants[j - 1], d1 = secants[j];
      if (d0 * d1 <= 0) { tangents[j] = 0; continue; }
      const h0 = X[a + j] - X[a + j - 1], h1 = X[a + j + 1] - X[a + j];
      tangents[j] = 3 * (h0 + h1) / ((2 * h1 + h0) / d0 + (h1 + 2 * h0) / d1);
    }
    const curve = (j) => {
      const x0 = X[a + j], y0 = Y[a + j], x1 = X[a + j + 1], y1 = Y[a + j + 1];
      const t = (x1 - x0) / 3;
      const c0x = x0 + t, c0y = y0 + tangents[j] * t, c1x = x1 - t, c1y = y1 - tangents[j + 1] * t;
      if (back) ctx.bezierCurveTo(c1x, c1y, c0x, c0y, x0, y0);
      else ctx.bezierCurveTo(c0x, c0y, c1x, c1y, x1, y1);
    };
    if (back) for (let j = n - 2; j >= 0; j--) curve(j);
    else for (let j = 0; j < n - 1; j++) curve(j);
  }

  function pathLine(ctx) {
    ctx.beginPath();
    for (let r = 0; r < runs.length; r++) runTo(ctx, vx, vy, runs[r], runEnd(r), false, false);
  }

  // Closed shapes between the line and its base. The base is an array, or one y.
  function pathFill(ctx, baseY) {
    ctx.beginPath();
    for (let r = 0; r < runs.length; r++) {
      const a = runs[r], b = runEnd(r);
      if (b - a < 2) continue;
      runTo(ctx, vx, vy, a, b, false, false);
      if (baseY == null) runTo(ctx, vx, vb, a, b, true, true);
      else { ctx.lineTo(vx[b - 1], baseY); ctx.lineTo(vx[a], baseY); }
      ctx.closePath();
    }
  }

  // The width of a line is opts.lineWidth. Otherwise, it is the width of the mark.
  const lineW = (own) => (opts.lineWidth > 0 ? opts.lineWidth : own);
  // hune: E14, F17. The width of the line of a series: its own `width` (0 draws no line), else the width of the chart.
  const seriesWidth = (s, own) => (s && Number.isFinite(s.width) && s.width >= 0 ? s.width : lineW(own));

  // hune: E43. `s.paint(part, ctx, L, c, alpha)` gives the style of the fill ("fill") or of the line and the points ("line") of a series: a gradient
  // of the host (9.19, the option `gradient_mode`), or null for the plain colour. `L` is the view of the scale of the series. An alpha of 0 has no paint.
  const paintOf = (s, part, ctx, L, c, alpha) => (typeof s.paint === "function" && (alpha > 0 || part === "line") ? s.paint(part, ctx, L, c, alpha) : null);

  // The fill under a line: opts.fillOpacity (0..1) applies to every line.
  // When it is unset, a lone series ("auto") gets a light wash. Each line of an area that is not stacked gets 0.3.
  function washOf(unstackedArea, s) {
    if (s && Number.isFinite(s.fill)) return s.fill; // hune: E14, the fill of the series
    if (Number.isFinite(opts.fillOpacity)) return opts.fillOpacity;
    if (unstackedArea) return 0.3;
    return opts.fill !== false && visibleSeries().length === 1 ? "auto" : 0;
  }

  function washStyle(ctx, L, c, alpha, wash) {
    if (wash !== "auto") return rgba(c, wash * alpha);
    const grad = ctx.createLinearGradient(0, L.top, 0, L.top + L.plotH);
    grad.addColorStop(0, rgba(c, 0.14 * alpha));
    grad.addColorStop(1, rgba(c, 0));
    return grad;
  }

  function drawSeries(ctx, L, s) {
    if (s.negate && !s.derived) s = negSeries(s); // hune: E14. The series draws -v.
    const c = seriesColor(s);
    const alpha = alphaFor(s);
    let type = s.type || opts.type;
    const nulls = s.nulls || null;
    const st = stacks && stacks.get(s.id);
    if (type === "area" && st) {
      // Fill between the top and the base of this series. Then stroke the top.
      trace(L, st.top, nulls, st.base); // hune: E32, a top with no value (stackSign) breaks the line only at a NULL of `nulls`
      pathFill(ctx, null);
      const fa = (Number.isFinite(s.fill) ? s.fill : Number.isFinite(opts.fillOpacity) ? opts.fillOpacity : 0.3) * alpha;
      ctx.fillStyle = paintOf(s, "fill", ctx, L, c, fa) || rgba(c, fa); // hune: E43
      ctx.fill();
      const lwTop = seriesWidth(s, 1.25);
      if (lwTop > 0) {
        pathLine(ctx);
        ctx.strokeStyle = paintOf(s, "line", ctx, L, c, alpha) || rgba(c, alpha); // hune: E43
        ctx.lineWidth = lwTop;
        ctx.lineJoin = "round";
        ctx.lineCap = "butt";
        strokeDashed(ctx, s);
      }
      return vCount;
    }
    // An area that is not stacked: each series is a line with its own fill.
    const unstackedArea = type === "area";
    if (unstackedArea) type = "line";
    // A line with few values (few rows, or a sparse series between gaps) marks each value.
    // opts.showPoints controls this. "always" marks all values. "never" marks none.
    const maxPoints = L.plotW / POINTS_AUTO_SPACING;
    let count = 0;
    if (type === "line") for (let i = L.v0; i < L.v1 && count <= maxPoints; i++) if (s.values[i] === s.values[i]) count++;
    // hune: E14. `s.points` and `s.pointSize` are the marks of one series. They replace the options of the chart.
    const pointsOpt = s.points !== undefined && s.points !== null ? s.points : opts.showPoints;
    const pointSize = Number.isFinite(s.pointSize) ? s.pointSize : opts.pointSize;
    const marks = pointsOpt === "always" ? "always" : pointsOpt === "never" ? "never" : "auto";
    const showPoints = type === "points" || (type === "line" && (marks === "always" ? true : marks === "auto" && count > 0 && count <= maxPoints));
    if (showPoints && type === "line") markedSeries++;
    const radius = pointSize > 0 ? pointSize / 2 : 2.6;
    let drawn = 0;
    if (type === "line") {
      const wash = washOf(unstackedArea, s);
      trace(L, s.values, nulls, null, s.derived ? null : summaryOf(s));
      if (decimated) return drawColumns(ctx, L, s, c, alpha, wash);
      drawn = vCount;
      // Grafana's "opacity" gradient: a light wash under each line.
      if (wash) {
        washedSeries++;
        const baseY = Math.min(L.top + L.plotH, Math.max(L.top, L.yOf(Math.max(L.yMin, Math.min(0, L.yMax)))));
        pathFill(ctx, baseY);
        ctx.fillStyle = paintOf(s, "fill", ctx, L, c, wash === "auto" ? 0 : wash * alpha) || washStyle(ctx, L, c, alpha, wash); // hune: E43
        ctx.fill();
      }
      const lw = seriesWidth(s, 1.5);
      if (lw > 0) {
        pathLine(ctx);
        ctx.strokeStyle = paintOf(s, "line", ctx, L, c, alpha) || rgba(c, alpha); // hune: E43
        ctx.lineWidth = lw;
        // Use round joins only where angles show. A decimated line is mostly
        // vertical strokes. These rasterise much cheaper with bevels.
        ctx.lineJoin = decimated ? "bevel" : "round";
        ctx.lineCap = decimated ? "butt" : "round";
        strokeDashed(ctx, s);
      }
      // Isolated values (a run of one point between NULLs) still show.
      if (!showPoints && lw > 0) {
        ctx.beginPath();
        let singles = 0;
        for (let r = 0; r < runs.length; r++) {
          const a = runs[r];
          if (runEnd(r) - a !== 1) continue;
          // hune: a lone value is a mark of the width of the line (Grafana draws no dot at this density), not a 4.4 px dot.
          const dotR = Math.max(0.6, lw / 2);
          ctx.moveTo(vx[a] + dotR, vy[a]);
          ctx.arc(vx[a], vy[a], dotR, 0, Math.PI * 2);
          singles++;
        }
        if (singles) { ctx.fillStyle = paintOf(s, "line", ctx, L, c, alpha) || rgba(c, alpha); ctx.fill(); } // hune: E43
      }
    }
    if (showPoints) {
      const xs = opts.xs;
      const category = opts.xKind === "category";
      ctx.beginPath();
      let dots = 0;
      for (let i = L.v0; i < L.v1 && dots < 20000; i++) {
        const v = s.values[i];
        if (v !== v) continue;
        const x = L.xOf(category ? i : xs[i]);
        const y = L.yOf(v);
        ctx.moveTo(x + radius, y);
        ctx.arc(x, y, radius, 0, Math.PI * 2);
        dots++;
      }
      ctx.fillStyle = paintOf(s, "line", ctx, L, c, alpha) || rgba(c, alpha); // hune: E43, the points take the paint of the line (Grafana)
      ctx.fill();
      if (type === "points") drawn = dots;
    }
    return drawn;
  }

  // Bars are centred on x. Groups are side by side, and the series of a group are stacked.
  // Past one bar per 3px, each slot keeps its tallest stack. Bars are 78 %
  // opaque, with a 1 px gap between neighbours.
  const BAR_ALPHA = 0.78;
  const BAR_GAP_PX = 1;
  const barOffsets = new Map();
  function drawBars(ctx, L, vis, perSeries, only = null) {
    if (!only) barOffsets.clear(); // hune: E14. `only` draws some of the bar series (a chart in series order); the offsets of the others stay.
    const xs = opts.xs;
    // hune: a time axis draws the bars next to the range, cut by the plot (E10).
    const { plotW } = L;
    const v0 = opts.xKind === "time" ? L.i0 : L.v0, v1 = opts.xKind === "time" ? L.i1 : L.v1;
    const n = v1 - v0;
    if (!n || !vis.length) return 0;
    const groups = [];
    for (const s of vis) {
      // hune: E23, `barOverlap`
      const key = isStacked() ? (s.group == null ? "" : String(s.group)) : opts.barOverlap ? "" : s.id;
      if (!groups.includes(key)) groups.push(key);
    }
    let slotPx;
    if (opts.xKind === "category") slotPx = L.kx;
    // hune: E24, `barSlot` (x units) and `barDecimate: false`
    else slotPx = opts.barSlot > 0 ? opts.barSlot * L.kx : n > 1 ? minGap(v0, v1) * L.kx : plotW / 2;
    const maxBars = Math.max(1, Math.floor(plotW / 3));
    let picks = null;
    if (n > maxBars && opts.barDecimate !== false) {
      picks = [];
      const bucketW = plotW / maxBars;
      let bucket = -1, best = -1, bestTotal = -Infinity;
      for (let i = v0; i < v1; i++) {
        const b = Math.floor((L.xOf(opts.xKind === "category" ? i : xs[i]) - L.left) / bucketW);
        let total = 0;
        for (const s of vis) {
          const st = stacks && stacks.get(s.id);
          const v = st ? st.top[i] - st.base[i] : drawnValues(s)[i];
          if (v === v) total += Math.abs(v);
        }
        if (b !== bucket) {
          if (best >= 0) picks.push(best);
          bucket = b; best = i; bestTotal = total;
        } else if (total > bestTotal) { best = i; bestTotal = total; }
      }
      if (best >= 0) picks.push(best);
      slotPx = Math.min(slotPx, bucketW);
    }
    // Adjacent bars keep a 1 px gap at least, however dense.
    // hune: `barMaxWidth` (px) replaces the cap of 64 px (56 px on a time axis) of a bar, and `barInnerRatio` is the share of its slot that a bar
    // fills inside a group of several bars (Grafana: group width, then bar width) (E10).
    const barCap = opts.barMaxWidth > 0 ? opts.barMaxWidth : opts.xKind === "category" ? 64 : 56;
    const barW = Math.max(1, Math.min(barCap, slotPx * barRatio(), slotPx >= 2 ? slotPx - BAR_GAP_PX : slotPx));
    const clusterW = barW / groups.length;
    const inner = opts.barInnerRatio > 0 && opts.barInnerRatio <= 1 ? opts.barInnerRatio : null;
    const drawW = groups.length > 1 ? (inner !== null ? Math.max(1, clusterW * inner) : Math.max(1, clusterW - Math.min(2, clusterW * 0.15))) : barW;
    let drawnMax = 0;
    // hune: `barLabel(ctx, { x, y, w, i, series, value, negative }, L)` draws the value of each bar after all bars are drawn (E10, the value labels of the bar chart).
    const labelBars = typeof opts.barLabel === "function" ? [] : null;
    for (const s of only || vis) {
      const c = seriesColor(s);
      const st = stacks && stacks.get(s.id);
      const V = viewFor(L, s); // hune: E13, the scale of the series
      const dvs = drawnValues(s); // hune: E14
      const offset = (groups.indexOf(isStacked() ? (s.group == null ? "" : String(s.group)) : opts.barOverlap ? "" : s.id) - (groups.length - 1) / 2) * clusterW;
      ctx.beginPath();
      let count = 0;
      // hune: E26. `barInset`: the line of a bar stays inside the bar at its top and its bottom (Grafana), so each end loses half the line.
      const ins = opts.barInset ? (Number.isFinite(s.width) ? s.width : opts.barLineWidth || 0) / 2 : 0;
      const insX = opts.barInset === "box" ? ins : 0; // hune: E34, "box": the line stays inside the bar at its sides too (the bar chart of Grafana)
      // hune: E44. `barAlign` -1 or 1 (the option `bar_alignment`): the bar ends half its line before its x, or starts half its line after it (Grafana).
      const lwBar = Number.isFinite(s.width) ? s.width : opts.barLineWidth > 0 ? opts.barLineWidth : 0;
      const shift = opts.barAlign === -1 || opts.barAlign === 1 ? opts.barAlign * (drawW + lwBar) / 2 : 0;
      const each = (i) => {
        const top = st ? st.top[i] : dvs[i];
        const bottom = st ? st.base[i] : 0;
        if (!(top === top) || top === bottom) return;
        const x = L.xOf(opts.xKind === "category" ? i : xs[i]) + offset - drawW / 2 + shift; // hune: E44
        const yTop = V.yOf(Math.max(top, bottom));
        const yBottom = V.yOf(Math.min(top, bottom));
        ctx.rect(x + insX, yTop + ins, Math.max(1 / dpr, drawW - 2 * insX), Math.max(1 / dpr, yBottom - yTop - 2 * ins));
        if (labelBars && !st) labelBars.push({ x, y: top >= bottom ? yTop : yBottom, w: drawW, i, series: s, value: top, negative: top < bottom });
        count++;
      };
      if (picks) for (const i of picks) each(i); else for (let i = v0; i < v1; i++) each(i);
      // hune: `barFillAlpha` is the fill opacity of a bar (the option `fill_opacity`). `barLineWidth` draws a line of that width around each bar,
      // in the colour of the series (Grafana draws both) (E10).
      const fillAlpha = Number.isFinite(s.fill) ? s.fill : opts.barFillAlpha >= 0 ? opts.barFillAlpha : BAR_ALPHA; // hune: E14, `s.fill` and `s.width` are the fill and the outline of one bar series
      const outline = Number.isFinite(s.width) ? s.width : opts.barLineWidth;
      ctx.fillStyle = paintOf(s, "fill", ctx, V, c, fillAlpha * alphaFor(s)) || rgba(c, fillAlpha * alphaFor(s)); // hune: E43
      if (fillAlpha > 0) ctx.fill(); // hune: E34, no fill at the opacity 0, as Grafana
      if (outline > 0) {
        ctx.lineWidth = outline;
        ctx.strokeStyle = paintOf(s, "line", ctx, V, c, alphaFor(s)) || rgba(c, alphaFor(s)); // hune: E43
        ctx.stroke();
      }
      drawnMax = Math.max(drawnMax, count);
      perSeries[s.label] = { points: count, runs: count ? 1 : 0 };
      barOffsets.set(s.id, offset);
    }
    if (labelBars) for (const b of labelBars) opts.barLabel(ctx, b, L);
    return drawnMax;
  }

  // --- cursor overlay ------------------------------------------------------------

  function nearestIndex(px) {
    const L = layout;
    const xs = opts.xs;
    const n = xs.length;
    if (!n) return -1;
    if (opts.xKind === "category") return Math.max(0, Math.min(n - 1, Math.round(L.xAt(px))));
    const x = L.xAt(px);
    const i = lowerBound(xs, x);
    if (i <= 0) return 0;
    if (i >= n) return n - 1;
    return x - xs[i - 1] <= xs[i] - x ? i - 1 : i;
  }

  // The x readout is the tooltip title and the x badge.
  // hune: E36. No range of a bucket (bucketMs, bucketAlign): Hune never sets these options.
  function xReadout(i) {
    if (typeof opts.xReadout === "function") return opts.xReadout(i);
    if (opts.xKind === "category") return String(opts.categories[i] ?? "");
    const x = opts.xs[i];
    if (opts.xKind === "time") return formatInstant(x, opts.xFractionDigits, clockOf()); // hune: E42
    if (opts.xKind === "index") return `Row ${formatExact(x)}`;
    return formatExact(x);
  }

  function scheduleOverlay() {
    overlayFrame();
  }

  // The cursor is DOM, like uPlot's. Dashed lines, series points and the axis readouts move with compositor-only transforms.
  // So, a hover never repaints or uploads a canvas, whatever the devicePixelRatio.
  const show = (el, on) => { if (el.hidden === on) el.hidden = !on; };
  const place = (el, x, y) => { el.style.transform = `translate(${Math.round(x * dpr) / dpr}px, ${Math.round(y * dpr) / dpr}px)`; };
  let lastBadgeX = "";
  let lastBadgeY = "";

  // Badge and tooltip sizes are set by a ResizeObserver, after the browser's own layout.
  // So, a cursor move never forces a synchronous layout.
  // A changed size re-places them on the next frame. Only the first sight of an element measures it directly.
  const sizes = new WeakMap();
  const sizeObserver = typeof ResizeObserver === "function"
    ? new ResizeObserver((entries) => {
      let changed = false;
      for (const entry of entries) {
        const box = entry.borderBoxSize && entry.borderBoxSize[0];
        const w = box ? box.inlineSize : entry.target.offsetWidth;
        const h = box ? box.blockSize : entry.target.offsetHeight;
        if (!w && !h) continue; // hidden: keep the last size
        const old = sizes.get(entry.target);
        if (old && old.w === w && old.h === h) continue;
        sizes.set(entry.target, { w, h });
        changed = true;
      }
      if (!changed || destroyed) return;
      if (contentTip && !tooltipEl.hidden) placeContentTip();
      else if (cursor && !tooltipEl.hidden && tooltipKey && tooltipKey !== "marker") renderTooltip();
      scheduleOverlay();
    })
    : null;
  if (sizeObserver) for (const el of [tooltipEl, xBadge, yBadge, selectBadge]) sizeObserver.observe(el);

  function measured(el) {
    let size = sizeObserver ? sizes.get(el) : null;
    if (!size) {
      size = { w: el.offsetWidth, h: el.offsetHeight };
      if (sizeObserver && (size.w || size.h)) sizes.set(el, size);
    }
    return size;
  }

  function placeBadge(el, x, y, align, lo, hi) {
    const w = measured(el).w;
    let bx = align === "center" ? x - w / 2 : align === "right" ? x - w : x;
    bx = Math.max(lo, Math.min(hi - w, bx));
    place(el, bx, y);
  }

  function dotFor(k) {
    while (cursorDots.length <= k) {
      const dot = document.createElement("i");
      dot.className = "chartCore__dot";
      dot.hidden = true;
      cursorEl.appendChild(dot);
      cursorDots.push(dot);
    }
    return cursorDots[k];
  }

  function drawOverlay() {
    if (!layout || released) return;
    const L = layout;
    const selecting = !!(drag && drag.active);
    show(selectEl, selecting);
    show(selectBadge, selecting && opts.xKind !== "category");
    if (selecting) {
      const a = Math.max(L.left, Math.min(drag.x0, drag.x1));
      const b = Math.min(L.left + L.plotW, Math.max(drag.x0, drag.x1));
      selectEl.style.width = `${Math.max(0, b - a)}px`;
      selectEl.style.height = `${L.plotH}px`;
      place(selectEl, a, L.top);
      const lo = L.xAt(a), hi = L.xAt(b);
      const fmt = (v) => (opts.xKind === "time" ? formatInstant(v, opts.xFractionDigits, clockOf()).slice(11) : formatValue(Number(v.toPrecision(8))));
      if (opts.xKind !== "category") {
        selectBadge.textContent = `${fmt(lo)} \u2013 ${fmt(hi)}`;
        placeBadge(selectBadge, (a + b) / 2, L.top + 4, "center", L.left, L.left + L.plotW);
      }
    }
    const i = cursorIndex;
    const active = !!(cursor && i >= 0);
    const synced = !active && syncedX != null && opts.xKind !== "category";
    let x = 0;
    if (active) x = L.xOf(opts.xKind === "category" ? i : opts.xs[i]);
    else if (synced) x = L.xOf(syncedX);
    const xIn = (active || synced) && x >= L.left - 0.5 && x <= L.left + L.plotW + 0.5;
    show(xLine, xIn);
    if (xIn) {
      xLine.style.height = `${L.plotH}px`;
      place(xLine, x, L.top);
    }
    show(yLine, active);
    show(xBadge, active);
    // hune: E13. The readout of the y axis follows the left scale. When the left axis does not show, it follows the right scale, at the right of the plot.
    const badgeOnRight = !leftShown() && rightShown() && !!L.y2;
    show(yBadge, active && (leftShown() || badgeOnRight));
    let used = 0;
    if (active) {
      const py = Math.max(L.top, Math.min(L.top + L.plotH, cursor.py));
      yLine.style.width = `${L.plotW}px`;
      place(yLine, L.left, py);
      // Points on every visible series at that x.
      for (const s of visibleSeries()) { // hune: E42, no cursorPoints
        if (s.xs || s.tooltipHidden) continue; // hune: a series with the flag tooltipHidden has no dot (E9, F03)
        const st = stacks && stacks.get(s.id);
        const v = st ? st.top[i] : drawnValues(s)[i];
        if (!(v === v) || !(s.values[i] === s.values[i])) continue;
        const dx = opts.type === "bar" ? (barOffsets.get(s.id) || 0) : 0;
        const y = viewFor(L, s).yOf(v); // hune: E13, E14. The dot sits on the scale of the series, at the drawn value.
        if (y < L.top - 1 || y > L.top + L.plotH + 1) continue;
        const dot = dotFor(used++);
        const bg = rgba(seriesColor(s), 1);
        if (dot._bg !== bg) { dot.style.background = bg; dot._bg = bg; }
        dot.classList.toggle("is-nearest", s.id === cursor.nearest);
        place(dot, x + dx, y);
        show(dot, true);
      }
      // Exact readouts on both axes.
      const xText = xReadout(i);
      if (xText !== lastBadgeX) { xBadge.textContent = xText; lastBadgeX = xText; }
      placeBadge(xBadge, x, L.top + L.plotH + 2, "center", 0, L.width);
      const bv = badgeOnRight ? L.y2 : L;
      const bo = scaleOpts(badgeOnRight ? "y2" : "y");
      const yd = Math.min(10, decimalsFor(bv.yStep) + 2);
      const yText = typeof bo.formatY === "function" ? bo.formatY(bv.yAt(py)) : formatExact(Number(bv.yAt(py).toFixed(yd)));
      if (yText !== lastBadgeY) { yBadge.textContent = yText; lastBadgeY = yText; }
      if (badgeOnRight) placeBadge(yBadge, L.left + L.plotW + 3, py - 8.5, "left", L.left + L.plotW + 2, L.width);
      else placeBadge(yBadge, L.left - 3, py - 8.5, "right", 0, L.left - 2);
      root.dataset.cursorIndex = String(i);
      root.dataset.cursorX = xText;
      root.dataset.cursorPx = String(Math.round(x * 100) / 100);
      // The pointer position this cursor was computed from (plot css px).
      root.dataset.pointerPx = String(Math.round(cursor.px * 100) / 100);
    } else {
      delete root.dataset.cursorIndex;
      delete root.dataset.cursorX;
      delete root.dataset.cursorPx;
      delete root.dataset.pointerPx;
    }
    for (let k = used; k < cursorDots.length; k++) show(cursorDots[k], false);
  }

  // --- tooltip ------------------------------------------------------------------

  function nearestSeries(i, py) {
    let best = null, bestD = Infinity;
    for (const s of visibleSeries()) {
      if (s.xs || s.tooltipHidden) continue; // hune: E9, F03
      const st = stacks && stacks.get(s.id);
      const v = st ? st.top[i] : drawnValues(s)[i];
      if (!(v === v) || !(s.values[i] === s.values[i])) continue;
      const d = Math.abs(viewFor(layout, s).yOf(v) - py);
      if (d < bestD) { bestD = d; best = s.id; }
    }
    return best;
  }

  function renderTooltip() {
    if (!cursor || cursorIndex < 0 || opts.tooltip === "none") { tooltipEl.hidden = true; tooltipKey = ""; return; }
    const i = cursorIndex;
    const key = `${i}|${cursor.nearest}|${opts.tooltip}`;
    if (key !== tooltipKey) {
      tooltipKey = key;
      const rowsHtml = [];
      let total = 0, any = false;
      for (const s of tooltipSeries(i)) {
        if (s.xs || s.tooltipHidden || (opts.tooltip === "single" && s.id !== cursor.nearest)) continue;
        const v = s.values[i];
        const has = v === v;
        if (has && v === 0 && opts.tooltipHideZeros === true) continue; // hune: hide_zeros removes the rows that equal 0 (E9, F20)
        const isNull = !has && (s.nulls === null || s.nulls === undefined || s.nulls[i]);
        if (!has && !isNull) continue; // no row for this series at this x
        if (has) { total += v; any = true; }
        const c = seriesColor(s);
        const text = has ? (typeof opts.formatValue === "function" ? opts.formatValue(v, s) : formatValue(v)) : "NULL";
        rowsHtml.push(`<span class="chartCore__tipRow${has ? "" : " is-empty"}${s.id === cursor.nearest ? " is-nearest" : ""}"><i style="background:${rgba(c)}"></i><em>${esc(s.label)}</em><b>${esc(text)}</b></span>`);
      }
      if (isStacked() && opts.tooltip !== "single" && rowsHtml.length > 1 && any) {
        rowsHtml.push(`<span class="chartCore__tipRow chartCore__tipRow--total"><i></i><em>Total</em><b>${esc(typeof opts.formatValue === "function" ? opts.formatValue(total, null) : formatValue(total))}</b></span>`);
      }
      if (!rowsHtml.length) rowsHtml.push(`<span class="chartCore__tipRow is-empty"><i></i><em>No value</em><b>\u2014</b></span>`);
      // hune: E42. No tooltipMaxRows, tooltipFooter or tooltipTitle: Hune never sets them.
      tooltipEl.innerHTML = `<strong>${esc(xReadout(i))}</strong>${rowsHtml.join("")}`;
      tooltipEl.dataset.index = String(i);
      tooltipEl.hidden = false;
    }
    tooltipSize = measured(tooltipEl);
    tooltipEl.hidden = false;
    const L = layout;
    const x = L.xOf(opts.xKind === "category" ? i : opts.xs[i]);
    // hune: the card sits 10 px right of and 10 px under the cursor (Grafana), and flips to the other side at an edge.
    let lx = x + 10;
    if (lx + tooltipSize.w > L.width - 4) lx = x - 10 - tooltipSize.w;
    lx = Math.max(4, lx);
    let ty = cursor.py + 10;
    if (ty + tooltipSize.h > L.height) ty = cursor.py - 10 - tooltipSize.h;
    ty = Math.max(0, Math.min(L.height - tooltipSize.h, ty));
    tooltipEl.style.transform = `translate(${Math.round(lx)}px, ${Math.round(ty)}px)`;
  }

  // --- pointer --------------------------------------------------------------------

  function localPoint(ev) {
    const box = overCanvas.getBoundingClientRect();
    return { px: ev.clientX - box.left, py: ev.clientY - box.top };
  }

  function inPlot(p) {
    const L = layout;
    return L && p.px >= L.left - 2 && p.px <= L.left + L.plotW + 2 && p.py >= L.top - 2 && p.py <= L.top + L.plotH + 2;
  }

  function moveCursor(p) {
    if (!layout || !opts.xs.length) return; // hune: E18, no 2-D brush.
    if (!inPlot(p) && !(drag && drag.active)) { leaveCursor(); return; }
    if (!(drag && drag.active) && showPick(p)) return;
    const px = Math.max(layout.left, Math.min(layout.left + layout.plotW, p.px));
    cursorIndex = nearestIndex(px);
    cursor = { px, py: p.py, nearest: cursorIndex >= 0 ? nearestSeries(cursorIndex, p.py) : null };
    if (drag) {
      drag.x1 = px;
      if (!drag.active && Math.abs(drag.x1 - drag.x0) >= DRAG_MIN_PX) drag.active = true;
    }
    if (drag && drag.active) tooltipEl.hidden = true; else renderTooltip();
    scheduleOverlay();
    broadcast(cursorIndex >= 0 && opts.xKind !== "category" ? opts.xs[cursorIndex] : null);
  }

  function leaveCursor() {
    clearPick();
    if (!cursor && tooltipEl.hidden) return;
    cursor = null;
    cursorIndex = -1;
    tooltipEl.hidden = true;
    tooltipKey = "";
    scheduleOverlay();
    broadcast(null);
  }

  function broadcast(x) {
    if (!opts.syncKey) return;
    const group = syncGroups.get(opts.syncKey);
    if (!group) return;
    for (const other of group) if (other !== api && other.xKind() === opts.xKind) other.syncCursor(x);
  }

  let pendingMove = null;
  const moveFrame = util.rafOnce(() => {
    if (!pendingMove) return;
    moveCursor(pendingMove);
    // This frame already runs. The crosshair and readouts move with the
    // tooltip now, not one frame behind it.
    if (overlayFrame.pending()) { overlayFrame.cancel(); drawOverlay(); }
  });
  // The selection follows every move (cheap); the cursor redraws once per frame.
  const trackDrag = (p) => {
    if (!drag || !layout) return;
    drag.x1 = Math.max(layout.left, Math.min(layout.left + layout.plotW, p.px));
    if (!drag.active && Math.abs(drag.x1 - drag.x0) >= DRAG_MIN_PX) drag.active = true;
  };
  overCanvas.addEventListener("pointermove", (ev) => {
    pendingMove = localPoint(ev);
    trackDrag(pendingMove);
    moveFrame();
  });
  overCanvas.addEventListener("pointerleave", () => {
    if (drag) return;
    pendingMove = null;
    leaveCursor();
  });
  // A press on the plot starts a zoom gesture. It never selects text or starts a native drag.
  // A native drag would cancel the pointer stream.
  overCanvas.addEventListener("mousedown", (ev) => {
    if (ev.button !== 0) return;
    ev.preventDefault();
    const sel = window.getSelection ? window.getSelection() : null;
    if (sel && !sel.isCollapsed) sel.removeAllRanges();
    overCanvas.focus({ preventScroll: true });
  });
  overCanvas.addEventListener("dragstart", (ev) => ev.preventDefault());
  overCanvas.addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0 || !layout) return;
    const p = localPoint(ev);
    if (!inPlot(p)) return;
    // hune: E18, a press never starts a box of the 2-D brush.
    moveCursor(p);
    if (opts.zoomable === false || opts.xs.length < 2) return;
    drag = { x0: Math.max(layout.left, Math.min(layout.left + layout.plotW, p.px)), x1: p.px, active: false, id: ev.pointerId };
    try { overCanvas.setPointerCapture(ev.pointerId); } catch { /* capture is optional */ }
  });
  const endDrag = (ev, commit) => {
    if (!drag) return;
    if (ev && ev.type === "pointerup") trackDrag(localPoint(ev));
    const d = drag;
    drag = null;
    try { overCanvas.releasePointerCapture(d.id); } catch { /* already released */ }
    if (commit && d.active && layout) {
      const a = Math.min(d.x0, d.x1), b = Math.max(d.x0, d.x1);
      if (b - a >= DRAG_MIN_PX) {
        let lo = layout.xAt(a), hi = layout.xAt(b);
        if (opts.xKind === "category") { lo = Math.floor(lo + 0.5) - 0.5; hi = Math.ceil(hi - 0.5) + 0.5; }
        setZoom(lo, hi, true);
        return;
      }
    }
    scheduleOverlay();
    if (ev && ev.type === "pointerup") {
      const p = localPoint(ev);
      if (inPlot(p)) moveCursor(p); else leaveCursor();
    }
  };
  overCanvas.addEventListener("pointerup", (ev) => endDrag(ev, true));
  overCanvas.addEventListener("pointercancel", (ev) => endDrag(ev, false));
  overCanvas.addEventListener("lostpointercapture", () => { if (drag) endDrag(null, false); });
  overCanvas.addEventListener("dblclick", (ev) => {
    ev.preventDefault();
    if (zoom) resetZoom(true);
  });
  // Escape cancels a drag in progress. ns.layers does not see the key.
  const onKey = (ev) => {
    if (ev.key === "Escape" && drag) { ev.preventDefault(); drag = null; scheduleOverlay(); }
  };
  // Keyboard cursor: the readouts point by point, without a mouse.
  overCanvas.addEventListener("keydown", (ev) => {
    if (!layout || !opts.xs.length) return;
    const L = layout;
    const last = Math.min(opts.xs.length, L.v1) - 1;
    const first = Math.min(L.v0, last);
    let i = cursorIndex >= 0 ? cursorIndex : first - 1;
    if (ev.key === "ArrowRight") i += ev.shiftKey ? 10 : 1;
    else if (ev.key === "ArrowLeft") i -= ev.shiftKey ? 10 : 1;
    else if (ev.key === "Home") i = first;
    else if (ev.key === "End") i = last;
    else if (ev.key === "Escape") { if (cursorIndex < 0) return; ev.preventDefault(); leaveCursor(); return; }
    else return;
    ev.preventDefault();
    i = Math.max(first, Math.min(last, i));
    const px = L.xOf(opts.xKind === "category" ? i : opts.xs[i]);
    const s = visibleSeries().find((x) => !x.xs && x.values[i] === x.values[i]);
    const st = s && stacks && stacks.get(s.id);
    const py = s ? viewFor(L, s).yOf(st ? st.top[i] : drawnValues(s)[i]) : L.top + L.plotH / 2;
    moveCursor({ px, py });
    // A hover readout is not announced. The keyboard cursor readout is announced in one polite region.
    if (!tooltipEl.hidden) uiState?.announce?.(tooltipEl.textContent);
  });
  overCanvas.addEventListener("blur", () => { if (!drag) leaveCursor(); });
  document.addEventListener("keydown", onKey, true);

  // --- legend (Grafana: click isolates, Ctrl / Cmd+click toggles) --------------

  const LEGEND_COLUMNS = ["total", "min", "max", "mean", "last"];
  // hune: the 18 calculations of the format (E9, F20). The names of the heads are those of Grafana. The engine computes the first five.
  // The host computes the others (opts.legendCalc(series, name)).
  const LEGEND_HEADS = {
    total: "Total", min: "Min", max: "Max", mean: "Mean", last: "Last",
    last_not_null: "Last *", first: "First", first_not_null: "First *", count: "Count", median: "Median", std_dev: "Std Dev",
    variance: "Variance", range: "Range", diff: "Difference", diff_percent: "Difference %", delta: "Delta",
    distinct_count: "Distinct count", change_count: "Change count",
  };
  const NO_CALCS = { total: NaN, min: NaN, max: NaN, mean: NaN, last: NaN };

  // The columns of the table, in their order. opts.legendColumns keeps some of them, in the order that it gives (the order of the document, as Grafana does).
  // A chart whose values do not add up omits the Total.
  function legendColumns() {
    const asked = Array.isArray(opts.legendColumns) ? opts.legendColumns.filter((name, k, all) => Object.prototype.hasOwnProperty.call(LEGEND_HEADS, name) && all.indexOf(name) === k) : [];
    return asked.length ? asked : LEGEND_COLUMNS;
  }

  // hune: the value of a column for a series. The engine computes the first five (over the visible range). The host computes the others.
  function legendValue(s, name) {
    if (LEGEND_COLUMNS.includes(name)) return calcs(s)[name];
    return typeof opts.legendCalc === "function" ? opts.legendCalc(s, name) : NaN;
  }

  // hune: the sort of the legend table (E9, F20). `opts.legendSort` is { by: "name" | a calculation, desc } or null. A click on a head changes it for this chart only:
  // first up, then down, then the order of the series.
  let legendSortState = null;
  let legendSortSource;
  const legendSortNow = () => {
    if (legendSortSource !== opts.legendSort) {
      legendSortSource = opts.legendSort;
      legendSortState = opts.legendSort && opts.legendSort.by ? { by: opts.legendSort.by, desc: !!opts.legendSort.desc } : null;
    }
    return legendSortState;
  };
  const NAME_COLLATOR = new Intl.Collator("en", { sensitivity: "base" });
  // The series of the legend, in the order of the rows: the series with no `legendHidden` flag, sorted. A key that is null or NaN goes last in both directions.
  // Rows with equal keys keep the order of the series.
  function legendRows() {
    const rows = opts.series.map((s, index) => ({ s, index })).filter((r) => !r.s.legendHidden);
    const sort = legendSortNow();
    if (!sort) return rows;
    const keyOf = (r) => {
      if (sort.by === "name") return String(r.s.label);
      const v = legendValue(r.s, sort.by);
      return typeof v === "number" && Number.isFinite(v) ? v : null;
    };
    const keyed = rows.map((r) => ({ r, key: keyOf(r) }));
    keyed.sort((a, b) => {
      if (a.key === null || b.key === null) return a.key === b.key ? a.r.index - b.r.index : a.key === null ? 1 : -1;
      const c = sort.by === "name" ? NAME_COLLATOR.compare(a.key, b.key) : a.key - b.key;
      return c !== 0 ? (sort.desc ? -c : c) : a.r.index - b.r.index;
    });
    return keyed.map((k) => k.r);
  }

  // The table shows these values of a series over the visible range: the total, the min, the max, the mean and the last value.
  // A series with `weights` (one per point: the calls behind an average, the requests behind a rate) has a weighted mean.
  // This mean is the average of all the points that it represents. It is not the average of the averages.
  function calcs(s) {
    const L = layout;
    let min = Infinity, max = -Infinity, sum = 0, count = 0, last = NaN, weighted = 0, weight = 0;
    // `stats`: the values to read when the plot omits some. An example is a zero that a stack does not draw.
    const values = s.stats && s.stats.length >= s.values.length ? s.stats : s.values;
    const a = L && !s.xs ? L.v0 : 0, b = L && !s.xs ? L.v1 : values.length;
    const weights = s.weights && s.weights.length >= values.length ? s.weights : null;
    const summary = weights || values !== s.values ? null : summaryOf(s);
    if (summary) {
      const r = summaryRange(summary, values, a, b);
      for (let i = b - 1; i >= a; i--) if (values[i] === values[i]) { last = values[i]; break; }
      return r.count ? { total: r.total, min: r.min, max: r.max, mean: r.total / r.count, last } : NO_CALCS;
    }
    for (let i = a; i < b; i++) {
      const v = values[i];
      if (v !== v) continue;
      if (v < min) min = v;
      if (v > max) max = v;
      sum += v;
      count++;
      last = v;
      if (weights && weights[i] > 0) { weighted += weights[i] * v; weight += weights[i]; }
    }
    if (!count) return NO_CALCS;
    return { total: sum, min, max, mean: weights ? (weight > 0 ? weighted / weight : NaN) : sum / count, last };
  }

  // An entry is on when its series is shown. An entry is a row of the list, or a cell of the first column of the table.
  // hune: E41. No legend that filters: no legendPressed, legendDisabled, legendTitle or onLegendClick.
  function legendItemHtml(s, index, table = false, rightFirst = false) {
    const pressed = !hidden.has(s.id);
    const c = rgba(seriesColor(s));
    const title = `${s.label}\nClick: show only this series (again: show all)\nCtrl / Cmd+click: show or hide it`;
    // hune: E13, V2. A table legend ends the name of a series of the right scale with " (right y-axis)" (a span of 12 px in the muted colour).
    // A list legend has no mark: its items of the right scale go to the end of the row (data-scale, `is-right-first`).
    const mark = table && scaleOf(s) === "y2" ? `<span class="chartCore__axisMark"> (right y-axis)</span>` : "";
    const right = scaleOf(s) === "y2" ? ` data-scale="y2"` : "";
    return `<button type="button" class="chartCore__legendItem${s.dash ? " is-dashed" : ""}${rightFirst ? " is-right-first" : ""}" data-index="${index}" data-series="${esc(s.id)}"${right} aria-pressed="${pressed}" title="${esc(title)}"><i style="background-color:${c}"></i><span>${esc(s.label)}${mark}</span></button>`;
  }

  // hune: E41. No totals legend (seriesTotal, legendTotalItemHtml).

  // The engine rebuilds the legend markup only when it changes.
  // A resize, a theme change or streamed rows redraw the plot, not the legend.
  // The values of the total, min, max, mean and last table are written into its cells.
  let legendHtml = null;
  function setLegendHtml(html) {
    if (html === legendHtml) return false;
    counters.legendBuilds++;
    legendHtml = html;
    legendEl.innerHTML = html;
    return true;
  }

  // The legend shows by default when there is more than one series.
  // "always" shows it even for one series. false hides it.
  function renderLegend() {
    const shown = opts.legend === "always" || (!!opts.legend && opts.series.length > 1);
    if (!shown || !opts.series.length || !opts.series.some((s) => !s.legendHidden)) { setLegendHtml(""); legendEl.hidden = true; return; }
    legendEl.hidden = false;
    legendEl.dataset.mode = legendMode;
    const columns = legendColumns();
    // hune: E41. No button of the mode: the mode comes from the document (9.19).
    if (legendMode === "table") {
      // A cell uses the format of the chart values (formatValue). Otherwise, legendFormat(value, column, series) formats it. A mean of counts has decimals.
      const fmt = (v, column, s) => {
        if (v !== v) return "\u2014";
        if (typeof opts.legendFormat === "function") return opts.legendFormat(v, column, s);
        return typeof opts.formatValue === "function" ? opts.formatValue(v) : formatValue(v);
      };
      // hune: the rows follow the sort. The head of a column is a button that sorts (aria-sort). The first head reads "Name", as in Grafana.
      const order = legendRows();
      const sort = legendSortNow();
      const head = (name, text) => {
        const state = sort && sort.by === name ? (sort.desc ? "descending" : "ascending") : "none";
        return `<th scope="col" aria-sort="${state}"><button type="button" class="chartCore__legendSort" data-column="${esc(name)}" title="Sort by ${esc(text)}">${esc(text)}</button></th>`;
      };
      const rows = order.map(({ s, index }) => `<tr><th scope="row">${legendItemHtml(s, index, true)}</th>${columns.map(() => "<td></td>").join("")}</tr>`).join("");
      const heads = columns.map((name) => head(name, LEGEND_HEADS[name])).join("");
      setLegendHtml(`<div class="chartCore__legendTableWrap"><table class="chartCore__legendTable" aria-label="Values of the series"><thead><tr>${head("name", "Name")}${heads}</tr></thead><tbody>${rows}</tbody></table></div>`);
      const body = $(".chartCore__legendTable tbody", legendEl);
      order.forEach(({ s }, position) => {
        const row = body && body.rows[position];
        if (!row) return;
        columns.forEach((name, k) => {
          const cell = row.cells[k + 1];
          const text = fmt(legendValue(s, name), name, s);
          if (cell.textContent !== text) { cell.textContent = text; counters.legendUpdates++; }
        });
      });
    } else {
      // hune: E13. The items of the right scale go after the others.
      const listed = opts.series.map((s, index) => ({ s, index })).filter((r) => !r.s.legendHidden);
      const ordered = [...listed.filter((r) => scaleOf(r.s) !== "y2"), ...listed.filter((r) => scaleOf(r.s) === "y2")];
      const firstRight = ordered.findIndex((r) => scaleOf(r.s) === "y2");
      setLegendHtml(`<div class="chartCore__legendList">${ordered.map((r, k) => legendItemHtml(r.s, r.index, false, k === firstRight)).join("")}</div>`);
    }
  }

  // The mode of the legend, list or table. hune: E41, no preference and no onLegendMode.
  function setLegendMode(mode) {
    if (mode !== "list" && mode !== "table") return;
    const changed = mode !== legendMode;
    legendMode = mode;
    // A chart that has not drawn yet (no theme) writes its legend with its first draw.
    if (changed && theme) renderLegend();
  }

  function setHiddenInternal(next, focusIndex) {
    hidden = next;
    stacks = null;
    axisStamp++;
    tooltipKey = "";
    flushDraw(true);
    if (focusIndex != null) {
      const again = $(`.chartCore__legendItem[data-index="${focusIndex}"]`, legendEl);
      if (again) again.focus({ preventScroll: true });
    }
  }

  legendEl.addEventListener("click", (ev) => {
    // hune: a click on a head of the table sorts the rows (E9): up, down, then the order of the series.
    const sortHead = ev.target.closest(".chartCore__legendSort");
    if (sortHead) {
      const by = sortHead.dataset.column;
      const now = legendSortNow();
      if (!now || now.by !== by) legendSortState = { by, desc: false };
      else legendSortState = now.desc ? null : { by, desc: true };
      renderLegend();
      const again = legendEl.querySelector(`.chartCore__legendSort[data-column="${by}"]`);
      if (again) again.focus({ preventScroll: true });
      return;
    }
    const item = ev.target.closest(".chartCore__legendItem");
    if (!item) return;
    const index = Number(item.dataset.index);
    const s = opts.series[index];
    if (!s) return;
    const ids = opts.series.map((x) => x.id);
    const next = new Set(hidden);
    if (legendToggles(ev)) {
      if (next.has(s.id)) next.delete(s.id); else next.add(s.id);
      if (next.size === ids.length) next.delete(s.id);
    } else {
      const isolated = !hidden.has(s.id) && hidden.size === ids.length - 1;
      next.clear();
      if (!isolated) for (const id of ids) if (id !== s.id) next.add(id);
    }
    setHiddenInternal(next, index);
  });
  legendEl.addEventListener("pointerover", (ev) => {
    const item = ev.target.closest(".chartCore__legendItem");
    const s = item ? opts.series[Number(item.dataset.index)] : null;
    const next = s && !hidden.has(s.id) && visibleSeries().length > 1 ? s.id : null;
    if (next !== highlight) { highlight = next; scheduleDraw(); }
  });
  legendEl.addEventListener("pointerleave", () => {
    if (highlight) { highlight = null; scheduleDraw(); }
  });

  // --- zoom --------------------------------------------------------------------------

  function setZoom(lo, hi, fromUser = false) {
    const [fullLo, fullHi] = fullDomain();
    let a = Math.max(fullLo, Math.min(lo, hi));
    let b = Math.min(fullHi, Math.max(lo, hi));
    if (!(b > a)) return;
    if (a <= fullLo && b >= fullHi) { resetZoom(fromUser); return; }
    zoom = [a, b];
    axisStamp++;
    tooltipKey = "";
    refreshCursor = true;
    // A gesture draws now (one per gesture); data changes wait for the frame.
    flushDraw(true);
    if (typeof opts.onZoom === "function") opts.onZoom([a, b], fromUser);
  }

  function resetZoom(fromUser = false) {
    if (!zoom) return;
    zoom = null;
    axisStamp++;
    tooltipKey = "";
    flushDraw(true);
    if (typeof opts.onZoom === "function") opts.onZoom(null, fromUser);
  }

  // --- Options for the Logs histogram and the Metrics panels ------------------------------
  //
  //   xDomain: [lo, hi]          the full x domain (the requested time range), not the data's
  //   zone: { parts(ms), epoch(y, mo, d, h, mi, s) }   hune (E7): the clock of the time axis, the cursor time and the tooltip head. Default: the browser.
  //   drawUnder(ctx, layout, { dpr })                  hune (E8): draws under the series, inside the plot (thresholds).
  //   legendColumns (any calculation, in order), legendCalc(series, name), legendSort { by, desc }, tooltipSort "asc" | "desc",
  //   tooltipHideZeros, series[].legendHidden / tooltipHidden, hidden (a Set of ids)   hune (E9)
  //   barFillAlpha, barLineWidth, barMaxWidth, barInnerRatio, barLabel(ctx, bar, layout), xGrid: false   hune (E10), bars
  //   xTitle, yTitle, xTicksFn(lo, hi, plotW, measure)   hune (E11), titles of the axes and the ticks of a number axis
  //   cellGap, barOverlap, barSlot, barDecimate: false   hune (E22 to E24); barInset   hune (E26), barInset: "box"   hune (E34)
  //   xTickRotation              hune (E25), degrees (positive: clockwise, the start of the text at the tick) of the labels of a category axis
  //   scales: { <series id>: "y" | "y2" }, y2: { yAxis, yBounds, ySoft, yTickFormat, yTitle, yScale, yUnit, yHeadroom, formatY, shown },
  //   yShown, ySoft: [soft_min, soft_max], gridScale: "y" | "y2" | "none"   hune (E13, E15), the right y scale, the soft ends and the grid
  //   drawOrder: "series"; series[].type / width / fill / dash / dashCap / points / pointSize / negate   hune (E14), the style of one series
  //   yHeadroom: 0.15            room above the largest value, a share of the range (default 0.06)
  //   yUnit(maxAbs)              -> { factor, suffix }: the y tick unit (default K / M / B / T)
  //   formatY(v)                 the y readout of the cursor
  //   barWidthRatio              bar width / slot width (default 0.72)
  //   series[].dash: [6, 3]      dashed line (and legend swatch)
  //   tooltipSort: "asc" | "desc" | "reverse"   rows by value, or bottom-of-stack last (hune: asc; tooltipHideZeros removes the rows that equal 0)
  // hune: E42, the options `tooltipTitle`, `tooltipMaxRows`, `tooltipNulls`, `tooltipFooter`, `onHiddenChange`, `xDateOnly`, `yInclude`,
  // `cursorPoints` and the function bridgeGaps are removed.
  // hune: E41, the options `legendClick`, `legendPressed`, `legendDisabled`, `legendTitle`, `onLegendClick`, `onLegendMode` and the totals legend are removed.
  //   legendColumns: ["min", ...] the columns of the Values table, among total, min, max, mean and last (all by default); series[].weights (one
  //                              number per point: the calls behind an average, the requests behind a rate) makes the Mean the mean of the values weighted by them;
  //                              series[].stats: the values that the table reads when the plot omits some (the zeros of a stack);
  //                              legendFormat(value, column, series): the text of a cell (default: formatValue)
  // hune: E40, the options `alignKey`, `yKind` and `yInteger` are removed.
  // hune: E17, the option `markers` is removed.

  function fixedDomain() {
    const d = opts.xDomain;
    return d && Number.isFinite(d[0]) && Number.isFinite(d[1]) && d[1] > d[0] ? [d[0], d[1]] : null;
  }


  function yUnitFor(maxAbs, so = opts) {
    const unit = typeof so.yUnit === "function" ? so.yUnit(maxAbs) : null;
    return unit && unit.factor > 0 ? { factor: unit.factor, suffix: String(unit.suffix || "") } : compactUnitFor(maxAbs);
  }

  function strokeDashed(ctx, s) {
    const dash = Array.isArray(s.dash) && s.dash.length ? s.dash : null;
    if (dash) {
      ctx.setLineDash(dash.map((v) => v / dpr)); // hune: E45. The array is in device pixels, as Grafana draws it (9.19).
      if (s.dashCap) ctx.lineCap = s.dashCap; // hune: E14. `dash` has a butt cap and `dot` has a round cap (paper 10.8, V3).
    }
    ctx.stroke();
    if (dash) ctx.setLineDash([]);
  }

  const barRatio = () => (opts.barWidthRatio > 0 && opts.barWidthRatio <= 1 ? opts.barWidthRatio : 0.72);

  function tooltipSeries(i) {
    const list = visibleSeries();
    if (opts.tooltipSort === "reverse") return list.reverse();
    if (opts.tooltipSort === "desc" || opts.tooltipSort === "asc") {
      // hune: asc and desc (E9, F20). A row with no value goes last. Rows with equal values keep the order of the series (a stable sort).
      const up = opts.tooltipSort === "asc";
      const has = (s) => s.values[i] === s.values[i];
      return list.sort((a, b) => {
        if (!has(a) || !has(b)) return has(a) === has(b) ? 0 : has(a) ? -1 : 1;
        return up ? a.values[i] - b.values[i] : b.values[i] - a.values[i];
      });
    }
    return list;
  }


  // hune: E41. Ctrl or Cmd+click toggles (no legendClick "toggle").
  function legendToggles(ev) {
    return ev.ctrlKey || ev.metaKey;
  }

  // hune: E17. The exemplar markers are removed: Hune never sets the option `markers` and never calls `setMarkers`.

  // --- lifecycle -------------------------------------------------------------------------

  function scheduleDraw() {
    if (!drawRaf && !destroyed) drawRaf = requestAnimationFrame(() => draw());
  }

  // Redraw when the width changes or when the chart shows again after a
  // draw was skipped (hidden panel) or its canvases were released.
  const resizeObserver = typeof ResizeObserver === "function"
    ? new ResizeObserver((entries) => {
      const width = Math.floor(entries[entries.length - 1].contentRect.width);
      observedWidth = width;
      widthObserved = true;
      if (!width) { release(); return; }
      if (width !== sizeW || released || pendingDraw) scheduleDraw();
    })
    : null;
  if (resizeObserver) resizeObserver.observe(plotEl);

  let dprQuery = null;
  const onDpr = () => { watchDpr(); scheduleDraw(); };
  function watchDpr() {
    if (dprQuery) dprQuery.removeEventListener("change", onDpr);
    dprQuery = window.matchMedia ? window.matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`) : null;
    if (dprQuery && dprQuery.addEventListener) dprQuery.addEventListener("change", onDpr);
  }
  watchDpr();

  // A hidden chart gives its canvas memory back; it redraws when shown.
  function release() {
    if (released) return;
    released = true;
    for (const canvas of [baseCanvas]) { canvas.width = 0; canvas.height = 0; }
    sizeW = 0;
    leaveCursor();
    // hune: E18, no box of the 2-D brush to cancel.
  }

  // Every change draws once, on the next animation frame. Several setData calls in a frame
  // (a streamed result, or a zoom reset with new data) cost one draw. next.append means that the arrays only grew at their end since the last
  // setData (rows appended). So, the block summaries grow and are not computed again.
  // A call to layout(), stats(), points() or toClient() draws a pending change first.
  function setData(next = {}) {
    counters.setData++;
    const appended = next.append === true;
    delete next.append;
    Object.assign(opts, next);
    if (appended) {
      for (const ser of opts.series) {
        const sum = summaries.get(ser.id);
        if (sum && sum.values !== ser.values) sum.adopt = ser.values;
      }
    } else summaries.clear();
    if (next.hidden) hidden = new Set(next.hidden);
    if (next.height) plotEl.style.height = `${opts.height}px`;
    if ("zoom" in next) zoom = next.zoom;
    // Keep a zoom while it still overlaps the data (streaming results).
    if (zoom) {
      const [lo, hi] = fullDomain();
      if (zoom[1] <= lo || zoom[0] >= hi) zoom = null;
    }
    stacks = null;
    axisStamp++;
    if (theme) for (const s of opts.series) if (!theme.seriesColors.has(s.id)) theme.seriesColors.set(s.id, color(s.color || "var(--qchart-1)"));
    tooltipKey = "";
    if (cursorIndex >= opts.xs.length) leaveCursor();
    refreshCursor = true;
    scheduleDraw();
  }

  // Draws a scheduled change now (force: even when none is scheduled).
  function flushDraw(force = false) {
    if (destroyed || (!drawRaf && !force)) return;
    if (drawRaf) { cancelAnimationFrame(drawRaf); drawRaf = 0; }
    draw(true);
  }

  // hune: E13. The label widths are measured at the axis font. When the page font arrives after the first draw, measure again (a gutter of the right size).
  const onFonts = () => { if (destroyed) return; widths.clear(); boldWidths.clear(); axisStamp++; scheduleDraw(); };
  if (document.fonts && document.fonts.addEventListener) document.fonts.addEventListener("loadingdone", onFonts);

  function themeChanged() {
    theme = null;
    axisStamp++;
    widths.clear();
    boldWidths.clear();
    tooltipKey = "";
    scheduleDraw();
  }

  function destroy() {
    destroyed = true;
    live.delete(api);
    if (opts.syncKey && syncGroups.has(opts.syncKey)) syncGroups.get(opts.syncKey).delete(api);
    if (resizeObserver) resizeObserver.disconnect();
    if (sizeObserver) sizeObserver.disconnect();
    if (dprQuery) dprQuery.removeEventListener("change", onDpr);
    document.removeEventListener("keydown", onKey, true);
    if (document.fonts && document.fonts.removeEventListener) document.fonts.removeEventListener("loadingdone", onFonts);
    if (drawRaf) cancelAnimationFrame(drawRaf);
    overlayFrame.cancel();
    moveFrame.cancel();
    // hune: E18, the 2-D brush has no listener to remove.
    release();
    root.remove();
  }

  // --- Extensions: own-x and per-point marks, picking, heatmap cells, custom /
  // log y axes, regions (first used by the Traces charts). Every option is
  // optional. Without them, the chart behaves as described above.
  // hune: E17 to E19 remove the exemplar markers, the engine annotations and the 2-D brush.
  //
  //   series[k].type        "line" | "area" | "points" | "bar": the series' own mark
  //                         (a line over bars, points over lines).
  //   series[k].xs          its own ascending x column (a scatter over the
  //                         shared-x series): it counts for the y domain, not
  //                         for the cursor snap, cursor dots or tooltip rows.
  //   series[k].radius      points: px, one number or a Float64Array per point.
  //   series[k].pointColor  points: (i) => CSS colour of point i (null: the series').
  //   series[k].fillAlpha   points: fill opacity (0.5; outlines at 0.85).
  //   series[k].pickable    points: hovering picks the point under the pointer
  //                         (within 3 px of its edge; the closer, smaller one wins).
  //   pick(pt)              custom picking, pt = { px, py, x, y }: a hit object
  //                         ({ key, x, px, py, r } all optional) or null.
  //   pickTooltip(hit)      { title, rows: [{ label, value, color, className }], footer }.
  //   onPick(hit)           a click on a hit (the pointer shows it is clickable).
  //   xReadout(i)           text of x index i (x badge, and the tooltip title).
  //   yTickFormat(v, step)  y tick labels.
  //   yAxis(min, max, plotH, raw) -> { min, max, ticks: [{ v, label }], step }: the y
  //                         domain and ticks from the data extent. hune: E30, raw: the data ends of yAxisOf.
  //   yBounds: [min, max]   fixed y edges (null: the data's own; a pair with max <= min is ignored).
  //   interpolation         "linear" | "smooth" | "stepBefore" | "stepAfter": the shape of line and area paths.
  //   lineWidth             px (default 1.5, an area's top 1.25); fillOpacity 0..1: the fill under every line.
  //   showPoints            "auto" | "always" | "never": the marks on a line; pointSize px (diameter, default 5.2).
  //   stackMode             "percent": a stack shows shares out of 100 (default: the values add up).
  //   stackSign: "series"   hune: E32, a series with more values below 0 stacks down from 0 in a group of its own (Grafana). E33: and a
  //                         series of another `type`.
  //   yScale: "log"         log y (positive values; ticks 1-2-5 per decade).
  //   cells: { x0, x1, y0, y1: Float64Array, level: Uint8Array, palette: [CSS colours] }
  //                         heatmap cells: one rect each, palette[level - 1] (0: none).
  // hune: E18, E19, the options `annotations` and `brush` are removed.
  //   regions: [{ id, x0, x1, y0, y1, className, hidden }]   persistent boxes in data
  //                         units (no y0 / y1: the plot height); setRegions(list).
  //   keyboard: false       no keyboard cursor (the host element handles keys).
  // (The x domain and the y readout are the xDomain and formatY options above.)
  // API: setRegions(list), showTooltip(content, px, py), hideTooltip(),
  // points(id) -> [{ index, x, y, r }] (plot px), toClient(x, y) -> { x, y }.

  // hune: E18, E19. No layer of engine annotations and no box of the 2-D brush.
  const pickRing = document.createElement("i");
  pickRing.className = "chartCore__pick";
  pickRing.hidden = true;
  cursorEl.appendChild(pickRing);
  const regionEls = new Map();
  const pickPoints = new Map(); // series id -> { index, px, py, r } arrays of the last draw
  let pickHit = null;
  let press = null;
  if (opts.keyboard === false) overCanvas.removeAttribute("tabindex");

  const pointsMark = (s) => s.type === "points" || (!!s.xs && !s.type);
  function ownMark(s) {
    if (scaleOpts(scaleOf(s)).yScale === "log" || s.xs) return true;
    if (s.type === "points") return s.radius != null || typeof s.pointColor === "function" || !!s.pickable;
    // hune: E14. A chart that draws in series order (opts.drawOrder === "series") draws a line between bars in its turn, not after the bars.
    return opts.type === "bar" && !!s.type && s.type !== "bar" && opts.drawOrder !== "series";
  }

  function ownRange(s, lo, hi) {
    return [lowerBound(s.xs, lo), upperBound(s.xs, hi)];
  }

  function cssColor(value) {
    if (!theme.cssColors) theme.cssColors = new Map();
    let c = theme.cssColors.get(value);
    if (!c) { c = color(value); theme.cssColors.set(value, c); }
    return c;
  }

  function yTickLabel(v, step, unit, so = opts) {
    return typeof so.yTickFormat === "function" ? so.yTickFormat(v, step) : formatTick(v, step, unit);
  }

  // The y domain and ticks of a yAxis hook, or of a log scale. hune: E30, raw.
  function customYAxis(dataMin, dataMax, plotH, so = opts, raw = null) {
    if (typeof so.yAxis === "function") {
      const ax = so.yAxis(dataMin, dataMax, plotH, raw);
      if (ax && ax.max > ax.min && Array.isArray(ax.ticks)) return ax;
    }
    if (so.yScale !== "log") return null;
    const lo = dataMin > 0 ? dataMin : dataMax > 0 ? dataMax / 10 : 1;
    const hi = dataMax > lo ? dataMax : lo * 10;
    const unit = compactUnitFor(hi);
    return { min: lo, max: hi, ticks: logTicks(lo, hi).map((v) => ({ v, label: yTickLabel(v, v, unit, so) })) };
  }

  function applyLogScale(L) {
    const lo = Math.max(L.yMin, Number.MIN_VALUE);
    const hi = Math.max(L.yMax, lo * 1.000001);
    const lmin = Math.log(lo), lmax = Math.log(hi);
    const ky = L.plotH / (lmax - lmin);
    const base = L.top + L.plotH;
    Object.assign(L, {
      logY: true, yMin: lo, yMax: hi, ky,
      yOf: (y) => base - (Math.log(Math.max(y, lo)) - lmin) * ky,
      yAt: (py) => Math.exp(lmin + (base - py) / ky),
      // The same plot linear in log units, for the line / area tracer.
      logView: { ...L, yMin: lmin, yMax: lmax, ky, yOf: (v) => base - (v - lmin) * ky, real: L }, // hune: E43, `real` is the view in the units of the values
    });
  }

  const logValues = new WeakMap();
  function logSeries(s) {
    let values = logValues.get(s.values);
    if (!values) {
      values = new Float64Array(s.values.length);
      for (let i = 0; i < values.length; i++) { const v = s.values[i]; values[i] = v > 0 ? Math.log(v) : NaN; }
      logValues.set(s.values, values);
    }
    return { ...s, values, derived: true };
  }

  let cellsCache = null;
  function cellsExtent(lo, hi) {
    const c = opts.cells;
    if (!c || !c.level || !c.level.length) return null;
    if (cellsCache && cellsCache.cells === c && cellsCache.lo === lo && cellsCache.hi === hi) return cellsCache.out;
    let min = Infinity, max = -Infinity;
    for (let k = 0; k < c.level.length; k++) {
      if (!(c.x1[k] > lo && c.x0[k] < hi)) continue;
      if (c.y0[k] < min) min = c.y0[k];
      if (c.y1[k] > max) max = c.y1[k];
    }
    const out = min <= max ? [min, max] : null;
    cellsCache = { cells: c, lo, hi, out };
    return out;
  }

  // One fill per palette step; cells snap to device pixels and keep a 1 px
  // gap once they are wider (taller) than 3 px.
  function drawCells(ctx, L) {
    const c = opts.cells;
    if (!c || !c.level || !c.level.length) { delete root.dataset.cellsDrawn; return 0; }
    const palette = c.palette || [];
    const snap = (v) => Math.round(v * dpr) / dpr;
    const gap = opts.cellGap >= 0 ? opts.cellGap : 1; // hune: E22, `cellGap` (px)
    let drawn = 0;
    for (let step = 1; step <= palette.length; step++) {
      ctx.beginPath();
      let any = false;
      for (let k = 0; k < c.level.length; k++) {
        if (c.level[k] !== step || !(c.x1[k] > L.xLo && c.x0[k] < L.xHi)) continue;
        const xa = snap(L.xOf(c.x0[k])), xb = snap(L.xOf(c.x1[k]));
        const ya = snap(L.yOf(c.y1[k])), yb = snap(L.yOf(c.y0[k]));
        const gx = xb - xa > 3 ? gap : 0, gy = yb - ya > 3 ? gap : 0;
        ctx.rect(xa, ya, Math.max(0.5, xb - xa - gx), Math.max(0.5, yb - ya - gy));
        any = true;
        drawn++;
      }
      if (!any) continue;
      ctx.fillStyle = rgba(cssColor(palette[step - 1]));
      ctx.fill();
    }
    root.dataset.cellsDrawn = String(drawn);
    return drawn;
  }

  // Series that the main pass omits: points with their own x or per-point
  // style settings, lines over bars, every series of a log chart.
  function drawOwnMarks(ctx, L, list, perSeries) {
    if (!list.length) return 0;
    let points = 0;
    const bars = list.filter((s) => (s.type || opts.type) === "bar" && !s.xs);
    if (bars.length) points = drawBars(ctx, L, bars, perSeries);
    for (const s of list) {
      if (bars.includes(s)) continue;
      let drawn;
      const V = viewFor(L, s);
      if (pointsMark(s)) drawn = drawPoints(ctx, V, s);
      else if (V.logY) drawn = drawSeries(ctx, V.logView, logSeries(s));
      else drawn = drawSeries(ctx, V, s);
      perSeries[s.label] = { points: drawn, runs: pointsMark(s) ? (drawn ? 1 : 0) : runs.length };
      points = Math.max(points, drawn);
    }
    return points;
  }

  function drawPoints(ctx, L, s) {
    const own = !!s.xs;
    const xs = own ? s.xs : opts.xs;
    const category = !own && opts.xKind === "category";
    const [i0, i1] = own ? ownRange(s, L.xLo, L.xHi) : [L.v0, L.v1];
    const values = drawnValues(s);
    const radius = s.radius;
    const alpha = alphaFor(s);
    const fill = s.fillAlpha == null ? 0.5 : s.fillAlpha;
    const cache = { index: [], px: [], py: [], r: [] };
    const groups = new Map();
    for (let i = i0; i < i1 && cache.index.length < 50000; i++) {
      const v = values[i];
      if (v !== v) continue;
      const key = typeof s.pointColor === "function" ? s.pointColor(i) || "" : "";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key).push(cache.index.length);
      cache.index.push(i);
      cache.px.push(L.xOf(category ? i : xs[i]));
      cache.py.push(L.yOf(v));
      cache.r.push(typeof radius === "number" ? radius : radius ? radius[i] : 2.6);
    }
    for (const [key, members] of groups) {
      const c = key ? cssColor(key) : seriesColor(s);
      ctx.beginPath();
      for (const k of members) {
        ctx.moveTo(cache.px[k] + cache.r[k], cache.py[k]);
        ctx.arc(cache.px[k], cache.py[k], cache.r[k], 0, Math.PI * 2);
      }
      ctx.fillStyle = rgba(c, fill * alpha);
      ctx.fill();
      ctx.lineWidth = 1;
      ctx.strokeStyle = rgba(c, 0.85 * alpha);
      ctx.stroke();
    }
    pickPoints.set(s.id, cache);
    return cache.index.length;
  }

  // --- picking ---

  const picking = () => typeof opts.pick === "function" || opts.series.some((s) => s.pickable);

  function pickPoint(p) {
    let best = null, bestScore = Infinity;
    for (const s of visibleSeries()) {
      if (!s.pickable) continue;
      const c = pickPoints.get(s.id);
      if (!c) continue;
      for (let k = 0; k < c.index.length; k++) {
        const r = c.r[k];
        const d = Math.hypot(c.px[k] - p.px, c.py[k] - p.py);
        if (d > r + 3) continue;
        const score = d / (r + 3) + r / 100;
        if (score < bestScore) {
          bestScore = score;
          const i = c.index[k];
          best = { key: `${s.id}:${i}`, series: s, seriesId: s.id, index: i, x: (s.xs || opts.xs)[i], px: c.px[k], py: c.py[k], r };
        }
      }
    }
    return best;
  }

  function pickAt(p) {
    if (!layout || !picking()) return null;
    const custom = typeof opts.pick === "function" ? opts.pick({ px: p.px, py: p.py, x: layout.xAt(p.px), y: layout.yAt(p.py) }) : null;
    return custom || pickPoint(p);
  }

  function tooltipHtml(content) {
    if (!content) return "";
    const rows = (content.rows || []).map((row) => {
      const swatch = row.color ? ` style="background:${rgba(cssColor(row.color))}"` : "";
      return `<span class="chartCore__tipRow${row.className ? ` ${esc(row.className)}` : ""}"><i${swatch}></i><em>${esc(row.label)}</em> <b>${esc(row.value)}</b></span>`;
    }).join("");
    return `${content.title != null ? `<strong>${esc(content.title)}</strong>` : ""}${rows}${content.footer ? `<small>${esc(content.footer)}</small>` : ""}`;
  }

  // A tooltip shows the host's content beside (px, py), in plot px. Its size
  // is set by the size observer. The observer re-places the tooltip after the new content
  // is positioned. A pointer move causes no synchronous layout.
  let contentTip = null;
  function showContentTooltip(content, px, py) {
    const key = `content|${JSON.stringify(content)}`;
    if (!content) { tooltipEl.hidden = true; tooltipKey = ""; contentTip = null; return; }
    if (key !== tooltipKey || tooltipEl.hidden) {
      tooltipKey = key;
      tooltipEl.innerHTML = tooltipHtml(content);
      delete tooltipEl.dataset.index;
      tooltipEl.hidden = false;
    }
    contentTip = { px, py };
    placeContentTip();
  }

  function placeContentTip() {
    if (!contentTip) return;
    const size = measured(tooltipEl);
    const { px, py } = contentTip;
    const width = layout ? layout.width : plotEl.clientWidth;
    let lx = px + 16;
    if (lx + size.w > width - 4) lx = px - 16 - size.w;
    lx = Math.max(4, lx);
    const ty = Math.max(0, Math.min(opts.height - size.h, py - size.h / 2));
    tooltipEl.style.transform = `translate(${Math.round(lx)}px, ${Math.round(ty)}px)`;
  }

  function showPick(p) {
    const hit = pickAt(p);
    if (!hit) { clearPick(); return false; }
    const same = pickHit && pickHit.key != null && pickHit.key === hit.key;
    pickHit = hit;
    cursor = { px: p.px, py: p.py, nearest: null };
    cursorIndex = -1;
    overCanvas.classList.toggle("is-pickable", typeof opts.onPick === "function");
    if (!same) {
      if (hit.r != null && hit.px != null) {
        const size = 2 * hit.r + 4;
        pickRing.style.width = `${size}px`;
        pickRing.style.height = `${size}px`;
        pickRing.style.margin = `${-size / 2}px 0 0 ${-size / 2}px`;
        place(pickRing, hit.px, hit.py);
        show(pickRing, true);
      } else show(pickRing, false);
      root.dataset.pick = String(hit.key == null ? "" : hit.key);
    }
    showContentTooltip(typeof opts.pickTooltip === "function" ? opts.pickTooltip(hit) : null, p.px, p.py);
    scheduleOverlay();
    broadcast(Number.isFinite(hit.x) && opts.xKind !== "category" ? hit.x : null);
    return true;
  }

  function clearPick() {
    if (!pickHit) return;
    pickHit = null;
    tooltipKey = "";
    contentTip = null;
    show(pickRing, false);
    overCanvas.classList.remove("is-pickable");
    delete root.dataset.pick;
  }

  overCanvas.addEventListener("pointerdown", (ev) => { press = { x: ev.clientX, y: ev.clientY }; });
  overCanvas.addEventListener("click", (ev) => {
    const from = press;
    press = null;
    if (typeof opts.onPick !== "function" || !from || Math.hypot(ev.clientX - from.x, ev.clientY - from.y) >= DRAG_MIN_PX) return;
    const hit = pickAt(localPoint(ev));
    if (hit) opts.onPick(hit);
  });

  // hune: E18. The 2-D brush is removed: Hune never sets the option `brush`. placeBox stays for the regions.

  function placeBox(el, r, L) {
    const xa = Math.max(L.left, L.xOf(r.x0)), xb = Math.min(L.left + L.plotW, L.xOf(r.x1));
    const y0 = r.y0 == null ? L.yMin : r.y0, y1 = r.y1 == null ? L.yMax : r.y1;
    const ya = Math.max(L.top, L.yOf(y1)), yb = Math.min(L.top + L.plotH, L.yOf(y0));
    el.style.width = `${Math.max(1, xb - xa)}px`;
    el.style.height = `${Math.max(1, yb - ya)}px`;
    place(el, xa, ya);
  }

  // hune: E19. The engine annotations are removed. The regions stay (DOM, moved on every draw).

  function placeRegions(L) {
    const list = Array.isArray(opts.regions) ? opts.regions : [];
    const seen = new Set();
    for (const r of list) {
      const id = String(r.id);
      seen.add(id);
      let el = regionEls.get(id);
      if (!el) {
        el = document.createElement("i");
        cursorEl.insertBefore(el, pickRing);
        regionEls.set(id, el);
      }
      el.className = `chartCore__region${r.className ? ` ${r.className}` : ""}`;
      const on = !r.hidden && r.x1 > r.x0 && !!L;
      el.hidden = !on;
      if (on) placeBox(el, r, L);
    }
    for (const [id, el] of regionEls) if (!seen.has(id)) el.hidden = true;
  }

  function publishExtras() {
    const L = layout;
    root.dataset.yTicks = JSON.stringify(L.yTicks.map((t) => t.label));
    if (L.y2) root.dataset.y2Ticks = JSON.stringify(L.y2.yTicks.map((t) => t.label)); else delete root.dataset.y2Ticks;
    // [label, context, left, right, context left, context right] of each x label drawn.
    root.dataset.xTicks = JSON.stringify(L.xLabels.map((t) => [t.label, t.context, Math.round(t.left), Math.round(t.right)]
      .concat(t.context ? [Math.round(t.cleft), Math.round(t.cright)] : [])));
    if (opts.yScale) root.dataset.yScale = opts.yScale; else delete root.dataset.yScale;
    // hune: E18, E19: no engine annotations, no box of the 2-D brush, and no destroyExtras.
    placeRegions(L);
    if (pickHit) clearPick();
  }

  const extraApi = {
    setRegions(list) {
      opts.regions = Array.isArray(list) ? list : [];
      if (layout && !released) placeRegions(layout);
    },
    showTooltip(content, px, py) { showContentTooltip(content, px, py); },
    hideTooltip() { tooltipEl.hidden = true; tooltipKey = ""; contentTip = null; },
    points(id) {
      flushDraw();
      const c = pickPoints.get(id);
      return c ? c.index.map((index, k) => ({ index, x: c.px[k], y: c.py[k], r: c.r[k] })) : [];
    },
    toClient(x, y) {
      flushDraw();
      const box = plotEl.getBoundingClientRect();
      return layout ? { x: box.left + layout.xOf(x), y: box.top + layout.yOf(y) } : null;
    },
  };

  const api = {
    root,
    setData,
    update: setData,
    setZoom: (lo, hi) => setZoom(lo, hi, false),
    resetZoom: () => resetZoom(false),
    getZoom: () => (zoom ? zoom.slice() : null),
    setHidden: (ids) => setHiddenInternal(new Set(ids)),
    getHidden: () => new Set(hidden),
    redraw: scheduleDraw,
    release,
    destroy,
    themeChanged,
    xKind: () => opts.xKind,
    syncCursor(x) {
      if (cursor) return;
      syncedX = x;
      if (x == null) delete root.dataset.syncX; else root.dataset.syncX = String(x);
      scheduleOverlay();
    },
    stats: () => { flushDraw(); return { ...stats }; },
    // The gaps of a regular time axis (rows missing between points): how many break the line.
    gaps: () => gapInfo().count,
    layout: () => { flushDraw(); return layout; },
    flush: () => flushDraw(),
    // True until the plot is drawn at its current width. This is true when a draw is
    // scheduled, or when the width changed (a scrollbar came, or the window
    // was resized) and the resize observer has not drawn it yet. Until then, layout() and
    // toClient() describe the last frame drawn.
    drawPending: () => !destroyed && (!!drawRaf || pendingDraw || (!released && plotWidth() !== sizeW)),
    // The mode of the legend, "list" or "table".
    legendMode: () => legendMode,
    setLegendMode,
    // hune: E17, no setMarkers.
  };
  Object.assign(api, extraApi);
  live.add(api);
  watchTheme();
  if (opts.syncKey) {
    if (!syncGroups.has(opts.syncKey)) syncGroups.set(opts.syncKey, new Set());
    syncGroups.get(opts.syncKey).add(api);
  }
  draw(); // hune: E40
  return api;
}

export const chartCore = {
  create,
  // Scales and formats, shared with the charts that draw their own marks.
  timeTicks,
  layoutXLabels,
  linearTicks,
  niceStep,
  compactUnitFor,
  formatTick,
  formatExact,
  formatValue,
  formatInstant,
  formatDuration,
  utcOffsetText,
  decimalsFor,
  lowerBound,
  upperBound,
  regularStep,
  logTicks,
  counters: () => ({ ...counters }),
  resetCounters,
  // The colour helpers and their batches, for the equivalence tests.
  colors: { parseColor, parseColors, rgba }, // hune: E37, no rgbaBatch.
  // hune: E42. No of(el): Hune keeps the API of each chart that it makes.
};
