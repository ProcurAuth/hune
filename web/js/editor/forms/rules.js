// The grammar and range checks of the forms (10.3a, 10.3b). Pure module: no DOM access.
// Each check returns "" when the value is valid, or the text of the error. The rules are the rules of section 4.
// The server checks the same rules at save. A form shows the error under its control and does not write the draft.

import { checkUrl } from "../../links.js";
import { parseTimeText } from "../../time.js";

const textBytes = new TextEncoder();
export const byteLength = (text) => textBytes.encode(text).length;
export const charLength = (text) => [...text].length;

/** A text has a control character when it has a code below U+0020 or U+007F (4.11, 7.7). */
export const hasControlChar = (text) => /[\u0000-\u001f\u007f]/.test(text);

/** A text of `min` to `max` characters. */
export function checkChars(text, min, max, what = "The text") {
  const n = charLength(text);
  if (n >= min && n <= max) return "";
  return min === 0 ? `${what} has ${max} characters at most.` : `${what} needs ${min} to ${max} characters.`;
}

/** A text of `min` to `max` bytes (a value, a name of a field, a matcher). */
export function checkBytes(text, min, max, what = "The text") {
  const n = byteLength(text);
  if (n >= min && n <= max) return "";
  return min === 0 ? `${what} has ${max} bytes at most.` : `${what} needs ${min} to ${max} bytes.`;
}

// --- patterns (4.24) -------------------------------------------------------------------------------------------------------

