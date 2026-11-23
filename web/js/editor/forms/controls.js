// The controls that the forms share (10.3a, 10.3b): a debounce scope, text and number fields, selects, icon buttons,
// a colour choice and the list editor (the buttons "Move up", "Move down" and "Remove" of each item, and "Add").
// Markup only: no text of a document goes in with innerHTML. A control writes the draft through the callback that the form gives.

import { NAMED } from "../../colors.js";
import { isColorValid } from "../validators.js";
import { button, el, toggle, uniqueId } from "../../ui.js";
import { field } from "../form.js";
import { atLimit, limitText, moveItem, removeItem, replaceItem } from "./model.js";
import { parseInteger, parseNumber } from "./rules.js";

export const DEBOUNCE_MS = 300;

// The 30 hue tokens and the two other tokens of 4.19.
export const COLOR_NAMES_ALL = Object.freeze([...Object.keys(NAMED), "text", "transparent"]);

// --- the debounce scope --------------------------------------------------------------------------------------------------------

// A scope holds the changes of text fields that wait for their 300 ms. `flush()` runs them at once, so a click on "Remove" or "Back"
// never loses the last keys, and a move of an item never writes to the wrong index.
export function createScope() {
  const waiting = new Set();
  return {
    // Wraps `fn` so that its calls wait `ms` after the last one. `fn.now()` runs the waiting call at once.
    debounce(fn, ms = DEBOUNCE_MS) {
      let timer = 0;
      let args = null;
      const run = (...a) => {
        args = a;
        clearTimeout(timer);
        waiting.add(run);
        timer = setTimeout(run.now, ms);
      };
      run.now = () => {
        clearTimeout(timer);
        waiting.delete(run);
        if (!args) return;
        const a = args;
        args = null;
        fn(...a);
      };
      run.cancel = () => {
        clearTimeout(timer);
        waiting.delete(run);
        args = null;
      };
      return run;
    },
    flush() {
      for (const run of [...waiting]) run.now();
    },
    cancel() {
      for (const run of [...waiting]) run.cancel();
    },
  };
}

// --- buttons ---------------------------------------------------------------------------------------------------------------------

// An icon button with an accessible name. `disabled` uses the attribute of the button; `inactive` uses aria-disabled and keeps the tooltip.
export function iconBtn(label, iconName, onClick, { disabled = false, testid = null, tooltip = null } = {}) {
  const b = button(label, { disabled, dataset: testid ? { testid } : {}, on: { click: onClick } }, iconName, { iconOnly: true, variant: "ghost", size: "sm" });
  if (tooltip) b.setAttribute("data-tooltip", tooltip);
  return b;
}

// A button that is not active but stays in the tab order, with a tooltip that says why (the limits of 4.35).
export function setInactive(node, inactive, why = null) {
  if (inactive) {
    node.setAttribute("aria-disabled", "true");
    if (why) node.setAttribute("data-tooltip", why);
  } else {
    node.removeAttribute("aria-disabled");
  }
  return node;
}

// --- fields ------------------------------------------------------------------------------------------------------------------------

export const testidOf = (control, testid) => {
  if (testid) control.dataset.testid = testid;
  return control;
};

/**
 * A text field that writes the draft 300 ms after the last key.
 * `check(text)` returns an error text or "". `apply(text)` runs for a valid text only.
 * `label` null gives a bare input with `ariaLabel` (the rows of a list).
 */
export function textField(scope, { label = null, ariaLabel = null, value = "", check = () => "", apply, testid = null, mono = false, list = null, placeholder = null, hint = null, multiline = false, rows = 3, numeric = false }) {
  const control = multiline
    ? el("textarea", { class: "input", rows, spellcheck: "false" })
    : el("input", { class: `input${mono ? " input--mono" : ""}${numeric ? " input--num" : ""}`, type: "text", spellcheck: "false", autocomplete: "off", ...(list ? { list } : {}), ...(placeholder ? { placeholder } : {}), ...(numeric ? { inputmode: "decimal" } : {}) });
  control.value = value;
  if (ariaLabel) control.setAttribute("aria-label", ariaLabel);
  testidOf(control, testid);
  let out;
  if (label !== null) out = field(label, control, { hint });
  else {
    const err = el("p", { class: "field-error", role: "alert", hidden: true });
    out = {
      node: el("div", { class: "field" }, control, err),
      control,
      error(text) {
        err.textContent = text || "";
        err.hidden = !text;
        if (text) control.setAttribute("aria-invalid", "true");
        else control.removeAttribute("aria-invalid");
      },
    };
  }
  const run = scope.debounce((text) => {
    const error = check(text);
    out.error(error);
    if (!error) apply(text);
  });
  control.addEventListener("input", () => run(control.value));
  control.addEventListener("blur", () => run.now());
  out.set = (text) => {
    control.value = text;
    out.error("");
    run.cancel();
  };
  return out;
}

