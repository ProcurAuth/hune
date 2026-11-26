// SPDX-License-Identifier: MIT OR Apache-2.0
// The loading previews of the pie chart and of the map (9.11): web/js/preview/piechart.js and web/js/preview/geomap.js.
// Each test follows a rule of the council files (piechart.md, geomap.md) and names the JSON member that decides it. The tests have no browser:
// the sizes come from the CSS, and the browser cases B370 to B377 measure them against the loaded panels.
import assert from "node:assert/strict";
import test from "node:test";
import { EXACT, LEAST, MOST, TOLERANCE_PX, boundHolds, setMeasurer } from "../../web/js/preview/common.js";
import { previewOf } from "../../web/js/preview/index.js";
import { LINE, extraColumns, geometry, literalName } from "../../web/js/preview/piechart.js";

// A measurer with a fixed width for each character, so that a width is easy to compute: 6 px for a character.
setMeasurer((text) => text.length * 6);

const legend = (over = {}) => ({ show: true, placement: "right", mode: "list", values: [], ...over });
const pie = (options = {}, fields = {}) => ({ id: 1, type: "piechart", options: { pie_type: "pie", reduce: { calc: "last", fields: null, values: true }, labels: [], legend: legend(), tooltip: { mode: "single" }, ...options }, fields });
const map = (options = {}, fields = {}) => ({ id: 1, type: "geomap", options: { view: { mode: "fixed", lat: 0, lon: 0, zoom: 1 }, layer: {}, tooltip: "details", show_zoom: true, mouse_wheel_zoom: true, ...options }, fields });
const build = (panel, w = 600, h = 300) => previewOf(panel, { w, h }, {}, {});
const walk = (nodes, fn) => nodes.forEach((n) => (typeof n === "string" ? null : (fn(n), walk(n.k || [], fn))));
const find = (out, cls) => {
  let hit = null;
  walk(out.elements, (n) => {
    if (!hit && (n.c || "").split(" ").includes(cls)) hit = n;
  });
  return hit;
};
const count = (out) => {
  let n = 0;
  walk(out.elements, () => n++);
  return n;
};
const partOf = (out, part) => {
  const hits = [];
  walk(out.elements, (n) => (n.a && n.a["data-skel-part"] === part ? hits.push(n) : null));
  return hits;
};

// ---- Pie: the box and the legend -------------------------------------------------------------------------------------------------

test("pie E3, E4: a hidden legend (legend.show = false) gives no legend box and a plot of the whole box, exact", () => {
  const out = build(pie({ legend: legend({ show: false }) }), 400, 250);
  assert.equal(find(out, "skel__legend"), null);
  assert.equal(find(out, "skel__pie--right") !== null, true, "no legend: the row form of the loaded pie, with one child");
  const g = geometry({ legend: legend({ show: false }), box: { w: 400, h: 250 } });
  assert.deepEqual(g.plot, { x: 0, y: 0, w: 400, h: 250 });
  assert.equal(g.cls, EXACT);
  assert.equal(out.bounds.plotW.kind, EXACT);
  assert.equal(out.bounds.plotH.value, 250);
  assert.equal(partOf(out, "plot")[0].a["data-skel-class"], EXACT);
  assert.equal(out.bounds.legendW, undefined, "no legend: nothing to claim");
});

test("pie E5: a bottom legend (legend.placement = bottom) is one line of 18.857 px under a gap of 4 px, bounded by 42 %", () => {
  assert.ok(Math.abs(LINE - 18.8568) < 1e-9);
  const out = build(pie({ legend: legend({ placement: "bottom" }) }), 400, 250);
  assert.ok(find(out, "skel__pie--bottom"));
  const g = geometry({ legend: legend({ placement: "bottom" }), box: { w: 400, h: 250 } });
  assert.ok(Math.abs(g.plot.h - (250 - 4 - LINE)) < 1e-9);
  assert.deepEqual(g.legend.w, 400);
  assert.equal(g.cls, MOST, "a legend of several lines gives a smaller loaded plot: the preview plot is a bound");
  assert.equal(out.bounds.plotH.kind, MOST);
  assert.equal(out.bounds.legendH.kind, LEAST, "the loaded legend has one line or more");
  assert.equal(partOf(out, "legend")[0].a["data-skel-class"], LEAST);
});

