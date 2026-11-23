// The shared part of the bucket maths and the histogram (9.19f, FE-43, FE-44, FE-46). Pure module: no DOM access.
// The heatmap has its own module, `heat_model.js` (9.19e), which imports the helpers below. The page does not import this file yet.
import { formatNumber, joinParts } from "./units.js";

/** The cap of the values that the page reads for the shape `raw` and for the values of a histogram (9.19e, 9.19f, section 3 and 4). */
export const MAX_BUCKET_VALUES = 500000;
/** The cap of the buckets of a histogram, empty buckets included (9.19f, section 4). */
export const MAX_HIST_BUCKETS = 4000;

// Numbers `m * 10^k` as the decimal text gives them: the double that Grafana rounds to.
export const decimal = (m, k) => Number(`${m}e${k}`);

/** The list `Z` of the bucket sizes of a histogram (9.19f, section 2, rule 3): `m * 10^k`, `k` from -9 to 9, `m` in 1, 2, 2.5, 4, 5. */
export const HIST_SIZES = Object.freeze(
  (() => {
    const out = [];
    for (let k = -9; k <= 9; k++) for (const m of [1, 2, 2.5, 4, 5]) out.push(decimal(m, k));
    return out;
  })(),
);

/** The bucket size of a histogram: the first `R` of `Z` with `y < R` and `R >= h`, or 0 when none fits. */
export function niceHist(y, h) {
  for (const r of HIST_SIZES) if (y < r && r >= h) return r;
  return 0;
}

/** A count with thousands separators. A count of 1e15 or more, and NaN, give a fixed text: the message stays short for absurd data. */
export const plain = (n) => (n < 1e15 ? n.toLocaleString("en-US") : "more than 1,000,000,000,000,000");
/** True for a finite number (false for NaN and the infinities). */
export const finite = (v) => v - v === 0;
/** True for a field of the kind `number` or `number_text`. */
export const isNumber = (f) => f.kind === "number" || f.kind === "number_text";
/** True for a field that has `hide_from.viz`. */
export const hidden = (f) => Boolean(f.config && f.config.hide_from && f.config.hide_from.viz);

/** The numbers of a field of the kind `number` or `number_text`: NaN for a null (`num` of a text field is `parseFloat` of the text, 9.19e, section 1). */
export const numbers = (f) => (f.kind === "number" ? f.values : f.num);

/** The message of a result that has more values than the cap. */
export const tooMany = (n) => `Too many values to bucket: ${plain(n)}. The limit is ${plain(MAX_BUCKET_VALUES)}. Bucket the values in the SQL.`;
const tooManyBuckets = (n) => `Too many buckets: ${plain(n)}. The limit is ${plain(MAX_HIST_BUCKETS)}.`;

/** The text of a count in the short number form (9.19, `4.91 K`). */
export const shortText = (v) => joinParts(formatNumber(v, "short", null));

// --- the histogram (9.19f) -----------------------------------------------------------------------------------------------------------

const r9 = (v) => Number(v.toFixed(9));
const r6 = (v) => Number(v.toFixed(6));

/**
 * The model of a histogram (9.19f). `options` are the options of 4.33d. Gives `{ error }`, `{ limit }` or
 * `{ shape, size, nb, lows, highs, series, field }`. A series has `name`, `counts` (one item for each bucket), `field` (its source field, `null`
 * for `combine`), `legend`, `tooltip`, `max` and `at` (the highest count and its bucket). The top `field` gives the unit of the edges.
 */
