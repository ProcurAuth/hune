// Named colours, series palette, thresholds (9.16, FE-07). Pure module: no DOM access.

const HUES = {
  red: ["#FFA6B0", "#FF7383", "#F2495C", "#E02F44", "#C4162A"],
  orange: ["#FFCB7D", "#FFB357", "#FF9830", "#FF780A", "#FA6400"],
  yellow: ["#FFF899", "#FFEE52", "#FADE2A", "#F2CC0C", "#E0B400"],
  green: ["#C8F2C2", "#96D98D", "#73BF69", "#56A64B", "#37872D"],
  blue: ["#C0D8FF", "#8AB8FF", "#5794F2", "#3274D9", "#1F60C4"],
  purple: ["#DEB6F2", "#CA95E5", "#B877D9", "#A352CC", "#8F3BB8"],
};

// The light theme of Grafana 12.4.2 draws each named colour with the next darker shade of its hue (read by pixel probes of table
// cells in the light theme, wave G2): `super-light-red` is the hex of `light-red` of the dark theme, and so on. The step
// `dark-*` has a shade of its own. The values are exact (they are not darkened for contrast: see `fillOf` and the AA rules).
const HUES_LIGHT = {
  red: ["#FF7383", "#F2495C", "#E02F44", "#C4162A", "#AD0317"],
  orange: ["#FFB357", "#FF9830", "#FF780A", "#FA6400", "#E55400"],
  yellow: ["#FFEE52", "#FADE2A", "#F2CC0C", "#E0B400", "#CC9D00"],
  green: ["#96D98D", "#73BF69", "#56A64B", "#37872D", "#19730E"],
  blue: ["#8AB8FF", "#5794F2", "#3274D9", "#1F60C4", "#1250B0"],
  purple: ["#CA95E5", "#B877D9", "#A352CC", "#8F3BB8", "#7C2EA3"],
};

function tokenTable(hues) {
  const out = {};
  for (const [hue, row] of Object.entries(hues)) {
    out[`super-light-${hue}`] = row[0];
    out[`light-${hue}`] = row[1];
    out[hue] = row[2];
    out[`semi-dark-${hue}`] = row[3];
    out[`dark-${hue}`] = row[4];
  }
  return Object.freeze(out);
}

/** The 30 hue tokens, token -> hex. `text` and `transparent` are not in this table. */
export const NAMED = tokenTable(HUES);

/** The 30 hue tokens as the light theme draws them in a fill, token -> hex (see above). The stored value stays the token. */
export const NAMED_LIGHT = tokenTable(HUES_LIGHT);

/**
 * The classic palette of 64 colours of Grafana 12.4.2 (9.16), in the order that Grafana draws series 1, 2, 3 and so on. No colour mode draws
 * it any more: the mode `palette` draws the series palette of ClickHouse Dash (`QCHART`, below),
 * the one series palette. The list stays as the reference of the difference from Grafana (19.20a).
 */
export const PALETTE = Object.freeze([
  "green", "semi-dark-yellow", "blue", "orange", "red", "purple",
  "dark-green", "dark-yellow", "dark-blue", "dark-orange", "dark-red", "dark-purple",
  "super-light-green", "super-light-yellow", "super-light-blue", "super-light-orange", "super-light-red", "super-light-purple",
  "#447EBC", "#C15C17", "#890F02", "#0A437C", "#6D1F62", "#584477", "#B7DBAB", "#F4D598", "#70DBED", "#F9BA8F",
  "#F29191", "#82B5D8", "#E5A8E2", "#AEA2E0", "#629E51", "#E5AC0E", "#64B0C8", "#E0752D", "#BF1B00", "#0A50A1",
  "#962D82", "#614D93", "#9AC48A", "#F2C96D", "#65C5DB", "#F9934E", "#EA6460", "#5195CE", "#D683CE", "#806EB7",
  "#3F6833", "#967302", "#2F575E", "#99440A", "#58140C", "#052B51", "#511749", "#3F2B5B", "#E0F9D7", "#FCEACA",
  "#CFFAFF", "#F9E2D2", "#FCE2DE", "#BADFF4", "#F9D9F9", "#DEDAF7",
]);

/** The number of colours of the series palette. A series after the 18th takes the colour of its index modulo 18, as ClickHouse Dash does. */
export const QCHART_COUNT = 18;

/**
 * The series palette of ClickHouse Dash (9.16): the tokens `--qchart-1` to `--qchart-18` and `--qchart-other` of
 * css/vendor/chdash/00-tokens.css, as hex, for the dark and the light theme. A test keeps the two copies equal. Each colour has 3:1
 * with the panel in both themes (the test of fe07_colors). `other` is the colour of an index that is not a slot (a negative index).
 */
