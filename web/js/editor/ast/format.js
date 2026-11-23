// The formatter of the query editor (F42): `formatSql` for ClickHouse and PostgreSQL, `formatJson` for the pipeline of MongoDB.
// Pure module: no DOM access, no event. The write into the text area is the job of the editor.
// The tree is never a security check. The server stays the only authority on the validity of a query, and a fault of the
// formatter never stops a save or a run. The editor loads this module on demand, so it is not in the page measure of 9.30.
//
// The formatter changes only blank tokens. It never changes, splits or joins a comment, a string, a name, a macro, a variable
// or any other token. Each call checks itself: it tokenizes its output with the tokenizer of the input and compares the
// non-blank tokens (kind, text and `closed`) with the input. It also checks that a line end between two strings stays.
// After any difference it returns the input unchanged with a reason.
// A text with a capped tree, with an error node in a statement that the format reads, or with an unclosed string or comment
// there is not formatted. A statement that does not start with SELECT, WITH or VALUES stays as it is (byte for byte).
//
// The style follows docs/04, 10.3d (the defaults of the options are in `FORMAT_DEFAULTS`):
// - A clause keyword starts a line. A clause that fits the width is one line.
// - A list clause (SELECT, FROM, GROUP BY, ORDER BY, SETTINGS, WINDOW, ARRAY JOIN, WITH, VALUES) with two items or more that
//   does not fit puts each item on its own line, 2 spaces deeper than the keyword. With one item, its brackets break.
// - A condition clause (WHERE, PREWHERE, HAVING, QUALIFY, the condition of ON) that does not fit puts each AND and OR first
//   on a new line, 2 spaces deeper. The AND of BETWEEN does not break.
// - A subquery of two clauses or more always breaks. A subquery of one clause fits when its flat form fits.
// - A call, a paren, an array or a tuple that does not fit has one item on each line. Items that are all literals, variables
//   or macros fill the lines. A CASE that does not fit has its WHEN and ELSE lines 2 spaces deeper.
// - Spaces: none after an opening bracket, none before a closing bracket, a comma or a semicolon, one after a comma, one on
//   each side of = == <> != < <= > >= and ||. Elsewhere a space stays where the input had a blank and no space stays where it
//   had none.
// - A macro call `$__name(...)` and a brace group are copied byte for byte.
// - A blank line between two statements stays (as one blank line). All other blank lines go.
// - A line end between two strings stays (PostgreSQL joins them as one constant).
// - JSON: an object or an array stays on one line when it fits the width, else one member or one item for each line; an
//   array of scalars fills the lines.
//
// The time of a format is linear in the length of the text.

import { JSON_NODE, MAX_AST_INPUT_BYTES, SQL_NODE, TOKEN, utf8Length } from "./nodes.js";
import { parseJson } from "./json_tree.js";
import { parseSql } from "./sql_tree.js";
import { tokensClickHouse } from "./tokens_ch.js";
import { tokensPostgres } from "./tokens_pg.js";

/** The default options of the formatter: the width of one level, the width of a line, and no range. */
export const FORMAT_DEFAULTS = Object.freeze({ indent: 2, width: 80, range: null });

// ---- The layout engine: a flat list of commands and a renderer with the rule of the groups (Wadler). ----

/** The commands. T: text of the source (a, b). TM: text with a line end inside. TC: a line comment (no width in a fit check). */
const OP_T = 1;
const OP_TM = 2;
const OP_TC = 3;
const OP_SP = 4;
const OP_LINE = 5; // a space in a flat group, a new line in a broken group
const OP_SOFT = 6; // nothing in a flat group, a new line in a broken group
const OP_HARD = 7; // always a new line
const OP_HARD2 = 8; // a new line and one blank line
const OP_GS = 9;
const OP_GE = 10;
const OP_IN = 11;
const OP_OUT = 12;
const OP_FILL = 13; // a space, or a new line when the next item does not fit the rest of the line (a broken group)

/** The width that a hard line gives to a group: the group never fits. */
const INF = 1e9;

/** The separators that wait for the next token, in the order of their force. */
const P_NONE = 0;
const P_SP = 1;
const P_SOFT = 2;
const P_FILL = 3;
const P_LINE = 4;
const P_HARD = 5;
const P_HARD2 = 6;

/** The strings of the indent, one for each width. */
const SPACES = [""];

/** Gives the string of `n` spaces. */
function spaces(n) {
  while (SPACES.length <= n) SPACES.push(SPACES[SPACES.length - 1] + " ");
  return SPACES[n];
}

/** A writer of commands: three typed arrays that grow when they are full. `end()` gives the list of commands. */
function newWriter(cap) {
  let ops = new Uint8Array(cap);
  let a = new Int32Array(cap);
  let b = new Int32Array(cap);
  let n = 0;
  return {
    emit(op, x, y) {
      if (n === ops.length) {
        const c = n * 2;
        const o2 = new Uint8Array(c);
        o2.set(ops);
        const a2 = new Int32Array(c);
        a2.set(a);
        const b2 = new Int32Array(c);
        b2.set(b);
        ops = o2;
        a = a2;
        b = b2;
      }
      ops[n] = op;
      a[n] = x;
      b[n] = y;
      n += 1;
    },
    end() {
      return { ops, a, b, n };
    },
  };
}

/** True when the text src[s, e) has a line feed. */
function hasLf(src, s, e) {
  for (let k = s; k < e; k++) if (src.charCodeAt(k) === 10) return true;
  return false;
}

/** The number of line feeds in the text src[s, e). */
function countLf(src, s, e) {
  let n = 0;
  for (let k = s; k < e; k++) if (src.charCodeAt(k) === 10) n++;
  return n;
}

/**
 * Renders the commands to a text. `base` is the indent of the new lines, `col0` the column where the text starts.
 * A group is flat when its flat width, the text up to the next possible line break, and the column fit in `width`.
 */
