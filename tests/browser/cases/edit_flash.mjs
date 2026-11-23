// Browser case B207 (9.6, 10.2, 10.2a): the charts stay painted across the states of the grid (owner bug of 2026-11-15, ID-K301).
// The switch "Edit" on and off, the panel editor opened from the edit mode and from the view and closed, and Back and Forward over these
// states. A probe in the page samples each frame just after its paint (requestAnimationFrame, then a message), and once more at the end of
// the task that writes the hook `data-hune-view` (a MutationObserver): a chart that returns to the page draws in that task, so the check
// does not depend on the order of the tasks under load (ID-K321). A second MutationObserver records each new canvas. The steps wait on
// states (the hook, the frames that the probe samples, the marks), never for a fixed time.
// While the grid shows (the states `view` and `edit`), each panel frame keeps its plot and its chart canvas (the
// same elements as before the first switch), each canvas has pixels, and no frame shows a waiting state (idle, queued, loading, the
// skeleton or the dim). No panel query goes out, because no data changed. Two themes, six panel types (with the heatmap, which has its own fit), a panel that repeats and a section
// that repeats. The time series 1 has the mark of an annotation: the edit mode hides it and the view shows it again (9.19d, rule 22).
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { panelQueries, waitPanelsReady } from "../lib/h.mjs";

const UID = "editflash207";
const g = (x, y, w, h) => ({ x, y, w, h });
const legend = { show: true, placement: "bottom", mode: "list", values: [] };

/** A state timeline on 10 rows of the seed range (the panel builder of docs.mjs has no options for this type). */
function timeline(id, grid) {
  return {
    id, type: "state_timeline", title: "States", description: "", transparent: false, grid,
    data: { query: { source: "prod", format: "table", sql: ["SELECT toDateTime(1794211200 + number * 120) AS time, number % 3 AS state", "FROM numbers(10)"], min_interval: null, max_data_points: null } },
    transforms: [], fields: { defaults: {}, overrides: [] }, links: [],
    options: { merge_values: true, show_values: "auto", align_values: "left", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend, tooltip: { mode: "single", sort: "none" } },
  };
}

/** A heatmap of 40 time rows of the seed range (the panel builder of docs.mjs has no options for this type). */
function heatmap(id, grid) {
  return {
    id, type: "heatmap", title: "Heat", description: "", transparent: false, grid,
    data: { query: { source: "prod", format: "table", sql: ["SELECT toDateTime(1794211200 + number * 120) AS time, (number * 7) % 50 AS value", "FROM numbers(40)"], min_interval: null, max_data_points: null } },
    transforms: [], fields: { defaults: {}, overrides: [] }, links: [],
    options: { calculate: true, x_buckets: { mode: "count", value: null }, y_buckets: { mode: "count", value: null, scale: "linear" }, y_axis: "left", y_reverse: false, y_min: null, y_max: null, color: { mode: "scheme", scheme: "oranges", fill: "dark-orange", scale: "exponential", exponent: 0.5, steps: 64, reverse: false, min: null, max: null }, cell_gap: 1, hide_le: 0, hide_ge: null, show_legend: true, tooltip: { mode: "single" } },
  };
}

const HOST_SERIES = ["SELECT $__timeInterval(event_time) AS t, sum(weight) AS weight", "FROM analytics.seed", "WHERE $__timeFilter(event_time) AND '${host:csv}' != ''", "GROUP BY t", "ORDER BY t"];
const HOST_BARS = ["SELECT node, sum(blocks) AS blocks", "FROM analytics.seed", "WHERE '${host:csv}' != ''", "GROUP BY node", "ORDER BY node"];

// One mark in the seed range, on the time series 1 only.
const MARK = ["SELECT toDateTime(1794211500) AS time, toDateTime(0) AS timeEnd, 'Mark' AS title, '' AS text, '' AS tags"];

