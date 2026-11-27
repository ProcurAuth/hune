// The loading preview of the panel type `table` (9.11).
// The rules of this family are in the council file `table.md` (scratchpad/council). The loaded panel is drawn by web/js/panels/table.js, web/js/table_model.js.
// The preview uses the JSON document only. It never reads a query. It draws a shape only when every loaded table that the document permits has the
// same shape at the same place. Where the document fixes only a lower bound, the preview draws the sure part and nothing more.
//
// The parts (data-skel-part) and their class:
//   plot     the view of the table (`.vtable__scroll`).       exact when the number of frames is known, else most
//   warning  the warning line of an invalid override pattern.  exact (the same element, with a transparent text)
//   frames   the select bar of a table with several frames.    exact (`label.panel__frames`)
//   select   the select in the bar.                            exact
//   head     the header row (`thead`).                         extent: left, top and height exact, right least
//   row      the first body row (the slot 0).                  extent
//   foot     the footer cells.                                 extent
//   head-label the label of the header of column 0 (a bar).    exact
//   mark     the sort mark of column 0.                        exact
//   filter   the filter button of column 0.                    exact
//   foot-name a name of the footer of column 0 (a bar).        exact
//   track    the track of a gauge cell of column 0.            extent
//   cell     a cell of an LCD gauge of column 0.               exact
// The class `extent` is for a shape whose right edge is a lower bound (the table is at least as wide): the harness checks left, top and height
// within 0.5 px and the width as a least bound.

import { BAR_HEIGHT, EXACT, LEAST, MOST, PARTS, barNode, boundHolds, exact, least, neutral, node, partAttrs, regionNode, resultOf, textWidth } from "./common.js";
import { parseGrafanaPattern } from "../fields.js";
import { visibleQueries } from "../queries.js";
import { footerName } from "../table_model.js";

export const EXTENT = "extent";
// The parts that this family adds to the shared names.
export const TABLE_PARTS = Object.freeze({ ...PARTS, FRAMES: "frames", SELECT: "select", FOOT: "foot", MARK: "mark", FILTER: "filter", FOOT_NAME: "foot-name", HEAD_LABEL: "head-label", CELL: "cell" });

// The sizes of the loaded table (web/css/panel-table.css, web/js/panels/table.js). The tokens that the environment gives are read from the CSS.
export const MIN_COL = 150; // the least width of an automatic column (table.js MIN_COL)
export const MARK_PX = 18; // the sort mark (panel-table.css `.dataTable__sort::after`)
export const GROUP_H = 22; // the group of the label and the sort mark
export const FILTER_GAP = 4; // the margin before the filter button
export const FOOT_LINE = 22; // one line of the footer
export const FOOT_BASE = 13; // the rule above, and 6 px of padding above and below the lines
export const GAUGE_H = 22; // the gauge of a cell
export const LCD_CELL = 12; // an LCD cell and its gap (table.js GAUGE_CELL)

// The values of the environment when the page cannot read them (a test gives its own). They are the values of a fine pointer and of overlay bars.
export const DEFAULT_ENV = Object.freeze({ coarse: false, sb: 0, head: 34, row: 36, pad: 6, filterW: 16, filterH: 22, framesH: 32, fsMd: 14, fsXs: 12, fwMedium: 500, footLs: 0 });

