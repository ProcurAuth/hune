// The SQL text area (10.3): a <textarea> in a monospace font, with line numbers and the colours of the copied app_highlight.js.
// The colour layer draws only the rows near the visible rows (sql_colour.js): a key costs the same for a text of 262,144 bytes as for a short text.
// F42 adds the minimal syntax tree (10.3c to 10.3g): a format on request, folds, a bracket pair and the current block.
// The tree is never a security check. A fault of the tree never stops a save, a run, a preview or a change of the draft.
// The text area keeps the text and the focus for a fold: the folded view is a visual layer (sql_fold_view.js).

import { clear, el } from "../ui.js";
import { MAX_AST_DEPTH, MAX_AST_INPUT_BYTES, MAX_AST_NODES, MAX_FOLD_MARKERS } from "./ast/nodes.js";
import { formatJson, formatSql } from "./ast/format.js";
import { parseJson } from "./ast/json_tree.js";
import { parseSql } from "./ast/sql_tree.js";
import "./ast/tokens_pg.js";
import { blockAt, bracketPartner, folds, foldedRows, nonBlankBefore, offsetAfterNonBlank, outerFolds } from "./ast/structure.js";
import { createColourLayer } from "./sql_colour.js";
import { ROW, createFoldView } from "./sql_fold_view.js";
import { indexOfPosition, linesToSql, sqlToLines } from "./sql_text.js";

/** The longest result that the editor writes with `execCommand` (one undo step of the browser). Above it, the write is own (10.3d). */
export const NATIVE_FORMAT_LINES = 500;

/** The delay of the parse after the last `input`, the time-out of the idle call, the delay of the caret update and the life of a message (10.3c, 10.3f, 10.3g). */
const PARSE_DELAY_MS = 150;
const IDLE_TIMEOUT_MS = 500;
const CARET_DELAY_MS = 50;
const MESSAGE_MS = 6000;

/** The gutter draws the rows that are visible and these rows above and below, the least number of visible rows, and the margin that draws the rows again. */
const GUTTER_BUFFER = 100;
const GUTTER_VISIBLE = 40;
const GUTTER_MARGIN = 30;

/** The keys that end the folded view and then move the caret natively (10.3e). */
const MOVE_KEYS = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End", "PageUp", "PageDown"]);

/** The messages of the status line (10.3g). */
const NUMBER = (n) => n.toLocaleString("en-US");
const CAP_NOTICES = {
  too_large: () => `Format and folds are off: the text has more than ${NUMBER(MAX_AST_INPUT_BYTES)} bytes.`,
  too_deep: () => `Format and folds are off: the text goes deeper than ${MAX_AST_DEPTH} levels.`,
  too_many_nodes: () => `Format and folds are off: the text has more than ${NUMBER(MAX_AST_NODES)} nodes.`,
};
const FAULT_NOTICE = "Format and folds stopped: an error occurred in the structure code. The text is not changed.";
const KEYS_TEXT = "Keys: Ctrl+Shift+F formats the text (Cmd+Shift+F on macOS). Ctrl+Shift+[ folds or unfolds the block at the caret. Ctrl+Shift+] unfolds all. Ctrl+Shift+\\ jumps to the partner of a bracket.";

let counter = 0;

