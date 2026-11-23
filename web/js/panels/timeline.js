// The state timeline and the status history (9.19c): a canvas, the legend, the card and the text alternative. The code is code of Hune.
import { parseCss } from "../colors.js";
import { buildTimeline, boxAt, boxText, cellWidth, durationText, gutterOf, legendItems, limitMessage, MIN_LABEL_PITCH, rowBox } from "../timeline_model.js";
import { chartCore } from "../vendor/chdash/app_chart_core.js";
import { el, uniqueId } from "../ui.js";
import { formatTime, zoneClock } from "../zone.js";
import { canvasColor, isDarkTheme, timeFractionDigits } from "./common.js";
import { cellInk } from "./items.js";

// The geometry of the plot in CSS pixels: PAD is the top and the right margin, and AXIS is the height of the band of the time axis.
const PAD = 8;
const AXIS = 21;
// The test hook (9.29) keeps 2,000 segments and 2,000 texts at most. Then it sets `cut`.
const MAX_REPORT = 2000;
const round2 = (v) => Math.round(v * 100) / 100;

// Draws a panel of type `state_timeline` or `status_history` in `host` (9.19c).
// `ctx` gives the panel, the fields, the names, the zone and the hooks of the frame (`setState`, `linkContext`).
// The model of `timeline_model.js` makes the rows, the states and the boxes. This function draws them on a canvas.
// It also adds the legend, the card, the keys of 9.26, the text alternative and the note of a limit.
// An error of the model goes to the frame as an exception, and the frame shows it. The return value is the handle of the frame.
export function render(host, ctx) {
  const { panel, fields, names } = ctx;
  const o = panel.options;
  const history = panel.type === "status_history";
  const range = typeof ctx.linkContext === "function" ? ctx.linkContext() : null;
  const model = buildTimeline({ kind: panel.type, fields, names, options: o, zone: ctx.zone, range: range && Number.isFinite(range.from) && Number.isFinite(range.to) && range.to > range.from ? range : null });
  if (model.error) throw new Error(model.error);
  const id = panel.id;
  const descId = uniqueId("timeline-desc");
  const pattern = `YYYY-MM-DD HH:mm:ss${timeFractionDigits(model.times) ? ".SSS" : ""}`;
  const when = (ms) => formatTime(ms, ctx.zone, pattern);
  const legend = o.legend || { show: true, placement: "bottom" };
  const mode = o.tooltip ? o.tooltip.mode : "single";
  const canvas = el("canvas", { class: "timeline__canvas", role: "img", tabindex: 0, "aria-label": `${panel.title || "Untitled"}, ${history ? "status history" : "state timeline"}`, "aria-describedby": descId });
  const tip = el("div", { class: "timeline__tip", role: "tooltip", hidden: true, dataset: { testid: `timeline-${id}-tip` } });
  const main = el("div", { class: "timeline__main" }, canvas, tip);
  const items = legend.show && !model.over ? legendItems(model): []; // no legend before the cap check
  const pills = items.map(() => el("i"));
  const legendEl = el("ul", { class: "timeline__legend", role: "list", hidden: !items.length, dataset: { testid: `timeline-${id}-legend` } },
    ...items.map((it, k) => el("li", { class: "timeline__legend-item", role: "listitem" }, pills[k], el("span", { text: it.text }))));
  const note = el("p", { class: "timeline__note", role: "note", hidden: true, dataset: { testid: `timeline-${id}-note` } });
  const last = (r) => r.t.length - 1;
  const sentence = (r) => (r.t.length ? `${r.name}: ${r.t.length} states, last state ${model.states[r.s[last(r)]].text} since ${when(r.t[last(r)])}.` : `${r.name}: no states.`);
  const desc = el("p", { class: "visually-hidden", id: descId, dataset: { testid: `timeline-${id}-desc` }, text: [...model.rows.slice(0, 50).map(sentence), ...(model.rows.length > 50 ? [`${model.rows.length - 50} more rows.`] : [])].join(" ") });
  const root = el("div", { class: `timeline timeline--${legend.placement || "bottom"}`, dataset: { testid: `timeline-${id}` } }, main, legendEl, note, desc);
  host.append(root);
  let view = null; // the geometry of the last draw
  let hover = new Map(); // row -> box under the pointer
  let cursor = null; // the keyboard cursor: { row, j }
  let limited = false;
  let told = false;

  // Shows the note of a limit instead of the plot when `text` is not empty. It tells the frame when the state changes.
  const showLimit = (text) => {
    limited = text !== "";
    main.hidden = limited;
    legendEl.hidden = limited || !items.length;
    note.hidden = !limited;
    note.textContent = text;
    root.dataset.limit = limited ? "1" : "0";
    if (limited) delete host.dataset.timeline;
    if (limited !== told && typeof ctx.setState === "function") ctx.setState(limited ? "timeline-limit" : "ready");
    told = limited;
  };

  // Puts the legend in its place, checks the limits of the size (9.19c) and draws when no limit applies.
  // It runs at the start and after a change of the size, of the theme or of the fonts.
  function paint() {
    if (model.over) return showLimit(limitMessage(model, 0));
    main.hidden = false;
    legendEl.hidden = !items.length;
    // The legend at the right moves under the plot when the panel is too narrow for it and a plot (the rule of 9.19).
    root.removeAttribute("data-fallback");
    legendEl.style.setProperty("--legend-max", `${Math.floor(host.clientHeight * 0.35)}px`);
    if (legend.placement === "right" && items.length && host.clientWidth < legendEl.offsetWidth + 228) root.dataset.fallback = "";
    const W = main.clientWidth;
    const message = limitMessage(model, W);
    showLimit(message);
    if (!message) draw(W, main.clientHeight);
  }

  // Draws the grid, the time axis, the row names and the boxes in W x H CSS pixels, at the pixel ratio of the screen.
  // The colours come from the theme. A box has the colour of its state with the fill opacity, and a box under the pointer is lighter.
  // The draw writes the test hook `data-timeline` (9.29) with the rows, the segments and the texts.
  function draw(W, H) {
    const dpr = window.devicePixelRatio || 1;
    const dark = isDarkTheme();
    canvas.width = Math.max(1, Math.round(W * dpr));
    canvas.height = Math.max(1, Math.round(H * dpr));
    const c = canvas.getContext("2d");
    const family = getComputedStyle(host).fontFamily || "sans-serif";
    const colorOf = (css) => {
      c.fillStyle = "#000";
      c.fillStyle = canvasColor(host, css);
      return parseCss(c.fillStyle) || { r: 0, g: 0, b: 0, a: 1 };
    };
    const text = colorOf("var(--text)");
    const bg = colorOf("var(--panel)");
    const rgba = (p, a) => `rgba(${p.r}, ${p.g}, ${p.b}, ${a})`;
    c.font = `400 12px ${family}`;
    const measure = (s) => c.measureText(s).width;
    const nameW = Math.max(0, ...model.rows.map((r) => measure(r.name)));
    const L = gutterOf(nameW, o.show_labels !== false);
    const plotW = Math.max(1, W - PAD - L);
    const plotH = Math.max(1, H - PAD - AXIS);
    const [lo, hi0] = model.x;
    const hi = hi0 > lo ? hi0 : lo + 1;
    const xOf = (t) => L + ((t - lo) * plotW) / (hi - lo);
    const tAt = (px) => lo + ((px - L) * (hi - lo)) / plotW;
    const n = model.rows.length;
    const boxes = model.rows.map((_, k) => rowBox(k, n, plotH, o.row_height));
    const lw = o.line_width;
    const opacity = o.fill_opacity / 100;
    const cw = history ? cellWidth((model.step * plotW) / (hi - lo), o.col_width, lw) : 0;
    const D = (v) => Math.round(v * dpr);
    const parsed = model.states.map((st) => colorOf(dark ? st.dark : st.light));
    const ink = new Map();
    const inkOf = (sid) => {
      if (!ink.has(sid)) {
        const p = parsed[sid];
        const a = p.a < 1 ? p.a : opacity;
        const mix = (f, b) => Math.round(b * (1 - a) + f * a);
        ink.set(sid, canvasColor(host, cellInk(`rgb(${mix(p.r, bg.r)}, ${mix(p.g, bg.g)}, ${mix(p.b, bg.b)})`)));
      }
      return ink.get(sid);
    };
    // The grid and the time axis: the ticks and the labels of the engine.
    const clock = zoneClock(ctx.zone);
    const ticks = chartCore.timeTicks(lo, hi, plotW, measure, clock);
    const labels = chartCore.layoutXLabels(ticks, xOf, L, L + plotW, W, measure, measure);
    c.strokeStyle = rgba(text, 0.09);
    c.lineWidth = 1;
    c.beginPath();
    for (const t of ticks) {
      const x = xOf(t.v);
      if (x < L - 0.5 || x > L + plotW + 0.5) continue;
      c.moveTo(Math.round(x * dpr) + 0.5, D(PAD));
      c.lineTo(Math.round(x * dpr) + 0.5, D(PAD + plotH));
    }
    c.stroke();
    c.fillStyle = rgba(text, 1);
    c.textBaseline = "top";
    c.textAlign = "center";
    for (const t of labels) c.fillText(t.label, t.x * dpr, D(PAD + plotH + 9));
    c.textBaseline = "middle";
    c.textAlign = "right";
    if (o.show_labels !== false && (n > 1 ? (boxes[n - 1].y - boxes[0].y) / (n - 1) : plotH) >= MIN_LABEL_PITCH) model.rows.forEach((r, k) => c.fillText(r.name, D(L - 16), D(PAD + boxes[k].y + boxes[k].h / 2)));
    // The boxes, clipped to the plot.
    const report = { kind: panel.type, box: [L, PAD, plotW, plotH], rows: [], segments: [], states: model.states.map((st, sid) => [st.text, dark ? st.dark : st.light]), texts: [], cut: false };
    c.save();
    c.beginPath();
    c.rect(D(L), D(PAD), D(plotW), D(plotH));
    c.clip();
    c.font = `500 12px ${family}`;
    const lwD = lw * dpr;
    model.rows.forEach((r, k) => {
      const y = D(PAD + boxes[k].y);
      const h = Math.max(1, D(PAD + boxes[k].y + boxes[k].h) - y);
      report.rows.push({ name: r.name, y: boxes[k].y + PAD, h: boxes[k].h, n: r.t.length });
      for (let j = 0; j < r.t.length; j++) {
        let a;
        let w;
        if (history) {
          w = Math.max(1, D(cw));
          a = Math.round(xOf(r.t[j]) * dpr - w / 2);
        } else {
          if (r.e[j] <= lo || r.t[j] >= hi) continue;
          a = Math.round(xOf(r.t[j]) * dpr);
          w = Math.max(1, Math.round(xOf(r.e[j]) * dpr) - a);
        }
        const va = Math.max(a, D(L));
        const vb = Math.min(a + w, D(L + plotW));
        if (vb <= va) continue;
        const p = parsed[r.s[j]];
        c.fillStyle = rgba(p, p.a < 1 ? p.a : opacity);
        c.fillRect(a, y, w, h);
        if (lw > 0) {
          c.strokeStyle = rgba(p, 1);
          c.lineWidth = lwD;
          c.strokeRect(a + lwD / 2, y + lwD / 2, w - lwD, h - lwD);
        }
        if (hover.get(k) === j) {
          c.fillStyle = "rgba(255, 255, 255, 0.2)";
          c.fillRect(a, y, w, h);
        }
        const label = boxText(model.states[r.s[j]].text, (vb - va) / dpr, o.show_values);
        if (label) {
          const align = history ? "center" : o.align_values || "left";
          const tx = align === "center" ? (va + vb) / 2 : align === "right" ? vb - D(lw + 2) : va + D(lw + 2);
          c.fillStyle = inkOf(r.s[j]);
          c.textAlign = align;
          c.fillText(label, tx, y + h / 2);
          if (report.texts.length < MAX_REPORT) report.texts.push([label, round2(tx / dpr), round2((y + h / 2) / dpr), c.fillStyle]);
          else report.cut = true;
        }
        if (report.segments.length < MAX_REPORT) report.segments.push([k, round2(va / dpr), round2((vb - va) / dpr), r.s[j]]);
        else report.cut = true;
      }
    });
    c.restore();
    for (let k = 0; k < pills.length; k++) pills[k].style.background = dark ? items[k].dark : items[k].light;
    view = { W, H, L, plotW, plotH, boxes, xOf, tAt, cw };
    host.dataset.timeline = JSON.stringify(report);
  }

  // The box of row `k` under the horizontal place `px`: the index, or -1. A cell holds the half step on each side of its sample.
  const find = (k, px) => (px >= view.L && px <= view.L + view.plotW ? boxAt(model.rows[k], view.tAt(px) + (history ? model.step / 2 : 0)) : -1);

  // Hides the card and removes the hover layer.
  function hide() {
    tip.hidden = true;
    if (hover.size) {
      hover = new Map();
      if (view) draw(view.W, view.H);
    }
  }

  // The card and the hover layer for a place (the pointer, or a box that the keys chose).
  function point(px, py) {
    if (!view) return;
    const row = model.rows.findIndex((_, k) => py >= PAD + view.boxes[k].y && py <= PAD + view.boxes[k].y + view.boxes[k].h);
    const next = new Map();
    let entries = [];
    if (mode === "multi") {
      model.rows.forEach((r, k) => {
        const j = find(k, px);
        if (j >= 0) {
          next.set(k, j);
          if (r.tooltip) entries.push([r, j]);
        }
      });
    } else if (row >= 0) {
      const j = find(row, px);
      if (j >= 0) {
        next.set(row, j);
        entries = [[model.rows[row], j]];
      }
    }
    const same = next.size === hover.size && [...next].every(([k, j]) => hover.get(k) === j);
    hover = next;
    if (!same && view) draw(view.W, view.H);
    if (mode === "hidden" || !entries.length) {
      tip.hidden = true;
      return;
    }
    const [first, fj] = entries[0];
    const dark = isDarkTheme();
    const head = history ? when(first.t[fj]) : `${when(first.t[fj])} -\n${when(first.e[fj])}`;
    const rows = entries.map(([r, j]) => el("div", { class: "timeline__tip-row" }, el("span", { class: "timeline__tip-name", text: r.name }), el("b", { text: model.states[r.s[j]].text }), el("i", { style: { background: dark ? model.states[r.s[j]].dark : model.states[r.s[j]].light } })));
    if (mode === "single" && !history) rows.push(el("div", { class: "timeline__tip-row" }, el("span", { class: "timeline__tip-name", text: "Duration" }), el("b", { text: durationText(first.e[fj] - first.t[fj]) })));
    tip.replaceChildren(el("div", { class: "timeline__tip-head", text: head }), ...rows);
    tip.hidden = false;
    const w = tip.offsetWidth;
    const h = tip.offsetHeight;
    tip.style.left = `${Math.max(0, px + 10 + w > view.W ? px - 10 - w : px + 10)}px`;
    tip.style.top = `${Math.max(0, py + 10 + h > view.H ? py - 10 - h : py + 10)}px`;
  }

  canvas.addEventListener("pointermove", (e) => {
    const r = canvas.getBoundingClientRect();
    cursor = null;
    point(e.clientX - r.left, e.clientY - r.top);
  });
  canvas.addEventListener("pointerleave", () => {
    if (!cursor) hide();
  });
  canvas.addEventListener("blur", () => hide());
  // The keys of 9.26. The left and right arrows go to the previous or the next box of the row.
  // The up and down arrows go to the box at the same time in the previous or the next row with data. Escape hides the card.
  canvas.addEventListener("keydown", (e) => {
    if (!view) return;
    if (e.key === "Escape") {
      cursor = null;
      return hide();
    }
    const dx = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    const dy = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (!dx && !dy) return;
    e.preventDefault();
    const filled = model.rows.map((r, k) => (r.t.length ? k : -1)).filter((k) => k >= 0);
    if (!filled.length) return;
    let { row, j } = cursor || { row: filled[0], j: 0 };
    if (cursor && dy) {
      const start = model.rows[row].t[j];
      row = filled[Math.max(0, Math.min(filled.length - 1, filled.indexOf(row) + dy))];
      const after = model.rows[row].t.findIndex((t) => t > start);
      j = after < 0 ? model.rows[row].t.length - 1 : Math.max(0, after - 1);
    } else if (cursor) j = Math.max(0, Math.min(model.rows[row].t.length - 1, j + dx));
    cursor = { row, j };
    const r = model.rows[row];
    const left = Math.max(view.L, view.xOf(r.t[j]));
    const right = Math.min(view.L + view.plotW, history ? view.xOf(r.t[j]) + view.cw / 2 : view.xOf(r.e[j]));
    point(history ? view.xOf(r.t[j]) : (left + right) / 2, PAD + view.boxes[row].y + view.boxes[row].h / 2);
  });

  const onTheme = () => paint();
  const observer = new ResizeObserver(() => paint());
  observer.observe(host);
  window.addEventListener("resize", onTheme);
  document.addEventListener("hune-theme", onTheme);
  document.addEventListener("hune-shown", onTheme); // 10.2a, rule 17 (common.js, fitHeight)
  if (document.fonts) document.fonts.addEventListener("loadingdone", onTheme);
  paint();
  // The handle of the frame: `state` gives the state of a limit, and `destroy` removes the listeners and the nodes.
  return {
    get state() {
      return limited ? "timeline-limit" : undefined;
    },
    destroy() {
      observer.disconnect();
      window.removeEventListener("resize", onTheme);
      document.removeEventListener("hune-theme", onTheme);
      document.removeEventListener("hune-shown", onTheme);
      if (document.fonts) document.fonts.removeEventListener("loadingdone", onTheme);
      root.remove();
    },
  };
}
