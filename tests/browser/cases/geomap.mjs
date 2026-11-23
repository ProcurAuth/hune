// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B226 and B227 (9.19i, 12.8, 13.12c): the map panel.
// B226: no block `geomap`. The plain background, the markers, the card of a marker with hostile names as text, the zoom, and no request
// that leaves the page origin. The header is the header of 12.8, byte for byte.
// B227: a block `geomap` with a LOCAL https tile server on 127.0.0.1. A Chrome of the case alone maps the test name tiles.hune.test to
// 127.0.0.1 (its own flags) and gets no other host. The case checks each tile request: no Referer, no cookie, only z/x/y in the path,
// 6 requests in flight at most, no request during a pan and one 200 ms after it, a tile of 512 px dropped, and no tile from a host with
// no CORS header (fail closed). The security review 3 added: one request for each failed tile through refreshes of 10 s, a pan that
// survives a refresh (F1), and the memory of 24 tiles of 16 MiB (F5). All the data is synthetic. Nothing leaves 127.0.0.1.
import { createServer } from "node:https";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { deflateSync } from "node:zlib";
import { Browser } from "../lib/cdp.mjs";
import { WORK, freePort, sh } from "../lib/env.mjs";
import { sleep } from "../lib/h.mjs";

const CSP_12_8 = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";
const EVIL = "<img src=x onerror=window.__alerted=1>";
const TILE_HOST = "tiles.hune.test";

const OPTIONS = (view, extra = {}) => ({
  view,
  layer: { lat_field: null, lon_field: null, size: 5, size_min: 2, size_max: 15, size_field: null, color: "dark-green", color_field: null, opacity: 40, ...extra },
  tooltip: "details",
  show_zoom: true,
  mouse_wheel_zoom: true,
});

function geoPanel(id, title, sql, options, grid) {
  return {
    id, type: "geomap", title, description: "", transparent: false, grid,
    data: { query: { source: "prod", format: "table", sql, min_interval: null, max_data_points: null } },
    transforms: [], fields: { defaults: {}, overrides: [] }, options, links: [],
  };
}

function geoDoc(uid, panels) {
  return {
    format_version: 3, uid, title: "Geomap", description: "", tags: [],
    time: { from: "now-6h", to: "now", timezone: "utc", refresh: null }, cursor_sync: "off", links: [], variables: [],
    sections: [{ title: null, collapsed: false, panels }],
  };
}

// Paris, London, New York, (0, 0), and a row with no valid position. The name column has a hostile text.
const SQL = [
  "SELECT * FROM (",
  `  SELECT 48.85 AS lat, 2.35 AS lon, 10 AS v, '${EVIL}' AS name UNION ALL`,
  "  SELECT 51.5, -0.12, 50, 'London' UNION ALL",
  "  SELECT 40.7, -74.0, 100, 'New York' UNION ALL",
  "  SELECT 0, 0, 30, 'Zero' UNION ALL",
  "  SELECT 999, 0, 1, 'Bad'",
  ") ORDER BY v",
];

const hookOf = (p, id) => p.eval(`JSON.parse(document.querySelector('[data-testid="panel-${id}"] .panel__plot').dataset.geomap || "null")`);

/** The page position of a marker: the canvas centre plus the projected position minus the projected centre. */
const markerAt = (p, id, lat, lon) => p.eval(`(() => {
  const c = document.querySelector('[data-testid="geomap-${id}"] canvas');
  const h = JSON.parse(c.closest('.panel__plot').dataset.geomap);
  const P = (la, lo, z) => { const s = Math.sin(Math.max(-85.0511, Math.min(85.0511, la)) * Math.PI / 180); const n = 256 * 2 ** z; return [(lo + 180) / 360 * n, (0.5 - Math.log((1 + s) / (1 - s)) / (4 * Math.PI)) * n]; };
  const r = c.getBoundingClientRect();
  const a = P(${lat}, ${lon}, h.zoom), o = P(h.lat, h.lon, h.zoom);
  return [r.left + r.width / 2 + a[0] - o[0], r.top + r.height / 2 + a[1] - o[1]];
})()`);

