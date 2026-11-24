// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the grouping, the tag filter, the short age, the threshold of the filters and the rights of a card on the list page (9.7).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { cardRights, filtersShown, groupDashboards, matchesTag, shortAge, tagColor } from "../../web/js/list.js";

const read = (p) => readFileSync(new URL(`../../web/${p}`, import.meta.url), "utf8");
const folders = [{ id: "b", title: "beta" }, { id: "a", title: "Alpha" }, { id: "c", title: "Zeta" }];
const item = (uid, folder, title, tags = []) => ({ uid, folder, title, tags });

test("groups in folder title order and cards in title order, ASCII fold", () => {
  const groups = groupDashboards(folders, [item("1", "b", "x"), item("2", "a", "Zed"), item("3", "a", "alpha"), item("4", "a", "Beta")]);
  assert.deepEqual(groups.map((g) => g.folder.id), ["a", "b", "c"]);
  assert.deepEqual(groups[0].items.map((i) => i.uid), ["3", "4", "2"]);
  assert.equal(groups[2].items.length, 0);
});

test("the tag filter matches one tag name, with ASCII letters folded, and not a part of a name, a title or a uid", () => {
  const i = item("Abc-1", "a", "Weight Report", ["Chain", "ops"]);
  assert.ok(matchesTag(i, "chain"));
  assert.ok(matchesTag(i, "CHAIN"));
  assert.ok(matchesTag(i, "ops"));
  assert.ok(!matchesTag(i, "chai"), "a part of a tag");
  assert.ok(!matchesTag(i, "weight"), "a word of the title");
  assert.ok(!matchesTag(i, "abc"), "a part of the uid");
  assert.ok(matchesTag(i, ""), "no filter");
  assert.ok(!matchesTag(item("u", "a", "t"), "chain"), "an item with no tag");
});

test("the Z-A preference reverses the folders and the dashboards", () => {
  const groups = groupDashboards(folders, [item("1", "b", "x"), item("2", "a", "Zed"), item("3", "a", "alpha")], { descending: true });
  assert.deepEqual(groups.map((g) => g.folder.id), ["c", "b", "a"]);
  assert.deepEqual(groups[2].items.map((i) => i.uid), ["2", "3"]);
});

test("a tag has the colour that Grafana 12.4.2 gives it, whatever the case", () => {
  const measured = { demo: "#3f2b5b", ops: "#3f2b5b", density: "#4c7a3f", overview: "#584477", production: "#052b51", showcase: "#ae561a", states: "#890f02", panels: "#d32d20", tables: "#000000", variables: "#3d71aa", stream: "#99440a" };
  for (const [name, color] of Object.entries(measured)) assert.equal(tagColor(name), color, name);
  assert.equal(tagColor("Showcase"), tagColor("showcase"));
});

test("the short age: now, minutes, hours, days, weeks, months and years, rounded down", () => {
  const now = Date.parse("2026-11-24T12:00:00Z");
  const ago = (ms) => new Date(now - ms).toISOString();
  const MIN = 60000;
  const HOUR = 60 * MIN;
  const DAY = 24 * HOUR;
  assert.equal(shortAge(ago(0), now), "now");
  assert.equal(shortAge(ago(59 * 1000), now), "now");
  assert.equal(shortAge(ago(-5 * MIN), now), "now", "a time in the future");
  assert.equal(shortAge(ago(MIN), now), "1m");
  assert.equal(shortAge(ago(59 * MIN), now), "59m");
  assert.equal(shortAge(ago(HOUR), now), "1h");
  assert.equal(shortAge(ago(2 * HOUR + 59 * MIN), now), "2h");
  assert.equal(shortAge(ago(23 * HOUR + 59 * MIN), now), "23h");
  assert.equal(shortAge(ago(DAY), now), "1d");
  assert.equal(shortAge(ago(3 * DAY), now), "3d");
  assert.equal(shortAge(ago(6 * DAY + 23 * HOUR), now), "6d");
  assert.equal(shortAge(ago(7 * DAY), now), "1w");
  assert.equal(shortAge(ago(14 * DAY), now), "2w");
  assert.equal(shortAge(ago(29 * DAY), now), "4w");
  assert.equal(shortAge(ago(30 * DAY), now), "1mo");
  assert.equal(shortAge(ago(364 * DAY), now), "12mo");
  assert.equal(shortAge(ago(365 * DAY), now), "1y");
  assert.equal(shortAge("not a time", now), "");
  assert.equal(shortAge(undefined, now), "");
});

