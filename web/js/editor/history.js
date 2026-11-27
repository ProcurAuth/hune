// History (10.11): the list, the read-only view of a version, the diff and the restore.

import * as api from "../api.js";
import * as editorRoutes from "./api_editor.js";
import * as busy from "../busy.js";
import { createGrid } from "../grid.js";
import { createPanelFrame } from "../panel_frame.js";
import { gridDoc } from "../var_layout.js";
import { formatTime } from "../units.js";
import { buildOptions, optionsOf, requestValue, resolveSelection } from "../var_options.js";
import { saveTextFile } from "../export_file.js";
import { badge, button, clear, el, notify, openDialog, uniqueId } from "../ui.js";
import { renderDiff } from "./diff_view.js";
import { makeSaveId } from "./draft.js";
import { neededVariables, previewBody } from "./preview_body.js";

const HEAD_CLASS = { Kind: "history__kind", "Saved by": "history__by", Size: "history__size num" };
const KIND_TONE = { create: "ok", save: null, restore: "info", move: "info", delete: "error", external: "warn" };
const NOTE = "Data not shown";
const fill = (b) => crypto.getRandomValues(b);

// A runtime without queries: every panel shows a note, and "Show data" runs the preview (10.11).
class VersionRuntime {
  constructor(doc) {
    this.doc = doc;
    this.listeners = new Set();
    this.panels = new Map();
    this.expanded = new Map();
    this.variables = doc.variables;
    doc.sections.forEach((s, si) => {
      this.expanded.set(si, !s.collapsed);
      for (const panel of s.panels) {
        this.panels.set(panel.id, { id: panel.id, panel, section: si, state: "idle", frame: null, error: null, note: NOTE, action: null });
      }
    });
  }

  on(l) {
    this.listeners.add(l);
    return () => this.listeners.delete(l);
  }

  set(id, patch) {
    const rec = this.panels.get(id);
    if (!rec) return;
    Object.assign(rec, patch);
    for (const l of this.listeners) l("panel", rec);
  }

  setVisible() {}
  setExpanded(i, e) {
    this.expanded.set(i, e);
  }
  retry() {}
  requestVariables() {
    return {};
  }
  destroy() {
    this.listeners.clear();
  }
}

const ref = (v) => (v === "current" ? "current" : Number(v));

