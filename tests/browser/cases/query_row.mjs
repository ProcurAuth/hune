// Browser case B209 (10.3, owner request of 2026-11-15, ID-K303): the row of the query settings in the tab "Query".
// One row at 1280 px: Source, Format, Name, Min interval, Max points, the button "Options" and "Run" have the same top (2 px).
// At 768 px and 390 px the row wraps and never gives a horizontal scroll. The popover "Query options" holds the choice "Reuse a panel",
// "Relative time", "Time shift" and "Hide time info". It has a dot while one of them is not at its default. The forms still write the members.
// The Tab order is the visual order, each control has a name, and each target is 24 px at least (the rule of B153).
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { draftOf, waitDraft } from "../lib/ed.mjs";
import { sleep } from "../lib/h.mjs";

const UID = "qr209";

/** Waits for two painted frames: the resize callbacks of the page have run (a wait on the frames, not on a fixed time). */
const painted = (p) => p.eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))");

/** Waits until the field `sel` has `aria-invalid="true"`: the check of the field runs after its pause (no fixed wait). */
const invalid = (p, sel, what) => p.waitFor(`document.querySelector(${JSON.stringify(sel)}).getAttribute("aria-invalid") === "true"`, { what });

function rowDoc() {
  const g = (x, y, w, h) => ({ x, y, w, h });
  const d = doc(UID, "Query row", [
    section("Overview", [
      panel(1, "timeseries", "Series", SQL.series, { grid: g(0, 0, 12, 8) }),
      panel(2, "table", "Table with a time override", SQL.table, { grid: g(12, 0, 12, 8) }),
      panel(3, "stat", "Stat", "SELECT 7 AS value", { grid: g(0, 8, 6, 4) }),
      panel(4, "table", "Reused", SQL.table, { grid: g(6, 8, 12, 4), reuse: { panel: 1, with_transforms: true } }),
    ]),
  ]);
  d.sections[0].panels[1].data.query.time_from = "now-1h";
  return d;
}

// The page code: the controls of the row (not the ones inside the popover), in DOM order, with their box and their name.
const ROW = `(() => {
  const bar = document.querySelector('.qarea > .qbar');
  if (!bar) return null;
  const name = (e) => {
    const l = e.id ? document.querySelector('label[for="' + e.id + '"]') : null;
    if (l) return l.textContent.trim();
    if (e.getAttribute('aria-label')) return e.getAttribute('aria-label');
    const g = e.closest('[role="group"][aria-label]');
    return (e.textContent || '').trim() + (g && g !== bar ? ' in ' + g.getAttribute('aria-label') : '');
  };
  const list = [...bar.querySelectorAll('select, input, button')].filter((e) => !e.closest('.qopt__pop') && e.offsetParent !== null).map((e, i) => {
    const b = e.getBoundingClientRect();
    return { i, tag: e.tagName.toLowerCase(), tid: e.dataset.testid || null, name: name(e), top: Math.round(b.top * 10) / 10, left: Math.round(b.left * 10) / 10, right: Math.round(b.right * 10) / 10, w: Math.round(b.width * 10) / 10, h: Math.round(b.height * 10) / 10 };
  });
  const box = bar.getBoundingClientRect();
  return { list, bars: document.querySelectorAll('.qarea > .qbar').length, bar: { top: box.top, height: box.height }, scroll: document.documentElement.scrollWidth, client: document.documentElement.clientWidth, win: innerWidth };
})()`;

const tops = (list) => [...new Set(list.map((c) => Math.round(c.top)))];

async function openAt(t, query, width) {
  const height = width === 390 ? 844 : 900;
  const p = await t.page({ width, height });
  await p.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await p.goto(t.dash(UID, query));
  await p.idle();
  await p.waitSel(p.tid("panel-editor"));
  await p.waitSel(p.tid("query-options"));
  await p.idle();
  await painted(p);
  return p;
}

const panelOf = async (p, id) => {
  const d = await draftOf(p, UID);
  for (const s of d.document.sections) for (const x of s.panels) if (x.id === id) return x;
  return null;
};

