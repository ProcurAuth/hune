// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the token stream of ClickHouse and of the SQL tree (F42): `tokensClickHouse` and `parseSql` for both dialects.
// The tree is never a security check: these tests check structure only. The PostgreSQL tokens come from a small adapter in
// this file, until the module `tokens_pg.js` of the PostgreSQL worker exists.
import test from "node:test";
import assert from "node:assert/strict";
import { lexPg, KIND } from "../../web/js/sql_lex_pg.js";
import { findMacros, findVariableNames } from "../../web/js/sqlscan.js";
import { AST_REASONS, MAX_AST_DEPTH, MAX_AST_INPUT_BYTES, MAX_AST_NODES, SQL_NODE, TOKEN, utf8Length, walk } from "../../web/js/editor/ast/nodes.js";
import { tokensClickHouse } from "../../web/js/editor/ast/tokens_ch.js";
import { parseSql, setTokenizer } from "../../web/js/editor/ast/sql_tree.js";

// ---------------------------------------------------------------------------------------------------------------------
// The PostgreSQL adapter of this test: the kinds of `lexPg` mapped to the common kinds. A word with a "$" is a variable or
// a macro. Two adjacent ":" are one operator. It stands in for `tokens_pg.js`.
const PG_KINDS = {
  [KIND.BLANK]: TOKEN.BLANK, [KIND.LINE]: TOKEN.COMMENT, [KIND.BLOCK]: TOKEN.COMMENT, [KIND.WORD]: TOKEN.WORD,
  [KIND.NAME]: TOKEN.NAME, [KIND.STRING]: TOKEN.STRING, [KIND.ESTRING]: TOKEN.STRING, [KIND.PREFIXED]: TOKEN.STRING,
  [KIND.HEREDOC]: TOKEN.STRING, [KIND.NUMBER]: TOKEN.NUMBER, [KIND.PUNCT]: TOKEN.PUNCT, [KIND.OPERATOR]: TOKEN.OPERATOR,
  [KIND.PARAM]: TOKEN.PARAM,
};

function pgTokens(text) {
  const out = [];
  for (const t of lexPg(text)) {
    let kind = PG_KINDS[t.kind];
    const src = text.slice(t.start, t.end);
    if (kind === TOKEN.WORD && src.startsWith("$")) kind = src.startsWith("$__") ? TOKEN.MACRO : TOKEN.VARIABLE;
    const last = out[out.length - 1];
    if (kind === TOKEN.PUNCT && src === ":" && last && last.kind === TOKEN.PUNCT && text[last.start] === ":" && last.end === t.start) {
      last.kind = TOKEN.OPERATOR;
      last.end = t.end;
      continue;
    }
    out.push({ kind, start: t.start, end: t.end, closed: t.closed });
  }
  return out;
}

// Parses a text with the tokens of the dialect.
function parse(text, dialect = "clickhouse") {
  return parseSql(text, dialect, dialect === "postgres" ? pgTokens(text) : tokensClickHouse(text));
}

// A short outline of a tree. A node with children is `type:name(children)`. A leaf is `type<source>`. A bang adds the error,
// a plus marks a foldable node, and an "L" marks a lambda.
function outline(text, dialect = "clickhouse") {
  const show = (n) => {
    const src = text.slice(n.start, n.end);
    const head = n.type + (n.name !== undefined ? `:${n.name}` : "") + (n.error ? `!${n.error}` : "") + (n.foldable ? "+" : "") + (n.lambda ? "L" : "");
    if (!n.children.length) return `${head}<${src.length > 24 ? `${src.slice(0, 24)}…` : src}>`;
    return `${head}(${n.children.map(show).join(" ")})`;
  };
  return show(parse(text, dialect).root);
}

// The nodes of a type, each as its source text, in document order.
function sources(text, type, dialect = "clickhouse") {
  const out = [];
  walk(parse(text, dialect).root, (n) => {
    if (n.type === type) out.push(text.slice(n.start, n.end));
  });
  return out;
}

// The number of nodes of a tree.
function countNodes(root) {
  let n = 0;
  walk(root, () => {
    n += 1;
  });
  return n;
}

// The checks of a result that hold for each text and each dialect. It returns the list of faults. The list is empty when all is well.
// The tokens cover the text. The root spans the text. Each child is inside its parent and after the child before it.
// Each code bracket belongs to one node with `open` or `close`, or to one `unmatched` error node.
function faults(text, dialect = "clickhouse") {
  const out = [];
  const r = parse(text, dialect);
  let at = 0;
  for (const t of r.tokens) {
    if (t.start !== at || t.end <= t.start) out.push(`token gap at ${at}`);
    at = t.end;
  }
  if (at !== text.length) out.push("tokens do not reach the end");
  if (r.root.type !== SQL_NODE.SCRIPT || r.root.start !== 0 || r.root.end !== text.length) out.push("root");
  const owners = new Map();
  const own = (offset) => owners.set(offset, (owners.get(offset) ?? 0) + 1);
  walk(r.root, (n) => {
    let prev = n.start;
    if (n.start > n.end) out.push(`negative ${n.type}`);
    for (const c of n.children) {
      if (c.start < prev || c.end > n.end) out.push(`${c.type}[${c.start},${c.end}) in ${n.type}[${n.start},${n.end}) after ${prev}`);
      prev = c.end;
    }
    if (n.open !== undefined) own(n.open);
    if (n.close !== undefined) own(n.close);
    if (n.type === SQL_NODE.ERROR && n.error === "unmatched") own(n.start);
  });
  if (!r.capped) {
    for (const t of r.tokens) {
      if (t.kind === TOKEN.PUNCT && t.end - t.start === 1 && "()[]{}".includes(text[t.start]) && owners.get(t.start) !== 1) out.push(`bracket at ${t.start}`);
    }
    for (const [offset, count] of owners) if (count !== 1) out.push(`offset ${offset} owned ${count} times`);
  }
  return out;
}

// A token list as `kind:text` items, without the blanks.
function show(text) {
  return tokensClickHouse(text)
    .filter((t) => t.kind !== TOKEN.BLANK)
    .map((t) => `${t.kind}:${text.slice(t.start, t.end)}`)
    .join(" | ");
}

// ---------------------------------------------------------------------------------------------------------------------
// The tokens of ClickHouse.

const TOKEN_VECTORS = [
  ["SELECT a, b FROM t", "word:SELECT | word:a | punct:, | word:b | word:FROM | word:t"],
  ["x->y a::b c||d", "word:x | operator:-> | word:y | word:a | operator::: | word:b | word:c | operator:|| | word:d"],
  ["a<=b a>=b a!=b a<>b a==b a<b a>b a=b", "word:a | operator:<= | word:b | word:a | operator:>= | word:b | word:a | operator:!= | word:b | word:a | operator:<> | word:b | word:a | operator:== | word:b | word:a | operator:< | word:b | word:a | operator:> | word:b | word:a | operator:= | word:b"],
  ["1 1.5 .5 1e-5 1E+5 0x1F 1_000 1abc", "number:1 | number:1.5 | number:.5 | number:1e-5 | number:1E+5 | number:0x1F | number:1_000 | number:1abc"],
  ["t.col t.1 a.b.c", "word:t | punct:. | word:col | word:t | number:.1 | word:a | punct:. | word:b | punct:. | word:c"],
  ["$__timeFilter(ts) $__fromTime $__interval_ms", "macro:$__timeFilter | punct:( | word:ts | punct:) | macro:$__fromTime | macro:$__interval_ms"],
  ["$v $name_1 ${v} ${v:csv} $1", "variable:$v | variable:$name_1 | variable:${v} | variable:${v:csv} | variable:$1"],
  ["${a-b} $ $$", "other:$ | punct:{ | word:a | operator:- | word:b | punct:} | other:$ | other:$ | other:$"],
  ["foo$bar", "word:foo | variable:$bar"],
  ["{p:UInt8} {p:Array(String)} {p:DateTime64(3, UTC)} {a} {a:} {1:x}", "param:{p:UInt8} | param:{p:Array(String)} | param:{p:DateTime64(3, UTC)} | punct:{ | word:a | punct:} | punct:{ | word:a | punct:: | punct:} | punct:{ | number:1 | punct:: | word:x | punct:}"],
  ["a -- line\nb", "word:a | comment:-- line | word:b"],
  ["a --x\r\nb", "word:a | comment:--x | word:b"],
  ["a # hash\nb", "word:a | comment:# hash | word:b"],
  ["/* block */ x /* open", "comment:/* block */ | word:x | comment:/* open"],
  ["'it''s' 'a\\'b' \"q\" `n` 'open", "string:'it''s' | string:'a\\'b' | name:\"q\" | name:`n` | string:'open"],
  ["é ü x_1 _y", "word:é | word:ü | word:x_1 | word:_y"],
  ["( ) [ ] { } , ; . :", "punct:( | punct:) | punct:[ | punct:] | punct:{ | punct:} | punct:, | punct:; | punct:. | punct::"],
  ["a ? b : c", "word:a | operator:? | word:b | punct:: | word:c"],
  ["x = 'a${v}b'", "word:x | operator:= | string:'a${v}b'"],
  ["SELECT -1 - -2 * 3 / 4 % 5", "word:SELECT | operator:- | number:1 | operator:- | operator:- | number:2 | operator:* | number:3 | operator:/ | number:4 | operator:% | number:5"],
  ["a @ b \\ c", "word:a | other:@ | word:b | other:\\ | word:c"],
];

