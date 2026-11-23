// The catalog of the properties (4.18, 10.3a): it agrees with the canonical document of wave 2 and with the rules of the format.
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { PROPERTIES, PROPERTY_IDS, addableProperties, allowedIn, checkProperty, initialValue, orderSet, setErrors } from "../../web/js/editor/props.js";

const FIXTURE = join(dirname(fileURLToPath(import.meta.url)), "../fixtures/dashboards/f2_members.json");

// The members of 4.18 in the canonical order.
const IDS = ["unit", "decimals", "min", "max", "color", "thresholds", "mappings", "links", "display_name", "no_value", "width", "align", "cell", "filterable", "hidden", "footer", "inspect", "hide_from", "axis_placement", "axis_label", "soft_min", "soft_max", "axis_grid", "draw", "line_width", "fill_opacity", "line_dash", "show_points", "point_size", "transform", "line_dash_array", "axis_centered_zero", "axis_width", "axis_border", "axis_color"];

function setsOf(doc) {
  const out = [];
  for (const s of doc.sections) {
    for (const p of s.panels) {
      if (!p.fields) continue;
      out.push({ type: p.type, where: "defaults", set: p.fields.defaults });
      for (const o of p.fields.overrides) out.push({ type: p.type, where: "set", set: o.set });
    }
  }
  return out;
}

test("the catalog lists the 35 members of 4.18 in the canonical order (axis_grid of wave H, P03; the five members of P14)", () => {
  assert.deepEqual(PROPERTY_IDS, IDS);
  assert.equal(new Set(PROPERTY_IDS).size, PROPERTIES.length);
});

test("each member of f2_members.json is in the catalog and allowed where it stands", { skip: !existsSync(FIXTURE) && "the fixture of format-importer-2 is not in the tree yet" }, () => {
  const doc = JSON.parse(readFileSync(FIXTURE, "utf8"));
  const seen = new Set();
  for (const { type, where, set } of setsOf(doc)) {
    for (const id of Object.keys(set)) {
      assert.ok(PROPERTY_IDS.includes(id), `${id} is not in the catalog`);
      assert.ok(allowedIn(id, type, where), `${id} is not allowed in ${where} of a ${type}`);
      seen.add(id);
    }
  }
  for (const id of ["axis_placement", "axis_label", "soft_min", "soft_max", "draw", "line_width", "fill_opacity", "line_dash", "show_points", "point_size", "transform"]) {
    assert.ok(seen.has(id), `the fixture does not use ${id}`);
  }
});

test("where and the panel types follow 4.18", () => {
  assert.ok(allowedIn("no_value", "stat", "defaults"));
  assert.ok(!allowedIn("no_value", "stat", "set"));
  for (const id of ["draw", "line_width", "fill_opacity", "show_points", "point_size"]) {
    assert.ok(allowedIn(id, "timeseries", "set"), id);
    assert.ok(!allowedIn(id, "timeseries", "defaults"), id);
    assert.ok(!allowedIn(id, "barchart", "set"), id);
  }
  for (const id of ["axis_placement", "line_dash", "transform"]) {
    assert.ok(allowedIn(id, "timeseries", "defaults") && allowedIn(id, "timeseries", "set"), id);
    assert.ok(!allowedIn(id, "barchart", "set"), id);
  }
  for (const id of ["axis_label", "soft_min", "soft_max"]) assert.ok(allowedIn(id, "barchart", "defaults") && allowedIn(id, "timeseries", "set") && !allowedIn(id, "stat", "set"), id);
  for (const id of ["width", "align", "cell", "filterable", "hidden", "footer", "inspect"]) assert.ok(allowedIn(id, "table", "set") && !allowedIn(id, "timeseries", "set"), id);
  // `axis_grid` (P03): a bar chart only, in the defaults and in a set.
  assert.ok(allowedIn("axis_grid", "barchart", "defaults") && allowedIn("axis_grid", "barchart", "set") && !allowedIn("axis_grid", "timeseries", "set"));
  assert.ok(allowedIn("hide_from", "piechart", "set"));
});

test("the list of the select Add property", () => {
  const ids = (type, set) => addableProperties(type, set).map((p) => p.id);
  assert.ok(!ids("table", {}).includes("no_value"));
  assert.ok(!ids("table", {}).includes("draw"));
  assert.ok(ids("timeseries", {}).includes("draw"));
  assert.ok(!ids("timeseries", { draw: "bars" }).includes("draw"));
  assert.ok(!ids("stat", {}).includes("width"));
  assert.ok(ids("table", {}).includes("footer"));
});

test("each initial value is valid and allowed", () => {
  for (const p of PROPERTIES) {
    const v = initialValue(p.id);
    assert.notEqual(v, undefined, p.id);
    assert.equal(checkProperty(p.id, v), "", p.id);
  }
});

test("the checks of the members", () => {
  assert.notEqual(checkProperty("decimals", 21), "");
  assert.notEqual(checkProperty("decimals", 1.5), "");
  assert.equal(checkProperty("line_width", 10), "");
  assert.notEqual(checkProperty("line_width", 11), "");
  assert.notEqual(checkProperty("point_size", 0), "");
  assert.equal(checkProperty("point_size", 40), "");
  assert.notEqual(checkProperty("fill_opacity", 101), "");
  assert.notEqual(checkProperty("width", 9), "");
  assert.equal(checkProperty("width", 4000), "");
  assert.notEqual(checkProperty("axis_placement", "top"), "");
  assert.notEqual(checkProperty("axis_label", ""), "");
  assert.notEqual(checkProperty("axis_label", "x".repeat(257)), "");
  assert.notEqual(checkProperty("unit", "nonsense"), "");
  assert.equal(checkProperty("unit", "suffix: pts"), "");
  assert.equal(checkProperty("color", { mode: "continuous_blues", fixed: null }), "");
  assert.notEqual(checkProperty("color", { mode: "continuous_blues", fixed: "red" }), "");
  assert.notEqual(checkProperty("color", { mode: "fixed", fixed: null }), "");
});

test("min below max, soft_min below soft_max", () => {
  assert.deepEqual(setErrors({ min: 0, max: 1 }), { max: "", soft_max: "" });
  assert.notEqual(setErrors({ min: 1, max: 1 }).max, "");
  assert.notEqual(setErrors({ soft_min: 5, soft_max: 1 }).soft_max, "");
  assert.deepEqual(setErrors({ min: 5, soft_max: 1 }), { max: "", soft_max: "" });
});

test("orderSet writes the canonical order", () => {
  const out = orderSet({ transform: "none", unit: "ms", draw: "bars", axis_label: "a" });
  assert.deepEqual(Object.keys(out), ["unit", "axis_label", "draw", "transform"]);
});
