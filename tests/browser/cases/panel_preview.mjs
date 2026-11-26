// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B320 and B321 (9.11): the loading preview of a panel that loads for the first time.
// B320: every queried panel type has a preview of its family. The measurement harness (lib/preview_measure.mjs) measures it while the queries are held,
//       then measures the loaded panel. A family that has no shapes yet shows the neutral form: an empty body with the light and the dots. A family
//       with shapes is in DRAWN, and the harness must find no difference of its claimed parts.
// B321: the light moves with a transform and stops with the reduced motion; a reload of a loaded panel shows no preview.
// The queries of the panels are held, so that the preview stays on the screen. A held request needs the interception of Chrome.
// HUNE_PREVIEW_DUMP=<file> makes B320 write the full table of the harness on the fixture to <file>, and the result to <file>.json.
import { writeFileSync } from "node:fs";
import { formatTable, measureDashboard } from "../lib/preview_measure.mjs";
import { FIXTURE_TYPES, allTypesPreviewDoc } from "../lib/preview_fixtures.mjs";
import { sleep } from "../lib/h.mjs";

const SKIP = process.env.HUNE_BROWSER === "firefox" ? "needs the hold of a request (Chrome)" : undefined;
const HOLD = (req) => /\/panels\/[^/]+\/query/.test(req.url);

// The family of each panel type (web/js/preview/index.js).
const FAMILY = {
  timeseries: "timeseries", xychart: "timeseries", barchart: "barchart", piechart: "piechart", stat: "stat", gauge: "gauge", bargauge: "bargauge", table: "table",
  heatmap: "heatmap", histogram: "histogram", state_timeline: "timeline", status_history: "timeline", logs: "logs", geomap: "geomap",
};
// The families that draw shapes. A family module that draws its first shape adds its name here, and the neutral test no longer applies to it.
const DRAWN = new Set(["piechart", "geomap", "stat", "gauge", "bargauge", "table", "logs", "heatmap", "histogram", "timeline", "timeseries", "barchart"]);

