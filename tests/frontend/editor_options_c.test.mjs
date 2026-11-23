// SPDX-License-Identifier: MIT OR Apache-2.0
// The option forms of `table`, `heatmap` and `histogram` (10.3h, F43, vector set FE-78): the tables against the format and the new panel, the write
// rule and the rules of display on the vectors, the grammar of the buckets, and the rule that each write keeps a heatmap valid (4.33c).
// The page does not draw `heatmap` and `histogram` yet, so the "new" options come from the first panel of their fixtures (the defaults of 9.19e, 9.19f).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, statSync } from "node:fs";
import { newOptions } from "../../web/js/editor/draft.js";
import table from "../../web/js/editor/forms/options/table.js";
import heatmap, { X_SIZE_DEFAULT, X_SIZE_MAX, formatDuration, parseDuration, yScaleWrites } from "../../web/js/editor/forms/options/heatmap.js";
import histogram from "../../web/js/editor/forms/options/histogram.js";
import { MAX_FOOTER, MAX_SORT_KEYS, newSortKey } from "../../web/js/editor/forms/options/table.js";
import { CALCS, CHOICE_MAX_PX, KINDS, choiceWidth, parseText, rowsWithoutState, shownIds, statesOf, valueOf, writeOption } from "../../web/js/editor/forms/options/rows.js";

const TABLES = { table, heatmap, histogram };
const here = (p) => new URL(p, import.meta.url);
const vectors = JSON.parse(readFileSync(here("./vectors/FE-78-options-c.json"), "utf8"));
const format = readFileSync(here("../../docs/02-format-and-storage.md"), "utf8");
const section = (id) => {
  const a = format.indexOf(`### ${id} `);
  return format.slice(a, format.indexOf("\n### ", a + 5));
};
const fixtureOptions = (name) => JSON.parse(readFileSync(here(`../fixtures/dashboards/${name}.json`), "utf8")).sections[0].panels[0].options;
const NEW = { table: newOptions("table"), heatmap: fixtureOptions("heatmap"), histogram: fixtureOptions("histogram") };
const get = (object, member) => member.split(".").reduce((at, key) => (at == null ? undefined : at[key]), object);

function leaves(value, path, out = []) {
  if (value && typeof value === "object" && !Array.isArray(value)) for (const [k, v] of Object.entries(value)) leaves(v, `${path}.${k}`, out);
  else out.push(path);
  return out;
}

// The hooks of 10.3h and of the text of F31, in the order of the tables.
const HOOKS = {
  table: ["opt-show-header", "opt-sort", "opt-col-width", "opt-col-align", "opt-wrap", "opt-filterable", "opt-footer", "defaults-cell-type", "opt-inspect", "opt-display-name"],
  heatmap: [
    "heat-calculate", "heat-x-mode", "heat-x-value", "heat-y-mode", "heat-y-scale", "heat-y-value", "heat-y-axis", "heat-y-reverse", "heat-y-min", "heat-y-max",
    "heat-color-mode", "heat-scheme", "heat-fill", "heat-scale", "heat-exponent", "heat-steps", "heat-reverse", "heat-min", "heat-max",
    "heat-gap", "heat-hide-le", "heat-hide-ge", "opt-tooltip-mode", "heat-legend", "opt-display-name",
  ],
  histogram: [
    "hist-count", "hist-size", "hist-offset", "hist-combine", "hist-stacking", "hist-line-width", "hist-fill-opacity", "opt-tooltip-mode", "opt-tooltip-sort",
    "opt-tooltip-hide-zeros", "opt-legend-show", "opt-legend-mode", "opt-legend-placement", "opt-legend-values", "opt-legend-sort-by", "opt-legend-sort-desc", "opt-display-name",
  ],
};
const LABELS = {
  table: ["Show table header", "Sort by", "Column width", "Column alignment", "Wrap text", "Column filter", "Calculation", "Cell type", "Cell value inspect", "Display name"],
  heatmap: [
    "Calculate from data", "X Bucket", "X Bucket value", "Y Bucket", "Y Bucket scale", "Y Bucket value", "Placement", "Reverse", "Min value", "Max value", "Mode", "Scheme", "Color", "Scale",
    "Exponent", "Steps", "Reverse", "Start color scale from value", "End color scale at value", "Cell gap", "Hide cells with values <=", "Hide cells with values >=", "Tooltip mode", "Show legend", "Display name",
  ],
  histogram: [
    "Bucket count", "Bucket size", "Bucket offset", "Combine series", "Stacking", "Line width", "Fill opacity", "Tooltip mode", "Values sort order", "Hide zeros", "Visibility", "Mode", "Placement", "Values",
    "Sort by", "Sort descending", "Display name",
  ],
};
const GROUPS = {
  table: [["table", "Table"], ["footer", "Table footer"], ["cells", "Cell options"]],
  heatmap: [["heat", "Heatmap"], ["yaxis", "Y axis"], ["colors", "Colors"], ["cells", "Cell display"], ["tooltip", "Tooltip"], ["legend", "Legend"]],
  histogram: [["hist", "Histogram"], ["tooltip", "Tooltip"], ["legend", "Legend"]],
};

