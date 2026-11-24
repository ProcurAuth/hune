// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B23 (16.10): mode `authzen` with `fake-pdp`.
//
// One server in mode `authzen`, one `fake-pdp` with the policy of lib/authzen.mjs, and a browser that sends the
// identity token in the configured header (the edge that adds it is absent: Network.setExtraHTTPHeaders).
// Parts: (1) the display filter of the lists, (2) the controls without dashboard.write, (3) a refused read,
// (4) "Run" without source.query, (5) a missing, an expired and a bad token, (6) the PDP down, (7) the move conflict.
import { HEADER } from "../lib/authzen.mjs";
import { enterEdit, noticeTexts, openPanelEditor, saveDialog, setPanelTitle, waitNotice, waitDraft, moreItem } from "../lib/ed.mjs";
import { seedDocument } from "../lib/env.mjs";
import { panelStates, sleep, waitPanelsReady } from "../lib/h.mjs";

const PDP_TEXT = /The access decision service does not answer\. Try again later\./;

/** A page that sends the identity token of `sub`, loaded on `path` under the base path. */
async function openAs(t, a, sub, path, { wait = "idle" } = {}) {
  const p = await t.page();
  await p.setHeaders(a.headers(sub));
  await p.goto(a.server.url + path);
  if (wait === "idle") await p.idle();
  return p;
}

