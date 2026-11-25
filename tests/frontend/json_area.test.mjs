// SPDX-License-Identifier: MIT OR Apache-2.0
// The JSON area of the edit mode (10.4): the colours of a line, the marks of the changed lines, the height rule and the tokens of the colours.
// The pure parts run here. The browser case B240 reads the real page.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { MAX_DIFF_CELLS, MAX_TOKENS_PER_LINE, diffLines, lineTokens } from "../../web/js/editor/json_check.js";

const read = (p) => readFileSync(new URL(`../../web/${p}`, import.meta.url), "utf8");
const kinds = (line) => lineTokens(line).map(([s, e, k]) => `${k}:${line.slice(s, e)}`);

test("a member name is a key, a text after a colon is a string, and the other values have their own kind", () => {
  assert.deepEqual(kinds('  "a": "b",'), ['key:"a"', "punct::", 'str:"b"', "punct:,"]);
  assert.deepEqual(kinds('"n": -2.5e3, "t": true, "f": false, "z": null'), [
    'key:"n"', "punct::", "num:-2.5e3", "punct:,", 'key:"t"', "punct::", "bool:true", "punct:,", 'key:"f"', "punct::", "bool:false", "punct:,", 'key:"z"', "punct::", "null:null",
  ]);
  assert.deepEqual(kinds("[1, 22]"), ["punct:[", "num:1", "punct:,", "num:22", "punct:]"]);
});

test("an escaped quote stays in the string, and a key may have spaces before its colon", () => {
  assert.deepEqual(kinds('"a\\"b"'), ['str:"a\\"b"']);
  assert.deepEqual(kinds('"k"  : 1'), ['key:"k"', "punct::", "num:1"]);
  assert.deepEqual(kinds('"\\\\"'), ['str:"\\\\"']);
});

test("a text that is not valid still gets colours, and a string with no closing quote runs to the end of the line", () => {
  assert.deepEqual(kinds('"open'), ['str:"open']);
  assert.deepEqual(kinds('  ~ "a" @ 1'), ['str:"a"', "num:1"]);
  assert.deepEqual(kinds("tru nul"), []);
  assert.deepEqual(kinds(""), []);
});

test("white space and unknown characters have no token, so they keep the plain colour", () => {
  for (const [start, end] of lineTokens("   \t  ")) assert.fail(`a token ${start}-${end}`);
  const tokens = lineTokens('  "a" : 1');
  assert.equal(tokens[0][0], 2);
});

test("a line gets at most MAX_TOKENS_PER_LINE tokens, so a document of one line stays fast", () => {
  const line = `[${Array.from({ length: MAX_TOKENS_PER_LINE + 500 }, () => "1").join(",")}]`;
  assert.equal(lineTokens(line).length, MAX_TOKENS_PER_LINE);
  assert.equal(MAX_TOKENS_PER_LINE, 2000);
});

test("the same text has no mark", () => {
  assert.deepEqual(diffLines("a\nb\nc", "a\nb\nc"), { added: [], modified: [], deleted: [] });
});

test("a changed line is modified, a new line is added, and a line that is gone leaves a mark at the border", () => {
  assert.deepEqual(diffLines("a\nb\nc\nd", "a\nB\nc\nd"), { added: [], modified: [1], deleted: [] });
  assert.deepEqual(diffLines("a\nb\nc\nd", "a\nb\nx\nc\nd"), { added: [2], modified: [], deleted: [] });
  assert.deepEqual(diffLines("a\nb\nc\nd", "a\nd"), { added: [], modified: [], deleted: [1] });
});

test("the first and the last line, and the marks at the ends of the text", () => {
  assert.deepEqual(diffLines("a\nb", "A\nb"), { added: [], modified: [0], deleted: [] });
  assert.deepEqual(diffLines("a\nb", "a\nB"), { added: [], modified: [1], deleted: [] });
  assert.deepEqual(diffLines("a\nb", "a"), { added: [], modified: [], deleted: [1] });
  assert.deepEqual(diffLines("a\nb", "b"), { added: [], modified: [], deleted: [0] });
  assert.deepEqual(diffLines("b", "a\nb"), { added: [0], modified: [], deleted: [] });
  assert.deepEqual(diffLines("a", "a\nb"), { added: [1], modified: [], deleted: [] });
  assert.deepEqual(diffLines("", "x"), { added: [], modified: [0], deleted: [] });
});

test("a change that replaces one line with three gives one modified line, then the added lines", () => {
  assert.deepEqual(diffLines("a\nb\nc", "a\nX\nY\nZ\nc"), { added: [2, 3], modified: [1], deleted: [] });
  assert.deepEqual(diffLines("a\nb\nc\nd", "a\nX\nY\nd"), { added: [], modified: [1, 2], deleted: [] });
});

test("a change that replaces more lines with fewer gives modified lines and a mark for the lines that are gone", () => {
  assert.deepEqual(diffLines("a\nb\nc\nd\ne", "a\nX\ne"), { added: [], modified: [1], deleted: [2] });
});

