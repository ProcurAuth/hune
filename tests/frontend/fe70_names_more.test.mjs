// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-70: names. organize, filter_fields_by_name and rename_by_regex (9.21a, rules 1, 2, 10 and 11), and the threats T86 and T87.
import test from "node:test";
import assert from "node:assert/strict";
import { applyMore } from "../../web/js/transform_more.js";
import { displayNames } from "../../web/js/names.js";
import { best, fieldOf, framesOf, measure, measureRun, run, vectors } from "./more_helpers.mjs";

test("FE-70 vectors of the file FE-70-names.json", () => {
  for (const c of vectors("FE-70-names.json").cases) assert.deepEqual(run(c.frames, c.transform), c.expect, c.name);
});

test("FE-70 the cases of the measure of Grafana: same, or a difference that 19.20j lists", () => {
  const cases = measure.filter((c) => /^(o|ff|rr)\d+$/.test(c.id));
  assert.ok(cases.length >= 48);
  for (const c of cases) {
    if (c.class === "not_mapped") continue;
    const got = measureRun(c);
    if (c.class === "same") assert.deepEqual(got, c.grafana, c.id);
    else {
      assert.deepEqual(got, c.got, c.id);
      assert.notDeepEqual(got, c.grafana, c.id);
    }
  }
});

// 40 hostile texts: markup, a template, a regular expression, a long text, control characters, names of the prototype chain.
const HOSTILE = [
  "<img src=x onerror=alert(1)>", "<script>alert(1)</script>", "\"><svg onload=alert(1)>", "javascript:alert(1)", "' OR '1'='1", "$1", "$&", "$`", "$'", "$$", "$0",
  "${__field.name}", "${__field.labels.env}", "$__interval", "[[var:csv]]", "{{x}}", "<b>bold</b>", "&lt;b&gt;", "&#60;script&#62;", "%3Cscript%3E",
  "__proto__", "constructor", "prototype", "toString", "hasOwnProperty", "a\nb", "a\u0000b", "‮evil", "  ", "😀 emoji", "ü ñ é", "(a+)+$", ".*", "/x/", "\\",
  "a".repeat(256), "<".repeat(256), "x\ty", "</textarea><script>1</script>", "=cmd|' /C calc'!A0",
];

test("FE-70 hostile names: a rename and an alias stay text and change nothing else", () => {
  assert.equal(HOSTILE.length, 40);
  for (const name of HOSTILE) {
    const frames = framesOf([[{ name: "x", kind: "number", values: [1] }, { name: "y", kind: "number", values: [2] }]]);
    const out = applyMore(frames, { kind: "organize", hide: [], order: [], rename: [{ field: "x", name }] })[0];
    assert.equal(out[0].config.display_name, name);
    assert.equal(out[0].name, "x");
    assert.equal(out[1].name, "y");
    assert.deepEqual(Array.from(out[0].values), [1]);
    const calc = applyMore(frames, { kind: "calculate_field", mode: "index", alias: name, replace_fields: false, index: { as_percentile: false } })[0];
    assert.equal(calc[2].name, name);
    assert.equal(({}).polluted, undefined);
  }
});

test("FE-70 hostile names: a name that is a field is found, renamed, filtered and used as a label", () => {
  for (const name of HOSTILE) {
    const field = fieldOf({ name, kind: "number", values: [1, 2] });
    const key = fieldOf({ name: "k", kind: "text", values: ["a", "b"] });
    const frames = [[key, field]];
    const found = applyMore(frames, { kind: "filter_fields_by_name", include: { names: [name], pattern: null }, exclude: { names: [], pattern: null } })[0];
    assert.equal(found.length, 1);
    const hidden = applyMore(frames, { kind: "organize", hide: [name], order: [], rename: [] })[0];
    assert.deepEqual(hidden.map((f) => f.name), ["k"]);
    const parts = applyMore(frames, { kind: "partition_by_values", fields: ["k"], keep_fields: false });
    assert.equal(parts.length, 2);
    const parted = applyMore([[fieldOf({ name, kind: "text", values: ["a", "b"] }), fieldOf({ name: "v", kind: "number", values: [1, 2] })]], { kind: "partition_by_values", fields: [name], keep_fields: false });
    assert.equal(Object.keys(parted[0][0].labels).length, 1);
    assert.equal(Object.getPrototypeOf(parted[0][0].labels), Object.prototype);
    assert.equal(parted[0][0].labels[name], "a");
    assert.equal(({}).polluted, undefined);
    assert.equal(displayNames(found).length, 1);
  }
});

test("FE-70 hostile texts as a replacement: the result is the text of the JavaScript replace and never HTML", () => {
  for (const text of HOSTILE) {
    const frames = framesOf([[{ name: "abc", kind: "number", values: [1] }]]);
    const out = applyMore(frames, { kind: "rename_by_regex", regex: "/b/", rename_pattern: text.slice(0, 256) })[0];
    const want = "abc".replace(/b/, text.slice(0, 256));
    // A result that is empty or equal to the name keeps the name (9.21a, rule 11).
    assert.equal(out[0].config.display_name, want === "" || want === "abc" ? undefined : want);
  }
});

test("FE-70 the module has no innerHTML and no other way to make markup (T86)", async () => {
  const { readFileSync } = await import("node:fs");
  const src = readFileSync(new URL("../../web/js/transform_more.js", import.meta.url), "utf8").replace(/\/\/.*$/gm, "");
  assert.ok(!/innerHTML|outerHTML|insertAdjacentHTML|document\.|\beval\b|new Function/.test(src));
});

test("FE-70 a slow pattern on 1,024 names runs in 50 ms or less (T87)", () => {
  const fields = Array.from({ length: 1024 }, (_, i) => fieldOf({ name: `${"a".repeat(10)}!${i % 10}`, kind: "number", values: [i] }));
  const spec = { kind: "filter_fields_by_name", include: { names: [], pattern: "/^(a+)+$/" }, exclude: { names: [], pattern: null } };
  const { ms, out } = best(() => applyMore([fields], spec));
  assert.equal(out[0].length, 0);
  assert.ok(ms <= 50, `${ms.toFixed(1)} ms`);
  const rename = { kind: "rename_by_regex", regex: "/(a+)+$/", rename_pattern: "x" };
  const timed = best(() => applyMore([fields], rename));
  assert.equal(timed.out[0].length, 1024);
  assert.ok(timed.ms <= 50, `${timed.ms.toFixed(1)} ms`);
});

test("FE-70 an invalid pattern is an error of the panel (the document check refuses it first)", () => {
  const frames = framesOf([[{ name: "x", kind: "number", values: [1] }]]);
  assert.throws(() => applyMore(frames, { kind: "rename_by_regex", regex: "(", rename_pattern: "" }), /The transform rename_by_regex failed: the pattern is not valid/);
  assert.throws(() => applyMore(frames, { kind: "filter_fields_by_name", include: { names: [], pattern: "(" }, exclude: { names: [], pattern: null } }), /pattern is not valid/);
});
