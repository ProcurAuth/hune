// The token stream of the dialect `clickhouse` for the minimal syntax tree of the query editor (F42).
// Pure module: no DOM access. The tree is never a security check. The server stays the only authority on the validity
// of a query, and a fault of this module never stops a save or a run.
// The editor loads this module on demand, so it is not in the page measure of 9.30.
//
// The comments, the strings and the quoted names come from `sqlLex.spans`, the lexer of the highlighter. In this way the
// colours, the selection (B57) and the tree agree. This module reads only the code between two spans.
// The tokens cover the text with no gap and no overlap. A position is a UTF-16 offset, as `selectionStart`.

import { sqlLex } from "../../vendor/chdash/app_sql_lex.js";
import { TOKEN } from "./nodes.js";

// The classes of the ASCII code units for the scan of the code: one table look-up for each unit.
const BLANK = 1;
const WORD_START = 2;
const DIGIT = 3;
const DOLLAR = 4;
const BRACE = 5;
const OPERATOR_UNIT = 6;
const PUNCT_UNIT = 7;
const COLON = 8;
const CLASS = new Uint8Array(128);
for (const c of [32, 9, 10, 13, 12, 11]) CLASS[c] = BLANK;
for (let c = 65; c <= 90; c++) CLASS[c] = WORD_START;
for (let c = 97; c <= 122; c++) CLASS[c] = WORD_START;
CLASS[95] = WORD_START;
for (let c = 48; c <= 57; c++) CLASS[c] = DIGIT;
CLASS[36] = DOLLAR;
CLASS[123] = BRACE;
// The units of a one-unit operator. The "-" and "/" are here: a "--", a "//" or a "/*" never reaches the scan, it is a
// span (7.5).
for (const ch of "+-*/%=<>!&|^~?") CLASS[ch.charCodeAt(0)] = OPERATOR_UNIT;
// The units of a punctuation token, except "{" (a parameter or a bracket) and ":" (a punctuation or a "::").
for (const ch of ",;()[]}.") CLASS[ch.charCodeAt(0)] = PUNCT_UNIT;
CLASS[58] = COLON;

// True for a decimal digit.
function isDigit(c) {
  return c >= 48 && c <= 57;
}

// True for a unit that goes on a word: a letter, a digit, "_", or a unit from 0x80. A "$" is not one: it starts a
// placeholder (7.6), as in `sqlscan.js`.
function isWordChar(c) {
  return c >= 128 || CLASS[c] === WORD_START || CLASS[c] === DIGIT;
}

// True for a unit of the name of a placeholder: `[A-Za-z0-9_]`.
function isNameChar(c) {
  return c < 128 && (CLASS[c] === WORD_START || CLASS[c] === DIGIT);
}

// The braced form of a placeholder, `${name}` or `${name:format}` (7.6). The scan starts at lastIndex.
const BRACED = /\$\{[A-Za-z0-9_]+(?::[A-Za-z0-9_]+)?\}/y;

// A query parameter of ClickHouse, `{name:Type}`. The type has words, digits, parentheses, commas and spaces. It has no quote.
const QUERY_PARAM = /\{\s*[A-Za-z_][A-Za-z0-9_]*\s*:\s*[A-Za-z_][A-Za-z0-9_(), ]*\}/y;

// The end of a number that starts at `at` (limit `to`): letters, digits, "_" and "." with the sign of an exponent.
function numberEnd(s, at, to) {
  const hex = s.charCodeAt(at) === 48 && (s.charCodeAt(at + 1) === 120 || s.charCodeAt(at + 1) === 88);
  let i = at;
  while (i < to) {
    const c = s.charCodeAt(i);
    if (c < 0x80 && (isNameChar(c) || c === 46)) {
      const exponent = hex ? c === 112 || c === 80 : c === 101 || c === 69;
      i += 1;
      if (exponent && i < to && (s.charCodeAt(i) === 43 || s.charCodeAt(i) === 45)) i += 1;
    } else {
      break;
    }
  }
  return i;
}

// The end of the query parameter that starts at `at` with "{" (limit `to`), or -1 when none starts there.
function queryParamEnd(s, at, to) {
  QUERY_PARAM.lastIndex = at;
  const m = QUERY_PARAM.exec(s);
  return m && at + m[0].length <= to ? at + m[0].length : -1;
}

