// Units and decimals (9.15, FE-06). Pure module: no DOM access.
import { wallClock } from "./time.js";

/** `Math.round`-based fixed text of 9.15, with the automatic decimals when `d` is absent. */
export function toFixed(v, d) {
  if (v === null || v === undefined) return "";
  if (Number.isNaN(v)) return "NaN";
  if (v === Infinity) return "∞";
  if (v === -Infinity) return "-∞";
  if (d === undefined || d === null) d = autoDecimals(v);
  if (v === 0) return (0).toFixed(d);
  const factor = 10 ** d;
  const text = String(Math.round(v * factor) / factor);
  if (text.includes("e")) return text;
  const pos = text.indexOf(".");
  const precision = pos === -1 ? 0 : text.length - pos - 1;
  if (precision < d) return (precision ? text : `${text}.`) + "0".repeat(d - precision);
  return text;
}

/** The automatic number of decimals of a value (9.15). */
export function autoDecimals(v) {
  if (!Number.isFinite(v)) return 0;
  if (Number.isInteger(v)) return 0;
  const abs = Math.abs(v);
  let dec = -Math.floor(Math.log(abs) / Math.LN10) + 1; // as Grafana: 99.99999999999999 gives 1 here, and Math.log10 gives 2
  const norm = abs / 10 ** -dec;
  if (norm > 2.25) dec += 1;
  return Math.max(0, dec);
}

const SHORT = ["", " K", " Mil", " Bil", " Tri", " Quadr", " Quint", " Sext", " Sept"];
const BIN = [" ", " Ki", " Mi", " Gi", " Ti", " Pi", " Ei", " Zi", " Yi"];
// The SI prefixes, for the step -5 to 8 (the step 0 has none).
const SI = [" f", " p", " n", " \u00b5", " m", " ", " k", " M", " G", " T", " P", " E", " Z", " Y"];
const COUNT_SCALE = ["", "K", "M", "B", "T"];
const CURRENCY = { currencyUSD: "$", currencyEUR: "\u20ac" };
const BOOL_WORDS = new Map([["bool", ["True", "False"]], ["bool_yes_no", ["Yes", "No"]], ["bool_on_off", ["On", "Off"]]]);

// The scaled units of Grafana 12.4.2 (its `scaledUnits`, 9.15). A row gives the names of the steps, the base, the step of the value 1
// of the first id, the unit and the ids. The id at the position n has the step `first + n` for the value 1: `kbytes` shows 1 as `1 KiB`.
const SCALED = new Map();
for (const [names, base, first, unit, ids] of [
  [SHORT, 1000, 0, "", "short"],
  [BIN, 1024, 0, "B", "bytes kbytes mbytes gbytes tbytes"],
  [BIN, 1024, 0, "b", "bits"],
  [BIN, 1024, 0, "B/s", "binBps"],
  [SI, 1000, 5, "B", "decbytes deckbytes decmbytes decgbytes dectbytes"],
  [SI, 1000, 5, "b", "decbits"],
  [SI, 1000, 5, "b/s", "bps Kbits Mbits Gbits"],
  [SI, 1000, 5, "B/s", "Bps KBs MBs GBs"],
  [SI, 1000, 5, "Hz", "hertz"],
  [SI, 1000, 5, "", "sishort"],
]) ids.split(" ").forEach((id, n) => SCALED.set(id, [base, names, first + n, unit]));
// The counts of Grafana (`simpleCountUnit`): the steps ``, `K`, `M`, `B`, `T`, then a space and the word.
const COUNTS = {
  reqps: "req/s", ops: "ops/s", rps: "rd/s", iops: "io/s", rowsps: "rows/s", rowspm: "rows/m", recps: "rec/s", cps: "c/s", wps: "wr/s", opm: "ops/m", reqpm: "req/m", rpm: "rd/m",
};
for (const [id, word] of Object.entries(COUNTS)) SCALED.set(id, [1000, COUNT_SCALE, 0, ` ${word}`]);

// Grafana `scaledUnits`: the step is floor(log10|v| / log10(base)), 0 for v = 0, kept between -offset and the last name.
function scaledUnits(v, base, names, offset, unit, d) {
  const i = v === 0 ? 0 : Math.floor(Math.log10(Math.abs(v)) / Math.log10(base));
  const k = Math.min(Math.max(i, -offset), names.length - offset - 1);
  return { prefix: "", text: toFixed(v / base ** k, d), suffix: names[k + offset] + unit };
}