test("tokensClickHouse: the vectors of the token kinds", () => {
  for (const [text, expected] of TOKEN_VECTORS) assert.equal(show(text), expected, JSON.stringify(text));
});

test("tokensClickHouse: the tokens cover the text with no gap and no overlap", () => {
  for (const [text] of TOKEN_VECTORS) {
    let at = 0;
    for (const t of tokensClickHouse(text)) {
      assert.equal(t.start, at, JSON.stringify(text));
      assert.ok(t.end > t.start);
      assert.ok(Object.values(TOKEN).includes(t.kind));
      at = t.end;
    }
    assert.equal(at, text.length);
  }
  assert.deepEqual(tokensClickHouse(""), []);
  assert.deepEqual(tokensClickHouse(null), []);
});

test("tokensClickHouse: a token that the text ends before it closes has closed false", () => {
  const closed = (text) => tokensClickHouse(text).filter((t) => t.kind !== TOKEN.BLANK).map((t) => t.closed);
  assert.deepEqual(closed("'a' 'b"), [true, false]);
  assert.deepEqual(closed("`a` `b"), [true, false]);
  assert.deepEqual(closed("\"a\" \"b"), [true, false]);
  assert.deepEqual(closed("/* a */ /* b"), [true, false]);
  assert.deepEqual(closed("-- a"), [true]);
  assert.deepEqual(closed("x 1 $v"), [true, true, true]);
});

test("tokensClickHouse: a line comment does not hold its line end, a blank token does", () => {
  const text = "a -- c\r\n  b";
  const tokens = tokensClickHouse(text);
  const comment = tokens.find((t) => t.kind === TOKEN.COMMENT);
  assert.equal(text.slice(comment.start, comment.end), "-- c");
  const next = tokens[tokens.indexOf(comment) + 1];
  assert.equal(next.kind, TOKEN.BLANK);
  assert.equal(text.slice(next.start, next.end), "\r\n  ");
  // A lone CR stays in the comment: the page lexer ends a line comment at LF only.
  const lone = "-- a\rb\nc";
  assert.equal(lone.slice(0, tokensClickHouse(lone)[0].end), "-- a\rb");
});

test("tokensClickHouse: the spans of the highlighter and the tokens agree on comments, strings and names", () => {
  const text = "SELECT 'a--b', \"c/*d\", `e#f` -- g 'h'\n/* i ' j */ # k\nFROM t";
  const kinds = tokensClickHouse(text).filter((t) => t.kind !== TOKEN.BLANK).map((t) => t.kind);
  assert.deepEqual(kinds, ["word", "string", "punct", "name", "punct", "name", "comment", "comment", "comment", "word", "word"]);
});

test("tokensClickHouse: the placeholders are never split, and the names agree with sqlscan.js", () => {
  const samples = ["SELECT $a, ${b}, ${c:csv}, $__timeFilter(ts), $__from + $__to", "x$y $1 $a_b ${__interval_ms} $__timeInterval_ms(t)", "WHERE a = $v AND b IN (${w:singlequote})"];
  for (const text of samples) {
    const names = tokensClickHouse(text)
      .filter((t) => t.kind === TOKEN.VARIABLE || t.kind === TOKEN.MACRO)
      .map((t) => {
        const src = text.slice(t.start, t.end);
        return src.startsWith("${") ? src.slice(2, -1).split(":")[0] : src.slice(1);
      });
    const expected = [...findVariableNames(text), ...findMacros(text)];
    assert.deepEqual([...new Set(names)].sort(), [...new Set(expected)].sort(), text);
  }
  // A placeholder in a string is not a token: the string keeps it.
  assert.equal(show("'$a ${b}'"), "string:'$a ${b}'");
  // A format that 7.9 does not list is still one token: the tree never finds one name less than the scan.
  assert.equal(show("${a:nofmt}"), "variable:${a:nofmt}");
});

// ---------------------------------------------------------------------------------------------------------------------
// The tree: vectors of each construct.

