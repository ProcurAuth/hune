// The items of a stat, a bar gauge or a pie chart: one for each field (reduced) or one for each row (9.19).
// No DOM access.

import { parseCss, relativeLuminance, readableTextOn } from "../colors.js";
import { makeDisplay } from "../display.js";
import { sharedRange } from "./series.js";
import { grafanaValue } from "../frame.js";
import { reduceField, selectReduceFields } from "../reduce.js";
import { formatNumber, joinParts } from "../units.js";

// The parts {prefix, text, suffix} of the display text of a value, so that a panel can draw the unit smaller.
// It gives null when the text is a mapping, the no value text, a time or a plain text.
export function valueParts(field, shown, exact = null) {
  const v = shown.raw;
  if (shown.mapped || shown.empty || typeof v !== "number" || field.kind === "time") return null;
  const cfg = field.config;
  const parts = formatNumber(v, cfg.unit, cfg.decimals, exact);
  return joinParts(parts) === shown.text && (parts.prefix || parts.suffix) ? parts : null;
}

// The unit that a calculation forces (9.19): a count has no unit, `diff_percent` has the unit `percent`. Else null: the unit of the field.
export function calcUnit(calc) {
  if (calc === "count" || calc === "distinct_count" || calc === "change_count") return "none";
  if (calc === "diff_percent") return "percent";
  return null;
}

// With `reduce.values`, the rows stop at `max` items: a gauge reads only the rows that it shows (9.19g).
export function reduceItems({ fields, names, baseNames, zone }, reduce, max = Infinity) {
  const indices = selectReduceFields(fields, baseNames, reduce.fields);
  const items = [];
  const firstText = fields.findIndex((f) => f.kind === "text");
  // Grafana adds the name of the field to the name of a row when the data has several numeric fields (measured: "web-1 avg_cpu"
  // with three numeric fields, "web-1" with one).
  const manyNumbers = fields.filter((f) => f.kind === "number" || f.kind === "number_text").length > 1;
  const forced = reduce.values ? null : calcUnit(reduce.calc);
  // A continuous colour mode uses the range of all number fields together (9.16).
  const range = sharedRange(fields);
  indices.forEach((fi, k) => {
    const field = fields[fi];
    const display = makeDisplay(forced ? { ...field, config: { ...field.config, unit: forced } } : field, { zone, seriesIndex: k, range });
    if (!reduce.values) {
      const r = reduceField(field, reduce.calc);
      const exact = field.kind === "number_text" && typeof r.value === "string" ? r.value : null;
      const shown = display.valueDisplay(exact !== null ? r.num : r.value, exact);
      items.push({ name: names[fi], fieldIndex: fi, field, value: shown.numeric, text: shown.text, parts: valueParts(forced ? { ...field, config: { ...field.config, unit: forced } } : field, shown, exact), color: shown.color, display: shown, min: field.config.min, max: field.config.max });
      return;
    }
    const count = field.values.length;
    for (let row = 0; row < count && items.length < max; row++) {
      const label = firstText >= 0 && firstText !== fi ? grafanaValue(fields[firstText], row) : null;
      const hasLabel = label !== null && label !== undefined && label !== "";
      const name = hasLabel ? (manyNumbers ? `${label} ${names[fi]}` : String(label)) : names[fi];
      const shown = display(row);
      items.push({ name, fieldIndex: fi, field, value: shown.numeric, text: shown.text, parts: valueParts(field, shown, field.kind === "number_text" && typeof grafanaValue(field, row) === "number" ? field.values[row] : null), color: shown.color, display: shown, min: field.config.min, max: field.config.max, row });
    }
  });
  return items;
}

// ---- colour legibility (pure) ----

function contrast(a, b) {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  if (la === null || lb === null) return null;
  const [hi, lo] = la >= lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}

// Moves the colour `css` toward white or black (by the side of the surface) until the contrast with `surface`
// is `min` or more. A colour that is not a plain colour (var(), transparent) is not changed.
export function legibleOn(css, surface, min = 3) {
  const c0 = parseCss(css);
  const s0 = parseCss(surface);
  if (!c0 || !s0 || c0.a < 1) return css;
  if (contrast(css, surface) >= min) return css;
  const toward = relativeLuminance(surface) < 0.4 ? 255 : 0;
  for (let step = 1; step <= 20; step++) {
    const k = step / 20;
    const mix = (x) => Math.round(x + (toward - x) * k);
    const out = `rgb(${mix(c0.r)}, ${mix(c0.g)}, ${mix(c0.b)})`;
    if (contrast(out, surface) >= min) return out;
  }
  return toward === 255 ? "#ffffff" : "#000000";
}

// The ink on a filled box: black or white by the luminance of the fill (9.20 rule), or a theme token.
export function inkOn(fill) {
  if (typeof fill !== "string" || fill.startsWith("var(")) return "var(--bg)";
  if (fill === "transparent") return "var(--text)";
  const p = parseCss(fill);
  if (p && p.a < 1) return "var(--text)";
  return readableTextOn(fill);
}

// The size of the unit of a value of the size `size` (px), as Grafana: 0.9 of it under 20 px, 0.8 under 30 px, else 0.6
// (measured: 17.7 px gives 15.9, 23.9 gives 19.1, 31.9 gives 19.1, 40.1 gives 24.0).
export function unitSizeOf(size) {
  return size * (size < 20 ? 0.9 : size < 30 ? 0.8 : 0.6);
}

