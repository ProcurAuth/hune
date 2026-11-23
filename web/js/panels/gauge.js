// The gauge panel (9.19g, 4.33e): the classic gauge of Grafana 12.4.2 (the flag `newGauge` is off there). Hune code draws each gauge on
// a canvas. The geometry follows the gauge of Grafana (measured: the arcs, the text sizes and the places of 34 panels in both themes).
// The arc goes from 0.9 pi to 2.1 pi (clockwise from the lower left to the lower right). Each value has its own cell, canvas and title.

import { fillOf, parseCss, resolveFill } from "../colors.js";
import { el } from "../ui.js";
import { canvasColor, chartLabel } from "./common.js";
import { legibleOn, reduceItems } from "./items.js";
import { sharedRange } from "./series.js";
import { innerSize } from "./box.js";

const START = 0.9; // the angles, in units of pi
const END = 2.1;
const SIN_TOP = Math.max(Math.sin(START * Math.PI), Math.sin(END * Math.PI)); // the highest point of the arc below the centre
const GAP = 8; // px between two cells
const MIN_CELL = 75; // the least width or height (px) of a cell in a row or a column
const LINE = 22 / 14; // the line height of a text (the body of Grafana and --lh-base)
const CELL_MARGIN = 5;
const MAX_ROWS = 25; // with reduce.values: the first rows only (the default limit of Grafana)
const TRACK_MIX = 0.06; // the empty arc: the text colour at 6 % over the panel, as the bar gauge track (--bar-off)
const MAX_NAME = 120;
let seq = 0;

/** The size of the title and the height of the gauge in a cell `w` x `h` (pure). `vertical` reads the height for the title size. */
export function autoSize(w, h, hasTitle, vertical = false) {
  const titleSize = Math.min(((vertical ? h : w) * 0.15) / 1.5, 20);
  return { titleSize, gaugeH: Math.min(hasTitle ? h - titleSize * 1.5 : h, w) };
}

/** The best square side for `n` cells in `w` x `h` (pure; one axis). */
function side(w, h, n) {
  const k = Math.ceil(Math.sqrt((n * w) / h));
  return Math.floor((k * h) / w) * k < n ? h / Math.ceil((k * h) / w) : w / k;
}

/** The cells of the orientation `auto`: a grid of columns and rows (pure). Gives [{ x, y, w, h }]. */
export function gridCells(n, w, h, gap = GAP) {
  const best = Math.max(side(w, h, n), side(h, w, n));
  let cols = Math.floor(w / best);
  const rows = Math.ceil(n / cols);
  cols = Math.ceil(n / rows);
  const lastCols = cols - (cols * rows - n);
  const cw = w / cols - gap + gap / cols;
  const ch = h / rows - gap + gap / rows;
  const lastW = w / lastCols - gap + gap / lastCols;
  const out = [];
  for (let i = 0; i < n; i++) {
    const col = i % cols;
    const row = Math.floor(i / cols);
    const cellW = row === rows - 1 ? lastW : cw;
    out.push({ x: col * cellW + gap * col, y: row * ch + gap * row, w: cellW, h: ch });
  }
  return out;
}

/** The cells of a column (`horizontal`) or a row (`vertical`) (pure). `min` is the least height or width of a cell. */
export function stackCells(n, w, h, vertical, min = MIN_CELL, gap = GAP) {
  const out = [];
  if (vertical) {
    const cw = Math.max(w / n - gap + gap / n, min);
    for (let i = 0; i < n; i++) out.push({ x: i * (cw + gap), y: 0, w: cw, h });
  } else {
    const ch = Math.max((h + gap) / n - gap, min);
    for (let i = 0; i < n; i++) out.push({ x: 0, y: i * (ch + gap), w, h: ch });
  }
  return out;
}

/**
 * The parts of one cell `w` x `h` (pure): the box of the canvas and of the title. A title and a gauge that are higher than the cell
 * both get smaller in the ratio of their heights (the flex column of Grafana), and the title moves up by 4 px.
 * Gives { canvasW, canvasH, top, titleSize, titleY, titleH, titleW, sizeW, sizeH } where `sizeW` and `sizeH` give the arc and text sizes.
 */
