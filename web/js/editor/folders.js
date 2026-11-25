// The controls of the list page (10.7, 10.13): new dashboard, folders, move, delete.
// Each function takes ctx = { session, folders?, navigate, reload } and the item.

import * as api from "../api.js";
import * as editorRoutes from "./api_editor.js";
import { dashboardPath } from "../url_state.js";
import { button, clear, el, notify, openDialog } from "../ui.js";
import { ensureCss, labelled } from "./form.js";
import { makeSaveId, makeUid, smallestDocument } from "./draft.js";
import { folderIdFromTitle, folderIdWithSuffix } from "./folder_id.js";
import { folderValue, folderWire, loadTargets, targetEntries } from "./root_target.js";
import { READ_ONLY_ERROR_TEXT } from "../stores.js";
import { removeStored } from "./draft_store.js";
import { deleteRetryIsDone } from "./save_logic.js";

const fill = (b) => crypto.getRandomValues(b);

// The places where the user can write: the folders with the right to write, and the root when the member `root` allows it (root_target.js).
const writableTargets = (ctx) => loadTargets(ctx, api);

// The text of an error under the fields (10.7). A bad_request lists each fault with its pointer.
function errorText(error) {
  if (error.code === "read_only_store") return READ_ONLY_ERROR_TEXT;
  const list = error.extra && error.extra.errors;
  if (list && list.length) return list.map((e) => `${e.pointer || "request"}: ${e.message}`).join(" ");
  return error.message;
}

// A select of places: `entries` are { value, label } (root_target.js targetEntries). The root entry is the first one.
function folderSelect(entries, value) {
  const s = el("select", { class: "input", "aria-label": "Folder", dataset: { testid: "folder-select" } });
  for (const e of entries) s.append(el("option", { value: e.value, text: e.label }));
  if (value && entries.some((e) => e.value === value)) s.value = value;
  return s;
}

// A dialog with one action that runs asynchronously. `run()` returns an error text, or null for success (then it closes).
function actionDialog({ title, testid, build, label, run, danger = false }) {
  ensureCss();
  const status = el("p", { class: "field-error", role: "alert", hidden: true });
  let dialog;
  let submit;
  const go = async () => {
    submit.disabled = true;
    status.hidden = true;
    const problem = await run(status);
    submit.disabled = false;
    if (problem) {
      status.textContent = problem;
      status.hidden = false;
    } else dialog.close();
  };
  dialog = openDialog({
    title,
    testid,
    build: (body) => {
      build(body);
      body.append(status);
    },
    buttons: [{ label: "Cancel" }, { label, primary: !danger, danger, onClick: () => { go(); return false; } }],
  });
  submit = dialog.querySelector(".btn--primary, .btn--danger");
  return dialog;
}

export async function newDashboard(ctx) {
  const { folders, rootOk } = await writableTargets(ctx);
  if (!folders.length && !rootOk) {
    notify({ level: "warning", text: "No folder accepts a new dashboard." });
    return;
  }
  const title = el("input", { class: "input", type: "text", "aria-label": "Title", dataset: { testid: "new-title" } });
  const folder = folderSelect(targetEntries(folders, rootOk));
  actionDialog({
    title: "New dashboard",
    testid: "new-dashboard-dialog",
    label: "Create",
    build: (body) => body.append(labelled("Title", title), labelled("Folder", folder)),
    run: async () => {
      const text = title.value.trim();
      if (!text || [...text].length > 256) return "The title is necessary and has 256 characters at most.";
      for (let attempt = 0; attempt < 2; attempt++) {
        const uid = makeUid(fill);
        try {
          await editorRoutes.createDashboard({ folder: folderWire(folder.value), message: "", document: smallestDocument(uid, text) }, makeSaveId(fill));
          ctx.navigate(`${dashboardPath(uid, text)}?edit=1`);
          return null;
        } catch (error) {
          if (error.code !== "already_exists" || attempt === 1) return errorText(error);
        }
      }
      return null;
    },
  });
}

