// The time series panel (9.19, 4.28): the copied chart engine draws it.

import { selectRows } from "../frame.js";
import { zoneClock } from "../zone.js";
import { applyLegend, canvasColor, chartColor, createChart, fitHeight, hiddenIds, hideFlags, isDarkTheme, labelChart, legendFormatter, legendOptions, numbersOf, overlapFill, timeFractionDigits, tooltipFormat, yTickLabeller } from "./common.js";
import { attach } from "./annotate.js";
import { thresholdsHook } from "./thresholds.js";
import { autoYRange, axisPlan, boundedYAxis, lastFinite, seriesStyle, sharedRange, tickLabel } from "./series.js";
import { seriesPaint } from "./gradient.js";
import { BAR_ALIGN, centredEnds, dashOf, joinShortGaps, log2Axis } from "./series_opts.js";
import { parseCss } from "../colors.js";

const INTERPOLATION = { linear: "linear", smooth: "smooth", step_before: "stepBefore", step_after: "stepAfter" };
const KIND_OF_DRAW = { bars: "bar", points: "points", line: "line" };

/**
 * The engine hook `yAxis` of one scale (9.19): the automatic range of Grafana for a line, bars and a stack, with the ends of the scale
 * (`ends`: `min`, `max`, `softMin`, `softMax`, null when not set). The hook reads the data ends that the engine gives without the soft ends and
 * with the tops of a stack (`raw`, E30). A percent stack has the soft ends 0 and 100 %, and a log scale keeps the axis of the engine.
 * P14: `centred` (`axis_centered_zero`) centres a linear scale; `log2` (`log_base` 2) gives `log2Axis`, with `positive()` the lowest value above 0.
 */
export function scaleAxis({ ends, binary, tickText, plainTick, percent, log, centred, log2, positive, whole }) {
  const unitTick = tickText !== tickLabel ? tickText : null;
  const wrap = (ax, label) => (ax ? { min: ax.min, max: ax.max, step: ax.step, ticks: ax.values.map((v) => ({ v, label: label(v, ax.step) })) } : null);
  if (log && log2) return { hook: (a, b, h, raw) => log2Axis(raw && raw[0] > 0 ? raw[0] : positive(), raw ? raw[1] : b, ends, h, tickText, whole), tick: unitTick || plainTick };
  if (log) return { hook: undefined, tick: unitTick || undefined };
  const own = percent ? { min: null, max: null, softMin: 0, softMax: 100 } : ends;
  return {
    hook: (dataMin, dataMax, plotH, raw) => {
      const [lo, hi] = raw || [dataMin, dataMax];
      const [min, max] = autoYRange(...(centred && !percent ? centredEnds(lo, hi) : [lo, hi]), own);
      return wrap(boundedYAxis(min, max, min, max, plotH, binary && !percent), percent ? (v, step) => `${plainTick(v, step)}%` : tickText);
    },
    tick: percent ? undefined : unitTick || plainTick,
  };
}

