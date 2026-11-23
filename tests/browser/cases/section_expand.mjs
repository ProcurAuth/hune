// Browser case B210 (9.8, 10.2a rule 17): the charts of a section draw in the task that expands it (owner answer of 2026-11-22, ID-K325).
// A collapsed section is out of the layout, so the chart engine releases its canvases. Before the fix the charts drew again only in the
// resize callback of the next rendering step: a task between the expand and that callback read blank canvases. The probe samples each frame
// just after its paint (requestAnimationFrame, then a message), at the end of the task of each click on a section toggle (a click listener
// on the window runs after the listener of the toggle), and in the next task (a message sent at that end). The steps wait on states (the
// attribute aria-expanded, the frames that the probe samples, the hook data-hune-state), never for a fixed time.
// Collapse then expand, for a section, for the copies 1 and 2 of a section that repeats, and in edit mode: each frame of an expanded section
// keeps its plot and its canvas (the same elements), each canvas has pixels, no frame shows a waiting state, and no panel query goes out.
// A section that is collapsed at the load was never drawn: its first expand shows the empty frames, sends one query for each panel, and
// then its charts draw. Two themes, five chart types (time series, bar chart, state timeline, pie chart, heatmap) and a stat.
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { panelIdOf, panelQueries, waitPanelsReady } from "../lib/h.mjs";
import { flashDoc } from "./edit_flash.mjs";

const UID = "sectionexpand210";
const g = (x, y, w, h) => ({ x, y, w, h });

// The state timeline and the heatmap of B207 (their builders), with new ids and places.
const kinds = flashDoc().sections[0].panels;
const like = (id, type, grid) => ({ ...structuredClone(kinds.find((p) => p.type === type)), id, grid });

const slices = { options: { reduce: { calc: "last_not_null", fields: null, values: true } } };
const HOST_BARS = ["SELECT node, sum(blocks) AS blocks", "FROM analytics.seed", "WHERE '${host:csv}' != ''", "GROUP BY node", "ORDER BY node"];

export function expandDoc() {
  return doc(UID, "Section expand", [
    section("Charts", [
      panel(1, "timeseries", "Series", SQL.series, { grid: g(0, 0, 8, 6) }),
      panel(2, "barchart", "Bars", SQL.bars, { grid: g(8, 0, 8, 6) }),
      like(3, "state_timeline", g(16, 0, 8, 6)),
      panel(4, "piechart", "Pie", SQL.bars, { grid: g(0, 6, 8, 6), ...slices }),
      like(5, "heatmap", g(8, 6, 8, 6)),
      panel(6, "stat", "Stat", "SELECT 42 AS value", { grid: g(16, 6, 8, 6) }),
    ]),
    { title: "Host ${host}", collapsed: false, repeat: { variable: "host" }, panels: [
      panel(7, "barchart", "Bars ${host}", HOST_BARS, { grid: g(0, 0, 12, 5) }),
      panel(8, "piechart", "Pie ${host}", HOST_BARS, { grid: g(12, 0, 12, 5), ...slices }),
    ] },
    section("Later", [
      panel(9, "timeseries", "Later series", SQL.series, { grid: g(0, 0, 12, 5) }),
      like(10, "heatmap", g(12, 0, 12, 5)),
    ], true),
  ], {
    variables: [{ kind: "custom", name: "host", label: "Host", hide: "none", options: [{ text: "Alpha", value: "a" }, { text: "Bravo", value: "b" }], multi: true, include_all: true, all_value: null, default: ["$__all"] }],
  });
}

// The section copy of each frame (9.8c), and the frames that draw on a canvas.
const SECTION = { 1: "section-0", 2: "section-0", 3: "section-0", 4: "section-0", 5: "section-0", 6: "section-0", 7: "section-1", 8: "section-1", "7-r2": "section-1-r2", "8-r2": "section-1-r2", 9: "section-2", 10: "section-2" };
const SHOWN = ["1", "2", "3", "4", "5", "6", "7", "7-r2", "8", "8-r2"]; // expanded at the load
const LATER = ["9", "10"]; // collapsed at the load
const CHARTS = ["1", "2", "3", "4", "5", "7", "7-r2", "8", "8-r2", "9", "10"];

/** True when the canvas has no size, a bitmap less than half of its box (a released canvas that the CSS stretches), or only transparent
 * pixels (an expression of the page). */
const BLANK = `(c) => { const r = devicePixelRatio; if (!c.width || !c.height || c.width * 2 < c.clientWidth * r || c.height * 2 < c.clientHeight * r) return true; const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data; for (let i = 3; i < d.length; i += 4) if (d[i]) return false; return true; }`;

