// SPDX-License-Identifier: MIT OR Apache-2.0
// F42 fixes (10.3c): the highlighter has no stack limit, the node cap of the SQL tree is exact, and the folded view draws only the rows near the visible rows.
// The tree is never a security check: a fault of the colours or of the tree never stops a draft change (the DOM test of this rule is in sql_editor_dom.test.mjs).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { highlight } from "../../web/js/vendor/chdash/app_highlight.js";
import { MAX_AST_NODES, utf8Length, walk } from "../../web/js/editor/ast/nodes.js";
import { parseSql } from "../../web/js/editor/ast/sql_tree.js";
import "../../web/js/editor/ast/tokens_ch.js";
import "../../web/js/editor/ast/tokens_pg.js";
import { folds, foldedRows, outerFolds } from "../../web/js/editor/ast/structure.js";

// ---------------------------------------------------------------------------------------------------------------------
// Fault 1: a text of about 125,000 tokens made the highlighter throw RangeError (a spread of all tokens, engine patch E16).

test("the highlighter reads a text of 130,001 tokens and gives the same text back (no RangeError)", () => {
  const text = `SELECT ${"1,".repeat(65000)}1`;
  const tokens = highlight.lexAllJs(text);
  assert.ok(tokens.length > 125000, `${tokens.length} tokens`);
  assert.equal(tokens[0].start, 0);
  assert.equal(tokens[tokens.length - 1].end, text.length);
  const html = highlight.toHtml(text);
  assert.equal(html.replace(/<[^>]+>/g, ""), text);
  // Strings and comments between the numbers go the same way: the plain text between two spans is one long run.
  const mixed = `SELECT ${"1,'a',".repeat(40000)}1 -- end`;
  assert.equal(highlight.toHtml(mixed).replace(/<[^>]+>/g, "").replace(/&#0?39;/g, "'").replace(/&quot;/g, '"'), mixed);
});

test("the engine edit E16 of app_highlight.js is marked, and the file has no spread push of tokens", () => {
  const source = readFileSync(new URL("../../web/js/vendor/chdash/app_highlight.js", import.meta.url), "utf8");
  assert.match(source, /\/\/ hune: E16, F42\./);
  assert.ok(!/\.push\(\.\.\./.test(source), "a spread push of the tokens would throw for 125,000 tokens");
});

// ---------------------------------------------------------------------------------------------------------------------
// Fault 2: the node cap is exact: 100,000 nodes work and the 100,001st node gives too_many_nodes.

const countNodes = (root) => {
  let n = 0;
  walk(root, () => {
    n += 1;
  });
  return n;
};

// "SELECT" and n numbers make 2n + 5 nodes (n > 1). A comment at the end adds one node.
const numbers = (n) => `SELECT ${new Array(n).fill("1").join(",")}`;

test("the node cap is exact for ClickHouse and PostgreSQL: 100,000 nodes work, 100,001 give too_many_nodes", () => {
  assert.equal(MAX_AST_NODES, 100000);
  for (const dialect of ["clickhouse", "postgres"]) {
    const under = parseSql(numbers(49997), dialect);
    assert.equal(under.capped, null, `${dialect} 99,999`);
    assert.equal(countNodes(under.root), 99999);
    const exact = parseSql(`${numbers(49997)} -- c`, dialect);
    assert.equal(exact.capped, null, `${dialect} 100,000`);
    assert.equal(countNodes(exact.root), MAX_AST_NODES);
    const exactBlock = parseSql(`${numbers(49997)} /* c */`, dialect);
    assert.equal(exactBlock.capped, null, `${dialect} 100,000 with a block comment`);
    const exactStatements = parseSql(`${numbers(49997)};\n-- c`, dialect);
    assert.equal(exactStatements.capped, null, `${dialect} 100,000 with a semicolon`);
    const over = parseSql(`${numbers(49997)} -- c\n-- d`, dialect);
    assert.equal(over.capped && over.capped.reason, "too_many_nodes", `${dialect} 100,001`);
    // The partial tree has no more nodes than the cap, and the tokens cover the whole text.
    assert.ok(countNodes(over.root) <= MAX_AST_NODES, `${dialect}: ${countNodes(over.root)} nodes`);
    const text = `${numbers(49997)} -- c\n-- d`;
    assert.equal(over.root.end, text.length);
    assert.equal(over.tokens[over.tokens.length - 1].end, text.length);
    let at = 0;
    for (const t of over.tokens) {
      assert.equal(t.start, at);
      at = t.end;
    }
    assert.equal(at, text.length);
    assert.ok(over.capped.at >= 0 && over.capped.at < text.length);
    const wide = parseSql(numbers(49998), dialect);
    assert.equal(wide.capped && wide.capped.reason, "too_many_nodes", `${dialect} 49,998 numbers`);
  }
});

test("the node cap is exact for many statements and for brackets, and it does not change a tree under the cap", () => {
  // `SELECT;` is 3 nodes (statement, query, clause) and the script is 1.
  for (const dialect of ["clickhouse", "postgres"]) {
    const ok = parseSql("SELECT;".repeat(33333), dialect);
    assert.equal(ok.capped, null);
    assert.ok(countNodes(ok.root) <= MAX_AST_NODES);
    assert.equal(parseSql("SELECT;".repeat(33334), dialect).capped.reason, "too_many_nodes");
    assert.equal(parseSql("()".repeat(120000), dialect).capped.reason, "too_many_nodes");
  }
});

// ---------------------------------------------------------------------------------------------------------------------
// Fault 3: the folded view draws a window of rows. A small DOM stands in for the browser: text nodes, a tree walker and a range.
// The layout cost of a real browser is measured by the browser case B176. Here the test counts the nodes that the view makes
// and times the JavaScript of one fold of a text of 262,144 bytes (the policy: the least of 7 runs, limit 300 ms; target 100 ms).

class N {
  constructor(nodeType, tag, data = "") {
    this.nodeType = nodeType;
    this.tag = tag;
    this.data = data;
    this.children = [];
    this.parentNode = null;
    this.style = {};
    this.dataset = {};
    this.attrs = new Map();
    this.listeners = new Map();
    this.className = "";
    this.hidden = false;
    this.scrollTop = 0;
    this.scrollLeft = 0;
    this.clientHeight = 600;
  }

  get firstChild() { return this.children[0] || null; }
  get childNodes() { return this.children; }
  get textContent() { return this.nodeType === 3 ? this.data : this.children.map((c) => c.textContent).join(""); }
  set textContent(v) { this.children = []; if (v !== "") this.append(new N(3, "#text", String(v))); }
  setAttribute(k, v) { this.attrs.set(k, String(v)); }
  addEventListener(type, fn) { this.listeners.set(type, [...(this.listeners.get(type) || []), fn]); }
  removeChild(n) { this.children.splice(this.children.indexOf(n), 1); n.parentNode = null; }
  contains(n) { return n === this || this.children.some((c) => c.contains(n)); }
  append(...kids) {
    for (const k of kids) {
      if (k.nodeType === 11) {
        const moved = k.children;
        k.children = [];
        for (const m of moved) { m.parentNode = this; this.children.push(m); }
      } else {
        k.parentNode = this;
        this.children.push(k);
      }
    }
  }
  replaceChildren(...kids) { this.children = []; this.append(...kids); }
  fire(type) { for (const fn of this.listeners.get(type) || []) fn({ type }); }
}

const textNodes = (root, out = []) => {
  if (root.nodeType === 3) out.push(root);
  else for (const c of root.children) textNodes(c, out);
  return out;
};

class FakeRange {
  setStart(node, off) { this.sn = node; this.so = off; }
  setEnd(node, off) { this.en = node; this.eo = off; }
  // The text nodes from the start to the end, cut at the two points. The structure of the elements is not copied.
  cloneContents() {
    let root = this.sn;
    while (root.parentNode) root = root.parentNode;
    const all = textNodes(root);
    const from = all.indexOf(this.sn);
    const to = all.indexOf(this.en);
    const frag = new N(11, "#fragment");
    for (let i = from; i <= to; i++) {
      const n = all[i];
      const a = n === this.sn ? this.so : 0;
      const b = n === this.en ? this.eo : n.data.length;
      if (b > a) frag.append(new N(3, "#text", n.data.slice(a, b)));
    }
    return frag;
  }
}

Object.assign(globalThis, {
  Node: N,
  Element: N,
  Range: FakeRange,
  document: {
    createElement: (tag) => new N(1, tag),
    createTextNode: (t) => new N(3, "#text", String(t)),
    createDocumentFragment: () => new N(11, "#fragment"),
    createTreeWalker: (root) => {
      const list = textNodes(root);
      let i = 0;
      return { nextNode: () => list[i++] || null };
    },
  },
});
const { createFoldView, ROW } = await import("../../web/js/editor/sql_fold_view.js");

// A colour layer: the text in pieces, as the highlighter makes them (one text node for each line, some in an element).
function colourLayer(text) {
  const code = new N(1, "code");
  const lines = text.split("\n");
  lines.forEach((line, i) => {
    const piece = `${line}${i < lines.length - 1 ? "\n" : ""}`;
    if (i % 2 === 0) code.append(new N(3, "#text", piece));
    else {
      const span = new N(1, "span");
      span.append(new N(3, "#text", piece));
      code.append(span);
    }
  });
  code.append(new N(3, "#text", "\n"));
  return code;
}

const BIG = (() => {
  const unit = "SELECT a, b\nFROM (\n  SELECT c\n  FROM t\n) AS x\nWHERE a = 1;\n";
  let text = "";
  while (text.length + unit.length <= 262144) text += unit;
  return text + " ".repeat(262144 - text.length);
})();

function foldOf(text, nth = 0) {
  const result = parseSql(text, "clickhouse");
  const list = folds(result, text);
  const key = list.items[nth].key;
  const outer = outerFolds(list.items, new Set([key]));
  return { outer, rows: foldedRows(text, outer) };
}

test("the folded view of a text of 262,144 bytes draws a window of rows, with the right text, chip and gaps", () => {
  assert.equal(utf8Length(BIG), 262144);
  const { outer, rows } = foldOf(BIG);
  assert.equal(outer.length, 1);
  const input = new N(1, "textarea");
  const code = colourLayer(BIG);
  const view = createFoldView({ code, input, onToggle() {} });
  view.show(BIG, outer, rows.rowLines, 0, 600);
  assert.equal(view.shown, true);
  const gapTop = view.folded.children[0];
  const body = view.folded.children[1];
  const gapBottom = view.folded.children[2];
  const total = rows.rowLines.length;
  const drawn = Math.round((total * ROW - parseInt(gapTop.style.height, 10) - parseInt(gapBottom.style.height, 10)) / ROW);
  assert.ok(drawn > 0 && drawn < 400, `${drawn} rows of ${total} are drawn`);
  assert.equal(gapTop.style.height, "0px");
  // The first rows: the first statement with its subquery as one chip, the same text as the source outside the fold.
  const lines = BIG.split("\n");
  const expected = `${lines[0]}\n${lines[1]}… 4 lines${lines[4]}\n${lines[5]}\n`;
  assert.equal(body.textContent.slice(0, expected.length), expected);
  assert.equal(body.textContent.split("\n").length, drawn);
  assert.equal(body.children.filter((c) => c.className === "sql__chip").length, 1);
  // A scroll to the end draws the last rows and the last line of the text, with no chip.
  view.folded.scrollTop = total * ROW;
  view.folded.fire("scroll");
  const after = view.folded.children[1].textContent;
  assert.ok(after.endsWith(BIG.slice(BIG.lastIndexOf("\n", BIG.length - 2) + 1)), "the end of the text");
  const drawnEnd = Math.round((total * ROW - parseInt(view.folded.children[0].style.height, 10) - parseInt(view.folded.children[2].style.height, 10)) / ROW);
  assert.ok(drawnEnd > 0 && drawnEnd < 400, `${drawnEnd} rows are drawn at the end`);
  assert.equal(view.folded.children[2].style.height, "0px");
  // A scroll in the middle: the window follows and the gaps and rows add up to the scroll height.
  view.folded.scrollTop = 5000 * ROW;
  view.folded.fire("scroll");
  const mid = parseInt(view.folded.children[0].style.height, 10) / ROW;
  assert.ok(mid > 4800 && mid < 5000, `window starts at row ${mid}`);
  view.hide();
  assert.equal(view.shown, false);
});

test("a fold of 262,144 bytes takes 300 ms or less in JavaScript (the least of 7 runs; target 100 ms)", () => {
  const { outer, rows } = foldOf(BIG);
  const view = createFoldView({ code: colourLayer(BIG), input: new N(1, "textarea"), onToggle() {} });
  const times = [];
  for (let k = 0; k < 8; k++) {
    const a = performance.now();
    foldedRows(BIG, outer);
    outerFolds([], new Set());
    view.show(BIG, outer, rows.rowLines, 0, 600);
    times.push(performance.now() - a);
    view.hide();
  }
  // The first run is the warm-up.
  const best = Math.min(...times.slice(1));
  console.log(`# fold of 262,144 bytes (JavaScript, small DOM): ${times.map((t) => `${t.toFixed(1)} ms`).join(", ")}; best of 7 ${best.toFixed(1)} ms`);
  assert.ok(best <= 300, `the least of 7 runs is ${best.toFixed(1)} ms`);
});
