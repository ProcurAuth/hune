// A JSON text area with the look of VS Code, in the same frame as the SQL text area (10.3, 10.4).
// `input` is the real <textarea>: it keeps the text, the selection, the caret and the scroll bars. Its text is transparent, and a layer over it draws
// the colours with the same metrics (the technique of the SQL area, 10.3). The layer draws only the rows near the visible rows.
// The area has the features of the editor of VS Code that fit a JSON text: the colours of the kinds of text, the colours of the brackets by depth,
// the guides of the indent, the band of the line of the caret with its number in the strong colour, the box on the bracket of the caret and on its
// pair, the wavy line under the first syntax error, and the marks of the gutter for the lines that differ from the saved text. It has no folding
// and no minimap.
// After a change of `input.value` from code, call `paint()`; typing repaints by itself.
// `setBase(getText)` turns on the change marks: the gutter marks each line that differs from the text that `getText()` gives (the saved
// document), and `paintMarks()` draws them again. `setError(line, column)` marks a syntax error: the line number in the gutter and the wavy line.

import { el } from "../ui.js";
import { bracketClass, bracketNear, diffLines, enclosingPair, errorRange, indentColumns, indentLength, lineTokens, rowOf, scanBrackets } from "./json_check.js";

/** The height of one row in px. It is the value of `--json-row` of the frame (editor.css, `.jframe`): 14 px text with a line height of 1.43. */
export const ROW = 20;
/** The rows that the layer keeps above and below the visible rows, and the margin to the border of the drawn rows that starts a new window. */
export const BUFFER = 100;
export const MARGIN = 30;
/** The most mark boxes that the gutter draws. A change of more runs than this shows the first ones. */
export const MAX_MARKS = 500;
/** The delay in ms between a key and the new marks. The colours do not wait. */
const MARKS_DELAY = 120;

const KIND_CLASS = { key: "jt-key", str: "jt-str", num: "jt-num", bool: "jt-bool", null: "jt-null", punct: "jt-punct", err: "jt-err" };

