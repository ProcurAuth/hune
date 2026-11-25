// Tests of the copy of rows of cells (9.8e): web/js/panels/row_copy.js builds one text line for each row from the cells that a selection touches, and the
// logs panel, the bar charts and the stylesheet use it. The selection is a fake: a text node is { data }, a cell has one text node, and a range holds
// the offsets of its two ends in these nodes.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { installRowCopy, joinRows, selectedRowsText } from "../../web/js/panels/row_copy.js";

const read = (path) => readFileSync(new URL(`../../web/${path}`, import.meta.url), "utf8");

globalThis.Range = { START_TO_START: 0, END_TO_END: 2 };

test("the rows are one text line each, the cells are joined by one space, the empty cells are left out", () => {
  assert.equal(joinRows([["2026-11-25 10:32:47.000", "WARN", "queue depth"], ["2026-11-25 10:32:23.000", "WARN", "slow query"]]), "2026-11-25 10:32:47.000 WARN queue depth\n2026-11-25 10:32:23.000 WARN slow query");
  assert.equal(joinRows([["", "WARN", "text"]]), "WARN text");
  assert.equal(joinRows([["only"]]), "only");
  assert.equal(joinRows([]), "");
  assert.equal(joinRows([["a", "b"], [], ["c"]]), "a b\n\nc");
});

// A cell with one text node. `hidden` marks a cell that assistive technology does not read.
function cell(text, hidden = false) {
  const node = { data: text, nodeType: 3 };
  const element = { textContent: text, firstChild: node, nodeType: 1, children: [], getAttribute: (name) => (name === "aria-hidden" && hidden ? "true" : null), closest: (sel) => (sel === "[data-copy-row] > *" ? element : null) };
  node.parentElement = element;
  return element;
}

// A row of cells, a list of rows, and a selection from a point to a point. A point is { cell, offset } in the text node of the cell.
function world(rowsText, { skip = false, noList = false } = {}) {
  const rows = rowsText.map((texts) => {
    const cells = texts.map((t) => (Array.isArray(t) ? cell(t[0], t[1]) : cell(t)));
    const row = { children: cells, nodeType: 1, contains: (n) => cells.includes(n), closest: () => null };
    cells.forEach((c) => { const closest = c.closest; c.closest = (sel) => (sel === "[data-copy-rows]" ? null : sel === "[data-copy-skip]" ? null : closest(sel)); c.row = row; });
    return row;
  });
  const list = { nodeType: 1, querySelectorAll: (sel) => (sel === "[data-copy-row]" ? rows : []), closest: (sel) => (sel === "[data-copy-rows]" ? (noList ? null : list) : sel === "[data-copy-skip]" ? (skip ? list : null) : null) };
  for (const row of rows) row.closest = (sel) => (sel === "[data-copy-rows]" ? (noList ? null : list) : sel === "[data-copy-skip]" ? (skip ? row : null) : null);
  return { rows, list };
}

function selection(w, from, to, common) {
  const flat = w.rows.flatMap((row, r) => row.children.map((c, i) => ({ c, r, i })));
  const index = (c) => flat.findIndex((x) => x.c === c);
  const range = {
    startContainer: from.cell.firstChild, startOffset: from.offset, endContainer: to.cell.firstChild, endOffset: to.offset,
    commonAncestorContainer: common,
    intersectsNode: (node) => {
      const lo = index(from.cell), hi = index(to.cell);
      if (w.rows.includes(node)) return node.children.some((c) => index(c) >= lo && index(c) <= hi);
      const at = index(node);
      return at >= lo && at <= hi;
    },
    compareBoundaryPoints: (how, part) => {
      const lo = index(from.cell), hi = index(to.cell), at = index(part.cellRef);
      if (how === 0) return lo < at ? -1 : lo > at ? 1 : from.offset > 0 ? 1 : 0;
      return hi > at ? 1 : hi < at ? -1 : to.offset < part.cellRef.textContent.length ? -1 : 0;
    },
  };
  const doc = {
    getSelection: () => ({ isCollapsed: false, rangeCount: 1, getRangeAt: () => range }),
    createRange: () => {
      const part = { from: 0, to: 0, cellRef: null,
        selectNodeContents(c) { this.cellRef = c; this.from = 0; this.to = c.textContent.length; },
        setStart(node, offset) { this.from = offset; },
        setEnd(node, offset) { this.to = offset; },
        toString() { return this.cellRef.textContent.slice(this.from, this.to); } };
      return part;
    },
  };
  return doc;
}

const LINES = [["2026-11-25 10:32:47.000", "WARN", "queue depth above 80 percent"], ["2026-11-25 10:32:23.000", "ERROR", "slow query"], ["2026-11-25 10:20:15.000", "INFO", "started"]];

test("a selection over whole rows gives one text line for each row", () => {
  const w = world(LINES);
  const doc = selection(w, { cell: w.rows[0].children[0], offset: 0 }, { cell: w.rows[2].children[2], offset: 7 }, w.list);
  assert.equal(selectedRowsText(doc), LINES.map((r) => r.join(" ")).join("\n"));
});

test("a selection that starts or ends inside a cell keeps the partial text of that cell", () => {
  const w = world(LINES);
  const doc = selection(w, { cell: w.rows[0].children[2], offset: 6 }, { cell: w.rows[1].children[0], offset: 10 }, w.list);
  assert.equal(selectedRowsText(doc), "depth above 80 percent\n2026-11-25");
});

