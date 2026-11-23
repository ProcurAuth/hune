// The JSON tree of the query editor (F42): the text of a MongoDB pipeline (7.28, 7.34). Pure module: no DOM access.
// The tree gives structure only: the format, the folds, the bracket pairs and the current block. It is never a security
// check. The server stays the only authority on the validity of a query, and a fault of the tree never stops a save or a run.
// The editor loads this module on demand, so it is not in the page measure of 9.30.
//
// The reader follows strict JSON (RFC 8259, rule J1 of 7.28) and it is tolerant. A text that is partial or wrong gives a
// partial tree with error nodes and never an exception. The reader is iterative: it has one explicit stack. Its time is
// linear in the length of the text.

import { JSON_NODE, MAX_AST_DEPTH, MAX_AST_INPUT_BYTES, MAX_AST_NODES, TOKEN, node, utf8Length } from "./nodes.js";

/**
 * The formats that a placeholder can have inside a string (7.9, 7.34). The page scanner `sqlscan.js` has the same list.
 * An unknown format gives no placeholder, as in the scanner.
 */
const FORMATS_IN_STRING = new Set([
  "csv", "singlequote", "sqlstring", "raw", "json", "pipe", "regex", "glob", "lucene", "percentencode", "queryparam",
  "distributed", "doublequote", "html", "text", "number", "date", "oid", "bool", "json_number", "json_oid",
]);

/** The formats that a placeholder can have outside a string. This is the list of 7.9 without the formats of MongoDB. */
const FORMATS_BARE = new Set([
  "csv", "singlequote", "sqlstring", "raw", "json", "pipe", "regex", "glob", "lucene", "percentencode", "queryparam",
  "distributed", "doublequote", "html", "text",
]);

/** The kinds of a lexeme. A lexeme is one significant item of the text for the reader of the tree. */
const L = Object.freeze({
  OPEN_OBJECT: 1,
  CLOSE_OBJECT: 2,
  OPEN_ARRAY: 3,
  CLOSE_ARRAY: 4,
  COLON: 5,
  COMMA: 6,
  STRING: 7,
  NUMBER: 8,
  LITERAL: 9,
  BAD_WORD: 10,
  BAD_NUMBER: 11,
  PLACEHOLDER: 12,
  COMMENT: 13,
});

/** The kinds of a frame of the stack. */
const F_DOC = 0;
const F_OBJECT = 1;
const F_ARRAY = 2;

/** The states of a frame. A document has D_EMPTY and D_DONE. An array has A_VALUE (a value is next) and A_AFTER. */
const D_EMPTY = 0;
const D_DONE = 1;
const A_VALUE = 0;
const A_AFTER = 1;
/** An object has O_KEY (a key is next), O_COLON (a colon is next), O_VALUE (a value is next) and O_AFTER. */
const O_KEY = 0;
const O_COLON = 1;
const O_VALUE = 2;
const O_AFTER = 3;

/** The code of a character that is a blank of JSON: space, TAB, LF and CR. */
const isBlank = (c) => c === 32 || c === 10 || c === 13 || c === 9;

/** The code of a character of a name of a placeholder: A-Z, a-z, 0-9 and the underscore. */
const isNameChar = (c) => (c >= 97 && c <= 122) || (c >= 65 && c <= 90) || (c >= 48 && c <= 57) || c === 95;

/** The code of a character that a bare word or a bare number can hold: A-Z, a-z, 0-9, underscore, dot, plus and minus. */
const isWordChar = (c) => isNameChar(c) || c === 46 || c === 43 || c === 45;

/** The code of a hexadecimal digit. */
const isHex = (c) => (c >= 48 && c <= 57) || (c >= 97 && c <= 102) || (c >= 65 && c <= 70);

/** True when the code is one of the six punctuation characters `{ } [ ] : ,` or the quote. */
const isStructural = (c) => c === 123 || c === 125 || c === 91 || c === 93 || c === 58 || c === 44 || c === 34;

/** Sets `error` on a node when it has no error yet. The first fault of a node is the one that stays. */
function setError(n, reason) {
  if (!n.error) n.error = reason;
}

/** The UTF-16 offset where the text goes above the UTF-8 limit, or the length of the text when it does not. */
function offsetOfByteLimit(text, limit) {
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c < 0x80) bytes += 1;
    else if (c < 0x800) bytes += 2;
    else if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length) {
      bytes += 4;
      i += 1;
    } else bytes += 3;
    if (bytes > limit) return i;
  }
  return text.length;
}

