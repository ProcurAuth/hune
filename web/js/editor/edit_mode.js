// Edit mode (10.2): the edit bar, the draft grid, the panel menu, the section controls, the draft in localStorage,
// and the routing between the views of the editor (panel editor, dashboard JSON, history).
// The page loads this module on demand with import(). The entry is enter(editorApi, mode).

import * as editorRoutes from "./api_editor.js";
import { createGrid } from "../grid.js";
import { createPanelFrame } from "../panel_frame.js";
import { STEPS, maxQuery } from "../header_layout.js";
import { gridDoc } from "../var_layout.js";
import { splitLayout } from "./draft.js";
import { placedMenu } from "./place.js";
import { badge, button, clear, el, icon, notify, openDialog, openMenu, dismissNotice } from "../ui.js";
import { attachDrag } from "./drag.js";
import { loadEditorCss, stubFrame } from "./form.js";
import { DraftRuntime } from "./draft_runtime.js";
import { frameAdopter } from "./adopt.js";
import {
  PANEL_TYPES, isDrawable, addPanel, annotationsNaming, addSection, canMoveSection, defaultPlacement, deletePanel, deleteSection, moveSection, draftDiffers, duplicatePanel, lastPlainSection, locatePanel, makeRecord, newPanel, newTextPanel, placementRefusal, renameSection, repeatableVariables, sectionRepeatRefusal, setCollapsed, setPlacement, setSectionRepeat, sameJson,
} from "./draft.js";
import { loadDraft, removeStored, writeStored, draftKey } from "./draft_store.js";
import { canMove, movePanel } from "./layout.js";
import { prettyJson } from "./json_check.js";
import { openSaveDialog, announceSaved } from "./save.js";
import { timeDifferences } from "./time_defaults.js";
import { SaveAttempt } from "./save_logic.js";

const fill = (bytes) => crypto.getRandomValues(bytes);

// True when "Revert" has no button in the header and the menu "More actions" has it (9.6, the window is 640 px or less).
function revertInMenu() {
  try {
    return window.matchMedia(maxQuery(STEPS.revertMenuMax)).matches;
  } catch (error) {
    return false;
  }
}
let current = null; // { api, editor }

export async function enter(editorApi, target) {
  if (current && current.api !== editorApi) {
    current.editor.dispose();
    current = null;
  }
  if (!current) current = { api: editorApi, editor: new Editor(editorApi) };
  await current.editor.open(target);
}

// The editor of the current page, for the other modules and the tests.
export function currentEditor() {
  return current ? current.editor : null;
}

const KINDS = { edit: "grid", "panel-edit": "panel", json: "json", settings: "settings", history: "history" };
const REGION_LABELS = { grid: "Dashboard edit", panel: "Panel editor", json: "Dashboard JSON", settings: "Dashboard settings", history: "History" };

class Editor {
  constructor(pageApi) {
    this.api = pageApi;
    this.uid = pageApi.uid;
    this.session = pageApi.session;
    this.sources = [];
    this.record = null;
    this.editing = false;
    this.view = null; // { kind, key, inst }
    this.started = false; // true after the first view is in the page
    this.openToken = 0;
    this.attempt = new SaveAttempt(fill);
    this.canonical = null;
    this.lastErrors = null;
    this.panelTab = "query";
    this.panelJsonText = null;
    this.persistTimer = 0;
    this.storageWarned = false;
    this.listeners = [];
    this.draftRt = null;
    this.closed = false;
    this.bar = null; // the refs of "Add", "Save" and "Revert" in the header: { saveButton, discard, addMain, addMenu, state }
    this.barNode = null; // the group of these buttons
  }

  // The page shows the switch "Edit" as on while the editor holds the draft.
  setEditing(on) {
    this.editing = on;
    this.api.setEditActive(on);
  }

  // --- the base document ------------------------------------------------------------------------------------------

  get base() {
    const l = this.api.loaded;
    return { doc: l.doc, etag: l.etag, version: l.version, folder: l.folder, access: l.access };
  }

  get doc() {
    return this.record ? this.record.document : this.base.doc;
  }

  get canEdit() {
    return this.session.editing === true && this.base.access && this.base.access.write === true;
  }

  dirty() {
    return !!this.record && draftDiffers(this.record, this.base.doc);
  }

  // The text of the draft, for a download (10.10).
  draftText() {
    return this.record && this.record.json_text ? this.record.json_text : prettyJson(this.doc);
  }

  // --- start and stop -----------------------------------------------------------------------------------------------

  // Shows the state in the page (10.2a). The modules, the style sheet and the list of the sources load first, with the old view still in the page.
  // The swap of the views is then one synchronous step, so no frame is blank or shows two views (rule 2).
  async open(target) {
    const token = ++this.openToken;
    const plan = await this.prepare(target);
    if (token !== this.openToken || this.closed) return;
    if (this.api.state !== target) {
      // The page left this state while the modules loaded.
      if (!this.started) this.dispose();
      return;
    }
    this.swap(target, plan);
  }

