// Tests of the runtime of the dashboard page (9.8, 9.10): scheduler, aborts, variable changes, reuse, 409.
import test from "node:test";
import assert from "node:assert/strict";
import { MAX_IN_FLIGHT } from "../../web/js/runtime.js";
import { RuntimeWithNames as Runtime } from "./helpers.mjs";
import { ApiError } from "../../web/js/api.js";

function makeDoc(panelCount, extra = {}) {
  const panels = [];
  for (let i = 0; i < panelCount; i++) {
    panels.push({ id: i + 1, type: "table", title: `p${i + 1}`, description: "", grid: { x: 0, y: i * 2, w: 24, h: 2 }, transforms: [], links: [], fields: { defaults: {}, overrides: [] }, options: {}, data: { query: { source: "s", format: "table", sql: [`SELECT ${i}`], min_interval: null, max_data_points: null } } });
  }
  return {
    format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off",
    time: { from: "now-6h", to: "now", timezone: "utc", refresh: null },
    variables: [], sections: [{ title: null, collapsed: false, panels }], ...extra,
  };
}

// A fake API: each panel query waits until the test releases it.
function fakeApi() {
  const calls = [];
  let inFlight = 0;
  let maxSeen = 0;
  return {
    calls,
    get maxSeen() { return maxSeen; },
    panelQuery(uid, id, body, signal) {
      return new Promise((resolve, reject) => {
        inFlight += 1;
        maxSeen = Math.max(maxSeen, inFlight);
        const call = { id, body, signal, aborted: false };
        call.resolve = (frame = { columns: [], values: [], rows: 0 }) => { inFlight -= 1; resolve(frame); };
        call.fail = (error) => { inFlight -= 1; reject(error); };
        signal.addEventListener("abort", () => { call.aborted = true; inFlight -= 1; reject(new ApiError({ status: 0, code: "aborted", message: "x" })); });
        calls.push(call);
      });
    },
    async variableOptions(uid, name, body) {
      calls.push({ options: name, body });
      return { options: (this.optionsFor && this.optionsFor(name, body)) || [{ value: "a", text: "a" }, { value: "b", text: "b" }] };
    },
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));
const range = { fromText: "now-6h", toText: "now" };

function visibleAll(rt) { for (const p of rt.panels.values()) rt.setVisible(p.id, true); }

test("scheduler: at most 6 requests at the same time, in document order", async () => {
  const api = fakeApi();
  const rt = new Runtime({ uid: "d", doc: makeDoc(10), etag: "e", api, widthOf: () => 500, range });
  await rt.start();
  visibleAll(rt);
  await tick();
  assert.equal(api.calls.length, MAX_IN_FLIGHT);
  assert.deepEqual(api.calls.map((c) => c.id), [1, 2, 3, 4, 5, 6]);
  api.calls[0].resolve();
  await tick();
  assert.equal(api.calls.length, 7);
  assert.equal(api.calls[6].id, 7);
  for (const c of api.calls) if (!c.aborted) c.resolve();
  await tick();
  for (let i = 0; i < 5; i++) { for (const c of api.calls) if (!c.settled && !c.aborted) { c.settled = true; c.resolve(); } await tick(); }
  assert.ok(api.maxSeen <= MAX_IN_FLIGHT);
  assert.equal([...rt.panels.values()].filter((p) => p.state === "ready").length, 10);
  rt.destroy();
});

test("only visible panels query; the width gives max_data_points", async () => {
  const api = fakeApi();
  const rt = new Runtime({ uid: "d", doc: makeDoc(3), etag: "e", api, widthOf: (id) => 300 + id, range });
  await rt.start();
  rt.setVisible(2, true);
  await tick();
  assert.deepEqual(api.calls.map((c) => c.id), [2]);
  assert.equal(api.calls[0].body.max_data_points, 302);
  assert.equal(api.calls[0].body.etag, "e");
  assert.equal(typeof api.calls[0].body.from_ms, "number");
  rt.destroy();
});

test("a collapsed section does not query until it expands", async () => {
  const api = fakeApi();
  const doc = makeDoc(2);
  doc.sections[0].title = "S";
  doc.sections[0].collapsed = true;
  const rt = new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 100, range });
  await rt.start();
  visibleAll(rt);
  await tick();
  assert.equal(api.calls.length, 0);
  rt.setExpanded(0, true);
  await tick();
  assert.equal(api.calls.length, 2);
  rt.destroy();
});

