// Date math, quick ranges, time zones (9.9, FE-01). Pure module: no DOM access.
// All zone arithmetic uses Intl.DateTimeFormat with an IANA time zone name, so a test can pass any zone.

export const MAX_MS = 4294967295999; // 2106-02-07T06:28:15.999Z, the last millisecond of DateTime
const MAX_ABSOLUTE = 253402300799999;
export const REFRESH_CHOICES = ["10s", "30s", "1m", "5m", "15m", "30m", "1h", "2h", "1d"];

/** The IANA name of a zone: "utc" gives "UTC", "browser" gives the zone of the browser, else the name itself. */
export function zoneName(zone = "browser") {
  if (zone === "utc") return "UTC";
  if (zone === "browser" || !zone) {
    try {
      return new Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
    } catch {
      return "UTC";
    }
  }
  return zone;
}

/** The label that follows a range: "UTC" for `utc`, "Browser time" for `browser`, else the name of the zone (9.9). */
export function zoneLabel(zone) {
  if (zone === "utc") return "UTC";
  if (zone === "browser" || !zone) return "Browser time";
  return String(zone);
}

/**
 * True for `browser`, `utc` and each name that `Intl.DateTimeFormat` accepts (9.9, decision D15). Any other value is false.
 * A name with a blank or a control character never counts, whatever the browser says.
 */
export function isKnownZone(name) {
  if (name === "browser" || name === "utc") return true;
  if (typeof name !== "string" || name === "" || name.length > 64 || /[\s\u0000-\u001f\u007f]/.test(name)) return false;
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: name });
    return true;
  } catch {
    return false;
  }
}

/** The zone that the page uses for a zone setting: the setting when the browser knows it, else `browser` (9.9). */
export function effectiveZone(name) {
  return isKnownZone(name) ? name : "browser";
}

const formatters = new Map();
function formatter(tz) {
  let f = formatters.get(tz);
  if (!f) {
    f = new Intl.DateTimeFormat("en-US", {
      timeZone: tz, hourCycle: "h23", year: "numeric", month: "numeric", day: "numeric",
      hour: "numeric", minute: "numeric", second: "numeric",
    });
    formatters.set(tz, f);
  }
  return f;
}

/** The wall clock of an epoch time in a zone: {year, month (1 to 12), day, hour, minute, second, ms}. */
export function wallClock(ms, zone = "browser") {
  const whole = Math.floor(ms);
  const parts = formatter(zoneName(zone)).formatToParts(new Date(whole));
  const out = { year: 0, month: 0, day: 0, hour: 0, minute: 0, second: 0, ms: whole - Math.floor(whole / 1000) * 1000 };
  for (const p of parts) {
    if (p.type in out) out[p.type] = Number(p.value);
  }
  if (out.hour === 24) out.hour = 0;
  return out;
}

function offsetAt(ms, tz) {
  const w = wallClock(ms, tz);
  return Date.UTC(w.year, w.month - 1, w.day, w.hour, w.minute, w.second) - (Math.floor(ms / 1000) * 1000);
}

function wallMatches(ms, tz, c) {
  const w = wallClock(ms, tz);
  return w.year === c.year && w.month === c.month && w.day === c.day && w.hour === c.hour && w.minute === c.minute && w.second === c.second;
}

/**
 * The epoch time whose wall clock in the zone is the given clock time (the clock-time rule).
 * A repeated clock time gives the earlier instant. A skipped clock time uses the offset before the change.
 */
export function wallToEpoch(c, zone = "browser") {
  const tz = zoneName(zone);
  const base = Date.UTC(c.year, c.month - 1, c.day, c.hour || 0, c.minute || 0, c.second || 0);
  const ms = c.ms || 0;
  const before = base - offsetAt(base - 86400000, tz);
  const after = base - offsetAt(base + 86400000, tz);
  const target = { year: c.year, month: c.month, day: c.day, hour: c.hour || 0, minute: c.minute || 0, second: c.second || 0 };
  // Normalise the target through a UTC date so that overflowing fields (day 0, month 13) work.
  const n = new Date(Date.UTC(target.year, target.month - 1, target.day, target.hour, target.minute, target.second));
  const norm = { year: n.getUTCFullYear(), month: n.getUTCMonth() + 1, day: n.getUTCDate(), hour: n.getUTCHours(), minute: n.getUTCMinutes(), second: n.getUTCSeconds() };
  const okBefore = wallMatches(before, tz, norm);
  const okAfter = wallMatches(after, tz, norm);
  let r;
  if (okBefore && okAfter) r = Math.min(before, after);
  else if (okBefore) r = before;
  else if (okAfter) r = after;
  else r = before;
  return r + ms;
}