// The person gives a title. The page makes the id from it (folder_id.js). When the id exists already, the page tries again with a short suffix.
export function newFolder(ctx) {
  const title = el("input", { class: "input", type: "text", "aria-label": "Title", dataset: { testid: "folder-title" } });
  actionDialog({
    title: "New folder",
    testid: "new-folder-dialog",
    label: "Create",
    build: (body) => body.append(labelled("Title", title)),
    run: async () => {
      const t = title.value.trim();
      if (!t || [...t].length > 256) return "The title is necessary and has 256 characters at most.";
      let id = folderIdFromTitle(t, fill);
      for (let attempt = 0; attempt < 3; attempt++) {
        try {
          await editorRoutes.createFolder({ id, title: t });
          await ctx.reload();
          return null;
        } catch (error) {
          if (error.code !== "already_exists" || attempt === 2) return errorText(error);
          id = folderIdWithSuffix(id, fill);
        }
      }
      return null;
    },
  });
}

export function renameFolder(ctx, folder) {
  const title = el("input", { class: "input", type: "text", value: folder.title, "aria-label": "Title" });
  actionDialog({
    title: `Rename the folder ${folder.title}`,
    label: "Rename",
    build: (body) => body.append(labelled("Title", title)),
    run: async () => {
      const t = title.value.trim();
      if (!t || [...t].length > 256) return "The title is necessary and has 256 characters at most.";
      try {
        await editorRoutes.renameFolder(folder.id, t);
        await ctx.reload();
        return null;
      } catch (error) {
        return errorText(error);
      }
    },
  });
}

export function deleteFolder(ctx, folder) {
  actionDialog({
    title: `Delete the folder ${folder.title}?`,
    label: "Delete",
    danger: true,
    build: (body) => body.append(el("p", { text: "An empty folder is deleted." })),
    run: async () => {
      try {
        await editorRoutes.deleteFolder(folder.id);
        await ctx.reload();
        return null;
      } catch (error) {
        return error.code === "folder_not_empty" ? "The folder holds dashboards. Move or delete them first." : errorText(error);
      }
    },
  });
}

export async function moveDashboard(ctx, item) {
  const { folders, rootOk } = await writableTargets(ctx);
  // A dashboard at the root has no entry "Root" in its own dialog: it is there already.
  const select = folderSelect(targetEntries(folders, rootOk && item.folder !== null), folderValue(item.folder));
  const saveId = makeSaveId(fill);
  actionDialog({
    title: `Move the dashboard ${item.title}`,
    label: "Move",
    build: (body) => body.append(labelled("Folder", select)),
    run: async () => {
      try {
        const wire = folderWire(select.value);
        const answer = await editorRoutes.moveDashboard(item.uid, { folder: wire, message: "" }, item.etag, saveId);
        if (answer && answer.noop) notify({ level: "info", text: wire === null ? "The dashboard is already at the root." : "The dashboard is already in this folder." });
        await ctx.reload();
        return null;
      } catch (error) {
        if (error.code === "conflict") return "The dashboard changed. Read it again and try again.";
        return errorText(error);
      }
    },
  });
}

export function deleteDashboard(ctx, item) {
  const saveId = makeSaveId(fill);
  let attempts = 0;
  actionDialog({
    title: `Delete the dashboard ${item.title}?`,
    testid: "delete-dashboard-dialog",
    label: "Delete",
    danger: true,
    build: (body) => body.append(el("p", { text: "It leaves the list. An operator can bring it back." })),
    run: async () => {
      attempts += 1;
      try {
        await editorRoutes.deleteDashboard(item.uid, item.etag, saveId);
      } catch (error) {
        // After a lost answer the same request is sent again. A 404 on that retry means that the first delete was done (10.13).
        if (!(attempts > 1 && deleteRetryIsDone(error))) {
          if (error.code === "conflict") return "The dashboard changed. Read it again and try again.";
          if (error.code === "storage_unavailable" || error.code === "network_error" || error.code === "request_timeout") return `${error.message} Select Delete to try again.`;
          return errorText(error);
        }
      }
      removeStored(item.uid);
      notify({ level: "info", text: `The dashboard ${item.title} is deleted.` });
      if (ctx.isDashboardPage) ctx.navigate(".");
      else await ctx.reload();
      return null;
    },
  });
}

export { button, clear };
