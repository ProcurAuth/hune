// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B201 (10.3h, F43): the option forms of `stat`, `bargauge` and `piechart`. For each control, the test changes it like a person does and
// reads the draft (the member of the control, with a valid value) and the DOM of the preview (the proof that the picture changed).
// The rows that a rule hides, the error of a bad pattern in "Fields", and the optional members that leave the draft are checked too.
import { doc, panel, section, SQL } from "../lib/docs.mjs";
import { sleep } from "../lib/h.mjs";

const g = (x, y, w, h) => ({ x, y, w, h });
const legend = (over = {}) => ({ show: true, placement: "bottom", mode: "list", values: [], ...over });
const reduce = (over = {}) => ({ calc: "last_not_null", fields: null, values: false, ...over });
const FIELDS = { defaults: { color: { mode: "thresholds", fixed: null }, thresholds: { mode: "absolute", steps: [{ value: null, color: "green" }, { value: 20, color: "red" }] } }, overrides: [] };
// A stat with a time column: two number fields (two items), each one with a graph.
const STAT_SQL = ["SELECT event_time AS t, weight, blocks", "FROM analytics.seed", "ORDER BY t"];

const UID = "tb201";
const STAT = 1;
const BARGAUGE = 2;
const PIE = 3;

function document201() {
  return doc(UID, "Option forms B", [section(null, [
    panel(STAT, "stat", "Stat", STAT_SQL, { grid: g(0, 0, 12, 6), fields: FIELDS, options: { reduce: reduce(), color_mode: "value", graph_mode: "area", text_mode: "auto", orientation: "auto" } }),
    panel(BARGAUGE, "bargauge", "Bar gauge", SQL.bars, { grid: g(12, 0, 12, 6), fields: { defaults: { min: 0, max: 400 }, overrides: [] }, options: { reduce: reduce(), orientation: "horizontal", display_mode: "gradient", show_unfilled: true } }),
    panel(PIE, "piechart", "Pie", SQL.bars, { grid: g(0, 6, 12, 6), options: { pie_type: "pie", reduce: reduce({ values: true }), labels: [], legend: legend({ placement: "right" }), tooltip: { mode: "single" } } }),
  ])]);
}

// --- the page --------------------------------------------------------------------------------------------------------------------

const OPT = (id) => `[data-testid="opt-${id}"]`;
const TOP = ".peditor__top";

// The page keeps no draft when the draft equals the saved document, so the panel is the saved panel then.
const SAVED = document201().sections[0].panels;
const panelExpr = (id) => `(() => { try { const r = JSON.parse(localStorage.getItem("hune.draft.v1.${UID}")); if (r) { for (const s of r.document.sections) for (const x of s.panels) if (x.id === ${id}) return x; } } catch (e) {} return ${JSON.stringify(SAVED.find((x) => x.id === id))}; })()`;

/** The panel of the draft in the local storage, or the saved panel. */
const draftPanel = (p, id) => p.eval(panelExpr(id));

/** Wait until `pred(panel)` holds for the panel of the draft. `pred` is a function of a panel: it runs in the page. */
async function waitPanel(p, id, pred, what) {
  try {
    await p.waitFor(`(${pred.toString()})(${panelExpr(id)})`, { what });
  } catch (error) {
    const now = await draftPanel(p, id);
    throw new Error(`${error.message}; the draft has ${JSON.stringify({ options: now.options, defaults: now.fields.defaults })}`);
  }
}

/** Wait until an expression of the preview is true. */
const waitView = (p, expr, what) => p.waitFor(`(() => { try { return !!(${expr}); } catch (e) { return false; } })()`, { what, timeout: 8000 });

/** Open the editor of a panel on a fresh page and wait for the preview. */
async function openEditor(t, id) {
  const p = await t.page({ width: 1500, height: 1000 });
  await p.goto(t.dash(UID, `edit=1&editPanel=${id}`));
  await p.waitSel(p.tid("panel-editor"), { timeout: 20000 });
  await p.waitFor(`(() => { const e = document.querySelector("${TOP} .panel"); return e && !["idle", "queued", "loading"].includes(e.dataset.state); })()`, { what: "the preview" });
  await p.idle();
  return p;
}

