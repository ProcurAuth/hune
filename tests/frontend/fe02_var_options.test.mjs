// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-02: options and selection of a variable (9.10).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { buildOptions, resolveSelection, requestValue, isNumberText, selectionToUrl, ALL } from "../../web/js/var_options.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-02-var-options.json", import.meta.url)));

test("FE-02 options: regex, duplicates, sort", () => {
  for (const c of v.build) assert.deepEqual(buildOptions(c.raw, c.variable), c.expect, c.name);
});

test("FE-02 selection rules", () => {
  for (const c of v.selection) assert.deepEqual(resolveSelection(c.variable, c.options, c.url || undefined, c.current || undefined), c.expect, c.name);
});

test("FE-02 request values (7.7)", () => {
  for (const c of v.request) assert.deepEqual(requestValue(c.variable, c.options, c.selection), c.expect, c.name);
});

test("FE-02 number grammar and URL form", () => {
  for (const [text, ok] of Object.entries(v.numberText)) assert.equal(isNumberText(text), ok, text);
  assert.deepEqual(selectionToUrl({ kind: "query" }, { all: true, values: [] }), [ALL]);
  assert.deepEqual(selectionToUrl({ kind: "query" }, { all: false, values: ["a", "b"] }), ["a", "b"]);
});

// F13: variables in titles, descriptions, section titles and link texts (9.13).
import { interpolateText, placeholderInfo } from "../../web/js/var_options.js";

const lookupOf = (table) => (name) => table[name];
const one = (value, text = value) => ({ all: false, allValue: null, values: [value], texts: [text] });
const many = (pairs) => ({ all: false, allValue: null, values: pairs.map((p) => p[0]), texts: pairs.map((p) => p[1]) });

test("F13 interpolateText: the table of 9.13", () => {
  const lookup = lookupOf({ service: one("api"), env: one("prod"), host: many([["a", "Alpha"], ["b", "Beta"]]), envs: many([["prod", "prod"], ["dev", "dev"]]) });
  assert.equal(interpolateText("Latency of $service in ${env}", lookup), "Latency of api in prod");
  // Without a format a display text shows the option texts (Grafana 12.4.2), as with `:text`.
  assert.equal(interpolateText("Hosts ${host}", lookup), "Hosts Alpha + Beta");
  assert.equal(interpolateText("Hosts $host", lookup), "Hosts Alpha + Beta");
  assert.equal(interpolateText("Hosts ${host:text}", lookup), "Hosts Alpha + Beta");
  assert.equal(interpolateText("Hosts ${host:glob}", lookup), "Hosts {a,b}");
  assert.equal(interpolateText("${host:csv} ${host:pipe}", lookup), "a,b a|b");
  assert.equal(interpolateText("${envs:csv} <b>", lookup), "prod,dev <b>");
  assert.equal(interpolateText("${envs:pipe}", lookup), "prod|dev");
});

test("F13 interpolateText: what stays as written", () => {
  const lookup = lookupOf({ env: one("prod") });
  assert.equal(interpolateText("$unknown ${unknown} ${unknown:csv}", lookup), "$unknown ${unknown} ${unknown:csv}");
  assert.equal(interpolateText("${env:date} ${env:raw} ${env:regex}", lookup), "${env:date} ${env:raw} ${env:regex}");
  assert.equal(interpolateText("$__all ${__from} $__interval", lookup), "$__all ${__from} $__interval");
  assert.equal(interpolateText("[[env]] $ ${} ${env", lookup), "[[env]] $ ${} ${env");
  // The longest run is the name: $env_name is not $env.
  assert.equal(interpolateText("$env_name ${env}_name", lookup), "$env_name prod_name");
  assert.equal(interpolateText("no placeholder", lookup), "no placeholder");
  assert.equal(interpolateText(null, lookup), null);
});

test("F13 interpolateText: All, all_value, empty selection, text variable", () => {
  const options = [{ value: "a", text: "Alpha" }, { value: "b", text: "Beta" }];
  const variable = { kind: "custom", name: "x", options, all_value: null };
  const all = placeholderInfo(variable, { all: true, values: [] }, null);
  const lookup = (name) => (name === "x" ? all : null);
  assert.equal(interpolateText("${x}", lookup), "All");
  assert.equal(interpolateText("${x:text}", lookup), "All");
  assert.equal(interpolateText("${x:glob}", lookup), "{a,b}");
  assert.equal(interpolateText("${x:csv}", lookup), "a,b");
  const withAllValue = placeholderInfo({ ...variable, all_value: ".*" }, { all: true, values: [] }, null);
  assert.equal(interpolateText("${x}|${x:glob}|${x:csv}|${x:text}", () => withAllValue), "All|.*|.*|All");
  const none = placeholderInfo(variable, { all: false, values: [] }, null);
  assert.equal(interpolateText("[${x}][${x:glob}]", () => none), "[][]");
  const text = placeholderInfo({ kind: "text", name: "t" }, { all: false, values: ["a b"] }, null);
  assert.equal(interpolateText("${t} ${t:text}", () => text), "a b a b");
  // The option text of a value that is no option is the value.
  const stray = placeholderInfo(variable, { all: false, values: ["a", "zz"] }, null);
  assert.equal(interpolateText("${x} ${x:text}", () => stray), "Alpha + zz Alpha + zz");
});

test("F13 the value is text: markup stays markup", () => {
  const lookup = lookupOf({ v: one("<img src=x onerror=alert(1)>") });
  assert.equal(interpolateText("T ${v}", lookup), "T <img src=x onerror=alert(1)>");
});
