// The rules and the data of the forms (10.3a, 10.3b): grammars, limits, new items, lists, variables. Each new item uses members of section 4 only.
import test from "node:test";
import assert from "node:assert/strict";
import { createScope } from "../../web/js/editor/forms/controls.js";
import {
  LIMITS, MAPPING_KINDS, MATCHER_KINDS, atLimit, duplicateValueIndexes, fieldNameHints, mappingOfKind, matcherOf, moveItem, newDashboardLink, newFilterMatcher, newLink, newMapping,
  newOverride, newTransform, newVariable, annotationFaults, newAnnotation, staleAnnotations, uniqueAnnotationName, withAnnotations, removeItem, replaceItem, setQueryMember, stalePanels, staleVariables, uniqueVariableName, variableCycle,
} from "../../web/js/editor/forms/model.js";
import {
  byteLength, checkBytes, checkChars, checkPattern, checkTag, checkTimeFrom, checkTimeOrder, checkTimeShift, checkTimeText, checkUrlTemplate, checkVariableName, checkZoneName, inCommonSubset, parseInteger, parseNumber,
} from "../../web/js/editor/forms/rules.js";
import { annotationSources } from "../../web/js/editor/forms/annotations.js";
import { defaultErrors, cycleError } from "../../web/js/editor/forms/variables.js";
import {
  canBeReuseSource, lastPlainSection, panelRepeatRefusal, removeRepeatsOf, renameRepeatVariable, repeatUsers, repeatableVariables, sectionRepeatRefusal, setPanelRepeat, setSectionRepeat,
} from "../../web/js/editor/draft.js";
import { previewRange } from "../../web/js/editor/preview_body.js";
import { panelRange } from "../../web/js/panel_time.js";

const keys = (o) => Object.keys(o);

test("patterns: the common subset of 4.24", () => {
  for (const ok of ["Total", "/^a.*$/i", "/a|b/gims", "(?:a|b){2,3}", "(?<n>a)", "\\d+\\.\\w", "[a-z\\]]", "\\x41", "a{2}", "a{2,}", "\\$\\."]) assert.equal(checkPattern(ok), "", ok);
  for (const bad of ["", "/", "/a/z", "/a/ii", "\\s", "\\A", "\\p{L}", "\\u0041", "\\1", "(?=a)", "(?i)a", "(?<=a)b", "(?<1a>x)", "[[:alpha:]]", "[a&&b]", "[a--b]", "[a[b]]", "a{,2}", "a{x}", "(", "[a", "x".repeat(1025)]) {
    assert.equal(checkPattern(bad), "The pattern is not valid.", JSON.stringify(bad));
  }
  // The plain form of a matcher has no slashes of the Grafana form: the text is the pattern.
  assert.equal(checkPattern("/a/z", { grafana: false }), "");
  assert.equal(checkPattern("/a/z"), "The pattern is not valid.");
  assert.ok(inCommonSubset("a\\/b"));
});

test("URLs: the prefix rule of 4.23", () => {
  for (const ok of ["/x", "?a=1", "#f", "http://x", "https://x/${__value.raw}", "${__value.raw}", "https://", "HTTPS://x"]) assert.equal(checkUrlTemplate(ok), "", ok);
  for (const bad of ["", "//x", "/\\x", "javascript:alert(1)", "ftp://x", "x/y", "a${b}"]) assert.notEqual(checkUrlTemplate(bad), "", bad);
  assert.equal(checkUrlTemplate("/" + "a".repeat(2047)), "");
  assert.equal(checkUrlTemplate("/" + "a".repeat(2048)), "The URL has 2048 bytes at most.");
  assert.equal(checkUrlTemplate("/" + "é".repeat(1024)).length > 0, true);
});

