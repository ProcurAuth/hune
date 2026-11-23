// Browser case B212 (9.5, 9.9; gap paper H02, step D02): a link of Grafana 12.4.2 with an ISO range and the name `timezone` opens with that
// range and that zone. The forms that Grafana reads as 1970 and an unknown zone give a notice. The browser is in America/New_York.
import { doc, panel, section } from "../lib/docs.mjs";
import { panelQueries, waitPanelsReady } from "../lib/h.mjs";

const UID = "b212-url-time";
const UNKNOWN = "b212-url-time-zone";
const FROM = 1794211200000; // 2026-11-09T08:00:00.000Z
const TO = 1794232800000; // 2026-11-09T14:00:00.000Z

const mk = (uid, timezone) => doc(uid, `URL time ${timezone}`, [section(null, [panel(1, "table", "Rows", ["SELECT toDateTime(1794211200) AS t, 1 AS v"], { grid: { x: 0, y: 0, w: 12, h: 6 } })])], { time: { from: "now-6h", to: "now", timezone, refresh: null } });

async function open(t, uid, query) {
  const p = await t.page();
  await p.emulateTimezone("America/New_York");
  await p.goto(t.url(`d/${uid}/x?${query}`));
  await p.idle();
  await waitPanelsReady(p, [1]);
  const state = await p.eval(`(() => {
    const b = document.querySelector('[data-testid="time-picker-button"]');
    const notices = [...document.querySelectorAll('[data-testid="notice"]')].map((n) => ({ level: n.dataset.level, hook: n.dataset.notice || null, text: n.querySelector(".notice__text").textContent }));
    return { button: b.querySelector(".picker__text").textContent, label: b.getAttribute("aria-label"), notices, search: location.search };
  })()`);
  const q = panelQueries(p).map((r) => JSON.parse(r.body));
  return { p, ...state, range: q.length ? [q[0].from_ms, q[0].to_ms] : null };
}

// The URL after the first write: a shift back moves the range by half of its span and writes the address (9.9).
async function afterShift(p) {
  const before = await p.eval("location.search");
  await p.click(p.tid("time-back"));
  await p.waitFor(`location.search !== ${JSON.stringify(before)}`, { what: "the address after the shift back" });
  return p.eval("location.search");
}

