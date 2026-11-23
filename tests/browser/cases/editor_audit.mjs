// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B152 and B153 (4.13, 9.8, 10.3, 15.6): the switch "Transparent background" and the audit of the options of the panel editor.
// B152: the switch changes the frame. For each of the 10 panel types, in the editor and in the saved view, in both themes, the computed
//       background and edge of the frame are opaque before and transparent after the switch. The title stays readable on the page.
//       The states error, empty and loading of a transparent panel stay readable. A panel that was not changed keeps its fill.
// B153: each option of the panel editor that applies to the type changes the draft and the preview (the table AUDIT of the case).
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { doc, panel, section, SQL } from "../lib/docs.mjs";
import { ROOT } from "../lib/env.mjs";
import * as A from "../lib/audit_a.mjs";
import * as B from "../lib/audit_b.mjs";
import * as C from "../lib/audit_c.mjs";
import { changedMembers, judge, membersWithoutControl, mergeExempt } from "../lib/audit_rules.mjs";
import { enterEdit, openPanelEditor, saveDialog, waitDraft } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const g = (x, y, w, h) => ({ x, y, w, h });
const legend = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const TIMELINE_OPTIONS = {
  state_timeline: { merge_values: true, show_values: "auto", align_values: "left", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend: legend(), tooltip: { mode: "single", sort: "none" } },
  status_history: { show_values: "auto", row_height: 90, col_width: 90, line_width: 1, fill_opacity: 70, show_labels: true, legend: legend(), tooltip: { mode: "single", sort: "none" } },
};
// The times are in the seed range of the runner (1794211200 s, 1200 s), so that the panel has data under the range of the page.
// Two rows, and a null in the first one: the row height, the connection over missing values and the legend then show.
const TL_SQL = "SELECT toDateTime(1794211200,'UTC') + number*40 AS time, if(number % 7 = 3, NULL, intDiv(number,3)%3) AS a, intDiv(number,5)%3 AS b FROM numbers(30)";
const TL_MAP = [{ kind: "value", value: "0", text: "OK", color: "green" }, { kind: "value", value: "1", text: "WARN", color: "yellow" }, { kind: "value", value: "2", text: "FAIL", color: "red" }];

/** A panel of a timeline type. */
const timeline = (id, type, title) => ({
  id, type, title, description: "", transparent: false, grid: g(0, 0, 12, 6),
  data: { query: { source: "prod", format: "table", sql: [TL_SQL], min_interval: null, max_data_points: null } },
  transforms: [], fields: { defaults: { mappings: TL_MAP }, overrides: [] }, options: TIMELINE_OPTIONS[type], links: [],
});
/** A logs panel (4.33f): five lines at five times, so that the order, the time and the levels show. */
const LOGS_SQL = "SELECT toDateTime64(1794211200 + number*60, 3, 'UTC') AS timestamp, concat('line ', toString(number)) AS body, ['info','error','warn'][number % 3 + 1] AS level, 'h1' AS host FROM numbers(5)";
const logs = (id) => ({ ...panel(id, "table", "Logs", LOGS_SQL, { grid: g(0, 18, 12, 6) }), type: "logs", options: {}, data: { query: { source: "prod", format: "logs", sql: [LOGS_SQL], min_interval: null, max_data_points: null } } });
/** A text panel (no query). */
const text = (id, title) => ({ id, type: "text", title, description: "", transparent: false, grid: g(0, 0, 12, 6), options: { mode: "markdown", content: ["Some text"] }, links: [] });

// The ten types, then one panel that the case never changes (the control), and three panels in a state.
const TYPES = [
  ["timeseries", (id) => panel(id, "timeseries", "Series", SQL.series, { grid: g(0, 0, 12, 6) })],
  ["stat", (id) => panel(id, "stat", "Stat", "SELECT 50 AS a, 150 AS b, CAST(NULL AS Nullable(Float64)) AS c", { grid: g(12, 0, 6, 6) })],
  ["bargauge", (id) => panel(id, "bargauge", "Bar gauge", SQL.bars, { grid: g(18, 0, 6, 6), fields: { defaults: { min: 0, max: 400 }, overrides: [] } })],
  ["piechart", (id) => panel(id, "piechart", "Pie chart", SQL.bars, { grid: g(0, 6, 8, 6) })],
  ["barchart", (id) => panel(id, "barchart", "Bar chart", SQL.bars, { grid: g(8, 6, 8, 6) })],
  ["xychart", (id) => panel(id, "xychart", "XY chart", SQL.xy, { grid: g(16, 6, 8, 6) })],
  ["table", (id) => panel(id, "table", "Table", ["SELECT node, weight, blocks, active, CAST(NULL AS Nullable(String)) AS note", "FROM analytics.seed", "ORDER BY event_time"], { grid: g(0, 12, 12, 6) })],
  ["text", (id) => text(id, "Text")],
  ["state_timeline", (id) => timeline(id, "state_timeline", "State timeline")],
  ["status_history", (id) => timeline(id, "status_history", "Status history")],
  ["gauge", (id) => ({ ...panel(id, "stat", "Gauge", "SELECT 50 AS a, 150 AS b", { grid: g(12, 12, 12, 6) }), type: "gauge", options: { reduce: { calc: "last_not_null", fields: null, values: false } } })],
  ["logs", (id) => logs(id)],
];
// The id of the control is apart from the ids of the types, so that a new type takes the next id (logs, F33).
const CONTROL = 99;
const TS_SQL = "SELECT toDateTime(1794211200,'UTC') + number*40 AS time, 150 + number AS a, if(number % 7 = 3, NULL, 40 + number) AS b, 85 + number AS c FROM numbers(30)";
// The data of the audit for the types whose options need more than the data of the other cases: two series in a bar chart (a group has two bars), three
// numbers in an XY chart (a choice of the y fields), several rows and a time column in a stat (a calculation, a graph), several rows in a pie.
const AUDIT_SQL = {
  timeseries: [TS_SQL],
  // Three series in the order middle, low, high: the sort of a card (ascending and descending) differs from the order of the series, and the thresholds
  // (the step at 80) have a base and a step in the picture.
  barchart: ["SELECT node, sum(blocks) % 20 + 60 AS mid, sum(blocks) % 20 + 5 AS low, sum(weight) % 40 + 150 AS high", "FROM analytics.seed", "GROUP BY node", "ORDER BY node"],
  xychart: ["SELECT blocks AS x, toFloat64(weight % 20 + 60) AS y, toFloat64(weight % 20 + 5) AS z, toFloat64(weight % 40 + 150) AS w", "FROM analytics.seed", "ORDER BY blocks"],
  stat: ["SELECT toDateTime(1794211200,'UTC') + number*60 AS time, 10 + number*20 AS a, 150 - number*10 AS b, CAST(NULL AS Nullable(Float64)) AS c FROM numbers(10)"],
  piechart: ["SELECT node, blocks, weight, 1 AS n, 150 AS big", "FROM analytics.seed", "ORDER BY event_time"],
  // Two gauges (a title shows, so the title size acts) on the scale 10 to 190 (the arc, the band and the labels show).
  gauge: ["SELECT 10 + number * 20 AS a, 190 - number * 10 AS b FROM numbers(10)"],
};
// The panels of the states (a document of their own: the error panel makes the console report a failed request on purpose).
const ERROR = 1;
const EMPTY = 2;
const LOADING = 3;

