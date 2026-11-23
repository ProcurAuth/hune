// The heatmap: the grid of raw values, the frame that is already bucketed, the cells, the colour scale and the axis (9.19e, FE-42, FE-44, FE-45).
// Pure module: no DOM access. `panels/heatmap.js` is the only page module that imports it (decision ID-K195b).
import { basisChannel, parseCss, resolveFill } from "./colors.js";
import { MAX_BUCKET_VALUES, clip, decimal, finite, hidden, isNumber, numbers, plain, shortText, tooMany } from "./bucket_model.js";
import { formatTime } from "./units.js";
import { NUMERIC_RE } from "./vendor/chdash/app_results_values.js";

/** The cap of the cells of a heatmap grid, empty cells included (9.19e, section 3). */
export const MAX_HEAT_CELLS = 250000;

/** The list `T` of the time sizes in milliseconds (9.19e, section 2, rule 3). */
export const TIME_SIZES = Object.freeze([
  1, 2, 4, 5, 10, 20, 25, 40, 50, 100, 200, 250, 400, 500, 1e3, 2e3, 4e3, 5e3, 1e4, 15e3, 2e4, 3e4, 6e4, 12e4, 24e4, 3e5, 6e5, 9e5, 12e5, 18e5,
  36e5, 72e5, 144e5, 216e5, 288e5, 432e5, 648e5, 864e5, 1728e5, 2592e5, 3456e5, 432e6, 5184e5, 6048e5, 864e6, 1296e6, 2592e6, 3888e6, 5184e6,
  7776e6, 15552e6, 31536e6, 63072e6, 94608e6, 126144e6, 157680e6, 189216e6, 220752e6, 252288e6, 283824e6, 315360e6,
]);

/** The list `N` of the value sizes (9.19e, section 2, rule 7): `m * 10^k`, `k` from -16 to 15, whole below 10. */
export const VALUE_SIZES = Object.freeze(
  (() => {
    const out = [];
    for (let k = -16; k <= 15; k++) for (let h = 2; h <= 19; h++) if (k < 0 || k > 0 || h % 2 === 0) out.push(decimal(h / 2, k));
    return out;
  })(),
);

// The largest number of an ascending list that is not above x, or `none`.
function floorIn(list, x, none) {
  let r = none;
  for (const n of list) {
    if (n > x) break;
    r = n;
  }
  return r;
}

/** The time size for a wanted step `x` in milliseconds: the largest of `T` not above it, or 1. */
export const niceTime = (x) => floorIn(TIME_SIZES, x, 1);

/** The value size for a wanted step `y`: the largest of `N` not above it, or 1e-16. */
export const niceValue = (y) => floorIn(VALUE_SIZES, y, 1e-16);

const tooManyCells = (nx, ny) => `Too many cells: ${plain(nx)} x ${plain(ny)}. The limit is ${plain(MAX_HEAT_CELLS)}.`;

/** The note of the values of 0 or less that a log scale skips (decision ID-K183), or "". */
export const logNote = (n) => (n ? `${plain(n)} ${n === 1 ? "value of 0 or less is" : "values of 0 or less are"} not on the log scale.` : "");

/**
 * The grid of the shape `raw` (9.19e, section 2). `times` is the time field, `cols` are the value arrays of the same length (NaN is skipped),
 * `x` and `y` are the options `x_buckets` and `y_buckets`. Gives `{ error }`, `{ limit }` or the grid:
 * `sx`, `c0`, `nx`, `sy`, `r0`, `ny`, `log` (0, 2 or 10), `split`, `counts` (index `column * ny + row`), `lo`, `hi`, `skipped` (values of 0 or less on a log scale).
 */