// options: { lines, label, onChange(lines) (300 ms after the last key), onRun() (optional: the SQL of a variable has no "Run"),
//   dialect: "clickhouse" | "postgres" | "mongodb", or a function that gives it (10.3c; read at each parse, so a change of the source works),
//   variables: the names of the variables of the document, or a function that gives them (the JSON reader needs them),
//   tools: nodes (buttons) that the tool row shows after "Unfold all" (10.3g) }
export function createSqlEditor({ lines, label = "SQL", onChange, onRun, dialect = "clickhouse", variables = [], tools = [] }) {
  counter += 1;
  const keysId = `sql-keys-${counter}`;
  // The gutter is a frame with a fixed height. The numbers are in `nums`, which moves with the scroll position of the text area.
  const nums = el("div", { class: "sql__nums", "aria-hidden": "true" });
  const gutterGapTop = el("div");
  const gutterGapBottom = el("div");
  let gutterRows = null;
  let gutterDigits = 0;
  // The scroll positions and the height that a fold read before it changed the page. A read after a change makes the browser lay out the page.
  let known = null;
  // The scroll position and the height of the text area, read in its scroll event (the layout is clean then). A key reads them from here:
  // a read of the scroll position after a change of the text makes the browser lay out the whole text area at once.
  const scroll = { top: 0, left: 0, height: 0 };
  // The scroll position that the colour layer has: a new value is written only when it differs, because a write makes the browser lay out the page.
  const preScroll = { top: 0, left: 0 };
  const code = el("code");
  const colourGapTop = el("div");
  const colourGapBottom = el("div");
  const colour = createColourLayer({ code, gapTop: colourGapTop, gapBottom: colourGapBottom });
  const pre = el("pre", { class: "sql__hl", "aria-hidden": "true" }, colourGapTop, code, colourGapBottom);
  // A change of the gaps above the visible rows must not move the scroll position of the colour layer.
  pre.style.overflowAnchor = "none";
  // The attribute spellcheck is a text ("false"), not a flag: el() skips a value that is false, and the native spell check would then
  // draw a red wavy line under each word that is not in the dictionary, for example the macro $__timeFilter (10.3).
  const input = el("textarea", {
    class: "sql__input",
    spellcheck: "false",
    autocorrect: "off",
    wrap: "off",
    "aria-label": label,
    autocapitalize: "off",
    autocomplete: "off",
    "aria-keyshortcuts": "Control+Shift+F Control+Shift+BracketLeft Control+Shift+BracketRight Control+Shift+Backslash",
    "aria-describedby": keysId,
    dataset: { testid: "sql-input" },
  });
  const view = createFoldView({ code, colour, input, onToggle: (key) => toggleFold(key) });
  // A new window of the colour layer has no mark of a bracket pair: the pair is drawn again.
  colour.onDraw = () => view.redrawPair();
  const gutter = el("div", { class: "sql__gutter" }, view.marks, nums);
  const area = el("div", { class: "sql", dataset: { testid: "sql-editor" } }, gutter, el("div", { class: "sql__body" }, view.band, pre, input, view.folded));
  // The tool row (10.3g): the buttons, the status line (live) and the text of the block (not live).
  const formatButton = el("button", { type: "button", class: "btn btn--sm", dataset: { testid: "sql-format" }, on: { click: () => runFormat(), mousedown: (event) => event.preventDefault() } }, "Format");
  const unfoldButton = el("button", { type: "button", class: "btn btn--sm", dataset: { testid: "sql-unfold-all" }, on: { click: () => unfoldAll(), mousedown: (event) => event.preventDefault() } }, "Unfold all");
  const undoButton = el("button", { type: "button", class: "btn btn--sm", hidden: true, dataset: { testid: "sql-undo-format" }, on: { click: () => ownStep(true), mousedown: (event) => event.preventDefault() } }, "Undo format");
  const status = el("p", { class: "sql__status", "aria-live": "polite", dataset: { testid: "sql-status" } });
  const info = el("span", { class: "sql__info", dataset: { testid: "sql-info" } });
  // The status line and the text of the block are in an absolute box with a fixed height: a change of their text makes the browser lay out only this box.
  // A change of text in the flow of the page makes it lay out the text area again (about 140 ms for 262,144 bytes). "Unfold all" is the last button and shows
  // with the style `visibility`, for the same reason: it keeps its place when it is not shown.
  const live = el("div", { class: "sql__live" }, status, info);
  const toolsRow = el("div", { class: "sql__tools" }, formatButton, ...tools, undoButton, unfoldButton, live);
  setUnfoldShown(false);
  const keys = el("p", { class: "sql__keys", id: keysId, dataset: { testid: "sql-keys" }, text: KEYS_TEXT });
  const root = el("div", { class: "sqlbox" }, area, toolsRow, keys);

  let errorLines = new Set();
  let timer = 0;
  let escaped = false;
  // The tree: a result for one version of the text and one dialect (10.3c). `version` grows at each change of the text.
  let version = 0;
  let tree = null;
  let foldList = { items: [], total: 0, truncated: false };
  let parseTimer = 0;
  let idleHandle = 0;
  let caretTimer = 0;
  // The fold state: the offsets of the open brackets that are folded. It is valid for one version of the text only (10.3e).
  const foldedKeys = new Set();
  let layout = null;
  // The write of the format: a flag so that `input` does not repaint for each event, and the own undo entry above 500 lines.
  let writing = false;
  let ownEntry = null;
  let message = "";
  let messageTimer = 0;
  let standing = "";
  let tracking = false;
  let destroyed = false;
  // True while the last paint of the colours failed. A tree that installs later keeps the notice of the fault.
  let paintFailed = false;

  // --- the dialect -------------------------------------------------------------------------------------------------

  function currentDialect() {
    const d = typeof dialect === "function" ? dialect() : dialect;
    return d === "postgres" || d === "mongodb" ? d : "clickhouse";
  }

  function variableNames() {
    const v = typeof variables === "function" ? variables() : variables;
    return Array.isArray(v) ? v : [];
  }

  function parse(text, d) {
    return d === "mongodb" ? parseJson(text, { variables: variableNames() }) : parseSql(text, d);
  }

  // --- the status line ---------------------------------------------------------------------------------------------

  function showStatus() {
    const text = message || standing;
    if (status.textContent !== text) status.textContent = text;
  }

  // A message that is not a cap goes away after 6 s, or at the next message (10.3g).
  function say(text) {
    clearTimeout(messageTimer);
    message = text;
    messageTimer = setTimeout(() => {
      message = "";
      showStatus();
    }, MESSAGE_MS);
    showStatus();
  }

  // A notice of a cap or of a fault stays while it holds.
  function setStanding(text) {
    standing = text;
    showStatus();
  }

  function setUnfoldShown(shown) {
    unfoldButton.style.visibility = shown ? "visible" : "hidden";
  }

  function syncUndoButton() {
    undoButton.hidden = !(ownEntry && !ownEntry.undone);
  }

  // --- the colours and the gutter ----------------------------------------------------------------------------------

  // The colour layer draws the rows near the visible rows only (sql_colour.js). The scroll position is the one of the last scroll event.
  function paintCode(text) {
    colour.setText(text);
    colour.draw(scroll.top, scroll.height);
  }

  // The number of lines of a text (the text area has no CR). It does not build the list of lines.
  function countLines(text) {
    let n = 1;
    for (let i = text.indexOf("\n"); i >= 0; i = text.indexOf("\n", i + 1)) n += 1;
    return n;
  }

  // The gutter shows the real line numbers of the rows. While folded, the numbers jump over the hidden lines. The mark of an
  // error on a hidden line goes to the row that hides it (10.3e). Only the rows near the visible rows are drawn, with a gap above
  // and below, so a text of 262,144 bytes costs no more than a small text.
  function gutterTop() {
    if (known) return known.top;
    return view.shown ? view.folded.scrollTop : scroll.top;
  }

  function gutterVisible() {
    const height = known ? known.height : view.shown ? view.folded.clientHeight : scroll.height;
    return Math.max(GUTTER_VISIBLE, Math.ceil((height || 0) / ROW));
  }

  function renderGutter() {
    const count = countLines(input.value);
    const rowLines = layout ? layout.rowLines : null;
    const rows = rowLines ? rowLines.length : count;
    const first = Math.floor(gutterTop() / ROW);
    const from = Math.max(0, Math.min(rows - 1, first - GUTTER_BUFFER));
    const to = Math.max(from + 1, Math.min(rows, first + gutterVisible() + GUTTER_BUFFER));
    // The width of the gutter: the marker column (24 px), the digits of the largest line number, the room at the right (8 px) and the border (1 px), at least 80 px.
    const digits = String(count).length;
    if (digits !== gutterDigits) {
      gutterDigits = digits;
      area.style.setProperty("--sql-gutter-w", `max(80px, calc(33px + ${digits}ch))`);
    }
    const marked = new Set();
    for (const n of errorLines) {
      const at = Math.max(0, Math.min(count - 1, n - 1));
      marked.add(layout ? layout.rowOfLine[at] : at);
    }
    clear(nums);
    const frag = document.createDocumentFragment();
    gutterGapTop.style.height = `${from * ROW}px`;
    frag.append(gutterGapTop);
    for (let r = from; r < to; r++) {
      const line = rowLines ? rowLines[r] : r;
      frag.append(el("span", { class: marked.has(r) ? "sql__no sql__no--error" : "sql__no", text: String(line + 1) }));
    }
    gutterGapBottom.style.height = `${Math.max(0, rows - to) * ROW}px`;
    frag.append(gutterGapBottom);
    nums.append(frag);
    gutterRows = { from, to, rows };
    if (foldList.items.length) drawMarkers();
    sync();
  }

  // A scroll that comes near the border of the drawn rows draws the rows again.
  function gutterNeedsRows() {
    if (!gutterRows) return false;
    const first = Math.floor(gutterTop() / ROW);
    const early = gutterRows.from > 0 && first < gutterRows.from + GUTTER_MARGIN;
    const late = gutterRows.to < gutterRows.rows && first + gutterVisible() > gutterRows.to - GUTTER_MARGIN;
    return early || late;
  }

  // The colours can fail (a fault of the highlighter): the text then stays plain, the standing notice shows, and nothing else stops.
  // The gutter is an own step, so that a fault of the colours does not hide the line numbers (10.3c rule 1).
  function paint() {
    const text = input.value;
    try {
      paintCode(text);
      paintFailed = false;
    } catch {
      paintFailed = true;
      try {
        colour.draw(scroll.top, scroll.height, true);
      } catch {
        code.textContent = `${text}\n`;
      }
      setStanding(FAULT_NOTICE);
    }
    renderGutter();
  }

  // Draws the window of the colour layer again when the visible rows come near its border. A fault shows plain text and the notice.
  function ensureColours() {
    try {
      colour.ensure(scroll.top, scroll.height);
    } catch {
      paintFailed = true;
      try {
        colour.draw(scroll.top, scroll.height, true);
      } catch {
        // The text of the text area stays: the colour layer is only a view.
      }
      setStanding(FAULT_NOTICE);
    }
  }

  // `force` writes the scroll position of the colour layer: the scroll event of the text area and the folded view do it.
  function sync(force = false) {
    const folded = view.shown;
    const top = known ? known.top : folded ? view.folded.scrollTop : scroll.top;
    if (gutterNeedsRows()) {
      renderGutter();
      return;
    }
    ensureColours();
    if (!folded && (force || preScroll.top !== scroll.top || preScroll.left !== scroll.left)) {
      pre.scrollTop = scroll.top;
      pre.scrollLeft = scroll.left;
      preScroll.top = scroll.top;
      preScroll.left = scroll.left;
    }
    nums.style.transform = `translateY(${-top}px)`;
    view.scrollMarks(top);
    view.scrollBand(known ? known.inputTop : scroll.top);
  }

  // Reads the scroll position of the text area. It is called in the scroll event and where the editor sets the scroll position itself.
  function readScroll() {
    scroll.top = input.scrollTop;
    scroll.left = input.scrollLeft;
    scroll.height = input.clientHeight;
  }

  // --- the tree ----------------------------------------------------------------------------------------------------

  function cancelParse() {
    clearTimeout(parseTimer);
    parseTimer = 0;
    if (idleHandle && typeof cancelIdleCallback === "function") cancelIdleCallback(idleHandle);
    idleHandle = 0;
  }

  // The parse runs once, after the last input, when the browser is idle. It never runs for each key (10.3c).
  function scheduleParse() {
    cancelParse();
    parseTimer = setTimeout(() => {
      parseTimer = 0;
      if (typeof requestIdleCallback === "function") idleHandle = requestIdleCallback(runParse, { timeout: IDLE_TIMEOUT_MS });
      else parseTimer = setTimeout(runParse, 0);
    }, PARSE_DELAY_MS);
  }

  // Builds the tree for the text now. A fault of the tree code gives a tree with `fault` and the notice, never an exception.
  function buildTree() {
    const text = input.value;
    const d = currentDialect();
    try {
      const result = parse(text, d);
      const list = result.capped ? { items: [], total: 0, truncated: false } : folds(result, text);
      return { version, dialect: d, text, result, folds: list, fault: false };
    } catch {
      return { version, dialect: d, text, result: null, folds: { items: [], total: 0, truncated: false }, fault: true };
    }
  }

  function current() {
    return tree && tree.version === version && tree.dialect === currentDialect() ? tree : null;
  }

  // The tree for the text and the dialect now: the one that exists, else a new one. A key that needs the tree calls it.
  function freshTree() {
    const have = current();
    if (have) return have;
    cancelParse();
    install(buildTree());
    return tree;
  }

  function runParse() {
    idleHandle = 0;
    if (destroyed) return;
    const asked = version;
    const built = buildTree();
    // A result is valid for one version of the text. A result of an old version is dropped (10.3c).
    if (built.version !== asked || asked !== version) return;
    install(built);
  }

  function install(built) {
    tree = built;
    foldList = built.folds;
    if (built.fault || paintFailed) setStanding(FAULT_NOTICE);
    else if (built.result.capped) setStanding((CAP_NOTICES[built.result.capped.reason] || CAP_NOTICES.too_large)());
    else if (built.folds.truncated) setStanding(`Only the first ${NUMBER(MAX_FOLD_MARKERS)} blocks can fold.`);
    else setStanding("");
    drawMarkers();
    updateCaret();
  }

  // --- the markers and the folded view ------------------------------------------------------------------------------

  function hiddenInFold(item) {
    return layout !== null && layout.outer.some((o) => item.open > o.open && item.open < o.close);
  }

  // The markers of the rows that the gutter draws only (a text of 262,144 bytes has up to 2,000 markers).
  function drawMarkers() {
    view.drawMarkers(foldList.items, foldedKeys, layout ? layout.rowOfLine : null, layout ? hiddenInFold : null, gutterRows);
    setUnfoldShown(foldedKeys.size !== 0);
  }

  // Builds the folded view for the keys that are folded. The caret goes out of a hidden range, to the offset after its open bracket.
  function showFolded(text, geometry) {
    const outer = outerFolds(foldList.items, foldedKeys);
    const rows = foldedRows(text, outer);
    const wasFolded = view.shown;
    layout = { outer, rowLines: rows.rowLines, rowOfLine: rows.rowOfLine };
    // A new view starts at the line that the text area shows at its top. A view that shows already keeps its scroll position.
    const topLine = Math.floor(geometry.inputTop / ROW);
    const top = wasFolded ? geometry.foldedTop : rows.rowOfLine[Math.min(topLine, rows.rowOfLine.length - 1)] * ROW + (geometry.inputTop % ROW);
    const left = wasFolded ? geometry.foldedLeft : geometry.inputLeft;
    known = { top, inputTop: geometry.inputTop, height: geometry.height };
    try {
      // The marks of the pair and the band go first: the folded view copies the coloured text of the colour layer.
      clearCaretMarks();
      view.show(text, outer, rows.rowLines, top, geometry.height);
      area.classList.add("sql--folded");
      renderGutter();
      drawMarkers();
      sync();
      moveCaretOut(outer);
      view.paintSelection(input.selectionStart, input.selectionEnd);
    } finally {
      known = null;
    }
    // The last step: the scroll position makes the browser lay out the page once, for all the changes above.
    view.setScroll(top, left);
  }

  function moveCaretOut(outer) {
    const hidden = (p) => outer.find((f) => f.from < p && p < f.to);
    const a = input.selectionStart;
    const b = input.selectionEnd;
    const ha = hidden(a);
    const hb = hidden(b);
    if (!ha && !hb) return;
    const start = ha ? ha.from : a;
    const end = hb ? hb.from : b;
    input.setSelectionRange(Math.min(start, end), Math.max(start, end));
  }

  // Ends the folded view. With `clearState` the fold state goes too (an edit, a new text); a fold key that opens the last fold passes true as well.
  function leaveFolded() {
    if (!view.shown && foldedKeys.size === 0) return;
    const wasShown = view.shown;
    const rows = layout;
    const top = view.folded.scrollTop;
    const left = view.folded.scrollLeft;
    foldedKeys.clear();
    layout = null;
    if (wasShown) {
      view.hide();
      area.classList.remove("sql--folded");
      // The text area goes back to the line that the folded view showed at its top.
      if (rows) {
        const row = Math.min(rows.rowLines.length - 1, Math.floor(top / ROW));
        input.scrollTop = rows.rowLines[Math.max(0, row)] * ROW + (top % ROW);
        input.scrollLeft = left;
        readScroll();
      }
    }
    renderGutter();
    drawMarkers();
    if (wasShown) scheduleCaret();
  }

  // The positions and the height that the fold needs, read before the page changes (a read after a change makes the browser lay out the page).
  function readGeometry() {
    const folded = view.shown;
    return {
      inputTop: input.scrollTop,
      inputLeft: input.scrollLeft,
      foldedTop: folded ? view.folded.scrollTop : 0,
      foldedLeft: folded ? view.folded.scrollLeft : 0,
      height: folded ? view.folded.clientHeight : input.clientHeight,
    };
  }

  function toggleFold(key) {
    const geometry = readGeometry();
    const t = freshTree();
    if (!t || t.fault || t.result.capped) return;
    const item = foldList.items.find((f) => f.key === key);
    if (!item) return;
    try {
      if (foldedKeys.has(key)) {
        foldedKeys.delete(key);
        say("Unfolded 1 block.");
      } else {
        foldedKeys.add(key);
        say(`Folded 1 block, ${item.lines} lines.`);
      }
      if (foldedKeys.size === 0) leaveFolded();
      else showFolded(input.value, geometry);
    } catch {
      fault();
    }
  }

  // The key Ctrl+Shift+[: folds the innermost node at the caret, or unfolds it when it is folded (10.3g).
  function foldAtCaret() {
    const t = freshTree();
    if (!t || t.fault || t.result.capped) return;
    const caret = input.selectionStart;
    let best = null;
    for (const f of foldList.items) {
      if (f.from <= caret && caret <= f.to && (!best || f.to - f.from <= best.to - best.from)) best = f;
    }
    if (!best) {
      say("Nothing to fold here.");
      return;
    }
    toggleFold(best.key);
  }

  function unfoldAll() {
    if (foldedKeys.size === 0) {
      say("Nothing is folded.");
      return;
    }
    leaveFolded();
    say("Unfolded all.");
  }

  // The key Ctrl+Shift+\: the caret goes to the partner of the bracket: after a close bracket, or before an open bracket.
  function jumpToPartner() {
    const t = freshTree();
    if (!t || t.fault || t.result.capped) return;
    leaveFolded();
    try {
      const pair = bracketPartner(t.result, input.selectionStart);
      if (!pair) {
        say("No bracket at the caret.");
        return;
      }
      if (!pair.matched) {
        say("This bracket has no match.");
        return;
      }
      const at = pair.side === "open" ? pair.partner + 1 : pair.partner;
      input.setSelectionRange(at, at);
      scheduleCaret();
    } catch {
      fault();
    }
  }

  // --- the current block and the bracket pair (10.3f) --------------------------------------------------------------

  function clearCaretMarks() {
    view.setBand(null, 0);
    view.clearPair();
    info.textContent = "";
  }

  function scheduleCaret() {
    clearTimeout(caretTimer);
    caretTimer = setTimeout(() => {
      caretTimer = 0;
      if (view.shown) view.paintSelection(input.selectionStart, input.selectionEnd);
      else updateCaret();
    }, CARET_DELAY_MS);
  }

  // The band and the pair show only with a collapsed caret, a tree that is valid for the text, no fold and no cap.
  function updateCaret() {
    clearCaretMarks();
    const t = current();
    if (!t || t.fault || t.result.capped || view.shown || destroyed) return;
    if (document.activeElement !== input || input.selectionStart !== input.selectionEnd) return;
    try {
      const caret = input.selectionStart;
      const parts = [];
      const block = blockAt(t.result, caret, t.text);
      view.setBand(block, input.scrollTop);
      if (block) parts.push(`Block: ${block.name ? `${block.type} ${block.name}` : block.type}, lines ${block.line + 1} to ${block.endLine + 1}.`);
      const pair = bracketPartner(t.result, caret);
      view.setPair(pair);
      if (pair && !pair.matched) parts.push("This bracket has no match.");
      info.textContent = parts.join(" ");
    } catch {
      clearCaretMarks();
      fault();
    }
  }

  // The tree code threw: the structure tools stop for this version, with a notice. The text, the save and the run are not touched.
  function fault() {
    cancelParse();
    tree = { version, dialect: currentDialect(), text: input.value, result: null, folds: { items: [], total: 0, truncated: false }, fault: true };
    foldList = tree.folds;
    foldedKeys.clear();
    layout = null;
    view.hide();
    area.classList.remove("sql--folded");
    view.clearMarkers();
    setUnfoldShown(false);
    renderGutter();
    setStanding(FAULT_NOTICE);
  }

  // --- the format (10.3d) ------------------------------------------------------------------------------------------

  function refusal(res, d) {
    const reason = res.reason || "";
    if (reason.startsWith("capped_")) return (CAP_NOTICES[reason.slice(7)] || CAP_NOTICES.too_large)();
    if (reason === "nothing_to_format" || reason === "no_statement") return "Nothing to format.";
    if (reason === "error_node" || reason === "unclosed") {
      const why = reason === "unclosed" ? "a string, a name or a comment has no end" : d === "mongodb" ? "the text is not valid JSON" : "a bracket has no match";
      return `Format needs a complete text: line ${res.line}, column ${res.column}: ${why}.`;
    }
    if (reason === "output_too_large") return `Format stopped: the result has more than ${NUMBER(MAX_AST_INPUT_BYTES)} bytes. The text is not changed.`;
    return "Format stopped: the check of the result failed. The text is not changed.";
  }

  function runFormat() {
    if (destroyed) return;
    const text = input.value;
    if (text.trim() === "") {
      say("Nothing to format.");
      return;
    }
    leaveFolded();
    const d = currentDialect();
    const a = input.selectionStart;
    const b = input.selectionEnd;
    let res;
    try {
      res = d === "mongodb" ? formatJson(text) : formatSql(text, d, a !== b ? { range: [a, b] } : null);
    } catch {
      fault();
      return;
    }
    if (res.reason) {
      say(refusal(res, d));
      if ((res.reason === "error_node" || res.reason === "unclosed") && res.at >= 0 && res.at <= text.length) {
        input.focus({ preventScroll: true });
        input.setSelectionRange(res.at, res.at);
      }
      return;
    }
    if (!res.changed || res.text === text) {
      say("Nothing to change.");
      return;
    }
    const rep = res.replace || { from: 0, to: text.length, text: res.text };
    const focusEnd = input.selectionDirection === "backward" ? a : b;
    const caret = offsetAfterNonBlank(res.text, nonBlankBefore(text, focusEnd));
    const own = writeFormat(text, res.text, rep, caret, [a, b]);
    say(own ? "Formatted. The undo history of the browser is cleared." : "Formatted.");
  }

  // Puts the result in the text area. Up to 500 lines: execCommand (one undo step). Above, or when it fails: setRangeText and an own
  // one-step undo. The flag `writing` stops the handler of `input` for each event, and the repaint happens once after the write.
  // It returns true when the own undo is used.
  function writeFormat(text, next, rep, caret, selection) {
    const top = input.scrollTop;
    const left = input.scrollLeft;
    const rows = rep.text === "" ? 0 : rep.text.split("\n").length;
    let own = rows > NATIVE_FORMAT_LINES;
    writing = true;
    try {
      input.focus({ preventScroll: true });
      if (!own) {
        input.setSelectionRange(rep.from, rep.to);
        let ok = false;
        try {
          ok = rep.text === "" ? document.execCommand("delete") : document.execCommand("insertText", false, rep.text);
        } catch {
          ok = false;
        }
        if (!ok || input.value !== next) {
          own = true;
          if (input.value !== text) input.value = text;
        }
      }
      if (own) {
        input.setRangeText(rep.text, rep.from, rep.to, "end");
        ownEntry = { from: rep.from, before: text.slice(rep.from, rep.to), after: rep.text, caretBefore: selection, caretAfter: [caret, caret], undone: false };
      } else ownEntry = null;
      input.setSelectionRange(caret, caret);
      input.scrollTop = top;
      input.scrollLeft = left;
      readScroll();
    } finally {
      writing = false;
    }
    syncUndoButton();
    changed();
    return own;
  }

  // The own undo and redo of the format (10.3d): swaps the region that the format wrote.
  function ownStep(undo) {
    const e = ownEntry;
    if (!e || e.undone === undo) return;
    const written = undo ? e.after : e.before;
    const next = undo ? e.before : e.after;
    const caret = undo ? e.caretBefore : e.caretAfter;
    writing = true;
    try {
      leaveFolded();
      input.focus({ preventScroll: true });
      const top = input.scrollTop;
      input.setRangeText(next, e.from, e.from + written.length, "end");
      input.setSelectionRange(caret[0], caret[1]);
      input.scrollTop = top;
      readScroll();
    } finally {
      writing = false;
    }
    e.undone = undo;
    syncUndoButton();
    changed();
    say(undo ? "Format undone." : "Format applied again.");
  }

  // --- the changes of the text ---------------------------------------------------------------------------------------

  // A change of the text: the tree is old, the markers, the band and the pair go at once, and the parse is scheduled (10.3c).
  // The page gets `onChange` 300 ms after the last change. A fold state ends with the change (10.3e).
  function changed() {
    version += 1;
    if (view.shown || foldedKeys.size) {
      foldedKeys.clear();
      layout = null;
      view.hide();
      area.classList.remove("sql--folded");
    }
    foldList = { items: [], total: 0, truncated: false };
    view.clearMarkers();
    setUnfoldShown(false);
    clearCaretMarks();
    // The change of the draft and the parse come first: a fault of the colours never loses a change (10.3c rule 1).
    clearTimeout(timer);
    timer = setTimeout(() => onChange(sqlToLines(input.value)), 300);
    scheduleParse();
    paint();
  }

  // --- the events ----------------------------------------------------------------------------------------------------

  input.addEventListener("scroll", () => {
    readScroll();
    sync(true);
  });
  view.folded.addEventListener("scroll", () => sync());
  input.addEventListener("input", () => {
    if (writing) return;
    ownEntry = null;
    syncUndoButton();
    changed();
  });
  // An edit in the folded view ends the folded view first: the edit then runs natively at the caret, with one undo step (10.3e).
  input.addEventListener("beforeinput", (event) => {
    if (ownEntry && (event.inputType === "historyUndo" || event.inputType === "historyRedo")) {
      event.preventDefault();
      ownStep(event.inputType === "historyUndo");
      return;
    }
    if (view.shown) leaveFolded();
  });
  input.addEventListener("compositionstart", () => {
    if (view.shown) leaveFolded();
  });

  function isOwnUndoKey(event) {
    if (!(event.ctrlKey || event.metaKey) || event.altKey) return null;
    const k = event.key.toLowerCase();
    if (k === "z") return event.shiftKey ? "redo" : "undo";
    if (k === "y" && event.ctrlKey && !event.metaKey) return "redo";
    return null;
  }

  function guarded(run) {
    try {
      run();
    } catch {
      fault();
    }
  }

  input.addEventListener("keydown", (event) => {
    if (ownEntry) {
      const step = isOwnUndoKey(event);
      if (step) {
        event.preventDefault();
        ownStep(step === "undo");
        return;
      }
    }
    // The new keys use event.code, so the layout of the keyboard does not change them (10.3g).
    if (event.shiftKey && !event.altKey) {
      const ctrlOnly = event.ctrlKey && !event.metaKey;
      if (event.code === "KeyF" && (ctrlOnly || (event.metaKey && !event.ctrlKey))) {
        event.preventDefault();
        if (!event.repeat) guarded(runFormat);
        return;
      }
      if (ctrlOnly && event.code === "BracketLeft") {
        event.preventDefault();
        guarded(foldAtCaret);
        return;
      }
      if (ctrlOnly && event.code === "BracketRight") {
        event.preventDefault();
        guarded(unfoldAll);
        return;
      }
      if (ctrlOnly && event.code === "Backslash") {
        event.preventDefault();
        guarded(jumpToPartner);
        return;
      }
    }
    if (view.shown && MOVE_KEYS.has(event.key)) leaveFolded();
    if (event.key === "Tab" && !event.shiftKey && !escaped) {
      event.preventDefault();
      leaveFolded();
      input.setRangeText("  ", input.selectionStart, input.selectionEnd, "end");
      input.dispatchEvent(new Event("input"));
      return;
    }
    // Escape and then Tab moves the focus out of the text area (10.3).
    escaped = event.key === "Escape";
    if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
      event.preventDefault();
      if (onRun) onRun();
    }
  });

  // A pointer in the folded view: the text area has no pointer events then. The point maps to an offset, the selection of the text
  // area is set, and the focus stays in the text area (10.3e). A press on a chip is a click that toggles its fold.
  let anchor = null;
  view.folded.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || (event.target instanceof Element && event.target.closest(".sql__chip"))) return;
    event.preventDefault();
    const at = view.offsetFromPoint(event.clientX, event.clientY);
    if (at === null) return;
    input.focus({ preventScroll: true });
    if (event.shiftKey) anchor = input.selectionDirection === "backward" ? input.selectionEnd : input.selectionStart;
    else anchor = at;
    input.setSelectionRange(Math.min(anchor, at), Math.max(anchor, at), at < anchor ? "backward" : "forward");
    try {
      view.folded.setPointerCapture(event.pointerId);
    } catch {
      // A pointer that is gone cannot be captured: the move then stops with the press.
    }
    view.paintSelection(input.selectionStart, input.selectionEnd);
  });
  // A press in the folded view must not start a native selection or move the focus: the text area keeps both. A click still fires.
  view.folded.addEventListener("mousedown", (event) => event.preventDefault());
  view.folded.addEventListener("pointermove", (event) => {
    if (anchor === null) return;
    const at = view.offsetFromPoint(event.clientX, event.clientY);
    if (at === null) return;
    input.setSelectionRange(Math.min(anchor, at), Math.max(anchor, at), at < anchor ? "backward" : "forward");
    view.paintSelection(input.selectionStart, input.selectionEnd);
  });
  const endPointer = () => {
    anchor = null;
  };
  view.folded.addEventListener("pointerup", endPointer);
  view.folded.addEventListener("pointercancel", endPointer);

  // The caret and the selection: the band and the pair follow them 50 ms later (10.3f). The listener on the document is on while the text area has the focus.
  const onSelection = () => scheduleCaret();
  input.addEventListener("focus", () => {
    // A change of the source while the area had no focus: the tree is built again for the new dialect.
    if (tree && tree.dialect !== currentDialect()) api.refresh();
    if (!tracking) {
      tracking = true;
      document.addEventListener("selectionchange", onSelection);
    }
    scheduleCaret();
  });
  input.addEventListener("blur", () => {
    escaped = false;
    if (tracking) {
      tracking = false;
      document.removeEventListener("selectionchange", onSelection);
    }
    clearTimeout(caretTimer);
    clearCaretMarks();
  });

  function setLines(next) {
    const text = linesToSql(next);
    ownEntry = null;
    syncUndoButton();
    if (input.value !== text || view.shown || foldedKeys.size) {
      if (input.value !== text) input.value = text;
      if (view.shown || foldedKeys.size) {
        foldedKeys.clear();
        layout = null;
        view.hide();
        area.classList.remove("sql--folded");
      }
    }
    version += 1;
    foldList = { items: [], total: 0, truncated: false };
    view.clearMarkers();
    setUnfoldShown(false);
    clearCaretMarks();
    paint();
    scheduleParse();
  }
  setLines(lines);

  const api = {
    el: root,
    input,
    setLines,
    // Errors with a line (from 1) are marked in the gutter.
    markLines(list) {
      errorLines = new Set(list);
      renderGutter();
    },
    // Puts the cursor on a line and a column in bytes (7.5). It ends the folded view first.
    focusAt(line, byteColumn = 1) {
      leaveFolded();
      const at = indexOfPosition(sqlToLines(input.value), line, byteColumn);
      input.focus();
      input.setSelectionRange(at, at);
    },
    // Adds a button (or any node) to the tool row, after "Unfold all". The "Examples" menu of a variable plugs in here.
    addTool(node) {
      toolsRow.insertBefore(node, undoButton);
    },
    // Tells the editor that the dialect or the variable names changed: the tree is built again.
    refresh() {
      if (current()) return;
      foldedKeys.clear();
      leaveFolded();
      clearCaretMarks();
      scheduleParse();
    },
    flush() {
      clearTimeout(timer);
      onChange(sqlToLines(input.value));
    },
    destroy() {
      destroyed = true;
      clearTimeout(timer);
      clearTimeout(caretTimer);
      clearTimeout(messageTimer);
      cancelParse();
      view.clearSelection();
      if (tracking) document.removeEventListener("selectionchange", onSelection);
      tracking = false;
    },
  };
  return api;
}
