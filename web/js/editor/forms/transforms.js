// The form of the transformations (10.3a, 4.25): a list of the thirteen kinds. `filter_by_value` has a list of filters.
// A filter has a field name, a matcher kind and the values of the kind. `grouping_to_matrix` has three field names and a select.
// The eleven kinds of 4.25a have their forms in transform_more_form.js. Each transform has a select of its kind (`transform-<n>-kind`).

import { button, el, uniqueId } from "../../ui.js";
import { iconBtn, listEditor, numberField, selectField, setInactive, switchField, textField } from "./controls.js";
import { EMPTY_VALUES, LIMITS, MATCHER_KINDS, TRANSFORM_KINDS, atLimit, limitText, moveItem, newFilter, newFilterMatcher, newTransform, removeItem, replaceItem } from "./model.js";
import { checkBytes, checkPattern, parseNumber } from "./rules.js";
import { moreBody } from "./transform_more_form.js";

const KIND_TEXT = Object.fromEntries(TRANSFORM_KINDS);

// A datalist of the field names of the last result of the panel (10.3a).
function namesList(hints) {
  const id = uniqueId("fnames");
  return { id, node: el("datalist", { id }, ...hints().map((n) => el("option", { value: n }))) };
}

function fieldNameField(scope, { label, value, listId, apply, testid }) {
  return textField(scope, { label, value, list: listId, check: (v) => checkBytes(v, 1, 1024, "The field name"), apply, testid });
}

// The values of one matcher kind. `update(matcher)` writes the matcher of the filter.
function matcherValues(scope, { matcher, update, testid }) {
  const k = matcher.kind;
  if (k === "is_null" || k === "is_not_null") return [];
  if (k === "range") {
    const err = el("p", { class: "field-error", role: "alert", hidden: true });
    const side = (key) => (n) => {
      const next = { ...matcher, [key]: n };
      const bad = next.from > next.to ? "From must be at most To." : "";
      err.textContent = bad;
      err.hidden = !bad;
      if (!bad) {
        matcher = next;
        update(next);
      }
    };
    return [el("div", { class: "field-grid" },
      numberField(scope, { label: "From", value: matcher.from, apply: side("from"), testid: `${testid}-from` }).node,
      numberField(scope, { label: "To", value: matcher.to, apply: side("to"), testid: `${testid}-to` }).node), err];
  }
  if (k === "regex") {
    return [textField(scope, { label: "Pattern", value: matcher.pattern, mono: true, check: (v) => checkPattern(v, { grafana: false }), apply: (v) => update({ ...matcher, pattern: v }), testid: `${testid}-pattern` }).node];
  }
  if (k === "equal" || k === "not_equal") {
    const asNumber = typeof matcher.value === "number";
    const holder = el("div", { class: "field" });
    const draw = (number, current) => {
      holder.replaceChildren(
        switchField({ label: "Number", checked: number, testid: `${testid}-number`, apply: (on) => {
          scope.flush();
          const next = on ? { ...matcher, value: parseNumber(String(current())) ?? 0 } : { ...matcher, value: String(current()) };
          matcher = next;
          update(next);
          draw(on, () => matcher.value);
        } }).node,
        number
          ? numberField(scope, { label: "Value", value: matcher.value, apply: (n) => update((matcher = { ...matcher, value: n })), testid: `${testid}-value` }).node
          : textField(scope, { label: "Value", value: String(matcher.value), check: (v) => checkBytes(v, 0, 1024, "The value"), apply: (v) => update((matcher = { ...matcher, value: v })), testid: `${testid}-value` }).node);
    };
    draw(asNumber, () => matcher.value);
    return [holder];
  }
  return [numberField(scope, { label: "Value", value: matcher.value, apply: (n) => update((matcher = { ...matcher, value: n })), testid: `${testid}-value` }).node];
}

