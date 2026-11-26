// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B340 to B347 (9.11): the loading previews of the values families, `stat`, `gauge` and `bargauge`.
// Each case holds the queries of the panels, so that the previews stay, reads them with the harness (lib/preview_measure.mjs), releases the queries and
// reads the loaded panels. The harness compares each part that a preview claims with the loaded part by the tolerance class (exact: 0.5 px; least: the
// preview takes at most the room of the loaded part). A part that the document does not decide is not claimed, so the cases also count the parts.
//   B340  stat: the box of the body for ten variants of the options, at three widths
//   B341  bar gauge: the box for eight variants, at three widths
//   B342  gauge whose count is data: the box only
//   B343  gauge with one new number field: the cell, the title, the canvas and the empty arc, 17 variants, at three widths
//   B344  gauge: the neutral form where the document does not decide (an override, a scale from the data, a text that can move the arc)
//   B345  the warning line of a draft with a pattern that does not compile, in the three families
//   B346  neutral tones in the light and the dark theme, and the captures of the loading state
//   B347  the preview builds again when the box changes
// HUNE_PREVIEW_SHOTS=<dir> writes the captures of B346 to <dir>. They need the hold of a request, so the cases are `skip` with Firefox.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { HOLD_QUERIES, compare, formatTable, installRecorder, measureDashboard, readLoaded, readPreview, summarize } from "../lib/preview_measure.mjs";
import { BARGAUGE_VARIANTS, GAUGE_N_VARIANTS, GAUGE_VARIANTS, STAT_VARIANTS, barGaugeDoc, gaugeDoc, gaugeNDoc, statDoc } from "../lib/values_fixtures.mjs";
import { sleep } from "../lib/h.mjs";

const SKIP = process.env.HUNE_BROWSER === "firefox" ? "needs the hold of a request (Chrome)" : undefined;
const TIME = { from: "now-6h", to: "now", timezone: "utc", refresh: null };
const WIDTHS = [1440, 997, 700];

