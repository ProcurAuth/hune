// The gradient modes of a series (4.28, 4.30, 9.19; wave H, item P10), as Grafana 12.4.2 draws them. Pure module: no DOM access.
// The engine calls the paint function of a series (E43) for its fill and its line; null keeps the plain colour.

import { CONTINUOUS_MODES, PANEL_BACKGROUND, QCHART, isContinuousMode, parseCss, resolveFill } from "../colors.js";

// The colours of `hue`: the steps of tinycolor2 that Grafana calls, with its rounding (an HSL percent keeps two decimals; `brighten` rounds first).
// The HSL helpers of colors.js round each channel, so they cannot give these digits; this file keeps its own steps.
const frac = (v, max) => (Math.abs(v - max) < 1e-6 ? 1 : (Math.min(max, Math.max(0, v)) % max) / max);
const cut = (f) => frac(parseInt(Math.min(100, Math.max(0, f * 100)) * 100, 10) / 100, 100);
const unit = (v) => Math.min(1, Math.max(0, v));

function toHsl(c) {
  const [r, g, b] = [frac(c.r, 255), frac(c.g, 255), frac(c.b, 255)];
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (!d) return { h: 0, s: 0, l };
  const h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return { h: h * 60, s: l > 0.5 ? d / (2 - max - min) : d / (max + min), l };
}

function fromHsl({ h, s, l }) {
  const [hh, ss, ll] = [frac(h, 360), cut(s), cut(l)];
  const q = ll < 0.5 ? ll * (1 + ss) : ll + ss - ll * ss;
  const p = 2 * ll - q;
  const ch = (t) => {
    const u = t < 0 ? t + 1 : t > 1 ? t - 1 : t;
    const f = ss === 0 ? ll : u < 1 / 6 ? p + (q - p) * 6 * u : u < 1 / 2 ? q : u < 2 / 3 ? p + (q - p) * (2 / 3 - u) * 6 : p;
    const v = Math.min(255, Math.max(0, f * 255));
    return v < 1 ? Math.round(v) : v;
  };
  return { r: ch(hh + 1 / 3), g: ch(hh), b: ch(hh - 1 / 3) };
}

const hsl = (c, key, n) => { const x = toHsl(c); x[key] = unit(x[key] + n / 100); return fromHsl(x); };
const spin = (c, n) => { const x = toHsl(c); x.h = (((x.h + n) % 360) + 360) % 360; return fromHsl(x); };
const brighten = (c, n) => { const d = Math.round(255 * -(n / 100)); const ch = (v) => Math.max(0, Math.min(255, Math.round(v) - d)); return { r: ch(c.r), g: ch(c.g), b: ch(c.b) }; };
const round = (c) => ({ r: Math.round(c.r), g: Math.round(c.g), b: Math.round(c.b) });

/** The [top, bottom] colours of `hue` for { r, g, b }: #73BF69 gives #9AF3AE and #719A42 (dark) or #8AB757 (light), as in Grafana. */
export function hueStops(c, dark) {
  const top = hsl(brighten(spin(hsl(c, "s", 20), 20), 10), "l", 10);
  const low = hsl(spin(c, -25), "l", -5);
  return [round(top), round(dark ? hsl(low, "l", -10) : low)];
}

export const rgbaText = (c, a) => `rgba(${Math.round(c.r)}, ${Math.round(c.g)}, ${Math.round(c.b)}, ${+Math.min(1, Math.max(0, a)).toFixed(3)})`;