export const QCHART = Object.freeze({
  dark: Object.freeze([
    "#4296fb", "#e86a34", "#29ae81", "#d28f09", "#de669a", "#4ba435", "#9e8cf4", "#49c1ea", "#febad9",
    "#ddd674", "#7572ae", "#a86751", "#9fb83c", "#0695b5", "#cf95c1", "#bdbcfd", "#8e8945", "#85e2ed",
  ]),
  light: Object.freeze([
    "#1a73d5", "#c14802", "#088963", "#9c6900", "#b84379", "#227702", "#644fb1", "#046480", "#700048",
    "#433f01", "#39346a", "#7c3f2a", "#768c02", "#078ead", "#7c4972", "#7b78b4", "#656019", "#03464c",
  ]),
  other: Object.freeze({ dark: "#898781", light: "#6f6d68" }),
});

/**
 * The colour (hex) of series `i` (from 0) in a theme: slot `i mod 18` of `QCHART`, in the order of ClickHouse Dash.
 * A negative index or an index that is not a number gives the colour `other`.
 */
export function qchartColor(i, dark = true) {
  const n = Math.trunc(Number(i));
  const key = dark ? "dark" : "light";
  if (!Number.isFinite(n) || n < 0) return QCHART.other[key];
  return QCHART[key][n % QCHART_COUNT];
}

// --- Colour modes of the second set (9.16, F26, FE-23): the 10 continuous gradients ---------------------------

/** The stops of the 10 continuous modes (4.19, 9.16). A token is a colour of this file. `panel` is the panel background of the theme. */
export const CONTINUOUS_MODES = Object.freeze({
  continuous_gr_yl_rd: Object.freeze(["green", "yellow", "red"]),
  continuous_rd_yl_gr: Object.freeze(["red", "yellow", "green"]),
  continuous_bl_yl_rd: Object.freeze(["dark-blue", "super-light-yellow", "dark-red"]),
  continuous_yl_rd: Object.freeze(["super-light-yellow", "dark-red"]),
  continuous_bl_pu: Object.freeze(["blue", "purple"]),
  continuous_yl_bl: Object.freeze(["super-light-yellow", "dark-blue"]),
  continuous_blues: Object.freeze(["panel", "dark-blue"]),
  continuous_reds: Object.freeze(["panel", "dark-red"]),
  continuous_greens: Object.freeze(["panel", "dark-green"]),
  continuous_purples: Object.freeze(["panel", "dark-purple"]),
});

/** The background of the panel in a theme (Grafana values): the stop `panel` of a gradient. */
export const PANEL_BACKGROUND = Object.freeze({ dark: "#181B1F", light: "#FFFFFF" });

/** True for one of the 10 continuous modes. */
export function isContinuousMode(mode) {
  return typeof mode === "string" && Object.hasOwn(CONTINUOUS_MODES, mode);
}

/** True when the colour of a value depends on the theme by more than the token shade: `palette` and the continuous modes. */
export function isThemedMode(mode) {
  return mode === "palette" || isContinuousMode(mode);
}

const hex2 = (n) => n.toString(16).toUpperCase().padStart(2, "0");

function stopRgb(stop, dark) {
  const css = stop === "panel" ? (dark ? PANEL_BACKGROUND.dark : PANEL_BACKGROUND.light) : (dark ? NAMED : NAMED_LIGHT)[stop];
  const h = css.slice(1);
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
}

// One channel of the uniform cubic B-spline through the stops (the function `interpolateRgbBasis` of d3, 9.16). `vs` has 2 stops or more.
export function basisChannel(vs, f) {
  const m = vs.length - 1;
  let i;
  let t;
  if (f <= 0) { i = 0; t = 0; } else if (f >= 1) { i = m - 1; t = 1; } else { i = Math.floor(f * m); t = f * m - i; }
  const a = vs[i];
  const b = vs[i + 1];
  const p = i > 0 ? vs[i - 1] : 2 * a - b;
  const q = i < m - 1 ? vs[i + 2] : 2 * b - a;
  const t2 = t * t;
  const t3 = t2 * t;
  const v = ((1 - 3 * t + 3 * t2 - t3) * p + (4 - 6 * t2 + 3 * t3) * a + (1 + 3 * t + 3 * t2 - 3 * t3) * b + t3 * q) / 6;
  return Math.max(0, Math.min(255, Math.round(v)));
}

/**
 * The fraction `f` of a value on the scale `min` to `max` for a continuous mode (9.16): `(v - min) / (max - min)` limited to 0 to 1.
 * A value that is null, NaN or -Infinity gives 0, and +Infinity gives 1. A scale with no width, or with an end that is not a number, gives 0.
 */
export function continuousFraction(value, min, max) {
  if (value === Infinity) return 1;
  if (typeof value !== "number" || !Number.isFinite(value)) return 0;
  if (!Number.isFinite(min) || !Number.isFinite(max) || !(max > min)) return 0;
  return Math.max(0, Math.min(1, (value - min) / (max - min)));
}