export function createHistory(E) {
  const controller = new AbortController();
  const root = el("section", { class: "history", dataset: { testid: "history" } });
  const view = E.api.state;
  const zone = E.base.doc.time.timezone;
  const canWrite = E.session.editing === true && E.base.access && E.base.access.write === true;
  let inner = null;

  // "Back to the dashboard" is the event `leave` and "Back to the history" is the event `up` (10.2a).
  const back = () => E.dispatch("leave");
  const toList = () => E.dispatch("up");
  const topBar = (toHistory, title, ...extra) => el("div", { class: "ebar" },
    button(toHistory ? "Back to the history" : "Back to the dashboard", { class: "btn btn--ghost", dataset: { testid: "history-back" }, on: { click: toHistory ? toList : back } }, "arrow-up"),
    el("h2", { class: "ebar__title", text: title }), ...extra);

  // The text of the current document reads `download=true` when the user has `access.export` (6.14). Without it, the text of the newest version
  // of the history stands for the current document (6.17, 10.11): the download of a version needs the history right only.
  const canExport = Boolean(E.base.access && E.base.access.export === true);
  async function currentText() {
    if (canExport) return (await api.dashboardDownload(E.uid, controller.signal)).text;
    const newest = await editorRoutes.history(E.uid, { limit: 1 }, controller.signal);
    const head = newest.items.find((i) => i.kind !== "delete");
    if (!head) throw new Error("This dashboard has no version to compare with.");
    return editorRoutes.versionDownload(E.uid, head.version, controller.signal);
  }
  const getText = async (r) => (r === "current" ? currentText() : editorRoutes.versionDownload(E.uid, r, controller.signal));

  function restoreDialog(n) {
    const messageId = uniqueId("restore-message");
    const message = el("textarea", { class: "input", id: messageId, rows: 3, maxlength: "500" });
    openDialog({
      title: `Restore version ${n} as a new version?`,
      testid: "restore-dialog",
      build: (b) => b.append(el("div", { class: "field" }, el("label", { class: "field__label", for: messageId, text: "Message" }), message, el("span", { class: "field__hint", text: "Optional, 500 characters at most." }))),
      buttons: [
        { label: "Cancel" },
        {
          label: "Restore",
          primary: true,
          onClick: () => {
            doRestore(n, message.value);
          },
        },
      ],
    });
  }

  async function doRestore(n, message) {
    busy.begin();
    try {
      const answer = await editorRoutes.restoreVersion(E.uid, { version: n, message }, E.base.etag, makeSaveId(fill));
      notify({ level: "info", text: `Version ${n} is restored as version ${answer.version}.` });
      // The restore ends the history: the page shows the state that opened it, with the new version (10.2a).
      await E.api.finishEdit("leave");
    } catch (error) {
      if (error.code === "conflict") notify({ level: "warning", text: "The dashboard changed. Load the history again." });
      else notify({ level: "error", text: `${error.code}: ${error.message}` });
    } finally {
      busy.end();
    }
  }

  // --- list ---------------------------------------------------------------------------------------------------------

  function listView() {
    const body = el("div", { class: "history__list", dataset: { testid: "history-list" } });
    const tableBody = el("tbody");
    const more = button("Load more", { dataset: { testid: "history-more" } });
    const compare = button("Compare", { disabled: true, dataset: { testid: "history-compare" } });
    const status = el("p", { class: "muted history__status", role: "status" });
    const picked = new Set();
    let next = null;
    let first = true;

    const updateCompare = () => {
      compare.disabled = picked.size !== 2;
    };
    compare.addEventListener("click", () => {
      const [a, b] = [...picked].sort((x, y) => x - y);
      E.dispatch("history-nav", { base: String(a), version: String(b) });
    });

    function addRows(items) {
      items.forEach((item, i) => {
        const isCurrent = first && i === 0 && item.kind !== "delete";
        const noBytes = item.kind === "delete" || (item.kind === "external" && item.size === null);
        const pick = el("input", { type: "checkbox", "aria-label": `Select version ${item.version}`, disabled: noBytes });
        pick.addEventListener("change", () => {
          if (pick.checked) picked.add(item.version);
          else picked.delete(item.version);
          if (picked.size > 2) {
            picked.delete(item.version);
            pick.checked = false;
          }
          updateCompare();
        });
        const actions = el("td", { class: "history__actions" },
          button("View", { class: "btn btn--sm", dataset: { testid: `history-view-${item.version}` }, on: { click: () => E.dispatch("history-nav", { version: String(item.version) }) } }),
          button("Compare with current", { class: "btn btn--sm", disabled: noBytes || isCurrent, on: { click: () => E.dispatch("history-nav", { base: String(item.version), version: "current" }) } }));
        if (!isCurrent && !noBytes && canWrite) actions.append(button("Restore", { class: "btn btn--sm", dataset: { testid: `history-restore-${item.version}` }, on: { click: () => restoreDialog(item.version) } }));
        tableBody.append(el("tr", { dataset: { version: String(item.version) } },
          el("td", { class: "history__pick" }, pick),
          el("td", { class: "history__version-cell nowrap" }, String(item.version), isCurrent ? badge("current", "accent") : null),
          el("td", { class: "history__kind" }, badge(item.kind, KIND_TONE[item.kind] || null)),
          el("td", { class: "nowrap", text: formatTime(Date.parse(item.saved_at), zone) }),
          el("td", { class: "nowrap history__by", text: item.saved_by }),
          el("td", { class: "history__message", text: item.message || "" }),
          el("td", { class: "num history__size", text: item.size === null ? "-" : String(item.size) }),
          actions));
        first = first && false;
      });
    }

    async function load() {
      more.disabled = true;
      busy.begin();
      try {
        const page = await editorRoutes.history(E.uid, { limit: 100, after: next }, controller.signal);
        addRows(page.items);
        next = page.next;
        more.hidden = !next;
        status.textContent = tableBody.children.length ? "" : "This dashboard has no history.";
      } catch (error) {
        if (!api.isAbort(error)) status.textContent = `The history did not load: ${error.message}`;
      } finally {
        more.disabled = false;
        busy.end();
      }
    }
    more.addEventListener("click", load);
    const head = el("thead", {}, el("tr", {}, ...["Pick", "Version", "Kind", "Saved at", "Saved by", "Message", "Size", "Actions"].map((t) => el("th", { scope: "col", class: HEAD_CLASS[t] || null, ...(t === "Size" ? { title: "Size in bytes" } : t === "Pick" ? { title: "Select two versions to compare" } : {}) }, t))));
    body.append(el("div", { class: "card history__card" }, el("table", { class: "table history__table" }, head, tableBody)), status, el("div", { class: "history__foot" }, more, compare, el("span", { class: "muted", text: "Select two versions to compare them." })));
    load();
    return el("div", {}, topBar(false, "History"), body);
  }

  // --- one version ------------------------------------------------------------------------------------------------------

  async function versionView(n) {
    const holder = el("div", { class: "history__version", dataset: { testid: "history-version" } });
    const out = el("div", {}, topBar(true, `Version ${n}`), holder);
    holder.append(el("p", { class: "muted", text: "Loading" }));
    busy.begin();
    try {
      let doc;
      let meta = null;
      if (n === "current") {
        const cur = await api.dashboard(E.uid, controller.signal);
        doc = cur.document;
        meta = { version: cur.version, saved_at: cur.updated_at, saved_by: cur.updated_by, etag: cur.etag };
      } else {
        try {
          const v = await editorRoutes.historyVersion(E.uid, n, controller.signal);
          doc = v.document;
          meta = v;
          if (doc === null) {
            clear(holder);
            const text = v.kind === "delete" ? "This version is a deletion." : `The file of this version was not a valid document. Hune keeps only its etag: ${v.etag}.`;
            holder.append(el("p", { text }));
            return out;
          }
        } catch (error) {
          if (error.code !== "stored_document_invalid") throw error;
          clear(holder);
          holder.append(el("p", { class: "field-error", text: "The current rules cannot read this version." }));
          const ul = el("ul", { class: "error-list" });
          for (const e of (error.extra && error.extra.errors) || []) ul.append(el("li", {}, el("code", { text: e.pointer }), ` ${e.message}`));
          holder.append(ul);
          const text = await editorRoutes.versionDownload(E.uid, n, controller.signal);
          const area = el("textarea", { class: "jsontext", readOnly: true, "aria-label": "Stored text of the version" });
          area.value = text;
          holder.append(area);
          return out;
        }
      }
      clear(holder);
      const by = meta.saved_by && meta.saved_by !== "-" ? `, by ${meta.saved_by}` : "";
      const sentence = `Version ${meta.version ?? n} of ${formatTime(Date.parse(meta.saved_at), zone)}${by}. This view is read-only.`;
      const banner = el("div", { class: "banner", role: "status", dataset: { testid: "version-banner" } }, el("span", { text: sentence }));
      const isCurrent = n === "current";
      const canShow = E.session.editing === true && doc.sections.every((s) => s.panels.every((p) => !(p.data && p.data.query) || E.sources.some((x) => x.id === p.data.query.source)));
      const acts = el("div", { class: "banner__actions" });
      if (!isCurrent && canWrite) acts.append(button("Restore this version", { class: "btn btn--sm", on: { click: () => restoreDialog(n) } }));
      if (!isCurrent) {
        acts.append(button("Compare with current", { class: "btn btn--sm", on: { click: () => E.dispatch("history-nav", { base: String(n), version: "current" }) } }));
        if (canExport) acts.append(button("Export this version", { class: "btn btn--sm", dataset: { testid: "version-export" }, on: { click: async () => saveTextFile(`${E.uid}-v${n}.json`, await getText(n)) } }, "download"));
      }
      // One page-level action loads the data of every panel that has a query (three at a time).
      if (canShow) {
        const show = button("Show data", { class: "btn btn--sm", dataset: { testid: "version-show-data" } }, "eye");
        show.addEventListener("click", async () => {
          show.disabled = true;
          const todo = doc.sections.flatMap((x) => x.panels).filter((p) => p.data && p.data.query);
          for (let i = 0; i < todo.length; i += 3) await Promise.all(todo.slice(i, i + 3).map((p) => showData(rt, doc, p)));
        });
        acts.prepend(show);
      }
      banner.append(acts);
      holder.append(banner);
      const rt = new VersionRuntime(doc);
      const page = { rt, zone: doc.time.timezone, syncKey: "", linkContext: () => ({ from: 0, to: 0, variables: [] }), panelUrl: () => "", viewPanel() {}, onZoom() {} };
      const grid = createGrid({ doc: gridDoc(doc), rt, makeFrame: (panel) => createPanelFrame({ panel, page }) });
      rt.on((type, rec) => {
        const f = grid.frames.get(rec.id);
        if (f) f.update(true);
      });
      holder.append(grid.el);
      inner = { destroy: () => { grid.destroy(); rt.destroy(); } };
    } catch (error) {
      if (!api.isAbort(error)) {
        clear(holder);
        holder.append(el("p", { class: "field-error", text: `The version did not load: ${error.message}` }));
      }
    } finally {
      busy.end();
    }
    return out;
  }

  // "Show data" (10.11): the preview with the variables of the version.
  async function showData(rt, doc, panel) {
    const q = panel.data && panel.data.query;
    if (!q) return;
    rt.set(panel.id, { note: null, action: null, state: "loading" });
    const range = E.api.runtime.range;
    try {
      const needed = neededVariables(doc.variables, q.sql);
      const known = {};
      for (const v of needed) {
        if (v.kind !== "query") {
          const options = optionsOf(v, null);
          known[v.name] = requestValue(v, options, resolveSelection(v, options, undefined, undefined));
          continue;
        }
        const body = { uid: E.uid, from_ms: range.from_ms, to_ms: range.to_ms, dashboard_variables: needed, variables: { ...known }, variable: v.name };
        const answer = await editorRoutes.preview(v.source, body);
        const options = buildOptions(answer.options || [], v);
        known[v.name] = requestValue(v, options, resolveSelection(v, options, undefined, undefined));
      }
      const body = previewBody({ doc, panel, target: { uid: E.uid }, range, maxDataPoints: 600, known });
      const frame = await editorRoutes.preview(q.source, body);
      rt.set(panel.id, { state: "ready", frame, error: null });
    } catch (error) {
      rt.set(panel.id, { state: "error", error });
    }
  }

  // --- diff -----------------------------------------------------------------------------------------------------------------

  async function diffView(a, b) {
    const holder = el("div", { class: "history__diff" }, el("p", { class: "muted", text: "Loading" }));
    const out = el("div", {}, topBar(true, "Compare versions"), holder);
    busy.begin();
    try {
      const [ta, tb] = await Promise.all([getText(a), getText(b)]);
      clear(holder);
      holder.append(renderDiff({ text: ta, name: `${E.uid}-${a === "current" ? "current" : `v${a}`}.json` }, { text: tb, name: `${E.uid}-${b === "current" ? "current" : `v${b}`}.json` }, { title: `Changes from version ${a} to version ${b}` }).el);
    } catch (error) {
      if (!api.isAbort(error)) {
        clear(holder);
        holder.append(el("p", { class: "field-error", text: `The versions did not load: ${error.message}` }));
      }
    } finally {
      busy.end();
    }
    return out;
  }

  // --- the view ---------------------------------------------------------------------------------------------------------------

  if (view.base !== null && view.version !== null) diffView(ref(view.base), ref(view.version)).then((n) => root.append(n));
  else if (view.version !== null) versionView(ref(view.version)).then((n) => root.append(n));
  else root.append(listView());

  return {
    el: root,
    destroy() {
      controller.abort();
      if (inner) inner.destroy();
    },
  };
}
