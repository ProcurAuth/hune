// A JSON text area with colours and a line gutter, in the same frame as the SQL text area (10.3, 10.4).
// `input` is the real <textarea>: it keeps the text, the selection and the scroll bars. Its text is transparent, and a layer under it draws the
// colours with the same metrics (the technique of the SQL area, 10.3). The layer draws only the rows near the visible rows.
// After a change of `input.value` from code, call `paint()`; typing repaints by itself.
// `setBase(getText)` turns on the change marks: the gutter marks each line that differs from the text that `getText()` gives (the saved
// document), and `paintMarks()` draws them again. `setErrorLine(n)` marks the line number of a syntax error.

import { el } from "../ui.js";
import { diffLines, lineTokens } from "./json_check.js";

/** The height of one row in px. It is the line height of the frame (editor.css, `.jframe`). */
export const ROW = 18;
/** The rows that the layer keeps above and below the visible rows, and the margin to the border of the drawn rows that starts a new window. */
export const BUFFER = 100;
export const MARGIN = 30;
/** The most mark boxes that the gutter draws. A change of more runs than this shows the first ones. */
export const MAX_MARKS = 500;
/** The delay in ms between a key and the new marks. The colours do not wait. */
const MARKS_DELAY = 120;

const KIND_CLASS = { key: "jt-key", str: "jt-str", num: "jt-num", bool: "jt-bool", null: "jt-null", punct: "jt-punct" };

export function createJsonArea({ label, testid, tall = false }) {
  // The gutter is a frame with a fixed height. The numbers and the marks are in boxes that move with the scroll position of the text area.
  const nums = el("pre", { class: "jarea__nums" });
  const marks = el("div", { class: "jarea__marks" });
  const gutter = el("div", { class: "jarea__gutter", "aria-hidden": "true" }, marks, nums);
  const input = el("textarea", { class: "jarea__input", spellcheck: "false", autocorrect: "off", wrap: "off", "aria-label": label, autocapitalize: "off", autocomplete: "off", dataset: { testid } });
  const code = el("code", { class: "jarea__code" });
  const layer = el("pre", { class: "jarea__hl", "aria-hidden": "true" }, el("div", { class: "jarea__hl-in" }, code));
  const body = el("div", { class: "jarea__body" }, layer, input);
  const root = el("div", { class: `jframe${tall ? " jframe--tall" : ""}` }, gutter, body);
  const layerIn = layer.firstChild;
  let count = 0;
  let lines = [];
  let first = 0; // the first drawn row
  let last = 0; // after the last drawn row
  let getBase = null;
  let errorLine = 0;
  let marksTimer = 0;

  // The text of the rows [first, last), with a span for each token.
  function drawRows(from, to) {
    const frag = document.createDocumentFragment();
    for (let r = from; r < to; r++) {
      const line = lines[r];
      let at = 0;
      for (const [start, end, kind] of lineTokens(line)) {
        if (start > at) frag.append(line.slice(at, start));
        frag.append(el("span", { class: KIND_CLASS[kind], text: line.slice(start, end) }));
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

  // Puts the numbers, the marks and the layer where the text area is. It reads nothing from the page.
  function place() {
    nums.style.transform = `translateY(${-top}px)`;
    marks.style.transform = `translateY(${-top}px)`;
    drawWindow(false);
    layerIn.style.transform = `translate(${-left}px, ${first * ROW - top}px)`;
  }

  function paint() {
    lines = input.value.split("\n");
    if (lines.length !== count) {
      count = lines.length;
      nums.textContent = Array.from({ length: count }, (_, i) => String(i + 1)).join("\n");
    }
    drawWindow(true);
    place();
    scheduleMarks();
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

  input.addEventListener("scroll", () => {
    readScroll();
    place();
  });
  input.addEventListener("input", paint);
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
    // The line (the first is 1) of a syntax error, or 0 for none.
    setErrorLine(line) {
      const next = Number.isInteger(line) && line > 0 ? line : 0;
      if (next === errorLine) return;
      errorLine = next;
      paintMarks();
    },
    destroy() {
      clearTimeout(marksTimer);
      if (observer) observer.disconnect();
    },
  };
}