/** The hex colour (`#RRGGBB`) of a continuous mode at the place `f` from 0 to 1, in a theme (9.16). An unknown mode gives null. */
export function gradientColor(mode, f, dark = true) {
  if (!isContinuousMode(mode)) return null;
  const place = f === Infinity ? 1 : typeof f === "number" && Number.isFinite(f) ? Math.max(0, Math.min(1, f)) : 0;
  const rgb = CONTINUOUS_MODES[mode].map((stop) => stopRgb(stop, dark));
  return `#${[0, 1, 2].map((c) => hex2(basisChannel(rgb.map((s) => s[c]), place))).join("")}`;
}

export const HEX_RE = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
export const RGB_RE = /^rgb\((0|[1-9][0-9]{0,2}), (0|[1-9][0-9]{0,2}), (0|[1-9][0-9]{0,2})\)$/;
export const RGBA_RE = /^rgba\((0|[1-9][0-9]{0,2}), (0|[1-9][0-9]{0,2}), (0|[1-9][0-9]{0,2}), (0|1|0\.[0-9]{1,3})\)$/;

/** A CSS colour string for a colour value. A token gives its hex. `text` gives `var(--text)`. */
export function resolveColor(c) {
  if (c === "text") return "var(--text)";
  if (c === "transparent") return "transparent";
  if (typeof c === "string" && Object.hasOwn(NAMED, c)) return NAMED[c];
  return c;
}

/** Parse a hex, rgb or rgba colour (tokens resolve first) to {r, g, b, a}, or null. */
export function parseCss(css) {
  const c = resolveColor(css);
  if (typeof c !== "string") return null;
  if (HEX_RE.test(c)) {
    let h = c.slice(1);
    if (h.length === 3) h = h.split("").map((x) => x + x).join("");
    const r = parseInt(h.slice(0, 2), 16);
    const g = parseInt(h.slice(2, 4), 16);
    const b = parseInt(h.slice(4, 6), 16);
    const a = h.length === 8 ? parseInt(h.slice(6, 8), 16) / 255 : 1;
    return { r, g, b, a };
  }
  let m = RGB_RE.exec(c);
  if (m) return { r: Number(m[1]), g: Number(m[2]), b: Number(m[3]), a: 1 };
  m = RGBA_RE.exec(c);
  if (m) return { r: Number(m[1]), g: Number(m[2]), b: Number(m[3]), a: Number(m[4]) };
  return null;
}

/** The colour of the classic palette at index i (modulo 64), resolved to CSS. */
export function paletteColor(i) {
  const n = PALETTE.length;
  return resolveColor(PALETTE[((Math.trunc(i) % n) + n) % n]);
}

/**
 * The colour (as in the document) of the active threshold step of value v (9.16), or null.
 * `min` and `max` serve the percentage mode.
 */
export function thresholdColor(thresholds, v, min, max) {
  if (typeof v !== "number" || Number.isNaN(v)) return null;
  const steps = thresholds && thresholds.steps;
  if (!steps || steps.length === 0) return null;
  let cmp = v;
  if (thresholds.mode === "percentage") {
    if (!(max !== min) || !Number.isFinite(min) || !Number.isFinite(max)) return steps[0].color;
    cmp = ((v - min) / (max - min)) * 100;
  }
  let active = steps[0];
  for (let i = 1; i < steps.length; i++) {
    const sv = steps[i].value;
    if (sv !== null && sv !== undefined && sv <= cmp) active = steps[i];
    else break;
  }
  return active.color;
}

function linear(u) {
  const s = u / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

/** The WCAG relative luminance of a colour, or null when it cannot be parsed. */
export function relativeLuminance(css) {
  const p = parseCss(css);
  if (!p) return null;
  return 0.2126 * linear(p.r) + 0.7152 * linear(p.g) + 0.0722 * linear(p.b);
}

/** The text colour on a background colour: `#000000` from luminance 0.179, else `#ffffff`. */
export function readableTextOn(css) {
  const l = relativeLuminance(css);
  if (l === null) return "#ffffff";
  return l >= 0.179 ? "#000000" : "#ffffff";
}

function rgbToHsl(r, g, b) {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  let h = 0;
  let s = 0;
  const l = (max + min) / 2;
  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h /= 6;
  }
  return { h: h * 360, s, l };
}

