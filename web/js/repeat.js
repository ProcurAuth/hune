// The repeat of panels and sections by a variable (4.13a, 9.8c): the values, the copies, the keys and the layout.
// Pure module: no DOM access. The runtime, the grid and the editor call these functions.
// The four open decisions are closed, each one in one place below:
//   The caps are server keys (`limitsOf`).  The order of the copies: `orderedValues`.
//   Fail closed, no truncation: `classify`.  A variable that is not multi has no repeat: `canRepeatBy` (the importer rule is in Rust).

// The caps are keys of the server configuration. The page reads them from the session (`repeat_limits`) and gives
// them to each function below as `limits`. These three numbers are the defaults of the server: the page uses them when the session has no such member.
export const MAX_REPEAT_PER_PANEL = 24;
export const MAX_REPEAT_PER_SECTION = 12;
export const MAX_REPEAT_PER_DASHBOARD = 120;
/** The limits that apply when the session gives none: `{ panel, section, dashboard }`. */
export const DEFAULT_LIMITS = Object.freeze({ panel: MAX_REPEAT_PER_PANEL, section: MAX_REPEAT_PER_SECTION, dashboard: MAX_REPEAT_PER_DASHBOARD });

const HARD_LIMITS = { panel: 200, section: 50, dashboard: 200 };
/**
 * The limits of a session (the answer of GET api/session, 6.10). A member that is absent gives its default (an older server). A member that is
 * present and not a whole number from 1 to its hard bound gives 0: the repeat fails closed and never changes the number to make it fit.
 */
export function limitsOf(session) {
  const given = session && typeof session === "object" ? session.repeat_limits : undefined;
  const bad = given !== undefined && !(given && typeof given === "object");
  const pick = (name) => {
    const v = bad ? null : given?.[name];
    return v === undefined ? DEFAULT_LIMITS[name] : Number.isSafeInteger(v) && v >= 1 && v <= HARD_LIMITS[name] ? v : 0;
  };
  return { panel: pick("panel"), section: pick("section"), dashboard: pick("dashboard") };
}
/** The default of `max_per_row`, the value of Grafana. The document never stores it (4.13a, rule 4). */
export const DEFAULT_MAX_PER_ROW = 4;
export const GRID_COLUMNS = 24;

/** True for a variable that a repeat can name: the kind `query` or `custom`, and `multi` = true (4.13a, rule 1). */
export const canRepeatBy = (variable) => !!variable && (variable.kind === "query" || variable.kind === "custom") && variable.multi === true;

/** The limit of one kind of repeat: `"panel"` or `"section"` (4.35). */
export const limitFor = (kind, limits = DEFAULT_LIMITS) => (kind === "section" ? limits.section : limits.panel);

// --- the values ------------------------------------------------------------------------------------------------------------

/**
 * The values of a repeat, in the order of the options. `options` is the list of the variable: `[{ value, text }]`.
 * With "All" each option is a value. Else the values are the options that the selection holds. A value that is not an option never shows.
 */
export function orderedValues(options, selection) {
  const seen = new Set();
  const picked = selection.all ? null : new Set(selection.values);
  const out = [];
  for (const o of options) {
    if (seen.has(o.value) || (picked && !picked.has(o.value))) continue;
    seen.add(o.value);
    out.push({ value: o.value, text: o.text });
  }
  return out;
}

/** The result for k values and a limit: `"empty"`, `"ok"` or `"limit"`. The list is never cut to fit. */
export const classify = (k, limit) => (k === 0 ? "empty" : k > limit ? "limit" : "ok");

// --- the messages (9.8c) ------------------------------------------------------------------------------------------------------------