/**
 * Reads a text of JSON and builds its token stream and its tree.
 * `options.variables` is optional: a list of the names of the variables of the document. With it, a name that is not in
 * the list is plain text in a string (a field path such as `"$email"`). A macro (a name that starts with two underscores)
 * is always a placeholder. Without it, each form `$name`, `${name}` and `${name:format}` is a placeholder, as in the scanner
 * of the page (it never finds one name less).
 *
 * The result is `{ root, tokens, capped }`.
 * - `tokens` cover the text with no gap and no overlap. The kinds are `blank`, `punct` (for `{ } [ ] : ,`), `string`,
 *   `number`, `word` (for `true`, `false` and `null`), `variable`, `macro`, `comment` (a comment is a fault of the text) and
 *   `other` (a wrong character or a wrong word). A string value that holds placeholders is cut: the pieces of text are
 *   `string` tokens and each placeholder is one `variable` or `macro` token. A key is never cut, because a key holds no
 *   placeholder (7.34). Only the last token of a string that the text ends has `closed: false`.
 * - `root` is a node `doc` that spans the whole text. Its children are the values of the top level. A node `object` or
 *   `array` has `open` and `close` (the offsets of the brackets). `close` is absent when the text ends first, and then
 *   `error` is `"unclosed"`. A node `member` has `name` (the text of the key, with the escapes decoded) and one value child.
 *   A value is a node `object`, `array`, `string`, `number`, `literal` or `error`.
 *   An error node can also stand in a container or in a member (before the value), for example a comma that is too much.
 *   The value of a member is the last child that is not an error node of type `error` placed before it.
 * - An `object` or an `array` that spans two lines or more has `foldable: true`.
 * - A stage of a pipeline is an object in an array of stages with one member whose key starts with `$`. It has `stage` and
 *   `name` (both are that key). An array of stages is the top array, the value of `aggregate` in the top object, the value
 *   of `pipeline` and a value inside `$facet`.
 * - A node can have `error` with one of these reasons. Container: `unclosed`, `missing_comma`, `extra_value`. String:
 *   `unclosed`, `bad_escape`, `control_char`, `missing_comma`. Member: `missing_key`, `bad_key`, `missing_colon`,
 *   `missing_value`, `missing_comma`, `duplicate_member`. Error node: `bad_value`, `bad_number`,
 *   `placeholder_outside_string`, `comment`, `unmatched`, `unexpected_comma`, `unexpected_colon`, `trailing_comma`.
 * - `capped` is null, or `{ reason, at }` with the reason `too_large`, `too_deep` or `too_many_nodes` (nodes.js). The tokens
 *   stay complete. The tree stops at the offset `at`: the nodes that were open end there with `error: "unclosed"`.
 */
