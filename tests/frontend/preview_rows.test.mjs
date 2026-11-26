// SPDX-License-Identifier: MIT OR Apache-2.0
// The loading previews of the family "rows" (9.11): the table (web/js/preview/table.js) and the logs panel (web/js/preview/logs.js). One test for
// each rule of the council files table.md and logs.md, with the JSON member that decides it, the neutral form where the document is silent, and the bounds.
// The pointer, the scroll bar and the tokens are an environment that the page measures: the tests give them (`ctx.env`).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { boundHolds, setMeasurer } from "../../web/js/preview/common.js";
import { previewOf } from "../../web/js/preview/index.js";
import * as table from "../../web/js/preview/table.js";
import * as logs from "../../web/js/preview/logs.js";

// A measurer of fixed width: 7 px for each character, whatever the font.
setMeasurer((text) => text.length * 7);

const read = (path) => readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");
const FINE = { coarse: false, sb: 10, head: 34, row: 36, pad: 6, filterW: 16, filterH: 22, framesH: 32, fsMd: 14, fsXs: 12, fwMedium: 500, footLs: 0 };
const COARSE = { ...FINE, coarse: true, filterW: 40, filterH: 40, framesH: 48 };
const OVERLAY = { ...FINE, sb: 0 };
const BOX = { w: 700, h: 300 };
const query = (over = {}) => ({ source: "s", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null, ...over });
const tablePanel = (over = {}) => ({ id: 1, type: "table", title: "t", data: { query: query() }, transforms: [], fields: { defaults: {}, overrides: [] }, options: { show_header: true, sort: [] }, ...over });
const run = (panel, env = FINE, box = BOX, doc = null) => table.build(panel, box, doc, { env });
const parts = (out) => {
  const found = [];
  const walk = (n) => {
    if (n.a && n.a["data-skel-part"]) found.push(n);
    for (const k of n.k || []) walk(k);
  };
  for (const n of out.elements) walk(n);
  return found;
};
const part = (out, name, i = 0) => parts(out).filter((n) => n.a["data-skel-part"] === name)[i];
const cls = (n) => n.a["data-skel-class"];
const num = (n, name) => Number.parseFloat(n.v[name]);
const widthOf = (n) => n.v;

// ---- R1: the inputs ------------------------------------------------------------------------------------------------------------------------

test("R1: the table preview never reads a query: the tree is the same when the text of a query changes", () => {
  const a = tablePanel({ data: { query: query({ sql: ["SELECT a FROM t"] }), extra_queries: [{ ref: "B", source: "s", format: "table", sql: ["SELECT 1"] }] } });
  const b = tablePanel({ data: { query: query({ sql: ["SELECT completely, different FROM other WHERE x = 1"] }), extra_queries: [{ ref: "B", source: "s", format: "table", sql: ["SELECT 2 AS n"] }] } });
  assert.deepEqual(run(a), run(b));
  const c = logsPanel();
  const d = logsPanel({ data: { query: query({ format: "logs", sql: ["SELECT other"] }) } });
  assert.deepEqual(logs.build(c, BOX, null, { env: { sb: 10 } }), logs.build(d, BOX, null, { env: { sb: 10 } }));
});

test("R1: no rows family module reads the text of a query, a regular expression of the page or a typical value", () => {
  for (const name of ["table", "logs"]) {
    const js = read(`web/js/preview/${name}.js`);
    assert.ok(!/\.queries\b|\.sql\b|\.expr\b|rawSql|query_text|rangeOf/.test(js.replace(/\/\/.*$/gm, "")), `${name} reads a query`);
  }
});

test("a panel with no environment (no page, no test value) gives the neutral form", () => {
  const out = table.build(tablePanel(), BOX, null, {});
  assert.equal(out.notes[0], "no environment");
  assert.deepEqual(out.elements, []);
  assert.equal(logs.build(logsPanel(), BOX, null, {}).notes[0], "no environment");
  // The registry catches a throw and gives the neutral form.
  assert.equal(previewOf(tablePanel(), null, null, { env: FINE }).notes[0], "no environment");
});

// ---- R2: the view and the rows around it -----------------------------------------------------------------------------------------------

test("R2: the view is the plot of the preview, exact when the number of frames is known", () => {
  const out = run(tablePanel());
  assert.equal(cls(part(out, "plot")), "exact");
  assert.equal(out.elements.at(-1).c, "skel__tview");
  assert.equal(out.bounds.view.hi, 300);
  assert.equal(out.bounds.view.lo, 300);
});

test("R2: a view with an unknown number of frames is a most bound: a select bar may take room from it", () => {
  const out = run(tablePanel({ transforms: [{ kind: "partition_by_values", fields: ["a"] }] }));
  assert.equal(cls(part(out, "plot")), "most");
  assert.equal(out.bounds.view.lo, -Infinity);
  assert.ok(out.notes.includes("frames unknown"));
});

// ---- R3: the warning line ----------------------------------------------------------------------------------------------------------------

test("R3: an override pattern that the page cannot compile gives the same warning line, once for each value, outside the view", () => {
  const panel = tablePanel({ fields: { defaults: {}, overrides: [{ match: { by: "regex", value: "(" }, set: {} }, { match: { by: "regex", value: "(" }, set: {} }, { match: { by: "regex", value: "/[/" }, set: {} }, { match: { by: "regex", value: "^ok$" }, set: {} }] } });
  const out = run(panel);
  const warning = out.elements[0];
  assert.equal(warning.t, "p");
  assert.match(warning.c, /panel__warning/);
  assert.equal(cls(part(out, "warning")), "exact");
  assert.equal(warning.k[1].x, " The override pattern ( is not a valid regular expression. The override pattern /[/ is not a valid regular expression.");
  assert.equal(out.elements[1].c, "skel__tview", "the view comes after the line");
  assert.equal(parts(run(tablePanel({ fields: { defaults: {}, overrides: [{ match: { by: "regex", value: "^ok$" }, set: {} }] } }))).filter((n) => n.a["data-skel-part"] === "warning").length, 0);
});

