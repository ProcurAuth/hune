// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the pure logic of the editor: draft (10.2, 10.6 to 10.9), SQL text (10.3), JSON scan (10.4), save logic (10.9 to 10.13).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import * as D from "../../web/js/editor/draft.js";
import { sqlToLines, linesToSql, utf16Offset, indexOfPosition } from "../../web/js/editor/sql_text.js";
import { scanJson, locatePointer, prettyJson } from "../../web/js/editor/json_check.js";
import * as S from "../../web/js/editor/save_logic.js";

const doc = JSON.parse(readFileSync(new URL("../fixtures/dashboards/nodes.json", import.meta.url)));
const filler = (seq) => { let n = 0; return (bytes) => { for (let i = 0; i < bytes.length; i++) bytes[i] = seq[n++ % seq.length]; }; };

test("save_id is a UUID version 4 in the canonical lower-case form", () => {
  const id = D.makeSaveId(filler([255, 0, 17]));
  assert.ok(D.isSaveId(id), id);
  assert.equal(id[14], "4");
  assert.ok("89ab".includes(id[19]));
  assert.equal(id.length, 36);
  const real = D.makeSaveId((b) => crypto.getRandomValues(b));
  assert.ok(D.isSaveId(real));
  assert.ok(!D.isSaveId("not-a-uuid"));
  assert.ok(!D.isSaveId(real.toUpperCase()));
});

test("uid: 14 characters of [a-z0-9], with no bias (bytes of 252 or more are dropped)", () => {
  assert.match(D.makeUid((b) => crypto.getRandomValues(b)), /^[a-z0-9]{14}$/);
  // Bytes 252 to 255 are dropped. 0 gives "a", 1 gives "b", 35 gives "9", 36 gives "a" again.
  const uid = D.makeUid(filler([255, 252, 0, 1, 35, 36, 71]));
  assert.equal(uid.length, 14);
  assert.match(uid, /^[a-z0-9]{14}$/);
  assert.equal(uid.slice(0, 5), "ab9a9");
});

test("the smallest document is a valid shape (4.5)", () => {
  const d = D.smallestDocument("abc", "T");
  assert.equal(D.FORMAT_VERSION, 3);
  assert.equal(d.format_version, D.FORMAT_VERSION);
  assert.deepEqual(Object.keys(d), ["format_version", "uid", "title", "description", "tags", "time", "cursor_sync", "links", "variables", "sections"]);
  assert.deepEqual(d.sections, [{ title: null, collapsed: false, panels: [] }]);
  assert.deepEqual(d.time, { from: "now-6h", to: "now", timezone: "browser", refresh: null });
});

test("new panel: id, grid at the bottom, defaults, and normalization after an add (10.6)", () => {
  const p = D.newPanel(doc, 0, "prod");
  assert.equal(p.id, 4);
  assert.deepEqual(p.grid, { x: 0, y: 8, w: 12, h: 8 });
  assert.equal(p.title, "New panel");
  // The member order of 4.13: `transparent` follows `description`, and a new panel is not transparent.
  assert.deepEqual(Object.keys(p), ["id", "type", "title", "description", "transparent", "grid", "data", "transforms", "fields", "options", "links"]);
  assert.equal(p.transparent, false);
  assert.deepEqual(p.data.query, { source: "prod", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null });
  assert.deepEqual(p.fields.defaults.thresholds.steps, [{ value: null, color: "green" }, { value: 80, color: "red" }]);
  assert.deepEqual(p.options, { show_header: true, sort: [] });
  const next = D.addPanel(doc, 0, p);
  assert.equal(next.sections[0].panels.length, 3);
  assert.equal(doc.sections[0].panels.length, 2, "the input stays");
});