// --- a tile server on 127.0.0.1 -------------------------------------------------------------------------------------------------------

const CRC = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
/** A PNG of one colour. `pad` bytes of a private chunk make the file larger with no change of the picture. */
function png(size, [r, g, b], pad = 0) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const row = Buffer.alloc(1 + size * 3);
  for (let x = 0; x < size; x++) row.set([r, g, b], 1 + x * 3);
  const raw = Buffer.concat(Array.from({ length: size }, () => row));
  const extra = pad ? [chunk("prVt", Buffer.alloc(pad, 0x41))] : [];
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", ihdr), ...extra, chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

// The resident memory (MiB) of the renderer processes of a Chrome of the case (Linux /proc). The pids come from the browser itself.
async function rendererMiB(browser) {
  const { processInfo } = await browser.send("SystemInfo.getProcessInfo");
  const pids = processInfo.filter((x) => x.type === "renderer").map((x) => x.id);
  let kib = 0;
  for (const pid of pids) kib += Number(/VmRSS:\s+(\d+)/.exec(readFileSync(`/proc/${pid}/status`, "utf8"))[1]);
  return { pids: pids.length, mib: Math.round(kib / 1024) };
}

async function tileServer(dir) {
  mkdirSync(dir, { recursive: true });
  const key = join(dir, "key.pem");
  const cert = join(dir, "cert.pem");
  const r = sh("openssl", ["req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-keyout", key, "-out", cert, "-days", "1", "-subj", `/CN=${TILE_HOST}`, "-addext", `subjectAltName=DNS:${TILE_HOST}`]);
  if (r.status !== 0) throw new Error("openssl req failed: " + r.stderr);
  // `bigNext`: the next request of the zoom 3 gets a tile of 512 px, which the page must drop.
  // `pad`: each tile of 256 px carries 16 MiB more (the private chunk), as a hostile host can send it.
  const state = { cors: true, delay: 150, requests: [], active: 0, peak: 0, bigNext: false, pad: false, padded: 0 };
  const good = png(256, [10, 200, 120]);
  const large = png(512, [10, 200, 120]);
  let heavy = null;
  const server = createServer({ key: readFileSync(key), cert: readFileSync(cert) }, (req, res) => {
    state.requests.push({ t: Date.now(), url: req.url, headers: { ...req.headers } });
    state.active += 1;
    state.peak = Math.max(state.peak, state.active);
    setTimeout(() => {
      state.active -= 1;
      const m = /^\/t\/(\d+)\/(\d+)\/(\d+)\.png$/.exec(req.url);
      const head = { "content-type": "image/png", "cache-control": "no-store" };
      if (state.cors) head["access-control-allow-origin"] = "*";
      if (!m) {
        res.writeHead(404, head);
        res.end();
        return;
      }
      const big = state.bigNext && m[1] === "3";
      if (big) state.bigNext = false;
      res.writeHead(200, head);
      if (state.pad && state.cors) {
        heavy ??= png(256, [10, 200, 120], 16 * 1048576);
        state.padded += 1;
        res.end(heavy);
        return;
      }
      res.end(big ? large : good);
    }, state.delay);
  });
  const port = await freePort();
  await new Promise((ok) => server.listen(port, "127.0.0.1", ok));
  return { state, port, stop: () => new Promise((ok) => server.close(() => ok())) };
}