test("R3: the view height is data with a warning line: its text wraps", () => {
  const out = run(tablePanel({ fields: { defaults: {}, overrides: [{ match: { by: "regex", value: "(" }, set: {} }] } }));
  assert.equal(out.bounds.view.lo, -Infinity);
  assert.equal(out.bounds.noOverflow, false);
});

// ---- R4: the select bar ----------------------------------------------------------------------------------------------------------------

const two = (over = {}) => tablePanel({ data: { query: query(), extra_queries: [{ ref: "B", source: "s", format: "table", sql: ["SELECT 2"] }] }, ...over });

test("R4: two visible queries give the select bar with the refs in order, and the view loses its height", () => {
  const out = run(two());
  const bar = part(out, "frames");
  assert.equal(bar.t, "label");
  assert.match(bar.c, /panel__frames/);
  assert.equal(cls(bar), "exact");
  const select = part(out, "select");
  assert.deepEqual(select.k.map((o) => o.x), ["A", "B"]);
  assert.equal(select.a.inert, "");
  assert.equal(bar.k[0].x, "Query");
  assert.equal(out.bounds.view.hi, 300 - 32);
  assert.equal(out.bounds.view.lo, 300 - 32);
});

test("R4: a hidden query leaves the bar out, and the ref of the first query is A unless the document says another", () => {
  assert.equal(part(run(two({ data: { query: query(), extra_queries: [{ ref: "B", source: "s", format: "table", sql: ["SELECT 2"], hide: true }] } })), "frames"), undefined);
  const out = run(tablePanel({ data: { query: query({ ref: "Q" }), extra_queries: [{ ref: "R", source: "s", format: "table", sql: ["SELECT 2"] }, { ref: "S", source: "s", format: "table", sql: ["x"], hide: true }] } }));
  assert.deepEqual(part(out, "select").k.map((o) => o.x), ["Q", "R"]);
});

test("R4: the transforms decide the number of frames: join_by_field gives one, merge keeps one and makes two unknown, partition is unknown", () => {
  assert.equal(part(run(two({ transforms: [{ kind: "join_by_field", mode: "outer", field: null }] })), "frames"), undefined);
  assert.equal(part(run(two({ transforms: [{ kind: "join_by_field", mode: "outer", field: null }] })), "plot").a["data-skel-class"], "exact", "join gives one frame, so the number is known");
  const merged = run(two({ transforms: [{ kind: "merge" }] }));
  assert.equal(part(merged, "frames"), undefined);
  assert.equal(cls(part(merged, "plot")), "most", "merge of two frames: the number of frames is unknown");
  assert.equal(cls(part(run(tablePanel({ transforms: [{ kind: "merge" }] })), "plot")), "exact", "merge of one frame keeps one frame");
  assert.equal(cls(part(run(two({ transforms: [{ kind: "organize" }, { kind: "sort_by", sort: [] }, { kind: "limit", count: 5, end: "first" }] })), "frames")), "exact", "the other kinds keep the frames");
  assert.equal(cls(part(run(two({ transforms: [{ kind: "partition_by_values", fields: ["a"] }] })), "plot")), "most");
});

test("R4: a reuse panel reads the queries and the transforms of its source in the document", () => {
  const source = tablePanel({ id: 7, data: { query: query(), extra_queries: [{ ref: "B", source: "s", format: "table", sql: ["SELECT 2"] }] }, transforms: [{ kind: "join_by_field", mode: "outer", field: null }] });
  const reuse = (withTransforms) => tablePanel({ id: 8, data: { reuse: { panel: 7, with_transforms: withTransforms } } });
  const doc = { sections: [{ panels: [source, reuse(false)] }] };
  assert.deepEqual(part(run(reuse(false), FINE, BOX, doc), "select").k.map((o) => o.x), ["A", "B"], "the source transforms do not apply");
  assert.equal(part(run(reuse(true), FINE, BOX, doc), "select"), undefined, "the source transforms apply: one frame");
  const unseen = run(reuse(false), FINE, BOX, { sections: [] });
  assert.equal(cls(part(unseen, "plot")), "most", "the source is not in the document: the frames are unknown");
  assert.ok(unseen.notes.includes("frames unknown"));
  assert.equal(table.build(tablePanel({ data: { reuse: { panel: 9, with_transforms: false } } }), BOX, { sections: [{ panels: [tablePanel({ id: 9, data: { query: query({ hide: true }) } })] }] }, { env: FINE }).notes[0], "no visible query");
});

// ---- R5: C1, at least one shown column ---------------------------------------------------------------------------------------------------

test("R5: a hidden member leaves the view empty: no shape without a shown column", () => {
  for (const fields of [{ defaults: { hidden: true }, overrides: [] }, { defaults: {}, overrides: [{ match: { by: "name", value: "a" }, set: { hidden: true } }] }]) {
    const out = run(tablePanel({ fields }));
    assert.deepEqual(parts(out).map((n) => n.a["data-skel-part"]), ["plot"]);
    assert.ok(out.notes.includes("no shown column is certain"));
  }
  assert.ok(part(run(tablePanel({ fields: { defaults: {}, overrides: [{ match: { by: "name", value: "a" }, set: { hidden: false } }] } })), "head"), "hidden false is a shown column");
});