test("pie E5: a bottom pie that is lower than 44.9 px has a legend as high as the 42 % cap, whatever the data: exact", () => {
  const g = geometry({ legend: legend({ placement: "bottom" }), box: { w: 300, h: 40 } });
  assert.ok(Math.abs(g.legend.h - 0.42 * 40) < 1e-9, "the cap binds");
  assert.equal(g.cls, EXACT);
  assert.equal(geometry({ legend: legend({ placement: "bottom" }), box: { w: 300, h: 44.8 } }).cls, EXACT, "the limit is H x 0.42 = one line (H = 44.9 px)");
  assert.equal(geometry({ legend: legend({ placement: "bottom" }), box: { w: 300, h: 45 } }).cls, MOST);
});

test("pie E6: a right legend (legend.placement = right) takes the floor of 72 px and a gap of 12 px: the plot is the box less 84 px", () => {
  const out = build(pie(), 600, 300);
  const g = geometry({ legend: legend(), box: { w: 600, h: 300 } });
  assert.deepEqual(g.plot, { x: 0, y: 0, w: 516, h: 300 });
  assert.equal(g.cls, MOST, "the loaded legend can be wider than the floor");
  assert.equal(out.bounds.plotW.kind, MOST);
  assert.equal(out.bounds.plotW.value, 516);
  assert.equal(out.bounds.plotH.kind, MOST);
  assert.equal(out.bounds.legendW.value, 72);
  assert.equal(partOf(out, "legend")[0].a["data-skel-class"], LEAST);
});

test("pie E6: a right pie of 138.46 px or less has a legend of 72 px whatever the data (52 % is not above the floor): exact", () => {
  assert.equal(geometry({ legend: legend(), box: { w: 138, h: 200 } }).cls, EXACT);
  assert.equal(geometry({ legend: legend(), box: { w: 138.46, h: 200 } }).cls, EXACT);
  assert.equal(geometry({ legend: legend(), box: { w: 139, h: 200 } }).cls, MOST);
  assert.deepEqual(geometry({ legend: legend(), box: { w: 100, h: 200 } }).plot, { x: 0, y: 0, w: 16, h: 200 });
});

test("pie E8: the count of value columns is the value column (a table, or a value in legend.values) and the percent column", () => {
  assert.equal(extraColumns(legend()), 0);
  assert.equal(extraColumns(legend({ values: ["percent"] })), 1);
  assert.equal(extraColumns(legend({ values: ["value"] })), 1);
  assert.equal(extraColumns(legend({ mode: "table" })), 1, "a table has a value column");
  assert.equal(extraColumns(legend({ mode: "table", values: ["percent"] })), 2);
  assert.equal(extraColumns(legend({ mode: "table", values: ["value", "percent"] })), 2, "the value column is counted once");
  assert.equal(extraColumns(legend({ values: ["percent", "value"] })), 2, "the order of the list is not a rule");
  for (const [options, cols] of [[legend(), 0], [legend({ values: ["value"] }), 1], [legend({ mode: "table", values: ["value", "percent"] }), 2]]) {
    const out = build(pie({ legend: options }));
    const box = find(out, "skel__legend");
    assert.ok(box.c.split(" ").includes(`skel__legend--c${cols}`), box.c);
    assert.equal(box.k.length, 2 + cols, "a swatch cell, a name cell and the empty value cells");
  }
});

test("pie E9, R9: the preview has no mark in the legend: the cells are empty, and the tree has no text, no slice and no swatch tone", () => {
  const out = build(pie({ legend: legend({ mode: "table", values: ["value", "percent"] }) }));
  walk(out.elements, (n) => {
    assert.ok(!("style" in (n.a || {})), "no style attribute");
    assert.ok((n.k || []).every((k) => typeof k !== "string"), "no text in the tree");
    assert.ok(!/skel__b\b|skel__path|skel__svg|skel__line/.test(n.c || ""), `no bar, path or line: ${n.c}`);
  });
  const names = [];
  walk(out.elements, (n) => names.push(n.c));
  assert.deepEqual(names, ["skel__holder", "skel__pie skel__pie--right", "skel__plot", "skel__ring", "skel__legend skel__legend--right skel__legend--c2", "skel__sw", "skel__name", "skel__val", "skel__val"]);
});

