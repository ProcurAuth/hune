// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B300 to B305 (9.6, 9.7, 10.7, 10.12, 10.13): the dashboards with no folder. The API writes `"folder": null` for them; a root group has
// no title; the dialogs New dashboard, Import and Move have the entry Root (no folder). Each case has a server of its own with root dashboards: the
// primary volume `main` has `root-one` and `root-two` (and the folder `transit`), and the read-only volume `lab` (title "Lab") has `lab-root` and its two
// folders. The seed volumes of the other cases have no root dashboard, so the cases that count cards keep their numbers.
// B300: the list: the root group of the primary is first and has no title, the folder below keeps its title, the root dashboard of the read-only store is
//       the first group under its title, a lock and no menu on it, and the root is not in the items of GET /api/folders (the member `root` has the right).
// B301: the dialog New dashboard has Root (no folder) as its first entry; a dashboard made there has the folder null and shows in the root group.
// B302: the dialog Import has Root (no folder) as its first entry; a file imported there has the folder null.
// B303: the dialog Move has Root (no folder) for a dashboard in a folder, not for a dashboard at the root; both moves reach the API.
// B304: the header selector lists the dashboards with no folder first, each one on its own, then the folder row and the store row.
// B305: a server with no primary store: the member `root` is read-only, the root dashboards show in the block of the store, and no New button shows.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { WORK, addRootDashboard, api, seedDocument } from "../lib/env.mjs";
import { setFiles } from "../lib/ed.mjs";

const prepare = (volume) => {
  addRootDashboard(volume, "root-one", "Root one", ["demo"]);
  addRootDashboard(volume, "root-two", "Root two");
};
const labPrepare = (volume) => addRootDashboard(volume, "lab-root", "Lab root");
const OWN = { lab: true, prepare, labPrepare };

const ROOT_VALUE = "::root::";
const OPTIONS = `[...document.querySelectorAll('[data-testid="folder-select"] option')].map((o) => [o.value, o.textContent])`;
const PRIMARY = ".dialog .btn--primary, dialog .btn--primary";

// The uid of the dashboard that the page opened after a create: the second segment of the path.
const openedUid = (p) => p.eval(`location.pathname.split("/")[2]`);

