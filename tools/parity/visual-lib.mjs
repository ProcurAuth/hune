// SPDX-License-Identifier: MIT OR Apache-2.0
// Pure functions of the visual parity capture (15.14c). This module has no browser, no network and no file access, so
// that a unit test can run each function (tests/tools/parity_visual.test.mjs).
//
// The tool `visual.mjs` records, in Grafana and in Hune, the canvas calls of each panel (the hook `canvas-hook.js`),
// the display names of the series, the box and the title of each panel and each copy of a repeat. This module
// pairs the two records and compares them with the tolerances of 15.14c.

/**
 * The tolerances of 15.14c. A box moves by 1 px at most. A colour of an exact scheme has 2 in each channel, `viridis` has 8.
 * A mark of a canvas has 1.5 px: the engine of Hune snaps each edge of a cell to a device pixel (0.5 px), on top of the 1 px.
 */
export const TOLERANCE = Object.freeze({ box: 1, marks: 1.5, colour: 2, viridis: 8, alpha: 0.02 });

// --- Colours -----------------------------------------------------------------------------------------------------------------------

/** A CSS colour as [r, g, b, a] (a from 0 to 1), or null for a gradient or a text that this parser does not know. */
export function parseColour(text) {
  if (typeof text !== "string") return null;
  const t = text.trim().toLowerCase();
  let m = /^#([0-9a-f]{3})$/.exec(t);
  if (m) return [...m[1]].map((h) => parseInt(h + h, 16)).concat(1);
  m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/.exec(t);
  if (m) return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16)).concat(m[2] ? parseInt(m[2], 16) / 255 : 1);
  m = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,/]+([\d.]+%?))?\s*\)$/.exec(t);
  if (m) {
    const a = m[4] === undefined ? 1 : m[4].endsWith("%") ? Number(m[4].slice(0, -1)) / 100 : Number(m[4]);
    return [Number(m[1]), Number(m[2]), Number(m[3]), a];
  }
  return null;
}

/** The colour of a drawing call: the colour with the global alpha applied. */
export function colourOf(op) {
  const c = parseColour(op.color);
  if (!c) return null;
  return [c[0], c[1], c[2], Math.round(c[3] * (op.alpha ?? 1) * 1000) / 1000];
}

/** The largest difference of a channel (alpha counted in 255 units), or Infinity when one colour is unknown. */
export function colourDistance(a, b) {
  if (!a || !b) return a === b ? 0 : Infinity;
  const rgb = Math.max(Math.abs(a[0] - b[0]), Math.abs(a[1] - b[1]), Math.abs(a[2] - b[2]));
  // A fully transparent cell shows no colour: its channels do not count.
  if (a[3] === 0 && b[3] === 0) return 0;
  return Math.max(rgb, Math.abs(a[3] - b[3]) * 255);
}

// --- The marks of a canvas ---------------------------------------------------------------------------------------------------------

/** The intersection of a rectangle `[x, y, w, h]` with a clip `[x, y, w, h]` or null (no clip). Width and height are 0 or more. */
export function clipRect(r, clip) {
  if (!clip) return r;
  const x0 = Math.max(r[0], clip[0]);
  const y0 = Math.max(r[1], clip[1]);
  const x1 = Math.min(r[0] + r[2], clip[0] + clip[2]);
  const y1 = Math.min(r[1] + r[3], clip[1] + clip[3]);
  return [x0, y0, Math.max(0, x1 - x0), Math.max(0, y1 - y0)];
}

/**
 * The plot of Grafana from a clip `[x, y, w, h]` of its canvas. uPlot clips a series to the plot with half the line width more on each side, so the
 * plot is the clip less half the widest line that the canvas strokes in this clip. Without such a line, the plot is the clip.
 */
export function grafanaPlot(canvas, clip) {
  if (!clip) return clip;
  const key = JSON.stringify(clip);
  const line = (canvas?.ops ?? []).reduce((w, o) => (o.k === "stroke" && JSON.stringify(o.clip ?? null) === key ? Math.max(w, o.width ?? 0) : w), 0);
  return line > 0 ? [clip[0] + line / 2, clip[1] + line / 2, clip[2] - line, clip[3] - line].map((v) => Math.round(v * 1000) / 1000) : clip;
}

/**
 * The plot of a canvas from its lines: the clip that holds the most strokes (the lines of a time series). For Grafana (`grafana`), the plot is this
 * clip less half the widest line (`grafanaPlot`). It gives null when no stroke has a clip.
 */
