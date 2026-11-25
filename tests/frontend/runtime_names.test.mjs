// SPDX-License-Identifier: MIT OR Apache-2.0
// The member `names` of the answer of 6.14 (9.10): the shape check of the page, and what the runtime does when the member is
// absent or wrong. The page never scans a query text: the server gives the names of each saved query.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { Runtime, readNames } from "../../web/js/runtime.js";
import { RuntimeWithNames, namesOfDocument } from "./helpers.mjs";

const variable = (name, kind, sql = ["SELECT 1"]) =>
  kind === "query"
    ? { kind, name, label: null, hide: "none", source: "s", sql, multi: false, include_all: false, all_value: null, refresh: "load", sort: "none", regex: null, default: ["a"] }
    : { kind, name, label: null, hide: "none", options: [{ text: "a", value: "a" }], multi: false, include_all: false, all_value: null, default: ["a"] };

const panel = (id, sql, extra) => ({
  id, type: "table", title: `p${id}`, description: "", grid: { x: 0, y: id * 2, w: 24, h: 2 }, transforms: [], links: [], fields: { defaults: {}, overrides: [] }, options: {},
  data: { query: { source: "s", format: "table", sql, min_interval: null, max_data_points: null }, ...(extra ? { extra_queries: extra } : {}) },
});

function doc() {
  return {
    format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off",
    time: { from: "now-6h", to: "now", timezone: "utc", refresh: null },
    variables: [variable("a", "query"), variable("b", "query", ["SELECT $a"]), variable("c", "custom")],
    annotations: [{ name: "N", source: "s", sql: ["SELECT ${c}"], color: "#1f78c1" }],
    sections: [{ title: null, collapsed: false, panels: [
      panel(1, ["SELECT ${a}"]),
      panel(2, ["SELECT 1"], [{ ref: "B", source: "s", format: "table", sql: ["SELECT $b"] }]),
      panel(3, ["SELECT 3"]),
      { id: 4, type: "text", title: "t", description: "", grid: { x: 0, y: 8, w: 24, h: 2 }, transforms: [], links: [], fields: { defaults: {}, overrides: [] }, options: { mode: "markdown", content: ["x"] } },
      { ...panel(5, []), data: { reuse: { panel: 1, with_transforms: false } } },
    ] }],
  };
}

function fakeApi() {
  const calls = [];
  return {
    calls,
    panelQuery: (uid, id, body, signal) => new Promise((resolve, reject) => {
      calls.push({ id });
      signal.addEventListener("abort", () => reject(new Error("aborted")));
      setTimeout(() => resolve({ columns: [], values: [], rows: 0 }), 0);
    }),
    variableOptions: async (uid, name) => {
      calls.push({ options: name });
      return { options: [{ value: "a", text: "a" }, { value: "b", text: "b" }] };
    },
  };
}

const tick = () => new Promise((r) => setTimeout(r, 5));
const range = { fromText: "now-6h", toText: "now" };
const GOOD = { panels: { 1: [["a"]], 2: [[], ["b"]], 3: [[]] }, variables: { a: [], b: ["a"] }, annotations: [["c"]] };

test("a member that follows 6.14 is read, each list as written", () => {
  const read = readNames(doc(), GOOD);
  assert.deepEqual(read.panels.get("2"), [[], ["b"]]);
  assert.deepEqual(read.variables.get("b"), ["a"]);
  assert.deepEqual(read.annotations, [["c"]]);
  // The helper of the tests makes the same member from the page scan of the editor.
  assert.deepEqual(namesOfDocument(doc()), GOOD);
});

