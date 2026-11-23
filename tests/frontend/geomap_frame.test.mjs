// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-85, security review 3, finding F1 (9.19i, T106): a map keeps its view and its failed tiles through a new draw.
// The test drives the real panel frame (panel_frame.js): each refresh stores a new frame object, as the runtime does, so the frame draws the
// map again. The ResizeObserver calls back once after observe(), as browsers do. Each tile fails, as with a host with no CORS header.
// A failed tile is not asked again in the same view, also after a refresh. A pan, a zoom, a new size and new view options start again.
import test from "node:test";
import assert from "node:assert/strict";
import { FakeNode, installFakeDom } from "./fake_dom.mjs";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let W = 1000;
const H = 600;
const urls = [];
const observers = [];

installFakeDom();
globalThis.document.documentElement = new FakeNode("html");
Object.defineProperty(FakeNode.prototype, "clientWidth", { configurable: true, get() { return W; } });
Object.defineProperty(FakeNode.prototype, "clientHeight", { configurable: true, get() { return H; } });
FakeNode.prototype.getContext = function getContext() {
  const g = {};
  for (const m of ["setTransform", "clearRect", "beginPath", "moveTo", "lineTo", "stroke", "fill", "drawImage", "arc"]) g[m] = () => {};
  return g;
};
FakeNode.prototype.setPointerCapture = () => {};
Object.defineProperty(FakeNode.prototype, "classList", {
  configurable: true,
  get() {
    const n = this;
    const list = () => n.className.split(/\s+/).filter(Boolean);
    return {
      add: (...c) => { n.className = [...new Set([...list(), ...c])].join(" "); },
      remove: (...c) => { n.className = list().filter((x) => !c.includes(x)).join(" "); },
      contains: (c) => list().includes(c),
      toggle: (c, on) => { n.className = (on ?? !list().includes(c)) ? [...list(), c].join(" ") : list().filter((x) => x !== c).join(" "); },
    };
  },
});
globalThis.window = { devicePixelRatio: 1 };
globalThis.getComputedStyle = () => ({ getPropertyValue: () => "#123456", paddingLeft: "0", paddingRight: "0" });
globalThis.ResizeObserver = class {
  constructor(cb) { this.cb = cb; observers.push(this); }
  observe() { queueMicrotask(() => this.cb([])); }
  disconnect() { this.cb = () => {}; }
};
globalThis.IntersectionObserver = class {
  constructor(cb) { this.cb = cb; }
  observe() { queueMicrotask(() => this.cb([{ isIntersecting: true }])); }
  disconnect() {}
};
globalThis.Image = class {
  set src(v) {
    this._src = v;
    if (v) {
      urls.push(v);
      setTimeout(() => this.onerror && this.onerror(), 5);
    }
  }
  get src() { return this._src; }
  removeAttribute() { this._src = undefined; }
};
globalThis.createImageBitmap = async () => ({ close() {} });

const { app } = await import("../../web/js/config.js");
app.session = { geomap: { tile_url: "https://tile.example.org/{z}/{x}/{y}.png", attribution: "OSM", max_zoom: 19 } };
const { createPanelFrame } = await import("../../web/js/panel_frame.js");

