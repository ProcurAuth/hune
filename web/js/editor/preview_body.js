// The body of a preview request (6.22, 10.3). Pure module: no DOM access.

import { findMacros, sqlText, variableNamesOfLines } from "../sqlscan.js";

// The names of the variables that a query names, with their parents (7.14), in document order.
export function neededVariables(variables, sqlLines) {
  const byName = new Map(variables.map((v) => [v.name, v]));
  const need = new Set();
  const visit = (lines) => {
    for (const name of variableNamesOfLines(lines)) {
      if (!byName.has(name) || need.has(name)) continue;
      need.add(name);
      const v = byName.get(name);
      if (v.kind === "query") visit(v.sql);
    }
  };
  visit(sqlLines);
  return variables.filter((v) => need.has(v.name));
}

// The value of a variable of the draft that the page does not know (a variable that the draft added): its default.
function defaultValue(variable) {
  // An adhoc variable has no default: no filter (4.10c, 7.7a).
  if (variable.kind === "adhoc") return { filters: [] };
  if (variable.kind === "text") return { all: false, values: [variable.default] };
  if (variable.kind === "constant") return { all: false, values: [variable.value] };
  if (variable.kind === "interval") return { all: false, values: [variable.default || (variable.auto ? "$__auto" : variable.intervals[0])] };
  const d = variable.default || [];
  if (d.includes("$__all")) return { all: true, values: [] };
  if (d.length) return { all: false, values: d };
  if (variable.kind === "custom" && variable.options.length) return { all: false, values: [variable.options[0].value] };
  return { all: false, values: [] };
}

/**
 * The range of a preview (9.8b, 10.3): the range of the page, or the range of the time override of the draft panel.
 * `range` is { from_ms, to_ms, fromText, toText }: the texts tell the function when both ends are absolute (`time_from` does nothing then). `panelRange(query, { from, to }, now, zone)` is the function of panel_time.js, or null when the page has none.
 * It returns { range } or { error }.
 */
export function previewRange(query, range, { panelRange = null, now = Date.now(), zone = "browser" } = {}) {
  if (!panelRange || (!query.time_from && !query.time_shift)) return { range };
  const r = panelRange(query, { from: range.from_ms, to: range.to_ms, fromText: range.fromText, toText: range.toText }, now, zone);
  if (r.error) return { error: r.error };
  return { range: { from_ms: r.from, to_ms: r.to } };
}

/**
 * The body of POST api/sources/<id>/preview for a draft panel.
 * `known` is the object `variables` of a query request of the page (the values of the base runtime).
 * `target` is { uid } for a saved dashboard or { folder } for a new one.
 */
export function previewBody({ doc, panel, target, range, maxDataPoints, known }) {
  const needed = neededVariables(doc.variables, panel.data.query.sql);
  // `$__adhocFilters` reads the adhoc variable of the source of the query (7.10a): the draft needs it too.
  const query = panel.data.query;
  const adhoc = findMacros(sqlText(query.sql)).includes("__adhocFilters") && doc.variables.find((v) => v.kind === "adhoc" && v.source === query.source);
  if (adhoc && !needed.includes(adhoc)) needed.push(adhoc);
  const variables = {};
  for (const v of needed) variables[v.name] = known && known[v.name] ? known[v.name] : defaultValue(v);
  return {
    ...target,
    from_ms: range.from_ms,
    to_ms: range.to_ms,
    max_data_points: Math.min(10000, Math.max(1, maxDataPoints)),
    dashboard_variables: needed,
    variables,
    panel,
  };
}

/**
 * The body of POST api/sources/<id>/preview for the options of a draft variable (6.22, 10.3b).
 * `dashboard_variables` holds the variable and its parents. `variables` holds the values of the parents, never a value of the variable itself.
 * The member `max_data_points` is absent. `known` and `target` are as for `previewBody`.
 */
export function variablePreviewBody({ doc, variable, target, range, known }) {
  const parents = new Set(neededVariables(doc.variables, variable.sql).map((v) => v.name));
  parents.delete(variable.name);
  const needed = doc.variables.filter((v) => v.name === variable.name || parents.has(v.name));
  const variables = {};
  for (const v of needed) if (v.name !== variable.name) variables[v.name] = known && known[v.name] ? known[v.name] : defaultValue(v);
  return { ...target, from_ms: range.from_ms, to_ms: range.to_ms, dashboard_variables: needed, variables, variable: variable.name };
}

// An error pointer of a preview, relative to the panel: the prefix /panel goes away (10.3).
// It returns { pointer, inVariables } where inVariables is true for the dashboard JSON.
export function panelPointer(pointer) {
  if (pointer === "/panel") return { pointer: "", inVariables: false };
  if (pointer.startsWith("/panel/")) return { pointer: pointer.slice(6), inVariables: false };
  if (pointer.startsWith("/dashboard_variables")) return { pointer, inVariables: true };
  return { pointer, inVariables: false };
}

// The validation errors of a whole draft that lie in one panel, with the pointer relative to the panel (10.3).
export function errorsOfPanel(errors, sectionIndex, panelIndex) {
  const prefix = `/sections/${sectionIndex}/panels/${panelIndex}`;
  const own = [];
  let other = 0;
  for (const e of errors) {
    if (e.pointer === prefix) own.push({ ...e, pointer: "" });
    else if (e.pointer.startsWith(`${prefix}/`)) own.push({ ...e, pointer: e.pointer.slice(prefix.length) });
    else other += 1;
  }
  return { own, other };
}