/** The visible element of a selector: the edit mode keeps the base grid out of the layout. */
const VISIBLE = `(s) => [...document.querySelectorAll(s)].find((e) => e.getClientRects().length > 0) || null`;

/** True when each chart frame of `keys` has a canvas with pixels: the first draw is done. */
const drawn = (keys) => `(() => { const blank = ${BLANK}; const vis = ${VISIBLE}; return ${JSON.stringify(keys.filter((k) => CHARTS.includes(k)))}.every((k) => { const e = vis('#app-main section.panel[data-testid="panel-' + k + '"] .panel__plot'); const c = e && (e.querySelector("canvas.chartCore__canvas") || e.querySelector("canvas")); return Boolean(c) && !blank(c); }); })()`;

// The probe. `window.__sx.base` is the reading of the frames when the probe starts (`rebase` adds frames). A frame whose section is
// collapsed is not judged. `ended` and `next` count the samples at the end of the task of a toggle click and in the next task.
const PROBE = `(() => {
  const ids = new WeakMap(); let next = 1;
  const idOf = (e) => { if (!e) return 0; if (!ids.has(e)) ids.set(e, next++); return ids.get(e); };
  const blank = ${BLANK};
  const vis = ${VISIBLE};
  const sections = ${JSON.stringify(SECTION)};
  const charts = new Set(${JSON.stringify(CHARTS)});
  const read1 = (k) => {
    const e = vis('#app-main section.panel[data-testid="panel-' + k + '"]');
    if (!e) return null;
    const plot = e.querySelector(".panel__plot");
    const canvas = plot && (plot.querySelector("canvas.chartCore__canvas") || plot.querySelector("canvas"));
    return { state: e.dataset.state, wait: e.classList.contains("panel--wait") || e.classList.contains("panel--progress"), plot: idOf(plot), canvas: idOf(canvas), blank: canvas ? blank(canvas) : null };
  };
  const open = (k) => { const t = vis('[data-testid="' + sections[k] + '"] > .section__head .section__toggle'); return Boolean(t) && t.getAttribute("aria-expanded") === "true"; };
  const log = (window.__sx = { phase: "start", keys: ${JSON.stringify(SHOWN)}, base: {}, samples: 0, judged: {}, ended: {}, next: {}, first: null, problems: [], added: [] });
  log.rebase = (keys) => { for (const k of keys) { log.base[k] = read1(k); if (!log.keys.includes(k)) log.keys.push(k); } };
  log.rebase(log.keys);
  for (const c of document.querySelectorAll("canvas")) idOf(c); // a canvas that moves with its frame is not new
  const problem = (text) => { if (!log.problems.includes(text) && log.problems.length < 200) log.problems.push(text); };
  const count = (m) => { m[log.phase] = (m[log.phase] || 0) + 1; };
  const sample = (kind) => {
    log.samples++;
    const view = document.documentElement.getAttribute("data-hune-view");
    if ((view !== "view" && view !== "edit") || document.querySelector('[data-testid="panel-editor"]')) return;
    count(log.judged);
    if (kind === "end") count(log.ended);
    if (kind === "next") count(log.next);
    if (kind === "end" && log.phase === "first expand Later") log.first = Object.fromEntries(${JSON.stringify(LATER)}.map((k) => [k, read1(k)]));
    for (const k of log.keys) {
      if (!open(k)) continue;
      const a = log.base[k];
      const b = read1(k);
      const at = log.phase + " (" + view + (kind === "end" ? ", at the end of the task" : kind === "next" ? ", in the next task" : "") + ")";
      if (!b) { problem(at + ": the frame " + k + " is not in the grid"); continue; }
      if (b.plot !== a.plot) problem(at + ": the plot of " + k + " is a new element");
      if (charts.has(k) && b.canvas !== a.canvas) problem(at + ": the canvas of " + k + (b.canvas ? " is a new element" : " is not in the page"));
      if (charts.has(k) && b.blank) problem(at + ": the canvas of " + k + " is blank");
      if (b.state === "idle" || b.state === "queued" || b.state === "loading") problem(at + ": the frame " + k + " has the state " + b.state);
      if (b.wait) problem(at + ": the frame " + k + " shows the skeleton or the dim");
    }
  };
  const channel = new MessageChannel();
  channel.port1.onmessage = (e) => sample(e.data);
  const tick = () => { if (log.stop) return; channel.port2.postMessage("frame"); requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
  // The listener of the toggle runs first: this one reads at the end of the task of the expand, and sends a message for the next task.
  window.addEventListener("click", (e) => {
    if (log.stop || !e.target.closest || !e.target.closest(".section__toggle")) return;
    sample("end");
    channel.port2.postMessage("next");
  });
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

// The number of frames that the probe samples after each step.
const FRAMES = 6;
const toggleOf = (sec) => `#app-main [data-testid="${sec}"] > .section__head .section__toggle`;
const expandedOf = (sec) => `(() => { const t = (${VISIBLE})(${JSON.stringify(toggleOf(sec))}); return t ? t.getAttribute("aria-expanded") : null; })()`;

