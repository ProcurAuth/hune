// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B160 to B169 (4.8, 4.9, 7.7, 9.10, 9.26, 10.3b, 15.6): the closed or free value of a variable (F41).
// B160: a closed query variable (a URL value that is not an option, the notice, the URL, no notice for an error or an empty list).
// B161: an open query variable with one value (the row Use, Enter, the URL, a reload, a range change, no request while typing).
// B162: an open multi variable with All (typed rows, checked rows, the order, All and a typed value exclude each other).
// B163: the combobox (the attributes, the keys, the errors, the contrast of the active row in both themes).
// B164: "No options" (the bar, a placed frame, the panel view, include_all, an open variable).
// B165: a custom variable and the server (a closed variable refuses with no source query, an open variable gives a literal, a quoted name is refused at save).
// B166: the placement and the repeat of an open variable.
// B167: the editor (the switch, the help text, the draft grid, the default, the menu Examples with the undo, "Show options").
// B168: the import of allowCustomValue.
// B169: four widths, two themes (the popup in the window, the rows of 38 px, the contrast, no horizontal scroll bar).
import { doc, panel, section } from "../lib/docs.mjs";
import { draftOf, enterEdit, moreItem, saveDialog, waitDraft } from "../lib/ed.mjs";
import { optionQueries, panelQueries, sleep, waitPanelsReady } from "../lib/h.mjs";
import { BINARY, ROOT, SEED_FROM } from "../lib/env.mjs";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SEED_SQL = ["SELECT DISTINCT node", "FROM analytics.seed", "ORDER BY node"];
const NONE_SQL = ["SELECT DISTINCT node", "FROM analytics.seed", "WHERE node = 'nothing'"];
// The range of the seed that holds one row only: a variable with the filter of time has the option v0 alone.
const TIME_SQL = ["SELECT DISTINCT node", "FROM analytics.seed", "WHERE $__timeFilter(event_time)", "ORDER BY node"];

const at = (section, x, y, w, h) => ({ section, x, y, w, h });
const withMember = (v, o) => {
  // The members follow the canonical order: `default`, then `allow_custom_value`, then `placement` (4.38).
  const { placement, ...rest } = v;
  return { ...rest, ...(o.open ? { allow_custom_value: true } : {}), ...(o.placement ? { placement: o.placement } : placement ? { placement } : {}) };
};
const qv = (name, o = {}) => withMember({ kind: "query", name, label: o.label ?? null, hide: "none", source: "prod", sql: o.sql ?? SEED_SQL, multi: !!o.multi, include_all: !!o.all, all_value: null, refresh: o.refresh ?? "load", sort: "alpha_asc", regex: null, default: o.default ?? [] }, o);
const cv = (name, o = {}) => withMember({ kind: "custom", name, label: o.label ?? null, hide: "none", options: o.options ?? [{ text: "Production", value: "prod" }, { text: "Preproduction", value: "preprod" }], multi: !!o.multi, include_all: !!o.all, all_value: null, default: o.default ?? ["prod"] }, o);
const table = (id, sql, grid = null) => panel(id, "table", `Rows ${id}`, sql, { grid: grid || { x: 0, y: (id - 1) * 6, w: 24, h: 6 } });
// A single value goes in a string. A list goes in code (7.8). A name with the prefix `*` is a list.
const inList = (names) => ["SELECT count() AS n FROM analytics.seed", `WHERE ${names.map((n) => (n.startsWith("*") ? `node IN (\${${n.slice(1)}})` : `node = '\${${n}}'`)).join(" AND ")}`];

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const sel = (n) => `[data-testid="variable-${n}"]`;
const btnText = (p, n) => p.text(`${p.tid(`variable-${n}-button`)} .select__value`);
const urlVars = async (p, n) => new URLSearchParams(await p.eval("location.search")).getAll(`var-${n}`);
const notices = (p) => p.eval(`[...document.querySelectorAll('[data-notice="variable"]')].map((e) => ({ key: e.dataset.noticeKey, text: e.querySelector(".notice__text").textContent, level: e.dataset.level }))`);
const lastBody = (p, id) => {
  const list = panelQueries(p).filter((r) => r.url.endsWith(`/panels/${id}/query`));
  return list.length ? JSON.parse(list[list.length - 1].body) : null;
};
async function openPopup(p, n) {
  await p.click(p.tid(`variable-${n}-button`));
  await p.waitSel(`${sel(n)} .select__popup`);
}
/** The visible rows of the list of a popup: value (null for the row Use), text, aria-selected, id and class. */
const rowsOf = (p, n) => p.eval(`[...document.querySelectorAll('${sel(n)} .listbox li[role="option"]')].filter((e) => !e.hidden).map((e) => ({ v: e.dataset.value ?? null, text: e.textContent, sel: e.getAttribute("aria-selected"), id: e.id, cls: e.className }))`);
const searchOf = (n) => `[data-testid="variable-${n}-search"]`;
async function type(p, n, text) {
  await p.eval(`document.querySelector('${searchOf(n)}').focus()`);
  await p.send("Input.insertText", { text });
}
const useRow = (p, n) => p.eval(`(() => { const e = document.querySelector('[data-testid="variable-${n}-use"]'); return e && !e.hidden ? e.textContent : null; })()`);
const CONTRAST = `
  const parse = (c) => { const m = /rgba?\\(([^)]+)\\)/.exec(c); const v = m[1].split(/[ ,\\/]+/).filter(Boolean).map(Number); return { r: v[0], g: v[1], b: v[2], a: v.length > 3 ? v[3] : 1 }; };
  const lum = ({ r, g, b }) => { const f = (x) => { x /= 255; return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4); }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
  const over = (top, bottom) => ({ r: top.r * top.a + bottom.r * (1 - top.a), g: top.g * top.a + bottom.g * (1 - top.a), b: top.b * top.a + bottom.b * (1 - top.a), a: 1 });
  const backdrop = (el) => { const chain = []; for (let e = el; e; e = e.parentElement) chain.push(parse(getComputedStyle(e).backgroundColor)); let c = { r: 255, g: 255, b: 255, a: 1 }; for (const layer of chain.reverse()) c = over(layer, c); return c; };
  const ratio = (fg, bg) => { const a = lum(fg), b = lum(bg); return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05); };`;

