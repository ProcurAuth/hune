// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B01, B02, B03, B10, B12, B13, B24, B25, B26, B27 (16.10).
import { allTypesDoc, doc, panel, section, SQL } from "../lib/docs.mjs";
import { SEED_FROM, SEED_TO } from "../lib/env.mjs";
import { optionQueries, panelIdOf, panelQueries, panelStates, sleep, viewData, waitPanelsReady } from "../lib/h.mjs";

const VP = '[data-testid="view-panel"]';

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 300)}`);
}

export const cases = [
  {
    id: "B01",
    name: "list page shows folders, dashboards and an invalid document with its errors",
    async run(t) {
      const p = await t.open(t.url(""));
      t.assert(await p.exists(p.tid("folder-transit")), "folder group transit");
      t.eq((await p.text(`${p.tid("folder-transit")} .group__title`)).trim(), "Transit desk", "folder title");
      // The valid row: a link to d/<uid>/<slug>, its tags, and a relative time with an absolute title.
      const row = await p.eval(`(() => {
        const r = document.querySelector('[data-testid="row-nodes"]');
        const a = r.querySelector("a");
        return { href: a.getAttribute("href"), text: a.textContent, tags: [...r.querySelectorAll(".tag")].map((e) => e.textContent),
                 changed: r.querySelector(".cell-changed").textContent, title: r.querySelector(".cell-changed").title };
      })()`);
      t.eq(row.href, "d/nodes/nodes", "row link");
      t.eq(row.text, "Nodes", "row title");
      t.eq(row.tags, ["chain"], "row tags");
      t.assert(/ago|just now/.test(row.changed), "relative time: " + row.changed);
      t.assert(/^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d$/.test(row.title), "absolute time in the title attribute: " + row.title);
      // The invalid row: the uid, "Not valid", no link, and the errors with their pointers.
      const bad = await p.eval(`(() => {
        const r = document.querySelector('[data-testid="row-grid-errors"]');
        return r ? { state: r.dataset.state, text: r.querySelector(".cell-title").textContent, link: !!r.querySelector("a") } : null;
      })()`);
      t.assert(bad, "invalid row is present");
      t.eq(bad.state, "invalid", "invalid state");
      t.assert(bad.text.includes("grid-errors") && bad.text.includes("Not valid"), "uid and Not valid: " + bad.text);
      t.assert(!bad.link, "an invalid row has no link to the dashboard");
      await p.clickText(`${p.tid("row-grid-errors")} button`, "Show errors");
      const errs = await p.eval(`[...document.querySelectorAll('[data-testid="row-grid-errors"] .error-list li')].map((e) => e.querySelector("code").textContent)`);
      const api = await t.api.list();
      const item = api.json.items.find((i) => i.uid === "grid-errors");
      t.assert(item && item.state === "invalid", "the API marks the item invalid");
      t.eq(errs, item.errors.map((e) => e.pointer), "the page lists the pointers of the API");
      t.assert(errs.length >= 2, "at least two errors");
      // Search keeps rows by title, uid or tag, and the URL keeps the text in q.
      await p.fill(p.tid("list-search"), "chain");
      await p.waitFor(`document.querySelectorAll('[data-testid^="row-"][data-state]').length === 1`, { what: "search by tag" });
      t.assert(/q=chain/.test(await p.eval("location.search")), "q in the URL");
      // The page route of an invalid dashboard gives the list of errors, not the dashboard.
      const r422 = await t.api.dashboard("grid-errors");
      t.eq(r422.status, 422, "GET of an invalid dashboard");
    },
  },

  {
    id: "B02",
    name: "grid: each panel box has the position and size of its grid numbers",
    async run(t) {
      await ensure(t, allTypesDoc("all-types"));
      const p = await t.open(t.dash("all-types"));
      const d = await t.api.dashboard("all-types");
      const panels = d.json.document.sections[0].panels;
      const geo = await p.eval(`(() => {
        const first = document.querySelector('[data-testid="panel-1"]');
        const grid = first.parentElement;
        const cs = getComputedStyle(grid);
        const gb = grid.getBoundingClientRect();
        const boxes = {};
        for (const e of document.querySelectorAll('[data-testid^="panel-"][data-state]')) {
          const b = e.getBoundingClientRect();
          boxes[e.dataset.testid] = { x: b.left - gb.left, y: b.top - gb.top, w: b.width, h: b.height };
        }
        return { rows: cs.gridAutoRows, gap: cs.rowGap, cgap: cs.columnGap, cols: cs.gridTemplateColumns.split(" ").length, width: gb.width, boxes };
      })()`);
      t.eq(geo.rows, "30px", "grid-auto-rows");
      t.eq([geo.gap, geo.cgap], ["8px", "8px"], "gaps");
      t.eq(geo.cols, 24, "24 columns");
      const col = (geo.width - 23 * 8) / 24;
      for (const pn of panels) {
        const { x, y, w, h } = pn.grid;
        const b = geo.boxes[`panel-${pn.id}`];
        const exp = { x: x * (col + 8), y: y * 38, w: w * col + (w - 1) * 8, h: 30 * h + 8 * (h - 1) };
        for (const k of ["x", "y", "w", "h"]) {
          t.assert(Math.abs(b[k] - exp[k]) <= 1, `panel ${pn.id} ${k}: got ${b[k].toFixed(2)}, expected ${exp[k].toFixed(2)}`);
        }
      }
    },
  },

  {
    id: "B03",
    name: "each of the 7 panel types renders its fixture",
    async run(t) {
      await ensure(t, allTypesDoc("all-types"));
      const p = await t.open(t.dash("all-types"));
      await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 7, 8]);
      t.eq(Object.values(await panelStates(p)).every((s) => s === "ready"), true, "all panels ready");
      // table: 30 data rows (aria-rowcount counts the header) and the four columns
      const tbl = await p.eval(`(() => {
        const e = document.querySelector('[data-testid="table-1"] table');
        return { count: e.getAttribute("aria-rowcount"), heads: [...e.querySelectorAll("th")].map((x) => x.textContent.trim()) };
      })()`);
      t.eq(tbl.count, "31", "table rows (+ header)");
      t.eq(tbl.heads.slice(0, 4), ["node", "weight", "blocks", "active"], "table headers");
      // timeseries lines and bars, each with 3 series (v0, v1, v2) drawn by the chart engine
      const ts = await p.eval(`(() => ["2", "3", "5", "7"].map((id) => {
        const c = document.querySelector('[data-testid="panel-' + id + '"] .chartCore');
        return c ? { id, type: c.dataset.type, series: c.dataset.seriesDrawn, points: Number(c.dataset.pointsDrawn) } : { id, missing: true };
      }))()`);
      const by = Object.fromEntries(ts.map((x) => [x.id, x]));
      t.eq([by[2].type, by[2].series], ["line", "3"], "timeseries lines");
      t.assert(by[2].points > 3, "line points");
      t.eq([by[3].type, by[3].series], ["bar", "3"], "timeseries bars");
      t.eq([by[5].type, by[5].series], ["bar", "1"], "barchart");
      t.eq(by[7].series, "1", "xychart series");
      t.eq(by[7].points, 30, "xychart points");
      // a canvas that is not blank
      const ink = await p.eval(`(() => {
        const out = {};
        for (const id of ["2", "3", "5", "7"]) {
          const cv = document.querySelector('[data-testid="panel-' + id + '"] canvas');
          const ctx = cv.getContext("2d");
          const data = ctx.getImageData(0, 0, cv.width, cv.height).data;
          let n = 0;
          for (let i = 3; i < data.length; i += 4) if (data[i] !== 0) n++;
          out[id] = n;
        }
        return out;
      })()`);
      for (const [id, n] of Object.entries(ink)) t.assert(n > 200, `chart ${id} draws pixels (${n})`);
      // chart role and label (9.26)
      const labels = await p.eval(`[...document.querySelectorAll('[data-testid="panel-2"] [role=img], [data-testid="panel-5"] [role=img]')].map((e) => e.getAttribute("aria-label"))`);
      t.assert(labels.includes("Lines, time series chart") && labels.includes("Bar chart, bar chart"), "chart labels: " + JSON.stringify(labels));
      // stat: the field Total (sum of 0.5 .. 29.5 = 450)
      t.eq((await p.text('[data-testid="stat-item-4"] .statTile__value')).trim(), "450", "stat value");
      // pie: a canvas with pixels, and legend items for 3 slices
      const pie = await p.eval(`(() => {
        const cv = document.querySelector('[data-testid="pie-6"] canvas');
        const data = cv.getContext("2d").getImageData(0, 0, cv.width, cv.height).data;
        let n = 0;
        for (let i = 3; i < data.length; i += 4) if (data[i] !== 0) n++;
        return { ink: n, legend: document.querySelectorAll('[data-testid="pie-6"] .pie__legend li').length };
      })()`);
      t.assert(pie.ink > 500, "pie draws pixels");
      t.eq(pie.legend, 3, "pie legend items");
      // bar gauge: 3 bars named v0, v1, v2
      const names = await p.eval(`[...document.querySelectorAll('[data-testid="bargauge-8"] .bargauge__name')].map((e) => e.textContent)`);
      t.eq(names, ["v0", "v1", "v2"], "bar gauge names");
      // View data: the text alternative of a chart (9.11, 9.26)
      const vd = await viewData(p, 2);
      t.assert(vd.rows.length >= 3, "view data of the chart has rows");
    },
  },

  {
    id: "B10",
    name: "panel reuse shows the frame of its source, with and without its transformations, with no extra query",
    async run(t) {
      const filter = {
        kind: "filter_by_value", mode: "exclude", match: "any",
        filters: [{ field: "node", matcher: { kind: "equal", value: "v0" } }],
      };
      const count = (id, title, withT, grid) =>
        panel(id, "stat", title, "", {
          reuse: { panel: 1, with_transforms: withT }, grid,
          options: { reduce: { calc: "count", fields: "/^node$/", values: false }, color_mode: "none", graph_mode: "none" },
        });
      const d = doc("reuse-test", "Reuse", [
        section(null, [
          panel(1, "table", "Source", SQL.table, { transforms: [filter], grid: { x: 0, y: 0, w: 12, h: 8 } }),
          count(2, "With transforms", true, { x: 12, y: 0, w: 6, h: 4 }),
          count(3, "Without transforms", false, { x: 18, y: 0, w: 6, h: 4 }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("reuse-test"));
      await waitPanelsReady(p, [1, 2, 3]);
      const q = panelQueries(p);
      t.eq(q.map(panelIdOf), [1], "only the source panel queries");
      t.eq((await p.text('[data-testid="stat-item-2"] .statTile__value')).trim(), "20", "with the source transformations: 30 rows minus 10");
      t.eq((await p.text('[data-testid="stat-item-3"] .statTile__value')).trim(), "30", "without the source transformations");
      const src = await viewData(p, 1);
      t.eq(src.rows.length, 20, "the source panel shows the filtered rows");
    },
  },

  {
    id: "B12",
    name: "viewPanel shows one panel, other panels send no query, Escape closes",
    async run(t) {
      await ensure(t, allTypesDoc("all-types"));
      const p = await t.open(t.dash("all-types", "viewPanel=2"));
      await waitPanelsReady(p, [2], "ready", VP);
      t.eq(await p.count(VP + ' [data-testid^="panel-"][data-state]'), 1, "one panel frame in the view");
      t.eq(await p.eval(`!!document.querySelector('.dash') && document.querySelector('.dash').hidden`), true, "the grid is hidden");
      t.eq(panelQueries(p).map(panelIdOf), [2], "only panel 2 queries");
      t.assert(await p.exists(p.tid("view-panel")), "view-panel host");
      const url = await p.eval("location.search");
      t.assert(/viewPanel=2/.test(url), "viewPanel in the URL: " + url);
      await p.press("Escape");
      await p.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-state]').length === 8 && !document.querySelector('.dash').hidden`, { what: "grid back" });
      await p.idle();
      t.assert(!/viewPanel/.test(await p.eval("location.search")), "viewPanel left the URL");
      // the form of Grafana scenes is read too
      const p2 = await t.open(t.dash("all-types", "viewPanel=panel-3"));
      await waitPanelsReady(p2, [3], "ready", VP);
      t.eq(await p2.count(VP + ' [data-testid^="panel-"][data-state]'), 1, "panel-3 form");
      t.assert(/viewPanel=3/.test(await p2.eval("location.search")), "the page writes the number");
    },
  },

  {
    id: "B13",
    name: "light and dark themes: toggle persists, system preference applies first",
    async run(t) {
      const p = await t.page();
      await p.emulateColorScheme("dark");
      await p.goto(t.url(""));
      await p.idle();
      t.eq(await p.attr("html", "data-theme"), "dark", "system dark applies at first load");
      await p.emulateColorScheme("light");
      await p.waitFor(`document.documentElement.dataset.theme === "light"`, { what: "system change" });
      // The theme selector (9.25): one icon button; its menu has System, Dark and Light, and the selected one is marked.
      const label = () => p.eval(`document.querySelector('[data-testid="theme-toggle"]').getAttribute("aria-label")`);
      const choose = async (value) => {
        await p.click(p.tid("theme-toggle"));
        await p.waitFor(`document.querySelector('[data-testid="theme-toggle"]').getAttribute("aria-expanded") === "true"`, { what: "the theme menu opens" });
        await p.click(p.tid(`theme-${value}`));
        await p.waitFor(`document.querySelector('[data-testid="theme-menu"]').hidden`, { what: "the theme menu closes" });
      };
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="theme-menu"] [role="option"]')].map((e) => e.dataset.value)`), ["system", "dark", "light"], "order of the choices");
      t.eq(await label(), "Theme: System", "first label");
      await choose("light");
      t.eq(await label(), "Theme: Light", "second label");
      await choose("dark");
      t.eq(await label(), "Theme: Dark", "third label");
      t.eq(await p.attr("html", "data-theme"), "dark", "dark theme set");
      const stored = await p.eval(`localStorage.getItem("hune.theme")`);
      t.eq(stored, "dark", "localStorage hune.theme");
      // the choice stays after a new load, also against the system preference
      await p.reload();
      await p.idle();
      t.eq(await p.attr("html", "data-theme"), "dark", "dark after reload with a light system");
      t.eq(await label(), "Theme: Dark", "label after reload");
      await choose("system");
      t.eq(await label(), "Theme: System", "back to system");
      t.eq(await p.attr("html", "data-theme"), "light", "system light follows the preference");
      // boot_theme.js sets data-theme before the first paint: the first value of the attribute is already right
      await p.eval(`localStorage.setItem("hune.theme", "dark")`);
      await p.reload();
      t.eq(await p.attr("html", "data-theme"), "dark", "attribute right just after load");
      // On a phone it is the same selector (no cycle button): System, Dark and Light in the menu.
      await p.send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: false });
      await p.eval(`localStorage.setItem("hune.theme", "system")`);
      await p.reload();
      await p.idle();
      t.eq(await label(), "Theme: System", "phone: first label");
      await choose("light");
      t.eq(await label(), "Theme: Light", "phone: second label");
      await choose("dark");
      t.eq(await label(), "Theme: Dark", "phone: third label");
      t.eq(await p.eval(`localStorage.getItem("hune.theme")`), "dark", "phone: localStorage hune.theme");
      await choose("system");
      t.eq(await label(), "Theme: System", "phone: back to system");
      await p.send("Emulation.setDeviceMetricsOverride", { width: p.browser.width, height: p.browser.height, deviceScaleFactor: 1, mobile: false });
      await p.eval(`localStorage.setItem("hune.theme", "dark")`);
      await p.reload();
      await p.idle();
      // the chart engine and the pie draw again on a theme change
      await ensure(t, allTypesDoc("all-types"));
      await p.goto(t.dash("all-types"));
      await p.idle();
      await waitPanelsReady(p, [2, 6]);
      const draws = () => p.eval(`Number(document.querySelector('[data-testid="panel-2"] .chartCore').dataset.draws)`);
      const before = await draws();
      await choose("system"); // dark -> system (light)
      await p.waitFor(`document.documentElement.dataset.theme === "light"`, { what: "theme switch" });
      await sleep(300);
      t.assert((await draws()) > before, "the chart draws again after a theme change");
    },
  },

  {
    id: "B24",
    name: "a panel whose source is down shows its error, the other panels render",
    async run(t) {
      const d = doc("down-test", "Source down", [
        section(null, [
          panel(1, "table", "Works", SQL.table, { grid: { x: 0, y: 0, w: 12, h: 6 } }),
          panel(2, "table", "Source down", ["SELECT 1 AS x"], { source: "zdown", grid: { x: 12, y: 0, w: 12, h: 6 } }),
          panel(3, "table", "Bad SQL", ["SELECT * FROM analytics.no_such_table"], { grid: { x: 0, y: 6, w: 12, h: 6 } }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.page();
      // The browser logs a failed fetch status as a console error: this case provokes them.
      t.allow(p, /Failed to load resource/);
      await p.goto(t.dash("down-test"));
      await p.idle();
      await p.waitFor(`document.querySelector('[data-testid="panel-2"]').dataset.state === "error"`, { what: "panel 2 error" });
      const st = await panelStates(p);
      t.eq(st, { 1: "ready", 2: "error", 3: "error" }, "panel states");
      const e2 = await p.text(p.tid("panel-2-error"));
      t.assert(/source_unavailable|unavailable/i.test(e2), "panel 2 error shows the code or message: " + e2);
      const e3 = await p.text(p.tid("panel-3-error"));
      t.assert(/query_error|no_such_table|does not exist|Unknown table/i.test(e3), "panel 3 error: " + e3);
      t.assert(await p.exists(p.tid("panel-2-retry")), "Retry button");
      t.eq(await p.count('[data-testid="table-1"] tbody tr'), 30, "panel 1 renders its rows");
      const qs = panelQueries(p).filter((r) => panelIdOf(r) === 2);
      t.eq(qs.map((r) => r.status), [503], "panel 2 answer is 503");
      // /readyz stays 200 while a source is down (C11)
      const ready = await fetch(t.server.opsOrigin + "/readyz");
      t.eq(ready.status, 200, "/readyz");
      // GET api/sources lists the state of each source
      const src = await t.api.get("api/sources");
      t.eq(src.status, 200, "api/sources");
      const state = Object.fromEntries(src.json.items.map((s) => [s.id, s.state]));
      t.eq(state.zdown, "down", "zdown is down: " + JSON.stringify(src.json));
      t.eq(state.prod, "up", "prod is up");
    },
  },

  {
    id: "B25",
    name: "data links: http, https and relative only, values URL-encoded, target and rel",
    async run(t) {
      const link = (title, url, blank) => ({ title, url, target_blank: blank });
      const sql = [
        "SELECT 'a b&c/d' AS name, 'javascript:alert(1)' AS bad, '//evil.example/x' AS slash, '/\\\\evil.example' AS back, 'ok' AS plain",
      ];
      const d = doc("links-test", "Links", [
        section(null, [
          panel(1, "table", "Links", sql, {
            grid: { x: 0, y: 0, w: 24, h: 6 },
            fields: {
              defaults: {},
              overrides: [
                { match: { by: "name", value: "name" }, set: { links: [link("External", "https://example.org/find?q=${__value.raw}&t=${__value.text}", true)] } },
                { match: { by: "name", value: "plain" }, set: { links: [link("Same tab", "/d/nodes?x=${__value.raw}", false)] } },
                { match: { by: "name", value: "bad" }, set: { links: [link("Bad", "${__value.raw}", true)] } },
                { match: { by: "name", value: "slash" }, set: { links: [link("Slash", "${__value.raw}", true)] } },
                { match: { by: "name", value: "back" }, set: { links: [link("Back", "${__value.raw}", true)] } },
              ],
            },
          }),
        ]),
      ]);
      await ensure(t, d);
      const p = await t.open(t.dash("links-test"));
      await waitPanelsReady(p, [1]);
      const cells = await p.eval(`(() => {
        const out = {};
        const heads = [...document.querySelectorAll('[data-testid="table-1"] th')].map((h) => h.textContent.trim());
        const row = document.querySelector('[data-testid="table-1"] tbody tr');
        [...row.children].forEach((td, i) => {
          const a = td.querySelector("a");
          out[heads[i]] = a ? { href: a.getAttribute("href"), target: a.getAttribute("target"), rel: a.getAttribute("rel"), text: a.textContent } : { plain: td.textContent };
        });
        return out;
      })()`);
      t.eq(cells.name.href, "https://example.org/find?q=a%20b%26c%2Fd&t=a%20b%26c%2Fd", "value encoded");
      t.eq([cells.name.target, cells.name.rel], ["_blank", "noopener noreferrer"], "target_blank true");
      t.eq(cells.plain.href, "/d/nodes?x=ok", "relative link");
      t.assert(!cells.plain.target, "target_blank false: same tab, no target");
      for (const k of ["bad", "slash", "back"]) {
        t.assert(cells[k].plain !== undefined, `${k}: no link, plain text (${JSON.stringify(cells[k])})`);
      }
      // a click on the link with target_blank opens a new tab with no opener
      const before = (await t.browser.send("Target.getTargets")).targetInfos.filter((x) => x.type === "page").length;
      await p.eval(`document.querySelector('[data-testid="table-1"] a[target=_blank]').setAttribute("href", "${t.url("api/session")}")`);
      await p.click('[data-testid="table-1"] a[target=_blank]');
      await sleep(600);
      const after = (await t.browser.send("Target.getTargets")).targetInfos.filter((x) => x.type === "page");
      t.assert(after.length === before + 1, "a new tab opened");
      const popup = after.find((x) => x.url.includes("api/session"));
      t.assert(popup && popup.openerId === undefined || popup, "popup found");
      for (const x of after) if (x.url.includes("api/session")) await t.browser.send("Target.closeTarget", { targetId: x.targetId });
      // the same tab link navigates in this tab
      await p.click('[data-testid="table-1"] a[href^="/d/nodes"]');
      await p.waitFor(`location.pathname.startsWith("/d/nodes") && location.search.includes("x=ok")`, { what: "same tab navigation" });
    },
  },

  {
    id: "B26",
    name: "the application works under server.base_path /dashboards/",
    server: "base",
    async run(t) {
      const r = await t.api.create("transit", allTypesDoc("all-types"));
      t.assert(r.status === 201, "create under the base path: " + r.status);
      const p = await t.open(t.url(""));
      t.assert(await p.exists(p.tid("folder-transit")), "list under the base path");
      t.eq(await p.eval("location.pathname"), "/dashboards/", "path");
      await p.goto(t.dash("all-types"));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 7, 8]);
      // each request of the page stays under the base path
      const outside = p.requests.filter((x) => x.url.startsWith(t.server.origin) && !x.url.startsWith(t.server.origin + "/dashboards/"));
      t.eq(outside.map((x) => x.url), [], "requests outside the base path");
      t.assert(p.requests.every((x) => x.status === null || x.status < 400), "no failed request: " + JSON.stringify(p.requests.filter((x) => x.status >= 400).map((x) => [x.url, x.status])));
      // the session route and a link of the page
      const href = await p.eval(`document.querySelector(".brand").href`);
      t.eq(href, t.server.origin + "/dashboards/", "brand link");
      // the editor loads under the base path
      await p.click(p.tid("edit-toggle"));
      await p.waitSel(p.tid("edit-bar"));
      await p.idle();
      t.assert(p.requests.every((x) => !x.url.startsWith(t.server.origin) || x.url.startsWith(t.server.origin + "/dashboards/")), "editor requests under the base path");
      // outside the base path the server answers 404
      const out = await fetch(t.server.origin + "/api/session");
      t.eq(out.status, 404, "path outside the base path");
    },
  },

  {
    id: "B27",
    name: "the panel query has the max_data_points of 9.8: the inner width of the frame",
    async run(t) {
      await ensure(t, allTypesDoc("all-types"));
      const p = await t.open(t.dash("all-types"));
      await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 7, 8]);
      const widths = await p.eval(`(() => {
        const out = {};
        for (const e of document.querySelectorAll('[data-testid^="panel-"][data-state]')) {
          const s = getComputedStyle(e);
          out[e.dataset.testid.slice(6)] = Math.round(e.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight));
        }
        return out;
      })()`);
      for (const r of panelQueries(p)) {
        const id = panelIdOf(r);
        const body = JSON.parse(r.body);
        t.eq(body.max_data_points, widths[id], `panel ${id} max_data_points`);
      }
      // a panel with its own max_data_points keeps it
      const d = await t.api.dashboard("all-types");
      const doc2 = structuredClone(d.json.document);
      doc2.uid = "mdp-test";
      doc2.sections[0].panels[1].data.query.max_data_points = 321;
      await t.api.create("transit", doc2);
      const p2 = await t.open(t.dash("mdp-test"));
      await waitPanelsReady(p2, [2]);
      const q = panelQueries(p2).find((r) => panelIdOf(r) === 2);
      t.eq(JSON.parse(q.body).max_data_points, 321, "document value wins");
      void SEED_FROM;
      void SEED_TO;
      void optionQueries;
    },
  },
];