export function flashDoc() {
  const series = panel(5, "timeseries", "Host ${host}", HOST_SERIES, { grid: g(0, 12, 24, 6) });
  series.repeat = { variable: "host", direction: "h" };
  const d = doc(UID, "Edit flash", [
    section("Overview", [
      panel(1, "timeseries", "Series", SQL.series, { grid: g(0, 0, 8, 8) }),
      panel(2, "barchart", "Bars", SQL.bars, { grid: g(8, 0, 8, 8) }),
      timeline(3, g(16, 0, 8, 8)),
      panel(4, "stat", "Stat", "SELECT 42 AS value", { grid: g(0, 8, 24, 4) }),
      series,
      heatmap(8, g(0, 18, 24, 6)),
    ]),
    { title: "Host ${host}", collapsed: false, repeat: { variable: "host" }, panels: [
      panel(6, "barchart", "Bars ${host}", HOST_BARS, { grid: g(0, 0, 12, 6) }),
      panel(7, "stat", "Stat ${host}", "SELECT length('${host:csv}') AS value", { grid: g(12, 0, 12, 6) }),
    ] },
  ], {
    variables: [{ kind: "custom", name: "host", label: "Host", hide: "none", options: [{ text: "Alpha", value: "a" }, { text: "Bravo", value: "b" }], multi: true, include_all: true, all_value: null, default: ["$__all"] }],
  });
  return { ...d, annotations: [{ name: "Deploys", source: "prod", sql: MARK, color: "red", panels: [1] }] };
}

/** The number of marks that the time series 1 of the grid draws (the hook `data-annotations`, 9.29). */
const MARKS = `(() => { const e = [...document.querySelectorAll('#app-main [data-testid="panel-1"] .panel__plot')].find((x) => !x.closest('[data-testid="panel-editor"]')); const h = e && e.dataset.annotations; return h ? JSON.parse(h).marks.length : 0; })()`;

// The keys of the frames: the panels, the copy of the panel that repeats and the copies of the section that repeats (9.8c).
const KEYS = ["1", "2", "3", "4", "5", "5-r2", "6", "6-r2", "7", "7-r2", "8"];
const CHARTS = ["1", "2", "3", "5", "5-r2", "6", "6-r2", "8"]; // the frames that draw on a canvas

/** True when the canvas has no size or only transparent pixels (an expression of the page). */
const BLANK = `(c) => { if (!c.width || !c.height) return true; const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data; for (let i = 3; i < d.length; i += 4) if (d[i]) return false; return true; }`;

/** True when each chart frame of the grid has a canvas with pixels: the first draw is done. */
const DRAWN = `(() => { const blank = ${BLANK}; return ${JSON.stringify(CHARTS)}.every((k) => { const e = document.querySelector('#app-main section.panel[data-testid="panel-' + k + '"] .panel__plot'); const c = e && (e.querySelector("canvas.chartCore__canvas") || e.querySelector("canvas")); return Boolean(c) && !blank(c); }); })()`;

