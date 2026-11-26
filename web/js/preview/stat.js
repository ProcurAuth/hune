// The loading preview of the panel type `stat` (9.11), and the helpers that the three families of values share (`stat`, `gauge`, `bargauge`).
// The rules are in the council file `stat.md` (scratchpad/council). The loaded panel is drawn by web/js/panels/stat.js, items.js.
//
// The document fixes the box of a stat and the warning line above it. It fixes nothing else that is visible: the number of tiles is data for a real
// stat, so no gap, no text slot and no tint is drawn. One case fixes the number (proof A, see `itemCount`): one tile. The tile then has the
// box of the grid, and the preview claims it. The fill of the tile is a colour of the document, so it is never drawn. The preview of a stat is
// its box and the light. The parts that this module claims:
//   holder   the box of the loaded `.stat` (exact). It reaches the four edges of the body, over the padding (9.19). With a warning line, it starts
//            at the top of the plot, under the paragraph.
//   warning  the warning line above the box, when an override has a pattern that does not compile (exact). It shows in an unsaved draft only.
//   tile     the one tile, only with proof A (exact). It has the box of the grid.
//   label    one bar for each line of the warning text (exact).

import { EXACT, PARTS, barNode, exact, node, partAttrs, px, rect, resultOf, textWidth } from "./common.js";
import { parseGrafanaPattern } from "../fields.js";


// The warning line of the frame (`.panel__warning`): 12 px text, padding 2 px above and under, a 14 px icon and a space before the text.
const WARNING_SIZE = 12;
const WARNING_PAD = 2;
const WARNING_ICON = 14;
// The line height of the body (the token `--lh-base`, 22 px on 14 px), and the unit of the layout of the browser engines (1/64 px).
const LINE_RATIO = 1.5714;
const LAYOUT_UNIT = 64;

// The padding of the body of a panel (`--panel-pad`): the stat grid reaches the edge of the body, so it needs the value.
export const BODY_PAD = 8;

/** The sentences of the warning line: one for each distinct override pattern that does not compile, in the order of the overrides (pure). */
export function warningSentences(panel) {
  const overrides = (panel && panel.fields && panel.fields.overrides) || [];
  const seen = new Set();
  const out = [];
  for (const o of overrides) {
    if (o.match && o.match.by === "regex" && parseGrafanaPattern(o.match.value) === null && !seen.has(o.match.value)) {
      seen.add(o.match.value);
      out.push(`The override pattern ${o.match.value} is not a valid regular expression.`);
    }
  }
  return out;
}

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
 * The number of items (tiles, gauges) that the document fixes (pure): 0, 1 or null (data). Case A: the field pattern of `reduce.fields` does not compile, so no field
 * is kept. Case B: one visible query, no `partition_by_values`, and the last transform is a `calculate_field` in the mode `index` or `reduce_row`
 * with `replace_fields` true. The result then holds the new number field, with or without rows. A literal field name never fixes the number: the
 * field can be missing, or two fields can have one name.
 */
