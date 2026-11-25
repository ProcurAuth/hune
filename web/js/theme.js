// The theme button of the header (9.25), on every page: one icon button that shows the choice (System, Dark or Light). A click goes to the next
// choice in the order System, Dark, Light, then System again. It opens no menu and no popup. Its tooltip and its name for assistive technology say
// the choice. The choice is in localStorage key "hune.theme". boot_theme.js applies it before the first paint; this module changes it.
// A change sends the event "hune-theme" on document: the pages that draw on a canvas draw again.

import { el, icon } from "./ui.js";

// The choices in the order of the click, their labels and icons.
export const THEME_CHOICES = ["system", "dark", "light"];
const LABELS = { system: "System", dark: "Dark", light: "Light" };
const ICONS = { system: "monitor", dark: "moon", light: "sun" };
const KEY = "hune.theme";

// A stored value is a choice, else "system".
export function normalizeChoice(value) {
  return THEME_CHOICES.includes(value) ? value : "system";
}

// The theme that applies for a choice: "dark" or "light". "system" follows the preference of the system.
export function resolveTheme(choice, systemPrefersDark) {
  const value = normalizeChoice(choice);
  return value === "system" ? (systemPrefersDark ? "dark" : "light") : value;
}

// The choice that a click gives: the next one in the order of THEME_CHOICES, and the first after the last.
export function nextChoice(choice) {
  const at = THEME_CHOICES.indexOf(normalizeChoice(choice));
  return THEME_CHOICES[(at + 1) % THEME_CHOICES.length];
}

export const buttonLabel = (choice) => `Theme: ${LABELS[normalizeChoice(choice)]}`;

let memory = null;

export function readTheme() {
  try {
    return normalizeChoice(window.localStorage.getItem(KEY));
  } catch (error) {
    // Blocked storage: the default applies.
    return "system";
  }
}

// The choice of the viewer: "system", "dark" or "light". It is not the resolved theme (that is data-theme of <html>).
export function currentTheme() {
  return memory || readTheme();
}

export function setTheme(value) {
  if (!THEME_CHOICES.includes(value)) return;
  try {
    window.localStorage.setItem(KEY, value);
  } catch (error) {
    // The choice then lasts for this page only.
  }
  memory = value;
  document.dispatchEvent(new Event("hune-theme"));
}

// Builds the button. Each call makes one control; the header makes one.
export function themeControl() {
  // The button holds the three icons. The css shows the one of data-theme-choice, which boot_theme.js sets before the first paint.
  const current = el("span", { class: "theme-select__current", "aria-hidden": "true" });
  for (const value of THEME_CHOICES) {
    const svg = icon(ICONS[value], 18);
    svg.classList.add("theme-icon", `theme-icon--${value}`);
    current.append(svg);
  }
  const trigger = el("button", {
    type: "button",
    class: "btn btn--tb btn--icon theme-select__button",
    dataset: { testid: "theme-toggle", tooltipAlign: "end" },
  }, current);

  function paint() {
    const choice = currentTheme();
    trigger.setAttribute("aria-label", buttonLabel(choice));
    trigger.setAttribute("data-tooltip", buttonLabel(choice));
  }

  trigger.addEventListener("click", () => {
    setTheme(nextChoice(currentTheme()));
    paint();
  });
  // The choice changes elsewhere (another tab, the system): the button shows it.
  document.addEventListener("hune-theme", paint);
  window.addEventListener("storage", (event) => {
    if (event.key === KEY) {
      memory = null;
      paint();
    }
  });
  paint();
  return trigger;
}
