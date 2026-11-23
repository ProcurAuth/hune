// Browser cases B60 and B61 (16.10): the y axes and the style of a series (milestone F, wave 2: F16, F17).
// B60 right y axis, axis label, soft and hard ends, grid, hidden axis, bar chart. B61 `draw`, `line_dash`, `line_width`, `transform`.
// All SQL is synthetic (`numbers(...)`).
import { doc, panel, section } from "../lib/docs.mjs";
import { reveal } from "../lib/h.mjs";

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const g = (x, y, w, h) => ({ x, y, w, h });
const plotSel = (id) => `[data-testid="panel-${id}"] .panel__plot`;

// Twenty points, one a minute from the start of the seed range.
const TWO = [
  "SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(10 + number) AS lat, toFloat64(5000 + number * 100) AS rate",
  "FROM numbers(20)",
];

/** The JSON of `data-axes` or `data-series-style` of the plot element of a panel. */
async function hook(p, id, name) {
  const text = await p.eval(`(() => { const e = document.querySelector(${JSON.stringify(plotSel(id))}); return e ? e.getAttribute(${JSON.stringify(name)}) : null; })()`);
  return text ? JSON.parse(text) : null;
}

/** The plot box of the chart `{ left, top, w, h, width }` in CSS px: `data-plot` of the engine and the width of the plot element. */
async function plotBox(p, id) {
  return p.eval(`(() => {
    const root = document.querySelector('[data-testid="panel-${id}"] .chartCore');
    const [left, top, w, h] = root.dataset.plot.split(" ").map(Number);
    return { left, top, w, h, width: Math.floor(root.querySelector(".chartCore__plot").getBoundingClientRect().width) };
  })()`);
}

/** The texts drawn on the canvas of a panel (needs `recordCanvasText` before `goto`): `[{ text, x, y, align, baseline }]`. */
async function canvasTexts(p, id) {
  return p.eval(`(() => { const c = document.querySelector('[data-testid="panel-${id}"] canvas.chartCore__canvas'); return c ? window.__canvasTexts(c) : []; })()`);
}

/** True when the canvas has a horizontal line at the row `y` (the pixel differs from the pixel 3 rows below it) at the column `x`. */
async function lineAt(p, id, x, y) {
  return p.eval(`(() => {
    const c = document.querySelector('[data-testid="panel-${id}"] canvas.chartCore__canvas');
    const px = (xx, yy) => { const d = c.getContext("2d").getImageData(xx, yy, 1, 1).data; return d[0] + d[1] + d[2]; };
    const row = Math.round(${y});
    return Math.abs(px(${x}, row) - px(${x}, row + 3)) > 6;
  })()`);
}