function render(src, doc, o) {
  const { ops, a, b, n } = doc;
  const pre = new Float64Array(n + 1);
  const stack = [];
  for (let k = 0; k < n; k++) {
    const op = ops[k];
    let w = 0;
    if (op === OP_T || op === OP_TM) w = b[k] - a[k];
    else if (op === OP_SP || op === OP_LINE || op === OP_FILL) w = 1;
    else if (op === OP_HARD || op === OP_HARD2) w = INF;
    pre[k + 1] = pre[k] + w;
    if (op === OP_GS) stack.push(k);
    else if (op === OP_GE) {
      const g = stack.pop();
      if (g !== undefined) {
        b[g] = k;
        b[k] = g;
        a[g] = pre[k] - pre[g];
      }
    }
  }
  // The text after a group end, up to the next place where a line can break.
  let run = 0;
  for (let k = n - 1; k >= 0; k--) {
    const op = ops[k];
    if (op === OP_T || op === OP_TM) run += b[k] - a[k];
    else if (op === OP_SP) run += 1;
    else if (op === OP_LINE || op === OP_SOFT || op === OP_HARD || op === OP_HARD2) run = 0;
    else if (op === OP_FILL) {
      a[k] = run;
      run = 0;
    } else if (op === OP_GE) a[k] = run;
  }
  const step = o.indent;
  const width = o.width;
  const eol = o.eol;
  let out = "";
  let col = o.col0;
  let ind = o.base;
  let flat = false;
  let pendSp = false;
  let started = false;
  const modes = [];
  const newline = (blank) => {
    pendSp = false;
    if (!started) return;
    out += blank ? eol + eol + spaces(ind) : eol + spaces(ind);
    col = ind;
  };
  for (let k = 0; k < n; k++) {
    switch (ops[k]) {
      case OP_T:
      case OP_TC:
        if (pendSp) {
          out += " ";
          col += 1;
          pendSp = false;
        }
        out += src.slice(a[k], b[k]);
        col += b[k] - a[k];
        started = true;
        break;
      case OP_TM: {
        if (pendSp) {
          out += " ";
          col += 1;
          pendSp = false;
        }
        const s = src.slice(a[k], b[k]);
        out += s;
        const nl = s.lastIndexOf("\n");
        col = nl < 0 ? col + s.length : s.length - nl - 1;
        started = true;
        break;
      }
      case OP_SP:
        if (started) pendSp = true;
        break;
      case OP_LINE:
        if (flat) {
          if (started) pendSp = true;
        } else newline(false);
        break;
      case OP_SOFT:
        if (!flat) newline(false);
        break;
      case OP_FILL:
        if (flat || col + (pendSp ? 1 : 0) + 1 + a[k] <= width) {
          if (started) pendSp = true;
        } else newline(false);
        break;
      case OP_HARD:
        newline(false);
        break;
      case OP_HARD2:
        newline(true);
        break;
      case OP_GS:
        modes.push(flat);
        if (!flat) flat = col + (pendSp ? 1 : 0) + a[k] + a[b[k]] <= width;
        break;
      case OP_GE:
        flat = modes.length ? modes.pop() : false;
        break;
      case OP_IN:
        ind += step;
        break;
      case OP_OUT:
        ind -= step;
        break;
      default:
        break;
    }
    if (out.length > MAX_AST_INPUT_BYTES) return null; // the length is never above the bytes: the caller refuses with output_too_large
  }
  return out;
}

// ---- Common helpers of the two formatters. ----

/** Normalizes the options. A wrong value gives the default. */
function readOptions(opts) {
  const o = opts && typeof opts === "object" ? opts : {};
  const indent = Number.isInteger(o.indent) && o.indent >= 0 && o.indent <= 16 ? o.indent : FORMAT_DEFAULTS.indent;
  const width = Number.isInteger(o.width) && o.width >= 20 && o.width <= 1000 ? o.width : FORMAT_DEFAULTS.width;
  let range = null;
  if (Array.isArray(o.range) && o.range.length === 2 && Number.isFinite(o.range[0]) && Number.isFinite(o.range[1])) {
    range = [Math.min(o.range[0], o.range[1]), Math.max(o.range[0], o.range[1])];
  }
  return { indent, width, range };
}

/** The line end of a text: CR LF when each line feed of the text has a CR before it (and there is one), else LF. */
function eolOf(src) {
  let at = src.indexOf("\n");
  if (at < 0) return "\n";
  while (at >= 0) {
    if (at === 0 || src.charCodeAt(at - 1) !== 13) return "\n";
    at = src.indexOf("\n", at + 1);
  }
  return "\r\n";
}

/** The number of lines of a text: zero for an empty text. */
function lineCount(s) {
  return s.length === 0 ? 0 : countLf(s, 0, s.length) + 1;
}

/** The line and the column (both from 1) of an offset. */
function locate(src, at) {
  let line = 1;
  let last = -1;
  for (let k = src.indexOf("\n"); k >= 0 && k < at; k = src.indexOf("\n", k + 1)) {
    line += 1;
    last = k;
  }
  return { line, column: at - last };
}

/** The refusal: the input unchanged with the reason, and the offset, the line and the column when there is one. */
function refusal(src, reason, at = -1) {
  const where = at >= 0 ? locate(src, Math.min(at, src.length)) : { line: 0, column: 0 };
  return { text: src, changed: false, reason, at, line: where.line, column: where.column, lines: 0, totalLines: lineCount(src), replace: null };
}

/** True when the string `a` at `i` equals the string `b` at `j` for `len` code units. */
function sameRun(a, i, b, j, len) {
  for (let k = 0; k < len; k++) if (a.charCodeAt(i + k) !== b.charCodeAt(j + k)) return false;
  return true;
}

/**
 * True when the non-blank tokens of two token lists are equal in kind, text and `closed`, and when each pair of strings that
 * follow one another has a line end between them in both texts or in none (a line end can be a part of the syntax).
 * No string is made for a token.
 */
function sameTokens(srcA, ta, srcB, tb) {
  let i = 0;
  let j = 0;
  let prevString = false;
  for (;;) {
    let lfA = false;
    let lfB = false;
    while (i < ta.length && ta[i].kind === TOKEN.BLANK) {
      if (!lfA && hasLf(srcA, ta[i].start, ta[i].end)) lfA = true;
      i++;
    }
    while (j < tb.length && tb[j].kind === TOKEN.BLANK) {
      if (!lfB && hasLf(srcB, tb[j].start, tb[j].end)) lfB = true;
      j++;
    }
    if (i >= ta.length || j >= tb.length) return i >= ta.length && j >= tb.length;
    const x = ta[i];
    const y = tb[j];
    const len = x.end - x.start;
    if (x.kind !== y.kind || x.closed !== y.closed || len !== y.end - y.start || !sameRun(srcA, x.start, srcB, y.start, len)) return false;
    if (prevString && x.kind === TOKEN.STRING && lfA !== lfB) return false;
    prevString = x.kind === TOKEN.STRING;
    i++;
    j++;
  }
}

