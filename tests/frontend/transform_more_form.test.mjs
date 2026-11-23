// SPDX-License-Identifier: MIT OR Apache-2.0
// The forms of the eleven more transforms (10.3a, F28) on a fake DOM: add, edit, move, remove, the error of 4.37 and a hostile text.
import test from "node:test";
import assert from "node:assert/strict";
import { byId, choose, click, errors, installFakeDom, type } from "./fake_dom.mjs";

installFakeDom();
const { createScope } = await import("../../web/js/editor/forms/controls.js");
const { transformsForm } = await import("../../web/js/editor/forms/transforms.js");
const { MORE_KIND_TEXT, moreFaults, newMoreTransform } = await import("../../web/js/editor/forms/transform_more_model.js");

// A form with a draft array. `hints` are the names of the last data.
function setup({ transforms = [], hints = ["a", "b", "c"], queries = 2 } = {}) {
  const state = { draft: transforms };
  const scope = createScope();
  const root = transformsForm(scope, { get: () => state.draft, set: (next) => { state.draft = next; }, hints: () => hints, queryCount: () => queries });
  return { state, scope, root, t: (i = 0) => state.draft[i] };
}

test("the form adds each of the eleven kinds, with a new transform that is valid", () => {
  const f = setup();
  for (const [kind] of MORE_KIND_TEXT) {
    choose(f.root, "transform-add-kind", kind);
    click(f.root, "transform-add");
  }
  assert.equal(f.state.draft.length, 11);
  f.state.draft.forEach((t, i) => {
    assert.deepEqual(t, newMoreTransform(MORE_KIND_TEXT[i][0], "a"), t.kind);
    assert.deepEqual(moreFaults(t), [], t.kind);
    assert.ok(byId(f.root, `transform-${i}-kind`), `the select of the kind of ${i}`);
    assert.equal(byId(f.root, `transform-${i}-kind`).value, t.kind);
  });
});

test("the old kinds stay: a filter and a matrix, and a select of the kind on each", () => {
  const f = setup();
  choose(f.root, "transform-add-kind", "filter_by_value");
  click(f.root, "transform-add");
  choose(f.root, "transform-add-kind", "grouping_to_matrix");
  click(f.root, "transform-add");
  assert.deepEqual(f.state.draft.map((t) => t.kind), ["filter_by_value", "grouping_to_matrix"]);
  assert.ok(byId(f.root, "transform-0-mode"));
  assert.ok(byId(f.root, "transform-1-column_field"));
  assert.ok(byId(f.root, "transform-0-kind"));
});

test("a new kind in the select replaces the transform with a new transform of that kind", () => {
  const f = setup({ transforms: [newMoreTransform("limit")] });
  choose(f.root, "transform-0-kind", "sort_by");
  assert.deepEqual(f.t(), { kind: "sort_by", sort: [{ field: "a", desc: false }] });
  choose(f.root, "transform-0-kind", "filter_by_value");
  assert.equal(f.t().kind, "filter_by_value");
});

test("organize: hide, rename, move; the order is written in full and goes back to empty at the natural order", () => {
  const f = setup({ transforms: [newMoreTransform("organize")] });
  click(f.root, "transform-0-hide-1");
  assert.deepEqual(f.t().hide, ["b"]);
  type(f.scope, f.root, "transform-0-rename-0", "A");
  assert.deepEqual(f.t().rename, [{ field: "a", name: "A" }]);
  click(f.root, "transform-0-up-2");
  assert.deepEqual(f.t().order, ["a", "c", "b"]);
  click(f.root, "transform-0-down-1");
  assert.deepEqual(f.t().order, []);
  type(f.scope, f.root, "transform-0-rename-0", "");
  assert.deepEqual(f.t().rename, []);
  click(f.root, "transform-0-hide-1");
  assert.deepEqual(f.t().hide, []);
  assert.deepEqual(moreFaults(f.t()), []);
});

test("organize: a field that the data does not hold stays in the list with a mark", () => {
  const f = setup({ transforms: [{ kind: "organize", hide: ["gone"], order: [], rename: [{ field: "gone2", name: "G" }] }] });
  assert.ok(byId(f.root, "transform-0-hide-3"));
  assert.ok(byId(f.root, "transform-0-hide-4"));
  const marks = f.root.querySelectorAll(".muted").filter((n) => n.textContent === "not in the data");
  assert.equal(marks.length, 2);
  assert.equal(byId(f.root, "transform-0-hide-3").checked, true);
});

