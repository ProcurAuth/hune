// Form building blocks of the editor: fields, collapsible option groups, the type list, colour choices and the stub
// frame of a panel with a type that the page cannot draw. Markup only: no document text goes in with innerHTML.

import { PANEL_TYPES } from "./draft.js";
import { assetUrl } from "../config.js";
import { clear, el, icon, segmented, uniqueId } from "../ui.js";

let cssLoaded = null;
// Loads css/editor.css once. The dialogs of the list page need it too (fields, file input, spacing).
// The promise ends when the style sheet has loaded (or fails, or after 1.5 s): an editor view shows only after it (10.2a, rule 2).
export function loadEditorCss() {
  if (cssLoaded) return cssLoaded;
  cssLoaded = new Promise((resolve) => {
    const link = el("link", { rel: "stylesheet", href: assetUrl("css/editor.css") });
    link.addEventListener("load", resolve);
    link.addEventListener("error", resolve);
    setTimeout(resolve, 1500);
    document.head.append(link);
  });
  return cssLoaded;
}

export function ensureCss() {
  loadEditorCss();
}

export const debounced = (fn, ms) => {
  let timer = 0;
  const run = (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
  run.flush = () => clearTimeout(timer);
  return run;
};

// A field: label, control, hint and an error line that the control refers to with aria-describedby.
export function field(label, control, { hint = null, inline = false } = {}) {
  const id = uniqueId("fld");
  const err = el("p", { class: "field-error", id: `${id}-err`, role: "alert", hidden: true });
  control.id = id;
  control.setAttribute("aria-describedby", `${id}-err`);
  const node = el("div", { class: `field${inline ? " field--inline" : ""}` },
    el("label", { class: "field__label", for: id, text: label }),
    control,
    ...(hint ? [el("span", { class: "field__hint", text: hint })] : []),
    err);
  return {
    node,
    control,
    error(text) {
      err.textContent = text || "";
      err.hidden = !text;
      if (text) control.setAttribute("aria-invalid", "true");
      else control.removeAttribute("aria-invalid");
    },
  };
}

// A label around an existing control for a dialog: it keeps the id and the aria-describedby of the control.
export function labelled(label, control, hint = null) {
  if (!control.id) control.id = uniqueId("fld");
  return el("div", { class: "field" }, el("label", { class: "field__label", for: control.id, text: label }), control, ...(hint ? [hint] : []));
}

// An error line: only the place (a JSON pointer, or a line and a column) is a link. The rest is plain text.
export function errorLink(place, rest, onClick) {
  const link = el("button", { type: "button", class: "link-button error-link" }, el("code", { text: place }));
  link.addEventListener("click", onClick);
  return el("span", { class: "error-line" }, link, rest ? ` ${rest}` : "");
}

export function selectOf(options, value) {
  const s = el("select", { class: "input" });
  for (const [v, text] of options) s.append(el("option", { value: v, text }));
  s.value = value;
  return s;
}

// A labelled segmented control (a group of two to four exclusive choices). The group has the name, not a `for`.
export function segmentedField(label, items, value, onChange) {
  const seg = segmented({ label, items, value, onChange });
  const node = el("div", { class: "field" },
    el("span", { class: "field__label", "aria-hidden": "true", text: label }), seg.el);
  return { node, seg };
}

// A collapsible group of options. `open` is a Map that keeps the state of each group across redraws.
// The badge shows a count. `section.setBadge(text)` changes it (an empty text hides it), so a form can keep it right while the person edits.
export function optionGroup({ key, title, open, defaultOpen = false, badge = null, children }) {
  const isOpen = open.has(key) ? open.get(key) : defaultOpen;
  const bodyId = uniqueId("og");
  const badgeEl = el("span", { class: "badge", text: badge || "", hidden: !badge });
  const head = el("button", { type: "button", class: "ogroup__head", "aria-expanded": String(isOpen), "aria-controls": bodyId, dataset: { group: key } },
    icon("chevron-right", 16), el("span", { class: "ogroup__title", text: title }), badgeEl);
  const body = el("div", { class: "ogroup__body", id: bodyId, hidden: !isOpen }, ...children);
  head.addEventListener("click", () => {
    const next = head.getAttribute("aria-expanded") !== "true";
    head.setAttribute("aria-expanded", String(next));
    body.hidden = !next;
    open.set(key, next);
  });
  const section = el("section", { class: "ogroup" }, head, body);
  section.setBadge = (text) => {
    badgeEl.textContent = text || "";
    badgeEl.hidden = !text;
  };
  return section;
}

// --- the type list ---------------------------------------------------------------------------------------------------

export const TYPE_INFO = {
  table: ["Table", "Rows and columns"],
  timeseries: ["Time series", "Lines, bars and points over time"],
  stat: ["Stat", "One big value"],
  barchart: ["Bar chart", "Bars by category"],
  piechart: ["Pie chart", "Shares of a total"],
  xychart: ["XY chart", "Two numbers as points"],
  bargauge: ["Bar gauge", "Values as horizontal bars"],
  gauge: ["Gauge", "Values on a round scale"],
  state_timeline: ["State timeline", "States of each row over time"],
  status_history: ["Status history", "One cell for each sample"],
  heatmap: ["Heatmap", "Counts in a grid of time and value"],
  histogram: ["Histogram", "Counts of values in buckets"],
  logs: ["Logs", "Lines of text with time and level"],
  geomap: ["Geomap", "Markers on a map"],
  text: ["Text", "Markdown text, no query"],
};

// A small picture of a type, drawn with CSS only (the rules are in css/editor.css).
export const glyph = (type) => el("span", { class: `tglyph tglyph--${type}`, "aria-hidden": "true" }, ...(type === "barchart" || type === "xychart" || type === "bargauge" || type === "state_timeline" || type === "status_history" || type === "heatmap" || type === "histogram" || type === "logs" ? [el("i"), el("i"), el("i"), el("i")] : []), ...(type === "stat" ? [el("b", { text: "42" })] : []));

// A visual list of the panel types. One item is pressed. `onPick(type)` runs for a new choice.
export function typeList(value, onPick) {
  const list = el("div", { class: "typelist", role: "group", "aria-label": "Visualisation", dataset: { testid: "type-list" } });
  const known = PANEL_TYPES.includes(value);
  for (const type of PANEL_TYPES) {
    const [name, text] = TYPE_INFO[type];
    const item = el("button", { type: "button", class: "typelist__item", "aria-pressed": String(type === value), title: text, dataset: { type } },
      glyph(type), el("span", { class: "typelist__text" }, el("span", { class: "typelist__name", text: name }), el("span", { class: "typelist__desc", text })));
    item.addEventListener("click", () => {
      if (type !== value) onPick(type);
    });
    list.append(item);
  }
  if (!known) list.append(el("p", { class: "field-error", role: "alert", text: `The type ${JSON.stringify(String(value))} is not a panel type. Pick one of the list.` }));
  return list;
}

// --- colours -------------------------------------------------------------------------------------------------------------

// A row of small swatches for the main named colours. A swatch is a button with the colour name as its label.
export function swatches(names, value, onPick) {
  const wrap = el("div", { class: "swatches", role: "group", "aria-label": "Named colours" });
  for (const name of names) {
    const b = el("button", { type: "button", class: "swatch", "aria-label": name, "aria-pressed": String(name === value), title: name, dataset: { color: name } });
    b.style.setProperty("--swatch-color", `var(--viz-${name})`);
    b.addEventListener("click", () => onPick(name));
    wrap.append(b);
  }
  return {
    el: wrap,
    set(next) {
      wrap.querySelectorAll(".swatch").forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.color === next)));
    },
  };
}

// The stub that stands in for a panel frame when the type is not one that the page can draw.
export function stubFrame(panel) {
  const root = el("section", { class: "panel panel--stub", dataset: { testid: `panel-${panel.id}`, state: "error", type: String(panel.type) } },
    el("div", { class: "panel__head" }, el("h3", { class: "panel__title", text: panel.title || "Untitled" })),
    el("div", { class: "panel__body" }, el("div", { class: "panel__overlay", dataset: { kind: "error" } },
      el("div", { class: "panel__error" },
        el("div", { class: "panel__error-head" }, icon("error", 14), el("span", { text: "Type not valid" })),
        el("p", { class: "panel__error-text", text: `The type ${JSON.stringify(String(panel.type))} is not a panel type. Open the panel editor and pick a type.` })))));
  return { el: root, update() {}, destroy() {}, width: () => 0 };
}

export { clear };
