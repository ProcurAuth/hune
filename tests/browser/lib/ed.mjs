// SPDX-License-Identifier: MIT OR Apache-2.0
// Helpers of the editor cases (section 10).
import { seedDocument } from "./env.mjs";
import { sleep } from "./h.mjs";

/** Create a dashboard from the seed document under a new uid (API), return `{ etag, doc }`. */
export async function makeSeed(t, uid, mutate = null, folder = "transit") {
  const doc = seedDocument(uid, `Case ${uid}`);
  if (mutate) mutate(doc);
  const r = await t.api.create(folder, doc);
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 300)}`);
  return { etag: r.etag, doc, version: r.json.version };
}

/** A CSS selector for the control of the label with this exact text, inside `root`. */
export async function byLabel(page, label, root = '[data-testid="panel-editor"]') {
  const id = await page.eval(`(() => {
    for (const l of document.querySelectorAll(${JSON.stringify(root)} + " label")) {
      if (l.textContent.trim() === ${JSON.stringify(label)} && l.htmlFor) return l.htmlFor;
    }
    return null;
  })()`);
  if (!id) throw new Error(`no label ${label}`);
  return `[id="${id}"]`;
}

/** Open edit mode on a dashboard page that is loaded. */
export async function enterEdit(page) {
  await page.click(page.tid("edit-toggle"));
  await page.waitSel(page.tid("edit-bar"));
  // The draft grid replaces the base grid: wait for its panel frames before a menu click.
  await page.waitSel('.edit-grid [data-testid^="panel-"][data-state]');
  await page.idle();
}

/** Click an item of the menu of the button "Add" of the header (edit mode): add-panel, add-section or add-variable (10.2). */
export async function addItem(page, id) {
  await page.click(page.tid("add-menu"));
  const item = `.menu ${page.tid(id)}`;
  await page.waitSel(item);
  await page.click(item);
}

/** Click an item of the menu "More actions" of the header: History, Export, Delete, the JSON and the settings have no button of their own (9.9, 10.2).
 *  "Add panel" and "Add section" are items of the menu of "Add" now, so those two ids go there. */
export async function moreItem(page, id) {
  if (id === "add-panel" || id === "add-section") return addItem(page, id);
  await page.click(page.tid("dashboard-menu"));
  const item = `.menu ${page.tid(id)}`;
  await page.waitSel(item);
  await page.click(item);
}

/** Turn the switch "Edit" off (it was on). With changes, the dialog "Exit edit mode" shows. */
export async function exitEdit(page) {
  await page.click(page.tid("edit-toggle"));
}

export async function openPanelEditor(page, id) {
  await page.click(page.tid(`panel-${id}-menu`));
  await page.click(page.tid(`panel-${id}-edit`));
  await page.waitSel(page.tid("panel-editor"));
  await page.idle();
}

export async function backToDashboard(page) {
  await page.click(page.tid("editor-back"));
  await page.waitGone(page.tid("panel-editor"));
  await page.idle();
}

/** Open a panel editor, set the panel title on the tab "Panel", go back. */
export async function setPanelTitle(page, id, title) {
  await openPanelEditor(page, id);
  await page.click(page.tid("tab-panel"));
  const sel = await byLabel(page, "Title");
  await page.fill(sel, title);
  await sleep(450); // 300 ms debounce
  await backToDashboard(page);
}

/** The draft record of a dashboard from localStorage, or null. */
export async function draftOf(page, uid) {
  const raw = await page.eval(`localStorage.getItem("hune.draft.v1.${uid}")`);
  return raw ? JSON.parse(raw) : null;
}

export async function waitDraft(page, uid, pred = () => true) {
  const end = Date.now() + 5000;
  for (;;) {
    const d = await draftOf(page, uid);
    if (d && pred(d)) return d;
    if (Date.now() > end) throw new Error("no draft in localStorage for " + uid);
    await sleep(100);
  }
}

/** Click "Save", fill the message, confirm. Returns without waiting for the answer. */
export async function saveDialog(page, message = "") {
  await page.click(page.tid("save-button"));
  await page.waitSel(page.tid("save-dialog"));
  if (message) await page.fill(page.tid("save-message"), message);
  await page.click(page.tid("save-confirm"));
}

export async function noticeTexts(page) {
  return page.eval(`[...document.querySelectorAll('[data-testid="notice"] .notice__text')].map((e) => e.textContent)`);
}

export async function waitNotice(page, re, timeout = 10000) {
  const src = re.source;
  const flags = re.flags;
  await page.waitFor(`[...document.querySelectorAll('[data-testid="notice"] .notice__text')].some((e) => new RegExp(${JSON.stringify(src)}, ${JSON.stringify(flags)}).test(e.textContent))`, { timeout, what: `notice ${re}` });
}

/** Set the files of an input element (CDP). */
export async function setFiles(page, selector, files) {
  await page.waitSel(selector);
  const { root } = await page.send("DOM.getDocument", { depth: 0 });
  const { nodeId } = await page.send("DOM.querySelector", { nodeId: root.nodeId, selector });
  if (!nodeId) throw new Error("no input " + selector);
  await page.send("DOM.setFileInputFiles", { nodeId, files });
}