// The token of a placeholder that starts at `at` with "$": the kind and the end. A name that starts with "__" is a macro.
// A "$" with no name is `other`. The braced form that does not match the grammar of 7.6 gives `other` for the "$" alone.
function placeholder(s, at, to) {
  let end = at + 1;
  let name = at + 1;
  if (s.charCodeAt(at + 1) === 123) {
    BRACED.lastIndex = at;
    const m = BRACED.exec(s);
    if (!m || at + m[0].length > to) return [TOKEN.OTHER, at + 1];
    end = at + m[0].length;
    name = at + 2;
  } else {
    while (end < to && isNameChar(s.charCodeAt(end))) end += 1;
    if (end === at + 1) return [TOKEN.OTHER, end];
  }
  const macro = s.charCodeAt(name) === 95 && s.charCodeAt(name + 1) === 95;
  return [macro ? TOKEN.MACRO : TOKEN.VARIABLE, end];
}

// Reads the code of s[from, to) and pushes its tokens. The range holds no comment, no string and no quoted name.
function scanCode(s, from, to, out) {
  let i = from;
  while (i < to) {
    const c = s.charCodeAt(i);
    const cls = c < 128 ? CLASS[c] : WORD_START;
    let kind = TOKEN.OTHER;
    let end = i + 1;
    if (cls === BLANK) {
      while (end < to && s.charCodeAt(end) < 128 && CLASS[s.charCodeAt(end)] === BLANK) end += 1;
      kind = TOKEN.BLANK;
    } else if (cls === WORD_START) {
      while (end < to && isWordChar(s.charCodeAt(end))) end += 1;
      kind = TOKEN.WORD;
    } else if (cls === DIGIT || (c === 46 && i + 1 < to && isDigit(s.charCodeAt(i + 1)))) {
      end = numberEnd(s, i, to);
      kind = TOKEN.NUMBER;
    } else if (cls === DOLLAR) {
      [kind, end] = placeholder(s, i, to);
    } else if (cls === BRACE) {
      const stop = queryParamEnd(s, i, to);
      if (stop > 0) {
        end = stop;
        kind = TOKEN.PARAM;
      } else {
        kind = TOKEN.PUNCT;
      }
    } else if (cls === OPERATOR_UNIT) {
      kind = TOKEN.OPERATOR;
      if (end < to) {
        const d = s.charCodeAt(end);
        // The two-unit operators: -> || <= >= != <> ==
        if ((c === 45 && d === 62) || (c === 124 && d === 124) || (d === 61 && (c === 60 || c === 62 || c === 33 || c === 61)) || (c === 60 && d === 62)) end += 1;
      }
    } else if (cls === COLON) {
      if (end < to && s.charCodeAt(end) === 58) {
        kind = TOKEN.OPERATOR;
        end += 1;
      } else {
        kind = TOKEN.PUNCT;
      }
    } else if (cls === PUNCT_UNIT) {
      kind = TOKEN.PUNCT;
    }
    out.push({ kind, start: i, end, closed: true });
    i = end;
  }
}

/**
 * Cuts a text into tokens with no gap. A token has `kind` (a value of `TOKEN`), `start`, `end` and `closed`.
 * A comment, a string or a quoted name that the text ends before it closes has `closed: false`.
 * A line comment ends before its line end. The line end is part of the next blank token, as in the tokens of PostgreSQL.
 *
 * The operators `->`, `::`, `||` and the comparisons are one token. A macro is `$__name`.
 * A variable is `$name`, `${name}` or `${name:format}`. A parameter is `{name:Type}`.
 * The function is pure and gives the same tokens for the same text. It makes no string for a token. It never throws.
 */
export function tokensClickHouse(text) {
  const s = String(text ?? "");
  const out = [];
  let at = 0;
  for (const span of sqlLex.spans(s)) {
    if (span.start > at) scanCode(s, at, span.start, out);
    let end = span.end;
    let kind;
    let closed = span.closed;
    if (span.kind === sqlLex.LINE || span.kind === sqlLex.BLOCK) {
      kind = TOKEN.COMMENT;
      if (span.kind === sqlLex.LINE) {
        // The line end is not part of a line comment. A CR before the LF stays with the line end.
        closed = true;
        if (span.closed) end -= s.charCodeAt(end - 2) === 13 && end - 2 > span.start ? 2 : 1;
      }
    } else {
      kind = span.kind === sqlLex.STRING ? TOKEN.STRING : TOKEN.NAME;
    }
    out.push({ kind, start: span.start, end, closed });
    at = end;
  }
  if (at < s.length) scanCode(s, at, s.length, out);
  return out;
}