test("FE-78: the tables have the groups, the labels and the hooks of 10.3h", () => {
  for (const [type, t] of Object.entries(TABLES)) {
    assert.equal(t.type, type);
    assert.deepEqual(t.groups, GROUPS[type]);
    assert.deepEqual(t.rows.map((r) => r.hook), HOOKS[type], `${type}: the hooks`);
    assert.deepEqual(t.rows.map((r) => r.label), LABELS[type], `${type}: the labels`);
  }
  assert.equal(table.rows.length + heatmap.rows.length + histogram.rows.length, 52);
});

test("FE-78: the ids and the hooks are unique, each kind is known, each group has a row, the rows follow the groups", () => {
  for (const [type, t] of Object.entries(TABLES)) {
    const ids = new Set();
    const hooks = new Set();
    const groups = new Set(t.groups.map(([key]) => key).concat("standard"));
    for (const r of t.rows) {
      assert.ok(!ids.has(r.id), `${type}: the id ${r.id}`);
      ids.add(r.id);
      assert.ok(!hooks.has(r.hook), `${type}: the hook ${r.hook}`);
      hooks.add(r.hook);
      assert.ok(KINDS.includes(r.kind), `${type}: the kind of ${r.id}`);
      assert.ok(groups.has(r.group), `${type}: the group of ${r.id}`);
      assert.ok(!r.when || r.whenText, `${type}: the text of the rule of ${r.id}`);
      assert.ok(r.kind !== "custom" || r.colour || r.hook.startsWith("defaults-") || typeof r.build === "function", `${type}: ${r.id} has a builder, a colour or an old control`);
      assert.ok(r.member.startsWith("options.") || r.member.startsWith("fields.defaults."), `${type}: the member of ${r.id}`);
      if (r.items) assert.ok(r.items.every(([, text]) => typeof text === "string" && text !== ""), `${type}: the items of ${r.id}`);
    }
    const keys = t.groups.map(([key]) => key);
    for (const key of keys) assert.ok(t.rows.some((r) => r.group === key), `${type}: the group ${key} has a row`);
    const order = t.rows.map((r) => keys.indexOf(r.group)).filter((i) => i >= 0);
    assert.deepEqual(order, [...order].sort((a, b) => a - b), `${type}: the rows follow the groups`);
  }
});

// The members with no row: the field settings that the type does not show (4.33c rule 6, 4.27).
const NO_CONTROL = { table: ["fields.defaults.hidden"], heatmap: [], histogram: [] };
const OPTIONAL = { table: ["options.wrap"], heatmap: [], histogram: ["options.tooltip.sort", "options.tooltip.hide_zeros", "options.legend.sort_by", "options.legend.sort_desc"] };

test("FE-78: each member of a new panel has a row, and each default equals the new panel", () => {
  for (const [type, t] of Object.entries(TABLES)) {
    const options = NEW[type];
    const byMember = new Map(t.rows.map((r) => [r.member, r]));
    assert.equal(byMember.size, t.rows.length, `${type}: one row for each member`);
    const want = new Set([...leaves(options, "options"), ...OPTIONAL[type]]);
    for (const member of want) assert.ok(byMember.has(member), `${type}: no row for ${member}`);
    for (const [member, r] of byMember) if (member.startsWith("options.")) assert.ok(want.has(member), `${type}: a row for a member that does not exist: ${member}`);
    for (const r of t.rows) {
      if (!r.member.startsWith("options.") || r.optional) continue;
      assert.deepEqual(r.default, get({ options }, r.member), `${type}: the default of ${r.member}`);
      assert.deepEqual(valueOf({ options }, r), r.default, `${type}: the value of a new panel for ${r.member}`);
    }
    for (const member of NO_CONTROL[type]) assert.ok(!byMember.has(member));
  }
});

