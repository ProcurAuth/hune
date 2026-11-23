// The transfer of the frames between the base grid and the draft grid (10.2a, rule 17; decision ID-K301, owner bug of 2026-11-15).
// The switch "Edit" and the panel editor keep the frames of the panels that the draft does not change: the draft grid takes the element of
// the base frame, with its chart and its last result, and returns it. The runtime keeps its records, and the page sends no query.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { RuntimeWithNames as Runtime } from "./helpers.mjs";
import { DraftRuntime } from "../../web/js/editor/draft_runtime.js";
import { frameAdopter } from "../../web/js/editor/adopt.js";
import { frameDataset } from "../../web/js/panel_frame.js";
import { updatePanel } from "../../web/js/editor/draft.js";

// A small tree of nodes: what adopt.js uses (parentNode, replaceWith, append, attributes, dataset, querySelector of a class).
class Node0 {
  constructor(cls = "") {
    this.cls = cls;
    this.parentNode = null;
    this.children = [];
    this.attrs = new Map();
    this.dataset = {};
  }
  detach() {
    if (this.parentNode) this.parentNode.children.splice(this.parentNode.children.indexOf(this), 1);
    this.parentNode = null;
  }
  append(...nodes) {
    for (const n of nodes) {
      n.detach();
      n.parentNode = this;
      this.children.push(n);
    }
  }
  replaceWith(n) {
    const p = this.parentNode;
    if (!p) return;
    n.detach();
    p.children[p.children.indexOf(this)] = n;
    n.parentNode = p;
    this.parentNode = null;
  }
  getAttribute(k) { return this.attrs.has(k) ? this.attrs.get(k) : null; }
  setAttribute(k, v) { this.attrs.set(k, String(v)); }
  removeAttribute(k) { this.attrs.delete(k); }
  querySelector(sel) {
    for (const c of this.children) {
      if (`.${c.cls}` === sel) return c;
      const r = c.querySelector(sel);
      if (r) return r;
    }
    return null;
  }
}
// The page hook of busy.js (data-hune-state) needs the root element.
globalThis.document = { createComment: () => new Node0("#comment"), documentElement: new Node0("html") };

const query = (sql) => ({ source: "s", format: "table", sql: [sql], min_interval: null, max_data_points: null });
const panel = (id, type, x) => ({ id, type, title: `Panel ${id}`, description: "", transparent: false, grid: { x, y: 0, w: 8, h: 6 }, transforms: [], links: [], fields: { defaults: {}, overrides: [] }, options: {}, data: { query: query(`SELECT ${id}`) } });
const doc = {
  format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off", links: [],
  time: { from: "now-6h", to: "now", timezone: "utc", refresh: null },
  variables: [], sections: [{ title: null, collapsed: false, panels: [panel(1, "timeseries", 0), panel(2, "barchart", 8), panel(3, "stat", 16)] }],
};

// A fake API: each panel query answers at once with a new frame object.
function fakeApi() {
  const calls = [];
  return {
    calls,
    async panelQuery(uid, id) {
      calls.push(id);
      return { columns: [], values: [], rows: 0, id };
    },
    async variableOptions() {
      return { options: [] };
    },
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

// A frame of the base grid: an element in a section grid, with a header that holds the menu button, and the hooks of panel_frame.js.
function baseFrame(rt, key, grid) {
  const rec = rt.panels.get(key);
  const el = new Node0("panel");
  Object.assign(el.dataset, frameDataset(rec.panel, rec.inst), { span: "half" });
  el.setAttribute("style", `grid-column: ${key} / span 8;`);
  const head = new Node0("panel__head");
  head.append(new Node0("panel__title"), new Node0("panel__menu"));
  el.append(head);
  grid.append(el);
  const log = [];
  return { el, log, page: "view", setPage(p) { this.page = p; log.push(`page ${p}`); }, resume() { log.push("resume"); }, suspend() { log.push("suspend"); } };
}

async function loaded() {
  const api = fakeApi();
  const rt = new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 400, range: { fromText: "now-6h", toText: "now" } });
  await rt.start();
  for (const key of [1, 2, 3]) rt.setVisible(key, true);
  await tick();
  await tick();
  const gridEl = new Node0("dash__grid");
  const frames = new Map([1, 2, 3].map((key) => [key, baseFrame(rt, key, gridEl)]));
  return { api, rt, gridEl, frames, pageApi: "view" };
}

