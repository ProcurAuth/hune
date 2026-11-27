// SPDX-License-Identifier: MIT OR Apache-2.0
// The load on demand of the preview families (9.11): web/js/preview/index.js. The registry holds no static import of a family module.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { loadFamily, loadPreviewFamilies, previewOf } from "../../web/js/preview/index.js";

const FAMILIES = ["timeseries", "barchart", "piechart", "stat", "gauge", "bargauge", "table", "heatmap", "histogram", "timeline", "logs", "geomap"];
const source = readFileSync(new URL("../../web/js/preview/index.js", import.meta.url), "utf8");

test("the registry imports each family with a quoted path and no static import", () => {
  for (const family of FAMILIES) {
    assert.ok(source.includes(`import("./${family}.js")`), family);
    assert.ok(!new RegExp(`^import .* from "\\./${family}\\.js"`, "m").test(source), `${family} has a static import`);
  }
});

test("previewOf gives null until the module of the family is loaded, then it draws", async () => {
  const pie = { id: 1, type: "piechart", options: {}, fields: {} };
  assert.equal(previewOf(pie, { w: 300, h: 200 }, {}, {}), null);
  const first = loadFamily("piechart");
  assert.equal(loadFamily("piechart"), first, "one promise for each family");
  await first;
  assert.equal(previewOf(pie, { w: 300, h: 200 }, {}, {}).family, "piechart");
  await loadPreviewFamilies();
  assert.equal(previewOf({ id: 2, type: "table", options: {}, fields: {} }, { w: 300, h: 200 }, {}, {}).family, "table");
});
