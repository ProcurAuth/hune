// SPDX-License-Identifier: MIT OR Apache-2.0
// The colour layer of the SQL area (10.3). It draws only the rows near the visible rows. These tests compare its markup
// with the markup that the engine writes for the whole text: each character must have the same kind. The layer has no DOM here: `code` and
// the gaps are plain objects, so the test reads the markup and the heights that the layer sets.
import test from "node:test";
import assert from "node:assert/strict";

// A small DOM: a fragment, a text node and a span are plain objects. `replaceChildren` of `code` keeps the nodes of the fragment.
const makeNode = (init) => ({ ...init, append(...nodes) { this.children.push(...nodes); } });
globalThis.document = {
  createDocumentFragment: () => makeNode({ children: [] }),
  createTextNode: (data) => ({ data, kind: "plain" }),
  createElement: () => ({ className: "", textContent: "", get data() { return this.textContent; }, get kind() { return this.className.replace("tok-", ""); } }),
};

const { createColourLayer, BUFFER, MARGIN, ROW, VISIBLE } = await import("../../web/js/editor/sql_colour.js");
const { highlight } = await import("../../web/js/vendor/chdash/app_highlight.js");

const layer = () => {
  const code = {
    style: {},
    children: [],
    replaceChildren(frag) { this.children = frag.children; this.written = null; },
    set textContent(value) { this.children = [{ data: value, kind: "plain" }]; },
    get textContent() { return this.children.map((n) => n.data).join(""); },
  };
  const gapTop = { style: {} };
  const gapBottom = { style: {} };
  const colour = createColourLayer({ code, gapTop, gapBottom });
  return { code, gapTop, gapBottom, colour };
};

const ENTITIES = { "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&#039;": "'" };

/** The kind of each character of a markup: a list of [character, kind]. */
function kinds(html) {
  const out = [];
  let kind = "plain";
  const re = /<span class="tok-(\w+)">|<\/span>|&amp;|&lt;|&gt;|&quot;|&#039;|[^<&]/gs;
  for (let m = re.exec(html); m; m = re.exec(html)) {
    if (m[1]) kind = m[1];
    else if (m[0] === "</span>") kind = "plain";
    else out.push([ENTITIES[m[0]] || m[0], kind]);
  }
  return out;
}

/** The kind of each character of a list of pieces or of the nodes in `code`. */
const kindsOfNodes = (nodes) => nodes.flatMap((n) => [...(n.data ?? n.text)].map((ch) => [ch, n.kind]));

/** A small random generator, so that a failure repeats. */
function random(seed) {
  let x = seed >>> 0;
  return () => {
    x = (Math.imul(x, 1664525) + 1013904223) >>> 0;
    return x / 4294967296;
  };
}

const PIECES = [
  "SELECT", "FROM", "WHERE", "AND", "OR", "AS", "NULL", "count(", "sum (", "toStartOfHour(ts)", "UInt64", "DateTime64", "host", "cpu_total", "42", "-7", "3.14e2",
  ",", "(", ")", "=", "<", ">", "&", "*", "+", "$__timeFilter(ts)", "${host}", " ", "  ", "\t", "'text'", "'a''b'", "'multi\nline'", "'esc\\'q'", '"name"', '"two\nlines"',
  "-- a comment\n", "# hash comment\n", "/* block */", "/* multi\nline\nblock */", "'unclosed", "/* open", "é à ü", "中文",
];
// A name in backticks and a keyword pair (GROUP BY, ORDER BY, ...) do not go into one text. The engine splits the plain text at a pair, even when the pair
// is inside a name, and then it pairs the backticks again from the new start. Such a text is not a case of the layer: a window may differ there.
const BACKTICKS = ["`back`", "`tick\nname`", "`it's`"];
const PAIRS = ["GROUP\nBY", "ORDER BY", "LEFT\nJOIN", "UNION ALL", "ARRAY JOIN"];

function randomText(next, rows, extra = PAIRS) {
  const pieces = [...PIECES, ...extra];
  const lines = [];
  for (let r = 0; r < rows; r++) {
    let line = "";
    const n = Math.floor(next() * 7);
    for (let k = 0; k < n; k++) line += pieces[Math.floor(next() * pieces.length)] + (next() < 0.6 ? " " : "");
    lines.push(line);
  }
  return lines.join("\n");
}

/** The offset of the start of each line. */
function starts(text) {
  const out = [0];
  for (let i = text.indexOf("\n"); i >= 0; i = text.indexOf("\n", i + 1)) out.push(i + 1);
  return out;
}

test("the pieces of the whole text have the kinds of the engine for each character", () => {
  const { colour } = layer();
  for (const text of ["", "SELECT 1", "SELECT a,\n  'x\ny' AS b -- c\nFROM `t\n--` GROUP\nBY 1, 2 /* a\nb */ ORDER BY 3\n", "a < b && c > \"d\" 'e'", "x -- open"]) {
    colour.setText(text);
    assert.deepEqual(kindsOfNodes(colour.pieces(0, text.length)), kinds(highlight.toHtml(text)), JSON.stringify(text));
  }
});