// ---- R6: the header ----------------------------------------------------------------------------------------------------------------------

test("R6: the header is 34 px on a fine pointer, with its rule on the last pixel row, and 0 with show_header false", () => {
  const out = run(tablePanel());
  const head = part(out, "head");
  assert.equal(num(head, "--h"), 34);
  assert.equal(cls(head), "extent");
  assert.equal(head.k[0].v["--y"], "33px");
  assert.equal(out.bounds.H.value, 34);
  const none = run(tablePanel({ options: { show_header: false } }));
  assert.equal(part(none, "head"), undefined);
  assert.equal(num(part(none, "row"), "--y"), 0);
});

test("R6: a coarse pointer gives a header of 47 px only when every shown column has a filter button", () => {
  const withFilter = (defaults, overrides = []) => tablePanel({ fields: { defaults, overrides } });
  assert.equal(num(part(run(withFilter({ filterable: true }), COARSE), "head"), "--h"), 47, "defaults true");
  assert.equal(num(part(run(withFilter({}), COARSE), "head"), "--h"), 34, "no member sets it");
  assert.equal(num(part(run(withFilter({ filterable: false }, [{ match: { by: "name", value: "a" }, set: { filterable: false } }]), COARSE), "head"), "--h"), 34);
  for (const fields of [{ defaults: { filterable: true }, overrides: [{ match: { by: "name", value: "a" }, set: { filterable: false } }] }, { defaults: {}, overrides: [{ match: { by: "name", value: "a" }, set: { filterable: true } }] }]) {
    const out = run(tablePanel({ fields }), COARSE);
    assert.deepEqual(parts(out).map((n) => n.a["data-skel-part"]), ["plot"], "the header height is data: no shape under the top of the view");
    assert.ok(out.notes.includes("header height is data"));
  }
  assert.equal(num(part(run(withFilter({ filterable: true }), FINE), "head"), "--h"), 34, "a fine pointer: the filter button is 22 px");
});

// ---- R7: the extent ----------------------------------------------------------------------------------------------------------------------

test("R7: with no width member the extent is the view less the scroll bar, and the whole view when no overflow is proven or the bar has no width", () => {
  const out = run(tablePanel());
  assert.equal(out.bounds.extentLeast.value, 690);
  assert.equal(part(out, "head").v["--sb"], "10px");
  assert.equal(part(out, "head").v["--m"], undefined);
  assert.equal(part(run(tablePanel(), OVERLAY), "head").v["--sb"], "0px", "overlay bars");
  const limited = run(tablePanel({ transforms: [{ kind: "limit", count: 2, end: "first" }] }));
  assert.equal(part(limited, "head").v["--sb"], "0px", "no vertical overflow: 34 + 72 <= 290");
  assert.equal(limited.bounds.noOverflow, true);
  assert.equal(run(tablePanel({ transforms: [{ kind: "limit", count: 2, end: "first" }], options: { wrap: true } })).bounds.noOverflow, false, "a wrapped row can be higher");
  assert.equal(run(tablePanel({ transforms: [{ kind: "limit", count: 8, end: "first" }] })).bounds.noOverflow, false, "34 + 288 > 290");
});

test("R7: with a width member the extent is the least width of column 0, cut by the view", () => {
  const w = (defaults, overrides = []) => run(tablePanel({ fields: { defaults, overrides } }));
  assert.equal(part(w({ width: 120 }), "head").v["--m"], "120px");
  assert.equal(part(w({}, [{ match: { by: "name", value: "a" }, set: { width: 90 } }]), "head").v["--m"], "90px", "the 150 px floor of an automatic column");
  assert.equal(part(w({}, [{ match: { by: "name", value: "a" }, set: { width: 400 } }]), "head").v["--m"], "150px", "an automatic column is 150 px at least");
  assert.equal(part(w({ width: 200 }, [{ match: { by: "name", value: "a" }, set: { width: 80 } }]), "head").v["--m"], "80px", "the least of all");
  assert.equal(w({ width: 900 }).bounds.extentLeast.value, 690, "cut by the view");
  assert.equal(w({ width: 120 }).bounds.extentLeast.value, 120);
  assert.ok(boundHolds(w({ width: 120 }).bounds.extentLeast, 960), "a table of 8 columns of 120 px");
  assert.ok(!boundHolds(w({ width: 120 }).bounds.extentLeast, 100));
});

// ---- R8: the slot of the first row ---------------------------------------------------------------------------------------------------------

test("R8: slot 0 is 36 px under the header, with its rule on the last pixel row, and only when wrap is not true", () => {
  const out = run(tablePanel());
  const slot = part(out, "row");
  assert.equal(num(slot, "--y"), 34);
  assert.equal(num(slot, "--h"), 36);
  assert.equal(slot.k[0].v["--y"], "35px");
  assert.equal(cls(slot), "extent");
  assert.equal(out.bounds.slotTop.value, 34);
  const wrapped = run(tablePanel({ options: { wrap: true } }));
  assert.equal(part(wrapped, "row"), undefined);
  assert.ok(wrapped.notes.includes("wrap: no slot"));
  assert.ok(part(wrapped, "head"), "the header stays");
  assert.equal(parts(out).filter((n) => n.a["data-skel-part"] === "row").length, 1, "one slot: the count of rows is data");
});

// ---- R9: the rows ---------------------------------------------------------------------------------------------------------------------------

