// SPDX-License-Identifier: MIT OR Apache-2.0
// The short address (9.5): the page writes a query name only when its value differs from the default of the dashboard.
import test from "node:test";
import assert from "node:assert/strict";
import { buildSearch, shortQuery } from "../../web/js/url_state.js";
import { defaultSelection, sameSelection, selectionToUrl } from "../../web/js/var_options.js";
import { RuntimeWithNames as Runtime } from "./helpers.mjs";

// The defaults of a dashboard: the range now-24h to now, the zone "utc", no refresh, and two variables with a known default.
const defaults = () => ({
  from: "now-24h",
  to: "now",
  timezone: null,
  vars: new Map([["region", ["$__all"]], ["env", ["prod"]]]),
  refresh: "off",
});
// The query of a page that shows the defaults.
const query = (change = {}) => ({
  from: "now-24h",
  to: "now",
  timezone: null,
  vars: new Map([["region", ["$__all"]], ["env", ["prod"]]]),
  refresh: "off",
  ...change,
});
const names = ["region", "env"];
const address = (q, view = {}) => buildSearch({ ...shortQuery(q, defaults()), ...view }, names);

test("all values are the defaults: the address has the view names only", () => {
  assert.equal(address(query()), "");
  assert.equal(address(query(), { edit: true }), "?edit=1");
  assert.equal(address(query(), { viewPanel: 3 }), "?viewPanel=3");
});

test("the range is one unit: a different from or to writes both", () => {
  assert.equal(address(query({ from: "now-6h" })), "?from=now-6h&to=now");
  assert.equal(address(query({ to: "now-1h" })), "?from=now-24h&to=now-1h");
  assert.equal(address(query({ from: "1794211200000", to: "1794232800000" }), { edit: true }), "?from=1794211200000&to=1794232800000&edit=1");
});

test("a refresh that differs from the default is written, and the same value is not", () => {
  assert.equal(address(query({ refresh: "30s" })), "?refresh=30s");
  assert.equal(address(query({ refresh: "off" })), "");
  // The document has a refresh of 1m: "off" differs from it and is written.
  const withRefresh = { ...defaults(), refresh: "1m" };
  assert.equal(buildSearch(shortQuery(query({ refresh: "1m" }), withRefresh), names), "");
  assert.equal(buildSearch(shortQuery(query({ refresh: "off" }), withRefresh), names), "?refresh=off");
});

test("a variable that differs is written in the order of the document; the default is not", () => {
  assert.equal(address(query({ vars: new Map([["region", ["eu-west"]], ["env", ["prod"]]]) })), "?var-region=eu-west");
  assert.equal(address(query({ vars: new Map([["region", ["eu-west"]], ["env", ["dev"]]]) })), "?var-region=eu-west&var-env=dev");
  assert.equal(address(query({ vars: new Map([["region", ["$__all"]], ["env", ["dev"]]]) })), "?var-env=dev");
});

test("a multi value is the default only with the same values in the same order", () => {
  const multi = { ...defaults(), vars: new Map([["region", ["a", "b"]]]) };
  const write = (values) => buildSearch(shortQuery({ ...query(), vars: new Map([["region", values]]) }, multi), ["region"]);
  assert.equal(write(["a", "b"]), "");
  assert.equal(write(["b", "a"]), "?var-region=b&var-region=a");
  assert.equal(write(["a"]), "?var-region=a");
  assert.equal(write(["a", "b", "c"]), "?var-region=a&var-region=b&var-region=c");
});

test("All is the default when the document default is All, and a value is not", () => {
  assert.equal(address(query({ vars: new Map([["region", ["$__all"]], ["env", ["prod"]]]) })), "");
  assert.equal(address(query({ vars: new Map([["region", ["us-east"]], ["env", ["prod"]]]) })), "?var-region=us-east");
});

test("a variable with no known default stays in the address", () => {
  // The options of a query variable are not loaded: the page has no default for it, so it keeps the name.
  const unknown = { ...defaults(), vars: new Map([["env", ["prod"]]]) };
  const out = buildSearch(shortQuery(query(), unknown), names);
  assert.equal(out, "?var-region=%24__all");
});