test("a new time range aborts the requests of the earlier state and runs again", async () => {
  const api = fakeApi();
  const rt = new Runtime({ uid: "d", doc: makeDoc(3), etag: "e", api, widthOf: () => 100, range });
  await rt.start();
  visibleAll(rt);
  await tick();
  assert.equal(api.calls.length, 3);
  assert.deepEqual(rt.setRange("now-1h", "now"), {});
  await tick();
  assert.ok(api.calls.slice(0, 3).every((c) => c.aborted));
  assert.equal(api.calls.length, 6);
  assert.deepEqual(rt.setRange("now", "now-1h").error !== undefined, true);
  assert.equal(rt.range.fromText, "now-1h");
  rt.destroy();
});

test("a variable change runs only the panels that name it; options below reload", async () => {
  const api = fakeApi();
  api.optionsFor = (name) => (name === "child" ? [{ value: "c1", text: "c1" }] : [{ value: "a", text: "a" }, { value: "b", text: "b" }]);
  const doc = makeDoc(3);
  doc.sections[0].panels[0].data.query.sql = ["SELECT * WHERE x = '${parent}'"];
  doc.sections[0].panels[1].data.query.sql = ["SELECT * WHERE y IN (${child})"];
  doc.variables = [
    { kind: "query", name: "child", label: null, hide: "none", source: "s", sql: ["SELECT v FROM t WHERE p = '$parent'"], multi: false, include_all: false, all_value: null, refresh: "load", sort: "none", regex: null, default: [] },
    { kind: "query", name: "parent", label: null, hide: "none", source: "s", sql: ["SELECT p"], multi: false, include_all: false, all_value: null, refresh: "load", sort: "none", regex: null, default: ["a"] },
  ];
  const rt = new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 100, range });
  assert.deepEqual(rt.graph.order, ["parent", "child"]);
  await rt.start();
  visibleAll(rt);
  await tick();
  const optionCalls = api.calls.filter((c) => c.options).map((c) => c.options);
  assert.deepEqual(optionCalls, ["parent", "child"]);
  for (const c of api.calls.filter((x) => x.resolve)) c.resolve();
  await tick();
  const before = api.calls.length;
  await rt.setSelection("parent", { all: false, values: ["b"] });
  await tick();
  const later = api.calls.slice(before);
  assert.deepEqual(later.filter((c) => c.options).map((c) => c.options), ["child"]);
  // Panel 1 names parent, panel 2 names child (whose options reloaded and kept c1): panel 3 stays.
  const ids = later.filter((c) => c.resolve).map((c) => c.id);
  assert.deepEqual(ids, [1]);
  assert.equal(later.find((c) => c.id === 1).body.variables.parent.values[0], "b");
  rt.destroy();
});

test("a reuse panel that is shown makes its source query, out of view", async () => {
  const api = fakeApi();
  const doc = makeDoc(2);
  doc.sections[0].panels[1].data = { reuse: { panel: 1, with_transforms: true } };
  const rt = new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 100, range });
  await rt.start();
  rt.setVisible(2, true);
  await tick();
  assert.deepEqual(api.calls.map((c) => c.id), [1]);
  api.calls[0].resolve({ columns: [], values: [], rows: 0 });
  await tick();
  assert.equal(rt.panels.get(1).state, "ready");
  rt.destroy();
});

test("409 dashboard_changed stops the runtime and tells the page", async () => {
  const api = fakeApi();
  const rt = new Runtime({ uid: "d", doc: makeDoc(2), etag: "e", api, widthOf: () => 100, range });
  let stale = null;
  rt.on((type, data) => { if (type === "stale") stale = data; });
  await rt.start();
  visibleAll(rt);
  await tick();
  api.calls[0].fail(new ApiError({ status: 409, code: "dashboard_changed", message: "x", extra: { current_etag: "new" } }));
  await tick();
  assert.equal(stale, "new");
  assert.ok(rt.stale);
  assert.equal(rt.isBusy(), false);
});

test("an error of a panel shows as its state; payload_too_large has the fixed text", async () => {
  const api = fakeApi();
  const rt = new Runtime({ uid: "d", doc: makeDoc(1), etag: "e", api, widthOf: () => 100, range });
  await rt.start();
  visibleAll(rt);
  await tick();
  api.calls[0].fail(new ApiError({ status: 413, code: "payload_too_large", message: "big" }));
  await tick();
  const p = rt.panels.get(1);
  assert.equal(p.state, "error");
  assert.equal(p.error.message, "The selection of the variables is too large. Select fewer values.");
  rt.retry(1);
  await tick();
  assert.equal(p.state, "loading");
  rt.destroy();
});

