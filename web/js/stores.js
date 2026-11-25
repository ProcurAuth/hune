// The stores of the server in the page (9.7, 9.8). The process has several stores. One is the primary store: the page writes only there. The others are
// read-only. Each read-only store shows as one root group with its title; the primary shows at the root with no title. The functions here are pure,
// so a test runs them without a browser. The texts are the one place of the words that the page says about a read-only store and a stale file.

export const READ_ONLY_TEXT = "Read-only store";
export const STALE_CARD_TEXT = "The file is not valid. The last valid version is shown.";
export const STALE_PAGE_TEXT = "The file of this dashboard is not valid. The last valid version is shown.";
export const READ_ONLY_ERROR_TEXT = "This dashboard is in a read-only store. It cannot be changed.";

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

const fold = (text) => String(text ?? "").replace(/[A-Z]/g, (c) => c.toLowerCase());
const compare = (a, b) => {
  const x = fold(a);
  const y = fold(b);
  return x < y ? -1 : x > y ? 1 : 0;
};

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
  const stores = [...blocks.values()].sort((a, b) => compare(a.store.title, b.store.title) || compare(a.store.id, b.store.id));
  if (descending) stores.reverse();
  return { primary, stores };
}

// The number of groups at the root: the folders of the primary and the read-only stores. A title shows above each of them only when there are two or
// more (9.7). Inside a read-only store, the folder titles show under its title; when the store title is hidden, they show only for two folders or more.
export function rootGroupCount(split) {
  return split.primary.length + split.stores.length;
}

// What a card shows beside the badge of the last change: the lock of a read-only store and the mark of a file that is not valid.
export function cardMarks(item) {
  return { readOnly: isReadOnly(item), stale: isStale(item) };
}
