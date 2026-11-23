// FE-35 and FE-36 (9.19d, 4.35): the pure module web/js/annotations.js. The vectors are in tests/frontend/vectors/.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import * as A from "../../web/js/annotations.js";

const load = (name) => JSON.parse(readFileSync(new URL(`./vectors/${name}`, import.meta.url)));
const fe35 = load("FE-35-annotations.json");
const fe36 = load("FE-36-annotations-geometry.json");

test("FE-35: the constants of the module equal the table of 4.35", () => {
  for (const [name, value] of Object.entries(fe35.constants)) assert.equal(A[name], value, name);
  assert.equal(A.MAX_ANNOTATION_PANELS, 200);
});

test("FE-35: forPanel gives the annotations that apply to a panel, in the order of the array", () => {
  assert.equal(fe35.forPanel.length, 10);
  for (const v of fe35.forPanel) assert.deepEqual(A.forPanel(v.annotations ? { annotations: v.annotations } : {}, v.id), v.expect, v.name);
});

test("FE-35: readAnswer accepts 10 answers and refuses 20, with one fixed message", () => {
  const good = fe35.readAnswer.filter((v) => v.ok);
  const bad = fe35.readAnswer.filter((v) => !v.ok);
  assert.deepEqual([good.length, bad.length], [10, 20]);
  for (const v of good) {
    const marks = A.readAnswer(v.answer);
    assert.equal(marks.length, v.answer.annotations.length, v.name);
    marks.forEach((m, i) => assert.deepEqual([m.t, m.end, m.title, m.text, m.tags], v.answer.annotations[i], v.name));
  }
  for (const v of bad) assert.throws(() => A.readAnswer(v.answer), { message: "The answer of the annotation is not valid." }, v.name);
  assert.throws(() => A.readAnswer({ rows: 1, annotations: [[Infinity, null, "", "", []]] }));
  assert.throws(() => A.readAnswer({ rows: 1, annotations: [[NaN, null, "", "", []]] }));
});

test("FE-35: marks gives the order, the tie, the range, the colour and the failed annotations", () => {
  for (const v of fe35.marks) {
    const got = A.marks(v.records, v.lo, v.hi);
    assert.equal(got.cut, v.cut, v.name);
    assert.deepEqual(got.list.map((m) => [m.t, m.index]), v.expect, v.name);
    if (v.titles) assert.deepEqual(got.list.map((m) => m.title), v.titles, v.name);
    if (v.colors) assert.deepEqual(got.list.map((m) => m.color), v.colors, v.name);
    assert.deepEqual(got.failed.map((f) => f.name), v.failed || [], v.name);
  }
});

test("FE-35: the cap of 2,000 marks for a panel is fail closed: no cut and no sample", () => {
  for (const v of fe35.cap) {
    const records = v.per.map((n, index) => (n === "error"
      ? { index, a: { name: `n${index}`, color: "red" }, items: [], error: { message: "m" } }
      : { index, a: { name: `n${index}`, color: "red" }, items: Array.from({ length: n }, (_, k) => ({ t: v.outside ? -k - 1 : k, end: null, title: "", text: "", tags: [] })), error: null }));
    const got = A.marks(records, 0, 1e9);
    assert.equal(got.n, v.n, v.name);
    assert.equal(got.cut, v.cut, v.name);
    assert.equal(got.list.length, v.cut || v.outside ? 0 : v.n, v.name);
  }
});

test("FE-35: the note of the title bar: the cap, or the annotations that failed", () => {
  assert.equal(A.noteOf({ n: 3, cut: false, failed: [] }), null);
  const cut = A.noteOf({ n: 2001, cut: true, failed: [] });
  assert.equal(cut.text, "Annotations: too many (2001, at most 2,000)");
  const failed = A.noteOf({ n: 0, cut: false, failed: [{ name: "Deploys", error: { code: "query_limit", message: "Too many rows." } }, { name: "B", error: { message: "x" } }] });
  assert.equal(failed.text, "Annotations: 2 failed");
  assert.equal(failed.tip, "Deploys: query_limit: Too many rows.\nB: x");
  assert.equal(A.noteOf({ n: 2001, cut: true, failed: [{ name: "B", error: { message: "x" } }] }).tip.split("\n").length, 2);
});

