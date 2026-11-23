// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-85 (9.19i, 4.33g, 13.13): the map panel. The grammar of the tile URL (the list that the Rust test of the configuration
// reads too), Web Mercator and its clamp, the tiles of a view and their cap, the URL of a tile, the queue of the requests, the cache, the rows
// with no valid position, MAX_GEO_POINTS, the size of a marker, and a render with a fake DOM: hostile names stay text, and the tile requests
// carry only z, x and y, with crossOrigin "anonymous" and referrerPolicy "no-referrer".
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  GEO_TILE_DELAY_MS, Lru, MAX_GEO_POINTS, MAX_GEO_TILES_IN_FLIGHT, MAX_GEO_TILES_VIEW, MAX_GEO_TILE_CACHE, MAX_GEO_ZOOM, MAX_LAT, TILE, TileQueue,
  checkTileUrl, clampLat, fillAlpha, fitView, numberAt, positionFields, project, readPositions, sizeScale, tileRange, tileSource, tilesOf, tileUrl, unproject,
} from "../../web/js/geo_model.js";
import { FakeNode, installFakeDom } from "./fake_dom.mjs";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-85-geomap.json", import.meta.url), "utf8"));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

test("FE-85: the constants are the caps of 4.35 and 9.19i", () => {
  assert.deepEqual({ TILE, MAX_LAT, MAX_GEO_POINTS, MAX_GEO_TILES_VIEW, MAX_GEO_TILES_IN_FLIGHT, GEO_TILE_DELAY_MS, MAX_GEO_TILE_CACHE, MAX_GEO_ZOOM }, v.constants);
});

test("FE-85: the grammar of the tile URL is the grammar of the server (13.13)", () => {
  for (const [text, source] of v.tile_url.valid) {
    const c = checkTileUrl(text);
    assert.ok(c, `valid: ${text}`);
    assert.equal(c.text, text);
    assert.ok(source.startsWith(c.origin), `${text}: the origin ${c.origin}`);
  }
  for (const text of v.tile_url.invalid) assert.equal(checkTileUrl(text), null, `refused: ${JSON.stringify(text)}`);
  for (const bad of [null, undefined, 42, {}, ["https://a.example/{z}/{x}/{y}"]]) assert.equal(checkTileUrl(bad), null);
});

test("FE-85: the session gives the tile source only with a valid template, attribution and zoom (6.10)", () => {
  const ok = { tile_url: "https://tiles.example.org/{z}/{x}/{y}.png", attribution: "Tiles: example.org", max_zoom: 19 };
  assert.deepEqual(tileSource({ geomap: ok }), { template: ok.tile_url, origin: "https://tiles.example.org", attribution: ok.attribution, maxZoom: 19 });
  for (const geomap of [null, undefined, "x", { ...ok, tile_url: "http://tiles.example.org/{z}/{x}/{y}.png" }, { ...ok, max_zoom: 23 }, { ...ok, max_zoom: -1 }, { ...ok, max_zoom: 1.5 }, { ...ok, attribution: "" }, { ...ok, attribution: 7 }]) {
    assert.equal(tileSource({ geomap }), null, JSON.stringify(geomap));
  }
  assert.equal(tileSource(null), null);
  assert.equal(tileSource({}), null);
});

test("FE-85: the URL of a tile holds the three integers in range only", () => {
  const t = "https://tiles.example.org/osm/{z}/{x}/{y}.png?key=public";
  assert.equal(tileUrl(t, 3, 5, 7), "https://tiles.example.org/osm/3/5/7.png?key=public");
  assert.equal(tileUrl(t, 0, 0, 0), "https://tiles.example.org/osm/0/0/0.png?key=public");
  assert.equal(tileUrl("https://a.example/{y}/{x}/{z}", 22, 4194303, 1), "https://a.example/1/4194303/22");
  for (const [z, x, y] of [[-1, 0, 0], [23, 0, 0], [1, 2, 0], [1, 0, 2], [1, -1, 0], [1.5, 0, 0], [1, 0.5, 0], [NaN, 0, 0], ["1", 0, 0], [1, "$&", 0]]) {
    assert.equal(tileUrl(t, z, x, y), null, `${z}/${x}/${y}`);
  }
});

