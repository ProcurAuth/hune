// The cache of the options of the query variables (9.10, rule "fewer requests"). Pure module: no DOM access, no timer.
// It lives in the runtime of one dashboard, so it never crosses a dashboard, and a new etag makes a new runtime. It holds answers only:
// a failed request is never kept (a Retry sends a new request).
//
// The key names what the answer depends on:
//   - the variable,
//   - the values of the variables that its SQL names (its parents), because they are in the request,
//   - the period, for a variable with `refresh` = "time_change". A variable with `refresh` = "dashboard_load" does not follow the period.
// A relative period (`now-24h`) moves at each refresh, so its key is the text of the period and the entry lives a short time. An absolute
// period (two dates) has the numbers of its ends and the entry lives longer.

/** How long an answer for a relative period stays valid, in ms. A refresh within this time sends no request. */
export const RELATIVE_TTL_MS = 60_000;
/** How long an answer for an absolute period stays valid, in ms. */
export const ABSOLUTE_TTL_MS = 300_000;
/** The most entries of one cache. The oldest use goes first. */
export const CACHE_CAP = 64;

const isRelative = (text) => /\bnow\b/.test(String(text));

/** The part of the key for the period, or "" for a variable that does not follow the period. `range` has fromText, toText, from_ms and to_ms. */
export function periodKey(variable, range, zone = "") {
  if (variable.refresh !== "time_change") return "";
  if (isRelative(range.fromText) || isRelative(range.toText)) return `rel:${range.fromText}|${range.toText}|${zone}`;
  return `abs:${range.from_ms}|${range.to_ms}`;
}

/** The time to live of an entry for this period key. */
export const ttlOf = (period) => (period.startsWith("rel:") ? RELATIVE_TTL_MS : ABSOLUTE_TTL_MS);

/**
 * The key of one answer. `parents` is the list of [name, requested value] of the variables that the SQL names, in a fixed order.
 * The text of the key is built with JSON, so no value can look like another part of the key.
 */
export function cacheKey(variable, parents, period) {
  return JSON.stringify([variable.name, parents, period]);
}

export class OptionsCache {
  /** @param {{ now?: () => number, cap?: number }} [o] */
  constructor({ now = Date.now, cap = CACHE_CAP } = {}) {
    this.now = now;
    this.cap = cap;
    this.entries = new Map(); // key -> { options, at, ttl }; the Map order is the order of use, the oldest first
  }

  get size() {
    return this.entries.size;
  }

  /** The options for the key, or undefined when there is none or when it is too old. A hit moves the entry to the end. */
  get(key) {
    const entry = this.entries.get(key);
    if (!entry) return undefined;
    if (this.now() - entry.at >= entry.ttl) {
      this.entries.delete(key);
      return undefined;
    }
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.options;
  }

  set(key, options, ttl) {
    this.entries.delete(key);
    this.entries.set(key, { options, at: this.now(), ttl });
    while (this.entries.size > this.cap) this.entries.delete(this.entries.keys().next().value);
  }

  /** Drops every entry of a variable (a Retry of its control, or a change of the document). */
  forget(name) {
    for (const key of [...this.entries.keys()]) if (JSON.parse(key)[0] === name) this.entries.delete(key);
  }

  clear() {
    this.entries.clear();
  }
}