export const cases = [
  {
    id: "B160",
    name: "a closed query variable: a URL value that is not an option gives one notice and the new URL; no notice for an options error or an empty list",
    async run(t) {
      const d = doc("ov160", "Closed variables", [section(null, [table(1, inList(["one", "*many"])), table(2, inList(["ranged"])), table(3, inList(["empty"]))])], {
        variables: [qv("one", { label: "One", default: ["v1"] }), qv("many", { multi: true, default: ["v0"] }), qv("ranged", { refresh: "time_change", sql: TIME_SQL, default: ["v0"] }), qv("empty", { sql: NONE_SQL })],
      });
      await ensure(t, d);
      // A single value that is not an option: the default, one notice, the new value in the URL.
      let p = await t.open(t.dash("ov160", "var-one=zzz"));
      await waitPanelsReady(p, [1]);
      t.eq(await btnText(p, "one"), "v1", "the default");
      let list = await notices(p);
      t.eq(list.length, 1, "one notice: " + JSON.stringify(list));
      t.eq(list[0], { key: "variable-one", text: 'This value is not an option of One: "zzz". The page used "v1".', level: "info" }, "the notice");
      t.assert(await p.eval(`document.getElementById("app-notices").getAttribute("aria-live") === "polite" && document.getElementById("app-notices").getAttribute("role") === "status"`), "the area of the notices is live");
      t.eq(await urlVars(p, "one"), ["v1"], "the new value is in the URL");
      // A reload of the new URL shows no notice.
      await p.reload();
      await p.idle();
      await waitPanelsReady(p, [1]);
      t.eq((await notices(p)).length, 0, "no second notice after a reload");
      t.eq(await btnText(p, "one"), "v1", "the value after the reload");
      // A multi URL with one value that is not an option keeps the others. A list: three values, then "and <n> more".
      p = await t.open(t.dash("ov160", "var-many=v1&var-many=zzz&var-many=v2"));
      await waitPanelsReady(p, [1]);
      t.eq(await btnText(p, "many"), "v1 + v2", "the other values stay");
      t.eq((await notices(p))[0].text, 'This value is not an option of many: "zzz". The page used "v1 + v2".', "the notice of a multi variable");
      t.eq(await urlVars(p, "many"), ["v1", "v2"], "the URL of a multi variable");
      p = await t.open(t.dash("ov160", `var-many=a&var-many=${"b".repeat(45)}&var-many=c&var-many=d&var-many=e`));
      await waitPanelsReady(p, [1]);
      t.eq((await notices(p))[0].text, `These values are not an option of many: "a", "${"b".repeat(40)}...", "c" and 2 more. The page used "v0".`, "a list of five values, one cut at 40 characters");
      // A change of the range with `refresh = time_change` that removes the selected option gives the same result.
      p = await t.open(t.dash("ov160", "var-ranged=v2"));
      await waitPanelsReady(p, [1]);
      t.eq(await btnText(p, "ranged"), "v2", "v2 is an option of the full range");
      t.eq((await notices(p)).length, 0, "no notice for an option");
      await p.click(p.tid("time-picker-button"));
      await p.fill(p.tid("time-from"), String(SEED_FROM));
      await p.fill(p.tid("time-to"), String(SEED_FROM + 1000));
      await p.click(p.tid("time-apply"));
      await p.waitFor(`document.querySelectorAll('[data-notice="variable"]').length === 1`, { what: "the notice after the change of the range" });
      t.eq(await btnText(p, "ranged"), "v0", "the first option of the new range");
      t.eq((await notices(p))[0].text, 'This value is not an option of ranged: "v2". The page used "v0".', "the notice after a reload of the options");
      t.eq(await urlVars(p, "ranged"), ["v0"], "the URL after the reload of the options");
      // An empty list gives no notice. An error of the options query gives no notice.
      p = await t.open(t.dash("ov160", "var-empty=zzz"));
      await waitPanelsReady(p, [3]);
      t.eq((await notices(p)).length, 0, "no notice for an empty list");
      const bad = doc("ov160b", "Options error", [section(null, [table(1, ["SELECT 1 AS one"])])], { variables: [qv("broken", { sql: ["SELECT * FROM analytics.nothing_here"], default: ["v1"] })] });
      await ensure(t, bad);
      p = await t.open(t.dash("ov160b", "var-broken=zzz"));
      t.allow(p, /Failed to load resource/);
      await p.waitFor(`document.querySelector('${sel("broken")} .select__error') && !document.querySelector('${sel("broken")} .select__error').hidden`, { what: "the options error" });
      t.eq((await notices(p)).length, 0, "no notice for an options error");
    },
  },

  {
    id: "B161",
    name: "an open query variable: the row Use, Enter, the URL, a reload, a change of the range; no request while the person types",
    async run(t) {
      const d = doc("ov161", "Open variable", [section(null, [table(1, inList(["svc"]))])], { variables: [qv("svc", { open: true, refresh: "time_change", sql: SEED_SQL, default: ["v1"] })] });
      await ensure(t, d);
      const p = await t.open(t.dash("ov161"));
      await waitPanelsReady(p, [1]);
      t.eq(await btnText(p, "svc"), "v1", "the default");
      await openPopup(p, "svc");
      const field = await p.eval(`(() => { const e = document.querySelector('${searchOf("svc")}'); return { role: e.getAttribute("role"), focus: document.activeElement === e, ph: e.placeholder }; })()`);
      t.eq(field, { role: "combobox", focus: true, ph: "Search or type a value" }, "the field is a combobox with the focus");
      p.clearRequests();
      await type(p, "svc", "zz1");
      t.eq(await useRow(p, "svc"), 'Use "zz1"', "the row Use");
      t.eq(p.requests.filter((r) => r.method === "POST").length, 0, "no request while the person types");
      await p.press("Enter");
      await p.waitFor(`document.querySelector('${sel("svc")} .select__popup') === null`, { what: "the popup closes" });
      await waitPanelsReady(p, [1]);
      t.eq(await btnText(p, "svc"), "zz1", "the select shows the typed value");
      t.eq(await urlVars(p, "svc"), ["zz1"], "the URL holds the typed value");
      t.eq(lastBody(p, 1).variables.svc, { all: false, values: ["zz1"] }, "the panel runs with the typed value as a free value");
      // A reload keeps it, with no notice.
      await p.reload();
      await p.idle();
      await waitPanelsReady(p, [1]);
      t.eq(await btnText(p, "svc"), "zz1", "a reload keeps the typed value");
      t.eq((await notices(p)).length, 0, "no notice for a typed value");
      // A change of the range (the variable reloads its options) keeps it.
      p.clearRequests();
      await p.click(p.tid("time-picker-button"));
      await p.fill(p.tid("time-to"), String(SEED_FROM + 1000));
      await p.click(p.tid("time-apply"));
      await p.waitFor(`location.search.includes("to=${SEED_FROM + 1000}")`);
      await p.idle();
      t.assert(optionQueries(p).length >= 1, "the options reload at a change of the range");
      t.eq(await btnText(p, "svc"), "zz1", "a change of the range keeps the typed value");
      t.eq(await urlVars(p, "svc"), ["zz1"], "the URL after the change of the range");
      // The reopened popup shows the typed row first, selected.
      await openPopup(p, "svc");
      const rows = await rowsOf(p, "svc");
      t.eq([rows[0].v, rows[0].sel], ["zz1", "true"], "the typed row is the first row, selected");
      // A text that equals an option, by its value or its text, gives no row Use. A text that is typed already gives none.
      await type(p, "svc", "v0");
      t.eq(await useRow(p, "svc"), null, "no row Use for the value of an option");
      await p.fill(searchOf("svc"), "zz1");
      t.eq(await useRow(p, "svc"), null, "no row Use for a typed row");
      await p.fill(searchOf("svc"), "zz2");
      t.eq(await useRow(p, "svc"), 'Use "zz2"', "a new text gives the row Use");
      await p.fill(searchOf("svc"), "x".repeat(60));
      t.eq(await useRow(p, "svc"), `Use "${"x".repeat(40)}..."`, "the row Use cuts the text at 40 characters");
      await p.press("Escape");
      t.eq(await btnText(p, "svc"), "zz1", "Escape closes with no change");
    },
  },

  {
    id: "B162",
    name: "an open multi variable with All: typed rows are checked rows, the order, a click removes, Enter in an empty field applies",
    async run(t) {
      const d = doc("ov162", "Open multi", [section(null, [table(1, inList(["*hosts"]))])], { variables: [qv("hosts", { open: true, multi: true, all: true, default: ["$__all"] })] });
      await ensure(t, d);
      const p = await t.open(t.dash("ov162"));
      await waitPanelsReady(p, [1]);
      t.eq(await btnText(p, "hosts"), "All", "the default All");
      await openPopup(p, "hosts");
      await type(p, "hosts", "t1");
      await p.press("Enter");
      t.assert(await p.exists(`${sel("hosts")} .select__popup`), "Enter keeps the popup open in a multi variable");
      t.eq(await p.eval(`document.querySelector('${searchOf("hosts")}').value`), "", "the field is empty again");
      let rows = await rowsOf(p, "hosts");
      t.eq(rows.map((r) => [r.v, r.sel]), [["t1", "true"], ["$__all", "false"], ["v0", "false"], ["v1", "false"], ["v2", "false"]], "All and a typed value exclude each other");
      await type(p, "hosts", "t2");
      await p.press("Enter");
      await p.click(`${sel("hosts")} .listbox [data-value="v0"]`);
      rows = await rowsOf(p, "hosts");
      t.eq(rows.filter((r) => r.sel === "true").map((r) => r.v), ["t1", "t2", "v0"], "two typed values and an option are three checked rows");
      t.eq(rows.slice(0, 2).map((r) => r.v), ["t1", "t2"], "the typed rows come first, in the order of the selection");
      t.eq(await p.text(`${sel("hosts")} .select__all-text`), "Selected (3)", "the count");
      // A click on a checked typed row removes it, and the row leaves the list.
      await p.click(`${sel("hosts")} .listbox [data-value="t1"]`);
      rows = await rowsOf(p, "hosts");
      t.eq(rows.map((r) => r.v).slice(0, 2), ["t2", "$__all"], "the row of t1 left the list");
      // Nothing goes to the server before the selection applies.
      const before = panelQueries(p).length;
      await p.press("Enter");
      await p.waitFor(`document.querySelector('${sel("hosts")} .select__popup') === null`, { what: "Enter in an empty field closes the popup" });
      await waitPanelsReady(p, [1]);
      t.assert(panelQueries(p).length > before, "the selection applied");
      t.eq(await urlVars(p, "hosts"), ["t2", "v0"], "the URL repeats the variable in the order of the selection");
      t.eq(await btnText(p, "hosts"), "t2 + v0", "the select shows the texts");
      t.eq(lastBody(p, 1).variables.hosts, { all: false, values: ["t2", "v0"] }, "the request holds the typed value");
      // All and a typed value exclude each other, the other way.
      await openPopup(p, "hosts");
      await p.click(`${sel("hosts")} .listbox [data-value="$__all"]`);
      rows = await rowsOf(p, "hosts");
      t.eq(rows.map((r) => r.v).includes("t2"), false, "All takes the typed row out");
      t.eq(rows.find((r) => r.v === "$__all").sel, "true", "All is selected");
      await p.press("Escape");
    },
  },

  {
    id: "B163",
    name: "the combobox of an open variable (9.26): the attributes, the focus, the arrow keys, Space, Escape, Tab, the errors with role alert, the contrast of the active row in both themes",
    async run(t) {
      const d = doc("ov163", "Combobox", [section(null, [table(1, inList(["s1", "*m1"]))])], { variables: [qv("s1", { open: true, default: ["v1"] }), qv("m1", { open: true, multi: true })] });
      await ensure(t, d);
      for (const scheme of ["dark", "light"]) {
        const p = await t.page({ width: 1280, height: 900 });
        await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
        await p.goto(t.dash("ov163"));
        await p.idle();
        await waitPanelsReady(p, [1]);
        await openPopup(p, "s1");
        const attrs = () => p.eval(`(() => {
          const e = document.querySelector('${searchOf("s1")}');
          const list = document.querySelector('${sel("s1")} .listbox');
          const active = [...list.querySelectorAll(".listbox__item--active")];
          return { role: e.getAttribute("role"), expanded: e.getAttribute("aria-expanded"), controls: e.getAttribute("aria-controls") === list.id, auto: e.getAttribute("aria-autocomplete"), focus: document.activeElement === e, desc: e.getAttribute("aria-activedescendant"), activeIds: active.map((a) => a.id), listRole: list.getAttribute("role") };
        })()`);
        let a = await attrs();
        t.eq([a.role, a.expanded, a.controls, a.auto, a.focus, a.desc, a.listRole], ["combobox", "true", true, "list", true, null, "listbox"], `${scheme}: the attributes of the field`);
        await p.press("ArrowDown");
        a = await attrs();
        const rows = await rowsOf(p, "s1");
        t.eq([a.activeIds.length, a.desc === rows[0].id, a.activeIds[0] === a.desc, a.focus], [1, true, true, true], `${scheme}: ArrowDown makes the first row active and the focus stays in the field`);
        await p.press("ArrowDown");
        a = await attrs();
        t.eq(a.desc, rows[1].id, `${scheme}: the second row`);
        await p.press("ArrowUp");
        t.eq((await attrs()).desc, rows[0].id, `${scheme}: ArrowUp goes back`);
        // The ring of the active row has 3:1 against the popup.
        const ring = await p.eval(`(() => {${CONTRAST}
          const row = document.querySelector('${sel("s1")} .listbox__item--active'); const cs = getComputedStyle(row);
          const bg = backdrop(row.closest(".select__popup"));
          const ringColour = cs.outlineStyle !== "none" && parseFloat(cs.outlineWidth) > 0 ? over(parse(cs.outlineColor), bg) : { r: 0, g: 0, b: 0, a: 1 };
          return { outline: cs.outlineStyle + " " + cs.outlineWidth, ratio: ratio(ringColour, bg) };
        })()`);
        t.assert(ring.outline.startsWith("solid") && ring.ratio >= 3, `${scheme}: the indicator of the active row has 3:1 with the popup: ${JSON.stringify(ring)}`);
        // Space types a space. Home and End are the keys of the field.
        await type(p, "s1", "a");
        await p.press(" ");
        await type(p, "s1", "b");
        t.eq(await p.eval(`document.querySelector('${searchOf("s1")}').value`), "a b", `${scheme}: Space types a space`);
        t.eq(await useRow(p, "s1"), 'Use "a b"', `${scheme}: the row Use shows the text`);
        t.assert(await p.exists(`${sel("s1")} .select__popup`), "Space did not choose a row");
        // Enter with the row Use active chooses the typed text.
        await p.press("ArrowDown");
        t.eq(await p.eval(`document.getElementById(document.querySelector('${searchOf("s1")}').getAttribute("aria-activedescendant")).dataset.testid`), "variable-s1-use", `${scheme}: the row Use is the last row`);
        await p.press("Enter");
        await p.waitFor(`document.querySelector('${sel("s1")} .select__popup') === null`);
        t.eq(await btnText(p, "s1"), "a b", `${scheme}: the typed value`);
        // Escape closes with no change.
        await openPopup(p, "s1");
        await p.press("ArrowDown");
        await p.press("Escape");
        await p.waitFor(`document.querySelector('${sel("s1")} .select__popup') === null`);
        t.eq([await btnText(p, "s1"), await urlVars(p, "s1")], ["a b", ["a b"]], `${scheme}: Escape changes nothing`);
        // The errors of a typed value: role alert, aria-describedby, no row Use.
        await openPopup(p, "s1");
        const errors = [["x".repeat(1025), "A value has 1 to 1024 bytes."], ["a\u0001b", "A value has no control character."], ["$__all", "This value is reserved."]];
        for (const [text, message] of errors) {
          await p.fill(searchOf("s1"), text);
          const e = await p.eval(`(() => { const f = document.querySelector('${searchOf("s1")}'); const m = document.getElementById(f.getAttribute("aria-describedby")); return { text: m.textContent, role: m.getAttribute("role"), hidden: m.hidden, invalid: f.getAttribute("aria-invalid") }; })()`);
          t.eq(e, { text: message, role: "alert", hidden: false, invalid: "true" }, `${scheme}: the error for ${JSON.stringify(text.slice(0, 8))}`);
          t.eq(await useRow(p, "s1"), null, `${scheme}: no row Use for a text with an error`);
        }
        // Enter on an invalid text adds nothing.
        await p.press("Enter");
        t.assert(await p.exists(`${sel("s1")} .select__popup`), `${scheme}: Enter on an invalid text keeps the popup`);
        await p.fill(searchOf("s1"), "");
        t.eq(await p.eval(`document.getElementById(document.querySelector('${searchOf("s1")}').getAttribute("aria-describedby")).hidden`), true, `${scheme}: no error for an empty field`);
        await p.press("Escape");
        // Tab closes the popup, and a multi selection applies, as for a closed variable.
        await openPopup(p, "m1");
        await p.click(`${sel("m1")} .listbox [data-value="v2"]`);
        // v0 is the default selection of m1, and the address does not write a default: no name before the popup closes, and the name `v0,v2` after.
        t.eq(await urlVars(p, "m1"), [], `${scheme}: nothing applies before the popup closes`);
        await p.press("Tab");
        await p.waitFor(`document.querySelector('${sel("m1")} .select__popup') === null`, { what: "Tab closes the popup" });
        await p.waitFor(`new URLSearchParams(location.search).getAll("var-m1").join() === "v0,v2"`, { what: "the multi selection applies" });
        // The scripted checks (axe-core is not run: no network): the ids are unique and each reference resolves.
        await openPopup(p, "m1");
        await type(p, "m1", "x");
        const c = await p.eval(`(() => {
          const seen = new Set(); const dup = []; for (const e of document.querySelectorAll("[id]")) { if (seen.has(e.id)) dup.push(e.id); seen.add(e.id); }
          const refs = []; for (const e of document.querySelectorAll("[aria-controls], [aria-describedby], [aria-activedescendant], [aria-labelledby]")) for (const a of ["aria-controls", "aria-describedby", "aria-activedescendant", "aria-labelledby"]) for (const id of (e.getAttribute(a) || "").split(" ").filter(Boolean)) if (!document.getElementById(id)) refs.push(a + " " + id);
          const use = document.querySelector('[data-testid="variable-m1-use"]');
          return { dup, refs, useRole: use.getAttribute("role"), useName: use.textContent };
        })()`);
        t.eq(c, { dup: [], refs: [], useRole: "option", useName: 'Use "x"' }, `${scheme}: ids, references, the row Use is an option`);
        await p.press("Escape");
      }
    },
  },

  {
    id: "B164",
    name: "the state No options: the bar, a placed frame, the panel view, the popup texts, the empty value in the panels, All, an open variable",
    async run(t) {
      const d = doc("ov164", "No options", [section(null, [table(1, inList(["none"]), { x: 0, y: 2, w: 24, h: 6 })])], {
        variables: [
          qv("none", { sql: NONE_SQL }),
          qv("noneall", { sql: NONE_SQL, all: true, default: ["$__all"] }),
          qv("noneopen", { sql: NONE_SQL, open: true }),
          qv("placednone", { sql: NONE_SQL, placement: at(0, 0, 0, 8, 2) }),
        ],
      });
      await ensure(t, d);
      const p = await t.open(t.dash("ov164"));
      t.allow(p, /Failed to load resource/);
      await waitPanelsReady(p, [1]);
      const shown = (n) => p.eval(`(() => { const b = document.querySelector('[data-testid="variable-${n}-button"]'); const v = b.querySelector(".select__value"); return { text: v.textContent, muted: v.classList.contains("muted"), name: b.getAttribute("aria-label") }; })()`);
      t.eq(await shown("none"), { text: "No options", muted: true, name: "none: no options" }, "the bar: the muted text and the name");
      t.eq(await shown("noneall"), { text: "All", muted: false, name: "noneall: All" }, "include_all: the control shows All");
      t.eq(await p.text(`[data-testid="placement-placednone"] .select__value`), "No options", "a placed frame");
      t.eq(lastBody(p, 1).variables.none, { all: false, values: [] }, "the panel runs with the empty value of 7.7");
      t.assert((await p.eval(`document.querySelector('[data-testid="panel-1"]').dataset.state`)) === "ready", "the panel shows what the query gives, not a new error");
      // The popup texts.
      await openPopup(p, "none");
      t.eq(await p.text(`${sel("none")} .listbox__empty`), "No options", "the popup of a closed variable");
      await p.press("Escape");
      await openPopup(p, "noneopen");
      t.eq(await p.text(`${sel("noneopen")} .listbox__empty`), "No options. Type a value.", "the popup of an open variable");
      t.eq(await shown("noneopen"), { text: "No options", muted: true, name: "noneopen: no options" }, "an open variable with no value");
      await type(p, "noneopen", "abc");
      await p.press("Enter");
      await p.waitFor(`document.querySelector('${sel("noneopen")} .select__popup') === null`);
      t.eq(await shown("noneopen"), { text: "abc", muted: false, name: "noneopen: abc" }, "an open variable takes a typed value");
      t.eq(await urlVars(p, "noneopen"), ["abc"], "the typed value is in the URL");
      // The panel view keeps the control.
      const view = await t.open(t.dash("ov164", "viewPanel=1"));
      t.allow(view, /Failed to load resource/);
      await view.waitFor(`document.querySelector('[data-testid="variable-none-button"]')`, { what: "the control in the panel view" });
      t.eq((await view.text(`${view.tid("variable-none-button")} .select__value`)), "No options", "the panel view");
    },
  },

  {
    id: "B165",
    name: "a custom variable and the server: a closed variable replaces a URL value and the server refuses it with no source query; an open variable gives a literal; a quoted name is refused at save",
    async run(t) {
      const d = doc("ov165", "Custom values", [section(null, [table(1, ["SELECT '${env}' AS e, 'B165-marker' AS m"]), table(2, ["SELECT '${oc}' AS o, 'B165-open' AS m"])])], {
        variables: [cv("env"), cv("oc", { open: true, options: [{ text: "Alpha", value: "a" }, { text: "Beta", value: "b" }], default: ["a"] })],
      });
      await ensure(t, d);
      const p = await t.open(t.dash("ov165", "var-env=zzz"));
      await waitPanelsReady(p, [1, 2]);
      t.eq((await notices(p))[0].text, 'This value is not an option of env: "zzz". The page used "Production".', "a closed variable replaces the URL value with a notice");
      t.eq(await urlVars(p, "env"), ["prod"], "the URL");
      const count = async (marker) => {
        await t.ch.query("SYSTEM FLUSH LOGS");
        return Number((await t.ch.query(`SELECT count() FROM system.query_log WHERE type = 'QueryFinish' AND query LIKE '%${marker}%' AND query NOT LIKE '%query_log%'`)).trim());
      };
      const before = await count("B165-marker");
      t.assert(before >= 1, "the page ran the panel with the option");
      const body = lastBody(p, 1);
      for (const values of [["zzz"], ["PROD"], ["prod "], ["Production"], ["prod", "zzz"]]) {
        const r = await t.api.call("POST", "api/dashboards/ov165/panels/1/query", { body: { ...body, variables: { ...body.variables, env: { all: false, values } } } });
        t.eq(r.status, 422, `422 for ${JSON.stringify(values)}: ${r.text.slice(0, 200)}`);
        t.assert(r.text.includes("variable_value_refused"), `the code: ${r.text.slice(0, 200)}`);
      }
      t.eq(await count("B165-marker"), before, "no source query for a refused value");
      // An open custom variable: a typed value with a quote reaches the panel as a literal.
      await openPopup(p, "oc");
      await type(p, "oc", "a'b");
      await p.press("Enter");
      await p.waitFor(`document.querySelector('${sel("oc")} .select__popup') === null`);
      await p.waitFor(`document.querySelector('[data-testid="panel-2"]').dataset.state === "ready"`, { what: "the panel runs" });
      await p.idle();
      t.eq(lastBody(p, 2).variables.oc, { all: false, values: ["a'b"] }, "the request holds the typed value");
      t.assert((await p.text('[data-testid="table-2"]')).includes("a'b"), "the panel shows the literal: " + (await p.text('[data-testid="table-2"]')));
      // A hostile value: the server answers with a literal, never with raw code.
      const open = lastBody(p, 2);
      const r2 = await t.api.call("POST", "api/dashboards/ov165/panels/2/query", { body: { ...open, variables: { ...open.variables, oc: { all: false, values: ["x' OR 1=1 --"] } } } });
      t.eq(r2.status, 200, "a value outside the options is not an error for an open variable: " + r2.text.slice(0, 200));
      // A save of an open variable in a quoted name shows the error in the editor.
      const q = doc("ov165q", "Quoted name", [section(null, [table(1, ["SELECT count() AS n FROM analytics.seed AS `${qn}`"])])], { variables: [cv("qn", { options: [{ text: "Seed", value: "seed" }], default: ["seed"] })] });
      await ensure(t, q);
      const e = await t.open(t.dash("ov165q"));
      t.allow(e, /Failed to load resource/);
      await waitPanelsReady(e, [1]);
      await enterEdit(e);
      await moreItem(e, "dash-settings");
      await e.waitSel(e.tid("settings-view"));
      await e.click(e.tid("settings-tab-variables"));
      await e.click(e.tid("variable-0-open"));
      await e.click(e.tid("variable-form-allow-custom"));
      await waitDraft(e, "ov165q", (x) => x.document.variables[0].allow_custom_value === true);
      await saveDialog(e);
      await e.waitFor(`!document.querySelector('[data-testid="save-errors"]').hidden`, { what: "the save errors" });
      const msg = await e.text(e.tid("save-errors"));
      t.assert(/not_permitted|quoted name/i.test(msg) && msg.includes("/sections/0/panels/0/data/query/sql/0"), "the save refuses a placeholder of an open variable in a quoted name: " + msg);
    },
  },

  {
    id: "B166",
    name: "the placement and the repeat of an open variable: the same control and keys in the grid; a typed value makes no copy; a closed variable is as it was",
    async run(t) {
      const rep = (id, name, grid) => {
        const p = panel(id, "stat", `Host \${${name}}`, [`SELECT '\${${name}:csv}' AS host, 1 AS n`], { grid });
        p.repeat = { variable: name, direction: "h" };
        return p;
      };
      const d = doc("ov166", "Repeat", [section(null, [rep(1, "rep", { x: 0, y: 2, w: 8, h: 4 }), rep(2, "clo", { x: 0, y: 6, w: 8, h: 4 })])], {
        variables: [qv("rep", { open: true, multi: true, all: true, default: ["$__all"], placement: at(0, 0, 0, 12, 2) }), qv("clo", { multi: true, all: true, default: ["$__all"] })],
      });
      await ensure(t, d);
      const p = await t.open(t.dash("ov166", "var-rep=v0&var-rep=v1&var-rep=zz&var-clo=v0&var-clo=v2"));
      await waitPanelsReady(p, [1, 2]);
      const keys = (id) => p.eval(`[...document.querySelectorAll('section.panel')].map((e) => e.dataset.testid).filter((k) => new RegExp("^panel-${id}(-r\\\\d+)?$").test(k))`);
      t.eq((await keys(1)).length, 2, "a typed value makes no copy: v0 and v1 only");
      t.eq((await keys(2)).length, 2, "a closed variable: one copy for each selected option");
      t.eq(await p.text(`[data-testid="placement-rep"] .select__value`), "v0 + v1 + zz", "the control in the grid shows the typed value");
      // The placed control is the same control: its popup holds the field and the keys of 9.10.
      await p.click(`[data-testid="placement-rep"] ${p.tid("variable-rep-button")}`);
      await p.waitSel(`${sel("rep")} .select__popup`);
      t.eq(await p.eval(`document.querySelector('${searchOf("rep")}').getAttribute("role")`), "combobox", "the field of the placed control");
      const rows = await rowsOf(p, "rep");
      t.eq(rows.slice(0, 3).map((r) => [r.v, r.sel]), [["zz", "true"], ["$__all", "false"], ["v0", "true"]], "the typed row first, checked");
      const box = await p.eval(`(() => { const r = document.querySelector('${sel("rep")} .select__popup').getBoundingClientRect(); return [r.left >= 0, r.right <= innerWidth, r.top >= 0, r.bottom <= innerHeight]; })()`);
      t.eq(box, [true, true, true, true], "the popup is in the window");
      await type(p, "rep", "q9");
      await p.press("Enter");
      await p.press("Enter");
      await p.waitFor(`document.querySelector('${sel("rep")} .select__popup') === null`);
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-rep").join() === "v0,v1,zz,q9"`, { what: "the URL of the placed control" });
      await p.idle();
      t.eq((await keys(1)).length, 2, "still two copies after a second typed value");
      // The closed variable is as it was: a value that is not an option is replaced, with a notice.
      const c = await t.open(t.dash("ov166", "var-clo=zz&var-clo=v1"));
      await waitPanelsReady(c, [2]);
      t.eq((await notices(c)).map((n) => n.key), ["variable-clo"], "the notice of the closed variable");
      t.eq(await c.text(`${c.tid("variable-clo-button")} .select__value`), "v1", "the closed variable keeps the option");
    },
  },

  {
    id: "B167",
    name: "the editor: the switch Allow custom values, the help texts, the draft grid, the typed default, the menu Examples and its undo, Show options",
    async run(t) {
      const seed = doc("ov167", "Editor", [section(null, [table(1, ["SELECT 1 AS one"], { x: 0, y: 2, w: 12, h: 4 })])], { variables: [] });
      await ensure(t, seed);
      const p = await t.open(t.dash("ov167"));
      t.allow(p, /Failed to load resource/);
      await waitPanelsReady(p, [1]);
      await enterEdit(p);
      await moreItem(p, "dash-settings");
      await p.waitSel(p.tid("settings-view"));
      await p.click(p.tid("settings-tab-variables"));
      const help = () => p.eval(`document.getElementById(document.querySelector('[data-testid="variable-form-allow-custom"]').getAttribute("aria-describedby")).textContent`);
      const checked = () => p.eval(`document.querySelector('[data-testid="variable-form-allow-custom"]').checked`);
      const draft = async () => (await draftOf(p, "ov167")).document.variables;
      // A custom variable: the switch is off, the help text, the typed default.
      await p.select(p.tid("variable-add-kind"), "custom");
      await p.click(p.tid("variable-add"));
      await p.waitSel(p.tid("variable-form-allow-custom"));
      t.eq(await checked(), false, "off for a new custom variable");
      t.eq(await help(), "Off: the value must be one of the options. On: a person can also type a value. A typed value is free text: the SQL cannot use it in a quoted name, and in code it must be a number.", "the help of a custom variable");
      const order = (await p.eval(`[...document.querySelectorAll('.vform [data-testid^="variable-form-"]')].map((e) => e.dataset.testid)`));
      t.assert(order.indexOf("variable-form-multi") < order.indexOf("variable-form-allow-custom") && order.indexOf("variable-form-allow-custom") < order.indexOf("variable-form-include-all"), "the switch is after Multi-value: " + order);
      await p.click(p.tid("variable-form-default-add"));
      await p.fill(p.tid("variable-form-default-0-value"), "typed");
      await sleep(600);
      t.assert((await p.text(".vform")).includes('The default "typed" is not an option.'), "closed: the default must be an option");
      await p.click(p.tid("variable-form-allow-custom"));
      await waitDraft(p, "ov167", (x) => x.document.variables[0] && x.document.variables[0].allow_custom_value === true);
      await sleep(300);
      t.assert(!(await p.text(".vform")).includes("is not an option"), "open: the default can be a typed value");
      t.eq(Object.keys((await draft())[0]).slice(-2), ["default", "allow_custom_value"], "the member follows default");
      await p.click(p.tid("variable-form-allow-custom"));
      await waitDraft(p, "ov167", (x) => x.document.variables[0] && !("allow_custom_value" in x.document.variables[0]));
      t.assert((await p.text(".vform")).includes('The default "typed" is not an option.'), "closed again: the error is back");
      await p.click(p.tid("variable-back"));
      // A query variable: the help text, the menu Examples, the undo, Show options.
      await p.select(p.tid("variable-add-kind"), "query");
      await p.click(p.tid("variable-add"));
      await p.waitSel(p.tid("variable-form-allow-custom"));
      t.eq(await checked(), false, "off for a new query variable");
      t.eq(await help(), "Off: the value must be one that the query returns. On: a person can also type a value.", "the help of a query variable");
      await p.fill(p.tid("sql-input"), "SELECT 1");
      await p.eval(`(() => { const e = document.querySelector('[data-testid="sql-input"]'); e.focus(); e.setSelectionRange(e.value.length, e.value.length); })()`);
      await p.click(p.tid("variable-sql-examples"));
      await p.waitSel(p.tid("variable-sql-example-1"));
      const items = await p.eval(`[...document.querySelectorAll('[data-testid^="variable-sql-example-"]')].map((e) => e.dataset.testid)`);
      t.eq(items, ["variable-sql-example-0", "variable-sql-example-1", "variable-sql-example-2"], "three items from 0");
      t.assert((await p.text(p.tid("variable-sql-example-1"))).includes("Set Refresh to On time range change."), "the hint of item 1");
      await p.click(p.tid("variable-sql-example-2"));
      await sleep(400);
      const value = () => p.eval(`document.querySelector('[data-testid="sql-input"]').value`);
      t.eq(await value(), "SELECT 1SELECT DISTINCT toString(id) AS value, name AS text\nFROM services\nORDER BY text\nLIMIT 1000", "the text goes at the cursor");
      t.eq(await p.eval(`document.activeElement === document.querySelector('[data-testid="sql-input"]')`), true, "the area keeps the focus");
      await p.press("z", { modifiers: 2 });
      await sleep(300);
      t.eq(await value(), "SELECT 1", "the undo of the browser takes the text out in one step");
      // Show options: the count, 50 rows at most, the error of a wrong SQL.
      await p.fill(p.tid("sql-input"), "SELECT toString(number) AS v FROM numbers(120)");
      await sleep(600);
      await p.click(p.tid("variable-form-preview"));
      await p.waitFor(`document.querySelector('[data-testid="variable-preview-status"]').textContent === "120 options"`, { what: "the count" });
      t.eq(await p.count(`${p.tid("variable-preview-list")} li`), 51, "50 rows and the text 'and <n> more'");
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-preview-list"] li:last-child').textContent`), "and 70 more", "the rest");
      await p.fill(p.tid("sql-input"), "SELEC nope");
      await sleep(600);
      await p.click(p.tid("variable-form-preview"));
      await p.waitFor(`/invalid|first word|SELECT/.test(document.querySelector('[data-testid="variable-preview-status"]').textContent)`, { what: "the error" });
      t.assert((await p.text(p.tid("variable-preview-status"))).includes("/dashboard_variables/0/sql/0"), "the pointer of the error: " + (await p.text(p.tid("variable-preview-status"))));
      // The draft grid shows the new control at once: a placed open variable has the combobox.
      await p.fill(p.tid("sql-input"), "SELECT DISTINCT node FROM analytics.seed ORDER BY node");
      await sleep(600);
      await p.click(p.tid("variable-form-placed"));
      await p.click(p.tid("variable-form-allow-custom"));
      await waitDraft(p, "ov167", (x) => x.document.variables[1] && x.document.variables[1].allow_custom_value === true && x.document.variables[1].placement);
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("settings-view"));
      const name = (await draft())[1].name;
      await p.waitSel(`[data-testid="placement-${name}"]`);
      await p.click(`[data-testid="placement-${name}"] ${p.tid(`variable-${name}-button`)}`);
      await p.waitSel(`[data-testid="variable-${name}-search"]`);
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-${name}-search"]').getAttribute("role")`), "combobox", "the control of the draft grid is a combobox");
      await p.press("Escape");
    },
  },

  {
    id: "B168",
    name: "the import of allowCustomValue (15.6): the member for true, false and absent, a custom variable in a quoted name and in code stays closed with a changed row, the editor shows the switch",
    async run(t) {
      const s = await t.ownServer("imp168");
      const work = mkdtempSync(join(tmpdir(), "hune-b168-"));
      const fx = join(ROOT, "tests/fixtures/grafana");
      mkdirSync(join(work, "dashboards"), { recursive: true });
      const source = JSON.parse(readFileSync(join(fx, "open_variables.json"), "utf8"));
      // The fixture has one open and one closed variable of each kind. Two copies of the open custom variable make the cases of 15.6 that keep it closed:
      // one in a quoted name, one in code with the format none.
      const openCustom = source.templating.list.find((v) => v.name === "env_open");
      // A list in a quoted name is refused for a closed variable too, so the copies have one value.
      const single = (name) => ({ ...structuredClone(openCustom), name, multi: false, includeAll: false, options: openCustom.options.filter((o) => o.value !== "$__all"), current: { selected: true, text: "Production", value: "prod" } });
      source.templating.list.push(single("env_q"), single("env_code"));
      source.panels[0].targets[0].rawSql += "\n  AND k = ${env_code}";
      const second = structuredClone(source.panels[0]);
      Object.assign(second, { id: 2, title: "Quoted", gridPos: { ...source.panels[0].gridPos, y: (source.panels[0].gridPos.y || 0) + (source.panels[0].gridPos.h || 8) } });
      second.targets[0].rawSql = "SELECT count() AS n\nFROM analytics.`${env_q}`";
      source.panels.push(second);
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
      const changed = items.filter((i) => i.class === "changed" && /allowCustomValue|allow_custom_value/.test(`${i.id} ${i.path || ""} ${i.message || ""}`));
      t.assert(changed.length >= 2, "the report has the changed rows for allowCustomValue: " + JSON.stringify(items.map((i) => [i.id, i.class]).slice(0, 12)) + " " + JSON.stringify(report).slice(0, 600) + dry.stdout.slice(-300));
      const accept = join(work, "accept.txt");
      writeFileSync(accept, `# accepted for the case B168\n${items.map((i) => i.id).join("\n")}\n`);
      await s.stop();
      const wrote = run(["--accept", accept, "--write"]);
      t.eq(wrote.status, 0, "the import: " + wrote.stdout.slice(-400) + wrote.stderr.slice(-300));
      await s.restart();
      const got = await (await fetch(`${s.url}api/dashboards/${source.uid}`)).json();
      const byName = Object.fromEntries(got.document.variables.map((v) => [v.name, v]));
      t.eq(byName.region_open.allow_custom_value, true, "a query variable with true is open");
      t.assert(!("allow_custom_value" in byName.region_closed), "a query variable with false is closed (no member)");
      t.assert(!("allow_custom_value" in byName.env_closed), "a custom variable with false is closed");
      t.eq(byName.env_open.allow_custom_value, true, "a custom variable with true in a format of a string is open");
      t.assert(!("allow_custom_value" in byName.env_q), "a custom variable in a quoted name stays closed");
      t.assert(!("allow_custom_value" in byName.env_code), "a custom variable in code with the format none stays closed");
      t.assert(!("allow_custom_value" in byName.note), "a text variable has no such member");
      const p = await t.page({ width: 1280, height: 900 });
      t.allow(p, /Failed to load resource/);
      await p.goto(`${s.url}d/${source.uid}`);
      await p.idle();
      await enterEdit(p);
      await moreItem(p, "dash-settings");
      await p.click(p.tid("settings-tab-variables"));
      const idx = got.document.variables.findIndex((v) => v.name === "region_open");
      await p.click(p.tid(`variable-${idx}-open`));
      await p.waitSel(p.tid("variable-form-allow-custom"));
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-form-allow-custom"]').checked`), true, "the editor shows the switch on for an imported open variable");
    },
  },

  {
    id: "B169",
    name: "layouts and themes: at 1920, 1280, 768 and 390 px in both themes the popup of an open variable stays in the window, the rows have 38 px, the texts have the contrast, no horizontal scroll bar",
    async run(t) {
      const d = doc("ov169", "Layouts", [section(null, [table(1, inList(["*o1"]))])], { variables: [qv("o1", { open: true, multi: true, all: true, default: ["$__all"] })] });
      await ensure(t, d);
      for (const width of [1920, 1280, 768, 390]) {
        for (const scheme of ["dark", "light"]) {
          const label = `${width} px, ${scheme}`;
          const p = await t.page({ width, height: width < 800 ? 800 : 900 });
          await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
          await p.goto(t.dash("ov169", "var-o1=a-typed-value-that-is-long&var-o1=v1"));
          await p.idle();
          await waitPanelsReady(p, [1]);
          await openPopup(p, "o1");
          const rowH = await p.eval(`[...document.querySelectorAll('${sel("o1")} .listbox__item')].filter((e) => !e.hidden && e.textContent.length < 6).map((e) => Math.round(e.getBoundingClientRect().height))`);
          await type(p, "o1", "new typed text");
          const m = await p.eval(`(() => {${CONTRAST}
            const popup = document.querySelector('${sel("o1")} .select__popup'); const r = popup.getBoundingClientRect();
            const low = [];
            for (const e of popup.querySelectorAll('.listbox__item:not([hidden]), .select__all, .select__search')) {
              const bg = backdrop(e); const fg = over(parse(getComputedStyle(e).color), bg); const c = ratio(fg, bg);
              if (c < 4.5) low.push((e.dataset.testid || e.dataset.value || e.className) + " " + c.toFixed(2));
            }
            return { inWindow: [r.left >= 0, r.right <= innerWidth, r.top >= 0, r.bottom <= innerHeight], low, noScroll: document.documentElement.scrollWidth <= document.documentElement.clientWidth, use: !!document.querySelector('[data-testid="variable-o1-use"]:not([hidden])') };
          })()`);
          t.eq(m.inWindow, [true, true, true, true], `${label}: the popup is in the window`);
          t.assert(rowH.length > 0 && rowH.every((h) => h === 38), `${label}: the rows have 38 px: ${rowH}`);
          t.eq(m.low, [], `${label}: texts below 4.5:1`);
          t.eq([m.noScroll, m.use], [true, true], `${label}: no horizontal scroll bar, and the row Use shows`);
          await p.press("Escape");
        }
      }
    },
  },
];
