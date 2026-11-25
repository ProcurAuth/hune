// The wait of a panel (9.11): which sign a panel shows while it waits, the shapes of the preview of each panel type, and the test that tells
// if a new answer holds the data that the panel already draws. Pure module: no DOM access.

// The states of a panel that wait for a result.
export const WAITING = new Set(["idle", "queued", "loading"]);

// What the body shows when the state is a waiting state (9.11):
//   "first"  nothing is on the screen: the panel never had a result, or its last try failed. It shows the preview of its type and the dots.
//   "reload" the panel has content, or its "No data" text. It keeps them as they are and shows the dots only.
//   ""       the panel does not wait.
// `shown` tells what is on the screen: "content" (a drawn panel), "empty" (the text "No data") or "nothing".
export function waitMode(state, shown) {
  if (!WAITING.has(state)) return "";
  return shown === "content" || shown === "empty" ? "reload" : "first";
}

// The preview of each panel type. A panel type that draws no queried data (a text) has none.
const KIND_OF_TYPE = {
  timeseries: "series",
  xychart: "series",
  barchart: "bars",
  stat: "stat",
  gauge: "gauge",
  bargauge: "bar-gauge",
  table: "table",
  piechart: "pie",
  heatmap: "heatmap",
  histogram: "histogram",
  state_timeline: "timeline",
  status_history: "timeline",
  logs: "logs",
  geomap: "geomap",
};

// The kind of preview of a panel type, or "" when the type has none.
export function skeletonKind(type) {
  return Object.prototype.hasOwnProperty.call(KIND_OF_TYPE, type) ? KIND_OF_TYPE[type] : "";
}

// The shapes. Each shape is { s, x, y, w, h, d }: the form (`bar`, `dot`, `line`, `ring`, `cell`, `blob`), the position and the size in per cent
// of the body, and the delay of its animation in 0.1 s steps. A shape moves its opacity only: it never moves and never changes its size.
const bar = (x, y, w, h, d = 0) => ({ s: "bar", x, y, w, h, d });
const dot = (x, y, d = 0) => ({ s: "dot", x, y, w: 0, h: 0, d });
const line = (y) => ({ s: "line", x: 6, y, w: 88, h: 0, d: 0 });
const cell = (x, y, w, h, d = 0) => ({ s: "cell", x, y, w, h, d });

const range = (n) => Array.from({ length: n }, (_, i) => i);

export const SKELETONS = {
  // Four grid lines and a wave of sixteen dots.
  series: [
    ...[15, 38, 61, 84].map(line),
    ...[62, 55, 58, 48, 50, 38, 44, 32, 40, 50, 56, 44, 36, 42, 34, 46].map((y, i) => dot(8 + i * 5.6, y, i)),
  ],
  // Seven bars on one base line.
  bars: [line(84), ...[40, 62, 48, 72, 36, 58, 44].map((h, i) => bar(10 + i * 12, 84 - h, 7, h, i))],
  // A big value, a caption and a short run of dots.
  stat: [
    bar(30, 26, 40, 26, 0),
    bar(38, 60, 24, 6, 2),
    ...[80, 76, 78, 72, 74].map((y, i) => dot(24 + i * 13, y, i)),
  ],
  // One ring and the value in its middle.
  gauge: [{ s: "ring", x: 50, y: 12, w: 0, h: 70, d: 0 }, bar(40, 44, 20, 12, 1)],
  // Three tracks, each one with a fill.
  "bar-gauge": [24, 46, 68].flatMap((y, i) => [bar(10, y, 80, 10, 0), bar(10, y, [60, 35, 75][i], 10, i + 1)]),
  // A header row and five rows of three cells.
  table: [
    ...[8, 38, 74].map((x, i) => cell(x, 10, [24, 30, 18][i], 8, 0)),
    ...range(5).flatMap((r) => [8, 38, 74].map((x, i) => cell(x, 26 + r * 14, [24, 30, 18][i], 7, r + 1))),
  ],
  // A ring of three parts.
  pie: [{ s: "ring", x: 50, y: 10, w: 0, h: 74, d: 0, parts: true }],
  // A grid of eight by four cells, with a wave from the top left.
  heatmap: range(4).flatMap((r) => range(8).map((c) => cell(8 + c * 10.5, 14 + r * 17, 9, 14, c + r))),
  // Ten bars side by side, like a bell.
  histogram: [line(84), ...[12, 22, 38, 56, 72, 60, 40, 24, 14, 8].map((h, i) => bar(8 + i * 8.4, 84 - h, 7.8, h, i))],
  // Three rows of segments.
  timeline: [
    [[6, 28], [36, 20], [58, 32]],
    [[6, 14], [22, 40], [64, 28]],
    [[6, 36], [44, 16], [62, 30]],
  ].flatMap((row, r) => row.map(([x, w], i) => bar(x, 20 + r * 22, w, 12, r + i))),
  // Six log lines: a level dot, then a line.
  logs: [70, 58, 76, 50, 64, 44].flatMap((w, i) => [dot(8, 15 + i * 14, i), bar(14, 12 + i * 14, w, 6, i)]),
  // A map shape and five markers.
  geomap: [
    { s: "blob", x: 14, y: 16, w: 46, h: 62, d: 0 },
    { s: "blob", x: 44, y: 28, w: 40, h: 54, d: 1 },
    ...[[30, 40], [48, 58], [62, 44], [70, 66], [38, 70]].map(([x, y], i) => dot(x, y, i)),
  ],
};

const COLUMN_CAP = 500000;

// True when two answers hold the data that a panel draws: the same columns, the same interval and the same values in the same order.
// `a` and `b` are the lists { ref, frame } of the answers. The members `query_id`, `stats`, `sql` and `etag` change with each request and
// are not data: the panel does not draw them. A frame of more than 500,000 cells counts as different, so the test stays cheap.
export function sameFrames(a, b) {
  if (!a || !b || a.length !== b.length) return false;
  let cells = 0;
  for (let i = 0; i < a.length; i++) {
    if (a[i].ref !== b[i].ref) return false;
    const x = a[i].frame;
    const y = b[i].frame;
    if (x === y) continue;
    if (!x || !y) return false;
    if (x.interval_ms !== y.interval_ms || x.rows !== y.rows) return false;
    if (JSON.stringify(x.columns) !== JSON.stringify(y.columns)) return false;
    const vx = x.values || [];
    const vy = y.values || [];
    if (vx.length !== vy.length) return false;
    for (let c = 0; c < vx.length; c++) {
      const p = vx[c];
      const q = vy[c];
      if (p === q) continue;
      if (!Array.isArray(p) || !Array.isArray(q) || p.length !== q.length) return false;
      cells += p.length;
      if (cells > COLUMN_CAP) return false;
      for (let r = 0; r < p.length; r++) if (p[r] !== q[r]) return false;
    }
  }
  return true;
}

// The range of a panel as a text, for the test of a redraw: a chart draws its x axis over the range, so a new range needs a new drawing
// even when the data are the same.
export const rangeKey = (r) => (r && !r.error && Number.isFinite(r.from_ms) && Number.isFinite(r.to_ms) ? `${r.from_ms}:${r.to_ms}` : "");
