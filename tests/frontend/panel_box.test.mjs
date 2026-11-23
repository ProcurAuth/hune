// SPDX-License-Identifier: MIT OR Apache-2.0
// The inner box of a panel part (9.19a): the sizes that stat and bar gauge give to their children always fit.
import { test } from "node:test";
import assert from "node:assert/strict";
import { LAYOUT_UNIT, floorUnit, innerSize } from "../../web/js/panels/box.js";
import { tileGrid } from "../../web/js/panels/stat.js";

test("floorUnit rounds down to 1/64 px and keeps a multiple as it is", () => {
  assert.equal(LAYOUT_UNIT, 1 / 64);
  assert.equal(floorUnit(335.828125), 335.828125);
  assert.equal(floorUnit(335.84), 335.828125);
  assert.equal(floorUnit(336), 336);
  assert.equal(floorUnit(0), 0);
  assert.equal(floorUnit(-3), 0);
  assert.equal(floorUnit(Number.NaN), 0);
});

test("innerSize reads the real box, not the rounded clientWidth, and takes away borders, padding and scroll bars", () => {
  const style = { borderLeftWidth: "1px", borderRightWidth: "1px", borderTopWidth: "0px", borderBottomWidth: "0px",
    paddingLeft: "2px", paddingRight: "2px", paddingTop: "0px", paddingBottom: "0px" };
  globalThis.getComputedStyle = () => style;
  try {
    // At zoom 90 %: the box is 335.83 px wide, clientWidth rounds up to 334 (336 less the borders).
    const el = { getBoundingClientRect: () => ({ width: 335.828125, height: 140 }), offsetWidth: 336, clientWidth: 334, offsetHeight: 140, clientHeight: 140 };
    assert.deepEqual(innerSize(el), { w: 329.828125, h: 140 });
    // A vertical scroll bar of 10 px.
    const scrolled = { ...el, clientWidth: 324 };
    assert.deepEqual(innerSize(scrolled), { w: 319.828125, h: 140 });
  } finally {
    delete globalThis.getComputedStyle;
  }
});

test("tileGrid: n tiles and their gaps of 3 px never take more than a fractional box", () => {
  const GAP = 3; // GAP of stat.js
  for (const size of [335.828125, 442.5, 1000.015625, 197.34375]) {
    for (const n of [2, 3, 5, 7]) {
      const row = tileGrid(n, size, 60, "vertical");
      if (row.tileW > 56) assert.ok(n * row.tileW + (n - 1) * GAP <= size, `${n} tiles side by side in ${size} px`);
      const col = tileGrid(n, 300, size, "horizontal");
      if (col.tileH > 24) assert.ok(n * col.tileH + (n - 1) * GAP <= size, `${n} tiles one below the other in ${size} px`);
    }
  }
});
