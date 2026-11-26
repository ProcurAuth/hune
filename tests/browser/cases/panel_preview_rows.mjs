// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B350 to B357 (9.11): the loading preview of the family "rows", the table and the logs panel. Each case holds the queries of a dashboard,
// reads the preview with the harness (lib/preview_measure.mjs), releases the queries, reads the loaded panel, and checks that every part that the preview
// claims agrees with the loaded part in its class. The dashboards come from lib/preview_rows_docs.mjs (one panel for each rule of the council files).
//   B350  the table: the view, the header, the slot of the first row and the extent, on 20 option variants (header, wrap, widths, sort, queries).
//   B351  the table: the select bar of several frames, and the neutral form where the document does not decide.
//   B352  the table: the footer in its two places, and the names of the footer of column 0.
//   B353  the table: the label, the sort mark and the filter of column 0, and the gauge track and the LCD cells.
//   B354  the table on a touch screen: the header of 47 px with a filter button, and the header that the document does not decide.
//   B355  the table and the logs panel with no scroll bar width (overlay bars).
//   B356  the logs panel: the list, the first line, the time bar of 23ch and the track, on 6 option variants.
//   B357  the loading state in the light and the dark theme: neutral tones only. The captures go to HUNE_PREVIEW_CAPTURES when it is set.
// A held request needs the interception of Chrome, so the cases are `skip` with Firefox.
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { Browser } from "../lib/cdp.mjs";
import { WORK } from "../lib/env.mjs";
import { sleep } from "../lib/h.mjs";
import { LOGS_VARIANTS, SEED, TABLE_VARIANTS, logsVariantsDoc, tableVariantsDoc } from "../lib/preview_rows_docs.mjs";
import { measureDashboard } from "../lib/preview_measure.mjs";

const SKIP = process.env.HUNE_BROWSER === "firefox" ? "needs the hold of a request (Chrome)" : undefined;
const HOLD = (req) => /\/panels\/[^/]+\/query/.test(req.url);
const CAPTURES = process.env.HUNE_PREVIEW_CAPTURES || "";

// The parts that the preview of each table variant claims on the seed table (the set of names, without the order). The variants of the corpus are the
// rules of table.md: a part that is missing here is a part that the document does not decide.
const TABLE_CLAIMS = {
  1: ["head", "plot", "row"],
  2: ["plot", "row"],
  3: ["head", "plot"],
  4: ["foot", "foot-name", "head", "plot", "row"],
  5: ["foot", "foot-name", "head", "plot", "row"],
  6: ["filter", "head", "head-label", "mark", "plot", "row"],
  7: ["head", "head-label", "plot", "row"],
  8: ["head", "plot", "row", "track"],
  9: ["cell", "head", "plot", "row"],
  10: ["frames", "head", "plot", "row", "select"],
  11: ["head", "plot", "row"],
  12: ["plot"],
  13: ["foot", "foot-name", "head", "plot", "row"],
  14: [],
  15: ["foot", "foot-name", "head", "plot", "row"],
  16: ["head", "plot", "row"],
  17: ["head", "plot", "track"],
  18: ["head", "plot", "row"],
  19: ["head", "plot", "row"],
  20: ["head", "plot", "row"],
};
const LOGS_CLAIMS = {
  1: ["plot", "row", "track"],
  2: ["plot", "row", "time", "track"],
  3: ["plot", "row", "track"],
  4: ["plot", "track"],
  5: ["plot", "time", "track"],
  6: ["plot", "row", "time", "track"],
};

const create = async (t, doc) => {
  const r = await t.api.create("transit", doc);
  t.assert(r.status === 201, `create ${doc.uid}: ${r.status} ${r.text.slice(0, 400)}`);
};

const claimed = (result, id) => result.rows.filter((r) => String(r.id) === String(id) && r.preview);
const names = (rows) => [...new Set(rows.map((r) => r.part))].sort();
const failures = (result) => result.rows.filter((r) => r.verdict === "fail" || r.verdict === "no-loaded").map((r) => `${r.id} ${r.part}#${r.index} ${r.verdict} preview ${JSON.stringify(r.preview)} loaded ${JSON.stringify(r.loaded)}`);
const near = (a, b, tol = 0.5) => Math.abs(a - b) <= tol;

