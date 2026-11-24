// The list page (9.7).

import * as api from "./api.js";
import { loadEditorModule } from "./editor_loader.js";
import { dashboardPath, listSearch, parseSearch } from "./url_state.js";
import { absoluteTime, anchorPopup, button, clear, closeMenu, el, icon, notify, openMenu, uniqueId } from "./ui.js";

const SORT_KEY = "hune.list.sort";

// ASCII-only lower case (9.7): non-ASCII letters keep their case.
const fold = (text) => String(text ?? "").replace(/[A-Z]/g, (c) => c.toLowerCase());

const compareText = (a, b) => {
  const x = fold(a);
  const y = fold(b);
  return x < y ? -1 : x > y ? 1 : 0;
};

// Groups the dashboards by folder: folders in title order, dashboards in title order (9.7).
// { descending: true } is the viewer preference "Z-A": the folders and the dashboards in the reverse order.
export function groupDashboards(folders, items, { descending = false } = {}) {
  const byFolder = new Map(folders.map((f) => [f.id, { folder: f, items: [] }]));
  for (const item of items) {
    if (!byFolder.has(item.folder)) byFolder.set(item.folder, { folder: { id: item.folder, title: item.folder, access: { write: false } }, items: [] });
    byFolder.get(item.folder).items.push(item);
  }
  const groups = [...byFolder.values()].sort((a, b) => compareText(a.folder.title, b.folder.title) || compareText(a.folder.id, b.folder.id));
  for (const group of groups) group.items.sort((a, b) => compareText(a.title, b.title) || compareText(a.uid, b.uid));
  if (descending) {
    groups.reverse();
    for (const group of groups) group.items.reverse();
  }
  return groups;
}

// The tag filter (9.7): the text q is one tag name. A dashboard matches when one of its tags is this name, with ASCII letters folded.
// An empty text matches every dashboard.
export function matchesTag(item, text) {
  if (!text) return true;
  const needle = fold(text);
  return (item.tags || []).some((t) => fold(t) === needle);
}

// The short form of the age of a change, for the note of a card (9.7): "now" below one minute (and for a time in the future), then
// minutes "5m", hours "2h", days "3d", weeks "2w", months of 30 days "1mo" and years of 365 days "1y". An unreadable time gives "".
export function shortAge(iso, now = Date.now()) {
  const then = Date.parse(iso);
  if (!Number.isFinite(then)) return "";
  const minutes = Math.floor((now - then) / 60000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  if (days < 30) return `${Math.floor(days / 7)}w`;
  if (days < 365) return `${Math.floor(days / 30)}mo`;
  return `${Math.floor(days / 365)}y`;
}

// True when the list page shows the sort and the tag filter (9.7): the user sees more dashboards than `list_filters_min_dashboards` of the
// session (6.10). A missing or invalid member shows them. The page keeps no copy of the number: each render reads the session.
export function filtersShown(session, count) {
  const min = session ? session.list_filters_min_dashboards : undefined;
  if (!Number.isSafeInteger(min) || min < 0) return true;
  return count > min;
}

// What a card offers to the user (9.7). The pencil and the bin share one condition: editing is on and the item has the write right.
export function cardRights(item, editing) {
  return { write: editing === true && Boolean(item.access && item.access.write) };
}

// Saves a text as a file in the browser.
export function saveTextFile(name, text) {
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  const link = el("a", { href: url, download: name });
  document.body.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function exportDashboard(uid) {
  try {
    const { text } = await api.dashboardDownload(uid);
    saveTextFile(`${uid}.json`, text);
  } catch (error) {
    notify({ level: "error", text: error.message });
  }
}


const SVG_NS = "http://www.w3.org/2000/svg";

// A small icon that the sprite does not have: a list of path strings in the 24 x 24 box of the sprite (same stroke rules).
function glyph(paths, size = 16) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "icon");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  for (const d of paths) {
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", d);
    svg.appendChild(path);
  }
  return svg;
}

const SORT_ARROWS = ["M5 19V5", "M2 8l3-3 3 3", "M12 7h9", "M12 12h6", "M12 17h3"];

