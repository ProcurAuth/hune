// SPDX-License-Identifier: MIT OR Apache-2.0
// The properties P1 to P7 on seeded random texts, and the bound of steps on 12 families of hostile texts at 3 sizes (9.19b, 16.10a).
import test from "node:test";
import assert from "node:assert/strict";
import { FAMILIES, checkProperties, makeRng, randomText } from "./markdown_properties.mjs";

for (let seed = 1; seed <= 8; seed++) {
  test(`P1 to P7 on 5,000 random texts, seed ${seed}`, () => {
    const rng = makeRng(seed);
    let ok = 0;
    for (let i = 0; i < 5000; i++) {
      const text = randomText(rng);
      try {
        if (checkProperties(text).ok) ok++;
      } catch (e) {
        assert.fail(`seed ${seed}, text ${i} ${JSON.stringify(text)}: ${e.message}`);
      }
    }
    assert.ok(ok > 1000, `only ${ok} texts gave a tree`);
  });
}

test("the generator is deterministic", () => {
  const a = makeRng(3);
  const b = makeRng(3);
  for (let i = 0; i < 50; i++) assert.equal(randomText(a), randomText(b));
});

test("12 families of hostile texts at 4,096, 16,384 and 65,536 characters keep the bound of 6 steps for each character", () => {
  assert.equal(Object.keys(FAMILIES).length, 12);
  for (const [name, make] of Object.entries(FAMILIES)) {
    for (const n of [4096, 16384, 65536]) {
      const text = make(n);
      assert.ok(text.length <= 65536, `${name} ${n}: ${text.length}`);
      const t0 = performance.now();
      const r = checkProperties(text);
      const ms = performance.now() - t0;
      assert.ok(r.steps <= 6 * (text.length + 1), `${name} ${n}`);
      assert.ok(ms < 1000, `${name} ${n}: ${ms} ms`);
    }
  }
});
