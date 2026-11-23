// SPDX-License-Identifier: MIT OR Apache-2.0
// The jobs of the annotations in the runtime (9.8, 9.19d): one query for each annotation, after the panel queries, never for a hover.
import test from "node:test";
import assert from "node:assert/strict";
import { MAX_IN_FLIGHT } from "../../web/js/runtime.js";
import { RuntimeWithNames as Runtime } from "./helpers.mjs";
import { ApiError } from "../../web/js/api.js";

const series = (id, y) => ({ id, type: "timeseries", title: `p${id}`, description: "", grid: { x: 0, y, w: 24, h: 2 }, transforms: [], links: [], fields: { defaults: {}, overrides: [] }, options: {}, data: { query: { source: "s", format: "time_series", sql: [`SELECT ${id}`], min_interval: null, max_data_points: null } } });
const stat = (id, y) => ({ ...series(id, y), type: "stat" });
const annotation = (name, extra = {}) => ({ name, source: "s", sql: ["SELECT now() AS time"], color: "red", ...extra });

function makeDoc({ panels, annotations, sections, variables = [] }) {
  return {
    format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off",
    time: { from: "now-6h", to: "now", timezone: "utc", refresh: null },
    variables, ...(annotations ? { annotations } : {}),
    sections: sections || [{ title: null, collapsed: false, panels }],
  };
}

function fakeApi() {
  const calls = [];
  const order = [];
  const settle = (call, ok) => { call.settled = true; return ok; };
  return {
    calls, order,
    panelQuery(uid, id, body, signal) {
      return new Promise((resolve, reject) => {
        const call = { kind: "panel", id, body, signal, aborted: false };
        call.resolve = () => resolve({ columns: [], values: [], rows: 0 });
        signal.addEventListener("abort", () => { call.aborted = true; reject(new ApiError({ status: 0, code: "aborted", message: "x" })); });
        calls.push(call);
        order.push(`p${id}`);
      });
    },
    annotationQuery(uid, index, body, signal) {
      return new Promise((resolve, reject) => {
        const call = { kind: "annotation", index, body, signal, aborted: false };
        call.resolve = (rows = []) => resolve({ etag: "e", query_id: "q", rows: rows.length, annotations: rows });
        call.fail = (error) => reject(error);
        signal.addEventListener("abort", () => { call.aborted = true; reject(new ApiError({ status: 0, code: "aborted", message: "x" })); });
        calls.push(call);
        order.push(`a${index}`);
      });
    },
    async variableOptions() { return { options: [{ value: "a", text: "a" }, { value: "b", text: "b" }] }; },
    settle,
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));
const range = { fromText: "now-6h", toText: "now" };
const visible = (rt) => { for (const p of rt.panels.values()) rt.setVisible(p.key, true); };
const mk = (doc, api) => new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 500, range });
const annCalls = (api) => api.calls.filter((c) => c.kind === "annotation");

test("4 time series and 2 annotations send 2 annotation requests, after the panel requests", async () => {
  const api = fakeApi();
  const rt = mk(makeDoc({ panels: [series(1, 0), series(2, 2), series(3, 4), series(4, 6)], annotations: [annotation("A"), annotation("B")] }), api);
  await rt.start();
  visible(rt);
  await tick();
  assert.deepEqual(api.order.slice(0, 4), ["p1", "p2", "p3", "p4"]);
  assert.deepEqual(api.order.slice(4), ["a0", "a1"]);
  assert.equal(annCalls(api).length, 2);
  assert.equal(api.calls.filter((c) => c.kind === "annotation").every((c) => c.body.etag === "e" && !("max_data_points" in c.body) && typeof c.body.from_ms === "number"), true);
  rt.destroy();
});

