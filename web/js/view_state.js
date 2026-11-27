// The state of the dashboard page (10.2a): the state from the URL, the URL of a state and the table of transitions.
// Pure functions, no DOM. dashboard.js drives the page with them. The unit test runs the vectors FE-26.
//
// A state is { page, panel, editing, version, base }:
//   page     "view", "panel-view", "edit", "panel-edit", "json", "settings" or "history"
//   panel    the panel id for "panel-edit"; the key of a panel or of a copy of a repeat for "panel-view" (a number, or `<id>-r<n>`, 9.8c,
//            or the form `<x>$panel-<id>` of Grafana while the values load: rule 16); else null
//   editing  true while edit mode is on
//   version, base  the names of the history (10.11), else null

import { copyKeyOf, parseViewKey, splitKey } from "./repeat.js";
import { VIEW_NAMES, buildSearch, panelIdFromParam, parseSearch } from "./url_state.js";

export const PAGES = ["view", "panel-view", "edit", "panel-edit", "json", "settings", "history"];
export const EVENTS = ["view-panel", "close-view", "edit-on", "edit-off", "panel-edit", "open-json", "open-settings", "open-history", "history-nav", "up", "leave", "save", "discard"];

export const baseState = () => ({ page: "view", panel: null, editing: false, version: null, base: null });

const isEditPage = (page) => page === "edit" || page === "panel-edit" || page === "json" || page === "settings";

/** True for the states that the editor module shows (10.2a). */
export const isEditorState = (state) => state.page !== "view" && state.page !== "panel-view";

/** The kind of the origin that a state gives to the state that it opens: `view` or `edit` (10.2a). */
export const kindOf = (state) => (state.page === "view" || state.page === "panel-view" ? "view" : "edit");

export const sameState = (a, b) => a.page === b.page && a.panel === b.panel && a.editing === b.editing && a.version === b.version && a.base === b.base;

const ids = (value) => new Set(value || []);

/**
 * The state of a query string, by the precedence of rule 1.
 * ctx = { canEdit, viewPanels, editPanels } (iterables of panel ids), or `panels` for both. `repeatLimits` is a Map from the id of a panel
 * that repeats (or sits in a section that repeats) to the limit of that repeat: it makes a key `<id>-r<n>` with n up to the limit valid (rule 6).
 * `plan` (buildPlan) gives the copy that the form `<x>$panel-<id>` of Grafana names.
 * Returns { state, notices }. A notice is { code: "no-right" } | { code: "no-panel", id } | { code: "bad-value", name, value }.
 */
export function parseView(search, ctx) {
  const viewIds = ids(ctx.viewPanels || ctx.panels);
  const editIds = ids(ctx.editPanels || ctx.panels);
  const limits = ctx.repeatLimits || new Map();
  // A key is known when its panel exists and, for a copy, when the panel repeats and n is up to the limit. The count of values comes later (rule 16).
  // The form of Grafana (no n) is known while the values load.
  const viewKnown = (key) => {
    const { id, n } = splitKey(key);
    return viewIds.has(id) && (n === 1 || (limits.has(id) && (n === undefined || n <= limits.get(id))));
  };
  // The form of Grafana gives the key of the copy of value <x>, null, or itself while the values load (rule 16).
  const viewKey = (key) => {
    const { id, value } = splitKey(key);
    if (value === undefined) return key;
    const found = limits.has(id) && ctx.plan ? copyKeyOf(ctx.plan, id, value) : null;
    return found === undefined ? key : found;
  };
  const s = parseSearch(search);
  const raw = rawValues(search);
  const notices = [];
  const state = baseState();
  const badValue = (name) => {
    const value = raw[name];
    if (value === undefined) return;
    if (name === "editview" && s.editview === null) notices.push({ code: "bad-value", name, value });
    if ((name === "viewPanel" && s.viewPanel === null) || (name === "editPanel" && s.editPanel === null)) notices.push({ code: "no-panel", id: value });
  };
  const editNames = s.edit || s.editPanel !== null || s.editview === "json" || s.editview === "settings";
  if (s.editview === "history") {
    state.page = "history";
    state.editing = s.edit && ctx.canEdit;
    state.version = s.version;
    state.base = s.base;
    if (s.edit && !ctx.canEdit) notices.push({ code: "no-right" });
    return { state, notices };
  }
  badValue("editview");
  if (!ctx.canEdit && editNames) notices.push({ code: "no-right" });
  if (ctx.canEdit && s.editview === "json") return { state: { ...state, page: "json", editing: true }, notices };
  if (ctx.canEdit && s.editview === "settings") return { state: { ...state, page: "settings", editing: true }, notices };
  if (ctx.canEdit && (s.editPanel !== null || raw.editPanel !== undefined)) {
    if (s.editPanel !== null && editIds.has(s.editPanel)) return { state: { ...state, page: "panel-edit", panel: s.editPanel, editing: true }, notices };
    if (s.editPanel !== null) notices.push({ code: "no-panel", id: s.editPanel });
    else badValue("editPanel");
  }
  if (ctx.canEdit && s.edit) return { state: { ...state, page: "edit", editing: true }, notices };
  if (s.viewPanel !== null || raw.viewPanel !== undefined) {
    const key = s.viewPanel === null ? null : viewKey(s.viewPanel);
    if (key !== null && viewKnown(key)) return { state: { ...state, page: "panel-view", panel: key }, notices };
    if (s.viewPanel !== null) notices.push({ code: "no-panel", id: s.viewPanel });
    else badValue("viewPanel");
  }
  return { state, notices };
}