test("R9: limit 0 is no data and gives the neutral form; limit 0 then reduce_fields gives one row; limit 0 then series_to_rows is unknown", () => {
  const lim = (count) => ({ kind: "limit", count, end: "first" });
  assert.equal(run(tablePanel({ transforms: [lim(0)] })).notes[0], "no data proven");
  assert.deepEqual(run(tablePanel({ transforms: [lim(0)] })).elements, []);
  const reduced = run(tablePanel({ transforms: [lim(0), { kind: "reduce", mode: "reduce_fields", reducers: ["max"] }] }));
  assert.ok(part(reduced, "row"), "reduce_fields gives one row from zero rows");
  const rows = run(tablePanel({ transforms: [lim(0), { kind: "reduce", mode: "series_to_rows", reducers: ["max"] }] }));
  assert.ok(part(rows, "row"), "series_to_rows: unknown, so the generic form");
  assert.equal(table.walkChain(tablePanel({ transforms: [lim(5), lim(3)] }), table.readPanel(tablePanel({ transforms: [lim(5), lim(3)] })), null).B, 3);
});

test("R9: the bound of the rows follows merge and join_by_field", () => {
  const walk = (transforms, extra = false) => {
    const p = extra ? two({ transforms }) : tablePanel({ transforms });
    return table.walkChain(p, table.readPanel(p), null);
  };
  const lim = { kind: "limit", count: 3, end: "first" };
  assert.deepEqual([walk([lim, { kind: "merge" }]).F, walk([lim, { kind: "merge" }]).B], [1, 3]);
  assert.deepEqual([walk([lim, { kind: "merge" }], true).F, walk([lim, { kind: "merge" }], true).B], [null, 6]);
  assert.equal(walk([lim, { kind: "join_by_field", mode: "inner", field: null }], true).B, 9, "equal keys multiply");
  assert.equal(walk([lim, { kind: "join_by_field", mode: "outer", field: null }], true).B, 6);
  assert.equal(walk([{ kind: "merge" }], true).B, null);
  assert.equal(walk([lim, { kind: "reduce", mode: "reduce_fields", reducers: ["max"] }]).B, 1);
  assert.equal(walk([lim, { kind: "reduce", mode: "series_to_rows", reducers: ["max"] }]).B, null);
  assert.equal(walk([lim, { kind: "organize" }, { kind: "filter_by_value" }, { kind: "sort_by", sort: [] }]).B, 3, "the other kinds keep the bound");
});

// ---- R10 and R11: the footer ------------------------------------------------------------------------------------------------------------------

const foot = (reducers, extra = {}) => tablePanel({ fields: { defaults: { footer: { reducers }, ...extra }, overrides: [] } });
const one = { kind: "limit", count: 1, end: "first" };

test("R10: case 1: one row of 36 px and a footer that fits sit under the row: the height is 13 + 22 L", () => {
  for (const [reducers, h] of [[["total"], 35], [["min", "max"], 57], [["min", "max", "mean"], 79]]) {
    const out = run(tablePanel({ ...foot(reducers), transforms: [one] }));
    const band = part(out, "foot");
    assert.equal(num(band, "--y"), 70, `${reducers.length} lines`);
    assert.equal(num(band, "--h"), h);
    assert.equal(cls(band), "extent");
    assert.equal(band.c.includes("skel__tfoot--b"), false);
    assert.equal(out.bounds.footTop.value, 70);
  }
  const wrapped = run(tablePanel({ ...foot(["total"]), transforms: [one], options: { wrap: true } }));
  assert.equal(part(wrapped, "foot"), undefined, "a wrapped row is not 36 px: the footer place is data");
  assert.ok(wrapped.notes.includes("footer place is data"));
});

test("R10: case 2: when the content is higher than the view the sticky footer is at the bottom, if the horizontal bar is known", () => {
  const low = { w: 700, h: 100 };
  // A bar that the width proves: m = 800 > 700 - 10.
  const out = run(tablePanel(foot(["total"], { width: 800 })), FINE, low);
  const band = part(out, "foot");
  assert.ok(band.c.includes("skel__tfoot--b"));
  assert.equal(band.v["--b"], "10px", "the bar of the width lifts the footer");
  assert.equal(out.bounds.footBottom.value, 10);
  assert.equal(part(run(tablePanel(foot(["total"])), FINE, low), "foot"), undefined, "classic bars and a bar that nothing proves: data");
  assert.equal(part(run(tablePanel(foot(["total"])), OVERLAY, low), "foot").v["--b"], "0px", "overlay bars: no bar to lift the footer");
  assert.equal(part(run(tablePanel(foot(["total"], { width: 800 })), OVERLAY, low), "foot").v["--b"], "0px");
  // The proof of the overflow needs 34 + 36 + 35 > the view.
  assert.equal(part(run(tablePanel(foot(["total"])), OVERLAY, { w: 700, h: 105 }), "foot"), undefined, "105 is not higher than 105");
  assert.ok(part(run(tablePanel(foot(["total"])), OVERLAY, { w: 700, h: 104 }), "foot"));
  assert.equal(part(run(tablePanel({ ...foot(["total"]), options: { wrap: true } }), OVERLAY, low), "foot").v["--b"], "0px", "wrap does not matter: rows are 36 px or more");
});