test("time override texts of 4.15", () => {
  for (const ok of ["", "now-6h", "now/d", "now-1d/d", "now-999999999y", "now-5M/w"]) assert.equal(checkTimeFrom(ok), "", ok);
  for (const bad of ["now", "6h", "now+1h", "now-0h", "now-6H", "now/", "now-1d/", "1731100000000", "now-1.5h", "now-6h/dd", "$x"]) {
    assert.equal(checkTimeFrom(bad), "A relative time starts with now-, as in now-6h, or with now/, as in now/d.", bad);
  }
  for (const ok of ["", "1d", "1w", "2M", "30m"]) assert.equal(checkTimeShift(ok), "", ok);
  for (const bad of ["-1d", "1d/d", "d", "0d", "1.5h", "1 d", "now-1d"]) assert.equal(checkTimeShift(bad), "A time shift has a count and a unit, as in 1d.", bad);
});

test("time texts of the dashboard and its order", () => {
  for (const ok of ["now", "now-6h", "now/d", "1731100000000", "0"]) assert.equal(checkTimeText(ok), "", ok);
  for (const bad of ["", "now+1h", "01731100000000", "4294967296000", "now-6H", "yesterday"]) assert.notEqual(checkTimeText(bad), "", bad);
  assert.equal(checkTimeOrder("1000", "2000"), "");
  assert.notEqual(checkTimeOrder("2000", "1000"), "");
  assert.notEqual(checkTimeOrder("2000", "2000"), "");
  assert.equal(checkTimeOrder("now-1h", "1000"), "");
});

test("zone names of 4.6", () => {
  for (const ok of ["Europe/Paris", "America/Argentina/Buenos_Aires", "Etc/GMT+5", "europe/paris", "Asia/Ho_Chi-Minh"]) assert.equal(checkZoneName(ok), "", ok);
  for (const bad of ["UTC", "GMT", "browser", "utc", "Europe/", "/Paris", "Europe/New York", "A/", "a/b/c/d", `A/${"x".repeat(63)}`, "1a/b"]) assert.equal(checkZoneName(bad), "The time zone is not valid.", bad);
  assert.equal(checkZoneName(`A/${"x".repeat(62)}`), "");
});

test("names, tags and texts", () => {
  assert.equal(checkVariableName("env"), "");
  assert.equal(checkVariableName("_a1"), "");
  for (const bad of ["", "1a", "a-b", "__x", "a".repeat(65), "a b"]) assert.notEqual(checkVariableName(bad), "", bad);
  assert.equal(checkVariableName("a".repeat(64)), "");
  assert.equal(checkVariableName("env", ["env"]), "This name exists.");
  assert.equal(checkTag("ch"), "");
  assert.equal(checkTag("ch", ["ch"]), "This tag exists.");
  assert.notEqual(checkTag(""), "");
  assert.notEqual(checkTag("x".repeat(65)), "");
  assert.equal(checkChars("\u{1F600}".repeat(256), 1, 256), "");
  assert.notEqual(checkChars("\u{1F600}".repeat(257), 1, 256), "");
  assert.equal(checkBytes("é".repeat(512), 1, 1024), "");
  assert.notEqual(checkBytes("é".repeat(513), 1, 1024), "");
  assert.equal(byteLength("é"), 2);
});

test("numbers of a form", () => {
  assert.equal(parseNumber(""), null);
  assert.equal(parseNumber(" 1.5 "), 1.5);
  assert.equal(parseNumber("-2e3"), -2000);
  assert.equal(parseNumber("abc"), undefined);
  assert.equal(parseNumber("1,5"), undefined);
  assert.equal(parseNumber("Infinity"), undefined);
  assert.equal(parseInteger("7", 0, 10), 7);
  assert.equal(parseInteger("11", 0, 10), undefined);
  assert.equal(parseInteger("1.5", 0, 10), undefined);
  assert.equal(parseInteger("", 0, 10), undefined);
});

