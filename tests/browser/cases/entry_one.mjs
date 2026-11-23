// Browser case B214 (9.7, 11.25): the entry redirect in mode `authzen` (owner decision ID-K326 on F38: no kiosk mode).
// A ProcurAuth subject who can see one dashboard of several goes to it by the entry redirect of 9.7. The list of 9.7 is the
// list that the display filter of 11.25 gives, not the whole catalog. The policy and the volume are those of lib/authzen.mjs:
// six items in four folders; `solo` reads the dashboard desk-open only; `reader` reads transit and desk-open.
// Without the ProcurAuth tools the case reports skip, as B23 and B35 do.
import { sleep } from "../lib/h.mjs";

const GUARD_KEY = "hune.entry.redirect";
const ALL = ["desk-closed", "desk-open", "grid-errors", "hidden-one", "lab-a", "nodes"];

// A probe at the start of each document of the page. It counts the documents of the tab (sessionStorage), keeps the
// history length before main.js runs, and records each list row or empty state that enters the DOM, each value of the
// hook data-hune-view, and the path at the release of the held first paint (attribute data-hune-hold).
const PROBE = `(() => {
  let docs = 0;
  try { docs = Number(sessionStorage.getItem("b214.docs") || 0) + 1; sessionStorage.setItem("b214.docs", String(docs)); } catch (e) {}
  const rec = window.__b214 = { docs, len0: history.length, rows: 0, empty: 0, views: [], holdOff: null };
  const LIST = 'tr[data-testid^="row-"], [data-testid="list-empty"]';
  new MutationObserver((list) => {
    for (const m of list) {
      if (m.type === "attributes") {
        const v = m.target.getAttribute(m.attributeName);
        if (m.attributeName === "data-hune-view" && v) rec.views.push(v);
        if (m.attributeName === "data-hune-hold" && v === null && rec.holdOff === null) rec.holdOff = location.pathname;
        continue;
      }
      for (const n of m.addedNodes) {
        if (n.nodeType !== 1) continue;
        for (const e of [n, ...n.querySelectorAll(LIST)]) {
          if (!e.matches(LIST)) continue;
          if (e.dataset.testid === "list-empty") rec.empty++;
          else rec.rows++;
        }
      }
    }
  }).observe(document, { subtree: true, childList: true, attributes: true, attributeFilter: ["data-hune-view", "data-hune-hold"] });
})();`;

const place = (p) => p.eval("location.pathname + location.search");
const rows = (p) => p.eval(`[...document.querySelectorAll('[data-testid^="row-"]:not([data-testid^="row-menu-"])')].map((e) => e.dataset.testid.slice(4))`);
const guard = (p) => p.eval(`sessionStorage.getItem(${JSON.stringify(GUARD_KEY)})`);
const probe = (p) => p.eval("JSON.parse(JSON.stringify(window.__b214))");
const listShown = (p) => p.waitFor(`!!document.querySelector('[data-testid="list"] tr[data-testid^="row-"], [data-testid="list-empty"]')`, { what: "list content" });

/** The path stays this one for a while: no late redirect. */
async function staysAt(t, p, path, what) {
  await p.idle();
  await sleep(500);
  t.eq(await place(p), path, what);
}

/** The uids of `GET api/dashboards` for a subject (one page holds the six items). */
async function listed(t, a, sub) {
  const r = await a.call(sub, "GET", "api/dashboards?limit=500");
  t.eq(r.status, 200, `GET api/dashboards as ${sub}`);
  t.eq(r.json.next ?? null, null, `one page for ${sub}`);
  return r.json.items.map((i) => i.uid).sort();
}

