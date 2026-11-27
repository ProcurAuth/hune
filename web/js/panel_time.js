// The time override of a panel (9.8b, F18, FE-22). Pure module: no DOM access.
// The members `time_from`, `time_shift` and `hide_time_override` of `data.query` (4.15) change the range of one panel.

import { MAX_MS, isRelative, parseTimeText, subtract, wallClock } from "./time.js";

/** The message of a panel whose override gives a range that the page cannot use (9.8b, step 4). */
export const RANGE_ERROR = "The time override of this panel gives a range that Hune cannot use.";

// The grammar of 4.15: `time-from` and `time-shift`. The count starts at 1 (4.4). The text `now` alone is not a time_from.
const FROM_RE = /^now(?:-([1-9][0-9]{0,8})([smhdwMy])(?:\/([smhdwMy]))?|\/([smhdwMy]))$/;
const SHIFT_RE = /^([1-9][0-9]{0,8})([smhdwMy])$/;

export const WORDS = { s: "second", m: "minute", h: "hour", d: "day", w: "week", M: "month", y: "year" };
const SO_FAR = { d: "Today so far", w: "This week so far", M: "This month so far", y: "This year so far" };

const hasText = (value) => typeof value === "string" && value !== "";

/** The time override of a query: `{ from, shift, hide }`, where `from` and `shift` are texts or null. A reuse panel has no query. */
function override(query) {
  if (!query || typeof query !== "object") return { from: null, shift: null, hide: false };
  return {
    from: hasText(query.time_from) ? query.time_from : null,
    shift: hasText(query.time_shift) ? query.time_shift : null,
    hide: query.hide_time_override === true,
  };
}

/** True when the query has a time override (a `time_from` or a `time_shift`). */
export function hasTimeOverride(query) {
  const o = override(query);
  return o.from !== null || o.shift !== null;
}

/**
 * True when `time_from` has an effect (9.8b, step 1, paper V6): at least one end of the dashboard range is a `now` text. The
 * dashboard range is `{ fromText, toText }`. A range with no texts counts as relative.
 */
function fromApplies(range) {
  if (!range || (range.fromText === undefined && range.toText === undefined)) return true;
  return isRelative(range.fromText) || isRelative(range.toText);
}

/**
 * The range of a panel (9.8b). `query` is the `data.query` of the panel, `range` is the dashboard range `{ from, to }` in epoch
 * milliseconds, with the optional texts `fromText` and `toText` (`time_from` does nothing when both are absolute times),
 * `now` is the time `t` of 9.9 and `zone` is the zone of the dashboard.
 *
 * It returns `{ from, to, error }`. `error` is null, or the text of RANGE_ERROR when the range is not usable (below 0, above 7.3,
 * or empty). A text that breaks the grammar of 4.15 leaves the range of the dashboard (Grafana does the same, paper V6): the note
 * of `timeNote` then says that the text is not valid.
 */
export function panelRange(query, range, now, zone = "browser") {
  let from = range.from;
  let to = range.to;
  const o = override(query);
  const fromMatch = o.from === null ? null : FROM_RE.exec(o.from);
  const shiftMatch = o.shift === null ? null : SHIFT_RE.exec(o.shift);
  const fail = { from, to, error: RANGE_ERROR };
  if (fromMatch && fromApplies(range)) {
    const start = parseTimeText(o.from, { now, zone, edge: "from" });
    const end = parseTimeText("now", { now, zone, edge: "to" });
    if (start.error || end.error) return fail;
    from = start.ms;
    to = end.ms;
    // A round `now/<unit>` at the first millisecond of a unit gives an empty range: the page uses one millisecond (9.8b, step 3).
    if (from === to) to = from + 1;
  }
  if (shiftMatch) {
    const count = Number(shiftMatch[1]);
    try {
      from = subtract(from, count, shiftMatch[2], zone);
      to = subtract(to, count, shiftMatch[2], zone);
    } catch (error) {
      // A shift that the Date type cannot hold (999999999 years) is a range that the page cannot use.
      if (error instanceof RangeError) return fail;
      throw error;
    }
  }
  const usable = Number.isFinite(from) && Number.isFinite(to) && from >= 0 && to <= MAX_MS && from < to;
  return usable ? { from, to, error: null } : { from, to, error: RANGE_ERROR };
}

/** The text of the note for a valid `time_from`: the first row of the table of 9.8b. */
function fromText(text) {
  const m = FROM_RE.exec(text);
  if (m[1]) return `Last ${m[1]} ${WORDS[m[2]]}${Number(m[1]) > 1 ? "s" : ""}`;
  return SO_FAR[m[4]] || `${text} to now`;
}

/**
 * The text of the note in the title bar (9.8b), or null when the panel has no override or `hide_time_override` is true. A
 * `time_from` that breaks the grammar gives "invalid time override", a `time_shift` that breaks it gives "invalid timeshift"
 * (paper V6). `range` is the dashboard range `{ fromText, toText }`, optional: when both ends are absolute times, `time_from`
 * does nothing and the note has no part for it. `zone` is the zone of the dashboard (reserved: the text has no time in it).
 */
export function timeNote(query, zone, range = null) {
  const o = override(query);
  if (o.hide || (o.from === null && o.shift === null)) return null;
  if (o.from !== null && !FROM_RE.test(o.from)) return "invalid time override";
  if (o.shift !== null && !SHIFT_RE.test(o.shift)) return "invalid timeshift";
  const useFrom = o.from !== null && fromApplies(range);
  if (!useFrom && o.shift === null) return null;
  if (!useFrom) return `Timeshift -${o.shift}`;
  const head = fromText(o.from);
  return o.shift === null ? head : `${head} + timeshift -${o.shift}`;
}

const p2 = (n) => String(n).padStart(2, "0");

/** The time `YYYY-MM-DD HH:mm:ss` of an epoch time in a zone. */
export function formatClock(ms, zone) {
  const w = wallClock(ms, zone);
  return `${String(w.year).padStart(4, "0")}-${p2(w.month)}-${p2(w.day)} ${p2(w.hour)}:${p2(w.minute)}:${p2(w.second)}`;
}

/**
 * The tooltip of the note (9.8b): `YYYY-MM-DD HH:mm:ss to YYYY-MM-DD HH:mm:ss` in the zone of the dashboard. Grafana adds the
 * name of the zone on a line after it: Hune does not (known difference).
 */
export function noteTooltip(from, to, zone) {
  return `${formatClock(from, zone)} to ${formatClock(to, zone)}`;
}