/** The text of a state with no copy. `scope` is `"panel"`, `"section"` or `"dashboard"`. */
export function repeatMessage(state, scope, { k = 0, limit = 0, label = "" } = {}) {
  if (state === "empty") return `No value of ${label} is selected. Nothing to repeat.`;
  if (limit < 1) return "The server gave a repeat limit that is not valid. Nothing repeats.";
  if (scope === "dashboard") return `The repeats of this dashboard have more than ${limit} copies. Select fewer values of ${label}.`;
  return `${k} values of ${label} are selected. A ${scope} repeats ${limit} times at most. Select fewer values.`;
}

/** True when a text names the variable in a placeholder (9.13): `${name}`, `${name:format}` or `$name`. */
export function mentionsVariable(text, name) {
  if (typeof text !== "string" || !text.includes("$") || !/^[A-Za-z0-9_]+$/.test(name)) return false;
  return new RegExp(`\\$\\{${name}(?::[A-Za-z0-9_]+)?\\}|\\$${name}(?![A-Za-z0-9_])`).test(text);
}

// --- the keys (9.8c) -----------------------------------------------------------------------------------------------------------------

/** The key of the copy n of a panel: the id for the first copy, else `<id>-r<n>`. */
export const instanceKey = (id, n) => (n <= 1 ? id : `${id}-r${n}`);
/** The key of the copy n of a section: the index for the first copy, else `<index>-r<n>`. */
export const sectionKey = (index, n) => (n <= 1 ? index : `${index}-r${n}`);
/** The `data-testid` of the copy n of a section (9.29). */
export const sectionTestid = (index, n) => (n <= 1 ? `section-${index}` : `section-${index}-r${n}`);
/** The key that a `data-panel-key` text holds: a number for a first copy, else the text. */
export const keyFromText = (text) => (/^[0-9]+$/.test(text) ? Number(text) : text);

const VIEW_FORM = /^(?:panel-)?([1-9][0-9]*)(?:-r([1-9][0-9]*))?$/;

/**
 * The panel and the copy that a `viewPanel` value names (9.5, 10.2a): `7`, `panel-7`, `7-r2` and `panel-7-r2` give `{ id, n }` with n from 1.
 * The form `<x>$panel-<id>` of Grafana gives `{ id, value }`: <x>, the text before the last `$panel-`, is the value of a copy. Else null.
 * The form `<id>-r1` is not valid: the first copy is the number.
 */
export function parseViewKey(value) {
  const text = String(value ?? "");
  const plain = VIEW_FORM.exec(text);
  if (plain) {
    const id = Number(plain[1]);
    const n = plain[2] === undefined ? 1 : Number(plain[2]);
    return Number.isSafeInteger(id) && Number.isSafeInteger(n) && (plain[2] === undefined || n >= 2) ? { id, n } : null;
  }
  const at = text.lastIndexOf("$panel-");
  const tail = text.slice(at + 7);
  return at > 0 && /^[1-9][0-9]*$/.test(tail) && Number.isSafeInteger(Number(tail)) ? { id: Number(tail), value: text.slice(0, at) } : null;
}

/** The key of a `viewPanel` value: the id, or `<id>-r<n>`. The form of Grafana stays as it is: only the values give its copy (copyKeyOf). Null when not valid. */
export function viewKeyFromParam(value) {
  const parsed = parseViewKey(value);
  return !parsed ? null : parsed.value === undefined ? instanceKey(parsed.id, parsed.n) : String(value);
}

/** The id and the number of a key that the page wrote (the id and the value for the form of Grafana). */
export function splitKey(key) {
  return parseViewKey(String(key)) || { id: Number(key), n: 1 };
}

/** The key of the copy of the panel `id` (or of its section) whose value is `value`, exact on the text (10.2a, rule 1). Null for none, undefined while the values load. */
export function copyKeyOf(plan, id, value) {
  const first = plan.instances.get(id);
  if (first && first.loading) return undefined;
  for (const inst of plan.instances.values()) if (inst.id === id && inst.n !== null && String(inst.value) === value) return inst.key;
  return null;
}

// --- the walk of the repeats and the budget (9.8c) -------------------------------------------------------------------------------------

