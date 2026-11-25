// The runtime of the dashboard page (9.8, 9.10): time range, variable graph, query scheduler and results.
// It has no DOM access. The page gives it the functions that it needs (api, widthOf) and listens to its events.

import { applies, readAnswer } from "./annotations.js";
import { adhocRequest, adhocSelection, readAdhocKeys } from "./adhoc.js";
import * as busy from "./busy.js";
import { MD_MAX_EXPANDED_BYTES } from "./markdown.js";
import { hasTimeOverride, panelRange } from "./panel_time.js";
import { DEFAULT_LIMITS, buildPlan, canRepeatBy, copyVariables, orderedValues, splitKey } from "./repeat.js";
import { failedSlots, isWholeError, panelState, queriesOf, requestBody } from "./queries.js";
import { effectiveZone, resolveRange } from "./time.js";
import { OptionsCache, cacheKey, periodKey, ttlOf } from "./options_cache.js";
import { gridDoc, labelOf } from "./var_layout.js";
import { ALL, buildOptions, defaultSelection, expandText, interpolateText, optionsOf, placeholderInfo, replacement, requestValue, resolveSelection } from "./var_options.js";

export const MAX_IN_FLIGHT = 6;

// The jobs of the annotations go after all the panel jobs in the queue (9.8): the rank of a panel is below this number.
const ANNOTATION_RANK = 1e6;

const TOO_LARGE = "The selection of the variables is too large. Select fewer values.";

const CONTENT_PLACEHOLDER = /\$\{([A-Za-z0-9_]+)(?::([A-Za-z0-9_]+))?\}|\$([A-Za-z0-9_]+)/g;

// The member `names` of the answer of 6.14 (9.10): the variable names of each saved query. The server makes it, and the page never scans a
// query text. Gives { panels: Map(panel id as text -> lists), variables: Map(name -> list), annotations: lists }, where each list is an array of
// distinct variable names of the document. Throws an Error that says what is wrong when the member does not follow 6.14.
export function readNames(doc, member) {
  const known = new Set(doc.variables.map((v) => v.name));
  const shape = (ok, text) => {
    if (!ok) throw new Error(text);
  };
  const lists = (x, count, where) => {
    shape(Array.isArray(x) && x.length === count, `${where} must be an array of ${count} lists`);
    for (const l of x) shape(Array.isArray(l) && new Set(l).size === l.length && l.every((n) => known.has(n)), `${where} must hold lists of distinct variable names`);
    return x;
  };
  const closed = (x, keys, where) => {
    shape(x !== null && typeof x === "object" && !Array.isArray(x), `${where} must be an object`);
    shape(JSON.stringify(Object.keys(x).sort()) === JSON.stringify([...keys].sort()), `${where} must have the keys ${keys.join(", ") || "(none)"}`);
    return x;
  };
  shape(member !== undefined, "the answer has no member names");
  closed(member, ["panels", "variables", "annotations"], "names");
  const queries = doc.sections.flatMap((s) => s.panels).filter((p) => p.data && p.data.query);
  const panels = closed(member.panels, queries.map((p) => String(p.id)), "names.panels");
  const queryVars = doc.variables.filter((v) => v.kind === "query").map((v) => v.name);
  const variables = closed(member.variables, queryVars, "names.variables");
  return {
    panels: new Map(queries.map((p) => [String(p.id), lists(panels[p.id], 1 + (p.data.extra_queries || []).length, `names.panels.${p.id}`)])),
    variables: new Map(queryVars.map((n) => [n, lists([variables[n]], 1, `names.variables.${n}`)[0]])),
    annotations: lists(member.annotations, (doc.annotations || []).length, "names.annotations"),
  };
}

const sameSelection = (a, b) => a.all === b.all && a.values.length === b.values.length && a.values.every((v, i) => v === b.values[i]);