export const cases = [
  {
    id: "B60",
    name: "y axes: left and right scale, label, soft and hard ends, grid on the left scale, hidden axis, all axes hidden (no top margin), bar chart",
    async run(t) {
      const ov = (name, set) => ({ match: { by: "name", value: name }, set });
      const mk = (id, title, grid, overrides, options = {}) => panel(id, "timeseries", title, TWO, { grid, options: { line_width: 2, ...options }, fields: { defaults: {}, overrides } });
      const bars = ["SELECT concat('c', toString(number)) AS cat, toFloat64(number * 5) AS v", "FROM numbers(8)"];
      const d = doc("y-axes", "Y axes", [
        section(null, [
          mk(1, "Two units", g(0, 0, 12, 8), [ov("lat", { unit: "ms", axis_label: "Latency", soft_min: 0, soft_max: 200 }), ov("rate", { unit: "reqps", axis_label: "Rate" })]),
          mk(2, "Left", g(12, 0, 12, 8), [ov("lat", { unit: "ms" }), ov("rate", { unit: "reqps", axis_placement: "left" })]),
          mk(3, "Hidden", g(0, 8, 12, 8), [ov("lat", { unit: "ms" }), ov("rate", { unit: "reqps", axis_placement: "hidden" })]),
          mk(4, "Hard wins", g(12, 8, 12, 8), [ov("lat", { unit: "ms", min: 50, soft_min: 0, soft_max: 400 }), ov("rate", { unit: "reqps", axis_placement: "right", axis_label: "Rate" })]),
          mk(5, "Same unit", g(0, 16, 12, 8), [ov("lat", { unit: "ms" }), ov("rate", { unit: "ms" })]),
          panel(6, "barchart", "Bars", bars, { grid: g(12, 16, 12, 8), options: { x_field: null }, fields: { defaults: { unit: "none", axis_label: "Count", soft_min: 0, soft_max: 1000 }, overrides: [] } }),
          mk(7, "All hidden", g(0, 24, 12, 8), [ov("lat", { unit: "ms", axis_placement: "hidden" }), ov("rate", { unit: "reqps", axis_placement: "hidden" })]),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.page();
      await p.recordCanvasText();
      await p.goto(t.dash("y-axes"));
      await p.idle();
      await reveal(p, [1, 2, 3, 4, 5, 6, 7]);

      // 1. Two units: the left and the right scale, in this order, with their labels and series.
      const a1 = await hook(p, 1, "data-axes");
      t.eq(a1.map((s) => s.side), ["left", "right"], "panel 1: two scales, left first");
      t.eq(a1.map((s) => s.series), [["lat"], ["rate"]], "panel 1: the series of each scale");
      t.eq(a1.map((s) => s.label), ["Latency", "Rate"], "panel 1: the title of each scale");
      t.eq(a1.map((s) => s.shown), [true, true], "panel 1: both axes show");
      t.eq([a1[0].softMin, a1[0].softMax], [0, 200], "panel 1: the soft ends of the left scale");
      t.assert(a1[0].min <= 0 && a1[0].max >= 200, `panel 1: the soft ends move the range: ${a1[0].min} to ${a1[0].max}`);
      t.assert(a1[1].min >= 0 && a1[1].max > 6900, `panel 1: the right scale has the range of its own series: ${a1[1].min} to ${a1[1].max}`);
      // The labels of each axis are drawn: the left labels end 5 px before the plot, the right labels start 5 px after it.
      const box1 = await plotBox(p, 1);
      const texts1 = await canvasTexts(p, 1);
      const leftLabels = texts1.filter((x) => x.align === "right");
      const rightLabels = texts1.filter((x) => x.align === "left");
      t.assert(leftLabels.length >= 3 && rightLabels.length >= 3, `panel 1: labels on both axes (${leftLabels.length}, ${rightLabels.length})`);
      t.assert(leftLabels.every((x) => Math.abs(x.x - (box1.left - 5)) < 0.01), "panel 1: the left labels end 5 px before the plot");
      t.assert(rightLabels.every((x) => Math.abs(x.x - (box1.left + box1.w + 5)) < 0.01), "panel 1: the right labels start 5 px after the plot");
      // The right gutter: the widest label of the right scale plus 9 px, plus 28 px for the title, rounded up.
      const widest = await p.eval(`(() => {
        const c = document.createElement("canvas").getContext("2d");
        const root = document.querySelector('[data-testid="panel-1"] .chartCore');
        c.font = "400 12px " + getComputedStyle(root).fontFamily;
        return ${JSON.stringify(rightLabels.map((x) => x.text))}.reduce((m, s) => Math.max(m, c.measureText(s).width), 0);
      })()`);
      const gutter = box1.width - box1.left - box1.w;
      t.assert(Math.abs(gutter - Math.ceil(widest + 9 + 28)) <= 3, `panel 1: right gutter ${gutter} is the widest label ${widest.toFixed(1)} + 9 + 28`);
      // The titles turn by -90 and 90 degrees: the engine draws them with `fillText` after a rotation, in the middle of the plot height.
      const titles = texts1.filter((x) => x.text === "Latency" || x.text === "Rate");
      t.eq(titles.map((x) => x.text).sort(), ["Latency", "Rate"], "panel 1: the two titles are drawn");
      t.assert(titles.every((x) => x.x === 0 && x.y === 0), "panel 1: a title is drawn at the origin of its rotation");
      // The grid follows the left scale: a line at each left tick, none at a tick of the right scale only.
      const probe = [box1.left + 6, box1.left + Math.floor(box1.w / 2) + 7, box1.left + box1.w - 6];
      const near = (y, list) => list.some((o) => Math.abs(o.y - y) < 4);
      for (const tick of leftLabels) {
        let seen = 0;
        for (const x of probe) if (await lineAt(p, 1, x, tick.y)) seen += 1;
        t.assert(seen >= 2, `panel 1: a grid line at the left tick "${tick.text}" (seen at ${seen} of 3 columns)`);
      }
      for (const tick of rightLabels.filter((x) => !near(x.y, leftLabels))) {
        let seen = 0;
        for (const x of probe) if (await lineAt(p, 1, x, tick.y)) seen += 1;
        t.assert(seen <= 1, `panel 1: no grid line at the right tick "${tick.text}" (seen at ${seen} of 3 columns)`);
      }

      // 2. `left`: the series goes to the left scale. One scale, no right axis, the gutter of the right side is the margin of 8 px.
      const a2 = await hook(p, 2, "data-axes");
      t.eq(a2.map((s) => s.side), ["left"], "panel 2: one scale");
      t.eq(a2[0].series, ["lat", "rate"], "panel 2: both series on the left scale");
      const box2 = await plotBox(p, 2);
      t.eq(box2.width - box2.left - box2.w, 8, "panel 2: no right axis, a margin of 8 px");

      // 3. `hidden`: the scale exists and has no axis. It takes no room: the margin is 8 px and the labels are not drawn.
      const a3 = await hook(p, 3, "data-axes");
      t.eq(a3.map((s) => [s.side, s.shown]), [["left", true], ["right", false]], "panel 3: the right scale does not show");
      const box3 = await plotBox(p, 3);
      t.eq(box3.width - box3.left - box3.w, 8, "panel 3: a hidden axis takes no room");
      t.eq((await canvasTexts(p, 3)).filter((x) => x.align === "left").length, 0, "panel 3: no label of the hidden axis");

      // 7. Every y axis hidden: no axis on either side, so the plot has no margin at the top (E27, 9.24), as in Grafana 12.4.2. The bottom band stays.
      const a7 = await hook(p, 7, "data-axes");
      t.assert(a7.length > 0 && a7.every((s) => s.shown === false), `panel 7: no scale shows its axis: ${JSON.stringify(a7)}`);
      const box7 = await plotBox(p, 7);
      t.eq([box3.top, box7.top], [8, 0], "the top margin: 8 px with an axis (panel 3), none with no axis (panel 7)");
      t.eq(box7.top + box7.h, box3.top + box3.h, "panel 7: the plot ends where the plot of panel 3 ends");
      t.eq((await canvasTexts(p, 7)).filter((x) => x.align === "left" || x.align === "right").length, 0, "panel 7: no label of a y axis");

      // 4. A hard `min` wins over `soft_min`; `soft_max` still moves the end of the data.
      const a4 = await hook(p, 4, "data-axes");
      t.eq(a4[0].min, 50, "panel 4: the hard min is the lower end");
      t.assert(a4[0].max >= 400, `panel 4: the soft max moves the upper end: ${a4[0].max}`);
      t.eq([a4[0].softMin, a4[0].softMax], [0, 400], "panel 4: the soft ends are reported");

      // 5. The same unit: one scale for two series.
      const a5 = await hook(p, 5, "data-axes");
      t.eq(a5.map((s) => s.series), [["lat", "rate"]], "panel 5: one scale");

      // 6. A bar chart has one value axis; `axis_label` and the soft ends apply to it, `axis_placement` has no effect.
      const a6 = await hook(p, 6, "data-axes");
      t.eq(a6.length, 1, "panel 6: one value axis");
      t.eq(a6[0].label, "Count", "panel 6: the title of the value axis");
      t.assert(a6[0].max >= 1000, `panel 6: the soft max moves the end of the axis: ${a6[0].max}`);
      t.assert(a6[0].min <= 0, `panel 6: the axis starts at 0: ${a6[0].min}`);
      const texts6 = await canvasTexts(p, 6);
      t.assert(texts6.some((x) => x.text === "Count"), "panel 6: the title is drawn");
      t.eq(texts6.filter((x) => x.align === "left").length, 0, "panel 6: the value axis stays at the left");

      // The legend of a table legend ends the name of a series of the right scale with " (right y-axis)". A list legend puts them at the right end.
      const legend1 = await p.eval(`[...document.querySelectorAll('[data-testid="panel-1"] .chartCore__legendItem')].map((e) => ({ text: e.textContent, scale: e.getAttribute("data-scale"), left: e.getBoundingClientRect().left }))`);
      t.eq(legend1.map((x) => x.text), ["lat", "rate"], "panel 1: the list legend has no text mark");
      t.assert(legend1[1].scale === "y2" && legend1[1].left > legend1[0].left + 100, `panel 1: the item of the right scale sits at the right end: ${JSON.stringify(legend1)}`);
    },
  },

  {
    id: "B61",
    name: "series style: an override sets bars, a dashed line, a dotted line, negative Y; the legend value stays positive",
    async run(t) {
      const ov = (name, set) => ({ match: { by: "name", value: name }, set });
      const red = { color: { mode: "fixed", fixed: "#ff0000" } };
      const blue = { color: { mode: "fixed", fixed: "#0000ff" } };
      // A flat series: the line is one row of pixels, so that the dash can be read along it.
      const flat = (v) => ["SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(" + v + ") AS v", "FROM numbers(20)"];
      const style = (id, title, grid, sql, overrides, options = {}) => panel(id, "timeseries", title, sql, { grid, options: { line_width: 3, ...options }, fields: { defaults: { min: 0, max: 100 }, overrides } });
      const two = ["SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(30 + number) AS a, toFloat64(5 + number) AS b", "FROM numbers(20)"];
      const d = doc("series-style", "Series style", [
        section(null, [
          style(1, "Bars and a dashed line", g(0, 0, 12, 8), two, [ov("a", { ...red, draw: "line", line_dash: "dash", line_width: 3 }), ov("b", { ...blue, draw: "bars", fill_opacity: 60, line_width: 1 })]),
          style(2, "Dash", g(12, 0, 12, 8), flat(50), [ov("v", { ...red, line_dash: "dash", line_width: 3, show_points: "never" })]),
          style(3, "Dot", g(0, 8, 12, 8), flat(50), [ov("v", { ...red, line_dash: "dot", line_width: 3, show_points: "never" })]),
          style(4, "Solid", g(12, 8, 12, 8), flat(50), [ov("v", { ...red, line_dash: "solid", line_width: 3, show_points: "never" })]),
          panel(5, "timeseries", "Negative Y", ["SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(20 + number * 2) AS v", "FROM numbers(20)"], {
            grid: g(0, 16, 12, 8),
            options: { line_width: 3, legend: { show: true, placement: "bottom", mode: "table", values: ["min", "max", "last"] }, tooltip: { mode: "multi" } },
            fields: { defaults: { unit: "percent", min: 0, max: 100 }, overrides: [ov("v", { ...red, transform: "negative_y" })] },
          }),
          panel(6, "timeseries", "Points", flat(50), { grid: g(12, 16, 12, 8), options: { line_width: 3 }, fields: { defaults: { min: 0, max: 100 }, overrides: [ov("v", { ...red, draw: "points", point_size: 8 })] } }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.page();
      await p.recordCanvasText();
      await p.goto(t.dash("series-style"));
      await p.idle();
      await reveal(p, [1, 2, 3, 4, 5, 6]);

      const rows = (id, [r, g2, b], y, tol = 60) => p.eval(`(() => {
        const c = document.querySelector('[data-testid="panel-${id}"] canvas.chartCore__canvas');
        const w = c.width;
        const d = c.getContext("2d").getImageData(0, ${y}, w, 1).data;
        const on = (i) => d[i * 4 + 3] > 120 && Math.abs(d[i * 4] - ${r}) <= ${tol} && Math.abs(d[i * 4 + 1] - ${g2}) <= ${tol} && Math.abs(d[i * 4 + 2] - ${b}) <= ${tol};
        const runs = [];
        for (let i = 0; i < w; i++) {
          if (!on(i)) continue;
          if (runs.length && runs[runs.length - 1].end === i) runs[runs.length - 1].end = i + 1;
          else runs.push({ start: i, end: i + 1 });
        }
        return runs.map((x) => [x.start, x.end - x.start]);
      })()`);
      const yOfFlat = async (id) => {
        // The series is flat at 50 on the axis 0 to 100: the line is in the middle of the plot.
        const box = await p.eval(`(() => { const r = document.querySelector('[data-testid="panel-${id}"] .chartCore'); return r.dataset.plot.split(" ").map(Number); })()`);
        const axis = await hook(p, id, "data-axes");
        const [, top, , h] = box;
        return Math.round(top + h - ((50 - axis[0].min) / (axis[0].max - axis[0].min)) * h);
      };
      const RED = [255, 0, 0];

      // The styles are in `data-series-style`.
      const s1 = await hook(p, 1, "data-series-style");
      t.eq(s1.map((s) => [s.name, s.draw, s.lineWidth, s.dash, s.negate]), [["a", "line", 3, "dash", false], ["b", "bars", 1, "solid", false]], "panel 1: the style of each series");
      t.eq(s1[1].fillOpacity, 60, "panel 1: the fill of the bars");
      // The bars and the line of one chart: both are drawn.
      const blueBars = await p.eval(`(() => {
        const c = document.querySelector('[data-testid="panel-1"] canvas.chartCore__canvas');
        const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
        let n = 0;
        for (let i = 0; i < d.length; i += 4) if (d[i + 2] > d[i] + 40 && d[i + 3] > 100) n++;
        return n;
      })()`);
      t.assert(blueBars > 300, `panel 1: the bars are drawn (${blueBars} blue pixels)`);

      // A dash: 10 px of line and 10 px of gap (a butt cap). A dot: a round dot of the width of the line every 10 px. A solid line: one run.
      const dash = await rows(2, RED, await yOfFlat(2));
      t.assert(dash.length >= 5, `panel 2: a dashed line has runs (${dash.length})`);
      const inner = dash.slice(1, -1);
      t.assert(inner.every(([, len]) => len >= 8 && len <= 12), `panel 2: each dash is about 10 px long: ${JSON.stringify(inner)}`);
      t.assert(inner.slice(1).every(([start], k) => Math.abs(start - inner[k][0] - 20) <= 2), `panel 2: the period is 20 px: ${JSON.stringify(inner)}`);
      const dot = await rows(3, RED, await yOfFlat(3));
      t.assert(dot.length >= 10, `panel 3: a dotted line has many runs (${dot.length})`);
      const dots = dot.slice(1, -1);
      t.assert(dots.every(([, len]) => len >= 2 && len <= 5), `panel 3: each dot is about 3 px wide: ${JSON.stringify(dots)}`);
      t.assert(dots.slice(1).every(([start], k) => Math.abs(start - dots[k][0] - 10) <= 2), `panel 3: the dots are 10 px apart: ${JSON.stringify(dots)}`);
      const solid = await rows(4, RED, await yOfFlat(4));
      t.eq(solid.length, 1, "panel 4: a solid line is one run");
      const st2 = await hook(p, 2, "data-series-style");
      t.eq([st2[0].dash, st2[0].lineWidth], ["dash", 3], "panel 2: data-series-style");
      t.eq((await hook(p, 3, "data-series-style"))[0].dash, "dot", "panel 3: data-series-style");

      // draw = points: dots and no line.
      const pts = await rows(6, RED, await yOfFlat(6));
      t.assert(pts.length >= 10 && pts.every(([, len]) => len <= 10), `panel 6: points, no line: ${JSON.stringify(pts.slice(0, 6))}`);
      t.eq((await hook(p, 6, "data-series-style"))[0].draw, "points", "panel 6: data-series-style");

      // negative_y: the series draws below the zero line, the axis shows the drawn values, the legend and the tooltip show the data.
      const st5 = await hook(p, 5, "data-series-style");
      t.eq(st5[0].negate, true, "panel 5: data-series-style negate");
      const a5 = await hook(p, 5, "data-axes");
      t.eq([a5[0].min, a5[0].max], [-100, 0], "panel 5: the range follows the drawn values (min 0 and max 100 swap and change sign)");
      const texts5 = await canvasTexts(p, 5);
      const axis5 = texts5.filter((x) => x.align === "right").map((x) => x.text);
      t.assert(axis5.includes("-100%") && axis5.includes("0%"), `panel 5: the axis shows -100% and 0% (U+002D): ${axis5}`);
      t.assert(!axis5.some((x) => /−/.test(x)), "panel 5: the sign is U+002D");
      const zero = await p.eval(`(() => {
        const [, top, , h] = document.querySelector('[data-testid="panel-5"] .chartCore').dataset.plot.split(" ").map(Number);
        return Math.round(top + h * (${a5[0].max} / (${a5[0].max} - ${a5[0].min})));
      })()`);
      const redBelow = await p.eval(`(() => {
        const c = document.querySelector('[data-testid="panel-5"] canvas.chartCore__canvas');
        const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
        let above = 0, below = 0;
        for (let y = 0; y < c.height; y++) for (let x = 0; x < c.width; x++) {
          const i = (y * c.width + x) * 4;
          if (d[i + 3] > 150 && d[i] > 200 && d[i + 1] < 60 && d[i + 2] < 60) { if (y < ${zero} - 2) above++; else below++; }
        }
        return { above, below };
      })()`);
      t.assert(redBelow.below > 30 && redBelow.above === 0, `panel 5: the line is below the zero line: ${JSON.stringify(redBelow)}`);
      const legend5 = await p.eval(`[...document.querySelectorAll('[data-testid="panel-5"] .chartCore__legendTable tbody tr')].map((r) => [...r.cells].map((c) => c.textContent))`);
      t.assert(legend5.length === 1 && legend5[0].slice(1).every((x) => !x.includes("-")), `panel 5: the legend shows the data, not the drawn values: ${JSON.stringify(legend5)}`);
      await p.hover('[data-testid="panel-5"] .chartCore__overlay', 0.5, 0.5);
      await p.waitFor(`(() => { const e = document.querySelector('[data-testid="panel-5"] .chartCore__tooltip'); return e && !e.hidden; })()`, { what: "tooltip", timeout: 3000 });
      const tip = await p.eval(`[...document.querySelectorAll('[data-testid="panel-5"] .chartCore__tooltip .chartCore__tipRow b')].map((e) => e.textContent)`);
      t.assert(tip.length === 1 && !tip[0].includes("-"), `panel 5: the tooltip shows the data: ${JSON.stringify(tip)}`);
    },
  },
];