/** A colour value as Grafana resolves a theme name (`resolveFill`: a token has the shade of the theme), `text` is `textColor`, `transparent` has the alpha 0. */
export function stopColor(value, dark, textColor) {
  if (value === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
  if (value === "text") return textColor || null;
  return parseCss(value === "panel" ? PANEL_BACKGROUND[dark ? "dark" : "light"] : resolveFill(value, dark));
}

/**
 * The stops [[value, colour]] of `scheme` (Grafana `getScaleGradientFn`) from `lo` to `hi`, or null for `fixed`: a hard stop at each threshold step
 * (a percent step is a share of lo..hi), or the colours of a continuous mode or of the series palette spread evenly.
 */
export function schemeStops(config, lo, hi, dark, textColor) {
  const mode = config && config.color ? config.color.mode : "thresholds";
  const spread = (list) => ({ stops: list.map((c, i) => [lo + ((hi - lo) * i) / Math.max(1, list.length - 1), c]), discrete: false });
  if (isContinuousMode(mode)) return spread(CONTINUOUS_MODES[mode].map((s) => stopColor(s, dark)));
  if (mode === "palette") return spread(QCHART[dark ? "dark" : "light"].map(parseCss));
  const t = mode === "thresholds" && config && config.thresholds;
  if (!t || !t.steps || !t.steps.length) return null;
  const at = (v) => (t.mode === "percentage" ? lo + ((hi - lo) * v) / 100 : v);
  return { stops: t.steps.map((s, i) => [i && s.value !== null ? at(s.value) : -Infinity, stopColor(s.color, dark, textColor) || { r: 0, g: 0, b: 0, a: 0 }]), discrete: true };
}

/** The ends of a scheme (Grafana `getGradientRange`): `min` or `soft_min` of the field, else the drawn data; one value gives the scale ends. */
export function schemeRange(set, raw, scale) {
  const pick = (a, b) => (Number.isFinite(a) ? a : Number.isFinite(b) ? b : null);
  let [lo, hi] = [pick(set.min, set.soft_min), pick(set.max, set.soft_max)];
  if (lo === null || hi === null) {
    const data = raw && raw[0] < raw[1] ? raw : scale;
    lo = lo ?? data[0];
    hi = hi ?? data[1];
  }
  return [lo, hi];
}

/**
 * The gradient of a scale (uPlot `scaleGradient`): `{ from, to, stops: [[offset, css]] }` in px, or one CSS colour. `stops` are [[value, css]] in
 * value order, `min` and `max` the scale ends, `pos(v)` the place of a value rounded to the device pixel. `discrete` makes hard edges.
 */
export function scaleGradient(stops, discrete, min, max, pos) {
  let a = null;
  let b = null;
  for (let i = 0; i < stops.length; i++) {
    if (stops[i][0] <= min || a === null) a = i;
    b = i;
    if (stops[i][0] >= max) break;
  }
  if (a === null) return null;
  if (a === b) return stops[a][1];
  const from = pos(stops[a][0] === -Infinity ? min : stops[a][0]);
  const to = pos(stops[b][0] === Infinity ? max : stops[b][0]);
  if (!(Math.abs(from - to) > 0)) return stops[b][1];
  const out = [];
  for (let i = a; i <= b; i++) {
    const f = Math.min(1, Math.max(0, (from - (i === a ? from : i === b ? to : pos(stops[i][0]))) / (from - to)));
    if (discrete && i > a) out.push([f, stops[i - 1][1]]);
    out.push([f, stops[i][1]]);
  }
  return { from, to, stops: out };
}

/**
 * The gradient of one part ("fill" or "line") of a series: `{ from, to, stops }`, one CSS colour, or null for the plain colour. `opacity` fills from
 * the colour at `alpha` (the start of `ends`: the top, or the right of a horizontal bar) to the alpha 0, and keeps the plain line. `hue` paints the
 * fill and the line from the two colours of `hueStops`. `scheme` paints them with the colour of the colour mode at each value.
 */
export function partGradient(mode, part, { c, alpha, dark, config, text, ends, scale, raw, pos }) {
  if (mode === "opacity" || mode === "hue") {
    if (mode === "opacity" && part !== "fill") return null;
    const [top, low] = mode === "hue" ? hueStops(c, dark) : [c, c];
    return { from: ends[0], to: ends[1], stops: [[0, rgbaText(top, alpha)], [1, rgbaText(low, mode === "hue" ? alpha : 0)]] };
  }
  if (mode !== "scheme") return null;
  const [lo, hi] = schemeRange((config && config.set) || {}, raw, scale);
  const scheme = schemeStops(config, lo, hi, dark, text);
  if (!scheme) return null;
  return scaleGradient(scheme.stops.map(([v, col]) => [v, rgbaText(col, col.a === 0 ? 0 : alpha)]), scheme.discrete, scale[0], scale[1], pos);
}

/**
 * The paint function of a series for the engine (E43), or undefined for `none`. `env` has `config`, `dark()`, `text()` (the text colour) and
 * `raw(L)` (the drawn ends of the scale, or null). A log scale gives the view in the units of the values as `L.real`.
 */
export function seriesPaint(mode, env) {
  if (mode !== "opacity" && mode !== "hue" && mode !== "scheme") return undefined;
  return (part, ctx, L, c, alpha) => {
    const V = L.real || L;
    const k = typeof ctx.getTransform === "function" ? ctx.getTransform().a || 1 : 1;
    const g = partGradient(mode, part, {
      c, alpha, dark: env.dark(), config: env.config, text: env.text(), ends: [V.top, V.top + V.plotH], scale: [V.yMin, V.yMax], raw: env.raw(L),
      pos: (v) => Math.round(V.yOf(v) * k) / k,
    });
    if (!g || typeof g === "string") return g;
    const grad = ctx.createLinearGradient(0, g.from, 0, g.to);
    for (const [f, css] of g.stops) grad.addColorStop(f, css);
    return grad;
  };
}

/** A gradient along x as one CSS `linear-gradient` from the left of a track of `width` px (the stops in the order of their place). */
export function cssGradient(g, width) {
  if (!g || typeof g === "string" || !(width > 0)) return g || null;
  const list = g.stops.map(([f, css]) => [g.from + (g.to - g.from) * f, css]);
  if (g.from > g.to) list.reverse();
  return `linear-gradient(90deg, ${list.map(([x, css]) => `${css} ${x.toFixed(2)}px`).join(", ")})`;
}
