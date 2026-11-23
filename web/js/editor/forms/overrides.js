// The form of the overrides (10.3a, 4.17): a list of overrides. An override has a matcher and a list of properties.
// The matcher has three kinds: a name, a regex, a list of names with a mode. The properties are the members of `set` (4.18).

import { el, uniqueId } from "../../ui.js";
import { addableProperties, allowedIn, initialValue, orderSet, propertyById } from "../props.js";
import { iconBtn, listEditor, selectField, setInactive, textField } from "./controls.js";
import { LIMITS, OVERRIDE_MATCHERS, matcherOf, newOverride } from "./model.js";
import { propertyControl } from "./properties.js";
import { checkBytes, checkPattern } from "./rules.js";

const MATCH_TEXT = Object.fromEntries(OVERRIDE_MATCHERS);

const describe = (o) => {
  const m = o.match;
  const what = m.by === "names" ? `${m.mode === "exclude" ? "not " : ""}${m.names.length} names` : JSON.stringify(m.value);
  return `${MATCH_TEXT[m.by] || m.by}: ${what}`;
};

// The list of the names of a matcher `names`: 1 to 64 texts, a select "Include" or "Exclude".
function namesEditor(scope, { match, ctx, listId, testid }) {
  const list = listEditor(scope, {
    testid: `${testid}-names`,
    itemTestid: (n) => `${testid}-name-${n}`,
    get: () => ctx.current().match.names.map((name) => ({ name })),
    set: (arr) => ctx.update({ match: { ...ctx.current().match, names: arr.map((x) => x.name) } }),
    limit: LIMITS.names,
    limitWhat: "names",
    minItems: 1,
    removeHint: "A matcher needs one name. Use another matcher kind instead.",
    title: (x, n) => `Name ${n + 1}`,
    add: { label: "Add name", testid: `${testid}-name-add`, make: () => ({ name: "Value" }) },
    renderItem: (x, n, item) => textField(scope, { ariaLabel: `Name ${n + 1}`, value: x.name, list: listId, check: (v) => checkBytes(v, 1, 1024, "The name"), apply: (v) => item.update({ name: v }), testid: `${testid}-name-${n}-text` }).node,
  });
  const mode = selectField({ label: "Mode", options: [["include", "Include"], ["exclude", "Exclude"]], value: match.mode, testid: `${testid}-mode`, apply: (v) => ctx.update({ match: { ...ctx.current().match, mode: v } }) });
  return [mode.node, list];
}

function propertyRow(scope, { override, n, id, ctx, type }) {
  const prop = propertyById(id);
  const set = override.set;
  const rowId = `override-${n}-property-${id}`;
  const only = Object.keys(set).length <= 1;
  const write = (value) => ctx.update({ set: orderSet({ ...ctx.current().set, [id]: value }) });
  const remove = iconBtn("Remove", "trash", () => {
    if (remove.getAttribute("aria-disabled") === "true") return;
    scope.flush();
    const next = { ...ctx.current().set };
    delete next[id];
    ctx.replace({ ...ctx.current(), set: next });
  }, { testid: `${rowId}-remove` });
  setInactive(remove, only, "An override needs one property. Remove the override instead.");
  const head = el("div", { class: "prop__head" });
  const body = el("div", { class: "prop__body" });
  const row = el("div", { class: "prop", dataset: { testid: rowId, property: id } }, head, body);
  if (!prop) {
    head.append(el("span", { class: "prop__label", text: id }), remove);
    body.append(el("p", { class: "muted", text: `The property ${id} is edited in the JSON tab.` }));
    return row;
  }
  let node;
  let control = null;
  if (!allowedIn(id, type, "set")) {
    node = el("p", { class: "field-error", role: "alert", text: "This property is not permitted for this type of panel." });
  } else {
    const made = propertyControl(prop, set[id], { scope, id: `${rowId}-value`, linksScope: `override-${n}`, getSet: () => ctx.current().set, setValue: write });
    node = made.node;
    control = made.control || null;
  }
  const label = control ? el("label", { class: "prop__label", text: prop.label }) : el("span", { class: "prop__label", text: prop.label });
  if (control) {
    if (!control.id) control.id = uniqueId("fld");
    label.htmlFor = control.id;
  }
  head.append(label, remove);
  body.append(node);
  return row;
}

/**
 * `type()` gives the panel type. `get()` and `set(array)` read and write `fields.overrides`. `hints()` gives the field names of the last result.
 * The hooks are `overrides`, `override-<n>`, `override-add`, `override-<n>-property-add`, `override-<n>-property-<id>`.
 */
export function overridesForm(scope, { type, get, set, hints }) {
  return listEditor(scope, {
    testid: "overrides",
    itemTestid: (i) => `override-${i}`,
    get,
    set,
    limit: LIMITS.overrides,
    limitWhat: "overrides",
    emptyText: "No override.",
    title: (o) => describe(o),
    add: { label: "Add override", testid: "override-add", make: () => newOverride(hints()[0] || "Value") },
    renderItem(o, n, ctx) {
      const base = ctx.testid;
      const listId = uniqueId("fnames");
      const datalist = el("datalist", { id: listId }, ...hints().map((x) => el("option", { value: x })));
      const live = { ...ctx, current: () => get()[n] };
      const matchKind = selectField({ label: "Match", options: OVERRIDE_MATCHERS, value: o.match.by, testid: `${base}-by`, apply: (by) => {
        scope.flush();
        ctx.replace({ ...get()[n], match: matcherOf(by, get()[n].match) });
      } });
      const matcherNodes = [];
      if (o.match.by === "names") matcherNodes.push(...namesEditor(scope, { match: o.match, ctx: live, listId, testid: base }));
      else {
        const regex = o.match.by === "regex";
        matcherNodes.push(textField(scope, {
          label: regex ? "Pattern" : "Field name",
          value: o.match.value,
          list: regex ? null : listId,
          mono: regex,
          check: (v) => (regex ? checkPattern(v) : checkBytes(v, 1, 1024, "The field name")),
          apply: (v) => live.update({ match: { ...live.current().match, value: v } }),
          testid: `${base}-value`,
          hint: regex ? "A text is an exact match. /pattern/flags matches a part." : null,
        }).node);
      }
      const rows = Object.keys(o.set).map((id) => propertyRow(scope, { override: o, n, id, ctx: live, type: type() }));
      const addable = addableProperties(type(), o.set);
      const addSelect = selectField({
        ariaLabel: "Add property",
        options: [["", "Add property"], ...addable.map((p) => [p.id, p.label])],
        value: "",
        testid: `${base}-property-add`,
        apply(id) {
          if (!id) return;
          scope.flush();
          live.replace({ ...live.current(), set: orderSet({ ...live.current().set, [id]: initialValue(id) }) });
        },
      });
      return [
        el("fieldset", { class: "omatch" }, el("legend", { class: "visually-hidden", text: "Matcher" }), matchKind.node, ...matcherNodes),
        el("div", { class: "props" }, ...rows),
        el("div", { class: "field" }, addSelect.node),
        datalist,
      ];
    },
  });
}