const PATTERN_ERROR = "The pattern is not valid.";
const GROUP_NAME = /^[A-Za-z_][A-Za-z0-9_]*>/;
const BRACES = /^\{[0-9]+(?:,[0-9]*)?\}/;
const SIMPLE_ESCAPES = new Set(["d", "D", "w", "W", "b", "B", "t", "n", "r", "f", "v"]);
const isAsciiPunct = (c) => /^[!-/:-@[-`{-~]$/.test(c);

/** True when `body` is in the common subset of 4.24 (the escapes, the classes, the groups and the braces). It does not compile the text. */
export function inCommonSubset(body) {
  let inClass = false;
  for (let i = 0; i < body.length; i++) {
    const c = body[i];
    if (c === "\\") {
      const n = body[i + 1];
      if (n === undefined) return false;
      if (n === "x") {
        if (!/^[0-9A-Fa-f]{2}$/.test(body.slice(i + 2, i + 4))) return false;
        i += 3;
      } else if (SIMPLE_ESCAPES.has(n) || isAsciiPunct(n)) {
        i += 1;
      } else {
        return false;
      }
      continue;
    }
    if (inClass) {
      if (c === "[") return false;
      const two = body.slice(i, i + 2);
      if (two === "&&" || two === "--" || two === "~~") return false;
      if (c === "]") inClass = false;
      continue;
    }
    if (c === "[") {
      inClass = true;
      // A "]" right after "[" or "[^" is a member of the class in the two engines; the subset keeps it out of the way.
      continue;
    }
    if (c === "(" && body[i + 1] === "?") {
      if (body.slice(i + 2, i + 3) === ":") {
        i += 2;
        continue;
      }
      if (body[i + 2] === "<" && GROUP_NAME.test(body.slice(i + 3))) {
        i += 2 + body.slice(i + 3).indexOf(">") + 1;
        continue;
      }
      return false;
    }
    if (c === "{" && !BRACES.test(body.slice(i))) return false;
  }
  return !inClass;
}

/**
 * The error of a pattern, or "".
 * `grafana` true: the Grafana form of 4.24 (`/body/flags`, or a plain text that is an exact match). False: the plain form of a matcher.
 */
export function checkPattern(text, { grafana = true } = {}) {
  if (typeof text !== "string" || byteLength(text) < 1 || byteLength(text) > 1024) return PATTERN_ERROR;
  let body = text;
  let flags = "";
  if (grafana && text.startsWith("/")) {
    const last = text.lastIndexOf("/");
    if (last === 0) return PATTERN_ERROR;
    body = text.slice(1, last);
    flags = text.slice(last + 1);
    if (!/^[gims]{0,4}$/.test(flags) || new Set(flags).size !== flags.length) return PATTERN_ERROR;
  }
  if (!inCommonSubset(body)) return PATTERN_ERROR;
  try {
    new RegExp(body, flags);
  } catch (error) {
    return PATTERN_ERROR;
  }
  return "";
}

// --- links (4.23) ----------------------------------------------------------------------------------------------------------

export const URL_ERROR = "The URL must start with /, ?, #, http:// or https://.";

/** The error of a URL template: 1 to 2048 bytes, and the text before the first token passes the prefix rule of 4.23. */
export function checkUrlTemplate(url) {
  if (typeof url !== "string" || url === "") return "Enter a URL.";
  if (byteLength(url) > 2048) return "The URL has 2048 bytes at most.";
  const prefix = url.split("${")[0];
  if (prefix !== "" && !checkUrl(prefix)) return URL_ERROR;
  return "";
}

// --- time texts (4.4, 4.6, 4.15) --------------------------------------------------------------------------------------------

const TIME_FROM = /^now(?:-[1-9][0-9]{0,8}[smhdwMy](?:\/[smhdwMy])?|\/[smhdwMy])$/;
const TIME_SHIFT = /^[1-9][0-9]{0,8}[smhdwMy]$/;

export const TIME_FROM_ERROR = "A relative time starts with now-, as in now-6h, or with now/, as in now/d.";
export const TIME_SHIFT_ERROR = "A time shift has a count and a unit, as in 1d.";

/** `query.time_from`: empty (absent) or a text of 4.15. */
export function checkTimeFrom(text) {
  return text === "" || TIME_FROM.test(text) ? "" : TIME_FROM_ERROR;
}

/** `query.time_shift`: empty (absent) or `<count><unit>`. */
export function checkTimeShift(text) {
  return text === "" || TIME_SHIFT.test(text) ? "" : TIME_SHIFT_ERROR;
}

/** One end of `time` (4.4): a relative or an absolute time text. */
export function checkTimeText(text) {
  const r = parseTimeText(text, { now: Date.now(), zone: "utc" });
  return r.error ? "Use a time such as now-6h, now/d or a number of milliseconds." : "";
}

/** The two ends together: absolute `from` is below absolute `to` (4.6, rule 1). */
export function checkTimeOrder(from, to) {
  const abs = (t) => typeof t === "string" && !t.startsWith("now") && /^[0-9]+$/.test(t);
  if (abs(from) && abs(to) && !(Number(from) < Number(to))) return "From must be before To.";
  return "";
}

const ZONE = /^[A-Za-z][A-Za-z0-9_+-]*\/[A-Za-z][A-Za-z0-9_+-]*(?:\/[A-Za-z][A-Za-z0-9_+-]*)?$/;

/** A zone name of 4.6: the grammar and 3 to 64 bytes. `browser` and `utc` are not names. */
export function checkZoneName(name) {
  const n = byteLength(name);
  return n >= 3 && n <= 64 && ZONE.test(name) ? "" : "The time zone is not valid.";
}

// --- names (4.3, 4.5, 4.9) ---------------------------------------------------------------------------------------------------

/** A variable name: 4.3. `others` are the names of the other variables. */
export function checkVariableName(name, others = []) {
  if (!/^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(name) || name.startsWith("__")) {
    return "A name has letters, digits and _, starts with a letter or _, has 64 characters at most, and does not start with __.";
  }
  return others.includes(name) ? "This name exists." : "";
}

/** A tag: 1 to 64 characters, not in `others`. */
export function checkTag(tag, others = []) {
  const e = checkChars(tag, 1, 64, "A tag");
  if (e) return e;
  return others.includes(tag) ? "This tag exists." : "";
}

/** A number text of a form: "" gives null, a finite number gives the number, else undefined. */
export function parseNumber(text) {
  const t = text.trim();
  if (t === "") return null;
  if (!/^[-+]?(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][-+]?[0-9]+)?$/.test(t)) return undefined;
  const n = Number(t);
  return Number.isFinite(n) ? n : undefined;
}

/** A whole number text of a form from `min` to `max`: the number, or undefined. */
export function parseInteger(text, min, max) {
  const t = text.trim();
  if (!/^-?[0-9]+$/.test(t)) return undefined;
  const n = Number(t);
  return Number.isSafeInteger(n) && n >= min && n <= max ? n : undefined;
}
