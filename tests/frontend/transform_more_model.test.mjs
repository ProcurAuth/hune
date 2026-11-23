// SPDX-License-Identifier: MIT OR Apache-2.0
// The rules of the forms of the eleven more transforms (10.3a, 4.25a): new transforms, the checks of one control and moreFaults.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { MORE_KIND_TEXT, MORE_LIMITS, calcOfMode, moreFaults, newMoreTransform, nameError, operandsError, patternError } from "../../web/js/editor/forms/transform_more_model.js";
import { applyMore } from "../../web/js/transform_more.js";
import { CALCS } from "../../web/js/reduce.js";
import { framesOf } from "./more_helpers.mjs";

const dir = new URL("../fixtures/dashboards/", import.meta.url);
const load = (path) => JSON.parse(readFileSync(new URL(path, dir), "utf8"));
const transformsOf = (doc) => doc.sections[0].panels[0].transforms;
const keys = (o) => Object.keys(o);

test("the 11 kinds are the kinds of the module, in the order of 4.25a", () => {
  assert.deepEqual(MORE_KIND_TEXT.map(([k]) => k), ["organize", "calculate_field", "sort_by", "limit", "merge", "reduce", "partition_by_values", "join_by_field", "filter_fields_by_name", "rename_by_regex", "convert_field_type"]);
});

test("a new transform has the members of the valid fixture in the canonical order, and no fault", () => {
  const fixture = transformsOf(load("transforms_more.json"));
  for (const [kind] of MORE_KIND_TEXT) {
    const made = newMoreTransform(kind, "a");
    const want = fixture.find((t) => t.kind === kind);
    // The fixture has a binary calculation: a new one has the mode `index`, so its object is `index`.
    assert.deepEqual(keys(made), kind === "calculate_field" ? ["kind", "mode", "alias", "replace_fields", "index"] : keys(want), kind);
    assert.deepEqual(moreFaults(made), [], kind);
  }
  assert.deepEqual(newMoreTransform("organize"), { kind: "organize", hide: [], order: [], rename: [] });
  assert.deepEqual(newMoreTransform("limit"), { kind: "limit", count: 100, end: "first" });
  assert.deepEqual(newMoreTransform("reduce"), { kind: "reduce", mode: "series_to_rows", reducers: ["max"] });
  assert.deepEqual(newMoreTransform("join_by_field"), { kind: "join_by_field", field: null, mode: "outer" });
  assert.deepEqual(newMoreTransform("rename_by_regex"), { kind: "rename_by_regex", regex: "(.*)", rename_pattern: "$1" });
  assert.deepEqual(newMoreTransform("convert_field_type", "n"), { kind: "convert_field_type", conversions: [{ field: "n", to: "text" }] });
  assert.deepEqual(newMoreTransform("merge"), { kind: "merge" });
  assert.equal(newMoreTransform("calculate_field").mode, "index");
});

test("a new transform changes no data", () => {
  const frames = () => framesOf([[{ name: "a", kind: "text", values: ["x", "y"] }, { name: "b", kind: "number", values: [1, 2] }]]);
  const plain = (fs) => JSON.stringify(fs.map((f) => f.map((x) => [x.name, Array.from(x.values)])));
  for (const [kind] of MORE_KIND_TEXT) {
    if (kind === "calculate_field" || kind === "reduce" || kind === "partition_by_values" || kind === "convert_field_type") continue;
    const before = frames();
    assert.equal(plain(applyMore(before, newMoreTransform(kind, "a"))), plain(frames()), kind);
  }
  // A new `calculate_field` adds the field Row: the other fields stay. A new `reduce` and a new `partition_by_values` are the first choice of the person.
  const calc = applyMore(frames(), newMoreTransform("calculate_field", "a"))[0];
  assert.deepEqual(calc.map((f) => f.name), ["a", "b", "Row"]);
});

test("calcOfMode keeps the alias and replace_fields, and puts the object of the mode last", () => {
  const before = { kind: "calculate_field", mode: "index", alias: "X", replace_fields: true, index: { as_percentile: false } };
  for (const [mode, key] of [["binary", "binary"], ["reduce_row", "reduce"], ["unary", "unary"], ["index", "index"]]) {
    const next = calcOfMode(before, mode, "f");
    assert.deepEqual(keys(next), ["kind", "mode", "alias", "replace_fields", key]);
    assert.equal(next.alias, "X");
    assert.equal(next.replace_fields, true);
    assert.deepEqual(moreFaults(next), []);
  }
  assert.deepEqual(keys(calcOfMode(null, "binary", "f").binary), ["left", "right", "operator"]);
});

