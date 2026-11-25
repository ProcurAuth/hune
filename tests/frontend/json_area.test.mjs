// SPDX-License-Identifier: MIT OR Apache-2.0
// The JSON area of the edit mode (10.4): the colours of a line, the marks of the changed lines, the height rule and the tokens of the colours.
// The pure parts run here. The browser case B240 reads the real page.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  MAX_DIFF_CELLS, MAX_TOKENS_PER_LINE, TAB_SIZE, bracketClass, bracketNear, diffLines, enclosingPair, errorRange, indentColumns, indentLength, lineTokens, rowOf, scanBrackets,
} from "../../web/js/editor/json_check.js";

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

test("the colours of the JSON area are the JSON tokens of VS Code, with no colour in the rules", () => {
  const css = read("css/editor.css");
  const tokens = {
    "jt-key": "--json-key", "jt-str": "--json-str", "jt-num": "--json-num", "jt-punct": "--json-punct",
    "jt-b0": "--json-bracket-1", "jt-b1": "--json-bracket-2", "jt-b2": "--json-bracket-3", "jt-bb": "--json-bracket-bad",
  };
  for (const [name, token] of Object.entries(tokens)) assert.match(css, new RegExp(`\\.${name} \\{ color: var\\(${token}\\); \\}`), name);
  assert.match(css, /\.jt-bool, \.jt-null \{ color: var\(--json-const\); \}/);
  const rules = css.slice(css.indexOf(".jarea__gutter {"), css.indexOf(".psummary {"));
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/.test(rules), "the rules of the area have no colour literal");
  assert.ok(!/--sql-/.test(rules), "the JSON area does not use the tokens of the SQL editor");
});

