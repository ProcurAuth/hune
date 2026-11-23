// The checks of a colour (4.19) and of a unit (4.20) for the forms of the editor. Pure module: no DOM access.
// Only the editor uses them, so they are in the editor and not in the page (9.30).
import { HEX_RE, NAMED, RGBA_RE, RGB_RE } from "../colors.js";

// The 59 fixed units of 4.20 (the units `suffix:` and `prefix:` have their own test).
const FIXED_UNITS = new Set([
  "none", "short", "percent", "percentunit", "ms", "s", "bytes", "decbytes",
  "ns", "\u00b5s", "m", "h", "d", "dtdurationms", "dtdurations", "bits", "bps", "Bps", "binBps", "reqps", "ops", "rps", "iops",
  "locale", "dateTimeAsIso", "dateTimeFromNow", "bool_yes_no", "currencyUSD", "currencyEUR",
  "string", "bool", "bool_on_off", "kbytes", "mbytes", "gbytes", "tbytes", "deckbytes", "decmbytes", "decgbytes", "dectbytes", "decbits",
  "KBs", "MBs", "GBs", "Kbits", "Mbits", "Gbits", "rowsps", "rowspm", "recps", "cps", "wps", "opm", "reqpm", "rpm", "dthms", "dtdhms", "hertz", "sishort",
]);

/** True when `c` is a colour value of 4.19. */
export function isColorValid(c) {
  if (typeof c !== "string") return false;
  if (c === "text" || c === "transparent" || Object.hasOwn(NAMED, c)) return true;
  if (HEX_RE.test(c)) return true;
  let m = RGB_RE.exec(c);
  if (m) return [m[1], m[2], m[3]].every((x) => Number(x) <= 255);
  m = RGBA_RE.exec(c);
  if (m) return [m[1], m[2], m[3]].every((x) => Number(x) <= 255);
  return false;
}

/** True when `unit` is a unit of 4.20. */
export function isUnitValid(unit) {
  if (typeof unit !== "string") return false;
  if (FIXED_UNITS.has(unit)) return true;
  const m = /^(suffix|prefix):(.*)$/s.exec(unit);
  if (!m) return false;
  const len = [...m[2]].length;
  return len >= 1 && len <= 32;
}
