// Save (10.9): the dialog "Message", the request and the table of answers.

import * as api from "../api.js";
import * as editorRoutes from "./api_editor.js";
import { badge, button, clear, el, notify, openDialog, toggle, uniqueId } from "../ui.js";
import { renderDiff } from "./diff_view.js";
import { errorLink } from "./form.js";
import { classifySaveError, savedText } from "./save_logic.js";
import { rangeProblem, refreshText, timeDifferences, withTimeDefaults } from "./time_defaults.js";

// Sends the save of the draft. It returns { answer } or { error, kind }.
// `etag` is the base etag, or the current etag after a conflict (10.10).
export async function sendSave(E, { message, etag }) {
  const saveId = E.attempt.id();
  E.record.save_id = saveId;
  E.record.message = message;
  E.persistNow();
  try {
    const answer = await editorRoutes.saveDashboard(E.uid, { message, document: E.doc }, etag, saveId);
    return { answer };
  } catch (error) {
    if (api.isAbort(error)) return { error, kind: "abort" };
    const c = classifySaveError(error);
    if (c.kind === "reused") {
      E.attempt.reset();
      E.record.save_id = null;
    }
    return { error, kind: c.kind, text: c.text };
  }
}

// Shows what happened after a successful save.
export function announceSaved(answer) {
  notify({ level: "info", text: savedText(answer) });
  // The last warning of a full list has the pointer "" (03 6.13): it names the document, as an error with no pointer does.
  for (const w of answer.warnings || []) notify({ level: "warning", text: `${w.pointer || "document"}: ${w.message}` });
}

// The tabs of the drawer (as in Grafana): "Details" holds the message, "Changes" holds the line diff from the stored version to the
// draft (10.15). The diff loads in the background, and its size shows on the tab. It reads the same two texts as the conflict screen.
function changesPanel(E, docOf) {
  const panel = el("div", { class: "save-changes", role: "tabpanel", hidden: true, dataset: { testid: "save-changes" } }, el("p", { class: "muted", text: "Loading the changes." }));
  const count = badge("");
  count.hidden = true;
  const load = async () => {
    try {
      const theirs = await api.dashboardDownload(E.uid);
      const mine = (await editorRoutes.validateDocument({ uid: E.uid }, JSON.stringify(docOf()))).canonical;
      const view = renderDiff({ text: theirs.text, name: `${E.uid}-stored.json` }, { text: mine, name: `${E.uid}-draft.json` });
      let changed = 0;
      for (const hunk of view.result.hunks || []) for (const l of hunk.lines) if (l.op !== " ") changed += 1;
      count.textContent = String(changed);
      count.hidden = view.result.tooLarge === true || changed === 0;
      clear(panel);
      panel.append(view.el);
    } catch (error) {
      clear(panel);
      panel.append(el("p", { class: "muted", text: "The changes are not available now." }));
    }
  };
  return { panel, count, load };
}

