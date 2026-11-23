// The folded view, the fold markers, the band of the current block and the bracket marks of the SQL area (10.3e, 10.3f).
// The text area never changes for a fold and keeps the focus. The folded view is a visual layer over it. It shows the
// coloured text with each outer folded range replaced by a chip. It is `aria-hidden`, as the colour layer is.
// The view holds only the rows near the visible rows (a window of rows with a gap above and below), so a fold costs the same for a
// text of 262,144 bytes as for a small text. The colours of a window come from the colour layer (sql_colour.js), which draws them
// from the spans of the whole text. The colour layer holds only its own window of rows, so the folded view never reads its elements.
// This module only draws. The editor (sql_editor.js) owns the state and the keys. The tree is never a security check.

import { clear, el } from "../ui.js";
import { lineIndex } from "./ast/structure.js";
import { BUFFER as WINDOW_BUFFER, MARGIN as WINDOW_MARGIN, ROW, VISIBLE as WINDOW_VISIBLE } from "./sql_colour.js";

/** The height of one row of the SQL area in px (10.3, case B57). The colour layer defines it. */
export { ROW };

/** The name of the highlight that paints the selection in the folded view (CSS.highlights). */
const SELECTION_NAME = "sql-selection";

/** Shows or hides a layer of the SQL area with the style `visibility`. A hidden layer keeps its box and has no content. */
export function setShown(node, shown) {
  node.style.display = "block";
  node.style.visibility = shown ? "visible" : "hidden";
}

/** True when a layer of the SQL area shows (see `setShown`). */
export function isShown(node) {
  return node.style.visibility !== "hidden";
}

/**
 * Makes the layers of the folds. `code` is the element of the colour layer, which holds the coloured rows near the visible rows.
 * `colour` is the colour layer: it gives the coloured text of any range. `input` is the text area. The result has the elements
 * to place (`folded`, `band`, `marks`) and the functions that draw.
 * `onToggle(key)` is called for a click on a marker or a chip. `key` is the offset of the open bracket of the fold.
 */