  async prepare(target) {
    await loadEditorCss();
    if (!this.started && this.sources.length === 0) {
      try {
        const answer = await editorRoutes.sources();
        this.sources = [...(answer.items || [])].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
      } catch (error) {
        this.sources = [];
      }
    }
    const kind = KINDS[target.page];
    const key = `${kind}:${target.panel ?? ""}:${target.version ?? ""}:${target.base ?? ""}`;
    let create;
    if (kind === "panel") {
      const m = await import("./panel_editor.js");
      create = () => m.createPanelEditor(this, target.panel);
    } else if (kind === "json") {
      const m = await import("./json_editor.js");
      create = () => m.createJsonEditor(this);
    } else if (kind === "settings") {
      const m = await import("./forms/settings.js");
      create = () => m.createSettings(this);
    } else if (kind === "history") {
      const m = await import("./history.js");
      create = () => m.createHistory(this);
    } else create = () => this.createGridView();
    return { kind, key, create };
  }

  // The first view: the runtime of the draft, the hooks of the page and the listeners.
  begin() {
    this.started = true;
    this.makeRuntime();
    this.api.setEditHooks({
      onStale: () => notify({ level: "warning", key: "stale", text: "The dashboard changed. Save to compare your changes with the new version." }),
      dirty: () => this.dirty(),
      confirmLeave: (then) => this.confirmLeave(then),
      close: () => this.close(),
      rebuilt: () => this.rebuilt(),
      // The ids of the panels of the draft, for the URL of a panel editor (10.2a). Null while the editor holds no draft.
      panelIds: () => (this.record ? this.doc.sections.flatMap((section) => section.panels.map((panel) => panel.id)) : null),
      menuItems: () => this.headerMenuItems(),
      destroy: () => this.dispose(),
    });
    this.api.pauseBase();
    this.listen(window, "beforeunload", (e) => {
      if (!this.dirty()) return;
      this.persistNow();
      e.preventDefault();
      e.returnValue = "";
    });
    this.listen(window, "storage", (e) => {
      if (!this.editing || e.key !== draftKey(this.uid)) return;
      let local = false;
      try {
        local = e.storageArea === window.localStorage;
      } catch (error) {
        local = false;
      }
      if (local) notify({ level: "warning", key: "tab", text: "This dashboard is open for edit in another tab." });
    });
    this.listen(document, "click", (e) => this.guardLink(e), true);
    // A change of the range, the refresh or a variable in the header: "Save" may become active or inactive (10.9).
    this.listen(document, "hune-query", () => this.paintBar());
    this.listen(document, "keydown", (e) => {
      if (this.editing && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        this.save();
      }
    });
  }

  // The way back of the panel editor is the arrow in the header, before the selector (10.3). Its name is the true target of the event
  // `leave` (10.2a): the panel view, the dashboard or the edit mode. A click with no modifier key is the event; the address serves the others.
  backLink() {
    const origin = this.api.origin;
    const page = origin ? origin.state.page : "edit";
    const label = page === "panel-view" ? "Back to panel view" : page === "view" ? "Back to dashboard" : "Back to edit mode";
    const click = (e) => {
      if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
      e.preventDefault();
      if (this.view && this.view.inst.flush) this.view.inst.flush();
      this.api.dispatch("leave");
    };
    return { label, attrs: { dataset: { testid: "editor-back" }, on: { click } } };
  }

  // The synchronous swap of the views for a state (10.2a).
  swap(target, plan) {
    if (!this.started) this.begin();
    if (target.editing) this.beginEdit();
    else if (this.editing) this.endEditing();
    if (target.page === "panel-edit" && !locatePanel(this.doc, target.panel)) {
      // The draft has no such panel: the page shows the dashboard edit instead.
      notify({ level: "warning", key: "state-no-panel", text: `This dashboard has no panel ${target.panel}.` });
      queueMicrotask(() => this.api.dispatch("leave"));
      return;
    }
    if (!this.view || this.view.key !== plan.key) {
      this.disposeView();
      const inst = plan.create();
      this.view = { kind: plan.kind, key: plan.key, inst };
      this.api.setViewTail(plan.kind === "panel", plan.kind === "panel" ? this.backLink() : null);
      const host = this.api.editHost;
      clear(host);
      host.append(inst.el);
      // The editor view is a region. The focus goes to it, and a pointer user sees no ring (10.2a, rule 14).
      inst.el.setAttribute("role", "region");
      inst.el.setAttribute("aria-label", REGION_LABELS[plan.kind]);
      inst.el.tabIndex = -1;
      inst.el.style.outline = "0";
      const focus = inst.el.querySelector("[data-autofocus]") || inst.el;
      // The dashboard edit that the switch opened keeps the focus on the switch, so the same key turns it off.
      this.api.whenShown(() => {
        const now = document.activeElement;
        if (plan.kind === "grid" && now && now.dataset && now.dataset.testid === "edit-toggle") return;
        focus.focus({ preventScroll: true });
      });
    }
    // "Save" and "Revert" are in the header while the editor holds a draft, in every view of it.
    if (this.editing) this.mountBar();
    else this.unmountBar();
    this.paintBar();
    this.api.shown(target);
    this.started = true;
  }