test("FE-85: Web Mercator and its clamp at 85.0511", () => {
  for (const p of v.project) {
    const r = project(p.lat, p.lon, p.z);
    assert.ok(Math.abs(r.x - p.x) < 0.5 && Math.abs(r.y - p.y) < 0.5, `${p.lat}, ${p.lon} at ${p.z}: ${r.x}, ${r.y}`);
  }
  assert.equal(clampLat(90), MAX_LAT);
  assert.equal(clampLat(-90), -MAX_LAT);
  assert.equal(clampLat(12.5), 12.5);
  for (const [lat, lon, z] of [[0, 0, 0], [48.8566, 2.3522, 10], [-33.86, 151.2, 5], [60, -150, 3]]) {
    const p = project(lat, lon, z);
    const back = unproject(p.x, p.y, z);
    assert.ok(Math.abs(back.lat - lat) < 1e-9 && Math.abs(back.lon - lon) < 1e-9, `${lat}, ${lon}`);
  }
});

test("FE-85: the tiles of a view, the cap of 256 tiles and the order from the centre", () => {
  for (const t of v.tiles) {
    const r = tileRange(t.cx, t.cy, t.z, t.w, t.h);
    assert.deepEqual({ x0: r.x0, x1: r.x1, y0: r.y0, y1: r.y1, count: r.count, over: r.over }, { x0: t.x0, x1: t.x1, y0: t.y0, y1: t.y1, count: t.count, over: t.over }, JSON.stringify(t));
    const list = tilesOf(r);
    assert.equal(list.length, t.over ? 0 : t.count, "no list above the cap");
  }
  const list = tilesOf(tileRange(2048, 2048, 4, 1000, 600));
  const near = (a) => (a.x - 7.5) ** 2 + (a.y - 7.5) ** 2;
  for (let i = 1; i < list.length; i++) assert.ok(near(list[i - 1]) <= near(list[i]));
  // A view outside the world needs no tile.
  assert.equal(tileRange(-5000, 128, 0, 300, 300).count, 0);
});

const num = (name, values) => ({ name, kind: "number", values: Float64Array.from(values.map((x) => (x === null ? NaN : x))), nulls: Uint8Array.from(values.map((x) => (x === null ? 1 : 0))), config: {} });
const txt = (name, values, kind = "text") => ({ name, kind, values, config: {} });

test("FE-85: the position fields by name, by the default names and by kind", () => {
  const fields = [txt("name", ["a"]), num("Latitude", [1]), num("LNG", [2])];
  assert.deepEqual(positionFields(fields, ["name", "Latitude", "LNG"], { lat_field: null, lon_field: null }), { lat: 1, lon: 2 });
  // A display name in the set counts as Grafana counts it, and a chosen name is an exact display name.
  assert.deepEqual(positionFields([num("a", [1]), num("b", [2])], ["lat", "lon"], { lat_field: null, lon_field: null }), { lat: 0, lon: 1 });
  assert.deepEqual(positionFields([num("y", [1]), num("x", [2])], ["Y", "X"], { lat_field: "Y", lon_field: "X" }), { lat: 0, lon: 1 });
  assert.match(positionFields([num("y", [1]), num("x", [2])], ["Y", "X"], { lat_field: "y", lon_field: "X" }).error, /No field has the name y\./);
  assert.match(positionFields([num("x", [1])], ["x"], { lat_field: null, lon_field: null }).error, /no latitude field/);
  assert.match(positionFields([num("lat", [1])], ["lat"], { lat_field: null, lon_field: null }).error, /no longitude field/);
  assert.match(positionFields([txt("lat", ["1"]), num("lon", [1])], ["lat", "lon"], { lat_field: null, lon_field: null }).error, /not a number field/);
  assert.deepEqual(positionFields([txt("lat", ["1"], "number_text"), num("lon", [1])], ["lat", "lon"], { lat_field: null, lon_field: null }), { lat: 0, lon: 1 });
});

test("FE-85: a row with no valid position is skipped and counted (the number grammar of 7.7)", () => {
  const lat = num("lat", [0, null, 91, -90, 45, NaN, Infinity, 10]);
  const lon = num("lon", [0, 1, 0, 180, 181, 0, 0, -180]);
  const p = readPositions(lat, lon, 8);
  assert.deepEqual([...p.ok], [1, 0, 0, 1, 0, 0, 0, 1]);
  assert.equal(p.bad, 5);
  const t = txt("lat", ["1.5", "1e3", "-0", " 2", "0x10", "3.", "-12.25", null], "number_text");
  assert.deepEqual([0, 1, 2, 3, 4, 5, 6, 7].map((r) => numberAt(t, r)), [1.5, NaN, -0, NaN, NaN, NaN, -12.25, NaN]);
  // A field shorter than the frame: the rows past its end have no position.
  const short = readPositions(num("lat", [1]), num("lon", [1, 2]), 2);
  assert.deepEqual([...short.ok], [1, 0]);
});

