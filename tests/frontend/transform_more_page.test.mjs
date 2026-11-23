// SPDX-License-Identifier: MIT OR Apache-2.0
// The 11 more transforms in the page pipeline (9.21a, 9.22a rule 2, 9.23): the list of frames, the refs, the select of a table,
// the current name for the two older kinds, and the name of a field after the configuration (9.13, 9.14).
import test from "node:test";
import assert from "node:assert/strict";
import { applyList, preparePanelFrames } from "../../web/js/transform.js";
import { computeFieldConfig, matchesField } from "../../web/js/fields.js";
import { baseDisplayNames, displayNames } from "../../web/js/names.js";
import { frameOf } from "./helpers.mjs";
// The session gives the cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
import "./session_cells.mjs";

const q = (ref, extra = {}) => ({ source: "s", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null, ref, ...extra });
const panel = (type, transforms, queries = ["A", "B"]) => ({ id: 1, type, data: { query: q(queries[0]), ...(queries.length > 1 ? { extra_queries: queries.slice(1).map((r) => q(r)) } : {}) }, transforms, fields: { defaults: {}, overrides: [] }, options: {} });
const frameA = frameOf([{ name: "host", kind: "text", values: ["a", "b"] }, { name: "x", kind: "number", values: [1, 2] }]);
const frameB = frameOf([{ name: "host", kind: "text", values: ["b", "c"] }, { name: "y", kind: "number", values: [20, 30] }]);
const items = [{ ref: "A", frame: frameA }, { ref: "B", frame: frameB }];
const cells = (fields) => fields.map((f) => Array.from(f.values));

test("merge reads the list of frames before the combination of 9.22a: one frame, no select", () => {
  const made = preparePanelFrames(panel("table", [{ kind: "merge" }]), items);
  assert.equal(made.lists.length, 1);
  assert.deepEqual(baseDisplayNames(made.fields), ["host", "x", "y"]);
  assert.deepEqual(made.fields.map((f) => f.ref), [undefined, undefined, undefined]);
  assert.deepEqual(made.fields[0].values, ["a", "b", "c"]);
});

test("join_by_field gives one frame; the fields keep their prefix and the key field has none", () => {
  const ts = (name, ref) => frameOf([{ name: "t", kind: "time", values: [0, 60] }, { name, kind: "number", values: [1, 2] }]);
  const made = preparePanelFrames(panel("timeseries", [{ kind: "join_by_field", field: null, mode: "outer" }]), [{ ref: "A", frame: ts("a") }, { ref: "B", frame: ts("b") }]);
  assert.equal(made.lists.length, 1);
  assert.deepEqual(displayNames(made.fields), ["t", "A a", "B b"]);
  assert.equal(made.fields[0].prefix, undefined);
});

test("a frame transform maps each frame alone and keeps the refs of the lists; with two queries a name has its prefix", () => {
  const made = preparePanelFrames(panel("table", [{ kind: "limit", count: 1, end: "first" }, { kind: "reduce", mode: "series_to_rows", reducers: ["max"] }]), items);
  assert.deepEqual(made.lists.map((l) => l.ref), ["A", "B"]);
  assert.deepEqual(made.lists.map((l) => l.fields[0].values), [["A host", "A x"], ["B host", "B y"]]);
  assert.deepEqual(made.lists.map((l) => l.fields[1].values.length), [2, 2]);
  // The name `x` finds no field of a panel with two queries: the current name is `A x` (9.21a, rule 1). The transform does nothing.
  const none = preparePanelFrames(panel("table", [{ kind: "sort_by", sort: [{ field: "x", desc: true }] }]), items);
  assert.deepEqual(none.lists.map((l) => cells(l.fields)), [[["a", "b"], [1, 2]], [["b", "c"], [20, 30]]]);
  const found = preparePanelFrames(panel("table", [{ kind: "sort_by", sort: [{ field: "A x", desc: true }] }]), items);
  assert.deepEqual(cells(found.lists[0].fields), [["b", "a"], [2, 1]]);
});

test("partition_by_values: each new frame is an option of the select of a table with the text <ref> <label value>", () => {
  const byHost = (keys) => panel("table", [{ kind: "partition_by_values", fields: keys, keep_fields: false }]);
  const made = preparePanelFrames(byHost(["A host", "B host"]), items);
  assert.deepEqual(made.lists.map((l) => l.ref), ["A a", "A b", "B b", "B c"]);
  // The choice is the text. A choice that is in the list stays.
  assert.deepEqual(cells(preparePanelFrames(byHost(["A host", "B host"]), items, null, "B c").fields), [[30]]);
  // A panel with one query: the ref of the query is the first word (A when the query has none).
  const one = preparePanelFrames(panel("table", [{ kind: "partition_by_values", fields: ["host"], keep_fields: false }], ["A"]), [{ ref: null, frame: frameA }]);
  assert.deepEqual(one.lists.map((l) => l.ref), ["A a", "A b"]);
});