test("the shape check fails on each wrong member and says what is wrong", () => {
  const wrong = (patch, text) => {
    const member = JSON.parse(JSON.stringify(GOOD));
    patch(member);
    assert.throws(() => readNames(doc(), member), (e) => e.message.includes(text), text);
  };
  assert.throws(() => readNames(doc(), undefined), /no member names/);
  assert.throws(() => readNames(doc(), null), /names must be an object/);
  assert.throws(() => readNames(doc(), []), /names must be an object/);
  wrong((m) => { m.extra = 1; }, "names must have the keys");
  wrong((m) => { delete m.annotations; }, "names must have the keys");
  wrong((m) => { delete m.panels[3]; }, "names.panels must have the keys");
  wrong((m) => { m.panels[4] = [[]]; }, "names.panels must have the keys");
  wrong((m) => { m.panels[2] = [[]]; }, "names.panels.2 must be an array of 2 lists");
  wrong((m) => { m.panels[1] = [["zzz"]]; }, "names.panels.1 must hold lists of distinct variable names");
  wrong((m) => { m.panels[1] = [["a", "a"]]; }, "distinct variable names");
  wrong((m) => { m.panels[1] = [[1]]; }, "distinct variable names");
  wrong((m) => { m.panels[1] = ["a"]; }, "distinct variable names");
  wrong((m) => { m.variables.c = []; }, "names.variables must have the keys");
  wrong((m) => { delete m.variables.b; }, "names.variables must have the keys");
  wrong((m) => { m.variables.a = "b"; }, "names.variables.a must hold lists");
  wrong((m) => { m.annotations = []; }, "names.annotations must be an array of 1 lists");
  wrong((m) => { m.annotations = "c"; }, "names.annotations must be an array of 1 lists");
});

test("each panel, each variable and each annotation takes its names from the member", async () => {
  const rt = new Runtime({ uid: "d", doc: doc(), etag: "e", api: fakeApi(), widthOf: () => 100, range, names: GOOD });
  assert.equal(rt.namesError, null);
  rt.destroy();
  const r = new RuntimeWithNames({ uid: "d", doc: doc(), etag: "e", api: fakeApi(), widthOf: () => 100, range });
  await r.start();
  assert.deepEqual([...r.panels.get(1).names], ["a"]);
  assert.deepEqual([...r.panels.get(2).names], ["b"]);
  assert.deepEqual([...r.panels.get(3).names], []);
  assert.deepEqual([...r.annotations[0].names], ["c"]);
  assert.deepEqual([...r.graph.parents.get("b")], ["a"]);
  assert.deepEqual(r.graph.order, ["a", "b", "c"]);
  r.destroy();
});

// Each wrong case: the page stays usable, tells the reason, and runs every query on every change.
for (const [label, names] of [["absent", undefined], ["not an object", "x"], ["a list that names a variable of nobody", { ...GOOD, panels: { ...GOOD.panels, 1: [["zzz"]] } }]]) {
  test(`a member that is ${label}: every panel and every annotation names every variable, and a change runs them all`, async () => {
    const api = fakeApi();
    const rt = new Runtime({ uid: "d", doc: doc(), etag: "e", api, widthOf: () => 100, range, names });
    assert.equal(typeof rt.namesError, "string");
    assert.ok(rt.namesError.length > 0);
    await rt.start();
    for (const id of [1, 2, 3]) rt.setVisible(id, true);
    await tick();
    for (const id of [1, 2, 3]) assert.deepEqual([...rt.panels.get(id).names].sort(), ["a", "b", "c"], `panel ${id}`);
    assert.deepEqual([...rt.annotations[0].names].sort(), ["a", "b", "c"]);
    // A query variable names every other variable: a change of `c` reloads a and b.
    assert.deepEqual([...rt.graph.parents.get("a")], ["b", "c"]);
    assert.deepEqual([...rt.graph.parents.get("c")], []);
    const before = api.calls.length;
    // A new value: the same value is a hit of the cache of the options and sends no request (9.10).
    await rt.setSelection("c", { all: false, values: ["b"] });
    await tick();
    const later = api.calls.slice(before);
    assert.deepEqual(later.filter((c) => c.options).map((c) => c.options).sort(), ["a", "b"]);
    // Panel 2 has two queries, so it sends two requests (9.8d).
    assert.deepEqual(later.filter((c) => c.id).map((c) => c.id).sort(), [1, 2, 2, 3]);
    rt.destroy();
  });
}

test("with a good member a change runs only the panels that name the variable", async () => {
  const api = fakeApi();
  const rt = new RuntimeWithNames({ uid: "d", doc: doc(), etag: "e", api, widthOf: () => 100, range });
  assert.equal(rt.namesError, null);
  await rt.start();
  for (const id of [1, 2, 3]) rt.setVisible(id, true);
  await tick();
  const before = api.calls.length;
  await rt.setSelection("c", { all: false, values: ["a"] });
  await tick();
  assert.deepEqual(api.calls.slice(before), []);
  rt.destroy();
});

test("the page does not import the scan of a query text", () => {
  const web = new URL("../../web/js/", import.meta.url);
  for (const file of ["runtime.js", "dashboard.js"]) assert.ok(!/sqlscan/.test(readFileSync(new URL(file, web), "utf8")), file);
});
