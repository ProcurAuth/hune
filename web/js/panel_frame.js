// The panel frame (9.11): header, menu, states, "View data". It prepares the fields of a panel
// (reuse, long-to-wide, transforms, field configuration) and gives them to the panel type.

import { marks, noteOf } from "./annotations.js";
import * as busy from "./busy.js";
import { makeDisplay } from "./display.js";
import { computeFieldConfig } from "./fields.js";
import { buildLink } from "./links.js";
import { baseDisplayNames, displayNames } from "./names.js";
import { noteTooltip, panelRange, timeNote } from "./panel_time.js";
import { buildPreview, observePreview } from "./preview/index.js";
import { rangeKey, sameFrames, WAITING, waitMode } from "./panel_wait.js";
import { mentionsVariable } from "./repeat.js";
import { badgeText, failedRefs, failedSlots, firstError, hasExtra, maxRows, slotsOf } from "./queries.js";
import { preparePanelFrames } from "./transform.js";
import { app } from "./config.js";
import { allRows, buildTable } from "./panels/annotate.js";
import { createText } from "./panels/text.js";
import { createVariableFrame } from "./panels/variable.js";
import { installCopyText } from "./panels/copy_text.js";
import { installRowCopy } from "./panels/row_copy.js";
import { installTextLayer } from "./panels/text_layer.js";
import { button, clear, copyText, el, emptyState, icon, notify, openDialog, openMenu } from "./ui.js";

// A person can select and copy the text of a panel (9.8e): the controls that a browser does not let one select (legend items, sortable headers,
// links of cells) and the text that a chart draws on its canvas. Both are made one time for the page, before the first panel draws.
installCopyText();
installTextLayer();
installRowCopy();

const LOADERS = {
  table: () => import("./panels/table.js"),
  timeseries: () => import("./panels/timeseries.js"),
  stat: () => import("./panels/stat.js"),
  barchart: () => import("./panels/barchart.js"),
  piechart: () => import("./panels/piechart.js"),
  xychart: () => import("./panels/xychart.js"),
  bargauge: () => import("./panels/bargauge.js"),
  state_timeline: () => import("./panels/timeline.js"),
  status_history: () => import("./panels/timeline.js"),
  heatmap: () => import("./panels/heatmap.js"),
  histogram: () => import("./panels/histogram.js"),
  logs: () => import("./panels/logs.js"),
  gauge: () => import("./panels/gauge.js"),
  geomap: () => import("./panels/geomap.js"),
};

const VALUE_TYPES = new Set(["stat", "bargauge", "piechart"]);
const VIEW_DATA_CAP = 5000;
const PROGRESS_MS = 300;
// A panel that waits (no result yet, queued or loading) shows the dots of its loader, and the preview of its type when it has nothing on the
// screen, after this time only: a result that comes sooner replaces the empty frame with no flash of the loader.
const WAIT_MS = 300;
// The icons of the panel menu: the three own items, and the items that the editor adds (found by their label).
const EXTRA_ICONS = { Edit: "edit", Duplicate: "copy", "Move up": "arrow-up", "Move down": "arrow-down", Delete: "trash" };

// The data attributes of the frame element: the test hooks (9.29), and `data-transparent` (present, empty) when the
// panel has `transparent` = true (4.13). The CSS hook `.panel[data-transparent]` removes the fill and the edge at rest.
// A copy of a repeat (9.8c) has the key of its instance in the hook, and the number of the copy and the count of copies (9.11).
export function frameDataset(panel, instance = null) {
  const copy = instance && instance.n !== null && instance.n !== undefined ? { repeatN: String(instance.n), repeatCount: String(instance.count) } : {};
  return { testid: `panel-${instance ? instance.key : panel.id}`, state: "idle", type: panel.type, ...copy, ...(panel.transparent === true ? { transparent: "" } : {}) };
}

// --- CSV of "View data" (9.11) ----------------------------------------------------------------------------------
// A text cell that starts with `=`, `+`, `-`, `@`, TAB or CR gets a `'` before its first character, so that a spreadsheet reads text and
// never a formula. A NUMBER (the display text of a numeric field: a number and a short unit) is written as it is: the sign of a
// negative number is not a formula. Then the quote rule applies.
const FORMULA_START = /^[=+\-@\t\r]/;
const NUMBER_TEXT = /^[+-]?\d[\d.,]*(?:[eE][+-]?\d+)?(?: ?[A-Za-z\u00B5%\u00B0/]+(?:[ /][A-Za-z\u00B5%]+)*)?$/;

