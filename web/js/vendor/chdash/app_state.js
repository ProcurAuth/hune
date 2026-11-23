// Shim of ClickHouse Dash app_state.js (origin "hune", 9.24). It exports `storage` (KEYS and pref), `state` and `features`.
// Every access to localStorage or sessionStorage is in a try block. A failure gives the fallback value.
// The keys of localStorage start with `hune.chdash.`.

const PREFIX = "hune.chdash.";

const KEYS = Object.freeze({
  chartLegend: `${PREFIX}chartLegend`,
  serviceColors: `${PREFIX}serviceColors`,
  theme: `${PREFIX}theme`,
});

// pref(key, fallback, options) -> { key, get, set, remove }
// The type of the fallback gives the stored form: boolean "1"/"0", number text, string as it is, object as JSON.
// options: session (sessionStorage), allowed (valid strings), text, json, legacy (older keys), valid(value).
function pref(key, fallback, options = {}) {
  const store = () => (options.session ? window.sessionStorage : window.localStorage);
  const kind = options.json ? "json"
    : typeof fallback === "boolean" ? "bool"
      : typeof fallback === "number" ? "number"
        : fallback !== null && typeof fallback === "object" ? "json" : "string";
  const encode = (value) => {
    if (kind === "bool") return options.text ? (value ? "true" : "false") : (value ? "1" : "0");
    if (kind === "json") return JSON.stringify(value);
    return String(value);
  };
  const decode = (raw) => {
    if (raw == null) return undefined;
    if (kind === "bool") return raw === "1" || raw === "true" ? true : raw === "0" || raw === "false" ? false : undefined;
    if (kind === "number") return raw.trim() !== "" && Number.isFinite(Number(raw)) ? Number(raw) : undefined;
    if (kind === "json") {
      try { return JSON.parse(raw); } catch { return undefined; }
    }
    return Array.isArray(options.allowed) && !options.allowed.includes(raw) ? undefined : raw;
  };
  const valid = (value) => value !== undefined && (typeof options.valid !== "function" || options.valid(value));
  const read = (name) => {
    try { return store().getItem(name); } catch { return null; }
  };
  const handle = {
    key,
    get() {
      let value = decode(read(key));
      if (value === undefined && Array.isArray(options.legacy)) {
        for (const older of options.legacy) {
          const legacy = decode(read(older));
          if (legacy !== undefined) { handle.set(legacy); value = legacy; break; }
        }
      }
      return valid(value) ? value : fallback;
    },
    set(value) {
      try { store().setItem(key, encode(value)); return true; } catch { return false; }
    },
    remove() {
      try { store().removeItem(key); } catch { /* nothing to remove */ }
    },
  };
  return handle;
}

const storage = { KEYS, pref };

// The copies read `state` only for the keyword and function metadata of the SQL highlighter. Hune has none.
const state = {};

const features = Object.freeze({});

export { storage, state, features };
