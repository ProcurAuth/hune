// The pie chart panel (9.19, 4.31): Hune code draws the slices on a canvas.
// A label shows only when its whole box is inside its slice: it never crosses an edge and it never clips.

import { fieldColorValue, fillOf, readableTextOn, resolveColor, resolveFill } from "../colors.js";
import { el } from "../ui.js";
import { legibleOn, reduceItems } from "./items.js";
import { canvasColor } from "./common.js";

const TAU = Math.PI * 2;
const SIZES = [12, 11, 10]; // the label font sizes to try, in px
const PAD = 3;
const EDGE = 4; // the least distance (px) between a label and the edge of its slice

// True when the point (x, y), relative to the centre, is in the sector from `a0` of `sweep` radians
// between the radii `r0` and `r1`, at `margin` px or more from its two straight edges (pure).
export function inSector(x, y, a0, sweep, r0, r1, margin = 0) {
  const r = Math.hypot(x, y);
  if (r < r0 || r > r1) return false;
  if (sweep >= TAU - 1e-9) return true;
  let d = (Math.atan2(y, x) - a0) % TAU;
  if (d < 0) d += TAU;
  const m = r > 0 ? margin / r : 0;
  return d >= m && d <= sweep - m;
}

// True when the box (centre cx, cy; size w, h; relative to the pie centre) is inside the sector (pure).
export function boxInSector(cx, cy, w, h, a0, sweep, r0, r1, margin = 0) {
  const xs = [cx - w / 2, cx, cx + w / 2];
  const ys = [cy - h / 2, cy, cy + h / 2];
  for (const x of xs) for (const y of ys) if (!inSector(x, y, a0, sweep, r0, r1, margin)) return false;
  return true;
}

/**
 * The best label of a slice: { lines, x, y, size } relative to the pie centre, or null (pure).
 * `parts` are the texts (name, value, percent). `measure(text)` gives the width in px at 12 px.
 * It tries all the parts, then fewer parts from the left. It tries one line, then one line for each part.
 * For each text it tries three font sizes, several radii and a few angles inside the slice.
 */
export function placeLabel({ parts, a0, sweep, inner, radius, measure }) {
  if (!parts.length || sweep <= 0) return null;
  const mid = a0 + sweep / 2;
  const r0 = inner + PAD;
  const r1 = radius - PAD;
  const band = r1 - r0;
  if (band < 10) return null;
  const radii = [0.5, 0.35, 0.65, 0.2, 0.8, 0.05, 0.95];
  const shifts = [0, -0.25, 0.25, -0.5, 0.5].map((k) => k * (sweep / 2));
  for (let k = 0; k < parts.length; k++) {
    const rest = parts.slice(k);
    const options = rest.length > 1 ? [[rest.join(" ")], rest] : [rest];
    for (const lines of options) {
      for (const size of SIZES) {
        const lineH = size + 2;
        const w = Math.max(...lines.map(measure)) * (size / SIZES[0]) + PAD * 2;
        const h = lines.length * lineH;
        for (const f of radii) {
          const rr = r0 + band * f;
          for (const shift of shifts) {
            const x = Math.cos(mid + shift) * rr;
            const y = Math.sin(mid + shift) * rr;
            if (boxInSector(x, y, w, h, a0, sweep, r0, r1, EDGE)) return { lines, x, y, size, lineH };
          }
        }
      }
    }
  }
  return null;
}

// The percent of a slice: the decimals of the field (0 when the field sets none), as Grafana shows "33%" and "33.2%".
export function percentText(amount, total, decimals) {
  if (!(total > 0)) return "";
  const d = Number.isInteger(decimals) && decimals >= 0 ? decimals : 0;
  return `${((amount / total) * 100).toFixed(d)}%`;
}

/**
 * The slices in the order of the drawing (pure): the largest value first, a tie keeps the order of the data. The first slice starts at
 * 12 o'clock and the slices go clockwise (measured on Grafana 12.4.2). The colour of a slice stays that of its place in the data.
 */
export function drawOrder(slices) {
  return slices.map((s, i) => ({ s, i })).sort((a, b) => b.s.amount - a.s.amount || a.i - b.i).map((x) => x.s);
}

const collator = new Intl.Collator("en", { sensitivity: "base" });

/**
 * The rows of a legend table sorted by `sort_by` (9.19, F20): the key is the name, the value or the percent of the slice. A row with
 * no key (null or NaN) goes last in both directions, and equal keys keep the order of `slices`.
 */
