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
    name: "list cards: the filters show above the threshold, the folder titles show for two folders, the grid has columns of 400 px and is centred, a writable card has the three dots at the top right, after the badge, with Edit, Export, Move and Delete in their menu, a read-only card has none, the badge of the last change shows only for a recorded change, the dropdowns are joined to their button",
    async run(t) {
      // ---- the default threshold (6): three dashboards show no sort and no tag filter.
      const own = await t.ownServer("default", { prepare: TWO });
      const p = await t.open(own.url);
      await size(p, 1280, 900);
      await cardsShown(p);
      t.eq(await hidden(p, ["list-sort", "list-tag-filter"]), [true, true], "3 dashboards, default threshold 6: no sort, no tag filter");
      // The page has one h1 for assistive technology, hidden with the class visually-hidden: no title shows on the screen.
      t.eq(await p.eval(`[...document.querySelectorAll("h1")].filter((e) => !e.classList.contains("visually-hidden") && e.getBoundingClientRect().width > 1 && e.getBoundingClientRect().height > 1).length`), 0, "no page title shows");
      t.eq(await p.eval(`document.querySelectorAll("h1.visually-hidden").length`), 1, "one h1 for assistive technology");
      t.assert(!/Can edit|Read only|dashboards in|\d+ dashboards/.test(await p.eval("document.body.textContent")), "no access word and no counter on the page");

      // The folder titles: two folders, two titles, in capitals.
      t.eq(await p.eval(`[...document.querySelectorAll(".group__head .group__title")].map((e) => e.textContent)`), ["Alpha", "Beta"], "two folders: two folder titles");
      t.eq(await p.eval(`getComputedStyle(document.querySelector(".group__head")).textTransform`), "uppercase", "the folder title is in capitals");
      t.eq(await p.count(".group__menu"), 2, "the user can write in both folders: a folder menu on each title");
      await p.click(`${p.tid("folder-alpha")} .group__menu`);
      t.eq(await menuItems(p), ["Rename", "Delete"], "the menu of a folder: Rename and Delete");
      await p.press("Escape");

      // The cards: columns of 400 px, the title on one line with an ellipsis and a title attribute, and no badge for a file of the volume.
      const geo = await p.eval(`(() => {
        const cards = [...document.querySelectorAll('article[data-testid^="row-"]')];
        const link = document.querySelector('[data-testid="row-a2"] .card__link');
        const s = getComputedStyle(link);
        return {
          widths: cards.map((c) => Math.round(c.getBoundingClientRect().width)),
          ellipsis: [s.textOverflow, s.whiteSpace, link.scrollWidth > link.clientWidth, link.title],
          badges: cards.map((c) => c.querySelectorAll(".card__badge").length),
          heights: cards.map((c) => Math.round(c.getBoundingClientRect().height)),
        };
      })()`);
      t.eq(geo.widths, [400, 400, 400], "each card is 400 px wide");
      t.eq(geo.ellipsis, ["ellipsis", "nowrap", true, LONG], "the title is on one line, with an ellipsis and the full name in the title attribute");
      t.eq(geo.badges, [0, 0, 0], "a file of the volume has no version that Hune saved: no badge of the last change (the date of a file is not a change)");
      t.eq(new Set(geo.heights).size, 1, "the three cards have one height");

      // The block of cards is centred in the window, across and down, when it fits.
      const centre = await p.eval(`(() => { const s = document.querySelector(".list-stack").getBoundingClientRect(); return [Math.abs((s.left + s.right) / 2 - document.documentElement.clientWidth / 2), Math.abs((s.top + s.bottom) / 2 - (44 + window.innerHeight) / 2), document.getElementById("app-main").scrollHeight - document.getElementById("app-main").clientHeight]; })()`);
      t.assert(centre[0] <= 2, "centred across: " + centre[0]);
      t.assert(centre[1] <= 2, "centred down: " + centre[1]);
      t.eq(centre[2], 0, "the page does not scroll when the block fits");

      // The three dots of a writable card: one button at the right end of the head row, after the title (and the badge), and no other button.
      const dots = await p.eval(`(() => { const card = document.querySelector('[data-testid="row-a1"]').getBoundingClientRect(); const m = document.querySelector('[data-testid="row-menu-a1"]').getBoundingClientRect(); const title = document.querySelector('[data-testid="row-a1"] .card__title').getBoundingClientRect(); return [Math.round(card.right - m.right), Math.round(m.top - card.top), m.left >= title.right - 0.5, Math.round(m.height), document.querySelectorAll('[data-testid="row-a1"] button:not(.list-tag):not([data-testid^="row-menu-"])').length, document.querySelectorAll('[data-testid="row-a1"] .card__buttons, [data-testid="row-a1"] .card__edit, [data-testid="row-a1"] .card__delete').length]; })()`);
      t.assert(dots[0] >= 4 && dots[0] <= 14 && dots[1] >= 6 && dots[1] <= 16, "the three dots are at the top right of the card: " + JSON.stringify(dots));
      t.eq([dots[2], dots[3], dots[4], dots[5]], [true, 24, 0, 0], "the dots are after the title, 24 px high, and the card has no pencil, no bin and no other button");
      await p.click(p.tid("row-menu-a1"));
      t.eq(await menuItems(p), ["Edit", "Export", "Move", "Delete"], "the menu of a card holds Edit, Export, Move and Delete in this order (Move is not in the menu of the dashboard page)");
      t.eq(await p.eval(`document.querySelector('[data-testid="card-edit-a1"]').getAttribute("href")`), "d/a1/first-dashboard?edit=1", "Edit opens the dashboard in the edit mode");
      t.eq(await p.eval(`document.querySelector('[data-testid="card-delete-a1"]').classList.contains("menu__item--danger")`), true, "Delete has the danger colour");
      await p.press("Escape");
      // Delete runs the delete dialog, which names the dashboard. The server decides: nothing is deleted here.
      await p.click(p.tid("row-menu-a1"));
      await p.click(p.tid("card-delete-a1"));
      await p.waitSel(p.tid("delete-dashboard-dialog"));
      t.assert((await p.text(p.tid("delete-dashboard-dialog"))).includes("First dashboard"), "the dialog names the dashboard");
      await p.press("Escape");
      t.eq((await api(own).list()).json.items.length, 3, "the dashboards are still there");
      // Edit opens the edit mode.
      await p.click(p.tid("row-menu-a1"));
      await p.click(p.tid("card-edit-a1"));
      await p.waitFor(`location.pathname === "/d/a1/first-dashboard" && location.search === "?edit=1"`, { what: "the edit address" });
      await p.waitFor(`document.querySelector('[data-testid="edit-toggle"]') && document.querySelector('[data-testid="edit-toggle"]').getAttribute("aria-checked") === "true"`, { what: "the edit mode" });

      // A window that is shorter than the block: it starts at the top and the page scrolls.
      const q = await t.open(own.url);
      await size(q, 1280, 260);
      await cardsShown(q);
      const tall = await q.eval(`(() => { const s = document.querySelector(".list-stack").getBoundingClientRect(); return [s.top < 80, document.getElementById("app-main").scrollHeight > document.getElementById("app-main").clientHeight]; })()`);
      t.eq(tall, [true, true], "taller than the window: the block starts at the top and the page scrolls");
      // A phone: one column of the full width.
      await size(q, 390, 844);
      const phone = await q.eval(`(() => { const cards = [...document.querySelectorAll('article[data-testid^="row-"]')].map((c) => c.getBoundingClientRect()); return [new Set(cards.map((c) => Math.round(c.left))).size, Math.max(...cards.map((c) => c.right)) <= 390, document.getElementById("app-main").scrollWidth <= 390 && document.documentElement.scrollWidth <= 390]; })()`);
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
      // The sort and the tag filter use the joined menu of the app: it touches its button, which loses its bottom edge, and it opens with the keys.
      t.eq(await pb.eval(`document.querySelector('[data-testid="list-sort"]').textContent.trim()`), "A–Z", "no saved choice: the button of the sort shows A–Z, not a placeholder");
      await pb.click(pb.tid("list-sort"));
      const joined = await pb.eval(`(() => { const button = document.querySelector('[data-testid="list-sort"]'); const b = button.getBoundingClientRect(); const menu = document.querySelector('[data-testid="list-sort-menu"]'); const r = menu.getBoundingClientRect(); return [menu.classList.contains("menu--joined"), button.classList.contains("is-joined-open"), button.getAttribute("aria-expanded"), Math.abs(r.top - (b.bottom - 1)) <= 1.5, Math.abs(r.left - b.left) <= 1.5, r.width >= b.width - 1.5, getComputedStyle(button).borderBottomColor === "rgba(0, 0, 0, 0)", menu.querySelectorAll(".menu__item").length]; })()`);
      t.eq(joined, [true, true, "true", true, true, true, true, 2], "the sort menu is joined to its button");
      await pb.click(pb.tid("list-sort"));
      t.eq(await pb.eval(`document.querySelector('[data-testid="list-sort-menu"]')`), null, "a second click on the button closes the menu");
      await pb.focus(pb.tid("list-sort"));
      await pb.press("ArrowDown");
      t.eq(await pb.eval(`document.activeElement.closest('[data-testid="list-sort-menu"]') !== null`), true, "the key Down on the button opens the menu with the focus on its first item");
      await pb.press("Enter");
      t.eq(await pb.eval(`document.querySelector('[data-testid="list-sort"]').textContent.trim()`), "A–Z", "Enter chooses the item, and the button shows it in short");
      await pb.click(pb.tid("list-sort"));
      t.eq(await pb.eval(`document.querySelectorAll('[data-testid="list-sort-menu"] .menu__check').length`), 1, "the chosen line has a check mark");
      await pb.press("Escape");
      t.eq(await pb.eval(`document.activeElement && document.activeElement.dataset.testid`), "list-sort", "Escape closes the menu and gives the focus back to the button");
      await pb.click(pb.tid("list-tag-filter"));
      const tagMenu = await pb.eval(`(() => { const items = [...document.querySelectorAll('[data-testid="list-tag-menu"] .menu__item')]; return [document.querySelector('[data-testid="list-tag-menu"]').classList.contains("menu--joined"), items.length > 0, items.every((e) => e.querySelector(".list-tag") && /^\\d+$/.test(e.querySelector(".menu__meta").textContent))]; })()`);
      t.eq(tagMenu, [true, true, true], "the tag menu: one chip and one count on each line, joined to its button");
      await pb.press("Escape");

      // The badge of the last change: a dashboard that Hune saved has it, at the top right, and the cards keep one height.
      const rec = await t.ownServer("recorded", { prepare: TWO, serverLines: ["list_filters_min_dashboards = 0"] });
      const made = await api(rec).create("alpha", seedDocument("a3", "Saved by Hune"));
      t.eq(made.status, 201, "the dashboard is created through the API");
      const pg = await t.open(rec.url);
      await size(pg, 1280, 900);
      await pg.waitFor(`document.querySelectorAll('article[data-testid^="row-"]').length === 4`, { what: "four cards" });
      const badge = await pg.eval(`(() => { const cards = [...document.querySelectorAll('article[data-testid^="row-"]')]; const card = document.querySelector('[data-testid="row-a3"]'); const b = card.querySelector(".card__badge"); const r = b ? b.getBoundingClientRect() : null; const c = card.getBoundingClientRect(); return { all: cards.map((e) => e.querySelectorAll(".card__badge").length), text: b ? b.textContent : null, title: b ? b.title : null, right: r ? Math.round(c.right - r.right) : null, top: r ? Math.round(r.top - c.top) : null, heights: [...new Set(cards.map((e) => Math.round(e.getBoundingClientRect().height)))] }; })()`);
      t.eq(badge.all.reduce((a, b) => a + b, 0), 1, "one badge: only the dashboard that Hune saved");
      t.assert(/^Last change (now|\d+m)$/.test(badge.text), "the text of the badge: " + badge.text);
      t.assert(/^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d$/.test(badge.title), "the full time is in the title attribute: " + badge.title);
      // The three dots take the right end of the head row, so the badge is at their left (a gap of 8 px between them).
      t.assert(badge.right >= 38 && badge.right <= 48 && badge.top >= 12 && badge.top <= 16, "the badge is at the top right, before the three dots: " + JSON.stringify([badge.right, badge.top]));
      t.eq(await pg.eval(`(() => { const card = document.querySelector('[data-testid="row-a3"]'); const b = card.querySelector(".card__badge").getBoundingClientRect(); const m = card.querySelector('[data-testid="row-menu-a3"]').getBoundingClientRect(); return [b.right <= m.left + 0.5, Math.abs((b.top + b.bottom) / 2 - (m.top + m.bottom) / 2) <= 6]; })()`), [true, true], "the three dots are after the badge, on the same row");
      t.eq(badge.heights.length, 1, "a card with the badge and a card without it have one height: " + JSON.stringify(badge.heights));

      // The text q of the address has no effect while the filters are hidden.
      const pq = await t.open(`${equal.url}?q=chain`);
      await cardsShown(pq);
      t.eq(await pq.count('article[data-testid^="row-"]'), 3, "q has no effect when the filters are hidden");

      // ---- a read-only user: no button on a card, and no access word.
      const ro = await t.ownServer("readonly", { prepare: TWO, editing: false, serverLines: ["list_filters_min_dashboards = 0"] });
      const pr = await t.open(ro.url);
      await size(pr, 1280, 900);
      await cardsShown(pr);
      t.eq(await pr.count('.card__more, [data-testid^="row-menu-"], [data-testid^="card-edit-"], [data-testid^="card-delete-"]'), 0, "a read-only card has no three dots");
      t.eq(await pr.count("article[data-testid^=\"row-\"] button:not(.list-tag)"), 0, "a read-only card has no button but its tag chips");
      t.assert(!/Can edit|Read only/.test(await pr.eval("document.body.textContent")), "no access word on the page");
    },
  },
];
