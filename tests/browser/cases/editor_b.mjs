// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B19, B20, B21, B22 and B230 (16.10): conflict, history, import and export, delete, no editing, the id of a new folder.
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { BINARY, SEED_FROM, SEED_TO, WORK, seedDocument } from "../lib/env.mjs";
import { backToDashboard, draftOf, enterEdit, makeSeed, saveDialog, setFiles, setPanelTitle, waitDraft, waitNotice, moreItem } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const titleOf = (p) => p.eval(`document.querySelector('.edit-grid [data-testid="panel-2"] .panel__title').textContent`);

async function waitDownload(dir, re, timeout = 8000) {
  const end = Date.now() + timeout;
  for (;;) {
    const f = existsSync(dir) ? readdirSync(dir).find((x) => re.test(x) && !x.endsWith(".crdownload")) : null;
    if (f) return join(dir, f);
    if (Date.now() > end) throw new Error(`no download matching ${re} in ${dir}: ${existsSync(dir) ? readdirSync(dir) : "no dir"}`);
    await sleep(100);
  }
}

/** Open a dashboard in a new context, enter edit mode, set the title of panel 2. */
async function editor(t, uid, title) {
  const p = await t.open(t.dash(uid));
  await waitPanelsReady(p, [1, 2, 3]);
  await enterEdit(p);
  await setPanelTitle(p, 2, title);
  await waitDraft(p, uid, (d) => d.document.sections[1].panels[0].title === title);
  return p;
}

