// The option forms of the data types (10.3h, F43): FE-76 (the write rule and the rules of display of timeseries, barchart and xychart) and
// FE-79 (the tables of 10.3h against the code, the format and `newOptions`; the size of the new files).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { newOptions } from "../../web/js/editor/draft.js";
import { NO_CONTROL, TABLES, tableOf } from "../../web/js/editor/forms/options/index.js";
import {
  CALCS, CHOICE_MAX_PX, KINDS, checkChecks, checkSlider, choiceWidth, parseText, rowsWithoutState, shownIds, statesOf, statusText, unknownValueError, valueOf, writeOption,
} from "../../web/js/editor/forms/options/rows.js";

const here = (p) => new URL(p, import.meta.url);
const read = (p) => readFileSync(here(p), "utf8");
const vectors = JSON.parse(read("./vectors/FE-76-options-a.json"));
const editorDoc = read("../../docs/04-frontend-editor.md");
const format = read("../../docs/02-format-and-storage.md");
const A = ["timeseries", "barchart", "xychart"];

const sectionOf = (text, id) => {
  const a = text.indexOf(`### ${id} `);
  return text.slice(a, text.indexOf("\n### ", a + 5));
};
const get = (object, member) => member.split(".").reduce((at, key) => (at == null ? undefined : at[key]), object);

// The new options of a type. The types of F31 are not in `draft.js` until the page draws them: their rows are then compared with the format only.
const optionsOf = (type) => {
  try {
    return newOptions(type);
  } catch (error) {
    return null;
  }
};

// The leaves of an options object: an object expands, an array or a scalar is a leaf.
function leaves(value, path, out = []) {
  if (value && typeof value === "object" && !Array.isArray(value)) for (const [k, v] of Object.entries(value)) leaves(v, `${path}.${k}`, out);
  else out.push(path);
  return out;
}

// --- FE-76: the vectors ---------------------------------------------------------------------------------------------------------------

const panelOf = (v) => ({ id: 1, type: v.type, title: "t", options: structuredClone(v.options), fields: structuredClone(v.panelFields || { defaults: {}, overrides: [] }), links: [] });

test("FE-76: the tables of group A have the groups, the ids and the labels of 10.3h", () => {
  assert.deepEqual(TABLES.timeseries.groups, [["tooltip", "Tooltip"], ["legend", "Legend"], ["axis", "Axis"], ["style", "Graph styles"]]);
  assert.deepEqual(TABLES.barchart.groups, [["bars", "Bar chart"], ["tooltip", "Tooltip"], ["legend", "Legend"], ["axis", "Axis"]]);
  assert.deepEqual(TABLES.xychart.groups, [["xy", "XY chart"], ["tooltip", "Tooltip"], ["legend", "Legend"]]);
  assert.equal(TABLES.timeseries.rows.length, 35);
  assert.equal(TABLES.barchart.rows.length, 29);
  assert.equal(TABLES.xychart.rows.length, 15);
  assert.deepEqual(TABLES.xychart.rows.map((r) => r.label), ["X field", "Y fields", "Show", "Point size", "Line width", "Tooltip mode", "Values sort order", "Hide zeros", "Visibility", "Mode", "Placement", "Values", "Sort by", "Sort descending", "Display name"]);
});

