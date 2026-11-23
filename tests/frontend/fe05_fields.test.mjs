// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-05: field configuration and matchers (9.14).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { computeFieldConfig, parseGrafanaPattern, matchesField } from "../../web/js/fields.js";
import { toFields } from "./helpers.mjs";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-05-fields.json", import.meta.url)));

test("FE-05 defaults, overrides in order, built-in defaults", () => {
  const { fields, warnings } = computeFieldConfig(toFields(v.cols), v.spec, "table");
  assert.deepEqual(warnings, v.warnings);
  v.expect.forEach((e, i) => {
    for (const [k, val] of Object.entries(e)) assert.deepEqual(fields[i].config[k], val, `${fields[i].name}.${k}`);
  });
  // The last override wins, property by property: the "name" override with decimals 2 comes after the regex one.
  assert.equal(fields[1].config.decimals, 2);
  // no_value comes from the defaults only.
  assert.equal(fields[2].config.no_value, "-");
});

test("FE-05 Grafana pattern form", () => {
  for (const c of v.patterns) {
    const re = parseGrafanaPattern(c.p);
    if (c.compiles === false) assert.equal(re, null, c.p);
    else {
      assert.ok(re, c.p);
      assert.equal(matchesField({ by: "regex", value: c.p }, { name: c.text }, c.text), c.match, `${c.p} ${c.text}`);
    }
  }
});

test("FE-05 byName matches the name or the base display name", () => {
  assert.ok(matchesField({ by: "name", value: "weight a1" }, { name: "weight" }, "weight a1"));
  assert.ok(matchesField({ by: "name", value: "weight" }, { name: "weight" }, "weight a1"));
  assert.ok(!matchesField({ by: "name", value: "Weight" }, { name: "weight" }, "weight"));
});

test("FE-05 default colour mode by panel type, default thresholds, bounds", () => {
  for (const [type, mode] of Object.entries(v.defaults_by_type)) {
    const { fields } = computeFieldConfig(toFields([{ name: "x", kind: "number", values: [1, null, 3] }]), { defaults: {}, overrides: [] }, type);
    assert.equal(fields[0].config.color.mode, mode, type);
    assert.deepEqual(fields[0].config.thresholds.steps, [{ value: null, color: "green" }, { value: 80, color: "red" }]);
    assert.equal(fields[0].config.min, 1);
    assert.equal(fields[0].config.max, 3);
  }
});

// F03: the matcher `names` and the property `hide_from` (9.14).
test("F03 matcher names: include, exclude, name and base display name", () => {
  const fields = toFields([
    { name: "t", kind: "time", values: [1, 2] },
    { name: "FRA p99", kind: "number", values: [1, 2] },
    { name: "FRA p50", kind: "number", values: [1, 2] },
    { name: "DEU p99", kind: "number", values: [1, 2] },
  ]);
  const include = { by: "names", names: ["FRA p99", "FRA p50"], mode: "include" };
  const exclude = { by: "names", names: ["FRA p99", "FRA p50"], mode: "exclude" };
  assert.deepEqual(fields.map((f) => matchesField(include, f, f.name)), [false, true, true, false]);
  assert.deepEqual(fields.map((f) => matchesField(exclude, f, f.name)), [true, false, false, true]);
  // The base display name counts as well as the name.
  assert.equal(matchesField({ by: "names", names: ["Shown"], mode: "include" }, fields[1], "Shown"), true);
  assert.equal(matchesField({ by: "names", names: ["Shown"], mode: "exclude" }, fields[1], "Shown"), false);
  // The comparison is exact.
  assert.equal(matchesField({ by: "names", names: ["fra p99"], mode: "include" }, fields[1], "FRA p99"), false);
  // A malformed matcher matches nothing.
  assert.equal(matchesField({ by: "names", mode: "include" }, fields[1], "FRA p99"), false);
  assert.equal(matchesField({ by: "names", names: ["FRA p99"], mode: "other" }, fields[1], "FRA p99"), false);
});

test("F03 hide_from: default, the override of Grafana for a legend click, replacement", () => {
  const fields = toFields([
    { name: "t", kind: "time", values: [1, 2] },
    { name: "FRA p99", kind: "number", values: [1, 2] },
    { name: "FRA p50", kind: "number", values: [1, 2] },
    { name: "DEU p99", kind: "number", values: [1, 2] },
  ]);
  const spec = {
    defaults: {},
    overrides: [
      // Grafana writes this override when a viewer clicks a series: the listed names stay visible, the others hide.
      { match: { by: "names", names: ["FRA p99", "FRA p50"], mode: "exclude" }, set: { hide_from: { legend: false, tooltip: true, viz: true } } },
    ],
  };
  const out = computeFieldConfig(fields, spec, "timeseries").fields;
  assert.deepEqual(out.map((f) => f.config.hide_from), [
    { legend: false, tooltip: true, viz: true },
    { legend: false, tooltip: false, viz: false },
    { legend: false, tooltip: false, viz: false },
    { legend: false, tooltip: true, viz: true },
  ]);
  // The default is all false, also with no override.
  assert.deepEqual(computeFieldConfig(fields, null, "timeseries").fields[1].config.hide_from, { legend: false, tooltip: false, viz: false });
  // A later override replaces the whole object, and the defaults give the base.
  const later = computeFieldConfig(fields, {
    defaults: { hide_from: { legend: true, tooltip: false, viz: false } },
    overrides: [{ match: { by: "name", value: "FRA p50" }, set: { hide_from: { legend: false, tooltip: false, viz: true } } }],
  }, "timeseries").fields;
  assert.deepEqual(later[1].config.hide_from, { legend: true, tooltip: false, viz: false });
  assert.deepEqual(later[2].config.hide_from, { legend: false, tooltip: false, viz: true });
  // The footer is null without a property.
  assert.equal(later[1].config.footer, null);
});
