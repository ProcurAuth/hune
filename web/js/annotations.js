// The annotations of a dashboard (4.5b, 9.19d): which one applies to a panel, the check of an answer, the marks, the cap and the
// hit test. Pure module: no DOM access. The server makes the marks (7.37). This module reads them as data and checks their shape.

/** The limits of 4.35. The numbers equal the constants of `hune-core`, and a unit test checks them. */
export const MAX_ANNOTATIONS = 8;
export const MAX_ANNOTATION_PANELS = 200;
export const MAX_ANNOTATION_ROWS = 1000;
export const MAX_ANNOTATIONS_PER_PANEL = 2000;
export const MAX_ANNOTATION_TITLE_CHARS = 256;
export const MAX_ANNOTATION_TEXT_BYTES = 2048;
export const MAX_ANNOTATION_TAGS = 8;
export const MAX_ANNOTATION_TAG_CHARS = 64;

/** The geometry of 9.19d in CSS pixels: the triangle and the bar, the hit zone (above and under the plot) and the reach of a line. */
export const TRIANGLE = { w: 10, h: 5 };
export const ZONE = { above: 16, under: 8, reach: 8 };
export const CARD = { gap: 12, lift: 8 };

/** True when the annotation `a` draws on the panel `id`: it is enabled, and `panels` is absent or names the id (9.19d, rule 4). */
export const applies = (a, id) => a.enabled !== false && (!a.panels || a.panels.includes(id));

/** The indexes of the annotations of the document `doc` that draw on the panel `id`. */
export function forPanel(doc, id) {
  const out = [];
  (doc.annotations || []).forEach((a, index) => {
    if (applies(a, id)) out.push(index);
  });
  return out;
}

const BAD_ANSWER = "The answer of the annotation is not valid.";
const isTime = (v) => typeof v === "number" && Number.isFinite(v);
// At most `n` characters (code points, as the server counts them): a UTF-16 length above 2n is already too long.
const within = (s, n) => s.length <= n || (s.length <= 2 * n && [...s].length <= n);
const utf8 = new TextEncoder();

/**
 * Checks the answer of the route of 6.21a and gives the list of marks `{ t, end, title, text, tags }`. `end` is null for a point.
 * An answer with another shape throws an `Error` with a fixed message: the page never draws a value that it did not check.
 */
export function readAnswer(json) {
  const rows = json && json.annotations;
  if (!Array.isArray(rows) || rows.length > MAX_ANNOTATION_ROWS || json.rows !== rows.length) throw new Error(BAD_ANSWER);
  return rows.map((r) => {
    const ok = Array.isArray(r) && r.length === 5 && isTime(r[0]) && (r[1] === null || isTime(r[1])) && typeof r[2] === "string" && typeof r[3] === "string"
      && Array.isArray(r[4]) && r[4].length <= MAX_ANNOTATION_TAGS && r[4].every((x) => typeof x === "string" && within(x, MAX_ANNOTATION_TAG_CHARS))
      && (r[1] === null || r[1] >= r[0]) && within(r[2], MAX_ANNOTATION_TITLE_CHARS) && r[3].length <= MAX_ANNOTATION_TEXT_BYTES && utf8.encode(r[3]).length <= MAX_ANNOTATION_TEXT_BYTES;
    if (!ok) throw new Error(BAD_ANSWER);
    return { t: r[0], end: r[1], title: r[2], text: r[3], tags: r[4] };
  });
}

/**
 * The marks of a panel. `records` are the annotations that apply to it: `{ index, a: { name, color }, items, error }`. The cap counts the
 * marks out of the range too. Above it `list` is empty and `cut` is true: no cut and no sample (fail closed). `list` holds the marks
 * that touch [lo, hi], in the order of the time, and a tie keeps the order of the annotations. `failed` lists the annotations with an error.
 */
export function marks(records, lo, hi) {
  let n = 0;
  const failed = [];
  const live = [];
  for (const r of records) {
    if (r.error) failed.push({ name: r.a.name, error: r.error });
    else {
      n += r.items.length;
      live.push(r);
    }
  }
  if (n > MAX_ANNOTATIONS_PER_PANEL) return { n, cut: true, list: [], failed };
  const list = [];
  for (const r of live) for (const it of r.items) if (it.t <= hi && (it.end === null ? it.t : it.end) >= lo) list.push({ ...it, index: r.index, color: r.a.color });
  return { n, cut: false, list: list.sort((x, y) => x.t - y.t), failed };
}

