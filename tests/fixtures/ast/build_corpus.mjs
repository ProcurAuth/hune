// The builder of the shared corpus of the syntax tree tests (F42). It writes corpus_sql.json, corpus_json.json and hostile.json.
// Use: node tests/fixtures/ast/build_corpus.mjs
// The corpus holds the queries of the repository files: the field `sql` and the field `lines` of each JSON file, the fenced
// `sql` and `json` blocks of docs/, and the `.sql` files of tests/. A text that starts with `{` or `[` goes to the JSON corpus.
// The output is sorted and has no duplicate, so a second run gives the same bytes. The hostile cases are written here as data.
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..", "..", "..");
const SKIP_DIRS = new Set(["target", "node_modules", ".git", "vendor", "fuzz", "build", "ast", "vectors"]);
const MAX_TEXT = 100000;

/** Lists the files below a directory, sorted, with the skipped directories left out. */
function files(dir, out = []) {
  for (const name of readdirSync(dir).sort()) {
    const path = join(dir, name);
    if (SKIP_DIRS.has(name)) continue;
    if (statSync(path).isDirectory()) files(path, out);
    else out.push(path);
  }
  return out;
}

/** Collects the values of the members `sql` and `lines` of a parsed JSON value. */
function collect(value, source, out) {
  if (Array.isArray(value)) {
    for (const v of value) collect(v, source, out);
  } else if (value && typeof value === "object") {
    for (const [key, v] of Object.entries(value)) {
      if ((key === "sql" || key === "lines") && Array.isArray(v) && v.length && v.every((x) => typeof x === "string")) out.push({ source, text: v.join("\n") });
      else if (key === "sql" && typeof v === "string") out.push({ source, text: v });
      else collect(v, source, out);
    }
  }
}

const found = [];
for (const path of files(join(ROOT, "tests"))) {
  const rel = relative(ROOT, path);
  if (rel.endsWith(".json")) {
    let value;
    try {
      value = JSON.parse(readFileSync(path, "utf8"));
    } catch {
      continue;
    }
    collect(value, rel, found);
  } else if (rel.endsWith(".sql")) {
    found.push({ source: rel, text: readFileSync(path, "utf8") });
  }
}
// The vectors of the lexers have the field `sql` as a string: `collect` reads them. The blocks of the documents follow.
for (const path of files(join(ROOT, "docs"))) {
  if (!path.endsWith(".md")) continue;
  const rel = relative(ROOT, path);
  const text = readFileSync(path, "utf8");
  for (const m of text.matchAll(/^```(sql|json)[^\n]*\n([\s\S]*?)^```/gm)) found.push({ source: `${rel}#${m[1]}`, text: m[2].replace(/\n$/, "") });
}

