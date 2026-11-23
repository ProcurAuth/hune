// SPDX-License-Identifier: MIT OR Apache-2.0
// The time picker of 9.9: the pure parts of time_calendar.js, and checks of the files
// of web/ that hold the rules of the popup. The browser cases B07, B69 and B92 check the behaviour in a real page.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { addDays, addMonths, dayText, keyOf, monthGrid, parseRecent, pushRecent, typedRange, weekday } from "../../web/js/time_calendar.js";
import { QUICK_RANGES } from "../../web/js/time.js";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../..");
const read = (path) => readFileSync(join(ROOT, path), "utf8");

test("a day key has the form year, month, day and its text has the zero padding", () => {
  assert.equal(keyOf(2026, 11, 9), 20261109);
  assert.equal(dayText(20261109), "2026-11-09");
  assert.equal(dayText(keyOf(2026, 1, 5)), "2026-01-05");
});

test("addDays crosses a month and a year, and goes back", () => {
  assert.equal(addDays(20261130, 1), 20261201);
  assert.equal(addDays(20261231, 1), 20270101);
  assert.equal(addDays(20260301, -1), 20260228);
  assert.equal(addDays(20240301, -1), 20240229);
  assert.equal(addDays(20261109, -7), 20261102);
});

test("addMonths keeps the day, or takes the last day of a shorter month", () => {
  assert.equal(addMonths(20261109, 1), 20261209);
  assert.equal(addMonths(20261215, 1), 20270115);
  assert.equal(addMonths(20260131, 1), 20260228);
  assert.equal(addMonths(20240131, 1), 20240229);
  assert.equal(addMonths(20260115, -2), 20251115);
  assert.equal(addMonths(20261109, 12), 20271109);
  assert.equal(addMonths(20240229, -12), 20230228);
});

test("the week starts on Monday", () => {
  assert.equal(weekday(20261109), 0); // Monday 2026-11-09
  assert.equal(weekday(20261115), 6); // Sunday
  assert.equal(weekday(20261101), 6); // Sunday 2026-11-01
});

test("monthGrid has 42 days from a Monday and marks the days of the other months", () => {
  const grid = monthGrid(2026, 11);
  assert.equal(grid.length, 42);
  // 2026-11-01 is a Sunday: the view starts on Monday 2026-10-26.
  assert.deepEqual(grid[0], { key: 20261026, y: 2026, m: 10, d: 26, out: true });
  assert.equal(grid[6].key, 20261101);
  assert.equal(grid[6].out, false);
  assert.equal(grid.filter((d) => !d.out).length, 30);
  assert.equal(grid[41].key, 20261206);
  for (let i = 0; i < 42; i += 7) assert.equal(weekday(grid[i].key), 0);
  // A month that starts on a Monday has no day of the month before.
  const june = monthGrid(2026, 6);
  assert.equal(june[0].key, 20260601);
  assert.equal(june[0].out, false);
  // February of a leap year.
  assert.equal(monthGrid(2024, 2).filter((d) => !d.out).length, 29);
  assert.equal(monthGrid(2026, 12).find((d) => d.key === 20270101).out, true);
});

test("typedRange reads a duration of the grammar of 4.4", () => {
  assert.deepEqual(typedRange("45m"), { label: "Last 45 minutes", from: "now-45m", to: "now" });
  assert.deepEqual(typedRange(" last 2d "), { label: "Last 2 days", from: "now-2d", to: "now" });
  assert.deepEqual(typedRange("now-3h"), { label: "Last 3 hours", from: "now-3h", to: "now" });
  assert.deepEqual(typedRange("1M"), { label: "Last 1 month", from: "now-1M", to: "now" });
  assert.deepEqual(typedRange("last 12 w"), { label: "Last 12 weeks", from: "now-12w", to: "now" });
  assert.equal(typedRange("1h").label, QUICK_RANGES.find((r) => r.from === "now-1h" && r.to === "now").label);
  for (const bad of ["", "m", "0m", "45", "45x", "now", "now/d", "-5m", "1.5h", "last", "1000000000m"]) assert.equal(typedRange(bad), null, bad);
});

test("pushRecent puts a range first, removes an equal one and keeps two ranges", () => {
  let list = [];
  list = pushRecent(list, { from: "1", to: "2" });
  list = pushRecent(list, { from: "3", to: "4", extra: "x" });
  assert.deepEqual(list, [{ from: "3", to: "4" }, { from: "1", to: "2" }]);
  list = pushRecent(list, { from: "1", to: "2" });
  assert.deepEqual(list, [{ from: "1", to: "2" }, { from: "3", to: "4" }]);
  list = pushRecent(list, { from: "5", to: "6" });
  assert.deepEqual(list, [{ from: "5", to: "6" }, { from: "1", to: "2" }]);
});

test("parseRecent keeps the valid ranges, at most two, and survives a bad text", () => {
  assert.deepEqual(parseRecent(JSON.stringify([{ from: "a", to: "b" }, { from: "c", to: "d" }, { from: "e", to: "f" }])), [{ from: "a", to: "b" }, { from: "c", to: "d" }]);
  assert.deepEqual(parseRecent(JSON.stringify([{ from: 1, to: "b" }, null, { from: "c", to: "d" }])), [{ from: "c", to: "d" }]);
  for (const bad of [null, "", "{", "{}", "5", "null"]) assert.deepEqual(parseRecent(bad), [], String(bad));
});

test("the popup rules are in picker.css, which the shell links after components.css", () => {
  const css = read("web/css/picker.css");
  assert.ok(!/\.picker__popup/.test(read("web/css/components.css")), "no rule of the popup stays in components.css");
  const shell = read("web/shell.html");
  assert.ok(shell.indexOf("css/components.css") < shell.indexOf("css/picker.css"));
  // The popup of ClickHouse Dash: 604 px, a 216 px column, the edge of a popover, a close and open motion of one duration token.
  assert.match(css, /\.picker__popup \{[^}]*width: min\(604px,/);
  assert.match(css, /grid-template-columns: minmax\(0, 1fr\) 216px/);
  assert.match(css, /border: 1px solid var\(--border-strong\)/);
  assert.match(css, /transform: translateY\(-6px\) scaleY\(0\.98\)/);
  assert.match(css, /transition: opacity var\(--dur-base\) var\(--ease\), transform var\(--dur-base\) var\(--ease\), visibility 0s linear var\(--dur-base\)/);
  assert.match(css, /@media \(prefers-reduced-motion: reduce\) \{ \.picker__popup \{ transition: none;/);
  assert.match(css, /\.picker__quick-item \{[^}]*min-height: 27px/);
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/.test(css.replace(/\/\*[\s\S]*?\*\//g, "")), "no literal colour");
});

test("the picker code is one module, and the page has no second picker", () => {
  const js = read("web/js/time_picker.js");
  assert.ok(js.includes("matches ? 0 : 160);"), "the popup leaves after 160 ms, at once with reduced motion");
  assert.match(js, /export function createTimePicker\(/);
  assert.ok(!/export function createTimePicker/.test(read("web/js/pickers.js")));
  assert.match(read("web/js/pickers.js"), /from "\.\/time_picker\.js"/);
  // The recent ranges are in the store under a try block, and the text of a field never goes in the page as HTML.
  assert.ok(!/innerHTML|insertAdjacentHTML/.test(js));
  assert.match(js, /localStorage\.getItem\(RECENT_KEY\)/);
});
