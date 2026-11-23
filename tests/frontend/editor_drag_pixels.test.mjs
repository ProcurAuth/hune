// F15 (10.16): the pixel rules of drag.js. The values are measures of Grafana 12.4.2 on a grid of 1498 px (24 columns, 8 px gap, 30 px rows).
import test from "node:test";
import assert from "node:assert/strict";
import { cellAt, sizeAt } from "../../web/js/editor/drag.js";

const M = { gap: 8, rowGap: 8, colPitch: (1498 + 8) / 24, rowPitch: 38 };

test("F15 cellAt: the cell turns over one gap later than a plain rounding", () => {
  // Panel at the column 20 (left 1255 px) moved left by 20, 26 px: the column is 20, then 19.
  assert.equal(cellAt(1255 - 20, 0, M).x, 20);
  assert.equal(cellAt(1255 - 26, 0, M).x, 19);
  // Panel at the column 12 (left 753 px) moved right by 38 and 41 px: the column is 12, then 13.
  assert.equal(cellAt(753 + 38, 0, M).x, 12);
  assert.equal(cellAt(753 + 41, 0, M).x, 13);
  // The rows use the same rule with the pitch of 38 px.
  assert.equal(cellAt(0, 38 * 4 + 26, M).y, 4);
  assert.equal(cellAt(0, 38 * 4 + 28, M).y, 5);
  // Left of the grid and above it: the cell 0.
  assert.deepEqual(cellAt(-300, -50, M), { x: 0, y: 0 });
});

test("F15 sizeAt: a size turns over at half a pitch", () => {
  // A panel of 4 columns (243 px) and 5 rows (182 px): +31 px stays at 4 columns, +32 px gives 5; +19 px of height gives 6 rows.
  assert.equal(sizeAt(243 + 31, 182, M).w, 4);
  assert.equal(sizeAt(243 + 32, 182, M).w, 5);
  assert.equal(sizeAt(243, 182 + 18, M).h, 5);
  assert.equal(sizeAt(243, 182 + 19, M).h, 6);
  assert.deepEqual(sizeAt(243, 182, M), { w: 4, h: 5 });
});