test("FE-36: the x of a mark, the hit zone, the nearest mark, the card and the keys", () => {
  assert.equal(fe36.vectors.length, 40);
  const xOf = (t) => fe36.scale.left + t / fe36.scale.divisor;
  const plot = fe36.plot;
  for (const v of fe36.vectors) {
    switch (v.kind) {
      case "span": {
        const [a, b] = A.span(v.mark, xOf);
        assert.ok(Math.abs(a - v.expect[0]) < 1e-9 && Math.abs(b - v.expect[1]) < 1e-9, v.name);
        break;
      }
      case "zone": assert.equal(A.inZone(plot, v.py), v.expect, v.name); break;
      case "hit": {
        const got = A.hitTest(v.list, xOf, plot, v.px, v.py);
        assert.equal(got === null ? null : v.list.indexOf(got), v.expect, v.name);
        break;
      }
      case "card": assert.deepEqual(A.cardPlace(plot, v.px, v.cw, v.ch), v.expect, v.name); break;
      case "key": assert.equal(A.stepKey(v.count, v.at, v.key), v.expect, v.name); break;
      default: assert.fail(v.kind);
    }
  }
});

test("FE-36: the texts of the card and the rows of the table of the marks", () => {
  const fmt = (ms) => `T${ms}`;
  assert.deepEqual(A.cardText({ t: 1, end: null, title: "A", text: "B", tags: ["x"] }, fmt), { head: "T1", body: "A\nB", tags: ["x"] });
  assert.equal(A.cardText({ t: 1, end: 5, title: "", text: "B", tags: [] }, fmt).head, "T1 - T5");
  assert.equal(A.cardText({ t: 1, end: null, title: "A", text: "", tags: [] }, fmt).body, "A");
  assert.equal(A.cardText({ t: 1, end: null, title: "", text: "", tags: Array.from({ length: 12 }, String) }, fmt).tags.length, 8);
  assert.deepEqual(A.tableRows([{ t: 1, end: null, title: "A", text: "B", tags: ["x", "y"] }, { t: 2, end: 3, title: "", text: "", tags: [] }], fmt), [["T1", "", "A", "B", "x, y"], ["T2", "T3", "", "", ""]]);
});

test("FE-36: the hook data-annotations has 5 members and 200 items at most", () => {
  const list = Array.from({ length: 250 }, (_, i) => ({ t: i * 10, end: i % 2 ? i * 10 + 50 : null, color: "red", index: 3 }));
  const info = { n: 250, cut: false };
  const hook = A.hookOf(info, list, (t) => 50 + t / 10, { left: 50, top: 10, w: 400, h: 200 }, (c) => `ink-${c}`);
  assert.deepEqual(Object.keys(hook), ["n", "drawn", "cut", "plot", "marks"]);
  assert.equal(hook.drawn, 250);
  assert.equal(hook.marks.length, 200);
  assert.deepEqual(hook.marks[0], [50, 50, 0, "ink-red", 3]);
  assert.deepEqual(hook.marks[1], [51, 56, 1, "ink-red", 3]);
  assert.deepEqual(hook.plot, [50, 10, 400, 200]);
});

test("FE-35: marks and hitTest for 2,000 and 10,000 marks are fast (5 ms and 20 ms on the reference machine)", () => {
  const records = (n) => [{ index: 0, a: { name: "A", color: "red" }, items: Array.from({ length: n }, (_, i) => ({ t: (i * 7919) % 100000, end: i % 5 ? null : (i * 7919) % 100000 + 30, title: "", text: "", tags: [] })), error: null }];
  const best = (fn) => {
    let low = Infinity;
    for (let k = 0; k < 7; k++) {
      const t0 = performance.now();
      fn();
      low = Math.min(low, performance.now() - t0);
    }
    return low;
  };
  const plot = { left: 0, top: 0, w: 1000, h: 200 };
  const xOf = (t) => t / 100;
  const r2 = records(2000);
  const two = A.marks(r2, 0, 1e6);
  assert.equal(two.list.length, 2000);
  assert.ok(best(() => A.marks(r2, 0, 1e6)) < 5, "marks 2,000");
  assert.ok(best(() => A.hitTest(two.list, xOf, plot, 500, 195)) < 5, "hitTest 2,000");
  const big = records(10000);
  assert.equal(A.marks(big, 0, 1e6).cut, true);
  assert.ok(best(() => A.marks(big, 0, 1e6)) < 20, "marks 10,000 (cut)");
  const all = big[0].items.map((m, i) => ({ ...m, index: 0, color: "red", i }));
  assert.ok(best(() => A.hitTest(all, xOf, plot, 500, 195)) < 20, "hitTest 10,000");
});