export function legendOrder(slices, sortBy, sortDesc) {
  if (!sortBy) return slices;
  const key = (s) => (sortBy === "name" ? s.name : sortBy === "percent" ? s.amount : s.value);
  return slices.map((s, i) => ({ s, i })).sort((a, b) => {
    const x = key(a.s);
    const y = key(b.s);
    const nx = x === null || x === undefined || Number.isNaN(x);
    const ny = y === null || y === undefined || Number.isNaN(y);
    if (nx || ny) return nx === ny ? a.i - b.i : nx ? 1 : -1;
    const c = sortBy === "name" ? collator.compare(String(x), String(y)) : x - y;
    return c ? (sortDesc ? -c : c) : a.i - b.i;
  }).map((r) => r.s);
}

function labelParts(labels, slice, total, decimals) {
  const parts = [];
  for (const label of labels || []) {
    if (label === "name") parts.push(slice.name);
    else if (label === "value") parts.push(slice.text);
    else if (label === "percent") parts.push(percentText(slice.amount, total, decimals));
  }
  return parts.filter((t) => t !== "");
}

function drawSlices(canvas, host, slices, o, hover, decimals) {
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  canvas.width = Math.max(1, Math.round(w * dpr));
  canvas.height = Math.max(1, Math.round(h * dpr));
  const ctx = canvas.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  const radius = Math.max(10, Math.min(w, h) / 2 - 8);
  const cx = w / 2;
  const cy = h / 2;
  const total = slices.reduce((s, x) => s + x.amount, 0);
  const edge = canvasColor(host, "var(--panel)");
  const textColor = canvasColor(host, "var(--text)");
  const font = getComputedStyle(host).getPropertyValue("--font-sans").trim() || "sans-serif";
  const inner = o.pie_type === "donut" ? radius * 0.55 : 0;
  const hits = [];
  if (total <= 0) {
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, TAU);
    if (inner > 0) ctx.arc(cx, cy, inner, 0, TAU, true);
    ctx.strokeStyle = canvasColor(host, "var(--border-strong)");
    ctx.lineWidth = 1;
    ctx.stroke();
    return { hits, cx, cy, radius, inner, total };
  }
  ctx.font = `600 ${SIZES[0]}px ${font}`;
  const measure = (t) => ctx.measureText(t).width;
  const whole = slices.filter((s) => s.amount > 0).length === 1;
  let angle = -Math.PI / 2;
  const labels = [];
  slices.forEach((s, index) => {
    if (s.amount <= 0) return;
    const sweep = (s.amount / total) * TAU;
    const R = radius + (hover === index ? 4 : 0);
    ctx.beginPath();
    if (whole) {
      ctx.arc(cx, cy, R, 0, TAU);
      if (inner > 0) {
        ctx.moveTo(cx + inner, cy);
        ctx.arc(cx, cy, inner, 0, TAU, true);
      }
      ctx.closePath();
    } else {
      ctx.moveTo(cx + Math.cos(angle) * inner, cy + Math.sin(angle) * inner);
      ctx.arc(cx, cy, R, angle, angle + sweep);
      if (inner > 0) ctx.arc(cx, cy, inner, angle + sweep, angle, true);
      else ctx.lineTo(cx, cy);
      ctx.closePath();
    }
    ctx.fillStyle = canvasColor(host, s.color);
    ctx.fill("evenodd");
    if (!whole) {
      ctx.strokeStyle = edge;
      ctx.lineWidth = 1.5;
      ctx.stroke();
    }
    hits.push({ slice: s, index, from: angle, to: angle + sweep });
    const place = placeLabel({ parts: labelParts(o.labels, s, total, decimals), a0: angle, sweep, inner, radius, measure });
    if (place) labels.push({ place, color: s.color && s.color !== "var(--text)" ? readableTextOn(resolveColor(s.color)) : textColor });
    angle += sweep;
  });
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  for (const { place, color } of labels) {
    ctx.fillStyle = color;
    ctx.font = `600 ${place.size}px ${font}`;
    const top = place.y - ((place.lines.length - 1) * place.lineH) / 2;
    place.lines.forEach((line, i) => ctx.fillText(line, cx + place.x, cy + top + i * place.lineH));
  }
  return { hits, cx, cy, radius, inner, total };
}

