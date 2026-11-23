// Lexer of the dialect `postgres` for the page (7.20, 9.10), for `sqlscan.js` only. It ports the PostgreSQL part of the
// server lexer (`lexer.rs`). The vectors tests/fixtures/sql-lex-vectors-pg.json bind the two: change both together.
// It reads UTF-16 code units: a unit of a multibyte character is at least 0x80, as every byte of it, so the tokens agree.
// The tokens cover the text with no gap. A token that the text ends before it closes has `closed: false`.

export const KIND = Object.freeze({
  BLANK: "blank",
  LINE: "line",
  BLOCK: "block",
  WORD: "word",
  NAME: "name",
  STRING: "string",
  ESTRING: "estring",
  PREFIXED: "prefixed",
  HEREDOC: "heredoc",
  NUMBER: "number",
  PUNCT: "punct",
  OPERATOR: "operator",
  PARAM: "param",
});

const LF = 10;
const CR = 13;
const QUOTE = 39; // '
const DQUOTE = 34; // "
const DOLLAR = 36; // $
const DASH = 45; // -
const SLASH = 47; // /
const STAR = 42; // *
const BACKSLASH = 92;

function isBlank(c) {
  return c === 32 || c === 9 || c === LF || c === CR || c === 12 || c === 11;
}

function isDigit(c) {
  return c >= 48 && c <= 57;
}

function isAlpha(c) {
  return (c >= 65 && c <= 90) || (c >= 97 && c <= 122);
}

function isWordStart(c) {
  return isAlpha(c) || c === 95 || c >= 0x80;
}

function isWordChar(c) {
  return isWordStart(c) || isDigit(c) || c === DOLLAR;
}

// Operator bytes, and the special ones that keep a trailing + or - (7.20).
const OPERATORS = "+-*/<>=~!@#%^&|`?";
const SPECIAL = "~!@#%^&|`?";

function startsLineComment(s, at) {
  return s.charCodeAt(at) === DASH && s.charCodeAt(at + 1) === DASH;
}

function startsBlockComment(s, at) {
  return s.charCodeAt(at) === SLASH && s.charCodeAt(at + 1) === STAR;
}

function lineCommentEnd(s, at) {
  let i = at;
  while (i < s.length && s.charCodeAt(i) !== LF && s.charCodeAt(i) !== CR) i += 1;
  return i;
}

function blockCommentEnd(s, at) {
  let depth = 0;
  let i = at;
  while (i < s.length) {
    if (startsBlockComment(s, i)) {
      depth += 1;
      i += 2;
    } else if (s.charCodeAt(i) === STAR && s.charCodeAt(i + 1) === SLASH) {
      i += 2;
      depth = Math.max(0, depth - 1);
      if (depth === 0) return [i, true];
    } else {
      i += 1;
    }
  }
  return [s.length, false];
}

// The end of the operator at `at` (7.20): a run that stops before "--" and "/*". Without a special byte, it loses trailing "+" and "-".
function operatorEnd(s, at) {
  let end = at;
  while (end < s.length && OPERATORS.includes(s[end])) {
    if (end > at && (startsLineComment(s, end) || startsBlockComment(s, end))) break;
    end += 1;
  }
  const sign = (e) => s[e - 1] === "+" || s[e - 1] === "-";
  if (end - at > 1 && sign(end) && ![...s.slice(at, end - 1)].some((c) => SPECIAL.includes(c))) {
    end -= 1;
    while (end - at > 1 && sign(end)) end -= 1;
  }
  return end;
}

// The quote that continues a string after the closing quote at `from`, or -1 (7.20): blanks or line comments,
// one LF or CR, blanks or line comments that end in a line end, then a quote mark.
function continuation(s, from) {
  let p = from;
  for (;;) {
    const c = s.charCodeAt(p);
    if (c === 32 || c === 9 || c === 11 || c === 12) p += 1;
    else if (startsLineComment(s, p)) p = lineCommentEnd(s, p);
    else break;
  }
  const lineEnd = s.charCodeAt(p);
  if (lineEnd !== LF && lineEnd !== CR) return -1;
  p += 1;
  for (;;) {
    const c = s.charCodeAt(p);
    if (isBlank(c)) {
      p += 1;
    } else if (startsLineComment(s, p)) {
      const e = lineCommentEnd(s, p);
      const after = s.charCodeAt(e);
      if (after !== LF && after !== CR) return -1;
      p = e + 1;
    } else {
      break;
    }
  }
  return s.charCodeAt(p) === QUOTE ? p : -1;
}

// The end of a string part: [end, closed]. In an E string a backslash escapes the next unit.
function stringPartEnd(s, open, escapes) {
  let p = open + 1;
  while (p < s.length) {
    const c = s.charCodeAt(p);
    if (escapes && c === BACKSLASH) {
      p += 2;
    } else if (c === QUOTE) {
      if (s.charCodeAt(p + 1) === QUOTE) p += 2;
      else return [p + 1, true];
    } else {
      p += 1;
    }
  }
  return [s.length, false];
}

function quotedNameEnd(s, open) {
  let p = open + 1;
  while (p < s.length) {
    if (s.charCodeAt(p) === DQUOTE) {
      if (s.charCodeAt(p + 1) === DQUOTE) p += 2;
      else return [p + 1, true];
    } else {
      p += 1;
    }
  }
  return [s.length, false];
}

