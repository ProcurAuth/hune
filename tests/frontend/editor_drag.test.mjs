// SPDX-License-Identifier: MIT OR Apache-2.0
// F15 (10.16): the model of drag and resize on the grid. Pure functions of layout.js, and the pixel rules of drag.js.
// The vectors below are measures of Grafana 12.4.2 in edit mode (the dashboard of the local reference, section "Overview", 24 columns).
// Each vector gives the panels, the panel that moves, the cells that the pointer crossed (a drag gives one cell after the other, and
// the layout of one step is the input of the next), and the layout after the drop.
import test from "node:test";
import assert from "node:assert/strict";
import { clampMove, clampSize, dragPanel, resizePanel, normalizeSection, overlaps, GRID_COLUMNS, MAX_HEIGHT } from "../../web/js/editor/layout.js";

// "id:x,y,w,h id:x,y,w,h ..." to panels, and back to a map.
const parse = (text) => text.split(" ").map((s) => { const [id, n] = s.split(":"); const [x, y, w, h] = n.split(",").map(Number); return { id: Number(id), grid: { x, y, w, h } }; });
const asText = (panels) => panels.map((p) => `${p.id}:${p.grid.x},${p.grid.y},${p.grid.w},${p.grid.h}`).join(" ");
const BASE = parse("1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,5,10,9 9:14,5,10,9 10:0,14,24,8");

// [name, moved id, cells, layout after the drop]
const DRAGS = [
  ["Active Requests: 1 columns, 0 rows", 1, "0,0 1,0", "1:1,0,4,5 2:4,5,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,10,10,9 9:14,5,10,9 10:0,19,24,8"],
  ["Active Requests: 3 columns, 0 rows", 1, "0,0 1,0 2,0 3,0", "1:3,0,4,5 2:4,5,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,10,10,9 9:14,5,10,9 10:0,19,24,8"],
  ["Closed Requests: -1 columns, 0 rows", 4, "12,0 11,0", "1:0,0,4,5 2:4,0,4,5 3:8,5,4,5 4:11,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,10,10,9 9:14,5,10,9 10:0,19,24,8"],
  ["Active Requests: 0 columns, 1 rows", 1, "0,0 0,1", "1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,5,10,9 9:14,5,10,9 10:0,14,24,8"],
  ["Active Requests: 0 columns, 2 rows", 1, "0,0 0,1 0,2", "1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,5,10,9 9:14,5,10,9 10:0,14,24,8"],
  ["Active Requests: 0 columns, 6 rows", 1, "0,0 0,1 0,2 0,3 0,4 0,5 0,6", "1:0,5,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,0,4,5 8:4,5,10,9 9:14,5,10,9 10:0,14,24,8"],
  ["Active Requests: 0 columns, 8 rows", 1, "0,0 0,1 0,2 0,3 0,4 0,5 0,6 0,7 0,8", "1:0,5,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,0,4,5 8:4,5,10,9 9:14,5,10,9 10:0,14,24,8"],
  ["Processed Jobs: 0 columns, -5 rows", 8, "4,4 4,3 4,2 4,1 4,0", "1:0,0,4,5 2:4,9,4,5 3:8,9,4,5 4:12,9,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,0,10,9 9:14,14,10,9 10:0,23,24,8"],
  ["Processed Jobs: 0 columns, -3 rows", 8, "4,5 4,4 4,3 4,2", "1:0,0,4,5 2:4,9,4,5 3:8,9,4,5 4:12,9,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,0,10,9 9:14,14,10,9 10:0,23,24,8"],
  ["Version by Region: 0 columns, -6 rows", 10, "0,13 0,12 0,11 0,10 0,9 0,8", "1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,5,10,9 9:14,5,10,9 10:0,14,24,8"],
  ["Version by Region: 0 columns, -9 rows", 10, "0,13 0,12 0,11 0,10 0,9 0,8 0,7 0,6 0,5", "1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,13,4,5 8:4,13,10,9 9:14,13,10,9 10:0,5,24,8"],
  ["Requests by Region: -4 columns, 0 rows", 9, "14,5 13,5 12,5 11,5 10,5", "1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,14,10,9 9:10,5,10,9 10:0,23,24,8"],
  ["Active Requests: -3 columns, -3 rows", 1, "0,0", "1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,5,10,9 9:14,5,10,9 10:0,14,24,8"],
  ["Active Requests: 30 columns, 0 rows", 1, "2,0 5,0 7,0 10,0 12,0 15,0 17,0 20,0", "1:20,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,5,4,5 7:0,0,4,5 8:4,5,10,9 9:14,10,10,9 10:0,19,24,8"],
  ["Active Requests: 4.4 columns, 0 rows", 1, "0,0 1,0 2,0 3,0 4,0", "1:4,0,4,5 2:4,5,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,0,4,5 8:4,10,10,9 9:14,5,10,9 10:0,19,24,8"],
  ["Active Requests: 4.6 columns, 0 rows", 1, "0,0 1,0 2,0 3,0 4,0", "1:4,0,4,5 2:4,5,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,0,4,5 8:4,10,10,9 9:14,5,10,9 10:0,19,24,8"],
  ["Upstream HTTP Health: 2 columns, -3 rows", 7, "0,5 0,4 1,4 1,3 2,2", "1:0,5,4,5 2:4,5,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:2,0,4,5 8:4,10,10,9 9:14,5,10,9 10:0,19,24,8"],
  ["Closed Requests: 0 columns, 4 rows", 4, "12,0 12,1 12,2 12,3 12,4", "1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,5,10,9 9:14,5,10,9 10:0,14,24,8"],
  ["Closed Requests: 0 columns, 9 rows", 4, "12,1 12,2 12,3 12,4 12,5 12,6 12,7 12,8 12,9", "1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:12,14,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,5,10,9 9:14,19,10,9 10:0,28,24,8"],
  ["Closed Requests: 2 columns, 6 rows", 4, "12,0 12,1 13,2 13,3 13,4 14,5 14,6", "1:0,0,4,5 2:4,0,4,5 3:8,0,4,5 4:14,5,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,5,10,9 9:14,10,10,9 10:0,19,24,8"],
];

