// The minimal syntax tree of a SQL text for the query editor (F42): dialects `clickhouse` and `postgres`.
// Pure module: no DOM access. The tree is never a security check. The server stays the only authority on the validity
// of a query, and a fault of the tree never stops a save or a run. The editor loads this module on demand, so it is not
// in the page measure of 9.30.
//
// The tree gives structure only. It serves the format, the folds, the bracket pairs and the current block.
// A word and a punctuation token make no node. Only a literal and the structural types make a node.
//   script > statement > (with > cte*) query
//   query > clause*          (a set operation, such as UNION ALL, is a clause with no brackets)
//   clause > list | expr     (a list has one expr for each top-level comma part)
//   expr > paren | array | call | subquery | literal | macro | variable | comment | error | expr (a lambda)
// A subquery is a bracket whose first word is SELECT or WITH. It holds (with >) query.
// A macro `$__name(...)` has `open` and `close`, and `opaque` is true. The editor never rewrites it.
// Its children only make the bracket pairs complete.
// A variable is `$name`, `${name}` or `${name:format}`. Each is one node and is never split.
// A comment is a node of the innermost node that holds the code on both sides of it.
//
// Positions are UTF-16 offsets. `end` is not in the node. The tokens cover the text with no gap and no overlap.
// A bracket has `open` and `close`. The `close` is absent when the text ends first, and then `error` is "unclosed".
// Each code bracket belongs to one node with `open` or to one `error` node with `error: "unmatched"`.
//
// The build is one pass over the tokens with an explicit stack. It has linear time, no recursion and no exception.
// The caps are in nodes.js. Above a cap the result has `capped` with the reason, and the tree is partial.

import { MAX_AST_DEPTH, MAX_AST_INPUT_BYTES, MAX_AST_NODES, SQL_NODE, TOKEN, node } from "./nodes.js";
import { tokensClickHouse } from "./tokens_ch.js";

// The `children` of every leaf node, and of a node while it has no child. It is frozen: a change of it throws.
// The first child that a container gets makes its own array, with the exact size.
const NO_CHILDREN = Object.freeze([]);

// The frames of the stack.
const SCRIPT = 0;
const STATEMENT = 1;
const WITH = 2;
const CTE = 3;
const QUERY = 4;
const CLAUSE = 5;
const LIST = 6;
const EXPR = 7;
const BR = 8;

// The codes of a punctuation token for the build.
const OPEN_PAREN = 1;
const CLOSE_PAREN = 2;
const OPEN_BRACKET = 3;
const CLOSE_BRACKET = 4;
const OPEN_BRACE = 5;
const CLOSE_BRACE = 6;
const SEMICOLON = 7;
const COMMA = 8;

// The signal of the node cap. A maker of a node throws it when the node would be the 100,001st: the cap is exact, and the tree
// never has more nodes than the cap. The build catches it. It does not leave the module (F42, 10.3c).
const NODE_CAP = Symbol("node cap");


// The words that start the main statement after a WITH list.
const STARTERS = new Set(["SELECT", "INSERT", "UPDATE", "DELETE", "MERGE", "VALUES"]);

// The words that can come before a "(" with no call: the "(" opens a plain parenthesis or a subquery.
const NOT_CALL = new Set([
  "IN", "NOT", "AND", "OR", "ON", "USING", "AS", "FROM", "JOIN", "WHERE", "HAVING", "BY", "SELECT", "DISTINCT", "ALL", "ANY",
  "SOME", "EXISTS", "VALUES", "WHEN", "THEN", "ELSE", "CASE", "BETWEEN", "LIKE", "ILIKE", "UNION", "EXCEPT", "INTERSECT",
  "LIMIT", "OFFSET", "PARTITION", "ORDER", "GROUP", "OVER", "FILTER", "WITHIN", "LATERAL", "INTO", "SET", "RETURNING", "IS",
  "PREWHERE", "QUALIFY", "WITH", "RECURSIVE", "MATERIALIZED", "ESCAPE",
]);

// The words after which a weak clause keyword is a column or an alias, not a clause.
const BLOCK_PREV = new Set([
  "SELECT", "BY", "AS", "DISTINCT", "AND", "OR", "NOT", "ON", "WHERE", "FROM", "HAVING", "JOIN", "IN", "IS", "LIKE", "BETWEEN",
  "THEN", "ELSE", "WHEN", "CASE",
]);

// The clause keywords that are also names of columns or of functions. They need a check of the word before them.
const WEAK = new Set(["FORMAT", "SETTINGS", "WINDOW", "OFFSET", "FETCH", "FOR", "SET", "RETURNING", "VALUES"]);

// The clause keywords that are also names of functions: a "(" after them means a call.
const CALLISH = new Set(["FORMAT", "SETTINGS", "WINDOW", "SET"]);

