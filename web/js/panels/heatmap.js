// The heatmap panel (9.19e, 4.33c): the copied chart engine draws the cells, the axes and the pick of a cell under the pointer.
// The page adds the colour bar, the card, the keys and the text alternative. The maths are in heat_model.js. A text of the data goes in
// with textContent only: the engine escapes the texts of its card.
import { clip, shortText } from "../bucket_model.js";
import { axisRange, bucketText, buildHeatmap, colEdges, heatCells, heatDescription, heatPalette, isShown, levelOf, logTicks, ordinalLabels, rowU } from "../heat_model.js";
import { el, emptyState, uniqueId } from "../ui.js";
import { formatTime } from "../units.js";
import { zoneClock } from "../zone.js";
import { createChart, isDarkTheme, joinPartsOf, yTickLabeller } from "./common.js";
import { niceStepAtLeast } from "./series.js";

// The test hook (9.29) keeps 2,000 cells at most, then it sets `cut`. The card of `multi` keeps 100 rows at most and counts the rest.
const REPORT_CELLS = 2000;
const CARD_ROWS = 100;
// The bar of the legend is 300 px wide. A content box narrower than 350 px gets a bar of its width less 50 px, 10 px at least (a rule of Hune: 9.19e, section 5).
const BAR_WIDTH = 300;
const KEYS = { ArrowRight: [1, 0], ArrowLeft: [-1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1] };

// The time range of the x axis: the range of the page, else the span of the times (the preview of the editor gives no range).
function rangeOf(ctx, fields) {
  const r = typeof ctx.linkContext === "function" ? ctx.linkContext() : null;
  if (r && Number.isFinite(r.from) && Number.isFinite(r.to) && r.to > r.from) return { from: r.from, to: r.to };
  const time = fields.find((f) => f.kind === "time");
  let from = Infinity;
  let to = -Infinity;
  for (const t of time ? time.values : []) {
    if (t < from) from = t;
    if (t > to) to = t;
  }
  return to > from ? { from, to } : { from: 0, to: 1 };
}