export function parseJson(text, options = null) {
  const src = typeof text === "string" ? text : String(text ?? "");
  const n = src.length;
  const vars = options && options.variables ? new Set(options.variables) : null;
  const tokens = [];
  const root = node(JSON_NODE.DOC, 0, n, []);
  const result = { root, tokens, capped: null };

  // The state of the reader.
  const stack = [{ kind: F_DOC, node: root, state: D_EMPTY, member: null, comma: null, line: 0, pipe: true, inPipe: false, facet: false, seen: null }];
  let nodeCount = 1;
  let depth = 0;
  let stopped = false;
  let lineNo = 0;
  let prevEnd = 0;
  let prevLine = 0;

  // The scratch values of the scan of a string and of a placeholder (no allocation for each item).
  let sError = null;
  let sEnd = 0;
  let sClosed = false;
  let sBack = false;
  let sDollar = false;
  let sU = false;
  let pA = 0;
  let pB = 0;

  /** Pushes a token. */
  const emit = (kind, start, end, closed = true) => {
    tokens.push({ kind, start, end, closed });
  };

  /** Stops the tree. The tokens go on. Each open node ends at the last content. */
  function stop(reason, at) {
    if (!result.capped) result.capped = { reason, at };
    stopped = true;
    while (stack.length > 1) popFrame(prevEnd, -1, -1, prevLine);
  }

  /** Makes a node and counts it. At the cap, it stops the tree and gives null. */
  function mk(type, start, end, children, extra) {
    if (nodeCount >= MAX_AST_NODES) {
      stop("too_many_nodes", start);
      return null;
    }
    nodeCount += 1;
    return node(type, start, end, children, extra);
  }

  /** True when the frame is an object that has a member that waits for its value. */
  const openMember = (f) => f.kind === F_OBJECT && (f.state === O_COLON || f.state === O_VALUE);

  /** Gives a placeholder match in `s` at `p` (a `$`) as the end offset, or -1. The name is in `pA`..`pB`. */
  function matchAt(s, p, b, formats) {
    let q = p + 1;
    if (q >= b) return -1;
    let end;
    if (s.charCodeAt(q) === 123) {
      q += 1;
      const a = q;
      while (q < b && isNameChar(s.charCodeAt(q))) q++;
      if (q === a) return -1;
      pA = a;
      pB = q;
      if (q < b && s.charCodeAt(q) === 58) {
        const f0 = q + 1;
        let f = f0;
        while (f < b && isNameChar(s.charCodeAt(f))) f++;
        if (f === f0 || f >= b || s.charCodeAt(f) !== 125) return -1;
        if (!formats.has(s.slice(f0, f))) return -1;
        q = f;
      }
      if (q >= b || s.charCodeAt(q) !== 125) return -1;
      end = q + 1;
    } else {
      const a = q;
      while (q < b && isNameChar(s.charCodeAt(q))) q++;
      if (q === a) return -1;
      pA = a;
      pB = q;
      end = q;
    }
    if (vars && !(s.charCodeAt(pA) === 95 && s.charCodeAt(pA + 1) === 95) && !vars.has(s.slice(pA, pB))) return -1;
    return end;
  }

  /** True when the name in `pA`..`pB` of `s` starts with two underscores: a macro. */
  const isMacro = (s) => s.charCodeAt(pA) === 95 && s.charCodeAt(pA + 1) === 95;

  /** Reads four hexadecimal digits at `p` in the text. */
  function hex4(p) {
    if (p + 4 > n) return false;
    return isHex(src.charCodeAt(p)) && isHex(src.charCodeAt(p + 1)) && isHex(src.charCodeAt(p + 2)) && isHex(src.charCodeAt(p + 3));
  }

  /**
   * Scans a string that starts with a quote at `i`. A raw LF or CR ends the string: it has no end. It sets the scratch
   * values `sEnd` (the end offset), `sClosed`, `sError` (the first fault), `sBack`, `sDollar` and `sU`.
   */
  function scanString(i) {
    sError = null;
    sClosed = false;
    sBack = false;
    sDollar = false;
    sU = false;
    for (let j = i + 1; j < n; j++) {
      const c = src.charCodeAt(j);
      if (c === 34) {
        sEnd = j + 1;
        sClosed = true;
        return;
      }
      if (c === 92) {
        sBack = true;
        const d = j + 1 < n ? src.charCodeAt(j + 1) : -1;
        if (d === 117) {
          sU = true;
          if (hex4(j + 2)) j += 5;
          else {
            if (!sError) sError = "bad_escape";
            j += 1;
          }
        } else if (d === 34 || d === 92 || d === 47 || d === 98 || d === 102 || d === 110 || d === 114 || d === 116) j += 1;
        else {
          if (!sError) sError = "bad_escape";
          if (d !== 10 && d !== 13 && d !== -1) j += 1;
        }
      } else if (c === 10 || c === 13) {
        sEnd = j;
        return;
      } else if (c < 32) {
        if (!sError) sError = "control_char";
      } else if (c === 36) sDollar = true;
    }
    sEnd = n;
  }

  /** Pushes the tokens of a string. A placeholder in the string is one token and cuts the string in pieces. */
  function emitString(i, end, closed, isKey) {
    if (isKey || !(sDollar || (sU && sBack))) {
      emit(TOKEN.STRING, i, end, closed);
      return;
    }
    let piece = i;
    const contentEnd = closed ? end - 1 : end;
    if (sBack && sError !== "bad_escape") {
      // The scanner of the page decodes the escapes first: `$host` is a placeholder. Map each decoded unit back.
      const units = [];
      const from = [];
      const to = [];
      for (let k = i + 1; k < contentEnd; k++) {
        const c = src.charCodeAt(k);
        if (c !== 92) {
          units.push(String.fromCharCode(c));
          from.push(k);
          to.push(k + 1);
          continue;
        }
        const d = src.charCodeAt(k + 1);
        let unit;
        let len = 2;
        if (d === 117) {
          unit = String.fromCharCode(parseInt(src.slice(k + 2, k + 6), 16));
          len = 6;
        } else unit = d === 98 ? "\b" : d === 102 ? "\f" : d === 110 ? "\n" : d === 114 ? "\r" : d === 116 ? "\t" : String.fromCharCode(d);
        units.push(unit);
        from.push(k);
        to.push(k + len);
        k += len - 1;
      }
      const dec = units.join("");
      for (let p = dec.indexOf("$"); p >= 0 && p < dec.length; p = dec.indexOf("$", p + 1)) {
        const e = matchAt(dec, p, dec.length, FORMATS_IN_STRING);
        if (e < 0) continue;
        const ms = from[p];
        if (ms > piece) emit(TOKEN.STRING, piece, ms, true);
        piece = to[e - 1];
        emit(isMacro(dec) ? TOKEN.MACRO : TOKEN.VARIABLE, ms, piece, true);
        p = e - 1;
      }
    } else {
      for (let p = src.indexOf("$", i + 1); p >= 0 && p < contentEnd; p = src.indexOf("$", p + 1)) {
        const e = matchAt(src, p, contentEnd, FORMATS_IN_STRING);
        if (e < 0) continue;
        if (p > piece) emit(TOKEN.STRING, piece, p, true);
        emit(isMacro(src) ? TOKEN.MACRO : TOKEN.VARIABLE, p, e, true);
        piece = e;
        p = e - 1;
      }
    }
    if (piece < end) emit(TOKEN.STRING, piece, end, closed);
    else if (!closed) tokens[tokens.length - 1].closed = false;
  }

  /** The text of a key: the escapes are decoded when the string is valid. */
  function keyText(s, e, closed, hasEscape, faulty) {
    const inner = src.slice(s + 1, closed ? e - 1 : e);
    if (!hasEscape || !closed || faulty) return inner;
    try {
      return JSON.parse(src.slice(s, e));
    } catch {
      return inner;
    }
  }

  /** True when the run `a`..`b` of the text is a number of RFC 8259. */
  function validNumber(a, b) {
    let i = a;
    if (src.charCodeAt(i) === 45) i++;
    if (i >= b) return false;
    let c = src.charCodeAt(i);
    if (c === 48) i++;
    else if (c >= 49 && c <= 57) {
      i++;
      while (i < b && (c = src.charCodeAt(i)) >= 48 && c <= 57) i++;
    } else return false;
    if (i < b && src.charCodeAt(i) === 46) {
      i++;
      const k = i;
      while (i < b && (c = src.charCodeAt(i)) >= 48 && c <= 57) i++;
      if (i === k) return false;
    }
    if (i < b && (src.charCodeAt(i) === 101 || src.charCodeAt(i) === 69)) {
      i++;
      if (i < b && (src.charCodeAt(i) === 43 || src.charCodeAt(i) === 45)) i++;
      const k = i;
      while (i < b && (c = src.charCodeAt(i)) >= 48 && c <= 57) i++;
      if (i === k) return false;
    }
    return i === b;
  }

  /** Gives the kind of lexeme of a bare run `a`..`b`: a number, a literal, a wrong number or a wrong word. */
  function classifyRun(a, b) {
    const c0 = src.charCodeAt(a);
    if (c0 === 45 || (c0 >= 48 && c0 <= 57)) return validNumber(a, b) ? L.NUMBER : L.BAD_NUMBER;
    const len = b - a;
    if ((len === 4 && (src.startsWith("true", a) || src.startsWith("null", a))) || (len === 5 && src.startsWith("false", a))) return L.LITERAL;
    return L.BAD_WORD;
  }

  // ---- The tree builder: one function for each kind of lexeme. ----

  /** Puts a value node in its parent frame, with the checks of the state, and moves the state on. */
  function placeValue(f, nd) {
    if (f.kind === F_DOC) {
      if (f.state === D_DONE) setError(nd, "extra_value");
      f.node.children.push(nd);
      f.state = D_DONE;
    } else if (f.kind === F_ARRAY) {
      if (f.state === A_AFTER) setError(nd, "missing_comma");
      f.node.children.push(nd);
      f.state = A_AFTER;
      f.comma = null;
    } else {
      const m = f.member;
      if (f.state === O_COLON) setError(m, "missing_colon");
      m.children.push(nd);
      if (nd.end > m.end) m.end = nd.end;
      f.state = O_AFTER;
    }
  }

  /** Adds an error node that does not change the state: in the open member when there is one, else in the frame. */
  function side(reason, s, e) {
    const f = stack[stack.length - 1];
    const nd = mk(JSON_NODE.ERROR, s, e, [], { error: reason });
    if (nd) (openMember(f) ? f.member : f.node).children.push(nd);
  }

  /** Ends the member that waits for its value: it has no value. */
  function closeMemberMissing(f) {
    const m = f.member;
    setError(m, f.state === O_COLON ? "missing_colon" : "missing_value");
    if (prevEnd > m.end) m.end = prevEnd;
    f.state = O_AFTER;
  }

  /** Takes the top frame off the stack and sets the end of its node. `cs` and `ce` are the offsets of the closing bracket. */
  function popFrame(end, cs, ce, endLine) {
    const f = stack.pop();
    const nd = f.node;
    if (f.kind === F_OBJECT && (f.state === O_COLON || f.state === O_VALUE)) closeMemberMissing(f);
    if (cs >= 0) {
      if (f.comma) {
        const t = mk(JSON_NODE.ERROR, f.comma.start, f.comma.end, [], { error: "trailing_comma" });
        if (t) {
          // Comments can stand between the comma and the bracket: keep the order of the children.
          let at = nd.children.length;
          while (at > 0 && nd.children[at - 1].start > t.start) at--;
          nd.children.splice(at, 0, t);
        }
      }
      nd.close = cs;
      nd.end = ce;
    } else {
      // A child that a cap left behind can end after the last content: the node ends at the end of its last child.
      const last = nd.children.length ? nd.children[nd.children.length - 1].end : 0;
      nd.end = Math.max(end, nd.start + 1, last);
      nd.error = "unclosed";
    }
    if (endLine > f.line) nd.foldable = true;
    depth -= 1;
    if (f.kind === F_OBJECT && f.inPipe) {
      let count = 0;
      let name = "";
      for (const c of nd.children) {
        if (c.type !== JSON_NODE.MEMBER) continue;
        count += 1;
        name = c.name;
      }
      if (count === 1 && name.charCodeAt(0) === 36) {
        nd.stage = name;
        nd.name = name;
      }
    }
    const p = stack[stack.length - 1];
    if (p && p.kind === F_OBJECT && p.member && nd.end > p.member.end) p.member.end = nd.end;
  }

  /** Opens an object or an array as a value of the top frame. */
  function openContainer(k, s) {
    if (depth >= MAX_AST_DEPTH) {
      stop("too_deep", s);
      return;
    }
    const f = stack[stack.length - 1];
    const isObject = k === L.OPEN_OBJECT;
    const nd = mk(isObject ? JSON_NODE.OBJECT : JSON_NODE.ARRAY, s, s + 1, [], null);
    if (!nd) return;
    nd.open = s;
    // The context of the stages of a pipeline (7.30, 7.31).
    let pipe = false;
    let inPipe = false;
    let facet = false;
    if (isObject) {
      inPipe = f.kind === F_ARRAY && f.pipe;
      facet = f.kind === F_OBJECT && f.member !== null && f.member.name === "$facet";
    } else if (f.kind === F_DOC) pipe = true;
    else if (f.kind === F_OBJECT) {
      const name = f.member.name;
      pipe = name === "pipeline" || f.facet || (name === "aggregate" && stack.length === 2);
    }
    placeValue(f, nd);
    depth += 1;
    stack.push({
      kind: isObject ? F_OBJECT : F_ARRAY,
      node: nd,
      state: isObject ? O_KEY : A_VALUE,
      member: null,
      comma: null,
      line: lineNo,
      pipe,
      inPipe,
      facet,
      seen: null,
    });
  }

  /** Handles a closing bracket. It closes the nearest frame of the right kind, and the frames above it end with no bracket. */
  function closer(k, s, e) {
    const want = k === L.CLOSE_OBJECT ? F_OBJECT : F_ARRAY;
    let t = stack.length - 1;
    while (t > 0 && stack[t].kind !== want) t--;
    if (t === 0) {
      const f = stack[stack.length - 1];
      const nd = mk(JSON_NODE.ERROR, s, e, [], { error: "unmatched" });
      if (!nd) return;
      if (openMember(f)) placeValue(f, nd);
      else f.node.children.push(nd);
      return;
    }
    while (stack.length - 1 > t) popFrame(prevEnd, -1, -1, prevLine);
    popFrame(e, s, e, lineNo);
  }

  /** Handles a comma. */
  function comma(s, e) {
    const f = stack[stack.length - 1];
    if (f.kind === F_OBJECT) {
      if (f.state === O_AFTER) {
        f.comma = { start: s, end: e };
        f.state = O_KEY;
        return;
      }
      if (f.state === O_COLON || f.state === O_VALUE) {
        closeMemberMissing(f);
        f.comma = { start: s, end: e };
        f.state = O_KEY;
        return;
      }
    } else if (f.kind === F_ARRAY && f.state === A_AFTER) {
      f.comma = { start: s, end: e };
      f.state = A_VALUE;
      return;
    }
    if (f.kind !== F_DOC) f.comma = null;
    side("unexpected_comma", s, e);
  }

  /** Handles a colon. */
  function colon(s, e) {
    const f = stack[stack.length - 1];
    if (f.kind === F_OBJECT) {
      if (f.state === O_COLON) {
        f.state = O_VALUE;
        f.member.end = Math.max(f.member.end, e);
        return;
      }
      if (f.state === O_KEY) {
        const m = mk(JSON_NODE.MEMBER, s, e, [], { name: "", error: "missing_key" });
        if (!m) return;
        f.comma = null;
        f.node.children.push(m);
        f.member = m;
        f.state = O_VALUE;
        return;
      }
    }
    side("unexpected_colon", s, e);
  }

  /** Handles a string, a scalar, a placeholder or an opening bracket. */
  function value(k, s, e, err, closed, back) {
    const f = stack[stack.length - 1];
    if ((k === L.OPEN_OBJECT || k === L.OPEN_ARRAY) && depth >= MAX_AST_DEPTH) {
      // The depth cap comes before any node of this value, so no node is left behind.
      stop("too_deep", s);
      return;
    }
    if (f.kind === F_OBJECT && (f.state === O_KEY || f.state === O_AFTER)) {
      // A new member starts. A key is a string. Another value makes a member with an error.
      const missing = f.state === O_AFTER;
      const container = k === L.OPEN_OBJECT || k === L.OPEN_ARRAY;
      let m;
      m = mk(JSON_NODE.MEMBER, s, container ? s : e, [], null);
      if (!m) return;
      if (k === L.STRING) m.name = keyText(s, e, closed, back, err === "bad_escape");
      else m.name = container ? "" : src.slice(s, e);
      if (missing) m.error = "missing_comma";
      else if (container) m.error = "missing_key";
      else if (k !== L.STRING) m.error = "bad_key";
      else if (err) m.error = err;
      if (k === L.STRING) {
        if (!f.seen) f.seen = new Set();
        if (f.seen.has(m.name)) setError(m, "duplicate_member");
        f.seen.add(m.name);
      }
      f.comma = null;
      f.node.children.push(m);
      f.member = m;
      if (!container) {
        f.state = O_COLON;
        return;
      }
      f.state = O_VALUE;
    }
    if (k === L.OPEN_OBJECT || k === L.OPEN_ARRAY) {
      openContainer(k, s);
      return;
    }
    let nd;
    if (k === L.STRING) {
      nd = mk(JSON_NODE.STRING, s, e, []);
      if (nd && err) nd.error = err;
    } else if (k === L.NUMBER) nd = mk(JSON_NODE.NUMBER, s, e, []);
    else if (k === L.LITERAL) nd = mk(JSON_NODE.LITERAL, s, e, []);
    else {
      const reason = k === L.BAD_NUMBER ? "bad_number" : k === L.PLACEHOLDER ? "placeholder_outside_string" : "bad_value";
      nd = mk(JSON_NODE.ERROR, s, e, [], { error: reason });
    }
    if (nd) placeValue(f, nd);
  }

  /** Gives one lexeme to the tree builder. */
  function feed(k, s, e, err = null, closed = true, back = false) {
    if (stopped) return;
    if (k === L.CLOSE_OBJECT || k === L.CLOSE_ARRAY) closer(k, s, e);
    else if (k === L.COMMA) comma(s, e);
    else if (k === L.COLON) colon(s, e);
    else if (k === L.COMMENT) side("comment", s, e);
    else value(k, s, e, err, closed, back);
    prevEnd = e;
    prevLine = lineNo;
  }

  // ---- The lexer. ----

  if (n * 3 > MAX_AST_INPUT_BYTES && utf8Length(src) > MAX_AST_INPUT_BYTES) {
    result.capped = { reason: "too_large", at: offsetOfByteLimit(src, MAX_AST_INPUT_BYTES) };
    stopped = true;
  }

  let i = 0;
  while (i < n) {
    const c = src.charCodeAt(i);
    if (isBlank(c)) {
      let j = i;
      let d = c;
      while (j < n && isBlank((d = src.charCodeAt(j)))) {
        if (d === 10) lineNo += 1;
        j++;
      }
      emit(TOKEN.BLANK, i, j);
      i = j;
    } else if (c === 123 || c === 125 || c === 91 || c === 93 || c === 58 || c === 44) {
      emit(TOKEN.PUNCT, i, i + 1);
      const k = c === 123 ? L.OPEN_OBJECT : c === 125 ? L.CLOSE_OBJECT : c === 91 ? L.OPEN_ARRAY : c === 93 ? L.CLOSE_ARRAY : c === 58 ? L.COLON : L.COMMA;
      feed(k, i, i + 1);
      i += 1;
    } else if (c === 34) {
      scanString(i);
      const end = sEnd;
      const closed = sClosed;
      const err = closed ? sError : "unclosed";
      const back = sBack;
      const top = stack[stack.length - 1];
      // A key holds no placeholder (7.34, rule G2): it is one token, so the operator keys do not cut.
      const isKey = !stopped && top.kind === F_OBJECT && (top.state === O_KEY || top.state === O_AFTER);
      emitString(i, end, closed, isKey);
      feed(L.STRING, i, end, err, closed, back);
      i = end;
    } else if (c === 36 && matchAt(src, i, n, FORMATS_BARE) >= 0) {
      const e = matchAt(src, i, n, FORMATS_BARE);
      emit(isMacro(src) ? TOKEN.MACRO : TOKEN.VARIABLE, i, e);
      feed(L.PLACEHOLDER, i, e);
      i = e;
    } else if (c === 47 && (src.charCodeAt(i + 1) === 47 || src.charCodeAt(i + 1) === 42)) {
      let j = i + 2;
      let closed = true;
      if (src.charCodeAt(i + 1) === 47) {
        while (j < n && src.charCodeAt(j) !== 10 && src.charCodeAt(j) !== 13) j++;
      } else {
        closed = false;
        for (; j < n; j++) {
          const d = src.charCodeAt(j);
          if (d === 10) lineNo += 1;
          else if (d === 42 && src.charCodeAt(j + 1) === 47) {
            j += 2;
            closed = true;
            break;
          }
        }
      }
      emit(TOKEN.COMMENT, i, j, closed);
      feed(L.COMMENT, i, j);
      i = j;
    } else if (isWordChar(c)) {
      let j = i + 1;
      while (j < n && isWordChar(src.charCodeAt(j))) j++;
      const k = classifyRun(i, j);
      emit(k === L.NUMBER || k === L.BAD_NUMBER ? TOKEN.NUMBER : k === L.LITERAL ? TOKEN.WORD : TOKEN.OTHER, i, j);
      feed(k, i, j);
      i = j;
    } else {
      // A run of characters that the grammar does not know. It stops before a blank, a bracket, a quote, a word,
      // a placeholder or a comment.
      let j = i + 1;
      while (j < n) {
        const d = src.charCodeAt(j);
        if (isBlank(d) || isStructural(d) || isWordChar(d)) break;
        if (d === 36 && matchAt(src, j, n, FORMATS_BARE) >= 0) break;
        if (d === 47 && (src.charCodeAt(j + 1) === 47 || src.charCodeAt(j + 1) === 42)) break;
        j++;
      }
      emit(TOKEN.OTHER, i, j);
      feed(L.BAD_WORD, i, j);
      i = j;
    }
  }

  if (!stopped) {
    while (stack.length > 1) popFrame(prevEnd, -1, -1, prevLine);
  }
  return result;
}