test("the zone is written when it is given and differs from the default", () => {
  assert.equal(address(query({ timezone: "Europe/Paris" })), "?timezone=Europe%2FParis");
  assert.equal(address(query({ timezone: null })), "");
  assert.equal(buildSearch(shortQuery(query({ timezone: "utc" }), { ...defaults(), timezone: "utc" }), names), "");
});

test("the result keeps the members of the query and never changes the input", () => {
  const input = query({ from: "now-6h", refresh: "30s" });
  const copy = structuredClone({ ...input, vars: [...input.vars] });
  const out = shortQuery(input, defaults());
  assert.deepEqual(Object.keys(out).sort(), ["from", "refresh", "timezone", "to", "vars"]);
  assert.deepEqual({ ...input, vars: [...input.vars] }, copy);
});

// The default selection of a variable and the comparison.
const custom = (extra) => ({ kind: "custom", name: "v", label: null, hide: "none", multi: false, include_all: false, all_value: null, default: [], options: [{ text: "a", value: "a" }, { text: "b", value: "b" }], ...extra });

test("the default selection follows the rules of the selection with no URL value", () => {
  const options = (variable) => variable.options;
  assert.deepEqual(defaultSelection(custom({ default: ["b"] }), options(custom())), { all: false, values: ["b"] });
  assert.deepEqual(defaultSelection(custom({ include_all: true, default: ["$__all"] }), options(custom())), { all: true, values: [] });
  assert.deepEqual(defaultSelection(custom({ default: [] }), options(custom())), { all: false, values: ["a"] });
  assert.deepEqual(defaultSelection(custom({ multi: true, default: ["a", "b"] }), options(custom())), { all: false, values: ["a", "b"] });
  assert.deepEqual(selectionToUrl(custom({ include_all: true }), defaultSelection(custom({ include_all: true, default: ["$__all"] }), options(custom()))), ["$__all"]);
});

test("two selections are the same with the same All flag and the same values in order", () => {
  assert.equal(sameSelection({ all: true, values: [] }, { all: true, values: [] }), true);
  assert.equal(sameSelection({ all: false, values: ["a", "b"] }, { all: false, values: ["a", "b"] }), true);
  assert.equal(sameSelection({ all: false, values: ["a", "b"] }, { all: false, values: ["b", "a"] }), false);
  assert.equal(sameSelection({ all: true, values: [] }, { all: false, values: [] }), false);
});

// The runtime gives the default selection of a variable, or null when it is not known.
function documentWith(variables) {
  return {
    format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off",
    time: { from: "now-6h", to: "now", timezone: "utc", refresh: null },
    links: [], variables, sections: [{ id: "s", title: null, collapsed: false, repeat: null, panels: [] }],
  };
}

test("the runtime knows the default of a custom variable at once and of a query variable after its options", async () => {
  const doc = documentWith([
    custom({ name: "c", default: ["b"] }),
    { kind: "constant", name: "k", label: null, hide: "variable", value: "x" },
    { kind: "query", name: "q", label: null, hide: "none", source: "s", sql: ["SELECT 1"], multi: false, include_all: false, all_value: null, refresh: "load", sort: "none", regex: null, default: [] },
  ]);
  const api = {
    async variableOptions() { return { options: [{ value: "x1", text: "x1" }, { value: "x2", text: "x2" }] }; },
    async query() { return { frames: [] }; },
  };
  const rt = new Runtime({ uid: "d", doc, etag: "e", api, widthOf: () => 100, range: { fromText: "now-6h", toText: "now" } });
  assert.deepEqual(rt.defaultSelection("c"), { all: false, values: ["b"] });
  assert.equal(rt.defaultSelection("k"), null);
  assert.equal(rt.defaultSelection("q"), null, "the options of q are not loaded yet");
  assert.equal(rt.defaultSelection("none"), null);
  await rt.start();
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(rt.defaultSelection("q"), { all: false, values: ["x1"] });
  rt.destroy();
});