/** Waits for the end of the work of the page and for FRAMES painted frames that the probe samples. */
async function settled(p, what) {
  await p.idle();
  const from = await p.eval("window.__sx.samples");
  await p.waitFor(`window.__sx.samples >= ${from + FRAMES}`, { what: `${FRAMES} frames after ${what}`, timeout: 20000 });
}

/** One click on the toggle of the section copy `sec`: the phase label, the click, the state of aria-expanded that follows. */
async function toggle(p, phase, sec, want) {
  await p.eval(`window.__sx.phase = ${JSON.stringify(phase)}`);
  const click = await p.eval(`(() => { const t = (${VISIBLE})(${JSON.stringify(toggleOf(sec))}); if (!t) return false; t.setAttribute("data-b210", "1"); return true; })()`);
  if (!click) throw new Error(`${phase}: no visible toggle of ${sec}`);
  await p.click(`#app-main [data-b210="1"]`);
  await p.eval(`document.querySelector('[data-b210="1"]') && document.querySelector('[data-b210="1"]').removeAttribute("data-b210")`);
  await p.waitFor(`${expandedOf(sec)} === ${JSON.stringify(String(want))}`, { what: `${phase}: aria-expanded ${want}`, timeout: 20000 });
  await settled(p, phase);
  // After a collapse: the number of chart canvases of the section that the engine released (no pixels). The case writes it as a note.
  const keys = CHARTS.filter((k) => SECTION[k] === sec);
  return want ? null : p.eval(`(() => { const blank = ${BLANK}; return ${JSON.stringify(keys)}.filter((k) => [...document.querySelectorAll('#app-main section.panel[data-testid="panel-' + k + '"] .panel__plot canvas')].some((c) => c.width <= 1 || blank(c))).length + " of ${keys.length}"; })()`);
}

