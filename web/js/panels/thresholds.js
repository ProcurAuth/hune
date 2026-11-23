// The thresholds in the plot of a time series or a bar chart (9.19, decision F12). The shapes are pure data. The draw function is the only
// part that touches a canvas. The page draws the shapes under the series. They never change the range of the y axis.

import { NAMED, parseCss, resolveColor } from "../colors.js";

const LINE_ALPHA = 0.7;
const AREA_ALPHA = 0.15;

/** The parts of a style: `{ lines, dashed, areas }`. The style `off` has none. */
export function styleParts(style) {
  switch (style) {
    case "line": return { lines: true, dashed: false, areas: false };
    case "dashed": return { lines: true, dashed: true, areas: false };
    case "area": return { lines: false, dashed: false, areas: true };
    case "line_area": return { lines: true, dashed: false, areas: true };
    case "dashed_area": return { lines: true, dashed: true, areas: true };
    default: return { lines: false, dashed: false, areas: false };
  }
}

// True for the colour `transparent` or a colour with the alpha 0.
function isTransparent(color) {
  if (color === "transparent") return true;
  const p = parseCss(color);
  return !!p && p.a === 0;
}

/**
 * The value of each step on the value axis. The first step has no value (minus infinity). In the mode `percentage`, the value of a step is
 * `lo + (hi - lo) * p / 100`. `lo` and `hi` are `min` and `max` of the field (`bounds`), else the ends of the axis (`axis`).
 * Returns an array of `{ value, color }`, where `value` is a number or `-Infinity`.
 */
export function stepValues(thresholds, bounds, axis) {
  const steps = (thresholds && thresholds.steps) || [];
  const lo = Number.isFinite(bounds && bounds[0]) ? bounds[0] : axis[0];
  const hi = Number.isFinite(bounds && bounds[1]) ? bounds[1] : axis[1];
  return steps.map((step, i) => {
    if (i === 0 || step.value === null || step.value === undefined) return { value: -Infinity, color: step.color };
    const value = thresholds.mode === "percentage" ? lo + ((hi - lo) * step.value) / 100 : step.value;
    return { value, color: step.color };
  });
}

/**
 * The lines and the areas to draw, in the document colours. `axis` is `[min, max]` of the value axis.
 * - Lines: one for each step except the first, at its value. A line outside the axis is not drawn.
 * - Areas: the band of a step goes from its value to the value of the next step. The first band starts at the bottom, the last ends at the top.
 * - A step with the colour `transparent` draws nothing. Each line from the second step to the first transparent step takes the colour of the
 *   step before it.
 * Returns `{ lines: [{ value, color }], areas: [{ from, to, color }] }`. Areas are in value order and cut to the axis.
 */
export function thresholdShapes(thresholds, style, bounds, axis) {
  const parts = styleParts(style);
  const out = { lines: [], areas: [] };
  const steps = stepValues(thresholds, bounds, axis);
  if (!steps.length || (!parts.lines && !parts.areas)) return out;
  const [min, max] = axis;
  if (parts.areas) {
    for (let i = 0; i < steps.length; i++) {
      if (isTransparent(steps[i].color)) continue;
      const from = Math.max(min, steps[i].value);
      const to = Math.min(max, i + 1 < steps.length ? steps[i + 1].value : Infinity);
      if (to > from) out.areas.push({ from, to, color: steps[i].color });
    }
  }
  if (parts.lines) {
    const firstTransparent = steps.findIndex((s) => isTransparent(s.color));
    for (let i = 1; i < steps.length; i++) {
      const color = firstTransparent >= 0 && i <= firstTransparent ? steps[i - 1].color : steps[i].color;
      if (isTransparent(color)) continue;
      const value = steps[i].value;
      if (!(value >= min && value <= max)) continue;
      out.lines.push({ value, color });
    }
  }
  return out;
}

// The CSS colour of a document colour for the canvas. A named token reads the variable of the page (the light theme darkens the tokens).
function canvasCss(host, color) {
  if (typeof color === "string" && Object.hasOwn(NAMED, color)) {
    const text = getComputedStyle(host).getPropertyValue(`--viz-${color}`).trim();
    if (text) return text;
  }
  if (color === "text") return getComputedStyle(host).getPropertyValue("--text").trim() || "#ccc";
  return resolveColor(color);
}

// The alpha that a colour keeps, or null when it has none (the page then gives the default alpha).
function ownAlpha(color) {
  const p = parseCss(color);
  return p && p.a < 1 ? p.a : null;
}

/**
 * The `drawUnder` hook of the chart engine for the thresholds of one field. `read()` gives the current `{ thresholds, style, bounds }`.
 * The hook draws the areas first, then the lines. A colour that has no alpha gets 0.7 for a line and 0.15 for an area.
 */
export function thresholdsHook(host, read) {
  return (ctx, L, env = {}) => {
    const { thresholds, style, bounds } = read();
    const shapes = thresholdShapes(thresholds, style, bounds, [L.yMin, L.yMax]);
    const dpr = env.dpr || 1;
    // The test hook (9.29): what the hook draws, as data. It holds the plot box and the axis, the lines and the bands with their position
    // (CSS px in the canvas), their CSS colour and their alpha. The attribute is the text alternative of the canvas for the tests only.
    const snapshot = { style, plot: { left: L.left, top: L.top, w: L.plotW, h: L.plotH }, axis: [L.yMin, L.yMax], dashed: styleParts(style).dashed, areas: [], lines: [] };
    if (!shapes.lines.length && !shapes.areas.length) {
      host.dataset.thresholds = JSON.stringify(snapshot);
      return;
    }
    ctx.save();
    for (const a of shapes.areas) {
      const css = canvasCss(host, a.color);
      const keep = ownAlpha(a.color);
      ctx.globalAlpha = keep !== null ? 1 : AREA_ALPHA;
      ctx.fillStyle = css;
      const y1 = L.yOf(a.to);
      const y0 = L.yOf(a.from);
      ctx.fillRect(L.left, y1, L.plotW, y0 - y1);
      snapshot.areas.push({ from: a.from, to: a.to, top: y1, bottom: y0, color: css, alpha: keep !== null ? keep : AREA_ALPHA });
    }
    const dashed = snapshot.dashed;
    ctx.lineWidth = 2;
    ctx.setLineDash(dashed ? [10, 10] : []);
    for (const line of shapes.lines) {
      const css = canvasCss(host, line.color);
      const keep = ownAlpha(line.color);
      ctx.globalAlpha = keep !== null ? 1 : LINE_ALPHA;
      ctx.strokeStyle = css;
      const y = Math.round(L.yOf(line.value) * dpr) / dpr;
      ctx.beginPath();
      ctx.moveTo(L.left, y);
      ctx.lineTo(L.left + L.plotW, y);
      ctx.stroke();
      snapshot.lines.push({ value: line.value, y, color: css, alpha: keep !== null ? keep : LINE_ALPHA });
    }
    ctx.restore();
    host.dataset.thresholds = JSON.stringify(snapshot);
  };
}