// The tables of the two dialects: the one-word clause keywords, the words of a join kind, and the lambda.
const CLICKHOUSE = {
  pg: false,
  lambda: true,
  simple: new Set(["SELECT", "FROM", "PREWHERE", "WHERE", "HAVING", "QUALIFY", "WINDOW", "LIMIT", "OFFSET", "SETTINGS", "FORMAT", "VALUES"]),
  mods: new Set(["GLOBAL", "ANY", "ALL", "ASOF", "SEMI", "ANTI", "INNER", "LEFT", "RIGHT", "FULL", "OUTER", "CROSS", "NATURAL", "PASTE", "ARRAY"]),
  first: null,
};
const POSTGRES = {
  pg: true,
  lambda: false,
  simple: new Set(["SELECT", "FROM", "WHERE", "HAVING", "WINDOW", "LIMIT", "OFFSET", "FETCH", "FOR", "SET", "RETURNING", "VALUES"]),
  mods: new Set(["NATURAL", "INNER", "LEFT", "RIGHT", "FULL", "OUTER", "CROSS"]),
  first: null,
};
// The first words of all multi-word clauses and of the set operations.
for (const d of [CLICKHOUSE, POSTGRES]) d.first = new Set([...d.simple, ...d.mods, "GROUP", "ORDER", "UNION", "EXCEPT", "INTERSECT", "JOIN"]);
// In PostgreSQL, ON CONFLICT of an INSERT is a clause too.
POSTGRES.first.add("ON");

// The upper-case form of a keyword of the tables above, or of the words that the build compares. Any other word is not a
// keyword: its entry in the word table of a build is null, and no string is made for it.
const KEYWORDS = new Set([
  ...STARTERS, ...NOT_CALL, ...BLOCK_PREV, ...WEAK, ...CALLISH, ...CLICKHOUSE.first, ...POSTGRES.first,
  "BY", "AS", "NOT", "MATERIALIZED", "RECURSIVE", "WITH", "SELECT", "ALL", "DISTINCT", "JOIN", "FILL", "ARRAY", "VALUES", "LIMIT", "CONFLICT", "ROWS",
]);

// The hash of the upper-case letters of s[a, b): no string is made. The keyword table checks the letters after the hash.
function hashUpper(s, a, b) {
  let h = 0;
  for (let k = a; k < b; k++) {
    let c = s.charCodeAt(k);
    if (c >= 97 && c <= 122) c -= 32;
    h = (Math.imul(h, 37) + c) | 0;
  }
  return h;
}

// The keywords by hash. A collision between two keywords would be a bug: the module stops at load, and a test shows it.
const KEYWORD_BY_HASH = new Map();
let longestKeyword = 0;
for (const w of KEYWORDS) {
  const h = hashUpper(w, 0, w.length);
  if (KEYWORD_BY_HASH.has(h)) throw new Error(`keyword hash collision: ${w}`);
  KEYWORD_BY_HASH.set(h, w);
  if (w.length > longestKeyword) longestKeyword = w.length;
}

// The keyword (upper case) that the word s[a, b) spells in any letter case, or null. It makes no string.
function keywordOf(s, a, b) {
  const len = b - a;
  if (len < 2 || len > longestKeyword) return null;
  const w = KEYWORD_BY_HASH.get(hashUpper(s, a, b));
  if (w === undefined || w.length !== len) return null;
  for (let k = 0; k < len; k++) {
    let c = s.charCodeAt(a + k);
    if (c >= 97 && c <= 122) c -= 32;
    if (c !== w.charCodeAt(k)) return null;
  }
  return w;
}

// The tokenizer of each dialect. The module of PostgreSQL registers its own with `setTokenizer`.
const TOKENIZERS = { clickhouse: tokensClickHouse, postgres: null };

/**
 * Sets the tokenizer of a dialect: `fn(text)` returns the tokens of the common stream. The module that reads PostgreSQL
 * calls this function once. Without it, `parseSql` for `postgres` needs the tokens as its third argument.
 */
export function setTokenizer(dialect, fn) {
  if (dialect in TOKENIZERS && typeof fn === "function") TOKENIZERS[dialect] = fn;
}

// A unit from 0x80: it has two bytes or more in UTF-8.
const NON_ASCII = /[^\x00-\x7f]/;

// The UTF-16 offset where the text passes the cap of the input in UTF-8 bytes, or -1 when it stays within the cap.
function overCap(s) {
  if (s.length * 3 <= MAX_AST_INPUT_BYTES) return -1;
  // A text with no unit from 0x80 has one byte for each unit: the test is native and fast.
  if (!NON_ASCII.test(s)) return s.length <= MAX_AST_INPUT_BYTES ? -1 : MAX_AST_INPUT_BYTES;
  let bytes = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) bytes += 1;
    else if (c < 0x800) bytes += 2;
    else if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
      bytes += 4;
      i += 1;
    } else bytes += 3;
    if (bytes > MAX_AST_INPUT_BYTES) return i;
  }
  return -1;
}

// The offsets of the line feeds of the text, in order.
function lineFeeds(s) {
  const out = [];
  for (let at = s.indexOf("\n"); at >= 0; at = s.indexOf("\n", at + 1)) out.push(at);
  return out;
}

// True when a line feed is in [start, end) : the node spans two lines or more. `feeds` is sorted: binary search.
function spansLines(feeds, start, end) {
  let lo = 0;
  let hi = feeds.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (feeds[mid] < start) lo = mid + 1;
    else hi = mid;
  }
  return lo < feeds.length && feeds[lo] < end;
}

/**
 * Parses a SQL text into a tree. `dialect` is "clickhouse" or "postgres"; another value reads as "clickhouse".
 * `tokens` is the token list of the text when the caller has it already. If it is absent, the tokenizer of the dialect runs.
 * The result is `{ root, tokens, capped }`. `root` is a `script` node that spans the whole text. `capped` is null, or
 * `{ reason, at }` with a reason of `AST_REASONS` when a cap of nodes.js stopped the build.
 * The call never throws. The tokens of the result always cover the text. The tree is never a security check.
 */