test("FE-85: the size of a marker is linear in the field range, as in Grafana 12.4.2", () => {
  const layer = { size: 5, size_min: 2, size_max: 15 };
  assert.equal(sizeScale(null, layer)(0), 5);
  const f = num("v", [10, 50, 100, 30, null, 200, 0]);
  f.config = { min: 10, max: 100 };
  const s = sizeScale(f, layer);
  assert.deepEqual([0, 1, 2, 3, 4, 5, 6].map((r) => Math.round(s(r) * 1000) / 1000), [2, 7.778, 15, 4.889, 2, 15, 2]);
  const flat = num("v", [3, 3]);
  flat.config = { min: 3, max: 3 };
  assert.equal(sizeScale(flat, layer)(0), 2);
  assert.equal(sizeScale(f, { size: 5, size_min: 9, size_max: 9 })(2), 9);
  // The fill alpha is the square of the opacity (measured: 0.4 gives 0.16, 0.8 gives 0.64, 1 gives 1).
  assert.ok(Math.abs(fillAlpha(40) - 0.16) < 1e-12 && Math.abs(fillAlpha(80) - 0.64) < 1e-12 && fillAlpha(100) === 1 && fillAlpha(0) === 0);
});

test("FE-85: the view of all the markers", () => {
  const p = readPositions(num("lat", [48.85, 51.5, 40.7]), num("lon", [2.35, -0.12, -74]), 3);
  const view = fitView(p, 800, 400, 15);
  assert.ok(view.zoom >= 1 && view.zoom <= 3, JSON.stringify(view));
  const one = fitView(readPositions(num("lat", [10]), num("lon", [20]), 1), 800, 400, 7);
  assert.equal(one.zoom, 7);
  assert.ok(Math.abs(one.lat - 10) < 1e-9 && Math.abs(one.lon - 20) < 1e-9);
  assert.equal(fitView(readPositions(num("lat", [null]), num("lon", [1]), 1), 800, 400, 7), null);
});

test("FE-85: the cache keeps the most recently used tiles, 256 at most", () => {
  const c = new Lru(3);
  c.set("a", 1);
  c.set("b", 2);
  c.set("c", 3);
  assert.equal(c.get("a"), 1);
  c.set("d", 4);
  assert.deepEqual([...c.map.keys()], ["c", "a", "d"]);
  assert.equal(c.has("b"), false);
  assert.equal(c.size, 3);
  // The callback gets each value that leaves: the least recently used one, and a value that a new value of its key replaces.
  const dropped = [];
  const d = new Lru(2, (x) => dropped.push(x));
  d.set("a", 1);
  d.set("b", 2);
  d.set("a", 3);
  d.set("c", 4);
  d.set("c", 4);
  assert.deepEqual(dropped, [1, 2]);
  assert.deepEqual([...d.map.entries()], [["a", 3], ["c", 4]]);
});

test("FE-85: the queue has 6 requests in flight at most, and drops the jobs of an owner that wait", () => {
  const q = new TileQueue(MAX_GEO_TILES_IN_FLIGHT);
  const running = [];
  const a = {};
  const b = {};
  for (let i = 0; i < 20; i++) q.push({ owner: i < 10 ? a : b, start: (done) => running.push(done) });
  assert.equal(q.active, 6);
  assert.equal(running.length, 6);
  q.drop(b);
  for (let i = 0; i < 6; i++) running.shift()();
  assert.equal(q.active, 4, "4 jobs of the owner a were left");
  while (running.length) running.shift()();
  assert.equal(q.active, 0);
  assert.equal(q.peak, 6);
  // A second `done` of one job counts once.
  let d = null;
  q.push({ owner: a, start: (done) => { d = done; } });
  d();
  d();
  assert.equal(q.active, 0);
});

// --- the render with a fake DOM -------------------------------------------------------------------------------------------------------