test("the filters show only above the threshold of the session; a missing or invalid member shows them", () => {
  const at = (min, count) => filtersShown({ list_filters_min_dashboards: min }, count);
  assert.equal(at(6, 6), false, "equal: hidden");
  assert.equal(at(6, 7), true);
  assert.equal(at(6, 1), false);
  assert.equal(at(0, 1), true, "0 shows them for any list that is not empty");
  assert.equal(at(0, 0), false);
  assert.equal(at(1000, 1000), false);
  for (const bad of [undefined, null, -1, 1.5, "6", NaN, Infinity, 2 ** 60]) assert.equal(at(bad, 3), true, `invalid member ${String(bad)} shows the filters`);
  assert.equal(filtersShown({}, 3), true);
  assert.equal(filtersShown(null, 3), true);
  assert.equal(filtersShown(undefined, 0), true);
});

test("a card has its buttons only with editing on and the write right; the pencil and the bin share the condition", () => {
  assert.deepEqual(cardRights({ access: { write: true } }, true), { write: true });
  assert.deepEqual(cardRights({ access: { write: false } }, true), { write: false });
  assert.deepEqual(cardRights({ access: { write: true } }, false), { write: false }, "editing off");
  assert.deepEqual(cardRights({ access: {} }, true), { write: false });
  assert.deepEqual(cardRights({}, true), { write: false });
  assert.deepEqual(cardRights({ access: { write: true } }, undefined), { write: false });
  assert.deepEqual(cardRights({ access: { write: true } }, "true"), { write: false }, "a text is not true");
});

test("the page has no search field, no view control, no collapse and no table; it keeps the sort in one key", () => {
  const js = read("js/list.js");
  for (const gone of ["hune.list.collapsed", "hune.list.layout", "list-search", "list-layout", "readCollapsed", "matchesQuery", "<table", '"table"', "list-colhead"]) {
    assert.ok(!js.includes(gone), `${gone} is gone from list.js`);
  }
  assert.match(js, /const SORT_KEY = "hune\.list\.sort"/);
  assert.ok(!/localStorage/.test(js.replace(/function (readPref|writePref)[\s\S]*?\n}\n/g, "")), "the sort is the only stored choice");
  // The page keeps no copy of the threshold: it reads the session at each paint.
  assert.ok(!/list_filters_min_dashboards\s*[=:]/.test(js.replace(/session\.list_filters_min_dashboards/g, "")), "no copy of the number");
});

test("the card is an article with a stretched title link, and its buttons are siblings of the link", () => {
  const js = read("js/list.js");
  assert.match(js, /el\("article", \{ class: `card/);
  assert.match(js, /class: "card__link"/);
  assert.match(js, /\?edit=1/);
  assert.match(js, /callEditor\("folders", "deleteDashboard", item\)/);
  assert.match(js, /callEditor\("folders", "moveDashboard", item\)/);
  const css = read("css/list.css");
  assert.match(css, /\.card__link::after \{ content: ""; position: absolute; inset: 0;/);
  assert.match(css, /\.card__buttons \{ position: absolute;[^}]*z-index: var\(--z-raised\)/);
  assert.match(css, /\.list-stack \{ display: grid; grid-template-columns: repeat\(auto-fit, 400px\);/);
  assert.match(css, /\.group, \.cards \{ display: contents; \}/);
  assert.match(css, /\.group__head \{\s*grid-column: 1 \/ -1;/);
});
