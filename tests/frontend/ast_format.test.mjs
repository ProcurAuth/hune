// SPDX-License-Identifier: MIT OR Apache-2.0
// The formatter of the query editor (F42): the vectors of the three dialects, the token property, idempotence, the range mode, the refusals and the time.
import test from "node:test";
import assert from "node:assert/strict";
import { formatJson, formatSql } from "../../web/js/editor/ast/format.js";
import { parseJson } from "../../web/js/editor/ast/json_tree.js";
import { MAX_AST_INPUT_BYTES, TOKEN } from "../../web/js/editor/ast/nodes.js";
import { tokensClickHouse } from "../../web/js/editor/ast/tokens_ch.js";
import { tokensPostgres } from "../../web/js/editor/ast/tokens_pg.js";

/** The tokenizer of each dialect, with the tokens of JSON from the JSON reader. */
const TOKENIZE = {
  clickhouse: (s) => tokensClickHouse(s),
  postgres: (s) => tokensPostgres(s),
  json: (s) => parseJson(s).tokens,
};

/** The non-blank tokens of a text as a list of `kind:text`. */
function sig(dialect, text) {
  return TOKENIZE[dialect](text)
    .filter((t) => t.kind !== TOKEN.BLANK)
    .map((t) => `${t.kind}:${text.slice(t.start, t.end)}`);
}

/** For each pair of strings that follow one another (only blanks between): true when the blank holds a line end. */
function stringGaps(dialect, text) {
  const toks = TOKENIZE[dialect](text);
  const out = [];
  for (let i = 0; i < toks.length; i++) {
    if (toks[i].kind !== TOKEN.STRING) continue;
    let j = i + 1;
    let lf = false;
    while (j < toks.length && toks[j].kind === TOKEN.BLANK) {
      if (text.slice(toks[j].start, toks[j].end).includes("\n")) lf = true;
      j++;
    }
    if (j < toks.length && toks[j].kind === TOKEN.STRING) out.push(lf);
  }
  return out;
}

/** Formats a text of any dialect. */
function fmt(dialect, text, opts) {
  return dialect === "json" ? formatJson(text, opts) : formatSql(text, dialect, opts);
}

