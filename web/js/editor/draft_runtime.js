// The runtime of the draft grid (10.2). It has the members of the runtime that the grid and the panel frames use
// (panels, expanded, setVisible, setExpanded, retry, on). A panel of the draft shows:
//  - the result of the base runtime, when its query is the query of the base document (the route of 6.20);
//  - else the last result of "Run" (6.22), or the text "Run the query to see the data.".
// The runtime also gives the controls of the grid the variables of the draft: a variable that the page runtime has is the page variable (one selection); a variable that only the draft
// has (new or renamed) has a local selection, so that its control works in the draft grid.
// It has no DOM access.

import { failedSlots, panelState, visibleQueries } from "../queries.js";
import { buildPlan, canRepeatBy, orderedValues } from "../repeat.js";
import { gridDoc } from "../var_layout.js";
import { optionsOf, resolveSelection } from "../var_options.js";

const sig = (panel) => (panel.data && panel.data.query ? JSON.stringify([panel.data.query, panel.data.extra_queries]) : null);

export class DraftRuntime {
  /**
   * @param {object} o
   * @param {object} o.base the runtime of the base document
   * @param {object} o.doc the draft document
   * @param {(index: number, collapsed: boolean) => void} o.onCollapse the collapse control changes the draft
   */
  constructor({ base, doc, onCollapse }) {
    this.base = base;
    this.onCollapse = onCollapse;
    this.listeners = new Set();
    this.previews = new Map(); // id -> { sig, frame, error } for one query, { sig, slots } for a panel with extra queries
    this.local = { selections: new Map(), options: new Map(), optionErrors: new Map(), defs: new Map() };
    this.panels = new Map(); // the instances of the draft: the key of a panel or of a copy of a repeat (9.8c)
    this.expanded = new Map();
    this.plan = null;
    this.variables = base.variables;
    this.range = base.range;
    this.unsubscribe = base.on((type, data) => {
      // A change of the selection or of the options changes the copies of the draft too (9.8c).
      if ((type === "instances" || type === "vars") && this.hasRepeat) {
        this.replan();
        return;
      }
      if (type !== "panel") return;
      const rec = this.panels.get(data.key);
      if (rec && rec.mirrored) this.sync(data.key);
      // A reuse panel follows its source.
      for (const other of this.panels.values()) if (other.panel.data.reuse && other.panel.data.reuse.panel === data.id) this.emit("panel", other);
    });
    this.setDoc(doc);
  }

  // The text of a title with the variables replaced: the same helper as the view runtime (runtime.js), on the same variables.
  // `key` names a copy of a repeat (9.8c).
  interpolate(text, key) {
    return this.interpolateWith(text, key === undefined ? null : this.panels.get(key) || null);
  }

  interpolateWith(text, override) {
    return this.base.interpolateWith(text, override);
  }

  // The text of a text panel with the values of the variables (9.19b): the same helper as the view runtime.
  interpolateContent(text) {
    return this.base.interpolateContent(text);
  }

  // The values of a repeat variable of the draft (9.8c): the options in their order, null while they are not known, undefined when the
  // draft has no such variable that can repeat. The selection is the one of the controls of the page (9.10).
  valuesOf(name) {
    const variable = this.varDef(name);
    if (!variable || !canRepeatBy(variable)) return undefined;
    const known = this.base.varByName.has(name);
    const selection = known ? this.base.selections.get(name) : this.localState(name).selections.get(name);
    if (variable.kind === "query") {
      const loaded = known ? this.base.options.get(name) : undefined;
      return loaded === undefined ? null : orderedValues(optionsOf(variable, loaded), selection);
    }
    return orderedValues(optionsOf(variable, null), selection);
  }

