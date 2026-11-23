// SPDX-License-Identifier: MIT OR Apache-2.0
// The verdict rule of the audit B153 (10.3h, rule 14; FE-79): one synthetic result for each verdict.
import test from "node:test";
import assert from "node:assert/strict";
import { MAX_EXEMPT, changedMembers, judge, membersWithoutControl, mergeExempt } from "../browser/lib/audit_rules.mjs";

const ok = (over = {}) => ({ type: "timeseries", state: "new panel", pointer: "options.draw", kind: "radio", draft: ["options.draw"], preview: true, back: true, skipped: null, ...over });
const base = { exempt: [], shown: { "timeseries|new panel": ["options.draw"] }, found: { "timeseries|new panel": ["options.draw"] } };
const run = (results, over = {}) => judge({ ...base, results, ...over });
const CAUSE = "the card shows on hover only, so the print is the same";
const entry = (over = {}) => ({ pointer: "options.draw", types: ["timeseries"], cls: "design", cause: CAUSE, ...over });

test("a control with an effect passes", () => assert.deepEqual(run([ok()]), []));

test("verdict: no effect on the preview", () => {
  const f = run([ok({ preview: false })]);
  assert.equal(f.length, 1);
  assert.match(f[0], /the preview does not change/);
});

test("verdict: a control that changes nothing fails", () => assert.match(run([ok({ draft: [], preview: false })])[0], /the draft does not change/));

test("verdict: a wrong member fails", () => {
  assert.match(run([ok({ draft: ["options.line_width"] })])[0], /changes other members: options.line_width/);
  assert.match(run([ok({ draft: ["options.draw", "options.stacking"] })])[0], /changes other members: options.stacking/);
});

test("the writes of the row and the members under an old control are permitted", () => {
  assert.deepEqual(run([ok({ pointer: "options.legend.sort_by", draft: ["options.legend.sort_by", "options.legend.sort_desc"], writes: ["options.legend.sort_desc"] })], { shown: {}, found: {} }), []);
  assert.deepEqual(run([ok({ pointer: "fields.defaults.cell", draft: ["fields.defaults.cell.type"] })], { shown: {}, found: {} }), []);
});

test("an entry lets a control with no preview effect pass", () => assert.deepEqual(run([ok({ preview: false })], { exempt: [entry()] }), []));

test("verdict: a stale entry fails", () => assert.match(run([ok()], { exempt: [entry()] })[0], /stale/));

test("verdict: a dead entry fails", () => {
  const f = run([ok()], { exempt: [entry({ pointer: "options.color_mode", types: ["stat"] })] });
  assert.equal(f.length, 1);
  assert.match(f[0], /dead entry/);
});

test("an entry needs a class and a cause of 20 characters or more", () => {
  assert.ok(run([ok({ preview: false })], { exempt: [entry({ cls: "lazy" })] }).some((m) => /not valid/.test(m)));
  assert.ok(run([ok({ preview: false })], { exempt: [entry({ cause: "short" })] }).some((m) => /not valid/.test(m)));
});

test("verdict: an unstable print fails", () => assert.match(run([ok({ back: false })])[0], /the preview is not stable/));

test("verdict: a row that the table lists and the pane lacks fails (a type with no form)", () => {
  const f = run([ok()], { found: { "timeseries|new panel": [] }, shown: { "timeseries|new panel": ["options.draw", "options.interpolation"] } });
  assert.equal(f.length, 2);
  assert.match(f[0], /the pane has no control for it/);
});

test("verdict: a control with no row (or no data-option) fails", () => {
  assert.match(run([ok()], { found: { "timeseries|new panel": ["options.draw", "options.zzz"] } })[0], /the table has no row for it/);
});

test("verdict: a control that was not tried fails", () => assert.match(run([ok({ skipped: "no other value" })])[0], /not tried/));

test("verdict: a list of 31 entries fails", () => {
  const exempt = Array.from({ length: MAX_EXEMPT + 1 }, (_, i) => ({ pointer: `options.m${i}`, types: ["t"], cls: "design", cause: "a written cause of enough length" }));
  assert.ok(run([ok()], { exempt }).some((m) => /the cap is 30/.test(m)));
  assert.ok(!run([ok()], { exempt: exempt.slice(0, 30) }).some((m) => /the cap is/.test(m)));
});

test("an entry covers the members under its member, and an entry with a value covers that choice only", () => {
  const sub = { pointer: "fields.defaults.thresholds", types: null, cls: "design", cause: "the colour mode of these types does not use the thresholds" };
  assert.deepEqual(run([ok({ pointer: "fields.defaults.thresholds.mode", draft: ["fields.defaults.thresholds.mode"], preview: false })], { exempt: [sub], shown: {}, found: {} }), []);
  const exempt = [{ pointer: "options.tooltip.mode=multi", types: ["piechart"], cls: "design", cause: "a pie has one slice under the pointer, so All equals Single" }];
  const a = ok({ type: "piechart", pointer: "options.tooltip.mode", value: "multi", draft: ["options.tooltip.mode"], preview: false });
  const b = ok({ type: "piechart", pointer: "options.tooltip.mode", value: "hidden", draft: ["options.tooltip.mode"], preview: false });
  assert.deepEqual(run([a], { exempt, shown: {}, found: {} }), []);
  assert.match(run([b, a], { exempt, shown: {}, found: {} }).join(), /the preview does not change/);
});

test("the lists of the three owners merge: the types of an entry add up", () => {
  const a = [{ pointer: "fields.defaults.min", types: ["state_timeline"], cls: "data", cause: "no scale uses the limit with this data" }];
  const b = [{ pointer: "fields.defaults.min", types: ["stat"], cls: "data", cause: "no scale uses the limit with this data" }];
  const merged = mergeExempt([a, b]);
  assert.equal(merged.length, 1);
  assert.deepEqual(merged[0].types, ["state_timeline", "stat"]);
  assert.equal(mergeExempt([a, [{ ...b[0], cls: "design" }]]).length, 2);
});

test("a member with no row and no NO_CONTROL entry is found", () => {
  const rows = ["options.draw"];
  assert.deepEqual(membersWithoutControl({ members: ["options.draw", "options.legend.values", "options.tooltip.sort"], rows, noControl: { "options.tooltip.sort": "cause" } }), ["options.legend.values"]);
});

test("the changed members are the leaves of the difference", () => {
  const a = { options: { draw: "line", legend: { show: true, values: [] } }, fields: { defaults: { unit: "ms" } } };
  const b = { options: { draw: "bars", legend: { show: true, values: ["max"] } }, fields: { defaults: {} } };
  assert.deepEqual(changedMembers(a, b).sort(), ["fields.defaults.unit", "options.draw", "options.legend.values"]);
  assert.deepEqual(changedMembers(a, a), []);
});