// The length of the dollar quote tag ($$ or $tag$) that starts at `at`, or 0.
function dollarTagLength(s, at) {
  if (s.charCodeAt(at) !== DOLLAR) return 0;
  let i = at + 1;
  if (isWordStart(s.charCodeAt(i))) {
    while (i < s.length && (isWordStart(s.charCodeAt(i)) || isDigit(s.charCodeAt(i)))) i += 1;
  }
  return s.charCodeAt(i) === DOLLAR ? i - at + 1 : 0;
}

function numberEnd(s, at) {
  const hex = s.charCodeAt(at) === 48 && (s.charCodeAt(at + 1) === 120 || s.charCodeAt(at + 1) === 88);
  let i = at;
  while (i < s.length) {
    const c = s.charCodeAt(i);
    if (c < 0x80 && (isAlpha(c) || isDigit(c) || c === 95 || c === 46)) {
      const exponent = hex ? c === 112 || c === 80 : c === 101 || c === 69;
      i += 1;
      if (exponent && (s.charCodeAt(i) === 43 || s.charCodeAt(i) === DASH)) i += 1;
    } else {
      break;
    }
  }
  return i;
}

function wordEnd(s, at) {
  let i = at;
  while (i < s.length && isWordChar(s.charCodeAt(i))) i += 1;
  return i;
}

function prefix(s, at) {
  const c0 = s.charCodeAt(at);
  const c1 = s.charCodeAt(at + 1);
  const c2 = s.charCodeAt(at + 2);
  if ((c0 === 69 || c0 === 101) && c1 === QUOTE) return [KIND.ESTRING, at + 1];
  if ((c0 === 66 || c0 === 98 || c0 === 88 || c0 === 120 || c0 === 78 || c0 === 110) && c1 === QUOTE) {
    return [KIND.PREFIXED, at + 1];
  }
  if ((c0 === 85 || c0 === 117) && c1 === 38) {
    if (c2 === QUOTE) return [KIND.PREFIXED, at + 2];
    if (c2 === DQUOTE) return [KIND.NAME, at + 2];
  }
  return null;
}

function nextToken(s, at) {
  const c = s.charCodeAt(at);
  let closed = true;
  let kind;
  let end;
  let pre;
  if (isBlank(c)) {
    end = at + 1;
    while (end < s.length && isBlank(s.charCodeAt(end))) end += 1;
    kind = KIND.BLANK;
  } else if (startsLineComment(s, at)) {
    kind = KIND.LINE;
    end = lineCommentEnd(s, at);
  } else if (startsBlockComment(s, at)) {
    kind = KIND.BLOCK;
    [end, closed] = blockCommentEnd(s, at);
  } else if (c === QUOTE) {
    kind = KIND.STRING;
    [end, closed] = stringPartEnd(s, at, false);
  } else if (c === DQUOTE) {
    kind = KIND.NAME;
    [end, closed] = quotedNameEnd(s, at);
  } else if ((pre = prefix(s, at))) {
    kind = pre[0];
    [end, closed] = kind === KIND.NAME ? quotedNameEnd(s, pre[1]) : stringPartEnd(s, pre[1], kind === KIND.ESTRING);
  } else if (c === DOLLAR) {
    const tag = dollarTagLength(s, at);
    if (tag > 0) {
      kind = KIND.HEREDOC;
      const found = s.indexOf(s.slice(at, at + tag), at + tag);
      closed = found >= 0;
      end = closed ? found + tag : s.length;
    } else if (isDigit(s.charCodeAt(at + 1))) {
      kind = KIND.PARAM;
      end = at + 1;
      while (isDigit(s.charCodeAt(end))) end += 1;
    } else {
      kind = KIND.WORD;
      end = wordEnd(s, at + 1);
    }
  } else if (isDigit(c) || (c === 46 && isDigit(s.charCodeAt(at + 1)))) {
    kind = KIND.NUMBER;
    end = numberEnd(s, at);
  } else if (isWordStart(c)) {
    kind = KIND.WORD;
    end = wordEnd(s, at);
  } else if (OPERATORS.includes(s[at])) {
    kind = KIND.OPERATOR;
    end = operatorEnd(s, at);
  } else {
    kind = KIND.PUNCT;
    end = at + 1;
  }
  if (end <= at) end = at + 1;
  return { kind, start: at, end: Math.min(end, s.length), closed };
}

const STRING_KINDS = new Set([KIND.STRING, KIND.ESTRING, KIND.PREFIXED]);

// All tokens of the text, with no error check. A string continued after a line end (7.20) gives the first part, the
// tokens of the gap, and the next part as a token of the same kind, from its quote mark.
export function lexPg(text) {
  const s = String(text ?? "");
  const out = [];
  let at = 0;
  while (at < s.length) {
    const t = nextToken(s, at);
    at = t.end;
    out.push(t);
    if (!t.closed || !STRING_KINDS.has(t.kind)) continue;
    for (let open = continuation(s, at); open >= 0; open = continuation(s, at)) {
      while (at < open) {
        const gap = nextToken(s, at);
        at = gap.end;
        out.push(gap);
      }
      const [end, closed] = stringPartEnd(s, open, t.kind === KIND.ESTRING);
      out.push({ kind: t.kind, start: open, end, closed });
      at = end;
      if (!closed) break;
    }
  }
  return out;
}

// The code tokens (7.6): a run of them can hold a placeholder.
export function isCodeKind(kind) {
  return (
    kind === KIND.BLANK ||
    kind === KIND.WORD ||
    kind === KIND.NUMBER ||
    kind === KIND.PUNCT ||
    kind === KIND.OPERATOR ||
    kind === KIND.PARAM
  );
}

export function isCommentKind(kind) {
  return kind === KIND.LINE || kind === KIND.BLOCK;
}