test("duplicate: same title, new id, y of the original plus h; a reuse keeps its source", () => {
  const { doc: next, panel } = D.duplicatePanel(doc, 3);
  assert.equal(panel.id, 4);
  assert.equal(panel.title, "Total weight");
  assert.deepEqual(panel.data.reuse, { panel: 2, with_transforms: true });
  assert.equal(next.sections[0].panels.find((p) => p.id === 4).grid.y, 8);
  // The copy keeps the member `transparent` of the original (4.13).
  const flagged = structuredClone(doc);
  flagged.sections[0].panels.find((p) => p.id === 3).transparent = true;
  assert.equal(D.duplicatePanel(flagged, 3).panel.transparent, true);
});

test("delete refuses a panel that others reuse; else normalizes", () => {
  const refused = D.deletePanel(doc, 2);
  assert.deepEqual(refused.refused, ["Total weight"]);
  const ok = D.deletePanel(doc, 3);
  assert.equal(D.allPanels(ok.doc).length, 2);
  const none = D.deletePanel(doc, 1);
  assert.equal(none.doc.sections[0].panels.length, 1);
});

test("sections: add, rename with the title rule, collapse", () => {
  const added = D.addSection(doc);
  assert.deepEqual(added.sections.at(-1), { title: "New section", collapsed: false, panels: [] });
  assert.ok(D.renameSection(doc, 1, "").error);
  assert.equal(D.renameSection(doc, 1, "X").doc.sections[1].title, "X");
  assert.equal(D.setCollapsed(doc, 1, true).sections[1].collapsed, true);
});

test("sections: move and delete (a reused panel or the last section blocks the delete)", () => {
  assert.equal(D.moveSection(doc, 0, -1), null);
  assert.deepEqual(D.moveSection(doc, 0, 1).sections.map((x) => x.title), [doc.sections[1].title, doc.sections[0].title]);
  assert.ok(D.deleteSection({ ...doc, sections: [doc.sections[0]] }, 0).error);
  const gone = D.deleteSection(D.addSection(doc), 2);
  assert.equal(gone.doc.sections.length, 2);
  assert.ok(D.deleteSection(doc, 1).refused || D.deleteSection(doc, 1).doc);
});

test("a new type keeps shared members and takes the New options; table-only properties leave", () => {
  const table = doc.sections[1].panels[0];
  const stat = D.changeType(table, "stat");
  assert.equal(stat.type, "stat");
  assert.deepEqual(stat.options, D.newOptions("stat"));
  assert.equal(stat.title, table.title);
  assert.deepEqual(stat.data, table.data);
  const sets = stat.fields.overrides.map((o) => Object.keys(o.set));
  for (const keys of sets) for (const k of D.TABLE_ONLY_PROPERTIES) assert.ok(!keys.includes(k));
  assert.ok(stat.fields.overrides.every((o) => Object.keys(o.set).length > 0));
  for (const type of D.PANEL_TYPES) assert.ok(D.newOptions(type));
});

test("draft records: read, discard rules, notices, difference", () => {
  const rec = D.makeRecord({ baseEtag: "e1", baseVersion: 3, subject: "-", folder: "transit", document: doc, now: 1000 });
  const text = JSON.stringify(rec);
  assert.equal(D.readRecord(text, "nodes", "-").draft.base_etag, "e1");
  assert.equal(D.readRecord("{bad", "nodes", "-").discard, "invalid");
  assert.equal(D.readRecord(text, "other", "-").discard, "uid");
  assert.equal(D.readRecord(text, "nodes", "user:bob").discard, "subject");
  assert.equal(D.readRecord(JSON.stringify({ a: 1 }), "nodes", "-").discard, "invalid");
  // A draft of the format before the migration of 4.42 cannot be saved: it is discarded.
  const oldDraft = JSON.stringify({ ...rec, document: { ...rec.document, format_version: 1 } });
  assert.equal(D.readRecord(oldDraft, "nodes", "-").discard, "format");
  assert.equal(D.draftKey("x1"), "hune.draft.v1.x1");
  assert.equal(D.resumeNotice(rec, "e1", () => "10:00"), "You have changes that are not saved, from 10:00.");
  assert.equal(D.resumeNotice(rec, "e2", () => "10:00"), "You have changes that are not saved, from 10:00. The dashboard changed since then.");
  assert.equal(D.draftDiffers(rec, doc), false);
  const changed = structuredClone(rec);
  changed.document.title = "x";
  assert.equal(D.draftDiffers(changed, doc), true);
  const withText = { ...rec, json_text: "{}" };
  assert.equal(D.draftDiffers(withText, doc), true);
  assert.equal(D.sameJson({ a: 1, b: [1, 2] }, { b: [1, 2], a: 1 }), true);
  assert.equal(D.sameJson({ a: 1 }, { a: 1, b: 2 }), false);
});

