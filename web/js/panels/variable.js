// The frame of the placed control of a variable (4.20, 9.10, 9.11): the control in the grid, drawn like a panel frame. The grid gives it as a
// panel of type `variable` (var_layout.js). It has no query and no state of the scheduler: its frame has `data-state="ready"` from the
// first frame. The control is the code of the bar (vars.js).

import { button, el, openMenu } from "../ui.js";
import { groupLabel, variableNameOf } from "../var_layout.js";
import { canHold, controlsFor, variableOf } from "../vars.js";

const NOTE = "The control of this variable is in another place.";

// The CSS file of the frame. The shell links it (css/panel-variable.css); this call adds the link when the shell does not.
function ensureCss() {
  if (document.querySelector('link[href$="panel-variable.css"]')) return;
  const sheet = document.querySelector('link[href$="panel-stat.css"]');
  if (!sheet) return;
  document.head.append(el("link", { rel: "stylesheet", href: sheet.getAttribute("href").replace("panel-stat.css", "panel-variable.css") }));
}

// page: { rt, placedMenu(name) }. The page has `placedMenu` in edit mode only: the frame then has a menu button.
export function createVariableFrame({ panel, page }) {
  ensureCss();
  const rt = page.rt;
  const name = variableNameOf(panel);
  const text = (value) => (typeof rt.interpolate === "function" ? rt.interpolate(value) : value);
  const editing = typeof page.placedMenu === "function";

  // The title bar shows with a title only (9.11). Without it the bar is a thin strip, the handle of the drag in edit mode.
  const head = el("header", { class: `panel__head${panel.title ? "" : " panel__head--bare"}` });
  const titleEl = panel.title ? el("h3", { class: "panel__title" }) : null;
  if (titleEl) head.append(titleEl);
  head.append(el("span", { class: "panel__spacer", "aria-hidden": "true" }));
  let menuButton = null;
  if (editing) {
    menuButton = button(`Menu of the control ${name}`, { dataset: { testid: `placement-${name}-menu` }, "aria-haspopup": "menu", "aria-expanded": "false", "data-tooltip": "Menu", "data-tooltip-align": "end" }, "more", { iconOnly: true, variant: "ghost" });
    menuButton.classList.add("panel__menu");
    head.append(menuButton);
    menuButton.addEventListener("click", () => openMenu(menuButton, page.placedMenu(name), { align: "end", testid: `placement-${name}-menu-popup` }));
  }

  const host = el("div", { class: "panel-var vars-bar" });
  const note = el("span", { class: "panel__empty panel-var__note", text: NOTE, hidden: true });
  const body = el("div", { class: "panel__body" }, el("div", { class: "panel__content" }, host, note));
  const root = el("section", {
    class: `panel panel--variable${editing ? " panel--var-edit" : ""}`,
    role: "group",
    dataset: { testid: `placement-${name}`, state: "ready", type: "variable" },
  }, head, body);
  const g = panel.grid;
  root.style.gridColumn = `${g.x + 1} / span ${g.w}`;
  root.style.gridRow = `${g.y + 1} / span ${g.h}`;
  root.style.setProperty("--panel-h", String(g.h));
  root.dataset.span = g.w <= 12 ? "half" : "full";

  let release = null;
  let destroyed = false;

  function paintTexts() {
    const variable = variableOf(rt, name);
    const title = text(panel.title);
    if (titleEl && titleEl.textContent !== title) {
      titleEl.textContent = title;
      titleEl.title = title;
    }
    if (variable) root.setAttribute("aria-label", groupLabel(title, variable));
    if (menuButton) menuButton.setAttribute("aria-label", `Menu of the control ${name}`);
  }

  function update() {
    if (destroyed) return;
    paintTexts();
    const variable = variableOf(rt, name);
    if (!variable) return;
    // The runtime of a version in the history view has no selection: the frame shows the name only.
    if (!canHold(rt)) {
      host.replaceChildren(el("span", { class: "panel__empty", text: `Control of the variable ${variable.label || variable.name}` }));
      return;
    }
    if (!release) release = controlsFor(rt).hold(name, host, rt, (home) => { if (note.hidden !== home) note.hidden = home; });
  }

  update();

  return {
    el: root,
    update,
    paintTexts,
    // The inner width of the frame: the content box (9.8).
    width() {
      const style = getComputedStyle(root);
      return Math.round(root.clientWidth - (parseFloat(style.paddingLeft) || 0) - (parseFloat(style.paddingRight) || 0));
    },
    suspend() {},
    resume() {},
    destroy() {
      destroyed = true;
      if (release) release();
      release = null;
      root.remove();
    },
  };
}