function hslToRgb(h, s, l) {
  h = (((h % 360) + 360) % 360) / 360;
  if (s === 0) {
    const v = Math.round(l * 255);
    return { r: v, g: v, b: v };
  }
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const hue = (t) => {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  return {
    r: Math.round(hue(h + 1 / 3) * 255),
    g: Math.round(hue(h) * 255),
    b: Math.round(hue(h - 1 / 3) * 255),
  };
}

/**
 * The CSS colour C2 of the gradient: C with HSL lightness 10 points lower and hue turned by 5 degrees. In the light theme
 * (`dark` false) Grafana raises the lightness instead, by 7 points (0.7 of the amount).
 */
export function darkenSpin(css, dark = true) {
  const p = parseCss(css);
  if (!p) return null;
  const hsl = rgbToHsl(p.r, p.g, p.b);
  const l = Math.min(1, Math.max(0, hsl.l + (dark ? -0.1 : 0.07)));
  const rgb = hslToRgb(hsl.h + 5, hsl.s, l);
  return p.a < 1 ? `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${p.a})` : `rgb(${rgb.r}, ${rgb.g}, ${rgb.b})`;
}

/** The background of a gradient cell: `linear-gradient(120deg, C2, C)`. */
export function gradientFrom(css, dark = true) {
  const c = resolveColor(css);
  const c2 = darkenSpin(c, dark);
  return c2 ? `linear-gradient(120deg, ${c2}, ${c})` : c;
}

/**
 * The colour value (as in the document: a token, a hex or an rgb text) of a value by the colour mode of a field configuration
 * (9.16), or null. `fieldColor` and `markColor` resolve it.
 * `ctx` has `min` and `max` (the scale of `thresholds` and of the continuous modes), `seriesIndex` (the index among the number fields, for
 * `palette`) and `dark` (the theme, true when it is not set).
 * `palette` and a continuous mode give a hex of the theme in `ctx.dark` (the one series palette, or a gradient).
 */
export function fieldColorValue(config, value, ctx = {}) {
  const color = config && config.color ? config.color : { mode: "thresholds", fixed: null };
  if (color.mode === "fixed") return color.fixed ? color.fixed : null;
  if (color.mode === "palette") return qchartColor(ctx.seriesIndex || 0, ctx.dark !== false);
  if (isContinuousMode(color.mode)) return gradientColor(color.mode, continuousFraction(value, ctx.min, ctx.max), ctx.dark !== false);
  return thresholdColor(config && config.thresholds, value, ctx.min, ctx.max);
}

/**
 * The CSS colour of a value by the colour mode of a field configuration (9.16), or null.
 * `ctx` has min, max and seriesIndex (the index among number fields, for the palette).
 */
export function fieldColor(config, value, ctx = {}) {
  const c = fieldColorValue(config, value, ctx);
  return c === null ? null : resolveColor(c);
}

/**
 * The CSS colour of a colour value for a mark that sits on the surface of the panel: a line, a point, a swatch, a value text.
 * A named token gives `var(--viz-<token>)`. The light theme darkens these tokens until they reach 3:1 on the panel
 * (css/tokens.css), and the dark theme keeps the Grafana hex. Another value (a hex, an rgb text, `text`) resolves as `resolveColor` does.
 * Use `resolveColor` or `fieldColor` for a fill that carries its own text (a stat tile, a table cell, a bar gauge fill).
 */
export function markCss(c) {
  if (typeof c === "string" && Object.hasOwn(NAMED, c)) return `var(--viz-${c})`;
  return resolveColor(c);
}

/** `fieldColor` for a mark (see `markCss`), or null. */
export function markColor(config, value, ctx = {}) {
  const c = fieldColorValue(config, value, ctx);
  return c === null ? null : markCss(c);
}

/**
 * The CSS colour of a colour value for a fill that carries its own text (a stat tile, a table cell, a bar gauge fill, a pie
 * slice). The dark theme gives `resolveColor`. The light theme gives the light shade of Grafana for a named token
 * (`NAMED_LIGHT`); a hex, an rgb text, `text` and `transparent` stay as they are. The stored document value does not change.
 */
export function resolveFill(c, dark = true) {
  if (!dark && typeof c === "string" && Object.hasOwn(NAMED_LIGHT, c)) return NAMED_LIGHT[c];
  return resolveColor(c);
}

/**
 * The fill colour of a display result of display.js (`color`, `mark`) in a theme. `mark` is `var(--viz-<token>)` when the colour
 * of the document is a named token: the light theme then gives the light shade. Another colour (a hex, an rgb text) stays.
 */
export function fillOf(shown, dark = true) {
  if (!shown) return null;
  // A colour that depends on the theme beyond the shade (`palette`, a continuous mode) carries its light colour in `light` (display.js).
  if (!dark && typeof shown.light === "string") return shown.light;
  if (dark || shown.color === null || shown.color === undefined) return shown.color;
  const m = /^var\(--viz-([a-z-]+)\)$/.exec(shown.mark || "");
  return m && Object.hasOwn(NAMED_LIGHT, m[1]) ? NAMED_LIGHT[m[1]] : shown.color;
}