export function linePlotOf(canvas, { grafana = false } = {}) {
  const count = new Map();
  for (const o of canvas?.ops ?? []) if (o.k === "stroke" && o.clip) count.set(JSON.stringify(o.clip), (count.get(JSON.stringify(o.clip)) ?? 0) + 1);
  let best = null;
  for (const [key, n] of count) if (best === null || n > best.n) best = { key, n };
  if (!best) return null;
  const clip = JSON.parse(best.key);
  return grafana ? grafanaPlot(canvas, clip) : clip;
}

/**
 * The filled rectangles of a canvas record `{ ops }` that show in its plot, each as `{ box, colour }` (the rectangle as drawn).
 * The plot is the clip rectangle that holds the most rectangles. A rectangle with no area inside the plot is not a mark.
 * The result holds the plot rectangle too. For the canvas of Grafana (`grafana`), the plot is the clip less half the widest line in it
 * (`grafanaPlot`): with a line of 3 px, the clip of the bars is 1.5 px larger than the plot on each side.
 */
export function marksOf(canvas, { grafana = false } = {}) {
  const fills = (canvas?.ops ?? []).filter((o) => (o.k === "fill" || o.k === "fillRect") && o.rects?.length);
  const count = new Map();
  for (const o of fills) {
    const key = JSON.stringify(o.clip ?? null);
    count.set(key, (count.get(key) ?? 0) + o.rects.length);
  }
  let best = null;
  for (const [key, n] of count) if (best === null || n > best.n) best = { key, n };
  const clip = best ? JSON.parse(best.key) : null;
  const plot = grafana ? grafanaPlot(canvas, clip) : clip;
  const marks = [];
  for (const o of fills) {
    if (JSON.stringify(o.clip ?? null) !== best?.key) continue;
    const colour = colourOf(o);
    for (const r of o.rects) {
      // A rectangle with no area inside the plot is not visible, so it is not a mark.
      // The clip of Hune is 1 px larger than its plot above and below: the test uses the plot with an inset of 1 px on both sides.
      const seen = clipRect(r, plot ? [plot[0], plot[1] + 1, plot[2], Math.max(0, plot[3] - 2)] : null);
      if (seen[2] < 0.5 || seen[3] < 0.5) continue;
      marks.push({ box: r, colour, raw: o.color });
    }
  }
  return { plot, marks };
}

/** The texts that a canvas drew, in draw order: `[{ text, x, y }]`. */
export function textsOf(canvas) {
  return (canvas?.ops ?? []).filter((o) => o.k === "text").map((o) => ({ text: o.text, x: o.x, y: o.y }));
}

/** The text of a list as a sorted array (the order of the draw is not a fact that the two products share). */
export const textBag = (list) => list.map((t) => t.text).sort();

// --- The pairing of the marks --------------------------------------------------------------------------------------------------------

/**
 * Pair each mark of Grafana with the nearest mark of Hune whose centre is at most `reach` px away on each axis.
 * It returns `{ pairs: [[g, h]], onlyG: [g], onlyH: [h] }`. Both lists keep the order of their input.
 */
export function pairMarks(gMarks, hMarks, reach = 2) {
  const cell = 4;
  const grid = new Map();
  const centre = (m) => [m.box[0] + m.box[2] / 2, m.box[1] + m.box[3] / 2];
  hMarks.forEach((m, i) => {
    const [cx, cy] = centre(m);
    const key = `${Math.floor(cx / cell)},${Math.floor(cy / cell)}`;
    if (!grid.has(key)) grid.set(key, []);
    grid.get(key).push(i);
  });
  const used = new Set();
  const pairs = [];
  const onlyG = [];
  const span = Math.ceil(reach / cell) + 1;
  for (const g of gMarks) {
    const [gx, gy] = centre(g);
    const gi = Math.floor(gx / cell);
    const gj = Math.floor(gy / cell);
    let best = null;
    for (let a = -span; a <= span; a++) {
      for (let b = -span; b <= span; b++) {
        for (const i of grid.get(`${gi + a},${gj + b}`) ?? []) {
          if (used.has(i)) continue;
          const [hx, hy] = centre(hMarks[i]);
          const dx = Math.abs(hx - gx);
          const dy = Math.abs(hy - gy);
          if (dx > reach || dy > reach) continue;
          const d = dx + dy;
          if (best === null || d < best.d) best = { i, d };
        }
      }
    }
    if (best) {
      used.add(best.i);
      pairs.push([g, hMarks[best.i]]);
    } else onlyG.push(g);
  }
  const onlyH = hMarks.filter((_, i) => !used.has(i));
  return { pairs, onlyG, onlyH };
}

