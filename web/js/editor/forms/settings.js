// The dashboard settings view (10.3b, `editview=settings`): the tabs "General", "Time", "Links", "Variables", "Annotations" and "JSON".
// Each form reads the draft and writes the draft. The tab "JSON" is the dashboard JSON of 10.4.

import { REFRESH_CHOICES } from "../../time.js";
import * as timeModule from "../../time.js";
import { button, el } from "../../ui.js";
import { createJsonEditor } from "../json_editor.js";
import { createAnnotations } from "./annotations.js";
import { createScope, selectField, textField } from "./controls.js";
import { dashLinksForm } from "./dash_links.js";
import { LIMITS } from "./model.js";
import { tagsEditor } from "./tags.js";
import { checkChars, checkTimeOrder, checkTimeText, checkZoneName } from "./rules.js";
import { createVariables } from "./variables.js";

export const SETTINGS_TABS = [["general", "General"], ["time", "Time"], ["links", "Links"], ["variables", "Variables"], ["annotations", "Annotations"], ["json", "JSON"]];

export const ZONE_UNKNOWN = "This browser does not know this time zone.";

// The page knows a zone name when Intl accepts it (9.9). `isKnownZone` of time.js is the same rule, when the page has it.
export function zoneKnown(name) {
  if (typeof timeModule.isKnownZone === "function") return timeModule.isKnownZone(name);
  if (name === "browser" || name === "utc") return true;
  try {
    new Intl.DateTimeFormat("en", { timeZone: name });
    return true;
  } catch (error) {
    return false;
  }
}

function zoneNames() {
  try {
    return typeof Intl.supportedValuesOf === "function" ? Intl.supportedValuesOf("timeZone") : [];
  } catch (error) {
    return [];
  }
}

