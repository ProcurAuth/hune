// SPDX-License-Identifier: MIT OR Apache-2.0
// Review v3 of the page code, the low findings P3 and P4.
// P3: the author patterns run on names and cells that hold data. A text longer than 1,024 code units never matches (9.14, T87).
// P4: the variable bar and its popup look up the selection with a map and a set (9.10).
// Each test failed (or ran for many seconds) before the fix. The timing tests take the minimum of 7 runs after 2 warm-ups.
import test from "node:test";
import assert from "node:assert/strict";
import { FakeNode, installFakeDom } from "./fake_dom.mjs";
// Namespace imports: the files also load on the code before the fix (it had no regexExec), so each test can fail alone.
import * as F from "../../web/js/fields.js";
import { applyMappings } from "../../web/js/mappings.js";
import { baseDisplayNames, displayNames } from "../../web/js/names.js";
import { selectReduceFields } from "../../web/js/reduce.js";
import { filterByValue } from "../../web/js/transform.js";
import { applyMore } from "../../web/js/transform_more.js";
import { buildOptions } from "../../web/js/var_options.js";
import { createVarsBar, selectionText } from "../../web/js/vars.js";

const best = (fn) => {
  fn();
  fn();
  let min = Infinity;
  for (let i = 0; i < 7; i++) {
    const t0 = performance.now();
    fn();
    min = Math.min(min, performance.now() - t0);
  }
  return min;
};

const LIMIT = 1024;
const text = (name, values) => ({ name, type: "string", kind: "text", values, labels: null, config: {} });
const num = (name, values) => ({ name, type: "number", kind: "number", values: Float64Array.from(values), nulls: new Uint8Array(values.length), labels: null, config: {} });
const shownNames = (frames) => displayNames(frames[0]);

// ---- P3: the results for a long text in each place ----------------------------------------------------------------------------

// A name of 1,024 code units matches; a name of 1,025 never does. Both hold "-" and end with "-1".
const AT = `${"a".repeat(LIMIT - 2)}-1`;
const OVER = `${"a".repeat(LIMIT - 1)}-1`;

test("P3: regexExec and regexTest read 1,024 code units at most, and leave no lastIndex", () => {
  assert.equal(F.MAX_PATTERN_TEXT, LIMIT);
  const re = /(.*)-(.*)/g;
  assert.equal(F.regexExec(re, AT)[2], "1");
  assert.equal(re.lastIndex, 0);
  assert.equal(F.regexExec(re, OVER), null);
  assert.equal(F.regexTest(re, AT), true);
  assert.equal(F.regexTest(re, OVER), false);
  // A multi-byte name of 1,024 bytes (512 code units) is far under the limit: the document limit in bytes always fits.
  assert.equal(F.regexTest(/^é+$/, "é".repeat(512)), true);
});

test("P3: rename_by_regex keeps a name longer than 1,024 code units, and renames the name at the limit", () => {
  const frames = [[num(AT, [1]), num(OVER, [2]), num("web-1", [3])]];
  const out = applyMore(frames, { kind: "rename_by_regex", regex: "/(.*)-(.*)/", rename_pattern: "$2" });
  assert.deepEqual(shownNames(out), ["1", OVER, "1"]);
});

test("P3: filter_fields_by_name: an inclusion pattern drops a long name, an exclusion pattern keeps it", () => {
  const frames = [[num(AT, [1]), num(OVER, [2]), num("web-2", [3])]];
  const include = applyMore(frames, { kind: "filter_fields_by_name", include: { names: [], pattern: "/.*-1/" }, exclude: { names: [], pattern: null } });
  assert.deepEqual(shownNames(include), [AT]);
  const exclude = applyMore(frames, { kind: "filter_fields_by_name", include: { names: [], pattern: null }, exclude: { names: [], pattern: "/.*-1/" } });
  assert.deepEqual(shownNames(exclude), [OVER, "web-2"]);
});

test("P3: an override `regex` and the `fields` of a reduce option never match a long name", () => {
  const fields = [num(AT, [1]), num(OVER, [2])];
  const spec = { defaults: {}, overrides: [{ match: { by: "regex", value: "/.*-1/" }, set: { unit: "short" } }] };
  const { fields: out } = F.computeFieldConfig(fields, spec, "timeseries");
  assert.deepEqual(out.map((f) => f.config.unit), ["short", "none"]);
  assert.deepEqual(selectReduceFields(fields, baseDisplayNames(fields), "/.*-1/"), [0]);
});