test("SQL text: lines, CR LF, byte columns to UTF-16 units", () => {
  assert.deepEqual(sqlToLines("a\r\nb\nc"), ["a", "b", "c"]);
  assert.equal(linesToSql(["a", "b"]), "a\nb");
  assert.equal(utf16Offset("abc", 2), 2);
  assert.equal(utf16Offset("é1", 2), 1); // é is two bytes
  assert.equal(utf16Offset("é1", 1), 0); // inside a character: its start
  assert.equal(utf16Offset("😀x", 4), 2); // an emoji: 4 bytes, 2 UTF-16 units
  assert.equal(utf16Offset("ab", 99), 2);
  assert.equal(indexOfPosition(["SELECT 1", "FROM é t"], 2, 8), 9 + 6);
});

test("JSON scan: syntax faults with line and column, duplicates, pointers", () => {
  const ok = scanJson('{\n  "a": [1, {"b": true}],\n  "c": "x"\n}');
  assert.ok(ok.ok);
  assert.equal(ok.value.c, "x");
  const at = locatePointer(ok.positions, "/a/1/b");
  assert.equal('{\n  "a": [1, {"b": true}],\n  "c": "x"\n}'.slice(at, at + 3), '"b"');
  assert.equal(locatePointer(ok.positions, "/a/9/zz") >= 0, true);
  const bad = scanJson('{\n  "a": 1,\n  "b": ,\n}');
  assert.equal(bad.ok, false);
  assert.deepEqual([bad.errors[0].code, bad.errors[0].line, bad.errors[0].column], ["syntax", 3, 8]);
  const dup = scanJson('{"a": 1, "a": 2, "b": {"c": 1, "c": 2}}');
  assert.deepEqual(dup.errors.map((e) => [e.pointer, e.code, e.line, e.column]), [["/a", "duplicate_member", 1, 10], ["/b/c", "duplicate_member", 1, 32]]);
  assert.equal(dup.ok, false);
  assert.ok(!scanJson("[1,]").ok);
  assert.ok(!scanJson('{"a": 1} x').ok);
  assert.ok(!scanJson('"abc').ok);
  assert.ok(scanJson('"\\u00e9"').ok);
  assert.ok(!scanJson("01").ok);
  assert.ok(scanJson('{"~/": 1}').positions.has("/~0~1"));
  assert.equal(prettyJson({ a: 1 }), '{\n  "a": 1\n}\n');
  assert.ok(Object.hasOwn(scanJson('{"__proto__": 1}').value, "__proto__"));
});

test("save: one save_id for each attempt, the same value on retry, a new one after a change", () => {
  const attempt = new S.SaveAttempt((b) => crypto.getRandomValues(b));
  const first = attempt.id();
  assert.equal(attempt.id(), first);
  attempt.reset();
  assert.notEqual(attempt.id(), first);
});

test("save: the table of answers (10.9)", () => {
  const e = (code, status, extra = {}) => Object.assign(new Error("m"), { code, status, extra });
  assert.equal(S.classifySaveError(e("conflict", 409)).kind, "conflict");
  assert.equal(S.classifySaveError(e("invalid_document", 422)).kind, "invalid");
  assert.deepEqual(S.classifySaveError(e("idempotency_key_reused", 422)), { kind: "reused", text: "Select Save again." });
  assert.equal(S.classifySaveError(e("storage_unavailable", 503)).kind, "retry");
  assert.equal(S.classifySaveError(e("network_error", 0)).kind, "retry");
  assert.equal(S.classifySaveError(e("unauthenticated", 401)).kind, "unauthenticated");
  assert.equal(S.classifySaveError(e("forbidden", 403)).kind, "show");
  assert.equal(S.classifySaveError(e("payload_too_large", 413)).text, "The dashboard is too large.");
  assert.equal(S.classifySaveError(e("storage_full", 507)).text, "The store is full. Tell the operator.");
  assert.equal(S.classifySaveError(e("sql_invalid", 422)).text, "sql_invalid: m");
  assert.equal(S.savedText({ version: 4 }), "Saved as version 4.");
  assert.equal(S.savedText({ noop: true, version: 4 }), "Nothing changed.");
});

