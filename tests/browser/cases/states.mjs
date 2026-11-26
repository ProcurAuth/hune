// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B72 to B79 and B87 to B89 (10.2a): the states of the page and their transitions.
// B72 panel editor from a panel view. B73 panel editor from the dashboard. B74 panel view. B75 dashboard edit. B76 Back and Forward.
// B77 reload. B78 deep links (the vectors FE-26). B79 rights. B87 leave warnings. B88 entry redirect and states. B89 a walk of events.
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { ROOT, seedDocument } from "../lib/env.mjs";
import { byLabel, enterEdit, moreItem } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";
import { normalSearch, parseView } from "../../../web/js/view_state.js";

const IDS = [1, 2, 3, 7];
const CTX = { panels: IDS, canEdit: true };

function statesDoc(uid, title) {
  const g = (x, y, w, h) => ({ x, y, w, h });
  return doc(uid, title, [
    section("Overview", [
      panel(1, "stat", "Stat one", "SELECT 7 AS value", { grid: g(0, 0, 6, 4) }),
      panel(2, "stat", "Stat two", "SELECT 14 AS value", { grid: g(6, 0, 6, 4) }),
      panel(3, "timeseries", "Series", SQL.series, { grid: g(0, 4, 12, 8) }),
    ]),
    section("Details", [panel(7, "table", "Table", SQL.table, { grid: g(0, 0, 12, 8) })]),
  ], { time: { from: "now-6h", to: "now", timezone: "browser", refresh: null } });
}

// The log of the writes of the history, and the log of the frames (installed in each new document).
const HLOG = `(() => {
  window.__hlog = [];
  for (const op of ["pushState", "replaceState"]) {
    const orig = history[op].bind(history);
    history[op] = (s, t, u) => { window.__hlog.push(op); return orig(s, t, u); };
  }
})()`;

const FRAMES = `(() => {
  window.__frames = [];
  let n = 0;
  const tick = () => {
    const root = document.documentElement;
    const q = (s) => document.querySelector(s);
    const main = q("#app-main");
    const tb = q('[data-testid="dashboard-toolbar"]');
    window.__frames.push({ n: n++, hold: root.hasAttribute("data-hune-hold"), view: root.getAttribute("data-hune-view"), kids: main ? main.children.length : -1, tb: tb ? Math.round(tb.getBoundingClientRect().top * 2) / 2 : null });
    if (n < 100) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
})()`;

async function openPage(t, url, { frames = false, width = 1400 } = {}) {
  const p = await t.page({ width, height: 900 });
  await p.send("Page.addScriptToEvaluateOnNewDocument", { source: HLOG });
  if (frames) await p.send("Page.addScriptToEvaluateOnNewDocument", { source: FRAMES });
  await p.goto(url);
  await p.idle();
  return p;
}

const view = (p) => p.eval(`document.documentElement.getAttribute("data-hune-view")`);
const origin = (p) => p.eval(`document.documentElement.getAttribute("data-hune-origin")`);
const histLen = (p) => p.eval("history.length");
const search = (p) => p.eval("location.search");
const hlog = (p) => p.eval("window.__hlog.slice()");
const clearLog = (p) => p.eval("window.__hlog.length = 0");
const focused = (p) => p.eval(`(() => { const a = document.activeElement; if (!a) return null; const r = a.closest('[data-testid]'); return r ? r.dataset.testid : a.tagName; })()`);

/** The view names of the URL: the part that a state chooses (10.2a). */
async function names(p) {
  return p.eval(`(() => { const s = new URLSearchParams(location.search); const o = {}; for (const k of ["viewPanel", "edit", "editPanel", "editview", "version", "base"]) if (s.has(k)) o[k] = s.get(k); return o; })()`);
}

async function waitView(p, name, what = "") {
  await p.waitFor(`document.documentElement.getAttribute("data-hune-view") === ${JSON.stringify(name)}`, { what: `state ${name} ${what}`, timeout: 10000 });
  await p.idle();
  await sleep(50);
}

/** The views that the page shows at once: the base grid, the view of a panel and the content of the editor. The rule is one. */
const SHOWN = `(() => {
  const vis = (e) => !!e && !e.hidden && e.getClientRects().length > 0;
  const host = document.querySelector('[data-testid="editor-host"]');
  const pv = document.querySelector('[data-testid="view-panel"]');
  const baseGrid = [...document.querySelectorAll('[data-testid="section-0"]')].filter((e) => !(host && host.contains(e)) && vis(e)).length;
  return { grid: baseGrid > 0, panelView: vis(pv) && pv.childElementCount > 0, editor: !!host && host.childElementCount > 0 && vis(host) };
})()`;

async function oneView(t, p, what) {
  const s = await p.eval(SHOWN);
  const n = Number(s.grid) + Number(s.panelView) + Number(s.editor);
  t.assert(n === 1, `${what}: ${n} views in the page ${JSON.stringify(s)}`);
  return s;
}

/** The state that the page must show for the URL: the page of `parseView` (the rule 1 of 10.2a). */
async function expectedPage(p, ctx = CTX) {
  return parseView(await search(p), ctx).state.page;
}

