// SPDX-License-Identifier: MIT OR Apache-2.0
// Generator of the golden files tests/golden/names/*.json (7.19, 6.14).
//
// The files hold the names that the CURRENT scan of the page (web/js/sqlscan.js, `variableNamesOfLines`) finds for
// each query text and each document of the repository. The Rust test crates/hune-core/tests/names_golden.rs checks
// that the server scan names every one of them. A name that the page does not find and the server finds is an
// extra name: it needs a reason in tests/golden/names/extra.json (this script never writes that file).
//
// Use: node tests/tools/golden_names.mjs            write the files
//      node tests/tools/golden_names.mjs --check    exit 1 when a file differs from the generated text
//
// Run it from any folder. It reads only files of the repository and it needs no network.

import { readdirSync, readFileSync, statSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const OUT = join(ROOT, "tests", "golden", "names");
const { variableNamesOfLines } = await import(pathToFileURL(join(ROOT, "web", "js", "sqlscan.js")).href);

// ---- the corpus: files of the repository ----

// The folders that hold query texts and documents. The golden files of this generator are not an input.
const FOLDERS = ["tests/fixtures", "tests/golden", "tests/frontend/vectors", "tests/conformance", "tools/parity"];
// The corpus of the editor syntax tree (tests/fixtures/ast) is not an input either: it holds synthetic tokenizer texts that change
// with the editor work, and its own tests compare its names with sqlscan.js (F42).
const SKIP = [join("tests", "golden", "names"), join("tests", "fixtures", "ast")];

function walk(dir, out = []) {
  for (const name of readdirSync(dir).sort()) {
    const path = join(dir, name);
    const rel = relative(ROOT, path);
    if (SKIP.some((s) => rel === s || rel.startsWith(s + "/"))) continue;
    if (statSync(path).isDirectory()) walk(path, out);
    else out.push(rel);
  }
  return out;
}

const files = FOLDERS.flatMap((f) => walk(join(ROOT, f)));
const texts = new Map(); // text -> id of its first source
const add = (id, text) => {
  if (typeof text === "string" && text.includes("$") && !texts.has(text)) texts.set(text, id);
};

// Each string with a "$", each array of strings joined by one line feed (4.16), and the JSON text of each value that
// holds a MongoDB query, compact and indented.
function collect(id, value) {
  if (typeof value === "string") {
    add(id, value);
  } else if (Array.isArray(value)) {
    if (value.length > 0 && value.every((x) => typeof x === "string")) {
      add(id, value.join("\n"));
      if (!["lines", "sql"].includes(id.split("/").pop())) value.forEach((x, i) => collect(`${id}/${i}`, x));
    } else {
      value.forEach((x, i) => collect(`${id}/${i}`, x));
    }
  } else if (value && typeof value === "object") {
    for (const [k, x] of Object.entries(value)) {
      if (NOT_A_QUERY.has(k)) continue;
      if (typeof x === "object" && x !== null && ["aggregate", "pipeline", "query", "find"].includes(k)) {
        add(`${id}/${k}#compact`, JSON.stringify(x));
        add(`${id}/${k}#indent`, JSON.stringify(x, null, 2));
      }
      collect(`${id}/${k}`, x);
    }
  }
}

// The members of the vectors that name a case or hold an expected result: they are not a query text.
const NOT_A_QUERY = new Set(["id", "name", "note", "description", "tags", "title", "expect", "expected", "reason", "grafana", "plugin", "capture", "sent_sql"]);

const documents = [];
function findDocuments(file, value, pointer) {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    if (value.format_version === 3 && Array.isArray(value.sections) && Array.isArray(value.variables)) {
      documents.push({ file, pointer, value });
      return;
    }
    for (const [k, x] of Object.entries(value)) findDocuments(file, x, `${pointer}/${k}`);
  } else if (Array.isArray(value)) {
    value.forEach((x, i) => findDocuments(file, x, `${pointer}/${i}`));
  }
}

for (const rel of files) {
  const raw = readFileSync(join(ROOT, rel), "utf8");
  if (rel.endsWith(".json")) {
    let value;
    try {
      value = JSON.parse(raw);
    } catch {
      continue; // a vector that is invalid JSON on purpose
    }
    collect(rel.replace(/^tests\/(golden\/|fixtures\/)?/, ""), value);
    if (!rel.includes("/invalid/")) findDocuments(rel, value, "");
  } else if (rel.endsWith(".sql") || rel.endsWith(".js")) {
    add(rel.replace(/^tests\/(golden\/|fixtures\/)?/, ""), raw);
  }
}

// ---- the curated cases: one for each family of names ----

