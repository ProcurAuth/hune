// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-48 (9.10, T78): the selection of a closed and of an open variable, the typed value, the notice of a replaced value and the
// runtime that sends the notice. The vectors are in `vectors/FE-48-open-variables.json`: they come from a reference that follows 9.10, not from the code.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { cut, droppedValues, isOpen, replacedNotice, replacement, resolveSelection, typedValueError } from "../../web/js/var_options.js";
import { RuntimeWithNames as Runtime } from "./helpers.mjs";

const fe48 = JSON.parse(readFileSync(new URL("./vectors/FE-48-open-variables.json", import.meta.url)));

test("FE-48 selection: 60 vectors for the URL, the current selection and the default, closed and open, single and multi", () => {
  assert.equal(fe48.selection.length, 60);
  for (const v of fe48.selection) {
    const got = resolveSelection(v.variable, v.options, v.url || undefined, v.current || undefined);
    assert.deepEqual(got, v.expect, v.name);
    assert.deepEqual(droppedValues(v.variable, v.options, v.url || v.current || v.variable.default), v.dropped, `${v.name}: the values that go`);
  }
  // A closed variable never keeps a value that is not an option. An open one keeps the typed values only.
  const closed = fe48.selection.filter((v) => !isOpen(v.variable));
  assert.equal(closed.length, 30);
  for (const v of closed) for (const x of resolveSelection(v.variable, v.options, v.url || undefined, v.current || undefined).values) assert.ok(v.options.some((o) => o.value === x), `${v.name}: ${x}`);
});

test("FE-48 typed: 20 vectors of typedValueError (bytes, multi-byte characters at the limit, control characters, the reserved value)", () => {
  assert.equal(fe48.typed.length, 20);
  for (const v of fe48.typed) assert.equal(typedValueError(v.text), v.error, v.name);
  assert.equal(fe48.typed.filter((v) => v.error === "").length, 9);
});

test("FE-48 notice: 10 vectors of replacedNotice, the cut of 40 characters and the list of 3 values", () => {
  assert.equal(fe48.notice.length, 10);
  for (const v of fe48.notice) assert.equal(replacedNotice(v.label, v.dropped, v.options, v.selection), v.text, v.name);
  assert.equal(cut("x".repeat(40)), "x".repeat(40));
  assert.equal(cut("x".repeat(41)), `${"x".repeat(40)}...`);
  // The notice of a variable that has no option, or that drops nothing, is null.
  const v = { name: "v", label: null, multi: false, include_all: false, default: [] };
  assert.equal(replacement(v, [], ["zzz"], undefined, { all: false, values: [] }), null);
  assert.equal(replacement(v, [{ value: "a", text: "a" }], ["a"], undefined, { all: false, values: ["a"] }), null);
  assert.deepEqual(replacement(v, [{ value: "a", text: "A" }], ["zzz"], undefined, { all: false, values: ["a"] }), { name: "v", text: 'This value is not an option of v: "zzz". The page used "A".' });
  assert.equal(replacement({ ...v, label: "Host" }, [{ value: "a", text: "A" }], undefined, ["b"], { all: false, values: ["a"] }).text.includes("of Host:"), true);
});

function docOf(variable) {
  const base = { name: "v", label: null, hide: "none", multi: false, include_all: false, all_value: null, default: [], ...variable };
  if (base.kind === "query") Object.assign(base, { source: "s", sql: ["SELECT 1"], refresh: "load", sort: "none", regex: null, ...variable });
  else Object.assign(base, { options: [{ text: "One", value: "1" }, { text: "Two", value: "2" }], ...variable });
  return {
    format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off", time: { from: "now-6h", to: "now", timezone: "utc", refresh: null },
    variables: [base], sections: [{ title: null, collapsed: false, panels: [] }],
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

test("FE-48 runtime: one notice at the load, one after a reload that removes the value, none for an open variable, an error or an empty list", async () => {
  assert.equal(fe48.runtime.length, 10);
  for (const c of fe48.runtime) {
    let list = c.options;
    const api = {
      async variableOptions() {
        if (list === "error") throw Object.assign(new Error("options failed"), { code: "query_error" });
        return { options: list.map((value) => ({ value, text: value })) };
      },
      async panelQuery() { return { columns: [], values: [], rows: 0 }; },
    };
    const seen = [];
    const rt = new Runtime({ uid: "d", doc: docOf(c.variable), etag: "e", api, widthOf: () => 500, range: { fromText: "now-6h", toText: "now" }, urlVars: new Map(c.url ? [["v", c.url]] : []), onNotice: (n) => seen.push(n) });
    await rt.start(new Map(c.url ? [["v", c.url]] : []));
    assert.deepEqual(seen.map((n) => n.text), c.expect, c.name);
    assert.ok(seen.every((n) => n.name === "v"), c.name);
    if (c.reload) {
      seen.length = 0;
      list = c.reload;
      rt.refreshNow();
      await tick();
      await tick();
      assert.deepEqual(seen.map((n) => n.text), c.expectAfter, `${c.name}: after the reload`);
    }
    rt.destroy();
  }
});

test("FE-48 runtime: a runtime with no onNotice works, and the notice comes after the selection is set", async () => {
  const api = { async variableOptions() { return { options: [{ value: "a", text: "a" }] }; }, async panelQuery() { return { columns: [], values: [], rows: 0 }; } };
  const rt = new Runtime({ uid: "d", doc: docOf({ kind: "query" }), etag: "e", api, widthOf: () => 500, range: { fromText: "now-6h", toText: "now" } });
  await rt.start(new Map([["v", ["zzz"]]]));
  assert.deepEqual(rt.selections.get("v"), { all: false, values: ["a"] });
  rt.destroy();
  let at = null;
  const rt2 = new Runtime({ uid: "d", doc: docOf({ kind: "query" }), etag: "e", api, widthOf: () => 500, range: { fromText: "now-6h", toText: "now" }, onNotice: () => { at = rt2.selections.get("v"); } });
  await rt2.start(new Map([["v", ["zzz"]]]));
  assert.deepEqual(at, { all: false, values: ["a"] }, "the selection is set when the page hears the notice");
  rt2.destroy();
});

test("FE-48 time: 10,000 values against 10,000 options in 100 ms or less (T78)", () => {
  const options = Array.from({ length: 10000 }, (_, i) => ({ value: `option-${i}`, text: `Option ${i}` }));
  const urlValues = Array.from({ length: 10000 }, (_, i) => (i % 2 ? `option-${i}` : `typed-${i}`));
  for (const open of [false, true]) {
    const variable = { kind: "query", name: "v", multi: true, include_all: false, default: [], ...(open ? { allow_custom_value: true } : {}) };
    let best = Infinity;
    let got = null;
    for (let run = 0; run < 5; run++) {
      const t0 = performance.now();
      got = resolveSelection(variable, options, urlValues, undefined);
      droppedValues(variable, options, urlValues);
      best = Math.min(best, performance.now() - t0);
    }
    assert.equal(got.values.length, open ? 10000 : 5000);
    assert.ok(best <= 100, `${open ? "open" : "closed"}: ${best.toFixed(1)} ms`);
  }
});
