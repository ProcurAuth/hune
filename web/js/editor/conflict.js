// The conflict screen (10.10).

import * as api from "../api.js";
import * as editorRoutes from "./api_editor.js";
import { formatTime } from "../units.js";
import { button, clear, copyText, el, icon, notify, openDialog } from "../ui.js";
import { saveTextFile } from "../list.js";
import { renderDiff } from "./diff_view.js";
import { prettyJson } from "./json_check.js";
import { announceSaved, sendSave } from "./save.js";
import { conflictKind, draftFileName } from "./save_logic.js";

// Shows the draft as text in a read-only side panel that stays until the page closes (choice 1).
function showDraftPanel(text) {
  const existing = document.querySelector("[data-testid=draft-panel]");
  if (existing) existing.remove();
  const area = el("textarea", { class: "jsontext jsontext--panel", readOnly: true, "aria-label": "My draft", rows: 18 });
  area.value = text;
  const panel = el("aside", { class: "draft-panel", dataset: { testid: "draft-panel" }, "aria-label": "My draft" },
    el("div", { class: "draft-panel__head" }, el("h2", { text: "My draft" }),
      button("Copy", { class: "btn btn--sm", on: { click: async () => notify({ level: "info", text: (await copyText(text)) ? "The draft is in the clipboard." : "The browser did not allow the copy." }) } }, "link"),
      button("Hide", { class: "btn btn--ghost btn--sm", on: { click: () => panel.remove() } }, "close")),
    area);
  document.body.append(panel);
}

export async function openConflict(E, error) {
  const kind = conflictKind(error, E.base.etag, E.base.folder);
  const extra = error.extra || {};
  const zone = E.doc.time.timezone;
  const when = extra.saved_at ? formatTime(Date.parse(extra.saved_at), zone) : "";
  const head = el("div", { class: "callout callout--warning conflict__head" }, icon("warning"),
    el("div", { class: "callout__text" }, el("p", { class: "conflict__lead", text: "The dashboard changed after you opened it." }),
      el("p", { class: "muted", text: `Version ${extra.current_version ?? "?"}, saved by ${extra.saved_by ?? "?"} at ${when} (${zone === "utc" ? "UTC" : "browser time"}).` })));
  const content = el("div", { class: "conflict__content" });
  const actions = el("div", { class: "conflict__actions" });
  let dialog;

  const downloadDraft = () => saveTextFile(draftFileName(E.uid, new Date()), E.draftText());
  const cancel = button("Cancel", { dataset: { testid: "conflict-cancel" }, on: { click: () => dialog.close() } });

  async function overwrite(etag, text) {
    if (text) {
      const ok = await new Promise((resolve) => {
        openDialog({ title: "Overwrite with mine", build: (b) => b.append(el("p", { text })), buttons: [{ label: "Cancel", onClick: () => resolve(false) }, { label: "Overwrite", danger: true, onClick: () => resolve(true) }] }).addEventListener("close", () => resolve(false));
      });
      if (!ok) return;
    }
    E.attempt.reset();
    const result = await sendSave(E, { message: E.record.message || "", etag });
    if (result.answer) {
      dialog.close();
      await E.afterSaved(result.answer);
      return;
    }
    if (result.kind === "conflict") {
      dialog.close();
      openConflict(E, result.error);
    } else notify({ level: "error", text: result.text || result.error.message });
  }

  if (kind === "move") {
    // The text names the folder by its title (10.10). The id stands in when the list of folders does not answer.
    let folderTitle = extra.current_folder;
    if (extra.current_folder !== null) {
      try {
        const answer = await api.folders();
        const hit = (answer.items || []).find((f) => f.id === extra.current_folder);
        if (hit && hit.title) folderTitle = hit.title;
      } catch (err) {
        // Keep the id.
      }
    }
    // A dashboard that moved to the root has no folder: the text says it so (10.10).
    content.append(el("p", { text: extra.current_folder === null ? "The dashboard moved to the root after you opened it." : `The dashboard moved to the folder ${folderTitle} after you opened it.` }));
    actions.append(
      button(extra.current_folder === null ? "Save at the root" : "Save in the new folder", { class: "btn btn--primary", dataset: { testid: "conflict-new-folder" }, on: { click: () => overwrite(extra.current_etag) } }),
      cancel);
  } else {
    let theirs = null;
    try {
      theirs = await api.dashboardDownload(E.uid);
    } catch (err) {
      if (err.code !== "not_found") {
        notify({ level: "error", text: `The current version did not load: ${err.message}` });
        return;
      }
    }
    if (theirs === null) {
      content.append(el("p", { text: "The dashboard was deleted after you opened it." }));
      actions.append(button("Download my draft", { class: "btn btn--primary", on: { click: downloadDraft } }, "download"), cancel);
    } else {
      let mine = null;
      let valid = true;
      try {
        mine = (await editorRoutes.validateDocument({ uid: E.uid }, JSON.stringify(E.doc))).canonical;
      } catch (err) {
        valid = false;
        mine = prettyJson(E.doc);
      }
      if (!valid) content.append(el("p", { class: "field-error", text: "Your draft is not a valid document." }));
      content.append(renderDiff({ text: theirs.text, name: `${E.uid}-theirs.json` }, { text: mine, name: `${E.uid}-mine.json` }, { title: "Changes from theirs to mine" }).el);
      actions.append(
        button("Load theirs, keep my draft", { class: "btn btn--primary", dataset: { testid: "conflict-load-theirs" }, on: { click: async () => {
          const text = E.draftText();
          saveTextFile(draftFileName(E.uid, new Date()), text);
          showDraftPanel(text);
          dialog.close();
          await E.loadTheirs();
        } } }),
        button("Overwrite with mine", { dataset: { testid: "conflict-overwrite" }, on: { click: () => overwrite(extra.current_etag, "Their version stays in the history.") } }),
        cancel);
    }
  }

  dialog = openDialog({
    title: "The dashboard changed",
    wide: true,
    testid: "conflict-screen",
    buttons: [],
    build(body) {
      body.append(head, actions, content);
    },
  });
  // Each choice gets a line that says what it does.
  const HINTS = {
    "conflict-new-folder": "Saves your changes in the folder where the dashboard is now.",
    "conflict-load-theirs": "Shows their version. Your draft goes to a file and to a side panel.",
    "conflict-overwrite": "Saves your draft as a new version. Their version stays in the history.",
    "conflict-cancel": "Close this window and go on with your edit.",
  };
  for (const b of [...actions.children]) {
    const choice = el("div", { class: "conflict__choice" });
    b.replaceWith(choice);
    choice.append(b);
    if (HINTS[b.dataset.testid]) choice.append(el("span", { class: "conflict__hint", text: HINTS[b.dataset.testid] }));
  }
  // The focus goes to the first choice. Escape is "Cancel" (the native dialog closes on Escape).
  const first = actions.querySelector("button");
  if (first) first.focus();
  return dialog;
}

export { announceSaved, clear };
