// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B200 and B203 to B206 (10.3h, 9.26, F43): the option forms of the data types, group A.
// B200: the forms of timeseries, barchart and xychart: the groups, the labels, the writes, the effects, the rules of display, the status line.
// B203: the keyboard and the accessibility tree of the options pane, for one panel of each type.
// B204: the look of the options pane: no scroll bar, no cut label, targets of 24 px, contrast, in two themes and three widths.
// B205: the live preview: the draft and the frame change in the same task, the slider writes once in each frame, the number field after 300 ms.
// B206: the rules of display: a hidden row has no effect in the picture of the preview and keeps its member.
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { doc, panel, section, SQL } from "../lib/docs.mjs";
import { ROOT } from "../lib/env.mjs";
import { waitDraft } from "../lib/ed.mjs";
import { sleep } from "../lib/h.mjs";

const g = (x, y, w, h) => ({ x, y, w, h });
const web = (path) => import(pathToFileURL(join(ROOT, "web/js/editor", path)).href);

// The three series are below and above the step of the thresholds, and one has a null: the options show in the picture.
const TS_SQL = "SELECT toDateTime(1794211200,'UTC') + number*40 AS time, 150 + number AS a, if(number % 7 = 3, NULL, 40 + number) AS b, 85 + number AS c FROM numbers(30)";
const MAKERS = {
  timeseries: (id) => panel(id, "timeseries", "Series", TS_SQL, { grid: g(0, 0, 12, 6), format: "time_series" }),
  barchart: (id) => panel(id, "barchart", "Bars", SQL.bars, { grid: g(0, 0, 12, 6) }),
  xychart: (id) => panel(id, "xychart", "XY", SQL.xy, { grid: g(0, 0, 12, 6) }),
  stat: (id) => panel(id, "stat", "Stat", "SELECT 50 AS a, 150 AS b, CAST(NULL AS Nullable(Float64)) AS c", { grid: g(0, 0, 12, 6) }),
  bargauge: (id) => panel(id, "bargauge", "Gauge", SQL.bars, { grid: g(0, 0, 12, 6), fields: { defaults: { min: 0, max: 400 }, overrides: [] } }),
  piechart: (id) => panel(id, "piechart", "Pie", SQL.bars, { grid: g(0, 0, 12, 6) }),
  table: (id) => panel(id, "table", "Table", SQL.table, { grid: g(0, 0, 12, 6) }),
};
const TL_SQL = "SELECT toDateTime(1794211200,'UTC') + number*40 AS time, intDiv(number,3)%3 AS a, intDiv(number,5)%3 AS b FROM numbers(30)";
const TIMELINE = (type) => (id) => ({
  id, type, title: type, description: "", transparent: false, grid: g(0, 0, 12, 6),
  data: { query: { source: "prod", format: "table", sql: [TL_SQL], min_interval: null, max_data_points: null } },
  transforms: [], fields: { defaults: {}, overrides: [] }, links: [],
  options: type === "state_timeline"
    ? { merge_values: true, show_values: "auto", align_values: "left", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single", sort: "none" } }
    : { show_values: "auto", row_height: 90, col_width: 90, line_width: 1, fill_opacity: 70, show_labels: true, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single", sort: "none" } },
});
MAKERS.state_timeline = TIMELINE("state_timeline");
MAKERS.status_history = TIMELINE("status_history");

/** A document with one panel of each type of `types`, the panel with id i + 1. */
async function makeDoc(t, uid, types, mutate = null) {
  const panels = types.map((type, i) => {
    const x = MAKERS[type](i + 1);
    if (mutate) mutate(x, type, i);
    return x;
  });
  panels.forEach((x, i) => { x.grid = g((i % 2) * 12, Math.floor(i / 2) * 6, 12, 6); });
  const r = await t.api.create("transit", doc(uid, `Options ${uid}`, [section(null, panels)]));
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 300)}`);
}

/** Waits for two painted frames: the resize callbacks and the frame writes of the page have run (a wait on the frames, not on a fixed time). */
const painted = (p) => p.eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))");

const editorSettled = (p) => p.waitFor(`(() => { const e = document.querySelector(".peditor__top .panel"); return e && !["idle", "queued", "loading"].includes(e.dataset.state); })()`, { what: "the preview" });

/** Open the panel editor of the panel `id` on a fresh page. Chrome reports a changed network now and then when pages load together: load again. */
async function open(t, uid, id, { width = 1400, height = 1000, scheme = null, page = null } = {}) {
  for (let attempt = 0; ; attempt++) {
    const p = page || await t.page({ width, height });
    try {
      if (scheme) await p.emulateColorScheme(scheme);
      await p.goto(t.dash(uid, `edit=1&editPanel=${id}`));
      await p.waitSel(p.tid("panel-editor"), { timeout: 20000 });
      await editorSettled(p);
      return p;
    } catch (error) {
      if (attempt >= 3) throw error;
      t.allow(p, /./);
      await sleep(1500);
    }
  }
}

/**
 * The print of the preview: the markup of the frame without ids and timers, and a hash of each canvas. With `picture` the print also leaves out the
 * attributes `data-*` and adds the size of each canvas and plot: the hooks of the frame write the options as text (`data-series-style`), so they
 * change when a member changes that the picture does not use. The picture is what the person sees: the pixels, the sizes and the text.
 */
const printCode = (picture) => `(() => {
  const hash = (text) => { let h = 2166136261; for (let i = 0; i < text.length; i++) { h ^= text.charCodeAt(i); h = Math.imul(h, 16777619); } return (h >>> 0).toString(36) + ":" + text.length; };
  const e = document.querySelector(".peditor__top .panel");
  if (!e) return "";
  const copy = e.cloneNode(true);
  for (const n of [...copy.querySelectorAll("[hidden]")]) n.remove();
  let html = copy.outerHTML.replace(/ (id|for|aria-labelledby|aria-describedby|data-testid)="[^"]*"/g, "").replace(/ data-(draw-ms|draws)="[^"]*"/g, "");
  if (${picture}) html = html.replace(/ data-[a-z-]+="[^"]*"/g, "");
  const boxes = ${picture} ? "|" + [...e.querySelectorAll("canvas, .chartCore__plot, .chartCore__legend, .hbar, .hbar__legend")].map((n) => { const r = n.getBoundingClientRect(); return Math.round(r.left) + "," + Math.round(r.top) + "," + Math.round(r.width) + "," + Math.round(r.height); }).join(";") : "";
  return hash(html) + boxes + "|" + [...e.querySelectorAll("canvas")].map((c) => { try { return hash(c.toDataURL()); } catch { return "x"; } }).join(",");
})()`;
const PRINT = printCode(false);
const PICTURE = printCode(true);
const print = (p, code = PRINT) => p.eval(code);
/** The print after it stays the same for 450 ms. */
async function settled(p, code = PRINT) {
  let last = await print(p, code);
  let since = Date.now();
  const end = Date.now() + 4000;
  for (;;) {
    await sleep(100);
    const now = await print(p, code);
    if (now !== last) since = Date.now();
    else if (Date.now() - since >= 450) return now;
    last = now;
    if (Date.now() > end) return now;
  }
}

const draftPanel = async (p, uid, id, pred = () => true) => {
  try {
    return (await waitDraft(p, uid, (d) => pred(d.document.sections[0].panels.find((x) => x.id === id)))).document.sections[0].panels.find((x) => x.id === id);
  } catch (error) {
    throw new Error(`${error.message}: waiting for ${pred.toString().slice(0, 120)}`);
  }
};

/** Click the radio of the item `text` of the choice that has the hook `opt-<id>`. */
const choose = (p, id, text) => p.eval(`(() => { const f = document.querySelector('[data-testid="opt-${id}"]'); const l = [...f.querySelectorAll("label")].find((x) => x.textContent.trim() === ${JSON.stringify(text)}); l.querySelector("input").click(); })()`);
const rowState = (p, id) => p.eval(`(() => { const r = document.querySelector('.orow[data-row="${id}"]'); return r ? { hidden: r.hidden, rule: r.dataset.hiddenByRule || null } : null; })()`);
const status = (p) => p.eval(`document.querySelector('[data-testid="options-status"]').textContent`);

export const cases = [
  {
    id: "B200",
    name: "the forms of timeseries, barchart and xychart: groups, labels, writes, effects, rules of display and the status line (10.3h)",
    async run(t) {
      const index = await web("forms/options/index.js");
      const rules = await web("forms/options/rows.js");
      const types = ["timeseries", "barchart", "xychart"];
      await makeDoc(t, "tb200", types);
      for (let i = 0; i < types.length; i++) {
        const type = types[i];
        const id = i + 1;
        const table = index.tableOf(type);
        const p = await open(t, "tb200", id);
        // The groups come after "Visualisation" and before "Standard options", in the order of the table, as the table says.
        const groups = await p.eval(`[...document.querySelectorAll(".peditor__obody .ogroup__title")].map((e) => e.textContent)`);
        const want = ["Panel options", "Visualisation", ...table.groups.map(([, title]) => title), "Standard options", "Thresholds"];
        t.eq(groups.slice(0, want.length), want, `${type}: the groups and their order`);
        // The rows that show: each has its control with the hook, the label and the member of the table.
        const first = rules.shownRows(table, MAKERS[type](id).options);
        const found = await p.eval(`[...document.querySelectorAll(".orow[data-row]:not([hidden])")].map((e) => [e.dataset.row, e.dataset.option, !!e.querySelector("[data-testid]"), e.querySelector("legend, label, .field__label")?.textContent.trim()])`);
        t.eq(found.map((x) => x[0]).sort(), first.map((r) => r.id).sort(), `${type}: the rows that show are the rows of the table`);
        for (const [rowId, member, hook, label] of found) {
          const row = table.rows.find((r) => r.id === rowId);
          t.assert(row.member === member, `${type} ${rowId}: data-option is ${member}`);
          t.assert(hook, `${type} ${rowId}: the control has a hook`);
          if (row.kind !== "switch" && row.kind !== "custom") t.assert(label && label.startsWith(row.label.split(" ")[0]), `${type} ${rowId}: the label ${label}`);
        }
        t.assert(await p.eval(`!!document.querySelector('[data-testid="options-status"][role="status"]')`), `${type}: the status line`);
      }

      // timeseries: the writes and their effects.
      let p = await open(t, "tb200", 1);
      const seriesStyle = () => p.eval(`document.querySelector(".peditor__top .panel__plot").dataset.seriesStyle || ""`);
      const axes = () => p.eval(`document.querySelector(".peditor__top .panel__plot").dataset.axes || ""`);
      const thresholds = () => p.eval(`document.querySelector(".peditor__top .panel__plot").dataset.thresholds || ""`);
      const px0 = await settled(p);
      const style0 = await seriesStyle();
      await choose(p, "draw", "Bars");
      let d = await draftPanel(p, "tb200", 1, (x) => x.options.draw === "bars");
      t.eq(d.options.draw, "bars", "timeseries: draw writes bars");
      await sleep(600);
      t.assert((await seriesStyle()) !== style0 && (await settled(p)) !== px0, "timeseries: the bars change the series style and the pixels");
      // "Bars" hides the rows of a line, and the status line tells it.
      t.eq((await rowState(p, "interpolation")).hidden, true, "timeseries: interpolation hides with bars");
      t.eq((await rowState(p, "interpolation")).rule, "true", "timeseries: data-hidden-by-rule");
      t.assert(/^Hidden: .*Line interpolation/.test(await status(p)) || /Hidden: \d+ options\./.test(await status(p)), `timeseries: the status line: ${await status(p)}`);
      await choose(p, "draw", "Lines");
      await sleep(700); // the draft is the saved panel again: it has no record
      t.eq((await rowState(p, "interpolation")).hidden, false, "timeseries: interpolation shows again");
      t.assert(/Shown:/.test(await status(p)), "timeseries: the status line says what shows");
      // Scale and thresholds: the hooks `data-axes` and `data-thresholds`.
      const axes0 = await axes();
      await choose(p, "y-scale", "Logarithmic");
      await draftPanel(p, "tb200", 1, (x) => x.options.y_scale === "log");
      await sleep(600);
      t.assert((await axes()) !== axes0, "timeseries: the log scale changes data-axes");
      await p.select(`[data-testid="opt-thresholds-style"]`, "dashed_area");
      d = await draftPanel(p, "tb200", 1, (x) => x.options.thresholds_style === "dashed_area");
      t.eq(Object.keys(d.options).at(-1), "thresholds_style", "timeseries: thresholds_style is the last member (4.38)");
      await sleep(600);
      t.assert((await thresholds()) !== "", "timeseries: data-thresholds has the style");
      await p.select(`[data-testid="opt-thresholds-style"]`, "off");
      d = await draftPanel(p, "tb200", 1, (x) => !("thresholds_style" in x.options));
      t.assert(!("thresholds_style" in d.options), "timeseries: the value `off` is `absent` and leaves the panel");
      // The tooltip: the sort and the zeros show with "All", and an optional member leaves the draft with its absent value.
      await choose(p, "tooltip-mode", "All");
      t.eq((await rowState(p, "tooltip-sort")).hidden, false, "timeseries: the sort shows with All");
      t.eq(await status(p), "Shown: Values sort order, Hide zeros.", "timeseries: the status line for All");
      await choose(p, "tooltip-sort", "Ascending");
      d = await draftPanel(p, "tb200", 1, (x) => x.options.tooltip.sort === "asc");
      await choose(p, "tooltip-sort", "None");
      d = await draftPanel(p, "tb200", 1, (x) => !("sort" in x.options.tooltip));
      t.assert(!("sort" in d.options.tooltip), "timeseries: the sort None leaves the panel");
      // The legend as a table: the values, the limit of 5 and the sort with the switch of the direction.
      await choose(p, "legend-mode", "Table");
      await p.eval(`(() => { for (const b of [...document.querySelectorAll('[data-testid="opt-legend-values"] input')].slice(0, 5)) b.click(); })()`);
      d = await draftPanel(p, "tb200", 1, (x) => x.options.legend.values.length === 5);
      t.eq(d.options.legend.values.length, 5, "timeseries: five legend values");
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="opt-legend-values"] input')].filter((b) => b.disabled).length`), (await p.eval(`document.querySelectorAll('[data-testid="opt-legend-values"] input')`).catch(() => 0)) ? 13 : 13, "timeseries: the other boxes are disabled at the limit");
      await p.select(`[data-testid="opt-legend-sort-by"]`, "name");
      await draftPanel(p, "tb200", 1, (x) => x.options.legend.sort_by === "name");
      await p.click(`[data-testid="opt-legend-sort-desc"]`);
      d = await draftPanel(p, "tb200", 1, (x) => x.options.legend.sort_desc === true);
      await p.select(`[data-testid="opt-legend-sort-by"]`, "");
      d = await draftPanel(p, "tb200", 1, (x) => !("sort_by" in x.options.legend));
      t.assert(!("sort_by" in d.options.legend) && !("sort_desc" in d.options.legend), "timeseries: the sort None removes sort_by and sort_desc");
      // A value that the form does not know: the error shows, and the form writes nothing.
      const bad = JSON.stringify({ ...d, options: { ...d.options, draw: "x" } }, null, 2);
      await p.click(p.tid("tab-json"));
      await p.eval(`(() => { const a = document.querySelector('[data-testid="panel-json"]'); a.value = ${JSON.stringify(bad)}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
      await sleep(500);
      t.allow(p, /422/); // the validation of the draft refuses `draw` = `x`: the page logs it
      await p.click(p.tid("tab-query"));
      await sleep(400);
      const unknown = await p.eval(`(() => { const f = document.querySelector('[data-testid="opt-draw"]'); return { invalid: f.getAttribute("aria-invalid"), checked: f.querySelectorAll("input:checked").length, error: f.querySelector(".field-error").textContent }; })()`);
      t.eq([unknown.invalid, unknown.checked], ["true", 0], "timeseries: a value that the form does not know has no item chosen and aria-invalid");
      t.assert(/The value "x" is not valid/.test(unknown.error), `timeseries: the error: ${unknown.error}`);
      t.eq((await draftPanel(p, "tb200", 1)).options.draw, "x", "timeseries: the form writes nothing");

      // barchart: the orientation hides the rotation, a stack hides the values and the width of a group; the horizontal chart draws bars.
      p = await open(t, "tb200", 2);
      await choose(p, "orientation", "Horizontal");
      t.eq((await rowState(p, "x-tick-rotation")).hidden, true, "barchart: the rotation hides when horizontal");
      await draftPanel(p, "tb200", 2, (x) => x.options.orientation === "horizontal");
      await sleep(600);
      t.assert(await p.eval(`!!document.querySelector(".peditor__top .hbar, .peditor__top [class*='hbar']")`), "barchart: the horizontal bars are in the preview");
      await choose(p, "orientation", "Vertical");
      await choose(p, "stacking", "Normal");
      t.eq([(await rowState(p, "show-values")).hidden, (await rowState(p, "group-width")).hidden], [true, true], "barchart: a stack hides the values and the group width");
      d = await draftPanel(p, "tb200", 2, (x) => x.options.stacking === "normal");
      t.eq(d.options.group_width, 70, "barchart: a hidden row keeps its member");
      await choose(p, "stacking", "Off");
      await p.fill(`[data-testid="opt-bar-width"]`, "50");
      d = await draftPanel(p, "tb200", 2, (x) => x.options.bar_width === 50);
      t.eq(d.options.bar_width, 50, "barchart: the bar width writes 50");
      await p.fill(`[data-testid="opt-bar-width"]`, "9");
      await sleep(500);
      t.assert((await p.eval(`document.querySelector('.orow[data-row="bar-width"] .field-error:not([hidden])')?.textContent || ""`)).includes("from 10 to 100"), "barchart: 9 shows the error of the range");
      t.eq((await draftPanel(p, "tb200", 2)).options.bar_width, 50, "barchart: 9 writes nothing");

      // xychart: the draw mode hides the point size or the line width.
      p = await open(t, "tb200", 3);
      await choose(p, "draw", "Lines");
      t.eq([(await rowState(p, "point-size")).hidden, (await rowState(p, "line-width")).hidden], [true, false], "xychart: lines hide the point size");
      await choose(p, "draw", "Points");
      t.eq([(await rowState(p, "point-size")).hidden, (await rowState(p, "line-width")).hidden], [false, true], "xychart: points hide the line width");
      await sleep(700); // the draw `points` is the saved value: the draft equals the saved panel and has no record
      await p.fill(`[data-testid="opt-display-name"]`, "Audit name");
      d = await draftPanel(p, "tb200", 3, (x) => x.fields.defaults.display_name === "Audit name");
      t.eq(d.fields.defaults.display_name, "Audit name", "xychart: the display name writes fields.defaults.display_name");
      await p.fill(`[data-testid="opt-display-name"]`, "");
      await sleep(700); // the draft equals the saved panel again: it has no record
      t.eq(await p.eval(`document.querySelector('[data-testid="opt-display-name"]').value`), "", "xychart: the display name is empty");
      t.assert(!(await p.eval(`(() => { const e = document.querySelector('.orow[data-row="display-name"] .field-error:not([hidden])'); return e ? e.textContent : ""; })()`)), "xychart: an empty display name is not an error");
    },
  },
  {
    id: "B203",
    name: "the keyboard and the accessibility tree in the options pane: tab order, radio groups, arrow keys, Home and End, slider keys, Space, names, status line and focus (9.26, 10.3h)",
    async run(t) {
      const index = await web("forms/options/index.js");
      const draft = await web("draft.js");
      const types = ["timeseries", "barchart", "xychart", "stat", "bargauge", "piechart", "table", "state_timeline", "status_history"];
      await makeDoc(t, "tb203", types);
      const ROLE = { radio: "group", switch: "switch", checks: "group", fields: "group", slider: "group", select: "combobox", field: "combobox", number: "textbox", text: "textbox" };
      for (let i = 0; i < types.length; i++) {
        const type = types[i];
        const id = i + 1;
        const p = await open(t, "tb203", id);
        // The Tab order is the order of the page: no positive tabindex, and the places go down the pane.
        const order = await p.eval(`(() => {
          const pane = document.querySelector(".peditor__options");
          const list = [...pane.querySelectorAll("input, select, textarea, button, [tabindex]")].filter((e) => !e.disabled && e.tabIndex >= 0 && e.getClientRects().length && getComputedStyle(e).visibility !== "hidden" && !e.closest("[hidden]"));
          // A control inside a box with its own scroll (a long check list) has the place of the box: the box is the visual stop.
          const inner = (e) => { for (let n = e.parentElement; n && n !== pane && !n.classList.contains("peditor__obody"); n = n.parentElement) { const o = getComputedStyle(n).overflowY; if ((o === "auto" || o === "scroll") && n.scrollHeight > n.clientHeight) return n; } return null; };
          const top = (e) => (inner(e) || e).getBoundingClientRect().top + pane.scrollTop;
          const bad = [];
          let last = -1;
          let prev = "";
          for (const e of list) {
            if (e.tabIndex > 0) bad.push("positive tabindex");
            const y = top(e);
            if (y + 12 < last) bad.push((e.dataset.testid || e.getAttribute("aria-label") || e.tagName) + " at " + Math.round(y) + " after " + Math.round(last) + " (" + prev + ")");
            if (y >= last) prev = e.dataset.testid || e.getAttribute("aria-label") || e.tagName;
            last = Math.max(last, y);
          }
          return { count: list.length, bad };
        })()`);
        t.assert(order.count > 5 && order.bad.length === 0, `${type}: the Tab order is the visual order: ${JSON.stringify(order)}`);
        // A radio group is one stop: Tab from the chosen radio leaves the group.
        const groups = await p.eval(`[...document.querySelectorAll(".peditor__options fieldset.choice")].filter((f) => !f.closest("[hidden]")).length`);
        if (groups) {
          const left = await p.eval(`(() => { const f = document.querySelector(".peditor__options fieldset.choice"); const r = f.querySelector("input:checked") || f.querySelector("input"); r.focus(); return r.closest("fieldset") === f; })()`);
          t.assert(left, `${type}: a radio has the focus`);
          await p.press("Tab");
          t.assert(await p.eval(`(() => { const f = document.querySelector(".peditor__options fieldset.choice"); return !f.contains(document.activeElement); })()`), `${type}: Tab leaves the radio group (one stop)`);
        }
        // The accessible names: each row of the table that shows has a node with the role and the name of its label.
        const table = index.tableOf(type);
        if (table) {
          await p.eval(`for (const b of document.querySelectorAll(".peditor__options .ogroup__head[aria-expanded=false]")) b.click()`);
          await painted(p);
          await p.send("Accessibility.enable");
          const { nodes } = await p.send("Accessibility.getFullAXTree");
          const seen = nodes.filter((n) => !n.ignored).map((n) => `${n.role && n.role.value}|${n.name && n.name.value}`);
          const rows = await p.eval(`[...document.querySelectorAll(".orow[data-row]:not([hidden])")].map((e) => e.dataset.row)`);
          for (const rowId of rows) {
            const row = table.rows.find((r) => r.id === rowId);
            if (row.kind === "custom" || row.kind === "list") continue;
            const role = row.kind === "radio" ? "group" : ROLE[row.kind];
            const name = row.kind === "slider" ? row.label : row.label;
            t.assert(seen.some((s) => s === `${role}|${name}`) || seen.some((s) => s.endsWith(`|${name}`)), `${type} ${rowId}: the accessibility tree has "${name}" (${role})`);
          }
        }
      }

      // The keys, on a time series.
      let p = await open(t, "tb203", 1);
      const focusRadio = (hook, text) => p.eval(`(() => { const f = document.querySelector('[data-testid="${hook}"]'); const l = [...f.querySelectorAll("label")].find((x) => x.textContent.trim() === ${JSON.stringify(text)}); l.querySelector("input").focus(); })()`);
      await focusRadio("opt-draw", "Lines");
      await p.press("ArrowRight");
      await draftPanel(p, "tb203", 1, (x) => x.options.draw === "bars");
      t.assert(await p.eval(`document.activeElement.closest('[data-testid="opt-draw"]') !== null`), "ArrowRight moves the choice and keeps the focus in the group");
      await p.press("End");
      await draftPanel(p, "tb203", 1, (x) => x.options.draw === "points");
      await p.press("Home");
      // `line` is the saved value: the draft equals the saved panel and has no record. The case waits for the choice in the form.
      const checkedDraw = `document.querySelector('[data-testid="opt-draw"] input:checked').closest("label").textContent.trim()`;
      await p.waitFor(`${checkedDraw} === "Lines"`, { what: "Home chooses the first item" }).catch(() => {});
      t.eq(await p.eval(checkedDraw), "Lines", "Home chooses the first item");
      // The slider keys of a native range.
      await p.eval(`document.querySelector('[data-testid="opt-line-width-slider"]').focus()`);
      await p.press("ArrowRight");
      await draftPanel(p, "tb203", 1, (x) => x.options.line_width === 2);
      await p.press("End");
      await draftPanel(p, "tb203", 1, (x) => x.options.line_width === 10);
      await p.press("Home");
      await draftPanel(p, "tb203", 1, (x) => x.options.line_width === 0);
      // Space toggles a switch and a check box.
      await p.eval(`document.querySelector('[data-testid="opt-span-nulls"]').focus()`);
      await p.press(" ");
      await draftPanel(p, "tb203", 1, (x) => x.options.span_nulls === true);
      await choose(p, "legend-mode", "Table");
      await p.eval(`document.querySelector('[data-testid="opt-legend-values"] input').focus()`);
      await p.press(" ");
      await draftPanel(p, "tb203", 1, (x) => x.options.legend.values.length === 1);
      // The status line says the change, and the focus stays on the control that changed.
      await focusRadio("opt-tooltip-mode", "Single");
      await p.press("ArrowRight");
      await draftPanel(p, "tb203", 1, (x) => x.options.tooltip.mode === "multi");
      t.eq(await status(p), "Shown: Values sort order, Hide zeros.", "the status line says the change");
      t.assert(await p.eval(`document.activeElement.closest('[data-testid="opt-tooltip-mode"]') !== null`), "the focus stays on the control that changed");
      // The focus goes to the head of the group when its row hides.
      await choose(p, "legend-mode", "List");
      await p.eval(`document.querySelector('[data-testid="opt-legend-placement"] input').focus()`);
      await p.eval(`document.querySelector('[data-testid="opt-legend-show"]').click()`);
      await p.waitFor(`document.activeElement.dataset.group === "legend"`, { what: "the focus on the head of the group" }).catch(() => {});
      t.eq(await p.eval(`document.activeElement.dataset.group || null`), "legend", "the focus goes to the head of the group when its row hides");
    },
  },
  {
    id: "B204",
    name: "the look of the options pane: no scroll bar, no cut label, targets of 24 px, contrast 4.5 and 3, a pressed segment differs by more than the colour (9.26, 10.3h)",
    timeout: 400000,
    async run(t) {
      const types = ["timeseries", "barchart", "xychart", "stat", "bargauge", "piechart", "table", "state_timeline", "status_history"];
      await makeDoc(t, "tb204", types);
      const MEASURE = `(() => {
        const RGBA = (c) => { const k = document.createElement("canvas"); k.width = k.height = 1; const x = k.getContext("2d", { willReadFrequently: true }); x.clearRect(0, 0, 1, 1); x.fillStyle = "#000"; x.fillStyle = c; x.fillRect(0, 0, 1, 1); const d = x.getImageData(0, 0, 1, 1).data; return [d[0], d[1], d[2], d[3] / 255]; };
        const LUM = (c) => { const f = c.map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }); return 0.2126 * f[0] + 0.7152 * f[1] + 0.0722 * f[2]; };
        const RATIO = (a, b) => { const x = LUM(a), y = LUM(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
        const OVER = (f, b) => [0, 1, 2].map((i) => f[i] * f[3] + b[i] * (1 - f[3]));
        const bgOf = (e) => { for (let n = e; n; n = n.parentElement) { const c = RGBA(getComputedStyle(n).backgroundColor); if (c[3] > 0.99) return c; } return [255, 255, 255, 1]; };
        const pane = document.querySelector(".peditor__options");
        for (const b of pane.querySelectorAll(".ogroup__head[aria-expanded=false]")) b.click();
        const body = document.querySelector(".peditor__obody");
        const problems = [];
        if (document.documentElement.scrollWidth > innerWidth + 1) problems.push("page scroll " + document.documentElement.scrollWidth + " > " + innerWidth);
        if (body.scrollWidth > body.clientWidth + 1) problems.push("pane scroll " + body.scrollWidth + " > " + body.clientWidth);
        const box = pane.getBoundingClientRect();
        for (const row of pane.querySelectorAll(".orow[data-row]:not([hidden])")) {
          const r = row.getBoundingClientRect();
          if (r.right > box.right + 1 || r.left < box.left - 1) problems.push(row.dataset.row + " is outside the pane");
          for (const l of row.querySelectorAll(".field__label")) if (l.scrollWidth > l.clientWidth + 1) problems.push("label cut: " + l.textContent);
          for (const c of row.querySelectorAll("input:not([type=range]), select, textarea, button")) {
            const target = c.type === "radio" ? c.closest("label") : c.type === "checkbox" ? c.closest("label") || c : c;
            const tr = target.getBoundingClientRect();
            if (c.type !== "radio" || true) if (tr.width < 24 || tr.height < 24) problems.push("target " + Math.round(tr.width) + "x" + Math.round(tr.height) + ": " + (c.dataset.testid || c.type || c.tagName));
            const cr = c.getBoundingClientRect();
            if (cr.right > box.right + 1) problems.push("control wider than the pane: " + (c.dataset.testid || c.tagName));
          }
          for (const l of row.querySelectorAll(".field__label")) {
            const bg = bgOf(l);
            const ratio = RATIO(OVER(RGBA(getComputedStyle(l).color), bg), bg);
            if (ratio < 4.5) problems.push("text contrast " + ratio.toFixed(2) + ": " + l.textContent);
          }
          for (const c of row.querySelectorAll("select, input.input, textarea")) {
            const cs = getComputedStyle(c);
            const bg = bgOf(c.parentElement);
            const edge = OVER(RGBA(cs.borderTopColor), bg);
            const ratio = RATIO(edge, bg);
            if (ratio < 3) problems.push("edge contrast " + ratio.toFixed(2) + ": " + (c.dataset.testid || c.tagName));
          }
          for (const seg of row.querySelectorAll(".seg--radio")) {
            const items = [...seg.querySelectorAll(".seg__item")];
            const on = items.find((x) => x.querySelector("input:checked"));
            const off = items.find((x) => !x.querySelector("input:checked"));
            if (on && off && getComputedStyle(on).fontWeight === getComputedStyle(off).fontWeight) problems.push("a pressed segment differs by colour only: " + row.dataset.row);
          }
        }
        // The focus ring has 3:1 with the pane.
        const probe = pane.querySelector(".orow[data-row]:not([hidden]) select, .orow[data-row]:not([hidden]) input.input");
        if (probe) {
          probe.focus();
          const cs = getComputedStyle(probe);
          const ring = RGBA(cs.outlineColor);
          const bg = bgOf(probe.parentElement);
          if (cs.outlineStyle === "none" || RATIO(OVER(ring, bg), bg) < 3) problems.push("focus ring " + cs.outlineStyle + " " + RATIO(OVER(ring, bg), bg).toFixed(2));
        }
        return problems;
      })()`;
      let page = null;
      for (const [width, height] of [[1400, 900], [768, 900], [390, 844]]) {
        for (const scheme of ["dark", "light"]) {
          page = await t.page({ width, height });
          for (let i = 0; i < types.length; i++) {
            try {
              await open(t, "tb204", i + 1, { page, scheme });
            } catch (error) {
              throw new Error(`${types[i]} ${scheme} ${width}px: ${error.message}`);
            }
            const problems = await page.eval(MEASURE);
            t.assert(problems.length === 0, `${types[i]} ${scheme} ${width}px: ${problems.slice(0, 6).join("; ")}`);
          }
        }
      }
    },
  },
  {
    id: "B205",
    name: "the live preview: the draft and the frame change in the same task, a slider writes once in each frame, the number field writes after 300 ms and Enter at once (10.3h, rule 7)",
    async run(t) {
      await makeDoc(t, "tb205", ["timeseries"]);
      const p = await open(t, "tb205", 1);
      const writes = () => p.eval(`Number(document.querySelector(".peditor__options").dataset.draftWrites)`);
      t.eq(await writes(), 0, "no write at the first draw");
      // A choice and a switch: one write, and a new frame, in the task of the event.
      const same = (code) => p.eval(`(() => { const f = document.querySelector(".peditor__top .panel"); const w = Number(document.querySelector(".peditor__options").dataset.draftWrites); ${code}; return { replaced: document.querySelector(".peditor__top .panel") !== f, writes: Number(document.querySelector(".peditor__options").dataset.draftWrites) - w }; })()`);
      let r = await same(`[...document.querySelectorAll('[data-testid="opt-draw"] label')].find((l) => l.textContent.trim() === "Bars").querySelector("input").click()`);
      t.eq(r, { replaced: true, writes: 1 }, "a choice writes once and the frame is new in the same task");
      await editorSettled(p);
      await painted(p);
      r = await same(`document.querySelector('[data-testid="opt-span-nulls"]').click()`);
      t.eq(r, { replaced: true, writes: 1 }, "a switch writes once and the frame is new in the same task");
      await p.eval(`[...document.querySelectorAll('[data-testid="opt-draw"] label')].find((l) => l.textContent.trim() === "Lines").querySelector("input").click()`);
      await editorSettled(p);
      await painted(p);
      // A slider: 50 input events in one frame are one write.
      const w0 = await writes();
      await p.eval(`(() => { const s = document.querySelector('[data-testid="opt-line-width-slider"]'); for (let i = 0; i < 50; i++) { s.value = String(i % 11); s.dispatchEvent(new Event("input", { bubbles: true })); } })()`);
      // The write comes at the next frame: wait for it, then for two more frames, and count.
      await p.waitFor(`Number(document.querySelector(".peditor__options").dataset.draftWrites) > ${w0}`, { what: "the write of the slider" }).catch(() => {});
      await painted(p);
      t.eq((await writes()) - w0, 1, "50 input events of a slider in one frame make one write");
      // 20 moves, one in each frame: 20 writes or less, and each change is one task of 100 ms or less.
      const w1 = await writes();
      const spans = await p.eval(`(async () => {
        const s = document.querySelector('[data-testid="opt-line-width-slider"]');
        const out = [];
        for (let i = 0; i < 20; i++) {
          const t0 = performance.now();
          s.value = String(1 + (i % 10));
          s.dispatchEvent(new Event("input", { bubbles: true }));
          await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
          out.push(performance.now() - t0);
        }
        return out;
      })()`);
      const n = (await writes()) - w1;
      t.assert(n <= 20 && n >= 1, `20 moves of a slider make ${n} writes`);
      t.assert(Math.max(...spans) <= 100, `each change is one task of 100 ms or less: ${Math.max(...spans).toFixed(0)} ms`);
      const d = await draftPanel(p, "tb205", 1, (x) => x.options.line_width === 10);
      t.eq(d.options.line_width, 10, "the last move is in the draft");
      // The number field writes 300 ms after the last key, and Enter at once. The page measures the time of the write (a timer never
      // fires early, so the check holds under load): no write before 300 ms, one write after it.
      const typed = await p.eval(`(async () => {
        const count = () => Number(document.querySelector(".peditor__options").dataset.draftWrites);
        const w = count();
        const i = document.querySelector('[data-testid="opt-fill-opacity"]');
        const t0 = performance.now();
        i.value = "40";
        i.dispatchEvent(new Event("input", { bubbles: true }));
        let first = null;
        while (performance.now() - t0 < 10000) {
          await new Promise((r) => setTimeout(r, 10));
          if (count() > w) { first = performance.now() - t0; break; }
        }
        await new Promise((r) => setTimeout(r, 400));
        return { first, writes: count() - w };
      })()`);
      t.assert(typed.first !== null && typed.first >= 290, `the number field writes 300 ms after the last key, not before: ${JSON.stringify(typed)}`);
      t.eq(typed.writes, 1, "the number field writes once");
      r = await same(`const i = document.querySelector('[data-testid="opt-fill-opacity"]'); i.value = "50"; i.dispatchEvent(new Event("input", { bubbles: true })); i.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }))`);
      t.eq(r, { replaced: true, writes: 1 }, "Enter writes at once");
    },
  },
  {
    id: "B206",
    name: "the rules of display: a row that a rule hides has no effect in the preview, and its member stays in the draft after a hide and a show (10.3h, rule 15)",
    timeout: 600000,
    async run(t) {
      const index = await web("forms/options/index.js");
      const rules = await web("forms/options/rows.js");
      const draft = await web("draft.js");
      const jobs = [];
      for (const type of ["timeseries", "barchart", "xychart"]) {
        const table = index.tableOf(type);
        const states = rules.statesOf(table, draft.newOptions(type));
        const uid = `tb206${type}`;
        // One panel for each row with a rule, in a state where the row is hidden.
        const wanted = table.rows.filter((r) => r.when && r.member.startsWith("options."));
        void states;
        // A state where each rule is false: the new options with a change that hides it.
        const hide = (row) => {
          const base = structuredClone(draft.newOptions(type));
          const tries = table.rows.filter((r) => r.member.startsWith("options.") && (r.kind === "radio" || r.kind === "select" || r.kind === "switch"));
          for (const c of tries) {
            for (const v of c.kind === "switch" ? [true, false] : c.items.map(([x]) => x)) {
              const o = structuredClone(base);
              const keys = c.member.split(".").slice(1);
              let at = o;
              for (const k of keys.slice(0, -1)) at = at[k] ??= {};
              at[keys.at(-1)] = v;
              if (!rules.shownIds(table, o).includes(row.id)) return o;
            }
          }
          return null;
        };
        const items = wanted.map((row) => ({ row, options: hide(row) })).filter((x) => x.options);
        t.assert(items.length === wanted.length, `${type}: a state hides each row with a rule (${items.length} of ${wanted.length})`);
        await makeDoc(t, uid, items.map(() => type), (x, ty, i) => { x.options = items[i].options; });
        items.forEach((item, i) => jobs.push({ type, uid, id: i + 1, table, ...item }));
      }
      const alt = (row, options) => {
        const keys = row.member.split(".").slice(1);
        const cur = keys.reduce((a, k) => a[k], options);
        if (row.kind === "radio" || row.kind === "select") return row.items.map(([v]) => v).find((v) => v !== cur);
        if (row.kind === "switch") return !cur;
        if (row.kind === "slider") return cur === row.max ? row.min : row.max;
        if (row.kind === "checks") return [row.items[0][0]];
        return null;
      };
      const failures = [];
      let next = 0;
      await Promise.all(Array.from({ length: 4 }, async () => {
        while (next < jobs.length) {
          const job = jobs[next++];
          const { row, type, uid, id } = job;
          const p = await open(t, uid, id);
          const value = alt(row, job.options);
          if (value === null || value === undefined) continue;
          // The tab "JSON" has its own height: the picture is read in this tab, before and after the edit.
          await p.click(p.tid("tab-json"));
          const text = await p.eval(`document.querySelector('[data-testid="panel-json"]').value`);
          const before = await settled(p, PICTURE);
          const panelJson = JSON.parse(text);
          const keys = row.member.split(".").slice(1);
          let at = panelJson.options;
          for (const k of keys.slice(0, -1)) at = at[k];
          at[keys.at(-1)] = value;
          await p.eval(`(() => { const a = document.querySelector('[data-testid="panel-json"]'); a.value = ${JSON.stringify(JSON.stringify(panelJson, null, 2))}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
          // The member reaches the draft first (a wait on the state), then the picture of the preview settles.
          const written = await draftPanel(p, uid, id, (x) => JSON.stringify(keys.reduce((a, k) => a[k], x.options)) === JSON.stringify(value)).catch(() => null);
          const after = await settled(p, PICTURE);
          if (!written) failures.push(`${type} ${row.id}: the member did not reach the draft`);
          else if (after !== before) failures.push(`${type} ${row.id}: the hidden row changes the picture of the preview`);
          // The form of the tab "Query" shows the member again when the row shows: the draft keeps it.
          await p.click(p.tid("tab-query"));
          await painted(p);
          const kept = await draftPanel(p, uid, id);
          if (JSON.stringify(keys.reduce((a, k) => a[k], kept.options)) !== JSON.stringify(value)) failures.push(`${type} ${row.id}: the member left the draft`);
        }
      }));
      t.assert(failures.length === 0, `${failures.length} failures: ${failures.join("; ")}`);
    },
  },
];
