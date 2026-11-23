// Browser cases B16, B17, B18 (16.10): the editor against the real server.
import { readdirSync } from "node:fs";
import { join } from "node:path";
import { backToDashboard, byLabel, draftOf, enterEdit, makeSeed, noticeTexts, openPanelEditor, saveDialog, setPanelTitle, waitDraft, waitNotice, moreItem } from "../lib/ed.mjs";
import { historyFiles, panelQueries, sleep, waitPanelsReady } from "../lib/h.mjs";

const TOP = ".peditor__top";
const previews = (p) => p.requests.filter((r) => r.method === "POST" && /\/api\/sources\/[^/]+\/preview$/.test(r.url));

export const cases = [
  {
    id: "B16",
    name: "editor: SQL tab and Run of the draft, the panel form, the JSON tab lists each error by pointer",
    async run(t) {
      await makeSeed(t, "ed16");
      const p = await t.open(t.dash("ed16"));
      await waitPanelsReady(p, [1, 2, 3]);
      await enterEdit(p);
      await openPanelEditor(p, 2);
      // the panel shows the result of the stored query
      await p.waitFor(`document.querySelectorAll('${TOP} [data-testid="table-2"] tbody tr').length === 30`, { what: "stored result in the top area" });
      // change the SQL: the draft differs from the stored query, so the panel asks for "Run"
      const sql = p.tid("sql-input");
      const base = await p.eval(`document.querySelector('[data-testid="sql-input"]').value`);
      t.assert(base.startsWith("SELECT node, weight, blocks, active\nFROM analytics.seed"), "SQL text of the stored lines: " + base);
      await p.fill(sql, "SELECT node, blocks\nFROM analytics.seed\nWHERE blocks < 3");
      await p.waitSel(p.tid("panel-2-hint"));
      t.eq((await p.text(p.tid("panel-2-hint"))).trim().startsWith("Run the query to see the data."), true, "hint text");
      p.clearRequests();
      await p.click(p.tid("run-button"));
      await p.waitFor(`document.querySelectorAll('${TOP} [data-testid="table-2"] tbody tr').length === 3`, { what: "3 rows after Run" });
      const pv = previews(p);
      t.eq(pv.length, 1, "one preview request");
      const body = JSON.parse(pv[0].body);
      t.eq(body.uid, "ed16", "the body has the uid of a saved dashboard");
      t.eq(body.panel.data.query.sql, ["SELECT node, blocks", "FROM analytics.seed", "WHERE blocks < 3"], "the draft SQL is sent as lines");
      t.assert(pv[0].url.endsWith("/api/sources/prod/preview"), "preview route");
      // Ctrl+Enter runs too
      await p.fill(sql, "SELECT node, blocks\nFROM analytics.seed\nWHERE blocks < 5");
      await sleep(400);
      await p.focus(sql);
      p.clearRequests();
      await p.press("Enter", { modifiers: 2 });
      await p.waitFor(`document.querySelectorAll('${TOP} [data-testid="table-2"] tbody tr').length === 5`, { what: "5 rows after Ctrl+Enter" });
      t.eq(previews(p).length, 1, "Ctrl+Enter sends one preview");
      // an SQL error shows in the error box with its code
      await p.fill(sql, "SELEC 1");
      await sleep(400);
      t.allow(p, /Failed to load resource/);
      await p.click(p.tid("run-button"));
      await p.waitFor(`!document.querySelector('[data-testid="panel-errors"]').hidden`, { what: "error box" });
      const errText = await p.text(p.tid("panel-errors"));
      t.assert(/sql|syntax|SELEC/i.test(errText), "SQL error text: " + errText);
      await p.fill(sql, "SELECT node, blocks\nFROM analytics.seed\nWHERE blocks < 5");
      await sleep(400);
      // tab "Panel": the form changes the draft
      await p.click(p.tid("tab-panel"));
      await p.fill(await byLabel(p, "Title"), "Form title");
      await p.fill(await byLabel(p, "Unit"), "percent");
      await p.fill(await byLabel(p, "Decimals"), "1");
      await sleep(500);
      await p.click(p.tid("tab-json"));
      const pj = JSON.parse(await p.eval(`document.querySelector('[data-testid="panel-json"]').value`));
      t.eq([pj.title, pj.fields.defaults.unit, pj.fields.defaults.decimals], ["Form title", "percent", 1], "form values are in the panel JSON");
      t.eq(pj.data.query.sql.length, 3, "SQL in the panel JSON");
      // tab "JSON": bad syntax shows the line and column, the draft keeps the last valid panel
      await p.fill(p.tid("panel-json"), '{"id": 2,');
      await p.waitFor(`[...document.querySelectorAll('[data-testid="panel-editor"] .field-error')].some((e) => !e.hidden && /Line \\d+, column \\d+/.test(e.textContent))`, { what: "syntax error" });
      // a valid edit applies
      pj.title = "From JSON";
      await p.fill(p.tid("panel-json"), JSON.stringify(pj, null, 2));
      await sleep(500);
      const d = await waitDraft(p, "ed16", (x) => x.document.sections[1].panels[0].title === "From JSON");
      t.eq(d.document.sections[1].panels[0].fields.defaults.unit, "percent", "draft holds the unit");
      // a panel with errors in the JSON tab: pointers relative to the panel
      // A type that no version of Hune has (heatmap became a type with F31).
      pj.type = "nosuchtype";
      await p.fill(p.tid("panel-json"), JSON.stringify(pj, null, 2));
      await p.waitFor(`!document.querySelector('[data-testid="panel-json-errors"]').hidden`, { what: "panel json errors", timeout: 8000 });
      t.assert((await p.text(p.tid("panel-json-errors"))).includes("/type"), "relative pointer /type");
      await p.fill(p.tid("panel-json"), JSON.stringify({ ...pj, type: "table" }, null, 2));
      await sleep(400);
      // Dashboard JSON tab: a text with 3 errors gives 3 lines, with the pointers of the server
      await backToDashboard(p);
      await moreItem(p, "json-tab");
      await p.waitSel(p.tid("json-text"));
      const cur = JSON.parse(await p.eval(`document.querySelector('[data-testid="json-text"]').value`));
      cur.sections[0].panels[0].type = "nosuchtype";
      cur.sections[1].panels[0].grid.w = 30;
      cur.title = "";
      const text = JSON.stringify(cur, null, 2);
      const api = await t.api.call("PUT", "api/dashboards/ed16?validate_only=true", { body: `{"document":${text}}` });
      t.eq(api.status, 422, "the server refuses the text");
      const want = api.json.error.errors.map((e) => e.pointer);
      t.eq(want.length, 3, "the fixture has 3 errors: " + JSON.stringify(want));
      t.allow(p, /Failed to load resource/);
      await p.fill(p.tid("json-text"), text);
      await p.click(p.tid("json-check"));
      await p.waitFor(`document.querySelectorAll('[data-testid="json-errors"] li').length === 3`, { what: "3 error lines" });
      const lines = await p.eval(`[...document.querySelectorAll('[data-testid="json-errors"] li')].map((e) => e.textContent)`);
      for (const ptr of want) t.assert(lines.some((l) => l.includes(ptr === "" ? "(document)" : ptr)), `line for ${ptr}: ${JSON.stringify(lines)}`);
      t.eq(await p.eval(`document.querySelector('[data-testid="json-apply"]').disabled`), true, "Apply is off with errors");
      // a duplicate member is found in the page, with line and column
      await p.fill(p.tid("json-text"), '{\n  "a": 1,\n  "a": 2\n}');
      await p.click(p.tid("json-check"));
      await p.waitFor(`/line 3/.test(document.querySelector('[data-testid="json-errors"]').textContent)`, { what: "duplicate member at line 3" });
      // a valid text: Apply is active and changes the draft
      cur.title = "Applied title";
      cur.sections[0].panels[0].type = "timeseries";
      cur.sections[1].panels[0].grid.w = 24;
      await p.fill(p.tid("json-text"), JSON.stringify(cur, null, 2));
      await p.click(p.tid("json-check"));
      await p.waitFor(`!document.querySelector('[data-testid="json-apply"]').disabled`, { what: "Apply active" });
      await p.click(p.tid("json-apply"));
      await waitDraft(p, "ed16", (x) => x.document.title === "Applied title");
    },
  },

  {
    id: "B17",
    name: "editor: add, duplicate, delete a panel; add, rename, collapse a section; grid numbers; move up and down",
    async run(t) {
      await makeSeed(t, "ed17");
      const p = await t.open(t.dash("ed17"));
      await waitPanelsReady(p, [1, 2, 3]);
      await enterEdit(p);
      const panels = async (si) => (await draftOf(p, "ed17")).document.sections[si].panels;
      const G = ".edit-grid";
      // add a panel (to the last section): defaults of 10.6, and the panel editor opens
      await moreItem(p, "add-panel");
      await p.waitSel(p.tid("panel-editor"));
      const d1 = await waitDraft(p, "ed17", (d) => d.document.sections[1].panels.length === 2);
      const np = d1.document.sections[1].panels.find((x) => x.id === 4);
      t.assert(np, "new panel has id 4 (highest id + 1)");
      t.eq(np.type, "table", "new type");
      t.eq(np.title, "New panel", "new title");
      t.eq(np.grid, { x: 0, y: 10, w: 12, h: 8 }, "new grid: bottom of the section");
      t.eq(np.data.query.source, "prod", "first source in id order");
      t.eq([np.data.query.format, np.data.query.sql], ["table", ["SELECT 1"]], "new query");
      t.eq(np.transforms, [], "transforms");
      t.assert(new URLSearchParams(await p.eval("location.search")).get("editPanel") === "4", "editPanel=4 in the URL");
      await backToDashboard(p);
      // duplicate: a new id, y = original y + h
      await p.click(p.tid("panel-4-menu"));
      await p.click(p.tid("panel-4-duplicate"));
      let list = (await waitDraft(p, "ed17", (d) => d.document.sections[1].panels.length === 3)).document.sections[1].panels;
      const dup = list.find((x) => x.id === 5);
      t.assert(dup && dup.title === "New panel" && dup.grid.y === 18, "duplicate: id 5 at y 18: " + JSON.stringify(dup && dup.grid));
      // Move up for panel 5: the two panels exchange x and y
      await p.click(p.tid("panel-5-menu"));
      await p.click(p.tid("panel-5-up"));
      list = (await waitDraft(p, "ed17", (d) => d.document.sections[1].panels.find((x) => x.id === 5).grid.y === 10)).document.sections[1].panels;
      t.eq(list.find((x) => x.id === 4).grid.y, 18, "panel 4 took y 18");
      // Move down brings it back
      await p.click(p.tid("panel-5-menu"));
      await p.click(p.tid("panel-5-down"));
      await waitDraft(p, "ed17", (d) => d.document.sections[1].panels.find((x) => x.id === 5).grid.y === 18);
      // the first and the last panel have no active move
      await p.click(p.tid("panel-2-menu"));
      t.eq(await p.eval(`document.querySelector('[data-testid="panel-2-up"]').disabled`), true, "first panel: Move up is off");
      await p.press("Escape");
      await p.click(p.tid("panel-5-menu"));
      t.eq(await p.eval(`document.querySelector('[data-testid="panel-5-down"]').disabled`), true, "last panel: Move down is off");
      await p.press("Escape");
      // delete a panel with a dialog
      await p.click(p.tid("panel-4-menu"));
      await p.click(p.tid("panel-4-delete"));
      await p.waitSel(p.tid("delete-panel-dialog"));
      t.assert((await p.text(`${p.tid("delete-panel-dialog")} h2, ${p.tid("delete-panel-dialog")} .dialog__title`)).includes("Delete the panel New panel?"), "dialog title");
      await p.clickText(`${p.tid("delete-panel-dialog")} button`, "Delete");
      list = (await waitDraft(p, "ed17", (d) => d.document.sections[1].panels.length === 2)).document.sections[1].panels;
      t.eq(list.map((x) => x.id), [2, 5], "panel 4 is gone");
      t.eq(list.find((x) => x.id === 5).grid.y, 10, "layout normalized after the delete");
      // a panel that others reuse cannot be deleted
      await p.click(p.tid("panel-2-menu"));
      await p.click(p.tid("panel-2-delete"));
      await waitNotice(p, /The panels Total weight reuse this panel\. Change them first\./);
      t.eq(await p.exists(p.tid("delete-panel-dialog")), false, "no dialog for a reused panel");
      // add a section, rename it, check the empty title error, collapse it
      await moreItem(p, "add-section");
      let doc = (await waitDraft(p, "ed17", (d) => d.document.sections.length === 3)).document;
      t.eq([doc.sections[2].title, doc.sections[2].collapsed, doc.sections[2].panels], ["New section", false, []], "new section");
      await p.click(p.tid("section-2-rename"));
      await p.fill(p.tid("section-2-title-input"), "");
      await p.press("Enter");
      await p.waitFor(`[...document.querySelectorAll(".section__head .field-error")].some((e) => !e.hidden && e.textContent.length > 0)`, { what: "empty title error" });
      await p.fill(p.tid("section-2-title-input"), "Renamed section");
      await p.press("Enter");
      doc = (await waitDraft(p, "ed17", (d) => d.document.sections[2].title === "Renamed section")).document;
      await p.click(p.tid("section-2-rename"));
      await p.fill(p.tid("section-2-title-input"), "Cancelled");
      await p.press("Escape");
      await p.waitGone(p.tid("section-2-title-input"));
      await p.click(`${G} [data-testid="section-1"] .section__toggle`);
      doc = (await waitDraft(p, "ed17", (d) => d.document.sections[1].collapsed === true)).document;
      t.eq(doc.sections[2].title, "Renamed section", "Escape cancels the rename");
      // "Add panel here" adds to that section
      await p.click(p.tid("section-2-add-panel"));
      await p.waitSel(p.tid("panel-editor"));
      doc = (await waitDraft(p, "ed17", (d) => d.document.sections[2].panels.length === 1)).document;
      t.eq(doc.sections[2].panels[0].id, 6, "panel 6 in the section of the button");
      t.eq(doc.sections[2].panels[0].grid, { x: 0, y: 0, w: 12, h: 8 }, "panel 6 at the top of the empty section");
      // grid numbers in the panel editor: valid values apply, x + w above 24 does not
      await p.click(p.tid("tab-panel"));
      await p.fill(await byLabel(p, "w"), "6");
      doc = (await waitDraft(p, "ed17", (d) => d.document.sections[2].panels[0].grid.w === 6)).document;
      await p.fill(await byLabel(p, "x"), "20");
      await sleep(500);
      t.assert(await p.eval(`[...document.querySelectorAll('[data-testid="panel-editor"] fieldset.position .field-error')].some((e) => !e.hidden && e.textContent.length > 0)`), "position error is shown");
      t.eq((await draftOf(p, "ed17")).document.sections[2].panels[0].grid.x, 0, "the invalid position did not apply");
      await backToDashboard(p);
      // the draft saves as one version
      await saveDialog(p, "layout work");
      await p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null`, { what: "edit mode left" });
      const saved = await t.api.dashboard("ed17");
      t.eq(saved.json.version, 2, "version 2");
      t.eq(saved.json.document.sections.map((s) => s.panels.length), [2, 2, 1], "saved layout");
      t.eq(saved.json.document.sections[1].collapsed, true, "saved collapsed flag");
      t.eq(saved.json.document.sections[2].title, "Renamed section", "saved section title");
      void noticeTexts;
    },
  },

  {
    id: "B18",
    name: "save: If-Match and Idempotency-Key, retry after a network failure with the same key, draft kept in localStorage",
    async run(t) {
      const { etag } = await makeSeed(t, "ed18");
      const filesBefore = await historyFiles(t.server, "ed18");
      const p = await t.open(t.dash("ed18"));
      await waitPanelsReady(p, [1, 2, 3]);
      await enterEdit(p);
      await setPanelTitle(p, 2, "Saved title");
      await waitDraft(p, "ed18", (d) => d.document.sections[1].panels[0].title === "Saved title");
      // inject a network failure for the save
      const isSave = (r) => r.method === "PUT" && /\/api\/dashboards\/ed18$/.test(r.url);
      const stop = p.failWhen(isSave);
      p.clearRequests();
      t.allow(p, /Failed to load resource|ERR_/);
      await saveDialog(p, "first try");
      await p.waitFor(`document.querySelector('[data-testid="save-dialog"] .save-status')?.textContent.includes("Saving is not possible now. Your draft is kept in this browser.")`, { what: "retry text" });
      t.eq(await p.eval(`document.querySelector('[data-testid="save-confirm"]').textContent.trim()`), "Retry", "the button says Retry");
      const first = p.requests.filter((r) => isSave(r));
      t.eq(first.length, 1, "one failed request");
      const h1 = first[0].headers;
      const lower = (h) => Object.fromEntries(Object.entries(h).map(([k, v]) => [k.toLowerCase(), v]));
      t.eq(lower(h1)["if-match"], `"${etag}"`, "If-Match is the quoted base etag");
      t.assert(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(lower(h1)["idempotency-key"] || ""), "Idempotency-Key is a UUID v4: " + lower(h1)["idempotency-key"]);
      t.eq(JSON.parse(first[0].body).message, "first try", "message in the body");
      // the draft is in localStorage, with the save_id
      const dr = await draftOf(p, "ed18");
      t.eq(dr.save_id, lower(h1)["idempotency-key"], "the draft keeps the save_id");
      t.eq(dr.base_etag, etag, "draft base etag");
      // the retry has the same Idempotency-Key and goes through
      stop();
      p.clearRequests();
      await p.click(p.tid("save-confirm"));
      await p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null`, { what: "saved" });
      const second = p.requests.filter((r) => isSave(r));
      t.eq(second.length, 1, "one retry request");
      t.eq(lower(second[0].headers)["idempotency-key"], lower(h1)["idempotency-key"], "same Idempotency-Key on the retry");
      t.eq(second[0].status, 200, "retry answer");
      await waitNotice(p, /Saved as version 2\./);
      // the new etag through the API, one new version on disk, the draft removed
      const after = await t.api.dashboard("ed18");
      t.assert(after.etag !== etag && after.json.etag !== etag, "new etag");
      t.eq(after.json.document.sections[1].panels[0].title, "Saved title", "stored title");
      const filesAfter = await historyFiles(t.server, "ed18");
      t.eq(filesAfter.length, filesBefore.length + 2, "one new version on disk (document and meta file): " + filesAfter);
      t.eq(await draftOf(p, "ed18"), null, "draft removed after the save");
      const hist = await t.api.history("ed18");
      t.eq([hist.json.items[0].version, hist.json.items[0].kind, hist.json.items[0].message], [2, "save", "first try"], "history head");
      // the same request again (a lost answer): the stored answer, one version
      const replay = await t.api.call("PUT", "api/dashboards/ed18", {
        body: JSON.parse(first[0].body),
        headers: { "If-Match": `"${etag}"`, "Idempotency-Key": lower(h1)["idempotency-key"] },
      });
      t.eq(replay.status, 200, "replay status");
      t.eq(replay.headers.get("idempotent-replayed"), "true", "Idempotent-Replayed");
      t.eq((await t.api.history("ed18")).json.items.length, 2, "still two versions");
      // the draft stays after a new load until the save succeeds
      await p.reload();
      await p.idle();
      await enterEdit(p);
      await setPanelTitle(p, 2, "Draft only");
      await waitDraft(p, "ed18", (d) => d.document.sections[1].panels[0].title === "Draft only");
      // a reload in edit mode (edit=1 in the URL) opens the edit mode with the draft
      await p.reload();
      await p.waitSel(p.tid("edit-bar"));
      await p.idle();
      t.eq(await p.eval(`document.querySelector('.edit-grid [data-testid="panel-2"] .panel__title').textContent`), "Draft only", "reload in edit mode keeps the draft");
      // a load of the view URL shows the notice with Resume and Discard
      await p.goto(t.dash("ed18"));
      await p.idle();
      await waitNotice(p, /You have changes that are not saved, from /);
      t.eq((await draftOf(p, "ed18")).document.sections[1].panels[0].title, "Draft only", "draft after a new load");
      await p.clickText('[data-testid="notice"] button', "Resume");
      await p.waitSel(p.tid("edit-bar"));
      await p.idle();
      t.eq(await p.eval(`document.querySelector('.edit-grid [data-testid="panel-2"] .panel__title').textContent`), "Draft only", "Resume shows the draft");
      await saveDialog(p, "");
      await p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null`, { what: "saved again" });
      t.eq(await draftOf(p, "ed18"), null, "draft removed after the second save");
      t.eq((await t.api.dashboard("ed18")).json.version, 3, "version 3");
      // Discard removes the draft
      await enterEdit(p);
      await setPanelTitle(p, 2, "To discard");
      await waitDraft(p, "ed18");
      await p.click(p.tid("discard-button"));
      await p.clickText(".dialog button, dialog button", "Discard");
      await p.waitFor(`localStorage.getItem("hune.draft.v1.ed18") === null`, { what: "draft removed by Discard" });
      void panelQueries;
      void readdirSync;
      void join;
    },
  },
];
