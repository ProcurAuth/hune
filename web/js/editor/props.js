// The catalog of the properties of a property set (4.18, 10.3a, 10.4). Pure module: no DOM access.
// The order of the list is the canonical order of the format. A test reads tests/fixtures/dashboards/f2_members.json and
// checks that each member of a property set is in this list.

import { isColorValid, isUnitValid } from "./validators.js";

export const CALCS = Object.freeze([
  "last", "last_not_null", "first", "first_not_null", "min", "max", "mean", "total", "count", "median",
  "std_dev", "variance", "range", "diff", "diff_percent", "delta", "distinct_count", "change_count",
]);

export const COLOR_MODES = Object.freeze([
  "fixed", "thresholds", "palette",
  "continuous_gr_yl_rd", "continuous_rd_yl_gr", "continuous_bl_yl_rd", "continuous_yl_rd", "continuous_bl_pu", "continuous_yl_bl",
  "continuous_blues", "continuous_reds", "continuous_greens", "continuous_purples",
]);

export const COLOR_MODE_LABELS = Object.freeze({
  fixed: "Single colour",
  thresholds: "From thresholds",
  palette: "Series palette",
  continuous_gr_yl_rd: "Green-Yellow-Red",
  continuous_rd_yl_gr: "Red-Yellow-Green",
  continuous_bl_yl_rd: "Blue-Yellow-Red",
  continuous_yl_rd: "Yellow-Red",
  continuous_bl_pu: "Blue-Purple",
  continuous_yl_bl: "Yellow-Blue",
  continuous_blues: "Blues",
  continuous_reds: "Reds",
  continuous_greens: "Greens",
  continuous_purples: "Purples",
});

const TABLE = ["table"];
const SERIES = ["timeseries"];
const AXIS = ["timeseries", "barchart"];

// id: the member. kind: the control. where: "both" (defaults and set), "defaults" or "set". types: the panel types, or null for all.
const row = (id, label, kind, rest = {}) => ({ id, label, kind, where: "both", types: null, ...rest });

export const PROPERTIES = Object.freeze([
  row("unit", "Unit", "unit"),
  row("decimals", "Decimals", "integer", { min: 0, max: 20 }),
  row("min", "Min", "number"),
  row("max", "Max", "number"),
  row("color", "Colour", "color"),
  row("thresholds", "Thresholds", "thresholds"),
  row("mappings", "Value mappings", "mappings"),
  row("links", "Data links", "links"),
  row("display_name", "Display name", "text", { min: 1, max: 256 }),
  row("no_value", "No value", "text", { min: 1, max: 256, where: "defaults" }),
  row("width", "Column width", "integer", { min: 10, max: 4000, types: TABLE }),
  row("align", "Cell alignment", "select", { values: ["auto", "left", "center", "right"], types: TABLE }),
  row("cell", "Cell type", "cell", { types: TABLE }),
  row("filterable", "Column filter", "switch", { types: TABLE }),
  row("hidden", "Hide column", "switch", { types: TABLE }),
  row("footer", "Footer calculations", "footer", { types: TABLE }),
  row("inspect", "Cell value inspect", "switch", { types: TABLE }),
  row("hide_from", "Hide in area", "hide_from"),
  row("axis_placement", "Axis placement", "select", { values: ["auto", "left", "right", "hidden"], types: SERIES }),
  row("axis_label", "Axis label", "text", { min: 1, max: 256, types: AXIS }),
  row("soft_min", "Axis soft min", "number", { types: AXIS }),
  row("soft_max", "Axis soft max", "number", { types: AXIS }),
  row("axis_grid", "Show grid lines", "switch", { types: ["barchart"] }),
  row("draw", "Style", "select", { values: ["line", "bars", "points"], types: SERIES, where: "set" }),
  row("line_width", "Line width", "integer", { min: 0, max: 10, types: SERIES, where: "set" }),
  row("fill_opacity", "Fill opacity", "integer", { min: 0, max: 100, types: SERIES, where: "set" }),
  row("line_dash", "Line style", "select", { values: ["solid", "dash", "dot"], types: SERIES }),
  row("show_points", "Show points", "select", { values: ["auto", "always", "never"], types: SERIES, where: "set" }),
  row("point_size", "Point size", "integer", { min: 1, max: 40, types: SERIES, where: "set" }),
  row("transform", "Transform", "select", { values: ["none", "negative_y"], types: SERIES }),
  // Wave H, item P14: the dash array and the look of the y axis.
  row("line_dash_array", "Dash array", "dash", { types: SERIES }),
  row("axis_centered_zero", "Centered zero", "switch", { types: AXIS }),
  row("axis_width", "Axis width", "integer", { min: 0, max: 500, types: AXIS }),
  row("axis_border", "Axis border", "switch", { types: AXIS }),
  row("axis_color", "Axis color", "select", { values: ["text", "series"], types: AXIS }),
]);

export const PROPERTY_IDS = Object.freeze(PROPERTIES.map((p) => p.id));