// The probe. `window.__ef.base` is the reading of the grid when the probe starts. Each sample compares each frame of the grid with it,
// while the page shows the grid (`view` or `edit`, no panel editor). `hooked` counts the samples at the hook for each phase. The problems
// are kept, without repeats.
const PROBE = `(() => {
  const ids = new WeakMap(); let next = 1;
  const idOf = (e) => { if (!e) return 0; if (!ids.has(e)) ids.set(e, next++); return ids.get(e); };
  const blank = ${BLANK};
  const frames = () => [...document.querySelectorAll('#app-main section.panel[data-testid^="panel-"]')].filter((e) => !e.closest('[data-testid="panel-editor"], [data-testid="view-panel"]') && e.getClientRects().length > 0);
  const read = () => {
    const out = {};
    for (const e of frames()) {
      const plot = e.querySelector(".panel__plot");
      const canvas = plot && (plot.querySelector("canvas.chartCore__canvas") || plot.querySelector("canvas"));
      out[e.dataset.testid.slice(6)] = { state: e.dataset.state, wait: e.classList.contains("panel--wait") || e.classList.contains("panel--progress"), plot: idOf(plot), canvas: idOf(canvas), blank: canvas ? blank(canvas) : null };
    }
    return out;
  };
  const log = (window.__ef = { phase: "start", base: read(), samples: 0, judged: {}, hooked: {}, problems: [], added: [] });
  for (const c of document.querySelectorAll("canvas")) idOf(c); // a canvas that moves with its frame is not new
  const keys = ${JSON.stringify(KEYS)};
  const charts = new Set(${JSON.stringify(CHARTS)});
  const problem = (text) => { if (!log.problems.includes(text) && log.problems.length < 40) log.problems.push(text); };
  const sample = (hook) => {
    log.samples++;
    const view = document.documentElement.getAttribute("data-hune-view");
    if ((view !== "view" && view !== "edit") || document.querySelector('[data-testid="panel-editor"]')) return;
    log.judged[log.phase] = (log.judged[log.phase] || 0) + 1;
    if (hook === true) log.hooked[log.phase] = (log.hooked[log.phase] || 0) + 1;
    const now = read();
    for (const k of keys) {
      const a = log.base[k];
      const b = now[k];
      const at = log.phase + " (" + view + (hook === true ? ", at the hook" : "") + ")";
      if (!b) { problem(at + ": the frame " + k + " is not in the grid"); continue; }
      if (b.plot !== a.plot) problem(at + ": the plot of " + k + " is a new element");
      if (charts.has(k) && b.canvas !== a.canvas) problem(at + ": the canvas of " + k + (b.canvas ? " is a new element" : " is not in the page"));
      if (charts.has(k) && b.blank) problem(at + ": the canvas of " + k + " is blank");
      if (b.state === "idle" || b.state === "queued" || b.state === "loading") problem(at + ": the frame " + k + " has the state " + b.state);
      if (b.wait) problem(at + ": the frame " + k + " shows the skeleton or the dim");
    }
  };
  const channel = new MessageChannel();
  channel.port1.onmessage = sample;
  const tick = () => { if (log.stop) return; channel.port2.postMessage(0); requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
  // The end of the task that writes the hook: another task (a script, or the message of the last frame under load) can run before the next paint.
  new MutationObserver(() => { if (!log.stop) sample(true); }).observe(document.documentElement, { attributes: true, attributeFilter: ["data-hune-view"] });
  // A canvas that the page makes after the start, outside the panel editor, replaces a chart.
  new MutationObserver((records) => {
    for (const r of records) for (const n of r.addedNodes) {
      if (n.nodeType !== 1) continue;
      for (const c of n.tagName === "CANVAS" ? [n] : n.querySelectorAll("canvas")) {
        if (ids.has(c) || c.closest('[data-testid="panel-editor"], [data-testid="view-panel"], dialog')) continue;
        idOf(c);
        const frame = c.closest("section.panel");
        log.added.push(log.phase + ": a new canvas in " + (frame ? frame.dataset.testid : "the page"));
      }
    }
  }).observe(document.body, { childList: true, subtree: true });
})()`;

const view = (p) => p.eval(`document.documentElement.getAttribute("data-hune-view")`);

// The number of frames that the probe samples in each state before the next step.
const FRAMES = 6;

/** Waits for the state `name`, for the end of the work of the page, and for FRAMES painted frames that the probe samples in that state. */
async function waitView(p, name, what) {
  await p.waitFor(`document.documentElement.getAttribute("data-hune-view") === ${JSON.stringify(name)}`, { what: `state ${name} (${what})`, timeout: 20000 });
  await p.idle();
  const from = await p.eval("window.__ef.samples");
  await p.waitFor(`window.__ef.samples >= ${from + FRAMES}`, { what: `${FRAMES} frames in the state ${name} (${what})`, timeout: 20000 });
}

/** The number of marks of the time series 1 once it is `want`, or its last value after 10 s (the case then reports it). */
async function marksNow(p, want) {
  try {
    await p.waitFor(`(${MARKS}) === ${want}`, { what: `${want} marks`, timeout: 10000 });
  } catch {
    // The value that the case reports.
  }
  return p.eval(MARKS);
}

/** One step: the label of the phase, the action, the state that follows. */
async function step(p, phase, act, next) {
  await p.eval(`window.__ef.phase = ${JSON.stringify(phase)}`);
  await act();
  await waitView(p, next, phase);
}

async function panelMenuItem(p, key, item) {
  await p.click(p.tid(`panel-${key}-menu`));
  await p.waitSel(`.menu ${p.tid(`panel-${key}-${item}`)}`);
  await p.click(`.menu ${p.tid(`panel-${key}-${item}`)}`);
}