// The list of the filters of one `filter_by_value`. `get()` and `set(array)` read and write the array of the transform.
function filtersList(scope, { prefix, get, set, hints }) {
  const names = namesList(hints);
  const root = el("div", { class: "flist flist--nested", dataset: { testid: `${prefix}-filters` } }, names.node);
  const itemId = (i) => `${prefix}-filter-${i}`;
  let focusAfter = null;
  function commit(next, redraw, focus) {
    set(next);
    if (!redraw) return;
    focusAfter = focus;
    draw();
  }
  function draw() {
    const items = get();
    for (const old of [...root.querySelectorAll(":scope > .flist__items, :scope > .flist__add")]) old.remove();
    const ol = el("ol", { class: "flist__items" });
    items.forEach((f, i) => {
      const structural = (fn) => {
        scope.flush();
        fn();
      };
      const up = iconBtn("Move up", "arrow-up", () => structural(() => commit(moveItem(get(), i, -1), true, [i - 1, "up"])), { disabled: i === 0, testid: `${itemId(i)}-up` });
      const down = iconBtn("Move down", "arrow-down", () => structural(() => commit(moveItem(get(), i, 1), true, [i + 1, "down"])), { disabled: i === items.length - 1, testid: `${itemId(i)}-down` });
      const remove = iconBtn("Remove", "trash", () => {
        if (remove.getAttribute("aria-disabled") === "true") return;
        structural(() => commit(removeItem(get(), i), true, [Math.min(i, items.length - 2), "remove"]));
      }, { testid: `${itemId(i)}-remove` });
      setInactive(remove, items.length <= 1, "A transformation needs one filter. Remove the transformation instead.");
      const matcherHost = el("div", { class: "fmatcher" });
      const drawMatcher = (matcher) => {
        matcherHost.replaceChildren(...matcherValues(scope, { matcher, update: (m) => commit(replaceItem(get(), i, { ...get()[i], matcher: m }), false), testid: `${itemId(i)}-matcher` }));
      };
      drawMatcher(f.matcher);
      const kind = selectField({
        label: "Condition",
        options: MATCHER_KINDS,
        value: f.matcher.kind,
        testid: `${itemId(i)}-kind`,
        apply: (k) => {
          scope.flush();
          const m = newFilterMatcher(k, get()[i].matcher);
          commit(replaceItem(get(), i, { ...get()[i], matcher: m }), false);
          drawMatcher(m);
        },
      });
      const title = el("span", { class: "fitem__title", text: `Filter ${i + 1}` });
      ol.append(el("li", { class: "fitem", dataset: { testid: itemId(i) } },
        el("div", { class: "fitem__head" }, title, el("span", { class: "fitem__actions" }, up, down, remove)),
        el("div", { class: "fitem__body" },
          fieldNameField(scope, { label: "Field", value: f.field, listId: names.id, apply: (v) => commit(replaceItem(get(), i, { ...get()[i], field: v }), false), testid: `${itemId(i)}-field` }).node,
          kind.node, matcherHost)));
    });
    const add = button("Add filter", { dataset: { testid: `${prefix}-filter-add` }, on: { click: () => {
      if (add.getAttribute("aria-disabled") === "true") return;
      scope.flush();
      commit([...get(), newFilter(hints()[0] || "Value")], true, [get().length, "remove"]);
    } } }, "plus", { size: "sm" });
    setInactive(add, atLimit(items, LIMITS.filters), limitText(LIMITS.filters, "filters"));
    root.append(ol, el("div", { class: "flist__add" }, add));
    if (focusAfter) {
      const [at, which] = focusAfter;
      focusAfter = null;
      const li = ol.children[at];
      const target = li && li.querySelector(`[data-testid$="-${which}"]`);
      if (target && !target.disabled) target.focus();
      else add.focus();
    }
  }
  draw();
  return root;
}

// The body of a transform of the two kinds of format 1.
function oldBody(scope, t, i, ctx, { get, hints }) {
  const names = namesList(hints);
  if (t.kind === "grouping_to_matrix") {
    const field = (key, label) => fieldNameField(scope, { label, value: t[key], listId: names.id, apply: (v) => ctx.update({ [key]: v }), testid: `${ctx.testid}-${key}` }).node;
    return [
      field("column_field", "Column"), field("row_field", "Row"), field("value_field", "Cell value"),
      selectField({ label: "Empty value", options: EMPTY_VALUES, value: t.empty_value, testid: `${ctx.testid}-empty`, apply: (v) => ctx.update({ empty_value: v }) }).node,
      names.node,
    ];
  }
  const filters = filtersList(scope, {
    prefix: ctx.testid,
    get: () => get()[i].filters,
    set: (next) => ctx.update({ filters: next }),
    hints,
  });
  return [
    selectField({ label: "Action", options: [["include", "Include rows that pass"], ["exclude", "Exclude rows that pass"]], value: t.mode, testid: `${ctx.testid}-mode`, apply: (v) => ctx.update({ mode: v }) }).node,
    selectField({ label: "Match", options: [["all", "All filters"], ["any", "Any filter"]], value: t.match, testid: `${ctx.testid}-match`, apply: (v) => ctx.update({ match: v }) }).node,
    filters,
  ];
}

/**
 * `get()` and `set(array)` read and write `transforms`. `hints()` gives the field names of the last result of the panel.
 * `queryCount()` gives the count of queries of the panel (the note of `merge`). The hooks are `transforms`, `transform-<n>`,
 * `transform-<n>-kind` and `transform-add`.
 */
export function transformsForm(scope, { get, set, hints, queryCount = () => 2 }) {
  return listEditor(scope, {
    testid: "transforms",
    itemTestid: (i) => `transform-${i}`,
    get,
    set,
    limit: LIMITS.transforms,
    limitWhat: "transformations",
    emptyText: "No transformation.",
    title: (t, i) => `${i + 1}. ${KIND_TEXT[t.kind] || t.kind}`,
    add: { label: "Add transformation", testid: "transform-add", kinds: TRANSFORM_KINDS, kindLabel: "Kind of the transformation", make: (kind) => newTransform(kind, hints()[0] || "Value") },
    renderItem(t, i, ctx) {
      // The select of the kind comes first. A new kind replaces the transform with a new transform of that kind (valid and with no effect).
      const kind = selectField({ label: "Kind", options: TRANSFORM_KINDS, value: t.kind, testid: `${ctx.testid}-kind`, apply: (k) => {
        scope.flush();
        ctx.replace(newTransform(k, hints()[0] || "Value"));
      } }).node;
      const more = moreBody(scope, t, ctx, { get: () => get()[i], hints, queryCount });
      return [kind, ...(more || oldBody(scope, t, i, ctx, { get, hints }))];
    },
  });
}