test("FE-78: the table of the type `table` is 4.27 and 4.18: the members, the ranges and the items", () => {
  const own = section("4.27");
  for (const leaf of ["show_header", "sort", "wrap"]) assert.ok(own.includes(`| \`${leaf}\` |`));
  const props = section("4.18");
  const line = (id) => props.split("\n").find((l) => l.startsWith(`| \`${id}\` |`));
  assert.match(line("width"), /10 to 4000/);
  for (const v of ["auto", "left", "center", "right"]) assert.ok(line("align").includes(`\`${v}\``));
  assert.match(line("display_name"), /1 to 256/);
  assert.ok(line("filterable") && line("footer") && line("cell"));
  const width = table.rows.find((r) => r.id === "col-width");
  assert.deepEqual([width.min, width.max, width.integer], [10, 4000, true]);
  assert.ok(props.includes("1 to 8 (`MAX_FOOTER_REDUCERS`)"));
  assert.equal(MAX_FOOTER, 8);
  assert.ok(own.includes("0 to 8 sort keys"));
  assert.equal(MAX_SORT_KEYS, 8);
  assert.deepEqual(table.rows.find((r) => r.id === "footer").items, CALCS);
  assert.deepEqual(newSortKey(["Time", "Value"]), { field: "Time", desc: false });
  assert.deepEqual(newSortKey([]), { field: "Value", desc: false });
});

test("FE-78: the heatmap and the histogram have the members, the items and the ranges of 4.33c and 4.33d", () => {
  for (const [type, id] of [["heatmap", "4.33c"], ["histogram", "4.33d"]]) {
    const text = section(id);
    for (const r of TABLES[type].rows) {
      if (!r.member.startsWith("options.")) continue;
      const parts = r.member.split(".");
      const leaf = parts.at(-1);
      // A member of an object is in the table of the object (`x_buckets`, `y_buckets`, `color`), the others in the main table. 4.26 holds `legend` and `tooltip`.
      if (["legend", "tooltip"].includes(parts.at(-2))) continue;
      const scope = parts.length > 2 ? text.slice(text.indexOf(`The object \`${parts.at(-2)}\``)) : text;
      const line = scope.split("\n").find((l) => l.startsWith(`| \`${leaf}\` |`));
      assert.ok(line, `${type}: no line of the format for ${r.member}`);
      if (r.items) for (const [value] of r.items) if (typeof value === "string") assert.ok(line.includes(`\`${value}\``), `${type}: the item ${value} of ${r.member}: ${line}`);
      if (r.kind === "slider") assert.ok(new RegExp(`${r.min} to ${r.max}\\b`).test(line.replace(/,/g, "")), `${type}: the range of ${r.member}: ${line}`);
    }
  }
  const byId = (t, id) => t.rows.find((r) => r.id === id);
  assert.deepEqual([byId(heatmap, "color-steps").min, byId(heatmap, "color-steps").max], [2, 128]);
  assert.deepEqual([byId(heatmap, "cell-gap").min, byId(heatmap, "cell-gap").max], [0, 25]);
  assert.deepEqual([byId(heatmap, "color-exponent").min, byId(heatmap, "color-exponent").max], [0.1, 2]);
  assert.deepEqual([byId(histogram, "bucket-count").min, byId(histogram, "bucket-count").max], [1, 1000]);
  assert.deepEqual([byId(histogram, "line-width").min, byId(histogram, "line-width").max], [0, 10]);
  assert.deepEqual([byId(histogram, "fill-opacity").min, byId(histogram, "fill-opacity").max], [0, 100]);
  assert.deepEqual(byId(heatmap, "color-scheme").items.map(([v]) => v), ["oranges", "blues", "greens", "greys", "purples", "reds", "yl_or_rd", "rd_yl_gn", "spectral", "viridis"]);
  // The tooltip of a heatmap has no sort and no zeros (4.33c, rule 4).
  assert.deepEqual(heatmap.rows.filter((r) => r.group === "tooltip").map((r) => r.id), ["tooltip-mode"]);
  assert.equal(heatmap.rows.some((r) => r.member.startsWith("options.legend.")), false);
});