const byPosition = (a, b) => a.grid.y - b.grid.y || a.grid.x - b.grid.x || a.id - b.id;

/**
 * The result of each repeat of a document. `valuesOf(name)` gives the values (`[{ value, text }]`), null while the options load, or
 * undefined when the variable does not exist or cannot repeat: the state is then `"empty"`, with no query. `labelOf(name)` gives the label. It returns `{ panels: Map(id), sections: Map(index) }`.
 * An entry is `{ kind, variable, direction, maxPerRow, label, state, scope, values, k, limit, message, cost }`. The state is
 * `"ok"`, `"empty"`, `"limit"` or `"loading"`. The walk follows the order of the document, so the result never depends on the scroll.
 */
export function planRepeats(doc, valuesOf, labelOf, limits = DEFAULT_LIMITS) {
  const out = { panels: new Map(), sections: new Map() };
  let budget = limits.dashboard;
  const resolve = (kind, repeat, cost1, extra) => {
    const values = valuesOf(repeat.variable);
    const label = labelOf(repeat.variable);
    const entry = { kind, variable: repeat.variable, label, values: values || [], k: values ? values.length : 0, limit: limitFor(kind, limits), state: "loading", scope: kind, message: "", cost: 0, direction: null, maxPerRow: null, ...extra };
    if (values === null) return entry;
    entry.state = classify(entry.k, entry.limit);
    if (entry.state === "ok") {
      entry.cost = entry.k * cost1;
      if (entry.cost > budget) {
        entry.state = "limit";
        entry.scope = "dashboard";
        entry.limit = limits.dashboard;
        entry.cost = 0;
      } else budget -= entry.cost;
    }
    if (entry.state !== "ok") entry.message = repeatMessage(entry.state, entry.scope, entry);
    return entry;
  };
  doc.sections.forEach((section, si) => {
    if (section.repeat) {
      out.sections.set(si, resolve("section", section.repeat, section.panels.length, {}));
      return;
    }
    for (const panel of [...section.panels].sort(byPosition)) {
      if (!panel.repeat) continue;
      const r = panel.repeat;
      out.panels.set(panel.id, resolve("panel", r, 1, { direction: r.direction, maxPerRow: r.max_per_row === undefined ? null : r.max_per_row }));
    }
  });
  return out;
}

// --- the layout (9.8c) ---------------------------------------------------------------------------------------------------------------

/** The band of a repeat with direction `h`: c columns, q rows, and the height in row units. `m` is `max_per_row`, or null. */
export function bandOf(k, m, h) {
  const c = Math.max(1, Math.min(k, m === null || m === undefined ? DEFAULT_MAX_PER_ROW : m));
  const q = Math.ceil(k / c);
  return { c, q, rows: q * h };
}

/** The extra height E of a repeat, in row units (9.8c). An entry that is not `ok` has none. */
export function extraRows(entry, h) {
  if (!entry || entry.state !== "ok") return 0;
  if (entry.direction === "v") return (entry.k - 1) * h;
  return (bandOf(entry.k, entry.maxPerRow, h).q - 1) * h;
}

// The columns of an item: x to x + w - 1. A panel with `h` has the full width.
const columnsOf = (it) => (it.entry && it.entry.direction === "h" ? [0, GRID_COLUMNS - 1] : [it.grid.x, it.grid.x + it.grid.w - 1]);

/**
 * The move of each item, by the rule of 9.8c. `items` is `[{ id, grid: { x, y, w, h }, entry }]`: a panel or a placed control, with the entry
 * of its repeat or null. It returns a Map from id to the displayed first row unit.
 */
