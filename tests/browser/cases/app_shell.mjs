// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B272 (9.6): the header is on top and spans the whole window, and the content scrolls in one container under it. The scrollbar of the
// page starts under the header. The document does not scroll, at four sizes, in the light and the dark theme, on the list, on a dashboard, in the view
// of a panel, in the edit mode, in the panel editor and in the JSON view. The row of the variables sticks under the header while the content scrolls,
// and a drag of a panel near the bottom edge scrolls the container.
import { allTypesDoc } from "../lib/docs.mjs";
import { seedDocument } from "../lib/env.mjs";
import { enterEdit, moreItem } from "../lib/ed.mjs";
import { sleep } from "../lib/h.mjs";

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 300)}`);
}

const SIZES = [[1440, 900], [1280, 720], [768, 900], [390, 800]];

const MEASURE = `(() => {
  const html = document.documentElement;
  const header = document.getElementById("app-header").getBoundingClientRect();
  const main = document.getElementById("app-main");
  const mr = main.getBoundingClientRect();
  return {
    win: window.innerWidth, headerW: Math.round(header.width), headerTop: Math.round(header.top), headerBottom: Math.round(header.bottom),
    mainTop: Math.round(mr.top), docSH: html.scrollHeight, docCH: html.clientHeight, bodySH: document.body.scrollHeight, bar: window.innerWidth - html.clientWidth,
    docSW: html.scrollWidth, docCW: html.clientWidth, mainSW: main.scrollWidth, mainCW: main.clientWidth, scrolls: main.scrollHeight > main.clientHeight + 1,
    pos: getComputedStyle(main).position, over: getComputedStyle(main).overflowY,
  };
})()`;

const mouse = (p, type, x, y) => p.send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1 });

export const cases = [
  {
    id: "B272",
    name: "the header spans the window and the content scrolls under it: the scrollbar starts under the header, the document does not scroll, the row of the variables sticks, a drag scrolls the container",
    async run(t) {
      await ensure(t, allTypesDoc("all-types"));
      // Enough small dashboards for the list to scroll in a low window.
      for (let i = 1; i <= 8; i++) await ensure(t, seedDocument("shell-" + i, "Shell " + i));
      const p = await t.page();
      const size = (w, h) => p.send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 1, mobile: false });
      const idle = async () => { await p.idle(); await sleep(300); };

      const pages = [
        ["the list", async () => { await p.goto(t.url("")); await p.waitSel(".card"); }],
        ["a dashboard", async () => { await p.goto(t.dash("nodes", "refresh=off")); await p.waitFor(`document.querySelectorAll('[data-testid^="panel-"]').length > 0`, { what: "panels" }); }],
        ["the view of a panel", async () => { await p.goto(t.dash("nodes", "viewPanel=1&refresh=off")); await p.waitSel(".panel--view"); }],
        ["the edit mode", async () => { await p.goto(t.dash("nodes", "edit=1&refresh=off")); await p.waitSel(".edit-grid"); }],
        ["the panel editor", async () => { await p.goto(t.dash("nodes", "editPanel=1&refresh=off")); await p.waitSel(".peditor"); }],
        ["the JSON view", async () => { await p.goto(t.dash("nodes", "refresh=off")); await p.waitSel(p.tid("edit-toggle")); await enterEdit(p); await moreItem(p, "json-tab"); await p.waitSel(".jeditor"); }],
      ];

      for (const [width, height] of SIZES) {
        for (const scheme of ["light", "dark"]) {
          await size(width, height);
          await p.emulateColorScheme(scheme);
          for (const [name, open] of pages) {
            const where = `${name} at ${width}x${height}, ${scheme}`;
            await open();
            await idle();
            const m = await p.eval(MEASURE);
            // (a) The header has the width of the window: no scrollbar beside it. It is at the top.
            t.eq([m.headerW, m.headerTop], [m.win, 0], `${where}: the header spans the window and is at the top`);
            // (b) The container starts under the header, so its scrollbar starts there.
            t.eq(m.mainTop, m.headerBottom, `${where}: the content starts under the header`);
            t.eq([m.pos, m.over], ["relative", "auto"], `${where}: #app-main is the positioned scroll container`);
            // (c) The document has no scrollbar and nothing makes it taller than the window.
            t.assert(m.docSH <= m.docCH && m.bodySH <= m.docCH, `${where}: the document does not scroll: ${m.docSH}, ${m.bodySH}, ${m.docCH}`);
            t.eq(m.bar, 0, `${where}: the document has no scrollbar`);
            t.assert(m.docSW <= m.docCW && m.mainSW <= m.mainCW, `${where}: no horizontal scroll: ${m.docSW}/${m.docCW}, ${m.mainSW}/${m.mainCW}`);
          }
        }
      }

      // The list scrolls inside the container when the window is low, and the header keeps its place.
      await size(390, 360);
      await p.goto(t.url(""));
      await p.waitSel(".card");
      await idle();
      const low = await p.eval(`(() => { const main = document.getElementById("app-main"); main.scrollTop = 120; return { st: main.scrollTop, doc: document.documentElement.scrollTop, headerTop: Math.round(document.getElementById("app-header").getBoundingClientRect().top), scrolls: main.scrollHeight > main.clientHeight }; })()`);
      t.eq([low.scrolls, low.st > 0, low.doc, low.headerTop], [true, true, 0, 0], "the list scrolls in the container: the document stays at 0 and the header stays at the top");

      // (d) The row of the variables sticks under the header, at the same place at the top and after a scroll.
      await size(1440, 420);
      await p.goto(t.dash("nodes", "refresh=off"));
      await p.waitFor(`!!document.querySelector(".dash-toolbar") && document.querySelectorAll('[data-testid^="panel-"]').length > 0`, { what: "the row of the variables" });
      await idle();
      const stick = async () => p.eval(`(() => {
        const main = document.getElementById("app-main");
        const row = document.querySelector(".dash-toolbar").getBoundingClientRect();
        const first = document.querySelector('[data-testid^="panel-"]').getBoundingClientRect();
        return { header: Math.round(document.getElementById("app-header").getBoundingClientRect().bottom), rowTop: Math.round(row.top), rowBottom: Math.round(row.bottom), firstTop: Math.round(first.top), st: main.scrollTop, max: main.scrollHeight - main.clientHeight };
      })()`);
      const top = await stick();
      t.eq(top.rowTop, top.header, "the row of the variables is under the header at the top");
      t.assert(top.firstTop >= top.rowBottom, `the first panel is under the row: ${top.firstTop} and ${top.rowBottom}`);
      t.assert(top.max > 100, `the page is taller than the window: ${top.max}`);
      await p.eval(`document.getElementById("app-main").scrollTop = 100`);
      await sleep(300);
      const scrolled = await stick();
      t.eq(scrolled.rowTop, scrolled.header, "the row of the variables stays under the header after a scroll");
      t.assert(scrolled.firstTop < top.firstTop, `the panels moved up: ${scrolled.firstTop} and ${top.firstTop}`);

      // (f) A drag of a panel near the bottom edge of the container scrolls the container, not the document.
      await size(1280, 520);
      await p.goto(t.dash("all-types", "edit=1&refresh=off"));
      await p.waitSel(".edit-grid");
      await idle();
      const head = await p.eval(`(() => { const r = document.querySelector('.edit-grid [data-testid^="panel-"] .panel__head').getBoundingClientRect(); return [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)]; })()`);
      await p.eval(`document.getElementById("app-main").scrollTop = 0`);
      await mouse(p, "mouseMoved", head[0], head[1]);
      await mouse(p, "mousePressed", head[0], head[1]);
      for (let i = 1; i <= 8; i++) { await mouse(p, "mouseMoved", head[0], head[1] + ((500 - head[1]) * i) / 8); await sleep(30); }
      await sleep(900);
      const dragged = await p.eval(`({ st: document.getElementById("app-main").scrollTop, doc: document.documentElement.scrollTop })`);
      await mouse(p, "mouseReleased", head[0], 500);
      t.assert(dragged.st > 30, `the drag near the bottom edge scrolled the container: ${dragged.st}`);
      t.eq(dragged.doc, 0, "the document did not scroll");
    },
  },
];
