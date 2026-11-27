// The list page (9.7).

import * as api from "./api.js";
import { loadEditorModule } from "./editor_loader.js";
import { dashboardPath, listSearch, parseSearch } from "./url_state.js";
import { absoluteTime, bindMenu, button, clear, closeMenu, el, icon, notify, openMenu, uniqueId } from "./ui.js";
import { READ_ONLY_TEXT, STALE_CARD_TEXT, cardMarks, compareText, fold, groupByStore, groupDashboards, rootGroupCount, rootWritable } from "./stores.js";
import { exportDashboard } from "./export_file.js";

export { groupDashboards };

const SORT_KEY = "hune.list.sort";

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

// What a card offers to the user (9.7). The pencil and the bin share one condition: editing is on and the item has the write right. Export
// and Download have their own flag, `access.export` (6.12).
export function cardRights(item, editing) {
  return { write: editing === true && Boolean(item.access && item.access.write), export: Boolean(item.access && item.access.export === true) };
}

// The badge of the last change on a card (9.7): the short age and the full time, or null. Hune knows a change only when it records one: the
// system must allow editing (`session.editing`), and the dashboard must have a version that Hune saved. A read-only system has no change:
// the date of a file is not a change. The version 0 (no version in the history) and the author `external` (a change outside Hune, with the
// date of the file) give no badge. A member that the list does not give gives no badge.
export function lastChange(item, editing, now = Date.now()) {
  if (editing !== true) return null;
  if (!Number.isSafeInteger(item.version) || item.version < 1) return null;
  if (typeof item.updated_by !== "string" || item.updated_by === "" || item.updated_by === "external") return null;
  const age = shortAge(item.updated_at, now);
  return age ? { age, title: absoluteTime(item.updated_at) } : null;
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
// The button of the sort shows the order that applies now, in short. No saved choice means the order A–Z (the default order of the page).
const SORT_SHORT = { az: "A–Z", za: "Z–A" };

export async function mount(ctx) {
  const controller = new AbortController();
  const { main, header, session } = ctx;
  const editing = session.editing === true;
  let folders = [];
  let folderAccess = { create: false };
  // The member `root` of the answer of GET /api/folders: the right to write at the root, and the store of the root (an older server has none).
  let folderRoot;
  let items = [];
  let query = parseSearch(window.location.search).q || "";
  let sort = readPref(SORT_KEY, ["az", "za"], null);
  let loadedOnce = false;

  header.clear();
  document.title = "Dashboards - Hune";

  // The one button of the actions of the page, as the button "Upload" of Stowr: a primary button with a plus, its name "New" and a chevron. Its menu
  // has "New dashboard", "New folder" and "Import", only the ones that the user may use (9.7). The empty state shows the same button.
  function newEntries() {
    const entries = [];
    if (canWrite()) entries.push({ label: "New dashboard", icon: "plus", testid: "new-dashboard", onSelect: () => callEditor("folders", "newDashboard") });
    if (editing && folderAccess.create) entries.push({ label: "New folder", icon: "folder-plus", testid: "new-folder", onSelect: () => callEditor("folders", "newFolder") });
    if (canWrite()) entries.push({ label: "Import", icon: "upload", testid: "import", onSelect: () => callEditor("import_export", "importDialog") });
    return entries;
  }
  function makeNewButton(testid) {
    const node = button("New", { class: "btn btn--tb-primary list-new", "aria-haspopup": "menu", "aria-expanded": "false", dataset: { testid } }, "plus", { variant: "tb-primary" });
    node.append(icon("chevron-down", 16));
    bindMenu(node, newEntries, { align: "end", gap: 4, testid: "list-new-menu-popup" });
    return node;
  }
  const newMenu = makeNewButton("list-new-menu");
  newMenu.hidden = true;
  // The live status says only the tag filter. It has no count (9.7).
  const status = el("p", { class: "list-status visually-hidden", role: "status" });

  const tagButton = el("button", { type: "button", class: "selector list-select--tags", "aria-label": "Filter by tag", dataset: { testid: "list-tag-filter" } });
  const sortLabel = el("span", { class: "selector__text" });
  const sortButton = el("button", { type: "button", class: "selector list-select--sort", "aria-label": "Sort", dataset: { testid: "list-sort" } },
    glyph(SORT_ARROWS, 16), sortLabel, icon("chevron-down", 16));
  // The sort and the tag filter are the start controls of the header: they follow the brand (9.6, 9.7).
  const filters = el("div", { class: "list-filters" }, sortButton, tagButton);

  const body = el("div", { class: "list-body", dataset: { testid: "list" }, "aria-busy": "true" },
    el("div", { class: "list-skeleton", "aria-hidden": "true" }, el("span", { class: "skeleton" }), el("span", { class: "skeleton" }), el("span", { class: "skeleton" })));
  clear(main);
  // The page has no visible title (9.7), but it has one heading for assistive technology: the name of the page.
  const page = el("section", { class: "list-page" }, el("h1", { class: "visually-hidden", text: "Dashboards" }), status, body);
  main.append(page);
  header.setStart(filters);
  header.setActions(newMenu);
  filters.hidden = true;

  const editorCtx = () => ({ session, folders, root: folderRoot, dashboards: items, canCreateFolder: editing && folderAccess.create === true, navigate: ctx.navigate, reload: load });

  async function callEditor(name, fn, ...args) {
    const mod = await loadEditorModule(name);
    if (mod && typeof mod[fn] === "function") await mod[fn](editorCtx(), ...args);
    else if (mod) notify({ level: "warning", text: "The editor is not available in this build." });
  }

  const canWrite = () => editing && (folders.some((f) => f.access && f.access.write) || rootWritable(folderRoot));

  // New folder, Import and New dashboard (the one primary action of the page).
  function buildToolbar() {
    // An empty list has the button in its empty state. The header shows it when the list has a dashboard and the user may use an entry.
    newMenu.hidden = !items.length || newEntries().length === 0;
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
    const card = el("article", { class: "card", dataset: { testid: `row-${item.uid}`, state: item.state || "valid" } });
    // The head row: the title at the left, then the badge of the last change (only when Hune recorded a change), then the three dots.
    const change = lastChange(item, editing);
    const badge = change ? el("span", { class: "card__badge", title: change.title, dataset: { testid: `card-change-${item.uid}` } }, icon("clock", 12), el("span", { text: `Last change ${change.age}` })) : null;
    // The marks at the right of the head row: the mark of a file that is not valid now, and the lock of a read-only store. Each is a picture with a name for
    // assistive technology and a tooltip. A card has the same height with and without them.
    const marks = cardMarks(item);
    const markNodes = [];
    if (marks.stale) markNodes.push(el("span", { class: "card__mark card__mark--stale", role: "img", "aria-label": STALE_CARD_TEXT, "data-tooltip": STALE_CARD_TEXT, "data-tooltip-align": "end", dataset: { testid: `card-stale-${item.uid}` } }, icon("warning", 16)));
    if (marks.readOnly) markNodes.push(el("span", { class: "card__mark card__mark--lock", role: "img", "aria-label": READ_ONLY_TEXT, "data-tooltip": READ_ONLY_TEXT, "data-tooltip-align": "end", dataset: { testid: `card-lock-${item.uid}` } }, icon("lock", 16)));
    if (valid) {
      card.append(el("div", { class: "card__top" }, el("h3", { class: "card__title" }, el("a", { class: "card__link", href: dashboardPath(item.uid, item.title), title: item.title, text: item.title })), badge, ...markNodes));
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
      card.append(el("div", { class: "card__top" }, el("h3", { class: "card__title card__title--invalid" }, icon("error", 14), el("code", { class: "card__uid", text: item.uid }), el("span", { class: "badge badge--error", text: "Not valid" })), badge, ...markNodes), el("div", { class: "card__errors" }, toggle, detail));
    }
    card.append(el("div", { class: "card__tags" }, ...(item.tags || []).map((t) => tagCardButton(t, interactiveTags))));
    // The three dots are the one menu of the card, at the right end of the head row, after the badge. A card that the user can edit offers Edit,
    // Export, Move and Delete; a read-only card has no menu. The server decides: a 403 shows the refusal. Edit opens the dashboard in the edit mode.
    // Export and Download need `access.export` (6.12): an item without it has no such entry.
    const exportOk = cardRights(item, editing).export;
    let entries = null;
    if (valid && rights.write) {
      entries = [
        { label: "Edit", icon: "edit", href: `${dashboardPath(item.uid, item.title)}?edit=1`, testid: `card-edit-${item.uid}` },
        ...(exportOk ? [{ label: "Export", icon: "download", onSelect: () => exportDashboard(item.uid) }] : []),
        { label: "Move", icon: "folder", onSelect: () => callEditor("folders", "moveDashboard", item) },
        { label: "Delete", icon: "trash", danger: true, testid: `card-delete-${item.uid}`, onSelect: () => callEditor("folders", "deleteDashboard", item) },
      ];
    } else if (!valid) {
      entries = [];
      if (exportOk) entries.push({ label: "Download", icon: "download", onSelect: () => exportDashboard(item.uid) });
      if (rights.write) entries.push({ label: "Replace", icon: "upload", onSelect: () => callEditor("import_export", "importDialog", { uid: item.uid, etag: item.etag }) });
    }
    if (entries && entries.length) {
      const more = cardButton(`Actions for ${valid ? item.title : item.uid}`, `row-menu-${item.uid}`, "more", { class: "btn btn--ghost btn--icon card__more", "aria-haspopup": "menu", "aria-expanded": "false" });
      more.addEventListener("click", () => openMenu(more, entries, { align: "end", gap: 4 }));
      card.querySelector(".card__top").append(more);
    }
    return card;
  }

  // The tags of all dashboards with the number of dashboards: the most used first, then in name order (the order of Grafana).
  function tagCounts() {
    const counts = new Map();
    for (const item of items) for (const t of item.tags || []) counts.set(t, (counts.get(t) || 0) + 1);
    return [...counts].sort((a, b) => b[1] - a[1] || compareText(a[0], b[0]));
  }

  // The tag filter: a select-like button that shows the active tag as its chip, and a joined menu with a chip and a count for each tag (the
  // menu of the host selector of ClickHouse Dash). The filter is the text q (9.7): a tag sets it, a second choice of the same tag clears it.
  const tagItems = () => tagCounts().map(([text, n]) => ({
    key: text, label: text, node: tagChip(text), meta: n, checked: fold(query) === fold(text), testid: `tag-filter-${text}`, onSelect: () => toggleTag(text),
  }));
  bindMenu(tagButton, tagItems, { joined: true, testid: "list-tag-menu" });

  // The sort: the two orders of the names, in the same joined menu.
  const choose = (value) => () => {
    sort = value;
    writePref(SORT_KEY, sort);
    render();
  };
  bindMenu(sortButton, () => [
    { label: SORT_LABELS.az, checked: sort !== "za", onSelect: choose("az") },
    { label: SORT_LABELS.za, checked: sort === "za", onSelect: choose("za") },
  ], { joined: true, testid: "list-sort-menu" });

  function renderFilters(shown) {
    filters.hidden = !shown;
    const counts = tagCounts();
    const active = counts.find(([t]) => fold(t) === fold(query));
    clear(tagButton);
    tagButton.append(icon("tag", 16));
    if (active) tagButton.append(el("span", { class: "selector__text" }, tagChip(active[0])));
    else tagButton.append(el("span", { class: "selector__text selector__text--placeholder", text: "Filter by tag" }));
    tagButton.append(icon("chevron-down", 16));
    tagButton.disabled = counts.length === 0;
    sortLabel.textContent = SORT_SHORT[sort === "za" ? "za" : "az"];
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
      actions.push(makeNewButton("new-menu-empty"));
      text = "Create a dashboard, or import a Grafana JSON file.";
    } else if (editing && folderAccess.create) {
      actions.push(makeNewButton("new-menu-empty"));
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
  function folderHead(group, nested = false) {
    const head = el("h2", { class: `group__head${nested ? " group__head--nested" : ""}` }, icon("folder", 14), el("span", { class: "group__title", text: group.folder.title }));
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
    // The groups at the root are the folders of the primary store and the read-only stores (stores.js). A title shows above each root group only when
    // the user sees two or more (9.7). The folders of a read-only store show under its title; when that title is hidden (it is the only root group),
    // they show only for two folders or more.
    const split = groupByStore(visible, session, { descending });
    const showRoots = rootGroupCount(split) >= 2;
    const stack = el("div", { class: "list-stack" });
    const addGroup = (group, showTitle, nested = false) => {
      if (!group.items.length && active) return;
      // A root group (dashboards with no folder) never has a title: it is the top of the page, or the top of the block of a read-only store.
      const isRoot = group.folder.root === true;
      const section = el("section", { class: `group${nested ? " group--nested" : ""}${isRoot ? " group--root" : ""}`, dataset: { testid: isRoot ? `root-${group.folder.store || "primary"}` : `folder-${group.folder.id}` } });
      if (showTitle && !isRoot) section.append(folderHead(group, nested));
      const cards = el("div", { class: "cards" });
      if (group.items.length) for (const item of group.items) cards.append(dashboardCard(item, shown));
      else cards.append(el("p", { class: "group__empty muted", text: "This folder is empty." }));
      section.append(cards);
      stack.append(section);
    };
    for (const group of split.primary) addGroup(group, showRoots);
    for (const block of split.stores) {
      const folderCount = block.groups.filter((g) => g.items.length || !active).length;
      if (!folderCount) continue;
      if (showRoots) stack.append(el("h2", { class: "group__head group__head--store", dataset: { testid: `store-${block.store.id}` } }, icon("lock", 14), el("span", { class: "group__title", text: block.store.title })));
      const showFolderTitles = showRoots || folderCount >= 2;
      for (const group of block.groups) addGroup(group, showFolderTitles, showRoots);
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
      folderRoot = folderAnswer.root;
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
      closeMenu({ restoreFocus: false });
    },
  };
}