// ---- The environment --------------------------------------------------------------------------------------------------------------------
// The environment is the set of platform facts that are not data: the pointer, the thickness of a thin scroll bar, the tokens of the table and the
// height of the select bar. A probe in the page measures them with the real classes of the loaded table. A test gives `ctx.env`.
export function environment(ctx) {
  if (ctx && ctx.env) return { ...DEFAULT_ENV, ...ctx.env };
  if (typeof document === "undefined" || !document.body) return null;
  const probe = document.createElement("div");
  probe.className = "panel__body";
  for (const [name, value] of [["position", "absolute"], ["visibility", "hidden"], ["pointer-events", "none"], ["left", "0px"], ["top", "0px"], ["width", "300px"], ["height", "300px"]]) probe.style.setProperty(name, value);
  const scroll = document.createElement("div");
  scroll.className = "vtable__scroll";
  const tall = document.createElement("div");
  tall.style.setProperty("height", "2000px");
  scroll.append(tall);
  const filter = document.createElement("button");
  filter.className = "vtable__filter";
  filter.style.setProperty("position", "absolute");
  const content = document.createElement("div");
  content.className = "panel__content";
  const frames = document.createElement("label");
  frames.className = "panel__frames";
  const select = document.createElement("select");
  select.className = "input";
  select.append(new Option("A"));
  frames.append(document.createElement("span"), select);
  const footName = document.createElement("span");
  footName.className = "vtable__foot-name";
  scroll.append(footName);
  content.append(frames);
  probe.append(scroll, filter, content);
  document.body.append(probe);
  try {
    const style = getComputedStyle(scroll);
    const px = (v, fallback) => (Number.isFinite(parseFloat(v)) ? parseFloat(v) : fallback);
    const label = getComputedStyle(frames);
    return {
      coarse: typeof matchMedia === "function" && matchMedia("(pointer: coarse)").matches,
      sb: scroll.offsetWidth - scroll.clientWidth,
      head: px(style.getPropertyValue("--vt-head"), DEFAULT_ENV.head),
      row: px(style.getPropertyValue("--vt-row"), DEFAULT_ENV.row),
      pad: px(getComputedStyle(scroll).getPropertyValue("--vt-pad"), DEFAULT_ENV.pad),
      filterW: filter.offsetWidth || DEFAULT_ENV.filterW,
      filterH: filter.offsetHeight || DEFAULT_ENV.filterH,
      framesH: frames.offsetHeight || DEFAULT_ENV.framesH,
      fsMd: px(getComputedStyle(probe).getPropertyValue("--fs-md"), DEFAULT_ENV.fsMd),
      fsXs: px(label.getPropertyValue("--fs-xs"), DEFAULT_ENV.fsXs),
      fwMedium: px(style.getPropertyValue("--fw-medium"), DEFAULT_ENV.fwMedium),
      footLs: px(getComputedStyle(footName).letterSpacing, 0),
    };
  } finally {
    probe.remove();
  }
}

// ---- Reading the panel ------------------------------------------------------------------------------------------------------------------
export const obj = (v) => (v && typeof v === "object" && !Array.isArray(v) ? v : {});
const list = (v) => (Array.isArray(v) ? v : []);

/** The panel `id` of the document, or null. */
export function findPanel(doc, id) {
  for (const section of list(obj(doc).sections)) for (const p of list(obj(section).panels)) if (p && p.id === id) return p;
  return null;
}

/** The members of the panel that the rules read, with their defaults. */
export function readPanel(panel) {
  const options = obj(panel.options);
  const fields = obj(panel.fields);
  const defaults = obj(fields.defaults);
  const overrides = list(fields.overrides).map((o) => ({ match: obj(obj(o).match), set: obj(obj(o).set) }));
  return {
    showHeader: options.show_header !== false,
    wrap: options.wrap === true,
    sort: list(options.sort),
    defaults,
    overrides,
    transforms: list(panel.transforms),
    data: obj(panel.data),
  };
}

/** The warning texts of the override patterns that the page cannot compile, once for each distinct value (fields.js computeFieldConfig). */
export function warningsOf(p) {
  const out = [];
  for (const o of p.overrides) {
    if (o.match.by === "regex" && parseGrafanaPattern(o.match.value) === null) {
      const text = `The override pattern ${o.match.value} is not a valid regular expression.`;
      if (!out.includes(text)) out.push(text);
    }
  }
  return out;
}

// ---- The walks over the transforms (table.md section 1) ----------------------------------------------------------------------------------
/**
 * The chain of the transforms that the panel applies: the transforms of the source panel first, only when `data.reuse.with_transforms` is true.
 * Gives { seen, owner, chain }. `seen` is false when the source panel is not in the document.
 */
export function chainOf(panel, p, doc) {
  const reuse = obj(p.data).reuse;
  if (!reuse) return { seen: true, owner: panel, chain: p.transforms };
  const source = findPanel(doc, reuse.panel);
  if (!source) return { seen: false, owner: null, chain: p.transforms };
  return { seen: true, owner: source, chain: [...(reuse.with_transforms === true ? list(source.transforms) : []), ...p.transforms] };
}

