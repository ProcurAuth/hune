// The header of wave K (9.6, 9.9, 10.2): one row, the joined time control, the switch Edit, the group of the edit mode (Add, Save,
// Revert, no reserved room), and the menu "More actions". The browser cases B69 to B71 check the behaviour in a real page. These tests check the files of web/ and the
// spec, so a change that breaks a rule fails before the browser suite runs.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../..");
const read = (path) => readFileSync(join(ROOT, path), "utf8");
const code = (path) => read(path).replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");

test("the time control is one joined control: the time group and the refresh group in one box", () => {
  const js = code("web/js/pickers.js");
  assert.match(js, /export function createTimeControl\(\{ time, refresh \}\)/);
  assert.match(js, /el\("div", \{ class: "tc", dataset: \{ testid: "time-control" \} \}, timePicker\.el, refreshPicker\.el\)/);
  // The refresh button has its icon only: its name is visually hidden by the css, and it has a tooltip.
  assert.match(js, /class: "btn btn--tb picker__run", "data-tooltip": "Refresh", dataset: \{ testid: "refresh-now" \}/);
  const css = read("web/css/components.css");
  assert.match(css, /\.picker__run \.btn__label \{ position: absolute;/);
  assert.match(css, /\.tc \{ display: flex; flex: none; align-items: stretch; \}/);
  // The corners join by type: a popup or the hidden list in a group does not change the corners.
  assert.match(css, /\.tb-group > \.btn--tb:not\(:first-of-type\)/);
  assert.match(css, /\.tc > \.tb-group:not\(:first-child\) > \.btn--tb:first-of-type/);
  const dash = code("web/js/dashboard.js");
  assert.match(dash, /createTimeControl\(\{/);
  assert.ok(!/createRefreshPicker|createTimePicker/.test(dash), "the page builds the control, not two pickers");
});

test("the header has, in order: the group of the edit mode, the time control, the switch Edit, the theme and More actions", () => {
  const dash = code("web/js/dashboard.js");
  assert.match(dash, /header\.setActions\(timeControl\.el, editSwitch\)/);
  assert.match(dash, /header\.setMore\(more\)/);
  // The group of the edit mode goes before the time control, and only in edit mode (ID-K177): the page has no slot for it.
  assert.match(dash, /setEditBar\(node\) \{[^}]*timeControl\.el\.before\(node\)/s);
  assert.ok(!/editSlot|header-edit-slot/.test(dash));
  const main = code("web/js/main.js");
  assert.match(main, /headerEl\.append\(brand, crumbs, actionsSlot, themeSlot, moreSlot\)/);
  assert.match(main, /setMore\(node\)/);
});

test("the switch Edit is a button with role switch and aria-checked, shown only with the right to write", () => {
  const dash = code("web/js/dashboard.js");
  assert.match(dash, /el\("button", \{ type: "button", role: "switch", class: "btn btn--tb edit-switch", "data-tooltip": "Edit", dataset: \{ testid: "edit-toggle" \} \}/);
  assert.match(dash, /editSwitch\.setAttribute\("aria-checked", String\(editActive\)\)/);
  assert.match(dash, /dispatch\(state\.editing \? "edit-off" : "edit-on"\)/);
  // The switch exists only with `editing` and `access.write`.
  assert.match(dash, /function canEdit\(\) \{\s*return session\.editing === true && loaded\.access && loaded\.access\.write === true;/);
  assert.match(dash, /if \(canEdit\(\)\) \{\s*editSwitch = buildEditSwitch\(\);\s*paintEditSwitch\(\);/);
  // The editor tells the page its state, and it can be turned off from the switch.
  assert.match(dash, /setEditActive\(on\) \{/);
  const edit = code("web/js/editor/edit_mode.js");
  // The switch is the event `edit-off` or `edit-on` of 10.2a: the editor asks the question of 10.2 in confirmLeave().
  assert.match(edit, /confirmLeave: \(then\) => this\.confirmLeave\(then\)/);
  assert.match(edit, /setEditing\(on\) \{\s*this\.editing = on;\s*this\.api\.setEditActive\(on\);/);
  assert.match(edit, /api2\.setEditActive\(false\);/);
  const css = read("web/css/components.css");
  assert.match(css, /\.edit-switch\.is-on \.edit-switch__track \{ border-color: transparent; background: var\(--primary\); \}/);
});

test("Add, Save and Revert are the edit controls of the header, and the other actions are items of More actions or of the menu of Add", () => {
  const edit = code("web/js/editor/edit_mode.js");
  assert.match(edit, /testid: "save-button"/);
  assert.match(edit, /button\("Revert all changes", \{ class: "btn btn--tb btn--icon edit-revert", dataset: \{ testid: "discard-button" \}/);
  assert.match(edit, /testid: "edit-bar"/);
  assert.match(edit, /this\.api\.setEditBar\(this\.barNode\)/);
  assert.ok(!/editSlot/.test(edit), "no reserved slot");
  // "Add" is a split button: the main part adds a panel at once, the chevron opens the menu (ID-K15).
  assert.match(edit, /testid: "add-button" \}, on: \{ click: \(\) => this\.addPanelTo\(/);
  assert.match(edit, /testid: "add-menu"/);
  assert.match(edit, /state, addGroup, saveButton, discard\)/, "the order: Add, Save, Revert");
  // The old buttons are gone.
  for (const gone of ["exit-button", "editbar__end", "editbar__mode", "toolbarSlots", "bar.left", "bar.end"]) assert.ok(!edit.includes(gone), `edit_mode.js still has ${gone}`);
  // The menu items keep their ids and come in this order.
  // The menu of Add has Add panel, Add section and the placement of a variable. More actions keeps the views.
  const ids = [...edit.matchAll(/testid: "(add-panel|add-section|add-variable|json-tab|dash-settings)"/g)].map((m) => m[1]);
  assert.deepEqual(ids, ["add-panel", "add-section", "add-variable", "json-tab", "dash-settings"]);
  const more = edit.slice(edit.indexOf("headerMenuItems() {"));
  assert.ok(!/add-panel|add-section/.test(more.slice(0, more.indexOf("paintBar() {"))), "More actions has no Add panel and no Add section");
  assert.match(edit, /menuItems: \(\) => this\.headerMenuItems\(\)/);
  // Revert asks first, as Discard did.
  assert.match(edit, /confirmDiscard\(\) \{\s*if \(!this\.dirty\(\)\) return;\s*openDialog\(\{\s*title: "Discard your changes"/);
  // The draft has no visible state text (ID-UF1).
  assert.match(edit, /class: "editbar__state visually-hidden"/);
});

test("the menu More actions has its items in order and by rights", () => {
  const dash = code("web/js/dashboard.js");
  const order = [...dash.matchAll(/testid: "(menu-time-back|menu-time-forward|menu-zoom-out|history-button|export-button|delete-button)"/g)].map((m) => m[1]);
  assert.deepEqual(order, ["menu-time-back", "menu-time-forward", "menu-zoom-out", "history-button", "export-button", "delete-button"]);
  // The time items show only when the window is narrow. The actions of the edit mode come before History. Delete needs the right to write.
  assert.match(dash, /if \(timeInMenu\(\)\) \{/);
  assert.match(dash, /editHooks && editHooks\.menuItems \? editHooks\.menuItems\(\) : \[\]/);
  assert.match(dash, /if \(canEdit\(\)\) \{\s*const item = \{ uid, title: loaded\.doc\.title, etag: loaded\.etag, folder: loaded\.folder \};\s*items\.push\(\{ separator: true \}\);\s*items\.push\(\{ label: "Delete"/);
  // Delete asks for a confirmation: it goes through the dialog of the editor (10.13).
  assert.match(dash, /editorCall\("folders", "deleteDashboard", item\)/);
  assert.match(code("web/js/editor/folders.js"), /testid: "delete-dashboard-dialog"/);
});

test("the header and the sizes of the controls: 44 px, fixed widths and the steps of header_layout.js", () => {
  const base = read("web/css/base.css");
  assert.match(base, /:root \{ --header-base-h: 44px; \}/);
  assert.match(base, /@media \(max-width: 1280px\) \{\s*\.brand__version \{ position: absolute;/);
  assert.match(base, /@media \(max-width: 640px\) \{\s*\.brand__name \{ position: absolute;/);
  const css = read("web/css/components.css");
  // The shift buttons and the zoom out hide up to 900 px (the edit group needs the room), and the range button gets round corners at its left.
  assert.match(css, /@media \(max-width: 900px\) \{\s*\.picker--time > \.btn--tb-nav, \.picker--time > \[data-testid="zoom-out"\] \{ display: none; \}/);
  // "Add" shows its icon only up to 900 px. "Revert" is in the menu up to 640 px (ID-K177).
  assert.match(css, /@media \(max-width: 900px\) \{\s*#app-header \{ --tb-add-w: var\(--tb-icon-w\); \}\s*\.add-main \{ padding: 0; \}\s*\.add-main \.btn__label \{ position: absolute;/);
  assert.match(css, /@media \(max-width: 640px\) \{\s*\.edit-revert \{ display: none; \}/);
  // Up to 560 px the time control, the switch and Add are smaller; a phone has 32 px icon buttons.
  assert.match(css, /@media \(max-width: 560px\) \{\s*#app-header \{ --tb-range-w: 40px; --tb-edit-w: 36px; --tb-addmenu-w: 20px; --tb-int-w: 28px; \}/);
  assert.match(css, /@media \(max-width: 480px\) \{\s*#app-header \{ --tb-icon-w: 32px; \}/);
  assert.match(base, /\.crumbs \{ container-type: inline-size; \}\s*@container \(max-width: 149px\)/);
  const list = read("web/css/list.css");
  assert.match(list, /\.list-search \{ position: relative; flex: 0 1 240px; min-width: 96px; \}/);
  assert.ok(!/data-layout="stacked"/.test(list));
});

test("the spec has the new rules and the hooks of the header", () => {
  const spec = read("docs/04-frontend-editor.md");
  assert.match(spec, /The bar is one row at every width, also in edit mode \(decision ID-K1/);
  assert.match(spec, /It is a decision of the owner \(2026-11-11, ID-UI-HDR\)/);
  assert.match(spec, /The time control, which is one joined control \(decision ID-K2/);
  assert.ok(!spec.includes("header-edit-slot"), "the reserved slot is gone (ID-K13)");
  assert.match(spec, /no button of the view moves \(decision ID-K177/);
  assert.match(spec, /The switch is a button with `role="switch"` and `aria-checked`/);
  for (const id of ["time-control", "export-button", "delete-button", "menu-time-back", "menu-zoom-out", "edit-bar", "discard-button", "add-button", "add-menu", "add-variable", "add-panel", "add-section"]) assert.ok(spec.includes(`"${id}"`), `the hook ${id} is in 9.29`);
  // The edit row is gone from the spec.
  assert.ok(!/header-edit-row/.test(spec));
  assert.ok(!/in the row of the edit mode/.test(spec));
});

test("the panel editor has no row for the back button: the crumb of the dashboard is the way back (ID-K178)", () => {
  const editor = code("web/js/editor/panel_editor.js");
  assert.ok(!/peditor__bar|peditor__id|peditor__sep/.test(editor), "no top bar in the panel editor");
  assert.match(editor, /root\.append\(mainBox\)/);
  const edit = code("web/js/editor/edit_mode.js");
  assert.match(edit, /backCrumb\(\) \{/);
  assert.match(edit, /"Back to panel view" : page === "view" \? "Back to dashboard" : "Back to edit mode"/);
  assert.match(edit, /dataset: \{ testid: "editor-back" \}/);
  assert.match(edit, /setCrumbTail\(plan\.kind === "panel" \? "Edit panel" : null, plan\.kind === "panel" \? this\.backCrumb\(\) : null\)/);
  // A click with a modifier key is not the event: the link keeps its address.
  assert.match(edit, /e\.metaKey \|\| e\.ctrlKey \|\| e\.shiftKey \|\| e\.altKey\) return;\s*e\.preventDefault\(\);/);
  const main = code("web/js/main.js");
  assert.match(main, /\.\.\.part\.attrs \}, part\.icon \? icon\(part\.icon, 14\) : null/);
  const css = read("web/css/editor.css");
  assert.ok(!/\.peditor__bar|--peditor-bar-h/.test(css), "the css of the top bar is gone");
  const base = read("web/css/base.css");
  assert.match(base, /@container \(max-width: 99px\) \{\s*\.crumbs__item--back \.crumbs__text/);
  const spec = read("docs/04-frontend-editor.md");
  assert.match(spec, /The panel editor has no top bar and no row of its own \(decision ID-K178/);
  assert.match(spec, /"Back to edit mode"/);
});