test("organize: a rename text with markup goes into the draft as text, and a new name of 257 characters is an error", () => {
  const f = setup({ transforms: [newMoreTransform("organize")] });
  type(f.scope, f.root, "transform-0-rename-0", "<img src=x onerror=alert(1)>");
  assert.equal(f.t().rename[0].name, "<img src=x onerror=alert(1)>");
  type(f.scope, f.root, "transform-0-rename-1", "x".repeat(257));
  assert.equal(f.t().rename.length, 1);
  assert.match(errors(f.root).join("|"), /1 to 256 characters/);
});

test("calculate_field: the modes, the operands, the alias and the error of two numbers", () => {
  const f = setup({ transforms: [newMoreTransform("calculate_field", "a")] });
  choose(f.root, "transform-0-mode", "binary");
  assert.deepEqual(Object.keys(f.t()), ["kind", "mode", "alias", "replace_fields", "binary"]);
  assert.deepEqual(f.t().binary, { left: { kind: "field", field: "a" }, right: { kind: "number", value: 1 }, operator: "add" });
  choose(f.root, "transform-0-operator", "divide");
  assert.equal(f.t().binary.operator, "divide");
  type(f.scope, f.root, "transform-0-right-value", "2.5");
  assert.deepEqual(f.t().binary.right, { kind: "number", value: 2.5 });
  type(f.scope, f.root, "transform-0-left-field", "b");
  assert.deepEqual(f.t().binary.left, { kind: "field", field: "b" });
  // Two numbers are not permitted (4.34): the form shows the error and the draft keeps the field.
  choose(f.root, "transform-0-left-kind", "number");
  assert.deepEqual(f.t().binary.left, { kind: "field", field: "b" });
  assert.match(errors(f.root).join("|"), /One operand must be a field/);
  choose(f.root, "transform-0-right-kind", "field");
  assert.deepEqual(f.t().binary.right, { kind: "field", field: "Value" });
  type(f.scope, f.root, "transform-0-alias", "S");
  assert.equal(f.t().alias, "S");
  type(f.scope, f.root, "transform-0-alias", "");
  assert.equal(f.t().alias, null);
  click(f.root, "transform-0-replace-fields");
  assert.equal(f.t().replace_fields, true);
  choose(f.root, "transform-0-mode", "reduce_row");
  assert.deepEqual(f.t().reduce, { reducer: "total", include: [] });
  assert.equal(f.t().replace_fields, true);
  choose(f.root, "transform-0-reducer", "mean");
  click(f.root, "transform-0-include-add");
  assert.deepEqual(f.t().reduce, { reducer: "mean", include: ["a"] });
  choose(f.root, "transform-0-mode", "unary");
  choose(f.root, "transform-0-operator", "percent");
  type(f.scope, f.root, "transform-0-field", "b");
  assert.deepEqual(f.t().unary, { operator: "percent", field: "b" });
  choose(f.root, "transform-0-mode", "index");
  click(f.root, "transform-0-as-percentile");
  assert.deepEqual(f.t().index, { as_percentile: true });
  assert.deepEqual(moreFaults(f.t()), []);
});

test("sort_by: add, descending, move, remove, and at most 8 keys", () => {
  const f = setup({ transforms: [newMoreTransform("sort_by", "a")] });
  assert.ok(byId(f.root, "transform-0-sort-0-remove").getAttribute("aria-disabled") === "true", "one key stays");
  click(f.root, "transform-0-sort-add");
  assert.deepEqual(f.t().sort, [{ field: "a", desc: false }, { field: "b", desc: false }]);
  click(f.root, "transform-0-sort-1-desc");
  assert.equal(f.t().sort[1].desc, true);
  click(f.root, "transform-0-sort-1-up");
  assert.deepEqual(f.t().sort.map((k) => k.field), ["b", "a"]);
  type(f.scope, f.root, "transform-0-sort-0-field", "a");
  // The key (a, descending) is not equal to (a, ascending): it is permitted. Equal keys are an error.
  assert.deepEqual(f.t().sort, [{ field: "a", desc: true }, { field: "a", desc: false }]);
  click(f.root, "transform-0-sort-0-desc");
  assert.match(errors(f.root).join("|"), /twice/);
  click(f.root, "transform-0-sort-1-remove");
  assert.equal(f.t().sort.length, 1);
  for (let i = 0; i < 9; i++) click(f.root, "transform-0-sort-add");
  assert.equal(f.t().sort.length, 8);
  assert.equal(byId(f.root, "transform-0-sort-add").getAttribute("aria-disabled"), "true");
  assert.deepEqual(moreFaults(f.t()), []);
});

