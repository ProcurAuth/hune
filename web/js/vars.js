// The variables bar and the controls of the variables (9.10). A control is the same code in the bar and in a panel of type `variable`
// (4.13a): each variable has one control in the page, and the registry below puts it in the right place.

import { ALL, cut, isNumberText, isOpen, optionsOf, typedValueError } from "./var_options.js";
import { anchorPopup, clear, el, icon, uniqueId } from "./ui.js";
import { labelOf, planHomes } from "./var_layout.js";
import { adhocControl } from "./adhoc.js";

const fold = (s) => String(s).toLowerCase();

// The text of the select: the selected texts joined by " + ", the first three, then " + <n> more".
// Only the first three values need a text: one pass over the options, from the end, so the first option of a value wins (9.10).
export function selectionText(variable, options, selection) {
  const values = selection.all ? ["All"] : selection.values;
  const head = values.slice(0, 3);
  const texts = new Map(head.map((v) => [v, v]));
  if (!selection.all) {
    for (let i = options.length - 1; i >= 0; i--) if (texts.has(options[i].value)) texts.set(options[i].value, options[i].text);
  }
  const shown = head.map((v) => texts.get(v)).join(" + ");
  return values.length > 3 ? `${shown} + ${values.length - 3} more` : shown;
}

function listControl(variable, rt, apply) {
  const labelText = variable.label || variable.name;
  const free = isOpen(variable);
  const button = el("button", { type: "button", class: "btn select__button", "aria-haspopup": "listbox", "aria-expanded": "false", dataset: { testid: `variable-${variable.name}-button` } });
  const valueEl = el("span", { class: "select__value" });
  button.append(valueEl, icon("chevron-down", 12));
  const errorEl = el("span", { class: "select__error", hidden: true });
  const wrapper = el("div", { class: "select" }, button, errorEl);
  let popup = null;
  let draft = null;
  let follow = null;

  const optionList = () => {
    const base = optionsOf(variable, rt.options.get(variable.name) || []);
    return variable.include_all ? [{ value: ALL, text: "All" }, ...base] : base;
  };
  const current = () => rt.selections.get(variable.name);

  function paint() {
    const loaded = rt.options.get(variable.name);
    const text = selectionText(variable, optionsOf(variable, loaded || []), current());
    const err = rt.optionErrors.get(variable.name);
    // A query variable whose options loaded with no row, and that has no value, shows "No options" (9.10, ID-K208).
    const none = !text && !err && variable.kind === "query" && Array.isArray(loaded) && loaded.length === 0;
    const shown = none ? "No options" : text;
    // A write that changes nothing is skipped: the registry watches the page, and a write of the same value would wake it again.
    if (valueEl.textContent !== shown) valueEl.textContent = shown;
    valueEl.classList.toggle("muted", none);
    button.setAttribute("aria-label", `${labelText}: ${none ? "no options" : text || "no value"}`);
    if (errorEl.hidden !== !err) errorEl.hidden = !err;
    const errText = err ? `Options failed: ${err.message}` : "";
    if (errorEl.textContent !== errText) errorEl.textContent = errText;
  }

  function close(applyChange, restoreFocus = true) {
    if (!popup) return;
    const finalDraft = draft;
    if (follow) follow();
    follow = null;
    popup.remove();
    popup = null;
    draft = null;
    button.setAttribute("aria-expanded", "false");
    // The list is gone: the reference to it must go too (9.26).
    button.removeAttribute("aria-controls");
    document.removeEventListener("pointerdown", onDown, true);
    if (applyChange && finalDraft) {
      const before = current();
      const same = before.all === finalDraft.all && before.values.join("\u0000") === finalDraft.values.join("\u0000");
      if (!same) apply(variable.name, finalDraft);
    }
    if (restoreFocus) button.focus();
  }

  // The focus leaves the popup (Tab): it closes like a click outside. A change of a multi variable applies then.
  function onFocusOut(event) {
    const next = event.relatedTarget;
    if (popup && next && !popup.contains(next) && next !== button) close(variable.multi, false);
  }

  function onDown(event) {
    if (popup && !popup.contains(event.target) && !button.contains(event.target)) {
      // A click outside closes the popup. A change of a multi variable applies then.
      close(variable.multi, false);
    }
  }

  function open() {
    if (popup) return close(false);
    const sel = current();
    draft = { all: sel.all, values: [...sel.values] };
    const listId = uniqueId("listbox");
    // An open variable has an editable combobox with list autocomplete (WAI-ARIA Authoring Practices, 9.10, ID-K207): the focus stays in the field.
    const search = el("input", free
      ? { type: "text", class: "input select__search", role: "combobox", "aria-expanded": "true", "aria-controls": listId, "aria-autocomplete": "list", "aria-label": `Search or type ${labelText}`, placeholder: "Search or type a value", autocomplete: "off", spellcheck: "false", dataset: { testid: `variable-${variable.name}-search` } }
      : { type: "search", class: "input select__search", "aria-label": `Search ${labelText}`, placeholder: "Search", dataset: { testid: `variable-${variable.name}-search` } });
    const typedError = free ? el("p", { class: "field-error select__typed-error", id: uniqueId("var-typed-error"), role: "alert", hidden: true }) : null;
    if (free) search.setAttribute("aria-describedby", typedError.id);
    const list = el("ul", { class: "listbox", role: "listbox", id: listId, tabIndex: -1, "aria-label": labelText, "aria-multiselectable": variable.multi ? "true" : null, on: { mousedown: (e) => free && e.preventDefault() } });
    const all = optionList();
    const optionValues = new Set(all.map((o) => o.value));
    const items = [];
    const typed = new Map(); // the typed values of the selection: value -> row (an open variable only)
    let active = null;
    let rowCount = 0;

    // The selected values as a set. A change makes a new draft, and the set follows it (9.10).
    let chosen = { of: null, set: null };
    const picked = () => (chosen.of === draft ? chosen.set : (chosen = { of: draft, set: new Set(draft.values) }).set);

    function isSelected(value) {
      return value === ALL ? draft.all : !draft.all && picked().has(value);
    }

    // The row "Selected (n)" of a multi list (9.10): it selects every option, or keeps the first option only when every option is
    // already selected. A selection is never empty. It is a button before the list, so the list keeps only options.
    const concrete = all.filter((o) => o.value !== ALL);
    const everything = () => draft.all || (concrete.length > 0 && concrete.every((o) => picked().has(o.value)));
    const allRow = variable.multi && concrete.length > 0
      ? el("button", { type: "button", class: "select__all", tabIndex: free ? -1 : null, dataset: { testid: `variable-${variable.name}-selected` }, on: { click: toggleAll } }, el("span", { class: "listbox__mark", "aria-hidden": "true" }), el("span", { class: "select__all-text" }))
      : null;
    const rows = () => [...typed.values(), ...items];

    // The typed values of the draft are rows before the options, in the order of the selection, checked: the chip of the design (ID-K207).
    function syncTyped() {
      const want = draft.all ? [] : draft.values.filter((v) => !optionValues.has(v));
      const wanted = new Set(want);
      for (const [value, row] of typed) {
        if (!wanted.has(value)) {
          row.node.remove();
          typed.delete(value);
        }
      }
      for (const value of want) if (!typed.has(value)) typed.set(value, makeRow({ value, text: value }));
      list.prepend(...want.map((v) => typed.get(v).node));
    }

    // The text in the field is a typed value: the row `Use "<text>"` shows. Else, for a text that breaks the rules, the field shows the error.
    function paintUse() {
      const text = search.value;
      const bad = text === "" ? "" : typedValueError(text);
      typedError.textContent = bad;
      typedError.hidden = !bad;
      if (bad) search.setAttribute("aria-invalid", "true");
      else search.removeAttribute("aria-invalid");
      const taken = text === "" || bad !== "" || rows().some((it) => it.option.value === text || it.option.text === text);
      if (useRow.hidden !== taken) useRow.hidden = taken;
      if (!taken) useRow.lastChild.textContent = `Use "${cut(text)}"`;
    }

    function refreshItems() {
      const needle = fold(search.value);
      if (free) syncTyped();
      for (const it of rows()) {
        it.node.hidden = needle !== "" && !fold(it.option.text).includes(needle);
        it.node.setAttribute("aria-selected", String(isSelected(it.option.value)));
      }
      if (free) {
        paintUse();
        if (active && (active.hidden || !active.isConnected)) setActive(null);
        if (empty) empty.hidden = typed.size > 0;
      }
      if (allRow) {
        const count = draft.all ? 1 : draft.values.length;
        const full = everything() && !draft.all;
        allRow.querySelector(".select__all-text").textContent = `Selected (${count})`;
        allRow.setAttribute("aria-pressed", String(full));
        allRow.setAttribute("aria-label", everything() ? `Selected (${count}). Keep the first option only` : `Selected (${count}). Select all options`);
      }
    }

    function toggleAll() {
      if (everything()) draft = { all: false, values: [concrete[0].value] };
      else draft = variable.include_all ? { all: true, values: [] } : { all: false, values: concrete.map((o) => o.value) };
      refreshItems();
      if (free) search.focus();
    }

    function toggle(option) {
      if (!variable.multi) {
        draft = option.value === ALL ? { all: true, values: [] } : { all: false, values: [option.value] };
        close(true);
        return;
      }
      if (option.value === ALL) draft = draft.all ? { all: false, values: [] } : { all: true, values: [] };
      else {
        const base = draft.all ? [] : draft.values;
        draft = { all: false, values: base.includes(option.value) ? base.filter((v) => v !== option.value) : [...base, option.value] };
      }
      if (free) search.value = "";
      refreshItems();
    }

    function makeRow(option) {
      const node = el("li", { role: "option", class: "listbox__item", tabIndex: -1, id: `${listId}-${rowCount++}`, "aria-selected": "false", dataset: { value: option.value }, text: option.text,
        on: { click: () => toggle(option), keydown: (e) => itemKey(e, option) } });
      node.prepend(el("span", { class: "listbox__mark", "aria-hidden": "true" }));
      return { node, option };
    }

    for (const option of all) {
      const row = makeRow(option);
      items.push(row);
      list.append(row.node);
    }
    const empty = all.length ? null : el("li", { class: "listbox__empty muted", text: free ? "No options. Type a value." : "No options" });
    if (empty) list.append(empty);
    // The row `Use` is the last row of the list. It is hidden until the text of the field is a typed value.
    const useRow = el("li", { role: "option", class: "listbox__item listbox__use", id: `${listId}-use`, hidden: true, "aria-selected": "false", dataset: { testid: `variable-${variable.name}-use` }, on: { click: () => toggle({ value: search.value, text: search.value }) } },
      el("span", { class: "listbox__mark", "aria-hidden": "true" }), el("span"));
    if (free) list.append(useRow);

    const visible = () => rows().filter((it) => !it.node.hidden);
    function move(from, delta) {
      const v = visible();
      if (!v.length) return;
      const at = v.findIndex((it) => it.node === from);
      const next = v[Math.max(0, Math.min(v.length - 1, at + delta))];
      next.node.focus();
    }

    function itemKey(event, option) {
      const node = event.currentTarget;
      if (event.key === "ArrowDown") { event.preventDefault(); move(node, 1); }
      else if (event.key === "ArrowUp") {
        event.preventDefault();
        const v = visible();
        if (v[0] && v[0].node === node) search.focus(); else move(node, -1);
      } else if (event.key === "Home") { event.preventDefault(); visible()[0]?.node.focus(); }
      else if (event.key === "End") { event.preventDefault(); visible().at(-1)?.node.focus(); }
      else if (event.key === " ") {
        event.preventDefault();
        if (variable.multi) toggle(option); else toggle(option);
      } else if (event.key === "Enter") {
        event.preventDefault();
        if (variable.multi) { toggle(option); close(true); } else toggle(option);
      }
    }

    // The active row of the combobox: `aria-activedescendant` names it, and the arrow keys move it through the visible rows, `Use` last (9.26).
    function setActive(node) {
      if (active) active.classList.remove("listbox__item--active");
      active = node;
      if (node) {
        node.classList.add("listbox__item--active");
        search.setAttribute("aria-activedescendant", node.id);
        if (node.scrollIntoView) node.scrollIntoView({ block: "nearest" });
      } else search.removeAttribute("aria-activedescendant");
    }

    // `Enter` in the combobox: the active row, else the row whose value or text is the typed text, else the typed text as a new value.
    function comboEnter() {
      const text = search.value;
      if (active) return active === useRow ? toggle({ value: text, text }) : toggle(rows().find((it) => it.node === active).option);
      if (text === "") return close(true);
      const row = rows().find((it) => it.option.value === text || it.option.text === text);
      if (row) return toggle(row.option);
      return typedValueError(text) ? undefined : toggle({ value: text, text });
    }

    function comboKey(event) {
      if (event.key === "Enter") { event.preventDefault(); comboEnter(); return; }
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      const nav = [...visible().map((it) => it.node), ...(useRow.hidden ? [] : [useRow])];
      const at = nav.indexOf(active);
      const next = event.key === "ArrowDown" ? nav[at + 1] || nav[0] : at > 0 ? nav[at - 1] : nav.at(-1);
      if (next) setActive(next);
    }

    search.addEventListener("input", () => {
      if (free) setActive(null);
      refreshItems();
    });
    search.addEventListener("keydown", (event) => {
      if (free) return comboKey(event);
      if (event.key === "ArrowDown") { event.preventDefault(); visible()[0]?.node.focus(); }
      else if (event.key === "Enter") {
        event.preventDefault();
        if (variable.multi) close(true);
        else { const first = visible()[0]; if (first) toggle(first.option); }
      }
      return undefined;
    });
    popup = el("div", { class: "select__popup", on: { focusout: onFocusOut, keydown: (e) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(false); } } } }, search, ...(free ? [typedError] : []), ...(allRow ? [allRow] : []), list);
    wrapper.append(popup);
    follow = anchorPopup(popup, button, { align: "start", gap: -1 });
    button.setAttribute("aria-expanded", "true");
    button.setAttribute("aria-controls", listId);
    refreshItems();
    document.addEventListener("pointerdown", onDown, true);
    search.focus();
  }

  button.addEventListener("click", open);
  paint();
  return { el: wrapper, refresh: paint, focusTarget: button };
}

