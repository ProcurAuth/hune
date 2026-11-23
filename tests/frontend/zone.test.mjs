// SPDX-License-Identifier: MIT OR Apache-2.0
// The time zone of the page (9.19, decision F10): formatTime, the clock of a zone, and the x axis of the chart engine in a zone.
import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { formatTime, zoneClock } from "../../web/js/zone.js";
import { chartCore } from "../../web/js/vendor/chdash/app_chart_core.js";

const here = dirname(fileURLToPath(import.meta.url));
const ZONE_JS = join(here, "../../web/js/zone.js");
const CORE_JS = join(here, "../../web/js/vendor/chdash/app_chart_core.js");

// 2026-11-09 08:00:00.250 UTC.
const T = Date.UTC(2026, 10, 9, 8, 0, 0, 250);

test("formatTime writes the tokens of the pattern in UTC", () => {
  assert.equal(formatTime(T, "utc"), "2026-11-09 08:00:00");
  assert.equal(formatTime(T, "utc", "HH:mm"), "08:00");
  assert.equal(formatTime(T, "utc", "YYYY-MM-DD HH:mm:ss.SSS"), "2026-11-09 08:00:00.250");
  assert.equal(formatTime(T, "utc", "MM/DD"), "11/09");
  // Other characters stay as they are.
  assert.equal(formatTime(T, "utc", "[DD] of MM"), "[09] of 11");
});

test("formatTime writes an IANA zone by name", () => {
  assert.equal(formatTime(T, "Asia/Tokyo", "YYYY-MM-DD HH:mm"), "2026-11-09 17:00");
  assert.equal(formatTime(T, "America/New_York", "YYYY-MM-DD HH:mm"), "2026-11-09 03:00");
  // The offset of a zone follows its rules for the date (daylight saving time).
  assert.equal(formatTime(Date.UTC(2026, 6, 1, 12), "America/New_York", "HH:mm"), "08:00");
});

test("formatTime gives an empty text for a time that is not a number, and falls back to the browser zone for an unknown zone", () => {
  assert.equal(formatTime(NaN, "utc"), "");
  assert.equal(formatTime(Infinity, "utc"), "");
  assert.equal(formatTime(undefined, "utc"), "");
  assert.equal(formatTime(T, "Nowhere/Land", "HH:mm"), formatTime(T, "browser", "HH:mm"));
  assert.deepEqual(zoneClock("Nowhere/Land").parts(T), zoneClock("browser").parts(T));
});

test("the clock of a zone gives the wall clock and the epoch time of a wall clock, with overflow", () => {
  const k = zoneClock("Asia/Tokyo");
  assert.deepEqual(k.parts(T), { y: 2026, mo: 11, d: 9, h: 17, mi: 0, s: 0, ms: 250 });
  assert.equal(k.epoch(2026, 11, 9, 17), T - 250);
  // Day 0 is the last day of the month before.
  assert.equal(k.epoch(2026, 11, 0, 17), T - 250 - 86400000 * 9);
  const u = zoneClock("utc");
  assert.equal(u.epoch(2026, 13, 1), Date.UTC(2027, 0, 1));
  assert.equal(u.parts(T).h, 8);
});

// A fixed width for each character, so that the ticks do not depend on a font.
const measure = (text) => text.length * 6.75;
const DEMO_START = 1794305000000;
const DEMO_END = 1794327000000;

test("the time axis puts a tick every 15 minutes where the room of a label allows it (Grafana, 6 hours at 1183 px)", () => {
  const ticks = chartCore.timeTicks(DEMO_START, DEMO_END, 1183, measure, zoneClock("utc"));
  assert.equal(ticks[1].v - ticks[0].v, 15 * 60000);
  assert.match(ticks[0].label, /^\d\d:\d\d$/);
  // At 1173 px, a range of 24 minutes has ticks every 5 minutes, because a step of 1 minute writes the seconds and needs more room.
  const short = chartCore.timeTicks(DEMO_START, DEMO_START + 24 * 60000, 1173, measure, zoneClock("utc"));
  assert.equal(short[1].v - short[0].v, 5 * 60000);
  // A range of 12 minutes has ticks every minute, with the seconds.
  const twelve = chartCore.timeTicks(DEMO_START, DEMO_START + 12 * 60000, 1173, measure, zoneClock("utc"));
  assert.equal(twelve[1].v - twelve[0].v, 60000);
  assert.match(twelve[0].label, /^\d\d:\d\d:00$/);
});

test("the time axis has no date inside one day, also at midnight, and writes the date over one day", () => {
  const start = Date.UTC(2026, 10, 9, 20);
  const day = chartCore.timeTicks(start, start + 6 * 3600000, 700, measure, zoneClock("utc"));
  assert.ok(day.some((t) => t.label === "00:00"));
  assert.ok(day.every((t) => /^\d\d:\d\d$/.test(t.label)));
  const long = chartCore.timeTicks(start, start + 30 * 3600000, 700, measure, zoneClock("utc"));
  assert.ok(long.every((t) => /^\d\d\/\d\d \d\d:\d\d$/.test(t.label)));
  const days = chartCore.timeTicks(start, start + 14 * 86400000, 700, measure, zoneClock("utc"));
  assert.ok(days.every((t) => /^\d\d\/\d\d$/.test(t.label)));
});