const create = async (t, uid) => {
  const r = await t.api.create("transit", allTypesPreviewDoc(uid, { from: "now-6h", to: "now", timezone: "utc", refresh: null }));
  t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 400)}`);
};

export const cases = [
  {
    id: "B320",
    skip: SKIP,
    timeout: 180000,
    name: "each queried panel type shows the preview of its family, neutral until the family draws, and the harness measures the loaded panel (9.11)",
    async run(t) {
      const uid = "pv320";
      await create(t, uid);
      const p = await t.page({ width: 1440, height: 4000 });
      const result = await measureDashboard(p, t.dash(uid), { width: 1440, height: 4000 });
      // HUNE_PREVIEW_DUMP=<file> writes the table and the full result of the harness on the fixture, for the work on a family.
      if (process.env.HUNE_PREVIEW_DUMP) {
        writeFileSync(process.env.HUNE_PREVIEW_DUMP, `${formatTable(result.rows, { all: true })}\n`);
        writeFileSync(`${process.env.HUNE_PREVIEW_DUMP}.json`, `${JSON.stringify(result, null, 2)}\n`);
      }
      const ids = FIXTURE_TYPES.map((_, i) => String(i + 1));
      t.eq(Object.keys(result.previews).sort((a, b) => a - b), ids, "one panel for each type");
      FIXTURE_TYPES.forEach((type, i) => {
        const v = result.previews[i + 1];
        t.assert(v, `panel ${i + 1} (${type}) was read`);
        t.eq(v.type, type === "status_history" ? "status_history" : type, `panel ${i + 1} type`);
        t.eq(v.family, FAMILY[type], `${type}: the family`);
        t.eq(v.shown, true, `${type}: the preview shows while the panel loads for the first time`);
        t.eq(v.wait, "first", `${type}: the wait is a first load`);
        t.eq(v.aria, "true", `${type}: the preview is hidden from assistive technology`);
        t.eq(v.sweep, "panel-skel", `${type}: the light sweeps`);
        t.assert(v.box[0] > 0 && v.box[1] > 0, `${type}: the box has a size ${v.box}`);
        if (!DRAWN.has(v.family)) {
          t.eq(v.children, 0, `${type}: the neutral form has no shape`);
          t.eq(v.notes, "neutral", `${type}: the neutral form says so`);
          t.eq(v.parts.length, 0, `${type}: the neutral form claims no part`);
        }
        // The box of the preview is the content box of the body: the same as the box of the loaded panel.
        const loaded = result.loaded[i + 1];
        const box = loaded.parts.find((x) => x.part === "box");
        t.assert(box, `${type}: the loaded panel has a box`);
        t.assert(Math.abs(box.rect[2] - v.box[0]) <= 0.5 && Math.abs(box.rect[3] - v.box[1]) <= 0.5, `${type}: the box of the preview ${v.box} and of the loaded panel ${box.rect}`);
        t.assert(["ready", "empty"].includes(loaded.state), `${type}: the panel loaded (${loaded.state})`);
      });
      // A claimed part that differs from the loaded panel is a failure. A neutral family claims nothing, so nothing can fail.
      const failed = result.rows.filter((r) => r.verdict === "fail" || r.verdict === "no-loaded");
      t.eq(failed.map((r) => `${r.id} ${r.part}#${r.index} ${r.verdict}`), [], "no claimed part differs from the loaded panel");
      // The loaded panels give targets: the charts give a plot, an axis and a legend.
      const targets = (id) => new Set(result.rows.filter((r) => String(r.id) === String(id)).map((r) => r.part));
      for (const part of ["plot", "axis-y", "axis-x", "legend"]) t.assert(targets(1).has(part), `the loaded time series gives the part ${part}`);
    },
  },
  {
    id: "B321",
    skip: SKIP,
    name: "the light of the preview moves with a transform and stops with the reduced motion, and a reload shows no preview (9.11)",
    async run(t) {
      const uid = "pv321";
      await create(t, uid);
      const p = await t.page({ width: 1440, height: 4000 });
      const hold = p.holdWhen(HOLD);
      await p.goto(t.dash(uid));
      await p.waitFor(`(() => { const e = document.querySelector('[data-testid="panel-1"] .panel__skeleton'); return e && getComputedStyle(e).display !== "none"; })()`, { what: "the preview", timeout: 20000 });
      await sleep(400);
      const light = () => p.eval(`(() => { const a = getComputedStyle(document.querySelector('[data-testid="panel-1"] .panel__skeleton'), "::after"); return { name: a.animationName, display: a.display }; })()`);
      t.eq(await light(), { name: "panel-skel", display: "block" }, "the light sweeps");
      await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
      await sleep(200);
      t.eq(await light(), { name: "none", display: "none" }, "the reduced motion stops the light");
      await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
      // The preview uses neutral tones in both themes: it takes no colour of the document.
      for (const theme of ["light", "dark"]) {
        const fill = await p.eval(`(async () => { document.documentElement.setAttribute("data-theme", ${JSON.stringify(theme)}); await new Promise((r) => setTimeout(r, 100)); return getComputedStyle(document.querySelector('[data-testid="panel-1"] .panel__skeleton')).getPropertyValue("--skel-fill").trim(); })()`);
        t.assert(fill.startsWith("color-mix(") || fill.startsWith("rgb"), `the fill of the preview is a neutral tone in the ${theme} theme: ${fill}`);
      }
      // After the load, a reload keeps the content and shows no preview (the dots only).
      hold.release();
      await p.waitFor(`[...document.querySelectorAll('[data-testid^="panel-"][data-type]')].every((e) => !["idle", "queued", "loading"].includes(e.dataset.state))`, { what: "the panels", timeout: 60000 });
      const again = p.holdWhen(HOLD);
      await p.click('[data-testid="refresh-now"]');
      await sleep(900);
      const reload = await p.eval(`(() => { const e = document.querySelector('[data-testid="panel-1"]'); return { wait: e.dataset.wait || null, shows: getComputedStyle(e.querySelector(".panel__skeleton")).display }; })()`);
      t.eq(reload, { wait: "reload", shows: "none" }, "a reload shows no preview");
      again.release();
    },
  },
];