test("lists: move, remove, replace and the limit", () => {
  assert.deepEqual(moveItem([1, 2, 3], 0, 1), [2, 1, 3]);
  assert.deepEqual(moveItem([1, 2, 3], 2, -1), [1, 3, 2]);
  assert.equal(moveItem([1, 2, 3], 0, -1), null);
  assert.equal(moveItem([1, 2, 3], 2, 1), null);
  const a = [1, 2, 3];
  assert.deepEqual(removeItem(a, 1), [1, 3]);
  assert.deepEqual(replaceItem(a, 0, 9), [9, 2, 3]);
  assert.deepEqual(a, [1, 2, 3]);
  assert.ok(atLimit(new Array(16).fill(0), LIMITS.links));
  assert.ok(!atLimit(new Array(15).fill(0), LIMITS.links));
  assert.deepEqual({ ...LIMITS }, { overrides: 512, mappings: 256, links: 16, transforms: 16, filters: 32, names: 64, reducers: 8, dashboardLinks: 16, linkTags: 8, variables: 64, annotations: 8, options: 10000, defaults: 10000, tags: 32 });
});

test("a new override, mapping, link and transform use the members of section 4 only", () => {
  const o = newOverride("a");
  assert.deepEqual(keys(o), ["match", "set"]);
  assert.deepEqual(o.match, { by: "name", value: "a" });
  assert.ok(keys(o.set).length >= 1);
  assert.deepEqual(matcherOf("names", o.match), { by: "names", names: ["a"], mode: "include" });
  assert.deepEqual(matcherOf("regex", { by: "names", names: ["x", "y"], mode: "exclude" }), { by: "regex", value: "x" });
  const order = { value: ["kind", "value", "text", "color"], range: ["kind", "from", "to", "text", "color"], regex: ["kind", "pattern", "text", "color"], special: ["kind", "match", "text", "color"] };
  for (const [kind] of MAPPING_KINDS) {
    const m = newMapping(kind);
    assert.deepEqual(keys(m), order[kind], kind);
    assert.ok(m.text !== null || m.color !== null);
  }
  assert.deepEqual(mappingOfKind("range", { kind: "value", value: "1", text: "T", color: "red" }), { kind: "range", from: 0, to: 100, text: "T", color: "red" });
  assert.deepEqual(keys(newLink()), ["title", "url", "target_blank"]);
  assert.equal(checkUrlTemplate(newLink().url), "");
  assert.deepEqual(keys(newTransform("filter_by_value")), ["kind", "mode", "match", "filters"]);
  assert.deepEqual(keys(newTransform("grouping_to_matrix")), ["kind", "column_field", "row_field", "value_field", "empty_value"]);
  const shapes = { equal: ["kind", "value"], not_equal: ["kind", "value"], greater: ["kind", "value"], greater_or_equal: ["kind", "value"], lower: ["kind", "value"], lower_or_equal: ["kind", "value"], range: ["kind", "from", "to"], regex: ["kind", "pattern"], is_null: ["kind"], is_not_null: ["kind"] };
  for (const [kind] of MATCHER_KINDS) assert.deepEqual(keys(newFilterMatcher(kind)), shapes[kind], kind);
  assert.deepEqual(newFilterMatcher("equal", { kind: "greater", value: 5 }), { kind: "equal", value: 5 });
  assert.deepEqual(newFilterMatcher("greater", { kind: "equal", value: "text" }), { kind: "greater", value: 0 });
});

test("a new dashboard link and a new variable", () => {
  assert.deepEqual(keys(newDashboardLink("link")), ["kind", "title", "url", "target_blank", "include_vars", "keep_time"]);
  assert.deepEqual(keys(newDashboardLink("dashboards")), ["kind", "title", "tags", "as_dropdown", "target_blank", "include_vars", "keep_time"]);
  assert.deepEqual(keys(newVariable("query", "a", "s")), ["kind", "name", "label", "hide", "source", "sql", "multi", "include_all", "all_value", "refresh", "sort", "regex", "default"]);
  assert.deepEqual(keys(newVariable("custom", "a")), ["kind", "name", "label", "hide", "options", "multi", "include_all", "all_value", "default"]);
  assert.deepEqual(keys(newVariable("text", "a")), ["kind", "name", "label", "hide", "value_type", "default"]);
  assert.equal(uniqueVariableName([{ name: "var1" }, { name: "var2" }]), "var3");
  assert.equal(uniqueVariableName([]), "var1");
  assert.equal(checkVariableName(uniqueVariableName([{ name: "var1" }])), "");
});

