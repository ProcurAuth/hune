// FE-01: date math (9.9), in three zones.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { effectiveZone, isKnownZone, parseTimeText, resolveRange, wallToEpoch, zoneLabel, zoomOut, shiftRange, zoneOffsetLabel, MAX_MS, quickLabel, formatRangeText, parseAbsoluteInput, QUICK_RANGES, refreshToMs, urlTimeText, cut } from "../../web/js/time.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-01-time.json", import.meta.url)));

test("FE-01 relative and absolute times in utc, Paris and New York", () => {
  for (const c of v.parse) {
    const got = parseTimeText(c.text, { now: Date.parse(c.now), zone: c.zone, edge: c.edge });
    assert.equal(got.error, undefined, `${c.zone} ${c.text} ${c.edge}`);
    assert.equal(new Date(got.ms).toISOString(), c.expect, `${c.zone} ${c.now} ${c.text} ${c.edge}`);
  }
});

test("FE-01 errors", () => {
  for (const c of v.errors) {
    const got = parseTimeText(c.text, { now: Date.parse(c.now), zone: c.zone, edge: c.edge });
    assert.ok(got.error, c.text);
    if (c.error) assert.equal(got.error, c.error);
    if (c.error_contains) assert.ok(got.error.includes(c.error_contains), got.error);
  }
});

test("FE-01 ranges: from must be below to", () => {
  for (const c of v.ranges) {
    const got = resolveRange(c.from, c.to, Date.parse(c.now), c.zone);
    if (c.error_contains) assert.ok(got.error && got.error.includes(c.error_contains), JSON.stringify(got));
    else {
      assert.equal(new Date(got.from_ms).toISOString(), c.from_ms);
      assert.equal(new Date(got.to_ms).toISOString(), c.to_ms);
    }
  }
});

test("FE-01 the browser zone resolves through the same path", () => {
  const now = Date.parse("2026-11-09T14:30:00.000Z");
  const a = parseTimeText("now/d", { now, zone: "browser" });
  const b = parseTimeText("now/d", { now, zone: Intl.DateTimeFormat().resolvedOptions().timeZone });
  assert.equal(a.ms, b.ms);
});

test("FE-01 zoom out, quick labels, picker text, absolute input, refresh", () => {
  assert.deepEqual(Object.values(zoomOut(v.zoomOut.from, v.zoomOut.to)), v.zoomOut.expect);
  for (const [key, label] of Object.entries(v.quick)) {
    const [f, t] = key.split("|");
    assert.equal(quickLabel(f, t), label);
  }
  assert.equal(QUICK_RANGES.length, 23);
  assert.equal(formatRangeText("now-5h", "now", "utc"), "now-5h to now");
  assert.equal(formatRangeText("1794211200000", "1794232800000", "utc"), "2026-11-09 08:00:00 to 2026-11-09 14:00:00");
  assert.equal(parseAbsoluteInput("2026-11-09 08:00:00", "utc").ms, Date.UTC(2026, 10, 9, 8));
  assert.ok(parseAbsoluteInput("2026-13-09 08:00:00", "utc").error);
  assert.equal(parseAbsoluteInput("now-1h", "utc", { now: 3600000 * 5 }).ms, 3600000 * 4);
  assert.equal(refreshToMs("1m"), 60000);
  assert.equal(refreshToMs("7s"), null);
});

test("FE-01 shift back and forward by half of the span (as Grafana), kept inside the limits", () => {
  const h = 3600000;
  // Measured in Grafana 12.4.2: a range of 6 h moves by 3 h.
  assert.deepEqual(shiftRange(14 * h, 20 * h, -1), { from_ms: 11 * h, to_ms: 17 * h });
  assert.deepEqual(shiftRange(14 * h, 20 * h, 1), { from_ms: 17 * h, to_ms: 23 * h });
  // back then forward gives the first range again
  const back = shiftRange(14 * h, 20 * h, -1);
  assert.deepEqual(shiftRange(back.from_ms, back.to_ms, 1), { from_ms: 14 * h, to_ms: 20 * h });
  // an odd span rounds the step down to a whole millisecond
  assert.deepEqual(shiftRange(0 + 10, 10 + 5, 1), { from_ms: 12, to_ms: 17 });
  // limits: the span stays
  assert.deepEqual(shiftRange(h, 3 * h, -1), { from_ms: 0, to_ms: 2 * h });
  assert.deepEqual(shiftRange(MAX_MS - 4 * h, MAX_MS - h, 1), { from_ms: MAX_MS - 3 * h, to_ms: MAX_MS });
});