// The last value of each view name in a query string (the page reads the last one, as parseSearch does).
function rawValues(search) {
  const out = {};
  for (const [key, value] of new URLSearchParams(search || "")) if (VIEW_NAMES.includes(key)) out[key] = value;
  return out;
}

/** The view names of a state: the part of the URL after from, to, var-<name> and refresh. */
export function viewNames(state) {
  switch (state.page) {
    case "panel-view": return { viewPanel: state.panel };
    case "edit": return { edit: true };
    case "panel-edit": return { edit: true, editPanel: state.panel };
    case "json": return { edit: true, editview: "json" };
    case "settings": return { edit: true, editview: "settings" };
    case "history": return { edit: state.editing, editview: "history", version: state.version, base: state.base };
    default: return {};
  }
}

/** The query string of a state. `query` has from, to, vars and refresh (url_state.js buildSearch). */
export function renderSearch(state, query = {}, varOrder = []) {
  return buildSearch({ from: null, to: null, vars: new Map(), refresh: null, ...query, ...viewNames(state) }, varOrder);
}

// Does the state use this raw view name with this raw value?
function uses(state, name, value) {
  switch (name) {
    case "viewPanel": return state.page === "panel-view" && parseViewKey(value)?.id === splitKey(state.panel).id;
    case "edit": return state.editing && value === "1";
    case "editPanel": return state.page === "panel-edit" && panelIdFromParam(value) === state.panel;
    case "editview": return (state.page === "json" || state.page === "settings" || state.page === "history") && value === (state.page === "history" ? "history" : state.page);
    case "version": return state.page === "history" && state.version === value;
    case "base": return state.page === "history" && state.base === value;
    default: return true;
  }
}

/**
 * The normal form of a query string (rule 4): the same query without each view name that the state does not use.
 * The order of the names stays as it is, and the form `panel-<id>` goes to the number. A query with nothing to change is returned as it is.
 * It returns the text, with the `?`, or "" when it is empty.
 */
export function normalSearch(search, state) {
  const params = new URLSearchParams(search || "");
  const keep = [];
  let removed = false;
  // A name that repeats keeps only the last value, the one that the page reads.
  const last = rawValues(search);
  for (const [key, value] of params) {
    if (VIEW_NAMES.includes(key) && (value !== last[key] || !uses(state, key, value))) {
      removed = true;
      continue;
    }
    // `viewPanel` gets the key of the state: the forms of 9.5 go to the number or to `<id>-r<n>` (the form of Grafana stays while the values
    // load, rule 16). `editPanel=panel-<id>` goes to the number.
    const written = key === "viewPanel" ? String(state.panel) : key === "editPanel" ? String(panelIdFromParam(value)) : value;
    if (written !== value) removed = true;
    keep.push([key, written]);
  }
  if (!removed) return search || "";
  const text = keep.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join("&");
  return text ? `?${text}` : "";
}

// --- the origin in history.state ------------------------------------------------------------------------------------------------

/** The `history.state` value of an entry that opened a state from `from` (a relative URL). */
export const originValue = (from) => ({ hune: { from } });