/** Makes the result of a format from the old text and the new text. `from` and `to` are the offsets of the part. */
function finish(src, out, reason = null) {
  if (out === src) return { text: src, changed: false, reason, at: -1, line: 0, column: 0, lines: 0, totalLines: lineCount(src), replace: null };
  // The part that differs: the common start and the common end are cut (no cut inside a surrogate pair).
  const max = Math.min(src.length, out.length);
  let p = 0;
  while (p < max && src.charCodeAt(p) === out.charCodeAt(p)) p++;
  let s = 0;
  while (s < max - p && src.charCodeAt(src.length - 1 - s) === out.charCodeAt(out.length - 1 - s)) s++;
  const isHigh = (c) => c >= 0xd800 && c <= 0xdbff;
  if (p > 0 && isHigh(src.charCodeAt(p - 1))) p--;
  if (s > 0 && isHigh(out.charCodeAt(out.length - s - 1))) s--;
  const text = out.slice(p, out.length - s);
  return {
    text: out,
    changed: true,
    reason: null,
    at: -1,
    line: 0,
    column: 0,
    lines: lineCount(text),
    totalLines: lineCount(out),
    replace: { from: p, to: src.length - s, text },
  };
}

// ---- The tree annotations of the SQL formatter. ----

const F_STMT = 1;
const F_CLAUSE = 8;
const F_SUBQ = 16;
const F_RAW = 32;
const F_ONE = 64; // a subquery of one clause
const F_FILL = 128; // a bracket whose items are all literals, variables or macros

/** The clause names that are lists: each item goes on its own line when the clause does not fit. */
const LIST_NAMES = new Set(["SELECT", "FROM", "GROUP BY", "ORDER BY", "SETTINGS", "WINDOW", "VALUES"]);

/** The clause names that hold conditions: AND and OR can break the line. */
const COND_NAMES = new Set(["WHERE", "PREWHERE", "HAVING", "QUALIFY"]);

/** The bits of a clause (no list bit): the keyword count in the high bits, 2 for a condition, 4 for a join. */
function clauseBase(name) {
  let words = 0;
  if (name.length > 0) words = name === "LIMIT BY" ? 1 : name.split(" ").length;
  let bits = words << 4;
  if (COND_NAMES.has(name)) bits |= 2;
  if (name.endsWith("JOIN") && !name.endsWith("ARRAY JOIN")) bits |= 6;
  return bits;
}

/** True when the clause name is a list name. */
function isListName(name) {
  return LIST_NAMES.has(name) || name.endsWith("ARRAY JOIN");
}

/** True when the clause has a list child: it has a comma at its top level. */
function hasListChild(nd) {
  const ch = nd.children;
  for (let k = 0; k < ch.length; k++) {
    if (ch[k].type === SQL_NODE.LIST) return true;
    if (ch[k].type !== SQL_NODE.COMMENT) return false;
  }
  return false;
}

/** A unit from 0x80: it has two bytes or more in UTF-8. */
const NON_ASCII = /[^\x00-\x7f]/;

/** True when the text has more UTF-8 bytes than the cap of the input. */
function overCap(s) {
  if (s.length * 3 <= MAX_AST_INPUT_BYTES) return false;
  if (!NON_ASCII.test(s)) return s.length > MAX_AST_INPUT_BYTES;
  return utf8Length(s) > MAX_AST_INPUT_BYTES;
}

/** True for the kinds of a token that a lexer can join to a touching token: words, names, numbers, strings and placeholders. */
function isWordish(kind) {
  return (
    kind === TOKEN.WORD || kind === TOKEN.NAME || kind === TOKEN.NUMBER || kind === TOKEN.STRING || kind === TOKEN.MACRO ||
    kind === TOKEN.VARIABLE || kind === TOKEN.PARAM || kind === TOKEN.OTHER
  );
}

/** The frame kinds of the builder of SQL. */
const FR_CLAUSE = 1;
const FR_BR = 2;
const FR_CASE = 3;

/** Makes a frame. All frames have the same shape. */
function frame(k) {
  return {
    k, list: false, cond: false, join: false, ind: false, g2: false, g3: false, bw: false, sub: false, group: false, one: false,
    fill: false, opened: false, cont: 0,
  };
}

/** The codes of the words that the layout reads. */
const W_AND = 1;
const W_OR = 2;
const W_ON = 3;
const W_USING = 4;
const W_CASE = 5;
const W_WHEN = 6;
const W_ELSE = 7;
const W_END = 8;
const W_BETWEEN = 9;

/** True when the lower-case word `w` is the text src[s, s + w.length) in any letter case. The word has letters only. */
function isWord(src, s, w) {
  for (let k = 0; k < w.length; k++) if ((src.charCodeAt(s + k) | 32) !== w.charCodeAt(k)) return false;
  return true;
}

/** Gives the code of a word of the layout, or 0. No string is made. */
function wordCode(src, s, e) {
  const len = e - s;
  switch (len) {
    case 2:
      if (isWord(src, s, "or")) return W_OR;
      return isWord(src, s, "on") ? W_ON : 0;
    case 3:
      if (isWord(src, s, "and")) return W_AND;
      return isWord(src, s, "end") ? W_END : 0;
    case 4:
      if (isWord(src, s, "case")) return W_CASE;
      if (isWord(src, s, "when")) return W_WHEN;
      return isWord(src, s, "else") ? W_ELSE : 0;
    case 5:
      return isWord(src, s, "using") ? W_USING : 0;
    case 7:
      return isWord(src, s, "between") ? W_BETWEEN : 0;
    default:
      return 0;
  }
}

/** True when the token is a punctuation token of one unit with this code. */
function isPunct(src, t, code) {
  return t.kind === TOKEN.PUNCT && t.end - t.start === 1 && src.charCodeAt(t.start) === code;
}

// ---- The builder of the layout of SQL. ----

/**
 * Builds the commands for the tokens lo..hi of a SQL text. `flag` and `aux` hold the annotations of the tree, `hc` the
 * brackets that have a comma at their top level. The function never throws for a text of tokens that cover the text.
 */