/** Mark the control of a row (or an element inside it that `inner` selects) and return a selector for it. */
async function target(p, id, inner) {
  const ok = await p.eval(`(() => {
    for (const m of document.querySelectorAll("[data-b201-target]")) m.removeAttribute("data-b201-target");
    const root = document.querySelector(${JSON.stringify(OPT(id))});
    if (!root) return false;
    const e = root.matches(${JSON.stringify(inner)}) ? root : root.querySelector(${JSON.stringify(inner)});
    if (!e) return false;
    e.setAttribute("data-b201-target", "1");
    return true;
  })()`);
  if (!ok) throw new Error(`no control ${inner} in the row ${id}`);
  return "[data-b201-target]";
}

/** A choice: click the radio item with this text. */
const choose = (p, id, text) => p.clickText(`${OPT(id)} label`, text);
/** A check box of a list: click it by its text. */
const check = (p, id, text) => p.clickText(`${OPT(id)} label`, text);
/** A switch. */
const flip = async (p, id) => p.click(await target(p, id, 'input[type="checkbox"]'));
/** A select by value. */
const pick = async (p, id, value) => p.select(await target(p, id, "select"), value);
/** A text field: type, and wait for the write of 300 ms. */
async function typeText(p, id, text) {
  await p.fill(await target(p, id, 'input[type="text"], input:not([type]), textarea'), text);
  await sleep(450);
}
/** The row is hidden by a rule (the attribute `hidden` on it or above it), or it does not exist. */
const isHidden = (p, id) => p.eval(`(() => { const e = document.querySelector(${JSON.stringify(OPT(id))}); return !e || !!e.closest("[hidden]"); })()`);
/** The visible error text under the row. */
const errorOf = (p, id) => p.eval(`(() => {
  const e = document.querySelector(${JSON.stringify(OPT(id))});
  if (!e) return null;
  const box = e.closest("[data-option]") || e.parentElement;
  const m = [...box.querySelectorAll(".field-error")].find((x) => !x.hidden && x.textContent.trim());
  return m ? m.textContent.trim() : "";
})()`);
/** The id of the radio that is checked, as its text. */
const chosen = (p, id) => p.eval(`(() => { const e = document.querySelector(${JSON.stringify(OPT(id))}); const r = e && [...e.querySelectorAll('input[type="radio"]')].find((x) => x.checked); return r ? r.parentElement.textContent.trim() : null; })()`);

// The tiles of the preview.
const statTexts = (p) => p.eval(`[...document.querySelectorAll('${TOP} [data-testid="stat-item-${STAT}"]')].map((e) => ({ name: e.title, num: [...e.querySelectorAll(".stat__num")].map((x) => x.textContent).join(""), align: e.dataset.align, filled: e.classList.contains("stat__item--filled"), graph: !!e.querySelector(".stat__graph:not([hidden])"), titleHidden: e.querySelector(".stat__title").hidden, valueHidden: e.querySelector(".stat__value").hidden }))`);
const barItems = (p) => p.eval(`[...document.querySelectorAll('${TOP} .bargauge__item')].map((e) => ({ name: e.querySelector(".bargauge__name").textContent, value: e.querySelector(".bargauge__num").textContent, fills: e.querySelectorAll(".bargauge__fill").length, cells: e.querySelectorAll(".bargauge__cell").length, unfilled: e.querySelector(".bargauge__track").classList.contains("bargauge__track--unfilled") }))`);
const pieLegend = (p) => p.eval(`[...document.querySelectorAll('${TOP} .pie__legend-item')].map((e) => [...e.children].slice(2).map((x) => x.textContent))`);
const pieNames = (p) => p.eval(`[...document.querySelectorAll('${TOP} .pie__legend-name')].map((e) => e.textContent)`);
/** The alpha of the canvas of the pie at its centre (a donut has a hole there) and a hash of the whole canvas. */
const pieCanvas = (p) => p.eval(`(() => { const c = document.querySelector('${TOP} .pie__canvas'); const x = c.getContext("2d"); const d = x.getImageData(Math.floor(c.width / 2), Math.floor(c.height / 2), 1, 1).data; let h = 2166136261; const s = c.toDataURL(); for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return { centre: d[3], hash: (h >>> 0).toString(36) }; })()`);

