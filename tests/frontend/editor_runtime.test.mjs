// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the draft runtime (10.2) and of the body of a preview (6.22, 10.3).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DraftRuntime } from "../../web/js/editor/draft_runtime.js";
import { errorsOfPanel, neededVariables, panelPointer, previewBody } from "../../web/js/editor/preview_body.js";
import { updatePanel } from "../../web/js/editor/draft.js";

const doc = JSON.parse(readFileSync(new URL("../fixtures/dashboards/nodes.json", import.meta.url)));

// A fake base runtime.
function fakeBase() {
  const listeners = new Set();
  const panels = new Map();
  for (const s of doc.sections) for (const p of s.panels) panels.set(p.id, { id: p.id, key: p.id, variable: null, value: null, panel: p, state: "ready", frame: { rows: 1 }, error: null });
  const calls = [];
  return {
    panels, calls, variables: doc.variables, range: { from_ms: 1, to_ms: 2 },
    on: (l) => { listeners.add(l); return () => listeners.delete(l); },
    emit: (id) => listeners.forEach((l) => l("panel", panels.get(id))),
    setVisible: (id, v) => calls.push(["visible", id, v]),
    retry: (id) => calls.push(["retry", id]),
    requestVariables: () => ({ node: { all: true, values: ["a", "b"] }, account: { all: false, values: ["w"] }, env: { all: false, values: ["prod"] } }),
  };
}

test("a draft panel with the base query shows the result of the base runtime", () => {
  const base = fakeBase();
  const rt = new DraftRuntime({ base, doc, onCollapse: () => {} });
  assert.equal(rt.panels.get(1).mirrored, true);
  assert.equal(rt.panels.get(1).state, "ready");
  assert.deepEqual(rt.panels.get(1).frame, { rows: 1 });
  const events = [];
  rt.on((t, rec) => events.push([t, rec.id]));
  base.panels.get(1).frame = { rows: 2 };
  base.emit(1);
  assert.deepEqual(rt.panels.get(1).frame, { rows: 2 });
  assert.ok(events.some((e) => e[1] === 1));
  // The reuse panel (3) follows its source (2).
  base.emit(2);
  assert.ok(events.some((e) => e[1] === 3));
});

test("a changed query is not mirrored: it needs a run, then shows the preview, until the query changes again", () => {
  const base = fakeBase();
  const rt = new DraftRuntime({ base, doc, onCollapse: () => {} });
  const changed = updatePanel(doc, 1, (p) => { p.data.query.sql = ["SELECT 2"]; return p; });
  rt.setDoc(changed);
  const rec = rt.panels.get(1);
  assert.equal(rec.mirrored, false);
  assert.equal(rec.needsRun, true);
  assert.equal(rec.state, "idle");
  rt.setPreview(1, { frame: { rows: 9 } });
  assert.equal(rt.panels.get(1).state, "ready");
  assert.deepEqual(rt.panels.get(1).frame, { rows: 9 });
  assert.equal(rt.panels.get(1).panel.data.query.sql[0], "SELECT 2");
  // Same query again: the preview stays. A new query: it goes.
  rt.setDoc(changed);
  assert.equal(rt.panels.get(1).state, "ready");
  rt.setDoc(updatePanel(changed, 1, (p) => { p.data.query.sql = ["SELECT 3"]; return p; }));
  assert.equal(rt.panels.get(1).needsRun, true);
  // An error of the run shows as the state error.
  rt.setPreview(1, { error: new Error("x") });
  assert.equal(rt.panels.get(1).state, "error");
});

test("visibility, retry and collapse go to the right place", () => {
  const base = fakeBase();
  const collapsed = [];
  const rt = new DraftRuntime({ base, doc, onCollapse: (i, c) => collapsed.push([i, c]) });
  rt.setVisible(2, true);
  rt.retry(2);
  rt.setExpanded(1, false);
  assert.deepEqual(base.calls, [["visible", 2, true], ["retry", 2]]);
  assert.deepEqual(collapsed, [[1, true]]);
  assert.equal(rt.expanded.get(1), false);
  assert.equal(rt.expanded.get(0), true);
});

test("preview body: the variables that the SQL names, with their parents; the values of the page", () => {
  const needed = neededVariables(doc.variables, doc.sections[0].panels[0].data.query.sql);
  assert.deepEqual(needed.map((v) => v.name), ["node"]);
  const withParent = [{ kind: "text", name: "p", default: "x" }, { kind: "query", name: "child", sql: ["SELECT 1 WHERE a = '$p'"], source: "s" }];
  assert.deepEqual(neededVariables(withParent, ["SELECT ${child}"]).map((v) => v.name), ["p", "child"]);
  const panel = doc.sections[0].panels[0];
  const body = previewBody({ doc, panel, target: { uid: "nodes" }, range: { from_ms: 10, to_ms: 20 }, maxDataPoints: 99999, known: { node: { all: true, values: ["a"] } } });
  assert.equal(body.uid, "nodes");
  assert.equal(body.max_data_points, 10000);
  assert.deepEqual(body.variables, { node: { all: true, values: ["a"] } });
  assert.deepEqual(body.dashboard_variables.map((v) => v.name), ["node"]);
  assert.equal(body.panel, panel);
  const unknown = previewBody({ doc, panel, target: { folder: "f" }, range: { from_ms: 1, to_ms: 2 }, maxDataPoints: 0, known: {} });
  assert.deepEqual(unknown.variables.node, { all: true, values: [] });
  assert.equal(unknown.folder, "f");
  assert.equal(unknown.max_data_points, 1);
});

test("pointers of a preview and of a whole draft are made relative to the panel", () => {
  assert.deepEqual(panelPointer("/panel/data/query/sql/3"), { pointer: "/data/query/sql/3", inVariables: false });
  assert.deepEqual(panelPointer("/panel"), { pointer: "", inVariables: false });
  assert.equal(panelPointer("/dashboard_variables/0/name").inVariables, true);
  const errors = [{ pointer: "/sections/0/panels/1/grid/w", code: "out_of_range" }, { pointer: "/sections/0/panels/10/grid/w", code: "out_of_range" }, { pointer: "/title", code: "x" }, { pointer: "/sections/0/panels/1", code: "overlap" }];
  const r = errorsOfPanel(errors, 0, 1);
  assert.deepEqual(r.own.map((e) => e.pointer), ["/grid/w", ""]);
  assert.equal(r.other, 2);
});
