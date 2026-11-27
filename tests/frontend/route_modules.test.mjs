// SPDX-License-Identifier: MIT OR Apache-2.0
// The modules that each route loads (9.30). The dashboard page does not load the list page, and the editor reads the file helper from its own module.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join, normalize, relative } from "node:path";
import { fileURLToPath } from "node:url";

const WEB = join(dirname(fileURLToPath(import.meta.url)), "../../web/js");

// The files that `entry` loads by a static import or by `import()` with a quoted path, except the editor.
function closure(entry) {
  const seen = new Set();
  const visit = (file) => {
    if (seen.has(file) || file.includes("/editor/")) return;
    seen.add(file);
    for (const m of readFileSync(file, "utf8").matchAll(/(?:from|import\()\s*["'](\.[^"']+)["']/g)) visit(normalize(join(dirname(file), m[1])));
  };
  visit(join(WEB, entry));
  return [...seen].map((f) => relative(WEB, f));
}

test("the dashboard page does not load list.js, and the list page does not load dashboard.js", () => {
  assert.ok(!closure("dashboard.js").includes("list.js"), "dashboard.js reaches list.js");
  assert.ok(closure("dashboard.js").includes("export_file.js"));
  assert.ok(!closure("list.js").includes("dashboard.js"), "list.js reaches dashboard.js");
});

test("the file helpers are in export_file.js, and every user imports them from there", () => {
  const src = readFileSync(join(WEB, "export_file.js"), "utf8");
  assert.match(src, /export function saveTextFile\(/);
  assert.match(src, /export async function exportDashboard\(/);
  for (const file of ["dashboard.js", "list.js"]) assert.match(readFileSync(join(WEB, file), "utf8"), /import \{ exportDashboard \} from "\.\/export_file\.js";/);
  for (const file of ["editor/diff_view.js", "editor/history.js", "editor/conflict.js"]) assert.match(readFileSync(join(WEB, file), "utf8"), /import \{ saveTextFile \} from "\.\.\/export_file\.js";/);
  assert.match(readFileSync(join(WEB, "switcher.js"), "utf8"), /import \{ groupByStore, groupDashboards, isRootGroup, rootGroupCount \} from "\.\/stores\.js";/);
});