test("a selection inside one cell is the copy of the browser", () => {
  const w = world(LINES);
  const body = w.rows[0].children[2];
  const doc = selection(w, { cell: body, offset: 2 }, { cell: body, offset: 9 }, body.firstChild);
  assert.equal(selectedRowsText(doc), null);
});

test("a selection inside one row over two cells is one text line", () => {
  const w = world(LINES);
  const doc = selection(w, { cell: w.rows[1].children[0], offset: 0 }, { cell: w.rows[1].children[2], offset: 10 }, w.rows[1]);
  assert.equal(selectedRowsText(doc), "2026-11-25 10:32:23.000 ERROR slow query");
});

test("a cell that is hidden from assistive technology is not in the copy", () => {
  const w = world([["checkout", ["checkout", true], "47"], ["search", ["search", true], "33"]]);
  const doc = selection(w, { cell: w.rows[0].children[0], offset: 0 }, { cell: w.rows[1].children[2], offset: 2 }, w.list);
  assert.equal(selectedRowsText(doc), "checkout 47\nsearch 33");
});

test("the copy of the browser applies to a selection out of the list, in a skipped part, with no row or with nothing selected", () => {
  const out = world(LINES, { noList: true });
  assert.equal(selectedRowsText(selection(out, { cell: out.rows[0].children[0], offset: 0 }, { cell: out.rows[1].children[2], offset: 3 }, out.list)), null);
  const skipped = world(LINES, { skip: true });
  assert.equal(selectedRowsText(selection(skipped, { cell: skipped.rows[0].children[0], offset: 0 }, { cell: skipped.rows[1].children[2], offset: 3 }, skipped.list)), null);
  const none = { getSelection: () => ({ isCollapsed: true, rangeCount: 0 }) };
  assert.equal(selectedRowsText(none), null);
  assert.equal(selectedRowsText({ getSelection: () => null }), null);
});

test("the listener of the copy is made one time and puts the text in the clipboard of the event", () => {
  const w = world(LINES);
  const doc = selection(w, { cell: w.rows[0].children[0], offset: 0 }, { cell: w.rows[1].children[2], offset: 10 }, w.list);
  let handler = null;
  doc.addEventListener = (type, fn, capture) => { assert.equal(type, "copy"); assert.equal(capture, true); handler = fn; };
  assert.equal(installRowCopy(doc), true);
  assert.equal(installRowCopy(doc), false);
  const data = {};
  let prevented = false;
  handler({ clipboardData: { setData: (type, text) => { data[type] = text; } }, preventDefault: () => { prevented = true; } });
  assert.equal(data["text/plain"], `${LINES[0].join(" ")}\n${LINES[1].join(" ")}`);
  assert.equal(prevented, true);
  // No clipboard on the event: nothing happens.
  prevented = false;
  handler({ preventDefault: () => { prevented = true; } });
  assert.equal(prevented, false);
});

test("the logs panel marks its rows, selects a line on a triple click and gives the toggle of a double click back", () => {
  const logs = read("js/panels/logs.js");
  assert.match(logs, /list\.dataset\.copyRows = "1";/);
  assert.match(logs, /row\.dataset\.copyRow = "1";/);
  assert.match(logs, /box\.dataset\.copySkip = "1";/);
  assert.match(logs, /if \(e\.detail >= 3\) \{[^}]*selection\.selectAllChildren\(row\);/s);
  assert.match(logs, /if \(e\.detail === 2\) \{\s*if \(toggledByFirst === row\) toggle\(row\);/);
  assert.match(logs, /selection\.selectAllChildren\(list\);/);
  assert.match(logs, /list\.addEventListener\("keydown", onSelectAll\);/);
  assert.match(read("js/panels/barchart_h.js"), /dataset: \{ copyRows: "1" \}/);
  assert.match(read("js/panels/bargauge.js"), /copyRows: "1"/);
  assert.match(read("js/panel_frame.js"), /^installRowCopy\(\);$/m);
});

test("the cursor over text is the text cursor, the stylesheet loads after the panel stylesheets, and nothing stops the menu of the browser", () => {
  const css = read("css/panel-select.css");
  assert.match(css, /\.logs__line, \.logs__line\[role="button"\], \.logs__line > span/);
  assert.match(css, /\.chartCore__legendItem > span/);
  assert.match(css, /\.vtable td/);
  assert.match(css, /cursor: text;/);
  assert.match(css, /\.edit-grid \.panel__head, \.edit-grid \.panel__head \.panel__title \{ cursor: move; \}/);
  assert.ok(!/\.dataTable__sort[^{]*\{[^}]*cursor: text/.test(css), "a sortable header keeps the pointer");
  const shell = read("shell.html");
  assert.ok(shell.indexOf("panel-select.css") > shell.indexOf("panel-logs.css"), "panel-select.css comes after panel-logs.css, which sets the pointer");
  assert.ok(shell.indexOf("panel-select.css") > shell.indexOf("panel-geomap.css"));
  // No code of the page calls preventDefault on the event contextmenu.
  for (const file of ["js/panels/copy_text.js", "js/panels/text_layer.js", "js/panels/row_copy.js", "js/panels/logs.js"]) assert.ok(!/contextmenu/.test(read(file)), `${file} has no handler of contextmenu`);
});
