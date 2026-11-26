// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B370 to B377 (9.11): the loading preview of the pie chart and of the map (the family "rings").
// Each case holds the queries of the panels, reads the preview, releases the queries and reads the loaded panel with the harness of
// lib/preview_measure.mjs. A claimed part must agree with the loaded part by its class: exact within 0.5 px, or bounded on one side.
// B370 to B373: the pie chart. B374 to B376: the map. B377: captures of the loading state in the light and the dark theme.
// The queries are held, so the cases need the interception of Chrome (they are skipped in Firefox).
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { measureDashboard } from "../lib/preview_measure.mjs";
import { MAP_VARIANTS, PIE_VARIANTS, mapDoc, pieDoc } from "../lib/preview_rings_fixtures.mjs";
import { sleep } from "../lib/h.mjs";

const SKIP = process.env.HUNE_BROWSER === "firefox" ? "needs the hold of a request (Chrome)" : undefined;
const HOLD = (req) => /\/panels\/[^/]+\/query/.test(req.url);
// The folder of the captures. HUNE_PREVIEW_SHOTS overrides it.
const SHOTS = process.env.HUNE_PREVIEW_SHOTS || "";

const create = async (t, document) => {
  const r = await t.api.create("transit", document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
};

// The claimed rows of a measure for one panel id: { part#index: row }.
const rowsOf = (result, id) => Object.fromEntries(result.rows.filter((r) => String(r.id) === String(id) && r.cls).map((r) => [`${r.part}#${r.index}`, r]));
const noFailure = (t, result, what) => {
  const failed = result.rows.filter((r) => r.verdict === "fail" || r.verdict === "no-loaded");
  t.eq(failed.map((r) => `${r.id} ${r.part}#${r.index} ${r.verdict} preview ${JSON.stringify(r.preview)} loaded ${JSON.stringify(r.loaded)}`), [], `${what}: no claimed part differs from the loaded panel`);
};
const variantId = (list, name) => String(list.findIndex((v) => v.name === name) + 1);

// A preview probe: the computed facts of the preview of one panel, read in the page while the queries are held.
const hold = async (t, uid, { width = 1440, height = 4000 } = {}) => {
  const p = await t.page({ width, height });
  const h = p.holdWhen(HOLD);
  await p.goto(t.dash(uid));
  await p.waitFor(`(() => { const e = document.querySelector('[data-testid="panel-1"] .panel__skeleton'); return e && getComputedStyle(e).display !== "none" && e.firstElementChild && e.firstElementChild.childElementCount > 0; })()`, { what: "the preview", timeout: 30000 });
  await sleep(300);
  return { p, release: () => h.release() };
};

export const cases = [
  {
    id: "B370",
    skip: SKIP,
    timeout: 240000,
    name: "the pie preview: the plot, the legend and the ring agree with the loaded pie on 14 option variants (9.11)",
    async run(t) {
      const uid = "pv370";
      await create(t, pieDoc(uid));
      const p = await t.page({ width: 1440, height: 4000 });
      const result = await measureDashboard(p, t.dash(uid), { width: 1440, height: 4000 });
      noFailure(t, result, "pie");
      // Each variant claims the three parts, with the class that the document decides.
      for (const v of PIE_VARIANTS) {
        const id = variantId(PIE_VARIANTS, v.name);
        const rows = rowsOf(result, id);
        t.assert(rows["plot#0"] && rows["ring#0"], `${v.name}: the plot and the ring are claimed and measured`);
        t.eq(rows["plot#0"].verdict, "ok", `${v.name}: plot ${JSON.stringify(rows["plot#0"].delta)}`);
        t.eq(rows["ring#0"].verdict, "ok", `${v.name}: ring ${JSON.stringify(rows["ring#0"].delta)}`);
        t.eq(Boolean(rows["legend#0"]), v.options.legend.show, `${v.name}: the legend is claimed when it shows`);
      }
      // Exact where the document fixes the place: no legend, a narrow right legend (W <= 138.46), a low bottom legend (H < 44.9), a literal name.
      for (const name of ["legend hidden", "right narrow", "donut bottom low", "literal name right"]) {
        const rows = rowsOf(result, variantId(PIE_VARIANTS, name));
        t.eq(rows["plot#0"].cls, "exact", `${name}: the plot is exact`);
        t.eq(rows["ring#0"].cls, "exact", `${name}: the ring is exact`);
        t.assert(rows["plot#0"].delta.every((d) => Math.abs(d) <= 0.5), `${name}: the plot agrees within 0.5 px ${JSON.stringify(rows["plot#0"].delta)}`);
        t.assert(rows["ring#0"].delta.every((d) => Math.abs(d) <= 0.5), `${name}: the ring agrees within 0.5 px ${JSON.stringify(rows["ring#0"].delta)}`);
      }
      // Bounded where the data decide: the table legends and the many items. The preview plot is at least the loaded plot.
      for (const name of ["right table value percent", "bottom table", "many items right", "many items bottom"]) {
        const rows = rowsOf(result, variantId(PIE_VARIANTS, name));
        t.eq(rows["plot#0"].cls, "most", `${name}: the plot is bounded`);
        t.assert(rows["plot#0"].delta[2] >= -0.5 && rows["plot#0"].delta[3] >= -0.5, `${name}: the preview plot is at least the loaded plot ${JSON.stringify(rows["plot#0"].delta)}`);
        t.assert(rows["ring#0"].delta[2] >= -0.5, `${name}: the preview ring is at least the loaded ring ${JSON.stringify(rows["ring#0"].delta)}`);
        t.assert(rows["legend#0"].delta[2] <= 0.5 && rows["legend#0"].delta[3] <= 0.5, `${name}: the preview legend is at most the loaded legend ${JSON.stringify(rows["legend#0"].delta)}`);
      }
      // The legend of many items is bigger than the least legend: the plot moves on load (the council question O1).
      const many = rowsOf(result, variantId(PIE_VARIANTS, "many items bottom"));
      t.assert(many["plot#0"].delta[3] > 20, `many items bottom: the loaded legend has several lines, so the plot is lower ${JSON.stringify(many["plot#0"].delta)}`);
    },
  },
  {
    id: "B371",
    skip: SKIP,
    timeout: 120000,
    name: "the pie preview draws no mark: no swatch, no text, no slice, a hole at 55 % for a donut and none for a pie (9.11)",
    async run(t) {
      const uid = "pv371";
      await create(t, pieDoc(uid));
      const { p, release } = await hold(t, uid);
      const facts = await p.eval(`(() => {
        const out = {};
        for (const el of document.querySelectorAll('[data-skel="piechart"]')) {
          const panel = el.closest('[data-testid^="panel-"]');
          const id = panel.getAttribute("data-testid").slice(6);
          const ring = el.querySelector(".skel__ring");
          const legend = el.querySelector(".skel__legend");
          out[id] = {
            text: el.textContent.trim(),
            canvas: el.querySelectorAll("canvas, img").length,
            ringImage: ring ? getComputedStyle(ring).backgroundImage : null,
            ringRadius: ring ? getComputedStyle(ring).borderRadius : null,
            ringFill: ring ? getComputedStyle(ring).backgroundColor : null,
            legendVisibility: legend ? getComputedStyle(legend).visibility : null,
            legendMarks: legend ? [...legend.querySelectorAll("*")].filter((n) => getComputedStyle(n).visibility !== "hidden").length : 0,
            plotChildren: el.querySelector(".skel__plot").childElementCount,
            legendMinWidth: legend ? getComputedStyle(legend).minWidth : null,
            legendMaxWidth: legend ? getComputedStyle(legend).maxWidth : null,
            legendMaxHeight: legend ? getComputedStyle(legend).maxHeight : null,
            gap: getComputedStyle(el.querySelector(".skel__pie")).gap,
            parts: el.querySelectorAll("[data-skel-part]").length,
          };
        }
        return out;
      })()`);
      release();
      const id = (name) => variantId(PIE_VARIANTS, name);
      for (const [key, f] of Object.entries(facts)) {
        t.eq(f.text, "", `panel ${key}: the preview has no text`);
        t.eq(f.canvas, 0, `panel ${key}: the preview has no canvas and no image`);
        t.eq(f.plotChildren, 1, `panel ${key}: the plot holds the ring only`);
        t.eq(f.legendMarks, 0, `panel ${key}: no visible mark in the legend`);
        t.eq(f.ringRadius, "50%", `panel ${key}: the ring is a circle`);
      }
      // The hole: the donut has a gradient with a hard stop at 55 %, the pie has none.
      t.assert(/radial-gradient\(circle closest-side/.test(facts[id("donut right table")].ringImage) && /55%/.test(facts[id("donut right table")].ringImage), `donut: ${facts[id("donut right table")].ringImage}`);
      t.eq(facts[id("right list")].ringImage, "none", "pie: no hole");
      // The legend has the loaded rules.
      t.eq(facts[id("right list")].legendMinWidth, "72px", "right legend: min-width");
      t.eq(facts[id("right list")].legendMaxWidth, "52%", "right legend: max-width");
      t.eq(facts[id("right list")].gap, "12px", "right: the gap");
      t.eq(facts[id("bottom list")].legendMaxHeight, "42%", "bottom legend: max-height");
      t.eq(facts[id("bottom list")].gap, "4px", "bottom: the gap");
      t.eq(facts[id("legend hidden")].legendVisibility, null, "no legend box when the legend is off");
    },
  },
  {
    id: "B372",
    skip: SKIP,
    timeout: 120000,
    name: "the pie preview follows a resize of the window with no JavaScript size: the plot keeps the loaded rule (9.11)",
    async run(t) {
      const uid = "pv372";
      await create(t, pieDoc(uid, PIE_VARIANTS.filter((v) => ["right list", "bottom list"].includes(v.name))));
      const { p, release } = await hold(t, uid, { width: 1440, height: 1200 });
      const read = () => p.eval(`(() => [...document.querySelectorAll('[data-skel="piechart"]')].map((el) => {
        const b = el.getBoundingClientRect();
        const r = (s) => { const x = el.querySelector(s).getBoundingClientRect(); return [x.x - b.x, x.y - b.y, x.width, x.height]; };
        return { box: [b.width, b.height], plot: r(".skel__plot"), legend: r(".skel__legend"), ring: r(".skel__ring") };
      }))()`);
      for (const width of [1440, 1000, 700]) {
        await p.send("Emulation.setDeviceMetricsOverride", { width, height: 1200, deviceScaleFactor: 1, mobile: false });
        await sleep(500);
        const [right, bottom] = await read();
        // The right legend has the least row: 72 px. The plot is the box less 12 px less 72 px.
        t.assert(Math.abs(right.plot[2] - (right.box[0] - 84)) <= 0.5 && Math.abs(right.plot[3] - right.box[1]) <= 0.5, `width ${width}: the right plot ${right.plot} in the box ${right.box}`);
        const d = Math.max(20, Math.min(right.plot[2], right.plot[3]) - 16);
        t.assert(Math.abs(right.ring[2] - d) <= 0.5 && Math.abs(right.ring[3] - d) <= 0.5, `width ${width}: the right ring ${right.ring} against ${d}`);
        // The bottom legend is one line: 18.857 px under a gap of 4 px.
        t.assert(Math.abs(bottom.plot[3] - (bottom.box[1] - 4 - 12 * 1.5714)) <= 0.5 && Math.abs(bottom.plot[2] - bottom.box[0]) <= 0.5, `width ${width}: the bottom plot ${bottom.plot} in the box ${bottom.box}`);
      }
      release();
    },
  },
  {
    id: "B373",
    skip: SKIP,
    timeout: 120000,
    name: "the pie preview: members with no effect change no rectangle, and the legend columns only add empty cells (9.11)",
    async run(t) {
      // Two dashboards that differ only in members that change no place: the same preview rectangles.
      const a = pieDoc("pv373a", PIE_VARIANTS.filter((v) => v.name === "right table value percent"));
      const b = pieDoc("pv373b", PIE_VARIANTS.filter((v) => v.name === "right table value percent"));
      const o = b.sections[0].panels[0].options;
      o.labels = ["name", "value"];
      o.tooltip = { mode: "hidden" };
      o.legend = { ...o.legend, mode: "list", values: ["percent", "value"] };
      b.sections[0].panels[0].fields = { defaults: { unit: "percent", decimals: 2, color: { mode: "fixed", fixed: "red" } }, overrides: [] };
      await create(t, a);
      await create(t, b);
      const rects = async (uid) => {
        const { p, release } = await hold(t, uid, { width: 1440, height: 1200 });
        const out = await p.eval(`(() => { const el = document.querySelector('[data-skel="piechart"]'); const o = el.getBoundingClientRect(); return [...el.querySelectorAll("[data-skel-part]")].map((e) => { const r = e.getBoundingClientRect(); return [e.getAttribute("data-skel-part"), r.x - o.x, r.y - o.y, r.width, r.height]; }); })()`);
        release();
        return out;
      };
      t.eq(await rects("pv373b"), await rects("pv373a"), "unit, decimals, colour, labels, tooltip and the order of the value columns change no rectangle");
    },
  },
  {
    id: "B374",
    skip: SKIP,
    timeout: 240000,
    name: "the map preview: the map box and the two zoom squares agree with the loaded map on 6 option variants (9.11)",
    async run(t) {
      const uid = "pv374";
      await create(t, mapDoc(uid));
      const p = await t.page({ width: 1440, height: 4000 });
      const result = await measureDashboard(p, t.dash(uid), { width: 1440, height: 4000 });
      noFailure(t, result, "map");
      for (const v of MAP_VARIANTS) {
        const rows = rowsOf(result, variantId(MAP_VARIANTS, v.name));
        t.eq(rows["plot#0"].cls, "exact", `${v.name}: the map box is exact`);
        t.assert(rows["plot#0"].delta.every((d) => Math.abs(d) <= 0.5), `${v.name}: the map box agrees ${JSON.stringify(rows["plot#0"].delta)}`);
        if (v.options.show_zoom) {
          for (const k of ["zoom#0", "zoom#1"]) {
            t.eq(rows[k].cls, "exact", `${v.name}: ${k} is exact`);
            t.assert(rows[k].delta.every((d) => Math.abs(d) <= 0.5), `${v.name}: ${k} agrees ${JSON.stringify(rows[k].delta)}`);
          }
          t.assert(Math.abs(rows["zoom#0"].preview[2] - 19.25) <= 0.01 && Math.abs(rows["zoom#1"].preview[1] - 27.25) <= 0.01, `${v.name}: the squares are 19.25 px, at 8 and 27.25`);
        } else {
          t.eq(Boolean(rows["zoom#0"]), false, `${v.name}: no square is claimed`);
          t.eq(result.loaded[variantId(MAP_VARIANTS, v.name)].parts.filter((x) => x.part === "zoom").length, 0, `${v.name}: the loaded map has no zoom box`);
        }
      }
    },
  },
  {
    id: "B375",
    skip: SKIP,
    timeout: 120000,
    name: "the map preview draws no data: no fill, no glyph, no marker, no line, no tile, and only the options of show_zoom change the tree (9.11)",
    async run(t) {
      const uid = "pv375";
      await create(t, mapDoc(uid));
      const { p, release } = await hold(t, uid);
      const facts = await p.eval(`(() => {
        const out = {};
        for (const el of document.querySelectorAll('[data-skel="geomap"]')) {
          const id = el.closest('[data-testid^="panel-"]').getAttribute("data-testid").slice(6);
          const map = el.querySelector(".skel__map");
          out[id] = {
            nodes: el.querySelectorAll("*").length,
            text: el.textContent.trim(),
            canvas: el.querySelectorAll("canvas, img, svg, path").length,
            mapFill: getComputedStyle(map).backgroundColor,
            squareFill: [...el.querySelectorAll(".skel__zoom")].map((s) => getComputedStyle(s).backgroundColor),
            squareText: [...el.querySelectorAll(".skel__zoom")].map((s) => s.childNodes.length),
            tree: el.innerHTML.replace(/ data-skel-class="[a-z]+"/g, ""),
          };
        }
        return out;
      })()`);
      release();
      const id = (name) => variantId(MAP_VARIANTS, name);
      for (const v of MAP_VARIANTS) {
        const f = facts[id(v.name)];
        t.eq(f.text, "", `${v.name}: no text`);
        t.eq(f.canvas, 0, `${v.name}: no canvas, image or path`);
        t.eq(f.mapFill, "rgba(0, 0, 0, 0)", `${v.name}: the map node has no fill`);
        t.eq(f.nodes, v.options.show_zoom ? 4 : 1, `${v.name}: the node count`);
        t.eq(f.squareText, v.options.show_zoom ? [0, 0] : [], `${v.name}: the squares have no child`);
      }
      // Members with no effect: fixed and fit, the narrow map, the transparent map, the wheel: the same tree as the first map.
      t.eq(facts[id("zoom fit")].tree, facts[id("zoom fixed")].tree, "view.mode changes nothing");
      t.eq(facts[id("transparent")].tree, facts[id("zoom fixed")].tree, "transparent changes nothing");
      t.eq(facts[id("narrow map")].tree, facts[id("zoom fixed")].tree, "mouse_wheel_zoom and the size change nothing");
      // The square tone is the neutral skeleton tone, not the fill of the toolbar button.
      t.assert(facts[id("zoom fixed")].squareFill.every((c) => c !== "rgba(0, 0, 0, 0)"), `the squares have a fill ${facts[id("zoom fixed")].squareFill}`);
    },
  },
  {
    id: "B376",
    skip: SKIP,
    timeout: 240000,
    name: "the warning line of an invalid regex override has its place in the preview of a pie and of a map (a draft: the server refuses to store it) (9.11)",
    async run(t) {
      // A stored document cannot hold the invalid pattern. The case stores a valid one and changes the answer of the server, as a draft does.
      const pie = pieDoc("pv376", PIE_VARIANTS.filter((v) => ["right list", "bottom list", "legend hidden"].includes(v.name)));
      const map = mapDoc("pv376m", MAP_VARIANTS.filter((v) => ["zoom fixed", "no zoom"].includes(v.name)));
      const mark = "^valid-pattern$";
      for (const d of [pie, map]) {
        for (const pn of d.sections[0].panels) {
          // One valid override: the case changes its pattern in the answer of the server.
          pn.fields = { defaults: {}, overrides: [{ match: { by: "regex", value: mark }, set: { unit: "none" } }] };
        }
      }
      for (const d of [pie, map]) await create(t, d);
      const bad = "/(/";
      for (const [uid, d] of [["pv376", pie], ["pv376m", map]]) {
        const real = await t.api.dashboard(uid);
        t.eq(real.status, 200, `read ${uid}`);
        const p = await t.page({ width: 1440, height: 4000 });
        p.fulfillWhen((r) => r.method === "GET" && new RegExp(`/api/dashboards/${uid}(\\?|$)`).test(r.url), {
          status: 200,
          headers: { "Content-Type": "application/json", ETag: real.headers.get("etag") || "" },
          body: real.text.replaceAll(mark, bad),
        });
        const result = await measureDashboard(p, t.dash(uid), { width: 1440, height: 4000 });
        noFailure(t, result, uid);
        const ids = d.sections[0].panels.map((x) => String(x.id));
        for (const id of ids) {
          const rows = rowsOf(result, id);
          t.assert(rows["warning#0"], `${uid} panel ${id}: the warning line is claimed`);
          t.eq(rows["warning#0"].verdict, "ok", `${uid} panel ${id}: the warning line agrees ${JSON.stringify(rows["warning#0"].delta)}`);
          t.assert(rows["plot#0"].delta.every((x) => x >= -0.5 || rows["plot#0"].cls !== "exact"), `${uid} panel ${id}: the plot follows the line ${JSON.stringify(rows["plot#0"].delta)}`);
        }
      }
    },
  },
  {
    id: "B377",
    skip: SKIP,
    timeout: 240000,
    name: "captures of the loading state of the pie and of the map in the light and the dark theme (9.11)",
    async run(t) {
      await create(t, pieDoc("pv377"));
      await create(t, mapDoc("pv377m"));
      const dir = SHOTS || join(process.cwd(), "target", "browser", "preview-rings");
      mkdirSync(dir, { recursive: true });
      for (const [uid, label, panels] of [["pv377", "pies", PIE_VARIANTS.length], ["pv377m", "maps", MAP_VARIANTS.length]]) {
        for (const scheme of ["light", "dark"]) {
          const { p, release } = await hold(t, uid, { width: 1440, height: 1500 });
          await p.eval(`document.documentElement.setAttribute("data-theme", ${JSON.stringify(scheme)})`);
          await sleep(500);
          const shown = await p.eval(`[...document.querySelectorAll('.panel__skeleton')].filter((e) => getComputedStyle(e).display !== "none" && e.firstElementChild.childElementCount > 0).length`);
          t.eq(shown, panels, `${label} ${scheme}: every preview shows`);
          await p.screenshot(join(dir, `${label}-${scheme}.png`));
          release();
        }
      }
    },
  },
];