function daysInMonth(y, m) {
  return new Date(Date.UTC(y, m, 0)).getUTCDate();
}

const UNIT_MS = { s: 1000, m: 60000, h: 3600000 };

/**
 * The time `t` moved back by `count` of `unit`. The units `s`, `m` and `h` are fixed spans. The units `d`, `w`, `M` and `y` are calendar
 * units in the zone (9.9): a day of a clock change keeps the wall clock.
 */
export function subtract(t, count, unit, zone = "browser") {
  if (unit in UNIT_MS) return t - count * UNIT_MS[unit];
  const w = wallClock(t, zone);
  if (unit === "d" || unit === "w") {
    const days = unit === "d" ? count : count * 7;
    return wallToEpoch({ ...w, day: w.day - days }, zone);
  }
  const months = unit === "M" ? count : count * 12;
  const total = w.year * 12 + (w.month - 1) - months;
  const year = Math.floor(total / 12);
  const month = total - year * 12 + 1;
  const day = Math.min(w.day, daysInMonth(year, month));
  return wallToEpoch({ ...w, year, month, day }, zone);
}

function weekdayMonday(w) {
  return (new Date(Date.UTC(w.year, w.month - 1, w.day)).getUTCDay() + 6) % 7;
}

function startOf(t, unit, zone) {
  const w = wallClock(t, zone);
  switch (unit) {
    case "s": return t - (((t % 1000) + 1000) % 1000);
    case "m": return wallToEpoch({ ...w, second: 0, ms: 0 }, zone);
    case "h": return wallToEpoch({ ...w, minute: 0, second: 0, ms: 0 }, zone);
    case "d": return wallToEpoch({ ...w, hour: 0, minute: 0, second: 0, ms: 0 }, zone);
    case "w": return wallToEpoch({ ...w, day: w.day - weekdayMonday(w), hour: 0, minute: 0, second: 0, ms: 0 }, zone);
    case "M": return wallToEpoch({ ...w, day: 1, hour: 0, minute: 0, second: 0, ms: 0 }, zone);
    default: return wallToEpoch({ ...w, month: 1, day: 1, hour: 0, minute: 0, second: 0, ms: 0 }, zone);
  }
}

function nextStart(start, unit, zone) {
  if (unit in UNIT_MS) return start + UNIT_MS[unit];
  const w = wallClock(start, zone);
  switch (unit) {
    case "d": return wallToEpoch({ ...w, day: w.day + 1 }, zone);
    case "w": return wallToEpoch({ ...w, day: w.day + 7 }, zone);
    case "M": return wallToEpoch({ ...w, month: w.month + 1 }, zone);
    default: return wallToEpoch({ ...w, year: w.year + 1 }, zone);
  }
}

const REL_RE = /^now(?:-([1-9][0-9]{0,8})([smhdwMy]))?(?:\/([smhdwMy]))?$/;
const ABS_RE = /^(?:0|[1-9][0-9]{0,14})$/;

/** True when the time text is a relative time (`now...`). */
export function isRelative(text) {
  return typeof text === "string" && text.startsWith("now");
}

function bound(ms) {
  if (ms > MAX_MS) return { error: "The time is after 2106-02-07." };
  if (ms < 0) return { error: "The time is before 1970-01-01." };
  return { ms };
}

// True when a clock time does not exist: a month, a day, an hour, a minute or a second out of its range.
const badClock = (y, mo, d, h, mi, s) => mo < 1 || mo > 12 || d < 1 || d > daysInMonth(y, mo) || h > 23 || mi > 59 || s > 59;