/** The largest difference of each edge of the paired boxes: `{ x, y, w, h }`, all in px. */
export function boxDelta(pairs) {
  const out = { x: 0, y: 0, w: 0, h: 0 };
  for (const [g, h] of pairs) {
    out.x = Math.max(out.x, Math.abs(g.box[0] - h.box[0]));
    out.y = Math.max(out.y, Math.abs(g.box[1] - h.box[1]));
    out.w = Math.max(out.w, Math.abs(g.box[2] - h.box[2]));
    out.h = Math.max(out.h, Math.abs(g.box[3] - h.box[3]));
  }
  for (const k of Object.keys(out)) out[k] = Math.round(out[k] * 100) / 100;
  return out;
}

/**
 * Compare the marks of two canvases. `colourTolerance` is the tolerance of one channel (2, or 8 for `viridis`).
 * The result has the counts, the unpaired marks, the largest box difference, the largest colour difference and the
 * list of the pairs whose colour is outside the tolerance (at most 8 of them, as examples).
 */
export function compareMarks(gCanvas, hCanvas, { colourTolerance = TOLERANCE.colour, boxTolerance = TOLERANCE.marks, mode = "cells", byValue = null } = {}) {
  const g = marksOf(gCanvas, { grafana: true });
  const h0 = marksOf(hCanvas);
  // The x axes of the two products cover other ranges (`byValue`, two scales of `xScaleOf`): each bar of Hune goes to the pixels of Grafana through
  // its x value, so that the bars pair by bucket.
  if (byValue) h0.marks = h0.marks.map((m) => ({ ...m, box: [byValue.g.a + (byValue.g.k * (m.box[0] - byValue.h.a)) / byValue.h.k, m.box[1], (m.box[2] * byValue.g.k) / byValue.h.k, m.box[3]] }));
  // The marks compare in the pixels of the canvas, with the rectangle that the page drew (before the clip). The difference of the
  // left edges of the two plots is a fact of the report (`plot_shift`).
  // The x and the width of both plots are exact (the clip of Hune, and the clip of Grafana less half its line), so the marks of Hune map into the
  // plot of Grafana in x whenever the plots differ: a wider label of the y axis then moves no mark. In y, the clip of Hune is 1 px larger above
  // and below, so a height that differs by 4 px or less is not mapped. A larger difference (a note under the plot) maps too. The `plot` check
  // reports the difference of the plots.
  const plotShift = g.plot && h0.plot ? Math.round((g.plot[0] - h0.plot[0]) * 1000) / 1000 : 0;
  const sx = g.plot && h0.plot && h0.plot[2] > 0 ? g.plot[2] / h0.plot[2] : 1;
  const sy = g.plot && h0.plot && h0.plot[3] > 0 ? g.plot[3] / h0.plot[3] : 1;
  const mapX = !byValue && g.plot && h0.plot && (Math.abs(plotShift) > 0.01 || Math.abs(g.plot[2] - h0.plot[2]) > 0.01);
  const mapY = g.plot && h0.plot && Math.abs(g.plot[3] - h0.plot[3]) > 4;
  const mapped = mapX || mapY;
  const h = !mapped
    ? h0
    : {
        ...h0,
        marks: h0.marks.map((m) => {
          const x = mapX ? g.plot[0] + (m.box[0] - h0.plot[0]) * sx : m.box[0];
          const w = mapX ? m.box[2] * sx : m.box[2];
          const y = mapY ? g.plot[1] + (m.box[1] - h0.plot[1]) * sy : m.box[1];
          const hh = mapY ? m.box[3] * sy : m.box[3];
          return { ...m, box: [x, y, w, hh] };
        }),
      };
  const paired = mode === "bars" ? pairBars(g, h) : pairMarks(g.marks, h.marks);
  const { pairs, onlyG } = paired;
  let onlyH = paired.onlyH;
  // A bar of 4 px or less (1 device px at least for each series, and the rounding of a stack) that Hune draws for an empty bucket has no bar in Grafana (19.20g).
  const slivers = mode === "bars" ? onlyH.filter((m) => m.box[3] <= 4) : [];
  if (slivers.length) onlyH = onlyH.filter((m) => !slivers.includes(m));
  const delta = mode === "bars" ? barDelta(pairs, g, paired.h) : boxDelta(pairs);
  let worst = 0;
  const badColours = [];
  for (const [a, b] of pairs) {
    const d = colourDistance(a.colour, b.colour);
    if (d > worst) worst = d;
    if (d > colourTolerance && badColours.length < 8) badColours.push({ box: a.box, grafana: a.raw, hune: b.raw, distance: Math.round(d * 100) / 100 });
  }
  const boxOk = Math.max(delta.x, delta.y, delta.w, delta.h) <= boxTolerance;
  return {
    grafana: g.marks.length,
    hune: h.marks.length,
    // A rectangle of Grafana with no finite x: the canvas draws nothing for it (one bucket, 19.20g).
    grafana_no_x: g.marks.filter((m) => !Number.isFinite(m.box[0])).length,
    paired: pairs.length,
    only_grafana: onlyG.slice(0, 8).map((m) => ({ box: m.box, colour: m.raw })),
    only_grafana_count: onlyG.length,
    only_hune: onlyH.slice(0, 8).map((m) => ({ box: m.box, colour: m.raw })),
    only_hune_count: onlyH.length,
    box_delta: delta,
    box_ok: boxOk,
    colour_worst: worst === Infinity ? "unknown" : Math.round(worst * 100) / 100,
    colour_bad_count: pairs.filter(([a, b]) => colourDistance(a.colour, b.colour) > colourTolerance).length,
    colour_bad: badColours,
    plot: { grafana: g.plot, hune: h0.plot },
    plot_shift: plotShift,
    slivers: slivers.length,
    plot_width_delta: g.plot && h0.plot ? Math.round(Math.abs(g.plot[2] - h0.plot[2]) * 100) / 100 : null,
    plot_height_delta: g.plot && h0.plot ? Math.round(Math.abs(g.plot[3] - h0.plot[3]) * 100) / 100 : null,
    plot_top_delta: g.plot && h0.plot ? Math.round(Math.abs(g.plot[1] - h0.plot[1]) * 100) / 100 : null,
  };
}