test("R10: the footer is data when its presence or its height is not certain, or when a warning or an unknown frame count moves the view", () => {
  const cases = {
    "an override only": tablePanel({ fields: { defaults: {}, overrides: [{ match: { by: "name", value: "a" }, set: { footer: { reducers: ["total"] } } }] }, transforms: [one] }),
    "two lengths": tablePanel({ fields: { defaults: { footer: { reducers: ["min", "max"] } }, overrides: [{ match: { by: "name", value: "a" }, set: { footer: { reducers: ["total"] } } }] }, transforms: [one] }),
    "the rows are data": foot(["total"]),
    "a warning": tablePanel({ fields: { defaults: { footer: { reducers: ["total"] } }, overrides: [{ match: { by: "regex", value: "(" }, set: {} }] }, transforms: [one] }),
  };
  for (const [name, panel] of Object.entries(cases)) assert.equal(part(run(panel, OVERLAY, { w: 700, h: name === "the rows are data" ? 300 : 100 }), "foot"), undefined, name);
  const unknown = run(tablePanel({ ...foot(["total"]), transforms: [one, { kind: "partition_by_values", fields: ["a"] }] }), OVERLAY, { w: 700, h: 100 });
  assert.equal(part(unknown, "foot"), undefined, "an unknown frame count");
  // An override with the same length as the defaults keeps the height certain.
  const same = tablePanel({ fields: { defaults: { footer: { reducers: ["min"] } }, overrides: [{ match: { by: "name", value: "a" }, set: { footer: { reducers: ["max"] } } }] }, transforms: [one] });
  assert.equal(num(part(run(same), "foot"), "--h"), 35);
});

test("R11: the footer names of column 0 are bars of the measured width, 22 px apart, and only when no override sets a footer and the name fits", () => {
  const out = run(tablePanel({ ...foot(["min", "last_not_null"]), transforms: [one] }));
  const names = parts(out).filter((n) => n.a["data-skel-part"] === "foot-name");
  assert.equal(names.length, 2);
  // The measurer gives 7 px for each character: MIN has 3, LAST * has 6.
  assert.deepEqual(names.map((n) => [num(n, "--x"), num(n, "--y"), num(n, "--w"), num(n, "--h")]), [[6, 14, 21, 8], [6, 36, 42, 8]]);
  assert.equal(cls(names[0]), "exact");
  assert.equal(part(out, "foot").k[0].v["--y"], "0px", "the rule on the top row of the band");
  // The td cuts a name that is wider than the cell: the least width of the cell is 40 px here.
  const narrow = run(tablePanel({ ...foot(["total"], { width: 40 }), transforms: [one] }));
  assert.equal(parts(narrow).filter((n) => n.a["data-skel-part"] === "foot-name").length, 0, "6 + 35 > 39");
  assert.ok(part(narrow, "foot"), "the footer stays");
  const override = tablePanel({ fields: { defaults: { footer: { reducers: ["min"] } }, overrides: [{ match: { by: "name", value: "a" }, set: { footer: { reducers: ["max"] } } }] }, transforms: [one] });
  assert.equal(parts(run(override)).filter((n) => n.a["data-skel-part"] === "foot-name").length, 0);
  // The page gives each character a letter spacing: the width follows it.
  const spaced = table.build(tablePanel({ ...foot(["min"]), transforms: [one] }), BOX, null, { env: { ...FINE, footLs: 0.15 } });
  assert.equal(num(part(spaced, "foot-name"), "--w"), 21 + 3 * 0.15);
});

// ---- R12: the header of column 0 ---------------------------------------------------------------------------------------------------------

const head = (defaults, over = {}) => tablePanel({ fields: { defaults, overrides: [] }, ...over });

test("R12: a literal display_name with a left alignment gives a label bar at x = 6 in the group of 22 px above the rule", () => {
  const out = run(head({ display_name: "Name", align: "left", width: 160 }));
  const label = part(out, "head-label");
  assert.deepEqual([num(label, "--x"), num(label, "--y"), num(label, "--w"), num(label, "--h")], [6, 12, 28, 8], "H - 22, centred in the group");
  assert.equal(cls(label), "exact");
  assert.equal(part(out, "mark"), undefined);
  assert.equal(part(out, "filter"), undefined);
  assert.equal(out.bounds.headLabel.value, 28);
});

test("R12: the sort mark and the filter button follow the label: 18 x 18, then 16 x 22 after 4 px (40 x 40 on a coarse pointer)", () => {
  const panel = head({ display_name: "Name", align: "left", width: 160, filterable: true }, { options: { show_header: true, sort: [{ field: "Name", desc: true }] } });
  const out = run(panel);
  const mark = part(out, "mark");
  assert.deepEqual([num(mark, "--x"), num(mark, "--y"), num(mark, "--w"), num(mark, "--h")], [34, 7, 18, 18]);
  const filter = part(out, "filter");
  assert.deepEqual([num(filter, "--x"), num(filter, "--y"), num(filter, "--w"), num(filter, "--h")], [56, 5, 16, 22]);
  const touch = run(panel, COARSE);
  assert.deepEqual([num(part(touch, "filter"), "--x"), num(part(touch, "filter"), "--y"), num(part(touch, "filter"), "--w"), num(part(touch, "filter"), "--h")], [56, 0, 40, 40]);
  assert.equal(num(part(touch, "head"), "--h"), 47);
  assert.equal(num(part(touch, "head-label"), "--y"), 25, "H - 22");
  // A sort key of another name gives no mark: every header has the name of the defaults.
  assert.equal(part(run(head({ display_name: "Name", align: "left", width: 160 }, { options: { show_header: true, sort: [{ field: "other", desc: false }] } })), "mark"), undefined);
});

