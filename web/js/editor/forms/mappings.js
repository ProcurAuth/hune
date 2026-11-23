// The form of the value mappings (10.3a, 4.22): a list of rows. A row has the members of its kind, a text and a colour.
// It serves the group "Value mappings" (`fields.defaults.mappings`) and the property `mappings` of an override.

import { el } from "../../ui.js";
import { colorChoice, listEditor, numberField, selectField, textField } from "./controls.js";
import { LIMITS, MAPPING_KINDS, SPECIAL_MATCHES, newMapping } from "./model.js";
import { checkBytes, checkChars, checkPattern } from "./rules.js";

const describe = (m) => {
  if (m.kind === "value") return `Value ${JSON.stringify(m.value)}`;
  if (m.kind === "range") return `Range ${m.from === null ? "" : m.from} to ${m.to === null ? "" : m.to}`;
  if (m.kind === "regex") return `Regex ${m.pattern}`;
  return `Special ${m.match}`;
};

/**
 * `get()` and `set(array)` read and write the array. `prefix` names the hooks: `<prefix>-<n>`, `<prefix>-add`.
 * `minItems`: 1 in an override, because the property `mappings` has 1 item or more (4.18).
 */
export function mappingsForm(scope, { listTestid = "mappings", prefix = "mapping", get, set, minItems = 0 }) {
  return listEditor(scope, {
    testid: listTestid,
    itemTestid: (i) => `${prefix}-${i}`,
    get,
    set,
    limit: LIMITS.mappings,
    limitWhat: "mappings",
    minItems,
    removeHint: "A property needs one mapping. Remove the property instead.",
    emptyText: "No mapping.",
    title: (m) => describe(m),
    add: { label: "Add mapping", testid: `${prefix}-add`, kinds: MAPPING_KINDS, kindLabel: "Kind of the mapping", make: (kind) => newMapping(kind) },
    renderItem(m, i, ctx) {
      const rowError = el("p", { class: "field-error", role: "alert", hidden: true });
      const showRow = (item) => {
        const bad = item.text === null && item.color === null;
        rowError.textContent = bad ? "A mapping needs a text or a colour." : "";
        rowError.hidden = !bad;
      };
      showRow(m);
      const t = (name) => `${ctx.testid}-${name}`;
      const members = [];
      if (m.kind === "value") {
        members.push(textField(scope, { label: "Value", value: m.value, check: (v) => checkBytes(v, 0, 1024, "The value"), apply: (v) => ctx.update({ value: v }), testid: t("value") }).node);
      } else if (m.kind === "range") {
        const rangeError = el("p", { class: "field-error", role: "alert", hidden: true });
        const bound = (key) => (n) => {
          const next = { ...get()[i], [key]: n };
          const bad = next.from === null && next.to === null ? "A range needs a start or an end." : next.from !== null && next.to !== null && next.from > next.to ? "From must be at most To." : "";
          rangeError.textContent = bad;
          rangeError.hidden = !bad;
          if (!bad) ctx.update({ [key]: n });
        };
        members.push(el("div", { class: "field-grid" },
          numberField(scope, { label: "From", value: m.from, optional: true, placeholder: "no start", apply: bound("from"), testid: t("from") }).node,
          numberField(scope, { label: "To", value: m.to, optional: true, placeholder: "no end", apply: bound("to"), testid: t("to") }).node), rangeError);
      } else if (m.kind === "regex") {
        members.push(textField(scope, { label: "Pattern", value: m.pattern, mono: true, check: (v) => checkPattern(v), apply: (v) => ctx.update({ pattern: v }), testid: t("pattern"), hint: "A text, or /pattern/flags. $1 to $9 give a capture group in the text." }).node);
      } else {
        members.push(selectField({ label: "Match", options: SPECIAL_MATCHES, value: m.match, apply: (v) => ctx.update({ match: v }), testid: t("match") }).node);
      }
      const text = textField(scope, {
        label: "Display text",
        value: m.text === null ? "" : m.text,
        check: (v) => (v === "" ? "" : checkChars(v, 1, 256, "The text")),
        apply: (v) => {
          const next = { ...get()[i], text: v === "" ? null : v };
          ctx.update({ text: next.text });
          showRow(next);
        },
        testid: t("text"),
        hint: "Empty keeps the text of the value.",
      });
      const color = colorChoice(scope, {
        value: m.color,
        nullable: true,
        ariaLabel: "Mapping colour",
        testid: t("color"),
        apply: (v) => {
          ctx.update({ color: v });
          showRow({ ...get()[i], color: v });
        },
      });
      return [...members, text.node, el("div", { class: "field" }, el("span", { class: "field__label", text: "Colour" }), color.node), rowError];
    },
  });
}
