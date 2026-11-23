// The renderer of the option forms (10.3h, F43). It reads the table of rows of a type (`forms/options/<type>.js`) and builds the groups of the
// options pane. Each control writes one member with the write rule of `rows.js` and calls `apply`, which writes the draft and draws the preview.
// Markup only: no text of a document goes in with innerHTML. A row that a rule of display hides has `hidden` and keeps its member in the draft.

import { el } from "../../ui.js";
import { optionGroup } from "../form.js";
import { checksField, choiceField, colorChoice, selectField, sliderField, switchField, textField } from "./controls.js";
import { tableOf } from "./options/index.js";
import { checkChecks, checkSlider, numberText, parseText, shownIds, statusText, unknownValueError, valueOf, writeOption } from "./options/rows.js";
import { defaultsControl } from "./properties.js";
import { buildSortList } from "./sort_list.js";
import { checkPattern } from "./rules.js";

const NOT_IN_DATA = "(not in the data)";

// The reading of the text of a row with the grammar of a pattern (4.24): empty is `null`, else a valid pattern.
const patternParse = (text) => {
  if (text.trim() === "") return { value: null };
  const error = checkPattern(text);
  return error ? { error } : { value: text };
};
const reader = (row) => (row.pattern && !row.parse ? { ...row, parse: patternParse } : row);

// The words of a select of fields: the item `null`, the names of the last result, and a name of the draft that the result does not hold.
function fieldItems(names, current, nullText) {
  const known = new Set(names);
  const items = [["", nullText], ...names.map((n) => [n, n])];
  if (typeof current === "string" && current !== "" && !known.has(current)) items.push([current, `${current} ${NOT_IN_DATA}`]);
  return items;
}

/**
 * The option form of one type. `type`: the panel type. `scope`: the debounce scope of the forms. `getPanel()`: the draft panel now.
 * `apply(fn)`: writes the draft with `fn(panel) -> panel`. `hints()`: the names of the fields of the last result. `open`: the Map of the open groups.
 * `heavy()`: true when the preview has more than 5,000 points (a slider then writes on release). `counter`: the element whose `data-draft-writes`
 * counts the writes. It returns null for a type with no table, else
 * { groups: [Node], extra: { standard: [Node], thresholds: [Node] }, status: Node, refresh(), refreshFields(), table, writes(), setGroup(key, node) }.
 * `setGroup` tells the form the group node of "Standard options" and "Thresholds", so the focus can go to a group head.
 */
