// The token stream of the dialect `postgres` for the minimal syntax tree of the query editor (F42).
// Pure module: no DOM access. The tree is never a security check. The server stays the only authority on the validity
// of a query, and a fault of this module never stops a save or a run.
// The editor loads this module on demand, so it is not in the page measure of 9.30.
//
// The base is `lexPg` of `sql_lex_pg.js` (7.20), the lexer of the page for this dialect. The comments, the strings, the
// quoted names and the dollar quotes come from it without change. In this way the colours, the selection (B57) and the
// tree agree. This module maps the kinds of `lexPg` to the kinds of `TOKEN`, joins `::` into one operator, and finds the
// macros and the variables in each run of code tokens.
// The tokens cover the text with no gap and no overlap. A position is a UTF-16 offset, as `selectionStart`.
//
// The rule of the placeholders is the rule of `tokens_ch.js` (7.6). The two modules stay in step in three ways. They use
// the same two patterns: `${name}` and `${name:format}` with a name of `[A-Za-z0-9_]`, and `$name` with the longest run
// of these units. They give a macro to a name that starts with `__`. They give `other` to a `$` with no name. The test
// file `ast_pg.test.mjs` runs both modules on the same code and compares the placeholder tokens.
// The one difference is a `$` with digits only, for example `$1`. In PostgreSQL it is a parameter, so this module gives
// the kind `param` to a whole token of lexPg. The module `tokens_ch.js` gives `variable` to it. Inside a word, as in `x$1` or
// `a$$b$$`, a `$` and a name is a variable, because `sqlscan.js` finds it there and the engine expands it.

import { KIND, lexPg } from "../../sql_lex_pg.js";
import { TOKEN } from "./nodes.js";
import { setTokenizer } from "./sql_tree.js";

// The kind of a token of the common stream for each kind of `lexPg`, by an object look-up.
const KIND_MAP = Object.freeze(Object.fromEntries([
  [KIND.BLANK, TOKEN.BLANK],
  [KIND.LINE, TOKEN.COMMENT],
  [KIND.BLOCK, TOKEN.COMMENT],
  [KIND.WORD, TOKEN.WORD],
  [KIND.NAME, TOKEN.NAME],
  [KIND.STRING, TOKEN.STRING],
  [KIND.ESTRING, TOKEN.STRING],
  [KIND.PREFIXED, TOKEN.STRING],
  [KIND.HEREDOC, TOKEN.STRING],
  [KIND.NUMBER, TOKEN.NUMBER],
  [KIND.PUNCT, TOKEN.PUNCT],
  [KIND.OPERATOR, TOKEN.OPERATOR],
  [KIND.PARAM, TOKEN.PARAM],
]));

// The kinds of `lexPg` that can hold a placeholder: the code tokens of 7.6, by an object look-up. A run of them is read as one text.
const IS_CODE = Object.freeze({ [KIND.BLANK]: true, [KIND.WORD]: true, [KIND.NUMBER]: true, [KIND.PUNCT]: true, [KIND.OPERATOR]: true, [KIND.PARAM]: true });

// The braced form of a placeholder, `${name}` or `${name:format}` (7.6). The same pattern as in `tokens_ch.js`.
const BRACED = /\$\{[A-Za-z0-9_]+(?::[A-Za-z0-9_]+)?\}/y;

// The plain form of a placeholder, `$name`: the longest run of `[A-Za-z0-9_]`.
const NAMED = /\$[A-Za-z0-9_]+/y;

// The list of a run with no placeholder. It is shared and never changed.
const NONE = Object.freeze([]);

// The code unit codes that this module compares.
const DOLLAR = 36;
const COLON = 58;
const LOW_LINE = 95;

// True when the text s[from, to) holds digits only.
function allDigits(s, from, to) {
  for (let i = from; i < to; i++) {
    const c = s.charCodeAt(i);
    if (c < 48 || c > 57) return false;
  }
  return true;
}

// The first "$" at `from` or after it, or -1. `cursor.at` holds the last answer, so a text with no "$" after a point is scanned
// once and not once for each run. The calls ask for growing `from` values: the total scan is linear.
function dollarFrom(s, cursor, from) {
  if (cursor.at >= 0 && cursor.at < from) cursor.at = s.indexOf("$", from);
  return cursor.at;
}

// The placeholders in the run s[from, to): a list of `{ start, end, kind, digits }` in text order. A `$` with no match gives
// none. `digits` is true for a `$` and digits only: it is a parameter when it is a whole token of lexPg, as the scan of
// `scanRun` decides. In a word, as in `x$1`, it is a variable, because `sqlscan.js` finds the name "1" there.
function placeholders(s, cursor, from, to) {
  const found = [];
  let p = dollarFrom(s, cursor, from);
  while (p >= 0 && p < to) {
    const braced = s.charCodeAt(p + 1) === 123;
    const re = braced ? BRACED : NAMED;
    re.lastIndex = p;
    const m = re.exec(s);
    const end = m ? p + m[0].length : -1;
    const name = braced ? p + 2 : p + 1;
    if (m && end <= to) {
      const macro = s.charCodeAt(name) === LOW_LINE && s.charCodeAt(name + 1) === LOW_LINE;
      found.push({ start: p, end, kind: macro ? TOKEN.MACRO : TOKEN.VARIABLE, digits: !braced && allDigits(s, name, end) });
      p = dollarFrom(s, cursor, end);
    } else {
      p = dollarFrom(s, cursor, p + 1);
    }
  }
  return found;
}

