// The controls of the properties of a property set (10.3a): one control for each kind of property, for an override `set` and for the groups
// "Axis" and "Series style" of `fields.defaults`. A control writes through `setValue(value)` and only for a valid value.

import { isUnitValid } from "../validators.js";
import { el, toggle, uniqueId } from "../../ui.js";
import { CALCS, OPTION_LABELS, checkProperty, propertyById } from "../props.js";
import { colorModeEditor, thresholdsEditor } from "./colors_form.js";
import { numberField, selectField, switchField, textField } from "./controls.js";
import { linksForm } from "./links.js";
import { mappingsForm } from "./mappings.js";
import { checkChars } from "./rules.js";
import { parseDash } from "./options/timeseries.js";

// The units that the page knows (9.15). The text field takes any valid unit, so the list is a help.
export const UNIT_HINTS = Object.freeze([
  "none", "short", "percent", "percentunit", "ms", "s", "bytes", "decbytes", "ns", "µs", "m", "h", "d", "dtdurationms", "dtdurations", "bits", "bps", "Bps", "binBps",
  "reqps", "ops", "rps", "iops", "locale", "dateTimeAsIso", "dateTimeFromNow", "bool_yes_no", "currencyUSD", "currencyEUR",
  "string", "bool", "bool_on_off", "kbytes", "mbytes", "gbytes", "tbytes", "deckbytes", "decmbytes", "decgbytes", "dectbytes", "decbits",
  "KBs", "MBs", "GBs", "Kbits", "Mbits", "Gbits", "rowsps", "rowspm", "recps", "cps", "wps", "opm", "reqpm", "rpm", "dthms", "dtdhms", "hertz", "sishort",
  "suffix:", "prefix:",
]);

const optionsOf = (values) => values.map((v) => [v, OPTION_LABELS[v] || v]);

const CELL_MODES = { color_background: ["gradient", "basic"], gauge: ["basic", "gradient", "lcd"] };
const CELL_TYPES = [["auto", "Auto"], ["color_text", "Coloured text"], ["color_background", "Coloured background"], ["gauge", "Gauge"], ["json", "JSON"]];

/**
 * The control of one property of a set.
 *  prop: the entry of the catalog.  value: the value now.  getSet(): the set now (the rules between two members).
 *  setValue(value): writes the member.  ctx: { scope, id (the hook), hints }.
 * It returns { node, control }. `control` is the element that the label of the row names. A group has no `control`.
 */
export function propertyControl(prop, value, { scope, id, getSet, setValue, linksScope = id }) {
  const ariaLabel = prop.label;
  switch (prop.kind) {
    case "unit": {
      const listId = uniqueId("unit-list");
      const f = textField(scope, { ariaLabel, value, list: listId, placeholder: "none", check: (v) => (isUnitValid(v) ? "" : checkProperty("unit", v)), apply: setValue, testid: id });
      return { node: el("div", { class: "field" }, f.node, el("datalist", { id: listId }, ...UNIT_HINTS.map((u) => el("option", { value: u })))), control: f.control };
    }
    case "integer": {
      const f = numberField(scope, { ariaLabel, value, integer: [prop.min, prop.max], apply: setValue, testid: id, hint: `${prop.min} to ${prop.max}` });
      return { node: f.node, control: f.control };
    }
    case "number": {
      const pair = { min: ["max", (n, o) => (n < o ? "" : "Min must be less than max.")], max: ["min", (n, o) => (o < n ? "" : "Min must be less than max.")], soft_min: ["soft_max", (n, o) => (n < o ? "" : "Soft min must be less than soft max.")], soft_max: ["soft_min", (n, o) => (o < n ? "" : "Soft min must be less than soft max.")] }[prop.id];
      const f = numberField(scope, {
        ariaLabel,
        value,
        testid: id,
        extra(n) {
          const other = getSet()[pair[0]];
          return typeof other === "number" ? pair[1](n, other) : "";
        },
        apply: setValue,
      });
      return { node: f.node, control: f.control };
    }
    case "text": {
      const f = textField(scope, { ariaLabel, value, check: (v) => checkChars(v, prop.min, prop.max, "The text"), apply: setValue, testid: id });
      return { node: f.node, control: f.control };
    }
    case "select": {
      const f = selectField({ ariaLabel, options: optionsOf(prop.values), value, apply: setValue, testid: id });
      return { node: f.node, control: f.control };
    }
    case "switch": {
      const f = switchField({ label: "On", checked: value, apply: setValue, testid: id });
      return { node: f.node, control: f.input };
    }
    case "dash": {
      // `line_dash_array` (4.18, P14): "10, 20" as the text of the array.
      const f = textField(scope, { ariaLabel, value: Array.isArray(value) ? value.join(", ") : "", check: (t) => (t.trim() === "" ? "Use 1 to 8 whole numbers." : parseDash(t).error || ""), apply: (t) => setValue(parseDash(t).value), testid: id });
      return { node: f.node, control: f.control };
    }
    case "color":
      return { node: colorModeEditor({ value, onCommit: setValue, testid: `${id}-mode` }) };
    case "thresholds":
      return { node: thresholdsEditor(scope, { value, onCommit: setValue }) };
    case "mappings":
      return { node: mappingsForm(scope, { listTestid: `${id}-mappings`, prefix: `${id}-mapping`, minItems: 1, get: () => getSet().mappings || [], set: (arr) => setValue(arr) }) };
    case "links":
      return { node: linksForm(scope, { scopeName: linksScope, minItems: 1, get: () => getSet().links || [], set: (arr) => setValue(arr) }) };
    case "cell": {
      const host = el("div", { class: "field-grid" });
      const draw = (cell) => {
        host.replaceChildren(selectField({ ariaLabel: "Cell type", options: CELL_TYPES, value: cell.type, testid: `${id}-type`, apply: (type) => {
          const next = { type, mode: CELL_MODES[type] ? CELL_MODES[type][0] : null };
          setValue(next);
          draw(next);
        } }).node);
        if (CELL_MODES[cell.type]) {
          host.append(selectField({ ariaLabel: "Cell mode", options: CELL_MODES[cell.type].map((m) => [m, m === "lcd" ? "LCD" : m[0].toUpperCase() + m.slice(1)]), value: cell.mode, testid: `${id}-mode`, apply: (mode) => {
            const next = { type: cell.type, mode };
            setValue(next);
            draw(next);
          } }).node);
        }
      };
      draw(value);
      return { node: host };
    }
    case "footer": {
      const err = el("p", { class: "field-error", role: "alert", hidden: true });
      let current = [...value.reducers];
      const boxes = el("div", { class: "checklist", role: "group", "aria-label": "Footer calculations", dataset: { testid: `${id}-reducers` } });
      const paint = () => {
        for (const c of boxes.querySelectorAll("input")) c.disabled = current.length >= 8 && !current.includes(c.dataset.calc);
      };
      for (const calc of CALCS) {
        const t = toggle({ label: calc, kind: "check", checked: current.includes(calc), onChange: (on) => {
          current = on ? [...current, calc] : current.filter((c) => c !== calc);
          const bad = current.length < 1 ? "Choose 1 to 8 calculations." : "";
          err.textContent = bad;
          err.hidden = !bad;
          paint();
          if (!bad) setValue({ reducers: [...current] });
        } });
        t.input.dataset.calc = calc;
        boxes.append(t.el);
      }
      paint();
      return { node: el("div", { class: "field" }, boxes, err) };
    }
    case "hide_from": {
      const make = (key, label) => switchField({ label, checked: value[key], testid: `${id}-${key}`, apply: (on) => {
        value = { ...value, [key]: on };
        setValue({ legend: value.legend, tooltip: value.tooltip, viz: value.viz });
      } }).node;
      return { node: el("div", { class: "field" }, make("legend", "Legend"), make("tooltip", "Tooltip"), make("viz", "Graph")) };
    }
    default:
      return { node: el("p", { class: "muted", text: `The property ${prop.id} is edited in the JSON tab.` }) };
  }
}