/** One CSV field. `numeric` is true for a field of the kind number or number_text. */
export function csvCell(text, numeric = false) {
  let out = String(text);
  if (FORMULA_START.test(out) && !(numeric && NUMBER_TEXT.test(out))) out = `'${out}`;
  return /["\r\n,]/.test(out) ? `"${out.replace(/"/g, '""')}"` : out;
}

/** The text of the file: the display names, then one row for each row of the data. `cell(f, r)` gives the display text. */
export function csvText(names, fields, cell, rows) {
  const numeric = fields.map((f) => f.kind === "number" || f.kind === "number_text");
  const lines = [names.map((n) => csvCell(n)).join(",")];
  for (let r = 0; r < rows; r++) lines.push(fields.map((f, i) => csvCell(cell(i, r), numeric[i])).join(","));
  return `${lines.join("\r\n")}\r\n`;
}

// --- "Inspect" (9.11, F22) ----------------------------------------------------------------------------------

/** True when the session says that the server allows the item "Inspect": the key `server.query_inspect` is `true` (6.10). */
export function inspectAllowed(session) {
  return !!session && session.query_inspect === true;
}

/**
 * What the item "Inspect" shows for the answer of a query (6.20). `sql` is the expanded SQL text, or `null` when the answer has
 * no `sql` member: the server gives it only to a subject that may see it. Any value that is not a text counts as absent.
 */
export function inspectModel(frame) {
  if (!frame || typeof frame !== "object") return null;
  return {
    sql: typeof frame.sql === "string" ? frame.sql : null,
    queryId: typeof frame.query_id === "string" ? frame.query_id : "",
    intervalMs: Number.isInteger(frame.interval_ms) ? frame.interval_ms : null,
    rows: Number.isInteger(frame.rows) ? frame.rows : null,
  };
}

/** The name of the file: the title, `-data-`, the UTC time and `.csv`. A character outside `A-Za-z0-9 _.-` becomes `_`. */
export function csvFileName(title, now = new Date()) {
  const safe = Array.from(title).map((c) => (/[A-Za-z0-9 _.-]/.test(c) ? c : "_")).join("");
  const p = (n) => String(n).padStart(2, "0");
  const stamp = `${now.getUTCFullYear()}${p(now.getUTCMonth() + 1)}${p(now.getUTCDate())}-${p(now.getUTCHours())}${p(now.getUTCMinutes())}${p(now.getUTCSeconds())}`;
  return `${safe === "" ? "panel" : safe}-data-${stamp}.csv`;
}

// page: { rt, zone, linkContext(), viewPanel(key), panelUrl(key), syncKey, onZoom(a, b), extraMenuItems(panel, instance), noView }
// `instance` is the instance of the plan (repeat.js): its key is the id of the panel, or `<id>-r<n>` for a copy of a repeat (9.8c).
// `key` names a copy that has no instance yet (the view of one panel, 10.2a rule 16). `lookup()` gives the record in place of the runtime (the preview of the panel editor).
export function createPanelFrame({ panel, page, instance: instArg = null, key: keyArg = null, lookup = null }) {
  // The placed control of a variable (4.20, 9.10) has its own frame and no query. The grid gives it as a panel of type `variable`.
  if (panel.type === "variable") return createVariableFrame({ panel, page });
  const rt = page.rt;
  const g = panel.grid;
  // A text panel (9.19b) has no query and no data frame: no state of the scheduler, and the menu has "View" only.
  const isText = panel.type === "text";
  const key = keyArg !== null ? keyArg : instArg ? instArg.key : panel.id;
  const record = () => (lookup ? lookup() : rt.panels && rt.panels.get(key)) || null;
  // The view of a copy has the key only: the instance of its record (or the key alone while the options load) gives the hooks of 9.11.
  const inst = instArg || (keyArg !== null ? (record() && record().inst ? { ...record().inst, key: keyArg } : { key: keyArg }) : null);
  const titleId = `panel-title-${key}`;
  // A query error: a red warning icon left of the title. Its message is the tooltip (filled in showError).
  const statusIcon = el("span", { class: "panel__status" });
  const head = el("header", { class: "panel__head" }, statusIcon);
  // The title, the description and the link texts show after the replacement of the variables (9.13). The page writes the result with
  // textContent only. A runtime without `interpolate` (the history view) shows the stored text.
  const text = (value) => (typeof rt.interpolate === "function" ? rt.interpolate(value, key) : value);
  // A copy of a repeat has the option text in its accessible name when its title does not name the variable (9.26).
  const copyOf = () => {
    const rec = record();
    return rec && rec.variable && rec.value !== null ? rec : inst && inst.n !== null && inst.variable ? inst : null;
  };
  const titleNow = () => text(panel.title);
  const titleEl = panel.title ? el("h3", { class: "panel__title", id: titleId }) : el("span", { class: "panel__title" });
  head.append(titleEl);
  const infoEl = panel.description ? el("span", { class: "panel__info", tabIndex: 0, role: "img", "data-tooltip-align": "start" }, icon("info", 16)) : null;
  // The description is the tooltip of the icon (a CSS tooltip: it shows on hover and on keyboard focus, and opens rightwards from the icon).
  if (infoEl) head.append(infoEl);
  // The links button is a title item (it follows the title and the info icon); the spacer pushes the menu to the right edge.
  if (panel.links && panel.links.length) {
    const linksButton = button("Panel links", { dataset: { testid: `panel-${key}-links` }, "aria-haspopup": "menu", "aria-expanded": "false", "data-tooltip-align": "end" }, "link", { iconOnly: true, variant: "ghost" });
    linksButton.addEventListener("click", () => {
      const items = panel.links.map((l) => buildLink(l, { ...page.linkContext(), interpolate: text })).filter((b) => b.href !== null).map((b) => ({ label: b.title, href: b.href, blank: b.target_blank, external: true }));
      openMenu(linksButton, items, { align: "end" });
    });
    head.append(linksButton);
  }
  const spacer = el("span", { class: "panel__spacer", "aria-hidden": "true" });
  head.append(spacer);
  // The accessible name names the panel. The tooltip is short.
  const menuButton = button(`Panel menu: ${titleNow() || `panel ${panel.id}`}`, { dataset: { testid: `panel-${key}-menu` }, "aria-haspopup": "menu", "aria-expanded": "false", "data-tooltip": "Menu", "data-tooltip-align": "end" }, "more", { iconOnly: true, variant: "ghost" });
  menuButton.classList.add("panel__menu");
  head.append(menuButton);
  let copyName = null; // the accessible name of a copy of a repeat (9.26)
  let textView = null; // a text panel: the parsed text, and the values that the text named at the last draw (9.19b)
  let textSig = null;
  let rootMade = false;
  // Writes the texts that hold variables. The page calls it again when a selection changes.
  function paintTexts() {
    const title = titleNow();
    if (isText && rootMade) paintText(false);
    if (panel.title) {
      if (titleEl.textContent !== title) titleEl.textContent = title;
      titleEl.title = title;
    }
    if (infoEl) {
      const description = text(panel.description);
      infoEl.setAttribute("data-tooltip", description);
      infoEl.setAttribute("aria-label", `Description: ${description}`);
    }
    menuButton.setAttribute("aria-label", `Panel menu: ${title || `panel ${panel.id}`}`);
    const copy = copyOf();
    copyName = copy ? (mentionsVariable(panel.title, copy.variable) ? title : `${title || `Panel ${panel.id}`}, ${copy.text}`) : null;
    if (rootMade && copyName !== null) root.setAttribute("aria-label", copyName);
    paintNote();
  }

  // --- the note of the time override (9.8b, F18) ---------------------------------------------------------------------------------
  // The note shows after the title and its icons. A reuse panel shows the note of its source (9.8b, step 9). The node exists only while the
  // note has a text: `hide_time_override` or a panel with no override has no node.
  let noteEl = null;
  let noteText = null;
  let annotEl = null; // the note of the annotations (9.19d, rule 16)

  function noteQuery() {
    if (isText) return null;
    const reuse = panel.data.reuse;
    if (!reuse) return panel.data.query || null;
    const source = rt.panels && rt.panels.get(reuse.panel);
    return source && source.panel.data.query ? source.panel.data.query : null;
  }

  // The range that the panel shows: the one of the runtime, or (the draft grid of the editor) the one that 9.8b gives for the draft query.
  function noteRange(query) {
    if (typeof rt.rangeOf === "function") {
      const r = rt.rangeOf(key);
      return r && !r.error ? r : null;
    }
    const d = rt.range;
    if (!d || !Number.isFinite(d.from_ms) || !Number.isFinite(d.to_ms)) return null;
    const r = panelRange(query, { from: d.from_ms, to: d.to_ms, fromText: d.fromText, toText: d.toText }, Date.now(), page.zone);
    return r.error ? null : { from_ms: r.from, to_ms: r.to };
  }

  function paintNote() {
    const query = noteQuery();
    const dash = rt.range && rt.range.fromText !== undefined ? { fromText: rt.range.fromText, toText: rt.range.toText } : null;
    const next = query ? timeNote(query, page.zone, dash) : null;
    if (next === null) {
      if (noteEl) noteEl.remove();
      noteEl = null;
      noteText = null;
      return;
    }
    if (!noteEl) {
      noteText = el("span", { class: "panel__time-text" });
      noteEl = el("span", { class: "panel__time-note", tabIndex: 0, role: "note", "data-tooltip-align": "start", dataset: { testid: `panel-${key}-time-note` } }, icon("clock", 14), noteText);
      head.insertBefore(noteEl, annotEl || spacer);
    }
    if (noteText.textContent !== next) noteText.textContent = next;
    const range = noteRange(query);
    const tip = range ? noteTooltip(range.from_ms, range.to_ms, page.zone) : "";
    if (tip) noteEl.setAttribute("data-tooltip", tip);
    else noteEl.removeAttribute("data-tooltip");
    noteEl.setAttribute("aria-label", tip ? `${next}, ${tip}` : next);
  }

  // The records of the annotations that draw on this panel (9.19d). A runtime without them and the draft grid (`noMarks`) give none.
  const annotationRecords = () => (panel.type === "timeseries" && !page.noMarks && typeof rt.annotationsFor === "function" ? rt.annotationsFor(panel.id) : []);

  // The note of the title bar: the cap, or the annotations that failed. The tooltip lists the names and the messages. It follows the time note.
  function paintAnnotNote() {
    const records = annotationRecords();
    const note = records.length ? noteOf(marks(records, -Infinity, Infinity)) : null;
    if (!note) {
      if (annotEl) annotEl.remove();
      annotEl = null;
      return;
    }
    if (!annotEl) {
      annotEl = el("span", { class: "panel__annot-note", tabIndex: 0, role: "note", "data-tooltip-align": "start", dataset: { testid: `panel-${key}-annot-note` } }, icon("warning", 14), el("span", { class: "panel__time-text" }));
      head.insertBefore(annotEl, spacer);
    }
    annotEl.lastChild.textContent = note.text;
    annotEl.setAttribute("data-tooltip", note.tip);
    annotEl.setAttribute("aria-label", `${note.text}. ${note.tip}`);
  }
  paintTexts();
  const content = el("div", { class: "panel__content" });
  const overlay = el("div", { class: "panel__overlay" });
  // The loader: three dots at the top right of the title bar, left of the menu. CSS shows them (panel-loading.css, classes `panel--wait` and
  // `panel--progress`). They take no room, take no click and have no accessible name. The preview of the panel type is in the body, under the
  // overlay, so an error, a hint or a note hides it. Only a panel with nothing on the screen shows it (`data-wait="first"`).
  const loader = el("div", { class: "panel__loader", "aria-hidden": "true" }, el("i"), el("i"), el("i"));
  head.append(loader);
  const skeleton = buildPreview(panel);
  const stopPreview = skeleton ? observePreview(skeleton, panel, rt.doc, { rt, page, key }) : () => {};
  const body = el("div", { class: "panel__body" }, content, skeleton, overlay);
  const root = el("section", {
    class: "panel",
    dataset: frameDataset(panel, inst),
    ...(panel.title && !(inst && inst.n !== null) ? { "aria-labelledby": titleId } : { "aria-label": `Panel ${panel.id}` }),
  }, head, body);
  rootMade = true;
  if (copyName !== null) root.setAttribute("aria-label", copyName);
  root.style.gridColumn = `${g.x + 1} / span ${g.w}`;
  root.style.gridRow = `${g.y + 1} / span ${g.h}`;
  // The height in rows: the stacked layout of a narrow screen uses it as the floor of the panel height (dashboard.css).
  root.style.setProperty("--panel-h", String(g.h));
  // The width class for the two-column layout of a tablet: a panel up to half of the grid takes one column.
  root.dataset.span = g.w <= 12 ? "half" : "full";

  let instance = null;
  let rendered = null; // the answers (the frames) that are drawn
  let renderedRange = ""; // the range of the panel at the last drawing (see `rangeKey`)
  let last = null; // { fields, names, baseNames } after the transformations, for "View data"
  let progressTimer = 0;
  let waitTimer = 0;
  let suspended = false;
  let pending = false;
  let destroyed = false;
  let token = 0;
  let badge = ""; // the text of the badge of the failed queries (9.8d): "Error:" and one line for each failed query
  let failedMany = 0; // the number of failed queries of the last update
  let frameRef = null; // the ref that a table with several queries shows (9.22a, rule 9)
  let lastInfo = null; // the data of the last draw, for the select of a table
  let keepFocus = false; // the select of a table keeps the focus through its own redraw

  // A time series draws its x axis over the range that its context gives (timeseries.js). For a panel with a time override that is the range of
  // the panel (9.8b), not the range of the dashboard. The other panel types read the context for their data links only: they keep the range
  // of the dashboard, which is the range of the tokens `${__from}` and `${__to}` (9.8b, step 8).
  const drawsRange = panel.type === "timeseries" || panel.type === "state_timeline" || panel.type === "status_history" || panel.type === "heatmap";
  const chartContext = drawsRange && typeof rt.rangeOf === "function"
    ? () => {
      const r = rt.rangeOf(key);
      const base = page.linkContext();
      return r && !r.error ? { ...base, from: r.from_ms, to: r.to_ms } : base;
    }
    : page.linkContext;

  // The data of this panel: its own, or the data of its source (9.23). `slots` are the runs of the visible queries (9.8d) and `items` the
  // answers of the slots that have a frame, in the order of the queries.
  function resolve() {
    const own = record() || { id: panel.id, panel, state: "idle", frame: null, error: null };
    const reuse = panel.data.reuse;
    const source = reuse ? rt.panels.get(reuse.panel) : null;
    if (reuse && !source) return { p: own, source: null, state: "error", slots: [], items: [], error: new Error("The source panel is not in the dashboard."), retryId: panel.id };
    const rec = source || own;
    const slots = slotsOf(rec);
    const failedNow = failedSlots(slots).length > 0 && rec.state === "error";
    let error = failedNow ? firstError(slots, hasExtra(rec.panel)) : rec.error;
    if (source && error) {
      const wrapped = new Error(`The source panel ${text(source.panel.title) || source.id} failed: ${error.message}`);
      wrapped.code = error.code;
      error = wrapped;
    }
    const items = slots.filter((slot) => slot.frame).map((slot) => ({ ref: slot.ref, frame: slot.frame }));
    return { p: own, source, state: rec.state, slots, items, error, retryId: source ? source.id : key };
  }

  // What a waiting panel shows (9.11): `first` (nothing on the screen: the preview and the dots) or `reload` (the earlier content stays: the
  // dots only). The attribute is set with the state. The classes below show the signs after 300 ms.
  function paintWait(state) {
    const shown = content.firstChild ? "content" : overlay.dataset.kind === "empty" && !overlay.hidden ? "empty" : "nothing";
    const mode = waitMode(state, shown);
    if (mode) root.dataset.wait = mode;
    else delete root.dataset.wait;
  }

  function setRootState(state) {
    root.dataset.state = state;
    paintWait(state);
    clearTimeout(progressTimer);
    root.classList.remove("panel--progress");
    if (state === "loading") progressTimer = setTimeout(() => root.classList.add("panel--progress"), PROGRESS_MS);
    // The wait class starts with the first waiting state and stays through idle, queued and loading (the timer does not restart).
    if (WAITING.has(state)) {
      if (!waitTimer && !root.classList.contains("panel--wait")) waitTimer = setTimeout(() => root.classList.add("panel--wait"), WAIT_MS);
    } else {
      clearTimeout(waitTimer);
      waitTimer = 0;
      root.classList.remove("panel--wait");
    }
    clear(statusIcon);
    for (const name of ["tabindex", "role", "aria-label", "data-tooltip", "data-tooltip-align", "data-testid"]) statusIcon.removeAttribute(name);
    // A partial result (9.8d) has the badge with the state `ready`, `empty` or another state that is not `error`.
    if (badge && state !== "error") paintBadge(badge, badge);
  }

  // The red badge left of the title: the tooltip and the accessible name hold the text, the key hook is `panel-<key>-status` (9.29).
  function paintBadge(tip, label) {
    clear(statusIcon);
    statusIcon.append(icon("warning", 16));
    statusIcon.setAttribute("tabindex", "0");
    statusIcon.setAttribute("role", "img");
    statusIcon.setAttribute("aria-label", label);
    statusIcon.setAttribute("data-tooltip", tip);
    statusIcon.setAttribute("data-tooltip-align", "start");
    statusIcon.setAttribute("data-testid", `panel-${key}-status`);
  }

  function showOverlay(kind, node) {
    clear(overlay);
    overlay.dataset.kind = kind;
    if (node) overlay.append(node);
    overlay.hidden = !node;
  }

  function showError(message, code, retryId) {
    destroyInstance();
    clear(content);
    const headLine = el("div", { class: "panel__error-head" }, icon("error", 14), code ? el("code", { class: "panel__error-code", text: code }) : el("span", { text: "Error" }));
    const text = el("p", { class: "panel__error-text", text: message });
    const retry = button("Retry", { dataset: { testid: `panel-${key}-retry` }, on: { click: () => rt.retry(retryId) } }, "refresh", { size: "sm" });
    const box = el("div", { class: "panel__error", role: "alert", dataset: { testid: `panel-${key}-error` } }, headLine, text, retry);
    showOverlay("error", box);
    setRootState("error");
    // The icon left of the title carries the message as its tooltip (and as its name); the body keeps the full error and Retry.
    // A panel with several failed queries lists each one (9.8d).
    if (failedMany > 1) paintBadge(badge, badge);
    else paintBadge(message, `Error: ${message}`);
  }

  function destroyInstance() {
    if (instance) {
      try {
        instance.destroy();
      } catch (error) {
        console.error(error);
      }
      instance = null;
    }
  }

  async function draw(info) {
    const mine = ++token;
    lastInfo = info;
    // A type that this page cannot draw yet (the format knows more types than LOADERS) gets a fixed message, never the text of an exception.
    if (!Object.hasOwn(LOADERS, panel.type)) {
      showError("This panel type is not available in this version of the page.", null, info.retryId);
      return;
    }
    let prepared;
    let lists;
    try {
      const made = preparePanelFrames(panel, info.items, info.source ? info.source.panel : null, frameRef);
      lists = made.lists;
      frameRef = lists.some((l) => l.ref === frameRef) ? frameRef : lists.length ? lists[0].ref : null;
      prepared = computeFieldConfig(made.fields, panel.fields, panel.type);
    } catch (error) {
      showError(error.message, null, info.retryId);
      return;
    }
    const { fields: preparedFields, warnings } = prepared;
    const rows = maxRows(preparedFields);
    // A stat or a bar gauge with no row has one item, "No data" (9.14), as Grafana draws it: the panel gets its first value field only, so
    // that it does not draw one "No data" for each column.
    const fields = (panel.type === "stat" || panel.type === "bargauge" || panel.type === "gauge") && rows === 0 && preparedFields.length > 1
      ? [preparedFields.find((f) => f.kind === "number" || f.kind === "number_text") || preparedFields[0]]
      : preparedFields;
    const noValue = fields.length && fields[0].config.no_value;
    last = { fields: preparedFields, names: displayNames(preparedFields), baseNames: baseDisplayNames(preparedFields) };
    // The module of the type loads before the old drawing goes. A reload then swaps the old drawing for the new one in one step: no frame is
    // painted with an empty body between them (9.11).
    let mod = null;
    if (rows > 0 || panel.type === "stat" || panel.type === "bargauge" || panel.type === "gauge" || VALUE_TYPES.has(panel.type)) {
      busy.begin();
      try {
        mod = await LOADERS[panel.type]();
      } catch (error) {
        console.error(error);
        showError(error.message, null, info.retryId);
        return;
      } finally {
        busy.end();
      }
      if (destroyed || mine !== token) return;
    }
    destroyInstance();
    clear(content);
    // A stat and a bar gauge draw their own "No data" for zero rows (9.14), as Grafana does: a big text in the colour of the base step.
    const ownEmpty = panel.type === "stat" || panel.type === "bargauge" || panel.type === "gauge" || (VALUE_TYPES.has(panel.type) && noValue);
    if (rows === 0 && !ownEmpty) {
      showOverlay("empty", emptyState({ icon: null, title: "No data", inline: true }));
      setRootState("empty");
      return;
    }
    showOverlay("none", null);
    busy.begin();
    try {
      if (destroyed || mine !== token) return;
      if (warnings.length) content.append(el("p", { class: "panel__warning", role: "status" }, icon("warning", 14), ` ${warnings.join(" ")}`));
      const holder = el("div", { class: "panel__plot" });
      content.append(holder);
      if (panel.type === "table" && lists.length > 1) content.append(framesSelect(lists));
      instance = mod.render(holder, {
        panel,
        fields,
        names: fields === preparedFields ? last.names : displayNames(fields),
        baseNames: fields === preparedFields ? last.baseNames : baseDisplayNames(fields),
        zone: page.zone,
        syncKey: panel.type === "timeseries" ? page.syncKey : "",
        onZoom: (a, b) => page.onZoom(a, b),
        linkContext: chartContext,
        key,
        annotationsOf: panel.type === "timeseries" && typeof rt.annotationsFor === "function" ? annotationRecords : undefined,
        // A timeline that breaks a cap (9.19c) tells the frame; the frame has no "Retry" for it.
        setState: (state) => {
          if (!destroyed && mine === token) setRootState(state);
        },
      });
      setRootState(instance.state || (rows === 0 && !noValue ? "empty" : "ready"));
    } catch (error) {
      console.error(error);
      showError(error.message, null, info.retryId);
    } finally {
      busy.end();
    }
  }

  // The select of a table with several queries (9.22a, rule 9): one option for each frame, with the ref as its text. A change draws the table again.
  function framesSelect(lists) {
    const select = el("select", { class: "input", id: `panel-frames-${key}`, dataset: { testid: `panel-${key}-frames` } }, ...lists.map((l) => el("option", { value: l.ref, text: l.ref, selected: l.ref === frameRef })));
    select.addEventListener("change", () => {
      frameRef = select.value;
      keepFocus = true;
      draw(lastInfo);
    });
    if (keepFocus) {
      keepFocus = false;
      queueMicrotask(() => select.focus());
    }
    return el("label", { class: "panel__frames", for: select.id }, el("span", { text: "Query" }), select);
  }

  // A frame of a repeat that has no copy (9.8c): the message shows in the body, and the frame sends no query.
  function showNote(note) {
    destroyInstance();
    clear(content);
    rendered = null;
    last = null;
    showOverlay("note", el("p", { class: "panel__note", role: "note", dataset: { testid: `repeat-note-${panel.id}` }, text: note.message }));
    setRootState(note.state);
  }

  function update(force = false) {
    if (destroyed) return;
    if (suspended) {
      pending = true;
      return;
    }
    if (isText) {
      paintText(force);
      return;
    }
    paintNote();
    paintAnnotNote();
    const info = resolve();
    const failed = failedSlots(info.slots);
    badge = failed.length ? badgeText(info.slots) : "";
    failedMany = failed.length;
    if (failed.length) root.dataset.failed = failedRefs(info.slots);
    else delete root.dataset.failed;
    // A repeat note is an object { state, message } (9.8c). The note of the history and of the editor is a text: the hint below shows it.
    if (info.p && info.p.note && typeof info.p.note === "object") {
      showNote(info.p.note);
      return;
    }
    if (overlay.dataset.kind === "note") showOverlay("none", null);
    if (info.state === "empty") {
      // No visible query (9.8d): the text "No data". The panel sends no request.
      destroyInstance();
      clear(content);
      rendered = null;
      last = { fields: [], names: [], baseNames: [] };
      showOverlay("empty", emptyState({ icon: null, title: "No data", inline: true }));
      setRootState("empty");
      return;
    }
    if (info.state === "error" && info.error) {
      rendered = null;
      showError(info.error.message, info.error.code, info.retryId);
      return;
    }
    if (info.state === "ready" && info.items.length) {
      // A new answer with the same data and the same range draws nothing again: the panel does not blink at a refresh. A chart draws its
      // x axis over the range, so a new range draws again.
      const at = drawsRange && typeof rt.rangeOf === "function" ? rangeKey(rt.rangeOf(key)) : "";
      if (force || at !== renderedRange || !sameFrames(rendered, info.items)) {
        rendered = info.items;
        renderedRange = at;
        draw(info);
      } else setRootState(["empty", "error", "timeline-limit", "bucket-limit", "geo-limit"].includes(root.dataset.state) ? root.dataset.state : "ready");
      return;
    }
    // idle, queued or loading: the earlier content stays until the new result comes.
    if (info.state === "loading" || info.state === "queued") {
      if (root.dataset.state === "error" || overlay.dataset.kind === "hint") showOverlay("none", null);
      setRootState(info.state);
    } else if (!info.items.length) {
      setRootState("idle");
      // The editor: a draft query that did not run (10.2), or a version that shows no data (10.11).
      const rec = info.p;
      if (rec && (rec.needsRun || rec.note)) {
        destroyInstance();
        clear(content);
        rendered = null;
        const box = el("div", { class: "panel__hint", dataset: { testid: `panel-${key}-hint` } }, el("p", { class: "panel__empty", text: rec.note || "Run the query to see the data." }));
        if (rec.action) box.append(button(rec.action.label, { dataset: { testid: `panel-${key}-action` }, on: { click: () => rec.action.onClick() } }));
        showOverlay("hint", box);
      }
    }
  }

  // "Download CSV" (9.11): the display names and the display texts of all the fields, built in the browser. No server route takes part.
  function downloadCsv() {
    if (!last) {
      notify({ level: "info", text: "The panel has no data yet." });
      return;
    }
    const { fields, names } = last;
    const displays = fields.map((f) => makeDisplay(f, { zone: page.zone }));
    const body = csvText(names, fields, (i, r) => (r < fields[i].values.length ? displays[i](r).text : ""), maxRows(fields));
    const url = URL.createObjectURL(new Blob([body], { type: "text/csv;charset=utf-8" }));
    const link = el("a", { href: url, download: csvFileName(titleNow()), hidden: true });
    document.body.append(link);
    link.click();
    link.remove();
    // The release waits one second, so that every browser has started the download.
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function viewData() {
    if (!last) {
      notify({ level: "info", text: "The panel has no data yet." });
      return;
    }
    const { fields, names } = last;
    const panelName = titleNow() || `panel ${panel.id}`;
    const displays = fields.map((f) => makeDisplay(f, { zone: page.zone }));
    const total = maxRows(fields);
    const dialog = openDialog({
      title: `Data: ${panelName}`,
      wide: true,
      testid: `view-data-${key}`,
      buttons: [{ label: "Download CSV", onClick: () => { downloadCsv(); return false; } }, { label: "Close" }],
      build(bodyEl) {
        const shown = Math.min(total, VIEW_DATA_CAP);
        const plural = (n, word) => `${n.toLocaleString("en-US")} ${word}${n === 1 ? "" : "s"}`;
        // The values that a log scale of a heatmap skipped (9.19e): the note stands above the table.
        if (instance && instance.note) bodyEl.append(el("p", { class: "view-data__meta", dataset: { testid: `view-data-${key}-skip` }, text: instance.note }));
        bodyEl.append(el("p", { class: "view-data__meta", dataset: { testid: `view-data-${key}-meta` }, text: total > VIEW_DATA_CAP ? `Showing the first ${VIEW_DATA_CAP.toLocaleString("en-US")} of ${plural(total, "row")}, ${plural(fields.length, "column")}.` : `${plural(total, "row")}, ${plural(fields.length, "column")}.` }));
        const numeric = fields.map((f) => f.kind === "number" || f.kind === "number_text");
        const thead = el("thead", {}, el("tr", {}, ...names.map((n, i) => el("th", { scope: "col", class: numeric[i] ? "num" : "", text: n }))));
        const rowsEl = [];
        for (let r = 0; r < shown; r++) rowsEl.push(el("tr", {}, ...fields.map((f, i) => el("td", { class: numeric[i] ? "num" : "", text: r < f.values.length ? displays[i](r).text : "" }))));
        bodyEl.append(el("div", { class: "view-data", tabIndex: 0, role: "region", "aria-label": `Data of ${panelName}` }, el("table", { class: "dataTable dataTable--compact" }, thead, el("tbody", {}, ...rowsEl))));
        // The text alternative of the marks (9.11, 9.26): the second table, after the table of the fields.
        const marked = allRows(annotationRecords(), page.zone);
        if (marked.length) bodyEl.append(buildTable(document, marked));
      },
    });
    const csvButton = dialog.querySelector(".dialog__footer .btn");
    if (csvButton) csvButton.dataset.testid = "view-data-csv";
  }

  // "Inspect" (9.11): the expanded SQL of the last answer. The text goes in with textContent only. The server gives the
  // member `sql` only to a subject that may see it, so a missing member gets a plain text.
  function inspectQuery() {
    const info = resolve();
    // A panel with several visible queries shows one block for each query that has an answer (9.11).
    if (info.slots.length > 1) return inspectBlocks(info.slots.filter((slot) => slot.frame));
    const model = inspectModel(info.items.length ? info.items[0].frame : null);
    if (!model) {
      notify({ level: "info", text: "The panel has no data yet." });
      return;
    }
    const panelName = titleNow() || `panel ${panel.id}`;
    const facts = [model.queryId ? `Query id ${model.queryId}` : "", model.intervalMs === null ? "" : `interval ${model.intervalMs} ms`, model.rows === null ? "" : `${model.rows.toLocaleString("en-US")} ${model.rows === 1 ? "row" : "rows"}`].filter(Boolean);
    const dialog = openDialog({
      title: `Query: ${panelName}`,
      wide: true,
      testid: `inspect-${key}`,
      buttons: model.sql === null ? [{ label: "Close" }] : [{ label: "Copy SQL", onClick: async () => { notify({ level: "info", text: (await copyText(model.sql)) ? "The SQL is in the clipboard." : "The browser did not copy the SQL." }); return false; } }, { label: "Close" }],
      build(bodyEl) {
        bodyEl.append(el("p", { class: "inspect__meta", dataset: { testid: `inspect-${key}-meta` }, text: facts.length ? `${facts.join(", ")}.` : "The expanded SQL that Hune sent to the source." }));
        if (model.sql === null) bodyEl.append(el("p", { class: "inspect__none", dataset: { testid: `inspect-${key}-none` }, text: "The server does not give the SQL of this query to you." }));
        else bodyEl.append(el("pre", { class: "inspect__sql", tabIndex: 0, role: "region", "aria-label": `Expanded SQL of ${panelName}`, dataset: { testid: `inspect-${key}-sql` }, text: model.sql }));
      },
    });
    const copyButton = dialog.querySelector(".dialog__footer .btn");
    if (copyButton && model.sql !== null) copyButton.dataset.testid = "inspect-copy";
  }

  // "Inspect" of a panel with several queries (9.11): one block for each answered query, with its own "Copy SQL". The footer has "Close" only.
  function inspectBlocks(answered) {
    if (!answered.length) {
      notify({ level: "info", text: "The panel has no data yet." });
      return;
    }
    const panelName = titleNow() || `panel ${panel.id}`;
    openDialog({
      title: `Query: ${panelName}`,
      wide: true,
      testid: `inspect-${key}`,
      buttons: [{ label: "Close" }],
      build(bodyEl) {
        for (const { ref, frame } of answered) {
          const model = inspectModel(frame);
          const facts = [model.queryId ? `Query id ${model.queryId}` : "", model.intervalMs === null ? "" : `interval ${model.intervalMs} ms`, model.rows === null ? "" : `${model.rows.toLocaleString("en-US")} ${model.rows === 1 ? "row" : "rows"}`].filter(Boolean);
          const block = el("section", { class: "inspect__block", "aria-label": `Query ${ref}` }, el("h4", { class: "inspect__ref", text: `Query ${ref}` }));
          block.append(el("p", { class: "inspect__meta", dataset: { testid: `inspect-${key}-meta-${ref}` }, text: facts.length ? `${facts.join(", ")}.` : "The expanded SQL that Hune sent to the source." }));
          if (model.sql === null) block.append(el("p", { class: "inspect__none", dataset: { testid: `inspect-${key}-none-${ref}` }, text: "The server does not give the SQL of this query to you." }));
          else {
            block.append(el("pre", { class: "inspect__sql", tabIndex: 0, role: "region", "aria-label": `Expanded SQL of query ${ref}`, dataset: { testid: `inspect-${key}-sql-${ref}` }, text: model.sql }));
            block.append(button("Copy SQL", { dataset: { testid: `inspect-copy-${ref}` }, on: { click: async () => notify({ level: "info", text: (await copyText(model.sql)) ? "The SQL is in the clipboard." : "The browser did not copy the SQL." }) } }, "copy", { size: "sm" }));
          }
          bodyEl.append(block);
        }
      },
    });
  }

  // A text panel (9.19b): the page parses the lines once. A change of a value that the text names draws the DOM again from the same tree.
  // The state is `ready`, or `text-limit` (the message and no text).
  function paintText(force) {
    textView = textView || createText(panel);
    const expand = (t) => (rt.interpolateContent ? rt.interpolateContent(t) : t);
    let sig;
    try { sig = textView.key(expand); } catch (e) { sig = "!"; } // fail closed: a thrown error never leaves the frame
    if (content.firstElementChild) content.firstElementChild.setAttribute("aria-label", titleNow() || "Text");
    if (!force && sig === textSig) return;
    textSig = sig;
    let drawn;
    try { drawn = textView.draw(document, expand); } catch (e) { drawn = textView.fail(); }
    clear(content);
    if (drawn.root) {
      const attrs = { role: "region", tabindex: "0", "aria-label": titleNow() || "Text", "data-testid": `text-${key}` };
      for (const name in attrs) drawn.root.setAttribute(name, attrs[name]);
      content.append(drawn.root);
    }
    showOverlay(drawn.root ? "none" : "note", drawn.root ? null : el("p", { class: "panel__note", role: "note", dataset: { testid: `text-${key}-note` }, text: drawn.message }));
    setRootState(drawn.root ? "ready" : "text-limit");
  }

  // A new frame waits for its first result: its loader shows after WAIT_MS (setRootState starts the timer).
  setRootState("idle");

  menuButton.addEventListener("click", () => {
    const url = page.panelUrl(key);
    // A frame with a note has no data: it has no "View data", "Inspect" and "Download CSV" (9.8c).
    const noted = Boolean(record() && record().note && typeof record().note === "object");
    const items = [
      // The item "View" is absent in edit mode: the editor already shows the panel (10.2a, rule 5).
      ...(page.noView ? [] : [{ icon: "eye", label: "View", href: url, onSelect: () => page.viewPanel(key), testid: `panel-${key}-view` }]),
      ...(noted ? [] : [{ icon: "table", label: "View data", onSelect: viewData, testid: `panel-${key}-view-data` }]),
      ...(inspectAllowed(app.session) && !noted ? [{ icon: "code", label: "Inspect", onSelect: inspectQuery, testid: `panel-${key}-inspect` }] : []),
      ...(noted ? [] : [{ icon: "download", label: "Download CSV", onSelect: downloadCsv, testid: `panel-${key}-csv` }]),
      ...(page.extraMenuItems ? page.extraMenuItems(panel, inst || { key }) : []).map((item) => (!item.separator && !item.heading && !item.icon && EXTRA_ICONS[item.label] ? { ...item, icon: EXTRA_ICONS[item.label] } : item)),
    ];
    // A text panel has no data: no "View data", "Inspect" and "Download CSV" (9.19b).
    openMenu(menuButton, isText ? items.filter((item) => !/-(view-data|inspect|csv)$/.test(item.testid || "")) : items, { align: "end", testid: `panel-${key}-menu-popup` });
  });

  return {
    el: root,
    update,
    paintTexts,
    // New results of the annotations: the note and the marks (9.19d). A suspended frame draws again at its resume.
    annotationsChanged() {
      if (destroyed || suspended) return;
      paintAnnotNote();
      if (instance && instance.annotate) instance.annotate();
    },
    // The inner width of the frame: the content box (9.8).
    width() {
      const style = getComputedStyle(root);
      return Math.round(root.clientWidth - (parseFloat(style.paddingLeft) || 0) - (parseFloat(style.paddingRight) || 0));
    },
    // The page of the grid that holds the frame now: its menu, its zoom and its marks (10.2a, rule 17).
    setPage(next) {
      page = next;
      this.annotationsChanged();
    },
    suspend() {
      suspended = true;
    },
    resume() {
      suspended = false;
      if (pending) {
        pending = false;
        update(true);
      }
    },
    destroy() {
      destroyed = true;
      clearTimeout(progressTimer);
      clearTimeout(waitTimer);
      stopPreview();
      destroyInstance();
      root.remove();
    },
  };
}