function buildSql(src, tokens, lo, hi, flag, aux, hc) {
  const writer = newWriter((hi - lo) * 3 + 64);
  const emit = (op, x = 0, y = 0) => writer.emit(op, x, y);
  const frames = [];
  let top = null;
  let brDepth = 0;
  let pend = P_NONE;
  let forced = false;
  let started = false;
  let atScript = true;
  let noRule = true;
  let kwLeft = 0;
  let kwFirst = false;
  let kwFrame = null;
  let closedFor = -1;
  let ncIdx = -1;
  let prevString = false;
  let touching = false;

  const pendUp = (level) => {
    if (level > pend) pend = level;
  };

  // Opens the one indent of a condition clause, at its first line break inside the body.
  const ensureIn = () => {
    if (top !== null && top.k === FR_CLAUSE && !top.list && !top.ind) {
      emit(OP_IN);
      top.ind = true;
    }
  };

  // Writes a separator.
  const flush = (w) => {
    pend = P_NONE;
    forced = false;
    if (w === P_NONE || !started) return;
    if (w === P_SP) emit(OP_SP);
    else if (w === P_SOFT) {
      ensureIn();
      emit(OP_SOFT);
    } else if (w === P_FILL) emit(OP_FILL);
    else if (w === P_LINE) {
      ensureIn();
      emit(OP_LINE);
    } else {
      ensureIn();
      emit(w === P_HARD2 ? OP_HARD2 : OP_HARD);
    }
  };

  // The separator before a token: the waiting one, the own one of the token, or the space of the source.
  const sep = (glue, own, blankBefore) => {
    if (touching) {
      // Two tokens that touch stay joined: a blank could change how a lexer reads them (a `$` in a word, `x$1`).
      flush(P_NONE);
      return;
    }
    let w = pend;
    if (glue && !forced) w = P_NONE;
    if (own > w) w = own;
    if (w === P_NONE && !glue && !noRule && blankBefore) w = P_SP;
    flush(w);
  };

  // Writes the text of a token.
  const text = (i) => {
    const t = tokens[i];
    const k = t.kind;
    if (k === TOKEN.COMMENT) {
      const block = src.charCodeAt(t.start) === 47 && src.charCodeAt(t.start + 1) === 42;
      emit(block ? (hasLf(src, t.start, t.end) ? OP_TM : OP_T) : OP_TC, t.start, t.end);
    } else if ((k === TOKEN.STRING || k === TOKEN.NAME) && hasLf(src, t.start, t.end)) emit(OP_TM, t.start, t.end);
    else emit(OP_T, t.start, t.end);
    started = true;
    noRule = false;
  };

  const rawText = (s, e) => {
    emit(hasLf(src, s, e) ? OP_TM : OP_T, s, e);
    started = true;
    noRule = false;
  };

  // Ends the continuation indents that a line comment opened in a frame.
  const unwind = (f) => {
    while (f.cont > 0) {
      emit(OP_OUT, 0, 0);
      f.cont -= 1;
    }
  };

  // Takes the top frame off the stack and writes the commands that close it.
  const closeFrame = () => {
    const f = frames.pop();
    top = frames.length ? frames[frames.length - 1] : null;
    unwind(f);
    switch (f.k) {
      case FR_CLAUSE:
        if (f.g3) {
          emit(OP_GE, 0, 0);
          emit(OP_OUT, 0, 0);
        }
        if (f.g2) emit(OP_GE, 0, 0);
        if (f.opened) {
          emit(OP_OUT, 0, 0);
          emit(OP_GE, 0, 0);
        } else if (f.ind) emit(OP_OUT, 0, 0);
        break;
      case FR_CASE:
        emit(OP_OUT, 0, 0);
        emit(OP_GE, 0, 0);
        break;
      default:
        if (f.sub || f.group) emit(OP_OUT, 0, 0);
        if (f.group) emit(OP_GE, 0, 0);
        brDepth -= 1;
        break;
    }
  };

  const push = (f) => {
    frames.push(f);
    top = f;
  };

  // Closes the clause frames (and a CASE) up to the container of a new clause.
  const closeClauseFrames = () => {
    while (top !== null && (top.k === FR_CLAUSE || top.k === FR_CASE)) closeFrame();
  };

  const closeAll = () => {
    while (top !== null) closeFrame();
  };

  // The start of the body of a clause, after the last word of its keyword.
  const bodyStart = (f) => {
    if (f.list) {
      emit(OP_GS, 0, 0);
      emit(OP_IN, 0, 0);
      f.opened = true;
      pend = P_LINE;
    } else pend = P_SP;
    forced = false;
  };

  // The index of the next token that is not a blank and not a comment, from i.
  const nextCode = (i) => {
    if (ncIdx > i) return ncIdx;
    let j = i + 1;
    while (j < hi && (tokens[j].kind === TOKEN.BLANK || tokens[j].kind === TOKEN.COMMENT)) j++;
    ncIdx = j;
    return j;
  };

  // True when there is a blank line (two line feeds or more) in the blank token before the token i.
  const gapBlank = (i) => i > lo && tokens[i - 1].kind === TOKEN.BLANK && countLf(src, tokens[i - 1].start, tokens[i - 1].end) >= 2;

  const doComment = (i) => {
    const t = tokens[i];
    const block = src.charCodeAt(t.start) === 47 && src.charCodeAt(t.start + 1) === 42;
    const prev = i > lo ? tokens[i - 1] : null;
    const own = started && prev !== null && prev.kind === TOKEN.BLANK && hasLf(src, prev.start, prev.end);
    const waiting = pend;
    if (own) {
      const nx = nextCode(i);
      if (nx >= hi) closeAll();
      else if ((flag[nx] & F_CLAUSE) !== 0 && closedFor !== nx) {
        closeClauseFrames();
        closedFor = nx;
      }
      let w = P_HARD;
      if (atScript && gapBlank(i)) w = P_HARD2;
      flush(pend > w ? pend : w);
    } else if (started) {
      const savedPend = pend;
      const savedForced = forced;
      flush(P_SP);
      pend = savedPend;
      forced = savedForced;
    }
    text(i);
    prevString = false;
    if (!block) {
      pendUp(P_HARD);
      forced = true;
      // A token that goes on the item after a line comment is 2 spaces deeper than the line of the item.
      if (waiting <= P_SP && top !== null && kwLeft === 0 && !(top.k === FR_CLAUSE && !top.list)) {
        const nx = nextCode(i);
        if (nx < hi && (flag[nx] & (F_CLAUSE | F_STMT)) === 0) {
          const nt = tokens[nx];
          const c = nt.kind === TOKEN.PUNCT && nt.end - nt.start === 1 ? src.charCodeAt(nt.start) : 0;
          if (c !== 44 && c !== 41 && c !== 93 && c !== 59) {
            emit(OP_IN, 0, 0);
            top.cont += 1;
          }
        }
      }
    } else {
      const next = i + 1 < hi ? tokens[i + 1] : null;
      if (next !== null && next.kind === TOKEN.BLANK && hasLf(src, next.start, next.end)) {
        pendUp(P_HARD);
        forced = true;
      } else pendUp(P_SP);
    }
  };

  // True for the operators that have one space on each side: = == <> != < <= > >= ||.
  const spacedOp = (t) => {
    const len = t.end - t.start;
    const c0 = src.charCodeAt(t.start);
    if (len === 1) return c0 === 61 || c0 === 60 || c0 === 62;
    if (len !== 2) return false;
    const c1 = src.charCodeAt(t.start + 1);
    if (c1 === 61) return c0 === 61 || c0 === 33 || c0 === 60 || c0 === 62;
    return (c0 === 60 && c1 === 62) || (c0 === 124 && c1 === 124);
  };

  let i = lo;
  while (i < hi) {
    const t = tokens[i];
    const k = t.kind;
    if (k === TOKEN.BLANK) {
      i++;
      continue;
    }
    if (k === TOKEN.COMMENT) {
      doComment(i);
      i++;
      continue;
    }
    const f = flag[i];
    const blankBefore = i > lo && tokens[i - 1].kind === TOKEN.BLANK;
    touching = !blankBefore && i > lo && isWordish(tokens[i - 1].kind) && isWordish(k);
    const isSemi = isPunct(src, t, 59);
    if ((f & F_STMT) !== 0) {
      if (started && !isSemi) pendUp(gapBlank(i) ? P_HARD2 : P_HARD);
      atScript = false;
    }
    if ((f & F_CLAUSE) !== 0) {
      closeClauseFrames();
      if (!(top !== null && top.k === FR_BR && top.one)) pendUp(P_HARD);
      flush(touching ? P_NONE : pend);
      const bits = aux[i];
      const fr = frame(FR_CLAUSE);
      fr.list = (bits & 1) !== 0;
      fr.cond = (bits & 2) !== 0;
      fr.join = (bits & 4) !== 0;
      push(fr);
      kwLeft = bits >> 4;
      kwFirst = true;
      kwFrame = fr;
      noRule = true;
      if (kwLeft === 0) kwFrame = null;
    }

    // The own separator of the token and the actions before it.
    let own = P_NONE;
    let isKeyword = false;
    let afterOn = false;
    if (kwLeft > 0 && k === TOKEN.WORD) {
      isKeyword = true;
      if (!kwFirst) own = P_SP;
    }
    const code = k === TOKEN.PUNCT && t.end - t.start === 1 ? src.charCodeAt(t.start) : 0;
    if ((f & F_RAW) !== 0) {
      const end = aux[i];
      sep(false, P_NONE, blankBefore);
      rawText(t.start, end);
      let j = i + 1;
      while (j < hi && tokens[j].start < end) j++;
      i = j;
      prevString = false;
      if ((f & F_STMT) !== 0) atScript = src.charCodeAt(end - 1) === 59;
      continue;
    }
    if (k === TOKEN.STRING && prevString && blankBefore && hasLf(src, tokens[i - 1].start, tokens[i - 1].end)) {
      // A line end between two strings is a part of the syntax (one constant in PostgreSQL): it stays.
      own = P_HARD;
    }
    if (k === TOKEN.WORD && !isKeyword && !(i > lo && isPunct(src, lastCodeToken(tokens, i, lo), 46))) {
      const w = wordCode(src, t.start, t.end);
      if (w !== 0) {
        if (w === W_AND) {
          if (top !== null && top.bw) top.bw = false;
          else if (top !== null && top.k === FR_CLAUSE && top.cond) {
            if (!top.g2) {
              emit(OP_GS, 0, 0);
              top.g2 = true;
            }
            own = P_LINE;
          }
        } else if (w === W_OR) {
          if (top !== null && top.k === FR_CLAUSE && top.cond) {
            if (!top.g2) {
              emit(OP_GS, 0, 0);
              top.g2 = true;
            }
            own = P_LINE;
          }
        } else if (w === W_ON || w === W_USING) {
          if (top !== null && top.k === FR_CLAUSE && top.join && !top.g2) {
            emit(OP_GS, 0, 0);
            top.g2 = true;
            own = P_LINE;
            afterOn = true;
          }
        } else if (w === W_BETWEEN) {
          if (top !== null) top.bw = true;
        } else if (w === W_CASE) {
          sep(false, P_NONE, blankBefore);
          emit(OP_GS, 0, 0);
          text(i);
          emit(OP_IN, 0, 0);
          push(frame(FR_CASE));
          prevString = false;
          i++;
          continue;
        } else if (w === W_WHEN || w === W_ELSE) {
          if (top !== null && top.k === FR_CASE) own = P_LINE;
        } else if (w === W_END) {
          if (top !== null && top.k === FR_CASE) {
            unwind(top);
            emit(OP_OUT, 0, 0);
            sep(false, P_LINE, blankBefore);
            text(i);
            emit(OP_GE, 0, 0);
            frames.pop();
            top = frames.length ? frames[frames.length - 1] : null;
            prevString = false;
            i++;
            continue;
          }
        }
      }
    }
    if (code === 40 || code === 91) {
      // An opening bracket.
      sep(false, own, blankBefore);
      const isQuery = (f & F_SUBQ) !== 0;
      const one = isQuery && (f & F_ONE) !== 0;
      const multi = isQuery && !one;
      const group = one || (!multi && hc[i] === 1);
      if (group) emit(OP_GS, 0, 0);
      text(i);
      const br = frame(FR_BR);
      br.sub = multi;
      br.group = group;
      br.one = one;
      br.fill = group && !one && (f & F_FILL) !== 0;
      push(br);
      brDepth += 1;
      if (multi) {
        emit(OP_IN, 0, 0);
        pend = P_HARD;
      } else if (group) {
        emit(OP_IN, 0, 0);
        pend = P_SOFT;
      } else noRule = true;
      prevString = false;
      i++;
      continue;
    }
    if ((code === 41 || code === 93) && brDepth > 0) {
      let j = frames.length - 1;
      while (j >= 0 && frames[j].k !== FR_BR) j--;
      if (j >= 0) {
        while (frames.length - 1 > j) closeFrame();
        const br = top;
        unwind(br);
        if (br.sub) {
          emit(OP_OUT, 0, 0);
          sep(true, P_HARD, blankBefore);
          text(i);
        } else if (br.group) {
          emit(OP_OUT, 0, 0);
          sep(true, P_SOFT, blankBefore);
          text(i);
          emit(OP_GE, 0, 0);
        } else {
          sep(true, P_NONE, blankBefore);
          text(i);
        }
        frames.pop();
        top = frames.length ? frames[frames.length - 1] : null;
        brDepth -= 1;
        prevString = false;
        i++;
        continue;
      }
    }
    if (code === 44) {
      sep(true, P_NONE, blankBefore);
      text(i);
      forced = false;
      if (top !== null) unwind(top);
      if (top !== null && top.k === FR_CLAUSE && top.list) pend = P_LINE;
      else if (top !== null && top.k === FR_BR && top.group) pend = top.fill ? P_FILL : P_LINE;
      else pend = P_SP;
      prevString = false;
      i++;
      continue;
    }
    if (isSemi && brDepth === 0) {
      closeAll();
      sep(true, P_NONE, blankBefore);
      text(i);
      atScript = true;
      pend = P_NONE;
      prevString = false;
      i++;
      continue;
    }
    const spaced = k === TOKEN.OPERATOR && spacedOp(t);
    if (spaced && own < P_SP) own = P_SP;
    sep(false, own, blankBefore);
    text(i);
    if (afterOn) {
      emit(OP_IN, 0, 0);
      emit(OP_GS, 0, 0);
      top.g3 = true;
    }
    if (spaced) pendUp(P_SP);
    prevString = k === TOKEN.STRING;
    if (isKeyword) {
      kwLeft -= 1;
      kwFirst = false;
      if (kwLeft === 0 && kwFrame !== null) {
        bodyStart(kwFrame);
        kwFrame = null;
      }
    }
    i++;
  }
  closeAll();
  return writer.end();
}

