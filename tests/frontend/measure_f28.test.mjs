// SPDX-License-Identifier: MIT OR Apache-2.0
// The measure of Grafana 12.4.2 for the 11 more transforms (docs/08, 19.20j): 240 cases of the worker f2827-spec (FE-70 to FE-74 use them).
import test from "node:test";
import assert from "node:assert/strict";
import { measure, measureRun } from "./more_helpers.mjs";
// The session gives the cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
import "./session_cells.mjs";

// The closed list of the differences. `listed` is true when 19.20j or 15.6 has a line for it.
const TAGS = {
  "include-by-name": true, "text-operand": true, "reduce-row-nulls": true, "sort-null-order": true, "sort-text-order": true, "sort-boolean": true,
  "merge-two-types": true, "reduce-count-and-kinds": true, "reduce-each-frame": true, "partition-each-frame": true, "join-no-key": true,
  "convert-strict": true, "convert-time-zone": true, "two-frames-display-name": true,
  "sort-first-key-only": false, "old-name-after-rename": false, "convert-second-conversion": false,
};
const NOT_MAPPED = new Set(["operand-fixed", "operator", "operand-empty", "reducer", "dropped", "fields", "no-reducer", "outerTabular", "pattern", "regex-absent", "no-conversion", "null-as-zero"]);

test("measure-f28: 240 cases, each is the same as Grafana, a listed difference, or not mapped by the importer", () => {
  assert.equal(measure.length, 240);
  const count = { same: 0, differs: 0, not_mapped: 0 };
  for (const c of measure) {
    assert.ok(c.class in count, `${c.id}: class ${c.class}`);
    count[c.class] += 1;
    if (c.class === "not_mapped") {
      assert.ok(NOT_MAPPED.has(c.why), `${c.id}: ${c.why}`);
      continue;
    }
    const got = measureRun(c);
    if (c.class === "same") {
      assert.deepEqual(got, c.grafana, c.id);
    } else {
      assert.ok(c.tag in TAGS, `${c.id}: the tag ${c.tag} is not in the closed list`);
      assert.deepEqual(got, c.got, c.id);
      assert.notDeepEqual(got, c.grafana, `${c.id} is the same as Grafana`);
    }
  }
  assert.deepEqual(count, { same: 176, differs: 40, not_mapped: 24 });
});

test("measure-f28: each tag of the closed list is used", () => {
  const used = new Set(measure.filter((c) => c.class === "differs").map((c) => c.tag));
  assert.deepEqual([...used].sort(), Object.keys(TAGS).sort());
});
