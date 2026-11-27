// The panel editor (10.3), in the layout of a panel editor of a dashboard tool: the panel preview on top, the query area
// below it, and an options pane on the right. The tabs "Query", "Panel" and "JSON" are one strip under the preview.
// "Query": query area and options pane. "Panel": a large preview and the options pane. "JSON": the JSON of the panel.

import * as api from "../api.js";
import * as editorRoutes from "./api_editor.js";
import * as busy from "../busy.js";
import { createPanelFrame } from "../panel_frame.js";
import { DEFAULT_MAX_PER_ROW, mentionsVariable } from "../repeat.js";
import { variableNamesOfLines } from "../sqlscan.js";
import { isUnitValid } from "./validators.js";
import { anchorPopup, button, clear, el, icon, notify, openDialog, toggle, uniqueId } from "../ui.js";
import { PANEL_TYPES, REDUCE_TYPES, TIMELINE_TYPES, allPanels, canBeReuseSource, changeType, locatePanel, defaultThresholds, panelRepeatRefusal, repeatableVariables, reuseFits, setPanelRepeat, updatePanel } from "./draft.js";
import { MAX_IN_FLIGHT } from "../runtime.js";
import { queriesOf, visibleQueries } from "../queries.js";
import { addQuery, addRefusal, deleteQuery, hideQuery, matchersOf, refError, renameQuery, setQueryAt } from "./query_ops.js";
import { createJsonArea } from "./json_area.js";
import { TYPE_INFO, debounced, errorLink, field, glyph, optionGroup, segmentedField, selectOf, stubFrame, typeList } from "./form.js";
import { colorModeEditor, thresholdsEditor } from "./forms/colors_form.js";
import { createScope } from "./forms/controls.js";
import { linksForm } from "./forms/links.js";
import { mappingsForm } from "./forms/mappings.js";
import { fieldNameHints, setQueryMember } from "./forms/model.js";
import { overridesForm } from "./forms/overrides.js";
import { colourNote, timelineControls } from "./forms/timeline_form.js";
import { optionForm } from "./forms/option_form.js";
import { checkTimeFrom, checkTimeShift } from "./forms/rules.js";
import { transformsForm } from "./forms/transforms.js";
import { setPosition, positionErrors } from "./layout.js";
import { errorsOfPanel, panelPointer, previewBody, previewRange } from "./preview_body.js";
import { createSqlEditor } from "./sql_editor.js";
import { countText, firstTextError, linesOf, textOf } from "./text_check.js";
import { locatePointer, prettyJson, scanJson } from "./json_check.js";

// The pointer of an SQL line of a query of the draft panel: `/data/query/sql/<line>` or `/data/extra_queries/<i>/sql/<line>`. It gives { index, line }.
function sqlPointer(pointer) {
  const m = /^\/data\/(?:query|extra_queries\/(\d+))\/sql\/(\d+)/.exec(pointer || "");
  return m ? { index: m[1] === undefined ? 0 : Number(m[1]) + 1, line: Number(m[2]) } : null;
}

// The root of one control of the options pane: `data-option` has the member that the control writes (9.29, 10.3h rule 12).
const own = (member, ...children) => el("div", { class: "orow", dataset: { option: member } }, ...children);

const SOURCE_KINDS = { clickhouse: "ClickHouse", postgres: "PostgreSQL", mongodb: "MongoDB" };

const UNITS = ["none", "short", "percent", "percentunit", "ms", "s", "bytes", "decbytes", "suffix:", "prefix:"];
const DURATION = /^([1-9][0-9]{0,8})(ms|s|m|h|d)$/;
const DURATION_MS = { ms: 1, s: 1000, m: 60000, h: 3600000, d: 86400000 };

const clock = (ms) => new Date(ms).toTimeString().slice(0, 8);