const click = (p, id) => p.click(p.tid(id));
async function panelMenuItem(p, id, item) {
  await click(p, `panel-${id}-menu`);
  await p.waitSel(`.menu ${p.tid(`panel-${id}-${item}`)}`);
  await p.click(`.menu ${p.tid(`panel-${id}-${item}`)}`);
}
/** The panel menu item of the panel in the view of a panel (the hidden grid has the same hooks). */
async function viewMenuItem(p, id, item) {
  await p.click(`[data-testid="view-panel"] ${p.tid(`panel-${id}-menu`)}`);
  await p.waitSel(`.menu ${p.tid(`panel-${id}-${item}`)}`);
  await p.click(`.menu ${p.tid(`panel-${id}-${item}`)}`);
}
/** The position in the history (the first entry is the blank page that the tab starts with) and the number of entries. */
const navIndex = (p) => p.eval("[navigation.currentEntry.index, navigation.entries().length]");
const noDialog = (p) => p.eval(`document.querySelectorAll("dialog[open]").length === 0`);
const panelTitleIn = (p, root) => p.eval(`(() => { const e = document.querySelector(${JSON.stringify(root)} + " .panel__title"); return e ? e.textContent.trim() : null; })()`);

async function setTitle(p, id, title) {
  await p.click(p.tid("tab-panel"));
  const sel = await byLabel(p, "Title");
  await p.fill(sel, title);
  await sleep(450);
}