// The picture of an empty state (300 x 200): a soft shape, a dashboard card and a sign. The colours are tokens (list.css).
function illustration(kind) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "list-empty__art");
  svg.setAttribute("width", "300");
  svg.setAttribute("height", "200");
  svg.setAttribute("viewBox", "0 0 300 200");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  const add = (name, attrs) => {
    const node = document.createElementNS(SVG_NS, name);
    for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
    svg.appendChild(node);
  };
  add("path", { class: "il-blob", d: "M34 118C14 66 66 16 140 20c80 4 148 38 130 106-12 48-80 58-130 54C80 176 48 170 34 118z" });
  add("rect", { class: "il-card", x: 78, y: 52, width: 124, height: 90, rx: 8 });
  add("rect", { class: "il-bar", x: 92, y: 68, width: 44, height: 8, rx: 4 });
  add("path", { class: "il-line", d: "M92 120l16-18 14 10 18-24 16 14" });
  if (kind === "match") {
    add("circle", { class: "il-glass", cx: 204, cy: 112, r: 30 });
    add("path", { class: "il-glass", d: "M226 134l24 24" });
    add("path", { class: "il-accent", d: "M194 102l20 20M214 102l-20 20" });
  } else {
    add("circle", { class: "il-glass", cx: 214, cy: 126, r: 26 });
    add("path", { class: "il-accent", d: "M214 114v24M202 126h24" });
  }
  return svg;
}

const TAG_PALETTE = [
  "#d32d20", "#1e72b8", "#b240a2", "#705da0", "#466803", "#497a3c", "#3d71aa", "#b15415", "#890f02", "#6e6e6e",
  "#0a437c", "#6d1f62", "#584477", "#4c7a3f", "#2f4f4f", "#bf1b00", "#7662b1", "#8a2eb8", "#517a00", "#000000",
  "#3f6833", "#2f575e", "#99440a", "#ae561a", "#0e4ab4", "#58140c", "#052b51", "#511749", "#3f2b5b",
];

// The colour of a tag chip is a function of the name: a hash of the lower-case name (the shift wraps to 32 bits, the sum stays a double) picks one of 29 colours (measured on
// Grafana 12.4.2). The chip shows the colour as a tint, and its text is the colour mixed with the ink of the theme (look.css), with 4.5:1 or more.
// The name is the only input: the same tag has the same colour on every card, on every page and in every session.
export function tagColor(name) {
  const text = String(name ?? "").toLowerCase();
  let hash = 5381;
  for (let i = 0; i < text.length; i++) hash = (hash << 5) + hash + text.charCodeAt(i);
  return TAG_PALETTE[Math.abs(hash % TAG_PALETTE.length)];
}

// A viewer preference of the list: one of the allowed strings, else the fallback. No state goes to the server.
function readPref(key, allowed, fallback) {
  try {
    const value = window.localStorage.getItem(key);
    return allowed.includes(value) ? value : fallback;
  } catch (error) {
    return fallback;
  }
}

function writePref(key, value) {
  try {
    if (value === null) window.localStorage.removeItem(key);
    else window.localStorage.setItem(key, value);
  } catch (error) {
    // The choice then lasts for this page only.
  }
}

const SORT_LABELS = { az: "Alphabetically (A–Z)", za: "Alphabetically (Z–A)" };

