// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B385 and B386 (9.19, 9.19g): the loaded panels of the group "values" (stat, bar gauge, gauge) that differed from the
// specification. The council of the loading previews found the differences. The loaded panel now follows the text.
// B385: the bar gauge in the mode `lcd` draws the track behind the cells when `show_unfilled` is `true`.
// B386: a warning line above a stat does not lie under the grid of the tiles.
import { doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const UID_LCD = "b385-lcd-unfilled";
const UID_WARN = "b386-stat-warning";
const g = (x, y, w, h) => ({ x, y, w, h });
const SQL_TWO = "SELECT 30 AS a, 60 AS b";
const FIELDS = { defaults: { min: 0, max: 100 }, overrides: [] };
const gauge = (id, title, grid, options) => panel(id, "bargauge", title, SQL_TWO, { grid, options, fields: FIELDS });

const DOC_LCD = doc(UID_LCD, "Bar gauge lcd unfilled", [
  section(null, [
    gauge(1, "lcd unfilled", g(0, 0, 8, 6), { display_mode: "lcd", show_unfilled: true }),
    gauge(2, "lcd not unfilled", g(8, 0, 8, 6), { display_mode: "lcd", show_unfilled: false }),
    gauge(3, "basic unfilled", g(16, 0, 8, 6), { display_mode: "basic", show_unfilled: true }),
    gauge(4, "lcd vertical unfilled", g(0, 6, 8, 8), { display_mode: "lcd", show_unfilled: true, orientation: "vertical" }),
    gauge(5, "lcd vertical not unfilled", g(8, 6, 8, 8), { display_mode: "lcd", show_unfilled: false, orientation: "vertical" }),
  ]),
]);

const DOC_WARN = doc(UID_WARN, "Stat warning", [
  section(null, [panel(1, "stat", "Stat", "SELECT 50 AS a, 150 AS b", { grid: g(0, 0, 12, 6) })]),
]);

/** The tracks of a bar gauge: the background, the radius and the cells. */
const tracksOf = (p, id) => p.eval(`(() => {
  const root = document.querySelector('[data-testid="bargauge-${id}"]');
  return [...root.querySelectorAll(".bargauge__track")].map((t) => {
    const cs = getComputedStyle(t);
    return { bg: cs.backgroundColor, image: cs.backgroundImage, radius: cs.borderTopLeftRadius, cells: t.querySelectorAll(".bargauge__cell").length, mode: root.dataset.mode };
  });
})()`);

const TRANSPARENT = "rgba(0, 0, 0, 0)";

export const cases = [
  {
    id: "B385",
    name: "a bar gauge in the mode lcd draws the track behind the cells when show_unfilled is true, in both themes (9.19)",
    async run(t) {
      const r = await t.api.create("transit", DOC_LCD);
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      for (const theme of ["dark", "light"]) {
        const p = await t.page({ width: 1400, height: 1000 });
        await p.emulateColorScheme(theme);
        await p.goto(t.url(`d/${UID_LCD}/x`));
        await p.idle();
        await waitPanelsReady(p, [1, 2, 3, 4, 5]);
        await p.eval("document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))))");
        const T = (s) => `${theme}: ${s}`;
        const track = async (id) => {
          const list = await tracksOf(p, id);
          t.eq(list.length, 2, T(`panel ${id}: one track for each of the two items`));
          return list;
        };
        const reference = (await track(3))[0];
        t.assert(reference.bg !== TRANSPARENT, T(`the track of the mode basic has a colour: ${reference.bg}`));
        for (const id of [1, 4]) {
          for (const x of await track(id)) {
            t.eq(x.mode, "lcd", T(`panel ${id} is in the mode lcd`));
            t.assert(x.cells > 0, T(`panel ${id}: the cells show`));
            t.eq(x.bg, reference.bg, T(`panel ${id}: show_unfilled true draws the track colour behind the cells`));
            t.eq(x.radius, "4px", T(`panel ${id}: the track has the radius of the other modes`));
          }
        }
        for (const id of [2, 5]) {
          for (const x of await track(id)) {
            t.assert(x.cells > 0, T(`panel ${id}: the cells show`));
            t.eq(x.bg, TRANSPARENT, T(`panel ${id}: show_unfilled false draws no track colour`));
            t.eq(x.image, "none", T(`panel ${id}: no image behind the cells`));
          }
        }
      }
    },
  },
  {
    id: "B386",
    name: "the grid of a stat starts under the warning line of a bad override pattern and still bleeds at the three other edges (9.11, 9.19)",
    async run(t) {
      const r = await t.api.create("transit", DOC_WARN);
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      const p = await t.page({ width: 1400, height: 1000 });
      await p.goto(t.dash(UID_WARN, "edit=1&editPanel=1"));
      await p.waitSel(p.tid("panel-editor"), { timeout: 20000 });
      await p.waitFor(`(() => { const e = document.querySelector(".peditor__top .panel"); return e && e.dataset.state === "ready"; })()`, { what: "the preview" });
      const probe = () => p.eval(`(() => {
        const panel = document.querySelector(".peditor__top .panel");
        const warn = panel.querySelector(".panel__warning");
        const plot = panel.querySelector(".panel__plot");
        const stat = panel.querySelector(".stat");
        if (!stat) return null;
        const s = stat.getBoundingClientRect();
        const q = plot.getBoundingClientRect();
        const w = warn ? warn.getBoundingClientRect() : null;
        return { warnBottom: w ? w.bottom : null, statTop: s.top, plotTop: q.top, left: s.left - q.left, right: q.right - s.right, bottom: q.bottom - s.bottom };
      })()`);
      // Without a warning the grid bleeds by 8 px at the four edges.
      const before = await probe();
      t.assert(before && before.warnBottom === null, "no warning line before the draft changes");
      t.eq(Math.round(before.plotTop - before.statTop), 8, "the grid bleeds by 8 px at the top without a warning");
      // The JSON tab applies the draft with a bad pattern: the frame gets the warning line.
      await p.click(p.tid("tab-json"));
      await p.waitSel(p.tid("panel-json"));
      const current = JSON.parse(await p.eval(`document.querySelector('[data-testid="panel-json"]').value`));
      current.fields = { defaults: {}, overrides: [{ match: { by: "regex", value: "/(/" }, set: { decimals: 1 } }] };
      t.allow(p, /422/);
      await p.eval(`(() => { const a = document.querySelector('[data-testid="panel-json"]'); a.value = ${JSON.stringify(JSON.stringify(current, null, 2))}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
      await p.waitFor(`!!document.querySelector(".peditor__top .panel .panel__warning")`, { what: "the warning line" });
      await p.idle();
      await p.eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))");
      const after = await probe();
      t.assert(after && after.warnBottom !== null, "the warning line shows in the preview");
      t.assert(after.statTop >= after.warnBottom - 0.5, `the grid starts under the warning line: stat top ${after.statTop}, warning bottom ${after.warnBottom}`);
      t.assert(Math.abs(after.statTop - after.plotTop) <= 0.5, `the grid starts at the top of the plot: ${after.statTop} and ${after.plotTop}`);
      for (const edge of ["left", "right", "bottom"]) t.assert(Math.abs(after[edge] + 8) <= 0.5, `the grid still bleeds by 8 px at the ${edge} edge: ${after[edge]}`);
    },
  },
];
