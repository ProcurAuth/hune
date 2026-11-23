// The colour layer of the SQL area (10.3, 10.3c rule 1). It draws only the rows near the visible rows, as the
// folded view and the gutter do (10.3e). The text area stays the source of the text, the selection and the metrics.
// The state of a string or of a comment at the first drawn row comes from the spans of the whole text. The highlighter of the
// engine (vendor/chdash/app_highlight.js) is not changed: it reads only the plain text between two spans.
// A text of 262,144 bytes then costs about the same as a text of 300 rows, for each key and for each scroll step.

import { highlight } from "../vendor/chdash/app_highlight.js";
import { sqlLex } from "../vendor/chdash/app_sql_lex.js";

/** The height of one row of the SQL area in px (10.3, case B57). The folded view uses the same value. */
export const ROW = 20;

/** The rows that the layer keeps above and below the visible rows, the least number of visible rows, and the margin that starts a new window. */
export const BUFFER = 100;
export const VISIBLE = 40;
export const MARGIN = 30;

/** The rows that the lexer reads beyond the edge of a window, so that a name before a bracket, or a word pair of two rows, has its context. */
const CONTEXT_ROWS = 3;

/** The kinds of span that the engine colours. A name in backticks is plain text for the engine, and is read as a unit. */
const SPAN_CLASS = { [sqlLex.LINE]: "com", [sqlLex.BLOCK]: "com", [sqlLex.STRING]: "str", [sqlLex.DQUOTE]: "str" };

/** The class of each kind of token. Plain text has no class. The engine writes the same classes (wrapHtml of app_highlight.js). */
const TOKEN_CLASS = { str: "tok-str", com: "tok-com", kw: "tok-kw", fn: "tok-fn", num: "tok-num", null: "tok-null", type: "tok-type" };

/** The first index of the spans whose end is after `offset`. The spans are in order and do not overlap. */
function firstSpanAfter(spans, offset) {
  let lo = 0;
  let hi = spans.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (spans[mid].end > offset) hi = mid;
    else lo = mid + 1;
  }
  return lo;
}

/** The start offset of each line of the text, in order. The text has no CR (10.3). */
function lineStarts(text) {
  const starts = [0];
  for (let i = text.indexOf("\n"); i >= 0; i = text.indexOf("\n", i + 1)) starts.push(i + 1);
  return starts;
}

/**
 * The width in columns of the widest line, with a tab at the next multiple of 2 columns (`tab-size` of the layers). A character of a wide
 * script counts 2. A width that is too large is harmless: the layer is then a little wider than the text area.
 */
function widestColumns(text) {
  let widest = 0;
  let col = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c === 10) {
      if (col > widest) widest = col;
      col = 0;
    } else if (c === 9) col = (col & ~1) + 2;
    else col += c >= 0x2e80 ? 2 : 1;
  }
  return Math.max(widest, col);
}

/**
 * The tokens of the offsets `a` to `b` of the text, with the spans of the whole text. A span that crosses an edge is cut at the edge.
 * The plain text between two spans goes to the engine in one piece. The edges must not cut a name in backticks.
 */
function tokensOf(text, spans, a, b) {
  const out = [];
  const plain = (from, to) => {
    if (to <= from) return;
    const piece = text.slice(from, to);
    let tokens = piece.length >= highlight.wasmMinChars ? highlight.lexAllWasm(piece) : null;
    if (!tokens) tokens = highlight.lexAllJs(piece);
    for (const t of tokens) out.push({ start: t.start + from, end: t.end + from, kind: t.kind });
  };
  let at = a;
  for (let i = firstSpanAfter(spans, a); i < spans.length && spans[i].start < b; i++) {
    const kind = SPAN_CLASS[spans[i].kind];
    if (!kind) continue;
    const from = Math.max(spans[i].start, a);
    const to = Math.min(spans[i].end, b);
    plain(at, from);
    out.push({ start: from, end: to, kind });
    at = to;
  }
  plain(at, b);
  return out;
}

/**
 * Makes the colour layer. `code` is the element that holds the coloured rows. The layer sets the heights of `gapTop` and `gapBottom`, two
 * elements that the caller puts before and after `code`: they keep the place of the rows that are not drawn.
 */