export class Runtime {
  /**
   * @param {object} o
   * @param {string} o.uid
   * @param {object} o.doc the document (section 4)
   * @param {string} o.etag
   * @param {object} o.api { panelQuery(uid, id, body, signal), variableOptions(uid, name, body, signal) }
   * @param {(key: number|string) => number} o.widthOf the inner width of the panel frame (the key of the instance, 9.8c), in CSS pixels
   * @param {{fromText: string, toText: string}} o.range
   * @param {Map<string, string[]>} o.urlVars
   * @param {Map<string, {all: boolean, values: string[]}>} [o.keepSelections] selections kept after a reload (409)
   * @param {{panel: number, section: number, dashboard: number}} [o.limits] the limits of a repeat (repeat.js, limitsOf)
   * @param {(notice: {name: string, text: string}) => void} [o.onNotice] called when a variable drops a requested value that is not an option (9.10)
   * @param {string} [o.zone] a known zone of the URL (9.5), in place of the zone of the document
   * @param {object} [o.names] the member `names` of the answer of 6.14. When it is absent or wrong, `namesError` has the reason and each query names
   *   every variable (9.10): the page stays correct and runs more queries.
   */
  constructor({ uid, doc, etag, api, widthOf, range, names, adhoc, urlVars = new Map(), keepSelections = null, limits = DEFAULT_LIMITS, now = () => Date.now(), onNotice = null, zone = null }) {
    this.uid = uid;
    this.onNotice = onNotice;
    this.limits = limits;
    this.doc = doc;
    this.etag = etag;
    this.api = api;
    this.widthOf = widthOf;
    this.now = now;
    // A zone name that the browser does not know gives the zone `browser` (9.9, D15). The page shows the notice. A zone of the URL wins (9.5).
    this.zone = zone || effectiveZone(doc.time.timezone);
    this.listeners = new Set();
    this.destroyed = false;
    this.stale = false;
    this.inFlight = 0;
    this.optionsLoading = 0;
    this.viewPanel = null;
    this.queue = [];
    this.varChain = Promise.resolve();
    this.optionController = new Map();
    this.variables = doc.variables;
    this.varByName = new Map(doc.variables.map((v) => [v.name, v]));
    this.namesError = null; // the reason when the member names is not usable: each query then names every variable (9.10)
    this.adhocKeys = readAdhocKeys(doc, adhoc); // the permitted keys (6.14, 9.10a)
    this.read = null;
    try {
      this.read = readNames(doc, names);
    } catch (error) {
      this.namesError = error.message;
    }
    // One record for each annotation (4.5b, 9.19d). The state is `idle`, `queued`, `loading`, `ready` or `error`. A record has the shape of a panel
    // record where the scheduler reads it (`controller`, `dirty`, `busyHeld`, `rank`), so the queue and the abort code work for both.
    this.annotations = (doc.annotations || []).map((a, index) => ({
      annotation: true, index, a, state: "idle", items: [], error: null, controller: null, dirty: true, busyHeld: false, rank: ANNOTATION_RANK + index,
      names: this.read ? new Set(this.read.annotations[index]) : new Set(this.varByName.keys()),
    }));
    this.options = new Map();
    this.optionErrors = new Map();
    // The state of the options of each query variable, for its control (9.10): `loading` (queued or running), `waiting` (a variable above it failed or
    // waits, so it sends no request), `ready` or `error`. The cache keeps the answers of the options for the period and the values above them.
    this.optionState = new Map();
    this.optionsCache = new OptionsCache({ now });
    this.selections = new Map();
    this.range = { fromText: range.fromText, toText: range.toText, from_ms: 0, to_ms: 1, error: null };
    this.recomputeRange();
    this.graph = this.buildGraph();
    for (const v of this.variables) if (v.kind === "query") this.optionState.set(v.name, { state: this.graph.parents.get(v.name).some((n) => this.varByName.get(n).kind === "query") ? "waiting" : "loading", parent: null });
    this.panels = new Map(); // the instances: the key is the id of a panel and of its first copy, else `<id>-r<n>` (9.8c)
    this.expanded = new Map(); // the sections and the copies of a section, by key
    this.order = [];
    this.plan = null;
    this.repeats = doc.sections.some((s) => s.repeat || s.panels.some((p) => p.repeat));
    this.gridDocument = gridDoc(doc);
    this.stored = new Map(doc.sections.flatMap((s) => s.panels.map((panel) => [panel.id, panel])));
    this.rangeNow = this.now();
    // The first selections: custom and text variables resolve now; a query variable resolves after its options.
    for (const v of this.variables) {
      const url = urlVars.get(v.name);
      const keep = keepSelections ? keepSelections.get(v.name) : undefined;
      const current = keep ? (keep.all ? [ALL] : keep.values) : undefined;
      if (v.kind === "adhoc") {
        const r = adhocSelection(v, this.adhocKeys.get(v.name), url || current);
        this.selections.set(v.name, r.selection);
        if (r.notice && onNotice) onNotice({ name: v.name, text: r.notice });
      } else if (v.kind === "query") {
        const candidates = url && url.length ? url : current && current.length ? current : v.default;
        this.selections.set(v.name, { all: candidates.includes(ALL), values: candidates.filter((x) => x !== ALL) });
      } else {
        const options = optionsOf(v, null);
        const after = resolveSelection(v, options, url, current);
        this.selections.set(v.name, after);
        this.notify(v, options, url, current, after);
      }
    }
    this.rebuild();
  }

  // The selection that a variable gets when the URL has no value for it (9.5), or null when it is not known: a constant has none in the URL, and a
  // query variable has none until its options load. The address leaves out a variable whose selection equals this one.
  defaultSelection(name) {
    const variable = this.varByName.get(name);
    if (!variable || variable.kind === "constant") return null;
    if (variable.kind === "adhoc") return adhocSelection(variable, this.adhocKeys.get(name), undefined).selection;
    if (variable.kind === "query") {
      const loaded = this.options.get(name);
      return loaded ? defaultSelection(variable, optionsOf(variable, loaded)) : null;
    }
    return defaultSelection(variable, optionsOf(variable, null));
  }

  // Tells the page that a variable dropped a requested value (9.10). The selection is set before the call.
  notify(variable, options, url, current, after) {
    const notice = this.onNotice && replacement(variable, options, url, current, after);
    if (notice) this.onNotice(notice);
  }

  // The info of a placeholder (placeholderInfo of var_options.js), or null when the document has no such variable. `override` is the repeat value of a copy (9.8c).
  infoOf(name, override) {
    if (override && override.variable === name && override.value !== null) return { all: false, allValue: null, values: [override.value], texts: [override.text] };
    const variable = this.varByName.get(name);
    const selection = variable ? this.selections.get(name) : undefined;
    return variable && selection ? placeholderInfo(variable, selection, this.options.get(name)) : null;
  }