test("FE-01 the offset label of a zone", () => {
  const winter = Date.UTC(2026, 0, 15, 12);
  const summer = Date.UTC(2026, 6, 15, 12);
  assert.equal(zoneOffsetLabel(winter, "utc"), "UTC+00:00");
  assert.equal(zoneOffsetLabel(winter, "Europe/Paris"), "UTC+01:00");
  assert.equal(zoneOffsetLabel(summer, "Europe/Paris"), "UTC+02:00");
  assert.equal(zoneOffsetLabel(winter, "America/New_York"), "UTC-05:00");
  assert.equal(zoneOffsetLabel(winter, "Asia/Kolkata"), "UTC+05:30");
});

// F35 (9.9, D15, V8): the zone names.
test("FE-01 zone names: isKnownZone, zoneLabel and effectiveZone", () => {
  for (const name of ["browser", "utc", "UTC", "Asia/Tokyo", "America/New_York", "Europe/Paris", "America/Argentina/Buenos_Aires", "Etc/GMT+5"]) assert.equal(isKnownZone(name), true, name);
  for (const name of ["Nowhere/Land", "Mars/Olympus", "", "Europe/", "a b", "Europe/Paris ", "x\u0000y", "A".repeat(65), null, undefined, 5, {}, []]) assert.equal(isKnownZone(name), false, String(name));
  assert.equal(zoneLabel("utc"), "UTC");
  assert.equal(zoneLabel("browser"), "Browser time");
  assert.equal(zoneLabel("Europe/Paris"), "Europe/Paris");
  assert.equal(effectiveZone("Asia/Tokyo"), "Asia/Tokyo");
  assert.equal(effectiveZone("Nowhere/Land"), "browser");
  assert.equal(effectiveZone("utc"), "utc");
});

test("FE-01 a named zone: the same clock arithmetic as the browser zone, whatever the zone of the browser", () => {
  const now = Date.parse("2026-11-09T14:30:00.000Z");
  assert.equal(new Date(parseTimeText("now/d", { now, zone: "Asia/Tokyo" }).ms).toISOString(), "2026-11-08T15:00:00.000Z");
  assert.equal(new Date(parseTimeText("now/d", { now, zone: "America/New_York" }).ms).toISOString(), "2026-11-09T05:00:00.000Z");
  assert.equal(new Date(parseTimeText("now/d", { now, zone: "America/New_York", edge: "to" }).ms).toISOString(), "2026-11-10T04:59:59.999Z");
});

test("FE-01 V8: a local time that does not exist uses the offset before the change", () => {
  assert.equal(new Date(wallToEpoch({ year: 2026, month: 3, day: 29, hour: 2, minute: 30, second: 0 }, "Europe/Paris")).toISOString(), "2026-03-29T01:30:00.000Z");
  // Havana: the clock goes from 23:59 to 01:00 on 2026-03-08, so midnight does not exist: `now/d` gives 05:00Z (UTC-5, the offset before).
  const havana = parseTimeText("now/d", { now: Date.parse("2026-03-08T15:00:00.000Z"), zone: "America/Havana" });
  assert.equal(new Date(havana.ms).toISOString(), "2026-03-08T05:00:00.000Z");
});

test("FE-01 V8: a local time that occurs two times has the earlier instant (Grafana gives the later one, ID-F2-V8)", () => {
  assert.equal(new Date(wallToEpoch({ year: 2026, month: 10, day: 25, hour: 2, minute: 30, second: 0 }, "Europe/Paris")).toISOString(), "2026-10-25T00:30:00.000Z");
  // Havana, 2026-11-01: midnight occurs two times (04:00Z and 05:00Z). Grafana gives 05:00Z, Hune gives 04:00Z.
  const havana = parseTimeText("now/d", { now: Date.parse("2026-11-01T15:00:00.000Z"), zone: "America/Havana" });
  assert.equal(new Date(havana.ms).toISOString(), "2026-11-01T04:00:00.000Z");
});

