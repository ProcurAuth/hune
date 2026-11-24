// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B228 (9.7): the cards of the list page.
// Each server of the case has its own volume with two folders and three dashboards. The case checks the threshold of the filters, the folder
// titles, the grid of 400 px, the centring, the buttons of a card and the card of a read-only user.
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { api, seedDocument } from "../lib/env.mjs";
import { sleep } from "../lib/h.mjs";

const LONG = "A dashboard with a very long title that cannot fit on one line of a card of four hundred pixels";

/** A volume `prepare` function: it removes every folder and writes `{ folder: { title, docs: [[uid, title]] } }`. */
function layout(spec) {
  return (volume) => {
    for (const name of readdirSync(volume)) rmSync(join(volume, name), { recursive: true, force: true });
    for (const [folder, { title, docs }] of Object.entries(spec)) {
      mkdirSync(join(volume, folder), { recursive: true });
      writeFileSync(join(volume, folder, "_folder.json"), JSON.stringify({ format_version: 1, title }, null, 2) + "\n");
      for (const [uid, docTitle] of docs) writeFileSync(join(volume, folder, `${uid}.json`), JSON.stringify(seedDocument(uid, docTitle), null, 2) + "\n");
    }
  };
}

const TWO = layout({
  alpha: { title: "Alpha", docs: [["a1", "First dashboard"], ["a2", LONG]] },
  beta: { title: "Beta", docs: [["b1", "Third dashboard"]] },
});

async function size(p, width, height = 900) {
  await p.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await sleep(250);
}