// Grafana `toDurationInHoursMinutesSeconds` (dthms) and `toDurationInDaysHoursMinutesSeconds` (dtdhms) of `v` seconds.
function clock(v, withDays) {
  const a = Math.abs(v);
  const days = withDays ? Math.floor(a / 86400) : 0;
  const s = Math.abs(a - days * 24 * 3600); // above 1e22 s the difference can be below 0: Grafana writes its size
  const two = (n) => (n > 9 ? `${n}` : `0${n}`);
  const text = `${days > 0 ? `${days} d ` : ""}${two(Math.floor(s / 3600))}:${two(Math.floor((s % 3600) / 60))}:${two(Math.floor((s % 3600) % 60))}`;
  return { prefix: "", text, suffix: v < 0 ? " ago" : "" };
}

// A word of a time unit takes the plural form (" mins", " hours") when the scaled value is more than 1 in size, as Grafana does:
// 1 gives "1 min", 1.00004 gives "1.00 mins", 0.999 gives "0.999 year". A unit that is a symbol (" ms", " s") has no plural.
function ladder(v, rows, d) {
  const abs = Math.abs(v);
  for (const [bound, op, factor, suffix, word] of rows) {
    if (abs < bound) {
      const size = op === "mul" ? v * factor : v / factor;
      return { prefix: "", text: toFixed(size, d), suffix: word && Math.abs(size) > 1 ? `${suffix}s` : suffix };
    }
  }
  throw new Error("unreachable");
}

const MS_ROWS = [
  [1000, "div", 1, " ms"], [60000, "div", 1000, " s"], [3600000, "div", 60000, " min", true], [86400000, "div", 3600000, " hour", true],
  [31536000000, "div", 86400000, " day", true], [Infinity, "div", 31536000000, " year", true],
];
const S_ROWS = [
  [0.000001, "mul", 1e9, " ns"], [0.001, "mul", 1e6, " \u00b5s"], [1, "mul", 1000, " ms"], [60, "div", 1, " s"],
  [3600, "div", 60, " min", true], [86400, "div", 3600, " hour", true], [604800, "div", 86400, " day", true],
  [31536000, "div", 604800, " week", true], [Infinity, "div", 31556900, " year", true],
];

// `plain` rows have a symbol or a word with no plural form (the first rows of `m` and `h`).
const NS_ROWS = [
  [1000, "div", 1, " ns"], [1e6, "div", 1000, " \u00b5s"], [1e9, "div", 1e6, " ms"], [6e10, "div", 1e9, " s"],
  [3.6e12, "div", 6e10, " min", true], [8.64e13, "div", 3.6e12, " hour", true], [Infinity, "div", 8.64e13, " day", true],
];
const US_ROWS = [[1000, "div", 1, " \u00b5s"], [1e6, "div", 1000, " ms"], [Infinity, "div", 1e6, " s"]];
const M_ROWS = [
  [60, "div", 1, " min"], [1440, "div", 60, " hour", true], [10080, "div", 1440, " day", true],
  [604800, "div", 10080, " week", true], [Infinity, "div", 525948, " year", true],
];
const H_ROWS = [[24, "div", 1, " hour"], [168, "div", 24, " day", true], [8760, "div", 168, " week", true], [Infinity, "div", 8760, " year", true]];
const D_ROWS = [[7, "div", 1, " day", true], [365, "div", 7, " week", true], [Infinity, "div", 365, " year", true]];

const EXACT_RE = /^-?[0-9]+(\.[0-9]+)?$/;

/** Round an exact decimal text to d decimals, a half towards positive infinity. Returns null when the text is not exact. */
export function roundExactText(text, d) {
  if (!EXACT_RE.test(text)) return null;
  const neg = text[0] === "-";
  const body = neg ? text.slice(1) : text;
  const dot = body.indexOf(".");
  const intPart = dot === -1 ? body : body.slice(0, dot);
  const frac = dot === -1 ? "" : body.slice(dot + 1);
  let digits;
  if (frac.length <= d) {
    digits = BigInt(intPart + frac + "0".repeat(d - frac.length));
  } else {
    digits = BigInt(intPart + frac.slice(0, d));
    const rest = frac.slice(d);
    const tail = rest.slice(1);
    const aboveHalf = rest[0] > "5" || (rest[0] === "5" && /[1-9]/.test(tail));
    const exactHalf = rest[0] === "5" && !/[1-9]/.test(tail);
    // Positive: a half goes up. Negative: a half goes towards zero.
    if (aboveHalf || (exactHalf && !neg)) digits += 1n;
  }
  let s = digits.toString();
  if (d > 0) {
    s = s.padStart(d + 1, "0");
    s = `${s.slice(0, s.length - d)}.${s.slice(s.length - d)}`;
  }
  const isZero = /^0(\.0*)?$/.test(s);
  return neg && !isZero ? `-${s}` : s;
}