  makeRuntime() {
    if (this.draftRt) this.draftRt.destroy();
    this.draftRt = new DraftRuntime({ base: this.api.runtime, doc: this.doc, onCollapse: (i, collapsed) => this.onCollapse(i, collapsed) });
    this.editPage = {
      get rt() {
        return editor.draftRt;
      },
      zone: this.base.doc.time.timezone,
      syncKey: "",
      linkContext: this.api.linkContext,
      panelUrl: (id) => this.api.pageApi.panelUrl(id),
      noView: true, // the panel menu has no item "View" in edit mode (10.2a, rule 5)
      noMarks: true, // a frame that the draft grid takes from the base grid draws no mark (9.19d, rule 22)
      viewPanel: () => {},
      onZoom: () => {},
      extraMenuItems: (panel) => this.panelMenu(panel),
      placedMenu: (name) => placedMenu(this, name),
    };
    const editor = this;
  }

  listen(target, type, fn, capture = false) {
    target.addEventListener(type, fn, capture);
    this.listeners.push(() => target.removeEventListener(type, fn, capture));
  }

  dispose() {
    if (this.closed) return;
    this.closed = true;
    clearTimeout(this.persistTimer);
    this.persistNow();
    this.unmountBar();
    this.disposeView();
    const host = this.api.editHost;
    if (host) clear(host);
    if (this.draftRt) this.draftRt.destroy();
    for (const off of this.listeners) off();
    this.listeners = [];
    if (current && current.editor === this) current = null;
  }

  // Leaves the editor: the page shows its own view again. The page chooses the state and writes the URL (10.2a).
  close() {
    if (this.closed) return;
    const api2 = this.api;
    this.dispose();
    api2.setEditActive(false);
    api2.setViewTail(null);
    api2.setEditHooks(null);
    api2.resumeBase();
  }

  // After a reload of the page ("Load theirs"): the runtime is new, so the draft runtime is new too, and the view shows again.
  rebuilt() {
    if (this.closed) return;
    this.disposeView();
    this.unmountBar();
    this.makeRuntime();
    this.api.pauseBase();
    this.open(this.api.state);
  }

  // The end of edit mode in a state that stays in the editor (the history, 10.2a): the draft stays in the browser when it has changes.
  endEditing() {
    this.persistNow();
    this.record = null;
    this.setEditing(false);
    this.unmountBar();
  }

  // --- the draft -------------------------------------------------------------------------------------------------------

  beginEdit() {
    if (this.editing) return;
    this.setEditing(true);
    const found = loadDraft(this.uid, this.session.subject);
    if (found.draft) {
      this.record = found.draft;
      this.draftRt.setDoc(this.record.document);
      notify({ level: "info", key: "resumed", text: "Your draft is back." });
    } else {
      this.record = makeRecord({ baseEtag: this.base.etag, baseVersion: this.base.version || 0, subject: this.session.subject, folder: this.base.folder, document: this.base.doc, now: Date.now() });
    }
    this.checkNotices();
  }

  // The notices of 10.2 and 10.14.
  checkNotices() {
    if (!sameJson(this.doc.variables, this.base.doc.variables)) {
      notify({ level: "info", key: "vars", text: "Save the dashboard to load the options of the changed variables." });
    } else dismissNotice("vars");
    const known = new Set(this.sources.map((s) => s.id));
    const missing = new Set();
    for (const s of this.doc.sections) for (const p of s.panels) if (p.data && p.data.query && !known.has(p.data.query.source)) missing.add(p.data.query.source);
    for (const id of missing) notify({ level: "warning", key: `source-${id}`, text: `A save needs the query right on the source ${id}.` });
  }

  // A change of the draft document (10.2). It ends the save attempt (10.9, rule 5).
  change(doc, { clearJsonText = false } = {}) {
    if (!this.record) return;
    this.record.document = doc;
    if (clearJsonText) this.record.json_text = null;
    this.attempt.reset();
    this.record.save_id = null;
    this.draftRt.setDoc(doc);
    this.schedulePersist();
    this.checkNotices();
    if (this.view && this.view.inst.onChange) this.view.inst.onChange();
    this.paintBar();
  }

  // The text of the dashboard JSON tab that is not applied (10.4).
  setJsonText(text) {
    if (!this.record) return;
    this.record.json_text = text;
    this.attempt.reset();
    this.record.save_id = null;
    this.schedulePersist();
    this.paintBar();
  }

  schedulePersist() {
    clearTimeout(this.persistTimer);
    this.persistTimer = setTimeout(() => this.persistNow(), 500);
  }