export function createJsonArea({ label, testid, tall = false }) {
  // The gutter is a frame with a fixed height. The numbers and the marks are in boxes that move with the scroll position of the text area.
  const nums = el("pre", { class: "jarea__nums" });
  const activeNum = el("div", { class: "jarea__active-num", hidden: true });
  const marks = el("div", { class: "jarea__marks" });
  const gutter = el("div", { class: "jarea__gutter", "aria-hidden": "true" }, nums, activeNum, marks);
  const input = el("textarea", { class: "jarea__input", spellcheck: "false", autocorrect: "off", wrap: "off", "aria-label": label, autocapitalize: "off", autocomplete: "off", dataset: { testid } });
  const code = el("code", { class: "jarea__code" });
  const layer = el("pre", { class: "jarea__hl", "aria-hidden": "true" }, el("div", { class: "jarea__hl-in" }, code));
  // The band of the line of the caret is under the text. The guide and the boxes of the brackets are over it, in a box that scrolls like the text.
  const band = el("div", { class: "jarea__band", hidden: true, "aria-hidden": "true" });
  const guide = el("div", { class: "jarea__guide", hidden: true });
  const matchA = el("div", { class: "jarea__match", hidden: true });
  const matchB = el("div", { class: "jarea__match", hidden: true });
  const deco = el("div", { class: "jarea__deco", "aria-hidden": "true" }, guide, matchA, matchB);
  // The text area is under the layer: its selection shows under the coloured text, as in VS Code. The layer and the deco do not take the pointer.
  const body = el("div", { class: "jarea__body" }, band, input, layer, deco);
  const root = el("div", { class: `jframe${tall ? " jframe--tall" : ""}` }, gutter, body);
  const layerIn = layer.firstChild;
  let count = 0;
  let lines = [];
  let scan = scanBrackets([]);
  let first = 0; // the first drawn row
  let last = 0; // after the last drawn row
  let getBase = null;
  let errorLine = 0;
  let errorColumn = 0;
  let marksTimer = 0;
  let caretFrame = 0;
  let caretRow = -1;

  // The text of the rows [first, last), with a span for each token. The white space at the start of a line is a box with the guides of the indent.
  function drawRows(from, to) {
    const frag = document.createDocumentFragment();
    for (let r = from; r < to; r++) {
      const line = lines[r];
      let at = 0;
      const lead = indentLength(line);
      if (lead > 0 && lead < line.length) {
        frag.append(el("span", { class: "jt-ind", text: line.slice(0, lead) }));
        at = lead;
      }
      const tokens = lineTokens(line);
      if (r === errorLine - 1 && errorColumn > 0) {
        const range = errorRange(line, tokens, errorColumn);
        if (range) {
          const [s, e] = range;
          const inside = tokens.findIndex(([ts, te]) => s >= ts && s < te);
          if (inside >= 0) tokens[inside] = [tokens[inside][0], tokens[inside][1], tokens[inside][2], true];
          else tokens.splice(tokens.findIndex(([ts]) => ts > s) < 0 ? tokens.length : tokens.findIndex(([ts]) => ts > s), 0, [s, e, "err"]);
        }
      }
      let bracket = scan.lineFirst[r];
      for (const [start, end, kind, flagged] of tokens) {
        if (start > at) frag.append(line.slice(at, start));
        let cls = KIND_CLASS[kind];
        if (kind === "punct" && end - start === 1 && "{}[]".includes(line[start])) cls = bracketClass(scan, bracket++);
        frag.append(el("span", { class: flagged ? `${cls} jt-err` : cls, text: line.slice(start, end) }));
        at = end;
      }
      frag.append(r < to - 1 ? `${line.slice(at)}\n` : line.slice(at));
    }
    code.replaceChildren(frag);
    first = from;
    last = to;
  }

  // The scroll position and the height of the text area. A key must not read them from the page: a read after a change of the text makes the browser
  // lay out the whole text area at once (about 30 ms for 6,600 lines). The scroll event and the resize of the area keep these values.
  let top = 0;
  let left = 0;
  let height = 600;
  function readScroll() {
    top = input.scrollTop;
    left = input.scrollLeft;
    height = input.clientHeight || height;
  }

  // Draws the rows again when the visible rows come near the border of the drawn rows, or when `force` says the text changed.
  function drawWindow(force) {
    const topRow = Math.floor(top / ROW);
    const visible = Math.ceil(height / ROW) + 1;
    const edgeOk = (first === 0 || topRow - first >= MARGIN) && (last >= lines.length || last - (topRow + visible) >= MARGIN);
    if (!force && edgeOk && last > first) return;
    drawRows(Math.max(0, topRow - BUFFER), Math.min(lines.length, topRow + visible + BUFFER));
  }

  // Puts the numbers, the marks, the band, the deco and the layer where the text area is. It reads nothing from the page.
  function place() {
    const y = `translateY(${-top}px)`;
    nums.style.transform = y;
    activeNum.style.transform = y;
    marks.style.transform = y;
    band.style.transform = y;
    deco.style.transform = `translate(${-left}px, ${-top}px)`;
    drawWindow(false);
    layerIn.style.transform = `translate(${-left}px, ${first * ROW - top}px)`;
  }

  function paint() {
    lines = input.value.split("\n");
    scan = scanBrackets(lines);
    if (lines.length !== count) {
      count = lines.length;
      nums.textContent = Array.from({ length: count }, (_, i) => String(i + 1)).join("\n");
    }
    drawWindow(true);
    place();
    scheduleMarks();
    queueCaret();
  }

  // A box for a run of lines: the gutter edge in a colour of the kind of change. A deleted run is a small mark on the line border.
  function paintMarks() {
    clearTimeout(marksTimer);
    marksTimer = 0;
    const boxes = [];
    if (getBase) {
      const diff = diffLines(getBase(), input.value);
      const run = (indexes, kind) => {
        let from = 0;
        for (let k = 1; k <= indexes.length; k++) {
          if (k === indexes.length || indexes[k] !== indexes[k - 1] + 1) {
            boxes.push([indexes[from], indexes[k - 1] - indexes[from] + 1, kind]);
            from = k;
          }
        }
      };
      run(diff.added, "added");
      run(diff.modified, "modified");
      for (const at of diff.deleted) boxes.push([at, 0, "deleted"]);
    }
    if (errorLine > 0) boxes.push([errorLine - 1, 1, "error"]);
    marks.replaceChildren(...boxes.slice(0, MAX_MARKS).map(([at, length, kind]) => {
      const box = el("div", { class: `jarea__mark jarea__mark--${kind}`, dataset: { kind, line: String(at + 1), lines: String(length) } });
      box.style.top = `calc(var(--space-8) + ${at * ROW}px)`;
      if (length) box.style.height = `${length * ROW}px`;
      return box;
    }));
  }

  function scheduleMarks() {
    if (!getBase && errorLine === 0) return;
    clearTimeout(marksTimer);
    marksTimer = setTimeout(paintMarks, MARKS_DELAY);
  }

  // Puts a box on the character at an offset. The row and the column are numbers for the css (`--r`, `--c`): a column is one `ch`.
  function putBox(node, offset) {
    const row = rowOf(scan, offset);
    node.style.setProperty("--r", String(row));
    node.style.setProperty("--c", String(offset - scan.lineStart[row]));
    node.hidden = false;
  }

  // The band, the number, the box of the bracket and its pair, and the active guide follow the caret. A key and a click ask for it, and one frame
  // answers, so a held key costs one update for each frame. Offsets come from the scan of the last paint: nothing parses the text here.
  function queueCaret() {
    if (!caretFrame) caretFrame = requestAnimationFrame(updateCaret);
  }
  function updateCaret() {
    caretFrame = 0;
    if (!lines.length) return;
    const start = input.selectionStart;
    const end = input.selectionEnd;
    const caret = input.selectionDirection === "backward" ? start : end;
    const row = rowOf(scan, caret);
    const empty = start === end;
    if (row !== caretRow) {
      caretRow = row;
      band.style.setProperty("--r", String(row));
      activeNum.style.setProperty("--r", String(row));
      activeNum.textContent = String(row + 1);
    }
    band.hidden = !empty;
    activeNum.hidden = false;
    matchA.hidden = true;
    matchB.hidden = true;
    guide.hidden = true;
    if (!empty) return;
    const near = bracketNear(scan, caret);
    if (near >= 0 && scan.mate[near] >= 0) {
      putBox(matchA, scan.pos[near]);
      putBox(matchB, scan.pos[scan.mate[near]]);
    }
    // The guide of the pair that holds the caret: it runs between the lines of the two brackets, at the column of the indent of the open line.
    const pair = enclosingPair(scan, caret);
    if (pair) {
      const openRow = rowOf(scan, scan.pos[pair[0]]);
      const closeRow = rowOf(scan, scan.pos[pair[1]]);
      if (closeRow - openRow > 1) {
        guide.style.setProperty("--r", String(openRow + 1));
        guide.style.setProperty("--c", String(indentColumns(lines[openRow])));
        guide.style.setProperty("--n", String(closeRow - openRow - 1));
        guide.hidden = false;
      }
    }
  }

  input.addEventListener("scroll", () => {
    readScroll();
    place();
  });
  input.addEventListener("input", paint);
  for (const name of ["keydown", "keyup", "mouseup", "focus", "select", "selectionchange"]) input.addEventListener(name, queueCaret);
  // A new size of the area (the window, the list of errors) changes the number of visible rows.
  const observer = typeof ResizeObserver === "function" ? new ResizeObserver(() => {
    readScroll();
    drawWindow(false);
    place();
  }) : null;
  if (observer) observer.observe(input);
  return {
    el: root,
    input,
    paint,
    paintMarks,
    // `getText` gives the saved text, or null for no marks.
    setBase(getText) {
      getBase = getText;
      paintMarks();
    },
    // The place of a syntax error: the line and the column (the first is 1), or 0 for none. The gutter marks the line, and the part of the line
    // under the column has a wavy line. A column of 0 marks the line only.
    setError(line, column = 0) {
      const nextLine = Number.isInteger(line) && line > 0 ? line : 0;
      const nextColumn = nextLine > 0 && Number.isInteger(column) && column > 0 ? column : 0;
      if (nextLine === errorLine && nextColumn === errorColumn) return;
      errorLine = nextLine;
      errorColumn = nextColumn;
      drawWindow(true);
      place();
      paintMarks();
    },
    destroy() {
      clearTimeout(marksTimer);
      if (caretFrame) cancelAnimationFrame(caretFrame);
      if (observer) observer.disconnect();
    },
  };
}
