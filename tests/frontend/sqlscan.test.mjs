// SPDX-License-Identifier: MIT OR Apache-2.0
// Golden cases of the placeholder scan (9.10, 7.6).
import { test } from "node:test";
import assert from "node:assert/strict";
import { findMacros, findVariableNames, sqlText, variableNamesOfLines } from "../../web/js/sqlscan.js";

const cases = [
  ["code, $name form", "SELECT * FROM t WHERE a = $env", ["env"]],
  ["braces form", "WHERE a IN (${ids})", ["ids"]],
  ["format csv", "WHERE a IN (${ids:csv})", ["ids"]],
  ["format singlequote", "x ${a:singlequote} y ${b:sqlstring} z ${c:raw}", ["a", "b", "c"]],
  ["unknown format is not read", "x ${a:date} ${b:join} ${c:uriencode} ${d:CSV}", []],
  ["formats of a string", "match(x, '${a:regex}') AND y = '${b:json}' AND '${c:pipe}${d:glob}${e:lucene}'", ["a", "b", "c", "d", "e"]],
  [
    "every format that is valid only in a string",
    "'${a:percentencode}${b:queryparam}${c:distributed}${d:doublequote}${e:html}${f:text}'",
    ["a", "b", "c", "d", "e", "f"],
  ],
  ["a format in a comment is not read", "SELECT 1 -- ${a:regex}\n/* ${b:json} */", []],
  ["the second argument of conditionalAll is a variable", "WHERE $__conditionalAll(env IN ($env), $env)", ["env"]],
  [
    "the new macros and built-ins are not variables",
    "SELECT $__from, $__to, $__interval_ms, $__fromTime_ms, $__timeFilter_ms(t), $__timeInterval_ms(t), $__dateFilter(d), $__dateTimeFilter(d, t) $v",
    ["v"],
  ],
  ["in a string", "WHERE a = '$env' AND b = 'x${y}z'", ["env", "y"]],
  ["in a quoted name", 'SELECT "col_$suffix" FROM `t_${tbl}`', ["suffix", "tbl"]],
  ["line comment ignored", "SELECT 1 -- $hidden\nFROM t WHERE a = $shown", ["shown"]],
  // ClickHouse 26.8 (the image of tests/images.lock) reads "//" as a line comment (7.5, engine edit E47). The quote marks in the two comments do
  // not make a string, so the query parameter between them is code, as ClickHouse reads it.
  ["slash comments hide their quote marks", "SELECT 1 // it's\n, {y:String} AS b // that's", ["y"]],
  ["block comment ignored", "SELECT /* $hidden ${also} */ $shown", ["shown"]],
  ["longest run", "WHERE a = $env_name", ["env_name"]],
  ["braces cut the run", "WHERE a = '${env}_name'", ["env"]],
  ["double bracket is text", "WHERE a = [[x]]", []],
  ["macros are not variables", "WHERE $__timeFilter(t) AND a = $v AND b > $__fromTime", ["v"]],
  ["first-use order, no duplicate", "$b $a $b ${a}", ["b", "a"]],
  // The page lexer of ClickHouse reads "# " as a comment. The lexer of PostgreSQL reads it as an operator (7.20).
  // The scan keeps the union of the two, so the name is found (the name scan of 7.19).
  ["hash is an operator for postgres", "SELECT 1 # $x\n, $y", ["x", "y"]],
  ["hash operator, no blank", "SELECT a #$x, b #> $y", ["x", "y"]],
  ["lone dollar", "SELECT '$' || $ || 1", []],
  // The query parameters of ClickHouse (7.6a, F39): a use of the variable in code only.
  ["query parameters are uses", "SELECT {a:String}, '{b:String}', ${c:csv} -- {d:Int64}\n, x{e:Array(UInt64)}", ["a", "c", "e"]],
  ["not a query parameter", "SELECT {1:2}, {'k': v}, ${x:String}, {__y:String}, `{z:String}`", []],
];

for (const [name, sql, expected] of cases) {
  test(`findVariableNames: ${name}`, () => {
    assert.deepEqual(findVariableNames(sql), expected);
  });
}

const postgresCases = [
  ["E string with an escaped quote", "SELECT E'a\\' $x' , $y", ["x", "y"]],
  ["backslash in a standard string", "SELECT '\\' || $x || '\\'", ["x"]],
  // The name "q" is an extra name that the page lexer of ClickHouse reads in the tag. An extra name is allowed (7.19).
  ["dollar quote", "SELECT $q$ text $x $q$, $y", ["q", "x", "y"]],
  ["empty tag dollar quote", "SELECT $$ it's $x $$, $y", ["x", "y"]],
  ["nested block comment hides a name", "SELECT /* a /* b */ $x */ $y", ["x", "y"]],
  ["prefixed string", "SELECT B'01' || U&'$x', $y", ["x", "y"]],
  ["U& name", 'SELECT U&"col_$x", $y', ["x", "y"]],
  ["a braced placeholder of code", "WHERE a = ANY(${ids:csv}) AND b ~ ${re}", ["ids", "re"]],
  ["a parameter is not a name for the run, the scan may keep it", "SELECT $1, $v", ["1", "v"]],
  ["a line comment hides a name in both", "SELECT 1 -- $x\n, $y", ["y"]],
  ["string continuation", "SELECT 'a'\n'$x', $y", ["x", "y"]],
  ["unclosed string reads to the end", "SELECT 'a $x", ["x"]],
];

for (const [name, sql, expected] of postgresCases) {
  test(`findVariableNames, postgres text: ${name}`, () => {
    assert.deepEqual(findVariableNames(sql), expected);
  });
}

