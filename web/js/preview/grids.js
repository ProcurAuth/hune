// The helpers that the three previews of the grid family share: heatmap.js, histogram.js and timeline.js (9.11).
// Each helper reads the JSON document only: the members of the panel, its fields and its overrides. No helper reads a query text.

import { parseGrafanaPattern } from "../fields.js";
import { visibleQueries } from "../queries.js";
import { fontOf, node, partAttrs, px, textWidth } from "./common.js";

// The marker of a key that no member of the document sets: the value comes from the data (the base name of a field, for `display_name`).
export const FROM_DATA = Object.freeze({ data: true });

// The line height of the text in the legends and the tables of the charts: 1.5714 of 12 px.
export const LINE = 1.5714 * 12;

// ---- The warning line ------------------------------------------------------------------------------------------------------------
// The loaded panel shows one warning line for each distinct override pattern that does not compile. The server refuses such a pattern, so a
// stored document never has one. The preview does not guess the height of the line: the family gives the neutral form.
export function warningValues(panel) {
  const overrides = panel && panel.fields && Array.isArray(panel.fields.overrides) ? panel.fields.overrides : [];
  const out = [];
  for (const o of overrides) {
    if (o && o.match && o.match.by === "regex" && parseGrafanaPattern(o.match.value) === null && !out.includes(o.match.value)) out.push(o.match.value);
  }
  return out;
}

// ---- The possible values of a key of the field config ------------------------------------------------------------------------------
// The loaded panel resolves the config of each field: the defaults, then each matching override in document order, then the built-in default.
// The preview has no field. It lists, for a key, the values that a drawn row can get (the council rule, section 1 of the timeline file).

// The panel whose queries give the fields: the source panel of a `reuse`, else the panel itself. Null when the source is not known.
export function ownerOf(panel, doc) {
  const reuse = panel && panel.data && panel.data.reuse;
  if (!reuse) return panel;
  for (const s of (doc && doc.sections) || []) for (const p of s.panels || []) if (p.id === reuse.panel) return p;
  return null;
}

// True when no transform can change the fields of the owner. A `reuse` with transforms also reads the transforms of the source.
function plain(panel, doc) {
  const own = panel && panel.transforms;
  if (Array.isArray(own) && own.length) return false;
  const reuse = panel && panel.data && panel.data.reuse;
  if (!reuse) return true;
  const src = ownerOf(panel, doc);
  if (!src) return false;
  return reuse.with_transforms !== true || !(Array.isArray(src.transforms) && src.transforms.length);
}

/**
 * The class of a matcher for the panel: "never" (no field can match), "every" (every field matches) or "maybe" (the data decide).
 * A `regex` that does not compile never matches. A `query` that names no visible query of the owner never matches (a field has the ref of a visible
 * query or none). A `query` on the only visible query matches every field when no transform applies.
 */
export function matchClass(match, panel, doc) {
  if (!match) return "never";
  if (match.by === "regex" && parseGrafanaPattern(match.value) === null) return "never";
  if (match.by === "query") {
    const owner = ownerOf(panel, doc);
    if (!owner) return "maybe";
    const visible = visibleQueries(owner);
    if (!visible.some((q) => q.ref === match.value)) return "never";
    return visible.length === 1 && plain(panel, doc) && (owner === panel || plain(owner, doc)) ? "every" : "maybe";
  }
  return "maybe";
}

/**
 * The list of the values that the key `key` can have for a row: the defaults (or `builtin` when the defaults do not set it), and the `set` of each
 * override that can match. An override that matches every field replaces the list. The elements of the list are the values of the document.
 */
export function possible(panel, doc, key, builtin = FROM_DATA) {
  const fields = (panel && panel.fields) || {};
  const defaults = fields.defaults || {};
  let list = [defaults[key] !== undefined ? defaults[key] : builtin];
  for (const o of Array.isArray(fields.overrides) ? fields.overrides : []) {
    if (!o || !o.set || o.set[key] === undefined) continue;
    const kind = matchClass(o.match, panel, doc);
    if (kind === "never") continue;
    list = kind === "every" ? [o.set[key]] : [...list, o.set[key]];
  }
  return list;
}

/** True when `h` (a `hide_from` object, or the data marker) hides the series from the picture. */
export const hidesViz = (h) => !!h && h !== FROM_DATA && h.viz === true;
/** True when `h` hides the series from the legend. */
export const hidesLegend = (h) => !!h && h !== FROM_DATA && h.legend === true;

/** A panel that can never draw a picture: every possible `hide_from` hides the series (the loaded body ends in a message). */
export const neverDrawn = (panel, doc) => possible(panel, doc, "hide_from", {}).every(hidesViz);

// ---- Text -------------------------------------------------------------------------------------------------------------------------
// The text of the charts, the legends and the tables has the letter spacing of the page: the body sets it in px and every text inherits that length.
// A canvas that is in the page uses it too (the engine measures on such a canvas). A canvas that is not in the page does not. So the preview
// measures on a probe canvas in the body, and its widths are the widths of the loaded texts. A test with no page uses the measurer of common.js.
let probe = null;
export function measureText(text, { size = 12, weight = 400 } = {}) {
  if (typeof document === "undefined" || !document.body) return textWidth(text, { size, weight });
  if (!probe) {
    const canvas = document.createElement("canvas");
    canvas.style.cssText = "position:absolute;left:-9999px;top:0;visibility:hidden;pointer-events:none";
    canvas.setAttribute("aria-hidden", "true");
    document.body.append(canvas);
    probe = canvas.getContext("2d");
  }
  probe.font = fontOf(size, weight);
  return probe.measureText(String(text)).width;
}

/** The width of a DOM text of `size` px and `weight`: the same as the width on the probe canvas, with the letter spacing of the page. */
export const domWidth = measureText;

// ---- The scroll bar ------------------------------------------------------------------------------------------------------------
// The width of a classic scroll bar of the platform: 0 with overlay scroll bars. A probe measures it once. A test sets it with `setGutter`.
let gutter = null;
export function setGutter(v) {
  gutter = v;
}
export function scrollGutter() {
  if (gutter !== null) return gutter;
  try {
    const probe = document.createElement("div");
    probe.style.cssText = "position:absolute;visibility:hidden;width:50px;height:50px;overflow-y:scroll;scrollbar-gutter:auto";
    document.body.append(probe);
    gutter = probe.offsetWidth - probe.clientWidth;
    probe.remove();
  } catch (error) {
    gutter = 0;
  }
  return gutter;
}

/** The pixel ratio of the screen, as the engine reads it. */
export function pixelRatio() {
  return typeof window !== "undefined" && window.devicePixelRatio ? window.devicePixelRatio : 1;
}

/** A guide: a rectangle that carries a measured part and paints nothing. `r` is { x, y, w, h } in px from the corner of the preview. */
export const guideNode = (r, part, cls, c = "skel__g") => node(c, { "--x": px(r.x), "--y": px(r.y), "--w": px(r.w), "--h": px(r.h) }, [], "div", partAttrs(part, cls));