test("the query keeps the canonical order of 4.15", () => {
  const q = { source: "s", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null };
  const a = setQueryMember(setQueryMember(q, "time_shift", "1d"), "time_from", "now-24h");
  assert.deepEqual(keys(a), ["source", "format", "sql", "min_interval", "max_data_points", "time_from", "time_shift"]);
  const b = setQueryMember(a, "hide_time_override", true);
  assert.deepEqual(keys(b).slice(-3), ["time_from", "time_shift", "hide_time_override"]);
  const c = setQueryMember(setQueryMember(b, "time_from", undefined), "hide_time_override", undefined);
  assert.deepEqual(keys(c), ["source", "format", "sql", "min_interval", "max_data_points", "time_shift"]);
  assert.deepEqual(keys(q).length, 5);
});

test("the range of a preview uses the function of the page", () => {
  const range = { from_ms: 1000, to_ms: 2000 };
  const fn = (query, r, now, zone) => (query.time_from === "bad" ? { error: "invalid time override" } : { from: r.from - 1, to: r.to + 1, error: null, now, zone });
  assert.deepEqual(previewRange({}, range, { panelRange: fn }), { range });
  assert.deepEqual(previewRange({ time_from: "now-1h" }, range, { panelRange: null }), { range });
  assert.deepEqual(previewRange({ time_from: "now-1h" }, range, { panelRange: fn }), { range: { from_ms: 999, to_ms: 2001 } });
  // The texts of the dashboard range go to the function.
  let seen = null;
  previewRange({ time_from: "now-1h" }, { ...range, fromText: "now-6h", toText: "now" }, { panelRange: (q, r) => { seen = r; return { from: 1, to: 2, error: null }; } });
  assert.deepEqual(seen, { from: 1000, to: 2000, fromText: "now-6h", toText: "now" });
  assert.deepEqual(previewRange({ time_from: "bad" }, range, { panelRange: fn }), { error: "invalid time override" });
});

test("field name hints", () => {
  assert.deepEqual(fieldNameHints(null), []);
  const frame = { columns: [{ name: "ts", kind: "time", type: "DateTime" }, { name: "v", kind: "number", type: "Float64" }], values: [[1, 2], [3, 4]], rows: 2 };
  assert.deepEqual(fieldNameHints(frame), ["ts", "v"]);
});

test("variables: stale references after a rename, cycles, defaults", () => {
  const doc = { sections: [{ panels: [{ id: 1, title: "A", data: { query: { sql: ["SELECT $env"] } } }, { id: 2, title: "B", data: { reuse: { panel: 1 } } }, { id: 3, title: "C", data: { query: { sql: ["SELECT ${other}"] } } }] }] };
  assert.deepEqual(stalePanels(doc, "env"), [{ id: 1, title: "A" }]);
  assert.deepEqual(stalePanels(doc, "none"), []);
  const vars = [{ kind: "query", name: "a", sql: ["SELECT $b"] }, { kind: "query", name: "b", sql: ["SELECT $a"] }, { kind: "text", name: "c" }, { kind: "query", name: "d", sql: ["SELECT $c, $__from"] }];
  assert.equal(variableCycle(vars), "a");
  assert.equal(variableCycle(vars.slice(2)), null);
  assert.equal(variableCycle([{ kind: "query", name: "s", sql: ["SELECT $s"] }]), "s");
  assert.equal(variableCycle([{ kind: "query", name: "x", sql: ["SELECT $y"] }, { kind: "custom", name: "y", options: [] }]), null);
  assert.match(cycleError(vars), /bad_reference/);
  assert.equal(cycleError(vars.slice(2)), "");
  assert.deepEqual(staleVariables(vars, "a", "z"), ["b"]);
  assert.deepEqual([...duplicateValueIndexes([{ value: "1" }, { value: "2" }, { value: "1" }])], [2]);
});