  persistNow() {
    clearTimeout(this.persistTimer);
    if (!this.editing || !this.record) return;
    if (!this.dirty()) {
      removeStored(this.uid);
      return;
    }
    this.record.updated_at_ms = Date.now();
    if (!writeStored(this.uid, this.record) && !this.storageWarned) {
      this.storageWarned = true;
      notify({ level: "warning", text: "Hune cannot keep your draft in this browser. Save soon." });
    }
  }

  // `quiet`: the exit dialog discards the draft and then leaves, so the view does not navigate by itself.
  discard({ quiet = false } = {}) {
    removeStored(this.uid);
    this.record = makeRecord({ baseEtag: this.base.etag, baseVersion: this.base.version || 0, subject: this.session.subject, folder: this.base.folder, document: this.base.doc, now: Date.now() });
    this.attempt.reset();
    this.canonical = null;
    this.lastErrors = null;
    this.draftRt.setDoc(this.base.doc);
    if (!quiet && this.view && this.view.inst.onDiscard) this.view.inst.onDiscard();
    else if (this.view && this.view.inst.onChange) this.view.inst.onChange();
    this.paintBar();
    notify({ level: "info", text: "The draft is discarded." });
  }

  // After a save that succeeds (10.9): the draft goes, the saved document is the base, edit mode ends (10.2a, rule 7).
  async afterSaved(answer) {
    removeStored(this.uid);
    this.record = null;
    this.setEditing(false);
    await this.api.finishEdit("save");
    announceSaved(answer);
  }

  // "Load theirs" of the conflict screen (10.10): the draft goes, the current version is the new base, edit continues.
  async loadTheirs() {
    removeStored(this.uid);
    this.record = null;
    this.setEditing(false);
    await this.api.reload({ notice: false });
  }

  save() {
    if (!this.editing) return;
    // A text field with a change that waits applies it first.
    if (this.view && this.view.inst.flush) this.view.inst.flush();
    openSaveDialog(this);
  }

  // Asks what to do with the changes before a link or "Exit" leaves edit mode (10.2).
  confirmLeave(then) {
    if (!this.dirty()) {
      then();
      return;
    }
    openDialog({
      title: "Exit edit mode",
      testid: "exit-dialog",
      build: (body) => body.append(el("p", { text: "You have changes that are not saved." })),
      buttons: [
        { label: "Stay" },
        { label: "Discard", onClick: () => { this.discard({ quiet: true }); then(); } },
        { label: "Save", primary: true, onClick: () => { openSaveDialog(this, { onSaved: () => then() }); } },
      ],
    });
  }

  guardLink(event) {
    if (!this.editing || !this.dirty() || event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey) return;
    const link = event.target instanceof Element ? event.target.closest("a[href]") : null;
    if (!link || link.target || link.hasAttribute("download") || link.dataset.external === "true" || link.closest("dialog, .editor-host")) return;
    const url = new URL(link.href, document.baseURI);
    if (url.origin !== window.location.origin) return;
    // A link to the edit mode of this dashboard (the way back of the panel editor) does not leave it.
    if (url.pathname === window.location.pathname && url.searchParams.get("edit") === "1") return;
    event.preventDefault();
    event.stopImmediatePropagation();
    const navigate = this.api.navigate;
    this.confirmLeave(() => {
      this.close();
      navigate(url);
    });
  }

  // --- events ------------------------------------------------------------------------------------------------------------

  // An event of 10.2a from a control of the editor. A JSON text that is not applied needs a confirmation before the view goes (10.4).
  dispatch(event, arg, extra) {
    const kind = this.view && this.view.kind;
    const staying = (event === "open-json" && kind === "json") || (event === "open-settings" && kind === "settings");
    const leaving = ["leave", "open-json", "open-settings", "open-history", "panel-edit"].includes(event);
    if (leaving && !staying && (kind === "json" || kind === "settings") && this.view.inst.hasUnapplied()) {
      openDialog({
        title: "Leave the JSON",
        build: (b) => b.append(el("p", { text: "The text of the JSON is not applied. It stays in your draft. Leave the tab?" })),
        buttons: [{ label: "Stay" }, { label: "Leave", primary: true, onClick: () => this.api.dispatch(event, arg, extra) }],
      });
      return Promise.resolve();
    }
    return this.api.dispatch(event, arg, extra);
  }

  disposeView() {
    if (this.view) {
      if (this.view.inst.destroy) this.view.inst.destroy();
      this.view = null;
    }
  }

  openJsonAt(pointer) {
    this.dispatch("open-json");
    const wait = () => (this.view && this.view.kind === "json" ? this.view.inst.focusPointer(pointer) : setTimeout(wait, 50));
    setTimeout(wait, 50);
  }

  // --- the draft grid ----------------------------------------------------------------------------------------------------------

  onCollapse(index, collapsed) {
    if (!this.record) return;
    this.change(setCollapsed(this.doc, index, collapsed));
  }