function transparentDoc(uid) {
  const panels = TYPES.map(([, make], i) => make(i + 1));
  panels.push(panel(CONTROL, "stat", "Control", SQL.total));
  // Two columns of panels, 6 rows high each, so that no frame overlaps another one.
  panels.forEach((p, i) => { p.grid = g((i % 2) * 12, Math.floor(i / 2) * 6, 12, 6); });
  return doc(uid, `Transparent ${uid}`, [section(null, panels)], { time: { from: "now-6h", to: "now", timezone: "browser", refresh: null } });
}

function stateDoc(uid) {
  const panels = [
    panel(ERROR, "table", "Error", "SELECT nope FROM analytics.seed", { transparent: true, grid: g(0, 0, 12, 6) }),
    panel(EMPTY, "table", "Empty", "SELECT node FROM analytics.seed WHERE 0", { transparent: true, grid: g(12, 0, 12, 6) }),
    panel(LOADING, "stat", "Loading", "SELECT sleep(2) AS value", { transparent: true, grid: g(0, 6, 12, 6) }),
  ];
  return doc(uid, `Transparent ${uid}`, [section(null, panels)], { time: { from: "now-6h", to: "now", timezone: "browser", refresh: null } });
}

// The page measures. `RGBA` turns any computed colour into [r, g, b, a] through a canvas. `PAGE_BG` is the first opaque background above an element.
const PAGE = `
  const RGBA = (c) => { const k = document.createElement("canvas"); k.width = k.height = 1; const x = k.getContext("2d", { willReadFrequently: true }); x.clearRect(0, 0, 1, 1); x.fillStyle = "#000"; x.fillStyle = c; x.fillRect(0, 0, 1, 1); const d = x.getImageData(0, 0, 1, 1).data; return [d[0], d[1], d[2], d[3] / 255]; };
  const OVER = (f, b) => [0, 1, 2].map((i) => f[i] * f[3] + b[i] * (1 - f[3]));
  const PAGE_BG = (e) => { for (let n = e; n; n = n.parentElement) { const c = RGBA(getComputedStyle(n).backgroundColor); if (c[3] > 0.99) return c; } return RGBA(getComputedStyle(document.documentElement).backgroundColor); };
  const LUM = (c) => { const f = c.map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }); return 0.2126 * f[0] + 0.7152 * f[1] + 0.0722 * f[2]; };
  const CONTRAST = (a, b) => { const x = LUM(a), y = LUM(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
  const FRAME = (e) => { const cs = getComputedStyle(e); return { bg: RGBA(cs.backgroundColor)[3], edge: cs.borderTopWidth === "0px" ? 0 : RGBA(cs.borderTopColor)[3], shadow: cs.boxShadow, attr: e.hasAttribute("data-transparent") }; };
  const TEXT_OF = (e) => { const c = RGBA(getComputedStyle(e).color); const bg = PAGE_BG(e); return { color: c, bg, ratio: CONTRAST(OVER(c.concat(), bg), bg) }; };
`;

/** The frame, the title and the head of the element that `sel` selects. */
const measure = (p, sel) => p.eval(`(() => { ${PAGE}
  const e = document.querySelector(${JSON.stringify(sel)});
  if (!e) return null;
  const t = e.querySelector(".panel__title");
  const head = e.querySelector(".panel__head");
  const o = e.querySelector(".panel__overlay");
  return { frame: FRAME(e), title: t && t.textContent.trim() ? TEXT_OF(t).ratio : null, head: head ? RGBA(getComputedStyle(head).backgroundColor)[3] : null, overlay: o ? RGBA(getComputedStyle(o).backgroundColor)[3] : null };
})()`);

/** Move the pointer to the corner of the window, away from every frame. */
const away = async (p) => {
  await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 2, y: 2 });
  await sleep(80);
};

const frameOf = (m) => `${m ? JSON.stringify(m.frame) : "none"}`;

/** Wait until the top frame of the editor has left the states that wait. */
const editorSettled = (p) => p.waitFor(`(() => { const e = document.querySelector(".peditor__top .panel"); return e && !["idle", "queued", "loading"].includes(e.dataset.state); })()`, { what: "the preview" });