test("the runtime keeps the records across the transfer: the draft mirrors the base results, and the visibility of the draft grid sends no query", async () => {
  const { api, rt } = await loaded();
  assert.deepEqual(api.calls, [1, 2, 3], "one query for each panel at the load");
  const records = new Map([...rt.panels].map(([k, r]) => [k, r.frame]));
  // Edit on: the draft runtime mirrors the records of the base runtime (the same frame objects).
  const draft = new DraftRuntime({ base: rt, doc: structuredClone(doc), onCollapse: () => {} });
  for (const key of [1, 2, 3]) {
    assert.equal(draft.panels.get(key).mirrored, true);
    assert.equal(draft.panels.get(key).state, "ready");
    assert.equal(draft.panels.get(key).frame, records.get(key), `the draft record ${key} has the base result`);
  }
  // The observer of the draft grid sees the frames, and leaves and sees one of them again.
  for (const key of [1, 2, 3]) draft.setVisible(key, true);
  draft.setVisible(2, false);
  draft.setVisible(2, true);
  // Edit off: the draft runtime goes, the base grid observes its frames again.
  draft.destroy();
  for (const key of [1, 2, 3]) rt.setVisible(key, true);
  await tick();
  assert.deepEqual(api.calls, [1, 2, 3], "no query: no data changed");
  for (const key of [1, 2, 3]) assert.equal(rt.panels.get(key).frame, records.get(key), `the base record ${key} keeps its result`);
  rt.destroy();
});

test("the draft grid takes the base frame of an unchanged panel, out of its place, and returns it with its place, its menu and its page", async () => {
  const { api, rt, gridEl, frames, pageApi } = await loaded();
  const draft = new DraftRuntime({ base: rt, doc: structuredClone(doc), onCollapse: () => {} });
  const adopter = frameAdopter({ api: { grid: { frames }, runtime: rt, pageApi }, draft, page: "edit" });
  const inst = (key) => draft.plan.instances.get(key);
  const editGrid = { frames: new Map() };
  const editEl = new Node0("edit");
  for (const key of [1, 2, 3]) {
    const frame = adopter.take(inst(key).panel, inst(key));
    assert.equal(frame, frames.get(key), `the frame of ${key} is the base frame`);
    assert.equal(frame.page, "edit", "the frame has the page of the edit mode");
    assert.deepEqual(frame.log, ["resume", "page edit"], "the frame runs again first, then it gets the page of the edit mode");
    editGrid.frames.set(key, frame);
    editEl.append(frame.el); // the draft grid places it
    frame.el.setAttribute("style", "grid-column: 1 / span 24;");
    frame.el.dataset.span = "full";
  }
  assert.equal(adopter.take(inst(1).panel, inst(1)), null, "a frame is taken once");
  assert.deepEqual(gridEl.children.map((n) => n.cls), ["#comment", "#comment", "#comment"], "a mark keeps the place of each frame in the base grid");
  // A copy n from 2 of the draft grid has no menu button: the draft grid removes it, and the return puts it again.
  frames.get(3).el.querySelector(".panel__menu").detach();
  adopter.giveBack(editGrid);
  assert.equal(editGrid.frames.size, 0, "the draft grid does not destroy the frames that it returns");
  assert.deepEqual(gridEl.children, [1, 2, 3].map((key) => frames.get(key).el), "each frame is at its place in the base grid");
  for (const key of [1, 2, 3]) {
    const f = frames.get(key);
    assert.equal(f.el.getAttribute("style"), `grid-column: ${key} / span 8;`, "the place of the base grid");
    assert.equal(f.el.dataset.span, "half");
    assert.equal(f.page, pageApi, "the page of the view");
    assert.deepEqual(f.log.slice(-2), ["page view", "suspend"], "the base grid stops it again until it shows");
    assert.ok(f.el.querySelector(".panel__menu"), "the menu button is in the frame again");
  }
  assert.deepEqual(api.calls, [1, 2, 3], "no query");
  rt.destroy();
});

test("a panel that the draft changes, or a copy with another value, gets a frame of the draft grid", async () => {
  const { rt, frames, pageApi } = await loaded();
  const draft = new DraftRuntime({ base: rt, doc: structuredClone(doc), onCollapse: () => {} });
  const adopter = frameAdopter({ api: { grid: { frames }, runtime: rt, pageApi }, draft, page: "edit" });
  // A new title: the record still mirrors the data, but the frame of the view draws the old title.
  draft.setDoc(updatePanel(doc, 2, (p) => ({ ...p, title: "New title" })));
  let inst = draft.plan.instances.get(2);
  assert.equal(adopter.take(inst.panel, inst), null, "a changed panel has a new frame");
  // A new query: the record does not mirror.
  draft.setDoc(updatePanel(doc, 3, (p) => ({ ...p, data: { query: query("SELECT 30") } })));
  inst = draft.plan.instances.get(3);
  assert.equal(draft.panels.get(3).mirrored, false);
  assert.equal(adopter.take(inst.panel, inst), null, "a changed query has a new frame");
  // The hooks differ (another copy): no take.
  inst = draft.plan.instances.get(1);
  frames.get(1).el.dataset.testid = "panel-1-r2";
  assert.equal(adopter.take(inst.panel, inst), null, "the hooks of the frame must be the hooks of the instance");
  // No base grid (the history view): no take.
  const none = frameAdopter({ api: { grid: null, runtime: rt, pageApi }, draft, page: "edit" });
  assert.equal(none.take(inst.panel, inst), null);
  rt.destroy();
});

