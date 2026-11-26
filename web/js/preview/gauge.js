// The loading preview of the panel type `gauge` (9.11).
// The rules are in the council file `gauge.md` (scratchpad/council). The loaded panel is drawn by web/js/panels/gauge.js. The preview calls the
// pure functions of that file (`gridCells`, `stackCells`, `cellParts`, `gaugeGeometry`, `angleOf`, `bandEntries`, `scaleOf`): the loaded panel and
// the preview use one source for each number.
//
// The document fixes the number of gauges in two cases only (E4): a field pattern that does not compile (no gauge) and a chain of transforms
// that ends with one new number field (one gauge). Every other gauge shows its box and the light. With one gauge, the preview draws the parts that
// the document fixes: the cell, the title box, the canvas, the empty arc, the band of the threshold marks and the neutral mark. The data choose
// the size of the value text, and the text can move the arc: the preview draws the arc of a text of no width, which is the least form (a bound).
// No colour of the document, no value, no text of the value and no threshold label is drawn.
//
// The parts that this module claims:
//   holder  the box of the loaded `.gauge` (exact)
//   warning, label (the lines of the warning, see stat.js)
//   tile    the cell of the gauge (exact), with one gauge
//   title   the box of the title (exact), when the document fixes a title
//   label   a bar for the title text, when the document fixes the text (exact)
//   plot    the canvas of the cell (exact)
//   arc     the box of the empty arc: exact when the value text cannot move it, else a lower bound (least)
//   scale   the box of the band of the threshold marks (exact), when the scale is fixed

import { EXACT, LEAST, PARTS, barNode, exact, least, node, partAttrs, px, rect, resultOf, svgNode } from "./common.js";
import { barTop, domTextWidth, holderNode, lineRatio, voidNode, warningLayout } from "./stat.js";
import { parseGrafanaPattern, computeFieldConfig } from "../fields.js";
import { interpolateDisplayName } from "../names.js";
import { angleOf, bandEntries, cellParts, gaugeGeometry, gridCells, scaleOf, stackCells } from "../panels/gauge.js";
import { floorUnit } from "../panels/box.js";

// The ends of the arc, in units of pi: the loaded gauge limits an angle to them, so a value outside the scale gives one end.
const START = angleOf(-Infinity, 0, 1);
const END = angleOf(Infinity, 0, 1);
// The width that stands for a text of any width: the geometry of the lower bound of the radius (see `geometryOf`).
const WIDE_TEXT = 1e9;
// The members of the field configuration that an override can set and that a part of the preview reads.
const MEMBERS = ["min", "max", "unit", "decimals", "thresholds", "color", "display_name"];

// ---- The chain of the data ------------------------------------------------------------------------------------------------------------

// The panel in the document with this id, or null. The document has sections with panels.
function panelOf(doc, id) {
  for (const s of (doc && doc.sections) || []) for (const p of s.panels || []) if (p.id === id) return p;
  return null;
}

/**
 * What the document says about the data of a panel (pure): the panel that owns the queries (the source of a `reuse`), the refs of its visible queries
 * and the chain of the transforms (the transforms of the source first, when `with_transforms` is true). `owner` is null when the source is not in
 * the document.
 */
export function chainOf(panel, doc) {
  const data = (panel && panel.data) || {};
  let owner = panel;
  let chain = [];
  if (data.reuse) {
    owner = panelOf(doc, data.reuse.panel);
    if (owner && data.reuse.with_transforms) chain = [...(owner.transforms || [])];
  }
  chain = [...chain, ...((panel && panel.transforms) || [])];
  const od = (owner && owner.data) || {};
  const refs = [];
  if (od.query && od.query.hide !== true) refs.push(od.query.ref || "A");
  for (const q of od.extra_queries || []) if (q.hide !== true) refs.push(q.ref);
  return { owner, refs, chain };
}

/**
 * The number of gauges that the document fixes (pure): 0, 1 or null (data). Case A: the field pattern of `reduce.fields` does not compile, so no field
 * is kept. Case B: one visible query, no `partition_by_values`, and the last transform is a `calculate_field` in the mode `index` or `reduce_row`
 * with `replace_fields` true. The result then holds the new number field, with or without rows. A literal field name never fixes the number: the
 * field can be missing, or two fields can have one name.
 */