export function bucketRaw({ times, cols, from, to, x, y }) {
  const rows = times.length;
  if (rows * cols.length > MAX_BUCKET_VALUES) return { limit: tooMany(rows * cols.length) };
  const log = y.scale === "log2" ? 2 : y.scale === "log10" ? 10 : 0;
  const lg = log === 2 ? Math.log2 : Math.log10;
  const split = y.value || 1;
  let lo = Infinity;
  let hi = -Infinity;
  let skipped = 0;
  for (const col of cols) {
    for (let i = 0; i < rows; i++) {
      const v = col[i];
      if (!finite(v)) continue;
      if (log && !(v > 0)) {
        skipped++;
        continue;
      }
      if (v < lo) lo = v;
      if (v > hi) hi = v;
    }
  }
  if (lo > hi) return { error: skipped ? `The log scale needs values above 0. The data has ${plain(skipped)} values of 0 or less.` : "The data has no value to bucket." };
  const sx = x.mode === "size" && x.value > 0 ? x.value : niceTime((to - from) / Math.max((x.value || 20) - 1, 1));
  const c0 = Math.floor(from / sx);
  const nx = Math.floor(to / sx) - c0 + 1;
  if (!(nx >= 1)) return { error: "The time range is empty." };
  let sy = 0;
  let r0;
  let ny;
  if (log) {
    r0 = Math.floor(lg(lo) * split);
    ny = Math.floor(lg(hi) * split) - r0 + 1;
  } else {
    sy = y.mode === "size" && y.value > 0 ? y.value : hi > lo ? niceValue((hi - lo) / Math.max((y.value || 10) - 1, 1)) : 1;
    r0 = Math.floor(lo / sy);
    ny = Math.floor(hi / sy) - r0 + 1;
  }
  if (!(nx * ny <= MAX_HEAT_CELLS)) return { limit: tooManyCells(nx, ny) };
  const counts = new Uint32Array(nx * ny);
  for (const col of cols) {
    for (let i = 0; i < rows; i++) {
      const v = col[i];
      if (!finite(v) || (log && !(v > 0))) continue;
      const c = Math.floor(times[i] / sx) - c0;
      const r = (log ? Math.floor(lg(v) * split) : Math.floor(v / sy)) - r0;
      if (c >= 0 && c < nx && r >= 0 && r < ny) counts[c * ny + r]++;
    }
  }
  return { sx, c0, nx, sy, r0, ny, log, split, counts, lo, hi, skipped };
}

/** The number of a row name (a decimal text, `+Inf` or `-Inf`), or NaN (9.19e, section 1, rule 4). */
export const nameNumber = (s) => (s === "+Inf" ? Infinity : s === "-Inf" ? -Infinity : NUMERIC_RE.test(s) ? Number(s) : NaN);

/**
 * The model of a heatmap (9.19e). `fields` have a resolved config, `names` are the display names, `options` are the options of 4.33c,
 * `range` is `{ from, to }`. Gives `{ error }`, `{ limit }` or `{ shape, nx, ny, counts, ... }`:
 * `raw` has the members of `bucketRaw` and `bucketed` has `times` (the end of each column), `step`, `labels` (the name of each row).
 * Both have `note` (the text of the values that a log scale skipped) and `field` (the first value field, for the unit and the decimals).
 */
export function buildHeatmap({ fields, names, options, range }) {
  const ti = fields.findIndex((f) => f.kind === "time");
  if (ti < 0) return { error: "The data has no time column." };
  const picked = [];
  fields.forEach((f, i) => {
    if (isNumber(f) && !hidden(f)) picked.push(i);
  });
  if (!picked.length) return { error: "The data has no number column." };
  const time = fields[ti].values;
  const field = fields[picked[0]];
  if (options.calculate) {
    const g = bucketRaw({ times: time, cols: picked.map((i) => numbers(fields[i])), from: range.from, to: range.to, x: options.x_buckets, y: options.y_buckets });
    return g.error || g.limit ? g : { shape: "raw", ...g, field, note: logNote(g.skipped) };
  }
  // The shape `bucketed`: the samples by time, up and stable; the later of two samples of one time wins.
  const order = [];
  for (let i = 0; i < time.length; i++) if (finite(time[i])) order.push(i);
  order.sort((a, b) => time[a] - time[b]);
  const at = [];
  for (let k = 0; k < order.length; k++) {
    if (k + 1 < order.length && time[order[k + 1]] === time[order[k]]) continue;
    at.push(order[k]);
  }
  if (at.length < 2) return { error: "A heatmap needs two time rows at least." };
  // A long frame (9.22) gives the names "<value field> <label>": the prefix goes when every row has it (decision ID-K195c).
  const lead = `${field.name} `;
  const drop = picked.every((i) => fields[i].name === field.name && Object.keys(fields[i].labels || {}).length && names[i].startsWith(lead));
  const list = picked.map((i, k) => ({ k, name: drop ? names[i].slice(lead.length) : names[i], i }));
  if (list.every((r) => !Number.isNaN(nameNumber(r.name)))) list.sort((a, b) => nameNumber(a.name) - nameNumber(b.name));
  const nx = at.length;
  const ny = list.length;
  if (nx * ny > MAX_HEAT_CELLS) return { limit: tooManyCells(nx, ny) };
  const counts = new Float64Array(nx * ny);
  list.forEach((r, row) => {
    const col = numbers(fields[r.i]);
    for (let c = 0; c < nx; c++) counts[c * ny + row] = col[at[c]];
  });
  return { shape: "bucketed", nx, ny, counts, times: at.map((i) => time[i]), step: time[at[1]] - time[at[0]], labels: list.map((r) => r.name), field, note: "" };
}