export function createFoldView({ code, colour, input, onToggle }) {
  const foldedCode = el("code");
  // The gaps hold the place of the rows that are not drawn, so the scroll height is the height of all rows.
  const gapTop = el("div");
  const gapBottom = el("div");
  const folded = el("pre", { class: "sql__hl sql__folded", "aria-hidden": "true", dataset: { testid: "sql-folded" } }, gapTop, foldedCode, gapBottom);
  foldedCode.style.display = "block";
  // The browser must not move the scroll position when a window changes: the gaps keep the height.
  folded.style.overflowAnchor = "none";
  const band = el("div", { class: "sql__band", "aria-hidden": "true", dataset: { testid: "sql-block" } });
  // The folded view and the band show and hide with the style `visibility`, not with the attribute `hidden` or the style `display`. A change of one of
  // these two makes the browser lay out the page again (about 150 ms with a text of 262,144 bytes in the text area), and a change of `visibility` does not.
  setShown(folded, false);
  setShown(band, false);
  const marksIn = el("div", { class: "sql__marks-in" });
  const marks = el("div", { class: "sql__marks", role: "group", "aria-label": "Fold markers" }, marksIn);
  // The text nodes of the folded view with the offset of their first character, and the chips with the range that they hide.
  let bases = new Map();
  let chips = new Map();
  let textLength = 0;
  let bandLine = 0;
  let highlighted = false;
  let shown = false;
  // The state of the window: the text, the folds, the first source line of each row, the line starts, the rows that are drawn
  // (`from` included, `to` excluded), the pair of brackets that the colour layer shows and the last selection that was painted.
  let source = "";
  let folds = [];
  let rowLines = [];
  let starts = [];
  let drawn = null;
  let pairShown = null;
  let lastSelection = null;
  let viewTop = 0;
  let viewHeight = WINDOW_VISIBLE * ROW;

  // The markers keep the focus where it was: a press on a marker does not move it.
  marks.addEventListener("mousedown", (event) => event.preventDefault());
  marks.addEventListener("click", (event) => {
    const button = event.target instanceof Element ? event.target.closest("button") : null;
    if (button && marks.contains(button)) onToggle(Number(button.dataset.key));
  });
  folded.addEventListener("click", (event) => {
    const chip = event.target instanceof Element ? event.target.closest(".sql__chip") : null;
    if (chip && chips.has(chip)) onToggle(chips.get(chip).key);
  });

  /**
   * Draws the markers. `items` are the folds, `openKeys` the set of folded keys, `hiddenIn(item)` tells if a fold is hidden, `rowOfLine` maps a line to
   * its row. Only the markers of the rows `rows.from` to `rows.to` (excluded) are drawn: a text of 262,144 bytes has up to 2,000 markers and each one costs
   * layout time. The attribute `data-count` of the group has the number of markers that the text has, drawn or not.
   */
  function drawMarkers(items, openKeys, rowOfLine, hiddenIn, rows = null) {
    clear(marksIn);
    const frag = document.createDocumentFragment();
    let count = 0;
    for (const item of items) {
      if (hiddenIn && hiddenIn(item)) continue;
      count += 1;
      const row = rowOfLine ? rowOfLine[item.line] : item.line;
      if (rows && (row < rows.from || row >= rows.to)) continue;
      const isFolded = openKeys.has(item.key);
      const verb = isFolded ? "Unfold" : "Fold";
      const b = el("button", {
        type: "button",
        class: isFolded ? "sql__fold sql__fold--folded" : "sql__fold",
        tabindex: "-1",
        "aria-expanded": String(!isFolded),
        "aria-label": `${verb} lines ${item.line + 1} to ${item.endLine + 1}`,
        dataset: { testid: "sql-fold", from: item.from, key: item.key },
      }, el("span", { class: "sql__fold-icon", "aria-hidden": "true", text: isFolded ? "▸" : "▾" }));
      b.style.top = `${row * ROW}px`;
      frag.append(b);
    }
    marksIn.append(frag);
    marks.dataset.count = String(count);
  }

  /** Removes the markers. */
  function clearMarkers() {
    clear(marksIn);
    marks.dataset.count = "0";
  }

  /** Moves the markers with the scroll position of the view. */
  function scrollMarks(top) {
    marksIn.style.transform = `translateY(${-top}px)`;
  }

  /** Copies the coloured text from `a` to `b`. The copy is a fragment. It is not in the page. A fault of the colours gives the plain text. */
  function copyColour(a, b) {
    try {
      return colour.fragment(a, b);
    } catch {
      const part = document.createDocumentFragment();
      part.append(document.createTextNode(source.slice(a, b)));
      return part;
    }
  }

  /** The first index of `folds` whose range ends after `offset`. */
  function firstFoldAfter(offset) {
    let lo = 0;
    let hi = folds.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (folds[mid].to > offset) hi = mid;
      else lo = mid + 1;
    }
    return lo;
  }

  /**
   * Draws the rows `r0` to `r1` (the last excluded): the coloured text of their lines, with each fold of the rows replaced by a chip.
   * A row boundary is the start of a line that no fold hides, so a fold is always inside the rows or outside them.
   */
  function drawRows(r0, r1) {
    bases = new Map();
    chips = new Map();
    const from = starts[rowLines[r0]];
    const to = r1 < rowLines.length ? starts[rowLines[r1]] - 1 : source.length;
    const frag = document.createDocumentFragment();
    let cur = from;
    const piece = (a, b) => {
      if (b <= a) return;
      const part = copyColour(a, b);
      const walker = document.createTreeWalker(part, 4);
      let at = a;
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        bases.set(n, at);
        at += n.data.length;
      }
      frag.append(part);
    };
    for (let i = firstFoldAfter(from); i < folds.length && folds[i].from < to; i++) {
      const f = folds[i];
      piece(cur, f.from);
      const chip = el("span", { class: "sql__chip", dataset: { testid: "sql-chip", from: f.from } }, `… ${f.lines} lines`);
      chips.set(chip, { key: f.key, from: f.from, to: f.to });
      frag.append(chip);
      cur = f.to;
    }
    piece(cur, to);
    foldedCode.replaceChildren(frag);
    gapTop.style.height = `${r0 * ROW}px`;
    gapBottom.style.height = `${(rowLines.length - r1) * ROW}px`;
    drawn = { from: r0, to: r1 };
    if (lastSelection) paintSelection(lastSelection[0], lastSelection[1]);
  }

  /** Draws the window around the visible rows, when the visible rows come near the border of the drawn rows (or `force`). */
  function drawWindow(force) {
    if (!shown) return;
    const first = Math.floor(viewTop / ROW);
    const visible = Math.max(WINDOW_VISIBLE, Math.ceil(viewHeight / ROW));
    if (!force && drawn) {
      const early = drawn.from > 0 && first < drawn.from + WINDOW_MARGIN;
      const late = drawn.to < rowLines.length && first + visible > drawn.to - WINDOW_MARGIN;
      if (!early && !late) return;
    }
    const r0 = Math.max(0, Math.min(rowLines.length - 1, first - WINDOW_BUFFER));
    const r1 = Math.min(rowLines.length, first + visible + WINDOW_BUFFER);
    drawRows(r0, Math.max(r1, r0 + 1));
  }

  // The scroll position is read here, after the browser has laid out the view, and not when a window is drawn: a read of it
  // would make the browser lay out the whole page at once (about 280 ms for a text of 262,144 bytes).
  folded.addEventListener("scroll", () => {
    viewTop = folded.scrollTop;
    viewHeight = folded.clientHeight || viewHeight;
    drawWindow(false);
  });

  /**
   * Shows the folded view. `text` is the text of the text area. The colour layer must have the same text. `outer` is in order and does not
   * overlap: each range is replaced by a chip `… N lines`. `rows` is the first source line of each row. `top` is the scroll position that the
   * view takes and `height` is the height of the view in px: the caller read both before it changed the page. Only the rows near the visible
   * rows are drawn. The caller calls `setScroll` after all its other changes of the page, because it makes the browser lay out the page.
   */
  function show(text, outer, rows, top, height) {
    source = text;
    folds = outer;
    rowLines = rows;
    starts = lineIndex(text).starts;
    textLength = text.length;
    // The widest line gives the width of the content, so the horizontal scroll bar does not change with the window.
    let widest = 0;
    for (const line of rows) widest = Math.max(widest, (line + 1 < starts.length ? starts[line + 1] - 1 : text.length) - starts[line]);
    foldedCode.style.minWidth = `${widest}ch`;
    drawn = null;
    shown = true;
    viewTop = top;
    viewHeight = height || viewHeight;
    setShown(folded, true);
    gapTop.style.height = "0px";
    gapBottom.style.height = `${rows.length * ROW}px`;
    drawWindow(true);
  }

  /** Sets the scroll position of the view. The browser lays out the page for it. */
  function setScroll(top, left) {
    folded.scrollTop = top;
    folded.scrollLeft = left;
  }

  /** Hides the folded view and the painted selection. */
  function hide() {
    setShown(folded, false);
    shown = false;
    clear(foldedCode);
    bases = new Map();
    chips = new Map();
    source = "";
    folds = [];
    rowLines = [];
    starts = [];
    drawn = null;
    clearSelection();
  }

  /** The offset of a DOM point of the folded view, or null. */
  function offsetOf(node, off) {
    const found = pointOffset(node, off);
    return found === null ? null : Math.max(0, Math.min(found, textLength));
  }

  function chipOf(node) {
    for (let n = node; n && n !== folded; n = n.parentNode) if (chips.has(n)) return n;
    return null;
  }

  function startOf(n) {
    if (chips.has(n)) return chips.get(n).from;
    if (n.nodeType === 3) return bases.has(n) ? bases.get(n) : null;
    for (const c of n.childNodes) {
      const v = startOf(c);
      if (v !== null) return v;
    }
    return null;
  }

  function endOf(n) {
    if (chips.has(n)) return chips.get(n).to;
    if (n.nodeType === 3) return bases.has(n) ? bases.get(n) + n.data.length : null;
    for (let i = n.childNodes.length - 1; i >= 0; i--) {
      const v = endOf(n.childNodes[i]);
      if (v !== null) return v;
    }
    return null;
  }

  function pointOffset(node, off) {
    const chip = chipOf(node);
    if (chip) return chips.get(chip).from;
    if (node.nodeType === 3) return bases.has(node) ? bases.get(node) + Math.min(off, node.data.length) : null;
    const child = node.childNodes[off];
    if (child) return startOf(child);
    return endOf(node);
  }

  /** The offset of the text under a point of the window (caretPositionFromPoint, else caretRangeFromPoint), or null. */
  function offsetFromPoint(x, y) {
    let node = null;
    let off = 0;
    if (typeof document.caretPositionFromPoint === "function") {
      const p = document.caretPositionFromPoint(x, y);
      if (p) {
        node = p.offsetNode;
        off = p.offset;
      }
    } else if (typeof document.caretRangeFromPoint === "function") {
      const r = document.caretRangeFromPoint(x, y);
      if (r) {
        node = r.startContainer;
        off = r.startOffset;
      }
    }
    if (!node || !folded.contains(node)) return null;
    return offsetOf(node, off);
  }

  /** Paints the selection `a..b` of the text area in the folded view with CSS.highlights, or removes it when it is empty. */
  function paintSelection(a, b) {
    if (typeof CSS === "undefined" || !CSS.highlights || typeof Highlight === "undefined") return;
    if (!shown || b <= a) {
      clearSelection();
      return;
    }
    lastSelection = [a, b];
    const ranges = [];
    for (const [node, base] of bases) {
      const end = base + node.data.length;
      if (end <= a || base >= b) continue;
      const r = new Range();
      r.setStart(node, Math.max(0, a - base));
      r.setEnd(node, Math.min(node.data.length, b - base));
      ranges.push(r);
    }
    for (const [chip, range] of chips) {
      if (range.to <= a || range.from >= b) continue;
      const r = new Range();
      r.selectNode(chip);
      ranges.push(r);
    }
    CSS.highlights.set(SELECTION_NAME, new Highlight(...ranges));
    highlighted = true;
  }

  /** Removes the painted selection. */
  function clearSelection() {
    if (highlighted && typeof CSS !== "undefined" && CSS.highlights) CSS.highlights.delete(SELECTION_NAME);
    highlighted = false;
    lastSelection = null;
  }

  /** Shows the band of a block (first and last line, 0-based, and its type), or hides it with null. `scrollTop` is the scroll of the text area. */
  function setBand(block, scrollTop) {
    if (!block) {
      setShown(band, false);
      return;
    }
    bandLine = block.line;
    band.dataset.type = block.type;
    band.style.height = `${(block.endLine - block.line + 1) * ROW}px`;
    band.style.transform = `translateY(${block.line * ROW - scrollTop}px)`;
    setShown(band, true);
  }

  /** Moves the band with the scroll position. */
  function scrollBand(scrollTop) {
    if (isShown(band)) band.style.transform = `translateY(${bandLine * ROW - scrollTop}px)`;
  }

  /** Removes each pair mark from the colour layer and joins the text nodes again. */
  function removePairMarks() {
    const old = code.querySelectorAll(".sql__pair");
    if (old.length === 0) return;
    for (const mark of old) mark.replaceWith(document.createTextNode(mark.textContent));
    code.normalize();
  }

  /** Removes the pair marks and forgets the pair. */
  function clearPair() {
    pairShown = null;
    removePairMarks();
  }

  /**
   * Wraps the character at `offset` of the text in a mark. The colour layer holds only a window of the text, so the offset becomes a
   * place in the window. It does nothing when the window has no such character: that bracket is not in view.
   */
  function markChar(offset, lone) {
    const bounds = colour.bounds();
    if (!bounds || offset < bounds.from || offset >= bounds.to) return;
    const local = offset - bounds.from;
    const walker = document.createTreeWalker(code, 4);
    let pos = 0;
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const len = n.data.length;
      if (local < pos + len) {
        const r = new Range();
        r.setStart(n, local - pos);
        r.setEnd(n, local - pos + 1);
        const mark = el("span", { class: lone ? "sql__pair sql__pair--lone" : "sql__pair", dataset: { testid: "sql-pair" } });
        r.surroundContents(mark);
        return;
      }
      pos += len;
    }
  }

  function markPair(pair) {
    const offsets = pair.matched ? [pair.at, pair.partner].sort((a, b) => b - a) : [pair.at];
    for (const o of offsets) markChar(o, !pair.matched);
  }

  /** Draws the marks of a pair `{ at, partner, matched }` from `bracketPartner`. A bracket with no partner gets the mark of an error. */
  function setPair(pair) {
    clearPair();
    if (!pair) return;
    pairShown = pair;
    markPair(pair);
  }

  /** Puts the marks back after the colour layer drew a new window. The new window has no mark. */
  function redrawPair() {
    if (pairShown) markPair(pairShown);
  }

  return {
    folded,
    band,
    marks,
    get shown() {
      return shown;
    },
    drawMarkers,
    clearMarkers,
    scrollMarks,
    show,
    setScroll,
    hide,
    offsetFromPoint,
    paintSelection,
    clearSelection,
    setBand,
    scrollBand,
    setPair,
    clearPair,
    redrawPair,
  };
}