// The duration in words (dtdurationms, dtdurations): the parts of 9.15, `decimals + 1` of them, from the first part that is 1 or more.
const DURATION_PARTS = [
  ["year", 365 * 86400000], ["month", 30 * 86400000], ["week", 7 * 86400000], ["day", 86400000], ["hour", 3600000], ["minute", 60000],
  ["second", 1000], ["millisecond", 1],
];

function durationWords(v, perUnit, decimals) {
  if (v === 0) return { prefix: "", text: perUnit === 1 ? "0 milliseconds" : "0 seconds", suffix: "" };
  const ago = v < 0;
  let rest = Math.abs(v) * perUnit;
  let start = DURATION_PARTS.findIndex(([, size]) => rest >= size);
  if (start < 0) start = DURATION_PARTS.length - 1;
  const want = (decimals === null || decimals === undefined ? 0 : decimals) + 1;
  const out = [];
  for (let k = start; k < DURATION_PARTS.length && out.length < want; k++) {
    const [word, size] = DURATION_PARTS[k];
    const n = Math.floor(rest / size);
    rest -= n * size;
    out.push(`${n} ${word}${n === 1 ? "" : "s"}`);
  }
  return { prefix: "", text: out.join(", "), suffix: ago ? " ago" : "" };
}

/**
 * The text of the time from now, by the rules of the Moment function `fromNow` (9.15). `diffMs` is the time less now.
 * The counts are rounded as Moment rounds them.
 */
export function fromNowText(diffMs) {
  const abs = Math.abs(diffMs);
  const seconds = Math.round(abs / 1000);
  const minutes = Math.round(abs / 60000);
  const hours = Math.round(abs / 3600000);
  const days = Math.round(abs / 86400000);
  const months = Math.round(abs / 86400000 / 30.436875);
  const years = Math.round(abs / 86400000 / 365.2425);
  let words;
  if (seconds <= 44) words = "a few seconds";
  else if (minutes <= 1) words = "a minute";
  else if (minutes < 45) words = `${minutes} minutes`;
  else if (hours <= 1) words = "an hour";
  else if (hours < 22) words = `${hours} hours`;
  else if (days <= 1) words = "a day";
  else if (days < 26) words = `${days} days`;
  else if (months <= 1) words = "a month";
  else if (months < 11) words = `${months} months`;
  else if (years <= 1) words = "a year";
  else words = `${years} years`;
  return diffMs > 0 ? `in ${words}` : `${words} ago`;
}

/**
 * Format a number with a unit and decimals into {prefix, text, suffix} (9.15).
 * `exactText` is the exact text of a `number_text` value. It applies to the units with no scale.
 */
