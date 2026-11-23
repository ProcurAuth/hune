// Browser case B65 (16.10): the dashboard links (milestone F, wave 2: F19, 9.8a). A `link` with include_vars and keep_time has the URL with the
// variables and the time. A URL that the check of 4.23 refuses gives no link. A `dashboards` link lists the dashboards that have all its tags, without the
// current one, in the order of 9.8a. A dropdown opens a menu, and a click opens the dashboard with pushState. A title with markup shows as text.
import { doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const g = (x, y, w, h) => ({ x, y, w, h });
const one = (uid, title, tags) => doc(uid, title, [section(null, [panel(1, "table", "T", ["SELECT 1 AS x"], { grid: g(0, 0, 12, 4) })])], { tags });

export const cases = [
  {
    id: "B65",
    name: "dashboard links: URL with variables and time, a refused URL, a dashboards link with two tags, a dropdown, pushState, a title with markup",
    async run(t) {
      await ensure(t, one("dl-a", "Alpha", ["linkt1", "linkt2"]));
      await ensure(t, one("dl-b", "Beta", ["linkt1"]));
      await ensure(t, one("dl-c", "gamma", ["linkt1", "linkt2"]));
      await ensure(t, one("dl-d", "Delta", ["linkt2", "linkt1", "other"]));
      const variables = [
        { kind: "custom", name: "env", label: null, hide: "none", options: [{ text: "prod", value: "prod" }, { text: "staging", value: "staging" }], multi: false, include_all: false, all_value: null, default: ["prod"] },
        { kind: "custom", name: "bad", label: null, hide: "variable", options: [{ text: "x", value: "javascript:alert(1)" }], multi: false, include_all: false, all_value: null, default: ["javascript:alert(1)"] },
      ];
      const main = doc("dl-main", "Links main", [section(null, [panel(1, "table", "T", ["SELECT 1 AS x"], { grid: g(0, 0, 12, 4) })])], { tags: ["linkt1", "linkt2"], variables });
      main.links = [
        { kind: "link", title: "<b>x</b> Runbook", url: "https://wiki.example.test/runbook?env=${env}", target_blank: true, include_vars: true, keep_time: true },
        { kind: "link", title: "Refused", url: "${bad}", target_blank: false, include_vars: false, keep_time: false },
        { kind: "dashboards", title: "", tags: ["linkt1", "linkt2"], as_dropdown: false, target_blank: false, include_vars: true, keep_time: true },
        { kind: "dashboards", title: "Related", tags: ["linkt1", "linkt2"], as_dropdown: true, target_blank: false, include_vars: false, keep_time: true },
        { kind: "dashboards", title: "", tags: ["no-dashboard-has-this-tag"], as_dropdown: false, target_blank: false, include_vars: false, keep_time: false },
      ];
      await ensure(t, main);

      const p = await t.open(t.url("d/dl-main"));
      await waitPanelsReady(p, [1]);
      await p.waitSel(p.tid("dash-link-2-item-dl-a"));
      t.assert(await p.exists(p.tid("dash-links")), "the group of the links exists");
      const toolbar = await p.eval(`(() => { const b = document.querySelector('[data-testid="dashboard-toolbar"]'); return b ? getComputedStyle(b).display : null; })()`);
      t.assert(toolbar && toolbar !== "none", "the toolbar row shows with the links");

      // The kind `link`: the title is text, the URL has the variables and the time, the tab opens with rel noreferrer.
      const l0 = await p.eval(`(() => { const a = document.querySelector('[data-testid="dash-link-0"]'); return { href: a.getAttribute("href"), text: a.textContent, target: a.target, rel: a.rel, bold: document.querySelectorAll('.dash-links b').length }; })()`);
      t.eq(l0.href, "https://wiki.example.test/runbook?env=prod&from=now-6h&to=now&var-env=prod&var-bad=javascript%3Aalert(1)", "the URL of the link 0");
      t.eq(l0.text, "<b>x</b> Runbook", "a title with markup shows as text");
      t.eq(l0.bold, 0, "no element b in the links");
      t.eq([l0.target, l0.rel], ["_blank", "noreferrer"], "target_blank");

      // A refused URL: no link and no javascript: URL anywhere.
      t.assert(!(await p.exists(p.tid("dash-link-1"))), "the refused URL gives no link");
      t.eq(await p.count('a[href^="javascript"]'), 0, "no javascript: URL in the page");

      // The list: the dashboards that have both tags, without the current one, sorted by the lower-case title.
      const items = await p.eval(`[...document.querySelectorAll('[data-testid^="dash-link-2-item-"]')].map((a) => [a.dataset.testid.replace("dash-link-2-item-", ""), a.textContent, a.getAttribute("href")])`);
      t.eq(items.map((x) => x[0]), ["dl-a", "dl-d", "dl-c"], "Alpha, Delta, gamma");
      t.eq(items.map((x) => x[1]), ["Alpha", "Delta", "gamma"], "the titles");
      t.eq(items[0][2], "d/dl-a/alpha?from=now-6h&to=now&var-env=prod&var-bad=javascript%3Aalert(1)", "the URL of an item");
      t.assert(!(await p.exists(p.tid("dash-link-4"))), "a link with no dashboard shows nothing");

      // The request: one tag in the query, no repeated tag, limit 500.
      const lists = p.requests.filter((r) => r.method === "GET" && /\/api\/dashboards\?/.test(r.url));
      t.assert(lists.length >= 1, "the page read the list");
      for (const r of lists) {
        const u = new URL(r.url);
        t.assert(u.searchParams.getAll("tag").length <= 1, `at most one tag: ${r.url}`);
      }
      t.assert(lists.some((r) => new URL(r.url).searchParams.get("tag") === "linkt1" && new URL(r.url).searchParams.get("limit") === "500"), "the first tag and limit 500");
      t.assert(lists.filter((r) => new URL(r.url).searchParams.get("tag") === "linkt1").length === 1, "two links with the same first tag share one request");

      // The dropdown: a button, a menu with one item for each dashboard, and a click that opens the dashboard with pushState.
      t.eq((await p.text(p.tid("dash-link-3-button"))).trim(), "Related", "the dropdown button");
      await p.click(p.tid("dash-link-3-button"));
      await p.waitSel(p.tid("dash-link-3-item-dl-c"));
      const menu = await p.eval(`[...document.querySelectorAll('.menu [data-testid^="dash-link-3-item-"]')].map((a) => a.textContent)`);
      t.eq(menu, ["Alpha", "Delta", "gamma"], "the menu items");
      await p.eval(`window.__marker = "same page"`);
      await p.click(p.tid("dash-link-3-item-dl-c"));
      await p.waitFor(`location.pathname.endsWith("/d/dl-c/gamma")`, { what: "the dashboard dl-c opens" });
      await p.idle();
      t.eq(await p.eval("window.__marker"), "same page", "the page did not reload (pushState)");
      t.assert((await p.eval("location.search")).includes("from=now-6h"), "keep_time puts the time in the address");

      // A change of a variable changes the URLs of the links (on the first dashboard).
      await p.goto(t.url("d/dl-main"));
      await p.idle();
      await p.waitSel(p.tid("dash-link-2-item-dl-a"));
      await p.click(p.tid("variable-env-button"));
      await p.click('.listbox__item[data-value="staging"]');
      await p.idle();
      await p.waitFor(`document.querySelector('[data-testid="dash-link-0"]').getAttribute("href").includes("env=staging")`, { what: "the link follows the variable" });
      t.assert((await p.attr(p.tid("dash-link-2-item-dl-a"), "href")).includes("var-env=staging"), "an item follows the variable");
    },
  },
];