async function session(t, scheme, uid) {
  const r = await t.api.create("transit", transparentDoc(uid));
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 300)}`);
  const p = await t.page({ width: 1400, height: 900 });
  await p.emulateColorScheme(scheme);
  await p.goto(t.dash(uid));
  await p.idle();
  await enterEdit(p);

  // 1. The editor: for each type, the preview frame is opaque, then transparent after the switch, then opaque again, then transparent (kept for the save).
  for (let i = 0; i < TYPES.length; i++) {
    const [type] = TYPES[i];
    const id = i + 1;
    const label = `${scheme} ${type}`;
    await openPanelEditor(p, id);
    await editorSettled(p);
    await away(p);
    const sel = ".peditor__top .panel";
    const off = await measure(p, sel);
    t.assert(off && off.frame.bg > 0.99 && off.frame.edge > 0.01 && !off.frame.attr, `${label}: the preview has the fill and the edge before the switch: ${frameOf(off)}`);
    await p.click(p.tid("panel-transparent"));
    await p.waitFor(`document.querySelector(".peditor__top .panel").hasAttribute("data-transparent")`, { what: `${label} the preview attribute` });
    await away(p);
    const on = await measure(p, sel);
    t.assert(on.frame.bg === 0 && on.frame.edge === 0 && on.frame.attr, `${label}: the preview has no fill and no edge after the switch: ${frameOf(on)}`);
    t.assert(on.title === null || on.title >= 4.5, `${label}: the title of the transparent preview has the contrast 4.5 on the page: ${on.title}`);
    t.assert(on.head === 0, `${label}: the title bar of the transparent preview has no fill: ${on.head}`);
    const d1 = await waitDraft(p, uid, (d) => JSON.stringify(d).includes('"transparent":true'));
    t.assert(JSON.stringify(d1).includes('"transparent":true'), `${label}: the draft has transparent = true`);
    await p.click(p.tid("panel-transparent"));
    await p.waitFor(`!document.querySelector(".peditor__top .panel").hasAttribute("data-transparent")`, { what: `${label} the attribute leaves` });
    await away(p);
    const back = await measure(p, sel);
    t.assert(back.frame.bg > 0.99 && back.frame.edge > 0.01, `${label}: the preview has the fill and the edge again after the second switch: ${frameOf(back)}`);
    await p.click(p.tid("panel-transparent"));
    await p.waitFor(`document.querySelector(".peditor__top .panel").hasAttribute("data-transparent")`, { what: `${label} the attribute again` });
    // The edge shows on hover, as in the view (9.8).
    await p.hover(".peditor__top .panel");
    const hov = await measure(p, sel);
    t.assert(hov.frame.edge > 0.01, `${label}: the edge of a transparent preview shows on hover: ${frameOf(hov)}`);
    await p.click(p.tid("editor-back"));
    await p.waitGone(p.tid("panel-editor"));
    await p.idle();
  }
  await saveDialog(p, "transparent");
  await p.waitFor(`!document.querySelector('[data-testid="edit-bar"]') || document.querySelector('[data-testid="notice"]')`, { what: "the save" }).catch(() => {});
  await sleep(600);
  const saved = await t.api.dashboard(uid);
  const flags = saved.json.document.sections[0].panels.map((x) => [x.id, x.transparent]);
  t.eq(flags, [...TYPES.map((_, i) => [i + 1, true]), [CONTROL, false]], `${scheme}: the saved document has transparent on the ten panels that the case changed`);
  return p;
}

/** The saved view: the ten panels are transparent, the control is not. */
async function viewChecks(t, scheme, uid) {
  const p = await t.page({ width: 1400, height: 1200 });
  await p.emulateColorScheme(scheme);
  await p.goto(t.dash(uid));
  await p.idle();
  await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 7, 9, 10, 11, CONTROL]).catch(() => {});
  await away(p);
  for (let i = 0; i < TYPES.length; i++) {
    const [type] = TYPES[i];
    const sel = `[data-testid="panel-${i + 1}"]`;
    await p.eval(`document.querySelector(${JSON.stringify(sel)}).scrollIntoView({ block: "center" })`);
    await away(p);
    await sleep(120);
    const m = await measure(p, sel);
    t.assert(m && m.frame.bg === 0 && m.frame.edge === 0 && m.frame.attr, `${scheme} ${type}: the view has no fill and no edge: ${frameOf(m)}`);
    t.assert(m.title === null || m.title >= 4.5, `${scheme} ${type}: the title has the contrast 4.5 on the page: ${m.title}`);
    t.assert(m.head === 0, `${scheme} ${type}: the title bar has no fill: ${m.head}`);
    await p.hover(sel, 0.5, 0.02);
    const hov = await measure(p, sel);
    t.assert(hov.frame.edge > 0.01, `${scheme} ${type}: the edge shows on hover: ${frameOf(hov)}`);
  }
  await p.eval(`document.querySelector('[data-testid="panel-${CONTROL}"]').scrollIntoView({ block: "center" })`);
  await away(p);
  const ctl = await measure(p, `[data-testid="panel-${CONTROL}"]`);
  t.assert(ctl.frame.bg > 0.99 && ctl.frame.edge > 0.01 && !ctl.frame.attr, `${scheme}: the panel that the case did not change keeps its fill and its edge: ${frameOf(ctl)}`);
  return p;
}

/** The states of a transparent panel stay readable: the text has the contrast 4.5 on the page. */
async function stateChecks(t, scheme, uid) {
  const r = await t.api.create("transit", stateDoc(uid));
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 300)}`);
  const p = await t.page({ width: 1400, height: 1200 });
  t.allow(p, /Failed to load resource/);
  await p.emulateColorScheme(scheme);
  await p.goto(t.dash(uid));
  // The loading state first: the query of the panel `Loading` lasts 2 s.
  await p.waitFor(`(() => { const e = document.querySelector('[data-testid="panel-${LOADING}"]'); return e && e.dataset.state === "loading"; })()`, { what: "the loading state" });
  await sleep(450);
  const texts = `(() => { ${PAGE}
    const e = document.querySelector('[data-testid="panel-${LOADING}"]');
    const f = FRAME(e);
    const w = e.querySelector(".panel__title");
    return { state: e.dataset.state, frame: f, title: TEXT_OF(w).ratio };
  })()`;
  const loading = await p.eval(texts);
  t.assert(loading.state === "loading" && loading.frame.bg === 0 && loading.title >= 4.5, `${scheme}: a transparent panel that loads has no fill and a readable title: ${JSON.stringify(loading)}`);
  await waitPanelsReady(p, [ERROR], "error");
  await waitPanelsReady(p, [EMPTY], "empty");
  for (const [id, state] of [[ERROR, "error"], [EMPTY, "empty"]]) {
    const m = await p.eval(`(() => { ${PAGE}
      const e = document.querySelector('[data-testid="panel-${id}"]');
      const o = e.querySelector(".panel__overlay");
      const words = [...e.querySelectorAll(".panel__overlay *")].filter((x) => x.children.length === 0 && x.textContent.trim() && x.tagName !== "BUTTON");
      return { state: e.dataset.state, frame: FRAME(e), overlay: o ? RGBA(getComputedStyle(o).backgroundColor)[3] : null, words: words.map((w) => [w.className, TEXT_OF(w).ratio]) };
    })()`);
    t.assert(m.state === state && m.frame.bg === 0 && m.frame.attr, `${scheme}: the ${state} state of a transparent panel has no fill: ${JSON.stringify(m)}`);
    t.assert(m.overlay === null || m.overlay === 0, `${scheme}: the ${state} overlay has no fill: ${JSON.stringify(m)}`);
    t.assert(m.words.length > 0 && m.words.every(([, ratio]) => ratio >= 4.5), `${scheme}: the text of the ${state} state has the contrast 4.5 on the page: ${JSON.stringify(m.words)}`);
  }
}