export function createSettings(E) {
  const scope = createScope();
  const root = el("section", { class: "settings", dataset: { testid: "settings-view" } });
  const body = el("div", { class: "settings__body" });
  const tabs = el("div", { class: "tabs settings__tabs", role: "tablist", "aria-label": "Dashboard settings" });
  let tab = E.settingsTab || "general";
  let json = null;
  let vars = null;
  let annotations = null;
  const change = (fn) => E.change(fn(structuredClone(E.doc)));
  const doc = () => E.doc;

  // --- the bar ---------------------------------------------------------------------------------------------------------------

  // "Save" and "Revert" are the buttons of the header (10.2): this bar has only "Back to dashboard", the event `leave` of 10.2a.
  const back = button("Back to dashboard", { class: "btn btn--ghost", dataset: { testid: "editor-back" }, on: { click: () => { scope.flush(); E.dispatch("leave"); } } }, "arrow-left");

  // --- the tabs --------------------------------------------------------------------------------------------------------------

  function general() {
    const title = textField(scope, { label: "Title", value: doc().title, check: (v) => checkChars(v, 1, 256, "The title"), apply: (v) => change((d) => ({ ...d, title: v })), testid: "settings-title" });
    const description = textField(scope, { label: "Description", value: doc().description, multiline: true, rows: 4, check: (v) => checkChars(v, 0, 4096, "The description"), apply: (v) => change((d) => ({ ...d, description: v })), testid: "settings-description" });
    const tags = tagsEditor({ label: "Tags", get: () => doc().tags, set: (next) => change((d) => ({ ...d, tags: next })), limit: LIMITS.tags, testid: "settings-tags" });
    const cursor = selectField({ label: "Cursor sync", options: [["off", "Off"], ["crosshair", "Crosshair"]], value: doc().cursor_sync, testid: "settings-cursor-sync", apply: (v) => change((d) => ({ ...d, cursor_sync: v })) });
    return el("div", { class: "settings__form" }, title.node, description.node, tags, cursor.node);
  }

  function time() {
    const t = doc().time;
    const set = (patch) => change((d) => ({ ...d, time: { ...d.time, ...patch } }));
    const from = textField(scope, {
      label: "From",
      value: t.from,
      mono: true,
      check: (v) => checkTimeText(v) || checkTimeOrder(v, doc().time.to),
      apply: (v) => set({ from: v }),
      testid: "settings-time-from",
    });
    const to = textField(scope, {
      label: "To",
      value: t.to,
      mono: true,
      check: (v) => checkTimeText(v) || checkTimeOrder(doc().time.from, v),
      apply: (v) => set({ to: v }),
      testid: "settings-time-to",
    });
    const isOther = t.timezone !== "browser" && t.timezone !== "utc";
    const warn = el("p", { class: "field__hint settings__warning", role: "status", hidden: true, dataset: { testid: "settings-zone-warning" } });
    const listId = "settings-zone-list";
    const zoneText = textField(scope, {
      label: "Time zone name",
      value: isOther ? t.timezone : "",
      list: listId,
      placeholder: "Europe/Paris",
      check: checkZoneName,
      apply(name) {
        set({ timezone: name });
        paintWarn(name);
      },
      testid: "settings-zone-name",
    });
    const paintWarn = (name) => {
      const unknown = name !== "" && !zoneKnown(name);
      warn.textContent = unknown ? ZONE_UNKNOWN : "";
      warn.hidden = !unknown;
    };
    const zoneHost = el("div", { class: "field" }, zoneText.node, warn, el("datalist", { id: listId }, ...zoneNames().map((n) => el("option", { value: n }))));
    zoneHost.hidden = !isOther;
    paintWarn(isOther ? t.timezone : "");
    const zone = selectField({
      label: "Time zone",
      options: [["browser", "Browser time"], ["utc", "UTC"], ["other", "Other"]],
      value: isOther ? "other" : t.timezone,
      testid: "settings-zone",
      apply(v) {
        scope.flush();
        zoneHost.hidden = v !== "other";
        if (v === "other") {
          zoneText.control.focus();
          return;
        }
        set({ timezone: v });
        paintWarn("");
      },
    });
    const refresh = selectField({
      label: "Refresh",
      options: [["", "Off"], ...REFRESH_CHOICES.map((r) => [r, r])],
      value: t.refresh === null ? "" : t.refresh,
      testid: "settings-refresh",
      apply: (v) => set({ refresh: v === "" ? null : v }),
    });
    return el("div", { class: "settings__form" }, el("div", { class: "field-grid" }, from.node, to.node), zone.node, zoneHost, refresh.node);
  }

  function links() {
    return el("div", { class: "settings__form" }, dashLinksForm(scope, {
      get: () => doc().links || [],
      set: (arr) => change((d) => ({ ...d, links: arr })),
    }));
  }

  function renderTab() {
    scope.flush();
    if (json) {
      json.destroy();
      json = null;
    }
    if (vars) {
      vars.destroy();
      vars = null;
    }
    if (annotations) {
      annotations.destroy();
      annotations = null;
    }
    body.replaceChildren();
    for (const node of tabs.querySelectorAll("[role=tab]")) {
      const on = node.dataset.tab === tab;
      node.setAttribute("aria-selected", String(on));
      node.tabIndex = on ? 0 : -1;
    }
    root.dataset.tab = tab;
    if (tab === "general") body.append(general());
    else if (tab === "time") body.append(time());
    else if (tab === "links") body.append(links());
    else if (tab === "variables") {
      vars = createVariables(E, scope);
      body.append(el("div", { class: "settings__form" }, vars.el));
    } else if (tab === "annotations") {
      annotations = createAnnotations(E, scope);
      body.append(el("div", { class: "settings__form" }, annotations.el));
    } else {
      json = createJsonEditor(E, { embedded: true });
      body.append(json.el);
    }
  }

  function select(name) {
    scope.flush();
    tab = name;
    E.settingsTab = name;
    renderTab();
  }

  for (const [name, label] of SETTINGS_TABS) {
    tabs.append(el("button", { type: "button", role: "tab", class: "tab", text: label, dataset: { tab: name, testid: `settings-tab-${name}` }, on: {
      click: () => select(name),
      keydown: (e) => {
        if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
        e.preventDefault();
        const at = SETTINGS_TABS.findIndex((x) => x[0] === tab);
        select(SETTINGS_TABS[(at + (e.key === "ArrowRight" ? 1 : SETTINGS_TABS.length - 1)) % SETTINGS_TABS.length][0]);
        tabs.querySelector("[aria-selected=true]").focus();
      },
    } }));
  }

  root.append(
    el("div", { class: "ebar" }, back, el("h2", { class: "ebar__title", text: "Dashboard settings" }), el("span", { class: "ebar__status" })),
    tabs,
    body);
  renderTab();

  return {
    el: root,
    destroy() {
      scope.flush();
      if (json) json.destroy();
      if (vars) vars.destroy();
      if (annotations) annotations.destroy();
    },
    // The text fields apply their last change at once (a save asks for it).
    flush: () => scope.flush(),
    onDiscard() {
      scope.cancel();
      renderTab();
    },
    hasUnapplied: () => Boolean(json && json.hasUnapplied()),
    focusPointer(pointer) {
      if (tab !== "json") select("json");
      if (json) json.focusPointer(pointer);
    },
    refreshErrors() {},
  };
}
