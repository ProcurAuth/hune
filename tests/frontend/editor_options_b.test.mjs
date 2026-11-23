// SPDX-License-Identifier: MIT OR Apache-2.0
// The option forms of `stat`, `bargauge` and `piechart` (10.3h, F43, vector set FE-77): the tables against the format and `newOptions`,
// the write rule and the rules of display on the vectors, and the grammar of the field "Fields".
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { newOptions } from "../../web/js/editor/draft.js";
import stat from "../../web/js/editor/forms/options/stat.js";
import bargauge from "../../web/js/editor/forms/options/bargauge.js";
import piechart from "../../web/js/editor/forms/options/piechart.js";
import { parseFields, reduceRows } from "../../web/js/editor/forms/options/reduce_rows.js";
import {
  CALCS, CHOICE_MAX_PX, KINDS, checkChecks, choiceWidth, parseText, rowsWithoutState, shownIds, statesOf, unknownValueError, valueOf, writeOption,
} from "../../web/js/editor/forms/options/rows.js";

const TABLES = { stat, bargauge, piechart };
const here = (p) => new URL(p, import.meta.url);
const vectors = JSON.parse(readFileSync(here("./vectors/FE-77-options-b.json"), "utf8"));
const format = readFileSync(here("../../docs/02-format-and-storage.md"), "utf8");
const section = (id) => {
  const a = format.indexOf(`### ${id} `);
  return format.slice(a, format.indexOf("\n### ", a + 5));
};
const SECTION = { stat: "4.29", bargauge: "4.33", piechart: "4.31" };
const get = (object, member) => member.split(".").reduce((at, key) => (at == null ? undefined : at[key]), object);

// The leaves of an options object: an object expands, an array or a scalar is a leaf.
function leaves(value, path, out = []) {
  if (value && typeof value === "object" && !Array.isArray(value)) for (const [k, v] of Object.entries(value)) leaves(v, `${path}.${k}`, out);
  else out.push(path);
  return out;
}

// The members with no row (NO_CONTROL of 10.3h) and the optional members that a table of this group covers.
const NO_CONTROL = { piechart: ["options.tooltip.sort", "options.tooltip.hide_zeros"] };
const OPTIONAL = {
  stat: ["options.justify", "options.percent_change", "options.percent_change_color", "options.title_size", "options.value_size"],
  bargauge: [],
  piechart: ["options.legend.sort_by", "options.legend.sort_desc"],
};

test("FE-77: the tables have the groups and the rows of 10.3h", () => {
  assert.deepEqual(stat.groups, [["value", "Value options"], ["styles", "Stat styles"], ["text", "Text size"]]);
  assert.deepEqual(bargauge.groups, [["value", "Value options"], ["gauge", "Bar gauge"]]);
  assert.deepEqual(piechart.groups, [["value", "Value options"], ["pie", "Pie chart"], ["tooltip", "Tooltip"], ["legend", "Legend"]]);
  assert.deepEqual(stat.rows.map((r) => r.id), [
    "reduce-values", "reduce-calc", "reduce-fields", "orientation", "text-mode", "color-mode", "graph-mode", "justify", "percent-change", "percent-change-color",
    "title-size", "value-size", "display-name",
  ]);
  assert.deepEqual(bargauge.rows.map((r) => r.id), ["reduce-values", "reduce-calc", "reduce-fields", "orientation", "display-mode", "show-unfilled", "display-name"]);
  assert.deepEqual(piechart.rows.map((r) => r.id), [
    "reduce-values", "reduce-calc", "reduce-fields", "pie-type", "labels", "tooltip-mode", "legend-show", "legend-mode", "legend-placement", "legend-values",
    "legend-sort-by", "legend-sort-desc", "display-name",
  ]);
  const labels = (t) => t.rows.map((r) => r.label);
  assert.deepEqual(labels(stat), [
    "Show", "Calculation", "Fields", "Orientation", "Text mode", "Color mode", "Graph mode", "Text alignment", "Show percent change", "Percent change color mode",
    "Title", "Value", "Display name",
  ]);
  assert.deepEqual(labels(bargauge), ["Show", "Calculation", "Fields", "Orientation", "Display mode", "Show unfilled area", "Display name"]);
  assert.deepEqual(labels(piechart), ["Show", "Calculation", "Fields", "Pie chart type", "Labels", "Tooltip mode", "Visibility", "Mode", "Placement", "Legend values", "Sort by", "Sort descending", "Display name"]);
  assert.equal(stat.rows.length + bargauge.rows.length + piechart.rows.length, 33);
});

