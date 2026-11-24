// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B28 to B35 (9.7): the entry redirect of the list page.
// One dashboard that the user can see opens at once, on the entry route of a page load only.
// Each case starts a server of its own, with a volume that holds exactly the dashboards that it needs.
import { cpSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ROOT, api, seedDocument } from "../lib/env.mjs";
import { sleep } from "../lib/h.mjs";

const GUARD_KEY = "hune.entry.redirect";

/** A volume `prepare` function: it removes every folder and writes `{ folder: { title, docs: [[uid, title]] } }`. */
function layout(spec) {
  return (volume) => {
    for (const name of readdirSync(volume)) rmSync(join(volume, name), { recursive: true, force: true });
    for (const [folder, { title, docs }] of Object.entries(spec)) {
      mkdirSync(join(volume, folder), { recursive: true });
      writeFileSync(join(volume, folder, "_folder.json"), JSON.stringify({ format_version: 1, title }, null, 2) + "\n");
      for (const [uid, docTitle] of docs) writeFileSync(join(volume, folder, `${uid}.json`), JSON.stringify(seedDocument(uid, docTitle), null, 2) + "\n");
    }
  };
}

const ONE = layout({ transit: { title: "Transit desk", docs: [["nodes", "Nodes"]] } });
const place = (p) => p.eval("location.pathname + location.search");
const rows = (p) => p.eval(`[...document.querySelectorAll('[data-testid^="row-"]:not([data-testid^="row-menu-"])')].map((e) => e.dataset.testid.slice(4))`);
const emptyText = (p) => p.eval(`(document.querySelector('[data-testid="list-empty"]') || {}).textContent || ""`);

/** The list page showed its content: a row or the empty state. */
const listShown = (p) => p.waitFor(`!!document.querySelector('[data-testid="list"] article[data-testid^="row-"], [data-testid="list-empty"]')`, { what: "list content" });

/** The dashboard page showed its grid. The redirect has no time range in the URL, so the panels need not have data. */
async function dashboardShown(p) {
  await p.waitSel(p.tid("panel-1"));
  await p.idle();
}

/** The path stays this one for a while: no late redirect. */
async function staysAt(t, p, path, what) {
  await p.idle();
  await sleep(500);
  t.eq(await place(p), path, what);
}

