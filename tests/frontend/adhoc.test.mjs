// FE-80, F36 (decision ID-K329): the URL form of the ad hoc filters (9.5, 9.10a), the checks of the page, the request member (7.7a) and
// the permitted keys of the answer of 6.14. The vectors come from the measure of Grafana 12.4.2 (tests/frontend/vectors/FE-80-adhoc-url.json).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { adhocRequest, adhocSelection, adhocUrl, filterError, filterText, parseFilterText, readAdhocKeys } from "../../web/js/adhoc.js";
import { buildSearch, parseSearch } from "../../web/js/url_state.js";

const V = JSON.parse(readFileSync(new URL("./vectors/FE-80-adhoc-url.json", import.meta.url), "utf8"));
const variable = { kind: "adhoc", name: "filters", label: "Filters", source: "prod", keys: ["host", "env", "region"] };
const keys = ["host", "env"];

test("FE-80 read: each item of the address gives the filter that Grafana 12.4.2 keeps", () => {
  for (const { item, filter } of V.read) assert.deepEqual(parseFilterText(item), filter, item);
});

test("FE-80 write: the item of each filter; it reads back to the same filter; the address decodes to the item of Grafana", () => {
  for (const { filter, item, grafana } of V.write) {
    assert.equal(filterText(filter), item, JSON.stringify(filter));
    // A text that holds an escape (`__gfp__`) reads back with the character, in Grafana too: the address loses it.
    const lossy = /__gf[pch]__/.test(filter.key + filter.value);
    if (!lossy) assert.deepEqual(parseFilterText(item), filter, item);
    else assert.equal(parseFilterText(item).value, filter.value.replaceAll("__gfp__", "|"), item);
    const search = buildSearch({ vars: new Map([["filters", [item]]]) }, ["filters"]);
    assert.equal(decodeURIComponent(search.slice("?var-filters=".length)), decodeURIComponent(grafana), item);
    // The address of Grafana reads to the same filter in the page.
    assert.deepEqual(parseFilterText(parseSearch(`?var-filters=${grafana}`).vars.get("filters")[0]), parseFilterText(item));
  }
  assert.deepEqual(adhocUrl({ all: false, values: V.write_none.filters }), V.write_none.items);
  assert.equal(filterText(V.write_empty_value.filter), V.write_empty_value.item);
});

test("the selection keeps the valid filters, 16 at most, and names the others in the notice", () => {
  const url = ["host|=|a", "", "env|!=|b|x", "region|=|c", "host|>|5", "host|=", "nope|=|x", `host|=|${"x".repeat(1025)}`, "host|=|a\tb"];
  const r = adhocSelection(variable, keys, url);
  assert.deepEqual(r.selection.values, ["host|=|a", "env|!=|b"], "the items of the selection are the canonical items");
  assert.deepEqual(r.selection.values.map(parseFilterText), [{ key: "host", op: "=", value: "a" }, { key: "env", op: "!=", value: "b" }]);
  assert.equal(r.selection.all, false);
  assert.match(r.notice, /^The page does not use these filters of Filters: "region\|=\|c", "host\|>\|5", "host\|=" and 3 more\.$/);
  const many = Array.from({ length: 18 }, (_, i) => `host|=|v${i}`);
  const m = adhocSelection(variable, keys, many);
  assert.equal(m.selection.values.length, 16);
  assert.match(m.notice, /"host\|=\|v16", "host\|=\|v17"\./);
  assert.equal(adhocSelection(variable, keys, [""]).notice, null);
  assert.deepEqual(adhocSelection(variable, keys, null).selection, { all: false, values: [] });
  // A selection kept after a reload is checked again with the keys of the new answer.
  const k = adhocSelection(variable, ["host"], ["host|=|a", "env|=|b"]);
  assert.deepEqual(k.selection.values, ["host|=|a"]);
  assert.match(k.notice, /"env\|=\|b"/);
  // An address item of Grafana with a label and an escape gives the canonical item.
  assert.deepEqual(adhocSelection(variable, keys, ["host,Host|=|a__gfc__b,label"]).selection.values, ["host|=|a__gfc__b"]);
});

test("the page checks the key, the operator and the value as the server does (7.7a)", () => {
  assert.equal(filterError({ key: "host", op: "=", value: "" }, keys), "");
  assert.equal(filterError({ key: "host", op: "!=", value: "é".repeat(512) }, keys), "");
  assert.match(filterError({ key: "host", op: "!=", value: "é".repeat(513) }, keys), /1024 bytes/);
  for (const op of ["<", ">", "=~", "!~", "IN", "==", " ="]) assert.match(filterError({ key: "host", op, value: "1" }, keys), /operator/, op);
  for (const key of ["region", "Host", "host ", ""]) assert.match(filterError({ key, op: "=", value: "1" }, keys), /key/, key);
  // The text of an escape would change in the address: the page refuses it (9.10a).
  for (const value of ["a__gfp__b", "__gfc__", "x__gfh__"]) assert.match(filterError({ key: "host", op: "=", value }, keys), /__gfp__/);
  for (const value of ["a\nb", "\u0000", "a\u007f"]) assert.match(filterError({ key: "host", op: "=", value }, keys), /control/);
});

test("the request member is the closed form {filters: [{key, op, value}]}", () => {
  const sel = { all: false, values: [filterText({ key: "host", op: "=", value: "a|b,c#d" }), filterText({ key: "env", op: "!=", value: "" })] };
  assert.deepEqual(adhocRequest(sel), { filters: [{ key: "host", op: "=", value: "a|b,c#d" }, { key: "env", op: "!=", value: "" }] });
  assert.deepEqual(adhocRequest({ all: false, values: [] }), { filters: [] });
});

test("the permitted keys of 6.14: a wrong or absent member gives no key (fail closed)", () => {
  const doc = { variables: [variable, { kind: "text", name: "t" }, { ...variable, name: "f2", keys: ["a"] }] };
  assert.deepEqual([...readAdhocKeys(doc, { filters: ["env"], f2: ["a"] })], [["filters", ["env"]], ["f2", ["a"]]]);
  assert.deepEqual([...readAdhocKeys(doc, { filters: ["zone"] })], [["filters", []], ["f2", []]]);
  assert.deepEqual([...readAdhocKeys(doc, undefined)], [["filters", []], ["f2", []]]);
  assert.deepEqual([...readAdhocKeys(doc, [])], [["filters", []], ["f2", []]]);
  assert.deepEqual([...readAdhocKeys(doc, { filters: "env" })], [["filters", []], ["f2", []]]);
});

test("the parse is linear: 16 items of 100,000 characters each in 50 ms (the minimum of 7 runs after 2 warm-ups)", () => {
  const big = Array.from({ length: 16 }, (_, i) => `host|=|${"a|,#_".repeat(20000)}${i}`);
  let best = Infinity;
  for (let i = 0; i < 9; i++) {
    const t = performance.now();
    adhocSelection(variable, keys, big);
    if (i >= 2) best = Math.min(best, performance.now() - t);
  }
  assert.ok(best < 50, `${best} ms`);
});
