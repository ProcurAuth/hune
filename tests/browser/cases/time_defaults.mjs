// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B231 and B232 (9.5, 10.2a, 10.9): the range and the refresh in edit mode.
// B231: a range and a refresh that the person chose in edit mode stay when the edit mode ends (the switch, the entry by `?edit=1`, a save).
// B232: the Save dialog offers the current range and the current refresh as the defaults of the dashboard, and the button "Save" is active
//       when only one of them differs from the default.
import { enterEdit, exitEdit, makeSeed, setPanelTitle, saveDialog } from "../lib/ed.mjs";

// The page opens with no range in the address, so the range is the default of the document. The seed data lie in another range: the panels
// may show "No data", so the case waits for the frames and not for data.
async function framesShown(p) {
  await p.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-state]').length >= 3`, { what: "the panel frames" });
  await p.idle();
}

// The cases make the document with the range now-6h to now and no refresh as its defaults. They choose "Last 24 hours" and "30s".
const DEFAULTS = { from: "now-6h", to: "now", timezone: "browser", refresh: null };
const seed = (t, uid) => makeSeed(t, uid, (d) => { d.time = { ...DEFAULTS }; });
async function setRange(p, label) {
  await p.click(p.tid("time-picker-button"));
  await p.waitSel(".picker__popup");
  await p.clickText(".picker__quick-item", label);
  await p.waitGone(".picker__popup");
  await p.idle();
}

async function setRefresh(p, value) {
  await p.click(p.tid("refresh-interval"));
  await p.waitSel(".menu");
  await p.clickText('.menu [role="menuitemradio"]', value);
  await p.waitGone(".menu");
  await p.idle();
}

const controls = (p) => p.eval(`({
  range: document.querySelector('[data-testid="time-picker-button"]').textContent.trim(),
  refresh: document.querySelector('[data-testid="refresh-interval"]').getAttribute("aria-label"),
  search: location.search,
})`);