  createGridView() {
    const E = this;
    const holder = el("div", { class: "edit-grid", dataset: { testid: "edit-grid" } });
    let grid = null;
    let drag = null;
    // A text for assistive technology after a change of the layout by the keys (10.16).
    const live = el("p", { class: "visually-hidden", role: "status", "aria-live": "polite", dataset: { testid: "layout-status" } });
    // A panel that the draft does not change keeps the frame of the base grid, with its chart and its result (10.2a, rule 17).
    const adopter = frameAdopter({ api: E.api, draft: E.draftRt, page: E.editPage });
    const build = () => {
      if (drag) drag.destroy();
      // The frames return to the base grid, and the new grid takes them in the same task: the page never shows them out of the document.
      adopter.giveBack(grid);
      if (grid) grid.destroy();
      grid = createGrid({
        doc: gridDoc(E.doc),
        rt: E.draftRt,
        makeFrame: (panel, inst) => {
          if (!isDrawable(panel.type)) return stubFrame(panel);
          const frame = adopter.take(panel, inst) || createPanelFrame({ panel, page: E.editPage, instance: inst });
          // A copy n from 2 is a view: it has no menu button (10.5a).
          if (inst && inst.n !== null && inst.n >= 2) frame.el.querySelector(".panel__menu").remove();
          return frame;
        },
        sectionExtras: (index, section) => E.sectionExtras(index, section),
      });
      holder.replaceChildren(grid.el);
      // An empty section says what to do (the grid shows nothing for it).
      E.doc.sections.forEach((section, i) => {
        if (section.panels.length) return;
        const box = grid.el.querySelector(`[data-testid="section-${i}"] .dash__grid`);
        if (box) box.append(el("p", { class: "section__empty", text: "This section has no panels yet. Use Add panel here, or the button Add in the header." }));
      });
      for (const frame of grid.frames.values()) frame.update();
      drag = attachDrag({
        view: grid,
        panelsOf: (index) => gridDoc(E.doc).sections[index].panels,
        commit: (index, panels, info) => E.commitLayout(index, panels, info),
        announce: (text) => {
          live.textContent = "";
          live.textContent = text;
        },
      });
    };
    build();
    // The frames show the result of their panel: of the base runtime, or of "Run" (10.2).
    // A new selection or new options change the copies of the draft: the grid is drawn again (9.8c).
    const off = E.draftRt.on((type, rec) => {
      if (!grid) return;
      if (type === "instances") {
        build();
        return;
      }
      if (type !== "panel") return;
      const frame = grid.frames.get(rec.key);
      if (frame) frame.update();
    });
    const root = el("div", { class: "editmode" }, holder, live);
    return {
      el: root,
      onChange: build,
      announce: (text) => {
        live.textContent = "";
        live.textContent = text;
      },
      // After a change by the keys the grid is new: the focus goes back to the panel that moved.
      focusPanel: (id) => {
        const frame = grid && grid.frames.get(id);
        const target = frame && frame.el.querySelector(".panel__menu");
        if (target) target.focus();
      },
      destroy: () => {
        off();
        if (drag) drag.destroy();
        adopter.giveBack(grid);
        if (grid) grid.destroy();
      },
    };
  }

  // The group of the edit mode in the header row (10.2), before the time control: "Add" (a split button), "Save" and "Revert". It is in the
  // page only in edit mode and takes its room from the selector, so no button of the view moves. "Save" is the primary button. "Revert"
  // discards all the changes after a question.
  buildBar() {
    const saveButton = button("Save", { dataset: { testid: "save-button" }, on: { click: () => this.save() } }, "save", { variant: "tb" });
    saveButton.classList.add("btn--tb-primary", "edit-save");
    const discard = button("Revert all changes", { class: "btn btn--tb btn--icon edit-revert", dataset: { testid: "discard-button" }, on: { click: () => this.confirmDiscard() } }, "undo", { iconOnly: true, variant: "tb" });
    // "Add" is a split button: the main part adds a panel at once (one click), the chevron opens the menu of the other additions.
    // The label "Add" hides up to 900 px; the name and the tooltip stay "Add panel".
    const addMain = button("Add", { class: "btn btn--tb add-main", "aria-label": "Add panel", "data-tooltip": "Add panel", dataset: { testid: "add-button" }, on: { click: () => this.addPanelTo(this.doc.sections.length - 1) } }, "plus", { variant: "tb" });
    const addMenu = button("Add menu", { class: "btn btn--tb add-menu", "aria-haspopup": "menu", "aria-expanded": "false", "data-tooltip": "Add menu", dataset: { testid: "add-menu", tooltipAlign: "end" } }, "chevron-down", { variant: "tb", iconOnly: true });
    addMenu.addEventListener("click", () => openMenu(addMenu, this.addItems(), { align: "end", gap: 8, testid: "add-menu-list" }));
    addMenu.addEventListener("keydown", (e) => {
      if (e.key === "ArrowDown" && !document.querySelector(".menu")) {
        e.preventDefault();
        addMenu.click();
      }
    });
    const addGroup = el("div", { class: "tb-group add-group", role: "group", "aria-label": "Add", dataset: { testid: "add-group" } }, addMain, addMenu);
    // The state of the draft has no visible text (10.2). The Save button carries it: active with changes, inactive without.
    // This hidden live region tells a screen reader the same state.
    const state = el("span", { class: "editbar__state visually-hidden", role: "status" });
    this.bar = { saveButton, discard, addMain, addMenu, state };
    this.barNode = el("div", { class: "editbar", role: "group", "aria-label": "Edit mode actions", dataset: { testid: "edit-bar" } }, state, addGroup, saveButton, discard);
    this.paintBar();
  }

