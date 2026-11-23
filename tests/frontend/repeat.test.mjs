// FE-27: the repeat of panels and sections (4.13a, 9.8c): the values, the states and the limits, the layout, the keys, the request and the queries of a trigger.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { ApiError } from "../../web/js/api.js";
import {
  DEFAULT_MAX_PER_ROW, MAX_REPEAT_PER_DASHBOARD, MAX_REPEAT_PER_PANEL, MAX_REPEAT_PER_SECTION, bandOf, buildPlan, canRepeatBy, classify, copyVariables, displayedRows, extraRows,
  copyKeyOf, instanceKey, keyFromText, limitsOf, mentionsVariable, orderedValues, parseViewKey, planRepeats, repeatLimits, repeatMessage, sectionKey, sectionSignature, sectionTestid, splitKey, viewKeyFromParam,
} from "../../web/js/repeat.js";
import { normalSearch, parseView } from "../../web/js/view_state.js";
import { MAX_IN_FLIGHT } from "../../web/js/runtime.js";
import { RuntimeWithNames as Runtime } from "./helpers.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const vectors = JSON.parse(readFileSync(join(HERE, "vectors/FE-27-repeat.json"), "utf8"));
const tick = () => new Promise((r) => setTimeout(r, 0));

// --- helpers --------------------------------------------------------------------------------------------------------------------