function textControl(variable, rt, apply) {
  const labelText = variable.label || variable.name;
  const errId = uniqueId("var-error");
  const input = el("input", { type: "text", class: "input", "aria-label": labelText, "aria-describedby": errId, dataset: { testid: `variable-${variable.name}-input` } });
  const err = el("span", { class: "field-error", id: errId, role: "alert", hidden: true });
  const wrapper = el("div", { class: "textvar" }, input, err);
  const value = () => (rt.selections.get(variable.name).values[0] ?? "");
  input.value = value();

  function commit() {
    const text = input.value;
    if (variable.value_type === "number" && !isNumberText(text)) {
      err.textContent = `"${text}" is not a number. Use a form such as 12 or -1.5.`;
      err.hidden = false;
      input.setAttribute("aria-invalid", "true");
      return;
    }
    err.hidden = true;
    input.removeAttribute("aria-invalid");
    if (text !== value()) apply(variable.name, { all: false, values: [text] });
  }
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); commit(); } });
  input.addEventListener("blur", commit);
  return { el: wrapper, refresh() { if (document.activeElement !== input) input.value = value(); }, focusTarget: input };
}

// --- one control: the label chip and the select or the field ----------------------------------------------------------------------
// `state` has the three maps that a control reads: `selections`, `options` and `optionErrors` (the runtime, or the state of a
// variable that only the draft has, draft_runtime.js).
function buildItem(variable, state, apply) {
  const control = (variable.kind === "text" ? textControl : variable.kind === "adhoc" ? adhocControl : listControl)(variable, state, apply);
  const children = [];
  if (variable.hide !== "label") {
    const id = uniqueId("var-label");
    children.push(el("span", { class: "var__label", id, title: labelOf(variable), text: labelOf(variable) }));
    if (variable.kind === "text") {
      control.focusTarget.setAttribute("aria-labelledby", id);
      control.focusTarget.removeAttribute("aria-label");
    }
  }
  children.push(control.el);
  const node = el("div", { class: "var", dataset: { testid: `variable-${variable.name}` } }, ...children);
  return { el: node, refresh: control.refresh, focusTarget: control.focusTarget };
}

