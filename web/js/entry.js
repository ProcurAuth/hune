// The entry redirect of the list page (9.7): one dashboard that the user can see opens at once.
// The decision is a pure function. main.js reads the inputs and runs the navigation.

import { dashboardPath } from "./url_state.js";

// The key of sessionStorage that holds the guard.
export const GUARD_KEY = "hune.entry.redirect";

// The guard stops a second automatic redirect to the same uid in the same tab for 10 seconds.
// A loop of full page loads (for example a proxy that sends a refused dashboard back to the home page)
// takes a few seconds for each turn, so 10 seconds breaks it. A user who comes back on purpose waits at most 10 seconds.
export const GUARD_WINDOW_MS = 10000;

// The text to store in the guard after a redirect to `uid`.
export function guardValue(uid, now) {
  return JSON.stringify({ uid, at: now });
}

// Reads the stored text. It returns { uid, at } or null for a missing or bad value.
export function parseGuard(raw) {
  if (typeof raw !== "string" || raw === "") return null;
  try {
    const value = JSON.parse(raw);
    if (value && typeof value.uid === "string" && Number.isFinite(value.at)) return { uid: value.uid, at: value.at };
  } catch (error) {
    // A bad value is the same as no guard.
  }
  return null;
}

// True when the guard names `uid` and is younger than the window. A date in the future is not young.
function guardHolds(guard, uid, now) {
  if (!guard || guard.uid !== uid) return false;
  const age = now - guard.at;
  return age >= 0 && age < GUARD_WINDOW_MS;
}

// The state of an item: a missing state is "valid".
const isValid = (item) => (item && item.state !== undefined && item.state !== null ? item.state : "valid") === "valid";

/**
 * Decides the entry redirect.
 * - items: the complete list (all pages) of `GET api/dashboards`.
 * - search: the search text `q` of the URL ("" when none).
 * - isEntry: true when the list is the first route of this page load.
 * - guard: the parsed guard ({ uid, at }) or null.
 * - now: the time in milliseconds.
 * It returns the relative path `d/<uid>/<slug>`, or null when the page must show the list.
 */
export function entryTarget({ items, search = "", isEntry, guard = null, now }) {
  if (isEntry !== true) return null;
  if (typeof search === "string" && search !== "") return null;
  if (!Array.isArray(items) || items.length !== 1) return null;
  const item = items[0];
  if (!isValid(item) || typeof item.uid !== "string" || item.uid === "") return null;
  if (guardHolds(guard, item.uid, now)) return null;
  return dashboardPath(item.uid, item.title);
}
