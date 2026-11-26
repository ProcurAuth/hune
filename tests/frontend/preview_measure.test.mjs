// SPDX-License-Identifier: MIT OR Apache-2.0
// The comparison of the measurement harness of the previews (tests/browser/lib/preview_measure.mjs): the tolerance classes, the matching of the parts
// and the table. The probes need a browser: the browser case B320 and the command preview_measure.mjs run them.
import assert from "node:assert/strict";
import test from "node:test";
import { CLASSES, TOLERANCE_PX, compare, compareRect, formatTable, summarize } from "../browser/lib/preview_measure.mjs";

test("an exact part agrees within half a pixel on each of x, y, width and height", () => {
  assert.equal(TOLERANCE_PX, 0.5);
  assert.equal(compareRect(CLASSES.EXACT, [10, 20, 100, 50], [10.5, 19.5, 100, 50.5]).verdict, "ok");
  assert.equal(compareRect(CLASSES.EXACT, [10, 20, 100, 50], [10, 20, 100, 51]).verdict, "fail");
  assert.deepEqual(compareRect(CLASSES.EXACT, [10, 20, 100, 50], [9, 20, 100, 50]).delta, [1, 0, 0, 0]);
});

test("a one-sided bound limits the size and not the place", () => {
  // least: the preview takes at most the room of the loaded part.
  assert.equal(compareRect(CLASSES.LEAST, [0, 0, 30, 20], [5, 5, 40, 20]).verdict, "ok");
  assert.equal(compareRect(CLASSES.LEAST, [0, 0, 41, 20], [5, 5, 40, 20]).verdict, "fail");
  assert.equal(compareRect(CLASSES.LEAST, [0, 0, 40.4, 20], [5, 5, 40, 20]).verdict, "ok", "within the tolerance");
  // most: the preview takes at least the room of the loaded part.
  assert.equal(compareRect(CLASSES.MOST, [0, 0, 50, 30], [0, 0, 40, 20]).verdict, "ok");
  assert.equal(compareRect(CLASSES.MOST, [0, 0, 39, 30], [0, 0, 40, 20]).verdict, "fail");
});

test("a data part has no shape and no comparison", () => {
  assert.equal(compareRect(CLASSES.DATA, [0, 0, 1, 1], [9, 9, 9, 9]).verdict, "data");
});

const preview = (parts) => ({ id: 2, type: "timeseries", family: "timeseries", parts });
const loaded = (parts) => ({ id: 2, type: "timeseries", parts });

test("the parts match by name and by order, and each part gives a row", () => {
  const rows = compare(
    preview([
      { part: "plot", cls: "exact", rect: [47, 8, 615, 126] },
      { part: "legend", cls: "least", rect: [0, 155, 670, 40] },
      { part: "label", cls: "exact", rect: [0, 0, 20, 8] },
      { part: "label", cls: "exact", rect: [0, 12, 20, 8] },
    ]),
    loaded([
      { part: "plot", rect: [47, 8, 615, 126] },
      { part: "legend", rect: [0, 155, 670, 83] },
      { part: "label", rect: [0, 0, 20, 9], text: "a" },
      { part: "label", rect: [0, 12, 25, 8], text: "b" },
      { part: "axis-x", rect: [1, 143, 600, 15] },
    ]),
  );
  assert.deepEqual(rows.map((r) => [r.part, r.index, r.verdict]), [
    ["plot", 0, "ok"],
    ["legend", 0, "ok"],
    ["label", 0, "fail"],
    ["label", 1, "fail"],
    ["axis-x", 0, "unclaimed"],
  ]);
  assert.deepEqual(summarize(rows), { ok: 2, fail: 2, data: 0, "no-loaded": 0, unclaimed: 1 });
});

test("a claimed part with no loaded part is no-loaded, and a data part stays data", () => {
  const rows = compare(preview([{ part: "plot", cls: "exact", rect: [0, 0, 1, 1] }, { part: "mark", cls: "data", rect: [0, 0, 1, 1] }]), loaded([]));
  assert.deepEqual(rows.map((r) => r.verdict), ["no-loaded", "data"]);
});

test("a neutral preview claims nothing, so each loaded part is a target", () => {
  const rows = compare(preview([]), loaded([{ part: "plot", rect: [1, 2, 3, 4] }, { part: "legend", rect: [5, 6, 7, 8] }]));
  assert.deepEqual(rows.map((r) => r.verdict), ["unclaimed", "unclaimed"]);
  assert.equal(rows[0].type, "timeseries");
});

test("a panel with no preview or no loaded panel still compares", () => {
  assert.deepEqual(compare(null, loaded([{ part: "box", rect: [0, 0, 1, 1] }])).map((r) => r.verdict), ["unclaimed"]);
  assert.deepEqual(compare(preview([{ part: "plot", cls: "exact", rect: [0, 0, 1, 1] }]), null).map((r) => r.verdict), ["no-loaded"]);
});

test("the table has one line for each row, and hides the unclaimed labels unless asked", () => {
  const rows = compare(preview([]), loaded([{ part: "plot", rect: [1, 2, 3, 4] }, { part: "label", rect: [1, 2, 3, 4], text: "x" }]));
  const short = formatTable(rows).split("\n");
  assert.equal(short.length, 3, "the head, the rule and one row");
  assert.match(short[0], /^panel\s+type\s+part\s+#\s+class\s+verdict\s+preview\s+loaded\s+delta$/);
  assert.equal(formatTable(rows, { all: true }).split("\n").length, 4);
  assert.match(short[2], /plot\s+0\s+-\s+unclaimed\s+-\s+\[1, 2, 3, 4\]/);
});