/** The edges `[start, end]` of column `j` in milliseconds. */
export const colEdges = (m, j) => (m.shape === "raw" ? [(m.c0 + j) * m.sx, (m.c0 + j + 1) * m.sx] : [m.times[j] - m.step, m.times[j]]);

/** The edges `[low, high]` of row `k` in value units. A `bucketed` row has no value edges: it gives the index band. */
export function rowEdges(m, k) {
  if (m.shape !== "raw") return [k - 0.5, k + 0.5];
  return m.log ? [m.log ** ((m.r0 + k) / m.split), m.log ** ((m.r0 + k + 1) / m.split)] : [(m.r0 + k) * m.sy, (m.r0 + k + 1) * m.sy];
}

/** The edges of row `k` on the plot coordinate `u`: the value for a linear scale, its logarithm for a log scale, the index band for a `bucketed` frame. */
export const rowU = (m, k) => (m.shape === "raw" && m.log ? [(m.r0 + k) / m.split, (m.r0 + k + 1) / m.split] : rowEdges(m, k));

/** The range of the value axis on `u` (9.19e, section 2, rule 12). `min` and `max` are `y_min` and `y_max`. A log scale rounds them out to a power. */
export function axisRange(m, min, max) {
  const lo = rowU(m, 0)[0];
  const hi = rowU(m, m.ny - 1)[1];
  if (m.shape !== "raw") return [lo, hi];
  const lg = m.log === 2 ? Math.log2 : Math.log10;
  return m.log ? [min === null ? lo : Math.floor(lg(min)), max === null ? hi : Math.ceil(lg(max))] : [min === null ? lo : min, max === null ? hi : max];
}

/** The level (0 to `steps - 1`) of a count (9.19e, section 4, rule 3). */
export const levelOf = (v, min, max, steps) => (v < min ? 0 : v > max ? steps - 1 : Math.min(steps - 1, Math.floor((steps * (v - min)) / (max - min || 1))));

/** True when a count is drawn: a finite number above `hide_le` and below `hide_ge` (9.19e, section 4, rule 1). `o` has `hide_le` and `hide_ge`. */
export const isShown = (v, o) => v - v === 0 && (o.hide_le === null || v > o.hide_le) && (o.hide_ge === null || v < o.hide_ge);

/**
 * The drawn cells (9.19e, section 4). `o` has `hide_le`, `hide_ge` and the `color` object. Gives the arrays of the engine in column order:
 * `x0`, `x1`, `y0`, `y1` (the plot coordinates), `level` (the index plus 1), and `col`, `row`, `count` for the card. `min` and `max` are the colour range.
 */
export function heatCells(m, o) {
  const { nx, ny, counts } = m;
  const shown = (v) => isShown(v, o);
  let n = 0;
  let lo = Infinity;
  let hi = -Infinity;
  for (let i = 0; i < counts.length; i++) {
    const v = counts[i];
    if (!shown(v)) continue;
    n++;
    if (v < lo) lo = v;
    if (v > hi) hi = v;
  }
  const min = o.color.min === null ? lo : o.color.min;
  const max = o.color.max === null ? hi : o.color.max;
  const c = { n, min, max, x0: new Float64Array(n), x1: new Float64Array(n), y0: new Float64Array(n), y1: new Float64Array(n), level: new Uint8Array(n), col: new Uint32Array(n), row: new Uint32Array(n), count: new Float64Array(n) };
  const ys = [];
  for (let r = 0; r < ny; r++) ys.push(rowU(m, r));
  let k = 0;
  for (let j = 0; j < nx; j++) {
    const [a, b] = colEdges(m, j);
    for (let r = 0; r < ny; r++) {
      const v = counts[j * ny + r];
      if (!shown(v)) continue;
      c.x0[k] = a;
      c.x1[k] = b;
      c.y0[k] = ys[r][0];
      c.y1[k] = ys[r][1];
      c.level[k] = levelOf(v, min, max, o.color.steps) + 1;
      c.col[k] = j;
      c.row[k] = r;
      c.count[k] = v;
      k++;
    }
  }
  return c;
}

/** The text of the bucket of row `k`: the two edges with `fmt` for a `raw` grid, the name for a `bucketed` frame. */
export const bucketText = (m, k, fmt) => (m.shape === "raw" ? rowEdges(m, k).map(fmt).join("-") : m.labels[k]);

// --- the colour scale (9.19e, section 5) ---------------------------------------------------------------------------------------------