/** The last token before the index `i` that is not a blank (the token itself is not read when none exists). */
function lastCodeToken(tokens, i, lo) {
  let j = i - 1;
  while (j > lo && tokens[j].kind === TOKEN.BLANK) j--;
  return tokens[j];
}

/** Marks the brackets that have a comma at their top level, for the tokens lo..hi. */
function markCommas(src, tokens, lo, hi) {
  const hc = new Uint8Array(tokens.length);
  const stack = [];
  for (let i = lo; i < hi; i++) {
    const t = tokens[i];
    if (t.kind !== TOKEN.PUNCT || t.end - t.start !== 1) continue;
    const c = src.charCodeAt(t.start);
    if (c === 40 || c === 91 || c === 123) stack.push(i);
    else if (c === 41 || c === 93 || c === 125) stack.pop();
    else if (c === 44 && stack.length) hc[stack[stack.length - 1]] = 1;
  }
  return hc;
}

/**
 * Reads the tree of a JSON part: gives the offset of the first fault (an error node or a node with `error`), or -1, and marks
 * in `fill` the open bracket of each array whose items are all scalars (strings, numbers and literals).
 */
function annotateJson(part, startTok, fill) {
  const stack = [part];
  let fault = -1;
  while (stack.length) {
    const nd = stack.pop();
    const ch = nd.children;
    for (let k = ch.length - 1; k >= 0; k--) stack.push(ch[k]);
    if ((nd.type === JSON_NODE.ERROR || nd.error) && (fault < 0 || nd.start < fault)) fault = nd.start;
    if (nd.type === JSON_NODE.ARRAY && ch.length > 0 && nd.open !== undefined) {
      let scalar = true;
      for (let k = 0; k < ch.length; k++) {
        const t = ch[k].type;
        if (t !== JSON_NODE.STRING && t !== JSON_NODE.NUMBER && t !== JSON_NODE.LITERAL) {
          scalar = false;
          break;
        }
      }
      const at = scalar ? startTok[nd.open] : -1;
      if (at >= 0) fill[at] = 1;
    }
  }
  return fault;
}

