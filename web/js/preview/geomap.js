// The loading preview of the panel type `geomap` (9.11).
// The rules of this family are in the council file `geomap.md` (scratchpad/council). The loaded panel is drawn by web/js/panels/geomap.js,
// web/js/geo_model.js, styled by web/css/panel-geomap.css.
//
// The preview uses the JSON document only. The document fixes three visible facts: the map box, the zoom column (`options.show_zoom`) and the
// warning line of an invalid regex override. The background (tiles, or the lines of the plain map), the markers, the notes, the attribution and the
// glyphs of the zoom squares depend on the data, on the server configuration or on the state of the page, so the preview draws none of them.
//
// Tree: the root `.skel` is a flex column. It holds the optional warning line and the map node `.skel__map`. The map node holds the zoom column
// `.skel__zooms` with two squares `.skel__zoom`, only when `show_zoom` is `true`. The sizes of the zoom squares are in the CSS (19.25 px: a px
// value of the tree would round them). The squares have the neutral tone and no glyph.
//
// Parts that the harness measures: `plot` (the map node, EXACT: the box of the loaded `.geo__canvas`), `zoom` (each square, EXACT) and `warning`.

import { EXACT, PARTS, exact, node, partAttrs, resultOf, unknown, warningNode } from "./common.js";

// The zoom squares of the loaded map (panel-geomap.css): 19.25 px, 8 px from the top left corner of the map.
export const ZOOM = 19.25;
export const ZOOM_MARGIN = 8;

export function build(panel, box, doc, ctx) {
  const o = panel.options || {};
  const warning = warningNode(panel);
  // The loaded code draws the column for `show_zoom` = true only (`hidden` otherwise), so a missing or other value draws no square.
  const zoom = o.show_zoom === true;
  const squares = zoom ? [node("skel__zoom", {}, [], "div", partAttrs("zoom", EXACT)), node("skel__zoom", {}, [], "div", partAttrs("zoom", EXACT))] : [];
  const map = node("skel__map", {}, zoom ? [node("skel__zooms", {}, squares)] : [], "div", partAttrs(PARTS.PLOT, EXACT));
  const notes = ["geomap", zoom ? "zoom" : "no zoom"];
  if (warning) notes.push("warning");
  // With a warning line the map box is lower than the preview box by a height that the browser gives: the width stays exact.
  const bounds = { plotW: exact(box.w), plotH: warning ? unknown() : exact(box.h) };
  if (zoom) {
    bounds.zoomW = exact(ZOOM);
    bounds.zoomH = exact(ZOOM);
    bounds.zoomX = exact(ZOOM_MARGIN);
    bounds.zoomY = exact(ZOOM_MARGIN);
  }
  return resultOf(warning ? [warning, map] : [map], bounds, notes);
}