test("FE-01 V8: a day of 25 hours and a day of 23 hours end at the last millisecond of the local day", () => {
  const at = (iso, text, edge) => new Date(parseTimeText(text, { now: Date.parse(iso), zone: "Europe/Paris", edge }).ms).toISOString();
  assert.equal(at("2026-10-25T12:00:00.000Z", "now/d", "from"), "2026-10-24T22:00:00.000Z");
  assert.equal(at("2026-10-25T12:00:00.000Z", "now/d", "to"), "2026-10-25T22:59:59.999Z");
  assert.equal(at("2026-03-29T12:00:00.000Z", "now/d", "from"), "2026-03-28T23:00:00.000Z");
  assert.equal(at("2026-03-29T12:00:00.000Z", "now/d", "to"), "2026-03-29T21:59:59.999Z");
});

test("FE-01 V8: `24h` is 24 hours and `1d` is a calendar day, on a day of a clock change", () => {
  const at = (text) => new Date(parseTimeText(text, { now: Date.parse("2026-03-29T10:00:00.000Z"), zone: "Europe/Paris" }).ms).toISOString();
  assert.equal(at("now-24h"), "2026-03-28T10:00:00.000Z");
  assert.equal(at("now-1d"), "2026-03-28T11:00:00.000Z");
});

test("FE-01 a count of years or days that the Date type cannot hold is an error, not an exception", () => {
  for (const text of ["now-999999999y", "now-999999999M", "now-999999999d", "now-999999999w", "now-999999999y/d"]) {
    for (const zone of ["utc", "Europe/Paris"]) {
      const got = parseTimeText(text, { now: Date.parse("2026-11-09T14:30:00.000Z"), zone });
      assert.equal(got.error, "The time is before 1970-01-01.", `${zone} ${text}`);
    }
  }
});

test("FE-01 a round to a unit gives a whole unit, whatever the milliseconds of now", () => {
  const now = Date.parse("2026-11-09T14:30:12.956Z");
  const at = (text, edge = "from", zone = "utc") => new Date(parseTimeText(text, { now, zone, edge }).ms).toISOString();
  assert.equal(at("now/m"), "2026-11-09T14:30:00.000Z");
  assert.equal(at("now/h"), "2026-11-09T14:00:00.000Z");
  assert.equal(at("now/d"), "2026-11-09T00:00:00.000Z");
  assert.equal(at("now/d", "to"), "2026-11-09T23:59:59.999Z");
  assert.equal(at("now/w"), "2026-11-09T00:00:00.000Z");
  assert.equal(at("now/M"), "2026-11-01T00:00:00.000Z");
  assert.equal(at("now/y", "from", "Europe/Paris"), "2025-12-31T23:00:00.000Z");
  assert.equal(at("now-1d/d"), "2026-11-08T00:00:00.000Z");
});

// D02 (9.5): the time forms of Grafana 12.4.2 in the URL, as the measure of 2026-11-22 gave them (39 URLs).
test("FE-01 D02 the three URL time forms of Grafana read as UTC, with the milliseconds of the ISO form", () => {
  const iso = (text) => new Date(Number(urlTimeText(text))).toISOString();
  assert.equal(iso("2026-11-09T08:00:00.000Z"), "2026-11-09T08:00:00.000Z");
  assert.equal(iso("2026-11-09T08:00:00.123Z"), "2026-11-09T08:00:00.123Z");
  assert.equal(iso("2026-11-09T08:00:00"), "2026-11-09T08:00:00.000Z");
  assert.equal(iso("20261109T080000"), "2026-11-09T08:00:00.000Z");
  assert.equal(iso("2028-02-29T23:59:59"), "2028-02-29T23:59:59.000Z");
  // The bounds of 4.4: the first and the last millisecond of the server, then the error of 4.4 after it.
  assert.equal(urlTimeText("1970-01-01T00:00:00.000Z"), "0");
  assert.equal(urlTimeText("2106-02-07T06:28:15.999Z"), String(MAX_MS));
  assert.equal(parseTimeText(urlTimeText("2106-02-07T06:28:16.000Z")).error, "The time is after 2106-02-07.");
  assert.equal(parseTimeText(urlTimeText("9999-12-31T23:59:59.999Z")).error, "The time is after 2106-02-07.");
  // The result is a time text of 4.4: the range resolves in any zone to the same instants.
  for (const zone of ["utc", "Europe/Paris", "America/New_York"]) {
    assert.deepEqual(resolveRange(urlTimeText("2026-11-09T08:00:00.000Z"), urlTimeText("20261109T140000"), 0, zone), { from_ms: 1794211200000, to_ms: 1794232800000 }, zone);
  }
  // Date math and epoch milliseconds stay as they are. A text of 8 digits is epoch milliseconds (4.4), not the date YYYYMMDD of Grafana.
  for (const text of ["now-6h", "now", "now/d", "1794211200000", "0", "", "20261109"]) assert.equal(urlTimeText(text), text);
  assert.equal(parseTimeText(urlTimeText("20261109")).ms, 20261109);
  for (const value of [null, undefined, 5, {}]) assert.equal(urlTimeText(value), value);
});