async function seed(t, uid, title = `Case ${uid}`) {
  const r = await t.api.create("transit", statesDoc(uid, title));
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 200)}`);
}

async function ready(p) {
  await waitPanelsReady(p, [1, 2, 3], "ready").catch(() => {});
  await p.idle();
}

export const cases = [
  {
    id: "B72",
    name: "the panel editor from a panel view: Edit opens it, Back to panel view pops, Forward returns, the dialog and Save and Discard (10.2a)",
    async run(t) {
      const uid = "st72";
      await seed(t, uid);
      const p = await openPage(t, t.dash(uid, "viewPanel=2"));
      await waitView(p, "panel-view");
      await oneView(t, p, "panel view");
      const before = await histLen(p);
      await click(p, "edit-toggle");
      await waitView(p, "panel-edit");
      t.eq(await origin(p), "view", "the origin is a view");
      t.eq(await names(p), { edit: "1", editPanel: "2" }, "the names of the panel editor: no viewPanel");
      t.eq(await oneView(t, p, "panel editor"), { grid: false, panelView: false, editor: true }, "the editor alone");
      t.eq(await focused(p), "panel-editor", "the focus is in the editor region");
      t.eq(await histLen(p), before + 1, "Edit pushes one entry");
      t.eq([await p.attr(p.tid("editor-back"), "aria-label"), await p.attr(p.tid("editor-back"), "title")], ["Back to panel view", "Back to panel view"], "the name and the tooltip of the way back");
      // Back to panel view: a pop. The history does not grow, Forward shows the editor.
      await clearLog(p);
      await click(p, "editor-back");
      await waitView(p, "panel-view");
      t.eq(await names(p), { viewPanel: "2" }, "the panel view with its URL");
      t.eq(await histLen(p), before + 1, "no new entry");
      t.assert(!(await hlog(p)).includes("pushState"), "no pushState: " + JSON.stringify(await hlog(p)));
      t.eq(await oneView(t, p, "panel view again"), { grid: false, panelView: true, editor: false }, "the panel view alone");
      t.eq(await focused(p), "view-panel", "the focus is in the view region");
      await p.eval("history.forward()");
      await waitView(p, "panel-edit", "after Forward");
      // A changed title: edit-off and Back to panel view show the dialog.
      await setTitle(p, 2, "Changed two");
      await click(p, "editor-back");
      await p.waitSel(p.tid("exit-dialog"));
      await p.clickText("dialog .btn", "Stay");
      await p.waitGone("dialog[open]");
      t.eq(await view(p), "panel-edit", "Stay keeps the editor");
      await click(p, "edit-toggle");
      await p.waitSel(p.tid("exit-dialog"));
      await p.clickText("dialog .btn", "Stay");
      await p.waitGone("dialog[open]");
      // Save shows the panel view with the new title.
      await click(p, "save-button");
      await p.waitSel(p.tid("save-dialog"));
      await click(p, "save-confirm");
      await waitView(p, "panel-view", "after Save");
      t.eq(await names(p), { viewPanel: "2" }, "Save gives the panel view URL");
      await p.waitFor(`(document.querySelector('[data-testid="view-panel"] .panel__title') || {}).textContent === "Changed two"`, { what: "the saved title in the panel view" });
      t.eq(await p.eval(`document.querySelectorAll('[data-testid="edit-bar"], [data-testid="edit-grid"]').length`), 0, "no hook of the editor");
      t.eq(await p.attr(p.tid("edit-toggle"), "aria-checked"), "false", "the switch is off");
      // Discard in the dialog shows the old title.
      await click(p, "edit-toggle");
      await waitView(p, "panel-edit");
      await setTitle(p, 2, "Second change");
      await click(p, "editor-back");
      await p.waitSel(p.tid("exit-dialog"));
      await p.clickText("dialog .btn", "Discard");
      await waitView(p, "panel-view", "after Discard");
      t.eq(await panelTitleIn(p, '[data-testid="view-panel"]'), "Changed two", "Discard shows the saved title");
      t.eq(await p.eval(`localStorage.getItem("hune.draft.v1.${uid}")`), null, "the draft is gone");
    },
  },
  {
    id: "B73",
    name: "the panel editor from the dashboard: the origin of each opening, Back pops, no View item in edit mode, deep link, Discard of a new panel (10.2a)",
    async run(t) {
      const uid = "st73";
      await seed(t, uid);
      const p = await openPage(t, t.dash(uid));
      await ready(p);
      // From the view: the item Edit of the panel menu (origin view).
      const l0 = await histLen(p);
      await panelMenuItem(p, 1, "edit");
      await waitView(p, "panel-edit");
      t.eq(await origin(p), "view", "origin of Edit in the view");
      await clearLog(p);
      await click(p, "editor-back");
      await waitView(p, "view");
      t.eq(await histLen(p), l0 + 1, "the stack is as before plus the entry that Forward keeps");
      t.assert(!(await hlog(p)).includes("pushState"), "Back is a pop");
      t.eq(await focused(p), "panel-1-menu", "the focus returns to the menu button");
      // From edit mode (origin edit): no View item.
      await enterEdit(p);
      await click(p, "panel-1-menu");
      await p.waitSel(".menu");
      t.eq(await p.eval(`!!document.querySelector('.menu [data-testid="panel-1-view"]')`), false, "no item View in edit mode");
      await p.press("Escape");
      const l1 = await histLen(p);
      await panelMenuItem(p, 2, "edit");
      await waitView(p, "panel-edit");
      t.eq(await origin(p), "edit", "origin of Edit in edit mode");
      await clearLog(p);
      await click(p, "editor-back");
      await waitView(p, "edit");
      t.eq(await histLen(p), l1 + 1, "the same stack");
      t.assert(!(await hlog(p)).includes("pushState"), "Back is a pop in edit mode");
      t.eq(await focused(p), "panel-2-menu", "the focus returns to the menu button of the panel");
      // Add panel (origin edit), then Discard of the new panel shows the state edit with the bar.
      await moreItem(p, "add-panel");
      await waitView(p, "panel-edit");
      t.eq(await origin(p), "edit", "origin of Add panel");
      await click(p, "discard-button");
      await p.waitSel("dialog[open]");
      await p.clickText("dialog .btn", "Discard");
      await waitView(p, "edit");
      t.eq(await p.exists(p.tid("edit-bar")), true, "the edit bar stays");
      t.eq(await p.exists(p.tid("edit-grid")), true, "the draft grid shows");
      // A deep link has no origin: Back to dashboard replaces.
      const d = await openPage(t, t.dash(uid, "edit=1&editPanel=1"));
      await waitView(d, "panel-edit");
      t.eq(await origin(d), null, "a deep link has no origin");
      t.eq(await d.attr(d.tid("editor-back"), "aria-label"), "Back to edit mode", "a deep link goes back to edit mode");
      const len = await histLen(d);
      await clearLog(d);
      await click(d, "editor-back");
      await waitView(d, "edit");
      t.eq(await histLen(d), len, "the stack does not grow");
      t.eq(await hlog(d), ["replaceState"], "Back to dashboard replaces");
    },
  },
  {
    id: "B74",
    name: "the panel view: open, Escape, the way back, the same arrow at 400 px, View of the shown panel, History, changes of the range (10.2a)",
    async run(t) {
      const uid = "st74";
      await seed(t, uid);
      const p = await openPage(t, t.dash(uid));
      await ready(p);
      const l0 = await histLen(p);
      await panelMenuItem(p, 2, "view");
      await waitView(p, "panel-view");
      t.eq(await names(p), { viewPanel: "2" }, "view-panel");
      t.eq(await histLen(p), l0 + 1, "view-panel pushes one entry");
      // The item View of the shown panel adds no entry.
      await viewMenuItem(p, 2, "view");
      await sleep(200);
      t.eq(await histLen(p), l0 + 1, "View for the shown panel adds no entry");
      // A change of the range keeps viewPanel and replaces.
      await clearLog(p);
      await click(p, "time-picker-button");
      await p.waitSel(".picker__popup");
      await p.clickText(".picker__quick-item", "Last 24 hours");
      await p.waitGone(".picker__popup");
      await p.idle();
      t.eq(await hlog(p), ["replaceState"], "the range writes replaceState");
      t.assert(/viewPanel=2/.test(await search(p)) && /from=now-24h/.test(await search(p)), "the URL keeps viewPanel: " + (await search(p)));
      // The history and back: the panel view again.
      await moreItem(p, "history-button");
      await waitView(p, "history");
      t.eq(await oneView(t, p, "history from the panel view"), { grid: false, panelView: false, editor: true }, "the history alone");
      await click(p, "history-back");
      await waitView(p, "panel-view");
      t.assert(/viewPanel=2/.test(await search(p)), "the panel view URL: " + (await search(p)));
      // Escape closes (a push), the focus returns to the menu button.
      const i1 = (await navIndex(p))[0];
      await p.press("Escape");
      await waitView(p, "view");
      t.eq((await navIndex(p))[0], i1 + 1, "close-view pushes");
      t.eq(await names(p), {}, "no view name");
      t.eq(await focused(p), "panel-2-menu", "the focus returns to the menu button of the panel");
      // The way back (the arrow before the selector) goes to the dashboard.
      await panelMenuItem(p, 2, "view");
      await waitView(p, "panel-view");
      await p.eval(`document.querySelector('[data-testid="view-back"]').click()`);
      await waitView(p, "view");
      // The same arrow at 400 px: it never hides.
      const q = await openPage(t, t.dash(uid, "viewPanel=2"), { width: 400 });
      await waitView(q, "panel-view");
      await q.click(q.tid("view-back"));
      await waitView(q, "view");
    },
  },
  {
    id: "B75",
    name: "the dashboard edit mode: edit-on, open and leave of each view keep the stack, History keeps edit=1, edit-off with changes and its three buttons, Save (10.2a)",
    async run(t) {
      const uid = "st75";
      await seed(t, uid);
      const p = await openPage(t, t.dash(uid));
      await ready(p);
      await enterEdit(p);
      await waitView(p, "edit");
      t.eq(await names(p), { edit: "1" }, "edit-on");
      const base = await histLen(p);
      for (const [item, name, back] of [["json-tab", "json", "editor-back"], ["dash-settings", "settings", "editor-back"], ["history-button", "history", "history-back"]]) {
        await moreItem(p, item);
        await waitView(p, name);
        t.eq(await origin(p), "edit", `origin of ${name}`);
        t.assert(/edit=1/.test(await search(p)), `${name} keeps edit=1: ` + (await search(p)));
        t.eq(await oneView(t, p, name), { grid: false, panelView: false, editor: true }, `${name} shows one view`);
        t.eq(await p.attr(p.tid("edit-toggle"), "aria-checked"), "true", `${name}: the switch stays on`);
        await click(p, back);
        await waitView(p, "edit");
        t.eq(await histLen(p), base + 1, `the stack after ${name} and leave`);
        t.eq(await oneView(t, p, "after leave"), { grid: false, panelView: false, editor: true }, "the draft grid alone");
      }
      // Revert keeps the edit mode and asks first.
      await setTitle2(p);
      await click(p, "discard-button");
      await p.waitSel("dialog[open]");
      await p.clickText("dialog .btn", "Discard");
      await p.waitGone("dialog[open]");
      t.eq(await view(p), "edit", "Revert stays in edit mode");
      // edit-off with changes: Stay, Discard, Save.
      await setTitle2(p);
      await click(p, "edit-toggle");
      await p.waitSel(p.tid("exit-dialog"));
      await p.clickText("dialog .btn", "Stay");
      await p.waitGone("dialog[open]");
      t.eq(await view(p), "edit", "Stay");
      await click(p, "edit-toggle");
      await p.waitSel(p.tid("exit-dialog"));
      await p.clickText("dialog .btn", "Discard");
      await waitView(p, "view");
      t.eq(await p.eval(`document.querySelectorAll('[data-testid="edit-grid"], [data-testid="edit-bar"]').length`), 0, "no hook of the editor after the exit");
      t.eq(await p.eval(`localStorage.getItem("hune.draft.v1.${uid}")`), null, "Discard removes the draft");
      await ready(p);
      await enterEdit(p);
      await setTitle2(p);
      await click(p, "edit-toggle");
      await p.waitSel(p.tid("exit-dialog"));
      await p.clickText("dialog .btn", "Save");
      await p.waitSel(p.tid("save-dialog"));
      await click(p, "save-confirm");
      await waitView(p, "view", "after Save");
      t.eq(await p.exists(p.tid("edit-bar")), false, "the edit bar is gone after Save");
    },
  },
  {
    id: "B76",
    name: "Back and Forward over the seven states: the state of each entry, one view, the draft kept with its notice, an open drawer closes (10.2a)",
    async run(t) {
      const uid = "st76";
      await seed(t, uid);
      const p = await openPage(t, t.dash(uid));
      await ready(p);
      // The walk by controls, with no leave: each step pushes an entry. V, PV, V (the way back), E, PE, JSON, settings, history.
      // The way back in the panel editor is a leave (a pop), so the walk reaches E from the view.
      const steps = [
        () => panelMenuItem(p, 2, "view"),
        () => p.eval(`document.querySelector('[data-testid="view-back"]').click()`),
        () => click(p, "edit-toggle"),
        () => panelMenuItem(p, 2, "edit"),
        () => moreItem(p, "json-tab"),
        () => moreItem(p, "dash-settings"),
        () => moreItem(p, "history-button"),
      ];
      const floor = (await navIndex(p))[0]; // the entry of the page at load: Back stops here
      const seen = [];
      const record = async () => seen.push({ view: await view(p), search: await search(p) });
      await record();
      for (const step of steps) {
        // Each step changes the state, so the walk waits for a new state before the next step (a fixed sleep was too short in a
        // full run under load: the next click then came before the editor opened).
        const before = seen[seen.length - 1].view;
        await step();
        await p.waitFor(`(() => { const v = document.documentElement.getAttribute("data-hune-view"); return v !== null && v !== ${JSON.stringify(before)}; })()`, { what: "the next state after " + before });
        await p.idle();
        await sleep(250);
        await record();
      }
      t.eq(new Set(seen.map((s) => s.view)).size, 7, "the walk visited the seven states: " + seen.map((s) => s.view).join(" "));
      // Each press of Back and Forward: the state of the URL, one view, no write of the history except the normal form.
      const check = async (what) => {
        await p.idle();
        await sleep(250);
        const want = await expectedPage(p);
        t.eq(await view(p), want, `${what}: data-hune-view is the state of the URL ${await search(p)}`);
        t.eq(normalSearch(await search(p), parseView(await search(p), CTX).state), await search(p), `${what}: the URL is in normal form`);
        await oneView(t, p, what);
        t.eq(await noDialog(p), true, `${what}: no dialog`);
      };
      await clearLog(p);
      let pressed = 0;
      while ((await navIndex(p))[0] > floor) {
        await p.eval("history.back()");
        await check(`back ${pressed++}`);
      }
      t.assert(pressed >= 6, `${pressed} entries in the stack`);
      while ((await navIndex(p)).reduce((a, b) => b - a - 1) > 0) {
        await p.eval("history.forward()");
        await check(`forward ${pressed--}`);
      }
      const writes = await hlog(p);
      t.assert(writes.length <= 2, "Back and Forward write the history only for the normal form: " + JSON.stringify(writes));
      // With changes: Back out of edit mode keeps the draft and shows the notice.
      const q = await openPage(t, t.dash(uid));
      await ready(q);
      await enterEdit(q);
      await panelMenuItem(q, 1, "edit");
      await waitView(q, "panel-edit");
      await setTitle(q, 1, "Walk title");
      await click(q, "editor-back");
      await waitView(q, "edit");
      await click(q, "save-button");
      await q.waitSel(p.tid("save-dialog"));
      await q.eval("history.back()");
      await waitView(q, "view");
      t.eq(await noDialog(q), true, "the drawer closes at Back");
      await q.waitFor(`[...document.querySelectorAll("#app-notices .notice")].some((n) => /You have changes that are not saved/.test(n.textContent) && /Resume/.test(n.textContent) && /Discard/.test(n.textContent))`, { what: "the notice of the draft" });
      t.assert((await q.eval(`localStorage.getItem("hune.draft.v1.${uid}")`)) !== null, "the draft stays");
      await q.clickText("#app-notices .notice .btn", "Resume");
      await waitView(q, "edit");
      await q.waitFor(`document.documentElement.getAttribute("data-hune-view") === "edit" && ![...document.querySelectorAll('[data-testid="save-button"]')].some((b) => b.disabled)`, { what: "the draft is in the editor" });
    },
  },
  {
    id: "B77",
    name: "reload of each state: the same state, URL and origin, no frame of another view, no empty main area, no move of the toolbar (10.2a)",
    async run(t) {
      const uid = "st77";
      await seed(t, uid);
      const urls = {
        view: "",
        "panel-view": "viewPanel=2",
        edit: "edit=1",
        "panel-edit": "edit=1&editPanel=2",
        json: "edit=1&editview=json",
        settings: "edit=1&editview=settings",
        history: "editview=history",
      };
      for (const [state, query] of Object.entries(urls)) {
        const p = await openPage(t, t.dash(uid, query), { frames: true });
        await waitView(p, state);
        let originBefore = null;
        if (state === "panel-edit") {
          // The origin of a state that a panel view opened survives the reload.
          const q = await openPage(t, t.dash(uid, "viewPanel=2"), { frames: true });
          await waitView(q, "panel-view");
          await click(q, "edit-toggle");
          await waitView(q, "panel-edit");
          originBefore = await origin(q);
          const url = await search(q);
          await q.reload();
          await q.idle();
          await waitView(q, "panel-edit", "after reload");
          t.eq(await origin(q), originBefore, "the origin stays after the reload");
          t.eq(await search(q), url, "the URL stays");
          const len = await histLen(q);
          await clearLog(q);
          await click(q, "editor-back");
          await waitView(q, "panel-view");
          t.eq(await histLen(q), len, "leave after the reload uses history.back()");
        }
        const url = await search(p);
        await p.reload();
        await p.idle();
        await waitView(p, state, "after reload");
        await sleep(1200);
        t.eq(await search(p), url, `${state}: the URL stays`);
        const frames = await p.eval("window.__frames");
        const shown = frames.filter((f) => !f.hold);
        t.assert(shown.length > 10, `${state}: frames were sampled`);
        const wrong = shown.filter((f) => f.view !== null && f.view !== state);
        t.assert(wrong.length === 0, `${state}: a frame shows another view: ${JSON.stringify(wrong.slice(0, 2))}`);
        const empty = shown.filter((f) => f.kids <= 1);
        t.assert(empty.length === 0, `${state}: a frame has an empty main area: ${JSON.stringify(empty.slice(0, 2))}`);
        const tops = shown.map((f) => f.tb).filter((v) => v !== null);
        t.assert(Math.max(...tops) - Math.min(...tops) <= 0.5, `${state}: the toolbar moved: ${[...new Set(tops)]}`);
        t.assert(shown.every((f) => f.view === state || f.view === null) && shown[0].view === state, `${state}: the first shown frame is the state: ${JSON.stringify(shown[0])}`);
      }
      // A draft comes back with its notice.
      const p = await openPage(t, t.dash(uid, "edit=1"));
      await waitView(p, "edit");
      await panelMenuItem(p, 1, "edit");
      await waitView(p, "panel-edit");
      await setTitle(p, 1, "Reload title");
      await p.waitFor(`localStorage.getItem("hune.draft.v1.${uid}") !== null`, { what: "draft stored" });
      await p.reload();
      await p.idle();
      await waitView(p, "panel-edit", "after reload with a draft");
      await p.waitFor(`[...document.querySelectorAll("#app-notices .notice")].some((n) => /Your draft is back/.test(n.textContent))`, { what: "the notice Your draft is back" });
    },
  },
  {
    id: "B78",
    name: "deep links: each vector of FE-26 gives its state, the normal form, one view and the notice of an unknown id (10.2a)",
    async run(t) {
      const uid = "st78";
      await seed(t, uid);
      const vectors = JSON.parse(readFileSync(join(ROOT, "tests/frontend/vectors/FE-26-view-state.json"), "utf8"));
      let n = 0;
      const page = await openPage(t, t.dash(uid));
      for (const v of vectors.urls) {
        if (!v.canEdit || v.state[3] !== null || v.state[4] !== null) continue; // the history versions have their own cases (B20)
        const query = v.search.replace(/^\?/, "");
        const p = page;
        await p.goto(t.dash(uid, query));
        await p.idle();
        await waitView(p, v.state[0], v.name);
        const state = parseView(await search(p), CTX).state;
        t.eq(state.page, v.state[0], `${v.name}: the URL names the state`);
        t.eq(normalSearch(await search(p), state), await search(p), `${v.name}: the URL is in normal form: ${await search(p)}`);
        await oneView(t, p, v.name);
        const noPanel = v.notices.filter((x) => x.code === "no-panel");
        if (noPanel.length) {
          await p.waitFor(`[...document.querySelectorAll("#app-notices .notice__text")].some((e) => /This dashboard has no panel/.test(e.textContent))`, { what: `${v.name}: the notice of an unknown id` });
        }
        n++;
        await sleep(150);
        await p.idle();
      }
      t.assert(n >= 20, `${n} vectors ran`);
      // editPanel and viewPanel: the panel editor and no panel view.
      const p = await openPage(t, t.dash(uid, "viewPanel=1&editPanel=2"));
      await waitView(p, "panel-edit");
      t.eq(await oneView(t, p, "editPanel and viewPanel"), { grid: false, panelView: false, editor: true }, "the editor and no panel view");
      t.eq(await names(p), { editPanel: "2" }, "the URL keeps the name of the panel editor and drops viewPanel");
    },
  },
  {
    id: "B79",
    name: "rights: edit names without the right give the view with a notice and no Resume, the draft stays, a history URL by hand gives the page of 403 (10.2a, 10.11, 10.14)",
    server: "noedit",
    async run(t) {
      const vectors = ["edit=1", "edit=1&editPanel=2", "edit=1&editview=json", "edit=1&editview=settings", "viewPanel=2&editPanel=2"];
      const uid = "nodes";
      // A draft of the user stays in the storage of the page.
      const first = await openPage(t, t.dash(uid));
      await waitPanelsReady(first, [1, 2, 3]);
      const dash = await (await fetch(t.url(`api/dashboards/${uid}`))).json();
      const sess = await (await fetch(t.url("api/session"))).json();
      const record = { base_etag: dash.etag || "x", base_version: dash.version || 0, subject: sess.subject, folder: dash.folder || "transit", updated_at_ms: Date.now(), document: { ...dash.document, title: "Draft title" }, json_text: null, save_id: null, message: "" };
      await first.eval(`localStorage.setItem("hune.draft.v1.${uid}", ${JSON.stringify(JSON.stringify(record))})`);
      for (const query of vectors) {
        await first.goto(t.dash(uid, query));
        await first.idle();
        await first.waitFor(`["view", "panel-view"].includes(document.documentElement.getAttribute("data-hune-view"))`, { what: `a view state for ${query}` });
        t.eq(await first.eval(`/edit|editPanel|editview/.test(location.search)`), false, `${query}: no edit name in the URL: ${await search(first)}`);
        await first.waitFor(`[...document.querySelectorAll("#app-notices .notice__text")].some((e) => /You cannot edit this dashboard/.test(e.textContent))`, { what: `${query}: the notice of rights` });
        t.eq(await first.eval(`[...document.querySelectorAll("#app-notices .notice .btn")].some((b) => /Resume/.test(b.textContent))`), false, `${query}: no Resume`);
        t.assert((await first.eval(`localStorage.getItem("hune.draft.v1.${uid}")`)) !== null, `${query}: the draft stays`);
      }
      // The history of a dashboard of a read-only store does not exist: a URL by hand shows the page of 403 (10.11).
      await first.goto(t.dash(uid, "editview=history"));
      await first.waitSel(first.tid("dashboard-forbidden"));
      t.eq(await first.exists(first.tid("history-list")), false, "no history list");
    },
  },
  {
    id: "B87",
    name: "leave warnings: the brand and the selector ask in each edit state, Back and Forward never ask, Resume from the view and the panel view, Discard of the notice (10.2, 10.8, 10.2a)",
    async run(t) {
      const uid = "st87";
      await seed(t, uid);
      const states = ["edit", "panel-edit", "json", "settings"];
      for (const name of states) {
        const p = await openPage(t, t.dash(uid, "edit=1"));
        await waitView(p, "edit");
        await setTitle2(p);
        if (name === "panel-edit") {
          await panelMenuItem(p, 1, "edit");
          await waitView(p, "panel-edit");
        }
        if (name === "json") await moreItem(p, "json-tab");
        if (name === "settings") await moreItem(p, "dash-settings");
        await waitView(p, name);
        // beforeunload asks.
        t.eq(await p.eval(`(() => { const e = new Event("beforeunload", { cancelable: true }); window.dispatchEvent(e); return e.defaultPrevented; })()`), true, `${name}: beforeunload asks`);
        await p.click("a.brand");
        await p.waitSel(p.tid("exit-dialog"));
        t.eq(await view(p), name, `${name}: the brand link keeps the state until the answer`);
        await p.clickText("dialog .btn", "Stay");
        await p.waitGone("dialog[open]");
        // Another dashboard of the selector asks in the same way.
        await p.click(p.tid("header-switcher"));
        await p.waitSel('[data-testid="header-switcher-menu"] a.menu__item:not(.is-current)');
        await p.eval(`document.querySelector('[data-testid="header-switcher-menu"] a.menu__item:not(.is-current)').click()`);
        await p.waitSel(p.tid("exit-dialog"));
        await p.clickText("dialog .btn", "Stay");
        await p.waitGone("dialog[open]");
        // Back and Forward show no dialog.
        await p.eval("history.back()");
        await sleep(400);
        t.eq(await noDialog(p), true, `${name}: Back shows no dialog`);
        await p.eval("history.forward()");
        await sleep(400);
        t.eq(await noDialog(p), true, `${name}: Forward shows no dialog`);
        await p.close().catch(() => {});
      }
      // Resume from the view and from the panel view: the state edit and no panel view.
      const p = await openPage(t, t.dash(uid, "edit=1"));
      await waitView(p, "edit");
      await panelMenuItem(p, 1, "edit");
      await waitView(p, "panel-edit");
      await setTitle(p, 1, "Resume title");
      await p.waitFor(`localStorage.getItem("hune.draft.v1.${uid}") !== null`, { what: "draft stored" });
      for (const query of ["", "viewPanel=2"]) {
        await p.goto(t.dash(uid, query));
        await p.idle();
        await waitView(p, query ? "panel-view" : "view");
        await p.waitFor(`[...document.querySelectorAll("#app-notices .notice .btn")].some((b) => /Resume/.test(b.textContent))`, { what: "the notice with Resume" });
        await p.clickText("#app-notices .notice .btn", "Resume");
        await waitView(p, "edit");
        t.eq(await oneView(t, p, "after Resume"), { grid: false, panelView: false, editor: true }, "Resume shows the editor alone");
      }
      // Discard of the notice removes the draft.
      await p.goto(t.dash(uid));
      await p.idle();
      await p.waitFor(`[...document.querySelectorAll("#app-notices .notice .btn")].some((b) => /Discard/.test(b.textContent))`, { what: "the notice" });
      await p.clickText("#app-notices .notice .btn", "Discard");
      t.eq(await p.eval(`localStorage.getItem("hune.draft.v1.${uid}")`), null, "the draft is removed");
    },
  },
  {
    id: "B88",
    name: "entry redirect and states: replaceState, Back leaves the page, a state URL gives no redirect, the brand shows the list (9.7, 10.2a, rule 13)",
    async run(t) {
      const uid = "st88";
      const own = await t.ownServer("states-one", {
        prepare: (volume) => {
          for (const name of readdirSync(volume)) rmSync(join(volume, name), { recursive: true, force: true });
          mkdirSync(join(volume, "transit"), { recursive: true });
          writeFileSync(join(volume, "transit", "_folder.json"), JSON.stringify({ format_version: 1, title: "Transit desk" }) + "\n");
          writeFileSync(join(volume, "transit", `${uid}.json`), JSON.stringify(statesDoc(uid, "Only one"), null, 2) + "\n");
        },
      });
      const p = await t.page({ width: 1400, height: 900 });
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: HLOG });
      await p.goto(own.url);
      await p.idle();
      await waitView(p, "view");
      t.assert((await hlog(p)).includes("replaceState") && !(await hlog(p)).includes("pushState"), "the redirect uses replaceState: " + JSON.stringify(await hlog(p)));
      await p.waitFor(`!!document.querySelector('[data-testid="panel-1"]')`, { what: "grid" });
      await enterEdit(p);
      await waitView(p, "edit");
      await p.eval("history.back()");
      await waitView(p, "view");
      await p.eval("history.back()").catch(() => {});
      await sleep(800);
      t.eq(await p.eval("location.href"), "about:blank", "Back leaves the page");
      // A state URL with q gives no redirect.
      const q = await openPage(t, `${own.url}d/${uid}?viewPanel=2&q=x`);
      await waitView(q, "panel-view");
      const r = await openPage(t, `${own.url}d/${uid}?edit=1&q=x`);
      await waitView(r, "edit");
      // The brand shows the list with its row.
      await r.eval(`document.querySelector("a.brand").click()`);
      await r.waitFor(`document.documentElement.getAttribute("data-hune-view") === "list"`, { what: "the list" });
      await r.waitSel(`[data-testid="row-${uid}"]`);
    },
  },
  {
    id: "B89",
    name: "a walk of 80 events with a fixed seed: after each event the state is the state of the URL, in normal form, one view, no dialog after Back or Forward (10.2a)",
    async run(t) {
      const uid = "st89";
      await seed(t, uid);
      const p = await openPage(t, t.dash(uid));
      await ready(p);
      const floor = (await navIndex(p))[0];
      let x = 20261112;
      const rnd = (n) => {
        x = (Math.imul(x, 1664525) + 1013904223) >>> 0;
        return x % n;
      };
      const log = [];
      const has = (id) => p.eval(`(() => { const e = document.querySelector('[data-testid="${id}"]'); return !!e && e.getClientRects().length > 0; })()`);
      const events = {
        "view-panel": async () => (await view(p)) === "view" && (await has("panel-2-menu")) && (await panelMenuItem(p, 2, "view"), true),
        "close-view": async () => (await view(p)) === "panel-view" && (await p.press("Escape"), true),
        "edit-on": async () => (await has("edit-toggle")) && (await p.attr(p.tid("edit-toggle"), "aria-checked")) === "false" && (await click(p, "edit-toggle"), true),
        "edit-off": async () => {
          if (!(await has("edit-toggle")) || (await p.attr(p.tid("edit-toggle"), "aria-checked")) !== "true") return false;
          await click(p, "edit-toggle");
          await sleep(300);
          if (!(await noDialog(p))) await p.clickText("dialog .btn", "Discard");
          return true;
        },
        "panel-edit": async () => (["edit", "view"].includes(await view(p))) && (await has("panel-1-menu")) && (await panelMenuItem(p, 1, "edit"), true),
        "open-json": async () => (await view(p)) === "edit" && (await moreItem(p, "json-tab"), true),
        "open-settings": async () => (await view(p)) === "edit" && (await moreItem(p, "dash-settings"), true),
        "open-history": async () => (await has("dashboard-menu")) && (await view(p)) !== "history" && (await moreItem(p, "history-button"), true),
        leave: async () => (await has("editor-back")) && (await click(p, "editor-back"), true),
        "history-back": async () => (await has("history-back")) && (await click(p, "history-back"), true),
        back: async () => (await navIndex(p))[0] > floor && (await p.eval("history.back()"), true),
        forward: async () => (await navIndex(p)).reduce((a, b) => b - a - 1) > 0 && (await p.eval("history.forward()"), true),
        reload: async () => (await p.reload(), true),
        change: async () => {
          if (!(await has("time-picker-button"))) return false;
          await click(p, "time-picker-button");
          await p.waitSel(".picker__popup");
          await p.clickText(".picker__quick-item", rnd(2) ? "Last 24 hours" : "Last 6 hours");
          await p.waitGone(".picker__popup");
          return true;
        },
      };
      const names2 = Object.keys(events);
      try {
        for (let i = 0; i < 80; i++) {
          let ran = false;
          for (let attempt = 0; attempt < 12 && !ran; attempt++) {
            const name = names2[rnd(names2.length)];
            if (await events[name]()) {
              log.push(name);
              ran = true;
              if (name === "back" || name === "forward") {
                await p.idle();
                await sleep(250);
                t.eq(await noDialog(p), true, `no dialog after ${name}`);
              }
            }
          }
          await p.idle();
          await sleep(120);
          await p.waitFor(`document.documentElement.getAttribute("data-hune-view") !== null`, { what: "a state" });
          const want = await expectedPage(p);
          await p.waitFor(`document.documentElement.getAttribute("data-hune-view") === ${JSON.stringify(want)}`, { what: `state ${want}`, timeout: 6000 });
          t.eq(normalSearch(await search(p), parseView(await search(p), CTX).state), await search(p), "the URL is in normal form");
          await oneView(t, p, `event ${i} (${log[log.length - 1]})`);
        }
      } catch (e) {
        throw new Error(`${e.message} [seed 20261112; events: ${log.join(" ")}]`);
      }
    },
  },
];

async function setTitle2(p) {
  // A change of the draft from the grid: the title of the first panel, by the panel editor.
  await panelMenuItem(p, 1, "edit");
  await waitView(p, "panel-edit");
  await setTitle(p, 1, `Title ${Date.now() % 10000}`);
  await click(p, "editor-back");
  await waitView(p, "edit");
}