/** The schemes: the rule of the inversion (`dark`, `light` or `always`) and the stops of the B-spline (hex, without `#`). */
export const SCHEMES = Object.freeze({
  oranges: ["dark", "fff5eb fee6ce fdd0a2 fdae6b fd8d3c f16913 d94801 a63603 7f2704"],
  blues: ["dark", "f7fbff deebf7 c6dbef 9ecae1 6baed6 4292c6 2171b5 08519c 08306b"],
  greens: ["dark", "f7fcf5 e5f5e0 c7e9c0 a1d99b 74c476 41ab5d 238b45 006d2c 00441b"],
  greys: ["dark", "ffffff f0f0f0 d9d9d9 bdbdbd 969696 737373 525252 252525 000000"],
  purples: ["dark", "fcfbfd efedf5 dadaeb bcbddc 9e9ac8 807dba 6a51a3 54278f 3f007d"],
  reds: ["dark", "fff5f0 fee0d2 fcbba1 fc9272 fb6a4a ef3b2c cb181d a50f15 67000d"],
  yl_or_rd: ["dark", "ffffcc ffeda0 fed976 feb24c fd8d3c fc4e2a e31a1c bd0026 800026"],
  rd_yl_gn: ["always", "a50026 d73027 f46d43 fdae61 fee08b ffffbf d9ef8b a6d96a 66bd63 1a9850 006837"],
  spectral: ["always", "9e0142 d53e4f f46d43 fdae61 fee08b ffffbf e6f598 abdda4 66c2a5 3288bd 5e4fa2"],
  viridis: ["light", "440154 48186a 472d7b 424086 3b528b 33638d 2c728e 26828e 21908c 1f9f88 27ad81 3ebc74 5cc863 82d34d aadc32 d5e21a fde725"],
});

const hex2 = (n) => n.toString(16).padStart(2, "0");

/**
 * The `steps` colours of a scale (9.19e, section 5), as `#rrggbb` (the mode `scheme`) or `#rrggbbaa` (the mode `opacity`).
 * `color` is the object of 4.33c. The first colour is for the lowest count.
 */
export function heatPalette(color, dark) {
  const n = color.steps;
  const out = [];
  if (color.mode === "opacity") {
    const c = parseCss(resolveFill(color.fill, dark)) || (dark ? { r: 204, g: 204, b: 220 } : { r: 36, g: 41, b: 46 });
    for (let j = 0; j < n; j++) {
      const f = j / (n - 1);
      out.push(`#${hex2(c.r)}${hex2(c.g)}${hex2(c.b)}${hex2(Math.round(255 * (color.scale === "linear" ? f : f ** color.exponent)))}`);
    }
    return out;
  }
  const [rule, text] = SCHEMES[color.scheme];
  const rgb = text.split(" ").map((s) => [0, 2, 4].map((k) => parseInt(s.slice(k, k + 2), 16)));
  for (let j = 0; j < n; j++) out.push(`#${[0, 1, 2].map((c) => hex2(basisChannel(rgb.map((s) => s[c]), n > 1 ? j / (n - 1) : 0))).join("")}`);
  if (rule === "always" || (rule === "dark") === dark) out.reverse();
  if (color.reverse) out.reverse();
  return out;
}

// --- the axis (9.19e, section 6) -----------------------------------------------------------------------------------------------------

/** The ticks of a log axis on `u` for a plot `h` px high (9.19e, section 6, rule 4): from the top edge of the last row, `m` powers apart. */
export function logTicks(m, h) {
  const [u0, u1] = axisRange(m, null, null);
  const pitch = h / (u1 - u0);
  let step = 1;
  while (step * pitch < 30 && step < 1e4) step++;
  const out = [];
  for (let u = rowU(m, m.ny - 1)[1]; u >= u0 - 1e-9; u -= step) out.push(u);
  return out;
}

/** The indexes of the row labels of an ordinal axis for `n` rows in a plot `h` px high (9.19e, section 6, rule 5). */
export function ordinalLabels(n, h) {
  if (h < 60) return n > 1 ? [0, n - 1] : [0];
  let step = 1;
  while (Math.ceil(n / step) > 3 && (h - 15) / Math.ceil(n / step) < 10) step *= 2;
  const out = [];
  for (let k = 0; k < n; k += step) out.push(k);
  return out;
}

/**
 * The text of the hidden description of a heatmap (9.19e, section 9). `c` is the result of `heatCells`, `fmt` writes a value with the unit of
 * the field and `zone` is the zone of the dashboard.
 */
export function heatDescription(m, c, fmt, zone) {
  const head = `${plain(m.nx)} columns and ${plain(m.ny)} rows.`;
  if (!c.n) return `${head} No cell is drawn.`;
  let top = 0;
  for (let i = 1; i < c.n; i++) if (c.count[i] > c.count[top]) top = i;
  const where = `at ${formatTime(colEdges(m, c.col[top])[1], zone)}, in the ${m.shape === "raw" ? "bucket" : "row"} ${clip(bucketText(m, c.row[top], fmt))}`;
  return `${head} The highest count is ${shortText(c.count[top])}, ${where}. ${plain(c.n)} cells are drawn.`;
}
