// Browser cases B36 to B42 (16.10): the features of wave 1 of milestone F.
// B36 stat `justify`, B37 hidden series, B38 thresholds in the plot, B39 zone of the dashboard, B40 table features,
// B41 CSV download, B42 titles with variables. All SQL is synthetic (`numbers(...)` or the seed table).
import { readFileSync, readdirSync, renameSync } from "node:fs";
import { join } from "node:path";
import { doc, panel, section } from "../lib/docs.mjs";
import { SEED_FROM, SEED_TO } from "../lib/env.mjs";
import { axisTexts, colourPixels, reveal, sleep, thresholdSnapshot, tooltipAt, waitPanelsReady } from "../lib/h.mjs";

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const g = (x, y, w, h) => ({ x, y, w, h });
const reduce = (over = {}) => ({ calc: "last_not_null", fields: null, values: false, ...over });
const hide = (o) => ({ legend: false, tooltip: false, viz: false, ...o });

// Twenty points, one a minute from the start of the seed range.
const SERIES3 = [
  "SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(number) AS a, toFloat64(number * 2) AS b, toFloat64(number * 1000) AS big",
  "FROM numbers(20)",
];

export const cases = [
  {
    id: "B36",
    name: "stat justify: auto centres an item with no name, left for a named item; center centres both",
    async run(t) {
      const one = ["SELECT toFloat64(42) AS Total"];
      const three = ["SELECT toFloat64(1) AS A, toFloat64(2) AS B, toFloat64(3) AS C"];
      const o = (over) => ({ color_mode: "none", graph_mode: "none", reduce: reduce({ calc: "last" }), ...over });
      const d = doc("justify-test", "Justify", [
        section(null, [
          panel(1, "stat", "One auto", one, { grid: g(0, 0, 6, 4), options: o({ justify: "auto" }) }),
          panel(2, "stat", "One center", one, { grid: g(6, 0, 6, 4), options: o({ justify: "center" }) }),
          panel(3, "stat", "Three auto", three, { grid: g(12, 0, 12, 4), options: o({ justify: "auto" }) }),
          panel(4, "stat", "Three center", three, { grid: g(0, 4, 12, 4), options: o({ justify: "center" }) }),
          panel(5, "stat", "One named auto", one, { grid: g(12, 4, 6, 4), options: o({ justify: "auto", text_mode: "value_and_name" }) }),
          panel(6, "stat", "One named center", one, { grid: g(18, 4, 6, 4), options: o({ justify: "center", text_mode: "value_and_name" }) }),
          panel(7, "stat", "Three, no names", three, { grid: g(0, 8, 12, 4), options: o({ justify: "auto", text_mode: "value" }) }),
          panel(8, "stat", "Default (no member)", one, { grid: g(12, 8, 6, 4), options: o({ text_mode: "value_and_name" }) }),
          panel(9, "stat", "Wide auto", three, { grid: g(0, 12, 12, 4), options: o({ justify: "auto", orientation: "horizontal" }) }),
          panel(10, "stat", "Wide center", three, { grid: g(12, 12, 12, 4), options: o({ justify: "center", orientation: "horizontal" }) }),
        ]),
      ]);
      // Panel 8 has no `justify` member at all: the member is optional.
      delete d.sections[0].panels[7].options.justify;
      await ensure(t, d);
      const p = await t.open(t.dash("justify-test"));
      await reveal(p, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
      const read = (id) => p.eval(`(() => {
        const items = [...document.querySelectorAll('[data-testid="stat-item-${id}"]')];
        return items.map((i) => ({
          align: i.dataset.align,
          named: !i.querySelector(".stat__title").hidden,
          valueAlign: getComputedStyle(i.querySelector(".stat__value")).textAlign,
          boxJustify: getComputedStyle(i.querySelector(".stat__box")).justifyContent,
          boxItems: getComputedStyle(i.querySelector(".stat__box")).alignItems,
          layout: i.dataset.layout,
          geom: (() => {
            const ib = i.getBoundingClientRect();
            const pad = parseFloat(getComputedStyle(i).paddingLeft);
            const rel = (e) => { const b = e.getBoundingClientRect(); return { left: Math.round(b.left - ib.left - pad), right: Math.round(ib.right - pad - b.right) }; };
            const t = i.querySelector(".stat__title-text");
            return { value: rel(i.querySelector(".stat__text")), title: t && !i.querySelector(".stat__title").hidden ? rel(t) : null };
          })(),
        }));
      })()`);
      const start = ["start", "left"];
      const geomProblems = [];
      const exp = async (id, count, named, center) => {
        const items = await read(id);
        t.eq(items.length, count, `panel ${id}: item count`);
        for (const it of items) {
          t.eq(it.named, named, `panel ${id}: name shown`);
          t.eq(it.align, center ? "center" : "start", `panel ${id}: data-align`);
          const gm = it.geom;
          if (center) {
            // A stacked tile centres each text. A wide tile centres the pair of the name and the value.
            if (it.layout === "stacked" || !gm.title) {
              t.assert(Math.abs(gm.value.left - gm.value.right) <= 2, `panel ${id}: the value is centred: ${JSON.stringify(gm)}`);
              if (gm.title) t.assert(Math.abs(gm.title.left - gm.title.right) <= 2, `panel ${id}: the name is centred: ${JSON.stringify(gm)}`);
            } else {
              const l = Math.min(gm.title.left, gm.value.left);
              const r = Math.min(gm.title.right, gm.value.right);
              t.assert(Math.abs(l - r) <= 2, `panel ${id}: the name and the value are centred together: ${JSON.stringify(gm)}`);
            }
          } else if (gm.title) {
            if (!(gm.title.left <= 2)) geomProblems.push(`panel ${id}: the name is not at the left edge: ${JSON.stringify(it)}`);
            if (it.layout === "stacked" && !(gm.value.left <= 2)) geomProblems.push(`panel ${id}: the value is not at the left edge: ${JSON.stringify(it)}`);
            if (it.layout === "wide" && !(gm.value.right <= 2)) geomProblems.push(`panel ${id}: the value is not at the right edge: ${JSON.stringify(it)}`);
          }
          if (center) {
            t.eq(it.valueAlign, "center", `panel ${id}: value text-align`);
            t.assert(it.boxJustify === "center" || it.boxItems === "center", `panel ${id}: box is centred: ${JSON.stringify(it)}`);
          } else {
            t.assert(start.includes(it.valueAlign), `panel ${id}: value text-align is at the start: ${it.valueAlign}`);
            t.assert(it.boxJustify !== "center", `panel ${id}: box is not centred: ${JSON.stringify(it)}`);
          }
        }
      };
      await exp(1, 1, false, true); // auto, one item, no name: centred
      await exp(2, 1, false, true); // center, no name
      await exp(3, 3, true, false); // auto, three items, names: left
      await exp(4, 3, true, true); // center, names
      await exp(5, 1, true, false); // auto, one item with a name: left
      await exp(6, 1, true, true); // center, one item with a name
      await exp(7, 3, false, true); // auto, text_mode value: no names, centred
      await exp(8, 1, true, false); // no member: auto
      await exp(9, 3, true, false); // auto, wide tiles, names: name at the left, value at the right
      await exp(10, 3, true, true); // center, wide tiles
      t.assert(geomProblems.length === 0, `${geomProblems.length} problems: ` + geomProblems.map((x) => x.replace(/^panel (\d+): the (\w+) is not at the (\w+) edge: .*"layout":"(\w+)".*$/, "panel $1 $4 $2 not at $3")).join("; "));
    },
  },

  {
    id: "B37",
    name: "hidden series: names + hide_from viz, legend, tooltip; a legend click changes the page only",
    async run(t) {
      const colours = [
        { match: { by: "name", value: "a" }, set: { color: { mode: "fixed", fixed: "#ff0000" } } },
        { match: { by: "name", value: "b" }, set: { color: { mode: "fixed", fixed: "#0000ff" } } },
        { match: { by: "name", value: "big" }, set: { color: { mode: "fixed", fixed: "#00ff00" } } },
      ];
      const names = (list, mode, flags) => ({ match: { by: "names", names: list, mode }, set: { hide_from: hide(flags) } });
      const opts = { line_width: 3, thresholds_style: "line", tooltip: { mode: "multi" } };
      const ts = (id, title, grid, overrides) => panel(id, "timeseries", title, SERIES3, { grid, options: opts, fields: { defaults: {}, overrides: [...colours, ...overrides] } });
      const d = doc("hide-series", "Hidden series", [
        section(null, [
          ts(1, "viz of big", g(0, 0, 12, 7), [names(["big"], "include", { viz: true })]),
          ts(2, "legend of b", g(12, 0, 12, 7), [names(["b"], "include", { legend: true })]),
          ts(3, "tooltip of b", g(0, 7, 12, 7), [names(["b"], "include", { tooltip: true })]),
          ts(4, "nothing hidden", g(12, 7, 12, 7), []),
          // The form that Grafana saves after a click on the legend and a save: `exclude` lists the series that stay visible.
          ts(5, "exclude a and b", g(0, 14, 12, 7), [names(["a", "b"], "exclude", { viz: true, legend: false, tooltip: false })]),
        ]),
      ]);
      await ensure(t, d);
      const before = await t.api.dashboard("hide-series");
      const p = await t.page();
      await p.recordCanvasText();
      await p.goto(t.dash("hide-series"));
      await p.idle();
      await reveal(p, [1, 2, 3, 4, 5]);
      const RED = [255, 0, 0];
      const BLUE = [0, 0, 255];
      const GREEN = [0, 255, 0];
      const legend = (id) => p.eval(`[...document.querySelectorAll('[data-testid="panel-${id}"] .chartCore__legendItem')].map((e) => ({ name: e.textContent, off: e.classList.contains("is-off"), pressed: e.getAttribute("aria-pressed") }))`);
      const names3 = (l) => l.map((e) => e.name).join(",");
      const axisOf = async (id) => (await thresholdSnapshot(p, id)).axis;

      // viz: the series is not drawn and the axis ignores it.
      t.assert((await colourPixels(p, 1, RED)) > 20, "panel 1: series a is drawn");
      t.assert((await colourPixels(p, 1, BLUE)) > 20, "panel 1: series b is drawn");
      t.eq(await colourPixels(p, 1, GREEN), 0, "panel 1: series big (viz) is not drawn");
      t.assert((await colourPixels(p, 4, GREEN)) > 20, "panel 4: big is drawn when nothing is hidden");
      const axis1 = await axisOf(1);
      const axis4 = await axisOf(4);
      t.assert(axis1[1] < 100, `panel 1: the axis ignores the hidden series (max ${axis1[1]})`);
      t.assert(axis4[1] >= 19000, `panel 4: the axis covers big (max ${axis4[1]})`);
      const ticks1 = (await axisTexts(p, 1)).y;
      t.assert(!ticks1.some((x) => /K|000/.test(x)), `panel 1: no tick of the hidden series: ${ticks1}`);
      // The legend keeps the entry of a series with viz and no `legend`, shown as off.
      const l1 = await legend(1);
      t.eq(names3(l1), "a,b,big", "panel 1: legend entries");
      // legend: no entry for b, the series is still drawn.
      t.eq(names3(await legend(2)), "a,big", "panel 2: legend has no entry for b");
      t.assert((await colourPixels(p, 2, BLUE)) > 20, "panel 2: b is still drawn");
      t.eq(names3(await legend(3)), "a,b,big", "panel 3: legend keeps all entries");
      // tooltip: no row for b, no dot.
      const tip3 = await tooltipAt(p, 3, 0.5);
      t.assert(tip3, "panel 3: tooltip shows");
      t.eq(tip3.rows.map((r) => r.label), ["a", "big"], "panel 3: no tooltip row for b");
      const dots3 = await p.eval(`[...document.querySelectorAll('[data-testid="panel-3"] .chartCore__dot')].filter((e) => !e.hidden && getComputedStyle(e).display !== "none").length`);
      t.assert(dots3 <= 2, `panel 3: at most two dots (a, big): ${dots3}`);
      const tip4 = await tooltipAt(p, 4, 0.5);
      t.eq(tip4.rows.map((r) => r.label), ["a", "b", "big"], "panel 4: tooltip has all rows");
      // exclude: a and b stay visible, big is hidden.
      t.assert((await colourPixels(p, 5, RED)) > 20, "panel 5: a is drawn (excluded from the hiding)");
      t.eq(await colourPixels(p, 5, GREEN), 0, "panel 5: big is hidden by `exclude`");
      // The hidden series stays in the data: View data keeps its column.
      await p.click(p.tid("panel-1-menu"));
      await p.click(p.tid("panel-1-view-data"));
      await p.waitSel(p.tid("view-data-1"));
      const heads = await p.eval(`[...document.querySelectorAll('[data-testid="view-data-1"] thead th')].map((e) => e.textContent)`);
      t.assert(heads.includes("big"), "View data keeps the field of the hidden series: " + heads);
      await p.press("Escape");
      await p.waitGone(p.tid("view-data-1")).catch(() => {});

      // A legend click: this page only. The document, its etag and the requests do not change.
      p.clearRequests();
      const legendBefore = await legend(4);
      const pxBefore = { blue: await colourPixels(p, 4, BLUE), green: await colourPixels(p, 4, GREEN) };
      await p.click('[data-testid="panel-4"] .chartCore__legendItem[data-index="0"]');
      await sleep(300);
      await p.idle();
      const legendAfter = await legend(4);
      t.assert(JSON.stringify(legendBefore) !== JSON.stringify(legendAfter), `the legend click changes the legend: ${JSON.stringify(legendAfter)}`);
      const pxAfter = { blue: await colourPixels(p, 4, BLUE), green: await colourPixels(p, 4, GREEN) };
      t.assert(pxBefore.blue > 20 && pxBefore.green > 20 && pxAfter.green === 0 && pxAfter.blue === 0, `the legend click changes the plot: ${JSON.stringify([pxBefore, pxAfter])}`);
      t.eq(p.requests.filter((r) => r.method !== "GET" && !/\/panels\/\d+\/query$/.test(r.url)).map((r) => r.method + " " + r.url), [], "no write request after the click");
      const after = await t.api.dashboard("hide-series");
      t.eq(after.etag, before.etag, "the etag is the same after the click");
      t.eq(after.text, before.text, "the document is the same after the click");
      const hf = JSON.stringify(after.json).match(/"hide_from":\{[^}]*\}/g) || [];
      t.eq(hf.length, JSON.stringify(before.json).match(/"hide_from":\{[^}]*\}/g).length, "hide_from is unchanged");
      // The page state, not the document: a reload shows the series again.
      await p.reload();
      await p.idle();
      await reveal(p, [4]);
      const reloaded = { green: await colourPixels(p, 4, GREEN), blue: await colourPixels(p, 4, BLUE), legend: await legend(4) };
      t.assert(reloaded.green > 20 && reloaded.blue > 20, `after a reload all series are drawn again: ${JSON.stringify(reloaded)} (before the click ${JSON.stringify(pxBefore)})`);
    },
  },

  {
    id: "B38",
    name: "thresholds_style in timeseries and barchart: lines and bands, absolute and percentage",
    async run(t) {
      const STYLES = ["line", "dashed", "area", "line_area", "dashed_area"];
      const steps = (a, b) => [{ value: null, color: "green" }, { value: a, color: "orange" }, { value: b, color: "rgba(10, 20, 30, 0.4)" }];
      const MODES = {
        absolute: { thresholds: { mode: "absolute", steps: steps(30, 70) }, values: [30, 70] },
        percentage: { thresholds: { mode: "percentage", steps: steps(25, 75) }, values: [25, 75] },
      };
      const ts = ["SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(number * 5) AS v", "FROM numbers(20)"];
      const bars = ["SELECT concat('c', toString(number)) AS cat, toFloat64(number * 5) AS v", "FROM numbers(20)"];
      const panels = [];
      const index = {};
      let id = 1;
      for (const type of ["timeseries", "barchart"]) {
        for (const mode of Object.keys(MODES)) {
          for (const style of STYLES) {
            const fields = { defaults: { min: 0, max: 100, thresholds: MODES[mode].thresholds, color: { mode: "fixed", fixed: "purple" } }, overrides: [] };
            const n = panels.length;
            panels.push(panel(id, type, `${type} ${mode} ${style}`, type === "timeseries" ? ts : bars, {
              grid: g((n % 2) * 12, Math.floor(n / 2) * 6, 12, 6),
              fields,
              options: { thresholds_style: style, ...(type === "barchart" ? { x_field: null } : {}) },
            }));
            index[id] = { type, mode, style };
            id++;
          }
        }
      }
      // Horizontal bar charts (DOM): the five styles in the mode absolute.
      const horizontal = {};
      for (const style of STYLES) {
        const n = panels.length;
        panels.push(panel(id, "barchart", `horizontal ${style}`, bars, {
          grid: g((n % 2) * 12, Math.floor(n / 2) * 6, 12, 6),
          fields: { defaults: { min: 0, max: 100, thresholds: MODES.absolute.thresholds, color: { mode: "fixed", fixed: "purple" } }, overrides: [] },
          options: { thresholds_style: style, orientation: "horizontal" },
        }));
        horizontal[id] = style;
        id++;
      }
      // One chart with `off`: no hook data. One chart with no thresholds: the default steps (a red line at 80).
      const offId = id++;
      panels.push(panel(offId, "timeseries", "off", ts, { grid: g(0, 6 * 18, 12, 6), options: { thresholds_style: "off" } }));
      const defId = id++;
      panels.push(panel(defId, "timeseries", "default steps", ts, { grid: g(12, 6 * 18, 12, 6), options: { thresholds_style: "line" } }));
      const d = doc("thresholds-plot", "Thresholds in the plot", [section(null, panels)]);
      await ensure(t, d);
      const p = await t.open(t.dash("thresholds-plot"));
      const css = (name) => p.eval(`getComputedStyle(document.documentElement).getPropertyValue(${JSON.stringify(name)}).trim()`);
      const green = await css("--viz-green");
      const orange = await css("--viz-orange");
      const near = (a, b, tol, msg) => t.assert(Math.abs(a - b) <= tol, `${msg}: ${a} vs ${b}`);
      for (const [pid, spec] of Object.entries(index)) {
        await reveal(p, [Number(pid)]);
        const label = `${spec.type} ${spec.mode} ${spec.style} (panel ${pid})`;
        const s = await thresholdSnapshot(p, Number(pid));
        t.assert(s, `${label}: the hook gives data`);
        t.eq(s.style, spec.style, `${label}: style`);
        const [a0, a1] = s.axis;
        if (spec.type === "timeseries") t.eq(s.axis, [0, 100], `${label}: the axis is the one of min and max`);
        const yOf = (v) => s.plot.top + (s.plot.h * (a1 - v)) / (a1 - a0);
        const wantLines = spec.style.includes("area") && !spec.style.startsWith("line") && !spec.style.startsWith("dashed") ? 0 : 2;
        const wantAreas = spec.style.includes("area") ? 3 : 0;
        t.eq(s.lines.length, wantLines, `${label}: line count (axis ${JSON.stringify(s.axis)}, ${JSON.stringify(s.lines)})`);
        t.eq(s.areas.length, wantAreas, `${label}: band count (axis ${JSON.stringify(s.axis)})`);
        t.eq(s.dashed, spec.style.startsWith("dashed"), `${label}: dashed`);
        const values = MODES[spec.mode].values;
        // With percentage: lo + (hi - lo) * p / 100, with min 0 and max 100 of the field.
        const abs = spec.mode === "absolute" ? values : values.map((q) => 0 + (100 - 0) * q / 100);
        if (wantLines) {
          s.lines.forEach((l, i) => {
            near(l.value, abs[i], 1e-9, `${label}: line ${i} value`);
            near(l.y, yOf(abs[i]), 0.51, `${label}: line ${i} position`);
            near(l.alpha, i === 0 ? 0.7 : 0.4, 1e-9, `${label}: line ${i} alpha`);
            t.eq(l.color, i === 0 ? orange : "rgba(10, 20, 30, 0.4)", `${label}: line ${i} colour`);
          });
        }
        if (wantAreas) {
          const edges = [a0, abs[0], abs[1], a1];
          const colours = [green, orange, "rgba(10, 20, 30, 0.4)"];
          const alphas = [0.15, 0.15, 0.4];
          s.areas.forEach((a, i) => {
            near(a.from, edges[i], 1e-9, `${label}: band ${i} from`);
            near(a.to, edges[i + 1], 1e-9, `${label}: band ${i} to`);
            near(a.top, yOf(edges[i + 1]), 0.01, `${label}: band ${i} top`);
            near(a.bottom, yOf(edges[i]), 0.01, `${label}: band ${i} bottom`);
            t.eq(a.color, colours[i], `${label}: band ${i} colour`);
            near(a.alpha, alphas[i], 1e-9, `${label}: band ${i} alpha`);
          });
          // The first band starts at the bottom of the plot and the last ends at the top.
          near(s.areas[0].bottom, s.plot.top + s.plot.h, 0.01, `${label}: first band at the bottom`);
          near(s.areas[2].top, s.plot.top, 0.01, `${label}: last band at the top`);
          // The canvas holds the band: its alpha at the middle of the orange band is about 0.15 (a grid line or a series can cover a pixel).
          const { top, bottom } = s.areas[1];
          const alphasSeen = await p.eval(`(() => {
            const c = document.querySelector('[data-testid="panel-${pid}"] canvas.chartCore__canvas');
            const out = [];
            for (const f of [0.2, 0.37, 0.63, 0.8]) for (let k = 0; k < 12; k++) out.push(c.getContext("2d").getImageData(Math.round(${s.plot.left} + 4 + k * ${s.plot.w / 13}), Math.round(${top} + (${bottom} - ${top}) * f), 1, 1).data[3]);
            return out;
          })()`);
          t.assert(alphasSeen.some((a) => Math.abs(a - 38) <= 3), `${label}: the canvas holds the band, alphas ${alphasSeen.join(",")}`);
        }
      }
      // Horizontal bars: the bands and the lines are upright, in the DOM.
      for (const [pid, style] of Object.entries(horizontal)) {
        await reveal(p, [Number(pid)]);
        await p.waitFor(`document.querySelectorAll('[data-testid="panel-${pid}"] .hbar__row').length > 0`, { what: "hbar rows" });
        const m = await p.eval(`(() => {
          const r = document.querySelector('[data-testid="panel-${pid}"]');
          const lefts = (sel) => [...r.querySelectorAll(sel)].map((e) => parseFloat(e.style.left));
          return { lines: lefts(".hbar__thr"), bands: [...r.querySelectorAll(".hbar__band")].map((e) => ({ left: parseFloat(e.style.left), width: parseFloat(e.style.width) })), dashed: [...r.querySelectorAll(".hbar__thr")].map((e) => e.classList.contains("is-dashed")) };
        })()`);
        const lines = style.includes("area") && !style.startsWith("line") && !style.startsWith("dashed") ? 0 : 2;
        t.eq(m.lines.length, lines, `horizontal ${style}: line count`);
        t.eq(m.bands.length, style.includes("area") ? 3 : 0, `horizontal ${style}: band count`);
        if (lines) {
          // The axis starts at 0: the positions are in the ratio of the values 30 and 70.
          near(m.lines[0] / m.lines[1], 30 / 70, 0.01, `horizontal ${style}: line positions`);
          t.assert(m.dashed.every((x) => x === style.startsWith("dashed")), `horizontal ${style}: dashed flag`);
        }
        if (m.bands.length) {
          near(m.bands[0].left, 0, 0.01, `horizontal ${style}: first band at the axis start`);
          near(m.bands[0].left + m.bands[0].width, m.bands[1].left, 0.01, `horizontal ${style}: bands touch`);
          near(m.bands[1].left + m.bands[1].width, m.bands[2].left, 0.01, `horizontal ${style}: bands touch`);
          near(m.bands[2].left + m.bands[2].width, 100, 0.05, `horizontal ${style}: last band at the end`);
        }
      }
      // `off` draws nothing and gives no hook data. Absent thresholds use the default steps: a red line at 80.
      await reveal(p, [offId, defId]);
      t.eq(await thresholdSnapshot(p, offId), null, "off: no hook data");
      const def = await thresholdSnapshot(p, defId);
      t.eq(def.lines.length, 1, "default steps: one line");
      t.eq(def.lines[0].value, 80, "default steps: at 80");
      t.eq(def.lines[0].color, await css("--viz-red"), "default steps: red");
    },
  },

  {
    id: "B39",
    name: "zone utc: same axis labels, tooltip head and time cells in Asia/Tokyo and America/New_York",
    async run(t) {
      const series = ["SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(number) AS v", "FROM numbers(20)"];
      const cells = ["SELECT event_time, blocks FROM analytics.seed ORDER BY event_time"];
      const mk = (uid, tz) => doc(uid, `Zone ${tz}`, [
        section(null, [
          panel(1, "timeseries", "Series", series, { grid: g(0, 0, 14, 8) }),
          panel(2, "table", "Cells", cells, { grid: g(14, 0, 10, 8) }),
        ]),
      ], { time: { from: "now-6h", to: "now", timezone: tz, refresh: null } });
      await ensure(t, mk("zone-utc", "utc"));
      await ensure(t, mk("zone-browser", "browser"));
      const run = async (uid, zone) => {
        const p = await t.page();
        await p.emulateTimezone(zone);
        await p.recordCanvasText();
        await p.goto(t.dash(uid));
        await p.idle();
        await waitPanelsReady(p, [1, 2]);
        const x = (await axisTexts(p, 1)).x;
        const tip = await tooltipAt(p, 1, 0.5);
        const cellTexts = await p.eval(`[...document.querySelectorAll('[data-testid="table-2"] tbody tr')].slice(0, 3).map((r) => r.children[0].textContent)`);
        const picker = await p.text(p.tid("time-picker-button"));
        const clock = await p.eval(`new Date(${SEED_FROM}).getHours() + ":" + Intl.DateTimeFormat().resolvedOptions().timeZone`);
        return { x, tip, cellTexts, picker, clock };
      };
      const tokyo = await run("zone-utc", "Asia/Tokyo");
      const ny = await run("zone-utc", "America/New_York");
      t.assert(tokyo.clock.endsWith("Asia/Tokyo") && ny.clock.endsWith("America/New_York"), `the zones apply: ${tokyo.clock}, ${ny.clock}`);
      t.assert(tokyo.clock.split(":")[0] !== ny.clock.split(":")[0], "the two browser clocks differ");
      // Equal in both zones.
      t.assert(tokyo.x.length >= 3, `x labels: ${tokyo.x}`);
      t.eq(tokyo.x, ny.x, "x axis labels equal in both zones");
      t.assert(tokyo.tip && ny.tip, "tooltips show");
      t.eq(tokyo.tip.head, ny.tip.head, "tooltip head equal in both zones");
      t.eq(tokyo.cellTexts, ny.cellTexts, "time cells equal in both zones");
      t.eq(tokyo.picker, ny.picker, "time picker text equal in both zones");
      // Equal to UTC.
      const utc = (ms) => new Date(ms).toISOString();
      const hhmm = (ms) => utc(ms).slice(11, 16);
      const lo = hhmm(1794211200000);
      const hi = hhmm(SEED_TO); // the x axis covers the requested range
      for (const label of tokyo.x) {
        const m = /^(\d\d):(\d\d)(?::(\d\d))?$/.exec(label);
        t.assert(m, `x label is a clock: ${label}`);
        const key = `${m[1]}:${m[2]}`;
        t.assert(key >= lo && key <= hi, `x label ${label} is in the UTC range ${lo} to ${hi}`);
      }
      const firstUtc = utc(SEED_FROM);
      t.assert(tokyo.cellTexts[0].includes(firstUtc.slice(11, 19)), `first cell ${tokyo.cellTexts[0]} holds the UTC clock ${firstUtc.slice(11, 19)}`);
      t.assert(tokyo.cellTexts[0].includes(firstUtc.slice(0, 10)), `first cell ${tokyo.cellTexts[0]} holds the UTC date ${firstUtc.slice(0, 10)}`);
      // The tooltip head names a time of the series, in UTC.
      const idx = await (async () => tokyo.tip.head)();
      const headClock = /(\d\d):(\d\d)(?::(\d\d))?/.exec(idx);
      t.assert(headClock && `${headClock[1]}:${headClock[2]}` >= lo && `${headClock[1]}:${headClock[2]}` <= hi, `tooltip head ${idx} is in the UTC range`);
      // Control: a dashboard in the zone `browser` follows the zone of the browser (nine hours of difference).
      const bTokyo = await run("zone-browser", "Asia/Tokyo");
      const bNy = await run("zone-browser", "America/New_York");
      t.assert(bTokyo.cellTexts[0] !== bNy.cellTexts[0], `browser zone: the cells differ (${bTokyo.cellTexts[0]} / ${bNy.cellTexts[0]})`);
      t.assert(bTokyo.cellTexts[0] !== tokyo.cellTexts[0], "browser zone in Tokyo differs from utc");
    },
  },

  {
    id: "B40",
    name: "table: footer, wrap, gauge cells (three modes), json cell with an injection value",
    async run(t) {
      const rows = ["SELECT node, blocks FROM analytics.seed ORDER BY event_time"];
      const foot = (reducers) => ({ footer: { reducers } });
      const filt = { filterable: true };
      const long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon";
      const wrapSql = [`SELECT '${long}' AS txt, number AS n FROM numbers(3)`];
      const thr = { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 10, color: "orange" }, { value: 20, color: "red" }] };
      const gaugeSql = ["SELECT toFloat64(0) AS g1, toFloat64(0) AS g2, toFloat64(0) AS g3 UNION ALL SELECT 14.5, 14.5, 14.5 UNION ALL SELECT 29, 29, 29"];
      const jsonSql = ["SELECT '<img src=x onerror=alert(1)>' AS evil, '{\"a\": 1, \"b\": [1, 2]}' AS ok UNION ALL SELECT 'plain text', '[ 1 , 2 ]'"];
      const d = doc("table-features", "Table features", [
        section(null, [
          panel(1, "table", "Footer one", rows, { grid: g(0, 0, 8, 9), fields: { defaults: {}, overrides: [{ match: { by: "name", value: "node" }, set: filt }, { match: { by: "name", value: "blocks" }, set: foot(["total"]) }] } }),
          panel(2, "table", "Footer two", rows, { grid: g(8, 0, 8, 9), fields: { defaults: {}, overrides: [{ match: { by: "name", value: "node" }, set: filt }, { match: { by: "name", value: "blocks" }, set: foot(["total", "max"]) }] } }),
          panel(3, "table", "Footer mixed", rows, { grid: g(16, 0, 8, 9), fields: { defaults: {}, overrides: [{ match: { by: "name", value: "node" }, set: { ...filt, ...foot(["count"]) } }, { match: { by: "name", value: "blocks" }, set: foot(["total", "max"]) }] } }),
          panel(4, "table", "No wrap", wrapSql, { grid: g(0, 9, 12, 8), options: { wrap: false }, fields: { defaults: {}, overrides: [{ match: { by: "name", value: "txt" }, set: { width: 200 } }] } }),
          panel(5, "table", "Wrap", wrapSql, { grid: g(12, 9, 12, 8), options: { wrap: true }, fields: { defaults: {}, overrides: [{ match: { by: "name", value: "txt" }, set: { width: 200, display_name: "A long header text that must not wrap in the header" } }] } }),
          panel(6, "table", "Gauges", gaugeSql, {
            grid: g(0, 17, 14, 8),
            fields: {
              defaults: { min: 0, max: 29, thresholds: thr, color: { mode: "thresholds", fixed: null } },
              overrides: [
                { match: { by: "name", value: "g1" }, set: { cell: { type: "gauge", mode: "basic" } } },
                { match: { by: "name", value: "g2" }, set: { cell: { type: "gauge", mode: "gradient" } } },
                { match: { by: "name", value: "g3" }, set: { cell: { type: "gauge", mode: "lcd" } } },
              ],
            },
          }),
          panel(7, "table", "Json", jsonSql, {
            grid: g(14, 17, 10, 8),
            fields: { defaults: {}, overrides: [{ match: { by: "names", names: ["evil", "ok"], mode: "include" }, set: { cell: { type: "json", mode: null } } }] },
          }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("table-features"));
      await reveal(p, [1, 2, 3, 4, 5, 6, 7]);

      // --- footer ---
      const sums = async (where) => {
        const n = (await t.ch.query(`SELECT sum(blocks), max(blocks), count() FROM analytics.seed ${where} FORMAT TSV`)).trim().split("\t");
        return { total: n[0], max: n[1], count: n[2] };
      };
      const all = await sums("");
      const v0 = await sums("WHERE node = 'v0'");
      const footer = (id) => p.eval(`(() => {
        const t = document.querySelector('[data-testid="table-${id}"]');
        const foot = t.querySelector("tfoot");
        if (!foot) return null;
        const f = foot.querySelector("td"); // the cells are sticky, not the element tfoot
        const sr = t.getBoundingClientRect();
        const fr = f.getBoundingClientRect();
        return {
          cells: [...foot.querySelectorAll("td")].map((td) => [...td.querySelectorAll(".vtable__foot-line")].map((l) => ({ name: (l.querySelector(".vtable__foot-name") || {}).textContent || "", value: (l.querySelector(".vtable__foot-value") || {}).textContent || "", h: l.getBoundingClientRect().height }))),
          inView: fr.bottom <= sr.bottom + 1 && fr.top >= sr.top,
          scrolls: t.scrollHeight > t.clientHeight,
        };
      })()`);
      // One reducer. The first visible column has no footer and the other columns have the same items: the first column shows the names.
      let f1 = await footer(1);
      t.eq(f1.cells.map((c) => c.map((l) => [l.name, l.value])), [[["Total", ""]], [["", all.total]]], "footer one: names in the first column, values in the others");
      t.eq(f1.cells[0][0].h, 22, "footer line is 22 px high");
      t.assert(f1.scrolls && f1.inView, `footer one stays in view: ${JSON.stringify({ s: f1.scrolls, v: f1.inView })}`);
      // The footer stays in view when the table scrolls.
      await p.eval(`document.querySelector('[data-testid="table-1"]').scrollTop = 99999`);
      await sleep(150);
      t.assert((await footer(1)).inView, "footer one stays in view after a scroll");
      await p.eval(`document.querySelector('[data-testid="table-1"]').scrollTop = 0`);
      // Two reducers.
      const f2 = await footer(2);
      t.eq(f2.cells.map((c) => c.map((l) => [l.name, l.value])), [[["Total", ""], ["Max", ""]], [["", all.total], ["", all.max]]], "footer two");
      // Different items: each cell shows its names and its values; the footer has as many lines as the column with the most items.
      const f3 = await footer(3);
      t.eq(f3.cells.map((c) => c.map((l) => [l.name, l.value])), [[["Count", all.count], ["", ""]], [["Total", all.total], ["Max", all.max]]], "footer mixed");
      // The filter changes the result.
      const filterTo = async (id, text) => {
        await p.click(p.tid(`filter-${id}-0`));
        await p.waitSel(".filter-popup");
        await p.clickText(".filter-popup .btn", "Clear");
        await p.eval(`(() => { for (const l of document.querySelectorAll(".filter-popup .filter-item")) if (l.textContent.trim() === ${JSON.stringify(text)}) l.querySelector("input").click(); })()`);
        await p.clickText(".filter-popup .btn", "Apply");
        await p.waitGone(".filter-popup");
        await sleep(150);
      };
      await filterTo(1, "v0");
      f1 = await footer(1);
      t.eq(f1.cells[1][0].value, v0.total, "footer one after the filter");
      await filterTo(2, "v0");
      const f2b = await footer(2);
      t.eq(f2b.cells[1].map((l) => l.value), [v0.total, v0.max], "footer two after the filter");
      await filterTo(3, "v0");
      const f3b = await footer(3);
      t.eq(f3b.cells.map((c) => c.map((l) => l.value)), [[v0.count, ""], [v0.total, v0.max]], "footer mixed after the filter");
      t.assert(v0.total !== all.total, "the filter changes the result");

      // --- wrap ---
      const rowsH = (id) => p.eval(`(() => {
        const t = document.querySelector('[data-testid="table-${id}"]');
        return { rows: [...t.querySelectorAll("tbody tr[data-row]")].map((r) => Math.round(r.getBoundingClientRect().height)), head: Math.round(t.querySelector("thead tr").getBoundingClientRect().height), ws: getComputedStyle(t.querySelector("tbody td")).whiteSpace };
      })()`);
      const nw = await rowsH(4);
      const w = await rowsH(5);
      t.eq(nw.rows, [36, 36, 36], "wrap false: rows are 36 px high");
      t.assert(w.rows.every((h) => h > 36), `wrap true: rows grow: ${w.rows}`);
      t.assert(w.rows.every((h) => h === w.rows[0]), "wrap true: equal rows are equal");
      t.eq(w.head, 34, "a header text never wraps (34 px)");
      t.eq(nw.ws, "nowrap", "wrap false: no wrap in cells");
      t.assert(w.ws !== "nowrap", "wrap true: cells wrap");

      // --- gauge ---
      const gauges = await p.eval(`(() => [...document.querySelectorAll('[data-testid="table-6"] tbody tr[data-row]')].map((r) => [...r.children].map((td) => {
        const g = td.querySelector(".cell-gauge");
        const fill = g.querySelector(".cell-gauge__fill");
        const bar = g.querySelector(".cell-gauge__bar");
        return { mode: g.dataset.mode, now: g.getAttribute("aria-valuenow"), text: td.querySelector(".cell-gauge__text").textContent, fill: fill ? fill.style.width : null, bg: fill ? (fill.style.backgroundImage || fill.style.background) : null, lit: g.querySelectorAll(".cell-gauge__cell.is-lit").length, cells: g.querySelectorAll(".cell-gauge__cell").length, lcd: bar.classList.contains("cell-gauge__bar--lcd") };
      })))()`);
      t.eq(gauges.map((r) => r.map((c) => c.mode)), [["basic", "gradient", "lcd"], ["basic", "gradient", "lcd"], ["basic", "gradient", "lcd"]], "gauge modes");
      // Row order of the SQL is not fixed: find the rows by value.
      const byValue = (v) => gauges.find((r) => r[0].now === String(v));
      const zero = byValue(0);
      const mid = byValue(14.5);
      const top = byValue(29);
      t.assert(zero && mid && top, "gauge rows found");
      t.eq([zero[0].fill, zero[1].fill], [null, null], "gauge: value 0 gives an empty bar");
      t.eq(zero[2].lit, 0, "gauge lcd: value 0 lights no cell");
      t.eq(parseFloat(mid[0].fill), 50, "gauge basic: 14.5 of 0 to 29 is 50 %");
      t.eq(parseFloat(mid[1].fill), 50, "gauge gradient: 50 %");
      t.eq(parseFloat(top[0].fill), 100, "gauge basic: full");
      t.eq(parseFloat(top[1].fill), 100, "gauge gradient: full");
      t.assert(top[2].cells > 3 && top[2].lit >= top[2].cells - 1, `gauge lcd: full lights the cells ${top[2].lit}/${top[2].cells}`);
      t.assert(mid[2].lit > 0 && mid[2].lit < mid[2].cells, `gauge lcd: half lights some cells ${mid[2].lit}/${mid[2].cells}`);
      t.assert(Math.abs(mid[2].lit / mid[2].cells - 0.5) < 0.15, `gauge lcd: about half ${mid[2].lit}/${mid[2].cells}`);
      t.assert(/gradient/.test(top[1].bg) && !/gradient/.test(top[0].bg), `gradient mode draws a gradient, basic a plain colour: ${top[1].bg} | ${top[0].bg}`);
      t.eq(top[0].text, "29", "gauge text");

      // --- json ---
      p.dialogs.length = 0;
      const j = await p.eval(`(() => {
        const t = document.querySelector('[data-testid="table-7"]');
        return { cells: [...t.querySelectorAll("tbody tr[data-row]")].map((r) => [...r.children].map((td) => td.textContent)), imgs: document.querySelectorAll('[data-testid="panel-7"] img').length, kids: [...t.querySelectorAll("tbody td")].map((td) => td.children.length), evil: !!window.alert.__called };
      })()`);
      const evil = "<img src=x onerror=alert(1)>";
      const flat = j.cells.flat();
      t.assert(flat.includes(evil), `the injection value shows as text: ${JSON.stringify(flat)}`);
      t.assert(flat.includes('{"a":1,"b":[1,2]}'), "valid JSON gives its compact form");
      t.assert(flat.includes("[1,2]"), "a JSON array gives its compact form");
      t.assert(flat.includes("plain text"), "text that is not JSON shows as it is");
      t.eq(j.imgs, 0, "no img element in the panel");
      t.eq(j.kids.every((n) => n === 0), true, "json cells hold text nodes only");
      await sleep(400);
      t.eq(p.dialogs.length, 0, "no alert ran");
      // View data shows the same text and runs no script.
      await p.click(p.tid("panel-7-menu"));
      await p.click(p.tid("panel-7-view-data"));
      await p.waitSel(p.tid("view-data-7"));
      const vd = await p.eval(`[...document.querySelectorAll('[data-testid="view-data-7"] tbody td')].map((e) => e.textContent).concat([document.querySelectorAll('[data-testid="view-data-7"] img').length])`);
      t.assert(vd.includes(evil) && vd[vd.length - 1] === 0, "View data shows the text and no img");
      await sleep(300);
      t.eq(p.dialogs.length, 0, "no alert ran in View data");
    },
  },

  {
    id: "B41",
    name: "CSV download: header, quote rule, formula guard, number fields, mapping text, file name, no CSP violation",
    async run(t) {
      const row = (idx, txt, n, pc) => `SELECT ${idx} AS idx, ${txt} AS \`=txt\`, toInt32(${n}) AS n, toFloat64(${pc}) AS pc`;
      const sql = [
        "SELECT * FROM (",
        [
          row(1, "'=cmd'", -1, -5.2),
          row(2, "'+1'", 7, 3),
          row(3, "'-1'", -5, 0),
          row(4, "'@x'", 0, 1.5),
          row(5, "concat(char(9), 'tab')", 2, -1.25),
          row(6, "'plain'", 3, 0),
          row(7, "'has,comma'", 4, 0),
          row(8, "'has\"quote'", 5, 0),
          row(9, "concat('line', char(10), 'break')", 6, 0),
          row(10, "'a'", 8, 0),
        ].join(" UNION ALL "),
        ") ORDER BY idx",
      ];
      const d = doc("csv-test", "CSV", [
        section(null, [
          panel(1, "table", "Export $env! <x>", sql, {
            grid: g(0, 0, 24, 10),
            fields: {
              defaults: {},
              overrides: [
                { match: { by: "name", value: "n" }, set: { mappings: [{ kind: "value", value: "-5", text: "-5 =cmd", color: null }] } },
                { match: { by: "name", value: "pc" }, set: { unit: "percent", decimals: 1 } },
              ],
            },
          }),
        ]),
      ], { variables: [{ kind: "custom", name: "env", label: null, hide: "none", options: [{ text: "Production", value: "prod" }], multi: false, include_all: false, all_value: null, default: ["prod"] }] });
      await ensure(t, d);
      const p = await t.page();
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: `window.__csp = []; document.addEventListener("securitypolicyviolation", (e) => window.__csp.push(e.violatedDirective + " " + e.blockedURI));` });
      await p.goto(t.dash("csv-test"));
      await p.idle();
      await waitPanelsReady(p, [1]);
      await p.click(p.tid("panel-1-menu"));
      await p.click(p.tid("panel-1-view-data"));
      await p.waitSel(p.tid("view-data-1"));
      await p.click(p.tid("view-data-csv"));
      let files = [];
      for (let i = 0; i < 100; i++) {
        files = readdirSync(t.downloads);
        if (files.length && !files.some((f) => f.endsWith(".crdownload"))) break;
        await sleep(100);
      }
      t.eq(files.length, 1, "one file downloaded: " + files);
      const name = files[0];
      t.assert(/^Export Production_ _x_-data-\d{8}-\d{6}\.csv$/.test(name), `file name: ${name}`);
      const stamp = /-data-(\d{4})(\d\d)(\d\d)-(\d\d)(\d\d)(\d\d)\.csv$/.exec(name);
      const at = Date.UTC(+stamp[1], +stamp[2] - 1, +stamp[3], +stamp[4], +stamp[5], +stamp[6]);
      t.assert(Math.abs(Date.now() - at) < 120000, "the name has the UTC time of now");
      const raw = readFileSync(join(t.downloads, name));
      t.assert(!(raw[0] === 0xef && raw[1] === 0xbb && raw[2] === 0xbf), "no byte order mark");
      const text = raw.toString("utf8");
      t.assert(text.endsWith("\r\n"), "the last row has a row end");
      t.assert(!/[^\r]\n/.test(text.replace(/"[^"]*"/g, "")), "rows end with CR LF (a bare LF is only inside quotes)");
      const lines = text.split("\r\n");
      lines.pop();
      const expected = [
        "idx,'=txt,n,pc",
        "1,'=cmd,-1,-5.2%",
        "2,'+1,7,3.0%",
        "3,'-1,'-5 =cmd,0.0%",
        "4,'@x,0,1.5%",
        "5,'\ttab,2,-1.3%", // a tab needs the guard, and no quote marks
        "6,plain,3,0.0%",
        "7,\"has,comma\",4,0.0%",
        "8,\"has\"\"quote\",5,0.0%",
        "9,\"line\nbreak\",6,0.0%",
        "10,a,8,0.0%",
      ];
      // The decimals of -1.25 with one decimal: do not fix the rounding rule here, only the shape of the cell.
      t.eq(lines.length, expected.length, "row count: " + JSON.stringify(lines));
      expected.forEach((e, i) => {
        if (i === 5) {
          t.assert(lines[i].startsWith("5,'\ttab,2,-1."), `row 5: ${JSON.stringify(lines[i])}`);
          t.assert(/,-1\.[23]%$/.test(lines[i]), `row 5 number cell has no quote guard: ${JSON.stringify(lines[i])}`);
        } else t.eq(lines[i], e, `row ${i}`);
      });
      // No `'` before a number such as -1 or -5.2% in a number field, a `'` before the mapping text.
      t.assert(lines[1].includes(",-1,-5.2%"), "number -1 and -5.2% have no guard");
      t.assert(lines[3].includes(",'-5 =cmd,"), "mapping text in a number field has the guard");
      const csp = await p.eval("window.__csp");
      t.eq(csp, [], "no securitypolicyviolation event");
      // The panel menu has the same entry. A second page has its own download permission, so that the browser does not hold back the file.
      // The first file moves aside: a second download in the same second has the same name, and the browser then replaces the file.
      renameSync(join(t.downloads, name), join(t.downloads, `${name}.first`));
      const p2 = await t.page();
      await p2.goto(t.dash("csv-test"));
      await p2.idle();
      await waitPanelsReady(p2, [1]);
      await p2.click(p2.tid("panel-1-menu"));
      await p2.click(p2.tid("panel-1-csv"));
      for (let i = 0; i < 100 && readdirSync(t.downloads).filter((f) => f.endsWith(".csv")).length < 1; i++) await sleep(100);
      const second = readdirSync(t.downloads).filter((f) => f.endsWith(".csv"));
      t.eq(second.length, 1, "the panel menu entry downloads a file too: " + readdirSync(t.downloads));
      if (second.length) t.eq(readFileSync(join(t.downloads, second[0]), "utf8"), text, "same content from the menu");
      t.eq(await p.eval("window.__csp"), [], "no securitypolicyviolation event after the second download");
    },
  },

  {
    id: "B42",
    name: "titles with variables: panel and section titles follow the selection; markup is text; a non-variable stays",
    async run(t) {
      const d = doc("titles-vars", "Titles", [
        section("Section ${env}", [
          panel(1, "table", "Latency of $svc in ${env} [[env]] ${nothere} $__name", ["SELECT 1 AS x"], { grid: g(0, 0, 12, 6), description: "Desc ${env}" }),
          panel(2, "stat", "Host ${svc:glob} / ${env:csv}", ["SELECT 1 AS x"], { grid: g(12, 0, 12, 6) }),
        ]),
        section("Plain section", [panel(3, "table", "No placeholder", ["SELECT 1 AS x"], { grid: g(0, 0, 12, 6) })]),
      ], {
        variables: [
          { kind: "custom", name: "env", label: null, hide: "none", options: [{ text: "Production", value: "prod" }, { text: "<b>x</b>", value: "raw" }], multi: false, include_all: false, all_value: null, default: ["prod"] },
          { kind: "text", name: "svc", label: null, hide: "none", value_type: "text", default: "api" },
        ],
      });
      await ensure(t, d);
      const p = await t.open(t.dash("titles-vars"));
      await waitPanelsReady(p, [1, 2, 3]);
      const titles = () => p.eval(`({
        p1: document.querySelector('[data-testid="panel-1"] .panel__title').textContent,
        p2: document.querySelector('[data-testid="panel-2"] .panel__title').textContent,
        p3: document.querySelector('[data-testid="panel-3"] .panel__title').textContent,
        s0: document.querySelector('[data-testid="section-0"] .section__title').textContent,
        s1: document.querySelector('[data-testid="section-1"] .section__title').textContent,
        label0: document.querySelector('[data-testid="section-0"]').getAttribute("aria-label"),
        desc: document.querySelector('[data-testid="panel-1"] .panel__info').getAttribute("data-tooltip"),
        markup: document.querySelectorAll('.panel__title b, .section__title b, .panel__title i, .section__title i').length,
      })`);
      let s = await titles();
      t.eq(s.p1, "Latency of api in Production [[env]] ${nothere} $__name", "panel title: values, [[env]], unknown name and __name stay");
      t.eq(s.p2, "Host api / prod", "panel title: glob and csv formats");
      t.eq(s.p3, "No placeholder", "title with no placeholder");
      t.eq(s.s0, "Section Production", "section title follows the selection");
      t.eq(s.s1, "Plain section", "plain section title");
      t.eq(s.label0, "Section Production", "the section label follows too");
      t.eq(s.desc, "Desc Production", "description follows the selection");
      // A value that holds markup shows as text.
      await p.click(p.tid("variable-env-button"));
      await p.click('.listbox__item[data-value="raw"]');
      await p.idle();
      await p.waitFor(`document.querySelector('[data-testid="section-0"] .section__title').textContent === "Section <b>x</b>"`, { what: "section title follows" });
      s = await titles();
      t.eq(s.p1, "Latency of api in <b>x</b> [[env]] ${nothere} $__name", "panel title shows markup as text");
      t.eq(s.p2, "Host api / raw", "csv format of the value");
      t.eq(s.s0, "Section <b>x</b>", "section title shows markup as text");
      t.eq(s.markup, 0, "no b or i element in a title");
      // The text variable follows too, with markup as text.
      await p.fill(p.tid("variable-svc-input"), "<i>y</i>");
      await p.press("Enter");
      await p.idle();
      await p.waitFor(`document.querySelector('[data-testid="panel-1"] .panel__title').textContent.includes("<i>y</i>")`, { what: "title follows the text variable" });
      s = await titles();
      t.eq(s.p1, "Latency of <i>y</i> in <b>x</b> [[env]] ${nothere} $__name", "panel title follows the text variable");
      t.eq(s.markup, 0, "no element from a variable value");
      // The stored document is not changed.
      const after = await t.api.dashboard("titles-vars");
      t.eq(after.json.document.sections[0].title, "Section ${env}", "the stored section title is unchanged");
      t.eq(after.json.document.sections[0].panels[0].title, "Latency of $svc in ${env} [[env]] ${nothere} $__name", "the stored panel title is unchanged");
    },
  },
];

