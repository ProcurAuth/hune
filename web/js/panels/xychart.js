// The xy chart panel (9.19, 4.32): the copied chart engine draws it (xKind "number").

import { selectRows } from "../frame.js";
import { applyLegend, chartColor, createChart, fitHeight, firstHeight, hiddenIds, hideFlags, labelChart, legendFormatter, legendOptions, numbersOf, tooltipFormat, valueFormatter, yTickLabeller } from "./common.js";
import { autoYAxis, autoYRange, lastFinite, numberTicks, sharedRange } from "./series.js";

const isNum = (f) => f.kind === "number" || f.kind === "number_text";

export function render(host, ctx) {
  const { panel, names } = ctx;
  let { fields } = ctx;
  const o = panel.options;
  let xIndex = o.x_field !== null && o.x_field !== undefined ? names.indexOf(o.x_field) : -1;
  if (xIndex < 0) xIndex = fields.findIndex(isNum);
  if (xIndex < 0 || !isNum(fields[xIndex])) throw new Error("The data has no number column for the x axis.");
  // Stable sort of the rows by x, up (9.19). A NULL or NaN x goes last and is not drawn.
  const xv = numbersOf(fields[xIndex]);
  const order = Array.from(xv.keys()).filter((i) => Number.isFinite(xv[i])).sort((a, b) => xv[a] - xv[b] || a - b);
  fields = selectRows(fields, order);
  const xs = Float64Array.from(numbersOf(fields[xIndex]));
  const wanted = o.y_fields && o.y_fields.length ? o.y_fields.map((name) => names.indexOf(name)).filter((i) => i >= 0) : null;
  const series = [];
  const shared = sharedRange(fields);
  let seriesIndex = 0;
  fields.forEach((f, i) => {
    if (i === xIndex || !isNum(f)) return;
    if (wanted && !wanted.includes(i)) return;
    const values = Float64Array.from(numbersOf(f));
    const showLine = o.draw === "lines" || o.draw === "points_and_lines";
    series.push({
      id: String(i),
      label: names[i],
      ...chartColor(f, lastFinite(values), { index: seriesIndex, shared }),
      values,
      nulls: f.nulls || null,
      type: o.draw === "points" ? "points" : "line",
      radius: o.point_size / 2,
      pickable: o.draw !== "lines",
      showLine,
      field: f,
      ...hideFlags(f),
    });
    seriesIndex += 1;
  });
  const first = series[0] ? series[0].field : null;
  const legend = legendOptions(o);
  let xLo = Infinity, xHi = -Infinity;
  for (const x of xs) {
    if (x < xLo) xLo = x;
    if (x > xHi) xHi = x;
  }
  const xRange = xHi >= xLo ? autoYRange(xLo, xHi) : undefined;
  const xLabel = yTickLabeller(fields[xIndex]);
  const yLabel = yTickLabeller(first);
  const options = {
    height: firstHeight(host, legend.legend !== false),
    xKind: "number",
    xs,
    series,
    type: o.draw === "points" ? "points" : "line",
    lineWidth: o.line_width,
    showPoints: o.draw === "lines" ? "never" : o.draw === "points_and_lines" ? "always" : "auto",
    pointSize: o.point_size,
    legend: legend.legend,
    legendColumns: legend.legendColumns,
    tooltip: legend.tooltip,
    formatValue: tooltipFormat(first),
    legendFormat: legendFormatter(series, legend.legendColumns || []),
    legendSort: legend.legendSort,
    legendCalc: legend.legendCalc,
    tooltipSort: legend.tooltipSort,
    tooltipHideZeros: legend.tooltipHideZeros,
    hidden: hiddenIds(series),
    // The two axes pad their range as the y axis of a time series does (Grafana: no forced zero, 10 % of room), and carry the names of the fields.
    xDomain: xRange,
    xTicksFn: (lo, hi, plotW, measure) => numberTicks(lo, hi, plotW, xLabel, measure),
    xTitle: names[xIndex],
    yTitle: series.length === 1 ? series[0].label : "",
    yAxis: (dataMin, dataMax, plotH) => {
      // The y axis of the xy chart keeps its ticks 30 px apart (the default of the plot library: Grafana does not use its height rule there).
      const ax = autoYAxis(dataMin, dataMax, plotH, 30);
      return ax ? { min: ax.min, max: ax.max, step: ax.step, ticks: ax.values.map((v) => ({ v, label: yLabel(v, ax.step) })) } : null;
    },
    xReadout: (i) => valueFormatter(fields[xIndex])(xs[i]),
  };
  const chart = createChart(host, options);
  applyLegend(host, chart, o.legend);
  labelChart(chart, panel);
  const unfit = fitHeight(host, chart, 60, options.height);
  return {
    destroy() {
      unfit();
      chart.destroy();
    },
  };
}
