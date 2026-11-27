// The URL contract (9.5): read and write of the query names. Pure functions, no DOM.

import { viewKeyFromParam } from "./repeat.js";
import { urlTimeText } from "./time.js";

// The names of the URL that choose a view (9.5). Each state uses a part of them.
export const VIEW_NAMES = ["viewPanel", "edit", "editPanel", "editview", "version", "base"];

// The slug of a title: ASCII letters to lower case, each run of other characters to one "-",
// no "-" at the start or the end, at most 64 characters (9.5).
export function slugify(title) {
  const folded = String(title ?? "").replace(/[A-Z]/g, (c) => c.toLowerCase());
  let slug = folded.replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  if (slug.length > 64) slug = slug.slice(0, 64).replace(/-+$/g, "");
  return slug;
}

// The relative path of a dashboard page: d/<uid> or d/<uid>/<slug>.
export function dashboardPath(uid, title) {
  const slug = slugify(title);
  return slug ? `d/${uid}/${slug}` : `d/${uid}`;
}

// The id of a panel from the value of viewPanel: "7" or "panel-7". It returns a number or null.
export function panelIdFromParam(value) {
  if (value === null || value === undefined) return null;
  const match = /^(?:panel-)?([1-9][0-9]*)$/.exec(String(value));
  if (!match) return null;
  const id = Number(match[1]);
  return Number.isSafeInteger(id) ? id : null;
}

// Reads a query string. `viewPanel` is the key of a panel or of a copy of a repeat (9.8c): a number, the text `<id>-r<n>`, or the form `<x>$panel-<id>` of Grafana as it is. Names that the contract does not know are dropped (orgId, for example).
// A time form of Grafana in `from` or `to` gives epoch milliseconds. The first `timezone` applies, as in Grafana; the page checks the name (9.5).
export function parseSearch(search) {
  const params = new URLSearchParams(search || "");
  const state = {
    from: null,
    to: null,
    timezone: null,
    vars: new Map(),
    refresh: null,
    viewPanel: null,
    edit: false,
    editPanel: null,
    editview: null,
    version: null,
    base: null,
    q: null,
  };
  for (const [key, value] of params) {
    if (key === "from") state.from = urlTimeText(value);
    else if (key === "to") state.to = urlTimeText(value);
    else if (key === "timezone") state.timezone ??= value;
    else if (key === "refresh") state.refresh = value;
    else if (key === "viewPanel") state.viewPanel = viewKeyFromParam(value);
    else if (key === "edit") state.edit = value === "1";
    else if (key === "editPanel") state.editPanel = panelIdFromParam(value);
    else if (key === "editview") state.editview = value === "json" || value === "history" || value === "settings" ? value : null;
    else if (key === "version") state.version = value;
    else if (key === "base") state.base = value;
    else if (key === "q") state.q = value;
    else if (key.startsWith("var-") && key.length > 4) {
      const name = key.slice(4);
      if (!state.vars.has(name)) state.vars.set(name, []);
      state.vars.get(name).push(value);
    }
  }
  return state;
}

// Writes a query string in the order of 9.5: from, to, timezone, var-<name> in document order, refresh, then the view names.
// `varOrder` is the list of variable names in document order. A state value that is null is not written.
export function buildSearch(state, varOrder = []) {
  const parts = [];
  const add = (key, value) => parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(value)}`);
  if (state.from !== null && state.from !== undefined) add("from", state.from);
  if (state.to !== null && state.to !== undefined) add("to", state.to);
  if (state.timezone) add("timezone", state.timezone);
  for (const name of varOrder) {
    const values = state.vars instanceof Map ? state.vars.get(name) : undefined;
    if (!values) continue;
    for (const value of values) add(`var-${name}`, value);
  }
  if (state.refresh !== null && state.refresh !== undefined) add("refresh", state.refresh);
  for (const name of VIEW_NAMES) {
    const value = state[name];
    if (value === null || value === undefined || value === false) continue;
    add(name, value === true ? "1" : value);
  }
  return parts.length ? `?${parts.join("&")}` : "";
}

// The query that the page writes (9.5): the names whose value differs from the default of the dashboard. A name that equals its default is
// left out, so the address of a state with the defaults has the view names only (`?edit=1`).
// `query` is { from, to, timezone, vars, refresh } as the page holds them: `vars` is a Map of a variable name to its URL values, `refresh` is a
// refresh choice or "off". `defaults` has the same members for the dashboard document: `vars` is a Map of a name to the URL values of the default
// selection, and a name with no entry has no known default yet (the options of a query variable are not loaded): that name stays.
// The range is one unit: when `from` or `to` differs, the query keeps both. The result has the same members, with null for a name that is left out.
export function shortQuery(query, defaults) {
  const sameRange = query.from === defaults.from && query.to === defaults.to;
  const vars = new Map();
  for (const [name, values] of query.vars instanceof Map ? query.vars : []) {
    const known = defaults.vars instanceof Map ? defaults.vars.get(name) : undefined;
    const same = Array.isArray(known) && known.length === values.length && known.every((v, i) => v === values[i]);
    if (!same) vars.set(name, values);
  }
  return {
    from: sameRange ? null : query.from,
    to: sameRange ? null : query.to,
    timezone: query.timezone && query.timezone !== defaults.timezone ? query.timezone : null,
    vars,
    refresh: query.refresh === defaults.refresh ? null : query.refresh,
  };
}

export function listSearch(q) {
  return q ? `?q=${encodeURIComponent(q)}` : "";
}
