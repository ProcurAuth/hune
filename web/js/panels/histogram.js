// The histogram panel (9.19f): the chart engine draws the bars on a number axis. The buckets come from `bucket_model.js`.
import { buildHistogram, histDescription, histTicks } from "../bucket_model.js";
import { el, uniqueId } from "../ui.js";
import { applyLegend, chartColor, createChart, fitHeight, firstHeight, labelChart, legendFormatter, legendOptions, tooltipFormat, valueFormatter, yTickLabeller } from "./common.js";
import { histYAxis, lastFinite, sharedRange } from "./series.js";

// The hook `data-histogram` (9.29) lists 2,000 bars at most. Then it sets `cut`.
const MAX_REPORT = 2000;

/**
 * The y axis of a histogram for the series that show (`vis`, each with `counts`), `stacking` and the highest drawn top `high` (9.19f, section 3).
 * The axis uses the lowest drawn top: the lowest count above 0, or in a stack the top of the first series in each bucket with a count, also 0.
 * A percent stack shows shares of 1, and the engine draws it out of 100.
 */
export function histAxis(vis, stacking, high, plotH) {
  const stacked = stacking !== "none";
  const pct = stacking === "percent";
  let low = Infinity;
  for (let j = 0; vis.length && j < vis[0].counts.length; j++) {
    const total = vis.reduce((a, s) => a + Math.max(0, s.counts[j]), 0);
    for (const s of total > 0 ? (stacked ? vis.slice(0, 1) : vis) : []) if (stacked || s.counts[j] > 0) low = Math.min(low, Math.max(0, s.counts[j]) / (pct ? total : 1));
  }
  const ax = histYAxis(low, pct ? 1 : high, plotH);
  return ax && pct ? { min: 0, max: ax.max * 100, step: ax.step * 100, ticks: ax.ticks.map((t) => ({ v: t.v * 100, label: t.label })) } : ax;
}

// Draws a panel of type `histogram` in `host` (9.19f). A model error goes to the frame as an exception. Above a cap the panel shows the note and
// draws nothing: the handle tells the frame the state `bucket-limit`.
export function render(host, ctx) {
  const { panel, fields, names } = ctx;
  const o = panel.options;
  const m = buildHistogram({ fields, names, options: o });
  if (m.error) throw new Error(m.error);
  const id = panel.id;
  if (m.limit) {
    const note = el("p", { class: "bucket__note", role: "note", dataset: { testid: `histogram-${id}-note` }, text: m.limit });
    host.append(note);
    return { state: "bucket-limit", destroy: () => note.remove() };
  }
  const stacked = o.stacking !== "none";
  const shared = sharedRange(fields);
  // A series draws with the colour of its field. Its `field` for the engine is a field of counts: no unit, so that the legend and the card write counts.
  const series = m.series.map((s, k) => {
    const values = Float64Array.from(s.counts);
    const src = s.field || m.field;
    const counts = { kind: "number", values, nulls: null, config: { unit: "none", decimals: null } };
    return { id: String(k), label: s.name, ...chartColor(src, lastFinite(values), { index: k, shared }), values, nulls: null, group: "all", field: counts, legendHidden: !s.legend, tooltipHidden: !s.tooltip };
  });
  const legend = legendOptions(o);
  const edge = valueFormatter(m.field);
  const label = yTickLabeller(m.field);
  const title = (i) => `${edge(m.lows[i])} - ${edge(m.highs[i])}`;
  const bars = [];
  m.series.forEach((s, k) => s.counts.forEach((c, j) => c > 0 && bars.length <= MAX_REPORT && bars.push([k, j, c])));
  const report = { shape: m.shape, size: m.size, lo: m.lows[0], buckets: m.nb, series: m.series.map((s) => ({ name: s.name, max: s.max, at: s.at })), bars: bars.slice(0, MAX_REPORT), cut: bars.length > MAX_REPORT };
  const descId = uniqueId("histogram-desc");
  const desc = el("p", { class: "visually-hidden", id: descId, dataset: { testid: `histogram-${id}-desc` }, text: histDescription(m, edge) });
  host.append(desc);
  let chart = null;
  const chartHeight = firstHeight(host, legend.legend !== false);
  chart = createChart(host, {
    height: chartHeight,
    xKind: "number",
    xs: Float64Array.from(m.lows, (v, j) => (v + m.highs[j]) / 2),
    // The edges of the first and the last bucket are the ends of the plot. A bar is its slot (the bucket size) less 1 px, and the bars overlap (9.24).
    xDomain: [m.lows[0], m.highs[m.nb - 1]],
    xTicksFn: (lo, hi, plotW) => histTicks(m, plotW).map((v) => ({ v, label: label(v, m.size) })),
    xReadout: title,
    xGrid: false,
    series,
    type: "bar",
    stack: stacked,
    stackMode: o.stacking === "percent" ? "percent" : undefined,
    barSlot: m.size,
    barWidthRatio: 1,
    barMaxWidth: 1e5,
    barOverlap: !stacked,
    barDecimate: false,
    barFillAlpha: o.fill_opacity / 100,
    barLineWidth: o.line_width,
    barInset: true,
    // The y axis of Grafana (9.19f, section 3) on the series that show: a click on the legend hides a series.
    yAxis: (dataMin, dataMax, plotH) => histAxis(m.series.filter((s, k) => !(chart && chart.getHidden().has(String(k)))), o.stacking, dataMax, plotH),
    legend: legend.legend,
    legendColumns: legend.legendColumns,
    legendSort: legend.legendSort,
    legendCalc: legend.legendCalc,
    legendFormat: legendFormatter(series, legend.legendColumns || []),
    tooltip: legend.tooltip,
    tooltipSort: legend.tooltipSort,
    tooltipHideZeros: legend.tooltipHideZeros,
    formatValue: tooltipFormat(series[0].field),
    zoomable: false,
    onDraw: () => {
      host.dataset.histogram = JSON.stringify(report);
    },
  });
  applyLegend(host, chart, o.legend);
  labelChart(chart, panel);
  chart.root.dataset.testid = `histogram-${id}`;
  const overlay = chart.root.querySelector(".chartCore__overlay");
  if (overlay) overlay.setAttribute("aria-describedby", descId);
  const unfit = fitHeight(host, chart, 60, chartHeight);
  return {
    destroy() {
      unfit();
      chart.destroy();
      desc.remove();
      delete host.dataset.histogram;
    },
  };
}