test("a time series draws one series for each partition frame (the lists join on time)", () => {
  const frame = frameOf([{ name: "t", kind: "time", values: [0, 0, 60] }, { name: "h", kind: "text", values: ["a", "b", "a"] }, { name: "v", kind: "number", values: [1, 2, 3] }]);
  const made = preparePanelFrames(panel("timeseries", [{ kind: "partition_by_values", fields: ["h"], keep_fields: false }], ["A"]), [{ ref: null, frame }]);
  assert.equal(made.lists.length, 2);
  assert.equal(made.fields.length, 3);
});

test("a reuse panel applies the transforms of its source, then its own, to the list of frames (rule 16)", () => {
  const source = panel("table", [{ kind: "organize", hide: [], order: [], rename: [{ field: "x", name: "X" }] }], ["A"]);
  const reuse = { ...panel("table", [{ kind: "sort_by", sort: [{ field: "X", desc: true }] }], ["A"]), data: { reuse: { panel: 1, with_transforms: true } } };
  const made = preparePanelFrames(reuse, [{ ref: null, frame: frameA }], source);
  assert.deepEqual(made.lists[0].fields.map((f) => f.config.display_name), [undefined, "X"]);
  assert.deepEqual(Array.from(made.lists[0].fields[1].values), [2, 1]);
});

test("a kind that the page does not know is an error of the panel, never a silent skip", () => {
  assert.throws(() => preparePanelFrames(panel("table", [{ kind: "future_kind" }]), items), /The transform future_kind is not available in this version of the page\./);
  assert.throws(() => applyList([{ ref: null, fields: [] }], [{ kind: "nope" }]), /not available/);
});

test("filter_by_value and grouping_to_matrix find a field by its current name (9.21a, rule 1)", () => {
  const rename = { kind: "organize", hide: [], order: [], rename: [{ field: "x", name: "X" }] };
  const filter = (field) => ({ kind: "filter_by_value", mode: "include", match: "all", filters: [{ field, matcher: { kind: "greater", value: 1 } }] });
  const p = (f) => panel("table", [rename, filter(f)], ["A"]);
  assert.deepEqual(preparePanelFrames(p("X"), [{ ref: null, frame: frameA }]).fields[1].values, Float64Array.from([2]));
  // The old name finds no field: the filter does not remove a row that it cannot test (9.21).
  assert.equal(preparePanelFrames(p("x"), [{ ref: null, frame: frameA }]).fields[1].values.length, 0);
});

test("the name of a field after the configuration: a transform name stays, a defaults or override name replaces it (9.13)", () => {
  const made = preparePanelFrames(panel("table", [{ kind: "organize", hide: [], order: [], rename: [{ field: "x", name: "X" }] }], ["A"]), [{ ref: null, frame: frameA }]);
  const kept = computeFieldConfig(made.fields, { defaults: {}, overrides: [] }, "table").fields;
  assert.deepEqual(displayNames(kept), ["host", "X"]);
  const replaced = computeFieldConfig(made.fields, { defaults: {}, overrides: [{ match: { by: "name", value: "X" }, set: { display_name: "Renamed" } }] }, "table").fields;
  assert.deepEqual(displayNames(replaced), ["host", "Renamed"]);
});

test("a matcher name finds a renamed field by its new name and by its old name (9.14)", () => {
  const made = preparePanelFrames(panel("table", [{ kind: "organize", hide: [], order: [], rename: [{ field: "x", name: "X" }] }], ["A"]), [{ ref: null, frame: frameA }]);
  const names = baseDisplayNames(made.fields);
  const find = (value) => made.fields.filter((f, i) => matchesField({ by: "name", value }, f, names[i])).map((f) => f.name);
  assert.deepEqual(find("X"), ["x"]);
  assert.deepEqual(find("x"), ["x"]);
  assert.deepEqual(find("nope"), []);
});

test("a hostile rename stays text through the pipeline", () => {
  const name = "<img src=x onerror=alert(1)>";
  const made = preparePanelFrames(panel("table", [{ kind: "organize", hide: [], order: [], rename: [{ field: "x", name }] }], ["A"]), [{ ref: null, frame: frameA }]);
  assert.deepEqual(displayNames(made.fields), ["host", name]);
});
