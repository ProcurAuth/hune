// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B190 to B199 (4.10a, 4.10b, 7.4a, 7.7, 9.5, 9.10, 9.13, 10.3b, 15.6): the variable kinds `interval` and `constant` (F27).
// B190: an interval control (the options with "Auto" first, a choice, the URL, the keys, a reload).
// B191: "Auto" in the SQL of two panels: the text follows the time range and is the same for both panels.
// B192: an interval with a placement (the control is in the grid, the bar does not show it).
// B193: a constant (no control, no URL value, the URL value does not change the SQL, the server refuses another value).
// B194: the titles `${bucket}` and `${k}`.
// B195: the editor (the two kinds, the list, the fields of Auto, the value, an error, `hide` of a constant, the JSON).
// B196: the import of a Grafana dashboard with an interval list and a constant (15.6).
// B197: a URL value that is not an item is dropped with the notice of 9.10.
// B198: hostile texts as items and as a value show as text, and no script runs.
// B199: a dashboard with all the kinds: a change of the interval runs only the panels that name it.
import { doc, panel, section } from "../lib/docs.mjs";
import { draftOf, enterEdit, moreItem, waitDraft } from "../lib/ed.mjs";
import { panelQueries, sleep, waitPanelsReady } from "../lib/h.mjs";
import { BINARY, ROOT, SEED_FROM } from "../lib/env.mjs";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";

const HOUR = 3600000;
const ITEMS = ["1m", "10m", "1h", "1d"];
const iv = (name, o = {}) => ({
  kind: "interval", name, label: o.label ?? null, hide: "none", intervals: o.intervals ?? ITEMS, auto: !!o.auto, auto_count: 30, auto_min: "10s", default: o.default ?? null,
  ...(o.placement ? { placement: o.placement } : {}),
});
const kv = (name, value) => ({ kind: "constant", name, label: null, hide: "variable", value });
const table = (id, sql, o = {}) => panel(id, "table", o.title ?? `Rows ${id}`, sql, { grid: o.grid || { x: 0, y: (id - 1) * 6, w: 24, h: 6 } });
// A single value goes in a string (7.8).
const show = (marker, ...names) => [`SELECT ${names.map((n) => `'\${${n}}' AS ${n}`).join(", ")}, '${marker}' AS m`];

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const sel = (n) => `[data-testid="variable-${n}"]`;
const btnText = (p, n) => p.text(`${p.tid(`variable-${n}-button`)} .select__value`);
const urlVars = async (p, n) => new URLSearchParams(await p.eval("location.search")).getAll(`var-${n}`);
const notices = (p) => p.eval(`[...document.querySelectorAll('[data-notice="variable"]')].map((e) => ({ key: e.dataset.noticeKey, text: e.querySelector(".notice__text").textContent }))`);
const rowsOf = (p, n) => p.eval(`[...document.querySelectorAll('${sel(n)} .listbox li[role="option"]')].filter((e) => !e.hidden).map((e) => ({ v: e.dataset.value ?? null, text: e.textContent }))`);
async function openPopup(p, n) {
  await p.click(p.tid(`variable-${n}-button`));
  await p.waitSel(`${sel(n)} .select__popup`);
}
const cell = (p, id) => p.text(`[data-testid="table-${id}"]`);
const lastBody = (p, id) => {
  const list = panelQueries(p).filter((r) => r.url.endsWith(`/panels/${id}/query`));
  return list.length ? JSON.parse(list[list.length - 1].body) : null;
};
const queriesOf = (p, id) => panelQueries(p).filter((r) => r.url.endsWith(`/panels/${id}/query`)).length;
/** The URL of a dashboard with an explicit range in milliseconds. */
const dashAt = (t, uid, from, to, extra = "") => t.url(`d/${uid}?from=${from}&to=${to}${extra ? `&${extra}` : ""}`);
const sourceQueries = async (t, marker) => {
  await t.ch.query("SYSTEM FLUSH LOGS");
  return Number((await t.ch.query(`SELECT count() FROM system.query_log WHERE type = 'QueryFinish' AND query LIKE '%${marker}%' AND query NOT LIKE '%query_log%'`)).trim());
};

