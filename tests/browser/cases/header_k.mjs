// Browser cases B69 to B71, B90, B150 and B151 (9.6, 9.9, 9.25, 10.2, 10.13, 9.29): the header of wave K.
// B150: the four fixed buttons (time control, switch Edit, theme selector, More actions) keep their x and y when the edit mode starts and
//       ends (ID-K177), at 1920, 1280, 900, 768 and 390 px, light and dark; no frame between; the tab order is the visual order.
// B151: the panel editor has no row for the back button (ID-K178): the editor gains the 48 px of the old row at 1920, 1280, 768 and 390 px,
//       and the crumb of the dashboard is the way back from each origin (the panel view, edit mode, the dashboard), with the focus rule.
// B90: the button Add of the edit mode: a split button (Add panel with one click, a menu for Add section and the placement of a variable),
//      the icon only at 900 px and below, the keyboard, and the items that left the menu More actions.
// B69: the composite time control: the range, the refresh and the interval are one joined control, with the keyboard.
// B70: the switch Edit (role, aria-checked, keys, rights), Save and Revert in edit mode, the confirmation, Save with no change.
// B71: one header row at 1920, 1280, 768 and 390 px, light and dark, in view and in edit mode, the height of 44 px, the menu
//      "More actions" by rights, the confirmation of Delete, and nothing that wraps at 390 px.
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { addItem, enterEdit, moreItem, waitDraft } from "../lib/ed.mjs";
import { panelQueries, sleep, waitPanelsReady } from "../lib/h.mjs";

const UID69 = "hk69";
const UID70 = "hk70";
const UID71 = "hk71";
const UID90 = "hk90";
const UID150 = "hk150";
const UID151 = "hk151";
const IDS = [1, 2, 10];

// Two stat panels and a time series in a section, and one more panel in a second section. The title is long enough to show an ellipsis
// in the crumbs of a narrow window.
function headerDoc(uid, title) {
  const g = (x, y, w, h) => ({ x, y, w, h });
  return doc(uid, title, [
    section("Overview", [
      panel(1, "stat", "Stat 1", "SELECT 7 AS value", { grid: g(0, 0, 6, 4) }),
      panel(2, "stat", "Stat 2", "SELECT 14 AS value", { grid: g(6, 0, 6, 4) }),
      panel(10, "timeseries", "Series", SQL.series, { grid: g(0, 4, 12, 8) }),
    ]),
    section("Details", [panel(20, "table", "Table", SQL.table, { grid: g(0, 0, 12, 8) })]),
  ], { time: { from: "now-6h", to: "now", timezone: "browser", refresh: null } });
}

async function openAt(t, url, width, { scheme = null, height = width === 390 ? 844 : 900 } = {}) {
  const p = await t.page({ width, height });
  await p.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  if (scheme) await p.emulateColorScheme(scheme);
  await p.goto(url);
  await p.idle();
  await sleep(150);
  return p;
}

async function ready(p, ids = IDS) {
  await waitPanelsReady(p, ids, "ready").catch(async () => {
    // A panel below the fold of a narrow window does not query: wait for the first panel only.
    await p.waitFor(`(() => { const e = document.querySelector('[data-testid="panel-1"]'); return e && (e.dataset.state === "ready" || e.dataset.state === "empty"); })()`, { what: "the first panel" });
  });
  await p.idle();
}

const rect = (p, id) => p.eval(`(() => { const e = document.querySelector('[data-testid="${id}"]'); if (!e || e.offsetParent === null) return null; const b = e.getBoundingClientRect(); return { left: b.left, right: b.right, top: b.top, bottom: b.bottom, width: b.width, height: b.height }; })()`);
const active = (p) => p.eval(`(document.activeElement && (document.activeElement.dataset.testid || document.activeElement.tagName)) || null`);
const noOverflow = (p) => p.eval(`document.documentElement.scrollWidth <= document.documentElement.clientWidth`);

// The measure of the header: its height, the vertical centres of its controls (one value means one row), the width that its content
// needs against the width that it has, and the height of each line of text of the crumbs (a wrap would give two lines).
const HEADER = `(() => {
  const h = document.getElementById("app-header");
  const hb = h.getBoundingClientRect();
  const nodes = [...h.querySelectorAll("button, a.brand, input")].filter((e) => e.offsetParent !== null && e.getBoundingClientRect().width > 1);
  const centres = [...new Set(nodes.map((e) => { const b = e.getBoundingClientRect(); return Math.round(b.top + b.height / 2); }))];
  const outside = nodes.filter((e) => { const b = e.getBoundingClientRect(); return b.left < hb.left - 0.5 || b.right > hb.right + 0.5 || b.top < hb.top - 0.5 || b.bottom > hb.bottom + 0.5; }).map((e) => e.dataset.testid || e.className);
  const lines = [...h.querySelectorAll(".crumbs__text")].filter((e) => e.offsetParent !== null).map((e) => Math.round(e.getBoundingClientRect().height));
  return { height: Math.round(hb.height * 10) / 10, centres, outside, scroll: h.scrollWidth, width: h.clientWidth, lines, count: nodes.length, position: getComputedStyle(h).position };
})()`;

