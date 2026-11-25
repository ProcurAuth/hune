// Select and copy the text of the controls that a browser does not let a person select: the legend items of a chart, the sortable headers of a table
// and the links of the cells. A drag that starts on a button never selects text (a rule of the browser), and a drag that starts on a link drags the
// link. This module makes the selection itself from the pointer, and it keeps the plain click as it is:
//   - a drag over the text selects it, and the click that ends the drag does nothing (no toggle of a series, no sort, no link);
//   - a double click selects a word and a triple click selects the text of the control, with no second click for the control;
//   - the key Ctrl+C or Cmd+C on a control that has the focus, with no selection, copies the text of the control.
// It is one set of listeners on the document, made one time (installCopyText). The pure functions are the model: isDrag, labelOf.

// The controls whose text a person can select. The class names come from the chart engine, the table engine and the table panel.
export const SELECTABLE = ".chartCore__legendItem, .dataTable__sort, .cell-link";

// A pointer that moved at least `threshold` pixels between the press and the release is a drag, not a click.
export function isDrag(x0, y0, x1, y1, threshold = 4) {
  return Math.hypot(x1 - x0, y1 - y0) >= threshold;
}

// The text that a copy gives for a control: its text with the spaces cut.
export function labelOf(node) {
  return String(node.textContent || "").replace(/\s+/g, " ").trim();
}

// The text position under a point, as { node, offset }, or null. Chrome has caretPositionFromPoint, and the older engines have caretRangeFromPoint.
function caretFromPoint(doc, x, y) {
  if (doc.caretPositionFromPoint) {
    const p = doc.caretPositionFromPoint(x, y);
    if (p) return { node: p.offsetNode, offset: p.offset };
  }
  if (doc.caretRangeFromPoint) {
    const r = doc.caretRangeFromPoint(x, y);
    if (r) return { node: r.startContainer, offset: r.startOffset };
  }
  return null;
}

// The element whose text a triple click selects: the label inside the control, or the control.
function textHost(control) {
  return control.querySelector(".vtable__label, span") || control;
}

export function installCopyText(doc = globalThis.document) {
  if (!doc || doc.__huneCopyText) return false;
  doc.__huneCopyText = true;
  let press = null; // { x, y, start, moved }
  let swallow = false; // the next click ends a drag: nobody handles it
  let swallowTimer = 0;

  // The target of a drag of text is a text node: take its parent element.
  const controlAt = (target) => {
    const element = target && target.nodeType === 3 ? target.parentElement : target;
    return element instanceof Element ? element.closest(SELECTABLE) : null;
  };

  doc.addEventListener("pointerdown", (event) => {
    press = null;
    if (event.button !== 0 || event.pointerType === "touch" || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    const control = controlAt(event.target);
    if (!control) return;
    press = { x: event.clientX, y: event.clientY, start: caretFromPoint(doc, event.clientX, event.clientY), moved: false };
  }, true);

  doc.addEventListener("pointermove", (event) => {
    if (!press) return;
    if (!(event.buttons & 1)) {
      press = null;
      return;
    }
    if (!press.moved && !isDrag(press.x, press.y, event.clientX, event.clientY)) return;
    press.moved = true;
    const end = caretFromPoint(doc, event.clientX, event.clientY);
    if (press.start && end) doc.getSelection().setBaseAndExtent(press.start.node, press.start.offset, end.node, end.offset);
  }, true);

  doc.addEventListener("pointerup", () => {
    if (press && press.moved) {
      swallow = true;
      clearTimeout(swallowTimer);
      swallowTimer = setTimeout(() => { swallow = false; }, 100);
    }
    press = null;
  }, true);

  // The click is the first listener of the page (capture on the document): it stops the click that ends a drag, and the second and third click of a
  // multiple click, before the control sees them.
  doc.addEventListener("click", (event) => {
    const control = controlAt(event.target);
    if (swallow) {
      swallow = false;
      clearTimeout(swallowTimer);
      if (control) {
        event.stopImmediatePropagation();
        event.preventDefault();
      }
      return;
    }
    if (!control || event.detail < 2) return;
    event.stopImmediatePropagation();
    event.preventDefault();
    const selection = doc.getSelection();
    if (event.detail === 2) {
      const at = caretFromPoint(doc, event.clientX, event.clientY);
      if (at) {
        selection.collapse(at.node, at.offset);
        selection.modify("move", "backward", "word");
        selection.modify("extend", "forward", "word");
      }
    } else {
      selection.selectAllChildren(textHost(control));
    }
  }, true);

  // A link must not start the drag of the link when the pointer goes down on its text.
  doc.addEventListener("dragstart", (event) => {
    if (controlAt(event.target)) event.preventDefault();
  }, true);

  // Ctrl+C with no selection on a control that has the focus copies the text of the control.
  doc.addEventListener("copy", (event) => {
    const selection = doc.getSelection();
    if (selection && !selection.isCollapsed && selection.toString() !== "") return;
    const active = doc.activeElement;
    const control = active ? controlAt(active) : null;
    if (!control || !event.clipboardData) return;
    event.clipboardData.setData("text/plain", labelOf(control));
    event.preventDefault();
  }, true);

  return true;
}