test("conflict kinds, delete retry, draft file name, Grafana files, validate body", () => {
  const err = (extra) => ({ extra });
  assert.equal(S.conflictKind(err({ current_etag: "a", current_folder: "ops" }), "a", "transit"), "move");
  assert.equal(S.conflictKind(err({ current_etag: "b", current_folder: "transit" }), "a", "transit"), "changed");
  assert.equal(S.conflictKind(err({ current_etag: "a", current_folder: "transit" }), "a", "transit"), "changed");
  assert.equal(S.deleteRetryIsDone({ code: "not_found" }), true);
  assert.equal(S.deleteRetryIsDone({ code: "conflict" }), false);
  assert.equal(S.draftFileName("v1", new Date(2026, 10, 9, 8, 5, 3)), "v1-draft-20261109-080503.json");
  assert.ok(S.isGrafanaFile({ apiVersion: "dashboard.grafana.app/v1beta1" }));
  assert.ok(S.isGrafanaFile({ schemaVersion: 39, panels: [] }));
  assert.ok(S.isGrafanaFile({ dashboard: { panels: [] } }));
  assert.ok(!S.isGrafanaFile(doc));
  assert.ok(!S.isGrafanaFile(null));
  assert.equal(S.validateBody('{"a":1}'), '{"document":{"a":1}}');
});

test("the panel editor has the switch 'Transparent background' in the group 'Panel options' (4.13)", () => {
  const js = readFileSync(new URL("../../web/js/editor/panel_editor.js", import.meta.url), "utf8");
  assert.match(js, /label: "Transparent background"/);
  assert.match(js, /checked: p\.transparent === true/);
  assert.match(js, /apply\(\(x\) => \(\{ \.\.\.x, transparent: checked \}\)\)/);
  assert.match(js, /optionGroup\(\{ key: "panel", title: "Panel options", open, children: \[own\("title", title\.node\), own\("description", description\.node\), own\("transparent", [^\]]*transparent\.el[^\]]*\)\] \}\)/);
});

// --- the text panel (4.33a, 9.19b, 10.3, 10.6) ---------------------------------------------------------------------------------

test("the new text panel of 10.6: the members of 4.33a, no data, no transforms, no fields", () => {
  const d = D.smallestDocument("abc", "T");
  d.sections[0].panels.push({ ...D.newTextPanel(d, 0), id: 4, grid: { x: 0, y: 0, w: 12, h: 8 } });
  const p = D.newTextPanel(d, 0);
  assert.deepEqual(p, {
    id: 5, type: "text", title: "New text", description: "", transparent: false, grid: { x: 0, y: 8, w: 12, h: 8 },
    options: { mode: "markdown", content: ["# Title", "", "Write text in Markdown."] }, links: [],
  });
  for (const key of ["data", "transforms", "fields", "repeat"]) assert.equal(key in p, false, key);
  assert.deepEqual(D.PANEL_TYPES.slice(-1), ["text"]);
  assert.equal(D.isDrawable("text"), true);
  assert.deepEqual(D.addPanel(d, 0, p).sections[0].panels.map((x) => x.id), [4, 5]);
});