test("F13 interpolate: the titles follow the selections (9.13)", async () => {
  const api = fakeApi();
  api.optionsFor = () => [{ value: "a", text: "Alpha" }, { value: "b", text: "Beta" }];
  const doc = makeDoc(1);
  doc.variables = [
    { kind: "query", name: "host", label: null, hide: "none", source: "s", sql: ["SELECT h"], multi: true, include_all: true, all_value: null, refresh: "load", sort: "none", regex: null, default: ["a"] },
    { kind: "text", name: "note", label: null, hide: "none", default: "x", value_type: "string" },
  ];
  const rt = new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 100, range });
  await rt.start();
  await tick();
  assert.equal(rt.interpolate("H ${host} N $note"), "H Alpha N x");
  await rt.setSelection("host", { all: false, values: ["a", "b"] });
  assert.equal(rt.interpolate("H ${host} ${host:text} ${host:csv} ${host:pipe} ${host:glob}"), "H Alpha + Beta Alpha + Beta a,b a|b {a,b}");
  await rt.setSelection("host", { all: true, values: [] });
  assert.equal(rt.interpolate("${host:text} ${host} ${host:csv}"), "All All a,b");
  assert.equal(rt.interpolate("$nothing ${host:raw}"), "$nothing ${host:raw}");
  rt.destroy();
});

// F18 (9.8b): the time override of a panel.
const NOW = Date.parse("2026-11-09T14:30:00.000Z");

function withOverride(over) {
  const doc = makeDoc(3);
  Object.assign(doc.sections[0].panels[0].data.query, over[0] || {});
  Object.assign(doc.sections[0].panels[1].data.query, over[1] || {});
  return doc;
}

test("time override: each panel queries with its own range, and the others keep the range of the dashboard", async () => {
  const api = fakeApi();
  const doc = withOverride([{ time_from: "now-24h" }, { time_shift: "1d" }]);
  const rt = new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 300, range, now: () => NOW });
  await rt.start();
  visibleAll(rt);
  await tick();
  const iso = (ms) => new Date(ms).toISOString();
  const by = (id) => api.calls.find((c) => c.id === id).body;
  assert.equal(iso(by(1).from_ms), "2026-11-08T14:30:00.000Z");
  assert.equal(iso(by(1).to_ms), "2026-11-09T14:30:00.000Z");
  assert.equal(iso(by(2).from_ms), "2026-11-08T08:30:00.000Z");
  assert.equal(iso(by(2).to_ms), "2026-11-08T14:30:00.000Z");
  assert.equal(iso(by(3).from_ms), "2026-11-09T08:30:00.000Z");
  assert.equal(iso(by(3).to_ms), "2026-11-09T14:30:00.000Z");
  assert.equal(iso(rt.range.from_ms), "2026-11-09T08:30:00.000Z");
  rt.destroy();
});

test("time override: a refresh computes the range again with a new now, and an absolute range of the dashboard leaves time_from without effect", async () => {
  let t = NOW;
  const api = fakeApi();
  const rt = new Runtime({ uid: "d", doc: withOverride([{ time_from: "now-1h" }, { time_shift: "1h" }]), etag: "e", api, widthOf: () => 300, range, now: () => t });
  await rt.start();
  visibleAll(rt);
  await tick();
  assert.equal(rt.rangeOf(1).to_ms, NOW);
  t = NOW + 60000;
  rt.refreshNow();
  assert.equal(rt.rangeOf(1).to_ms, NOW + 60000);
  assert.equal(rt.rangeOf(2).to_ms, NOW + 60000 - 3600000);
  // A drag sets an absolute range: both ends are absolute, so time_from does nothing (9.8b, step 1, paper V6) and the shift still applies.
  const a = NOW - 7200000;
  const b = NOW - 3600000;
  assert.deepEqual(rt.setRange(String(a), String(b)), {});
  assert.equal(rt.rangeOf(1).from_ms, a);
  assert.equal(rt.rangeOf(1).to_ms, b);
  assert.equal(rt.rangeOf(2).from_ms, a - 3600000);
  assert.equal(rt.rangeOf(2).to_ms, b - 3600000);
  assert.equal(rt.rangeOf(3).from_ms, a);
  // A range with one end in `now` text makes time_from apply again.
  assert.deepEqual(rt.setRange(String(a), "now"), {});
  assert.equal(rt.rangeOf(1).from_ms, t - 3600000);
  assert.equal(rt.rangeOf(1).to_ms, t);
  rt.destroy();
});