export function cellParts({ w, h, title, orientation, titleSize = null }) {
  const vertical = orientation === "vertical";
  const own = autoSize(w, h, title, vertical);
  const size = titleSize ?? own.titleSize;
  const titleLine = title ? size * LINE : 0;
  const total = own.gaugeH + titleLine;
  const shrink = total > h ? (total - h) / total : 0;
  const canvasH = own.gaugeH * (1 - shrink);
  const titleH = titleLine * (1 - shrink);
  const top = total < h ? (h - total) / 2 : 0;
  // The arc and the text read the size without the orientation (Grafana): a vertical gauge has the height of the gauge as its width.
  const plain = autoSize(w, h, title, false);
  const sizeW = vertical ? plain.gaugeH : w;
  return { canvasW: vertical ? own.gaugeH : w, canvasH, top, titleSize: size, titleY: top + canvasH - 4, titleH, titleW: vertical ? own.gaugeH : w, sizeW, sizeH: Math.min(sizeW, plain.gaugeH) };
}

/** The font size of the value text (pure): the text width at 14 px is `w14`. The text fits `width`, `height` and `max`. */
export function valueFont(w14, width, height, max) {
  return Math.min(height, (width / (w14 + 2)) * 14, max);
}

/**
 * The geometry of one gauge on a canvas `cw` x `ch` (pure). `sizeW` and `sizeH` give the sizes of the arc and of the text, `w14` the
 * width of the value text at 14 px. `markers` is the option, `bands` says that the threshold band shows (markers and two entries).
 * Gives { arcW, markW, valueSize, labelSize, radius, cx, cy, labelMargin } (px).
 */
export function gaugeGeometry({ cw, ch, sizeW, sizeH, w14, labels, markers, bands, valueSize = null }) {
  const arcW0 = Math.min(sizeH / 5.5, 40) / (labels ? 1.5 : 1);
  const markW = arcW0 / 5;
  const room = Math.min(sizeW, sizeH * 1.3) * 0.9 - ((arcW0 + (markers ? markW : 0) + (labels ? 10 : 0)) * 2 + 10);
  let font = valueSize ?? valueFont(w14, room, sizeH, arcW0 * 1.7);
  if (!(font > 0)) font = 14; // a cell too small: the browser of Grafana keeps the body size for a size below 0
  const labelSize = Math.max(font / 2.5, 12);
  const valueMargin = Math.max(1, cw / 30);
  const band = bands ? markW : 0;
  const labelMargin = labels ? markW + 1 : 0;
  const labelFont = labels ? labelSize : 0;
  const byWidth = cw / 2 - CELL_MARGIN - band - labelMargin * 2 - labelFont;
  let z = (ch - CELL_MARGIN * 2) / (1 + SIN_TOP);
  if (z * SIN_TOP < valueMargin + font / 2) z = ch - CELL_MARGIN * 2 - valueMargin - font / 2;
  const radius = Math.min(byWidth, z - labelMargin * 2 - labelFont - band);
  const arcW = arcW0 >= radius ? Math.max(3, radius / 3) : arcW0;
  const reach = labelMargin * 2 + labelFont + band + radius;
  const outer = Math.max(reach * (1 + SIN_TOP), reach + valueMargin + font / 2);
  const cy = CELL_MARGIN + band + labelFont + labelMargin * 2 + radius + (ch - CELL_MARGIN * 2 - outer) / 2;
  return { arcW, markW: band, valueSize: font, labelSize, radius, cx: cw / 2, cy, labelMargin };
}

/** The angle (units of pi) of the value `v` on the scale `min` to `max`, limited to the arc (pure). An empty scale gives NaN. */
export function angleOf(v, min, max) {
  const a = START + (END - START) * ((v - min) / (max - min));
  return a < START ? START : a > END ? END : a;
}

// The step of the thresholds that holds `v`: the last step with a value at or below `v` (the first step has no value).
function activeStep(steps, v) {
  let active = steps[0];
  for (let i = 1; i < steps.length; i++) {
    if (v >= steps[i].value) active = steps[i];
    else break;
  }
  return active;
}

/**
 * The entries of the threshold band from `min` to `max` (pure): [{ value, color }] where `color` is the colour of the part that ends at
 * `value` (the colour of the step before). The ends are rounded to `decimals`. The percentage mode uses the scale 0 to 100.
 * The entries come back in the order of their values.
 */
export function bandEntries(thresholds, min, max, decimals) {
  const steps = thresholds.steps.map((s, i) => ({ value: i === 0 || s.value === null ? -Infinity : s.value, color: s.color }));
  if (thresholds.mode === "percentage") [min, max] = [0, 100];
  const atMin = activeStep(steps, min);
  const atMax = activeStep(steps, max);
  const out = [{ value: parseFloat(min.toFixed(decimals)), color: atMin.color }];
  let skip = true;
  for (let i = 0; i < steps.length; i++) {
    const s = steps[i];
    if (skip) {
      if (s === atMin) skip = false;
      continue;
    }
    out.push({ value: Number.isFinite(s.value) ? s.value : 0, color: steps[i - 1].color });
    if (s === atMax) break;
  }
  if (max > atMax.value) out.push({ value: parseFloat(max.toFixed(decimals)), color: atMax.color });
  return out.sort((a, b) => a.value - b.value);
}