export const cases = [
  {
    id: "B214",
    name: "entry redirect in mode authzen: the one dashboard of the display filter opens with replace and no list paint; the brand link and Go to the list show one row; two show the list",
    async run(t) {
      const a = await t.authzen();
      const open = async (sub, path = "") => {
        const p = await t.page();
        await p.send("Page.addScriptToEvaluateOnNewDocument", { source: PROBE });
        await p.setHeaders(a.headers(sub));
        await p.goto(a.server.url + path);
        return p;
      };

      // ---- 1. The list of 9.7 is the list of the display filter (11.25), not the catalog.
      t.eq(await listed(t, a, "admin"), ALL, "admin sees the six items of the catalog");
      t.eq(await listed(t, a, "solo"), ["desk-open"], "the display filter leaves one item to solo");

      // ---- 2. solo opens "/": the entry route redirects to desk-open, with replaceState, in the same document.
      const s = await open("solo");
      await s.waitFor(`location.pathname.startsWith("/d/desk-open")`, { what: "the redirect of solo" });
      await s.waitSel(s.tid("panel-1"));
      await s.idle();
      t.eq((await place(s)).startsWith("/d/desk-open/desk-open"), true, "redirect target with the slug: " + (await place(s)));
      const list = s.requests.filter((r) => r.method === "GET" && /\/api\/dashboards\?/.test(r.url));
      t.assert(list.length >= 1 && list.every((r) => r.status === 200), "the list route read GET api/dashboards: " + list.map((r) => r.url + " " + r.status).join(", "));
      const p1 = await probe(s);
      t.eq(p1.docs, 1, "one document load: the redirect is a navigation in the page, not a reload");
      t.eq(await s.eval("history.length"), p1.len0, "the redirect replaced the history entry (no push)");
      t.eq(p1.rows, 0, "no list row entered the DOM before the redirect");
      t.eq(p1.empty, 0, "no empty state of the list entered the DOM");
      t.eq(p1.views.includes("list"), false, "the hook data-hune-view never named the list: " + JSON.stringify(p1.views));
      t.note(`hold released at ${p1.holdOff}`);
      t.assert((await guard(s) || "").includes('"desk-open"'), "the guard names the uid");

      // ---- 3. The brand link shows the list with its one row and does not redirect again. The guard is removed first,
      // so only the rule "a navigation in the page never causes a redirect" keeps the list.
      await s.eval(`sessionStorage.removeItem(${JSON.stringify(GUARD_KEY)}); window.__marker = 1`);
      await s.click("a.brand");
      await s.waitSel(s.tid("list"));
      await listShown(s);
      t.eq(await rows(s), ["desk-open"], "the brand link shows the list with one row");
      await staysAt(t, s, "/", "no second redirect after the brand link");
      t.eq(await guard(s), null, "no guard written: no redirect");
      t.eq(await s.eval("window.__marker"), 1, "the brand link did not reload the document");
      t.eq((await probe(s)).views.includes("list"), true, "the list view shows");

      // ---- 4. "Go to the list" from a refused dashboard (a new tab with no guard): the list shows one row, no redirect.
      const g = await open("solo", "d/desk-closed");
      t.allow(g, /Failed to load resource/);
      await g.waitSel(g.tid("dashboard-forbidden"));
      await g.idle();
      await g.click(`${g.tid("dashboard-forbidden")} a`);
      await g.waitSel(g.tid("list"));
      await listShown(g);
      t.eq(await rows(g), ["desk-open"], "Go to the list shows the list with one row");
      await staysAt(t, g, "/", "no redirect after Go to the list");
      t.eq(await guard(g), null, "no guard in this tab");

      // ---- 5. A subject who sees two dashboards gets the list. The admin removes the item that is not valid, so the
      // display filter leaves exactly desk-open and nodes to reader.
      const item = (await a.call("admin", "GET", "api/dashboards?limit=500")).json.items.find((i) => i.uid === "grid-errors");
      const del = await a.call("admin", "DELETE", "api/dashboards/grid-errors", { headers: { "If-Match": `"${item.etag}"`, "Idempotency-Key": crypto.randomUUID() } });
      t.eq(del.status, 200, "admin deletes grid-errors: " + del.text.slice(0, 200));
      t.eq(await listed(t, a, "reader"), ["desk-open", "nodes"], "the display filter leaves two items to reader");
      const r = await open("reader");
      await r.idle();
      await listShown(r);
      t.eq((await rows(r)).sort(), ["desk-open", "nodes"], "reader sees the list with two rows");
      await staysAt(t, r, "/", "no redirect for two dashboards");
      t.eq(await guard(r), null, "no guard written for reader");
      const p2 = await probe(r);
      t.assert(p2.rows >= 2, "the list painted its rows: " + p2.rows);
      t.eq(p2.views.includes("list"), true, "the list view shows for reader");
    },
  },
];