// Draws a panel of type `heatmap` in `host` (9.19e). `ctx` gives the panel, the fields, the names, the zone and the range.
// An error of the model goes to the frame as an exception. A cap gives the state `bucket-limit` and a message, with no picture.
// The return value is the handle of the frame: `destroy` and `note` (the text of the values that a log scale skipped, for "View data").
export function render(host, ctx) {
  const { panel, fields, names, zone } = ctx;
  const o = panel.options;
  const id = panel.id;
  const range = rangeOf(ctx, fields);
  const m = buildHeatmap({ fields, names, options: o, range });
  if (m.error) throw new Error(m.error);
  const root = el("div", { class: "heat", dataset: { testid: `heatmap-${id}` } });
  host.append(root);
  if (m.limit) {
    root.append(el("p", { class: "bucket__note", role: "note", text: m.limit, dataset: { testid: `heatmap-${id}-note` } }));
    return { state: "bucket-limit", destroy: () => root.remove() };
  }
  const c = heatCells(m, o);
  if (!c.n) {
    root.append(emptyState({ icon: null, title: "No data", inline: true }));
    return { state: "empty", destroy: () => root.remove() };
  }
  const color = o.color;
  const sign = o.y_reverse ? -1 : 1;
  const mode = o.tooltip ? o.tooltip.mode : "single";
  const field = m.field;
  const fmt = (v) => joinPartsOf(field, v);
  const tick = yTickLabeller(field);
  let palette = heatPalette(color, isDarkTheme());
  // The engine draws a cell on a linear plot coordinate. A reversed axis uses the coordinate `-u` (9.19e, section 6, rule 6).
  const cells = { x0: c.x0, x1: c.x1, y0: sign < 0 ? c.y1.map((v) => -v) : c.y0, y1: sign < 0 ? c.y0.map((v) => -v) : c.y1, level: c.level, palette };
  const xs = Float64Array.from({ length: m.nx }, (_, j) => {
    const [a, b] = colEdges(m, j);
    return (a + b) / 2;
  });

  // The value axis (9.19e, section 6): the range, then the ticks of the scale. `h` is the plot height. A tick is `[u, text]`.
  function axis(h) {
    let [lo, hi] = axisRange(m, o.y_min, o.y_max);
    if (!(hi > lo) || !Number.isFinite(hi - lo)) [lo, hi] = axisRange(m, null, null);
    const ticks = [];
    if (m.shape !== "raw") for (const k of ordinalLabels(m.ny, h)) ticks.push([k, clip(m.labels[k])]);
    else if (m.log) {
      // The ticks are `m` powers apart, as many as the range on the axis needs for 30 px (the pitch is the height of one power).
      const [u0, u1] = axisRange(m, null, null);
      for (const u of logTicks(m, (h * (u1 - u0)) / (hi - lo))) if (u >= lo && u <= hi) ticks.push([u, tick(m.log ** u, m.log ** u)]);
    } else {
      const step = niceStepAtLeast((30 * (hi - lo)) / h);
      for (let k = Math.ceil(lo / step - 1e-9); k * step <= hi + step * 1e-9 && ticks.length < 200; k++) ticks.push([Number((k * step).toPrecision(12)), tick(Number((k * step).toPrecision(12)), step)]);
    }
    return { min: sign < 0 ? -hi : lo, max: sign < 0 ? -lo : hi, step: 1, ticks: ticks.map(([u, label]) => ({ v: sign * u, label })) };
  }

  // The cell under a position: `[column, row]` or null. `x` is the time and `u` the plot coordinate (9.19e, section 6).
  function where(x, u) {
    const raw = m.shape === "raw";
    let j;
    if (raw) j = Math.floor(x / m.sx) - m.c0;
    else {
      // The first column whose end is not before x. A column spans `end - step` to `end`.
      let a = 0;
      let b = m.nx;
      while (a < b) {
        const mid = (a + b) >> 1;
        if (m.times[mid] < x) a = mid + 1;
        else b = mid;
      }
      j = a < m.nx && x > m.times[a] - m.step ? a : -1;
    }
    const k = raw ? Math.floor(m.log ? u * m.split : u / m.sy) - m.r0 : Math.floor(u + 0.5);
    return j >= 0 && j < m.nx && k >= 0 && k < m.ny ? [j, k] : null;
  }

  // The card (9.19e, section 8): the head is the end of the time bucket. `single` has two rows, `multi` has one row for each drawn cell of the column.
  const count = (j, k) => m.counts[j * m.ny + k];
  const dot = (v) => palette[levelOf(v, c.min, c.max, color.steps)];
  const named = m.shape === "raw" ? "Bucket" : "Name";
  const card = (j, k) => {
    const title = formatTime(colEdges(m, j)[1], zone);
    if (mode !== "multi") {
      const v = count(j, k);
      return { title, rows: [{ label: m.shape === "raw" ? "Count" : "Value", value: shortText(v - v === 0 ? v : 0), color: dot(v) }, { label: named, value: clip(bucketText(m, k, fmt)) }] };
    }
    const rows = [];
    let more = 0;
    for (let r = 0; r < m.ny; r++) {
      const v = count(j, r);
      if (!isShown(v, o)) continue;
      if (rows.length < CARD_ROWS) rows.push({ label: `${named} ${clip(bucketText(m, r, fmt))}`, value: shortText(v), color: dot(v) });
      else more++;
    }
    return { title, rows, footer: more ? `${more.toLocaleString("en-US")} more` : "" };
  };

  // The cell that the pointer or the keys mark: the region of the engine, with the class `heat__hover` (9.19e, section 7).
  let chart;
  let lit = null;
  const mark = (at) => {
    if (at === lit || (at && lit && at[0] === lit[0] && at[1] === lit[1])) return;
    lit = at;
    if (!at) return chart.setRegions([]);
    const [x0, x1] = colEdges(m, at[0]);
    const [u0, u1] = rowU(m, at[1]);
    chart.setRegions([{ id: "heat", x0, x1, y0: sign < 0 ? -u1 : u0, y1: sign < 0 ? -u0 : u1, className: "heat__hover" }]);
  };

  // The legend (9.19e, section 5, rule 7): a bar of the `steps` colours and the two ends of the colour range.
  const bar = el("canvas", { class: "heat__bar", width: BAR_WIDTH, height: 9, "aria-hidden": "true" });
  const legend = el("div", { class: "heat__legend", hidden: !o.show_legend, dataset: { testid: `heatmap-${id}-legend` } }, bar, el("div", { class: "heat__labels" }, el("span", { text: shortText(c.min) }), el("span", { text: shortText(c.max) })));
  const paintBar = () => {
    const g = bar.getContext("2d");
    g.clearRect(0, 0, bar.width, bar.height);
    for (let x = 0; x < bar.width; x++) {
      g.fillStyle = palette[Math.floor((x * palette.length) / bar.width)];
      g.fillRect(x, 0, 1, bar.height);
    }
  };
  const descId = uniqueId("heatmap-desc");
  const main = el("div", { class: "heat__main" });
  const skip = m.note ? [el("p", { class: "heat__skip", role: "note", text: m.note, dataset: { testid: `heatmap-${id}-skip` } })] : [];
  root.append(main, legend, ...skip, el("p", { class: "visually-hidden", id: descId, text: heatDescription(m, c, fmt, zone), dataset: { testid: `heatmap-${id}-desc` } }));

  // The hook of 9.29: written after each draw.
  const report = [];
  for (let i = 0; i < Math.min(c.n, REPORT_CELLS); i++) report.push([c.col[i], c.row[i], c.count[i], c.level[i] - 1]);
  const raw = m.shape === "raw";
  const hook = () => {
    const engine = main.querySelector(".chartCore");
    host.dataset.heatmap = JSON.stringify({
      shape: m.shape, cols: m.nx, rows: m.ny, drawn: Number(engine ? engine.dataset.cellsDrawn : 0) || 0, sx: raw ? m.sx : m.step, sy: raw && !m.log ? m.sy : null,
      scale: raw ? (m.log ? `log${m.log}` : "linear") : "ordinal", min: c.min, max: c.max, steps: color.steps, palette: [palette[0], palette[palette.length - 1]],
      legend: o.show_legend ? [c.min, c.max] : null, cells: report, cut: c.n > REPORT_CELLS,
    });
  };

  chart = createChart(main, {
    height: Math.max(60, main.clientHeight),
    xKind: "time",
    zone: zoneClock(zone),
    xs,
    xDomain: [range.from, range.to],
    series: [],
    type: "line",
    legend: false,
    keyboard: false,
    zoomable: false,
    tooltip: "none",
    yShown: o.y_axis !== "hidden",
    yAxis: (min, max, h) => axis(h),
    cells,
    cellGap: o.cell_gap,
    pick: (pt) => {
      const at = where(pt.x, sign * pt.y);
      if (!at || !isShown(count(at[0], at[1]), o)) return mark(null);
      mark(at);
      return { key: `${at[0]}:${at[1]}`, at };
    },
    pickTooltip: (hit) => (mode === "hidden" ? null : card(hit.at[0], hit.at[1])),
    onDraw: hook,
  });
  const tip = chart.root.querySelector(".chartCore__tooltip");
  if (tip) tip.dataset.testid = `heatmap-${id}-tip`;

  // The canvas of the engine is the picture (9.19e, section 9): `role="img"`, a name, a description and the keys.
  const canvas = chart.root.querySelector(".chartCore__overlay");
  canvas.setAttribute("role", "img");
  canvas.removeAttribute("aria-roledescription");
  canvas.setAttribute("aria-label", `${panel.title || "Untitled"}, heatmap`);
  canvas.setAttribute("aria-describedby", descId);
  canvas.tabIndex = 0;
  let active = null; // the position of the keys: [column, row]
  const showActive = () => {
    mark(active);
    if (!active) return chart.hideTooltip();
    const [a, b] = colEdges(m, active[0]);
    const [u0, u1] = rowU(m, active[1]);
    const L = chart.layout();
    chart.showTooltip(mode === "hidden" ? null : card(active[0], active[1]), L.xOf((a + b) / 2), L.yOf((sign * (u0 + u1)) / 2));
  };
  const clamp = (v, n) => Math.max(0, Math.min(n - 1, v));
  // The listener is on the parent in the capture phase: the keys that it handles do not reach the keyboard cursor of the engine.
  const onKey = (ev) => {
    if (ev.ctrlKey || ev.altKey || ev.metaKey) return;
    const step = KEYS[ev.key];
    if (ev.key === "Escape") active = null;
    else if (step || ev.key === "Home" || ev.key === "End") {
      const [j, k] = active || [0, 0];
      if (!active) active = [0, 0];
      else active = ev.key === "Home" ? [0, k] : ev.key === "End" ? [m.nx - 1, k] : [clamp(j + step[0], m.nx), clamp(k + sign * step[1], m.ny)];
    } else return;
    ev.preventDefault();
    ev.stopPropagation();
    showActive();
  };
  main.addEventListener("keydown", onKey, true);
  const onLeave = () => mark(active);
  const onBlur = () => {
    active = null;
    showActive();
  };
  canvas.addEventListener("pointerleave", onLeave);
  canvas.addEventListener("blur", onBlur);

  // The size: the plot takes the height of the body, and the bar of a narrow body is shorter. The palette follows the theme.
  let last = Math.max(60, main.clientHeight);
  let gone = false; // the host had no width: its frame was out of the page
  const fit = () => {
    const width = root.clientWidth < 350 ? Math.max(10, root.clientWidth - 50) : BAR_WIDTH;
    if (width !== bar.width) bar.width = width;
    paintBar();
    const height = Math.max(60, main.clientHeight);
    if (height !== last) chart.update({ height });
    last = height;
    // A heatmap that returns to the page draws before the paint, not one frame later (10.2a, rule 17).
    if (root.clientWidth && gone) {
      chart.redraw();
      chart.flush();
    }
    gone = !root.clientWidth;
  };
  const observer = new ResizeObserver(fit);
  observer.observe(root);
  const onTheme = () => {
    palette = heatPalette(color, isDarkTheme());
    cells.palette = palette;
    paintBar();
    chart.redraw();
  };
  document.addEventListener("hune-theme", onTheme);
  const shown = () => gone && fit(); // 10.2a, rule 17 (common.js, fitHeight)
  document.addEventListener("hune-shown", shown);
  fit();
  return {
    note: m.note,
    destroy() {
      observer.disconnect();
      document.removeEventListener("hune-theme", onTheme);
      document.removeEventListener("hune-shown", shown);
      chart.destroy();
      root.remove();
    },
  };
}