// The time forms of Grafana in the URL, in UTC (9.5), by the length of the text: the length selects one fixed pattern.
const URL_FORMS = new Map([
  [24, /^(\d{4})-(\d\d)-(\d\d)T(\d\d):(\d\d):(\d\d)\.(\d{3})Z$/],
  [19, /^(\d{4})-(\d\d)-(\d\d)T(\d\d):(\d\d):(\d\d)()$/],
  [15, /^(\d{4})(\d\d)(\d\d)T(\d\d)(\d\d)(\d\d)()$/],
]);

/**
 * The time text of a URL value `from` or `to` (9.5): a time form of Grafana gives its epoch milliseconds as text.
 * Any other text, and a time before 1970, stays as it is: the grammar of 4.4 then applies to it.
 */
export function urlTimeText(text) {
  const re = typeof text === "string" ? URL_FORMS.get(text.length) : null;
  const m = re ? re.exec(text) : null;
  if (!m) return text;
  const [y, mo, d, h, mi, s, ms] = m.slice(1).map(Number);
  // Date.UTC reads a year below 100 as 1900 + year: the check of 1970 comes first.
  return y < 1970 || badClock(y, mo, d, h, mi, s) ? text : String(Date.UTC(y, mo - 1, d, h, mi, s) + ms);
}

/**
 * Resolve a time text of 4.4 to epoch milliseconds.
 * Options: now (epoch ms), zone ("utc", "browser" or an IANA name), edge ("from" or "to").
 * Returns {ms} or {error}.
 */
export function parseTimeText(text, { now = Date.now(), zone = "browser", edge = "from" } = {}) {
  if (typeof text !== "string") return { error: "The time text is not valid." };
  const s = text.trim();
  const m = REL_RE.exec(s);
  if (m) {
    try {
      let t = Math.floor(now);
      if (m[1]) t = subtract(t, Number(m[1]), m[2], zone);
      if (m[3]) {
        const start = startOf(t, m[3], zone);
        t = edge === "to" ? nextStart(start, m[3], zone) - 1 : start;
      }
      return bound(t);
    } catch (error) {
      // A count of years that the Date type cannot hold (`now-999999999y`): the time is far before 1970.
      if (error instanceof RangeError) return { error: "The time is before 1970-01-01." };
      throw error;
    }
  }
  if (ABS_RE.test(s)) {
    const v = Number(s);
    if (v > MAX_ABSOLUTE) return { error: "The time is after 2106-02-07." };
    return bound(v);
  }
  return { error: `The time text "${cut(s)}" is not valid.` };
}

/** A text of more than 64 characters cut to 64 characters and `…`, for an error text (9.5). */
export const cut = (s) => (s.length > 64 ? `${s.slice(0, 64)}…` : s);

/** Resolve both ends of a range. Returns {from_ms, to_ms} or {error}. */
export function resolveRange(fromText, toText, now, zone = "browser") {
  const a = parseTimeText(fromText, { now, zone, edge: "from" });
  if (a.error) return { error: a.error };
  const b = parseTimeText(toText, { now, zone, edge: "to" });
  if (b.error) return { error: b.error };
  if (!(a.ms < b.ms)) return { error: "The start of the range must be before its end." };
  return { from_ms: a.ms, to_ms: b.ms };
}

const Q = (label, from, to) => ({ label, from, to });
/** The quick ranges of the picker (9.9). */
export const QUICK_RANGES = Object.freeze([
  Q("Last 5 minutes", "now-5m", "now"), Q("Last 15 minutes", "now-15m", "now"), Q("Last 30 minutes", "now-30m", "now"),
  Q("Last 1 hour", "now-1h", "now"), Q("Last 3 hours", "now-3h", "now"), Q("Last 6 hours", "now-6h", "now"),
  Q("Last 12 hours", "now-12h", "now"), Q("Last 24 hours", "now-24h", "now"),
  Q("Last 2 days", "now-2d", "now"), Q("Last 7 days", "now-7d", "now"), Q("Last 30 days", "now-30d", "now"),
  Q("Last 90 days", "now-90d", "now"), Q("Last 6 months", "now-6M", "now"), Q("Last 1 year", "now-1y", "now"),
  Q("Today", "now/d", "now/d"), Q("Today so far", "now/d", "now"), Q("Yesterday", "now-1d/d", "now-1d/d"),
  Q("This week", "now/w", "now/w"), Q("This week so far", "now/w", "now"),
  Q("This month", "now/M", "now/M"), Q("This month so far", "now/M", "now"),
  Q("This year", "now/y", "now/y"), Q("This year so far", "now/y", "now"),
]);