test("look.css has the same JSON tokens in its four contexts, with the palette of VS Code Light+ and Dark+", () => {
  const look = read("css/look.css");
  const names = ["key", "str", "num", "const", "punct", "bracket-1", "bracket-2", "bracket-3", "bracket-bad", "lineno", "lineno-active", "line-active", "guide", "guide-active", "match-bg", "match-edge", "selection", "selection-idle", "squiggle", "thumb", "thumb-hover"];
  for (const name of names) assert.equal(look.split(`--json-${name}:`).length - 1, 4, `--json-${name} is in four contexts`);
  const light = /html\[data-theme="light"\] \{[\s\S]*?--json-key: (#[0-9a-f]{6});/.exec(look);
  assert.equal(light[1], "#0451a5", "Light+ keys");
  assert.match(look, /--json-selection: #add6ff;/, "Light+ selection");
  assert.match(look, /--json-key: #9cdcfe;/, "Dark+ keys");
  assert.match(look, /--json-selection: #264f78;/, "Dark+ selection");
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
  assert.match(editor, /frame\.setError\(/);
});

// --- Brackets, indent, caret and the place of an error (the look of VS Code) ----------------------------------------------------------

const doc = ["{", '  "a": [1, {"b": "}"}],', '  "c": []', "}"];

test("scanBrackets pairs the brackets, gives their depth and parent, and skips brackets inside texts", () => {
  const scan = scanBrackets(doc);
  assert.deepEqual([...scan.pos], [0, 9, 13, 22, 23, 33, 34, 36]);
  assert.deepEqual([...scan.mate], [7, 4, 3, 2, 1, 6, 5, 0]);
  assert.deepEqual([...scan.depth], [0, 1, 2, 2, 1, 1, 1, 0]);
  assert.deepEqual([...scan.parent], [-1, 0, 1, 1, 0, 0, 0, -1]);
  assert.deepEqual([...scan.lineStart], [0, 2, 26, 36, 38]);
  assert.deepEqual([...scan.lineFirst], [0, 1, 5, 7, 8]);
});

test("a bracket with no pair, or a close bracket of the wrong kind, has no mate and does not close the open one", () => {
  const scan = scanBrackets(["[{]", "}"]);
  // The ] does not match the { on top, so it has no mate and the { stays open; the } of the next line closes it.
  assert.deepEqual([...scan.mate], [-1, 3, -1, 1]);
  assert.equal(bracketClass(scan, 0), "jt-bb");
  assert.equal(bracketClass(scan, 2), "jt-bb");
  assert.equal(bracketClass(scan, 1), "jt-b1");
  const open = scanBrackets(["{", '"x"']);
  assert.deepEqual([...open.mate], [-1]);
});

test("the colour of a bracket follows its depth in three colours that repeat", () => {
  const scan = scanBrackets(["[[[[]]]]"]);
  assert.deepEqual([0, 1, 2, 3].map((i) => bracketClass(scan, i)), ["jt-b0", "jt-b1", "jt-b2", "jt-b0"]);
  assert.deepEqual([4, 5, 6, 7].map((i) => bracketClass(scan, i)), ["jt-b0", "jt-b2", "jt-b1", "jt-b0"]);
});

test("a text with an escaped quote, or with no closing quote, hides the brackets up to the end of its line only", () => {
  // Line 0 is: "a\"}  (the } is inside the text). Line 2 is: "open {  (a text with no closing quote).
  const scan = scanBrackets(['"a\\"}', "{}", '"open {', "[]"]);
  assert.deepEqual([...scan.pos], [6, 7, 17, 18]);
  assert.deepEqual([...scan.mate], [1, 0, 3, 2]);
});

test("rowOf gives the line of an offset, with the first line at 0 and the end of the text on the last line", () => {
  const scan = scanBrackets(doc);
  assert.equal(rowOf(scan, 0), 0);
  assert.equal(rowOf(scan, 1), 0);
  assert.equal(rowOf(scan, 2), 1);
  assert.equal(rowOf(scan, 25), 1);
  assert.equal(rowOf(scan, 26), 2);
  assert.equal(rowOf(scan, 36), 3);
  assert.equal(rowOf(scanBrackets([""]), 0), 0);
});

test("bracketNear takes the bracket before the caret, else the one after it", () => {
  const scan = scanBrackets(doc);
  assert.equal(bracketNear(scan, 1), 0, "right after the first {");
  assert.equal(bracketNear(scan, 0), 0, "right before the first {");
  assert.equal(bracketNear(scan, 10), 1, "right after the [");
  assert.equal(bracketNear(scan, 13), 2, "right before the {");
  assert.equal(bracketNear(scan, 24), 4, "right after the ]");
  assert.equal(bracketNear(scan, 5), -1, "inside a name");
  assert.equal(scanBrackets(["{}"]).mate[bracketNear(scanBrackets(["{}"]), 2)], 0, "a caret after the } reaches its {");
});

test("enclosingPair gives the pair that holds the caret, also after a closed pair, and null at the document level", () => {
  const scan = scanBrackets(doc);
  assert.deepEqual(enclosingPair(scan, 5), [0, 7], "in a name of the first object");
  assert.deepEqual(enclosingPair(scan, 11), [1, 4], "inside the array");
  assert.deepEqual(enclosingPair(scan, 14), [2, 3], "inside the inner object");
  assert.deepEqual(enclosingPair(scan, 24), [0, 7], "after the closed array: the object holds the caret");
  assert.equal(enclosingPair(scan, 0), null, "before the first bracket");
  assert.equal(enclosingPair(scan, 40), null, "after the last bracket");
  assert.equal(enclosingPair(scanBrackets(["{", "x"]), 3), null, "a bracket with no pair holds nothing");
});

test("indentColumns counts a tab to the next multiple of the tab size, and indentLength counts the characters", () => {
  assert.equal(TAB_SIZE, 2);
  assert.equal(indentColumns("    x"), 4);
  assert.equal(indentColumns("\tx"), 2);
  assert.equal(indentColumns(" \tx"), 2);
  assert.equal(indentColumns("  \tx"), 4);
  assert.equal(indentColumns("x"), 0);
  assert.equal(indentColumns("   "), 3);
  assert.equal(indentLength("\t  x"), 3);
  assert.equal(indentLength("x"), 0);
  assert.equal(indentLength(""), 0);
});

test("errorRange gives the part of the line under the column of a syntax error", () => {
  const line = '  "tags": [ oops, "x"]';
  const tokens = lineTokens(line);
  assert.deepEqual(errorRange(line, tokens, 3), [2, 8], "a column in a token gives the token");
  assert.deepEqual(errorRange(line, tokens, 13), [12, 16], "a column in plain text gives the word, up to the comma");
  assert.deepEqual(errorRange(line, tokens, 12), [12, 16], "a column on white space goes to the next word");
  assert.deepEqual(errorRange("a  ", lineTokens("a  "), 99), [0, 1], "a column after the end gives the last character");
  assert.equal(errorRange("", [], 1), null, "an empty line has no part");
  assert.equal(errorRange("   ", [], 2), null, "a line of white space has no part");
  assert.equal(errorRange(line, tokens, 0), null, "a column under 1 gives none");
});

test("the css has the guides, the band, the boxes of the brackets, the wavy line and the scroll bars, with tokens only", () => {
  const css = read("css/editor.css");
  assert.match(css, /\.jt-ind \{[^}]*display: inline-block; height: var\(--json-row\)[^}]*repeating-linear-gradient\(to right, var\(--json-guide\) 0 1px, transparent 1px 2ch\)/, "a guide every two columns");
  assert.match(css, /\.jarea__band \{[^}]*background: var\(--json-line-active\)/, "the band of the line of the caret");
  assert.match(css, /\.jarea__match \{[^}]*border: 1px solid var\(--json-match-edge\); background: var\(--json-match-bg\)/, "the box of a bracket");
  assert.match(css, /\.jarea__guide \{[^}]*background: var\(--json-guide-active\)/, "the active guide");
  assert.match(css, /\.jt-err \{ text-decoration: underline wavy var\(--json-squiggle\)/, "the wavy line of an error");
  assert.match(css, /\.jarea__input::selection \{ background: var\(--json-selection\); \}/);
  assert.match(css, /\.jarea__input:not\(:focus\)::selection \{ background: var\(--json-selection-idle\); \}/);
  assert.match(css, /\.jarea__input::-webkit-scrollbar \{ width: 10px; height: 10px; \}/);
  assert.match(css, /@supports not selector\(::-webkit-scrollbar\) \{\s*\.jarea__input \{ scrollbar-width: thin; scrollbar-color: var\(--json-thumb\) transparent; \}/, "the standard properties only where the pseudo elements do not exist");
  assert.ok(!/\.jarea__input \{ scrollbar-color/.test(css), "scrollbar-color would turn off the 10 px bar in Chrome");
  assert.match(css, /\.jarea__active-num \{[^}]*color: var\(--json-lineno-active\)/);
  assert.match(css, /\.jarea__gutter \{[^}]*color: var\(--json-lineno\)/);
});

test("the row height of the script is the row height of the css, and the text is 14 px with a line height of 1.4 to 1.5", () => {
  const css = read("css/editor.css");
  const area = read("js/editor/json_area.js");
  const row = Number(/export const ROW = (\d+);/.exec(area)[1]);
  assert.equal(row, Number(/\.jframe \{ --json-row: (\d+)px;/.exec(css)[1]), "ROW is --json-row");
  assert.match(css, /\.jframe \{[^}]*line-height: var\(--json-row\)/);
  const size = Number(/--fs-md:\s*(\d+)px/.exec(read("css/tokens.css"))[1]);
  assert.equal(size, 14);
  assert.ok(row / size >= 1.4 && row / size <= 1.5, `the line height is ${(row / size).toFixed(2)}`);
});

test("the area keeps the guides, the band and the boxes in layers that take no pointer, and it never reads the layout while a key runs", () => {
  const area = read("js/editor/json_area.js");
  assert.match(area, /el\("div", \{ class: "jarea__body" \}, band, input, layer, deco\)/, "the text area is under the layer: the selection shows under the text");
  const caret = area.slice(area.indexOf("  function updateCaret() {"), area.indexOf("  input.addEventListener(\"scroll\""));
  assert.ok(!/scrollTop|clientHeight|scrollLeft|getBoundingClientRect|offsetHeight|offsetWidth/.test(caret), "the caret update reads no layout value");
  assert.match(area, /requestAnimationFrame\(updateCaret\)/, "one update for each frame");
  const css = read("css/editor.css");
  for (const rule of [".jarea__hl", ".jarea__deco"]) assert.match(css, new RegExp(`\\${rule} \\{[^}]*pointer-events: none`), `${rule} takes no pointer`);
});
