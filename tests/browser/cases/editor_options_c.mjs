// Browser case B202 (10.3h, F43): the option forms of `table`, and of `heatmap` and `histogram` when the page has them.
// The table: the header switch, the wrap, the sort list (add, move, remove, 8 at most), the width, the alignment, the filter, the footer calculations
// (8 at most) and the cell type with its mode. Each control writes its member and the preview shows the effect. The heatmap and the histogram: the
// buckets and their grammar, the log scale that writes `size`, the colour mode, the steps, the gap, the bounds, `combine` and the rules of display.
import { doc, panel, section } from "../lib/docs.mjs";
import { draftOf, enterEdit, openPanelEditor } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const T0 = Date.UTC(2026, 10, 13);
const HOUR = 3600000;
const RANGE = { from: String(T0), to: String(T0 + 5 * HOUR), timezone: "utc", refresh: null };
const QUERY = "toDateTime('2026-11-13 00:00:00','UTC')";

const UID = "f43c-table";
const LONG = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon";
const SQL = [`SELECT number AS n, toString(number % 3) AS g, '${LONG}' AS txt, number * 1.5 AS x FROM numbers(12)`];

const ensure = async (t, d) => {
  if ((await t.api.dashboard(d.uid)).status === 200) return;
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create ${d.uid}: ${r.status} ${r.text.slice(0, 400)}`);
};

let BASE = null; // the saved panel: the draft has no record while it equals the saved document
/** The draft panel now (the first panel of the first section), or the saved panel when the draft has no record. */
const draftPanel = async (p, uid, index = 0) => {
  const d = await draftOf(p, uid);
  return d ? d.document.sections[0].panels[index] : BASE;
};
const until = async (p, uid, pred, index = 0) => {
  const end = Date.now() + 6000;
  for (;;) {
    const now = await draftPanel(p, uid, index);
    if (now && pred(now)) return now;
    if (Date.now() > end) throw new Error(`the draft never held ${pred} (it has ${JSON.stringify(now ? { options: now.options, defaults: now.fields.defaults } : null)})`);
    await sleep(100);
  }
};
const ctl = (p, id) => `${p.tid(id)}`;
const PREVIEW = ".peditor .vtable";
const headers = (p) => p.eval(`[...document.querySelectorAll('${PREVIEW} thead th .vtable__label')].map((e) => e.textContent)`);
const firstColumn = (p) => p.eval(`[...document.querySelectorAll('${PREVIEW} tbody tr[data-row]')].slice(0, 12).map((tr) => tr.children[0].textContent.trim())`);

async function tableForms(t) {
  const d = doc(UID, "F43 table forms", [section(null, [panel(1, "table", "Table", SQL, { grid: { x: 0, y: 0, w: 24, h: 10 } })])]);
  await ensure(t, d);
  BASE = d.sections[0].panels[0];
  const p = await t.page({ width: 1500, height: 1100 });
  await p.goto(t.dash(UID));
  await p.idle();
  await waitPanelsReady(p, [1]);
  await enterEdit(p);
  await openPanelEditor(p, 1);
  await p.waitSel(PREVIEW);

  // The groups come after "Visualisation", in the order of 10.3h, and the hooks are there.
  const titles = await p.eval(`[...document.querySelectorAll('.peditor__obody .ogroup__title')].map((e) => e.textContent)`);
  const at = (name) => titles.indexOf(name);
  t.assert(at("Visualisation") >= 0 && at("Table") === at("Visualisation") + 1 && at("Table footer") === at("Table") + 1 && at("Cell options") === at("Table footer") + 1 && at("Standard options") > at("Cell options"), `the groups ${titles}`);
  const hooks = ["opt-show-header", "opt-sort", "opt-col-width", "opt-col-align", "opt-wrap", "opt-filterable", "opt-footer", "defaults-cell-type", "opt-display-name"];
  t.eq(await p.eval(`${JSON.stringify(hooks)}.map((id) => !!document.querySelector('[data-testid="' + id + '"]'))`), Array(hooks.length).fill(true), "the controls of a table");
  t.eq(await p.eval(`${JSON.stringify(hooks)}.map((id) => !!document.querySelector('[data-testid="' + id + '"]').closest("[data-option]"))`), Array(hooks.length).fill(true), "each control is in an element with data-option");

  // The header switch.
  t.eq((await headers(p)).length, 4, "four columns with a header");
  await p.click(ctl(p, "opt-show-header"));
  t.eq((await until(p, UID, (x) => x.options.show_header === false)).options.show_header, false, "show_header false");
  t.eq(await p.count(`${PREVIEW} thead`), 0, "no header in the preview");
  await p.click(ctl(p, "opt-show-header"));
  await until(p, UID, (x) => x.options.show_header === true);
  await p.waitSel(`${PREVIEW} thead`);

  // Wrap: the member leaves the panel when the switch is off, and the class of the table shows the effect.
  await p.click(ctl(p, "opt-wrap"));
  t.eq((await until(p, UID, (x) => x.options.wrap === true)).options.wrap, true, "wrap true");
  await p.waitSel(`${PREVIEW}.vtable--wrap`);
  await p.click(ctl(p, "opt-wrap"));
  const nowrap = await until(p, UID, (x) => !("wrap" in x.options));
  t.eq(Object.keys(nowrap.options), ["show_header", "sort"], "wrap leaves the panel");
  await p.waitGone(`${PREVIEW}.vtable--wrap`);

  // The sort list: Add, the field name, Descending, move, remove, and 8 keys at most.
  t.eq(await firstColumn(p), ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11"], "the order of the data");
  await p.click(ctl(p, "opt-sort-add"));
  let sorted = await until(p, UID, (x) => x.options.sort.length === 1);
  t.assert(typeof sorted.options.sort[0].field === "string" && sorted.options.sort[0].desc === false, `a new key ${JSON.stringify(sorted.options.sort)}`);
  await p.fill(ctl(p, "opt-sort-0-field"), "n");
  await sleep(450);
  await p.click(ctl(p, "opt-sort-0-desc"));
  sorted = await until(p, UID, (x) => x.options.sort[0] && x.options.sort[0].field === "n" && x.options.sort[0].desc === true);
  t.eq(sorted.options.sort, [{ field: "n", desc: true }], "the key writes field and desc");
  await p.waitFor(`(() => { const r = document.querySelector('${PREVIEW} tbody tr[data-row]'); return r && r.children[0].textContent.trim() === "11"; })()`, { what: "the sort in the preview" });
  await p.click(ctl(p, "opt-sort-add"));
  await until(p, UID, (x) => x.options.sort.length === 2);
  await p.fill(ctl(p, "opt-sort-1-field"), "g");
  await sleep(450);
  await p.click(ctl(p, "opt-sort-1-up"));
  sorted = await until(p, UID, (x) => x.options.sort[0].field === "g");
  t.eq(sorted.options.sort.map((k) => k.field), ["g", "n"], "a key moves up");
  await p.click(ctl(p, "opt-sort-0-remove"));
  sorted = await until(p, UID, (x) => x.options.sort.length === 1);
  t.eq(sorted.options.sort.map((k) => k.field), ["n"], "a key leaves the list");
  for (let i = sorted.options.sort.length; i < 8; i++) await p.click(ctl(p, "opt-sort-add"));
  sorted = await until(p, UID, (x) => x.options.sort.length === 8);
  t.eq(await p.attr(ctl(p, "opt-sort-add"), "aria-disabled"), "true", "Add is not active at 8 keys");
  await p.click(ctl(p, "opt-sort-add"));
  await sleep(300);
  t.eq((await draftPanel(p, UID)).options.sort.length, 8, "a 9th key is not added");
  while ((await draftPanel(p, UID)).options.sort.length > 0) {
    const n = (await draftPanel(p, UID)).options.sort.length;
    await p.click(ctl(p, "opt-sort-0-remove"));
    await until(p, UID, (x) => x.options.sort.length === n - 1);
  }
  t.eq((await draftPanel(p, UID)).options.sort, [], "an empty list stays in the panel");

  // The width: a valid number, a number out of the range (an error, no write), and an empty field (the member leaves).
  await p.fill(ctl(p, "opt-col-width"), "150");
  let panelNow = await until(p, UID, (x) => x.fields.defaults.width === 150);
  t.eq(panelNow.fields.defaults.width, 150, "width 150");
  await p.waitFor(`[...document.querySelectorAll('${PREVIEW} colgroup col')].every((c) => c.style.width === "150px")`, { what: "the width in the preview" });
  await p.fill(ctl(p, "opt-col-width"), "9");
  await sleep(450);
  t.eq((await draftPanel(p, UID)).fields.defaults.width, 150, "width 9 is not written");
  t.assert(await p.eval(`!!document.querySelector('[data-option="fields.defaults.width"] [aria-invalid="true"], [data-option="fields.defaults.width"] .field-error:not([hidden])')`), "the error of the range shows");
  await p.fill(ctl(p, "opt-col-width"), "");
  await until(p, UID, (x) => !("width" in x.fields.defaults));

  // The alignment: the text of the cells follows.
  const align = () => p.eval(`getComputedStyle(document.querySelector('${PREVIEW} tbody tr[data-row] td')).textAlign`);
  const radio = (hook, text) => p.eval(`(() => { const g = document.querySelector('[data-testid="${hook}"]'); const l = [...g.querySelectorAll("label")].find((x) => x.textContent.trim() === ${JSON.stringify(text)}); if (!l) return false; l.click(); return true; })()`);
  t.assert(await radio("opt-col-align", "Right"), "the item Right");
  await until(p, UID, (x) => x.fields.defaults.align === "right");
  await p.waitFor(`getComputedStyle(document.querySelector('${PREVIEW} tbody tr[data-row] td')).textAlign === "right"`, { what: "right" });
  t.assert(await radio("opt-col-align", "Center"), "the item Center");
  await until(p, UID, (x) => x.fields.defaults.align === "center");
  await p.waitFor(`getComputedStyle(document.querySelector('${PREVIEW} tbody tr[data-row] td')).textAlign === "center"`, { what: "center" });
  t.assert(await radio("opt-col-align", "Left"), "the item Left");
  await until(p, UID, (x) => x.fields.defaults.align === "left");
  t.eq(await align(), "left", "left");

  // The filter.
  t.eq(await p.count(`${PREVIEW} .vtable__filter`), 0, "no filter button");
  await p.click(ctl(p, "opt-filterable"));
  await until(p, UID, (x) => x.fields.defaults.filterable === true);
  await p.waitFor(`document.querySelectorAll('${PREVIEW} .vtable__filter').length === 4`, { what: "four filter buttons" });
  await p.click(ctl(p, "opt-filterable"));
  await until(p, UID, (x) => !("filterable" in x.fields.defaults));
  await p.waitFor(`document.querySelectorAll('${PREVIEW} .vtable__filter').length === 0`, { what: "no filter button" });

  // The footer: the calculations (8 at most). An empty list removes the member.
  const boxes = `${p.tid("opt-footer")} input[type="checkbox"]`;
  const tick = (label) => p.eval(`(() => { const b = [...document.querySelectorAll(${JSON.stringify(boxes)})].find((i) => i.closest("label").textContent.trim() === ${JSON.stringify(label)}); if (!b || b.disabled) return false; b.click(); return true; })()`);
  t.assert(await tick("Total") && await tick("Max"), "two calculations");
  panelNow = await until(p, UID, (x) => x.fields.defaults.footer && x.fields.defaults.footer.reducers.length === 2);
  t.eq(panelNow.fields.defaults.footer, { reducers: ["total", "max"] }, "the footer members");
  await p.waitSel(`${PREVIEW} tfoot .vtable__foot-line`);
  for (const label of ["Min", "Mean", "Count", "Median", "First", "Last"]) t.assert(await tick(label), `tick ${label}`);
  panelNow = await until(p, UID, (x) => x.fields.defaults.footer.reducers.length === 8);
  t.eq(await p.eval(`[...document.querySelectorAll(${JSON.stringify(boxes)})].filter((i) => !i.checked).every((i) => i.disabled)`), true, "at 8 the other boxes are disabled");
  for (const label of ["Total", "Max", "Min", "Mean", "Count", "Median", "First", "Last"]) t.assert(await tick(label), `untick ${label}`);
  await until(p, UID, (x) => !("footer" in x.fields.defaults));
  await p.waitGone(`${PREVIEW} tfoot .vtable__foot-line`);

  // The cell type with its mode.
  await p.select(p.tid("defaults-cell-type"), "gauge");
  panelNow = await until(p, UID, (x) => x.fields.defaults.cell && x.fields.defaults.cell.type === "gauge");
  t.eq(panelNow.fields.defaults.cell, { type: "gauge", mode: "basic" }, "gauge writes the first mode");
  await p.waitSel(`${PREVIEW} .cell-gauge`);
  await p.select(p.tid("defaults-cell-mode"), "lcd");
  panelNow = await until(p, UID, (x) => x.fields.defaults.cell.mode === "lcd");
  t.eq(panelNow.fields.defaults.cell, { type: "gauge", mode: "lcd" }, "the mode lcd");
  await p.waitFor(`document.querySelector('${PREVIEW} .cell-gauge').dataset.mode === "lcd"`, { what: "the lcd gauge" });
  await p.select(p.tid("defaults-cell-type"), "json");
  panelNow = await until(p, UID, (x) => x.fields.defaults.cell.type === "json");
  t.eq(panelNow.fields.defaults.cell, { type: "json", mode: null }, "json has no mode");

  // The display name.
  await p.fill(ctl(p, "opt-display-name"), "Shown");
  await until(p, UID, (x) => x.fields.defaults.display_name === "Shown");
  t.assert((await headers(p)).every((h) => h === "Shown" || h.startsWith("Shown")), `the header shows the name ${await headers(p)}`);
  await p.fill(ctl(p, "opt-display-name"), "");
  await until(p, UID, (x) => !("display_name" in x.fields.defaults));
}

// --- the heatmap and the histogram (10.3h, 4.33c, 4.33d, 9.19e, 9.19f) -----------------------------------------------------------------

const RAW = (n) => `SELECT ${QUERY} + toUInt32(intDiv(number*18000, ${n})) AS time, round(40 * (1 + 0.6*sin(number/${n}*6.2832*2)) * -log(1 - (cityHash64(number) % 1000000 + 0.5)/1000000), 3) AS latency FROM numbers(${n}) ORDER BY time`;
const EXPS = (seed, scale, shift = 0) => `round(${scale}*-log(1-(cityHash64(number+${seed}) % 1000000 + 0.5)/1000000)${shift ? `+${shift}` : ""},3)`;
const TWO = (n) => `SELECT ${QUERY} + number AS time, ${EXPS(0, 40)} AS a, ${EXPS(7, 25, 10)} AS b FROM numbers(${n})`;
const COLOR = { mode: "scheme", scheme: "oranges", fill: "dark-orange", scale: "exponential", exponent: 0.5, steps: 64, reverse: false, min: null, max: null };
const HEAT = { calculate: true, x_buckets: { mode: "count", value: null }, y_buckets: { mode: "count", value: null, scale: "linear" }, y_axis: "left", y_reverse: false, y_min: null, y_max: null, color: COLOR, cell_gap: 1, hide_le: 0, hide_ge: null, show_legend: true, tooltip: { mode: "single" } };
const HIST = { bucket_count: 30, bucket_size: null, bucket_offset: 0, combine: false, stacking: "none", line_width: 1, fill_opacity: 80, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single" } };
const typed = (id, type, title, sql, options) => ({ ...panel(id, "table", title, [sql], { grid: { x: 0, y: 0, w: 24, h: 10 } }), type, options });

/** A helper kit for the editor of one panel: reads of the draft, the controls and the preview. */
function kit(t, p, uid, base) {
  let last = "start";
  const draft = async () => {
    const d = await draftOf(p, uid);
    return d ? d.document.sections[0].panels[0] : base;
  };
  const wait = async (pred, what) => {
    last = `wait ${what}`;
    const end = Date.now() + 8000;
    for (;;) {
      const now = await draft();
      if (pred(now)) return now;
      if (Date.now() > end) throw new Error(`${what}: the draft never held ${pred} (it has ${JSON.stringify(now.options)})`);
      await sleep(100);
    }
  };
  const hidden = (hook) => p.eval(`(() => { const e = document.querySelector('[data-testid="${hook}"]'); return !e || !!e.closest("[hidden]"); })()`);
  const shown = async (hooks) => (await Promise.all(hooks.map(hidden))).map((h) => !h);
  const radio = (hook, text) => p.eval(`(() => { const g = document.querySelector('[data-testid="${hook}"]'); const l = [...g.querySelectorAll("label")].find((x) => x.textContent.trim() === ${JSON.stringify(text)}); if (!l) return false; l.click(); return true; })()`);
  const type = async (hook, text) => {
    last = `type ${hook} ${JSON.stringify(text)}`;
    await p.fill(p.tid(hook), text);
    await sleep(450);
  };
  const value = (hook) => p.eval(`document.querySelector('[data-testid="${hook}"]').value`);
  const error = (member) => p.eval(`(() => { const e = document.querySelector('[data-option="${member}"] .field-error:not([hidden])'); return e ? e.textContent : ""; })()`);
  const hookOf = async (attr) => {
    const r = await p.eval(`(() => { const f = document.querySelector('.peditor [data-testid="panel-1"]'); const e = f && f.querySelector('.panel__plot'); return e && e.dataset.${attr} ? JSON.parse(e.dataset.${attr}) : { missing: f ? f.dataset.state + ": " + f.textContent.slice(0, 200) : "no frame" }; })()`);
    if (r && r.missing) throw new Error(`no hook ${attr} in the preview (${r.missing}) after ${last}`);
    return r;
  };
  // A print of the picture: the pixels of each canvas of the preview.
  const picture = () => p.eval(`[...document.querySelectorAll('.peditor [data-testid="panel-1"] canvas')].map((c) => { const d = c.toDataURL(); let h = 0; for (let i = 0; i < d.length; i += 7) h = (h * 31 + d.charCodeAt(i)) | 0; return d.length + ":" + h; }).join("|")`);
  const settle = async () => {
    await sleep(600);
    await p.idle();
  };
  const status = () => p.eval(`document.querySelector('[data-testid="options-status"]').textContent`);
  const toggle = (hook) => p.click(p.tid(hook));
  return { toggle, draft, wait, shown, hidden, radio, type, value, error, hookOf, picture, settle, status };
}

async function heatmapForms(t) {
  const uid = "f43c-heat";
  const base = typed(1, "heatmap", "Heat", RAW(2000), structuredClone(HEAT));
  await ensure(t, doc(uid, "F43 heatmap forms", [section(null, [base])], { time: RANGE }));
  const p = await t.page({ width: 1500, height: 1200 });
  await p.goto(t.url(`d/${uid}?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`));
  await p.idle();
  await waitPanelsReady(p, [1]);
  await enterEdit(p);
  await openPanelEditor(p, 1);
  await p.waitFor(`document.querySelector('.peditor [data-testid="panel-1"] .panel__plot').dataset.heatmap`, { what: "the heatmap in the preview", timeout: 30000 });
  const k = kit(t, p, uid, base);
  const opts = async () => (await k.draft()).options;

  // The groups and the rows: with `calculate` on, 22 rows show, and the colour rows follow the mode (scheme).
  const titles = await p.eval(`[...document.querySelectorAll('.peditor__obody .ogroup__title')].map((e) => e.textContent)`);
  const at = titles.indexOf("Heatmap");
  t.eq(titles.slice(at, at + 6), ["Heatmap", "Y axis", "Colors", "Cell display", "Tooltip", "Legend"], "the groups of a heatmap");
  const calc = ["heat-calculate", "heat-x-mode", "heat-x-value", "heat-y-mode", "heat-y-scale", "heat-y-value", "heat-y-axis", "heat-y-reverse", "heat-y-min", "heat-y-max", "heat-color-mode", "heat-scheme", "heat-steps", "heat-reverse", "heat-min", "heat-max", "heat-gap", "heat-hide-le", "heat-hide-ge", "opt-tooltip-mode", "heat-legend", "opt-display-name"];
  t.eq(await k.shown(calc), Array(calc.length).fill(true), "the rows with calculate on and the mode scheme");
  t.eq(await k.shown(["heat-fill", "heat-scale", "heat-exponent"]), [false, false, false], "the mode scheme has no fill, scale or exponent");
  t.eq(await p.eval(`['heat-fill','heat-scale','heat-exponent'].map((id) => { const w = document.querySelector('[data-testid="' + id + '"]').closest(".orow"); return w.hidden && w.dataset.hiddenByRule === "true"; })`), [true, true, true], "a hidden row has hidden and data-hidden-by-rule");

  // The buckets of the time axis: the mode Size writes 10m, the field shows the duration, the preview follows.
  const first = await k.hookOf("heatmap");
  t.assert(await k.radio("heat-x-mode", "Size"), "the item Size");
  let o = (await k.wait((x) => x.options.x_buckets.mode === "size", "x mode")).options;
  t.eq(o.x_buckets, { mode: "size", value: 600000 }, "the mode Size writes 10m in milliseconds");
  t.eq(await k.value("heat-x-value"), "10m", "the field shows the duration");
  await k.settle();
  t.eq((await k.hookOf("heatmap")).sx, 600000, "the preview has the size of 10 minutes");
  await k.type("heat-x-value", "30m");
  o = (await k.wait((x) => x.options.x_buckets.value === 1800000, "x value")).options;
  await k.settle();
  const x30 = await k.hookOf("heatmap");
  t.eq([x30.sx, x30.cols < first.cols + 100], [1800000, true], "30m gives the size 1,800,000 ms in the preview");
  for (const bad of ["1m30s", "0s", "366d", "600000", ""]) {
    await k.type("heat-x-value", bad);
    t.assert((await k.error("options.x_buckets.value")) !== "", `the text ${JSON.stringify(bad)} shows an error`);
    t.eq((await opts()).x_buckets.value, 1800000, `the text ${JSON.stringify(bad)} writes nothing`);
  }
  await k.type("heat-x-value", "365d");
  await k.wait((x) => x.options.x_buckets.value === 31536000000, "365d");
  t.assert(await k.radio("heat-x-mode", "Count"), "the item Count");
  o = (await k.wait((x) => x.options.x_buckets.mode === "count", "x count")).options;
  t.eq(o.x_buckets, { mode: "count", value: null }, "the mode Count clears the value");
  t.eq(await k.value("heat-x-value"), "", "the field is empty");
  await k.type("heat-x-value", "20");
  await k.wait((x) => x.options.x_buckets.value === 20, "x 20");
  for (const bad of ["0", "1001", "2.5", "10m"]) {
    await k.type("heat-x-value", bad);
    t.assert((await k.error("options.x_buckets.value")) !== "", `the count ${JSON.stringify(bad)} shows an error`);
  }
  t.eq((await opts()).x_buckets.value, 20, "an invalid count writes nothing");
  await k.type("heat-x-value", "");
  await k.wait((x) => x.options.x_buckets.value === null, "x empty");

  // The buckets of the value axis: the mode, the log scale that writes `size`, and the split.
  t.assert(await k.radio("heat-y-mode", "Size"), "Y Size");
  o = (await k.wait((x) => x.options.y_buckets.mode === "size", "y mode")).options;
  t.eq(o.y_buckets, { mode: "size", value: 1, scale: "linear" }, "the mode Size writes the value 1");
  await k.type("heat-y-value", "5");
  await k.wait((x) => x.options.y_buckets.value === 5, "y 5");
  await k.settle();
  t.eq((await k.hookOf("heatmap")).sy, 5, "the preview has the height of 5");
  await k.type("heat-y-value", "0");
  t.assert((await k.error("options.y_buckets.value")) !== "", "a size of 0 shows an error");
  t.assert(await k.radio("heat-y-scale", "Log base 2"), "Log base 2");
  o = (await k.wait((x) => x.options.y_buckets.scale === "log2", "log2")).options;
  t.eq(o.y_buckets, { mode: "size", value: 5, scale: "log2" }, "the log scale writes the mode size and keeps a split of 1 to 16");
  t.eq(await k.hidden("heat-y-mode"), true, "the log scale hides the row Y Bucket");
  await k.settle();
  t.eq((await k.hookOf("heatmap")).scale, "log2", "the preview has the log scale");
  await k.type("heat-y-value", "17");
  t.assert((await k.error("options.y_buckets.value")) !== "", "a split of 17 shows an error");
  await k.type("heat-y-min", "0");
  t.assert((await k.error("options.y_min")) !== "", "a log scale refuses 0 as the lowest value");
  await k.type("heat-y-value", "");
  await k.wait((x) => x.options.y_buckets.value === null, "split empty");
  t.assert(await k.radio("heat-y-scale", "Log base 10"), "Log base 10");
  await k.wait((x) => x.options.y_buckets.scale === "log10", "log10");
  t.assert(await k.radio("heat-y-scale", "Linear"), "Linear");
  o = (await k.wait((x) => x.options.y_buckets.scale === "linear", "linear")).options;
  t.eq(o.y_buckets, { mode: "size", value: 1, scale: "linear" }, "the linear scale after a log scale keeps the mode size with a value above 0");
  t.eq(await k.hidden("heat-y-mode"), false, "the row Y Bucket shows again");
  t.assert(await k.radio("heat-y-mode", "Count"), "Y Count");
  o = (await k.wait((x) => x.options.y_buckets.mode === "count", "y count")).options;
  t.eq(o.y_buckets, { mode: "count", value: 1, scale: "linear" }, "the mode Count keeps a whole value");

  // The bounds of the axis: the picture follows.
  const p0 = await k.picture();
  await k.type("heat-y-min", "10");
  await k.type("heat-y-max", "100");
  o = (await k.wait((x) => x.options.y_min === 10 && x.options.y_max === 100, "y bounds")).options;
  await k.settle();
  t.assert((await k.picture()) !== p0, "the bounds of the y axis change the picture");
  await k.type("heat-y-min", "");
  await k.type("heat-y-max", "");
  await k.wait((x) => x.options.y_min === null && x.options.y_max === null, "y bounds empty");
  t.assert(await k.radio("heat-y-axis", "Hidden"), "y axis hidden");
  await k.wait((x) => x.options.y_axis === "hidden", "y axis");
  await k.toggle("heat-y-reverse");
  await k.wait((x) => x.options.y_reverse === true, "y reverse");

  // The colours: the mode Opacity shows the colour, the scale and the exponent, and hides the scheme.
  const palette0 = (await k.hookOf("heatmap")).palette;
  await p.select(p.tid("heat-scheme"), "viridis");
  await k.wait((x) => x.options.color.scheme === "viridis", "scheme");
  await k.settle();
  t.assert(JSON.stringify((await k.hookOf("heatmap")).palette) !== JSON.stringify(palette0), "the scheme changes the palette in the preview");
  t.assert(await k.radio("heat-color-mode", "Opacity"), "mode Opacity");
  o = (await k.wait((x) => x.options.color.mode === "opacity", "color mode")).options;
  t.eq(o.color, { ...COLOR, scheme: "viridis", mode: "opacity" }, "the other members of the colour stay");
  t.eq(await k.shown(["heat-scheme", "heat-fill", "heat-scale", "heat-exponent"]), [false, true, true, true], "the mode Opacity shows fill, scale and exponent");
  t.assert((await k.status()).includes("Shown:") && (await k.status()).includes("Hidden:"), `the status line ${await k.status()}`);
  await k.type("heat-exponent", "0.3");
  await k.wait((x) => x.options.color.exponent === 0.3, "exponent");
  await k.type("heat-exponent", "0.25");
  t.assert((await k.error("options.color.exponent")) !== "", "an exponent with two decimals shows an error");
  t.eq((await opts()).color.exponent, 0.3, "and writes nothing");
  t.assert(await k.radio("heat-scale", "Linear"), "scale Linear");
  o = (await k.wait((x) => x.options.color.scale === "linear", "scale")).options;
  t.eq([o.color.exponent, await k.hidden("heat-exponent")], [0.3, true], "the scale Linear hides the exponent and keeps its value");

  // The steps (a slider with a number field), the reverse switch and the bounds of the colour scale.
  await k.type("heat-steps", "8");
  await k.wait((x) => x.options.color.steps === 8, "steps");
  await k.settle();
  t.eq((await k.hookOf("heatmap")).steps, 8, "the preview has 8 steps");
  await k.type("heat-steps", "129");
  t.assert((await k.error("options.color.steps")) !== "", "129 steps show an error");
  await k.type("heat-steps", "8");
  await k.type("heat-min", "5");
  await k.wait((x) => x.options.color.min === 5, "color min");
  await k.type("heat-max", "5");
  t.assert((await k.error("options.color.max")) !== "", "a colour end equal to the start shows an error");
  await k.type("heat-max", "50");
  await k.wait((x) => x.options.color.max === 50, "color max");
  await k.settle();
  const legend = (await k.hookOf("heatmap")).legend;
  t.eq(legend, [5, 50], "the colour bar has the bounds");
  await k.type("heat-min", "");
  await k.type("heat-max", "");
  await k.wait((x) => x.options.color.min === null && x.options.color.max === null, "color bounds empty");

  // The cells: the gap, and the rules that hide cells.
  const p1 = await k.picture();
  await k.type("heat-gap", "6");
  await k.wait((x) => x.options.cell_gap === 6, "gap");
  await k.settle();
  t.assert((await k.picture()) !== p1, "the gap changes the picture");
  await k.type("heat-gap", "26");
  t.assert((await k.error("options.cell_gap")) !== "", "a gap of 26 shows an error");
  const before = await k.hookOf("heatmap");
  await k.type("heat-hide-ge", String(before.max));
  await k.wait((x) => x.options.hide_ge === before.max, "hide ge max");
  await k.settle();
  const less = (await k.hookOf("heatmap")).drawn;
  t.assert(before.drawn > 0 && less > 0 && less < before.drawn, `hide >= the highest count hides some cells (${before.drawn} and ${less})`);
  await k.type("heat-hide-ge", "1");
  await k.wait((x) => x.options.hide_ge === 1, "hide ge 1");
  await k.settle();
  t.eq(await p.eval(`document.querySelector('.peditor [data-testid="panel-1"]').dataset.state`), "empty", "a rule that hides each cell leaves the state empty");
  await k.type("heat-hide-ge", "");
  await k.wait((x) => x.options.hide_ge === null, "hide ge empty");
  await k.settle();
  t.eq((await k.hookOf("heatmap")).drawn, before.drawn, "an empty rule shows the cells again");
  await k.type("heat-hide-le", "");
  await k.wait((x) => x.options.hide_le === null, "hide le empty");

  // The legend, the tooltip and the display name.
  await k.toggle("heat-legend");
  await k.wait((x) => x.options.show_legend === false, "legend off");
  await k.settle();
  t.eq((await k.hookOf("heatmap")).legend, null, "no colour bar");
  t.assert(await k.radio("opt-tooltip-mode", "Hidden"), "tooltip hidden");
  await k.wait((x) => x.options.tooltip.mode === "hidden", "tooltip");
  await k.type("opt-display-name", "Latency");
  await k.wait((x) => x.fields.defaults.display_name === "Latency", "display name");

  // `calculate` off hides the rows of the buckets and the bounds, and the draft keeps their members.
  t.assert(await k.radio("heat-calculate", "No"), "calculate No");
  o = (await k.wait((x) => x.options.calculate === false, "calculate")).options;
  t.eq(await k.shown(["heat-x-mode", "heat-x-value", "heat-y-mode", "heat-y-scale", "heat-y-value", "heat-y-min", "heat-y-max", "heat-y-axis", "heat-steps"]), [false, false, false, false, false, false, false, true, true], "calculate No hides the rows of the buckets");
  t.eq(o.y_buckets, { mode: "count", value: 1, scale: "linear" }, "the hidden rows keep their members");
  t.assert((await k.status()).includes("Hidden:"), `the status line ${await k.status()}`);
}

async function histogramForms(t) {
  const uid = "f43c-hist";
  const base = typed(1, "histogram", "Hist", TWO(2000), structuredClone(HIST));
  await ensure(t, doc(uid, "F43 histogram forms", [section(null, [base])], { time: RANGE }));
  const p = await t.page({ width: 1500, height: 1200 });
  await p.goto(t.url(`d/${uid}?from=${T0}&to=${T0 + 5 * HOUR}&timezone=utc`));
  await p.idle();
  await waitPanelsReady(p, [1]);
  await enterEdit(p);
  await openPanelEditor(p, 1);
  await p.waitFor(`document.querySelector('.peditor [data-testid="panel-1"] .panel__plot').dataset.histogram`, { what: "the histogram in the preview", timeout: 30000 });
  const k = kit(t, p, uid, base);
  const opts = async () => (await k.draft()).options;
  const titles = await p.eval(`[...document.querySelectorAll('.peditor__obody .ogroup__title')].map((e) => e.textContent)`);
  const at = titles.indexOf("Histogram");
  t.eq(titles.slice(at, at + 3), ["Histogram", "Tooltip", "Legend"], "the groups of a histogram");
  t.eq(await k.shown(["hist-count", "hist-size", "hist-offset", "hist-combine", "hist-stacking", "hist-line-width", "hist-fill-opacity", "opt-tooltip-mode", "opt-legend-show", "opt-display-name"]), Array(10).fill(true), "the rows of a new histogram");
  t.eq(await k.shown(["opt-tooltip-sort", "opt-legend-values"]), [false, false], "the tooltip sort and the legend values are hidden");
  const first = await k.hookOf("histogram");
  t.eq(first.series.length, 2, "two series");

  // The bucket count: a number from 1 to 1000. The size replaces it.
  await k.type("hist-count", "10");
  await k.wait((x) => x.options.bucket_count === 10, "count");
  await k.settle();
  const ten = await k.hookOf("histogram");
  t.assert(ten.buckets < first.buckets, `10 buckets give fewer buckets than 30 (${ten.buckets} and ${first.buckets})`);
  for (const bad of ["0", "1001", "2.5", "x"]) {
    await k.type("hist-count", bad);
    t.assert((await k.error("options.bucket_count")) !== "", `the count ${JSON.stringify(bad)} shows an error`);
  }
  t.eq((await opts()).bucket_count, 10, "an invalid count writes nothing");
  await k.type("hist-count", "10");
  await k.type("hist-size", "20");
  await k.wait((x) => x.options.bucket_size === 20, "size");
  await k.settle();
  t.eq([(await k.hookOf("histogram")).size, await k.hidden("hist-count"), (await opts()).bucket_count], [20, true, 10], "the size 20 hides the count and keeps its value");
  for (const bad of ["0", "-1", "x"]) {
    await k.type("hist-size", bad);
    t.assert((await k.error("options.bucket_size")) !== "", `the size ${JSON.stringify(bad)} shows an error`);
  }
  t.eq((await opts()).bucket_size, 20, "an invalid size writes nothing");
  await k.type("hist-offset", "5");
  await k.wait((x) => x.options.bucket_offset === 5, "offset");
  await k.settle();
  const off = await k.hookOf("histogram");
  t.eq(((off.lo - 5) / 20) % 1, 0, `the edges of the buckets are the offset 5 plus whole sizes (lo ${off.lo})`);
  await k.type("hist-offset", "");
  t.assert((await k.error("options.bucket_offset")) !== "", "an empty offset shows an error");
  await k.type("hist-offset", "-12.5");
  await k.wait((x) => x.options.bucket_offset === -12.5, "negative offset");
  await k.type("hist-size", "");
  await k.wait((x) => x.options.bucket_size === null, "size empty");
  t.eq(await k.hidden("hist-count"), false, "an empty size shows the count again");

  // The series and the stack: combine makes one series and hides the stacking.
  const pic0 = await k.picture();
  t.assert(await k.radio("hist-stacking", "Normal"), "stacking Normal");
  await k.wait((x) => x.options.stacking === "normal", "stacking");
  await k.settle();
  const pic1 = await k.picture();
  t.assert(pic1 !== pic0, "the stacking changes the picture");
  await k.toggle("hist-combine");
  const o = (await k.wait((x) => x.options.combine === true, "combine")).options;
  await k.settle();
  t.eq([(await k.hookOf("histogram")).series.length, await k.hidden("hist-stacking"), o.stacking], [1, true, "normal"], "combine makes one series, hides the stacking and keeps its value");
  await k.toggle("hist-combine");
  await k.wait((x) => x.options.combine === false, "combine off");
  t.assert(await k.radio("hist-stacking", "Off"), "stacking Off");
  await k.wait((x) => x.options.stacking === "none", "stacking off");

  // The line and the fill.
  await k.settle();
  const pic2 = await k.picture();
  await k.type("hist-line-width", "6");
  await k.wait((x) => x.options.line_width === 6, "line width");
  await k.settle();
  const pic3 = await k.picture();
  t.assert(pic3 !== pic2, "the line width changes the picture");
  await k.type("hist-fill-opacity", "10");
  await k.wait((x) => x.options.fill_opacity === 10, "fill opacity");
  await k.settle();
  t.assert((await k.picture()) !== pic3, "the fill opacity changes the picture");
  await k.type("hist-line-width", "11");
  t.assert((await k.error("options.line_width")) !== "", "a line width of 11 shows an error");
  await k.type("hist-fill-opacity", "101");
  t.assert((await k.error("options.fill_opacity")) !== "", "a fill opacity of 101 shows an error");
  t.eq([(await opts()).line_width, (await opts()).fill_opacity], [6, 10], "an invalid value writes nothing");

  // The tooltip and the legend rows (the shared rows) and the display name.
  t.assert(await k.radio("opt-tooltip-mode", "All"), "tooltip All");
  await k.wait((x) => x.options.tooltip.mode === "multi", "tooltip");
  t.eq(await k.shown(["opt-tooltip-sort", "opt-tooltip-hide-zeros"]), [true, true], "the mode All shows the sort and the zeros");
  t.assert(await k.radio("opt-legend-mode", "Table"), "legend Table");
  await k.wait((x) => x.options.legend.mode === "table", "legend mode");
  t.eq(await k.shown(["opt-legend-values", "opt-legend-sort-by"]), [true, true], "the legend table shows the values and the sort");
  await k.type("opt-display-name", "Count");
  await k.wait((x) => x.fields.defaults.display_name === "Count", "display name");
}

export const cases = [
  {
    id: "B202",
    name: "the forms of `table`, `heatmap` and `histogram`: the header, wrap, the sort list (8 keys at most), width, alignment, filter, the footer (8 calculations at most) and the cell type (10.3h)",
    async run(t) {
      await tableForms(t);
      await heatmapForms(t);
      await histogramForms(t);
    },
  },
];