test("a change to text removes the query, the transformations and the fields; a change from text gives the query of a new panel (10.3)", () => {
  const table = { id: 1, type: "table", title: "T", description: "d", transparent: true, grid: { x: 0, y: 0, w: 12, h: 8 }, data: { query: { source: "s1", format: "table", sql: ["SELECT 2"], min_interval: null, max_data_points: null } }, transforms: [{ id: "x" }], fields: { defaults: { unit: "ms" }, overrides: [] }, options: D.newOptions("table"), links: [], repeat: { variable: "v", direction: "h" } };
  const text = D.changeType(table, "text");
  assert.deepEqual(Object.keys(text).sort(), ["description", "grid", "id", "links", "options", "title", "transparent", "type"]);
  assert.deepEqual(text.options, D.newOptions("text"));
  assert.equal(text.transparent, true);
  assert.equal(table.type, "table", "the input is not changed");
  const back = D.changeType(text, "stat", "s9");
  assert.equal(back.type, "stat");
  assert.deepEqual(back.data, { query: { source: "s9", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null } });
  assert.deepEqual(back.transforms, []);
  assert.deepEqual(back.fields.overrides, []);
  assert.deepEqual(back.options, D.newOptions("stat"));
  assert.equal(back.title, "T");
  assert.equal(D.changeType(table, "stat").data.query.sql[0], "SELECT 2", "a change between two types with a query keeps the query");
});

test("a text panel is never the source of a reuse and never repeats (4.33a, rule 6)", () => {
  const d = D.smallestDocument("abc", "T");
  d.sections[0].panels.push({ ...D.newTextPanel(d, 0), id: 1 });
  assert.equal(D.canBeReuseSource(d, d.sections[0].panels[0]), false);
  assert.match(D.panelRepeatRefusal(d, 1), /no query/);
  assert.deepEqual(D.reusersOf(d, 1), []);
  assert.equal(D.deletePanel(d, 1).doc.sections[0].panels.length, 0);
  assert.equal(D.duplicatePanel(d, 1).panel.type, "text");
  assert.equal(D.sectionRepeatRefusal({ ...d, sections: [{ title: null, collapsed: false, panels: d.sections[0].panels }], variables: [] }, 0) !== undefined, true);
});

test("the New options of the timeline types are the table of 9.19c, in the order of 4.33b", () => {
  assert.deepEqual(D.newOptions("state_timeline"), {
    merge_values: true, show_values: "auto", align_values: "left", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true,
    legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single", sort: "none" },
  });
  assert.deepEqual(D.newOptions("status_history"), {
    show_values: "auto", row_height: 90, col_width: 90, line_width: 1, fill_opacity: 70, show_labels: true,
    legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single", sort: "none" },
  });
  assert.deepEqual(Object.keys(D.newOptions("state_timeline")), ["merge_values", "show_values", "align_values", "row_height", "line_width", "fill_opacity", "span_nulls", "show_labels", "legend", "tooltip"]);
  assert.ok(!("reduce" in D.newOptions("state_timeline")) && !("reduce" in D.newOptions("status_history")));
  assert.deepEqual(D.TIMELINE_TYPES, ["state_timeline", "status_history"]);
  assert.deepEqual(D.REDUCE_TYPES, ["stat", "piechart", "bargauge", "gauge"]);
});

test("a change of the type to or from a timeline keeps data, transforms and fields, and takes the New options (10.3)", () => {
  const stat = { id: 1, type: "stat", title: "T", description: "", transparent: false, grid: { x: 0, y: 0, w: 12, h: 8 }, data: { query: { source: "s1", format: "table", sql: ["SELECT 2"], min_interval: null, max_data_points: null } }, transforms: [{ id: "x" }], fields: { defaults: { unit: "ms", mappings: [{ kind: "value", value: "1", text: "UP", color: "green" }] }, overrides: [] }, options: D.newOptions("stat"), links: [] };
  const timeline = D.changeType(stat, "state_timeline");
  assert.equal(timeline.type, "state_timeline");
  assert.deepEqual(timeline.options, D.newOptions("state_timeline"));
  assert.deepEqual([timeline.data, timeline.transforms, timeline.fields], [stat.data, stat.transforms, stat.fields]);
  const history = D.changeType(timeline, "status_history");
  assert.deepEqual(history.options, D.newOptions("status_history"));
  assert.deepEqual(history.fields, stat.fields);
  assert.deepEqual(D.changeType(history, "bargauge").options, D.newOptions("bargauge"));
  assert.equal(D.isDrawable("state_timeline") && D.isDrawable("status_history"), true);
  assert.equal(D.canBeReuseSource({ sections: [{ panels: [timeline] }] }, timeline), true, "a timeline can be the source of a reuse");
});