export function buildHistogram({ fields, names, options }) {
  const picked = [];
  fields.forEach((f, i) => {
    if (isNumber(f) && !hidden(f)) picked.push(i);
  });
  if (!picked.length) return { error: "The data has no number column." };
  const flag = (i, key) => !(fields[i].config.hide_from && fields[i].config.hide_from[key]);
  const series = (name, counts, i) => {
    let at = 0;
    for (let j = 1; j < counts.length; j++) if (counts[j] > counts[at] || counts[at] !== counts[at]) at = j;
    return { name, counts, field: i < 0 ? null : fields[i], legend: i < 0 || flag(i, "legend"), tooltip: i < 0 || flag(i, "tooltip"), max: counts.length ? counts[at] : 0, at };
  };
  const xmin = picked.find((i) => names[i] === "xMin" || names[i] === "BucketMin");
  if (xmin !== undefined) {
    const xmax = picked.find((i) => names[i] === "xMax" || names[i] === "BucketMax");
    const rest = picked.filter((i) => i !== xmin && i !== xmax);
    if (!rest.length) return { error: "The data has no number column." };
    const lo = numbers(fields[xmin]);
    const order = [];
    for (let i = 0; i < lo.length; i++) if (finite(lo[i])) order.push(i);
    if (order.length > MAX_HIST_BUCKETS) return { limit: tooManyBuckets(order.length) };
    order.sort((a, b) => lo[a] - lo[b]);
    let w;
    if (xmax !== undefined) w = numbers(fields[xmax])[order[0]] - lo[order[0]];
    else if (order.length > 1) w = r6(lo[order[1]] - lo[order[0]]);
    if (!(w > 0)) return { error: "The buckets have no width." };
    const lows = Float64Array.from(order, (i) => lo[i]);
    const counts = rest.map((i) => {
      const col = numbers(fields[i]);
      return series(names[i], Float64Array.from(order, (r) => col[r]), i);
    });
    return { shape: "bucketed", size: w, nb: lows.length, lows, highs: lows.map((v) => r9(v + w)), series: counts, field: fields[xmin] };
  }
  let n = 0;
  for (const i of picked) n += fields[i].values.length;
  if (n > MAX_BUCKET_VALUES) return { limit: tooMany(n) };
  const cols = picked.map((i) => numbers(fields[i]));
  const all = new Float64Array(n);
  let m = 0;
  for (const col of cols) for (let i = 0; i < col.length; i++) if (finite(col[i])) all[m++] = col[i];
  if (!m) return { error: "The data has no value to bucket." };
  const sorted = all.subarray(0, m).sort();
  const offset = options.bucket_offset;
  let size = options.bucket_size;
  if (!(size > 0)) {
    let h = m === 1 ? 1 : Infinity;
    for (let i = 1; i < m; i++) {
      const d = sorted[i] - sorted[i - 1];
      if (d !== 0 && d < h) h = d;
    }
    size = h === Infinity ? 1 : niceHist((sorted[m - 1] - sorted[0]) / options.bucket_count, h);
    if (!size) return { error: "No bucket size fits the values." };
  }
  const at = (v) => Math.floor((v - offset) / size);
  const b0 = at(sorted[0]);
  const nb = at(sorted[m - 1]) - b0 + 1;
  if (!(nb <= MAX_HIST_BUCKETS)) return { limit: tooManyBuckets(nb) };
  const rows = cols.map((col) => {
    const counts = new Uint32Array(nb);
    for (let i = 0; i < col.length; i++) if (finite(col[i])) counts[at(col[i]) - b0]++;
    return counts;
  });
  let list;
  if (options.combine) {
    const sum = new Uint32Array(nb);
    for (const r of rows) for (let j = 0; j < nb; j++) sum[j] += r[j];
    list = [series("Count", sum, -1)];
  } else list = rows.map((r, k) => series(names[picked[k]], r, picked[k]));
  const edge = (j) => r9((b0 + j) * size + offset);
  const lows = new Float64Array(nb);
  const highs = new Float64Array(nb);
  for (let j = 0; j < nb; j++) {
    lows[j] = edge(j);
    highs[j] = edge(j + 1);
  }
  return { shape: "values", size, nb, lows, highs, series: list, field: fields[picked[0]] };
}

/** The edges that show on the x axis of a histogram in a plot `w` px wide: every `k`-th edge, `k = ceil(50 / bucket pitch)` (9.19f, section 3). */
export function histTicks(m, w) {
  const span = m.highs[m.nb - 1] - m.lows[0];
  const k = Math.max(1, Math.ceil(50 / ((w * m.size) / span)));
  const out = [];
  for (let j = 0; j <= m.nb; j += k) out.push(j < m.nb ? m.lows[j] : m.highs[m.nb - 1]);
  return out;
}

// A name cut at `n` characters, never inside a pair of surrogates. The texts are plain strings: the page writes them with `textContent`.
/** The text of a name, cut at 120 characters with an ellipsis. */
export const clip = (s, n = 120) => (s.length <= n ? s : `${Array.from(s.slice(0, 2 * n)).slice(0, n).join("")}\u2026`);

/** The text of the hidden description of a histogram (9.19f, section 5): the number of buckets, the size and the highest count of each series. */
export function histDescription(m, fmt) {
  const parts = [`${plain(m.nb)} buckets of the size ${fmt(m.size)}.`];
  for (const s of m.series) parts.push(`${clip(s.name)}: the highest count is ${shortText(s.max)}, in the bucket ${fmt(m.lows[s.at])} - ${fmt(m.highs[s.at])}.`);
  return parts.join(" ");
}