export function displayedRows(items) {
  const order = [...items].sort(byPosition);
  const move = new Map();
  // Only an item with a move or an extra height gives its move to the items below it: the others add 0.
  const givers = [];
  for (const q of order) {
    const [b1, b2] = columnsOf(q);
    let best = 0;
    for (const g of givers) {
      const [a1, a2] = columnsOf(g.item);
      if (g.item.grid.y + g.item.grid.h <= q.grid.y && a1 <= b2 && b1 <= a2) best = Math.max(best, g.total);
    }
    move.set(q.id, best);
    const total = best + extraRows(q.entry, q.grid.h);
    if (total > 0) givers.push({ item: q, total });
  }
  return new Map(order.map((q) => [q.id, q.grid.y + move.get(q.id)]));
}

// --- the plan of a page ----------------------------------------------------------------------------------------------------------------

/**
 * The plan of the page: the copies of each section and the instances (panels and copies of panels) of each copy.
 * `doc` is the document for the grid (the placements are placed panels, var_layout.js). `repeat: false` gives a plan with no repeat.
 * `limits` is `{ panel, section, dashboard }` (limitsOf).
 * An instance is `{ key, id, panel, sectionIndex, sectionKey, copy, n, count, variable, value, text, grid, stored, note, loading, band }`:
 *  - `n` and `count` are the number of the copy and the number of copies (null for a frame that is not a copy);
 *  - `variable`, `value` and `text` are the repeat variable and the value of the copy (null for a panel with no value);
 *  - `grid` is the displayed place `{ x, y, w, h }`, and `stored` is the stored one; `note` is null or `{ state, message }`;
 *  - `band` is the id of the band that holds the copy, or null.
 * It returns `{ entries, sections, instances, order, loading }`.
 */