  mountBar() {
    if (!this.barNode) this.buildBar();
    this.api.setEditBar(this.barNode);
  }

  unmountBar() {
    this.api.setEditBar(null);
    this.barNode = null;
    this.bar = null;
  }

  // The items of the menu of "Add" (10.2): the panel, the section and the placement of a variable in the grid (10.6).
  addItems() {
    return [
      { label: "Add panel", icon: "plus", testid: "add-panel", onSelect: () => this.addPanelTo(this.doc.sections.length - 1) },
      { label: "Add text panel", icon: "plus", testid: "add-text", onSelect: () => this.addTextPanelTo(this.doc.sections.length - 1) },
      { label: "Add section", icon: "folder-plus", testid: "add-section", onSelect: () => this.addSectionHere() },
      { separator: true },
      { label: "Place a variable in the grid", icon: "grid", testid: "add-variable", disabled: this.unplaced().length === 0 || lastPlainSection(this.doc) < 0, onSelect: () => this.placeVariable() },
    ];
  }

  addSectionHere() {
    this.change(addSection(this.doc));
    this.announce("Section added.");
  }

  announce(text) {
    if (this.view && this.view.inst.announce) this.view.inst.announce(text);
  }

  // The variables that a control can show in the grid and that are in the top bar now (4.20).
  unplaced() {
    return this.doc.variables.filter((v) => !v.placement && !placementRefusal(v));
  }

  // One such variable goes to the grid at once. Several: a menu lets the user choose. The control goes to the bottom of the last section, as
  // the switch "Place in the grid" of the form does (10.3b).
  placeVariable() {
    const list = this.unplaced();
    const put = (v) => {
      // A section that repeats has no placement (4.7a, rule 11): the control goes to the last section that does not repeat.
      this.change(setPlacement(this.doc, v.name, defaultPlacement(this.doc, lastPlainSection(this.doc))));
      this.announce(`The control of the variable ${v.name} is in the grid.`);
    };
    if (list.length === 1) put(list[0]);
    else if (list.length > 1) {
      openMenu(this.bar.addMenu, [{ heading: "Place a variable" }, ...list.map((v) => ({ label: v.label || v.name, testid: `add-variable-${v.name}`, onSelect: () => put(v) }))], { align: "end", gap: 8, testid: "add-variable-list" });
    }
  }

  // The actions that the header has no button for (10.2): they are at the top of the menu "More actions" while the editor holds a draft.
  // "Dashboard JSON" and "Settings" show in the other views.
  headerMenuItems() {
    if (!this.editing || this.closed) return [];
    const kind = this.view ? this.view.kind : null;
    const views = [];
    if (revertInMenu()) views.push({ label: "Revert all changes", icon: "undo", testid: "discard-menu", disabled: !this.dirty(), onSelect: () => this.confirmDiscard() }, { separator: true });
    if (kind !== "json") views.push({ label: "Dashboard JSON", icon: "code", testid: "json-tab", onSelect: () => this.dispatch("open-json") });
    if (kind !== "settings") views.push({ label: "Settings", icon: "settings", testid: "dash-settings", onSelect: () => this.dispatch("open-settings") });
    return views;
  }

  paintBar() {
    if (!this.bar) return;
    const dirty = this.dirty();
    // "Save" is active when the draft changed, or when the current range or refresh differs from the default of the draft: the Save dialog then
    // offers them as the defaults (10.9). "Revert" acts on the draft only.
    const time = this.record ? timeDifferences(this.doc.time, this.api.pageApi.currentTime) : { range: false, refresh: false };
    const timeOnly = !dirty && (time.range || time.refresh);
    this.bar.saveButton.disabled = !dirty && !timeOnly;
    this.bar.discard.disabled = !dirty;
    // "Add" acts on the draft grid. In the other views it stays in its place and is not active, so the header does not change (10.2).
    const grid = Boolean(this.view) && this.view.kind === "grid";
    this.bar.addMain.disabled = !grid;
    this.bar.addMenu.disabled = !grid;
    this.bar.saveButton.title = dirty ? "Save the changes" : timeOnly ? "Save the time range or the refresh as the default" : "There are no changes to save";
    this.bar.state.dataset.dirty = String(dirty);
    this.bar.state.textContent = dirty ? "Unsaved changes" : "No changes";
  }