const seen = new Set();
const sql = [];
const json = [];
for (const item of found.sort((a, b) => (a.source < b.source ? -1 : a.source > b.source ? 1 : a.text < b.text ? -1 : a.text > b.text ? 1 : 0))) {
  if (item.text.trim() === "" || item.text.length > MAX_TEXT || seen.has(item.text)) continue;
  seen.add(item.text);
  (/^\s*[{[]/.test(item.text) ? json : sql).push(item);
}
const dump = (name, description, entries) => writeFileSync(join(HERE, name), `${JSON.stringify({ description, entries }, null, 1)}\n`);
dump("corpus_sql.json", "The SQL texts of the repository files. Built by build_corpus.mjs.", sql);
dump("corpus_json.json", "The JSON texts (pipelines of MongoDB) of the repository files. Built by build_corpus.mjs.", json);

// ---- The hostile cases. `parts` is a list of [text, times]. `dialects` is absent for all parsers. ----
const H = [];
const add = (name, ...parts) => H.push({ name, parts: parts.map((p) => (typeof p === "string" ? [p, 1] : p)) });

// Empty and blank texts.
add("empty", "");
add("blank", " ");
add("newline", "\n");
add("crlf_only", "\r\n");
add("tabs", "\t\t\t");
// Lone and mixed brackets.
for (const b of ["(", ")", "[", "]", "{", "}", ")(", "][", "}{", "()", "[]", "{}", ")))", "(((", "]]]", "[[[", "}}}", "{{{"]) add(`lone_${b.replace(/\W/g, (c) => `u${c.charCodeAt(0)}`)}`, b);
for (const b of ["(]", "[)", "{)", "([)]", "(((]]]", "{[(}])", "(}", "[}", ")(][}{", "([{)]}", "SELECT (1]", "SELECT [1)", "SELECT {1)", "SELECT ((1) + [2)]", "[{(}])"]) add(`mixed_${H.length}`, b);
// Deep nesting at and around the cap of the depth (128).
for (const d of [1, 127, 128, 129, 200, 1000]) {
  add(`deep_paren_${d}`, ["(", d], ["1", 1], [")", d]);
  add(`deep_array_${d}`, ["[", d], ["]", d]);
  add(`deep_brace_${d}`, ["{", d], ["}", d]);
  add(`deep_mixed_${d}`, ["([{", d], ["}])", d]);
  add(`deep_unclosed_paren_${d}`, ["(", d]);
  add(`deep_select_${d}`, ["SELECT * FROM (", d], ["SELECT 1", 1], [")", d]);
  add(`deep_json_array_${d}`, ["[", d], ["]", d]);
  add(`deep_json_object_${d}`, ['{"a":', d], ["1", 1], ["}", d]);
  add(`deep_with_${d}`, ["WITH a AS (", d], ["SELECT 1", 1], [")", d]);
  add(`deep_call_${d}`, ["f(", d], ["x", 1], [")", d]);
  add(`deep_macro_${d}`, ["$__timeFilter(", d], ["x", 1], [")", d]);
}
// Unclosed strings, names, comments and dollar quotes.
for (const s of ["'", "'abc", "\"", "\"abc", "`", "`abc", "$$", "$$abc", "$a$ abc", "'a\\", "E'\\", "'a''", "'\\'", "/*", "/* /* */", "/* /*", "--", "-- x\r", "-- x", "#", "# x", "#!", "$1", "$$$$", "$$ $$ $$", "'a' 'b", "'; DROP", "\"; --"]) add(`unclosed_${H.length}`, s);
add("unclosed_in_call", "SELECT f('abc, (1");
add("unclosed_comment_in_paren", "SELECT (1 /* x");
add("line_comment_before_close", "SELECT (1 -- x\n)");
add("line_comment_at_end_of_paren", "SELECT (1 -- x)");
add("comment_between_clauses", "SELECT 1 /* a */ FROM /* b */ t -- c\nWHERE /* d */ 1");
// Control characters and unusual spaces.
add("nul", "\u0000");
add("nul_in_select", "SELECT\u0000 1");
add("controls", "\u0001\u0002\u0003\u001b[31m\u007f");
add("controls_in_string", "SELECT '\u0000\u0001'");
add("separators", "SELECT  1  FROM t");
add("nel_and_nbsp", "SELECT\u0085 1 FROM t");
add("bom", "﻿SELECT 1");
add("zero_width", "SEL​ECT 1");
add("bidi", "SELECT '‮ abc ‬'");
// Astral characters and lone surrogates.
add("astral_literal", "SELECT '😀' AS \"𝒳\", `😀`");
add("astral_word", "SELECT 😀 FROM 𝒳");
add("astral_comment", "/* 😀 */ SELECT 1 -- 😀");
add("lone_high", "\ud800");
add("lone_low", "\udc00");
add("lone_high_paren", "SELECT (\ud800)");
add("lone_low_in_string", "SELECT '\udc00'");
add("pair_cut_by_quote", "'\ud800'\udc00");
add("astral_json", '{"a": "😀", "😀": [1, 2]}');
add("lone_in_json_string", '{"a": "\ud800"}');
add("astral_many", ["😀", 1000]);
// CRLF and bare CR.
add("crlf_query", "SELECT 1\r\nFROM t\r\n-- c\r\nWHERE a = 1\r\n");
add("crlf_subquery", "SELECT (\r\nSELECT 1\r\n)\r\n");
add("bare_cr", "SELECT 1\rFROM t\r-- c\rWHERE a");
add("mixed_eol", "a\r\n\r\r\n\nb");
add("crlf_json", '{\r\n "a": [\r\n  1,\r\n  2\r\n ]\r\n}');
add("crlf_line_comment", "SELECT 1 -- c\r\n, 2");
// Macros and variables, complete and cut.
for (const m of ["$", "$$$", "${", "${a", "${a:", "${a:csv", "${a:csv}", "${}", "${:}", "$__", "$__(", "$__a(", "$__timeFilter(", "$__timeFilter(ts", "$__timeFilter(ts))", "${__interval}", "$__interval_ms", "$a$b", "$1", "${1}", "$ a", "${a b}", "${a:b:c}", "$__timeFilter(\"x\")", "$__timeFilter('a)')", "$__timeFilter(/* ) */ x)", "$__timeFilter(-- )\nx)"]) add(`macro_${H.length}`, `SELECT ${m} FROM t`);
add("macro_in_string", "SELECT '$__timeFilter(a)', \"${v}\"");
add("macro_in_comment", "SELECT 1 /* $__timeFilter(a) */ -- ${v}");
add("macro_json_value", '{"a": "$__timeFrom", "b": "${v:json}", "c": "$v"}');
add("macro_json_bare", '{"a": $__timeFrom, "b": ${v}, "c": $v}');
add("macro_json_key", '{"$v": 1, "${v}": 2}');
// Statements and clauses in odd places.
for (const q of [";", ";;;", "; ;", "SELECT 1;;SELECT 2", "SELECT", "SELECT FROM", "FROM", "WITH", "WITH a", "WITH a AS", "WITH a AS (", "WITH a AS ()", "WITH RECURSIVE a AS (SELECT 1) SELECT * FROM a", "SELECT * FROM (", "SELECT * FROM ()", "SELECT * FROM (SELECT", "UNION ALL", "SELECT 1 UNION ALL", "ORDER BY", "GROUP BY GROUP BY", "ARRAY JOIN", "SELECT x -> y", "SELECT (x, y) -> x", "SELECT a::int::text", "SELECT a::", "SELECT 'a'::", "SELECT a->>'b'", "SELECT a#>>'{b}'", "SELECT $$a$$", "SELECT $t$a$$b$t$", "SELECT ARRAY[1,2][1]", "SELECT x FORMAT JSON", "SETTINGS a=1", "SELECT 1 SETTINGS", "SELECT 1 FORMAT", "SELECT a FROM t PREWHERE", "SELECT a FROM t LEFT ASOF JOIN", "SELECT 1 WINDOW w AS (PARTITION BY a)"]) add(`odd_${H.length}`, q);
// JSON-only hostile texts.
for (const j of ['{"a":', '{"a" 1}', "[1,]", "{,}", "[,,]", '{"a":1,,}', '"\\u12', '"\\ud800"', "nul", "tru", "1e", "-", "0123", "01", "1.", ".5", "+1", "NaN", "Infinity", "-Infinity", "/* c */ {}", "// c\n{}", '{"a":1} {"b":2}', '{"a":"\n"}', '{"a":"\t"}', '{"a":"\\x"}', '{"a":}', '{:1}', '{"a":1:}', '{1:1}', "{'a':1}", "[1 2]", '["a" "b"]', "[}", "{]", "}", "]", ",", ":", '{"a":{"b":{"c":', '{"a":[{"b":[{"c":[', '"', '"abc', "[[[", "]]]", '{"a":1}}', '[1]]', '{"$match":{"a":{"$in":[1,2,3]}}}']) add(`json_${H.length}`, j);
add("json_long_string", ['{"a": "', 1], ["x", 50000], ['"}', 1]);
add("json_many_members", ['{"a":1,', 1], ['"b":2,', 5000], ['"c":3}', 1]);
add("json_many_keys", ["{", 1], ['"k": 1, ', 1000], ['"z": 2}', 1]);
add("json_pipeline_text", '{"collection":"c","aggregate":[{"$match":{"$expr":{"$and":[{"$gte":["$ts","$__timeFrom"]},{"$lt":["$ts","$__timeTo"]}]}}},{"$group":{"_id":"$h","n":{"$sum":1}}}]}');
// The cap of the input: 262,144 bytes in UTF-8 is the limit, and the next byte is over it.
add("cap_a_exact", ["a", 262144]);
add("cap_a_over", ["a", 262145]);
add("cap_numbers_exact", ["1,", 131072]);
add("cap_numbers_over", ["1,", 131072], ["1", 1]);
add("cap_open_paren", ["(", 262144]);
add("cap_close_paren", [")", 262144]);
add("cap_open_brace", ["{", 262144]);
add("cap_open_bracket", ["[", 262144]);
add("cap_quote", ["'", 262144]);
add("cap_comment_open", ["/*", 131072]);
add("cap_comment_pairs", ["/**/", 65536]);
add("cap_line_comments", ["--\n", 87381]);
add("cap_union", ["SELECT 1 UNION ALL ", 13797], ["SELECT 1", 1]);
add("cap_select_columns", ["SELECT ", 1], ["a, ", 87000], ["b", 1]);
add("cap_calls", ["f(", 32768], ["1", 1], [")", 32768]);
add("cap_flat_calls", ["f(1),", 52428]);
add("cap_macros", ["$__timeFilter(a)+", 15420]);
add("cap_variables", ["${v:csv},", 29127]);
add("cap_cte_list", ["WITH a AS (SELECT 1),", 1], ["b AS (SELECT 1),", 16000], ["c AS (SELECT 1) SELECT 1", 1]);
add("cap_subqueries", ["SELECT (SELECT 1),", 14563]);
add("cap_two_byte_exact", ["é", 131072]);
add("cap_two_byte_over", ["é", 131073]);
add("cap_three_byte_in", ["€", 87381]);
add("cap_three_byte_over", ["€", 87382]);
add("cap_four_byte_exact", ["😀", 65536]);
add("cap_four_byte_over", ["😀", 65537]);
add("cap_lone_high", ["\ud800", 87382]);
add("cap_control", ["\u0001", 262144]);
add("cap_crlf", ["a\r\n", 87381]);
add("cap_json_array", ["[", 1], ["1,", 131070], ["1]", 1]);
add("cap_json_nested", ["[", 100000], ["]", 100000]);
add("cap_json_members", ["{", 1], ['"a":1,', 43690], ['"b":2}', 1]);
add("cap_json_string", ['"', 1], ["x", 262142], ['"', 1]);
add("cap_json_nulls", ["[", 1], ["null,", 52000], ["null]", 1]);

writeFileSync(join(HERE, "hostile.json"), `${JSON.stringify({ description: "The hostile cases of the syntax tree tests. A part is [text, times]. Built by build_corpus.mjs.", cases: H }, null, 1)}\n`);
console.log(`corpus_sql ${sql.length}, corpus_json ${json.length}, hostile ${H.length}`);