/** A number field. `optional`: an empty text gives `null` to `apply`. `apply(number | null)`. `extra(number)` is a second check. */
export function numberField(scope, { label = null, ariaLabel = null, value, optional = false, integer = null, extra = () => "", apply, testid = null, placeholder = null, hint = null }) {
  const text = value === undefined || value === null ? "" : String(value);
  return textField(scope, {
    label,
    ariaLabel,
    value: text,
    testid,
    placeholder,
    hint,
    numeric: true,
    check(t) {
      if (integer) {
        if (optional && t.trim() === "") return "";
        const n = parseInteger(t, integer[0], integer[1]);
        return n === undefined ? `Use a whole number from ${integer[0]} to ${integer[1]}${optional ? ", or leave it empty" : ""}.` : extra(n);
      }
      const n = parseNumber(t);
      if (n === undefined) return `Use a number${optional ? ", or leave it empty" : ""}.`;
      if (n === null) return optional ? "" : "Enter a number.";
      return extra(n);
    },
    apply(t) {
      const n = integer ? (t.trim() === "" ? null : parseInteger(t, integer[0], integer[1])) : parseNumber(t);
      apply(n);
    },
  });
}

/** A select. `options` is [[value, text], ...]. `apply(value)` runs at each change. */
export function selectField({ label = null, ariaLabel = null, options, value, apply, testid = null, hint = null }) {
  const control = el("select", { class: "input" });
  for (const [v, text] of options) control.append(el("option", { value: v, text }));
  control.value = value;
  if (ariaLabel) control.setAttribute("aria-label", ariaLabel);
  testidOf(control, testid);
  control.addEventListener("change", () => apply && apply(control.value));
  if (label === null) return { node: control, control, error() {} };
  const out = field(label, control, { hint });
  return out;
}

/** A switch with its own visible label. `apply(boolean)`. */
export function switchField({ label, checked, apply, testid = null }) {
  const t = toggle({ label, checked, testid, onChange: (value) => apply(value) });
  return { node: el("div", { class: "field field--switch" }, t.el), input: t.input };
}

// --- colours -------------------------------------------------------------------------------------------------------------------------

/**
 * A colour for a mapping or a step: a select of the named colours with "Custom" and, when `nullable`, "No colour"; a text for the value.
 * `apply(value | null)` runs for a valid colour only.
 */
export function colorChoice(scope, { value, nullable = false, ariaLabel = "Colour", apply, testid = null }) {
  const names = COLOR_NAMES_ALL;
  const known = (v) => (v === null || v === "" ? (nullable ? "" : "__custom") : names.includes(v) ? v : "__custom");
  const options = [...(nullable ? [["", "No colour"]] : []), ...names.map((n) => [n, n]), ["__custom", "Custom"]];
  const select = el("select", { class: "input", "aria-label": `${ariaLabel} choice` });
  for (const [v, text] of options) select.append(el("option", { value: v, text }));
  select.value = known(value);
  const text = el("input", { class: "input input--mono", type: "text", spellcheck: "false", autocomplete: "off", "aria-label": `${ariaLabel} value`, value: value || "", placeholder: "#rrggbb" });
  const dot = el("span", { class: "step__dot", "aria-hidden": "true" });
  const err = el("p", { class: "field-error", role: "alert", hidden: true });
  testidOf(select, testid);
  if (testid) text.dataset.testid = `${testid}-text`;
  const paint = () => {
    const v = text.value;
    dot.style.setProperty("--swatch-color", v && isColorValid(v) ? (names.includes(v) ? `var(--viz-${v})` : v) : "transparent");
    text.disabled = select.value === "";
  };
  const show = (message) => {
    err.textContent = message;
    err.hidden = !message;
    if (message) text.setAttribute("aria-invalid", "true");
    else text.removeAttribute("aria-invalid");
  };
  select.addEventListener("change", () => {
    if (select.value === "") {
      text.value = "";
      show("");
      apply(null);
    } else if (select.value === "__custom") {
      if (!isColorValid(text.value)) text.value = "";
      text.focus();
    } else {
      text.value = select.value;
      show("");
      apply(select.value);
    }
    paint();
  });
  const run = scope.debounce((v) => {
    if (!isColorValid(v)) {
      show("A colour is a name, #rgb, #rrggbb, #rrggbbaa, rgb(...) or rgba(...).");
      return;
    }
    show("");
    select.value = known(v);
    apply(v);
    paint();
  });
  text.addEventListener("input", () => {
    paint();
    run(text.value);
  });
  paint();
  return { node: el("div", { class: "colorchoice" }, select, el("span", { class: "colorchoice__row" }, dot, text), err), select, text };
}

