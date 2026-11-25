// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the dashboards with no folder in the page (9.6, 9.7, 10.7, 10.12, 10.13): the root group of the list, its place and its title rule, the
// root entry of the dialogs New dashboard, Import and Move, the bodies with `folder: null`, the selector, and the conflict of a move to the root.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { groupDashboards } from "../../web/js/list.js";
import { switcherItems } from "../../web/js/switcher.js";
import { groupByStore, isRootGroup, rootGroupCount, rootWritable } from "../../web/js/stores.js";
import { ROOT_LABEL, ROOT_NAME, ROOT_VALUE, folderValue, folderWire, loadTargets, placeName, targetEntries } from "../../web/js/editor/root_target.js";
import { conflictKind } from "../../web/js/editor/save_logic.js";

const read = (p) => readFileSync(new URL(`../../web/${p}`, import.meta.url), "utf8");
const ID_RULE = /^[a-z0-9][a-z0-9-]{0,39}$/;

const session = {
  editing: true,
  stores: [
    { id: "main", kind: "volume", title: null, primary: true, read_only: false },
    { id: "lab", kind: "volume", title: "Lab", primary: false, read_only: true },
  ],
};
const folders = [
  { id: "ops", title: "Operations", store: "main", read_only: false, access: { write: true } },
  { id: "lab-ops", title: "Lab operations", store: "lab", read_only: true, access: { write: false } },
];
const item = (uid, folder, store, extra = {}) => ({ uid, folder, title: uid, store, read_only: store !== "main", ...extra });
const items = [
  item("zed", "ops", "main"), item("r2", null, "main"), item("r1", null, "main"),
  item("lab-a", "lab-ops", "lab"), item("lab-root", null, "lab"),
];

test("the value of the root entry is never a folder id, and the value for the API of the root is null", () => {
  assert.ok(!ID_RULE.test(ROOT_VALUE), "no folder has this id");
  assert.equal(folderWire(ROOT_VALUE), null);
  assert.equal(folderWire("ops"), "ops");
  assert.equal(folderValue(null), ROOT_VALUE);
  assert.equal(folderValue(undefined), ROOT_VALUE);
  assert.equal(folderValue("ops"), "ops");
  assert.equal(folderWire(folderValue(null)), null);
  assert.equal(ROOT_LABEL, "Root (no folder)");
  assert.equal(ROOT_NAME, "Root");
});

test("the right to write at the root comes from the member root of the answer, and an older server gives none", () => {
  assert.equal(rootWritable({ access: { write: true }, read_only: false, store: "main" }), true);
  assert.equal(rootWritable({ access: { write: false }, read_only: true, store: null }), false);
  assert.equal(rootWritable(undefined), false);
  assert.equal(rootWritable(null), false);
  assert.equal(rootWritable({}), false);
  assert.equal(rootWritable({ access: { write: "yes" } }), false, "only the value true counts");
});

test("a folder select has the root first when the root accepts a dashboard, then the folders in their order", () => {
  const list = [{ id: "a", title: "Alpha" }, { id: "b", title: "Beta" }];
  assert.deepEqual(targetEntries(list, true), [{ value: ROOT_VALUE, label: ROOT_LABEL }, { value: "a", label: "Alpha" }, { value: "b", label: "Beta" }]);
  assert.deepEqual(targetEntries(list, false).map((e) => e.value), ["a", "b"]);
  assert.deepEqual(targetEntries([], true).map((e) => e.value), [ROOT_VALUE], "no folder accepts a dashboard but the root does");
  assert.deepEqual(targetEntries([], false), []);
});

test("loadTargets uses the folders and the root of the list page, else it asks the API once", async () => {
  const calls = [];
  const api = { folders: async () => { calls.push("folders"); return { items: folders, root: { access: { write: true } } }; } };
  const page = await loadTargets({ folders, root: { access: { write: false } } }, api);
  assert.deepEqual(page.folders.map((f) => f.id), ["ops"], "only the folders with the right to write");
  assert.equal(page.rootOk, false);
  assert.deepEqual(calls, [], "the list page already has both answers");
  const fetched = await loadTargets({}, api);
  assert.equal(fetched.rootOk, true);
  assert.deepEqual(calls, ["folders"]);
  const noRoot = await loadTargets({ folders, root: undefined }, { folders: async () => ({ items: folders }) });
  assert.equal(noRoot.rootOk, false, "an older server has no member root");
});

test("placeName says Root for no folder and the title of a folder otherwise", () => {
  assert.equal(placeName(null), "Root");
  assert.equal(placeName("ops", folders), "Operations");
  assert.equal(placeName("gone", folders), "gone");
});

test("the list has a root group for each store, first, and the folders after it", () => {
  const groups = groupDashboards(folders, items);
  assert.deepEqual(groups.map((g) => (isRootGroup(g) ? `root:${g.folder.store}` : g.folder.id)), ["root:lab", "root:main", "lab-ops", "ops"]);
  const main = groups.find((g) => isRootGroup(g) && g.folder.store === "main");
  assert.equal(main.folder.id, null);
  assert.equal(main.folder.read_only, false);
  assert.deepEqual(main.items.map((i) => i.uid), ["r1", "r2"], "the dashboards of a group are in title order");
  assert.equal(groups.find((g) => isRootGroup(g) && g.folder.store === "lab").folder.read_only, true);
});