test("the annotation jobs count in the 6 requests at the same time and come after all the panels of the trigger", async () => {
  const api = fakeApi();
  const panels = Array.from({ length: 8 }, (_, i) => series(i + 1, i * 2));
  const rt = mk(makeDoc({ panels, annotations: [annotation("A")] }), api);
  await rt.start();
  visible(rt);
  await tick();
  assert.equal(api.calls.length, MAX_IN_FLIGHT);
  assert.equal(annCalls(api).length, 0);
  for (const c of api.calls.slice(0, 2)) c.resolve();
  await tick();
  assert.equal(api.calls.length, 8, "the 2 panels that wait start before the annotation");
  assert.equal(annCalls(api).length, 0);
  for (const c of api.calls.slice(2, 6)) c.resolve();
  await tick();
  assert.equal(annCalls(api).length, 1);
  rt.destroy();
});

test("an annotation with enabled false, and an annotation whose panels are in a collapsed section, send nothing; the expansion sends it", async () => {
  const api = fakeApi();
  const doc = makeDoc({
    annotations: [annotation("off", { enabled: false }), annotation("only 2", { panels: [2] }), annotation("all")],
    sections: [{ title: "S1", collapsed: false, panels: [series(1, 0)] }, { title: "S2", collapsed: true, panels: [series(2, 0)] }],
  });
  const rt = mk(doc, api);
  await rt.start();
  visible(rt);
  await tick();
  assert.deepEqual(annCalls(api).map((c) => c.index), [2]);
  rt.setExpanded(rt.plan.sections[1].copies[0].key, true);
  await tick();
  assert.deepEqual(annCalls(api).map((c) => c.index), [2, 1]);
  rt.destroy();
});

test("a stat panel gets no annotation job", async () => {
  const api = fakeApi();
  const rt = mk(makeDoc({ panels: [stat(1, 0)], annotations: [annotation("A")] }), api);
  await rt.start();
  visible(rt);
  await tick();
  assert.equal(annCalls(api).length, 0);
  assert.deepEqual(rt.annotationsFor(1).map((r) => r.index), [0], "the record exists; the panel type decides the draw");
  rt.destroy();
});

test("a change of the range and a refresh send the annotation requests again, and abort the earlier ones", async () => {
  const api = fakeApi();
  const rt = mk(makeDoc({ panels: [series(1, 0)], annotations: [annotation("A"), annotation("B")] }), api);
  await rt.start();
  visible(rt);
  await tick();
  api.calls.find((c) => c.kind === "panel").resolve();
  await tick();
  assert.equal(annCalls(api).length, 2);
  rt.setRange("now-1h", "now");
  await tick();
  assert.ok(annCalls(api).slice(0, 2).every((c) => c.aborted));
  assert.equal(annCalls(api).length, 4);
  assert.equal(api.calls.filter((c) => c.kind === "panel").length, 2);
  for (const c of api.calls) if (!c.aborted && c.kind === "panel") c.resolve();
  await tick();
  rt.refreshNow();
  await tick();
  assert.equal(annCalls(api).length, 6);
  rt.destroy();
});

test("a variable that one SQL names sends one request; a pointer move, a hover and a legend click send none", async () => {
  const api = fakeApi();
  const doc = makeDoc({
    panels: [series(1, 0)],
    annotations: [annotation("uses env", { sql: ["SELECT now() AS time WHERE e = '$env'"] }), annotation("no variable")],
    variables: [{ kind: "custom", name: "env", label: null, hide: "none", options: [{ text: "a", value: "a" }, { text: "b", value: "b" }], multi: false, include_all: false, all_value: null, default: ["a"] }],
  });
  const rt = mk(doc, api);
  await rt.start();
  visible(rt);
  await tick();
  for (const c of api.calls) if (c.kind === "panel") c.resolve();
  for (const c of annCalls(api)) c.resolve();
  await tick();
  const before = api.calls.length;
  rt.setVisible(1, true);
  rt.schedule();
  await tick();
  assert.equal(api.calls.length, before, "no trigger, no request");
  await rt.setSelection("env", { all: false, values: ["b"] });
  await tick();
  assert.deepEqual(annCalls(api).slice(2).map((c) => c.index), [0]);
  assert.deepEqual(annCalls(api)[2].body.variables.env.values, ["b"]);
  rt.destroy();
});