test("FE-78: the choices are 280 px wide at most, and each rule of display is true in one state and false in one state", () => {
  for (const [type, t] of Object.entries(TABLES)) {
    for (const r of t.rows) if (r.kind === "radio") assert.ok(choiceWidth(r.items) <= CHOICE_MAX_PX, `${type}: the choice ${r.id} is ${Math.round(choiceWidth(r.items))} px wide`);
    const options = NEW[type];
    assert.deepEqual(rowsWithoutState(t, options), [], `${type}: each row shows in one state`);
    const seen = new Map();
    for (const state of statesOf(t, options)) for (const r of t.rows) if (r.when) (seen.get(r.id) || seen.set(r.id, new Set()).get(r.id)).add(Boolean(r.when(state.options)));
    // The states of the audit show each row. A rule must also be false in a state that the moves of a choice or a switch reach: test the others by hand.
    const hide = {
      "x-mode": (o) => ({ ...o, calculate: false }),
      "x-value": (o) => ({ ...o, calculate: false }),
      "y-mode": (o) => ({ ...o, y_buckets: { ...o.y_buckets, scale: "log2" } }),
      "y-scale": (o) => ({ ...o, calculate: false }),
      "y-value": (o) => ({ ...o, calculate: false }),
      "y-min": (o) => ({ ...o, calculate: false }),
      "y-max": (o) => ({ ...o, calculate: false }),
      "color-scheme": (o) => ({ ...o, color: { ...o.color, mode: "opacity" } }),
      "color-fill": (o) => o,
      "color-scale": (o) => o,
      "color-exponent": (o) => o,
      "bucket-count": (o) => ({ ...o, bucket_size: 10 }),
      stacking: (o) => ({ ...o, combine: true }),
    };
    for (const r of t.rows) {
      if (!r.when) continue;
      const states = [...(seen.get(r.id) || [])];
      const falseSomewhere = states.includes(false) || (hide[r.id] && r.when(hide[r.id](structuredClone(options))) === false);
      assert.ok(falseSomewhere || ["tooltip-sort", "tooltip-hide-zeros", "legend-mode", "legend-placement", "legend-values", "legend-sort-by", "legend-sort-desc", "color-fill", "color-scale", "color-exponent"].includes(r.id), `${type}: ${r.id} hides in one state`);
      assert.ok(r.when(structuredClone(options)) || states.includes(true), `${type}: ${r.id} shows in one state`);
    }
  }
  // The rules of the colour: the mode "opacity" shows the colour and the scale, and the exponent needs the scale "exponential".
  const c = (patch) => ({ ...structuredClone(NEW.heatmap), color: { ...NEW.heatmap.color, ...patch } });
  const when = (id, o) => heatmap.rows.find((r) => r.id === id).when(o);
  assert.deepEqual(["color-scheme", "color-fill", "color-scale", "color-exponent"].map((id) => when(id, c({}))), [true, false, false, false]);
  assert.deepEqual(["color-scheme", "color-fill", "color-scale", "color-exponent"].map((id) => when(id, c({ mode: "opacity" }))), [false, true, true, true]);
  assert.deepEqual(["color-scheme", "color-fill", "color-scale", "color-exponent"].map((id) => when(id, c({ mode: "opacity", scale: "linear" }))), [false, true, true, false]);
  // The rule of the Y mode: a log scale has no mode "count".
  const y = (patch) => ({ ...structuredClone(NEW.heatmap), calculate: true, y_buckets: { ...NEW.heatmap.y_buckets, ...patch } });
  assert.equal(when("y-mode", y({})), true);
  assert.equal(when("y-mode", y({ scale: "log2", mode: "size" })), false);
  assert.equal(when("y-mode", { ...y({}), calculate: false }), false);
});

test("FE-78: a duration has one count and one unit (4.4), in milliseconds", () => {
  for (const [text, ms] of [["1ms", 1], ["999ms", 999], ["1s", 1000], ["90s", 90000], ["10m", 600000], ["1h", 3600000], ["365d", 31_536_000_000], ["999999999ms", 999999999]]) assert.equal(parseDuration(text), ms, text);
  for (const bad of ["", "0s", "01s", "1m30s", "10", "10 m", "1w", "1M", "-1s", "1.5h", "1000000000ms", "m"]) assert.equal(parseDuration(bad), undefined, JSON.stringify(bad));
  for (const [ms, text] of [[1, "1ms"], [1000, "1s"], [1500, "1500ms"], [600000, "10m"], [3_600_000, "1h"], [86_400_000, "1d"], [31_536_000_000, "365d"], [90_000, "90s"]]) assert.equal(formatDuration(ms), text, String(ms));
  for (const ms of [1, 7, 1000, 60_000, 123_456, 3_600_000, 86_400_000, 31_536_000_000]) assert.equal(parseDuration(formatDuration(ms)), ms);
  assert.equal(X_SIZE_DEFAULT, 600_000);
  assert.equal(X_SIZE_MAX, 31_536_000_000);
});