const itemSig = (variable, draft) => JSON.stringify([draft, variable.kind, variable.name, variable.label, variable.hide, variable.multi, variable.include_all, variable.value_type, variable.allow_custom_value]);

/** True when `rt` can hold the controls of the variables (the runtime of the page and the runtime of the draft grid). */
export const canHold = (rt) => !!rt && ((rt.selections && typeof rt.selections.get === "function") || !!rt.base);

/** The variable of that name in the document that `rt` shows, or undefined. The draft runtime reads the draft. */
export const variableOf = (rt, name) => (typeof rt.varDef === "function" ? rt.varDef(name) : rt.varByName ? rt.varByName.get(name) : (rt.variables || []).find((v) => v.name === name));

// One registry for each page runtime. It keeps one control for each variable and puts it in the first usable panel that holds the
// variable, else in the bar. A panel is usable when its grid is in the page and not hidden (a collapsed section stays usable: the
// control is then hidden with its section and the variable stays out of the bar, decision ID-K4). The registry looks again at each
// change of the page (a MutationObserver) so that it needs no call from the pages.
class Controls {
  constructor(rt) {
    this.rt = rt;
    this.items = new Map(); // name -> { el, refresh, focusTarget, sig, draft, from, focusIn }
    this.slots = []; // { name, host, from, onHome }
    this.bar = null;
    this.pageApply = null;
    this.observer = null;
    this.burst = 0;
    this.burstTimer = 0;
    this.scheduled = false;
    this.dead = false;
    this.off = rt.on((type) => {
      if (type === "vars") this.refreshAll();
      else if (type === "destroy") this.destroy();
    });
  }

