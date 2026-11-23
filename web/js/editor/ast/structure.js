// The structure functions of the query text (F42, 10.3e and 10.3f): the folds, the partner of a bracket, the current block, and
// the pure model of the folded view. Pure module: no DOM access. It reads a result of `parseSql` or `parseJson` and the text.
// The tree is never a security check. The server is the only authority on the validity of a query, and a fault of the tree
// never stops a save or a run. A capped result gives no fold, no pair and no block: the editor shows the notice of the cap.
// The editor loads this module on demand, so it is not in the page measure of 9.30.

import { MAX_FOLD_MARKERS, SQL_NODE } from "./nodes.js";

/** The types of node that can be the current block (10.3f), and the JSON types that count as a block of a pipeline (JSON text only). */
const BLOCK_TYPES = new Set(["subquery", "cte", "clause", "object", "array"]);

/**
 * The line starts of a text. `lineOf(offset)` gives the 0-based line of an offset in O(log n). `count` is the number of lines.
 * A line ends at LF only: the text of a text area has no CR (10.3).
 */
export function lineIndex(text) {
  const starts = [0];
  for (let i = text.indexOf("\n"); i >= 0; i = text.indexOf("\n", i + 1)) starts.push(i + 1);
  const lineOf = (offset) => {
    let lo = 0;
    let hi = starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (starts[mid] <= offset) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  };
  return { starts, lineOf, count: starts.length };
}

/** True when the result can give structure: it exists, it has a root and it has no cap. */
function usable(result) {
  return Boolean(result && result.root && !result.capped);
}

/** True when `offset` is an integer inside the text of the result, ends included. */
function inText(result, offset) {
  return Number.isInteger(offset) && offset >= 0 && offset <= result.root.end;
}

/**
 * The folds of a result, in the order of the text. A fold is a node with an open bracket and a close bracket on two lines or
 * more. A macro is opaque and never folds. The result is `{ items, total, truncated }`. `items` has at most
 * `MAX_FOLD_MARKERS` folds. `total` is the number of folds of the text. `truncated` is true when `total` is above the cap.
 * A fold has `key` (the offset of the open bracket), `open`, `close`, `from` (the first hidden character), `to` (the end of the
 * hidden range, the offset of the close bracket), `type`, `name`, `line` and `endLine` (0-based lines of the brackets),
 * `lines` (the number of lines from the open bracket to the close bracket) and `node`.
 * A capped result, or a result with no root, has no fold.
 */
export function folds(result, text) {
  const out = { items: [], total: 0, truncated: false };
  if (!usable(result)) return out;
  const src = String(text ?? "");
  const index = lineIndex(src);
  const stack = [result.root];
  let last = -1;
  while (stack.length) {
    const n = stack.pop();
    if (typeof n.open === "number" && typeof n.close === "number" && n.close > n.open && n.type !== SQL_NODE.MACRO && n.open > last) {
      const line = index.lineOf(n.open);
      const endLine = index.lineOf(n.close);
      if (endLine > line) {
        last = n.open;
        out.total += 1;
        if (out.items.length < MAX_FOLD_MARKERS) {
          out.items.push({ key: n.open, open: n.open, close: n.close, from: n.open + 1, to: n.close, type: n.type, name: n.name || "", line, endLine, lines: endLine - line + 1, node: n });
        }
      }
    }
    for (let i = n.children.length - 1; i >= 0; i--) stack.push(n.children[i]);
  }
  out.truncated = out.total > MAX_FOLD_MARKERS;
  return out;
}

/** The child of `parent` that holds `offset` (`start <= offset < end`), or null. The children are in order and do not overlap. */
function childAt(parent, offset) {
  const list = parent.children;
  let lo = 0;
  let hi = list.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (list[mid].start <= offset) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  if (found < 0) return null;
  const c = list[found];
  return offset < c.end ? c : null;
}

/** The bracket that stands at the offset `pos`, or null. It gives `{ at, partner, side, matched }`. */
function bracketAt(result, pos) {
  let n = result.root;
  while (n) {
    if (n.open === pos) return typeof n.close === "number" ? { at: pos, partner: n.close, side: "open", matched: true } : { at: pos, partner: null, side: "open", matched: false };
    if (n.close === pos) return typeof n.open === "number" ? { at: pos, partner: n.open, side: "close", matched: true } : { at: pos, partner: null, side: "close", matched: false };
    if (n.type === SQL_NODE.ERROR && n.error === "unmatched" && n.start === pos) return { at: pos, partner: null, side: "lone", matched: false };
    n = childAt(n, pos);
  }
  return null;
}

/**
 * The bracket pair at a caret (10.3f). The pair is the bracket before the caret, else the bracket at the caret. The result is
 * `{ at, partner, side, matched }`: `at` is the offset of the bracket, `partner` the offset of its partner or null, `side` is
 * `open`, `close` or `lone`, and `matched` is false for a bracket with no partner (an unclosed node or an `unmatched` error).
 * It gives null when there is no bracket, when the offset is outside the text (-1, or the length plus 1), or for a capped result.
 */
