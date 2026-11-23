// The time picker (9.9): the range button and its popup. The popup has the absolute range at the left (From, To, a calendar, "Apply time
// range") and the search and the lists at the right. Its look, keys and labels follow the range picker of ClickHouse Dash (ID-K60 to ID-K66).

import { QUICK_RANGES, formatRangeText, parseAbsoluteInput, wallClock, zoneLabel, zoneName, zoneOffsetLabel } from "./time.js";
import { addDays, addMonths, dayText, keyOf, monthGrid, parseRecent, pushRecent, typedRange, weekday } from "./time_calendar.js";
import { anchorPopup, button, el, icon, uniqueId } from "./ui.js";
import { formatTime } from "./zone.js";

const RECENT_KEY = "hune.timeRanges";
const same = (a, b) => a.from === b.from && a.to === b.to;

// time: { zone, getRange(): { fromText, toText }, apply(fromText, toText): { error? }, onZoomOut(): void, onShift(dir): void }
// dir is -1 (back) or 1 (forward): the page moves the range by half of its span (time.js shiftRange).
export function createTimePicker({ zone, getRange, apply, onZoomOut, onShift = () => {} }) {
  const label = el("span", { class: "picker__text" });
  const chevron = el("span", { class: "picker__chevron" }, icon("chevron-down", 16));
  const toggle = el("button", { type: "button", class: "btn btn--tb picker__button", "aria-haspopup": "dialog", "aria-expanded": "false", dataset: { testid: "time-picker-button" } }, icon("clock", 18), label, chevron);
  // The shift buttons have an aria-label and no text: a text would start with "Move" (the browser suite counts such buttons when editing is off).
  const shiftButton = (name, testid, glyph) => el("button", { type: "button", class: "btn btn--tb btn--icon btn--tb-nav", "aria-label": name, "data-tooltip": name, dataset: { testid } }, icon(glyph, 18));
  const backButton = shiftButton("Shift the time range back", "time-back", "chevrons-left");
  const forwardButton = shiftButton("Shift the time range forward", "time-forward", "chevrons-right");
  const zoomButton = button("Zoom out", { class: "btn btn--tb btn--icon", dataset: { testid: "zoom-out" } }, "zoom-out", { iconOnly: true, variant: "tb" });
  backButton.addEventListener("click", () => onShift(-1));
  forwardButton.addEventListener("click", () => onShift(1));
  zoomButton.addEventListener("click", () => onZoomOut());
  const root = el("div", { class: "picker picker--time tb-group", dataset: { testid: "time-picker" } }, backButton, toggle, forwardButton, zoomButton);
  let popup = null;
  let leaving = null;
  let follow = null;

  function paint() {
    const { fromText, toText } = getRange();
    label.textContent = formatRangeText(fromText, toText, zone);
    toggle.setAttribute("aria-label", `Time range: ${label.textContent}, ${zoneLabel(zone)}`);
    toggle.setAttribute("data-tooltip", `${zoneLabel(zone)}. Change the time range.`);
  }

  // The popup fades out in 160 ms and then leaves the page (at once with reduced motion). A new popup removes the one that leaves.
  function close(restore = true) {
    if (!popup) return;
    const gone = popup;
    leaving = gone;
    popup = null;
    if (follow) follow();
    follow = null;
    gone.classList.remove("is-open");
    setTimeout(() => {
      gone.remove();
      if (leaving === gone) leaving = null;
    }, window.matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : 160);
    toggle.setAttribute("aria-expanded", "false");
    chevron.replaceChildren(icon("chevron-down", 16));
    document.removeEventListener("pointerdown", onDown, true);
    if (restore) toggle.focus();
  }

  function onDown(event) {
    if (popup && !popup.contains(event.target) && !root.contains(event.target)) close(false);
  }

  const recent = () => {
    try { return parseRecent(window.localStorage.getItem(RECENT_KEY)); } catch { return []; }
  };

  function open() {
    if (popup) return close();
    if (leaving) leaving.remove();
    leaving = null;
    const cur = getRange();
    const current = { from: cur.fromText, to: cur.toText };
    const id = uniqueId("tr");
    const monthFormat = new Intl.DateTimeFormat("en-US", { month: "long", year: "numeric", timeZone: "UTC" });
    const dayFormat = new Intl.DateTimeFormat("en-US", { dateStyle: "full", timeZone: "UTC" });
    const dayOf = (ms) => {
      const w = wallClock(ms, zone);
      return keyOf(w.year, w.month, w.day);
    };

    // The fields From and To. An absolute time is epoch milliseconds in the page, and the field shows it in the zone.
    const makeField = (edge, text, example) => {
      const input = el("input", {
        class: "input", id: `${id}-${edge}`, type: "text", value: /^[0-9]+$/.test(text) ? formatTime(Number(text), zone) : text, autocomplete: "off", spellcheck: "false",
        placeholder: `YYYY-MM-DD HH:mm:ss or ${example}`, "aria-describedby": `${id}-${edge}-err`, dataset: { testid: `time-${edge}` },
      });
      const err = el("p", { class: "field__error field-error", id: `${id}-${edge}-err`, role: "alert", hidden: true });
      return { input, err, edge, box: el("div", { class: "picker__field" }, el("label", { for: input.id, text: edge === "from" ? "From" : "To" }), input, err) };
    };
    const fields = [makeField("from", current.from, "now-6h"), makeField("to", current.to, "now")];
    const [fromIn, toIn] = fields.map((f) => f.input);
    const generalErr = el("p", { class: "field__error field-error picker__error", role: "alert", hidden: true });
    const timeOf = (f) => {
      const r = parseAbsoluteInput(f.input.value, zone, { now: Date.now(), edge: f.edge });
      return r.error ? NaN : r.ms;
    };
    const clearError = (f) => {
      f.err.hidden = generalErr.hidden = true;
      f.input.removeAttribute("aria-invalid");
    };

    // The calendar: the month of `cal.focus`; `mode` is the side that a click on a day sets; `hover` is the day under the pointer.
    const cal = { y: 0, m: 0, focus: 0, mode: "from", hover: 0 };
    let cells = [];
    const hint = el("p", { class: "picker__hint", "aria-live": "polite" });
    const title = el("strong", { class: "picker__cal-title", "aria-live": "polite" });
    const grid = el("div", { class: "picker__days", role: "group", "aria-label": "Calendar" });
    const nav = (name, step, glyph, flip) => el("button", {
      type: "button", class: `picker__nav${flip ? " picker__nav--flip" : ""}`, "aria-label": name, "data-tooltip": name, on: { click: () => view(addMonths(cal.focus, step)) },
    }, icon(glyph, 16));
    const calendar = el("div", { class: "picker__cal" },
      el("div", { class: "picker__cal-head" }, nav("Previous year", -12, "chevrons-left"), nav("Previous month", -1, "chevron-right", true), title, nav("Next month", 1, "chevron-right"), nav("Next year", 12, "chevrons-right")),
      grid);

    function paintDays() {
      const a = timeOf(fields[0]);
      const b = timeOf(fields[1]);
      const start = Number.isFinite(a) ? dayOf(a) : 0;
      const picking = cal.mode === "to" && start > 0 && cal.hover >= start;
      const end = picking ? cal.hover : Number.isFinite(b) && b > a ? dayOf(b - 1) : 0;
      const today = dayOf(Date.now());
      for (const { key, btn } of cells) {
        btn.classList.toggle("is-today", key === today);
        btn.classList.toggle("is-start", key === start);
        btn.classList.toggle("is-end", key === end);
        btn.classList.toggle("is-single", key === start && !(end > start));
        btn.classList.toggle("is-inside", key > start && key < end);
        btn.classList.toggle("is-preview", picking && key > start && key <= end);
        btn.setAttribute("aria-pressed", String(key === start || key === end));
        btn.tabIndex = key === cal.focus ? 0 : -1;
      }
      if (!cells.some(({ btn }) => btn.tabIndex === 0)) cells.find(({ btn }) => !btn.classList.contains("is-outside")).btn.tabIndex = 0;
    }

    // The calendar shows the month of a day. A day of another month builds the 42 days again.
    function view(key) {
      cal.focus = key;
      const y = Math.floor(key / 10000);
      const m = Math.floor(key / 100) % 100;
      if (cal.y !== y || cal.m !== m) {
        [cal.y, cal.m] = [y, m];
        title.textContent = monthFormat.format(Date.UTC(y, m - 1, 1));
        cells = monthGrid(y, m).map((d) => ({
          key: d.key,
          btn: el("button", { type: "button", class: `picker__day${d.out ? " is-outside" : ""}`, tabIndex: -1, "aria-label": dayFormat.format(Date.UTC(d.y, d.m - 1, d.d)), dataset: { key: d.key }, text: String(d.d) }),
        }));
        grid.replaceChildren(...["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].map((w) => el("span", { class: "picker__weekday", "aria-hidden": "true", text: w })), ...cells.map((c) => c.btn));
      }
      paintDays();
    }

    function setMode(mode) {
      cal.mode = mode;
      if (mode !== "to") cal.hover = 0;
      fields.forEach((f) => f.box.classList.toggle("is-active", f.edge === mode));
      hint.textContent = mode === "to" ? "Pick the end date" : "Pick the start date";
      paintDays();
    }

    // The first day picked is the start (00:00:00) and the second is the end (23:59:59). The focus goes to To, then to the button.
    function pick(key, byPointer) {
      clearError(fields[0]);
      const a = timeOf(fields[0]);
      if (cal.mode !== "to" || !Number.isFinite(a) || key < dayOf(a)) {
        fromIn.value = `${dayText(key)} 00:00:00`;
        if (!(timeOf(fields[1]) > timeOf(fields[0]))) toIn.value = "";
        setMode("to");
        cal.hover = key;
        view(key);
        if (byPointer) toIn.focus({ preventScroll: true });
        return;
      }
      toIn.value = `${dayText(key)} 23:59:59`;
      setMode("from");
      view(key);
      applyButton.focus({ preventScroll: true });
    }

    const dayAt = (event) => event.target.closest?.("[data-key]");
    grid.addEventListener("click", (event) => dayAt(event) && pick(Number(dayAt(event).dataset.key), event.detail > 0));
    const hover = (key) => {
      if (cal.mode === "to" && cal.hover !== key) {
        cal.hover = key;
        paintDays();
      }
    };
    grid.addEventListener("mouseover", (event) => dayAt(event) && hover(Number(dayAt(event).dataset.key)));
    grid.addEventListener("mouseleave", () => cal.hover && hover(0));
    grid.addEventListener("keydown", (event) => {
      const key = Number(event.target.dataset?.key);
      const year = event.shiftKey ? 12 : 1;
      const next = key && {
        ArrowLeft: () => addDays(key, -1), ArrowRight: () => addDays(key, 1), ArrowUp: () => addDays(key, -7), ArrowDown: () => addDays(key, 7),
        Home: () => addDays(key, -weekday(key)), End: () => addDays(key, 6 - weekday(key)), PageUp: () => addMonths(key, -year), PageDown: () => addMonths(key, year),
      }[event.key];
      if (!next) return;
      event.preventDefault();
      const target = next();
      if (cal.mode === "to") cal.hover = target;
      view(target);
      cells.find((c) => c.key === target)?.btn.focus({ preventScroll: true });
    });
    for (const f of fields) {
      f.input.addEventListener("focus", () => cal.mode !== f.edge && setMode(f.edge));
      f.input.addEventListener("input", () => {
        clearError(f);
        const ms = timeOf(f);
        if (Number.isFinite(ms)) view(dayOf(f.edge === "to" ? ms - 1 : ms));
        else paintDays();
      });
    }

    // A range from the form or from the recent list goes to the recent list. A quick range does not: it is always in the list.
    function choose(range, remember) {
      const result = apply(range.from, range.to);
      if (result && result.error) {
        generalErr.textContent = result.error;
        generalErr.hidden = false;
        return;
      }
      if (remember) {
        try { window.localStorage.setItem(RECENT_KEY, JSON.stringify(pushRecent(recent(), range))); } catch { /* no store: no recent list */ }
      }
      close();
    }

    function submit() {
      fields.forEach(clearError);
      const results = fields.map((f) => parseAbsoluteInput(f.input.value, zone, { now: Date.now(), edge: f.edge }));
      let first = null;
      fields.forEach((f, i) => {
        if (!results[i].error) return;
        f.err.textContent = results[i].error;
        f.err.hidden = false;
        f.input.setAttribute("aria-invalid", "true");
        first = first || f.input;
      });
      if (first) return first.focus({ preventScroll: true });
      // A field in date math stays as text. An absolute time becomes epoch milliseconds.
      const [from, to] = fields.map((f, i) => (/^now/.test(f.input.value.trim()) ? f.input.value.trim() : String(results[i].ms)));
      choose({ from, to }, true);
    }

    const applyButton = el("button", { type: "submit", class: "btn btn--tb btn--tb-primary picker__apply", text: "Apply time range", dataset: { testid: "time-apply" } });
    const form = el("form", { class: "picker__abs", novalidate: true, on: { submit: (e) => { e.preventDefault(); submit(); } } },
      el("p", { class: "picker__heading", text: "Absolute time range" }), fields[0].box, fields[1].box, hint, calendar, generalErr, applyButton);

    // The search, "Recently used" (up to two ranges) and "Quick ranges". A text such as 45m or last 2d makes a range.
    const search = el("input", { type: "search", class: "input picker__search-input", placeholder: "Search quick ranges", autocomplete: "off", spellcheck: "false", "aria-label": "Search quick ranges", dataset: { testid: "time-quick-search" } });
    const lists = el("div", { class: "picker__lists" });
    const items = () => [...lists.querySelectorAll(".picker__quick-item")];
    function renderLists() {
      const text = search.value.trim();
      const needle = text.toLowerCase();
      const fits = (name, r) => needle === "" || name.toLowerCase().includes(needle) || r.from.toLowerCase() === needle;
      let quick = QUICK_RANGES.filter((r) => fits(r.label, r));
      const typed = typedRange(text);
      if (typed && !quick.some((r) => same(r, typed))) quick = [typed, ...quick];
      const saved = recent().map((r) => ({ ...r, label: formatRangeText(r.from, r.to, zone) })).filter((r) => fits(r.label, r));
      const group = (name, kind, list, empty) => [
        el("div", { class: "picker__heading", id: `${id}-${kind}`, text: name }),
        list.length
          ? el("div", { class: "picker__group", role: "group", "aria-labelledby": `${id}-${kind}` }, ...list.map((r) => el("button", {
            type: "button", class: "picker__quick-item", text: r.label, "aria-current": same(r, current) ? "true" : null, dataset: { from: r.from, to: r.to, kind },
          })))
          : el("p", { class: "picker__none", text: empty }),
      ];
      lists.replaceChildren(...(saved.length ? group("Recently used", "recent", saved) : []), ...group("Quick ranges", "quick", quick, `No quick range matches “${text}”.`));
    }
    search.addEventListener("input", renderLists);
    search.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== "ArrowDown") return;
      event.preventDefault();
      if (event.key === "Enter") items()[0]?.click();
      else items()[0]?.focus({ preventScroll: true });
    });
    lists.addEventListener("click", (event) => {
      const b = event.target.closest(".picker__quick-item");
      if (b) choose({ from: b.dataset.from, to: b.dataset.to }, b.dataset.kind === "recent");
    });
    lists.addEventListener("keydown", (event) => {
      const all = items();
      const at = all.indexOf(document.activeElement);
      if ((event.key !== "ArrowDown" && event.key !== "ArrowUp") || at < 0) return;
      event.preventDefault();
      if (event.key === "ArrowUp" && at === 0) search.focus({ preventScroll: true });
      else all[Math.max(0, Math.min(all.length - 1, at + (event.key === "ArrowDown" ? 1 : -1)))].focus({ preventScroll: true });
    });

    popup = el("div", {
      class: "picker__popup", role: "dialog", "aria-label": "Time range",
      on: {
        // The focus leaves the popup (Tab): the popup closes and the focus goes on where the viewer sent it.
        focusout: (event) => event.relatedTarget && popup && !popup.contains(event.relatedTarget) && event.relatedTarget !== toggle && close(false),
        keydown: (event) => {
          if (event.key !== "Escape") return;
          event.preventDefault();
          event.stopPropagation();
          close();
        },
      },
    },
    el("div", { class: "picker__body" }, form, el("section", { class: "picker__col", "aria-label": "Quick ranges" }, el("div", { class: "picker__col-in" }, el("div", { class: "picker__search" }, icon("search", 16), search), lists))),
    el("footer", { class: "picker__footer" }, el("span", { class: "picker__zone", text: `${zoneLabel(zone)}${zone === "browser" ? ` · ${zoneName(zone)}` : ""} (${zoneOffsetLabel(Date.now(), zone)})` })));
    const fromMs = timeOf(fields[0]);
    view(dayOf(Number.isFinite(fromMs) ? fromMs : Date.now()));
    setMode("from");
    renderLists();
    root.append(popup);
    // The right edge of the popup is the right edge of the whole time group, 5 px below it.
    follow = anchorPopup(popup, root, { align: "end", gap: 5 });
    toggle.setAttribute("aria-expanded", "true");
    chevron.replaceChildren(icon("chevron-up", 16));
    document.addEventListener("pointerdown", onDown, true);
    void popup.offsetHeight; // the closed style is laid out first, so that the class "is-open" runs the fade
    popup.classList.add("is-open");
    // On a phone the keyboard would hide the popup: the focus goes on the popup itself. Elsewhere it goes to From, with its text selected.
    if (window.matchMedia("(max-width: 560px)").matches) {
      popup.tabIndex = -1;
      popup.focus();
    } else {
      fromIn.focus({ preventScroll: true });
      fromIn.select();
    }
  }

  toggle.addEventListener("click", open);
  paint();
  return { el: root, update: paint, close };
}
