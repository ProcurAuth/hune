// The time control of the header and the refresh picker (9.9). The time picker is in time_picker.js.

import { REFRESH_CHOICES } from "./time.js";
import { createTimePicker } from "./time_picker.js";
import { clear, el, icon, openMenu, uniqueId } from "./ui.js";

// The time control of the header (9.9) is one composite control: the time group, then the refresh group, joined. createTimeControl builds
// it. The time group has back, the range button, forward and zoom out; the refresh group has the refresh button and the interval button.
// Each one is a toolbar button (.btn--tb); the buttons of the control touch.
export function createTimeControl({ time, refresh }) {
  const timePicker = createTimePicker(time);
  const refreshPicker = createRefreshPicker(refresh);
  const root = el("div", { class: "tc", dataset: { testid: "time-control" } }, timePicker.el, refreshPicker.el);
  return { el: root, time: timePicker, refresh: refreshPicker };
}

// The refresh group: the run button and the interval button, as one group. The run button shows its icon only; its name "Refresh" is for
// the tooltip and for assistive technology. A click on the run button always refreshes: it stops the older queries and runs again
// (9.9, B09). While queries run, a spinner replaces the refresh icon (setRunning). The interval button is a small chevron, with the
// value beside it when the interval is on. It opens a themed menu with "Off" and the values of REFRESH_CHOICES (9.9).
// A hidden native list (data-testid refresh-select) holds the value and the choices: it is the one source of the value, and a
// change event on it is a choice of the menu.
export function createRefreshPicker({ choices = REFRESH_CHOICES, value, onChange, onRefreshNow }) {
  const select = el("select", { id: uniqueId("refresh"), hidden: true, tabIndex: -1, "aria-hidden": "true", dataset: { testid: "refresh-select" } });
  select.append(el("option", { value: "off", text: "Off" }));
  for (const c of choices) select.append(el("option", { value: c, text: c }));
  const runIcon = el("span", { class: "picker__run-icon" });
  const runText = el("span", { class: "btn__label", text: "Refresh" });
  const now = el("button", { type: "button", class: "btn btn--tb picker__run", "data-tooltip": "Refresh", dataset: { testid: "refresh-now" } }, runIcon, runText);
  let running = false;
  const spinner = () => {
    const node = icon("spinner", 18);
    node.classList.add("icon--spin");
    return node;
  };
  function paintRun() {
    runIcon.replaceChildren(running ? spinner() : icon("refresh", 18));
    now.classList.toggle("is-running", running);
  }
  now.addEventListener("click", () => onRefreshNow());
  const text = el("span", { class: "picker__interval-text" });
  const interval = el("button", {
    type: "button",
    class: "btn btn--tb picker__interval",
    "aria-haspopup": "menu",
    "aria-expanded": "false",
    dataset: { testid: "refresh-interval", tooltip: "Refresh interval", tooltipAlign: "end" },
  }, text, icon("chevron-down", 16));
  interval.dataset.tooltip = "Refresh interval";
  const root = el("div", { class: "picker picker--refresh tb-group", dataset: { testid: "refresh-picker" } }, now, interval, select);
  function paint(v) {
    select.value = v === null || v === undefined ? "off" : v;
    root.classList.toggle("is-active", select.value !== "off");
    // No interval: the button shows a dash before its chevron, so it is never empty (the name for assistive technology still says "Off").
    text.textContent = select.value === "off" ? "-" : select.value;
    interval.setAttribute("aria-label", `Refresh interval: ${select.value === "off" ? "Off" : select.value}`);
  }
  paint(value);
  paintRun();
  select.addEventListener("change", () => {
    paint(select.value);
    onChange(select.value === "off" ? null : select.value);
  });
  interval.addEventListener("click", () => {
    const items = ["off", ...choices].map((v) => ({
      label: v === "off" ? "Off" : v,
      checked: select.value === v,
      onSelect: () => {
        select.value = v;
        select.dispatchEvent(new Event("change", { bubbles: true }));
      },
    }));
    openMenu(interval, items, { align: "end", testid: "refresh-menu", gap: 8 });
  });
  return {
    el: root,
    set: paint,
    // true while the queries of the page run: a spinner replaces the refresh icon.
    setRunning(next) {
      if (running === Boolean(next)) return;
      running = Boolean(next);
      paintRun();
    },
  };
}

export { clear };