/** The note of the title bar (9.19d, rule 16): `{ text, tip }`, or null when nothing is wrong. `info` is the result of `marks`. */
export function noteOf(info) {
  if (!info.cut && !info.failed.length) return null;
  const cut = `Annotations: too many (${info.n}, at most ${MAX_ANNOTATIONS_PER_PANEL.toLocaleString("en-US")})`;
  const lines = info.failed.map((f) => `${f.name}: ${f.error.code ? `${f.error.code}: ` : ""}${f.error.message}`);
  return { text: info.cut ? cut : `Annotations: ${info.failed.length} failed`, tip: [...(info.cut ? [cut] : []), ...lines].join("\n") };
}

/** The x of the start and of the end of a mark in px. `xOf` is the scale of the chart. A point has the same value twice. */
export function span(m, xOf) {
  const a = xOf(m.t);
  return [a, m.end === null ? a : Math.max(a, xOf(m.end))];
}

/** True when the pointer (py, in px of the plot) is in the hit zone: a band that starts above the bottom of the plot and ends under it. */
export const inZone = (plot, py) => py >= plot.top + plot.h - ZONE.above && py <= plot.top + plot.h + ZONE.under;

/**
 * The mark under the pointer, or null. The pointer must be in the zone. A mark matches when its line is within `ZONE.reach` px, or when the
 * pointer is inside the width of a region. The nearer mark wins, then the earlier time. `plot` is `{ left, top, w, h }`.
 */
export function hitTest(list, xOf, plot, px, py) {
  if (!inZone(plot, py)) return null;
  let best = null;
  let near = Infinity;
  for (const m of list) {
    const [a, b] = span(m, xOf);
    const d = px < a ? a - px : px > b ? px - b : 0;
    if (d <= ZONE.reach && d < near) {
      best = m;
      near = d;
    }
  }
  return best;
}

/** The place of the card in px of the plot: 12 px right of the pointer (it flips left at the right edge), 8 px above the zone, inside the plot. */
export function cardPlace(plot, px, cw, ch) {
  let x = px + CARD.gap;
  if (x + cw > plot.left + plot.w) x = px - CARD.gap - cw;
  const y = plot.top + plot.h - ZONE.above - CARD.lift - ch;
  return { x: Math.max(plot.left, x), y: Math.max(plot.top, y) };
}

/** The texts of the card: the head (one time, or `<start> - <end>`), the body (title, a line end, text) and the tags (8 at most). */
export function cardText(m, fmt) {
  return {
    head: m.end === null ? fmt(m.t) : `${fmt(m.t)} - ${fmt(m.end)}`,
    body: m.title && m.text ? `${m.title}\n${m.text}` : m.title || m.text,
    tags: m.tags.slice(0, MAX_ANNOTATION_TAGS),
  };
}

/** The index of the active mark after a key, or -1 for a key that has no meaning. `at` is -1 before the first key. */
export function stepKey(count, at, key) {
  if (!count) return -1;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  if (key === "ArrowRight") return Math.min(count - 1, at + 1);
  if (key === "ArrowLeft") return Math.max(0, at - 1);
  return -1;
}

/** The rows of the second table of "View data" (9.11): time, end, title, text and tags as texts. */
export function tableRows(list, fmt) {
  return list.map((m) => [fmt(m.t), m.end === null ? "" : fmt(m.end), m.title, m.text, m.tags.join(", ")]);
}

/** The value of the test hook `data-annotations` (9.29): the item of a mark is `[x, x_end, kind, color, index]`; 200 items at most. */
export function hookOf(info, shown, xOf, plot, inkOf) {
  const items = shown.slice(0, 200).map((m) => {
    const [a, b] = span(m, xOf);
    return [Math.round(a * 100) / 100, Math.round(b * 100) / 100, m.end === null ? 0 : 1, inkOf(m.color), m.index];
  });
  return { n: info.n, drawn: shown.length, cut: info.cut, plot: [plot.left, plot.top, plot.w, plot.h], marks: items };
}