/**
 * Formats a SQL text. `dialect` is "clickhouse" or "postgres" (another value reads as "clickhouse"). `opts` has `indent` (2),
 * `width` (80) and `range` (null, or `[start, end]`: only the statements that the range covers are formatted, and the
 * result holds the whole text).
 *
 * The result is `{ text, changed, reason, at, lines, totalLines, replace }`.
 * - `text` is the whole new text (the input when nothing changes).
 * - `reason` is null when the text is formatted (also when it was already in the style). Else it is the reason why nothing
 *   was formatted: `capped_too_large`, `capped_too_deep`, `capped_too_many_nodes`, `output_too_large` (the new text would pass the cap of the input), `error_node` (a node of the tree has a
 *   fault in the part), `unclosed` (a string, a quoted name or a comment that the text ends first), `no_statement` (the range
 *   covers no statement) or `self_check` (the check of the tokens failed: a bug of the formatter, the input stays).
 *   `at` is the offset of the fault when there is one, else -1.
 * - `replace` is `{ from, to, text }`: the least change that turns the input into `text`. `lines` is the number of lines of
 *   `replace.text`: the editor uses it to choose the way of the write. `totalLines` is the number of lines of `text`.
 * The call never throws.
 */
export function formatSql(text, dialect = "clickhouse", opts = null) {
  const src = typeof text === "string" ? text : String(text ?? "");
  try {
    return formatSqlInner(src, dialect === "postgres" ? "postgres" : "clickhouse", readOptions(opts));
  } catch {
    return refusal(src, "self_check");
  }
}

/** True when the first token of a statement is the word SELECT, WITH or VALUES, in any letter case. */
function isFormattable(src, t) {
  if (t.kind !== TOKEN.WORD) return false;
  const len = t.end - t.start;
  if (len === 4) return isWord(src, t.start, "with");
  if (len === 6) return isWord(src, t.start, "select") || isWord(src, t.start, "values");
  return false;
}

/** True when the node is a literal, a variable or a macro that is alone in its item (an item that fills a line). */
function isScalarItem(nd) {
  if (nd.type !== SQL_NODE.EXPR || nd.children.length !== 1) return false;
  const c = nd.children[0].type;
  return c === SQL_NODE.LITERAL || c === SQL_NODE.MACRO || c === SQL_NODE.VARIABLE;
}

/** True when a bracket node has a list child and each item of it is a scalar item. */
function hasScalarItems(nd) {
  for (const c of nd.children) {
    if (c.type !== SQL_NODE.LIST) continue;
    for (const it of c.children) if (!isScalarItem(it)) return false;
    return c.children.length > 0;
  }
  return false;
}

/**
 * Marks the tokens of one statement that the format reads, with the nodes of its tree. Gives the offset of the first fault of
 * the tree in the statement, or -1. `flag` and `aux` are the arrays of the annotations (indexed by token).
 */
