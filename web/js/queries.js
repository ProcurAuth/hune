// The queries of a panel, the join of frames, the display prefix and the state of a panel (4.15a, 9.8d, 9.22a). Pure module: no DOM access.
// A "query" is `data.query` or an item of `data.extra_queries`. A "slot" is the run of one visible query: { ref, state, frame, error }.
import { checkShape } from "./transform_more.js";

/** The queries of one panel at most (4.35). The document validation checks it: the page only reads it. */
export const MAX_QUERIES_PER_PANEL = 8;

/** The rows that the frames of one panel have at most for a join (9.22a, rule 8, decision ID-K167). */
export const MAX_JOIN_ROWS = 200000;

// The panel types that join their frames on time. The other types that take several frames collect them (9.22a, rule 4).
const JOINED = new Set(["timeseries", "state_timeline", "status_history", "heatmap"]);

// The HTTP statuses of an error of the request itself (9.8d). `503 pdp_unavailable` is checked by its code.
const WHOLE_STATUS = new Set([401, 403, 404, 409, 413]);

const NUMBERS = new Set(["number", "time"]);

/**
 * The queries of a panel in the order of the document, each with its `ref` (`A` when absent in `data.query`) and a boolean `hide`.
 * A panel with no query (a `reuse`, a text panel) has none.
 */
export function queriesOf(panel) {
  const d = panel && panel.data;
  if (!d || !d.query) return [];
  return [{ ...d.query, ref: d.query.ref || "A", hide: d.query.hide === true }, ...(d.extra_queries || []).map((q) => ({ ...q, hide: q.hide === true }))];
}

/** The queries that run: the queries with no `hide` equal to `true` (4.15a, rule 3). */
export const visibleQueries = (panel) => queriesOf(panel).filter((q) => !q.hide);

/** True when the panel has `extra_queries`: each request then names its query with `ref` (6.20). */
export const hasExtra = (panel) => !!(panel && panel.data && panel.data.extra_queries);

/** The body of the request of one query: the body of the panel, with `ref` only for a panel that has `extra_queries` (9.8d, rule 1). */
export const requestBody = (panel, query, body) => (hasExtra(panel) ? { ...body, ref: query.ref } : body);

/** The slots of a panel record that has none: the plain record of one query is one slot with the ref of its query. */
export function slotsOf(record) {
  if (record.slots) return record.slots;
  const query = record.panel && record.panel.data && record.panel.data.query;
  return [{ ref: (query && query.ref) || "A", state: record.state, frame: record.frame || null, error: record.error || null }];
}

/**
 * The state of a panel as a function of its slots (9.8d). `empty`: no visible query. `loading` and `queued` come first. A slot that did
 * not start while no slot loads or waits gives `idle`. When all slots ended: `ready` with at least one frame, else `error`.
 */
export function panelState(slots) {
  if (!slots.length) return "empty";
  if (slots.some((s) => s.state === "loading")) return "loading";
  if (slots.some((s) => s.state === "queued")) return "queued";
  if (slots.some((s) => s.state === "idle")) return "idle";
  return slots.some((s) => s.frame) ? "ready" : "error";
}

/** The slots that failed, in the order of the queries. */
export const failedSlots = (slots) => slots.filter((s) => s.state === "error" && s.error);

/** The refs of the failed slots, for the attribute `data-failed` (9.8d). */
export const failedRefs = (slots) => failedSlots(slots).map((s) => s.ref).join(",");

/** The text of the badge: "Error:" and one line for each failed query, "Query <ref>: <code>: <message>" (9.8d). */
export function badgeText(slots) {
  const lines = failedSlots(slots).map((s) => `Query ${s.ref}: ${[s.error.code, s.error.message].filter(Boolean).join(": ")}`);
  return ["Error:", ...lines].join("\n");
}

/** The message of the state `error`: the error of the first failed slot, with "Query <ref>: " for a panel that has `extra_queries` (9.8d). */
export function firstError(slots, many) {
  const slot = failedSlots(slots)[0];
  if (!slot) return null;
  if (!many) return slot.error;
  return Object.assign(new Error(`Query ${slot.ref}: ${slot.error.message}`), { code: slot.error.code, status: slot.error.status });
}

/** True for an error of the request that no query causes alone: the same error goes to all the slots of the panel (9.8d). */
export const isWholeError = (error) => WHOLE_STATUS.has(error.status) || error.code === "pdp_unavailable";

