// The map panel (9.19i, 4.33g): one layer of markers on the tiles of the block `geomap` of the configuration, or on a
// plain background when the server has no tile source. Hune code draws the tiles, the graticule and the markers on one canvas.
//
// Security rules of the map: the tile source comes only from the session (6.10), and the page checks its template again with the grammar
// of 13.13. A tile is an Image with crossOrigin "anonymous" (no cookie, no HTTP auth) and referrerPolicy "no-referrer". The URL of a tile
// holds only the three integers z, x and y. A host with no CORS header gives no tile (fail closed). The page requests a tile only for a
// panel in the viewport, 200 ms after the last pan or zoom, with 6 requests in flight on the page at most, no prefetch and no second try
// in the same view, also after a refresh. Each text of the data goes in with textContent only.
import { app } from "../config.js";
import { makeDisplay } from "../display.js";
import { markColor, markCss } from "../colors.js";
import {
  GEO_TILE_DELAY_MS, Lru, MAX_GEO_POINTS, MAX_GEO_TILES_IN_FLIGHT, MAX_GEO_TILE_CACHE, MAX_GEO_TILES_VIEW, MAX_GEO_ZOOM, TILE, TileQueue,
  fillAlpha, fitView, numberAt, positionFields, project, readPositions, sizeScale, tileRange, tileSource, tilesOf, tileUrl, unproject,
} from "../geo_model.js";
import { el, emptyState, uniqueId } from "../ui.js";
import { maxRows } from "../queries.js";
import { canvasColor, isDarkTheme } from "./common.js";

// One queue and one cache for the whole page: the caps of 4.35 count the requests and the tiles of all the maps together. The cache keeps
// decoded bitmaps: an image keeps its downloaded bytes, and they have no cap (T106). A bitmap that leaves the cache is closed.
const queue = new TileQueue(MAX_GEO_TILES_IN_FLIGHT);
const cache = new Lru(MAX_GEO_TILE_CACHE, (bitmap) => bitmap.close());
// The view and the failed tiles of each map, by the dashboard and the key of the panel: a new draw of the map (a refresh, a new result)
// keeps them (9.19i). A change of the start view, of the template or of the highest zoom starts again. A document has 200 panels at most.
const views = new Lru(MAX_GEO_TILE_CACHE);
// The card shows 32 rows at most, and counts the rest.
const CARD_ROWS = 32;
// The arrow keys move the view by 64 CSS pixels.
const KEY_STEP = 64;
const KEYS = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };

