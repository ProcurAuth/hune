// FE-16: layout normalization (10.5).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { normalizeSection, movePanel, setPosition, positionErrors, canMove, bottomOf } from "../../web/js/editor/layout.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-16-layout.json", import.meta.url)));
const shape = (panels) => panels.map((p) => [p.id, p.grid.x, p.grid.y]);

test("FE-16 normalization", () => {
  for (const c of v.normalize) assert.deepEqual(shape(normalizeSection(c.panels)), c.expect, c.name);
});

test("FE-16 normalization never changes x, w and h, never changes its input, and is idempotent", () => {
  for (const c of v.normalize) {
    const before = JSON.stringify(c.panels);
    const once = normalizeSection(c.panels);
    assert.equal(JSON.stringify(c.panels), before);
    once.forEach((p, i) => assert.deepEqual([p.grid.x, p.grid.w, p.grid.h], [c.panels[i].grid.x, c.panels[i].grid.w, c.panels[i].grid.h]));
    assert.deepEqual(normalizeSection(once), once);
  }
  // A pseudo random layout: no overlap after normalization.
  let seed = 7;
  const rnd = (n) => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed % n; };
  for (let round = 0; round < 50; round++) {
    const panels = Array.from({ length: 8 }, (_, i) => { const w = 1 + rnd(12); return { id: i + 1, grid: { x: rnd(24 - w + 1), y: rnd(20), w, h: 1 + rnd(6) } }; });
    const out = normalizeSection(panels);
    for (let i = 0; i < out.length; i++) for (let j = i + 1; j < out.length; j++) {
      const a = out[i].grid; const b = out[j].grid;
      assert.ok(!(a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h), `overlap in round ${round}`);
    }
  }
});

test("FE-16 move up and move down, position, bottom", () => {
  for (const c of v.moves) {
    const got = movePanel(c.panels, c.id, c.dir);
    assert.deepEqual(got === null ? null : shape(got), c.expect, c.name);
    assert.equal(canMove(c.panels, c.id, c.dir), c.expect !== null);
  }
  for (const c of v.position) assert.deepEqual(shape(setPosition(c.panels, c.id, c.grid)), c.expect, c.name);
  for (const c of v.positionErrors) assert.equal(positionErrors(c.grid).length, c.errors, JSON.stringify(c.grid));
  assert.equal(bottomOf([]), 0);
  assert.equal(bottomOf([{ id: 1, grid: { x: 0, y: 3, w: 1, h: 4 } }]), 7);
});