function fakePage({ w = 600, h = 300, session = null } = {}) {
  installFakeDom();
  const images = [];
  const ops = [];
  Object.defineProperty(FakeNode.prototype, "clientWidth", { configurable: true, get() { return w; } });
  Object.defineProperty(FakeNode.prototype, "clientHeight", { configurable: true, get() { return h; } });
  FakeNode.prototype.getContext = function getContext() {
    const g = { ops };
    for (const m of ["setTransform", "clearRect", "beginPath", "moveTo", "lineTo", "stroke", "fill", "drawImage"]) g[m] = (...args) => ops.push([m, ...args]);
    g.arc = (...args) => ops.push(["arc", ...args]);
    return g;
  };
  FakeNode.prototype.setPointerCapture = () => {};
  globalThis.window = { devicePixelRatio: 1 };
  globalThis.getComputedStyle = () => ({ getPropertyValue: () => "#123456" });
  globalThis.ResizeObserver = class { observe() {} disconnect() {} };
  globalThis.IntersectionObserver = class { constructor(cb) { this.cb = cb; globalThis.lastObserver = this; } observe() { this.cb([{ isIntersecting: true }]); } disconnect() {} };
  globalThis.Image = class {
    constructor() { this.attrs = new Map(); images.push(this); }
    set src(v) { this._src = v; }
    get src() { return this._src; }
    removeAttribute(n) { if (n === "src") this._src = undefined; }
  };
  // The decoded bitmap of an image (F5 of the security review 3): the test records each one and its `close`.
  globalThis.createImageBitmap = async (img) => {
    const bitmap = new FakeBitmap(img.src);
    bitmaps.push(bitmap);
    return bitmap;
  };
  return { images, ops };
}

class FakeBitmap {
  constructor(from) { this.from = from; this.width = TILE; this.height = TILE; this.closed = false; }
  close() { this.closed = true; }
}
const bitmaps = [];

async function renderMap(fields, names, options, session = null, size = {}, extra = {}) {
  const page = fakePage(size);
  const { app } = await import("../../web/js/config.js");
  app.session = session;
  const { render } = await import("../../web/js/panels/geomap.js");
  const host = new FakeNode("div");
  const panel = { id: 7, type: "geomap", title: "<b>Map</b>", options };
  const handle = render(host, { panel, fields, names, zone: "utc", ...extra });
  return { ...page, host, handle };
}

const OPTIONS = { view: { mode: "fixed", lat: 0, lon: 0, zoom: 1 }, layer: { lat_field: null, lon_field: null, size: 5, size_min: 2, size_max: 15, size_field: null, color: "dark-green", color_field: null, opacity: 40 }, tooltip: "details", show_zoom: true, mouse_wheel_zoom: true };
const HOSTILE = ["<img src=x onerror=alert(1)>", "‮evil", "a\u0000b", "${__all}", "</div><script>alert(1)</script>", "&amp;&lt;", "😀".repeat(3)];

test("FE-85 render: the plain background, the markers, the note of the skipped rows, and hostile names that stay text", async () => {
  const fields = [num("lat", [0, 10, null]), num("lon", [0, 20, 5]), ...HOSTILE.map((n) => txt(n, [n, n, n]))];
  const names = fields.map((f) => f.name);
  const { host, ops, images, handle } = await renderMap(fields, names, OPTIONS);
  const hook = JSON.parse(host.dataset.geomap);
  assert.equal(hook.mode, "plain");
  assert.equal(hook.points, 2);
  assert.equal(hook.skipped, 1);
  assert.equal(hook.tiles, null);
  assert.equal(ops.filter((o) => o[0] === "arc").length, 2, "two markers");
  assert.ok(ops.some((o) => o[0] === "stroke"), "the graticule");
  assert.equal(images.length, 0, "no tile source: no request");
  assert.equal(handle.note, "1 row has no valid position.");
  // The pointer on the marker at (0, 0): the card lists each field with textContent.
  const root = host.children[0];
  const canvas = root.querySelector(".geo__canvas");
  canvas.dispatch("pointermove", { clientX: 300, clientY: 150 });
  const card = root.querySelector(".geo__card");
  assert.equal(card.hidden, false);
  const rowsOfCard = card.querySelectorAll(".geo__row").map((r) => [r.children[0].textContent, r.children[1].textContent]);
  assert.equal(rowsOfCard.length, fields.length);
  for (const n of HOSTILE) assert.ok(rowsOfCard.some(([a, b]) => a === n && b === n), `the name ${JSON.stringify(n)} stays text`);
  assert.ok(!card.all().some((node) => node.tag === "img" || node.tag === "script"), "no element from a text");
  canvas.dispatch("pointerleave", {});
  assert.equal(card.hidden, true);
  handle.destroy();
});

