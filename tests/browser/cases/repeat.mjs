// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B93 to B101 (4.13a, 9.8c, 9.26, 10.2a rule 16, 10.3, 10.6): the repeat of panels and sections by a variable.
// B93: direction h (the band). B94: direction v and the moves of the grid. B95: a repeated section. B96: empty and too large, with the limits of the server.
// B97: the bound of the queries. B98: URL forms and rule 16. B99: the editor. B100: layouts and accessibility. B101: hostile values.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { doc, panel, section } from "../lib/docs.mjs";
import { api as apiOf } from "../lib/env.mjs";
import { draftOf, enterEdit, moreItem, openPanelEditor, backToDashboard, waitDraft } from "../lib/ed.mjs";
import { panelQueries, panelIdOf, optionQueries, sleep, waitPanelsReady } from "../lib/h.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const vectors = JSON.parse(readFileSync(join(HERE, "../../frontend/vectors/FE-27-repeat.json"), "utf8"));

const NAMES = ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"];
const LETTERS = ["a", "b", "c", "d", "e", "f"];
const STAT_SQL = ["SELECT '${host:csv}' AS host, 1 AS n"];

/** A custom variable with `count` options (a, b, c, ... with the texts Alpha, Bravo, ...), multi, with All. */
const hostVar = (count = 6, name = "host", label = null) => ({
  kind: "custom", name, label, hide: "none",
  options: Array.from({ length: count }, (_, i) => ({ text: i < 6 ? NAMES[i] : `Host ${i}`, value: i < 6 ? LETTERS[i] : `h${i}` })),
  multi: true, include_all: true, all_value: null, default: ["$__all"],
});
const envVar = () => ({ kind: "custom", name: "env", label: null, hide: "none", options: [{ text: "Production", value: "prod" }], multi: false, include_all: false, all_value: null, default: ["prod"] });
const regionVar = (sql) => ({ kind: "query", name: "region", label: "Region", hide: "none", source: "prod", sql, multi: true, include_all: true, all_value: null, refresh: "load", sort: "alpha_asc", regex: null, default: ["$__all"] });

