// FE-66 (10.3e, 10.3f): the structure functions of the query text: folds, bracketPartner, blockAt, and the model of the folded view.
// The tree is never a security check: these tests only look at the structure.
import test from "node:test";
import assert from "node:assert/strict";
import { blockAt, bracketPartner, folds, foldedRows, lineIndex, nonBlankBefore, offsetAfterNonBlank, outerFolds } from "../../web/js/editor/ast/structure.js";
import { MAX_FOLD_MARKERS } from "../../web/js/editor/ast/nodes.js";
import { parseSql } from "../../web/js/editor/ast/sql_tree.js";
import { parseJson } from "../../web/js/editor/ast/json_tree.js";
import "../../web/js/editor/ast/tokens_pg.js";

const WITH_TEXT = "WITH a AS (\n  SELECT 1\n)\nSELECT f(x,\n y), [1,\n2] FROM (SELECT 2\nFROM t) -- c\nWHERE x IN (1,2) AND (\n)";
const SAMPLES = [
  "",
  "   \n  ",
  "SELECT 1",
  "SELECT (1",
  "SELECT 1)",
  "SELECT 'a(\n' || \"b)\"\n, c",
  WITH_TEXT,
  "SELECT *\nFROM (\n  SELECT a\n  FROM (\n    SELECT 1\n  )\n)\nWHERE a > 0",
  "SELECT 1; SELECT (\n2\n)",
  "SELECT $__timeFilter(\n ts) FROM t -- (\n/* ) */",
  "SELECT arrayMap(x -> x + 1,\n a) FROM t",
  "SELECT '\u{1F600}(' , (\né)",
  "SELECT ((((\n))))",
  "(\n(\n(\n",
  "SELECT {a:String}, ${v}, $x FROM t WHERE (\n a)",
  "SELECT x::Int, $$ ( \n ) $$ FROM (\n t)",
  "SELECT 1 FROM t WHERE a IN (\n1,\n2\n) ORDER BY 1",
  "SELECT a FROM t UNION ALL\nSELECT b FROM (\nSELECT 1\n)",
  "WITH x AS (SELECT 1), y AS (\nSELECT 2\n) SELECT * FROM x",
  "SELECT CASE WHEN a THEN (\n1) END",
];
const JSON_SAMPLES = [
  "",
  "[]",
  '[{"$match":{"a":1}},\n{"$group":{\n"x":1}}]',
  '[{"$facet":{"a":[\n{"$limit":1}\n]}}]',
  '{"aggregate":[\n{"$sort":{"a":1}}\n]',
  '[{"$match":{"h":"$host"}},\n',
  '[1,\n2,\n3]',
  '[{"a":\n1},}',
  '"text',
  '[[[[\n]]]]',
];

const parse = (text, dialect) => (dialect === "json" ? parseJson(text, { variables: [] }) : parseSql(text, dialect));
const CASES = [...SAMPLES.map((t) => [t, "clickhouse"]), ...SAMPLES.map((t) => [t, "postgres"]), ...JSON_SAMPLES.map((t) => [t, "json"])];

test("FE-66: no function throws for each caret offset, and an offset outside the text gives null", () => {
  assert.ok(CASES.length >= 30);
  for (const [text, dialect] of CASES) {
    const result = parse(text, dialect);
    for (let o = 0; o <= text.length; o++) {
      const pair = bracketPartner(result, o);
      const block = blockAt(result, o, text);
      assert.ok(pair === null || typeof pair === "object");
      assert.ok(block === null || typeof block === "object");
    }
    for (const o of [-1, text.length + 1, 1.5, NaN, undefined, null]) {
      assert.equal(bracketPartner(result, o), null, `${dialect} ${JSON.stringify(text)} ${o}`);
      assert.equal(blockAt(result, o, text), null);
    }
    const f = folds(result, text);
    assert.ok(f.total >= f.items.length);
  }
});