const CLICKHOUSE_VECTORS = [
  ["SELECT 1", "script(statement(query(clause:SELECT(expr(literal<1>)))))"],
  ["SELECT a, b FROM t WHERE a > 1", "script(statement(query(clause:SELECT(list(expr<a> expr<b>)) clause:FROM(expr<t>) clause:WHERE(expr(literal<1>)))))"],
  ["SELECT a FROM t GROUP BY a HAVING count() > 1 ORDER BY a DESC LIMIT 10 OFFSET 5", "script(statement(query(clause:SELECT(expr<a>) clause:FROM(expr<t>) clause:GROUP BY(expr<a>) clause:HAVING(expr(call:count<()> literal<1>)) clause:ORDER BY(expr<a DESC>) clause:LIMIT(expr(literal<10>)) clause:OFFSET(expr(literal<5>)))))"],
  ["SELECT a FROM t PREWHERE x = 1 WHERE y = 2", "script(statement(query(clause:SELECT(expr<a>) clause:FROM(expr<t>) clause:PREWHERE(expr(literal<1>)) clause:WHERE(expr(literal<2>)))))"],
  ["SELECT x FROM t LIMIT 2 BY k, j LIMIT 10", "script(statement(query(clause:SELECT(expr<x>) clause:FROM(expr<t>) clause:LIMIT BY(expr(literal<2>) list(expr<k> expr<j>)) clause:LIMIT(expr(literal<10>)))))"],
  ["SELECT a FROM t ARRAY JOIN arr AS x, arr2 AS y", "script(statement(query(clause:SELECT(expr<a>) clause:FROM(expr<t>) clause:ARRAY JOIN(list(expr<arr AS x> expr<arr2 AS y>)))))"],
  ["SELECT 1 FROM a LEFT JOIN b ON a.k = b.k INNER JOIN c USING (k) CROSS JOIN d", "script(statement(query(clause:SELECT(expr(literal<1>)) clause:FROM(expr<a>) clause:LEFT JOIN(expr<b ON a.k = b.k>) clause:INNER JOIN(expr(paren(expr<k>))) clause:CROSS JOIN(expr<d>))))"],
  ["SELECT 1 FROM a GLOBAL ANY LEFT JOIN b ON 1 LEFT ARRAY JOIN z", "script(statement(query(clause:SELECT(expr(literal<1>)) clause:FROM(expr<a>) clause:GLOBAL ANY LEFT JOIN(expr(literal<1>)) clause:LEFT ARRAY JOIN(expr<z>))))"],
  ["SELECT 1 UNION ALL SELECT 2 UNION DISTINCT SELECT 3 EXCEPT SELECT 4 INTERSECT SELECT 5", "script(statement(query(clause:SELECT(expr(literal<1>)) clause:UNION ALL<UNION ALL> clause:SELECT(expr(literal<2>)) clause:UNION DISTINCT<UNION DISTINCT> clause:SELECT(expr(literal<3>)) clause:EXCEPT<EXCEPT> clause:SELECT(expr(literal<4>)) clause:INTERSECT<INTERSECT> clause:SELECT(expr(literal<5>)))))"],
  ["SELECT 1 SETTINGS a = 1, b = 2 FORMAT JSON", "script(statement(query(clause:SELECT(expr(literal<1>)) clause:SETTINGS(list(expr(literal<1>) expr(literal<2>))) clause:FORMAT(expr<JSON>))))"],
  ["WITH a AS (SELECT 1), b AS (SELECT 2) SELECT * FROM a, b", "script(statement(with:WITH(cte:a(subquery(query(clause:SELECT(expr(literal<1>))))) cte:b(subquery(query(clause:SELECT(expr(literal<2>)))))) query(clause:SELECT(expr<*>) clause:FROM(list(expr<a> expr<b>)))))"],
  ["WITH 5 AS n, (SELECT 1) AS m SELECT n + m", "script(statement(with:WITH(cte:n(expr(literal<5>)) cte:m(expr(subquery(query(clause:SELECT(expr(literal<1>))))))) query(clause:SELECT(expr<n + m>))))"],
  ["WITH (SELECT 1) AS x SELECT x", "script(statement(with:WITH(cte:x(expr(subquery(query(clause:SELECT(expr(literal<1>))))))) query(clause:SELECT(expr<x>))))"],
  ["(WITH a AS (SELECT 1) SELECT * FROM a)", "script(statement(query(clause:(expr(subquery(with:WITH(cte:a(subquery(query(clause:SELECT(expr(literal<1>)))))) query(clause:SELECT(expr<*>) clause:FROM(expr<a>))))))))"],
  ["SELECT 1; SELECT 2;\nSELECT 3", "script(statement(query(clause:SELECT(expr(literal<1>)))) statement(query(clause:SELECT(expr(literal<2>)))) statement(query(clause:SELECT(expr(literal<3>)))))"],
  ["SELECT *\nFROM (\n  SELECT 1\n)", "script(statement(query(clause:SELECT(expr<*>) clause:FROM(expr(subquery+(query(clause:SELECT(expr(literal<1>)))))))))"],
  ["WITH a AS (\n  SELECT 1\n)\nSELECT 1", "script(statement(with:WITH(cte:a+(subquery+(query(clause:SELECT(expr(literal<1>)))))) query(clause:SELECT(expr(literal<1>)))))"],
  ["SELECT arrayMap(x -> x + 1, a), arrayFilter((x, y) -> x > y, a, b)", "script(statement(query(clause:SELECT(list(expr(call:arrayMap(list(exprL(literal<1>) expr<a>))) expr(call:arrayFilter(list(exprL(paren(list(expr<x> expr<y>))) expr<a> expr<b>))))))))"],
  ["SELECT x -> y", "script(statement(query(clause:SELECT(exprL<x -> y>))))"],
  ["SELECT a, (b -> c), d", "script(statement(query(clause:SELECT(list(expr<a> expr(paren(exprL<b -> c>)) expr<d>)))))"],
  ["SELECT [1, 2][1], [], f()", "script(statement(query(clause:SELECT(list(expr(array(list(expr(literal<1>) expr(literal<2>))) array(expr(literal<1>))) expr(array<[]>) expr(call:f<()>))))))"],
  ["SELECT $__timeFilter(ts), $__timeInterval(toDateTime(ts)), $__fromTime, ${__from}", "script(statement(query(clause:SELECT(list(expr(macro:$__timeFilter(expr<ts>)) expr(macro:$__timeInterval(expr(call:toDateTime(expr<ts>)))) expr(macro:$__fromTime<$__fromTime>) expr(macro:${__from}<${__from}>))))))"],
  ["SELECT $v, ${v}, ${v:csv}, '$v ${v}', {p:UInt8}", "script(statement(query(clause:SELECT(list(expr(variable:v<$v>) expr(variable:v<${v}>) expr(variable:v<${v:csv}>) expr(literal<'$v ${v}'>) expr(literal<{p:UInt8}>))))))"],
  ["SELECT a -- one\n, /* two */ b", "script(statement(query(clause:SELECT(list(expr<a> comment<-- one> comment</* two */> expr<b>)))))"],
  ["SELECT (1, 2", "script(statement(query(clause:SELECT(expr(paren!unclosed(list(expr(literal<1>) expr(literal<2>))))))))"],
  ["SELECT 'open", "script(statement(query(clause:SELECT(expr(literal<'open>)))))"],
  ["SELECT 1 /* open", "script(statement(query(clause:SELECT(expr(literal<1>)))) comment</* open>)"],
  ["SELECT 1) + ]", "script(statement(query(clause:SELECT(expr(literal<1> error!unmatched<)> error!unmatched<]>)))))"],
  ["SELECT (a [b) c]", "script(statement(query(clause:SELECT(expr(paren(expr(array!unclosed(expr<b>))) error!unmatched<]>)))))"],
  ["SELECT * EXCEPT (a, b) FROM t", "script(statement(query(clause:SELECT(expr(paren(list(expr<a> expr<b>)))) clause:FROM(expr<t>))))"],
  ["SELECT format(x), left(y, 2) AS format FROM t", "script(statement(query(clause:SELECT(list(expr(call:format(expr<x>)) expr(call:left(list(expr<y> expr(literal<2>)))))) clause:FROM(expr<t>))))"],
  ["SELECT a.limit, t.from FROM t", "script(statement(query(clause:SELECT(list(expr<a.limit> expr<t.from>)) clause:FROM(expr<t>))))"],
  ["INSERT INTO t VALUES (1, 2), (3, 4)", "script(statement(query(clause:INSERT(expr<INTO t>) clause:VALUES(list(expr(paren(list(expr(literal<1>) expr(literal<2>)))) expr(paren(list(expr(literal<3>) expr(literal<4>)))))))))"],
  ["EXPLAIN SELECT 1", "script(statement(query(clause:EXPLAIN<EXPLAIN> clause:SELECT(expr(literal<1>)))))"],
  ["   \n  ", "script<   \n  >"],
  ["-- only a comment\n/* and another */", "script(comment<-- only a comment> comment</* and another */>)"],
  ["SELECT CASE WHEN a THEN 1 ELSE 2 END", "script(statement(query(clause:SELECT(expr(literal<1> literal<2>)))))"],
  ["SELECT {'a': 1}, (1, (2, 3))", "script(statement(query(clause:SELECT(list(expr(paren(expr(literal<'a'> literal<1>))) expr(paren(list(expr(literal<1>) expr(paren(list(expr(literal<2>) expr(literal<3>))))))))))))"],
  ["SELECT quantile(0.5)(x), count(*)", "script(statement(query(clause:SELECT(list(expr(call:quantile(expr(literal<0.5>)) paren(expr<x>)) expr(call:count(expr<*>)))))))"],
  ["SELECT a FROM t ORDER BY a WITH FILL FROM 1 TO 10 STEP 1", "script(statement(query(clause:SELECT(expr<a>) clause:FROM(expr<t>) clause:ORDER BY(expr(literal<1> literal<10> literal<1>)))))"],
  ["SELECT x FROM t WHERE x IN (SELECT y FROM u WHERE y > (SELECT max(z) FROM w))", "script(statement(query(clause:SELECT(expr<x>) clause:FROM(expr<t>) clause:WHERE(expr(subquery(query(clause:SELECT(expr<y>) clause:FROM(expr<u>) clause:WHERE(expr(subquery(query(clause:SELECT(expr(call:max(expr<z>))) clause:FROM(expr<w>))))))))))))"],
  ["(SELECT 1) UNION ALL (SELECT 2)", "script(statement(query(clause:(expr(subquery(query(clause:SELECT(expr(literal<1>)))))) clause:UNION ALL(expr(subquery(query(clause:SELECT(expr(literal<2>)))))))))"],
  ["SELECT 1 +", "script(statement(query(clause:SELECT(expr(literal<1>)))))"],
  ["SELECT\nFROM t", "script(statement(query(clause:SELECT<SELECT> clause:FROM(expr<t>))))"],
  ["; ;", "script(statement<;> statement<;>)"],
  ["select a from t where x = 1 order by a limit 2", "script(statement(query(clause:SELECT(expr<a>) clause:FROM(expr<t>) clause:WHERE(expr(literal<1>)) clause:ORDER BY(expr<a>) clause:LIMIT(expr(literal<2>)))))"],
];