// Measures a dashboard with the harness and checks the part sets and the tolerance verdicts of the claimed variants.
async function measure(t, doc, opts = {}) {
  await create(t, doc);
  const p = await t.page({ width: opts.width || 1440, height: 4000 });
  return measureDashboard(p, t.dash(doc.uid), { width: opts.width || 1440, height: 4000, coarse: !!opts.coarse });
}

function checkClaims(t, result, claims, label) {
  for (const [id, expected] of Object.entries(claims)) {
    t.eq(names(claimed(result, id)), expected, `${label} ${id}: the parts that the preview claims`);
  }
  t.eq(failures(result), [], `${label}: no claimed part differs from the loaded part`);
}

const rowOf = (result, id, part, index = 0) => result.rows.find((r) => String(r.id) === String(id) && r.part === part && r.index === index);

export const cases = [
  {
    id: "B350",
    skip: SKIP,
    timeout: 240000,
    name: "the table preview draws the view, the header, the slot of the first row and the extent, and each claimed part agrees with the loaded table (9.11)",
    async run(t) {
      const result = await measure(t, tableVariantsDoc("pv350", SEED));
      checkClaims(t, result, TABLE_CLAIMS, "table");
      // The view is the loaded view (exact) in every variant that draws one.
      for (const v of TABLE_VARIANTS) {
        if (v.id === 14) continue;
        const plot = rowOf(result, v.id, "plot");
        t.eq(plot.verdict, "ok", `${v.title}: the view`);
        t.eq(plot.delta, [0, 0, 0, 0], `${v.title}: the view delta`);
      }
      // The header is 34 px on a fine pointer, and it is exact in its top and height. The slot of the first row has 36 px under it.
      const head = rowOf(result, 1, "head");
      t.eq([head.preview[1], head.preview[3]], [0, 34], "the header: top and height");
      t.assert(head.preview[2] <= head.loaded[2] + 0.5, "the header extent is a least bound");
      const slot = rowOf(result, 1, "row");
      t.eq([slot.preview[1], slot.preview[3]], [34, 36], "the first row: top and height");
      t.assert(near(slot.loaded[1], 34) && near(slot.loaded[3], 36), `the loaded first row ${slot.loaded}`);
      // Without a header the first row is at the top. With wrap, the row height is data: no slot.
      t.eq(rowOf(result, 2, "row").preview.slice(1, 2), [0], "show_header false: the slot is at the top");
      t.eq(claimed(result, 3).filter((r) => r.part === "row"), [], "wrap: no slot");
      // A width member limits the extent to the least width of column 0, and an override of 100 px does the same.
      t.eq(rowOf(result, 11, "head").preview[2], 100, "the least width of column 0 is the override");
      t.eq(rowOf(result, 20, "head").preview[2], 40, "narrow columns");
      // The count of rows is data: one slot only.
      t.eq(result.rows.filter((r) => String(r.id) === "1" && r.part === "row" && r.preview).length, 1, "one slot");
    },
  },
  {
    id: "B351",
    skip: SKIP,
    timeout: 240000,
    name: "the table preview reserves the select bar of several frames, and stays neutral where the document does not decide (9.11)",
    async run(t) {
      const result = await measure(t, tableVariantsDoc("pv351", SEED));
      t.eq(failures(result), [], "no claimed part differs");
      // Two visible queries: the bar and its select are exact, and the view is the loaded view.
      for (const part of ["frames", "select"]) {
        const r = rowOf(result, 10, part);
        t.eq(r.verdict, "ok", `the ${part} of the select bar`);
        t.eq(r.delta, [0, 0, 0, 0], `the ${part} delta`);
      }
      const frames = rowOf(result, 10, "frames");
      t.assert(frames.preview[3] >= 28, `the bar is a control high: ${frames.preview}`);
      t.eq(rowOf(result, 10, "plot").loaded[3] + frames.loaded[3], rowOf(result, 1, "plot").loaded[3], "the view and the bar fill the body");
      // A hidden column, a limit of 0 and footers of two lengths: nothing that the document does not decide.
      t.eq(names(claimed(result, 12)), ["plot"], "no shown column is certain");
      t.eq(claimed(result, 14), [], "limit 0 is no data");
      t.eq(result.loaded[14].state, "empty", "the loaded panel shows No data");
      const unknown = claimed(result, 19).map((r) => r.part);
      t.assert(!unknown.includes("foot") && !unknown.includes("foot-name"), "footers of two lengths: the height is data");
      // The notes say which rule gave the form.
      t.assert(result.previews[10].notes.includes("frames"), result.previews[10].notes);
      t.assert(result.previews[12].notes.includes("no shown column is certain"), result.previews[12].notes);
      t.eq(result.previews[14].notes, "no data proven", "the note of the proven No data");
    },
  },
  {
    id: "B352",
    skip: SKIP,
    timeout: 240000,
    name: "the table preview draws the footer under the row (one row) or at the bottom of the view (overflow), with the names of column 0 (9.11)",
    async run(t) {
      const result = await measure(t, tableVariantsDoc("pv352", SEED));
      t.eq(failures(result), [], "no claimed part differs");
      // Case 1: one row, the footer fits: it is under the row. Height 13 + 22 L.
      for (const [id, h] of [[5, 35], [13, 35]]) {
        const foot = rowOf(result, id, "foot");
        t.eq(foot.verdict, "ok", `footer ${id}`);
        t.eq([foot.preview[1], foot.preview[3]], [70, h], `footer ${id}: top 34 + 36, height ${h}`);
        t.assert(near(foot.loaded[1], 70) && near(foot.loaded[3], h), `the loaded footer ${foot.loaded}`);
      }
      // Case 2: the content is higher than the view: the footer is at the bottom, 10 px up when the table is wider than the view (classic bar).
      for (const [id, h] of [[4, 35], [15, 57]]) {
        const foot = rowOf(result, id, "foot");
        t.eq(foot.verdict, "ok", `footer ${id}`);
        t.eq(foot.preview[3], h, `footer ${id}: height ${h}`);
        t.assert(near(foot.preview[1], foot.loaded[1]) && near(foot.preview[3], foot.loaded[3]), `footer ${id}: top and height ${foot.preview} / ${foot.loaded}`);
      }
      // The names of column 0: one bar for each line, 22 px apart.
      const names15 = result.rows.filter((r) => String(r.id) === "15" && r.part === "foot-name");
      t.eq(names15.length, 2, "two names for two reducers");
      t.assert(near(names15[1].preview[1] - names15[0].preview[1], 22), "the lines are 22 px apart");
      t.assert(names15.every((r) => r.verdict === "ok" && r.preview[3] === 8), "the names are bars of 8 px at the measured width");
    },
  },
  {
    id: "B353",
    skip: SKIP,
    timeout: 240000,
    name: "the table preview draws the label, the sort mark and the filter of column 0, and the gauge track and the LCD cells, at the loaded place (9.11)",
    async run(t) {
      const result = await measure(t, tableVariantsDoc("pv353", SEED));
      t.eq(failures(result), [], "no claimed part differs");
      for (const part of ["head-label", "mark", "filter"]) {
        const r = rowOf(result, 6, part);
        t.eq(r.verdict, "ok", `panel 6 ${part}`);
        t.assert(r.delta.every((d) => Math.abs(d) <= 0.5), `${part} delta ${r.delta}`);
      }
      t.eq(rowOf(result, 6, "mark").preview.slice(2), [18, 18], "the sort mark is 18 x 18");
      t.eq(rowOf(result, 6, "filter").preview.slice(2), [16, 22], "the filter button is 16 x 22");
      t.eq(rowOf(result, 7, "head-label").verdict, "ok", "the centred label");
      t.eq(rowOf(result, 7, "mark"), undefined, "no sort key: no mark");
      // The gauge: a track of 22 px, at 6.5 px under the header (6 with wrap).
      for (const [id, y] of [[8, 40.5], [17, 40]]) {
        const track = rowOf(result, id, "track");
        t.eq(track.verdict, "ok", `panel ${id} track`);
        t.eq([track.preview[0], track.preview[1], track.preview[3]], [6, y, 22], `panel ${id} track place`);
      }
      const cells = result.rows.filter((r) => String(r.id) === "9" && r.part === "cell");
      t.eq(cells.length, 12, "an LCD gauge of 160 px: floor((160 - 11) / 12) cells");
      t.assert(cells.every((r) => r.verdict === "ok" && r.preview[2] === 10 && r.preview[3] === 22), "each cell is 10 x 22 px");
      t.assert(near(cells[1].preview[0] - cells[0].preview[0], 12), "the cells are 12 px apart");
      t.eq(claimed(result, 17).filter((r) => r.part === "row"), [], "wrap: the gauge has no row claim");
    },
  },
  {
    id: "B354",
    skip: SKIP,
    timeout: 240000,
    name: "the table preview on a touch screen: the header is 47 px with a filter button on every column, and it is data when only an override sets it (9.11)",
    async run(t) {
      const result = await measure(t, tableVariantsDoc("pv354", SEED), { coarse: true, width: 1000 });
      t.eq(failures(result), [], "no claimed part differs");
      for (const id of [6, 16]) {
        const head = rowOf(result, id, "head");
        t.eq(head.verdict, "ok", `panel ${id} header`);
        t.eq([head.preview[3], head.loaded[3]], [47, 47], `panel ${id}: the header is 47 px`);
        t.eq(rowOf(result, id, "row").preview[1], 47, `panel ${id}: the first row starts at 47`);
      }
      const filter = rowOf(result, 6, "filter");
      t.eq(filter.preview.slice(2), [40, 40], "the filter button is 40 x 40");
      t.eq(filter.verdict, "ok", "the filter button of column 0");
      // An override that sets filterable on one field: the header height is data, so the view only.
      t.eq(names(claimed(result, 18)), ["plot"], "the header height is data");
      t.eq(result.previews[18].notes.includes("header height is data"), true, result.previews[18].notes);
      // The header without a filter button stays 34 px.
      t.eq(rowOf(result, 1, "head").preview[3], 34, "no filter button: 34 px");
      t.eq(rowOf(result, 2, "row").preview[1], 0, "no header: the first row is at the top");
    },
  },
  {
    id: "B355",
    skip: SKIP,
    timeout: 300000,
    name: "the table and logs previews with no scroll bar width: the extents are the whole view and the footer is at the bottom of the view (9.11)",
    async run(t) {
      const profile = join(WORK, "profile-rows-overlay");
      mkdirSync(WORK, { recursive: true });
      const browser = await Browser.launch({ profileDir: profile, args: ["--hide-scrollbars"] });
      try {
        const tables = tableVariantsDoc("pv355", SEED);
        const logs = logsVariantsDoc("pv355l", SEED);
        await create(t, tables);
        await create(t, logs);
        const context = await browser.newContext();
        const page = await context.newPage({ width: 1440, height: 4000 });
        const a = await measureDashboard(page, t.dash("pv355"), { width: 1440, height: 4000 });
        const l = await measureDashboard(await context.newPage({ width: 1440, height: 4000 }), t.dash("pv355l"), { width: 1440, height: 4000 });
        t.eq(failures(a), [], "tables: no claimed part differs");
        t.eq(failures(l), [], "logs: no claimed part differs");
        // With no bar the extent of a table with no width member is the whole view, and the footer is claimed in the case of the low panel.
        const head = rowOf(a, 1, "head");
        t.eq(head.preview[2], rowOf(a, 1, "plot").preview[2], "the extent is the whole view");
        t.assert(rowOf(a, 4, "foot") && rowOf(a, 4, "foot").verdict === "ok", "the footer of the low panel");
        const lowNoWidth = claimed(a, 15).map((r) => r.part);
        t.assert(lowNoWidth.includes("foot"), "the footer at the bottom with an exact bottom");
        // The line of the logs panel is as wide as the list.
        t.eq(rowOf(l, 1, "row").preview[2], rowOf(l, 1, "plot").preview[2], "the line is the width of the list");
      } finally {
        await browser.close();
      }
    },
  },
  {
    id: "B356",
    skip: SKIP,
    timeout: 240000,
    name: "the logs preview is one line slot: the time bar of 23ch only with show_time, then one track from the level cell, on 6 option variants (9.11)",
    async run(t) {
      const result = await measure(t, logsVariantsDoc("pv356", SEED));
      checkClaims(t, result, LOGS_CLAIMS, "logs");
      for (const v of LOGS_VARIANTS) {
        const plot = rowOf(result, v.id, "plot");
        t.eq(plot.delta, [0, 0, 0, 0], `${v.title}: the list is the loaded list`);
        const track = rowOf(result, v.id, "track");
        t.eq([track.preview[1], track.preview[3]], [7, 8], `${v.title}: the bar is 8 px high at 7 px (centre 11 px in the text band)`);
        const time = rowOf(result, v.id, "time");
        t.eq(!!time, v.options.show_time === true, `${v.title}: the time bar comes with show_time true only`);
        if (time) {
          t.eq(time.verdict, "ok", `${v.title}: the time box`);
          t.assert(near(time.preview[2], time.loaded[2], 0.6), `${v.title}: 23ch ${time.preview[2]} / ${time.loaded[2]}`);
        }
        t.eq(result.loaded[v.id].state, "ready", `${v.title}: loaded`);
      }
      // The gap of 12 px between the time bar and the track (without wrap), and 0 with wrap, as the loaded list draws it (the spec says 12).
      const gap = (id) => rowOf(result, id, "track").preview[0] - (rowOf(result, id, "time").preview[0] + rowOf(result, id, "time").preview[2]);
      t.assert(near(gap(2), 12, 0.6), `gap without wrap ${gap(2)}`);
      t.assert(near(gap(5), 0, 0.6), `gap with wrap ${gap(5)}`);
      // The first line: 28 px high, at the top of the list.
      const row = rowOf(result, 1, "row");
      t.eq([row.preview[1], row.preview[3]], [0, 28], "slot 1");
      t.eq(result.rows.filter((r) => String(r.id) === "1" && r.part === "row" && r.preview).length, 1, "one slot: the count of lines is data");
    },
  },
  {
    id: "B357",
    skip: SKIP,
    timeout: 240000,
    name: "the loading state of the table and the logs panel uses neutral tones in the light and the dark theme, and holds no text a person can read (9.11)",
    async run(t) {
      await create(t, tableVariantsDoc("pv357", SEED));
      await create(t, logsVariantsDoc("pv357l", SEED));
      if (CAPTURES) mkdirSync(CAPTURES, { recursive: true });
      for (const [uid, ids] of [["pv357", [1, 6, 8, 10, 15]], ["pv357l", [2, 4]]]) {
        const p = await t.page({ width: 1440, height: 1400 });
        const hold = p.holdWhen(HOLD);
        await p.goto(t.dash(uid));
        await p.waitFor(`(() => { const e = document.querySelector('[data-testid="panel-${ids[0]}"] .panel__skeleton'); return e && getComputedStyle(e).display !== "none" && e.firstElementChild && e.firstElementChild.childElementCount > 0; })()`, { what: "the preview", timeout: 30000 });
        await sleep(500);
        for (const theme of ["light", "dark"]) {
          await p.eval(`document.documentElement.setAttribute("data-theme", ${JSON.stringify(theme)})`);
          await sleep(250);
          // Every element of the preview has a neutral tone: a colour of the document never shows. The text of the select bar and of the warning is transparent.
          const facts = await p.eval(`(() => {
            const out = { bad: [], text: [], shapes: 0 };
            const first = document.querySelector('[data-testid="panel-${ids[0]}"] .panel__skeleton');
            const tone = (name) => { const e = document.createElement("i"); e.style.setProperty("background-color", "var(" + name + ")"); first.append(e); const c = getComputedStyle(e).backgroundColor; e.remove(); return c; };
            const allowed = new Set(["rgba(0, 0, 0, 0)", tone("--skel-fill"), tone("--skel-edge"), tone("--skel-grid")]);
            for (const id of ${JSON.stringify(ids)}) {
              const sk = document.querySelector('[data-testid="panel-' + id + '"] .panel__skeleton');
              if (!sk) continue;
              for (const e of sk.querySelectorAll("*")) {
                const cs = getComputedStyle(e);
                out.shapes++;
                if (!allowed.has(cs.backgroundColor)) out.bad.push(e.className + " " + cs.backgroundColor);
                if (parseFloat(cs.borderTopWidth) > 0 && !allowed.has(cs.borderTopColor)) out.bad.push(e.className + " border " + cs.borderTopColor);
                if (e.childNodes.length && [...e.childNodes].some((n) => n.nodeType === 3 && n.nodeValue.trim()) && cs.color !== "rgba(0, 0, 0, 0)") out.text.push(e.className + " " + cs.color);
              }
            }
            return out;
          })()`);
          t.assert(facts.shapes > 0, `${theme}: the previews have shapes`);
          t.eq(facts.bad, [], `${theme}: the colours`);
          t.eq(facts.text, [], `${theme}: no readable text in a preview`);
          if (CAPTURES) await p.screenshot(join(CAPTURES, `${uid}-${theme}.png`));
        }
        hold.release();
      }
    },
  },
];
