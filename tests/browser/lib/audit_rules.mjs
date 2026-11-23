// SPDX-License-Identifier: MIT OR Apache-2.0
// The verdict rule of the audit B153 (10.3h, rule 14, F43). It is a pure function: it reads the results of the probes and the closed list, and it
// gives the list of failures. A front test (`tests/frontend/editor_audit_rule.test.mjs`) gives it synthetic results, one for each verdict.
//
// A result: { type, state, pointer, value, kind, draft: [changed members], writes: [permitted extra members], preview: boolean,
//             back: boolean, skipped: string | null }
//   `pointer` is the member of the control (the `data-option` of its root), as a dotted path from the panel: `options.draw`.
//   `draft` lists the members that changed in the panel, as dotted paths of the leaves (an array is one leaf).
// An entry of the closed list: { pointer, types: [type, ...] | null, cls: "design" | "data", cause }
//   One entry covers its member and the members under it. A pointer with `=` and a value (`options.tooltip.mode=multi`) covers that choice only.
//   `types` null means all types.
// HOVER (the lists audit_a, audit_b and audit_c): { type: { sel?, x, y } }. `sel` is the element of the preview (default `.panel__plot`); `x` and `y` are
//   fractions of its box. The audit moves the pointer there before each read of the preview of a control of the tooltip.
// SHOWN: { "<type>|<state>": [members that the table shows in this state] }   FOUND: the same shape, from the pane.

export const MAX_EXEMPT = 30;
export const MIN_CAUSE = 20;

const under = (pointer, base) => pointer === base || pointer.startsWith(`${base}.`) || pointer.startsWith(`${base}[`);

const covers = (entry, r) => {
  if (entry.types && !entry.types.includes(r.type)) return false;
  const p = entry.pointer;
  if (p.includes("=")) return `${r.pointer}=${r.value}` === p;
  return under(r.pointer, p);
};

/**
 * Merges the lists of the owners (A, B and C) into one list. Two entries with the same member and the same class join: their types add up, and the
 * first cause stays. Two entries with the same member and two classes stay apart, so the verdict reports the second one as written twice.
 */
export function mergeExempt(lists) {
  const out = [];
  for (const list of lists) {
    for (const entry of list) {
      const same = out.find((e) => e.pointer === entry.pointer && e.cls === entry.cls && e.cause === entry.cause);
      if (!same) {
        out.push({ ...entry, types: entry.types ? [...entry.types] : null });
        continue;
      }
      if (same.types === null || entry.types === null) same.types = null;
      else same.types = [...new Set([...same.types, ...entry.types])];
    }
  }
  return out;
}

/** The members that a result may change: its own member, the members under it, and the `writes` of its row. */
const permitted = (r, member) => under(member, r.pointer) || (r.writes || []).some((w) => under(member, w));

/** The failures of a run. An empty list is a pass. */
export function judge({ results, exempt = [], shown = {}, found = {} }) {
  const fail = [];
  const used = new Set();
  const seen = new Set();
  for (const e of exempt) {
    const key = `${e.pointer}|${(e.types || ["*"]).slice().sort().join()}`;
    if (!["design", "data"].includes(e.cls) || typeof e.cause !== "string" || e.cause.length < MIN_CAUSE) fail.push(`entry ${e.pointer}: the class or the cause is not valid`);
    if (seen.has(key)) fail.push(`entry ${e.pointer}: written twice`);
    seen.add(key);
  }
  if (exempt.length > MAX_EXEMPT) fail.push(`the closed list has ${exempt.length} entries; the cap is ${MAX_EXEMPT}`);
  for (const r of results) {
    const where = `${r.type} ${r.state} ${r.pointer}${r.value === undefined ? "" : `=${r.value}`}${r.label ? ` [${r.label}]` : ""}`;
    if (r.skipped) {
      fail.push(`${where}: not tried (${r.skipped})`);
      continue;
    }
    const hits = exempt.map((e, i) => [e, i]).filter(([e]) => covers(e, r));
    for (const [e, i] of hits) for (const t of e.types || [r.type]) if (t === r.type) used.add(`${i}|${t}`);
    const entry = hits.length ? hits[0][0] : null;
    const extra = r.draft.filter((m) => !permitted(r, m));
    if (r.draft.length === 0) fail.push(`${where}: the draft does not change`);
    else if (extra.length) fail.push(`${where}: the draft changes other members: ${extra.join(", ")}`);
    else if (!r.draft.some((m) => under(m, r.pointer))) fail.push(`${where}: the draft does not change this member`);
    if (!r.preview && !entry) fail.push(`${where}: the preview does not change`);
    if (r.preview && entry) fail.push(`${where}: the entry is stale (the preview changes): remove it`);
    if (r.back === false) fail.push(`${where}: the preview is not stable`);
  }
  exempt.forEach((e, i) => {
    for (const t of e.types || ["*"]) {
      const hit = t === "*" ? [...used].some((u) => u.startsWith(`${i}|`)) : used.has(`${i}|${t}`);
      if (!hit) fail.push(`entry ${e.pointer}${t === "*" ? "" : ` for ${t}`}: no control matches it (dead entry)`);
    }
  });
  for (const [state, want] of Object.entries(shown)) {
    const got = new Set(found[state] || []);
    for (const m of want) if (!got.has(m)) fail.push(`${state}: the table lists ${m}, the pane has no control for it`);
    for (const m of got) if (!want.includes(m)) fail.push(`${state}: the pane has a control for ${m}, the table has no row for it`);
  }
  return fail;
}

/**
 * The members of a type that have no row and no entry of NO_CONTROL (rule F5). `members` are the dotted paths of `newOptions(type)` and of the
 * optional members; `rows` are the member paths of the table; `noControl` is the map of the causes.
 */
export function membersWithoutControl({ members, rows, noControl = {} }) {
  return members.filter((m) => !rows.includes(m) && !Object.keys(noControl).some((n) => under(m, n)));
}

/** The members of the draft that changed between two panels, as dotted paths of leaves (an array or a scalar is one leaf). */
export function changedMembers(before, after) {
  const out = [];
  const walk = (a, b, path) => {
    const plain = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
    if (plain(a) && plain(b)) {
      for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) walk(a[key], b[key], path ? `${path}.${key}` : key);
      return;
    }
    if (JSON.stringify(a) !== JSON.stringify(b)) out.push(path);
  };
  walk(before, after, "");
  return out;
}