test("FE-66: the folds are the pairs of brackets on two lines or more, in order, with the lines of 10.3e", () => {
  const r = parseSql(WITH_TEXT);
  const { items, total, truncated } = folds(r, WITH_TEXT);
  assert.equal(truncated, false);
  assert.equal(total, items.length);
  assert.deepEqual(items.map((f) => [f.type, f.open, f.close, f.lines]), [
    ["subquery", 10, 23, 3],
    ["call", 33, 39, 2],
    ["array", 42, 47, 2],
    ["subquery", 54, 70, 2],
    ["paren", 98, 100, 2],
  ].filter((x) => items.some((f) => f.open === x[1])));
  for (const f of items) {
    assert.equal(f.from, f.open + 1);
    assert.equal(f.to, f.close);
    assert.equal(WITH_TEXT[f.open] !== undefined, true);
    assert.ok(f.endLine > f.line);
  }
  for (let i = 1; i < items.length; i++) assert.ok(items[i].open > items[i - 1].open);
  // The first fold is the body of the CTE.
  assert.equal(items[0].type, "subquery");
  assert.equal(items[0].lines, 3);
  // A fold of one line is not a fold.
  assert.equal(folds(parseSql("SELECT (1, 2) FROM (SELECT 1)"), "SELECT (1, 2) FROM (SELECT 1)").total, 0);
});

test("FE-66: a node with no close bracket does not fold, and a macro never folds", () => {
  const text = "SELECT $__timeFilter(\n ts) FROM t WHERE (\n a";
  const r = parseSql(text);
  assert.deepEqual(folds(r, text).items.map((f) => f.type), []);
});

test("FE-66: a capped result has no fold, no pair and no block", () => {
  const text = `SELECT ${"(".repeat(200)}\n1${")".repeat(200)}`;
  const r = parseSql(text);
  assert.ok(r.capped);
  assert.deepEqual(folds(r, text), { items: [], total: 0, truncated: false });
  assert.equal(bracketPartner(r, 8), null);
  assert.equal(blockAt(r, 8, text), null);
  assert.deepEqual(folds(null, ""), { items: [], total: 0, truncated: false });
});

test("FE-66: 2,001 foldable blocks give 2,000 items, the total and the flag", () => {
  const text = `SELECT ${Array.from({ length: MAX_FOLD_MARKERS + 1 }, () => "f(\n1)").join(", ")}`;
  const r = parseSql(text);
  assert.equal(r.capped, null);
  const f = folds(r, text);
  assert.equal(f.items.length, MAX_FOLD_MARKERS);
  assert.equal(f.total, MAX_FOLD_MARKERS + 1);
  assert.equal(f.truncated, true);
});

test("FE-66: the bracket pair: before the caret first, then at the caret, with the partner", () => {
  const text = "SELECT (a + (b)) FROM t";
  const r = parseSql(text);
  // The caret after the last ")" of "(a + (b))" sees the bracket before it.
  assert.deepEqual(bracketPartner(r, 16), { at: 15, partner: 7, side: "close", matched: true });
  // The caret before "(" sees the bracket at the caret.
  assert.deepEqual(bracketPartner(r, 7), { at: 7, partner: 15, side: "open", matched: true });
  // The caret after "(" sees the bracket before it.
  assert.deepEqual(bracketPartner(r, 8), { at: 7, partner: 15, side: "open", matched: true });
  assert.deepEqual(bracketPartner(r, 13), { at: 12, partner: 14, side: "open", matched: true });
  assert.equal(bracketPartner(r, 0), null);
  assert.equal(bracketPartner(r, 18), null);
  // A bracket and its partner are each other's partner.
  for (let o = 0; o <= text.length; o++) {
    const p = bracketPartner(r, o);
    if (!p || !p.matched) continue;
    const q = bracketPartner(r, p.partner + 1);
    assert.equal(q.at, p.partner);
    assert.equal(q.partner, p.at);
  }
});

test("FE-66: a lone bracket has no partner: an unclosed node, an unmatched close, and a bracket in a string is not code", () => {
  const open = parseSql("SELECT (1");
  assert.deepEqual(bracketPartner(open, 8), { at: 7, partner: null, side: "open", matched: false });
  const close = parseSql("SELECT 1)");
  assert.deepEqual(bracketPartner(close, 9), { at: 8, partner: null, side: "lone", matched: false });
  const string = parseSql("SELECT '(' FROM t");
  assert.equal(bracketPartner(string, 9), null);
  const comment = parseSql("SELECT 1 -- (\n");
  assert.equal(bracketPartner(comment, 13), null);
  const j = parseJson('{"a": [1, 2');
  assert.equal(bracketPartner(j, 7).matched, false);
});