test("limit: a whole number from 0 to 1,000,000, and a select of the end", () => {
  const f = setup({ transforms: [newMoreTransform("limit")] });
  type(f.scope, f.root, "transform-0-count", "5");
  assert.equal(f.t().count, 5);
  type(f.scope, f.root, "transform-0-count", "1000001");
  assert.equal(f.t().count, 5);
  assert.match(errors(f.root).join("|"), /whole number from 0 to 1000000/);
  type(f.scope, f.root, "transform-0-count", "2.5");
  assert.equal(f.t().count, 5);
  type(f.scope, f.root, "transform-0-count", "0");
  assert.equal(f.t().count, 0);
  choose(f.root, "transform-0-end", "last");
  assert.equal(f.t().end, "last");
});

test("merge: no field, and the note shows when the panel has one query", () => {
  const two = setup({ transforms: [newMoreTransform("merge")], queries: 2 });
  assert.equal(byId(two.root, "transform-0-note").hidden, true);
  const one = setup({ transforms: [newMoreTransform("merge")], queries: 1 });
  assert.equal(byId(one.root, "transform-0-note").hidden, false);
  assert.equal(byId(one.root, "transform-0-note").textContent, "Merge needs two queries or more.");
  assert.deepEqual(one.t(), { kind: "merge" });
});

test("reduce: the mode, the calculations and the rule of one calculation at least", () => {
  const f = setup({ transforms: [newMoreTransform("reduce")] });
  choose(f.root, "transform-0-mode", "reduce_fields");
  assert.equal(f.t().mode, "reduce_fields");
  const box = (calc) => f.root.querySelectorAll("input").find((n) => n.dataset.value === calc);
  assert.equal(box("max").checked, true);
  assert.equal(box("min").checked, false);
  box("min").checked = true;
  box("min").dispatch("change");
  assert.deepEqual(f.t().reducers, ["max", "min"]);
  box("max").checked = false;
  box("max").dispatch("change");
  box("min").checked = false;
  box("min").dispatch("change");
  assert.deepEqual(f.t().reducers, ["min"]);
  assert.match(errors(f.root).join("|"), /Choose one calculation/);
});

test("partition_by_values: 1 to 4 fields and keep fields", () => {
  const f = setup({ transforms: [newMoreTransform("partition_by_values", "a")] });
  assert.equal(byId(f.root, "transform-0-fields-0-remove").getAttribute("aria-disabled"), "true");
  click(f.root, "transform-0-fields-add");
  assert.deepEqual(f.t().fields, ["a", "b"]);
  type(f.scope, f.root, "transform-0-fields-1-name", "a");
  assert.deepEqual(f.t().fields, ["a", "b"]);
  assert.match(errors(f.root).join("|"), /twice/);
  click(f.root, "transform-0-fields-add");
  click(f.root, "transform-0-fields-add");
  click(f.root, "transform-0-fields-add");
  assert.equal(f.t().fields.length, 4);
  assert.equal(new Set(f.t().fields).size, 4);
  assert.equal(byId(f.root, "transform-0-fields-add").getAttribute("aria-disabled"), "true");
  click(f.root, "transform-0-keep-fields");
  assert.equal(f.t().keep_fields, true);
  assert.deepEqual(moreFaults(f.t()), []);
});

test("join_by_field: Time is null, a name, and the mode", () => {
  const f = setup({ transforms: [newMoreTransform("join_by_field")] });
  assert.equal(byId(f.root, "transform-0-field").value, "");
  choose(f.root, "transform-0-field", "b");
  assert.equal(f.t().field, "b");
  choose(f.root, "transform-0-field", "");
  assert.equal(f.t().field, null);
  choose(f.root, "transform-0-mode", "inner");
  assert.equal(f.t().mode, "inner");
});