export function parseSql(text, dialect = "clickhouse", tokens = null) {
  const s = String(text ?? "");
  const d = dialect === "postgres" ? POSTGRES : CLICKHOUSE;
  let toks = tokens;
  // The cap first: a text over the cap gets one `other` token that covers it, and no token object is made.
  const over = overCap(s);
  if (over >= 0) {
    return { root: node(SQL_NODE.SCRIPT, 0, s.length), tokens: toks || [{ kind: TOKEN.OTHER, start: 0, end: s.length, closed: true }], capped: { reason: "too_large", at: over } };
  }
  if (!toks) {
    const tokenize = TOKENIZERS[dialect === "postgres" ? "postgres" : "clickhouse"];
    // A dialect with no tokenizer yet gives one `other` token: the tokens cover the text and the tree is empty.
    if (!tokenize) {
      const other = s.length > 0 ? [{ kind: TOKEN.OTHER, start: 0, end: s.length, closed: true }] : [];
      return { root: node(SQL_NODE.SCRIPT, 0, s.length), tokens: other, capped: null };
    }
    toks = tokenize(s);
  }
  try {
    return build(s, toks, d);
  } catch {
    // A fault of the builder is a bug. The tree is not a check, so the editor gets an empty tree and goes on.
    return { root: node(SQL_NODE.SCRIPT, 0, s.length), tokens: toks, capped: null };
  }
}

