// FE-17: line diff (10.15).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { diffLines, splitLines } from "../../web/js/editor/diff.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-17-diff.json", import.meta.url)));
const text = (lines) => lines.join("\n");

test("FE-17 hunks, headers and the three lines of context", () => {
  for (const c of v.cases) {
    const d = diffLines(text(c.a), text(c.b));
    assert.equal(d.tooLarge, false, c.name);
    assert.deepEqual(d.hunks.map((h) => ({ header: h.header, lines: h.lines.map((l) => l.op + l.text) })), c.hunks, c.name);
  }
});

test("FE-17 gutter line numbers of both sides", () => {
  const d = diffLines(text(v.gutters.a), text(v.gutters.b));
  assert.deepEqual(d.hunks[0].lines.map((l) => [l.aNo, l.bNo]), v.gutters.expect);
});

test("FE-17 applying the script to A gives B (random texts)", () => {
  let seed = 3;
  const rnd = (n) => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed % n; };
  for (let round = 0; round < 100; round++) {
    const a = Array.from({ length: rnd(30) }, () => String(rnd(6)));
    const b = Array.from({ length: rnd(30) }, () => String(rnd(6)));
    const d = diffLines(text(a), text(b), {});
    // Rebuild B from the hunks and the lines of A outside the hunks.
    const out = [];
    let ai = 0;
    for (const h of d.hunks) {
      const startA = h.aCount === 0 ? h.aStart : h.aStart - 1;
      while (ai < startA) out.push(a[ai++]);
      for (const l of h.lines) {
        if (l.op === "+") out.push(l.text);
        else { assert.equal(l.text, a[ai]); ai += 1; if (l.op === " ") out.push(l.text); }
      }
    }
    while (ai < a.length) out.push(a[ai++]);
    assert.deepEqual(out, splitLines(text(b)), `round ${round}: ${JSON.stringify([a, b])}`);
  }
});

test("FE-17 limits: too many lines, edit distance too large", () => {
  assert.equal(diffLines("a\nb", "a\nc", { maxLines: 1 }).tooLarge, true);
  assert.equal(diffLines("1\n2\n3\n4", "5\n6\n7\n8", { maxDistance: 3 }).tooLarge, true);
  assert.equal(diffLines("1\n2\n3\n4", "5\n6\n7\n8", { maxDistance: 8 }).tooLarge, false);
  const big = Array.from({ length: 5000 }, (_, i) => `line ${i}`).join("\n");
  assert.equal(diffLines(big, big.replace("line 2500", "changed")).hunks.length, 1);
});

test("FE-17 split at LF: a closing LF is not a line", () => {
  assert.deepEqual(splitLines("a\nb\n"), ["a", "b"]);
  assert.deepEqual(splitLines(""), []);
});