// --- the list editor ---------------------------------------------------------------------------------------------------------------

/**
 * A list of items with the buttons "Move up", "Move down" and "Remove" on each item, and an "Add" button (10.3a).
 *  get(): the array now.        set(next): writes the array to the draft (no redraw by the draft).
 *  limit, limitWhat: the limit of 4.35 and its words.     minItems: "Remove" is not active at or below it.
 *  title(item, i): the text of the head of an item.       renderItem(item, i, ctx): the body of an item (a Node or an array).
 *  ctx.update(patch): changes the item i with no redraw.  ctx.replace(item): changes the item i and redraws.
 *  add: { label, testid, kinds: [[value, text]] | null, make(kind) } builds the "Add" control.
 */
// `limitMessage` replaces the words of the tooltip of "Add" at the limit.
// `beforeRemove(item, i, proceed)` can stop a remove to ask a question: it returns true and calls `proceed()` when the person confirms.
export function listEditor(scope, { testid, itemTestid, get, set, limit, limitWhat, title, renderItem, add, minItems = 0, removeHint = "This item stays. Remove the whole group instead.", emptyText = null, pageSize = 0, beforeRemove = null, limitMessage = null }) {
  const root = el("div", { class: "flist", dataset: testid ? { testid } : {} });
  const listError = el("p", { class: "field-error", role: "alert", hidden: true });
  let focusAfter = null;
  let page = 0;

  function commit(next, { redraw = true, focus = null } = {}) {
    set(next);
    if (!redraw) return;
    focusAfter = focus;
    draw();
  }

  function structural(fn) {
    scope.flush();
    fn();
  }

  function draw() {
    const items = get();
    listError.hidden = true;
    root.replaceChildren();
    if (items.length === 0 && emptyText) root.append(el("p", { class: "flist__empty muted", text: emptyText }));
    const ol = el("ol", { class: "flist__items" });
    // A long list (the options of a custom variable) shows `pageSize` items at a time.
    if (pageSize) page = Math.max(0, Math.min(page, Math.floor((items.length - 1) / pageSize)));
    const from = pageSize ? page * pageSize : 0;
    const to = pageSize ? Math.min(items.length, from + pageSize) : items.length;
    for (let i = from; i < to; i++) {
      const item = items[i];
      const titleEl = el("span", { class: "fitem__title", text: title(item, i) });
      const ctx = {
        update: (patch) => {
          commit(replaceItem(get(), i, { ...get()[i], ...patch }), { redraw: false });
          titleEl.textContent = title(get()[i], i);
        },
        replace: (next) => commit(replaceItem(get(), i, next)),
        index: i,
        testid: itemTestid(i),
      };
      const up = iconBtn("Move up", "arrow-up", () => structural(() => commit(moveItem(get(), i, -1), { focus: [i - 1, "up"] })), { disabled: i === 0, testid: `${itemTestid(i)}-up` });
      const down = iconBtn("Move down", "arrow-down", () => structural(() => commit(moveItem(get(), i, 1), { focus: [i + 1, "down"] })), { disabled: i === items.length - 1, testid: `${itemTestid(i)}-down` });
      const lastOne = items.length <= minItems;
      const remove = iconBtn("Remove", "trash", () => {
        if (remove.getAttribute("aria-disabled") === "true") return;
        const proceed = () => structural(() => commit(removeItem(get(), i), { focus: [Math.min(i, items.length - 2), "remove"] }));
        if (beforeRemove && beforeRemove(item, i, proceed)) return;
        proceed();
      }, { testid: `${itemTestid(i)}-remove` });
      setInactive(remove, lastOne, removeHint);
      const body = renderItem(item, i, ctx);
      const li = el("li", { class: "fitem", dataset: { testid: itemTestid(i) } },
        el("div", { class: "fitem__head" }, titleEl, el("span", { class: "fitem__actions" }, up, down, remove)),
        el("div", { class: "fitem__body" }, ...(Array.isArray(body) ? body : [body])));
      ol.append(li);
    }
    root.append(ol);
    if (pageSize && items.length > pageSize) {
      const go = (delta) => () => {
        scope.flush();
        page += delta;
        draw();
      };
      const prev = button("Previous", { disabled: page === 0, dataset: testid ? { testid: `${testid}-prev` } : {}, on: { click: go(-1) } }, null, { size: "sm" });
      const next = button("Next", { disabled: to >= items.length, dataset: testid ? { testid: `${testid}-next` } : {}, on: { click: go(1) } }, null, { size: "sm" });
      root.append(el("div", { class: "flist__pager" }, prev, el("span", { class: "muted", role: "status", text: `Items ${from + 1} to ${to} of ${items.length}` }), next));
    }
    if (add) root.append(addControl());
    root.append(listError);
    if (focusAfter) {
      const [at, which] = focusAfter;
      focusAfter = null;
      const li = root.querySelectorAll(":scope > .flist__items > .fitem")[at - from];
      const target = li && (li.querySelector(`[data-testid$="-${which}"]`) || li.querySelector("button:not([disabled])"));
      if (target && !target.disabled) target.focus();
      else {
        const addButton = root.querySelector(".flist__add button");
        if (addButton) addButton.focus();
      }
    }
  }

  function addControl() {
    const kindSelect = add.kinds ? selectField({ ariaLabel: add.kindLabel || "Kind", options: add.kinds, value: add.kinds[0][0], testid: add.testid ? `${add.testid}-kind` : null }) : null;
    const b = button(add.label, { dataset: add.testid ? { testid: add.testid } : {}, on: { click: () => {
      if (b.getAttribute("aria-disabled") === "true") return;
      structural(() => {
        if (pageSize) page = Math.floor(get().length / pageSize);
        commit([...get(), add.make(kindSelect ? kindSelect.control.value : null)], { focus: [get().length, "remove"] });
      });
    } } }, "plus", { size: "sm" });
    setInactive(b, atLimit(get(), limit), limitMessage || limitText(limit, limitWhat));
    return el("div", { class: "flist__add" }, b, ...(kindSelect ? [kindSelect.node] : []));
  }

  draw();
  root.redraw = draw;
  root.showError = (message) => {
    listError.textContent = message || "";
    listError.hidden = !message;
  };
  return root;
}