test("FE-66: the current block is the innermost subquery, CTE or clause of two lines or more", () => {
  const text = "SELECT *\nFROM (\n  SELECT a\n  FROM (\n    SELECT 1\n  )\n)\nWHERE a > 0";
  const r = parseSql(text);
  const at = (s) => text.indexOf(s);
  const inner = blockAt(r, at("SELECT 1") + 3, text);
  assert.equal(inner.type, "subquery");
  assert.deepEqual([inner.line, inner.endLine, inner.lines], [3, 5, 3]);
  const outer = blockAt(r, at("SELECT a") + 2, text);
  assert.equal(outer.type, "subquery");
  assert.deepEqual([outer.line, outer.endLine], [1, 6]);
  // A one-line clause is no block.
  assert.equal(blockAt(r, at("WHERE") + 2, text), null);
  // The caret before the open bracket is not in the subquery.
  assert.equal(blockAt(r, at("(\n  SELECT a"), text).type, "clause");
  // The caret right after the open bracket, and right before the close bracket, are in the subquery.
  assert.equal(blockAt(r, at("(\n  SELECT a") + 1, text).type, "subquery");
  assert.equal(blockAt(r, at(")\nWHERE"), text).type, "subquery");
  // After the close bracket, the caret is out of the subquery, but still in the clause FROM that holds it.
  const after = blockAt(r, at(")\nWHERE") + 1, text);
  assert.deepEqual([after.type, after.name], ["clause", "FROM"]);
});

test("FE-66: a clause that spans two lines is a block, and a CTE with its name is a block", () => {
  const text = "SELECT a,\n  b\nFROM t";
  const r = parseSql(text);
  const b = blockAt(r, 3, text);
  assert.equal(b.type, "clause");
  assert.equal(b.name, "SELECT");
  assert.deepEqual([b.line, b.endLine], [0, 1]);
  const cte = "WITH a AS (\n  SELECT 1\n)\nSELECT 2";
  const rc = parseSql(cte);
  assert.equal(blockAt(rc, 5, cte).type, "cte");
  assert.equal(blockAt(rc, 16, cte).type, "subquery");
});

test("FE-66: for a JSON text the block is the innermost object or array of two lines or more", () => {
  const text = '[{"$match":{"a":1}},\n{"$group":{\n"x":1}}]';
  const r = parseJson(text, { variables: [] });
  const b = blockAt(r, text.indexOf('"x"'), text);
  assert.equal(b.type, "object");
  assert.equal(b.line, 1);
  assert.equal(blockAt(r, 5, text).type, "array");
  // A SQL array is not a block.
  const sql = "SELECT [1,\n2]";
  assert.equal(blockAt(parseSql(sql), 11, sql).type, "clause");
});

test("folded model: the outer folds, the rows and the real line of each row", () => {
  const text = "a (\n b (\n  c\n )\n)\nz (\n y\n)\nend";
  const r = parseSql(text);
  const { items } = folds(r, text);
  assert.equal(items.length, 3);
  const keys = new Set(items.map((f) => f.key));
  const outer = outerFolds(items, keys);
  assert.deepEqual(outer.map((f) => f.open), [items[0].open, items[2].open]);
  const { rowLines, rowOfLine } = foldedRows(text, outer);
  // Lines: 0 "a (", 1 " b (", 2 "  c", 3 " )", 4 ")", 5 "z (", 6 " y", 7 ")", 8 "end".
  assert.deepEqual(rowLines, [0, 5, 8]);
  assert.deepEqual(Array.from(rowOfLine), [0, 0, 0, 0, 0, 1, 1, 1, 2]);
  // Only the inner fold is folded: the outer lines stay.
  const only = outerFolds(items, new Set([items[1].key]));
  assert.deepEqual(foldedRows(text, only).rowLines, [0, 1, 4, 5, 6, 7, 8]);
  assert.deepEqual(outerFolds(items, new Set()), []);
});

test("non-blank counts map a caret through a change of blanks", () => {
  const before = "select  a,b\nfrom t";
  const after = "select a, b\nfrom t";
  for (let o = 0; o <= before.length; o++) {
    const n = nonBlankBefore(before, o);
    const mapped = offsetAfterNonBlank(after, n);
    assert.equal(nonBlankBefore(after, mapped), n);
  }
  assert.equal(offsetAfterNonBlank("a b", 0), 0);
  assert.equal(offsetAfterNonBlank("a b", 99), 3);
  assert.equal(lineIndex("a\nb\n").count, 3);
  assert.equal(lineIndex("a\nb\n").lineOf(2), 1);
});