// --- B153: the audit of the options of the panel editor (10.3h, rule 14, F43) ------------------------------------------------------------

// The code of the page for the audit. It runs in the page: `controls()` lists the controls of the options pane by their root (`data-option`), with a key
// that stays the same when the pane is drawn again; `change(key, attempt)` changes one control like a person does and gives its old value back;
// `put(key, old, kind)` puts a value back; `print()` gives a text that changes when the picture of the preview changes.
function auditPage() {
  const q = (sel, root = document) => [...root.querySelectorAll(sel)];
  const hash = (text) => { let h = 2166136261; for (let i = 0; i < text.length; i++) { h ^= text.charCodeAt(i); h = Math.imul(h, 16777619); } return `${(h >>> 0).toString(36)}:${text.length}`; };
  const PANE = ".peditor__options";
  const labelOf = (c) => {
    if (c.matches("button.seg__item")) { const g = c.closest(".seg"); return `${(g && g.getAttribute("aria-label")) || "seg"}:${c.textContent.trim()}`; }
    const l = c.id ? document.querySelector(`label[for="${c.id}"]`) : null;
    const own = c.closest("label");
    return ((l && l.textContent) || (own && own.textContent) || c.getAttribute("aria-label") || c.textContent || c.dataset.testid || "").trim();
  };
  const roots = () => q(`${PANE} [data-option]`).filter((r) => !r.parentElement.closest("[data-option]") && !r.closest(".orow[hidden]"));
  // The controls of one root: a radio group gives one entry for each item that is not chosen; a check list gives its first two boxes.
  const atoms = (root) => {
    const out = [];
    for (const fs of q("fieldset.choice", root)) {
      const legend = fs.querySelector("legend").textContent.trim();
      for (const r of q("input[type=radio]", fs)) if (!r.checked) out.push({ c: r, kind: "radio", label: `${legend}:${r.parentElement.textContent.trim()}` });
    }
    const boxes = new Map();
    for (const c of q("input, select, textarea, button", root)) {
      if (c.type === "radio" || c.type === "range" || c.disabled) continue;
      let kind = c.tagName === "INPUT" ? c.type : c.tagName.toLowerCase();
      const label = labelOf(c);
      if (c.tagName === "BUTTON") {
        if (c.matches("button.seg__item")) { if (c.getAttribute("aria-pressed") === "true") continue; kind = "seg"; }
        else if (/^Add /.test(c.textContent.trim())) kind = "add";
        else continue;
      }
      if (/^Kind of the /.test(label)) continue;
      if (c.type === "checkbox" && c.closest(".checks__list")) {
        const list = c.closest(".checks__list");
        const n = boxes.get(list) || 0;
        boxes.set(list, n + 1);
        if (n >= 2) continue;
      }
      out.push({ c, kind, label });
    }
    return out;
  };
  const list = () => {
    const seen = new Map();
    const controls = [];
    const found = [];
    const all = [];
    for (const root of roots()) {
      const pointer = root.dataset.option;
      all.push(pointer);
      if (root.dataset.row) found.push(pointer);
      for (const a of atoms(root)) {
        const base = `${pointer}|${a.label}`;
        const n = seen.get(base) || 0;
        seen.set(base, n + 1);
        a.c.setAttribute("data-audit", `${base}|${n}`);
        controls.push({ key: `${base}|${n}`, pointer, row: root.dataset.row || null, label: a.label, kind: a.kind });
      }
    }
    const orphans = q(`${PANE} .ogroup__body input, ${PANE} .ogroup__body select, ${PANE} .ogroup__body textarea`).filter((c) => !c.closest("[data-option]") && !c.closest(".typelist") && !c.closest(".peditor__ohead")).map(labelOf);
    return { controls, found, all, orphans };
  };
  const find = (key, fresh) => {
    if (fresh) list();
    return document.querySelector(`[data-audit="${key.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"]`);
  };
  const fire = (c, names) => { for (const n of names) c.dispatchEvent(new Event(n, { bubbles: true })); };
  const TEXT = { Title: ["Audit title"], Description: ["Audit description"], Unit: ["percent", "ms"], Decimals: ["3", "0"], Min: ["0", "-1000"], Max: ["500", "1"], "No value": ["n/a"], Label: ["Audit axis"], "Soft min": ["0", "-1000"], "Soft max": ["900", "1"], Content: ["Audit **text**"], "Display name": ["Audit name"] };
  const num = (c, row) => {
    const cur = Number(c.value);
    const lo = row.min;
    const hi = row.max;
    const has = (v) => v !== undefined && v !== null;
    let cand;
    if (has(lo) && has(hi)) cand = [cur + 1, cur - 1, Math.round((lo + hi) / 2), hi, lo];
    else if (has(lo)) cand = [cur + 5, lo + 1, lo + 100];
    else cand = [cur + 5, cur + 100, cur - 100];
    cand = cand.filter((v) => (!has(lo) || (row.exclusiveMin ? v > lo : v >= lo)) && (!has(hi) || v <= hi) && v !== cur && (!row.integer || Number.isInteger(v)));
    return [...new Set(cand)].slice(0, 4).map(String);
  };
  const values = (c, label, pointer) => {
    const cur = c.value;
    const row = window.__au.rows[pointer];
    if (row && (row.kind === "number" || row.kind === "slider")) return num(c, row);
    if (label in TEXT) return TEXT[label].map((v) => (v === cur ? `${cur}1` : v));
    if (/ colour( value)?$/.test(label)) return ["red", "blue", "purple"].filter((x) => x !== cur);
    if (/^(x|y|w|h)$/.test(label)) {
      const get = (k) => Number((q(".position input").find((i) => (document.querySelector(`label[for="${i.id}"]`) || {}).textContent === k) || {}).value);
      const x = get("x");
      const w = get("w");
      if (label === "x") return [String(x + w <= 23 ? x + 1 : x - 1)];
      if (label === "y") return ["40", "0", "3"];
      if (label === "w") return [String(w > 1 ? w - 1 : w + 1)];
      return [String(get("h") + 1)];
    }
    if (/^-?\d+(\.\d+)?$/.test(cur)) return [String(Number(cur) + 5), "-1000000", "1000000"];
    return cur === "7" ? ["8"] : ["7"];
  };
  window.__au = {
    rows: {},
    controls: list,
    change(key, attempt = 0) {
      const c = find(key, true);
      if (!c) return { missing: true };
      const label = labelOf(c);
      const pointer = c.closest("[data-option]").dataset.option;
      if (c.type === "radio") { const old = q("input[type=radio]", c.closest("fieldset")).findIndex((x) => x.checked); c.click(); return { old, kind: "radio" }; }
      if (c.type === "checkbox") { c.click(); return { old: !c.checked, kind: "checkbox" }; }
      if (c.matches("button.seg__item")) { const items = q("button.seg__item", c.closest(".seg")); const old = items.findIndex((x) => x.getAttribute("aria-pressed") === "true"); c.click(); return { old, kind: "seg" }; }
      if (c.matches("button")) { c.click(); return { old: null, kind: "button" }; }
      if (c.tagName === "SELECT") {
        const old = c.value;
        const others = [...c.options].map((o) => o.value).filter((v) => v !== old && v !== "");
        if (others.length === 0) return { none: true, select: true };
        const next = others[Math.min(attempt, others.length - 1)];
        c.value = next; fire(c, ["change"]);
        return { old, next, kind: "select", more: attempt + 1 < Math.min(others.length, 4) };
      }
      const old = c.value;
      const all = values(c, label, pointer);
      if (all.length === 0) return { none: true };
      const next = all[Math.min(attempt, all.length - 1)];
      c.value = next; fire(c, ["input", "change"]);
      return { old, next, kind: "text", more: attempt + 1 < all.length };
    },
    put(key, old, kind) {
      // An old control that draws again (the cell type) has a new element: the key finds it.
      const c = find(key, false) || find(key, true);
      if (!c) return;
      if (kind === "radio") { const items = q("input[type=radio]", c.closest("fieldset")); if (items[old]) items[old].click(); return; }
      if (kind === "checkbox") { if (c.checked !== old) c.click(); return; }
      if (kind === "seg") { const items = q("button.seg__item", c.closest(".seg")); if (items[old]) items[old].click(); return; }
      if (kind === "select") { c.value = old; fire(c, ["change"]); return; }
      if (kind === "text") { c.value = old; fire(c, ["input", "change"]); }
    },
    print(raw) {
      const e = document.querySelector(".peditor__top .panel");
      if (!e) return "";
      // A hidden element shows nothing: the cursor, the badges and the card keep their last text and place when they hide, so they are not in the print.
      const copy = e.cloneNode(true);
      for (const n of [...copy.querySelectorAll("[hidden]")]) n.remove();
      const html = copy.outerHTML.replace(/ (id|for|aria-labelledby|aria-describedby)="[^"]*"/g, "").replace(/ data-(draw-ms|draws)="[^"]*"/g, "");
      const px = q("canvas", e).map((c) => { try { return hash(c.toDataURL()); } catch { return "x"; } });
      if (raw) return `${html}\n${q("canvas", e).map((c) => { try { return `${c.width}x${c.height}:${hash(c.toDataURL())}`; } catch { return "x"; } }).join(",")}`;
      return `${hash(html)}|${px.join(",")}`;
    },
  };
  return true;
}
const AUDIT_PAGE = `(${auditPage.toString()})()`;