test("the rules of the default values (4.11)", () => {
  const q = (o) => ({ kind: "query", multi: true, include_all: false, options: [], default: [], ...o });
  assert.equal(defaultErrors(q({ default: ["a", "b"] })), "");
  assert.notEqual(defaultErrors(q({ multi: false, default: ["a", "b"] })), "");
  assert.notEqual(defaultErrors(q({ default: ["$__all"] })), "");
  assert.equal(defaultErrors(q({ include_all: true, default: ["$__all"] })), "");
  assert.notEqual(defaultErrors(q({ include_all: true, default: ["$__all", "a"] })), "");
  const c = (o) => ({ kind: "custom", multi: true, include_all: false, options: [{ text: "A", value: "a" }], default: [], ...o });
  assert.equal(defaultErrors(c({ default: ["a"] })), "");
  assert.notEqual(defaultErrors(c({ default: ["z"] })), "");
});

test("the scope runs a waiting change at once, or drops it", async () => {
  const scope = createScope();
  const seen = [];
  const run = scope.debounce((x) => seen.push(x), 20);
  run("a");
  run("b");
  assert.deepEqual(seen, []);
  scope.flush();
  assert.deepEqual(seen, ["b"]);
  run("c");
  scope.cancel();
  await new Promise((r) => setTimeout(r, 40));
  assert.deepEqual(seen, ["b"]);
  run("d");
  await new Promise((r) => setTimeout(r, 40));
  assert.deepEqual(seen, ["b", "d"]);
  scope.flush();
  assert.deepEqual(seen, ["b", "d"]);
});

test("the preview range with the real range function of the page (9.8b)", () => {
  const now = Date.UTC(2026, 10, 11, 12);
  const rel = { from_ms: now - 6 * 3600000, to_ms: now, fromText: "now-6h", toText: "now" };
  const a = previewRange({ time_from: "now-24h" }, rel, { panelRange, now, zone: "utc" });
  assert.deepEqual(a, { range: { from_ms: now - 24 * 3600000, to_ms: now } });
  // Two absolute ends: `time_from` does nothing.
  const abs = { from_ms: 1700000000000, to_ms: 1700000005000, fromText: "1700000000000", toText: "1700000005000" };
  assert.deepEqual(previewRange({ time_from: "now-24h" }, abs, { panelRange, now, zone: "utc" }), { range: { from_ms: 1700000000000, to_ms: 1700000005000 } });
  // A shift applies to an absolute range.
  assert.deepEqual(previewRange({ time_shift: "1d" }, abs, { panelRange, now, zone: "utc" }), { range: { from_ms: 1700000000000 - 86400000, to_ms: 1700000005000 - 86400000 } });
});

// --- the group "Repeat options" and the repeat of a section (4.13a, 10.3, 10.3b, 10.6) ---------------------------------------------------

const rp = (id, grid, extra = {}) => ({ id, type: "table", title: `p${id}`, description: "", transparent: false, grid, data: { query: { source: "s", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null } }, transforms: [], fields: { defaults: {}, overrides: [] }, options: {}, links: [], ...extra });
const rv = (name, kind, multi) => (kind === "custom" ? { kind, name, label: null, hide: "none", options: [{ text: "A", value: "a" }], multi, include_all: false, all_value: null, default: ["a"] } : kind === "text" ? { kind, name, label: null, hide: "none", value_type: "text", default: "" } : { kind, name, label: null, hide: "none", source: "s", sql: ["SELECT 1"], multi, include_all: false, all_value: null, refresh: "load", sort: "none", regex: null, default: [] });
const repeatDoc = () => ({
  variables: [rv("host", "custom", true), rv("one", "custom", false), rv("txt", "text", false), rv("reg", "query", true)],
  sections: [
    { title: null, collapsed: false, panels: [rp(1, { x: 0, y: 0, w: 12, h: 4 }), rp(2, { x: 12, y: 0, w: 12, h: 4 }), rp(3, { x: 0, y: 4, w: 24, h: 2 }, { data: { reuse: { panel: 1, with_transforms: true } } })] },
    { title: "Hosts", collapsed: false, panels: [rp(4, { x: 0, y: 0, w: 24, h: 2 })] },
  ],
});

