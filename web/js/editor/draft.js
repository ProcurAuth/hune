// Draft logic of the editor (10.2, 10.6, 10.7, 10.8, 10.9). Pure module: no DOM access, no storage access.
// A document is the JSON of section 4. Every function returns new objects and never changes its input.

import { canRepeatBy } from "../repeat.js";
import { PLACED_TYPE, gridDoc, placedId } from "../var_layout.js";
import { bottomOf, normalizeSection, setPosition } from "./layout.js";

export const DRAFT_PREFIX = "hune.draft.v1.";
export const TABLE_ONLY_PROPERTIES = ["width", "align", "cell", "filterable", "hidden", "inspect"];
// The 12 types of the type list of the panel editor (10.3): the 11 that have a query (the last four are 4.33b, 9.19c, 4.33c, 9.19e and 4.33d, 9.19f), and the text panel (4.33a, 9.19b). The logs panel (4.33f, 9.19h) has a query too.
export const PANEL_TYPES = ["table", "timeseries", "stat", "barchart", "piechart", "xychart", "bargauge", "gauge", "state_timeline", "status_history", "heatmap", "histogram", "logs", "geomap", "text"];
// The types that draw the state of each row over time (4.33b), and the types that have a `reduce` member (4.30 to 4.33).
export const TIMELINE_TYPES = ["state_timeline", "status_history"];
export const REDUCE_TYPES = ["stat", "piechart", "bargauge", "gauge"];

// The types that the grid can draw: the 8 above and the placed control of a variable (var_layout.js). A document never has the second one.
export const isDrawable = (type) => PANEL_TYPES.includes(type) || type === PLACED_TYPE;

const clone = (value) => structuredClone(value);

export function draftKey(uid) {
  return `${DRAFT_PREFIX}${uid}`;
}

// --- deep equality of two JSON values, with no dependency on the order of members ------------------

export function sameJson(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b || a === null || b === null || typeof a !== "object") return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  if (Array.isArray(a)) return a.length === b.length && a.every((v, i) => sameJson(v, b[i]));
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => Object.hasOwn(b, k) && sameJson(a[k], b[k]));
}

// --- random values: the caller gives the source of bytes (crypto.getRandomValues) --------------------