test("the view of one panel sends the annotation requests of that panel only", async () => {
  const api = fakeApi();
  const rt = mk(makeDoc({ panels: [series(1, 0), series(2, 2), stat(3, 4)], annotations: [annotation("A", { panels: [1] }), annotation("B", { panels: [2] })] }), api);
  await rt.start();
  visible(rt);
  await tick();
  for (const c of api.calls) if (c.kind === "panel") c.resolve();
  await tick();
  for (const c of annCalls(api)) c.resolve();
  await tick();
  const first = annCalls(api).length;
  assert.equal(first, 2);
  rt.setViewPanel(2);
  await tick();
  const sent = annCalls(api).slice(first);
  assert.deepEqual(sent.map((c) => c.index), [1]);
  rt.setViewPanel(3);
  await tick();
  assert.equal(annCalls(api).slice(first).length, 1, "a stat panel in the view sends none");
  rt.destroy();
});

test("a ready answer gives the marks; a failed annotation gives an error and the others stay; the state events come", async () => {
  const api = fakeApi();
  const rt = mk(makeDoc({ panels: [series(1, 0)], annotations: [annotation("A"), annotation("B")] }), api);
  const events = [];
  rt.on((type, data) => { if (type === "annotations") events.push([data.index, data.state]); });
  await rt.start();
  visible(rt);
  await tick();
  api.calls.find((c) => c.kind === "panel").resolve();
  await tick();
  const [a, b] = annCalls(api);
  a.resolve([[1000, null, "T", "x", ["t"]]]);
  b.fail(new ApiError({ status: 422, code: "annotation_invalid", message: "The title has a control character.", extra: { reason: "bad_chars", row: 1 } }));
  await tick();
  const [ra, rb] = rt.annotationsFor(1);
  assert.equal(ra.state, "ready");
  assert.deepEqual(ra.items.map((m) => [m.t, m.title]), [[1000, "T"]]);
  assert.equal(rb.state, "error");
  assert.equal(rb.error.code, "annotation_invalid");
  assert.deepEqual(rb.items, []);
  assert.deepEqual(events.map((e) => e.join(":")), ["0:queued", "1:queued", "0:loading", "1:loading", "0:ready", "1:error"]);
  rt.destroy();
});

test("an answer with a wrong shape is an error of the annotation, with a fixed message", async () => {
  const api = fakeApi();
  const rt = mk(makeDoc({ panels: [series(1, 0)], annotations: [annotation("A")] }), api);
  await rt.start();
  visible(rt);
  await tick();
  api.calls.find((c) => c.kind === "panel").resolve();
  await tick();
  annCalls(api)[0].resolve([[1, null, 5, "", []]]);
  await tick();
  const [r] = rt.annotationsFor(1);
  assert.equal(r.state, "error");
  assert.equal(r.error.message, "The answer of the annotation is not valid.");
  rt.destroy();
});

test("a 409 on an annotation makes the page read the document again", async () => {
  const api = fakeApi();
  const rt = mk(makeDoc({ panels: [series(1, 0)], annotations: [annotation("A")] }), api);
  const stale = [];
  rt.on((type, data) => { if (type === "stale") stale.push(data); });
  await rt.start();
  visible(rt);
  await tick();
  api.calls.find((c) => c.kind === "panel").resolve();
  await tick();
  annCalls(api)[0].fail(new ApiError({ status: 409, code: "dashboard_changed", message: "changed", extra: { current_etag: "n" } }));
  await tick();
  assert.deepEqual(stale, ["n"]);
  rt.destroy();
});

test("a document with no annotation has no job and no record", async () => {
  const api = fakeApi();
  const rt = mk(makeDoc({ panels: [series(1, 0)] }), api);
  await rt.start();
  visible(rt);
  await tick();
  assert.equal(annCalls(api).length, 0);
  assert.deepEqual(rt.annotationsFor(1), []);
  rt.destroy();
});

test("a draft of the editor has no annotation function: the frames of the editor draw no mark", async () => {
  const { DraftRuntime } = await import("../../web/js/editor/draft_runtime.js");
  assert.equal(typeof DraftRuntime.prototype.annotationsFor, "undefined");
});