test("F15 drag: the layouts of Grafana for the same path of the pointer", () => {
  for (const [name, id, cells, expect] of DRAGS) {
    let panels = BASE;
    let cell = null;
    for (const step of cells.split(" ")) {
      const [x, y] = step.split(",").map(Number);
      const r = dragPanel(panels, id, x, y);
      panels = r.panels;
      cell = r.grid;
    }
    assert.equal(asText(panels), expect, name);
    // The cell of the placeholder is the cell of the moved panel.
    assert.deepEqual(cell, panels.find((p) => p.id === id).grid, name);
    // The compaction of the drop (10.5) changes nothing more.
    assert.equal(asText(normalizeSection(panels)), expect, name);
  }
});

test("F15 drag: collision pushes the panels below, and the gravity pulls them up", () => {
  // Panel 1 goes right by one column over panel 2: panel 2 goes below it, and panel 8 (under panel 2) goes below panel 2.
  const a = dragPanel(BASE, 1, 1, 0).panels;
  assert.deepEqual(a.find((p) => p.id === 2).grid, { x: 4, y: 5, w: 4, h: 5 });
  assert.deepEqual(a.find((p) => p.id === 8).grid, { x: 4, y: 10, w: 10, h: 9 });
  // Panel 1 goes back: the panels go up to their first cells.
  const b = dragPanel(a, 1, 0, 0).panels;
  assert.equal(asText(b), asText(BASE));
  // A move below free cells: the panel falls back up (gravity).
  assert.deepEqual(dragPanel(BASE, 1, 0, 2).grid, { x: 0, y: 0, w: 4, h: 5 });
});

test("F15 drag: a panel that leaves a cell above another panel swaps with it when there is room", () => {
  // Panel 7 (0,5) goes up to row 4 over panel 1 (0,0): panel 1 goes below panel 7.
  const r = dragPanel(BASE, 7, 0, 4);
  assert.deepEqual(r.grid, { x: 0, y: 0, w: 4, h: 5 });
  assert.deepEqual(r.panels.find((p) => p.id === 1).grid, { x: 0, y: 5, w: 4, h: 5 });
});

test("F15 drag: the target stays in the section (clampMove)", () => {
  const g = { x: 3, y: 2, w: 4, h: 5 };
  assert.deepEqual(clampMove(g, -4, -1), { x: 0, y: 0 });
  assert.deepEqual(clampMove(g, 99, 10001), { x: 20, y: 10000 });
  assert.deepEqual(clampMove(g, 7.4, 3.6), { x: 7, y: 4 });
  assert.deepEqual(clampMove(g, NaN, undefined), { x: 3, y: 2 });
  const wide = dragPanel(BASE, 10, 5, 14);
  assert.equal(wide.grid.x, 0, "a panel of 24 columns cannot leave the column 0");
});

