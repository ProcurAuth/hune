// The pure model of the map panel (9.19i, 4.33g, decision ID-K328): the tile URL grammar of 13.13, Web Mercator, the tiles of a
// view, the caps, the positions of the rows, the size of a marker, the fit of a view, the queue of the tile requests and the cache of
// the tiles. No DOM access: the Node tests load it (FE-85).

/** The side of a tile, in CSS pixels. A tile has 256 x 256 pixels: a tile of another natural size is dropped. */
export const TILE = 256;
/** The latitude bound of Web Mercator: a position above it draws on the edge of the map. */
export const MAX_LAT = 85.0511;
/** The most rows of the data of one map (4.35). Above it the panel draws nothing and shows a message. */
export const MAX_GEO_POINTS = 50000;
/** The most tiles of one view of one panel (4.35). Above it the panel draws the plain background and a note. */
export const MAX_GEO_TILES_VIEW = 256;
/** The most tile requests in flight on one page (4.35). */
export const MAX_GEO_TILES_IN_FLIGHT = 6;
/** The wait after the last pan or zoom before the first tile request, in milliseconds (4.35). */
export const GEO_TILE_DELAY_MS = 200;
/** The most decoded tiles (bitmaps of 256 KiB) that one page keeps (4.35): the least recently used tile goes first. */
export const MAX_GEO_TILE_CACHE = 256;
/** The highest zoom of the format and of `geomap.max_zoom` (4.33g, 13.12c). */
export const MAX_GEO_ZOOM = 22;

// The number grammar of 7.7. A value of a field of the kind `number_text` must match it.
const NUMBER = /^-?[0-9]+(\.[0-9]+)?$/;
const LAT_NAMES = new Set(["lat", "latitude"]);
const LON_NAMES = new Set(["lon", "lng", "longitude"]);
const PLACEHOLDERS = ["{z}", "{x}", "{y}"];

// --- the tile URL (13.13) --------------------------------------------------------------------------------------------------

const isLower = (c) => c >= 97 && c <= 122;
const isDigit = (c) => c >= 48 && c <= 57;
const isAlnum = (c) => isDigit(c) || isLower(c) || (c >= 65 && c <= 90);
const pathByte = (c) => isAlnum(c) || c === 46 || c === 95 || c === 126 || c === 45 || c === 47;
const queryByte = (c) => isAlnum(c) || c === 46 || c === 95 || c === 126 || c === 61 || c === 38 || c === 45;

// A lower-case DNS name with a dot. A last label of digits only is an IPv4 address, or a top-level label that does not exist.
function hostOk(host) {
  if (host.length < 1 || host.length > 253 || !host.includes(".")) return false;
  const labels = host.split(".");
  for (const label of labels) {
    if (label.length < 1 || label.length > 63 || label[0] === "-" || label[label.length - 1] === "-") return false;
    for (let i = 0; i < label.length; i++) {
      const c = label.charCodeAt(i);
      if (!(isLower(c) || isDigit(c) || c === 45)) return false;
    }
  }
  return !/^[0-9]+$/.test(labels[labels.length - 1]);
}

function pathOk(path) {
  if (path[0] !== "/") return false;
  const counts = [0, 0, 0];
  for (let i = 0; i < path.length; ) {
    const c = path.charCodeAt(i);
    if (c === 123) {
      const k = PLACEHOLDERS.findIndex((p) => path.startsWith(p, i));
      if (k < 0) return false;
      counts[k] += 1;
      i += 3;
    } else if (pathByte(c)) i += 1;
    else return false;
  }
  if (counts.some((n) => n !== 1)) return false;
  return path.slice(1).split("/").every((s) => s !== "" && s !== "." && s !== "..");
}

/**
 * The grammar of `geomap.tile_url` (13.13), the same as the server's: the page checks the template of the session again (6.10). The
 * result is `{ text, origin }`, or null for a text outside the grammar: the page then draws the plain background and contacts no host.
 */