  setApply(apply) {
    this.pageApply = apply;
  }

  attachBar(bar) {
    this.bar = bar;
    this.reconcile();
  }

  // A panel gives the empty element that will hold the control. `from` is the runtime that the panel reads (the draft runtime in the
  // edit mode). `onHome(true | false)` tells the panel if it has the control now.
  hold(name, host, from, onHome = () => {}) {
    const slot = { name, host, from, onHome, home: null };
    this.slots.push(slot);
    this.watch();
    this.schedule();
    return () => {
      const at = this.slots.indexOf(slot);
      if (at >= 0) this.slots.splice(at, 1);
      this.schedule();
    };
  }

  watch() {
    if (this.observer || typeof MutationObserver !== "function") return;
    // A safety stop: more than 200 calls in 50 ms mean a loop of changes, and the page must stay alive. The panels keep their control.
    this.observer = new MutationObserver(() => {
      this.burst += 1;
      if (!this.burstTimer) this.burstTimer = setTimeout(() => { this.burst = 0; this.burstTimer = 0; }, 50);
      if (this.burst > 200 && this.observer) {
        this.observer.disconnect();
        this.observer = null;
        return;
      }
      this.reconcile();
    });
    this.observer.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["hidden"] });
  }

  schedule() {
    if (this.scheduled || this.dead) return;
    this.scheduled = true;
    queueMicrotask(() => {
      this.scheduled = false;
      this.reconcile();
    });
  }

  refreshAll() {
    for (const item of this.items.values()) item.refresh();
  }

  apply(name, selection) {
    const item = this.items.get(name);
    if (item && item.draft) {
      item.from.setSelection(name, selection);
      this.refreshAll();
    } else if (this.pageApply) this.pageApply(name, selection);
    else this.rt.setSelection(name, selection);
  }

  // The item of a name: from the runtime of the page, or from the draft when only the draft has the variable.
  ensureItem(name, from) {
    const base = this.rt.varByName.get(name);
    const variable = base || (from && typeof from.varDef === "function" ? from.varDef(name) : null);
    if (!variable) return null;
    const draft = !base;
    const sig = itemSig(variable, draft);
    const known = this.items.get(name);
    if (known && known.sig === sig) return known;
    if (known) known.el.remove();
    const state = draft ? from.localState(name) : this.rt;
    const item = buildItem(variable, state, (n, s) => this.apply(n, s));
    Object.assign(item, { sig, draft, from, focusIn: false });
    item.el.addEventListener("focusin", () => {
      item.focusIn = true;
    });
    item.el.addEventListener("focusout", (event) => {
      if (event.relatedTarget) item.focusIn = false;
    });
    this.items.set(name, item);
    return item;
  }

  // The grid of a slot is usable when it is in the page and no grid that hides (the view of one panel) holds it.
  static usable(host) {
    return host.isConnected && !host.closest(".dash[hidden]");
  }

  reconcile() {
    if (this.dead) return;
    const byName = new Map();
    for (const slot of this.slots) {
      if (!byName.has(slot.name)) byName.set(slot.name, []);
      byName.get(slot.name).push({ usable: Controls.usable(slot.host), slot });
    }
    const homes = planHomes(this.rt.variables, byName);
    const barItems = [];
    const moved = [];
    for (const [name, home] of homes) {
      const list = byName.get(name) || [];
      const first = home.slot !== undefined ? list[home.slot].slot : null;
      const item = home.none ? null : this.ensureItem(name, (first || (list[0] && list[0].slot) || {}).from);
      for (const entry of list) {
        const has = entry.slot === first && !!item;
        if (entry.slot.home !== has) {
          entry.slot.home = has;
          entry.slot.onHome(has);
        }
      }
      if (!item) {
        const known = this.items.get(name);
        if (known && known.el.parentNode) known.el.remove();
        continue;
      }
      if (first) {
        if (item.el.parentNode !== first.host) {
          first.host.append(item.el);
          moved.push(item);
        }
      } else if (home.bar && this.bar) barItems.push({ name, item });
      else if (item.el.parentNode) item.el.remove();
    }
    if (this.bar) {
      const order = this.rt.variables.map((v) => v.name);
      barItems.sort((a, b) => order.indexOf(a.name) - order.indexOf(b.name));
      const current = [...this.bar.children];
      const same = current.length === barItems.length && current.every((node, i) => node === barItems[i].item.el);
      if (!same) {
        for (const { item } of barItems) {
          if (item.el.parentNode && item.el.parentNode !== this.bar) moved.push(item);
        }
        this.bar.replaceChildren(...barItems.map((b) => b.item.el));
      }
      if (this.bar.hidden !== (barItems.length === 0)) this.bar.hidden = barItems.length === 0;
    }
    // A control that had the focus keeps it after a move (9.26).
    for (const item of moved) {
      if (item.focusIn && item.el.isConnected && !item.el.contains(document.activeElement) && (!document.activeElement || document.activeElement === document.body)) {
        item.focusTarget.focus({ preventScroll: true });
      }
    }
  }

  destroy() {
    if (this.dead) return;
    this.dead = true;
    this.off();
    if (this.observer) this.observer.disconnect();
    this.observer = null;
    for (const item of this.items.values()) item.el.remove();
    this.items.clear();
    this.slots = [];
  }
}

const registries = new WeakMap();

/** The registry of the controls for a runtime. The draft runtime of the editor shares the registry of the page runtime (`base`). */
export function controlsFor(rt) {
  const root = rt.base || rt;
  let registry = registries.get(root);
  if (!registry) {
    registry = new Controls(root);
    registries.set(root, registry);
  }
  return registry;
}

// Builds the bar. `apply(name, selection)` is called after the viewer changed a variable. The bar holds the controls of the variables
// that no panel of type `variable` holds (9.10). It has no child, and it is hidden, when there is none.
export function createVarsBar(rt, apply) {
  const registry = controlsFor(rt);
  registry.setApply(apply);
  const bar = el("div", { class: "vars-bar", role: "group", "aria-label": "Variables", dataset: { testid: "variables-bar" } });
  registry.attachBar(bar);
  return { el: bar, update: () => registry.refreshAll() };
}

export { clear };
