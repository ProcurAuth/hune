// The horizontal bar chart (9.19, 4.30): Hune code draws it in the DOM, because the copied chart engine has no
// horizontal bars. One row for each category (each row of the data), one bar for each series in a row, a value axis
// at the bottom and the value of each bar at its end. The text of a category, a series or a value goes in the DOM
// only with textContent.

import { el } from "../ui.js";
import { CALC_COLUMNS, LEGEND_HEADS, canvasColor, chartLabel, explicitBounds, isDarkTheme, legendCalc, legendFormatter, reduceValues, yTickLabeller } from "./common.js";
import { cssGradient, partGradient } from "./gradient.js";
import { niceStepAtLeast } from "./series.js";
import { markCss, parseCss } from "../colors.js";
import { styleParts, thresholdShapes } from "./thresholds.js";
import { formatNumber, joinParts } from "../units.js";

const MIN_ROW_FOR_VALUE = 12; // px: a row that is lower than this shows no value in the mode `auto`
const TICK_GAP = 60; // px between two ticks of the value axis (9.19)
const LABEL_MAX_PX = 180;
const LABEL_PAD_PX = 22; // 6 px at the left of a category name, 16 px between the name and the bars
const VALUE_MAX_PX = 28; // the largest value label (Grafana scales it with the bar)
const VALUE_MIN_PX = 10; // as the value labels of the vertical bars (9.19)
const PERCENT_AXIS = Object.freeze({ min: 0, max: 100, step: 20, values: [0, 20, 40, 60, 80, 100] });

/** The text of a number for the field `field`: its unit and decimals. */
function valueText(field, v) {
  return Number.isFinite(v) ? joinParts(formatNumber(v, field.config.unit, field.config.decimals)) : "";
}

const LEGEND_MAX_SHARE = 0.35; // the legend keeps at most 35 % of the height of the panel (9.19)
const MIN_PLOT_WIDTH = 220; // a legend at the right keeps the plot at least this wide (9.19)
const LEGEND_GAP = 8;
const collator = new Intl.Collator("en", { sensitivity: "base" });

/**
 * The legend of the horizontal bars (pure), from the shared object `legend` of 4.26 as the vertical form reads it (9.19): the legend shows when
 * `show` is not false and a series has an entry (`hide_from.legend` removes an entry). `mode` `table` has one column for each value of `values`,
 * in document order. `values` = [] gives the column "Name" only. Returns `{ show, mode, columns, entries }`.
 */
export function hbarLegendModel(series, legend) {
  const l = legend || { show: false };
  const entries = series.filter((s) => !s.legendHidden);
  const table = l.mode === "table";
  const columns = table ? (l.values || []).filter((v, k, all) => CALC_COLUMNS.has(v) && all.indexOf(v) === k) : [];
  return { show: l.show !== false && entries.length > 0, mode: table ? "table" : "list", columns, entries };
}

/**
 * The rows of the legend table in the order of `sort` (`{ by, desc }`, or null for the series order), as 9.19 says: the key is the name or the
 * calculation `by` of the series. A row with no key goes last in both directions, and equal keys keep the series order.
 */
export function sortLegend(entries, sort) {
  if (!sort || !sort.by) return entries.slice();
  const key = (s) => {
    if (sort.by === "name") return s.label;
    const v = ["total", "min", "max", "mean", "last"].includes(sort.by) ? reduceValues(s.values)[sort.by] : legendCalc(s, sort.by);
    return Number.isFinite(v) ? v : null;
  };
  const keyed = entries.map((s, i) => ({ s, i, k: key(s) }));
  const sign = sort.desc ? -1 : 1;
  keyed.sort((a, b) => {
    if (a.k === null || b.k === null) return a.k === b.k ? a.i - b.i : a.k === null ? 1 : -1;
    const c = sort.by === "name" ? collator.compare(a.k, b.k) : a.k - b.k;
    return c ? sign * c : a.i - b.i;
  });
  return keyed.map((r) => r.s);
}