export const cases = [
  {
    id: "B190",
    name: "an interval control: the options with Auto first, a choice, the URL, the keys, a reload keeps the choice",
    async run(t) {
      const d = doc("iv190", "Interval control", [section(null, [table(1, show("B190", "bucket"))])], { variables: [iv("bucket", { auto: true, label: "Bucket" })] });
      await ensure(t, d);
      const p = await t.open(t.dash("iv190"));
      await waitPanelsReady(p, [1]);
      t.eq(await btnText(p, "bucket"), "Auto", "the first option is the choice with no default");
      t.eq(await p.eval(`document.querySelector('${sel("bucket")} .var__label').textContent`), "Bucket", "the label");
      await openPopup(p, "bucket");
      const rows = await rowsOf(p, "bucket");
      t.eq(rows.map((r) => [r.v, r.text]), [["$__auto", "Auto"], ["1m", "1m"], ["10m", "10m"], ["1h", "1h"], ["1d", "1d"]], "Auto first, then the items in the order of the document");
      t.eq(await p.count(`${sel("bucket")} .select__all`), 0, "no All row and no list of selected values");
      await p.click(`${sel("bucket")} .listbox li[data-value="1h"]`);
      await p.waitFor(`document.querySelector('${sel("bucket")} .select__popup') === null`, { what: "the popup closes after one choice" });
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-bucket").join() === "1h"`, { what: "the URL" });
      t.eq(await btnText(p, "bucket"), "1h", "the choice");
      await p.waitFor(`document.querySelector('[data-testid="table-1"]').textContent.includes("1h")`, { what: "the panel runs with the choice" });
      t.eq(lastBody(p, 1).variables.bucket, { all: false, values: ["1h"] }, "the request holds the item");
      // The keys: the arrow keys move through the rows, Enter chooses.
      await openPopup(p, "bucket");
      await p.press("ArrowDown");
      await p.press("ArrowDown");
      await p.press("ArrowDown");
      await p.press("Enter");
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-bucket").join() === "10m"`, { what: "the URL after the keys" });
      t.eq(await btnText(p, "bucket"), "10m", "the third row by the keys");
      await openPopup(p, "bucket");
      await p.press("Escape");
      t.eq(await p.count(`${sel("bucket")} .select__popup`), 0, "Escape closes the popup");
      t.eq(await btnText(p, "bucket"), "10m", "Escape changes nothing");
      // Auto in the URL, and a reload.
      await openPopup(p, "bucket");
      await p.click(`${sel("bucket")} .listbox li[data-value="$__auto"]`);
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-bucket").join() === "$__auto"`, { what: "the URL for Auto" });
      await p.reload();
      await p.idle();
      await waitPanelsReady(p, [1]);
      t.eq(await btnText(p, "bucket"), "Auto", "a reload keeps Auto");
      t.eq((await notices(p)).length, 0, "no notice");
    },
  },

  {
    id: "B191",
    name: "Auto in the SQL: the text follows the time range (6 h gives 10m, 5 min gives 10s), and two panels get the same text",
    async run(t) {
      const d = doc("iv191", "Auto text", [section(null, [table(1, show("B191-a", "bucket")), table(2, show("B191-b", "bucket"))])], { variables: [iv("bucket", { auto: true, default: "$__auto" })] });
      await ensure(t, d);
      for (const [range, expected] of [[6 * HOUR, "10m"], [300000, "10s"], [30 * 24 * HOUR, "1d"]]) {
        const p = await t.open(dashAt(t, "iv191", SEED_FROM, SEED_FROM + range));
        await waitPanelsReady(p, [1, 2]);
        t.eq(await btnText(p, "bucket"), "Auto", "the control shows Auto");
        t.assert((await cell(p, 1)).includes(expected), `panel 1, range ${range} ms: ${await cell(p, 1)}`);
        t.assert((await cell(p, 2)).includes(expected), `panel 2, range ${range} ms: ${await cell(p, 2)}`);
        t.eq(lastBody(p, 1).variables.bucket, { all: false, values: ["$__auto"] }, "the page sends the selection, never the text");
        t.assert(!(await cell(p, 1)).includes("$__auto"), "the SQL never holds the value $__auto");
      }
    },
  },

  {
    id: "B192",
    name: "an interval with a placement: the control is in the grid, and the bar does not show it",
    async run(t) {
      const placement = { section: 0, x: 0, y: 0, w: 12, h: 2 };
      const d = doc("iv192", "Placed interval", [section(null, [table(1, show("B192", "bucket"), { grid: { x: 0, y: 2, w: 24, h: 6 } })])], { variables: [iv("bucket", { auto: true, placement })] });
      await ensure(t, d);
      const p = await t.open(t.dash("iv192"));
      await waitPanelsReady(p, [1]);
      t.eq(await p.count(`[data-testid="placement-bucket"] ${sel("bucket")}`), 1, "the control is in the placed frame");
      t.eq(await p.count(`[data-testid="variables-bar"] ${sel("bucket")}`), 0, "the bar does not hold it");
      t.eq(await p.text(`[data-testid="placement-bucket"] .select__value`), "Auto", "the control shows Auto");
      await p.click(`[data-testid="placement-bucket"] ${p.tid("variable-bucket-button")}`);
      await p.waitSel(`${sel("bucket")} .select__popup`);
      await p.click(`${sel("bucket")} .listbox li[data-value="1d"]`);
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-bucket").join() === "1d"`, { what: "the URL" });
      await p.waitFor(`document.querySelector('[data-testid="table-1"]').textContent.includes("1d")`, { what: "the panel runs" });
    },
  },

  {
    id: "B193",
    name: "a constant: no control, no URL value, the URL value does not change the SQL, the server refuses another value with no source query",
    async run(t) {
      const value = "B193-value";
      const d = doc("iv193", "Constant", [section(null, [table(1, show("B193", "k"))])], { variables: [iv("bucket", { default: "1m" }), kv("k", value)] });
      await ensure(t, d);
      let p = await t.open(t.dash("iv193", "var-k=zzz"));
      await waitPanelsReady(p, [1]);
      t.eq(await p.count(sel("k")), 0, "no control for a constant");
      t.eq(await p.count(`[data-testid="variables-bar"] ${sel("bucket")}`), 1, "the bar holds the interval");
      t.assert((await cell(p, 1)).includes(value), "the SQL has the value of the document: " + (await cell(p, 1)));
      t.assert(!(await cell(p, 1)).includes("zzz"), "the URL value is not used");
      t.eq(lastBody(p, 1).variables.k, { all: false, values: [value] }, "the request holds the value of the document");
      // A change of another variable writes the URL: it never has a value of the constant.
      p = await t.open(t.dash("iv193"));
      await waitPanelsReady(p, [1]);
      await openPopup(p, "bucket");
      await p.click(`${sel("bucket")} .listbox li[data-value="1h"]`);
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-bucket").join() === "1h"`, { what: "the URL" });
      t.eq(await urlVars(p, "k"), [], "no var-k in the URL");
      // The server: no member and the equal member give 200. Another member, All and two values give 422 with no source query.
      const body = lastBody(p, 1);
      const call = (member) => t.api.call("POST", "api/dashboards/iv193/panels/1/query", { body: { ...body, variables: member === undefined ? { bucket: body.variables.bucket } : { ...body.variables, k: member } } });
      t.eq((await call(undefined)).status, 200, "no member");
      t.eq((await call({ all: false, values: [value] })).status, 200, "the equal member");
      const before = await sourceQueries(t, "B193");
      for (const member of [{ all: false, values: ["zzz"] }, { all: true, values: [] }, { all: false, values: [value, value] }, { all: false, values: [value.toUpperCase()] }, { all: false, values: [] }]) {
        const r = await call(member);
        t.eq(r.status, 422, `422 for ${JSON.stringify(member)}: ${r.text.slice(0, 200)}`);
        t.assert(r.text.includes("variable_value_refused"), "the code: " + r.text.slice(0, 200));
      }
      t.eq(await sourceQueries(t, "B193"), before, "no source query for a refused value");
    },
  },

  {
    id: "B194",
    name: "the titles: a placeholder of an interval shows the option text (Auto for Auto), a placeholder of a constant shows its value",
    async run(t) {
      const d = doc("iv194", "Titles", [section(null, [table(1, show("B194", "bucket"), { title: "Every ${bucket} for ${k}" })])], { variables: [iv("bucket", { auto: true }), kv("k", "alpha")] });
      await ensure(t, d);
      const p = await t.open(t.dash("iv194"));
      await waitPanelsReady(p, [1]);
      const title = () => p.text('[data-testid="panel-1"] .panel__title');
      t.eq(await title(), "Every Auto for alpha", "Auto and the value");
      await openPopup(p, "bucket");
      await p.click(`${sel("bucket")} .listbox li[data-value="10m"]`);
      await p.waitFor(`document.querySelector('[data-testid="panel-1"] .panel__title').textContent === "Every 10m for alpha"`, { what: "the title follows the choice" });
    },
  },

  {
    id: "B195",
    name: "the editor: it adds each kind, edits the list, the fields of Auto and the value, shows an error of 4.10a, and keeps hide = variable for a constant; the draft agrees",
    async run(t) {
      const seed = doc("iv195", "Editor", [section(null, [table(1, ["SELECT 1 AS one"], { grid: { x: 0, y: 2, w: 12, h: 4 } })])], { variables: [] });
      await ensure(t, seed);
      const p = await t.open(t.dash("iv195"));
      await waitPanelsReady(p, [1]);
      await enterEdit(p);
      await moreItem(p, "dash-settings");
      await p.waitSel(p.tid("settings-view"));
      await p.click(p.tid("settings-tab-variables"));
      const draft = async () => (await draftOf(p, "iv195")).document.variables;
      const kinds = await p.eval(`[...document.querySelector('[data-testid="variable-add-kind"]').options].map((o) => o.value)`);
      // The kind `adhoc` joins the list (F36, 4.10c).
      t.eq(kinds, ["query", "custom", "text", "interval", "constant", "adhoc"], "the kinds of the select");
      // An interval variable.
      await p.select(p.tid("variable-add-kind"), "interval");
      await p.click(p.tid("variable-add"));
      await p.waitSel(p.tid("variable-form-intervals"));
      await waitDraft(p, "iv195", (x) => x.document.variables.length === 1);
      t.eq((await draft())[0], { kind: "interval", name: "var1", label: null, hide: "none", intervals: ITEMS, auto: false, auto_count: 30, auto_min: "10s", default: null }, "a new interval variable");
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-form-intervals"]').value`), ITEMS.join("\n"), "one item on each line");
      await p.fill(p.tid("variable-form-intervals"), "1m, 5m\n2h,1w");
      await waitDraft(p, "iv195", (x) => x.document.variables[0].intervals.join() === "1m,5m,2h,1w");
      // An item that breaks 4.10a shows its error and does not change the draft.
      await p.fill(p.tid("variable-form-intervals"), "1m, 1M");
      await sleep(600);
      t.assert((await p.text(".vform")).includes('"1M" is not an interval'), "the error: " + (await p.text(".vform")));
      t.eq((await draft())[0].intervals, ["1m", "5m", "2h", "1w"], "the draft keeps the last valid list");
      await p.fill(p.tid("variable-form-intervals"), "366d");
      await sleep(600);
      t.assert((await p.text(".vform")).includes("more than 365 days"), "the bound of 365 days");
      await p.fill(p.tid("variable-form-intervals"), "1m, 1m");
      await sleep(600);
      t.assert((await p.text(".vform")).includes("twice"), "a repeated item");
      await p.fill(p.tid("variable-form-intervals"), "1m, 5m, 2h");
      await waitDraft(p, "iv195", (x) => x.document.variables[0].intervals.join() === "1m,5m,2h");
      // Auto: the fields wake up, the default lists Auto.
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-form-auto-count"]').disabled`), true, "the step count waits for the switch");
      await p.click(p.tid("variable-form-auto"));
      await waitDraft(p, "iv195", (x) => x.document.variables[0].auto === true);
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-form-auto-count"]').disabled`), false, "the step count is active");
      await p.fill(p.tid("variable-form-auto-count"), "1000");
      await waitDraft(p, "iv195", (x) => x.document.variables[0].auto_count === 1000);
      await p.fill(p.tid("variable-form-auto-count"), "1001");
      await sleep(600);
      t.eq((await draft())[0].auto_count, 1000, "1,001 is not stored");
      await p.select(p.tid("variable-form-auto-min"), "1m");
      await waitDraft(p, "iv195", (x) => x.document.variables[0].auto_min === "1m");
      const choices = await p.eval(`[...document.querySelector('[data-testid="variable-form-interval-default"]').options].map((o) => o.value)`);
      t.eq(choices, ["", "$__auto", "1m", "5m", "2h"], "the default lists the first option, Auto and the items");
      await p.select(p.tid("variable-form-interval-default"), "$__auto");
      await waitDraft(p, "iv195", (x) => x.document.variables[0].default === "$__auto");
      // The switch off takes the default $__auto back to null, in the same change.
      await p.click(p.tid("variable-form-auto"));
      await waitDraft(p, "iv195", (x) => x.document.variables[0].auto === false && x.document.variables[0].default === null);
      t.eq(Object.keys((await draft())[0]), ["kind", "name", "label", "hide", "intervals", "auto", "auto_count", "auto_min", "default"], "the members keep the canonical order, auto_count and auto_min stay");
      await p.click(p.tid("variable-back"));
      // A constant variable.
      await p.select(p.tid("variable-add-kind"), "constant");
      await p.click(p.tid("variable-add"));
      await p.waitSel(p.tid("variable-form-value"));
      await waitDraft(p, "iv195", (x) => x.document.variables.length === 2);
      t.eq((await draft())[1], { kind: "constant", name: "var2", label: null, hide: "variable", value: "" }, "a new constant has hide = variable and an empty value");
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-form-hide"]').disabled`), true, "Hide is read only");
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-form-hide"]').selectedOptions[0].textContent`), "Variable", "Hide shows Variable");
      t.eq(await p.count(p.tid("variable-form-placed")), 0, "no placement for a constant");
      t.assert((await p.text(".vform")).includes("The viewer never sees or changes this value."), "the help text");
      await p.fill(p.tid("variable-form-value"), "a'b\"c");
      await waitDraft(p, "iv195", (x) => x.document.variables[1].value === "a'b\"c");
      await p.fill(p.tid("variable-form-value"), "x".repeat(1025));
      await sleep(600);
      t.assert((await p.text(".vform")).includes("1024 bytes at most"), "the limit of 1,024 bytes: " + (await p.text(".vform")));
      t.eq((await draft())[1].value, "a'b\"c", "the draft keeps the last valid value");
      t.eq((await draft())[1].hide, "variable", "hide stays variable");
    },
  },

  {
    id: "B196",
    name: "the import of a Grafana dashboard with an interval list 1m,1.5h,1M and a constant shows the rows of 15.6",
    async run(t) {
      const s = await t.ownServer("imp196");
      const work = mkdtempSync(join(tmpdir(), "hune-b196-"));
      const fx = join(ROOT, "tests/fixtures/grafana");
      mkdirSync(join(work, "dashboards"), { recursive: true });
      const source = JSON.parse(readFileSync(join(fx, "open_variables.json"), "utf8"));
      source.templating.list = [
        { type: "interval", name: "bucket", label: "Bucket", query: "1m,1.5h,1M", auto: true, auto_count: 30, auto_min: "90s", current: { text: "Auto", value: "$__auto_interval_bucket" }, hide: 0, options: [] },
        { type: "constant", name: "k", label: "K", query: "alpha", hide: 0, current: { text: "alpha", value: "alpha" }, options: [] },
      ];
      source.panels[0].targets[0].rawSql = "SELECT '${bucket}' AS b, '${k}' AS k";
      writeFileSync(join(work, "dashboards", `${source.uid}.json`), JSON.stringify(source));
      for (const f of ["folders.json", "health.json", "plugin-clickhouse.json"]) cpSync(join(fx, "export", f), join(work, f));
      writeFileSync(join(work, "search.json"), JSON.stringify([{ uid: source.uid, folderUid: "fe1x0abc2kq9" }]));
      const run = (extra) => spawnSync(BINARY, ["import-grafana", "--in", work, "--source-map", "clickhouse=prod", "--folders", join(fx, "export/folders.map.json"), "--report", join(work, "report"), "-c", s.configPath, ...extra], { encoding: "utf8", env: {} });
      const dry = run([]);
      let report;
      try {
        report = JSON.parse(readFileSync(join(work, "report.json"), "utf8"));
      } catch (error) {
        throw new Error(`no report: status ${dry.status}: ${dry.stdout.slice(-600)} ${dry.stderr.slice(-600)}`);
      }
      const collect = (o) => (Array.isArray(o) ? o.flatMap(collect) : o && typeof o === "object" ? Object.entries(o).flatMap(([k, v]) => (k === "items" && Array.isArray(v) ? v : collect(v))) : []);
      const items = collect(report.dashboards);
      const text = JSON.stringify(items);
      for (const word of ["1M", "auto_min", "hide"]) t.assert(text.includes(word), `the report has a row about ${word}: ${text.slice(0, 900)}`);
      const accept = join(work, "accept.txt");
      writeFileSync(accept, `# accepted for the case B196\n${[...new Set(items.map((i) => i.id))].join("\n")}\n`);
      await s.stop();
      const wrote = run(["--accept", accept, "--write"]);
      t.eq(wrote.status, 0, "the import: " + wrote.stdout.slice(-500) + wrote.stderr.slice(-300));
      await s.restart();
      const got = await (await fetch(`${s.url}api/dashboards/${source.uid}`)).json();
      const byName = Object.fromEntries(got.document.variables.map((v) => [v.name, v]));
      t.eq(byName.bucket.intervals, ["1m", "5h"], "the items that break 4.10a are removed (1.5h gives 1 and 5h, 1M is refused)");
      t.eq([byName.bucket.auto, byName.bucket.auto_count, byName.bucket.auto_min, byName.bucket.default], [true, 30, "1m", "$__auto"], "auto, the count, the minimum 90s gives 1m, the default Auto");
      t.eq([byName.k.kind, byName.k.value, byName.k.hide], ["constant", "alpha", "variable"], "the constant is always hidden");
      const p = await t.page({ width: 1280, height: 900 });
      t.allow(p, /Failed to load resource/);
      await p.goto(`${s.url}d/${source.uid}`);
      await p.idle();
      t.eq(await p.count(sel("k")), 0, "the page shows no control for the constant");
      t.eq(await btnText(p, "bucket"), "Auto", "the page shows Auto");
    },
  },

  {
    id: "B197",
    name: "a URL value that is not an item is dropped with the notice of 9.10, and the URL gets the value that the page used",
    async run(t) {
      const d = doc("iv197", "Dropped value", [section(null, [table(1, show("B197", "bucket"))])], { variables: [iv("bucket", { label: "Bucket", default: "1h" })] });
      await ensure(t, d);
      for (const bad of ["7m", "1H", "1m' OR 1=1 --", "$__auto"]) {
        const p = await t.open(t.dash("iv197", `var-bucket=${encodeURIComponent(bad)}`));
        await waitPanelsReady(p, [1]);
        t.eq(await btnText(p, "bucket"), "1h", `the default replaces ${bad}`);
        const list = await notices(p);
        t.eq(list.length, 1, `one notice for ${bad}`);
        t.eq(list[0].text, `This value is not an option of Bucket: "${bad}". The page used "1h".`, "the notice");
        t.eq(await urlVars(p, "bucket"), ["1h"], "the URL has the used value");
        t.assert((await cell(p, 1)).includes("1h") && !(await cell(p, 1)).includes(bad.slice(0, 4)), "the panel ran with the item: " + (await cell(p, 1)));
        t.eq(lastBody(p, 1).variables.bucket, { all: false, values: ["1h"] }, "the request holds the item");
      }
      // A valid item gives no notice.
      const p = await t.open(t.dash("iv197", "var-bucket=10m"));
      await waitPanelsReady(p, [1]);
      t.eq((await notices(p)).length, 0, "no notice for an item");
      t.eq(await btnText(p, "bucket"), "10m", "the item");
    },
  },

  {
    id: "B198",
    name: "hostile texts as items and as a value show as text, and no script runs; the CSP report is empty",
    async run(t) {
      const hostile = "<img src=x onerror=alert(1)>";
      const d = doc("iv198", "Hostile", [section(null, [table(1, show("B198", "k"), { title: "T ${bucket} ${k}" })])], { variables: [iv("bucket", { intervals: ["1m"], default: null }), kv("k", hostile)] });
      await ensure(t, d);
      const p = await t.open(t.dash("iv198", `var-bucket=${encodeURIComponent(hostile)}&var-k=${encodeURIComponent(hostile)}`));
      await p.eval(`window.__xss = 0; window.alert = () => { window.__xss += 1; };`);
      await waitPanelsReady(p, [1]);
      t.eq(await p.text('[data-testid="panel-1"] .panel__title'), `T 1m ${hostile}`, "the title shows the constant as text");
      t.assert((await cell(p, 1)).includes(hostile), "the table shows the value as text");
      t.eq(await p.count('[data-testid="panel-1"] img, [data-testid="variables-bar"] img'), 0, "no element made from the text");
      t.eq(await p.eval("window.__xss"), 0, "no script ran");
      t.eq((await notices(p))[0].text.includes(hostile), true, "the notice holds the dropped URL value as text");
      t.eq(await p.count('[data-testid="app-notices"] img, #app-notices img'), 0, "no element in the notice");
      // The save refuses an item that is markup.
      const bad = doc("iv198b", "Hostile item", [section(null, [table(1, ["SELECT 1 AS one"])])], { variables: [iv("bucket", { intervals: [hostile] })] });
      const r = await t.api.create("transit", bad);
      t.eq(r.status, 422, "the server refuses the item: " + r.text.slice(0, 200));
      t.assert(r.text.includes("/variables/0/intervals/0"), "the pointer: " + r.text.slice(0, 300));
    },
  },

  {
    id: "B199",
    name: "a dashboard with interval, constant, query, custom and text variables loads, and a change of the interval runs only the panels that name it",
    async run(t) {
      const query = { kind: "query", name: "q", label: null, hide: "none", source: "prod", sql: ["SELECT DISTINCT node", "FROM analytics.seed", "ORDER BY node"], multi: false, include_all: false, all_value: null, refresh: "load", sort: "alpha_asc", regex: null, default: ["v0"] };
      const custom = { kind: "custom", name: "c", label: null, hide: "none", options: [{ text: "One", value: "1" }, { text: "Two", value: "2" }], multi: false, include_all: false, all_value: null, default: ["1"] };
      const text = { kind: "text", name: "x", label: null, hide: "none", value_type: "text", default: "hello" };
      const d = doc("iv199", "All kinds", [section(null, [table(1, show("B199-1", "bucket")), table(2, show("B199-2", "q")), table(3, show("B199-3", "k")), table(4, show("B199-4", "c", "x"))])], {
        variables: [iv("bucket", { auto: true, default: "1h" }), kv("k", "const"), query, custom, text],
      });
      await ensure(t, d);
      const p = await t.open(t.dash("iv199"));
      await waitPanelsReady(p, [1, 2, 3, 4]);
      t.eq(await p.count(sel("k")), 0, "no control for the constant");
      for (const n of ["bucket", "q", "c", "x"]) t.eq(await p.count(sel(n)), 1, `the control of ${n}`);
      t.assert((await cell(p, 3)).includes("const"), "the constant in panel 3");
      t.assert((await cell(p, 1)).includes("1h"), "the interval in panel 1");
      const before = [1, 2, 3, 4].map((id) => queriesOf(p, id));
      await openPopup(p, "bucket");
      await p.click(`${sel("bucket")} .listbox li[data-value="1d"]`);
      await p.waitFor(`document.querySelector('[data-testid="table-1"]').textContent.includes("1d")`, { what: "panel 1 runs again" });
      await p.idle();
      await sleep(500);
      const after = [1, 2, 3, 4].map((id) => queriesOf(p, id));
      t.eq(after.map((n, i) => n - before[i]), [1, 0, 0, 0], "only panel 1 runs: " + JSON.stringify([before, after]));
      t.eq(lastBody(p, 3).variables.k, { all: false, values: ["const"] }, "panel 3 sends the constant");
    },
  },
];