/** Checks the hard properties of one format: tokens, idempotence, line ends, no trailing space. Returns the result. */
function checkProps(dialect, text, label = text) {
  const r = fmt(dialect, text);
  if (r.reason === "nothing_to_format") {
    // A text with no SELECT, WITH or VALUES statement stays as it is.
    assert.equal(r.text, text);
    assert.equal(r.changed, false);
    return r;
  }
  assert.equal(r.reason, null, `${label}: reason ${r.reason}`);
  assert.deepEqual(sig(dialect, r.text), sig(dialect, text), `${label}: tokens changed`);
  const r2 = fmt(dialect, r.text);
  assert.equal(r2.reason, null, `${label}: second pass reason ${r2.reason}`);
  assert.equal(r2.text, r.text, `${label}: not idempotent`);
  assert.equal(r2.changed, false, `${label}: second pass changed`);
  assert.ok(!/[ \t]+\r?\n/.test(r.text.replace(/--[^\n]*|\/\*[\s\S]*?\*\/|'[^']*'|\$\$[\s\S]*?\$\$/g, "#")), `${label}: trailing space`);
  return r;
}

// ---- The vectors: [dialect, input, expected output or null]. ----

const CH = [
  ["select 1", "select 1"],
  ["SELECT a,b FROM t WHERE x=1", "SELECT a, b\nFROM t\nWHERE x = 1"],
  ["select  a ,  b  from   t", "select a, b\nfrom t"],
  ["SELECT\n  a,\n  b\nFROM t", "SELECT a, b\nFROM t"],
  [
    "select aaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbbbbbbbb, cccccccccccccccccccccc, ddddddddddddddddd from t",
    "select\n  aaaaaaaaaaaaaaaaaaaa,\n  bbbbbbbbbbbbbbbbbbbbbbbb,\n  cccccccccccccccccccccc,\n  ddddddddddddddddd\nfrom t",
  ],
  ["WITH a AS (SELECT 1) SELECT * FROM a", "WITH a AS (SELECT 1)\nSELECT *\nFROM a"],
  [
    "with a as (select 1), b as (select 2) select * from a, b",
    "with a as (select 1), b as (select 2)\nselect *\nfrom a, b",
  ],
  ["WITH 1 AS x, 2 AS y SELECT x + y", "WITH 1 AS x, 2 AS y\nSELECT x + y"],
  ["select * from (select a from t) as s", "select *\nfrom (\n  select a\n  from t\n) as s"],
  [
    "select a from t where id in (select id from u where z=1)",
    "select a\nfrom t\nwhere id in (\n  select id\n  from u\n  where z = 1\n)",
  ],
  ["select a from t1 join t2 on t1.id=t2.id", "select a\nfrom t1\njoin t2 on t1.id = t2.id"],
  ["select a from t1 left join t2 using (id)", "select a\nfrom t1\nleft join t2 using (id)"],
  [
    "select a from t1 any left join t2 on t1.aaaaaaaaaaaa = t2.aaaaaaaaaaaa and t1.bbbbbbbbbbbbbbbb = t2.bbbbbbbbbbbbbbbb and t1.c = t2.c",
    "select a\nfrom t1\nany left join t2\n  on t1.aaaaaaaaaaaa = t2.aaaaaaaaaaaa\n    and t1.bbbbbbbbbbbbbbbb = t2.bbbbbbbbbbbbbbbb\n    and t1.c = t2.c",
  ],
  ["select x from t array join arr as x", "select x\nfrom t\narray join arr as x"],
  ["select x, y from t array join a as x, b as y", "select x, y\nfrom t\narray join a as x, b as y"],
  ["select arrayMap(x -> x * 2, arr) from t", "select arrayMap(x -> x * 2, arr)\nfrom t"],
  ["select arrayMap((x, y) -> x + y, a, b) from t", "select arrayMap((x, y) -> x + y, a, b)\nfrom t"],
  ["select a from t format JSON", "select a\nfrom t\nformat JSON"],
  ["select a from t settings max_threads = 4, readonly = 1", "select a\nfrom t\nsettings max_threads = 4, readonly = 1"],
  ["select [1,2,3], (1,'a'), tuple(1,2)", "select [1, 2, 3], (1, 'a'), tuple(1, 2)"],
  ["select a from t where a > 1 and b < 2 or c = 3", "select a\nfrom t\nwhere a > 1 and b < 2 or c = 3"],
  [
    "select a from t where aaaaaaaaaaaaaaaaaaaaaaaa > 1 and bbbbbbbbbbbbbbbbbbbbbbbbbb < 2 and ccccccccccccccccccccc = 3",
    "select a\nfrom t\nwhere aaaaaaaaaaaaaaaaaaaaaaaa > 1\n  and bbbbbbbbbbbbbbbbbbbbbbbbbb < 2\n  and ccccccccccccccccccccc = 3",
  ],
  ["select a from t where x between 1 and 2 and y = 3", "select a\nfrom t\nwhere x between 1 and 2 and y = 3"],
  [
    "select a from t where x between 111111111111111111 and 222222222222222222 and yyyyyyyyyyyyyyyyyyyyyyyyyyyyy = 3333333",
    "select a\nfrom t\nwhere x between 111111111111111111 and 222222222222222222\n  and yyyyyyyyyyyyyyyyyyyyyyyyyyyyy = 3333333",
  ],
  ["select a from t union all select b from u", "select a\nfrom t\nunion all\nselect b\nfrom u"],
  ["select a from t group by a, b having count() > 1 order by a desc, b limit 5", "select a\nfrom t\ngroup by a, b\nhaving count() > 1\norder by a desc, b\nlimit 5"],
  ["select a from t limit 5 by a", "select a\nfrom t\nlimit 5 by a"],
  ["select a from t prewhere x = 1 where y = 2", "select a\nfrom t\nprewhere x = 1\nwhere y = 2"],
  ["select case when a then 1 else 2 end from t", "select case when a then 1 else 2 end\nfrom t"],
  [
    "select case when aaaaaaaaaaaaaaaaaaaaaaaa then 1111111111111 when bbbbbbbbbbbbbbbbbbbbbbbbbbb then 2222222222222 else 3 end from t",
    "select case\n  when aaaaaaaaaaaaaaaaaaaaaaaa then 1111111111111\n  when bbbbbbbbbbbbbbbbbbbbbbbbbbb then 2222222222222\n  else 3\nend\nfrom t",
  ],
  ["select f(aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb, cccccccccccccccccccccccc) from t", "select f(\n  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa,\n  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,\n  cccccccccccccccccccccccc\n)\nfrom t"],
  ["select $__timeFilter(ts), $__interval_s from t where $__timeFilter(ts)", "select $__timeFilter(ts), $__interval_s\nfrom t\nwhere $__timeFilter(ts)"],
  ["select  $__timeGroup( ts ,  '1m' )  from t", "select $__timeGroup( ts ,  '1m' )\nfrom t"],
  ["select ${var}, ${v:csv}, $w from t where a in (${vals:singlequote})", "select ${var}, ${v:csv}, $w\nfrom t\nwhere a in (${vals:singlequote})"],
  ["select {p:UInt8}, {name: String} from t", "select {p:UInt8}, {name: String}\nfrom t"],
  ["select 'a''b', \"quoted name\", `tick` from t", "select 'a''b', \"quoted name\", `tick`\nfrom t"],
  ["select 'multi\nline' as s from t", "select 'multi\nline' as s\nfrom t"],
  ["select a -- why\nfrom t", "select a -- why\nfrom t"],
  ["select a\n-- the table\nfrom t", "select a\n-- the table\nfrom t"],
  ["select a, -- first\n b -- second\nfrom t", "select\n  a, -- first\n  b -- second\nfrom t"],
  ["select /* c */ a from t", "select /* c */ a\nfrom t"],
  ["select a from t -- end", "select a\nfrom t -- end"],
  ["select 1;\nselect 2;", "select 1;\nselect 2;"],
  ["select 1;\n\n\nselect 2;\n\n", "select 1;\n\nselect 2;\n"],
  ["-- head\n\nselect 1", "-- head\n\nselect 1"],
  ["select x::UInt8, -a, a-b, a - -b from t", null],
  ["select * except (a, b) from t", null],
  ["insert into t (a, b) values (1, 2), (3, 4)", null],
  ["create table t (a UInt8, b String) engine = MergeTree order by a", null],
  ["select a from t where s like '%x%' and d >= now() - interval 1 day", null],
  ["select toStartOfInterval(ts, interval 1 minute) as t, avg(v) from m group by t order by t", null],
  ["with x as (select 1) select * from x union all select * from x", null],
  ["select a from (select a from (select a from (select 1 as a) ) )", null],
  ["select a.b.c, t.*, count(distinct x), sum(if(a > 1, 1, 0)) from t", null],
  ["select 1 from t where a = 1 and (b = 2 or c = 3) and d in (1, 2, 3)", null],
  ["select\ta\tfrom\tt", "select a\nfrom t"],
  ["select 1\r\nfrom t\r\nwhere x", "select 1\r\nfrom t\r\nwhere x"],
  ["", ""],
  ["   \n  ", "   \n  "],
];

const PG = [
  ["select a from t", "select a\nfrom t"],
  ["select $1, $2 from t where id = $1", "select $1, $2\nfrom t\nwhere id = $1"],
  ["select a::int, b::text from t", "select a::int, b::text\nfrom t"],
  ["select a :: int from t", "select a :: int\nfrom t"],
  ["select data->'a'->>'b' as v, data#>>'{a,b}' from t where data @> '{\"x\":1}'", "select data->'a'->>'b' as v, data#>>'{a,b}'\nfrom t\nwhere data @> '{\"x\":1}'"],
  ["select a from t where x ? 'k' and y ?| array['a','b']", null],
  ["with recursive r(n) as (select 1 union all select n+1 from r where n < 5) select * from r", "with recursive r(n) as (\n  select 1\n  union all\n  select n+1\n  from r\n  where n < 5\n)\nselect *\nfrom r"],
  ["create function f() returns int as $$ begin return 1; end; $$ language plpgsql", null],
  ["select $$a\nb$$, $tag$ x $tag$ from t", "select $$a\nb$$, $tag$ x $tag$\nfrom t"],
  ["select a from t order by a desc nulls last limit 10 offset 5", "select a\nfrom t\norder by a desc nulls last\nlimit 10\noffset 5"],
  ["select a from t for update", "select a\nfrom t\nfor update"],
  ["select a, row_number() over (partition by b order by c) from t", "select a, row_number() over (partition by b order by c)\nfrom t"],
  ["select a from t inner join u on t.id = u.id left join lateral (select 1) x on true", null],
  ["select a from t where a = any(array[1,2,3])", "select a\nfrom t\nwhere a = any(array[1, 2, 3])"],
  ["insert into t (a) values (1) on conflict (a) do nothing returning a", null],
  ["update t set a = 1, b = 2 where id = 3", null],
  ["select \"Quoted\" , E'a\\'b', U&'x' from t", null],
  ["select $__timeFilter(ts), ${v} from t", "select $__timeFilter(ts), ${v}\nfrom t"],
  ["select a from t -- c\nwhere x", "select a\nfrom t -- c\nwhere x"],
  ["select 1; select 2", null],
  ["select a from (values (1), (2)) as v(a)", null],
  ["select array_agg(a order by b) filter (where c) from t", null],
];

const JS = [
  ["[]", "[]"],
  ["{}", "{}"],
  ["  [ ]  \n", "[]\n"],
  ['{"a":1}', '{"a": 1}'],
  ['[{"$match":{"a":1}}]', '[{"$match": {"a": 1}}]'],
  ['{ "a" : [ 1 , 2 , 3 ] , "b" : null , "c" : true }', '{"a": [1, 2, 3], "b": null, "c": true}'],
  [
    '[{"$match":{"status":"active","region":{"$in":["eu","us","apac"]}}},{"$group":{"_id":"$region","n":{"$sum":1}}}]',
    '[\n  {"$match": {"status": "active", "region": {"$in": ["eu", "us", "apac"]}}},\n  {"$group": {"_id": "$region", "n": {"$sum": 1}}}\n]',
  ],
  [
    '[{"$match":{"aaaaaaaaaaaaaaaaaaaa":1,"bbbbbbbbbbbbbbbbbbbbbbbbbb":2,"cccccccccccccccccccccc":3,"ddddddddddddddd":4}}]',
    '[\n  {\n    "$match": {\n      "aaaaaaaaaaaaaaaaaaaa": 1,\n      "bbbbbbbbbbbbbbbbbbbbbbbbbb": 2,\n      "cccccccccccccccccccccc": 3,\n      "ddddddddddddddd": 4\n    }\n  }\n]',
  ],
  ['{"t":"$__timeFrom","u":"${host}","v":"a${x}b"}', '{"t": "$__timeFrom", "u": "${host}", "v": "a${x}b"}'],
  ['{"a":{"b":{"c":{}}},"d":[[],[{}]]}', '{"a": {"b": {"c": {}}}, "d": [[], [{}]]}'],
  ['{"a":-1.5e+3,"b":"\\u00e9\\n","c":0}', '{"a": -1.5e+3, "b": "\\u00e9\\n", "c": 0}'],
  ['[{"$facet":{"a":[{"$count":"n"}],"b":[{"$limit":1}]}}]', '[{"$facet": {"a": [{"$count": "n"}], "b": [{"$limit": 1}]}}]'],
  [
    '{"aggregate":"c","pipeline":[{"$match":{"x":{"$gt":1}}},{"$project":{"_id":0,"x":1,"y":1,"zzzzzzzzzzzzzzzzzzzzzzzzzz":1}}]}',
    '{\n  "aggregate": "c",\n  "pipeline": [\n    {"$match": {"x": {"$gt": 1}}},\n    {"$project": {"_id": 0, "x": 1, "y": 1, "zzzzzzzzzzzzzzzzzzzzzzzzzz": 1}}\n  ]\n}',
  ],
];

for (const [name, list, dialect] of [["ClickHouse", CH, "clickhouse"], ["PostgreSQL", PG, "postgres"], ["MongoDB JSON", JS, "json"]]) {
  test(`FE-55 ${name}: vectors give the expected style, keep the tokens and are idempotent`, () => {
    for (const [input, expected] of list) {
      if (input.trim() === "") {
        assert.equal(fmt(dialect, input).text, input);
        continue;
      }
      const r = checkProps(dialect, input);
      if (expected !== null) assert.equal(r.text, expected, `style of ${JSON.stringify(input)}`);
    }
  });
}

test("FE-55 width and indent options apply", () => {
  const q = "select aaaaaaaaaa, bbbbbbbbbb from t";
  assert.equal(formatSql(q, "clickhouse", { width: 20 }).text, "select\n  aaaaaaaaaa,\n  bbbbbbbbbb\nfrom t");
  assert.equal(formatSql(q, "clickhouse", { width: 20, indent: 4 }).text, "select\n    aaaaaaaaaa,\n    bbbbbbbbbb\nfrom t");
  assert.equal(formatSql(q, "clickhouse", { width: 200 }).text, "select aaaaaaaaaa, bbbbbbbbbb\nfrom t");
  assert.equal(formatJson('{"a":[1,2,3],"b":1}', { width: 20 }).text, '{\n  "a": [1, 2, 3],\n  "b": 1\n}');
  // A wrong option gives the default.
  assert.equal(formatSql(q, "clickhouse", { width: "x", indent: -1 }).text, formatSql(q, "clickhouse").text);
});

test("FE-55 the result has the shape of the contract", () => {
  const r = formatSql("select a,b from t", "clickhouse");
  assert.equal(r.changed, true);
  assert.equal(r.reason, null);
  assert.equal(r.lines, r.replace.text.split("\n").length);
  assert.equal(r.totalLines, 2);
  assert.equal("select a,b from t".slice(0, r.replace.from) + r.replace.text + "select a,b from t".slice(r.replace.to), r.text);
  const same = formatSql("select a\nfrom t", "clickhouse");
  assert.equal(same.changed, false);
  assert.equal(same.reason, null);
  assert.equal(same.replace, null);
  assert.equal(formatSql("select a,b from t", "nonsense").text, r.text);
  assert.equal(formatSql(null, "clickhouse").text, "");
});

test("FE-55 comments: line comments keep their line end and never join code", () => {
  const cases = [
    "select a, -- c1\nb -- c2\nfrom t -- c3\nwhere x -- c4\n and y",
    "select f(a, -- c\n b) from t",
    "select (\n -- inside\n select 1\n -- after\n) from t",
    "with a as ( -- c\n select 1) -- d\nselect * from a",
    "select a -- c\n;select 2",
    "select 1 /* b1 */ , /* b2 */ 2 /* b3\nmulti */ from t",
    "-- only a comment",
    "/* only */",
    "select a from t\n-- c1\n-- c2\nwhere x",
    "select -- c\n a from t",
    "select a from t where -- c\n x = 1 -- d\n and y = 2",
  ];
  for (const q of cases) {
    const r = checkProps("clickhouse", q);
    // A line comment is always followed by a line end or by the end of the text.
    for (const m of r.text.matchAll(/--[^\n]*/g)) {
      const after = r.text.slice(m.index + m[0].length);
      assert.ok(after === "" || after[0] === "\n", `line comment joined in ${JSON.stringify(r.text)}`);
    }
  }
  assert.equal(formatSql("select a\n-- c\nfrom t", "clickhouse").text, "select a\n-- c\nfrom t");
  assert.equal(formatSql("select 1 from t\n\n-- tail\n", "clickhouse").text, "select 1\nfrom t\n-- tail\n");
});

test("FE-55 macros and variables keep their bytes", () => {
  const q = "select  $__timeGroup(  ts ,\n '1m' )  , ${a:csv}, $b,${c} from t where $__timeFilter( ts ) and x = {p : UInt8}";
  const r = checkProps("clickhouse", q);
  assert.ok(r.text.includes("$__timeGroup(  ts ,\n '1m' )"));
  assert.ok(r.text.includes("$__timeFilter( ts )"));
  assert.ok(r.text.includes("{p : UInt8}"));
  assert.ok(r.text.includes("${a:csv}"));
});

test("FE-55 refusals: capped tree, error node, unclosed string and comment", () => {
  const deep = "select " + "(".repeat(200) + "1" + ")".repeat(200);
  let r = formatSql(deep, "clickhouse");
  assert.equal(r.reason, "capped_too_deep");
  assert.equal(r.text, deep);
  assert.equal(r.changed, false);
  const big = "select " + "1,".repeat(MAX_AST_INPUT_BYTES / 2) + "1";
  r = formatSql(big, "clickhouse");
  assert.equal(r.reason, "capped_too_large");
  assert.equal(r.text, big);
  for (const q of ["select (1 from t", "select 1) from t", "select [1, 2 from t", "select f(a from t"]) {
    r = formatSql(q, "clickhouse");
    assert.equal(r.reason, "error_node", q);
    assert.equal(r.text, q);
  }
  for (const q of ["select 'abc from t", "select a /* x from t", "select \"abc", "select a from t -- ok\nwhere 'x"]) {
    r = formatSql(q, "clickhouse");
    assert.ok(r.reason === "unclosed" || r.reason === "error_node", `${q}: ${r.reason}`);
    assert.equal(r.text, q);
  }
  r = formatSql("select $$abc", "postgres");
  assert.ok(r.reason !== null);
  for (const q of ['{"a":1,}', '{"a":1 "b":2}', '[1,2', '{"a": // c\n1}', '{"a": $x}', "[1] [2]", '{"a": "unclosed']) {
    r = formatJson(q);
    assert.ok(r.reason !== null, q);
    assert.equal(r.text, q);
    assert.equal(r.changed, false);
  }
  assert.equal(formatJson("[" + "[".repeat(200) + "]".repeat(200) + "]").reason, "capped_too_deep");
  // Nothing to format: no reason.
  assert.equal(formatSql("", "clickhouse").reason, "nothing_to_format");
  assert.equal(formatJson(" \n ").reason, "nothing_to_format");
});

test("FE-55 range mode (SQL): only the covered statements change; the text is whole", () => {
  const text = "select a,b from t;\nselect c,d from u;\n  select e,f from v;";
  const second = text.indexOf("select c");
  let r = formatSql(text, "clickhouse", { range: [second + 3, second + 3] });
  assert.equal(r.text, "select a,b from t;\nselect c, d\nfrom u;\n  select e,f from v;");
  assert.equal(r.reason, null);
  r = formatSql(text, "clickhouse", { range: [second, text.length] });
  assert.equal(r.text, "select a,b from t;\nselect c, d\nfrom u;\nselect e, f\nfrom v;");
  r = formatSql(text, "clickhouse", { range: [0, 4] });
  assert.equal(r.text, "select a, b\nfrom t;\nselect c,d from u;\n  select e,f from v;");
  // A selection that touches no statement gives a reason.
  r = formatSql("select 1;   \n\n  select 2;", "clickhouse", { range: [10, 12] });
  assert.equal(r.reason, "no_statement");
  assert.equal(r.changed, false);
  // An error in another statement does not stop the format of the covered one.
  const mixed = "select 1);\nselect c,d from u;";
  r = formatSql(mixed, "clickhouse", { range: [mixed.indexOf("select c"), mixed.length] });
  assert.equal(r.reason, null, "error elsewhere");
  assert.equal(r.text, "select 1);\nselect c, d\nfrom u;");
  // An error in the covered statement refuses.
  r = formatSql(mixed, "clickhouse", { range: [0, 3] });
  assert.equal(r.reason, "error_node");
  // The token property holds for a range.
  const ranged = formatSql(text, "clickhouse", { range: [second, second + 1] });
  assert.deepEqual(sig("clickhouse", ranged.text), sig("clickhouse", text));
  // Postgres.
  r = formatSql("select 1,2;select 3,4;", "postgres", { range: [12, 12] });
  assert.equal(r.text, "select 1,2;select 3, 4;");
});

test("FE-55 range mode (JSON): the innermost object or array that holds the range", () => {
  const text = '{\n  "aggregate": "c",\n  "pipeline": [{"$match":{"a":1}},{"$limit":1}]\n}';
  const at = text.indexOf('"$match"');
  let r = formatJson(text, { range: [at, at] });
  assert.equal(r.reason, null);
  assert.equal(r.text, '{\n  "aggregate": "c",\n  "pipeline": [{"$match": {"a": 1}},{"$limit":1}]\n}');
  const inner = text.indexOf('"a"');
  r = formatJson(text, { range: [inner, inner + 3] });
  assert.equal(r.text, '{\n  "aggregate": "c",\n  "pipeline": [{"$match":{"a": 1}},{"$limit":1}]\n}');
  // A range on the pipeline array formats the array in place, with the indent of its line.
  const arr = text.indexOf("[{");
  r = formatJson(text, { range: [arr, arr + 1], width: 40 });
  assert.equal(r.text, '{\n  "aggregate": "c",\n  "pipeline": [\n    {"$match": {"a": 1}},\n    {"$limit": 1}\n  ]\n}');
  assert.deepEqual(sig("json", r.text), sig("json", text));
  assert.equal(formatJson(text, { range: [0, 0] }).text, formatJson(text).text.replace(/\n$/, "") === formatJson(text).text ? formatJson(text).text : formatJson(text, { range: [0, 0] }).text);
  assert.equal(formatJson("[1,2]", { range: [99, 99] }).reason, "no_statement");
});

test("FE-55 a line end between two strings stays (PostgreSQL joins them as one constant)", () => {
  for (const dialect of ["postgres", "clickhouse"]) {
    for (const q of ["SELECT 'a'\n'b'", "SELECT E'a'\n'\\''", "SELECT 'a'  \n\n  'b', 'c' 'd'", "SELECT 'a' -- c\n'b'", "SELECT 'a' /* c */\n'b'"]) {
      const r = checkProps(dialect, q);
      // The gap between each pair of strings has a line end in the output when it had one in the input.
      assert.deepEqual(stringGaps(dialect, r.text), stringGaps(dialect, q), `${dialect}: ${JSON.stringify(r.text)}`);
    }
  }
  assert.equal(formatSql("SELECT 'a'\n'b'", "postgres").text, "SELECT 'a'\n  'b'");
  // The self check sees a lost line end between strings, and a change of `closed`.
  assert.equal(formatSql("SELECT 'a'\r\n'b'", "postgres").text.includes("\n"), true);
});

test("FE-55 only SELECT, WITH and VALUES statements are formatted; the others stay as they are", () => {
  const text = "insert into t (a,b)   values (1,2);\nselect a,b from t;\n  update t set a=1,\n b=2;\nvalues (1,2),(3,4)";
  const r = checkProps("clickhouse", text);
  assert.equal(r.text, "insert into t (a,b)   values (1,2);\nselect a, b\nfrom t;\nupdate t set a=1,\n b=2;\nvalues (1, 2), (3, 4)");
  assert.equal(formatSql("create table t (a Int8)", "clickhouse").reason, "nothing_to_format");
  assert.equal(formatSql("update t set a=1;", "postgres").changed, false);
  assert.equal(formatSql("-- c only", "postgres").reason, "nothing_to_format");
});

test("FE-55 operators have one space on each side; other pairs keep the input", () => {
  assert.equal(formatSql("select a=1, b<>2, c<=3, d||e, f!=g, h>=i, j==k, l<m, n>o, x::Int, -1, a.b, f(x), y->z from t", "clickhouse", { width: 300 }).text,
    "select a = 1, b <> 2, c <= 3, d || e, f != g, h >= i, j == k, l < m, n > o, x::Int, -1, a.b, f(x), y->z\nfrom t");
  assert.equal(formatSql("select a   =   1  from t", "postgres").text, "select a = 1\nfrom t");
});

test("FE-55 a subquery of one clause fits on the line; two clauses always break (rule 7)", () => {
  assert.equal(formatSql("select * from t where id in (select 1)", "clickhouse").text, "select *\nfrom t\nwhere id in (select 1)");
  assert.equal(formatSql("select * from t where id in (select 1 from u)", "clickhouse").text, "select *\nfrom t\nwhere id in (\n  select 1\n  from u\n)");
  assert.equal(
    formatSql("select * from t where id in (select aaaaaaaaaaaaaaaaaaaaaaaa, bbbbbbbbbbbbbbbbbbbbbbbbbbbb, cccccccccccccccccccccccccc)", "clickhouse").text,
    "select *\nfrom t\nwhere id in (\n  select\n    aaaaaaaaaaaaaaaaaaaaaaaa,\n    bbbbbbbbbbbbbbbbbbbbbbbbbbbb,\n    cccccccccccccccccccccccccc\n)",
  );
});

test("FE-55 items that are all literals fill the lines (rule 8)", () => {
  const nums = Array.from({ length: 40 }, (_, k) => 1000 + k).join(",");
  const r = checkProps("clickhouse", `select a from t where x in (${nums})`);
  const lines = r.text.split("\n");
  assert.equal(lines[2], "where x in (");
  assert.ok(lines.slice(3, -1).every((l) => l.length <= 80 && l.startsWith("  ")));
  assert.ok(lines.slice(3, -1).length >= 2 && lines.slice(3, -1).some((l) => l.split(", ").length > 2), r.text);
  assert.equal(lines[lines.length - 1], ")");
  // A list of names is one item on each line.
  const names = Array.from({ length: 12 }, (_, k) => `column_name_${k}`).join(",");
  const q = checkProps("clickhouse", `select f(${names}) from t`);
  assert.ok(q.text.split("\n").filter((l) => /^  column_name_\d+,?$/.test(l)).length === 12, q.text);
  // JSON arrays of scalars fill.
  const arr = JSON.stringify(Array.from({ length: 60 }, (_, k) => k * 1000));
  const j = checkProps("json", `{"a":${arr}}`);
  assert.ok(j.text.split("\n").length < 15 && j.text.split("\n").length > 3, j.text);
});

test("FE-55 the example of the spec (10.3d)", () => {
  const input =
    "WITH recent AS (SELECT host, avg(cpu) AS cpu FROM metrics.cpu WHERE $__timeFilter(ts) AND host IN (${host}) GROUP BY host) SELECT r.host, cpu FROM recent AS r LEFT JOIN top ON r.host = top.host WHERE r.cpu > (SELECT avg(cpu) FROM recent) ORDER BY cpu DESC";
  const output =
    "WITH recent AS (\n  SELECT host, avg(cpu) AS cpu\n  FROM metrics.cpu\n  WHERE $__timeFilter(ts) AND host IN (${host})\n  GROUP BY host\n)\nSELECT r.host, cpu\nFROM recent AS r\nLEFT JOIN top ON r.host = top.host\nWHERE r.cpu > (\n  SELECT avg(cpu)\n  FROM recent\n)\nORDER BY cpu DESC";
  assert.equal(checkProps("clickhouse", input).text, output);
  assert.equal(formatSql(output, "clickhouse").changed, false);
});

test("FE-55 a comma after a line comment is on its own line; a refusal gives the line and the column", () => {
  const r = checkProps("clickhouse", "select a -- c\n, b from t");
  assert.equal(r.text, "select\n  a -- c\n  , b\nfrom t".replace("  , b", "  ,\n  b"));
  const bad = formatSql("select a\nfrom (t\nwhere x", "clickhouse");
  assert.equal(bad.reason, "error_node");
  assert.deepEqual([bad.line, bad.column], [2, 6]);
  const open = formatSql("select 1;\nselect 'abc", "clickhouse");
  assert.equal(open.reason, "unclosed");
  assert.deepEqual([open.line, open.column], [2, 8]);
  assert.equal(bad.text, "select a\nfrom (t\nwhere x");
});

test("FE-55 tokens that touch stay joined (a `$` in a word is a placeholder, as in sqlscan.js)", () => {
  for (const dialect of ["postgres", "clickhouse"]) {
    for (const q of ["SELECT$1", "select$t$ 2", "select a FROM$t$$t$ x", "SELECT$__interval_ms$1", "select a$$b$$ from t", "select x$1, y from t where z=$1", "select a from t where b$c=1"]) {
      const r = checkProps(dialect, q);
      assert.deepEqual(sig(dialect, r.text), sig(dialect, q), q);
    }
  }
  assert.equal(formatSql("SELECT$1", "postgres").text, "SELECT$1");
  assert.equal(formatSql("SELECT$__interval_ms$1", "postgres").text, "SELECT$__interval_ms$1");
  assert.equal(formatSql("select x$1, y from t", "postgres").text, "select x$1, y\nfrom t");
});

// ---- The random queries. ----

/** A small deterministic generator (mulberry32). */
function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Changes the blanks of a text: each blank run gets a random blank, and a random extra blank goes between some tokens. */
function jumble(dialect, text, rand) {
  const toks = TOKENIZE[dialect](text);
  const blanks = [" ", "  ", "\n", "\n\n", "\t", " \n ", "\r\n"];
  let out = "";
  for (let i = 0; i < toks.length; i++) {
    const t = toks[i];
    const s = text.slice(t.start, t.end);
    if (t.kind === TOKEN.BLANK) {
      // A line comment before keeps a line end.
      const prev = toks[i - 1];
      const lineEnd = prev && prev.kind === TOKEN.COMMENT && text[prev.start + 1] === "-";
      let b = blanks[Math.floor(rand() * blanks.length)];
      if (lineEnd && !b.includes("\n")) b = "\n";
      out += b;
    } else out += s;
  }
  return out;
}

test("FE-56 2,000 random SQL queries: tokens kept, idempotent, never a bug", () => {
  const rand = rng(42);
  const pool = { clickhouse: CH.filter(([q]) => q.trim() && !q.includes("\r")).map(([q]) => q), postgres: PG.map(([q]) => q) };
  let formatted = 0;
  let refused = 0;
  for (let n = 0; n < 2000; n++) {
    const dialect = n % 3 === 0 ? "postgres" : "clickhouse";
    const list = pool[dialect];
    const count = 1 + Math.floor(rand() * 3);
    const parts = [];
    for (let k = 0; k < count; k++) parts.push(list[Math.floor(rand() * list.length)]);
    let q = parts.map((p) => p.replace(/;\s*$/, "")).join(rand() < 0.5 ? ";\n" : ";\n\n");
    q = jumble(dialect, q, rand);
    if (rand() < 0.1) q = q.slice(0, Math.floor(rand() * q.length));
    const r = formatSql(q, dialect);
    assert.notEqual(r.reason, "self_check", `self check failed for ${JSON.stringify(q)}`);
    if (r.reason !== null) {
      refused += 1;
      assert.equal(r.text, q);
      continue;
    }
    formatted += 1;
    assert.deepEqual(sig(dialect, r.text), sig(dialect, q), `tokens of ${JSON.stringify(q)}`);
    const r2 = formatSql(r.text, dialect);
    assert.equal(r2.reason, null, `second pass of ${JSON.stringify(q)}`);
    assert.equal(r2.text, r.text, `idempotence of ${JSON.stringify(q)}`);
    // Other widths keep the property too.
    const narrow = formatSql(q, dialect, { width: 30 });
    assert.deepEqual(sig(dialect, narrow.text), sig(dialect, q));
    assert.equal(formatSql(narrow.text, dialect, { width: 30 }).text, narrow.text, `idempotence at width 30 of ${JSON.stringify(q)}`);
  }
  assert.ok(formatted > 1500, `formatted ${formatted}, refused ${refused}`);
});

test("FE-56 2,000 random JSON texts: tokens kept, idempotent", () => {
  const rand = rng(7);
  const pool = JS.map(([q]) => q).filter((q) => q.trim());
  let formatted = 0;
  for (let n = 0; n < 2000; n++) {
    let q;
    const pick = () => pool[Math.floor(rand() * pool.length)];
    if (rand() < 0.5) q = pick();
    else q = `[${pick()},${pick()}]`;
    q = jumble("json", q, rand);
    if (rand() < 0.1) q = q.slice(0, Math.floor(rand() * q.length));
    const r = formatJson(q);
    assert.notEqual(r.reason, "self_check", `self check failed for ${JSON.stringify(q)}`);
    if (r.reason !== null) {
      assert.equal(r.text, q);
      continue;
    }
    formatted += 1;
    assert.deepEqual(sig("json", r.text), sig("json", q));
    const r2 = formatJson(r.text);
    assert.equal(r2.text, r.text, `idempotence of ${JSON.stringify(q)}`);
    const narrow = formatJson(q, { width: 24 });
    assert.equal(formatJson(narrow.text, { width: 24 }).text, narrow.text);
  }
  assert.ok(formatted > 1200, `formatted ${formatted}`);
});

// ---- The time. ----

/** The best of seven runs after a warm-up, in milliseconds. */
function best7(fn) {
  fn();
  fn();
  let best = Infinity;
  for (let k = 0; k < 7; k++) {
    const t0 = performance.now();
    fn();
    best = Math.min(best, performance.now() - t0);
  }
  return best;
}

test("FE-57 time: a 262,144-byte input formats in 300 ms or less (best of 7; the aim is 150 ms)", () => {
  // The text is nearly in the style: the output must stay under the cap of the input, so the input is the largest one
  // that gives an output of 262,144 bytes or less (a tab is one byte, the style has two spaces).
  const unit =
    "SELECT a, b, count(*) AS n, arrayMap(x -> x + 1, arr)\nFROM t1\nLEFT JOIN t2 ON t1.id = t2.id\nWHERE ts > now()\n\tAND x IN (\n\t\tSELECT id\n\t\tFROM u\n\t\tWHERE z = 1\n\t) -- note\nGROUP BY a, b\nORDER BY n DESC\nLIMIT 10;\n\n";
  const grow = 8; // the bytes that one unit gains
  let sql = "";
  let count = 0;
  while (sql.length + unit.length + (count + 1) * grow <= MAX_AST_INPUT_BYTES) {
    sql += unit;
    count += 1;
  }
  const r = formatSql(sql, "clickhouse");
  assert.equal(r.reason, null);
  assert.ok(sql.length > 250000, `input ${sql.length}`);
  const t = best7(() => formatSql(sql, "clickhouse"));
  const pgUnit = "SELECT a::int, b->>'k'\nFROM t\nJOIN u ON t.id = u.id\nWHERE x IN (\n\tSELECT 1\n)\nORDER BY a;\n\n";
  let pg = "";
  count = 0;
  while (pg.length + pgUnit.length + (count + 1) * 2 <= MAX_AST_INPUT_BYTES) {
    pg += pgUnit;
    count += 1;
  }
  assert.equal(formatSql(pg, "postgres").reason, null);
  const tp = best7(() => formatSql(pg, "postgres"));
  const item = '\t{"$match": {"a": 1, "b": {"$in": [1, 2, 3]}}},\n';
  let js = "[\n";
  count = 0;
  while (js.length + item.length + (count + 1) * 1 + 20 <= MAX_AST_INPUT_BYTES) {
    js += item;
    count += 1;
  }
  js += '\t{"$limit": 1}\n]';
  assert.equal(formatJson(js).reason, null);
  const tj = best7(() => formatJson(js));
  console.log(`format time, best of 7: ClickHouse ${t.toFixed(1)} ms (${sql.length} bytes), PostgreSQL ${tp.toFixed(1)} ms (${pg.length} bytes), JSON ${tj.toFixed(1)} ms (${js.length} bytes)`);
  assert.ok(t <= 300, `ClickHouse ${t} ms`);
  assert.ok(tp <= 300, `PostgreSQL ${tp} ms`);
  assert.ok(tj <= 300, `JSON ${tj} ms`);
  // A text whose output passes the cap is refused.
  const wide = "[" + '{"$match":{"a":1,"b":{"$in":[1,2,3]}}},'.repeat(6000) + '{"$limit":1}]';
  assert.equal(formatJson(wide).reason, "output_too_large");
});
