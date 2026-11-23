// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the grouping and the search of the list page (9.7).
import test from "node:test";
import assert from "node:assert/strict";
import { groupDashboards, matchesQuery, tagColor } from "../../web/js/list.js";

const folders = [{ id: "b", title: "beta" }, { id: "a", title: "Alpha" }, { id: "c", title: "Zeta" }];
const item = (uid, folder, title, tags = []) => ({ uid, folder, title, tags });

test("groups in folder title order and rows in title order, ASCII fold", () => {
  const groups = groupDashboards(folders, [item("1", "b", "x"), item("2", "a", "Zed"), item("3", "a", "alpha"), item("4", "a", "Beta")]);
  assert.deepEqual(groups.map((g) => g.folder.id), ["a", "b", "c"]);
  assert.deepEqual(groups[0].items.map((i) => i.uid), ["3", "4", "2"]);
  assert.equal(groups[2].items.length, 0);
});

test("search matches title, uid or tag, with ASCII letters folded", () => {
  const i = item("Abc-1", "a", "Weight Report", ["Chain"]);
  assert.ok(matchesQuery(i, "weight"));
  assert.ok(matchesQuery(i, "ABC"));
  assert.ok(matchesQuery(i, "chai"));
  assert.ok(!matchesQuery(i, "other"));
  assert.ok(matchesQuery(i, ""));
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