export const cases = [
  {
    id: "B201",
    name: "the forms of stat, bargauge and piechart: the reduction, the text mode, the colour mode, the graph mode, the orientation, the display mode, the unfilled area, the pie type, the labels, the legend values and the legend sort (10.3h)",
    async run(t) {
      const created = await t.api.create("transit", document201());
      t.assert(created.status === 201, `create ${UID}: ${created.status} ${created.text.slice(0, 300)}`);
      await stat(t);
      await bargauge(t);
      await piechart(t);
    },
  },
];

// --- stat ------------------------------------------------------------------------------------------------------------------------

async function stat(t) {
  const p = await openEditor(t, STAT);
  const groups = await p.eval(`[...document.querySelectorAll(".peditor__obody .ogroup__title")].map((e) => e.textContent)`);
  t.assert(groups.join("|").includes("Value options|Stat styles"), `stat: the groups in order ${groups}`);
  for (const id of ["reduce-values", "reduce-calc", "reduce-fields", "orientation", "text-mode", "color-mode", "graph-mode", "justify", "display-name"]) t.assert(await p.exists(OPT(id)), `stat: the row ${id}`);
  for (const id of ["reduce-values", "reduce-calc", "reduce-fields", "orientation", "text-mode", "color-mode", "graph-mode", "justify", "display-name"]) {
    t.assert(await p.eval(`!!document.querySelector(${JSON.stringify(OPT(id))}).closest("[data-option]")`), `stat: ${id} is in an element with data-option`);
  }
  const base = await statTexts(p);
  t.eq(base.map((x) => x.name), ["weight", "blocks"], "stat: two items for two fields");
  t.assert(base.every((x) => x.graph), "stat: each item has a graph (graph mode area)");

  // The reduction: Show.
  t.eq(await chosen(p, "reduce-values"), "Calculate", "stat: Show is Calculate");
  await choose(p, "reduce-values", "All values");
  await waitPanel(p, STAT, (x) => x.options.reduce.values === true, "stat: values true");
  await waitView(p, `document.querySelectorAll('${TOP} [data-testid="stat-item-${STAT}"]').length > 2`, "stat: one item for each row");
  t.assert(await isHidden(p, "reduce-calc"), "stat: the row Calculation hides while values is true");
  t.eq((await draftPanel(p, STAT)).options.reduce.calc, "last_not_null", "stat: a hidden row keeps its member");
  t.eq(await p.eval(`document.querySelector('[data-testid="options-status"]').textContent.trim()`), "Hidden: Calculation, Show percent change.", "stat: the status line tells the hidden rows (P11)");
  await choose(p, "reduce-values", "Calculate");
  await waitView(p, `document.querySelectorAll('${TOP} [data-testid="stat-item-${STAT}"]').length === 2`, "stat: two items again");
  t.assert(!(await isHidden(p, "reduce-calc")), "stat: the row Calculation shows again");
  // Calculation.
  await pick(p, "reduce-calc", "max");
  await waitPanel(p, STAT, (x) => x.options.reduce.calc === "max", "stat: calc max");
  await pick(p, "reduce-calc", "first");
  await waitPanel(p, STAT, (x) => x.options.reduce.calc === "first", "stat: calc first");
  await waitView(p, `[...document.querySelectorAll('${TOP} .stat__num')].map((e) => e.textContent).join("") !== ${JSON.stringify(base.map((x) => x.num).join(""))}`, "stat: the numbers change with the calculation");
  await pick(p, "reduce-calc", "last_not_null");
  // Fields: a pattern, an invalid pattern (no write), and empty (null).
  await typeText(p, "reduce-fields", "/^weight$/");
  await waitPanel(p, STAT, (x) => x.options.reduce.fields === "/^weight$/", "stat: fields written");
  await waitView(p, `document.querySelectorAll('${TOP} [data-testid="stat-item-${STAT}"]').length === 1`, "stat: one item for one field");
  await typeText(p, "reduce-fields", "/(/");
  t.eq(await errorOf(p, "reduce-fields"), "The pattern is not valid.", "stat: the error of a bad pattern");
  t.eq((await draftPanel(p, STAT)).options.reduce.fields, "/^weight$/", "stat: a bad pattern writes nothing");
  await typeText(p, "reduce-fields", "");
  await waitPanel(p, STAT, (x) => x.options.reduce.fields === null, "stat: empty gives null");
  await waitView(p, `document.querySelectorAll('${TOP} [data-testid="stat-item-${STAT}"]').length === 2`, "stat: two items after the empty text");
  t.eq(await errorOf(p, "reduce-fields"), "", "stat: no error after a good text");

  // Stat styles.
  await choose(p, "orientation", "Vertical");
  await waitPanel(p, STAT, (x) => x.options.orientation === "vertical", "stat: orientation vertical");
  await waitView(p, `document.querySelector('${TOP} .stat').dataset.orientation === "row"`, "stat: vertical puts the items side by side");
  await choose(p, "orientation", "Horizontal");
  await waitView(p, `document.querySelector('${TOP} .stat').dataset.orientation === "column"`, "stat: horizontal stacks the items");
  await pick(p, "text-mode", "name");
  await waitPanel(p, STAT, (x) => x.options.text_mode === "name", "stat: text mode name");
  await waitView(p, `[...document.querySelectorAll('${TOP} .stat__num')].map((e) => e.textContent).join(",") === "weight,blocks"`, "stat: the big text is the name");
  await pick(p, "text-mode", "none");
  await waitView(p, `[...document.querySelectorAll('${TOP} .stat__value')].every((e) => e.hidden)`, "stat: no value with the mode none");
  await pick(p, "text-mode", "value_and_name");
  await waitView(p, `[...document.querySelectorAll('${TOP} .stat__title')].every((e) => !e.hidden)`, "stat: the names show with value_and_name");
  await pick(p, "text-mode", "value");
  await waitView(p, `[...document.querySelectorAll('${TOP} .stat__title')].every((e) => e.hidden)`, "stat: no name with the mode value");
  await pick(p, "text-mode", "auto");
  await waitPanel(p, STAT, (x) => x.options.text_mode === "auto", "stat: text mode auto");
  await pick(p, "color-mode", "background");
  await waitPanel(p, STAT, (x) => x.options.color_mode === "background", "stat: color mode background");
  await waitView(p, `document.querySelector('${TOP} .stat').dataset.colorMode === "background" && document.querySelector('${TOP} .stat__item--filled')`, "stat: the box has the colour");
  await pick(p, "color-mode", "none");
  await waitView(p, `document.querySelector('${TOP} .stat').dataset.colorMode === "none" && !document.querySelector('${TOP} .stat__item--filled')`, "stat: no colour");
  await pick(p, "color-mode", "value");
  await choose(p, "graph-mode", "None");
  await waitPanel(p, STAT, (x) => x.options.graph_mode === "none", "stat: graph mode none");
  await waitView(p, `!document.querySelector('${TOP} .stat__graph')`, "stat: no graph");
  await choose(p, "graph-mode", "Area");
  await waitView(p, `document.querySelector('${TOP} .stat__graph')`, "stat: the graph is back");
  // The text alignment is optional: Center adds the member, Auto removes it.
  t.eq(await chosen(p, "justify"), "Auto", "stat: justify shows its absent value");
  t.assert(!("justify" in (await draftPanel(p, STAT)).options), "stat: no justify in a new panel");
  await choose(p, "justify", "Center");
  await waitPanel(p, STAT, (x) => x.options.justify === "center", "stat: justify center");
  await waitView(p, `[...document.querySelectorAll('${TOP} [data-testid="stat-item-${STAT}"]')].every((e) => e.dataset.align === "center")`, "stat: the text is centred");
  await choose(p, "justify", "Auto");
  await waitPanel(p, STAT, (x) => !("justify" in x.options), "stat: Auto removes the member");
  // Display name: a text; empty removes the member.
  await typeText(p, "display-name", "Load");
  await waitPanel(p, STAT, (x) => x.fields.defaults.display_name === "Load", "stat: display name");
  await waitView(p, `[...document.querySelectorAll('${TOP} [data-testid="stat-item-${STAT}"]')].every((e) => e.title === "Load")`, "stat: the name of the items");
  await typeText(p, "display-name", "");
  await waitPanel(p, STAT, (x) => !("display_name" in x.fields.defaults), "stat: empty removes the display name");
  // The members of the document keep the order of 4.38.
  t.eq(Object.keys((await draftPanel(p, STAT)).options), ["reduce", "color_mode", "graph_mode", "text_mode", "orientation"], "stat: the order of the members");
}

