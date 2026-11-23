// Tests of the pairing by ref of the parity capture (4.15a, 15.14c): a panel with several queries has one request
// of Hune for each visible query, and the capture pairs it with the query of Grafana that has the same refId.
import assert from "node:assert/strict";
import test from "node:test";
import { comparePanelQuery, pairByRef } from "../../tools/parity/capture-lib.mjs";

const record = (sql) => ({ sql, interval_ms: 60000, max_data_points: 1200, from_ms: 1, to_ms: 2, table: null });

test("a panel whose requests have a ref pairs each refId with its own record", () => {
  const a = record("SELECT 1");
  const b = record("SELECT 2");
  const panel = { ...a, explicit_ref: true, refs: new Map([["A", a], ["B", b]]) };
  assert.equal(pairByRef(panel, "A", 0), a);
  assert.equal(pairByRef(panel, "B", 1), b);
  assert.equal(pairByRef(panel, "C", 2), null, "Hune has no request for this refId");
});

test("the order of the queries does not decide: the ref does", () => {
  const z = record("SELECT z");
  const a = record("SELECT a");
  const panel = { ...z, explicit_ref: true, refs: new Map([["Z", z], ["A", a]]) };
  assert.equal(pairByRef(panel, "A", 1), a);
  assert.equal(pairByRef(panel, "Z", 0), z);
});

test("a panel with one query and no ref in its request pairs with the first query of Grafana", () => {
  const only = record("SELECT 1");
  const panel = { ...only, explicit_ref: false, refs: new Map([["A", only]]) };
  assert.equal(pairByRef(panel, "B", 0), panel, "the refId B of the one query of Grafana");
  assert.equal(pairByRef(panel, "B", 1), null, "a second query of Grafana has no query of Hune");
});

test("a missing panel gives null and a record of two queries compares each ref alone", () => {
  assert.equal(pairByRef(null, "A", 0), null);
  const g = { sql: "SELECT 1", interval_ms: 60000, max_data_points: 1200, from_ms: 1, to_ms: 2 };
  const v = { sql: "SELECT 1", interval_ms: 60000, max_data_points: 1200, from_ms: 1, to_ms: 2 };
  assert.equal(comparePanelQuery(g, v).sql.status, "equal");
  assert.notEqual(comparePanelQuery({ ...g, sql: "SELECT 2" }, v).sql.status, "equal");
  assert.equal(comparePanelQuery(g, null).sql.status, "different", "no query of Hune for the refId");
});
