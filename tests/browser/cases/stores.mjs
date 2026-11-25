// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B241 to B244 (9.6, 9.7, 9.8): a server with a primary volume `main` (folder `transit`) and a read-only volume `lab` with the title "Lab"
// (folders `lab-ops` and `lab-archive`). The lab volume has no edit, whatever the page does.
// B241: the list shows the read-only store as one root group with a lock, its folders under it, a lock on each of its cards and no three dots; the
//       dialogs of New dashboard offer only the folders of the primary.
// B242: a file of the read-only store that is not valid after the start: the card, the page and the API show the warning; the lock and the warning
//       text are the ones of the specification.
// B243: the header selector shows the store as a row with a lock that opens its folders and its dashboards.
// B244: the server refuses a write to the read-only store with the code `read_only_store`, and the page of its dashboard has no edit control.
import { api, seedDocument } from "../lib/env.mjs";
import { breakFile, sleep } from "../lib/h.mjs";

const OWN = { lab: true };
const LAB_FILE = "../lab/lab-ops/lab-traffic.json";
const STALE_CARD = "The file is not valid. The last valid version is shown.";
const STALE_PAGE = "The file of this dashboard is not valid. The last valid version is shown.";

export const cases = [
  {
    id: "B241",
    name: "a read-only store: one root group with a lock, a lock on each card, no three dots, and no folder of the store in the dialogs",
    async run(t) {
      const own = await t.ownServer("stores", OWN);
      const session = (await api(own).session()).json;
      t.eq(session.stores.map((x) => [x.id, x.primary, x.read_only]).sort(), [["lab", false, true], ["main", true, false]], "the session lists the stores");
      const p = await t.open(own.url);
      await p.waitSel(p.tid("row-lab-traffic"));
      // Two root groups: the folder of the primary and the store. Each has a title, and the folders of the store are under its title.
      t.eq(await p.text(`${p.tid("store-lab")} .group__title`), "Lab", "the title of the store");
      t.eq(await p.exists(`${p.tid("store-lab")} .icon`), true, "the lock of the store");
      t.eq(await p.eval(`[...document.querySelectorAll(".group__head--nested .group__title")].map((e) => e.textContent).sort()`), ["Lab archive", "Lab operations"], "the folders of the store, under its title");
      t.eq(await p.exists(`${p.tid("folder-transit")} .group__head`), true, "the folder of the primary has its title: two root groups");
      // The lock and the three dots.
      for (const uid of ["lab-traffic", "lab-inventory"]) {
        t.eq(await p.attr(p.tid(`card-lock-${uid}`), "aria-label"), "Read-only store", `the lock of ${uid}`);
        t.eq(await p.exists(p.tid(`row-menu-${uid}`)), false, `no three dots on ${uid}`);
        t.eq(await p.eval(`document.querySelector('[data-testid="row-${uid}"]').querySelectorAll("button:not(.list-tag)").length`), 0, `no button on ${uid}`);
      }
      t.eq(await p.exists(p.tid("card-lock-nodes")), false, "no lock on a card of the primary");
      t.eq(await p.exists(p.tid("row-menu-nodes")), true, "the three dots on a card of the primary");
      const heights = await p.eval(`[...new Set([...document.querySelectorAll('article[data-testid^="row-"]')].map((e) => Math.round(e.getBoundingClientRect().height)))]`);
      t.eq(heights.length, 1, "all cards have one height, with and without the lock: " + heights);
      // The dialogs offer the folders of the primary only.
      await p.click(p.tid("list-new-menu"));
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="list-new-menu-popup"] [role="menuitem"]')].map((e) => e.textContent.trim())`), ["New dashboard", "New folder", "Import"], "the New menu");
      await p.click(p.tid("new-dashboard"));
      await p.waitSel(p.tid("new-dashboard-dialog"));
      // The root of the primary is the first entry; no folder and no root of the read-only store is offered.
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="folder-select"] option')].map((o) => o.value)`), ["::root::", "transit"], "New dashboard offers the root and the folders of the primary only");
      await p.press("Escape");
    },
  },

  {
    id: "B242",
    name: "a file of the read-only store that is not valid after the start: the warning on the card and on the page, with the last valid state",
    async run(t) {
      const own = await t.ownServer("stale-lab", OWN);
      const restore = await breakFile(t, "lab-traffic", { file: LAB_FILE, server: own });
      const list = (await api(own).list()).json.items.find((i) => i.uid === "lab-traffic");
      t.eq([list.read_only, list.store, list.stale], [true, "lab", "file_invalid"], "the API gives read_only, store and stale");
      t.assert(!JSON.stringify(list).includes("lab-traffic.json"), "no path in the item");
      const p = await t.open(own.url);
      await p.waitSel(p.tid("card-stale-lab-traffic"));
      t.eq(await p.attr(p.tid("card-stale-lab-traffic"), "aria-label"), STALE_CARD, "the name of the mark of the card");
      t.eq(await p.exists(p.tid("card-lock-lab-traffic")), true, "the lock stays beside the mark");
      const d = await t.open(`${own.url}d/lab-traffic`);
      await d.waitSel('[data-notice="stale"]');
      t.eq(await d.eval(`document.querySelector('[data-notice="stale"]').textContent`), STALE_PAGE, "the text of the warning of the page");
      t.eq(await d.attr(d.tid("read-only-lock"), "aria-label"), "Read-only store", "the lock of the header");
      t.eq(await d.exists(d.tid("edit-toggle")), false, "no switch Edit");
      await restore();
      const again = await t.open(own.url);
      await again.waitSel(again.tid("row-lab-traffic"));
      t.eq(await again.exists(again.tid("card-stale-lab-traffic")), false, "the mark goes when the file is valid again");
    },
  },

  {
    id: "B243",
    name: "the header selector shows the read-only store as a row with a lock that opens its folders and its dashboards",
    async run(t) {
      const own = await t.ownServer("selector-lab", OWN);
      const p = await t.open(`${own.url}d/nodes`);
      await p.waitSel(p.tid("header-switcher"));
      await p.click(p.tid("header-switcher"));
      const menu = '[data-testid="header-switcher-menu"]';
      await p.waitSel(`${menu} .menu__group`);
      const rows = await p.eval(`[...document.querySelectorAll('${menu} .menu__group')].map((e) => [e.textContent.trim(), !!e.querySelector(".icon:not(.menu__chevron .icon)"), e.getAttribute("aria-expanded")])`);
      t.assert(rows.some(([text]) => text === "Lab"), "a row named Lab: " + JSON.stringify(rows));
      const width = await p.eval(`(() => { const m = document.querySelector('${menu}').getBoundingClientRect(); const b = document.querySelector('[data-testid="header-switcher"]').getBoundingClientRect(); return [Math.round(m.width), Math.round(b.width)]; })()`);
      t.assert(Math.abs(width[0] - width[1]) <= 1 || width[0] >= 224, `the menu has the width of the button or the minimum: ${width}`);
      await p.clickText(`${menu} .menu__group`, "Lab");
      t.eq(await p.eval(`[...document.querySelectorAll('${menu} .menu__group')].map((e) => e.textContent.trim())`).then((x) => x.filter((s) => /^Lab/.test(s)).sort()), ["Lab", "Lab archive", "Lab operations"], "the store opens its two folders");
      await p.clickText(`${menu} .menu__group`, "Lab operations");
      await p.click(p.tid("switcher-item-lab-traffic"));
      await p.waitFor(`location.pathname.endsWith("/d/lab-traffic/lab-traffic")`, { what: "the dashboard of the store opens" });
      await p.waitSel(p.tid("read-only-lock"));
      t.eq(await p.exists(p.tid("read-only-lock")), true, "the lock in the header of the dashboard of the store");
    },
  },

  {
    id: "B244",
    name: "a write to the read-only store gives read_only_store, and its dashboard page has no edit control",
    async run(t) {
      const own = await t.ownServer("refuse-lab", OWN);
      const a = api(own);
      const create = await a.create("lab-ops", seedDocument("new-in-lab", "New in lab"));
      t.eq([create.status, create.json.error.code], [403, "read_only_store"], "create into a folder of the read-only store");
      const item = (await a.dashboard("lab-traffic")).json;
      const save = await a.save("lab-traffic", item.etag, item.document);
      t.eq([save.status, save.json.error.code], [403, "read_only_store"], "save of a dashboard of the read-only store");
      const del = await a.del("lab-traffic", item.etag);
      t.eq([del.status, del.json.error.code], [403, "read_only_store"], "delete of a dashboard of the read-only store");
      const folder = await a.createFolder("lab-ops", "Other");
      t.assert(folder.status === 409 || folder.status === 403, "a folder id of the read-only store is not made again: " + folder.status);
      const made = await a.create("transit", seedDocument("new-in-main", "New in main"));
      t.eq(made.status, 201, "the primary accepts a write");
      const e = await t.open(`${own.url}d/lab-traffic?edit=1`);
      await e.waitSel(e.tid("read-only-lock"));
      await sleep(300);
      t.eq(await e.eval(`["edit-toggle", "edit-bar", "save-button", "add-panel", "json-tab", "edit-grid"].filter((id) => document.querySelector('[data-testid="' + id + '"]'))`), [], "no edit control with edit=1");
      t.eq(await e.exists(e.tid("read-only-lock")), true, "the lock");
    },
  },
];