  /** The text with the placeholders replaced, and the repeat variable fixed to the value of a copy (9.8c). `override` is { variable, value, text } or null. */
  interpolateWith(text, override) {
    return interpolateText(text, (name) => this.infoOf(name, override));
  }

  /** The text of a text panel with the placeholders replaced (9.19b): `$name` gives the values of the selection, joined with ", " (a title gives the option texts). */
  interpolateContent(text) {
    if (typeof text !== "string" || !text.includes("$")) return text;
    // The budget is MD_MAX_EXPANDED_BYTES: the result has at least one byte for each character, so a longer result is over the cap.
    // It gives null when the budget ends (the text panel shows `text-limit`).
    let used = text.length;
    const out = text.replace(CONTENT_PLACEHOLDER, (whole, braced, format, bare) => {
      if (used > MD_MAX_EXPANDED_BYTES) return "";
      const name = braced || bare;
      const info = name.startsWith("__") ? null : this.infoOf(name, null);
      if (!info) return whole;
      const piece = format !== undefined ? expandText(whole, () => info, MD_MAX_EXPANDED_BYTES) : info.all && info.allValue !== null ? String(info.allValue) : info.values.join(", "); // text, csv, pipe and glob (9.13)
      if (piece === null) used = Infinity;
      else used += piece.length - whole.length;
      return piece === null ? "" : piece;
    });
    return used > MD_MAX_EXPANDED_BYTES ? null : out;
  }

  /** The text with its variable placeholders replaced by the current selections (9.13). Never HTML. `key` names the copy of a repeat (9.8c). */
  interpolate(text, key) {
    return this.interpolateWith(text, key === undefined ? null : this.panels.get(key) || null);
  }

  // --- events ------------------------------------------------------------------------------------------