test("FE-77: the hooks, the kinds and the groups of each row are valid", () => {
  for (const [type, table] of Object.entries(TABLES)) {
    const ids = new Set();
    const hooks = new Set();
    const groups = new Set(table.groups.map(([key]) => key).concat("standard"));
    assert.equal(table.type, type);
    for (const r of table.rows) {
      assert.ok(!ids.has(r.id), `${type}: the id ${r.id} is unique`);
      ids.add(r.id);
      assert.equal(r.hook, `opt-${r.id}`, `${type}: the hook of ${r.id}`);
      assert.ok(!hooks.has(r.hook));
      hooks.add(r.hook);
      assert.ok(KINDS.includes(r.kind), `${type}: the kind of ${r.id}`);
      assert.ok(groups.has(r.group), `${type}: the group of ${r.id}`);
      assert.ok(r.member.startsWith("options.") || r.member === "fields.defaults.display_name", `${type}: the member of ${r.id}`);
      assert.ok(!r.when || r.whenText, `${type}: a rule of display has its text (${r.id})`);
      if (r.items) assert.ok(r.items.every(([, text]) => typeof text === "string" && text !== ""), `${type}: the items of ${r.id} have texts`);
    }
    // The groups of the type are in the order of the table, and each one has a row.
    const used = table.groups.map(([key]) => key);
    for (const key of used) assert.ok(table.rows.some((r) => r.group === key), `${type}: the group ${key} has a row`);
    const order = table.rows.map((r) => used.indexOf(r.group)).filter((i) => i >= 0);
    assert.deepEqual(order, [...order].sort((a, b) => a - b), `${type}: the rows follow the order of the groups`);
  }
});

test("FE-77: each member of a new panel has a row or is in NO_CONTROL, and each default equals the new panel", () => {
  for (const [type, table] of Object.entries(TABLES)) {
    const options = newOptions(type);
    const byMember = new Map(table.rows.map((r) => [r.member, r]));
    const want = new Set([...leaves(options, "options"), ...OPTIONAL[type]]);
    for (const member of want) assert.ok(byMember.has(member) || (NO_CONTROL[type] || []).includes(member), `${type}: no row for ${member}`);
    for (const member of byMember.keys()) {
      if (member.startsWith("fields.")) continue;
      assert.ok(want.has(member), `${type}: a row for a member that does not exist: ${member}`);
    }
    for (const r of table.rows) {
      if (!r.member.startsWith("options.") || r.optional) continue;
      assert.deepEqual(r.default, get({ options }, r.member), `${type}: the default of ${r.member}`);
    }
    for (const member of NO_CONTROL[type] || []) assert.ok(!byMember.has(member));
  }
});

test("FE-77: each member and each item of a row is in 4.26 to 4.33", () => {
  const shared = section("4.26");
  for (const [type, table] of Object.entries(TABLES)) {
    const own = section(SECTION[type]);
    for (const r of table.rows) {
      if (!r.member.startsWith("options.")) continue;
      const parts = r.member.split(".");
      const leaf = parts.at(-1);
      const object = parts.length > 2 ? parts.at(-2) : null;
      let text = own;
      if (object) {
        const at = shared.indexOf(`Object \`${object}\``);
        const next = shared.indexOf("\nObject ", at + 5);
        text = shared.slice(at, next > 0 ? next : undefined);
      }
      const line = text.split("\n").find((l) => l.startsWith(`| \`${leaf}\` |`));
      assert.ok(line, `${type}: no line of the format for ${r.member}`);
      if (r.kind === "radio" || (r.kind === "select" && r.items.length < 18)) {
        for (const [value] of r.items) if (typeof value === "string") assert.ok(line.includes(`\`${value}\``), `${type}: the item ${value} of ${r.member} is in the format: ${line}`);
      }
    }
  }
  // The 18 calculations of 4.26 and the 18 items of the form agree.
  const calcLine = section("4.26").split("\n").find((l) => l.startsWith("| `calc` |"));
  const inFormat = [...calcLine.matchAll(/`([a-z_]+)`/g)].map((m) => m[1]).filter((v) => CALCS.some(([c]) => c === v));
  assert.deepEqual([...inFormat].sort(), CALCS.map(([c]) => c).sort());
  assert.equal(CALCS.length, 18);
  for (const table of [stat, bargauge, piechart]) {
    const calc = table.rows.find((r) => r.id === "reduce-calc");
    assert.equal(calc.items, CALCS);
  }
});