test("the variables of the group are the query and custom variables with multi", () => {
  assert.deepEqual(repeatableVariables(repeatDoc()).map((v) => v.name), ["host", "reg"]);
});

test("a panel that reuses, or is the source of a reuse, cannot repeat (4.13a, rules 4 and 8)", () => {
  const doc = repeatDoc();
  assert.match(panelRepeatRefusal(doc, 3), /reuses/);
  assert.match(panelRepeatRefusal(doc, 1), /source of a reuse/);
  assert.equal(panelRepeatRefusal(doc, 2), "");
  assert.equal(canBeReuseSource(doc, doc.sections[0].panels[0]), true);
  const repeated = setPanelRepeat(doc, 2, { variable: "host", direction: "v" });
  assert.equal(canBeReuseSource(repeated, repeated.sections[0].panels[1]), false);
  assert.match(panelRepeatRefusal(setSectionRepeat(doc, 1, "host"), 4), /section repeats/);
});

test("the direction h writes x = 0 and w = 24 and gives the panel its own rows; v removes max_per_row", () => {
  const doc = repeatDoc();
  const h = setPanelRepeat(doc, 2, { variable: "host", direction: "h", max_per_row: 3 });
  const panel = h.sections[0].panels.find((p) => p.id === 2);
  assert.deepEqual([panel.grid.x, panel.grid.w], [0, 24]);
  assert.deepEqual(panel.repeat, { variable: "host", direction: "h", max_per_row: 3 });
  assert.deepEqual(Object.keys(panel).slice(-1), ["repeat"], "repeat is the last member");
  // No other panel shares a row unit with the band (4.13a, rule 5).
  for (const o of h.sections[0].panels) if (o.id !== 2) assert.ok(o.grid.y >= panel.grid.y + panel.grid.h || o.grid.y + o.grid.h <= panel.grid.y, `panel ${o.id}`);
  const v = setPanelRepeat(h, 2, { variable: "host", direction: "v", max_per_row: 3 });
  assert.deepEqual(v.sections[0].panels.find((p) => p.id === 2).repeat, { variable: "host", direction: "v" });
  const plain = setPanelRepeat(h, 2, { variable: "host", direction: "h" });
  assert.equal("max_per_row" in plain.sections[0].panels.find((p) => p.id === 2).repeat, false, "the default 4 is never written");
  assert.equal("repeat" in setPanelRepeat(h, 2, null).sections[0].panels.find((p) => p.id === 2), false);
  assert.equal(doc.sections[0].panels[1].repeat, undefined, "the input is not changed");
});

test("a section can repeat when it has a title, no placement and no panel that repeats or reuses (4.13a, rule 7)", () => {
  const doc = repeatDoc();
  assert.match(sectionRepeatRefusal(doc, 0), /first section/);
  assert.match(sectionRepeatRefusal({ ...doc, sections: [doc.sections[0], { ...doc.sections[0], title: "T" }] }, 1), /reuses/);
  assert.equal(sectionRepeatRefusal(doc, 1), "");
  const placed = { ...doc, variables: [{ ...doc.variables[0], placement: { section: 1, x: 0, y: 2, w: 6, h: 2 } }, ...doc.variables.slice(1)] };
  assert.match(sectionRepeatRefusal(placed, 1), /placed/);
  assert.match(sectionRepeatRefusal(setPanelRepeat(doc, 2, { variable: "host", direction: "v" }), 0), /first section|repeats/);
  const s = setSectionRepeat(doc, 1, "host");
  assert.deepEqual(s.sections[1].repeat, { variable: "host" });
  assert.deepEqual(Object.keys(s.sections[1]).slice(-1), ["repeat"]);
  assert.equal("repeat" in setSectionRepeat(s, 1, null).sections[1], false);
  assert.equal(lastPlainSection(s), 0);
  assert.equal(lastPlainSection(doc), 1);
});