const POSTGRES_VECTORS = [
  ["SELECT a::int, b->>'k', c || 'x' FROM t", "script(statement(query(clause:SELECT(list(expr<a::int> expr(literal<'k'>) expr(literal<'x'>))) clause:FROM(expr<t>))))"],
  ["WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r) SELECT n FROM r", "script(statement(with:WITH RECURSIVE(cte:r(call:r(expr<n>) subquery(query(clause:SELECT(expr(literal<1>)) clause:UNION ALL<UNION ALL> clause:SELECT(expr(literal<1>)) clause:FROM(expr<r>))))) query(clause:SELECT(expr<n>) clause:FROM(expr<r>))))"],
  ["SELECT $$a;b$$ AS s, $t$ x $t$", "script(statement(query(clause:SELECT(list(expr(literal<$$a;b$$>) expr(literal<$t$ x $t$>))))))"],
  ["SELECT * FROM t ORDER BY a FETCH FIRST 5 ROWS ONLY FOR UPDATE", "script(statement(query(clause:SELECT(expr<*>) clause:FROM(expr<t>) clause:ORDER BY(expr<a>) clause:FETCH(expr(literal<5>)) clause:FOR(expr<UPDATE>))))"],
  ["SELECT * FROM a NATURAL JOIN b FULL OUTER JOIN c ON true", "script(statement(query(clause:SELECT(expr<*>) clause:FROM(expr<a>) clause:NATURAL JOIN(expr<b>) clause:FULL OUTER JOIN(expr<c ON true>))))"],
  ["SELECT format('%s', x), x -> y FROM t", "script(statement(query(clause:SELECT(list(expr(call:format(list(expr(literal<'%s'>) expr<x>))) expr<x -> y>)) clause:FROM(expr<t>))))"],
  ["SELECT prewhere, settings FROM t LIMIT ALL OFFSET 3", "script(statement(query(clause:SELECT(list(expr<prewhere> expr<settings>)) clause:FROM(expr<t>) clause:LIMIT(expr<ALL>) clause:OFFSET(expr(literal<3>)))))"],
  ["SELECT 1 UNION SELECT 2 UNION ALL SELECT 3", "script(statement(query(clause:SELECT(expr(literal<1>)) clause:UNION<UNION> clause:SELECT(expr(literal<2>)) clause:UNION ALL<UNION ALL> clause:SELECT(expr(literal<3>)))))"],
  ["SELECT * FROM (VALUES (1), (2)) AS v(a)", "script(statement(query(clause:SELECT(expr<*>) clause:FROM(expr(subquery(query(clause:VALUES(list(expr(paren(expr(literal<1>))) expr(paren(expr(literal<2>))))))) call:v(expr<a>))))))"],
  ["WITH x AS MATERIALIZED (SELECT 1), y(a, b) AS (SELECT 1, 2) SELECT * FROM x", "script(statement(with:WITH(cte:x(subquery(query(clause:SELECT(expr(literal<1>))))) cte:y(call:y(list(expr<a> expr<b>)) subquery(query(clause:SELECT(list(expr(literal<1>) expr(literal<2>))))))) query(clause:SELECT(expr<*>) clause:FROM(expr<x>))))"],
  ["SELECT ARRAY[1, 2], a[1], '{}'::jsonb", "script(statement(query(clause:SELECT(list(expr(array(list(expr(literal<1>) expr(literal<2>)))) expr(array(expr(literal<1>))) expr(literal<'{}'>))))))"],
  ["SELECT $1, $__timeFilter(ts)", "script(statement(query(clause:SELECT(list(expr(literal<$1>) expr(macro:$__timeFilter(expr<ts>)))))))"],
  ["SELECT 'a'\n'b' AS s", "script(statement(query(clause:SELECT(expr(literal<'a'> literal<'b'>)))))"],
];

test("parseSql clickhouse: the vectors of each construct", () => {
  for (const [text, expected] of CLICKHOUSE_VECTORS) assert.equal(outline(text), expected, JSON.stringify(text));
});

test("parseSql postgres: the vectors of each construct", () => {
  for (const [text, expected] of POSTGRES_VECTORS) assert.equal(outline(text, "postgres"), expected, JSON.stringify(text));
});

test("the vectors give a sound tree: tokens, positions and bracket pairs", () => {
  for (const [text] of CLICKHOUSE_VECTORS) assert.deepEqual(faults(text), [], JSON.stringify(text));
  for (const [text] of POSTGRES_VECTORS) assert.deepEqual(faults(text, "postgres"), [], JSON.stringify(text));
});

test("the ranges of statements, clauses, lists and expressions", () => {
  const text = "SELECT a, b + 1 FROM t WHERE x = 1; SELECT 2";
  assert.deepEqual(sources(text, "statement"), ["SELECT a, b + 1 FROM t WHERE x = 1;", "SELECT 2"]);
  assert.deepEqual(sources(text, "clause"), ["SELECT a, b + 1", "FROM t", "WHERE x = 1", "SELECT 2"]);
  assert.deepEqual(sources(text, "list"), ["a, b + 1"]);
  assert.deepEqual(sources(text, "expr"), ["a", "b + 1", "t", "x = 1", "2"]);
  // A statement holds its semicolon, and `semicolon` is the offset of it.
  const r = parse(text);
  assert.equal(r.root.children[0].semicolon, text.indexOf(";"));
  assert.equal(r.root.children[1].semicolon, undefined);
  assert.equal(r.root.start, 0);
  assert.equal(r.root.end, text.length);
});

test("a query that starts with a clause keyword in lower case or mixed case reads the same", () => {
  assert.equal(outline("Select a From t Where x = 1"), outline("SELECT a FROM t WHERE x = 1"));
  assert.equal(outline("select a, b from t group by a order by b limit 1"), outline("SELECT a, b FROM t GROUP BY a ORDER BY b LIMIT 1"));
});

test("the name of a clause is the upper-case keywords with one space between", () => {
  const names = (text, d = "clickhouse") => {
    const out = [];
    walk(parse(text, d).root, (n) => {
      if (n.type === SQL_NODE.CLAUSE) out.push(n.name);
    });
    return out;
  };
  assert.deepEqual(names("select 1 from a group   by x\n having 1 order /* c */ by y limit 1 offset 2"), ["SELECT", "FROM", "GROUP BY", "HAVING", "ORDER BY", "LIMIT", "OFFSET"]);
  assert.deepEqual(names("SELECT 1 FROM a ANY LEFT OUTER JOIN b ON 1 RIGHT JOIN c ON 1 FULL JOIN d ON 1 ASOF JOIN e ON 1 PASTE JOIN f"), [
    "SELECT", "FROM", "ANY LEFT OUTER JOIN", "RIGHT JOIN", "FULL JOIN", "ASOF JOIN", "PASTE JOIN",
  ]);
  assert.deepEqual(names("SELECT 1 FROM a JOIN b USING (x) ARRAY JOIN y LEFT ARRAY JOIN z"), ["SELECT", "FROM", "JOIN", "ARRAY JOIN", "LEFT ARRAY JOIN"]);
  assert.deepEqual(names("SELECT 1 UNION ALL SELECT 2 UNION DISTINCT SELECT 3"), ["SELECT", "UNION ALL", "SELECT", "UNION DISTINCT", "SELECT"]);
  assert.deepEqual(names("SELECT 1 FROM t WHERE 1 SETTINGS a = 1 FORMAT CSV"), ["SELECT", "FROM", "WHERE", "SETTINGS", "FORMAT"]);
  assert.deepEqual(names("SELECT 1 FROM t LIMIT 1 BY a"), ["SELECT", "FROM", "LIMIT BY"]);
  assert.deepEqual(names("SELECT 1 FROM t LIMIT 1, 2 BY a LIMIT 5"), ["SELECT", "FROM", "LIMIT BY", "LIMIT"]);
  assert.deepEqual(names("SELECT a FROM t PREWHERE p WHERE w"), ["SELECT", "FROM", "PREWHERE", "WHERE"]);
  assert.deepEqual(names("SELECT * FROM t ORDER BY a FETCH FIRST 1 ROWS ONLY FOR UPDATE", "postgres"), ["SELECT", "FROM", "ORDER BY", "FETCH", "FOR"]);
  assert.deepEqual(names("SELECT 1 FROM a NATURAL LEFT JOIN b", "postgres"), ["SELECT", "FROM", "NATURAL LEFT JOIN"]);
});