  on(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  emit(type, data) {
    for (const l of this.listeners) l(type, data);
  }

  // The record of one draft instance: a panel, or a copy of a panel (9.8c). The copies show the data of the base runtime when the query is the same
  // and the value is the same. A panel that repeats has no preview of "Run": the preview has the selection of the draft, not one value.
  record(inst) {
    // A text panel (9.19b) has no `data`: its record has an empty one, because the code of the runtimes reads `panel.data` of each record.
    const text = inst.panel.type === "text";
    const panel = text ? { ...inst.panel, data: {} } : inst.panel;
    const base = {
      id: panel.id,
      key: inst.key,
      panel,
      section: inst.sectionKey,
      sectionIndex: inst.sectionIndex,
      variable: inst.variable,
      value: inst.value,
      text: inst.text,
      loading: inst.loading,
      state: "idle",
      frame: null,
      error: null,
      mirrored: false,
      needsRun: false,
      note: inst.note,
      action: null,
    };
    if (text) return { ...base, state: "ready" };
    if (panel.data.reuse || inst.note || inst.loading) return base;
    const own = this.base.panels.get(inst.key);
    if (own && !own.provisional && own.variable === inst.variable && own.value === inst.value && sig(own.panel) === sig(panel)) {
      return { ...base, mirrored: true, state: own.state, frame: own.frame, error: own.error, slots: own.slots };
    }
    // A panel whose queries are all hidden shows "No data" (4.15a, rule 3, 9.8d).
    if (!visibleQueries(panel).length) return { ...base, state: "empty", slots: [] };
    const preview = this.previews.get(panel.id);
    if ((inst.plain || (!inst.variable && !panel.repeat)) && preview && preview.sig === sig(panel)) {
      if (preview.slots) {
        // The results of "Run" for each query: the state of the panel follows the slots, as in the runtime (9.8d).
        const state = panelState(preview.slots);
        const first = preview.slots.find((s) => s.frame);
        const failed = failedSlots(preview.slots)[0];
        return { ...base, state, slots: preview.slots, frame: first ? first.frame : null, error: state === "error" && failed ? failed.error : null };
      }
      return preview.error ? { ...base, state: "error", error: preview.error } : { ...base, state: "ready", frame: preview.frame };
    }
    return { ...base, needsRun: true };
  }

  // The record of a panel as one panel, with no copy and no note: the preview of the panel editor shows it (9.8c, 10.3). A panel that does not
  // repeat has its own record.
  single(id) {
    const rec = this.panels.get(id);
    if (!rec || (!rec.variable && !rec.note && !rec.loading)) return rec || null;
    const inst = this.plan.instances.get(id);
    return this.record({ ...inst, plain: true, variable: null, value: null, text: null, note: null, loading: false });
  }

  // The plan of the draft (repeat.js) and the records of its instances. A record keeps its note and its action.
  makePlan(doc) {
    this.doc = doc;
    this.hasRepeat = doc.sections.some((s) => s.repeat || s.panels.some((p) => p.repeat));
    const labelOf = (name) => (this.varDef(name) && this.varDef(name).label) || name;
    this.plan = buildPlan(gridDoc(doc), (n) => this.valuesOf(n), labelOf, { repeat: this.hasRepeat, limits: this.base.limits });
    const before = this.panels;
    this.panels = new Map();
    for (const inst of this.plan.instances.values()) {
      if (!inst.panel.data && inst.panel.type !== "text") continue;
      const rec = this.record(inst);
      const old = before.get(inst.key);
      if (old && old.panel === inst.panel) {
        rec.action = old.action;
        if (!inst.note) rec.note = old.note;
      }
      this.panels.set(inst.key, rec);
    }
    const expanded = new Map();
    this.plan.sections.forEach((sp) => {
      for (const copy of sp.copies) {
        const first = copy.n <= 1;
        const kept = this.expanded.get(copy.key);
        expanded.set(copy.key, first || kept === undefined ? !doc.sections[sp.index].collapsed : kept);
      }
    });
    this.expanded = expanded;
  }

  // A new draft document: the records and the expanded sections are made again.
  setDoc(doc) {
    this.panels = new Map();
    this.makePlan(doc);
    this.emit("doc", doc);
  }

  // The selection or the options changed: the copies of the draft are made again, and the grid draws them (9.8c).
  replan() {
    this.makePlan(this.doc);
    this.emit("instances", this.plan);
  }

  sync(key) {
    const rec = this.panels.get(key);
    if (!rec) return;
    const inst = this.plan && this.plan.instances.get(key);
    const next = inst ? this.record(inst) : rec;
    next.note = rec.note;
    next.action = rec.action;
    this.panels.set(key, next);
    this.emit("panel", next);
    for (const other of this.panels.values()) if (other.panel.data.reuse && other.panel.data.reuse.panel === next.id) this.emit("panel", other);
  }

  // The result of "Run" for a panel (10.3). `result` is { frame }, { error } or { slots } (one slot for each visible query).
  setPreview(id, result) {
    const rec = this.panels.get(id);
    if (!rec || !rec.panel.data.query) return;
    this.previews.set(id, { sig: sig(rec.panel), frame: result.frame || null, error: result.error || null, slots: result.slots || null });
    this.sync(id);
  }

  getPreview(id) {
    return this.previews.get(id) || null;
  }

  setVisible(key, visible) {
    const rec = this.panels.get(key);
    if (rec && rec.mirrored) this.base.setVisible(key, visible);
  }

  // A collapse of the first copy of a section changes the draft. A copy n from 2 is a view: its state is not stored (9.8c).
  setExpanded(key, expanded) {
    this.expanded.set(key, expanded);
    if (typeof key === "number" && this.onCollapse) this.onCollapse(key, !expanded);
  }

  retry(key) {
    const rec = this.panels.get(key);
    if (rec && rec.mirrored) this.base.retry(key);
  }

  requestVariables() {
    return this.base.requestVariables();
  }

  // --- the controls of the variables in the draft grid (9.10) -----------------------------------------------------------

  /** The variable of the draft that has this name, or undefined. */
  varDef(name) {
    return this.doc.variables.find((v) => v.name === name);
  }

  /** The state (the maps `selections`, `options`, `optionErrors`) for a variable that only the draft has. It makes the selection when needed. */
  localState(name) {
    const v = this.varDef(name);
    const l = this.local;
    if (v && l.defs.get(name) !== JSON.stringify(v)) {
      l.defs.set(name, JSON.stringify(v));
      const current = l.selections.get(name);
      // An adhoc variable of the draft starts with no filter and keeps its filters (9.10a).
      if (v.kind === "adhoc") l.selections.set(name, { all: false, values: current ? current.values : [] });
      else l.selections.set(name, resolveSelection(v, optionsOf(v, null), undefined, current ? (current.all ? ["$__all"] : current.values) : undefined));
    }
    return l;
  }

  /** A change of a control of the draft grid: the page variable, or the local selection of a variable that only the draft has. */
  setSelection(name, selection) {
    if (this.base.varByName.has(name)) return this.base.setSelection(name, selection);
    this.local.selections.set(name, selection);
    this.emit("vars");
    return undefined;
  }

  destroy() {
    this.unsubscribe();
    this.listeners.clear();
  }
}