/**
 * The pairing of the bars of a histogram. The bars pair by the x of their centre (at most 2 px apart after the map of the plot);
 * the y axes of the two products differ (the engine picks other ticks), so the height is not part of the pairing. Bars that
 * share an x (two fields draw bars at the same place) pair in the order of the drawing.
 */
export function pairBars(g, h) {
  const centre = (m) => m.box[0] + m.box[2] / 2;
  const clusters = (marks) => {
    const out = [];
    for (const m of [...marks].sort((a, b) => centre(a) - centre(b))) {
      const last = out[out.length - 1];
      if (last && Math.abs(centre(m) - last.x) <= 2) last.items.push(m);
      else out.push({ x: centre(m), items: [m] });
    }
    return out;
  };
  const order = (marks, all) => all.items.sort((a, b) => marks.indexOf(a) - marks.indexOf(b));
  const gc = clusters(g.marks);
  const hc = clusters(h.marks);
  const pairs = [];
  const onlyG = [];
  const usedH = new Set();
  for (const c of gc) {
    let best = -1;
    for (let i = 0; i < hc.length; i++) if (!usedH.has(i) && Math.abs(hc[i].x - c.x) <= 2 && (best < 0 || Math.abs(hc[i].x - c.x) < Math.abs(hc[best].x - c.x))) best = i;
    if (best < 0) {
      onlyG.push(...c.items);
      continue;
    }
    usedH.add(best);
    const a = order(g.marks, c);
    const b = order(h.marks, hc[best]);
    for (let k = 0; k < Math.max(a.length, b.length); k++) {
      if (a[k] && b[k]) pairs.push([a[k], b[k]]);
      else if (a[k]) onlyG.push(a[k]);
    }
  }
  const pairedH = new Set(pairs.map((p) => p[1]));
  const onlyH = h.marks.filter((m) => !pairedH.has(m));
  return { pairs, onlyG, onlyH, h };
}

