// Tests of the placement of a variable in the grid (4.20, 9.10, 10.3b, 10.6): the pure rules, the draft, the draft runtime and the static rules.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { DraftRuntime } from "../../web/js/editor/draft_runtime.js";
import {
  PANEL_TYPES, PLACEMENT_TITLE_MAX, defaultPlacement, deleteSection, isDrawable, isPlacedPanel, movePlacement, moveSection, placeInSection, placementRefusal, placementTitleError, sectionBottom, setPlacement, variableIndexOf,
} from "../../web/js/editor/draft.js";
import { dragPanel, resizePanel } from "../../web/js/editor/layout.js";
import { placedMenu } from "../../web/js/editor/place.js";
import {
  barNames, gridDoc, groupLabel, labelOf, placedId, placedPanel, placedVariables, planHomes, variableNameOf,
} from "../../web/js/var_layout.js";
import { splitLayout } from "../../web/js/editor/draft.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const custom = (name, extra = {}) => ({ kind: "custom", name, label: null, hide: "none", options: [{ text: "A", value: "a" }, { text: "B", value: "b" }], multi: false, include_all: false, all_value: null, default: ["a"], ...extra });
const stat = (id, grid) => ({ id, type: "stat", title: `s${id}`, description: "", transparent: false, grid, transforms: [], fields: { defaults: {}, overrides: [] }, options: {}, links: [], data: { query: { source: "s", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null } } });
const doc = (variables, sections) => ({ format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off", time: { from: "now-6h", to: "now", timezone: "utc", refresh: null }, links: [], variables, sections });
const sec = (title, panels = []) => ({ title, collapsed: false, panels });
const at = (section, x, y, w = 6, h = 2, extra = {}) => ({ section, x, y, w, h, ...extra });

// --- the pure rules (var_layout.js) ---------------------------------------------------------------------------------------------------

test("a placement becomes a placed panel with a negative id, the title of the placement and the grid of the placement", () => {
  const v = custom("env", { placement: at(0, 3, 4, 5, 2, { title: "Environment" }) });
  const p = placedPanel(v, 2);
  assert.deepEqual(p, { id: -3, type: "variable", title: "Environment", description: "", transparent: false, grid: { x: 3, y: 4, w: 5, h: 2 }, options: { variable: "env" } });
  assert.equal(placedPanel(custom("a", { placement: at(0, 0, 0) }), 0).title, "", "no title: the empty text");
  assert.equal(isPlacedPanel(p), true);
  assert.equal(isPlacedPanel({ type: "stat" }), false);
  assert.equal(isPlacedPanel(null), false);
  assert.equal(variableNameOf(p), "env");
  assert.equal(variableNameOf({}), "");
  assert.equal(variableIndexOf(p), 2);
  assert.equal(placedId(0), -1);
  assert.equal(placedId(1), -2);
  assert.equal(labelOf({ name: "env", label: null }), "env");
  assert.equal(labelOf({ name: "env", label: "Environment" }), "Environment");
  assert.equal(groupLabel("Mine", { name: "e", label: "E" }), "Mine");
  assert.equal(groupLabel("", { name: "e", label: "E" }), "E");
  assert.equal(PLACEMENT_TITLE_MAX, 256);
});

test("the grid document: the placed panels join the panels of their section; a document with no placement is the same object", () => {
  const plain = doc([custom("a")], [sec(null, [stat(1, { x: 0, y: 0, w: 4, h: 4 })])]);
  assert.equal(gridDoc(plain), plain);
  const d = doc([custom("a", { placement: at(1, 0, 0) }), custom("b"), custom("c", { placement: at(0, 6, 0, 3, 2) }), custom("lost", { placement: at(9, 0, 0) })],
    [sec(null, [stat(1, { x: 0, y: 0, w: 4, h: 4 })]), sec("Two")]);
  const g = gridDoc(d);
  assert.deepEqual(g.sections[0].panels.map((p) => p.id), [1, -3]);
  assert.deepEqual(g.sections[1].panels.map((p) => p.id), [-1]);
  assert.equal(d.sections[0].panels.length, 1, "the document is not changed");
  assert.deepEqual(placedVariables(d).map((x) => [x.variable.name, x.index]), [["a", 0], ["c", 2], ["lost", 3]]);
  assert.equal(JSON.stringify(g.sections).includes("lost"), false, "a placement in a section that does not exist draws nothing: the variable goes to the bar");
});

test("a new layout of a section: the panels stay in the section, the placed panels go back to the placements", () => {
  const d = doc([custom("a", { placement: at(0, 0, 4, 6, 2, { title: "T" }) }), custom("b")], [sec(null, [stat(1, { x: 0, y: 0, w: 4, h: 4 })]), sec("Two")]);
  const panels = gridDoc(d).sections[0].panels.map((p) => (p.id === -1 ? { ...p, grid: { x: 5, y: 6, w: 7, h: 3 } } : p.id === 1 ? { ...p, grid: { x: 0, y: 1, w: 4, h: 4 } } : p));
  const next = splitLayout(d, 0, panels);
  assert.deepEqual(next.variables[0].placement, { section: 0, x: 5, y: 6, w: 7, h: 3, title: "T" });
  assert.deepEqual(next.sections[0].panels.map((p) => [p.id, p.grid.y]), [[1, 1]]);
  assert.deepEqual(d.variables[0].placement, at(0, 0, 4, 6, 2, { title: "T" }), "the input is not changed");
  assert.equal(JSON.stringify(next.sections).includes("variable"), false, "no placed panel goes into a document");
});

test("the drag code moves and resizes a placed panel, and the document gets x, y, w and h of the placement only (10.16)", () => {
  const d = doc([custom("a", { placement: at(0, 0, 4, 6, 2) })], [sec(null, [stat(1, { x: 0, y: 0, w: 12, h: 4 })])]);
  const panels = gridDoc(d).sections[0].panels;
  const moved = dragPanel(panels, -1, 10, 4);
  assert.ok(moved);
  const afterMove = splitLayout(d, 0, moved.panels);
  assert.deepEqual([afterMove.variables[0].placement.x, afterMove.variables[0].placement.w], [10, 6]);
  assert.deepEqual(afterMove.sections[0].panels[0].grid, { x: 0, y: 0, w: 12, h: 4 }, "the panel stays");
  const sized = resizePanel(panels, -1, 9, 3);
  assert.ok(sized);
  const afterSize = splitLayout(d, 0, sized.panels);
  assert.deepEqual([afterSize.variables[0].placement.w, afterSize.variables[0].placement.h], [9, 3]);
  assert.equal(afterSize.variables[0].placement.section, 0);
  // A placed panel pushes a panel (the overlap rule of the panels applies to it).
  const push = dragPanel(panels, -1, 0, 0);
  assert.ok(push);
  const pushed = splitLayout(d, 0, push.panels);
  assert.ok(pushed.sections[0].panels[0].grid.y >= pushed.variables[0].placement.h || pushed.variables[0].placement.y >= 4, "no overlap after the push");
});

test("where the controls go: the first usable slot, else the bar, never the bar for hide = variable (9.10)", () => {
  const vars = [custom("a"), custom("b"), custom("c", { hide: "variable" }), custom("d")];
  const slots = new Map([["a", [{ usable: true }]], ["b", [{ usable: false }]], ["c", [{ usable: true }]], ["z", [{ usable: false }]]]);
  const homes = planHomes(vars, slots);
  assert.deepEqual(homes.get("a"), { slot: 0 });
  assert.deepEqual(homes.get("b"), { bar: true }, "a placement in a grid that is not shown: the bar");
  assert.deepEqual(homes.get("c"), { slot: 0 });
  assert.deepEqual(homes.get("d"), { bar: true });
  assert.deepEqual(homes.get("z"), { none: true });
  assert.deepEqual(barNames(vars, homes), ["b", "d"]);
  assert.deepEqual(planHomes([custom("a")], new Map([["a", [{ usable: false }, { usable: true }, { usable: true }]]])).get("a"), { slot: 1 });
  assert.deepEqual(planHomes([custom("c", { hide: "variable" })], new Map()).get("c"), { none: true });
  assert.deepEqual(barNames([custom("y"), custom("x")], planHomes([custom("y"), custom("x")], new Map())), ["y", "x"]);
});

// --- the draft (editor/draft.js) ------------------------------------------------------------------------------------------------------

test("a variable goes to the grid at the bottom of a section, 6 columns, 2 rows; a placement is removed with null (10.6)", () => {
  const d = doc([custom("a"), custom("b", { placement: at(1, 0, 0, 6, 2) })], [sec(null, [stat(1, { x: 0, y: 0, w: 4, h: 5 })]), sec("Two")]);
  assert.equal(sectionBottom(d, 0), 5);
  assert.equal(sectionBottom(d, 1), 2, "the placed control counts");
  assert.deepEqual(defaultPlacement(d, 1), { section: 1, x: 0, y: 2, w: 6, h: 2 });
  const placed = setPlacement(d, "a", defaultPlacement(d, 0));
  assert.deepEqual(placed.variables[0].placement, { section: 0, x: 0, y: 5, w: 6, h: 2 });
  assert.deepEqual(Object.keys(placed.variables[0]).at(-1), "placement", "the placement is the last member");
  assert.equal("placement" in d.variables[0], false, "the input is not changed");
  const back = setPlacement(placed, "a", null);
  assert.equal("placement" in back.variables[0], false);
  assert.deepEqual(back.variables[0], d.variables[0], "a variable with no placement has exactly its earlier form");
  assert.equal(setPlacement(d, "nope", defaultPlacement(d, 0)), d);
  assert.equal(placementRefusal(custom("x", { hide: "variable" })) !== "", true);
  assert.equal(placementRefusal(custom("x")), "");
  assert.equal(placementTitleError("x".repeat(256)), "");
  assert.match(placementTitleError("x".repeat(257)), /256 characters at most/);
});

test("the fields x, y, w and h move a placement and normalize the section with the panels; a change of section goes to the bottom of it", () => {
  const d = doc([custom("a", { placement: at(0, 0, 4, 6, 2, { title: "T" }) })], [sec(null, [stat(1, { x: 0, y: 0, w: 12, h: 4 })]), sec("Two", [stat(2, { x: 0, y: 0, w: 12, h: 6 })])]);
  const moved = movePlacement(d, "a", { x: 4, y: 4, w: 6, h: 2 });
  assert.deepEqual([moved.variables[0].placement.x, moved.variables[0].placement.w, moved.variables[0].placement.title], [4, 6, "T"]);
  const over = movePlacement(d, "a", { x: 0, y: 0, w: 6, h: 2 });
  const place = over.variables[0].placement;
  assert.ok(place.y >= 4 || over.sections[0].panels[0].grid.y >= 2, "the panel and the control do not overlap");
  assert.equal(movePlacement(d, "none", { x: 0, y: 0, w: 1, h: 1 }), d);
  const other = placeInSection(d, "a", 1);
  assert.deepEqual(other.variables[0].placement, { section: 1, x: 0, y: 6, w: 6, h: 2, title: "T" });
  assert.equal(placeInSection(doc([custom("b")], d.sections), "b", 1).variables[0].placement, undefined, "a variable with no placement stays in the bar");
});

test("a move of a section changes the index of the placements; a delete of a section removes its placements and shifts the later ones (10.6)", () => {
  const d = doc([custom("a", { placement: at(0, 0, 0) }), custom("b", { placement: at(1, 0, 0) }), custom("c", { placement: at(2, 0, 0) }), custom("d")], [sec(null), sec("One"), sec("Two")]);
  const down = moveSection(d, 0, 1);
  assert.deepEqual(down.variables.map((v) => v.placement && v.placement.section), [1, 0, 2, undefined]);
  const up = moveSection(d, 2, -1);
  assert.deepEqual(up.variables.map((v) => v.placement && v.placement.section), [0, 2, 1, undefined]);
  assert.equal(moveSection(d, 0, -1), null);
  const del = deleteSection(d, 1).doc;
  assert.deepEqual(del.variables.map((v) => v.placement && v.placement.section), [0, undefined, 1, undefined]);
  assert.equal("placement" in del.variables[1], false, "the variable is back in the top bar");
  assert.deepEqual(d.variables[1].placement, at(1, 0, 0), "the input is not changed");
  assert.equal(deleteSection(doc([], [sec(null)]), 0).error, "A dashboard needs at least one section.");
});

test("the grid draws the 11 types and the placed control; the type list has the 10 types with a query and the type text (9.19b, 9.19c, 9.19f)", () => {
  assert.equal(PANEL_TYPES.length, 15);
  assert.deepEqual(PANEL_TYPES.slice(8), ["state_timeline", "status_history", "heatmap", "histogram", "logs", "geomap", "text"]);
  assert.equal(isDrawable("text"), true);
  assert.equal(PANEL_TYPES.includes("variable"), false);
  assert.equal(isDrawable("variable"), true);
  assert.equal(isDrawable("table"), true);
  assert.equal(isDrawable("nope"), false);
});

test("the menu of a placed control: edit the variable, or put it back in the top bar (10.6)", () => {
  const d = doc([custom("a", { placement: at(0, 0, 0) })], [sec(null)]);
  const calls = [];
  const E = { doc: d, dispatch: (...a) => calls.push(["dispatch", ...a]), change: (n) => calls.push(["change", n]) };
  const items = placedMenu(E, "a");
  assert.deepEqual(items.map((i) => [i.label, i.testid]), [["Edit the variable", "placement-a-edit"], ["Put back in the top bar", "placement-a-remove"]]);
  items[1].onSelect();
  assert.equal("placement" in calls[0][1].variables[0], false);
  items[0].onSelect();
  assert.equal(E.settingsTab, "variables");
  assert.equal(E.openVariable, "a");
  assert.deepEqual(calls[1], ["dispatch", "open-settings"]);
});

// --- the draft runtime ---------------------------------------------------------------------------------------------------------------

function fakeBase(d) {
  const listeners = new Set();
  const set = [];
  return {
    panels: new Map(), variables: d.variables, varByName: new Map(d.variables.map((v) => [v.name, v])), range: { from_ms: 1, to_ms: 2 }, set,
    on: (l) => { listeners.add(l); return () => listeners.delete(l); },
    setSelection: (n, s) => set.push([n, s]),
    interpolate: (t) => t,
    requestVariables: () => ({}),
  };
}

test("the draft runtime gives the variables of the draft to the controls of the draft grid (9.10)", () => {
  const d = doc([custom("env", { placement: at(0, 0, 0) })], [sec(null)]);
  const base = fakeBase(d);
  const rt = new DraftRuntime({ base, doc: d, onCollapse: () => {} });
  assert.equal(rt.varDef("env").name, "env");
  assert.equal(rt.varDef("nope"), undefined);
  rt.setSelection("env", { all: false, values: ["b"] });
  assert.deepEqual(base.set, [["env", { all: false, values: ["b"] }]], "a variable of the page: the page selection");
  const next = { ...d, variables: [...d.variables, custom("fresh", { default: ["b"] })] };
  rt.setDoc(next);
  const events = [];
  rt.on((type) => events.push(type));
  assert.deepEqual(rt.localState("fresh").selections.get("fresh"), { all: false, values: ["b"] });
  rt.setSelection("fresh", { all: false, values: ["a"] });
  assert.deepEqual(rt.localState("fresh").selections.get("fresh"), { all: false, values: ["a"] });
  assert.ok(events.includes("vars"));
  assert.equal(base.set.length, 1, "the page runtime did not see it");
  rt.setDoc({ ...next, variables: next.variables.map((v) => (v.name === "fresh" ? { ...v, multi: true } : v)) });
  assert.deepEqual(rt.localState("fresh").selections.get("fresh"), { all: false, values: ["a"] });
  rt.destroy();
});

// --- the static rules ------------------------------------------------------------------------------------------------------------------

test("the frame of a placed control: group role, ready from the first frame, no query, no View, a menu in edit mode only (9.11, 9.26)", () => {
  assert.match(read("../../web/js/panel_frame.js"), /if \(panel\.type === "variable"\) return createVariableFrame\(\{ panel, page \}\);/);
  const js = read("../../web/js/panels/variable.js");
  assert.match(js, /role: "group"/);
  assert.match(js, /state: "ready"/);
  assert.match(js, /typeof page\.placedMenu === "function"/);
  assert.match(js, /groupLabel\(/);
  assert.doesNotMatch(js, /panelQuery|rt\.retry|viewPanel|View data|Inspect/);
  assert.doesNotMatch(js.replace(/\/\/.*$/gm, ""), /innerHTML/);
});

test("the stylesheet: thin handle without a title, no floor in the flow, no print, scroll for a low body (9.8, 9.11)", () => {
  const css = read("../../web/css/panel-variable.css");
  assert.match(css, /\.panel--variable \.panel__head--bare \{[^}]*height: var\(--space-8\)/);
  assert.match(css, /\.dash__grid > \.panel\[data-type="variable"\] \{ --panel-floor: 0px; \}/);
  assert.match(css, /@media print \{ \.panel--variable \{ display: none; \} \}/);
  assert.match(css, /\.panel-var \{[^}]*margin-block: auto/);
  assert.match(css, /\.panel--variable \.panel__content \{ overflow: auto; \}/);
});

test("the bar and the grid share the control code of vars.js; the call of the page does not change (9.10)", () => {
  const vars = read("../../web/js/vars.js");
  assert.match(vars, /export function createVarsBar\(rt, apply\)/);
  assert.match(vars, /export function controlsFor\(rt\)/);
  assert.match(vars, /function listControl\(/);
  assert.match(vars, /function textControl\(/);
  const bar = vars.slice(vars.indexOf("export function createVarsBar"));
  assert.doesNotMatch(bar, /listControl|textControl/, "the bar builds no control of its own");
  assert.match(vars, /testid: `variable-\$\{variable\.name\}-button`/);
  assert.match(vars, /dataset: \{ testid: `variable-\$\{variable\.name\}` \}/);
});
