// The loading preview of the panel type `logs` (9.11).
// The rules of this family are in the council file `logs.md` (scratchpad/council). The loaded panel is drawn by web/js/panels/logs.js.
// The preview uses the JSON document only. The document fixes the box, the font context, the first line slot (28 px), the time box (23ch) and the
// left edge of the level cell. Everything else on a line is data, so the preview draws one slot only: a time bar when `show_time` is `true`, and one
// track from the left edge of the level cell.
//
// The parts (data-skel-part) and their class:
//   plot     the list (`.logs`).                                     exact
//   warning  the warning line of an invalid override pattern.        exact (the same element, with a transparent text)
//   row      the first line.                                         extent: left, top and height exact, right least (no wrap only)
//   time     the time box of the first line (a bar of 23ch).         exact (left, width and centre)
//   track    the region from the level cell to the end of the line.  extent
// The rules that the council leaves to the owner are in the final report of the family. The check of a chain of transforms that gives no list
// (the cases N1 to N3 of the council file) is not built: the rule is in docs/04 9.11, and a shared reader will serve it later.

import { BAR_HEIGHT, EXACT, PARTS, neutral, node, partAttrs, resultOf } from "./common.js";
import { parseGrafanaPattern } from "../fields.js";

export const EXTENT = "extent";
export const LOGS_PARTS = Object.freeze({ ...PARTS, TIME: "time" });
// The sizes of the loaded list (web/css/panel-logs.css): the line height of the text band, the padding under a line, the gap between the columns.
export const LINE_BAND = 22;
export const LINE_PAD = 6;
export const GAP = 12;
// The least proven form: the bar has the thickness of the glyphs of a line, centred in the text band (`margin-top` 7 px).
export const BAR_TOP = (LINE_BAND - BAR_HEIGHT) / 2;

const obj = (v) => (v && typeof v === "object" && !Array.isArray(v) ? v : {});

// The environment: the thickness of the classic vertical scroll bar of the list. A probe with the real class measures it. A test gives `ctx.env`.
export function environment(ctx) {
  if (ctx && ctx.env) return { sb: 0, ...ctx.env };
  if (typeof document === "undefined" || !document.body) return null;
  const probe = document.createElement("div");
  probe.className = "logs";
  for (const [name, value] of [["position", "absolute"], ["visibility", "hidden"], ["pointer-events", "none"], ["left", "0px"], ["top", "0px"], ["width", "200px"], ["height", "100px"]]) probe.style.setProperty(name, value);
  const tall = document.createElement("div");
  tall.style.setProperty("height", "1000px");
  probe.append(tall);
  document.body.append(probe);
  try {
    return { sb: probe.offsetWidth - probe.clientWidth };
  } finally {
    probe.remove();
  }
}

/** The warning texts of the override patterns that the page cannot compile, once for each distinct value. */
export function warningsOf(panel) {
  const out = [];
  const overrides = Array.isArray(obj(panel.fields).overrides) ? panel.fields.overrides : [];
  for (const o of overrides) {
    const match = obj(obj(o).match);
    if (match.by === "regex" && parseGrafanaPattern(match.value) === null) {
      const text = `The override pattern ${match.value} is not a valid regular expression.`;
      if (!out.includes(text)) out.push(text);
    }
  }
  return out;
}

/**
 * The preview of a logs panel (logs.md). `panel` is the panel of the document, `box` is { w, h } (the content box of the body), `ctx` is
 * { rt, page, env } (`env` is for the tests). Gives { elements, bounds, notes }.
 */
export function build(panel, box, doc, ctx) {
  const env = environment(ctx);
  if (!env || !panel || !box) return neutral("no environment");
  const options = obj(panel.options);
  const showTime = options.show_time === true;
  const wrap = options.wrap === true;
  const notes = [];
  const bounds = {};
  const elements = [];

  // E4: the warning line is outside the mono box, as in the loaded panel (inside it would take the mono font).
  const warnings = warningsOf(panel);
  if (warnings.length) {
    notes.push("warning");
    const icon = node("icon", {}, [], "svg", { width: "14", height: "14", viewBox: "0 0 24 24", "aria-hidden": "true", focusable: "false" });
    const text = node("", {}, [], "span", {});
    text.x = ` ${warnings.join(" ")}`;
    elements.push(node("panel__warning skel__twarn", {}, [icon, text], "p", partAttrs(PARTS.WARNING, EXACT)));
  }

  // E5 to E8: slot 1. The time box is 23ch and comes only with `show_time` equal to `true`. The level cell starts after the time box and a gap of
  // 12 px. With `wrap` the loaded code puts the 12 px inside the time box, so the gap is 0 (the spec says 12: see the report of the family).
  const kids = [];
  if (showTime) {
    kids.push(node("skel__ltime", {}, [], "div", partAttrs(LOGS_PARTS.TIME, EXACT)));
    bounds.time = { kind: EXACT, value: "23ch" };
  }
  kids.push(node("skel__ltrack", {}, [], "div", partAttrs(PARTS.TRACK, EXTENT)));
  bounds.trackLeft = { kind: EXACT, value: showTime ? (wrap ? "23ch" : "23ch+12px") : "0" };
  // The right edge is the edge of the line box, at least the client width of the list: the list less its classic vertical bar.
  bounds.trackRight = { kind: "least", value: box.w - env.sb };
  const slot = node(`skel__log${wrap ? " skel__log--wrap" : ""}`, { "--sb": `${env.sb}px` }, kids, "div", wrap ? {} : partAttrs(PARTS.ROW, EXTENT));
  elements.push(node("skel__logs", {}, [slot], "div", partAttrs(PARTS.PLOT, EXACT)));
  notes.push(showTime ? "slot with time" : "slot");
  return resultOf(elements, bounds, notes);
}