// --- the old controls of `fields.defaults` that the option forms place (10.3h, F43) ------------------------------------------------

/**
 * One control of a member of `fields.defaults` that the groups "Axis", "Graph styles" and "Cell options" show. An empty field or the item "Not set"
 * removes the member. `id` is a member of 4.18: axis_placement, axis_label, soft_min, soft_max, line_dash, transform or cell.
 *  defaults(): the object now.  setDefaults(patch): `undefined` removes a member.  hook: the `data-testid` of the control.
 * It returns { node, control } or null for an id that is not in this list.
 */
export function defaultsControl(scope, id, { label, defaults, setDefaults, hook }) {
  const d = defaults();
  const choice = () => {
    const f = selectField({ label, options: [["", "Not set"], ...optionsOf(propertyById(id).values)], value: d[id] === undefined ? "" : d[id], testid: hook, apply: (v) => setDefaults({ [id]: v === "" ? undefined : v }) });
    return { node: f.node, control: f.control, set: (v) => { f.control.value = v === undefined ? "" : v; }, error: f.error };
  };
  switch (id) {
    case "axis_placement":
    case "line_dash":
    case "transform":
      return choice();
    case "axis_label": {
      const f = textField(scope, {
        label,
        value: d.axis_label || "",
        check: (v) => (v === "" ? "" : checkChars(v, 1, 256, "The label")),
        apply: (v) => setDefaults({ axis_label: v === "" ? undefined : v }),
        testid: hook,
        hint: "Empty: no title.",
      });
      return { node: f.node, control: f.control, set: (v) => f.set(v || ""), error: f.error };
    }
    case "soft_min":
    case "soft_max": {
      const otherId = id === "soft_min" ? "soft_max" : "soft_min";
      const ok = id === "soft_min" ? (n, o) => n < o : (n, o) => o < n;
      const f = numberField(scope, {
        label,
        value: d[id],
        optional: true,
        placeholder: "none",
        testid: hook,
        extra(n) {
          const other = defaults()[otherId];
          return typeof other === "number" && !ok(n, other) ? "Soft min must be less than soft max." : "";
        },
        apply: (n) => setDefaults({ [id]: n === null ? undefined : n }),
      });
      return { node: f.node, control: f.control, set: (v) => f.set(v === undefined || v === null ? "" : String(v)), error: f.error };
    }
    case "cell": {
      const value = d.cell || { type: "auto", mode: null };
      const set = (cell) => setDefaults({ cell });
      const got = propertyControl({ id: "cell", kind: "cell", label }, value, { scope, id: hook.replace(/-type$/, ""), getSet: defaults, setValue: set });
      return { node: got.node, control: got.node.querySelector("select"), set() {}, error() {} };
    }
    default:
      return null;
  }
}