test("FE-85 render: MAX_GEO_POINTS is checked before any position is read, with no picture", async () => {
  const many = MAX_GEO_POINTS + 1;
  const lat = { name: "lat", kind: "number", get values() { return { length: many }; }, config: {} };
  const { host, handle, ops } = await renderMap([lat, num("lon", [0])], ["lat", "lon"], OPTIONS);
  assert.equal(handle.state, "geo-limit");
  assert.equal(host.children[0].textContent, "Too many points: 50,001. The limit is 50,000.");
  assert.equal(ops.length, 0);
  const atCap = await renderMap([num("lat", new Array(MAX_GEO_POINTS).fill(1)), num("lon", new Array(MAX_GEO_POINTS).fill(1))], ["lat", "lon"], OPTIONS);
  assert.equal(atCap.handle.state, undefined);
  assert.equal(JSON.parse(atCap.host.dataset.geomap).points, MAX_GEO_POINTS);
  atCap.handle.destroy();
});

test("FE-85 render: tiles as images with no cookie and no Referer, z/x/y only, the caps, no retry in a view, and a tile of another size dropped", async () => {
  const session = { geomap: { tile_url: "https://tiles.example.org/osm/{z}/{x}/{y}.png", attribution: "<b>Tiles</b> & co", max_zoom: 5 } };
  const opts = { ...OPTIONS, view: { mode: "fit", lat: 0, lon: 0, zoom: 3 } };
  const { host, images, handle } = await renderMap([num("lat", [40]), num("lon", [-70])], ["lat", "lon"], opts, session, { w: 1000, h: 600 });
  const root = host.children[0];
  assert.equal(root.querySelector(".geo__attribution").textContent, "<b>Tiles</b> & co");
  assert.equal(root.querySelector(".geo__attribution").children.length, 0, "the attribution is text, with no link");
  assert.equal(images.length, 0, "no request before 200 ms");
  await sleep(GEO_TILE_DELAY_MS + 60);
  const hook = JSON.parse(host.dataset.geomap);
  // With a tile source the view `fit` does not apply: the view is the fixed centre (0, 0) at zoom 3.
  assert.equal(hook.zoom, 3);
  assert.equal(hook.lat, 0);
  assert.equal(hook.tiles.needed, 16);
  assert.equal(images.length, 6, "6 requests in flight at most");
  for (const img of images) {
    assert.equal(img.crossOrigin, "anonymous");
    assert.equal(img.referrerPolicy, "no-referrer");
    assert.match(img.src, /^https:\/\/tiles\.example\.org\/osm\/3\/[0-7]\/[0-7]\.png$/);
  }
  // A failure, a tile of 512 px and a good tile.
  images[0].onerror();
  images[1].naturalWidth = 512;
  images[1].naturalHeight = 512;
  images[1].onload();
  images[2].naturalWidth = 256;
  images[2].naturalHeight = 256;
  images[2].onload();
  assert.equal(images.length, 9, "three slots free: three more requests");
  await sleep(0); // the decode of the good tile
  const after = JSON.parse(host.dataset.geomap);
  assert.equal(after.tiles.failed, 2);
  assert.equal(after.tiles.loaded, 1);
  assert.equal(after.tiles.drawn, 1);
  // The panel leaves the viewport and comes back: the same view, so the two failed tiles are not tried again. The requests that the
  // exit cancelled are made again.
  const failedUrls = [images[0].src, images[1].src];
  const loadedUrl = images[2].src;
  globalThis.lastObserver.cb([{ isIntersecting: false }]);
  const before = images.length;
  globalThis.lastObserver.cb([{ isIntersecting: true }]);
  await sleep(GEO_TILE_DELAY_MS + 60);
  const again = images.slice(before).map((img) => img.src);
  assert.equal(again.length, 6, "6 in flight again");
  for (const url of [...failedUrls, loadedUrl]) assert.ok(!again.includes(url), `no second request in the view: ${url}`);
  // A view of more than 256 tiles: the plain background and a note, and no request.
  handle.destroy();
  const big = await renderMap([num("lat", [40]), num("lon", [-70])], ["lat", "lon"], { ...OPTIONS, view: { mode: "fixed", lat: 0, lon: 0, zoom: 5 } }, session, { w: 5000, h: 4000 });
  await sleep(GEO_TILE_DELAY_MS + 60);
  const bigHook = JSON.parse(big.host.dataset.geomap);
  assert.equal(bigHook.tiles.over, true);
  assert.equal(big.images.length, 0);
  const tileNote = big.host.children[0].querySelector("[data-testid=geomap-7-tiles-note]");
  assert.equal(tileNote.hidden, false);
  assert.equal(tileNote.textContent, "The map needs 320 tiles. The limit is 256.");
  big.handle.destroy();
});

