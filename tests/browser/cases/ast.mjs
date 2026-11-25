// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B170 to B177 (16.10): the minimal syntax tree of the query editor (F42, 10.3c to 10.3g).
// B170 the folds and their keys. B171 the folded view and the edit (a key, a paste, an IME composition, a drag, the clipboard).
// B172 the format (the button, the key, the write path, the own undo, the refusals). B173 the band of the current block and the
// bracket pair (no glyph moves, contrast). B174 the caps and the hostile texts. B175 the access (scripted checks: axe-core is not
// available offline). B176 the time and the budget. B177 the kinds of source and the other areas (variable, annotation).
//
// The tree is never a security check: no case here gives the tree a power over the save or the run. All data is synthetic.
// Chrome is the only engine of this suite. The cases B170, B171 and B172 also have a manual check in Firefox and in WebKit
// (16.10): the folded view and the write of the format depend on the engine of the text area.
import { chmodSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { doc, panel, section } from "../lib/docs.mjs";
import { WORK, api, mongoBlockText, probeMongoDriver, seedDocument } from "../lib/env.mjs";
import { backToDashboard, byLabel, draftOf, enterEdit, makeSeed, moreItem, openPanelEditor, saveDialog, waitDraft } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const SQL = '[data-testid="sql-input"]';
const tid = (id) => `[data-testid="${id}"]`;
const ROW = 20;

// ---------------------------------------------------------------------------------------------------------------------
// The keys. `Page.press` of the suite sets `code` from the letter, so the keys with a bracket and a bar need their own call.

const MOD = { alt: 1, ctrl: 2, meta: 4, shift: 8 };
const VK = { Enter: 13, Backspace: 8, Delete: 46, Tab: 9, Escape: 27, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, Home: 36, End: 35, PageUp: 33, PageDown: 34, BracketLeft: 219, BracketRight: 221, Backslash: 220 };

/**
 * One key press through the DevTools protocol, with a physical `code`, the modifiers and, where Chrome needs it, the editing
 * commands of the keyboard shortcut (headless Chrome does not map Ctrl+Z, Ctrl+C or Ctrl+V to a command by itself).
 * `mods` is a list of "alt", "ctrl", "meta" and "shift". `repeat` sets `event.repeat`.
 */
export async function press(p, code, { key = null, mods = [], commands, text, repeat = false } = {}) {
  const modifiers = mods.reduce((a, m) => a | MOD[m], 0);
  const letter = /^Key([A-Z])$/.exec(code);
  const k = key ?? (letter ? (mods.includes("shift") ? letter[1] : letter[1].toLowerCase()) : code);
  const vk = VK[code] ?? (letter ? letter[1].charCodeAt(0) : 0);
  const base = { key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers, autoRepeat: repeat };
  const typed = text ?? (k.length === 1 && !mods.includes("ctrl") && !mods.includes("meta") && !mods.includes("alt") ? k : undefined);
  await p.send("Input.dispatchKeyEvent", { type: typed ? "keyDown" : "rawKeyDown", ...base, text: typed, ...(commands ? { commands } : {}) });
  await p.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
}

const fold = (p, mods = ["ctrl", "shift"]) => press(p, "BracketLeft", { key: "{", mods });
const unfoldAllKey = (p) => press(p, "BracketRight", { key: "}", mods: ["ctrl", "shift"] });
const partnerKey = (p) => press(p, "Backslash", { key: "|", mods: ["ctrl", "shift"] });
const formatKey = (p, mods = ["ctrl", "shift"], extra = {}) => press(p, "KeyF", { key: "F", mods, ...extra });
const undoKey = (p, mods = ["ctrl"]) => press(p, "KeyZ", { mods, commands: mods.includes("shift") ? ["redo"] : ["undo"] });
const redoKey = (p) => press(p, "KeyZ", { mods: ["ctrl", "shift"], commands: ["redo"] });

// ---------------------------------------------------------------------------------------------------------------------
// The page.

/** Open the panel editor of panel 2 of a new dashboard that is a copy of the seed. Returns the page. */
async function openEditor(t, uid, { theme = null, width = 1400, height = 1000, server = null, source = null, sql = null } = {}) {
  let url = t.dash(uid);
  if (server) {
    // A dashboard of another server (it has the sources of PostgreSQL and MongoDB): the seed with the source and the text of panel 2.
    const document = seedDocument(uid, `Case ${uid}`);
    const q = document.sections.flatMap((x) => x.panels).find((x) => x.id === 2).data.query;
    if (source) q.source = source;
    if (sql) q.sql = sql;
    const r = await api(server).create("transit", document);
    if (r.status !== 201) throw new Error(`create ${uid}: ${r.status} ${r.text.slice(0, 300)}`);
    url = `${server.url}d/${uid}?${t.range}`;
  } else await makeSeed(t, uid);
  const p = await t.page({ width, height });
  if (theme) await p.emulateColorScheme(theme);
  t.allow(p, /Failed to load resource/);
  await p.goto(url);
  await p.idle();
  // Panel 2 of a dashboard with the source of another kind has no data (the source is down): only panel 1 is ready (panel 3 reuses panel 2).
  await waitPanelsReady(p, source && source !== "prod" ? [1] : [1, 2, 3]);
  await enterEdit(p);
  await openPanelEditor(p, 2);
  await p.waitSel(tid("sql-input"));
  return p;
}

/** The uid of the dashboard that a page shows (the path of the page is `/d/<uid>`). */
const uidOf = (p) => p.eval(`decodeURIComponent(location.pathname.split("/d/")[1].split("/")[0])`);

/** Put a text in the area with one real insert (one `beforeinput` and one `input`), and wait for the tree. */
async function setText(p, text) {
  await p.fill(SQL, text);
  await settle(p);
}

/** Wait for the parse (150 ms after the last input, then an idle call) and for the debounce of the draft (300 ms). */
async function settle(p, ms = 700) {
  await sleep(ms);
}

/** The state of the SQL area that the cases read. */
async function state(p) {
  return p.eval(`(() => {
    const i = document.querySelector('[data-testid="sql-input"]');
    const q = (s) => document.querySelector('[data-testid="' + s + '"]');
    const band = q("sql-block");
    const folded = q("sql-folded");
    return {
      value: i.value, a: i.selectionStart, b: i.selectionEnd, top: i.scrollTop, left: i.scrollLeft,
      status: q("sql-status").textContent, info: q("sql-info").textContent,
      markers: [...document.querySelectorAll('[data-testid="sql-fold"]')].map((b) => ({ name: b.getAttribute("aria-label"), expanded: b.getAttribute("aria-expanded"), from: Number(b.dataset.from), tab: b.getAttribute("tabindex"), top: b.getBoundingClientRect().top })),
      folded: folded.style.visibility !== "hidden",
      chips: [...document.querySelectorAll('[data-testid="sql-chip"]')].map((c) => ({ text: c.textContent, from: Number(c.dataset.from) })),
      numbers: [...document.querySelectorAll(".sql__no")].map((e) => e.textContent),
      band: band.style.visibility === "hidden" ? null : { type: band.dataset.type, top: band.getBoundingClientRect().top, height: band.getBoundingClientRect().height, left: band.getBoundingClientRect().left, width: band.getBoundingClientRect().width },
      pairs: [...document.querySelectorAll('[data-testid="sql-pair"]')].map((e) => ({ text: e.textContent, lone: e.classList.contains("sql__pair--lone") })),
      active: document.activeElement === i ? "sql-input" : (document.activeElement.dataset || {}).testid || document.activeElement.tagName,
      unfoldAll: q("sql-unfold-all").style.visibility !== "hidden",
      undoButton: !q("sql-undo-format").hidden,
    };
  })()`);
}

/** Wait until the tree has drawn the markers (or the cap notice) for the text now. The count is the attribute `data-count` of the group of markers: a big text draws only the markers near the visible rows. */
async function waitTree(p, { markers = null, status = null, timeout = 8000 } = {}) {
  await p.waitFor(
    `(() => { const n = Number(document.querySelector('.sql__marks').dataset.count); const s = document.querySelector('[data-testid="sql-status"]').textContent;
      return ${markers === null ? "true" : `n === ${markers}`} && ${status === null ? "true" : `s === ${JSON.stringify(status)}`}; })()`,
    { timeout, what: `tree with ${markers} markers and status ${status}` },
  );
}

/** The 0-based line of an offset, and the brackets of a text that has no string and no comment with a bracket. */
function lineOf(text, offset) {
  let n = 0;
  for (let i = 0; i < offset; i++) if (text[i] === "\n") n++;
  return n;
}

/** The pairs of brackets of a text (own scan, no tree): `{ open, close, line, endLine }` for each pair, by the open offset. */
export function bracketPairs(text) {
  const stack = [];
  const out = [];
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === "(" || c === "[" || c === "{") stack.push(i);
    else if (c === ")" || c === "]" || c === "}") {
      const open = stack.pop();
      if (open !== undefined) out.push({ open, close: i, line: lineOf(text, open), endLine: lineOf(text, i) });
    }
  }
  return out.sort((x, y) => x.open - y.open);
}

/** The folds that the spec expects for a text (10.3e): a pair of brackets that spans two lines or more. */
export function expectedFolds(text) {
  return bracketPairs(text).filter((x) => x.endLine > x.line);
}

/** The centre of an element of the page, after a scroll into view of the text area (the area scrolls itself). */
async function rect(p, selector) {
  return p.eval(`(() => { const e = document.querySelector(${JSON.stringify(selector)}); const b = e.getBoundingClientRect(); return { x: b.left, y: b.top, w: b.width, h: b.height, cx: b.left + b.width / 2, cy: b.top + b.height / 2 }; })()`);
}

async function mouse(p, type, x, y, extra = {}) {
  await p.send("Input.dispatchMouseEvent", { type, x, y, button: type === "mouseMoved" ? "none" : "left", clickCount: 1, ...extra });
}

/** The rectangle of the gutter number of a real line (the row that shows it), or null when the line is hidden. */
async function rowOfLine(p, line1) {
  return p.eval(`(() => { for (const e of document.querySelectorAll(".sql__no")) if (e.textContent === ${JSON.stringify(String(line1))}) { const b = e.getBoundingClientRect(); return { top: b.top, h: b.height, cy: b.top + b.height / 2 }; } return null; })()`);
}

/** A click in the folded view, beyond the end of the text of a row: the caret goes to the end of that line. */
async function clickEndOfRow(p, line1) {
  const r = await rowOfLine(p, line1);
  if (!r) throw new Error(`no row for line ${line1}`);
  const b = await rect(p, ".sql__body");
  await mouse(p, "mouseMoved", b.x + b.w - 60, r.cy);
  await mouse(p, "mousePressed", b.x + b.w - 60, r.cy);
  await mouse(p, "mouseReleased", b.x + b.w - 60, r.cy);
}

// ---------------------------------------------------------------------------------------------------------------------
// The texts.

/** A query with a WITH, two subqueries and a call on several lines. It has no bracket in a string or a comment. */
export const FOLD_TEXT = [
  "WITH recent AS (",
  "  SELECT host, avg(cpu) AS cpu",
  "  FROM metrics.cpu",
  "  WHERE $__timeFilter(ts)",
  "  GROUP BY host",
  ")",
  "SELECT r.host, cpu",
  "FROM (",
  "  SELECT host, cpu",
  "  FROM recent",
  ") AS r",
  "WHERE r.cpu > (",
  "  SELECT avg(cpu)",
  "  FROM recent",
  ")",
  "  AND arrayMap(",
  "    x -> x + 1,",
  "    [1, 2, 3]",
  "  ) != []",
].join("\n");

// ---------------------------------------------------------------------------------------------------------------------
// B170