const create = async (t, d) => {
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create ${d.uid}: ${r.status} ${r.text.slice(0, 400)}`);
};

// The rows of a measure that fail, and the claimed parts of a panel.
const failures = (rows) => rows.filter((r) => r.verdict === "fail" || r.verdict === "no-loaded").map((r) => `${r.id} ${r.part}#${r.index} ${r.verdict} ${JSON.stringify(r.delta)}`);
const claimed = (result, id) => result.previews[id].parts.map((p) => `${p.part}:${p.cls}`);

// Measure a dashboard at the widths of WIDTHS. Each width is a new page. Gives { width: result }.
async function measureAt(t, uid, widths = WIDTHS) {
  const out = {};
  for (const width of widths) {
    const p = await t.page({ width, height: 4000 });
    out[width] = await measureDashboard(p, t.dash(uid), { width, height: 4000 });
  }
  return out;
}

export const cases = [
  {
    id: "B340",
    skip: SKIP,
    timeout: 240000,
    name: "stat: the preview is the box of the body, as the loaded grid, for ten variants of the options and three widths (9.11)",
    async run(t) {
      const uid = "pv340";
      await create(t, statDoc(uid, TIME));
      const results = await measureAt(t, uid);
      for (const [width, result] of Object.entries(results)) {
        t.eq(failures(result.rows), [], `${width}: no claimed part differs from the loaded panel`);
        STAT_VARIANTS.forEach(([title], i) => {
          const id = i + 1;
          const v = result.previews[id];
          t.eq(v.family, "stat", `${width} ${title}: the family`);
          t.eq(v.shown, true, `${width} ${title}: the preview shows`);
          // The strict form: one box, no tile, no gap, no text slot, no tint (the number of tiles is data).
          t.eq(v.children, 1, `${width} ${title}: the preview has one element`);
          t.eq(claimed(result, id), ["holder:exact"], `${width} ${title}: the claimed parts`);
          t.eq(v.notes, "box", `${width} ${title}: the note`);
          const row = result.rows.find((r) => String(r.id) === String(id) && r.part === "holder");
          t.eq(row.verdict, "ok", `${width} ${title}: the box is the loaded grid (${JSON.stringify(row.preview)} and ${JSON.stringify(row.loaded)})`);
          t.assert(row.loaded[2] > 0 && row.loaded[3] > 0, `${width} ${title}: the loaded grid has a size`);
          // The grid reaches the edge of the body: the preview has the box of the body, not the box inside the padding.
          const box = result.loaded[id].parts.find((x) => x.part === "box");
          t.assert(Math.abs(box.rect[2] - row.loaded[2]) <= 0.5 && Math.abs(box.rect[3] - row.loaded[3]) <= 0.5, `${width} ${title}: the box of the body ${box.rect}`);
        });
      }
    },
  },
  {
    id: "B341",
    skip: SKIP,
    timeout: 240000,
    name: "bar gauge: the preview is the box of the loaded root, with nothing inside, for eight variants and three widths (9.11)",
    async run(t) {
      const uid = "pv341";
      await create(t, barGaugeDoc(uid, TIME));
      const results = await measureAt(t, uid);
      for (const [width, result] of Object.entries(results)) {
        t.eq(failures(result.rows), [], `${width}: no claimed part differs from the loaded panel`);
        BARGAUGE_VARIANTS.forEach(([title], i) => {
          const id = i + 1;
          const v = result.previews[id];
          t.eq(v.family, "bargauge", `${width} ${title}: the family`);
          t.eq(v.children, 1, `${width} ${title}: no item, no track, no name, no value, no cell`);
          t.eq(claimed(result, id), ["holder:exact"], `${width} ${title}: the claimed parts`);
          const row = result.rows.find((r) => String(r.id) === String(id) && r.part === "holder");
          t.eq(row.verdict, "ok", `${width} ${title}: the box is the loaded root (${JSON.stringify(row.preview)} and ${JSON.stringify(row.loaded)})`);
          // The loaded panel has items, and the preview claims none of their parts: they are data.
          t.assert(result.rows.some((r) => String(r.id) === String(id) && r.part === "track" && r.verdict === "unclaimed"), `${width} ${title}: the loaded panel has tracks that the preview leaves to the data`);
        });
      }
    },
  },
  {
    id: "B342",
    skip: SKIP,
    timeout: 180000,
    name: "gauge: when the document does not fix the number of gauges, the preview is the box of the holder only (9.11)",
    async run(t) {
      const uid = "pv342";
      await create(t, gaugeNDoc(uid, TIME));
      const results = await measureAt(t, uid, [1440, 997]);
      for (const [width, result] of Object.entries(results)) {
        t.eq(failures(result.rows), [], `${width}: no claimed part differs from the loaded panel`);
        GAUGE_N_VARIANTS.forEach(([title], i) => {
          const id = i + 1;
          const v = result.previews[id];
          t.eq(v.children, 1, `${width} ${title}: one element`);
          t.eq(claimed(result, id), ["holder:exact"], `${width} ${title}: the claimed parts`);
          t.eq(v.notes, "box", `${width} ${title}: the note`);
          t.assert(result.rows.some((r) => String(r.id) === String(id) && r.part === "plot" && r.verdict === "unclaimed"), `${width} ${title}: the loaded panel has a canvas that the preview does not claim`);
        });
      }
    },
  },
  {
    id: "B343",
    skip: SKIP,
    timeout: 300000,
    name: "gauge with one new number field: the cell, the title box and bar, the canvas and the empty arc agree with the loaded gauge for 17 variants and three widths (9.11)",
    async run(t) {
      const uid = "pv343";
      await create(t, gaugeDoc(uid, TIME));
      const results = await measureAt(t, uid);
      for (const [width, result] of Object.entries(results)) {
        t.eq(failures(result.rows), [], `${width}: no claimed part differs from the loaded panel`);
        const exactRows = result.rows.filter((r) => r.cls === "exact" && r.verdict === "ok");
        t.assert(exactRows.length >= 17 * 3, `${width}: ${exactRows.length} exact parts agree`);
        GAUGE_VARIANTS.forEach(([title, , over], i) => {
          const id = i + 1;
          const parts = claimed(result, id);
          t.assert(parts.includes("holder:exact") && parts.includes("tile:exact") && parts.includes("plot:exact"), `${width} ${title}: the box, the cell and the canvas (${parts})`);
          t.assert(parts.some((p) => p.startsWith("arc:")), `${width} ${title}: the empty arc (${parts})`);
          const arc = result.rows.find((r) => String(r.id) === String(id) && r.part === "arc");
          t.eq(arc.verdict, "ok", `${width} ${title}: the arc ${JSON.stringify(arc.preview)} against ${JSON.stringify(arc.loaded)}`);
          // The loaded arc is never smaller than the preview arc (a lower bound), and equal when the class is exact.
          if (arc.cls === "least") t.assert(arc.preview[2] <= arc.loaded[2] + 0.5 && arc.preview[3] <= arc.loaded[3] + 0.5, `${width} ${title}: the arc is a lower bound`);
          // value_size and a fixed band: the text cannot move the arc.
          if (over.options && over.options.value_size && over.options.show_threshold_markers === false) t.eq(arc.cls, "exact", `${width} ${title}: the arc is exact`);
        });
        // Three variants have a title: the box and the bar of the text.
        for (const id of [2, 3, 8, 15]) t.assert(claimed(result, id).includes("title:exact") && claimed(result, id).includes("label:exact"), `${width} ${GAUGE_VARIANTS[id - 1][0]}: the title box and bar`);
        for (const id of [1, 4, 5, 6, 7]) t.assert(!claimed(result, id).includes("title:exact"), `${width} ${GAUGE_VARIANTS[id - 1][0]}: no title`);
      }
      // The wide cell of a manual minimum height is taller than the holder: it scrolls in the loaded panel.
      const first = results[1440];
      const tall = first.rows.find((r) => r.id == 6 && r.part === "tile");
      t.assert(tall.preview[3] > first.rows.find((r) => r.id == 6 && r.part === "holder").preview[3], "the cell of a manual minimum height is taller than the holder");
    },
  },
  {
    id: "B344",
    skip: SKIP,
    timeout: 240000,
    name: "gauge: the preview keeps the neutral form where the document does not decide: an override, a scale from the data, a text that can move the arc (9.11)",
    async run(t) {
      const uid = "pv344";
      await create(t, gaugeDoc(uid, TIME));
      const p = await t.page({ width: 1440, height: 4000 });
      const result = await measureDashboard(p, t.dash(uid), { width: 1440, height: 4000 });
      t.eq(failures(result.rows), [], "no claimed part differs");
      const by = (title) => GAUGE_VARIANTS.findIndex((v) => v[0].startsWith(title)) + 1;
      const parts = (title) => claimed(result, by(title));
      // A scale from the data (no min and no max): no band ring, but the room of the band is reserved, so the arc is a lower bound.
      t.assert(!parts("scale from the data").some((x) => x.startsWith("scale:")), "a scale from the data draws no band ring");
      t.assert(parts("scale from the data").includes("arc:least"), "and the arc is a lower bound");
      // An override that can match the field and sets the maximum: the scale is unknown, so no band ring.
      t.assert(!parts("an override that sets the maximum").some((x) => x.startsWith("scale:")), "an override of the maximum: no band ring");
      // An override that sets the colour: the room of the defaults is reserved, with a lower bound.
      t.assert(parts("an override that sets the colour").includes("arc:least"), "an override of the colour: a lower bound");
      // A fixed scale and thresholds that the document decides: the band ring.
      t.assert(parts("auto, scale 0 to 400").some((x) => x.startsWith("scale:")), "a fixed scale draws the band ring");
      t.assert(parts("percentage thresholds").some((x) => x.startsWith("scale:")), "the percentage mode fixes the scale");
      t.assert(parts("unit percent").some((x) => x.startsWith("scale:")), "a percent unit fixes the scale");
      // The title that the document fixes shows as a bar of the measured width, and nothing more of the data shows.
      for (const id of Object.keys(result.previews)) {
        const v = result.previews[id];
        t.assert(v.children <= 5, `panel ${id}: the preview has few elements (${v.children})`);
        t.eq(v.parts.filter((x) => x.cls === "data").length, 0, `panel ${id}: no part of the class data`);
      }
      // The value text, the filled arc and the threshold labels are never drawn: the only bar in a cell is the title.
      for (const id of Object.keys(result.previews)) {
        const bars = await p.eval(`document.querySelectorAll('[data-testid="panel-${id}"] .panel__skeleton .skel__b').length`);
        t.assert(bars <= 1, `panel ${id}: at most one bar (${bars})`);
      }
    },
  },
  {
    id: "B345",
    skip: SKIP,
    timeout: 180000,
    name: "the warning line of a draft with an override pattern that does not compile: the paragraph, its bars and the box under it agree with the loaded panel for a stat, a gauge and a bar gauge (9.11)",
    async run(t) {
      const uid = "pv345";
      const stat = statDoc(uid, TIME).sections[0].panels[0];
      const gauge = gaugeDoc(uid, TIME).sections[0].panels;
      const bar = barGaugeDoc(uid, TIME).sections[0].panels[0];
      const place = (panel, id, grid) => ({ ...panel, id, grid });
      const panels = [place(stat, 1, { x: 0, y: 0, w: 8, h: 6 }), place(gauge[1], 2, { x: 8, y: 0, w: 8, h: 8 }), place(bar, 3, { x: 16, y: 0, w: 8, h: 6 }), place(gauge[0], 4, { x: 0, y: 8, w: 4, h: 6 })];
      const base = { ...statDoc(uid, TIME), sections: [{ title: null, collapsed: false, panels }] };
      await create(t, base);
      const bad = (panel, patterns) => ({ ...panel, fields: { ...panel.fields, overrides: patterns.map((value) => ({ match: { by: "regex", value }, set: { unit: "percent" } })) } });
      const draft = { ...base, sections: [{ title: null, collapsed: false, panels: [bad(panels[0], ["/(/"]), bad(panels[1], ["/(/", "/[/"]), bad(panels[2], ["/(/", "/[a/", "/(?x/", "/a)/"]), panels[3]] }] };
      const dash = await t.api.dashboard(uid);
      const session = await t.api.session();
      const p = await t.page({ width: 1440, height: 4000 });
      await p.send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 4000, deviceScaleFactor: 1, mobile: false });
      await installRecorder(p);
      await p.goto(t.dash(uid));
      await p.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-type]').length > 0`, { what: "the panels" });
      await sleep(800);
      // The page keeps the unsaved changes of the dashboard in the draft record of its storage (10.8). An editor that opens it shows the draft.
      const record = { base_etag: dash.etag, base_version: dash.json.version || 0, subject: session.json.subject, folder: "transit", updated_at_ms: Date.now(), document: draft, json_text: null, save_id: null, message: "" };
      await p.eval(`localStorage.setItem("hune.draft.v1.${uid}", ${JSON.stringify(JSON.stringify(record))})`);
      const hold = p.holdWhen(HOLD_QUERIES);
      await p.goto(t.dash(uid, "edit=1"));
      await p.waitFor(`document.querySelectorAll('.edit-grid [data-testid^="panel-"][data-state]').length > 0`, { what: "the edit grid", timeout: 30000 });
      await sleep(1200);
      const ids = ["1", "2", "3", "4"];
      const previews = {};
      for (const id of ids) previews[id] = await readPreview(p, id);
      hold.release();
      await p.waitFor(`${JSON.stringify(ids)}.every((id) => { const e = document.querySelector('[data-testid="panel-' + id + '"]'); return !e || !["idle", "queued", "loading"].includes(e.dataset.state); })`, { what: "the loaded panels", timeout: 60000 });
      await sleep(900);
      const rows = [];
      for (const id of ids) rows.push(...compare(previews[id], await readLoaded(p, id, previews[id].type)));
      t.eq(failures(rows), [], `no claimed part differs\n${formatTable(rows.filter((r) => r.verdict !== "unclaimed"), { all: true })}`);
      for (const id of ["1", "2", "3"]) {
        t.assert(previews[id].notes.includes("warning") || previews[id].parts.some((x) => x.part === "warning"), `panel ${id}: the preview holds the warning line`);
        const warn = rows.find((r) => String(r.id) === id && r.part === "warning");
        t.eq(warn.verdict, "ok", `panel ${id}: the paragraph has the loaded rectangle ${JSON.stringify(warn.preview)} / ${JSON.stringify(warn.loaded)}`);
        t.assert(warn.loaded[3] > 20, `panel ${id}: the paragraph has a height ${warn.loaded[3]}`);
        const holder = rows.find((r) => String(r.id) === id && r.part === "holder");
        t.eq(holder.verdict, "ok", `panel ${id}: the box starts under the paragraph`);
        const lines = rows.filter((r) => String(r.id) === id && r.part === "label" && r.cls === "exact");
        t.assert(lines.length >= 1 && lines.every((r) => r.verdict === "ok"), `panel ${id}: each line of the text is a bar of the measured width (${lines.length})`);
      }
      // A panel with no bad pattern has no warning and no paragraph.
      t.assert(!previews["4"].parts.some((x) => x.part === "warning"), "no warning for a panel with no bad pattern");
      // The bar gauge has four patterns, so its paragraph has several lines.
      t.assert(rows.filter((r) => r.id == 3 && r.part === "label").length >= 2, "the paragraph of four sentences has several lines");
    },
  },
  {
    id: "B346",
    skip: SKIP,
    timeout: 180000,
    name: "the previews of the values families use the neutral tones in the light and the dark theme, and never a colour of the document (9.11)",
    async run(t) {
      const uid = "pv346";
      const g = gaugeDoc(uid, TIME).sections[0].panels;
      const colorful = { ...g[11], id: 4, grid: { x: 0, y: 6, w: 8, h: 8 } };
      const base = statDoc(uid, TIME);
      base.sections = [
        {
          title: null,
          collapsed: false,
          panels: [
            { ...base.sections[0].panels[4], id: 1, grid: { x: 0, y: 0, w: 8, h: 6 } },
            { ...g[1], id: 2, grid: { x: 8, y: 0, w: 8, h: 8 } },
            { ...barGaugeDoc(uid, TIME).sections[0].panels[3], id: 3, grid: { x: 16, y: 0, w: 8, h: 8 } },
            { ...colorful, fields: { defaults: { min: 0, max: 400, color: { mode: "fixed", fixed: "purple" }, thresholds: { mode: "absolute", steps: [{ value: null, color: "red" }, { value: 50, color: "blue" }] } }, overrides: [] } },
          ],
        },
      ];
      await create(t, base);
      const p = await t.page({ width: 1440, height: 1100 });
      const hold = p.holdWhen(HOLD_QUERIES);
      await p.goto(t.dash(uid));
      await p.waitFor(`[1, 2, 3, 4].every((id) => { const e = document.querySelector('[data-testid="panel-' + id + '"] .panel__skeleton'); return e && getComputedStyle(e).display !== "none" && e.firstElementChild && e.firstElementChild.childElementCount > 0; })`, { what: "the previews", timeout: 30000 });
      await sleep(400);
      const shots = process.env.HUNE_PREVIEW_SHOTS;
      if (shots) mkdirSync(shots, { recursive: true });
      for (const theme of ["light", "dark"]) {
        await p.eval(`document.documentElement.setAttribute("data-theme", ${JSON.stringify(theme)})`);
        await sleep(300);
        const read = await p.eval(`(() => {
          const out = {};
          for (const id of [1, 2, 3, 4]) {
            const sk = document.querySelector('[data-testid="panel-' + id + '"] .panel__skeleton');
            const probe = document.createElement("i");
            probe.style.setProperty("background", "var(--skel-fill)");
            sk.append(probe);
            const fill = getComputedStyle(probe).backgroundColor;
            probe.remove();
            const ring = sk.querySelector(".skel__ring");
            const colors = [...sk.querySelectorAll("*")].map((e) => { const c = getComputedStyle(e); return [c.backgroundColor, c.fill, c.stroke, c.color, c.backgroundImage]; });
            out[id] = { fill, ring: ring ? getComputedStyle(ring).fill : null, colors: JSON.stringify(colors) };
          }
          return out;
        })()`);
        for (const id of [1, 2, 3, 4]) {
          if (read[id].ring) t.eq(read[id].ring, read[id].fill, `${theme} panel ${id}: the arc has the neutral fill`);
          t.assert(!/rgb\(\s*(128, 0, 128|255, 0, 0|0, 0, 255)/.test(read[id].colors), `${theme} panel ${id}: no colour of the document (${read[id].colors.slice(0, 200)})`);
        }
        if (shots) {
          for (const id of [1, 2, 3, 4]) {
            const clip = await p.eval(`(() => { const r = document.querySelector('[data-testid="panel-${id}"]').getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height, scale: 1 }; })()`);
            const shot = await p.send("Page.captureScreenshot", { format: "png", clip });
            writeFileSync(join(shots, `values-${theme}-panel-${id}.png`), Buffer.from(shot.data, "base64"));
          }
        }
      }
            const notes = await p.eval(`[1, 2, 3, 4].map((id) => document.querySelector('[data-testid="panel-' + id + '"] .panel__skeleton').firstElementChild.getAttribute("data-skel-notes"))`);
      t.eq(notes, ["box", "one gauge, bounded arc", "box", "one gauge, exact arc"], "the notes: the box of a stat and of a bar gauge, a bounded arc with a band that the scale does not prove, an exact arc with a fixed colour");
      hold.release();
    },
  },
  {
    id: "B347",
    skip: SKIP,
    timeout: 240000,
    name: "the preview of a gauge builds again when the box changes: after a resize it agrees with the loaded gauge of the new size (9.11)",
    async run(t) {
      const uid = "pv347";
      await create(t, gaugeDoc(uid, TIME));
      const p = await t.page({ width: 1440, height: 4000 });
      await installRecorder(p);
      const hold = p.holdWhen(HOLD_QUERIES);
      await p.goto(t.dash(uid));
      await p.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-type]').length > 0`, { what: "the panels" });
      await sleep(700);
      const ids = GAUGE_VARIANTS.map((_, i) => String(i + 1));
      const before = await readPreview(p, "1");
      await p.send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 4000, deviceScaleFactor: 1, mobile: false });
      await sleep(900);
      const previews = {};
      for (const id of ids) previews[id] = await readPreview(p, id);
      const after = previews["1"];
      t.assert(after.box[0] !== before.box[0], `the box changed ${before.box} to ${after.box}`);
      hold.release();
      await p.waitFor(`${JSON.stringify(ids)}.every((id) => { const e = document.querySelector('[data-testid="panel-' + id + '"]'); return !e || !["idle", "queued", "loading"].includes(e.dataset.state); })`, { what: "the loaded panels", timeout: 60000 });
      await sleep(1000);
      const rows = [];
      for (const id of ids) rows.push(...compare(previews[id], await readLoaded(p, id, previews[id].type)));
      t.eq(failures(rows), [], `no claimed part differs after the resize\n${formatTable(rows.filter((r) => r.verdict === "fail"), { all: true })}`);
      t.assert(summarize(rows).ok >= 17 * 4, `${summarize(rows).ok} parts agree`);
    },
  },
];
