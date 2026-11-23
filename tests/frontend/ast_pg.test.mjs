// The token stream and the tree of the dialect `postgres` of the minimal syntax tree of the query editor (F42).
// The vectors are in tests/fixtures/ast/pg_tokens.json (tokens), pg_trees.json (tree summaries) and pg_gaps.json (rules
// that the tree has since the rules were added; they run as normal tests). The tree is never a security check: no test here reads it as one.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { findMacros, findVariableNames } from "../../web/js/sqlscan.js";
import { KIND, lexPg } from "../../web/js/sql_lex_pg.js";
import { MAX_AST_DEPTH, MAX_AST_INPUT_BYTES, MAX_AST_NODES, TOKEN, utf8Length, walk } from "../../web/js/editor/ast/nodes.js";
import { tokensPostgres } from "../../web/js/editor/ast/tokens_pg.js";
import { tokensClickHouse } from "../../web/js/editor/ast/tokens_ch.js";
import { parseSql } from "../../web/js/editor/ast/sql_tree.js";

const load = (name) => JSON.parse(readFileSync(new URL(`../fixtures/ast/${name}`, import.meta.url), "utf8"));
const TOKEN_VECTORS = load("pg_tokens.json").vectors;
const TREE_VECTORS = load("pg_trees.json");
const GAP_VECTORS = load("pg_gaps.json");

const KINDS = new Set(Object.values(TOKEN));

// The non-blank tokens as "kind:text". A "!" after the kind marks an unclosed token.
function compact(text, tokens = tokensPostgres(text)) {
  return tokens.filter((t) => t.kind !== TOKEN.BLANK).map((t) => `${t.kind}${t.closed ? "" : "!"}:${text.slice(t.start, t.end)}`);
}

// The invariants of every token list: no gap, no overlap, no empty token, a known kind, a boolean `closed`.
function assertCovers(text, tokens, label = "") {
  let at = 0;
  for (const t of tokens) {
    assert.equal(t.start, at, `${label} gap or overlap at ${at}`);
    assert.ok(t.end > t.start, `${label} empty token at ${at}`);
    assert.ok(KINDS.has(t.kind), `${label} unknown kind ${t.kind}`);
    assert.equal(typeof t.closed, "boolean", `${label} closed at ${at}`);
    at = t.end;
  }
  assert.equal(at, text.length, `${label} the tokens stop at ${at} of ${text.length}`);
}

// The summary of a tree, in the form of the members of the vectors of pg_trees.json.
function summary(text, result) {
  const o = { statements: 0, queries: [], withs: [], ctes: [], subqueries: 0, macros: [], variables: [], foldable: [], errors: [] };
  walk(result.root, (n) => {
    if (n.type === "statement") o.statements += 1;
    if (n.type === "query") o.queries.push(n.children.filter((c) => c.type === "clause").map((c) => c.name));
    if (n.type === "with") o.withs.push(n.name);
    if (n.type === "cte") o.ctes.push(n.name);
    if (n.type === "subquery") o.subqueries += 1;
    if (n.type === "macro") o.macros.push(text.slice(n.start, n.end));
    if (n.type === "variable") o.variables.push(text.slice(n.start, n.end));
    if (n.error) o.errors.push(n.error);
    if (n.foldable) o.foldable.push(n.type);
  });
  return o;
}

// The invariants of a tree: the root spans the text, each child lies inside its parent, the children keep the order of the text,
// and each code bracket belongs to one node with `open` or `close`, or to an `error` node with the reason "unmatched".
function assertTree(text, result, label = "") {
  const { root, tokens } = result;
  assert.equal(root.type, "script", `${label} root type`);
  assert.equal(root.start, 0, `${label} root start`);
  assert.equal(root.end, text.length, `${label} root end`);
  const owned = new Set();
  walk(root, (n) => {
    assert.ok(n.start >= 0 && n.end >= n.start && n.end <= text.length, `${label} node ${n.type} range`);
    let prevEnd = n.start;
    for (const c of n.children) {
      assert.ok(c.start >= n.start && c.end <= n.end, `${label} ${c.type} [${c.start},${c.end}) is outside ${n.type} [${n.start},${n.end})`);
      assert.ok(c.start >= prevEnd, `${label} ${c.type} at ${c.start} overlaps its previous sibling`);
      prevEnd = c.end;
    }
    if (n.open !== undefined) owned.add(n.open);
    if (n.close !== undefined) owned.add(n.close);
    if (n.type === "error" && n.error === "unmatched") owned.add(n.start);
    return true;
  });
  for (const t of tokens) {
    if (t.kind === TOKEN.PUNCT && t.end - t.start === 1 && "()[]{}".includes(text[t.start])) {
      assert.ok(owned.has(t.start), `${label} the bracket at ${t.start} belongs to no node`);
    }
  }
}