test("a window of rows has the same kinds of characters as the same rows of the whole text", () => {
  const next = random(20260115);
  const { colour } = layer();
  let compared = 0;
  for (let round = 0; round < 120; round++) {
    const text = randomText(next, 8 + Math.floor(next() * 60), round % 2 ? BACKTICKS : PAIRS);
    const full = kinds(highlight.toHtml(text));
    assert.equal(full.length, text.length);
    colour.setText(text);
    const list = starts(text);
    for (let k = 0; k < 12; k++) {
      const r0 = Math.floor(next() * list.length);
      const r1 = Math.min(list.length, r0 + 1 + Math.floor(next() * 25));
      const a = list[r0];
      const b = r1 < list.length ? list[r1] : text.length;
      assert.deepEqual(kindsOfNodes(colour.pieces(a, b)), full.slice(a, b), `rows ${r0} to ${r1} of ${JSON.stringify(text)}`);
      compared += 1;
    }
  }
  assert.ok(compared > 1000);
});

test("a range that starts or ends inside a row (the pieces of the folded view) has the same kinds as the whole text", () => {
  const next = random(77);
  const { colour } = layer();
  for (let round = 0; round < 80; round++) {
    const text = randomText(next, 10 + Math.floor(next() * 40), round % 2 ? BACKTICKS : PAIRS);
    const full = kinds(highlight.toHtml(text));
    colour.setText(text);
    for (let k = 0; k < 15; k++) {
      const a = Math.floor(next() * text.length);
      const b = Math.min(text.length, a + 1 + Math.floor(next() * 200));
      assert.deepEqual(kindsOfNodes(colour.pieces(a, b)), full.slice(a, b), `range ${a} to ${b} of ${JSON.stringify(text)}`);
    }
  }
});

test("a string or a comment that starts above the window keeps its colour in the window", () => {
  const rows = Array.from({ length: 3000 }, (_, i) => `line ${i}`);
  rows[10] = "/* a long comment";
  rows[2500] = "end */ SELECT 1";
  rows[2600] = "'open string";
  const text = rows.join("\n");
  const { colour } = layer();
  colour.setText(text);
  const list = starts(text);
  const middle = kindsOfNodes(colour.pieces(list[1000], list[1010])).every(([, kind]) => kind === "com");
  assert.equal(middle, true, "rows inside the comment are all comment");
  const end = kindsOfNodes(colour.pieces(list[2500], list[2501]));
  assert.equal(end.slice(0, 6).every(([, kind]) => kind === "com"), true, "the end of the comment");
  assert.equal(end.slice(7, 13).map(([c]) => c).join(""), "SELECT");
  assert.equal(end[7][1], "kw");
  const open = kindsOfNodes(colour.pieces(list[2700], list[2710]));
  assert.equal(open.every(([, kind]) => kind === "str"), true, "rows inside the open string are all string");
});

test("a text of 262,144 bytes draws a window of rows, with gaps that keep the height of all rows", () => {
  const unit = "SELECT a, b\nFROM (\n  SELECT c\n  FROM t\n) AS x\nWHERE a = 1;\n";
  let text = "";
  while (text.length + unit.length <= 262144) text += unit;
  const { code, gapTop, gapBottom, colour } = layer();
  colour.setText(text);
  const rows = starts(text).length;
  colour.draw(0, 300);
  const drawnText = code.textContent;
  const first = Math.floor(0 / ROW);
  const last = Math.min(rows, first + Math.max(VISIBLE, Math.ceil(300 / ROW)) + BUFFER);
  assert.equal(drawnText, text.slice(0, starts(text)[last]));
  assert.equal(gapTop.style.height, "0px");
  assert.equal(gapBottom.style.height, `${(rows - last) * ROW}px`);
  assert.deepEqual(colour.bounds(), { from: 0, to: starts(text)[last] });
  // A scroll inside the margins draws nothing. A scroll near the edge of the window draws again.
  const limit = last - MARGIN - VISIBLE;
  assert.equal(colour.ensure(20 * ROW, 300), false);
  assert.equal(colour.ensure(limit * ROW, 300), false);
  assert.equal(colour.ensure((limit + 2) * ROW, 300), true);
  assert.equal(colour.ensure(1000 * ROW, 300), true);
  const again = colour.bounds();
  assert.equal(again.from, starts(text)[900]);
  const r0 = Number.parseInt(gapTop.style.height, 10) / ROW;
  assert.equal(r0 * ROW + (Number.parseInt(gapBottom.style.height, 10) + (starts(text).indexOf(again.to) - r0) * ROW), rows * ROW, "the gaps and the rows make the height of all rows");
  // The end of the text: the last row has a line end that the layer adds, as the text area draws a last empty row.
  colour.draw((rows - 5) * ROW, 300);
  assert.equal(gapBottom.style.height, "0px");
  assert.equal(code.textContent.endsWith(`${text.slice(text.lastIndexOf("\n", text.length - 2) + 1)}\n`), true);
});

test("the plain draw has the same rows with no markup, and a text with no line end has one row", () => {
  const { code, colour } = layer();
  colour.setText("SELECT 'a'");
  colour.draw(0, 300, true);
  assert.equal(code.textContent, "SELECT 'a'\n");
  colour.setText("");
  colour.draw(0, 300);
  assert.equal(code.textContent, "\n");
});

test("the layer is as wide as the widest row: a tab counts to the next multiple of 2 columns", () => {
  const { code, colour } = layer();
  colour.setText("a\n\t\tb\n中文文");
  colour.draw(0, 300);
  assert.equal(code.style.minWidth, "6ch");
  colour.setText("abc");
  colour.draw(0, 300);
  assert.equal(code.style.minWidth, "3ch");
});
