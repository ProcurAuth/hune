// The ad hoc filters (4.10c, 9.10a): the URL form of Grafana 12.4.2, the checks of the page, the request member (7.7a) and the control of
// the bar (textContent only). A selection is { all: false, values }: one address item for each filter, so the generic code stays the same.

import { el, icon } from "./ui.js";

export const OPS = ["=", "!="];
export const MAX_FILTERS = 16;
// The escapes of Grafana 12.4.2 (measure of 2026-11-22). The read undoes them in this order, as Grafana does.
const ESC = [["|", "__gfp__"], [",", "__gfc__"], ["#", "__gfh__"]];
const esc = (s) => ESC.reduce((t, [c, e]) => t.split(c).join(e), s);
const unesc = (s) => ESC.reduce((t, [c, e]) => t.split(e).join(c), s);
const cut = (t) => (t.length > 40 ? `${t.slice(0, 40)}...` : t);

/** The text of one `var-<name>` item (9.5): `key|op|value`, with the escapes of Grafana. */
export const filterText = (f) => `${esc(f.key)}|${f.op}|${esc(f.value)}`;

/** Reads one item as Grafana does: the text before a `#`, cut at `|`, each part before a `,` (a label). Null for fewer than three parts. */
export function parseFilterText(text) {
  const p = String(text).split("#")[0].split("|").map((x) => unesc(x.split(",")[0]));
  return p.length < 3 ? null : { key: p[0], op: p[1], value: p[2] };
}

/**
 * The fault of a filter (7.7a), or "": a permitted key, `=` or `!=`, a value of 0 to 1,024 bytes with no control character. The page also
 * refuses the text of an escape in a value, which the address would change (9.10a).
 */
export function filterError(f, keys) {
  if (!keys.includes(f.key) || !OPS.includes(f.op)) return "The key or the operator is not permitted.";
  if (new TextEncoder().encode(f.value).length > 1024) return "A value has 1024 bytes at most.";
  if (/__gf[pch]__/.test(f.value)) return "A value cannot hold __gfp__, __gfc__ or __gfh__.";
  return /[\u0000-\u001f\u007f]/.test(f.value) ? "A value has no control character." : "";
}

/** The selection from the address items, or the values kept after a 409. A bad item or one after the 16th is dropped and named in `notice`. */
export function adhocSelection(variable, keys, items) {
  const values = [];
  const dropped = [];
  for (const item of items || []) {
    if (item === "") continue;
    const f = parseFilterText(item);
    if (!f || filterError(f, keys) || values.length >= MAX_FILTERS) dropped.push(item);
    else values.push(filterText(f));
  }
  const list = dropped.slice(0, 3).map((t) => `"${cut(t)}"`).join(", ") + (dropped.length > 3 ? ` and ${dropped.length - 3} more` : "");
  return { selection: { all: false, values }, notice: dropped.length ? `The page does not use these filters of ${variable.label || variable.name}: ${list}.` : null };
}

/** The items of the address: one for each filter, or one empty item, as Grafana writes. */
export const adhocUrl = (sel) => (sel.values.length ? sel.values : [""]);

/** The member of the request (7.7a). */
export const adhocRequest = (sel) => ({ filters: sel.values.map(parseFilterText) });

/** The permitted keys of the member `adhoc` of 6.14 for each adhoc variable. A wrong or absent member gives no key (fail closed). */
export function readAdhocKeys(doc, member) {
  const ok = (v, list) => Array.isArray(list) && list.every((k) => v.keys.includes(k));
  return new Map(doc.variables.filter((v) => v.kind === "adhoc").map((v) => [v.name, ok(v, member && member[v.name]) ? member[v.name] : []]));
}

/** The control (9.10a): a chip for each filter with a remove button, and a plus button that shows the form of a filter in the bar. */
export function adhocControl(variable, state, apply) {
  const name = variable.name;
  const label = variable.label || name;
  const t = (part) => ({ testid: `variable-${name}-${part}` });
  // The keys of 6.14 from the runtime of the page, else (a variable of the draft only) the keys of the variable.
  const keysOf = () => (state.adhocKeys ? state.adhocKeys.get(name) || [] : variable.keys);
  const values = () => (state.selections.get(name) || { values: [] }).values;
  const list = el("span", { class: "adhoc__list" });
  const addBtn = el("button", { type: "button", class: "btn btn--icon adhoc__add", "aria-label": `Add filter to ${label}`, "data-tooltip": "Add filter", "aria-expanded": "false", dataset: t("add") }, icon("plus", 14));
  const keySel = el("select", { class: "input", "aria-label": "Key", dataset: t("key") });
  const opSel = el("select", { class: "input", "aria-label": "Operator", dataset: t("op") }, ...OPS.map((o) => el("option", { value: o, text: o })));
  const input = el("input", { type: "text", class: "input", "aria-label": "Value", placeholder: "Value", autocomplete: "off", spellcheck: "false", dataset: t("value") });
  const err = el("span", { class: "field-error", role: "alert", hidden: true });
  const form = el("span", { class: "adhoc__form", hidden: true }, keySel, opSel, input,
    el("button", { type: "button", class: "btn btn--primary", text: "Add", dataset: t("apply"), on: { click: add } }), err);
  const set = (next) => {
    apply(name, { all: false, values: next });
    addBtn.focus();
  };
  let shown = null;

  function show(on) {
    form.hidden = !on;
    addBtn.setAttribute("aria-expanded", String(on));
    if (!on) return addBtn.focus();
    keySel.replaceChildren(...keysOf().map((k) => el("option", { value: k, text: k })));
    input.value = err.textContent = "";
    err.hidden = true;
    input.removeAttribute("aria-invalid");
    return keySel.focus();
  }

  function add() {
    const f = { key: keySel.value, op: opSel.value, value: input.value };
    const bad = filterError(f, keysOf());
    err.textContent = bad;
    err.hidden = !bad;
    if (bad) return input.setAttribute("aria-invalid", "true");
    show(false);
    return set([...values(), filterText(f)]);
  }

  // A paint that changes nothing writes nothing: the registry of vars.js watches the page.
  function paint() {
    const now = values();
    const sig = JSON.stringify([now, keysOf()]);
    if (sig === shown) return;
    shown = sig;
    list.replaceChildren(...now.map((v, i) => {
      const f = parseFilterText(v);
      const text = `${f.key} ${f.op} ${f.value}`;
      return el("span", { class: "adhoc__chip", dataset: t("filter") }, el("span", { class: "adhoc__text", title: text, text }),
        el("button", { type: "button", class: "adhoc__remove", "aria-label": `Remove filter with key ${f.key}`, on: { click: () => set(now.filter((_, j) => j !== i)) } }, icon("close", 12)));
    }));
    addBtn.disabled = now.length >= MAX_FILTERS || !keysOf().length;
  }

  addBtn.addEventListener("click", () => show(form.hidden));
  input.addEventListener("keydown", (e) => e.key === "Enter" && (e.preventDefault(), add()));
  form.addEventListener("keydown", (e) => e.key === "Escape" && (e.preventDefault(), e.stopPropagation(), show(false)));
  paint();
  return { el: el("div", { class: "adhoc", role: "group", "aria-label": label }, list, addBtn, form), refresh: paint, focusTarget: addBtn };
}