const editEnded = (p) => p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null && document.querySelector(".edit-grid") === null`, { what: "the edit mode ends" });
const saveState = (p) => p.eval(`({ disabled: document.querySelector('[data-testid="save-button"]').disabled, title: document.querySelector('[data-testid="save-button"]').title, revert: document.querySelector('[data-testid="discard-button"]').disabled })`);
const options = (p) => p.eval(`[...document.querySelectorAll('[data-testid="save-defaults"] label.check')].map((e) => [e.textContent.trim(), e.querySelector("input").checked])`);
const timeOf = async (t, uid) => (await t.api.dashboard(uid)).json.document.time;

export const cases = [
  {
    id: "B231",
    name: "edit mode: a range and a refresh that the person chose in edit mode stay after the edit mode ends by the switch, by the entry `?edit=1` and by a save",
    async run(t) {
      const uid = "ed231";
      await seed(t, uid);
      const p = await t.open(t.url(`d/${uid}`));
      await framesShown(p);

      // The switch: the chosen values are on the controls and in the address after the edit mode.
      await enterEdit(p);
      await setRange(p, "Last 24 hours");
      await setRefresh(p, "30s");
      t.eq(await controls(p), { range: "Last 24 hours", refresh: "Refresh interval: 30s", search: "?from=now-24h&to=now&refresh=30s&edit=1" }, "in edit mode: the chosen values are on the controls and in the address");
      await exitEdit(p);
      await editEnded(p);
      await p.idle();
      t.eq(await controls(p), { range: "Last 24 hours", refresh: "Refresh interval: 30s", search: "?from=now-24h&to=now&refresh=30s" }, "after the switch goes off: the range and the refresh are kept, on the controls and in the address");

      // The entry by the address `?edit=1`: the edit mode ends by a replace, and the values stay too.
      await p.goto(t.url(`d/${uid}/x?edit=1`));
      await p.waitSel(p.tid("edit-bar"));
      await p.waitSel('.edit-grid [data-testid^="panel-"][data-state]');
      await setRange(p, "Last 24 hours");
      await setRefresh(p, "1m");
      await exitEdit(p);
      await editEnded(p);
      await p.idle();
      t.eq(await controls(p), { range: "Last 24 hours", refresh: "Refresh interval: 1m", search: "?from=now-24h&to=now&refresh=1m" }, "the entry by ?edit=1: the values stay after the switch goes off");

      // A save of a change of the draft: the range and the refresh stay, and the stored defaults do not change.
      const p2 = await t.open(t.url(`d/${uid}`));
      await framesShown(p2);
      await enterEdit(p2);
      await setRange(p2, "Last 24 hours");
      await setRefresh(p2, "30s");
      await setPanelTitle(p2, 1, "Renamed in B231");
      await saveDialog(p2, "title only");
      await editEnded(p2);
      await p2.idle();
      t.eq(await controls(p2), { range: "Last 24 hours", refresh: "Refresh interval: 30s", search: "?from=now-24h&to=now&refresh=30s" }, "after a save with no default box: the range and the refresh are kept");
      t.eq(await timeOf(t, uid), DEFAULTS, "the stored defaults did not change");
    },
  },
  {
    id: "B232",
    name: "Save dialog: the current range and the current refresh are offered as the defaults only when they differ, both boxes are off at first, the button Save is active for a change of the range or the refresh alone, and the saved document and the address follow",
    async run(t) {
      const uid = "ed232";
      await seed(t, uid);
      const p = await t.open(t.url(`d/${uid}`));
      await framesShown(p);
      await enterEdit(p);

      // Nothing differs: no change in the draft, "Save" is not active.
      t.eq(await saveState(p), { disabled: true, title: "There are no changes to save", revert: true }, "range and refresh equal the defaults: Save is not active");

      // Only the range differs: "Save" is active, "Revert" is not (it acts on the draft). The dialog has one box.
      await setRange(p, "Last 24 hours");
      t.eq(await saveState(p), { disabled: false, title: "Save the time range or the refresh as the default", revert: true }, "the range differs: Save is active and Revert is not");
      await p.click(p.tid("save-button"));
      await p.waitSel(p.tid("save-dialog"));
      t.eq(await options(p), [["Save the current time range as the default", false]], "one box for the range, off at first");
      t.eq(await p.eval(`document.querySelector('[data-testid="save-defaults"] .field__hint').textContent`), "Now: now-24h to now. Default: now-6h to now.", "the hint gives the current and the default range");
      await p.press("Escape");
      await p.waitGone(p.tid("save-dialog"));

      // The refresh differs too: two boxes. A range that returns to the default removes its box.
      await setRefresh(p, "30s");
      await p.click(p.tid("save-button"));
      await p.waitSel(p.tid("save-dialog"));
      t.eq(await options(p), [["Save the current time range as the default", false], ["Save the current refresh as the default", false]], "two boxes, both off at first");
      t.eq(await p.eval(`document.querySelectorAll('[data-testid="save-defaults"] .field__hint')[1].textContent`), "Now: 30s. Default: Off.", "the hint gives the current and the default refresh");

      // The range alone: the saved document has the new range and no refresh. The edit mode ends and the address names the refresh only.
      await p.click(p.tid("save-default-range"));
      await p.click(p.tid("save-confirm"));
      await editEnded(p);
      await p.idle();
      t.eq(await timeOf(t, uid), { from: "now-24h", to: "now", timezone: "browser", refresh: null }, "the saved document has the new range and the old refresh");
      t.eq(await controls(p), { range: "Last 24 hours", refresh: "Refresh interval: 30s", search: "?refresh=30s" }, "the address has the refresh only: the range equals the new default");

      // The refresh alone, in a second edit mode: only the refresh differs now. The address is empty after the save.
      await enterEdit(p);
      t.eq((await saveState(p)).disabled, false, "the refresh differs from the default: Save is active");
      await p.click(p.tid("save-button"));
      await p.waitSel(p.tid("save-dialog"));
      t.eq(await options(p), [["Save the current refresh as the default", false]], "one box for the refresh");
      await p.click(p.tid("save-default-refresh"));
      await p.click(p.tid("save-confirm"));
      await editEnded(p);
      await p.idle();
      t.eq(await timeOf(t, uid), { from: "now-24h", to: "now", timezone: "browser", refresh: "30s" }, "the saved document has the new refresh");
      t.eq(await controls(p), { range: "Last 24 hours", refresh: "Refresh interval: 30s", search: "" }, "range and refresh equal the defaults: the address is empty");
    },
  },
];