test("a rename rewrites the repeats; a delete removes them (4.13a, rule 9)", () => {
  const doc = setSectionRepeat(setPanelRepeat(repeatDoc(), 2, { variable: "host", direction: "v" }), 1, "host");
  assert.equal(repeatUsers(doc, "host").panels.length, 1);
  assert.equal(repeatUsers(doc, "host").sections.length, 1);
  const renamed = renameRepeatVariable(doc, "host", "node");
  assert.equal(renamed.sections[0].panels[1].repeat.variable, "node");
  assert.equal(renamed.sections[1].repeat.variable, "node");
  assert.equal(renameRepeatVariable(doc, "reg", "zone"), doc);
  const gone = removeRepeatsOf(doc, "host");
  assert.equal(gone.sections[0].panels[1].repeat, undefined);
  assert.equal(gone.sections[1].repeat, undefined);
  assert.equal(removeRepeatsOf(doc, "reg"), doc);
});

// --- the group "Text" of 10.3 (4.33a, 9.19b): the checks of the content ---------------------------------------------------------------
import { countText, firstTextError, linesOf, textOf } from "../../web/js/editor/text_check.js";
import { MD_MESSAGES } from "../../web/js/markdown.js";
import { TYPE_INFO } from "../../web/js/editor/form.js";

test("the text of the field splits at each line end into the lines of the content (10.3)", () => {
  assert.deepEqual(linesOf(""), []);
  assert.deepEqual(linesOf("a"), ["a"]);
  assert.deepEqual(linesOf("a\r\nb\rc\nd"), ["a", "b", "c", "d"]);
  assert.deepEqual(linesOf("a\n"), ["a", ""]);
  assert.equal(textOf(linesOf("a\r\nb\n\nc")), "a\nb\n\nc");
});

test("the count line: the bytes of the joined text and the lines (10.3)", () => {
  assert.equal(countText([]), "0 of 65,536 bytes, 0 of 2,000 lines");
  assert.equal(countText(["ab", "cé"]), "6 of 65,536 bytes, 2 of 2,000 lines");
});

test("the first error of a text follows the order of 10.3", () => {
  assert.equal(firstTextError(["ok", "line two"]), "");
  assert.equal(firstTextError(["a\tb"]), "", "a tab is allowed");
  assert.equal(firstTextError(["a", "b\u0000"]), "Line 2 has a control character.");
  assert.equal(firstTextError(["x\u007f"]), "Line 1 has a control character.");
  assert.equal(firstTextError(["x\u0085"]), "Line 1 has a control character.");
  assert.equal(firstTextError(["a".repeat(8193)]), "Line 1 has more than 8,192 bytes.");
  assert.equal(firstTextError(["é".repeat(4097)]), "Line 1 has more than 8,192 bytes.");
  assert.equal(firstTextError(["é".repeat(4096)]), "");
  assert.equal(firstTextError(Array.from({ length: 2001 }, () => "a")), MD_MESSAGES.size);
  assert.equal(firstTextError(["> > > > > > > a"]), MD_MESSAGES.depth);
  assert.equal(firstTextError(["*a **b *c **d *e **f *g* f** e* d** c* b** a*"]), MD_MESSAGES.inline);
  // A control character comes before a long line and before a limit.
  assert.equal(firstTextError(["a".repeat(9000), "x\u0001"]), "Line 2 has a control character.");
});

test("the type list has the 12 types and the type text has a name and a text", () => {
  assert.deepEqual(TYPE_INFO.text, ["Text", "Markdown text, no query"]);
  assert.deepEqual(TYPE_INFO.state_timeline, ["State timeline", "States of each row over time"]);
  assert.deepEqual(TYPE_INFO.status_history, ["Status history", "One cell for each sample"]);
  assert.deepEqual(TYPE_INFO.heatmap, ["Heatmap", "Counts in a grid of time and value"]);
  assert.deepEqual(TYPE_INFO.histogram, ["Histogram", "Counts of values in buckets"]);
  assert.deepEqual(TYPE_INFO.gauge, ["Gauge", "Values on a round scale"]);
  assert.deepEqual(TYPE_INFO.logs, ["Logs", "Lines of text with time and level"]);
  assert.deepEqual(TYPE_INFO.geomap, ["Geomap", "Markers on a map"]);
  assert.equal(Object.keys(TYPE_INFO).length, 15);
});

