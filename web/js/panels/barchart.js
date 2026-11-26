// The bar chart panel (9.19, 4.30): the copied chart engine draws it.

import { grafanaValue } from "../frame.js";
import { formatTime } from "../units.js";
import { applyLegend, canvasColor, chartColor, createChart, explicitBounds, fitHeight, hiddenIds, hideFlags, isDarkTheme, joinPartsOf, labelChart, legendFormatter, legendOptions, numbersOf, tooltipFormat, yTickLabeller } from "./common.js";
import { thresholdsHook } from "./thresholds.js";
import { renderHorizontalBars } from "./barchart_h.js";
import { autoYRange, axisSettings, boundedYAxis, lastFinite, sharedRange } from "./series.js";
import { seriesPaint } from "./gradient.js";
import { centredEnds } from "./series_opts.js";
import { parseCss } from "../colors.js";

/**
 * The engine hook `yAxis` of the value axis of a bar chart (9.19, rule 15), as Grafana 12.4.2 draws it: the automatic range of the drawn ends (the
 * tops of a stack, E30) with the ends of the field. A side with no end has the soft end 0. `scale` is 100 for a percent stack: the engine draws shares
 * out of 100, and the labels show shares of 1 in the unit of the field. P14: `centred` centres the axis; `seen(raw)` keeps the drawn ends (P10).
 */
export function barValueAxis({ min: lo, max: hi, softMin, softMax }, scale, tickText, binary, centred = false, seen = () => {}) {
  const end = (v) => (v === null ? null : v * scale);
  const ends = { min: end(lo), max: end(hi), softMin: end(softMin ?? (lo === null ? 0 : null)), softMax: end(softMax ?? (hi === null ? 0 : null)) };
  return (dataMin, dataMax, plotH, raw) => {
    seen(raw);
    const [a, b] = raw || [dataMin, dataMax];
    const [min, max] = autoYRange(...(centred && scale === 1 ? centredEnds(a, b) : [a, b]), ends);
    const ax = boundedYAxis(min, max, min, max, plotH, binary && scale === 1);
    return ax ? { min: ax.min, max: ax.max, step: ax.step, ticks: ax.values.map((v) => ({ v, label: tickText(v / scale, ax.step / scale) })) } : null;
  };
}

/** The orientation `auto` (4.30, 9.19), as Grafana 12.4.2: horizontal bars when the plot is narrower than it is high. */
export const autoHorizontal = (w, h) => w < h;

export function render(host, ctx) {
  const o = ctx.panel.options;
  if (o.orientation !== "auto") return draw(host, ctx, o.orientation === "horizontal");
  // A resize that crosses the rule draws the chart again in the other orientation.
  const pick = () => autoHorizontal(host.clientWidth, host.clientHeight);
  let flat = pick();
  let view = draw(host, ctx, flat);
  const observer = new ResizeObserver(() => {
    if (!host.clientWidth || pick() === flat) return;
    flat = !flat;
    view.destroy();
    view = draw(host, ctx, flat);
  });
  observer.observe(host);
  return {
    destroy() {
      observer.disconnect();
      view.destroy();
    },
  };
}

