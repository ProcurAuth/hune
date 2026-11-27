// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B401 to B403 (9.9, 9.7): a control with nothing to do does not show.
// B401: a file with no primary store (`storage.read_only`), no history and no export: the dashboard page has no button "More actions" above
//       900 px, and the list page has no button "New" and no menu on a card. At 900 px or less the button shows with the time actions only,
//       and it follows the width of the window with no reload.
// B402: the same file with `server.export = true`: the button shows with "Export" only, at every width.
// B403: a store with a primary, with the history and the export on: a dashboard of a read-only store has the button with "Export", and
//       a dashboard of the primary store has the button with its full menu.
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { api } from "../lib/env.mjs";
import { sleep } from "../lib/h.mjs";

const UID = "ma401";

function oneDoc(uid, title) {
  return doc(uid, title, [section("Overview", [panel(1, "stat", "Stat", SQL.stat ?? "SELECT 7 AS value", { grid: { x: 0, y: 0, w: 6, h: 4 } })])], {
    time: { from: "now-6h", to: "now", timezone: "browser", refresh: null },
  });
}

const prepare = (volume) => writeFileSync(join(volume, "transit", `${UID}.json`), JSON.stringify(oneDoc(UID, "More actions")));

async function openAt(t, url, width) {
  const height = width === 390 ? 844 : 900;
  const p = await t.page({ width, height });
  await p.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await p.goto(url);
  await p.idle();
  await sleep(150);
  return p;
}