// The font size (px) for a text that has the width `w100` at 100 px, in a box `boxW` x `boxH`.
// The text uses at most `fillW` of the width and `fillH` of the height (line height `lineHeight`). There is no upper limit
// by default: the value of a stat panel in a view panel is very large, as in Grafana (wave G).
export function fitFontSize(w100, boxW, boxH, { fillW = 0.92, fillH = 0.85, min = 11, max = Infinity, lineHeight = 1.1 } = {}) {
  if (!(boxW > 0) || !(boxH > 0) || !(w100 > 0)) return min;
  const byWidth = (boxW * fillW * 100) / w100;
  const byHeight = (boxH * fillH) / lineHeight;
  return Math.max(min, Math.min(max, byWidth, byHeight));
}

// ---- stat fill (background colour mode, wave G) ----

export const INK_LIGHT = "#f7f8fa"; // the text on a dark fill (measured in Grafana)
export const INK_DARK = "#202226"; // the text on a light fill
const LIGHT_SCALE = 0.7; // light theme: the lightness is raised by 0.7 of the dark theme amounts (0.105 and 0.035)
const AA_LARGE = 3; // the least contrast of a large text (WCAG 1.4.3)

function rgbToHsl({ r, g, b }) {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return { h: 0, s: 0, l };
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h;
  if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return { h: h * 60, s, l };
}

function hslToRgb({ h, s, l }) {
  const hh = (((h % 360) + 360) % 360) / 360;
  if (s === 0) {
    const v = Math.round(l * 255);
    return { r: v, g: v, b: v };
  }
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const chan = (t) => {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  return { r: Math.round(chan(hh + 1 / 3) * 255), g: Math.round(chan(hh) * 255), b: Math.round(chan(hh - 1 / 3) * 255) };
}

const rgbText = ({ r, g, b }) => `rgb(${r}, ${g}, ${b})`;

/**
 * The fill and the ink of a stat panel in the colour mode `background` (pure).
 * Grafana 12.4.2 draws `linear-gradient(120deg, A, B)`. In a dark theme A is the colour with the HSL lightness 15 points lower and the hue
 * turned by +8 degrees, B is 5 points lower and -8 degrees. In a light theme the lightness is raised instead, by 0.105 and 0.035.
 * The ink is dark when the colour has a brightness above 180 ((299 R + 587 G + 114 B) / 1000), else light. Grafana puts its light ink on
 * mid-light greens at about 2:1. Here the light ink stays only where its contrast with the lighter end of the fill is 3:1 or more
 * (large text, 9.26), else the ink with the better contrast is used. Gives null for a colour that is not a plain colour (token, transparent).
 */
export function statFill(color, dark = true) {
  const p = parseCss(color);
  if (!p || p.a < 1) return null;
  const hsl = rgbToHsl(p);
  const shade = (dl, spin) => hslToRgb({ h: hsl.h + spin, s: hsl.s, l: Math.min(1, Math.max(0, hsl.l + (dark ? -dl : dl * LIGHT_SCALE))) });
  const a = shade(0.15, 8);
  const b = shade(0.05, -8);
  const brightness = (299 * p.r + 587 * p.g + 114 * p.b) / 1000;
  const lightest = [a, b].map(rgbText).sort((x, y) => relativeLuminance(y) - relativeLuminance(x))[0];
  const darkest = [a, b].map(rgbText).sort((x, y) => relativeLuminance(x) - relativeLuminance(y))[0];
  // The least contrast of each ink over the whole fill. Grafana's choice stays when it passes; else the better ink wins.
  const onLight = contrast(INK_LIGHT, lightest);
  const onDark = contrast(INK_DARK, darkest);
  let light = brightness <= 180;
  if (light ? onLight < AA_LARGE : onDark < AA_LARGE) light = onLight >= onDark;
  return { gradient: `linear-gradient(120deg, ${rgbText(a)}, ${rgbText(b)})`, from: rgbText(a), to: rgbText(b), ink: light ? INK_LIGHT : INK_DARK };
}

const AA_TEXT = 4.5; // the least contrast of a normal text (WCAG 1.4.3)

/**
 * The ink of a table cell or another small text on a fill (pure). `color` is the plain colour of the cell, `ends` the colours that
 * the background really shows (the two ends of a gradient; the colour itself when it is flat). Grafana puts a dark ink on a colour
 * with a brightness above 180 ((299 R + 587 G + 114 B) / 1000), else a light ink. That choice stays when its contrast with the
 * background is 4.5:1 or more over the whole fill (9.26); else the ink with the better contrast wins. Gives a theme token for a
 * colour that is not a plain colour (`var(--text)`: the page colour; alpha: the text colour).
 */
export function cellInk(color, ends = [color]) {
  const p = parseCss(color);
  if (!p || p.a < 1 || ends.some((e) => !parseCss(e) || parseCss(e).a < 1)) return inkOn(color);
  const lums = ends.map((e) => relativeLuminance(e));
  const ratio = (a, b) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  // The least contrast of an ink over the whole fill: the worst place is the end with the nearest luminance.
  const worst = (ink) => Math.min(...lums.map((l) => ratio(relativeLuminance(ink), l)));
  const light = (299 * p.r + 587 * p.g + 114 * p.b) / 1000 <= 180;
  const order = light ? [INK_LIGHT, INK_DARK, "#ffffff", "#000000"] : [INK_DARK, INK_LIGHT, "#000000", "#ffffff"];
  const pass = order.find((ink) => worst(ink) >= AA_TEXT);
  if (pass) return pass;
  return order.reduce((best, ink) => (worst(ink) > worst(best) ? ink : best));
}

// The position (0 to 100, in percent of the scale) of a threshold step value.
export function stepPercent(t, value, min, max) {
  if (value === null || value === undefined) return 0;
  const v = t.mode === "percentage" ? min + ((max - min) * value) / 100 : value;
  return Math.max(0, Math.min(100, ((v - min) / (max - min)) * 100));
}