test("FE-77: the rows of the reduction are the same rows in the three types", () => {
  const own = reduceRows();
  assert.equal(own.length, 3);
  for (const table of [stat, bargauge, piechart]) {
    const [a, b, c] = table.rows;
    for (const [got, want] of [[a, own[0]], [b, own[1]], [c, own[2]]]) {
      assert.equal(got.id, want.id);
      assert.equal(got.member, want.member);
      assert.equal(got.kind, want.kind);
      assert.equal(got.group, "value");
    }
  }
  assert.equal(own[2].kind, "text");
  assert.equal(own[2].nullable, true);
  assert.equal(own[2].default, null);
});

test("FE-77: the choices are 280 px wide at most, and each rule of display is true in one state and false in one state", () => {
  for (const [type, table] of Object.entries(TABLES)) {
    for (const r of table.rows) if (r.kind === "radio") assert.ok(choiceWidth(r.items) <= CHOICE_MAX_PX, `${type}: the choice ${r.id} is ${Math.round(choiceWidth(r.items))} px wide`);
    const options = newOptions(type);
    assert.deepEqual(rowsWithoutState(table, options), [], `${type}: each row shows in one state`);
    const seen = new Map();
    for (const state of statesOf(table, options)) for (const r of table.rows) if (r.when) (seen.get(r.id) || seen.set(r.id, new Set()).get(r.id)).add(r.when(state.options));
    // The states come from one change of a choice, a switch or a select. A rule must be false somewhere too: test the values that hide it.
    const hiding = { "reduce-calc": { reduce: { values: true } } };
    for (const r of table.rows) {
      if (!r.when) continue;
      const hidden = hiding[r.id];
      if (hidden) assert.equal(r.when({ ...structuredClone(options), ...hidden }), false, `${type}: ${r.id} is hidden`);
      assert.equal(r.when(structuredClone(options)) || [...(seen.get(r.id) || [])].includes(true), true, `${type}: ${r.id} shows in one state`);
    }
  }
  // The legend of a pie chart: the rules follow `legend.show` and `legend.mode`.
  const o = newOptions("piechart");
  const rows = Object.fromEntries(piechart.rows.map((r) => [r.id, r]));
  assert.equal(rows["legend-mode"].when(o), true);
  assert.equal(rows["legend-mode"].when({ ...o, legend: { ...o.legend, show: false } }), false);
  assert.equal(rows["legend-values"].when(o), false);
  assert.equal(rows["legend-values"].when({ ...o, legend: { ...o.legend, mode: "table" } }), true);
  assert.equal(rows["legend-values"].when({ ...o, legend: { ...o.legend, mode: "table", show: false } }), false);
  assert.equal(rows["legend-sort-desc"].when({ ...o, legend: { ...o.legend, mode: "table" } }), false);
  assert.equal(rows["legend-sort-desc"].when({ ...o, legend: { ...o.legend, mode: "table", sort_by: "name" } }), true);
});

test("FE-77: the field Fields takes a pattern of 4.24 or an empty text", () => {
  assert.deepEqual(parseFields(""), { value: null });
  for (const ok of ["Total", "/^Total$/", "/a|b/gims", "(?:a|b){2}", "\\d+", "x".repeat(1024)]) assert.deepEqual(parseFields(ok), { value: ok }, ok);
  for (const bad of ["/", "/(/", "/a/z", "/a/ii", "\\s", "\\p{L}", "(?i)a", "[[:alpha:]]", "x".repeat(1025), " ".repeat(1025)]) assert.equal(parseFields(bad).error, "The pattern is not valid.", JSON.stringify(bad));
  // A text of spaces is a plain text: it is an exact match of the spaces, and the page does not change it.
  assert.deepEqual(parseFields(" "), { value: " " });
  const fields = reduceRows()[2];
  assert.deepEqual(parseText(fields, ""), { value: null });
  assert.equal(parseText(fields, "/(/").error, "The pattern is not valid.");
});