/** The difference of the bars: x and width in px, the height as a share of the tallest bar of each side, in px of the plot of Grafana. */
export function barDelta(pairs, g, h) {
  const top = (marks) => marks.reduce((a, m) => Math.max(a, m.box[3]), 0) || 1;
  const hg = top(g.marks);
  const hh = top(h.marks);
  const plotH = g.plot ? g.plot[3] : hg;
  const out = { x: 0, y: 0, w: 0, h: 0 };
  for (const [a, b] of pairs) {
    out.x = Math.max(out.x, Math.abs(a.box[0] - b.box[0]));
    out.w = Math.max(out.w, Math.abs(a.box[2] - b.box[2]));
    out.h = Math.max(out.h, Math.abs(a.box[3] / hg - b.box[3] / hh) * plotH);
  }
  for (const k of Object.keys(out)) out[k] = Math.round(out[k] * 100) / 100;
  return out;
}

/** Split the texts of a canvas into the labels of the x axis (below the plot) and the others (the y axis). */
export function splitTexts(canvas) {
  const plot = marksOf(canvas).plot;
  const all = textsOf(canvas).filter((t) => t.text !== "");
  if (!plot) return { x: [], y: all };
  const bottom = plot[1] + plot[3] - 2;
  return { x: all.filter((t) => t.y >= bottom), y: all.filter((t) => t.y < bottom) };
}

/**
 * The scale of a number axis from its labels `[{ text, x }]`: `{ a, k }` with `x = a + k * value`, or null with less than two numeric labels.
 * The first guess is the median slope and the median offset of the labels. A label more than 3 px off this guess is not used: the last label of
 * Hune moves to stay in the canvas, and the label 0 of the y axis sits at the bottom of the plot. A least-squares line through the other labels is
 * the scale (Grafana rounds the x of a label to a whole pixel).
 */
export function xScaleOf(labels) {
  const pts = labels.map((t) => [Number(t.text), t.x]).filter(([v, x]) => Number.isFinite(v) && Number.isFinite(x)).sort((p, q) => p[0] - q[0]);
  const median = (l) => [...l].sort((p, q) => p - q)[Math.floor(l.length / 2)];
  const slopes = [];
  for (let i = 1; i < pts.length; i++) if (pts[i][0] > pts[i - 1][0]) slopes.push((pts[i][1] - pts[i - 1][1]) / (pts[i][0] - pts[i - 1][0]));
  if (!slopes.length) return null;
  const k0 = median(slopes);
  const a0 = median(pts.map(([v, x]) => x - k0 * v));
  const ok = pts.filter(([v, x]) => Math.abs(x - (a0 + k0 * v)) <= 3);
  const n = ok.length;
  const sum = (f) => ok.reduce((t, p) => t + f(p), 0);
  const den = n * sum(([v]) => v * v) - sum(([v]) => v) ** 2;
  if (n < 2 || !(Math.abs(den) > 0)) return { a: a0, k: k0 };
  const k = (n * sum(([v, x]) => v * x) - sum(([v]) => v) * sum(([, x]) => x)) / den;
  return { a: (sum(([, x]) => x) - k * sum(([v]) => v)) / n, k };
}

/** Compare two lists of drawn texts as bags. It returns `{ equal, only_grafana, only_hune }`. */
export function compareTexts(gTexts, hTexts) {
  const count = (list) => {
    const m = new Map();
    for (const t of list) m.set(t, (m.get(t) ?? 0) + 1);
    return m;
  };
  const g = count(textBag(gTexts));
  const h = count(textBag(hTexts));
  const only = (a, b) => [...a].flatMap(([t, n]) => Array(Math.max(0, n - (b.get(t) ?? 0))).fill(t));
  const og = only(g, h);
  const oh = only(h, g);
  return { equal: og.length === 0 && oh.length === 0, only_grafana: og, only_hune: oh };
}

/** The tolerance of one channel for the colour scheme of a panel (15.14c): 8 for `viridis`, else 2. */
export function colourToleranceOf(scheme) {
  return typeof scheme === "string" && scheme.toLowerCase() === "viridis" ? TOLERANCE.viridis : TOLERANCE.colour;
}

// --- The boxes ---------------------------------------------------------------------------------------------------------------------

/** The difference of two boxes `[x, y, w, h]` and whether each edge is inside the tolerance. */
export function compareBox(g, h, tolerance = TOLERANCE.box) {
  if (!g || !h) return { ok: false, delta: null };
  const delta = g.map((v, i) => Math.round(Math.abs(v - h[i]) * 100) / 100);
  return { ok: delta.every((d) => d <= tolerance), delta };
}