export const cases = [
  {
    id: "B28",
    name: "entry redirect: no dashboard shows the empty state of the list",
    async run(t) {
      const own = await t.ownServer("zero", { prepare: layout({ transit: { title: "Transit desk", docs: [] } }) });
      const p = await t.open(own.url);
      await listShown(p);
      t.eq(await place(p), "/", "the URL stays at the list");
      t.eq(await rows(p), [], "no row");
      t.assert((await emptyText(p)).includes("No dashboard yet."), "empty text: " + (await emptyText(p)));
      await staysAt(t, p, "/", "no redirect");
    },
  },
  {
    id: "B29",
    name: "entry redirect: one dashboard opens at once with replaceState; Back and the brand link do not loop",
    async run(t) {
      const own = await t.ownServer("one", { prepare: ONE });
      const dashPath = "/d/nodes/nodes";

      // (a) The entry route opens the dashboard; the URL is replaced, not pushed.
      const p = await t.page();
      await p.goto(own.origin + dashPath + "?" + t.range); // an earlier entry in the history of the tab
      await p.idle();
      const before = await p.eval("history.length");
      await p.goto(own.url);
      await p.waitFor(`location.pathname.startsWith("/d/nodes")`, { what: "the redirect to the dashboard" });
      await dashboardShown(p);
      t.eq((await place(p)).startsWith(dashPath), true, "redirect target with the slug: " + (await place(p)));
      t.eq(await p.eval("history.length"), before + 1, "the load added one entry and the redirect replaced it (no push)");
      t.assert((await p.eval(`sessionStorage.getItem(${JSON.stringify(GUARD_KEY)})`)).includes('"nodes"'), "the guard names the uid");

      // (b) Back goes to the earlier entry, not to the list: no loop.
      await p.eval("window.__oldDocument = true; history.back()");
      await p.waitFor(`window.__oldDocument === undefined && location.pathname.startsWith("/d/nodes") && !!document.querySelector('[data-testid="panel-1"]')`, { what: "Back to the earlier entry (a new document)" });
      await p.idle();
      t.eq(await p.exists(p.tid("list")), false, "Back does not show the list");

      // (c) The brand link is an in-app navigation: the list shows with its one row and does not redirect.
      await p.click("a.brand");
      await p.waitSel(p.tid("list"));
      await listShown(p);
      t.eq(await rows(p), ["nodes"], "the list shows its one row");
      await staysAt(t, p, "/", "the brand link shows the list");

      // (d) Back and Forward are in-app too: Forward shows the list again, with no redirect.
      await p.eval("window.__marker = 1");
      await p.eval("history.back()");
      await p.waitFor(`location.pathname.startsWith("/d/nodes")`, { what: "Back to the dashboard" });
      await dashboardShown(p);
      await p.eval("history.forward()");
      await p.waitFor(`location.pathname === "/"`, { what: "Forward to the list" });
      await listShown(p);
      await staysAt(t, p, "/", "Forward shows the list");
      t.eq(await rows(p), ["nodes"], "one row after Forward");
      t.eq(await p.eval("window.__marker"), 1, "Back and Forward did not reload the document");
    },
  },
  {
    id: "B30",
    name: "entry redirect: one dashboard inside a folder opens at once",
    async run(t) {
      const own = await t.ownServer("folder", {
        prepare: layout({ ops: { title: "Operations", docs: [["inner", "Inner board"]] }, empty: { title: "Empty folder", docs: [] } }),
      });
      const p = await t.open(own.url);
      await p.waitFor(`location.pathname.startsWith("/d/inner")`, { what: "the redirect to the dashboard of the folder" });
      await dashboardShown(p);
      t.eq((await place(p)).startsWith("/d/inner/inner-board"), true, "path with the slug: " + (await place(p)));
      await p.click("a.brand");
      await p.waitSel(p.tid("list"));
      await listShown(p);
      t.eq(await rows(p), ["inner"], "the list shows the dashboard in its folder");
    },
  },
  {
    id: "B31",
    name: "entry redirect: two dashboards show the list and do not redirect",
    async run(t) {
      const own = await t.ownServer("two", { prepare: layout({ transit: { title: "Transit desk", docs: [["nodes", "Nodes"], ["second", "Second"]] } }) });
      const p = await t.open(own.url);
      await listShown(p);
      t.eq((await rows(p)).sort(), ["nodes", "second"], "two rows");
      await staysAt(t, p, "/", "no redirect");
      t.eq(await p.eval(`sessionStorage.getItem(${JSON.stringify(GUARD_KEY)})`), null, "no guard written");
    },
  },
  {
    id: "B32",
    name: "entry redirect: access lost between two loads (403) gives the error page, then the list, with no loop",
    async run(t) {
      const own = await t.ownServer("lost", { prepare: ONE });
      const oapi = api(own);
      const p = await t.page();
      t.allow(p, /Failed to load resource/);
      const refuse = p.fulfillWhen((r) => r.method === "GET" && /\/api\/dashboards\/nodes(\?|$)/.test(r.url), {
        status: 403,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ error: { code: "forbidden", message: "No access.", retryable: false } }),
      });
      await p.goto(own.url);
      await p.waitSel(p.tid("dashboard-forbidden"));
      await p.idle();
      t.eq((await place(p)).startsWith("/d/nodes"), true, "the redirect happened once");
      t.eq(await p.exists(p.tid("list")), false, "no list on the error page");

      // "Go to the list" is an in-app navigation: the list shows its row and does not redirect again.
      await p.click(`${p.tid("dashboard-forbidden")} a`);
      await p.waitSel(p.tid("list"));
      await listShown(p);
      t.eq(await rows(p), ["nodes"], "the list shows the row");
      await staysAt(t, p, "/", "no second redirect after the link");

      // A full reload inside the window of the guard does not redirect either.
      await p.reload();
      await p.idle();
      await listShown(p);
      t.eq(await rows(p), ["nodes"], "the row after the reload");
      await staysAt(t, p, "/", "the guard stops the second automatic redirect to the same uid");
      refuse();

      // The dashboard is really gone: the list shows the empty state.
      const got = await oapi.dashboard("nodes");
      t.assert([200, 204].includes((await oapi.del("nodes", got.etag)).status), "delete");
      await p.reload();
      await p.idle();
      await listShown(p);
      t.assert((await emptyText(p)).includes("No dashboard yet."), "empty text after the delete");
      await staysAt(t, p, "/", "no redirect with no dashboard");
    },
  },
  {
    id: "B33",
    name: "entry redirect: one item that is not valid never redirects",
    async run(t) {
      const own = await t.ownServer("invalid", {
        prepare: (volume) => {
          layout({ transit: { title: "Transit desk", docs: [] } })(volume);
          cpSync(join(ROOT, "tests/fixtures/dashboards/invalid/grid_w_and_extra_member.json"), join(volume, "transit", "broken.json"));
        },
      });
      const p = await t.open(own.url);
      await listShown(p);
      t.eq(await rows(p), ["broken"], "one row");
      t.eq(await p.attr(p.tid("row-broken"), "data-state"), "invalid", "the row is invalid");
      await staysAt(t, p, "/", "no redirect");
    },
  },
  {
    id: "B34",
    name: "entry redirect: a tag filter q in the URL shows the list; an empty q does not stop the redirect",
    async run(t) {
      // The filters show (threshold 0): q is a tag name. The dashboard has the tag `chain`.
      const own = await t.ownServer("query", { prepare: ONE, serverLines: ["list_filters_min_dashboards = 0"] });
      const p = await t.open(own.url + "?q=chain");
      await listShown(p);
      t.eq(await rows(p), ["nodes"], "the tag filter keeps the one card");
      await staysAt(t, p, "/?q=chain", "q=chain: no redirect");
      const none = await t.open(own.url + "?q=zzz");
      await none.waitSel(none.tid("list-no-match"));
      await staysAt(t, none, "/?q=zzz", "q=zzz: no redirect");
      t.eq(await rows(none), [], "no card for q=zzz");
      const empty = await t.open(own.url + "?q=");
      await empty.waitFor(`location.pathname.startsWith("/d/nodes")`, { what: "redirect with an empty q" });
      // The default threshold hides the filters: q has no effect on the cards, and it still stops the redirect.
      const hid = await t.ownServer("query-hidden", { prepare: ONE });
      const h = await t.open(hid.url + "?q=zzz");
      await listShown(h);
      t.eq(await rows(h), ["nodes"], "the filters are hidden: q does not hide the card");
      await staysAt(t, h, "/?q=zzz", "q=zzz with the filters hidden: no redirect");
    },
  },
  {
    id: "B35",
    name: "entry redirect in mode authzen: one visible dashboard opens, none shows the text of the mode, several show the list",
    async run(t) {
      const a = await t.authzen();
      const open = async (sub, path = "") => {
        const p = await t.page();
        await p.setHeaders(a.headers(sub));
        await p.goto(a.server.url + path);
        await p.idle();
        return p;
      };
      // solo: the display filter leaves one dashboard.
      const s = await open("solo");
      await s.waitFor(`location.pathname.startsWith("/d/desk-open")`, { what: "redirect of solo" });
      await dashboardShown(s);
      t.eq((await place(s)).startsWith("/d/desk-open/desk-open"), true, "solo path: " + (await place(s)));
      await s.click("a.brand");
      await s.waitSel(s.tid("list"));
      await listShown(s);
      t.eq(await rows(s), ["desk-open"], "solo sees one row in the list");
      await staysAt(t, s, "/", "the brand link shows the list");
      // nobody: the empty state of the mode.
      const n = await open("nobody");
      await listShown(n);
      t.assert((await emptyText(n)).includes("No dashboard is visible to you."), "text of the mode authzen");
      await staysAt(t, n, "/", "no redirect for nobody");
      // reader: several dashboards.
      const r = await open("reader");
      await listShown(r);
      t.eq((await rows(r)).length > 1, true, "reader sees several rows");
      await staysAt(t, r, "/", "no redirect for reader");
    },
  },
];
