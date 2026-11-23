// FE-68 (9.5, 9.10, 9.13, 4.10a, 4.10b): the selection of the kinds `interval` and `constant`: 40 vectors. The expected values are written by hand from
// the rules of the spec, not computed by the code under test.
// FE-69 (T92): 40 hostile texts as an item, as a value and as a URL value. The page writes each of them as text only.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { AUTO, interpolateText, optionsOf, placeholderInfo, replacement, requestValue, resolveSelection, selectionToUrl } from "../../web/js/var_options.js";
import { buildSearch, parseSearch } from "../../web/js/url_state.js";
import { selectionText } from "../../web/js/vars.js";
import { AUTO_MIN_TEXTS, intervalError, intervalsError, newVariable, splitIntervals } from "../../web/js/editor/forms/model.js";
import { variablePreviewBody } from "../../web/js/editor/preview_body.js";

const base = { kind: "interval", name: "b", label: null, hide: "none", intervals: ["1m", "1h", "7d"], auto: false, auto_count: 30, auto_min: "10s", default: null };
const withAuto = { ...base, auto: true };
const constant = { kind: "constant", name: "k", label: null, hide: "variable", value: "abc" };
const sel = (...values) => ({ all: false, values });
const texts = (o) => o.map((x) => x.text);
const pick = (variable, url, current) => resolveSelection(variable, optionsOf(variable, null), url, current);

test("FE-68 options: 6 vectors", () => {
  assert.deepEqual(optionsOf(base, null).map((o) => o.value), ["1m", "1h", "7d"]);
  assert.deepEqual(optionsOf(withAuto, null), [{ value: AUTO, text: "Auto" }, { value: "1m", text: "1m" }, { value: "1h", text: "1h" }, { value: "7d", text: "7d" }]);
  assert.deepEqual(texts(optionsOf({ ...withAuto, intervals: ["1y"] }, null)), ["Auto", "1y"]);
  assert.deepEqual(optionsOf({ ...base, auto_count: 1000, auto_min: "1y" }, null).length, 3); // auto_count and auto_min do nothing with auto = false
  assert.deepEqual(optionsOf(constant, null), []);
  assert.equal(AUTO, "$__auto");
});

test("FE-68 selection: 19 vectors for the URL, the current value and the default", () => {
  const cases = [
    [base, undefined, undefined, sel("1m")],
    [base, ["1h"], undefined, sel("1h")],
    [base, ["7m"], undefined, sel("1m")],
    [base, [AUTO], undefined, sel("1m")], // `$__auto` with auto = false is not an option
    [withAuto, [AUTO], undefined, sel(AUTO)],
    [withAuto, undefined, undefined, sel(AUTO)],
    [{ ...base, default: "1h" }, undefined, undefined, sel("1h")],
    [{ ...base, default: "1h" }, ["7m"], undefined, sel("1h")],
    [{ ...base, default: "1h" }, undefined, ["7d"], sel("7d")],
    [{ ...base, default: "1h" }, ["7d"], ["1h"], sel("7d")],
    [{ ...withAuto, default: AUTO }, undefined, undefined, sel(AUTO)],
    [{ ...withAuto, default: AUTO }, ["1h"], undefined, sel("1h")],
    [base, ["1H"], undefined, sel("1m")], // the match is exact
    [base, ["1h", "7d"], undefined, sel("1h")], // no multi
    [base, ["$__all"], undefined, sel("1m")], // no All
    [base, [""], undefined, sel("1m")],
    [constant, ["zzz"], undefined, sel("abc")],
    [constant, undefined, ["x"], sel("abc")],
    [{ ...constant, value: "" }, ["zzz"], undefined, sel("")],
  ];
  assert.equal(cases.length, 19);
  for (const [variable, url, current, expect] of cases) assert.deepEqual(pick(variable, url, current), expect, JSON.stringify([variable.value ?? variable.default, url, current]));
});

test("FE-68 request, URL and display: 15 vectors", () => {
  const out = [
    requestValue(base, optionsOf(base, null), sel("1h")),
    requestValue(withAuto, optionsOf(withAuto, null), sel(AUTO)),
    requestValue(constant, [], sel("abc")),
    requestValue({ ...constant, value: "" }, [], sel("")),
    selectionToUrl(base, sel("1h")),
    selectionToUrl(withAuto, sel(AUTO)),
    selectionToUrl(constant, sel("abc")),
    selectionToUrl({ ...constant, value: "" }, sel("")),
  ];
  assert.deepEqual(out, [
    { all: false, values: ["1h"] }, { all: false, values: [AUTO] }, { all: false, values: ["abc"] }, { all: false, values: [""] },
    ["1h"], [AUTO], [], [],
  ]);
  const look = (variable, selection) => (name) => (name === variable.name ? placeholderInfo(variable, selection, null) : null);
  assert.equal(interpolateText("Bucket ${b}", look(withAuto, sel(AUTO))), "Bucket Auto");
  assert.equal(interpolateText("Bucket $b", look(base, sel("1h"))), "Bucket 1h");
  assert.equal(interpolateText("Note ${k}", look(constant, sel("abc"))), "Note abc");
  assert.equal(interpolateText("${k:text}", look(constant, sel("abc"))), "abc");
  assert.equal(interpolateText("${b:csv}", look(withAuto, sel(AUTO))), AUTO);
  assert.equal(interpolateText("${k:csv}", look(constant, sel("abc"))), "abc");
  assert.equal(selectionText(withAuto, optionsOf(withAuto, null), sel(AUTO)), "Auto");
});

