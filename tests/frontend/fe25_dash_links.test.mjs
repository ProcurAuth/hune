// FE-25: the URLs and the lists of the dashboard links (9.8a, F19).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { MAX_ITEMS, MAX_PAGES, addedNames, appendNames, dashboardItems, fetchItems, itemHref, linkHref, listTag } from "../../web/js/dash_links.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-25-dash-links.json", import.meta.url)));

test("FE-25 the URL of a link of the kind `link`", () => {
  for (const c of v.hrefs) {
    const link = { kind: "link", title: "t", target_blank: false, include_vars: false, keep_time: false, ...c.link };
    const ctx = { ...v.ctx, ...(c.ctx || {}) };
    assert.equal(linkHref(link, ctx), c.href, c.name);
  }
});

test("FE-25 the added names: the time, then the variables in the order of the list", () => {
  assert.equal(addedNames({ keep_time: true, include_vars: true }, v.ctx), "from=now-6h&to=now&var-env=prod&var-v=%24__all&var-s=a%20b");
  assert.equal(addedNames({ keep_time: false, include_vars: false }, v.ctx), "");
  assert.equal(addedNames({ include_vars: true }, { ...v.ctx, variables: [] }), "");
  assert.equal(addedNames({ keep_time: true }, { ...v.ctx, fromText: "1700000000000", toText: "now/d" }), "from=1700000000000&to=now%2Fd");
  assert.equal(appendNames("a", ""), "a");
});

test("FE-25 the filter, the order and the cap of the items", () => {
  for (const c of v.items.cases) {
    const got = dashboardItems({ kind: "dashboards", tags: c.tags }, v.items.list, "current").map((i) => i.uid);
    assert.deepEqual(got, c.uids, c.name);
  }
});

test("FE-25 the order is the lower-case title by code unit: 10, 9, a b, a(b, a-b, a.b, a_b, ab, beta, ea, zz, Ea with an accent", () => {
  const titles = ["Éa", "zz", "ea", "beta", "ab", "a_b", "a.b", "a-b", "a(b", "a b", "9", "10"].map((title, i) => ({ uid: `u${i}`, title, tags: [], state: "valid" }));
  const got = dashboardItems({ tags: [] }, titles, "x").map((i) => i.title);
  assert.deepEqual(got, ["10", "9", "a b", "a(b", "a-b", "a.b", "a_b", "ab", "beta", "ea", "zz", "Éa"]);
});

test("FE-25 at most 50 items, and the input list does not change", () => {
  const list = Array.from({ length: 60 }, (_, i) => ({ uid: `u${String(i).padStart(2, "0")}`, title: `t${String(i).padStart(2, "0")}`, tags: [], state: "valid" }));
  const copy = JSON.stringify(list);
  const got = dashboardItems({ tags: [] }, list, "none");
  assert.equal(got.length, MAX_ITEMS);
  assert.equal(got[0].uid, "u00");
  assert.equal(got[49].uid, "u49");
  assert.equal(JSON.stringify(list), copy);
  assert.deepEqual(dashboardItems({ tags: [] }, null, "x"), []);
});

test("FE-25 an item with no title or no uid, and a tag list that is not an array, never match", () => {
  const items = [{ uid: "a", state: "valid", tags: [] }, { title: "t", state: "valid", tags: [] }, { uid: "b", title: "b", state: "valid" }, null];
  assert.deepEqual(dashboardItems({ tags: ["x"] }, items, "c"), []);
  assert.deepEqual(dashboardItems({ tags: [] }, items, "c").map((i) => i.uid), ["b"]);
});

test("FE-25 the URL of a dashboard item", () => {
  const ctx = v.ctx;
  assert.equal(itemHref({ keep_time: false, include_vars: false }, { uid: "abc", title: "My Board" }, ctx), "d/abc/my-board");
  assert.equal(itemHref({ keep_time: true, include_vars: false }, { uid: "abc", title: "My Board" }, ctx), "d/abc/my-board?from=now-6h&to=now");
  assert.equal(itemHref({ keep_time: true, include_vars: true }, { uid: "abc", title: "!!!" }, ctx), "d/abc?from=now-6h&to=now&var-env=prod&var-v=%24__all&var-s=a%20b");
});

test("FE-25 the tag that the page sends: the first one", () => {
  assert.equal(listTag({ tags: ["x", "y"] }), "x");
  assert.equal(listTag({ tags: [] }), null);
  assert.equal(listTag({}), null);
});

test("FE-25 the page reads three pages at most and sends the first tag only", async () => {
  const calls = [];
  const list = async (params) => {
    calls.push(params);
    const n = calls.length;
    return { items: [{ uid: `u${n}`, title: `t${n}`, tags: ["x", "y"], state: "valid" }], next: `cursor${n}` };
  };
  const items = await fetchItems({ kind: "dashboards", tags: ["x", "y"] }, list, "z");
  assert.equal(calls.length, MAX_PAGES);
  assert.deepEqual(calls.map((c) => [c.tag, c.limit, c.after]), [["x", 500, null], ["x", 500, "cursor1"], ["x", 500, "cursor2"]]);
  assert.deepEqual(items.map((i) => i.uid), ["u1", "u2", "u3"]);
});

test("FE-25 the page stops at the last page, sends no tag for a link with no tag, and tests the other tags itself", async () => {
  const calls = [];
  const list = async (params) => {
    calls.push(params);
    return { items: [{ uid: "a", title: "a", tags: ["x"], state: "valid" }, { uid: "b", title: "b", tags: ["x", "y"], state: "valid" }], next: null };
  };
  const both = await fetchItems({ kind: "dashboards", tags: ["x", "y"] }, list, "z");
  assert.deepEqual(both.map((i) => i.uid), ["b"]);
  await fetchItems({ kind: "dashboards", tags: [] }, list, "z");
  assert.equal(calls.length, 2);
  assert.equal(calls[1].tag, null);
});

test("FE-25 a failed request rejects: the caller shows no item and writes a warning", async () => {
  await assert.rejects(() => fetchItems({ tags: [] }, async () => { throw new Error("no"); }, "z"), /no/);
});