export const cases = [
  {
    id: "B226",
    name: "geomap with no tile source: the plain background, the markers, a hostile name as text in the card, the zoom, no request off the page origin",
    async run(t) {
      const uid = "geomap-plain";
      const doc = geoDoc(uid, [
        geoPanel(1, "Points", SQL, OPTIONS({ mode: "fixed", lat: 0, lon: 0, zoom: 1 }), { x: 0, y: 0, w: 12, h: 12 }),
        geoPanel(2, "Fit", SQL, OPTIONS({ mode: "fit", lat: 0, lon: 0, zoom: 10 }), { x: 12, y: 0, w: 12, h: 12 }),
      ]);
      if ((await t.api.dashboard(uid)).status !== 200) {
        const r = await t.api.create("transit", doc);
        t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 300)}`);
      }
      // The header of each kind of answer is the header of 12.8, and the session member is null.
      for (const path of ["", `d/${uid}`, "api/session"]) {
        const res = await fetch(t.url(path));
        t.eq(res.headers.get("content-security-policy"), CSP_12_8, `CSP of ${path || "/"}`);
      }
      const session = await (await fetch(t.url("api/session"))).json();
      t.eq(session.geomap, null, "session member");
      const p = await t.open(t.dash(uid), { width: 1400, height: 900 });
      await p.eval("window.alert = () => { window.__alerted = 1; }");
      await p.waitFor(`!!document.querySelector('[data-testid="panel-1"] .panel__plot')?.dataset.geomap`, { what: "map 1 drawn" });
      const h1 = await hookOf(p, 1);
      t.eq([h1.mode, h1.points, h1.skipped, h1.drawn, h1.zoom, h1.tiles], ["plain", 4, 1, 4, 1, null], "hook of panel 1");
      t.eq(await p.text('[data-testid="geomap-1-skip"]'), "1 row has no valid position.", "skip note");
      // The view `fit` without a tile source: the box of the markers, at a zoom of 10 or less.
      const h2 = await hookOf(p, 2);
      t.assert(h2.zoom >= 1 && h2.zoom <= 3 && h2.lat > 20 && h2.lat < 50, `fit view: ${JSON.stringify(h2)}`);
      // The pixel of the marker at (0, 0): the fill of dark-green with the alpha 0.16 on the panel.
      // The pixel 3 px right of and under the centre: the graticule lines cross at the centre (0, 0).
      const [x, y] = await markerAt(p, 1, 0, 0);
      const px = await p.eval(`(() => { const c = document.querySelector('[data-testid="geomap-1"] canvas'); const r = c.getBoundingClientRect(); const d = c.getContext('2d').getImageData(Math.round((${x} + 3 - r.left) * devicePixelRatio), Math.round((${y} + 3 - r.top) * devicePixelRatio), 1, 1).data; return [d[0], d[1], d[2], d[3]]; })()`);
      t.assert(px[3] >= 36 && px[3] <= 46 && px[1] > px[0] && px[1] > px[2], `marker pixel ${px}: dark-green at the alpha 0.16`);
      // The card of the marker of Paris: the hostile name stays text.
      const [mx, my] = await markerAt(p, 1, 48.85, 2.35);
      await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: mx, y: my });
      await p.waitFor(`!document.querySelector('[data-testid="geomap-1-tip"]').hidden`, { what: "card" });
      const card = await p.eval(`(() => { const c = document.querySelector('[data-testid="geomap-1-tip"]'); return { text: [...c.querySelectorAll('.geo__row')].map((r) => r.textContent), imgs: c.querySelectorAll('img').length, alerted: !!window.__alerted }; })()`);
      t.assert(card.text.some((s) => s === `name${EVIL}`), `card rows: ${JSON.stringify(card.text)}`);
      t.eq([card.imgs, card.alerted], [0, false], "no element and no script from the text");
      await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 5, y: 5 });
      // The zoom buttons and the keys.
      await p.click('[data-testid="geomap-1-zoom-in"]');
      t.eq((await hookOf(p, 1)).zoom, 2, "zoom in");
      await p.focus('[data-testid="geomap-1"] canvas');
      await p.press("-");
      t.eq((await hookOf(p, 1)).zoom, 1, "zoom out by the key");
      await p.click('[data-testid="geomap-1-zoom-out"]');
      t.eq((await hookOf(p, 1)).zoom, 0, "zoom 0");
      t.eq(await p.attr('[data-testid="geomap-1-zoom-out"]', "disabled"), "", "the button is off at zoom 0");
      // No request leaves the origin of the page.
      const origin = new URL(t.url()).origin;
      const off = p.requests.filter((r) => !r.url.startsWith(origin) && !r.url.startsWith("data:"));
      t.eq(off.map((r) => r.url), [], "requests off the page origin");
    },
  },
  {
    id: "B227",
    name: "geomap with a local https tile source: the CSP, the session, tile requests with no Referer and no cookie, z/x/y only, the caps, CORS fail closed",
    timeout: 200000,
    async run(t) {
      const dir = join(WORK, "geomap-b227");
      rmSync(dir, { recursive: true, force: true });
      const tiles = await tileServer(join(dir, "tls"));
      let browser = null;
      try {
        const tileUrl = `https://${TILE_HOST}:${tiles.port}/t/{z}/{x}/{y}.png`;
        const block = `geomap {\n  tile_url    = ${JSON.stringify(tileUrl)}\n  attribution = "Test tiles <b>bold</b>"\n  max_zoom    = 6\n}\n`;
        const s = await t.ownServer("geomap", { geomapBlock: block });
        const want = CSP_12_8.replace("img-src 'self' data:;", `img-src 'self' data: https://${TILE_HOST}:${tiles.port}/t/;`);
        for (const path of ["", "api/session"]) {
          const res = await fetch(s.url + path);
          t.eq(res.headers.get("content-security-policy"), want, `CSP of ${path || "/"}`);
        }
        t.eq((await (await fetch(s.url + "api/session")).json()).geomap, { tile_url: tileUrl, attribution: "Test tiles <b>bold</b>", max_zoom: 6 }, "session");
        const uid = "geomap-tiles";
        const api = (await import("../lib/env.mjs")).api(s);
        const r = await api.create("transit", geoDoc(uid, [geoPanel(1, "Tiles", SQL, OPTIONS({ mode: "fixed", lat: 0, lon: 0, zoom: 2 }), { x: 0, y: 0, w: 16, h: 14 })]));
        t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 300)}`);
        // A Chrome of the case alone: the test name goes to 127.0.0.1, each other name fails, and the certificate of the test is accepted.
        browser = await Browser.launch({
          profileDir: join(dir, "profile"),
          width: 1400,
          height: 900,
          args: [`--host-resolver-rules=MAP ${TILE_HOST} 127.0.0.1, MAP * ~NOTFOUND, EXCLUDE 127.0.0.1`, "--ignore-certificate-errors"],
        });
        const ctx = await browser.newContext();
        const p = await ctx.newPage();
        p.failWhen((req) => !/^(https?:\/\/(127\.0\.0\.1|tiles\.hune\.test)[:/]|data:)/.test(req.url));
        await p.send("Network.setCookie", { name: "track", value: "1", domain: TILE_HOST, path: "/", secure: true });
        await p.goto(`${s.url}d/${uid}?from=now-6h&to=now`);
        await p.waitFor(`(() => { const h = document.querySelector('[data-testid="panel-1"] .panel__plot')?.dataset.geomap; return h && JSON.parse(h).tiles.needed > 0 && JSON.parse(h).tiles.drawn === JSON.parse(h).tiles.needed; })()`, { timeout: 30000, what: "the tiles drawn" });
        const h = await hookOf(p, 1);
        t.eq([h.mode, h.zoom, h.tiles.failed, h.tiles.requested], ["tiles", 2, 0, h.tiles.needed], `hook ${JSON.stringify(h)}`);
        t.assert(h.tiles.needed >= 7, `the view needs 7 tiles or more: ${h.tiles.needed}`);
        t.assert(tiles.state.peak <= 6 && tiles.state.peak >= 2, `peak of requests in flight: ${tiles.state.peak}`);
        for (const req of tiles.state.requests) {
          t.assert(/^\/t\/2\/[0-3]\/[0-3]\.png$/.test(req.url), `path ${req.url}`);
          t.assert(!("referer" in req.headers), `no Referer: ${JSON.stringify(req.headers)}`);
          t.assert(!("cookie" in req.headers), `no cookie: ${JSON.stringify(req.headers)}`);
          t.assert(!("authorization" in req.headers), "no authorization");
        }
        t.eq(await p.text('[data-testid="geomap-1-attribution"]'), "Test tiles <b>bold</b>", "the attribution is text");
        t.eq(await p.eval(`document.querySelector('[data-testid="geomap-1-attribution"]').children.length`), 0, "no element in the attribution");
        // The tile pixels show (the image is CORS-clean, so the canvas can be read).
        const pixel = await p.eval(`(() => { const c = document.querySelector('[data-testid="geomap-1"] canvas'); const d = c.getContext('2d').getImageData(5, Math.round(c.height / 2) + 20, 1, 1).data; return [d[0], d[1], d[2]]; })()`);
        t.eq(pixel, [10, 200, 120], "a tile pixel");
        // A pan: no request while the button is down, the first one 200 ms after the release at least.
        const before = tiles.state.requests.length;
        const requestedBefore = (await hookOf(p, 1)).tiles.requested;
        const [cx, cy] = await p.eval(`(() => { const r = document.querySelector('[data-testid="geomap-1"] canvas').getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
        await p.send("Input.dispatchMouseEvent", { type: "mousePressed", x: cx, y: cy, button: "left", clickCount: 1 });
        for (let i = 1; i <= 10; i++) {
          await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: cx + i * 20, y: cy + i * 30, button: "left", buttons: 1 });
          await sleep(40);
        }
        t.eq(tiles.state.requests.length, before, "no request during the pan");
        await p.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: cx + 200, y: cy + 300, button: "left", clickCount: 1 });
        const released = Date.now();
        await p.waitFor(`JSON.parse(document.querySelector('[data-testid="panel-1"] .panel__plot').dataset.geomap).tiles.requested > ${requestedBefore}`, { timeout: 10000, what: "requests after the pan" });
        const after = tiles.state.requests.slice(before);
        t.assert(after.length > 0 && after[0].t - released >= 190, `the first request after the pan: ${after.length ? after[0].t - released : "none"} ms`);
        // A tile of 512 px is dropped, and a host with no CORS header gives no tile, with no second try in the view.
        tiles.state.cors = false;
        const n0 = tiles.state.requests.length;
        await p.click('[data-testid="geomap-1-zoom-in"]');
        await p.waitFor(`JSON.parse(document.querySelector('[data-testid="panel-1"] .panel__plot').dataset.geomap).tiles.failed > 0`, { timeout: 10000, what: "failed tiles" });
        await sleep(2500);
        const hz = await hookOf(p, 1);
        t.eq([hz.zoom, hz.tiles.drawn], [3, 0], `no tile without CORS: ${JSON.stringify(hz)}`);
        const urls = tiles.state.requests.slice(n0).map((q) => q.url);
        t.eq(urls.length, new Set(urls).size, "no second request for a tile in the view");
        t.eq(hz.tiles.failed, hz.tiles.needed, "each needed tile failed once");
        tiles.state.cors = true;
        tiles.state.bigNext = true;
        await p.click('[data-testid="geomap-1-zoom-out"]');
        await p.click('[data-testid="geomap-1-zoom-in"]');
        await p.waitFor(`(() => { const h = JSON.parse(document.querySelector('[data-testid="panel-1"] .panel__plot').dataset.geomap); return h.zoom === 3 && h.tiles.drawn === h.tiles.needed - 1; })()`, { timeout: 15000, what: "the tiles of zoom 3 but the tile of 512 px" });
        // Security review 3, finding F1: a refresh of 10 s draws the map again with a new result. The view and its failed tiles stay, so a
        // host with no CORS header gets one request for each tile of the view in 25 s, through three runs of the panel.
        const allFailed = `(() => { const h = JSON.parse(document.querySelector('[data-testid="panel-1"] .panel__plot')?.dataset.geomap || "null"); return !!h && h.tiles.needed > 0 && h.tiles.failed === h.tiles.needed; })()`;
        const runs = () => p.reqs("/panels/1/query", "POST").length;
        tiles.state.cors = false;
        const r0 = tiles.state.requests.length;
        p.clearRequests();
        await p.goto(`${s.url}d/${uid}?from=now-6h&to=now&refresh=10s`);
        const opened = Date.now();
        await p.waitFor(allFailed, { timeout: 15000, what: "each tile failed" });
        await sleep(25000 - (Date.now() - opened));
        t.assert(runs() >= 3, `the panel ran ${runs()} times in 25 s`);
        const per = {};
        for (const q of tiles.state.requests.slice(r0)) per[q.url] = (per[q.url] || 0) + 1;
        t.eq([Object.keys(per).length, Object.values(per).filter((n) => n !== 1)], [(await hookOf(p, 1)).tiles.needed, []], `one request for each tile in 25 s: ${JSON.stringify(per)}`);
        t.note(`refresh 10 s, no CORS: ${runs()} runs in 25 s, ${Object.keys(per).length} tiles, ${Object.values(per).reduce((a, b) => a + b, 0)} requests`);
        // A pan survives a refresh: the next run draws the map again at the panned view, and the new draw asks for no tile.
        await p.focus('[data-testid="geomap-1"] canvas');
        await p.press("ArrowRight");
        await p.press("ArrowRight");
        const panned = await hookOf(p, 1);
        const runsBefore = runs();
        for (let i = 0; i < 75 && runs() === runsBefore; i++) await sleep(200);
        await sleep(1500);
        const kept = await hookOf(p, 1);
        t.eq([kept.zoom, kept.lat, kept.lon, kept.tiles.requested], [panned.zoom, panned.lat, panned.lon, 0], `the panned view ${JSON.stringify(panned)} after a refresh: ${JSON.stringify(kept)}`);
        t.assert(panned.lon !== 0, `the pan moved the view: ${panned.lon}`);
        // Security review 3, finding F5: the cache keeps decoded bitmaps, not the images with their downloaded bytes. Tiles of 256 px with
        // 16 MiB of padding grow the renderers by less than 64 MiB for 24 tiles: the resident memory after a garbage collection.
        const memUid = "geomap-memory";
        const rm = await api.create("transit", geoDoc(memUid, [geoPanel(1, "Large", SQL, OPTIONS({ mode: "fixed", lat: 0, lon: 0, zoom: 3 }), { x: 0, y: 0, w: 24, h: 20 })]));
        t.assert(rm.status === 201, `create: ${rm.status} ${rm.text.slice(0, 300)}`);
        await p.goto(`${s.url}d/${memUid}?from=now-6h&to=now`);
        await p.waitFor(allFailed, { timeout: 15000, what: "each tile of the large map failed" });
        t.assert((await hookOf(p, 1)).tiles.needed >= 24, `the large map needs 24 tiles or more: ${(await hookOf(p, 1)).tiles.needed}`);
        await p.send("HeapProfiler.enable");
        await p.send("HeapProfiler.collectGarbage");
        await sleep(1500);
        const m0 = await rendererMiB(browser);
        tiles.state.cors = true;
        tiles.state.pad = true;
        await p.focus('[data-testid="geomap-1"] canvas');
        await p.press("ArrowRight"); // a new view: each tile is asked again
        await p.waitFor(`(() => { const h = JSON.parse(document.querySelector('[data-testid="panel-1"] .panel__plot').dataset.geomap); return h.tiles.drawn === h.tiles.needed; })()`, { timeout: 90000, what: "the padded tiles drawn" });
        await p.send("HeapProfiler.collectGarbage");
        await sleep(1500);
        const m1 = await rendererMiB(browser);
        tiles.state.pad = false;
        t.assert(tiles.state.padded >= 24, `tiles of 16 MiB sent: ${tiles.state.padded}`);
        t.assert(m0.pids > 0 && m1.mib - m0.mib < 64, `the renderers: ${m0.mib} MiB, then ${m1.mib} MiB after ${tiles.state.padded} tiles of 16 MiB`);
        t.note(`tiles of 16 MiB: ${tiles.state.padded} (the view needs ${(await hookOf(p, 1)).tiles.needed}); the renderers (${m0.pids}): ${m0.mib} MiB, then ${m1.mib} MiB (+${m1.mib - m0.mib} MiB)`);
        const problems = p.unexpectedProblems();
        t.eq(problems.filter((x) => !/net::ERR|Failed to load resource|CORS|Access-Control/.test(x)), [], "page problems");
        await p.close();
        await ctx.dispose();
      } finally {
        if (browser) await browser.close();
        await tiles.stop();
        rmSync(dir, { recursive: true, force: true });
      }
    },
  },
];