export function checkTileUrl(text) {
  if (typeof text !== "string" || text.length < 16 || text.length > 512 || !text.startsWith("https://") || text.includes("#")) return null;
  const rest = text.slice(8);
  const slash = rest.indexOf("/");
  if (slash < 0) return null;
  const authority = rest.slice(0, slash);
  const tail = rest.slice(slash);
  if (/[@{}[\]]/.test(authority)) return null;
  const colon = authority.indexOf(":");
  const host = colon < 0 ? authority : authority.slice(0, colon);
  if (colon >= 0) {
    const port = authority.slice(colon + 1);
    if (!/^[1-9][0-9]{0,4}$/.test(port) || Number(port) > 65535) return null;
  }
  if (!hostOk(host)) return null;
  const q = tail.indexOf("?");
  const path = q < 0 ? tail : tail.slice(0, q);
  if (!pathOk(path)) return null;
  if (q >= 0) {
    const query = tail.slice(q + 1);
    if (query === "") return null;
    for (let i = 0; i < query.length; i++) if (!queryByte(query.charCodeAt(i))) return null;
  }
  return { text, origin: `https://${authority}` };
}

/** The tile source of the session (6.10): `{ template, attribution, maxZoom }`, or null when the session has none or a bad one. */
export function tileSource(session) {
  const g = session && typeof session === "object" ? session.geomap : null;
  if (!g || typeof g !== "object") return null;
  const checked = checkTileUrl(g.tile_url);
  const zoom = g.max_zoom;
  const attribution = typeof g.attribution === "string" ? g.attribution : "";
  if (!checked || !Number.isInteger(zoom) || zoom < 0 || zoom > MAX_GEO_ZOOM || attribution === "" || attribution.length > 400) return null;
  return { template: checked.text, origin: checked.origin, attribution, maxZoom: zoom };
}

/** The URL of one tile: the template with the three integers. A value out of range gives null: the URL holds no other data. */
export function tileUrl(template, z, x, y) {
  if (!Number.isInteger(z) || z < 0 || z > MAX_GEO_ZOOM) return null;
  const n = 2 ** z;
  if (!Number.isInteger(x) || !Number.isInteger(y) || x < 0 || y < 0 || x >= n || y >= n) return null;
  return template.replace("{z}", String(z)).replace("{x}", String(x)).replace("{y}", String(y));
}

// --- Web Mercator ------------------------------------------------------------------------------------------------------------

/** The latitude in the bounds of Web Mercator. */
export const clampLat = (lat) => Math.max(-MAX_LAT, Math.min(MAX_LAT, lat));

/** The world pixel of a position at a zoom: x from 0 to 256 x 2^z, west to east; y from 0 to 256 x 2^z, north to south. */
export function project(lat, lon, z) {
  const size = TILE * 2 ** z;
  const s = Math.sin((clampLat(lat) * Math.PI) / 180);
  return { x: ((lon + 180) / 360) * size, y: (0.5 - Math.log((1 + s) / (1 - s)) / (4 * Math.PI)) * size };
}

/** The position of a world pixel at a zoom. */
export function unproject(x, y, z) {
  const size = TILE * 2 ** z;
  const lon = (x / size) * 360 - 180;
  const lat = (Math.atan(Math.sinh(Math.PI * (1 - (2 * y) / size))) * 180) / Math.PI;
  return { lat, lon };
}

/** The tiles that a view of `w` x `h` CSS pixels needs. `cx`, `cy` are the world pixels of the centre at the zoom `z`. */
export function tileRange(cx, cy, z, w, h) {
  const n = 2 ** z;
  const left = cx - w / 2;
  const top = cy - h / 2;
  const x0 = Math.max(0, Math.floor(left / TILE));
  const x1 = Math.min(n - 1, Math.floor((left + w - 1) / TILE));
  const y0 = Math.max(0, Math.floor(top / TILE));
  const y1 = Math.min(n - 1, Math.floor((top + h - 1) / TILE));
  const count = x1 >= x0 && y1 >= y0 ? (x1 - x0 + 1) * (y1 - y0 + 1) : 0;
  return { z, x0, x1, y0, y1, count, over: count > MAX_GEO_TILES_VIEW, left, top };
}