test("R12: the conditions: a literal name, an explicit left or centre alignment, and no override of name, alignment, hidden or footer", () => {
  const none = (defaults, overrides = [], over = {}) => part(run(tablePanel({ fields: { defaults, overrides }, ...over })), "head-label");
  assert.ok(none({ display_name: "Name", align: "left" }));
  assert.equal(none({ display_name: "Name", align: "auto" }), undefined, "auto needs the kind of the field: data");
  assert.equal(none({ display_name: "Name", align: "right" }), undefined, "right: the last column has no rule, so 1 px of doubt");
  assert.equal(none({ display_name: "Name" }), undefined);
  assert.equal(none({ display_name: "${__field.name}", align: "left" }), undefined, "a template is not literal");
  assert.equal(none({ display_name: "[[x]]", align: "left" }), undefined);
  assert.equal(none({ display_name: "", align: "left" }), undefined);
  for (const set of [{ display_name: "Other" }, { align: "right" }, { hidden: true }]) assert.equal(none({ display_name: "Name", align: "left" }, [{ match: { by: "name", value: "a" }, set }]), undefined, JSON.stringify(set));
  assert.equal(none({ display_name: "Name", align: "left" }, [], { options: { show_header: false } }), undefined);
  // A text that does not fit in the least width of the column: the group is cut with an ellipsis (data).
  assert.equal(none({ display_name: "A long name", align: "left", width: 80 }), undefined);
  assert.ok(none({ display_name: "A long name", align: "left" }), "an automatic column is 150 px at least");
});

test("R12: a centred label needs the exact width of the column and a certain filter button", () => {
  const centred = (defaults, overrides = []) => part(run(tablePanel({ fields: { defaults: { display_name: "Name", align: "center", ...defaults }, overrides } })), "head-label");
  const label = centred({ width: 200 });
  assert.equal(num(label, "--x"), 6 + (200 - 13 - 28) / 2);
  assert.equal(num(centred({ width: 200, filterable: true }), "--x"), 6 + (200 - 13 - 28 - 20) / 2, "the filter button takes 20 px");
  assert.equal(centred({}), undefined, "an automatic width is data");
  assert.equal(centred({ width: 200 }, [{ match: { by: "name", value: "a" }, set: { width: 100 } }]), undefined);
  assert.equal(centred({ width: 200 }, [{ match: { by: "name", value: "a" }, set: { filterable: true } }]), undefined, "an override may add the filter button");
});

// ---- R13: the gauge of column 0 ------------------------------------------------------------------------------------------------------------

const gauge = (cell, defaults = {}, over = {}) => tablePanel({ fields: { defaults: { cell, ...defaults }, overrides: [] }, ...over });

test("R13: a basic or gradient gauge has a track of 22 px, centred in the 23 px of the cell, from x = 6", () => {
  for (const mode of ["basic", "gradient"]) {
    const out = run(gauge({ type: "gauge", mode }, { width: 160 }));
    const track = part(out, "track");
    assert.equal(cls(track), "extent");
    assert.deepEqual([num(track, "--x"), num(track, "--y"), num(track, "--h")], [6, 6.5, 22], mode);
    assert.equal(num(track, "--w"), 160 - 13);
    assert.equal(part(out, "row").k.includes(track), true, "the track sits in the slot");
  }
  assert.equal(num(part(run(gauge({ type: "gauge", mode: "basic" })), "track"), "--w"), 150 - 13, "the least width of an automatic column");
});

test("R13: with wrap the cell is top-aligned and the track is 6 px under the header, in a holder that claims no row", () => {
  const out = run(gauge({ type: "gauge", mode: "basic" }, { width: 160 }, { options: { wrap: true } }));
  assert.equal(num(part(out, "track"), "--y"), 6);
  assert.equal(part(out, "row"), undefined);
});

test("R13: an LCD gauge has max(1, floor((W - 11) / 12)) cells of 10 x 22 px, 2 px apart, when the width is exact", () => {
  const cells = (width) => parts(run(gauge({ type: "gauge", mode: "lcd" }, width === null ? {} : { width }))).filter((n) => n.a["data-skel-part"] === "cell");
  assert.equal(cells(160).length, 12);
  assert.deepEqual(cells(160).slice(0, 2).map((n) => [num(n, "--x"), num(n, "--w"), num(n, "--h")]), [[6, 10, 22], [18, 10, 22]]);
  assert.equal(cells(23).length, 1, "at least one cell");
  assert.equal(cells(60).length, 4);
  assert.equal(cells(null).length, 0, "an automatic width: the count is data");
  assert.equal(cls(cells(160)[0]), "exact");
  const withOverride = tablePanel({ fields: { defaults: { width: 160, cell: { type: "gauge", mode: "lcd" } }, overrides: [{ match: { by: "name", value: "a" }, set: { width: 90 } }] } });
  assert.equal(parts(run(withOverride)).filter((n) => n.a["data-skel-part"] === "cell").length, 0, "an override of the width");
});

test("R13: an override that sets the cell, or another cell type, gives no track", () => {
  assert.equal(part(run(gauge({ type: "color_text", mode: null }, { width: 160 })), "track"), undefined);
  const overridden = tablePanel({ fields: { defaults: { width: 160, cell: { type: "gauge", mode: "basic" } }, overrides: [{ match: { by: "name", value: "a" }, set: { cell: { type: "auto", mode: null } } }] } });
  assert.equal(part(run(overridden), "track"), undefined);
});

// ---- Decorative forms and the D evaluator ----------------------------------------------------------------------------------------------------

test("the derived column set is not built: a reduce with series_to_rows draws the generic form", () => {
  const out = run(tablePanel({ transforms: [{ kind: "reduce", mode: "series_to_rows", reducers: ["max"] }] }));
  assert.deepEqual(parts(out).map((n) => n.a["data-skel-part"]), ["plot", "head", "row"]);
});

