// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-06: units and decimals (9.15).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { formatNumber, joinParts, toFixed, autoDecimals, formatTime, formatTimeMs, sigDecimals, fromNowText } from "../../web/js/units.js";
import { isUnitValid } from "../../web/js/editor/validators.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-06-units.json", import.meta.url)));
const num = (x) => (x === "NaN" ? NaN : x === "Infinity" ? Infinity : x === "-Infinity" ? -Infinity : x);

test("FE-06 units", () => {
  for (const c of v.cases) {
    const got = joinParts(formatNumber(num(c.v), c.unit, c.d, c.exact ?? null));
    assert.equal(got, c.expect, `${c.unit} ${c.v} ${c.d}`);
  }
});

test("FE-06 the time text of a time field has the milliseconds", () => {
  assert.equal(formatTimeMs(Date.UTC(2026, 10, 10, 21, 9, 59, 613), "utc"), "2026-11-10 21:09:59.613");
  assert.equal(formatTimeMs(Date.UTC(2026, 10, 10, 21, 9, 59, 0), "utc"), "2026-11-10 21:09:59.000");
  assert.equal(formatTimeMs(1794341399613.0000002, "utc"), "2026-11-10 20:09:59.613");
  assert.equal(formatTimeMs(Date.UTC(1969, 11, 31, 23, 59, 59, 5), "utc"), "1969-12-31 23:59:59.005");
  assert.equal(formatTimeMs(NaN, "utc"), "");
});

test("FE-06 sigDecimals gives no decimal to an integer", () => {
  assert.equal(sigDecimals(56, "none"), 0);
  assert.equal(sigDecimals(69, "none"), 0);
  assert.equal(sigDecimals(2000, "short"), 0);
  assert.equal(sigDecimals(56.4, "none"), 1);
  assert.equal(sigDecimals(5.123, "none"), 2);
});

test("FE-06 to_fixed and auto decimals", () => {
  for (const c of v.toFixed) assert.equal(toFixed(c.v, c.d), c.expect, `${c.v}`);
  for (const c of v.autoDecimals) assert.equal(autoDecimals(c.v), c.expect, `${c.v}`);
  assert.equal(toFixed(null, 2), "");
});

test("FE-06 parts and time text", () => {
  assert.deepEqual(formatNumber(12.5, "suffix: pts"), { prefix: "", text: "12.5", suffix: " pts" });
  assert.deepEqual(formatNumber(12.5, "prefix:$"), { prefix: "$", text: "12.5", suffix: "" });
  for (const c of v.time) assert.equal(formatTime(c.ms, c.zone), c.expect);
});

test("FE-06 the 21 units of milestone F are valid units", () => {
  for (const u of ["ns", "µs", "m", "h", "d", "dtdurationms", "dtdurations", "bits", "bps", "Bps", "binBps", "reqps", "ops", "rps", "iops", "locale", "dateTimeAsIso", "dateTimeFromNow", "bool_yes_no", "currencyUSD", "currencyEUR"]) {
    assert.ok(isUnitValid(u), u);
  }
  assert.ok(!isUnitValid("us"));
});

test("FE-06 dateTimeAsIso uses the zone of the dashboard", () => {
  assert.equal(joinParts(formatNumber(1794211200000, "dateTimeAsIso", null, null, "utc")), "2026-11-09 08:00:00");
  assert.equal(joinParts(formatNumber(Number.NaN, "dateTimeAsIso", null, null, "utc")), "");
});

test("FE-06 dateTimeFromNow follows the thresholds of Moment", () => {
  const H = 3600000;
  const cases = [[-1000, "a few seconds ago"], [-44000, "a few seconds ago"], [-45000, "a minute ago"], [-89000, "a minute ago"], [-90000, "2 minutes ago"],
    [-44 * 60000, "44 minutes ago"], [-45 * 60000, "an hour ago"], [-89 * 60000, "an hour ago"], [-2 * H, "2 hours ago"], [-21 * H, "21 hours ago"],
    [-22 * H, "a day ago"], [-35 * H, "a day ago"], [-36 * H, "2 days ago"], [5 * 86400000, "in 5 days"], [-26 * 86400000, "a month ago"],
    [-45 * 86400000, "a month ago"], [-46 * 86400000, "2 months ago"], [-300 * 86400000, "10 months ago"], [-330 * 86400000, "a year ago"], [-400 * 86400000, "a year ago"], [-800 * 86400000, "2 years ago"],
    [2 * H, "in 2 hours"], [0, "a few seconds ago"]];
  for (const [diff, text] of cases) assert.equal(fromNowText(diff), text, String(diff));
  const real = Date.now;
  Date.now = () => 1000000000000;
  try {
    assert.equal(joinParts(formatNumber(1000000000000 - 2 * H, "dateTimeFromNow")), "2 hours ago");
  } finally {
    Date.now = real;
  }
});

test("FE-06 locale uses the language of the browser", () => {
  const language = typeof navigator !== "undefined" ? navigator.language : undefined;
  assert.equal(joinParts(formatNumber(1234.5, "locale", 0)), (1234.5).toLocaleString(language, { maximumFractionDigits: 0 }));
  assert.equal(joinParts(formatNumber(1234.5678, "locale")), (1234.5678).toLocaleString(language));
});

test("FE-06 the unit of a bool value", () => {
  assert.equal(joinParts(formatNumber(1, "bool_yes_no")), "Yes");
  assert.equal(joinParts(formatNumber(0, "bool_yes_no")), "No");
  assert.equal(joinParts(formatNumber(Infinity, "bool_yes_no")), "Yes");
});