export function buildPlan(doc, valuesOf, labelOf, { repeat = true, limits = DEFAULT_LIMITS } = {}) {
  const entries = repeat ? planRepeats(doc, valuesOf, labelOf, limits) : { panels: new Map(), sections: new Map() };
  const instances = new Map();
  const sections = doc.sections.map((section, si) => {
    const sectionEntry = entries.sections.get(si) || null;
    const spread = sectionEntry && sectionEntry.state === "ok";
    const note = sectionEntry && (sectionEntry.state === "empty" || sectionEntry.state === "limit") ? { state: `repeat-${sectionEntry.state}`, message: sectionEntry.message } : null;
    const specs = spread ? sectionEntry.values.map((v, i) => ({ copy: i + 1, value: v.value, text: v.text })) : [{ copy: 1, value: null, text: null }];
    const copies = specs.map((spec) => {
      const copy = { n: spec.copy, key: sectionKey(si, spec.copy), testid: sectionTestid(si, spec.copy), value: spec.value, text: spec.text, variable: spread ? sectionEntry.variable : null, note: spec.copy === 1 ? note : null, items: [] };
      if (copy.note) return copy;
      const panelEntry = (panel) => (spread ? null : entries.panels.get(panel.id) || null);
      const rows = displayedRows(section.panels.map((panel) => ({ id: panel.id, grid: panel.grid, entry: panelEntry(panel) })));
      const base = (panel) => ({
        id: panel.id, panel, sectionIndex: si, sectionKey: copy.key, copy: copy.n, n: spread ? copy.n : null, count: spread ? sectionEntry.k : null,
        variable: spread ? sectionEntry.variable : null, value: spec.value, text: spec.text, stored: panel.grid, note: null, loading: !!sectionEntry && sectionEntry.state === "loading", band: null,
      });
      const make = (inst) => {
        instances.set(inst.key, inst);
        return inst;
      };
      const ordered = [...section.panels].sort((a, b) => rows.get(a.id) - rows.get(b.id) || a.grid.x - b.grid.x || a.id - b.id);
      for (const panel of ordered) {
        const entry = panelEntry(panel);
        const y = rows.get(panel.id);
        const grid = { ...panel.grid, y };
        if (!entry) {
          copy.items.push({ kind: "panel", instance: make({ ...base(panel), key: instanceKey(panel.id, spread ? copy.n : 1), grid }) });
          continue;
        }
        const wide = entry.direction === "h" ? { ...grid, x: 0, w: GRID_COLUMNS } : grid;
        if (entry.state !== "ok") {
          const state = entry.state === "loading" ? null : { state: `repeat-${entry.state}`, message: entry.message };
          copy.items.push({ kind: "panel", instance: make({ ...base(panel), key: panel.id, grid: wide, note: state, loading: entry.state === "loading" }) });
          continue;
        }
        const own = (i) => ({ ...base(panel), key: instanceKey(panel.id, i + 1), n: i + 1, count: entry.k, variable: entry.variable, value: entry.values[i].value, text: entry.values[i].text });
        if (entry.direction === "h") {
          const geometry = bandOf(entry.k, entry.maxPerRow, panel.grid.h);
          const band = { kind: "band", id: panel.id, y, h: panel.grid.h, c: geometry.c, q: geometry.q, rows: geometry.rows, entry, instances: [] };
          for (let i = 0; i < entry.k; i++) band.instances.push(make({ ...own(i), grid: { x: 0, y, w: GRID_COLUMNS, h: panel.grid.h }, band: panel.id }));
          copy.items.push(band);
        } else {
          for (let i = 0; i < entry.k; i++) copy.items.push({ kind: "panel", instance: make({ ...own(i), grid: { ...grid, y: y + i * panel.grid.h } }) });
        }
      }
      // The items are in the order of the grid: the displayed y, then x, then the number n (9.8c, 9.26).
      const at = (it) => (it.kind === "band" ? [it.y, 0, it.id, 0] : [it.instance.grid.y, it.instance.grid.x, it.instance.id, it.instance.n || 1]);
      copy.items.sort((a, b) => {
        const [p1, p2] = [at(a), at(b)];
        return p1[0] - p2[0] || p1[1] - p2[1] || p1[2] - p2[2] || p1[3] - p2[3];
      });
      return copy;
    });
    return { index: si, entry: sectionEntry, copies };
  });
  // The order of the scheduler: the sections, the copies of a section, then the panels by displayed y, x and the number n.
  const order = [...instances.values()].filter((i) => i.panel.data).sort((a, b) => a.sectionIndex - b.sectionIndex || a.copy - b.copy || a.grid.y - b.grid.y || a.grid.x - b.grid.x || (a.n || 1) - (b.n || 1) || a.id - b.id);
  const loading = [...entries.panels.values(), ...entries.sections.values()].some((e) => e.state === "loading");
  return { entries, sections, instances, order, loading };
}

/** A text that is the same for two plans of the same structure, for one section: the grid redraws the section when it changes. */
export function sectionSignature(sectionPlan) {
  return JSON.stringify(sectionPlan.copies.map((c) => [c.key, c.value, c.text, c.note && c.note.message, c.items.map((it) => (it.kind === "band" ? ["b", it.id, it.y, it.c, it.instances.map((i) => [i.key, i.value, i.text])] : ["p", it.instance.key, it.instance.value, it.instance.text, it.instance.grid, it.instance.note && it.instance.note.message, it.instance.loading]))]));
}

// --- the request of a copy (9.8c) ---------------------------------------------------------------------------------------------------------

/** The member `variables` of a copy: the member of the page, with the repeat variable fixed to one value (`all` = false). */
export function copyVariables(variables, name, value) {
  return { ...variables, [name]: { all: false, values: [value] } };
}

/** The ids of the panels that repeat, or sit in a section that repeats, with the limit of that repeat: `Map(id -> limit)` (10.2a, rule 6). */
export function repeatLimits(doc, limits = DEFAULT_LIMITS) {
  const out = new Map();
  for (const section of doc.sections) {
    for (const panel of section.panels) {
      if (section.repeat) out.set(panel.id, limits.section);
      else if (panel.repeat) out.set(panel.id, limits.panel);
    }
  }
  return out;
}