export function render(host, ctx) {
  const { panel, names } = ctx;
  let { fields } = ctx;
  const o = panel.options;
  const timeIndex = fields.findIndex((f) => f.kind === "time");
  if (timeIndex < 0) throw new Error("The data has no time column.");
  let time = fields[timeIndex];
  // The engine needs an ascending x axis.
  const xs0 = time.values;
  let sorted = true;
  for (let i = 1; i < xs0.length; i++) if (!(xs0[i] >= xs0[i - 1])) { sorted = false; break; }
  let order = null;
  if (!sorted) {
    order = Array.from(xs0.keys()).filter((i) => !time.nulls[i]).sort((a, b) => xs0[a] - xs0[b] || a - b);
    fields = selectRows(fields, order);
    time = fields[timeIndex];
  }
  const xs = Float64Array.from(time.values);
  const series = [];
  const raws = {};
  const styles = [];
  const shared = sharedRange(fields);
  let seriesIndex = 0;
  fields.forEach((f, i) => {
    if (i === timeIndex || (f.kind !== "number" && f.kind !== "number_text")) return;
    const values = Float64Array.from(numbersOf(f));
    const last = lastFinite(values);
    const style = seriesStyle(f, o);
    const dash = dashOf(f.config.set); // P14: `line_dash_array`
    styles.push(style);
    series.push({
      id: String(i),
      label: names[i],
      ...chartColor(f, last, { index: seriesIndex, shared }),
      values,
      // P14: `span_nulls_ms` joins the short runs of nulls of a line (not of bars, as in Grafana).
      nulls: o.span_nulls ? new Uint8Array(values.length) : joinShortGaps(xs, f.nulls || null, style.draw === "bars" ? 0 : o.span_nulls_ms),
      group: "all",
      field: f,
      // The style of the series (9.19, E14): an override gives its own, else the option of the panel.
      width: style.lineWidth,
      fill: overlapFill(style.fillOpacity / 100),
      points: style.showPoints,
      pointSize: style.pointSize,
      dash: dash.dash || undefined,
      dashCap: dash.cap || undefined,
      negate: style.negate || undefined,
      // P10: the gradient of the fill and of the line (E43). A scheme spans the drawn ends of the scale (E30), which its hook keeps.
      paint: seriesPaint(o.gradient_mode, { config: f.config, dark: isDarkTheme, text: () => parseCss(canvasColor(host, "var(--text)")), raw: (L) => raws[L.scaleKey || "y"] }),
      ...hideFlags(f),
    });
    seriesIndex += 1;
  });
  const first = series[0] ? series[0].field : null;
  const stacked = o.stacking && o.stacking !== "none";
  // The kind of a series is its `draw`. When all series have one kind, the chart has it. Else the chart keeps the kind of the panel option,
  // a series of another kind says so (`type`), and the engine draws the series in order (E14).
  const kinds = styles.map((s) => KIND_OF_DRAW[s.draw] || "line");
  const panelKind = KIND_OF_DRAW[o.draw] || "line";
  const mixed = kinds.length > 0 && kinds.some((k) => k !== kinds[0]);
  const chartKind = kinds.length && !mixed ? kinds[0] : panelKind;
  const anyFill = styles.some((s) => s.fillOpacity > 0);
  let type = chartKind === "bar" ? "bar" : chartKind === "points" ? "points" : anyFill || stacked ? "area" : "line";
  if (stacked && chartKind === "points") type = "area";
  if (mixed) series.forEach((s, k) => { if (kinds[k] !== chartKind) s.type = kinds[k]; });
  const legend = legendOptions(o);
  const thresholdStyle = o.thresholds_style || "off";
  const log = o.y_scale === "log";
  // The scales (9.19, F16): the side of each series, and the range, the label and the axis of each scale.
  const plan = axisPlan(series.map((s) => s.field));
  const scales = {};
  series.forEach((s, k) => { if (plan.sides[k] === "right") scales[s.id] = "y2"; });
  // P14: the first series of a scale gives its centred zero, border and colour; the width is the widest of the scale (Grafana merges them so).
  const hookOf = (sc) => {
    const first = series[sc.first];
    const set = first.field.config.set || {};
    const key = sc.side === "right" ? "y2" : "y";
    const positive = () => sc.series.reduce((m, k) => series[k].values.reduce((a, v) => (v > 0 && v < a ? v : a), m), Infinity);
    const made = scaleAxis({
      ends: sc, binary: sc.unit === "bytes", tickText: yTickLabeller(first.field), plainTick: tickLabel, percent: o.stacking === "percent", log,
      centred: set.axis_centered_zero === true, log2: o.log_base === 2, positive, whole: first.field.config.decimals === 0,
    });
    const widths = sc.series.map((k) => series[k].field.config.set?.axis_width).filter(Number.isFinite);
    const scheme = o.gradient_mode === "scheme" && first.field.config.color?.mode === "thresholds";
    const look = {
      yWidth: widths.length ? Math.max(...widths) : undefined,
      yBorder: set.axis_border,
      yColor: set.axis_color === "series" ? (ctx, L) => (scheme ? first.paint("line", ctx, L, null, 1) : first.color) : undefined,
    };
    return { ...made, look, hook: made.hook && ((a, b, h, raw) => { raws[key] = raw; return made.hook(a, b, h, raw); }) };
  };
  const soft = (sc) => (sc.softMin !== null || sc.softMax !== null ? [sc.softMin, sc.softMax] : undefined);
  const left = plan.left ? hookOf(plan.left) : null;
  const rightScale = plan.right ? { sc: plan.right, ...hookOf(plan.right) } : null;
  const firstScale = plan.sides[0] === "right" ? plan.right : plan.left;
  const firstSides = plan.sides[0] || "left";
  // The x axis covers the requested time range, as in Grafana, not only the span of the data. A page without a range
  // (the preview of the editor gives 0 and 0) keeps the span of the data.
  const range = typeof ctx.linkContext === "function" ? ctx.linkContext() : null;
  const xDomain = range && Number.isFinite(range.from) && Number.isFinite(range.to) && range.to > range.from ? [range.from, range.to] : undefined;
  const styleReport = () => series.map((s, k) => ({
    name: s.label,
    draw: styles[k].draw,
    lineWidth: styles[k].lineWidth,
    fillOpacity: styles[k].fillOpacity,
    dash: styles[k].dash,
    showPoints: styles[k].showPoints,
    pointSize: styles[k].pointSize,
    negate: styles[k].negate,
    dashArray: series[k].dash || null,
  }));
  const axesReport = (info) => plan.scales.map((sc) => ({
    side: sc.side,
    min: sc.side === "left" ? info.yMin : info.y2Min,
    max: sc.side === "left" ? info.yMax : info.y2Max,
    label: sc.label,
    softMin: sc.softMin,
    softMax: sc.softMax,
    shown: sc.shown,
    series: sc.series.map((k) => series[k].label),
  }));
  // The hook of the thresholds draws on the scale of the first series (9.19).
  const thresholds = thresholdStyle !== "off" && first
    ? thresholdsHook(host, () => ({ thresholds: first.config.thresholds, style: thresholdStyle, bounds: [firstScale ? firstScale.min : null, firstScale ? firstScale.max : null] }))
    : null;
  let marksLayer = null; // the marks of the annotations (9.19d), made after the chart
  const options = {
    height: Math.max(60, host.clientHeight - 28),
    xKind: "time",
    zone: zoneClock(ctx.zone),
    xs,
    xDomain,
    series,
    type,
    stack: stacked,
    stackMode: o.stacking === "percent" ? "percent" : undefined,
    stackSign: "series", // E32: a series below 0 stacks down from 0, as in Grafana (9.19)
    interpolation: INTERPOLATION[o.interpolation] || "linear",
    connectGaps: true,
    lineWidth: o.line_width,
    fillOpacity: overlapFill(o.fill_opacity / 100),
    xFractionDigits: timeFractionDigits(xs),
    showPoints: o.show_points,
    pointSize: o.point_size,
    yScale: log ? "log" : undefined,
    legend: legend.legend,
    legendColumns: legend.legendColumns,
    tooltip: legend.tooltip,
    syncKey: ctx.syncKey || "",
    formatValue: tooltipFormat(first),
    legendFormat: legendFormatter(series, legend.legendColumns || []),
    legendSort: legend.legendSort,
    legendCalc: legend.legendCalc,
    tooltipSort: legend.tooltipSort,
    tooltipHideZeros: legend.tooltipHideZeros,
    hidden: hiddenIds(series),
    drawUnder: thresholds ? (c, L, env) => thresholds(c, firstSides === "right" && L.y2 ? L.y2 : L, env) : undefined,
    // The left scale (the placement of the series decides it, E13). A chart whose series all sit on the right has no left axis.
    yShown: plan.left ? plan.left.shown : !plan.right,
    yTitle: plan.left && plan.left.label ? plan.left.label : undefined,
    ySoft: plan.left ? soft(plan.left) : undefined,
    yBounds: plan.left && (plan.left.min !== null || plan.left.max !== null) ? [plan.left.min, plan.left.max] : undefined,
    yTickFormat: left ? left.tick : undefined,
    yAxis: left ? left.hook : undefined,
    ...(left ? left.look : {}),
    scales: rightScale ? scales : undefined,
    y2: rightScale
      ? {
        shown: rightScale.sc.shown,
        yTitle: rightScale.sc.label || undefined,
        ySoft: soft(rightScale.sc),
        yBounds: rightScale.sc.min !== null || rightScale.sc.max !== null ? [rightScale.sc.min, rightScale.sc.max] : undefined,
        yTickFormat: rightScale.tick,
        yAxis: rightScale.hook,
        yScale: log ? "log" : undefined,
        ...rightScale.look,
      }
      : undefined,
    gridScale: plan.grid === "right" ? "y2" : plan.grid === "left" || !plan.scales.length ? "y" : "none",
    drawOrder: mixed ? "series" : undefined,
    barFillAlpha: type === "bar" ? o.fill_opacity / 100 : undefined,
    barLineWidth: type === "bar" ? o.line_width : undefined,
    barAlign: BAR_ALIGN[o.bar_alignment], // P14, E44
    onDraw: (info) => {
      if (marksLayer) marksLayer.paint();
      host.dataset.axes = JSON.stringify(axesReport(info));
      host.dataset.seriesStyle = JSON.stringify(styleReport());
    },
    onZoom: (zoomRange, fromUser) => {
      if (fromUser && zoomRange && zoomRange[1] > zoomRange[0]) ctx.onZoom(Math.round(zoomRange[0]), Math.round(zoomRange[1]));
    },
  };
  const chart = createChart(host, options);
  host.dataset.chart = "timeseries";
  applyLegend(host, chart, o.legend);
  labelChart(chart, panel);
  const unfit = fitHeight(host, chart);
  // The marks draw on their own canvas after each draw of the chart (9.19d). A panel that no annotation applies to has no layer.
  if (typeof ctx.annotationsOf === "function" && ctx.annotationsOf().length) {
    marksLayer = attach(host, chart, ctx);
    marksLayer.paint();
  }
  return {
    annotate: () => marksLayer && marksLayer.update(),
    destroy() {
      if (marksLayer) marksLayer.destroy();
      unfit();
      chart.destroy();
    },
  };
}