async function runB170(t) {
  const p = await openEditor(t, "ed170");
  await setText(p, FOLD_TEXT);
  const want = expectedFolds(FOLD_TEXT);
  t.eq(want.map((f) => [f.line + 1, f.endLine + 1]), [[1, 6], [8, 11], [12, 15], [16, 19]], "the texts folds, from the own bracket scan");
  await waitTree(p, { markers: want.length });

  // The markers: one for each fold, in the order of the text, with a name and `aria-expanded`, out of the Tab order.
  let s = await state(p);
  t.eq(s.markers.map((m) => m.name), want.map((f) => `Fold lines ${f.line + 1} to ${f.endLine + 1}`), "names of the markers");
  t.eq(s.markers.map((m) => [m.expanded, m.tab]), want.map(() => ["true", "-1"]), "aria-expanded and tabindex of the markers");
  t.eq(s.markers.map((m) => m.from), want.map((f) => f.open + 1), "data-from is the first folded character");
  const marker = await p.eval(`(() => { const b = document.querySelector('[data-testid="sql-fold"]'); const r = b.getBoundingClientRect(); const g = document.querySelector(".sql__gutter").getBoundingClientRect(); return { w: r.width, h: r.height, gutter: g.width, tag: b.tagName, type: b.type }; })()`);
  t.eq([marker.w, marker.h, marker.tag, marker.type], [24, 20, "BUTTON", "button"], "a marker is a 24 by 20 px button");
  t.assert(marker.gutter >= 80, `the gutter is 80 px or more: ${marker.gutter}`);
  t.eq(s.numbers.length, 19, "19 gutter numbers before a fold");
  const rows = await p.eval(`[...document.querySelectorAll(".sql__no")].map((e) => e.getBoundingClientRect().height)`);
  t.assert(rows.every((h) => h === ROW), `each row is ${ROW} px high: ${rows}`);

  // A click on a marker folds. The text area keeps its value, its selection, its scroll position and the focus.
  const lenEnd = FOLD_TEXT.length;
  await p.focus(SQL);
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.setSelectionRange(${lenEnd}, ${lenEnd}); i.scrollTop = 8; })()`);
  await sleep(100);
  const before = await state(p);
  await p.click(`${tid("sql-fold")}[data-from="${want[0].open + 1}"]`);
  s = await state(p);
  t.assert(s.folded, "the folded view shows after a click on a marker");
  t.eq([s.value === before.value, s.a, s.b, s.top, s.active], [true, before.a, before.b, before.top, "sql-input"], "value, selection, scrollTop and focus after a fold by a click");
  t.eq(s.status, "Folded 1 block, 6 lines.", "status of a fold");
  t.eq(s.chips.map((c) => c.text), ["… 6 lines"], "the chip");
  t.eq(s.markers[0].name, "Unfold lines 1 to 6", "the marker names the unfold");
  t.eq(s.markers[0].expanded, "false", "aria-expanded of a folded marker");
  t.assert(s.unfoldAll, "the button Unfold all shows while a fold exists");
  // The gutter shows the real line numbers: the hidden lines 2 to 5 jump, line 6 joins the row of line 1.
  t.eq(s.numbers, ["1", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19"], "the real line numbers jump over the hidden lines");
  // The rows are 20 px high and the chip adds no height.
  const geometry = await p.eval(`(() => {
    const f = document.querySelector('[data-testid="sql-folded"]');
    const rows = [...document.querySelectorAll(".sql__no")].map((e) => e.getBoundingClientRect().height);
    const chip = document.querySelector('[data-testid="sql-chip"]');
    const cs = getComputedStyle(f);
    const padTop = parseFloat(cs.paddingTop), padBottom = parseFloat(cs.paddingBottom);
    return { rows, content: f.scrollHeight - padTop - padBottom, chipHeight: chip.getBoundingClientRect().height, lineHeight: parseFloat(cs.lineHeight), chipRows: chip.getClientRects().length };
  })()`);
  t.assert(geometry.rows.every((h) => h === ROW), "each gutter row is 20 px in the folded view");
  t.eq(geometry.content, 14 * ROW, "the folded view has 14 rows of 20 px, so the chip adds no height");
  t.eq([geometry.lineHeight, geometry.chipRows], [ROW, 1], "line height 20 px, the chip is on one row");
  t.assert(geometry.chipHeight <= ROW, `the chip is not higher than a row: ${geometry.chipHeight}`);

  // A second click unfolds. The last fold opens: the folded view ends.
  await p.click(`${tid("sql-fold")}[data-from="${want[0].open + 1}"]`);
  s = await state(p);
  t.eq([s.folded, s.status, s.chips.length, s.unfoldAll], [false, "Unfolded 1 block.", 0, false], "a click on the marker unfolds");
  t.eq(s.numbers.length, 19, "the gutter has 19 rows again");
  t.eq([s.value === before.value, s.a, s.b, s.top], [true, before.a, before.b, before.top], "value, selection, scrollTop after the unfold");

  // The key Ctrl+Shift+[ folds the innermost node at the caret, and again it unfolds. The text keeps its value.
  const inCte = FOLD_TEXT.indexOf("FROM metrics.cpu") + 5;
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.focus(); i.setSelectionRange(${inCte}, ${inCte}); i.scrollTop = 0; })()`);
  await sleep(100);
  const reach = await listenKeys(p);
  await fold(p);
  s = await state(p);
  t.assert(s.folded, "Ctrl+Shift+[ folds the block at the caret");
  t.eq(s.status, "Folded 1 block, 6 lines.", "status of a fold by the key");
  t.eq([s.value === FOLD_TEXT, s.a, s.b], [true, want[0].open + 1, want[0].open + 1], "the caret goes after the open bracket of the folded range");
  t.eq(s.active, "sql-input", "the focus stays in the text area");
  await fold(p);
  s = await state(p);
  t.eq([s.folded, s.status], [false, "Unfolded 1 block."], "the key unfolds the same node");

  // The innermost node: a caret in the call folds the call, not the clause around it.
  const inCall = FOLD_TEXT.indexOf("[1, 2, 3]") + 2;
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.setSelectionRange(${inCall}, ${inCall}); })()`);
  await fold(p);
  s = await state(p);
  t.eq([s.folded, s.status], [true, "Folded 1 block, 4 lines."], "the call folds (4 lines, the innermost node)");
  // Fold a second block, then Ctrl+Shift+] unfolds all.
  const inSub = FOLD_TEXT.indexOf("FROM recent\n)") + 3;
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.setSelectionRange(${inSub}, ${inSub}); })()`);
  await fold(p);
  s = await state(p);
  t.eq(s.chips.length, 2, "two folds");
  await unfoldAllKey(p);
  s = await state(p);
  t.eq([s.folded, s.status, s.chips.length, s.unfoldAll], [false, "Unfolded all.", 0, false], "Ctrl+Shift+] unfolds all");
  await unfoldAllKey(p);
  t.eq((await state(p)).status, "Nothing is folded.", "Ctrl+Shift+] with no fold");
  // A caret outside any pair of brackets: nothing to fold.
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.setSelectionRange(${FOLD_TEXT.indexOf("SELECT r.host") + 2}, ${FOLD_TEXT.indexOf("SELECT r.host") + 2}); })()`);
  await fold(p);
  t.eq((await state(p)).status, "Nothing to fold here.", "Ctrl+Shift+[ outside a block");

  // The button "Unfold all".
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.setSelectionRange(${inCte}, ${inCte}); })()`);
  await fold(p);
  await p.click(tid("sql-unfold-all"));
  s = await state(p);
  t.eq([s.folded, s.status, s.active], [false, "Unfolded all.", "sql-input"], "the button Unfold all");

  // The keys reach the page: each one gives a keydown on the document, and `preventDefault` stops the default of the browser.
  const keys = await p.eval(`window.__keys.length`);
  t.assert(keys >= 5, `the page saw the keys: ${keys}`);
  const seen = await p.eval(`window.__keys`);
  for (const k of seen) t.assert(k.prevented, `preventDefault for ${k.code} with ${k.mods}`);
  void reach;
  // The keys that are not for the editor keep their default: Alt, or Cmd for the fold keys.
  await p.eval(`window.__keys.length = 0`);
  await press(p, "BracketLeft", { key: "{", mods: ["ctrl", "shift", "alt"] });
  await press(p, "BracketLeft", { key: "{", mods: ["meta", "shift"] });
  await press(p, "KeyF", { key: "F", mods: ["ctrl", "shift", "alt"] });
  const other = await p.eval(`window.__keys`);
  t.eq(other.map((k) => k.prevented), [false, false, false], "Alt keys and Cmd+Shift+[ are not taken");
  t.eq((await state(p)).folded, false, "no fold by a key that is not ours");
  // A layout that gives another character: the keys follow `event.code`.
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.setSelectionRange(${inCte}, ${inCte}); })()`);
  await press(p, "BracketLeft", { key: "х", mods: ["ctrl", "shift"] });
  t.assert((await state(p)).folded, "the fold key works on a layout where the key is another character (by event.code)");
  await unfoldAllKey(p);

  // An error on a hidden line shows on the row that hides it. The server refuses the macro on line 3 of the text below (7.10).
  const bad = ["SELECT 1 FROM (", "  SELECT 2", "  , $__foo AS x", ") AS x"].join("\n");
  await setText(p, bad);
  await waitTree(p, { markers: 1 });
  await p.click(tid("run-button"));
  await p.waitFor(`document.querySelectorAll(".sql__no--error").length > 0`, { timeout: 15000, what: "an error mark in the gutter" });
  s = await state(p);
  const marked = await p.eval(`[...document.querySelectorAll(".sql__no")].map((e) => e.classList.contains("sql__no--error") ? e.textContent : "").filter(Boolean)`);
  t.eq(marked, ["3"], "the server error is on line 3");
  await p.click(tid("sql-fold"));
  const hidden = await p.eval(`[...document.querySelectorAll(".sql__no")].map((e) => (e.classList.contains("sql__no--error") ? "!" : "") + e.textContent)`);
  t.eq(hidden, ["!1"], "the mark of the hidden line 3 is on the row of line 1 that hides it");
  await p.click(tid("sql-unfold-all"));
  t.eq(await p.eval(`[...document.querySelectorAll(".sql__no--error")].map((e) => e.textContent)`), ["3"], "the mark is on line 3 again");
}

/** Record each keydown that reaches the document, with `defaultPrevented` read after the handler of the text area. */
async function listenKeys(p) {
  await p.eval(`(() => { window.__keys = []; document.addEventListener("keydown", (e) => window.__keys.push({ code: e.code, mods: [e.ctrlKey && "ctrl", e.shiftKey && "shift", e.altKey && "alt", e.metaKey && "meta"].filter(Boolean).join("+"), prevented: e.defaultPrevented, repeat: e.repeat }), false); })()`);
}

// ---------------------------------------------------------------------------------------------------------------------
// B171

/** The text of the SQL lines that the page sent to the preview, from the last request (the area sends the same bytes with or without the tree). */
function previews(p) {
  return p.requests.filter((r) => r.method === "POST" && /\/api\/sources\/[^/]+\/preview$/.test(r.url)).map((r) => JSON.parse(r.body).panel.data.query.sql.join("\n"));
}

/** Give the clipboard of the page to the case (read and write), for the origin of the server. */
async function grantClipboard(t, p) {
  const origin = new URL(t.url()).origin;
  await p.browser.send("Browser.grantPermissions", { origin, permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"], browserContextId: p.context.id });
  await p.eval(`navigator.clipboard.writeText("")`);
}

const readClip = (p) => p.eval(`navigator.clipboard.readText()`);
const writeClip = (p, text) => p.eval(`navigator.clipboard.writeText(${JSON.stringify(text)})`);

/** The text of FOLD_TEXT in the area, the scroll at the top, and the two folds of the CTE (lines 1 to 6) and of the subquery in FROM (lines 8 to 11). */
async function foldTwo(p, text = FOLD_TEXT) {
  await setText(p, text);
  const want = expectedFolds(text);
  await waitTree(p, { markers: want.length });
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.scrollTop = 0; i.dispatchEvent(new Event("scroll")); i.focus(); })()`);
  await sleep(80);
  await p.click(`${tid("sql-fold")}[data-from="${want[0].open + 1}"]`);
  await p.click(`${tid("sql-fold")}[data-from="${want[1].open + 1}"]`);
  const s = await state(p);
  if (!s.folded || s.chips.length !== 2) throw new Error(`foldTwo: ${s.chips.length} chips, status ${s.status}`);
  return want;
}

/** The offset of the end of a line (1-based) of a text: the offset of its line end. */
const endOfLine = (text, line1) => text.split("\n").slice(0, line1).join("\n").length;