test("the words of a clause keyword that are columns, aliases or functions do not start a clause", () => {
  const names = (text, d = "clickhouse") => {
    const out = [];
    walk(parse(text, d).root, (n) => {
      if (n.type === SQL_NODE.CLAUSE) out.push(n.name);
    });
    return out;
  };
  assert.deepEqual(names("SELECT format(x), settings, offset AS limit FROM t"), ["SELECT", "FROM"]);
  assert.deepEqual(names("SELECT a.format, t.window FROM t"), ["SELECT", "FROM"]);
  assert.deepEqual(names("SELECT left(a, 1), right(a, 1), any(x) FROM t"), ["SELECT", "FROM"]);
  assert.deepEqual(names("SELECT * EXCEPT (a) FROM t"), ["SELECT", "FROM"]);
  assert.deepEqual(names("SELECT x FROM t EXCEPT (SELECT y FROM u)"), ["SELECT", "FROM", "EXCEPT", "SELECT", "FROM"]);
  assert.deepEqual(names("SELECT x FROM t ORDER BY x WITH FILL FROM 1 TO 5"), ["SELECT", "FROM", "ORDER BY"]);
  assert.deepEqual(names("SELECT a IS DISTINCT FROM b", "postgres"), ["SELECT"]);
  // A clause keyword inside a bracket is part of the expression.
  assert.deepEqual(names("SELECT extract(YEAR FROM d), f(x ORDER BY y) FROM t"), ["SELECT", "FROM"]);
  // In PostgreSQL, the ClickHouse words are plain words.
  assert.deepEqual(names("SELECT 1 FROM t PREWHERE x FORMAT y", "postgres"), ["SELECT", "FROM"]);
});

test("a subquery is a bracket whose first word is SELECT or WITH, with or without a call name", () => {
  const types = (text, d = "clickhouse") => {
    const out = [];
    walk(parse(text, d).root, (n) => {
      if (n.open !== undefined) out.push(`${n.type}${n.name ? `:${n.name}` : ""}`);
    });
    return out;
  };
  assert.deepEqual(types("SELECT (SELECT 1), (WITH a AS (SELECT 1) SELECT 2), (1), f(2), f(SELECT 3)"), [
    "subquery", "subquery", "subquery", "paren", "call:f", "subquery:f",
  ]);
  assert.deepEqual(types("SELECT x IN (SELECT 1), y IN (1, 2), EXISTS (SELECT 1) FROM t JOIN u USING (k)"), ["subquery", "paren", "subquery", "paren"]);
  assert.deepEqual(types("SELECT * FROM (VALUES (1)) AS v", "postgres"), ["subquery", "paren"]);
  assert.deepEqual(types("SELECT * FROM (VALUES (1)) AS v"), ["paren", "paren"]);
});

test("the brackets: open, close, tuple, subscript, unclosed and unmatched", () => {
  const text = "SELECT f(a, b), (1, 2), [x][1], {1}, g(";
  const nodes = [];
  walk(parse(text).root, (n) => {
    if (n.open !== undefined) nodes.push(n);
  });
  const call = nodes[0];
  assert.equal(call.type, "call");
  assert.equal(call.name, "f");
  assert.equal(text[call.open], "(");
  assert.equal(text[call.close], ")");
  assert.equal(call.end, call.close + 1);
  assert.equal(call.start, call.open);
  const tuple = nodes[1];
  assert.equal(tuple.type, "paren");
  assert.equal(tuple.tuple, true);
  const array = nodes[2];
  assert.equal(array.type, "array");
  assert.equal(array.subscript, undefined);
  const subscript = nodes[3];
  assert.equal(subscript.type, "array");
  assert.equal(subscript.subscript, true);
  const brace = nodes[4];
  assert.equal(brace.type, "paren");
  assert.equal(brace.bracket, "{");
  const open = nodes[5];
  assert.equal(open.error, "unclosed");
  assert.equal(open.close, undefined);
  assert.equal(open.end, text.length);
  const errors = [];
  walk(parse("SELECT )").root, (n) => {
    if (n.type === SQL_NODE.ERROR) errors.push(n.error);
  });
  assert.deepEqual(errors, ["unmatched"]);
});

test("a macro is one opaque node with its brackets, a variable is one leaf node", () => {
  const text = "SELECT $__timeFilter(ts, (1)) AND x = $v AND y = ${w:csv} AND z = $__from";
  const found = [];
  walk(parse(text).root, (n) => {
    if (n.type === "macro" || n.type === "variable") found.push([n.type, n.name, text.slice(n.start, n.end), n.format, n.open !== undefined && n.opaque === true]);
  });
  assert.deepEqual(found, [
    ["macro", "$__timeFilter", "$__timeFilter(ts, (1))", undefined, true],
    ["variable", "v", "$v", undefined, false],
    ["variable", "w", "${w:csv}", "csv", false],
    ["macro", "$__from", "$__from", undefined, false],
  ]);
  // A macro name followed by a blank is not a call: the bracket after the blank is a plain paren.
  assert.equal(outline("SELECT $__from (1)"), "script(statement(query(clause:SELECT(expr(macro:$__from<$__from> paren(expr(literal<1>)))))))");
  // The bracket of a macro in a string or a comment is no node.
  assert.equal(outline("SELECT '$__f(' -- $__g(\n"), "script(statement(query(clause:SELECT(expr(literal<'$__f('>)))) comment<-- $__g(>)");
});

test("a lambda: the arrow, the parameters and the end of the body", () => {
  const lambdas = (text) => {
    const out = [];
    walk(parse(text).root, (n) => {
      if (n.lambda) out.push([text.slice(n.start, n.end), n.params, text[n.arrow]]);
    });
    return out;
  };
  assert.deepEqual(lambdas("SELECT arrayMap(x -> x + 1, a)"), [["x -> x + 1", ["x"], "-"]]);
  assert.deepEqual(lambdas("SELECT arrayMap((x, y) -> x + y, a, b)"), [["(x, y) -> x + y", ["x", "y"], "-"]]);
  assert.deepEqual(lambdas("SELECT f(a, x -> g(x, 1), y -> y)"), [["x -> g(x, 1)", ["x"], "-"], ["y -> y", ["y"], "-"]]);
  assert.deepEqual(lambdas("SELECT arrayFilter(x -> x IN (1, 2), a)"), [["x -> x IN (1, 2)", ["x"], "-"]]);
  assert.deepEqual(lambdas("SELECT f(1 + x -> x)").map((l) => l[0]), ["x -> x"]);
  assert.deepEqual(lambdas("SELECT f(x -> y -> z)").map((l) => l[0]), ["x -> y -> z", "y -> z"]);
  // The arrow has no lambda in PostgreSQL (a JSON operator).
  const pg = [];
  walk(parse("SELECT a -> 'b'", "postgres").root, (n) => {
    if (n.lambda) pg.push(n);
  });
  assert.deepEqual(pg, []);
});

test("a comment is a node of the innermost node that holds the code on both sides", () => {
  const text = "SELECT a /*1*/, /*2*/ f(/*3*/ x /*4*/) /*5*/ FROM /*6*/ t -- 7\n";
  assert.equal(
    outline(text),
    "script(statement(query(clause:SELECT(list(expr<a> comment</*1*/> comment</*2*/> expr(call:f(comment</*3*/> expr<x> comment</*4*/>)))) comment</*5*/> clause:FROM(comment</*6*/> expr<t>))) comment<-- 7>)",
  );
  const r = parse(text);
  assert.deepEqual(faults(text), []);
  const styles = [];
  walk(r.root, (n) => {
    if (n.type === "comment") styles.push(n.style);
  });
  assert.deepEqual(styles, ["block", "block", "block", "block", "block", "block", "line"]);
  assert.equal(outline("/* a */ SELECT 1"), "script(comment</* a */> statement(query(clause:SELECT(expr(literal<1>)))))");
  assert.equal(outline("-- a\n"), "script(comment<-- a>)");
  // An unclosed comment has closed false.
  const open = parse("SELECT 1 /* x").root.children[1];
  assert.equal(open.type, "comment");
  assert.equal(open.closed, false);
});

test("foldable: a subquery, a CTE, a paren and an array of two lines or more", () => {
  const folds = (text) => {
    const out = [];
    walk(parse(text).root, (n) => {
      if (n.foldable) out.push(`${n.type}@${text.slice(n.start, n.end).split("\n").length}`);
    });
    return out;
  };
  assert.deepEqual(folds("SELECT (1)"), []);
  assert.deepEqual(folds("SELECT (\n1\n)"), ["paren@3"]);
  assert.deepEqual(folds("SELECT [\n1,\n2\n]"), ["array@4"]);
  assert.deepEqual(folds("SELECT * FROM (\nSELECT 1\n)"), ["subquery@3"]);
  assert.deepEqual(folds("WITH a AS (\nSELECT 1\n) SELECT 1"), ["cte@3", "subquery@3"]);
  assert.deepEqual(folds("WITH 1 AS\nx SELECT x"), ["cte@2"]);
  // Not a call, not a macro, not an unclosed bracket, not a statement.
  assert.deepEqual(folds("SELECT f(\n1\n), $__timeFilter(\nts\n)"), []);
  assert.deepEqual(folds("SELECT (\n1\n"), []);
  assert.deepEqual(folds("SELECT 1\nFROM t\nWHERE 1"), []);
});