export const cases = [
  {
    id: "B207",
    name: "the charts stay painted across edit on and off, the panel editor and Back and Forward: the same canvas, never blank, no waiting state, no query, with repeats, in both themes (9.6, 10.2, 10.2a)",
    timeout: 200000,
    async run(t) {
      const r = await t.api.create("transit", flashDoc());
      t.assert(r.status === 201, `create ${UID}: ${r.status} ${r.text.slice(0, 400)}`);
      const found = [];
      for (const scheme of ["dark", "light"]) {
        const p = await t.page({ width: 1280, height: 1000 });
        await p.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 1000, deviceScaleFactor: 1, mobile: false });
        await p.emulateColorScheme(scheme);
        await p.goto(t.dash(UID));
        await p.idle();
        await waitPanelsReady(p, KEYS, "ready");
        await p.waitFor(DRAWN, { what: `${scheme}: the first draw of each chart`, timeout: 20000 });
        await p.eval(PROBE);
        const base = await p.eval("window.__ef.base");
        const missing = KEYS.filter((k) => !base[k] || (CHARTS.includes(k) && (!base[k].canvas || base[k].blank)));
        t.assert(missing.length === 0, `${scheme}: the frames are not all drawn before the switch: ${JSON.stringify(missing)} ${JSON.stringify(base)}`);
        await p.waitFor(`(() => { const e = document.querySelector('[data-testid="panel-1"] .panel__plot'); return e && e.dataset.annotations && JSON.parse(e.dataset.annotations).marks.length === 1; })()`, { what: "the mark on the time series 1" });
        const queries = panelQueries(p).length;
        const click = (id) => () => p.click(p.tid(id));
        // The switch on and off, twice. The mark hides in edit mode and shows again after it (9.19d, rule 22).
        await step(p, "edit-on", click("edit-toggle"), "edit");
        const marksEdit = await marksNow(p, 0);
        await step(p, "edit-off", click("edit-toggle"), "view");
        const marksView = await marksNow(p, 1);
        if (marksEdit !== 0 || marksView !== 1) found.push(`${scheme}: the marks of the time series 1 are ${marksEdit} in edit mode and ${marksView} after it, expected 0 and 1`);
        await step(p, "edit-on 2", click("edit-toggle"), "edit");
        // The panel editor from the edit mode, and the return to the edit mode.
        await step(p, "panel editor from edit", () => panelMenuItem(p, "1", "edit"), "panel-edit");
        await step(p, "leave to edit", click("editor-back"), "edit");
        await step(p, "edit-off 2", click("edit-toggle"), "view");
        // The panel editor from the view (origin view), and the return to the view.
        await step(p, "panel editor from view", () => panelMenuItem(p, "2", "edit"), "panel-edit");
        await step(p, "leave to view", click("editor-back"), "view");
        // Back and Forward over the view, the edit mode and the panel editor.
        await step(p, "edit-on 3", click("edit-toggle"), "edit");
        await step(p, "panel editor 3", () => panelMenuItem(p, "3", "edit"), "panel-edit");
        await step(p, "back to edit", () => p.eval("history.back()"), "edit");
        await step(p, "back to view", () => p.eval("history.back()"), "view");
        await step(p, "forward to edit", () => p.eval("history.forward()"), "edit");
        await step(p, "forward to panel editor", () => p.eval("history.forward()"), "panel-edit");
        await step(p, "back to edit 2", () => p.eval("history.back()"), "edit");
        await step(p, "back to view 2", () => p.eval("history.back()"), "view");
        const log = await p.eval("(() => { window.__ef.stop = true; return { problems: window.__ef.problems, added: window.__ef.added, judged: window.__ef.judged, hooked: window.__ef.hooked, samples: window.__ef.samples }; })()");
        // Each step that ends in the grid has samples after the paint and a sample at the hook.
        const phases = ["edit-on", "edit-off", "edit-on 2", "leave to edit", "edit-off 2", "leave to view", "edit-on 3", "back to edit", "back to view", "forward to edit", "back to edit 2", "back to view 2"];
        const thin = phases.filter((ph) => !(log.judged[ph] >= 5));
        if (thin.length) found.push(`${scheme}: too few samples of the grid in ${JSON.stringify(thin)}: ${JSON.stringify(log.judged)}`);
        const unhooked = phases.filter((ph) => !(log.hooked[ph] >= 1));
        if (unhooked.length) found.push(`${scheme}: no sample at the hook in ${JSON.stringify(unhooked)}: ${JSON.stringify(log.hooked)}`);
        found.push(...log.problems.slice(0, 12).map((x) => `${scheme}: ${x}`));
        found.push(...[...new Set(log.added)].slice(0, 6).map((x) => `${scheme}: ${x}`));
        const extra = panelQueries(p).length - queries;
        if (extra) found.push(`${scheme}: ${extra} panel queries during the walk, expected none (no data changed)`);
        t.eq(await view(p), "view", `${scheme}: the walk ends in the view`);
      }
      t.eq(found, [], "flash");
    },
  },
];