/**
 * Tags the fields of one query with its `ref` and, when the panel has two visible queries or more, with the `prefix` of the display name
 * (9.22a, rule 5). A time field never has a prefix.
 */
export const tagFields = (fields, ref, many) => fields.map((f) => ({ ...f, ref, prefix: many && f.kind !== "time" ? ref : null }));

// The time field of a frame and its rows in order (9.22a, rule 6): the rows with a null time leave, the sort is stable, the last row of equal times wins.
function timeRows(fields, ref) {
  const t = fields.find((f) => f.kind === "time");
  if (!t) throw new Error(`The frame of query ${ref} has no time column.`);
  const order = [];
  for (let i = 0; i < t.values.length; i++) if (!t.nulls[i] && !Number.isNaN(t.values[i])) order.push(i);
  order.sort((a, b) => t.values[a] - t.values[b] || a - b);
  const rows = [];
  for (const i of order) {
    if (rows.length && t.values[rows[rows.length - 1]] === t.values[i]) rows[rows.length - 1] = i;
    else rows.push(i);
  }
  return { t, rows };
}

/**
 * The outer join on time of the frames (9.22a, rules 6 to 8). `lists` is [{ ref, fields }] in the order of the queries.
 * It throws an Error for a frame with no time column and for more than MAX_JOIN_ROWS rows in all (checked first).
 */
export function joinByTime(lists) {
  let total = 0;
  for (const l of lists) total += l.fields.length ? l.fields[0].values.length : 0;
  if (total > MAX_JOIN_ROWS) throw new Error(`The frames have ${total} rows. A join takes ${MAX_JOIN_ROWS.toLocaleString("en-US")} rows at most.`);
  const parts = lists.map((l) => timeRows(l.fields, l.ref));
  const all = new Float64Array(parts.reduce((n, p) => n + p.rows.length, 0));
  let at = 0;
  for (const p of parts) for (const i of p.rows) all[at++] = p.t.values[i];
  all.sort();
  const axis = all.filter((v, i) => i === 0 || v !== all[i - 1]);
  // The caps of 9.21a, rule 13, before the page builds a field: the time and the other fields of each frame.
  checkShape("The join on time failed", lists.reduce((n, l) => n + l.fields.length - 1, 1), axis.length);
  const index = new Map(Array.from(axis, (v, i) => [v, i]));
  const first = parts[0].t;
  const out = [{ name: first.name, type: first.type, kind: "time", values: axis, labels: null, config: {}, nulls: new Uint8Array(axis.length) }];
  lists.forEach((l, k) => {
    const { t, rows } = parts[k];
    for (const f of l.fields) {
      if (f === t) continue;
      const g = { ...f, config: { ...f.config }, values: NUMBERS.has(f.kind) ? new Float64Array(axis.length).fill(NaN) : new Array(axis.length).fill(null) };
      if (f.nulls) g.nulls = new Uint8Array(axis.length).fill(1);
      if (f.num) g.num = new Float64Array(axis.length).fill(NaN);
      for (const i of rows) {
        const r = index.get(t.values[i]);
        g.values[r] = f.values[i];
        if (f.nulls) g.nulls[r] = f.nulls[i];
        if (f.num) g.num[r] = f.num[i];
      }
      out.push(g);
    }
  });
  return out;
}

/**
 * The fields of the panel from the lists of fields of its frames (9.22a, rule 4). `lists` is [{ ref, fields }] in the order of the queries.
 * `choice` is the ref that a table shows. One list is the result. A table takes one list. A joined type joins. A value type collects.
 * A type that draws one frame (`barchart`, `xychart`) takes the first list.
 */
export function combine(type, lists, choice = null) {
  if (lists.length < 2) return lists.length ? lists[0].fields : [];
  if (JOINED.has(type)) return joinByTime(lists);
  if (type === "table") return (lists.find((l) => l.ref === choice) || lists[0]).fields;
  if (type === "stat" || type === "bargauge" || type === "piechart" || type === "histogram" || type === "gauge") return lists.flatMap((l) => l.fields);
  return lists[0].fields;
}

/** The rows of a list of fields that can differ in length: the longest field. */
export const maxRows = (fields) => fields.reduce((n, f) => Math.max(n, f.values.length), 0);