export function gaugeCount(panel, doc) {
  const o = (panel && panel.options) || {};
  const reduce = o.reduce || {};
  const pattern = reduce.fields;
  if (typeof pattern === "string" && pattern !== "" && parseGrafanaPattern(pattern) === null) return 0;
  if (reduce.values !== false) return null;
  if (pattern !== null && pattern !== undefined && pattern !== "") return null;
  const { owner, refs, chain } = chainOf(panel, doc);
  if (!owner || refs.length !== 1 || !chain.length) return null;
  if (chain.some((t) => t.kind === "partition_by_values")) return null;
  const last = chain[chain.length - 1];
  return last.kind === "calculate_field" && last.replace_fields === true && (last.mode === "index" || last.mode === "reduce_row") ? 1 : null;
}

// ---- The configuration that the document decides --------------------------------------------------------------------------------------

// An override is excluded when its matcher matches no field: a pattern that does not compile, or a query ref that no visible query has.
function excluded(match, refs) {
  if (!match) return true;
  if (match.by === "regex") return parseGrafanaPattern(match.value) === null;
  if (match.by === "query") return !refs.includes(match.value);
  return false;
}

/**
 * The configuration of the gauge that the document fixes (pure). `config` is the resolved configuration of `fields.defaults`. `decided[name]` is true
 * when no override that can match a field sets the member `name`: the member of the defaults then applies to every field.
 */
export function configOf(panel, refs) {
  const spec = (panel && panel.fields) || {};
  const probe = { kind: "number", name: "x", labels: null, values: new Float64Array(0), config: {} };
  const config = computeFieldConfig([probe], { defaults: spec.defaults || {}, overrides: [] }, "gauge").fields[0].config;
  const live = (spec.overrides || []).filter((o) => !excluded(o.match, refs));
  const decided = {};
  for (const m of MEMBERS) decided[m] = !live.some((o) => o.set && Object.prototype.hasOwnProperty.call(o.set, m));
  return { config, decided };
}

// The scale of the gauge, when the document fixes it: { min, max } as the loaded panel reads them, or null. The `percentage` mode of the thresholds
// uses 0 to 100. Else both ends come from `min` and `max`, or from a unit `percent` or `percentunit`. With the markers, the ends are rounded to
// `decimals`, so `decimals` must be decided too.
function scaleFixed(cfg, decided, markers) {
  if (!decided.thresholds) return null;
  let min;
  let max;
  if (cfg.thresholds && cfg.thresholds.mode === "percentage") {
    [min, max] = [0, 100];
  } else {
    if (!decided.min || !decided.max || !decided.unit) return null;
    const set = cfg.set || {};
    const pct = cfg.unit === "percent" || cfg.unit === "percentunit";
    if (!(Number.isFinite(set.min) || pct) || !(Number.isFinite(set.max) || pct)) return null;
    ({ min, max } = scaleOf({ set, unit: cfg.unit }, { min: null, max: null }));
  }
  if (markers) {
    if (!decided.decimals) return null;
    const d = cfg.decimals ?? 2;
    [min, max] = [parseFloat(min.toFixed(d)), parseFloat(max.toFixed(d))];
  }
  return min < max ? { min, max } : null;
}

// The room of the threshold band (E12). `bands` is true (the room is reserved), false (no room) and `exact` says that the document decides it.
function bandsOf(cfg, decided, markers, scale) {
  if (!markers) return { bands: false, exact: true };
  if (decided.color && !(cfg.color && cfg.color.mode === "thresholds")) return { bands: false, exact: true };
  if (decided.color && decided.thresholds && scale) {
    const entries = bandEntries(cfg.thresholds, scale.min, scale.max, cfg.decimals ?? 2);
    return { bands: entries.length > 1, exact: true, entries };
  }
  // The band can be missing (one entry, or an override of the colour): the room is the least form. The loaded radius is at least this radius.
  return { bands: true, exact: false };
}

// ---- The title text -----------------------------------------------------------------------------------------------------------------------