/**
 * The value axis of the bars (pure): the ticks from 0 (or from the lowest negative value) to the highest value, with
 * 10 % of room above, as Grafana draws it. The room ends the axis where it ends: the last tick is the last multiple of
 * the step inside the axis. At least `TICK_GAP` px between two ticks. Returns `{ min, max, step, values }`.
 */
export function barAxis(data, widthPx, binary = false, tickGap = TICK_GAP) {
  let lo = 0;
  let hi = 0;
  for (const v of data) {
    if (!Number.isFinite(v)) continue;
    if (v < lo) lo = v;
    if (v > hi) hi = v;
  }
  if (lo === 0 && hi === 0) return { min: 0, max: 1, step: 0.2, values: [0, 0.2, 0.4, 0.6, 0.8, 1] };
  const room = (hi - lo) * 0.1;
  const min = lo < 0 ? lo - room : 0;
  const max = hi > 0 ? hi + room : 0;
  const slots = Math.max(1, Math.floor(Math.max(widthPx, tickGap) / tickGap));
  const step = niceStepAtLeast((max - min) / slots, binary);
  const values = [];
  for (let k = Math.ceil(min / step - 1e-9); k * step <= max + step * 1e-9 && values.length < 200; k++) {
    const v = k * step;
    values.push(Math.abs(v) < step * 1e-9 ? 0 : Number(v.toPrecision(12)));
  }
  return { min, max, step, values };
}

/**
 * Draw the bars. `model`: `{ categories: string[], series: [{ id, label, color, values, field }], options, legend }`
 * where `legend` is the shared object of 4.26 (`show`, `placement`, `mode`, `values`, `sort_by`, `sort_desc`) and `options` has `stacking`, `show_values`, `bar_width`, `group_width`,
 * `tooltip.mode`. Returns `{ destroy }`.
 */