export function formatNumber(v, unit = "none", decimals = null, exactText = null, zone = "browser") {
  const d = decimals === null || decimals === undefined ? undefined : decimals;
  if (v === null || v === undefined) return { prefix: "", text: "", suffix: "" };
  const u = unit || "none";
  const noScale = u === "none" || u === "percent" || u.startsWith("suffix:") || u.startsWith("prefix:");
  let text;
  if (noScale && typeof exactText === "string" && Number.isFinite(v) && EXACT_RE.test(exactText)) {
    const dd = d === undefined ? autoDecimals(v) : d;
    text = v === 0 ? (0).toFixed(dd) : roundExactText(exactText, dd);
  } else {
    text = toFixed(v, d);
  }
  if (u === "none") return { prefix: "", text, suffix: "" };
  if (u === "percent") return { prefix: "", text, suffix: "%" };
  if (u.startsWith("suffix:")) return { prefix: "", text, suffix: u.slice(7) };
  if (u.startsWith("prefix:")) return { prefix: u.slice(7), text, suffix: "" };
  if (u === "string") return { prefix: "", text: String(v), suffix: "" };
  const words = BOOL_WORDS.get(u);
  if (words) return { prefix: "", text: Number.isNaN(v) ? "NaN" : words[v === 0 ? 1 : 0], suffix: "" };
  if (u === "dateTimeAsIso") return { prefix: "", text: formatTime(v, zone), suffix: "" };
  if (u === "dateTimeFromNow") return { prefix: "", text: Number.isFinite(v) ? fromNowText(v - Date.now()) : "", suffix: "" };
  if (!Number.isFinite(v)) return { prefix: "", text: toFixed(v, d), suffix: u === "percentunit" ? "%" : "" };
  const scale = SCALED.get(u);
  if (scale) return scaledUnits(v, ...scale, d);
  switch (u) {
    case "percentunit": return { prefix: "", text: toFixed(100 * v, d), suffix: "%" };
    case "dthms": return clock(v, false);
    case "dtdhms": return clock(v, true);
    case "ms": return ladder(v, MS_ROWS, d);
    case "s":
      if (v === 0) return { prefix: "", text: "0", suffix: " s" };
      return ladder(v, S_ROWS, d);
    case "ns": return ladder(v, NS_ROWS, d);
    case "\u00b5s": return ladder(v, US_ROWS, d);
    case "m": return ladder(v, M_ROWS, d);
    case "h": return ladder(v, H_ROWS, d);
    case "d": return ladder(v, D_ROWS, d);
    case "dtdurationms": return durationWords(v, 1, decimals);
    case "dtdurations": return durationWords(v, 1000, decimals);
    case "currencyUSD":
    case "currencyEUR": {
      return { ...scaledUnits(Math.abs(v), 1000, COUNT_SCALE, 0, "", d), prefix: `${v < 0 ? "-" : ""}${CURRENCY[u]}` };
    }
    case "locale": {
      // The language is the default of the runtime: it is the language of the browser.
      const options = d === undefined ? undefined : { maximumFractionDigits: d };
      return { prefix: "", text: v.toLocaleString(undefined, options), suffix: "" };
    }
    default:
      // An unknown unit is not valid in a document: show the plain number.
      return { prefix: "", text, suffix: "" };
  }
}

/** The number of decimals in a formatted number text (`"73.55"` gives 2, `"1.5e+21"` gives 0). */
export function textDecimals(text) {
  const m = /^-?\d+\.(\d+)$/.exec(String(text));
  return m ? m[1].length : 0;
}

/**
 * The decimals that make the value `v` read with `digits` significant digits in its unit (after the scale of the unit):
 * 31200 with unit `short` and 3 digits gives 1 (31.2 K). The result is between 0 and 6. A value of 0, a value that is not finite
 * and a value that is an integer in its unit (69, 2 K) give 0.
 */
export function sigDecimals(v, unit = "none", digits = 3) {
  if (!Number.isFinite(v) || v === 0) return 0;
  const scaled = Number.parseFloat(formatNumber(v, unit, 6).text);
  if (!Number.isFinite(scaled) || scaled === 0) return 0;
  if (Number.isInteger(scaled)) return 0; // an integer value reads "69", not "69.0" (as Grafana)
  return Math.max(0, Math.min(6, digits - 1 - Math.floor(Math.log10(Math.abs(scaled)))));
}

/**
 * The decimals that the display rules (9.15) give to the value `v` of a unit when the field sets none: the number of decimals of the
 * automatic text. A list of values with one common unit uses the largest of their decimals, so that a column keeps one precision.
 */
export function autoTextDecimals(v, unit = "none") {
  if (!Number.isFinite(v)) return 0;
  return textDecimals(formatNumber(v, unit, null).text);
}

/** Join the three parts of a formatted number into one text. */
export function joinParts(p) {
  return `${p.prefix}${p.text}${p.suffix}`;
}

/** `YYYY-MM-DD HH:mm:ss` of an epoch time in the zone ("utc", "browser" or an IANA name). */
export function formatTime(ms, zone) {
  if (!Number.isFinite(ms)) return "";
  const w = wallClock(ms, zone);
  const p2 = (n) => String(n).padStart(2, "0");
  return `${String(w.year).padStart(4, "0")}-${p2(w.month)}-${p2(w.day)} ${p2(w.hour)}:${p2(w.minute)}:${p2(w.second)}`;
}

/** `YYYY-MM-DD HH:mm:ss.SSS` of an epoch time in the zone: the text of a time field with no unit when its mean step is below 20 s (display.js timeFieldHasMs). */
export function formatTimeMs(ms, zone) {
  if (!Number.isFinite(ms)) return "";
  const frac = String(((Math.round(ms) % 1000) + 1000) % 1000).padStart(3, "0");
  return `${formatTime(Math.floor(ms / 1000) * 1000, zone)}.${frac}`;
}