const web = (path) => import(pathToFileURL(join(ROOT, "web/js/editor", path)).href);

// The permitted extra members of a control of the old forms (no row): a repeat in the direction `h` writes the place of the panel (4.13a, rule 5).
const OLD_WRITES = { repeat: ["grid"], "repeat.direction": ["repeat.max_per_row", "grid"] };

/** The panel of the draft in the page, or the saved panel when the page keeps no draft, and the print of the preview. */
const state = async (p, uid, id, saved, hover) => {
  if (hover) {
    await p.hover(hover.sel, hover.x, hover.y);
    await sleep(120);
  }
  const [raw, print, dump] = await p.eval(`[localStorage.getItem("hune.draft.v1.${uid}"), window.__au.print(), ${process.env.AUDIT_DUMP ? "window.__au.print(true)" : "null"}]`);
  let panel = saved;
  if (raw) {
    for (const sec of JSON.parse(raw).document.sections) for (const x of sec.panels) if (x.id === id) panel = x;
  }
  return { panel, text: JSON.stringify(panel), print, dump };
};

/** The state of the page after it stays the same for 450 ms (an animation, a canvas that draws in the next frames or a late answer changes it for a short time). */
async function stableState(p, uid, id, saved, hover, wait = 3500) {
  const end = Date.now() + wait;
  let last = await state(p, uid, id, saved, hover);
  let since = Date.now();
  for (;;) {
    await sleep(110);
    const now = await state(p, uid, id, saved, hover);
    if (now.print !== last.print || now.text !== last.text) since = Date.now();
    else if (Date.now() - since >= 450) return { ...now, stable: true };
    last = now;
    if (Date.now() > end) return { ...now, stable: false };
  }
}

