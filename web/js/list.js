// The list page (9.7).

import * as api from "./api.js";
import { loadEditorModule } from "./editor_loader.js";
import { dashboardPath, listSearch, parseSearch } from "./url_state.js";
import { absoluteTime, anchorPopup, button, clear, closeMenu, el, icon, notify, openMenu, relativeTime, uniqueId } from "./ui.js";

const COLLAPSE_KEY = "hune.list.collapsed";
const LAYOUT_KEY = "hune.list.layout";
const SORT_KEY = "hune.list.sort";

// ASCII-only lower case (9.7): non-ASCII letters keep their case.
const fold = (text) => String(text ?? "").replace(/[A-Z]/g, (c) => c.toLowerCase());

function readCollapsed() {
  try {
    const raw = window.localStorage.getItem(COLLAPSE_KEY);
    const list = raw ? JSON.parse(raw) : [];
    return new Set(Array.isArray(list) ? list.filter((x) => typeof x === "string") : []);
  } catch (error) {
    return new Set();
  }
}

function writeCollapsed(set) {
  try {
    window.localStorage.setItem(COLLAPSE_KEY, JSON.stringify([...set]));
  } catch (error) {
    // The state then lasts for this page only.
  }
}

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

export function matchesQuery(item, text) {
  if (!text) return true;
  const needle = fold(text);
  return fold(item.title).includes(needle) || fold(item.uid).includes(needle) || (item.tags || []).some((t) => fold(t).includes(needle));
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

const FOLDER_OPEN = ["M3 19V6a1 1 0 0 1 1-1h5l2 2h7a1 1 0 0 1 1 1v2", "M3 19l3-8.5a1 1 0 0 1 1-.5h14.5a.5.5 0 0 1 .5.7L19 18.5a1 1 0 0 1-1 .5H3z"];
const LIST_LINES = ["M9 6h12", "M9 12h12", "M9 18h12", "M4 6h.01", "M4 12h.01", "M4 18h.01"];
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
// Grafana 12.4.2). Each colour holds white text with at least 4.7:1. The name is the only input: the same tag has the same colour
// in every row, on every page and in every session.
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
  let layout = readPref(LAYOUT_KEY, ["folders", "list"], "folders");
  let sort = readPref(SORT_KEY, ["az", "za"], null);
  let loadedOnce = false;
  const collapsed = readCollapsed();

  header.clear();
  document.title = "Dashboards - Hune";

  const searchInput = el("input", {
    type: "search",
    class: "list-search__input",
    id: "list-search",
    value: query,
    placeholder: "Search by title, uid or tag",
    autocomplete: "off",
    dataset: { testid: "list-search" },
    "aria-label": "Search dashboards",
  });
  const clearButton = el("button", { type: "button", class: "list-search__clear", hidden: !query, dataset: { testid: "list-search-clear" }, on: { click: () => { setQuery(""); searchInput.focus(); } } },
    icon("close", 14), el("span", { text: "Clear" }));
  // The search field is in the header (9.6, 9.7). Its fit width is its width in one row; it grows when the controls take a row of their own.
  const searchBox = el("div", { class: "list-search", dataset: { fitWidth: "240" } }, el("span", { class: "list-search__icon", "aria-hidden": "true" }, icon("search", 16)), searchInput, clearButton);

  const actionsBox = el("div", { class: "list-toolbar__actions" });
  // The actions are in the header. Up to 768 px the three actions hide and one button "New" opens a menu with them. The menu runs the same buttons.
  const newMenu = button("New", { class: "btn btn--tb list-new", "aria-haspopup": "menu", "aria-expanded": "false", hidden: true, dataset: { testid: "list-new-menu" } }, null, { variant: "tb" });
  newMenu.append(icon("chevron-down", 16));
  newMenu.addEventListener("click", () => {
    const items = [...actionsBox.querySelectorAll("[data-toolbar-action]")].reverse().map((node) => ({ label: node.textContent.trim(), onSelect: () => node.click() }));
    if (items.length) openMenu(newMenu, items, { align: "end", gap: 4, testid: "list-new-menu-popup" });
  });
  const status = el("p", { class: "list-status visually-hidden", role: "status" });
  const head = el("div", { class: "list-head" },
    el("div", { class: "list-head__text" }, el("h1", { class: "page-title", text: "Dashboards" }), el("p", { class: "list-subtitle", text: "Create and manage dashboards to visualize your data" }), status));

  const tagButton = el("button", { type: "button", class: "list-select list-select--tags", "aria-haspopup": "listbox", "aria-expanded": "false", "aria-label": "Filter by tag", dataset: { testid: "list-tag-filter" }, on: { click: () => openTagMenu() } });
  const layoutItems = [
    { value: "folders", title: "Folder view", testid: "list-layout-folders", glyph: () => icon("folder", 16) },
    { value: "list", title: "List view", testid: "list-layout-list", glyph: () => glyph(LIST_LINES, 16) },
  ];
  const layoutNodes = new Map();
  const layoutControl = el("div", { class: "list-seg", role: "group", "aria-label": "View" });
  for (const item of layoutItems) {
    const node = el("button", { type: "button", class: "list-seg__item", "aria-label": item.title, dataset: { testid: item.testid }, on: { click: () => {
      if (layout === item.value) return;
      layout = item.value;
      writePref(LAYOUT_KEY, layout === "folders" ? null : layout);
      render();
    } } }, item.glyph());
    node.setAttribute("data-tooltip", item.title);
    layoutNodes.set(item.value, node);
    layoutControl.append(node);
  }
  tagButton.addEventListener("keydown", onTagKey);
  const sortLabel = el("span", { class: "list-select__text" });
  const sortButton = el("button", { type: "button", class: "list-select list-select--sort", "aria-haspopup": "menu", "aria-expanded": "false", "aria-label": "Sort", dataset: { testid: "list-sort" }, on: { click: () => openSortMenu() } },
    glyph(SORT_ARROWS, 16), sortLabel, icon("chevron-down", 16));
  const filters = el("div", { class: "list-filters" }, tagButton, layoutControl, sortButton);

  const body = el("div", { class: "list-body", dataset: { testid: "list" }, "aria-busy": "true" },
    el("div", { class: "list-skeleton", "aria-hidden": "true" }, el("span", { class: "skeleton" }), el("span", { class: "skeleton" }), el("span", { class: "skeleton" })));
  clear(main);
  const page = el("section", { class: "list-page", dataset: { layout } }, head, body);
  main.append(page);
  // The controls of the page are in the header (9.6): the search, the filters, the actions and the menu "New" (a phone).
  header.setActions(searchBox, filters, actionsBox, newMenu);
  searchBox.hidden = true;
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

  // A click on a tag filters the list by this tag. A second click on the same tag clears the filter.
  function setQuery(text) {
    query = text;
    searchInput.value = text;
    clearButton.hidden = !text;
    window.history.replaceState(null, "", `${window.location.pathname}${listSearch(query)}`);
    clearTimeout(debounce);
    render();
  }

  const toggleTag = (text) => setQuery(fold(query) === fold(text) ? "" : text);

  // A chip with the colour of its name. In a row it is a button that filters; in the tag filter it is only a picture.
  const tagChip = (text) => el("span", { class: "list-tag", style: { "--tag-bg": tagColor(text) }, text });

  function tagRowButton(text) {
    return el("button", { type: "button", class: "tag tag--button list-tag", style: { "--tag-bg": tagColor(text) }, text, title: `Filter by the tag ${text}`, on: { click: () => toggleTag(text) } });
  }

  function dashboardRow(item, flat) {
    const valid = item.state !== "invalid";
    const cells = [];
    const writable = editing && item.access && item.access.write;
    if (valid) {
      cells.push(el("td", { class: "cell-title" }, el("span", { class: "cell-title__inner" }, icon("grid", 14), el("a", { class: "cell-title__link", href: dashboardPath(item.uid, item.title), title: item.title, text: item.title }))));
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
      cells.push(el("td", { class: "cell-title" }, el("span", { class: "cell-title__inner" }, icon("error", 14), el("code", { class: "cell-title__uid", text: item.uid }), el("span", { class: "badge badge--error", text: "Not valid" }), toggle), detail));
    }
    if (flat) {
      const folder = folders.find((f) => f.id === item.folder);
      cells.push(el("td", { class: "cell-folder" }, el("span", { class: "cell-folder__inner" }, icon("folder", 14), el("span", { class: "cell-folder__text", text: folder ? folder.title : item.folder }))));
    }
    cells.push(el("td", { class: "cell-tags" }, el("div", { class: "cell-tags__list" }, ...(item.tags || []).map(tagRowButton))));
    const by = item.updated_by && item.updated_by !== "-" ? item.updated_by : null;
    cells.push(el("td", { class: "cell-changed", title: absoluteTime(item.updated_at) }, relativeTime(item.updated_at), by ? el("span", { class: "cell-changed__by", text: ` by ${by}` }) : null));
    const menuItems = [];
    if (valid) {
      menuItems.push({ label: "Export", icon: "download", onSelect: () => exportDashboard(item.uid) });
      if (writable) {
        menuItems.push({ label: "Move", icon: "folder", onSelect: () => callEditor("folders", "moveDashboard", item) });
        menuItems.push({ separator: true });
        menuItems.push({ label: "Delete", icon: "trash", danger: true, onSelect: () => callEditor("folders", "deleteDashboard", item) });
      }
    } else {
      menuItems.push({ label: "Download", icon: "download", onSelect: () => exportDashboard(item.uid) });
      if (writable) menuItems.push({ label: "Replace", icon: "upload", onSelect: () => callEditor("import_export", "importDialog", { uid: item.uid, etag: item.etag }) });
    }
    const menuButton = button(`Actions for ${item.title}`, { "aria-haspopup": "menu", "aria-expanded": "false", dataset: { testid: `row-menu-${item.uid}`, tooltipAlign: "end" } }, "more", { iconOnly: true, variant: "ghost" });
    menuButton.addEventListener("click", () => openMenu(menuButton, menuItems, { align: "end" }));
    cells.push(el("td", { class: "cell-menu" }, menuButton));
    return el("tr", { dataset: { testid: `row-${item.uid}`, state: item.state || "valid" } }, ...cells);
  }

  // The tags of all dashboards with the number of dashboards: the most used first, then in name order (the order of Grafana).
  function tagCounts() {
    const counts = new Map();
    for (const item of items) for (const t of item.tags || []) counts.set(t, (counts.get(t) || 0) + 1);
    return [...counts].sort((a, b) => b[1] - a[1] || compareText(a[0], b[0]));
  }

  // The tag filter: a select-like button that shows the active tag as its chip, and a popup with a chip and a count for each tag.
  // The filter is the search text (9.7): a tag fills the field, a second choice of the same tag clears it.
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

  function renderFilters() {
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
    for (const [value, node] of layoutNodes) node.setAttribute("aria-pressed", String(value === layout));
    page.dataset.layout = layout;
  }

  // One empty state for an empty list and for a search with no result: a picture, a title, a hint and the main action.
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

  // The visible head of the columns. The tables below it keep their own hidden head for screen readers.
  function columnHead(flat) {
    const cell = (cls, text) => el("span", { class: `list-colhead__cell ${cls}`, text });
    return el("div", { class: "list-colhead", "aria-hidden": "true" }, cell("list-colhead__title", "Name"), flat ? cell("list-colhead__folder", "Folder") : null, cell("list-colhead__tags", "Tags"), cell("list-colhead__changed", "Last change"), cell("list-colhead__menu", ""));
  }

  function tableHead(flat) {
    return el("thead", { class: "visually-hidden" }, el("tr", {},
      el("th", { scope: "col", text: "Title" }), flat ? el("th", { scope: "col", text: "Folder" }) : null, el("th", { scope: "col", text: "Tags" }), el("th", { scope: "col", text: "Last change" }), el("th", { scope: "col", text: "Actions" })));
  }

  function render() {
    closeTagMenu({ restoreFocus: false });
    clear(body);
    body.removeAttribute("aria-busy");
    // With nothing to search or to create from the toolbar, the page shows the empty state only.
    searchBox.hidden = !items.length;
    filters.hidden = !items.length;
    renderFilters();
    if (!items.length) {
      status.textContent = "";
      renderEmpty();
      return;
    }
    const descending = sort === "za";
    const filtered = items.filter((item) => matchesQuery(item, query));
    const groups = groupDashboards(folders, filtered, { descending });
    const count = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
    status.textContent = filtered.length === items.length ? `${count(items.length, "dashboard")} in ${count(groups.length, "folder")}` : `${filtered.length} of ${count(items.length, "dashboard")}`;
    if (!filtered.length) {
      body.append(emptyBox({
        kind: "match",
        title: "No dashboard matches the search.",
        text: null,
        actions: [button("Clear search and filters", { dataset: { testid: "list-no-match-clear" }, on: { click: () => setQuery("") } }, null, { variant: "tb" })],
        testid: "list-no-match",
      }));
      return;
    }
    if (layout === "list") {
      const flat = groups.flatMap((g) => g.items).sort((a, b) => compareText(a.title, b.title) || compareText(a.uid, b.uid));
      if (descending) flat.reverse();
      body.append(columnHead(true), el("table", { class: "list-table list-table--flat" }, tableHead(true), el("tbody", {}, ...flat.map((item) => dashboardRow(item, true)))));
      return;
    }
    body.append(columnHead(false));
    for (const group of groups) {
      if (!group.items.length && query) continue;
      const open = !collapsed.has(group.folder.id);
      const listId = uniqueId("folder");
      const toggle = el("button", {
        type: "button",
        class: "group__toggle",
        "aria-expanded": String(open),
        "aria-controls": listId,
        on: { click: () => {
          if (collapsed.has(group.folder.id)) collapsed.delete(group.folder.id);
          else collapsed.add(group.folder.id);
          writeCollapsed(collapsed);
          render();
        } },
      }, icon(open ? "chevron-down" : "chevron-right", 16), open ? glyph(FOLDER_OPEN, 14) : icon("folder", 14), el("span", { class: "group__title", text: group.folder.title }),
      el("span", { class: "group__count visually-hidden" }, `${group.items.length} ${group.items.length === 1 ? "dashboard" : "dashboards"}`));
      const headRow = el("h2", { class: "group__head" }, toggle);
      if (editing && group.folder.access && group.folder.access.write && folders.some((f) => f.id === group.folder.id)) {
        const menuButton = button(`Actions for the folder ${group.folder.title}`, { "aria-haspopup": "menu", "aria-expanded": "false", class: "btn btn--ghost btn--icon group__menu", dataset: { tooltipAlign: "end" } }, "more", { iconOnly: true, variant: "ghost" });
        menuButton.addEventListener("click", () => openMenu(menuButton, [
          { label: "Rename", icon: "edit", onSelect: () => callEditor("folders", "renameFolder", group.folder) },
          { separator: true },
          { label: "Delete", icon: "trash", danger: true, onSelect: () => callEditor("folders", "deleteFolder", group.folder) },
        ], { align: "end" }));
        headRow.append(menuButton);
      }
      const table = el("table", { id: listId, class: "list-table", hidden: !open },
        el("colgroup", {}, el("col", { class: "col-title" }), el("col", { class: "col-tags" }), el("col", { class: "col-changed" }), el("col", { class: "col-menu" })),
        tableHead(false),
        el("tbody", {}, ...group.items.map((item) => dashboardRow(item, false))));
      body.append(el("section", { class: `group${open ? "" : " group--collapsed"}`, dataset: { testid: `folder-${group.folder.id}` } }, headRow, group.items.length ? table : el("p", { class: "group__empty muted", text: "This folder is empty.", hidden: !open })));
    }
  }

  // Loads all pages of the folders and the dashboards. After the first load, and before the rows exist, the router can take over
  // (ctx.afterLoad, the entry redirect). The function returns true when it did. A later load (search, import, delete) never asks again.
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

  let debounce = 0;
  searchInput.addEventListener("input", () => {
    query = searchInput.value;
    clearButton.hidden = !query;
    window.history.replaceState(null, "", `${window.location.pathname}${listSearch(query)}`);
    clearTimeout(debounce);
    debounce = setTimeout(render, 80);
  });
  searchInput.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && searchInput.value) {
      event.preventDefault();
      setQuery("");
    }
  });

  if (await load() === true) return { destroy() { controller.abort(); } };
  return {
    destroy() {
      controller.abort();
      clearTimeout(debounce);
      closeTagMenu({ restoreFocus: false });
    },
  };
}