test("FE-85 render: a bad template in the session gives the plain background and no request", async () => {
  const session = { geomap: { tile_url: "https://tiles.example.org/osm/{z}/{x}/{y}.png;script-src", attribution: "x", max_zoom: 5 } };
  const { host, images, handle } = await renderMap([num("lat", [1]), num("lon", [1])], ["lat", "lon"], OPTIONS, session);
  await sleep(GEO_TILE_DELAY_MS + 60);
  assert.equal(JSON.parse(host.dataset.geomap).mode, "plain");
  assert.equal(images.length, 0);
  handle.destroy();
});

// Security review 3, finding F5 (T106): an image keeps its downloaded bytes, and a host can put any number of bytes in a tile of 256 x 256.
// The cache keeps the decoded bitmap, the image loses its source, and a bitmap that leaves the cache of 256 tiles is closed.
test("FE-85 render: the cache keeps decoded bitmaps, the image loses its source, and a bitmap that leaves the cache is closed", async () => {
  const session = { geomap: { tile_url: "https://bitmaps.example.org/{z}/{x}/{y}.png", attribution: "x", max_zoom: 8 } };
  const fields = [num("lat", [40]), num("lon", [-70])];
  // Each request gets a good tile of 256 px, until no request waits.
  const loadAll = async (images) => {
    for (let busy = true; busy;) {
      busy = false;
      for (const img of images) {
        if (img.src === undefined || img.done) continue;
        img.done = busy = true;
        img.naturalWidth = img.naturalHeight = TILE;
        img.onload();
      }
      await sleep(0);
    }
  };
  const start = bitmaps.length;
  const a = await renderMap(fields, ["lat", "lon"], { ...OPTIONS, view: { mode: "fixed", lat: 0, lon: 0, zoom: 3 } }, session, { w: 1000, h: 600 }, { key: "f5-a" });
  await sleep(GEO_TILE_DELAY_MS + 60);
  await loadAll(a.images);
  const mine = bitmaps.slice(start);
  assert.equal(mine.length, 16);
  assert.equal(JSON.parse(a.host.dataset.geomap).tiles.drawn, 16);
  assert.ok(a.images.every((img) => img.src === undefined), "each image lost its source after the decode");
  const drawn = a.ops.filter((op) => op[0] === "drawImage");
  assert.ok(drawn.length >= 16 && drawn.every((op) => op[1] instanceof FakeBitmap), "the canvas draws the bitmaps, not the images");
  a.handle.destroy();
  // A view of 256 tiles fills the cache: the 16 older bitmaps leave it, and each one is closed.
  const b = await renderMap(fields, ["lat", "lon"], { ...OPTIONS, view: { mode: "fixed", lat: 0, lon: 0, zoom: 5 } }, session, { w: 4096, h: 4096 }, { key: "f5-b" });
  await sleep(GEO_TILE_DELAY_MS + 60);
  await loadAll(b.images);
  assert.equal(JSON.parse(b.host.dataset.geomap).tiles.drawn, MAX_GEO_TILES_VIEW);
  assert.ok(mine.every((x) => x.closed), "the bitmaps that left the cache are closed");
  assert.equal(bitmaps.slice(start + 16).filter((x) => !x.closed).length, MAX_GEO_TILE_CACHE, "the cache keeps 256 open bitmaps");
  b.handle.destroy();
  // A new draw of the same map (a refresh) takes its tiles from the cache: no request.
  const again = await renderMap(fields, ["lat", "lon"], { ...OPTIONS, view: { mode: "fixed", lat: 0, lon: 0, zoom: 5 } }, session, { w: 4096, h: 4096 }, { key: "f5-b" });
  await sleep(GEO_TILE_DELAY_MS + 60);
  assert.equal(again.images.length, 0);
  assert.equal(JSON.parse(again.host.dataset.geomap).tiles.drawn, MAX_GEO_TILES_VIEW);
  again.handle.destroy();
});