const cardsShown = (p) => p.waitFor(`document.querySelectorAll('article[data-testid^="row-"]').length === 3`, { what: "three cards" });
/** True for each test id whose control is not visible. */
const hidden = (p, ids) => p.eval(`${JSON.stringify(ids)}.map((id) => { const e = document.querySelector('[data-testid="' + id + '"]'); return !e || e.offsetParent === null; })`);
const menuItems = (p) => p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`);

export const cases = [
  {
    id: "B228",
    name: "list cards: the filters show above the threshold, the folder titles show for two folders, the grid has columns of 400 px and is centred, a writable card has a pencil, a menu and a bin, a read-only card has none",
    async run(t) {
      // ---- the default threshold (6): three dashboards show no sort and no tag filter.
      const own = await t.ownServer("default", { prepare: TWO });
      const p = await t.open(own.url);
      await size(p, 1280, 900);
      await cardsShown(p);
      t.eq(await hidden(p, ["list-sort", "list-tag-filter"]), [true, true], "3 dashboards, default threshold 6: no sort, no tag filter");
      t.eq(await p.eval(`document.querySelector("h1")`), null, "no page title");
      t.assert(!/Can edit|Read only|dashboards in|\d+ dashboards/.test(await p.eval("document.body.textContent")), "no access word and no counter on the page");

      // The folder titles: two folders, two titles, in capitals.
      t.eq(await p.eval(`[...document.querySelectorAll(".group__head .group__title")].map((e) => e.textContent)`), ["Alpha", "Beta"], "two folders: two folder titles");
      t.eq(await p.eval(`getComputedStyle(document.querySelector(".group__head")).textTransform`), "uppercase", "the folder title is in capitals");
      t.eq(await p.count(".group__menu"), 2, "the user can write in both folders: a folder menu on each title");
      await p.click(`${p.tid("folder-alpha")} .group__menu`);
      t.eq(await menuItems(p), ["Rename", "Delete"], "the menu of a folder: Rename and Delete");
      await p.press("Escape");

      // The cards: columns of 400 px, the title on one line with an ellipsis and a title attribute, the note of the last change.
      const geo = await p.eval(`(() => {
        const cards = [...document.querySelectorAll('article[data-testid^="row-"]')];
        const link = document.querySelector('[data-testid="row-a2"] .card__link');
        const s = getComputedStyle(link);
        return {
          widths: cards.map((c) => Math.round(c.getBoundingClientRect().width)),
          ellipsis: [s.textOverflow, s.whiteSpace, link.scrollWidth > link.clientWidth, link.title],
          notes: cards.map((c) => c.querySelector(".card__updated").textContent),
          noteHeights: cards.map((c) => c.querySelector(".card__updated").getBoundingClientRect().height),
        };
      })()`);
      t.eq(geo.widths, [400, 400, 400], "each card is 400 px wide");
      t.eq(geo.ellipsis, ["ellipsis", "nowrap", true, LONG], "the title is on one line, with an ellipsis and the full name in the title attribute");
      t.assert(geo.notes.every((n) => /^(now|\d+m)$/.test(n)), "a new dashboard has the short note now or Nm: " + JSON.stringify(geo.notes));
      t.eq(new Set(geo.noteHeights).size, 1, "the note has a fixed height");

      // The block of cards is centred in the window, across and down, when it fits.
      const centre = await p.eval(`(() => { const s = document.querySelector(".list-stack").getBoundingClientRect(); return [Math.abs((s.left + s.right) / 2 - document.documentElement.clientWidth / 2), Math.abs((s.top + s.bottom) / 2 - (44 + window.innerHeight) / 2), document.documentElement.scrollHeight - window.innerHeight]; })()`);
      t.assert(centre[0] <= 2, "centred across: " + centre[0]);
      t.assert(centre[1] <= 2, "centred down: " + centre[1]);
      t.eq(centre[2], 0, "the page does not scroll when the block fits");

      // The buttons of a writable card: the pencil at the left, the menu, the bin at the right, in fixed slots.
      const order = await p.eval(`(() => { const x = (id) => document.querySelector('[data-testid="' + id + '"]').getBoundingClientRect(); const a = x("card-edit-a1"), m = x("row-menu-a1"), d = x("card-delete-a1"); const card = x("row-a1"); return [a.right <= m.left, m.right <= d.left, card.right - d.right, d.top - card.top, document.querySelector('[data-testid="card-edit-a1"]').getAttribute("href")]; })()`);
      t.eq([order[0], order[1]], [true, true], "the pencil, the menu and the bin are in this order");
      t.assert(order[2] >= 6 && order[2] <= 12 && order[3] >= 6 && order[3] <= 12, "the buttons are at the top right: " + JSON.stringify(order));
      t.eq(order[4], "d/a1/first-dashboard?edit=1", "the pencil opens the dashboard in the edit mode");
      await p.click(p.tid("row-menu-a1"));
      t.eq(await menuItems(p), ["Export", "Move"], "the menu of a card holds Export and Move (Move is not in the menu of the dashboard page)");
      await p.press("Escape");
      // The bin runs the delete dialog, which names the dashboard. The server decides: nothing is deleted here.
      await p.click(p.tid("card-delete-a1"));
      await p.waitSel(p.tid("delete-dashboard-dialog"));
      t.assert((await p.text(p.tid("delete-dashboard-dialog"))).includes("First dashboard"), "the dialog names the dashboard");
      await p.press("Escape");
      t.eq((await api(own).list()).json.items.length, 3, "the dashboards are still there");
      // The pencil opens the edit mode.
      await p.click(p.tid("card-edit-a1"));
      await p.waitFor(`location.pathname === "/d/a1/first-dashboard" && location.search === "?edit=1"`, { what: "the edit address" });
      await p.waitFor(`document.querySelector('[data-testid="edit-toggle"]') && document.querySelector('[data-testid="edit-toggle"]').getAttribute("aria-checked") === "true"`, { what: "the edit mode" });

      // A window that is shorter than the block: it starts at the top and the page scrolls.
      const q = await t.open(own.url);
      await size(q, 1280, 260);
      await cardsShown(q);
      const tall = await q.eval(`(() => { const s = document.querySelector(".list-stack").getBoundingClientRect(); return [s.top < 80, document.documentElement.scrollHeight > window.innerHeight]; })()`);
      t.eq(tall, [true, true], "taller than the window: the block starts at the top and the page scrolls");
      // A phone: one column of the full width.
      await size(q, 390, 844);
      const phone = await q.eval(`(() => { const cards = [...document.querySelectorAll('article[data-testid^="row-"]')].map((c) => c.getBoundingClientRect()); return [new Set(cards.map((c) => Math.round(c.left))).size, Math.max(...cards.map((c) => c.right)) <= 390, document.documentElement.scrollWidth <= 390]; })()`);
      t.eq(phone, [1, true, true], "390 px: one column, no horizontal scroll");

      // ---- the threshold: the filters show when the count is above it, and not when it is equal.
      const equal = await t.ownServer("equal", { prepare: TWO, serverLines: ["list_filters_min_dashboards = 3"] });
      const pe = await t.open(equal.url);
      await size(pe, 1280, 900);
      await cardsShown(pe);
      t.eq(await hidden(pe, ["list-sort", "list-tag-filter"]), [true, true], "3 dashboards, threshold 3: no filters");
      const below = await t.ownServer("below", { prepare: TWO, serverLines: ["list_filters_min_dashboards = 2"] });
      const pb = await t.open(below.url);
      await size(pb, 1280, 900);
      await cardsShown(pb);
      t.eq(await hidden(pb, ["list-sort", "list-tag-filter"]), [false, false], "3 dashboards, threshold 2: sort and tag filter");
      // The text q of the address has no effect while the filters are hidden.
      const pq = await t.open(`${equal.url}?q=chain`);
      await cardsShown(pq);
      t.eq(await pq.count('article[data-testid^="row-"]'), 3, "q has no effect when the filters are hidden");

      // ---- a read-only user: no button on a card, and no access word.
      const ro = await t.ownServer("readonly", { prepare: TWO, editing: false, serverLines: ["list_filters_min_dashboards = 0"] });
      const pr = await t.open(ro.url);
      await size(pr, 1280, 900);
      await cardsShown(pr);
      t.eq(await pr.count('.card__buttons, .card__edit, [data-testid^="row-menu-"], [data-testid^="card-delete-"]'), 0, "a read-only card has no pencil, no menu and no bin");
      t.eq(await pr.count("article[data-testid^=\"row-\"] button:not(.list-tag)"), 0, "a read-only card has no button but its tag chips");
      t.assert(!/Can edit|Read only/.test(await pr.eval("document.body.textContent")), "no access word on the page");
    },
  },
];