const panelOf = (v) => ({ id: 1, type: v.type, title: "t", options: structuredClone(v.options), fields: { defaults: structuredClone(v.defaults), overrides: [] }, links: [] });
const rowOf = (type, id) => TABLES[type].rows.find((r) => r.id === id);
const valueOfVector = (v) => (v.value && typeof v.value === "object" && !Array.isArray(v.value) && v.value.unset === true ? undefined : v.value);

test("FE-78: the vectors give the write rule, the order of 4.38 and the rules of display", () => {
  const writes = vectors.filter((v) => v.kind === "write");
  const parses = vectors.filter((v) => v.kind === "parse");
  assert.ok(writes.length >= 50, `${writes.length} write vectors`);
  assert.ok(parses.length >= 45, `${parses.length} parse vectors`);
  const names = new Set();
  for (const v of vectors) {
    assert.ok(!names.has(`${v.kind}|${v.name}`), `the name ${v.name} is unique`);
    names.add(`${v.kind}|${v.name}`);
    assert.ok(rowOf(v.type, v.row), `${v.name}: the row ${v.row}`);
  }
  for (const v of writes) {
    const before = panelOf(v);
    const snapshot = JSON.stringify(before);
    const next = writeOption(before, rowOf(v.type, v.row), valueOfVector(v));
    assert.equal(JSON.stringify(before), snapshot, `${v.name}: the write does not change its input`);
    assert.equal(JSON.stringify(next.options), JSON.stringify(v.options_after), `${v.name}: the options (with the order)`);
    assert.equal(JSON.stringify(next.fields.defaults), JSON.stringify(v.defaults_after), `${v.name}: the defaults (with the order)`);
    assert.deepEqual(shownIds(TABLES[v.type], v.options), v.shown_before, `${v.name}: the rows shown before`);
    assert.deepEqual(shownIds(TABLES[v.type], next.options), v.shown_after, `${v.name}: the rows shown after`);
  }
  for (const v of parses) {
    const r = parseText(rowOf(v.type, v.row), v.text, structuredClone(v.options));
    if ("error" in v) {
      assert.equal(r.error, v.error, `${v.name}: the error`);
      assert.equal("value" in r, false, `${v.name}: an error has no value`);
    } else assert.deepEqual(r, { value: v.value }, `${v.name}: the value`);
  }
});

test("FE-78: the cases of the vectors that the rules of 10.3h name", () => {
  const by = (name) => vectors.find((v) => v.name === name);
  // An optional member leaves the panel with its absent value.
  assert.equal("wrap" in by("table: wrap off removes the member").options_after, false);
  assert.equal("width" in by("table: empty width removes the member").defaults_after, false);
  assert.equal("footer" in by("table: empty footer removes the member").defaults_after, false);
  assert.deepEqual(by("table: footer with two calculations").defaults_after.footer, { reducers: ["min", "max"] });
  // A list that is empty keeps its member, because `sort` is necessary (4.27).
  assert.deepEqual(by("table: empty sort keeps the member").options_after.sort, []);
  // The row "Bucket count" hides while the size is a number, and the member stays.
  const size = by("histogram: size 25 hides the count");
  assert.equal(size.shown_before.includes("bucket-count"), true);
  assert.equal(size.shown_after.includes("bucket-count"), false);
  assert.equal(size.options_after.bucket_count, 30);
  // The log scale writes the mode `size` (4.33c, rule 2).
  assert.equal(by("heatmap: log2 writes the mode size").options_after.y_buckets.mode, "size");
  // A hidden row keeps its member.
  assert.equal(by("histogram: combine hides the stacking").options_after.stacking, "none");
  assert.equal(by("heatmap: scale linear hides the exponent").options_after.color.exponent, 0.5);
});