test("pie E7: a literal display name (fields.defaults.display_name, reduce.values = false) gives the width of the name to the empty name cell", () => {
  const panel = pie({ reduce: { calc: "last", fields: null, values: false } }, { defaults: { display_name: "Total weight" }, overrides: [] });
  assert.equal(literalName(panel), "Total weight");
  const out = build(panel, 600, 300);
  const name = find(out, "skel__name");
  assert.equal(name.v["--w"], "72px", "12 characters of 6 px");
  // The content is 10 + 8 + 72 = 90 px, above the floor of 72 px and under 52 % of 600 px: the width of the legend is fixed, so the plot is exact.
  const g = geometry({ legend: legend(), box: { w: 600, h: 300 }, name: "Total weight" });
  assert.equal(g.legend.w, 90);
  assert.equal(g.plot.w, 600 - 12 - 90);
  assert.equal(g.cls, EXACT);
  assert.equal(out.bounds.plotW.kind, EXACT);
  assert.equal(out.bounds.plotW.value, 498);
  assert.equal(partOf(out, "plot")[0].a["data-skel-class"], EXACT);
  // A column of values adds a width that the data decide: a bound.
  const table = build({ ...panel, options: { ...panel.options, legend: legend({ mode: "table" }) } }, 600, 300);
  assert.equal(table.bounds.plotW.kind, MOST);
  // A long name is held by the cap of 52 %.
  const long = geometry({ legend: legend(), box: { w: 200, h: 100 }, name: "x".repeat(100) });
  assert.equal(long.legend.w, 104);
});

test("pie E7: the name is data when a template mark, an override of display_name, a row item or an empty name is possible", () => {
  const f = (display_name, overrides = []) => ({ defaults: { display_name }, overrides });
  const off = { calc: "last", fields: null, values: false };
  assert.equal(literalName(pie({ reduce: off }, f("name"))), "name");
  assert.equal(literalName(pie({ reduce: off }, f("${__field.name}"))), null);
  assert.equal(literalName(pie({ reduce: off }, f("[[__field.name]]"))), null);
  assert.equal(literalName(pie({ reduce: off }, f(""))), null);
  assert.equal(literalName(pie({ reduce: off }, {})), null);
  assert.equal(literalName(pie({ reduce: off }, f("name", [{ match: { by: "name", value: "a" }, set: { display_name: "other" } }]))), null);
  assert.equal(literalName(pie({ reduce: off }, f("name", [{ match: { by: "name", value: "a" }, set: { unit: "none" } }]))), "name", "another member of an override does not change the name");
  assert.equal(literalName(pie({ reduce: { ...off, values: true } }, f("name"))), null, "with rows, the names come from the data");
  // The name goes only into the right legend: a bottom legend has no name cell.
  const bottom = build(pie({ reduce: off, legend: legend({ placement: "bottom" }) }, f("name")));
  assert.equal(find(bottom, "skel__name"), null);
});

test("pie E12, E13: the ring is the loaded circle: the least side less 16 px, not under 20 px, and a hole for a donut only (options.pie_type)", () => {
  assert.equal(geometry({ legend: legend({ show: false }), box: { w: 400, h: 250 } }).d, 234);
  assert.equal(geometry({ legend: legend({ show: false }), box: { w: 30, h: 250 } }).d, 20, "the floor of the radius is 10 px");
  assert.equal(geometry({ legend: legend(), box: { w: 600, h: 300 } }).d, 284);
  const donut = build(pie({ pie_type: "donut" }));
  assert.ok(find(donut, "skel__ring--donut"));
  assert.ok(donut.notes.includes("donut"));
  const plain = build(pie({ pie_type: "pie" }));
  assert.equal(find(plain, "skel__ring--donut"), null);
  assert.equal(find(build(pie({ pie_type: "anything" })), "skel__ring--donut"), null, "only the exact word donut gives a hole (the renderer tests === donut)");
  assert.equal(partOf(donut, "ring").length, 1);
});

test("pie R10: members with no effect change no node: sort, labels, tooltip, calc, fields, unit, decimals, colour, thresholds, mappings, no_value, transforms", () => {
  const base = build(pie({ legend: legend({ mode: "table", values: ["value", "percent"] }) }));
  const other = build({
    ...pie({
      legend: legend({ mode: "list", values: ["percent", "value"], sort_by: "name", sort_desc: true }),
      labels: ["name", "value", "percent"],
      tooltip: { mode: "hidden" },
      reduce: { calc: "mean", fields: "/^a$/", values: true },
    }, { defaults: { unit: "percent", decimals: 3, color: { mode: "fixed", fixed: "red" }, thresholds: { mode: "absolute", steps: [] }, mappings: [], no_value: "none" }, overrides: [{ match: { by: "name", value: "a" }, set: { color: { mode: "fixed", fixed: "blue" } } }] }),
    transforms: [{ id: "limit" }],
  });
  assert.deepEqual(other.elements, base.elements);
  assert.deepEqual(other.bounds, base.bounds);
});