function draw(host, ctx, horizontal) {
  const { panel, fields, names, zone } = ctx;
  const o = panel.options;
  let xIndex = -1;
  if (o.x_field !== null && o.x_field !== undefined) xIndex = names.indexOf(o.x_field);
  if (xIndex < 0) xIndex = fields.findIndex((f) => f.kind === "text" || f.kind === "time");
  if (xIndex < 0) throw new Error("The data has no text or time column for the x axis.");
  const xf = fields[xIndex];
  const n = xf.values.length;
  const isTime = xf.kind === "time";
  const categories = [];
  for (let i = 0; i < n; i++) {
    const v = grafanaValue(xf, i);
    categories.push(isTime ? (v === null ? "" : formatTime(v, zone)) : v === null ? "" : String(v));
  }
  const xs = Float64Array.from({ length: n }, (_, i) => i);
  const series = [];
  const shared = sharedRange(fields);
  let seriesIndex = 0;
  let raw = null;
  fields.forEach((f, i) => {
    if (i === xIndex || (f.kind !== "number" && f.kind !== "number_text")) return;
    const values = Float64Array.from(numbersOf(f));
    series.push({
      id: String(i),
      label: names[i],
      ...chartColor(f, lastFinite(values), { index: seriesIndex, shared }),
      values,
      nulls: f.nulls || null,
      group: "all",
      field: f,
      // P10: the gradient of the bars (E43); a scheme spans the drawn ends of the value axis.
      paint: seriesPaint(o.gradient_mode, { config: f.config, dark: isDarkTheme, text: () => parseCss(canvasColor(host, "var(--text)")), raw: () => raw }),
      ...hideFlags(f),
    });
    seriesIndex += 1;
  });
  const first = series[0] ? series[0].field : null;
  const legend = legendOptions(o);
  // The value axis is one axis. Its title and its soft ends are those of the first field that sets them (9.19, F16). `axis_placement` has no effect.
  const settings = series.map((s) => axisSettings(s.field));
  const firstWith = (key) => settings.map((c) => c[key]).find((v) => v !== null && v !== undefined) ?? null;
  // `axis_grid` (4.18) of the first field that sets it: `false` removes the grid lines of the value axis.
  const grid = series.map((s) => s.field.config.set?.axis_grid).find((v) => typeof v === "boolean") ?? null;
  // P14: the first field gives the centred zero, the border and the colour of the value axis; the width is the widest that a field sets.
  const set0 = (series.length && series[0].field.config.set) || {};
  const widths = series.map((s) => s.field.config.set?.axis_width).filter(Number.isFinite);
  const look = { centred: set0.axis_centered_zero === true, border: set0.axis_border, width: widths.length ? Math.max(...widths) : undefined, color: set0.axis_color === "series" ? () => series[0].color : undefined };
  const axis = { label: firstWith("label"), softMin: firstWith("softMin"), softMax: firstWith("softMax"), grid, look };
  // The chart engine has no horizontal bars: Hune code draws them (9.24).
  if (horizontal) {
    return renderHorizontalBars(host, { categories, series, options: o, axis, gradient: o.gradient_mode, legend: o.legend || { show: false, placement: "bottom", mode: "list", values: [] } }, panel);
  }
  const thresholdStyle = o.thresholds_style || "off";
  const stacked = o.stacking && o.stacking !== "none";
  const gw = (o.group_width || 70) / 100;
  const bw = (o.bar_width || 97) / 100;
  // Bars in one group (a stack is one group): the group takes `group_width` of its band and each bar takes `bar_width` of its share of the group.
  // A single bar takes `bar_width` of the band (Grafana).
  const perGroup = stacked ? 1 : series.length;
  const share = perGroup > 1 ? gw : bw;
  const barUnits = n > 1 ? (share * (n - 1)) / (n - share) : share;
  const showValues = o.show_values !== "never" && !stacked;
  const unit = first ? first.config.unit : "none";
  const tickText = yTickLabeller(first);
  const [lo, hi] = first ? explicitBounds(first) : [null, null];
  const options = {
    height: Math.max(60, host.clientHeight - 28),
    xKind: "category",
    xs,
    categories,
    series,
    type: "bar",
    stack: stacked,
    stackMode: o.stacking === "percent" ? "percent" : undefined,
    stackSign: "series", // E32, E33: the stack of Grafana (9.19)
    // Grafana puts the first bar against the left edge of the plot and the last against the right edge. A bar (or a group) takes `r` of the
    // band `plot / n`: the domain reaches half a bar beyond the first and the last category, and a bar is `b` wide in band units.
    xDomain: n > 1 ? [-barUnits / 2, n - 1 + barUnits / 2] : undefined,
    barWidthRatio: n > 1 ? barUnits : share,
    barInnerRatio: perGroup > 1 ? bw : undefined,
    xGrid: false,
    // The rotation of the x labels in degrees (4.31, E25). A horizontal bar chart returned above and ignores it.
    xTickRotation: o.x_tick_rotation,
    barMaxWidth: 100000,
    // `fill_opacity` and `line_width` (4.30). The line stays inside the bar on its four sides (E34), as in Grafana.
    barFillAlpha: (o.fill_opacity ?? 80) / 100,
    barLineWidth: o.line_width ?? 1,
    barInset: "box",
    gridScale: grid === false ? "none" : undefined,
    yAxis: barValueAxis({ min: lo, max: hi, softMin: axis.softMin, softMax: axis.softMax }, o.stacking === "percent" ? 100 : 1, tickText, unit === "bytes", look.centred, (r) => { raw = r; }),
    yWidth: look.width,
    yBorder: look.border,
    yColor: look.color,
    yTitle: axis.label || undefined,
    ySoft: axis.softMin !== null || axis.softMax !== null ? [axis.softMin, axis.softMax] : undefined,
    onDraw: (info) => {
      host.dataset.axes = JSON.stringify([{ side: "left", min: info.yMin, max: info.yMax, label: axis.label, softMin: axis.softMin, softMax: axis.softMax, shown: true, series: series.map((s) => s.label) }]);
    },
    barLabel: showValues ? valueLabelHook(host, first, o.show_values === "always") : undefined,
    legend: legend.legend,
    legendColumns: legend.legendColumns,
    tooltip: legend.tooltip,
    zoomable: false,
    formatValue: tooltipFormat(first),
    legendFormat: legendFormatter(series, legend.legendColumns || []),
    legendSort: legend.legendSort,
    legendCalc: legend.legendCalc,
    tooltipSort: legend.tooltipSort,
    tooltipHideZeros: legend.tooltipHideZeros,
    hidden: hiddenIds(series),
    drawUnder: thresholdStyle !== "off" && first ? thresholdsHook(host, () => ({ thresholds: first.config.thresholds, style: thresholdStyle, bounds: explicitBounds(first) })) : undefined,
  };
  const chart = createChart(host, options);
  applyLegend(host, chart, o.legend);
  labelChart(chart, panel);
  const unfit = fitHeight(host, chart);
  return {
    destroy() {
      unfit();
      chart.destroy();
    },
  };
}

// The value of a bar above its end (below it when the value is negative), in the text colour. The size follows the width of the bar: a
// third of its width for each character, from 10 px to 28 px. In the mode `auto` a value that would need less than 10 px is left out.
function valueLabelHook(host, field, always) {
  const text = (v) => (field ? joinPartsOf(field, v) : String(v));
  return (ctx, b) => {
    const label = text(b.value);
    if (!label) return;
    const raw = Math.floor(b.w / (Math.max(1, label.length) * 0.6));
    if (raw < 10 && !always) return;
    const size = Math.max(10, Math.min(28, raw));
    const cs = getComputedStyle(host);
    ctx.save();
    ctx.font = `400 ${size}px ${cs.fontFamily}`;
    ctx.fillStyle = canvasColor(host, "var(--text)");
    ctx.textAlign = "center";
    ctx.textBaseline = "alphabetic";
    ctx.fillText(label, b.x + b.w / 2, b.negative ? b.y + 6 + size * 0.75 : b.y - 6);
    ctx.restore();
  };
}