export function itemCount(panel, doc) {
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

let probe = null;

/**
 * The box of a text that the page draws in the DOM, at `size` px: { w, h } (px). The browser measures a hidden span with the font of the page, with
 * the letter spacing and the rounding of its layout. A canvas gives another width for such a text. Without a page, `textWidth` gives the width and
 * the height is the size of the font box of Inter (the ascent and the descent, each one rounded).
 */
export function domTextBox(text, size = WARNING_SIZE) {
  if (typeof document === "undefined" || !document.body) return { w: textWidth(text, { size }), h: Math.round(0.96875 * size) + Math.round(0.2421875 * size) };
  if (!probe || !probe.isConnected) {
    probe = document.createElement("span");
    probe.setAttribute("aria-hidden", "true");
    for (const [name, value] of [["position", "absolute"], ["left", "-10000px"], ["top", "0"], ["visibility", "hidden"], ["white-space", "pre"], ["font-weight", "400"], ["line-height", "normal"]]) probe.style.setProperty(name, value);
    document.body.append(probe);
  }
  probe.style.setProperty("font-size", `${size}px`);
  probe.textContent = text;
  const r = probe.getBoundingClientRect();
  return { w: r.width, h: r.height };
}

/** The width of a text that the page draws in the DOM (see `domTextBox`). */
export const domTextWidth = (text, size = WARNING_SIZE) => domTextBox(text, size).w;

/**
 * The top of a bar that stands for a line of text of `size` px in a line box of `lineH` px that starts at `lineTop`: the bar is 8 px high and has the
 * middle of the font box of the text. The engine puts the font box in the line box with half of the leading rounded down to a whole pixel.
 */
export function barTop(lineTop, lineH, size) {
  const h = domTextBox("x", size).h;
  return lineTop + Math.floor((lineH - h) / 2) + h / 2 - 4;
}

/** The ratio of the line height of the body (the token `--lh-base`). */
export function lineRatio() {
  try {
    const cs = getComputedStyle(document.body);
    const r = parseFloat(cs.lineHeight) / parseFloat(cs.fontSize);
    if (Number.isFinite(r) && r > 0) return r;
  } catch (error) {
    // No page: the constant of the token is the value.
  }
  return LINE_RATIO;
}

// The height of one line of the warning, rounded down to the layout unit of the browser.
const lineHeight = () => Math.floor(WARNING_SIZE * lineRatio() * LAYOUT_UNIT + 1e-6) / LAYOUT_UNIT;

/**
 * The lines of the warning text in a paragraph `width` px wide (pure but for the measure of a text). The first line holds the icon and a
 * space before the words. A word never breaks. Gives [{ x, w }]: the start and the width of the text of each line, from the left edge.
 */
export function warningLines(sentences, width) {
  if (!sentences.length) return [];
  const space = domTextWidth(" ", WARNING_SIZE);
  const words = sentences.join(" ").split(" ").filter((w) => w !== "");
  const lines = [];
  let first = true;
  // The text node of the loaded line starts with the space after the icon, so the first bar starts at the right edge of the icon.
  let line = { x: WARNING_ICON, w: space, n: 0 };
  let cursor = WARNING_ICON + space;
  for (const word of words) {
    const w = domTextWidth(word, WARNING_SIZE);
    const gap = line.n > 0 ? space : 0;
    // A word that does not fit goes to the next line. A line after the first has at least one word: a long word overflows. On the first
    // line, the icon can stay alone.
    if (cursor + gap + w > width && (line.n > 0 || first)) {
      lines.push({ x: line.x, w: line.w });
      line = { x: 0, w: 0, n: 0 };
      cursor = 0;
      first = false;
    }
    const add = (line.n > 0 ? space : 0) + w;
    line.w += add;
    cursor += add;
    line.n += 1;
  }
  lines.push({ x: line.x, w: line.w });
  return lines;
}

/**
 * The warning line of a panel as nodes (pure). `left` and `top` give its corner in the preview, `width` its width. Gives
 * { height, nodes }: the height is 0 and the list is empty when the panel has no warning. The region is the paragraph (padding included).
 * Each line of text is a bar of the measured width.
 */
export function warningLayout(panel, left, top, width) {
  const sentences = warningSentences(panel);
  if (!sentences.length) return { height: 0, nodes: [] };
  const line = lineHeight();
  const lines = warningLines(sentences, width);
  const height = 2 * WARNING_PAD + lines.length * line;
  const nodes = [voidNode(rect(left, top, width, height), PARTS.WARNING, EXACT)];
  lines.forEach((l, i) => {
    if (l.w > 0) nodes.push(barNode(left + l.x, barTop(top + WARNING_PAD + i * line, WARNING_SIZE * lineRatio(), WARNING_SIZE), Math.min(l.w, width - l.x), PARTS.LABEL, EXACT));
  });
  return { height, nodes };
}

/** A region with no paint: it holds a place and a size for the tests. `r` is { x, y, w, h } in px from the corner of the preview. */
export const voidNode = (r, part, cls) => node("skel__v", { "--x": px(r.x), "--y": px(r.y), "--w": px(r.w), "--h": px(r.h) }, [], "div", part ? partAttrs(part, cls) : {});

/**
 * The box that fills the preview under a top margin `top`, to the three other edges (pure). The edges stay those of the CSS box, so a box with a
 * fraction of a pixel stays exact. It is the holder of the loaded panel.
 */
export const holderNode = (top = 0, part = "holder", cls = EXACT) => node("skel__fill", { "--y": px(top) }, [], "div", partAttrs(part, cls));

// `box` is the body of the panel (the CSS makes the preview of a stat reach the edge of the body: panel-loading.css, family stat).
export function build(panel, box, doc, ctx) {
  const warn = warningLayout(panel, BODY_PAD, BODY_PAD, (box.fw ?? box.w) - 2 * BODY_PAD);
  // The loaded grid starts at the top of the plot, under the paragraph (the rule `.panel__warning + .panel__plot > .stat`): the paragraph and the
  // padding above it are over the grid. With no warning the grid starts at the top of the body.
  const top = warn.height ? BODY_PAD + warn.height : 0;
  const height = Math.max(0, box.h - top);
  const elements = [...warn.nodes, holderNode(top)];
  const bounds = { holder: exact(height), warning: exact(warn.height) };
  const notes = [warn.height ? "box warning" : "box"];
  // Proof A: one tile. With one item the tile has the size of the grid in each direction (`tileGrid`), so its box is the box of the grid.
  if (itemCount(panel, doc) === 1) {
    elements.push(holderNode(top, "tile"));
    bounds.tile = exact(height);
    notes.push("one tile");
  }
  return resultOf(elements, bounds, notes);
}