async function runB171(t) {
  const p = await openEditor(t, "ed171");
  await grantClipboard(t, p);
  const end7 = endOfLine(FOLD_TEXT, 7);

  // An edit in the folded view ends the folded view, and it lands at the caret that the click mapped. One Ctrl+Z undoes it.
  const edits = [
    ["a typed letter", async () => press(p, "KeyX", { key: "x" }), (v, c) => v.slice(0, c) + "x" + v.slice(c)],
    ["Backspace", async () => press(p, "Backspace"), (v, c) => v.slice(0, c - 1) + v.slice(c)],
    ["Delete at the line end (the line end)", async () => press(p, "Delete"), (v, c) => v.slice(0, c) + v.slice(c + 1)],
    ["Enter", async () => press(p, "Enter", { key: "Enter", text: "\r" }), (v, c) => v.slice(0, c) + "\n" + v.slice(c)],
    ["a paste", async () => { await writeClip(p, "PASTED"); await press(p, "KeyV", { mods: ["ctrl"], commands: ["paste"] }); }, (v, c) => v.slice(0, c) + "PASTED" + v.slice(c)],
    ["an IME composition", async () => { await p.send("Input.imeSetComposition", { text: "にほ", selectionStart: 2, selectionEnd: 2 }); await sleep(60); await p.send("Input.insertText", { text: "日本" }); }, (v, c) => v.slice(0, c) + "日本" + v.slice(c)],
    ["a dead key (the composition of a dead key, then the letter)", async () => { await press(p, "Quote", { key: "Dead" }); await p.send("Input.imeSetComposition", { text: "´", selectionStart: 1, selectionEnd: 1 }); await sleep(60); await p.send("Input.insertText", { text: "é" }); }, (v, c) => v.slice(0, c) + "é" + v.slice(c)],
  ];
  const events = await p.eval(`(() => { window.__comp = []; for (const n of ["compositionstart", "compositionend"]) document.addEventListener(n, () => window.__comp.push(n + ":" + (document.querySelector('[data-testid="sql-folded"]').style.visibility === "hidden" ? "open" : "folded")), false); return 1; })()`);
  void events;
  for (const [name, run, expect] of edits) {
    await foldTwo(p);
    await clickEndOfRow(p, 7);
    const before = await state(p);
    t.assert(before.folded, `${name}: still folded after a click in the folded view`);
    t.eq(before.a, end7, `${name}: the click in the folded view maps to the end of line 7`);
    t.eq(before.active, "sql-input", `${name}: the text area keeps the focus in the folded view`);
    await run();
    await sleep(120);
    const after = await state(p);
    t.assert(!after.folded && after.chips.length === 0, `${name}: the folded view ends`);
    t.eq(after.value, expect(FOLD_TEXT, end7), `${name}: the edit lands at the mapped caret`);
    t.eq(after.active, "sql-input", `${name}: the focus stays in the text area`);
    // One Ctrl+Z undoes the edit (the native undo has one step for it).
    await undoKey(p);
    await sleep(80);
    t.eq((await state(p)).value, FOLD_TEXT, `${name}: one Ctrl+Z undoes the edit`);
  }
  const comp = await p.eval(`window.__comp`);
  t.eq(comp, ["compositionstart:open", "compositionend:open", "compositionstart:open", "compositionend:open"], "the folded view has ended when the composition starts (read after the handler of the area), for the IME and for the dead key");

  // A drag over two chips copies the full text, with the hidden lines (read back from the clipboard).
  await foldTwo(p);
  const r1 = await rowOfLine(p, 1);
  const r8 = await rowOfLine(p, 8);
  const body = await rect(p, ".sql__body");
  const x0 = body.x + 11;
  await mouse(p, "mouseMoved", x0, r1.cy);
  await mouse(p, "mousePressed", x0, r1.cy);
  for (let k = 1; k <= 8; k++) await mouse(p, "mouseMoved", body.x + body.w - 60, r1.cy + ((r8.cy - r1.cy) * k) / 8, { buttons: 1 });
  await mouse(p, "mouseReleased", body.x + body.w - 60, r8.cy);
  await sleep(150);
  let s = await state(p);
  t.assert(s.folded, "a drag does not end the folded view");
  t.eq([s.a, s.b], [0, endOfLine(FOLD_TEXT, 11)], "the drag selects from line 1 to the end of line 11, across the two chips");
  const painted = await p.eval(`CSS.highlights.has("sql-selection") ? [...CSS.highlights.get("sql-selection")].length : 0`);
  t.assert(painted >= 2, `the folded view paints the selection with CSS.highlights: ${painted} ranges`);
  await press(p, "KeyC", { mods: ["ctrl"], commands: ["copy"] });
  t.eq(await readClip(p), FOLD_TEXT.slice(0, endOfLine(FOLD_TEXT, 11)), "the copy of the drag has the full text, with the hidden lines");
  t.assert((await readClip(p)).split("\n").length === 11, "11 lines were copied for the 8 rows");
  // Ctrl+A and Ctrl+C copy all, and the folded view stays.
  await press(p, "KeyA", { mods: ["ctrl"], commands: ["selectAll"] });
  await press(p, "KeyC", { mods: ["ctrl"], commands: ["copy"] });
  s = await state(p);
  t.eq([s.folded, s.a, s.b], [true, 0, FOLD_TEXT.length], "Ctrl+A selects the full text and the view stays folded");
  t.eq(await readClip(p), FOLD_TEXT, "Ctrl+A then Ctrl+C copy all the text");
  // Run and Ctrl+Enter work while folded and send the same text.
  p.clearRequests();
  await p.click(tid("run-button"));
  await sleep(800);
  await p.focus(SQL);
  await press(p, "Enter", { key: "Enter", mods: ["ctrl"] });
  await sleep(800);
  t.eq(previews(p), [FOLD_TEXT, FOLD_TEXT], "Run and Ctrl+Enter send the full text while the view is folded");
  t.assert((await state(p)).folded, "Run does not end the folded view");

  // The arrow keys, Home, End, PageUp and PageDown end the folded view and then move the caret natively.
  const lines = FOLD_TEXT.split("\n");
  const startOf = (line1) => lines.slice(0, line1 - 1).join("\n").length + (line1 > 1 ? 1 : 0);
  const moves = [
    ["ArrowLeft", (c) => c === end7 - 1],
    ["ArrowRight", (c) => c === end7 + 1],
    ["ArrowUp", (c) => c === endOfLine(FOLD_TEXT, 6)],
    ["ArrowDown", (c) => c === endOfLine(FOLD_TEXT, 8)],
    ["Home", (c) => c === startOf(7)],
    ["End", (c) => c === end7],
    ["PageUp", (c) => c < end7],
    ["PageDown", (c) => c > end7],
  ];
  for (const [code, ok] of moves) {
    await foldTwo(p);
    await clickEndOfRow(p, 7);
    t.eq((await state(p)).a, end7, `${code}: the caret is mapped to the end of line 7`);
    await press(p, code);
    await sleep(100);
    s = await state(p);
    t.assert(!s.folded, `${code}: the folded view ends`);
    t.assert(ok(s.a) && s.a === s.b, `${code}: the caret moves natively in the full text (caret ${s.a}, line 7 ends at ${end7})`);
    t.eq(s.value, FOLD_TEXT, `${code}: the text does not change`);
  }

  // A click on a chip unfolds that node only.
  await foldTwo(p);
  const chip = await p.eval(`(() => { const c = document.querySelectorAll('[data-testid="sql-chip"]'); return [c.length, c[0].dataset.from]; })()`);
  t.eq(chip[0], 2, "two chips");
  await p.click(`${tid("sql-chip")}[data-from="${chip[1]}"]`);
  s = await state(p);
  t.eq([s.folded, s.chips.length, s.chips[0] && s.chips[0].text, s.status], [true, 1, "… 4 lines", "Unfolded 1 block."], "a click on the chip of lines 1 to 6 unfolds that node, the other fold stays");
  t.eq(s.active, "sql-input", "the focus stays in the text area after a click on a chip");
  await p.click(tid("sql-unfold-all"));
  t.eq((await state(p)).value, FOLD_TEXT, "the text is the same after all these folds");
}

// ---------------------------------------------------------------------------------------------------------------------
// B172

/** A server of the case with a PostgreSQL source (down) and, when the build has the driver, a MongoDB source (down): only the kinds count. */
async function kindsServer(t, name) {
  const passwordFile = join(WORK, `${name}.password`);
  writeFileSync(passwordFile, "unused-password\n", { mode: 0o600 });
  chmodSync(passwordFile, 0o600);
  const pg = [
    'postgres "pgdown" {', '  label              = "PostgreSQL down"', '  host               = "127.0.0.1"', "  port               = 1", '  database           = "analytics"',
    '  user               = "hune_ro"', `  password_file      = ${JSON.stringify(passwordFile)}`, '  search_path        = "metrics"', '  sslmode            = "disable"',
    '  connect_timeout    = "1s"', '  max_execution_time = "1m"', "  max_result_rows    = 100000", "  max_result_bytes   = 33554432", "  max_query_size     = 1048576",
    "  max_concurrent     = 16", '  queue_wait         = "5s"', "}",
  ].join("\n");
  const driver = probeMongoDriver();
  if (driver.error) throw new Error("MongoDB probe: " + driver.error);
  const opts = { postgres: { block: pg } };
  if (driver.driver) opts.mongodb = { block: mongoBlockText({ port: 1, passwordFile }) };
  const server = await t.ownServer(name, opts);
  return { server, mongo: driver.driver === true };
}

/** One text for a dialect with `n` items, on one line with no blank after the commas: the format makes one line for each item. */
function longText(dialect, n) {
  if (dialect === "clickhouse") return `SELECT ${Array.from({ length: n }, (_, i) => `${i + 1} AS c${i + 1}`).join(",")} FROM analytics.seed`;
  if (dialect === "postgres") return `SELECT ${Array.from({ length: n }, (_, i) => `${i + 1}::int AS c${i + 1}`).join(",")} FROM metrics.http_requests`;
  return `{"collection":"node_load","aggregate":[${Array.from({ length: n }, (_, i) => `{"$limit":${i + 1}}`).join(",")}]}`;
}

/** The number of items that gives a result of `lines` lines: SQL has the keyword and the FROM line, JSON has the 5 lines of the frame. */
const itemsFor = (dialect, lines) => (dialect === "mongodb" ? lines - 5 : lines - 2);

/** The count of the characters that are not blank before an offset. */
const nonBlankBefore = (text, at) => (text.slice(0, at).match(/\S/g) || []).length;

/** The SQL of panel 2 in a draft record. */
function draftSql(d) {
  const panels = d.document.sections.flatMap((x) => x.panels);
  return panels.find((x) => x.id === 2).data.query.sql.join("\n");
}

const statusOf = async (p) => (await state(p)).status;

async function runB172(t) {
  const { server, mongo } = await kindsServer(t, "ast172");
  const dialects = [["clickhouse", null, "prod"], ["postgres", server, "pgdown"], ...(mongo ? [["mongodb", server, "mongo"]] : [])];
  if (!mongo) t.note("no MongoDB driver in this build: the MongoDB texts of B172 and B177 are not run");
  const sizes = [20, 500, 501];
  for (const [dialect, srv, source] of dialects) {
    const first = dialect === "mongodb" ? '{"collection":"node_load","aggregate":[{"$limit":1}]}' : dialect === "postgres" ? "SELECT 1" : null;
    const uid = `ed172${dialect.slice(0, 2)}`;
    const p = await openEditor(t, uid, { server: srv, source, sql: first ? first.split("\n") : null });
    await p.waitSel(SQL);
    for (const lines of sizes) {
      const tag = `${dialect} ${lines} lines`;
      const text = longText(dialect, itemsFor(dialect, lines));
      await setText(p, text);
      const marker = dialect === "mongodb" ? '{"$limit":7}' : "7 AS c7";
      const caret = text.indexOf(marker) + marker.length - (dialect === "mongodb" ? 0 : 1);
      await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.focus(); i.setSelectionRange(${caret}, ${caret}); })()`);
      await waitTree(p);
      const native = lines <= 500;
      // The key for the 20 lines, the button for the others.
      if (lines === 20) await formatKey(p);
      else await p.click(tid("sql-format"));
      await sleep(250);
      let s = await state(p);
      t.eq(s.value.split("\n").length, lines, `${tag}: the result has ${lines} lines`);
      t.eq(s.status, native ? "Formatted." : "Formatted. The undo history of the browser is cleared.", `${tag}: status after the format`);
      t.eq(s.undoButton, !native, `${tag}: the button Undo format shows only above 500 lines`);
      t.eq([s.a, s.b], [s.a, s.a], `${tag}: a collapsed caret remains`);
      t.eq(nonBlankBefore(s.value, s.a), nonBlankBefore(text, caret), `${tag}: the caret keeps its count of non-blank characters`);
      t.eq(s.value.replace(/\s+/g, ""), text.replace(/\s+/g, ""), `${tag}: only the blanks changed`);
      const formatted = s.value;
      // The draft changes as for an edit.
      const draft = await waitDraft(p, await uidOf(p), (d) => draftSql(d) === formatted);
      t.eq(draftSql(draft), formatted, `${tag}: the draft holds the formatted text`);
      // A second format changes nothing, says so, and makes no undo step.
      if (lines === 20) await p.click(tid("sql-format"));
      else await formatKey(p, ["ctrl", "shift"]);
      await sleep(120);
      s = await state(p);
      t.eq([s.value === formatted, s.status], [true, "Nothing to change."], `${tag}: a second format says "Nothing to change."`);
      t.eq(s.undoButton, !native, `${tag}: the second format does not touch the own undo entry`);
      if (native) {
        // One undo step and one redo step.
        await undoKey(p);
        await sleep(100);
        t.eq((await state(p)).value, text, `${tag}: one Ctrl+Z restores the text before the format (the second format made no step)`);
        await redoKey(p);
        await sleep(100);
        t.eq((await state(p)).value, formatted, `${tag}: one redo writes the format again`);
        await undoKey(p);
        await sleep(100);
        t.eq((await state(p)).value, text, `${tag}: and one more Ctrl+Z`);
      } else {
        // The own undo: the four keys of the spec, the button, and the end of the entry at another input.
        const steps = [
          [() => undoKey(p, ["ctrl"]), text, "Ctrl+Z"],
          [() => redoKey(p), formatted, "Ctrl+Shift+Z"],
          [() => undoKey(p, ["meta"]), text, "Cmd+Z"],
          [() => press(p, "KeyZ", { mods: ["meta", "shift"], commands: ["redo"] }), formatted, "Cmd+Shift+Z"],
          [() => undoKey(p, ["ctrl"]), text, "Ctrl+Z (again)"],
          [() => press(p, "KeyY", { mods: ["ctrl"], commands: ["redo"] }), formatted, "Ctrl+Y"],
          [() => p.click(tid("sql-undo-format")), text, "the button Undo format"],
          [() => press(p, "KeyY", { mods: ["ctrl"], commands: ["redo"] }), formatted, "Ctrl+Y after the button"],
        ];
        for (const [run, expect, name] of steps) {
          await run();
          await sleep(120);
          t.eq((await state(p)).value, expect, `${tag}: ${name}`);
        }
        s = await state(p);
        t.assert(s.undoButton, `${tag}: the button shows again after the redo`);
        t.eq(s.value.length, formatted.length, `${tag}: the redo gives the same bytes`);
        // Any other input removes the entry. The key then undoes the letter only (the native stack is empty before it).
        await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.focus(); i.setSelectionRange(0, 0); })()`);
        await press(p, "KeyX", { key: "x" });
        await sleep(120);
        s = await state(p);
        t.eq([s.value, s.undoButton], ["x" + formatted, false], `${tag}: another input removes the entry and the button`);
        await undoKey(p);
        await sleep(100);
        t.eq((await state(p)).value, formatted, `${tag}: Ctrl+Z undoes the letter, not the format`);
      }
    }

    // A selection formats the statements that it touches. The statements around it keep their bytes.
    if (dialect !== "mongodb") {
      const a = "SELECT  a,b  FROM t1;";
      const b = "SELECT  c,d  FROM t2;";
      const c = "SELECT e,f FROM t3";
      const text = [a, "", b, "", c].join("\n");
      await setText(p, text);
      const from = text.indexOf("c,d");
      await p.eval(`document.querySelector('${SQL}').setSelectionRange(${from}, ${from + 3})`);
      await p.click(tid("sql-format"));
      await sleep(150);
      t.eq((await state(p)).value, [a, "", "SELECT c, d\nFROM t2;", "", c].join("\n"), `${dialect}: a selection formats its statement only`);
      // A collapsed caret formats the whole text.
      await p.eval(`document.querySelector('${SQL}').setSelectionRange(${from}, ${from})`);
      await p.click(tid("sql-format"));
      await sleep(150);
      t.eq((await state(p)).value, ["SELECT a, b\nFROM t1;", "", "SELECT c, d\nFROM t2;", "", "SELECT e, f\nFROM t3"].join("\n"), `${dialect}: a collapsed caret formats the whole text`);
    }

    // The refusals: the text and the draft stay, the status gives the line and the column, the caret goes there.
    const refused = dialect === "mongodb"
      ? [['{"collection":"node_load",\n "aggregate":[1, 2}', "line 2, column 14: the text is not valid JSON.", '{"collection":"node_load",\n "aggregate":'.length]]
      : [["SELECT 1\nFROM (\n  SELECT 2\n", "line 2, column 6: a bracket has no match.", 14], ["SELECT 'abc\nFROM t", "line 1, column 8: a string, a name or a comment has no end.", 7], ["SELECT 1 -- c\n/* open\nFROM t", "line 2, column 1: a string, a name or a comment has no end.", 14]];
    for (const [bad, message, at] of refused) {
      await setText(p, bad);
      await p.click(tid("sql-format"));
      await sleep(120);
      const s = await state(p);
      t.eq(s.status, `Format needs a complete text: ${message}`, `${dialect}: refusal of ${JSON.stringify(bad)}`);
      t.eq(s.value, bad, `${dialect}: a refused format keeps the text`);
      if (at !== undefined) t.eq([s.a, s.b, s.active], [at, at, "sql-input"], `${dialect}: the caret goes to the place of the refusal`);
      t.eq(s.undoButton, false, `${dialect}: a refusal makes no undo entry`);
    }
  }

  // The save is not blocked: a text that the format changed is saved as it is (ClickHouse, 20 lines).
  const p = await openEditor(t, "ed172save");
  const saveUid = await uidOf(p);
  const text = longText("clickhouse", 18);
  await setText(p, text);
  await p.click(tid("sql-format"));
  await sleep(250);
  const formatted = (await state(p)).value;
  await backToDashboard(p);
  await saveDialog(p, "format");
  await p.waitFor(`document.querySelector('[data-testid="save-dialog"]') === null`, { timeout: 15000, what: "the save dialog closes" });
  let saved = null;
  for (let i = 0; i < 40 && saved !== formatted; i++) {
    await sleep(250);
    const g = await t.api.dashboard(saveUid);
    saved = g.json && g.json.document ? g.json.document.sections.flatMap((x) => x.panels).find((x) => x.id === 2).data.query.sql.join("\n") : null;
  }
  t.eq(saved, formatted, "the server holds the formatted text after the save");
}

