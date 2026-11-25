// SPDX-License-Identifier: MIT OR Apache-2.0
// The chips of the selected values of a variable (9.10): the model, the removal rules and the fit in a row.
import test from "node:test";
import assert from "node:assert/strict";
import { chipsOf, fitCount, withoutLast, withoutValue } from "../../web/js/var_chips.js";

const multi = { name: "m", kind: "query", multi: true, include_all: true };
const options = [
  { value: "a", text: "Alpha" },
  { value: "b", text: "Beta" },
  { value: "c", text: "Gamma" },
];

test("a variable with one value keeps its text: no chips", () => {
  assert.equal(chipsOf({ ...multi, multi: false }, options, { all: false, values: ["a"] }), null);
  assert.equal(chipsOf({ name: "t", kind: "text" }, [], { all: false, values: ["x"] }), null);
  assert.equal(chipsOf({ name: "i", kind: "interval", multi: false }, [], { all: false, values: ["1m"] }), null);
});

test("All is one chip with no cross", () => {
  assert.deepEqual(chipsOf(multi, options, { all: true, values: [] }), [{ value: "$__all", text: "All", all: true, removable: false }]);
});

test("each value is a chip with the text of its option, and a value that is not an option shows as it is", () => {
  const chips = chipsOf(multi, options, { all: false, values: ["a", "c", "typed"] });
  assert.deepEqual(chips.map((c) => c.text), ["Alpha", "Gamma", "typed"]);
  assert.ok(chips.every((c) => c.removable && !c.all));
});

test("the last value has no cross: a selection is never empty", () => {
  const chips = chipsOf(multi, options, { all: false, values: ["b"] });
  assert.equal(chips.length, 1);
  assert.equal(chips[0].removable, false);
});

test("the first option wins when two options have one value", () => {
  const dup = [{ value: "a", text: "First" }, { value: "a", text: "Second" }];
  assert.equal(chipsOf(multi, dup, { all: false, values: ["a", "b"] })[0].text, "First");
});

test("removing a value keeps the others in order, and refuses the last value", () => {
  assert.deepEqual(withoutValue({ all: false, values: ["a", "b", "c"] }, "b"), { all: false, values: ["a", "c"] });
  const one = { all: false, values: ["a"] };
  assert.equal(withoutValue(one, "a"), one, "the last value stays");
  const all = { all: true, values: [] };
  assert.equal(withoutValue(all, "a"), all, "All has no value to remove");
  const two = { all: false, values: ["a", "b"] };
  assert.equal(withoutValue(two, "zzz"), two, "an unknown value changes nothing");
});

test("Backspace removes the last chip and stops at the first one", () => {
  assert.deepEqual(withoutLast({ all: false, values: ["a", "b", "c"] }), { all: false, values: ["a", "b"] });
  const one = { all: false, values: ["a"] };
  assert.equal(withoutLast(one), one);
});

test("every chip shows when the row is wide enough", () => {
  assert.equal(fitCount([40, 40, 40], 4, 200, () => 30), 3);
  assert.equal(fitCount([], 4, 100, () => 30), 0);
});

test("the chips that do not fit give a plus chip, and the plus chip takes its own room", () => {
  // 3 chips of 60: the row is 150 wide. Two chips and the plus chip of 30 need 60 + 4 + 60 + 4 + 30 = 158: too much. One chip and the plus chip: 60 + 4 + 30 = 94.
  assert.equal(fitCount([60, 60, 60], 4, 150, () => 30), 1);
  // A row of 160 shows two chips and the plus chip.
  assert.equal(fitCount([60, 60, 60], 4, 160, () => 30), 2);
  // The width of the plus chip can depend on the count of hidden chips.
  assert.equal(fitCount([60, 60, 60, 60], 4, 160, (n) => (n > 9 ? 40 : 30)), 2);
});

test("at least one chip shows, even when it is wider than the row", () => {
  assert.equal(fitCount([300, 40], 4, 100, () => 30), 1);
});
