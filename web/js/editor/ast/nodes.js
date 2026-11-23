// The shared part of the minimal syntax tree of the query editor (F42). Pure module: no DOM access.
// The tree gives structure only: the format, the folds, the bracket pairs and the current block. It is never a security
// check. The server stays the only authority on the validity of a query, and a fault of the tree never stops a save or a run.
// The editor loads this module on demand, so it is not in the page measure of 9.30.

/** The largest input in UTF-8 bytes, as the body of a query (12.16). Above it, the structure functions stop and the colours stay. */
export const MAX_AST_INPUT_BYTES = 262144;

/** The deepest nesting of brackets and queries. A deeper text gives a partial tree with the reason `too_deep`. */
export const MAX_AST_DEPTH = 128;

/** The largest number of nodes in one tree. A larger tree stops with the reason `too_many_nodes`. */
export const MAX_AST_NODES = 100000;

/** The largest number of fold markers that the gutter shows. Above it, the folds stop with a notice (10.3e). */
export const MAX_FOLD_MARKERS = 2000;

/** The reasons of a tree that is not complete. The editor shows one notice for each reason. */
export const AST_REASONS = Object.freeze(["too_large", "too_deep", "too_many_nodes"]);

/** The kinds of a token of the common token stream. Each token has `kind`, `start`, `end` and `closed`. */
export const TOKEN = Object.freeze({
  BLANK: "blank",
  COMMENT: "comment",
  STRING: "string",
  NAME: "name",
  WORD: "word",
  NUMBER: "number",
  PUNCT: "punct",
  OPERATOR: "operator",
  MACRO: "macro",
  VARIABLE: "variable",
  PARAM: "param",
  OTHER: "other",
});

/** The types of a node of a SQL tree. */
export const SQL_NODE = Object.freeze({
  SCRIPT: "script",
  STATEMENT: "statement",
  WITH: "with",
  CTE: "cte",
  QUERY: "query",
  SUBQUERY: "subquery",
  CLAUSE: "clause",
  LIST: "list",
  PAREN: "paren",
  ARRAY: "array",
  CALL: "call",
  EXPR: "expr",
  LITERAL: "literal",
  MACRO: "macro",
  VARIABLE: "variable",
  COMMENT: "comment",
  ERROR: "error",
});

/** The types of a node of a JSON tree (the pipeline of a MongoDB query). */
export const JSON_NODE = Object.freeze({
  DOC: "doc",
  OBJECT: "object",
  MEMBER: "member",
  ARRAY: "array",
  STRING: "string",
  NUMBER: "number",
  LITERAL: "literal",
  ERROR: "error",
});

/**
 * Makes a node. `start` and `end` are UTF-16 offsets in the text, as the selection of a text area; `end` is not in the node.
 * `extra` can hold `name` (the upper-case keywords of a clause, or the name of a call), `open` and `close` (the offsets of the
 * brackets of the node), `error` (a short reason) and `foldable` (true when the node can fold).
 */
export function node(type, start, end, children = [], extra = null) {
  const out = { type, start, end, children };
  if (extra) Object.assign(out, extra);
  return out;
}

/** The UTF-8 length of a text, with no allocation. */
export function utf8Length(text) {
  let n = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c <= 0xdbff && i + 1 < text.length) {
      n += 4;
      i += 1;
    } else n += 3;
  }
  return n;
}

/** Each node of a tree in document order, with no recursion. `visit(node, depth)` returns false to skip the children. */
export function walk(root, visit) {
  const stack = [[root, 0]];
  while (stack.length) {
    const [n, depth] = stack.pop();
    if (visit(n, depth) === false) continue;
    for (let i = n.children.length - 1; i >= 0; i--) stack.push([n.children[i], depth + 1]);
  }
}