  confirmDiscard() {
    if (!this.dirty()) return;
    openDialog({
      title: "Discard your changes",
      build: (b) => b.append(el("p", { text: "Discard your changes and show the saved dashboard?" })),
      buttons: [{ label: "Stay" }, { label: "Discard", danger: true, onClick: () => this.discard() }],
    });
  }

  addPanelTo(sectionIndex) {
    if (!this.sources.length) {
      notify({ level: "warning", text: "No source is available to you." });
      return;
    }
    const panel = newPanel(this.doc, sectionIndex, this.sources[0].id);
    this.change(addPanel(this.doc, sectionIndex, panel));
    this.dispatch("panel-edit", panel.id);
  }

  // "Add text panel" (10.6): a panel with no query needs no source, so it works when the list of the sources is empty.
  addTextPanelTo(sectionIndex) {
    const panel = newTextPanel(this.doc, sectionIndex);
    this.change(addPanel(this.doc, sectionIndex, panel));
    this.dispatch("panel-edit", panel.id);
  }

  panelMenu(panel) {
    const at = locatePanel(this.doc, panel.id);
    const panels = at ? this.doc.sections[at.section].panels : [];
    const move = (dir) => {
      const next = movePanel(panels, panel.id, dir);
      if (!next) return;
      const doc = structuredClone(this.doc);
      doc.sections[at.section].panels = next;
      this.change(doc);
    };
    return [
      { separator: true },
      { label: "Edit", onSelect: () => this.dispatch("panel-edit", panel.id), testid: `panel-${panel.id}-edit` },
      { label: "Duplicate", onSelect: () => this.change(duplicatePanel(this.doc, panel.id).doc), testid: `panel-${panel.id}-duplicate` },
      { label: "Move up", disabled: !canMove(panels, panel.id, -1), onSelect: () => move(-1), testid: `panel-${panel.id}-up` },
      { label: "Move down", disabled: !canMove(panels, panel.id, 1), onSelect: () => move(1), testid: `panel-${panel.id}-down` },
      { label: "Delete", onSelect: () => this.confirmDelete(panel), testid: `panel-${panel.id}-delete` },
    ];
  }

  // A new layout of one section from a drag, a resize or the arrow keys (10.16). It goes through the same path as "Move up" and the form
  // "Position": the draft changes, so the bar, the draft in the browser storage, Save and the conflict rules (10.9, 10.10) stay right.
  commitLayout(index, panels, info) {
    this.change(splitLayout(this.doc, index, panels));
    if (info && info.kind === "key" && this.view && this.view.inst.focusPanel) this.view.inst.focusPanel(info.id);
  }

  moveSectionBy(index, dir) {
    const next = moveSection(this.doc, index, dir);
    if (next) this.change(next);
  }

  confirmDeleteSection(index, section) {
    const result = deleteSection(this.doc, index);
    if (result.error) {
      notify({ level: "warning", text: result.error });
      return;
    }
    if (result.refused) {
      notify({ level: "warning", text: `The panels ${result.refused.join(", ")} reuse a panel of this section. Change them first.` });
      return;
    }
    const count = section.panels.length;
    // A placement in this section goes with it: its variable goes back to the top bar (10.6).
    const placed = (this.doc.variables || []).filter((v) => v.placement && v.placement.section === index).length;
    const placedText = placed ? ` ${placed === 1 ? "One placed control goes" : `${placed} placed controls go`} back to the top bar.` : "";
    openDialog({
      title: `Delete the section ${section.title}?`,
      testid: "delete-section-dialog",
      build: (b) => b.append(el("p", { text: (count ? `The section and its ${count} panel${count === 1 ? "" : "s"} leave the draft.` : "The section leaves the draft.") + placedText + " Nothing is deleted until you save." })),
      buttons: [{ label: "Cancel" }, { label: "Delete", danger: true, onClick: () => this.afterDelete(result) }],
    });
  }

  // The change of a delete, and the notice for each annotation that had only the deleted panel (10.3b).
  afterDelete(result) {
    this.change(result.doc);
    for (const name of result.off || []) notify({ level: "info", text: `The annotation ${name} had only this panel. It is off.` });
  }

  confirmDelete(panel) {
    const result = deletePanel(this.doc, panel.id);
    if (result.refused) {
      notify({ level: "warning", text: `The panels ${result.refused.join(", ")} reuse this panel. Change them first.` });
      return;
    }
    openDialog({
      title: `Delete the panel ${panel.title || panel.id}?`,
      testid: "delete-panel-dialog",
      build: (b) => {
        b.append(el("p", { text: "The panel leaves the draft. Nothing is deleted until you save." }));
        // The annotations that name the panel lose its id in the same draft change (10.3b, 10.6).
        const named = annotationsNaming(this.doc, [panel.id]);
        if (named.length) b.append(el("p", { text: "These annotations lose this panel:" }), el("ul", { dataset: { testid: "delete-panel-annotations" } }, ...named.map((a) => el("li", { text: a.only ? `${a.name} (it goes off)` : a.name }))));
      },
      buttons: [{ label: "Cancel" }, { label: "Delete", danger: true, onClick: () => this.afterDelete(result) }],
    });
  }