/** Wait until the draft has the members of `target` again (a put-back that a text field writes 300 ms later), then until the page is stable. */
async function settle(p, uid, id, saved, hover, target) {
  const end = Date.now() + 4000;
  for (;;) {
    const now = await state(p, uid, id, saved, hover);
    if (changedMembers(target.panel, now.panel).length === 0 || Date.now() > end) break;
    await sleep(100);
  }
  return stableState(p, uid, id, saved, hover);
}

/**
 * One probe of one control (rule 14): the audit changes the control, reads the draft and the print of the preview (with the hover for a control of the
 * tooltip), puts the old value back and reads the print again. A slider, a number and a select get up to 4 values: the audit stops at the first
 * value that changes the preview.
 */
async function probe(p, uid, id, saved, key, { restore = true, hover = null } = {}) {
  let result = null;
  let first = null;
  for (let attempt = 0; attempt < 4; attempt++) {
    const pre = first ? await settle(p, uid, id, saved, hover, first) : await stableState(p, uid, id, saved, hover);
    first = first || pre;
    const done = await p.eval(`window.__au.change(${JSON.stringify(key)}, ${attempt})`);
    if (done.missing) return { skipped: "gone" };
    if (done.none) return done.select ? { ignore: true } : { skipped: "no value to try" };
    // The draft changes 300 ms after the last key, or in the next frame for a slider; the preview draws at once for the DOM and in the next frames for the canvas.
    const end = Date.now() + 1500;
    let draft = [];
    let preview = false;
    let post = pre;
    while (Date.now() < end && !(draft.length && preview)) {
      await sleep(100);
      post = await state(p, uid, id, saved, hover);
      draft = changedMembers(pre.panel, post.panel);
      preview = post.print !== pre.print;
    }
    result = { draft: [...new Set([...(result ? result.draft : []), ...draft])], preview: preview || !!(result && result.preview), kind: done.kind, stable: pre.stable, new: done.next };
    const again = !preview && done.more && attempt < 3;
    if ((restore || again) && done.kind !== "button") {
      await p.eval(`window.__au.put(${JSON.stringify(key)}, ${JSON.stringify(done.old)}, ${JSON.stringify(done.kind)})`);
    }
    if (!again) break;
  }
  if (restore && result.kind !== "button" && result.kind !== undefined) {
    const back = await settle(p, uid, id, saved, hover, first);
    result.back = back.print === first.print;
    if (!result.back && process.env.AUDIT_DUMP) {
      const a = first.dump;
      const b = back.dump;
      let i = 0;
      while (i < a.length && a[i] === b[i]) i++;
      console.error(`NOT STABLE ${key}\n  before: ${a.slice(Math.max(0, i - 120), i + 200)}\n  after:  ${b.slice(Math.max(0, i - 120), i + 200)}`);
    }
  }
  // The pointer goes away from the plot, so the cursor of the chart does not stay in the print of the next control.
  if (hover) await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 2, y: 2 });
  return result;
}

/** The document of the audit of one type: one panel for each state, and the variable of the repeat. */
function auditDoc(uid, panels) {
  panels.forEach((p, i) => { p.grid = g(0, i * 6, 12, 6); });
  return doc(uid, `Audit ${uid}`, [section(null, panels)], {
    time: { from: "now-6h", to: "now", timezone: "browser", refresh: null },
    variables: [{ kind: "custom", name: "region", label: null, hide: "none", options: [{ text: "North", value: "n" }, { text: "South", value: "s" }], multi: true, include_all: false, all_value: null, default: ["n", "s"] }],
  });
}

/** Open the panel editor of a panel on a fresh page, with the code of the audit. Chrome reports a changed network now and then when many pages load together: load again. */
async function openEditor(t, uid, id, rows) {
  for (let attempt = 0; ; attempt++) {
    const p = await t.page({ width: 1400, height: 900 });
    try {
      await p.goto(t.dash(uid, `edit=1&editPanel=${id}`));
      await p.waitSel(p.tid("panel-editor"), { timeout: 20000 });
      await editorSettled(p);
      await p.eval(AUDIT_PAGE);
      await p.eval(`window.__au.rows = ${JSON.stringify(rows)}`);
      return p;
    } catch (error) {
      if (attempt >= 3) throw error;
      t.allow(p, /./);
      await sleep(1500);
    }
  }
}

/** Run `tasks` (functions that return a promise), `limit` at the same time. */
async function pool(limit, tasks) {
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(limit, tasks.length) }, async () => {
    while (next < tasks.length) await tasks[next++]();
  }));
}