export const fieldId = () => uniqueId("fld");

// --- the controls of the option forms (10.3h, F43) ---------------------------------------------------------------------------------

/**
 * A choice: a `<fieldset>` with a `<legend>` and one native radio input for each item (10.3h, rule 3). The arrow keys move the choice and each
 * move writes. `Home` and `End` go to the first and the last item. `items` is [[value, text], ...]. A value that no item has shows no choice, with
 * `aria-invalid` and the error of 10.3h, rule 5. It returns { node, set(value), error(text) }.
 */
export function choiceField({ label, items, value, apply, testid = null }) {
  const name = uniqueId("choice");
  const err = el("p", { class: "field-error", role: "alert", hidden: true });
  const group = el("div", { class: "seg seg--radio" });
  const inputs = items.map(([, text], i) => {
    const input = el("input", { type: "radio", name, value: String(i), class: "seg__input" });
    group.append(el("label", { class: "seg__item" }, input, el("span", { text })));
    input.addEventListener("change", () => {
      if (input.checked) {
        paint(items[i][0]);
        apply(items[i][0]);
      }
    });
    input.addEventListener("keydown", (event) => {
      if (event.key !== "Home" && event.key !== "End") return;
      event.preventDefault();
      const target = inputs[event.key === "Home" ? 0 : inputs.length - 1];
      target.checked = true;
      target.focus();
      target.dispatchEvent(new Event("change", { bubbles: true }));
    });
    return input;
  });
  const node = el("fieldset", { class: "field choice", dataset: testid ? { testid } : {} }, el("legend", { class: "field__label", text: label }), group, err);
  function paint(next) {
    const at = items.findIndex(([v]) => v === next);
    inputs.forEach((input, i) => {
      input.checked = i === at;
    });
    // One tab stop for the group: a group with no choice keeps the first item in the order.
    const bad = at < 0 && next !== undefined;
    if (bad) node.setAttribute("aria-invalid", "true");
    else node.removeAttribute("aria-invalid");
    err.hidden = !bad;
    err.textContent = bad ? `The value ${JSON.stringify(next)} is not valid. Choose a value.` : "";
  }
  paint(value);
  return { node, set: paint, error(text) { err.textContent = text || ""; err.hidden = !text; } };
}