function annotateStatement(stmt, startTok, flag, aux, bases) {
  const stack = [stmt];
  let fault = -1;
  while (stack.length) {
    const nd = stack.pop();
    const ch = nd.children;
    for (let k = ch.length - 1; k >= 0; k--) stack.push(ch[k]);
    if ((nd.type === SQL_NODE.ERROR || nd.error) && (fault < 0 || nd.start < fault)) fault = nd.start;
    let at;
    switch (nd.type) {
      case SQL_NODE.WITH: {
        at = startTok[nd.start];
        if (at < 0) break;
        let items = 0;
        for (const c of ch) if (c.type === SQL_NODE.CTE) items++;
        flag[at] |= F_CLAUSE;
        aux[at] = ((nd.name === "WITH RECURSIVE" ? 2 : 1) << 4) | (items >= 2 ? 1 : 0);
        break;
      }
      case SQL_NODE.CLAUSE: {
        at = startTok[nd.start];
        if (at < 0) break;
        const name = nd.name || "";
        let bits = bases.get(name);
        if (bits === undefined) {
          bits = clauseBase(name);
          bases.set(name, bits);
        }
        if (isListName(name) && hasListChild(nd)) bits |= 1;
        flag[at] |= F_CLAUSE;
        aux[at] = bits;
        break;
      }
      case SQL_NODE.SUBQUERY: {
        at = startTok[nd.open];
        if (at < 0) break;
        flag[at] |= F_SUBQ;
        let clauses = 0;
        let withs = 0;
        for (const c of ch) {
          if (c.type === SQL_NODE.WITH) withs++;
          else if (c.type === SQL_NODE.QUERY) for (const q of c.children) if (q.type === SQL_NODE.CLAUSE) clauses++;
        }
        if (withs === 0 && clauses <= 1) flag[at] |= F_ONE;
        break;
      }
      case SQL_NODE.MACRO:
        if (nd.open !== undefined) {
          at = startTok[nd.start];
          if (at >= 0) {
            flag[at] |= F_RAW;
            aux[at] = nd.end;
          }
        }
        break;
      case SQL_NODE.PAREN:
      case SQL_NODE.CALL:
      case SQL_NODE.ARRAY:
        if (nd.bracket === "{" && nd.type === SQL_NODE.PAREN) {
          at = startTok[nd.start];
          if (at >= 0) {
            flag[at] |= F_RAW;
            aux[at] = nd.end;
          }
        } else if (nd.open !== undefined && hasScalarItems(nd)) {
          at = startTok[nd.open];
          if (at >= 0) flag[at] |= F_FILL;
        }
        break;
      default:
        break;
    }
  }
  return fault;
}

/** The body of `formatSql`. */
function formatSqlInner(src, dialect, o) {
  const tokenize = dialect === "postgres" ? tokensPostgres : tokensClickHouse;
  const tokens = tokenize(src);
  const n = tokens.length;
  let nonBlank = false;
  for (let k = 0; k < n; k++) {
    if (tokens[k].kind !== TOKEN.BLANK) {
      nonBlank = true;
      break;
    }
  }
  if (!nonBlank) return finish(src, src, "nothing_to_format");
  const res = parseSql(src, dialect, tokens);
  if (res.capped) return refusal(src, `capped_${res.capped.reason}`, res.capped.at);
  const root = res.root;

  // The part: the whole text, or the statements that the range covers.
  const statements = [];
  for (const s of root.children) if (s.type === SQL_NODE.STATEMENT) statements.push(s);
  let first = 0;
  let last = statements.length - 1;
  if (o.range !== null) {
    const [rs, re] = o.range;
    first = -1;
    for (let k = 0; k < statements.length; k++) {
      const s = statements[k];
      const hit = rs === re ? s.start <= rs && rs <= s.end : s.start < re && s.end > rs;
      if (hit) {
        if (first < 0) first = k;
        last = k;
      }
    }
    if (first < 0) return refusal(src, "no_statement");
  }
  if (first > last) return finish(src, src, "nothing_to_format");
  const from = o.range === null ? 0 : statements[first].start;
  const to = o.range === null ? src.length : statements[last].end;
  const startTok = new Int32Array(src.length + 1).fill(-1);
  for (let k = 0; k < n; k++) startTok[tokens[k].start] = k;
  startTok[src.length] = n;
  const lo = o.range === null ? 0 : startTok[from];
  const hi = o.range === null ? n : startTok[to];
  if (lo < 0 || hi < 0) return refusal(src, "self_check");

  // The annotations. A statement that does not start with SELECT, WITH or VALUES stays as it is.
  const flag = new Uint8Array(n);
  const aux = new Int32Array(n);
  const bases = new Map();
  let formatted = 0;
  for (let k = first; k <= last; k++) {
    const s = statements[k];
    const at = startTok[s.start];
    if (at < 0) return refusal(src, "self_check");
    flag[at] |= F_STMT;
    if (!isFormattable(src, tokens[at])) {
      flag[at] |= F_RAW;
      aux[at] = s.end;
      continue;
    }
    formatted += 1;
    const fault = annotateStatement(s, startTok, flag, aux, bases);
    if (fault >= 0) return refusal(src, "error_node", fault);
  }
  // A string, a name or a comment that has no end, outside the statements that stay as they are.
  for (let j = lo; j < hi; j++) {
    if ((flag[j] & (F_RAW | F_STMT)) === (F_RAW | F_STMT)) {
      while (j + 1 < hi && tokens[j + 1].start < aux[j]) j++;
    } else if (!tokens[j].closed) return refusal(src, "unclosed", tokens[j].start);
  }
  if (formatted === 0) return finish(src, src, "nothing_to_format");

  const hc = markCommas(src, tokens, lo, hi);
  const doc = buildSql(src, tokens, lo, hi, flag, aux, hc);
  const eol = eolOf(src);
  let col0 = 0;
  if (from > 0) {
    const nl = src.lastIndexOf("\n", from - 1);
    col0 = from - nl - 1;
  }
  const part = render(src, doc, { indent: o.indent, width: o.width, base: 0, col0, eol });
  if (part === null) return refusal(src, "output_too_large");
  let out;
  if (o.range === null) {
    // The leading blanks go. The trailing line end stays, as one line end.
    const lastTok = tokens[n - 1];
    const tail = lastTok.kind === TOKEN.BLANK && hasLf(src, lastTok.start, lastTok.end) ? eol : "";
    out = part + tail;
  } else out = src.slice(0, from) + part + src.slice(to);
  if (out === src) return finish(src, src);
  if (overCap(out)) return refusal(src, "output_too_large");
  const check = tokenize(out);
  if (!sameTokens(src, tokens, out, check)) return refusal(src, "self_check");
  return finish(src, out);
}

// ---- The JSON formatter. ----