test("the reducer list of the form is the list of 4.26 with the titles of 9.21a", () => {
  assert.equal(CALCS.length, MORE_LIMITS.reducers);
});

test("the checks of one control", () => {
  assert.equal(nameError(["a", "b"], 1), "");
  assert.match(nameError(["a", "a"], 1), /twice/);
  assert.match(nameError(["a", ""], 1), /1 to 1024 bytes/);
  assert.match(nameError(["a".repeat(1025)], 0), /1 to 1024 bytes/);
  assert.equal(nameError(["é".repeat(512)], 0), "");
  assert.match(operandsError({ left: { kind: "number", value: 1 }, right: { kind: "number", value: 2 } }), /field/);
  assert.equal(operandsError({ left: { kind: "field", field: "a" }, right: { kind: "number", value: 2 } }), "");
  assert.equal(patternError("", true), "");
  assert.equal(patternError("x|y"), "");
  assert.equal(patternError("/^x/i"), "");
  assert.match(patternError("("), /not valid/);
  assert.match(patternError(""), /not valid/);
});

// The faults that moreFaults reports for the invalid fixtures that a form or the JSON tab can reach: the code and the pointer of the server (4.37).
const FAULTS = [
  ["organize_hide_65_items", "too_large", "transforms/0/hide"], ["organize_hide_duplicate", "not_unique", "transforms/0/hide/1"],
  ["organize_order_duplicate", "not_unique", "transforms/0/order/2"], ["organize_rename_same_field", "not_unique", "transforms/0/rename/1"],
  ["organize_rename_name_empty", "out_of_range", "transforms/0/rename/0/name"], ["calc_alias_empty", "out_of_range", "transforms/1/alias"],
  ["calc_alias_257_chars", "out_of_range", "transforms/1/alias"], ["calc_binary_two_numbers", "not_permitted", "transforms/1/binary"],
  ["calc_reduce_include_duplicate", "not_unique", "transforms/1/reduce/include/1"], ["sort_empty", "out_of_range", "transforms/2/sort"],
  ["sort_9_keys", "too_large", "transforms/2/sort"], ["limit_count_minus_1", "out_of_range", "transforms/3/count"],
  ["limit_count_1000001", "out_of_range", "transforms/3/count"], ["reduce_19_reducers", "too_large", "transforms/5/reducers"],
  ["reduce_reducers_duplicate", "not_unique", "transforms/5/reducers/1"], ["partition_fields_empty", "out_of_range", "transforms/6/fields"],
  ["partition_5_fields", "too_large", "transforms/6/fields"], ["partition_fields_duplicate", "not_unique", "transforms/6/fields/1"],
  ["filter_include_names_duplicate", "not_unique", "transforms/8/include/names/1"], ["filter_include_pattern_bad", "bad_format", "transforms/8/include/pattern"],
  ["rename_regex_bad", "bad_format", "transforms/9/regex"], ["rename_pattern_257_chars", "out_of_range", "transforms/9/rename_pattern"],
  ["convert_17_conversions", "too_large", "transforms/10/conversions"], ["convert_duplicate_field", "not_unique", "transforms/10/conversions/1"],
];

test("moreFaults gives the code and the pointer of the server for the invalid fixtures f28_*", () => {
  for (const [name, code, pointer] of FAULTS) {
    const transforms = transformsOf(load(`invalid/f28_${name}.json`));
    const faults = transforms.flatMap((t, i) => moreFaults(t, `/sections/0/panels/0/transforms/${i}`));
    const want = `/sections/0/panels/0/${pointer}`;
    assert.ok(faults.some((f) => f.code === code && f.pointer === want), `${name}: ${JSON.stringify(faults)}`);
  }
});

test("moreFaults gives no fault for the valid fixture", () => {
  const transforms = transformsOf(load("transforms_more.json"));
  assert.equal(transforms.length, 11);
  for (const t of transforms) assert.deepEqual(moreFaults(t), [], t.kind);
});
