// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-73: merge and join_by_field on lists of frames (9.21a, rules 6 and 7).
import test from "node:test";
import assert from "node:assert/strict";
import { measure, measureRun, run, vectors } from "./more_helpers.mjs";
// The session gives the cap of the cells of a transform (6.10): the default of `server.transform_max_cells`.
import "./session_cells.mjs";

test("FE-73 vectors of the file FE-73-merge-join.json", () => {
  const v = vectors("FE-73-merge-join.json");
  for (const c of v.cases) assert.deepEqual(run(c.frames, c.transform), c.expect, c.name);
});

test("FE-73 the cases of the measure of Grafana: same, or a difference that 19.20j lists", () => {
  const cases = measure.filter((c) => /^(m|j)\d+$/.test(c.id));
  assert.ok(cases.length > 0);
  for (const c of cases) {
    if (c.class === "not_mapped") continue;
    const got = measureRun(c);
    if (c.class === "same") assert.deepEqual(got, c.grafana, c.id);
    else {
      assert.deepEqual(got, c.got, c.id);
      assert.notDeepEqual(got, c.grafana, c.id);
    }
  }
});

