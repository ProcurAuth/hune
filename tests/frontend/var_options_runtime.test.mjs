// SPDX-License-Identifier: MIT OR Apache-2.0
// The options of the query variables in the runtime (9.10): the state of each variable, a failed variable that keeps its selection, a variable that
// waits for the variable above it, the Retry, the cache of the answers and the cancel of a request that leaves with the page.
import test from "node:test";
import assert from "node:assert/strict";
import { RuntimeWithNames } from "./helpers.mjs";

const variable = (name, sql, extra = {}) => ({
  kind: "query", name, label: extra.label ?? null, hide: "none", source: "s", sql, multi: false, include_all: false, all_value: null,
  refresh: "time_change", sort: "none", regex: null, default: ["a"], ...extra,
});
const panel = (id, sql) => ({
  id, type: "table", title: `p${id}`, description: "", grid: { x: 0, y: id * 2, w: 24, h: 2 }, transforms: [], links: [], fields: { defaults: {}, overrides: [] }, options: {},
  data: { query: { source: "s", format: "table", sql, min_interval: null, max_data_points: null } },
});
// `top` and `solo` have no variable above. `below` names `top`. `deep` names `below`.
const doc = (variables) => ({
  format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off",
  time: { from: "now-6h", to: "now", timezone: "utc", refresh: null },
  variables: variables || [
    variable("top", ["SELECT 1"], { label: "Top label" }),
    variable("below", ["SELECT ${top}"]),
    variable("deep", ["SELECT ${below}"]),
    variable("solo", ["SELECT 2"]),
  ],
  annotations: [],
  sections: [{ title: null, collapsed: false, panels: [panel(1, ["SELECT ${top} ${below} ${deep} ${solo}"])] }],
});

const failure = (code = "query_memory_limit") => Object.assign(new Error("Code: 241. DB::Exception: memory (MEMORY_LIMIT_EXCEEDED) (version 26.3.1.1 (official build))"), { code, status: 422, retryable: false, extra: {}, aborted: false });

// An API that records the calls, fails the variables of `failing` (a Set of names) and answers the others with two options.
function fakeApi({ failing = new Set(), hold = null } = {}) {
  const api = {
    failing,
    calls: [],
    bodies: [],
    signals: new Map(),
    panelQuery: async (uid, id, body) => {
      api.bodies.push({ panel: id, variables: body.variables });
      return { columns: [], values: [], rows: 0 };
    },
    variableOptions: (uid, name, body, signal) => {
      api.calls.push(name);
      api.signals.set(name, signal);
      const answer = () => (api.failing.has(name) ? Promise.reject(failure()) : Promise.resolve({ options: [{ value: "a", text: "a" }, { value: "b", text: "b" }] }));
      if (hold && hold.name === name) return new Promise((resolve, reject) => {
        signal.addEventListener("abort", () => reject(Object.assign(new Error("aborted"), { code: "aborted", aborted: true })));
        hold.release = () => answer().then(resolve, reject);
      });
      return answer();
    },
  };
  return api;
}
const make = (api, o = {}) => new RuntimeWithNames({ uid: "d", doc: doc(o.variables), etag: "e", api, widthOf: () => 100, range: { fromText: "now-6h", toText: "now" }, now: o.now, ...(o.extra || {}) });
const tick = () => new Promise((r) => setTimeout(r, 5));

test("each variable is loading or waiting at first, and ready when its options came", async () => {
  const rt = make(fakeApi());
  assert.deepEqual(rt.optionStateOf("top"), { state: "loading", parent: null });
  assert.equal(rt.optionStateOf("below").state, "waiting", "a variable with a query variable above it waits");
  assert.equal(rt.optionStateOf("solo").state, "loading");
  assert.equal(rt.optionStateOf("not-a-variable").state, "ready");
  await rt.start();
  for (const name of ["top", "below", "deep", "solo"]) assert.equal(rt.optionStateOf(name).state, "ready", name);
  rt.destroy();
});

test("a variable whose options fail is in the state error, keeps its selection, and the panels still run with it", async () => {
  const api = fakeApi({ failing: new Set(["solo"]) });
  const rt = make(api);
  await rt.start();
  await tick();
  assert.equal(rt.optionStateOf("solo").state, "error");
  assert.ok(rt.optionErrors.has("solo"));
  // The default of the variable stays: a value that is not an option is not dropped when the options failed.
  assert.deepEqual(rt.selections.get("solo"), { all: false, values: ["a"] });
  rt.setVisible(1, true);
  await tick();
  assert.ok(api.bodies.length > 0, "the panel queried");
  assert.deepEqual(api.bodies.at(-1).variables.solo, { all: false, values: ["a"] });
  rt.destroy();
});

test("a failed variable keeps $__all, a selection of several values and a value that the person typed", async () => {
  const variables = [
    variable("everything", ["SELECT 1"], { include_all: true, multi: true, default: ["$__all"] }),
    variable("several", ["SELECT 2"], { multi: true, default: ["x", "y"] }),
    variable("typed", ["SELECT 3"], { allow_custom_value: true, default: ["free"] }),
  ];
  const api = fakeApi({ failing: new Set(["everything", "several", "typed"]) });
  const rt = make(api, { variables });
  await rt.start();
  assert.deepEqual(rt.selections.get("everything"), { all: true, values: [] });
  assert.deepEqual(rt.selections.get("several"), { all: false, values: ["x", "y"] });
  assert.deepEqual(rt.selections.get("typed"), { all: false, values: ["free"] });
  rt.destroy();
});