// The rules of 4.33c for the buckets, as a check of the document that the writes make.
function heatErrors(o) {
  const errors = [];
  const { x_buckets: x, y_buckets: y } = o;
  const int = (v, lo, hi) => Number.isInteger(v) && v >= lo && v <= hi;
  if (x.mode === "count" ? !(x.value === null || int(x.value, 1, 1000)) : !int(x.value, 1, X_SIZE_MAX)) errors.push(`x_buckets/value ${x.mode} ${x.value}`);
  if (y.scale !== "linear" && y.mode !== "size") errors.push("y_buckets/mode: a log scale has the mode size");
  if (y.scale !== "linear" && !(y.value === null || int(y.value, 1, 16))) errors.push(`y_buckets/value log ${y.value}`);
  if (y.scale === "linear" && y.mode === "count" && !(y.value === null || int(y.value, 1, 1000))) errors.push(`y_buckets/value count ${y.value}`);
  if (y.scale === "linear" && y.mode === "size" && !(typeof y.value === "number" && y.value > 0)) errors.push(`y_buckets/value size ${y.value}`);
  return errors;
}

test("FE-78: each change of a mode or a scale of the buckets keeps a valid heatmap, in 20,000 random changes", () => {
  let seed = 78;
  const rnd = (n) => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed % n; };
  const choices = {
    "x-mode": ["size", "count"], "y-mode": ["size", "count"], "y-scale": ["linear", "log2", "log10"],
    "x-value": ["", "1", "1000", "10m", "1h", "365d", "5", "2.5", "0"], "y-value": ["", "1", "16", "17", "500", "0.25", "2.5", "0"],
  };
  const ids = Object.keys(choices);
  let panel = { type: "heatmap", options: structuredClone(NEW.heatmap), fields: { defaults: {}, overrides: [] } };
  panel.options.calculate = true;
  assert.deepEqual(heatErrors(panel.options), []);
  let applied = 0;
  for (let i = 0; i < 20000; i++) {
    const id = ids[rnd(ids.length)];
    const r = rowOf("heatmap", id);
    // A row that the rules hide has no control, so the person cannot change it.
    if (!r.when(panel.options)) continue;
    const pick = choices[id][rnd(choices[id].length)];
    let value = pick;
    if (id.endsWith("value")) {
      const parsed = parseText(r, pick, panel.options);
      if ("error" in parsed) continue;
      value = parsed.value;
    }
    panel = writeOption(panel, r, value);
    applied++;
    assert.deepEqual(heatErrors(panel.options), [], `change ${i}: ${id} = ${pick}`);
  }
  assert.ok(applied > 5000, `${applied} changes applied`);
});

test("FE-78: the writes of the Y scale", () => {
  const o = (patch) => ({ y_buckets: { mode: "count", value: null, scale: "linear", ...patch } });
  assert.deepEqual(yScaleWrites("log2", o({})), { "options.y_buckets.mode": "size", "options.y_buckets.value": null });
  assert.deepEqual(yScaleWrites("log10", o({ value: 16 })), { "options.y_buckets.mode": "size", "options.y_buckets.value": 16 });
  assert.deepEqual(yScaleWrites("log10", o({ value: 17 })), { "options.y_buckets.mode": "size", "options.y_buckets.value": null });
  assert.deepEqual(yScaleWrites("log10", o({ value: 2.5 })), { "options.y_buckets.mode": "size", "options.y_buckets.value": null });
  assert.deepEqual(yScaleWrites("linear", o({ mode: "size", scale: "linear", value: null })), { "options.y_buckets.value": 1 });
  assert.deepEqual(yScaleWrites("linear", o({ mode: "size", value: 3 })), { "options.y_buckets.value": 3 });
  assert.deepEqual(yScaleWrites("linear", o({ mode: "count", value: 2000 })), { "options.y_buckets.value": null });
});

test("FE-78: the audit list of group C is closed and has valid entries", async () => {
  const { EXEMPT, HOVER } = await import("../browser/lib/audit_c.mjs");
  const seen = new Set();
  for (const e of EXEMPT) {
    assert.ok(["design", "data"].includes(e.cls), e.pointer);
    assert.ok(e.cause.length >= 20, `${e.pointer}: the cause`);
    assert.ok(e.types.every((t) => TABLES[t]), `${e.pointer}: the types`);
    const key = `${e.pointer}|${e.types.join()}`;
    assert.ok(!seen.has(key), key);
    seen.add(key);
  }
  assert.ok(EXEMPT.length <= 30);
  for (const type of Object.keys(HOVER)) assert.ok(TABLES[type]);
  assert.equal("table" in HOVER, false, "a table has no tooltip");
});

test("FE-78: the files of group C are small", () => {
  let bytes = 0;
  for (const f of ["table", "heatmap", "histogram"]) bytes += statSync(here(`../../web/js/editor/forms/options/${f}.js`)).size;
  assert.ok(bytes < 22000, `${bytes} bytes`);
});