export function optionForm({ type, scope, getPanel, apply, hints = () => [], open, heavy = () => false, counter = null }) {
  const table = tableOf(type);
  if (!table || table.rows.length === 0) return null;
  let writes = 0;
  const countWrite = () => {
    writes += 1;
    if (counter) counter.dataset.draftWrites = String(writes);
  };
  if (counter) counter.dataset.draftWrites = "0";
  const status = el("p", { class: "visually-hidden", role: "status", dataset: { testid: "options-status" } });
  const entries = [];
  const groupNodes = new Map();
  let shown = shownIds(table, getPanel().options || {});
  // The key of a control state: the value, and its text when the row has a `format` (a duration shows as 10m and the document stores milliseconds).
  const keyOf = (panel, row) => {
    const value = valueOf(panel, row);
    return JSON.stringify([value ?? null, row.format ? row.format(value, panel.options || {}) : null]);
  };

  // The write of one row: the draft, then the rules of display and the status line.
  function write(row, value, entry) {
    const before = shown;
    countWrite();
    apply((x) => writeOption(x, row, value));
    afterWrite(before, entry);
  }

  function afterWrite(before, entry) {
    const panel = getPanel();
    if (!panel) return;
    shown = shownIds(table, panel.options || {});
    if (entry) entry.last = keyOf(panel, entry.row);
    status.textContent = statusText(table, before, shown);
    refresh(entry);
  }

  // Draws the rules of display again, and the controls that the draft changed from outside the control (a row with `also`, the tab "JSON").
  function refresh(except = null) {
    const panel = getPanel();
    if (!panel) return;
    const now = shownIds(table, panel.options || {});
    for (const entry of entries) {
      const visible = now.includes(entry.row.id);
      const wasHidden = entry.wrapper.hidden;
      if (!visible && !wasHidden && entry.wrapper.contains(document.activeElement)) {
        // The focus goes to the head of the group when its row hides (10.3h, rule 9).
        const head = groupNodes.get(entry.row.group) && groupNodes.get(entry.row.group).querySelector(".ogroup__head");
        if (head) head.focus();
      }
      entry.wrapper.hidden = !visible;
      if (visible) delete entry.wrapper.dataset.hiddenByRule;
      else entry.wrapper.dataset.hiddenByRule = "true";
      if (entry === except) continue;
      const value = valueOf(panel, entry.row);
      const text = keyOf(panel, entry.row);
      if (text !== entry.last) {
        entry.last = text;
        if (entry.ctl.set) entry.ctl.set(value);
      }
    }
    shown = now;
  }

  function refreshFields() {
    const names = hints();
    for (const entry of entries) {
      if (entry.row.kind === "field" && entry.ctl.refresh) entry.ctl.refresh(names);
      if (entry.row.kind === "fields" && entry.ctl.refresh) entry.ctl.refresh(names);
    }
  }

  // One control for a row. It returns { node, set(value), error(text), refresh(names) }.
  function build(row, entry) {
    const panel = getPanel();
    const value = valueOf(panel, row);
    const put = (v) => write(row, v, entry);
    const live = () => getPanel().options || {};
    switch (row.kind) {
      case "radio": {
        const f = choiceField({ label: row.label, items: row.items, value, apply: put, testid: row.hook });
        return f;
      }
      case "select": {
        const options = [...(row.none ? [["", row.none]] : []), ...row.items];
        const f = selectField({ label: row.label, options, value: value === undefined ? "" : value, testid: row.hook, apply: (v) => put(v === "" ? undefined : v), hint: row.hint || null });
        const check = (v) => f.error(unknownValueError(row, v));
        check(value);
        return { node: f.node, set(v) { f.control.value = v === undefined ? "" : v; check(v); }, error: f.error };
      }
      case "switch": {
        const f = switchField({ label: row.label, checked: value === true, testid: row.hook, apply: put });
        return { node: f.node, set(v) { f.input.checked = v === true; } };
      }
      case "slider": {
        const f = sliderField(scope, { label: row.label, value: value ?? row.default, min: row.min, max: row.max, testid: row.hook, heavy, apply: (n) => {
          const error = checkSlider(row, n);
          f.error(error);
          if (!error) put(n);
        } });
        return f;
      }
      case "number":
      case "text": {
        const r = reader(row);
        const show = (v, o = live()) => (row.format ? row.format(v, o) : row.kind === "number" ? numberText(v) : v === null || v === undefined ? "" : String(v));
        const f = textField(scope, {
          label: row.label,
          value: show(value),
          numeric: row.kind === "number",
          testid: row.hook,
          hint: row.hint || null,
          check: (t) => parseText(r, t, live()).error || "",
          apply: (t) => put(parseText(r, t, live()).value),
        });
        f.control.addEventListener("keydown", (event) => {
          if (event.key === "Enter") scope.flush();
          if (event.key === "Escape") f.set(show(valueOf(getPanel(), row)));
        });
        return { node: f.node, error: f.error, set: (v) => f.set(show(v)) };
      }
      case "checks": {
        const f = checksField({ label: row.label, items: row.items, value: Array.isArray(value) ? value : [], max: row.max ?? null, testid: row.hook, apply: (list) => {
          const error = checkChecks(row, list);
          f.error(error);
          if (!error) put(list);
        } });
        return f;
      }
      case "field": {
        const names = () => hints();
        const make = (list) => {
          const current = valueOf(getPanel(), row);
          return fieldItems(list, current, row.nullText || "Not set");
        };
        const f = selectField({ label: row.label, options: make(names()), value: value === null || value === undefined ? "" : value, testid: row.hook, apply: (v) => put(v === "" ? null : v) });
        const fill = (list) => {
          const current = valueOf(getPanel(), row);
          f.control.replaceChildren(...make(list).map(([v, text]) => el("option", { value: v, text })));
          f.control.value = current === null || current === undefined ? "" : current;
        };
        return { node: f.node, set: () => fill(names()), refresh: fill, error: f.error };
      }
      case "fields": {
        const f = checksField({ label: row.label, items: hints().map((n) => [n, n]), value: Array.isArray(value) ? value : [], max: row.max ?? null, testid: row.hook, missingText: NOT_IN_DATA, apply: (list) => {
          const error = checkChecks(row, list);
          f.error(error);
          if (!error) put(list);
        } });
        const hint = row.hint ? el("p", { class: "field__hint", text: row.hint }) : null;
        if (hint) f.node.append(hint);
        return { node: f.node, set: f.set, error: f.error, refresh: (names) => f.refresh(names.map((n) => [n, n])) };
      }
      case "list":
      case "custom": {
        const builder = row.build || (row.kind === "list" && row.id === "sort" ? buildSortList : null);
        if (builder) {
          const built = builder({ scope, row, hints, get: () => valueOf(getPanel(), row), set: put, options: live });
          const node = built instanceof Node ? built : built.node;
          return { node, set: built.set || (() => {}), error: built.error || (() => {}) };
        }
        if (row.colour) {
          const f = colorChoice(scope, { value, ariaLabel: row.label, testid: row.hook, apply: (v) => put(v) });
          return { node: el("div", { class: "field" }, el("span", { class: "field__label", text: row.label }), f.node), set(v) { f.text.value = v || ""; } };
        }
        const id = row.member.split(".").at(-1);
        const defaults = () => (getPanel().fields && getPanel().fields.defaults) || {};
        const got = defaultsControl(scope, id, { label: row.label, defaults, hook: row.hook, setDefaults: (patch) => put(Object.values(patch)[0]) });
        if (!got) return { node: el("p", { class: "muted", text: `The option ${row.member} is edited in the JSON tab.` }), set() {} };
        return got;
      }
      default:
        return { node: el("p", { class: "muted", text: `The option ${row.member} is edited in the JSON tab.` }), set() {} };
    }
  }

  const rowsOf = (group) => table.rows.filter((r) => r.group === group);
  const nodes = new Map();
  for (const row of table.rows) {
    const entry = { row, last: keyOf(getPanel(), row), wrapper: null, ctl: null };
    entry.ctl = build(row, entry);
    entry.wrapper = el("div", { class: "orow", dataset: { option: row.member, row: row.id } }, entry.ctl.node);
    entries.push(entry);
    nodes.set(row.id, entry.wrapper);
  }
  const groups = table.groups.map(([key, title]) => {
    if (!open.has(key)) open.set(key, true);
    const g = optionGroup({ key, title, open, children: rowsOf(key).map((r) => nodes.get(r.id)) });
    groupNodes.set(key, g);
    return g;
  });
  const extra = { standard: rowsOf("standard").map((r) => nodes.get(r.id)), thresholds: rowsOf("thresholds").map((r) => nodes.get(r.id)) };
  refresh();
  return { groups, extra, status, refresh, refreshFields, table, writes: () => writes, setGroup(key, node) { groupNodes.set(key, node); } };
}