const rowIds = (p) => p.eval(`[...document.querySelectorAll('[data-testid^="row-"]:not([data-testid^="row-menu-"])')].map((e) => e.dataset.testid.slice(4))`);
const folderIds = (p) => p.eval(`[...document.querySelectorAll('section[data-testid^="folder-"]')].map((e) => e.dataset.testid.slice(7))`);
const menuItems = (p) => p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent)`);
const toolbar = (p) => p.eval(`["new-dashboard", "import", "new-folder"].filter((id) => document.querySelector('[data-testid="' + id + '"]'))`);

/** The controls of the edit mode that must be absent. */
const EDIT_CONTROLS = ["edit-toggle", "edit-bar", "save-button", "add-panel", "add-section", "json-tab", "discard-button", "edit-grid"];

/** The notices and the first paragraph of the full page of the main area. */
async function screenText(p) {
  return p.eval(`(document.querySelector('[data-testid="full-page"], [data-testid="unauthorized"], [data-testid="no-answer"], [data-testid="dashboard-error"], [data-testid="dashboard-forbidden"], [data-testid="dashboard-not-found"]') || {}).textContent || ""`);
}

export const cases = [
  {
    id: "B23",
    name: "mode authzen with fake-pdp: display filter, no edit control, no Run, 401 page, PDP down notice, move conflict",
    async run(t) {
      const a = await t.authzen();
      const server = a.server;

      // ---- 0. The server answers 401 to a request without a token; the page of the session needs the token only.
      const bare = await fetch(server.origin + "/");
      t.eq(bare.status, 401, "GET / without a token");
      t.eq((await bare.json()).error.code, "unauthenticated", "401 code");
      t.eq((await fetch(server.origin + "/.well-known/procurauth/actions")).status, 200, "action document needs no token");

      // ---- 1. admin: every item and every control; the identity header goes with each request.
      {
        const p = await openAs(t, a, "admin", "");
        t.eq(await folderIds(p), ["desk", "lab", "secret", "transit"], "admin sees the four folders");
        t.eq(await toolbar(p), ["new-dashboard", "import", "new-folder"], "admin toolbar");
        const api = p.requests.filter((r) => /\/api\//.test(r.url));
        t.assert(api.length >= 3, "the page made API calls");
        for (const r of api) t.assert(r.headers[HEADER] || r.headers[HEADER.toLowerCase()], `request ${r.url} carries ${HEADER}`);
        t.eq((await a.call("admin", "GET", "api/session")).json.subject, "admin", "session subject is the token subject");
        const d = await openAs(t, a, "admin", "d/nodes?" + t.range);
        await waitPanelsReady(d, [1, 2, 3]);
        t.eq(await d.exists(d.tid("edit-toggle")), true, "admin has the edit toggle");
        t.note("1 admin: folders, toolbar and edit toggle present");
      }

      // ---- 2. reader: the display filter and no edit control.
      {
        const p = await openAs(t, a, "reader", "");
        t.eq(await folderIds(p), ["desk", "transit"], "reader sees the folders desk and transit only (secret and lab are hidden)");
        const rows = await rowIds(p);
        t.eq(rows.sort(), ["desk-open", "grid-errors", "nodes"], "reader sees desk-open (one dashboard grant), and the dashboards of transit, no desk-closed");
        t.eq(await toolbar(p), [], "no New dashboard, Import or New folder");
        t.eq(await p.count('.group__head button[aria-haspopup="menu"]'), 0, "no folder menu");
        // A reader has no button on a card (9.7): no pencil, no bin and no menu. The dashboard page has Export.
        t.eq(await p.count('[data-testid^="row-menu-"], [data-testid^="card-edit-"], [data-testid^="card-delete-"]'), 0, "no button on a card of a reader");
        t.eq(await p.count("article[data-testid^=\"row-\"] button:not(.list-tag), article[data-testid^=\"row-\"] .card__edit"), 0, "a card of a reader holds no button but the tag chips");
        // The list shows only what the display filter gives.
        t.eq(await p.exists(p.tid("row-hidden-one")), false, "no hidden-one in the DOM");
        t.eq(await p.exists(p.tid("row-lab-a")), false, "no lab-a in the DOM");
        t.assert(!(await p.eval("document.body.textContent")).includes("Hidden one"), "the title of a hidden dashboard is nowhere in the page");

        // The dashboard page: the panels load, there is no edit control, edit=1 opens no editor.
        const d = await openAs(t, a, "reader", "d/nodes?" + t.range + "&edit=1");
        await waitPanelsReady(d, [1, 2, 3]);
        t.eq(await d.eval(`${JSON.stringify(EDIT_CONTROLS)}.filter((id) => document.querySelector('[data-testid="' + id + '"]'))`), [], "no edit control with edit=1");
        t.eq(await d.eval(`[...document.querySelectorAll("button")].filter((b) => /^(Edit|Save|Restore|Delete|Move|Duplicate)/.test(b.textContent.trim())).length`), 0, "no edit button by text");
        await d.click(d.tid("panel-2-menu"));
        t.eq(await menuItems(d), ["View", "View data", "Download CSV"], "panel menu has no edit entry");
        await d.press("Escape");
        await d.click(d.tid("dashboard-menu"));
        // A reader has History (dashboard.read reads the history, 05) and Export in the menu "More actions"; no Delete.
        t.eq(await menuItems(d), ["History", "Export"], "dashboard menu of a reader: History and Export");
        await d.press("Escape");
        // The history shows (dashboard.read), with no Restore.
        await moreItem(d, "history-button");
        await d.waitSel(d.tid("history-list"));
        await d.idle();
        t.eq(await d.eval(`[...document.querySelectorAll("button")].some((b) => /Restore/.test(b.textContent))`), false, "no Restore for the reader");
        // The dashboard of a dashboard grant opens.
        const o = await openAs(t, a, "reader", "d/desk-open?" + t.range);
        await waitPanelsReady(o, [1, 2, 3]);
        t.eq(await o.exists(o.tid("edit-toggle")), false, "no edit toggle on desk-open");
        // The server refuses what the page hides.
        const w = await a.call("reader", "POST", "api/dashboards", {
          body: { folder: "transit", message: "", document: seedDocument("reader-new", "Reader new") },
          headers: { "Idempotency-Key": crypto.randomUUID() },
        });
        t.eq([w.status, w.json.error.code], [403, "forbidden"], "a create by the reader is refused");
        t.note("2 reader: list filtered, no controls, history without Restore, create refused by the server");
      }

      // ---- 3. A refused read: the 403 page. An unknown uid gives the same page to a subject without the right.
      {
        for (const uid of ["hidden-one", "desk-closed", "no-such-dashboard"]) {
          const p = await openAs(t, a, "reader", `d/${uid}?` + t.range);
          t.allow(p, /Failed to load resource/);
          await p.waitSel(p.tid("dashboard-forbidden"));
          t.eq((await p.text(p.tid("dashboard-forbidden"))).includes("You have no access to this dashboard."), true, `403 page for ${uid}`);
          t.eq(await p.exists(p.tid("panel-1")), false, `no panel for ${uid}`);
          t.eq(await p.exists(p.tid("edit-toggle")), false, `no edit toggle for ${uid}`);
        }
        const adminUnknown = await openAs(t, a, "admin", "d/no-such-dashboard?" + t.range);
        t.allow(adminUnknown, /Failed to load resource/);
        await adminUnknown.waitSel(adminUnknown.tid("dashboard-not-found"));
        t.eq((await adminUnknown.text(adminUnknown.tid("dashboard-not-found"))).includes("This dashboard does not exist."), true, "admin sees 404 for an unknown uid");
        const refused = await a.call("reader", "POST", "api/dashboards/hidden-one/panels/1/query", {
          body: { etag: "0".repeat(64), from_ms: 1794211200000, to_ms: 1794212400000, max_data_points: 100, variables: {} },
        });
        t.eq([refused.status, refused.json.error.code], [403, "forbidden"], "a query on a hidden dashboard is refused");
        // A subject with no grant: an empty list with its own text.
        const n = await openAs(t, a, "nobody", "");
        t.eq(await rowIds(n), [], "nobody has no row");
        t.assert((await n.eval("document.body.textContent")).includes("No dashboard is visible to you."), "empty list text of the mode authzen");
        t.eq(await toolbar(n), [], "nobody has no toolbar");
        t.note("3 refused read: 403 page for hidden, closed and unknown uid; admin sees 404; nobody sees the empty text");
      }

      // ---- 4. A writer without source.query: the edit mode opens, "Run" is absent, and the save is refused.
      {
        const p = await openAs(t, a, "author", "d/nodes?" + t.range);
        await waitPanelsReady(p, [1, 2, 3]);
        await enterEdit(p);
        await openPanelEditor(p, 2);
        t.eq(await p.eval(`document.querySelector('[data-testid="run-button"]').hidden`), true, "Run is hidden without source.query");
        t.eq(await p.eval(`getComputedStyle(document.querySelector('[data-testid="run-button"]')).display`), "none", "Run is not displayed");
        const options = await p.eval(`[...document.querySelectorAll('[data-testid="panel-editor"] select option')].map((o) => o.textContent).filter((x) => /prod/.test(x))`);
        t.assert(options.some((x) => /not available to you/.test(x)), "the source shows as not available: " + JSON.stringify(options));
        await p.click(p.tid("editor-back"));
        await p.waitGone(p.tid("panel-editor"));
        // No source to a new panel.
        await moreItem(p, "add-panel");
        await waitNotice(p, /No source is available to you\./);
        // The save of SQL on a source without source.query is refused: the draft stays.
        t.allow(p, /Failed to load resource/);
        await setPanelTitle(p, 2, "Author title");
        await waitDraft(p, "nodes", (d) => d.document.sections[1].panels[0].title === "Author title");
        await saveDialog(p, "author save");
        await waitNotice(p, /A save needs the query right on the source prod\./);
        const notices = await noticeTexts(p);
        t.note("4 author: Run hidden, source marked unavailable, save refused with the notice " + JSON.stringify(notices));
        const after = await a.call("admin", "GET", "api/dashboards/nodes");
        t.eq(after.json.version, 0, "the refused save wrote no version");
        t.eq(after.json.document.sections[1].panels[0].title === "Author title", false, "the refused save changed nothing");
      }

      // ---- 5. sqlauthor: "Run" is present and it works.
      {
        const p = await openAs(t, a, "sqlauthor", "d/nodes?" + t.range);
        await waitPanelsReady(p, [1, 2, 3]);
        await enterEdit(p);
        await openPanelEditor(p, 2);
        t.eq(await p.eval(`document.querySelector('[data-testid="run-button"]').hidden`), false, "Run shows with source.query");
        await p.fill(p.tid("sql-input"), "SELECT node, blocks\nFROM analytics.seed\nWHERE blocks < 3");
        await sleep(450);
        p.clearRequests();
        await p.click(p.tid("run-button"));
        await p.waitFor(`document.querySelectorAll('[data-testid="table-2"] tbody tr').length === 3`, { what: "3 rows after Run" });
        const pv = p.requests.filter((r) => r.method === "POST" && /\/api\/sources\/prod\/preview$/.test(r.url));
        t.eq(pv.length, 1, "one preview request");
        t.eq(pv[0].status, 200, "the preview is permitted");
        t.note("5 sqlauthor: Run shows and the preview answers 200");
      }

      // ---- 6. A missing, an expired and a bad token: the 401 page, and the Reload button after a good token.
      // The token is read from each request. The dashboard page is loaded, so its modules are in memory:
      // the next request is an API call (the refresh), which gets the 401 (9.6, step 3).
      {
        const p = await openAs(t, a, "reader", "d/nodes?" + t.range);
        await waitPanelsReady(p, [1, 2, 3]);
        t.allow(p, /Failed to load resource/);
        for (const [what, headers] of [
          ["no token", {}],
          ["expired token", a.headers("reader", { ageSeconds: 700 })],
          ["wrong audience", a.headers("reader", { aud: "another-service" })],
          ["garbage", { [HEADER]: "not.a.token" }],
        ]) {
          await p.setHeaders(headers);
          await p.click(p.tid("refresh-now"));
          await p.waitSel(p.tid("unauthorized"));
          await p.idle();
          t.eq((await p.text(p.tid("unauthorized"))).includes("You are not signed in, or your session ended. Reload the page."), true, `401 page for ${what}`);
          t.eq(await p.exists(p.tid("panel-1")), false, `no dashboard under the 401 page (${what})`);
          t.eq(await p.exists('[data-testid="unauthorized"] button'), true, "a Reload button");
          t.eq((await noticeTexts(p)).some((x) => PDP_TEXT.test(x)), false, "no PDP notice for a 401");
          // The way back: a good token and a click on Reload gives the dashboard again.
          await p.setHeaders(a.headers("reader"));
          await p.click('[data-testid="unauthorized"] button');
          await p.waitSel(p.tid("panel-1"));
          await waitPanelsReady(p, [1, 2, 3]);
        }
        // A request for a page module with a bad token is a static file that gets 401: the page shows the
        // step 4 page, since the 401 of a module is not an API answer.
        const l = await openAs(t, a, "reader", "");
        t.allow(l, /Failed to load resource|dynamically imported|console\.error/);
        await l.setHeaders({});
        await l.click(`a[href*="d/nodes"]`);
        await l.waitFor(`!!document.querySelector('[data-testid="unauthorized"], [data-testid="no-answer"]')`, { what: "a full page" });
        t.note("6 token: 401 page for no token, expired token, wrong audience and garbage (API call); Reload with a good token shows the dashboard. A 401 on a lazy module gives: " + JSON.stringify((await screenText(l)).slice(0, 80)));
      }

      // ---- 7. The PDP down: the notice of 9.6, the pages and the session still answer, the readiness stays 200.
      {
        const p = await openAs(t, a, "reader", "");
        t.eq((await rowIds(p)).includes("nodes"), true, "the list loaded");
        await a.pdp.stop();
        t.allow(p, /Failed to load resource|console\.error/);
        // A click on a dashboard asks the PDP: 503 pdp_unavailable.
        await p.click(`a[href*="d/nodes"]`);
        await waitNotice(p, PDP_TEXT);
        t.eq((await noticeTexts(p)).filter((x) => PDP_TEXT.test(x)).length, 1, "one PDP notice");
        await p.waitFor(`!!document.querySelector('[data-testid="dashboard-error"], [data-testid="no-answer"]')`, { what: "a full page for the failed load" });
        const pageText = await screenText(p);
        t.note("7 PDP down: full page text " + JSON.stringify(pageText.slice(0, 120)));
        t.eq(await p.exists(p.tid("panel-1")), false, "no panel is shown after a failed decision");
        // Nothing but the action document, the shell, the session and /readyz answer.
        const hdr = { [HEADER]: a.token("reader") };
        t.eq((await fetch(server.url, { headers: hdr })).status, 200, "the shell still answers");
        t.eq((await fetch(server.url + "api/session", { headers: hdr })).status, 200, "the session still answers");
        t.eq((await fetch(server.opsOrigin + "/readyz")).status, 200, "/readyz stays 200");
        t.eq((await fetch(server.origin + "/.well-known/procurauth/actions")).status, 200, "the action document still answers");
        const down = await fetch(server.url + "api/dashboards/nodes", { headers: hdr });
        const downBody = await down.json();
        t.eq([down.status, downBody.error.code, downBody.error.retryable], [503, "pdp_unavailable", true], "the API answer");
        t.assert(!/reason|budget/.test(JSON.stringify(downBody)), "the body has no PDP reason");
        // A new load of the list during the outage: shell and session pass, the list fails with the notice.
        const q = await openAs(t, a, "reader", "", { wait: "none" });
        t.allow(q, /Failed to load resource|console\.error|ApiError/);
        await q.waitSel(q.tid("no-answer"));
        await waitNotice(q, PDP_TEXT);
        t.eq((await q.text(q.tid("no-answer"))).includes("Hune does not answer."), true, "the list page shows the full page and its Retry button");
        // The PDP is back: Retry shows the list.
        await a.pdp.start();
        await q.click(`${q.tid("no-answer")} button`);
        await q.waitFor(`!!document.querySelector('[data-testid="row-nodes"]')`, { what: "the list after the PDP is back" });
        await q.idle();
        t.eq((await rowIds(q)).includes("nodes"), true, "the list shows after Retry");
      }

      // ---- 8. The PDP fails while a dashboard shows: the refresh gives panel errors and the notice.
      {
        const p = await openAs(t, a, "reader", "d/nodes?" + t.range);
        await waitPanelsReady(p, [1, 2, 3]);
        t.allow(p, /Failed to load resource|console\.error/);
        await a.pdp.admin("POST", "/__fake/failures", { failures: [{ kind: "error_503" }] });
        await p.click(p.tid("refresh-now"));
        await waitPanelsReady(p, [1, 2, 3], "error");
        await waitNotice(p, PDP_TEXT);
        const states = await panelStates(p);
        t.assert(Object.values(states).every((x) => x === "error"), "each panel shows its error: " + JSON.stringify(states));
        const code = await p.text('[data-testid="panel-1-error"] code');
        t.eq(code, "pdp_unavailable", "the panel error code");
        t.note("8 PDP 503 during a refresh: panel states " + JSON.stringify(states));
        await a.pdp.admin("POST", "/__fake/failures", { failures: [] });
      }

      // ---- 9. The move conflict screen (10.10), and the save that the PDP refuses after a move.
      {
        const admin = a.call.bind(a, "admin");
        const create = async (uid) => {
          const r = await admin("POST", "api/dashboards", { body: { folder: "transit", message: "", document: seedDocument(uid, `Move ${uid}`) }, headers: { "Idempotency-Key": crypto.randomUUID() } });
          t.eq(r.status, 201, `create ${uid}: ${r.text.slice(0, 200)}`);
          return r.etag;
        };
        const moveTo = async (uid, folder, etag, sub = "admin") => {
          const r = await a.call(sub, "POST", `api/dashboards/${uid}/move`, { body: { folder, message: "" }, headers: { "If-Match": `"${etag}"`, "Idempotency-Key": crypto.randomUUID() } });
          t.eq(r.status, 200, `move ${uid}: ${r.text.slice(0, 200)}`);
        };
        // 9a. A move between the decision and the write is a race: the server decides on the chain that it reads
        // at the request, so a save after a plain move passes (the etag does not change, 6.19). The test makes the
        // race with an injected 409 on the first PUT (current_etag = the base etag, current_folder = lab). The
        // second PUT is real: the PDP decides on the new folder (dashboard.write on lab).
        {
          const uid = "mv-a";
          const etag = await create(uid);
          const A = await openAs(t, a, "sqlauthor", `d/${uid}?` + t.range);
          await waitPanelsReady(A, [1, 2, 3]);
          await enterEdit(A);
          await setPanelTitle(A, 2, "Mine after move");
          await waitDraft(A, uid, (d) => d.document.sections[1].panels[0].title === "Mine after move");
          await moveTo(uid, "lab", etag);
          t.allow(A, /Failed to load resource/);
          let injected = 0;
          const off = A.fulfillWhen((r) => r.method === "PUT" && /\/api\/dashboards\/mv-a$/.test(r.url) && injected++ === 0, {
            status: 409,
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ error: { code: "conflict", message: "The dashboard changed after you opened it.", retryable: false, current_etag: etag, current_version: 2, current_folder: "lab", saved_at: "2026-11-10T10:00:00.000Z", saved_by: "admin" } }),
          });
          await saveDialog(A, "after the move");
          await A.waitSel(A.tid("conflict-screen"));
          await A.idle();
          off();
          const text = await A.text(A.tid("conflict-screen"));
          t.assert(text.includes("The dashboard moved to the folder Lab after you opened it."), "move text: " + text.slice(0, 200));
          t.eq(await A.exists(A.tid("diff-view")), false, "no diff for a move");
          const buttons = await A.eval(`[...document.querySelectorAll('[data-testid="conflict-screen"] button')].map((b) => b.textContent.trim()).filter(Boolean)`);
          t.eq(buttons, ["Save in the new folder", "Cancel"], "the two choices of a move");
          await A.clickText('[data-testid="conflict-screen"] button', "Save in the new folder");
          await A.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null`, { what: "edit mode left after the save in the new folder" });
          const saved = await admin("GET", `api/dashboards/${uid}`);
          t.eq([saved.json.folder, saved.json.document.sections[1].panels[0].title], ["lab", "Mine after move"], "saved in the new folder with the draft");
          t.note("9a move conflict screen (injected 409): text, two choices, the real save in lab passed");
        }
        // 9b. The writer has no `dashboard.write` on the new folder: the PDP refuses the save, and the draft stays.
        {
          const uid = "mv-b";
          const etag = await create(uid);
          // `mover` may write in transit only: the policy has it from the start (lib/authzen.mjs).
          const B = await openAs(t, a, "mover", `d/${uid}?` + t.range);
          await waitPanelsReady(B, [1, 2, 3]);
          await enterEdit(B);
          await setPanelTitle(B, 2, "Mover title");
          await waitDraft(B, uid, (d) => d.document.sections[1].panels[0].title === "Mover title");
          await moveTo(uid, "lab", etag);
          t.allow(B, /Failed to load resource/);
          await saveDialog(B, "after the move");
          // The refusal comes from the PDP before the etag check: a 403 in the save dialog, no conflict screen.
          await B.waitFor(`(document.querySelector('[data-testid="save-dialog"] .save-status') || {}).textContent.length > 0 && !/Saving/.test(document.querySelector('[data-testid="save-dialog"] .save-status').textContent)`, { what: "the refusal in the save dialog" });
          const refusal = await B.text('[data-testid="save-dialog"] .save-status');
          t.eq(await B.exists(B.tid("conflict-screen")), false, "no conflict screen for a folder the writer cannot write");
          const put = B.requests.filter((r) => r.method === "PUT");
          t.eq(put.map((r) => r.status), [403], "the PUT got 403");
          t.assert(/right|permit|allowed|forbidden|access/i.test(refusal), "the dialog shows the refusal: " + refusal);
          await B.press("Escape");
          t.eq(await B.exists(B.tid("edit-bar")), true, "edit mode stays");
          t.assert((await waitDraft(B, uid)).document.sections[1].panels[0].title === "Mover title", "the draft stays");
          const stored = await admin("GET", `api/dashboards/${uid}`);
          t.eq([stored.json.folder, stored.json.document.sections[1].panels[0].title === "Mover title"], ["lab", false], "nothing saved");
          t.note("9b refused save after a move: dialog text " + JSON.stringify(refusal));
        }
      }
    },
  },
];