// ---------------------------------------------------------------------------------------------------------------------
// B173

/** Put the caret of the text area at an offset, with the focus (the band and the pair follow it 50 ms later). */
async function caretAt(p, a, b = a) {
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.focus({ preventScroll: true }); i.setSelectionRange(${a}, ${b}); document.dispatchEvent(new Event("selectionchange")); })()`);
  await sleep(160);
}

/** The offsets of the marks of the pair in the colour layer: the sum of the lengths of the text nodes before each mark. */
async function pairOffsets(p) {
  return p.eval(`(() => {
    const code = document.querySelector(".sql__hl code");
    const marks = [...code.querySelectorAll('[data-testid="sql-pair"]')];
    const w = document.createTreeWalker(code, NodeFilter.SHOW_TEXT);
    const out = [];
    let pos = 0;
    for (let n = w.nextNode(); n; n = w.nextNode()) {
      const m = marks.find((x) => x.firstChild === n);
      if (m) out.push({ at: pos, text: n.data, lone: m.classList.contains("sql__pair--lone"), outline: getComputedStyle(m).outlineColor, width: getComputedStyle(m).outlineWidth, offset: getComputedStyle(m).outlineOffset, style: getComputedStyle(m).outlineStyle, bg: getComputedStyle(m).backgroundColor, pad: getComputedStyle(m).padding, margin: getComputedStyle(m).margin, border: getComputedStyle(m).borderTopWidth, weight: getComputedStyle(m).fontWeight });
      pos += n.data.length;
    }
    return out;
  })()`);
}

/** The rectangle of each of the first `n` characters of the colour layer, in CSS px. */
async function charRects(p, n) {
  return p.eval(`(() => {
    const code = document.querySelector(".sql__hl code");
    const w = document.createTreeWalker(code, NodeFilter.SHOW_TEXT);
    const out = [];
    for (let node = w.nextNode(); node && out.length < ${n}; node = w.nextNode()) {
      for (let k = 0; k < node.data.length && out.length < ${n}; k++) {
        const r = document.createRange();
        r.setStart(node, k);
        r.setEnd(node, k + 1);
        const b = r.getBoundingClientRect();
        out.push([b.left, b.top, b.width, b.height]);
      }
    }
    return out;
  })()`);
}

/** Contrast measures of the page: the colours are read from the page and blended on a canvas, so a colour with an alpha counts. */
async function contrastMeasures(p) {
  return p.eval(`(() => {
    const cv = document.createElement("canvas");
    cv.width = cv.height = 1;
    const ctx = cv.getContext("2d", { willReadFrequently: true });
    const paint = (css) => { ctx.fillStyle = css; ctx.fillRect(0, 0, 1, 1); };
    const read = () => [...ctx.getImageData(0, 0, 1, 1).data.slice(0, 3)];
    const lum = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
    const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
    const body = document.querySelector(".sql__body");
    const chain = [];
    for (let e = body; e; e = e.parentElement) chain.unshift(e);
    paint("rgb(0,0,0)");
    for (const e of chain) paint(getComputedStyle(e).backgroundColor);
    const surface = read();
    const band = document.querySelector('[data-testid="sql-block"]');
    paint("rgb(" + surface.join(",") + ")");
    paint(getComputedStyle(band).backgroundColor);
    const over = read();
    const tokens = {};
    // The classes that the sample text does not give (the function colour needs the list of functions, the error colour needs an error) are made here for the measure.
    const extra = ["tok-fn", "tok-err"].map((k) => { const e = document.createElement("span"); e.className = k; document.querySelector(".sql__hl code").append(e); return e; });
    for (const span of document.querySelectorAll('.sql__hl code [class^="tok-"]')) {
      const k = span.className.split(" ")[0];
      if (tokens[k]) continue;
      paint("rgb(" + over.join(",") + ")");
      paint(getComputedStyle(span).color);
      tokens[k] = +ratio(read(), over).toFixed(2);
    }
    for (const e of extra) e.remove();
    // The text colour of a plain word (no class), over the band.
    paint("rgb(" + over.join(",") + ")");
    paint(getComputedStyle(document.querySelector(".sql__hl code")).color);
    const plain = +ratio(read(), over).toFixed(2);
    const probe = document.createElement("i");
    document.body.append(probe);
    probe.style.background = "var(--accent-fill)";
    paint("rgb(" + surface.join(",") + ")");
    paint(getComputedStyle(probe).backgroundColor);
    const accent = read();
    probe.style.background = "var(--danger)";
    paint("rgb(" + surface.join(",") + ")");
    paint(getComputedStyle(probe).backgroundColor);
    const danger = read();
    probe.remove();
    return { tokens, plain, bar: +ratio(accent, surface).toFixed(2), danger: +ratio(danger, surface).toFixed(2), fill: getComputedStyle(band).backgroundColor, shadow: getComputedStyle(band).boxShadow, accent, dangerRgb: danger, surface };
  })()`);
}

const BAND_TEXT = ["SELECT toStartOfHour(now()), 'text' AS s, 42 AS n, NULL, CAST(n AS UInt8) -- a note", "FROM (", "  SELECT 1", "  FROM t", ") AS x"].join("\n");

async function runB173(t) {
  const results = [];
  for (const theme of ["dark", "light"]) {
    const p = await openEditor(t, `ed173${theme[0]}`, { theme });
    await setText(p, FOLD_TEXT);
    await waitTree(p, { markers: 4 });
    const pairs = bracketPairs(FOLD_TEXT);
    const at = (needle, add = 0) => FOLD_TEXT.indexOf(needle) + add;
    const body = await rect(p, ".sql__body");
    await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.scrollTop = 0; i.dispatchEvent(new Event("scroll")); })()`);

    // The band covers the lines of the innermost block, from its first line to its last line, over the full width.
    const blocks = [
      ["in the CTE", at("FROM metrics.cpu", 6), [1, 6], /^(subquery|cte)$/],
      ["in the subquery of FROM", at("SELECT host, cpu", 4), [8, 11], /^subquery$/],
      ["in the subquery of WHERE", FOLD_TEXT.lastIndexOf("FROM recent") + 3, [12, 15], /^subquery$/],
      ["in the array inside the call (the clause WHERE is the block)", at("[1, 2, 3]", 2), [12, 19], /^clause$/],
    ];
    for (const [name, caret, [first, last], type] of blocks) {
      await caretAt(p, caret);
      const s = await state(p);
      t.assert(s.band, `${theme}: a band shows ${name}`);
      t.assert(type.test(s.band.type), `${theme}: the type of the band ${name}: ${s.band.type}`);
      const row = await p.eval(`(() => { const out = {}; for (const e of document.querySelectorAll(".sql__no")) out[e.textContent] = e.getBoundingClientRect().top; return out; })()`);
      t.assert(Math.abs(s.band.top - row[first]) < 0.5, `${theme}: the band ${name} starts on line ${first}: ${s.band.top} against ${row[first]}`);
      t.eq(s.band.height, (last - first + 1) * ROW, `${theme}: the band ${name} covers lines ${first} to ${last}`);
      t.assert(Math.abs(s.band.left - body.x) < 0.5 && Math.abs(s.band.width - body.w) < 0.5, `${theme}: the band covers the full width ${name}`);
      t.assert(new RegExp(`^Block: ${s.band.type}( [^,]+)?, lines ${first} to ${last}\\.$`).test(s.info), `${theme}: the text of the block ${name}: ${s.info}`);
    }
    t.eq(await p.eval(`(() => { const e = document.querySelector('[data-testid="sql-info"]'); return !!e.closest("[aria-live]"); })()`), false, `${theme}: the text of the block is not live`);
    t.eq(await p.eval(`document.querySelector('[data-testid="sql-block"]').getAttribute("aria-hidden")`), "true", `${theme}: the band is aria-hidden`);

    // The band moves with the caret, when it moves by the keys. A caret in no block gives no band.
    await caretAt(p, at("FROM metrics.cpu", 3));
    t.eq((await state(p)).band.type.length > 0, true, `${theme}: a band before the move`);
    for (let k = 0; k < 4; k++) await press(p, "ArrowDown");
    await sleep(160);
    let s = await state(p);
    t.eq([s.a >= at("SELECT r.host"), s.band], [true, null], `${theme}: the caret on line 7 is in no block, so no band`);
    await press(p, "ArrowDown");
    await sleep(160);
    s = await state(p);
    t.assert(s.band && s.band.type === "clause" && s.band.height === 4 * ROW, `${theme}: the band moves to the clause FROM (lines 8 to 11): ${JSON.stringify(s.band)}`);
    // Not while the person types; not with a selection; not while folded.
    await press(p, "KeyX", { key: "x" });
    t.eq((await state(p)).band, null, `${theme}: no band at once after a key`);
    await press(p, "Backspace");
    await sleep(500);
    t.assert((await state(p)).band, `${theme}: the band is back after the parse`);
    await caretAt(p, at("FROM metrics.cpu", 3), at("FROM metrics.cpu", 8));
    s = await state(p);
    t.eq([s.band, s.pairs.length], [null, 0], `${theme}: no band and no mark with a selection`);
    await caretAt(p, at("FROM metrics.cpu", 3));
    await p.click(`${tid("sql-fold")}[data-from="${pairs[0].open + 1}"]`);
    await sleep(160);
    s = await state(p);
    t.eq([s.folded, s.band, s.pairs.length], [true, null, 0], `${theme}: no band and no mark while folded`);
    await p.click(tid("sql-unfold-all"));

    // The pair: the bracket before the caret, else the bracket at the caret, and its partner.
    const cte = pairs[0];
    for (const [name, caret] of [["after the close bracket", cte.close + 1], ["before the close bracket", cte.close], ["after the open bracket", cte.open + 1], ["before the open bracket", cte.open]]) {
      await caretAt(p, caret);
      const marks = await pairOffsets(p);
      t.eq(marks.map((m) => [m.at, m.text, m.lone]), [[cte.open, "(", false], [cte.close, ")", false]], `${theme}: both brackets of the pair have a mark, ${name}`);
    }
    await caretAt(p, cte.close + 1);
    let marks = await pairOffsets(p);
    t.eq([marks[0].style, marks[0].width, marks[0].offset, marks[0].bg, marks[0].pad, marks[0].margin, marks[0].border, marks[0].weight], ["solid", "1px", "-1px", "rgba(0, 0, 0, 0)", "0px", "0px", "0px", marks[0].weight], `${theme}: a mark is an outline of 1 px with an offset of -1 px, no fill, padding or margin`);
    const array = bracketPairs(FOLD_TEXT).find((x) => FOLD_TEXT.slice(x.open, x.close + 1) === "[1, 2, 3]");
    await caretAt(p, array.close + 1);
    marks = await pairOffsets(p);
    t.eq(marks.map((m) => [m.at, m.text]), [[array.open, "["], [array.close, "]"]], `${theme}: the pair of an array`);
    // A bracket in a string is not a bracket: no mark.
    await setText(p, "SELECT '(' AS a, 1)");
    await waitTree(p);
    await caretAt(p, "SELECT '(".length);
    t.eq((await state(p)).pairs.length, 0, `${theme}: a bracket in a string has no mark`);
    // A bracket with no partner: the mark of an error, and the text of the tool row.
    for (const [text, caret, text2] of [["SELECT (1 + 2\nFROM t", "SELECT (".length, "("], ["SELECT 1)\nFROM t", "SELECT 1)".length, ")"]]) {
      await setText(p, text);
      await waitTree(p);
      await caretAt(p, caret);
      s = await state(p);
      marks = await pairOffsets(p);
      t.eq(marks.map((m) => [m.text, m.lone]), [[text2, true]], `${theme}: a bracket with no match gets one mark of an error: ${JSON.stringify(text)}`);
      t.assert(s.info.includes("This bracket has no match."), `${theme}: the tool row says that the bracket has no match: ${s.info}`);
      const danger = await p.eval(`(() => { const e = document.createElement("i"); document.body.append(e); e.style.color = "var(--danger)"; const c = getComputedStyle(e).color; e.remove(); return c; })()`);
      t.eq(marks[0].outline, danger, `${theme}: the lone mark is drawn in --danger`);
      await partnerKey(p);
      t.eq((await state(p)).status, "This bracket has no match.", `${theme}: Ctrl+Shift+\\ on a bracket with no match says so`);
    }

    // The key Ctrl+Shift+\ : the caret goes after a close bracket, or before an open bracket.
    await setText(p, FOLD_TEXT);
    await waitTree(p, { markers: 4 });
    await caretAt(p, cte.close + 1);
    await partnerKey(p);
    await sleep(80);
    t.eq((await state(p)).a, cte.open, `${theme}: from after the close bracket the caret goes before the open bracket`);
    await partnerKey(p);
    await sleep(80);
    t.eq((await state(p)).a, cte.close + 1, `${theme}: from before the open bracket the caret goes after the close bracket`);
    // In the folded view the key ends the folded view first.
    await caretAt(p, cte.open + 1);
    // The layers of the gutter follow the area at the event "scroll", which the browser sends at the next frame. A click at once would find the
    // marker where it was before the scroll (148 px too high, under the preview). So the case sends the event and waits, as it does at the start.
    await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.scrollTop = 0; i.dispatchEvent(new Event("scroll")); })()`);
    await sleep(100);
    await p.click(`${tid("sql-fold")}[data-from="${cte.open + 1}"]`);
    await sleep(160);
    t.assert((await state(p)).folded, `${theme}: folded before the jump`);
    await partnerKey(p);
    await sleep(100);
    s = await state(p);
    t.eq([s.folded, s.a], [false, cte.close + 1], `${theme}: Ctrl+Shift+\\ ends the folded view and jumps`);

    // No glyph moves: the rectangle of each character is the same with the marks and with none, and the pixels are the same
    // when the outline and the band are hidden. The factors are 1, 0.9, 1.1 and 1.25 (the case B57 does the same for the selection).
    await setText(p, FOLD_TEXT);
    await waitTree(p, { markers: 4 });
    for (const dsf of [1, 0.9, 1.1, 1.25]) {
      await p.send("Emulation.setDeviceMetricsOverride", { width: 1400, height: 1000, deviceScaleFactor: dsf, mobile: false });
      await sleep(300);
      await caretAt(p, cte.close + 1);
      const withMarks = await charRects(p, 300);
      s = await state(p);
      t.assert(s.band && s.pairs.length === 2, `${theme} @${dsf}: a band and a pair show for the measure`);
      await p.eval(`(() => { const s = document.createElement("style"); s.id = "b173-hide"; s.textContent = ".sql__pair{outline:0 !important} .sql__band{display:none !important} .sql__input{visibility:hidden !important}"; document.head.append(s); })()`);
      const clip = await p.eval(`(() => { const b = document.querySelector(".sql__body").getBoundingClientRect(); return { x: b.left + 6, y: b.top + 6, width: b.width - 40, height: b.height - 24 }; })()`);
      const a = await p.send("Page.captureScreenshot", { format: "png", clip: { ...clip, scale: 1 } });
      await p.eval(`document.querySelector('${SQL}').blur()`);
      await sleep(200);
      s = await state(p);
      t.eq([s.band, s.pairs.length], [null, 0], `${theme} @${dsf}: the marks are gone after a blur`);
      const without = await charRects(p, 300);
      const b = await p.send("Page.captureScreenshot", { format: "png", clip: { ...clip, scale: 1 } });
      await p.eval(`document.getElementById("b173-hide").remove()`);
      let worst = 0;
      for (let i = 0; i < withMarks.length; i++) for (let k = 0; k < 4; k++) worst = Math.max(worst, Math.abs(withMarks[i][k] - without[i][k]));
      t.assert(withMarks.length === without.length && worst < 0.05, `${theme} @${dsf}: a mark and a band move no glyph (largest shift ${worst} px over ${withMarks.length} characters; a shift of 1/64 px is the unit of the layout)`);
      t.assert(a.data === b.data, `${theme} @${dsf}: the pixels of the colour layer are the same with and without the marks (outline and band hidden)`);
      results.push(`${theme} @${dsf}: ${worst} px`);
    }
    await p.send("Emulation.setDeviceMetricsOverride", { width: 1400, height: 1000, deviceScaleFactor: 1, mobile: false });

    // The contrast of each token colour over the band, and of the bar and of the mark with the surface.
    await setText(p, BAND_TEXT);
    await waitTree(p);
    await caretAt(p, BAND_TEXT.indexOf("SELECT 1") + 3);
    s = await state(p);
    t.assert(s.band, `${theme}: a band shows for the contrast`);
    const c = await contrastMeasures(p);
    const kinds = Object.keys(c.tokens);
    for (const k of ["tok-kw", "tok-str", "tok-com", "tok-num", "tok-null", "tok-type", "tok-fn", "tok-err"]) t.assert(kinds.includes(k), `${theme}: the sample has the token ${k}: ${kinds}`);
    for (const [k, v] of Object.entries(c.tokens)) t.assert(v >= 4.5, `${theme}: ${k} over the band has ${v}:1, 4.5:1 is the minimum`);
    t.assert(c.plain >= 4.5, `${theme}: the plain text over the band has ${c.plain}:1`);
    t.assert(c.bar >= 3, `${theme}: the bar of the band has ${c.bar}:1 with the surface, 3:1 is the minimum`);
    results.push(`${theme} contrast: tokens ${JSON.stringify(c.tokens)}, plain ${c.plain}, bar and mark ${c.bar}, danger ${c.danger}`);
    await caretAt(p, BAND_TEXT.indexOf("(") + 1);
    marks = await pairOffsets(p);
    const mark = await p.eval(`(() => { const e = document.querySelector('[data-testid="sql-pair"]'); return getComputedStyle(e).outlineColor; })()`);
    const accent = await p.eval(`(() => { const e = document.createElement("i"); document.body.append(e); e.style.color = "var(--accent-fill)"; const c = getComputedStyle(e).color; e.remove(); return c; })()`);
    t.eq(mark, accent, `${theme}: the pair mark is drawn in --accent-fill`);
    t.assert(c.danger >= 3, `${theme}: the mark of an error has ${c.danger}:1 with the surface`);
  }
  t.note("B173 largest shift of a glyph and contrast: " + results.join("; "));
}

// ---------------------------------------------------------------------------------------------------------------------
// B174

/** Set the text of the area from an expression that the page evaluates (a big text is made in the page, not sent), and fire `input`. */
async function setValue(p, expression) {
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.focus({ preventScroll: true }); i.value = ${expression}; i.setSelectionRange(0, 0); i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
}

/** The words of the notices of 10.3g. */
const NOTICE = {
  too_large: "Format and folds are off: the text has more than 262,144 bytes.",
  too_deep: "Format and folds are off: the text goes deeper than 128 levels.",
  too_many_nodes: "Format and folds are off: the text has more than 100,000 nodes.",
  markers: "Only the first 2,000 blocks can fold.",
};

/** The number of token spans of the colour layer: the colours stay above a cap. */
const colourSpans = (p) => p.eval(`document.querySelectorAll(".sql__hl code [class^='tok-']").length`);

/** Wait for the notice (or for none) and the number of markers. */
async function expectTree(t, p, label, { status, markers }) {
  try {
    await waitTree(p, { markers, status, timeout: 20000 });
  } catch (e) {
    const s = await state(p);
    throw new Error(`${label}: expected status ${JSON.stringify(status)} and ${markers} markers, got ${JSON.stringify(s.status)} and ${s.markers.length} (${e.message})`);
  }
}

async function runB174(t) {
  const p = await openEditor(t, "ed174");
  const sizes = {
    "ASCII": ["a", 1],
    "2-byte characters": ["é", 2],
    "3-byte characters": ["€", 3],
    "4-byte characters": ["😀", 4],
  };
  // The size: 262,144 bytes work, 262,145 bytes are a notice. The text has one fold at its start, and a comment of the filler.
  for (const [name, [ch, bytes]] of Object.entries(sizes)) {
    const head = "SELECT (\\n1\\n) -- ";
    const headBytes = 16;
    const count = (262144 - headBytes) / bytes;
    await setValue(p, `"${head}" + ${JSON.stringify(ch)}.repeat(${count})`);
    await expectTree(t, p, `${name}, 262,144 bytes`, { status: "", markers: 1 });
    t.assert(await colourSpans(p) > 0, `${name}: the colours are there`);
    await setValue(p, `"${head}" + ${JSON.stringify(ch)}.repeat(${count}) + "a"`);
    await expectTree(t, p, `${name}, 262,145 bytes`, { status: NOTICE.too_large, markers: 0 });
    t.assert(await colourSpans(p) > 0, `${name}: the colours stay above the cap`);
    t.eq((await p.eval(`document.querySelector('${SQL}').value.length`)) > 0, true, `${name}: the text is whole`);
    // The format says why it stops, and the text is not changed.
    await p.click(tid("sql-format"));
    t.eq((await state(p)).status, NOTICE.too_large, `${name}: the format stops with the notice`);
  }

  // The depth: 128 levels work, 129 levels are a notice.
  await setValue(p, `"(\\n".repeat(128) + "1" + "\\n)".repeat(128)`);
  await expectTree(t, p, "128 levels", { status: "", markers: 128 });
  await setValue(p, `"(\\n".repeat(129) + "1" + "\\n)".repeat(129)`);
  await expectTree(t, p, "129 levels", { status: NOTICE.too_deep, markers: 0 });
  t.assert(await colourSpans(p) > 0, "the colours stay above the depth cap");

  // The nodes. SQL: the cap has a margin of a few nodes in the code (the report says it), so the case takes a text far under and a text far over.
  await setValue(p, `"SELECT (\\n1\\n), " + Array(49900).fill("1").join(",")`);
  await expectTree(t, p, "49,900 numbers", { status: "", markers: 1 });
  await setValue(p, `"SELECT (\\n1\\n), " + Array(50100).fill("1").join(",")`);
  await expectTree(t, p, "50,100 numbers", { status: NOTICE.too_many_nodes, markers: 0 });
  t.assert(await colourSpans(p) > 0, "the colours stay above the node cap");
  // The folds: 2,000 blocks give 2,000 markers and no notice, 2,001 give 2,000 markers and the notice.
  const blocks = (n) => `"SELECT\\n" + Array.from({ length: ${n} }, (_, k) => "(\\n" + k + "\\n)").join(",\\n")`;
  await setValue(p, blocks(2000));
  await expectTree(t, p, "2,000 blocks", { status: "", markers: 2000 });
  await setValue(p, blocks(2001));
  await expectTree(t, p, "2,001 blocks", { status: NOTICE.markers, markers: 2000 });
  t.assert(await colourSpans(p) > 0, "the colours stay with 2,001 blocks");

  // The save and the run send the same bytes as with no tree, in each capped case: the draft, the preview and the save request.
  const uid = await uidOf(p);
  for (const [name, expr, notice] of [
    ["too many nodes", `"SELECT (\\n1\\n), " + Array(50100).fill("1").join(",")`, NOTICE.too_many_nodes],
    ["too deep", `"SELECT " + "(\\n".repeat(129) + "1" + "\\n)".repeat(129)`, NOTICE.too_deep],
    ["too large", `"SELECT 1 -- " + "a".repeat(262144)`, NOTICE.too_large],
  ]) {
    await setValue(p, expr);
    await expectTree(t, p, name, { status: notice, markers: 0 });
    const value = await p.eval(`document.querySelector('${SQL}').value`);
    await sleep(450); // the 300 ms debounce of the draft
    p.clearRequests();
    await p.click(tid("run-button"));
    await sleep(900);
    const sentTexts = previews(p);
    t.assert(sentTexts.length === 1 && sentTexts[0] === value, `${name}: the run sends the bytes of the text area (${sentTexts.length} requests, ${sentTexts.map((x) => x.length)} against ${value.length})`);
    const draft = await waitDraft(p, uid, (d) => draftSql(d) === value);
    t.assert(draftSql(draft) === value, `${name}: the draft holds the bytes of the text area`);
  }
  // The save of a capped text: the request holds the same bytes.
  await setValue(p, `"SELECT (\\n1\\n), " + Array(50100).fill("1").join(",")`);
  await expectTree(t, p, "save", { status: NOTICE.too_many_nodes, markers: 0 });
  const value = await p.eval(`document.querySelector('${SQL}').value`);
  await waitDraft(p, uid, (d) => draftSql(d) === value);
  await backToDashboard(p);
  p.clearRequests();
  await saveDialog(p, "capped");
  await p.waitFor(`document.querySelector('[data-testid="save-dialog"]') === null`, { timeout: 20000, what: "the save dialog closes" });
  const put = p.requests.find((r) => r.method === "PUT" && r.url.endsWith(`/api/dashboards/${uid}`));
  t.assert(put, "the save sent a PUT");
  const sent = JSON.parse(put.body).document.sections.flatMap((x) => x.panels).find((x) => x.id === 2).data.query.sql.join("\n");
  t.assert(sent === value, "the save sends the bytes of the text area, as it does with no tree");

  // Hostile texts: no exception and no console error (the runner fails the case on one). The format answers, or refuses, and the area works after.
  // The page loads the saved version of the dashboard: wait for panel 2 to settle before the menu (a click that comes in the middle of the redraw is lost).
  await p.waitFor(`(() => { const e = document.querySelector('[data-testid="panel-2"]'); return !!e && ["ready", "error", "empty"].includes(e.dataset.state); })()`, { timeout: 30000, what: "panel 2 settled after the save" });
  await p.idle();
  await sleep(500);
  try {
    await openPanelEditor(p, 2);
  } catch {
    await openPanelEditor(p, 2);
  }
  const hostile = {
    "a line of 100,000 characters": `"SELECT '" + "x".repeat(100000) + "' AS a"`,
    "a line of 100,000 names": `"SELECT " + "a,".repeat(50000) + "a"`,
    "10,000 open brackets": `"(".repeat(10000)`,
    "10,000 close brackets": `")".repeat(10000)`,
    "10,000 open square and curly brackets": `"[{".repeat(5000)`,
    "10,000 unclosed brackets with a select": `"SELECT " + "f(".repeat(10000)`,
    "a lone high surrogate": `"SELECT '\\uD800' AS a, (1"`,
    "a lone low surrogate": `"SELECT '\\uDC00' AS a, 1)"`,
    "a pair of surrogates and bidi controls": `"SELECT '\\uD83D\\uDE00\\u202E\\u2066' AS a"`,
    "a NUL and a BOM": `"\\uFEFFSELECT '\\u0000' AS a"`,
    "a string with no end": `"SELECT '" + "x".repeat(50000)`,
    "a comment with no end": `"SELECT 1 /* " + "x".repeat(50000)`,
    "dollar quotes": `"SELECT $$" + "($tag$".repeat(5000)`,
  };
  for (const [name, expr] of Object.entries(hostile)) {
    await setValue(p, expr);
    await sleep(900);
    const before = await p.eval(`document.querySelector('${SQL}').value`);
    await p.click(tid("sql-format"));
    await sleep(300);
    const s = await state(p);
    t.assert(s.value === before || s.value.replace(/\s+/g, "") === before.replace(/\s+/g, ""), `${name}: the format keeps the tokens`);
    t.assert(s.status !== "" || s.value !== before, `${name}: the format answers with a status or a change (${JSON.stringify(s.status)})`);
    await unfoldAllKey(p);
    await press(p, "BracketLeft", { key: "{", mods: ["ctrl", "shift"] });
    await partnerKey(p);
    await sleep(100);
    const probe = await p.eval(`document.querySelector('${SQL}').value.length`);
    t.assert(probe === s.value.length, `${name}: the keys of the tree do not change the text`);
  }
  // The area still works after all this: a normal text gives its markers again.
  await setText(p, FOLD_TEXT);
  await waitTree(p, { markers: 4, status: "" });

  // MongoDB: the JSON reader counts the nodes exactly. The doc and the array are two nodes, so 99,998 numbers make 100,000 nodes.
  const { server, mongo } = await kindsServer(t, "ast174");
  if (!mongo) {
    t.note("no MongoDB driver in this build: the node cap of the JSON text is not run in B174");
    return;
  }
  const m = await openEditor(t, "ed174mg", { server, source: "mongo", sql: ['{"collection":"node_load","aggregate":[{"$limit":1}]}'] });
  // A string every 1,000 items cuts the plain runs of the colour layer: a run of more than about 125,000 tokens is a known fault of the highlighter (reported, not a case of F42).
  const items = (n) => `"[" + Array.from({ length: ${n} }, (_, k) => (k % 1000 === 999 ? '"s"' : "1")).join(",") + "]"`;
  await setValue(m, items(99998));
  await expectTree(t, m, "99,998 numbers in JSON", { status: "", markers: 0 });
  t.assert(await colourSpans(m) >= 0, "the JSON text of 100,000 nodes works");
  await setValue(m, items(99999));
  await expectTree(t, m, "99,999 numbers in JSON", { status: NOTICE.too_many_nodes, markers: 0 });
  await setValue(m, `"[\\n".repeat(128) + "1" + "\\n]".repeat(128)`);
  await expectTree(t, m, "128 levels in JSON", { status: "", markers: 128 });
  await setValue(m, `"[\\n".repeat(129) + "1" + "\\n]".repeat(129)`);
  await expectTree(t, m, "129 levels in JSON", { status: NOTICE.too_deep, markers: 0 });
}

// ---------------------------------------------------------------------------------------------------------------------
// B175

/** The nodes of the accessibility tree that are not ignored, as `{ role, name, value }`. */
async function axNodes(p) {
  const { nodes } = await p.send("Accessibility.getFullAXTree");
  const byId = new Map(nodes.map((n) => [n.nodeId, n]));
  // The lines of the value of a text area are its own children in the tree: they are part of the text area.
  const inTextbox = (n) => { for (let x = byId.get(n.parentId); x; x = byId.get(x.parentId)) if (x.role && x.role.value === "textbox") return true; return false; };
  return nodes.filter((n) => !n.ignored && !inTextbox(n)).map((n) => ({ id: n.nodeId, role: n.role && n.role.value, name: n.name && n.name.value, value: n.value && n.value.value, backendDOMNodeId: n.backendDOMNodeId }));
}

/** The relative luminance contrast of the colour of an element over the surface of the area (a canvas blends an alpha). */
async function colourOver(p, selector) {
  return p.eval(`(() => {
    const cv = document.createElement("canvas"); cv.width = cv.height = 1;
    const ctx = cv.getContext("2d", { willReadFrequently: true });
    const paint = (css) => { ctx.fillStyle = css; ctx.fillRect(0, 0, 1, 1); };
    const read = () => [...ctx.getImageData(0, 0, 1, 1).data.slice(0, 3)];
    const lum = ([r, g, b]) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
    const chain = [];
    for (let e = document.querySelector(".sql__gutter"); e; e = e.parentElement) chain.unshift(e);
    paint("rgb(0,0,0)");
    for (const e of chain) paint(getComputedStyle(e).backgroundColor);
    const surface = read();
    const el = document.querySelector(${JSON.stringify(selector)});
    paint("rgb(" + surface.join(",") + ")");
    paint(getComputedStyle(el).color);
    const fg = read();
    const a = lum(fg), b = lum(surface);
    return +((Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)).toFixed(2);
  })()`);
}

async function runB175(t) {
  const results = [];
  for (const theme of ["dark", "light"]) {
    const p = await openEditor(t, `ed175${theme[0]}`, { theme });
    await p.send("Accessibility.enable");
    await setText(p, FOLD_TEXT);
    await waitTree(p, { markers: 4 });
    const want = expectedFolds(FOLD_TEXT);

    // The names and the references of the controls.
    const names = await p.eval(`(() => {
      const q = (s) => document.querySelector('[data-testid="' + s + '"]');
      const i = q("sql-input");
      const keys = document.getElementById(i.getAttribute("aria-describedby"));
      return { keyshortcuts: i.getAttribute("aria-keyshortcuts"), keys: keys && keys.textContent, keysTestid: keys && keys.dataset.testid, live: q("sql-status").getAttribute("aria-live"), format: q("sql-format").textContent, unfoldAll: q("sql-unfold-all").textContent, undo: q("sql-undo-format").textContent, label: i.getAttribute("aria-label"), ids: [...document.querySelectorAll("[id]")].map((e) => e.id).filter((x, k, all) => all.indexOf(x) !== k) };
    })()`);
    t.eq(names.keyshortcuts, "Control+Shift+F Control+Shift+BracketLeft Control+Shift+BracketRight Control+Shift+Backslash", `${theme}: aria-keyshortcuts`);
    t.eq(names.keysTestid, "sql-keys", `${theme}: aria-describedby points to the list of keys`);
    t.assert(/Ctrl\+Shift\+F/.test(names.keys) && /Ctrl\+Shift\+\[/.test(names.keys) && /Ctrl\+Shift\+\]/.test(names.keys) && /Ctrl\+Shift\+\\/.test(names.keys), `${theme}: the list of keys names the four keys`);
    t.eq([names.live, names.format, names.unfoldAll, names.undo], ["polite", "Format", "Unfold all", "Undo format"], `${theme}: the status is polite, the buttons have names`);
    t.eq(names.ids, [], `${theme}: no id is used twice`);

    // The area is the only accessible text: the layers, the band and the numbers are aria-hidden, and the markers are named buttons.
    const hidden = await p.eval(`["sql__hl", "sql__folded", "sql__band", "sql__nums"].map((c) => { const e = document.querySelector("." + c); return [c, e.closest('[aria-hidden="true"]') !== null]; })`);
    t.eq(hidden, [["sql__hl", true], ["sql__folded", true], ["sql__band", true], ["sql__nums", true]], `${theme}: the layers are aria-hidden`);
    let ax = await axNodes(p);
    const word = "recent";
    const texts = ax.filter((n) => typeof n.name === "string" && n.name.includes(word) || typeof n.value === "string" && n.value.includes(word));
    t.eq(texts.map((n) => n.role), ["textbox"], `${theme}: the text of the query is in the text area only (accessible tree)`);
    t.eq(texts[0].value.length, FOLD_TEXT.length, `${theme}: the text area holds the full text`);
    // The markers: named buttons, not in the Tab order.
    const markers = ax.filter((n) => n.role === "button" && /^(Fold|Unfold) lines/.test(n.name || ""));
    t.eq(markers.map((m) => m.name), want.map((f) => `Fold lines ${f.line + 1} to ${f.endLine + 1}`), `${theme}: the markers have names in the accessible tree`);

    // Folded: the text area still holds the full text, and the accessible tree has no text of the folded view.
    await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.scrollTop = 0; i.dispatchEvent(new Event("scroll")); i.focus(); })()`);
    await p.click(`${tid("sql-fold")}[data-from="${want[0].open + 1}"]`);
    await p.click(`${tid("sql-fold")}[data-from="${want[1].open + 1}"]`);
    ax = await axNodes(p);
    const folded = ax.filter((n) => typeof n.name === "string" && n.name.includes(word) || typeof n.value === "string" && n.value.includes(word));
    t.eq(folded.map((n) => n.role), ["textbox"], `${theme}: folded, the text is still in the text area only`);
    t.eq(folded[0].value, FOLD_TEXT, `${theme}: folded, the text area holds the full text`);
    t.assert(!ax.some((n) => /^… \d+ lines$/.test(n.name || "")), `${theme}: the chips are not in the accessible tree`);
    await p.click(tid("sql-unfold-all"));

    // The messages of the status line, in order (one per action): a recorder reads the text each time it changes.
    await p.eval(`(() => { window.__said = []; const e = document.querySelector('[data-testid="sql-status"]'); new MutationObserver(() => { if (e.textContent) window.__said.push(e.textContent); }).observe(e, { childList: true, characterData: true, subtree: true }); })()`);
    const inCte = FOLD_TEXT.indexOf("FROM metrics.cpu") + 5;
    await caretAt(p, inCte);
    await fold(p);
    await sleep(60);
    await fold(p);
    await sleep(60);
    await fold(p);
    await sleep(60);
    await unfoldAllKey(p);
    await sleep(60);
    await unfoldAllKey(p);
    await sleep(60);
    await caretAt(p, FOLD_TEXT.indexOf("SELECT r.host") + 2);
    await fold(p);
    await sleep(60);
    await p.click(tid("sql-format"));
    await sleep(100);
    const said = await p.eval(`window.__said`);
    t.eq(said, ["Folded 1 block, 6 lines.", "Unfolded 1 block.", "Folded 1 block, 6 lines.", "Unfolded all.", "Nothing is folded.", "Nothing to fold here.", "Formatted."], `${theme}: the messages of the status line, in order`);
    t.eq((await state(p)).active, "sql-input", `${theme}: the focus stayed in the text area for a fold, an unfold and a format`);
    // A jump keeps the focus too.
    await caretAt(p, (await state(p)).value.indexOf("(") + 1);
    await partnerKey(p);
    t.eq((await state(p)).active, "sql-input", `${theme}: the focus stays in the text area for a jump`);
    await p.click(tid("sql-format"));

    // Tab, Escape then Tab, Ctrl+Enter work as before (10.3).
    await setText(p, "SELECT 1");
    await caretAt(p, 8);
    await press(p, "Tab");
    t.eq((await state(p)).value, "SELECT 1  ", `${theme}: Tab types two spaces`);
    await press(p, "Escape");
    await press(p, "Tab");
    await sleep(60);
    t.assert((await state(p)).active !== "sql-input", `${theme}: Escape then Tab moves the focus out of the text area`);
    t.eq((await state(p)).value, "SELECT 1  ", `${theme}: Escape then Tab types nothing`);
    await p.focus(SQL);
    p.clearRequests();
    await press(p, "Enter", { key: "Enter", mods: ["ctrl"] });
    await sleep(900);
    t.eq(previews(p).length, 1, `${theme}: Ctrl+Enter runs the query`);
    // The markers are not in the Tab order: ten Tab presses from the text area never land on one.
    await setText(p, FOLD_TEXT);
    await waitTree(p, { markers: 4 });
    const landed = [];
    await p.focus(SQL);
    for (let k = 0; k < 10; k++) {
      await press(p, "Escape");
      await press(p, "Tab");
      landed.push(await p.eval(`(document.activeElement.dataset || {}).testid || document.activeElement.tagName`));
      await p.focus(SQL);
    }
    t.assert(!landed.includes("sql-fold"), `${theme}: no Tab lands on a fold marker: ${landed}`);
    t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="sql-fold"]')].every((b) => b.tabIndex === -1)`), true, `${theme}: each marker has tabindex -1`);

    // The contrast of the markers and of the notes.
    const marker = await colourOver(p, tid("sql-fold"));
    const status = await colourOver(p, tid("sql-status"));
    const keysText = await colourOver(p, tid("sql-keys"));
    t.assert(marker >= 3, `${theme}: a fold marker has ${marker}:1 over the gutter`);
    t.assert(status >= 4.5 && keysText >= 4.5, `${theme}: the status ${status}:1 and the keys ${keysText}:1 are 4.5:1 or more`);
    results.push(`${theme}: marker ${marker}, status ${status}, keys ${keysText}`);
  }
  // No animation, also with prefers-reduced-motion: reduce (and without it).
  const p = await openEditor(t, "ed175m", { theme: "dark" });
  for (const motion of ["reduce", "no-preference"]) {
    await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: motion }, { name: "prefers-color-scheme", value: "dark" }] });
    await setText(p, FOLD_TEXT);
    await waitTree(p, { markers: 4 });
    const want = expectedFolds(FOLD_TEXT);
    await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.scrollTop = 0; i.dispatchEvent(new Event("scroll")); i.focus(); window.__anim = 0; window.__animOutside = 0; const count = (e) => { if (e.target.closest && e.target.closest(".sqlbox")) window.__anim++; else window.__animOutside++; }; document.addEventListener("animationstart", count, true); document.addEventListener("transitionstart", count, true); })()`);
    await caretAt(p, FOLD_TEXT.indexOf("FROM metrics.cpu") + 5);
    await p.click(`${tid("sql-fold")}[data-from="${want[0].open + 1}"]`);
    await press(p, "KeyX", { key: "x" });
    await sleep(500);
    const info = await p.eval(`(() => {
      const els = [document.querySelector(".sql"), ...document.querySelectorAll(".sql *")];
      const withAnim = els.filter((e) => { const c = getComputedStyle(e); return c.animationName !== "none" || parseFloat(c.transitionDuration) > 0; }).map((e) => e.className || e.tagName);
      return { withAnim, running: document.querySelector(".sqlbox").getAnimations({ subtree: true }).length, events: window.__anim, outside: window.__animOutside };
    })()`);
    // The events are counted for the editor only (.sqlbox), as the two other measures are. The page has other parts that move on their own, for
    // example the three dots of the preview while it loads: they are not the fold, the band or the pair.
    t.note(`B175 motion ${motion}: events outside the editor, not counted: ${info.outside}`);
    t.eq([info.withAnim, info.running, info.events], [[], 0, 0], `motion ${motion}: the fold, the band and the pair use no animation and no transition`);
  }
  t.note("B175 contrast: " + results.join("; ") + ". axe-core is not run: it is not available offline (as in B14 and B82); the scripted checks above replace it");
}

// ---------------------------------------------------------------------------------------------------------------------
// B176

/** The URL of a module of the editor, as the page loaded it (the path has the hash of the build). */
const moduleUrl = (p, file) => p.eval(`performance.getEntriesByType("resource").map((e) => e.name).find((n) => n.endsWith(${JSON.stringify("/js/editor/" + file)}))`);

/** Times of one function over `runs` runs, in ms, sorted, with the first run thrown away (the warm-up). */
const TIMER = `
  const times = async (fn, runs) => { fn(); const out = []; for (let k = 0; k < runs; k++) { const a = performance.now(); fn(); out.push(performance.now() - a); } return out.sort((x, y) => x - y); };
