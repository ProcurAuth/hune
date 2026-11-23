// SPDX-License-Identifier: MIT OR Apache-2.0
// The queries of a panel in the draft (10.3): add, delete, rename, hide, and the dialogs' rules (4.15a).
import test from "node:test";
import assert from "node:assert/strict";
import { changeType, panelRepeatRefusal, reuseFits, sectionRepeatRefusal } from "../../web/js/editor/draft.js";
import { addQuery, addRefusal, deleteQuery, dropExtra, freeRef, hideQuery, matchersOf, refError, renameQuery, setQueryAt } from "../../web/js/editor/query_ops.js";
import { queriesOf } from "../../web/js/queries.js";

const one = () => ({
  id: 1, type: "timeseries", title: "t",
  data: { query: { source: "prod", format: "time_series", sql: ["SELECT 1"], min_interval: "1m", max_data_points: null, time_from: "now-1h" } },
  transforms: [], fields: { defaults: {}, overrides: [{ match: { by: "query", value: "B" }, set: { unit: "ms" } }] }, options: {},
});
const three = () => addQuery(addQuery(one(), 0), 0);
const refs = (p) => queriesOf(p).map((q) => q.ref);

test("Add query: the first free ref, the source and the format of the selected query, an empty SQL line", () => {
  const p = addQuery(one(), 0);
  assert.deepEqual(refs(p), ["A", "B"]);
  assert.deepEqual(p.data.extra_queries, [{ ref: "B", source: "prod", format: "time_series", sql: [""] }]);
  assert.deepEqual(refs(three()), ["A", "B", "C"]);
  // A ref that a query uses is not free: the first query can be C.
  assert.equal(freeRef(three()), "D");
  const gap = deleteQuery(three(), 1);
  assert.equal(freeRef(gap), "B");
});

test("Add query: the 26 letters then AA to AZ, and the limit of 8", () => {
  let p = one();
  for (let i = 0; i < 7; i++) p = addQuery(p, 0);
  assert.deepEqual(refs(p), ["A", "B", "C", "D", "E", "F", "G", "H"]);
  assert.equal(addRefusal(p, false), "A panel has 8 queries at most.");
  assert.equal(addRefusal(one(), true), "A panel with a repeat has one query.");
  assert.match(addRefusal({ ...one(), type: "barchart" }, false), /draw one query/);
  assert.equal(addRefusal(one(), false), "");
});

test("the members of the panel stay in the first query, and the canonical order puts ref and hide last", () => {
  const p = hideQuery(setQueryAt(three(), 0, { sql: ["SELECT 2"] }), 0, true);
  assert.deepEqual(Object.keys(p.data.query), ["source", "format", "sql", "min_interval", "max_data_points", "time_from", "hide"]);
  assert.deepEqual(Object.keys(p.data.extra_queries[0]), ["ref", "source", "format", "sql"]);
  const q = hideQuery(three(), 1, true);
  assert.deepEqual(Object.keys(q.data.extra_queries[0]), ["ref", "source", "format", "sql", "hide"]);
  assert.equal("hide" in hideQuery(q, 1, false).data.extra_queries[0], false, "a shown query has no hide member");
});

test("Delete: the next query becomes the first one with the members of the panel; the last extra query removes extra_queries; the last query stays", () => {
  const p = deleteQuery(three(), 0);
  assert.deepEqual(refs(p), ["B", "C"]);
  assert.deepEqual(p.data.query, { source: "prod", format: "time_series", sql: [""], min_interval: "1m", max_data_points: null, time_from: "now-1h", ref: "B" });
  const two = deleteQuery(addQuery(one(), 0), 1);
  assert.equal("extra_queries" in two.data, false);
  assert.deepEqual(two, one());
  assert.deepEqual(deleteQuery(one(), 0), one(), "the last query cannot be deleted");
});

test("Rename: the grammar, the duplicate, and the matchers query that follow", () => {
  const p = addQuery(one(), 0);
  assert.match(refError(p, 1, "1x"), /starts with a letter/);
  assert.match(refError(p, 1, "x".repeat(17)), /16 characters at most/);
  assert.equal(refError(p, 1, "A"), "Another query has this name.");
  assert.equal(refError(p, 1, "B"), "");
  assert.deepEqual(matchersOf(p, "B"), [{ index: 0 }]);
  const r = renameQuery(p, 1, "Zed");
  assert.deepEqual(refs(r), ["A", "Zed"]);
  assert.equal(r.fields.overrides[0].match.value, "Zed");
  // The first query: `A` writes no member, another name does.
  assert.equal("ref" in renameQuery(r, 0, "A").data.query, false);
  assert.equal(renameQuery(r, 0, "Q").data.query.ref, "Q");
});

test("a change to barchart or xychart removes the extra queries; the repeat and reuse rules", () => {
  const p = three();
  assert.equal(changeType(p, "barchart").data.extra_queries, undefined);
  assert.equal(changeType(p, "xychart").data.extra_queries, undefined);
  assert.ok(changeType(p, "stat").data.extra_queries);
  assert.equal("extra_queries" in dropExtra(p).data, false);
  assert.equal(reuseFits("barchart", p), false);
  assert.equal(reuseFits("table", p), true);
  assert.equal(reuseFits("barchart", one()), true);
  const doc = { variables: [], sections: [{ title: "s", collapsed: false, panels: [p] }] };
  assert.match(panelRepeatRefusal(doc, 1), /several queries/);
  assert.match(sectionRepeatRefusal({ ...doc, variables: [] }, 0), /several queries/);
});

// A draft that is not valid for a moment (the JSON tab): a panel with neither `query` nor `reuse`, or with no `data`. Nothing throws.
import { DraftRuntime } from "../../web/js/editor/draft_runtime.js";
import { PANEL_TYPES } from "../../web/js/editor/draft.js";
import { slotsOf, visibleQueries } from "../../web/js/queries.js";

for (const type of PANEL_TYPES) {
  for (const data of [undefined, {}, { extra_queries: [] }]) {
    test(`a ${type} panel with data ${JSON.stringify(data)}: the query helpers and the draft runtime do not throw`, () => {
      const panel = { id: 7, type, title: "t", description: "", grid: { x: 0, y: 0, w: 12, h: 4 }, transforms: [], fields: { defaults: {}, overrides: [] }, options: { content: ["x"] }, links: [], ...(data === undefined ? {} : { data }) };
      assert.deepEqual(queriesOf(panel), []);
      assert.deepEqual(visibleQueries(panel), []);
      assert.doesNotThrow(() => addRefusal(panel, false));
      assert.doesNotThrow(() => slotsOf({ panel: { ...panel, data: panel.data || {} }, state: "idle", frame: null, error: null }));
      const base = { panels: new Map(), variables: [], range: { from_ms: 1, to_ms: 2 }, limits: undefined, on: () => () => {}, requestVariables: () => ({}) };
      const doc = { variables: [], sections: [{ title: null, collapsed: false, panels: [panel] }] };
      assert.doesNotThrow(() => new DraftRuntime({ base, doc, onCollapse: () => {} }));
    });
  }
}