  // "Rename" and "Add panel here" in the header of a section (10.2).
  sectionExtras(index, section) {
    const repeatVariable = section.repeat ? this.doc.variables.find((v) => v.name === section.repeat.variable) : null;
    const badgeNode = section.repeat ? badge(`Repeats by ${repeatVariable ? repeatVariable.label || repeatVariable.name : section.repeat.variable}`) : null;
    if (badgeNode) badgeNode.dataset.testid = `section-${index}-repeat-badge`;
    const rename = button("Rename", { class: "btn btn--ghost btn--sm", dataset: { testid: `section-${index}-rename` } }, "edit");
    const addHere = button("Add panel here", { class: "btn btn--ghost btn--sm", dataset: { testid: `section-${index}-add-panel` }, on: { click: () => this.addPanelTo(index) } }, "plus");
    const more = button("Section actions", { dataset: { testid: `section-${index}-menu` } }, "more", { iconOnly: true, variant: "ghost", size: "sm" });
    more.setAttribute("aria-haspopup", "menu");
    more.addEventListener("click", () => openMenu(more, [
      { label: "Add text panel here", onSelect: () => this.addTextPanelTo(index), testid: `section-${index}-add-text` },
      { separator: true },
      { label: "Move up", disabled: !canMoveSection(this.doc, index, -1), onSelect: () => this.moveSectionBy(index, -1), testid: `section-${index}-up` },
      { label: "Move down", disabled: !canMoveSection(this.doc, index, 1), onSelect: () => this.moveSectionBy(index, 1), testid: `section-${index}-down` },
      { separator: true },
      { label: "Repeat by a variable", disabled: Boolean(sectionRepeatRefusal(this.doc, index)), onSelect: () => this.repeatDialog(index), testid: `section-${index}-repeat` },
      { separator: true },
      { label: "Delete section", danger: true, onSelect: () => this.confirmDeleteSection(index, section), testid: `section-${index}-delete` },
    ], { align: "end" }));
    rename.addEventListener("click", () => {
      const head = rename.closest(".section__head");
      const toggle = head.querySelector(".section__toggle");
      const err = el("span", { class: "field-error", role: "alert", hidden: true });
      const input = el("input", { class: "input", type: "text", value: section.title, "aria-label": "Section title", dataset: { testid: `section-${index}-title-input` } });
      const hint = el("span", { class: "section__hint", text: "Enter saves, Escape cancels." });
      toggle.hidden = true;
      rename.hidden = true;
      addHere.hidden = true;
      more.hidden = true;
      head.prepend(input, hint, err);
      input.focus();
      input.select();
      const stop = () => {
        input.remove();
        hint.remove();
        err.remove();
        toggle.hidden = false;
        rename.hidden = false;
        addHere.hidden = false;
        more.hidden = false;
      };
      input.addEventListener("keydown", (e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          stop();
          toggle.focus();
        } else if (e.key === "Enter") {
          e.preventDefault();
          const result = renameSection(this.doc, index, input.value);
          if (result.error) {
            err.textContent = result.error;
            err.hidden = false;
            return;
          }
          this.change(result.doc);
        }
      });
    });
    // The tooltip of the item "Repeat by a variable" says why it is not active (10.6).
    more.addEventListener("click", () => {
      const reason = sectionRepeatRefusal(this.doc, index);
      const item = document.querySelector(`[data-testid="section-${index}-repeat"]`);
      if (reason && item) item.setAttribute("title", reason);
    });
    return [...(badgeNode ? [badgeNode] : []), rename, addHere, more];
  }

  // The dialog of the item "Repeat by a variable" (4.13a, 10.6): the variables that can repeat, and "None" to remove the repeat.
  repeatDialog(index) {
    const section = this.doc.sections[index];
    const vars = repeatableVariables(this.doc);
    const choices = [["", "None"], ...vars.map((v) => [v.name, v.label || v.name])];
    if (section.repeat && !vars.some((v) => v.name === section.repeat.variable)) choices.push([section.repeat.variable, `${section.repeat.variable} (not valid)`]);
    const select = el("select", { class: "input", "aria-label": "Variable", dataset: { testid: `section-${index}-repeat-variable` } });
    for (const [v, text] of choices) select.append(el("option", { value: v, text }));
    select.value = section.repeat ? section.repeat.variable : "";
    openDialog({
      title: "Repeat the section by a variable",
      testid: `section-${index}-repeat-dialog`,
      build: (body) => body.append(
        el("p", { text: "The page shows one copy of the section for each value of the variable." }),
        ...(vars.length ? [] : [el("p", { class: "muted", text: "No variable has several values (Multi-value). Turn it on for a variable first." })]),
        select),
      buttons: [{ label: "Cancel" }, { label: "Apply", primary: true, onClick: () => this.change(setSectionRepeat(this.doc, index, select.value || null)) }],
    });
  }
}

export { icon };