test("WITH: the CTE forms, RECURSIVE, and the main query", () => {
  const ctes = (text, d = "clickhouse") => {
    const out = [];
    walk(parse(text, d).root, (n) => {
      if (n.type === "cte") out.push(`${n.name}/${n.form}`);
      if (n.type === "with") out.push(n.name);
    });
    return out;
  };
  assert.deepEqual(ctes("WITH a AS (SELECT 1), b AS (SELECT 2) SELECT 3"), ["WITH", "a/name", "b/name"]);
  assert.deepEqual(ctes("WITH 1 AS x, (SELECT 2) AS y, f(3) AS z SELECT 4"), ["WITH", "x/expr", "y/expr", "z/expr"]);
  assert.deepEqual(ctes("WITH RECURSIVE r(n) AS (SELECT 1) SELECT n FROM r", "postgres"), ["WITH RECURSIVE", "r/name"]);
  assert.deepEqual(ctes("WITH \"my cte\" AS NOT MATERIALIZED (SELECT 1) SELECT 2", "postgres"), ["WITH", "\"my cte\"/name"]);
  // The main query starts at the first SELECT of the top level. A SELECT in a bracket does not end the WITH list.
  const text = "WITH a AS (SELECT 1) SELECT 2 FROM a";
  const statement = parse(text).root.children[0];
  assert.deepEqual(statement.children.map((c) => c.type), ["with", "query"]);
  assert.equal(text.slice(statement.children[1].start, statement.children[1].end), "SELECT 2 FROM a");
  // Incomplete texts keep a partial tree.
  assert.deepEqual(faults("WITH"), []);
  assert.deepEqual(faults("WITH a AS ("), []);
  assert.deepEqual(faults("WITH a AS (SELECT 1),"), []);
  assert.deepEqual(faults("WITH a AS (SELECT 1) , , SELECT 2"), []);
});

test("a statement ends at a top-level semicolon only", () => {
  assert.deepEqual(sources("SELECT 1; SELECT 2;; SELECT 3", "statement"), ["SELECT 1;", "SELECT 2;", ";", "SELECT 3"]);
  assert.deepEqual(sources("SELECT f(1;2); SELECT 3", "statement"), ["SELECT f(1;2);", "SELECT 3"]);
  assert.deepEqual(sources("SELECT ';' -- ;\n; SELECT 'x;'", "statement"), ["SELECT ';' -- ;\n;", "SELECT 'x;'"]);
  assert.deepEqual(sources("", "statement"), []);
  assert.deepEqual(sources("  \n", "statement"), []);
});

test("a statement with another first word is a query with a first clause named after the word", () => {
  const names = (text) => {
    const out = [];
    walk(parse(text).root, (n) => {
      if (n.type === SQL_NODE.CLAUSE) out.push(n.name);
    });
    return out;
  };
  assert.deepEqual(names("EXPLAIN SELECT 1"), ["EXPLAIN", "SELECT"]);
  assert.deepEqual(names("show tables"), ["SHOW"]);
  assert.deepEqual(names("1 + 1"), [""]);
  assert.deepEqual(names("$v"), [""]);
  assert.deepEqual(names("(SELECT 1) UNION ALL (SELECT 2)"), ["", "SELECT", "UNION ALL", "SELECT"]);
});

test("the error nodes: unmatched closers and unclosed brackets", () => {
  assert.equal(outline("SELECT 1)"), "script(statement(query(clause:SELECT(expr(literal<1> error!unmatched<)>)))))");
  assert.equal(outline("SELECT (1]"), "script(statement(query(clause:SELECT(expr(paren!unclosed(expr(literal<1> error!unmatched<]>)))))))");
  assert.equal(outline("SELECT (1, [2"), "script(statement(query(clause:SELECT(expr(paren!unclosed(list(expr(literal<1>) expr(array!unclosed(expr(literal<2>))))))))))");
  // A closer ends the brackets that it skips: the inner one is unclosed, the outer one is closed.
  const r = parse("SELECT f(a, [b)");
  const nodes = [];
  walk(r.root, (n) => {
    if (n.open !== undefined) nodes.push([n.type, n.error, n.close !== undefined]);
  });
  assert.deepEqual(nodes, [["call", undefined, true], ["array", "unclosed", false]]);
});

test("the dialect tables: PostgreSQL has no ClickHouse words and no lambda, ClickHouse has no FETCH or FOR", () => {
  assert.equal(outline("SELECT x FROM t FETCH 1 FOR UPDATE"), "script(statement(query(clause:SELECT(expr<x>) clause:FROM(expr(literal<1>)))))");
  assert.equal(outline("SELECT x FROM t FETCH 1", "postgres"), "script(statement(query(clause:SELECT(expr<x>) clause:FROM(expr<t>) clause:FETCH(expr(literal<1>)))))");
  assert.equal(outline("SELECT a ARRAY JOIN b", "postgres"), "script(statement(query(clause:SELECT(expr<a ARRAY>) clause:JOIN(expr<b>))))");
  assert.equal(outline("SELECT $$x$$, E'a\\'b'::text", "postgres"), "script(statement(query(clause:SELECT(list(expr(literal<$$x$$>) expr(literal<E'a\\'b'>))))))");
});

test("the tokens of the result are the tokens of the call, or the tokens of the dialect", () => {
  const text = "SELECT a -> b FROM t";
  const tokens = tokensClickHouse(text);
  assert.equal(parseSql(text, "clickhouse", tokens).tokens, tokens);
  assert.deepEqual(parseSql(text).tokens, tokens);
  assert.deepEqual(parseSql(text, "unknown").tokens, tokens);
  assert.equal(outline(text), outline(text, "unknown"));
  assert.equal(parseSql(undefined).root.end, 0);
  assert.equal(parseSql(null, "clickhouse").tokens.length, 0);
});

test("setTokenizer: the tokenizer of a dialect, and a dialect with no tokenizer gives an empty tree", () => {
  // No tokenizer for postgres, and no tokens: one `other` token and an empty tree. This is how the module stands until tokens_pg.js registers.
  const before = parseSql("SELECT 1", "postgres");
  if (before.tokens.length === 1 && before.tokens[0].kind === TOKEN.OTHER) {
    assert.deepEqual(before.root.children, []);
    setTokenizer("postgres", pgTokens);
    assert.equal(parseSql("SELECT 1", "postgres").root.children.length, 1);
  }
  setTokenizer("postgres", pgTokens);
  assert.equal(outline("SELECT 1", "postgres"), "script(statement(query(clause:SELECT(expr(literal<1>)))))");
  setTokenizer("nothing", () => []);
  setTokenizer("postgres", "not a function");
  assert.equal(parseSql("SELECT 1", "postgres").root.children.length, 1);
});

test("a leaf node has no child and shares one frozen children array", () => {
  const r = parse("SELECT 1, 'a', $v, -- c\n )");
  const leaves = [];
  walk(r.root, (n) => {
    if (["literal", "comment", "variable", "error"].includes(n.type)) leaves.push(n);
  });
  assert.ok(leaves.length >= 5);
  for (const n of leaves) {
    assert.equal(n.children.length, 0);
    assert.ok(Object.isFrozen(n.children));
  }
});

// ---------------------------------------------------------------------------------------------------------------------
// Tolerance: no exception, no lost token, no lost bracket, for a broken text.