test("the order Z-A reverses the folders and the dashboards, and the root groups stay first", () => {
  const groups = groupDashboards(folders, items, { descending: true });
  assert.deepEqual(groups.map((g) => (isRootGroup(g) ? `root:${g.folder.store}` : g.folder.id)), ["root:lab", "root:main", "ops", "lab-ops"]);
  assert.deepEqual(groups.find((g) => isRootGroup(g) && g.folder.store === "main").items.map((i) => i.uid), ["r2", "r1"]);
});

test("a list with no root dashboard has no root group", () => {
  assert.equal(groupDashboards(folders, [item("a", "ops", "main")]).some(isRootGroup), false);
  assert.equal(isRootGroup({ folder: { id: "ops" } }), false);
  assert.equal(isRootGroup(null), false);
});

test("the root group of the primary store is at the root of the page and the root group of a read-only store opens its block", () => {
  const split = groupByStore(groupDashboards(folders, items), session);
  assert.deepEqual(split.primary.map((g) => (isRootGroup(g) ? "root" : g.folder.id)), ["root", "ops"], "the root group is first");
  assert.equal(split.stores.length, 1);
  assert.deepEqual(split.stores[0].groups.map((g) => (isRootGroup(g) ? "root" : g.folder.id)), ["root", "lab-ops"], "the root of the store is before its folders");
  assert.equal(rootGroupCount(split), 3, "the root group counts as a group, so the folder titles show");
  const onlyRoot = groupByStore(groupDashboards([], [item("r", null, "main")]), session);
  assert.equal(rootGroupCount(onlyRoot), 1, "one root group alone: no title anywhere");
});

test("the selector lists the dashboards with no folder first and each one on its own, then the folder rows", () => {
  const data = { folders, items, session };
  const rows = switcherItems(data, "zed", () => {});
  assert.deepEqual(rows.map((r) => [r.key, r.group === true]), [["r1", false], ["r2", false], ["folder:ops", true], ["store:lab", true]]);
  const lab = rows[3];
  assert.deepEqual(lab.children.map((c) => [c.key, c.group === true]), [["lab-root", false], ["folder:lab-ops", true]], "the root dashboard of the store is before its folder");
  const rootsOnly = switcherItems({ folders: [], items: [item("r1", null, "main"), item("r2", null, "main")], session: { stores: session.stores } }, "r1", () => {});
  assert.deepEqual(rootsOnly.map((r) => r.key), ["r1", "r2"], "one root group alone: the dashboards, with no row");
  const rootAndFolder = switcherItems({ folders: [folders[0]], items: [item("r1", null, "main"), item("zed", "ops", "main")], session: { stores: session.stores } }, "r1", () => {});
  assert.deepEqual(rootAndFolder.map((r) => [r.key, r.group === true]), [["r1", false], ["folder:ops", true]], "a root dashboard and one folder: the folder keeps its row");
});

test("the conflict of a move sees a move to the root and a move from the root", () => {
  const err = (extra) => ({ extra });
  assert.equal(conflictKind(err({ current_etag: "a", current_folder: null }), "a", "ops"), "move", "moved to the root");
  assert.equal(conflictKind(err({ current_etag: "a", current_folder: "ops" }), "a", null), "move", "moved from the root");
  assert.equal(conflictKind(err({ current_etag: "a", current_folder: null }), "a", null), "changed", "the folder did not change");
  assert.equal(conflictKind(err({ current_etag: "a" }), "a", "ops"), "changed", "no member current_folder: not a move");
  assert.equal(conflictKind(err({ current_etag: "b", current_folder: null }), "a", "ops"), "changed", "the etag changed");
});

test("the dialogs send null for the root, and the list page reads the root right and never shows a title for the root group", () => {
  const folders_js = read("js/editor/folders.js");
  assert.match(folders_js, /createDashboard\(\{ folder: folderWire\(folder\.value\), message: "", document: smallestDocument\(uid, text\) \}/);
  assert.match(folders_js, /const wire = folderWire\(select\.value\);/);
  assert.match(folders_js, /moveDashboard\(item\.uid, \{ folder: wire, message: "" \}/);
  assert.match(folders_js, /targetEntries\(folders, rootOk && item\.folder !== null\)/, "a dashboard at the root has no root entry in its move dialog");
  assert.match(folders_js, /if \(!folders\.length && !rootOk\)/, "the dialog opens when only the root accepts a dashboard");
  const imp = read("js/editor/import_export.js");
  assert.match(imp, /targetEntries\(folders, rootOk\)/);
  assert.match(imp, /if \(select\.value !== NEW_FOLDER\) return folderWire\(select\.value\);/);
  const list = read("js/list.js");
  assert.match(list, /folderRoot = folderAnswer\.root;/);
  assert.match(list, /rootWritable\(folderRoot\)/, "New dashboard and Import need the root or a folder");
  assert.match(list, /root: folderRoot/, "the dialogs get the root right of the page");
  assert.match(list, /if \(showTitle && !isRoot\) section\.append\(folderHead\(group, nested\)\);/, "a root group never has a title");
  assert.match(list, /isRoot \? `root-\$\{group\.folder\.store \|\| "primary"\}`/, "the hook of a root group");
});