/** The relative URL of the origin that a `history.state` value holds, or null (10.2a). */
export function readOrigin(historyState) {
  const from = historyState && historyState.hune && historyState.hune.from;
  return typeof from === "string" && from !== "" ? from : null;
}

// --- the transitions -------------------------------------------------------------------------------------------------------------------

/**
 * The next state for an event (the second table of 10.2a).
 * ctx = { origin: null | { kind: "view"|"edit", state }, dirty: boolean, newPanel?: boolean }
 * `origin` is null when the entry has no origin. `origin.state` is the state of the entry that opened this one.
 * Returns { state, history: "push"|"replace"|"pop"|"none", writeOrigin, dialog }.
 */
export function nextState(state, event, arg, ctx) {
  const o = ctx.origin;
  const none = (st = state) => ({ state: st, history: "none", writeOrigin: false, dialog: false });
  const push = (st, origin = true) => ({ state: st, history: "push", writeOrigin: origin, dialog: false });
  const replace = (st, dialog = false) => ({ state: st, history: "replace", writeOrigin: false, dialog });
  const edit = (page, panel = null) => ({ ...baseState(), page, panel, editing: true });
  // End of edit mode (rule 7): the origin when its kind is `view`, else the dashboard view. `pop` only from an origin of the kind `view`.
  const endEdit = (withDialog) => {
    const target = o && o.kind === "view" ? o.state : baseState();
    const pops = Boolean(o && o.kind === "view");
    return { state: target, history: pops ? "pop" : "replace", writeOrigin: false, dialog: withDialog && ctx.dirty };
  };
  switch (event) {
    case "view-panel":
      if (state.page === "view") return push({ ...baseState(), page: "panel-view", panel: arg }, false);
      return none();
    case "close-view":
      return state.page === "panel-view" ? push(baseState(), false) : none();
    case "edit-on":
      if (state.page === "view") return push(edit("edit"));
      // The view of a copy edits the stored panel: editPanel takes the id only (10.2a, rule 5 and the table of the events; 9.8c).
      if (state.page === "panel-view") return push(edit("panel-edit", splitKey(state.panel).id));
      if (state.page === "history" && !state.editing) return replace({ ...state, editing: true });
      return none();
    case "edit-off":
      if (state.page === "history") return state.editing ? replace({ ...state, editing: false }, ctx.dirty) : none();
      return isEditPage(state.page) ? endEdit(true) : none();
    case "save":
      if (state.page === "history") return state.editing ? replace({ ...state, editing: false }) : none();
      return isEditPage(state.page) ? endEdit(false) : none();
    case "panel-edit":
      if (state.page === "edit" || state.page === "view" || state.page === "panel-view") return push(edit("panel-edit", arg));
      return none();
    case "open-json":
    case "open-settings": {
      const page = event === "open-json" ? "json" : "settings";
      return isEditPage(state.page) && state.page !== page ? push(edit(page)) : none();
    }
    case "open-history":
      if (state.page === "history") return none();
      return push({ ...baseState(), page: "history", editing: isEditPage(state.page) });
    case "history-nav":
      // A version or a diff of the history, or the list again: a push with an origin (10.11).
      if (state.page !== "history") return none();
      return push({ ...state, version: arg && arg.version !== undefined ? arg.version : null, base: arg && arg.base !== undefined ? arg.base : null });
    case "up":
      // From a version or a diff to the list of the history.
      if (state.page !== "history" || (state.version === null && state.base === null)) return none();
      if (o) return { state: o.state, history: "pop", writeOrigin: false, dialog: false };
      return replace({ ...state, version: null, base: null });
    case "leave": {
      if (!(isEditPage(state.page) && state.page !== "edit") && state.page !== "history") return none();
      if (o) {
        const leavesEdit = o.kind === "view" && state.editing;
        return { state: o.state, history: "pop", writeOrigin: false, dialog: leavesEdit && ctx.dirty };
      }
      return replace(leaveTarget(state, null));
    }
    case "discard":
      if (state.page === "panel-edit" && ctx.newPanel) return nextState(state, "leave", arg, ctx);
      return none();
    default:
      return none();
  }
}

/** The target of `leave` for a state and an origin (the third set of FE-26). With no origin: edit mode, or the dashboard view. */
export function leaveTarget(state, origin) {
  if (origin) return origin.state;
  if (state.page === "history") return state.editing ? edit("edit") : baseState();
  return edit("edit");
}

function edit(page) {
  return { ...baseState(), page, editing: true };
}