`;

async function runB176(t) {
  const p = await openEditor(t, "ed176");
  const U = {
    sql: await moduleUrl(p, "ast/sql_tree.js"),
    json: await moduleUrl(p, "ast/json_tree.js"),
    format: await moduleUrl(p, "ast/format.js"),
    structure: await moduleUrl(p, "ast/structure.js"),
  };
  // A text of exactly 262,144 bytes that makes no more than 100,000 nodes: statements with a subquery on a few lines.
  const make = `(() => {
    const unit = "SELECT a, b\\nFROM (\\n  SELECT c\\n  FROM t\\n) AS x\\nWHERE a = 1;\\n";
    let text = "";
    while (text.length + unit.length <= 262144) text += unit;
    return text + " ".repeat(262144 - text.length);
  })()`;
  const parse = await p.eval(`(async () => {
    ${TIMER}
    const sql = await import(${JSON.stringify(U.sql)});
    const fmt = await import(${JSON.stringify(U.format)});
    const st = await import(${JSON.stringify(U.structure)});
    const text = ${make};
    const bytes = new TextEncoder().encode(text).length;
    const res = sql.parseSql(text, "clickhouse");
    const pg = sql.parseSql(text, "postgres");
    const parseTimes = await times(() => sql.parseSql(text, "clickhouse"), 7);
    const pgTimes = await times(() => sql.parseSql(text, "postgres"), 7);
    const foldTimes = await times(() => st.folds(res, text), 7);
    const out = fmt.formatSql(text, "clickhouse");
    const formatTimes = await times(() => fmt.formatSql(text, "clickhouse"), 5);
    const jsonText = "[" + Array.from({ length: 40000 }, (_, k) => '{"$limit":' + (k % 10) + '}').join(",") + "]";
    const json = await import(${JSON.stringify(U.json)});
    const jsonBytes = new TextEncoder().encode(jsonText).length;
    const jsonTimes = await times(() => json.parseJson(jsonText), 7);
    const jsonFormat = await times(() => fmt.formatJson(jsonText), 3);
    return { bytes, capped: res.capped, pgCapped: pg.capped, parseTimes, pgTimes, foldTimes, reason: out.reason || null, changed: out.changed, formatTimes, jsonBytes, jsonTimes, jsonFormat };
  })()`);
  t.eq(parse.bytes, 262144, "the text of the measure has 262,144 bytes");
  t.eq([parse.capped, parse.pgCapped, parse.reason], [null, null, null], "the text of the measure is not capped and the format does not refuse it");
  const med = (a) => a[Math.floor(a.length / 2)];
  const r2 = (x) => Math.round(x * 10) / 10;
  t.note(`B176 parse of 262,144 bytes (ClickHouse): min ${r2(parse.parseTimes[0])} ms, median ${r2(med(parse.parseTimes))} ms, max ${r2(parse.parseTimes.at(-1))} ms; PostgreSQL: min ${r2(parse.pgTimes[0])}, median ${r2(med(parse.pgTimes))}; folds: median ${r2(med(parse.foldTimes))} ms; format: min ${r2(parse.formatTimes[0])}, median ${r2(med(parse.formatTimes))}, max ${r2(parse.formatTimes.at(-1))} ms; JSON parse of ${parse.jsonBytes} bytes: median ${r2(med(parse.jsonTimes))} ms, JSON format ${r2(parse.jsonFormat[0])} ms`);
  // The target is 50 ms and 300 ms on the reference machine. A loaded machine only adds time, so the best of the runs must meet the target, and the median must meet the limit of the unit tests (100 ms).
  t.assert(parse.parseTimes[0] <= 50, `the parse of 262,144 bytes takes 50 ms or less (best of 7: ${r2(parse.parseTimes[0])} ms, median ${r2(med(parse.parseTimes))} ms)`);
  t.assert(med(parse.parseTimes) <= 100, `the median parse is under the limit of 100 ms of the unit tests: ${r2(med(parse.parseTimes))} ms`);
  t.assert(parse.formatTimes[0] <= 300, `the format of 262,144 bytes takes 300 ms or less (best of 5: ${r2(parse.formatTimes[0])} ms)`);

  // The folded view: the time from the click on a marker to the second frame. A text of 19 lines (the usual size) and the big text (262,144 bytes) must draw in 100 ms or less.
  // The folded view, the gutter, the markers and the colour layer draw only the rows near the visible rows (10.3e). A fold changes no text in the flow of the
  // page and no `hidden` or `display` of a layer, so the browser does not lay out the text area again (about 140 ms for 262,144 bytes). Before the change, one fold took 335 ms.
  const foldDraw = (n) => p.eval(`(async () => {
    const out = [];
    for (let k = 0; k < ${n}; k++) {
      const marker = document.querySelector('[data-testid="sql-fold"]');
      const a = performance.now();
      marker.click();
      const handler = performance.now() - a;
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      out.push({ handler, total: performance.now() - a, shown: document.querySelector('[data-testid="sql-folded"]').style.visibility !== "hidden" });
      document.querySelector('[data-testid="sql-unfold-all"]').click();
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    }
    return out;
  })()`);
  await setText(p, FOLD_TEXT);
  await waitTree(p, { markers: 4 });
  await p.eval(`document.querySelector('${SQL}').scrollTop = 0`);
  const usual = await foldDraw(5);
  t.assert(usual.every((x) => x.shown), "the folded view shows");
  const usualBest = Math.min(...usual.map((x) => x.total));
  t.note(`B176 folded view of a 19-line text: handler and handler plus two frames: ${usual.map((x) => `${r2(x.handler)} and ${r2(x.total)} ms`).join(", ")}`);
  t.assert(usualBest <= 100, `the folded view draws in 100 ms or less (best of 5 on a 19-line text: ${r2(usualBest)} ms)`);
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.focus(); i.value = ${make}; i.setSelectionRange(0, 0); i.dispatchEvent(new Event("input", { bubbles: true })); })()`);
  await p.waitFor(`Number(document.querySelector('.sql__marks').dataset.count) > 100`, { timeout: 60000, what: "the markers of the big text" });
  await p.eval(`document.querySelector('${SQL}').scrollTop = 0`);
  const big = await foldDraw(7);
  t.assert(big.every((x) => x.shown), "the folded view shows for the big text");
  const bigBest = Math.min(...big.map((x) => x.total));
  const bigHandler = Math.min(...big.map((x) => x.handler));
  t.note(`B176 folded view of the big text (262,144 bytes, one fold): handler and handler plus two frames: ${big.map((x) => `${r2(x.handler)} and ${r2(x.total)} ms`).join(", ")}; the least of 7: handler ${r2(bigHandler)} ms, with two frames ${r2(bigBest)} ms (target 100 ms)`);
  t.assert(bigBest <= 100, `the folded view of the big text (262,144 bytes) draws in 100 ms or less (the least of 7: ${r2(bigBest)} ms, handler ${r2(bigHandler)} ms)`);

  // A key: the work of F42 is to clear the marks and to start a timer. The colour layer draws only the rows near the visible rows, so a key in the big
  // text costs the same as a key in a small text. The time of a key is the time from the event to the second frame after it (it has the two frames, about 33 ms, in it).
  await p.eval(`(() => { const i = document.querySelector('${SQL}'); i.focus(); i.setSelectionRange(0, 0); window.__long = []; new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__long.push(Math.round(e.duration)); }).observe({ entryTypes: ["longtask"] }); })()`);
  await sleep(1500);
  await p.send("Profiler.enable");
  await p.send("Profiler.setSamplingInterval", { interval: 200 });
  await p.eval(`window.__long.length = 0`);
  await p.send("Profiler.start");
  for (let k = 0; k < 3; k++) {
    await press(p, "KeyX", { key: "x" });
    await sleep(1800);
  }
  const { profile } = await p.send("Profiler.stop");
  const longTasks = await p.eval(`window.__long`);
  // Inclusive time of each call frame by name, from the samples.
  const nodes = new Map(profile.nodes.map((n) => [n.id, n]));
  const parent = new Map();
  for (const n of profile.nodes) for (const c of n.children || []) parent.set(c, n.id);
  const self = new Map();
  profile.samples.forEach((id, k) => self.set(id, (self.get(id) || 0) + (profile.timeDeltas[k] || 0)));
  const chainOf = (id) => { const out = []; for (let x = id; x !== undefined; x = parent.get(x)) out.push(nodes.get(x).callFrame); return out; };
  let colour = 0;
  let f42 = 0;
  let total = 0;
  for (const [id, us] of self) {
    const chain = chainOf(id);
    const names = chain.map((c) => `${c.url.split("/").pop()}:${c.functionName}`);
    const urls = chain.map((c) => c.url);
    if (!urls.some((u) => u.endsWith("/sql_editor.js"))) continue;
    total += us;
    if (names.some((n) => n === "sql_editor.js:paintCode" || n === "sql_editor.js:renderGutter" || n === "sql_editor.js:paint")) colour += us;
    else if (urls.some((u) => /\/editor\/ast\/|\/sql_fold_view\.js$/.test(u)) || names.some((n) => /^sql_editor\.js:(changed|scheduleParse|clearCaretMarks|cancelParse)$/.test(n))) f42 += us;
  }
  t.note(`B176 typing in the big text: 3 keys; the longest tasks of the main thread (ms): ${longTasks.join(", ") || "none"}; JS time under the area: colour layer ${r2(colour / 3000)} ms for each key, F42 ${r2(f42 / 3000)} ms for each key`);
  t.assert(f42 / 3000 < 50, `at a key, the JS work of F42 is under 50 ms (${r2(f42 / 3000)} ms for each key)`);
  t.assert(colour / 3000 < 50, `at a key, the JS work of the colour layer is under 50 ms (${r2(colour / 3000)} ms for each key)`);
  t.assert(Math.max(0, ...longTasks) < 100, `no task of the main thread takes 100 ms or more while typing in the big text (longest: ${Math.max(0, ...longTasks)} ms; the parse after the last key is one of the tasks)`);
  await p.send("Profiler.disable");
  // The time of a key: from the event `input` (its time stamp is the time of the key) to the second frame after it. The least of 5 keys must be under 100 ms.
  await p.eval(`(() => { window.__keyTimes = []; document.querySelector('${SQL}').addEventListener("input", (e) => { const t0 = e.timeStamp; requestAnimationFrame(() => requestAnimationFrame(() => window.__keyTimes.push(performance.now() - t0))); }); })()`);
  for (let k = 0; k < 5; k++) {
    await press(p, "KeyX", { key: "x" });
    await sleep(1200);
  }
  const keyTimes = (await p.eval(`window.__keyTimes`)).sort((a, b) => a - b);
  t.note(`B176 typing in the big text: time from a key to the second frame after it (ms, sorted): ${keyTimes.map(r2).join(", ")}`);
  t.assert(keyTimes.length === 5 && keyTimes[0] < 100, `a key in the big text (246 KB and more) takes less than 100 ms to the second frame after it (the least of 5: ${r2(keyTimes[0])} ms)`);

  // The page measure: a dashboard page that is in view mode requests no file of the editor, no `ast/` and no folded view.
  const v = await t.page({ width: 1400, height: 1000 });
  await v.goto(t.dash("ed176"));
  await v.idle();
  await waitPanelsReady(v, [1, 2, 3]);
  const asked = v.requests.map((r) => r.url);
  t.eq(asked.filter((u) => /\/js\/editor\/ast\//.test(u) || /sql_fold_view\.js|sql_editor\.js/.test(u)), [], "the view page requests no file of editor/ast/ and no editor file of the SQL area");
  t.note(`B176 view page: ${asked.filter((u) => /\.js(\?|$)/.test(u)).length} script requests, none of the tree`);
}

// ---------------------------------------------------------------------------------------------------------------------
// B177

/** What the tool row has: the hooks of 9.29 and the attributes of the text area. */
async function toolRow(p) {
  return p.eval(`(() => {
    const has = (id) => !!document.querySelector('[data-testid="' + id + '"]');
    const i = document.querySelector('[data-testid="sql-input"]');
    return { hooks: ["sql-format", "sql-unfold-all", "sql-undo-format", "sql-status", "sql-info", "sql-keys", "sql-folded", "sql-block"].map(has), describedby: !!document.getElementById(i.getAttribute("aria-describedby")), keyshortcuts: !!i.getAttribute("aria-keyshortcuts"), formatVisible: document.querySelector('[data-testid="sql-format"]').getBoundingClientRect().height > 0 };
  })()`);
}

/** Set a text, wait for the tree, and say what the area does with it: the names of the markers and the answer of the format. */
async function behaviour(p, text) {
  await setText(p, text);
  await p.focus(SQL);
  await sleep(500);
  const markers = (await state(p)).markers.map((m) => m.name);
  await p.click(tid("sql-format"));
  await sleep(150);
  const s = await state(p);
  return { markers, status: s.status, value: s.value };
}

// A `#` is a comment in ClickHouse and an operator in PostgreSQL: the open bracket of line 1 has a match only in ClickHouse (10.3).
const HASH_TEXT = "SELECT 1 # (\n, (\n2\n) FROM t";
// `$$ ... $$` is one string in PostgreSQL. A bracket inside it is not code there.
const DOLLAR_TEXT = "SELECT $$(\n$$ AS a,\n(\n1\n) AS b FROM t";

async function runB177(t) {
  const { server, mongo } = await kindsServer(t, "ast177");
  const p = await openEditor(t, "ed177", { server });
  const label = await byLabel(p, "Source");

  // ClickHouse (the source prod): the tool row.
  const row = await toolRow(p);
  t.eq([row.hooks.every(Boolean), row.describedby, row.keyshortcuts, row.formatVisible], [true, true, true, true], "ClickHouse panel editor: the tool row");
  const chHash = await behaviour(p, HASH_TEXT);
  t.eq([chHash.markers, chHash.status], [["Fold lines 2 to 4"], "Formatted."], "ClickHouse: `#` is a comment, the bracket pair folds and the format works");
  const chDollar = await behaviour(p, DOLLAR_TEXT);
  const chLambda = await behaviour(p, "SELECT arrayMap(\n  x -> x,\n  a\n) FROM t");
  t.eq(chLambda.markers, ["Fold lines 1 to 4"], "ClickHouse: a call with a lambda folds");
  t.eq(chLambda.status, "Formatted.", "ClickHouse: a lambda formats");
  t.assert(chLambda.value.includes("x -> x"), "ClickHouse: the lambda keeps its bytes");

  // A change of the source changes the dialect: the same texts under PostgreSQL.
  await p.select(label, "pgdown");
  await sleep(300);
  const pgHash = await behaviour(p, HASH_TEXT);
  t.assert(/^Format needs a complete text: line 1, column 12: a bracket has no match\.$/.test(pgHash.status), `PostgreSQL: \`#\` is an operator, so the bracket of line 1 has no match: ${pgHash.status}`);
  t.assert(chHash.status !== pgHash.status, "the same text gives another answer when the source changes");
  const pgDollar = await behaviour(p, DOLLAR_TEXT);
  t.eq([pgDollar.markers, pgDollar.status], [["Fold lines 3 to 5"], "Formatted."], "PostgreSQL: a bracket in `$$ ... $$` is text, the pair after it folds and the format works");
  t.assert(pgDollar.value.includes("$$(\n$$"), "PostgreSQL: the dollar quote keeps its bytes and its line end");
  t.assert(JSON.stringify([chDollar.markers, chDollar.status]) !== JSON.stringify([pgDollar.markers, pgDollar.status]), `the dollar quote gives another result in ClickHouse (${JSON.stringify([chDollar.markers, chDollar.status])})`);
  const pgCast = await behaviour(p, "SELECT   x::Int ,  y::text[]   FROM   t");
  t.eq(pgCast.value, "SELECT x::Int, y::text[]\nFROM t", "PostgreSQL: a cast is not split");
  t.eq((await toolRow(p)).hooks.every(Boolean), true, "PostgreSQL panel editor: the tool row");
  // And back.
  await p.select(label, "prod");
  await sleep(300);
  const again = await behaviour(p, HASH_TEXT);
  t.eq([again.markers, again.status], [chHash.markers, chHash.status], "back to ClickHouse: the dialect is ClickHouse again");

  // MongoDB: the JSON text, with `$facet`.
  const FACET = ['{"collection":"node_load","aggregate":[', '{"$facet":{', '"a":[{"$limit":1}],', '"b":[{"$limit":2}]', "}}", "]}"].join("\n");
  if (mongo) {
    await p.select(label, "mongo");
    await sleep(300);
    const mg = await behaviour(p, FACET);
    t.eq(mg.markers, expectedFolds(FACET).map((f) => `Fold lines ${f.line + 1} to ${f.endLine + 1}`), "MongoDB: the objects and the arrays of the pipeline fold, `$facet` too");
    t.eq(mg.status, "Formatted.", "MongoDB: the JSON text formats");
    t.assert(mg.value.includes('"$facet": {') && mg.value.replace(/\s+/g, "") === FACET.replace(/\s+/g, ""), "MongoDB: the format keeps the tokens and writes `: `");
    t.eq((await toolRow(p)).hooks.every(Boolean), true, "MongoDB panel editor: the tool row");
    const sqlText = await behaviour(p, "SELECT 1 # (\n, (\n2\n) FROM t");
    t.assert(/not valid JSON/.test(sqlText.status), `MongoDB: a SQL text is not valid JSON: ${sqlText.status}`);
    await p.select(label, "prod");
    await sleep(300);
  } else t.note("no MongoDB driver in this build: the MongoDB part of B177 is not run");

  // A panel with two queries: one area for each query, each with its tool row, and the dialect of the source of its query.
  await setText(p, HASH_TEXT);
  await p.click(tid("query-add"));
  await p.waitSel(tid("query-chip-B"));
  await p.waitSel(SQL);
  await sleep(300);
  t.eq((await toolRow(p)).hooks.every(Boolean), true, "query B: the tool row");
  t.eq(await p.count(tid("sql-input")), 1, "one area at a time");
  await p.select(await byLabel(p, "Source"), "pgdown");
  await sleep(300);
  const b = await behaviour(p, HASH_TEXT);
  t.assert(/^Format needs a complete text: line 1, column 12/.test(b.status), `query B on a PostgreSQL source has the dialect of PostgreSQL: ${b.status}`);
  await p.click(tid("query-chip-A"));
  await sleep(300);
  t.eq((await state(p)).value, HASH_TEXT, "query A keeps its own text");
  const a = await behaviour(p, HASH_TEXT);
  t.eq([a.markers, a.status], [chHash.markers, chHash.status], "query A on the ClickHouse source has the dialect of ClickHouse");
  t.eq((await toolRow(p)).hooks.every(Boolean), true, "query A: the tool row");
  await p.click(tid("query-chip-B"));
  await sleep(300);
  t.eq((await state(p)).value.replace(/\s+/g, ""), HASH_TEXT.replace(/\s+/g, ""), "query B keeps its own text");
  const b2 = await behaviour(p, DOLLAR_TEXT);
  t.eq([b2.markers, b2.status], [["Fold lines 3 to 5"], "Formatted."], "query B: PostgreSQL after a change of chip");

  // The SQL of a variable and the SQL of an annotation have the same area.
  await backToDashboard(p);
  await moreItem(p, "dash-settings");
  await p.click(p.tid("settings-tab-variables"));
  await p.waitSel(p.tid("variable-0-open"));
  await p.click(p.tid("variable-0-open"));
  await p.waitSel(`${tid("settings-variables")} ${SQL}`);
  const varRow = await toolRow(p);
  t.eq([varRow.hooks.every(Boolean), varRow.describedby, varRow.keyshortcuts], [true, true, true], "the SQL of a variable: the tool row");
  t.eq(await p.eval(`document.querySelector('${SQL}').getAttribute("aria-label")`), "Variable SQL", "the area of the variable");
  const v = await behaviour(p, "SELECT node FROM (\n  SELECT 1 AS node\n) AS x");
  t.eq([v.markers, v.status], [["Fold lines 1 to 3"], "Formatted."], "the SQL of a variable folds and formats");
  await p.click(p.tid("variable-back"));
  await p.click(p.tid("settings-tab-annotations"));
  await p.waitSel(p.tid("annotation-add"));
  await p.click(p.tid("annotation-add"));
  await p.waitSel(`${tid("settings-annotations")} ${SQL}`);
  const annRow = await toolRow(p);
  t.eq([annRow.hooks.every(Boolean), annRow.describedby, annRow.keyshortcuts], [true, true, true], "the SQL of an annotation: the tool row");
  t.eq(await p.eval(`document.querySelector('${SQL}').getAttribute("aria-label")`), "Annotation SQL", "the area of the annotation");
  const n = await behaviour(p, "SELECT now() AS time,\n  (\n    SELECT 'x'\n  ) AS title");
  t.eq([n.markers, n.status], [["Fold lines 2 to 4"], "Formatted."], "the SQL of an annotation folds and formats");
}

// ---------------------------------------------------------------------------------------------------------------------

export const cases = [
  { id: "B177", name: "F42 the kinds of source: the panel editor of ClickHouse, PostgreSQL and MongoDB sources, a panel with two queries, the SQL of a variable and of an annotation each have the tool row; `#`, `$$`, `x::Int`, a lambda and `$facet` fold and format under the dialect of the source, and a change of the source changes it", timeout: 200000, run: runB177 },
  { id: "B176", name: "F42 the time and the budget: parse 50 ms and format 300 ms for 262,144 bytes, the folded view 100 ms for 262,144 bytes, a key under 100 ms in a text of 262,144 bytes, the work of F42 and of the colour layer at a key, no file of editor/ast/ on the view page", timeout: 300000, run: runB176 },
  { id: "B175", name: "F42 the access (scripted checks, no axe-core offline): the text area is the only accessible text, folded too, names and tabindex of the markers, the status messages in order, Tab, Escape then Tab, Ctrl+Enter, the focus, no animation, in both themes", timeout: 200000, run: runB175 },
  { id: "B174", name: "F42 the caps and the hostile texts: 262,144 and 262,145 bytes (ASCII, 2, 3 and 4-byte characters), 128 and 129 levels, the nodes, 2,000 and 2,001 blocks, the colours stay, the save and the run send the same bytes, no exception", timeout: 400000, run: runB174 },
  { id: "B173", name: "F42 the band of the block and the pair: the lines of the innermost block, it moves with the caret, both brackets of a pair, the mark of a lone bracket, Ctrl+Shift+\\, no glyph moves at 1, 0.9, 1.1 and 1.25, the contrast, in both themes", run: runB173 },
  { id: "B172", name: "F42 the format: the button and the key on ClickHouse, PostgreSQL and MongoDB texts of 20, 500 and 501 lines, one undo and redo step or the own undo with its keys and button, Nothing to change, the caret, a selection, the refusals with line and column, the draft and the save", run: runB172 },
  { id: "B171", name: "F42 the folded view and the edit: a letter, Backspace, Enter, a paste, an IME composition and a dead key end the view and land at the mapped caret, one Ctrl+Z, a drag over two chips, Ctrl+A and Ctrl+C, the arrow keys, Run and Ctrl+Enter, a click on a chip", run: runB171 },
  { id: "B170", name: "F42 the folds: markers with a name and aria-expanded, a click and Ctrl+Shift+[ fold and unfold, the text area does not change, rows of 20 px, real line numbers, the error of a hidden line, Ctrl+Shift+]", run: runB170 },
];
