// SPDX-License-Identifier: MIT OR Apache-2.0
// The fuzz tool of the Markdown of the text panel (16.10a, job `nightly`). It runs the properties P1 to P7 on random texts.
// Usage: node tests/frontend/tools/markdown_fuzz.mjs [seed] [count] [seconds]
// The seed and the count come from the arguments. A third argument gives a time in seconds: the tool then runs again with the next seed
// until the time ends. It prints the seed and the text of the first failure, and exits with 1.
import { checkProperties, makeRng, randomText } from "../markdown_properties.mjs";

const seed0 = Number(process.argv[2] ?? 1);
const count = Number(process.argv[3] ?? 100000);
const seconds = Number(process.argv[4] ?? 0);
if (![seed0, count, seconds].every(Number.isFinite) || count < 1) {
  console.error("Usage: node markdown_fuzz.mjs [seed] [count] [seconds]");
  process.exit(2);
}
const end = Date.now() + seconds * 1000;
let total = 0;
let trees = 0;
for (let seed = seed0; ; seed++) {
  const rng = makeRng(seed);
  for (let i = 0; i < count; i++) {
    const text = randomText(rng);
    try {
      if (checkProperties(text).ok) trees++;
    } catch (e) {
      console.error(`FAIL seed ${seed}, text ${i}: ${e.message}\n${JSON.stringify(text)}`);
      process.exit(1);
    }
    total++;
  }
  console.log(`seed ${seed}: ${count} texts ok`);
  if (Date.now() >= end) break;
}
console.log(`done: ${total} texts, ${trees} trees, first seed ${seed0}`);