export const OPTION_LABELS = Object.freeze({
  auto: "Auto", left: "Left", right: "Right", hidden: "Hidden", center: "Center",
  line: "Lines", bars: "Bars", points: "Points", solid: "Solid", dash: "Dash", dot: "Dots",
  always: "Always", never: "Never", none: "None", negative_y: "Negative Y", text: "Text", series: "Series",
});

const byId = new Map(PROPERTIES.map((p) => [p.id, p]));

export const propertyById = (id) => byId.get(id) || null;

/** True when the member `id` is valid in a property set at `where` ("defaults" or "set") of a panel of the type `type`. */
export function allowedIn(id, type, where) {
  const p = byId.get(id);
  if (!p) return false;
  if (p.where !== "both" && p.where !== where) return false;
  return p.types === null || p.types.includes(type);
}

/** The properties of the list "Add property" of an override: the ones that the type permits and that the set does not hold. `no_value` never shows. */
export function addableProperties(type, set) {
  return PROPERTIES.filter((p) => allowedIn(p.id, type, "set") && !Object.hasOwn(set || {}, p.id));
}

/** A copy of a property set with its members in the canonical order. A member that is not in the catalog keeps its place after the others. */
export function orderSet(set) {
  const out = {};
  for (const id of PROPERTY_IDS) if (Object.hasOwn(set, id)) out[id] = set[id];
  for (const key of Object.keys(set)) if (!Object.hasOwn(out, key)) out[key] = set[key];
  return out;
}

const THRESHOLDS = () => ({ mode: "absolute", steps: [{ value: null, color: "green" }, { value: 80, color: "red" }] });

/** The value that a new property gets when the person adds it. It is valid for the format. */
export function initialValue(id) {
  switch (id) {
    case "unit": return "short";
    case "decimals": return 2;
    case "min": return 0;
    case "max": return 100;
    case "color": return { mode: "fixed", fixed: "green" };
    case "thresholds": return THRESHOLDS();
    case "mappings": return [{ kind: "value", value: "1", text: "One", color: null }];
    case "links": return [{ title: "Link", url: "https://", target_blank: false }];
    case "display_name": return "Name";
    case "no_value": return "-";
    case "width": return 120;
    case "align": return "auto";
    case "cell": return { type: "auto", mode: null };
    case "filterable": return true;
    case "hidden": return true;
    case "footer": return { reducers: ["last_not_null"] };
    case "inspect": return true;
    case "hide_from": return { legend: false, tooltip: false, viz: true };
    case "axis_placement": return "right";
    case "axis_label": return "Label";
    case "soft_min": return 0;
    case "soft_max": return 100;
    case "axis_grid": return false;
    case "draw": return "bars";
    case "line_width": return 2;
    case "fill_opacity": return 20;
    case "line_dash": return "dash";
    case "show_points": return "always";
    case "point_size": return 5;
    case "transform": return "negative_y";
    case "line_dash_array": return [10, 20];
    case "axis_centered_zero": return true;
    case "axis_width": return 60;
    case "axis_border": return true;
    case "axis_color": return "series";
    default: return undefined;
  }
}

const isInt = (v) => Number.isInteger(v);

/** The error of one member of a property set, or "" when the member is valid for its own rule. The rules between two members are in `setErrors`. */
export function checkProperty(id, value) {
  const p = byId.get(id);
  if (!p) return `The property ${id} is not known.`;
  switch (p.kind) {
    case "unit": return isUnitValid(value) ? "" : "This unit is not valid. Use a unit of the list, suffix:<text> or prefix:<text>.";
    case "integer": return isInt(value) && value >= p.min && value <= p.max ? "" : `Use a whole number from ${p.min} to ${p.max}.`;
    case "number": return typeof value === "number" && Number.isFinite(value) ? "" : "Use a number.";
    case "text": {
      const n = typeof value === "string" ? [...value].length : 0;
      return n >= p.min && n <= p.max ? "" : `Use ${p.min} to ${p.max} characters.`;
    }
    case "select": return p.values.includes(value) ? "" : `Use one of ${p.values.join(", ")}.`;
    case "switch": return typeof value === "boolean" ? "" : "Use true or false.";
    case "dash": return Array.isArray(value) && value.length >= 1 && value.length <= 8 && value.every((n) => isInt(n) && n >= 0 && n <= 100) && value.some((n) => n > 0) ? "" : "Use 1 to 8 whole numbers from 0 to 100, one above 0.";
    case "color": return value && COLOR_MODES.includes(value.mode) && (value.mode === "fixed" ? isColorValid(value.fixed) : value.fixed === null) ? "" : "The colour is not valid.";
    default: return "";
  }
}

/** The errors between two members of one set: `min` below `max`, `soft_min` below `soft_max` (4.18, rules 2 and 7). Returns `{ max, soft_max }` with a text or "". */
export function setErrors(set) {
  const out = { max: "", soft_max: "" };
  if (typeof set.min === "number" && typeof set.max === "number" && !(set.min < set.max)) out.max = "Min must be less than max.";
  if (typeof set.soft_min === "number" && typeof set.soft_max === "number" && !(set.soft_min < set.soft_max)) out.soft_max = "Soft min must be less than soft max.";
  return out;
}