const query = (n = 0) => ({ source: "s", format: "table", sql: [`SELECT ${n}`, "WHERE host IN (${host})"], min_interval: null, max_data_points: null });
const panel = (id, grid, extra = {}) => ({ id, type: "table", title: `p${id} \${host}`, description: "", transparent: false, grid, data: { query: query(id) }, transforms: [], fields: { defaults: {}, overrides: [] }, options: {}, links: [], ...extra });
const custom = (name, values, extra = {}) => ({ kind: "custom", name, label: null, hide: "none", options: values.map((v) => ({ text: v.toUpperCase(), value: v })), multi: true, include_all: true, all_value: null, default: ["$__all"], ...extra });
const doc = (variables, sections) => ({ format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off", time: { from: "now-6h", to: "now", timezone: "utc", refresh: null }, links: [], variables, sections });
const values = (list) => list.map((v) => ({ value: v, text: v.toUpperCase() }));

// A fake API: each panel query waits until the test releases it.
function fakeApi() {
  const calls = [];
  let open = 0;
  let max = 0;
  return {
    calls,
    get max() { return max; },
    get open() { return open; },
    panelQuery(uid, id, body, signal) {
      return new Promise((resolve, reject) => {
        open += 1;
        max = Math.max(max, open);
        const call = { id, body, signal, aborted: false, settled: false };
        call.resolve = () => { if (call.settled) return; call.settled = true; open -= 1; resolve({ columns: [], values: [], rows: 0 }); };
        signal.addEventListener("abort", () => { if (call.settled) return; call.settled = true; call.aborted = true; open -= 1; reject(new ApiError({ status: 0, code: "aborted", message: "x" })); });
        calls.push(call);
      });
    },
    options: [],
    variableOptions: async function (uid, name) { this.optionCalls = (this.optionCalls || 0) + 1; return { options: this.options }; },
  };
}
const range = { fromText: "now-6h", toText: "now" };
const makeRt = (d, api, urlVars = new Map()) => new Runtime({ uid: "d", doc: d, etag: "e", api, widthOf: () => 400, range, urlVars });
const showAll = (rt) => { for (const p of rt.panels.values()) rt.setVisible(p.key, true); };
async function finish(api, rounds = 30) {
  for (let i = 0; i < rounds; i++) {
    for (const c of api.calls) c.resolve();
    await tick();
  }
}

// --- the module -----------------------------------------------------------------------------------------------------------------------

test("repeat.js has no DOM access and no import, and its constants equal limits.rs and the table of 4.35", () => {
  const src = readFileSync(join(HERE, "../../web/js/repeat.js"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
  assert.ok(!/(?<![.\w])document\.|\b(window|navigator|localStorage|HTMLElement)\b/.test(src));
  assert.ok(!/^\s*import\b/m.test(src));
  const rust = readFileSync(join(HERE, "../../crates/hune-core/src/limits.rs"), "utf8");
  const read = (name) => Number(new RegExp(`pub const ${name}: usize = (\\d+);`).exec(rust)[1]);
  assert.equal(MAX_REPEAT_PER_PANEL, read("MAX_REPEAT_PER_PANEL"));
  assert.equal(MAX_REPEAT_PER_SECTION, read("MAX_REPEAT_PER_SECTION"));
  assert.equal(MAX_REPEAT_PER_DASHBOARD, read("MAX_REPEAT_PER_DASHBOARD"));
  assert.deepEqual([MAX_REPEAT_PER_PANEL, MAX_REPEAT_PER_SECTION, MAX_REPEAT_PER_DASHBOARD, DEFAULT_MAX_PER_ROW], [24, 12, 120, 4]);
  const spec = readFileSync(join(HERE, "../../docs/02-format-and-storage.md"), "utf8");
  for (const [name, n] of [["MAX_REPEAT_PER_PANEL", 24], ["MAX_REPEAT_PER_SECTION", 12], ["MAX_REPEAT_PER_DASHBOARD", 120]]) {
    const row = new RegExp(`\\| \`${name}\` \\| (\\d+) \\|`).exec(spec);
    if (row) assert.equal(Number(row[1]), n, `the table of 4.35 for ${name}`);
  }
});

test("FE-27 set 1: the values follow the order of the options", () => {
  for (const v of vectors.values) {
    assert.deepEqual(orderedValues(v.options, v.selection).map((o) => o.value), v.expect, v.name);
  }
  // The text of an option goes with its value.
  assert.deepEqual(orderedValues([{ value: "a", text: "Alpha" }], { all: true, values: [] }), [{ value: "a", text: "Alpha" }]);
});

test("a variable can repeat when it is a query or custom variable with multi", () => {
  assert.equal(canRepeatBy({ kind: "query", multi: true }), true);
  assert.equal(canRepeatBy({ kind: "custom", multi: true }), true);
  assert.equal(canRepeatBy({ kind: "custom", multi: false }), false);
  assert.equal(canRepeatBy({ kind: "text", multi: true }), false);
  assert.equal(canRepeatBy(undefined), false);
});

test("FE-27 set 2: the states and the limits, and the messages", () => {
  for (const s of vectors.states) {
    const limit = s.kind === "section" ? MAX_REPEAT_PER_SECTION : MAX_REPEAT_PER_PANEL;
    assert.equal(classify(s.k, limit), s.expect, s.name);
  }
  assert.equal(repeatMessage("empty", "panel", { label: "Host" }), "No value of Host is selected. Nothing to repeat.");
  assert.equal(repeatMessage("limit", "panel", { k: 25, limit: 24, label: "Host" }), "25 values of Host are selected. A panel repeats 24 times at most. Select fewer values.");
  assert.equal(repeatMessage("limit", "section", { k: 13, limit: 12, label: "Host" }), "13 values of Host are selected. A section repeats 12 times at most. Select fewer values.");
  assert.equal(repeatMessage("limit", "dashboard", { k: 3, limit: 120, label: "Host" }), "The repeats of this dashboard have more than 120 copies. Select fewer values of Host.");
});

test("FE-27 set 2: the walk of the budget of the dashboard", () => {
  for (const w of vectors.walks) {
    const variables = [];
    const lists = {};
    const sections = [];
    w.repeats.forEach(([kind, k, panels], i) => {
      const name = `v${i}`;
      variables.push(custom(name, ["x"]));
      lists[name] = k === 0 ? [] : values(Array.from({ length: k }, (_, j) => `o${j}`));
      if (kind === "panel") sections.push({ title: `S${i}`, collapsed: false, panels: [panel(i + 1, { x: 0, y: 0, w: 24, h: 2 }, { repeat: { variable: name, direction: "v" } })] });
      else sections.push({ title: `S${i}`, collapsed: false, repeat: { variable: name }, panels: Array.from({ length: panels }, (_, j) => panel(100 * (i + 1) + j, { x: 0, y: j * 2, w: 24, h: 2 })) });
    });
    const result = planRepeats(doc(variables, sections), (n) => lists[n], (n) => n);
    const got = w.repeats.map(([kind], i) => (kind === "panel" ? result.panels.get(i + 1) : result.sections.get(i)));
    assert.deepEqual(got.map((e) => e.state), w.expect, w.name);
    assert.deepEqual(got.map((e) => (e.state === "limit" ? e.scope : null)), w.scope, w.name);
    // The same result at each call: the walk never depends on a scroll or on the order of the answers.
    const again = planRepeats(doc(variables, sections), (n) => lists[n], (n) => n);
    assert.deepEqual([...again.panels.values(), ...again.sections.values()].map((e) => e.state).sort(), [...result.panels.values(), ...result.sections.values()].map((e) => e.state).sort(), w.name);
  }
});

test("a repeat that waits for its options is loading, a missing variable is repeat-empty (ID-K86)", () => {
  const d = doc([custom("host", ["a"])], [{ title: null, collapsed: false, panels: [panel(1, { x: 0, y: 0, w: 24, h: 2 }, { repeat: { variable: "host", direction: "h" } }), panel(2, { x: 0, y: 2, w: 24, h: 2 }, { repeat: { variable: "gone", direction: "h" } })] }]);
  const loading = planRepeats(d, (n) => (n === "host" ? null : undefined), (n) => n);
  assert.equal(loading.panels.get(1).state, "loading");
  assert.equal(loading.panels.get(2).state, "empty");
  const plan = buildPlan(d, (n) => (n === "host" ? null : undefined), (n) => n);
  assert.equal(plan.loading, true);
  assert.equal(plan.instances.get(2).note.state, "repeat-empty");
  const first = plan.instances.get(1);
  assert.equal(first.loading, true);
  assert.equal(first.note, null);
  assert.deepEqual(first.grid, { x: 0, y: 0, w: 24, h: 2 });
});

test("FE-27 set 3: the band of direction h and the region of direction v", () => {
  assert.equal(vectors.layout.length, 40);
  for (const v of vectors.layout) {
    const name = `${v.dir} k=${v.k} m=${v.m ?? "absent"}`;
    if (v.dir === "h") {
      const b = bandOf(v.k, v.m, v.h);
      assert.deepEqual([b.c, b.q, b.rows], [v.c, v.q, v.rows], name);
      assert.equal(extraRows({ state: "ok", direction: "h", k: v.k, maxPerRow: v.m }, v.h), v.extra, name);
      // The plan puts the copy n in the column ((n - 1) mod c) and the row floor((n - 1) / c).
      const plan = buildPlan(doc([custom("host", ["x"])], [{ title: null, collapsed: false, panels: [panel(1, { x: 3, y: 5, w: 6, h: v.h }, { repeat: { variable: "host", direction: "h", ...(v.m === null ? {} : { max_per_row: v.m }) } })] }]), () => values(Array.from({ length: v.k }, (_, i) => `o${i}`)), (n) => n);
      const band = plan.sections[0].copies[0].items[0];
      if (v.k > MAX_REPEAT_PER_PANEL) {
        assert.equal(band.kind, "panel", name);
        assert.equal(band.instance.note.state, "repeat-limit", name);
        continue;
      }
      assert.equal(band.kind, "band", name);
      assert.deepEqual([band.c, band.q, band.rows, band.y], [v.c, v.q, v.rows, 5], name);
      assert.deepEqual(band.instances.map((_, i) => [i % band.c, Math.floor(i / band.c)]), v.cells, name);
      // The stored x and w do not change the view: the copies have the full width.
      assert.ok(band.instances.every((i) => i.grid.x === 0 && i.grid.w === 24 && i.grid.h === v.h));
    } else {
      assert.equal(extraRows({ state: "ok", direction: "v", k: v.k }, v.h), v.extra, name);
      const plan = buildPlan(doc([custom("host", ["x"])], [{ title: null, collapsed: false, panels: [panel(1, { x: 4, y: 3, w: 8, h: v.h }, { repeat: { variable: "host", direction: "v" } })] }]), () => values(Array.from({ length: v.k }, (_, i) => `o${i}`)), (n) => n);
      const items = plan.sections[0].copies[0].items;
      if (v.k > MAX_REPEAT_PER_PANEL) {
        assert.equal(items[0].instance.note.state, "repeat-limit", name);
        continue;
      }
      assert.deepEqual(items.map((it) => it.instance.grid.y - 3), v.firsts, name);
      assert.ok(items.every((it) => it.instance.grid.x === 4 && it.instance.grid.w === 8));
    }
  }
});

test("FE-27 set 3: the moves of the other items, with the measure of Grafana 12.4.2", () => {
  for (const m of vectors.moves) {
    const items = m.items.map((it) => ({ id: it.id, grid: { x: it.x, y: it.y, w: it.w, h: it.h }, entry: it.rep ? { state: it.rep.state || "ok", direction: it.rep.dir, k: it.rep.k, maxPerRow: it.rep.m ?? null } : null }));
    const got = displayedRows(items);
    assert.deepEqual(Object.fromEntries([...got.entries()].map(([k, v]) => [String(k), v])), m.expect, m.name);
  }
  // The measure: the tops of the panels 5 to 8 in pixels are 341, 645, 341 and 721 (the top of the row 0 is at 113, a row unit and its gap are 38).
  const grafana = vectors.moves[0];
  const got = displayedRows(grafana.items.map((it) => ({ id: it.id, grid: { x: it.x, y: it.y, w: it.w, h: it.h }, entry: it.rep ? { state: "ok", direction: it.rep.dir, k: it.rep.k, maxPerRow: null } : null })));
  assert.deepEqual([5, 6, 7, 8].map((id) => 113 + got.get(id) * 38), [341, 645, 341, 721]);
});

test("the plan puts the copies in the order of the grid, with the keys of 9.8c", () => {
  const d = doc([custom("host", ["x"])], [{ title: null, collapsed: false, panels: [
    panel(1, { x: 0, y: 0, w: 24, h: 2 }, { repeat: { variable: "host", direction: "h", max_per_row: 2 } }),
    panel(2, { x: 0, y: 2, w: 12, h: 2 }),
    panel(3, { x: 12, y: 2, w: 12, h: 2 }, { repeat: { variable: "host", direction: "v" } }),
    panel(4, { x: 12, y: 4, w: 12, h: 2 }),
  ] }]);
  const plan = buildPlan(d, () => values(["a", "b", "c"]), (n) => n);
  assert.deepEqual([...plan.instances.keys()], [1, "1-r2", "1-r3", 2, 3, "3-r2", "3-r3", 4]);
  assert.deepEqual(plan.order.map((i) => i.key), [1, "1-r2", "1-r3", 2, 3, "3-r2", "3-r3", 4]);
  // The band of the panel 1 has 2 rows: the panels below are 2 row units lower. The panel 4 is below the region of the panel 3.
  assert.equal(plan.instances.get(2).grid.y, 4);
  assert.equal(plan.instances.get(3).grid.y, 4);
  assert.equal(plan.instances.get("3-r3").grid.y, 8);
  assert.equal(plan.instances.get(4).grid.y, 4 + 2 + 4);
  const second = plan.instances.get("1-r2");
  assert.deepEqual([second.n, second.count, second.variable, second.value, second.text, second.band], [2, 3, "host", "b", "B", 1]);
  assert.equal(plan.instances.get(2).n, null);
  assert.equal(sectionSignature(plan.sections[0]), sectionSignature(buildPlan(d, () => values(["a", "b", "c"]), (n) => n).sections[0]));
  assert.notEqual(sectionSignature(plan.sections[0]), sectionSignature(buildPlan(d, () => values(["a", "b"]), (n) => n).sections[0]));
});

test("a repeated section has one section for each value, with the keys of the panels of the copy", () => {
  const d = doc([custom("host", ["x"])], [
    { title: null, collapsed: false, panels: [panel(1, { x: 0, y: 0, w: 24, h: 2 })] },
    { title: "Host ${host}", collapsed: true, repeat: { variable: "host" }, panels: [panel(2, { x: 0, y: 0, w: 12, h: 2 }), panel(3, { x: 12, y: 0, w: 12, h: 2 })] },
  ]);
  const plan = buildPlan(d, () => values(["a", "b", "c"]), (n) => n);
  assert.deepEqual(plan.sections[1].copies.map((c) => [c.n, c.key, c.testid, c.value]), [[1, 1, "section-1", "a"], [2, "1-r2", "section-1-r2", "b"], [3, "1-r3", "section-1-r3", "c"]]);
  assert.deepEqual([...plan.instances.keys()], [1, 2, 3, "2-r2", "3-r2", "2-r3", "3-r3"]);
  assert.equal(plan.instances.get("2-r2").value, "b");
  assert.equal(plan.instances.get(2).count, 3);
  assert.equal(plan.sections[0].copies.length, 1);
  // The first section is never a copy. The cost of the section is k times its panels.
  assert.equal(plan.entries.sections.get(1).cost, 6);
  const empty = buildPlan(d, () => [], (n) => n);
  assert.equal(empty.sections[1].copies.length, 1);
  assert.equal(empty.sections[1].copies[0].note.state, "repeat-empty");
  assert.equal(empty.instances.has(2), false);
});

test("FE-27 set 4: the keys and the URL forms", () => {
  for (const [text, expect, written] of vectors.keys.forms) {
    assert.deepEqual(parseViewKey(text), expect, text);
    assert.equal(viewKeyFromParam(text), written, text);
    assert.deepEqual(splitKey(written), expect, `split ${written}`);
  }
  for (const text of vectors.keys.refused) {
    assert.equal(parseViewKey(text), null, JSON.stringify(text));
    assert.equal(viewKeyFromParam(text), null, JSON.stringify(text));
  }
  assert.equal(instanceKey(7, 1), 7);
  assert.equal(instanceKey(7, 3), "7-r3");
  assert.equal(sectionKey(2, 1), 2);
  assert.equal(sectionKey(2, 4), "2-r4");
  assert.equal(sectionTestid(2, 1), "section-2");
  assert.equal(sectionTestid(2, 4), "section-2-r4");
  assert.equal(keyFromText("7"), 7);
  assert.equal(keyFromText("7-r2"), "7-r2");
  const d = doc([custom("host", ["x"])], [{ title: "S", collapsed: false, repeat: { variable: "host" }, panels: [panel(1, { x: 0, y: 0, w: 24, h: 2 })] }, { title: "T", collapsed: false, panels: [panel(2, { x: 0, y: 0, w: 24, h: 2 }, { repeat: { variable: "host", direction: "v" } }), panel(3, { x: 0, y: 2, w: 24, h: 2 })] }]);
  assert.deepEqual([...repeatLimits(d)], [[1, MAX_REPEAT_PER_SECTION], [2, MAX_REPEAT_PER_PANEL]]);
  assert.equal(mentionsVariable("Host ${host}", "host"), true);
  assert.equal(mentionsVariable("Host ${host:text}", "host"), true);
  assert.equal(mentionsVariable("Host $host!", "host"), true);
  assert.equal(mentionsVariable("Host $hostname", "host"), false);
  assert.equal(mentionsVariable("Host", "host"), false);
});

// The form `<x>$panel-<id>` of Grafana names the copy by its value, not by its number (10.2a, rules 1 and 16; ID-K92, Grafana 12.4.2).
const grafanaDoc = (options) => doc([{ ...custom("host", []), options }], [
  { title: null, collapsed: false, panels: [panel(1, { x: 0, y: 0, w: 24, h: 2 }, { repeat: { variable: "host", direction: "h" } }), panel(3, { x: 0, y: 2, w: 24, h: 2 })] },
  { title: "S ${host}", collapsed: false, repeat: { variable: "host" }, panels: [panel(2, { x: 0, y: 0, w: 24, h: 2 })] },
]);
const grafanaCtx = (d, plan) => ({ canEdit: false, panels: [1, 2, 3], repeatLimits: repeatLimits(d), plan });

test("FE-27 set 4: the form of Grafana names the copy whose value is <x>: exact match, never an index", () => {
  const g = vectors.keys.grafana;
  for (const c of g.cases) {
    const options = g.options[c.options].map(([value, text]) => ({ value, text }));
    const d = grafanaDoc(options);
    const selection = c.selection === "all" ? { all: true, values: [] } : { all: false, values: c.selection };
    const plan = buildPlan(d, () => orderedValues(options, selection), (n) => n);
    const { id, value } = parseViewKey(c.view);
    assert.equal(copyKeyOf(plan, id, value), c.key, `${c.name}: copyKeyOf`);
    // The state of the URL and its normal form: the key of the copy, or the state view with the notice of rule 4.
    const search = `?viewPanel=${encodeURIComponent(c.view)}`;
    const got = parseView(search, grafanaCtx(d, plan));
    if (c.key === null) {
      assert.deepEqual([got.state.page, got.notices], ["view", [{ code: "no-panel", id: c.view }]], c.name);
      assert.equal(normalSearch(search, got.state), "", `${c.name}: the name goes`);
    } else {
      assert.deepEqual([got.state.page, got.state.panel, got.notices], ["panel-view", c.key, []], c.name);
      assert.equal(normalSearch(search, got.state), `?viewPanel=${c.key}`, `${c.name}: the page writes the key`);
      assert.equal(plan.instances.get(c.key).value, value, `${c.name}: the copy has the value`);
    }
  }
});

test("FE-27 set 4: while the values load, the form of Grafana keeps the state panel-view and the URL as it is (rule 16)", () => {
  const options = vectors.keys.grafana.options.text.map(([value, text]) => ({ value, text }));
  const d = grafanaDoc(options);
  const loading = buildPlan(d, () => null, (n) => n);
  assert.equal(copyKeyOf(loading, 1, "b"), undefined);
  assert.equal(copyKeyOf(loading, 2, "b"), undefined, "a section that waits for its values");
  for (const view of ["b$panel-1", "z$panel-2", "2$panel-1"]) {
    const search = `?viewPanel=${encodeURIComponent(view)}`;
    const got = parseView(search, grafanaCtx(d, loading));
    assert.deepEqual([got.state.page, got.state.panel, got.notices], ["panel-view", view, []], view);
    assert.equal(normalSearch(search, got.state), search, `${view}: no write of the URL`);
  }
  // A panel with no repeat never waits: the form names no panel at once. Without a plan, the form names no panel.
  assert.equal(parseView("?viewPanel=b%24panel-3", grafanaCtx(d, loading)).state.page, "view");
  assert.equal(parseView("?viewPanel=b%24panel-1", { canEdit: false, panels: [1], repeatLimits: repeatLimits(d) }).state.page, "view");
});

test("FE-27 set 5: the request of a copy fixes the repeat variable and changes nothing else", () => {
  for (const c of vectors.request.cases) {
    const before = JSON.stringify(vectors.request.variables);
    assert.deepEqual(copyVariables(vectors.request.variables, c.variable, c.value), c.expect, c.name);
    assert.equal(JSON.stringify(vectors.request.variables), before, "the member of the page is not changed");
  }
});

// --- the queries of a trigger (set 6) -----------------------------------------------------------------------------------------------

const hostDoc = (k, direction = "v", more = []) => doc([custom("host", Array.from({ length: k }, (_, i) => `o${i}`)), custom("env", ["p"], { multi: false, include_all: false, default: ["p"] })], [{ title: null, collapsed: false, panels: [panel(1, { x: 0, y: 0, w: 24, h: 4 }, { repeat: { variable: "host", direction } }), panel(2, { x: 0, y: 4, w: 24, h: 2 }), ...more] }]);

test("FE-27 set 6: k copies send k requests, each with its own value, and the other panels send their request", async () => {
  const api = fakeApi();
  const rt = makeRt(hostDoc(3), api);
  await rt.start();
  showAll(rt);
  await tick();
  assert.equal(api.calls.length, 4);
  const copies = api.calls.filter((c) => c.id === 1);
  assert.deepEqual(copies.map((c) => c.body.variables.host), [{ all: false, values: ["o0"] }, { all: false, values: ["o1"] }, { all: false, values: ["o2"] }]);
  assert.ok(copies.every((c) => JSON.stringify(c.body.variables.env) === JSON.stringify({ all: false, values: ["p"] })));
  // The panel with no repeat keeps the selection of the page: All gives all the values.
  assert.deepEqual(api.calls.find((c) => c.id === 2).body.variables.host, { all: true, values: [] });
  assert.deepEqual([...rt.panels.keys()], [1, "1-r2", "1-r3", 2]);
  rt.destroy();
});

test("FE-27 set 6: 24 copies send 6 requests at most at the same time, and 24 in the end", async () => {
  const api = fakeApi();
  const rt = makeRt(hostDoc(24), api);
  await rt.start();
  showAll(rt);
  await tick();
  assert.equal(api.calls.filter((c) => c.id === 1).length, MAX_IN_FLIGHT);
  await finish(api);
  assert.equal(api.calls.filter((c) => c.id === 1).length, 24);
  assert.ok(api.max <= MAX_IN_FLIGHT, `${api.max} requests at the same time`);
  rt.destroy();
});

test("FE-27 set 6: only the copies near the viewport query, and a scroll queries the others", async () => {
  const api = fakeApi();
  const rt = makeRt(hostDoc(8), api);
  await rt.start();
  for (const key of [1, "1-r2", "1-r3"]) rt.setVisible(key, true);
  await tick();
  assert.deepEqual(api.calls.map((c) => c.body.variables.host.values[0]), ["o0", "o1", "o2"]);
  for (const key of ["1-r4", "1-r5"]) rt.setVisible(key, true);
  await tick();
  assert.equal(api.calls.length, 5);
  rt.destroy();
});

test("FE-27 set 6: the states repeat-empty and repeat-limit send no request", async () => {
  for (const [k, state] of [[0, "repeat-empty"], [25, "repeat-limit"]]) {
    const api = fakeApi();
    // A query variable with no row has no option: nothing to repeat. 25 options with All give 25 values.
    api.options = Array.from({ length: k }, (_, i) => ({ value: `o${i}`, text: `O${i}` }));
    const query = { kind: "query", name: "host", label: "Host", hide: "none", source: "s", sql: ["SELECT 1"], multi: true, include_all: true, all_value: null, refresh: "load", sort: "none", regex: null, default: ["$__all"] };
    const d = doc([query], [{ title: null, collapsed: false, panels: [panel(1, { x: 0, y: 0, w: 24, h: 4 }, { repeat: { variable: "host", direction: "h" } }), panel(2, { x: 0, y: 4, w: 24, h: 2 })] }]);
    const rt = makeRt(d, api);
    await rt.start();
    showAll(rt);
    await tick();
    assert.equal(api.calls.filter((c) => c.id === 1).length, 0, `${state}: no query for the repeat`);
    assert.equal(api.calls.filter((c) => c.id === 2).length, 1, `${state}: the panel with no repeat queries`);
    const rec = rt.panels.get(1);
    assert.equal(rec.note.state, state);
    assert.equal(rec.note.message, k === 0 ? "No value of Host is selected. Nothing to repeat." : "25 values of Host are selected. A panel repeats 24 times at most. Select fewer values.");
    assert.equal(rt.panels.size, 2);
    rt.destroy();
  }
});

test("FE-27 set 6: a repeat of an unknown variable or of a variable that cannot repeat is repeat-empty and sends no request", async () => {
  for (const name of ["gone", "env"]) {
    const api = fakeApi();
    const d = hostDoc(3);
    d.sections[0].panels[0].repeat.variable = name;
    const rt = makeRt(d, api);
    await rt.start();
    showAll(rt);
    await tick();
    assert.equal(api.calls.filter((c) => c.id === 1).length, 0, `${name}: no query for the repeat`);
    assert.equal(api.calls.filter((c) => c.id === 2).length, 1, `${name}: the panel with no repeat queries`);
    assert.equal(rt.panels.get(1).note.state, "repeat-empty", name);
    assert.equal(rt.panels.get(1).note.message, `No value of ${name} is selected. Nothing to repeat.`);
    rt.destroy();
  }
});

test("FE-27 set 6: a new list of values aborts the old requests and starts the new copies", async () => {
  const api = fakeApi();
  const rt = makeRt(hostDoc(4), api);
  await rt.start();
  showAll(rt);
  await tick();
  const old = api.calls.filter((c) => c.id === 1);
  assert.equal(old.length, 4);
  rt.setSelection("host", { all: false, values: ["o3", "o1"] });
  await tick();
  // The copies of o1 (1) and o3 (3) are the new list [o1, o3]: o0 and o2 leave, and the key 1 and 1-r2 now hold other values.
  assert.deepEqual([...rt.panels.keys()], [1, "1-r2", 2]);
  assert.equal(old.filter((c) => c.aborted).length, 4, "each old request is aborted");
  for (const key of [1, "1-r2"]) rt.setVisible(key, true);
  await tick();
  const fresh = api.calls.filter((c) => c.id === 1 && !c.aborted);
  assert.deepEqual(fresh.map((c) => c.body.variables.host.values[0]), ["o1", "o3"]);
  // A refresh runs each copy one time.
  const before = api.calls.length;
  rt.refreshNow();
  await tick();
  assert.equal(api.calls.length - before, 3);
  rt.destroy();
});

test("FE-27 set 6: a copy keeps its result when the list changes and its key and value stay", async () => {
  const api = fakeApi();
  const rt = makeRt(hostDoc(3), api);
  await rt.start();
  showAll(rt);
  await tick();
  await finish(api, 3);
  const first = rt.panels.get(1);
  assert.equal(first.state, "ready");
  rt.setSelection("host", { all: false, values: ["o0", "o1"] });
  await tick();
  assert.equal(rt.panels.get(1), first, "the first copy has the same record");
  assert.equal(rt.panels.has("1-r3"), false);
  rt.destroy();
});

test("FE-27 set 6: the options of a query variable load first, and the repeat has no value before", async () => {
  const api = fakeApi();
  let release;
  api.variableOptions = (uid, name) => new Promise((resolve) => { release = () => resolve({ options: [{ value: "a", text: "A" }, { value: "b", text: "B" }] }); });
  const d = doc([{ kind: "query", name: "host", label: null, hide: "none", source: "s", sql: ["SELECT 1"], multi: true, include_all: true, all_value: null, refresh: "load", sort: "none", regex: null, default: ["$__all"] }], [{ title: null, collapsed: false, panels: [panel(1, { x: 0, y: 0, w: 24, h: 4 }, { repeat: { variable: "host", direction: "h" } })] }]);
  const rt = makeRt(d, api);
  assert.equal(rt.plan.loading, true);
  assert.equal(rt.panels.get(1).loading, true);
  assert.equal(rt.panels.get(1).note, null);
  const started = rt.start();
  showAll(rt);
  await tick();
  assert.equal(api.calls.length, 0, "no request before the options");
  // The view of a copy that waits for the options has a record in the state idle.
  const view = rt.ensureView("1-r2");
  assert.equal(view.provisional, true);
  assert.equal(view.state, "idle");
  release();
  await started;
  showAll(rt);
  await tick();
  assert.equal(rt.plan.loading, false);
  assert.deepEqual([...rt.panels.keys()], [1, "1-r2"]);
  assert.equal(rt.panels.get("1-r2").provisional, false);
  assert.equal(api.calls.length, 2);
  rt.destroy();
});

test("FE-27 set 6: the view of the form of Grafana waits for the options with the frame of its panel, then the copy of its value queries alone (rule 16)", async () => {
  const api = fakeApi();
  let release;
  api.variableOptions = () => new Promise((resolve) => { release = () => resolve({ options: [{ value: "a", text: "A" }, { value: "b", text: "B" }] }); });
  const d = doc([{ kind: "query", name: "host", label: null, hide: "none", source: "s", sql: ["SELECT 1"], multi: true, include_all: true, all_value: null, refresh: "load", sort: "none", regex: null, default: ["$__all"] }], [{ title: null, collapsed: false, panels: [panel(1, { x: 0, y: 0, w: 24, h: 4 }, { repeat: { variable: "host", direction: "h" } })] }]);
  const rt = makeRt(d, api);
  const started = rt.start();
  rt.setViewPanel("b$panel-1");
  await tick();
  // The frame of the view is the record of the panel: it waits too, and the runtime keeps no record under the text of the URL.
  assert.equal(rt.ensureView("b$panel-1"), rt.panels.get(1));
  assert.equal(rt.panels.get(1).loading, true);
  assert.equal(rt.panels.has("b$panel-1"), false);
  assert.equal(copyKeyOf(rt.plan, 1, "b"), undefined);
  release();
  await started;
  await tick();
  assert.equal(api.calls.length, 0, "no copy queries while the view names the form of Grafana");
  assert.equal(copyKeyOf(rt.plan, 1, "b"), "1-r2");
  rt.setViewPanel("1-r2");
  await tick();
  assert.deepEqual(api.calls.map((c) => [c.id, c.body.variables.host.values]), [[1, ["b"]]], "the copy of the value b queries alone");
  rt.destroy();
});

test("FE-27 set 6: a repeated section sends the queries of its panels for each copy, and a collapsed copy sends none", async () => {
  const api = fakeApi();
  const d = doc([custom("host", ["a", "b", "c"])], [{ title: "H ${host}", collapsed: false, repeat: { variable: "host" }, panels: [panel(1, { x: 0, y: 0, w: 12, h: 2 }), panel(2, { x: 12, y: 0, w: 12, h: 2 })] }]);
  const rt = makeRt(d, api);
  await rt.start();
  rt.setExpanded("0-r2", false);
  showAll(rt);
  await tick();
  // 3 copies and 2 panels: 6 panels, and the copy 2 is collapsed.
  assert.equal(rt.panels.size, 6);
  assert.equal(api.calls.length, 4);
  assert.deepEqual(api.calls.map((c) => [c.id, c.body.variables.host.values[0]]).sort(), [[1, "a"], [1, "c"], [2, "a"], [2, "c"]]);
  assert.equal(rt.interpolateWith("H ${host}", { variable: "host", value: "b", text: "B" }), "H B");
  assert.equal(rt.interpolate("p ${host}", "2-r2"), "p B");
  rt.setExpanded("0-r2", true);
  await tick();
  assert.equal(api.calls.length, 6);
  rt.destroy();
});

test("FE-27 set 6: Retry on a copy runs this copy only", async () => {
  const api = fakeApi();
  const rt = makeRt(hostDoc(3), api);
  await rt.start();
  showAll(rt);
  await tick();
  await finish(api, 3);
  const before = api.calls.length;
  rt.retry("1-r2");
  await tick();
  assert.equal(api.calls.length - before, 1);
  assert.equal(api.calls[api.calls.length - 1].body.variables.host.values[0], "o1");
  rt.destroy();
});

test("the functions run in 50 ms or less for a document of 200 panels and 120 copies", () => {
  const panels = [];
  for (let i = 0; i < 200; i++) {
    const repeat = i < 5 ? { repeat: { variable: "host", direction: i % 2 ? "h" : "v", max_per_row: i % 2 ? 4 : undefined } } : {};
    if (repeat.repeat && repeat.repeat.max_per_row === undefined) delete repeat.repeat.max_per_row;
    panels.push(panel(i + 1, { x: (i % 2) * 12, y: i * 2, w: 12, h: 2 }, repeat));
  }
  const d = doc([custom("host", ["x"])], [{ title: null, collapsed: false, panels }]);
  const list = values(Array.from({ length: 24 }, (_, i) => `o${i}`));
  const run = () => {
    const plan = buildPlan(d, () => list, (n) => n);
    displayedRows(panels.map((p) => ({ id: p.id, grid: p.grid, entry: plan.entries.panels.get(p.id) || null })));
    sectionSignature(plan.sections[0]);
    return plan;
  };
  const plan = run();
  assert.equal([...plan.entries.panels.values()].reduce((a, e) => a + e.cost, 0), 120);
  // The minimum of 7 runs after 2 warm-up runs: one run on a loaded machine measures the machine, not the code.
  run();
  run();
  let ms = Infinity;
  for (let i = 0; i < 7; i++) {
    const t0 = performance.now();
    run();
    ms = Math.min(ms, performance.now() - t0);
  }
  assert.ok(ms <= 50, `${ms} ms`);
});

test("the limits come from the session: the constants are the defaults of an old server only", async () => {
  assert.deepEqual(limitsOf(null), { panel: 24, section: 12, dashboard: 120 });
  assert.deepEqual(limitsOf({}), { panel: 24, section: 12, dashboard: 120 });
  assert.deepEqual(limitsOf({ repeat_limits: { panel: 3, section: 2, dashboard: 5 } }), { panel: 3, section: 2, dashboard: 5 });
  // A member that is absent has its default. A member that is present and not a whole number from 1 to its hard bound fails closed (0).
  assert.deepEqual(limitsOf({ repeat_limits: { panel: 3 } }), { panel: 3, section: 12, dashboard: 120 });
  assert.deepEqual(limitsOf({ repeat_limits: { panel: 0, section: "2", dashboard: 5.5 } }), { panel: 0, section: 0, dashboard: 0 });
  assert.deepEqual(limitsOf({ repeat_limits: { panel: 201, section: 51, dashboard: 201 } }), { panel: 0, section: 0, dashboard: 0 });
  assert.deepEqual(limitsOf({ repeat_limits: { panel: 200, section: 50, dashboard: 200 } }), { panel: 200, section: 50, dashboard: 200 });
  assert.deepEqual(limitsOf({ repeat_limits: { panel: null, section: -1, dashboard: NaN } }), { panel: 0, section: 0, dashboard: 0 });
  for (const bad of [null, "x", 5, []]) assert.deepEqual(limitsOf({ repeat_limits: bad }), bad && typeof bad === "object" ? { panel: 24, section: 12, dashboard: 120 } : { panel: 0, section: 0, dashboard: 0 }, JSON.stringify(bad));
  const closed = buildPlan(hostDoc(2), () => values(["a", "b"]), (n) => n, { limits: limitsOf({ repeat_limits: { panel: 0, section: 12, dashboard: 120 } }) });
  assert.equal(closed.instances.get(1).note.state, "repeat-limit");
  assert.equal(closed.instances.get(1).note.message, "The server gave a repeat limit that is not valid. Nothing repeats.");
  assert.equal(closed.instances.size, 2);
  const limits = { panel: 3, section: 2, dashboard: 5 };
  const d = hostDoc(4);
  const plan = buildPlan(d, () => values(["a", "b", "c", "d"]), (n) => n, { limits });
  assert.equal(plan.instances.get(1).note.state, "repeat-limit");
  assert.equal(plan.instances.get(1).note.message, "4 values of host are selected. A panel repeats 3 times at most. Select fewer values.");
  const ok = buildPlan(d, () => values(["a", "b", "c"]), (n) => n, { limits });
  assert.equal(ok.entries.panels.get(1).state, "ok");
  const api = fakeApi();
  const rt = new Runtime({ uid: "d", doc: hostDoc(3), etag: "e", api, widthOf: () => 400, range, limits });
  await rt.start();
  showAll(rt);
  await tick();
  assert.equal(api.calls.filter((c) => c.id === 1).length, 3);
  const small = new Runtime({ uid: "d", doc: hostDoc(4), etag: "e", api: fakeApi(), widthOf: () => 400, range, limits });
  assert.equal(small.panels.get(1).note.state, "repeat-limit");
  const dash = planRepeats(doc([custom("a", ["x"]), custom("b", ["x"])], [{ title: "S", collapsed: false, panels: [panel(1, { x: 0, y: 0, w: 24, h: 2 }, { repeat: { variable: "a", direction: "v" } }), panel(2, { x: 0, y: 2, w: 24, h: 2 }, { repeat: { variable: "b", direction: "v" } })] }]), (n) => values(["p", "q", "r"]), (n) => n, limits);
  assert.deepEqual([dash.panels.get(1).state, dash.panels.get(2).state, dash.panels.get(2).scope], ["ok", "limit", "dashboard"]);
  rt.destroy();
  small.destroy();
});