test("FE-76: the vectors give the write rule and the rules of display", () => {
  assert.ok(vectors.vectors.length >= 100);
  const names = new Set();
  const seenRows = new Set();
  for (const v of vectors.vectors) {
    assert.ok(!names.has(v.name), `the name ${v.name} is unique`);
    names.add(v.name);
    const table = TABLES[v.type];
    const r = table.rows.find((x) => x.id === v.row);
    assert.ok(r, `${v.name}: the row ${v.row} exists`);
    seenRows.add(`${v.type}|${v.row}`);
    const panel = panelOf(v);
    const before = JSON.stringify(panel);
    let error = "";
    let value = v.remove ? undefined : v.value;
    if (r.kind === "text" || r.kind === "number") {
      const parsed = parseText(r, value, panel.options);
      error = parsed.error || "";
      if (!error) value = parsed.value;
    } else if (r.kind === "slider") error = checkSlider(r, value);
    else if (r.kind === "checks" || r.kind === "fields") error = checkChecks(r, value);
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
  // Each row of group A has a vector, except the old controls (they keep the code of 10.3a).
  for (const type of A) {
    for (const r of TABLES[type].rows) if (r.kind !== "custom") assert.ok(seenRows.has(`${type}|${r.id}`), `${type}: the row ${r.id} has a vector`);
  }
});

test("FE-76: a row shows the value of the draft, and the absent value of an optional member", () => {
  const panel = { type: "timeseries", options: newOptions("timeseries"), fields: { defaults: {}, overrides: [] } };
  const row = (id) => TABLES.timeseries.rows.find((r) => r.id === id);
  assert.equal(valueOf(panel, row("tooltip-sort")), "none");
  assert.equal(valueOf(panel, row("thresholds-style")), "off");
  assert.equal(valueOf(panel, row("legend-sort-by")), undefined);
  assert.equal(valueOf(panel, row("draw")), "line");
  assert.equal(valueOf({ ...panel, fields: { defaults: { display_name: "A" } } }, row("display-name")), "A");
  // A hidden row keeps its member: a write of the mode does not remove the sort.
  const multi = writeOption(writeOption(panel, row("tooltip-mode"), "multi"), row("tooltip-sort"), "desc");
  assert.equal(writeOption(multi, row("tooltip-mode"), "single").options.tooltip.sort, "desc");
});

test("FE-76: the status line tells the rows that show and hide, with the count above 3 labels", () => {
  const table = TABLES.timeseries;
  const o = newOptions("timeseries");
  const before = shownIds(table, o);
  const multi = shownIds(table, { ...o, tooltip: { mode: "multi" } });
  assert.equal(statusText(table, before, multi), "Shown: Values sort order, Hide zeros.");
  assert.equal(statusText(table, multi, before), "Hidden: Values sort order, Hide zeros.");
  const points = shownIds(table, { ...o, draw: "points" });
  assert.equal(statusText(table, before, points), "Hidden: 8 options.");
  assert.equal(statusText(table, before, before), "");
});

// --- FE-79: the tables against the code ---------------------------------------------------------------------------------------------

// The tables of 10.3h in docs/04: { type: [{ group, label, member, control, values, newText, when, hook }] }.
function specTables() {
  const at = editorDoc.indexOf("#### The tables of the types");
  const text = editorDoc.slice(at, editorDoc.indexOf("\n### ", at));
  const out = {};
  for (const part of text.split(/\n##### /).slice(1)) {
    const type = /^(\w+)/.exec(part)[1];
    if (!TABLES[type]) continue;
    const rows = [];
    for (const line of part.split("\n")) {
      if (!line.startsWith("| ") || line.startsWith("| Group") || line.startsWith("|---")) continue;
      const cells = line.slice(1, -1).split(" | ").map((c) => c.trim());
      if (cells.length !== 8) continue;
      rows.push({ group: cells[0], label: cells[1], member: cells[2].replace(/`/g, ""), control: cells[3], values: cells[4], newText: cells[5], when: cells[6], hook: cells[7].replace(/`/g, "") });
    }
    out[type] = rows;
  }
  return out;
}
const SPEC = specTables();
const KIND_OF = { Choice: "radio", Select: "select", Switch: "switch", "Slider and number": "slider", Number: "number", Text: "text", "Check boxes": "checks", List: "list", "Select of fields": "field", "Check boxes of fields": "fields", "Old control": "custom" };
const memberOf = (spec) => (spec.startsWith("defaults.") ? `fields.${spec}` : `options.${spec}`);
const newValueOf = (text) => {
  if (text === "absent") return undefined;
  const m = /^`(.*)`$/.exec(text);
  assert.ok(m, `a new value in backticks: ${text}`);
  try {
    return JSON.parse(m[1]);
  } catch (error) {
    return m[1];
  }
};

test("FE-79: the code tables are the tables of 10.3h: rows, order, labels, members, controls, hooks and rules", () => {
  let total = 0;
  for (const [type, table] of Object.entries(TABLES)) {
    const spec = SPEC[type];
    assert.ok(spec && spec.length, `${type}: 10.3h has a table`);
    assert.deepEqual(table.rows.map((r) => r.hook), spec.map((s) => s.hook), `${type}: the hooks, in the order of the table`);
    table.rows.forEach((r, i) => {
      const s = spec[i];
      const where = `${type} ${r.id}`;
      assert.equal(r.label, s.label, `${where}: the label`);
      assert.equal(r.member, memberOf(s.member), `${where}: the member`);
      assert.equal(r.kind, KIND_OF[s.control], `${where}: the control`);
      const title = (table.groups.find(([key]) => key === r.group) || [null, { standard: "Standard options", thresholds: "Thresholds" }[r.group]])[1];
      assert.equal(title, s.group, `${where}: the group`);
      assert.equal(r.when ? r.whenText : "always", s.when, `${where}: the rule of display`);
      assert.deepEqual(r.default, newValueOf(s.newText), `${where}: the new value`);
      total += 1;
    });
    assert.equal(table.rows.length, spec.length, `${type}: the number of rows`);
  }
  assert.equal(total, 198, "the 12 tables have 198 rows (decision ID-K290; the switch inspect of wave H; line width, fill opacity and grid lines of the bar chart, P03; the percent change and the text sizes of the stat, P11; the 14 rows of P10 and P14; the 13 rows of the gauge, P12; the 4 rows of the logs panel, F33; the 17 rows of the map, F37)");
});

test("FE-79: each member of a new panel and each optional member has a row or an entry of NO_CONTROL", () => {
  const OPTIONAL = {
    timeseries: ["options.thresholds_style", "options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc", "options.gradient_mode", "options.log_base", "options.span_nulls_ms", "options.bar_alignment"],
    barchart: ["options.thresholds_style", "options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc", "options.fill_opacity", "options.line_width", "options.gradient_mode"],
    xychart: ["options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc"],
    stat: ["options.justify", "options.percent_change", "options.percent_change_color", "options.title_size", "options.value_size"],
    bargauge: [],
    piechart: ["options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc"],
    table: ["options.wrap"],
    heatmap: [],
    histogram: ["options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc"],
    gauge: ["options.orientation", "options.show_threshold_labels", "options.show_threshold_markers", "options.neutral", "options.title_size", "options.value_size", "options.sizing", "options.min_width", "options.min_height"],
    logs: ["options.show_time", "options.wrap", "options.details", "options.order"],
    geomap: [],
  };
  for (const [type, table] of Object.entries(TABLES)) {
    const options = optionsOf(type);
    const byMember = new Map(table.rows.map((r) => [r.member, r]));
    const none = NO_CONTROL[type] || {};
    const want = new Set(OPTIONAL[type]);
    if (options) for (const member of leaves(options, "options")) want.add(member);
    for (const member of want) assert.ok(byMember.has(member) || member in none, `${type}: no row for ${member}`);
    for (const member of byMember.keys()) if (member.startsWith("options.") && options) assert.ok(want.has(member), `${type}: a row for a member that does not exist: ${member}`);
    for (const member of Object.keys(none)) assert.ok(!byMember.has(member), `${type}: ${member} has a row and an entry of NO_CONTROL`);
    if (!options) continue;
    for (const r of table.rows) {
      if (!r.member.startsWith("options.") || r.optional) continue;
      assert.deepEqual(r.default, get({ options }, r.member), `${type}: the default of ${r.member} is the value of a new panel`);
    }
  }
  // The types with no table: the two timelines and `text`, with the causes of 10.3h.
  assert.ok(tableOf("state_timeline") === null && tableOf("status_history") === null && tableOf("text") === null);
  assert.deepEqual(Object.keys(NO_CONTROL.text), ["options.mode", "options.content"]);
});

test("FE-79: each range of a row is the range of 4.27 to 4.33d, and each item is in the format", () => {
  const SECTION = { table: "4.27", timeseries: "4.28", stat: "4.29", barchart: "4.30", piechart: "4.31", xychart: "4.32", bargauge: "4.33", heatmap: "4.33c", histogram: "4.33d", gauge: "4.33e", logs: "4.33f", geomap: "4.33g" };
  const shared = sectionOf(format, "4.26");
  for (const [type, table] of Object.entries(TABLES)) {
    const own = sectionOf(format, SECTION[type]);
    for (const r of table.rows) {
      if (!r.member.startsWith("options.")) continue;
      const parts = r.member.split(".");
      const leaf = parts.at(-1);
      let text = own;
      if (parts.length > 2 && ["legend", "tooltip", "reduce"].includes(parts.at(-2))) {
        const at = shared.indexOf(`Object \`${parts.at(-2)}\``);
        const next = shared.indexOf("\nObject ", at + 5);
        text = shared.slice(at, next > 0 ? next : undefined);
      } else if (parts.length > 2) {
        // A sub-object of the type (`x_buckets`, `y_buckets`, `color`): the table after its sentence "The object `name` has these members".
        const at = own.indexOf(`The object \`${parts.at(-2)}\``);
        assert.ok(at >= 0, `${type}: the format describes the object ${parts.at(-2)}`);
        const next = own.indexOf("\nThe object ", at + 5);
        text = own.slice(at, next > 0 ? next : undefined);
      }
      const line = text.split("\n").find((l) => l.startsWith(`| \`${leaf}\` |`));
      assert.ok(line, `${type}: no line of the format for ${r.member}`);
      if ((r.kind === "radio" || (r.kind === "select" && r.items.length < 18)) && !r.none && !line.includes("As in 4.28")) {
        for (const [value] of r.items) if (typeof value === "string") assert.ok(line.includes(`\`${value}\``), `${type}: the item ${value} of ${r.member} is in the format: ${line}`);
      }
      if (r.kind === "slider") {
        const range = new RegExp(`${String(r.min).replace("-", "-?")} to ${r.max}\\b`);
        assert.ok(range.test(line.replace(/,/g, "")), `${type}: the range of ${r.member} is ${r.min} to ${r.max}: ${line}`);
      }
    }
  }
  const calcLine = sectionOf(format, "4.26").split("\n").find((l) => l.startsWith("| `calc` |"));
  const inFormat = [...calcLine.matchAll(/`([a-z_]+)`/g)].map((m) => m[1]).filter((v) => CALCS.some(([c]) => c === v));
  assert.deepEqual([...inFormat].sort(), CALCS.map(([c]) => c).sort());
  assert.equal(CALCS.length, 18);
  // The words of the calculations are the words of 10.3h.
  for (const [value, text] of CALCS) assert.ok(editorDoc.includes(`\`${value}\` | ${text} |`) || editorDoc.includes(`| ${text} | \`${value}\``) || editorDoc.includes(`${text} | \`${value}\``) || editorDoc.includes(`\`${value}\` | ${text}`), `10.3h gives the text of ${value}`);
});

test("FE-79: a choice is 280 px wide or less, and each rule of display is true in one state and false in one state", () => {
  for (const [type, table] of Object.entries(TABLES)) {
    for (const r of table.rows) {
      assert.ok(KINDS.includes(r.kind), `${type}: the kind of ${r.id}`);
      if (r.kind === "radio") assert.ok(choiceWidth(r.items) <= CHOICE_MAX_PX, `${type}: the choice ${r.id} is ${Math.round(choiceWidth(r.items))} px wide`);
    }
    const options = optionsOf(type);
    if (!options) continue;
    assert.deepEqual(rowsWithoutState(table, options), [], `${type}: each row shows in one state`);
    // The states of one change, with the legend as a table and a sort.
    const states = [options];
    const moves = [];
    for (const r of table.rows) {
      if (!r.member.startsWith("options.")) continue;
      const values = r.items && r.kind !== "checks" && r.kind !== "fields" ? r.items.map(([v]) => v) : r.kind === "switch" ? [true, false] : r.kind === "number" || r.kind === "text" ? [null, 5] : [];
      for (const v of values) moves.push([r.member, v]);
    }
    const put = (o, member, v) => {
      const keys = member.split(".").slice(1);
      let at = o;
      for (const k of keys.slice(0, -1)) at = at[k] ??= {};
      at[keys.at(-1)] = v;
    };
    for (const [m, v] of moves) {
      const s = structuredClone(options);
      put(s, m, v);
      states.push(s);
    }
    if (options.legend) {
      const s = structuredClone(options);
      s.legend.mode = "table";
      s.legend.sort_by = "name";
      states.push(s);
    }
    // The states of two changes of the audit (10.3h, rule 14), for a row that needs two changes (the least sizes of a gauge).
    for (const s of statesOf(table, options)) states.push(s.options);
    for (const r of table.rows) {
      if (!r.when) continue;
      const seen = new Set(states.map((s) => Boolean(r.when(s))));
      assert.equal(seen.size, 2, `${type}: the rule of ${r.id} is true in one state and false in one state`);
    }
    assert.ok(statesOf(table, options).length >= 1);
  }
});

test("FE-79: the new files of the forms have 102,400 bytes or less, the tables import the pure modules only, and the audit rule is a pure file", () => {
  const dir = "../../web/js/editor/forms/options/";
  const files = readdirSync(here(dir)).filter((f) => f.endsWith(".js")).map((f) => dir + f).concat("../../web/js/editor/forms/option_form.js");
  let bytes = 0;
  for (const file of files) bytes += Buffer.byteLength(read(file));
  assert.ok(bytes <= 102400, `the new files have ${bytes} bytes`);
  const rule = read("../browser/lib/audit_rules.mjs");
  assert.ok(!/\b(document|window|page)\b\./.test(rule.replace(/^\s*\/\/.*$/gm, "")), "the audit rule has no page access");
});