test("the table preview has no column pitch, no row texture, no fill of a cell and no text bar of a guessed width", () => {
  const out = run(tablePanel({ fields: { defaults: { width: 120, footer: { reducers: ["total"] }, cell: { type: "color_background", mode: "basic" } }, overrides: [] }, transforms: [one] }));
  assert.deepEqual([...new Set(parts(out).map((n) => n.a["data-skel-part"]))].sort(), ["foot", "foot-name", "head", "plot", "row"]);
  assert.equal(parts(out).filter((n) => n.a["data-skel-part"] === "row").length, 1);
  const css = read("web/css/panel-loading.css");
  const section = css.slice(css.indexOf("family: table"), css.indexOf("family: heatmap"));
  assert.ok(!/--skel-line|#[0-9a-f]{3,8}\b|rgb\(|hsl\(/i.test(section), "neutral tones only");
  assert.ok(!/style=/.test(read("web/js/preview/table.js")), "no style attribute");
  for (const n of parts(out)) for (const [name, value] of Object.entries(n.v)) assert.match(value, /^-?[0-9.]+(px|%)?$/, `${name}`);
});

// ---- The logs panel -----------------------------------------------------------------------------------------------------------------------

function logsPanel(over = {}) {
  return { id: 5, type: "logs", title: "l", data: { query: query({ format: "logs" }) }, transforms: [], fields: { defaults: {}, overrides: [] }, options: {}, ...over };
}
const L = { sb: 10 };
const lbuild = (panel, box = BOX, env = L) => logs.build(panel, box, null, { env });

test("logs R5: the preview is one slot: the list box, one line of 28 px, and nothing after it", () => {
  const out = lbuild(logsPanel());
  assert.deepEqual(out.elements.map((n) => n.c), ["skel__logs"]);
  assert.equal(cls(part(out, "plot")), "exact");
  const slot = part(out, "row");
  assert.equal(cls(slot), "extent");
  assert.equal(slot.v["--sb"], "10px");
  assert.equal(parts(out).filter((n) => n.a["data-skel-part"] === "row").length, 1, "no slot after slot 1: it would claim a count of lines");
  assert.ok(out.notes.includes("slot"));
});

test("logs R6: the time bar comes only with show_time equal to true, and it is 23ch", () => {
  const time = (options) => part(lbuild(logsPanel({ options })), "time");
  assert.ok(time({ show_time: true }));
  assert.equal(cls(time({ show_time: true })), "exact");
  assert.equal(lbuild(logsPanel({ options: { show_time: true } })).bounds.time.value, "23ch");
  for (const options of [{}, { show_time: false }, { show_time: "true" }, { show_time: 1 }, { wrap: true }, { details: false }]) assert.equal(time(options), undefined, JSON.stringify(options));
  assert.equal(time({ show_time: true, wrap: true }) !== undefined, true, "wrap does not remove it");
});

test("logs R7: the track starts at the level cell: 23ch + 12 px after the time, 0 without it, with and without wrap", () => {
  const left = (options) => lbuild(logsPanel({ options })).bounds.trackLeft.value;
  assert.equal(left({}), "0");
  assert.equal(left({ show_time: true }), "23ch+12px");
  assert.equal(left({ show_time: true, wrap: true }), "23ch+12px");
  assert.equal(left({ wrap: true }), "0");
  assert.ok(lbuild(logsPanel({ options: { show_time: true, wrap: true } })).elements[0].k[0].c.includes("skel__log--wrap"), "the gap of 12 px is a padding of the time box");
  const css = read("web/css/panel-loading.css");
  assert.match(css, /\.skel--logs \.skel__log \{[^}]*column-gap: 12px/, "the gap between the columns");
  assert.match(css, /\.skel--logs \.skel__log--wrap \{ column-gap: 0/);
  assert.match(css, /\.skel--logs \.skel__log--wrap \.skel__ltime \{[^}]*box-sizing: content-box; padding-right: 12px/, "as the loaded wrap mode");
  assert.match(css, /\.skel--logs \.skel__ltime \{[^}]*width: 23ch/);
});

test("logs R7: the right edge is the least edge of a line: the list less its classic vertical bar", () => {
  assert.equal(lbuild(logsPanel(), { w: 700, h: 300 }, { sb: 10 }).bounds.trackRight.value, 690);
  assert.equal(lbuild(logsPanel(), { w: 700, h: 300 }, { sb: 0 }).bounds.trackRight.value, 700);
  assert.equal(part(lbuild(logsPanel(), BOX, { sb: 0 }), "row").v["--sb"], "0px");
  assert.ok(boundHolds({ kind: "least", value: 690 }, 690), "a line as wide as the client width");
  assert.ok(boundHolds({ kind: "least", value: 690 }, 2000), "a long line");
  assert.ok(!boundHolds({ kind: "least", value: 690 }, 600));
  assert.equal(cls(part(lbuild(logsPanel()), "track")), "extent");
});

test("logs R3: an invalid override pattern gives the warning line before the list, outside the mono box", () => {
  const panel = logsPanel({ fields: { defaults: {}, overrides: [{ match: { by: "regex", value: "(" }, set: {} }] } });
  const out = lbuild(panel);
  assert.equal(out.elements[0].t, "p");
  assert.match(out.elements[0].c, /panel__warning/);
  assert.equal(out.elements[1].c, "skel__logs");
  assert.equal(out.elements[0].k[1].x, " The override pattern ( is not a valid regular expression.");
  assert.equal(cls(part(out, "warning")), "exact");
  assert.equal(parts(lbuild(logsPanel())).filter((n) => n.a["data-skel-part"] === "warning").length, 0);
});

test("logs R10: the tree is the same for any order, details, wrap-free setting, field setting, link, title or grid", () => {
  const base = JSON.stringify(lbuild(logsPanel()));
  const variants = [
    { options: { order: "oldest_first" } }, { options: { details: false } }, { options: { details: true, order: "newest_first" } },
    { fields: { defaults: { unit: "ms", width: 40, cell: { type: "gauge" } }, overrides: [{ match: { by: "name", value: "x" }, set: { hidden: true } }, { match: { by: "regex", value: "^a$" }, set: { decimals: 1 } }] } },
    { links: [{ title: "a", url: "/d/x" }], transparent: true, title: "another", description: "d", grid: { x: 3, y: 4, w: 5, h: 6 } },
    { transforms: [{ kind: "organize" }] },
  ];
  for (const v of variants) assert.equal(JSON.stringify(lbuild(logsPanel(v))), base, JSON.stringify(v));
});

test("logs R9 and R11: neutral tones only, no accent, and nothing else than the slot, the time bar and the track", () => {
  const css = read("web/css/panel-loading.css");
  const section = css.slice(css.indexOf("family: logs"), css.indexOf("family: geomap"));
  assert.ok(!/--skel-line|#[0-9a-f]{3,8}\b|rgb\(|hsl\(/i.test(section), "no accent and no colour literal");
  assert.ok(/--skel-edge/.test(section) && /--skel-fill/.test(section));
  const out = lbuild(logsPanel({ options: { show_time: true, wrap: true } }));
  assert.deepEqual(parts(out).map((n) => n.a["data-skel-part"]).sort(), ["plot", "time", "track"]);
  const js = read("web/js/preview/logs.js");
  assert.ok(!/style=|\.sql\b/.test(js));
  for (const n of parts(out)) for (const [name, value] of Object.entries(n.v)) assert.match(value, /^-?[0-9.]+(px|%)?$/, name);
});

const lchain = (transforms, extra = {}) => lbuild(logsPanel({ transforms, ...extra }));
const empty = (out) => out.elements.length === 0 && out.notes.some((n) => /^no list|neutral|no visible/.test(n));

test("logs N1: a limit of 0 at the end of the chain proves no list, also behind keep kinds; a limit of 1 or an earlier limit does not", () => {
  assert.ok(empty(lchain([{ kind: "limit", count: 0, end: "first" }])));
  assert.ok(empty(lchain([{ kind: "limit", count: 0, end: "last" }, { kind: "sort_by", sort: [] }, { kind: "organize" }])));
  assert.ok(part(lchain([{ kind: "limit", count: 1, end: "first" }]), "row"));
  assert.ok(part(lchain([{ kind: "limit", count: 0, end: "first" }, { kind: "merge" }]), "row"), "a merge after the limit may add rows: not proven");
  assert.ok(part(lchain([{ kind: "limit", count: 0, end: "first" }, { kind: "calculate_field", mode: "index" }]), "row"));
  assert.ok(lchain([{ kind: "limit", count: 0 }]).notes[0].startsWith("no list"));
});

test("logs N2 and N3: a reduce, or a calculate_field that replaces the fields in the mode index or reduce_row, ends the chain with no list", () => {
  assert.ok(empty(lchain([{ kind: "reduce", mode: "series_to_rows" }])));
  assert.ok(empty(lchain([{ kind: "reduce", mode: "reduce_fields" }, { kind: "organize" }, { kind: "limit", count: 5 }])));
  assert.ok(empty(lchain([{ kind: "calculate_field", mode: "index", replace_fields: true }])));
  assert.ok(empty(lchain([{ kind: "calculate_field", mode: "reduce_row", replace_fields: true }, { kind: "sort_by", sort: [] }])));
  for (const t of [
    { kind: "calculate_field", mode: "index" }, { kind: "calculate_field", mode: "index", replace_fields: false },
    { kind: "calculate_field", mode: "binary", replace_fields: true }, { kind: "calculate_field", mode: "unary", replace_fields: true },
  ]) assert.ok(part(lchain([t]), "row"), JSON.stringify(t));
  assert.ok(part(lchain([{ kind: "reduce", mode: "series_to_rows" }, { kind: "merge" }]), "row"), "a kind that is not a keep kind after the reduce: not proven");
  assert.ok(part(lchain([{ kind: "limit", count: 3 }, { kind: "reduce", mode: "series_to_rows" }, { kind: "calculate_field", mode: "index" }]), "row"));
});

test("logs: the chain of the source panel counts first when data.reuse.with_transforms is true", () => {
  const source = { id: 1, type: "table", data: { query: query({}) }, transforms: [{ kind: "reduce", mode: "series_to_rows" }] };
  const doc = { sections: [{ panels: [source] }] };
  const reuse = (with_transforms) => logsPanel({ id: 2, data: { reuse: { panel: 1, with_transforms } }, transforms: [{ kind: "organize" }] });
  assert.ok(empty(logs.build(reuse(true), BOX, doc, { env: L })));
  assert.ok(part(logs.build(reuse(false), BOX, doc, { env: L }), "row"));
  assert.ok(part(logs.build(reuse(true), BOX, { sections: [] }, { env: L }), "row"), "a source that the document lacks proves nothing");
});

test("logs: a hidden query gives the neutral form", () => {
  const out = lbuild(logsPanel({ data: { query: query({ format: "logs", hide: true }) } }));
  assert.equal(out.elements.length, 0);
  assert.ok(!/\.sql\b|\.queries\b|\.expr\b/.test(read("web/js/preview/logs.js")));
});

test("both modules draw through the registry: a table and a logs panel give their family", () => {
  const t = previewOf(tablePanel(), { w: 500, h: 200 }, null, { env: FINE });
  assert.equal(t.family, "table");
  assert.ok(t.elements.length > 0);
  const l = previewOf(logsPanel(), { w: 500, h: 200 }, null, { env: L });
  assert.equal(l.family, "logs");
  assert.equal(l.elements.length, 1);
});
