// FE-13: grouping_to_matrix (9.21).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { groupingToMatrix } from "../../web/js/transform.js";
import { grafanaValue } from "../../web/js/frame.js";
import { baseDisplayNames } from "../../web/js/names.js";
import { toFields } from "./helpers.mjs";
// The session gives the cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
import { withCells } from "./session_cells.mjs";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-13-matrix.json", import.meta.url)));
const column = (f) => Array.from({ length: f.values.length }, (_, i) => grafanaValue(f, i));
const asObject = (fields) => Object.fromEntries(fields.map((f) => [f.name, column(f)]));

test("FE-13 worked example and empty values", () => {
  for (const c of v.cases) {
    const input = toFields(v.cols);
    const out = groupingToMatrix(input, c.spec);
    if (c.unchanged) assert.equal(out, input, c.name);
    else assert.deepEqual(asObject(out), c.expect, c.name);
  }
});

test("FE-13 the last row of a cell wins, keys are distinct values", () => {
  const out = groupingToMatrix(toFields(v.last.cols), v.last.spec);
  assert.deepEqual(asObject(out), v.last.expect);
});

test("FE-13 SameValueZero for number keys (NaN equals NaN)", () => {
  const out = groupingToMatrix(toFields(v.numeric.cols), v.numeric.spec);
  const o = asObject(out);
  assert.equal(o["r\\c"][0], 1);
  assert.ok(Number.isNaN(o["r\\c"][1]));
  assert.deepEqual(o["0"], v.numeric.expect["0"]);
});

test("FE-13 the first column name and kinds (9.13)", () => {
  const out = groupingToMatrix(toFields(v.cols), v.cases[0].spec);
  assert.deepEqual(baseDisplayNames(out), ["host\\metric", "cpu", "mem"]);
  assert.equal(out[0].kind, "text");
  assert.equal(out[1].kind, "number");
});

// Review v3 (finding N4, 2026-11-16): grouping_to_matrix checks the caps of 9.21a, rule 13, before it builds a field. 4,000 distinct
// column keys and 4,000 distinct row keys made 16,000,000 cells in 6.5 s.
test("FE-13 the caps: 4,001 fields or 10,006,000 cells are an error before a field is built", () => {
  const frame = (R, cols) => [
    { name: "row", type: "UInt32", kind: "number", values: Float64Array.from({ length: R }, (_, i) => i), nulls: new Uint8Array(R), labels: null, config: {} },
    { name: "col", type: "String", kind: "text", values: Array.from({ length: R }, (_, i) => `c${i % cols}`), labels: null, config: {} },
    { name: "v", type: "Float64", kind: "number", values: new Float64Array(R).fill(1), nulls: new Uint8Array(R), labels: null, config: {} },
  ];
  const spec = { column_field: "col", row_field: "row", value_field: "v", empty_value: "null" };
  const t0 = performance.now();
  assert.throws(() => groupingToMatrix(frame(4000, 4000), spec), { message: "The transform grouping_to_matrix failed: 4001 fields, 2,000 at most" });
  assert.ok(performance.now() - t0 < 300, "the error comes before the work");
  // 1,999 column keys and 5,003 row keys: 2,000 fields x 5,003 rows.
  assert.throws(() => groupingToMatrix(frame(5003, 1999), spec), { message: "The transform grouping_to_matrix failed: 10006000 cells, 10,000,000 at most" });
  assert.equal(groupingToMatrix(frame(4000, 1999), spec).length, 2000);
  // The cap of the cells is the value of the session (6.10, `server.transform_max_cells`). A configured value applies as it is:
  // 49 column keys give 50 fields, so 100 rows are 5,000 cells and 101 rows are 5,050 cells.
  withCells(5000, () => {
    assert.equal(groupingToMatrix(frame(100, 49), spec).length, 50);
    assert.throws(() => groupingToMatrix(frame(101, 49), spec), { message: "The transform grouping_to_matrix failed: 5050 cells, 5,000 at most" });
  });
  // With no valid value in the session, the step fails closed.
  for (const bad of [undefined, 0, "5000"]) withCells(bad, () => assert.throws(() => groupingToMatrix(frame(2, 2), spec), { message: "The transform grouping_to_matrix failed: the session has no valid transform_max_cells" }));
});
