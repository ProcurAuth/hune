// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B330 to B339 (9.11): the loading preview of the charts (the time series, the xy chart and the bar chart).
// Each case builds a dashboard with option variants, holds the queries so that each panel keeps its preview, measures the preview, releases the
// queries, and measures the loaded panel (lib/preview_measure.mjs). The tolerance classes decide: an exact part is the same within 0.5 px, a
// contain part holds the loaded part on the edges that the rule claims, a least part is not larger than the loaded part.
// HUNE_PREVIEW_DUMP=<dir> writes the table of each case to <dir>/<id>.txt, and the captures of the loading state in the light and the dark theme.
import { mkdirSync, writeFileSync } from "node:fs";
import { formatTable, measureDashboard } from "../lib/preview_measure.mjs";
import { BAR_VARIANTS, TS_VARIANTS, barVariantsDoc, timeSeriesVariantsDoc } from "../lib/preview_charts_fixtures.mjs";
import { sleep } from "../lib/h.mjs";

const SKIP = process.env.HUNE_BROWSER === "firefox" ? "needs the hold of a request (Chrome)" : undefined;
const DUMP = process.env.HUNE_PREVIEW_DUMP || "";

const create = async (t, doc) => {
  const r = await t.api.create("transit", doc);
  t.assert(r.status === 201, `create ${doc.uid}: ${r.status} ${r.text.slice(0, 400)}`);
};

// Measures a dashboard and writes the table when HUNE_PREVIEW_DUMP is set.
async function measure(t, doc, name, height = 6000) {
  await create(t, doc);
  const p = await t.page({ width: 1440, height });
  const result = await measureDashboard(p, t.dash(doc.uid), { width: 1440, height });
  if (DUMP) {
    mkdirSync(DUMP, { recursive: true });
    writeFileSync(`${DUMP}/${name}.txt`, `${formatTable(result.rows, { all: true })}\n`);
    writeFileSync(`${DUMP}/${name}.json`, `${JSON.stringify(result, null, 2)}\n`);
  }
  return result;
}

export const cases = [
  {
    id: "B330",
    skip: SKIP,
    timeout: 300000,
    name: "draft",
    async run(t) {
      const result = await measure(t, timeSeriesVariantsDoc("pv330"), "B330");
      const failed = result.rows.filter((r) => r.verdict === "fail" || r.verdict === "no-loaded");
      t.eq(failed.map((r) => `${r.id} ${r.part}#${r.index} ${r.verdict} ${JSON.stringify(r.preview)} ${JSON.stringify(r.loaded)}`), [], "no claimed part differs");
    },
  },
  {
    id: "B331",
    skip: SKIP,
    timeout: 300000,
    name: "draft",
    async run(t) {
      const result = await measure(t, barVariantsDoc("pv331"), "B331");
      const failed = result.rows.filter((r) => r.verdict === "fail" || r.verdict === "no-loaded");
      t.eq(failed.map((r) => `${r.id} ${r.part}#${r.index} ${r.verdict} ${JSON.stringify(r.preview)} ${JSON.stringify(r.loaded)}`), [], "no claimed part differs");
    },
  },
];