/** The audit of the options: the states of each type, each shown control, the rules F1 to F6. It returns { results, shown, found, extra }. */
async function audit(t) {
  const index = await web("forms/options/index.js");
  const rules = await web("forms/options/rows.js");
  const draft = await web("draft.js");
  const hoverAll = { ...A.HOVER, ...B.HOVER, ...C.HOVER };
  const only = process.env.AUDIT_ONLY ? process.env.AUDIT_ONLY.split(",") : null; // a debug aid: the types to audit
  const results = [];
  const shown = {};
  const found = {};
  const extra = [];
  const jobs = [];
  for (const [type, make] of TYPES) {
    if ((only && !only.includes(type)) || !draft.PANEL_TYPES.includes(type)) continue;
    const table = index.tableOf(type);
    const options = draft.newOptions(type);
    const states = table ? rules.statesOf(table, options) : [{ name: "new panel", options, changes: [], shown: null }];
    const rowsMap = {};
    for (const r of table ? table.rows : []) rowsMap[r.member] = { kind: r.kind, min: r.min, max: r.max, integer: r.kind === "slider" || r.integer === true, exclusiveMin: r.exclusiveMin };
    const uid = `ta153-${type.replace("_", "")}`;
    // The three series of a time series are below, at and above the step of the thresholds (80) and the step that the audit adds (90): each
    // control of the thresholds then shows in the colour of a line.
    const panels = states.map((s, i) => { const x = make(i + 1); x.options = structuredClone(s.options); if (AUDIT_SQL[type]) x.data.query.sql = AUDIT_SQL[type]; if (type.endsWith("timeline") || type === "status_history") x.fields.defaults = {}; return x; });
    // A filler panel under the others: the audit moves the audited panel (the field `y`), and a lone panel falls back to the top.
    panels.push(make(states.length + 1));
    const created = await t.api.create("transit", auditDoc(uid, panels));
    t.assert(created.status === 201, `create ${uid}: ${created.status} ${created.text.slice(0, 300)}`);
    const stored = (await t.api.dashboard(uid)).json.document.sections[0].panels;
    const hover = hoverAll[type] ? { sel: hoverAll[type].sel ? (hoverAll[type].sel.startsWith(".peditor") ? hoverAll[type].sel : `.peditor__top ${hoverAll[type].sel}`) : ".peditor__top .panel__plot", x: hoverAll[type].x, y: hoverAll[type].y } : null;
    const rowOf = (pointer) => (table ? table.rows.find((r) => r.member === pointer) : null);
    const log = (st, c, r) => {
      if (r.ignore) return;
      let value;
      const row = rowOf(c.pointer);
      if (c.kind === "checkbox" && row && row.kind === "checks") {
        const item = (row.items || []).find(([, label]) => label === c.label);
        value = item ? item[0] : undefined;
      }
      if (c.kind === "seg") value = c.label.split(":").slice(1).join(":").toLowerCase();
      if (c.kind === "radio" && row) {
        const text = c.label.split(":").slice(1).join(":");
        const item = (row.items || []).find(([, label]) => label === text);
        value = item ? item[0] : undefined;
      }
      const writes = [...((row && row.writes) || []), ...(OLD_WRITES[c.pointer] || [])];
      results.push({ type, state: st.name, pointer: c.pointer, value, kind: c.kind, writes, label: c.label, draft: [], back: true, ...r });
      if (process.env.AUDIT_LOG) console.error(type, st.name, c.pointer, c.label, JSON.stringify(r));
    };
    states.forEach((st, si) => {
      const id = si + 1;
      const saved = stored.find((x) => x.id === id);
      const probeHover = (c) => (hover && c.pointer.startsWith("options.tooltip") ? hover : null);
      jobs.push(async () => {
        const first = await openEditor(t, uid, id, rowsMap);
        const { controls, found: roots, all, orphans } = await first.eval("window.__au.controls()");
        if (table) {
          shown[`${type}|${st.name}`] = rules.shownRows(table, st.options).map((r) => r.member);
          found[`${type}|${st.name}`] = roots;
        } else if (si === 0) {
          for (const m of draftMembersWithoutControl(options, all, index.NO_CONTROL[type] || {})) extra.push(`${type}: the member ${m} has no control and no entry of NO_CONTROL`);
        }
        if (table && si === 0) {
          const members = [...leaves(options, "options"), ...(index.OPTIONAL_MEMBERS[type] || [])];
          for (const m of membersWithoutControl({ members, rows: table.rows.map((r) => r.member), noControl: index.NO_CONTROL[type] || {} })) extra.push(`${type}: the member ${m} has no row and no entry of NO_CONTROL`);
        }
        for (const o of orphans) extra.push(`${type} ${st.name}: a control with no data-option: ${o}`);
        // The first state has all the controls. The other states probe the rows of the table only (the old forms do not change with the state).
        const mine = controls.filter((c) => !c.pointer.startsWith("repeat") && c.kind !== "add" && (si === 0 || c.row));
        const adds = si === 0 ? controls.filter((c) => c.kind === "add") : [];
        const slices = [];
        for (let i = 0; i < mine.length; i += 14) slices.push(mine.slice(i, i + 14));
        // A group shows its effect in one colour mode only: the thresholds in "From thresholds", the legend of a timeline in "Series palette".
        const prepare = async (p, c, modes) => {
          const mode = { "fields.defaults.thresholds": "thresholds" }[c.pointer] || (type.endsWith("timeline") || type === "status_history" ? { "options.legend.show": "palette", "options.legend.placement": "palette" }[c.pointer] : null);
          if (!mode || modes.has(c.pointer)) return;
          modes.add(c.pointer);
          await p.eval(`(() => { window.__au.controls(); const s = document.querySelector('[data-option="fields.defaults.color"] select'); if (s && s.value !== ${JSON.stringify(mode)}) { s.value = ${JSON.stringify(mode)}; s.dispatchEvent(new Event("change", { bubbles: true })); } })()`);
          await sleep(500);
        };
        const runSlice = async (p, part) => {
          const modes = new Set();
          for (const c of part) {
            await prepare(p, c, modes);
            log(st, c, await probe(p, uid, id, saved, c.key, { hover: probeHover(c) }));
          }
        };
        const tasks = [() => runSlice(first, slices[0] || [])];
        slices.slice(1).forEach((part) => tasks.push(async () => runSlice(await openEditor(t, uid, id, rowsMap), part)));
        for (const c of adds) {
          tasks.push(async () => {
            const p = await openEditor(t, uid, id, rowsMap);
            await prepare(p, c, new Set());
            log(st, c, await probe(p, uid, id, saved, c.key, { restore: false }));
          });
        }
        if (si === 0 && type !== "text") {
          // The repeat options: the variable, then the direction and the maximum for each row.
          tasks.push(async () => {
            const p = await openEditor(t, uid, id, rowsMap);
            const head = { pointer: "repeat", label: "Repeat by variable", kind: "select", row: null };
            log(st, head, await probe(p, uid, id, saved, "repeat|Repeat by variable|0", { restore: false }));
            // "Max per row" first: the choice "Vertical" of the direction removes it.
            const more = (await p.eval("window.__au.controls()")).controls.filter((x) => x.pointer.startsWith("repeat") && x.key !== "repeat|Repeat by variable|0").sort((a, b) => (a.label === "Max per row" ? -1 : b.label === "Max per row" ? 1 : 0));
            for (const c of more) log(st, c, await probe(p, uid, id, saved, c.key, { restore: false }));
          });
        }
        await Promise.all(tasks.map((fn) => fn()));
      });
    });
  }
  await pool(Number(process.env.AUDIT_POOL || 4), jobs);
  return { results, shown, found, extra, hoverAll };
}