test("FE-01 D02 the forms that Grafana reads as 1970 or never writes stay as they are, and the grammar of 4.4 refuses them", () => {
  const refused = [
    // Grafana 12.4.2 gives 1970-01-01T00:00:02.026Z for these texts.
    "2026-11-09T08:00:00Z", "2026-11-09T08:00:00.000", "2026-11-09", "2026-11-09T08:00:00+01:00", "2026-11-09T08:00:00.000+01:00", "2026-11-09T08:00",
    "2026-11-09T08:00:00.5Z", "2026-11-09T08:00:00.000000Z", " 2026-11-09T08:00:00.000Z", "2026-11-09T08:00:00.000Zx", "+2026-11-09T08:00:00Z", "20261109T080000Z",
    // Grafana reads these forms, but never writes them.
    "2026-11-09T08:00:00+0100", "2026-11-09 08:00:00", "20261109t080000", "2026-11-09T24:00:00.000Z",
    // Not a clock time, or before 1970.
    "2026-11-09t08:00:00.000z", "2026-02-30T08:00:00.000Z", "2026-02-29T00:00:00", "20260230T080000", "2026-13-01T00:00:00", "2026-00-10T00:00:00", "2026-11-00T00:00:00",
    "2026-11-09T08:60:00", "2026-11-09T08:00:60.000Z", "1969-12-31T23:59:59.999Z", "0000-01-01T00:00:00", "0099-01-01T00:00:00",
    // Digits that are not ASCII, and signs in the place of digits.
    "２０２６-11-09T08:00:00", "2026-11-09T08:00:0١", "2026-11-09T-8:00:00", "2026-1-109T08:00:00",
  ];
  for (const text of refused) {
    assert.equal(urlTimeText(text), text, text);
    const error = parseTimeText(text).error;
    assert.ok(error && error.includes("is not valid"), `${text}: ${error}`);
  }
});

test("FE-01 D02 a very long or hostile URL time is refused in linear time, and the error text cuts it to 64 characters", () => {
  const texts = ["2026-11-09T08:00:00.000Z".repeat(40000), "9".repeat(1e6), `now-${"1".repeat(1e6)}h`, "-".repeat(1e6), "0".repeat(1e6), "\u0000".repeat(1e5), `${"2026".repeat(5)}T${":".repeat(1e5)}`];
  for (const text of texts) {
    assert.equal(urlTimeText(text), text);
    const error = parseTimeText(text).error;
    assert.ok(error && error.length < 100 && error.endsWith("…\" is not valid."), `${text.length}: ${error && error.slice(0, 120)}`);
  }
  assert.equal(parseTimeText("<img src=x onerror=alert(1)>").error, "The time text \"<img src=x onerror=alert(1)>\" is not valid.");
  assert.equal(cut("a".repeat(64)), "a".repeat(64));
  assert.equal(cut("a".repeat(65)), `${"a".repeat(64)}…`);
  // The time of one pass over all the texts: the minimum of 7 runs after 2 warm-ups.
  const run = () => {
    const t0 = performance.now();
    for (const text of texts) parseTimeText(urlTimeText(text));
    return performance.now() - t0;
  };
  run();
  run();
  const best = Math.min(...Array.from({ length: 7 }, run));
  assert.ok(best < 50, `${best.toFixed(2)} ms for ${texts.length} texts of up to 1,000,000 characters`);
});
