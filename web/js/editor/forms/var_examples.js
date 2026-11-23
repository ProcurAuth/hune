// The SQL examples of a `query` variable and the preview of its options (10.3b).
// The menu "Examples" puts one of three ClickHouse texts in the SQL area. The button "Show options" runs the options query of the draft (6.22).
// This module is in the editor, so it is outside the measure of the page (9.30). It never makes HTML from a string (T79): each text of a
// value, of an option or of an error goes in the page with `textContent`. It never writes a value to a log.
// The functions that make nodes take the `doc` (a document), so a test can give a fake one.

import { buildOptions } from "../../var_options.js";

/** The count of options that the preview lists (10.3b). */
export const PREVIEW_ROWS = 50;

/** The message of the preview of a dashboard that has no uid yet (10.3b). */
export const SAVE_FIRST = "Save the dashboard to show the options.";

/**
 * The three examples. The names `events`, `service`, `timestamp`, `services`, `id` and `name` are names that the person changes.
 * Each text is valid ClickHouse SQL and passes the checks of 7.18. An example with a `hint` shows it under its label.
 */
export const EXAMPLES = Object.freeze([
  Object.freeze({
    label: "Values of the last 3 days, newest first",
    lines: ["SELECT service", "FROM events", "WHERE timestamp >= now() - INTERVAL 3 DAY", "GROUP BY service ORDER BY max(timestamp) DESC LIMIT 1000"],
  }),
  Object.freeze({
    label: "Values in the time range of the dashboard",
    hint: "Set Refresh to On time range change.",
    lines: ["SELECT DISTINCT service", "FROM events", "WHERE $__timeFilter(timestamp)", "ORDER BY service LIMIT 1000"],
  }),
  Object.freeze({
    label: "Value and text, sorted",
    lines: ["SELECT DISTINCT toString(id) AS value, name AS text", "FROM services", "ORDER BY text", "LIMIT 1000"],
  }),
]);

/** The text of the example `n`: its lines joined by a line feed. */
export const exampleText = (n) => EXAMPLES[n].lines.join("\n");

/** The items of the menu (`openMenu` of ui.js). `pick(n)` runs for the example `n`. The hooks are `variable-sql-example-<n>`. */
export const exampleItems = (pick) => EXAMPLES.map((e, n) => ({ label: e.label, testid: `variable-sql-example-${n}`, onSelect: () => pick(n) }));

/** Adds the hint of each example under its label, in the popup that `openMenu` made. The label and the hint are text nodes. */
export function addExampleHints(popup, doc = document) {
  EXAMPLES.forEach((e, n) => {
    if (!e.hint) return;
    const item = popup.querySelector(`[data-testid="variable-sql-example-${n}"]`);
    if (!item) return;
    const box = make(doc, "span", "menu__text");
    box.append(make(doc, "span", null, e.label), make(doc, "span", "menu__hint", e.hint));
    item.replaceChildren(box);
  });
}

/**
 * Puts `text` at the cursor of the text area `input`, in place of the selected text. The area keeps the focus.
 * `execCommand("insertText")` makes one undo step of the browser. If the browser refuses it, the text goes in with `setRangeText`.
 */