const SAMPLES = [
  "SELECT a, b FROM t WHERE x = 1 AND y IN (1, 2) ORDER BY a DESC LIMIT 10",
  "SELECT toStartOfInterval(ts, INTERVAL 1 MINUTE) AS t, count() AS c FROM logs WHERE $__timeFilter(ts) GROUP BY t ORDER BY t",
  "WITH a AS (SELECT 1 AS x), b AS (SELECT x + 1 AS y FROM a) SELECT * FROM a, b",
  "SELECT arrayMap(x -> x * 2, [1, 2, 3]) AS m, arrayFilter((k, v) -> v > 0, ks, vs) FROM t",
  "SELECT a FROM t1 LEFT JOIN t2 ON t1.k = t2.k ARRAY JOIN arr AS e WHERE e != '' SETTINGS max_threads = 4 FORMAT JSON",
  "SELECT 1 UNION ALL SELECT 2 UNION DISTINCT (SELECT 3)",
  "SELECT '${v}' AS s, ${w:csv}, $name, {p:UInt8}, \"q\", `n` -- a comment\n/* block */ FROM t",
  "SELECT x FROM t LIMIT 3 BY k, j PREWHERE p = 1",
  "SELECT CASE WHEN a THEN 1 ELSE 2 END, multiIf(a, 1, b, 2, 3), (1, 2), [[1], [2]], map('a', 1)['a'] FROM t",
  "SELECT x FROM (SELECT y FROM (SELECT z FROM w WHERE z IN (SELECT 1)) a) b",
  "INSERT INTO t (a, b) VALUES (1, 'x'), (2, 'y')",
  "SELECT * EXCEPT (a, b) FROM t WHERE ts BETWEEN $__fromTime AND $__toTime",
  "SELECT quantile(0.5)(x), sumIf(a, b > 1), countIf(x = 'é€😀') FROM t GROUP BY ALL",
  "SELECT a FROM t ORDER BY a WITH FILL FROM 1 TO 10 STEP 2 INTERPOLATE (b AS b + 1)",
  "SELECT 1;\n-- next\nSELECT 2 /* c */;\n\nSELECT 3",
  "SELECT a::int, b->>'k', c || 'x', d[1], ARRAY[1, 2] FROM t FETCH FIRST 2 ROWS ONLY",
  "WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 3) SELECT n FROM r",
  "SELECT $$a;b$$, E'a\\'b', U&'x', \"q\"\"r\" FROM t FOR UPDATE",
  "select\n  a,\n  b\nfrom\n  t\nwhere\n  (a = 1\n   or b = 2)\n",
  "SELECT f(g(h(i(j(k(1, 2), 3), 4), 5), 6), 7)",
];

// A deterministic generator of numbers.
function generator(seed) {
  let state = seed;
  return () => {
    state = (Math.imul(state, 1103515245) + 12345) & 0x7fffffff;
    return state / 0x7fffffff;
  };
}

test("tolerance: each prefix of 20 sample queries parses with no exception, in both dialects", () => {
  assert.equal(SAMPLES.length, 20);
  let count = 0;
  for (const text of SAMPLES) {
    for (const dialect of ["clickhouse", "postgres"]) {
      for (let n = 0; n <= text.length; n++) {
        const prefix = text.slice(0, n);
        assert.deepEqual(faults(prefix, dialect), [], `${dialect} ${JSON.stringify(prefix)}`);
        count += 1;
      }
    }
  }
  assert.ok(count > 3000, `${count} prefixes`);
});

test("tolerance: each suffix and each text with one deleted character parses with no exception", () => {
  for (const text of SAMPLES.slice(0, 10)) {
    for (let n = 0; n < text.length; n++) {
      assert.deepEqual(faults(text.slice(n)), [], JSON.stringify(text.slice(n)));
      assert.deepEqual(faults(text.slice(0, n) + text.slice(n + 1)), [], JSON.stringify(text.slice(0, n) + text.slice(n + 1)));
    }
  }
});

test("tolerance: texts of random tokens give a sound tree in both dialects", () => {
  const vocabulary = [
    "SELECT", "FROM", "WHERE", "(", ")", "[", "]", "{", "}", ",", ";", "->", "x", "a.b", "1", "'s'", "`n`", "\"q\"", "-- c\n", "/* c */",
    "$__f(", "$v", "${w}", "WITH", "AS", "UNION", "ALL", "LEFT", "JOIN", "ON", "LIMIT", "BY", "ORDER", "GROUP", "ARRAY", "FORMAT",
    "SETTINGS", "FILL", " ", "\n", "::", "||", "=", "'open", "/* open", "EXCEPT", "RECURSIVE", "NOT", "MATERIALIZED", "IN", "count", ":", ".",
  ];
  const random = generator(987654);
  for (let run = 0; run < 3000; run++) {
    let text = "";
    const length = 1 + Math.floor(random() * 60);
    for (let k = 0; k < length; k++) text += vocabulary[Math.floor(random() * vocabulary.length)] + (random() < 0.7 ? " " : "");
    for (const dialect of ["clickhouse", "postgres"]) assert.deepEqual(faults(text, dialect), [], `${dialect} ${JSON.stringify(text)}`);
  }
});

test("bracket coverage: each code bracket belongs to one node or one error node", () => {
  const texts = ["SELECT (((1)))", "SELECT ((1", "SELECT 1))", "SELECT [{(]})", "SELECT '(' \"[\" `{` -- )\n/* ] */ (1)", "(((", ")))", "{[(]})", "SELECT f(a, [b], {c: (d)})"];
  for (const text of texts) assert.deepEqual(faults(text), [], text);
  // Brackets in strings, names and comments are no code.
  const r = parse("SELECT '(' , \"[\", `{` -- )\n/* ] */");
  let brackets = 0;
  walk(r.root, (n) => {
    if (n.open !== undefined || n.type === SQL_NODE.ERROR) brackets += 1;
  });
  assert.equal(brackets, 0);
});

// ---------------------------------------------------------------------------------------------------------------------
// The caps.

test("caps: the input above 262,144 bytes gives the reason too_large and the tokens that cover the text", () => {
  assert.deepEqual(AST_REASONS, ["too_large", "too_deep", "too_many_nodes"]);
  const text = `SELECT ${"a, ".repeat(100000)}`;
  assert.ok(utf8Length(text) > MAX_AST_INPUT_BYTES);
  const r = parse(text);
  assert.equal(r.capped.reason, "too_large");
  assert.equal(r.root.type, "script");
  assert.equal(r.root.end, text.length);
  assert.deepEqual(r.root.children, []);
  assert.equal(r.tokens[r.tokens.length - 1].end, text.length);
  // The offset of the cap is the first unit that is past the cap in bytes.
  assert.equal(utf8Length(text.slice(0, r.capped.at + 1)), MAX_AST_INPUT_BYTES + 1);
  // The edge: exactly the cap is accepted, one byte more is not.
  const exact = "SELECT 1".padEnd(MAX_AST_INPUT_BYTES, " ");
  assert.equal(parse(exact).capped, null);
  assert.equal(parse(`${exact} `).capped.reason, "too_large");
  // The cap counts UTF-8 bytes: a two-byte unit counts two.
  const wide = `SELECT '${"é".repeat(131100)}'`;
  assert.ok(wide.length < MAX_AST_INPUT_BYTES);
  assert.equal(parse(wide).capped.reason, "too_large");
  assert.equal(parse(`SELECT '${"é".repeat(130000)}'`).capped, null);
  // A text of four-byte units (a pair of UTF-16 units).
  assert.equal(parse(`SELECT '${"😀".repeat(65600)}'`).capped.reason, "too_large");
});

test("caps: a nesting deeper than 128 gives a partial tree with the reason too_deep", () => {
  assert.equal(MAX_AST_DEPTH, 128);
  const nest = (depth) => `SELECT ${"(".repeat(depth)}1${")".repeat(depth)}`;
  assert.equal(parse(nest(128)).capped, null);
  assert.deepEqual(faults(nest(128)), []);
  const r = parse(nest(129));
  assert.equal(r.capped.reason, "too_deep");
  assert.equal(r.capped.at, nest(129).indexOf("(") + 128);
  assert.deepEqual(faults(nest(129)), []);
  assert.deepEqual(faults(nest(5000)), []);
  assert.equal(r.root.end, nest(129).length);
  // Subqueries and brackets of each kind count the same.
  assert.equal(parse(`SELECT ${"(SELECT ".repeat(128)}1${")".repeat(128)}`).capped, null);
  assert.equal(parse(`SELECT ${"(SELECT ".repeat(129)}1${")".repeat(129)}`).capped.reason, "too_deep");
  assert.equal(parse(`SELECT ${"f(".repeat(129)}1`).capped.reason, "too_deep");
  assert.equal(parse(`SELECT ${"[".repeat(129)}`).capped.reason, "too_deep");
  assert.equal(parse(`SELECT ${"$__m(".repeat(129)}`).capped.reason, "too_deep");
  assert.equal(parse("(".repeat(300000).slice(0, 200000)).capped.reason, "too_deep");
  // The tokens still cover the text.
  assert.equal(r.tokens[r.tokens.length - 1].end, nest(129).length);
});

