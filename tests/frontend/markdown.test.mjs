// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-28: the grammar of the Markdown of the text panel (9.19b, 16.10a). The vectors have the lines and the expected tree.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { MD_MAX_COLUMNS, MD_MAX_DEPTH, MD_MAX_EXPANDED_BYTES, MD_MAX_INLINE_DEPTH, MD_MAX_NODES, MD_MAX_URL_BYTES, MAX_TEXT_BYTES, MAX_TEXT_LINES, parseMarkdown } from "../../web/js/markdown.js";
import { MD_KINDS, expandLines, serialize } from "./markdown_helpers.mjs";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-28-markdown.json", import.meta.url)));
const COUNTS = { blocks: 140, inline: 140, links: 60, tables: 20, items: 15, escapes: 15, limits: 20 };

test("FE-28 has 7 sets and the counts of 16.10a", () => {
  assert.deepEqual(Object.keys(v).sort(), Object.keys(COUNTS).sort());
  for (const [name, min] of Object.entries(COUNTS)) assert.ok(v[name].length >= min, `${name}: ${v[name].length}`);
  assert.ok(Object.values(v).flat().length >= 400);
});

for (const [set, list] of Object.entries(v)) {
  test(`FE-28 set ${set}: tree, steps and input`, () => {
    for (const c of list) {
      const lines = expandLines(c.lines);
      const copy = lines.slice();
      const r = parseMarkdown(lines);
      assert.deepEqual(lines, copy, `${c.name}: the input changed`);
      const got = r.ok ? (c.expect === "ok" ? "ok" : serialize(r.tree)) : `limit:${r.limit}`;
      assert.equal(got, c.expect, c.name);
      const chars = lines.reduce((a, l) => a + l.length + 1, 0);
      assert.ok(r.stats.steps <= 6 * chars, `${c.name}: ${r.stats.steps} steps for ${chars} characters`);
      if (r.ok) assert.ok(r.stats.nodes <= MD_MAX_NODES, c.name);
    }
  });
}

test("each limit has a vector at the maximum and a vector one above (FE-28)", () => {
  const names = v.limits.map((c) => c.name);
  for (const key of ["lines", "bytes", "depth", "inline", "nodes", "columns", "url"]) {
    assert.ok(names.some((n) => n.includes(key) && /allowed|below/.test(n)), `${key} at the maximum`);
    assert.ok(names.some((n) => n.includes(key) && !/allowed|below/.test(n)), `${key} one above`);
  }
});

test("the kinds of the tree are the kinds of the list", () => {
  const seen = new Set();
  const walk = (n) => {
    seen.add(n.k);
    for (const c of n.c || []) walk(c);
    for (const c of n.head || []) walk(c);
    for (const r of n.rows || []) for (const c of r) walk(c);
  };
  for (const c of [...v.blocks, ...v.inline, ...v.tables]) {
    const r = parseMarkdown(expandLines(c.lines));
    if (r.ok) r.tree.forEach(walk);
  }
  for (const k of seen) assert.ok(MD_KINDS.includes(k), k);
  for (const k of MD_KINDS) assert.ok(seen.has(k), `no vector makes ${k}`);
});

test("the constants are the tables of 4.35 and 9.19b", () => {
  assert.deepEqual([MAX_TEXT_LINES, MAX_TEXT_BYTES, MD_MAX_DEPTH, MD_MAX_INLINE_DEPTH, MD_MAX_NODES, MD_MAX_COLUMNS, MD_MAX_URL_BYTES, MD_MAX_EXPANDED_BYTES], [2000, 65536, 6, 6, 4000, 20, 2048, 262144]);
});

test("a text with U+0000 and a tab at the start is parsed as text", () => {
  const r = parseMarkdown(["a\u0000b", "\tc"]);
  assert.ok(r.ok);
  assert.equal(serialize(r.tree), "<p>a\u0000b c</p>");
});