// --- bargauge --------------------------------------------------------------------------------------------------------------------

async function bargauge(t) {
  const p = await openEditor(t, BARGAUGE);
  for (const id of ["reduce-values", "reduce-calc", "reduce-fields", "orientation", "display-mode", "show-unfilled", "display-name"]) t.assert(await p.exists(OPT(id)), `bargauge: the row ${id}`);
  const base = await barItems(p);
  t.eq(base.length, 1, "bargauge: one item for the one field");
  t.eq(await p.exists(`${OPT("orientation")} input[type="radio"]`), true, "bargauge: the orientation is a choice");
  t.eq(await p.eval(`[...document.querySelectorAll('${OPT("orientation")} label')].map((e) => e.textContent.trim())`), ["Horizontal", "Vertical"], "bargauge: no Auto in the orientation");
  await choose(p, "reduce-values", "All values");
  await waitPanel(p, BARGAUGE, (x) => x.options.reduce.values === true, "bargauge: values true");
  await waitView(p, `document.querySelectorAll('${TOP} .bargauge__item').length === 3`, "bargauge: one item for each row");
  t.assert(await isHidden(p, "reduce-calc"), "bargauge: Calculation hides");
  await choose(p, "reduce-values", "Calculate");
  await waitView(p, `document.querySelectorAll('${TOP} .bargauge__item').length === 1`, "bargauge: one item again");
  await pick(p, "reduce-calc", "min");
  await waitPanel(p, BARGAUGE, (x) => x.options.reduce.calc === "min", "bargauge: calc min");
  await waitView(p, `document.querySelector('${TOP} .bargauge__num').textContent !== ${JSON.stringify(base[0].value)}`, "bargauge: the value changes with the calculation");
  await pick(p, "reduce-calc", "last_not_null");
  await typeText(p, "reduce-fields", "/^zzz$/");
  await waitPanel(p, BARGAUGE, (x) => x.options.reduce.fields === "/^zzz$/", "bargauge: fields");
  await waitView(p, `document.querySelectorAll('${TOP} .bargauge__item').length === 0`, "bargauge: no item for a pattern that no field matches");
  await typeText(p, "reduce-fields", "[a");
  t.eq(await errorOf(p, "reduce-fields"), "The pattern is not valid.", "bargauge: the error of a bad pattern");
  t.eq((await draftPanel(p, BARGAUGE)).options.reduce.fields, "/^zzz$/", "bargauge: nothing is written");
  await typeText(p, "reduce-fields", "");
  await waitPanel(p, BARGAUGE, (x) => x.options.reduce.fields === null, "bargauge: empty gives null");
  await waitView(p, `document.querySelectorAll('${TOP} .bargauge__item').length === 1`, "bargauge: the item is back");

  await choose(p, "orientation", "Vertical");
  await waitPanel(p, BARGAUGE, (x) => x.options.orientation === "vertical", "bargauge: vertical");
  await waitView(p, `document.querySelector('${TOP} .bargauge').classList.contains("bargauge--vertical")`, "bargauge: the class of the vertical bar");
  await choose(p, "orientation", "Horizontal");
  await waitView(p, `document.querySelector('${TOP} .bargauge').classList.contains("bargauge--horizontal")`, "bargauge: the class of the horizontal bar");
  await choose(p, "display-mode", "Retro LCD");
  await waitPanel(p, BARGAUGE, (x) => x.options.display_mode === "lcd", "bargauge: lcd");
  await waitView(p, `document.querySelector('${TOP} .bargauge').dataset.mode === "lcd" && document.querySelectorAll('${TOP} .bargauge__cell').length > 3 && !document.querySelector('${TOP} .bargauge__fill')`, "bargauge: the cells of the lcd mode");
  // The row "Show unfilled area" shows in the mode lcd too, and it changes the track there (19.20m).
  t.assert(!(await isHidden(p, "show-unfilled")), "bargauge: Show unfilled area shows for lcd");
  await flip(p, "show-unfilled");
  await waitPanel(p, BARGAUGE, (x) => x.options.show_unfilled === false, "bargauge: unfilled false");
  await waitView(p, `!document.querySelector('${TOP} .bargauge__track').classList.contains("bargauge__track--unfilled")`, "bargauge: no unfilled track in lcd");
  await flip(p, "show-unfilled");
  await waitView(p, `document.querySelector('${TOP} .bargauge__track').classList.contains("bargauge__track--unfilled")`, "bargauge: the unfilled track is back in lcd");
  await choose(p, "display-mode", "Basic");
  await waitPanel(p, BARGAUGE, (x) => x.options.display_mode === "basic", "bargauge: basic");
  await waitView(p, `document.querySelector('${TOP} .bargauge').dataset.mode === "basic" && document.querySelector('${TOP} .bargauge__fill') && !document.querySelector('${TOP} .bargauge__cell')`, "bargauge: the plain fill of the basic mode");
  await flip(p, "show-unfilled");
  await waitView(p, `!document.querySelector('${TOP} .bargauge__track').classList.contains("bargauge__track--unfilled")`, "bargauge: no unfilled track in basic");
  await flip(p, "show-unfilled");
  await choose(p, "display-mode", "Gradient");
  await waitPanel(p, BARGAUGE, (x) => x.options.display_mode === "gradient", "bargauge: gradient");
  await typeText(p, "display-name", "Disk");
  await waitPanel(p, BARGAUGE, (x) => x.fields.defaults.display_name === "Disk", "bargauge: display name");
  await waitView(p, `document.querySelector('${TOP} .bargauge__name').textContent === "Disk"`, "bargauge: the name of the bar");
  await typeText(p, "display-name", "");
  await waitPanel(p, BARGAUGE, (x) => !("display_name" in x.fields.defaults), "bargauge: empty removes the display name");
  t.eq(Object.keys((await draftPanel(p, BARGAUGE)).options), ["reduce", "orientation", "display_mode", "show_unfilled"], "bargauge: the order of the members");
}