test("P3: a `regex` value mapping never matches a raw text longer than 1,024 code units", () => {
  const mappings = [{ kind: "regex", pattern: "/(.*)-(.*)/", text: "$2", color: null }];
  assert.deepEqual(applyMappings(mappings, AT), { text: "1", color: null });
  assert.equal(applyMappings(mappings, OVER), null);
});

test("P3: filter_by_value: a `regex` matcher never passes a long cell (include drops the row, exclude keeps it)", () => {
  const fields = [text("host", [AT, OVER, "web-2"]), num("v", [1, 2, 3])];
  const spec = (mode) => ({ mode, match: "any", filters: [{ field: "host", matcher: { kind: "regex", pattern: "-1$" } }] });
  assert.deepEqual(Array.from(filterByValue(fields, spec("include"))[1].values), [1]);
  assert.deepEqual(Array.from(filterByValue(fields, spec("exclude"))[1].values), [2, 3]);
  // A pattern that does not compile never passes, as before.
  const bad = { mode: "include", match: "any", filters: [{ field: "host", matcher: { kind: "regex", pattern: "(" } }] };
  assert.equal(filterByValue(fields, bad)[1].values.length, 0);
});

test("P3: the `regex` of a query variable removes an option whose text is longer than 1,024 code units", () => {
  const raw = [{ value: "1", text: AT }, { value: "2", text: OVER }, { value: "3", text: "web-1" }];
  assert.deepEqual(buildOptions(raw, { regex: "/.*-1/", sort: "none" }).map((o) => o.value), ["1", "3"]);
  assert.deepEqual(buildOptions(raw, { regex: "/.*-(?<value>.*)/g", sort: "none" }).map((o) => o.value), ["1"]);
  // With no regex, the option stays: the limit applies to the pattern only.
  assert.deepEqual(buildOptions(raw, { regex: null, sort: "none" }).map((o) => o.value), ["1", "2", "3"]);
});

// ---- P3: the time on a label of 1,000,000 characters (a data value that long-to-wide puts in a display name) ---------------------

// The frame that long-to-wide makes of the rows (1, web-1, 1) and (2, HUGE, 2): the label `host` holds the data value (9.22).
const HUGE = "a".repeat(1000000);
const wide = [
  { name: "time", type: "time", kind: "time", values: Float64Array.of(1, 2), nulls: Uint8Array.of(0, 0), labels: null, config: {} },
  { ...num("value", [1, NaN]), labels: { host: "web-1" } },
  { ...num("value", [NaN, 2]), labels: { host: HUGE } },
];
const within = (label, fn) => {
  const ms = best(fn);
  assert.ok(ms <= 50, `${label}: ${ms.toFixed(1)} ms`);
};

test("P3: each pattern place takes 50 ms or less with a label or a cell of 1,000,000 characters", () => {
  assert.ok(displayNames(wide).some((n) => n.length > 1000000));
  const rename = { kind: "rename_by_regex", regex: "/(.*)-(.*)/", rename_pattern: "$2" };
  assert.deepEqual(displayNames(applyMore([wide], rename)[0]).map((n) => n.length), displayNames(wide).map((n, i) => (i === 1 ? 1 : n.length)));
  within("rename_by_regex /(.*)-(.*)/", () => applyMore([wide], rename));
  for (const pattern of ["/.*-1/", "/(.*)-(.*)/", "/^(a+)+$/"]) {
    within(`filter_fields_by_name include ${pattern}`, () => applyMore([wide], { kind: "filter_fields_by_name", include: { names: [], pattern }, exclude: { names: [], pattern: null } }));
    within(`filter_fields_by_name exclude ${pattern}`, () => applyMore([wide], { kind: "filter_fields_by_name", include: { names: [], pattern: null }, exclude: { names: [], pattern } }));
    within(`override ${pattern}`, () => F.computeFieldConfig(wide, { defaults: {}, overrides: [{ match: { by: "regex", value: pattern }, set: { unit: "short" } }] }, "timeseries"));
    within(`reduce fields ${pattern}`, () => selectReduceFields(wide, baseDisplayNames(wide), pattern));
    within(`value mapping ${pattern}`, () => applyMappings([{ kind: "regex", pattern, text: "$1", color: null }], HUGE));
    within(`variable regex ${pattern}`, () => buildOptions([{ value: "x", text: HUGE }], { regex: pattern, sort: "none" }));
  }
  const cells = [text("host", ["web-1", HUGE, HUGE]), num("v", [1, 2, 3])];
  for (const pattern of ["(.*)-(.*)", ".*-1", "^(a+)+$"]) {
    for (const mode of ["include", "exclude"]) {
      within(`filter_by_value ${mode} ${pattern}`, () => filterByValue(cells, { mode, match: "any", filters: [{ field: "host", matcher: { kind: "regex", pattern } }] }));
    }
  }
});

