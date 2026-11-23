// The text of a time in a zone (9.19, decision F10). Pure module: no DOM access.
// Every time text of the page uses `formatTime`. Code never calls `toLocaleString` or `getHours` without a zone for such a text.

import { wallClock, wallToEpoch, zoneName } from "./time.js";

const pad = (v, n = 2) => String(v).padStart(n, "0");

// The pattern tokens, longest first. Any other character stays as it is.
const TOKEN = /YYYY|SSS|MM|DD|HH|mm|ss/g;

/**
 * The IANA name of a zone: "utc", "browser" or an IANA name. A name that the browser does not know gives "browser" (9.9, decision D15).
 * The page also checks the name (`isKnownZone`) and shows a notice. This is the second check, for a caller that does not.
 */
function safeZone(zone) {
  if (zone === "utc" || zone === "browser" || !zone) return zone || "browser";
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: zone });
    return zone;
  } catch {
    return "browser";
  }
}

/**
 * The wall clock of an epoch time in a zone: {y, mo (1 to 12), d, h, mi, s, ms}.
 * The zones "utc" and "browser" use the methods of Date. An IANA name uses Intl.
 */
function parts(ms, zone) {
  const whole = Math.floor(ms);
  if (zone === "utc") {
    const d = new Date(whole);
    return { y: d.getUTCFullYear(), mo: d.getUTCMonth() + 1, d: d.getUTCDate(), h: d.getUTCHours(), mi: d.getUTCMinutes(), s: d.getUTCSeconds(), ms: d.getUTCMilliseconds() };
  }
  if (zone === "browser") {
    const d = new Date(whole);
    return { y: d.getFullYear(), mo: d.getMonth() + 1, d: d.getDate(), h: d.getHours(), mi: d.getMinutes(), s: d.getSeconds(), ms: d.getMilliseconds() };
  }
  const w = wallClock(whole, zone);
  return { y: w.year, mo: w.month, d: w.day, h: w.hour, mi: w.minute, s: w.second, ms: w.ms };
}

/** The epoch time of a wall clock in a zone. A field may overflow (day 0, month 13). */
function epoch(zone, y, mo, d, h = 0, mi = 0, s = 0) {
  if (zone === "utc") return Date.UTC(y, mo - 1, d, h, mi, s);
  if (zone === "browser") return new Date(y, mo - 1, d, h, mi, s).getTime();
  return wallToEpoch({ year: y, month: mo, day: d, hour: h, minute: mi, second: s }, zone);
}

/**
 * The clock of a zone for the chart engine (the option `zone`, 9.24): `parts(ms)` and `epoch(y, mo, d, h, mi, s)`.
 * The engine puts the ticks of the x axis at the boundaries of this clock.
 */
export function zoneClock(zone) {
  const z = safeZone(zone);
  return {
    zone: z,
    parts: (ms) => parts(ms, z),
    epoch: (y, mo, d, h, mi, s) => epoch(z, y, mo, d, h, mi, s),
  };
}

/**
 * The text of an epoch time in milliseconds in a zone. `zone` is "utc", "browser" or an IANA name.
 * `pattern` has the tokens YYYY, MM, DD, HH, mm, ss and SSS. The default is `YYYY-MM-DD HH:mm:ss`.
 * A time that is not a finite number gives an empty text.
 */
export function formatTime(ms, zone = "browser", pattern = "YYYY-MM-DD HH:mm:ss") {
  if (typeof ms !== "number" || !Number.isFinite(ms)) return "";
  const p = parts(ms, safeZone(zone));
  return pattern.replace(TOKEN, (t) => {
    switch (t) {
      case "YYYY": return pad(p.y, 4);
      case "MM": return pad(p.mo);
      case "DD": return pad(p.d);
      case "HH": return pad(p.h);
      case "mm": return pad(p.mi);
      case "ss": return pad(p.s);
      default: return pad(p.ms, 3);
    }
  });
}

// The zone name as the page shows it (re-export, so that a caller needs one import).
export { zoneName };