// The leaves of an options object: an object expands, an array or a scalar is a leaf.
function leaves(value, path, out = []) {
  if (value && typeof value === "object" && !Array.isArray(value)) for (const [k, v] of Object.entries(value)) leaves(v, `${path}.${k}`, out);
  else out.push(path);
  return out;
}

// The members of a type with no table (a timeline or `text`) that no root of the pane covers and that NO_CONTROL does not list.
function draftMembersWithoutControl(options, roots, noControl) {
  return membersWithoutControl({ members: leaves(options, "options"), rows: roots, noControl });
}

/**
 * The saved view: a change of the frame options in the editor reaches the stored document and the dashboard view. One stat panel, with the
 * variable `region`. The options: title, description, position (width) and repeat. `check(view)` reads the view after the save.
 */
async function viewStage(t, uid, keys, check) {
  const d = auditDoc(uid, [panel(1, "stat", "Stat", "SELECT 50 AS a, 150 AS b, CAST(NULL AS Nullable(Float64)) AS c")]);
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 300)}`);
  const measure = `(() => { const f = [...document.querySelectorAll('[data-testid^="panel-1"][data-type]')]; return { count: f.length, repeat: f.map((e) => e.dataset.repeatCount || null), width: f.length ? Math.round(f[0].getBoundingClientRect().width) : 0, titles: f.map((e) => (e.querySelector(".panel__title") || {}).textContent), info: f.filter((e) => e.querySelector(".panel__info")).length }; })()`;
  const view = async () => {
    for (let attempt = 0; ; attempt++) {
      const p = await t.page({ width: 1400, height: 900 });
      try {
        await p.goto(t.dash(uid));
        await p.idle();
        await p.waitSel('[data-testid^="panel-1"][data-type]', { timeout: 20000 });
        await sleep(400);
        return await p.eval(measure);
      } catch (error) {
        if (attempt >= 3) throw error;
        t.allow(p, /./);
        await sleep(1500);
      }
    }
  };
  const before = await view();
  const p = await openEditor(t, uid, 1, {});
  for (const key of keys) {
    const done = await p.eval(`window.__au.change(${JSON.stringify(key)})`);
    t.assert(!done.missing && !done.none, `${uid}: the control ${key} is in the editor`);
    await sleep(500);
  }
  await saveDialog(p, "view stage");
  await p.waitFor(`!document.querySelector('[data-testid="save-dialog"]')`, { what: "the save dialog" });
  await sleep(800);
  t.eq((await t.api.dashboard(uid)).status, 200, `${uid}: the dashboard is stored`);
  check(before, await view());
}

/** The view of the frame options: the title, the description, the width and the copies of a repeat reach the saved view. */
async function viewStages(t) {
  await viewStage(t, "ta153v", ["title|Title|0", "description|Description|0", "grid.w|w|0"], (before, after) => {
    t.eq(after.titles, ["Audit title"], "the saved view has the new title");
    t.assert(after.info === 1 && before.info === 0, `the saved view has the info icon of the new description: ${JSON.stringify([before.info, after.info])}`);
    t.assert(after.width < before.width, `the saved view has the new width: ${before.width} then ${after.width}`);
  });
  await viewStage(t, "ta153r", ["repeat|Repeat by variable|0"], (before, after) => {
    t.assert(before.count === 1 && after.count === 2, `the saved view has one frame for each value of the variable: ${before.count} then ${after.count}`);
  });
}

export const cases = [
  {
    id: "B152",
    name: "the switch Transparent background changes the frame of the preview and of the saved view, for the ten types, in both themes (4.13, 9.8, 10.3)",
    async run(t) {
      for (const scheme of ["dark", "light"]) {
        // Chrome reports a changed network now and then (another process on the host starts a container) and then fails a module fetch:
        // the session starts again, under a new uid, after such a failure only.
        for (let attempt = 0; ; attempt++) {
          const uid = `ta152-${scheme}${attempt ? `-${attempt}` : ""}`;
          try {
            await session(t, scheme, uid);
            await viewChecks(t, scheme, uid);
            break;
          } catch (error) {
            if (attempt >= 2 || !/timeout waiting for selector/.test(error.message)) throw error;
            for (const page of t.pages) t.allow(page, /ERR_NETWORK_CHANGED|dynamically imported module/);
          }
        }
        await stateChecks(t, scheme, `ta152s-${scheme}`);
      }
    },
  },
  {
    id: "B153",
    timeout: 900000, // the states and the hover probe add about 170 probes to the 317 control changes of before F43 (16.10)
    name: "the audit of the options of the panel editor: the draft changes in the member of the control, the preview changes or the control is in the closed list, the print comes back (10.3, 10.3h)",
    async run(t) {
      const started = Date.now();
      const { results, shown, found, extra } = await audit(t);
      if (process.env.AUDIT_LOG) console.error("audit seconds", (Date.now() - started) / 1000);
      if (process.env.AUDIT_OUT) (await import("node:fs")).writeFileSync(process.env.AUDIT_OUT, JSON.stringify(results, null, 1));
      const only = process.env.AUDIT_ONLY ? process.env.AUDIT_ONLY.split(",") : null;
      let exempt = mergeExempt([A.EXEMPT, B.EXEMPT, C.EXEMPT]);
      // The entries of a type that the page does not draw yet (heatmap and histogram, F31) wait: no control matches them now.
      const drawn = (await web("draft.js")).PANEL_TYPES.filter((x) => !only || only.includes(x));
      exempt = exempt.map((e) => (e.types ? { ...e, types: e.types.filter((x) => drawn.includes(x)) } : e)).filter((e) => !e.types || e.types.length);
      const bad = [...judge({ results, exempt, shown, found }), ...extra];
      if (!only) t.assert(results.length > 300, `the audit changed ${results.length} controls`);
      if (process.env.AUDIT_OUT) (await import("node:fs")).writeFileSync(`${process.env.AUDIT_OUT}.failures.txt`, `${bad.join("\n")}\n`);
      t.assert(bad.length === 0, `${bad.length} failures of the audit: ${bad.slice(0, 12).join("; ")}${bad.length > 12 ? "; ..." : ""}`);
      await viewStages(t);
    },
  },
];