test("pie: a missing legend member gives the default of the loaded code (right, list), and a box with no size gives no negative size", () => {
  const out = build({ id: 1, type: "piechart", options: { reduce: { values: true } }, fields: {} });
  assert.ok(find(out, "skel__legend--right"));
  const small = geometry({ legend: legend(), box: { w: 40, h: 40 } });
  assert.equal(small.plot.w, 0);
  assert.equal(small.d, 20);
});

test("pie: each custom property of the tree is a px value or a percent, and a bound holds against a loaded value by its class", () => {
  const out = build(pie({ reduce: { calc: "last", fields: null, values: false } }, { defaults: { display_name: "abc" }, overrides: [] }));
  walk(out.elements, (n) => {
    for (const [name, value] of Object.entries(n.v || {})) {
      assert.match(name, /^--[a-z-]+$/);
      assert.match(String(value), /^-?[0-9.]+(px|%)?$/);
    }
  });
  // A bounded plot: the preview is at least the loaded plot.
  const b = build(pie()).bounds;
  assert.equal(boundHolds(b.plotW, 400), true);
  assert.equal(boundHolds(b.plotW, 516.4), true);
  assert.equal(boundHolds(b.plotW, 530), false, "a loaded plot wider than the preview breaks the bound");
  assert.equal(boundHolds(b.legendW, 150), true);
  assert.equal(boundHolds(b.legendW, 60), false, "a loaded legend under the floor breaks the bound");
  const e = build(pie({ legend: legend({ show: false }) }), 400, 250).bounds;
  assert.equal(boundHolds(e.plotW, 400.5, TOLERANCE_PX), true);
  assert.equal(boundHolds(e.plotW, 401, TOLERANCE_PX), false);
});

// ---- The warning line --------------------------------------------------------------------------------------------------------------

const BAD = { match: { by: "regex", value: "/(/" }, set: { unit: "none" } };
const GOOD = { match: { by: "regex", value: "^a.*$" }, set: { unit: "none" } };