export function openSaveDialog(E, { onSaved } = {}) {
  const messageId = uniqueId("save-message");
  const message = el("textarea", { class: "input", id: messageId, rows: 4, maxlength: "500", placeholder: "Describe the change (optional)", dataset: { testid: "save-message" } });
  message.value = E.record.message || "";
  const status = el("p", { role: "status", class: "save-status" });
  const count = el("span", { class: "field__hint", text: "0 of 500" });
  const paintCount = () => { count.textContent = `Optional. ${[...message.value].length} of 500 characters.`; };
  message.addEventListener("input", paintCount);
  paintCount();
  const errors = el("div", { class: "errors", hidden: true, dataset: { testid: "save-errors" } });
  let saveButton;
  let dialog;

  // The default time range and the default refresh (10.9, rule 6). The header controls hold a current range and a current refresh. A box shows only
  // when its value differs from the default of the draft. Both boxes are off at first: nothing changes unless the person selects one.
  const current = E.api.pageApi.currentTime;
  const differs = timeDifferences(E.doc.time, current);
  const boxes = {};
  let applied = false;
  let changesLoaded = false;
  const choose = () => ({ range: Boolean(boxes.range && boxes.range.input.checked), refresh: Boolean(boxes.refresh && boxes.refresh.input.checked) });
  const effectiveDoc = () => (applied ? E.doc : withTimeDefaults(E.doc, current, choose()));
  const defaultsBox = el("div", { class: "save-defaults", hidden: !differs.range && !differs.refresh, dataset: { testid: "save-defaults" } });
  const addOption = (key, label, hint, testid) => {
    boxes[key] = toggle({ label, kind: "checkbox", testid, onChange: () => { changesLoaded = false; } });
    defaultsBox.append(el("div", { class: "field" }, boxes[key].el, el("span", { class: "field__hint", text: hint })));
  };
  if (differs.range) addOption("range", "Save the current time range as the default", `Now: ${current.from} to ${current.to}. Default: ${E.doc.time.from} to ${E.doc.time.to}.`, "save-default-range");
  if (differs.refresh) addOption("refresh", "Save the current refresh as the default", `Now: ${refreshText(current.refresh)}. Default: ${refreshText(E.doc.time.refresh)}.`, "save-default-refresh");

  const showErrors = (list) => {
    clear(errors);
    errors.hidden = list.length === 0;
    const ul = el("ul", { class: "error-list" });
    for (const e of list) {
      const item = errorLink(e.pointer || "(document)", `[${e.code}] ${e.message}`, () => {
        E.lastErrors = list;
        dialog.close();
        E.openJsonAt(e.pointer);
      });
      ul.append(el("li", {}, item));
    }
    errors.append(ul);
  };

  async function submit() {
    if ([...message.value].length > 500) {
      status.textContent = "The message has 500 characters at most.";
      return;
    }
    // The chosen defaults go into the draft once, before the first send. A retry then sends the same document with the same `save_id` (10.9).
    const pick = choose();
    if (!applied && (pick.range || pick.refresh)) {
      const problem = pick.range ? rangeProblem(current) : "";
      if (problem) {
        status.textContent = `The current time range cannot be the default. ${problem}`;
        return;
      }
      E.change(withTimeDefaults(E.doc, current, pick));
      applied = true;
      for (const box of Object.values(boxes)) box.input.disabled = true;
    }
    saveButton.disabled = true;
    status.textContent = "Saving";
    const result = await sendSave(E, { message: message.value, etag: E.base.etag });
    saveButton.disabled = false;
    if (result.answer) {
      dialog.close();
      await E.afterSaved(result.answer);
      if (onSaved) onSaved(result.answer);
      return;
    }
    if (result.kind === "abort") return;
    if (result.kind === "conflict") {
      dialog.close();
      const { openConflict } = await import("./conflict.js");
      openConflict(E, result.error);
    } else if (result.kind === "invalid") {
      const list = (result.error.extra && result.error.extra.errors) || [];
      E.lastErrors = list;
      status.textContent = `The document has ${list.length} error${list.length === 1 ? "" : "s"}. Select one to open the JSON at the member.`;
      showErrors(list);
    } else if (result.kind === "retry") {
      // "Retry" sends the same request with the same save_id (10.9).
      status.textContent = result.text;
      saveButton.querySelector("span").textContent = "Retry";
    } else if (result.kind !== "unauthenticated") {
      status.textContent = result.text;
    }
  }

  const changes = changesPanel(E, effectiveDoc);
  dialog = openDialog({
    title: "Save the dashboard",
    subtitle: (E.doc && E.doc.title) || null,
    drawer: true,
    testid: "save-dialog",
    buttons: [{ label: "Cancel" }],
    build(body, shell) {
      saveButton = button("Save", { class: "btn btn--primary", dataset: { testid: "save-confirm" }, on: { click: submit } });
      saveButton.classList.add("btn--primary");
      const details = el("div", { class: "save-details", role: "tabpanel" }, el("div", { class: "field" }, el("label", { class: "field__label", for: messageId, text: "Message" }), message, count), defaultsBox, status, errors);
      body.append(details, changes.panel);
      // The tabs go under the title in the head. The footer (Cancel, Save) belongs to "Details" and hides on "Changes".
      const tabs = el("div", { class: "dialog__tabs", role: "tablist", "aria-label": "Save dashboard" });
      const detailsTab = el("button", { type: "button", class: "dialog__tab", role: "tab", "aria-selected": "true", text: "Details", dataset: { testid: "save-tab-details" } });
      const changesTab = el("button", { type: "button", class: "dialog__tab", role: "tab", "aria-selected": "false", dataset: { testid: "save-tab-changes" } }, el("span", { text: "Changes" }), changes.count);
      tabs.append(detailsTab, changesTab);
      const select = (name) => {
        const isDetails = name === "details";
        // The diff loads when the tab opens the first time. It reads the stored bytes and validates the draft (a PUT with
        // validate_only): a save that does not look at the changes sends no extra request and asks no extra access question.
        if (!isDetails && !changesLoaded) {
          changesLoaded = true;
          changes.load();
        }
        detailsTab.setAttribute("aria-selected", String(isDetails));
        changesTab.setAttribute("aria-selected", String(!isDetails));
        details.hidden = !isDetails;
        changes.panel.hidden = isDetails;
        const footer = body.parentElement && body.parentElement.querySelector(".dialog__footer");
        if (footer) footer.hidden = !isDetails;
      };
      detailsTab.addEventListener("click", () => select("details"));
      changesTab.addEventListener("click", () => select("changes"));
      tabs.addEventListener("keydown", (event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        const next = event.key === "ArrowRight" ? changesTab : detailsTab;
        select(next === changesTab ? "changes" : "details");
        next.focus();
      });
      shell.querySelector(".dialog__head").append(tabs);
    },
  });
  // The safe choice is first, the action is last (design system 6.7).
  dialog.querySelector(".dialog__footer").append(saveButton);
  return dialog;
}