export function insertExample(input, text, doc = document) {
  input.focus();
  let done = false;
  try {
    done = doc.execCommand("insertText", false, text) === true;
  } catch {
    done = false;
  }
  if (done) return;
  input.setRangeText(text, input.selectionStart, input.selectionEnd, "end");
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

// --- the preview ---------------------------------------------------------------------------------------------------------------

/** The rows of the preview of the answer `raw` of 6.22: the options with the page rules (9.10), at most 50 rows, and the count that stays out. */
export function previewRows(raw, variable) {
  const options = buildOptions(Array.isArray(raw) ? raw : [], variable);
  return { count: options.length, rows: options.slice(0, PREVIEW_ROWS), more: Math.max(0, options.length - PREVIEW_ROWS) };
}

/** The text of the status line for a count of options. */
export const countText = (n) => `${n} ${n === 1 ? "option" : "options"}`;

/** The text of the status line for an error of the route. An `invalid_document` error gives each pointer and message (6.2). */
export function errorText(error) {
  const list = error && error.extra && Array.isArray(error.extra.errors) ? error.extra.errors : [];
  if (error && error.code === "invalid_document" && list.length) {
    const shown = list.slice(0, 3).map((e) => `${e.pointer || "/"}: ${e.message}`).join(" ");
    return `${error.message || "The document is not valid."} ${shown}${list.length > 3 ? ` And ${list.length - 3} more.` : ""}`;
  }
  const at = error && error.extra && error.extra.line ? ` (line ${error.extra.line}${error.extra.column ? `, column ${error.extra.column}` : ""})` : "";
  return `${(error && error.message) || "The options did not load."}${at}`;
}

function make(doc, tag, className = null, text = null) {
  const node = doc.createElement(tag);
  if (className) node.className = className;
  if (text !== null) node.textContent = text;
  return node;
}

/**
 * The block "Show options": the button, the status line and the list of the first 50 options.
 * `getVariable()` gives the draft variable. `canUse()` tells if the source of the variable is in the list of the person.
 * `hasDashboard()` tells if the dashboard has a uid. `send(signal)` sends the request and gives the answer of 6.22.
 * It returns { node, sync, clear, destroy }: `sync` shows or hides the block, `clear` empties the result (the draft changed).
 */
export function createPreview(doc, { getVariable, canUse, hasDashboard, send, isAbort = () => false }) {
  const button = make(doc, "button", "btn btn--sm", "Show options");
  button.setAttribute("type", "button");
  button.dataset.testid = "variable-form-preview";
  const status = make(doc, "p", "vpreview__status");
  status.setAttribute("aria-live", "polite");
  status.dataset.testid = "variable-preview-status";
  const list = make(doc, "ul", "vpreview__list");
  list.dataset.testid = "variable-preview-list";
  list.hidden = true;
  const node = make(doc, "div", "vpreview");
  node.append(button, status, list);
  let controller = null;
  let serial = 0;

  const say = (text, isError = false) => {
    status.textContent = text;
    status.className = isError ? "vpreview__status field-error" : "vpreview__status";
  };
  const empty = () => {
    list.replaceChildren();
    list.hidden = true;
  };
  const stop = () => {
    serial += 1;
    if (controller) controller.abort();
    controller = null;
    button.disabled = false;
  };
  const clear = () => {
    stop();
    say("");
    empty();
  };

  async function show() {
    if (button.disabled) return;
    empty();
    if (!hasDashboard()) {
      say(SAVE_FIRST);
      return;
    }
    stop();
    const mine = serial;
    controller = typeof AbortController === "function" ? new AbortController() : null;
    button.disabled = true;
    say("Loading the options.");
    try {
      const answer = await send(controller ? controller.signal : undefined);
      if (mine !== serial) return;
      const { count, rows, more } = previewRows(answer && answer.options, getVariable());
      say(countText(count));
      for (const o of rows) {
        const item = make(doc, "li", "vpreview__row");
        item.append(make(doc, "code", null, o.value));
        if (o.text !== o.value) item.append(make(doc, "span", "muted", ` ${o.text}`));
        list.append(item);
      }
      if (more) list.append(make(doc, "li", "vpreview__more muted", `and ${more} more`));
      list.hidden = rows.length === 0;
    } catch (error) {
      if (mine !== serial || isAbort(error)) return;
      say(errorText(error), true);
    } finally {
      if (mine === serial) {
        button.disabled = false;
        controller = null;
      }
    }
  }
  button.addEventListener("click", () => {
    show();
  });
  const sync = () => {
    node.hidden = !canUse();
  };
  sync();
  return { node, sync, clear, destroy: stop };
}
