// The text that a copy gives for rows of cells (9.8e). A row of the logs panel is a line of flex cells (the time, the level, the body). The copy of
// the browser puts each cell on its own line, so one log line becomes three lines of text. This module builds the text from the cells that the
// selection touches: one text line for each row, the cells joined by one space, and one line end between the rows. A cell that the selection
// cuts gives its selected part only.
//
// A page marks a list with `data-copy-rows` and each row of it with `data-copy-row`. The cells are the children of the row. A selection that is
// inside one cell, a selection that leaves the list and a selection with no row use the copy of the browser. A table keeps its own copy of the
// browser, with the cells separated by a tab. The pure functions are the model: joinRows.

// The rows as lists of cell texts: one text line for each row, the empty cells left out, and one line end between the rows.
export function joinRows(rows) {
  return rows.map((cells) => cells.filter((cell) => cell !== "").join(" ")).join("\n");
}

const elementOf = (node) => (node && node.nodeType === 3 ? node.parentElement : node);

// The part of the text of a cell that the range holds.
function clipped(doc, cell, range) {
  const part = doc.createRange();
  part.selectNodeContents(cell);
  if (range.compareBoundaryPoints(Range.START_TO_START, part) > 0) part.setStart(range.startContainer, range.startOffset);
  if (range.compareBoundaryPoints(Range.END_TO_END, part) < 0) part.setEnd(range.endContainer, range.endOffset);
  return part.toString();
}

// The text of a copy for the selection of a document, or null when the copy of the browser applies.
export function selectedRowsText(doc) {
  const selection = doc.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return null;
  const range = selection.getRangeAt(0);
  const common = elementOf(range.commonAncestorContainer);
  const list = common && common.closest ? common.closest("[data-copy-rows]") : null;
  if (!list) return null;
  if (common.closest("[data-copy-skip]")) return null;
  const rows = [...list.querySelectorAll("[data-copy-row]")].filter((row) => range.intersectsNode(row));
  if (rows.length === 0) return null;
  // A selection inside one cell is the text of the cell.
  if (rows.length === 1 && common !== rows[0] && rows[0].contains(common) && common.closest("[data-copy-row] > *")) return null;
  // A cell that is hidden from assistive technology holds a copy of a text that another cell shows (a probe of the width): it is not in the copy.
  const cellsOf = (row) => [...row.children].filter((cell) => cell.getAttribute("aria-hidden") !== "true" && range.intersectsNode(cell));
  return joinRows(rows.map((row) => cellsOf(row).map((cell) => clipped(doc, cell, range))));
}

// The copy listener of the document, made one time. It runs before the listeners of the page (capture).
export function installRowCopy(doc = globalThis.document) {
  if (!doc || doc.__huneRowCopy) return false;
  doc.__huneRowCopy = true;
  doc.addEventListener("copy", (event) => {
    const text = selectedRowsText(doc);
    if (text === null || !event.clipboardData) return;
    event.clipboardData.setData("text/plain", text);
    event.preventDefault();
  }, true);
  return true;
}