test("the page code of the transfer: the frame follows its page, and a chart that returns to the page draws in its resize callback too (10.2a, rule 17)", () => {
  const frame = readFileSync(new URL("../../web/js/panel_frame.js", import.meta.url), "utf8");
  // The menu, the zoom and the marks read the page at the time of use, so a frame can change its page.
  assert.match(frame, /setPage\(next\) \{\s+page = next;\s+this\.annotationsChanged\(\);/);
  assert.match(frame, /onZoom: \(a, b\) => page\.onZoom\(a, b\)/);
  assert.match(frame, /!page\.noMarks &&/);
  const common = readFileSync(new URL("../../web/js/panels/common.js", import.meta.url), "utf8");
  assert.match(common, /if \(host\.clientWidth && gone\) \{\s+chart\.redraw\(\);\s+chart\.flush\(\);\s+\}\s+gone = !host\.clientWidth;/);
  const editor = readFileSync(new URL("../../web/js/editor/edit_mode.js", import.meta.url), "utf8");
  // The draft grid returns the frames before it closes and before each new build.
  assert.equal((editor.match(/adopter\.giveBack\(grid\);\s+if \(grid\) grid\.destroy\(\);/g) || []).length, 2);
  assert.match(editor, /adopter\.take\(panel, inst\) \|\| createPanelFrame\(/);
  assert.match(editor, /noMarks: true/);
});

// ID-K321 (owner answer of 2026-11-21): the page wrote the hook `data-hune-view` while the charts that came back from the panel editor had
// no pixels; they drew at the next resize callback. A task between the two (a script, or the probe of B207 under load) read blank canvases.
test("a chart that returns to the page draws in the task of the event hune-shown, before any resize callback (10.2a, rule 17)", async () => {
  const { fitHeight } = await import("../../web/js/panels/common.js");
  const on = new Map();
  const kept = { document: globalThis.document, ResizeObserver: globalThis.ResizeObserver };
  globalThis.document = {
    addEventListener: (type, fn) => on.set(type, [...(on.get(type) || []), fn]),
    removeEventListener: (type, fn) => on.set(type, (on.get(type) || []).filter((f) => f !== fn)),
  };
  let resize = null;
  globalThis.ResizeObserver = class {
    constructor(callback) { resize = callback; }
    observe() {}
    disconnect() {}
  };
  const shown = () => { for (const fn of on.get("hune-shown") || []) fn(); };
  try {
    const host = { clientWidth: 400, clientHeight: 200 };
    const calls = [];
    const chart = { root: { querySelector: () => null }, update: (o) => calls.push(`height ${o.height}`), redraw: () => calls.push("redraw"), flush: () => calls.push("flush") };
    const unfit = fitHeight(host, chart);
    assert.deepEqual(calls, ["height 200"]);
    shown();
    assert.deepEqual(calls, ["height 200"], "a chart that stays in the page does not draw again");
    // The grid leaves the page in the task of the change: the event comes before the resize callback, and the chart draws nothing.
    Object.assign(host, { clientWidth: 0, clientHeight: 0 });
    shown();
    assert.deepEqual(calls, ["height 200"], "a chart that leaves the page draws nothing at the event");
    resize(); // the host has no width: the engine releases the canvas
    assert.deepEqual(calls, ["height 200", "height 60"]);
    // The grid returns and the page shows the state: the chart draws at once, with no resize callback.
    Object.assign(host, { clientWidth: 400, clientHeight: 200 });
    shown();
    assert.deepEqual(calls.slice(2), ["height 200", "redraw", "flush"], "the chart draws in the task of the event");
    resize();
    assert.deepEqual(calls.length, 5, "the resize callback after it draws nothing more");
    unfit();
    assert.deepEqual(on.get("hune-shown"), [], "the destroy removes the listener");
  } finally {
    Object.assign(globalThis, kept);
  }
});

test("the page sends hune-shown before it writes the hook of the state, and each chart with a canvas listens (10.2a, rule 17)", () => {
  const read = (path) => readFileSync(new URL(`../../web/js/${path}`, import.meta.url), "utf8");
  assert.match(read("dashboard.js"), /function settle\(target\) \{[^}]*document\.dispatchEvent\(new Event\("hune-shown"\)\);[^}]*root\.setAttribute\("data-hune-view", target\.page\);/);
  const heat = read("panels/heatmap.js");
  assert.match(heat, /const shown = \(\) => gone && fit\(\);/);
  assert.match(heat, /document\.addEventListener\("hune-shown", shown\);[\s\S]*document\.removeEventListener\("hune-shown", shown\);/);
  for (const path of ["panels/timeline.js", "panels/piechart.js"]) {
    assert.match(read(path), /document\.addEventListener\("hune-shown", onTheme\);[\s\S]*document\.removeEventListener\("hune-shown", onTheme\);/, path);
  }
});