test("F15 resize: Grafana pushes the panels that the new size meets below the panel", () => {
  assert.equal(asText(resizePanel(BASE, 1, 6, 8).panels), "1:0,0,6,8 2:4,8,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,8,4,5 8:4,13,10,9 9:14,5,10,9 10:0,22,24,8");
  assert.equal(asText(resizePanel(BASE, 1, 9, 5).panels), "1:0,0,9,5 2:4,5,4,5 3:8,5,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,5,4,5 8:4,10,10,9 9:14,5,10,9 10:0,19,24,8");
  // The smallest panel is 1 by 1, and the panels under it go up.
  assert.equal(asText(resizePanel(BASE, 1, 0, 0).panels), "1:0,0,1,1 2:4,0,4,5 3:8,0,4,5 4:12,0,4,5 5:16,0,4,5 6:20,0,4,5 7:0,1,4,5 8:4,5,10,9 9:14,5,10,9 10:0,14,24,8");
  // A shrink brings the panels back up.
  const grown = resizePanel(BASE, 1, 9, 5).panels;
  assert.equal(asText(resizePanel(grown, 1, 4, 5).panels), asText(BASE));
});

test("F15 resize: the size stays in the section (clampSize)", () => {
  const g = { x: 20, y: 0, w: 4, h: 5 };
  assert.deepEqual(clampSize(g, 0, 0), { w: 1, h: 1 });
  assert.deepEqual(clampSize(g, 24, 999), { w: 4, h: MAX_HEIGHT });
  assert.deepEqual(clampSize(g, 2.6, 7.2), { w: 3, h: 7 });
  assert.deepEqual(clampSize({ ...g, x: 0 }, 99, 5), { w: GRID_COLUMNS, h: 5 });
  assert.deepEqual(clampSize(g, NaN, undefined), { w: 4, h: 5 });
  assert.equal(resizePanel(BASE, 1, 99, 999).grid.w, 24);
  assert.equal(resizePanel(BASE, 1, 99, 999).grid.h, 200);
});

test("F15: the functions never change their input and answer null for an unknown panel", () => {
  const before = JSON.stringify(BASE);
  dragPanel(BASE, 1, 5, 3);
  resizePanel(BASE, 2, 9, 9);
  assert.equal(JSON.stringify(BASE), before);
  assert.equal(dragPanel(BASE, 99, 0, 0), null);
  assert.equal(resizePanel(BASE, 99, 1, 1), null);
  // A panel that does not change keeps its object (the draft shares it).
  const r = dragPanel(BASE, 1, 0, 0);
  r.panels.forEach((p, i) => assert.equal(p, BASE[i]));
  // The fields other than the grid stay.
  const rich = BASE.map((p) => ({ ...p, title: `t${p.id}`, type: "stat" }));
  for (const p of dragPanel(rich, 1, 2, 2).panels) assert.equal(p.title, `t${p.id}`);
});

test("F15: any path of drags and resizes ends with no overlap, in the section, and equal to the normalization of 10.5", () => {
  let seed = 31;
  const rnd = (n) => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed % n; };
  for (let round = 0; round < 300; round++) {
    let panels = normalizeSection(Array.from({ length: 2 + rnd(9) }, (_, i) => { const w = 1 + rnd(12); return { id: i + 1, grid: { x: rnd(24 - w + 1), y: rnd(20), w, h: 1 + rnd(6) } }; }));
    const id = 1 + rnd(panels.length);
    const steps = 1 + rnd(8);
    for (let step = 0; step < steps; step++) {
      const start = panels.find((p) => p.id === id).grid;
      const drag = rnd(3) > 0;
      const r = drag ? dragPanel(panels, id, rnd(24), rnd(25)) : resizePanel(panels, id, 1 + rnd(24), 1 + rnd(10));
      panels = r.panels;
      const g = r.grid;
      assert.ok(g.x >= 0 && g.y >= 0 && g.w >= 1 && g.h >= 1 && g.x + g.w <= 24 && g.h <= 200, `in the section, round ${round}`);
      if (drag) assert.deepEqual([g.w, g.h], [start.w, start.h], "a drag keeps the size");
      else assert.deepEqual([g.x], [start.x], "a resize keeps the column");
    }
    for (let i = 0; i < panels.length; i++) for (let j = i + 1; j < panels.length; j++) assert.ok(!overlaps(panels[i].grid, panels[j].grid), `overlap in round ${round}`);
    assert.deepEqual(normalizeSection(panels), panels, `compact in round ${round}`);
  }
});