// --- the annotations and the delete of a panel (10.3b, 10.6) -----------------------------------------------------------------------

const withAnnotations = () => ({
  ...doc,
  annotations: [
    { name: "Only 3", source: "s", sql: ["SELECT 1"], color: "red", panels: [3] },
    { name: "Two", source: "s", sql: ["SELECT 1"], color: "blue", panels: [3, 1] },
    { name: "All", source: "s", sql: ["SELECT 1"], color: "green" },
  ],
});

test("the delete dialog lists the annotations that name the panel, and which ones go off", () => {
  assert.deepEqual(D.annotationsNaming(withAnnotations(), [3]), [{ name: "Only 3", only: true }, { name: "Two", only: false }]);
  assert.deepEqual(D.annotationsNaming(withAnnotations(), [1]), [{ name: "Two", only: false }]);
  assert.deepEqual(D.annotationsNaming(doc, [3]), []);
});

test("deletePanel takes the id out of panels in the same change; an empty list goes and the annotation is off", () => {
  const before = withAnnotations();
  const result = D.deletePanel(before, 3);
  assert.deepEqual(result.off, ["Only 3"]);
  const [a, b, c] = result.doc.annotations;
  assert.ok(!("panels" in a));
  assert.equal(a.enabled, false);
  assert.deepEqual(b.panels, [1]);
  assert.equal(b.enabled, undefined);
  assert.ok(!("panels" in c));
  assert.deepEqual(before.annotations[0].panels, [3], "the input stays as it was");
  assert.deepEqual(D.deletePanel(doc, 3).off, []);
});

test("deleteSection takes the ids of its panels out of the annotations", () => {
  const d = {
    variables: [],
    sections: [{ title: "S1", panels: [{ id: 1 }] }, { title: "S2", panels: [{ id: 2 }] }],
    annotations: [{ name: "A", panels: [2] }, { name: "B", panels: [1, 2] }],
  };
  const result = D.deleteSection(d, 1);
  assert.deepEqual(result.off, ["A"]);
  assert.equal(result.doc.annotations[0].enabled, false);
  assert.ok(!("panels" in result.doc.annotations[0]));
  assert.deepEqual(result.doc.annotations[1].panels, [1]);
});

test("F41: an open variable keeps allow_custom_value before placement through the placement changes of the draft (4.38, 10.3b)", () => {
  const open = { kind: "query", name: "svc", label: null, hide: "none", source: "s", sql: ["SELECT 1"], multi: false, include_all: false, all_value: null, refresh: "load", sort: "none", regex: null, default: [], allow_custom_value: true };
  const base = { ...D.smallestDocument("abc", "T"), variables: [open] };
  const placed = D.setPlacement(base, "svc", { section: 0, x: 0, y: 0, w: 6, h: 2 });
  assert.deepEqual(Object.keys(placed.variables[0]).slice(-3), ["default", "allow_custom_value", "placement"]);
  const back = D.setPlacement(placed, "svc", null);
  assert.deepEqual(Object.keys(back.variables[0]), Object.keys(open));
  assert.equal(back.variables[0].allow_custom_value, true);
  // The record of the draft is plain data: the member goes through a copy with no change.
  assert.deepEqual(JSON.parse(JSON.stringify(placed)).variables[0], placed.variables[0]);
  // A rename of the variable keeps the member.
  const renamed = D.renameRepeatVariable({ ...base, variables: [{ ...open, name: "svc2" }] }, "svc", "svc2");
  assert.equal(renamed.variables[0].allow_custom_value, true);
});