const expanded = (p) => p.attr(p.tid("query-options"), "aria-expanded");
const dotOn = (p) => p.attr(p.tid("query-options"), "data-set");

export const cases = [
  {
    id: "B209",
    name: "the row of the query settings: one row at 1280 px, no horizontal scroll at 768 and 390 px, the popover Query options and its dot, the Tab order, the names and the targets (10.3)",
    async run(t) {
      const created = await t.api.create("transit", rowDoc());
      t.assert(created.status === 201, `create ${UID}: ${created.status} ${created.text.slice(0, 400)}`);

      // 1. 1280 px: one row, one top (2 px), the order of the controls, a name for each, 24 px at least.
      const p = await openAt(t, "refresh=off&edit=1&editPanel=1", 1280);
      const row = await p.eval(ROW);
      t.eq(row.bars, 1, "one bar of controls in the tab Query");
      t.eq(row.list.map((c) => c.name), ["Source", "Table in Format", "Time series in Format", "Name", "Min interval", "Max points", "Query options", "Run"], "the controls of the row, in this order: " + JSON.stringify(row.list.map((c) => c.name)));
      const spread = Math.max(...row.list.map((c) => c.top)) - Math.min(...row.list.map((c) => c.top));
      // The two segments of Format are 3 px lower than the other controls inside their group (the group has a 2 px padding and a 1 px edge): compare the groups.
      const groups = await p.eval(`[...document.querySelectorAll('.qarea > .qbar > .qbar__item, .qarea > .qbar > .qopt')].map((e) => Math.round(e.getBoundingClientRect().top * 10) / 10)`);
      t.assert(Math.max(...groups) - Math.min(...groups) <= 2, "the items of the row have the same top (2 px): " + JSON.stringify(groups));
      t.assert(spread <= 4, "the controls have the same top (the segments sit 3 px lower in their group): " + spread);
      t.assert(row.bar.height <= 40, "the row is one line high: " + row.bar.height);
      t.eq(row.scroll <= row.client, true, "1280: no horizontal scroll");
      for (const c of row.list) t.assert(c.w >= 24 && c.h >= 24, `the target ${c.name} is 24 px at least: ${c.w} x ${c.h}`);
      // The Tab order is the visual order: the DOM order is the order from left to right on one row.
      const lefts = row.list.map((c) => c.left);
      t.eq(lefts, [...lefts].sort((a, b) => a - b), "1280: the DOM order is the visual order");
      // The real Tab key goes through the row in the same order.
      await p.focus('.qarea > .qbar select');
      const seen = [];
      for (let i = 0; i < row.list.length - 1; i++) {
        await p.press("Tab");
        seen.push(await p.eval(`(() => { const e = document.activeElement; return e.dataset.testid || (e.id && document.querySelector('label[for="' + e.id + '"]') ? document.querySelector('label[for="' + e.id + '"]').textContent.trim() : e.textContent.trim()); })()`));
      }
      t.eq(seen, ["Table", "Time series", "query-ref", "Min interval", "Max points", "query-options", "run-button"], "the Tab key follows the row");

      // 2. The popover: closed at first, no dot at the default. It opens with its button, and it holds the four settings.
      t.eq(await expanded(p), "false", "the popover is closed at first");
      t.eq(await dotOn(p), "false", "no dot at the default values");
      t.eq(await p.eval(`document.querySelector('[data-testid="query-options-dot"]').hidden`), true, "the dot is hidden");
      t.eq(await p.eval(`getComputedStyle(document.querySelector('[data-testid="query-options-pop"]')).display`), "none", "the closed popover has no box");
      await p.click(p.tid("query-options"));
      t.eq(await expanded(p), "true", "the button opens the popover");
      const pop = await p.eval(`(() => {
        const r = document.querySelector('[data-testid="query-options-pop"]').getBoundingClientRect();
        const names = [...document.querySelectorAll('[data-testid="query-options-pop"] label, [data-testid="query-options-pop"] .field__label, [data-testid="query-options-pop"] .seg__item')].map((e) => e.textContent.trim());
        return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, w: innerWidth, h: innerHeight, names };
      })()`);
      t.eq(pop.names, ["Data", "Query", "Reuse a panel", "Relative time", "Time shift", "Hide time info"], "the popover holds the four settings: " + JSON.stringify(pop.names));
      t.assert(pop.left >= 0 && pop.right <= pop.w && pop.bottom <= pop.h, "the popover is inside the window: " + JSON.stringify(pop));
      // The Tab key goes from the button into the popover, then to "Run" (the panel follows its button in the DOM).
      await p.focus(p.tid("query-options"));
      await p.press("Tab");
      t.eq(await p.eval(`document.activeElement.textContent.trim() || document.activeElement.dataset.testid`), "Query", "Tab from the button enters the popover");
      // Escape closes it and gives the focus back to the button.
      await p.press("Escape");
      t.eq(await expanded(p), "false", "Escape closes the popover");
      t.eq(await p.eval(`document.activeElement.dataset.testid`), "query-options", "the focus is on the button");
      t.assert(await p.eval(`!!document.querySelector('[data-testid="panel-editor"]')`), "Escape leaves the editor open");
      // A click outside closes it.
      await p.click(p.tid("query-options"));
      t.eq(await expanded(p), "true", "open again");
      await p.click(".peditor__top");
      t.eq(await expanded(p), "false", "a click outside closes the popover");

      // 3. The settings write the members, and the dot follows them. A bad text writes nothing and keeps the dot off.
      await p.click(p.tid("query-options"));
      await p.fill(p.tid("query-time-from"), "6h");
      await invalid(p, p.tid("query-time-from"), "6h is not a relative time");
      t.eq(await dotOn(p), "false", "a bad text keeps the dot off");
      await p.fill(p.tid("query-time-from"), "now-3h");
      await waitDraft(p, UID, (d) => d.document.sections[0].panels[0].data.query.time_from === "now-3h");
      t.eq(await dotOn(p), "true", "the dot shows with a relative time");
      t.eq(await p.eval(`document.querySelector('[data-testid="query-options-dot"]').hidden`), false, "the dot is visible");
      t.assert((await p.eval(`document.querySelector('[data-testid="query-options-note"]').textContent`)).includes("set"), "the dot has a text for the screen reader");
      await p.fill(p.tid("query-time-shift"), "1d");
      await p.click(p.tid("query-hide-time"));
      await waitDraft(p, UID, (d) => { const q = d.document.sections[0].panels[0].data.query; return q.time_shift === "1d" && q.hide_time_override === true; });
      t.eq(Object.keys((await panelOf(p, 1)).data.query).slice(-3), ["time_from", "time_shift", "hide_time_override"], "the members are in the canonical order");
      await p.fill(p.tid("query-time-from"), "");
      await p.fill(p.tid("query-time-shift"), "");
      await p.click(p.tid("query-hide-time"));
      // A draft that equals the saved document is removed: no draft means the saved members, and the saved panel 1 has none of the three.
      for (let i = 0; i < 50; i++) {
        const d = await draftOf(p, UID);
        const q = d ? d.document.sections[0].panels[0].data.query : {};
        if (!("time_from" in q) && !("time_shift" in q) && !("hide_time_override" in q)) break;
        t.assert(i < 49, "the three members leave the draft: " + JSON.stringify(q));
        await sleep(100);
      }
      t.eq(await dotOn(p), "false", "the dot is off when the three settings are at their default");
      // The two numbers of the row keep their rule.
      await p.press("Escape");
      await p.fill('.qarea > .qbar [placeholder="auto"]', "20000");
      await invalid(p, '.qarea > .qbar [placeholder="auto"]', "20000 is not a number of points");
      await p.fill('.qarea > .qbar [placeholder="auto"]', "500");
      await p.fill('.qarea > .qbar [placeholder="1ms"]', "30s");
      await waitDraft(p, UID, (d) => { const q = d.document.sections[0].panels[0].data.query; return q.max_data_points === 500 && q.min_interval === "30s"; });
      // The error of a field of the row floats under it: the row keeps its height.
      await p.fill('.qarea > .qbar [placeholder="auto"]', "20000");
      await invalid(p, '.qarea > .qbar [placeholder="auto"]', "the error of 20000 shows");
      await painted(p);
      const withError = await p.eval(`document.querySelector('.qarea > .qbar').getBoundingClientRect().height`);
      t.assert(withError <= 40, "an error does not make the row higher: " + withError);
      await p.fill('.qarea > .qbar [placeholder="auto"]', "500");

      // 4. A panel with a time override has the dot at once. A panel that reuses another panel has it too, and the popover has the way back.
      const q2 = await openAt(t, "refresh=off&edit=1&editPanel=2", 1280);
      t.eq(await dotOn(q2), "true", "the dot shows for a saved relative time");
      const q4 = await openAt(t, "refresh=off&edit=1&editPanel=4", 1280);
      t.eq(await dotOn(q4), "true", "the dot shows for a panel that reuses another panel");
      t.eq(await q4.eval(`document.querySelectorAll('.qarea > .qbar').length`), 1, "one row for a panel that reuses another panel");
      t.eq(await q4.eval(`!!document.querySelector('.qarea > .qbar select')`), true, "the row has the select of the source panel");
      t.eq(await q4.eval(`!!document.querySelector('[data-testid="query-ref"]')`), false, "no query fields for a reuse");
      await q4.click(q4.tid("query-options"));
      await q4.clickText('[data-testid="query-options-pop"] .seg__item', "Query");
      await q4.waitSel(q4.tid("query-ref"));
      t.eq(await dotOn(q4), "false", "the dot is off again with a query");
      const back = await q4.eval(ROW);
      t.eq(back.bars, 1, "one row after the way back");
      t.assert(tops(back.list.filter((c) => c.tid !== null)).length >= 1, "the row is drawn");

      // 5. 768 px and 390 px: the row wraps, there is no horizontal scroll, and the order stays the visual order (rows from top to bottom, then left to right).
      for (const [width, maxRows] of [[1024, 2], [768, 2], [390, 4]]) {
        const s = await openAt(t, "refresh=off&edit=1&editPanel=1", width);
        const r = await s.eval(ROW);
        t.assert(r.scroll <= r.client, `${width}: no horizontal scroll (${r.scroll} > ${r.client})`);
        for (const c of r.list) t.assert(c.left >= -0.5 && c.right <= r.win + 0.5, `${width}: ${c.name} is inside the window: ${c.left} to ${c.right}`);
        t.assert(tops(r.list.filter((c) => !/ in Format$/.test(c.name))).length <= maxRows, `${width}: at most ${maxRows} rows: ${JSON.stringify(tops(r.list))}`);
        // The rows are found by the vertical centre: a segment of Format is 3 px lower in its group, and its centre is the centre of the row.
        const centre = (c) => Math.round((c.top + c.h / 2) / 10);
        const visual = [...r.list].sort((a, b) => centre(a) - centre(b) || a.left - b.left).map((c) => c.i);
        t.eq(visual, r.list.map((c) => c.i), `${width}: the DOM order is the visual order`);
        for (const c of r.list) t.assert(c.w >= 24 && c.h >= 24, `${width}: the target ${c.name} is 24 px at least: ${c.w} x ${c.h}`);
        await s.click(s.tid("query-options"));
        const box = await s.eval(`(() => { const r = document.querySelector('[data-testid="query-options-pop"]').getBoundingClientRect(); return { left: r.left, right: r.right, w: innerWidth, scroll: document.documentElement.scrollWidth, client: document.documentElement.clientWidth }; })()`);
        t.assert(box.left >= 0 && box.right <= box.w && box.scroll <= box.client, `${width}: the open popover is inside the window and gives no scroll: ${JSON.stringify(box)}`);
      }
    },
  },
];
