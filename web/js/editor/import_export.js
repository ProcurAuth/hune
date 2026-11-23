// Import of one document (10.12). Export is a download of `download=true` (list.js, history.js).

import * as api from "../api.js";
import * as editorRoutes from "./api_editor.js";
import { dashboardPath } from "../url_state.js";
import { button, el, icon, notify, openDialog, clear } from "../ui.js";
import { ensureCss, labelled } from "./form.js";
import { renderDiff } from "./diff_view.js";
import { makeSaveId, makeUid } from "./draft.js";
import { scanJson } from "./json_check.js";
import { isGrafanaFile } from "./save_logic.js";

const fill = (b) => crypto.getRandomValues(b);

const errorText = (error) => {
  const list = error.extra && error.extra.errors;
  return list && list.length ? list.map((e) => `${e.pointer || "document"}: ${e.message}`).join(" ") : error.message;
};

// replace: { uid, etag } when the import comes from the "Replace" of an invalid row.
export async function importDialog(ctx, replace = null) {
  ensureCss();
  const folders = ((ctx.folders || (await api.folders()).items || [])).filter((f) => f.access && f.access.write);
  if (!folders.length) {
    notify({ level: "warning", text: "No folder accepts a dashboard." });
    return;
  }
  const limit = ctx.session.limits && ctx.session.limits.document_bytes;
  const file = el("input", { type: "file", accept: ".json,application/json", class: "filepick__input", "aria-label": "JSON file", dataset: { testid: "import-file" } });
  const fileName = el("span", { class: "filepick__name", text: "No file chosen" });
  // The file zone: a click opens the file dialog (the label holds the input), and a file dropped on it counts as a chosen file.
  const picker = el("label", { class: "filepick filepick--drop" },
    icon("upload", 20),
    el("span", { class: "filepick__title", text: "Upload dashboard JSON file" }),
    el("span", { class: "filepick__hint", text: "Drag and drop here or click to browse" }),
    el("span", { class: "filepick__row" }, el("span", { class: "btn" }, icon("upload", 14), el("span", { text: "Choose file" })), file, fileName));
  picker.addEventListener("dragover", (event) => { event.preventDefault(); picker.classList.add("is-over"); });
  picker.addEventListener("dragleave", () => picker.classList.remove("is-over"));
  picker.addEventListener("drop", (event) => {
    event.preventDefault();
    picker.classList.remove("is-over");
    const dropped = event.dataTransfer && event.dataTransfer.files;
    if (!dropped || !dropped.length) return;
    file.files = dropped;
    file.dispatchEvent(new Event("change", { bubbles: true }));
  });
  const select = el("select", { class: "input", "aria-label": "Folder", dataset: { testid: "import-folder" } }, ...folders.map((f) => el("option", { value: f.id, text: f.title })));
  const info = el("span", { class: "field__hint", text: "A JSON file saved from a Hune dashboard. Grafana files use the command hune import-grafana." });
  const errors = el("div", { class: "errors", role: "alert", hidden: true, dataset: { testid: "import-errors" } });
  const choices = el("span", { class: "import__choices" });
  const placeholder = () => button("Import", { class: "btn btn--primary", disabled: true });
  const resetChoices = () => {
    clear(choices);
    choices.append(placeholder());
  };
  const diffHost = el("div");
  let dialog;
  let text = null;
  let value = null;
  let canonical = null;

  const fail = (messages) => {
    clear(errors);
    errors.hidden = !messages.length;
    const ul = el("ul", { class: "error-list" });
    for (const m of messages) ul.append(el("li", { text: m }));
    errors.append(ul);
  };

  async function createNew(doc, from403 = false) {
    try {
      const answer = await editorRoutes.createDashboard({ folder: select.value, message: "", document: doc }, makeSaveId(fill));
      dialog.close();
      ctx.navigate(dashboardPath(answer.uid, doc.title));
    } catch (error) {
      if (error.code === "already_exists" && from403) fail(["A dashboard with this uid exists, but you cannot read it."]);
      else fail([errorText(error)]);
    }
  }

  async function importAsNew() {
    const doc = structuredClone(value);
    doc.uid = makeUid(fill);
    try {
      await editorRoutes.validateDocument({ folder: select.value }, JSON.stringify(doc));
    } catch (error) {
      fail([errorText(error)]);
      return;
    }
    await createNew(doc);
  }

  async function replaceCurrent(etag) {
    try {
      await editorRoutes.saveDashboard(value.uid, { message: "", document: value }, etag, makeSaveId(fill));
      dialog.close();
      ctx.navigate(dashboardPath(value.uid, value.title));
    } catch (error) {
      fail([error.code === "conflict" ? "The dashboard changed. Select the file again." : errorText(error)]);
    }
  }

  async function afterValid() {
    clear(choices);
    clear(diffHost);
    let current = null;
    let etag = replace ? replace.etag : null;
    let exists = false;
    let forbidden = false;
    try {
      const answer = await api.dashboard(value.uid);
      exists = true;
      etag = answer.etag;
      current = (await api.dashboardDownload(value.uid)).text;
    } catch (error) {
      if (error.code === "stored_document_invalid") {
        exists = true;
        const item = (ctx.dashboards || []).find((d) => d.uid === value.uid);
        etag = etag || (item && item.etag);
        const download = await api.dashboardDownload(value.uid);
        current = download.text;
        etag = etag || download.etag;
      } else if (error.status === 403) forbidden = true;
      else if (error.code !== "not_found") {
        fail([errorText(error)]);
        return;
      }
    }
    if (exists) {
      diffHost.append(renderDiff({ text: current, name: `${value.uid}-current.json` }, { text: canonical, name: `${value.uid}-file.json` }, { title: "Changes from the current dashboard to the file" }).el);
      choices.append(
        ...(replace ? [] : [el("button", { type: "button", class: "btn", text: "Import as a new dashboard", dataset: { testid: "import-new" }, on: { click: importAsNew } })]),
        el("button", { type: "button", class: "btn btn--primary", text: "Replace the current dashboard", dataset: { testid: "import-replace" }, on: { click: () => {
          openDialog({ title: "Replace the current dashboard?", build: (b) => b.append(el("p", { text: "The current version stays in the history." })), buttons: [{ label: "Cancel" }, { label: "Replace", danger: true, onClick: () => { replaceCurrent(etag); } }] });
        } } }),
      );
    } else {
      choices.append(el("button", { type: "button", class: "btn btn--primary", text: "Import", dataset: { testid: "import-create" }, on: { click: () => createNew(value, forbidden) } }));
    }
  }

  async function onFile() {
    fail([]);
    resetChoices();
    clear(diffHost);
    const f = file.files[0];
    fileName.textContent = f ? f.name : "No file chosen";
    if (!f) return;
    if (limit && f.size > limit) return fail([`The file is larger than ${limit} bytes.`]);
    text = await f.text();
    const scan = scanJson(text);
    if (!scan.ok) return fail(scan.errors.map((e) => `Line ${e.line}, column ${e.column}: ${e.message}`));
    value = scan.value;
    if (isGrafanaFile(value)) return fail(["This is a Grafana file. Use the command `hune import-grafana`."]);
    if (!value || typeof value !== "object" || typeof value.uid !== "string") return fail(["The file is not a Hune dashboard: it has no uid."]);
    if (replace && value.uid !== replace.uid) return fail([`The file has the uid ${value.uid}, but this row is ${replace.uid}.`]);
    info.textContent = `Uid of the document: ${value.uid}`;
    try {
      const answer = await editorRoutes.validateDocument({ folder: select.value }, text);
      canonical = answer.canonical;
    } catch (error) {
      return fail(errorText(error).split(/(?<=\.) /));
    }
    await afterValid();
  }
  file.addEventListener("change", onFile);

  dialog = openDialog({
    title: replace ? `Replace the dashboard ${replace.uid}` : "Import a dashboard",
    testid: "import-dialog",
    buttons: [{ label: "Cancel" }],
    build: (body) => body.append(el("div", { class: "field" }, el("span", { class: "field__label", text: "JSON file" }), picker, info), labelled("Folder", select), errors, diffHost),
  });
  // The choices sit in the footer after Cancel: the action is last (design system 6.7).
  resetChoices();
  dialog.querySelector(".dialog__footer").append(choices);
}