/** Gives the start offset of the line of `at` and the indent of this line (the count of leading blank units). */
function lineIndent(src, at) {
  const s = at === 0 ? 0 : src.lastIndexOf("\n", at - 1) + 1;
  let k = s;
  while (k < at && (src.charCodeAt(k) === 32 || src.charCodeAt(k) === 9)) k++;
  return { lineStart: s, indent: k - s };
}

/** Builds the commands for the tokens lo..hi of a JSON text with no fault. */
function buildJson(src, tokens, lo, hi, fill) {
  const writer = newWriter((hi - lo) * 2 + 64);
  const emit = (op, x = 0, y = 0) => writer.emit(op, x, y);
  let pend = P_NONE;
  let started = false;
  let depth = 0;
  const fills = [];
  const flush = (w) => {
    pend = P_NONE;
    if (w === P_NONE || !started) return;
    emit(w === P_SP ? OP_SP : w === P_SOFT ? OP_SOFT : w === P_FILL ? OP_FILL : w === P_LINE ? OP_LINE : OP_HARD, 0, 0);
  };
  const text = (t) => {
    emit(t.kind === TOKEN.STRING && hasLf(src, t.start, t.end) ? OP_TM : OP_T, t.start, t.end);
    started = true;
  };
  let i = lo;
  while (i < hi) {
    const t = tokens[i];
    if (t.kind === TOKEN.BLANK) {
      i++;
      continue;
    }
    const blankBefore = i > lo && tokens[i - 1].kind === TOKEN.BLANK;
    const c = t.kind === TOKEN.PUNCT ? src.charCodeAt(t.start) : 0;
    if (c === 123 || c === 91) {
      flush(pend !== P_NONE ? pend : blankBefore && started ? P_SP : P_NONE);
      let nx = i + 1;
      while (nx < hi && tokens[nx].kind === TOKEN.BLANK) nx++;
      if (nx < hi && tokens[nx].kind === TOKEN.PUNCT && src.charCodeAt(tokens[nx].start) === c + 2) {
        text(t);
        text(tokens[nx]);
        i = nx + 1;
        continue;
      }
      emit(OP_GS, 0, 0);
      text(t);
      emit(OP_IN, 0, 0);
      depth += 1;
      fills.push(c === 91 && fill[i] === 1);
      pend = P_SOFT;
    } else if ((c === 125 || c === 93) && depth > 0) {
      emit(OP_OUT, 0, 0);
      flush(P_SOFT);
      text(t);
      emit(OP_GE, 0, 0);
      depth -= 1;
      fills.pop();
    } else if (c === 44) {
      flush(P_NONE);
      text(t);
      pend = fills.length > 0 && fills[fills.length - 1] ? P_FILL : P_LINE;
    } else if (c === 58) {
      flush(P_NONE);
      text(t);
      pend = P_SP;
    } else {
      flush(pend !== P_NONE ? pend : blankBefore && started ? P_SP : P_NONE);
      text(t);
    }
    i++;
  }
  while (depth-- > 0) {
    emit(OP_OUT, 0, 0);
    emit(OP_GE, 0, 0);
  }
  return writer.end();
}

/**
 * Formats the text of a MongoDB pipeline (JSON). `opts` is as for `formatSql`. With `range`, the innermost object or array
 * that holds the range is formatted in place, with the indent of its line; the result holds the whole text.
 * The result has the shape of `formatSql`. The reasons are the same, with `error_node` for a comment, a placeholder outside a
 * string, a missing comma and the other faults of the JSON reader, and `no_statement` when the range holds no object or array.
 * A text that is not JSON is never formatted. The call never throws.
 */
export function formatJson(text, opts = null) {
  const src = typeof text === "string" ? text : String(text ?? "");
  try {
    return formatJsonInner(src, readOptions(opts));
  } catch {
    return refusal(src, "self_check");
  }
}

/** The body of `formatJson`. */
function formatJsonInner(src, o) {
  const res = parseJson(src);
  const tokens = res.tokens;
  const n = tokens.length;
  let nonBlank = false;
  for (let k = 0; k < n; k++) {
    if (tokens[k].kind !== TOKEN.BLANK) {
      nonBlank = true;
      break;
    }
  }
  if (!nonBlank) return finish(src, src, "nothing_to_format");
  if (res.capped) return refusal(src, `capped_${res.capped.reason}`, res.capped.at);
  let from = 0;
  let to = src.length;
  let part = res.root;
  if (o.range !== null) {
    const [rs, re] = o.range;
    // The innermost object or array that holds the range.
    let best = null;
    let cur = res.root;
    for (;;) {
      let next = null;
      for (const c of cur.children) {
        if (c.start <= rs && c.end >= re && c.type !== JSON_NODE.ERROR) {
          next = c;
          break;
        }
      }
      if (next === null) break;
      if (next.type === JSON_NODE.OBJECT || next.type === JSON_NODE.ARRAY) best = next;
      cur = next;
    }
    if (best === null) return refusal(src, "no_statement");
    from = best.start;
    to = best.end;
    part = best;
  }
  const startTok = new Int32Array(src.length + 1).fill(-1);
  for (let k = 0; k < n; k++) startTok[tokens[k].start] = k;
  startTok[src.length] = n;
  const fill = new Uint8Array(n);
  const bad = annotateJson(part, startTok, fill);
  if (bad >= 0) return refusal(src, "error_node", bad);
  let lo = startTok[from];
  let hi = startTok[to];
  if (o.range === null) {
    lo = 0;
    hi = n;
  }
  if (lo < 0 || hi < 0) return refusal(src, "self_check");
  for (let k = lo; k < hi; k++) {
    if (!tokens[k].closed) return refusal(src, "unclosed", tokens[k].start);
  }
  const doc = buildJson(src, tokens, lo, hi, fill);
  const eol = eolOf(src);
  let base = 0;
  let col0 = 0;
  if (o.range !== null) {
    const li = lineIndent(src, from);
    base = li.indent;
    col0 = from - li.lineStart;
  }
  const body = render(src, doc, { indent: o.indent, width: o.width, base, col0, eol });
  if (body === null) return refusal(src, "output_too_large");
  let out;
  if (o.range === null) {
    const last = tokens[n - 1];
    out = body + (last.kind === TOKEN.BLANK && hasLf(src, last.start, last.end) ? eol : "");
  } else out = src.slice(0, from) + body + src.slice(to);
  if (out === src) return finish(src, src);
  if (overCap(out)) return refusal(src, "output_too_large");
  if (!sameTokens(src, tokens, out, parseJson(out).tokens)) return refusal(src, "self_check");
  return finish(src, out);
}