export async function mount(ctx) {
  const controller = new AbortController();
  const { main, header, session } = ctx;
  const editing = session.editing === true;
  let folders = [];
  let folderAccess = { create: false };
  let items = [];
  let query = parseSearch(window.location.search).q || "";
  let sort = readPref(SORT_KEY, ["az", "za"], null);
  let loadedOnce = false;

  header.clear();
  document.title = "Dashboards - Hune";

  const actionsBox = el("div", { class: "list-actions" });
  // The actions are in the header. Up to 900 px the actions hide and one button "New" opens a menu with them. The menu runs the same buttons.
  const newMenu = button("New", { class: "btn btn--tb list-new", "aria-haspopup": "menu", "aria-expanded": "false", hidden: true, dataset: { testid: "list-new-menu" } }, null, { variant: "tb" });
  newMenu.append(icon("chevron-down", 16));
  newMenu.addEventListener("click", () => {
    const entries = [...actionsBox.querySelectorAll("[data-toolbar-action]")].reverse().map((node) => ({ label: node.textContent.trim(), onSelect: () => node.click() }));
    if (entries.length) openMenu(newMenu, entries, { align: "end", gap: 4, testid: "list-new-menu-popup" });
  });
  // The live status says only the tag filter. It has no count (9.7).
  const status = el("p", { class: "list-status visually-hidden", role: "status" });

  const tagButton = el("button", { type: "button", class: "list-select list-select--tags", "aria-haspopup": "listbox", "aria-expanded": "false", "aria-label": "Filter by tag", dataset: { testid: "list-tag-filter" }, on: { click: () => openTagMenu() } });
  tagButton.addEventListener("keydown", onTagKey);
  const sortLabel = el("span", { class: "list-select__text" });
  const sortButton = el("button", { type: "button", class: "list-select list-select--sort", "aria-haspopup": "menu", "aria-expanded": "false", "aria-label": "Sort", dataset: { testid: "list-sort" }, on: { click: () => openSortMenu() } },
    glyph(SORT_ARROWS, 16), sortLabel, icon("chevron-down", 16));
  // The sort and the tag filter are the lead controls of the header: they follow the brand (9.6, 9.7).
  const filters = el("div", { class: "list-filters" }, sortButton, tagButton);

  const body = el("div", { class: "list-body", dataset: { testid: "list" }, "aria-busy": "true" },
    el("div", { class: "list-skeleton", "aria-hidden": "true" }, el("span", { class: "skeleton" }), el("span", { class: "skeleton" }), el("span", { class: "skeleton" })));
  clear(main);
  const page = el("section", { class: "list-page" }, status, body);
  main.append(page);
  header.setLead(filters);
  header.setActions(actionsBox, newMenu);
  filters.hidden = true;

  const editorCtx = () => ({ session, folders, dashboards: items, navigate: ctx.navigate, reload: load });

  async function callEditor(name, fn, ...args) {
    const mod = await loadEditorModule(name);
    if (mod && typeof mod[fn] === "function") await mod[fn](editorCtx(), ...args);
    else if (mod) notify({ level: "warning", text: "The editor is not available in this build." });
  }

  const canWrite = () => editing && folders.some((f) => f.access && f.access.write);

  // New folder, Import and New dashboard (the one primary action of the page).
  function buildToolbar() {
    clear(actionsBox);
    newMenu.hidden = true;
    if (!items.length) return; // an empty list has the actions in its empty state
    const add = (label, handler, testid, iconName, variant = "tb") => actionsBox.append(button(label, { dataset: { toolbarAction: "1", testid }, on: { click: handler } }, iconName, { variant }));
    if (editing && folderAccess.create) add("New folder", () => callEditor("folders", "newFolder"), "new-folder", "folder-plus");
    if (canWrite()) {
      add("Import", () => callEditor("import_export", "importDialog"), "import", "upload");
      add("New dashboard", () => callEditor("folders", "newDashboard"), "new-dashboard", "plus", "tb-primary");
    }
    newMenu.hidden = actionsBox.childElementCount === 0;
  }

  function errorsRow(item) {
    const list = el("ul", { class: "error-list" });
    for (const e of item.errors || []) list.append(el("li", {}, el("code", { text: e.pointer }), ` ${e.message}`));
    return list;
  }

  // The tag filter is the text q of the address (9.7). A choice of a tag sets it. A second choice of the same tag clears it.
  function setQuery(text) {
    query = text;
    window.history.replaceState(null, "", `${window.location.pathname}${listSearch(query)}`);
    render();
  }

  const toggleTag = (text) => setQuery(fold(query) === fold(text) ? "" : text);

  // A chip with the colour of its name. On a card it is a button that filters when the page shows the filters, else a picture.
  const tagChip = (text) => el("span", { class: "list-tag", style: { "--tag-c": tagColor(text) }, text });

  function tagCardButton(text, interactive) {
    if (!interactive) return tagChip(text);
    return el("button", { type: "button", class: "list-tag", style: { "--tag-c": tagColor(text) }, text, title: `Filter by the tag ${text}`, on: { click: () => toggleTag(text) } });
  }

  // A button of the card that is not the title link: an icon button on the layer above the stretched link.
  const cardButton = (label, testid, iconName, attrs = {}) => button(label, { dataset: { testid, tooltipAlign: "end" }, ...attrs }, iconName, { iconOnly: true, variant: "ghost" });

  // One card for one dashboard (9.7). The card is an element with a stretched title link. The buttons are its siblings, never inside the link.
  function dashboardCard(item, interactiveTags) {
    const valid = item.state !== "invalid";
    const rights = cardRights(item, editing);
    const card = el("article", { class: `card${rights.write ? " card--actions" : ""}`, dataset: { testid: `row-${item.uid}`, state: item.state || "valid" } });
    if (valid) {
      card.append(el("h3", { class: "card__title" }, el("a", { class: "card__link", href: dashboardPath(item.uid, item.title), title: item.title, text: item.title })));
    } else {
      const panelId = uniqueId("errors");
      const chevron = el("span", { class: "errors-toggle__chevron" }, icon("chevron-down", 12));
      const toggleText = el("span", { class: "errors-toggle__text", text: "Show errors" });
      const toggle = el("button", { type: "button", class: "btn btn--sm btn--ghost errors-toggle", "aria-expanded": "false", "aria-controls": panelId, on: { click: () => {
        const open = toggle.getAttribute("aria-expanded") === "true";
        toggle.setAttribute("aria-expanded", String(!open));
        toggleText.textContent = open ? "Show errors" : "Hide errors";
        detail.hidden = open;
      } } }, toggleText, chevron);
      const detail = el("div", { id: panelId, class: "invalid-detail", hidden: true }, errorsRow(item));
      card.append(el("h3", { class: "card__title card__title--invalid" }, icon("error", 14), el("code", { class: "card__uid", text: item.uid }), el("span", { class: "badge badge--error", text: "Not valid" })), el("div", { class: "card__errors" }, toggle, detail));
    }
    // The note under the title: the last change in the short form. Its title attribute has the full time. The note has a fixed height.
    card.append(el("p", { class: "card__updated", title: absoluteTime(item.updated_at), text: shortAge(item.updated_at) }));
    card.append(el("div", { class: "card__tags" }, ...(item.tags || []).map((t) => tagCardButton(t, interactiveTags))));
    const buttons = [];
    if (valid && rights.write) {
      // The pencil opens the dashboard in the edit mode. The bin runs the delete dialog. The server decides: a 403 shows the refusal.
      buttons.push(el("a", { class: "btn btn--ghost btn--icon card__edit", href: `${dashboardPath(item.uid, item.title)}?edit=1`, "aria-label": `Edit the dashboard ${item.title}`, dataset: { testid: `card-edit-${item.uid}`, tooltipAlign: "end" }, "data-tooltip": "Edit" }, icon("edit", 16)));
      const more = cardButton(`Actions for ${item.title}`, `row-menu-${item.uid}`, "more", { "aria-haspopup": "menu", "aria-expanded": "false" });
      more.addEventListener("click", () => openMenu(more, [
        { label: "Export", icon: "download", onSelect: () => exportDashboard(item.uid) },
        { label: "Move", icon: "folder", onSelect: () => callEditor("folders", "moveDashboard", item) },
      ], { align: "end" }));
      buttons.push(more);
      const bin = cardButton(`Delete the dashboard ${item.title}`, `card-delete-${item.uid}`, "trash", { class: "btn btn--ghost btn--icon card__delete" });
      bin.addEventListener("click", () => callEditor("folders", "deleteDashboard", item));
      buttons.push(bin);
    } else if (!valid) {
      const more = cardButton(`Actions for ${item.uid}`, `row-menu-${item.uid}`, "more", { "aria-haspopup": "menu", "aria-expanded": "false" });
      const entries = [{ label: "Download", icon: "download", onSelect: () => exportDashboard(item.uid) }];
      if (rights.write) entries.push({ label: "Replace", icon: "upload", onSelect: () => callEditor("import_export", "importDialog", { uid: item.uid, etag: item.etag }) });
      more.addEventListener("click", () => openMenu(more, entries, { align: "end" }));
      buttons.push(more);
    }
    if (buttons.length) card.append(el("div", { class: "card__buttons" }, ...buttons));
    return card;
  }

  // The tags of all dashboards with the number of dashboards: the most used first, then in name order (the order of Grafana).
  function tagCounts() {
    const counts = new Map();
    for (const item of items) for (const t of item.tags || []) counts.set(t, (counts.get(t) || 0) + 1);
    return [...counts].sort((a, b) => b[1] - a[1] || compareText(a[0], b[0]));
  }

  // The tag filter: a select-like button that shows the active tag as its chip, and a popup with a chip and a count for each tag.
  // The filter is the text q (9.7): a tag sets it, a second choice of the same tag clears it.
  let tagMenu = null;
  function closeTagMenu({ restoreFocus = true } = {}) {
    if (!tagMenu) return;
    const { popup, stop, onDown } = tagMenu;
    tagMenu = null;
    document.removeEventListener("pointerdown", onDown, true);
    stop();
    popup.remove();
    tagButton.setAttribute("aria-expanded", "false");
    if (restoreFocus) tagButton.focus();
  }

  function openTagMenu() {
    if (tagMenu) {
      closeTagMenu();
      return;
    }
    closeMenu({ restoreFocus: false });
    const counts = tagCounts();
    if (!counts.length) return;
    const options = [];
    const popup = el("div", { class: "menu list-tagmenu", role: "listbox", "aria-label": "Tags", dataset: { testid: "list-tag-menu" } });
    for (const [text, n] of counts) {
      const selected = fold(query) === fold(text);
      const option = el("button", {
        type: "button",
        class: "list-tagmenu__item",
        role: "option",
        "aria-selected": String(selected),
        tabIndex: -1,
        dataset: { testid: `tag-filter-${text}` },
        on: { click: () => { closeTagMenu({ restoreFocus: false }); toggleTag(text); tagButton.focus(); } },
      }, tagChip(text), el("span", { class: "list-tagmenu__count", text: `(${n})` }));
      options.push(option);
      popup.append(option);
    }
    document.body.append(popup);
    popup.style.minWidth = `${tagButton.offsetWidth}px`;
    const stop = anchorPopup(popup, tagButton, { gap: 3 });
    tagButton.setAttribute("aria-expanded", "true");
    const onDown = (event) => {
      if (!popup.contains(event.target) && !tagButton.contains(event.target)) closeTagMenu({ restoreFocus: false });
    };
    document.addEventListener("pointerdown", onDown, true);
    tagMenu = { popup, stop, onDown };
    popup.addEventListener("keydown", onTagKey);
    (options.find((o) => o.getAttribute("aria-selected") === "true") || options[0]).focus();
  }

  // Keys of the tag filter: on the button, ArrowDown opens the popup; in the popup the arrows move, Escape closes.
  function onTagKey(event) {
    if (!tagMenu) {
      if (event.target === tagButton && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
        event.preventDefault();
        openTagMenu();
      }
      return;
    }
    const options = [...tagMenu.popup.querySelectorAll('[role="option"]')];
    const at = options.indexOf(document.activeElement);
    if (event.key === "Escape") {
      event.preventDefault();
      closeTagMenu();
    } else if (event.key === "Tab") {
      closeTagMenu({ restoreFocus: false });
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const next = event.key === "Home" ? 0 : event.key === "End" ? options.length - 1 : (at + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length;
      options[next].focus();
    }
  }

  function openSortMenu() {
    const choose = (value) => () => {
      sort = value;
      writePref(SORT_KEY, sort);
      render();
    };
    const popup = openMenu(sortButton, [
      { label: SORT_LABELS.az, checked: sort === "az", onSelect: choose("az") },
      { label: SORT_LABELS.za, checked: sort === "za", onSelect: choose("za") },
    ], { gap: 3, testid: "list-sort-menu" });
    popup.style.minWidth = `${sortButton.offsetWidth}px`;
  }

  function renderFilters(shown) {
    filters.hidden = !shown;
    const counts = tagCounts();
    const active = counts.find(([t]) => fold(t) === fold(query));
    clear(tagButton);
    tagButton.append(icon("tag", 16));
    if (active) tagButton.append(el("span", { class: "list-select__text" }, tagChip(active[0])));
    else tagButton.append(el("span", { class: "list-select__text list-select__text--placeholder", text: "Filter by tag" }));
    tagButton.append(icon("chevron-down", 16));
    tagButton.disabled = counts.length === 0;
    sortLabel.textContent = sort ? SORT_LABELS[sort] : "Sort";
    sortLabel.classList.toggle("list-select__text--placeholder", !sort);
  }

  // One empty state for an empty list and for a filter with no result: a picture, a title, a hint and the main action.
  function emptyBox({ kind, title, text, actions, testid }) {
    const box = el("div", { class: "list-empty", role: "status", dataset: { testid } }, illustration(kind), el("p", { class: "list-empty__title", text: title }));
    if (text) box.append(el("p", { class: "list-empty__text", text }));
    if (actions.length) box.append(el("div", { class: "list-empty__actions" }, ...actions));
    return box;
  }

  function renderEmpty() {
    const actions = [];
    let text;
    if (canWrite()) {
      actions.push(button("New dashboard", { dataset: { testid: "new-dashboard" }, on: { click: () => callEditor("folders", "newDashboard") } }, "plus", { variant: "tb-primary" }));
      actions.push(button("Import", { dataset: { testid: "import" }, on: { click: () => callEditor("import_export", "importDialog") } }, "upload", { variant: "tb" }));
      text = "Create a dashboard, or import a Grafana JSON file.";
    } else if (editing && folderAccess.create) {
      actions.push(button("New folder", { dataset: { testid: "new-folder" }, on: { click: () => callEditor("folders", "newFolder") } }, "folder-plus", { variant: "tb-primary" }));
      text = "Create a folder first, then add dashboards to it.";
    } else if (session.mode === "authzen") text = "Ask an administrator to give you access to a folder.";
    else text = "A dashboard shows here when it is added to the store of Hune.";
    body.append(emptyBox({
      kind: "empty",
      title: session.mode === "authzen" ? "No dashboard is visible to you." : "No dashboard yet.",
      text,
      actions,
      testid: "list-empty",
    }));
  }

  // The folder title above a group: the folder icon and the name, with the menu of the folder when the user can write in it.
  function folderHead(group) {
    const head = el("h2", { class: "group__head" }, icon("folder", 14), el("span", { class: "group__title", text: group.folder.title }));
    if (editing && group.folder.access && group.folder.access.write && folders.some((f) => f.id === group.folder.id)) {
      const menuButton = button(`Actions for the folder ${group.folder.title}`, { "aria-haspopup": "menu", "aria-expanded": "false", class: "btn btn--ghost btn--icon group__menu", dataset: { tooltipAlign: "end" } }, "more", { iconOnly: true, variant: "ghost" });
      menuButton.addEventListener("click", () => openMenu(menuButton, [
        { label: "Rename", icon: "edit", onSelect: () => callEditor("folders", "renameFolder", group.folder) },
        { separator: true },
        { label: "Delete", icon: "trash", danger: true, onSelect: () => callEditor("folders", "deleteFolder", group.folder) },
      ], { align: "end" }));
      head.append(menuButton);
    }
    return head;
  }

  function render() {
    closeTagMenu({ restoreFocus: false });
    clear(body);
    body.removeAttribute("aria-busy");
    // The sort and the tag filter show only above the threshold of the session. Without them, the page uses the default order, and the
    // text q of the address has no effect: a hidden control never changes the page.
    const shown = items.length > 0 && filtersShown(session, items.length);
    renderFilters(shown);
    const active = shown ? query : "";
    status.textContent = active ? `Showing the dashboards with the tag ${active}.` : "";
    if (!items.length) {
      renderEmpty();
      return;
    }
    const descending = shown && sort === "za";
    const groups = groupDashboards(folders, items, { descending });
    const visible = groups.map((g) => ({ ...g, items: g.items.filter((item) => matchesTag(item, active)) }));
    if (!visible.some((g) => g.items.length)) {
      body.append(emptyBox({
        kind: "match",
        title: "No dashboard has this tag.",
        text: null,
        actions: [button("Clear the filter", { dataset: { testid: "list-no-match-clear" }, on: { click: () => setQuery("") } }, null, { variant: "tb" })],
        testid: "list-no-match",
      }));
      return;
    }
    // A folder title shows only when the user sees two folders or more (9.7).
    const showFolders = groups.length >= 2;
    const stack = el("div", { class: "list-stack" });
    for (const group of visible) {
      if (!group.items.length && active) continue;
      const section = el("section", { class: "group", dataset: { testid: `folder-${group.folder.id}` } });
      if (showFolders) section.append(folderHead(group));
      const cards = el("div", { class: "cards" });
      if (group.items.length) for (const item of group.items) cards.append(dashboardCard(item, shown));
      else cards.append(el("p", { class: "group__empty muted", text: "This folder is empty." }));
      section.append(cards);
      stack.append(section);
    }
    body.append(stack);
  }

  // Loads all pages of the folders and the dashboards. After the first load, and before the cards exist, the router can take over
  // (ctx.afterLoad, the entry redirect). The function returns true when it did. A later load (import, delete) never asks again.
  async function load() {
    try {
      const [folderAnswer, list] = await Promise.all([api.folders(controller.signal), api.dashboards(controller.signal)]);
      folders = folderAnswer.items || [];
      folderAccess = folderAnswer.access || { create: false };
      items = list;
      if (!loadedOnce) {
        loadedOnce = true;
        if (typeof ctx.afterLoad === "function" && ctx.afterLoad({ items, folders }) === true) return true;
      }
      buildToolbar();
      render();
      return false;
    } catch (error) {
      if (api.isAbort(error)) return false;
      if (error.status === 401) return false;
      clear(body);
      status.textContent = `The list did not load: ${error.message}`;
      throw error;
    }
  }

  if (await load() === true) return { destroy() { controller.abort(); } };
  return {
    destroy() {
      controller.abort();
      closeTagMenu({ restoreFocus: false });
    },
  };
}