// The builder: one pass over the tokens with an explicit stack of frames.
function build(s, tokens, d) {
  const n = tokens.length;
  const feeds = lineFeeds(s);

  // The first pass: the upper-case word of each word token, the code of each punctuation token and the bracket pairs.
  const kw = new Array(n);
  const brk = new Uint8Array(n);
  const mate = new Int32Array(n).fill(-1);
  const openAt = new Int32Array(n);
  const openType = new Uint8Array(n);
  const openCount = [0, 0, 0];
  let sp = 0;
  for (let i = 0; i < n; i++) {
    const t = tokens[i];
    if (t.kind === TOKEN.WORD) {
      kw[i] = keywordOf(s, t.start, t.end);
    } else if (t.kind === TOKEN.PUNCT && t.end - t.start === 1) {
      const code = punctCode(s.charCodeAt(t.start));
      brk[i] = code;
      if (code === OPEN_PAREN || code === OPEN_BRACKET || code === OPEN_BRACE) {
        const type = (code - 1) >> 1;
        openAt[sp] = i;
        openType[sp] = type;
        sp += 1;
        openCount[type] += 1;
      } else if (code === CLOSE_PAREN || code === CLOSE_BRACKET || code === CLOSE_BRACE) {
        const type = (code - 2) >> 1;
        if (openCount[type] > 0) {
          // Each opener above the match has no closer: it stays at -1.
          while (openType[sp - 1] !== type) {
            sp -= 1;
            openCount[openType[sp]] -= 1;
          }
          sp -= 1;
          openCount[type] -= 1;
          mate[openAt[sp]] = i;
          mate[i] = openAt[sp];
        }
      }
    }
  }

  const root = node(SQL_NODE.SCRIPT, 0, s.length);
  let count = 1;
  let capped = null;
  let brDepth = 0;
  let prevCode = -1; // the index of the last code token that the build used (not a comment)
  let lastCode = 0; // the end of this token
  let carry = []; // the trailing comments of the frames that closed in this step

  // The frames. Each frame has these members.
  // k is the kind. node is the node of the frame. host is the frame that rules the clause words.
  // min is the least end. openIdx is the token index of the opener of a bracket. base is the first child of the body.
  // form is the form of a CTE: 1 for name first, 2 for expression first. asIdx is the token index of the AS of a CTE.
  // cur is, for a query, the node of the open clause. qry is true for a bracket that holds a query.
  // inner is true for a query bracket that has its with frame or query frame.
  const makeFrame = (k, nd, host) => ({ k, node: nd, host, min: nd.start, openIdx: -1, base: 0, form: 0, asIdx: -1, cur: null, qry: false, inner: false });
  const frames = [makeFrame(SCRIPT, root, null)];
  const free = []; // the closed frames, for reuse
  frames[0].host = frames[0];
  let top = frames[0];

  // The makers of the leaf nodes: a literal, a comment, a variable or a macro with no brackets, an error. All the leaves share
  // one frozen empty array for `children`. This saves a large share of the memory of a big tree. A caller must not change a leaf.
  // Each maker writes its extra member in the object literal, so that the engine gives the leaf its final shape at once.
  const mkLiteral = (start, end, kind) => {
    if (++count > MAX_AST_NODES) throw NODE_CAP;
    return { type: SQL_NODE.LITERAL, start, end, children: NO_CHILDREN, kind };
  };
  const mkComment = (start, end, style) => {
    if (++count > MAX_AST_NODES) throw NODE_CAP;
    return { type: SQL_NODE.COMMENT, start, end, children: NO_CHILDREN, style };
  };
  const mkNamedLeaf = (type, start, end, name) => {
    if (++count > MAX_AST_NODES) throw NODE_CAP;
    return { type, start, end, children: NO_CHILDREN, name };
  };
  const mkError = (start, end, error) => {
    if (++count > MAX_AST_NODES) throw NODE_CAP;
    return { type: SQL_NODE.ERROR, start, end, children: NO_CHILDREN, error };
  };
  // The maker of a node with a name or a bracket: a clause, a with, a cte, a bracket node.
  const mkNamed = (type, start, end, name) => {
    if (++count > MAX_AST_NODES) throw NODE_CAP;
    return { type, start, end, children: NO_CHILDREN, name };
  };
  const mkBracket = (type, start, end, open) => {
    if (++count > MAX_AST_NODES) throw NODE_CAP;
    return { type, start, end, children: NO_CHILDREN, open };
  };

  // Makes a node and counts it. It has the shape of `node()`; the caller sets the extra members. This is faster than `node()`.
  const mk = (type, start, end) => {
    if (++count > MAX_AST_NODES) throw NODE_CAP;
    return { type, start, end, children: NO_CHILDREN };
  };

  // Adds a child to a node. The first child makes the array of the node, so an empty node has no array of its own.
  const append = (parent, child) => {
    if (parent.children === NO_CHILDREN) parent.children = [child];
    else parent.children.push(child);
  };

  // Pushes a frame for a node and adds the node to the children of the frame below.
  const pushFrame = (k, nd) => {
    const parent = top;
    append(parent.node, nd);
    const exprLike = k === EXPR || k === LIST || k === CLAUSE;
    const parentLike = parent.k === EXPR || parent.k === LIST || parent.k === CLAUSE;
    let f;
    if (free.length > 0) {
      // A closed frame is reused: the build makes less garbage.
      f = free.pop();
      f.k = k;
      f.node = nd;
      f.min = nd.start;
      f.openIdx = -1;
      f.base = 0;
      f.form = 0;
      f.asIdx = -1;
      f.cur = null;
      f.qry = false;
      f.inner = false;
    } else {
      f = makeFrame(k, nd, null);
    }
    f.host = exprLike ? (parentLike ? parent.host : parent) : f;
    frames.push(f);
    top = f;
    return f;
  };

  // Moves the carried comments into the node of the top frame.
  const flush = () => {
    if (carry.length === 0) return;
    for (let j = 0; j < carry.length; j++) append(top.node, carry[j]);
    carry = [];
  };

  // The index of the next token after `i` that is not a blank and not a comment, or n.
  const nextCode = (i) => {
    let j = i + 1;
    while (j < n && (tokens[j].kind === TOKEN.BLANK || tokens[j].kind === TOKEN.COMMENT)) j += 1;
    return j;
  };

  // Adds the comment nodes of the tokens between the indexes a and b (not included) to the top node.
  const commentsBetween = (a, b) => {
    for (let j = a + 1; j < b; j++) if (tokens[j].kind === TOKEN.COMMENT) addComment(j);
  };

  // Adds the comment of the token i to the top node. The node of the top frame holds it.
  function addComment(i) {
    const t = tokens[i];
    const c = mkComment(t.start, t.end, s.charCodeAt(t.start) === 47 ? "block" : "line");
    if (!t.closed) c.closed = false;
    append(top.node, c);
  }

  // Takes the token i as used. It returns the next index.
  const eat = (i) => {
    prevCode = i;
    lastCode = tokens[i].end;
    return i + 1;
  };

  // Closes the top frame that is not a bracket. The comments after its last code go to `carry`. The caller flushes them to the new top.
  const popFrame = () => {
    const f = frames.pop();
    const nd = f.node;
    const end = f.min > lastCode ? f.min : lastCode;
    nd.end = end;
    const ch = nd.children;
    let c = ch.length;
    while (c > 0 && ch[c - 1].type === SQL_NODE.COMMENT && ch[c - 1].start >= end) c -= 1;
    if (c < ch.length) {
      const tail = ch.splice(c);
      carry = carry.length === 0 ? tail : tail.concat(carry);
      if (c === 0) nd.children = NO_CHILDREN;
    }
    if (nd.type === SQL_NODE.CTE) {
      if (f.form === 2 && f.asIdx >= 0) {
        const j = nextCode(f.asIdx);
        if (j < n && tokens[j].start < end && (tokens[j].kind === TOKEN.WORD || tokens[j].kind === TOKEN.NAME)) {
          nd.name = s.slice(tokens[j].start, tokens[j].end);
        }
      }
      if (spansLines(feeds, nd.start, end)) nd.foldable = true;
    }
    free.push(f);
    top = frames[frames.length - 1];
  };

  // Closes the top frame, a bracket. `closer` is the token that closes it (index `at`). It is null when the bracket is unclosed.
  const popBracket = (closer, at) => {
    const f = frames.pop();
    const nd = f.node;
    if (closer) {
      nd.end = closer.end;
      nd.close = closer.start;
      prevCode = at;
      lastCode = closer.end;
      if (nd.type !== SQL_NODE.MACRO && nd.type !== SQL_NODE.CALL && spansLines(feeds, nd.start, nd.end)) nd.foldable = true;
      if (nd.type === SQL_NODE.PAREN && nd.children.some((c) => c.type === SQL_NODE.LIST)) nd.tuple = true;
    } else {
      nd.error = "unclosed";
      const ch = nd.children;
      let e = lastCode;
      if (ch.length > 0 && ch[ch.length - 1].end > e) e = ch[ch.length - 1].end;
      nd.end = Math.max(e, nd.open + 1);
      if (nd.end > lastCode) lastCode = nd.end;
    }
    brDepth -= 1;
    free.push(f);
    top = frames[frames.length - 1];
  };

  // Pops the frames up to the bracket with the opener index `m`, and closes it with the token `at`.
  const closeBracket = (at, m) => {
    for (;;) {
      if (top.k === BR) {
        flush();
        if (top.openIdx === m) break;
        popBracket(null, -1);
      } else if (top.k === SCRIPT) {
        break;
      } else {
        popFrame();
      }
    }
    if (top.k === BR) popBracket(tokens[at], at);
  };

  // Pops the frames up to a frame of kind `k`, and keeps it. The comments go into it.
  const popTo = (k) => {
    while (top.k !== k && top.k !== SCRIPT) popFrame();
    flush();
  };

  // Makes sure that the top frame can hold the code of an expression: it pushes an expr frame when needed.
  const ensureExpr = (start) => {
    if (top.k === EXPR) return;
    if (top.k === CTE && top.form === 1) return;
    pushFrame(EXPR, mk(SQL_NODE.EXPR, start, start));
  };

  // True when the previous code token is a ".".
  const afterDot = () => prevCode >= 0 && tokens[prevCode].kind === TOKEN.PUNCT && s.charCodeAt(tokens[prevCode].start) === 46;

  // The name and the last token of the clause keyword that `clauseAt` found.
  let kwName = "";
  let kwLast = 0;

  // The join keyword that starts at word i: the modifiers and then JOIN, at most eight words.
  const joinAt = (i) => {
    let j = i;
    let parts = "";
    for (let step = 0; step < 9; step++) {
      const w = kw[j];
      if (!w) return false;
      if (w === "JOIN") {
        kwName = `${parts}JOIN`;
        kwLast = j;
        return true;
      }
      if (!d.mods.has(w)) return false;
      parts += `${w} `;
      j = nextCode(j);
      if (j >= n) return false;
    }
    return false;
  };

  // True when the word i starts a clause of the query. It sets kwName and kwLast.
  const clauseAt = (i) => {
    const w = kw[i];
    if (!w || !d.first.has(w)) return false;
    if (prevCode >= 0) {
      const p = tokens[prevCode];
      const pc = brk[prevCode];
      if (p.kind === TOKEN.PUNCT && s.charCodeAt(p.start) === 46) return false;
      if (WEAK.has(w)) {
        if (p.kind === TOKEN.OPERATOR) return false;
        if (p.kind === TOKEN.PUNCT && pc !== CLOSE_PAREN && pc !== CLOSE_BRACKET && pc !== CLOSE_BRACE) return false;
        if (p.kind === TOKEN.WORD && BLOCK_PREV.has(kw[prevCode])) return false;
      }
      if (p.kind === TOKEN.WORD && kw[prevCode] === "AS") return false;
      if (w === "FROM" && p.kind === TOKEN.WORD && (kw[prevCode] === "FILL" || kw[prevCode] === "DISTINCT" || kw[prevCode] === "ROWS")) return false;
    }
    const nx = nextCode(i);
    if (w === "GROUP" || w === "ORDER") {
      if (nx < n && kw[nx] === "BY") {
        kwName = `${w} BY`;
        kwLast = nx;
        return true;
      }
      return false;
    }
    if (w === "ON") {
      // Only `ON CONFLICT` starts a clause: the ON of a join stays in the join.
      if (nx < n && kw[nx] === "CONFLICT") {
        kwName = "ON CONFLICT";
        kwLast = nx;
        return true;
      }
      return false;
    }
    if (w === "UNION" || w === "EXCEPT" || w === "INTERSECT") {
      if (w === "EXCEPT") {
        // In ClickHouse, `* EXCEPT (a, b)` is a column modifier, not a set operation.
        if (prevCode >= 0 && tokens[prevCode].kind === TOKEN.OPERATOR && s.charCodeAt(tokens[prevCode].start) === 42) return false;
        if (nx < n && brk[nx] === OPEN_PAREN) {
          const f = nextCode(nx);
          if (!(f < n && (kw[f] === "SELECT" || kw[f] === "WITH"))) return false;
        }
      }
      kwName = w;
      kwLast = i;
      if (nx < n && (kw[nx] === "ALL" || kw[nx] === "DISTINCT")) {
        kwName = `${w} ${kw[nx]}`;
        kwLast = nx;
      }
      return true;
    }
    if (d.simple.has(w)) {
      if (CALLISH.has(w) && nx < n && brk[nx] === OPEN_PAREN) return false;
      kwName = w;
      kwLast = i;
      return true;
    }
    return joinAt(i);
  };

  // Starts the clause of the keyword found by `clauseAt` at word i. It closes the clause before. It returns the next index.
  const startClause = (i) => {
    popTo(QUERY);
    const t = tokens[i];
    const endKw = tokens[kwLast].end;
    const nd = mkNamed(SQL_NODE.CLAUSE, t.start, endKw, kwName);
    const cl = pushFrame(CLAUSE, nd);
    top.host.cur = nd;
    cl.min = endKw;
    commentsBetween(i, kwLast);
    prevCode = kwLast;
    lastCode = endKw;
    return kwLast + 1;
  };

  // Pushes a clause with no keyword (a text that starts with no word) or with the first word as its name.
  const pushClause = (name, start, endKw) => {
    const nd = mkNamed(SQL_NODE.CLAUSE, start, endKw, name);
    const cl = pushFrame(CLAUSE, nd);
    top.host.cur = nd;
    cl.min = endKw;
  };

  // The kind of a CTE that starts at the token i: 1 for `name AS (...)`, 2 for `expr AS name`.
  const cteForm = (i) => {
    const t = tokens[i];
    if (t.kind !== TOKEN.WORD && t.kind !== TOKEN.NAME) return 2;
    let k = nextCode(i);
    if (k < n && brk[k] === OPEN_PAREN && mate[k] >= 0) k = nextCode(mate[k]);
    if (k < n && kw[k] === "AS") {
      k = nextCode(k);
      if (k < n && kw[k] === "NOT") k = nextCode(k);
      if (k < n && kw[k] === "MATERIALIZED") k = nextCode(k);
      if (k < n && brk[k] === OPEN_PAREN) return 1;
    }
    return 2;
  };

  // Starts a `with` frame at the word i (WITH). It takes RECURSIVE too. It returns the next index.
  const startWith = (i) => {
    const t = tokens[i];
    const nd = mkNamed(SQL_NODE.WITH, t.start, t.end, "WITH");
    const f = pushFrame(WITH, nd);
    let next = eat(i);
    const j = nextCode(i);
    if (j < n && kw[j] === "RECURSIVE") {
      commentsBetween(i, j);
      f.node.name = "WITH RECURSIVE";
      next = eat(j);
    }
    return next;
  };

  // Starts the query of a statement or of a subquery at the token i (a with frame or a query frame, no token used).
  const startBody = (i) => {
    if (kw[i] === "WITH") return startWith(i);
    const t = tokens[i];
    pushFrame(QUERY, mk(SQL_NODE.QUERY, t.start, t.end));
    return i;
  };

  // Closes the with frame (and the CTE that is open), and starts the main query at the word i. No token is used.
  const endWith = (i) => {
    while (top.k !== WITH && top.k !== SCRIPT) popFrame();
    flush();
    popFrame();
    flush();
    const t = tokens[i];
    pushFrame(QUERY, mk(SQL_NODE.QUERY, t.start, t.end));
    return i;
  };

  // Handles a "," at the index i.
  const comma = (i) => {
    while (top.k === EXPR) popFrame();
    flush();
    if (top.k === CTE) {
      popFrame();
      flush();
      return eat(i);
    }
    if (top.k === LIST) return eat(i);
    const t = tokens[i];
    const ch = top.node.children;
    let idx = ch.length;
    while (idx > top.base && ch[idx - 1].type === SQL_NODE.COMMENT) idx -= 1;
    let list;
    if (idx > top.base && ch[idx - 1].type === SQL_NODE.EXPR) {
      // The first element and the comments after it move into the new list.
      const first = ch[idx - 1];
      list = mk(SQL_NODE.LIST, first.start, first.end);
      const moved = ch.splice(idx - 1);
      list.children = moved;
    } else {
      list = mk(SQL_NODE.LIST, t.start, t.end);
    }
    pushFrame(LIST, list);
    return eat(i);
  };

  // Handles a lambda arrow `->` at the index i (ClickHouse). It returns the next index.
  const lambda = (i) => {
    const t = tokens[i];
    ensureExpr(t.start);
    if (top.k !== EXPR) return eat(i);
    const e = top;
    let start = t.start;
    let params = [];
    let moveFrom = 0;
    const p = prevCode;
    if (p >= 0 && tokens[p].start >= e.node.start && (tokens[p].kind === TOKEN.WORD || tokens[p].kind === TOKEN.NAME)) {
      start = tokens[p].start;
      params = [s.slice(start, tokens[p].end)];
    } else if (p >= 0 && brk[p] === CLOSE_PAREN) {
      const ch = e.node.children;
      let idx = ch.length;
      while (idx > 0 && ch[idx - 1].type === SQL_NODE.COMMENT) idx -= 1;
      const cand = idx > 0 ? ch[idx - 1] : null;
      if (cand && cand.type === SQL_NODE.PAREN && cand.close === tokens[p].start && !cand.bracket) {
        start = cand.start;
        for (let j = mate[p] + 1; j < p; j++) if (tokens[j].kind === TOKEN.WORD) params.push(s.slice(tokens[j].start, tokens[j].end));
      }
    }
    if (start === e.node.start && !e.node.lambda) {
      e.node.lambda = true;
      e.node.params = params;
      e.node.arrow = t.start;
    } else {
      const nd = mk(SQL_NODE.EXPR, start, t.end);
      nd.lambda = true;
      nd.params = params;
      nd.arrow = t.start;
      // The children that start at or after the parameter (the paren, the comments between) move into the lambda.
      const ch = e.node.children;
      moveFrom = ch.length;
      while (moveFrom > 0 && ch[moveFrom - 1].start >= start) moveFrom -= 1;
      if (moveFrom < ch.length) {
        const moved = ch.splice(moveFrom);
        nd.children = moved;
      }
      pushFrame(EXPR, nd);
    }
    return eat(i);
  };

  // The macro `$__name(` at the index i: a bracket frame whose opener is the next token. It returns the next index.
  const macroCall = (i) => {
    const t = tokens[i];
    const o = tokens[i + 1];
    ensureExpr(t.start);
    if (brDepth >= MAX_AST_DEPTH) {
      capped = { reason: "too_deep", at: t.start };
      return n;
    }
    const nd = mkBracket(SQL_NODE.MACRO, t.start, o.end, o.start);
    nd.name = s.slice(t.start, t.end);
    nd.opaque = true;
    const f = pushFrame(BR, nd);
    f.openIdx = i + 1;
    brDepth += 1;
    return eat(i + 1);
  };

  // A bracket that opens at the index i. It returns the next index.
  const opener = (i, code) => {
    const t = tokens[i];
    if (brDepth >= MAX_AST_DEPTH) {
      capped = { reason: "too_deep", at: t.start };
      return n;
    }
    ensureExpr(t.start);
    const prev = prevCode >= 0 ? tokens[prevCode] : null;
    let type;
    let name = null;
    let subscript = false;
    let isQuery = false;
    if (code === OPEN_PAREN) {
      const nx = nextCode(i);
      const first = nx < n ? kw[nx] : null;
      isQuery = first === "SELECT" || first === "WITH" || (d.pg && first === "VALUES");
      // A data-modifying statement in a bracket (a PostgreSQL CTE body) is a query when a word or a name follows its first word.
      if (!isQuery && d.pg && (first === "INSERT" || first === "UPDATE" || first === "DELETE" || first === "MERGE")) {
        const after = nextCode(nx);
        isQuery = after < n && (tokens[after].kind === TOKEN.WORD || tokens[after].kind === TOKEN.NAME);
      }
      let callee = null;
      if (prev && (prev.kind === TOKEN.NAME || (prev.kind === TOKEN.WORD && !NOT_CALL.has(kw[prevCode])))) callee = s.slice(prev.start, prev.end);
      if (isQuery) type = SQL_NODE.SUBQUERY;
      else type = callee ? SQL_NODE.CALL : SQL_NODE.PAREN;
      name = callee;
    } else if (code === OPEN_BRACKET) {
      type = SQL_NODE.ARRAY;
      if (prev && prev.end === t.start && (prev.kind === TOKEN.NAME || (prev.kind === TOKEN.WORD && kw[prevCode] !== "ARRAY") || brk[prevCode] === CLOSE_PAREN || brk[prevCode] === CLOSE_BRACKET)) {
        subscript = true;
      }
    } else {
      type = SQL_NODE.PAREN;
    }
    const nd = mkBracket(type, t.start, t.end, t.start);
    if (name) nd.name = name;
    if (subscript) nd.subscript = true;
    if (code === OPEN_BRACE) nd.bracket = "{";
    const f = pushFrame(BR, nd);
    f.openIdx = i;
    f.qry = isQuery;
    brDepth += 1;
    return eat(i);
  };

  // The leaf node of a variable, a macro or a parameter token.
  const placeholderNode = (i) => {
    const t = tokens[i];
    const raw = s.slice(t.start, t.end);
    if (t.kind === TOKEN.MACRO) return mkNamedLeaf(SQL_NODE.MACRO, t.start, t.end, raw);
    if (raw.charCodeAt(1) === 123) {
      const inner = raw.slice(2, -1);
      const colon = inner.indexOf(":");
      const nd = mkNamedLeaf(SQL_NODE.VARIABLE, t.start, t.end, colon < 0 ? inner : inner.slice(0, colon));
      if (colon >= 0) nd.format = inner.slice(colon + 1);
      return nd;
    }
    return mkNamedLeaf(SQL_NODE.VARIABLE, t.start, t.end, raw.slice(1));
  };

  // Handles the token i. It returns the index of the next token to read: i itself when the state changed and the token waits.
  const handle = (i) => {
    const t = tokens[i];
    const kind = t.kind;
    const code = brk[i];
    // A statement starts at the first token. A ";" at the top level is a statement that ends at once.
    if (top.k === SCRIPT) {
      pushFrame(STATEMENT, mk(SQL_NODE.STATEMENT, t.start, t.end));
      if (code === SEMICOLON) return endStatement(i);
    }
    if (top.k === STATEMENT || (top.k === BR && top.qry && !top.inner)) {
      if (top.k === BR) top.inner = true;
      const next = startBody(i);
      if (next !== i) return next;
    }
    if (code === SEMICOLON && brDepth === 0) return endStatement(i);
    if (top.k === WITH && code !== CLOSE_PAREN && code !== CLOSE_BRACKET && code !== CLOSE_BRACE) {
      if (code === COMMA) return eat(i);
      if (kind === TOKEN.WORD && STARTERS.has(kw[i])) return endWith(i);
      const form = cteForm(i);
      const nd = form === 1 ? mkNamed(SQL_NODE.CTE, t.start, t.end, s.slice(t.start, t.end)) : mk(SQL_NODE.CTE, t.start, t.end);
      nd.form = form === 1 ? "name" : "expr";
      pushFrame(CTE, nd).form = form;
    }
    if (top.k === QUERY) {
      // The first token of a query: a clause keyword, a first word that names the clause, or a clause with no name.
      if (kind === TOKEN.WORD) {
        if (clauseAt(i)) return startClause(i);
        const word = kw[i] ?? s.slice(t.start, t.end).toUpperCase();
        pushClause(word, t.start, t.end);
        return eat(i);
      }
      pushClause("", t.start, t.start);
    }
    if (code === CLOSE_PAREN || code === CLOSE_BRACKET || code === CLOSE_BRACE) {
      const m = mate[i];
      if (m < 0) {
        ensureExpr(t.start);
        append(top.node, mkError(t.start, t.end, "unmatched"));
        return eat(i);
      }
      closeBracket(i, m);
      return i + 1;
    }
    if (code === COMMA) return comma(i);
    const host = top.k === EXPR || top.k === LIST || top.k === CLAUSE ? top.host : top;
    if (kind === TOKEN.WORD) {
      const w = kw[i];
      if (host.k === QUERY) {
        if (w === "BY" && !d.pg && host.cur && host.cur.name === "LIMIT") {
          popTo(CLAUSE);
          top.node.name = "LIMIT BY";
          top.node.by = t.start;
          top.base = top.node.children.length;
          return eat(i);
        }
        if (clauseAt(i)) return startClause(i);
      } else if (host.k === CTE && w !== null && STARTERS.has(w) && !afterDot()) {
        return endWith(i);
      }
      if (host.k === CTE && host.form === 2 && w === "AS") host.asIdx = i;
      ensureExpr(t.start);
      return eat(i);
    }
    if (code === OPEN_PAREN || code === OPEN_BRACKET || code === OPEN_BRACE) return opener(i, code);
    if (kind === TOKEN.OPERATOR && d.lambda && t.end - t.start === 2 && s.charCodeAt(t.start) === 45 && s.charCodeAt(t.start + 1) === 62) {
      return lambda(i);
    }
    if (kind === TOKEN.MACRO && i + 1 < n && brk[i + 1] === OPEN_PAREN && tokens[i + 1].start === t.end && s.charCodeAt(t.start + 1) !== 123) {
      return macroCall(i);
    }
    ensureExpr(t.start);
    if (kind === TOKEN.STRING || kind === TOKEN.NUMBER) {
      const lit = mkLiteral(t.start, t.end, kind === TOKEN.STRING ? "string" : "number");
      if (!t.closed) lit.closed = false;
      append(top.node, lit);
    } else if (kind === TOKEN.PARAM) {
      // A parameter (`$1`, `{p:Type}`) is an opaque literal of the kind "param", never a variable of the document.
      append(top.node, mkLiteral(t.start, t.end, "param"));
    } else if (kind === TOKEN.MACRO || kind === TOKEN.VARIABLE) {
      append(top.node, placeholderNode(i));
    }
    return eat(i);
  };

  // Takes the token i when the top frame is an expr and the token only goes into it.
  // These tokens are: a word that is no clause keyword, a number or a string (it adds a literal node), and an operator
  // that is not the lambda arrow. A name, an unknown unit, and a punctuation that is no bracket, comma or semicolon also go in.
  // It returns false when the token needs the full rules.
  const inExpression = (i, t) => {
    const kind = t.kind;
    if (kind === TOKEN.WORD) {
      const w = kw[i];
      if (!w) return true;
      const hostKind = top.host.k;
      return (hostKind !== QUERY && hostKind !== CTE) || (hostKind === QUERY && w !== "BY" && !d.first.has(w));
    }
    if (kind === TOKEN.NUMBER || kind === TOKEN.STRING) {
      const lit = mkLiteral(t.start, t.end, kind === TOKEN.STRING ? "string" : "number");
      if (!t.closed) lit.closed = false;
      append(top.node, lit);
      return true;
    }
    if (kind === TOKEN.OPERATOR) return !(d.lambda && t.end - t.start === 2 && s.charCodeAt(t.start) === 45 && s.charCodeAt(t.start + 1) === 62);
    return kind === TOKEN.NAME || kind === TOKEN.OTHER || kind === TOKEN.PUNCT;
  };

  // Ends the statement at the ";" of the index i: it closes the frames up to the statement, then the statement.
  function endStatement(i) {
    popTo(STATEMENT);
    lastCode = tokens[i].end;
    prevCode = i;
    top.node.semicolon = tokens[i].start;
    popFrame();
    flush();
    return i + 1;
  }

  // The main loop. A guard stops a loop with no progress: the token is then used as it is.
  // A maker of a node throws NODE_CAP at the 100,001st node, so the cap is exact. The tokens still cover the text.
  let i = 0;
  let spin = 0;
  try {
    while (i < n) {
      const t = tokens[i];
      if (t.kind === TOKEN.BLANK) {
        i += 1;
        continue;
      }
      if (t.kind === TOKEN.COMMENT) {
        addComment(i);
        i += 1;
        continue;
      }
      if (top.k === EXPR && brk[i] === 0 && inExpression(i, t)) {
        // The common case, a token inside an expression that changes no frame: the build uses it at once.
        prevCode = i;
        lastCode = t.end;
        i += 1;
        spin = 0;
        continue;
      }
      let next = handle(i);
      if (capped) break;
      if (next === i) {
        spin += 1;
        if (spin > 8) next = eat(i);
      } else {
        spin = 0;
      }
      i = next;
    }
    if (!capped) {
      // The text ends: the open frames close. A bracket that is still open is unclosed.
      while (top.k !== SCRIPT) {
        if (top.k === BR) {
          flush();
          popBracket(null, -1);
        } else {
          popFrame();
        }
      }
      flush();
    }
  } catch (error) {
    if (error !== NODE_CAP) throw error;
    capped = { reason: "too_many_nodes", at: tokens[Math.min(i, n - 1)].start };
  }

  if (capped) {
    // A partial tree: each open node runs to the end of the text.
    for (const f of frames) f.node.end = s.length;
    return { root, tokens, capped };
  }
  return { root, tokens, capped: null };
}

// The code of a punctuation unit for the build: the six brackets, the semicolon and the comma. All other units give 0.
function punctCode(c) {
  switch (c) {
    case 40: return OPEN_PAREN;
    case 41: return CLOSE_PAREN;
    case 91: return OPEN_BRACKET;
    case 93: return CLOSE_BRACKET;
    case 123: return OPEN_BRACE;
    case 125: return CLOSE_BRACE;
    case 59: return SEMICOLON;
    case 44: return COMMA;
    default: return 0;
  }
}