// --- The repeat (docs/07 15.14c, 9.8c) -----------------------------------------------------------------------------------------------

/** The key of Grafana for the copy n of a panel (n from 1): `panel-<id>` for the first copy, else `panel-<id>-clone-<n - 1>`. */
export const grafanaCopyKey = (id, n) => (n <= 1 ? `panel-${id}` : `panel-${id}-clone-${n - 1}`);
/** The `data-testid` of Hune for the copy n of a panel: `panel-<id>` for the first copy, else `panel-<id>-r<n>`. */
export const huneCopyTestid = (id, n) => (n <= 1 ? `panel-${id}` : `panel-${id}-r${n}`);

/**
 * The pairing of the copies of a repeat. `key` is the `data-viz-panel-key` of Grafana: `panel-<id>` or `panel-<id>-clone-<k>`.
 * It returns `{ id, n }` with n from 1 (the copy k of Grafana is the copy k + 1), or null for another text.
 */
export function parseGrafanaKey(key) {
  const m = /^panel-(\d+)(?:-clone-(\d+))?$/.exec(String(key));
  if (!m) return null;
  return { id: Number(m[1]), n: m[2] === undefined ? 1 : Number(m[2]) + 1 };
}

/** The same for the `data-testid` of Hune: `panel-<id>` or `panel-<id>-r<n>` with n from 2. */
export function parseHuneTestid(testid) {
  const m = /^panel-(\d+)(?:-r(\d+))?$/.exec(String(testid));
  if (!m) return null;
  const n = m[2] === undefined ? 1 : Number(m[2]);
  return n >= 2 || m[2] === undefined ? { id: Number(m[1]), n } : null;
}

/**
 * Pair the panels of the two products by (id, copy). `g` and `h` are lists of `{ key, ... }` records in document order:
 * `key` is the key of Grafana or the test id of Hune. A panel of a repeated section has the same key in Grafana for each
 * copy of the section: the n-th record with this key is the copy n.
 * It returns `{ pairs: [{ id, n, grafana, hune }], onlyG, onlyH }`.
 */
export function pairCopies(g, h) {
  const seen = new Map();
  const gs = new Map();
  for (const r of g) {
    const p = parseGrafanaKey(r.key);
    if (!p) continue;
    // A key that repeats (a repeated section) counts its occurrences.
    const times = (seen.get(r.key) ?? 0) + 1;
    seen.set(r.key, times);
    const n = p.n === 1 ? times : p.n;
    gs.set(`${p.id}:${n}`, { ...r, id: p.id, n });
  }
  const hs = new Map();
  for (const r of h) {
    const p = parseHuneTestid(r.key);
    if (p) hs.set(`${p.id}:${p.n}`, { ...r, id: p.id, n: p.n });
  }
  const pairs = [];
  const onlyG = [];
  for (const [k, gr] of gs) {
    const hr = hs.get(k);
    if (hr) pairs.push({ id: gr.id, n: gr.n, grafana: gr, hune: hr });
    else onlyG.push(gr);
  }
  const onlyH = [...hs].filter(([k]) => !gs.has(k)).map(([, v]) => v);
  pairs.sort((a, b) => a.id - b.id || a.n - b.n);
  return { pairs, onlyG, onlyH };
}

/**
 * The comparison of one pair of copies: the box, the title and the number of queries (docs/07 15.14c).
 * A copy of the first row of a section repeat is placed by the section, so the boxes compare relative to the first panel of the
 * dashboard when `origin` is given: `{ grafana: [x, y], hune: [x, y] }`.
 */
export function compareCopy(pair, { origin = null, tolerance = TOLERANCE.box } = {}) {
  const shift = (box, o) => (o ? [box[0] - o[0], box[1] - o[1], box[2], box[3]] : box);
  const box = compareBox(shift(pair.grafana.box, origin?.grafana), shift(pair.hune.box, origin?.hune), tolerance);
  return {
    id: pair.id,
    n: pair.n,
    grafana_key: pair.grafana.key,
    hune_key: pair.hune.key,
    box_ok: box.ok,
    box_delta: box.delta,
    title: { grafana: pair.grafana.title ?? null, hune: pair.hune.title ?? null, equal: (pair.grafana.title ?? null) === (pair.hune.title ?? null) },
    queries: { grafana: pair.grafana.queries ?? null, hune: pair.hune.queries ?? null, equal: (pair.grafana.queries ?? null) === (pair.hune.queries ?? null) },
  };
}