// ---------------------------------------------------------------- the token stream

test("the tokens of the vectors have the kinds and the texts of the file", () => {
  assert.ok(TOKEN_VECTORS.length >= 35);
  for (const v of TOKEN_VECTORS) {
    const tokens = tokensPostgres(v.text);
    assertCovers(v.text, tokens, v.name);
    assert.deepEqual(compact(v.text, tokens), v.tokens, v.name);
  }
});

test("each kind of lexPg maps to the kind of the common stream that the module documents", () => {
  const cases = [
    [KIND.BLANK, " \n", TOKEN.BLANK],
    [KIND.LINE, "-- a", TOKEN.COMMENT],
    [KIND.BLOCK, "/* a */", TOKEN.COMMENT],
    [KIND.WORD, "abc", TOKEN.WORD],
    [KIND.NAME, '"a b"', TOKEN.NAME],
    [KIND.STRING, "'a'", TOKEN.STRING],
    [KIND.ESTRING, "E'a'", TOKEN.STRING],
    [KIND.PREFIXED, "B'1'", TOKEN.STRING],
    [KIND.HEREDOC, "$t$ a $t$", TOKEN.STRING],
    [KIND.NUMBER, "12", TOKEN.NUMBER],
    [KIND.PUNCT, ",", TOKEN.PUNCT],
    [KIND.OPERATOR, "->>", TOKEN.OPERATOR],
    [KIND.PARAM, "$3", TOKEN.PARAM],
  ];
  for (const [kind, text, want] of cases) {
    const lex = lexPg(text);
    assert.equal(lex.length, 1, text);
    assert.equal(lex[0].kind, kind, `the lexer reads ${text} as ${kind}`);
    const tokens = tokensPostgres(text);
    assert.equal(tokens.length, 1, text);
    assert.equal(tokens[0].kind, want, text);
    assert.equal(tokens[0].closed, true, text);
  }
});

test("the unclosed tokens have closed false, as in lexPg, except a line comment", () => {
  for (const text of ["'a", "E'a", "B'a", '"a', "/* a", "$$ a", "$t$ a", 'U&"a']) {
    const tokens = tokensPostgres(text);
    assert.equal(tokens.length, 1, text);
    assert.equal(tokens[0].closed, false, text);
    assert.equal(tokens[0].end, text.length, text);
  }
  assert.equal(tokensPostgres("-- a")[0].closed, true);
});

test("the tokens agree with lexPg on the offsets of the strings, the names and the comments", () => {
  const text = "SELECT 'a''b', \"c\"\"d\", E'x\\'y', $q$ z $q$ /* c /* n */ */ -- l\nFROM t";
  const fromLex = lexPg(text)
    .filter((t) => [KIND.STRING, KIND.ESTRING, KIND.PREFIXED, KIND.HEREDOC, KIND.NAME, KIND.LINE, KIND.BLOCK].includes(t.kind))
    .map((t) => [t.start, t.end]);
  const mine = tokensPostgres(text)
    .filter((t) => [TOKEN.STRING, TOKEN.NAME, TOKEN.COMMENT].includes(t.kind))
    .map((t) => [t.start, t.end]);
  assert.deepEqual(mine, fromLex);
});

test("a line end belongs to the blank token after a line comment, LF or CRLF", () => {
  for (const eol of ["\n", "\r\n", "\r"]) {
    const text = `a -- c${eol}b`;
    const tokens = tokensPostgres(text);
    const c = tokens.find((t) => t.kind === TOKEN.COMMENT);
    assert.equal(text.slice(c.start, c.end), "-- c");
    const after = tokens[tokens.indexOf(c) + 1];
    assert.equal(after.kind, TOKEN.BLANK);
    assert.equal(text.slice(after.start, after.end), eol);
  }
});