// A UUID version 4 in the canonical lower-case form (10.9).
export function makeSaveId(fill) {
  const bytes = new Uint8Array(16);
  fill(bytes);
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export const isSaveId = (text) => /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(String(text));

// A uid of 14 characters of [a-z0-9] with no bias (10.7): a byte of 252 or more is dropped (36 x 7 = 252).
export function makeUid(fill) {
  const alphabet = "abcdefghijklmnopqrstuvwxyz0123456789";
  let out = "";
  const bytes = new Uint8Array(32);
  while (out.length < 14) {
    fill(bytes);
    for (const b of bytes) {
      if (b < 252 && out.length < 14) out += alphabet[b % 36];
    }
  }
  return out;
}

// --- documents ------------------------------------------------------------------------------------------------

// The format version of the document that this page reads and writes (4.42). The server refuses another one.
export const FORMAT_VERSION = 3;

export function smallestDocument(uid, title) {
  return {
    format_version: FORMAT_VERSION,
    uid,
    title,
    description: "",
    tags: [],
    time: { from: "now-6h", to: "now", timezone: "browser", refresh: null },
    cursor_sync: "off",
    links: [],
    variables: [],
    sections: [{ title: null, collapsed: false, panels: [] }],
  };
}

export const defaultThresholds = () => ({ mode: "absolute", steps: [{ value: null, color: "green" }, { value: 80, color: "red" }] });

const LEGEND = (placement = "bottom") => ({ show: true, placement, mode: "list", values: [] });
const REDUCE = () => ({ calc: "last_not_null", fields: null, values: false });

// The "New" column of 9.19.
export function newOptions(type) {
  switch (type) {
    case "table": return { show_header: true, sort: [] };
    case "timeseries":
      return { draw: "line", interpolation: "linear", line_width: 1, fill_opacity: 0, show_points: "auto", point_size: 5, span_nulls: false, stacking: "none", y_scale: "linear", legend: LEGEND(), tooltip: { mode: "single" } };
    case "stat": return { reduce: REDUCE(), color_mode: "value", graph_mode: "area", text_mode: "auto", orientation: "auto" };
    case "barchart":
      return { orientation: "auto", x_field: null, stacking: "none", show_values: "auto", bar_width: 97, group_width: 70, x_tick_rotation: 0, legend: LEGEND(), tooltip: { mode: "single" } };
    case "piechart": return { pie_type: "pie", reduce: REDUCE(), labels: [], legend: LEGEND("right"), tooltip: { mode: "single" } };
    case "xychart": return { x_field: null, y_fields: [], draw: "points", point_size: 5, line_width: 1, legend: LEGEND(), tooltip: { mode: "single" } };
    case "bargauge": return { reduce: REDUCE(), orientation: "horizontal", display_mode: "gradient", show_unfilled: true };
    case "gauge": return { reduce: REDUCE() };
    case "state_timeline":
      return { merge_values: true, show_values: "auto", align_values: "left", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend: LEGEND(), tooltip: { mode: "single", sort: "none" } };
    case "status_history":
      return { show_values: "auto", row_height: 90, col_width: 90, line_width: 1, fill_opacity: 70, show_labels: true, legend: LEGEND(), tooltip: { mode: "single", sort: "none" } };
    case "heatmap":
      return { calculate: false, x_buckets: { mode: "count", value: null }, y_buckets: { mode: "count", value: null, scale: "linear" }, y_axis: "left", y_reverse: false, y_min: null, y_max: null, color: { mode: "scheme", scheme: "oranges", fill: "dark-orange", scale: "exponential", exponent: 0.5, steps: 64, reverse: false, min: null, max: null }, cell_gap: 1, hide_le: 0, hide_ge: null, show_legend: true, tooltip: { mode: "single" } };
    case "histogram":
      return { bucket_count: 30, bucket_size: null, bucket_offset: 0, combine: false, stacking: "none", line_width: 1, fill_opacity: 80, legend: LEGEND(), tooltip: { mode: "single", sort: "none" } };
    // Each option of a logs panel is optional, and a new panel has none (4.33f).
    case "logs": return {};
    // The defaults of Grafana 12.4.2 (9.19i, section 8): the view (0, 0) at zoom 1, the size 5 from 2 to 15, dark-green at 0.4.
    case "geomap":
      return { view: { mode: "fixed", lat: 0, lon: 0, zoom: 1 }, layer: { lat_field: null, lon_field: null, size: 5, size_min: 2, size_max: 15, size_field: null, color: "dark-green", color_field: null, opacity: 40 }, tooltip: "details", show_zoom: true, mouse_wheel_zoom: true };
    case "text": return { mode: "markdown", content: ["# Title", "", "Write text in Markdown."] };
    default: throw new Error(`Unknown panel type ${type}`);
  }
}

export function allPanels(doc) {
  return doc.sections.flatMap((s) => s.panels);
}

export function nextPanelId(doc) {
  return allPanels(doc).reduce((m, p) => Math.max(m, p.id), 0) + 1;
}

export function locatePanel(doc, id) {
  for (let s = 0; s < doc.sections.length; s++) {
    const p = doc.sections[s].panels.findIndex((x) => x.id === id);
    if (p >= 0) return { section: s, index: p, panel: doc.sections[s].panels[p] };
  }
  return null;
}

// The new panel of 10.6.
export function newPanel(doc, sectionIndex, source) {
  return {
    id: nextPanelId(doc),
    type: "table",
    title: "New panel",
    description: "",
    transparent: false,
    grid: { x: 0, y: bottomOf(doc.sections[sectionIndex].panels), w: 12, h: 8 },
    data: { query: { source, format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null } },
    transforms: [],
    fields: { defaults: { thresholds: defaultThresholds() }, overrides: [] },
    options: newOptions("table"),
    links: [],
  };
}

// The new text panel of 10.6 (4.33a): no `data`, no `transforms`, no `fields`.
export function newTextPanel(doc, sectionIndex) {
  return {
    id: nextPanelId(doc),
    type: "text",
    title: "New text",
    description: "",
    transparent: false,
    grid: { x: 0, y: bottomOf(doc.sections[sectionIndex].panels), w: 12, h: 8 },
    options: newOptions("text"),
    links: [],
  };
}

// Adds a panel to a section and normalizes the layout of the section.
export function addPanel(doc, sectionIndex, panel) {
  const next = clone(doc);
  const section = next.sections[sectionIndex];
  section.panels = normalizeSection([...section.panels, panel]);
  return next;
}

// --- the placement of a variable (4.7a, 10.3b, 10.6) ----------------------------------------------------------------------------

/** The bottom of a section for the grid: the first free row under the panels and the placed controls (10.5). */
export function sectionBottom(doc, sectionIndex) {
  return bottomOf(gridDoc(doc).sections[sectionIndex].panels);
}

/** The placement of a variable that goes to the grid for the first time (10.6): the bottom of the section, 6 columns, 2 rows. */
export function defaultPlacement(doc, sectionIndex) {
  return { section: sectionIndex, x: 0, y: sectionBottom(doc, sectionIndex), w: 6, h: 2 };
}

/** The document with the placement of the variable `name` set, or removed when `placement` is null (the variable is back in the top bar). */
export function setPlacement(doc, name, placement) {
  const next = clone(doc);
  const variable = next.variables.find((v) => v.name === name);
  if (!variable) return doc;
  delete variable.placement;
  if (placement) variable.placement = placement;
  return next;
}

/** The text that says why a variable cannot go to the grid, or "" (a variable with `hide = "variable"` has no control, 4.7a). */
export const placementRefusal = (variable) => (variable.hide === "variable" ? "A variable with Hide set to Show nothing has no control to place." : "");

/** The size of the title of a placement (4.7a): 1 to 256 characters. The editor only uses it (9.30). */
export const PLACEMENT_TITLE_MAX = 256;

/** The error of the title of a placement (4.7a), or "". */
export const placementTitleError = (text) => ([...text].length > PLACEMENT_TITLE_MAX ? `The title has ${PLACEMENT_TITLE_MAX} characters at most.` : "");

/** A new grid for the placement of one variable (the fields x, y, w, h of the form): the section is normalized with the panels. */
export function movePlacement(doc, name, grid) {
  const at = doc.variables.findIndex((v) => v.name === name);
  const variable = doc.variables[at];
  if (!variable || !variable.placement) return doc;
  const id = placedId(at);
  const panels = gridDoc(doc).sections[variable.placement.section].panels.map((p) => (p.id === id ? { ...p, grid: { ...p.grid, ...grid } } : p));
  return splitLayout(doc, variable.placement.section, normalizeSection(panels));
}

/** A placement to another section: at the bottom of that section, with the same size and title. */
export function placeInSection(doc, name, sectionIndex) {
  const variable = doc.variables.find((v) => v.name === name);
  if (!variable || !variable.placement) return doc;
  const { w, h, title } = variable.placement;
  const without = setPlacement(doc, name, null);
  const placement = { ...defaultPlacement(without, sectionIndex), w, h };
  if (title !== undefined) placement.title = title;
  return setPlacement(without, name, placement);
}

// --- the repeat of a panel and of a section (4.13a, 10.3, 10.6) ------------------------------------------------------------------------

/** The variables that a repeat can name: the kind `query` or `custom`, with `multi` = true (4.13a, rule 1). */
export const repeatableVariables = (doc) => doc.variables.filter(canRepeatBy);

/** The text that says why a panel cannot repeat, or "" (4.13a, rules 4, 7 and 8). */
export function panelRepeatRefusal(doc, id) {
  const at = locatePanel(doc, id);
  if (!at) return "";
  if (at.panel.type === "text") return "A text panel has no query, so it cannot repeat.";
  if (at.panel.data && at.panel.data.reuse) return "A panel that reuses another panel cannot repeat.";
  if (at.panel.data && at.panel.data.extra_queries) return "A panel with several queries cannot repeat.";
  if (doc.sections[at.section].repeat) return "This section repeats. A repeat has one level only.";
  if (reusersOf(doc, id).length) return "Another panel reuses this panel. A panel that is the source of a reuse cannot repeat.";
  return "";
}

/**
 * The document with the `repeat` of a panel set, or removed when `repeat` is null. `repeat` is `{ variable, direction, max_per_row? }`.
 * The direction `h` gives the panel the full width and a place of its own: the section is normalized (4.13a, rule 5). `max_per_row` is written
 * only for `h`, and only when it is given: the document never stores the default 4.
 */
export function setPanelRepeat(doc, id, repeat) {
  const at = locatePanel(doc, id);
  if (!at) return doc;
  const next = updatePanel(doc, id, (panel) => {
    delete panel.repeat;
    if (repeat) {
      const out = { variable: repeat.variable, direction: repeat.direction };
      if (repeat.direction === "h" && repeat.max_per_row !== undefined && repeat.max_per_row !== null) out.max_per_row = repeat.max_per_row;
      panel.repeat = out;
    }
    return panel;
  });
  if (!repeat || repeat.direction !== "h") return next;
  const section = next.sections[at.section];
  section.panels = setPosition(section.panels, id, { x: 0, w: 24 });
  return next;
}

/** The index of the last section that does not repeat, or -1. A control of a variable goes there by default (4.7a, rule 11). */
export function lastPlainSection(doc) {
  for (let i = doc.sections.length - 1; i >= 0; i--) if (!doc.sections[i].repeat) return i;
  return -1;
}

/** The text that says why a section cannot repeat, or "" (4.13a, rule 7; 4.7a, rule 11). */
export function sectionRepeatRefusal(doc, index) {
  const section = doc.sections[index];
  if (!section) return "";
  if (section.title === null) return "The first section has no header, so its copies could not be told apart.";
  if (doc.variables.some((v) => v.placement && v.placement.section === index)) return "A control of a variable is placed in this section.";
  if (section.panels.some((p) => p.repeat || (p.data && (p.data.reuse || p.data.extra_queries)))) return "A panel of this section repeats, reuses another panel or has several queries.";
  return "";
}

/** The document with the `repeat` of a section set to a variable, or removed when `variable` is null. */
export function setSectionRepeat(doc, index, variable) {
  const next = clone(doc);
  const section = next.sections[index];
  if (!section) return doc;
  delete section.repeat;
  if (variable) section.repeat = { variable };
  return next;
}

/** The panels and the sections that repeat by a variable: `{ panels: [panel], sections: [{ index, section }] }`. */
export function repeatUsers(doc, name) {
  const panels = allPanels(doc).filter((p) => p.repeat && p.repeat.variable === name);
  const sections = doc.sections.map((section, index) => ({ section, index })).filter(({ section }) => section.repeat && section.repeat.variable === name);
  return { panels, sections };
}

/** The document after a rename of a variable: each repeat that names it names the new name (10.3b). */
export function renameRepeatVariable(doc, from, to) {
  if (from === to || (repeatUsers(doc, from).panels.length === 0 && repeatUsers(doc, from).sections.length === 0)) return doc;
  const next = clone(doc);
  for (const section of next.sections) {
    if (section.repeat && section.repeat.variable === from) section.repeat.variable = to;
    for (const panel of section.panels) if (panel.repeat && panel.repeat.variable === from) panel.repeat.variable = to;
  }
  return next;
}

/** The document without the repeats that name a variable: the variable is deleted (10.3b, 4.13a rule 9). */
export function removeRepeatsOf(doc, name) {
  const users = repeatUsers(doc, name);
  if (!users.panels.length && !users.sections.length) return doc;
  const next = clone(doc);
  for (const section of next.sections) {
    if (section.repeat && section.repeat.variable === name) delete section.repeat;
    for (const panel of section.panels) if (panel.repeat && panel.repeat.variable === name) delete panel.repeat;
  }
  return next;
}

/** True when the panel can be the source of a reuse: it has a query, does not repeat and is not in a section that repeats (4.13a, rule 8). */
export const canBeReuseSource = (doc, panel) => {
  const at = locatePanel(doc, panel.id);
  return !!(panel.data && panel.data.query) && !panel.repeat && !(at && doc.sections[at.section].repeat);
};

/** True when a reuse panel of the type `type` can name `source` as its source: a bar chart and an XY chart draw one frame (4.15, rule 6). */
export const reuseFits = (type, source) => !((type === "barchart" || type === "xychart" || type === "geomap") && source.data && source.data.extra_queries);

export function duplicatePanel(doc, id) {
  const at = locatePanel(doc, id);
  if (!at) return { doc, panel: null };
  const copy = clone(at.panel);
  copy.id = nextPanelId(doc);
  copy.grid.y = at.panel.grid.y + at.panel.grid.h;
  return { doc: addPanel(doc, at.section, copy), panel: copy };
}

// The panels that reuse the panel `id` (10.6).
export function reusersOf(doc, id) {
  return allPanels(doc).filter((p) => p.data && p.data.reuse && p.data.reuse.panel === id);
}

// The annotations that name a panel of `ids` in `panels` (10.3b, 10.6): [{ name, only }]. `only` is true when no other id stays in the list.
export function annotationsNaming(doc, ids) {
  const gone = new Set(ids);
  return (doc.annotations || []).filter((a) => a.panels && a.panels.some((id) => gone.has(id))).map((a) => ({ name: a.name, only: a.panels.every((id) => gone.has(id)) }));
}

// Takes the ids `ids` out of the `panels` of the annotations of `doc`, in place (10.3b). An empty list goes: `panels` leaves and `enabled` is false.
// It returns the names of the annotations that this turned off.
export function dropPanelIds(doc, ids) {
  const gone = new Set(ids);
  const off = [];
  for (const a of doc.annotations || []) {
    if (!a.panels || !a.panels.some((id) => gone.has(id))) continue;
    a.panels = a.panels.filter((id) => !gone.has(id));
    if (!a.panels.length) {
      delete a.panels;
      a.enabled = false;
      off.push(a.name);
    }
  }
  return off;
}

// Deletes a panel. It returns { doc, off } or { refused: [titles] } when other panels reuse it. `off` lists the annotations that the delete turns off.
export function deletePanel(doc, id) {
  const users = reusersOf(doc, id);
  if (users.length) return { refused: users.map((p) => p.title || `panel ${p.id}`) };
  const at = locatePanel(doc, id);
  if (!at) return { doc, off: [] };
  const next = clone(doc);
  const section = next.sections[at.section];
  section.panels = normalizeSection(section.panels.filter((p) => p.id !== id));
  return { doc: next, off: dropPanelIds(next, [id]) };
}

export function addSection(doc) {
  const next = clone(doc);
  next.sections.push({ title: "New section", collapsed: false, panels: [] });
  return next;
}

// The title rule of 4.12: 1 to 256 characters. It returns { doc } or { error }.
export function renameSection(doc, index, title) {
  if (title.length < 1) return { error: "The title cannot be empty." };
  if ([...title].length > 256) return { error: "The title has 256 characters at most." };
  const next = clone(doc);
  next.sections[index].title = title;
  return { doc: next };
}

// Deletes a section with its panels (10.6). It returns { doc }, { refused } (a panel of another section reuses a panel
// of this one) or { error } (a document keeps at least one section).
export function deleteSection(doc, index) {
  if (doc.sections.length <= 1) return { error: "A dashboard needs at least one section." };
  const ids = new Set(doc.sections[index].panels.map((p) => p.id));
  const users = [];
  doc.sections.forEach((section, i) => {
    if (i === index) return;
    for (const p of section.panels) if (p.data && p.data.reuse && ids.has(p.data.reuse.panel)) users.push(p.title || `panel ${p.id}`);
  });
  if (users.length) return { refused: users };
  const next = clone(doc);
  const off = dropPanelIds(next, doc.sections[index].panels.map((p) => p.id));
  next.sections.splice(index, 1);
  // The placements in the section go with it (the variables are back in the top bar). The later sections have an index less.
  for (const v of next.variables) {
    if (!v.placement) continue;
    if (v.placement.section === index) delete v.placement;
    else if (v.placement.section > index) v.placement.section -= 1;
  }
  return { doc: next, off };
}

// True when the section can move by `dir` (-1 up, 1 down).
export const canMoveSection = (doc, index, dir) => index + dir >= 0 && index + dir < doc.sections.length;

// A placement names its section by index (4.7a): a move of a section changes the index of the placements in it and in its neighbour.
export function moveSection(doc, index, dir) {
  if (!canMoveSection(doc, index, dir)) return null;
  const next = clone(doc);
  const [s] = next.sections.splice(index, 1);
  next.sections.splice(index + dir, 0, s);
  for (const v of next.variables) {
    if (!v.placement) continue;
    if (v.placement.section === index) v.placement.section = index + dir;
    else if (v.placement.section === index + dir) v.placement.section = index;
  }
  return next;
}

export function setCollapsed(doc, index, collapsed) {
  const next = clone(doc);
  next.sections[index].collapsed = collapsed;
  return next;
}

// Replaces one panel by the result of fn(copy of the panel). Layout is not normalized here.
export function updatePanel(doc, id, fn) {
  const at = locatePanel(doc, id);
  if (!at) return doc;
  const next = clone(doc);
  const copy = fn(clone(at.panel));
  next.sections[at.section].panels[at.index] = copy;
  return next;
}

// A new type keeps the shared members and takes the "New" options (10.3). The table-only properties leave.
// A text panel has no query, no transformations and no field options (4.33a): a change to `text` removes them, and a change from `text` gives
// the panel the query of a new panel on `source` (10.6). The page asks a question first (10.3).
export function changeType(panel, type, source = "") {
  const out = clone(panel);
  const was = out.type;
  out.type = type;
  out.options = newOptions(type);
  // A bar chart and an XY chart draw one frame, so the extra queries leave (4.15a, rule 7).
  if ((type === "barchart" || type === "xychart" || type === "logs" || type === "geomap") && out.data && out.data.extra_queries) delete out.data.extra_queries;
  // The format `logs` is for a logs panel only: a logs panel takes it, and another type goes back to `table` (4.33f).
  if (out.data && out.data.query) {
    if (type === "logs") out.data.query.format = "logs";
    else if (out.data.query.format === "logs") out.data.query.format = "table";
  }
  if (type === "text") {
    for (const key of ["data", "transforms", "fields", "repeat"]) delete out[key];
    return out;
  }
  if (was === "text") {
    const fresh = newPanel({ sections: [{ panels: [] }] }, 0, source);
    if (type === "logs") fresh.data.query.format = "logs";
    return { ...out, data: fresh.data, transforms: fresh.transforms, fields: fresh.fields };
  }
  if (type !== "table" && out.fields) {
    const strip = (set) => {
      for (const key of TABLE_ONLY_PROPERTIES) delete set[key];
    };
    if (out.fields.defaults) strip(out.fields.defaults);
    out.fields.overrides = (out.fields.overrides || []).filter((o) => {
      strip(o.set);
      return Object.keys(o.set).length > 0;
    });
  }
  return out;
}

// --- the draft record of 10.8 ---------------------------------------------------------------------------------------

export function makeRecord({ baseEtag, baseVersion, subject, folder, document, now }) {
  return { base_etag: baseEtag, base_version: baseVersion, subject, folder, updated_at_ms: now, document: clone(document), json_text: null, save_id: null, message: "" };
}

// Reads the text of a stored draft. Returns { draft } or { discard: "invalid" | "format" | "uid" | "subject" } (10.8).
export function readRecord(text, uid, subject) {
  let record;
  try {
    record = JSON.parse(text);
  } catch (error) {
    return { discard: "invalid" };
  }
  const ok = record && typeof record === "object" && typeof record.base_etag === "string" && record.document && typeof record.document === "object" && Array.isArray(record.document.sections);
  if (!ok) return { discard: "invalid" };
  // A draft of an older format cannot be saved: the server refuses it (4.42, item 2). The page removes it with its own notice (10.8).
  if (record.document.format_version !== FORMAT_VERSION) return { discard: "format" };
  if (record.document.uid !== uid) return { discard: "uid" };
  if (record.subject !== subject) return { discard: "subject" };
  return { draft: { json_text: null, save_id: null, message: "", base_version: 0, ...record } };
}

// The notice text of a draft that is found at load (10.8).
export function resumeNotice(record, currentEtag, formatTime) {
  const parts = [`You have changes that are not saved, from ${formatTime(record.updated_at_ms)}.`];
  if (record.base_etag !== currentEtag) parts.push("The dashboard changed since then.");
  return parts.join(" ");
}

// The draft differs from the base: the document or an unapplied JSON text (10.2, 10.4).
export function draftDiffers(record, baseDocument) {
  return record.json_text !== null && record.json_text !== undefined ? true : !sameJson(record.document, baseDocument);
}

/** True for a placed panel (a panel that stands for the placement of a variable). The editor only uses it (9.30). */
export const isPlacedPanel = (panel) => !!panel && panel.type === PLACED_TYPE;

/** The index of the variable that a placed panel stands for. The editor only uses it (9.30). */
export const variableIndexOf = (panel) => -panel.id - 1;

/**
 * The editor only uses this function, so it is in the editor and not in the page (9.30).
 * The document after a new layout of one section (the drag, the resize and the keys of 10.16). `panels` holds the real panels and the placed
 * panels of the section, each with its new grid. The real panels go to the section. A placed panel changes x, y, w and h of the placement.
 */
export function splitLayout(doc, sectionIndex, panels) {
  const next = structuredClone(doc);
  next.sections[sectionIndex].panels = panels.filter((p) => !isPlacedPanel(p));
  for (const p of panels) {
    if (!isPlacedPanel(p)) continue;
    const variable = next.variables[variableIndexOf(p)];
    if (variable && variable.placement) variable.placement = { ...variable.placement, x: p.grid.x, y: p.grid.y, w: p.grid.w, h: p.grid.h };
  }
  return next;
}