test("filter_fields_by_name: names and a pattern for each side, and a pattern that 4.24 refuses", () => {
  const f = setup({ transforms: [newMoreTransform("filter_fields_by_name")] });
  click(f.root, "transform-0-include-names-add");
  assert.deepEqual(f.t().include, { names: ["a"], pattern: null });
  type(f.scope, f.root, "transform-0-include-pattern", "x|y");
  assert.equal(f.t().include.pattern, "x|y");
  type(f.scope, f.root, "transform-0-exclude-pattern", "(");
  assert.equal(f.t().exclude.pattern, null);
  assert.match(errors(f.root).join("|"), /pattern is not valid/);
  type(f.scope, f.root, "transform-0-exclude-pattern", "/^tmp_/i");
  assert.equal(f.t().exclude.pattern, "/^tmp_/i");
  type(f.scope, f.root, "transform-0-include-pattern", "");
  assert.equal(f.t().include.pattern, null);
  assert.deepEqual(moreFaults(f.t()), []);
});

test("rename_by_regex: Match and Replace", () => {
  const f = setup({ transforms: [newMoreTransform("rename_by_regex")] });
  type(f.scope, f.root, "transform-0-regex", "(");
  assert.equal(f.t().regex, "(.*)");
  assert.match(errors(f.root).join("|"), /pattern is not valid/);
  type(f.scope, f.root, "transform-0-regex", "(.*)_total");
  assert.equal(f.t().regex, "(.*)_total");
  type(f.scope, f.root, "transform-0-rename-pattern", "<b>$1</b>");
  assert.equal(f.t().rename_pattern, "<b>$1</b>");
  type(f.scope, f.root, "transform-0-rename-pattern", "x".repeat(257));
  assert.equal(f.t().rename_pattern, "<b>$1</b>");
});

test("convert_field_type: 1 to 16 rows, a type for each, no field twice", () => {
  const f = setup({ transforms: [newMoreTransform("convert_field_type", "a")] });
  click(f.root, "transform-0-conversions-add");
  assert.deepEqual(f.t().conversions, [{ field: "a", to: "text" }, { field: "b", to: "text" }]);
  choose(f.root, "transform-0-conversions-1-to", "number");
  assert.equal(f.t().conversions[1].to, "number");
  type(f.scope, f.root, "transform-0-conversions-1-field", "a");
  assert.equal(f.t().conversions[1].field, "b");
  assert.match(errors(f.root).join("|"), /twice/);
  for (let i = 0; i < 20; i++) click(f.root, "transform-0-conversions-add");
  assert.equal(f.t().conversions.length, 16);
  assert.equal(new Set(f.t().conversions.map((c) => c.field)).size, 16);
  assert.deepEqual(moreFaults(f.t()), []);
});

test("the list moves and removes transforms of the new kinds", () => {
  const f = setup({ transforms: [newMoreTransform("limit"), newMoreTransform("merge"), newMoreTransform("organize")] });
  click(f.root, "transform-2-up");
  assert.deepEqual(f.state.draft.map((t) => t.kind), ["limit", "organize", "merge"]);
  click(f.root, "transform-0-remove");
  assert.deepEqual(f.state.draft.map((t) => t.kind), ["organize", "merge"]);
});

test("a hostile field name in the data stays text in the list of names", () => {
  const hostile = "<img src=x onerror=alert(1)>";
  const f = setup({ transforms: [newMoreTransform("organize")], hints: [hostile] });
  const title = f.root.querySelectorAll(".fitem__title").find((n) => n.textContent === hostile);
  assert.ok(title);
  assert.equal(title.children.length, 0);
});

test("a hostile field name in the data is an option of the list of suggestions, set as a value and never as markup", () => {
  const hostile = "<img src=x onerror=alert(1)>";
  const f = setup({ transforms: [newMoreTransform("sort_by", hostile)], hints: [hostile] });
  assert.ok(f.root.querySelectorAll("option").some((o) => o.value === hostile));
  assert.equal(byId(f.root, "transform-0-sort-0-field").value, hostile);
  assert.equal(f.t().sort[0].field, hostile);
});
