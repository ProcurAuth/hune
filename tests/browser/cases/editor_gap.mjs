// Browser case B208 (9.6, 10.3; decision ID-K302): the gap under the sticky header.
// The content of every view that has the page layout starts one token `--page-pad` under the header, and the panel editor too: the preview
// and the options pane start at least that far under the header, in the editor of each of the 12 panel types, in the settings view, the JSON
// view and the history view. The view mode of the dashboard is the reference. At 1280, 1024, 768 and 390 px, in both themes, and at the
// browser zoom 90 %. The editor, the options pane and the query area still fit the window: no page scroll bar, no overflow.
import { doc, panel, section, SQL } from "../lib/docs.mjs";

/** Waits for two painted frames: the resize callbacks of the page have run (a wait on the frames, not on a fixed time). */
const painted = (p) => p.eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))");

const UID = "gap208";
const UID_VARS = "gap208v";
const g = (x, y, w, h) => ({ x, y, w, h });
const legend = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const TS = "SELECT toDateTime(1794211200,'UTC') + number*40 AS time, 150 + number AS a, 85 + number AS b FROM numbers(30)";
const MAP = [{ kind: "value", value: "0", text: "OK", color: "green" }, { kind: "value", value: "1", text: "WARN", color: "yellow" }];
const TL_STATE = "SELECT toDateTime(1794211200,'UTC') + number*40 AS time, intDiv(number,5)%2 AS a FROM numbers(30)";
const timeline = (id, type, title) => ({
  id, type, title, description: "", transparent: false, grid: g(0, 0, 12, 6),
  data: { query: { source: "prod", format: "table", sql: [TL_STATE], min_interval: null, max_data_points: null } },
  transforms: [], fields: { defaults: { mappings: MAP }, overrides: [] }, links: [],
  options: type === "state_timeline"
    ? { merge_values: true, show_values: "auto", align_values: "left", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend: legend(), tooltip: { mode: "single", sort: "none" } }
    : { show_values: "auto", row_height: 90, col_width: 90, line_width: 1, fill_opacity: 70, show_labels: true, legend: legend(), tooltip: { mode: "single", sort: "none" } },
});
const COLOR = { mode: "scheme", scheme: "oranges", fill: "dark-orange", scale: "exponential", exponent: 0.5, steps: 64, reverse: false, min: null, max: null };
const HEAT = { calculate: true, x_buckets: { mode: "count", value: null }, y_buckets: { mode: "count", value: null, scale: "linear" }, y_axis: "left", y_reverse: false, y_min: null, y_max: null, color: COLOR, cell_gap: 1, hide_le: 0, hide_ge: null, show_legend: true, tooltip: { mode: "single" } };
const HIST = { bucket_count: 30, bucket_size: null, bucket_offset: 0, combine: false, stacking: "none", line_width: 1, fill_opacity: 80, legend: legend(), tooltip: { mode: "single", sort: "none" } };

const LOGS = "SELECT toDateTime64(1794211200 + number*60, 3, 'UTC') AS timestamp, concat('line ', toString(number)) AS body FROM numbers(3)";
// The panel types, in the order of the panel ids 1 to n.
const TYPES = ["table", "timeseries", "stat", "barchart", "piechart", "xychart", "bargauge", "gauge", "state_timeline", "status_history", "heatmap", "histogram", "logs", "geomap", "text"];
const MAKE = {
  table: (id) => panel(id, "table", "Table", SQL.table),
  timeseries: (id) => panel(id, "timeseries", "Series", SQL.series),
  stat: (id) => panel(id, "stat", "Stat", SQL.total),
  barchart: (id) => panel(id, "barchart", "Bar chart", SQL.bars),
  piechart: (id) => panel(id, "piechart", "Pie chart", SQL.bars),
  xychart: (id) => panel(id, "xychart", "XY chart", SQL.xy),
  bargauge: (id) => panel(id, "bargauge", "Bar gauge", SQL.bars, { fields: { defaults: { min: 0, max: 400 }, overrides: [] } }),
  gauge: (id) => ({ ...panel(id, "stat", "Gauge", SQL.total), type: "gauge", options: { reduce: { calc: "last_not_null", fields: null, values: false } } }),
  state_timeline: (id) => timeline(id, "state_timeline", "State timeline"),
  status_history: (id) => timeline(id, "status_history", "Status history"),
  heatmap: (id) => ({ ...panel(id, "table", "Heatmap", TS), type: "heatmap", options: HEAT }),
  histogram: (id) => ({ ...panel(id, "table", "Histogram", TS), type: "histogram", options: HIST }),
  logs: (id) => ({ ...panel(id, "table", "Logs", LOGS), type: "logs", options: {}, data: { query: { source: "prod", format: "logs", sql: [LOGS], min_interval: null, max_data_points: null } } }),
  // The map (F37): the defaults of 9.19i, section 8, on two positions.
  geomap: (id) => ({ ...panel(id, "table", "Geomap", "SELECT 48.85 AS lat, 2.35 AS lon UNION ALL SELECT 40.7, -74.0"), type: "geomap", options: { view: { mode: "fixed", lat: 0, lon: 0, zoom: 1 }, layer: { lat_field: null, lon_field: null, size: 5, size_min: 2, size_max: 15, size_field: null, color: "dark-green", color_field: null, opacity: 40 }, tooltip: "details", show_zoom: true, mouse_wheel_zoom: true } }),
  text: (id) => ({ id, type: "text", title: "Text", description: "", transparent: false, grid: g(0, 0, 12, 6), options: { mode: "markdown", content: ["Some text"] }, links: [] }),
};