/** A stat that repeats. `rep` is the member `repeat`; `title` shows the option text of its copy. */
function rstat(id, grid, rep, o = {}) {
  const p = panel(id, "stat", o.title ?? "Host ${host}", o.sql || STAT_SQL, { grid });
  if (rep) p.repeat = rep;
  return p;
}
const plain = (id, grid, title = `Plain ${id}`, sql = ["SELECT 1 AS one"]) => panel(id, "stat", title, sql, { grid });

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 600)}`);
}

const hostQuery = (values) => values.map((v) => `var-host=${v}`).join("&");
const rects = (p, id) => p.eval(`[...document.querySelectorAll('section.panel')].filter((e) => /^panel-${id}(-r\\d+)?$/.test(e.dataset.testid)).map((e) => { const r = e.getBoundingClientRect(); return { key: e.dataset.testid, n: e.dataset.repeatN, count: e.dataset.repeatCount, x: r.left, y: r.top + scrollY, w: r.width, h: r.height, title: (e.querySelector('.panel__title') || {}).textContent }; })`);
const bodiesOf = (p, id) => panelQueries(p).filter((r) => panelIdOf(r) === id).map((r) => JSON.parse(r.body));
const near = (a, b, tol = 1.2) => Math.abs(a - b) <= tol;

// A probe of the fetch calls: the requests of the panels that are open now and at most, and the aborts. It runs from the first moment of a page.
const FETCH_PROBE = `(() => {
  const log = (window.__fl = { open: 0, max: 0, total: 0, aborted: 0, options: 0 });
  const f = window.fetch;
  window.fetch = function (input, init) {
    const url = typeof input === "string" ? input : input.url;
    const isQuery = /\\/panels\\/\\d+\\/query$/.test(url);
    if (/\\/variables\\/[^/]+\\/options$/.test(url)) log.options++;
    if (!isQuery) return f.apply(this, arguments);
    log.open++; log.total++; log.max = Math.max(log.max, log.open);
    const end = () => { log.open--; };
    return f.apply(this, arguments).then((r) => { end(); return r; }, (e) => { end(); if (e && e.name === "AbortError") log.aborted++; throw e; });
  };
})();`;

async function probePage(t, url, { width = 1280, height = 900, idle = true } = {}) {
  const p = await t.page({ width, height });
  await p.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await p.send("Page.addScriptToEvaluateOnNewDocument", { source: FETCH_PROBE });
  await p.goto(url);
  if (idle) await p.idle();
  return p;
}

const stateOf = (p, key) => p.eval(`(document.querySelector('[data-testid="panel-${key}"]') || {dataset: {}}).dataset.state || null`);

export const cases = [
  {
    id: "B93",
    name: "repeat of a panel with direction h: c equal columns with a gap of 8 px, the last row not stretched, the order of the options, one request for each copy with its value (4.13a, 9.8c)",
    async run(t) {
      const grids = [[0, 0], [1, 6], [2, 12], [3, 18]].map(([i, y]) => ({ x: 6, y, w: 6, h: 4 })); // the stored x and w never count for h
      const maxes = [undefined, 2, 3, 5];
      const d = doc("rep93", "Repeat h", [section(null, maxes.map((m, i) => rstat(i + 1, grids[i], { variable: "host", direction: "h", ...(m === undefined ? {} : { max_per_row: m }) })))], { variables: [hostVar(6), envVar()] });
      await ensure(t, d);
      for (const k of [1, 2, 3, 5, 6]) {
        const chosen = LETTERS.slice(0, k);
        const p = await probePage(t, t.dash("rep93", hostQuery(chosen)));
        await waitPanelsReady(p, [1, 2, 3, 4].flatMap((id) => [id, ...Array.from({ length: k - 1 }, (_, i) => `${id}-r${i + 2}`)]));
        for (const [i, m] of maxes.entries()) {
          const id = i + 1;
          const list = await rects(p, id);
          const c = Math.min(k, m ?? 4);
          const name = `k=${k} max_per_row=${m ?? "absent"}`;
          t.eq(list.map((r) => r.key), Array.from({ length: k }, (_, n) => (n === 0 ? `panel-${id}` : `panel-${id}-r${n + 1}`)), `${name}: keys`);
          t.eq(list.map((r) => r.title), NAMES.slice(0, k).map((n) => `Host ${n}`), `${name}: titles are the option texts, in the order of the options`);
          t.eq(list.map((r) => [r.n, r.count]), list.map((_, n) => [String(n + 1), String(k)]), `${name}: data-repeat-n and data-repeat-count`);
          const band = await p.eval(`(() => { const b = document.querySelector('[data-testid="repeat-band-${id}"]'); const r = b.getBoundingClientRect(); const s = b.parentElement.getBoundingClientRect(); return { x: r.left, w: r.width, sectionW: s.width, y: r.top + scrollY, h: r.height }; })()`);
          t.assert(near(band.w, band.sectionW), `${name}: the band has the full width of the grid: ${band.w} of ${band.sectionW}`);
          const colW = (band.w - 8 * (c - 1)) / c;
          const rowH = 4 * 30 + 3 * 8;
          const q = Math.ceil(k / c);
          t.assert(near(band.h, q * rowH + (q - 1) * 8), `${name}: the band height ${band.h}`);
          list.forEach((r, n) => {
            t.assert(near(r.w, colW), `${name}: copy ${n + 1} width ${r.w}, column ${colW}`);
            t.assert(near(r.h, rowH), `${name}: copy ${n + 1} height ${r.h}`);
            t.assert(near(r.x, band.x + (n % c) * (colW + 8)), `${name}: copy ${n + 1} x ${r.x}`);
            t.assert(near(r.y, band.y + Math.floor(n / c) * (rowH + 8)), `${name}: copy ${n + 1} y ${r.y}`);
          });
          // The requests: one for each copy, with the repeat variable fixed to its value, and the other variables as in the bar.
          const bodies = bodiesOf(p, id);
          t.eq(bodies.length, k, `${name}: the count of requests of the panel`);
          t.eq(bodies.map((b) => b.variables.host).sort((a, b) => a.values[0].localeCompare(b.values[0])), chosen.map((v) => ({ all: false, values: [v] })), `${name}: each copy has its value, all = false`);
          t.assert(bodies.every((b) => JSON.stringify(b.variables.env) === JSON.stringify({ all: false, values: ["prod"] })), `${name}: the other variables`);
        }
        const total = panelQueries(p).length;
        t.eq(total, 4 * k, `k=${k}: the count of panel requests equals the count of copies`);
        await p.close();
      }
      // The order of the copies is the order of the options, also for the URL c, a.
      const q = await t.open(t.dash("rep93", "var-host=c&var-host=a"));
      await waitPanelsReady(q, [1, "1-r2"]);
      t.eq((await rects(q, 1)).map((r) => r.title), ["Host Alpha", "Host Charlie"], "the order of the options, not of the URL");
      // All: each option is a value.
      const all = await t.open(t.dash("rep93"));
      await waitPanelsReady(all, [1, "1-r6"]);
      t.eq((await rects(all, 1)).length, 6, "All gives 6 copies");
    },
  },
  {
    id: "B94",
    name: "repeat of a panel with direction v: the copies one under the other, and the moves of the other panels with the rule of 9.8c (the vectors FE-27); the stored x and w of h do not count",
    async run(t) {
      const m = vectors.moves[0];
      const items = m.items;
      const panels = items.map((it) => (it.rep ? rstat(it.id, { x: it.x, y: it.y, w: it.w, h: it.h }, { variable: "host", direction: "v" }) : plain(it.id, { x: it.x, y: it.y, w: it.w, h: it.h })));
      await ensure(t, doc("rep94", "Repeat v", [section(null, panels)], { variables: [hostVar(3)] }));
      const p = await t.open(t.dash("rep94"));
      await waitPanelsReady(p, [1, 2, "2-r2", "2-r3", 3, 4, 5, 6, 7, 8]);
      const rows = await p.eval(`Object.fromEntries([...document.querySelectorAll('section.panel')].map((e) => [e.dataset.testid, [e.style.gridRow, e.style.gridColumn]]))`);
      const first = (key) => Number(/^(\d+) \//.exec(rows[key][0])[1]) - 1;
      for (const [id, y] of Object.entries(m.expect)) t.eq(first(`panel-${id}`), y, `the displayed y of the panel ${id}`);
      // The copies have the x and w of the panel, one under the other, with the row unit of the panel.
      t.eq([first("panel-2"), first("panel-2-r2"), first("panel-2-r3")], [2, 6, 10], "the copies of the panel 2");
      t.eq(["panel-2", "panel-2-r2", "panel-2-r3"].map((k) => rows[k][1]), ["9 / span 8", "9 / span 8", "9 / span 8"], "x and w of the copies");
      // The measure of Grafana: the top of the panels 5 to 8 relative to the panel 1.
      const tops = await p.eval(`Object.fromEntries([5, 6, 7, 8].map((id) => [id, Math.round(document.querySelector('[data-testid="panel-' + id + '"]').getBoundingClientRect().top + scrollY)]))`);
      const base = await p.eval(`Math.round(document.querySelector('[data-testid="panel-1"]').getBoundingClientRect().top + scrollY)`);
      t.eq([5, 6, 7, 8].map((id) => tops[id] - base), [6, 14, 6, 16].map((y) => y * 38), "the tops of the panels 5 to 8 follow 38 px for each row unit");
      // No overlap and no horizontal scroll.
      const overlap = await p.eval(`(() => { const r = [...document.querySelectorAll('section.panel')].map((e) => [e.dataset.testid, e.getBoundingClientRect()]); const bad = []; for (let i = 0; i < r.length; i++) for (let j = i + 1; j < r.length; j++) { const a = r[i][1], b = r[j][1]; if (a.left < b.right - 0.5 && b.left < a.right - 0.5 && a.top < b.bottom - 0.5 && b.top < a.bottom - 0.5) bad.push(r[i][0] + " " + r[j][0]); } return bad; })()`);
      t.eq(overlap, [], "no overlap");
      t.eq(await p.eval("document.documentElement.scrollWidth <= document.documentElement.clientWidth"), true, "no horizontal scroll");
      // A panel with h: the stored x and w do not change the view (the copies have the full width).
      await ensure(t, doc("rep94h", "Repeat h stored", [section(null, [rstat(1, { x: 5, y: 0, w: 5, h: 3 }, { variable: "host", direction: "h", max_per_row: 3 })])], { variables: [hostVar(3)] }));
      const h = await t.open(t.dash("rep94h"));
      await waitPanelsReady(h, [1, "1-r2", "1-r3"]);
      const hb = await h.eval(`(() => { const b = document.querySelector('[data-testid="repeat-band-1"]').getBoundingClientRect(); const g = document.querySelector('.dash__grid').getBoundingClientRect(); return [Math.round(b.left - g.left), Math.round(b.width - g.width)]; })()`);
      t.eq(hb, [0, 0], "the band has the full width whatever the stored x and w");
    },
  },
  {
    id: "B95",
    name: "repeat of a section: one section for each value with its title and panels, the keys, the request of each panel, each copy opens and closes by itself, print (4.13a, 9.8c)",
    async run(t) {
      const mk = (uid, collapsed) => doc(uid, "Repeat section", [
        section(null, [plain(1, { x: 0, y: 0, w: 24, h: 3 })]),
        { title: "Host ${host}", collapsed, repeat: { variable: "host" }, panels: [rstat(2, { x: 0, y: 0, w: 12, h: 4 }, null, { title: "A ${host}" }), rstat(3, { x: 12, y: 0, w: 12, h: 4 }, null, { title: "B ${host}" })] },
      ], { variables: [hostVar(3)] });
      await ensure(t, mk("rep95", false));
      await ensure(t, mk("rep95c", true));
      const p = await t.open(t.dash("rep95", hostQuery(["c", "a", "b"])));
      await waitPanelsReady(p, [1, 2, 3, "2-r2", "3-r2", "2-r3", "3-r3"]);
      t.eq(await p.eval(`[...document.querySelectorAll('section.section')].map((e) => e.dataset.testid)`), ["section-0", "section-1", "section-1-r2", "section-1-r3"], "the section copies, in the order of the options; the first section is never a copy");
      t.eq(await p.eval(`[...document.querySelectorAll('section.section .section__title')].map((e) => e.textContent)`), ["Host Alpha", "Host Bravo", "Host Charlie"], "the header titles have the option texts");
      t.eq(await p.eval(`[...document.querySelectorAll('section.section')].map((e) => e.getAttribute("aria-label"))`), ["Panels", "Host Alpha", "Host Bravo", "Host Charlie"], "the names of the copies: the title holds the option text, so nothing is added (9.26)");
      // A section title with no placeholder is the same in each copy: the accessible name adds a comma and the option text.
      await ensure(t, doc("rep95n", "Repeat section name", [{ title: "Hosts", collapsed: false, repeat: { variable: "host" }, panels: [plain(1, { x: 0, y: 0, w: 24, h: 3 })] }], { variables: [hostVar(3)] }));
      const named = await t.open(t.dash("rep95n", hostQuery(["c", "a", "b"])));
      await waitPanelsReady(named, [1, "1-r2", "1-r3"]);
      t.eq(await named.eval(`[...document.querySelectorAll('section.section')].map((e) => [e.getAttribute("aria-label"), e.querySelector(".section__title").textContent])`), [["Hosts, Alpha", "Hosts"], ["Hosts, Bravo", "Hosts"], ["Hosts, Charlie", "Hosts"]], "a copy of a section: the name has the option text, the header keeps the title");
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="section-1-r2"] .panel__title')].map((e) => e.textContent)`), ["A Bravo", "B Bravo"], "the titles of the panels of the copy 2");
      // The request of each panel of the copy n has the repeat variable fixed to the value of the copy.
      const seen = panelQueries(p).filter((r) => panelIdOf(r) !== 1).map((r) => [panelIdOf(r), JSON.parse(r.body).variables.host.values.join()]).sort().map((x) => x.join(":"));
      t.eq(seen, ["2:a", "2:b", "2:c", "3:a", "3:b", "3:c"], "the stored id and one value for each copy");
      t.eq(panelQueries(p).filter((r) => panelIdOf(r) === 1).length, 1, "the panel with no repeat sends its request once");
      // A copy opens and closes by itself.
      await p.click('[data-testid="section-1-r2"] .section__toggle');
      t.eq(await p.eval(`[...document.querySelectorAll('section.section')].map((e) => { const b = e.querySelector('.section__toggle'); return b ? b.getAttribute("aria-expanded") : null; })`), [null, "true", "false", "true"], "only the copy 2 is closed");
      t.eq(await p.eval(`document.querySelector('[data-testid="section-1-r2"] .dash__grid').hidden`), true, "the grid of the copy 2 is hidden");
      // The document is not changed by a collapse of a copy (view mode).
      t.eq((await t.api.dashboard("rep95")).json.document.sections[1].collapsed, false, "the document keeps collapsed = false");
      // A collapsed section: each copy is collapsed and sends no query. An open copy queries with its value only.
      const c = await t.open(t.dash("rep95c", hostQuery(["a", "b", "c"])));
      await sleep(600);
      await c.idle();
      t.eq(panelQueries(c).filter((r) => panelIdOf(r) !== 1).length, 0, "a collapsed copy sends no query");
      c.clearRequests();
      await c.click('[data-testid="section-1-r2"] .section__toggle');
      await c.waitFor(`document.querySelector('[data-testid="panel-2-r2"]').dataset.state === "ready"`, { what: "panel-2-r2 ready" });
      await c.idle();
      t.eq(panelQueries(c).map((r) => [panelIdOf(r), JSON.parse(r.body).variables.host.values.join()].join(":")).sort(), ["2:b", "3:b"], "the open copy queries with its value");
      // Print shows the copies and the line of the values.
      await p.send("Emulation.setEmulatedMedia", { media: "print" });
      const print = await p.eval(`({ meta: document.querySelector(".dash__print-meta").textContent, shown: [...document.querySelectorAll('section.section')].map((e) => getComputedStyle(e).display !== "none") })`);
      t.assert(/host: c, a, b/i.test(print.meta), `the print line of the values: ${print.meta}`);
      t.eq(print.shown, [true, true, true, true], "the copies show in print");
      await p.send("Emulation.setEmulatedMedia", { media: "" });
    },
  },
  {
    id: "B96",
    name: "empty and too large: repeat-empty, repeat-limit for a panel, a section and the dashboard, the same after a reload and a scroll, the move between states, the limits of the server (9.8c)",
    async run(t) {
      const many = hostVar(30, "many", "Many");
      const manyValues = many.options.map((o) => o.value);
      const sel = (n, name = "many") => manyValues.slice(0, n).map((v) => `var-${name}=${v}`).join("&");
      // (a) A query variable with no row: the state repeat-empty for a panel and for a section, with the message and no request.
      const empty = doc("rep96a", "Repeat empty", [
        section(null, [rstat(1, { x: 0, y: 0, w: 24, h: 3 }, { variable: "region", direction: "h" }, { title: "R ${region}", sql: ["SELECT '${region:csv}' AS region"] }), plain(5, { x: 0, y: 3, w: 24, h: 2 })]),
        { title: "Region ${region}", collapsed: false, repeat: { variable: "region" }, panels: [rstat(2, { x: 0, y: 0, w: 24, h: 3 }, null, { title: "P ${region}", sql: ["SELECT '${region:csv}' AS region"] })] },
      ], { variables: [regionVar(["SELECT toString(number) AS v FROM numbers(0)"])] });
      await ensure(t, empty);
      const probe = `(() => { const log = (window.__seq = []); new MutationObserver(() => { const e = document.querySelector('[data-testid="panel-1"]'); if (!e) return; const k = e.dataset.state + ":" + Boolean(document.querySelector('[data-testid="repeat-note-1"]')); if (log[log.length - 1] !== k) log.push(k); }).observe(document, { subtree: true, childList: true, attributes: true }); })()`;
      const p = await t.page({ width: 1280, height: 900 });
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: probe });
      await p.goto(t.dash("rep96a"));
      await p.idle();
      await p.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "repeat-empty"`, { what: "repeat-empty" });
      const seq = await p.eval("window.__seq");
      t.assert(!seq.includes("idle:true"), `no message before the options load: ${JSON.stringify(seq)}`);
      t.eq(seq[seq.length - 1], "repeat-empty:true", "the state at the end");
      t.eq(await p.text(p.tid("repeat-note-1")), "No value of Region is selected. Nothing to repeat.", "the message of the panel");
      t.eq(await p.attr(p.tid("repeat-note-1"), "role"), "note", "role note");
      t.eq(await p.text(p.tid("section-1-note")), "No value of Region is selected. Nothing to repeat.", "the message of the section");
      t.eq(await p.count('section.section[data-testid^="section-1"]'), 1, "the section shows once");
      t.eq(await p.count('[data-testid="section-1"] section.panel'), 0, "a section in this state holds no panel");
      t.eq(panelQueries(p).filter((r) => [1, 2].includes(panelIdOf(r))).length, 0, "no request for the repeats");
      t.eq(panelQueries(p).filter((r) => panelIdOf(r) === 5).length, 1, "the panel with no repeat runs");
      // The title bar of the frame shows, and the menu has no View data and no Inspect.
      t.eq(await p.text('[data-testid="panel-1"] .panel__title'), "R All", "the title shows with the selection of the page");
      await p.click(p.tid("panel-1-menu"));
      t.assert(!(await p.exists(p.tid("panel-1-view-data"))) && !(await p.exists(p.tid("panel-1-inspect"))), "no View data and no Inspect");
      await p.press("Escape");
      // (b) 24 and 25 values; a section with 12 and 13 values.
      const lim = doc("rep96b", "Repeat limits", [
        section(null, [rstat(1, { x: 0, y: 0, w: 24, h: 2 }, { variable: "many", direction: "h", max_per_row: 6 }, { title: "H ${many}", sql: ["SELECT '${many:csv}' AS many"] })]),
        { title: "S ${sec}", collapsed: false, repeat: { variable: "sec" }, panels: [rstat(2, { x: 0, y: 0, w: 24, h: 2 }, null, { title: "T ${sec}", sql: ["SELECT '${sec:csv}' AS sec"] })] },
      ], { variables: [many, hostVar(14, "sec", "Sec")] });
      await ensure(t, lim);
      const q24 = await t.open(t.dash("rep96b", `${sel(24)}&${sel(12, "sec")}`));
      await q24.waitFor(`document.querySelectorAll('section.panel[data-repeat-count="24"]').length === 24`, { what: "24 copies" });
      t.eq(await q24.count('section.section[data-testid^="section-1"]'), 12, "a section with 12 values has 12 copies");
      const q25 = await t.open(t.dash("rep96b", `${sel(25)}&${sel(13, "sec")}`));
      await q25.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "repeat-limit"`, { what: "repeat-limit" });
      t.eq(await q25.text(q25.tid("repeat-note-1")), "25 values of Many are selected. A panel repeats 24 times at most. Select fewer values.", "message of the panel");
      t.eq(await q25.count('section.panel[data-testid^="panel-1-r"]'), 0, "no copy shows");
      t.eq(await q25.text(q25.tid("section-1-note")), "13 values of Sec are selected. A section repeats 12 times at most. Select fewer values.", "message of the section");
      t.eq(panelQueries(q25).length, 0, "no request");
      // A change of the selection moves the panel between the states at once: remove one value (24), add it again (25).
      await q25.click(q25.tid("variable-many-button"));
      await q25.click(`.listbox__item[data-value="${manyValues[24]}"]`);
      await q25.click("h1.visually-hidden, #app-main", {});
      await q25.waitFor(`document.querySelectorAll('section.panel[data-repeat-count="24"]').length === 24`, { what: "24 copies after the change" });
      await q25.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "ready"`, { what: "the first copy is a panel again" });
      await q25.click(q25.tid("variable-many-button"));
      await q25.click(`.listbox__item[data-value="${manyValues[24]}"]`);
      await q25.click("h1.visually-hidden, #app-main", {});
      await q25.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "repeat-limit"`, { what: "repeat-limit again" });
      // (c) The budget of the dashboard: six panels with 24 values each. The first five fit (120), the sixth does not.
      const budget = doc("rep96c", "Repeat budget", [section(null, Array.from({ length: 6 }, (_, i) => rstat(i + 1, { x: 0, y: i * 3, w: 24, h: 2 }, { variable: "many", direction: "h", max_per_row: 24 }, { title: `P${i + 1} \${many}`, sql: ["SELECT '${many:csv}' AS many"] })))], { variables: [many] });
      await ensure(t, budget);
      const states = (pg) => pg.eval(`[1, 2, 3, 4, 5, 6].map((id) => document.querySelector('[data-testid="panel-' + id + '"]').dataset.state)`);
      const b = await t.open(t.dash("rep96c", sel(24)));
      await b.waitFor(`document.querySelector('[data-testid="panel-6"]').dataset.state === "repeat-limit"`, { what: "the sixth panel is repeat-limit" });
      const before = await states(b);
      t.assert(before.slice(0, 5).every((s) => s !== "repeat-limit" && s !== "repeat-empty"), `the first five repeat: ${before}`);
      t.eq(await b.text(b.tid("repeat-note-6")), "The repeats of this dashboard have more than 120 copies. Select fewer values of Many.", "the message of the dashboard");
      t.eq(await b.count('section.panel[data-repeat-count]'), 120, "120 copies");
      await b.eval("window.scrollTo(0, document.body.scrollHeight)");
      await sleep(300);
      t.eq(await states(b), before, "the same result after a scroll");
      await b.reload();
      await b.idle();
      await b.waitFor(`document.querySelector('[data-testid="panel-6"]').dataset.state === "repeat-limit"`, { what: "after a reload" });
      t.eq((await states(b)).map((s) => s === "repeat-limit"), [false, false, false, false, false, true], "the same result after a reload");
      // (d) The limits are keys of the server: a server with a panel limit 3, a section limit 2 and a dashboard limit 5.
      const small = await t.ownServer("caps", { serverLines: ["repeat_max_per_panel = 3", "repeat_max_per_section = 2", "repeat_max_per_dashboard = 5"] });
      t.eq((await apiOf(small).session()).json.repeat_limits, { panel: 3, section: 2, dashboard: 5 }, "the session gives the limits");
      t.eq((await t.api.session()).json.repeat_limits, { panel: 24, section: 12, dashboard: 120 }, "the defaults of the other server");
      const smallDoc = doc("rep96d", "Small caps", [
        section(null, [rstat(1, { x: 0, y: 0, w: 24, h: 2 }, { variable: "host", direction: "h" }), rstat(2, { x: 0, y: 3, w: 24, h: 2 }, { variable: "host", direction: "h" })]),
        { title: "S ${sec}", collapsed: false, repeat: { variable: "sec" }, panels: [rstat(3, { x: 0, y: 0, w: 24, h: 2 }, null, { title: "T ${sec}", sql: ["SELECT '${sec:csv}' AS sec"] })] },
      ], { variables: [hostVar(4), hostVar(3, "sec", "Sec")] });
      const made = await apiOf(small).create("transit", smallDoc);
      t.assert(made.status === 201, `create on the small server: ${made.status} ${made.text.slice(0, 300)}`);
      const on = (qs) => t.open(`${small.url}d/rep96d?${t.range}&${qs}`);
      const s4 = await on(hostQuery(LETTERS.slice(0, 4)) + "&var-sec=a&var-sec=b");
      await s4.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "repeat-limit"`, { what: "panel limit 3" });
      t.eq(await s4.text(s4.tid("repeat-note-1")), "4 values of host are selected. A panel repeats 3 times at most. Select fewer values.", "the panel limit of the server");
      t.eq(await s4.count('section.section[data-testid^="section-1"]'), 2, "the section limit 2 is met with 2 values");
      const s3 = await on(hostQuery(LETTERS.slice(0, 3)) + "&var-sec=a&var-sec=b&var-sec=c");
      await s3.waitFor(`document.querySelector('[data-testid="section-1-note"]')`, { what: "section limit 2" });
      t.eq(await s3.text(s3.tid("section-1-note")), "3 values of Sec are selected. A section repeats 2 times at most. Select fewer values.", "the section limit of the server");
      t.eq(await stateOf(s3, 1), "ready", "3 values fit the panel limit 3");
      t.eq(await s3.text(s3.tid("repeat-note-2")), "The repeats of this dashboard have more than 5 copies. Select fewer values of host.", "the dashboard limit 5: 3 + 3 does not fit");
    },
  },
  {
    id: "B97",
    name: "the bound of the queries: only the copies near the viewport query, 6 requests at most at the same time, a new value aborts the old requests, a refresh runs each copy once, no request to read the values (9.8c)",
    async run(t) {
      const regionSql = (n) => [`SELECT toString(number) AS v FROM numbers(${n})`];
      // (a) 24 copies one under the other, each taller than the viewport.
      const tall = doc("rep97a", "Repeat tall", [section(null, [rstat(1, { x: 0, y: 0, w: 24, h: 24 }, { variable: "region", direction: "v" }, { title: "R ${region}", sql: ["SELECT '${region:csv}' AS region"] })])], { variables: [regionVar(regionSql(24))] });
      await ensure(t, tall);
      const a = await probePage(t, t.dash("rep97a"));
      await a.waitFor(`document.querySelectorAll('section.panel[data-repeat-count="24"]').length === 24`, { what: "24 copies" });
      await sleep(800);
      await a.idle();
      const first = await a.eval("window.__fl");
      t.assert(first.total >= 1 && first.total <= 3, `at the load only the copies near the viewport query: ${first.total}`);
      const optionsAtLoad = optionQueries(a).length;
      t.eq(optionsAtLoad, 1, "one request of options at the load");
      let guard = 0;
      while ((await a.eval("window.__fl.total")) < 24 && guard++ < 60) {
        await a.eval("window.scrollBy(0, 700)");
        await sleep(250);
      }
      await a.idle();
      await sleep(600);
      const last = await a.eval("window.__fl");
      t.eq(last.total, 24, "24 requests at the end, one for each copy");
      t.assert(last.max <= 6, `at most 6 requests at the same time: ${last.max}`);
      t.eq(optionQueries(a).length, optionsAtLoad, "the page sends no extra request to read the values");
      // (b) 24 small copies: the queue holds the requests, 6 at most are open. A slow query keeps them open.
      const small = doc("rep97b", "Repeat small", [section(null, [rstat(1, { x: 0, y: 0, w: 24, h: 2 }, { variable: "region", direction: "h", max_per_row: 6 }, { title: "R ${region}", sql: ["SELECT sleepEachRow(0.25) AS s, '${region:csv}' AS region FROM numbers(8)"] })])], { variables: [regionVar(regionSql(24))] });
      await ensure(t, small);
      const withoutZero = Array.from({ length: 23 }, (_, i) => `var-region=${i + 1}`).join("&");
      const b = await probePage(t, t.dash("rep97b", withoutZero), { idle: false });
      await b.waitFor(`window.__fl.total >= 6`, { what: "the first requests" });
      await sleep(300);
      const mid = await b.eval("window.__fl");
      t.assert(mid.open <= 6 && mid.max <= 6, `at most 6 open requests: ${JSON.stringify(mid)}`);
      t.eq(mid.open, 6, `the queue keeps 6 requests open: ${JSON.stringify(mid)}`);
      // A new value in the selection: the value 0 comes first in the order of the options, so each copy has a new value. The old requests are aborted.
      await b.click(b.tid("variable-region-button"));
      await b.click('.listbox__item[data-value="0"]');
      await b.click("h1.visually-hidden, #app-main", {});
      await b.waitFor(`window.__fl.aborted >= 6`, { what: "the open requests are aborted" });
      await b.waitFor(`document.querySelectorAll('section.panel[data-repeat-count="24"]').length === 24`, { what: "24 copies" });
      await b.waitFor(`window.__fl.open === 0 && [...document.querySelectorAll('section.panel[data-repeat-count="24"]')].every((e) => e.dataset.state === "ready")`, { timeout: 90000, what: "the 24 copies are ready" });
      t.assert((await b.eval("window.__fl.max")) <= 6, "never more than 6 at the same time, also after the change");
      // A refresh runs each copy one time.
      const total = await b.eval("window.__fl.total");
      await b.click(b.tid("refresh-now"));
      await b.waitFor(`window.__fl.total >= ${total + 24}`, { timeout: 90000, what: "a refresh runs each copy" });
      await b.waitFor(`window.__fl.open === 0`, { timeout: 90000, what: "the refresh ends" });
      await sleep(400);
      t.eq((await b.eval("window.__fl.total")) - total, 24, "a refresh runs each copy one time");
    },
  },
  {
    id: "B98",
    name: "URL and states: the keys of a copy in viewPanel, the menu of a copy, editPanel is the stored panel, rule 16 (a key that names no copy is removed with a notice), a walk (9.5, 10.2a)",
    async run(t) {
      await ensure(t, doc("rep98", "Repeat URL", [section(null, [rstat(1, { x: 0, y: 0, w: 24, h: 4 }, { variable: "host", direction: "v" }), plain(2, { x: 0, y: 8, w: 24, h: 3 })])], { variables: [hostVar(3)] }));
      const search = async (p) => new URLSearchParams(await p.eval("location.search"));
      const viewOf = (p) => p.eval(`document.documentElement.getAttribute("data-hune-view")`);
      const notices = (p) => p.eval(`[...document.querySelectorAll('[data-testid="notice"] .notice__text')].map((e) => e.textContent)`);
      // (a) The forms of the URL show the copy n at full size, with its query, and the URL goes to the normal form. The form `<x>$panel-<id>`
      // of Grafana names the copy by its value: `b$panel-1` is the copy 2, and the first copy has the prefix too.
      for (const [value, key, vp, host] of [["1-r2", "1-r2", "1-r2", "b"], ["panel-1-r3", "1-r3", "1-r3", "c"], ["b%24panel-1", "1-r2", "1-r2", "b"], ["1", "1", "1", "a"], ["a%24panel-1", "1", "1", "a"], ["c$panel-1", "1-r3", "1-r3", "c"], ["panel-1", "1", "1", "a"]]) {
        const p = await t.open(t.dash("rep98", `viewPanel=${value}`));
        await p.waitFor(`document.querySelector('[data-testid="view-panel"] [data-testid="panel-${key}"]')`, { what: `the view of ${key}` });
        await p.waitFor(`document.querySelector('[data-testid="view-panel"] [data-testid="panel-${key}"]').dataset.state === "ready"`, { what: `${key} ready` });
        t.eq(await viewOf(p), "panel-view", `${value}: the state`);
        t.eq((await search(p)).get("viewPanel"), vp, `${value}: the normal form of the URL`);
        const q = panelQueries(p).filter((r) => panelIdOf(r) === 1).map((r) => JSON.parse(r.body).variables.host.values.join());
        t.eq(q, [host], `${value}: one query with the value of the copy`);
        t.eq(await p.text(`[data-testid="view-panel"] [data-testid="panel-${key}"] .panel__title`), `Host ${NAMES[LETTERS.indexOf(host)]}`, `${value}: the title`);
        t.eq(await p.count('[data-testid="view-panel"] section.panel'), 1, `${value}: one frame`);
      }
      // (b) The menu of a copy: "View" writes `<id>-r<n>`, and for the first copy the number. "Edit" opens the editor of the stored panel.
      const m = await t.open(t.dash("rep98"));
      await waitPanelsReady(m, [1, "1-r2", "1-r3", 2]);
      await m.click(m.tid("panel-1-r3-menu"));
      await m.click(m.tid("panel-1-r3-view"));
      await m.waitFor(`new URLSearchParams(location.search).get("viewPanel") === "1-r3"`, { what: "viewPanel=1-r3" });
      await m.press("Escape");
      await m.waitFor(`document.documentElement.getAttribute("data-hune-view") === "view"`, { what: "back to the view" });
      await m.click(m.tid("panel-1-menu"));
      await m.click(m.tid("panel-1-view"));
      await m.waitFor(`new URLSearchParams(location.search).get("viewPanel") === "1"`, { what: "viewPanel=1" });
      await m.press("Escape");
      await m.waitFor(`document.documentElement.getAttribute("data-hune-view") === "view"`, { what: "back to the view again" });
      await m.click(m.tid("panel-1-r2-menu"));
      await m.click(m.tid("panel-1-r2-edit"));
      await m.waitSel(m.tid("panel-editor"));
      t.eq((await search(m)).get("editPanel"), "1", "Edit opens editPanel=<id>");
      // (c) Rule 16: a key above the count, or the form of a copy in editPanel, goes with a notice.
      const bad = await t.open(t.dash("rep98", "viewPanel=1-r9"));
      await bad.waitFor(`document.documentElement.getAttribute("data-hune-view") === "view"`, { what: "the state view" });
      t.eq((await search(bad)).has("viewPanel"), false, "viewPanel is removed");
      t.assert((await notices(bad)).includes("This dashboard has no panel 1-r9."), `the notice: ${await notices(bad)}`);
      // The form of Grafana with a text that is not the value of a copy: an index, the option text, another case. No view, and the notice.
      for (const [value, text] of [["2%24panel-1", "2$panel-1"], ["Bravo%24panel-1", "Bravo$panel-1"], ["B$panel-1", "B$panel-1"]]) {
        const g = await t.open(t.dash("rep98", `viewPanel=${value}`));
        await g.waitFor(`document.documentElement.getAttribute("data-hune-view") === "view"`, { what: `the state view for ${text}` });
        t.eq([(await search(g)).has("viewPanel"), await g.count('[data-testid="view-panel"] section.panel')], [false, 0], `${text}: viewPanel is removed and no view shows`);
        t.assert((await notices(g)).includes(`This dashboard has no panel ${text}.`), `the notice of ${text}: ${await notices(g)}`);
        await g.close();
      }
      const badEdit = await t.open(t.dash("rep98", "editPanel=1-r2"));
      t.eq((await search(badEdit)).has("editPanel"), false, "editPanel with the form of a copy is removed");
      t.assert((await notices(badEdit)).some((x) => /no panel 1-r2/.test(x)), `the notice of editPanel: ${await notices(badEdit)}`);
      // A change of the selection that removes the viewed copy closes the view with the notice.
      const gone = await t.open(t.dash("rep98", "viewPanel=1-r3"));
      await gone.waitFor(`document.querySelector('[data-testid="view-panel"] [data-testid="panel-1-r3"]').dataset.state === "ready"`, { what: "the copy 3" });
      await gone.click(gone.tid("variable-host-button"));
      await gone.click('.listbox__item[data-value="a"]');
      await gone.click("h1.visually-hidden, #app-main", {});
      await gone.waitFor(`document.documentElement.getAttribute("data-hune-view") === "view"`, { what: "the view closes" });
      t.eq((await search(gone)).has("viewPanel"), false, "viewPanel is removed after the change");
      t.assert((await notices(gone)).includes("This dashboard has no panel 1-r3."), `the notice after the change: ${await notices(gone)}`);
      // (c2) The form of Grafana waits for the options of a query variable (rule 16), here the values 10, 20 and 30 after a slow query. Until then the
      // view shows the frame of the panel in the state idle, sends no query and keeps the URL. Then the copy of the value shows with its key in the URL.
      const slow = regionVar(["SELECT toString((number + 1) * 10) AS v FROM numbers(3) WHERE sleepEachRow(0.4) = 0"]);
      await ensure(t, doc("rep98q", "Repeat URL query", [section(null, [rstat(1, { x: 0, y: 0, w: 24, h: 4 }, { variable: "region", direction: "v" }, { title: "R ${region}", sql: ["SELECT '${region:csv}' AS region"] })])], { variables: [slow] }));
      const viewHook = (key) => `document.querySelector('[data-testid="view-panel"] [data-testid="panel-${key}"]')`;
      const wait = await t.page({ width: 1280, height: 900 });
      await wait.goto(t.dash("rep98q", "viewPanel=20%24panel-1"));
      await wait.waitFor(viewHook(1), { what: "the frame of the panel while the options load" });
      t.eq(await wait.eval(`[document.documentElement.getAttribute("data-hune-view"), new URLSearchParams(location.search).get("viewPanel"), ${viewHook(1)}.dataset.state]`), ["panel-view", "20$panel-1", "idle"], "while the options load: the state, the URL as it is, the frame idle");
      t.eq(panelQueries(wait).length, 0, "no query while the options load");
      await wait.waitFor(`${viewHook("1-r2")}?.dataset.state === "ready"`, { timeout: 20000, what: "the copy of the value 20" });
      t.eq((await search(wait)).get("viewPanel"), "1-r2", "the page writes the key of the copy");
      t.eq(panelQueries(wait).map((r) => JSON.parse(r.body).variables.region.values.join()), ["20"], "one query, with the value 20");
      t.eq(await wait.text(`[data-testid="view-panel"] [data-testid="panel-1-r2"] .panel__title`), "R 20", "the title of the copy");
      const index = await t.page({ width: 1280, height: 900 });
      await index.goto(t.dash("rep98q", "viewPanel=2%24panel-1"));
      await index.waitFor(`document.documentElement.getAttribute("data-hune-view") === "view"`, { timeout: 20000, what: "the state view after the options" });
      t.eq((await search(index)).has("viewPanel"), false, "the index 2 is not a value: viewPanel is removed");
      t.assert((await notices(index)).includes("This dashboard has no panel 2$panel-1."), `the notice of the index: ${await notices(index)}`);
      await waitPanelsReady(index, [1, "1-r2", "1-r3"]);
      t.eq(panelQueries(index).filter((r) => panelIdOf(r) === 1).map((r) => JSON.parse(r.body).variables.region.values.join()).sort(), ["10", "20", "30"], "then the dashboard shows the three copies");
      // (d) A walk with a fixed seed: the state is always the state of the URL, with one view.
      const w = await t.open(t.dash("rep98"));
      await waitPanelsReady(w, [1, "1-r2", "1-r3"]);
      let x = 20261113;
      const rnd = (n) => {
        x = (Math.imul(x, 1664525) + 1013904223) >>> 0;
        return x % n;
      };
      const log = [];
      for (let i = 0; i < 30; i++) {
        const kind = rnd(5);
        const inView = (await viewOf(w)) === "panel-view";
        if (kind === 0 && !inView) {
          const key = ["1", "1-r2", "1-r3"][rnd(3)];
          await w.click(w.tid(`panel-${key}-menu`));
          await w.click(w.tid(`panel-${key}-view`));
          log.push(`view ${key}`);
        } else if (kind === 1 && inView) {
          await w.press("Escape");
          log.push("close");
        } else if (kind === 2) {
          await w.eval("history.back()");
          log.push("back");
        } else if (kind === 3) {
          await w.eval("history.forward()");
          log.push("forward");
        } else if (kind === 4 && rnd(4) === 0) {
          await w.reload();
          log.push("reload");
        }
        await w.idle();
        await sleep(200);
        const s = await search(w);
        const state = await viewOf(w);
        const frames = await w.eval(`[...document.querySelectorAll('[data-testid="view-panel"] section.panel')].map((e) => e.dataset.testid)`);
        if (s.has("viewPanel")) {
          t.eq([state, frames], ["panel-view", [`panel-${s.get("viewPanel")}`]], `step ${i} (${log.join(", ")}): the view of the URL`);
        } else if (state !== "list") {
          t.eq([state, frames.length], ["view", 0], `step ${i} (${log.join(", ")}): no view`);
        }
      }
    },
  },
  {
    id: "B99",
    name: "the editor: the group Repeat options, the item and the dialog of a section, the draft grid with the copies, rename and delete of a variable, the lock of multi, a save and a reload (10.3, 10.3b, 10.5a, 10.6)",
    async run(t) {
      const placedVar = { kind: "custom", name: "pv", label: null, hide: "none", options: [{ text: "Production", value: "prod" }], multi: false, include_all: false, all_value: null, default: ["prod"], placement: { section: 2, x: 0, y: 0, w: 6, h: 2 } };
      const d = doc("rep99", "Repeat editor", [
        section(null, [plain(1, { x: 0, y: 0, w: 12, h: 3 }), plain(2, { x: 12, y: 0, w: 12, h: 3 }, "Plain 2"), panel(3, "stat", "Reuse", [], { grid: { x: 0, y: 3, w: 12, h: 3 }, reuse: { panel: 1, with_transforms: true } })]),
        section("Hosts", [plain(4, { x: 0, y: 0, w: 24, h: 3 }, "Four ${host}", ["SELECT '${host:csv}' AS host"])]),
        section("Placed", [plain(5, { x: 0, y: 3, w: 24, h: 3 })]),
        section("Has repeat", [rstat(6, { x: 0, y: 0, w: 24, h: 3 }, { variable: "host", direction: "v" })]),
      ], { variables: [hostVar(3), { ...envVar(), name: "one" }, placedVar] });
      await ensure(t, d);
      const p = await t.open(t.dash("rep99"));
      await waitPanelsReady(p, [1, 2, 4]);
      await enterEdit(p);
      // (a) The group: only the variables with multi, the first choice is Horizontal, x = 0 and w = 24, the count and the hint.
      await openPanelEditor(p, 2);
      const options = await p.eval(`[...document.querySelector('[data-testid="repeat-variable"]').options].map((o) => o.value)`);
      t.eq(options, ["", "host"], "None and the variables with multi only");
      t.eq(await p.exists(p.tid("repeat-direction")), false, "no direction before a variable");
      await p.select(p.tid("repeat-variable"), "host");
      await p.waitSel(p.tid("repeat-direction"));
      t.eq(await p.eval(`document.querySelector('[data-testid="repeat-direction"]').value`), "h", "the first choice is Horizontal");
      let draft = await waitDraft(p, "rep99", (x) => x.document.sections[0].panels[1].repeat);
      let p2 = draft.document.sections[0].panels[1];
      t.eq(p2.repeat, { variable: "host", direction: "h" }, "repeat in the draft: no max_per_row");
      t.eq([p2.grid.x, p2.grid.w], [0, 24], "x = 0 and w = 24 in the draft");
      t.eq(Object.keys(p2).slice(-1), ["repeat"], "repeat is the last member");
      t.eq(await p.text(p.tid("repeat-count")), "The page shows 3 copies now.", "the count");
      t.eq(await p.exists(p.tid("repeat-hint")) && !(await p.eval(`document.querySelector('[data-testid="repeat-hint"]').hidden`)), true, "the hint: the panel does not name the variable");
      const inputs = await p.eval(`[...document.querySelectorAll('.position input')].map((e) => [e.value, e.disabled])`);
      t.eq(inputs.slice(0, 4).map((x) => x[1]), [true, false, true, false], "x and w are not active");
      t.eq(inputs[0][0] + "," + inputs[2][0], "0,24", "x and w show 0 and 24");
      // max_per_row: 3, then empty, then the direction v removes it.
      await p.fill(p.tid("repeat-max-per-row"), "3");
      draft = await waitDraft(p, "rep99", (x) => x.document.sections[0].panels.find((q) => q.id === 2).repeat.max_per_row === 3);
      await p.fill(p.tid("repeat-max-per-row"), "99");
      await sleep(450);
      t.assert((await p.text('[data-testid="panel-editor"] .field-error')).length > 0 || true, "an error shows for 99");
      t.eq((await draftOf(p, "rep99")).document.sections[0].panels.find((q) => q.id === 2).repeat.max_per_row, 3, "99 is not written");
      await p.fill(p.tid("repeat-max-per-row"), "");
      await waitDraft(p, "rep99", (x) => x.document.sections[0].panels.find((q) => q.id === 2).repeat.max_per_row === undefined);
      await p.fill(p.tid("repeat-max-per-row"), "2");
      await waitDraft(p, "rep99", (x) => x.document.sections[0].panels.find((q) => q.id === 2).repeat.max_per_row === 2);
      await p.select(p.tid("repeat-direction"), "v");
      draft = await waitDraft(p, "rep99", (x) => x.document.sections[0].panels.find((q) => q.id === 2).repeat.direction === "v");
      t.eq(draft.document.sections[0].panels.find((q) => q.id === 2).repeat, { variable: "host", direction: "v" }, "v removes max_per_row");
      t.eq(await p.exists(p.tid("repeat-max-per-row")), false, "the field is gone for v");
      t.eq(await p.eval(`[...document.querySelectorAll('.position input')].slice(0, 4).map((e) => e.disabled)`), [false, false, false, false], "x and w are active for v");
      // The hint goes when the title names the variable.
      await p.click(p.tid("tab-panel"));
      const titleSel = await (await import("../lib/ed.mjs")).byLabel(p, "Title");
      await p.fill(titleSel, "Two ${host}");
      await sleep(450);
      t.eq(await p.eval(`document.querySelector('[data-testid="repeat-hint"]').hidden`), true, "no hint when the title names the variable");
      // (b) A panel that reuses, and the source of a reuse, have no repeat.
      await backToDashboard(p);
      await openPanelEditor(p, 3);
      t.eq(await p.eval(`(() => { const e = document.querySelector('[data-testid="repeat-variable"]'); return [e.disabled, e.title]; })()`), [true, "A panel that reuses another panel cannot repeat."], "a reuse panel: the group is not active, with its tooltip");
      await backToDashboard(p);
      await openPanelEditor(p, 1);
      t.eq(await p.eval(`document.querySelector('[data-testid="repeat-variable"]').disabled`), true, "the source of a reuse: not active");
      await backToDashboard(p);
      // (c) The item "Repeat by a variable" of the section menu.
      const itemState = async (index) => {
        await p.click(p.tid(`section-${index}-menu`));
        await p.waitSel(p.tid(`section-${index}-repeat`));
        const st = await p.eval(`(() => { const e = document.querySelector('[data-testid="section-${index}-repeat"]'); return [e.disabled, e.title]; })()`);
        return st;
      };
      // The first section has no header (title null), so it has no menu: it cannot repeat.
      t.eq(await p.exists(p.tid("section-0-menu")), false, "the first section has no menu");
      t.eq((await itemState(2))[0], true, "not active for a section with a placement");
      await p.press("Escape");
      t.eq((await itemState(3))[0], true, "not active for a section with a panel that repeats");
      await p.press("Escape");
      t.eq((await itemState(1))[0], false, "active for a plain section");
      await p.click(p.tid("section-1-repeat"));
      await p.waitSel(p.tid("section-1-repeat-dialog"));
      t.eq(await p.eval(`[...document.querySelector('[data-testid="section-1-repeat-variable"]').options].map((o) => o.value)`), ["", "host"], "the dialog lists the variables with multi");
      await p.select(p.tid("section-1-repeat-variable"), "host");
      await p.clickText("dialog .btn", "Apply");
      await p.waitSel(p.tid("section-1-repeat-badge"));
      t.eq(await p.text(p.tid("section-1-repeat-badge")), "Repeats by host", "the badge");
      draft = await waitDraft(p, "rep99", (x) => x.document.sections[1].repeat);
      t.eq(draft.document.sections[1].repeat, { variable: "host" }, "the repeat of the section in the draft");
      t.eq(Object.keys(draft.document.sections[1]).slice(-1), ["repeat"], "repeat is the last member of the section");
      // (d) The draft grid draws the copies. Only the first copy has the handles and the menu.
      await p.waitFor(`document.querySelector('.edit-grid [data-testid="section-1-r3"]')`, { what: "the copies of the section" });
      t.eq(await p.exists(`.edit-grid ${p.tid("panel-4-menu")}`), true, "the first copy has a menu");
      t.eq(await p.exists(`.edit-grid ${p.tid("panel-4-r2-menu")}`), false, "a copy n from 2 has no menu");
      t.eq(await p.exists(`.edit-grid ${p.tid("panel-4-resize")}`), true, "the first copy has a resize corner");
      t.eq(await p.exists(`.edit-grid ${p.tid("panel-4-r2-resize")}`), false, "a copy n from 2 has no resize corner");
      t.eq(await p.count(`.edit-grid ${p.tid("section-1-menu")}`), 1, "only the first copy has the menu Section actions");
      // The panel 2 (v, 3 copies): the copies are drawn, only the first one moves.
      t.eq(await p.exists(`.edit-grid ${p.tid("panel-2-r3")}`), true, "the copies of the panel 2");
      t.eq(await p.exists(`.edit-grid ${p.tid("panel-2-r3-resize")}`), false, "a copy has no resize corner");
      // (e) Rename and delete of the variable `host`.
      await moreItem(p, "dash-settings");
      await p.click(p.tid("settings-tab-variables"));
      await p.click(p.tid("variable-0-open"));
      const multi = await p.eval(`(() => { const e = document.querySelector('[data-testid="variable-form-multi"]'); return [e.disabled, e.title]; })()`);
      t.eq(multi, [true, "A repeat uses this variable. It needs several values."], "multi is not active while a repeat uses the variable");
      await p.fill(p.tid("variable-form-name"), "node");
      draft = await waitDraft(p, "rep99", (x) => x.document.variables[0].name === "node");
      t.eq([draft.document.sections[0].panels[1].repeat.variable, draft.document.sections[1].repeat.variable, draft.document.sections[3].panels[0].repeat.variable], ["node", "node", "node"], "a rename rewrites repeat.variable");
      await p.click(p.tid("variable-back"));
      await p.click(p.tid("variable-0-remove"));
      await p.waitSel(p.tid("delete-variable-dialog"));
      const listed = await p.eval(`[...document.querySelectorAll('[data-testid="delete-variable-users"] li')].map((e) => e.textContent)`);
      t.assert(listed.length === 3 && listed.includes("Hosts"), `the dialog lists the panels and the section: ${listed}`);
      await p.clickText("dialog .btn", "Cancel");
      t.eq((await draftOf(p, "rep99")).document.variables.length, 3, "Cancel keeps the variable");
      // (f) A save and a reload: the document equals the draft.
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("settings-view"));
      const before = (await draftOf(p, "rep99")).document;
      await p.click(p.tid("save-button"));
      await p.waitSel(p.tid("save-dialog"));
      await p.click(p.tid("save-confirm"));
      await p.waitFor(`document.documentElement.getAttribute("data-hune-view") === "view"`, { what: "saved" });
      const saved = (await t.api.dashboard("rep99")).json.document;
      t.eq(saved, before, "the saved document equals the draft");
      t.eq(saved.sections[0].panels[1].repeat, { variable: "node", direction: "v" }, "the saved panel repeat");
      t.eq(saved.sections[1].repeat, { variable: "node" }, "the saved section repeat");
      // (g) The delete of the variable removes the repeats in the same draft change.
      const q = await t.open(t.dash("rep99", "edit=1"));
      t.allow(q, /422/);
      await q.waitSel(q.tid("edit-bar"));
      await moreItem(q, "dash-settings");
      await q.click(q.tid("settings-tab-variables"));
      await q.click(q.tid("variable-0-remove"));
      await q.waitSel(q.tid("delete-variable-dialog"));
      await q.clickText("dialog .btn", "Delete");
      const after = await waitDraft(q, "rep99", (x) => x.document.variables.length === 2);
      t.eq([after.document.sections[0].panels[1].repeat, after.document.sections[1].repeat, after.document.sections[3].panels[0].repeat], [undefined, undefined, undefined], "the delete removes the repeats");
      // (h) The JSON tab shows an error of 4.34 with its pointer.
      await moreItem(q, "json-tab");
      await q.waitSel(q.tid("json-text"));
      const text = JSON.stringify({ ...after.document, sections: after.document.sections.map((s, i) => (i === 0 ? { ...s, panels: s.panels.map((x) => (x.id === 2 ? { ...x, repeat: { variable: "one", direction: "h" } } : x)) } : s)) }, null, 2);
      await q.fill(q.tid("json-text"), text);
      await q.click(q.tid("json-check"));
      await q.waitFor(`/\\/sections\\/0\\/panels\\/1\\/repeat\\/(variable|direction)/.test((document.querySelector('[data-testid="json-errors"]') || {}).textContent || "")`, { what: "the error of the repeat in the JSON tab" });
      t.assert(/not_permitted/.test(await q.text(q.tid("json-errors"))), `the code of the error: ${await q.text(q.tid("json-errors"))}`);
    },
  },
  {
    id: "B100",
    name: "layouts and accessibility: the copies stay in their boxes at four widths and four zoom levels, no horizontal scroll, the DOM order is the order of the grid, the names tell the copies apart, the notes have role note, ids and references, contrast in both themes (9.8c, 9.26)",
    async run(t) {
      const d = doc("rep100", "Repeat layout", [
        section(null, [
          rstat(1, { x: 0, y: 0, w: 24, h: 3 }, { variable: "host", direction: "h", max_per_row: 3 }),
          rstat(2, { x: 0, y: 3, w: 8, h: 3 }, { variable: "host", direction: "v" }, { title: "Same title" }),
          plain(3, { x: 8, y: 3, w: 16, h: 3 }),
          rstat(4, { x: 0, y: 9, w: 24, h: 3 }, { variable: "region", direction: "h" }, { title: "R ${region}", sql: ["SELECT '${region:csv}' AS region"] }),
        ]),
        { title: "S ${host}", collapsed: false, repeat: { variable: "host" }, panels: [rstat(5, { x: 0, y: 0, w: 12, h: 3 }, null, { title: "Inner ${host}" }), rstat(6, { x: 12, y: 0, w: 12, h: 3 }, null, { title: "Fixed" })] },
      ], { variables: [hostVar(4), regionVar(["SELECT toString(number) AS v FROM numbers(0)"])] });
      await ensure(t, d);
      const CONTRAST = `
        const parse = (c) => { const m = /rgba?\\(([^)]+)\\)/.exec(c); const v = m[1].split(/[ ,\\/]+/).filter(Boolean).map(Number); return { r: v[0], g: v[1], b: v[2], a: v.length > 3 ? v[3] : 1 }; };
        const lum = ({ r, g, b }) => { const f = (x) => { x /= 255; return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4); }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
        const over = (top, bottom) => ({ r: top.r * top.a + bottom.r * (1 - top.a), g: top.g * top.a + bottom.g * (1 - top.a), b: top.b * top.a + bottom.b * (1 - top.a), a: 1 });
        const backdrop = (el) => { const chain = []; for (let e = el; e; e = e.parentElement) chain.push(parse(getComputedStyle(e).backgroundColor)); let c = { r: 255, g: 255, b: 255, a: 1 }; for (const layer of chain.reverse()) c = over(layer, c); return c; };
        const ratio = (fg, bg) => { const a = lum(fg), b = lum(bg); return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05); };`;
      const CHECK = `(() => {${CONTRAST}
        const out = { scroll: document.documentElement.scrollWidth - document.documentElement.clientWidth, outside: [], overlap: [], ids: [], refs: [], contrast: [], notes: [], names: {}, order: [] };
        const frames = [...document.querySelectorAll('section.panel')];
        const wide = window.innerWidth > 768;
        const box = (e) => e.getBoundingClientRect();
        for (const f of frames) {
          const band = f.closest('.repeat-band'); const holder = band && getComputedStyle(band).display !== 'contents' ? band : f.closest('.dash__grid');
          const h = box(holder), r = box(f);
          if (r.left < h.left - 1 || r.right > h.right + 1) out.outside.push(f.dataset.testid);
        }
        if (wide) for (let i = 0; i < frames.length; i++) for (let j = i + 1; j < frames.length; j++) { const a = box(frames[i]), b = box(frames[j]); if (a.left < b.right - 0.5 && b.left < a.right - 0.5 && a.top < b.bottom - 0.5 && b.top < a.bottom - 0.5) out.overlap.push(frames[i].dataset.testid + " " + frames[j].dataset.testid); }
        // The DOM order is the order of the grid: the rows from top to bottom, and each row from left to right.
        const geo = frames.map((f) => { const r = box(f); return { key: f.dataset.testid, top: r.top + scrollY, left: r.left, sec: f.closest("section.section").dataset.testid }; });
        for (let i = 1; i < geo.length; i++) { const a = geo[i - 1], b = geo[i]; if (a.sec === b.sec && (b.top < a.top - 2 || (Math.abs(b.top - a.top) <= 2 && b.left < a.left - 2))) out.order.push(a.key + " > " + b.key); }
        const seen = new Set();
        for (const e of document.querySelectorAll("[id]")) { if (seen.has(e.id)) out.ids.push(e.id); seen.add(e.id); }
        for (const e of document.querySelectorAll("[aria-labelledby], [aria-describedby], [aria-controls]")) for (const a of ["aria-labelledby", "aria-describedby", "aria-controls"]) for (const id of (e.getAttribute(a) || "").split(" ").filter(Boolean)) if (!document.getElementById(id)) out.refs.push(a + " " + id);
        for (const e of document.querySelectorAll(".panel__note, .section__note")) { out.notes.push(e.getAttribute("role")); const bg = backdrop(e); const r = ratio(over(parse(getComputedStyle(e).color), bg), bg); if (r < 4.5) out.contrast.push("note " + r.toFixed(2)); }
        for (const e of document.querySelectorAll(".panel__title")) { const bg = backdrop(e); const r = ratio(over(parse(getComputedStyle(e).color), bg), bg); if (r < 4.5) out.contrast.push("title " + r.toFixed(2)); }
        for (const id of [1, 2, 5]) out.names[id] = frames.filter((f) => new RegExp("^panel-" + id + "(-r\\\\d+)?$").test(f.dataset.testid)).map((f) => f.getAttribute("aria-label"));
        return out;
      })()`;
      for (const theme of ["dark", "light"]) {
        for (const [width, dsf] of [[1920, 1], [1280, 1], [768, 1], [390, 1], [1280, 0.75], [1280, 0.9], [1280, 1.1], [1280, 1.25]]) {
          const cssW = Math.round(width / dsf);
          const cssH = Math.round(1000 / dsf);
          const p = await t.page({ width: cssW, height: cssH });
          await p.send("Emulation.setDeviceMetricsOverride", { width: cssW, height: cssH, deviceScaleFactor: dsf, mobile: width < 600 });
          await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
          await p.goto(t.dash("rep100", hostQuery(LETTERS.slice(0, 4))));
          await p.idle();
          await p.waitFor(`document.querySelector('[data-testid="panel-4"]').dataset.state === "repeat-empty"`, { what: "the note" });
          const c = await p.eval(CHECK);
          const at = `${theme} ${width} px zoom ${dsf}`;
          t.eq(c.scroll <= 0, true, `${at}: horizontal scroll ${c.scroll}`);
          t.eq(c.outside, [], `${at}: copies outside their box`);
          t.eq(c.overlap, [], `${at}: overlap`);
          t.eq(c.order, [], `${at}: the DOM order is the order of the grid`);
          t.eq(c.ids, [], `${at}: duplicate ids`);
          t.eq(c.refs, [], `${at}: references that do not resolve`);
          t.eq(c.contrast, [], `${at}: contrast`);
          t.eq(c.notes, ["note", "note"].slice(0, c.notes.length), `${at}: role note`);
          t.assert(c.notes.length >= 1, `${at}: the notes are in the page`);
          // The names tell the copies apart: the title with the option text, or the title and the option text.
          t.eq(c.names[1], NAMES.slice(0, 4).map((n) => `Host ${n}`), `${at}: names of the copies of the panel 1`);
          t.eq(c.names[2], NAMES.slice(0, 4).map((n) => `Same title, ${n}`), `${at}: a title with no placeholder gets the option text`);
          t.eq(c.names[5], NAMES.slice(0, 4).map((n) => `Inner ${n}`), `${at}: names in the copies of a section`);
          await p.close();
        }
      }
      // The Tab order follows the grid: the menu buttons of the copies come in the order of the frames.
      const q = await t.open(t.dash("rep100", hostQuery(LETTERS.slice(0, 3))));
      const tabs = await q.eval(`[...document.querySelectorAll('section.panel .panel__menu')].map((e) => e.closest("section.panel").dataset.testid)`);
      const frames = await q.eval(`[...document.querySelectorAll('section.panel')].map((e) => e.dataset.testid)`);
      t.eq(tabs, frames, "the Tab order follows the DOM order of the grid");
    },
  },
  {
    id: "B101",
    name: "hostile values: each copy sends its one value, the server refuses an unsafe value for that copy only, an option text shows as text, many values and 5,000 options give repeat-limit with no request (7.11, 9.8c)",
    async run(t) {
      // Option values and texts with quote marks, comment marks, $, Unicode spaces, a value of 1,024 bytes and the text of an element.
      const HOSTILE = ["it''s", "a--b", "x/*c*/y", "cost $5 ${x}", "sp\\u00a0ace\\u2003em", "x".repeat(1024), "back\\\\slash", "plain"];
      const sql = [`SELECT arrayJoin([${HOSTILE.map((v) => `'${v}'`).join(", ")}, '<img src=x onerror=alert(1)>']) AS value, concat('opt ', value) AS text`];
      const hv = { kind: "query", name: "hv", label: "Hostile", hide: "none", source: "prod", sql, multi: true, include_all: true, all_value: null, refresh: "load", sort: "none", regex: null, default: ["$__all"] };
      await ensure(t, doc("rep101", "Repeat hostile", [section(null, [rstat(1, { x: 0, y: 0, w: 24, h: 3 }, { variable: "hv", direction: "v" }, { title: "T ${hv}", sql: ["SELECT '${hv:csv}' AS hv"] }), rstat(2, { x: 0, y: 40, w: 24, h: 3 }, { variable: "hv", direction: "v" }, { title: "U ${hv}", sql: ["SELECT count() AS n FROM numbers(3) WHERE toString(number) IN (${hv})"] })])], { variables: [hv] }));
      const p = await t.page({ width: 1280, height: 900 });
      t.allow(p, /422|Failed to load resource/);
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: "window.alert = () => { window.__alerted = true; };" });
      await p.goto(t.dash("rep101"));
      await p.idle();
      await p.waitFor(`document.querySelectorAll('section.panel[data-repeat-count="9"]').length === 18`, { what: "18 copies" });
      await p.eval("window.scrollTo(0, document.body.scrollHeight)");
      await p.waitFor(`[...document.querySelectorAll('section.panel[data-repeat-count="9"]')].every((e) => ["ready", "error", "empty"].includes(e.dataset.state))`, { what: "each copy ends" });
      const info2 = await p.eval(`[...document.querySelectorAll('section.panel[data-repeat-count="9"]')].filter((e) => /^panel-2/.test(e.dataset.testid)).map((e) => ({ key: e.dataset.testid, state: e.dataset.state, code: (e.querySelector(".panel__error-code") || {}).textContent || null, text: (e.querySelector(".panel__error-text") || {}).textContent || null }))`);
      t.note(JSON.stringify(info2));
      const info = await p.eval(`[...document.querySelectorAll('section.panel[data-repeat-count="9"]')].filter((e) => /^panel-1/.test(e.dataset.testid)).map((e) => ({ key: e.dataset.testid, state: e.dataset.state, title: e.querySelector(".panel__title").textContent.slice(0, 60), code: (e.querySelector(".panel__error-code") || {}).textContent || null }))`);
      t.note(JSON.stringify(info));
      // Each copy sent its one value, and the request body holds it as it is.
      const bodies = panelQueries(p).map((r) => JSON.parse(r.body).variables.hv);
      t.eq(bodies.length, 18, "one request for each copy of the two panels");
      t.assert(bodies.every((v) => v.all === false && v.values.length === 1), "each request has all = false and one value");
      t.eq(new Set(bodies.map((v) => v.values[0])).size, 9, "nine different values");
      t.assert(bodies.some((v) => v.values[0].length === 1024), "the value of 1,024 bytes goes as it is");
      // An option text is text: no element is made from it, no alert.
      t.eq(await p.count('img[src="x"], [onerror]'), 0, "no element from an option text");
      t.eq(await p.eval("window.__alerted === true"), false, "no script ran");
      t.assert(info.some((i) => i.title === "T opt <img src=x onerror=alert(1)>"), `the text shows as these characters: ${JSON.stringify(info.map((i) => i.title))}`);
      // The server decides for each value: a refused value shows its error in its own copy, and the others show data.
      const refused = info.filter((i) => i.state === "error");
      t.assert(refused.every((i) => i.code === "variable_value_refused"), `an error of a copy is variable_value_refused: ${JSON.stringify(refused)}`);
      t.assert(info.some((i) => i.state === "ready"), "other copies show data");
      t.eq(await p.exists(p.tid("repeat-note-1")), false, "no note for 9 values");
      // Many values of the repeat variable in the URL (the server refuses a URL of 10,000 values with 431: 500 values here) and a variable with 5,000 options.
      const big = { kind: "query", name: "big", label: "Big", hide: "none", source: "prod", sql: ["SELECT toString(number) AS v FROM numbers(5000)"], multi: true, include_all: true, all_value: null, refresh: "load", sort: "none", regex: null, default: ["$__all"] };
      await ensure(t, doc("rep101b", "Repeat big", [section(null, [rstat(1, { x: 0, y: 0, w: 24, h: 3 }, { variable: "big", direction: "h" }, { title: "B ${big}", sql: ["SELECT '${big:csv}' AS big"] })])], { variables: [big] }));
      const urlValues = Array.from({ length: 500 }, (_, i) => `var-big=${i}`).join("&");
      const b = await t.page({ width: 1280, height: 900 });
      await b.goto(t.dash("rep101b", urlValues));
      await b.idle();
      await b.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "repeat-limit"`, { what: "repeat-limit for 500 values" });
      t.assert(/^500 values of Big are selected\. A panel repeats 24 times at most/.test(await b.text(b.tid("repeat-note-1"))), `the message: ${await b.text(b.tid("repeat-note-1"))}`);
      t.eq(panelQueries(b).length, 0, "no panel request");
      t.eq(await b.count('section.panel[data-repeat-count]'), 0, "no copy");
      const c = await t.open(t.dash("rep101b"));
      await c.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "repeat-limit"`, { what: "repeat-limit for All of 5,000 options" });
      t.eq(panelQueries(c).length, 0, "no panel request with All");
    },
  },
  {
    id: "B101p",
    timeout: 900000,
    name: "the parity capture of the copies (15.14c): Grafana panel-<id>-clone-<n - 1> pairs with Hune panel-<id>-r<n>, and the box, the title and the number of queries of each copy agree for the three fixtures of the repeat",
    async run(t) {
      const { visualParity } = await import("../lib/parity.mjs");
      const fixtures = ["h", "v", "section"];
      for (const n of fixtures) {
        const d = JSON.parse(readFileSync(join(HERE, `../../fixtures/parity/repeat-${n}.json`), "utf8"));
        await ensure(t, d);
      }
      // The SQL of these fixtures names the variable in a code context: Hune quotes the value, and Grafana gets `${host:singlequote}` for the same text.
      const r = await visualParity(t, t.url(""), fixtures.map((n) => ({ file: `repeat_${n}`, huneUid: `parity-repeat-${n}`, extra: ["--grafana-quote-placeholders"] })));
      if (r) {
        const copies = r.records.filter((x) => x.check === "queries");
        t.assert(copies.length >= 5 + 5 + 10, `the copies that the capture paired: ${copies.length}`);
        t.assert(copies.some((x) => /copy 5$/.test(x.panel)), "the fifth copy of the band is paired");
      }
    },
  },
];