/**
 * The title text that the document fixes (pure), or null. It is the display name of the defaults, when no override sets a name, the name has no
 * `__field` token (the field gives its value) and it is not empty after the `__series` tokens give the empty text (a gauge has no series name).
 */
export function titleText(cfg, decided) {
  const template = cfg.set && cfg.set.display_name;
  if (!template || !decided.display_name || typeof template !== "string") return null;
  if (/\$\{__field|\[\[__field/.test(template)) return null;
  const text = interpolateDisplayName(template, { name: "", labels: null }, "", "");
  return text !== "" ? text : null;
}

// ---- The geometry ---------------------------------------------------------------------------------------------------------------------

/**
 * The geometry of one gauge in a box `box` (the holder, px) (pure): the cell, its parts, the geometry of the arc and the bound that the document gives
 * for it. It follows `layout()` of the loaded panel. The text of the value is not known, so the arc is the one of a text of no width: the loaded
 * arc has at least this radius, and the same radius when the text cannot move it (`exactArc`).
 */
export function gaugeLayout(panel, box, refs, cfg, decided) {
  const o = panel.options || {};
  const orientation = o.orientation || "auto";
  const vertical = orientation === "vertical";
  const manual = o.sizing === "manual";
  const w = floorUnit(box.fw ?? box.w);
  const h = floorUnit(box.fh ?? box.h);
  const minW = manual && vertical && Number.isFinite(o.min_width) ? o.min_width : undefined;
  const minH = manual && orientation === "horizontal" && Number.isFinite(o.min_height) ? o.min_height : undefined;
  const cell = orientation === "auto" ? gridCells(1, w, h)[0] : stackCells(1, w, h, vertical, vertical ? minW : minH)[0];
  const titled = !!(panel.fields && panel.fields.defaults && panel.fields.defaults.display_name);
  const titleSize = Number.isFinite(o.title_size) ? o.title_size : null;
  const parts = cellParts({ w: cell.w, h: cell.h, title: titled, orientation, titleSize });
  const labels = o.show_threshold_labels === true;
  const markers = o.show_threshold_markers !== false;
  const scale = scaleFixed(cfg, decided, markers);
  const room = bandsOf(cfg, decided, markers, scale);
  const valueSize = Number.isFinite(o.value_size) ? o.value_size : null;
  const args = { cw: parts.canvasW, ch: parts.canvasH, sizeW: parts.sizeW, sizeH: parts.sizeH, labels, markers, bands: room.bands, valueSize };
  const g = gaugeGeometry({ ...args, w14: 0 });
  // The other bound: a value text that is as wide as it can be. The loaded radius is between the two, and the same when they agree.
  const wide = gaugeGeometry({ ...args, w14: WIDE_TEXT });
  const exactArc = room.exact && Math.abs(g.radius - wide.radius) < 1e-9 && Math.abs(g.cy - wide.cy) < 1e-9;
  return { orientation, vertical, cell, parts, titled, labels, markers, scale, room, g, exactArc, valueSize };
}

// The path of a ring from the radius `r` (outer) to `r - w` (inner), between the angles `a1` and `a2` (units of pi): the arc, a flat end, the inner
// arc back and a flat end. The canvas of the loaded panel draws the same shape (gauge.js, `ring`).
export function ringPath(cx, cy, r, w, a1, a2) {
  const p1 = a1 * Math.PI;
  const p2 = a2 * Math.PI;
  const at = (rad, p) => `${Number((cx + rad * Math.cos(p)).toFixed(4))} ${Number((cy + rad * Math.sin(p)).toFixed(4))}`;
  const large = a2 - a1 > 1 ? 1 : 0;
  const inner = r - w;
  return `M${at(r, p1)}A${r} ${r} 0 ${large} 1 ${at(r, p2)}L${at(inner, p2)}A${inner} ${inner} 0 ${large} 0 ${at(inner, p1)}Z`;
}

// A ring as a node: a path with the class of the shape. `attrs` has the hooks of the tests, or nothing.
const ringNode = (d, attrs = {}) => node("skel__ring", {}, [], "path", { d, ...attrs });

// The nodes of a gauge in a box (pure). `at` is the corner of the holder in the preview.
function gaugeNodes(panel, lay, cfg, decided, at) {
  const { cell, parts, g } = lay;
  const out = [];
  const cellX = at.x + cell.x;
  const cellY = at.y + cell.y;
  out.push(voidNode(rect(cellX, cellY, cell.w, cell.h), PARTS.TILE, EXACT));
  const bounds = { tile: exact(cell.w) };
  const fixedTitle = lay.titled ? titleText(cfg, decided) : null;
  if (lay.titled) {
    const tr = rect(cellX, cellY + parts.titleY, parts.titleW, parts.titleH);
    out.push(voidNode(tr, "title", EXACT));
    bounds.title = exact(parts.titleH);
    if (fixedTitle !== null && parts.titleH > 0) {
      const text = domTextWidth(fixedTitle, parts.titleSize);
      const bar = Math.min(text, parts.titleW);
      out.push(barNode(tr.x + (parts.titleW - bar) / 2, barTop(tr.y, parts.titleSize * lineRatio(), parts.titleSize), bar, PARTS.LABEL, EXACT));
    }
  }
  const canvas = rect(cellX, cellY + parts.top, parts.canvasW, parts.canvasH);
  const shapes = [];
  const drawn = g.radius >= 0 && g.radius - g.arcW >= 0;
  if (drawn) {
    shapes.push(ringNode(ringPath(g.cx, g.cy, g.radius, g.arcW, START, END), partAttrs(PARTS.ARC, lay.exactArc ? EXACT : LEAST)));
    bounds.radius = lay.exactArc ? exact(g.radius) : least(g.radius);
    bounds.cy = lay.exactArc ? exact(g.cy) : least(g.cy);
    const { scale, room } = lay;
    if (scale && room.exact && room.bands && room.entries) {
      const last = room.entries[room.entries.length - 1];
      const end = angleOf(last.value, scale.min, scale.max);
      if (end > START) shapes.push(ringNode(ringPath(g.cx, g.cy, g.radius + g.markW, g.markW - 2, START, end), end === END ? partAttrs(PARTS.SCALE, lay.exactArc ? EXACT : LEAST) : {}));
    }
    const neutral = (panel.options || {}).neutral;
    if (scale && Number.isFinite(neutral) && g.arcW - 4 > 0 && g.radius - 2 >= 0) {
      const d = (scale.max - scale.min) / 600;
      const a1 = angleOf(neutral - d, scale.min, scale.max);
      const a2 = angleOf(neutral + d, scale.min, scale.max);
      if (a2 > a1) shapes.push(ringNode(ringPath(g.cx, g.cy, g.radius - 2, g.arcW - 4, a1, a2), {}));
    }
  }
  out.push(node("skel__canvas", { "--x": px(canvas.x), "--y": px(canvas.y), "--w": px(canvas.w), "--h": px(canvas.h) }, [svgNode(canvas.w, canvas.h, shapes)], "div", partAttrs(PARTS.PLOT, EXACT)));
  bounds.canvas = exact(canvas.w);
  return { nodes: out, bounds };
}

// `box` is the content box of the body: the gauge holder is under the warning line, if any.
export function build(panel, box, doc, ctx) {
  const warn = warningLayout(panel, 0, 0, box.fw ?? box.w);
  const holder = { w: box.w, h: Math.max(0, box.h - warn.height), fw: box.fw, fh: box.fh === undefined ? undefined : Math.max(0, box.fh - warn.height) };
  const elements = [...warn.nodes, holderNode(warn.height)];
  const bounds = { holder: exact(holder.h), warning: exact(warn.height) };
  const n = gaugeCount(panel, doc);
  if (n !== 1) return resultOf(elements, bounds, [n === 0 ? "no gauge" : "box"]);
  const { refs } = chainOf(panel, doc);
  const { config, decided } = configOf(panel, refs);
  const lay = gaugeLayout(panel, holder, refs, config, decided);
  const body = gaugeNodes(panel, lay, config, decided, { x: 0, y: warn.height });
  return resultOf([...elements, ...body.nodes], { ...bounds, ...body.bounds }, [lay.exactArc ? "one gauge, exact arc" : "one gauge, bounded arc"]);
}
