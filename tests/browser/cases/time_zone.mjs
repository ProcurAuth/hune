// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B405 (9.9, 9.5): the time picker has a list of time zones next to "Apply time range". A choice changes the zone of the fields and the
// calendar, and Apply gives the zone to the page: the address names it, the page builds again with it, and the range keeps its instants.
import { SEED_FROM, SEED_TO } from "../lib/env.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

export const cases = [
  {
    id: "B405",
    name: "the time picker has a list of time zones next to Apply: the fields show the instants in the zone, and Apply sets the zone of the page and of the address",
    async run(t) {
      const url = t.url(`d/nodes?from=${SEED_FROM}&to=${SEED_TO}&refresh=off`);
      const p = await t.open(url);
      await waitPanelsReady(p, [1, 2, 3]);
      await p.click(p.tid("time-picker-button"));
      await p.waitSel(p.tid("time-zone"));
      // The list is next to the button Apply, on one row.
      const row = await p.eval(`(() => { const s = document.querySelector('[data-testid="time-zone"]').getBoundingClientRect(); const a = document.querySelector('[data-testid="time-apply"]').getBoundingClientRect(); return [Math.abs((s.top + s.height / 2) - (a.top + a.height / 2)) < 8, s.right <= a.left + 1]; })()`);
      t.eq(row, [true, true], "the zone list is on the row of the button Apply, before it");
      // The list holds the default zones, the offsets and the zone names, after the first use.
      await p.eval(`document.querySelector('[data-testid="time-zone"]').dispatchEvent(new Event("pointerdown"))`);
      const options = await p.eval(`[...document.querySelectorAll('[data-testid="time-zone"] option')].map((o) => o.value)`);
      t.assert(options.includes("browser") && options.includes("utc") && options.includes("Etc/GMT-2") && options.includes("Europe/Paris"), "the list has browser, utc, an offset and a zone name: " + options.length + " options");
      t.assert(options.length > 40, "the list has many zones: " + options.length);
      // The choice UTC+02:00: both fields show the same instants two hours after UTC.
      await p.select(p.tid("time-zone"), "Etc/GMT-2");
      const utcFrom = new Date(SEED_FROM).toISOString().slice(0, 19).replace("T", " ");
      const plus2 = new Date(SEED_FROM + 2 * 3600000).toISOString().slice(0, 19).replace("T", " ");
      t.eq(await p.eval(`document.querySelector('[data-testid="time-from"]').value`), plus2, `the From field shows ${utcFrom} UTC as ${plus2} in UTC+02:00`);
      t.assert((await p.text(p.tid("time-zone-text"))).includes("UTC+02:00"), "the footer says UTC+02:00");
      // A second choice, UTC: the field goes back two hours.
      await p.select(p.tid("time-zone"), "utc");
      t.eq(await p.eval(`document.querySelector('[data-testid="time-from"]').value`), utcFrom, "the From field in UTC");
      // Apply with the zone UTC+02:00 sets the zone of the page and the address.
      await p.select(p.tid("time-zone"), "Etc/GMT-2");
      await p.click(p.tid("time-apply"));
      await p.waitFor(`new URLSearchParams(location.search).get("timezone") === "Etc/GMT-2"`, { what: "the zone in the address" });
      await waitPanelsReady(p, [1, 2, 3]);
      const q = new URL(await p.eval("location.href")).searchParams;
      t.eq([q.get("from"), q.get("to")], [String(SEED_FROM), String(SEED_TO)], "the range keeps its instants");
      await p.click(p.tid("time-picker-button"));
      await p.waitSel(p.tid("time-zone"));
      t.eq(await p.eval(`document.querySelector('[data-testid="time-zone"]').value`), "Etc/GMT-2", "the list shows the zone of the page");
      t.eq(await p.eval(`document.querySelector('[data-testid="time-from"]').value`), plus2, "the field shows the zone of the page");
      t.assert((await p.eval(`document.querySelector('[data-testid="time-picker-button"]').getAttribute("data-tooltip")`)).includes("UTC+02:00"), "the tooltip of the range button names the zone");
      await p.press("Escape");
    },
  },
];
