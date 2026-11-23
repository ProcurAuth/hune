// The expand of a section (9.8, 10.2a rule 17; decision ID-K325, owner answer of 2026-11-22). A collapsed section is out of the layout, so
// the engine releases the canvases of its charts. The charts drew again only at the next resize callback, and a task between the expand and
// that callback read blank canvases. The grid now sends the event "hune-shown" at the end of the expand, after the section shows: each chart
// that had no width draws in that task (common.js fitHeight, heatmap.js, timeline.js, piechart.js).
import test from "node:test";
import assert from "node:assert/strict";
import { FakeNode, installFakeDom } from "./fake_dom.mjs";

const panel = (id, x) => ({ id, type: "timeseries", title: `Panel ${id}`, description: "", transparent: false, grid: { x, y: 0, w: 12, h: 6 }, transforms: [], links: [], fields: { defaults: {}, overrides: [] }, options: {}, data: { query: { source: "s", format: "table", sql: [`SELECT ${id}`], min_interval: null, max_data_points: null } } });
const doc = { title: "D", variables: [], sections: [{ title: "Open", collapsed: false, panels: [panel(1, 0)] }, { title: "Closed", collapsed: true, panels: [panel(2, 0), panel(3, 12)] }] };

test("the expand of a section sends hune-shown once the section shows, and a collapse sends nothing (9.8, 10.2a rule 17)", async () => {
  installFakeDom();
  const sent = [];
  let closed = null; // the grid element of the section "Closed"
  document.createComment = () => new FakeNode("#comment");
  document.dispatchEvent = (event) => sent.push(`${event.type}, hidden ${closed.hidden}`);
  globalThis.window = { addEventListener() {}, removeEventListener() {} };
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  FakeNode.prototype.replaceChild = function (node, old) {
    this.children[this.children.indexOf(old)] = node;
    node.parent = this;
  };
  const { createGrid } = await import("../../web/js/grid.js");
  const asked = [];
  const rt = { plan: null, expanded: new Map([[0, true], [1, false]]), setVisible() {}, setExpanded(key, value) { asked.push([key, value]); this.expanded.set(key, value); }, on: () => () => {} };
  const grid = createGrid({ doc, rt, makeFrame: () => ({ el: new FakeNode("section"), update() {}, destroy() {}, suspend() {}, resume() {} }) });
  const head = grid.el.querySelector('[data-testid="section-1"] > h2 > button');
  closed = grid.el.querySelector('[data-testid="section-1"] > div');
  assert.equal(closed.hidden, true, "the section is collapsed at the start");
  head.dispatch("click");
  assert.deepEqual(asked, [[1, true]]);
  assert.deepEqual(sent, ["hune-shown, hidden false"], "the event comes in the task of the expand, after the section shows");
  head.dispatch("click");
  assert.equal(closed.hidden, true);
  assert.deepEqual(sent, ["hune-shown, hidden false"], "a collapse sends no event");
  grid.setExpanded(1, true);
  assert.deepEqual(sent, ["hune-shown, hidden false", "hune-shown, hidden false"], "an expand by the page sends it too");
});