// --- The queries of a copy --------------------------------------------------------------------------------------------------------

/** The queries that a Grafana request holds. */
const grafanaQueryCount = (r) => {
  try {
    return (JSON.parse(r.body ?? "{}").queries ?? []).length;
  } catch {
    return 0;
  }
};

/**
 * The number of queries that each copy of a panel sent in Grafana, in the order of the copies. Grafana gives each clone another
 * `X-Panel-Id` (a hash) and keeps the title as the author wrote it. The requests that share a title and an id are one copy: the copies
 * keep the order in which their first request left the page, which is the order of the values on a page that shows all the values.
 * `plain` is true for a panel with no repeat: then the id of the panel selects the requests.
 */
export function grafanaCopyQueries(requests, { title, id = null, plain = false }) {
  const groups = new Map();
  for (const r of requests) {
    const h = r.headers ?? {};
    if (plain ? h["x-panel-id"] !== String(id) : h["x-panel-title"] !== title) continue;
    const key = h["x-panel-id"] ?? "";
    groups.set(key, (groups.get(key) ?? 0) + grafanaQueryCount(r));
  }
  return [...groups.values()];
}

/**
 * The number of queries that Hune sent for the copy of a panel that has the value `value` of the variable `variable`. Hune sends one
 * request for each query, with the values of the copy in `variables`. A panel with no repeat (`variable` null) counts the requests of
 * its id.
 */
export function huneCopyQueries(requests, { id, variable = null, value = null }) {
  let n = 0;
  for (const r of requests) {
    const m = /\/panels\/(\d+)\/query$/.exec(r.url);
    if (!m || Number(m[1]) !== id) continue;
    if (variable === null) {
      n++;
      continue;
    }
    let body;
    try {
      body = JSON.parse(r.body ?? "{}");
    } catch {
      continue;
    }
    const v = body.variables?.[variable];
    if (v && !v.all && v.values.length === 1 && v.values[0] === value) n++;
  }
  return n;
}

// --- The view URL of a copy (9.5, 10.2a) -----------------------------------------------------------------------------------------------

/**
 * The view URL value of Grafana for a copy: `<value>$panel-<id>`. `<value>` is the VALUE of the repeat variable (the option value, not
 * the text and not an index), and the first copy has it too (measure on Grafana 12.4.2, 2026-11-15).
 */
export const grafanaViewValue = (value, id) => `${value}$panel-${id}`;

// --- The names of the series (F24) ---------------------------------------------------------------------------------------------------

/** The names of two lists as one comparison: the same names in the same order. */
export function compareNames(g, h) {
  const same = g.length === h.length && g.every((v, i) => v === h[i]);
  const sorted = (l) => [...l].sort();
  const bag = sorted(g).join("\n") === sorted(h).join("\n");
  return { equal: same, same_set: bag, grafana: g, hune: h };
}

// --- The report ---------------------------------------------------------------------------------------------------------------------

/** The class of a difference: `documented` with the id of a section of 19.20, or `fault`. */
export const classify = (rule) => (rule ? { class: "documented", rule } : { class: "fault", rule: null });

/** The Markdown text of a list of records `{ fixture, panel, check, status, class, rule, detail }`. */
export function renderReport(records, meta = {}) {
  const lines = [`# Visual parity capture`, ""];
  for (const [k, v] of Object.entries(meta)) lines.push(`- ${k}: ${v}`);
  lines.push("");
  const total = records.length;
  const equal = records.filter((r) => r.status === "equal").length;
  const documented = records.filter((r) => r.status === "different" && r.class === "documented").length;
  const faults = records.filter((r) => r.status === "different" && r.class === "fault").length;
  lines.push(`Checks: ${total}. Equal: ${equal}. Documented differences: ${documented}. Faults: ${faults}.`, "");
  lines.push("| Fixture | Panel | Check | Status | Class | Rule | Detail |", "|---|---|---|---|---|---|---|");
  for (const r of records) {
    const detail = (typeof r.detail === "string" ? r.detail : JSON.stringify(r.detail ?? "")).replace(/\|/g, "\\|").slice(0, 300);
    lines.push(`| ${r.fixture} | ${r.panel} | ${r.check} | ${r.status} | ${r.class ?? ""} | ${r.rule ?? ""} | ${detail} |`);
  }
  return lines.join("\n") + "\n";
}