// [note, text]
const CURATED = [
  ["plain", "SELECT $a, ${b}, ${c:csv}, $d_1 FROM t"],
  ["longest run", "$env_name ${env}_name $a$b $a-$b"],
  ["macros", "SELECT $__timeInterval(ts), $__timeFilter(ts), ${__from}, $__interval_ms, $x WHERE $__timeFilter_ms(ts)"],
  ["every format of 7.9", "SELECT ${a:csv},${b:singlequote},${c:sqlstring},${d:raw},${e:json},${f:pipe},${g:regex},${h:glob},${i:lucene},${j:percentencode},${k:queryparam},${l:distributed},${m:doublequote},${n:html},${o:text}"],
  ["formats of 7.34", "{\"a\": \"${x:number}\", \"b\": \"${y:date}\", \"c\": \"${z:oid}\", \"d\": \"${w:bool}\", \"e\": \"${v:json_number}\", \"f\": \"${u:json_oid}\"}"],
  ["unknown format", "SELECT ${a:nope}, ${b:csv}, ${c:CSV}, ${d:}, ${e: csv}"],
  ["unclosed braces", "SELECT ${a, ${b:csv, ${c} $"],
  ["single quote string", "SELECT '$a', '${b}', '${c:csv}', 'x$d y'"],
  ["double quote name", "SELECT \"$a\", \"${b}\", \"x$c\""],
  ["backtick name", "SELECT `$a`, `${b}`"],
  ["escape in clickhouse string", "select '\\'$a', '\\\\' || $b || '\\\\', '\\$c'"],
  ["line comment", "SELECT 1 -- $a\n, $b"],
  ["hash comment", "SELECT 1 # $a\n, $b\n#! $c\n, $d"],
  ["block comment", "SELECT 1 /* $a */, $b /* $c /* $d */ $e */, $f"],
  ["open block comment", "SELECT $a /* $b"],
  ["heredoc clickhouse", "SELECT $$ $a $$, $tag$ $b $tag$, $c"],
  ["postgres dollar quote", "DO $$ BEGIN PERFORM $a; END $$; SELECT $b"],
  ["postgres tagged dollar quote", "SELECT $fn$ $a $fn$, $b, $fn2$ $c"],
  ["postgres function", "CREATE FUNCTION f() RETURNS int AS $body$ SELECT $x $body$ LANGUAGE sql; SELECT $y"],
  ["postgres E string", "select E'\\' $a', E'\\\\' || $b, E'it\\'s $c'"],
  ["postgres string with backslash", "select '\\' || $a || '\\'"],
  ["postgres prefixed strings", "select B'$a', X'$b', N'$c', U&'$d', U&\"$e\", $f"],
  ["postgres doubled quote", "select 'it''s $a', \"q\"\"$b\", $c"],
  ["postgres continued string", "select 'a'\n'$b'\n  -- c\n  '$d'"],
  ["postgres parameter", "select $1, $22, $1x$a$U$a$ , $b"],
  ["postgres word with dollar", "select a$b, a$$c, x$1"],
  ["postgres cast and operators", "select a::int, b #> '{x}', c || $d, e @> $f, g ~ '$h'"],
  ["postgres no hash comment", "select 1 # $a\n"],
  ["unicode space", "select\u00a0$a,\u2003$b,\ufeff$c, é$d, $é"],
  ["unicode word", "select 'é$a', \"é$b\", 😀$c, $😀"],
  ["crlf", "select 1 -- $a\r\n, $b\r\n-- $c\r, $d"],
  ["placeholder in number", "select 1$a, 0x1$b, 1e$c, 1.5$d, .5$e"],
  ["brackets are plain text", "select [[a]], [[$b]], $[c], $c"],
  ["empty and lone dollar", "$"],
  ["empty text", ""],
  ["only dollars", "$ $$ $$$ $$$$ $_ $-"],
  ["mongo pipeline", "{\"collection\": \"c\", \"aggregate\": [{\"$match\": {\"host\": \"$host\", \"v\": {\"$gt\": \"${min:number}\"}}}, {\"$limit\": 10}]}"],
  ["mongo key placeholder", "{\"$key\": 1, \"${k}\": 2, \"a$b\": \"${v}\"}"],
  ["mongo escaped dollar", "{\"a\": \"\\u0024esc\", \"b\": \"\\u0024{braced}\", \"c\": \"\\\\$real\"}"],
  ["mongo surrogates", "{\"a\": \"\\ud800\\u0024lone\", \"b\": \"\\ud83d\\ude00$pair\", \"c\": \"\\udc00$low\"}"],
  ["mongo bad escape", "{\"a\": \"\\x$bad\", \"b\": \"$after\"}"],
  ["mongo raw control char", "{\"a\": \"x\ty$tab\", \"b\": \"line\n$nl\"}"],
  ["mongo unterminated string", "{\"a\": \"$closed\", \"b\": \"$open"],
  ["mongo trailing backslash", "{\"a\": \"$a\", \"b\": \"$b\\"],
  ["mongo quote in sql comment", "SELECT 1 -- say \"$a\"\n, $b"],
  ["mongo single quotes only", "{'a': '$a'}"],
  ["mongo nested json text", "{\"a\": \"{\\\"b\\\": \\\"$inner\\\"}\"}"],
  ["mongo macro string", "{\"t\": \"$__timeFrom\", \"u\": \"${__timeTo}\", \"v\": \"x $__from y\", \"w\": \"$notmacro\"}"],
  ["json mixed with sql string", "SELECT '\"$a\"', \"'$b'\", '\"', $c, \"\""],
  ["quote then escape", "SELECT \"a\\\"$b\", \"c\\\\\", $d"],
  ["lines like a document", "SELECT\n  $__timeInterval(ts) AS t,\n  sum(v)\nFROM x\nWHERE $__timeFilter(ts)\n  AND h IN (${host})\n  AND e = '${env}'"],
];

