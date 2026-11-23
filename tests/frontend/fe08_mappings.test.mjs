// FE-08: value mappings (9.17).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { applyMappings } from "../../web/js/mappings.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-08-mappings.json", import.meta.url)));

test("FE-08 mappings", () => {
  for (const c of v.cases) {
    const value = c.v === "NaN" ? NaN : c.v;
    assert.deepEqual(applyMappings(c.m, value), c.expect, c.name);
  }
});