export function render(host, ctx) {
  const { panel } = ctx;
  const o = panel.options;
  const items = reduceItems(ctx, o.reduce);
  // Slices in data order, with the colour of the colour mode (palette by slice index).
  const slices = items.map((item, i) => ({
    name: item.name,
    amount: item.value !== null && item.value > 0 ? item.value : 0,
    value: item.value,
    text: item.text,
    // The colour in a theme: a named colour has its own light shade (colors.js NAMED_LIGHT, as Grafana); the document keeps the token.
    baseIn: (dark) => {
      if (item.display.mapped && item.color) return fillOf(item.display, dark);
      const value = fieldColorValue(item.field.config, item.value, { min: item.min, max: item.max, seriesIndex: i, dark });
      return value === null ? "var(--text)" : resolveFill(value, dark);
    },
    color: "var(--text)",
  }));
  // A slice colour needs 3:1 against the panel (graphics): in the light theme a pale colour is darkened, hue kept.
  function recolor() {
    const surface = canvasColor(host, "var(--panel)");
    const dark = document.documentElement.dataset.theme !== "light";
    slices.forEach((sl, i) => {
      sl.color = legibleOn(sl.baseIn(dark), surface, 3);
      if (sl.swatch) sl.swatch.style.background = sl.color;
    });
  }
  const legend = o.legend || { show: true, placement: "right", mode: "list", values: [] };
  const total = slices.reduce((s, x) => s + x.amount, 0);
  // The decimals of the percent: the decimals of the field (the first field of the items).
  const decimals = items.length && items[0].field.config ? items[0].field.config.decimals : null;
  const drawn = drawOrder(slices);
  const canvas = el("canvas", { class: "pie__canvas", role: "img", "aria-label": `${panel.title || "Untitled"}, pie chart` });
  const tip = el("div", { class: "pie__tip", role: "tooltip", hidden: true });
  const plot = el("div", { class: "pie__plot" }, canvas, tip);
  const root = el("div", { class: `pie pie--${legend.placement}`, dataset: { testid: `pie-${panel.id}` } }, plot);
  if (legend.show) {
    const showValue = (legend.values || []).includes("value");
    const showPercent = (legend.values || []).includes("percent");
    const extra = (showValue || legend.mode === "table" ? 1 : 0) + (showPercent ? 1 : 0);
    const list = el("ul", { class: `pie__legend pie__legend--${legend.mode}`, role: "list" });
    list.style.setProperty("--legend-cols", `var(--swatch) minmax(0, 1fr)${" auto".repeat(extra)}`);
    for (const s of legend.mode === "table" && legend.sort_by ? legendOrder(drawn, legend.sort_by, legend.sort_desc) : drawn) {
      const swatch = el("span", { class: "pie__swatch", "aria-hidden": "true" });
      s.swatch = swatch;
      const row = el("li", { class: "pie__legend-item", role: "listitem" }, swatch, el("span", { class: "pie__legend-name", text: s.name, title: s.name }));
      if (showValue || legend.mode === "table") row.append(el("span", { class: "pie__legend-value", text: s.text }));
      if (showPercent) row.append(el("span", { class: "pie__legend-value pie__legend-percent", text: percentText(s.amount, total, decimals) }));
      list.append(row);
    }
    root.append(list);
  }
  host.append(root);
  let geometry = null;
  let hover = -1;
  const paint = () => {
    recolor();
    geometry = drawSlices(canvas, host, drawn, o, hover, decimals);
  };
  paint();
  const observer = new ResizeObserver(paint);
  observer.observe(plot);
  // The canvas cannot use var(): it draws again when the theme changes.
  const onTheme = () => paint();
  document.addEventListener("hune-theme", onTheme);
  document.addEventListener("hune-shown", onTheme); // 10.2a, rule 17 (common.js, fitHeight)
  const mode = o.tooltip ? o.tooltip.mode : "single";
  function setHover(index) {
    if (index === hover) return;
    hover = index;
    paint();
  }
  function onMove(event) {
    if (mode === "hidden" || !geometry || !geometry.hits.length) return;
    const box = canvas.getBoundingClientRect();
    const dx = event.clientX - box.left - geometry.cx;
    const dy = event.clientY - box.top - geometry.cy;
    const dist = Math.hypot(dx, dy);
    if (dist > geometry.radius || dist < geometry.inner) {
      tip.hidden = true;
      setHover(-1);
      return;
    }
    let a = Math.atan2(dy, dx);
    if (a < -Math.PI / 2) a += TAU;
    const hit = geometry.hits.find((x) => a >= x.from && a < x.to);
    if (!hit) {
      tip.hidden = true;
      setHover(-1);
      return;
    }
    tip.textContent = `${hit.slice.name}: ${hit.slice.text} (${percentText(hit.slice.amount, geometry.total, decimals)})`;
    tip.hidden = false;
    const maxLeft = Math.max(0, plot.clientWidth - tip.offsetWidth - 4);
    tip.style.left = `${Math.min(event.clientX - box.left + 12, maxLeft)}px`;
    tip.style.top = `${event.clientY - box.top + 12}px`;
    setHover(hit.index);
  }
  canvas.addEventListener("pointermove", onMove);
  canvas.addEventListener("pointerleave", () => {
    tip.hidden = true;
    setHover(-1);
  });
  return {
    destroy() {
      observer.disconnect();
      document.removeEventListener("hune-theme", onTheme);
      document.removeEventListener("hune-shown", onTheme);
      root.remove();
    },
  };
}
