// FE-15: panel reuse (9.23), with the worked document 4.40.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { preparePanelFields } from "../../web/js/transform.js";
import { computeFieldConfig } from "../../web/js/fields.js";
import { reduceField, selectReduceFields } from "../../web/js/reduce.js";
import { baseDisplayNames } from "../../web/js/names.js";
import { frameOf } from "./helpers.mjs";
// The session gives the cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
import "./session_cells.mjs";

const doc = JSON.parse(readFileSync(new URL("../fixtures/dashboards/nodes.json", import.meta.url)));
const panels = Object.fromEntries(doc.sections.flatMap((s) => s.panels).map((p) => [p.id, p]));

const v = JSON.parse(readFileSync(new URL("./vectors/FE-15-reuse.json", import.meta.url)));
const tableFrame = frameOf(v.tableFrame);

test("FE-15 the reuse panel takes the source frame and the source transforms first", () => {
  assert.ok(panels[3].data.reuse.with_transforms);
  const fields = preparePanelFields(panels[3], tableFrame, panels[2]);
  // The source transform removes the row whose status is null.
  assert.deepEqual(fields[0].values, v.expect.withTransforms);
  assert.equal(fields[2].values.length, 2);
});

test("FE-15 with_transforms false skips the source transforms", () => {
  const reuse = structuredClone(panels[3]);
  reuse.data.reuse.with_transforms = false;
  assert.deepEqual(preparePanelFields(reuse, tableFrame, panels[2])[0].values, v.expect.withoutTransforms);
});

test("FE-15 the reuse transforms apply after the source transforms; configuration is the reuse panel's", () => {
  const reuse = structuredClone(panels[3]);
  reuse.transforms = [{ kind: "filter_by_value", mode: "include", match: "any", filters: [{ field: "node", matcher: { kind: "equal", value: "v2" } }] }];
  const fields = preparePanelFields(reuse, tableFrame, panels[2]);
  assert.deepEqual(fields[0].values, v.expect.reuseFilter);
  const { fields: configured } = computeFieldConfig(fields, reuse.fields, "stat");
  assert.equal(configured[2].config.unit, "suffix: pts");
  assert.equal(configured[2].config.decimals, 0);
  // The stat of the fixture reduces "weight" with total.
  const names = baseDisplayNames(configured);
  const idx = selectReduceFields(configured, names, reuse.options.reduce.fields);
  assert.deepEqual(idx, [2]);
  assert.equal(reduceField(configured[2], "total").value, v.expect.reuseTotalWeight);
});

test("FE-15 the source long-to-wide applies before a reuse of a time series", () => {
  const source = panels[1];
  const frame = frameOf([
    { name: "t", kind: "time", values: [100, 100, 200] },
    { name: "node", kind: "text", values: ["a1", "b2", "a1"] },
    { name: "weight", kind: "number", values: [10, 20, 11] },
  ]);
  const reuse = { id: 9, type: "stat", data: { reuse: { panel: 1, with_transforms: false } }, transforms: [] };
  const fields = preparePanelFields(reuse, frame, source);
  assert.deepEqual(baseDisplayNames(fields), ["t", "weight a1", "weight b2"]);
});

test("FE-15 a reuse without a source panel is an error", () => {
  assert.throws(() => preparePanelFields(panels[3], tableFrame, null));
});