  on(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  emit(type, data) {
    for (const l of this.listeners) l(type, data);
  }

  // --- structure ---------------------------------------------------------------------------------------

  // The values of a repeat variable (9.8c): the options in their order, null while the options of a query variable load, undefined when
  // the document has no such variable that can repeat.
  valuesOf(name) {
    const variable = this.varByName.get(name);
    if (!variable || !canRepeatBy(variable)) return undefined;
    if (variable.kind === "query" && !this.options.has(name)) return null;
    return orderedValues(this.currentOptions(variable), this.selections.get(name));
  }

  // The instances of the page (9.8c): the plan gives the copies, and each instance has one record with its key. A record that has the same
  // key and the same value stays with its result. A record that leaves is aborted. It emits `instances` when the plan is not the first one.
  rebuild() {
    const labelOf = (name) => this.varByName.get(name)?.label || name;
    const plan = buildPlan(this.gridDocument, (n) => this.valuesOf(n), labelOf, { repeat: this.repeats, limits: this.limits });
    const next = new Map();
    for (const inst of plan.instances.values()) {
      // A placed control has no query and no record. A text panel has no query and a record in the state `ready` (9.19b).
      if (!inst.panel.data && inst.panel.type !== "text") continue;
      const sig = `${inst.variable ?? ""}\u0000${inst.value ?? ""}`;
      let rec = this.panels.get(inst.key);
      if (!rec || rec.sig !== sig || rec.provisional) rec = this.makeRecord(inst, sig);
      rec.inst = inst;
      rec.section = inst.sectionKey;
      rec.note = inst.note;
      rec.loading = inst.loading;
      next.set(inst.key, rec);
    }
    // A copy that the view of one panel asks for stays while its repeat loads (9.8c, rule 16 of 10.2a).
    for (const [key, rec] of this.panels) {
      if (!next.has(key) && rec.provisional && this.repeatLoading(rec.id, plan)) next.set(key, rec);
    }
    for (const [key, rec] of this.panels) if (next.get(key) !== rec) this.abortPanel(rec);
    this.panels = next;
    this.plan = plan;
    plan.sections.forEach((section) => {
      section.copies.forEach((copy) => {
        if (!this.expanded.has(copy.key)) this.expanded.set(copy.key, !this.doc.sections[section.index].collapsed);
      });
    });
    const keys = new Set(plan.sections.flatMap((s) => s.copies.map((c) => c.key)));
    for (const key of [...this.expanded.keys()]) if (!keys.has(key)) this.expanded.delete(key);
    this.order = plan.order.map((inst) => next.get(inst.key));
    this.order.forEach((p, i) => {
      p.rank = i;
      // The slots of a panel go in the queue after the panel before it and before the next panel, in the order of the queries (9.8d).
      for (const slot of p.slots) slot.rank = i + slot.index / 10;
    });
    this.recomputePanelRanges(this.rangeNow);
    this.emit("instances", plan);
  }

  makeRecord(inst, sig) {
    const text = inst.panel.type === "text";
    const query = text ? null : inst.panel.data.query;
    const skip = inst.variable !== null && inst.value !== null ? inst.variable : null;
    const rec = {
      id: inst.id,
      key: inst.key,
      // The code of the runtime reads `panel.data` of each record: a text panel, which has no `data`, gets an empty one in its record.
      panel: text ? { ...inst.panel, data: {} } : inst.panel,
      section: inst.sectionKey,
      sectionIndex: inst.sectionIndex,
      inst,
      sig,
      variable: inst.variable,
      value: inst.value,
      text: inst.text,
      note: inst.note,
      loading: inst.loading,
      provisional: false,
      state: text ? "ready" : "idle",
      frame: null,
      error: null,
      slots: [],
      visible: false,
      names: query ? this.panelNames(inst.id, skip) : new Set(),
    };
    // One slot for each visible query (9.8d). A slot has the shape of an annotation record where the scheduler reads it (`controller`, `dirty`,
    // `busyHeld`, `rank`). A panel whose queries are all hidden has no slot and the state `empty`.
    if (query) {
      rec.slots = queriesOf(rec.panel).map((q, index) => ({
        slot: true, rec, ref: q.ref, query: q, index, state: "idle", frame: null, error: null, controller: null, dirty: true, busyHeld: false, rank: 0,
        names: this.slotNames(inst.id, index, skip),
      })).filter((slot) => !slot.query.hide);
      if (!rec.slots.length) rec.state = "empty";
    }
    return rec;
  }

  // The variables that one query of a panel names (9.10), without the variable of a repeat copy. Without a usable member, all variables.
  slotNames(id, index, skip) {
    const names = this.read ? this.read.panels.get(String(id))[index] : [...this.varByName.keys()];
    return new Set(names.filter((n) => n !== skip));
  }

  // The variables that the queries of a panel name, without the variable of its repeat copy (9.10). Without a usable member, all variables.
  panelNames(id, skip) {
    const lists = this.read ? this.read.panels.get(String(id)).flat() : [...this.varByName.keys()];
    return new Set(lists.filter((n) => n !== skip));
  }

  // True while the values of the repeat of a panel are not known (the options of a query variable load).
  repeatLoading(id, plan = this.plan) {
    const panel = this.stored.get(id);
    if (!panel || !plan) return false;
    const section = plan.sections.find((s) => this.doc.sections[s.index].panels.includes(panel));
    const entry = (section && section.entry) || plan.entries.panels.get(id);
    return !!entry && entry.state === "loading";
  }

  // The record of a copy for the view of one panel (9.8c, rule 16): while the options load, a copy `<id>-r<n>` has a record in the state `idle`.
  ensureView(key) {
    const known = this.panels.get(key);
    if (known) return known;
    const { id, n } = splitKey(key);
    const first = this.panels.get(id);
    if (n < 2 || !first || !this.repeatLoading(id)) return null;
    // The form of Grafana: the frame of the panel, which waits too (rule 16 of 10.2a).
    if (n === undefined) return first;
    const rec = this.makeRecord({ ...first.inst, key, n, value: null, text: null, variable: null }, "view");
    rec.provisional = true;
    rec.loading = true;
    this.panels.set(key, rec);
    return rec;
  }

  // The graph of the query variables: parents are the variables that the SQL names (9.10).
  buildGraph() {
    const parents = new Map();
    for (const v of this.variables) {
      const named = this.read ? this.read.variables.get(v.name) : [...this.varByName.keys()];
      parents.set(v.name, v.kind === "query" ? named.filter((n) => n !== v.name) : []);
    }
    // Graph order: topological, document order between variables with no link.
    const order = [];
    const done = new Set();
    const visit = (name, trail) => {
      if (done.has(name) || trail.has(name)) return;
      trail.add(name);
      for (const p of parents.get(name)) visit(p, trail);
      trail.delete(name);
      done.add(name);
      order.push(name);
    };
    for (const v of this.variables) visit(v.name, new Set());
    return { parents, order };
  }

  // The query variables below `names` in the graph, in graph order.
  descendantsOf(names) {
    const hit = new Set(names);
    const out = [];
    for (const name of this.graph.order) {
      const v = this.varByName.get(name);
      if (v.kind !== "query") continue;
      if (this.graph.parents.get(name).some((p) => hit.has(p))) {
        hit.add(name);
        out.push(name);
      }
    }
    return out;
  }

  // --- range ---------------------------------------------------------------------------------------------

  recomputeRange() {
    const t = this.now();
    const r = resolveRange(this.range.fromText, this.range.toText, t, this.zone);
    if (r.error) {
      this.range.error = r.error;
      return false;
    }
    this.range.from_ms = r.from_ms;
    this.range.to_ms = r.to_ms;
    this.range.error = null;
    this.recomputePanelRanges(t);
    return true;
  }

  // The range of each panel (9.8b): the range of the dashboard, or the range of its time override. `t` is the time `now` of the
  // dashboard range, so that the panels and the dashboard agree. A panel with `error` in its range sends no query (start1).
  recomputePanelRanges(t) {
    if (!this.panels) return; // the constructor reads the range before it builds the panels
    this.rangeNow = t;
    const dashboard = { from: this.range.from_ms, to: this.range.to_ms, fromText: this.range.fromText, toText: this.range.toText };
    for (const p of this.panels.values()) {
      const query = p.panel.data.query;
      if (query && hasTimeOverride(query)) {
        const r = panelRange(query, dashboard, t, this.zone);
        p.range = { from_ms: r.from, to_ms: r.to, error: r.error };
      } else {
        p.range = { from_ms: this.range.from_ms, to_ms: this.range.to_ms, error: null };
      }
    }
  }

  // The range that a panel shows and queries with. A reuse panel has the range of its source (9.8b, step 9). `id` is the key of the instance.
  rangeOf(id) {
    const p = this.panels.get(id);
    if (!p) return null;
    const reuse = p.panel.data.reuse;
    const source = reuse ? this.panels.get(reuse.panel) : null;
    return (source || p).range;
  }

  // --- variables -------------------------------------------------------------------------------------------

  currentOptions(variable) {
    return optionsOf(variable, this.options.get(variable.name) || []);
  }

  // The member `variables` of a query body (6.20, 7.7): each variable of the document.
  requestVariables() {
    const out = {};
    for (const v of this.variables) out[v.name] = v.kind === "adhoc" ? adhocRequest(this.selections.get(v.name)) : requestValue(v, this.currentOptions(v), this.selections.get(v.name));
    return out;
  }

  // The values that the request of the options of `variable` sends for the variables its SQL names (the key of the cache, 9.10).
  parentValues(variable) {
    const all = this.requestVariables();
    return this.graph.parents.get(variable.name).map((n) => [n, all[n]]);
  }

  // The options of a variable: from the cache when the key is known and young, else from the server (9.10). `force` skips the cache (Retry).
  async loadOptions(variable, signal, force = false) {
    const period = periodKey(variable, this.range, this.zone);
    const key = cacheKey(variable, this.parentValues(variable), period);
    if (!force) {
      const hit = this.optionsCache.get(key);
      if (hit) return hit;
    }
    const body = { etag: this.etag, from_ms: this.range.from_ms, to_ms: this.range.to_ms, variables: this.requestVariables() };
    const answer = await this.api.variableOptions(this.uid, variable.name, body, signal);
    const options = buildOptions(answer.options || [], variable);
    this.optionsCache.set(key, options, ttlOf(period));
    return options;
  }

  // The state of the options of a variable for its control: { state, parent }. A variable that is not a query variable is always `ready`.
  optionStateOf(name) {
    return this.optionState.get(name) || { state: "ready", parent: null };
  }

  setOptionState(name, state, parent = null) {
    this.optionState.set(name, { state, parent });
  }

  // Loads the options of the query variables `names`, in order, and resolves their selections.
  // It returns the names whose selection changed. A variable whose options fail keeps its selection, and the panels run with it (9.10).
  async reloadOptions(names, { keepUrl = null, force = false } = {}) {
    const changed = [];
    const handled = new Set(names);
    // The variables of this reload whose options failed or that wait: a variable below one of them waits too and sends no request (9.10).
    const unavailable = new Set();
    this.optionsLoading += 1;
    busy.begin();
    try {
      // At once: a variable that has a query variable above it in this reload waits, the others are in the queue.
      for (const name of names) {
        const above = this.graph.parents.get(name).find((n) => handled.has(n) && this.varByName.get(n).kind === "query");
        this.setOptionState(name, above ? "waiting" : "loading", above ? labelOf(this.varByName.get(above)) : null);
      }
      this.emit("vars");
      for (const name of names) {
        if (this.destroyed) break;
        const variable = this.varByName.get(name);
        const blocked = this.graph.parents.get(name).find((n) => unavailable.has(n));
        if (blocked) {
          unavailable.add(name);
          this.optionErrors.delete(name);
          this.setOptionState(name, "waiting", labelOf(this.varByName.get(blocked)));
          this.emit("vars");
          continue;
        }
        this.setOptionState(name, "loading");
        this.emit("vars");
        const controller = new AbortController();
        this.optionController.get(name)?.abort();
        this.optionController.set(name, controller);
        let options;
        let failed = false;
        try {
          options = await this.loadOptions(variable, controller.signal, force);
          this.optionErrors.delete(name);
          this.setOptionState(name, "ready");
        } catch (error) {
          if (error.aborted || this.destroyed) {
            this.releaseOptionController(name, controller);
            return changed;
          }
          if (error.code === "dashboard_changed") {
            this.releaseOptionController(name, controller);
            this.handleStale(error);
            return changed;
          }
          unavailable.add(name);
          this.optionErrors.set(name, error);
          this.setOptionState(name, "error");
          // The options that the page knew stay: the person can still choose in them. The selection stays as it is (9.10).
          options = this.options.get(name) || [];
          failed = true;
        }
        this.releaseOptionController(name, controller);
        this.options.set(name, options);
        const before = this.selections.get(name);
        const current = before.all ? [ALL] : before.values;
        const after = failed ? before : resolveSelection(variable, options, keepUrl ? keepUrl.get(name) : undefined, current);
        if (!sameSelection(before, after)) changed.push(name);
        this.selections.set(name, after);
        if (!failed) this.notify(variable, options, keepUrl ? keepUrl.get(name) : undefined, current, after);
        if (this.repeats) this.rebuild();
        this.emit("vars");
      }
    } finally {
      this.optionsLoading -= 1;
      busy.end();
    }
    return changed;
  }

  // The Retry of the control of a variable (9.10): the options of the variable and of the variables below it load again, without the cache. The panels
  // that name a variable whose selection changed query again.
  retryOptions(name) {
    const variable = this.varByName.get(name);
    if (!variable || variable.kind !== "query" || this.stale) return undefined;
    const names = [name, ...this.descendantsOf([name])];
    for (const n of names) this.optionsCache.forget(n);
    this.varChain = this.varChain.then(async () => {
      if (this.destroyed || this.stale) return;
      const changed = new Set(await this.reloadOptions(names, { force: true }));
      this.markPanelsDirty(changed);
      this.schedule();
    });
    return this.varChain;
  }

  // The query variables that a change of time must reload: refresh = "time_change", and what is below them.
  timeChangeVariables() {
    const first = this.variables.filter((v) => v.kind === "query" && v.refresh === "time_change").map((v) => v.name);
    const all = new Set([...first, ...this.descendantsOf(first)]);
    return this.graph.order.filter((n) => all.has(n));
  }

  // --- start ---------------------------------------------------------------------------------------------------

  // 9.8 step 4: load the options in graph order, then plan the panel queries.
  async start(urlVars = new Map()) {
    const queryVars = this.graph.order.filter((n) => this.varByName.get(n).kind === "query");
    await this.reloadOptions(queryVars, { keepUrl: urlVars });
    if (this.destroyed || this.stale) return;
    this.markAllDirty();
    this.schedule();
  }

  // --- panel state ----------------------------------------------------------------------------------------------

  setState(p, state, extra = {}) {
    p.state = state;
    Object.assign(p, extra);
    this.emit("panel", p);
    // A reuse panel shows the result of its source.
    for (const other of this.panels.values()) if (other.panel.data.reuse && other.panel.data.reuse.panel === p.id) this.emit("panel", other);
  }

  // A slot changed: the panel state, the first frame and the first error follow the slots (9.8d, FE-41).
  setSlot(slot, state, extra = {}) {
    Object.assign(slot, { state }, extra);
    this.settle(slot.rec);
  }

  settle(p) {
    const state = panelState(p.slots);
    const first = p.slots.find((s) => s.frame);
    const failed = failedSlots(p.slots)[0];
    this.setState(p, state, { frame: first ? first.frame : null, error: state === "error" && failed ? failed.error : null });
  }

  // An error of the request that no query causes alone goes to all the slots of the panel (9.8d).
  wholeError(p, error) {
    for (const s of p.slots) {
      this.abortJob(s);
      Object.assign(s, { state: "error", error, frame: null, dirty: false });
    }
    this.settle(p);
  }

  // The slots of a panel that waited or loaded go back to `idle`, and a panel in the state `queued` or `loading` follows its slots.
  idle(p) {
    for (const s of p.slots) if (s.state === "queued" || s.state === "loading") s.state = "idle";
    if (p.state === "queued" || p.state === "loading") p.state = p.slots.length ? panelState(p.slots) : "idle";
  }

  // The slots of a panel that run again at the next schedule. `running` marks the slots that are in flight too.
  markDirty(p, running = true) {
    for (const s of p.slots) if (running || !s.controller) s.dirty = true;
  }

  holdBusy(p) {
    if (!p.busyHeld) {
      p.busyHeld = true;
      busy.begin();
    }
  }

  releaseBusy(p) {
    if (p.busyHeld) {
      p.busyHeld = false;
      busy.end();
    }
  }

  markAllDirty() {
    for (const p of this.panels.values()) this.markDirty(p);
    for (const r of this.annotations) r.dirty = true;
  }

  // Aborts the requests of a panel (one for each slot) or of an annotation, and takes them out of the queue.
  abortPanel(p) {
    for (const job of p.slots || [p]) this.abortJob(job);
  }

  // Aborts the request of one slot or annotation job.
  abortJob(job) {
    if (job.controller) {
      job.controller.abort();
      job.controller = null;
      this.inFlight = Math.max(0, this.inFlight - 1);
    }
    const at = this.queue.indexOf(job);
    if (at >= 0) this.queue.splice(at, 1);
    this.releaseBusy(job);
  }

  // A request that ends forgets its own controller only: a newer request of the same variable has put its own in the map (9.10).
  releaseOptionController(name, controller) {
    if (this.optionController.get(name) === controller) this.optionController.delete(name);
  }

  abortAll() {
    this.queue = [];
    for (const p of this.panels.values()) {
      this.abortPanel(p);
      this.idle(p);
    }
    for (const r of this.annotations) {
      this.abortPanel(r);
      if (r.state === "queued" || r.state === "loading") r.state = "idle";
    }
    for (const c of this.optionController.values()) c.abort();
  }

  // The panels that query now: see 9.8 "When a panel queries".
  wanted(p) {
    // A frame with a note (`repeat-empty`, `repeat-limit`) and a repeat that waits for its options send no query (9.8c).
    if (!p.panel.data.query || p.note || p.loading || p.provisional) return false;
    if (this.viewPanel !== null) {
      if (p.key === this.viewPanel) return true;
      const view = this.panels.get(this.viewPanel);
      return !!(view && view.panel.data.reuse && view.panel.data.reuse.panel === p.id);
    }
    if (this.expanded.get(p.section) && p.visible) return true;
    // The source of a reuse panel that is shown queries also out of view or in a collapsed section.
    for (const r of this.panels.values()) {
      if (r.panel.data.reuse && r.panel.data.reuse.panel === p.id && this.expanded.get(r.section) && r.visible) return true;
    }
    return false;
  }

  // An annotation job runs when it is enabled and a time series that it applies to is in view, as for a panel query, or is the `viewPanel` (9.8).
  annotationWanted(r) {
    if (r.a.enabled === false) return false;
    const fits = (p) => p.panel.type === "timeseries" && applies(r.a, p.id);
    if (this.viewPanel !== null) {
      const view = this.panels.get(this.viewPanel);
      return !!view && fits(view);
    }
    return [...this.panels.values()].some((p) => fits(p) && p.visible && this.expanded.get(p.section));
  }

  // The annotations that draw on the panel `id`: the records, in the order of the document (9.19d). A reuse panel and a copy use their own id.
  annotationsFor(id) {
    return this.annotations.filter((r) => applies(r.a, id));
  }

  setAnnotation(r, state, extra = {}) {
    r.state = state;
    Object.assign(r, extra);
    this.emit("annotations", r);
  }

  schedule() {
    if (this.destroyed || this.stale) return;
    for (const p of this.order) {
      if (!p.panel.data.query || !this.wanted(p)) continue;
      for (const slot of p.slots) {
        if (!slot.dirty || slot.controller || this.queue.includes(slot)) continue;
        this.queue.push(slot);
        this.holdBusy(slot);
        this.setSlot(slot, "queued");
      }
    }
    this.queue.sort((a, b) => a.rank - b.rank);
    this.pump();
    // The annotation jobs wait one microtask, so the panels that one batch of the observer shows start first (9.8).
    if (this.annotations.length) queueMicrotask(() => this.scheduleAnnotations());
  }

  scheduleAnnotations() {
    if (this.destroyed || this.stale) return;
    for (const r of this.annotations) {
      if (!r.dirty || r.controller || this.queue.includes(r) || !this.annotationWanted(r)) continue;
      this.queue.push(r);
      this.holdBusy(r);
      this.setAnnotation(r, "queued");
    }
    this.queue.sort((a, b) => a.rank - b.rank);
    this.pump();
  }

  pump() {
    while (!this.destroyed && !this.stale && this.optionsLoading === 0 && this.inFlight < MAX_IN_FLIGHT && this.queue.length) {
      const job = this.queue.shift();
      if (job.annotation) this.startAnnotation(job);
      else this.start1(job);
    }
  }

  maxDataPoints(p) { // `p` is the record of the panel
    const q = p.panel.data.query;
    if (q.max_data_points !== null && q.max_data_points !== undefined) return q.max_data_points;
    const width = Math.round(this.widthOf(p.key) || 0);
    return Math.min(10000, Math.max(1, width));
  }

  // One request for one slot: the query of the slot, with the range and the width of its panel (9.8d). The panel members apply to all slots (4.15a, rule 4).
  async start1(slot) {
    const p = slot.rec;
    // A time override that gives an unusable range: the panel has the state `error` and sends no query (9.8b, step 4).
    if (p.range && p.range.error) {
      this.wholeError(p, Object.assign(new Error(p.range.error), { code: "invalid_range" }));
      return;
    }
    const controller = new AbortController();
    slot.controller = controller;
    this.inFlight += 1;
    slot.dirty = false;
    this.setSlot(slot, "loading");
    const body = requestBody(p.panel, slot.query, {
      etag: this.etag,
      from_ms: p.range.from_ms,
      to_ms: p.range.to_ms,
      max_data_points: this.maxDataPoints(p),
      // A copy of a repeat sends the request of its panel with the repeat variable fixed to its value (9.8c).
      variables: p.variable !== null && p.value !== null ? copyVariables(this.requestVariables(), p.variable, p.value) : this.requestVariables(),
    });
    try {
      const frame = await this.api.panelQuery(this.uid, p.id, body, controller.signal);
      if (slot.controller !== controller) return;
      slot.controller = null;
      this.inFlight -= 1;
      this.releaseBusy(slot);
      this.setSlot(slot, "ready", { frame, error: null });
    } catch (error) {
      if (slot.controller !== controller) return; // aborted: the code that aborted it set the state
      slot.controller = null;
      this.inFlight -= 1;
      this.releaseBusy(slot);
      if (error.aborted) return;
      if (error.code === "dashboard_changed") {
        slot.dirty = true;
        this.handleStale(error);
        return;
      }
      const shown = error.code === "payload_too_large" ? Object.assign(error, { message: TOO_LARGE }) : error;
      if (isWholeError(shown)) this.wholeError(p, shown);
      else this.setSlot(slot, "error", { error: shown, frame: null });
    }
    this.pump();
  }

  // One request for an annotation (6.21a): all the panels share its result. The body has no SQL and no `max_data_points`.
  async startAnnotation(r) {
    const controller = new AbortController();
    r.controller = controller;
    this.inFlight += 1;
    r.dirty = false;
    this.setAnnotation(r, "loading");
    const body = { etag: this.etag, from_ms: this.range.from_ms, to_ms: this.range.to_ms, variables: this.requestVariables() };
    try {
      const items = readAnswer(await this.api.annotationQuery(this.uid, r.index, body, controller.signal));
      if (r.controller !== controller) return;
      r.controller = null;
      this.inFlight -= 1;
      this.releaseBusy(r);
      this.setAnnotation(r, "ready", { items, error: null });
    } catch (error) {
      if (r.controller !== controller) return; // aborted: the code that aborted it set the state
      r.controller = null;
      this.inFlight -= 1;
      this.releaseBusy(r);
      if (error.aborted) return;
      if (error.code === "dashboard_changed") {
        r.dirty = true;
        this.handleStale(error);
        return;
      }
      this.setAnnotation(r, "error", { items: [], error });
    }
    this.pump();
  }

  // 409 dashboard_changed (7.15): the page reads the document again.
  handleStale(error) {
    if (this.stale) return;
    this.stale = true;
    const waiting = [...this.panels.values()].filter((p) => p.state === "queued" || p.state === "loading");
    this.abortAll();
    // A panel that waited shows its last result again, or nothing. In edit mode the page stays as it is (10.8).
    for (const p of waiting) this.setState(p, p.frame ? "ready" : "idle");
    this.emit("stale", error.extra && error.extra.current_etag);
  }

  // --- triggers -------------------------------------------------------------------------------------------------

  setVisible(key, visible) {
    const p = this.panels.get(key);
    if (!p || p.visible === visible) return;
    p.visible = visible;
    if (visible) this.schedule();
  }

  setExpanded(section, expanded) {
    this.expanded.set(section, expanded);
    if (expanded) this.schedule();
  }

  // The view of one panel has its own width, so the panel (and the source of a reuse panel) queries again with it (9.8).
  setViewPanel(id) {
    this.viewPanel = id;
    if (id !== null) {
      this.abortAllExcept(id);
      for (const r of this.annotations) if (!r.controller) r.dirty = true;
      const view = this.ensureView(id);
      if (view) {
        this.markDirty(view, false);
        const source = view.panel.data.reuse ? this.panels.get(view.panel.data.reuse.panel) : null;
        if (source) this.markDirty(source, false);
      }
    }
    this.schedule();
  }

  abortAllExcept(id) {
    const keep = new Set([id]);
    const view = this.panels.get(id);
    if (view && view.panel.data.reuse) keep.add(view.panel.data.reuse.panel);
    for (const p of this.panels.values()) {
      if (keep.has(p.key)) continue;
      this.abortPanel(p);
      if (p.state === "queued" || p.state === "loading") {
        this.markDirty(p);
        this.idle(p);
        this.setState(p, p.state);
      }
    }
    // An annotation that does not apply to the view stops, and it queries again when the view closes.
    for (const r of this.annotations) {
      if (view && view.panel.type === "timeseries" && applies(r.a, view.id)) continue;
      this.abortPanel(r);
      r.dirty = true;
      if (r.state === "queued" || r.state === "loading") r.state = "idle";
    }
  }

  retry(id) {
    const p = this.panels.get(id);
    if (!p) return;
    this.abortPanel(p);
    this.markDirty(p);
    this.schedule();
  }

  // A new time range. It returns { error } when the range is not valid (the earlier range stays).
  setRange(fromText, toText) {
    const before = { ...this.range };
    this.range.fromText = fromText;
    this.range.toText = toText;
    if (!this.recomputeRange()) {
      const error = this.range.error;
      Object.assign(this.range, before);
      return { error };
    }
    this.afterTimeChange();
    return {};
  }

  // "Refresh now" and each tick: a relative range gets a new `t` (9.9).
  refreshNow() {
    if (this.stale) return;
    this.recomputeRange();
    this.afterTimeChange();
  }

  afterTimeChange() {
    this.abortAll();
    this.emit("range");
    const names = this.timeChangeVariables();
    this.markAllDirty();
    for (const p of this.panels.values()) if (p.state === "queued" || p.state === "loading") p.state = "idle";
    if (!names.length) {
      this.schedule();
      return;
    }
    this.varChain = this.varChain.then(async () => {
      await this.reloadOptions(names);
      this.schedule();
    });
  }

  // A variable changed in the bar (9.10): reload the options below, then run the panels that name a changed variable.
  setSelection(name, selection) {
    const variable = this.varByName.get(name);
    if (!variable) return;
    this.selections.set(name, selection);
    this.emit("vars");
    this.varChain = this.varChain.then(async () => {
      const changed = new Set([name]);
      const below = this.descendantsOf([name]);
      for (const p of this.panels.values()) for (const slot of p.slots) if ([...slot.names].some((n) => changed.has(n))) this.abortJob(slot);
      // A new list of values makes new copies at once: the copies that leave are aborted, and the new ones query when they are near the viewport (9.8c).
      if (this.repeats) this.rebuild();
      if (below.length) for (const n of await this.reloadOptions(below)) changed.add(n);
      this.markPanelsDirty(changed);
      this.schedule();
    });
    return this.varChain;
  }

  // A changed variable aborts and marks only the slots whose query names it (9.8d, rule 5). The other slots keep their frames.
  markPanelsDirty(changed) {
    for (const p of this.panels.values()) {
      for (const slot of p.slots) {
        if (![...slot.names].some((n) => changed.has(n))) continue;
        this.abortJob(slot);
        slot.dirty = true;
        if (slot.state === "queued" || slot.state === "loading") slot.state = "idle";
      }
      this.idle(p);
    }
    // The annotations whose SQL names a changed variable query again (9.8).
    for (const r of this.annotations) {
      if (![...r.names].some((n) => changed.has(n))) continue;
      this.abortPanel(r);
      r.dirty = true;
      if (r.state === "queued" || r.state === "loading") r.state = "idle";
    }
  }

  // True while a request is in flight or queued (the refresh tick skips then, 9.9).
  isBusy() {
    return this.inFlight > 0 || this.queue.length > 0 || this.optionsLoading > 0;
  }

  destroy() {
    this.destroyed = true;
    this.abortAll();
    // The controls of the variables leave with the runtime (vars.js).
    this.emit("destroy");
    this.listeners.clear();
  }
}