/**
 * The walk over the chain. It gives F (the number of frames, or null when unknown), B (an upper bound of the rows of a frame, or null when
 * unknown) and the refs of the frames (the options of the select, or null).
 * `empty` is true when the owner has no visible query: the panel shows "No data" and sends no request.
 */
export function walkChain(panel, p, doc) {
  const { seen, owner, chain } = chainOf(panel, p, doc);
  if (!seen) return { seen: false, empty: false, F: null, B: null, refs: null };
  const visible = visibleQueries(owner);
  if (!visible.length) return { seen: true, empty: true, F: 0, B: null, refs: [] };
  let F = visible.length;
  let B = null;
  let refs = visible.map((q) => q.ref);
  for (const t of chain) {
    const kind = obj(t).kind;
    if (kind === "limit") {
      const count = Number(t.count);
      if (Number.isFinite(count)) B = B === null ? count : Math.min(B, count);
    } else if (kind === "reduce") {
      B = t.mode === "reduce_fields" ? 1 : null;
    } else if (kind === "merge") {
      if (F !== 1) {
        B = F !== null && B !== null ? F * B : null;
        F = null;
        refs = null;
      }
    } else if (kind === "join_by_field") {
      if (F !== 1) B = F !== null && B !== null ? (t.mode === "inner" ? B ** F : F * B) : null;
      F = 1;
      refs = null;
    } else if (kind === "partition_by_values") {
      F = null;
      refs = null;
    }
  }
  return { seen: true, empty: false, F, B, refs: F !== null && F >= 2 ? refs : null };
}

// ---- The members that decide the columns ------------------------------------------------------------------------------------------------
const setsAny = (p, name, test) => p.overrides.some((o) => name in o.set && test(o.set[name]));

/** C1: at least one shown column is certain (no `hidden: true` in the defaults or in an override). */
export const shownColumn = (p) => p.defaults.hidden !== true && !setsAny(p, "hidden", (v) => v === true);

/**
 * The header height H: { shown, H }. `H` is null when the document does not decide it. A fine pointer gives 34. A coarse pointer gives a filter
 * button of the touch size, so H depends on `filterable`.
 */
export function headerOf(p, env) {
  if (!p.showHeader) return { shown: false, H: 0 };
  if (!env.coarse) return { shown: true, H: env.head };
  const all = p.defaults.filterable === true && !setsAny(p, "filterable", (v) => v === false);
  const none = p.defaults.filterable !== true && !setsAny(p, "filterable", (v) => v === true);
  if (all) return { shown: true, H: Math.max(env.head, env.filterH + env.pad + 1) };
  if (none) return { shown: true, H: env.head };
  return { shown: true, H: null };
}

/** The widths: the least width m that column 0 can have, or null when no member sets a width (every column is automatic). */
export function leastWidth(p) {
  const widths = [];
  if (p.defaults.width !== undefined) widths.push(Number(p.defaults.width));
  for (const o of p.overrides) if ("width" in o.set) widths.push(Number(o.set.width));
  if (!widths.length) return { m: MIN_COL, any: false };
  if (p.defaults.width === undefined) widths.push(MIN_COL);
  return { m: Math.min(...widths), any: true };
}

/** The reducers arrays of the document: { has, min, max, same }. `same` is the common length when all arrays have the same length, else null. */
export function footerArrays(p) {
  const arrays = [];
  const add = (f) => {
    if (f && typeof f === "object") arrays.push(list(f.reducers).length);
  };
  add(p.defaults.footer);
  for (const o of p.overrides) if ("footer" in o.set) add(o.set.footer);
  const has = p.defaults.footer !== undefined || p.overrides.some((o) => "footer" in o.set);
  if (!arrays.length) return { has, min: 0, max: 0, same: null };
  return { has, min: Math.min(...arrays), max: Math.max(...arrays), same: arrays.every((n) => n === arrays[0]) ? arrays[0] : null };
}

const textOk = (s) => typeof s === "string" && s !== "" && !s.includes("$") && !s.includes("[[");

// ---- The build --------------------------------------------------------------------------------------------------------------------------

