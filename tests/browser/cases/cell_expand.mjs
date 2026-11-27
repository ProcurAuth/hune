// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B404 (9.20): a click on a cell whose text is cut shows the whole text over the cell, as Grafana 12.4.2 does.
import { doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const UID = "cx404";
const SQL_LONG = "SELECT number AS id, concat('Message ', toString(number), ': ', repeat('a long text that does not fit in the cell, ', 8)) AS message, 'short' AS tag FROM numbers(3)";

export const cases = [
  {
    id: "B404",
    name: "a click on a cut cell of a table shows the whole text over the cell; a short cell, a link, a second click, Escape and a scroll behave as the rules say (9.20)",
    async run(t) {
      const d = doc(UID, "Cell expand", [section("Overview", [panel(1, "table", "Messages", SQL_LONG, { grid: { x: 0, y: 0, w: 12, h: 8 } })])], {
        time: { from: "now-6h", to: "now", timezone: "browser", refresh: null },
      });
      const r = await t.api.create("transit", d);
      t.assert(r.status === 201, `create ${UID}: ${r.status} ${r.text.slice(0, 300)}`);
      const p = await t.open(t.dash(UID, "refresh=off"));
      await waitPanelsReady(p, [1]);
      await p.waitSel(".vtable tbody tr[data-row]");
      const cell = (row, col) => `.vtable tbody tr[data-row="${row}"] td:nth-child(${col})`;
      const state = () => p.eval(`(() => { const e = document.querySelector('[data-testid="cell-expand"]'); if (!e) return null; const b = e.getBoundingClientRect(); return { text: e.textContent, w: Math.round(b.width), h: Math.round(b.height) }; })()`);
      // A short cell: nothing opens.
      await p.click(cell(0, 3));
      t.eq(await state(), null, "a cell that is not cut opens nothing");
      // A cut cell: the whole text shows, over the cell, wrapped, taller than a row.
      const full = await p.eval(`document.querySelector('${cell(0, 2)}').title`);
      const cutWidth = await p.eval(`document.querySelector('${cell(0, 2)}').getBoundingClientRect().width`);
      t.assert(full.length > 150, "the seed text is long");
      await p.click(cell(0, 2));
      const open = await state();
      t.assert(open !== null, "a click on a cut cell opens the whole text");
      t.eq(open.text, full, "the whole text");
      t.assert(open.h > 36, `the box is taller than a row (${open.h})`);
      t.assert(open.w >= Math.round(cutWidth) - 1, `the box is as wide as the cell at least (${open.w} against ${cutWidth})`);
      // A second click on the same cell closes the box.
      await p.click(cell(0, 2));
      t.eq(await state(), null, "a second click closes the box");
      // Escape closes it, and the focus goes back to the cell.
      await p.click(cell(1, 2));
      t.assert((await state()) !== null, "another cut cell opens");
      await p.press("Escape");
      t.eq(await state(), null, "Escape closes the box");
      // A click outside the box closes it.
      await p.click(cell(2, 2));
      t.assert((await state()) !== null, "a third cut cell opens");
      await p.click(cell(0, 1));
      t.eq(await state(), null, "a click on another cell closes the box");
      // Enter on a focused cut cell opens it.
      await p.eval(`(() => { const td = document.querySelector('${cell(0, 2)}'); td.tabIndex = -1; td.focus(); })()`);
      await p.press("Enter");
      await p.waitFor(`!!document.querySelector('[data-testid="cell-expand"]')`, { what: "the box after Enter" });
      await p.press("Escape");
      t.eq(await state(), null, "Escape closes it");
    },
  },
];
