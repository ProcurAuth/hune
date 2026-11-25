// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B92 (9.9): the time range picker is the picker of ClickHouse Dash.
// It checks the look of the popup, the quick ranges and their search, the recent ranges, the absolute fields, the calendar, the keys
// (Enter, Escape, Tab, the arrows), the 160 ms motion and reduced motion, the narrow layouts, and the shift and zoom buttons.
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { sleep } from "../lib/h.mjs";
import { QUICK_RANGES } from "../../../web/js/time.js";

const UID = "pk92";
const POP = ".picker__popup";

const pickerDoc = () => doc(UID, "Picker", [
  section("Overview", [panel(1, "stat", "Stat", "SELECT 7 AS value", { grid: { x: 0, y: 0, w: 12, h: 4 } })]),
], { time: { from: "now-6h", to: "now", timezone: "browser", refresh: null } });

const texts = (p, sel) => p.eval(`[...document.querySelectorAll(${JSON.stringify(sel)})].filter((e) => !e.hidden).map((e) => e.textContent.trim())`);
const params = async (p) => Object.fromEntries(new URLSearchParams(await p.eval("location.search")));
const activeId = (p) => p.eval(`(document.activeElement && (document.activeElement.dataset.testid || document.activeElement.dataset.key || document.activeElement.className)) || null`);
const val = (p, id) => p.eval(`document.querySelector('[data-testid="${id}"]').value`);
const local = (p, y, m, d, h = 0, mi = 0, s = 0) => p.eval(`new Date(${y}, ${m - 1}, ${d}, ${h}, ${mi}, ${s}).getTime()`);
const openPopup = async (p) => {
  await p.click(p.tid("time-picker-button"));
  await p.waitSel(`${POP}.is-open`);
};
const closeWith = async (p, key) => {
  await p.press(key);
  await p.waitGone(POP);
};
/** A key that the helper `press` does not know: the codes of Home, End, PageUp and PageDown. */
const CODES = { Home: 36, End: 35, PageUp: 33, PageDown: 34 };
async function key(p, name, modifiers = 0) {
  const base = { key: name, code: name, windowsVirtualKeyCode: CODES[name], nativeVirtualKeyCode: CODES[name], modifiers };
  await p.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
  await p.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
}
const title = (p) => p.text(".picker__cal-title");
const dayClasses = (p, k) => p.eval(`document.querySelector('[data-key="${k}"]').className.split(" ").filter((c) => c.startsWith("is-")).sort()`);
const keysWith = (p, cls) => p.eval(`[...document.querySelectorAll(".picker__day.${cls}")].map((e) => Number(e.dataset.key))`);