export function createColourLayer({ code, gapTop, gapBottom }) {
  code.style.display = "block";
  let text = "";
  let spans = null;
  let starts = null;
  let columns = -1;
  let drawn = null;
  let shownWidth = "";
  let onDraw = null;

  function spansOfText() {
    if (!spans) spans = sqlLex.spans(text);
    return spans;
  }

  function startsOfText() {
    if (!starts) starts = lineStarts(text);
    return starts;
  }

  /** The offsets `a` to `b` widened to whole rows and by the context rows, and moved out of a name in backticks. */
  function context(a, b) {
    const list = startsOfText();
    const rowOf = (offset) => {
      let lo = 0;
      let hi = list.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (list[mid] <= offset) lo = mid;
        else hi = mid - 1;
      }
      return lo;
    };
    const all = spansOfText();
    let from = list[Math.max(0, rowOf(a) - CONTEXT_ROWS)];
    const lastRow = rowOf(Math.max(a, b - 1)) + CONTEXT_ROWS + 1;
    let to = lastRow < list.length ? list[lastRow] : text.length;
    const first = all[firstSpanAfter(all, from)];
    if (first && first.kind === sqlLex.BACKTICK && first.start < from) from = first.start;
    const last = all[firstSpanAfter(all, to - 1)];
    if (last && last.kind === sqlLex.BACKTICK && last.start < to && last.end > to) to = last.end;
    return [from, to];
  }

  /**
   * The coloured text from `a` to `b` as a list of pieces `{ text, kind }`. The kinds are the kinds of the engine for the whole text. Plain
   * pieces that follow each other are one piece.
   */
  function pieces(a, b) {
    const out = [];
    if (b <= a) return out;
    const [from, to] = context(a, b);
    for (const t of tokensOf(text, spansOfText(), from, to)) {
      if (t.end <= a) continue;
      if (t.start >= b) break;
      const part = text.slice(Math.max(t.start, a), Math.min(t.end, b));
      const last = out[out.length - 1];
      if (t.kind === "plain" && last && last.kind === "plain") last.text += part;
      else out.push({ text: part, kind: t.kind });
    }
    return out;
  }

  /** A fragment (not in the page) with the coloured text from `a` to `b`: a text node for plain text and a span with a class for the rest. */
  function fragment(a, b, tail = "") {
    const frag = document.createDocumentFragment();
    for (const piece of pieces(a, b)) {
      const name = TOKEN_CLASS[piece.kind];
      if (!name) {
        frag.append(document.createTextNode(piece.text));
        continue;
      }
      const span = document.createElement("span");
      span.className = name;
      span.textContent = piece.text;
      frag.append(span);
    }
    if (tail) frag.append(document.createTextNode(tail));
    return frag;
  }

  /** The rows to draw for a scroll position and a height of the view: the first row, the end row (excluded) and the number of rows. */
  function rowsFor(top, height) {
    const rows = startsOfText().length;
    const first = Math.max(0, Math.min(rows - 1, Math.floor(top / ROW)));
    const visible = Math.max(VISIBLE, Math.ceil((height || 0) / ROW));
    const r0 = Math.max(0, first - BUFFER);
    return { first, visible, rows, r0, r1: Math.max(r0 + 1, Math.min(rows, first + visible + BUFFER)) };
  }

  /** Puts the window in the page. `plain` writes the text with no colour (the engine failed). */
  function write(top, height, plain) {
    const { rows, r0, r1 } = rowsFor(top, height);
    const list = startsOfText();
    const from = list[r0];
    const to = r1 < rows ? list[r1] : text.length;
    const tail = r1 === rows ? "\n" : "";
    if (plain) code.textContent = text.slice(from, to) + tail;
    else code.replaceChildren(fragment(from, to, tail));
    gapTop.style.height = `${r0 * ROW}px`;
    gapBottom.style.height = `${(rows - r1) * ROW}px`;
    // The widest row gives the width of the content, so the horizontal scroll range does not change with the window.
    if (columns < 0) columns = widestColumns(text);
    const width = `${columns}ch`;
    if (width !== shownWidth) {
      shownWidth = width;
      code.style.minWidth = width;
    }
    drawn = { r0, r1, rows, from, to };
    if (onDraw) onDraw();
  }

  return {
    /** Sets the text. The next `draw` shows it. A call with the same text does nothing. */
    setText(next) {
      if (next === text && spans) return;
      text = next;
      spans = null;
      starts = null;
      columns = -1;
      drawn = null;
    },
    /** Draws the window for the scroll position `top` and the height of the view. With `plain`, the text has no colour. */
    draw(top, height, plain = false) {
      write(top, height, plain);
    },
    /** Draws the window again when the visible rows come near its border. It tells if it drew. */
    ensure(top, height) {
      if (!drawn) return false;
      const { first, visible, rows } = rowsFor(top, height);
      const early = drawn.r0 > 0 && first < drawn.r0 + MARGIN;
      const late = drawn.r1 < rows && first + visible > drawn.r1 - MARGIN;
      if (!early && !late) return false;
      write(top, height, false);
      return true;
    },
    /** The offsets of the text that the page shows in the layer: `from` and `to` (excluded), or null before the first draw. */
    bounds() {
      return drawn ? { from: drawn.from, to: drawn.to } : null;
    },
    pieces,
    fragment,
    /** A function that the layer calls after each draw. The editor uses it to put the marks of a bracket pair back. */
    set onDraw(fn) {
      onDraw = fn;
    },
  };
}
