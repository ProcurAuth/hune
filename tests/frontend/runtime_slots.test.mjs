// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-41: the scheduler of the queries of a panel (9.8d). One slot for each visible query, 6 requests at most at the same time, a hidden
// query never starts, and a change of a variable runs only the slots whose query names it.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { ApiError } from "../../web/js/api.js";
import { failedRefs } from "../../web/js/queries.js";
import { MAX_IN_FLIGHT } from "../../web/js/runtime.js";
import { RuntimeWithNames as Runtime } from "./helpers.mjs";

const vectors = JSON.parse(readFileSync(new URL("./vectors/FE-41-slots.json", import.meta.url), "utf8")).vectors;
const FRAME = { columns: [{ name: "v", kind: "number", type: "x" }], values: [[1]], rows: 1 };
const tick = () => new Promise((r) => setTimeout(r, 0));
const settle = async () => {
  for (let i = 0; i < 4; i++) await tick();
};

const variable = (name, values) => ({ kind: "custom", name, label: null, hide: "none", options: values.map((v) => ({ text: v, value: v })), multi: false, include_all: false, all_value: null, default: [values[0]] });

function documentOf(spec) {
  const panels = Object.entries(spec).map(([id, queries], i) => {
    const sql = (q) => [`SELECT 1${q.uses.map((n) => ` \${${n}}`).join("")}`];
    const [first, ...rest] = queries;
    return {
      id: Number(id), type: "table", title: `p${id}`, description: "", grid: { x: 0, y: i * 2, w: 24, h: 2 }, transforms: [], links: [], fields: { defaults: {}, overrides: [] }, options: {},
      data: {
        query: { source: "s", format: "table", sql: sql(first), min_interval: null, max_data_points: null, ...(first.ref !== "A" ? { ref: first.ref } : {}), ...(first.hide ? { hide: true } : {}) },
        ...(rest.length ? { extra_queries: rest.map((q) => ({ ref: q.ref, source: "s", format: "table", sql: sql(q), ...(q.hide ? { hide: true } : {}) })) } : {}),
      },
    };
  });
  return {
    format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off",
    time: { from: "now-6h", to: "now", timezone: "utc", refresh: null },
    variables: [variable("a", ["a", "b", "c"]), variable("b", ["x", "y"])],
    sections: [{ title: null, collapsed: false, panels }],
  };
}

// A fake API: a request waits until the test answers it. A request that the runtime aborts ends with the code `aborted`.
function fakeApi() {
  const calls = [];
  let flying = 0;
  const api = {
    calls,
    maxSeen: 0,
    panelQuery(uid, id, body, signal) {
      return new Promise((resolve, reject) => {
        flying += 1;
        api.maxSeen = Math.max(api.maxSeen, flying);
        const call = { id, ref: body.ref, body, done: false };
        const finish = () => {
          call.done = true;
          flying -= 1;
        };
        call.resolve = () => { finish(); resolve(FRAME); };
        call.fail = (error) => { finish(); reject(error); };
        signal.addEventListener("abort", () => { call.aborted = true; finish(); reject(new ApiError({ status: 0, code: "aborted", message: "x" })); });
        calls.push(call);
      });
    },
    async variableOptions() {
      return { options: [] };
    },
  };
  return api;
}

for (const v of vectors) {
  test(`FE-41 ${v.name}`, async () => {
    const doc = documentOf(v.panels);
    const hidden = new Set(Object.entries(v.panels).flatMap(([id, qs]) => qs.filter((q) => q.hide).map((q) => `${id}:${q.ref}`)));
    const api = fakeApi();
    const rt = new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 500, range: { fromText: "now-6h", toText: "now" } });
    let stale = false;
    rt.on((type) => {
      if (type === "stale") stale = true;
    });
    await rt.start();
    let seen = 0;
    for (const e of v.events) {
      if (e.do === "show") for (const id of e.ids) rt.setVisible(id, true);
      else if (e.do === "answer" || e.do === "fail") {
        const call = api.calls.find((c) => c.id === e.id && c.ref === e.ref && !c.done);
        assert.ok(call, `a pending request ${e.id}:${e.ref}`);
        if (e.do === "answer") call.resolve();
        else call.fail(new ApiError({ status: e.status, code: e.code, message: `${e.code} message` }));
      } else if (e.do === "range") rt.setRange("now-1h", "now");
      else if (e.do === "refresh") rt.refreshNow();
      else if (e.do === "select") rt.setSelection(e.name, { all: false, values: [e.value] });
      else if (e.do === "retry") rt.retry(e.id);
      else if (e.do === "view") rt.setViewPanel(e.id);
      await settle();
      const started = api.calls.slice(seen).map((c) => `${c.id}:${c.ref}`);
      seen = api.calls.length;
      assert.deepEqual(started, e.expect.started, `${e.do}: the requests that start`);
      for (const [id, state] of Object.entries(e.expect.states)) assert.equal(rt.panels.get(Number(id)).state, state, `${e.do}: state of panel ${id}`);
      if (e.expect.failed !== undefined) assert.equal(failedRefs(rt.panels.get(Number(Object.keys(v.panels)[0])).slots), e.expect.failed, `${e.do}: failed refs`);
      if (e.expect.stale) assert.ok(stale, `${e.do}: the event stale`);
    }
    assert.ok(api.maxSeen <= MAX_IN_FLIGHT, `${api.maxSeen} requests at the same time`);
    for (const c of api.calls) assert.ok(!hidden.has(`${c.id}:${c.ref}`), `no request for the hidden query ${c.id}:${c.ref}`);
    rt.destroy();
  });
}

test("FE-41 a panel with no extra query sends no ref, and the body of a copy of the panel members is the same for each query", async () => {
  const api = fakeApi();
  const rt = new Runtime({ uid: "d", doc: documentOf({ 1: [{ ref: "A", uses: [], hide: false }], 2: [{ ref: "A", uses: [], hide: false }, { ref: "B", uses: [], hide: false }] }), etag: "e", api, widthOf: () => 500, range: { fromText: "now-6h", toText: "now" } });
  await rt.start();
  rt.setVisible(1, true);
  rt.setVisible(2, true);
  await settle();
  assert.ok(!("ref" in api.calls[0].body), "the request of a panel with one query has no ref");
  const [b1, b2] = api.calls.filter((c) => c.id === 2).map((c) => c.body);
  assert.deepEqual([b1.ref, b2.ref], ["A", "B"]);
  const { ref: r1, ...rest1 } = b1;
  const { ref: r2, ...rest2 } = b2;
  assert.deepEqual(rest1, rest2, "the same range, the same max_data_points and the same variables");
  rt.destroy();
});
