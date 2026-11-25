// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the stores in the page (9.6, 9.7, 9.8): the groups of the list by store, the marks of a card, the items of the selector, and the texts.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { groupDashboards } from "../../web/js/list.js";
import { switcherItems } from "../../web/js/switcher.js";
import { READ_ONLY_ERROR_TEXT, READ_ONLY_TEXT, STALE_CARD_TEXT, STALE_PAGE_TEXT, cardMarks, groupByStore, isReadOnly, isStale, rootGroupCount, storeTable } from "../../web/js/stores.js";
import { classifySaveError } from "../../web/js/editor/save_logic.js";

const read = (p) => readFileSync(new URL(`../../web/${p}`, import.meta.url), "utf8");
const session = {
  editing: true,
  stores: [
    { id: "main", kind: "volume", title: null, primary: true, read_only: false },
    { id: "lab", kind: "volume", title: "Lab", primary: false, read_only: true },
    { id: "archive", kind: "postgres", title: "Archive", primary: false, read_only: true },
  ],
};
const folders = [
  { id: "ops", title: "Operations", store: "main", read_only: false },
  { id: "lab-ops", title: "Lab operations", store: "lab", read_only: true },
  { id: "lab-old", title: "Lab archive", store: "lab", read_only: true },
  { id: "arc", title: "Old", store: "archive", read_only: true },
];
const item = (uid, folder, store, extra = {}) => ({ uid, folder, title: uid, store, read_only: store !== "main", ...extra });
const items = [item("a", "ops", "main"), item("b", "lab-ops", "lab"), item("c", "lab-old", "lab"), item("d", "arc", "archive")];

test("the table of the stores comes from the session; an older server gives an empty table", () => {
  assert.deepEqual([...storeTable(session).keys()], ["main", "lab", "archive"]);
  assert.equal(storeTable({}).size, 0);
  assert.equal(storeTable(null).size, 0);
  assert.equal(storeTable({ stores: "x" }).size, 0);
});

test("a read-only store is one root group with its folders in it; the primary folders stay at the root", () => {
  const split = groupByStore(groupDashboards(folders, items), session);
  assert.deepEqual(split.primary.map((g) => g.folder.id), ["ops"]);
  assert.deepEqual(split.stores.map((b) => b.store.title), ["Archive", "Lab"], "the stores are in title order");
  assert.deepEqual(split.stores[1].groups.map((g) => g.folder.id), ["lab-old", "lab-ops"], "the folders of a store are in title order");
  assert.equal(rootGroupCount(split), 3, "the root groups: one folder of the primary and two stores");
});

test("the Z-A preference reverses the stores", () => {
  const split = groupByStore(groupDashboards(folders, items, { descending: true }), session, { descending: true });
  assert.deepEqual(split.stores.map((b) => b.store.title), ["Lab", "Archive"]);
});

test("a server with no member stores gives only the primary, and a store with no title uses its id", () => {
  const older = groupByStore(groupDashboards([{ id: "a", title: "A" }, { id: "b", title: "B" }], []), {});
  assert.equal(older.primary.length, 2, "folders with no marks are folders of the primary");
  assert.equal(older.stores.length, 0);
  const noTable = groupByStore(groupDashboards(folders, items), {});
  assert.equal(noTable.primary.length, 1, "with no table, a folder is read-only when it says so");
  assert.equal(noTable.stores.length, 2, "the two read-only stores are blocks, named by their id");
  const bare = groupByStore(groupDashboards([{ id: "x", title: "X", store: "z", read_only: true }], [item("u", "x", "z")]), { stores: [] });
  assert.equal(bare.stores[0].store.title, "z");
});

test("an item that names a folder that the list does not hold keeps its store", () => {
  const groups = groupDashboards([], [item("q", "lost", "lab")]);
  const split = groupByStore(groups, session);
  assert.equal(split.primary.length, 0);
  assert.equal(split.stores[0].store.id, "lab");
});

test("a card shows the lock for a read-only item and the mark for a file that is not valid", () => {
  assert.deepEqual(cardMarks({ read_only: true }), { readOnly: true, stale: false });
  assert.deepEqual(cardMarks({ read_only: false, stale: "file_invalid" }), { readOnly: false, stale: true });
  assert.deepEqual(cardMarks({ read_only: true, stale: "file_invalid" }), { readOnly: true, stale: true });
  assert.deepEqual(cardMarks({}), { readOnly: false, stale: false });
  assert.ok(isReadOnly({ read_only: true }) && !isReadOnly({ read_only: "yes" }) && !isReadOnly(null));
  assert.ok(isStale({ stale: "file_invalid" }) && !isStale({ stale: "" }) && !isStale({}) && !isStale(null));
});

