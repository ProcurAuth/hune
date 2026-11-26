// The loading preview of the panel type `stat` (9.11), and the helpers that the three families of values share (`stat`, `gauge`, `bargauge`).
// The rules are in the council file `stat.md` (scratchpad/council). The loaded panel is drawn by web/js/panels/stat.js, items.js.
//
// The document fixes the box of a stat and the warning line above it. It fixes nothing else that is visible: the number of tiles is data (no
// member of the document gives it for a real stat), so no tile, no gap, no text slot and no tint is drawn. The preview of a stat is its box and
// the light. The parts that this module claims:
//   holder   the box of the loaded `.stat` (exact). It reaches the four edges of the body, over the padding (9.19), except for a warning line.
//   warning  the warning line above the box, when an override has a pattern that does not compile (exact). It shows in an unsaved draft only.
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
  let line = { x: WARNING_ICON + space, w: 0, n: 0 };
  let cursor = line.x;
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
  // The loaded grid starts under the paragraph and covers the last 8 px of it: its top is the height of the paragraph.
  return resultOf([...warn.nodes, holderNode(warn.height)], { holder: exact(box.h - warn.height), warning: exact(warn.height) }, [warn.height ? "box warning" : "box"]);
}