test("two changes apart are two marks, and a line that returns to its saved text has no mark", () => {
  assert.deepEqual(diffLines("a\nb\nc\nd\ne", "a\nB\nc\nD\ne"), { added: [], modified: [1, 3], deleted: [] });
  assert.deepEqual(diffLines("a\nb\nc", "a\nb\nc"), { added: [], modified: [], deleted: [] });
});

test("a change in a long text costs little: the common start and end are cut first", () => {
  const saved = Array.from({ length: 6000 }, (_, i) => `line ${i}`).join("\n");
  const lines = saved.split("\n");
  lines[3000] = "changed";
  const started = performance.now();
  const result = diffLines(saved, lines.join("\n"));
  assert.deepEqual(result, { added: [], modified: [3000], deleted: [] });
  assert.ok(performance.now() - started < 200, "a one-line change in 6,000 lines takes well under 200 ms");
});

test("above MAX_DIFF_CELLS the lines are compared by their place, and a line that is the same has no mark", () => {
  assert.equal(MAX_DIFF_CELLS, 1_000_000);
  const n = 1500;
  const saved = Array.from({ length: n }, (_, i) => `row ${i}`);
  const text = saved.map((l, i) => (i === 0 || i === n - 1 || i % 2 ? l : `${l} changed`));
  text[0] = "first changed";
  text[n - 1] = "last changed";
  const result = diffLines(saved.join("\n"), text.join("\n"));
  assert.ok(result.modified.includes(0) && result.modified.includes(n - 1), "the changed ends have marks");
  assert.ok(!result.modified.includes(1), "a line that is the same has no mark");
  assert.deepEqual(result.added, []);
  assert.deepEqual(result.deleted, []);
});

test("the colours of the JSON area are the tokens of the SQL colours, with no colour in the rules", () => {
  const css = read("css/editor.css");
  const tokens = { "jt-key": "--sql-keyword", "jt-str": "--sql-string", "jt-num": "--sql-number", "jt-bool": "--sql-function", "jt-null": "--sql-null" };
  for (const [name, token] of Object.entries(tokens)) assert.match(css, new RegExp(`\\.${name} \\{ color: var\\(${token}\\); \\}`), name);
  assert.match(css, /\.jt-punct \{ color: var\(--muted\); \}/);
  const rules = css.slice(css.indexOf(".jarea__gutter {"), css.indexOf(".psummary {"));
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/.test(rules), "the rules of the area have no colour literal");
});

test("the two layers of the JSON area share the text metrics of the SQL area", () => {
  const css = read("css/editor.css");
  const rule = /\.jarea__hl, \.jarea__input \{([^}]*)\}/.exec(css);
  assert.ok(rule, "one rule for the two layers");
  for (const property of ["font: inherit", "line-height: inherit", "letter-spacing: 0", "word-spacing: 0", "tab-size: 2", "text-rendering: auto", "font-kerning: none", "font-variant-ligatures: none", "white-space: pre"]) {
    assert.ok(rule[1].includes(property), `the shared rule has ${property}`);
  }
  assert.match(css, /\.jarea__hl \{[^}]*overflow: hidden/, "the layer does not scroll by itself");
  assert.match(css, /\.jarea__input \{[^}]*color: transparent/, "the text of the text area is transparent");
  assert.match(css, /\.jarea__input \{[^}]*caret-color: var\(--text\)/, "the caret shows");
});

test("the JSON view fills the page: the height is the window less the header and the padding above and below", () => {
  const css = read("css/editor.css");
  assert.match(css, /\.jeditor \{[^}]*height: max\(320px, calc\(100dvh - var\(--header-h\) - 2 \* var\(--page-pad\)\)\)/);
  assert.match(css, /\.jeditor > \.jframe--tall \{ flex: 1 1 auto; height: auto;/);
  assert.match(css, /\.jeditor > \.ebar, \.jeditor > \.errors \{ flex: none; \}/);
  assert.match(css, /\.settings\[data-tab="json"\] \{[^}]*height: max\(320px, calc\(100dvh - var\(--header-h\) - 2 \* var\(--page-pad\)\)\)/);
  assert.ok(!/\.jframe--tall \{ height: max\(420px/.test(css), "the old fixed height is gone");
});

test("the area reads nothing from the page while a key runs, and the editor draws the marks again after a revert", () => {
  const area = read("js/editor/json_area.js");
  const paint = area.slice(area.indexOf("  function paint() {"), area.indexOf("  // A box for a run of lines"));
  assert.ok(!/scrollTop|clientHeight|scrollLeft|getBoundingClientRect|offsetHeight/.test(paint), "paint reads no layout value");
  const place = area.slice(area.indexOf("  function place() {"), area.indexOf("  function paint() {"));
  assert.ok(!/scrollTop|clientHeight|scrollLeft/.test(place), "place reads no layout value");
  const editor = read("js/editor/json_editor.js");
  assert.match(editor, /frame\.setBase\(\(\) => prettyJson\(E\.base\.doc\)\)/);
  assert.match(editor, /onChange\(\) \{[\s\S]*?frame\.paintMarks\(\);/);
  assert.match(editor, /frame\.setErrorLine\(/);
});