test("the texts are the ones of the specification", () => {
  assert.equal(READ_ONLY_TEXT, "Read-only store");
  assert.equal(STALE_CARD_TEXT, "The file is not valid. The last valid version is shown.");
  assert.equal(STALE_PAGE_TEXT, "The file of this dashboard is not valid. The last valid version is shown.");
  assert.equal(READ_ONLY_ERROR_TEXT, "This dashboard is in a read-only store. It cannot be changed.");
});

test("a refused write to a read-only store gives the plain message and is shown, not retried", () => {
  const answer = classifySaveError({ code: "read_only_store", status: 403, message: "x" });
  assert.deepEqual(answer, { kind: "show", text: READ_ONLY_ERROR_TEXT });
  assert.equal(classifySaveError({ code: "editing_disabled", status: 403, message: "Editing is off." }).text, "Editing is off.");
});

test("the selector shows each read-only store as a row with a lock; one folder opens its dashboards at once", () => {
  const data = { folders, items, session };
  const rows = switcherItems(data, "b", () => {});
  assert.deepEqual(rows.map((r) => [r.key, r.label, r.icon]), [
    ["folder:ops", "Operations", "folder"],
    ["store:archive", "Archive", "lock"],
    ["store:lab", "Lab", "lock"],
  ]);
  const lab = rows[2];
  assert.equal(lab.open, true, "the store of the current dashboard is open");
  assert.deepEqual(lab.children.map((c) => [c.key, c.label]), [["folder:lab-old", "Lab archive"], ["folder:lab-ops", "Lab operations"]], "two folders: two rows");
  const archive = rows[1];
  assert.equal(archive.open, false);
  assert.deepEqual(archive.children.map((c) => c.key), ["d"], "one folder: the dashboards of the store directly");
});

test("the selector lists the dashboards alone for one root group, also when it is a read-only store", () => {
  const onlyStore = { folders: [folders[1]], items: [item("b", "lab-ops", "lab")], session };
  assert.deepEqual(switcherItems(onlyStore, "b", () => {}).map((r) => r.key), ["b"]);
  const twoFolders = { folders: [folders[1], folders[2]], items: [item("b", "lab-ops", "lab"), item("c", "lab-old", "lab")], session };
  assert.deepEqual(switcherItems(twoFolders, "b", () => {}).map((r) => r.key), ["folder:lab-old", "folder:lab-ops"]);
  assert.deepEqual(switcherItems({ folders: [], items: [], session }, "x", () => {}), []);
});

test("the list page shows the lock and the mark on a card, a title for each store and no menu for a read-only card", () => {
  const list = read("js/list.js");
  assert.match(list, /markNodes\.push\(el\("span", \{ class: "card__mark card__mark--stale"/);
  assert.match(list, /markNodes\.push\(el\("span", \{ class: "card__mark card__mark--lock"/);
  assert.match(list, /testid: `card-lock-\$\{item\.uid\}`/);
  assert.match(list, /testid: `card-stale-\$\{item\.uid\}`/);
  assert.match(list, /class: "group__head group__head--store"/);
  assert.match(list, /const showRoots = rootGroupCount\(split\) >= 2;/);
  const css = read("css/list.css");
  assert.match(css, /\.card__mark--stale \{ color: var\(--warning\); \}/);
  assert.match(css, /\.group__head--nested \{[^}]*padding-left: var\(--space-16\)/);
  assert.ok(!/#[0-9a-fA-F]{3,8}\b/.test(css.slice(css.indexOf(".card__mark"), css.indexOf(".card__mark") + 400)), "no colour literal in the mark rules");
});

test("the dashboard page has the lock where the switch Edit would be, and says once that the file is not valid", () => {
  const dash = read("js/dashboard.js");
  assert.match(dash, /readOnly: answer\.read_only === true, stale: isStale\(answer\)/);
  assert.match(dash, /header\.setActions\(timeControl\.el, lock, editSwitch\);/);
  assert.match(dash, /dataset: \{ testid: "read-only-lock" \}/);
  assert.match(dash, /key: "stale", text: STALE_PAGE_TEXT/);
  assert.match(dash, /if \(!loaded\.stale \|\| staleNoticed\) return;/);
  assert.match(dash, /function canEdit\(\) \{\s*return session\.editing === true && loaded\.access && loaded\.access\.write === true;/, "the switch follows access.write, false for a read-only store");
});
