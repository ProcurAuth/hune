// Browser case B64 (16.10): the time override of a panel (milestone F, wave 2: F18, 9.8b).
// The requests of the panels carry the range that 9.8b computes. The note of the title bar has its text, its style and its tooltip.
// `hide_time_override` removes the note, a `reuse` panel shows the note of its source, and an unusable range gives the state `error`.
import { doc, panel, section } from "../lib/docs.mjs";
import { panelIdOf, panelQueries, waitPanelsReady } from "../lib/h.mjs";

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const g = (x, y, w, h) => ({ x, y, w, h });
const DAY = 86400000;
const HOUR = 3600000;

/** The last body of the query requests of each panel: `{ "1": { from_ms, to_ms, ... } }`. */
function bodies(p) {
  const out = {};
  for (const r of panelQueries(p)) out[panelIdOf(r)] = JSON.parse(r.body);
  return out;
}

/** The text and the facts of the note of a panel, or null when the panel has no note. */
function noteOf(p, id) {
  return p.eval(`(() => {
    const n = document.querySelector('[data-testid="panel-${id}-time-note"]');
    if (!n) return null;
    const s = getComputedStyle(n);
    const svg = n.querySelector("svg");
    const title = document.querySelector('[data-testid="panel-${id}"] .panel__title');
    return { text: n.textContent, tip: n.getAttribute("data-tooltip"), color: s.color, size: s.fontSize, weight: s.fontWeight,
      icon: svg ? [svg.getBoundingClientRect().width, svg.getBoundingClientRect().height] : null,
      gap: s.columnGap, after: title ? n.getBoundingClientRect().left >= title.getBoundingClientRect().right : null };
  })()`);
}

const clock = (ms) => new Date(ms).toISOString().replace("T", " ").slice(0, 19);

