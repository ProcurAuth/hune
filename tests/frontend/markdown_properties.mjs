// SPDX-License-Identifier: MIT OR Apache-2.0
// The properties P1 to P7 of the Markdown of the text panel and the 12 families of hostile texts (9.19b, 16.10a).
// The tests and the fuzz tool share this file. It is not part of the page.
import { checkUrl } from "../../web/js/links.js";
import { MD_MAX_DEPTH, MD_MAX_INLINE_DEPTH, MD_MAX_NODES, isSafeHref, parseMarkdown } from "../../web/js/markdown.js";
import { buildDom } from "../../web/js/panels/text.js";
import { MD_KINDS, checkDom, fakeDocument, shapeOf, textOf } from "./markdown_helpers.mjs";

/** A seeded generator (xorshift32): the same seed gives the same texts. It gives a number in [0, 1). */
export function makeRng(seed) {
  let s = (seed >>> 0) || 1;
  return () => {
    s ^= s << 13;
    s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5;
    s >>>= 0;
    return s / 4294967296;
  };
}

const PAYLOADS = ["<script>alert(1)</script>", "<img src=x onerror=alert(1)>", "[x](javascript:alert(1))", "![x](http://evil.example/b.png)", "<a href=javascript:alert(1)>x</a>", "[x](//evil.example)", "$v", "${v}", "${v:csv}", "\\$v"];
export const ALPHABET = [
  "*", "**", "_", "__", "`", "``", "```", "~~~", "[", "]", "(", ")", "](", "[a](", "http://", "https://", "javascript:", "data:", "x", "ab", " ", "  ", "\n", "\n\n", "\\", "<", ">", "&", "#", "# ", "###### ", "####### ", "- ", "* ", "+ ", "1. ", "2) ", "> ", ">", "|", "|-|", "---", "***", "___", "$", "{", "}", "/", "//", "?", "\t", "!", "![", ":", "-", "=", "%0a",
  "é", "‮", "​", " ", "\u0000", "😀", " ", "\\n", ...PAYLOADS,
];

/** A text of 1 to 60 pieces of the alphabet. */
export function randomText(rng) {
  const n = 1 + Math.floor(rng() * 60);
  let s = "";
  for (let i = 0; i < n; i++) s += ALPHABET[Math.floor(rng() * ALPHABET.length)];
  return s;
}

const HOSTILE_VALUE = "\"><img src=x onerror=alert(1)>[x](javascript:alert(1))**x**\n# h\n- l\n```";

// The depth of the blocks (quotes and items) and of the inline constructs (em, strong, links) of a tree, and its count of nodes.
function measure(tree) {
  let nodes = 0;
  let block = 0;
  let inl = 0;
  const walk = (n, b, i) => {
    nodes++;
    if (!MD_KINDS.includes(n.k)) throw new Error(`kind ${n.k}`);
    if (n.k === "t" && typeof n.v !== "string") throw new Error("text leaf");
    if (n.k === "a" && !isSafeHref(n.href)) throw new Error(`destination ${n.href}`);
    const nb = n.k === "quote" || n.k === "li" ? b + 1 : b;
    const ni = n.k === "em" || n.k === "strong" || n.k === "a" ? i + 1 : i;
    block = Math.max(block, nb);
    inl = Math.max(inl, ni);
    for (const c of n.c || []) walk(c, nb, ni);
    for (const c of n.head || []) walk(c, nb, ni);
    for (const r of n.rows || []) for (const c of r) walk(c, nb, ni);
  };
  for (const n of tree) walk(n, 0, 0);
  return { nodes, block, inl };
}

/**
 * Check P1 to P7 on one text (a string; the line ends split it). It throws an Error that names the property, and gives `{ ok, steps, limit }`.
 * `identity` is the identity expansion, and the hostile expansion replaces each placeholder by a value that holds Markdown and HTML.
 */
export function checkProperties(text) {
  const lines = text.split("\n");
  const copy = lines.slice();
  let r;
  try {
    r = parseMarkdown(lines); // P1: it never throws
  } catch (e) {
    throw new Error(`P1 ${e && e.message}`);
  }
  const r2 = parseMarkdown(lines);
  if (JSON.stringify(r) !== JSON.stringify(r2)) throw new Error("P7 two runs differ");
  if (JSON.stringify(lines) !== JSON.stringify(copy)) throw new Error("P7 the input changed");
  const chars = lines.reduce((a, l) => a + l.length + 1, 0);
  if (r.stats.steps > 6 * chars) throw new Error(`P3 ${r.stats.steps} steps for ${chars} characters`);
  if (!r.ok) {
    if (!["size", "depth", "inline", "nodes", "columns"].includes(r.limit)) throw new Error(`P1 limit ${r.limit}`);
    return { ok: false, steps: r.stats.steps, limit: r.limit };
  }
  const m = measure(r.tree); // P2
  if (m.block > MD_MAX_DEPTH || m.inl > MD_MAX_INLINE_DEPTH || m.nodes > MD_MAX_NODES) throw new Error(`P4 ${JSON.stringify(m)}`);
  const plain = buildDom(fakeDocument, r.tree, (t) => t);
  const hostile = buildDom(fakeDocument, r.tree, (t) => t.replace(/\$\{[^}]*\}|\$[A-Za-z0-9_]+/g, HOSTILE_VALUE));
  if (!plain.root) throw new Error("P5 no root");
  const hrefs = checkDom(plain.root, checkUrl); // P5
  if (hostile.root) {
    if (shapeOf(hostile.root) !== shapeOf(plain.root)) throw new Error("P6 the shape changed");
    if (JSON.stringify(checkDom(hostile.root, checkUrl)) !== JSON.stringify(hrefs)) throw new Error("P6 the destinations changed");
  }
  if (typeof textOf(plain.root) !== "string") throw new Error("P2 text");
  return { ok: true, steps: r.stats.steps };
}

const rep = (s, n) => s.repeat(Math.max(1, Math.ceil(n / s.length))).slice(0, n);
const lineOf = (s, n, count) => Array.from({ length: count }, () => s).join("\n").slice(0, n);

/** The 12 families of hostile texts. Each one gives a text of about `n` characters (65,536 at most, line ends included). */
export const FAMILIES = {
  "nested quotes": (n) => rep(">", n),
  "nested quote lines": (n) => lineOf("> > > > > > x", n, Math.ceil(n / 13)),
  "nested lists": (n) => Array.from({ length: Math.ceil(n / 20) }, (_, i) => `${" ".repeat(Math.min(2 * i, 40))}- x`).join("\n").slice(0, n),
  "open emphasis with star": (n) => rep("*a ", n),
  "open emphasis with underscore": (n) => rep("_a ", n),
  "mixed marks": (n) => rep("_a __b *c **d ", n),
  "brackets": (n) => rep("[", n),
  "bracket pairs with parentheses": (n) => rep("[a](", n),
  "backtick runs": (n) => { let s = ""; for (let k = 1; s.length < n; k++) s += `${"`".repeat(k)} `; return s.slice(0, n); },
  "backslashes": (n) => rep("\\", n),
  "url runs": (n) => rep("http://", n),
  "wide tables and short lines": (n) => `${"|a".repeat(Math.ceil(n / 4))}|\n${"|-".repeat(Math.ceil(n / 4))}|\n${rep("a\n", n / 2)}`.slice(0, n),
};