/** The entry that gives the colour of the value arc for `v` (pure): the first entry above `v`, else the last entry. */
export function arcEntry(entries, v) {
  let found = null;
  for (const e of entries) {
    found = e;
    if (v < e.value) break;
  }
  return found;
}

/** The scale of a field (pure): the document min and max, else 0 to 100 (`percent`) or 0 to 1 (`percentunit`), else the range of the data. */
export function scaleOf(config, range) {
  const set = config.set || {};
  const unit = config.unit;
  const pct = unit === "percent" || unit === "percentunit";
  const min = Number.isFinite(set.min) ? set.min : pct ? 0 : Number.isFinite(range.min) ? range.min : 0;
  const max = Number.isFinite(set.max) ? set.max : pct ? (unit === "percent" ? 100 : 1) : Number.isFinite(range.max) ? range.max : 0;
  return { min, max };
}

// One band of a ring from `r` (outer) to `r - w` (inner), between the angles `a1` and `a2` (units of pi). Gives false for a radius below 0:
// the gauge stops there, as Grafana stops on the error of the canvas.
function ring(ctx, cx, cy, r, w, a1, a2, stroke, lineWidth, fill) {
  if (!Number.isFinite(a1) || !Number.isFinite(a2) || a1 === a2) return true;
  if (r < 0 || r - w < 0) return false;
  const p1 = a1 * Math.PI;
  const p2 = a2 * Math.PI;
  ctx.beginPath();
  ctx.arc(cx, cy, r, p1, p2, false);
  ctx.lineTo(cx + (r - w) * Math.cos(p2), cy + (r - w) * Math.sin(p2));
  ctx.arc(cx, cy, r - w, p2, p1, true);
  ctx.closePath();
  if (stroke) {
    ctx.lineWidth = lineWidth;
    ctx.strokeStyle = stroke;
    ctx.stroke();
  }
  if (fill) {
    ctx.fillStyle = fill;
    ctx.fill();
  }
  return true;
}

// A text centred on (x, y): the box of one line of the text (line height LINE) has its middle there. `turn` is the angle (degrees).
function text(ctx, s, x, y, size, color, family, turn = 0) {
  ctx.save();
  ctx.font = `400 ${size}px ${family}`;
  const m = ctx.measureText(s);
  const asc = m.fontBoundingBoxAscent ?? size * 0.96875;
  const desc = m.fontBoundingBoxDescent ?? size * 0.2421875;
  ctx.translate(x, y);
  if (turn) ctx.rotate((turn * Math.PI) / 180);
  ctx.textAlign = "center";
  ctx.textBaseline = "alphabetic";
  ctx.fillStyle = color;
  ctx.fillText(s, 0, (asc - desc) / 2);
  ctx.restore();
}

/**
 * Draws one gauge (pure but for the context). `g` is the geometry, `min` and `max` the scale, `entries` the threshold entries with their
 * `paint`, `value` the number on the scale, `neutral` a number or null, `labels` and `bands` the parts that show, `colors` { track, arc,
 * value, label } and `label` the value text.
 */
export function drawGauge(ctx, g, { min, max, entries, value, neutral, labels, bands, colors, label, family }) {
  const { cx, cy, radius, arcW } = g;
  const at = (v) => angleOf(v, min, max);
  if (!ring(ctx, cx, cy, radius, arcW, START, END, null, 0, colors.track)) return;
  let a1 = START;
  let a2 = at(value);
  if (neutral !== null) [a1, a2] = value < neutral ? [at(value), at(neutral)] : [at(neutral), at(value)];
  if (!ring(ctx, cx, cy, radius - 1, arcW - 2, a1, a2, colors.arc, 1, colors.arc)) return;
  if (neutral !== null) {
    const d = (max - min) / 600;
    if (!ring(ctx, cx, cy, radius - 2, arcW - 4, at(neutral - d), at(neutral + d), colors.arc, 2, colors.track)) return;
  }
  if (bands) {
    let from = START;
    for (const e of entries) {
      const to = at(e.value);
      if (!ring(ctx, cx, cy, radius + g.markW, g.markW - 2, from, to, e.paint, 1, e.paint)) return;
      from = to;
    }
  }
  if (labels) {
    const dist = g.labelMargin + g.labelSize / 2 + radius;
    const put = (v, a) => {
      const p = a * Math.PI;
      text(ctx, String(v), Math.trunc(cx + dist * Math.cos(p)), Math.trunc(cy + dist * Math.sin(p)), g.labelSize, colors.label, family, 180 * a + 90);
    };
    put(min, START);
    put(max, END);
    entries.forEach((e) => e.value > min && e.value < max && put(e.value, at(e.value)));
    if (neutral !== null && neutral > min && neutral < max) put(neutral, at(neutral));
  }
  text(ctx, label, Math.trunc(cx), cy - g.valueSize / 2 + (g.valueSize * LINE) / 2, g.valueSize, colors.value, family);
}