/** The texts of the items of the open menu. */
const menuItems = (p) => p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`);

export const cases = [
  {
    id: "B69",
    name: "the time control is one joined control: range, refresh and interval, each with its keyboard, with no separate refresh button (9.9)",
    async run(t) {
      const r = await t.api.create("transit", headerDoc(UID69, "Header composite"));
      t.assert(r.status === 201, `create ${UID69}: ${r.status} ${r.text.slice(0, 300)}`);
      const p = await openAt(t, t.dash(UID69, "refresh=off"), 1280);
      await ready(p);

      // 1. One control: the six buttons are in one box, in this order, in one row, and they touch. There is one refresh button.
      const ids = ["time-back", "time-picker-button", "time-forward", "zoom-out", "refresh-now", "refresh-interval"];
      const boxes = {};
      for (const id of ids) boxes[id] = await rect(p, id);
      t.assert(ids.every((id) => boxes[id]), "a button of the time control is missing: " + JSON.stringify(boxes));
      for (let i = 1; i < ids.length; i++) {
        const a = boxes[ids[i - 1]];
        const b = boxes[ids[i]];
        t.assert(b.left >= a.right - 0.5 && b.left - a.right <= 1.5, `${ids[i - 1]} and ${ids[i]} touch: ${a.right} then ${b.left}`);
        t.assert(Math.abs(a.top - b.top) <= 0.5 && Math.abs(a.height - b.height) <= 0.5, `${ids[i - 1]} and ${ids[i]} are on one line, with one height`);
      }
      t.eq(await p.eval(`(() => { const c = document.querySelector('[data-testid="time-control"]'); return ["time-picker", "refresh-picker"].map((id) => c.contains(document.querySelector('[data-testid="' + id + '"]'))); })()`), [true, true], "the time group and the refresh group are in the time control");
      t.eq(await p.eval(`[document.querySelectorAll('[data-testid="refresh-now"]').length, document.querySelectorAll('[data-testid="refresh-interval"]').length, [...document.querySelectorAll("#app-header button")].filter((e) => /^Refresh$/.test(e.textContent.trim()) && e.offsetParent !== null && e.getBoundingClientRect().width > 40).length]`), [1, 1, 0], "one refresh button and one interval button, and no wide button with a label Refresh");
      t.eq(await p.eval(`(() => { const c = document.querySelector('[data-testid="time-control"]').getBoundingClientRect(); const r = document.querySelector('[data-testid="refresh-interval"]').getBoundingClientRect(); return [Math.round(c.height), Math.abs(c.right - r.right) <= 1]; })()`), [32, true], "the control is one box of 32 px");

      // 2. The range: a press opens the popup, Escape closes it and gives the focus back to the range button.
      await p.click(p.tid("time-picker-button"));
      await p.waitSel(".picker__popup");
      t.eq(await p.attr(p.tid("time-picker-button"), "aria-expanded"), "true", "the range button is expanded");
      await p.press("Escape");
      await p.waitGone(".picker__popup");
      t.eq(await active(p), "time-picker-button", "Escape gives the focus back to the range button");
      // The keys: Space on the focused range button opens the popup, a quick range applies and closes it.
      await p.focus(p.tid("time-picker-button"));
      await p.press(" ");
      await p.waitSel(".picker__popup");
      await p.clickText(".picker__quick-item", "Last 24 hours");
      await p.waitGone(".picker__popup");
      await p.idle();
      t.eq(await p.eval(`document.querySelector('[data-testid="time-picker-button"]').textContent.trim()`), "Last 24 hours", "a quick range changes the text of the range button");
      t.assert(/from=now-24h/.test(await p.eval("location.search")), "the address has the range: " + (await p.eval("location.search")));
      // The width of the range button does not depend on the text.
      const wRange = (await rect(p, "time-picker-button")).width;
      t.eq(wRange, boxes["time-picker-button"].width, "the range button has one width");

      // 3. The refresh: a click and the key Enter on the focused button refresh the panels.
      const queries = () => panelQueries(p).length;
      await ready(p);
      let before = queries();
      await p.click(p.tid("refresh-now"));
      await sleep(400);
      await p.idle();
      t.assert(queries() > before, "a click on the refresh button runs the queries again");
      before = queries();
      await p.focus(p.tid("refresh-now"));
      await p.press("Enter");
      await sleep(400);
      await p.idle();
      t.assert(queries() > before, "the key Enter on the refresh button runs the queries again");
      t.eq(await p.attr(p.tid("refresh-now"), "aria-label"), null, "the refresh button has its name from its content");
      t.eq(await p.eval(`document.querySelector('[data-testid="refresh-now"]').textContent.trim()`), "Refresh", "the name of the refresh button");

      // 4. The interval: Enter opens the menu on "Off", the arrows move, Enter chooses; the value shows beside the chevron.
      t.eq(await p.attr(p.tid("refresh-interval"), "aria-label"), "Refresh interval: Off", "the interval is off");
      await p.focus(p.tid("refresh-interval"));
      await p.press("Enter");
      await p.waitSel(".menu");
      t.eq(await p.eval(`document.activeElement.textContent.trim()`), "Off", "the menu opens on the current item");
      t.eq(await menuItems(p).then((x) => x.slice(0, 3)), [], "the items of the interval menu are radio items, not plain items");
      t.eq(await p.eval(`[...document.querySelectorAll('.menu [role="menuitemradio"]')].map((e) => e.textContent.trim()).slice(0, 4)`), ["Off", "10s", "30s", "1m"], "the intervals of the menu");
      await p.press("ArrowDown");
      await p.press("ArrowDown");
      t.eq(await p.eval(`document.activeElement.textContent.trim()`), "30s", "two arrows go to 30s");
      await p.press("Enter");
      await p.waitGone(".menu");
      t.eq(await p.eval(`[document.querySelector('[data-testid="refresh-select"]').value, document.querySelector('[data-testid="refresh-interval"]').textContent.trim(), document.querySelector('[data-testid="refresh-interval"]').getAttribute("aria-label")]`), ["30s", "30s", "Refresh interval: 30s"], "the interval is 30s");
      t.assert(/refresh=30s/.test(await p.eval("location.search")), "the address has the interval");
      t.eq(await active(p), "refresh-interval", "the focus is back on the interval button");
      // The two buttons keep their box when the interval changes.
      for (const id of ["refresh-now", "refresh-interval"]) {
        const b = await rect(p, id);
        t.eq([b.left, b.width, b.height], [boxes[id].left, boxes[id].width, boxes[id].height], `${id} keeps its place and size with an interval`);
      }
      // Escape closes the menu; the choice Off puts the interval back.
      await p.click(p.tid("refresh-interval"));
      await p.waitSel(".menu");
      await p.press("Escape");
      await p.waitGone(".menu");
      await p.click(p.tid("refresh-interval"));
      await p.waitSel(".menu");
      await p.clickText('.menu [role="menuitemradio"]', "Off");
      await p.waitGone(".menu");
      t.eq(await p.eval(`document.querySelector('[data-testid="refresh-select"]').value`), "off", "Off puts the interval back");

      // 5. The tab order follows the picture: back, range, forward, zoom out, refresh, interval, then the switch Edit.
      await p.focus(p.tid("time-back"));
      const order = [await active(p)];
      for (let i = 0; i < 6; i++) {
        await p.press("Tab");
        order.push(await active(p));
      }
      t.eq(order, [...ids, "edit-toggle"], "the tab order");

      // 6. A phone: the shift buttons and the zoom out are in the menu More actions, the rest of the control stays joined and in one row.
      const q = await openAt(t, t.dash(UID69, "refresh=off"), 390);
      await ready(q);
      t.eq(await q.eval(`["time-back", "time-forward", "zoom-out"].map((id) => document.querySelector('[data-testid="' + id + '"]').offsetParent === null)`), [true, true, true], "390: the shift buttons and the zoom out hide");
      const small = {};
      for (const id of ["time-picker-button", "refresh-now", "refresh-interval"]) small[id] = await rect(q, id);
      t.assert(Math.abs(small["time-picker-button"].top - small["refresh-now"].top) <= 0.5 && small["refresh-now"].left - small["time-picker-button"].right <= 1.5 && small["refresh-interval"].left - small["refresh-now"].right <= 1.5, "390: the three buttons touch, in one line " + JSON.stringify(small));
      await q.click(q.tid("dashboard-menu"));
      await q.waitSel(".menu");
      t.eq((await menuItems(q)).slice(0, 3), ["Shift the time range back", "Shift the time range forward", "Zoom out"], "390: the menu has the time actions");
      const from0 = await q.eval(`new URL(location.href).searchParams.get("from")`);
      await q.click('.menu [data-testid="menu-zoom-out"]');
      await q.idle();
      t.assert(await q.eval(`new URL(location.href).searchParams.get("from")`) !== from0, "390: Zoom out of the menu changes the range");
    },
  },

  {
    id: "B70",
    name: "the switch Edit has role switch and aria-checked, works with Space and Enter, needs the right to write; Add, Save and Revert show in edit mode with no reserved slot, and Revert asks first (10.2)",
    async run(t) {
      const r = await t.api.create("transit", headerDoc(UID70, "Header switch"));
      t.assert(r.status === 201, `create ${UID70}: ${r.status} ${r.text.slice(0, 300)}`);
      const p = await openAt(t, t.dash(UID70, "refresh=off"), 1280);
      await ready(p);
      const sw = `[data-testid="edit-toggle"]`;
      const state = () => p.eval(`(() => { const e = document.querySelector('[data-testid="edit-toggle"]'); return { role: e.getAttribute("role"), checked: e.getAttribute("aria-checked"), tag: e.tagName, name: e.textContent.trim(), bar: !!document.querySelector('[data-testid="edit-bar"]') }; })()`);

      // 1. The semantics: a button with role="switch", off, named "Edit".
      t.eq(await state(), { role: "switch", checked: "false", tag: "BUTTON", name: "Edit", bar: false }, "the switch in the view");
      t.eq(await p.eval(`document.querySelector('[data-testid="edit-bar"]')`), null, "no Save and no Revert in the view");
      // The view has no slot for the edit mode (ID-K13): nothing is between the switch and the theme selector but one gap.
      t.eq(await rect(p, "header-edit-slot"), null, "no reserved slot in the view");
      const switchBox = await rect(p, "edit-toggle");
      const gapView = await p.eval(`document.querySelector(".theme-select__button").getBoundingClientRect().left - document.querySelector('[data-testid="edit-toggle"]').getBoundingClientRect().right`);
      t.eq(gapView, 8, "the view has no blank at the right of the switch: one gap of 8 px to the theme selector");

      // 2. The keyboard: Tab reaches the switch with a visible focus; Space turns it on; Enter turns it off (no change: no question).
      await p.focus(p.tid("refresh-interval"));
      await p.press("Tab");
      t.eq(await active(p), "edit-toggle", "Tab reaches the switch");
      const ring = await p.eval(`(() => { const s = getComputedStyle(document.activeElement); return [document.activeElement.matches(":focus-visible"), parseFloat(s.outlineWidth) > 0, s.outlineStyle !== "none"]; })()`);
      t.eq(ring, [true, true, true], "the focus is visible");
      await p.press(" ");
      await p.waitSel(p.tid("edit-bar"));
      await p.waitSel('.edit-grid [data-testid^="panel-"][data-state]');
      await p.idle();
      t.eq((await state()).checked, "true", "Space turns the switch on");
      t.eq(await active(p), "edit-toggle", "the switch keeps the focus");
      t.assert(/edit=1/.test(await p.eval("location.search")), "the address has edit=1");
      await p.press("Enter");
      await p.waitGone(p.tid("edit-bar"));
      await p.idle();
      t.eq((await state()).checked, "false", "Enter turns the switch off");
      t.assert(!/edit=1/.test(await p.eval("location.search")), "the address has no edit=1");
      t.eq(await p.exists(".dialog, dialog[open]"), false, "no question without a change");

      // 3. A press with the mouse turns it on. Add, Save and Revert show at the left of the time control. Nothing of the view moves: the
      //    crumbs give the room (B59 and B150 measure all of them).
      const pre = {};
      for (const id of ["time-picker-button", "refresh-now", "refresh-interval", "edit-toggle", "dashboard-menu"]) pre[id] = await rect(p, id);
      const themePre = await p.eval(`document.querySelector(".theme-select__button").getBoundingClientRect().left`);
      await enterEdit(p);
      const barBox = await rect(p, "edit-bar");
      for (const id of ["time-picker-button", "refresh-now", "refresh-interval", "edit-toggle", "dashboard-menu"]) t.eq(await rect(p, id), pre[id], `${id} does not move when the edit mode starts`);
      t.assert(Math.abs(barBox.right + 8 - (await rect(p, "time-back")).left) <= 0.5, "the group ends one gap before the time control");
      t.eq(await p.eval(`document.querySelector(".theme-select__button").getBoundingClientRect().left`), themePre, "the theme selector does not move");
      t.eq((await state()).checked, "true", "the switch is on in edit mode");
      const inBar = await p.eval(`(() => { const s = document.querySelector('[data-testid="edit-bar"]').getBoundingClientRect(); return ["add-button", "add-menu", "save-button", "discard-button"].map((id) => { const b = document.querySelector('[data-testid="' + id + '"]').getBoundingClientRect(); return b.left >= s.left - 0.5 && b.right <= s.right + 0.5 && b.top >= s.top - 0.5 && b.bottom <= s.bottom + 0.5; }); })()`);
      t.eq(inBar, [true, true, true, true], "Add, Save and Revert are in the edit bar");
      const order = await p.eval(`(() => { const x = (id) => document.querySelector('[data-testid="' + id + '"]').getBoundingClientRect().left; const theme = document.querySelector(".theme-select__button").getBoundingClientRect().left; return [x("add-button") < x("add-menu"), x("add-menu") < x("save-button"), x("save-button") < x("discard-button"), x("discard-button") < x("time-back"), x("refresh-interval") < x("edit-toggle"), x("edit-toggle") < theme, theme < x("dashboard-menu")]; })()`);
      t.eq(order, [true, true, true, true, true, true, true], "the order: Add, Save, Revert, time control, switch, theme, More actions");

      // 4. Save and Revert have no change to act on: they are not active, and no text says "Unsaved".
      const dirtyState = () => p.eval(`(() => { const s = document.querySelector('[data-testid="save-button"]'); const d = document.querySelector('[data-testid="discard-button"]'); return { save: s.disabled, revert: d.disabled, title: s.title, saveBox: [s.getBoundingClientRect().left, s.getBoundingClientRect().width, s.getBoundingClientRect().height] }; })()`);
      const clean = await dirtyState();
      t.eq([clean.save, clean.revert, clean.title], [true, true, "There are no changes to save"], "no change: Save and Revert are not active");
      const visibleUnsaved = () => p.eval(`[...document.querySelectorAll("#app-header *, #app-main *")].filter((e) => e.children.length === 0 && /unsaved/i.test(e.textContent) && e.getBoundingClientRect().width > 1 && e.getBoundingClientRect().height > 1 && getComputedStyle(e).visibility !== "hidden").length`);
      t.eq(await visibleUnsaved(), 0, "no visible text 'Unsaved'");
      t.eq(await p.exists(".editbar__state[data-dirty='false']"), true, "a hidden status says that nothing changed (for a screen reader)");

      // 5. A change makes them active; the Save button keeps its box.
      const sections0 = await p.count('[data-testid^="section-"].section, .edit-grid [data-testid^="section-"]');
      await moreItem(p, "add-section");
      await p.waitFor(`!document.querySelector('[data-testid="save-button"]').disabled`, { what: "Save active" });
      const dirty = await dirtyState();
      t.eq([dirty.save, dirty.revert, dirty.title], [false, false, "Save the changes"], "a change: Save and Revert are active");
      t.eq(dirty.saveBox, clean.saveBox, "Save keeps its box");
      t.eq(await visibleUnsaved(), 0, "still no visible text 'Unsaved'");
      t.eq(await p.exists(".editbar__state[data-dirty='true']"), true, "the hidden status says that something changed");
      const sections1 = await p.count('[data-testid^="section-"].section, .edit-grid [data-testid^="section-"]');
      t.assert(sections1 > sections0, `a section was added (${sections0} then ${sections1})`);

      // 6. Revert asks first. "Stay" keeps the changes; "Discard" goes back to the saved version.
      await p.click(p.tid("discard-button"));
      await p.waitSel("dialog[open]");
      t.eq(await p.eval(`document.querySelector("dialog[open]").textContent.replace(/\\s+/g, "").trim()`), "DiscardyourchangesDiscardyourchangesandshowthesaveddashboard?StayDiscard", "the question of Revert");
      await p.clickText("dialog[open] button", "Stay");
      await p.waitGone("dialog[open]");
      t.eq((await dirtyState()).save, false, "Stay keeps the changes");
      await p.click(p.tid("discard-button"));
      await p.waitSel("dialog[open]");
      await p.clickText("dialog[open] button", "Discard");
      await p.waitGone("dialog[open]");
      await p.waitFor(`document.querySelector('[data-testid="save-button"]').disabled`, { what: "Save not active after Revert" });
      t.eq(await p.count('.edit-grid [data-testid^="section-"]'), sections0, "Revert goes back to the saved sections");
      t.eq((await state()).checked, "true", "Revert does not leave the edit mode");

      // 7. The switch off with changes asks too (the dialog "Exit edit mode"). Stay keeps the mode; Discard leaves it.
      await moreItem(p, "add-section");
      await p.waitFor(`!document.querySelector('[data-testid="save-button"]').disabled`, { what: "Save active again" });
      await p.click(sw);
      await p.waitSel(p.tid("exit-dialog"));
      await p.clickText('[data-testid="exit-dialog"] button', "Stay");
      await p.waitGone(p.tid("exit-dialog"));
      t.eq((await state()).checked, "true", "Stay keeps the switch on");
      await p.click(sw);
      await p.waitSel(p.tid("exit-dialog"));
      await p.clickText('[data-testid="exit-dialog"] button', "Discard");
      await p.waitGone(p.tid("edit-bar"));
      await p.idle();
      t.eq((await state()).checked, "false", "Discard leaves the edit mode: the switch is off");
      t.eq(await rect(p, "edit-toggle"), switchBox, "the switch has one place and one size");

      // 8. Save opens its dialog from the header (the dialog is closed, nothing is saved).
      await enterEdit(p);
      await moreItem(p, "add-section");
      await p.waitFor(`!document.querySelector('[data-testid="save-button"]').disabled`, { what: "Save active for the dialog" });
      await p.click(p.tid("save-button"));
      await p.waitSel(p.tid("save-dialog"));
      await p.press("Escape");
      await p.waitGone(p.tid("save-dialog"));
      t.eq((await dirtyState()).save, false, "after the dialog the draft is still there");

      // 9. The rights: a server with editing off has no switch and no slot. A phone keeps the switch in the header.
      const viewer = await t.ownServer("viewer", { editing: false, prepare: (volume) => writeFileSync(join(volume, "transit", `${UID70}.json`), JSON.stringify(headerDoc(UID70, "Header switch"))) });
      const v = await openAt(t, `${viewer.url}d/${UID70}?${t.range}`, 1280);
      t.eq(await v.eval(`[document.querySelectorAll('[data-testid="edit-toggle"]').length, document.querySelectorAll('[data-testid="add-button"]').length, document.querySelectorAll('[data-testid="edit-bar"]').length]`), [0, 0, 0], "editing off: no switch, no Add, no Save");
      const phone = await openAt(t, t.dash(UID70, "refresh=off"), 390);
      t.eq(await phone.eval(`(() => { const e = document.querySelector('[data-testid="edit-toggle"]'); const b = e.getBoundingClientRect(); const h = document.getElementById("app-header").getBoundingClientRect(); return [e.getAttribute("role"), b.width > 0, b.right <= h.right, e.textContent.trim()]; })()`), ["switch", true, true, "Edit"], "390: the switch is in the header, with its name");
    },
  },

  {
    id: "B71",
    name: "the header is one row of 44 px at 1920, 1280, 768 and 390 px, in light and dark, in view and edit mode; More actions has its items by rights; Delete asks first (9.6, 9.9, 10.13)",
    async run(t) {
      const r = await t.api.create("transit", headerDoc(UID71, "A dashboard with a rather long title for the crumbs of a narrow window"));
      t.assert(r.status === 201, `create ${UID71}: ${r.status} ${r.text.slice(0, 300)}`);
      const found = [];
      const check = (label, m, { sticky = true } = {}) => {
        if (m.height > 48) found.push(`${label}: the header is ${m.height} px high (at most 48)`);
        if (m.height !== 44) found.push(`${label}: the header is ${m.height} px, expected 44`);
        if (m.centres.length !== 1) found.push(`${label}: the controls are on ${m.centres.length} lines (${m.centres.join(", ")})`);
        if (m.outside.length) found.push(`${label}: outside the header: ${m.outside.join(", ")}`);
        if (m.scroll > m.width) found.push(`${label}: the content is ${m.scroll} px wide in a header of ${m.width} px`);
        if (m.lines.some((h) => h > 20)) found.push(`${label}: a crumb wraps (${m.lines.join(", ")})`);
        if (sticky && m.position !== "sticky") found.push(`${label}: position ${m.position}`);
      };

      for (const scheme of ["dark", "light"]) {
        for (const width of [1920, 1280, 768, 390]) {
          const label = `${scheme} ${width}`;
          const p = await openAt(t, t.dash(UID71, "refresh=30s"), width, { scheme });
          await ready(p);
          check(`${label} view`, await p.eval(HEADER));
          if (!(await noOverflow(p))) found.push(`${label} view: the page scrolls sideways`);
          await enterEdit(p);
          await sleep(150);
          check(`${label} edit`, await p.eval(HEADER));
          if (!(await noOverflow(p))) found.push(`${label} edit: the page scrolls sideways`);
          // The panel editor keeps the same header.
          await p.click(p.tid("panel-1-menu"));
          await p.click(p.tid("panel-1-edit"));
          await p.waitSel(p.tid("panel-editor"));
          await p.idle();
          check(`${label} panel editor`, await p.eval(HEADER));
          // The list page: one row too, for an editor.
          const l = await openAt(t, t.url(""), width, { scheme });
          await l.waitFor(`!!document.querySelector('[data-testid="list"] tr[data-testid^="row-"]')`, { what: "the list" });
          check(`${label} list`, await l.eval(HEADER));
          if (!(await noOverflow(l))) found.push(`${label} list: the page scrolls sideways`);
        }
      }

      // The menu by rights: an editor sees History, Export and Delete; in edit mode it also has the actions of the edit mode.
      const e = await openAt(t, t.dash(UID71, "refresh=off"), 1280);
      await ready(e);
      await e.click(e.tid("dashboard-menu"));
      await e.waitSel(".menu");
      t.eq(await menuItems(e), ["History", "Export", "Delete"], "the menu of an editor in the view");
      t.eq(await e.eval(`document.querySelector('[data-testid="dashboard-menu"]').getAttribute("aria-expanded")`), "true", "the button shows the open menu");
      await e.press("Escape");
      await enterEdit(e);
      await e.click(e.tid("dashboard-menu"));
      await e.waitSel(".menu");
      t.eq(await menuItems(e), ["Dashboard JSON", "Settings", "History", "Export", "Delete"], "the menu of an editor in edit mode");
      await e.press("Escape");
      await e.click(e.tid("edit-toggle"));
      await e.waitGone(e.tid("edit-bar"));

      // Delete asks first. Cancel keeps the dashboard.
      await moreItem(e, "delete-button");
      await e.waitSel(e.tid("delete-dashboard-dialog"));
      t.eq(await e.eval(`document.querySelector('[data-testid="delete-dashboard-dialog"]').textContent.replace(/\\s+/g, " ")`).then((x) => /Delete the dashboard/.test(x)), true, "the dialog names the dashboard");
      await e.clickText('[data-testid="delete-dashboard-dialog"] button', "Cancel");
      await e.waitGone(e.tid("delete-dashboard-dialog"));
      t.eq((await t.api.dashboard(UID71)).status, 200, "Cancel keeps the dashboard");

      // A viewer (editing off): History and Export only, no switch, and the same one row.
      const viewer = await t.ownServer("viewer", { editing: false, prepare: (volume) => writeFileSync(join(volume, "transit", `${UID71}.json`), JSON.stringify(headerDoc(UID71, "A dashboard with a rather long title for the crumbs of a narrow window"))) });
      for (const width of [1920, 1280, 768, 390]) {
        const v = await openAt(t, `${viewer.url}d/${UID71}?${t.range}`, width);
        await ready(v);
        check(`viewer ${width}`, await v.eval(HEADER));
        await v.click(v.tid("dashboard-menu"));
        await v.waitSel(".menu");
        t.eq((await menuItems(v)).filter((x) => !/time range|Zoom/.test(x)), ["History", "Export"], `${width}: the menu of a viewer`);
        await v.press("Escape");
        const vl = await openAt(t, viewer.url, width);
        await vl.waitFor(`!!document.querySelector('[data-testid="list"] tr[data-testid^="row-"]')`, { what: "the list of the viewer" });
        check(`viewer list ${width}`, await vl.eval(HEADER));
      }

      // The brand and the crumbs are small: a logo of 20 px, a crumb font of 12 px, one line, and the ellipsis with the tooltip.
      const c = await openAt(t, t.dash(UID71, "refresh=off"), 390);
      await ready(c);
      t.eq(await c.eval(`(() => { const logo = document.querySelector(".brand .icon").getBoundingClientRect(); const text = document.querySelector(".crumbs__item--current .crumbs__text"); const s = getComputedStyle(text); return [Math.round(logo.width), s.fontSize, s.textOverflow, text.scrollWidth > text.clientWidth, document.querySelector(".crumbs__item--current").title]; })()`), [20, "12px", "ellipsis", true, "A dashboard with a rather long title for the crumbs of a narrow window"], "390: a small logo, small crumbs, an ellipsis and the tooltip");
      const wide = await openAt(t, t.dash(UID71, "refresh=off"), 1920);
      await ready(wide);
      t.eq(await wide.eval(`(() => { const v = document.querySelector('[data-testid="app-version"]').getBoundingClientRect(); const n = document.querySelector(".brand__name").getBoundingClientRect(); return [v.left >= n.right - 1, Math.abs((v.top + v.height / 2) - (n.top + n.height / 2)) <= 8]; })()`), [true, true], "1920: the version line is beside the name, on the same line");
      t.eq(found.slice(0, 12), [], "header rows");
    },
  },

  {
    id: "B90",
    name: "the button Add of the edit mode is a split button: one click adds a panel, the menu has Add section and the placement of a variable, the label hides at 900 px, and the keyboard works (10.2, 10.6)",
    async run(t) {
      const g = (x, y, w, h) => ({ x, y, w, h });
      const custom = (name, label, hide = "none") => ({ kind: "custom", name, label, hide, options: [{ text: "Production", value: "prod" }, { text: "Preproduction", value: "preprod" }], multi: false, include_all: false, all_value: null, default: ["prod"] });
      const d = doc(UID90, "Header add", [
        section("Overview", [panel(1, "stat", "Stat 1", "SELECT 7 AS value", { grid: g(0, 0, 6, 4) })]),
        section("Details", [panel(20, "table", "Table", SQL.table, { grid: g(0, 0, 12, 8) })]),
      ], { time: { from: "now-6h", to: "now", timezone: "browser", refresh: null }, variables: [custom("env", null), custom("region", "Region"), custom("secret", null, "variable")] });
      const r = await t.api.create("transit", d);
      t.assert(r.status === 201, `create ${UID90}: ${r.status} ${r.text.slice(0, 300)}`);
      const p = await openAt(t, t.dash(UID90, "refresh=off"), 1280);
      await p.waitFor(`document.querySelector('[data-testid="panel-1"]')`, { what: "the first panel" });
      await p.idle();
      t.eq(await p.count(p.tid("add-button")), 0, "the view has no Add button");
      await enterEdit(p);
      // The draft goes to the storage 500 ms after a change: each read waits for the value that it expects.
      const draftPanels = async (index, n) => (await waitDraft(p, UID90, (x) => x.document.sections[index].panels.length === n)).document.sections[index].panels.length;
      const draftSections = async (n) => (await waitDraft(p, UID90, (x) => x.document.sections.length === n)).document.sections.length;
      const menuTexts = () => p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`);

      // 1. The shape: two buttons that touch (main part and chevron), 32 px high, the main part has the label "Add" and the name "Add panel".
      const shape = await p.eval(`(() => { const m = document.querySelector('[data-testid="add-button"]'); const c = document.querySelector('[data-testid="add-menu"]'); const a = m.getBoundingClientRect(); const b = c.getBoundingClientRect(); return { label: m.textContent.trim(), name: m.getAttribute("aria-label"), tip: m.dataset.tooltip, mainW: a.width, menuW: b.width, h: [a.height, b.height], touch: a.right === b.left, haspopup: c.getAttribute("aria-haspopup"), menuName: c.textContent.trim(), group: m.parentElement.getAttribute("role") }; })()`);
      t.eq(shape, { label: "Add", name: "Add panel", tip: "Add panel", mainW: 68, menuW: 28, h: [32, 32], touch: true, haspopup: "menu", menuName: "Add menu", group: "group" }, "the split button: label, name, sizes, joined parts");

      // 2. The menu: its items, its hooks, aria-expanded, and Escape that gives the focus back to the chevron.
      await p.click(p.tid("add-menu"));
      await p.waitSel(".menu");
      t.eq(await menuTexts(), ["Add panel", "Add text panel", "Add section", "Place a variable in the grid"], "the items of the menu of Add");
      t.eq(await p.eval(`["add-panel", "add-section", "add-variable"].map((id) => !!document.querySelector('.menu [data-testid="' + id + '"]'))`), [true, true, true], "the hooks of the items");
      t.eq(await p.attr(p.tid("add-menu"), "aria-expanded"), "true", "the chevron shows the open menu");
      t.eq(await active(p), "add-panel", "the first item has the focus");
      await p.press("Escape");
      await p.waitGone(".menu");
      t.eq(await active(p), "add-menu", "Escape gives the focus back to the chevron");

      // 3. One click on the main part adds a panel to the last section and opens its editor.
      const panels0 = 1; // the section Details has the table; the draft has no change yet, so it is not in the storage
      await p.click(p.tid("add-button"));
      await p.waitSel(p.tid("panel-editor"));
      await p.idle();
      t.eq(await draftPanels(1, panels0 + 1), panels0 + 1, "one click: a panel is in the last section");
      t.assert(/editPanel=\d+/.test(await p.eval("location.search")), "the address names the new panel");
      t.eq(await p.eval(`[document.querySelector('[data-testid="add-button"]').disabled, document.querySelector('[data-testid="add-menu"]').disabled]`), [true, true], "Add is in its place and not active in the panel editor");
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("panel-editor"));
      await p.idle();
      t.eq(await p.eval(`[document.querySelector('[data-testid="add-button"]').disabled, document.querySelector('[data-testid="add-menu"]').disabled]`), [false, false], "Add is active again in the dashboard edit");

      // 4. Add section from the menu.
      const sections0 = 2;
      await addItem(p, "add-section");
      await p.waitFor(`document.querySelectorAll('.edit-grid [data-testid^="section-"]').length >= ${sections0 + 1}`, { what: "a new section" });
      t.eq(await draftSections(sections0 + 1), sections0 + 1, "Add section adds one section");
      t.eq(await p.text(p.tid("layout-status")), "Section added.", "a status for assistive technology");

      // 5. Place a variable: two variables can go (env, region; secret has no control). The menu asks, then each one goes to the grid.
      const lastSection = sections0;
      await p.click(p.tid("add-menu"));
      await p.waitSel(".menu");
      t.eq(await p.eval(`document.querySelector('.menu [data-testid="add-variable"]').disabled`), false, "the item is active with variables to place");
      await p.click('.menu [data-testid="add-variable"]');
      await p.waitSel(p.tid("add-variable-list"));
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="add-variable-list"] [role="menuitem"]')].map((e) => e.textContent.trim())`), ["env", "Region"], "a menu of the variables that can go to the grid");
      await p.click(p.tid("add-variable-region"));
      await p.waitSel(p.tid("placement-region"));
      let placed = (await waitDraft(p, UID90, (x) => x.document.variables.find((v) => v.name === "region").placement)).document.variables.find((v) => v.name === "region").placement;
      t.eq([placed.section, placed.x, placed.w, placed.h], [lastSection, 0, 6, 2], "the control is at the bottom of the last section, 6 columns, 2 rows");
      t.eq(await p.eval(`document.querySelector('[data-testid="section-${lastSection}"]').contains(document.querySelector('[data-testid="placement-region"]'))`), true, "the control shows in the last section");
      // One variable is left: the item places it at once, with no menu.
      await addItem(p, "add-variable");
      await p.waitSel(p.tid("placement-env"));
      t.eq(await p.exists(".menu"), false, "one variable: no menu of choice");
      // None is left: the item is not active.
      await p.click(p.tid("add-menu"));
      await p.waitSel(".menu");
      t.eq(await p.eval(`document.querySelector('.menu [data-testid="add-variable"]').disabled`), true, "no variable to place: the item is not active");
      await p.press("Escape");
      await p.waitGone(".menu");

      // 6. The menu More actions has no Add panel and no Add section any more.
      await p.click(p.tid("dashboard-menu"));
      await p.waitSel(".menu");
      t.eq(await menuTexts(), ["Dashboard JSON", "Settings", "History", "Export", "Delete"], "the menu More actions in edit mode");
      await p.press("Escape");
      await p.waitGone(".menu");

      // 7. The keyboard: Tab goes Add, the chevron, Save. Enter on the chevron opens the menu; the arrows move; Escape closes it.
      //    ArrowDown on the chevron opens the menu. Enter on Add adds a panel.
      await p.focus(p.tid("add-button"));
      t.eq(await active(p), "add-button", "Add takes the focus");
      const ring = await p.eval(`(() => { const s = getComputedStyle(document.activeElement); return [document.activeElement.matches(":focus-visible"), s.outlineStyle !== "none"]; })()`);
      t.eq(ring, [true, true], "the focus of Add is visible");
      await p.press("Tab");
      t.eq(await active(p), "add-menu", "Tab reaches the chevron");
      await p.press("Enter");
      await p.waitSel(".menu");
      t.eq(await active(p), "add-panel", "Enter opens the menu on its first item");
      await p.press("ArrowDown");
      t.eq(await active(p), "add-text", "ArrowDown moves to the next item (Add text panel, F29)");
      await p.press("Escape");
      await p.waitGone(".menu");
      t.eq(await active(p), "add-menu", "Escape gives the focus back");
      await p.press("ArrowDown");
      await p.waitSel(".menu");
      t.eq(await menuTexts(), ["Add panel", "Add text panel", "Add section", "Place a variable in the grid"], "ArrowDown on the chevron opens the menu");
      await p.press("Escape");
      await p.waitGone(".menu");
      await p.focus(p.tid("add-button"));
      const count = (x) => x.document.sections.reduce((n, s) => n + s.panels.length, 0);
      const total0 = count(await waitDraft(p, UID90));
      await p.press("Enter");
      await p.waitSel(p.tid("panel-editor"));
      t.eq(count(await waitDraft(p, UID90, (x) => count(x) === total0 + 1)), total0 + 1, "Enter on Add adds a panel");

      // 8. The label hides at 900 px and below: the button is an icon, its name and its tooltip stay. The chevron is 28 px, and 20 px up to 560 px.
      for (const [width, main, menu, labelShown] of [[901, 68, 28, true], [900, 36, 28, false], [561, 36, 28, false], [560, 36, 20, false], [390, 32, 20, false]]) {
        const q = await openAt(t, t.dash(UID90, "refresh=off"), width);
        await q.waitFor(`document.querySelector('[data-testid="panel-1"]')`, { what: "the first panel" });
        await enterEdit(q);
        const m = await q.eval(`(() => { const b = document.querySelector('[data-testid="add-button"]'); const l = b.querySelector(".btn__label").getBoundingClientRect(); return { w: b.getBoundingClientRect().width, h: b.getBoundingClientRect().height, label: l.width > 2, name: b.getAttribute("aria-label"), tip: b.dataset.tooltip, menuW: document.querySelector('[data-testid="add-menu"]').getBoundingClientRect().width }; })()`);
        t.eq(m, { w: main, h: 32, label: labelShown, name: "Add panel", tip: "Add panel", menuW: menu }, `${width}: the size of Add and its label`);
        if (width <= 900) {
          await q.click(q.tid("add-menu"));
          await q.waitSel(".menu");
          t.eq(await q.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`), ["Add panel", "Add text panel", "Add section", "Place a variable in the grid"], `${width}: the menu of Add still has its items`);
          await q.press("Escape");
        }
      }

      // 9. The state of the page does not depend on the menu: a viewer has no Add (B70), and the view mode has no Add button.
      const v = await openAt(t, t.dash(UID90, "refresh=off"), 1280);
      t.eq(await v.count(v.tid("add-button")), 0, "the view has no Add button");
    },
  },
  {
    id: "B150",
    name: "the time control, the switch Edit, the theme selector and More actions keep their x and y when the edit mode starts and ends, at 1920, 1280, 900, 768 and 390 px, light and dark; the tab order is the visual order (9.6, 9.9, 10.2)",
    async run(t) {
      const r = await t.api.create("transit", headerDoc(UID150, "A dashboard with a rather long title for the crumbs of a narrow window"));
      t.assert(r.status === 201, `create ${UID150}: ${r.status} ${r.text.slice(0, 300)}`);
      const found = [];
      // The rectangles of the four fixed buttons, and of the crumbs.
      const FIXED = `(() => { const box = (e) => { if (!e || e.offsetParent === null) return null; const b = e.getBoundingClientRect(); return [b.left, b.top, b.width, b.height].map((v) => Math.round(v * 100) / 100); }; const q = (id) => document.querySelector('[data-testid="' + id + '"]'); return { "time-control": box(q("time-control")), "edit-toggle": box(q("edit-toggle")), theme: box(document.querySelector(".theme-select__button")), "dashboard-menu": box(q("dashboard-menu")), crumbs: box(document.querySelector(".crumbs")), bar: box(q("edit-bar")) }; })()`;
      const KEYS = ["time-control", "edit-toggle", "theme", "dashboard-menu"];
      // A sampler: at each frame, the signature of the four. The layouts that it sees are the layouts that a viewer can see.
      const SAMPLE = `(() => { const log = (window.__fixed = []); const f = () => { const r = ${FIXED}; log.push(${JSON.stringify(KEYS)}.map((k) => r[k] && r[k].join(",")).join("|")); requestAnimationFrame(f); }; requestAnimationFrame(f); })()`;
      const differ = (a, b, label, what) => {
        for (const k of KEYS) if (!a[k] || !b[k] || a[k].some((v, i) => Math.abs(v - b[k][i]) > 0.01)) found.push(`${label} ${what}: ${k} ${JSON.stringify(a[k])} -> ${JSON.stringify(b[k])}`);
      };
      for (const scheme of ["dark", "light"]) {
        for (const width of [1920, 1280, 900, 768, 390]) {
          const label = `${scheme} ${width}`;
          const p = await openAt(t, t.dash(UID150, "refresh=off"), width, { scheme });
          await ready(p);
          const view = await p.eval(FIXED);
          await p.eval(SAMPLE);
          await p.click(p.tid("edit-toggle"));
          await p.waitSel(p.tid("edit-bar"));
          await p.waitSel('.edit-grid [data-testid^="panel-"][data-state]');
          await p.idle();
          await sleep(300);
          const edit = await p.eval(FIXED);
          differ(view, edit, label, "edit mode starts");
          const frames = await p.eval("window.__fixed");
          const layouts = new Set(frames);
          if (frames.length < 5) found.push(`${label}: the sampler saw only ${frames.length} frames`);
          if (layouts.size !== 1) found.push(`${label}: the four buttons had ${layouts.size} layouts during the start of the edit mode`);
          // The group is at the left of the time control: only the crumbs gave room.
          if (!edit.bar || edit.bar[0] + edit.bar[2] > edit["time-control"][0] + 0.01) found.push(`${label}: the group is not at the left of the time control ${JSON.stringify([edit.bar, edit["time-control"]])}`);
          if (edit.crumbs[2] >= view.crumbs[2]) found.push(`${label}: the crumbs did not get narrower (${view.crumbs[2]} -> ${edit.crumbs[2]})`);
          // The tab order is the visual order: each stop is at the right of the one before (disabled Save and Revert count by their place).
          const order = await p.eval(`(() => { const h = document.getElementById("app-header"); const nodes = [...h.querySelectorAll('a[href], button, input, [tabindex="0"]')].filter((e) => e.offsetParent !== null && e.getBoundingClientRect().width > 1); return nodes.map((e) => [e.dataset.testid || e.className, Math.round(e.getBoundingClientRect().left)]); })()`);
          order.forEach((x, i) => { if (i > 0 && x[1] < order[i - 1][1]) found.push(`${label}: the tab order differs from the visual order at ${x[0]} (${JSON.stringify(order.slice(Math.max(0, i - 2), i + 1))})`); });
          // The real key: Tab from the brand goes through the controls from the left to the right.
          await p.focus("a.brand");
          const seen = [];
          for (let i = 0; i < 30; i++) {
            await p.press("Tab");
            const at = await p.eval(`(() => { const a = document.activeElement; if (!a || !document.getElementById("app-header").contains(a)) return null; return [a.dataset.testid || a.className, Math.round(a.getBoundingClientRect().left)]; })()`);
            if (!at) break;
            seen.push(at);
          }
          seen.forEach((x, i) => { if (i > 0 && x[1] < seen[i - 1][1]) found.push(`${label}: Tab goes from ${seen[i - 1][0]} to ${x[0]}, which is at its left`); });
          if (!seen.some((x) => x[0] === "add-button") || !seen.some((x) => x[0] === "edit-toggle")) found.push(`${label}: Tab does not reach Add and the switch: ${JSON.stringify(seen)}`);
          // The crumbs end with an ellipsis and their tooltip has the whole title.
          const crumb = await p.eval(`(() => { const text = document.querySelector(".crumbs__item--current .crumbs__text"); return [text.scrollWidth > text.clientWidth, getComputedStyle(text).textOverflow, document.querySelector(".crumbs__item--current").title]; })()`);
          if (width <= 1280 && (crumb[0] !== true || crumb[1] !== "ellipsis")) found.push(`${label}: the long title has no ellipsis ${JSON.stringify(crumb)}`);
          if (!/rather long title/.test(crumb[2])) found.push(`${label}: the tooltip of the title has not the whole text`);
          // The edit mode ends: nothing moves back.
          await p.eval(SAMPLE);
          await p.click(p.tid("edit-toggle"));
          await p.waitGone(p.tid("edit-bar"));
          await p.idle();
          await sleep(300);
          const back = await p.eval(FIXED);
          differ(view, back, label, "edit mode ends");
          const framesEnd = await p.eval("window.__fixed");
          if (new Set(framesEnd).size !== 1) found.push(`${label}: the four buttons had ${new Set(framesEnd).size} layouts when the edit mode ended`);
        }
      }
      t.eq(found.slice(0, 12), [], "the four fixed buttons");
    },
  },
  {
    id: "B151",
    name: "the panel editor has no row for the back button: it gains the height of the old row at four widths, and the crumb of the dashboard is the way back from the panel view, edit mode and the dashboard, with the focus rule (10.2a, 10.3)",
    async run(t) {
      const r = await t.api.create("transit", headerDoc(UID151, "A dashboard with a rather long title for the crumbs of a narrow window"));
      t.assert(r.status === 201, `create ${UID151}: ${r.status} ${r.text.slice(0, 300)}`);
      const found = [];
      const view = (p) => p.eval(`document.documentElement.dataset.huneView`);
      const focusId = (p) => p.eval(`(() => { const a = document.activeElement; const r = a && a.closest("[data-testid]"); return r ? r.dataset.testid : (a ? a.tagName : null); })()`);
      const LAYOUT = `(() => { const b = (e) => { if (!e || e.offsetParent === null) return null; const x = e.getBoundingClientRect(); return [x.left, x.top, x.width, x.height].map((v) => Math.round(v * 100) / 100); }; const q = (s) => document.querySelector(s); return { header: b(q("#app-header")), main: b(q(".peditor__main")), ed: b(q('[data-testid="panel-editor"]')), top: b(q(".peditor__top")), back: b(q('[data-testid="editor-back"]')), add: b(q('[data-testid="add-button"]')), bar: !!q(".peditor__bar"), region: !!q('[data-testid="panel-editor"]'), win: innerHeight }; })()`;

      // 1. The height: no row between the header and the editor. The editor starts at the foot of the header, and at 901 px and more it fills the
      //    window down from there. The old row was 48 px, so the gain is 48 px.
      for (const scheme of ["dark", "light"]) {
        for (const width of [1920, 1280, 768, 390]) {
          const label = `${scheme} ${width}`;
          const p = await openAt(t, t.dash(UID151, "refresh=off&edit=1&editPanel=1"), width, { scheme });
          await p.waitSel(p.tid("panel-editor"));
          await p.idle();
          await sleep(200);
          const m = await p.eval(LAYOUT);
          if (m.bar) found.push(`${label}: the row .peditor__bar is still in the page`);
          // The editor box starts at the foot of the header; its top padding is the gap of ID-K302 (B208 measures it), so the main part starts 16 px lower.
          if (!m.ed || Math.abs(m.ed[1] - (m.header[1] + m.header[3])) > 0.5) found.push(`${label}: the editor starts at ${m.ed && m.ed[1]}, the header ends at ${m.header[1] + m.header[3]}`);
          if (width > 900 && Math.abs(m.ed[3] - (m.win - m.header[3])) > 0.5) found.push(`${label}: the editor is ${m.ed[3]} px high, expected ${m.win - m.header[3]} (the old row took 48 px)`);
          // The way back is in the header, in view, with room, and it does not overlap the first button of the group.
          if (!m.back || m.back[2] < 16 || m.back[1] < m.header[1] || m.back[1] + m.back[3] > m.header[1] + m.header[3]) found.push(`${label}: the back crumb is not visible in the header ${JSON.stringify(m.back)}`);
          else if (m.add && m.back[0] + m.back[2] > m.add[0] + 0.5) found.push(`${label}: the back crumb overlaps Add ${JSON.stringify([m.back, m.add])}`);
          if (width === 1280 && scheme === "dark") {
            // The same editor opened from edit mode keeps the layout when it opens and closes (no shift of the header).
            t.eq(await p.eval(`document.getElementById("app-header").getBoundingClientRect().height`), 44, "the header stays 44 px in the panel editor");
          }
        }
      }

      // 2. The way back from the panel view: the name says so, and the focus goes to the view region.
      const pv = await openAt(t, t.dash(UID151, "refresh=off&viewPanel=1"), 1280);
      await pv.waitFor(`document.documentElement.dataset.huneView === "panel-view"`, { what: "the panel view" });
      await pv.click(pv.tid("edit-toggle"));
      await pv.waitSel(pv.tid("panel-editor"));
      t.eq(await pv.attr(pv.tid("editor-back"), "aria-label"), "Back to panel view", "the name from the panel view");
      t.eq(await pv.attr(pv.tid("editor-back"), "title"), "Back to panel view", "the tooltip from the panel view");
      t.eq(await pv.eval(`document.querySelector('[data-testid="editor-back"]').tagName`), "A", "the back crumb is a link");
      t.eq(await pv.eval(`document.querySelector('[data-testid="editor-back"]').closest(".crumbs") !== null`), true, "the back crumb is in the crumbs of the header");
      const historyBefore = await pv.eval("history.length");
      await pv.click(pv.tid("editor-back"));
      await pv.waitFor(`document.documentElement.dataset.huneView === "panel-view"`, { what: "the panel view again" });
      t.eq(await pv.eval("history.length"), historyBefore, "Back to panel view pops: the history does not grow");
      t.eq(await focusId(pv), "view-panel", "the focus is in the view region");

      // 3. The way back from edit mode: the name is "Back to edit mode", the key Enter works, and the focus goes back to the control that opened it.
      const pe = await openAt(t, t.dash(UID151, "refresh=off"), 1280);
      await ready(pe);
      await enterEdit(pe);
      await pe.click(pe.tid("panel-1-menu"));
      await pe.click(pe.tid("panel-1-edit"));
      await pe.waitSel(pe.tid("panel-editor"));
      await pe.idle();
      t.eq(await pe.attr(pe.tid("editor-back"), "aria-label"), "Back to edit mode", "the name from edit mode");
      const opener = await pe.eval(`window.history.state && window.history.state.hune && window.history.state.hune.from`);
      t.assert(typeof opener === "string" && /edit=1/.test(opener), `the entry has its origin: ${opener}`);
      // The order of the keys: the back crumb comes after the brand and before the time control ("Add" is not active in the panel editor).
      await pe.focus("a.brand");
      const seen = [];
      for (let i = 0; i < 6; i++) {
        await pe.press("Tab");
        seen.push(await focusId(pe));
      }
      t.assert(seen.indexOf("editor-back") >= 0 && seen.indexOf("editor-back") < seen.indexOf("time-back"), `Tab reaches the back crumb before the time control: ${JSON.stringify(seen)}`);
      await pe.focus(pe.tid("editor-back"));
      await pe.press("Enter");
      await pe.waitGone(pe.tid("panel-editor"));
      await pe.idle();
      t.eq(await view(pe), "edit", "Enter on the back crumb shows edit mode");
      t.eq(await focusId(pe), "panel-1-menu", "the focus is back on the control that opened the editor");
      t.eq(await pe.eval(`!!document.querySelector('[data-testid="edit-bar"]')`), true, "edit mode stays on");

      // 4. The way back from the dashboard view (Edit in the panel menu opens the editor with the dashboard as origin).
      const pd = await openAt(t, t.dash(UID151, "refresh=off"), 1280);
      await ready(pd);
      await pd.click(pd.tid("panel-1-menu"));
      await pd.click(pd.tid("panel-1-edit"));
      await pd.waitSel(pd.tid("panel-editor"));
      await pd.idle();
      t.eq(await pd.attr(pd.tid("editor-back"), "aria-label"), "Back to dashboard", "the name from the dashboard");
      await pd.click(pd.tid("editor-back"));
      await pd.waitGone(pd.tid("panel-editor"));
      await pd.idle();
      t.eq(await view(pd), "view", "Back to dashboard shows the dashboard view");
      t.eq(await focusId(pd), "panel-1-menu", "the focus is back on the control that opened the editor");

      // 5. A deep link has no origin: the name is "Back to edit mode". A modified click is not the event: the link keeps its address.
      const dl = await openAt(t, t.dash(UID151, "refresh=off&edit=1&editPanel=1"), 1280);
      await dl.waitSel(dl.tid("panel-editor"));
      t.eq(await dl.attr(dl.tid("editor-back"), "aria-label"), "Back to edit mode", "a deep link: the name");
      t.assert(/^.*d\/hk151/.test(await dl.eval(`document.querySelector('[data-testid="editor-back"]').getAttribute("href")`)), "the back crumb has an address");

      // 6. A phone: the back crumb is the arrow alone (its name stays), and it works.
      const ph = await openAt(t, t.dash(UID151, "refresh=off&edit=1&editPanel=1"), 390);
      await ph.waitSel(ph.tid("panel-editor"));
      await ph.idle();
      const small = await ph.eval(`(() => { const a = document.querySelector('[data-testid="editor-back"]'); const t = a.querySelector(".crumbs__text").getBoundingClientRect(); const b = a.getBoundingClientRect(); return { w: b.width, textHidden: t.width <= 2, label: a.getAttribute("aria-label"), icon: !!a.querySelector(".icon") }; })()`);
      t.eq([small.textHidden, small.icon, small.w >= 16], [true, true, true], "390: the arrow alone " + JSON.stringify(small));
      await ph.click(ph.tid("editor-back"));
      await ph.waitGone(ph.tid("panel-editor"));
      t.eq(await view(ph), "edit", "390: the arrow goes back to edit mode");
      t.eq(found.slice(0, 12), [], "the height of the panel editor and the back crumb");
    },
  },
];
