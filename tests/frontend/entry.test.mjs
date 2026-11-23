// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the entry redirect of the list page (9.7): the pure decision and the guard.
import test from "node:test";
import assert from "node:assert/strict";
import { GUARD_KEY, GUARD_WINDOW_MS, entryTarget, guardValue, parseGuard } from "../../web/js/entry.js";

const one = [{ uid: "stream", title: "Stream Example", folder: "ops", state: "valid" }];
const NOW = 1_800_000_000_000;
const decide = (over = {}) => entryTarget({ items: one, search: "", isEntry: true, guard: null, now: NOW, ...over });

test("one valid dashboard on the entry route gives its path with the slug", () => {
  assert.equal(decide(), "d/stream/stream-example");
});

test("a title with no slug gives d/<uid>", () => {
  assert.equal(decide({ items: [{ uid: "x1", title: "!!!", state: "valid" }] }), "d/x1");
});

test("a missing state counts as valid", () => {
  assert.equal(decide({ items: [{ uid: "a", title: "A" }] }), "d/a/a");
});

test("a folder does not matter", () => {
  assert.equal(decide({ items: [{ uid: "a", title: "A", folder: "deep-folder", state: "valid" }] }), "d/a/a");
});

test("not the entry route: no redirect", () => {
  assert.equal(decide({ isEntry: false }), null);
  assert.equal(decide({ isEntry: undefined }), null);
});

test("a search text in the URL: no redirect", () => {
  assert.equal(decide({ search: "str" }), null);
  assert.equal(decide({ search: " " }), null);
});

test("an empty search text does not stop the redirect", () => {
  assert.equal(decide({ search: "" }), "d/stream/stream-example");
  assert.equal(decide({ search: undefined }), "d/stream/stream-example");
});

test("0 items, 2 items or more: no redirect", () => {
  assert.equal(decide({ items: [] }), null);
  assert.equal(decide({ items: [...one, { uid: "b", title: "B", state: "valid" }] }), null);
  assert.equal(decide({ items: null }), null);
});

test("an invalid item never redirects, alone or beside a valid one", () => {
  assert.equal(decide({ items: [{ uid: "bad", title: "Bad", state: "invalid" }] }), null);
  assert.equal(decide({ items: [...one, { uid: "bad", title: "Bad", state: "invalid" }] }), null);
});

test("an item with no uid never redirects", () => {
  assert.equal(decide({ items: [{ title: "A", state: "valid" }] }), null);
  assert.equal(decide({ items: [{ uid: "", title: "A", state: "valid" }] }), null);
});

test("the guard stops a second redirect to the same uid inside the window", () => {
  assert.equal(decide({ guard: { uid: "stream", at: NOW - 1 } }), null);
  assert.equal(decide({ guard: { uid: "stream", at: NOW - (GUARD_WINDOW_MS - 1) } }), null);
  assert.equal(decide({ guard: { uid: "stream", at: NOW } }), null);
});

test("the guard ends with the window", () => {
  assert.equal(decide({ guard: { uid: "stream", at: NOW - GUARD_WINDOW_MS } }), "d/stream/stream-example");
  assert.equal(decide({ guard: { uid: "stream", at: NOW - 3_600_000 } }), "d/stream/stream-example");
});

test("the guard of another uid does not count", () => {
  assert.equal(decide({ guard: { uid: "other", at: NOW - 1 } }), "d/stream/stream-example");
});

test("a guard date in the future does not block (clock change)", () => {
  assert.equal(decide({ guard: { uid: "stream", at: NOW + 60_000 } }), "d/stream/stream-example");
});

test("the guard text round-trips and a bad text is no guard", () => {
  assert.deepEqual(parseGuard(guardValue("stream", NOW)), { uid: "stream", at: NOW });
  for (const bad of [null, undefined, "", "x", "{}", '{"uid":1,"at":2}', '{"uid":"a","at":"b"}', "[]", "null"]) assert.equal(parseGuard(bad), null, String(bad));
});

test("the guard window is 10 seconds and the key names the entry redirect", () => {
  assert.equal(GUARD_WINDOW_MS, 10000);
  assert.match(GUARD_KEY, /^hune\./);
});