const VIEW = { mode: "fixed", lat: 48.85, lon: 2.35, zoom: 5 };
function mapFrame(uid, id = 1) {
  const panel = {
    id, type: "geomap", title: "Map", grid: { x: 0, y: 0, w: 24, h: 10 }, links: [], transforms: [], fields: { defaults: {}, overrides: [] },
    data: { query: { ref: "A", source: "ch", code: "SELECT 1" } },
    options: { view: VIEW, layer: { lat_field: null, lon_field: null, size: 5, size_min: 2, size_max: 15, size_field: null, color: "green", color_field: null, opacity: 40 }, tooltip: "details", show_zoom: true, mouse_wheel_zoom: true },
  };
  const makeFrame = () => ({ columns: [{ name: "lat", type: "Float64", kind: "number" }, { name: "lon", type: "Float64", kind: "number" }], values: [[48.85], [2.35]] });
  const rec = { id, panel, state: "ready", frame: makeFrame(), error: null };
  const rt = { panels: new Map([[id, rec]]), interpolate: (v) => v, retry() {} };
  const page = { rt, zone: "utc", linkContext: () => ({ dashboardUid: uid }), panelUrl: () => "", viewPanel() {}, onZoom() {}, syncKey: "" };
  const frame = createPanelFrame({ panel, page });
  const hook = () => JSON.parse(frame.el.querySelector(".panel__plot").dataset.geomap);
  // A refresh: the runtime stores the new answer, a new object, and the frame draws again. The result is the count of tile requests.
  const refresh = async (change = () => {}) => {
    const before = urls.length;
    rec.frame = makeFrame();
    change(panel);
    frame.update();
    await sleep(400); // the delay of 200 ms and the failures of all the tiles, 6 at a time
    return urls.length - before;
  };
  // An action on the drawn map (a key, a new size). The result is the count of tile requests after it.
  const act = async (fn) => {
    const before = urls.length;
    fn(frame.el.querySelector(".geo__canvas"));
    await sleep(400);
    return urls.length - before;
  };
  return { frame, hook, refresh, act };
}

test("FE-85 frame: a failed tile is asked one time only in a view, also through the refreshes (new frame objects)", async () => {
  const m = mapFrame("dash-a");
  const counts = [];
  for (let i = 0; i < 5; i++) counts.push(await m.refresh());
  const needed = m.hook().tiles.needed;
  assert.equal(needed, 20);
  assert.deepEqual(counts, [20, 0, 0, 0, 0], "tile requests at each refresh of the same view");
  assert.equal(new Set(urls).size, urls.length, "no URL twice");
  m.frame.destroy();
});

test("FE-85 frame: a pan survives a refresh; a pan, a zoom, a new size and new view options make a new view", async () => {
  const m = mapFrame("dash-b");
  assert.equal(await m.refresh(), 20);
  // Four arrow keys pan by 256 px: a new view, so its tiles are asked one time, and the next refresh keeps the view and asks for nothing.
  const panned = await m.act((canvas) => { for (let i = 0; i < 4; i++) canvas.dispatch("keydown", { key: "ArrowRight", preventDefault() {} }); });
  assert.equal(panned, 20);
  const lon = m.hook().lon;
  assert.ok(lon > 13 && lon < 14, `the panned centre: ${lon}`);
  assert.equal(await m.refresh(), 0);
  assert.equal(m.hook().lon, lon, "the view after the refresh is the panned view");
  // A zoom: a new view.
  assert.equal(await m.act((canvas) => canvas.dispatch("keydown", { key: "+", preventDefault() {} })), 20);
  assert.equal(m.hook().zoom, 6);
  assert.equal(await m.refresh(), 0);
  assert.equal(m.hook().zoom, 6);
  // A real new size: a new view. The first callback of the next draw comes at the kept size, so the refresh asks for nothing.
  W = 1300;
  const resized = await m.act(() => { for (const o of observers) o.cb([]); });
  assert.equal(resized, m.hook().tiles.needed);
  assert.ok(resized > 20, `the wider view: ${resized}`);
  assert.equal(await m.refresh(), 0);
  W = 1000;
  await m.act(() => { for (const o of observers) o.cb([]); });
  // New view options start at the new start view.
  const fresh = await m.refresh((panel) => { panel.options = { ...panel.options, view: { ...VIEW, zoom: 4 } }; });
  assert.equal(m.hook().zoom, 4);
  assert.equal(m.hook().lon, 2.35);
  assert.equal(fresh, m.hook().tiles.needed);
  assert.equal(await m.refresh(), 0);
  m.frame.destroy();
});

test("FE-85 frame: the same panel key in another dashboard has its own view", async () => {
  const a = mapFrame("dash-c", 3);
  await a.refresh();
  await a.act((canvas) => canvas.dispatch("keydown", { key: "ArrowDown", preventDefault() {} }));
  a.frame.destroy();
  const b = mapFrame("dash-d", 3);
  await b.refresh();
  assert.equal(b.hook().lat, 48.85, "the start view of the other dashboard");
  b.frame.destroy();
});
