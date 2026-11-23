// The dashboard JSON tab (10.4): the JSON of the whole draft, with every error by JSON pointer.

import * as api from "../api.js";
import * as editorRoutes from "./api_editor.js";
import * as busy from "../busy.js";
import { button, clear, el } from "../ui.js";
import { errorLink } from "./form.js";
import { createJsonArea } from "./json_area.js";
import { locatePointer, prettyJson, scanJson } from "./json_check.js";

export const MAX_ERRORS_SHOWN = 100;

// `embedded`: the tab "JSON" of the settings view (10.3b) has its own bar, so this view has no "Back" button and no title.
export function createJsonEditor(E, { embedded = false } = {}) {
  const controller = new AbortController();
  const root = el("section", { class: "jeditor", dataset: { testid: "json-editor" } });
  const frame = createJsonArea({ label: "Dashboard JSON", testid: "json-text", tall: true });
  const area = frame.input;
  const list = el("div", { class: "errors", dataset: { testid: "json-errors" }, role: "region", "aria-label": "Errors of the JSON", hidden: true });
  const status = el("p", { class: "ebar__status", role: "status" });
  let lastCheck = null; // { text, canonical }
  let positions = null;
  let timer = 0;

  const apply = button("Apply", { class: "btn btn--primary", disabled: true, dataset: { testid: "json-apply" } }, "check");
  const format = button("Format", { disabled: true, dataset: { testid: "json-format" } }, "code");
  const checkButton = button("Check", { dataset: { testid: "json-check" } }, "refresh");
  const back = button("Back to the dashboard", { class: "btn btn--ghost", dataset: { testid: "editor-back" }, on: { click: () => E.dispatch("leave") } }, "arrow-up");

  area.value = E.record.json_text !== null && E.record.json_text !== undefined ? E.record.json_text : E.canonical || prettyJson(E.doc);
  frame.paint();

  function showErrors(errors, warnings = []) {
    clear(list);
    list.hidden = errors.length === 0 && warnings.length === 0;
    if (list.hidden) return;
    const ul = el("ul", { class: "error-list" });
    for (const e of errors.slice(0, MAX_ERRORS_SHOWN)) {
      const where = e.line ? ` (line ${e.line}, column ${e.column})` : "";
      const item = errorLink(e.pointer || "(document)", `[${e.code}] ${e.message}${where}`, () => goTo(e));
      ul.append(el("li", {}, item));
    }
    list.append(ul);
    for (const w of warnings) list.append(el("p", { class: "errors__warning", text: `Warning ${w.pointer}: ${w.message}` }));
  }

  // Puts the cursor on the member of an error, or on its line and column.
  function goTo(e) {
    let index = 0;
    if (e.line && !positions) {
      const lines = area.value.split("\n");
      index = lines.slice(0, e.line - 1).reduce((n, l) => n + l.length + 1, 0) + Math.max(0, (e.column || 1) - 1);
    } else if (positions) index = locatePointer(positions, e.pointer);
    area.focus();
    area.setSelectionRange(index, index);
    const before = area.value.slice(0, index).split("\n").length - 1;
    area.scrollTop = Math.max(0, before * 18 - 60);
    area.dispatchEvent(new Event("scroll"));
  }

  async function check() {
    clearTimeout(timer);
    const text = area.value;
    const scan = scanJson(text);
    positions = scan.positions.size ? scan.positions : null;
    lastCheck = null;
    apply.disabled = true;
    format.disabled = true;
    if (!scan.ok) {
      status.textContent = "The JSON is not valid.";
      showErrors(scan.errors);
      return;
    }
    status.textContent = "Checking on the server";
    busy.begin();
    try {
      const answer = await editorRoutes.validateDocument({ uid: E.uid }, text, controller.signal);
      if (area.value !== text) return;
      lastCheck = { text, canonical: answer.canonical };
      apply.disabled = false;
      format.disabled = false;
      status.textContent = "The document is valid.";
      showErrors([], answer.warnings || []);
    } catch (error) {
      if (api.isAbort(error)) return;
      if (error.code === "invalid_document") {
        const errors = (error.extra && error.extra.errors) || [];
        status.textContent = `${errors.length} error${errors.length === 1 ? "" : "s"}.`;
        showErrors(errors);
      } else {
        status.textContent = `The check failed: ${error.message}`;
      }
    } finally {
      busy.end();
    }
  }

  area.addEventListener("input", () => {
    E.setJsonText(area.value);
    lastCheck = null;
    apply.disabled = true;
    format.disabled = true;
    clearTimeout(timer);
    timer = setTimeout(check, 1000);
  });
  checkButton.addEventListener("click", check);
  format.addEventListener("click", () => {
    if (!lastCheck) return;
    area.value = lastCheck.canonical;
    frame.paint();
    E.setJsonText(area.value);
    lastCheck = { text: area.value, canonical: lastCheck.canonical };
  });
  apply.addEventListener("click", () => {
    if (!lastCheck) return;
    const doc = JSON.parse(lastCheck.canonical);
    E.canonical = lastCheck.canonical;
    area.value = lastCheck.canonical;
    frame.paint();
    E.change(doc, { clearJsonText: true });
    status.textContent = "Applied to the draft.";
    apply.disabled = true;
  });

  root.append(
    el("div", { class: "ebar" }, ...(embedded ? [] : [back, el("h2", { class: "ebar__title", text: "Dashboard JSON" })]), status, checkButton, format, apply),
    list, frame.el);
  // The errors of the last save show first (10.9).
  if (E.lastErrors && E.lastErrors.length) {
    showErrors(E.lastErrors);
    status.textContent = "Errors of the last save.";
  }
  check();

  return {
    el: root,
    hasUnapplied: () => E.record.json_text !== null && E.record.json_text !== undefined,
    focusPointer(pointer) {
      const scan = scanJson(area.value);
      if (scan.positions.size) {
        positions = scan.positions;
        goTo({ pointer });
      }
    },
    destroy() {
      clearTimeout(timer);
      controller.abort();
    },
  };
}