test("P3: filter_by_value compiles its pattern one time, not one time for each row", () => {
  const Real = globalThis.RegExp;
  let made = 0;
  globalThis.RegExp = new Proxy(Real, { construct(target, args) { made += 1; return new target(...args); } });
  try {
    const rows = Array.from({ length: 1000 }, (_, i) => `h-${i}`);
    filterByValue([text("host", rows)], { mode: "include", match: "all", filters: [{ field: "host", matcher: { kind: "regex", pattern: "-1" } }] });
  } finally {
    globalThis.RegExp = Real;
  }
  assert.ok(made <= 2, `${made} RegExp objects`);
});

// ---- P4: selectionText -------------------------------------------------------------------------------------------------------

// The code before the fix: the reference for the output on normal data.
function referenceText(options, selection) {
  const texts = selection.all ? ["All"] : selection.values.map((value) => {
    const option = options.find((o) => o.value === value);
    return option ? option.text : value;
  });
  const shown = texts.slice(0, 3).join(" + ");
  return texts.length > 3 ? `${shown} + ${texts.length - 3} more` : shown;
}

// A seeded generator (mulberry32): the same cases at each run.
function random(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

test("P4: selectionText gives the same text as before on 2,000 normal cases", () => {
  const next = random(48);
  const pick = (n) => Math.floor(next() * n);
  for (let c = 0; c < 2000; c++) {
    // Options with duplicate values and texts, and selections with values that are not options and duplicate values.
    const options = Array.from({ length: pick(8) }, () => ({ value: `v${pick(6)}`, text: `T${pick(9)}` }));
    const values = Array.from({ length: pick(7) }, () => `v${pick(9)}`);
    const selection = { all: next() < 0.1, values };
    assert.equal(selectionText({ kind: "query", name: "v" }, options, selection), referenceText(options, selection), JSON.stringify({ options, selection }));
  }
  assert.equal(selectionText({}, [{ value: "All", text: "x" }], { all: true, values: [] }), "All");
  assert.equal(selectionText({}, [], { all: false, values: [] }), "");
});

// 10,000 options of 1,005 characters with a common prefix of 1,000 (a query variable), and a selection of all of them. The
// selection values are new strings, as the URL gives them.
const N = 10000;
const PREFIX = "p".repeat(1000);
const longOptions = Array.from({ length: N }, (_, i) => {
  const value = `${PREFIX}${String(i).padStart(5, "0")}`;
  return { value, text: value };
});
const copies = () => longOptions.map((o) => (` ${o.value}`).slice(1));

test("P4: selectionText with 10,000 selected values and 10,000 options of 1,005 characters takes 100 ms or less", () => {
  const selection = { all: false, values: copies() };
  const got = selectionText({ kind: "query", name: "v", multi: true }, longOptions, selection);
  assert.equal(got, `${longOptions.slice(0, 3).map((o) => o.text).join(" + ")} + 9997 more`);
  const ms = best(() => selectionText({ kind: "query", name: "v", multi: true }, longOptions, selection));
  assert.ok(ms <= 100, `${ms.toFixed(1)} ms`);
});

// ---- P4: the popup, with the fake DOM ---------------------------------------------------------------------------------------

// The parts of the DOM that the popup uses and fake_dom.mjs does not have: classList, prepend, isConnected and window.
installFakeDom();
Object.defineProperty(FakeNode.prototype, "classList", {
  get() {
    const node = this;
    const list = () => node.className.split(/\s+/).filter(Boolean);
    const put = (on, c) => { node.className = (on ? [...new Set([...list(), c])] : list().filter((x) => x !== c)).join(" "); };
    return { add: (c) => put(true, c), remove: (c) => put(false, c), toggle: (c, on = !list().includes(c)) => put(on, c), contains: (c) => list().includes(c) };
  },
});
FakeNode.prototype.prepend = function prepend(...nodes) {
  const rest = this.children;
  this.children = [];
  this.append(...nodes);
  this.children.push(...rest);
};
Object.defineProperty(FakeNode.prototype, "isConnected", { get: () => false });
globalThis.window = { addEventListener() {}, removeEventListener() {} };

// A bar with one multi variable, as the page runtime gives it (selections, options, optionErrors).
function barOf(variable, options, selection) {
  const rt = {
    variables: [variable],
    varByName: new Map([[variable.name, variable]]),
    selections: new Map([[variable.name, selection]]),
    options: new Map([[variable.name, options]]),
    optionErrors: new Map(),
    on: () => () => {},
  };
  const applied = [];
  const bar = createVarsBar(rt, (name, s) => applied.push([name, s]));
  const at = (id) => bar.el.querySelector(`[data-testid="${id}"]`);
  return { bar, at, applied };
}

const stateOf = (at, name) => {
  const rows = at(`variable-${name}-button`).parent.querySelectorAll("li");
  const all = at(`variable-${name}-selected`);
  return {
    selected: rows.filter((r) => r.getAttribute("aria-selected") === "true").map((r) => r.dataset.value),
    count: all.querySelector(".select__all-text").textContent,
    pressed: all.getAttribute("aria-pressed"),
    label: all.getAttribute("aria-label"),
  };
};

test("P4: the popup marks the same rows as before, after clicks on rows and on the row Selected (n)", () => {
  const options = ["a", "b", "c", "d"].map((v) => ({ value: v, text: v.toUpperCase() }));
  const variable = { kind: "query", name: "h", label: null, hide: "none", multi: true, include_all: false, allow_custom_value: false };
  const { at } = barOf(variable, options, { all: false, values: ["b", "d"] });
  assert.equal(at("variable-h-button").textContent, "B + D");
  at("variable-h-button").dispatch("click");
  assert.deepEqual(stateOf(at, "h"), { selected: ["b", "d"], count: "Selected (2)", pressed: "false", label: "Selected (2). Select all options" });
  const row = (v) => at("variable-h-button").parent.querySelectorAll("li").find((r) => r.dataset.value === v);
  row("a").dispatch("click");
  assert.deepEqual(stateOf(at, "h").selected, ["a", "b", "d"]);
  row("b").dispatch("click");
  assert.deepEqual(stateOf(at, "h").selected, ["a", "d"]);
  at("variable-h-selected").dispatch("click");
  assert.deepEqual(stateOf(at, "h"), { selected: ["a", "b", "c", "d"], count: "Selected (4)", pressed: "true", label: "Selected (4). Keep the first option only" });
  at("variable-h-selected").dispatch("click");
  assert.deepEqual(stateOf(at, "h"), { selected: ["a"], count: "Selected (1)", pressed: "false", label: "Selected (1). Select all options" });
});

test("P4: the popup with `include_all` marks the row All, and no other row", () => {
  const options = ["a", "b"].map((v) => ({ value: v, text: v }));
  const variable = { kind: "query", name: "k", label: null, hide: "none", multi: true, include_all: true, allow_custom_value: false };
  const { at } = barOf(variable, options, { all: true, values: [] });
  at("variable-k-button").dispatch("click");
  assert.deepEqual(stateOf(at, "k"), { selected: ["$__all"], count: "Selected (1)", pressed: "false", label: "Selected (1). Keep the first option only" });
});

test("P4: the popup with 10,000 selected values of 1,005 characters: a key in the search field takes 100 ms or less", () => {
  const variable = { kind: "query", name: "big", label: null, hide: "none", multi: true, include_all: false, allow_custom_value: false };
  const { at } = barOf(variable, longOptions, { all: false, values: copies() });
  at("variable-big-button").dispatch("click");
  const search = at("variable-big-search");
  assert.equal(stateOf(at, "big").count, `Selected (${N})`);
  assert.equal(stateOf(at, "big").pressed, "true");
  search.value = "p";
  const ms = best(() => search.dispatch("input"));
  assert.ok(ms <= 100, `${ms.toFixed(1)} ms`);
  assert.equal(stateOf(at, "big").selected.length, N);
});