test("a placeholder inside a string, a name, a dollar quote or a comment never splits the token", () => {
  for (const text of ["'$__timeFrom ${v:csv} $x'", '"$x"', "$$ $x ${y} $__m $$", "$t$ $x $t$", "/* $x */", "-- ${y}", "U&'$x'"]) {
    const tokens = tokensPostgres(text);
    assert.equal(tokens.length, 1, text);
    assert.ok([TOKEN.STRING, TOKEN.NAME, TOKEN.COMMENT].includes(tokens[0].kind), text);
  }
});

test("the macros and the variables are one token each, in the places of the code", () => {
  const places = [
    "SELECT $x",
    "SELECT a FROM $t",
    "SELECT a FROM t WHERE c = $x AND d IN ($y)",
    "SELECT a FROM t WHERE c IN (${y:csv}, ${z})",
    "SELECT $x::int, ($y)::text, a[$i], a->$k",
    "SELECT a FROM t GROUP BY $x ORDER BY $x LIMIT $x OFFSET $x",
    "WITH w AS (SELECT $x) SELECT * FROM w, LATERAL (SELECT $y) l",
    "SELECT sum(x) FILTER (WHERE a = $x) OVER (PARTITION BY $y) FROM t",
    "SELECT ARRAY[$x, $y], '{1,2}'::int[] || ARRAY[$z]",
    "SELECT $__timeFilter(ts), $__timeFrom(), $__timeGroupAlias(ts, '1h'), ${__x}",
  ];
  for (const text of places) {
    const tokens = tokensPostgres(text);
    assertCovers(text, tokens, text);
    const found = tokens.filter((t) => t.kind === TOKEN.MACRO || t.kind === TOKEN.VARIABLE).map((t) => text.slice(t.start, t.end));
    const expected = [...text.matchAll(/\$\{[A-Za-z0-9_]+(?::[A-Za-z0-9_]+)?\}|\$[A-Za-z_][A-Za-z0-9_]*/g)].map((m) => m[0]);
    assert.deepEqual(found, expected, text);
    for (const t of tokens) {
      if (t.kind === TOKEN.MACRO) assert.match(text.slice(t.start, t.end), /^\$\{?__/);
      if (t.kind === TOKEN.VARIABLE) assert.doesNotMatch(text.slice(t.start, t.end), /^\$\{?__/);
    }
  }
});

test("the placeholder tokens are the same as the tokens of tokens_ch.js on code that both lexers read alike", () => {
  const codeOnly = [
    "SELECT $x, ${y}, ${z:csv}, $__timeFilter(ts), $__interval, $_a, $a1b2 FROM t",
    "SELECT ${a} + $b * $__c - $d",
    "SELECT $x::int, $y.z, $w)",
    "SELECT ${ bad }, ${a:}, $, $-1, ${__m:json}",
    "WHERE a = $a AND b = $__b OR c = ${c}",
  ];
  const pick = (text, tokens) => tokens.filter((t) => [TOKEN.MACRO, TOKEN.VARIABLE, TOKEN.OTHER].includes(t.kind)).map((t) => [t.kind, t.start, t.end]);
  for (const text of codeOnly) {
    assert.deepEqual(pick(text, tokensPostgres(text)), pick(text, tokensClickHouse(text)), text);
  }
});

test("the macros and the variables of the code tokens are the names that sqlscan.js finds in code", () => {
  const codeOnly = [
    "SELECT $x, ${y}, ${z:csv}, $__timeFilter(ts), $__interval, $_a, $a1b2 FROM t",
    "SELECT a$b, $c $d, ${e}f WHERE $__g() AND ${h:csv}",
    "SELECT a FROM t WHERE c IN ($y) AND $__timeFrom > $z",
  ];
  for (const text of codeOnly) {
    const tokens = tokensPostgres(text);
    const name = (t) => text.slice(t.start, t.end).replace(/^\$\{?/, "").replace(/[:}].*$/, "");
    const macros = tokens.filter((t) => t.kind === TOKEN.MACRO).map(name);
    const vars = tokens.filter((t) => t.kind === TOKEN.VARIABLE).map(name);
    // The tokens hold the occurrences, sqlscan.js the distinct names: the sets agree.
    assert.deepEqual([...new Set(macros)].sort(), findMacros(text).sort(), text);
    assert.deepEqual([...new Set(vars)].sort(), findVariableNames(text).sort(), text);
  }
});

test("a dollar and digits is a parameter, and a dollar, digits and more is a variable", () => {
  assert.deepEqual(compact("$1 $22 $1a $a1"), ["param:$1", "param:$22", "variable:$1a", "variable:$a1"]);
});

test("the time is linear for texts with many runs and no dollar sign", () => {
  const time = (unit, units) => {
    const text = unit.repeat(units);
    return bestOf(7, () => tokensPostgres(text));
  };
  for (const unit of ["--\n", "a\n", "'a'\n", "/*x*/\n", ";\n", "a$b\n"]) {
    const small = time(unit, 16384);
    const big = time(unit, 262144);
    // Sixteen times the text, best of 7 runs after 2 warm-ups (the policy of the timing tests). A linear time gave a ratio of 25 to 38
    // on 2026-11-21, also with four other runs in parallel (the small text fits the caches, the big text does not). A quadratic time
    // gives 256. The bound 96 keeps a margin of more than 2 on each side; the old bound (24 for eight times the text) failed under the
    // load of the full front suite (ID-K324 checks).
    assert.ok(big < Math.max(small, 0.5) * 96, `${JSON.stringify(unit)}: ${small.toFixed(2)} ms then ${big.toFixed(2)} ms`);
  }
});

test("a word that holds a dollar and a name gives the variable that sqlscan.js finds in it", () => {
  for (const text of ["a$$b$$", "a$b", "x$1", "a$$b", "SELECT a$b FROM t"]) {
    const tokens = tokensPostgres(text);
    assertCovers(text, tokens, text);
    const names = tokens.filter((t) => t.kind === TOKEN.VARIABLE).map((t) => text.slice(t.start, t.end).replace(/^\$/, ""));
    assert.deepEqual(names, findVariableNames(text), text);
  }
  assert.deepEqual(compact("a$ ab$ a$$"), ["word:a$", "word:ab$", "word:a$$"]);
});

test("the call never throws and always covers the text, for any input", () => {
  for (const v of [undefined, null, "", 0, 12, {}, [], "\u0000", "\ud800", "a\udc00b", "$".repeat(50), "'".repeat(7), "${".repeat(9)]) {
    const tokens = tokensPostgres(v);
    assertCovers(String(v ?? ""), tokens, String(v));
  }
});

test("a seeded random text of the PostgreSQL alphabet gives tokens that cover it", () => {
  const alphabet = ["SELECT", " ", "\n", "'", '"', "$", "$$", "$t$", "{", "}", "(", ")", "[", "]", ":", "::", "-", "--", "/*", "*/", "*", "->", "->>", "||", "$1", "${v}", "${v:csv}", "$__m", "E'", "\\", "é", "𝄞", ",", ";", ".", "1", "e", "@", "?", "#", "x"];
  let seed = 20261114;
  const next = () => (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0);
  for (let n = 0; n < 400; n++) {
    let text = "";
    for (let k = next() % 60; k >= 0; k--) text += alphabet[next() % alphabet.length];
    const tokens = tokensPostgres(text);
    assertCovers(text, tokens, JSON.stringify(text));
    const result = parseSql(text, "postgres", tokens);
    assertTree(text, result, JSON.stringify(text));
  }
});

// ---------------------------------------------------------------- the tree

test("parseSql reads the tokens of this module for the dialect postgres", () => {
  const text = "SELECT $$ a $$::text, ${v}";
  const a = parseSql(text, "postgres");
  const b = parseSql(text, "postgres", tokensPostgres(text));
  assert.deepEqual(a.tokens, b.tokens);
  assert.deepEqual(a.root, b.root);
  assert.ok(a.tokens.some((t) => t.kind === TOKEN.VARIABLE), "the tokenizer of the dialect is registered");
});

test("the tree of each vector has the statements, the clauses, the CTEs, the folds, the macros and the variables of the file", () => {
  assert.ok(TREE_VECTORS.length >= 35);
  for (const v of TREE_VECTORS) {
    const result = parseSql(v.text, "postgres");
    assert.equal(result.capped, null, v.name);
    assertCovers(v.text, result.tokens, v.name);
    assertTree(v.text, result, v.name);
    const { name, text, ...want } = v;
    assert.deepEqual(summary(v.text, result), want, name);
  }
});

test("a macro or a variable is a leaf or an opaque node with the exact bytes of the text", () => {
  const text = "SELECT $__timeGroupAlias(ts, $__interval), ${host:csv}, $x FROM t WHERE $__timeFilter(ts)";
  const { root } = parseSql(text, "postgres");
  const found = [];
  walk(root, (n) => {
    if (n.type === "macro" || n.type === "variable") found.push([n.type, text.slice(n.start, n.end)]);
  });
  assert.deepEqual(found, [
    ["macro", "$__timeGroupAlias(ts, $__interval)"],
    ["macro", "$__interval"],
    ["variable", "${host:csv}"],
    ["variable", "$x"],
    ["macro", "$__timeFilter(ts)"],
  ]);
});

test("the dollar quote is one literal: a semicolon or a keyword inside it changes nothing", () => {
  const text = "SELECT $f$ ; FROM ) ( WHERE $f$ AS body FROM t";
  const result = parseSql(text, "postgres");
  assertTree(text, result);
  const s = summary(text, result);
  assert.equal(s.statements, 1);
  assert.deepEqual(s.queries, [["SELECT", "FROM"]]);
  assert.deepEqual(s.errors, []);
});

test("a cast, a JSON operator and an array type never make a clause or a bracket node by themselves", () => {
  const text = "SELECT a::int[], j->>'k', j#>'{a}', '{1,2}'::int[] FROM t";
  const result = parseSql(text, "postgres");
  assertTree(text, result);
  assert.deepEqual(summary(text, result).queries, [["SELECT", "FROM"]]);
});

test("a multi-line subquery, CTE, parenthesis and array fold, and a one-line one does not", () => {
  const folds = (text) => summary(text, parseSql(text, "postgres")).foldable;
  assert.deepEqual(folds("SELECT * FROM (\n  SELECT 1\n) s"), ["subquery"]);
  assert.deepEqual(folds("SELECT * FROM (SELECT 1) s"), []);
  assert.deepEqual(folds("WITH a AS (\n SELECT 1\n) SELECT * FROM a"), ["cte", "subquery"]);
  assert.deepEqual(folds("SELECT (\n1\n+ 2)"), ["paren"]);
  assert.deepEqual(folds("SELECT ARRAY[\n1,\n2\n]"), ["array"]);
  assert.deepEqual(folds("SELECT ARRAY[1,\n2]::int[]"), ["array"]);
});

test("the offsets of the tree are UTF-16 offsets", () => {
  const text = "SELECT '𝄞é' AS x, $v FROM t";
  const { root } = parseSql(text, "postgres");
  let variable = null;
  walk(root, (n) => {
    if (n.type === "variable") variable = n;
  });
  assert.equal(text.slice(variable.start, variable.end), "$v");
});

// ---------------------------------------------------------------- tolerance, caps, time

// A sample of 20 texts: the vectors that use most of the PostgreSQL forms.
const SAMPLE_NAMES = [
  "with_recursive_fold", "cte_multi_fold", "subqueries_places", "lateral", "filter_and_window", "arrays", "dollar_do",
  "dollar_semicolon_inside", "macros_and_variables", "macro_without_parens", "placeholder_in_string_name", "set_operations",
  "fetch_for", "comments", "values_subquery", "from_inside_call", "json_ops_casts", "joins_kinds", "generate_series_exists",
  "paren_multiline_fold",
];

test("every prefix of 20 samples gives tokens that cover it and a tree with no exception", () => {
  assert.equal(SAMPLE_NAMES.length, 20);
  let checked = 0;
  for (const name of SAMPLE_NAMES) {
    const v = TREE_VECTORS.find((x) => x.name === name);
    assert.ok(v, name);
    for (let n = 0; n <= v.text.length; n++) {
      const prefix = v.text.slice(0, n);
      const tokens = tokensPostgres(prefix);
      assertCovers(prefix, tokens, `${name}[0,${n})`);
      const result = parseSql(prefix, "postgres", tokens);
      assertTree(prefix, result, `${name}[0,${n})`);
      checked += 1;
    }
  }
  assert.ok(checked > 1500);
});

test("every prefix of the token vectors gives tokens that cover it", () => {
  for (const v of TOKEN_VECTORS) {
    for (let n = 0; n <= v.text.length; n++) assertCovers(v.text.slice(0, n), tokensPostgres(v.text.slice(0, n)), `${v.name}[0,${n})`);
  }
});

test("a prefix that cuts a token marks the string, the name and the comment as unclosed", () => {
  const text = "SELECT 'abc', \"d\", $$e$$, /* f */ 1";
  for (const [cut, kind] of [[10, TOKEN.STRING], [16, TOKEN.NAME], [22, TOKEN.STRING], [30, TOKEN.COMMENT]]) {
    const tokens = tokensPostgres(text.slice(0, cut));
    const last = tokens[tokens.length - 1];
    assert.equal(last.kind, kind, `cut ${cut}`);
    assert.equal(last.closed, false, `cut ${cut}`);
  }
});

test("a text above 262,144 bytes keeps its tokens and gives a tree capped with too_large", () => {
  const unit = "SELECT 'é', a FROM t;\n";
  const text = unit.repeat(Math.ceil((MAX_AST_INPUT_BYTES + 1) / utf8Length(unit)));
  assert.ok(utf8Length(text) > MAX_AST_INPUT_BYTES);
  const result = parseSql(text, "postgres");
  assert.equal(result.capped.reason, "too_large");
  assert.equal(result.root.end, text.length);
  assertCovers(text, result.tokens, "capped input");
  // A text of exactly the cap in bytes is read in full.
  const edge = "x".repeat(MAX_AST_INPUT_BYTES);
  assert.equal(parseSql(edge, "postgres").capped, null);
  assert.equal(parseSql(`${edge}x`, "postgres").capped.reason, "too_large");
  // The cap counts UTF-8 bytes, not UTF-16 units.
  const twoBytes = "é".repeat(MAX_AST_INPUT_BYTES / 2);
  assert.equal(parseSql(twoBytes, "postgres").capped, null);
  assert.equal(parseSql(`${twoBytes}é`, "postgres").capped.reason, "too_large");
});

test("a nesting above 128 gives too_deep, and a tree above 100,000 nodes gives too_many_nodes", () => {
  const deep = `SELECT ${"(".repeat(MAX_AST_DEPTH + 40)}1${")".repeat(MAX_AST_DEPTH + 40)}`;
  const deepResult = parseSql(deep, "postgres");
  assert.equal(deepResult.capped.reason, "too_deep");
  assertCovers(deep, deepResult.tokens, "deep");
  // Subqueries that nest count too.
  const subs = `${"SELECT * FROM (".repeat(MAX_AST_DEPTH + 40)}SELECT 1${") s".repeat(MAX_AST_DEPTH + 40)}`;
  assert.equal(parseSql(subs, "postgres").capped.reason, "too_deep");
  const atLimit = `SELECT ${"(".repeat(20)}1${")".repeat(20)}`;
  assert.equal(parseSql(atLimit, "postgres").capped, null);

  const wide = `SELECT ${"1,".repeat(120000)}1`;
  assert.ok(utf8Length(wide) <= MAX_AST_INPUT_BYTES);
  const wideResult = parseSql(wide, "postgres");
  assert.equal(wideResult.capped.reason, "too_many_nodes");
  let count = 0;
  walk(wideResult.root, () => {
    count += 1;
    return true;
  });
  assert.ok(count <= MAX_AST_NODES, `the tree has ${count} nodes`);
  assertCovers(wide, wideResult.tokens, "wide");
});

// The text of a statement mix that uses the forms of the dialect, cut to `bytes` ASCII bytes.
function bigText(bytes) {
  const block =
    "-- the load of the nodes\n" +
    "WITH RECURSIVE r(n) AS (\n  SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 10\n)\n" +
    "SELECT $__timeGroupAlias(created_at, $__interval) AS time, j->>'k' AS k, x::int::text, ARRAY[1,2,3], '{1,2}'::int[],\n" +
    "  count(*) FILTER (WHERE a > 1) OVER (PARTITION BY g ORDER BY t), E'it\\'s' || 'a''b' || $f$ body; $f$\n" +
    "FROM metrics.node_load m LEFT JOIN LATERAL (SELECT * FROM u WHERE u.id = m.id LIMIT 1) x ON true\n" +
    "WHERE $__timeFilter(created_at) AND host IN (${host:csv}) AND env = '$env' AND \"q\" <> 1 /* c */\n" +
    "GROUP BY 1, host HAVING count(*) > 1 ORDER BY 1 DESC LIMIT 100;\n";
  return block.repeat(Math.ceil(bytes / block.length)).slice(0, bytes);
}

// The best time of `runs` runs after two warm-up runs, in milliseconds.
function bestOf(runs, fn) {
  fn();
  fn();
  let best = Infinity;
  for (let i = 0; i < runs; i++) {
    const t0 = performance.now();
    fn();
    best = Math.min(best, performance.now() - t0);
  }
  return best;
}

test("262,144 bytes: the tokens in 100 ms or less (target 50 ms), best of 7 runs after 2 warm-ups", (t) => {
  const text = bigText(MAX_AST_INPUT_BYTES);
  assert.equal(utf8Length(text), MAX_AST_INPUT_BYTES);
  // A second text with no string and no comment: one long run of code tokens with many placeholders.
  const code = "SELECT a$b, $x, ${y:csv}, $__m(1), c::int, d->e FROM t WHERE a = 1 AND b <> $z\n".repeat(4000).slice(0, MAX_AST_INPUT_BYTES);
  const mixed = bestOf(7, () => tokensPostgres(text));
  const run = bestOf(7, () => tokensPostgres(code));
  const lex = bestOf(7, () => lexPg(text));
  t.diagnostic(`tokensPostgres mixed text: ${mixed.toFixed(1)} ms (lexPg alone: ${lex.toFixed(1)} ms); one run of code: ${run.toFixed(1)} ms`);
  // The target is 50 ms on the reference machine (10.3c). The unit test has a limit of 100 ms for the minimum of 7 runs (16.10a), so a
  // loaded machine does not fail it; the browser case B176 measures the target.
  assert.ok(mixed <= 100, `the tokens of the mixed text took ${mixed.toFixed(1)} ms`);
  // The dense text has 170,000 tokens and a placeholder in each 12: the bound is wider, and lexPg alone is the floor.
  assert.ok(run <= 100, `the tokens of the code run took ${run.toFixed(1)} ms`);
});

test("262,144 bytes: the tokens and the tree in 100 ms or less (target 50 ms), best of 7 runs after 2 warm-ups", (t) => {
  const text = bigText(MAX_AST_INPUT_BYTES);
  const tokens = tokensPostgres(text);
  const tree = bestOf(7, () => parseSql(text, "postgres", tokens));
  const both = bestOf(7, () => parseSql(text, "postgres"));
  const result = parseSql(text, "postgres", tokens);
  assert.equal(result.capped, null, "the load text is under the caps");
  t.diagnostic(`parseSql with the tokens given: ${tree.toFixed(1)} ms; tokens and tree together: ${both.toFixed(1)} ms; ${tokens.length} tokens`);
  assert.ok(tree <= 100, `the tree took ${tree.toFixed(1)} ms`);
  assert.ok(both <= 100, `the tokens and the tree took ${both.toFixed(1)} ms`);
});

// ---------------------------------------------------------------- the rules that the tree lacks

for (const g of GAP_VECTORS.vectors) {
  test(`rule of sql_tree.js: ${g.name}`, () => {
    const result = parseSql(g.text, "postgres");
    assertTree(g.text, result, g.name);
    const got = summary(g.text, result);
    for (const [key, value] of Object.entries(g.expect)) assert.deepEqual(got[key], value, `${g.name}: ${key}`);
  });
}

test("a parameter is an opaque literal leaf of the kind param, in both dialects", () => {
  const find = (text, dialect) => {
    const out = [];
    walk(parseSql(text, dialect).root, (n) => {
      if (n.type === "literal" && n.kind === "param") out.push([text.slice(n.start, n.end), n.children.length]);
      if (n.type === "variable") out.push(["variable", text.slice(n.start, n.end)]);
    });
    return out;
  };
  assert.deepEqual(find("SELECT $1, $23 FROM t WHERE a = $1", "postgres"), [["$1", 0], ["$23", 0], ["$1", 0]]);
  // In ClickHouse, `$1` stays a variable, as sqlscan.js reads it. The query parameter `{p:Type}` is the param literal.
  assert.deepEqual(find("SELECT $1, {p:UInt8}", "clickhouse"), [["variable", "$1"], ["{p:UInt8}", 0]]);
});