async function waitSaved(p) {
  await p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null`, { what: "edit mode left after the save" });
}

// The action "Import" is an item of the menu New of the header (9.7).
const openImport = async (p) => {
  await p.click(p.tid("list-new-menu"));
  await p.click(p.tid("import"));
};

export const cases = [
  {
    id: "B19",
    name: "conflict: a second context saves first; the screen has a line diff and three choices, each with its result",
    async run(t) {
      const uid = "ed19";
      // The default range is the range of the address of `t.dash`: the button Save is active when the range differs from the default (10.9).
      await makeSeed(t, uid, (d) => { d.time = { ...d.time, from: String(SEED_FROM), to: String(SEED_TO) }; });
      const A = await editor(t, uid, "Mine 1");
      const B = await editor(t, uid, "Theirs 1");
      await saveDialog(B, "their change");
      await waitSaved(B);
      t.eq((await t.api.dashboard(uid)).json.version, 2, "B saved version 2");
      // A saves: conflict screen
      t.allow(A, /Failed to load resource/);
      await saveDialog(A, "my change");
      await A.waitSel(A.tid("conflict-screen"));
      await A.idle();
      const screen = await A.text(A.tid("conflict-screen"));
      t.assert(screen.includes("The dashboard changed after you opened it."), "lead text");
      t.assert(/Version 2, saved by -/.test(screen), "version and subject: " + screen.slice(0, 200));
      t.assert(await A.exists(`${A.tid("conflict-screen")} ${A.tid("diff-view")}`), "line diff in the screen");
      const diff = await A.text(A.tid("diff-view"));
      t.assert(diff.includes("@@") && diff.includes("Theirs 1") && diff.includes("Mine 1"), "diff shows both titles");
      t.eq(await A.eval(`document.activeElement.dataset.testid`), "conflict-load-theirs", "focus on the first choice");
      for (const id of ["conflict-load-theirs", "conflict-overwrite", "conflict-cancel"]) t.assert(await A.exists(A.tid(id)), "choice " + id);
      // Escape is Cancel: edit mode continues with the draft and the old base
      await A.press("Escape");
      await A.waitGone(A.tid("conflict-screen"));
      t.eq(await A.exists(A.tid("edit-bar")), true, "edit mode continues after Escape");
      t.eq((await draftOf(A, uid)).document.sections[1].panels[0].title, "Mine 1", "draft kept");
      // the Cancel button too
      await saveDialog(A, "my change");
      await A.waitSel(A.tid("conflict-screen"));
      await A.click(A.tid("conflict-cancel"));
      await A.waitGone(A.tid("conflict-screen"));
      // Overwrite with mine: a confirmation, then the save with the current etag
      await saveDialog(A, "my change");
      await A.waitSel(A.tid("conflict-screen"));
      await A.click(A.tid("conflict-overwrite"));
      await A.waitFor(`document.body.textContent.includes("Their version stays in the history.")`, { what: "overwrite confirmation" });
      await A.clickText("dialog button, .dialog button", "Overwrite");
      await waitSaved(A);
      const mine = await t.api.dashboard(uid);
      t.eq(mine.json.document.sections[1].panels[0].title, "Mine 1", "my version is current");
      t.eq(mine.json.version, 3, "version 3");
      const hist = await t.api.history(uid);
      t.eq(hist.json.items.map((i) => i.kind), ["save", "save", "create"], "their version stays in the history");
      t.eq((await t.api.call("GET", `api/dashboards/${uid}/history/2`)).json.document.sections[1].panels[0].title, "Theirs 1", "version 2 is theirs");
      // Load theirs and keep my draft
      await enterEdit(A);
      await setPanelTitle(A, 2, "Mine 2");
      await waitDraft(A, uid, (d) => d.document.sections[1].panels[0].title === "Mine 2");
      const B2 = await editor(t, uid, "Theirs 2");
      await saveDialog(B2, "");
      await waitSaved(B2);
      await saveDialog(A, "");
      await A.waitSel(A.tid("conflict-screen"));
      await A.click(A.tid("conflict-load-theirs"));
      await A.waitSel(A.tid("draft-panel"));
      const keep = await A.eval(`document.querySelector('[data-testid="draft-panel"] textarea').value`);
      t.assert(keep.includes('"Mine 2"'), "the side panel holds my draft");
      const file = await waitDownload(t.downloads, /^ed19-draft-\d{8}-\d{6}\.json$/);
      t.assert(readFileSync(file, "utf8").includes('"Mine 2"'), "the downloaded draft holds my text");
      await waitDraft(A, uid, () => true).catch(() => null);
      await A.waitFor(`localStorage.getItem("hune.draft.v1.${uid}") === null || JSON.parse(localStorage.getItem("hune.draft.v1.${uid}")).document.sections[1].panels[0].title === "Theirs 2"`, { what: "draft replaced" });
      await A.waitSel(A.tid("edit-bar"));
      await A.idle();
      t.eq(await titleOf(A), "Theirs 2", "edit mode continues on theirs");
      t.eq(await A.eval(`document.querySelector('[data-testid="save-button"]').disabled`), true, "no change on theirs");
      t.eq(await A.exists(A.tid("draft-panel")), true, "the side panel stays");
      // A move does not change the etag. In mode open the save has no folder decision, so it passes (5.5, rule 2:
      // the folder comparison needs the access decision of mode authzen). The save does not change the folder (6.15).
      const fr = await t.api.createFolder("ops19", "Operations");
      t.assert(fr.status === 201 || fr.status === 409, "folder ops19: " + fr.status);
      const m = await makeSeed(t, "ed19m");
      const C = await editor(t, "ed19m", "After move");
      const mv = await t.api.call("POST", "api/dashboards/ed19m/move", { body: { folder: "ops19", message: "" }, headers: { "If-Match": `"${m.etag}"`, "Idempotency-Key": t.api.uuid() } });
      t.eq(mv.status, 200, "move through the API: " + mv.text);
      await saveDialog(C, "");
      await waitSaved(C);
      const after = await t.api.dashboard("ed19m");
      t.eq([after.json.folder, after.json.document.sections[1].panels[0].title], ["ops19", "After move"], "saved, and the dashboard stays in the new folder");
    },
  },

  {
    id: "B20",
    name: "history: list, read-only view, diff, restore as a new version",
    async run(t) {
      const uid = "ed20";
      const { etag } = await makeSeed(t, uid);
      const edit = async (e, title, msg) => {
        const d = JSON.parse(JSON.stringify((await t.api.dashboard(uid)).json.document));
        d.sections[1].panels[0].title = title;
        const r = await t.api.save(uid, e, d, msg);
        t.assert(r.status === 200, "save: " + r.text);
        return r.etag;
      };
      const e2 = await edit(etag, "Second", "second change");
      await edit(e2, "Third", "third change");
      const p = await t.open(t.dash(uid));
      await waitPanelsReady(p, [1, 2, 3]);
      await moreItem(p, "history-button");
      await p.waitSel(p.tid("history-list"));
      await p.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length === 3`, { what: "3 versions" });
      const rows = await p.eval(`[...document.querySelectorAll('[data-testid="history-list"] tbody tr')].map((r) => [...r.children].slice(1, 7).map((c) => c.textContent))`);
      t.eq(rows.map((r) => r[0].replace("current", "") + "|" + r[1] + "|" + r[4]), ["3|save|third change", "2|save|second change", "1|create|"], "list rows, newest first");
      t.assert(rows[0][0].includes("current"), "the newest has the mark current");
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="history-list"] th')].map((h) => h.textContent).slice(1).join("|")`), "Version|Kind|Saved at|Saved by|Message|Size|Actions", "columns");
      t.eq(await p.exists(p.tid("history-restore-3")), false, "no Restore for the current version");
      t.eq(await p.exists(p.tid("history-restore-2")), true, "Restore for an older version");
      // view version 1: read-only, with the banner and the note on each panel
      await p.click(p.tid("history-view-1"));
      await p.waitSel(p.tid("version-banner"));
      await p.idle();
      const banner = await p.text(p.tid("version-banner"));
      t.assert(/Version 1 of [^,]*\. This view is read-only\./.test(banner), "banner (no author when the subject is unknown): " + banner);
      for (const b of ["Restore this version", "Compare with current", "Export this version"]) t.assert(banner.includes(b), "banner button " + b);
      t.eq(await p.count(".history__version [data-testid^=panel-][data-state]"), 3, "three panels");
      t.assert((await p.text(p.tid("panel-1-hint"))).includes("Data not shown"), "note on a panel");
      t.assert(new URLSearchParams(await p.eval("location.search")).get("version") === "1", "URL has version=1");
      // Show data: the preview with the variables of the version
      p.clearRequests();
      await p.click(p.tid("version-show-data"));
      await p.waitFor(`document.querySelector('.history__version [data-testid="panel-2"]').dataset.state === "ready"`, { what: "preview result", timeout: 20000 });
      t.assert(p.requests.some((r) => r.method === "POST" && /\/api\/sources\/prod\/preview$/.test(r.url)), "the preview route ran");
      // Export this version
      await p.clickText(`${p.tid("version-banner")} button`, "Export this version");
      const exp = await (async () => {
        const end = Date.now() + 8000;
        while (Date.now() < end) {
          const f = existsSync(t.downloads) ? readdirSync(t.downloads).find((x) => x === "ed20-v1.json") : null;
          if (f) return join(t.downloads, f);
          await sleep(100);
        }
        throw new Error("no ed20-v1.json download");
      })();
      const v1 = await t.api.call("GET", "api/dashboards/ed20/history/1?download=true", { raw: true });
      t.eq(readFileSync(exp, "utf8"), v1.text, "exported bytes are the stored bytes");
      // Compare with current: a diff with the title of 10.11
      await p.clickText(`${p.tid("version-banner")} button`, "Compare with current");
      await p.waitSel(p.tid("diff-view"));
      await p.idle();
      const dtext = await p.text(p.tid("diff-view"));
      t.assert(dtext.includes("Changes from version 1 to version current"), "diff title: " + dtext.slice(0, 120));
      t.assert(dtext.includes("Third") && dtext.includes("@@"), "the diff shows the change");
      t.assert(new URLSearchParams(await p.eval("location.search")).get("base") === "1", "URL has base=1");
      // the browser Back goes to the earlier view
      await p.eval("history.back()");
      await p.waitSel(p.tid("version-banner"));
      await p.eval("history.back()");
      await p.waitSel(p.tid("history-list"));
      // select two rows and compare
      await p.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length === 3`);
      await p.click('[data-testid="history-list"] tr[data-version="1"] input[type=checkbox]');
      await p.click('[data-testid="history-list"] tr[data-version="2"] input[type=checkbox]');
      await p.click(p.tid("history-compare"));
      await p.waitSel(p.tid("diff-view"));
      await p.idle();
      t.assert((await p.text(p.tid("diff-view"))).includes("Changes from version 1 to version 2"), "diff of two rows");
      await p.eval("history.back()");
      await p.waitSel(p.tid("history-list"));
      await p.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length === 3`);
      // restore version 1 as a new version
      await p.click(p.tid("history-restore-1"));
      await p.waitSel(p.tid("restore-dialog"));
      t.assert((await p.text(p.tid("restore-dialog"))).includes("Restore version 1 as a new version?"), "restore dialog title");
      await p.fill(`${p.tid("restore-dialog")} textarea`, "back to the first one");
      await p.clickText(`${p.tid("restore-dialog")} button`, "Restore");
      await waitNotice(p, /Version 1 is restored as version 4\./);
      const cur = await t.api.dashboard(uid);
      t.eq(cur.json.version, 4, "version 4");
      t.eq(cur.etag, etag, "the restored bytes are the bytes of version 1 (same etag)");
      const h = await t.api.history(uid);
      t.eq([h.json.items[0].kind, h.json.items[0].message, h.json.items.length], ["restore", "back to the first one", 4], "history after the restore");
    },
  },

  {
    id: "B21",
    name: "export and import of one document; delete, then restore with hune restore",
    async run(t) {
      const tmp = join(WORK, "tmp21");
      mkdirSync(tmp, { recursive: true });
      await makeSeed(t, "exp21");
      const p = await t.open(t.url(""));
      // Export from the menu of a row: the stored canonical bytes as <uid>.json
      await p.click(p.tid("row-menu-exp21"));
      await p.clickText('.menu [role="menuitem"]', "Export");
      const f = await waitDownload(t.downloads, /^exp21\.json$/);
      const stored = await t.api.download("exp21");
      t.eq(readFileSync(f, "utf8"), stored.text, "export bytes equal the stored bytes");
      // Import: a new uid
      const doc = seedDocument("imp21", "Imported");
      const file1 = join(tmp, "imp21.json");
      writeFileSync(file1, JSON.stringify(doc, null, 4) + "\n"); // not the canonical bytes
      await openImport(p);
      await p.waitSel(p.tid("import-dialog"));
      await setFiles(p, '[data-testid="import-file"]', [file1]);
      await p.waitSel(p.tid("import-create"));
      await p.click(p.tid("import-create"));
      await p.waitFor(`location.pathname.startsWith("/d/imp21")`, { what: "dashboard opened" });
      await p.idle();
      const imp = await t.api.download("imp21");
      t.assert(imp.text !== readFileSync(file1, "utf8"), "the store keeps the canonical bytes, not the bytes of the file");
      t.eq(imp.text, (await t.api.call("PUT", "api/dashboards/imp21?validate_only=true", { body: `{"document":${readFileSync(file1, "utf8")}}` })).json.canonical, "canonical bytes");
      t.eq((await t.api.history("imp21")).json.items[0].version, 1, "version 1");
      // Import the same uid again: replace or import as new, with a diff
      doc.title = "Imported again";
      const file2 = join(tmp, "imp21b.json");
      writeFileSync(file2, JSON.stringify(doc, null, 2));
      await p.goto(t.url(""));
      await p.idle();
      await openImport(p);
      await setFiles(p, '[data-testid="import-file"]', [file2]);
      await p.waitSel(p.tid("import-replace"));
      t.eq(await p.exists(p.tid("import-new")), true, "second choice");
      t.assert((await p.text(p.tid("diff-view"))).includes("Imported again"), "diff to the file");
      await p.click(p.tid("import-new"));
      await p.waitFor(`location.pathname.startsWith("/d/") && !location.pathname.startsWith("/d/imp21")`, { what: "new uid opened" });
      await p.idle();
      const list = (await t.api.list()).json.items.filter((i) => i.title === "Imported again");
      t.eq(list.length, 1, "a new dashboard with another uid");
      t.assert(list[0].uid !== "imp21" && /^[a-z0-9]{14}$/.test(list[0].uid), "uid of 14 characters: " + list[0].uid);
      await p.goto(t.url(""));
      await p.idle();
      await openImport(p);
      await setFiles(p, '[data-testid="import-file"]', [file2]);
      await p.waitSel(p.tid("import-replace"));
      await p.click(p.tid("import-replace"));
      await p.clickText("dialog button, .dialog button", "Replace");
      await p.waitFor(`location.pathname.startsWith("/d/imp21")`, { what: "replaced dashboard opened" });
      const rep = await t.api.dashboard("imp21");
      t.eq([rep.json.document.title, rep.json.version], ["Imported again", 2], "replaced");
      // Errors of an invalid file, with pointers
      const bad = seedDocument("imp21bad", "Bad");
      bad.sections[1].panels[0].grid.w = 30;
      const file3 = join(tmp, "bad.json");
      writeFileSync(file3, JSON.stringify(bad));
      await p.goto(t.url(""));
      await p.idle();
      t.allow(p, /Failed to load resource/);
      await openImport(p);
      await setFiles(p, '[data-testid="import-file"]', [file3]);
      await p.waitFor(`!document.querySelector('[data-testid="import-errors"]').hidden`, { what: "errors" });
      t.assert((await p.text(p.tid("import-errors"))).includes("/sections/1/panels/0/grid"), "pointer in the error text: " + (await p.text(p.tid("import-errors"))));
      t.eq(await p.exists(p.tid("import-create")), false, "no import button for an invalid file");
      // A Grafana file stops the import
      const graf = join(tmp, "grafana.json");
      writeFileSync(graf, JSON.stringify({ uid: "g1", title: "G", schemaVersion: 39, panels: [] }));
      await setFiles(p, '[data-testid="import-file"]', [graf]);
      await p.waitFor(`document.querySelector('[data-testid="import-errors"]').textContent.includes("This is a Grafana file. Use the command")`, { what: "grafana message" });
      // Delete from the list: dialog, notice, tombstone on disk
      await p.goto(t.url(""));
      await p.idle();
      await p.click(p.tid("row-menu-exp21"));
      await p.clickText('.menu [role="menuitem"]', "Delete");
      await p.waitSel(p.tid("delete-dashboard-dialog"));
      t.assert((await p.text(p.tid("delete-dashboard-dialog"))).includes("Delete the dashboard Case exp21?"), "delete dialog text");
      await p.clickText(`${p.tid("delete-dashboard-dialog")} button`, "Delete");
      await waitNotice(p, /The dashboard Case exp21 is deleted\./);
      await p.waitGone(p.tid("row-exp21"));
      t.eq((await t.api.dashboard("exp21")).status, 404, "GET after the delete");
      const dir = join(t.server.volume, ".history", "exp21");
      const metas = readdirSync(dir).filter((x) => x.endsWith(".meta.json")).sort();
      t.eq(metas.length, 2, "create and delete versions on disk");
      t.assert(readFileSync(join(dir, metas[1]), "utf8").includes('"delete"'), "the last version is a deletion version");
      const again = await t.api.create("transit", seedDocument("exp21", "Again"));
      t.eq([again.status, again.json.error.code], [409, "already_exists"], "a create of the uid gives already_exists");
      // Restore with `hune restore` on a server of this case alone (the command needs the lock)
      const own = await t.ownServer("restore");
      const oapi = (await import("../lib/env.mjs")).api(own);
      const mk = await oapi.create("transit", seedDocument("res21", "To restore"));
      t.eq(mk.status, 201, "create on own server");
      const q = await t.open(own.url);
      await q.click(q.tid("row-menu-res21"));
      await q.clickText('.menu [role="menuitem"]', "Delete");
      await q.waitSel(q.tid("delete-dashboard-dialog"));
      await q.clickText(`${q.tid("delete-dashboard-dialog")} button`, "Delete");
      await q.waitGone(q.tid("row-res21"));
      t.eq((await oapi.dashboard("res21")).status, 404, "deleted on own server");
      await own.stop();
      const run = spawnSync(BINARY, ["restore", "-c", own.configPath, "--uid", "res21"], { encoding: "utf8", env: {} });
      t.eq(run.status, 0, "hune restore exit code: " + run.stdout + run.stderr);
      t.assert(/3/.test(run.stdout), "the command prints the new version: " + run.stdout);
      await own.restart();
      const back = await oapi.dashboard("res21");
      t.eq(back.status, 200, "restored dashboard answers");
      const oh = await oapi.history("res21");
      t.eq([oh.json.items[0].kind, oh.json.items[0].version], ["restore", 3], "history head after hune restore");
      await q.goto(own.url);
      await q.idle();
      t.eq(await q.exists(q.tid("row-res21")), true, "the row is back in the list");
    },
  },

  {
    id: "B22",
    name: "a dashboard of a read-only store has no edit control and no History entry; the primary store is empty (13.10, 11.25)",
    server: "noedit",
    async run(t) {
      const sess = await t.api.session();
      t.eq(sess.json.editing, true, "session.editing is always true: a primary store always exists");
      t.eq(Object.fromEntries(sess.json.stores.map((x) => [x.id, x.read_only])), { main: true, scratch: false }, "the seed volume is read-only and the scratch volume is the primary store");
      const p = await t.open(t.url(""));
      t.eq(await p.exists(p.tid("row-menu-nodes")), false, "a card that cannot be edited has no three dots");
      // The volume is a read-only store: each card has no lock and no three dots.
      t.eq(await p.exists(p.tid("row-menu-stale-one")), false, "a card of a read-only store has no three dots");
      t.eq(await p.count('[data-testid^="card-lock-"], .card__mark--lock'), 0, "a card of a read-only store has no lock");
      // The flags of the answer: no write and no history in a read-only store, the export follows the key.
      const one = await t.api.dashboard("nodes");
      t.eq(one.json.access, { write: false, history: false, export: true }, "the flags of a dashboard of a read-only store");
      // the dashboard page: no edit toggle, and edit=1 opens no editor
      const d = await t.open(t.dash("nodes"));
      await waitPanelsReady(d, [1, 2, 3]);
      t.eq(await d.exists(d.tid("edit-toggle")), false, "no edit toggle");
      t.eq(await d.exists(d.tid("read-only-lock")), false, "a dashboard of a read-only store shows no lock");
      const e = await t.open(t.dash("nodes", "edit=1"));
      await waitPanelsReady(e, [1, 2, 3]);
      const forbidden = await e.eval(`["edit-toggle", "edit-bar", "save-button", "add-panel", "add-section", "json-tab", "discard-button", "edit-grid"].filter((id) => document.querySelector('[data-testid="' + id + '"]'))`);
      t.eq(forbidden, [], "no edit control with edit=1");
      t.eq(await e.eval(`[...document.querySelectorAll("button")].filter((b) => /^(Edit|Save|Restore|Delete|Move|Duplicate)/.test(b.textContent.trim())).length`), 0, "no edit button by text");
      // the panel menu has no edit entry
      await e.click(e.tid("panel-2-menu"));
      t.eq(await e.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((x) => x.textContent)`), ["View", "View data", "Download CSV"], "panel menu");
      await e.press("Escape");
      // The menu More actions has Export and no History.
      await d.click(d.tid("dashboard-menu"));
      t.eq(await d.exists(d.tid("history-button")), false, "no History entry");
      t.eq(await d.exists(d.tid("export-button")), true, "the Export entry stays");
      await d.press("Escape");
      // A history URL opened by hand shows the standard refusal page, and the server answers history_unavailable.
      const h = await t.open(t.dash("nodes", "editview=history"));
      await h.waitSel(h.tid("dashboard-forbidden"));
      t.eq(await h.exists(h.tid("history")), false, "no history page");
      const hist = await t.api.history("nodes");
      t.eq([hist.status, hist.json.error.code], [404, "history_unavailable"], "the history route of a read-only store");
      // the server refuses a write into the read-only store
      const w = await t.api.create("transit", seedDocument("nope", "Nope"));
      t.eq([w.status, w.json.error.code], [403, "read_only_store"], "write refused by the server");
      void backToDashboard;
      void sleep;
    },
  },
  {
    id: "B230",
    name: "a new folder: the dialog asks for a title only and the page makes the id from it, with a short suffix when the id exists; the import makes a folder on the fly (10.7, 10.12)",
    async run(t) {
      const tmp = join(WORK, "tmp230");
      mkdirSync(tmp, { recursive: true });
      await makeSeed(t, "fid230");
      const p = await t.open(t.url(""));
      // The two answers that the page expects: 409 for a folder id that exists (it tries again with a suffix), 404 for the uid of an import (it checks if it exists).
      t.allow(p, /Failed to load resource: the server responded with a status of (409|404)/);
      await p.waitSel(p.tid("list-new-menu"));
      const folders = () => p.eval(`fetch("/api/folders").then((r) => r.json()).then((j) => j.items.map((f) => f.id + "|" + f.title))`);
      // The dialog "New folder" has one field, the title. The id follows the title: no accent, one dash for each run of other characters.
      const makeFolder = async (title) => {
        await p.click(p.tid("list-new-menu"));
        await p.click(p.tid("new-folder"));
        await p.waitSel(p.tid("new-folder-dialog"));
        t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="new-folder-dialog"] input, [data-testid="new-folder-dialog"] textarea, [data-testid="new-folder-dialog"] select')].map((e) => e.getAttribute("aria-label"))`), ["Title"], "the dialog has the field Title only");
        await p.fill(p.tid("folder-title"), title);
        await p.clickText("dialog button, .dialog button", "Create");
        await p.waitGone(p.tid("new-folder-dialog"));
        await p.idle();
      };
      await makeFolder("Équipe réseau");
      t.assert((await folders()).includes("equipe-reseau|Équipe réseau"), "the id is made from the title: " + JSON.stringify(await folders()));
      // The same title again: the id exists, so the page adds a dash and four characters, and the folder has the same title.
      await makeFolder("Équipe réseau");
      const again = (await folders()).filter((f) => f.endsWith("|Équipe réseau"));
      t.eq(again.length, 2, "two folders have the title: " + JSON.stringify(again));
      t.assert(again.some((f) => /^equipe-reseau-[a-z0-9]{4}\|/.test(f)), "the second id has a short suffix: " + JSON.stringify(again));
      // A title with no letter and no digit gives folder- and four characters.
      await makeFolder("***");
      t.assert((await folders()).some((f) => /^folder-[a-z0-9]{4}\|\*\*\*$/.test(f)), "a title of symbols: " + JSON.stringify(await folders()));

      // The import has the entry "New folder..." in the select of the folders: the folder is made when the person chooses "Import".
      const doc = seedDocument("imp230", "Imported on the fly");
      const file = join(tmp, "imp230.json");
      writeFileSync(file, JSON.stringify(doc, null, 2) + "\n");
      await openImport(p);
      await p.waitSel(p.tid("import-dialog"));
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="import-folder"] option')].pop().textContent`), "New folder...", "the last entry of the select");
      t.eq(await p.eval(`document.querySelector('[data-testid="import-new-folder-title"]').hidden`), true, "the field of the title is hidden until the entry is chosen");
      await setFiles(p, '[data-testid="import-file"]', [file]);
      await p.waitSel(p.tid("import-create"));
      await p.select(p.tid("import-folder"), "::new-folder::");
      t.eq(await p.eval(`document.querySelector('[data-testid="import-new-folder-title"]').hidden`), false, "the field of the title shows");
      await p.fill(p.tid("import-new-folder-title"), "Imported team");
      await p.click(p.tid("import-create"));
      await p.waitFor(`location.pathname.startsWith("/d/imp230")`, { what: "dashboard opened" });
      t.assert((await folders()).includes("imported-team|Imported team"), "the folder was made: " + JSON.stringify(await folders()));
      const item = await p.eval(`fetch("/api/dashboards").then((r) => r.json()).then((j) => j.items.find((i) => i.uid === "imp230").folder)`);
      t.eq(item, "imported-team", "the dashboard is in the new folder");
    },
  },
  {
    id: "B306",
    name: "the keys server.history and server.export hide the entries: the card menu, the menu More actions, the version view, and the history URL by hand (6.12, 9.9, 10.11, 10.14)",
    async run(t) {
      const menu = (page) => page.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent.trim()).filter((x) => !/time range|Zoom/.test(x))`);
      // Both keys true: History, Export and Delete, and the card menu has Export.
      const p = await t.open(t.url(""));
      await p.waitSel(p.tid("row-menu-nodes"));
      await p.click(p.tid("row-menu-nodes"));
      t.eq(await menu(p), ["Edit", "Export", "Move", "Delete"], "the card menu with both keys");
      await p.press("Escape");
      const d = await t.open(t.dash("nodes"));
      await waitPanelsReady(d, [1]);
      await d.click(d.tid("dashboard-menu"));
      t.eq(await menu(d), ["History", "Export", "Delete"], "the menu with both keys");
      await d.press("Escape");

      // Both keys false: no History, no Export, no Download; the history URL shows the page of 403; the routes refuse.
      const off = await t.ownServer("off", { history: false, export: false });
      const po = await t.open(`${off.url}`);
      await po.waitSel(po.tid("row-menu-nodes"));
      await po.click(po.tid("row-menu-nodes"));
      t.eq(await menu(po), ["Edit", "Move", "Delete"], "the card menu without export");
      await po.press("Escape");
      const doff = await t.open(`${off.url}d/nodes?${t.range}`);
      await waitPanelsReady(doff, [1]);
      await doff.click(doff.tid("dashboard-menu"));
      t.eq(await menu(doff), ["Delete"], "the menu without history and export");
      await doff.press("Escape");
      const hoff = await t.open(`${off.url}d/nodes?${t.range}&editview=history`);
      await hoff.waitSel(hoff.tid("dashboard-forbidden"));
      t.eq(await hoff.exists(hoff.tid("history")), false, "no history page");
      const offApi = (await import("../lib/env.mjs")).api(off);
      const flags = await offApi.dashboard("nodes");
      t.eq(flags.json.access, { write: true, history: false, export: false }, "the flags with both keys false");
      const hist = await offApi.history("nodes");
      t.eq([hist.status, hist.json.error.code], [403, "history_disabled"], "the history route");
      const dl = await offApi.call("GET", "api/dashboards/nodes?download=true");
      t.eq([dl.status, dl.json.error.code], [403, "export_disabled"], "the download");

      // History true, export false: History stays; a version view has no export button; Compare with current works.
      const noExport = await t.ownServer("noexport", { export: false });
      const noApi = (await import("../lib/env.mjs")).api(noExport);
      const first = await noApi.dashboard("nodes");
      const doc = JSON.parse(JSON.stringify(first.json.document));
      doc.title = "Nodes changed";
      const saved = await noApi.save("nodes", first.json.etag, doc, "second");
      t.eq(saved.status, 200, "a save makes a version");
      const pn = await t.open(`${noExport.url}d/nodes?${t.range}`);
      await waitPanelsReady(pn, [1]);
      await pn.click(pn.tid("dashboard-menu"));
      t.eq(await menu(pn), ["History", "Delete"], "the menu without export");
      await pn.press("Escape");
      const hn = await t.open(`${noExport.url}d/nodes?${t.range}&editview=history&version=1`);
      await hn.waitSel(hn.tid("version-banner"));
      t.eq(await hn.exists(hn.tid("version-export")), false, "no Export this version");
      const diff = await t.open(`${noExport.url}d/nodes?${t.range}&editview=history&base=1&version=current`);
      await diff.waitSel(".history__diff .diff, .history__diff table, .history__diff pre");
      t.eq(await diff.exists(".history__diff .field-error"), false, "the comparison with the current text works without the export right");
    },
  },
];