// Draws a panel of type `geomap` in `host` (9.19i). An error of the data goes to the frame as an exception. The cap of the points gives the
// state `geo-limit` and a message, with no picture. The return value is the handle of the frame: `destroy` and `note` (the skipped rows).
export function render(host, ctx) {
  const { panel, fields, names, zone } = ctx;
  const o = panel.options;
  const id = panel.id;
  const root = el("div", { class: "geo", dataset: { testid: `geomap-${id}` } });
  host.append(root);
  let destroyed = false;
  const rows = maxRows(fields);
  if (rows > MAX_GEO_POINTS) {
    root.append(el("p", { class: "geo__limit", role: "note", text: `Too many points: ${rows.toLocaleString("en-US")}. The limit is ${MAX_GEO_POINTS.toLocaleString("en-US")}.`, dataset: { testid: `geomap-${id}-note` } }));
    host.dataset.geomap = JSON.stringify({ points: 0, skipped: 0, limit: rows });
    return { state: "geo-limit", destroy: () => root.remove() };
  }
  const which = positionFields(fields, names, o.layer);
  if (which.error) throw new Error(which.error);
  const pos = readPositions(fields[which.lat], fields[which.lon], rows);
  const valid = rows - pos.bad;
  if (!valid) {
    root.append(emptyState({ icon: null, title: "No data", inline: true }));
    return { state: "empty", destroy: () => root.remove() };
  }
  const tiles = tileSource(app.session);
  const maxZoom = tiles ? tiles.maxZoom : MAX_GEO_ZOOM;
  const named = (name) => (name === null ? -1 : names.indexOf(name));
  const sizeField = fields[named(o.layer.size_field)] || null;
  const colorField = fields[named(o.layer.color_field)] || null;
  const sizeOf = sizeScale(sizeField, o.layer);
  const alpha = fillAlpha(o.layer.opacity);
  const note = pos.bad ? `${pos.bad.toLocaleString("en-US")} ${pos.bad === 1 ? "row has" : "rows have"} no valid position.` : "";

  // --- the elements ------------------------------------------------------------------------------------------------------------
  const canvas = el("canvas", { class: "geo__canvas", tabIndex: 0, role: "img", "aria-label": `${panel.title || "Untitled"}, map with ${valid.toLocaleString("en-US")} ${valid === 1 ? "marker" : "markers"}` });
  const descId = uniqueId("geomap-desc");
  canvas.setAttribute("aria-describedby", descId);
  const card = el("div", { class: "geo__card", role: "tooltip", hidden: true, dataset: { testid: `geomap-${id}-tip` } });
  const zoomIn = el("button", { class: "geo__zoom", type: "button", "aria-label": "Zoom in", title: "Zoom in", text: "+", dataset: { testid: `geomap-${id}-zoom-in` } });
  const zoomOut = el("button", { class: "geo__zoom", type: "button", "aria-label": "Zoom out", title: "Zoom out", text: "–", dataset: { testid: `geomap-${id}-zoom-out` } });
  const zoomBox = el("div", { class: "geo__zooms", hidden: !o.show_zoom }, zoomIn, zoomOut);
  const attribution = el("p", { class: "geo__attribution", hidden: true, dataset: { testid: `geomap-${id}-attribution` } });
  const tileNote = el("p", { class: "geo__note", role: "note", hidden: true, dataset: { testid: `geomap-${id}-tiles-note` } });
  const skipNote = el("p", { class: "geo__note geo__note--skip", role: "note", hidden: !note, text: note, dataset: { testid: `geomap-${id}-skip` } });
  const desc = el("p", { class: "visually-hidden", id: descId, text: `${valid} ${valid === 1 ? "marker" : "markers"}. ${note}`.trim() });
  root.append(canvas, zoomBox, el("div", { class: "geo__notes" }, skipNote, tileNote), attribution, card, desc);
  if (tiles) attribution.textContent = tiles.attribution;

  // --- the view ----------------------------------------------------------------------------------------------------------------
  // `cx`, `cy` are the world pixels of the centre at the zoom `z`. The view `fit` applies only with no tile source: with tiles, the tile
  // coordinates would tell the tile host the area of the data. A new draw starts at the kept view, and `fit` fits again.
  const sig = JSON.stringify([o.view, tiles && tiles.template, maxZoom]);
  const vkey = `${typeof ctx.linkContext === "function" ? ctx.linkContext().dashboardUid : ""}|${ctx.key ?? id}`;
  let kept = views.get(vkey);
  if (!kept || kept.sig !== sig) views.set(vkey, (kept = { sig, z: null, cx: 0, cy: 0, failed: new Set(), size: "" }));
  let z = Math.min(o.view.zoom, maxZoom);
  let cx = 0;
  let cy = 0;
  let fitted = false;
  const setCentre = (lat, lon) => {
    const p = project(lat, lon, z);
    cx = p.x;
    cy = p.y;
  };
  setCentre(o.view.lat, o.view.lon);
  if (kept.z !== null) ({ z, cx, cy } = kept);
  const size = () => ({ w: root.clientWidth, h: root.clientHeight });
  const fit = () => {
    if (fitted || tiles || o.view.mode !== "fit") return;
    const { w, h } = size();
    if (!w || !h) return;
    const v = fitView(pos, w, h, Math.min(o.view.zoom, maxZoom));
    fitted = true;
    if (!v) return;
    z = v.zoom;
    setCentre(v.lat, v.lon);
  };

  // --- the tiles ---------------------------------------------------------------------------------------------------------------
  let visible = false;
  let timer = 0;
  let viewId = 0; // a pan, a zoom or a new size makes a new view: the failures of a view (`kept.failed`) are not tried again in that view
  const inFlight = new Map(); // key -> Image
  const stats = { requested: 0, failed: 0, loaded: 0 };
  const owner = {};
  const keyOf = (t) => `${tiles.template}|${t.z}/${t.x}/${t.y}`;
  const stopRequests = () => {
    queue.drop(owner);
    for (const img of inFlight.values()) img.cancel();
    inFlight.clear();
  };
  const changed = () => {
    viewId += 1;
    kept.failed = new Set();
    stopRequests();
    clearTimeout(timer);
    if (tiles) timer = setTimeout(loadTiles, GEO_TILE_DELAY_MS);
  };
  function loadTiles() {
    if (!tiles || !visible || destroyed) return;
    const { w, h } = size();
    const range = tileRange(cx, cy, z, w, h);
    if (range.over) return;
    const mine = viewId;
    for (const t of tilesOf(range)) {
      const key = keyOf(t);
      if (cache.has(key) || kept.failed.has(key) || inFlight.has(key)) continue;
      const url = tileUrl(tiles.template, t.z, t.x, t.y);
      if (url === null) continue;
      queue.push({
        owner,
        start(done) {
          if (destroyed || mine !== viewId) return done();
          const img = new Image();
          img.crossOrigin = "anonymous";
          img.referrerPolicy = "no-referrer";
          img.decoding = "async";
          let ended = false;
          const end = (ok) => {
            if (ended) return;
            ended = true;
            img.onload = img.onerror = null;
            inFlight.delete(key);
            done();
            const fail = () => {
              if (mine !== viewId) return;
              // A failure, a host with no CORS header and a tile of another size: no tile, and no second try in this view.
              kept.failed.add(key);
              stats.failed += 1;
              hook();
            };
            if (!ok || img.naturalWidth !== TILE || img.naturalHeight !== TILE) return fail();
            // The cache keeps the decoded bitmap. The image and its downloaded bytes go.
            createImageBitmap(img).then((bitmap) => {
              img.removeAttribute("src");
              cache.set(key, bitmap);
              stats.loaded += 1;
              draw();
            }, fail);
          };
          img.cancel = () => {
            if (ended) return;
            ended = true;
            img.onload = img.onerror = null;
            img.removeAttribute("src");
            done();
          };
          img.onload = () => end(true);
          img.onerror = () => end(false);
          inFlight.set(key, img);
          stats.requested += 1;
          img.src = url;
        },
      });
    }
    hook();
  }
  const io = typeof IntersectionObserver === "function"
    ? new IntersectionObserver((entries) => {
      const now = entries.some((e) => e.isIntersecting);
      if (now && !visible) {
        visible = true;
        clearTimeout(timer);
        if (tiles) timer = setTimeout(loadTiles, GEO_TILE_DELAY_MS);
      } else if (!now && visible) {
        visible = false;
        clearTimeout(timer);
        stopRequests();
      }
    })
    : null;
  if (io) io.observe(root);

  // --- the drawing -------------------------------------------------------------------------------------------------------------
  // The world pixels of each position at the zoom 0: a zoom multiplies them by 2^z, so a draw and a hit test need no trigonometry.
  const wx = new Float64Array(rows);
  const wy = new Float64Array(rows);
  for (let r = 0; r < rows; r++) {
    if (!pos.ok[r]) continue;
    const p = project(pos.lat[r], pos.lon[r], 0);
    wx[r] = p.x;
    wy[r] = p.y;
  }
  let drawnMarkers = 0;
  let lastRange = null;
  const colourCache = new Map();
  const cssColour = (css) => {
    if (!colourCache.has(css)) colourCache.set(css, canvasColor(root, css));
    return colourCache.get(css);
  };
  const fixedColour = markCss(o.layer.color);
  const colourOf = (r) => {
    if (!colorField) return cssColour(fixedColour);
    const v = colorField.kind === "number" || colorField.kind === "number_text" ? numberAt(colorField, r) : NaN;
    const c = markColor(colorField.config, v, { min: colorField.config.min, max: colorField.config.max, seriesIndex: 0, dark: isDarkTheme() });
    return cssColour(c || fixedColour);
  };

  // The plain background (9.19i): the theme fill of the panel, a graticule of 30 degrees and the equator, inside the bounds of the world.
  function graticule(g, w, h) {
    const world = TILE * 2 ** z;
    const ox = w / 2 - cx;
    const oy = h / 2 - cy;
    g.lineWidth = 1;
    g.strokeStyle = canvasColor(root, "var(--grid-line)");
    g.beginPath();
    for (let lon = -180; lon <= 180; lon += 30) {
      const x = Math.round(ox + ((lon + 180) / 360) * world) + 0.5;
      g.moveTo(x, Math.max(0, oy));
      g.lineTo(x, Math.min(h, oy + world));
    }
    for (let lat = -60; lat <= 60; lat += 30) {
      if (lat === 0) continue;
      const y = Math.round(oy + project(lat, 0, z).y) + 0.5;
      g.moveTo(Math.max(0, ox), y);
      g.lineTo(Math.min(w, ox + world), y);
    }
    g.stroke();
    g.strokeStyle = canvasColor(root, "var(--border-strong)");
    g.beginPath();
    const y = Math.round(oy + world / 2) + 0.5;
    g.moveTo(Math.max(0, ox), y);
    g.lineTo(Math.min(w, ox + world), y);
    g.stroke();
  }

  function draw() {
    if (destroyed) return;
    fit();
    Object.assign(kept, { z, cx, cy });
    const { w, h } = size();
    if (!w || !h) return;
    const dpr = window.devicePixelRatio || 1;
    const bw = Math.round(w * dpr);
    const bh = Math.round(h * dpr);
    if (canvas.width !== bw || canvas.height !== bh) {
      canvas.width = bw;
      canvas.height = bh;
    }
    const g = canvas.getContext("2d");
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    const range = tiles ? tileRange(cx, cy, z, w, h) : null;
    lastRange = range;
    const useTiles = !!range && !range.over;
    tileNote.hidden = !(range && range.over);
    if (range && range.over) tileNote.textContent = `The map needs ${range.count.toLocaleString("en-US")} tiles. The limit is ${MAX_GEO_TILES_VIEW}.`;
    attribution.hidden = !useTiles;
    if (useTiles) {
      for (const t of tilesOf(range)) {
        const img = cache.get(keyOf(t));
        if (img) g.drawImage(img, Math.round(t.x * TILE - range.left), Math.round(t.y * TILE - range.top), TILE, TILE);
      }
    } else graticule(g, w, h);
    // The markers, in the order of the rows: a later row draws on top. A marker is a disc of radius `size` + 0.5 px, with the fill alpha
    // of `fillAlpha` (measured on Grafana 12.4.2).
    g.globalAlpha = alpha;
    const ox = w / 2 - cx;
    const oy = h / 2 - cy;
    const k = 2 ** z;
    drawnMarkers = 0;
    for (let r = 0; r < rows; r++) {
      if (!pos.ok[r]) continue;
      const x = ox + wx[r] * k;
      const y = oy + wy[r] * k;
      const s = sizeOf(r) + 0.5;
      if (x < -s || y < -s || x > w + s || y > h + s) continue;
      g.fillStyle = colourOf(r);
      g.beginPath();
      g.arc(x, y, s, 0, Math.PI * 2);
      g.fill();
      drawnMarkers += 1;
    }
    g.globalAlpha = 1;
    zoomIn.disabled = z >= maxZoom;
    zoomOut.disabled = z <= 0;
    hook();
  }

  // The hook of 9.29: written after each draw and after each tile result.
  function hook() {
    const centre = unproject(cx, cy, z);
    let drawnTiles = 0;
    if (lastRange && !lastRange.over && tiles) for (const t of tilesOf(lastRange)) if (cache.has(keyOf(t))) drawnTiles += 1;
    host.dataset.geomap = JSON.stringify({
      points: valid, skipped: pos.bad, drawn: drawnMarkers, mode: tiles ? "tiles" : "plain", zoom: z,
      lat: Math.round(centre.lat * 1e6) / 1e6, lon: Math.round(centre.lon * 1e6) / 1e6,
      tiles: tiles ? { needed: lastRange ? lastRange.count : 0, over: !!(lastRange && lastRange.over), drawn: drawnTiles, ...stats, peak: queue.peak } : null,
    });
  }

  // --- the card ----------------------------------------------------------------------------------------------------------------
  const displays = fields.map((f) => makeDisplay(f, { zone }));
  let shown = -1;
  // The row of the marker under a point, or -1. The last row that draws is on top, so the search goes from the end.
  function hit(px, py) {
    const { w, h } = size();
    const ox = w / 2 - cx;
    const oy = h / 2 - cy;
    const k = 2 ** z;
    for (let r = rows - 1; r >= 0; r--) {
      if (!pos.ok[r]) continue;
      const s = sizeOf(r) + 1.5;
      if ((ox + wx[r] * k - px) ** 2 + (oy + wy[r] * k - py) ** 2 <= s * s) return r;
    }
    return -1;
  }
  function showCard(r, px, py) {
    if (r === shown) return placeCard(px, py);
    shown = r;
    card.replaceChildren();
    if (r < 0) {
      card.hidden = true;
      return;
    }
    let count = 0;
    let more = 0;
    fields.forEach((f, i) => {
      if (f.config && f.config.hide_from && f.config.hide_from.tooltip) return;
      if (count >= CARD_ROWS) {
        more += 1;
        return;
      }
      count += 1;
      const value = r < f.values.length ? displays[i](r).text : "";
      card.append(el("div", { class: "geo__row" }, el("span", { class: "geo__name", text: names[i] }), el("span", { class: "geo__value", text: value })));
    });
    if (more) card.append(el("div", { class: "geo__more", text: `${more.toLocaleString("en-US")} more` }));
    card.hidden = false;
    placeCard(px, py);
  }
  function placeCard(px, py) {
    if (card.hidden) return;
    const { w, h } = size();
    const cw = card.offsetWidth || 0;
    const ch = card.offsetHeight || 0;
    const x = px + 10 + cw > w ? Math.max(0, px - 10 - cw) : px + 10;
    const y = py + 10 + ch > h ? Math.max(0, py - 10 - ch) : py + 10;
    card.style.left = `${Math.round(x)}px`;
    card.style.top = `${Math.round(y)}px`;
  }
  const local = (ev) => {
    const b = canvas.getBoundingClientRect();
    return [ev.clientX - b.left, ev.clientY - b.top];
  };

  // --- the pan and the zoom ----------------------------------------------------------------------------------------------------
  let drag = null;
  const zoomAt = (step, px, py) => {
    const next = Math.max(0, Math.min(maxZoom, z + step));
    if (next === z) return;
    const { w, h } = size();
    // The world point under (px, py) stays under it.
    const wx = cx + px - w / 2;
    const wy = cy + py - h / 2;
    const f = 2 ** (next - z);
    cx = wx * f - (px - w / 2);
    cy = wy * f - (py - h / 2);
    z = next;
    showCard(-1);
    changed();
    draw();
  };
  const onDown = (ev) => {
    if (ev.button !== 0) return;
    drag = { x: ev.clientX, y: ev.clientY, cx, cy, moved: false };
    canvas.setPointerCapture(ev.pointerId);
  };
  const onMove = (ev) => {
    if (drag) {
      const dx = ev.clientX - drag.x;
      const dy = ev.clientY - drag.y;
      if (!drag.moved && Math.abs(dx) + Math.abs(dy) < 3) return;
      if (!drag.moved) {
        drag.moved = true;
        showCard(-1);
        stopRequests();
        clearTimeout(timer);
      }
      cx = drag.cx - dx;
      cy = drag.cy - dy;
      draw();
      return;
    }
    if (o.tooltip !== "details") return;
    const [px, py] = local(ev);
    showCard(hit(px, py), px, py);
  };
  const onUp = (ev) => {
    const was = drag;
    drag = null;
    if (!was) return;
    if (was.moved) changed();
    else if (o.tooltip === "none") {
      // The option `none`: the card shows after a click, as in Grafana.
      const [px, py] = local(ev);
      const r = hit(px, py);
      showCard(r === shown ? -1 : r, px, py);
    }
  };
  let wheelSum = 0;
  const onWheel = (ev) => {
    if (!o.mouse_wheel_zoom) return;
    ev.preventDefault();
    wheelSum += ev.deltaMode === 1 ? ev.deltaY * 16 : ev.deltaY;
    if (Math.abs(wheelSum) < 100) return;
    const step = wheelSum < 0 ? 1 : -1;
    wheelSum = 0;
    const [px, py] = local(ev);
    zoomAt(step, px, py);
  };
  const onKey = (ev) => {
    if (ev.ctrlKey || ev.altKey || ev.metaKey) return;
    const { w, h } = size();
    if (KEYS[ev.key]) {
      cx += KEYS[ev.key][0] * KEY_STEP;
      cy += KEYS[ev.key][1] * KEY_STEP;
      showCard(-1);
      changed();
      draw();
    } else if (ev.key === "+" || ev.key === "=") zoomAt(1, w / 2, h / 2);
    else if (ev.key === "-") zoomAt(-1, w / 2, h / 2);
    else if (ev.key === "Escape") showCard(-1);
    else return;
    ev.preventDefault();
  };
  const onLeave = () => {
    if (!drag && o.tooltip === "details") showCard(-1);
  };
  canvas.addEventListener("pointerdown", onDown);
  canvas.addEventListener("pointermove", onMove);
  canvas.addEventListener("pointerup", onUp);
  canvas.addEventListener("pointercancel", onUp);
  canvas.addEventListener("pointerleave", onLeave);
  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("keydown", onKey);
  zoomIn.addEventListener("click", () => zoomAt(1, size().w / 2, size().h / 2));
  zoomOut.addEventListener("click", () => zoomAt(-1, size().w / 2, size().h / 2));

  // --- the size, the theme and the return to the page ----------------------------------------------------------------------------
  // A new size is a new view: it can need other tiles. The first callback of the observer comes after the first draw. At the kept size it
  // is the same view, so the failed tiles of the view stay failed after a refresh. A hidden map (no size) keeps its view.
  let lastSize = "";
  const onResize = () => {
    const { w, h } = size();
    draw();
    const now = `${w}x${h}`;
    if (now === lastSize || !w || !h) return;
    const same = !lastSize && kept.size === now;
    lastSize = kept.size = now;
    if (tiles && !same) changed();
  };
  const observer = new ResizeObserver(onResize);
  observer.observe(root);
  const onTheme = () => {
    colourCache.clear();
    draw();
  };
  document.addEventListener("hune-theme", onTheme);
  const onShown = () => onResize(); // 10.2a, rule 17: a map that returns to the page draws in this task
  document.addEventListener("hune-shown", onShown);
  draw();
  return {
    note,
    destroy() {
      destroyed = true;
      clearTimeout(timer);
      stopRequests();
      observer.disconnect();
      if (io) io.disconnect();
      document.removeEventListener("hune-theme", onTheme);
      document.removeEventListener("hune-shown", onShown);
      root.remove();
    },
  };
}