export const cases = [
  {
    id: "B64",
    name: "time override: the range of each panel, the note of the title bar, hide_time_override, a reuse panel, an absolute dashboard range, an unusable range",
    async run(t) {
      const sql = ["SELECT 1 AS x"];
      const d = doc("time-override", "Time override", [
        section(null, [
          panel(1, "table", "Last 24 hours", sql, { grid: g(0, 0, 8, 5) }),
          panel(2, "table", "Shifted", sql, { grid: g(8, 0, 8, 5) }),
          panel(3, "table", "Both", sql, { grid: g(16, 0, 8, 5) }),
          panel(4, "table", "Plain", sql, { grid: g(0, 5, 8, 5) }),
          panel(5, "table", "Hidden note", sql, { grid: g(8, 5, 8, 5) }),
          panel(6, "table", "Reuse of 1", null, { grid: g(16, 5, 8, 5), reuse: { panel: 1, with_transforms: true } }),
          panel(7, "table", "Today", sql, { grid: g(0, 10, 8, 5) }),
          panel(8, "table", "Unusable", sql, { grid: g(8, 10, 8, 5) }),
        ]),
      ], { time: { from: "now-6h", to: "now", timezone: "utc", refresh: null } });
      const q = (id) => d.sections[0].panels[id - 1].data.query;
      q(1).time_from = "now-24h";
      q(2).time_shift = "1d";
      q(3).time_from = "now-24h";
      q(3).time_shift = "1d";
      q(5).time_from = "now-24h";
      q(5).hide_time_override = true;
      q(7).time_from = "now/d";
      q(8).time_shift = "999999999y";
      await ensure(t, d);

      // A relative range: each request carries the range of 9.8b.
      const before = Date.now();
      const p = await t.open(t.url("d/time-override"));
      await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 7]);
      await waitPanelsReady(p, [8], "error");
      const after = Date.now();
      const b = bodies(p);
      t.assert(!b[6] && !b[8], "a reuse panel and a panel with an unusable range send no query");
      const near = (ms, want, what) => t.assert(ms >= want - (after - before) - 2000 && ms <= want + 2000, `${what}: ${new Date(ms).toISOString()} is not near ${new Date(want).toISOString()}`);
      t.eq(b[1].to_ms - b[1].from_ms, DAY, "panel 1: 24 hours");
      near(b[1].to_ms, after, "panel 1 ends at now");
      t.eq(b[2].to_ms - b[2].from_ms, 6 * HOUR, "panel 2: the span of the dashboard");
      near(b[2].to_ms, after - DAY, "panel 2 ends one day before now");
      t.eq(b[3].to_ms - b[3].from_ms, DAY, "panel 3: 24 hours");
      near(b[3].to_ms, after - DAY, "panel 3 ends one day before now");
      t.eq(b[4].to_ms - b[4].from_ms, 6 * HOUR, "panel 4: the range of the dashboard");
      near(b[4].to_ms, after, "panel 4 ends at now");
      t.eq(b[5].to_ms - b[5].from_ms, DAY, "panel 5 has its override, with no note");
      const startOfDay = Math.floor(b[7].to_ms / DAY) * DAY;
      t.eq(b[7].from_ms, startOfDay, "panel 7: the start of the day in UTC");

      // The notes.
      const n1 = await noteOf(p, 1);
      t.eq(n1 && n1.text, "Last 24 hours", "note of panel 1");
      t.assert(n1.after, "the note follows the title");
      t.eq([n1.size, n1.weight, n1.gap], ["14px", "400", "8px"], "font 14 px, weight 400, gap 8 px");
      t.eq(n1.icon, [14, 14], "the clock icon is 14 px");
      t.assert(["rgb(110, 159, 255)", "rgb(31, 98, 224)"].includes(n1.color), `the link colour of a theme: ${n1.color}`);
      t.eq(n1.tip, `${clock(b[1].from_ms)} to ${clock(b[1].to_ms)}`, "the tooltip is the range in the zone of the dashboard");
      t.eq((await noteOf(p, 2)).text, "Timeshift -1d", "note of panel 2");
      t.eq((await noteOf(p, 3)).text, "Last 24 hours + timeshift -1d", "note of panel 3");
      t.eq(await noteOf(p, 4), null, "panel 4 has no note");
      t.eq(await noteOf(p, 5), null, "hide_time_override removes the note");
      t.eq((await noteOf(p, 6)).text, "Last 24 hours", "a reuse panel shows the note of its source");
      t.eq((await noteOf(p, 7)).text, "Today so far", "note of panel 7");
      const err = await p.eval(`(() => { const e = document.querySelector('[data-testid="panel-8-error"]'); return e ? e.textContent : null; })()`);
      t.assert(err && err.includes("The time override of this panel gives a range that Hune cannot use."), `panel 8 error: ${err}`);

      // An absolute range of the dashboard: time_from does nothing, the shift applies, and the panel with time_from shows no note.
      const from = 1794211200000;
      const to = 1794214800000;
      const a = await t.open(t.url(`d/time-override?from=${from}&to=${to}`));
      await waitPanelsReady(a, [1, 2, 3, 4, 5, 6, 7]);
      const c = bodies(a);
      t.eq([c[1].from_ms, c[1].to_ms], [from, to], "panel 1: time_from does nothing");
      t.eq([c[2].from_ms, c[2].to_ms], [from - DAY, to - DAY], "panel 2: the shift applies");
      t.eq([c[3].from_ms, c[3].to_ms], [from - DAY, to - DAY], "panel 3: only the shift applies");
      t.eq([c[4].from_ms, c[4].to_ms], [from, to], "panel 4: the range of the dashboard");
      t.eq(await noteOf(a, 1), null, "no note when time_from does nothing");
      t.eq((await noteOf(a, 3)).text, "Timeshift -1d", "the note of panel 3 is the note of the shift alone");
      t.eq(await noteOf(a, 6), null, "the reuse panel follows its source");
    },
  },
];