test("a variable below a failed variable waits, sends no request, and names the label of its parent", async () => {
  const api = fakeApi({ failing: new Set(["top"]) });
  const rt = make(api);
  await rt.start();
  assert.deepEqual(api.calls.sort(), ["solo", "top"], "below and deep sent no options request");
  assert.deepEqual(rt.optionStateOf("below"), { state: "waiting", parent: "Top label" });
  // The label of the nearest variable above that waits or failed: `deep` names `below`.
  assert.equal(rt.optionStateOf("deep").state, "waiting");
  assert.equal(rt.optionStateOf("deep").parent, "below");
  assert.equal(rt.optionStateOf("solo").state, "ready", "a variable with no link goes on");
  // The selections of the waiting variables stay: the panels run with them.
  assert.deepEqual(rt.selections.get("below"), { all: false, values: ["a"] });
  rt.destroy();
});

test("Retry loads the failed variable and the variables below it, and a variable below sends its request only after its parent came", async () => {
  const api = fakeApi({ failing: new Set(["top"]) });
  const rt = make(api);
  await rt.start();
  api.calls.length = 0;
  api.failing.clear();
  await rt.retryOptions("top");
  await tick();
  assert.deepEqual(api.calls, ["top", "below", "deep"], "in the order of the graph, and not solo");
  for (const name of ["top", "below", "deep"]) assert.equal(rt.optionStateOf(name).state, "ready", name);
  assert.equal(rt.optionErrors.has("top"), false);
  rt.destroy();
});

test("Retry that fails again leaves the variable in the state error and the children waiting", async () => {
  const api = fakeApi({ failing: new Set(["top"]) });
  const rt = make(api);
  await rt.start();
  api.calls.length = 0;
  await rt.retryOptions("top");
  await tick();
  assert.deepEqual(api.calls, ["top"], "the children did not send");
  assert.equal(rt.optionStateOf("top").state, "error");
  assert.equal(rt.optionStateOf("below").state, "waiting");
  rt.destroy();
});

test("a refresh within the time to live sends no options request, and the next one after it does", async () => {
  let t = 1_795_000_000_000;
  const api = fakeApi();
  const rt = make(api, { now: () => t });
  await rt.start();
  const first = api.calls.length;
  assert.equal(first, 4, "one request for each variable at the start");
  t += 5_000; // a tick of the refresh: the relative period moved, but its text did not
  rt.refreshNow();
  await rt.varChain;
  await tick();
  assert.equal(api.calls.length, first, "the cache answered");
  t += 61_000;
  rt.refreshNow();
  await rt.varChain;
  await tick();
  assert.equal(api.calls.length, first + 4, "the time to live is over: the four variables ask again");
  rt.destroy();
});

test("a variable that loads with the dashboard does not follow the period, and a variable that follows it does", async () => {
  const variables = [variable("fixed", ["SELECT 1"], { refresh: "dashboard_load" }), variable("moving", ["SELECT 2"], { refresh: "time_change" })];
  let t = 1_795_000_000_000;
  const api = fakeApi();
  const rt = make(api, { variables, now: () => t });
  await rt.start();
  api.calls.length = 0;
  // An absolute period: each end is a number. A new period is a new key for `moving`.
  rt.setRange(String(t - 7_200_000), String(t - 3_600_000));
  await rt.varChain;
  await tick();
  assert.deepEqual(api.calls, ["moving"], "only the variable that follows the period asks again");
  api.calls.length = 0;
  // The same absolute period again: the key is known.
  rt.setRange("now-6h", "now");
  rt.setRange(String(t - 7_200_000), String(t - 3_600_000));
  await rt.varChain;
  await tick();
  assert.deepEqual(api.calls, [], "back to a known period: no request");
  rt.destroy();
});

test("a change of the value of the variable above makes a new key, and a return to the first value is a hit", async () => {
  const api = fakeApi();
  const rt = make(api);
  await rt.start();
  api.calls.length = 0;
  await rt.setSelection("top", { all: false, values: ["b"] });
  await tick();
  // `below` names `top`, so it asks with the new value. Its own value did not change, so `deep` has the same key as before: a hit.
  assert.deepEqual(api.calls, ["below"], "only the variable that names the changed value asks");
  api.calls.length = 0;
  await rt.setSelection("top", { all: false, values: ["a"] });
  await tick();
  assert.deepEqual(api.calls, [], "the first value is in the cache");
  rt.destroy();
});

test("a failed answer is not kept: the next load sends a request again", async () => {
  const api = fakeApi({ failing: new Set(["solo"]) });
  const rt = make(api);
  await rt.start();
  api.calls.length = 0;
  rt.refreshNow();
  await rt.varChain;
  await tick();
  assert.ok(api.calls.includes("solo"), "solo asks again after its failure");
  assert.equal(api.calls.filter((n) => n !== "solo").length, 0, "the others come from the cache");
  rt.destroy();
});

test("a request of options that is running is cancelled when the runtime is destroyed", async () => {
  const hold = { name: "solo", release: null };
  const api = fakeApi({ hold });
  const rt = make(api);
  const started = rt.start();
  await tick();
  assert.equal(api.signals.get("solo").aborted, false);
  rt.destroy();
  assert.equal(api.signals.get("solo").aborted, true, "the signal of the request is aborted");
  await started;
});

test("a new request for a variable cancels the older one", async () => {
  const hold = { name: "solo", release: null };
  const api = fakeApi({ hold });
  const rt = make(api);
  rt.start();
  await tick();
  const older = api.signals.get("solo");
  assert.equal(older.aborted, false);
  rt.retryOptions("solo");
  await tick();
  assert.equal(older.aborted, true, "the first request is cancelled");
  rt.destroy();
});
