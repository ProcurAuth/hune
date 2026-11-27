// The stores of the server in the page (9.7, 9.8). The process has several stores. One is the primary store: the page writes only there. The others are
// read-only. Each read-only store shows as one root group with its title; the primary shows at the root with no title. The functions here are pure,
// so a test runs them without a browser. The texts are the one place of the words that the page says about a read-only store and a stale file.

export const STALE_CARD_TEXT = "The file is not valid. The last valid version is shown.";
export const STALE_PAGE_TEXT = "The file of this dashboard is not valid. The last valid version is shown.";
export const READ_ONLY_ERROR_TEXT = "This dashboard is in a read-only store. It cannot be changed.";

// True when the member `root` of the answer of GET /api/folders says that the user can write at the root (a dashboard with no folder). An older server
// has no member `root`: false. The right never comes from a folder.
export const rootWritable = (root) => Boolean(root) && Boolean(root.access) && root.access.write === true;

// True for the group of the dashboards with no folder (groupDashboards below). Its title never shows.
export const isRootGroup = (group) => Boolean(group) && Boolean(group.folder) && group.folder.root === true;

// True for an item or a folder that the server marks as part of a read-only store.
export const isReadOnly = (entity) => Boolean(entity) && entity.read_only === true;

// True for a dashboard whose file is not valid now: the page shows the last valid state.
export const isStale = (entity) => Boolean(entity) && typeof entity.stale === "string" && entity.stale !== "";

// The stores of the session by id: { id, kind, title, primary, read_only }. An older server has no member `stores`: the table is empty and every
// folder belongs to the primary.
export function storeTable(session) {
  const table = new Map();
  const list = session && Array.isArray(session.stores) ? session.stores : [];
  for (const s of list) if (s && typeof s.id === "string") table.set(s.id, s);
  return table;
}

// ASCII-only lower case (9.7): non-ASCII letters keep their case.
export const fold = (text) => String(text ?? "").replace(/[A-Z]/g, (c) => c.toLowerCase());

export const compareText = (a, b) => {
  const x = fold(a);
  const y = fold(b);
  return x < y ? -1 : x > y ? 1 : 0;
};

// Groups the dashboards by folder: the root groups first, then the folders in title order, dashboards in title order (9.7).
// A dashboard with no folder (`folder` null) is in a ROOT group. There is one root group for each store (the store of the dashboard), so the root of
// the primary store and the root of a read-only store stay apart. A root group is { folder: { id: null, root: true, title: "Root", access: { write: false },
// read_only, store }, items }; its title never shows. { descending: true } is the viewer preference "Z-A": the folders and the dashboards in the reverse
// order; the root groups stay first.
export function groupDashboards(folders, items, { descending = false } = {}) {
  const byFolder = new Map(folders.map((f) => [f.id, { folder: f, items: [] }]));
  const roots = new Map();
  for (const item of items) {
    if (item.folder === null) {
      const store = typeof item.store === "string" ? item.store : "";
      if (!roots.has(store)) roots.set(store, { folder: { id: null, root: true, title: "Root", access: { write: false }, read_only: item.read_only, store: item.store }, items: [] });
      roots.get(store).items.push(item);
      continue;
    }
    if (!byFolder.has(item.folder)) byFolder.set(item.folder, { folder: { id: item.folder, title: item.folder, access: { write: false }, read_only: item.read_only, store: item.store }, items: [] });
    byFolder.get(item.folder).items.push(item);
  }
  const rootGroups = [...roots.values()].sort((a, b) => compareText(a.folder.store ?? "", b.folder.store ?? ""));
  const groups = [...byFolder.values()].sort((a, b) => compareText(a.folder.title, b.folder.title) || compareText(a.folder.id, b.folder.id));
  for (const group of [...rootGroups, ...groups]) group.items.sort((a, b) => compareText(a.title, b.title) || compareText(a.uid, b.uid));
  if (descending) {
    groups.reverse();
    for (const group of [...rootGroups, ...groups]) group.items.reverse();
  }
  return [...rootGroups, ...groups];
}

// Splits the folder groups of the list (groupDashboards) by store. `groups` are { folder, items } in the order of the page. The result is
// { primary: groups, stores: [{ store: { id, title }, groups }] }: the groups of the primary at the root, then one block for each read-only store
// in title order (the reverse order for "Z-A"). A folder of a read-only store keeps its place in the order of the groups inside its block.
export function groupByStore(groups, session, { descending = false } = {}) {
  const table = storeTable(session);
  const primary = [];
  const blocks = new Map();
  for (const group of groups) {
    const folder = group.folder || {};
    const readOnly = isReadOnly(folder) || (typeof folder.store === "string" && table.has(folder.store) && table.get(folder.store).read_only === true);
    if (!readOnly) {
      primary.push(group);
      continue;
    }
    const id = typeof folder.store === "string" ? folder.store : "";
    if (!blocks.has(id)) {
      const known = table.get(id);
      blocks.set(id, { store: { id, title: known && typeof known.title === "string" && known.title ? known.title : id }, groups: [] });
    }
    blocks.get(id).groups.push(group);
  }
  const stores = [...blocks.values()].sort((a, b) => compareText(a.store.title, b.store.title) || compareText(a.store.id, b.store.id));
  if (descending) stores.reverse();
  return { primary, stores };
}

// The number of groups at the root: the folders of the primary and the read-only stores. A title shows above each of them only when there are two or
// more (9.7). Inside a read-only store, the folder titles show under its title; when the store title is hidden, they show only for two folders or more.
export function rootGroupCount(split) {
  return split.primary.length + split.stores.length;
}

// What a card shows beside the badge of the last change: the mark of a file that is not valid. A read-only store has no mark.
export function cardMarks(item) {
  return { stale: isStale(item) };
}