// The band of a shape: a box at (0, y) with the extent X of the table (the CSS gives its width from --m and --sb).
const band = (cls, y, h, part, partClass, children = []) => node(`skel__tband ${cls}`.trim(), { "--y": `${y}px`, "--h": `${h}px` }, children, "div", part ? partAttrs(part, partClass) : {});

/**
 * The preview of a table (table.md). Gives { elements, bounds, notes }. `panel` is the panel of the document, `box` is { w, h } (the content box of
 * the body), `doc` the document, `ctx` is { rt, page, env } (`env` is for the tests: the pointer, the scroll bar and the tokens).
 */
export function build(panel, box, doc, ctx) {
  const env = environment(ctx);
  if (!env || !panel || !box) return neutral("no environment");
  const p = readPanel(panel);
  const walk = walkChain(panel, p, doc);
  if (walk.empty) return neutral("no visible query");
  if (walk.B === 0) return neutral("no data proven");
  const notes = [];
  const bounds = {};
  const elements = [];

  // E2 and E3: the rows above and under the view. The view takes the rest, as in the loaded flex column.
  const warnings = warningsOf(p);
  const frameBar = walk.F !== null && walk.F >= 2 && walk.refs !== null;
  if (warnings.length) {
    notes.push("warning");
    const icon = node("icon", {}, [], "svg", { width: "14", height: "14", viewBox: "0 0 24 24", "aria-hidden": "true", focusable: "false" });
    const text = node("", {}, [], "span", {});
    text.x = ` ${warnings.join(" ")}`;
    const warn = node("panel__warning skel__twarn", {}, [icon, text], "p", partAttrs(PARTS.WARNING, EXACT));
    elements.push(warn);
  }
  const view = node("skel__tview", {}, [], "div", partAttrs(PARTS.PLOT, walk.F === null ? MOST : EXACT));
  elements.push(view);
  if (frameBar) {
    notes.push("frames");
    const q = node("skel__tq", {}, [], "span", {});
    q.x = "Query";
    const select = node("input skel__tselect", {}, walk.refs.map((r) => Object.assign(node("", {}, [], "option", {}), { x: r })), "select", { ...partAttrs(TABLE_PARTS.SELECT, EXACT), tabindex: "-1", inert: "" });
    elements.push(node("panel__frames skel__tframes", {}, [q, select], "label", partAttrs(TABLE_PARTS.FRAMES, EXACT)));
  } else if (walk.F === null) notes.push("frames unknown");

  // The view height as a range: exact when no warning shows (its height wraps with the text) and the number of frames is known.
  const frameH = frameBar ? env.framesH : 0;
  const vhHi = box.h - frameH;
  const vhLo = warnings.length || walk.F === null ? -Infinity : box.h - frameH;
  bounds.view = { hi: vhHi, lo: vhLo };

  // C1: no shape in the view without a shown column.
  if (!shownColumn(p)) {
    notes.push("no shown column is certain");
    return resultOf(elements, bounds, notes);
  }
  const { shown, H } = headerOf(p, env);
  if (H === null) {
    notes.push("header height is data");
    return resultOf(elements, bounds, notes);
  }
  bounds.H = exact(H);
  const { m, any } = leastWidth(p);
  const foot = footerArrays(p);
  const footerCertain = foot.has && p.defaults.footer && list(p.defaults.footer.reducers).length >= 1 && foot.min >= 1;
  const L = footerCertain && foot.same !== null ? foot.same : null;
  const fhOf = (n) => env.pad + env.pad + FOOT_LINE * n + 1;
  const Fh = L === null ? null : fhOf(L);
  const FhMin = footerCertain ? fhOf(L !== null ? L : foot.min) : 0;
  const FhMax = foot.has ? fhOf(foot.max) : 0;

  // The overflow proofs (table.md section 1).
  const rowPx = env.row;
  const noOverflow = !p.wrap && walk.B !== null && vhLo !== -Infinity && H + rowPx * walk.B + FhMax <= vhLo - env.sb;
  const overflow = H + rowPx + FhMin > vhHi;
  bounds.noOverflow = noOverflow;
  bounds.overflow = overflow;

  // E6: the extent of the rules, the rows and the footer. The width is min(m, view - sb) when a width member exists, else view - sb.
  const sbEff = env.sb === 0 || noOverflow ? 0 : env.sb;
  const xVars = { "--sb": `${sbEff}px` };
  if (any) xVars["--m"] = `${m}px`;
  bounds.extentLeast = least(any ? Math.min(m, box.w - sbEff) : box.w - sbEff);
  const tall = (y, h, cls, part, kids = []) => {
    const b = band(cls, y, h, part, EXTENT, kids);
    Object.assign(b.v, xVars);
    return b;
  };

  // E5: the header row and its rule.
  if (shown) {
    const kids = [node("skel__tline", { "--y": `${H - 1}px` }, [], "div", {})];
    headerGroup(p, env, H, m, any, kids, bounds, notes);
    view.k.push(tall(0, H, "", PARTS.HEAD, kids));
  }

  // E7: the slot 0, when the row height is exact. The gauge of column 0 does not need the row height: with `wrap` it has a holder with no part.
  const slotCertain = !p.wrap;
  const kids = slotCertain ? [node("skel__tline", { "--y": `${rowPx - 1}px` }, [], "div", {})] : [];
  gaugeTrack(p, env, p.wrap, m, any, kids, bounds, notes);
  if (slotCertain) {
    view.k.push(tall(H, rowPx, "skel__tslot", PARTS.ROW, kids));
    bounds.slotTop = exact(H);
    notes.push("slot 0");
  } else {
    if (kids.length) view.k.push(tall(H, rowPx, "", null, kids));
    notes.push("wrap: no slot");
  }

  // E16 to E18: the footer, in the two cases of the place.
  if (footerCertain && Fh !== null) {
    const barAt = (cls) => {
      const names = footerNames(p, env, m, Fh, L);
      return tall(0, Fh, cls, TABLE_PARTS.FOOT, [node("skel__tline", { "--y": "0px" }, [], "div", {}), ...names]);
    };
    // Case 1: one row of 36 px (B = 1) and the footer fits under it. Its place is the end of the row.
    const case1 = !p.wrap && walk.B === 1 && vhLo !== -Infinity && H + rowPx + Fh <= vhLo - env.sb;
    // Case 2: the content is higher than the view, so the sticky footer sits at the bottom of the view. The bottom is exact when the horizontal bar
    // is known: no bar (sb = 0), or a bar that the width proves.
    const barKnown = env.sb === 0 || (any && m > box.w - env.sb);
    const case2 = overflow && barKnown && walk.F !== null && !warnings.length;
    if (case1) {
      const b = barAt("skel__tfoot");
      b.v["--y"] = `${H + rowPx}px`;
      view.k.push(b);
      bounds.footTop = exact(H + rowPx);
      notes.push("footer case 1");
    } else if (case2) {
      const b = barAt("skel__tfoot skel__tfoot--b");
      b.v["--b"] = `${any && m > box.w - env.sb ? env.sb : 0}px`;
      view.k.push(b);
      bounds.footBottom = exact(any && m > box.w - env.sb ? env.sb : 0);
      notes.push("footer case 2");
    } else notes.push("footer place is data");
  } else if (foot.has) notes.push("footer is data");
  return resultOf(elements, bounds, notes);
}