function gapDoc() {
  const panels = TYPES.map((type, i) => MAKE[type](i + 1));
  panels.forEach((p, i) => { p.grid = g((i % 2) * 12, Math.floor(i / 2) * 6, 12, 6); });
  return doc(UID, "Gap 208", [section(null, panels)], { time: { from: "now-6h", to: "now", timezone: "browser", refresh: null } });
}

// A dashboard with a variable: the row of the variables is docked in the editor, and its content starts one token under the header too.
function varsDoc() {
  const variable = { kind: "custom", name: "host", label: "Host", hide: "none", options: [{ text: "Alpha", value: "a" }, { text: "Bravo", value: "b" }], multi: true, include_all: true, all_value: null, default: ["$__all"] };
  return doc(UID_VARS, "Gap 208 variables", [section(null, [MAKE.timeseries(1)])], { variables: [variable], time: { from: "now-6h", to: "now", timezone: "browser", refresh: null } });
}

// The page measure. `rows` is a list of [name, selector]. The result has the foot of the header, the value of the token, the top of each
// element that is visible, and the facts of the fit: the page scroll bars and the boxes of the editor.
const MEASURE = (rows) => `(() => {
  const q = (s) => { for (const e of document.querySelectorAll(s)) if (e.offsetParent !== null || getComputedStyle(e).position === "fixed") return e; return null; };
  const header = document.getElementById("app-header").getBoundingClientRect();
  const probe = document.createElement("div");
  probe.style.cssText = "position:absolute;visibility:hidden;height:0;padding-top:var(--page-pad)";
  document.body.append(probe);
  const token = parseFloat(getComputedStyle(probe).paddingTop);
  probe.remove();
  const de = document.documentElement;
  const tops = {};
  for (const [name, sel] of ${JSON.stringify(rows)}) { const e = q(sel); tops[name] = e ? Math.round((e.getBoundingClientRect().top - header.bottom) * 100) / 100 : null; }
  const r = (s) => { const e = q(s); if (!e) return null; const b = e.getBoundingClientRect(); return { top: b.top, bottom: b.bottom, left: b.left, right: b.right, h: b.height, w: b.width }; };
  return {
    bottom: header.bottom, token, tops, win: [innerWidth, innerHeight],
    scrollY: de.scrollHeight - de.clientHeight, scrollX: de.scrollWidth - de.clientWidth,
    top: r(".peditor__top"), lower: r(".peditor__lower"), opts: r(".peditor__options"), main: r(".peditor__main"),
    pos: (() => { const e = q(".peditor__options"); return e ? getComputedStyle(e).position : null; })(),
  };
})()`;

const ROUTES = [
  ["view", "", [["content", ".dash"]]],
  ["settings", "edit=1&editview=settings", [["content", ".settings"]]],
  ["json", "edit=1&editview=json", [["content", ".jeditor"]]],
  ["history", "edit=1&editview=history", [["content", ".history"]]],
];

// [label, css width, scheme, zoom]
const SETUPS = [
  ["1280 light", 1280, "light", 1], ["1280 dark", 1280, "dark", 1],
  ["1024 light", 1024, "light", 1], ["1024 dark", 1024, "dark", 1],
  ["768 light", 768, "light", 1], ["768 dark", 768, "dark", 1],
  ["390 light", 390, "light", 1], ["390 dark", 390, "dark", 1],
  ["1280 zoom 90 light", 1280, "light", 0.9], ["1024 zoom 90 dark", 1024, "dark", 0.9],
];

async function openAt(t, url, width, scheme, zoom) {
  const cssW = Math.round(width / zoom);
  const cssH = Math.round((width === 390 ? 844 : 900) / zoom);
  const p = await t.page({ width: cssW, height: cssH });
  await p.send("Emulation.setDeviceMetricsOverride", { width: cssW, height: cssH, deviceScaleFactor: zoom, mobile: false });
  await p.emulateColorScheme(scheme);
  await p.goto(url);
  await p.idle();
  return p;
}