// The page has its header controls: the time control is on the dashboard page, and a card is on the list page.
// A network change of the host (the container of ClickHouse that starts) can break the first load of the first case: the page loads again once.
async function waitDashboard(p) {
  const ready = `!!document.querySelector('[data-testid="time-control"]') && !!document.querySelector('[data-testid="panel-1"]')`;
  try {
    await p.waitFor(ready, { what: "the dashboard page", timeout: 8000 });
  } catch (error) {
    await p.reload();
    await p.waitFor(ready, { what: "the dashboard page after a reload" });
  }
}
const waitList = (p) => p.waitFor(`!!document.querySelector('[data-testid="list"] article[data-testid^="row-"]')`, { what: "the list" });
const menuItems = (p) => p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim())`);
// The button counts only when the user can see it: the slot of an empty menu has no box.
const moreShown = (p) => p.eval(`(() => { const e = document.querySelector('[data-testid="dashboard-menu"]'); return !!e && e.offsetParent !== null && e.getBoundingClientRect().width > 0; })()`);

// The button New of the list page shows when it has a box: a hidden button has none.
const newShown = (p) => p.eval(`(() => { const e = document.querySelector('[data-testid="list-new-menu"]'); return !!e && e.offsetParent !== null && e.getBoundingClientRect().width > 0; })()`);

export const cases = [
  {
    id: "B401",
    name: "a file with no primary store, no history and no export: no button More actions above 900 px, the time actions only at 900 px or less, and no button New and no menu on the list page",
    async run(t) {
      const s = await t.ownServer("ro-none", { readOnlyStore: true, export: false, prepare });
      const found = [];
      for (const width of [1920, 1280, 901]) {
        const p = await openAt(t, `${s.url}d/${UID}?${t.range}`, width);
        await waitDashboard(p);
        if (await moreShown(p)) found.push(`${width}: the button More actions shows with no action`);
        if (await p.exists(p.tid("edit-toggle"))) found.push(`${width}: the switch Edit shows`);
        if (!(await p.exists(p.tid("read-only-lock")))) found.push(`${width}: the lock of a read-only store is missing`);
        const l = await openAt(t, s.url, width);
        await waitList(l);
        if (await newShown(l)) found.push(`${width}: the list has the button New`);
        const cardButtons = await l.count('article[data-testid^="row-"] button:not(.list-tag), .card__more, .group__menu');
        if (cardButtons !== 0) found.push(`${width}: the list has ${cardButtons} menu buttons on its cards`);
      }
      // At 900 px or less the time actions are in the menu: the button shows, and the menu has these three items only.
      for (const width of [900, 390]) {
        const p = await openAt(t, `${s.url}d/${UID}?${t.range}`, width);
        await waitDashboard(p);
        if (!(await moreShown(p))) found.push(`${width}: the button More actions is missing for the time actions`);
        else {
          await p.click(p.tid("dashboard-menu"));
          await p.waitSel(".menu");
          t.eq(await menuItems(p), ["Shift the time range back", "Shift the time range forward", "Zoom out"], `${width}: the menu has the time actions only`);
          await p.press("Escape");
        }
      }
      // The width changes with no reload: the button follows it.
      const p = await openAt(t, `${s.url}d/${UID}?${t.range}`, 1280);
      await waitDashboard(p);
      const steps = [];
      for (const width of [1280, 800, 1280, 500, 1000]) {
        await p.send("Emulation.setDeviceMetricsOverride", { width, height: 900, deviceScaleFactor: 1, mobile: false });
        await p.waitFor(`window.innerWidth === ${width}`, { what: `the width ${width}` });
        await sleep(100);
        steps.push([width, await moreShown(p)]);
      }
      t.eq(steps, [[1280, false], [800, true], [1280, false], [500, true], [1000, false]], "the button follows the width of the window");
      t.eq(found, [], "B401");
    },
  },

  {
    id: "B402",
    name: "a file with no primary store and the export on: the button More actions shows with Export only, and the list page has no button on a card",
    async run(t) {
      const s = await t.ownServer("ro-export", { readOnlyStore: true, export: true, prepare });
      for (const width of [1920, 1280]) {
        const p = await openAt(t, `${s.url}d/${UID}?${t.range}`, width);
        await waitDashboard(p);
        t.eq(await moreShown(p), true, `${width}: the button shows`);
        await p.click(p.tid("dashboard-menu"));
        await p.waitSel(".menu");
        t.eq(await menuItems(p), ["Export"], `${width}: the menu has Export only`);
        await p.press("Escape");
      }
      const l = await openAt(t, s.url, 1280);
      await waitList(l);
      t.eq(await l.count('article[data-testid^="row-"] button:not(.list-tag), .card__more'), 0, "a card of a read-only store has no button");
      t.eq(await newShown(l), false, "no button New");
    },
  },

  {
    id: "B403",
    name: "a store with a primary, the history and the export on: a dashboard of a read-only store has the button with Export, and a dashboard of the primary store has the full menu",
    async run(t) {
      const s = await t.ownServer("ro-primary", { editing: false, prepare });
      const folder = await api(s).createFolder("work", "Work");
      t.assert(folder.status === 201, `create the folder work: ${folder.status} ${folder.text.slice(0, 300)}`);
      const r = await api(s).create("work", oneDoc("ma403", "Primary dashboard"));
      t.assert(r.status === 201, `create ma403: ${r.status} ${r.text.slice(0, 300)}`);
      for (const width of [1920, 1280]) {
        const ro = await openAt(t, `${s.url}d/${UID}?${t.range}`, width);
        await waitDashboard(ro);
        t.eq(await moreShown(ro), true, `${width}: the read-only dashboard has the button`);
        await ro.click(ro.tid("dashboard-menu"));
        await ro.waitSel(".menu");
        t.eq(await menuItems(ro), ["Export"], `${width}: the menu of a read-only dashboard`);
        await ro.press("Escape");
        const pr = await openAt(t, `${s.url}d/ma403?${t.range}`, width);
        await waitDashboard(pr);
        t.eq(await moreShown(pr), true, `${width}: the dashboard of the primary store has the button`);
        await pr.click(pr.tid("dashboard-menu"));
        await pr.waitSel(".menu");
        t.eq(await menuItems(pr), ["History", "Export", "Delete"], `${width}: the menu of the primary store`);
        await pr.press("Escape");
      }
    },
  },
];