/** The tiles of a range, the nearest to the centre first. Each item is `{ z, x, y }`. */
export function tilesOf(range) {
  const out = [];
  if (!range.count || range.over) return out;
  const mx = (range.x0 + range.x1) / 2;
  const my = (range.y0 + range.y1) / 2;
  for (let y = range.y0; y <= range.y1; y++) for (let x = range.x0; x <= range.x1; x++) out.push({ z: range.z, x, y, d: (x - mx) ** 2 + (y - my) ** 2 });
  out.sort((a, b) => a.d - b.d);
  return out.map(({ z, x, y }) => ({ z, x, y }));
}

// --- the data ----------------------------------------------------------------------------------------------------------------

const isNumberKind = (f) => f.kind === "number" || f.kind === "number_text";

/** The field of a name of the options (an exact display name), or by the default names (Grafana: the name in lower case, or the display name). */
function findField(fields, names, chosen, defaults) {
  if (chosen !== null && chosen !== undefined) {
    const i = names.indexOf(chosen);
    return i < 0 ? -1 : i;
  }
  return fields.findIndex((f, i) => defaults.has(String(f.name).toLowerCase()) || defaults.has(names[i]));
}

/**
 * The two position fields (4.33g). The result is `{ lat, lon }` (indexes), or `{ error }` with the text of the panel error. A field
 * of another kind than `number` or `number_text` is an error.
 */
export function positionFields(fields, names, layer) {
  const pick = (chosen, defaults, word, list) => {
    const i = findField(fields, names, chosen, defaults);
    if (i < 0) return { error: chosen !== null && chosen !== undefined ? `No field has the name ${chosen}.` : `The data has no ${word} field: name it ${list}, or choose the field in the options.` };
    if (!isNumberKind(fields[i])) return { error: `The ${word} field ${names[i]} is not a number field.` };
    return { i };
  };
  const a = pick(layer.lat_field, LAT_NAMES, "latitude", "lat or latitude");
  if (a.error) return { error: a.error };
  const b = pick(layer.lon_field, LON_NAMES, "longitude", "lon, lng or longitude");
  if (b.error) return { error: b.error };
  return { lat: a.i, lon: b.i };
}

/** The number of row `r` of a number field, or NaN. A text of a `number_text` field must match the grammar of 7.7. */
export function numberAt(field, r) {
  if (field.kind === "number") return field.nulls && field.nulls[r] ? NaN : field.values[r];
  const t = field.values[r];
  return typeof t === "string" && NUMBER.test(t) ? Number(t) : NaN;
}

/**
 * The positions of the rows: `lat`, `lon` (Float64Array) and `ok` (Uint8Array), and `bad`, the count of the rows that have no valid
 * position. A valid position has a finite latitude from -90 to 90 and a finite longitude from -180 to 180.
 */
export function readPositions(latField, lonField, rows) {
  const lat = new Float64Array(rows);
  const lon = new Float64Array(rows);
  const ok = new Uint8Array(rows);
  let bad = 0;
  for (let r = 0; r < rows; r++) {
    const a = r < latField.values.length ? numberAt(latField, r) : NaN;
    const b = r < lonField.values.length ? numberAt(lonField, r) : NaN;
    if (Number.isFinite(a) && Number.isFinite(b) && a >= -90 && a <= 90 && b >= -180 && b <= 180) {
      lat[r] = a;
      lon[r] = b;
      ok[r] = 1;
    } else bad += 1;
  }
  return { lat, lon, ok, bad };
}

/**
 * The size of the markers (9.19i): a function of the row. With no field, it is `size`. With a field, the value goes linearly from the
 * field minimum to the field maximum (the `min` and `max` of 9.14) to `size_min` to `size_max`, as in Grafana 12.4.2. A value outside
 * the range takes the near end. A field with no range, a range of 0 and a value that is not a number give `size_min`.
 */