export const cases = [
  {
    id: "B92",
    name: "the time range picker is the picker of ClickHouse Dash: popup, quick ranges, absolute fields with a calendar, keys, 160 ms motion (9.9)",
    async run(t) {
      const r = await t.api.create("transit", pickerDoc());
      t.assert(r.status === 201, `create ${UID}: ${r.status} ${r.text.slice(0, 300)}`);
      const p = await t.open(t.dash(UID, "refresh=off"), { width: 1280, height: 900 });
      await p.waitFor(`document.querySelector('[data-testid="panel-1"]')?.dataset.state === "ready"`, { what: "the panel" });

      // 1. The look: the popup of 604 px under the right edge of the time group, two columns, the texts of ClickHouse Dash, the footer.
      await openPopup(p);
      await sleep(250); // the motion of 160 ms ends: a box that is measured during it holds the transform
      const geo = await p.eval(`(() => {
        const r = (s) => { const b = document.querySelector(s).getBoundingClientRect(); return { left: b.left, right: b.right, top: b.top, bottom: b.bottom, width: b.width, height: b.height }; };
        const probe = document.createElement("i");
        probe.style.borderTopColor = "var(--border-strong)";
        document.body.append(probe);
        const token = getComputedStyle(probe).borderTopColor;
        probe.remove();
        const pop = getComputedStyle(document.querySelector(".picker__popup"));
        return { pop: r(".picker__popup"), group: r('[data-testid="time-picker"]'), abs: r(".picker__abs"), col: r(".picker__col"), item: r(".picker__quick-item"),
          border: pop.borderTopColor, token, width: pop.borderTopWidth, radius: pop.borderTopLeftRadius };
      })()`);
      t.eq(geo.pop.width, 604, "the popup is 604 px wide");
      t.assert(Math.abs(geo.pop.right - geo.group.right) <= 1, `the right edge of the popup is the right edge of the time group: ${geo.pop.right} ${geo.group.right}`);
      t.assert(Math.abs(geo.pop.top - geo.group.bottom - 5) <= 1.5, `the popup is 5 px below the time group: ${geo.pop.top} ${geo.group.bottom}`);
      t.assert(geo.abs.right <= geo.col.left + 1 && Math.abs(geo.col.width - 216) <= 1, `the absolute range at the left, the column of 216 px at the right: ${JSON.stringify([geo.abs, geo.col])}`);
      t.eq(geo.border, geo.token, "the edge of the popup is the token --border-strong");
      t.eq([geo.width, geo.radius], ["1px", "8px"], "a border of 1 px and a radius of 8 px");
      t.eq(geo.item.height, 27, "a quick range is 27 px high");
      t.eq(await texts(p, ".picker__popup .picker__heading"), ["Absolute time range", "Quick ranges"], "the headings");
      t.eq(await texts(p, ".picker__field > label"), ["From", "To"], "the labels of the fields");
      t.eq(await p.text(p.tid("time-apply")), "Apply time range", "the label of the button");
      t.eq(await p.attr(p.tid("time-quick-search"), "placeholder"), "Search quick ranges", "the search");
      t.eq(await p.attr(p.tid("time-from"), "placeholder"), "YYYY-MM-DD HH:mm:ss or now-6h", "the placeholder of From");
      t.assert(/^Browser time \u00b7 .+ \(UTC[+-]\d\d:\d\d\)$/.test(await p.text(".picker__zone")), "the footer: " + (await p.text(".picker__zone")));
      t.eq(await texts(p, ".picker__quick-item"), QUICK_RANGES.map((q) => q.label), "the quick ranges of 9.9, in order");
      t.eq(await p.text(".picker__hint"), "Pick the start date", "the hint of the calendar");
      // The focus is in From, with its text selected. The first field is the active one.
      t.eq(await activeId(p), "time-from", "the focus is in From");
      t.eq(await p.eval(`(() => { const e = document.querySelector('[data-testid="time-from"]'); return [e.selectionStart, e.selectionEnd === e.value.length]; })()`), [0, true], "the text of From is selected");
      t.assert(await p.eval(`document.querySelector(".picker__field").classList.contains("is-active")`), "the field From is active");
      t.eq(await title(p), "November 2026", "the calendar shows the month of the range");
      t.eq(await p.count(".picker__day"), 42, "42 days");
      t.eq(await p.eval(`[...document.querySelectorAll(".picker__weekday")].map((e) => e.textContent)`), ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"], "the week starts on Monday");
      // The range of the URL (2026-11-09 from 08:00 to 08:20 UTC) shows in the calendar as one day.
      t.eq(await keysWith(p, "is-start"), [20261109], "the first day of the range");
      t.eq(await dayClasses(p, 20261109), ["is-end", "is-single", "is-start"], "one day: start, end and single");
      await closeWith(p, "Escape");
      t.eq(await activeId(p), "time-picker-button", "Escape gives the focus back to the range button");

      // 2. The motion: 160 ms for the opacity and the transform, a fade that shows between 0 and 1, and a fade out before the popup leaves.
      const frames = await p.eval(`new Promise((resolve) => {
        document.querySelector('[data-testid="time-picker-button"]').click();
        const e = document.querySelector(".picker__popup");
        const seen = [];
        const t0 = performance.now();
        const step = () => { seen.push(Number(getComputedStyle(e).opacity)); if (performance.now() - t0 < 320) requestAnimationFrame(step); else resolve(seen); };
        requestAnimationFrame(step);
      })`);
      t.assert(frames.some((v) => v > 0 && v < 1) && frames[frames.length - 1] === 1, "the popup fades in: " + JSON.stringify(frames.map((v) => Math.round(v * 100) / 100)));
      t.eq(await p.eval(`getComputedStyle(document.querySelector(".picker__popup")).transitionDuration`), "0.16s, 0.16s, 0s", "open: 160 ms for the opacity and the transform");
      await p.press("Escape");
      const leave = await p.eval(`(() => { const e = document.querySelector(".picker__popup"); return e ? [e.classList.contains("is-open"), getComputedStyle(e).transitionDuration + " | " + getComputedStyle(e).transitionDelay, document.querySelector('[data-testid="time-picker-button"]').getAttribute("aria-expanded")] : null; })()`);
      t.eq(leave, [false, "0.16s, 0.16s, 0s | 0s, 0s, 0.16s", "false"], "the popup is still in the page while it fades out (160 ms, then it hides), and the button is collapsed");
      await p.waitGone(POP, { timeout: 1000 });
      // A new press in the 160 ms of a close opens a new popup and removes the old one.
      await p.eval(`(() => { const b = document.querySelector('[data-testid="time-picker-button"]'); b.click(); b.click(); b.click(); })()`);
      await sleep(250);
      t.eq(await p.count(POP), 1, "one popup after three quick presses");
      await closeWith(p, "Escape");

      // 3. Quick ranges: a click applies and closes; the item of the current range has aria-current.
      await openPopup(p);
      await p.clickText(".picker__quick-item", "Last 6 hours");
      await p.waitGone(POP);
      // "Last 6 hours" is the range of the document: the address leaves out the range (9.5). B69 and B07 check a relative range that differs.
      t.eq(JSON.stringify([(await params(p)).from, (await params(p)).to]), "[null,null]", "a quick range that is the default of the document is not in the address");
      await openPopup(p);
      t.eq(await p.eval(`[...document.querySelectorAll('.picker__quick-item[aria-current="true"]')].map((e) => e.textContent)`), ["Last 6 hours"], "the current range is marked");
      t.eq(await texts(p, ".picker__popup .picker__heading"), ["Absolute time range", "Quick ranges"], "a quick range is not in the recent list");
      t.eq(await val(p, "time-from"), "now-6h", "From shows the text of the range");
      t.eq(await val(p, "time-to"), "now", "To shows the text of the range");

      // 4. The search: it filters, it says when nothing matches, it offers a typed duration, Enter applies the first item.
      await p.fill(p.tid("time-quick-search"), "6 h");
      t.eq(await texts(p, ".picker__quick-item"), ["Last 6 hours"], "the search keeps the items that hold its text");
      await p.fill(p.tid("time-quick-search"), "zzz");
      t.eq(await p.text(".picker__none"), "No quick range matches \u201czzz\u201d.", "the text of an empty search");
      await p.fill(p.tid("time-quick-search"), "45m");
      t.eq(await texts(p, ".picker__quick-item"), ["Last 45 minutes"], "a duration that is typed makes a range");
      await p.press("Enter");
      await p.waitGone(POP);
      t.eq([(await params(p)).from, (await params(p)).to], ["now-45m", "now"], "Enter applies the first item");
      t.eq(await p.text(`${p.tid("time-picker-button")} .picker__text`), "now-45m to now", "the button shows the range");
      await openPopup(p);
      t.eq(await texts(p, ".picker__popup .picker__heading"), ["Absolute time range", "Quick ranges"], "a typed duration is not in the recent list");
      await p.fill(p.tid("time-quick-search"), "last 1 hour");
      await p.press("Enter");
      await p.waitGone(POP);
      t.eq((await params(p)).from, "now-1h", "Enter on a search text of a label");

      // 5. The keys of the list: ArrowDown from the search goes in, ArrowUp from the first item goes back, Enter on an item applies.
      await openPopup(p);
      await p.focus(p.tid("time-quick-search"));
      await p.press("ArrowDown");
      t.eq(await p.eval(`document.activeElement.textContent`), "Last 5 minutes", "ArrowDown from the search focuses the first item");
      await p.press("ArrowDown");
      t.eq(await p.eval(`document.activeElement.textContent`), "Last 15 minutes", "ArrowDown moves to the next item");
      await p.press("ArrowUp");
      await p.press("ArrowUp");
      t.eq(await activeId(p), "time-quick-search", "ArrowUp from the first item goes back to the search");
      await p.press("ArrowDown");
      await p.press("ArrowDown");
      await p.press("Enter");
      await p.waitGone(POP);
      t.eq((await params(p)).from, "now-15m", "Enter on an item applies it");

      // 6. The absolute fields: Enter in a field applies the form, the times are in the zone, date math stays as text.
      await openPopup(p);
      await p.fill(p.tid("time-from"), "2026-11-09 09:00:00");
      await p.fill(p.tid("time-to"), "2026-11-09 09:10:00");
      t.eq(await title(p), "November 2026", "the calendar follows the fields");
      await p.press("Enter");
      await p.waitGone(POP);
      const a1 = [await local(p, 2026, 11, 9, 9, 0, 0), await local(p, 2026, 11, 9, 9, 10, 0)];
      t.eq([(await params(p)).from, (await params(p)).to], a1.map(String), "Enter in a field applies the absolute range");
      await openPopup(p);
      t.eq([await val(p, "time-from"), await val(p, "time-to")], ["2026-11-09 09:00:00", "2026-11-09 09:10:00"], "the fields show an absolute time as text");
      await p.fill(p.tid("time-from"), "now-2h");
      await p.fill(p.tid("time-to"), "now/d");
      await p.click(p.tid("time-apply"));
      await p.waitGone(POP);
      t.eq([(await params(p)).from, (await params(p)).to], ["now-2h", "now/d"], "date math stays as text");

      // 7. The recent ranges: up to two, the last used first, a click applies, a range of the list is the current one.
      await openPopup(p);
      await p.fill(p.tid("time-from"), "2026-11-09 09:20:00");
      await p.fill(p.tid("time-to"), "2026-11-09 09:30:00");
      await p.click(p.tid("time-apply"));
      await p.waitGone(POP);
      await openPopup(p);
      await p.fill(p.tid("time-from"), "2026-11-09 09:40:00");
      await p.fill(p.tid("time-to"), "2026-11-09 09:50:00");
      await p.click(p.tid("time-apply"));
      await p.waitGone(POP);
      await openPopup(p);
      t.eq(await texts(p, ".picker__popup .picker__heading"), ["Absolute time range", "Recently used", "Quick ranges"], "the recent list shows");
      const recent = () => p.eval(`[...document.querySelectorAll('.picker__quick-item[data-kind="recent"]')].map((e) => [e.textContent, e.getAttribute("aria-current")])`);
      t.eq(await recent(), [["2026-11-09 09:40:00 to 2026-11-09 09:50:00", "true"], ["2026-11-09 09:20:00 to 2026-11-09 09:30:00", null]], "two recent ranges, the last first, the current one marked (the first of three is gone)");
      await p.fill(p.tid("time-quick-search"), "09:20");
      t.eq(await texts(p, ".picker__quick-item"), ["2026-11-09 09:20:00 to 2026-11-09 09:30:00"], "the search filters the recent ranges");
      await p.press("Enter");
      await p.waitGone(POP);
      t.eq((await params(p)).from, String(await local(p, 2026, 11, 9, 9, 20, 0)), "a recent range applies");
      await openPopup(p);
      t.eq((await recent()).map((x) => x[0]), ["2026-11-09 09:20:00 to 2026-11-09 09:30:00", "2026-11-09 09:40:00 to 2026-11-09 09:50:00"], "the range that was used goes first");
      await closeWith(p, "Escape");

      // 8. Errors: the text under the field, aria-invalid, the focus on the first wrong field; a range that the page refuses shows one text.
      await openPopup(p);
      await p.fill(p.tid("time-from"), "yesterday");
      await p.click(p.tid("time-apply"));
      await p.waitFor(`[...document.querySelectorAll(".picker__field .field-error")].some((e) => !e.hidden && e.textContent.length > 0)`, { what: "the error of the field" });
      t.eq(await p.attr(p.tid("time-from"), "aria-invalid"), "true", "the wrong field is marked");
      t.eq(await activeId(p), "time-from", "the focus goes to the wrong field");
      t.assert((await p.text(".picker__field .field-error")).startsWith('The time text "yesterday" is not valid.'), "the text of the error");
      const before = await params(p);
      await p.fill(p.tid("time-from"), "2026-11-09 10:00:00");
      t.assert(await p.eval(`document.querySelector(".picker__field .field-error").hidden`), "the error goes when the field changes");
      await p.fill(p.tid("time-to"), "2026-11-09 09:00:00");
      await p.click(p.tid("time-apply"));
      await p.waitFor(`!document.querySelector(".picker__error").hidden`, { what: "the error of the range" });
      t.assert((await p.text(".picker__error")).length > 0, "a range with From after To shows an error");
      t.eq(await params(p), before, "an invalid range keeps the earlier range");
      await closeWith(p, "Escape");

      // 9. The calendar: two clicks make a range (the focus goes to To, then to the button), a preview shows on a hover.
      await p.goto(t.dash(UID, "refresh=off"));
      await p.idle();
      await openPopup(p);
      await p.click('[data-key="20261105"]');
      t.eq(await val(p, "time-from"), "2026-11-05 00:00:00", "the first click sets the start at 00:00:00");
      t.eq(await p.text(".picker__hint"), "Pick the end date", "the hint asks for the end");
      t.eq(await activeId(p), "time-to", "the focus goes to To");
      t.assert(await p.eval(`document.querySelectorAll(".picker__field")[1].classList.contains("is-active")`), "the field To is active");
      await p.hover('[data-key="20261108"]');
      t.eq(await keysWith(p, "is-preview"), [20261106, 20261107, 20261108], "the days up to the hover show a preview");
      await p.click('[data-key="20261108"]');
      t.eq(await val(p, "time-to"), "2026-11-08 23:59:59", "the second click sets the end at 23:59:59");
      t.eq(await activeId(p), "time-apply", "the focus goes to the button");
      t.eq(await p.text(".picker__hint"), "Pick the start date", "the hint asks for the start again");
      t.eq([await keysWith(p, "is-start"), await keysWith(p, "is-end"), await keysWith(p, "is-inside"), await keysWith(p, "is-preview")], [[20261105], [20261108], [20261106, 20261107], []], "the range is tinted");
      await p.click(p.tid("time-apply"));
      await p.waitGone(POP);
      t.eq([(await params(p)).from, (await params(p)).to], [String(await local(p, 2026, 11, 5, 0, 0, 0)), String(await local(p, 2026, 11, 8, 23, 59, 59))], "the days make the absolute range");
      // An end before the start makes a new start.
      await openPopup(p);
      await p.click('[data-key="20261112"]');
      await p.click('[data-key="20261110"]');
      t.eq([await val(p, "time-from"), await p.text(".picker__hint")], ["2026-11-10 00:00:00", "Pick the end date"], "a day before the start makes a new start");
      await closeWith(p, "Escape");

      // 10. The keys of the calendar: arrows, Home and End, PageUp and PageDown, Shift for the year; the buttons of the header.
      await openPopup(p);
      await p.focus('.picker__day[tabindex="0"]');
      t.eq(await activeId(p), "20261105", "the day of the start is the one tab stop of the grid");
      await p.press("ArrowRight");
      t.eq(await activeId(p), "20261106", "ArrowRight: the next day");
      await p.press("ArrowDown");
      t.eq(await activeId(p), "20261113", "ArrowDown: the next week");
      await p.press("ArrowUp");
      t.eq(await activeId(p), "20261106", "ArrowUp: the week before");
      await key(p, "Home");
      t.eq(await activeId(p), "20261102", "Home: the Monday");
      await key(p, "End");
      t.eq(await activeId(p), "20261108", "End: the Sunday");
      await key(p, "PageDown");
      t.eq([await activeId(p), await title(p)], ["20261208", "December 2026"], "PageDown: the next month");
      await key(p, "PageUp", 8);
      t.eq([await activeId(p), await title(p)], ["20251208", "December 2025"], "Shift PageUp: the year before");
      await key(p, "PageUp");
      t.eq([await activeId(p), await title(p)], ["20251108", "November 2025"], "PageUp: the month before");
      await p.press("Enter");
      t.eq(await val(p, "time-from"), "2025-11-08 00:00:00", "Enter on a day picks it");
      await p.click('[aria-label="Next month"]');
      t.eq(await title(p), "December 2025", "the button Next month");
      await p.click('[aria-label="Next year"]');
      t.eq(await title(p), "December 2026", "the button Next year");
      await p.click('[aria-label="Previous month"]');
      await p.click('[aria-label="Previous year"]');
      t.eq(await title(p), "November 2025", "the buttons Previous month and Previous year");
      await p.fill(p.tid("time-from"), "2025-03-10 00:00:00");
      t.eq(await title(p), "March 2025", "a typed date moves the calendar");
      await closeWith(p, "Escape");

      // 11. Close: a press outside, Tab out of the popup.
      await openPopup(p);
      for (const type of ["mouseMoved", "mousePressed", "mouseReleased"]) await p.send("Input.dispatchMouseEvent", { type, x: 100, y: 700, button: "left", clickCount: 1 });
      await p.waitGone(POP);
      await openPopup(p);
      await p.eval(`(() => { const all = [...document.querySelectorAll(".picker__quick-item")]; all[all.length - 1].focus(); })()`);
      await p.press("Tab");
      await p.waitGone(POP);
      t.assert(await p.eval(`!document.activeElement.closest(".picker__popup")`), "the focus left the popup");
      t.eq(await p.attr(p.tid("time-picker-button"), "aria-expanded"), "false", "the button is collapsed");

      // 12. The shift buttons and the zoom out stay in the time control: they act on the applied range.
      await p.goto(t.dash(UID, "refresh=off"));
      await p.idle();
      const d0 = (await params(p));
      const from0 = Number(d0.from);
      const to0 = Number(d0.to);
      await p.click(p.tid("time-back"));
      await p.waitFor(`new URLSearchParams(location.search).get("from") !== "${d0.from}"`, { what: "back" });
      t.eq(Number((await params(p)).from), from0 - Math.floor((to0 - from0) / 2), "back moves the range by half of its span");
      await p.click(p.tid("zoom-out"));
      await p.idle();
      t.assert(Number((await params(p)).to) - Number((await params(p)).from) === 2 * (to0 - from0), "zoom out doubles the span");

      // 13. Reduced motion: no fade, the popup leaves at once.
      const q = await t.open(t.dash(UID, "refresh=off"), { width: 1280, height: 900 });
      await q.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
      await openPopup(q);
      t.eq(await q.eval(`getComputedStyle(document.querySelector(".picker__popup")).transitionDuration`), "0s", "reduced motion: no transition");
      await q.press("Escape");
      await q.waitFor(`!document.querySelector(".picker__popup")`, { timeout: 120, what: "the popup leaves at once" });

      // 14. Narrow layouts: one column up to 768 px; on a phone the focus is on the popup and nothing overflows.
      const n = await t.open(t.dash(UID, "refresh=off"), { width: 768, height: 900 });
      await openPopup(n);
      const stack = await n.eval(`(() => { const a = document.querySelector(".picker__abs").getBoundingClientRect(); const c = document.querySelector(".picker__col").getBoundingClientRect(); return [c.top >= a.bottom - 1, document.documentElement.scrollWidth <= document.documentElement.clientWidth]; })()`);
      t.eq(stack, [true, true], "768: the lists are under the absolute range, and the page does not overflow");
      t.eq(await n.eval(`document.querySelectorAll(".picker__day").length`), 42, "the calendar shows at 768");
      const s = await t.open(t.dash(UID, "refresh=off"), { width: 390, height: 844 });
      await openPopup(s);
      const phone = await s.eval(`(() => { const b = document.querySelector(".picker__popup").getBoundingClientRect(); return [Math.round(b.width), b.left >= 0 && b.right <= innerWidth, document.activeElement.classList.contains("picker__popup"), document.documentElement.scrollWidth <= document.documentElement.clientWidth]; })()`);
      t.eq(phone, [374, true, true, true], "390: the popup fits, the focus is on the popup, the page does not overflow");
      await s.press("Escape");
      await s.waitGone(POP);
      t.eq(await s.eval(`document.activeElement.dataset.testid`), "time-picker-button", "Escape gives the focus back");
    },
  },
];
