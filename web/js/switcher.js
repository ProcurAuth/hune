// The selector of the header of a dashboard (9.6). The button shows the title of the dashboard. Its joined menu lists the
// dashboards that the user can see, with one row for each folder that opens and closes the list of its dashboards (the switcher of Stowr).
// The menu is the one menu of the app (ui.js openMenu, joined). The page keeps no list of its own: each open reads the folders and the dashboards
// again, the menu shows the last answer at once, and the new answer replaces it when it comes. The page never waits for it to draw.

import * as api from "./api.js";
import { groupByStore, groupDashboards, isRootGroup, rootGroupCount } from "./stores.js";
import { dashboardPath } from "./url_state.js";
import { bindMenu, el, icon } from "./ui.js";

const MENU_TESTID = "header-switcher-menu";

// The last answer of the list and the state of the folder rows that the person opened or closed. Both last for the page load.
let tree = null;
const folderOpen = new Map();

// The session of the page, for the table of the stores. A call of locationNodes sets it.
let sessionRef = null;

async function readTree() {
  const [folders, items] = await Promise.all([api.folders(), api.dashboards()]);
  tree = { folders: folders.items || [], items, session: sessionRef };
  return tree;
}

// The items of the menu for an answer. The root rows are the dashboards with no folder, the folders of the primary store and the read-only stores
// (stores.js). One root group and it is the primary: the dashboards alone, as the list page shows no folder title for one folder (9.7). A read-only store is a row with a lock
// that opens its folders (its dashboards, for one folder). An entry that is not valid has no page to open: it is not in the menu. The row of the
// current dashboard is open until the person closes it.
export function switcherItems(data, uid, navigate) {
  const groups = groupDashboards(data.folders, data.items.filter((item) => item.state !== "invalid")).filter((group) => group.items.length);
  const split = groupByStore(groups, data.session || null);
  const entry = (item) => {
    const href = dashboardPath(item.uid, item.title);
    return { key: item.uid, label: item.title, title: item.title, href, current: item.uid === uid, testid: `switcher-item-${item.uid}`, onSelect: () => navigate(href) };
  };
  const isOpen = (key, items) => (folderOpen.has(key) ? folderOpen.get(key) : items.some((item) => item.uid === uid));
  const folderRow = (group) => {
    const key = `folder:${group.folder.id}`;
    return { group: true, key, label: group.folder.title, icon: "folder", open: isOpen(key, group.items), onToggle: (open) => folderOpen.set(key, open), children: group.items.map(entry) };
  };
  // The dashboards with no folder (a root group) are listed first, each one on its own with no folder row. A folder is a row that opens. With one
  // group only, the dashboards are listed alone; a root group and one folder stay apart: the folder keeps its row.
  const rowsOf = (groups) => groups.flatMap((group) => (isRootGroup(group) ? group.items.map(entry) : [folderRow(group)]));
  const storeChildren = (block) => (block.groups.length === 1 ? block.groups[0].items.map(entry) : rowsOf(block.groups));
  const roots = rootGroupCount(split);
  if (roots === 0) return [];
  if (roots === 1) return split.primary.length === 1 ? split.primary[0].items.map(entry) : storeChildren(split.stores[0]);
  return [
    ...rowsOf(split.primary),
    ...split.stores.map((block) => {
      const key = `store:${block.store.id}`;
      const all = block.groups.flatMap((group) => group.items);
      return { group: true, key, label: block.store.title, icon: "lock", open: isOpen(key, all), onToggle: (open) => folderOpen.set(key, open), children: storeChildren(block) };
    }),
  ];
}

// The nodes of the header for a dashboard page: the selector and, when the page shows a view of a panel or the panel editor, a way back (a link with
// an arrow, at the right of the selector). The views have no visible name. `back` is { href, label, attrs }: `attrs` can have a click handler and a test id.
export function locationNodes({ uid, title, navigate, back = null, session = null }) {
  sessionRef = session;
  const nodes = [];
  const trigger = el("button", { type: "button", class: "selector switcher", title, dataset: { testid: "header-switcher" } },
    icon("grid", 16), el("span", { class: "selector__text", text: title }), icon("chevron-down", 16));
  const items = () => {
    // The refresh starts at each open. A fresh answer fills the menu again while it is open, and never moves the focus away from its row.
    readTree().then((data) => {
      const popup = document.querySelector(`[data-testid="${MENU_TESTID}"]`);
      if (popup && popup.update) popup.update(switcherItems(data, uid, navigate));
    }).catch(() => {
      const popup = document.querySelector(`[data-testid="${MENU_TESTID}"]`);
      if (popup && popup.update && !tree) popup.update([{ label: "The list did not load", disabled: true }]);
    });
    return tree ? switcherItems(tree, uid, navigate) : [{ label: "Loading…", disabled: true }];
  };
  bindMenu(trigger, items, { joined: true, fit: true, testid: MENU_TESTID });
  nodes.push(trigger);
  if (back) {
    nodes.push(el("a", { class: "location__back btn btn--ghost btn--icon", href: back.href, title: back.label, "aria-label": back.label, ...(back.attrs || {}) }, icon("arrow-left", 16)));
  }
  return nodes;
}