export const cases = [
  {
    id: "B300",
    name: "the list: the root group of the primary first with no title, the folder below with its title, the root of the read-only store under its title",
    async run(t) {
      const own = await t.ownServer("root-list", OWN);
      const a = api(own);
      const items = (await a.list()).json.items;
      t.eq(items.filter((i) => i.folder === null).map((i) => [i.uid, i.store, i.read_only]).sort(), [["lab-root", "lab", true], ["root-one", "main", false], ["root-two", "main", false]], "the API writes folder null for the dashboards with no folder");
      const folders = (await a.get("api/folders")).json;
      t.eq(folders.root, { access: { write: true }, read_only: false, store: "main" }, "the member root of the list of folders");
      t.assert(!folders.items.some((f) => f.id === null || f.id === "_root" || f.id === ROOT_VALUE), "the root is not an item of the list of folders");
      const p = await t.open(own.url);
      await p.waitSel(p.tid("row-root-one"));
      const order = await p.eval(`[...document.querySelectorAll(".list-stack > *")].map((e) => e.dataset.testid || e.tagName)`);
      t.eq(order, ["root-main", "folder-transit", "store-lab", "root-lab", "folder-lab-archive", "folder-lab-ops"], "the root group of the primary is first, the root group of the store opens its block");
      t.eq(await p.exists(`${p.tid("root-main")} .group__head`), false, "the root group of the primary has no title");
      t.eq(await p.exists(`${p.tid("root-lab")} .group__head`), false, "the root group of the store has no title of its own");
      t.eq(await p.exists(`${p.tid("folder-transit")} .group__head`), true, "the folder keeps its title: the root group counts as a group");
      t.eq(await p.eval(`[...document.querySelectorAll('${p.tid("root-main")} article[data-testid^="row-"]')].map((e) => e.dataset.testid)`), ["row-root-one", "row-root-two"], "the dashboards of the root group are in title order");
      t.eq(await p.exists(p.tid("card-lock-lab-root")), false, "no lock on the root dashboard of the read-only store");
      t.eq(await p.exists(p.tid("row-menu-lab-root")), false, "no three dots on it");
      t.eq(await p.exists(p.tid("row-menu-root-one")), true, "the three dots on a root dashboard of the primary");
      t.eq(await p.eval(`document.body.textContent.includes("_root")`), false, "the page never shows the reserved word");
      const heights = await p.eval(`[...new Set([...document.querySelectorAll('article[data-testid^="row-"]')].map((e) => Math.round(e.getBoundingClientRect().height)))]`);
      t.eq(heights.length, 1, "all cards have one height: " + heights);
    },
  },

  {
    id: "B301",
    name: "the dialog New dashboard has the entry Root (no folder) first, and the dashboard that it makes has no folder",
    async run(t) {
      const own = await t.ownServer("root-new", OWN);
      const p = await t.open(own.url);
      await p.waitSel(p.tid("list-new-menu"));
      await p.click(p.tid("list-new-menu"));
      await p.click(p.tid("new-dashboard"));
      await p.waitSel(p.tid("new-dashboard-dialog"));
      t.eq(await p.eval(OPTIONS), [[ROOT_VALUE, "Root (no folder)"], ["transit", "Transit desk"]], "the root is the first entry, the folders follow");
      t.eq(await p.eval(`document.querySelector('[data-testid="folder-select"]').value`), ROOT_VALUE, "the first entry is chosen");
      await p.fill(p.tid("new-title"), "Made at the root");
      await p.clickText(PRIMARY, "Create");
      await p.waitFor(`location.pathname.startsWith("/d/")`, { what: "the new dashboard opens" });
      const uid = await openedUid(p);
      const made = (await api(own).dashboard(uid)).json;
      t.eq([made.folder, made.document.title], [null, "Made at the root"], "the API writes folder null");
      const item = (await api(own).list()).json.items.find((i) => i.uid === uid);
      t.eq([item.folder, item.store], [null, "main"], "the list says the same");
      const list = await t.open(own.url);
      await list.waitSel(list.tid(`row-${uid}`));
      t.eq(await list.eval(`[...document.querySelectorAll('${list.tid("root-main")} article[data-testid^="row-"]')].map((e) => e.dataset.testid)`), [`row-${uid}`, "row-root-one", "row-root-two"], "the new dashboard is in the root group, first in title order");
    },
  },

  {
    id: "B302",
    name: "the dialog Import has the entry Root (no folder) first, and the dashboard that it imports has no folder",
    async run(t) {
      const own = await t.ownServer("root-import", OWN);
      const tmp = join(WORK, "tmp-B302");
      mkdirSync(tmp, { recursive: true });
      const file = join(tmp, "imp-root.json");
      writeFileSync(file, JSON.stringify(seedDocument("imp-root", "Imported at the root"), null, 2) + "\n");
      const p = await t.open(own.url);
      // The dialog asks for the uid of the file before it imports it: the 404 of a new uid is the expected answer.
      t.allow(p, /Failed to load resource: the server responded with a status of 404/);
      await p.waitSel(p.tid("list-new-menu"));
      await p.click(p.tid("list-new-menu"));
      await p.click(p.tid("import"));
      await p.waitSel(p.tid("import-dialog"));
      const options = await p.eval(`[...document.querySelectorAll('[data-testid="import-folder"] option')].map((o) => [o.value, o.textContent])`);
      t.eq(options.slice(0, 2), [[ROOT_VALUE, "Root (no folder)"], ["transit", "Transit desk"]], "the root is the first entry");
      t.eq(await p.eval(`document.querySelector('[data-testid="import-folder"]').value`), ROOT_VALUE, "the first entry is chosen");
      await setFiles(p, '[data-testid="import-file"]', [file]);
      await p.waitSel(p.tid("import-create"));
      await p.click(p.tid("import-create"));
      await p.waitFor(`location.pathname.startsWith("/d/imp-root")`, { what: "the imported dashboard opens" });
      t.eq((await api(own).dashboard("imp-root")).json.folder, null, "the API writes folder null for the import");
    },
  },

  {
    id: "B303",
    name: "the dialog Move has the entry Root (no folder) for a dashboard in a folder, not for a dashboard at the root",
    async run(t) {
      const own = await t.ownServer("root-move", OWN);
      const a = api(own);
      const p = await t.open(own.url);
      await p.waitSel(p.tid("row-menu-nodes"));
      // A dashboard in a folder: the root is offered, the folder is chosen.
      await p.click(p.tid("row-menu-nodes"));
      await p.clickText('.menu [role="menuitem"]', "Move");
      await p.waitSel(p.tid("folder-select"));
      t.eq(await p.eval(OPTIONS), [[ROOT_VALUE, "Root (no folder)"], ["transit", "Transit desk"]], "the root is offered to a dashboard in a folder");
      t.eq(await p.eval(`document.querySelector('[data-testid="folder-select"]').value`), "transit", "its own folder is chosen");
      await p.select(p.tid("folder-select"), ROOT_VALUE);
      await p.clickText(PRIMARY, "Move");
      await p.waitFor(`!document.querySelector('[data-testid="folder-select"]')`, { what: "the dialog closes" });
      t.eq((await a.dashboard("nodes")).json.folder, null, "the dashboard is at the root");
      await p.waitSel(`${p.tid("root-main")} ${p.tid("row-nodes")}`);
      // A dashboard at the root: no entry Root; it moves to a folder.
      await p.click(p.tid("row-menu-root-one"));
      await p.clickText('.menu [role="menuitem"]', "Move");
      await p.waitSel(p.tid("folder-select"));
      t.eq(await p.eval(OPTIONS), [["transit", "Transit desk"]], "a dashboard at the root has no entry Root in its own dialog");
      await p.clickText(PRIMARY, "Move");
      await p.waitFor(`!document.querySelector('[data-testid="folder-select"]')`, { what: "the dialog closes" });
      t.eq((await a.dashboard("root-one")).json.folder, "transit", "the dashboard is in the folder");
      t.eq((await a.list()).json.items.filter((i) => i.folder === null).map((i) => i.uid).sort(), ["lab-root", "nodes", "root-two"], "the list agrees");
    },
  },

  {
    id: "B304",
    name: "the header selector lists the dashboards with no folder first, each on its own, then the folder row and the store row",
    async run(t) {
      const own = await t.ownServer("root-selector", OWN);
      const p = await t.open(`${own.url}d/nodes`);
      await p.waitSel(p.tid("header-switcher"));
      await p.click(p.tid("header-switcher"));
      const menu = '[data-testid="header-switcher-menu"]';
      await p.waitSel(`${menu} .menu__group`);
      const top = await p.eval(`[...document.querySelector('${menu}').children].filter((e) => !e.classList.contains("menu__children")).map((e) => e.dataset.testid || e.textContent.trim())`);
      t.eq(top, ["switcher-item-root-one", "switcher-item-root-two", "Transit desk", "Lab"], "the dashboards with no folder first, then the folder and the store");
      await p.clickText(`${menu} .menu__group`, "Lab");
      t.eq(await p.eval(`[...document.querySelectorAll('${menu} [data-testid^="switcher-item-lab-"]')].map((e) => e.dataset.testid)`).then((x) => x.slice(0, 1)), ["switcher-item-lab-root"], "the root dashboard of the store comes before its folders");
      await p.click(p.tid("switcher-item-root-two"));
      await p.waitFor(`location.pathname.endsWith("/d/root-two/root-two")`, { what: "the dashboard with no folder opens" });
    },
  },

  {
    id: "B305",
    name: "a read-only store with root dashboards: its root dashboards show in its block with a lock and no menu, the root of the page belongs to the primary store, and the button New shows",
    async run(t) {
      const own = await t.ownServer("root-readonly", { editing: false, prepare });
      const folders = (await api(own).get("api/folders")).json;
      t.eq(folders.root, { access: { write: true }, read_only: false, store: "scratch" }, "the root of the page belongs to the primary store");
      const p = await t.open(own.url);
      await p.waitSel(p.tid("row-root-one"));
      t.eq(await p.eval(`document.querySelector('[data-testid="list-new-menu"]').hidden`), false, "the button New shows: the primary store takes the new dashboards");
      t.eq(await p.exists(p.tid("row-menu-root-one")), false, "no three dots on a root dashboard");
      t.eq(await p.exists(p.tid("card-lock-root-one")), false, "no lock on the root dashboard");
    },
  },
];