export const cases = [
  {
    id: "B208",
    name: "the gap under the header: the view, the panel editor of the 12 types, the settings, the JSON and the history views start one page padding under the header, and the editor still fits (ID-K302)",
    timeout: 480000,
    async run(t) {
      const r = await t.api.create("transit", gapDoc());
      t.assert(r.status === 201, `create ${UID}: ${r.status} ${r.text.slice(0, 300)}`);
      const rv = await t.api.create("transit", varsDoc());
      t.assert(rv.status === 201, `create ${UID_VARS}: ${rv.status} ${rv.text.slice(0, 300)}`);
      const found = [];
      const check = (label, route, m, extra = {}) => {
        const where = `${label} ${route}`;
        if (!(m.token >= 8)) found.push(`${where}: the token --page-pad is ${m.token}`);
        for (const [name, top] of Object.entries(m.tops)) {
          if (top === null) found.push(`${where}: ${name} is not in the page`);
          else if (top < m.token - 0.5) found.push(`${where}: ${name} is ${top} px under the header, the token is ${m.token}`);
        }
        // The row of tabs of the settings view scrolls on its own (editor.css), so no route has a horizontal page scroll.
        if (m.scrollX > 0) found.push(`${where}: horizontal page scroll ${m.scrollX} px`);
        if (extra.fit) {
          const wide = m.win[0] >= 901;
          if (wide && m.scrollY > 0) found.push(`${where}: page scroll bar, ${m.scrollY} px`);
          if (!m.top || m.top.h < 200 - 0.5) found.push(`${where}: the preview is ${m.top && m.top.h} px high, 200 at least`);
          if (wide && m.lower && m.lower.bottom > m.win[1] + 0.5) found.push(`${where}: the query area ends at ${m.lower.bottom}, the window at ${m.win[1]}`);
          if (wide && m.opts && (m.opts.bottom > m.win[1] + 0.5 || m.opts.right > m.win[0] + 0.5)) found.push(`${where}: the options pane leaves the window ${JSON.stringify(m.opts)}`);
          if (wide && m.top && m.lower && Math.abs(m.lower.top - m.top.bottom - 16) > 0.5) found.push(`${where}: preview and query area are ${m.lower.top - m.top.bottom} px apart`);
          if (wide && m.pos !== "fixed") found.push(`${where}: the options pane is ${m.pos}, fixed expected`);
          if (!extra.vars && m.top && m.opts && wide && Math.abs(m.top.top - m.opts.top) > 0.5) found.push(`${where}: the preview starts at ${m.top.top}, the options pane at ${m.opts.top}`);
        }
      };

      for (const [label, width, scheme, zoom] of SETUPS) {
        // 1. The reference and the views with the page layout.
        for (const [route, query, rows] of ROUTES) {
          const p = await openAt(t, t.dash(UID, `refresh=off${query ? `&${query}` : ""}`), width, scheme, zoom);
          await p.waitFor(`document.querySelector(${JSON.stringify(rows[0][1])}) !== null`, { what: route });
          await p.idle();
          await painted(p);
          check(label, route, await p.eval(MEASURE(rows)));
          await p.close();
        }
        // 2. The panel editor of the 12 types: the preview and the options pane.
        const p = await openAt(t, t.dash(UID, "refresh=off&edit=1&editPanel=1"), width, scheme, zoom);
        for (let i = 0; i < TYPES.length; i++) {
          if (i > 0) { await p.goto(t.dash(UID, `refresh=off&edit=1&editPanel=${i + 1}`)); await p.idle(); }
          await p.waitSel(p.tid("panel-editor"));
          await p.waitFor(`(() => { const e = document.querySelector(".peditor__top .panel"); return e && !["idle", "queued", "loading"].includes(e.dataset.state); })()`, { what: `the preview of ${TYPES[i]}` });
          await painted(p);
          check(label, `editor ${TYPES[i]}`, await p.eval(MEASURE([["preview", ".peditor__top"], ["options", ".peditor__options"], ["main", ".peditor__main"]])), { fit: true });
        }
        await p.close();
      }
      // 3. The row of the variables in the editor: its content, the preview and the options pane start under the header by the token.
      for (const [label, width, scheme, zoom] of SETUPS.filter((x) => /^(1280|390) /.test(x[0]) && x[3] === 1)) {
        const p = await openAt(t, t.dash(UID_VARS, "refresh=off&edit=1&editPanel=1"), width, scheme, zoom);
        await p.waitSel(p.tid("panel-editor"));
        await p.waitSel(".peditor > .dash-toolbar .vars-bar");
        await p.idle();
        await painted(p);
        const m = await p.eval(MEASURE([["variables", ".peditor > .dash-toolbar .vars-bar"], ["preview", ".peditor__top"], ["options", ".peditor__options"]]));
        check(label, "editor with variables", m, { fit: true, vars: true });
        const bar = await p.eval(`document.querySelector(".peditor > .dash-toolbar").getBoundingClientRect().bottom <= document.querySelector(".peditor__top").getBoundingClientRect().top + 0.5`);
        if (!bar) found.push(`${label} editor with variables: the preview starts over the end of the row of the variables`);
        await p.close();
      }
      t.eq(found.slice(0, 40), [], `the gap under the header (${found.length} faults)`);
    },
  },
];
