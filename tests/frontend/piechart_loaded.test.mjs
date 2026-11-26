// SPDX-License-Identifier: MIT OR Apache-2.0
// The loaded pie chart against the specification (9.19, 9.19a, 4.26, 9.29).
// D1: the pie sizes its circle from the real box with the fraction of a pixel, not from the rounded clientWidth (9.19a).
// D2: the pie has no default for the member `legend`, which is necessary (4.26).
// The test drives the real module `panels/piechart.js` on the small fake DOM of the other panel tests.
import test from "node:test";
import assert from "node:assert/strict";
import { FakeNode, installFakeDom } from "./fake_dom.mjs";
import { computeFieldConfig } from "../../web/js/fields.js";
import { baseDisplayNames, displayNames } from "../../web/js/names.js";
import { toFields } from "./helpers.mjs";

installFakeDom();
globalThis.document.documentElement = new FakeNode("html");
globalThis.document.documentElement.dataset = { theme: "dark" };
globalThis.window = { devicePixelRatio: 1 };
globalThis.getComputedStyle = () => ({ getPropertyValue: () => "#123456", borderLeftWidth: "0", borderRightWidth: "0", borderTopWidth: "0", borderBottomWidth: "0", paddingLeft: "0", paddingRight: "0", paddingTop: "0", paddingBottom: "0" });
globalThis.ResizeObserver = class {
  observe() {}
  disconnect() {}
};

// The real box of every node: a size with a fraction. The rounded sizes are the ones that clientWidth and clientHeight give.
let BOX = { width: 333.6, height: 200.4 };
Object.defineProperty(FakeNode.prototype, "clientWidth", { configurable: true, get() { return Math.round(BOX.width); } });
Object.defineProperty(FakeNode.prototype, "clientHeight", { configurable: true, get() { return Math.round(BOX.height); } });
Object.defineProperty(FakeNode.prototype, "offsetWidth", { configurable: true, get() { return Math.round(BOX.width); } });
Object.defineProperty(FakeNode.prototype, "offsetHeight", { configurable: true, get() { return Math.round(BOX.height); } });
FakeNode.prototype.getBoundingClientRect = function getBoundingClientRect() {
  return { left: 0, top: 0, width: BOX.width, height: BOX.height };
};
const transforms = [];
FakeNode.prototype.getContext = function getContext() {
  const g = { measureText: () => ({ width: 40 }) };
  for (const m of ["clearRect", "beginPath", "moveTo", "lineTo", "arc", "closePath", "fill", "stroke", "fillText"]) g[m] = () => {};
  g.setTransform = (...a) => transforms.push(a);
  return g;
};

const { render } = await import("../../web/js/panels/piechart.js");

function ctxOf(options) {
  const fields = computeFieldConfig(toFields([{ name: "a", kind: "number", values: [1] }, { name: "b", kind: "number", values: [3] }]), { defaults: {}, overrides: [] }, "piechart").fields;
  return { panel: { id: 1, title: "Pie", options }, fields, names: displayNames(fields), baseNames: baseDisplayNames(fields), zone: "utc" };
}

const OPTIONS = (over = {}) => ({
  pie_type: "pie",
  reduce: { calc: "last_not_null", fields: null, values: false },
  labels: [],
  legend: { show: true, placement: "right", mode: "list", values: [] },
  tooltip: { mode: "single" },
  ...over,
});

const canvasOf = (host) => host.querySelector("canvas");
const geometry = (host) => canvasOf(host).dataset.geometry.split(" ").map(Number);

test("D1: the circle comes from the real box with its fraction, not from the rounded client size (9.19a)", () => {
  BOX = { width: 333.6, height: 200.4 };
  const host = new FakeNode("div");
  render(host, ctxOf(OPTIONS()));
  const [cx, cy, radius, inner] = geometry(host);
  // 333.6 px wide and 200.4 px high; the old code used the rounded 334 and 200.
  assert.ok(Math.abs(cx - 333.6 / 2) < 0.02, `cx ${cx}`);
  assert.ok(Math.abs(cy - 200.4 / 2) < 0.02, `cy ${cy}`);
  assert.ok(Math.abs(radius - (200.4 / 2 - 8)) < 0.02, `radius ${radius}`);
  assert.equal(inner, 0);

});

test("D1: the donut hole is 0.55 of the radius at the same centre", () => {
  BOX = { width: 333.6, height: 200.4 };
  const host = new FakeNode("div");
  render(host, ctxOf(OPTIONS({ pie_type: "donut" })));
  const [, , radius, inner] = geometry(host);
  assert.ok(Math.abs(inner - radius * 0.55) < 0.002, `inner ${inner}`);
});

test("D1: the bitmap is not stretched: one unit of the drawing is one CSS px of the real box", () => {
  BOX = { width: 333.6, height: 200.4 };
  transforms.length = 0;
  const host = new FakeNode("div");
  render(host, ctxOf(OPTIONS()));
  const canvas = canvasOf(host);
  const [sx, , , sy] = transforms[0];
  // The bitmap has a whole number of pixels. The scale makes its size equal to the real box.
  assert.ok(Math.abs(canvas.width / sx - 333.6) < 0.02, `x ${canvas.width / sx}`);
  assert.ok(Math.abs(canvas.height / sy - 200.4) < 0.02, `y ${canvas.height / sy}`);
});

test("D1: a box with a whole size gives the same circle as before", () => {
  BOX = { width: 300, height: 200 };
  const host = new FakeNode("div");
  render(host, ctxOf(OPTIONS()));
  assert.deepEqual(geometry(host), [150, 100, 92, 0]);
});

test("D1: a box with no size draws no circle that the page can see and does not stop", () => {
  BOX = { width: 0, height: 0 };
  const host = new FakeNode("div");
  render(host, ctxOf(OPTIONS()));
  const [cx, cy, radius] = geometry(host);
  assert.equal(cx, 0);
  assert.equal(cy, 0);
  assert.equal(radius, 10);
});

test("D2: the pie has no default for a missing `legend` (4.26): the member is necessary", () => {
  BOX = { width: 300, height: 200 };
  const host = new FakeNode("div");
  const options = OPTIONS();
  delete options.legend;
  assert.throws(() => render(host, ctxOf(options)), TypeError);
  // The old code drew a legend at the right with no item rule of the document.
  assert.equal(host.querySelector(".pie__legend"), null);
});

test("D2: the pie draws no legend with `show` = false, and the placement gives the class", () => {
  BOX = { width: 300, height: 200 };
  for (const placement of ["right", "bottom"]) {
    const host = new FakeNode("div");
    render(host, ctxOf(OPTIONS({ legend: { show: true, placement, mode: "list", values: [] } })));
    assert.ok(host.querySelector(`.pie--${placement}`), placement);
    assert.ok(host.querySelector(".pie__legend"), placement);
  }
  const host = new FakeNode("div");
  render(host, ctxOf(OPTIONS({ legend: { show: false, placement: "right", mode: "list", values: [] } })));
  assert.equal(host.querySelector(".pie__legend"), null);
});
