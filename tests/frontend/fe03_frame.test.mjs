// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-03: frame to fields (9.12).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { framesToFields, grafanaValue, valueText, rowCount, selectRows } from "../../web/js/frame.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-03-frame.json", import.meta.url)));

test("FE-03 kinds of the worked frame", () => {
  const f = framesToFields(v.frame);
  assert.equal(f.length, 6);
  assert.ok(f[0].values instanceof Float64Array);
  assert.deepEqual(Array.from(f[0].values), [1794211200000, 1794211220500]);
  assert.deepEqual(f[1].values, ["a1", null]);
  assert.deepEqual(f[2].values, ["12.500000000", "3.000000000"]);
  assert.deepEqual(Array.from(f[2].num), [12.5, 3]);
  assert.deepEqual(Array.from(f[3].nulls), [0, 1]);
  assert.ok(Number.isNaN(f[3].values[1]));
  assert.deepEqual(f[4].values, [true, false]);
  assert.deepEqual(f[5].values, [null, null]);
  assert.deepEqual(f.map((x) => x.labels), [null, null, null, null, null, null]);
  assert.equal(rowCount(f), 2);
});

test("FE-03 Grafana values and text forms", () => {
  const f = framesToFields(v.frame);
  assert.equal(grafanaValue(f[0], 1), 1794211220500);
  assert.equal(grafanaValue(f[1], 1), null);
  assert.equal(grafanaValue(f[2], 0), 12.5);
  assert.equal(grafanaValue(f[3], 1), null);
  assert.equal(grafanaValue(f[4], 0), true);
  assert.equal(grafanaValue(f[5], 0), null);
  assert.equal(valueText(12.5), "12.5");
  assert.equal(valueText(null), "null");
});

test("FE-03 special numbers, exact texts, NULL", () => {
  const f = framesToFields(v.frame2);
  const g = (x, i) => grafanaValue(x, i);
  assert.ok(Number.isNaN(g(f[0], 0)));
  assert.equal(g(f[0], 1), Infinity);
  assert.equal(g(f[0], 2), -Infinity);
  assert.equal(g(f[0], 3), 1.5);
  assert.equal(g(f[0], 4), null);
  assert.equal(f[1].values[0], "9007199254740993"); // kept exact
  assert.equal(g(f[1], 0), 9007199254740992); // the double loses the last digit
  assert.equal(g(f[1], 2), null);
});

test("FE-03 selectRows keeps kinds and order", () => {
  const f = framesToFields(v.frame);
  const s = selectRows(f, [1, 0]);
  assert.deepEqual(Array.from(s[0].values), [1794211220500, 1794211200000]);
  assert.deepEqual(s[2].values, ["3.000000000", "12.500000000"]);
  assert.deepEqual(Array.from(s[3].nulls), [1, 0]);
  assert.equal(rowCount(selectRows(f, [])), 0);
});