// --- the tab "Annotations" (10.3b, 4.5b) ---------------------------------------------------------------------------------------

test("a new annotation has the colour red, is enabled and draws on all the time series; the name is unique", () => {
  const a = newAnnotation("Annotation 1", "prod");
  assert.deepEqual(keys(a), ["name", "source", "sql", "color", "enabled"]);
  assert.equal(a.color, "red");
  assert.equal(a.enabled, true);
  assert.ok(!("panels" in a));
  assert.equal(uniqueAnnotationName([]), "Annotation 1");
  assert.equal(uniqueAnnotationName([{ name: "Annotation 1" }, { name: "Annotation 3" }]), "Annotation 2");
  assert.equal(LIMITS.annotations, 8);
});

test("the editor offers the ClickHouse, PostgreSQL and MongoDB sources, and no other kind (ID-K142, ID-K333)", () => {
  const list = [{ id: "a", kind: "clickhouse" }, { id: "b", kind: "postgres" }, { id: "c", kind: "mongodb" }, { id: "d", kind: "clickhouse" }, { id: "e", kind: "prometheus" }, { id: "f" }];
  assert.deepEqual(annotationSources(list).map((s) => s.id), ["a", "b", "c", "d"]);
});

test("the member annotations goes after variables, and an empty array leaves it out (4.38)", () => {
  const doc = { format_version: 3, title: "T", variables: [], sections: [] };
  assert.deepEqual(keys(withAnnotations(doc, [{ name: "A" }])), ["format_version", "title", "variables", "annotations", "sections"]);
  assert.deepEqual(keys(withAnnotations({ ...doc, annotations: [{ name: "A" }] }, [])), ["format_version", "title", "variables", "sections"]);
  assert.deepEqual(keys(withAnnotations({ title: "T", annotations: [1], sections: [] }, [{ name: "A" }])), ["title", "sections", "annotations"]);
});

test("a rename lists the annotations whose SQL still names the old name", () => {
  const doc = { annotations: [{ name: "A", sql: ["SELECT 1 WHERE e = '$env'"] }, { name: "B", sql: ["SELECT ${other}"] }] };
  assert.deepEqual(staleAnnotations(doc, "env"), ["A"]);
  assert.deepEqual(staleAnnotations({ sql: [] }, "env"), []);
});

test("the faults of an annotation before the save: name, source, interval macros and panels (4.5b, 7.18)", () => {
  const doc = {
    annotations: [
      { name: "A", source: "prod", sql: ["SELECT 1", "WHERE $__timeFilter(t) AND x = $__interval_ms"], color: "red", panels: [1, 9] },
      { name: "A", source: "gone", sql: ["SELECT $__timeInterval(t)"], color: "red" },
    ],
    sections: [{ panels: [{ id: 1 }] }],
  };
  const sources = [{ id: "prod" }];
  assert.deepEqual(annotationFaults(doc, 0, sources).map((f) => [f.code, f.pointer]), [["not_unique", "/annotations/0/name"], ["not_permitted", "/annotations/0/sql/1"], ["bad_reference", "/annotations/0/panels/1"]]);
  assert.deepEqual(annotationFaults(doc, 1, sources).map((f) => [f.code, f.pointer]), [["not_unique", "/annotations/1/name"], ["bad_reference", "/annotations/1/source"], ["not_permitted", "/annotations/1/sql/0"]]);
  assert.deepEqual(annotationFaults({ ...doc, annotations: [{ ...doc.annotations[0], name: "Z", panels: [1], sql: ["SELECT 1"] }] }, 0, sources), []);
});
