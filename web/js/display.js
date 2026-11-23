// The display of a cell value: mappings, no value text, unit, decimals and colour (9.14 to 9.17, 9.20).
// Pure module: no DOM access. The field configuration is resolved by fields.js.
import { fieldColorValue, isContinuousMode, isThemedMode, markCss, resolveColor, resolveFill } from "./colors.js";
import { grafanaValue } from "./frame.js";
import { applyMappings } from "./mappings.js";
import { formatNumber, formatTime, formatTimeMs, joinParts } from "./units.js";

// A time field with no unit shows the milliseconds when the mean step of its values is below 20 s (measured on Grafana 12.4.2, 9.15):
// `|last - first| / (rows - 1)`, with the first and the last value that are not null. One row has no step, so it shows none.
export const TIME_MS_STEP = 20000;

export function timeFieldHasMs(field) {
  const n = field.values.length;
  let first = 0;
  while (first < n && field.nulls[first] === 1) first++;
  let last = n - 1;
  while (last > first && field.nulls[last] === 1) last--;
  if (!(last > first)) return false;
  return Math.abs((field.values[last] - field.values[first]) / (n - 1)) < TIME_MS_STEP;
}

function formatPlain(value, cfg, kind, exactText, zone, withMs) {
  if (typeof value === "boolean") return cfg.unit === "bool_yes_no" ? (value ? "Yes" : "No") : String(value);
  if (typeof value === "string") return value;
  if (typeof value === "number") {
    if (kind === "time" && (cfg.unit === "none" || !cfg.unit)) return withMs ? formatTimeMs(value, zone) : formatTime(value, zone);
    return joinParts(formatNumber(value, cfg.unit, cfg.decimals, exactText, zone));
  }
  return "";
}

/**
 * Make the display function of a field. `display(i)` gives { text, color, numeric, raw, empty, mapped } for row i.
 * `display.valueDisplay(value, exactText)` does the same for a reduced value.
 * `range` is `{ min, max }`, the scale of a continuous mode when the
 * page knows the data of all the fields (9.16): an end that the field sets itself wins.
 */
export function makeDisplay(field, { zone = "browser", seriesIndex = 0, range } = {}) {
  const cfg = field.config || {};
  const set = cfg.set || {};
  const bounds = {
    min: Number.isFinite(set.min) || !range || !Number.isFinite(range.min) ? cfg.min : range.min,
    max: Number.isFinite(set.max) || !range || !Number.isFinite(range.max) ? cfg.max : range.max,
    seriesIndex,
  };
  const withMs = field.kind === "time" && timeFieldHasMs(field);

  function valueDisplay(value, exactText = null) {
    const mapping = applyMappings(cfg.mappings, value);
    const isEmpty = value === null || value === undefined || value === "";
    let text;
    let mapped = false;
    if (mapping && mapping.text !== null) {
      text = mapping.text;
      mapped = true;
    } else if (isEmpty) {
      text = cfg.no_value !== null && cfg.no_value !== undefined ? cfg.no_value : "";
    } else {
      text = formatPlain(value, cfg, field.kind, exactText, zone, withMs);
    }
    // `colorValue` is the colour as in the document. `color` is its CSS (a hex, for a fill). `mark` is its CSS for a text or a line
    // on the panel surface (colors.js markCss: a token follows the theme).
    let colorValue = null;
    let lightValue = null;
    const mode = cfg.color ? cfg.color.mode : "thresholds";
    if (mapping && mapping.color !== null) colorValue = mapping.color;
    else if (!isEmpty && !mapped) {
      const numeric = typeof value === "number" && !Number.isNaN(value);
      if (numeric || mode === "fixed" || mode === "palette" || isContinuousMode(mode)) colorValue = fieldColorValue(cfg, numeric ? value : NaN, bounds);
    } else if (isEmpty && !mapped && isContinuousMode(mode)) {
      // A value that is null has the colour of the first stop of a continuous mode (9.16).
      colorValue = fieldColorValue(cfg, NaN, bounds);
    }
    // A colour that the theme changes beyond the shade (`palette`, a continuous mode) also has its colour in the light theme.
    if (colorValue !== null && isThemedMode(mode) && !(mapping && mapping.color !== null)) {
      const light = fieldColorValue(cfg, typeof value === "number" && !Number.isNaN(value) ? value : NaN, { ...bounds, dark: false });
      lightValue = light === null ? null : resolveFill(light, false);
    }
    const color = colorValue === null ? null : resolveColor(colorValue);
    return {
      text,
      color,
      light: lightValue,
      mark: colorValue === null ? null : markCss(colorValue),
      numeric: typeof value === "number" && !Number.isNaN(value) ? value : null,
      raw: value,
      empty: isEmpty && !(mapping && mapping.text !== null),
      mapped,
    };
  }

  function display(i) {
    const value = grafanaValue(field, i);
    return valueDisplay(value, field.kind === "number_text" && value !== null && typeof value === "number" ? field.values[i] : null);
  }
  display.valueDisplay = valueDisplay;
  return display;
}
