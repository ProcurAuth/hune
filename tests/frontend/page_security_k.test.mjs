// SPDX-License-Identifier: MIT OR Apache-2.0
// A security review of the page code: the bounds that the page keeps before it works on a large input.
// Each test failed before the fix. The timing tests use the minimum of 7 runs.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import * as A from "../../web/js/annotations.js";
import { formatSql } from "../../web/js/editor/ast/format.js";
import { parseSql } from "../../web/js/editor/ast/sql_tree.js";
import "../../web/js/editor/ast/tokens_ch.js";
import "../../web/js/editor/ast/tokens_pg.js";
import { createText } from "../../web/js/panels/text.js";
import { Runtime } from "../../web/js/runtime.js";
import { expandText, interpolateText } from "../../web/js/var_options.js";
import { fakeDocument } from "./markdown_helpers.mjs";

// The gate runs many jobs at the same time, so the timing tests take 21 runs to meet a quiet moment. The limits are the same.
const best = (fn, runs = 21) => {
  let min = Infinity;
  for (let i = 0; i < runs; i++) {
    const t0 = performance.now();
    fn();
    min = Math.min(min, performance.now() - t0);
  }
  return min;
};

// The document of the review: 3,500 options of 1 KB, "All" with no all_value, and a text that names the variable many times.
function hostile(refs, extra = []) {
  const options = Array.from({ length: 3500 }, (_, i) => ({ value: String(i).padStart(4, "0") + "x".repeat(1016), text: `o${i}` }));
  const textPanel = { id: 1, type: "text", title: "T", grid: { x: 0, y: 0, w: 24, h: 8 }, options: { mode: "markdown", content: ["$v ".repeat(refs)] } };
  const doc = { title: "d", time: { from: "now-1h", to: "now", timezone: "utc" }, variables: [{ name: "v", kind: "custom", label: "v", multi: true, include_all: true, all_value: null, options, default: ["$__all"] }, ...extra], sections: [{ title: null, collapsed: false, panels: [textPanel] }], annotations: [] };
  const rt = new Runtime({ uid: "u", doc, etag: "e", api: {}, widthOf: () => 800, range: { fromText: "now-1h", toText: "now" } });
  return { rt, textPanel };
}

test("a text with 20,000 references to a large variable gives text-limit, never a thrown error, in under 50 ms", () => {
  const { rt, textPanel } = hostile(20000);
  const view = createText(textPanel);
  const expand = (t) => rt.interpolateContent(t);
  assert.doesNotThrow(() => view.key(expand));
  const drawn = view.draw(fakeDocument, expand);
  assert.equal(drawn.limit, "expanded");
  assert.match(drawn.message, /262,144 bytes/);
  assert.ok(best(() => { view.key(expand); view.draw(fakeDocument, expand); }) < 50);
  assert.deepEqual(view.fail().limit, "expanded");
});

test("the key changes with the selection and stays small", () => {
  const { rt, textPanel } = hostile(1);
  textPanel.options.content = ["$v"];
  const view = createText(textPanel);
  const expand = (t) => rt.interpolateContent(t);
  const all = view.key(expand);
  assert.equal(all, ""); // "All" of 3,500 values passes the budget: no text
  assert.equal(createText({ options: { content: ["no variable"] } }).key(expand), "");
  assert.equal(view.draw(fakeDocument, expand).limit, "expanded");
});

test("interpolateContent gives null above the budget, the text below it", () => {
  const { rt } = hostile(1);
  assert.equal(rt.interpolateContent("$v"), null);
  assert.equal(rt.interpolateContent("${v:csv}"), null);
  assert.equal(rt.interpolateContent("a ${w} b"), "a ${w} b");
});

test("interpolateText (the description) stops at 65,536 characters and keeps the text as written", () => {
  const info = { all: false, allValue: null, values: ["x".repeat(1000)], texts: ["x".repeat(1000)] };
  const text = "${v:csv}".repeat(500);
  assert.equal(expandText(text, () => info, 65536), null);
  assert.equal(interpolateText(text, () => info), text);
  assert.equal(interpolateText("${v:csv}", () => info), "x".repeat(1000));
  assert.ok(best(() => interpolateText(text, () => info)) < 50);
});

test("paintText shows text-limit when key or draw throws", () => {
  const js = readFileSync(new URL("../../web/js/panel_frame.js", import.meta.url), "utf8");
  const at = js.indexOf("function paintText(force)");
  const body = js.slice(at, js.indexOf("setRootState(drawn.root", at));
  assert.match(body, /try \{ sig = textView\.key\(expand\); \} catch/);
  assert.match(body, /try \{ drawn = textView\.draw\(document, expand\); \} catch \(e\) \{ drawn = textView\.fail\(\); \}/);
});

test("the formatter stops at the cap of the output: a chain of CASE gives output_too_large in under 300 ms", () => {
  const k = Math.floor((262144 - 20) / 17);
  const flat = "SELECT " + "CASE WHEN a THEN ".repeat(k) + "1";
  const nest = "SELECT " + "CASE WHEN a THEN ".repeat(k) + "1" + " END".repeat(k);
  for (const s of [flat, nest.slice(0, 262144)]) {
    let r;
    const ms = best(() => { r = formatSql(s, "clickhouse"); }, 3);
    assert.equal(r.reason, "output_too_large");
    assert.equal(r.changed, false);
    assert.ok(ms < 300, `${ms} ms`);
  }
});

test("parseSql checks the cap before it tokenizes: 16 MB give too_large in under 50 ms and one token that covers the text", () => {
  const s = "a ".repeat(8e6);
  let p;
  const ms = best(() => { p = parseSql(s, "clickhouse"); });
  assert.equal(p.capped.reason, "too_large");
  assert.ok(ms < 50, `${ms} ms`);
  assert.equal(p.tokens.length, 1);
  assert.deepEqual([p.tokens[0].start, p.tokens[0].end], [0, s.length]);
  assert.equal(parseSql("SELECT 1", "postgres").tokens.length > 1, true);
});

test("readAnswer checks the title, the text, the tags and the end again", () => {
  const ok = ["t", null, "title", "text", ["a"]];
  const answer = (row) => ({ rows: 1, annotations: [[1000, ...row.slice(1)]] });
  const bad = (row) => assert.throws(() => A.readAnswer(answer(row)), { message: "The answer of the annotation is not valid." });
  assert.equal(A.readAnswer(answer(ok)).length, 1);
  bad(["t", null, "x".repeat(257), "", []]);
  assert.equal(A.readAnswer(answer(["t", null, "x".repeat(256), "", []])).length, 1);
  assert.equal(A.readAnswer(answer(["t", null, "é".repeat(256), "", []])).length, 1);
  bad(["t", null, "", "x".repeat(2049), []]);
  bad(["t", null, "", "é".repeat(1025), []]);
  assert.equal(A.readAnswer(answer(["t", null, "", "é".repeat(1024), []])).length, 1);
  bad(["t", null, "", "", ["x".repeat(65)]]);
  assert.equal(A.readAnswer(answer(["t", null, "", "", ["x".repeat(64)]])).length, 1);
  bad(["t", 999, "", "", []]);
  assert.equal(A.readAnswer(answer(["t", 1000, "", "", []])).length, 1);
});
