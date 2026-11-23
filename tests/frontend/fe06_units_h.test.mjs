// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-06, the later options (P01): the 30 units of 4.20 that the later options adds, against the texts of Grafana 12.4.2 (9.15). The vector file holds the
// measure as it is: `expect` is the text of the page, and `grafana` is there only where the page differs on purpose (19.20n).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { makeDisplay } from "../../web/js/display.js";
import { UNIT_HINTS } from "../../web/js/editor/forms/properties.js";
import { isUnitValid } from "../../web/js/editor/validators.js";
import { computeFieldConfig } from "../../web/js/fields.js";
import { formatNumber, joinParts } from "../../web/js/units.js";
import { toFields } from "./helpers.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const v = JSON.parse(read("./vectors/FE-06-units-h.json"));
const num = (x) => (x === "NaN" ? NaN : x === "Infinity" ? Infinity : x);

test("FE-06 wave H: the text of each unit, value and decimals is the text of Grafana", () => {
  assert.equal(Object.keys(v.units).length, 30);
  for (const [unit, rows] of Object.entries(v.units)) {
    v.decimals.forEach((d, k) => {
      v.values.forEach((value, i) => {
        assert.equal(joinParts(formatNumber(value, unit, d)), rows[k][i], `${unit} ${value} decimals ${d}`);
      });
    });
  }
});

test("FE-06 wave H: the display of a number, a text and a bool value with each unit", () => {
  const kinds = { number: [0], text: ["x"], bool: [true] };
  for (const c of v.display) {
    const [field] = computeFieldConfig(toFields([{ name: "f", kind: c.kind, values: kinds[c.kind] }]), { defaults: { unit: c.unit, decimals: c.d }, overrides: [] }, "table").fields;
    assert.equal(makeDisplay(field).valueDisplay(num(c.v)).text, c.expect, `${c.unit} ${c.kind} ${c.v} decimals ${c.d}`);
  }
});

test("FE-06 wave H: the main parts of a scaled unit, a clock and a bool unit", () => {
  assert.deepEqual(formatNumber(1536, "kbytes"), { prefix: "", text: "1.50", suffix: " MiB" });
  assert.deepEqual(formatNumber(0.5, "sishort"), { prefix: "", text: "500", suffix: " m" });
  assert.deepEqual(formatNumber(-42, "dtdhms"), { prefix: "", text: "00:00:42", suffix: " ago" });
  assert.deepEqual(formatNumber(1, "bool_on_off"), { prefix: "", text: "On", suffix: "" });
  // `decbytes` is the SI scale of Grafana: a value below 1 takes a prefix below none (measured on Grafana 12.4.2).
  assert.equal(joinParts(formatNumber(0.5, "decbytes")), "500 mB");
  // The three bool units have one rule: NaN gives NaN, as the display of Grafana does.
  for (const u of ["bool", "bool_yes_no", "bool_on_off"]) assert.equal(joinParts(formatNumber(NaN, u)), "NaN", u);
  // Two edges of the run of the page code and of the Grafana formatter in one Chrome (2026-11-22, 1,544,400 cases, no difference):
  // the automatic decimals use Math.log / Math.LN10 as Grafana, and the clock of dtdhms writes the size of a remainder below 0.
  assert.equal(joinParts(formatNumber(1e-7, "deckbytes")), "100.0 µB");
  assert.equal(joinParts(formatNumber(99.99999999999999, "none")), "100.0");
  assert.equal(joinParts(formatNumber(2.5e22, "dtdhms")), "289351851851851840 d 1165:05:04");
});

// The ids of the table of 4.20 in docs/02, without `suffix:<text>` and `prefix:<text>`.
function documentedUnits() {
  const spec = read("../../docs/02-format-and-storage.md");
  const section = spec.slice(spec.indexOf("### 4.20 Units"), spec.indexOf("### 4.21"));
  return [...section.matchAll(/^\| `([^`]+)` \|/gm)].map((m) => m[1]).filter((u) => !u.includes(":"));
}

test("FE-06 wave H: the closed list of 4.20 is the same in docs/02, the format check, the editor and the page", () => {
  const ids = documentedUnits();
  assert.equal(ids.length, 59);
  const rust = read("../../crates/hune-core/src/document/grammar.rs");
  const list = rust.slice(rust.indexOf("const FIXED_UNITS"), rust.indexOf("];", rust.indexOf("const FIXED_UNITS")));
  const rustIds = [...list.matchAll(/"([^"]+)"/g)].map((m) => m[1].replace("\\u{b5}", "µ"));
  assert.deepEqual(rustIds, ids);
  assert.deepEqual(UNIT_HINTS.filter((u) => !u.includes(":")), ids);
  for (const u of ids) assert.ok(isUnitValid(u), u);
  for (const u of ["pixel", "Tbits", "bool:on/off", "si:B", "count:rows", "kbits", "String"]) assert.ok(!isUnitValid(u), u);
  // Each unit other than `none` has its own rule in the page: none of them gives the plain number of the default branch.
  for (const u of ids.filter((x) => x !== "none")) assert.notEqual(joinParts(formatNumber(1234.5678, u, null, null, "utc")), "1235", u);
});