test("FE-68 notice: a URL value that is not an item gives the notice of 9.10; a valid value and a constant give none (2 vectors)", () => {
  const options = optionsOf(base, null);
  const after = pick(base, ["7m"]);
  assert.match(replacement(base, options, ["7m"], undefined, after).text, /"7m"/);
  assert.equal(replacement(base, options, ["1h"], undefined, pick(base, ["1h"])), null);
  assert.equal(replacement(constant, [], ["zzz"], undefined, pick(constant, ["zzz"])), null);
});

test("FE-68 editor rules of 4.10a: the items, the list, the new variables, the preview body", () => {
  for (const ok of ["1ms", "999999999ms", "1y", "365d", "52w", "8760h", "525600m", "31536000s"]) assert.equal(intervalError(ok), "", ok);
  for (const bad of ["1M", "1.5h", "0m", "-1m", "1 m", "", "366d", "53w", "8761h", "01m", "1000000000ms", "1H", "m", "5x"]) assert.notEqual(intervalError(bad), "", bad);
  assert.deepEqual(splitIntervals("1m, 1h\n7d\r\n,,  "), ["1m", "1h", "7d"]);
  assert.equal(intervalsError([]), "Use 1 to 50 intervals.");
  assert.equal(intervalsError(Array.from({ length: 51 }, (_, i) => `${i + 1}s`)), "Use 1 to 50 intervals.");
  assert.equal(intervalsError(Array.from({ length: 50 }, (_, i) => `${i + 1}s`)), "");
  assert.match(intervalsError(["1m", "1m"]), /twice/);
  assert.equal(AUTO_MIN_TEXTS.length, 30);
  assert.deepEqual(newVariable("interval", "x"), { kind: "interval", name: "x", label: null, hide: "none", intervals: ["1m", "10m", "1h", "1d"], auto: false, auto_count: 30, auto_min: "10s", default: null });
  assert.deepEqual(newVariable("constant", "x"), { kind: "constant", name: "x", label: null, hide: "variable", value: "" });
  const q = { kind: "query", name: "q", source: "s", sql: ["SELECT '$b', '$k', '$c'"] };
  const body2 = variablePreviewBody({ doc: { variables: [base, constant, { ...withAuto, name: "c" }, q] }, variable: q, target: { uid: "u" }, range: { from_ms: 1, to_ms: 2 }, known: {} });
  assert.deepEqual(body2.variables, { b: sel("1m"), k: sel("abc"), c: sel(AUTO) });
});

// --- FE-69 ----------------------------------------------------------------------------------------------------------------

const HOSTILE = JSON.parse(readFileSync(new URL("./vectors/FE-49-open-variables-form.json", import.meta.url))).hostile;

test("FE-69: 40 hostile texts stay text in the options, the display, the request and the URL", () => {
  assert.equal(HOSTILE.length, 40);
  for (const text of HOSTILE) {
    // As an item (the server refuses it at save; the page still never reads it as markup).
    const iv = { ...base, intervals: [text] };
    assert.deepEqual(texts(optionsOf(iv, null)), [text]);
    assert.equal(selectionText(iv, optionsOf(iv, null), sel(text)), text);
    // As a value of a constant: the selection is the value, never a URL value, and the URL has none.
    const k = { ...constant, value: text };
    assert.deepEqual(pick(k, [text], [text]), sel(text));
    assert.deepEqual(selectionToUrl(k, sel(text)), []);
    assert.equal(buildSearch({ vars: new Map([["k", selectionToUrl(k, sel(text))]]) }, ["k"]), "");
    assert.equal(interpolateText("$k", (name) => (name === "k" ? placeholderInfo(k, sel(text), null) : null)), text);
    assert.deepEqual(requestValue(k, [], sel(text)), { all: false, values: [text] });
    // As a URL value of an interval: it is not an option, so the selection is the first option, and the notice holds it as text only.
    assert.deepEqual(pick(base, [text]), sel("1m"));
    const notice = replacement(base, optionsOf(base, null), [text], undefined, sel("1m"));
    assert.equal(notice.name, "b");
    // The URL writer encodes the value and the reader gives it back unchanged.
    const search = buildSearch({ vars: new Map([["b", [text]]]) }, ["b"]);
    assert.ok(!/[<>" ]/.test(search), search);
    assert.deepEqual(parseSearch(search).vars.get("b"), [text]);
  }
});

test("FE-69: the code of the page writes no markup from a string", () => {
  for (const file of ["var_options.js", "vars.js", "var_layout.js"]) {
    const src = readFileSync(new URL(`../../web/js/${file}`, import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
    assert.ok(!/innerHTML|outerHTML|insertAdjacentHTML|document\.write|\beval\(|new Function/.test(src), file);
  }
});