test("time override: a range that the page cannot use gives the state error and sends no query", async () => {
  const api = fakeApi();
  const early = Date.parse("1970-01-01T05:00:00.000Z");
  const rt = new Runtime({ uid: "d", doc: withOverride([{ time_from: "now-1d" }]), etag: "e", api, widthOf: () => 300, range: { fromText: "now-1h", toText: "now" }, now: () => early });
  await rt.start();
  visibleAll(rt);
  await tick();
  assert.deepEqual(api.calls.map((c) => c.id), [2, 3]);
  const p = rt.panels.get(1);
  assert.equal(p.state, "error");
  assert.equal(p.error.message, "The time override of this panel gives a range that Hune cannot use.");
  assert.equal(rt.isBusy(), true); // the other two panels still wait
  for (const c of api.calls) c.resolve();
  await tick();
  assert.equal(rt.isBusy(), false);
  // Retry gives the same error and still sends nothing.
  rt.retry(1);
  await tick();
  assert.equal(api.calls.length, 2);
  assert.equal(rt.panels.get(1).state, "error");
  rt.destroy();
});

test("time override: a reuse panel has the range of its source, and a panel with no query has the range of the dashboard", () => {
  const doc = makeDoc(2);
  doc.sections[0].panels[0].data.query.time_from = "now-24h";
  doc.sections[0].panels[1].data = { reuse: { panel: 1, with_transforms: true } };
  const rt = new Runtime({ uid: "d", doc, etag: "e", api: fakeApi(), widthOf: () => 300, range, now: () => NOW });
  assert.equal(rt.rangeOf(2), rt.rangeOf(1));
  assert.equal(new Date(rt.rangeOf(2).from_ms).toISOString(), "2026-11-08T14:30:00.000Z");
  assert.equal(rt.panels.get(2).range.from_ms, rt.range.from_ms);
  assert.equal(rt.rangeOf(99), null);
  rt.destroy();
});

test("time override: the options of a query variable and the dashboard range use the range of the dashboard", async () => {
  const api = fakeApi();
  const doc = withOverride([{ time_from: "now-24h" }]);
  doc.variables = [{ name: "q", kind: "query", source: "s", sql: ["SELECT 1"], refresh: "time_change", multi: false, include_all: false, default: [], hide: "none", label: "" }];
  const rt = new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 300, range, now: () => NOW });
  await rt.start();
  const options = api.calls.find((c) => c.options === "q");
  assert.equal(new Date(options.body.from_ms).toISOString(), "2026-11-09T08:30:00.000Z");
  rt.destroy();
});

test("zone names: a name that the browser does not know gives the zone browser in the runtime", () => {
  const mk = (timezone) => new Runtime({ uid: "d", doc: { ...makeDoc(1), time: { from: "now-6h", to: "now", timezone, refresh: null } }, etag: "e", api: fakeApi(), widthOf: () => 1, range, now: () => NOW });
  assert.equal(mk("Asia/Tokyo").zone, "Asia/Tokyo");
  assert.equal(mk("Nowhere/Land").zone, "browser");
  assert.equal(mk("utc").zone, "utc");
  // The range of a named zone: now/d is the start of the day in that zone.
  const tokyo = new Runtime({ uid: "d", doc: { ...makeDoc(1), time: { from: "now/d", to: "now", timezone: "Asia/Tokyo", refresh: null } }, etag: "e", api: fakeApi(), widthOf: () => 1, range: { fromText: "now/d", toText: "now" }, now: () => NOW });
  assert.equal(new Date(tokyo.range.from_ms).toISOString(), "2026-11-08T15:00:00.000Z");
});

test("D02 (9.5): a known zone of the URL replaces the zone of the document for the range, and the document does not change", () => {
  const doc = { ...makeDoc(1), time: { from: "now/d", to: "now", timezone: "Asia/Tokyo", refresh: null } };
  const rt = new Runtime({ uid: "d", doc, etag: "e", api: fakeApi(), widthOf: () => 1, range: { fromText: "now/d", toText: "now/d" }, now: () => NOW, zone: "Europe/Paris" });
  assert.equal(rt.zone, "Europe/Paris");
  assert.equal(new Date(rt.range.from_ms).toISOString(), "2026-11-08T23:00:00.000Z");
  assert.equal(new Date(rt.range.to_ms).toISOString(), "2026-11-09T22:59:59.999Z");
  assert.equal(doc.time.timezone, "Asia/Tokyo");
  // No zone of the URL: the zone of the document, as before.
  assert.equal(new Runtime({ uid: "d", doc, etag: "e", api: fakeApi(), widthOf: () => 1, range, now: () => NOW, zone: null }).zone, "Asia/Tokyo");
});