export const cases = [
  {
    id: "B212",
    name: "a Grafana 12 link: the ISO range, the compact form and timezone=Europe/Paris give that range and zone with no notice; an offset, a bare date, an unknown zone and hostile values give the notice (9.5)",
    async run(t) {
      for (const [uid, zone] of [[UID, "Asia/Tokyo"], [UNKNOWN, "Nowhere/Land"]]) {
        const r = await t.api.create("transit", mk(uid, zone));
        t.assert(r.status === 201, `create ${uid}: ${r.status} ${r.text.slice(0, 400)}`);
      }
      const warnings = (s) => s.notices.filter((n) => n.level === "warning");

      // The URL that Grafana 12.4.2 writes (measure of 2026-11-22): timezone first, the ISO range, orgId=1.
      const a = await open(t, UID, "timezone=Europe/Paris&from=2026-11-09T08:00:00.000Z&to=2026-11-09T14:00:00.000Z&orgId=1");
      t.eq(a.button, "2026-11-09 09:00:00 to 2026-11-09 15:00:00", "the range button shows the range in Europe/Paris");
      t.assert(a.label.endsWith(", Europe/Paris"), `the name of the button holds the zone: ${a.label}`);
      t.eq(JSON.stringify(a.range), JSON.stringify([FROM, TO]), "the panel request has the range of the link");
      t.eq(JSON.stringify(warnings(a)), "[]", "no warning notice");
      const written = await afterShift(a.p);
      t.eq(written, `?from=${FROM - 10800000}&to=${TO - 10800000}&timezone=Europe%2FParis&refresh=off`, "the page writes epoch milliseconds, then timezone after to, and drops orgId");

      // The compact form and the form with no zone letter, read as UTC; the zone utc.
      const b = await open(t, UID, "from=20261109T080000&to=2026-11-09T14:00:00&timezone=utc");
      t.eq(b.button, "2026-11-09 08:00:00 to 2026-11-09 14:00:00", "the compact form and the form with no zone letter are UTC");
      t.eq(JSON.stringify(b.range), JSON.stringify([FROM, TO]), "the same range");
      t.eq(JSON.stringify(warnings(b)), "[]", "no warning notice");

      // The milliseconds of the ISO form stay; with no timezone the zone of the document (Asia/Tokyo) applies.
      const c = await open(t, UID, "from=2026-11-09T08:00:00.123Z&to=2026-11-09T14:00:00.000Z");
      t.eq(JSON.stringify(c.range), JSON.stringify([FROM + 123, TO]), "the milliseconds of the ISO form stay");
      t.eq(c.button, "2026-11-09 17:00:00 to 2026-11-09 23:00:00", "the zone of the document applies with no timezone");

      // An offset and a bare date: Grafana gives 1970; Hune refuses them with the notice and uses the range of the document.
      for (const [query, text] of [["from=2026-11-09T08:00:00%2B01:00&to=2026-11-09T14:00:00.000Z", "2026-11-09T08:00:00+01:00"], ["from=2026-11-09&to=2026-11-10", "2026-11-09"]]) {
        const d = await open(t, UID, query);
        t.eq(JSON.stringify(warnings(d).map((n) => n.text)), JSON.stringify([`The time range of the address is not used: The time text "${text}" is not valid.`]), `the notice of ${text}`);
        t.eq(d.button, "Last 6 hours", `${text}: the range of the document`);
        t.assert(d.range && d.range[1] - d.range[0] === 21600000, `${text}: the request has the range of the document: ${d.range}`);
      }

      // An unknown zone: the notice, and the zone of the document. The page drops the name at the next write.
      const e = await open(t, UID, "timezone=Mars/Olympus&from=2026-11-09T08:00:00.000Z&to=2026-11-09T14:00:00.000Z");
      t.eq(JSON.stringify(warnings(e).map((n) => [n.hook, n.text])), JSON.stringify([["zone-url", "The time zone “Mars/Olympus” of the address is not known to this browser. The page uses the zone of the dashboard."]]), "the notice of an unknown zone");
      t.eq(e.button, "2026-11-09 17:00:00 to 2026-11-09 23:00:00", "the zone of the document (Asia/Tokyo) applies");
      t.eq(JSON.stringify(e.range), JSON.stringify([FROM, TO]), "the range of the link still applies");
      const dropped = await afterShift(e.p);
      t.assert(!dropped.includes("timezone"), `the page drops the unknown name: ${dropped}`);

      // A known zone of the URL: no notice of the unknown zone of the document.
      const f = await open(t, UNKNOWN, "timezone=utc&from=2026-11-09T08:00:00.000Z&to=2026-11-09T14:00:00.000Z");
      t.eq(JSON.stringify(warnings(f)), "[]", "no notice of the zone of the document when the URL gives a known zone");
      t.eq(f.button, "2026-11-09 08:00:00 to 2026-11-09 14:00:00", "the zone of the URL applies");

      // Hostile values: a zone of 2,000 characters with markup and a time of 4,000 digits (the server refuses a target above 8 KiB, 6.27).
      // The notices cut them, and the page shows text only.
      const zone = `<img src=x onerror=window.__b212=1>${"Z".repeat(2000)}`;
      const g = await open(t, UID, `timezone=${encodeURIComponent(zone)}&from=${"9".repeat(4000)}&to=now`);
      const texts = warnings(g).map((n) => n.text);
      t.eq(texts.length, 2, `two notices: ${JSON.stringify(texts).slice(0, 400)}`);
      t.assert(texts.every((x) => x.length < 200 && x.includes("…")), `the notices cut the values: ${JSON.stringify(texts).slice(0, 600)}`);
      t.eq(await g.p.eval("window.__b212 === undefined && !document.querySelector('#app-notices img')"), true, "the markup is text: no element, no script");
      t.eq(g.button, "Last 6 hours", "the range of the document");
    },
  },
];