export function sizeScale(field, layer) {
  if (!field) return () => layer.size;
  const lo = layer.size_min;
  const span = layer.size_max - layer.size_min;
  const min = field.config ? field.config.min : null;
  const max = field.config ? field.config.max : null;
  const delta = Number.isFinite(min) && Number.isFinite(max) ? max - min : NaN;
  if (!(span > 0) || !(delta > 0)) return () => lo;
  return (r) => {
    const v = numberAt(field, r);
    if (!Number.isFinite(v)) return lo;
    return lo + Math.max(0, Math.min(1, (v - min) / delta)) * span;
  };
}

/** The alpha of the fill of a marker: the square of the opacity, as Grafana 12.4.2 draws it (measured: 0.4 gives 0.16, 0.8 gives 0.64). */
export const fillAlpha = (opacity) => (opacity / 100) ** 2;

/**
 * The view of all the markers (9.19i, view `fit`): the centre of their box and the highest whole zoom where the box with a padding of
 * 5 percent fits in `w` x `h`, from 0 to `maxZoom`. With no valid position the result is null. One position gives `maxZoom`.
 */
export function fitView(pos, w, h, maxZoom) {
  let x0 = Infinity, x1 = -Infinity, y0 = Infinity, y1 = -Infinity;
  for (let r = 0; r < pos.ok.length; r++) {
    if (!pos.ok[r]) continue;
    const p = project(pos.lat[r], pos.lon[r], 0);
    if (p.x < x0) x0 = p.x;
    if (p.x > x1) x1 = p.x;
    if (p.y < y0) y0 = p.y;
    if (p.y > y1) y1 = p.y;
  }
  if (x0 > x1) return null;
  const dx = (x1 - x0) * 1.05;
  const dy = (y1 - y0) * 1.05;
  let zoom = maxZoom;
  if (dx > 0 || dy > 0) {
    const scale = Math.min(dx > 0 ? w / dx : Infinity, dy > 0 ? h / dy : Infinity);
    zoom = Math.max(0, Math.min(maxZoom, Math.floor(Math.log2(scale))));
  }
  const c = unproject((x0 + x1) / 2, (y0 + y1) / 2, 0);
  return { lat: c.lat, lon: c.lon, zoom };
}

// --- the tile requests -------------------------------------------------------------------------------------------------------

/**
 * A cache with a size limit: the least recently used entry goes first. `drop(value)` gets each value that leaves the cache, also a value
 * that a new value of the same key replaces. One cache serves the whole page.
 */
export class Lru {
  constructor(limit, drop = () => {}) {
    this.limit = limit;
    this.drop = drop;
    this.map = new Map();
  }
  get(key) {
    if (!this.map.has(key)) return undefined;
    const v = this.map.get(key);
    this.map.delete(key);
    this.map.set(key, v);
    return v;
  }
  has(key) {
    return this.map.has(key);
  }
  set(key, value) {
    const old = this.map.get(key);
    if (old !== undefined && old !== value) this.drop(old);
    this.map.delete(key);
    this.map.set(key, value);
    while (this.map.size > this.limit) {
      const [k, v] = this.map.entries().next().value;
      this.map.delete(k);
      this.drop(v);
    }
  }
  get size() {
    return this.map.size;
  }
}

/**
 * The queue of the tile requests of one page: at most `limit` jobs run at a time. A job is `{ owner, start(done) }`: `start` begins the
 * request and calls `done` one time at the end, success or failure. `drop(owner)` removes the jobs of an owner that wait.
 */
export class TileQueue {
  constructor(limit) {
    this.limit = limit;
    this.active = 0;
    this.peak = 0;
    this.waiting = [];
  }
  push(job) {
    this.waiting.push(job);
    this.pump();
  }
  drop(owner) {
    this.waiting = this.waiting.filter((j) => j.owner !== owner);
  }
  pump() {
    while (this.active < this.limit && this.waiting.length) {
      const job = this.waiting.shift();
      this.active += 1;
      this.peak = Math.max(this.peak, this.active);
      let ended = false;
      job.start(() => {
        if (ended) return;
        ended = true;
        this.active -= 1;
        this.pump();
      });
    }
  }
}
