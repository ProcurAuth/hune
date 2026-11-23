// FE-38 to FE-40: the queries of a panel, the join of frames, the combination by type and the names (4.15a, 9.8d, 9.22a).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { framesToFields } from "../../web/js/frame.js";
import { matchesField } from "../../web/js/fields.js";
import { baseDisplayNames } from "../../web/js/names.js";
import { MAX_JOIN_ROWS, MAX_QUERIES_PER_PANEL, badgeText, combine, failedRefs, firstError, hasExtra, isWholeError, joinByTime, panelState, queriesOf, requestBody, slotsOf, tagFields, visibleQueries } from "../../web/js/queries.js";
import { applyList, preparePanelFrames } from "../../web/js/transform.js";
import { frameOf, plain } from "./helpers.mjs";
// The session gives the cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
import { withCells } from "./session_cells.mjs";

const load = (name) => JSON.parse(readFileSync(new URL(`./vectors/${name}`, import.meta.url), "utf8"));
const fe38 = load("FE-38-queries.json");
const fe39 = load("FE-39-join.json");
const fe40 = load("FE-40-combine.json");

test("FE-38 the queries of a panel in the order of the document, with their refs and hide", () => {
  for (const v of fe38.queries) {
    assert.deepEqual(queriesOf(v.panel).map((q) => q.ref), v.refs, v.name);
    assert.deepEqual(visibleQueries(v.panel).map((q) => q.ref), v.visible, v.name);
    assert.equal(hasExtra(v.panel), v.hasExtra, v.name);
  }
  assert.equal(MAX_QUERIES_PER_PANEL, 8);
});

test("FE-38 the body of a request has `ref` for a panel with extra queries only", () => {
  const { base, cases } = fe38.bodies;
  for (const c of cases) {
    const panel = fe38.panels[c.panel];
    const query = queriesOf(panel).find((q) => q.ref === c.ref);
    assert.deepEqual(requestBody(panel, query, base), c.body, `${c.panel} ${c.ref}`);
  }
  assert.ok(!("ref" in base), "the base body is not changed");
});

test("FE-38 the state of a panel is a function of its slots", () => {
  assert.equal(fe38.states.length, 31);
  for (const v of fe38.states) assert.equal(panelState(v.slots), v.state, JSON.stringify(v.slots.map((s) => s.state)));
});

test("FE-38 the failed refs, the badge and the first error", () => {
  for (const v of fe38.failed) {
    assert.equal(failedRefs(v.slots), v.failed);
    assert.equal(badgeText(v.slots), v.badge);
    const error = firstError(v.slots, v.many);
    assert.equal(error ? error.message : null, v.firstMessage);
  }
  for (const v of fe38.whole) assert.equal(isWholeError(v.error), v.whole, JSON.stringify(v.error));
});

test("FE-38 a plain record is one slot with the ref of its query", () => {
  const record = { panel: fe38.panels.firstRef, state: "ready", frame: { rows: 1 }, error: null };
  assert.deepEqual(slotsOf(record), [{ ref: "Z", state: "ready", frame: { rows: 1 }, error: null }]);
  assert.equal(slotsOf({ panel: fe38.panels.one, state: "idle" })[0].ref, "A");
  const own = [{ ref: "B" }];
  assert.equal(slotsOf({ slots: own }), own);
});

const fieldsOf = (frame) => tagFields(framesToFields(frameOf(frame.cols)), frame.ref, true);

test("FE-39 the join of frames: 40 vectors", () => {
  assert.equal(fe39.vectors.length, 40);
  for (const v of fe39.vectors) {
    const lists = v.frames.map((f) => ({ ref: f.ref, fields: fieldsOf(f) }));
    if (v.error) {
      assert.throws(() => joinByTime(lists), { message: v.error }, v.name);
      continue;
    }
    const out = joinByTime(lists);
    assert.deepEqual(out.map((f) => f.name), v.fields.map((f) => f.name), v.name);
    out.forEach((f, i) => {
      const want = v.fields[i];
      assert.deepEqual(plain(f), want.values, `${v.name}: ${want.name}`);
      assert.equal(f.ref, want.ref, `${v.name}: ref of ${want.name}`);
      if (i > 0) assert.equal(f.kind, want.kind, `${v.name}: kind of ${want.name}`);
    });
    assert.equal(out[0].kind, "time");
    assert.equal(out[0].ref, undefined, "the joined time field has no ref");
  }
});

test("FE-39 the cap: 200,000 rows join and 200,001 rows give the error", () => {
  const frame = (ref, n, start) => ({ ref, fields: tagFields(framesToFields(frameOf([{ name: "time", kind: "time", values: Array.from({ length: n }, (_, i) => start + i) }, { name: ref.toLowerCase(), kind: "number", values: Array.from({ length: n }, (_, i) => i) }])), ref, true) });
  assert.equal(MAX_JOIN_ROWS, 200000);
  const ok = joinByTime([frame("A", 100000, 0), frame("B", 100000, 100000)]);
  assert.equal(ok[0].values.length, 200000);
  assert.throws(() => joinByTime([frame("A", 100000, 0), frame("B", 100001, 100000)]), { message: "The frames have 200001 rows. A join takes 200,000 rows at most." });
});

