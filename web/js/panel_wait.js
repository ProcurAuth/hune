// The wait of a panel (9.11): which sign a panel shows while it waits, and the test that tells if a new answer holds the data that the panel
// already draws. Pure module: no DOM access. The preview of each panel type is in web/js/preview/ (registry: index.js).

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