test("pie and map E2: an override of the kind regex that does not compile gives the warning line, with the loaded sentence, before the plot", () => {
  for (const make of [pie, map]) {
    const out = build(make({}, { defaults: {}, overrides: [GOOD, BAD, { ...BAD }, { match: { by: "regex", value: "(" }, set: {} }, { match: { by: "name", value: "(" }, set: {} }] }));
    const first = out.elements[0];
    assert.equal(first.t, "p");
    assert.equal(first.c, "panel__warning skel__warning");
    assert.equal(first.a["data-skel-part"], "warning");
    assert.equal(first.a["data-skel-class"], EXACT);
    assert.equal(first.a.role, undefined, "the loaded line has no role");
    const [icon, text] = first.k;
    assert.equal(icon.t, "svg");
    assert.equal(icon.a.width, "14");
    assert.equal(icon.k[0].t, "use");
    assert.match(icon.k[0].a.href, /^#hune-icon-warning$/);
    // One sentence for each distinct pattern, in the order of the overrides, joined by a space after a space.
    assert.equal(text, " The override pattern /(/ is not a valid regular expression. The override pattern ( is not a valid regular expression.");
    assert.ok(out.notes.includes("warning"));
  }
});

test("pie and map E2: a valid pattern, a name matcher and no override give no warning line, and the order of the tree is the loaded order", () => {
  for (const make of [pie, map]) {
    for (const fields of [{}, { defaults: {}, overrides: [] }, { defaults: {}, overrides: [GOOD] }, { defaults: {}, overrides: [{ match: { by: "name", value: "/(/" }, set: {} }] }]) {
      const out = build(make({}, fields));
      assert.equal(partOf(out, "warning").length, 0);
    }
  }
});

test("pie E2: a warning line keeps the widths and drops the heights: the browser gives the wrapped height", () => {
  const out = build(pie({}, { defaults: {}, overrides: [BAD] }), 600, 300);
  assert.equal(out.bounds.plotH.kind, "data");
  assert.equal(out.bounds.ringD.kind, "data");
  assert.equal(out.bounds.plotW.value, 516);
  assert.equal(out.bounds.legendH.kind, "data");
  assert.equal(out.elements.length, 2);
  assert.equal(out.elements[1].c, "skel__holder");
});

// ---- Map ---------------------------------------------------------------------------------------------------------------------------

test("map E1, E4: the map node takes the whole box, with no legend, no band and no fill; its box is exact", () => {
  const out = build(map({ show_zoom: false }), 500, 300);
  assert.equal(out.elements.length, 1);
  const node = out.elements[0];
  assert.equal(node.c, "skel__map");
  assert.deepEqual(node.v, {}, "no size from JavaScript: the CSS gives the box");
  assert.equal(node.a["data-skel-part"], "plot");
  assert.equal(node.a["data-skel-class"], EXACT);
  assert.equal(out.bounds.plotW.value, 500);
  assert.equal(out.bounds.plotH.value, 300);
  assert.equal(count(out), 1);
});

test("map E5, R3, R5: show_zoom = true gives the zoom column of two squares of 19.25 px at 8 px, and the tree has four nodes", () => {
  const out = build(map(), 500, 300);
  assert.equal(count(out), 4);
  const squares = partOf(out, "zoom");
  assert.equal(squares.length, 2);
  for (const s of squares) {
    assert.equal(s.a["data-skel-class"], EXACT);
    assert.deepEqual(s.k, [], "no glyph, no child");
    assert.deepEqual(s.v, {}, "the size is in the CSS: px() would round 19.25 to 19.3");
  }
  assert.equal(out.bounds.zoomW.value, 19.25);
  assert.equal(out.bounds.zoomX.value, 8);
  assert.equal(out.elements[0].k[0].c, "skel__zooms");
});

test("map E5: show_zoom = false, a missing member or a text gives no zoom square (the loaded column has the hidden attribute)", () => {
  for (const show_zoom of [false, undefined, "true", 1, null]) {
    const out = build(map({ show_zoom }));
    assert.equal(count(out), 1, String(show_zoom));
    assert.equal(partOf(out, "zoom").length, 0);
  }
});

test("map E6, E7, E8, E9, E10, R6: no member of view, layer, tooltip, wheel, defaults, valid overrides, transforms or transparent changes the tree", () => {
  const base = build(map());
  const same = [
    map({ view: { mode: "fit", lat: 48, lon: 2, zoom: 22 } }),
    map({ view: { mode: "fixed", lat: -85, lon: 180, zoom: 0 } }),
    map({ layer: { lat_field: "a", lon_field: "b", size: 20, size_field: "c", color: "red", color_field: "d", opacity: 100 } }),
    map({ tooltip: "none", mouse_wheel_zoom: false }),
    map({}, { defaults: { unit: "percent", decimals: 4, min: 0, max: 9, color: { mode: "fixed", fixed: "red" } }, overrides: [GOOD] }),
    { ...map(), transforms: [{ id: "limit" }], links: [{ title: "x", url: "y" }], transparent: true },
  ];
  for (const p of same) {
    const out = build(p);
    assert.deepEqual(out.elements, base.elements);
    assert.deepEqual(out.bounds, base.bounds);
  }
  // Only show_zoom and an invalid pattern change it.
  assert.notDeepEqual(build(map({ show_zoom: false })).elements, base.elements);
  assert.notDeepEqual(build(map({}, { defaults: {}, overrides: [BAD] })).elements, base.elements);
});

test("map E2: a warning line keeps the width of the map and drops its height", () => {
  const out = build(map({}, { defaults: {}, overrides: [BAD] }), 500, 300);
  assert.equal(out.elements.length, 2);
  assert.equal(out.bounds.plotW.value, 500);
  assert.equal(out.bounds.plotH.kind, "data");
  assert.equal(out.elements[1].c, "skel__map");
});

test("map R5: the tree has no marker, no line, no tile, no note, no attribution and no card", () => {
  const all = [];
  walk(build(map()).elements, (n) => all.push(n.c));
  assert.deepEqual(all, ["skel__map", "skel__zooms", "skel__zoom", "skel__zoom"]);
});

test("each family gives a note that names the rule, and a type that is not drawn by this lot keeps its neutral form", () => {
  assert.ok(build(pie()).notes.includes("piechart"));
  assert.ok(build(map()).notes.includes("geomap"));
  assert.deepEqual(build({ id: 1, type: "stat", options: {}, fields: {} }).notes, ["neutral"]);
});