// ---- The header of column 0 (E11) -------------------------------------------------------------------------------------------------------
// The conditions: no hidden member (field 0 is column 0), a literal `display_name` in the defaults and in no override, and an explicit alignment
// that no override sets. Every header then shows this name. The label is a bar of the measured width.
function headerGroup(p, env, H, m, any, kids, bounds, notes) {
  const D = p.defaults.display_name;
  if (!textOk(D) || setsAny(p, "display_name", () => true) || p.defaults.hidden === true || setsAny(p, "hidden", () => true)) return;
  const align = p.defaults.align;
  if (setsAny(p, "align", () => true) || (align !== "left" && align !== "center")) return;
  // The filter button: certain when the defaults set it and no override clears it. Absent when no member sets it.
  const filterSure = p.defaults.filterable === true && !setsAny(p, "filterable", (v) => v === false);
  const filterNone = p.defaults.filterable !== true && !setsAny(p, "filterable", (v) => v === true);
  if (align === "center" && !(filterSure || filterNone)) return;
  const sorted = p.sort.some((k) => k && k.field === D);
  const textW = textWidth(D, { size: env.fsMd, weight: env.fwMedium });
  const sortW = textW + (sorted ? MARK_PX : 0);
  const filterW = filterSure ? FILTER_GAP + env.filterW : filterNone ? 0 : null;
  // The group must fit in the cell, with a filter button if one can show. A longer text is cut with an ellipsis (data).
  const room = m - 2 * env.pad - 1 - (filterW === null ? FILTER_GAP + env.filterW : filterW);
  if (sortW > room) return;
  let x = env.pad;
  if (align === "center") {
    // The width of the column is exact only with `defaults.width` and no override width.
    if (!(any && p.defaults.width !== undefined && !setsAny(p, "width", () => true))) return;
    x = env.pad + (m - 2 * env.pad - 1 - sortW - filterW) / 2;
  }
  const top = H - env.pad - 1 - GROUP_H;
  kids.push(barNode(x, top + GROUP_H / 2 - BAR_HEIGHT / 2, textW, TABLE_PARTS.HEAD_LABEL, EXACT));
  bounds.headLabel = exact(textW);
  if (sorted) kids.push(regionNode({ x: x + textW, y: top + (GROUP_H - MARK_PX) / 2, w: MARK_PX, h: MARK_PX }, TABLE_PARTS.MARK, EXACT, "skel__r skel__rr"));
  if (filterSure) kids.push(regionNode({ x: x + sortW + FILTER_GAP, y: H - env.pad - 1 - env.filterH, w: env.filterW, h: env.filterH }, TABLE_PARTS.FILTER, EXACT, "skel__r skel__rr"));
  notes.push("header label");
}