test("a UTC axis puts its ticks on UTC boundaries, whatever the zone of the browser", () => {
  const start = Date.UTC(2026, 10, 9, 20);
  const ticks = chartCore.timeTicks(start, start + 6 * 3600000, 700, measure, zoneClock("utc"));
  for (const t of ticks) assert.equal(t.v % 1800000, 0, "a half hour in UTC");
  // The zone Asia/Kolkata has an offset of 5 h 30: its hours are not hours in UTC.
  const kolkata = chartCore.timeTicks(start, start + 6 * 3600000, 700, measure, zoneClock("Asia/Kolkata"));
  assert.ok(kolkata.every((t) => (t.v + 5.5 * 3600000) % 1800000 === 0));
  assert.ok(kolkata.some((t) => t.v % 3600000 !== 0));
});

// The page in two zones of the browser: the labels of a UTC axis are the same, the labels of the zone "browser" differ (16.10).
function inZone(tz, code) {
  const script = `
    import { formatTime, zoneClock } from ${JSON.stringify(ZONE_JS)};
    import { chartCore } from ${JSON.stringify(CORE_JS)};
    const measure = (t) => t.length * 6.75;
    const start = ${DEMO_START};
    const labels = (zone) => chartCore.timeTicks(start, start + 6 * 3600000, 900, measure, zoneClock(zone)).map((t) => t.label);
    ${code}
  `;
  return execFileSync(process.execPath, ["--input-type=module", "-e", script], { env: { ...process.env, TZ: tz }, encoding: "utf8" }).trim();
}

test("a UTC dashboard shows the same texts in each zone of the browser, a browser dashboard does not", () => {
  const code = `console.log(JSON.stringify({ utc: labels("utc"), head: formatTime(start, "utc"), browser: labels("browser"), browserHead: formatTime(start, "browser") }));`;
  const tokyo = JSON.parse(inZone("Asia/Tokyo", code));
  const la = JSON.parse(inZone("America/Los_Angeles", code));
  assert.deepEqual(tokyo.utc, la.utc);
  assert.equal(tokyo.head, la.head);
  assert.equal(tokyo.head, formatTime(DEMO_START, "utc"));
  assert.notEqual(tokyo.browserHead, la.browserHead);
  assert.equal(tokyo.browserHead, formatTime(DEMO_START, "Asia/Tokyo"));
  assert.equal(la.browserHead, formatTime(DEMO_START, "America/Los_Angeles"));
});

// F35, the axis part (9.9, 9.19, B63): the x axis of a chart in a named zone.
const NY_DST_START = Date.UTC(2026, 2, 7, 5); // 2026-03-07 00:00 in New York (UTC-5); the clocks change on 2026-03-08 at 02:00.
const NY_FIVE_DAYS = NY_DST_START + 5 * 86400000;

test("an axis in a named zone puts the day ticks on the midnights of that zone, also over a clock change", () => {
  const ticks = chartCore.timeTicks(NY_DST_START, NY_FIVE_DAYS, 700, measure, zoneClock("America/New_York"));
  const clock = zoneClock("America/New_York");
  assert.ok(ticks.length >= 3);
  for (const t of ticks) {
    const p = clock.parts(t.v);
    assert.equal(p.h, 0, `a tick at ${t.label} is a local midnight`);
    assert.equal(p.mi, 0);
  }
  // The day of the change has 23 hours: two ticks one day apart are 23 hours apart there.
  const gaps = ticks.slice(1).map((t, k) => t.v - ticks[k].v);
  assert.ok(gaps.includes(23 * 3600000), "the day of the change is 23 hours long");
  assert.ok(gaps.includes(24 * 3600000));
  // The label of a day is the date of the zone.
  assert.deepEqual(ticks.map((t) => t.label), ticks.map((t) => formatTime(t.v, "America/New_York", "MM/DD")));
});

test("the head of the tooltip and the labels follow the named zone, not the zone of the browser", () => {
  const code = `console.log(JSON.stringify({ tokyo: labels("Asia/Tokyo"), ny: labels("America/New_York"), head: formatTime(start, "Asia/Tokyo") }));`;
  const inNy = JSON.parse(inZone("America/New_York", code));
  const inTokyo = JSON.parse(inZone("Asia/Tokyo", code));
  assert.deepEqual(inNy.tokyo, inTokyo.tokyo);
  assert.deepEqual(inNy.ny, inTokyo.ny);
  assert.equal(inNy.head, formatTime(DEMO_START, "Asia/Tokyo"));
  assert.notDeepEqual(inNy.tokyo, inNy.ny);
});