/** The label of the quick range with the same from and to, or null. */
export function quickLabel(from, to) {
  const q = QUICK_RANGES.find((r) => r.from === from && r.to === to);
  return q ? q.label : null;
}

function shown(text, zone) {
  if (ABS_RE.test(text)) {
    const w = wallClock(Number(text), zone);
    const p2 = (n) => String(n).padStart(2, "0");
    return `${String(w.year).padStart(4, "0")}-${p2(w.month)}-${p2(w.day)} ${p2(w.hour)}:${p2(w.minute)}:${p2(w.second)}`;
  }
  return text;
}

/** The text of the picker: the quick range label, else `<from> to <to>`. An absolute time shows in the zone. */
export function formatRangeText(from, to, zone = "browser") {
  const q = quickLabel(from, to);
  if (q) return q;
  return `${shown(from, zone)} to ${shown(to, zone)}`;
}

const INPUT_RE = /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})$/;

/**
 * Parse a field of the absolute form: `YYYY-MM-DD HH:mm:ss` in the zone, or date math.
 * `opts` has now and edge for the date math.
 */
export function parseAbsoluteInput(text, zone = "browser", opts = {}) {
  const s = String(text).trim();
  const m = INPUT_RE.exec(s);
  if (m) {
    const [year, month, day, hour, minute, second] = m.slice(1).map(Number);
    if (badClock(year, month, day, hour, minute, second)) return { error: `The date "${s}" is not valid.` };
    return bound(wallToEpoch({ year, month, day, hour, minute, second }, zone));
  }
  return parseTimeText(s, { now: opts.now ?? Date.now(), zone, edge: opts.edge ?? "from" });
}

/** Zoom out: `[from - d / 2, to + d / 2]` with `d = to - from`, in whole milliseconds, kept in the range of the server. */
export function zoomOut(fromMs, toMs) {
  const d = toMs - fromMs;
  return {
    from_ms: Math.max(0, Math.floor(fromMs - d / 2)),
    to_ms: Math.min(MAX_MS, Math.ceil(toMs + d / 2)),
  };
}

/**
 * Shift the range back (`dir` -1) or forward (`dir` +1) by half of its span, as Grafana does.
 * The span stays the same. A shift past 0 or past MAX_MS stops at that limit and keeps the span.
 */
export function shiftRange(fromMs, toMs, dir) {
  const d = toMs - fromMs;
  const step = Math.floor(d / 2) * (dir < 0 ? -1 : 1);
  let from = fromMs + step;
  let to = toMs + step;
  if (from < 0) {
    from = 0;
    to = d;
  }
  if (to > MAX_MS) {
    to = MAX_MS;
    from = MAX_MS - d;
  }
  return { from_ms: from, to_ms: to };
}

/** The offset of a zone at one time, as the popup footer shows it: "UTC+01:00", "UTC-05:00" or "UTC+00:00". */
export function zoneOffsetLabel(ms, zone = "browser") {
  const minutes = Math.round(offsetAt(ms, zoneName(zone)) / 60000);
  const sign = minutes < 0 ? "-" : "+";
  const abs = Math.abs(minutes);
  const p2 = (n) => String(n).padStart(2, "0");
  return `UTC${sign}${p2(Math.floor(abs / 60))}:${p2(abs % 60)}`;
}

const REFRESH_MS = { "10s": 10000, "30s": 30000, "1m": 60000, "5m": 300000, "15m": 900000, "30m": 1800000, "1h": 3600000, "2h": 7200000, "1d": 86400000 };

/** The interval of a refresh value in milliseconds, or null for "off" and any value that is not offered. */
export function refreshToMs(text) {
  return Object.hasOwn(REFRESH_MS, text) ? REFRESH_MS[text] : null;
}
