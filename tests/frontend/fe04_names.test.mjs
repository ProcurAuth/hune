// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-04: display names (9.13).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { baseDisplayNames, displayNames } from "../../web/js/names.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-04-names.json", import.meta.url)));

test("FE-04 display names of the table of 9.13 and edge cases", () => {
  for (const c of v.cases) {
    const names = c.base ? baseDisplayNames(c.fields) : displayNames(c.fields, c.options);
    assert.deepEqual(names, c.base || c.expect, c.name);
    if (c.base) assert.deepEqual(displayNames(c.fields, c.options), c.expect, c.name);
  }
});