// The JSON reader of MongoDB query text (7.19, 7.34). The cases are JavaScript strings of JSON text.
// The scan also finds the name of an operator such as `$in`: the page keeps only the names of the variables of the document.
const mongoCases = [
  ["whole-leaf placeholder in a string", '{"find":"node_load","filter":{"node":"$host"}}', ["host"]],
  ["braced form", '{"filter":{"node":"${host}"}}', ["host"]],
  ["format of a whole-leaf placeholder", '{"filter":{"n":"${n:number}","t":"${t:date}","i":"${i:oid}","b":"${b:bool}"}}', ["n", "t", "i", "b"]],
  ["formats of lists", '{"$in":"${a:json}","$nin":"${b:json_number}","$eq":"${c:json_oid}"}', ["in", "a", "nin", "b", "eq", "c"]],
  ["embedded placeholder", '{"$regex":"^${host:regex}$","$options":"i"}', ["regex", "host", "options"]],
  ["unknown format is not read", '{"a":"${x:nope}","b":"${y:singlequoted}"}', []],
  ["the format number is not read in a SQL text", "SELECT ${x:number}, ${y:oid}", []],
  ["a placeholder in a key is found, the server refuses it", '{"$match":{"$k":1,"${key}":2}}', ["match", "k", "key"]],
  ["a key placeholder of the simple form", '{"$match":{"$col":"a"}}', ["match", "col"]],
  ["a unicode escape gives the dollar", '{"a":"\\u0024esc","b":"\\u0024\\u007bbraced:json}"}', ["esc", "braced"]],
  ["an escaped quote does not end the string", '{"a":"x\\" $one \\"y $two","b":"$three"}', ["one", "two", "three"]],
  ["an escaped backslash ends before the quote", '{"a":"\\\\","b":"$name"}', ["name"]],
  ["nested arrays and objects", '{"pipeline":[[{"$match":{"x":["$a",["${b:json}"]]}}],{"y":{"z":["$c"]}}]}', ["match", "a", "b", "c"]],
  ["a text outside a string is not a JSON placeholder", '{"a":1,"b":[1,2],"c":true}', []],
  ["first-use order and no duplicate", '{"a":"$b","c":["${a}","$b"]}', ["b", "a"]],
  ["an invalid escape keeps the text as written", '{"a":"\\q $kept"}', ["kept"]],
  ["a string with no end reads to the end", '{"a":"$open', ["open"]],
  ["a macro is not a variable", '{"t":{"$gte":"$__timeFrom"},"h":"$host"}', ["gte", "host"]],
];

for (const [name, text, expected] of mongoCases) {
  test(`findVariableNames, MongoDB text: ${name}`, () => {
    assert.deepEqual(findVariableNames(text), expected);
  });
}

test("findMacros finds the macros of a MongoDB text, with the escapes decoded", () => {
  assert.deepEqual(findMacros('{"a":"$__timeFrom","b":"${__interval_s}","c":"\\u0024__to","d":"$v"}'), ["__timeFrom", "__interval_s", "__to"]);
});

test("the union of the readers finds at least the names of each reader", () => {
  // A JSON text read as SQL (quoted names) gives the same names as the JSON reader for a plain text.
  assert.deepEqual(findVariableNames('{"a":"$x"} -- "$y"'), ["x", "y"]);
});

test("a macro after a hash operator is found", () => {
  assert.deepEqual(findMacros("SELECT a #> $__from, $__to"), ["__from", "__to"]);
});

test("findMacros lists the new macros and built-in variables", () => {
  assert.deepEqual(
    findMacros("$__from $__to ${__interval_ms} $__timeFilter_ms(t) $__conditionalAll(a, $v) $__dateFilter(d)"),
    ["__from", "__to", "__interval_ms", "__timeFilter_ms", "__conditionalAll", "__dateFilter"],
  );
});

test("findMacros lists the macros apart, in first-use order", () => {
  assert.deepEqual(findMacros("WHERE $__timeFilter(t) AND $v AND ${__interval_s} AND $__timeFilter(u)"), ["__timeFilter", "__interval_s"]);
  assert.deepEqual(findMacros("SELECT /* $__x */ 1"), []);
});

test("sqlText joins the lines with one line feed", () => {
  assert.equal(sqlText(["SELECT 1", "FROM t"]), "SELECT 1\nFROM t");
  assert.equal(sqlText(["only"]), "only");
});

test("variableNamesOfLines scans the joined text", () => {
  assert.deepEqual(variableNamesOfLines(["SELECT 1 -- $a", "FROM t WHERE x = $b", "  AND y = '${c}'"]), ["b", "c"]);
});

test("a comment that ends at the line break does not hide the next line", () => {
  assert.deepEqual(variableNamesOfLines(["-- $a", "$b"]), ["b"]);
});

test("the page lexer reads // as a line comment that ends at the line feed only (7.5, engine edit E47)", async () => {
  const { sqlLex } = await import("../../web/js/vendor/chdash/app_sql_lex.js");
  const text = "a // b'\r, c\nd / e /// f";
  assert.deepEqual(
    sqlLex.spans(text).map((s) => [s.kind, s.start, s.end]),
    [
      [sqlLex.LINE, 2, 12],
      [sqlLex.LINE, 18, 23],
    ],
  );
  // One "/" is code, and "/*" is still a block comment.
  assert.deepEqual(sqlLex.spans("1 / 2 /* x */").map((s) => s.kind), [sqlLex.BLOCK]);
});