test("caps: a tree of more than 100,000 nodes gives the reason too_many_nodes and never more nodes than the cap", () => {
  assert.equal(MAX_AST_NODES, 100000);
  const text = `SELECT ${"1,".repeat(130000)}`;
  assert.ok(utf8Length(text) <= MAX_AST_INPUT_BYTES);
  const r = parse(text);
  assert.equal(r.capped.reason, "too_many_nodes");
  assert.ok(countNodes(r.root) <= MAX_AST_NODES);
  assert.ok(countNodes(r.root) > MAX_AST_NODES - 100);
  assert.equal(r.root.end, text.length);
  assert.equal(r.tokens[r.tokens.length - 1].end, text.length);
  assert.deepEqual(faults(text), []);
  // Many statements, many comments and many brackets stop in the same way.
  assert.equal(parse("SELECT;".repeat(37000)).capped.reason, "too_many_nodes");
  assert.equal(parse("()".repeat(120000)).capped.reason, "too_many_nodes");
  assert.equal(parse("-- c\n".repeat(52000)).capped, null);
  // A tree just under the cap is complete.
  assert.equal(parse(`SELECT ${"1,".repeat(30000)}1`).capped, null);
});

test("caps: the partial tree has no exception and the same invariants for texts in each state", () => {
  const texts = [
    `SELECT ${"f(".repeat(200)}`,
    `${"(SELECT ".repeat(150)}1`,
    `SELECT ${"x -> ".repeat(2000)}1`,
    `WITH ${"a AS (SELECT 1), ".repeat(30000)}`,
  ];
  for (const text of texts) {
    const r = parse(text);
    assert.deepEqual(faults(text), [], text.slice(0, 40));
    assert.ok(r.capped === null || AST_REASONS.includes(r.capped.reason));
  }
});

test("linear time: a text of twice the size takes about twice the time", () => {
  const unit = "SELECT a, f(x, [1, 2]) FROM (SELECT b FROM t WHERE k IN (1, 2)) AS q ORDER BY a;\n";
  const make = (n) => unit.repeat(n);
  const time = (text) => {
    let best = Infinity;
    for (let run = 0; run < 5; run++) {
      const t0 = performance.now();
      parse(text);
      best = Math.min(best, performance.now() - t0);
    }
    return best;
  };
  time(make(100));
  const small = time(make(1000));
  const large = time(make(2000));
  assert.ok(large < small * 4 + 5, `1000 units ${small.toFixed(1)} ms, 2000 units ${large.toFixed(1)} ms`);
});

test("time: a query of 262,144 bytes parses in 100 ms or less (target 50 ms), minimum of 7 runs after warm-up parses", (t) => {
  const unit = "SELECT a, b + 1 AS c, f(x, [1, 2]), 'str' FROM (SELECT $__timeFilter(ts), arrayMap(x -> x * 2, v) FROM t WHERE k IN (1, 2) -- note\n) AS q WHERE ${v:csv} > 0 ORDER BY a LIMIT 10;\n";
  let text = "";
  while (text.length + unit.length <= MAX_AST_INPUT_BYTES) text += unit;
  assert.ok(utf8Length(text) <= MAX_AST_INPUT_BYTES);
  assert.ok(utf8Length(text) > MAX_AST_INPUT_BYTES - 200);
  // The warm-up gives the engine the time to compile the code. An editor parses again at each pause of the user, so the
  // steady state is what counts. The first parse of a session on a text of this size is slower: its time is in the report.
  const cold0 = performance.now();
  parseSql(text, "clickhouse");
  const cold = performance.now() - cold0;
  parseSql(text, "clickhouse");
  parseSql(text, "clickhouse");
  parseSql(text, "clickhouse");
  const times = [];
  for (let run = 0; run < 7; run++) {
    const t0 = performance.now();
    const r = parseSql(text, "clickhouse");
    times.push(performance.now() - t0);
    assert.equal(r.capped, null);
  }
  // The machine can be busy with other work. A round that is slower than the limit is repeated, three times at most.
  for (let round = 1; round < 4 && Math.min(...times) > 100; round++) {
    for (let run = 0; run < 3; run++) {
      const t0 = performance.now();
      parseSql(text, "clickhouse");
      times.push(performance.now() - t0);
    }
  }
  const best = Math.min(...times);
  t.diagnostic(`262144 bytes: first parse ${cold.toFixed(1)} ms; then ${times.map((x) => `${x.toFixed(1)} ms`).join(", ")}; best ${best.toFixed(1)} ms; ${text.length} units`);
  // The target is 50 ms on the reference machine (10.3c). The unit test has a limit of 100 ms for the minimum of 7 runs (16.10a), so a
  // loaded machine does not fail it; the browser case B176 measures the target.
  assert.ok(best <= 100, `best of the runs: ${best.toFixed(1)} ms`);
});

test("the DML rules: a data-modifying CTE body, ROWS FROM, SET, RETURNING, VALUES and ON CONFLICT", () => {
  const names = (text, d) => {
    const out = [];
    walk(parse(text, d).root, (n) => {
      if (n.type === SQL_NODE.QUERY) out.push([]);
      if (n.type === SQL_NODE.CLAUSE) out[out.length - 1].push(n.name);
    });
    return out;
  };
  const pg = "postgres";
  assert.deepEqual(names("WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d", pg), [["DELETE", "FROM", "RETURNING"], ["SELECT", "FROM"]]);
  assert.deepEqual(names("WITH i AS (INSERT INTO t VALUES (1) RETURNING a) SELECT a FROM i", pg), [["INSERT", "VALUES", "RETURNING"], ["SELECT", "FROM"]]);
  assert.deepEqual(names("WITH u AS (UPDATE t SET a = 1 RETURNING a) SELECT a FROM u", pg), [["UPDATE", "SET", "RETURNING"], ["SELECT", "FROM"]]);
  assert.deepEqual(names("SELECT * FROM ROWS FROM (f(1), g(2)) AS r(a, b)", pg), [["SELECT", "FROM"]]);
  assert.deepEqual(names("UPDATE t SET a = 1 WHERE b = 2 RETURNING a", pg), [["UPDATE", "SET", "WHERE", "RETURNING"]]);
  assert.deepEqual(names("INSERT INTO t (a) VALUES (1) ON CONFLICT (a) DO NOTHING RETURNING a", pg), [["INSERT", "VALUES", "ON CONFLICT", "RETURNING"]]);
  assert.deepEqual(names("INSERT INTO t (a) VALUES (1) ON CONFLICT (a) DO UPDATE SET a = 2", pg), [["INSERT", "VALUES", "ON CONFLICT", "SET"]]);
  // The ON of a join is no clause. A column or a function with a clause word as name is no clause.
  assert.deepEqual(names("SELECT set(a), returning, values FROM a JOIN b ON a.k = b.k", pg), [["SELECT", "FROM", "JOIN"]]);
  // A bracket that starts with a DML word and no word after it is a plain paren.
  assert.deepEqual(names("SELECT (delete), (update, 1)", pg), [["SELECT"]]);
  // ClickHouse: INSERT with VALUES, SETTINGS and FORMAT.
  assert.deepEqual(names("INSERT INTO t VALUES (1) SETTINGS a = 1 FORMAT JSON"), [["INSERT", "VALUES", "SETTINGS", "FORMAT"]]);
  assert.deepEqual(names("SELECT * FROM values('a Int', (1))"), [["SELECT", "FROM"]]);
  // The DML CTE body is a subquery that holds a query, and the CTE is a name-first cte.
  const text = "WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d";
  assert.equal(outline(text, pg).startsWith("script(statement(with:WITH(cte:d(subquery(query(clause:DELETE"), true);
  assert.deepEqual(faults(text, pg), []);
});

test("a parameter token is an opaque literal of the kind param, never a variable", () => {
  const found = (text, d) => {
    const out = [];
    walk(parse(text, d).root, (n) => {
      if (n.type === SQL_NODE.LITERAL && n.kind === "param") out.push(["param", text.slice(n.start, n.end), n.children.length]);
      if (n.type === SQL_NODE.VARIABLE) out.push(["variable", text.slice(n.start, n.end)]);
    });
    return out;
  };
  assert.deepEqual(found("SELECT $1, $2", "postgres"), [["param", "$1", 0], ["param", "$2", 0]]);
  assert.deepEqual(found("SELECT {p:UInt8}, {q:Array(String)}"), [["param", "{p:UInt8}", 0], ["param", "{q:Array(String)}", 0]]);
  // In ClickHouse the text `$1` is a variable, as sqlscan.js reads it (`findVariableNames` gives "1"). The tree follows the scan.
  assert.deepEqual(findVariableNames("SELECT $1"), ["1"]);
  assert.deepEqual(found("SELECT $1"), [["variable", "$1"]]);
});
