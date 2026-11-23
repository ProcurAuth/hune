// Vectors of the editor fixes (10.3): a valid macro is never an error in the editor, and the text areas switch the spell check off.
// The editor marks no macro by itself. The server judges a macro (7.10, 7.22), and the editor shows its message.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { highlight } from "../../web/js/vendor/chdash/app_highlight.js";
import { findMacros, findVariableNames } from "../../web/js/sqlscan.js";
import { KIND, lexPg } from "../../web/js/sql_lex_pg.js";

const WEB = join(dirname(fileURLToPath(import.meta.url)), "../../web");

// Every macro of 7.10 (ClickHouse) and 7.22 (PostgreSQL), written as in a query: a name, and the arguments that it takes.
const CLICKHOUSE = [
  "$__timeFilter(ts)",
  "$__timeFilter_ms(ts)",
  "$__fromTime",
  "$__toTime",
  "$__fromTime_ms",
  "$__toTime_ms",
  "$__timeInterval(ts)",
  "$__timeInterval_ms(ts)",
  "$__interval_s",
  "$__interval_ms",
  "$__from",
  "$__to",
  "$__dateFilter(d)",
  "$__dateTimeFilter(d, ts)",
  "$__dt(d, ts)",
  "$__conditionalAll(env IN ($env), $env)",
];
const POSTGRES = [
  "$__time(ts)",
  "$__timeEpoch(ts)",
  "$__timeFilter(ts)",
  "$__timeFrom",
  "$__timeTo",
  "$__timeFrom()",
  "$__timeTo()",
  "$__timeGroup(ts, '5m')",
  "$__timeGroupAlias(ts, $__interval)",
  "$__unixEpochFilter(ts)",
  "$__unixEpochFrom",
  "$__unixEpochTo",
  "$__unixEpochNanoFilter(ts)",
  "$__unixEpochNanoFrom",
  "$__unixEpochNanoTo",
  "$__unixEpochGroup(ts, '1h')",
  "$__unixEpochGroupAlias(ts, '1h')",
  "$__interval_s",
  "$__interval_ms",
  "$__from",
  "$__to",
  "$__conditionalAll(env IN ($env), $env)",
];
// A wrong case, an unknown name and a form that 7.10 refuses. The editor does not judge them. The server answers "is not a macro of Hune".
const WRONG = ["$__TIMEFILTER(ts)", "$__timefilter(ts)", "$__timeFilterX(ts)", "$__range", "$__interval", "$__timezone", "$__foo(ts)"];

const SHAPES = [
  (m) => `SELECT 1 FROM t WHERE ${m}`,
  (m) => `SELECT ${m} AS x FROM t`,
  (m) => `SELECT 1\nFROM t\nWHERE\n  ${m}\n  AND a = 1`,
  (m) => `SELECT 1 FROM t WHERE   ${m}   AND  a = 1`,
  (m) => `SELECT 1 FROM t WHERE a = '${m}'`,
  (m) => `SELECT 1 FROM t -- ${m}\nWHERE a = 1`,
];

// The text of the coloured layer, without the tags: it must be the query itself.
const plain = (html) => html.replace(/<[^>]*>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&#0?39;/g, "'").replace(/&amp;/g, "&");

function noErrorMark(text) {
  const html = highlight.toHtml(text);
  assert.ok(!/tok-err|class="[^"]*err/.test(html), `the coloured layer marks an error in: ${JSON.stringify(text)}`);
  assert.equal(plain(html), text, "the coloured layer keeps the text");
  // The lexer of PostgreSQL has no error kind: its tokens cover the text with no gap, and a macro is a parameter token or a word.
  let at = 0;
  for (const token of lexPg(text)) {
    assert.equal(token.start, at, "no gap in the tokens");
    assert.ok(Object.values(KIND).includes(token.kind), `a known kind: ${token.kind}`);
    at = token.end;
  }
  assert.equal(at, text.length);
}

test("each valid macro of ClickHouse (7.10) has no error mark, in each place", () => {
  for (const macro of CLICKHOUSE) for (const shape of SHAPES) noErrorMark(shape(macro));
});

test("each valid macro of PostgreSQL (7.22) has no error mark, in each place", () => {
  for (const macro of POSTGRES) for (const shape of SHAPES) noErrorMark(shape(macro));
});

test("a macro is not a variable, and the scan finds it by its name", () => {
  for (const macro of [...CLICKHOUSE, ...POSTGRES]) {
    const name = /^\$(__\w+)/.exec(macro)[1];
    const text = `SELECT 1 FROM t WHERE ${macro} AND x = $v`;
    assert.ok(findMacros(text).includes(name), `${name} is a macro`);
    assert.ok(!findVariableNames(text).includes(name), `${name} is not a variable`);
    assert.ok(findVariableNames(text).includes("v"), "the variable stays");
  }
});

test("a wrong case or an unknown name has no mark in the editor: the server gives the error", () => {
  for (const macro of WRONG) {
    for (const shape of SHAPES.slice(0, 4)) noErrorMark(shape(macro));
    const name = /^\$(__\w+)/.exec(macro)[1];
    assert.deepEqual(findMacros(`WHERE ${macro}`), [name], "the name reaches the server as written");
  }
});

// The root cause of the false error: el() skips an attribute whose value is false, so `spellcheck: false` left the spell check on.
// The native spell check then draws a red wavy line under a word such as timeFilter, in a transparent text area.
function ownScripts(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== "vendor") ownScripts(path, out);
    } else if (entry.name.endsWith(".js")) out.push(path);
  }
  return out;
}

test("no script gives the boolean false as the attribute spellcheck (el() skips it)", () => {
  for (const file of ownScripts(join(WEB, "js"))) {
    const src = readFileSync(file, "utf8").replace(/\/\/.*$/gm, "");
    assert.ok(!/\bspellcheck\s*:\s*false\b/.test(src), `${relative(WEB, file)} gives spellcheck: false`);
  }
});

test("the SQL text area and the JSON text area have spellcheck set to the text false", () => {
  for (const name of ["editor/sql_editor.js", "editor/json_area.js"]) {
    const src = readFileSync(join(WEB, "js", name), "utf8");
    assert.match(src, /el\("textarea",\s*\{[^}]*spellcheck:\s*"false"/, name);
  }
});

test("the two layers of the SQL area set the same text properties", () => {
  const css = readFileSync(join(WEB, "css/editor.css"), "utf8");
  const rule = /\.sql__hl, \.sql__input \{([^}]*)\}/.exec(css);
  assert.ok(rule, "one rule for the two layers");
  for (const property of ["font: inherit", "line-height", "letter-spacing: 0", "word-spacing: 0", "tab-size: 2", "text-rendering: auto", "font-kerning: none", "font-variant-ligatures: none", "white-space: pre", "padding:", "border: 0", "box-sizing: border-box"]) {
    assert.ok(rule[1].includes(property), `the shared rule has ${property}`);
  }
  assert.match(css, /\.sql__hl \{[^}]*overflow: hidden/, "the coloured layer does not scroll by itself");
});