export const cases = [
  {
    id: "B210",
    name: "the charts of a section draw in the task that expands it: collapse then expand keeps each canvas, never blank at the end of the task or in the next task, no waiting state, no query, with a section that repeats and in edit mode; a first expand queries, in both themes (9.8, 10.2a)",
    timeout: 200000,
    async run(t) {
      const r = await t.api.create("transit", expandDoc());
      t.assert(r.status === 201, `create ${UID}: ${r.status} ${r.text.slice(0, 400)}`);
      const found = [];
      for (const scheme of ["dark", "light"]) {
        const p = await t.page({ width: 1280, height: 1000 });
        await p.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 1000, deviceScaleFactor: 1, mobile: false });
        await p.emulateColorScheme(scheme);
        await p.goto(t.dash(UID));
        await p.idle();
        await waitPanelsReady(p, SHOWN, "ready");
        await p.waitFor(drawn(SHOWN), { what: `${scheme}: the first draw of each chart`, timeout: 20000 });
        await p.eval(PROBE);
        const base = await p.eval("window.__sx.base");
        const missing = SHOWN.filter((k) => !base[k] || (CHARTS.includes(k) && (!base[k].canvas || base[k].blank)));
        t.assert(missing.length === 0, `${scheme}: the frames are not all drawn before the first collapse: ${JSON.stringify(missing)} ${JSON.stringify(base)}`);
        // The section "Later" is collapsed at the load: its frames wait and send no query (9.8).
        const later = (ids) => panelQueries(p).filter((q) => ids.includes(panelIdOf(q))).length;
        const laterStates = await p.eval(`${JSON.stringify(LATER)}.map((k) => document.querySelector('[data-testid="panel-' + k + '"]').dataset.state)`);
        if (later([9, 10]) !== 0 || laterStates.some((s) => s !== "idle")) found.push(`${scheme}: the collapsed section sent ${later([9, 10])} queries and has the states ${JSON.stringify(laterStates)}, expected 0 and idle`);
        const queries = panelQueries(p).length;
        // Collapse then expand: a section, the copy 2 and the copy 1 of a section that repeats, and the first section again.
        const cycles = [["Charts", "section-0"], ["Host copy 2", "section-1-r2"], ["Host copy 1", "section-1"], ["Charts 2", "section-0"]];
        const released = {};
        for (const [name, sec] of cycles) {
          released[name] = await toggle(p, `collapse ${name}`, sec, false);
          await toggle(p, `expand ${name}`, sec, true);
        }
        const steady = panelQueries(p).length - queries;
        if (steady) found.push(`${scheme}: ${steady} panel queries for collapse then expand, expected none (no data changed)`);
        // The first expand of the section "Later": the empty frames, one query for each panel, then the charts draw.
        await toggle(p, "first expand Later", "section-2", true);
        await waitPanelsReady(p, LATER, "ready");
        await p.waitFor(drawn(LATER), { what: `${scheme}: the first draw of the section Later`, timeout: 20000 });
        const first = await p.eval("window.__sx.first");
        const empty = first && LATER.every((k) => first[k] && ["idle", "queued", "loading"].includes(first[k].state));
        t.note(`${scheme}: the frames of Later at the end of the task of the first expand: ${JSON.stringify(first && Object.fromEntries(LATER.map((k) => [k, first[k] && first[k].state])))}`);
        if (!empty) found.push(`${scheme}: at the end of the task of the first expand, the frames of Later are ${JSON.stringify(first)}, expected the empty frame (idle, queued or loading)`);
        if (later([9]) !== 1 || later([10]) !== 1) found.push(`${scheme}: the first expand sent ${later([9])} and ${later([10])} queries for the panels 9 and 10, expected 1 and 1`);
        await p.eval(`window.__sx.rebase(${JSON.stringify(LATER)})`);
        const atFirst = panelQueries(p).length;
        released.Later = await toggle(p, "collapse Later", "section-2", false);
        await toggle(p, "expand Later", "section-2", true);
        // In edit mode a collapse changes the draft (10.2): the draft grid takes the frames again, with their charts.
        await p.eval(`window.__sx.phase = "edit-on"`);
        await p.click(p.tid("edit-toggle"));
        await p.waitFor(`document.documentElement.getAttribute("data-hune-view") === "edit"`, { what: `${scheme}: edit mode` });
        await settled(p, "edit-on");
        released["Charts in edit"] = await toggle(p, "collapse Charts in edit", "section-0", false);
        await toggle(p, "expand Charts in edit", "section-0", true);
        released["Host copy 2 in edit"] = await toggle(p, "collapse Host copy 2 in edit", "section-1-r2", false);
        await toggle(p, "expand Host copy 2 in edit", "section-1-r2", true);
        await p.eval(`window.__sx.phase = "edit-off"`);
        await p.click(p.tid("edit-toggle"));
        await p.waitFor(`document.documentElement.getAttribute("data-hune-view") === "view"`, { what: `${scheme}: the view after edit mode` });
        await settled(p, "edit-off");
        const extra = panelQueries(p).length - atFirst;
        if (extra) found.push(`${scheme}: ${extra} panel queries after the first expand, expected none (no data changed)`);
        const log = await p.eval("(() => { window.__sx.stop = true; return { problems: window.__sx.problems, added: window.__sx.added, judged: window.__sx.judged, ended: window.__sx.ended, next: window.__sx.next }; })()");
        // Each expand has samples after the paint, one at the end of its task and one in the next task.
        const expands = [...cycles.map(([name]) => `expand ${name}`), "expand Later", "expand Charts in edit", "expand Host copy 2 in edit"];
        const thin = expands.filter((ph) => !(log.judged[ph] >= 5));
        if (thin.length) found.push(`${scheme}: too few samples in ${JSON.stringify(thin)}: ${JSON.stringify(log.judged)}`);
        const unsampled = expands.filter((ph) => !(log.ended[ph] >= 1 && log.next[ph] >= 1));
        if (unsampled.length) found.push(`${scheme}: no sample at the end of the task or in the next task in ${JSON.stringify(unsampled)}: ${JSON.stringify([log.ended, log.next])}`);
        // One line for each phase and place of the sample, with the problems of the frames.
        const groups = new Map();
        for (const x of log.problems) {
          const at = x.slice(0, x.indexOf("): ") + 1);
          groups.set(at, [...(groups.get(at) || []), x.slice(at.length + 2)]);
        }
        found.push(...[...groups].slice(0, 24).map(([at, list]) => `${scheme}: ${at}: ${list.join("; ")}`));
        // The first expand makes the charts of the section "Later": these canvases are new.
        found.push(...[...new Set(log.added.filter((x) => !x.startsWith("first expand Later:")))].slice(0, 6).map((x) => `${scheme}: ${x}`));
        t.note(`${scheme}: canvases released by each collapse: ${JSON.stringify(released)}`);
      }
      t.eq(found, [], "expand");
    },
  },
];