/**
 * A slider with a number field (10.3h, rules 3 and 7): one group with the name of the option. The slider writes at each `input`, at most once in
 * each frame, or on release when `heavy()` is true (more than 5,000 points, decision ID-K293). The number field writes 300 ms after the last key.
 * `scope.flush()` runs a write that waits. The group is a `role="group"` named by its label. `apply(integer)` runs for a valid value only.
 */
export function sliderField(scope, { label, value, min, max, apply, testid = null, rangeTestid = testid ? `${testid}-slider` : null, heavy = () => false, hint = null }) {
  const labelId = uniqueId("sl");
  const range = el("input", { type: "range", min, max, step: 1, value: String(value), "aria-label": `${label}, slider`, dataset: rangeTestid ? { testid: rangeTestid } : {} });
  const num = textField(scope, {
    label: null,
    ariaLabel: `${label}, value`,
    value: String(value),
    numeric: true,
    testid,
    check: (text) => (/^-?\d+$/.test(text.trim()) ? (Number(text) >= min && Number(text) <= max ? "" : `Use a whole number from ${min} to ${max}.`) : `Use a whole number from ${min} to ${max}.`),
    apply: (text) => {
      range.value = String(Number(text));
      apply(Number(text));
    },
  });
  num.control.addEventListener("keydown", (event) => {
    if (event.key === "Enter") scope.flush();
    if (event.key === "Escape") num.set(range.value);
  });
  // The last value of a frame wins. The pending write is in the scope, so a redraw of the pane never loses it.
  const run = scope.debounce((n) => apply(n), 60000);
  let frame = 0;
  range.addEventListener("input", () => {
    num.set(range.value);
    if (heavy()) return;
    run(Number(range.value));
    if (!frame) {
      frame = requestAnimationFrame(() => {
        frame = 0;
        run.now();
      });
    }
  });
  range.addEventListener("change", () => {
    num.set(range.value);
    run(Number(range.value));
    run.now();
  });
  const node = el("div", { class: "field sfield", role: "group", "aria-labelledby": labelId }, el("span", { class: "field__label", id: labelId, text: label }), el("div", { class: "sfield__row" }, range, num.node), ...(hint ? [el("span", { class: "field__hint", text: hint })] : []));
  return {
    node,
    control: num.control,
    range,
    set(n) {
      range.value = String(n);
      num.set(String(n));
    },
    error: num.error,
  };
}

/**
 * Check boxes: a `<fieldset>` with a `<legend>` and one check box for each item (10.3h, rule 3). `max` is the limit: at the limit the unchecked
 * boxes are `disabled` and a hint says "<n> of <max> chosen". A value that no item has shows as an extra item with `missingText`.
 * `apply(list)` runs at each change with the values in the order of the choice. It returns { node, set(list), error(text), refresh(items) }.
 */
export function checksField({ label, items, value, max = null, apply, testid = null, missingText = "(not valid)" }) {
  const err = el("p", { class: "field-error", role: "alert", hidden: true });
  const list = el("div", { class: "checks__list", dataset: testid ? { testid: `${testid}-list` } : {} });
  const hint = el("p", { class: "field__hint", hidden: max === null });
  const node = el("fieldset", { class: "field checks", dataset: testid ? { testid } : {} }, el("legend", { class: "field__label", text: label }), list, hint, err);
  let current = [...value];
  let shown = items;
  function draw() {
    list.replaceChildren();
    const all = [...shown, ...current.filter((v) => !shown.some(([x]) => x === v)).map((v) => [v, `${v} ${missingText}`])];
    for (const [v, text] of all) {
      const box = toggle({ label: text, kind: "check", checked: current.includes(v), onChange: (on) => {
        current = on ? [...current, v] : current.filter((x) => x !== v);
        paint();
        apply([...current]);
      } });
      box.input.dataset.value = String(v);
      list.append(box.el);
    }
    // A list that scrolls (12 rem) is a stop of the Tab key, so the keys of the keyboard reach it.
    if (all.length > 6) list.tabIndex = 0;
    else list.removeAttribute("tabindex");
    paint();
  }
  function paint() {
    for (const input of list.querySelectorAll("input")) {
      input.checked = current.some((v) => String(v) === input.dataset.value);
      input.disabled = max !== null && current.length >= max && !input.checked;
    }
    if (max !== null) hint.textContent = `${current.length} of ${max} chosen`;
  }
  draw();
  return {
    node,
    set(next) {
      current = [...next];
      draw();
    },
    refresh(nextItems) {
      shown = nextItems;
      draw();
    },
    error(text) {
      err.textContent = text || "";
      err.hidden = !text;
    },
  };
}
