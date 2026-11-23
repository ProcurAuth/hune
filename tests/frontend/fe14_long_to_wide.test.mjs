// FE-14: long-to-wide (9.22).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { longToWide, preparePanelFields } from "../../web/js/transform.js";
import { grafanaValue } from "../../web/js/frame.js";
import { baseDisplayNames } from "../../web/js/names.js";
import { toFields } from "./helpers.mjs";
// The session gives the cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
import { withCells } from "./session_cells.mjs";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-14-long-to-wide.json", import.meta.url)));
const column = (f) => Array.from({ length: f.values.length }, (_, i) => grafanaValue(f, i));
const shape = (fields) => fields.map((f) => ({ name: f.name, labels: f.labels, values: column(f) }));

test("FE-14 worked example of 9.22 and the display names", () => {
  const out = longToWide(toFields(v.worked.cols));
  assert.deepEqual(shape(out), v.worked.expect);
  assert.deepEqual(baseDisplayNames(out), v.worked.names);
  assert.equal(out[0].kind, "time");
  assert.equal(out[1].kind, "number");
});

test("FE-14 null time left out, sorted, last row wins, null label is empty text", () => {
  assert.deepEqual(shape(longToWide(toFields(v.unsorted_null_time_duplicates.cols))), v.unsorted_null_time_duplicates.expect);
});

test("FE-14 the series are sorted by label values, whatever the order of the rows", () => {
  const cols = [
    { name: "t", kind: "time", values: [1, 1, 1, 1] },
    { name: "region", kind: "text", values: ["SIN", "AMS", "NYC", "FRA"] },
    { name: "zone", kind: "text", values: ["b", "a", "a", "a"] },
    { name: "v", kind: "number", values: [4, 1, 3, 2] },
  ];
  const out = longToWide(toFields(cols));
  assert.deepEqual(out.slice(1).map((f) => f.labels.region), ["AMS", "FRA", "NYC", "SIN"]);
  const two = longToWide(toFields([
    { name: "t", kind: "time", values: [1, 1, 1] },
    { name: "a", kind: "text", values: ["x", "x", "w"] },
    { name: "b", kind: "text", values: ["2", "1", "9"] },
    { name: "v", kind: "number", values: [1, 2, 3] },
  ]));
  assert.deepEqual(two.slice(1).map((f) => `${f.labels.a}${f.labels.b}`), ["w9", "x1", "x2"]);
});

test("FE-14 several value fields; no value field; no time column", () => {
  assert.deepEqual(shape(longToWide(toFields(v.two_values.cols))), v.two_values.expect);
  const noValue = toFields(v.no_value_field.cols);
  assert.equal(longToWide(noValue), noValue);
  assert.throws(() => longToWide(toFields(v.no_time.cols)), { message: v.no_time.error });
});

test("FE-14 applies only to format time_series with a text field", () => {
  const frame = { columns: v.worked.cols.map((c) => ({ name: c.name, kind: c.kind, type: c.kind })), values: v.worked.cols.map((c) => c.values), rows: 3 };
  const make = (format) => ({ data: { query: { format } }, transforms: [] });
  assert.equal(preparePanelFields(make("table"), frame).length, 3);
  assert.equal(preparePanelFields(make("time_series"), frame).length, 3);
  assert.equal(preparePanelFields(make("time_series"), frame)[1].labels.node, "a1");
  assert.equal(preparePanelFields(make("table"), frame)[1].labels, null);
  const noText = { columns: [{ name: "t", kind: "time", type: "x" }, { name: "v", kind: "number", type: "x" }], values: [[1], [2]], rows: 1 };
  assert.equal(preparePanelFields(make("time_series"), noText).length, 2);
});

// Review v3 (finding N3, 2026-11-16): long-to-wide checks the caps of 9.21a, rule 13, before it builds a field. A time series query with a
// text column of distinct values gave one field for each row: 8,000 rows made 8,001 fields x 8,000 rows (10.7 s, 576 MB).
test("FE-14 the caps: 8,001 fields or 10,011,001 cells are an error before a field is built", () => {
  const query = (R, hosts) => [
    { name: "time", type: "DateTime", kind: "time", values: Float64Array.from({ length: R }, (_, i) => i * 1000), nulls: new Uint8Array(R), labels: null, config: {} },
    { name: "host", type: "String", kind: "text", values: Array.from({ length: R }, (_, i) => `h${i % hosts}`), labels: null, config: {} },
    { name: "v", type: "Float64", kind: "number", values: new Float64Array(R).fill(1), nulls: new Uint8Array(R), labels: null, config: {} },
  ];
  const big = query(8000, 8000);
  const t0 = performance.now();
  assert.throws(() => longToWide(big), { message: "The long-to-wide failed: 8001 fields, 2,000 at most" });
  assert.ok(performance.now() - t0 < 300, "the error comes before the work");
  // 1,000 hosts and 10,001 times: 1,001 fields x 10,001 rows.
  assert.throws(() => longToWide(query(10001, 1000)), { message: "The long-to-wide failed: 10011001 cells, 10,000,000 at most" });
  // 2,000 fields: 1,999 hosts and the time, 3,998 times.
  assert.equal(longToWide(query(3998, 1999)).length, 2000);
  // The cap of the cells is the value of the session (6.10, `server.transform_max_cells`). A configured value applies as it is:
  // 49 hosts give 50 fields, so 100 times are 5,000 cells and 101 times are 5,050 cells.
  withCells(5000, () => {
    assert.equal(longToWide(query(100, 49)).length, 50);
    assert.throws(() => longToWide(query(101, 49)), { message: "The long-to-wide failed: 5050 cells, 5,000 at most" });
  });
  // With no valid value in the session, the step fails closed.
  for (const bad of [undefined, -1, 2.5]) withCells(bad, () => assert.throws(() => longToWide(query(2, 2)), { message: "The long-to-wide failed: the session has no valid transform_max_cells" }));
});