test("FE-39 the join of 8 frames and 200,000 rows takes 150 ms or less", () => {
  const lists = Array.from({ length: 8 }, (_, k) => {
    const n = 25000;
    const ref = String.fromCharCode(65 + k);
    // Each frame has its own shift, so the axis has about 25,000 + 7 times and the work is real.
    const fields = tagFields(framesToFields(frameOf([{ name: "time", kind: "time", values: Array.from({ length: n }, (_, i) => i * 2 + (k % 2)) }, { name: ref, kind: "number", values: Array.from({ length: n }, (_, i) => i) }])), ref, true);
    return { ref, fields };
  });
  // The minimum of 7 runs after 2 warm-up runs (the timing policy of the front tests): one run on a loaded machine measures
  // the machine, not the code.
  joinByTime(lists);
  joinByTime(lists);
  const best = Math.min(...[0, 1, 2, 3, 4, 5, 6].map(() => {
    const start = performance.now();
    joinByTime(lists);
    return performance.now() - start;
  }));
  assert.ok(best <= 150, `${best.toFixed(1)} ms`);
});

// Review v3 (finding P2, 2026-11-16): the join on time checks the fields and the cells of 9.21a, rule 13, before it builds a field.
test("FE-39 the join on time: 3,001 fields or 190,100,000 cells are an error before a field is built", () => {
  // A query of R rows (time, host of 100 values, F value fields) and partition_by_values by host: 100 frames, 1 + 100 F fields.
  const lists = (F, R) => {
    const time = { name: "time", type: "DateTime", kind: "time", values: Float64Array.from({ length: R }, (_, i) => i * 1000), nulls: new Uint8Array(R), labels: null, config: {} };
    const host = { name: "host", type: "String", kind: "text", values: Array.from({ length: R }, (_, i) => `h${i % 100}`), labels: null, config: {} };
    const values = Array.from({ length: F }, (_, c) => ({ name: `v${c}`, type: "Float64", kind: "number", values: new Float64Array(R).fill(c), nulls: new Uint8Array(R), labels: null, config: {} }));
    return applyList([{ ref: "A", fields: [time, host, ...values] }], [{ kind: "partition_by_values", fields: ["host"], keep_fields: false }]);
  };
  const wide = lists(30, 6000);
  assert.equal(wide.length, 100);
  const t0 = performance.now();
  assert.throws(() => combine("timeseries", wide), { message: "The join on time failed: 3001 fields, 2,000 at most" });
  assert.ok(performance.now() - t0 < 300, "the error comes before the work");
  assert.throws(() => combine("heatmap", lists(19, 100000)), { message: "The join on time failed: 190100000 cells, 10,000,000 at most" });
  // 1 + 100 x 19 = 1,901 fields x 5,000 times = 9,505,000 cells pass.
  assert.equal(combine("timeseries", lists(19, 5000)).length, 1901);
  // The cap of the cells is the value of the session (6.10, `server.transform_max_cells`). A configured value applies as it is:
  // 1 value field gives 1 + 100 fields, so 100 times are 10,100 cells and 200 times are 20,200 cells.
  withCells(10100, () => {
    assert.equal(combine("timeseries", lists(1, 100)).length, 101);
    assert.throws(() => combine("timeseries", lists(1, 200)), { message: "The join on time failed: 20200 cells, 10,100 at most" });
  });
  // With no valid value in the session, the join fails closed.
  for (const bad of [undefined, null, Infinity]) withCells(bad, () => assert.throws(() => combine("timeseries", lists(1, 100)), { message: "The join on time failed: the session has no valid transform_max_cells" }));
});

test("FE-40 the combination by type and the names: 30 vectors", () => {
  assert.equal(fe40.vectors.length, 30);
  for (const v of fe40.vectors) {
    const items = v.items.map((i) => ({ ref: i.ref, frame: frameOf(i.cols) }));
    const made = preparePanelFrames(v.panel, items, null, v.choice);
    assert.deepEqual(baseDisplayNames(made.fields), v.names, v.name);
    assert.deepEqual(made.lists.map((l) => l.ref), v.lists, v.name);
    if (v.rows) assert.deepEqual(made.fields.map((f) => f.values.length), v.rows, v.name);
    for (const m of v.matches || []) {
      const names = baseDisplayNames(made.fields);
      assert.deepEqual(made.fields.filter((f, i) => matchesField(m, f, names[i])).map((f, i, all) => names[made.fields.indexOf(f)]), m.fields, `${v.name}: ${m.by} ${m.value}`);
    }
  }
});

test("FE-40 combine: one list is the result, a table takes the choice, and no list gives no field", () => {
  const list = (ref) => ({ ref, fields: [{ name: ref, kind: "number", values: [], config: {} }] });
  assert.deepEqual(combine("timeseries", []), []);
  assert.equal(combine("table", [list("A")])[0].name, "A");
  assert.equal(combine("table", [list("A"), list("B")], "B")[0].name, "B");
  assert.equal(combine("stat", [list("A"), list("B")]).length, 2);
  assert.equal(combine("barchart", [list("A"), list("B")]).length, 1);
});
