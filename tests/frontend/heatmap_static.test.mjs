// The page module of the heatmap (9.19e): it writes data in the page with textContent and canvas pixels only, it has 12 % of comments or more,
// and the frame knows its loader, its range and the note of "View data".
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const read = (f) => readFileSync(new URL(`../../web/js/${f}`, import.meta.url), "utf8");
const code = (f) => read(f).replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");

test("panels/heatmap.js writes no markup, makes no request and keeps 12 % of comments", () => {
  const src = read("panels/heatmap.js");
  for (const re of [/innerHTML/, /outerHTML/, /insertAdjacentHTML/, /createElementNS/, /\bfetch\s*\(/, /\beval\s*\(/, /\bFunction\s*\(/, /\.style\.cssText/, /setAttribute\(\s*["']style["']/]) assert.ok(!re.test(code("panels/heatmap.js")), `${re}`);
  const comment = [...src.matchAll(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g)].reduce((n, m) => n + Buffer.byteLength(m[0]), 0);
  assert.ok(comment / Buffer.byteLength(src) >= 0.12, `${comment}`);
  // It reads the engine through common.js only (9.24), and only heatmap.js imports heat_model.js (ID-K195b).
  assert.ok(!/vendor\//.test(code("panels/heatmap.js")));
  assert.ok(/heat_model/.test(code("panels/heatmap.js")) && !/heat_model/.test(code("panels/histogram.js")));
});

test("the panel frame loads the heatmap, gives it the range of the panel and shows its note in View data", () => {
  const frame = code("panel_frame.js");
  assert.match(frame, /heatmap: \(\) => import\("\.\/panels\/heatmap\.js"\)/);
  assert.match(frame, /panel\.type === "heatmap"\) && typeof rt\.rangeOf/);
  assert.match(frame, /instance\.note/);
  assert.match(code("queries.js"), /JOINED = new Set\(\[[^\]]*"heatmap"/);
});