// ---- the random cases: a token soup, as in the unit test of cross.rs, with JSON pieces ----

function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const SOUP = [
  "'", "''", "\"", "`", "\\", "$", "$$", "$a$", "$1", "$a", "$b", "${c}", "${d:csv}", "${e:nope}", "${f:json}", "-", "--", "#", "# ",
  "/", "*", "/*", "*/", "+", "=", ";", ",", ".", "(", ")", " ", "\n", "\r", "\t", "\u00a0", "\u00e9", "\u2003", "a", "E", "e", "B", "X",
  "N", "U", "U&", "1", "0x", "1e", "E'", "b'", "U&'", "U&\"", "x", "_", "{", "}", "[", "]", ":", "\\u0024", "\\\"", "\\ud800",
  "\\ude00", "\"$g\"", "{\"$match\":", "\"${h}\"", "$__timeFilter", "${__from}", "😀",
];

function random(count, seed) {
  const next = mulberry32(seed);
  const out = [];
  for (let i = 0; i < count; i++) {
    const n = 1 + Math.floor(next() * 20);
    let text = "";
    for (let k = 0; k < n; k++) text += SOUP[Math.floor(next() * SOUP.length)];
    out.push(text);
  }
  return out;
}

// ---- the names, as the page finds them ----

const namesOf = (text) => variableNamesOfLines(text);

// The document form: the page keeps only the names of variables of the document (runtime.js: `names`, `buildGraph`),
// for the first query of each panel, each extra query (4.15a), each query variable and each annotation.
function pageDocumentNames(doc) {
  const known = new Set(doc.variables.map((v) => v.name));
  const list = (lines, skip) => namesOf(lines).filter((n) => known.has(n) && n !== skip);
  const panels = {};
  for (const section of doc.sections) {
    for (const panel of section.panels) {
      const query = panel.data && panel.data.query;
      if (!query) continue;
      panels[String(panel.id)] = [list(query.sql), ...(panel.data.extra_queries || []).map((q) => list(q.sql))];
    }
  }
  const variables = {};
  for (const v of doc.variables) if (v.kind === "query") variables[v.name] = list(v.sql, v.name);
  const annotations = (doc.annotations || []).map((a) => list(a.sql));
  return { panels, variables, annotations };
}

const outputs = {};

// The page scan finds no name in most strings that have a "$". Only a text with one name or more is a case.
function cases(entries) {
  const list = [];
  for (const [id, text] of entries) {
    const names = namesOf(text);
    if (names.length > 0) list.push({ id, text, names });
  }
  return list;
}

outputs["texts.json"] = cases([...texts].map(([text, id]) => [id, text]));
outputs["curated.json"] = cases(CURATED.map(([note, text]) => [note, text]));
outputs["random.json"] = cases(random(4000, 0x4b313736).map((text, i) => [`random-${i}`, text]));
outputs["documents.json"] = documents.map(({ file, pointer, value }) => ({
  file,
  pointer,
  uid: value.uid,
  names: pageDocumentNames(value),
}));

// ---- write ----

mkdirSync(OUT, { recursive: true });
const check = process.argv.includes("--check");
let differs = 0;
for (const [name, data] of Object.entries(outputs)) {
  // One case on each line, so that a diff shows one case.
  const body = "[\n" + data.map((c) => JSON.stringify(c)).join(",\n") + "\n]\n";
  const path = join(OUT, name);
  if (check) {
    let old = "";
    try {
      old = readFileSync(path, "utf8");
    } catch {
      // a missing file differs
    }
    if (old !== body) {
      differs += 1;
      console.error(`differs: ${relative(ROOT, path)}`);
    }
  } else {
    writeFileSync(path, body);
    console.log(`${relative(ROOT, path)}: ${data.length} cases`);
  }
}
if (!check) {
  const names = new Set(Object.values(outputs).flatMap((l) => l.flatMap((c) => c.names || [])));
  console.log(`distinct names: ${names.size}; texts read: ${texts.size}; documents: ${documents.length}`);
}
if (differs > 0) process.exit(1);
