// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B63 (16.10): the zone names (the Grafana-parity work: F35, 9.9). The chart part: the labels of the x axis and the head of the tooltip
// follow the zone of the dashboard, a name of an IANA zone, whatever the zone of the browser. The cells of a `time` field and the notice of an unknown
// name are checked too. The runner sets the zone of the browser with `Emulation.setTimezoneOverride`.
import { doc, panel, section } from "../lib/docs.mjs";
import { SEED_FROM, SEED_TO } from "../lib/env.mjs";
import { axisTexts, reveal, tooltipAt, waitPanelsReady } from "../lib/h.mjs";

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const g = (x, y, w, h) => ({ x, y, w, h });

/** The wall clock of an epoch time in a zone, as `YYYY-MM-DD HH:mm:ss` (the same tzdata as the browser). */
function clock(ms, zone) {
  const parts = Object.fromEntries(new Intl.DateTimeFormat("en-GB", { timeZone: zone, hourCycle: "h23", year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit" }).formatToParts(new Date(ms)).map((p) => [p.type, p.value]));
  return `${parts.year}-${parts.month}-${parts.day} ${parts.hour}:${parts.minute}:${parts.second}`;
}

// The minutes `HH:mm` that a range covers in a zone.
function minutesOf(from, to, zone) {
  const out = new Set();
  for (let ms = from; ms <= to; ms += 60000) out.add(clock(ms, zone).slice(11, 16));
  return out;
}

export const cases = [
  {
    id: "B63",
    name: "zone names: Asia/Tokyo in a New York browser and the reverse; a clock change; an unknown name gives the notice and the browser zone",
    async run(t) {
      const series = ["SELECT toDateTime(1794211200 + number * 60) AS t, toFloat64(number) AS v", "FROM numbers(20)"];
      const cells = ["SELECT event_time, blocks FROM analytics.seed ORDER BY event_time"];
      const mk = (uid, tz) => doc(uid, `Zone ${tz}`, [
        section(null, [
          panel(1, "timeseries", "Series", series, { grid: g(0, 0, 14, 8) }),
          panel(2, "table", "Cells", cells, { grid: g(14, 0, 10, 8) }),
        ]),
      ], { time: { from: "now-6h", to: "now", timezone: tz, refresh: null } });
      await ensure(t, mk("zone-tokyo", "Asia/Tokyo"));
      await ensure(t, mk("zone-new-york", "America/New_York"));
      await ensure(t, mk("zone-unknown", "Nowhere/Land"));

      const run = async (url, browserZone, ids = [1, 2]) => {
        const p = await t.page();
        await p.emulateTimezone(browserZone);
        await p.recordCanvasText();
        await p.goto(url);
        await p.idle();
        await waitPanelsReady(p, ids);
        const x = (await axisTexts(p, 1)).x;
        const tip = await tooltipAt(p, 1, 0.5);
        const cell = ids.includes(2) ? await p.eval(`(() => { const r = document.querySelector('[data-testid="table-2"] tbody tr'); return r ? r.children[0].textContent : ""; })()`) : "";
        const notice = await p.eval(`!!document.querySelector('[data-notice="zone"]')`);
        const resolved = await p.eval(`Intl.DateTimeFormat().resolvedOptions().timeZone`);
        return { p, x, tip, cell, notice, resolved };
      };
      const hhmm = (label) => label.slice(0, 5);

      // The dashboard in Asia/Tokyo, the browser in New York: the labels, the head of the tooltip and the cell are the clock of Tokyo.
      const a = await run(t.dash("zone-tokyo"), "America/New_York");
      t.eq(a.resolved, "America/New_York", "the browser zone applies");
      const tokyoMinutes = minutesOf(SEED_FROM, SEED_TO, "Asia/Tokyo");
      const nyMinutes = minutesOf(SEED_FROM, SEED_TO, "America/New_York");
      t.assert(a.x.length >= 3, `x labels: ${a.x}`);
      t.assert(a.x.every((l) => tokyoMinutes.has(hhmm(l))), `every label is a minute of Tokyo in the range: ${a.x}`);
      t.assert(!a.x.some((l) => nyMinutes.has(hhmm(l)) && !tokyoMinutes.has(hhmm(l))), "no label is a minute of New York");
      const tokyoHeads = new Set(Array.from({ length: 20 }, (_, i) => clock(SEED_FROM + i * 60000, "Asia/Tokyo")));
      t.assert(a.tip && tokyoHeads.has(a.tip.head.slice(0, 19)), `tooltip head ${a.tip && a.tip.head} is a time of the series in Tokyo`);
      t.assert(a.cell.includes(clock(SEED_FROM, "Asia/Tokyo")), `the first cell ${a.cell} holds ${clock(SEED_FROM, "Asia/Tokyo")}`);
      t.assert(!a.notice, "a known name gives no notice");

      // The dashboard in America/New_York, the browser in Tokyo: the reverse.
      const b = await run(t.dash("zone-new-york"), "Asia/Tokyo");
      t.eq(b.resolved, "Asia/Tokyo", "the browser zone applies");
      t.assert(b.x.every((l) => nyMinutes.has(hhmm(l))), `every label is a minute of New York in the range: ${b.x}`);
      const nyHeads = new Set(Array.from({ length: 20 }, (_, i) => clock(SEED_FROM + i * 60000, "America/New_York")));
      t.assert(b.tip && nyHeads.has(b.tip.head.slice(0, 19)), `tooltip head ${b.tip && b.tip.head} is a time of the series in New York`);
      t.assert(b.cell.includes(clock(SEED_FROM, "America/New_York")), `the first cell ${b.cell} holds ${clock(SEED_FROM, "America/New_York")}`);
      t.assert(a.cell !== b.cell, "the two zones give two texts");

      // A day of a clock change: New York, from 2026-03-07 07:00 to 2026-03-09 08:00 local time (the clocks change on 2026-03-08 at 02:00). The browser is in Tokyo.
      // The labels are the clock of New York: each label is the wall clock of the instant under it, with the offset of that instant.
      const from = Date.UTC(2026, 2, 7, 12);
      const to = Date.UTC(2026, 2, 9, 12);
      const c = await run(t.url(`d/zone-new-york?from=${from}&to=${to}`), "Asia/Tokyo", [1]);
      const ticks = await c.p.eval(`(() => {
        const r = document.querySelector('[data-testid="panel-1"] .chartCore');
        const [left, , w] = r.dataset.plot.split(" ").map(Number);
        const lo = Number(r.dataset.xMin), hi = Number(r.dataset.xMax);
        // A label near an edge of the plot is moved inside it: its place is not the place of its tick. Read the others.
        return JSON.parse(r.dataset.xTicks).map((x) => ({ label: x[0], centre: (x[2] + x[3]) / 2 - left, at: lo + (((x[2] + x[3]) / 2 - left) / w) * (hi - lo), w })).filter((x) => x.centre > 40 && x.centre < x.w - 40);
      })()`);
      t.assert(ticks.length >= 3, `the axis has labels over two days: ${JSON.stringify(ticks)}`);
      for (const tick of ticks) {
        const m = /(\d\d):(\d\d)$/.exec(tick.label);
        t.assert(m, `a label ends with a clock: ${tick.label}`);
        const want = clock(tick.at, "America/New_York").slice(11, 16);
        const diff = Math.abs((Number(m[1]) * 60 + Number(m[2])) - (Number(want.slice(0, 2)) * 60 + Number(want.slice(3))));
        // The position of a label is rounded to a pixel (about 5 minutes over two days). A wrong offset is one hour or more.
        t.assert(Math.min(diff, 1440 - diff) <= 12, `the label ${tick.label} is the clock of New York at its place (${want})`);
      }
      const hours = new Set(ticks.map((x) => x.label.slice(-5, -3)));
      t.assert(hours.size >= 2, `labels at several hours: ${[...hours]}`);

      // An unknown name: the notice shows, and the page uses the zone of the browser.
      const u = await run(t.dash("zone-unknown"), "Asia/Tokyo");
      t.assert(u.notice, "an unknown zone name gives the notice (data-notice=zone)");
      const browserMinutes = minutesOf(SEED_FROM, SEED_TO, "Asia/Tokyo");
      t.assert(u.x.every((l) => browserMinutes.has(hhmm(l))), `the labels follow the zone of the browser: ${u.x}`);
      t.assert(u.cell.includes(clock(SEED_FROM, "Asia/Tokyo")), `the first cell follows the zone of the browser: ${u.cell}`);
    },
  },
];
