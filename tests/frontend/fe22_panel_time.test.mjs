// FE-22: the range of a panel with a time override and its note (9.8b, F18), and the zone names of 9.9 (D15, V8).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { RANGE_ERROR, formatClock, hasTimeOverride, noteTooltip, panelRange, timeNote } from "../../web/js/panel_time.js";
import { resolveRange } from "../../web/js/time.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-22-panel-time.json", import.meta.url)));
const iso = (ms) => new Date(ms).toISOString();

test("FE-22 the range of a panel", () => {
  for (const c of v.ranges) {
    const now = Date.parse(c.now);
    const d = resolveRange(c.dash[0], c.dash[1], now, c.zone);
    assert.equal(d.error, undefined, `${c.name}: the dashboard range`);
    const got = panelRange(c.query, { from: d.from_ms, to: d.to_ms, fromText: c.dash[0], toText: c.dash[1] }, now, c.zone);
    if (c.error) {
      assert.equal(got.error, RANGE_ERROR, c.name);
      continue;
    }
    assert.equal(got.error, null, c.name);
    assert.equal(iso(got.from), c.from, `${c.name}: from`);
    assert.equal(iso(got.to), c.to, `${c.name}: to`);
  }
});

test("FE-22 a range with no texts counts as relative, and the dashboard range stays when there is no override", () => {
  const now = Date.parse("2026-11-09T14:30:00.000Z");
  const r = panelRange({ time_from: "now-1h" }, { from: 0, to: 1 }, now, "utc");
  assert.equal(iso(r.from), "2026-11-09T13:30:00.000Z");
  const same = panelRange({ source: "s" }, { from: 5, to: 9 }, now, "utc");
  assert.deepEqual(same, { from: 5, to: 9, error: null });
  assert.deepEqual(panelRange(undefined, { from: 5, to: 9 }, now, "utc"), { from: 5, to: 9, error: null });
});

test("FE-22 the range of the panel never changes the range that it reads", () => {
  const now = Date.parse("2026-11-09T14:30:00.000Z");
  const dash = { from: now - 3600000, to: now, fromText: "now-1h", toText: "now" };
  const copy = { ...dash };
  panelRange({ time_from: "now-1d", time_shift: "1h" }, dash, now, "utc");
  assert.deepEqual(dash, copy);
});

test("FE-22 the note of the title bar", () => {
  for (const c of v.notes) {
    const dash = c.dash ? { fromText: c.dash[0], toText: c.dash[1] } : null;
    assert.equal(timeNote(c.query, "utc", dash), c.note, JSON.stringify(c));
  }
});

test("FE-22 hasTimeOverride", () => {
  assert.equal(hasTimeOverride({ time_from: "now-1h" }), true);
  assert.equal(hasTimeOverride({ time_shift: "1d" }), true);
  assert.equal(hasTimeOverride({ hide_time_override: true }), false);
  assert.equal(hasTimeOverride(null), false);
  assert.equal(hasTimeOverride({ time_from: "" }), false);
});

test("FE-22 the tooltip of the note is the range in the zone of the dashboard", () => {
  const from = Date.parse("2026-11-09T08:30:05.000Z");
  const to = Date.parse("2026-11-09T14:30:00.000Z");
  assert.equal(noteTooltip(from, to, "utc"), "2026-11-09 08:30:05 to 2026-11-09 14:30:00");
  assert.equal(noteTooltip(from, to, "Asia/Tokyo"), "2026-11-09 17:30:05 to 2026-11-09 23:30:00");
  assert.equal(formatClock(0, "utc"), "1970-01-01 00:00:00");
});

test("FE-22 a count that the Date type cannot hold gives the error, never an exception", () => {
  const now = Date.parse("2026-11-09T14:30:00.000Z");
  for (const zone of ["utc", "Europe/Paris"]) {
    for (const query of [{ time_shift: "999999999y" }, { time_shift: "999999999d" }, { time_from: "now-999999999y" }, { time_from: "now-999999999M/d" }, { time_shift: "999999999h" }]) {
      const got = panelRange(query, { from: now - 3600000, to: now, fromText: "now-1h", toText: "now" }, now, zone);
      assert.equal(got.error, RANGE_ERROR, `${zone} ${JSON.stringify(query)}`);
    }
  }
});