export function render(host, ctx) {
  const { panel, fields } = ctx;
  const o = panel.options;
  const orientation = o.orientation || "auto";
  const labels = o.show_threshold_labels === true;
  const markers = o.show_threshold_markers !== false;
  const neutral = Number.isFinite(o.neutral) ? o.neutral : null;
  const manual = o.sizing === "manual";
  // The scale of each number field: the document, else the unit, else the range of all the number fields (Grafana). The display of a
  // value (its colour in the percentage and the continuous modes) uses the same scale.
  const range = sharedRange(fields);
  const scaled = fields.map((f) => {
    if (f.kind !== "number" && f.kind !== "number_text") return f;
    const s = scaleOf(f.config, range);
    return { ...f, config: { ...f.config, min: s.min, max: s.max, set: { ...f.config.set, min: s.min, max: s.max } } };
  });
  const items = reduceItems({ ...ctx, fields: scaled }, o.reduce, MAX_ROWS);
  const noValueText = fields[0] && fields[0].config.no_value;
  const titled = items.length > 1 || !!(panel.fields && panel.fields.defaults && panel.fields.defaults.display_name);
  const root = el("div", { class: "gauge", role: "img", "aria-label": chartLabel(panel), dataset: { testid: `gauge-${panel.id}` } });
  const desc = el("p", { class: "visually-hidden", id: `gauge-desc-${panel.id}-${++seq}`, dataset: { testid: `gauge-${panel.id}-desc` } });
  root.setAttribute("aria-describedby", desc.id);
  const cells = items.map((item) => {
    const cfg = item.field.config;
    const empty = item.display.empty;
    const label = empty && !noValueText && item.text === "" ? "No data" : item.text;
    const box = el("div", { class: "gauge__item", dataset: { testid: `gauge-item-${panel.id}` } });
    const canvas = el("canvas", { "aria-hidden": "true" });
    const title = el("div", { class: "gauge__title", text: item.name, title: item.name });
    title.hidden = !titled;
    box.append(canvas, title);
    return { item, cfg, label, box, canvas, title };
  });
  root.append(...cells.map((c) => c.box));
  host.append(root, desc);
  const named = (s) => (s.length > MAX_NAME ? `${s.slice(0, MAX_NAME)}…` : s);
  desc.textContent = cells.map((c) => `${named(c.item.name)}: ${c.label}.`).join(" ");

  const measure = document.createElement("canvas").getContext("2d");
  const family = getComputedStyle(host).getPropertyValue("--font-sans").trim() || "Inter, sans-serif";
  let drawn = [];

  function paint(c, parts, dark) {
    const { item, cfg } = c;
    const thresholds = cfg.thresholds || { mode: "absolute", steps: [{ value: null, color: "green" }] };
    const decimals = cfg.decimals ?? 2;
    const pctMode = thresholds.mode === "percentage";
    let min = Number.isFinite(item.min) ? item.min : 0;
    let max = Number.isFinite(item.max) ? item.max : 0;
    let value = item.display.empty ? 0 : item.value;
    const resolve = (color) => canvasColor(host, resolveFill(color, dark));
    const textColor = canvasColor(host, "var(--text)");
    // The colour of the value: its display (a mapping, a threshold step, a fixed or a continuous colour), else the base step.
    const own = fillOf(item.display, dark) || (thresholds.steps.length ? resolveFill(thresholds.steps[0].color, dark) : textColor);
    const valueColor = canvasColor(host, own);
    let entries;
    if (cfg.color && cfg.color.mode === "thresholds" && thresholds.steps.length) entries = bandEntries(thresholds, min, max, decimals).map((e) => ({ ...e, paint: resolve(e.color) }));
    else entries = [{ value: min, color: own, paint: valueColor }];
    if (pctMode) {
      value = ((value - min) / (max - min)) * 100;
      [min, max] = [0, 100];
    }
    if (markers) [min, max] = [parseFloat(min.toFixed(decimals)), parseFloat(max.toFixed(decimals))];
    const bands = markers && entries.length > 1;
    measure.font = "400 14px Inter";
    const w14 = measure.measureText(c.label).width;
    const g = gaugeGeometry({ cw: parts.canvasW, ch: parts.canvasH, sizeW: parts.sizeW, sizeH: parts.sizeH, w14, labels, markers, bands, valueSize: Number.isFinite(o.value_size) ? o.value_size : null });
    const dpr = window.devicePixelRatio || 1;
    c.canvas.width = Math.floor(parts.canvasW * dpr);
    c.canvas.height = Math.floor(parts.canvasH * dpr);
    const g2 = c.canvas.getContext("2d");
    g2.setTransform(dpr, 0, 0, dpr, 0, 0);
    g2.clearRect(0, 0, parts.canvasW, parts.canvasH);
    const panelColor = canvasColor(host, "var(--panel)");
    const track = mix(textColor, panelColor, TRACK_MIX);
    const valueInk = legibleOn(valueColor, panelColor, g.valueSize >= 24 ? 3 : 4.5);
    drawGauge(g2, g, { min, max, entries, value, neutral, labels, bands, colors: { track, arc: arcEntry(entries, value).paint, value: valueInk, label: textColor }, label: c.label, family });
    return { ...g, min, max, value, label: c.label, bands };
  }

  function layout() {
    const { w, h } = innerSize(host);
    if (!(w > 0) || !(h > 0) || !cells.length) return;
    const n = cells.length;
    const vertical = orientation === "vertical";
    const minW = manual && vertical && Number.isFinite(o.min_width) ? o.min_width : MIN_CELL;
    const minH = manual && orientation === "horizontal" && Number.isFinite(o.min_height) ? o.min_height : MIN_CELL;
    const boxes = orientation === "auto" ? gridCells(n, w, h) : stackCells(n, w, h, vertical, vertical ? minW : minH);
    const last = boxes[n - 1];
    root.style.overflowX = orientation === "vertical" && last.x + last.w > w + 0.5 ? "auto" : "hidden";
    root.style.overflowY = orientation === "horizontal" && last.y + last.h > h + 0.5 ? "auto" : "hidden";
    const dark = document.documentElement.dataset.theme !== "light";
    const titleSize = Number.isFinite(o.title_size) ? o.title_size : null;
    drawn = cells.map((c, i) => {
      const b = boxes[i];
      const parts = cellParts({ w: b.w, h: b.h, title: titled, orientation, titleSize });
      Object.assign(c.box.style, { left: `${b.x}px`, top: `${b.y}px`, width: `${b.w}px`, height: `${b.h}px` });
      Object.assign(c.canvas.style, { top: `${parts.top}px`, width: `${parts.canvasW}px`, height: `${parts.canvasH}px` });
      Object.assign(c.title.style, { top: `${parts.titleY}px`, width: `${parts.titleW}px`, height: `${parts.titleH}px`, fontSize: `${parts.titleSize}px` });
      const r = paint(c, parts, dark);
      return { x: b.x, y: b.y, w: b.w, h: b.h, canvasW: parts.canvasW, canvasH: parts.canvasH, titleSize: titled ? parts.titleSize : 0, cx: r.cx, cy: r.cy, radius: r.radius, arcW: r.arcW, markW: r.markW, valueSize: r.valueSize, labelSize: r.labelSize, min: r.min, max: r.max, bands: r.bands, text: r.label };
    });
    root.dataset.gauge = JSON.stringify(drawn);
  }

  layout();
  const observer = new ResizeObserver(() => layout());
  observer.observe(host);
  const again = () => layout();
  if (document.fonts) {
    if (document.fonts.ready) document.fonts.ready.then(again);
    if (document.fonts.addEventListener) document.fonts.addEventListener("loadingdone", again);
  }
  document.addEventListener("hune-theme", again);
  return {
    destroy() {
      observer.disconnect();
      document.removeEventListener("hune-theme", again);
      if (document.fonts && document.fonts.removeEventListener) document.fonts.removeEventListener("loadingdone", again);
      root.remove();
      desc.remove();
    },
  };
}

// The colour `a` at the share `k` over the colour `b`, as an rgb text (pure). A colour that is not a hex or an rgb text gives `b`.
export function mix(a, b, k) {
  const x = parseCss(a);
  const y = parseCss(b);
  if (!x || !y) return b;
  const at = (c) => Math.round(x[c] * k + y[c] * (1 - k));
  return `rgb(${at("r")}, ${at("g")}, ${at("b")})`;
}
