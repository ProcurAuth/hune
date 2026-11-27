// The pure parts of the time picker (9.9): the days of a month, a typed duration and the recent ranges. No DOM access.
// A day is the number `year * 10000 + month * 100 + day`. It has no zone: the picker reads it in the zone of the dashboard.

import { WORDS } from "./panel_time.js";

const TYPED = /^(?:last\s*|now-)?([1-9][0-9]{0,8})\s*([smhdwMy])$/;
const split = (key) => [Math.floor(key / 10000), Math.floor(key / 100) % 100, key % 100];
const pad = (n) => String(n).padStart(2, "0");

export const keyOf = (year, month, day) => year * 10000 + month * 100 + day;

/** The text `YYYY-MM-DD` of a day. */
export const dayText = (key) => { const [y, m, d] = split(key); return `${y}-${pad(m)}-${pad(d)}`; };

/** The day `n` days after `key` (`n` may be negative). */
export function addDays(key, n) {
  const [y, m, d] = split(key);
  const t = new Date(Date.UTC(y, m - 1, d + n));
  return keyOf(t.getUTCFullYear(), t.getUTCMonth() + 1, t.getUTCDate());
}

/** The day `n` months after `key`. A day that the month does not have becomes its last day. */
export function addMonths(key, n) {
  const [y, m, d] = split(key);
  const total = y * 12 + m - 1 + n;
  const year = Math.floor(total / 12);
  const month = total - year * 12 + 1;
  return keyOf(year, month, Math.min(d, new Date(Date.UTC(year, month, 0)).getUTCDate()));
}

/** The place of a day in its week: 0 for Monday, 6 for Sunday. */
export function weekday(key) {
  const [y, m, d] = split(key);
  return (new Date(Date.UTC(y, m - 1, d)).getUTCDay() + 6) % 7;
}

/** The 42 days of the month view (6 weeks from a Monday): `{ key, y, m, d, out }`. `out` is true for a day of another month. */
export function monthGrid(year, month) {
  const first = keyOf(year, month, 1);
  return Array.from({ length: 42 }, (_, i) => {
    const key = addDays(first, i - weekday(first));
    const [y, m, d] = split(key);
    return { key, y, m, d, out: m !== month };
  });
}

/** The range that a typed text means (`45m`, `last 2d`, `now-3h`), or null. The label reads `Last 45 minutes`. */
export function typedRange(text) {
  const m = TYPED.exec(String(text).trim());
  return m && { label: `Last ${m[1]} ${WORDS[m[2]]}${m[1] === "1" ? "" : "s"}`, from: `now-${m[1]}${m[2]}`, to: "now" };
}

/** The recent ranges after a use of `range`: it comes first, an equal one goes, and two stay. */
export const pushRecent = (list, range) => [{ from: range.from, to: range.to }, ...list.filter((r) => r.from !== range.from || r.to !== range.to)].slice(0, 2);

/** The recent ranges of a stored text. A text that is not valid gives an empty list. */
export function parseRecent(raw) {
  try {
    const list = JSON.parse(raw);
    return Array.isArray(list) ? list.filter((r) => r && typeof r.from === "string" && typeof r.to === "string").slice(0, 2) : [];
  } catch {
    return [];
  }
}
