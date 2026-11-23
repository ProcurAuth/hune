// The queries of a panel in the draft (4.15a, 10.3): add, delete, rename, hide. Pure module: no DOM access.
// Each function takes a panel and gives a new panel. The first query is `data.query` and carries the members of the panel
// (`min_interval`, `max_data_points`, `time_from`, `time_shift`, `hide_time_override`); the others are the items of `data.extra_queries`.

import { MAX_QUERIES_PER_PANEL, queriesOf } from "../queries.js";
import { setQueryMember } from "./forms/model.js";

const PANEL_MEMBERS = ["min_interval", "max_data_points", "time_from", "time_shift", "hide_time_override"];
const REF = /^[A-Za-z][A-Za-z0-9_]{0,15}$/;
const LETTERS = Array.from({ length: 26 }, (_, i) => String.fromCharCode(65 + i));
// The refs that "Add query" offers, in order: A to Z, then AA to AZ (10.3).
const OFFERED = [...LETTERS, ...LETTERS.map((l) => `A${l}`)];

/** The queries of the panel as plain items { ref, source, format, sql, hide }, in the order of the document. */
const listOf = (panel) => queriesOf(panel).map((q) => ({ ref: q.ref, source: q.source, format: q.format, sql: q.sql, hide: q.hide }));

/** The panel with its queries written from the list `items`: the canonical forms of 4.15a and 4.38. The panel members stay in `data.query`. */
function withList(panel, items) {
  const [first, ...rest] = items;
  const old = panel.data.query;
  let query = { source: first.source, format: first.format, sql: first.sql };
  for (const key of PANEL_MEMBERS) if (Object.hasOwn(old, key)) query = setQueryMember(query, key, old[key]);
  query = setQueryMember(query, "ref", first.ref === "A" ? undefined : first.ref);
  query = setQueryMember(query, "hide", first.hide ? true : undefined);
  const data = { query };
  if (rest.length) data.extra_queries = rest.map((q) => ({ ref: q.ref, source: q.source, format: q.format, sql: q.sql, ...(q.hide ? { hide: true } : {}) }));
  return { ...panel, data };
}

/** The panel with the member `patch` ({ source, format, sql }) set in the query at `index`. */
export function setQueryAt(panel, index, patch) {
  const items = listOf(panel);
  items[index] = { ...items[index], ...patch };
  return withList(panel, items);
}

/** The first free ref of the list A to Z, then AA to AZ, or null when there is none. */
export const freeRef = (panel) => OFFERED.find((r) => !queriesOf(panel).some((q) => q.ref === r)) || null;

/** The text that says why "Add query" is off for this panel, or "". `repeats` is true for a panel with `repeat` or in a section with `repeat`. */
export function addRefusal(panel, repeats) {
  if (queriesOf(panel).length >= MAX_QUERIES_PER_PANEL) return `A panel has ${MAX_QUERIES_PER_PANEL} queries at most.`;
  if (repeats) return "A panel with a repeat has one query.";
  if (panel.type === "barchart" || panel.type === "xychart") return "A bar chart and an XY chart draw one query.";
  if (panel.type === "geomap") return "A map draws one query.";
  return "";
}

/** The panel with a new query at the end: the source and the format of the query at `index`, an empty SQL line, the first free ref. */
export function addQuery(panel, index) {
  const items = listOf(panel);
  const from = items[index] || items[0];
  items.push({ ref: freeRef(panel), source: from.source, format: from.format, sql: [""], hide: false });
  return withList(panel, items);
}

/** The panel without the query at `index`. The last query stays. When the first one leaves, the next one takes the members of the panel. */
export function deleteQuery(panel, index) {
  const items = listOf(panel);
  if (items.length < 2) return panel;
  items.splice(index, 1);
  return withList(panel, items);
}

/** The panel with the query at `index` hidden, or shown when `hide` is false. */
export const hideQuery = (panel, index, hide) => setQueryAt(panel, index, { hide });

/** The text of the error of a new ref for the query at `index`, or "" (10.3). */
export function refError(panel, index, ref) {
  if (!REF.test(ref)) return "A name starts with a letter, then letters, digits or _, 16 characters at most.";
  return queriesOf(panel).some((q, i) => i !== index && q.ref === ref) ? "Another query has this name." : "";
}

/** The matchers `query` of the panel that name `ref`, as the list of the overrides that change in a rename: [{ index }]. */
export const matchersOf = (panel, ref) => ((panel.fields && panel.fields.overrides) || []).flatMap((o, index) => (o.match && o.match.by === "query" && o.match.value === ref ? [{ index }] : []));

/** The panel with the ref of the query at `index` changed, and the matchers `query` of the panel that name the old ref changed too. */
export function renameQuery(panel, index, ref) {
  const old = queriesOf(panel)[index].ref;
  const items = listOf(panel);
  items[index] = { ...items[index], ref };
  const next = withList(panel, items);
  if (!next.fields || !next.fields.overrides) return next;
  return { ...next, fields: { ...next.fields, overrides: next.fields.overrides.map((o) => (o.match && o.match.by === "query" && o.match.value === old ? { ...o, match: { ...o.match, value: ref } } : o)) } };
}

/** The panel without its extra queries: the first query stays (a change to `barchart` or `xychart`, 10.3). */
export function dropExtra(panel) {
  if (!panel.data || !panel.data.extra_queries) return panel;
  const { extra_queries: gone, ...data } = panel.data;
  return { ...panel, data };
}