export function createPanelEditor(E, id) {
  const state = { tab: E.panelTab || "query", errors: [], running: false, ran: null, qi: 0 };
  const controller = new AbortController();
  // The debounce scope of the forms (10.3a): a change of a text field applies 300 ms after the last key, or at once on a structural change.
  const scope = createScope();
  let badgeSetters = [];
  const open = new Map([["panel", true], ["viz", false], ["text", true], ["standard", true], ["position", true], ["repeat", true], ["timeline", true], ["legend", true], ["tooltip", true]]);
  let topFrame = null;
  let ended = false; // true after `destroy()`: the preview stays as it is
  let sqlEditor = null;
  let popovers = []; // the close functions of the popovers "Query options" of the tab
  let optForm = null; // the option form of the type (10.3h), or null
  let runButton = null;
  let statusLine = null;
  const root = el("section", { class: "peditor", dataset: { testid: "panel-editor", tab: state.tab } });
  const topHost = el("div", { class: "peditor__top" });
  const errorsBox = el("div", { class: "errors peditor__errors", dataset: { testid: "panel-errors" }, role: "region", "aria-label": "Errors", hidden: true });
  const tabs = el("div", { class: "tabs peditor__tabs", role: "tablist", "aria-label": "Panel editor" });
  const body = el("div", { class: "peditor__lbody" });
  const optionsHost = el("div", { class: "peditor__obody" });

  const where = () => locatePanel(E.doc, id);
  const panel = () => (where() ? where().panel : null);

  // --- the draft ---------------------------------------------------------------------------------------------

  function apply(fn, { layout = null } = {}) {
    // The panel can be gone: a discard of a new panel removes it while a field still has a change that waits (10.2a).
    if (!where()) return;
    let doc = updatePanel(E.doc, id, fn);
    if (layout) {
      const at = locatePanel(doc, id);
      doc = structuredClone(doc);
      doc.sections[at.section].panels = layout(doc.sections[at.section].panels);
    }
    E.change(doc);
    drawTop();
    if (state.tab === "panel") body.replaceChildren(summaryTab());
    for (const set of badgeSetters) set();
    paintRun();
  }

  function drawTop() {
    const p = panel();
    // A change that comes after the end of the editor (a late timer) must not make a new frame: nothing would destroy it.
    if (!p || ended) return;
    if (topFrame) topFrame.destroy();
    // A type that the page cannot draw (for example from the JSON tab) gets a stub, never an exception.
    // The preview is one panel with the selection of the draft, also for a panel that repeats: it reads the plain record (9.8c, 10.3).
    topFrame = PANEL_TYPES.includes(p.type) && (p.type === "text" || p.data) ? createPanelFrame({ panel: p, page: E.editPage, lookup: () => E.draftRt.single(id) }) : stubFrame(p);
    topFrame.el.style.gridColumn = "";
    topFrame.el.style.gridRow = "";
    topFrame.el.classList.add("panel--view", "panel--editing");
    topHost.replaceChildren(topFrame.el);
    topFrame.update(true);
    // The base grid is out of the page here: a panel below the fold of the dashboard was never in view, and its record would stay idle. The preview is in view.
    E.draftRt.setVisible(id, true);
  }

  const onPanel = (rec) => {
    if (rec.id === id || (rec.panel.data.reuse && rec.panel.data.reuse.panel === id)) {
      if (topFrame && PANEL_TYPES.includes(rec.panel.type)) topFrame.update();
      // The names of the fields of the new result go to the selects of fields (10.3h).
      if (rec.id === id && optForm) optForm.refreshFields();
    }
  };
  const offPanel = E.draftRt.on((type, rec) => {
    if (type === "panel") onPanel(rec);
  });

  // --- errors --------------------------------------------------------------------------------------------------

  function showErrors() {
    clear(errorsBox);
    const list = [...state.errors];
    const at = locatePanel(E.doc, id);
    const saved = E.lastErrors && at ? errorsOfPanel(E.lastErrors, at.section, at.index) : { own: [], other: 0 };
    for (const e of saved.own) list.push({ ...e, from: "the last save" });
    errorsBox.hidden = list.length === 0 && !saved.other;
    paintStatus();
    if (errorsBox.hidden) {
      if (sqlEditor) sqlEditor.markLines([]);
      return;
    }
    const ul = el("ul", { class: "error-list" });
    const many = queriesOf(panel()).length > 1;
    for (let e of list) {
      const m = sqlPointer(e.pointer);
      // The query that gave the error: a preview names it (`queryIndex`), a pointer of the draft names it by its place (10.3).
      const index = e.queryIndex !== undefined ? e.queryIndex : m ? m.index : null;
      // A check at save or preview gives an item with the pointer of the SQL line and a message "column C: ..." (7.18):
      // the box names the line and the column too, as for `sql_invalid`.
      const colAt = !e.line && m ? /^column (\d+):\s*/.exec(e.message || "") : null;
      if (colAt) {
        e = { ...e, line: m.line + 1, column: Number(colAt[1]), message: e.message.slice(colAt[0].length) };
      }
      const sqlAt = (e.code === "sql_invalid" || colAt) && e.line;
      const go = () => {
        if (state.tab !== "query") selectTab("query");
        if (index !== null && index !== state.qi && index < queriesOf(panel()).length) {
          state.qi = index;
          renderTab();
        }
        if (sqlAt && sqlEditor) sqlEditor.focusAt(e.line, e.column || 1);
        else if (m && sqlEditor) sqlEditor.focusAt(m.line + 1, 1);
        else if (sqlEditor) sqlEditor.focusAt(1, 1);
      };
      const named = many && index !== null ? `Query ${queriesOf(panel())[index]?.ref ?? index}: ` : "";
      const place = sqlAt ? `${named}line ${e.line}, column ${e.column ?? 1}` : e.pointer || "(panel)";
      const rest = `${e.code && !sqlAt ? `[${e.code}] ` : ""}${e.message}`;
      ul.append(el("li", {}, !sqlAt && !e.pointer ? el("span", { class: "error-line", text: `${named}${rest}` }) : errorLink(place, rest, go)));
    }
    errorsBox.append(el("h3", { class: "errors__title", text: "Errors" }), ul);
    if (saved.other) {
      const link = el("button", { type: "button", class: "link-button", text: "Open the dashboard JSON", on: { click: () => E.dispatch("open-json") } });
      errorsBox.append(el("p", {}, `${saved.other} more error${saved.other === 1 ? "" : "s"} in other parts of the document. `, link));
    }
    const lines = [];
    for (const e of list) {
      const m = sqlPointer(e.pointer);
      // Only the lines of the selected query show in the text area.
      if ((e.queryIndex !== undefined ? e.queryIndex : m ? m.index : 0) !== state.qi) continue;
      if (e.code === "sql_invalid" && e.line) lines.push(e.line);
      if (m) lines.push(m.line + 1);
    }
    if (sqlEditor) sqlEditor.markLines(lines);
  }

  // --- run -----------------------------------------------------------------------------------------------------

  // The draft panel of the preview of one query: the query as `data.query`, with the members of the panel (6.22, 10.3). It has no `ref`, no `hide`
  // and no extra query, because a preview runs one query.
  function previewPanel(p, q) {
    const { ref, hide, source, format, sql, ...members } = p.data.query;
    return { ...p, data: { query: { source: q.source, format: q.format, sql: q.sql, ...members } } };
  }

  // "Run" and Ctrl+Enter: one preview for each visible query, in the order of the queries, 6 at the same time at most (10.3). The editor combines
  // the answers as the page does, and an error of one query shows as a partial result (9.8d).
  async function run() {
    const p = panel();
    const queries = p && p.data && p.data.query ? visibleQueries(p) : [];
    if (!queries.length || state.running || queries.some((q) => !E.sources.some((s) => s.id === q.source))) return;
    state.running = true;
    state.errors = [];
    state.ran = null;
    paintRun();
    busy.begin();
    const frameWidth = topFrame ? topFrame.width() : 600;
    const started = performance.now();
    const slots = queries.map((q) => ({ ref: q.ref, state: "idle", frame: null, error: null }));
    let stopped = false;
    try {
      // The preview uses the range of 9.8b with the values of the draft (10.3). The module panel_time.js belongs to the page.
      const panelTime = await import("../panel_time.js").catch(() => null);
      const pr = previewRange(p.data.query, E.api.runtime.range, { panelRange: panelTime && panelTime.panelRange, now: Date.now(), zone: E.editPage.zone });
      if (pr.error) throw Object.assign(new Error(pr.error), { code: "invalid_range" });
      const known = E.draftRt.requestVariables();
      const all = queriesOf(p);
      const send = async (q, slot) => {
        const queryIndex = all.findIndex((x) => x.ref === q.ref);
        try {
          const body = previewBody({ doc: E.doc, panel: previewPanel(p, q), target: { uid: E.uid }, range: pr.range, maxDataPoints: p.data.query.max_data_points ?? (frameWidth || 600), known });
          Object.assign(slot, { state: "ready", frame: await editorRoutes.preview(q.source, body, controller.signal) });
        } catch (error) {
          if (api.isAbort(error)) {
            stopped = true;
            return;
          }
          Object.assign(slot, { state: "error", error });
          if (error.code === "invalid_document") {
            for (const e of (error.extra && error.extra.errors) || []) {
              const rel = panelPointer(e.pointer);
              state.errors.push({ ...e, queryIndex, pointer: rel.pointer, message: rel.inVariables ? `${e.message} (see the dashboard JSON)` : e.message });
            }
          } else {
            state.errors.push({ pointer: "", queryIndex, code: error.code, message: error.message, line: error.extra && error.extra.line, column: error.extra && error.extra.column });
          }
        }
      };
      let next = 0;
      const worker = async () => {
        while (next < queries.length) {
          const at = next++;
          await send(queries[at], slots[at]);
        }
      };
      await Promise.all(Array.from({ length: Math.min(MAX_IN_FLIGHT, queries.length) }, worker));
      state.ran = { at: Date.now(), ms: Math.round(performance.now() - started) };
    } catch (error) {
      if (api.isAbort(error)) stopped = true;
      else {
        state.errors.push({ pointer: "", code: error.code, message: error.message });
        for (const slot of slots) Object.assign(slot, { state: "error", error });
      }
    } finally {
      busy.end();
      state.running = false;
      // The names of the fields of the new result go to the selects of fields (10.3h).
      if (optForm) optForm.refreshFields();
      if (!stopped) {
        // A panel with one query keeps the result of one preview. A panel with extra queries keeps one result for each query.
        if (slots.length === 1 && !p.data.extra_queries) E.draftRt.setPreview(id, slots[0].frame ? { frame: slots[0].frame } : { error: slots[0].error });
        else E.draftRt.setPreview(id, { slots });
      }
      paintRun();
      showErrors();
    }
  }

  // True when the panel has a visible query and the person can use the source of each one (10.3, 10.14).
  function canRun(p) {
    const queries = p && p.data && p.data.query ? visibleQueries(p) : [];
    return queries.length > 0 && queries.every((q) => E.sources.some((s) => s.id === q.source));
  }

  function paintRun() {
    paintStatus();
    if (!runButton) return;
    const p = panel();
    const allowed = canRun(p);
    // "Run" is absent when the source is not in the list of the sources (10.3, 10.14).
    runButton.hidden = !allowed;
    runButton.disabled = state.running;
    runButton.title = "Run (Ctrl+Enter)";
    runButton.querySelector("span").textContent = state.running ? "Running" : "Run";
  }

  // The result or error line under the SQL text.
  function paintStatus() {
    if (!statusLine) return;
    const p = panel();
    const allowed = canRun(p);
    let tone = "idle";
    let text = allowed ? "Run the query to see the data. Ctrl+Enter runs it." : p && p.data && p.data.query && !visibleQueries(p).length ? "All the queries are hidden." : "This source is not available to you: the query cannot run here.";
    const firstError = state.errors[0];
    // One message: the Errors box and the preview say what failed, so the status line stays quiet.
    statusLine.hidden = !!firstError && !state.running;
    if (state.running) {
      tone = "idle";
      text = "Running the query";
    } else if (firstError) {
      tone = "error";
      text = "The query failed.";
    } else if (state.ran && allowed) {
      tone = "ok";
      text = `The query ran at ${clock(state.ran.at)} in ${state.ran.ms} ms. The preview shows the result.`;
    }
    statusLine.dataset.tone = tone;
    clear(statusLine);
    statusLine.append(icon(tone === "error" ? "error" : tone === "ok" ? "success" : "info", 14), el("span", { class: "qstatus__text", text }));
  }

  // --- tab "Query": the query area ---------------------------------------------------------------------------------

  // The dialog of the change that removes the extra queries (10.3): the type `barchart` or `xychart`, or the choice "Reuse a panel".
  function askQueriesLeave(title, change) {
    return openDialog({
      title,
      testid: "type-change-dialog",
      build: (b) => b.append(el("p", { text: "The extra queries of this panel leave the draft." })),
      buttons: [{ label: "Cancel" }, { label: "Change", primary: true, onClick: change }],
    });
  }

  // True for a panel with `repeat`, or in a section with `repeat`: it has one query (4.15a, rule 8).
  function repeats() {
    const at = where();
    return !!at && (!!at.panel.repeat || !!E.doc.sections[at.section].repeat);
  }

  let chipsNode = null; // the strip of the chips: a rename draws it again, and the text field of the name keeps the focus

  function selectQuery(index) {
    scope.flush();
    state.qi = index;
    renderTab();
  }

  // The strip of chips above the fields of the tab "Query" (10.3): one chip for each query, with its eye and its "Delete", and "Add query".
  function chipStrip(p) {
    const list = queriesOf(p);
    const strip = el("div", { class: "qchips", role: "group", "aria-label": "Queries", dataset: { testid: "query-chips" } });
    list.forEach((q, i) => {
      const chosen = i === state.qi;
      const chip = el("span", { class: "qchip", dataset: { selected: String(chosen), hidden: String(q.hide) } },
        el("button", { type: "button", class: "qchip__name", text: q.ref, "aria-pressed": String(chosen), "aria-label": `Query ${q.ref}${q.hide ? ", hidden" : ""}`, dataset: { testid: `query-chip-${q.ref}` }, on: { click: () => selectQuery(i) } }),
        el("button", { type: "button", class: "qchip__eye", "aria-pressed": String(q.hide), "aria-label": `${q.hide ? "Show" : "Hide"} query ${q.ref}`, "data-tooltip": q.hide ? "Show the query" : "Hide the query", dataset: { testid: `query-hide-${q.ref}` }, on: { click: () => {
          scope.flush();
          apply((x) => hideQuery(x, i, !q.hide));
          renderTab();
        } } }, icon(q.hide ? "eye-off" : "eye", 14)));
      if (list.length > 1) {
        chip.append(el("button", { type: "button", class: "qchip__delete", "aria-label": `Delete query ${q.ref}`, "data-tooltip": "Delete", dataset: { testid: `query-delete-${q.ref}` }, on: { click: () => {
          scope.flush();
          apply((x) => deleteQuery(x, i));
          // The chips after the deleted one move up, so a selection after it moves with them.
          if (i < state.qi) state.qi -= 1;
          state.qi = Math.min(state.qi, list.length - 2);
          renderTab();
        } } }, icon("close", 14)));
      }
      strip.append(chip);
    });
    const refusal = addRefusal(p, repeats());
    const add = button("Add query", { dataset: { testid: "query-add" }, "aria-disabled": refusal ? "true" : "false", ...(refusal ? { title: refusal, "data-tooltip": refusal } : {}), on: { click: () => {
      if (refusal) return;
      scope.flush();
      apply((x) => addQuery(x, state.qi));
      state.qi = queriesOf(panel()).length - 1;
      renderTab();
    } } }, "plus", { size: "sm" });
    if (refusal) add.classList.add("is-disabled");
    strip.append(add);
    return strip;
  }

  // The field "Name" of the selected query (10.3): a text of the grammar of 4.3. A rename changes the matchers `query` of the panel.
  function nameField(p, q) {
    const name = field("Name", el("input", { class: "input input--mono input--ref", type: "text", value: q.ref, maxlength: "16", spellcheck: "false", autocomplete: "off", dataset: { testid: "query-ref" } }));
    let asking = false;
    // The text applies 300 ms after the last key. A rename that changes matchers waits for the end of the entry (the event `change`) and asks first.
    const rename = (final) => {
      const ref = name.control.value.trim();
      const live = panel();
      if (!live) return;
      const error = refError(live, state.qi, ref);
      name.error(error);
      if (error || ref === queriesOf(live)[state.qi].ref) return;
      const old = queriesOf(live)[state.qi].ref;
      const hit = matchersOf(live, old);
      const done = () => {
        apply((x) => renameQuery(x, state.qi, ref));
        if (chipsNode) {
          const fresh = chipStrip(panel());
          chipsNode.replaceWith(fresh);
          chipsNode = fresh;
        }
      };
      if (!hit.length) return done();
      // A key and a blur can both end the entry: one dialog only.
      if (!final || asking) return;
      asking = true;
      const ask = openDialog({
        title: `Rename query ${old}?`,
        testid: "query-rename-dialog",
        build: (b) => b.append(el("p", { text: `${hit.length === 1 ? "One override matches" : `${hit.length} overrides match`} the query ${old} by its name. ${hit.length === 1 ? "It changes" : "They change"} to ${ref}.` })),
        buttons: [{ label: "Cancel", onClick: () => { name.control.value = old; name.error(""); } }, { label: "Rename", primary: true, onClick: done }],
      });
      ask.addEventListener("close", () => {
        asking = false;
      });
    };
    name.control.addEventListener("input", scope.debounce(() => rename(false), 300));
    name.control.addEventListener("change", () => rename(true));
    return name;
  }

  // The popover "Query options" (10.3): one button on the row of the tab "Query". The panel under it holds the fields that the row has no room for.
  // `isSet()` is true when one of these fields has a value that is not its default: the button then shows a dot. The panel is in the DOM after
  // its button, so the Tab order is the visual order. Escape, a click outside and a focus that leaves the group close it.
  function optionsPopover(children, isSet) {
    const id = uniqueId("qopt");
    const dot = el("span", { class: "qopt__dot", "aria-hidden": "true", hidden: true, dataset: { testid: "query-options-dot" } });
    const note = el("span", { class: "visually-hidden", dataset: { testid: "query-options-note" } });
    const trigger = el("button", { type: "button", class: "btn qopt__button", "aria-expanded": "false", "aria-controls": id, "aria-label": "Query options", title: "Query options", dataset: { testid: "query-options" } }, icon("settings", 14), el("span", { text: "Options" }), dot, note);
    const pop = el("div", { class: "qopt__pop", id, role: "group", "aria-label": "Query options", hidden: true, dataset: { testid: "query-options-pop" } }, ...children);
    const node = el("div", { class: "qopt" }, trigger, pop);
    let follow = null;
    const onDown = (event) => {
      if (!node.isConnected || !node.contains(event.target)) close(false);
    };
    function close(focus) {
      document.removeEventListener("pointerdown", onDown, true);
      if (pop.hidden) return;
      pop.hidden = true;
      if (follow) follow();
      follow = null;
      trigger.setAttribute("aria-expanded", "false");
      if (focus) trigger.focus();
    }
    popovers.push(() => close(false));
    trigger.addEventListener("click", () => {
      if (!pop.hidden) return close(true);
      pop.hidden = false;
      follow = anchorPopup(pop, trigger, { align: "end", gap: 4 });
      trigger.setAttribute("aria-expanded", "true");
      document.addEventListener("pointerdown", onDown, true);
    });
    node.addEventListener("keydown", (event) => {
      if (event.key !== "Escape" || pop.hidden) return;
      event.preventDefault();
      event.stopPropagation();
      close(true);
    });
    node.addEventListener("focusout", (event) => {
      if (event.relatedTarget && !node.contains(event.relatedTarget)) close(false);
    });
    const paint = () => {
      const on = isSet();
      dot.hidden = !on;
      note.textContent = on ? ", some options are set" : "";
      trigger.dataset.set = String(on);
    };
    paint();
    return { node, paint };
  }

  function queryTab() {
    const p = panel();
    const box = el("div", { class: "qarea" });
    // A text panel has no query (4.33a): the tab has a note and no field, no "Source" and no "Run".
    if (p.type === "text") {
      box.append(el("p", { class: "qhint", dataset: { testid: "text-no-query" }, text: "A text panel has no query." }));
      return box;
    }
    // A draft that has neither a query nor a reuse (the JSON tab can make it for a moment) shows a note and no field.
    if (!p.data || (!p.data.query && !p.data.reuse)) {
      box.append(el("p", { class: "qhint", dataset: { testid: "no-data-note" }, text: "This panel has no query and no reuse. Correct the JSON of the panel." }));
      return box;
    }
    const isReuse = !!p.data.reuse;
    const dataSeg = segmentedField("Data", [{ value: "query", label: "Query" }, { value: "reuse", label: "Reuse a panel" }], isReuse ? "reuse" : "query", (kind) => {
      if (kind === "reuse") {
        const first = allPanels(E.doc).find((x) => x.id !== id && canBeReuseSource(E.doc, x) && reuseFits(p.type, x));
        if (!first) {
          dataSeg.seg.set("query");
          reuseError.textContent = "No other panel has a query.";
          reuseError.hidden = false;
          return;
        }
        // A panel that reuses another panel has no repeat (4.13a, rule 4) and no extra query (4.15a, rule 7).
        const change = () => {
          apply((x) => {
            const { repeat, ...rest } = x;
            return { ...rest, data: { reuse: { panel: first.id, with_transforms: true } } };
          });
          state.qi = 0;
          renderTab();
        };
        if (p.data.extra_queries) {
          dataSeg.seg.set("query");
          return askQueriesLeave("Change to Reuse?", change);
        }
        return change();
      }
      apply((x) => ({ ...x, data: { query: { source: (E.sources[0] && E.sources[0].id) || "", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null } } }));
      renderTab();
    });
    const reuseError = el("p", { class: "field-error", role: "alert", hidden: true });
    if (isReuse) {
      const candidates = allPanels(E.doc).filter((x) => x.id !== id && canBeReuseSource(E.doc, x) && reuseFits(p.type, x));
      const note = (x) => (x.data.extra_queries ? `, ${x.data.extra_queries.length + 1} queries` : "");
      const src = field("Source panel", selectOf(candidates.map((x) => [String(x.id), `${x.title || "Untitled"} (${x.id}${note(x)})`]), String(p.data.reuse.panel)));
      src.control.addEventListener("change", () => apply((x) => ({ ...x, data: { reuse: { ...x.data.reuse, panel: Number(src.control.value) } } })));
      const withT = toggle({ label: "Apply its transformations", checked: p.data.reuse.with_transforms, onChange: (checked) => apply((x) => ({ ...x, data: { reuse: { ...x.data.reuse, with_transforms: checked } } })) });
      // The toggle keeps its own visible label, and the label of the form helper is for the select only.
      // The choice "Reuse a panel" has its place in the popover "Query options", with a dot while it is on (10.3).
      const reuseOptions = optionsPopover([dataSeg.node], () => true);
      box.append(el("div", { class: "qbar" }, el("div", { class: "qbar__item qbar__item--grow" }, src.node), el("div", { class: "qbar__item qbar__item--check" }, withT.el), reuseOptions.node), reuseError, el("p", { class: "qhint", text: "This panel shows the result of the other panel. It has no query of its own." }));
      return box;
    }
    const list = queriesOf(p);
    if (state.qi >= list.length) state.qi = 0;
    const q = list[state.qi];
    // The members of the panel (4.15a, rule 4) live in the first query and show once, for all the queries.
    const panelQuery = p.data.query;
    // The item shows the kind of the source (6.22): "Label (PostgreSQL, up)". An unknown kind shows as the API gives it.
    const options = E.sources.map((s) => [s.id, `${s.label || s.id} (${[SOURCE_KINDS[s.kind] || s.kind, s.state].filter(Boolean).join(", ")})`]);
    if (!E.sources.some((s) => s.id === q.source)) options.unshift([q.source, `${q.source} (not available to you)`]);
    const source = field("Source", selectOf(options, q.source));
    // The row has no room for a visible label here: the label stays for the screen reader, and the title says the name.
    source.node.querySelector(".field__label").classList.add("visually-hidden");
    source.control.title = "Source";
    source.control.addEventListener("change", () => {
      apply((x) => setQueryAt(x, state.qi, { source: source.control.value }));
      paintRun();
    });
    const format = segmentedField("Format", [{ value: "table", label: "Table" }, { value: "time_series", label: "Time series" }, ...(p.type === "logs" ? [{ value: "logs", label: "Logs" }] : [])], q.format, (value) => apply((x) => setQueryAt(x, state.qi, { format: value })));
    // The group of the two choices has the name "Format" (aria-label), so the visible label of the field leaves the row.
    format.node.querySelector(".field__label").remove();
    const minInterval = field("Min interval", el("input", { class: "input", type: "text", value: panelQuery.min_interval || "", placeholder: "1ms", title: "Minimum interval. Empty (1ms), or a duration from 1ms to 1d." }));
    const onMin = scope.debounce(() => {
      const text = minInterval.control.value.trim();
      const m = DURATION.exec(text);
      if (text !== "" && (!m || Number(m[1]) * DURATION_MS[m[2]] < 1 || Number(m[1]) * DURATION_MS[m[2]] > 86400000)) {
        minInterval.error("Use a duration of one unit from 1ms to 1d, for example 30s.");
        return;
      }
      minInterval.error("");
      apply((x) => ({ ...x, data: { ...x.data, query: { ...x.data.query, min_interval: text === "" ? null : text } } }));
    }, 300);
    minInterval.control.addEventListener("input", onMin);
    const maxPoints = field("Max points", el("input", { class: "input", type: "text", inputmode: "numeric", placeholder: "auto", title: "Max data points. Empty (the width of the plot), or 1 to 10,000.", value: panelQuery.max_data_points === null ? "" : String(panelQuery.max_data_points) }));
    const onMax = scope.debounce(() => {
      const text = maxPoints.control.value.trim();
      const n = Number(text);
      if (text !== "" && (!/^[0-9]+$/.test(text) || n < 1 || n > 10000)) {
        maxPoints.error("Use a whole number from 1 to 10000, or leave it empty.");
        return;
      }
      maxPoints.error("");
      apply((x) => ({ ...x, data: { ...x.data, query: { ...x.data.query, max_data_points: text === "" ? null : n } } }));
    }, 300);
    maxPoints.control.addEventListener("input", onMax);
    // The time override of the panel (4.15, 9.8b): two texts and a check box. An empty text or an unchecked box removes the member.
    let optionsDot = null;
    const setQuery = (key, value) => {
      apply((x) => ({ ...x, data: { ...x.data, query: setQueryMember(x.data.query, key, value) } }));
      if (optionsDot) optionsDot.paint();
    };
    const timeFrom = field("Relative time", el("input", { class: "input input--mono", type: "text", spellcheck: "false", autocomplete: "off", value: panelQuery.time_from || "", placeholder: "now-6h", title: "Empty, or a text such as now-6h or now/d.", dataset: { testid: "query-time-from" } }));
    timeFrom.control.addEventListener("input", scope.debounce(() => {
      const text = timeFrom.control.value.trim();
      const error = checkTimeFrom(text);
      timeFrom.error(error);
      if (!error) setQuery("time_from", text === "" ? undefined : text);
    }, 300));
    const timeShift = field("Time shift", el("input", { class: "input input--mono", type: "text", spellcheck: "false", autocomplete: "off", value: panelQuery.time_shift || "", placeholder: "1d", title: "Empty, or a count and a unit, such as 1d.", dataset: { testid: "query-time-shift" } }));
    timeShift.control.addEventListener("input", scope.debounce(() => {
      const text = timeShift.control.value.trim();
      const error = checkTimeShift(text);
      timeShift.error(error);
      if (!error) setQuery("time_shift", text === "" ? undefined : text);
    }, 300));
    const hideTime = toggle({ label: "Hide time info", kind: "check", checked: panelQuery.hide_time_override === true, testid: "query-hide-time", onChange: (checked) => setQuery("hide_time_override", checked ? true : undefined) });
    sqlEditor = createSqlEditor({
      lines: q.sql,
      // The dialect of the syntax tree follows the kind of the source now (10.3c); the JSON reader needs the names of the variables.
      dialect: () => ((E.sources.find((s) => s.id === queriesOf(panel())[state.qi]?.source) || {}).kind || "clickhouse"),
      variables: () => (E.doc.variables || []).map((v) => v.name),
      onChange: (lines) => apply((x) => setQueryAt(x, state.qi, { sql: lines })),
      onRun: run,
    });
    runButton = button("Run", { class: "btn btn--primary", dataset: { testid: "run-button" }, on: { click: run } }, "refresh");
    statusLine = el("p", { class: "qstatus", role: "status", dataset: { testid: "query-status" } });
    paintRun();
    chipsNode = chipStrip(p);
    // The popover "Query options": the choice "Reuse a panel" and the fields of the panel that the row has no room for (10.3).
    // It has a dot when one of them is not at its default. The fields of the panel apply to all the queries (4.15a, rule 4).
    optionsDot = optionsPopover(
      [dataSeg.node, el("div", { class: "qopt__pair" }, timeFrom.node, timeShift.node), hideTime.el],
      () => {
        const live = panel();
        const query = live && live.data && live.data.query;
        return !!live && !!live.data && (!!live.data.reuse || (!!query && (!!query.time_from || !!query.time_shift || query.hide_time_override === true)));
      },
    );
    // One row (10.3): Source, Format, Name, Minimum interval, Max data points, "Query options", then "Run" at the end.
    box.append(
      chipsNode,
      el("div", { class: "qbar", role: "group", "aria-label": "Query settings" },
        el("div", { class: "qbar__item qbar__item--source" }, source.node),
        el("div", { class: "qbar__item qbar__item--fixed" }, format.node),
        el("div", { class: "qbar__item qbar__item--name" }, nameField(p, q).node),
        el("div", { class: "qbar__item qbar__item--num" }, minInterval.node),
        el("div", { class: "qbar__item qbar__item--num" }, maxPoints.node),
        optionsDot.node,
        el("div", { class: "qbar__item qbar__item--end" }, runButton)),
      reuseError,
      sqlEditor.el,
      statusLine);
    return box;
  }

  // --- the group "Repeat options" (4.13a, 10.3) ------------------------------------------------------------------------------

  function applyDoc(doc) {
    if (!where()) return;
    E.change(doc);
    drawTop();
    if (state.tab === "panel") body.replaceChildren(summaryTab());
    for (const set of badgeSetters) set();
  }

  // The count line: the copies that the page shows now, with the selection of the draft, or the message of 9.8c.
  function repeatCountText() {
    const entry = E.draftRt.plan && E.draftRt.plan.entries.panels.get(id);
    if (!entry) return "";
    if (entry.state === "ok") return `The page shows ${entry.k} ${entry.k === 1 ? "copy" : "copies"} now.`;
    if (entry.state === "loading") return "The values of the variable are not loaded yet. The page shows the copies after the load.";
    return entry.message;
  }

  // True when the title, the description or the SQL names the variable: the copies are not the same (9.13, 9.10).
  function namesVariable(p, name) {
    const sql = p.data && p.data.query ? variableNamesOfLines(p.data.query.sql) : [];
    return mentionsVariable(p.title, name) || mentionsVariable(p.description, name) || sql.includes(name);
  }

  function repeatOptions(p) {
    const refusal = panelRepeatRefusal(E.doc, id);
    const vars = repeatableVariables(E.doc);
    const current = p.repeat || null;
    const choices = [["", "None"], ...vars.map((v) => [v.name, v.label || v.name])];
    // A stored variable that cannot repeat any more stays in the list, so the person sees it and chooses another one.
    if (current && !vars.some((v) => v.name === current.variable)) choices.push([current.variable, `${current.variable} (not valid)`]);
    const variable = field("Repeat by variable", selectOf(choices, current ? current.variable : ""), { hint: vars.length ? null : "No variable has several values (Multi-value). Turn it on for a variable first." });
    variable.control.dataset.testid = "repeat-variable";
    if (refusal) {
      variable.control.disabled = true;
      variable.control.title = refusal;
    }
    variable.control.addEventListener("change", () => {
      const name = variable.control.value;
      scope.flush();
      if (name === "") applyDoc(setPanelRepeat(E.doc, id, null));
      else applyDoc(setPanelRepeat(E.doc, id, { variable: name, direction: current ? current.direction : "h", max_per_row: current && current.direction === "h" ? current.max_per_row : undefined }));
      renderOptions();
    });
    const nodes = [own("repeat", variable.node)];
    if (current) {
      const direction = field("Direction", selectOf([["h", "Horizontal"], ["v", "Vertical"]], current.direction));
      direction.control.dataset.testid = "repeat-direction";
      direction.control.addEventListener("change", () => {
        scope.flush();
        // The direction `v` removes `max_per_row`. The direction `h` writes x = 0 and w = 24 (4.13a, rule 5).
        applyDoc(setPanelRepeat(E.doc, id, { variable: current.variable, direction: direction.control.value, max_per_row: direction.control.value === "h" ? current.max_per_row : undefined }));
        renderOptions();
      });
      nodes.push(own("repeat.direction", direction.node));
      if (current.direction === "h") {
        const max = field("Max per row", el("input", { class: "input input--num", type: "text", inputmode: "numeric", value: current.max_per_row === undefined ? "" : String(current.max_per_row), size: 4 }), { hint: `Empty: ${DEFAULT_MAX_PER_ROW}` });
        max.control.dataset.testid = "repeat-max-per-row";
        max.control.addEventListener("input", scope.debounce(() => {
          const t = max.control.value.trim();
          if (t !== "" && (!/^[0-9]+$/.test(t) || Number(t) < 1 || Number(t) > 24)) return max.error("Use a whole number from 1 to 24, or leave it empty.");
          max.error("");
          const live = panel().repeat;
          if (!live) return;
          applyDoc(setPanelRepeat(E.doc, id, { variable: live.variable, direction: "h", max_per_row: t === "" ? undefined : Number(t) }));
          countLine.textContent = repeatCountText();
        }, 300));
        nodes.push(own("repeat.max_per_row", max.node));
      }
    }
    const countLine = el("p", { class: "field__hint", dataset: { testid: "repeat-count" }, role: "status", text: current ? repeatCountText() : "" });
    countLine.hidden = !current;
    nodes.push(countLine);
    if (current) {
      // The hint and the count follow each change of the draft (a title that names the variable removes the hint).
      const hint = el("p", { class: "field__hint", dataset: { testid: "repeat-hint" }, text: "Neither the title nor the SQL names this variable. The copies are the same." });
      const paint = () => {
        const live = panel() && panel().repeat;
        hint.hidden = !live || namesVariable(panel(), live.variable);
        countLine.textContent = repeatCountText();
      };
      paint();
      badgeSetters.push(paint);
      nodes.push(hint);
    }
    return nodes;
  }

  // --- the text panel (4.33a, 9.19b, 10.3) ---------------------------------------------------------------------------------------

  // The choice of a type. A change to or from `text` asks first: the members of one kind leave the draft. "Change" applies it to the draft.
  function pickType(type) {
    const fromText = panel().type === "text";
    const toText = type === "text";
    const change = () => {
      scope.flush();
      const had = !!(panel().data && panel().data.extra_queries);
      apply((x) => changeType(x, type, (E.sources[0] && E.sources[0].id) || ""));
      // The extra queries left the draft with a change to a type that draws one frame: the chips follow.
      if (had && !(panel().data && panel().data.extra_queries)) state.qi = 0;
      if (fromText !== toText || (had && state.tab === "query")) renderTab();
      else renderOptions();
      const again = optionsHost.querySelector(`.typelist__item[data-type="${type}"]`);
      if (again) again.focus();
    };
    // A change to a type that draws one frame asks first when the panel has extra queries: they leave the draft (10.3).
    if ((type === "barchart" || type === "xychart" || type === "geomap" || type === "logs") && panel().data && panel().data.extra_queries) return askQueriesLeave(`Change to ${TYPE_INFO[type][0]}?`, change);
    // A change from a type with a calculation (`reduce`) to a timeline asks first: the calculation leaves the draft (10.3).
    if (REDUCE_TYPES.includes(panel().type) && TIMELINE_TYPES.includes(type)) {
      return openDialog({
        title: `Change to ${TYPE_INFO[type][0]}?`,
        testid: "type-change-dialog",
        build: (b) => b.append(el("p", { text: "The calculation of this panel leaves the draft. The panel takes the options of the new type." })),
        buttons: [{ label: "Cancel" }, { label: "Change", primary: true, onClick: change }],
      });
    }
    if (fromText === toText) return change();
    if (fromText && !E.sources.length) return notify({ level: "warning", text: "No source is available to you." });
    return openDialog({
      title: toText ? "Change to Text?" : `Change to ${TYPE_INFO[type][0]}?`,
      testid: "type-change-dialog",
      build: (b) => b.append(el("p", { text: toText ? "The query, the transformations and the field options of this panel leave the draft." : "The text of this panel leaves the draft. The panel gets the query of a new panel." })),
      buttons: [{ label: "Cancel" }, { label: "Change", primary: true, onClick: change }],
    });
  }

  // The field "Content": a plain textarea (no second layer), the count, the first error and the help. A change applies 300 ms after the last key.
  function textField(p) {
    const fid = uniqueId("txt");
    const area = el("textarea", { class: "input input--mono text-content", id: fid, rows: 12, spellcheck: "true", autocomplete: "off", autocapitalize: "off", wrap: "soft", "aria-describedby": `${fid}-count ${fid}-err ${fid}-help`, dataset: { testid: "text-content" } });
    area.value = textOf(p.options.content || []);
    const counter = el("p", { class: "field__hint", id: `${fid}-count`, dataset: { testid: "text-count" } });
    const error = el("p", { class: "field-error", id: `${fid}-err`, role: "alert", hidden: true, dataset: { testid: "text-error" } });
    const help = el("p", { class: "field__hint", id: `${fid}-help`, dataset: { testid: "text-help" }, text: "Markdown. HTML shows as text. Images do not show. A link starts with /, ?, #, http:// or https://. $name shows the values of a variable." });
    const paint = (lines) => {
      counter.textContent = countText(lines);
      const message = firstTextError(lines);
      error.textContent = message;
      error.hidden = message === "";
      if (message) area.setAttribute("aria-invalid", "true");
      else area.removeAttribute("aria-invalid");
    };
    paint(p.options.content || []);
    const commit = scope.debounce(() => apply((x) => ({ ...x, options: { ...x.options, content: linesOf(area.value) } })), 300);
    area.addEventListener("input", () => {
      paint(linesOf(area.value));
      commit();
    });
    return el("div", { class: "field" }, el("label", { class: "field__label", for: fid, text: "Content" }), area, counter, error, help);
  }

  // --- the options pane ------------------------------------------------------------------------------------------------

  function optionsPane() {
    const p = panel();
    const live = () => (panel().fields && panel().fields.defaults) || {};
    const defaults = live();
    const setDefaults = (patch) => apply((x) => {
      const d = { ...((x.fields && x.fields.defaults) || {}) };
      for (const [k, v] of Object.entries(patch)) {
        if (v === undefined) delete d[k];
        else d[k] = v;
      }
      return { ...x, fields: { ...x.fields, defaults: d } };
    });

    // Panel options
    const title = field("Title", el("input", { class: "input", type: "text", value: p.title }));
    title.control.addEventListener("input", scope.debounce(() => {
      if ([...title.control.value].length > 256) return title.error("The title has 256 characters at most.");
      title.error("");
      apply((x) => ({ ...x, title: title.control.value }));
    }, 300));
    const description = field("Description", el("textarea", { class: "input", rows: 3 }));
    description.control.value = p.description;
    description.control.addEventListener("input", scope.debounce(() => {
      if ([...description.control.value].length > 4096) return description.error("The description has 4096 characters at most.");
      description.error("");
      apply((x) => ({ ...x, description: description.control.value }));
    }, 300));

    // Transparent background (4.13): a switch, as in the panel options of a dashboard tool.
    const transparent = toggle({
      label: "Transparent background",
      checked: p.transparent === true,
      testid: "panel-transparent",
      onChange: (checked) => apply((x) => ({ ...x, transparent: checked })),
    });

    // Position
    const g = p.grid;
    const posRows = {};
    const pos = el("fieldset", { class: "position" }, el("legend", { class: "visually-hidden", text: "Position" }));
    const posError = el("p", { class: "field-error", role: "alert", hidden: true });
    const posGrid = el("div", { class: "position__grid" });
    for (const key of ["x", "y", "w", "h"]) {
      posRows[key] = field(key, el("input", { class: "input input--num", type: "text", inputmode: "numeric", value: String(g[key]), size: 4 }));
      posRows[key].control.addEventListener("input", scope.debounce(() => {
        const next = {};
        for (const k of ["x", "y", "w", "h"]) next[k] = /^-?[0-9]+$/.test(posRows[k].control.value.trim()) ? Number(posRows[k].control.value) : NaN;
        const errors = positionErrors(next);
        posError.textContent = errors.join(" ");
        posError.hidden = errors.length === 0;
        if (errors.length) return;
        apply((x) => x, { layout: (panels) => setPosition(panels, id, next) });
      }, 300));
      // The direction `h` gives x = 0 and w = 24: the two fields show them and do not change them (10.3).
      if (p.repeat && p.repeat.direction === "h" && (key === "x" || key === "w")) posRows[key].control.disabled = true;
      posGrid.append(own(`grid.${key}`, posRows[key].node));
    }
    pos.append(posGrid, posError);

    // Repeat options (4.13a, 10.3)
    const repeatBox = p.type === "text" ? [] : repeatOptions(p);

    // Visualisation
    const types = typeList(p.type, pickType);

    // A text panel (4.33a, 9.19b, 10.3): no standard options, thresholds, colour, mappings, data links, transformations or overrides.
    if (p.type === "text") {
      const links = own("links", linksForm(scope, { scopeName: "panel", get: () => panel().links || [], set: (arr) => apply((x) => ({ ...x, links: arr })) }));
      const linkBadge = () => ((panel().links || []).length ? String((panel().links || []).length) : null);
      const linkGroup = optionGroup({ key: "panellinks", title: "Panel links", open, badge: linkBadge(), children: [links] });
      badgeSetters.push(() => linkGroup.setBadge(linkBadge()));
      return [
        optionGroup({ key: "panel", title: "Panel options", open, children: [own("title", title.node), own("description", description.node), own("transparent", el("div", { class: "field field--switch" }, transparent.el))] }),
        optionGroup({ key: "viz", title: "Visualisation", open, children: [types] }),
        optionGroup({ key: "text", title: "Text", open, children: [own("options.content", textField(p))] }),
        linkGroup,
        optionGroup({ key: "position", title: "Position", open, children: [pos] }),
      ];
    }

    // Standard options (field defaults)
    const unit = field("Unit", el("input", { class: "input", type: "text", value: defaults.unit || "", list: "unit-list", placeholder: "none" }), { hint: "For example short, percent, ms, suffix: pts." });
    const unitList = el("datalist", { id: "unit-list" }, ...UNITS.map((u) => el("option", { value: u })));
    unit.control.addEventListener("input", scope.debounce(() => {
      const v = unit.control.value;
      if (v !== "" && !isUnitValid(v)) return unit.error("This unit is not valid. Use a unit of the list, suffix:<text> or prefix:<text>.");
      unit.error("");
      setDefaults({ unit: v === "" ? undefined : v });
    }, 300));
    const decimals = field("Decimals", el("input", { class: "input input--num", type: "text", inputmode: "numeric", placeholder: "auto", value: defaults.decimals === undefined ? "" : String(defaults.decimals) }), { hint: "0 to 20, or empty." });
    decimals.control.addEventListener("input", scope.debounce(() => {
      const t = decimals.control.value.trim();
      if (t !== "" && (!/^[0-9]+$/.test(t) || Number(t) > 20)) return decimals.error("Use a whole number from 0 to 20, or leave it empty.");
      decimals.error("");
      setDefaults({ decimals: t === "" ? undefined : Number(t) });
    }, 300));
    const numberRow = (name, key) => {
      const r = field(name, el("input", { class: "input input--num", type: "text", inputmode: "decimal", placeholder: "auto", value: defaults[key] === undefined ? "" : String(defaults[key]) }));
      r.control.addEventListener("input", scope.debounce(() => {
        const t = r.control.value.trim();
        if (t !== "" && !Number.isFinite(Number(t))) return r.error("Use a number, or leave it empty.");
        const d = live();
        const other = key === "min" ? d.max : d.min;
        if (t !== "" && other !== undefined && (key === "min" ? Number(t) >= other : Number(t) <= other)) return r.error("Min must be less than max.");
        r.error("");
        setDefaults({ [key]: t === "" ? undefined : Number(t) });
      }, 300));
      return r.node;
    };
    const noValue = field("No value", el("input", { class: "input", type: "text", placeholder: "-", value: defaults.no_value || "" }), { hint: "Empty (not set) or up to 256 characters." });
    noValue.control.addEventListener("input", scope.debounce(() => {
      const t = noValue.control.value;
      if ([...t].length > 256) return noValue.error("Use 256 characters at most.");
      noValue.error("");
      setDefaults({ no_value: t === "" ? undefined : t });
    }, 300));

    // Thresholds and colour (the editors of forms/colors_form.js)
    const thresholds = defaults.thresholds || defaultThresholds();
    const thresholdsBox = thresholdsEditor(scope, { value: thresholds, onCommit: (t) => setDefaults({ thresholds: t }) });
    const colorBox = colorModeEditor({
      value: defaults.color,
      fallback: p.type === "state_timeline" ? "continuous_gr_yl_rd" : p.type === "stat" || p.type === "bargauge" || p.type === "gauge" || p.type === "table" || p.type === "status_history" ? "thresholds" : "palette",
      onCommit: (c) => setDefaults({ color: c }),
    });

    // The forms of 10.3a. Each one reads the draft at each redraw and writes the draft at each valid change.
    const hints = () => {
      const rec = E.draftRt.panels.get(id);
      const src = rec && rec.panel.data.reuse ? E.draftRt.panels.get(rec.panel.data.reuse.panel) : rec;
      return fieldNameHints(src ? src.frame : null);
    };
    const type = () => panel().type;
    const overrides = overridesForm(scope, {
      type,
      hints,
      get: () => (panel().fields && panel().fields.overrides) || [],
      set: (arr) => apply((x) => ({ ...x, fields: { ...x.fields, overrides: arr } })),
    });
    const mappings = mappingsForm(scope, {
      get: () => live().mappings || [],
      set: (arr) => setDefaults({ mappings: arr.length ? arr : undefined }),
    });
    const dataLinks = linksForm(scope, {
      scopeName: "defaults",
      get: () => live().links || [],
      set: (arr) => setDefaults({ links: arr.length ? arr : undefined }),
    });
    const panelLinks = linksForm(scope, {
      scopeName: "panel",
      get: () => panel().links || [],
      set: (arr) => apply((x) => ({ ...x, links: arr })),
    });
    const transforms = transformsForm(scope, {
      hints,
      // The note of `merge` (10.3a): the queries of the panel are `data.query` and `data.extra_queries`.
      queryCount: () => ((panel().data || {}).query ? 1 : 0) + (((panel().data || {}).extra_queries || []).length),
      get: () => panel().transforms || [],
      set: (arr) => apply((x) => ({ ...x, transforms: arr })),
    });
    // The preview has more than 5,000 points: a slider then writes on release (10.3h, rule 7).
    const heavy = () => {
      const rec = E.draftRt.panels.get(id);
      const f = rec && rec.frame;
      return !!(f && f.values && f.values[0] && f.values[0].length * Math.max(1, f.values.length - 1) > 5000);
    };
    optForm = optionForm({ type: p.type, scope, getPanel: panel, apply, hints, open, heavy, counter: optionsPane2 });

    // The groups of the timeline types (4.33b, 10.3): the options, the legend and the card.
    const isTimeline = TIMELINE_TYPES.includes(p.type);
    const timeline = isTimeline ? timelineControls(scope, { type: p.type, heavy, get: () => panel().options, set: (patch) => apply((x) => ({ ...x, options: { ...x.options, ...patch } })) }) : null;
    const count = (n) => (n ? String(n) : null);
    // "Display name" comes after "Max" and before "No value" (10.3h, rule 6). "Show thresholds" is the last row of the group "Thresholds".
    const extra = optForm ? optForm.extra : { standard: [], thresholds: [] };
    const standardGroup = optionGroup({ key: "standard", title: "Standard options", open, children: [own("fields.defaults.unit", unit.node, unitList), el("div", { class: "field-grid" }, own("fields.defaults.decimals", decimals.node), el("span", { class: "field-grid__fill" }), own("fields.defaults.min", numberRow("Min", "min")), own("fields.defaults.max", numberRow("Max", "max"))), ...extra.standard, own("fields.defaults.no_value", noValue.node)] });
    const thresholdsGroup = optionGroup({ key: "thresholds", title: "Thresholds", open, badge: String(thresholds.steps.length), children: [own("fields.defaults.thresholds", thresholdsBox), ...extra.thresholds] });
    if (optForm) {
      optForm.setGroup("standard", standardGroup);
      optForm.setGroup("thresholds", thresholdsGroup);
    }
    const groups = [
      optionGroup({ key: "panel", title: "Panel options", open, children: [own("title", title.node), own("description", description.node), own("transparent", el("div", { class: "field field--switch" }, transparent.el))] }),
      optionGroup({ key: "viz", title: "Visualisation", open, children: [types] }),
      ...(timeline ? [optionGroup({ key: "timeline", title: p.type === "state_timeline" ? "State timeline" : "Status history", open, children: timeline.main }), optionGroup({ key: "legend", title: "Legend", open, children: timeline.legend }), optionGroup({ key: "tooltip", title: "Tooltip", open, children: timeline.tooltip })] : []),
      ...(optForm ? optForm.groups : []),
      // A logs panel has no groups of the field settings: they have no effect on the lines (4.33f, 10.3h).
      ...(p.type === "logs" ? [] : [standardGroup, thresholdsGroup, optionGroup({ key: "color", title: "Colour", open, children: isTimeline ? [own("fields.defaults.color", colorBox), colourNote()] : [own("fields.defaults.color", colorBox)] })]),
      optionGroup({ key: "position", title: "Position", open, children: [pos] }),
      optionGroup({ key: "repeat", title: "Repeat options", open, children: repeatBox }),
    ];
    const withForm = (key, title, node, counter, member) => {
      const g = optionGroup({ key, title, open, badge: count(counter()), children: [own(member, node)] });
      badgeSetters.push(() => g.setBadge(count(counter())));
      groups.push(g);
    };
    if (p.type !== "logs") {
      withForm("overrides", "Overrides", overrides, () => ((panel().fields && panel().fields.overrides) || []).length, "fields.overrides");
      withForm("mappings", "Value mappings", mappings, () => (live().mappings || []).length, "fields.defaults.mappings");
      withForm("datalinks", "Data links", dataLinks, () => (live().links || []).length, "fields.defaults.links");
    }
    withForm("panellinks", "Panel links", panelLinks, () => (panel().links || []).length, "links");
    withForm("transforms", "Transformations", transforms, () => (panel().transforms || []).length, "transforms");
    return groups;
  }

  // The header of the options pane: the picture and the name of the type, and "Change", which opens the group "Visualisation"
  // (the type choice of 10.3) and puts the focus on the type that is chosen now.
  const optionsHead = el("div", { class: "peditor__ohead", dataset: { testid: "options-head" } });
  function paintOptionsHead() {
    clear(optionsHead);
    const p = panel();
    if (!p) return;
    const info = TYPE_INFO[p.type];
    const change = el("button", { type: "button", class: "peditor__change", text: "Change", "aria-label": "Change the visualisation", dataset: { testid: "change-type" }, on: { click: () => {
      open.set("viz", true);
      renderOptions();
      const chosen = optionsHost.querySelector('.typelist__item[aria-pressed="true"]') || optionsHost.querySelector(".typelist__item");
      if (chosen) {
        chosen.scrollIntoView({ block: "nearest" });
        chosen.focus();
      }
    } } });
    optionsHead.append(
      PANEL_TYPES.includes(p.type) ? glyph(p.type) : icon("settings", 16),
      el("span", { class: "peditor__otype", text: info ? info[0] : "Unknown type" }),
      change);
  }

  function renderOptions() {
    scope.flush();
    badgeSetters = [];
    clear(optionsHost);
    paintOptionsHead();
    if (state.tab === "json") return;
    optForm = null;
    optionsHost.append(...optionsPane());
    if (optForm) optionsHost.append(optForm.status);
  }

  // --- tab "Panel": a read-only summary under the preview ---------------------------------------------------------------

  function summaryTab() {
    const p = panel();
    const rows = [];
    const add = (name, value) => rows.push(el("div", { class: "psummary__row" }, el("dt", { text: name }), el("dd", { text: value })));
    const d = (p.fields && p.fields.defaults) || {};
    add("Type", p.type);
    if (p.type === "text") add("Lines", String((p.options.content || []).length));
    else if (p.data && p.data.query) add("Data", `${{ time_series: "Time series", logs: "Logs" }[p.data.query.format] || "Table"} from ${p.data.query.source}`);
    else if (p.data && p.data.reuse) add("Data", `Result of panel ${p.data.reuse.panel}${p.data.reuse.with_transforms ? " with its transformations" : ""}`);
    else add("Data", "No query and no reuse: correct the JSON of the panel");
    if (p.data && p.data.extra_queries) {
      const hidden = queriesOf(p).filter((q) => q.hide).length;
      add("Queries", `${queriesOf(p).length}, ${hidden} hidden`);
    }
    add("Place", `column ${p.grid.x}, row ${p.grid.y}, ${p.grid.w} wide, ${p.grid.h} high`);
    if (p.type !== "text") {
      add("Unit", d.unit || "none");
      add("Thresholds", d.thresholds ? `${d.thresholds.steps.length} steps, ${d.thresholds.mode}` : "default");
      add("Overrides", String(((p.fields && p.fields.overrides) || []).length));
    }
    return el("div", { class: "psummary" },
      el("p", { class: "psummary__hint", text: "Use the options on the right. The preview shows each change at once." }),
      el("dl", { class: "psummary__list" }, ...rows));
  }

  // --- tab "JSON" -----------------------------------------------------------------------------------------------------

  function jsonTab() {
    const p = panel();
    const box = el("div", { class: "jarea" });
    const frame = createJsonArea({ label: "Panel JSON", testid: "panel-json" });
    const area = frame.input;
    area.value = E.panelJsonText && E.panelJsonText.id === id ? E.panelJsonText.text : prettyJson(p).trimEnd();
    frame.paint();
    const syntax = el("p", { class: "field-error", role: "alert", hidden: true });
    const list = el("div", { class: "errors", dataset: { testid: "panel-json-errors" }, hidden: true });
    let lastScan = null;
    const check = debounced(async () => {
      const at = locatePanel(E.doc, id);
      if (!at) return;
      busy.begin();
      try {
        await editorRoutes.validateDocument({ uid: E.uid }, JSON.stringify(E.doc), controller.signal);
        showList([], 0);
      } catch (error) {
        if (error.code === "invalid_document") {
          const { own, other } = errorsOfPanel((error.extra && error.extra.errors) || [], at.section, at.index);
          showList(own, other);
        }
      } finally {
        busy.end();
      }
    }, 1000);
    function showList(own, other) {
      clear(list);
      list.hidden = own.length === 0 && other === 0;
      if (list.hidden) return;
      const ul = el("ul", { class: "error-list" });
      for (const e of own) {
        const b = errorLink(e.pointer || "(panel)", `[${e.code}] ${e.message}`, () => {
          const idx = lastScan ? locatePointer(lastScan.positions, e.pointer) : 0;
          area.focus();
          area.setSelectionRange(idx, idx);
        });
        ul.append(el("li", {}, b));
      }
      list.append(ul);
      if (other) list.append(el("p", {}, `${other} more error${other === 1 ? "" : "s"} in other parts of the document. `, el("button", { type: "button", class: "link-button", text: "Open the dashboard JSON", on: { click: () => E.dispatch("open-json") } })));
    }
    area.addEventListener("input", () => {
      E.panelJsonText = { id, text: area.value };
      const scan = scanJson(area.value);
      lastScan = scan;
      if (!scan.ok || scan.value === null || typeof scan.value !== "object" || Array.isArray(scan.value)) {
        const e = scan.errors[0];
        syntax.textContent = e ? `Line ${e.line}, column ${e.column}: ${e.message}` : "The panel must be a JSON object.";
        syntax.hidden = false;
        return;
      }
      syntax.hidden = true;
      apply(() => scan.value);
      check();
    });
    box.append(
      el("div", { class: "jarea__head" }, el("span", { class: "jarea__title", text: "Panel JSON" }), el("span", { class: "muted", text: "This tab edits the whole panel, with the options of the type. The forms edit the same members." })),
      frame.el, syntax, list);
    check();
    return box;
  }

  // --- tabs --------------------------------------------------------------------------------------------------------------

  const TABS = [["query", "Query"], ["panel", "Panel"], ["json", "JSON"]];

  // An open popover of the tab that leaves removes its listeners of the window and of the document.
  function releasePopovers() {
    for (const close of popovers) close();
    popovers = [];
  }

  // The tab "Panel" has no lower area: the preview is large and the options pane stays on the right.
  function renderTab() {
    releasePopovers();
    sqlEditor = null;
    runButton = null;
    statusLine = null;
    clear(body);
    root.dataset.tab = state.tab;
    body.hidden = false;
    if (state.tab === "query") body.append(queryTab());
    else if (state.tab === "json") body.append(jsonTab());
    else body.append(summaryTab());
    renderOptions();
    tabs.querySelectorAll("[role=tab]").forEach((t) => {
      const on = t.dataset.tab === state.tab;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
    });
    showErrors();
  }

  function selectTab(name) {
    scope.flush();
    state.tab = name;
    E.panelJsonText = null;
    E.panelTab = name;
    renderTab();
  }

  for (const [name, label] of TABS) {
    tabs.append(el("button", { type: "button", role: "tab", class: "tab", text: label, dataset: { tab: name, testid: `tab-${name}` }, on: { click: () => selectTab(name), keydown: (e) => {
      const at = TABS.findIndex((t) => t[0] === state.tab);
      if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
        e.preventDefault();
        selectTab(TABS[(at + (e.key === "ArrowRight" ? 1 : TABS.length - 1)) % TABS.length][0]);
        tabs.querySelector("[aria-selected=true]").focus();
      }
    } } }));
  }

  // The panel editor has no top bar and no row of its own. "Save" and "Revert" are the buttons of the header (10.2). The title
  // The header has no name for this view, and the arrow after the selector is the way back (edit_mode.js backLink): the event `leave` (10.2a).
  const optionsPane2 = el("aside", { class: "peditor__options", "aria-label": "Panel options" },
    optionsHead, optionsHost);
  const mainBox = el("div", { class: "peditor__main" },
    el("div", { class: "peditor__stage" }, topHost, el("div", { class: "peditor__lower" }, tabs, body, errorsBox)),
    optionsPane2);
  root.append(mainBox);
  // The toolbar row of the page (time range, refresh) sits above the preview, as in Grafana.
  E.api.dockToolbar(root, mainBox);
  drawTop();
  renderTab();

  return {
    el: root,
    destroy() {
      scope.flush();
      // The last text of the SQL area goes to the draft first. This change redraws the preview, so the frame is destroyed after it.
      // An error of the flush does not stop the end of the editor.
      try {
        if (sqlEditor) sqlEditor.flush();
      } finally {
        ended = true;
        E.api.undockToolbar();
        controller.abort();
        offPanel();
        if (topFrame) topFrame.destroy();
        releasePopovers();
      }
    },
    // The text fields apply their last change at once (a save asks for it).
    flush: () => scope.flush(),
    // The draft went back to the saved document: the panel may have changed or gone.
    onDiscard() {
      // A change that waits for its 300 ms must not write again over the saved document.
      scope.cancel();
      if (!panel()) {
        // A new panel is gone: this is the event `discard` of a new panel, with the rules of `leave` (10.2a).
        E.api.dispatch("discard", null, { newPanel: true });
        return;
      }
      drawTop();
      renderTab();
    },
    // After a failed save: the errors of the last save show here.
    refreshErrors: showErrors,
    focusPointer(pointer) {
      const m = sqlPointer(pointer);
      if (m && m.index !== state.qi && m.index < queriesOf(panel()).length) {
        state.qi = m.index;
        renderTab();
      }
      if (m && sqlEditor) sqlEditor.focusAt(m.line + 1, 1);
    },
  };
}