export function bracketPartner(result, offset) {
  if (!usable(result) || !inText(result, offset)) return null;
  if (offset > 0) {
    const before = bracketAt(result, offset - 1);
    if (before) return before;
  }
  return offset < result.root.end ? bracketAt(result, offset) : null;
}

/** The end of a node without its trailing blanks, for the last line of a clause. */
function trimmedEnd(text, start, end) {
  let e = end;
  while (e > start && (text.charCodeAt(e - 1) === 32 || text.charCodeAt(e - 1) === 10 || text.charCodeAt(e - 1) === 9 || text.charCodeAt(e - 1) === 13)) e -= 1;
  return e;
}

/** True when `offset` is in the node for the purpose of a block: inside the brackets for a bracket node, else `start..end`. */
function holds(n, offset) {
  if (typeof n.open === "number") return n.open < offset && offset <= (typeof n.close === "number" ? n.close : n.end);
  return n.start <= offset && offset <= n.end;
}

/**
 * The current block at a caret (10.3f): the innermost node of the type `subquery`, `cte` or `clause` that holds the caret and
 * spans two lines or more. For a JSON text, the innermost `object` or `array` with brackets counts. The caret is in a bracket
 * node when it is after the open bracket and not after the close bracket. The result is `{ type, name, start, end, line,
 * endLine, lines, node }` (`line` and `endLine` are 0-based, `end` is exclusive), or null. `text` is the text of the result.
 * It gives null for an offset outside the text, and for a capped result.
 */
export function blockAt(result, offset, text) {
  if (!usable(result) || !inText(result, offset)) return null;
  const src = String(text ?? "");
  const path = [];
  const json = result.root.type === "doc";
  let n = result.root;
  while (n) {
    path.push(n);
    n = childAt(n, offset) || (offset > 0 ? childAt(n, offset - 1) : null);
  }
  let index = null;
  for (let i = path.length - 1; i >= 0; i--) {
    const c = path[i];
    if (!BLOCK_TYPES.has(c.type) || !holds(c, offset)) continue;
    // An object and an array are blocks of a JSON text only: a SQL array is not a block (10.3f).
    if ((c.type === "object" || c.type === "array") && (json === false || typeof c.open !== "number")) continue;
    if (!index) index = lineIndex(src);
    const first = typeof c.open === "number" ? c.open : c.start;
    const lastChar = typeof c.close === "number" ? c.close : Math.max(first, trimmedEnd(src, c.start, c.end) - 1);
    const line = index.lineOf(first);
    const endLine = index.lineOf(lastChar);
    if (endLine > line) return { type: c.type, name: c.name || c.stage || "", start: c.start, end: c.end, line, endLine, lines: endLine - line + 1, node: c };
  }
  return null;
}

/**
 * The outer folded folds: from the fold items and the set of open keys, the folds that are folded and not inside another
 * folded fold. They are in order and do not overlap.
 */
export function outerFolds(items, openKeys) {
  const out = [];
  let end = -1;
  for (const f of items) {
    if (!openKeys.has(f.key) || f.open < end) continue;
    out.push(f);
    end = f.close + 1;
  }
  return out;
}

/**
 * The rows of the folded view. `outer` is the result of `outerFolds`. A line is hidden when the line feed before it is in a
 * folded range. The result is `{ rowLines, rowOfLine }`: `rowLines[r]` is the real 0-based line that starts the row `r`, and
 * `rowOfLine[l]` is the row that shows the line `l` (for a hidden line, the row that hides it).
 */
export function foldedRows(text, outer) {
  const { starts } = lineIndex(text);
  const rowLines = [];
  const rowOfLine = new Int32Array(starts.length);
  let p = 0;
  let row = -1;
  for (let l = 0; l < starts.length; l++) {
    const s = starts[l];
    while (p < outer.length && outer[p].to < s) p += 1;
    const hidden = p < outer.length && outer[p].from < s && s <= outer[p].to;
    if (!hidden) {
      row += 1;
      rowLines.push(l);
    }
    rowOfLine[l] = Math.max(row, 0);
  }
  return { rowLines, rowOfLine };
}

/** The number of non-blank characters before an offset. A blank is a space, TAB, LF, CR, FF or VT, as the tokenizers read it. */
export function nonBlankBefore(text, offset) {
  let count = 0;
  const to = Math.min(offset, text.length);
  for (let i = 0; i < to; i++) {
    const c = text.charCodeAt(i);
    if (c !== 32 && c !== 10 && c !== 9 && c !== 13 && c !== 12 && c !== 11) count += 1;
  }
  return count;
}

/** The offset in a text that has `count` non-blank characters before it, with the smallest offset that does. A count too large gives the length. */
export function offsetAfterNonBlank(text, count) {
  if (count <= 0) return 0;
  let seen = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c !== 32 && c !== 10 && c !== 9 && c !== 13 && c !== 12 && c !== 11) {
      seen += 1;
      if (seen === count) return i + 1;
    }
  }
  return text.length;
}