const panelOf = (v) => ({ id: 1, type: v.type, title: "t", options: structuredClone(v.options), fields: structuredClone(v.panelFields || { defaults: {}, overrides: [] }), links: [] });

test("FE-77: the vectors give the write rule and the rules of display", () => {
  assert.ok(vectors.vectors.length >= 60);
  const names = new Set();
  for (const v of vectors.vectors) {
    assert.ok(!names.has(v.name), `the name ${v.name} is unique`);
    names.add(v.name);
    const table = TABLES[v.type];
    const r = table.rows.find((x) => x.id === v.row);
    assert.ok(r, `${v.name}: the row ${v.row} exists`);
    const panel = panelOf(v);
    const before = JSON.stringify(panel);
    let error = "";
    let value = v.remove ? undefined : v.value;
    if (r.kind === "text" || r.kind === "number") {
      const parsed = parseText(r, value, panel.options);
      error = parsed.error || "";
      if (!error) value = parsed.value;
    } else if (r.kind === "checks") error = checkChecks(r, value);
    else if (r.kind === "radio" || r.kind === "select") error = v.remove ? "" : unknownValueError(r, value);
    if (v.error) {
      assert.ok(error.includes(v.error), `${v.name}: the error "${error}" has "${v.error}"`);
      assert.equal(JSON.stringify(panel), before, `${v.name}: nothing is written`);
      continue;
    }
    assert.equal(error, "", `${v.name}: no error`);
    const next = writeOption(panel, r, value);
    assert.equal(JSON.stringify(panel), before, `${v.name}: the write does not change its input`);
    // The exact JSON, with the order of 4.38: a text compare of the options.
    assert.equal(JSON.stringify(next.options), JSON.stringify(v.after), v.name);
    assert.equal(JSON.stringify(next.fields), JSON.stringify(v.afterFields || panel.fields), `${v.name}: the fields`);
    assert.deepEqual(shownIds(table, next.options), v.shown, `${v.name}: the shown rows`);
  }
});

test("FE-77: a row shows the value of the draft, and the absent value of an optional member", () => {
  const stat1 = { type: "stat", options: newOptions("stat"), fields: { defaults: {}, overrides: [] } };
  const row = (table, id) => table.rows.find((r) => r.id === id);
  assert.equal(valueOf(stat1, row(stat, "justify")), "auto");
  assert.equal(valueOf({ ...stat1, options: { ...stat1.options, justify: "center" } }, row(stat, "justify")), "center");
  assert.equal(valueOf(stat1, row(stat, "reduce-fields")), null);
  assert.equal(valueOf(stat1, row(stat, "reduce-values")), false);
  assert.equal(valueOf(stat1, row(stat, "display-name")), undefined);
  assert.equal(valueOf({ ...stat1, fields: { defaults: { display_name: "A" } } }, row(stat, "display-name")), "A");
  // A hidden row keeps its member: a write of `values` does not remove `calc`.
  const written = writeOption(stat1, row(stat, "reduce-values"), true);
  assert.equal(written.options.reduce.calc, "last_not_null");
});

test("FE-77: the tables import the pure modules only and the new files stay small", () => {
  const dir = "../../web/js/editor/forms/options/";
  const own = ["stat.js", "bargauge.js", "piechart.js", "reduce_rows.js"];
  let bytes = 0;
  for (const file of own) {
    const text = readFileSync(here(dir + file), "utf8");
    bytes += Buffer.byteLength(text);
    for (const m of text.matchAll(/^import .* from "([^"]+)";$/gm)) assert.ok(["./rows.js", "./reduce_rows.js", "../rules.js"].includes(m[1]), `${file} imports ${m[1]}`);
    assert.ok(!/\b(document|window|el\()/.test(text.replace(/^\/\/.*$/gm, "")), `${file} has no DOM access`);
  }
  assert.ok(bytes < 8192, `the files of group B have ${bytes} bytes`);
});