// True when the token `t` is a lone ":" and the last token of `out` is a lone ":" that ends where `t` starts. The two make
// the operator "::": the last token takes the place of both.
function joinsColon(s, out, t) {
  if (t.end - t.start !== 1 || s.charCodeAt(t.start) !== COLON) return false;
  const prev = out[out.length - 1];
  if (!prev || prev.kind !== TOKEN.PUNCT || prev.end !== t.start || prev.end - prev.start !== 1 || s.charCodeAt(prev.start) !== COLON) {
    return false;
  }
  prev.kind = TOKEN.OPERATOR;
  prev.end = t.end;
  return true;
}

// Pushes the token `t` of lexPg, mapped in place: only its kind changes, so no object is made. A `$` alone, with no name, has
// no place in the grammar: it is `other`, as in `tokens_ch.js`.
function pushInPlace(s, out, t) {
  const lone = t.kind === KIND.WORD && t.end - t.start === 1 && s.charCodeAt(t.start) === DOLLAR;
  t.kind = lone ? TOKEN.OTHER : (KIND_MAP[t.kind] ?? TOKEN.OTHER);
  if (t.kind === TOKEN.PUNCT && joinsColon(s, out, t)) return;
  out.push(t);
}

// Pushes a new token for a piece of a lexPg token or for a placeholder. It joins "::" as `pushInPlace` does.
function pushNew(s, out, kind, start, end) {
  const t = { kind, start, end, closed: true };
  if (kind === TOKEN.PUNCT && joinsColon(s, out, t)) return;
  out.push(t);
}

// Pushes the tokens of the run lex[a, b) of code tokens. The placeholders cut the tokens: a word `a$b` gives the word `a` and
// the variable `$b`, as the engine reads the run (7.6). A placeholder can start in one token and end in a later one.
function scanRun(s, cursor, lex, a, b, out) {
  const from = lex[a].start;
  const to = lex[b - 1].end;
  const found = from < to ? placeholders(s, cursor, from, to) : NONE;
  let r = 0;
  for (let k = a; k < b; k++) {
    const t = lex[k];
    while (r < found.length && found[r].end <= t.start) r += 1;
    const hit0 = r < found.length ? found[r] : null;
    if (hit0 === null || hit0.start >= t.end || (hit0.digits && t.kind === KIND.PARAM && hit0.start === t.start && hit0.end === t.end)) {
      // No placeholder touches this token, or the token is a parameter: it keeps its object and its bounds.
      pushInPlace(s, out, t);
      continue;
    }
    const base = KIND_MAP[t.kind] ?? TOKEN.OTHER;
    let pos = t.start;
    while (pos < t.end) {
      while (r < found.length && found[r].end <= pos) r += 1;
      const hit = r < found.length ? found[r] : null;
      if (hit && hit.start <= pos) {
        // Inside a placeholder. The token of the placeholder is pushed once, where it starts.
        if (hit.start === pos) pushNew(s, out, hit.kind, hit.start, hit.end);
        pos = Math.min(t.end, hit.end);
      } else {
        const stop = hit && hit.start < t.end ? hit.start : t.end;
        const lone = t.kind === KIND.WORD && stop - pos === 1 && s.charCodeAt(pos) === DOLLAR;
        pushNew(s, out, lone ? TOKEN.OTHER : base, pos, stop);
        pos = stop;
      }
    }
  }
}

/**
 * Cuts a text into tokens with no gap. A token has `kind` (a value of `TOKEN`), `start`, `end` and `closed`.
 * A comment, a string, a quoted name or a dollar quote that the text ends before it closes has `closed: false`.
 * A line comment ends before its line end. The line end is part of the next blank token.
 * A string, an E string, a prefixed string and a dollar quote are `string`. A quoted name is `name`.
 * The operators `::`, `->`, `->>`, `#>`, `#>>`, `@>`, `<@`, `?|`, `?&`, `||` and the comparisons are one token each.
 * A macro is `$__name`. A variable is `$name`, `${name}` or `${name:format}`. A parameter is `$` and digits.
 * The token objects are the objects of `lexPg` with a new kind: the call makes new objects only for a macro, a variable and a cut.
 * The call is pure and gives the same tokens for the same text. It never throws. After a fault of the lexer the result is one `other` token that covers the text.
 */
export function tokensPostgres(text) {
  const s = String(text ?? "");
  const out = [];
  const cursor = { at: s.indexOf("$") };
  try {
    const lex = lexPg(s);
    let i = 0;
    while (i < lex.length) {
      const t = lex[i];
      if (IS_CODE[t.kind] === true) {
        let j = i + 1;
        while (j < lex.length && IS_CODE[lex[j].kind] === true) j += 1;
        scanRun(s, cursor, lex, i, j, out);
        i = j;
      } else {
        t.kind = KIND_MAP[t.kind] ?? TOKEN.OTHER;
        out.push(t);
        i += 1;
      }
    }
    return out;
  } catch {
    return s.length > 0 ? [{ kind: TOKEN.OTHER, start: 0, end: s.length, closed: true }] : [];
  }
}

// The parser of SQL reads this tokenizer for the dialect `postgres` after the load of this module.
setTokenizer("postgres", tokensPostgres);
