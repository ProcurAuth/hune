// SPDX-License-Identifier: MIT OR Apache-2.0
// The cache of the options of the query variables (9.10): the key, the time to live, the cap, and what is never kept.
import test from "node:test";
import assert from "node:assert/strict";
import { ABSOLUTE_TTL_MS, CACHE_CAP, OptionsCache, RELATIVE_TTL_MS, cacheKey, periodKey, ttlOf } from "../../web/js/options_cache.js";

const variable = (name, refresh) => ({ name, refresh });
const relative = { fromText: "now-24h", toText: "now", from_ms: 1000, to_ms: 2000 };
const absolute = { fromText: "1000", toText: "2000", from_ms: 1000, to_ms: 2000 };

test("a variable that follows the period has the text of a relative period, and the two ends of an absolute period", () => {
  assert.equal(periodKey(variable("v", "time_change"), relative, "utc"), "rel:now-24h|now|utc");
  assert.equal(periodKey(variable("v", "time_change"), absolute, "utc"), "abs:1000|2000");
  // A moving period has one key although its milliseconds change at each refresh.
  assert.equal(periodKey(variable("v", "time_change"), { ...relative, from_ms: 5, to_ms: 6 }, "utc"), "rel:now-24h|now|utc");
});

test("a variable that loads with the dashboard does not follow the period", () => {
  assert.equal(periodKey(variable("v", "dashboard_load"), relative, "utc"), "");
  assert.equal(periodKey(variable("v", "dashboard_load"), absolute, "utc"), "");
});

test("the key names the variable, the values of its parents and the period", () => {
  const v = variable("host", "time_change");
  const a = cacheKey(v, [["region", { all: false, values: ["eu"] }]], "rel:now-1h|now|utc");
  const b = cacheKey(v, [["region", { all: false, values: ["us"] }]], "rel:now-1h|now|utc");
  const c = cacheKey(v, [["region", { all: false, values: ["eu"] }]], "rel:now-6h|now|utc");
  assert.notEqual(a, b, "another parent value");
  assert.notEqual(a, c, "another period");
  assert.notEqual(a, cacheKey(variable("other", "time_change"), [["region", { all: false, values: ["eu"] }]], "rel:now-1h|now|utc"), "another variable");
  assert.equal(a, cacheKey(v, [["region", { all: false, values: ["eu"] }]], "rel:now-1h|now|utc"));
});

test("a time to live is short for a relative period and longer for an absolute one", () => {
  assert.equal(ttlOf("rel:now-1h|now|utc"), RELATIVE_TTL_MS);
  assert.equal(ttlOf("abs:1|2"), ABSOLUTE_TTL_MS);
  assert.equal(ttlOf(""), ABSOLUTE_TTL_MS, "a variable with no period keeps the long time");
});

test("an entry is returned until its time to live ends", () => {
  let now = 1000;
  const cache = new OptionsCache({ now: () => now });
  cache.set("k", [{ value: "a", text: "a" }], 100);
  assert.deepEqual(cache.get("k"), [{ value: "a", text: "a" }]);
  now = 1099;
  assert.ok(cache.get("k"), "still valid");
  now = 1100;
  assert.equal(cache.get("k"), undefined, "too old");
  assert.equal(cache.size, 0, "an old entry is dropped");
});

test("the cache has a cap and the entry used least recently goes first", () => {
  const cache = new OptionsCache({ cap: 3 });
  for (const k of ["a", "b", "c"]) cache.set(k, [], 1000);
  cache.get("a"); // a is now the most recent
  cache.set("d", [], 1000);
  assert.equal(cache.size, 3);
  assert.equal(cache.get("b"), undefined, "b was the oldest use");
  assert.ok(cache.get("a") && cache.get("c") && cache.get("d"));
  assert.equal(CACHE_CAP, 64);
});

test("forget drops the entries of one variable only, and clear drops all", () => {
  const cache = new OptionsCache();
  const mk = (name, period) => cacheKey(variable(name, "time_change"), [], period);
  cache.set(mk("a", "p1"), [], 1000);
  cache.set(mk("a", "p2"), [], 1000);
  cache.set(mk("b", "p1"), [], 1000);
  cache.forget("a");
  assert.equal(cache.size, 1);
  assert.ok(cache.get(mk("b", "p1")));
  cache.clear();
  assert.equal(cache.size, 0);
});