// --- piechart --------------------------------------------------------------------------------------------------------------------

async function piechart(t) {
  const p = await openEditor(t, PIE);
  const ids = ["reduce-values", "reduce-calc", "reduce-fields", "pie-type", "labels", "tooltip-mode", "legend-show", "legend-mode", "legend-placement", "display-name"];
  for (const id of ids) t.assert(await p.exists(OPT(id)), `pie: the row ${id}`);
  for (const id of ["tooltip-sort", "tooltip-hide-zeros"]) t.assert(!(await p.exists(OPT(id))), `pie: no row ${id}`);
  for (const id of ["legend-values", "legend-sort-by", "legend-sort-desc"]) t.assert(await isHidden(p, id), `pie: the row ${id} hides in the mode list`);
  t.eq(await p.eval(`[...document.querySelectorAll('${OPT("tooltip-mode")} label')].map((e) => e.textContent.trim())`), ["Single", "All", "Hidden"], "pie: the choice All stays in the form");

  // The reduction.
  t.eq(await pieNames(p), ["v2", "v1", "v0"], "pie: three slices for the three rows");
  await choose(p, "reduce-values", "Calculate");
  await waitPanel(p, PIE, (x) => x.options.reduce.values === false, "pie: values false");
  await waitView(p, `document.querySelectorAll('${TOP} .pie__legend-item').length === 1`, "pie: one slice for the one field");
  t.assert(!(await isHidden(p, "reduce-calc")), "pie: Calculation shows");
  await typeText(p, "display-name", "Share");
  await waitPanel(p, PIE, (x) => x.fields.defaults.display_name === "Share", "pie: display name");
  await waitView(p, `document.querySelector('${TOP} .pie__legend-name').textContent === "Share"`, "pie: the name of the slice");
  await typeText(p, "display-name", "");
  await waitPanel(p, PIE, (x) => !("display_name" in x.fields.defaults), "pie: empty removes the display name");
  await pick(p, "reduce-calc", "min");
  await waitPanel(p, PIE, (x) => x.options.reduce.calc === "min", "pie: calc min");
  await pick(p, "reduce-calc", "last_not_null");
  await choose(p, "reduce-values", "All values");
  await waitPanel(p, PIE, (x) => x.options.reduce.values === true, "pie: values true");
  await waitView(p, `document.querySelectorAll('${TOP} .pie__legend-item').length === 3`, "pie: three slices again");
  t.assert(await isHidden(p, "reduce-calc"), "pie: Calculation hides with All values");
  await typeText(p, "reduce-fields", "(?=a)");
  t.eq(await errorOf(p, "reduce-fields"), "The pattern is not valid.", "pie: the error of a bad pattern");
  t.eq((await draftPanel(p, PIE)).options.reduce.fields, null, "pie: nothing is written");
  await typeText(p, "reduce-fields", "");

  // The pie type: a donut has a hole at the centre.
  const pie0 = await pieCanvas(p);
  t.assert(pie0.centre > 0, "pie: the pie has colour at the centre");
  await choose(p, "pie-type", "Donut");
  await waitPanel(p, PIE, (x) => x.options.pie_type === "donut", "pie: donut");
  await waitView(p, `(() => { const c = document.querySelector('${TOP} .pie__canvas'); return c.getContext("2d").getImageData(Math.floor(c.width / 2), Math.floor(c.height / 2), 1, 1).data[3] === 0; })()`, "pie: the hole of the donut");
  await choose(p, "pie-type", "Pie");
  await waitView(p, `(() => { const c = document.querySelector('${TOP} .pie__canvas'); return c.getContext("2d").getImageData(Math.floor(c.width / 2), Math.floor(c.height / 2), 1, 1).data[3] > 0; })()`, "pie: no hole in the pie");
  // The labels: a check list of at most 3, the canvas changes.
  const plain = (await pieCanvas(p)).hash;
  await check(p, "labels", "Name");
  await waitPanel(p, PIE, (x) => JSON.stringify(x.options.labels) === '["name"]', "pie: label name");
  await waitView(p, `(() => { const c = document.querySelector('${TOP} .pie__canvas'); let h = 2166136261; const s = c.toDataURL(); for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return (h >>> 0).toString(36) !== ${JSON.stringify(plain)}; })()`, "pie: the labels change the canvas");
  await check(p, "labels", "Percent");
  await waitPanel(p, PIE, (x) => JSON.stringify(x.options.labels) === '["name","percent"]', "pie: labels name and percent");
  await check(p, "labels", "Value");
  await waitPanel(p, PIE, (x) => x.options.labels.length === 3, "pie: three labels");
  t.assert(await p.eval(`document.querySelector(${JSON.stringify(OPT("labels"))}).textContent.includes("3 of 3 chosen")`), "pie: the hint of the limit");
  await check(p, "labels", "Name");
  await check(p, "labels", "Value");
  await check(p, "labels", "Percent");
  await waitPanel(p, PIE, (x) => x.options.labels.length === 0, "pie: no label");

  // The tooltip: Hidden shows no card, Single shows the card, All equals Single (a pie has one slice under the pointer).
  await p.hover(`${TOP} .pie__canvas`, 0.5, 0.15);
  await waitView(p, `!document.querySelector('${TOP} .pie__tip').hidden`, "pie: the card of the single mode");
  await choose(p, "tooltip-mode", "Hidden");
  await waitPanel(p, PIE, (x) => x.options.tooltip.mode === "hidden", "pie: tooltip hidden");
  await p.hover(`${TOP} .pie__canvas`, 0.2, 0.5);
  await p.hover(`${TOP} .pie__canvas`, 0.5, 0.15);
  await sleep(200);
  t.assert(await p.eval(`document.querySelector('${TOP} .pie__tip').hidden`), "pie: no card in the mode hidden");
  await choose(p, "tooltip-mode", "All");
  await waitPanel(p, PIE, (x) => x.options.tooltip.mode === "multi", "pie: tooltip all");
  await p.hover(`${TOP} .pie__canvas`, 0.2, 0.5);
  await p.hover(`${TOP} .pie__canvas`, 0.5, 0.15);
  await waitView(p, `!document.querySelector('${TOP} .pie__tip').hidden`, "pie: the card of the mode All is the card of Single");
  await choose(p, "tooltip-mode", "Single");
  await waitPanel(p, PIE, (x) => x.options.tooltip.mode === "single", "pie: tooltip single");

  // The legend.
  await choose(p, "legend-placement", "Bottom");
  await waitPanel(p, PIE, (x) => x.options.legend.placement === "bottom", "pie: legend bottom");
  await waitView(p, `document.querySelector('${TOP} .pie').classList.contains("pie--bottom")`, "pie: the legend is below");
  await choose(p, "legend-placement", "Right");
  await waitView(p, `document.querySelector('${TOP} .pie').classList.contains("pie--right")`, "pie: the legend is at the right");
  await choose(p, "legend-mode", "Table");
  await waitPanel(p, PIE, (x) => x.options.legend.mode === "table", "pie: legend table");
  await waitView(p, `document.querySelector('${TOP} .pie__legend--table')`, "pie: the legend table");
  for (const id of ["legend-values", "legend-sort-by"]) t.assert(!(await isHidden(p, id)), `pie: the row ${id} shows in the mode table`);
  t.assert(await isHidden(p, "legend-sort-desc"), "pie: Sort descending hides while no sort is set");
  await check(p, "legend-values", "Percent");
  await waitPanel(p, PIE, (x) => JSON.stringify(x.options.legend.values) === '["percent"]', "pie: legend values percent");
  await waitView(p, `document.querySelectorAll('${TOP} .pie__legend-percent').length === 3`, "pie: the percent column");
  await check(p, "legend-values", "Value");
  await waitPanel(p, PIE, (x) => JSON.stringify(x.options.legend.values) === '["percent","value"]' || JSON.stringify(x.options.legend.values) === '["value","percent"]', "pie: legend values both");
  t.eq((await pieLegend(p))[0].length, 2, "pie: two columns after the name");
  await check(p, "legend-values", "Value");
  await check(p, "legend-values", "Percent");
  await waitPanel(p, PIE, (x) => x.options.legend.values.length === 0, "pie: no legend value");
  // The sort: by name ascending, then descending, then None removes both members.
  t.eq(await pieNames(p), ["v2", "v1", "v0"], "pie: the legend is in the order of the slices");
  await pick(p, "legend-sort-by", "name");
  await waitPanel(p, PIE, (x) => x.options.legend.sort_by === "name" && !("sort_desc" in x.options.legend), "pie: sort by name");
  await waitView(p, `[...document.querySelectorAll('${TOP} .pie__legend-name')].map((e) => e.textContent).join() === "v0,v1,v2"`, "pie: the legend sorted by name");
  t.assert(!(await isHidden(p, "legend-sort-desc")), "pie: Sort descending shows when a sort is set");
  await flip(p, "legend-sort-desc");
  await waitPanel(p, PIE, (x) => x.options.legend.sort_desc === true, "pie: sort descending");
  await waitView(p, `[...document.querySelectorAll('${TOP} .pie__legend-name')].map((e) => e.textContent).join() === "v2,v1,v0"`, "pie: the legend sorted from z to a");
  await pick(p, "legend-sort-by", "");
  await waitPanel(p, PIE, (x) => !("sort_by" in x.options.legend) && !("sort_desc" in x.options.legend), "pie: None removes sort_by and sort_desc");
  t.assert(await isHidden(p, "legend-sort-desc"), "pie: Sort descending hides again");
  await choose(p, "legend-mode", "List");
  await waitView(p, `document.querySelector('${TOP} .pie__legend--list')`, "pie: the legend list");
  // The visibility: the rows of the legend hide, and the legend leaves the preview.
  await flip(p, "legend-show");
  await waitPanel(p, PIE, (x) => x.options.legend.show === false, "pie: legend off");
  await waitView(p, `!document.querySelector('${TOP} .pie__legend')`, "pie: no legend");
  for (const id of ["legend-mode", "legend-placement"]) t.assert(await isHidden(p, id), `pie: the row ${id} hides with the legend off`);
  await flip(p, "legend-show");
  await waitView(p, `document.querySelector('${TOP} .pie__legend')`, "pie: the legend is back");
  t.eq(Object.keys((await draftPanel(p, PIE)).options), ["pie_type", "reduce", "labels", "legend", "tooltip"], "pie: the order of the members");
  t.eq(Object.keys((await draftPanel(p, PIE)).options.legend), ["show", "placement", "mode", "values"], "pie: the order of the legend members");
}
