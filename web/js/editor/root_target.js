// The root as the target of a dashboard (10.7, 10.12, 10.13). A dashboard can have no folder: the API writes `folder: null`. The dialogs "New
// dashboard", "Import a dashboard" and "Move" offer this place as the first entry of their folder select, with the label "Root (no folder)". The
// page uses a value that cannot be a folder id for it (ROOT_VALUE), and sends `null` to the API. The functions here are pure, so a test runs them
// without a browser. The right to write at the root comes from the member `root` of GET /api/folders, never from a folder.

import { rootWritable } from "../stores.js";

export const ROOT_VALUE = "::root::";
export const ROOT_LABEL = "Root (no folder)";
export const ROOT_NAME = "Root";

// True when the member `root` of the answer of GET /api/folders says that the user can write at the root (the function is in stores.js, which the list
// page uses too).
export { rootWritable };

// The value of the select for the value of the API: `null` (no folder) is the root.
export const folderValue = (folder) => (folder === null || folder === undefined ? ROOT_VALUE : folder);

// The value for the API from the value of the select: the root is `null`.
export const folderWire = (value) => (value === ROOT_VALUE ? null : value);

// The entries of a folder select: { value, label }. The root is the first entry when `rootOk` is true, then the folders in the order that they have.
export function targetEntries(folders, rootOk) {
  const entries = [];
  if (rootOk) entries.push({ value: ROOT_VALUE, label: ROOT_LABEL });
  for (const f of folders) entries.push({ value: f.id, label: f.title });
  return entries;
}

// The place where the user can write for a dialog: the folders with the right to write, and whether the root accepts a dashboard. `ctx` can hold the
// folders and the root that the list page already has (ctx.folders, ctx.root); else the function asks `api.folders()`. `api` is given by the caller.
export async function loadTargets(ctx, api) {
  let list = ctx.folders;
  let root = ctx.root;
  if (!list || root === undefined) {
    const answer = await api.folders();
    if (!list) list = answer.items || [];
    if (root === undefined) root = answer.root;
  }
  return { folders: list.filter((f) => f.access && f.access.write), rootOk: rootWritable(root) };
}

// The text for the place of a dashboard in a notice or a dialog: the title of the folder, or "Root".
export const placeName = (folder, folders = []) => {
  if (folder === null || folder === undefined) return ROOT_NAME;
  const hit = folders.find((f) => f.id === folder);
  return hit && hit.title ? hit.title : folder;
};
