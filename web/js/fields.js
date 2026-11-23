// Field configuration and matchers (9.14, FE-05). Pure module: no DOM access.
import { baseDisplayNames } from "./names.js";

/**
 * Compile a pattern in the Grafana form of 4.24 (`stringToJsRegex`).
 * `/body/flags` adds no anchor. Another text `t` gives `^t$`. Returns null when the pattern cannot compile.
 * The flag `u` is never added and never accepted.
 */
export function parseGrafanaPattern(text) {
  if (typeof text !== "string") return null;
  try {
    if (!text.startsWith("/")) return new RegExp(`^${text}$`);
    const last = text.lastIndexOf("/");
    if (last === 0) return null;
    const body = text.slice(1, last);
    const flags = text.slice(last + 1);
    if (!/^[gims]{0,4}$/.test(flags) || new Set(flags).size !== flags.length) return null;
    return new RegExp(body, flags);
  } catch {
    return null;
  }
}

/** The longest text that a pattern of the page reads: 1,024 code units, the value of `MAX_VALUE_BYTES` (4.35). */
export const MAX_PATTERN_TEXT = 1024;

/**
 * Run a regular expression on a text, without a state left in `lastIndex`. A text longer than MAX_PATTERN_TEXT never matches: a name
 * or a cell can hold data, and the time of `(.*)-(.*)` grows with the square of the length (9.14, T87).
 */
export function regexExec(re, text) {
  if (text.length > MAX_PATTERN_TEXT) return null;
  re.lastIndex = 0;
  const r = re.exec(text);
  re.lastIndex = 0;
  return r;
}

/** True when `re` matches `text` (regexExec). */
export const regexTest = (re, text) => regexExec(re, text) !== null;

/** True when the matcher `match` ({by, value}) matches the field. `baseName` is its base display name. */
export function matchesField(match, field, baseName) {
  if (!match) return false;
  // A matcher `name` also finds the name that `organize` or `rename_by_regex` set (9.14).
  if (match.by === "name") return match.value === field.name || match.value === baseName || match.value === field.config?.display_name;
  if (match.by === "regex") {
    const re = parseGrafanaPattern(match.value);
    return re !== null && regexTest(re, baseName);
  }
  // `byFrameRefID` (9.14): the field comes from the query with this ref. The joined time field has no ref, so it never matches.
  if (match.by === "query") return field.ref !== undefined && field.ref === match.value;
  if (match.by === "names") {
    // `byNames`: include = the name or the base display name equals one text; exclude = it equals none (9.14). The comparison is exact.
    if (!Array.isArray(match.names)) return false;
    const listed = match.names.includes(field.name) || match.names.includes(baseName);
    return match.mode === "exclude" ? !listed : match.mode === "include" ? listed : false;
  }
  return false;
}

const PALETTE_PANELS = new Set(["timeseries", "barchart", "piechart", "xychart", "histogram"]);

/** The lowest and the highest finite value of a number field, or nulls. */
export function dataBounds(field) {
  let src = null;
  if (field.kind === "number") src = field.values;
  else if (field.kind === "number_text") src = field.num;
  if (!src) return { min: null, max: null };
  let min = Infinity;
  let max = -Infinity;
  for (let i = 0; i < src.length; i++) {
    const v = src[i];
    if (Number.isFinite(v) && !(field.nulls && field.nulls[i])) {
      if (v < min) min = v;
      if (v > max) max = v;
    }
  }
  return min <= max ? { min, max } : { min: null, max: null };
}

function defaultThresholds() {
  return { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 80, color: "red" }] };
}

/**
 * Compute the configuration of each field (9.14): defaults, then overrides in order, then built-in defaults.
 * Returns new field objects with a resolved `config`, and the warnings.
 */
export function computeFieldConfig(fields, fieldsSpec, panelType) {
  const spec = fieldsSpec || {};
  const defaults = spec.defaults || {};
  const overrides = spec.overrides || [];
  const base = baseDisplayNames(fields);
  const warnings = [];
  const warned = new Set();
  for (const o of overrides) {
    if (o.match && o.match.by === "regex" && parseGrafanaPattern(o.match.value) === null && !warned.has(o.match.value)) {
      warned.add(o.match.value);
      warnings.push(`The override pattern ${o.match.value} is not a valid regular expression.`);
    }
  }
  const out = fields.map((f, i) => {
    const cfg = { ...defaults };
    for (const o of overrides) {
      if (!matchesField(o.match, f, base[i])) continue;
      // P14: a set with `line_dash` or `line_dash_array` replaces both, as a Grafana override replaces the whole `lineStyle` (4.18).
      if ("line_dash" in o.set || "line_dash_array" in o.set) {
        delete cfg.line_dash;
        delete cfg.line_dash_array;
      }
      Object.assign(cfg, o.set);
    }
    const bounds = dataBounds(f);
    const resolved = {
      unit: cfg.unit !== undefined ? cfg.unit : "none",
      decimals: cfg.decimals !== undefined ? cfg.decimals : null,
      min: cfg.min !== undefined ? cfg.min : bounds.min,
      max: cfg.max !== undefined ? cfg.max : bounds.max,
      color: cfg.color !== undefined ? cfg.color : { mode: PALETTE_PANELS.has(panelType) ? "palette" : panelType === "state_timeline" ? "continuous_gr_yl_rd" : "thresholds", fixed: null },
      thresholds: cfg.thresholds !== undefined ? cfg.thresholds : defaultThresholds(),
      mappings: cfg.mappings !== undefined ? cfg.mappings : [],
      links: cfg.links !== undefined ? cfg.links : [],
      // The name of a defaults or an override replaces the name that a transform set (9.13).
      display_name: cfg.display_name !== undefined ? cfg.display_name : f.config.display_name !== undefined ? f.config.display_name : null,
      no_value: defaults.no_value !== undefined ? defaults.no_value : null,
      width: cfg.width !== undefined ? cfg.width : null,
      align: cfg.align !== undefined ? cfg.align : "auto",
      cell: cfg.cell !== undefined ? cfg.cell : { type: "auto", mode: null },
      filterable: cfg.filterable !== undefined ? cfg.filterable : false,
      hidden: cfg.hidden !== undefined ? cfg.hidden : false,
      // The footer of a table column, or null (the table has no footer for it).
      footer: cfg.footer !== undefined ? cfg.footer : null,
      // Where the series does not show (9.14, 9.19). An override replaces the whole object, as it does for every property.
      hide_from: cfg.hide_from !== undefined ? { legend: cfg.hide_from.legend === true, tooltip: cfg.hide_from.tooltip === true, viz: cfg.hide_from.viz === true } : { legend: false, tooltip: false, viz: false },
      // The properties that the document sets (defaults and overrides), before the built-in defaults.
      set: cfg,
    };
    return { ...f, labels: f.labels ? { ...f.labels } : null, config: resolved };
  });
  return { fields: out, warnings };
}