// ---- The gauge track of column 0 (E15) -------------------------------------------------------------------------------------------------
function gaugeTrack(p, env, wrap, m, any, kids, bounds, notes) {
  const cell = obj(p.defaults.cell);
  if (cell.type !== "gauge" || setsAny(p, "cell", () => true)) return;
  const y = env.pad + (wrap ? 0 : 0.5);
  const mode = cell.mode || "basic";
  if (mode === "lcd") {
    // The number of cells is exact when the width is exact: `defaults.width` and no override width.
    if (!(p.defaults.width !== undefined && !setsAny(p, "width", () => true))) return;
    const n = Math.max(1, Math.floor((Number(p.defaults.width) - 11) / LCD_CELL));
    for (let k = 0; k < n; k++) kids.push(regionNode({ x: env.pad + k * LCD_CELL, y, w: 10, h: GAUGE_H }, TABLE_PARTS.CELL, EXACT, "skel__r skel__rc"));
    bounds.lcdCells = exact(n);
    notes.push("gauge lcd");
    return;
  }
  const track = regionNode({ x: env.pad, y, w: Math.max(0, m - 2 * env.pad - 1), h: GAUGE_H }, PARTS.TRACK, EXTENT, "skel__r skel__rr skel__ttrack");
  track.v["--tm"] = `${Math.max(0, m - 2 * env.pad - 1)}px`;
  kids.push(track);
  bounds.track = least(Math.max(0, m - 2 * env.pad - 1));
  notes.push("gauge track");
}

// ---- The footer names of column 0 (E18) -------------------------------------------------------------------------------------------------
// Each cell shows its own names when the first shown column has a footer: the name of each line is at the left of the cell. It is drawn when the
// column 0 footer comes from the defaults (no override sets a footer) and the name fits in the cell.
function footerNames(p, env, m, Fh, L) {
  if (setsAny(p, "footer", () => true)) return [];
  const reducers = list(obj(p.defaults.footer).reducers);
  const out = [];
  reducers.slice(0, L).forEach((calc, l) => {
    if (typeof calc !== "string") return;
    const text = footerName(calc).toUpperCase();
    // The page gives the body a letter spacing, in px, to every text that does not reset it. The canvas does not know it.
    const w = textWidth(text, { size: env.fsXs, weight: 400 }) + [...text].length * env.footLs;
    if (env.pad + w > m - 1) return;
    out.push(barNode(env.pad, 1 + env.pad + FOOT_LINE * l + (FOOT_LINE - BAR_HEIGHT) / 2, w, TABLE_PARTS.FOOT_NAME, EXACT));
  });
  return out;
}

// Re-exports for the tests.
export { boundHolds, LEAST };
