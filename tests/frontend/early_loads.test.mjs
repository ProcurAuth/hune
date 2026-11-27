// SPDX-License-Identifier: MIT OR Apache-2.0
// The loads that start early (9.8, 10.2): the module of a panel type starts when the panel is wanted, and the editor starts its style sheet,
// its sources and its view module together.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../..");
const code = (path) => readFileSync(join(ROOT, path), "utf8").replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");

test("a panel asks for the module of its type when it is wanted or queried, and the draw takes the same import (9.8, 9.11)", () => {
  const frame = code("web/js/panel_frame.js");
  assert.match(frame, /const warm = \(\) => Object\.hasOwn\(LOADERS, panel\.type\) && LOADERS\[panel\.type\]\(\)\.catch\(\(\) => \{\}\);/);
  assert.match(frame, /if \(info\.state === "loading" \|\| info\.state === "queued"\) \{\s*warm\(\);/);
  assert.match(frame, /mod = await LOADERS\[panel\.type\]\(\);/, "the draw still awaits the import");
  assert.match(frame, /\n    warm,\n/, "the frame gives warm to the grid");
  // The grid calls it for a frame that comes near the viewport, never for one that leaves.
  assert.match(code("web/js/grid.js"), /if \(entry\.isIntersecting && frames\.has\(key\) && frames\.get\(key\)\.warm\) frames\.get\(key\)\.warm\(\);\s*rt\.setVisible\(key, entry\.isIntersecting\);/);
});

test("the panel modules have no work at load: no listener, no DOM write, no request at the top level", () => {
  const seen = new Set();
  const visit = (file) => {
    if (seen.has(file) || file.includes("/editor/")) return;
    seen.add(file);
    for (const m of readFileSync(file, "utf8").matchAll(/(?:from|import\()\s*["'](\.[^"']+)["']/g)) visit(join(dirname(file), m[1]));
  };
  for (const name of readdirSync(join(ROOT, "web/js/panels"))) visit(join(ROOT, "web/js/panels", name));
  assert.ok(seen.size > 20);
  const offenders = [];
  for (const file of seen) {
    let depth = 0;
    readFileSync(file, "utf8").split("\n").forEach((line, i) => {
      // A statement at the top level that is not a declaration, an import or an export: the only ones that are permitted fill a table or reset a counter.
      if (depth === 0 && /^(document|window|navigator|localStorage|addEventListener|queueMicrotask|setTimeout|fetch|new )/.test(line)) offenders.push(`${file}:${i + 1}`);
      depth += (line.match(/[{(\[]/g) || []).length - (line.match(/[})\]]/g) || []).length;
      if (depth < 0) depth = 0;
    });
  }
  assert.deepEqual(offenders, []);
});

test("the editor starts the style sheet, the sources and the module of the view together, and shows the view after all three (10.2)", () => {
  const edit = code("web/js/editor/edit_mode.js");
  const body = edit.slice(edit.indexOf("async prepare(target)"), edit.indexOf("// The first view: the runtime"));
  // The three loads start before the first await.
  const first = body.indexOf("await ");
  for (const start of ["loadEditorCss()", "this.loadSources()", "VIEW_MODULES[kind]()"]) assert.ok(body.indexOf(start) >= 0 && body.indexOf(start) < first, `${start} starts before the first await`);
  assert.match(body, /await css;\s*await sources;/);
  // The list of the sources is read at each entry of the edit mode, as before: the condition is the same.
  assert.match(body, /!this\.started && this\.sources\.length === 0 \? this\.loadSources\(\) : null/);
  // A failed module still fails where the page awaits it. The early failure has no unhandled rejection.
  assert.match(body, /load\.catch\(\(\) => \{\}\);/);
  assert.equal((body.match(/await load;/g) || []).length, 4);
  assert.match(edit, /panel: \(\) => import\("\.\/panel_editor\.js"\)/);
});