export function renderHorizontalBars(host, model, panel) {
  const { categories, series, options: o } = model;
  const axisModel = model.axis || { label: null, softMin: null, softMax: null };
  const legend = model.legend || { show: false, placement: "bottom", mode: "list", values: [] };
  const stacked = o.stacking === "normal" || o.stacking === "percent";
  const percent = o.stacking === "percent";
  const n = categories.length;
  const first = series[0] ? series[0].field : null;
  const unit = first ? first.config.unit : "none";
  const thresholdStyle = o.thresholds_style || "off";
  const tickText = yTickLabeller(first);

  // The extent of the bars: a stack adds up, a group does not.
  const totals = [];
  const extent = [];
  for (let i = 0; i < n; i++) {
    let pos = 0;
    let neg = 0;
    for (const s of series) {
      const v = s.values[i];
      if (!Number.isFinite(v)) continue;
      if (v >= 0) pos += stacked ? v : 0;
      else neg += stacked ? v : 0;
      if (!stacked) extent.push(v);
    }
    totals.push(pos - neg);
    if (stacked) extent.push(pos, neg);
  }
  // The soft ends of the value axis move the end of the data, before the room of 10 % (9.19, F16).
  if (!percent) for (const soft of [axisModel.softMin, axisModel.softMax]) if (Number.isFinite(soft)) extent.push(soft);
  // P14: `axis_centered_zero` puts 0 in the middle of the axis. `drawn` is the range of a scheme gradient (P10).
  const look = axisModel.look || {};
  const drawn = extent.reduce((r, v) => (Number.isFinite(v) ? [Math.min(r[0], v), Math.max(r[1], v)] : r), [Infinity, -Infinity]);
  const m = Math.max(Math.abs(drawn[0]), Math.abs(drawn[1])) || 80;
  if (look.centred && !percent && m < Infinity) extent.push(-m, m);
  const bounds = percent ? [0, 100] : extent;

  const root = el("div", { class: "hbar", role: "img", "aria-label": chartLabel(panel), dataset: { testid: `hbar-${panel.id}`, legendPlacement: legend.show === false ? "bottom" : legend.placement || "bottom" } });
  // `fill_opacity` and `line_width` (4.30): the CSS of a bar reads them. The line stays inside the bar, as in Grafana.
  root.style.setProperty("--fa", `${o.fill_opacity ?? 80}%`);
  root.style.setProperty("--lw", `${o.line_width ?? 1}px`);
  const plot = el("div", { class: "hbar__plot" });
  // The copy of rows (9.8e): row_copy.js gives one text line for each row, the name and the value with one space.
  const rowsEl = el("div", { class: "hbar__rows", dataset: { copyRows: "1" } });
  const axisEl = el("div", { class: "hbar__axis" });
  // P14: the band of the value axis is `axis_width` px high; a border is a line and ticks of 4 px in the colour of the axis, and the labels go 4 px down.
  const axisColor = look.color ? look.color() : null;
  if (axisColor) axisEl.style.color = axisColor;
  if (Number.isFinite(look.width)) axisEl.style.flex = `0 0 ${look.width}px`;
  if (look.border) axisEl.style.borderTop = `1px solid ${axisColor || "var(--text)"}`;
  const gridEl = el("div", { class: "hbar__grid" });
  const body = el("div", { class: "hbar__body" }, gridEl, rowsEl);
  plot.append(body, axisEl);
  // The title of the value axis (9.19, F16): 12 px, under the ticks, centred under the bars.
  if (axisModel.label) plot.append(el("div", { class: "hbar__axisTitle", text: axisModel.label }));
  const tip = el("div", { class: "hbar__tip", role: "presentation", hidden: true });
  root.append(plot);
  const legendModel = hbarLegendModel(series, model.legend);
  const legendEl = legendModel.show ? buildLegend(legendModel, series, model.legend) : null;
  if (legendEl) root.append(legendEl);
  root.append(tip);
  host.append(root);

  // The legend table and the list (9.19). The head of a column sorts the rows in three steps: up, down, series order.
  function buildLegend(m, list, shared) {
    const box = el("div", { class: "hbar__legend", role: "group", "aria-label": "Series", dataset: { mode: m.mode } });
    const item = (s) => el("span", { class: "hbar__legendItem", title: s.label }, el("i", { style: { "background-color": s.color } }), el("span", { text: s.label }));
    if (m.mode === "list") {
      box.append(...m.entries.map(item));
      return box;
    }
    const fmt = legendFormatter(m.entries, m.columns);
    const cell = (s, name) => {
      const v = ["total", "min", "max", "mean", "last"].includes(name) ? reduceValues(s.values)[name] : legendCalc(s, name);
      return Number.isFinite(v) ? fmt(v, name, s) : "\u2014";
    };
    let sort = shared && shared.sort_by ? { by: shared.sort_by, desc: shared.sort_desc === true } : null;
    const tbody = el("tbody");
    const headRow = el("tr");
    const wrap = el("div", { class: "chartCore__legendTableWrap" }, el("table", { class: "chartCore__legendTable", "aria-label": "Values of the series" }, el("thead", {}, headRow), tbody));
    const heads = [["name", "Name"], ...m.columns.map((c) => [c, LEGEND_HEADS[c]])];
    const paint = () => {
      headRow.replaceChildren(...heads.map(([name, text]) => {
        const state = sort && sort.by === name ? (sort.desc ? "descending" : "ascending") : "none";
        const button = el("button", { type: "button", class: "chartCore__legendSort", title: `Sort by ${text}`, dataset: { column: name }, text });
        button.addEventListener("click", () => {
          sort = !sort || sort.by !== name ? { by: name, desc: false } : !sort.desc ? { by: name, desc: true } : null;
          paint();
        });
        return el("th", { scope: "col", "aria-sort": state }, button);
      }));
      tbody.replaceChildren(...sortLegend(m.entries, sort).map((s) => el("tr", {}, el("th", { scope: "row" }, item(s)), ...m.columns.map((c) => el("td", { text: cell(s, c) })))));
    };
    paint();
    box.append(wrap);
    return box;
  }

  // The legend keeps at most 35 % of the height of the panel and scrolls beyond it (CSS reads --legend-max). A legend at the right keeps the
  // plot 220 px wide at least: under that, the legend goes under the plot (data-legend-fallback), as in the vertical form.
  function fitLegend() {
    if (!legendEl) return;
    legendEl.style.setProperty("--legend-max", `${Math.floor(root.clientHeight * LEGEND_MAX_SHARE)}px`);
    if (legend.placement !== "right") return;
    if (root.hasAttribute("data-legend-fallback")) root.removeAttribute("data-legend-fallback");
    if (root.clientWidth < legendEl.offsetWidth + MIN_PLOT_WIDTH + LEGEND_GAP) root.setAttribute("data-legend-fallback", "");
  }

  // The text of the page at 12 px measures the category names (a canvas: no layout read for each name).
  const measureCtx = document.createElement("canvas").getContext("2d");
  const nameWidth = () => {
    const cs = getComputedStyle(root);
    measureCtx.font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
    let w = 0;
    for (const c of categories) w = Math.max(w, measureCtx.measureText(c).width);
    return w;
  };
  const rows = [];
  const bw = (o.bar_width || 97) / 100;
  const gw = (o.group_width || 70) / 100;
  for (let i = 0; i < n; i++) {
    const name = el("span", { class: "hbar__name", text: categories[i], title: categories[i] });
    const track = el("div", { class: "hbar__track" });
    const bars = [];
    series.forEach((s, k) => {
      const bar = el("div", { class: "hbar__bar" });
      bar.style.setProperty("--c", s.color);
      const label = el("span", { class: "hbar__value" });
      track.append(bar, label);
      bars.push({ s, k, bar, label });
    });
    const row = el("div", { class: "hbar__row", dataset: { index: i, copyRow: "1" } }, name, track);
    rowsEl.append(row);
    rows.push({ i, row, name, track, bars });
  }

  // The CSS colour of a document colour at an alpha, unless the colour has its own alpha.
  const shade = (color, alpha) => {
    const css = markCss(color);
    const p = parseCss(color);
    return p && p.a < 1 ? css : `color-mix(in srgb, ${css} ${Math.round(alpha * 100)}%, transparent)`;
  };

  // P10: the fill of a bar is one CSS gradient over the whole track, as Grafana paints one gradient over the plot. The line keeps the plain colour.
  const paintBar = (bar, s, left, trackW, pos) => {
    const c = parseCss(canvasColor(host, s.color));
    const g = c && partGradient(model.gradient, "fill", {
      c, alpha: (o.fill_opacity ?? 80) / 100, dark: isDarkTheme(), config: s.field.config, text: parseCss(canvasColor(host, "var(--text)")),
      ends: [trackW, 0], scale: [axis.min, axis.max], raw: drawn, pos: (v) => Math.round((pos(v) / 100) * trackW),
    });
    const css = cssGradient(g, trackW);
    if (!css) return;
    bar.style.background = `${css.startsWith("linear") ? css : `linear-gradient(${css}, ${css})`} border-box ${(-(left / 100) * trackW).toFixed(2)}px 0 / ${trackW}px 100%`;
  };

  let axis = null;
  function layout() {
    const width = Math.max(1, plot.clientWidth);
    // The height of the rows does not depend on the width of the labels: read it before the write of `--hbar-label`.
    const rowH = rowsEl.clientHeight / Math.max(1, n);
    const labelW = Math.min(LABEL_MAX_PX, Math.ceil(nameWidth()) + LABEL_PAD_PX);
    root.style.setProperty("--hbar-label", `${labelW}px`);
    const trackW = Math.max(40, width - labelW - 8);
    // Two ticks are TICK_GAP px apart or more (9.19). The width of a label of the highest value is less than a label of a fine step.
    axis = percent ? PERCENT_AXIS : barAxis(bounds, trackW, unit === "bytes");
    const span = axis.max - axis.min || 1;
    const pos = (v) => ((v - axis.min) / span) * 100;
    // Bars in one group: the group takes `group_width` of the row, and each bar takes `bar_width` of its share of the group, centred in it.
    // A single bar, or a stack, takes `bar_width` of the row (Grafana).
    const grouped = series.length > 1 && !stacked;
    const thickness = grouped ? gw : bw;
    const slot = grouped ? thickness / series.length : thickness;
    const barShare = grouped ? slot * bw : thickness;
    const placed = []; // the labels to place, after all the writes of the loop (one layout, not one for each label)
    for (const r of rows) {
      let acc = 0;
      let accNeg = 0;
      for (const { s, k, bar, label } of r.bars) {
        const v = s.values[r.i];
        const ok = Number.isFinite(v);
        let from = 0;
        let to = ok ? v : 0;
        if (stacked && ok) {
          const scale = percent ? (totals[r.i] > 0 ? 100 / totals[r.i] : 0) : 1;
          const sv = v * scale;
          if (sv >= 0) {
            from = acc;
            acc += sv;
            to = acc;
          } else {
            from = accNeg;
            accNeg += sv;
            to = accNeg;
          }
        }
        const left = Math.min(pos(from), pos(to));
        const right = Math.max(pos(from), pos(to));
        bar.hidden = !ok || right - left <= 0;
        bar.style.left = `${left.toFixed(3)}%`;
        bar.style.width = `${(right - left).toFixed(3)}%`;
        const t = barShare;
        const top = grouped ? (1 - thickness) / 2 + k * slot + (slot - barShare) / 2 : (1 - thickness) / 2;
        bar.style.top = `${(top * 100).toFixed(3)}%`;
        bar.style.height = `${(t * 100).toFixed(3)}%`;
        if (model.gradient && model.gradient !== "none" && !bar.hidden) paintBar(bar, s, left, trackW, pos);
        // The value: at the end of the bar, or inside its end when the bar reaches the right edge.
        const barPx = (rowH * t) ;
        const show = o.show_values === "always" || (o.show_values !== "never" && barPx >= MIN_ROW_FOR_VALUE);
        label.hidden = !show || !ok;
        if (show && ok) {
          label.textContent = percent ? `${Math.round((v / (totals[r.i] || 1)) * 100)}%` : valueText(s.field, v);
          label.style.top = bar.style.top;
          label.style.height = bar.style.height;
          // The size follows the thickness of the bar (2/3, from 10 px to 28 px). The value sits 8 px after the bar, or inside
          // its end when it would leave the track.
          const size = Math.max(VALUE_MIN_PX, Math.min(VALUE_MAX_PX, Math.round(barPx * (2 / 3))));
          const gap = Math.max(3, Math.round(size * 0.4));
          label.style.fontSize = `${size}px`;
          label.style.marginLeft = `${gap}px`;
          label.style.left = `${right.toFixed(3)}%`;
          placed.push({ label, right, gap });
        }
      }
    }
    const widths = placed.map((p) => p.label.offsetWidth);
    placed.forEach(({ label, right, gap }, i) => {
      const inside = (right / 100) * trackW + gap + widths[i] > trackW;
      label.classList.toggle("is-inside", inside);
      if (inside) label.style.marginLeft = `${-gap}px`;
    });
    // Gridlines and the ticks of the axis.
    const lines = [];
    const ticks = [];
    const fmt = tickText;
    for (const v of axis.values) {
      const p = pos(v);
      // `axis_grid` = `false` (4.18): no grid line; the ticks stay.
      if (axisModel.grid !== false) lines.push(el("i", { class: "hbar__line", style: { left: `${p.toFixed(3)}%` } }));
      ticks.push(el("span", { class: "hbar__tick", style: { left: `${p.toFixed(3)}%`, ...(look.border ? { top: "11px" } : {}) }, text: percent ? `${v}%` : fmt(v, axis.step) }));
      if (look.border) ticks.push(el("i", { style: { position: "absolute", left: `${p.toFixed(3)}%`, top: 0, width: "1px", height: "4px", background: axisColor || "var(--text)" } }));
    }
    // The thresholds: bands and lines on the value axis, under the bars (thresholds_style, 9.19). They stand upright, as the value axis is horizontal.
    const shapes = first && thresholdStyle !== "off" ? thresholdShapes(first.config.thresholds, thresholdStyle, explicitBounds(first), [axis.min, axis.max]) : { lines: [], areas: [] };
    const bands = shapes.areas.map((a) => el("i", { class: "hbar__band", style: { left: `${pos(a.from).toFixed(3)}%`, width: `${(pos(a.to) - pos(a.from)).toFixed(3)}%`, background: shade(a.color, 0.15) } }));
    const marks = shapes.lines.map((l) => {
      const e = el("i", { class: `hbar__thr${styleParts(thresholdStyle).dashed ? " is-dashed" : ""}`, style: { left: `${pos(l.value).toFixed(3)}%` } });
      e.style.setProperty("--thr", shade(l.color, 0.7));
      return e;
    });
    gridEl.replaceChildren(...bands, ...lines, ...marks);
    axisEl.replaceChildren(...ticks);
    host.dataset.axes = JSON.stringify([{ side: "bottom", min: axis.min, max: axis.max, label: axisModel.label, softMin: axisModel.softMin, softMax: axisModel.softMax, shown: true, series: series.map((s) => s.label) }]);
  }

  // The tooltip: the row under the pointer, with the value of each series.
  const mode = o.tooltip ? o.tooltip.mode : "single";
  function onMove(ev) {
    if (mode === "hidden") return;
    const target = ev.target instanceof Element ? ev.target.closest(".hbar__row") : null;
    if (!target) {
      tip.hidden = true;
      return;
    }
    const r = rows[Number(target.dataset.index)];
    const hit = ev.target.closest(".hbar__bar");
    const list = mode === "multi" || !hit ? series : [series[r.bars.findIndex((b) => b.bar === hit)]].filter(Boolean);
    const head = el("div", { class: "hbar__tipHead", text: categories[r.i] });
    const lines = list.map((s) => el("div", { class: "hbar__tipRow" }, el("i", { style: { "background-color": s.color } }), el("span", { text: s.label }), el("b", { text: valueText(s.field, s.values[r.i]) || "—" })));
    tip.replaceChildren(head, ...lines);
    tip.hidden = false;
    const box = root.getBoundingClientRect();
    const x = ev.clientX - box.left + 14;
    const y = ev.clientY - box.top + 14;
    tip.style.left = `${Math.min(x, Math.max(0, box.width - tip.offsetWidth - 4))}px`;
    tip.style.top = `${Math.min(y, Math.max(0, box.height - tip.offsetHeight - 4))}px`;
  }
  const onLeave = () => {
    tip.hidden = true;
  };
  root.addEventListener("pointermove", onMove);
  root.addEventListener("pointerleave", onLeave);

  layout();
  fitLegend();
  const observer = new ResizeObserver(layout);
  observer.observe(plot);
  const legendObserver = new ResizeObserver(fitLegend);
  legendObserver.observe(root);
  // The names were measured before the page font arrived? Measure them again once.
  const onFonts = () => layout();
  if (document.fonts && document.fonts.addEventListener) document.fonts.addEventListener("loadingdone", onFonts);
  return {
    destroy() {
      if (document.fonts && document.fonts.removeEventListener) document.fonts.removeEventListener("loadingdone", onFonts);
      observer.disconnect();
      legendObserver.disconnect();
      root.removeEventListener("pointermove", onMove);
      root.removeEventListener("pointerleave", onLeave);
      root.remove();
    },
  };
}
