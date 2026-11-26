// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-33 (9.19c, section 5, and T58): the legend, the card, the description and the message that panels/timeline.js builds, with a fake DOM.
// A hostile state text, row name or mapping text must stay a text node. No element, no attribute and no style comes from data.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { computeFieldConfig } from "../../web/js/fields.js";
import { baseDisplayNames } from "../../web/js/names.js";
import { toFields } from "./helpers.mjs";

const fe33 = JSON.parse(readFileSync(new URL("./vectors/FE-33-timeline-caps.json", import.meta.url)));

// --- a fake DOM: the elements, the attributes, the text nodes and the listeners that the panel uses ----------------------------------
class FNode {
  static width = 600;

  constructor(tag) {
    this.tag = tag;
    this.attrs = new Map();
    this.children = [];
    this.dataset = {};
    this.listeners = new Map();
    this.props = {};
    this.style = { setProperty: (k, v) => { this.props[k] = String(v); }, left: "", top: "" };
    this.hidden = false;
    this.clientWidth = FNode.width;
    this.clientHeight = 240;
    this.offsetWidth = 120;
    this.offsetHeight = 60;
    this.className = "";
    this.text = "";
  }

  set textContent(value) {
    this.children = value === "" ? [] : [Object.assign(new FNode("#text"), { text: String(value) })];
  }

  get textContent() {
    return this.tag === "#text" ? this.text : this.children.map((c) => c.textContent).join("");
  }

  set innerHTML(_) { throw new Error("innerHTML"); }
  set outerHTML(_) { throw new Error("outerHTML"); }
  setAttribute(name, value) { this.attrs.set(name, String(value)); }
  getAttribute(name) { return this.attrs.has(name) ? this.attrs.get(name) : null; }
  removeAttribute(name) { this.attrs.delete(name); }
  hasAttribute(name) { return this.attrs.has(name); }
  appendChild(child) { this.children.push(child); return child; }
  append(...nodes) { for (const n of nodes) this.children.push(typeof n === "string" ? Object.assign(new FNode("#text"), { text: n }) : n); }
  replaceChildren(...nodes) { this.children = []; this.append(...nodes); }
  remove() {}
  addEventListener(name, fn) { this.listeners.set(name, fn); }
  focus() {}
  getBoundingClientRect() { return { left: 0, top: 0, width: this.clientWidth, height: this.clientHeight }; }
  getContext() {
    const calls = [];
    const ctx = new Proxy({ calls }, {
      get: (t, k) => (k === "measureText" ? (s) => ({ width: 7 * String(s).length }) : k in t ? t[k] : (...a) => calls.push([k, ...a])),
      set: (t, k, v) => { t[k] = v; return true; },
    });
    this.ctx = ctx;
    return ctx;
  }
}

const walk = (root) => {
  const out = [];
  const visit = (n) => {
    out.push(n);
    n.children.forEach(visit);
  };
  visit(root);
  return out;
};

const ALLOWED = new Set(["div", "p", "ul", "li", "span", "b", "i", "canvas", "#text"]);
const ATTRS = new Set(["role", "tabindex", "aria-label", "aria-describedby", "id", "hidden", "data-fallback"]);

function install() {
  const root = new FNode("html");
  globalThis.Node = FNode;
  globalThis.document = {
    createElement: (tag) => new FNode(tag),
    createTextNode: (text) => Object.assign(new FNode("#text"), { text: String(text) }),
    getElementById: () => null,
    addEventListener() {},
    removeEventListener() {},
    documentElement: Object.assign(root, { dataset: { theme: "dark" } }),
    fonts: { addEventListener() {}, removeEventListener() {} },
  };
  globalThis.window = { devicePixelRatio: 1, addEventListener() {}, removeEventListener() {}, matchMedia: () => ({ matches: false }) };
  globalThis.getComputedStyle = () => ({ fontFamily: "Inter, sans-serif", getPropertyValue: (name) => ({ "--text": "#ccccdc", "--panel": "#181b1f" })[name] || "#888888" });
  globalThis.ResizeObserver = class { observe() {} disconnect() {} };
}

function uninstall() {
  for (const k of ["Node", "document", "window", "getComputedStyle", "ResizeObserver"]) delete globalThis[k];
}

// Draw a panel with `texts` as the states of one row (a mapping text for each value) and `name` as the row name.
async function draw({ texts, name = "row", type = "state_timeline", options = {}, width = 600, setState = null }) {
  const { render } = await import("../../web/js/panels/timeline.js");
  const n = texts.length;
  const mappings = texts.map((t, i) => ({ kind: "value", value: String(i), text: t, color: i % 2 ? "yellow" : "green" }));
  const cols = [{ name: "t", kind: "time", values: Array.from({ length: n }, (_, i) => 1000 + i * 60) }, { name, kind: "number", values: Array.from({ length: n }, (_, i) => i) }];
  const { fields } = computeFieldConfig(toFields(cols), { defaults: { mappings } }, type);
  const panel = { id: 7, type, title: "T", options: { merge_values: true, show_values: "auto", align_values: "left", row_height: 90, col_width: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single", sort: "none" }, ...options } };
  FNode.width = width;
  const host = new FNode("div");
  const handle = render(host, { panel, fields, names: baseDisplayNames(fields), zone: "utc", linkContext: () => ({ from: 1_000_000, to: 1_000_000 + n * 60_000 }), setState });
  return { host, handle, root: host.children[0] };
}

const checkTree = (root) => {
  for (const node of walk(root)) {
    assert.ok(ALLOWED.has(node.tag), `element ${node.tag}`);
    for (const name of node.attrs.keys()) assert.ok(ATTRS.has(name), `attribute ${name} on ${node.tag}`);
    // The style of an element is a size of the page or a colour of a state: never a text of the data.
    for (const [k, v] of Object.entries(node.props)) assert.ok(k === "--legend-max" ? /^\d+px$/.test(v) : k === "background" && /^#[0-9a-fA-F]{6}$/.test(v), `style ${k}: ${v}`);
  }
};

const find = (root, testid) => walk(root).find((n) => n.dataset.testid === testid);
const textNodes = (root) => walk(root).filter((n) => n.tag === "#text").map((n) => n.text);

test("FE-33 has 30 hostile state texts", () => {
  assert.equal(fe33.hostile.length, 30);
});

test("FE-33 a hostile state text stays a text node in the legend, the card and the description", async () => {
  install();
  try {
    for (const type of ["state_timeline", "status_history"]) {
      for (const text of fe33.hostile) {
        const { root, host } = await draw({ texts: [text, "other", text], type, options: { tooltip: { mode: "multi", sort: "none" }, legend: { show: true, placement: "bottom", mode: "list", values: [] } } });
        const legend = find(root, "timeline-7-legend");
        // A state timeline has a continuous mode by default and no legend. A status history lists the steps of its thresholds.
        assert.equal(legend.hidden, type === "state_timeline");
        if (type === "status_history") assert.deepEqual(legend.children.map((li) => li.children[1].children[0].text), ["< 80", "80+"]);
        const desc = find(root, "timeline-7-desc");
        assert.ok(desc.textContent.includes(text.slice(0, 40)) || text === "\u0000" || text.includes("\u0000"), `${type}: the description names the state`);
        checkTree(root);
        // The pointer card: move over the middle of the first box of the row.
        const canvas = walk(root).find((n) => n.tag === "canvas");
        const view = JSON.parse(host.dataset.timeline);
        const [, x, w] = view.segments[0];
        canvas.listeners.get("pointermove")({ clientX: x + w / 2, clientY: view.rows[0].y + view.rows[0].h / 2 });
        const tip = find(root, "timeline-7-tip");
        assert.equal(tip.hidden, false, "the card shows");
        checkTree(tip);
        assert.ok(textNodes(tip).includes(text), `${type}: the card shows the state as a text node`);
      }
    }
  } finally {
    uninstall();
  }
});

test("FE-33 the legend of a mode with states shows the hostile texts as text nodes", async () => {
  install();
  try {
    const { render } = await import("../../web/js/panels/timeline.js");
    const n = fe33.hostile.length;
    const cols = [{ name: "t", kind: "time", values: Array.from({ length: n }, (_, i) => 1000 + i * 60) }, { name: "r", kind: "text", values: fe33.hostile }];
    const { fields } = computeFieldConfig(toFields(cols), { defaults: { color: { mode: "fixed", fixed: "blue" } } }, "state_timeline");
    const panel = { id: 7, type: "state_timeline", title: "<img onerror=alert(1)>", options: { merge_values: true, show_values: "always", align_values: "center", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend: { show: true, placement: "right", mode: "list", values: [] }, tooltip: { mode: "single", sort: "none" } } };
    const host = new FNode("div");
    render(host, { panel, fields, names: ["<script>"], zone: "utc", linkContext: () => ({ from: 1_000_000, to: 1_000_000 + n * 60_000 }) });
    const root = host.children[0];
    checkTree(root);
    const legend = find(root, "timeline-7-legend");
    assert.equal(legend.children.length, n, "one item for each state");
    const shown = legend.children.map((li) => li.children[1].children[0].text);
    assert.deepEqual(shown, fe33.hostile);
    assert.equal(walk(root).filter((x) => x.tag === "canvas")[0].attrs.get("aria-label"), "<img onerror=alert(1)>, state timeline");
    // Each text on the canvas goes through fillText: no call builds markup.
    const calls = walk(root).find((x) => x.tag === "canvas").ctx.calls.filter((c) => c[0] === "fillText");
    assert.ok(calls.length > 0);
  } finally {
    uninstall();
  }
});

test("FE-33 a hostile row name stays a text node in the description and the card", async () => {
  install();
  try {
    for (const name of fe33.hostile.slice(0, 24)) {
      const { root, host } = await draw({ texts: ["a", "b"], name, options: { tooltip: { mode: "multi", sort: "none" } }, width: 400 });
      checkTree(root);
      assert.ok(find(root, "timeline-7-desc").textContent.startsWith(`${name}: `), JSON.stringify(name));
      void host;
    }
  } finally {
    uninstall();
  }
});

test("FE-33 a panel above the cap draws no canvas call and shows its message with role note", async () => {
  install();
  try {
    const states = [];
    const texts = Array.from({ length: 10001 }, (_, i) => `s${i % 2}`);
    const { root, host, handle } = await draw({ texts, setState: (s) => states.push(s) });
    const note = find(root, "timeline-7-note");
    assert.equal(note.hidden, false);
    assert.equal(note.attrs.get("role"), "note");
    assert.equal(note.textContent, fe33.caps[2].message);
    assert.equal(find(root, "timeline-7-legend").hidden, true);
    assert.equal(walk(root).find((n) => n.tag === "canvas").ctx, undefined, "no canvas call: the page never gets the context");
    assert.equal(host.dataset.timeline, undefined);
    assert.equal(handle.state, "timeline-limit");
    assert.deepEqual(states, ["timeline-limit"]);
    // Below the cap the same code draws.
    const ok = await draw({ texts: texts.slice(0, 10000) });
    assert.equal(find(ok.root, "timeline-7-note").hidden, true);
    assert.equal(JSON.parse(ok.host.dataset.timeline).rows[0].n, 10000);
  } finally {
    uninstall();
  }
});

test("FE-33 a status history above half the width shows the message and draws again at a new width", async () => {
  install();
  try {
    const texts = Array.from({ length: 492 }, (_, i) => `s${i % 3}`);
    const states = [];
    const { root, host, handle } = await draw({ texts, type: "status_history", width: 982, setState: (s) => states.push(s) });
    assert.equal(find(root, "timeline-7-note").textContent, fe33.width[1].message);
    assert.equal(handle.state, "timeline-limit");
    const pass = await draw({ texts: texts.slice(0, 491), type: "status_history", width: 982 });
    assert.equal(find(pass.root, "timeline-7-note").hidden, true);
    assert.ok(JSON.parse(pass.host.dataset.timeline).segments.length > 0);
    void host;
  } finally {
    uninstall();
  }
});

test("FE-33 the keys move the cursor over the boxes and Escape hides the card", async () => {
  install();
  try {
    const { root, host } = await draw({ texts: ["one", "two", "three"], options: { merge_values: false } });
    const canvas = walk(root).find((n) => n.tag === "canvas");
    const key = (k) => canvas.listeners.get("keydown")({ key: k, preventDefault() {} });
    const tip = find(root, "timeline-7-tip");
    key("ArrowRight");
    assert.equal(tip.hidden, false);
    assert.ok(textNodes(tip).includes("one"));
    key("ArrowRight");
    assert.ok(textNodes(tip).includes("two"));
    key("ArrowRight");
    key("ArrowRight");
    assert.ok(textNodes(tip).includes("three"), "the cursor stops at the last box");
    key("ArrowLeft");
    assert.ok(textNodes(tip).includes("two"));
    assert.ok(textNodes(tip).includes("Duration"), "a state timeline card has a duration row");
    key("Escape");
    assert.equal(tip.hidden, true);
    assert.ok(JSON.parse(host.dataset.timeline).segments.length === 3);
  } finally {
    uninstall();
  }
});

// --- the controls of the editor (10.3, 4.33b) -------------------------------------------------------------------------------------

test("the groups of the editor have one control for each option, with the hooks of 10.3, and write only valid members", async () => {
  install();
  try {
    const { createScope } = await import("../../web/js/editor/forms/controls.js");
    const { timelineControls, colourNote } = await import("../../web/js/editor/forms/timeline_form.js");
    const { newOptions } = await import("../../web/js/editor/draft.js");
    const ids = (nodes) => nodes.flatMap(walk).map((n) => n.dataset.testid).filter(Boolean);
    for (const type of ["state_timeline", "status_history"]) {
      let options = newOptions(type);
      const patches = [];
      const scope = createScope();
      const c = timelineControls(scope, { type, get: () => options, set: (patch) => { patches.push(patch); options = { ...options, ...patch }; } });
      const main = ids(c.main);
      const want = type === "state_timeline"
        ? ["timeline-merge", "timeline-show-values", "timeline-align", "timeline-row-height", "timeline-line-width", "timeline-fill-opacity", "timeline-span-nulls", "timeline-labels"]
        : ["timeline-show-values", "timeline-row-height", "timeline-col-width", "timeline-line-width", "timeline-fill-opacity", "timeline-labels"];
      assert.deepEqual(main, want, type);
      assert.deepEqual(ids(c.legend), ["timeline-legend-show", "timeline-legend-placement"]);
      assert.deepEqual(ids(c.tooltip), ["timeline-tooltip-mode"]);
      const at = (testid) => [...c.main, ...c.legend, ...c.tooltip].flatMap(walk).find((n) => n.dataset.testid === testid);
      // A number field takes an integer in its range only: 9 gives no patch, 10 and 100 do.
      const row = at("timeline-row-height");
      for (const [text, valid] of [["9", false], ["101", false], ["10.5", false], ["10", true], ["100", true]]) {
        const before = patches.length;
        row.value = text;
        row.listeners.get("input")();
        scope.flush();
        assert.equal(patches.length - before, valid ? 1 : 0, `${type} row_height ${text}`);
      }
      assert.deepEqual(patches.at(-1), { row_height: 100 });
      const width = at("timeline-line-width");
      width.value = "11";
      width.listeners.get("input")();
      scope.flush();
      assert.notDeepEqual(patches.at(-1), { line_width: 11 }, "line_width 11 is refused");
      width.value = "10";
      width.listeners.get("input")();
      scope.flush();
      assert.deepEqual(patches.at(-1), { line_width: 10 });
      // The segmented controls and the switches.
      const pick = (testid, label) => walk(at(testid)).find((n) => n.tag === "button" && n.children[0] && n.children[0].textContent === label).listeners.get("click")();
      pick("timeline-show-values", "Never");
      assert.deepEqual(patches.at(-1), { show_values: "never" });
      pick("timeline-legend-placement", "Right");
      assert.deepEqual(patches.at(-1), { legend: { show: true, placement: "right", mode: "list", values: [] } }, "the legend keeps mode list and values []");
      pick("timeline-tooltip-mode", "All");
      assert.deepEqual(patches.at(-1), { tooltip: { mode: "multi", sort: "none" } });
      at("timeline-labels").listeners.get("change")({ target: { checked: false } });
      assert.deepEqual(patches.at(-1), { show_labels: false });
      at("timeline-legend-show").listeners.get("change")({ target: { checked: false } });
      assert.equal(patches.at(-1).legend.show, false);
      if (type === "state_timeline") {
        at("timeline-merge").listeners.get("change")({ target: { checked: false } });
        assert.deepEqual(patches.at(-1), { merge_values: false });
        pick("timeline-align", "Center");
        assert.deepEqual(patches.at(-1), { align_values: "center" });
        at("timeline-span-nulls").listeners.get("change")({ target: { checked: true } });
        assert.deepEqual(patches.at(-1), { span_nulls: true });
      }
      for (const patch of patches) for (const key of Object.keys(patch)) assert.ok(Object.hasOwn(newOptions(type), key), `${type}: ${key} is a member of the type`);
    }
    assert.match(colourNote().textContent, /value mapping gives the colour of a state/);
  } finally {
    uninstall();
  }
});

// Security review of the page: a panel above the cap builds no legend before the check of the cap.
test("20,000 distinct states with a fixed colour give timeline-limit and no legend item", async () => {
  install();
  try {
    const { render } = await import("../../web/js/panels/timeline.js");
    const n = 20000;
    const cols = [{ name: "t", kind: "time", values: Array.from({ length: n }, (_, i) => 1000 + i * 60) }, { name: "r", kind: "text", values: Array.from({ length: n }, (_, i) => `s${i}`) }];
    const { fields } = computeFieldConfig(toFields(cols), { defaults: { color: { mode: "fixed", fixed: "blue" } } }, "state_timeline");
    const panel = { id: 7, type: "state_timeline", title: "T", options: { merge_values: true, show_values: "auto", align_values: "left", row_height: 90, line_width: 0, fill_opacity: 70, span_nulls: false, show_labels: true, legend: { show: true, placement: "bottom", mode: "list", values: [] }, tooltip: { mode: "single", sort: "none" } } };
    const host = new FNode("div");
    const handle = render(host, { panel, fields, names: ["r"], zone: "utc", linkContext: () => ({ from: 1_000_000, to: 1_000_000 + n * 60_000 }) });
    assert.equal(handle.state, "timeline-limit");
    const legend = find(host.children[0], "timeline-7-legend");
    assert.equal(legend.children.length, 0);
    assert.equal(legend.hidden, true);
  } finally {
    uninstall();
  }
});

// 10.2a, rule 17: the grid leaves the page while the panel editor shows, and the resize callback draws the canvas at its least
// size. When the page shows the grid again, it sends "hune-shown" before it writes the hook of the state: the timeline draws in that task.
test("a timeline that returns to the page draws in the task of the event hune-shown, before any resize callback", async () => {
  install();
  const on = new Map();
  document.addEventListener = (type, fn) => on.set(type, fn);
  document.removeEventListener = (type, fn) => on.get(type) === fn && on.delete(type);
  let resize = null;
  globalThis.ResizeObserver = class {
    constructor(callback) { resize = callback; }
    observe() {}
    disconnect() {}
  };
  try {
    const { root, handle } = await draw({ texts: ["up", "down", "up"] });
    const main = root.children[0];
    const canvas = main.children[0];
    assert.equal(canvas.width, 600);
    main.clientWidth = 0;
    resize();
    assert.equal(canvas.width, 1, "out of the page, the canvas has its least size");
    main.clientWidth = 600;
    assert.equal(typeof on.get("hune-shown"), "function", "the timeline listens to hune-shown");
    on.get("hune-shown")();
    assert.equal(canvas.width, 600, "the canvas has its width again in the task of the event");
    handle.destroy();
    assert.equal(on.has("hune-shown"), false, "the destroy removes the listener");
  } finally {
    uninstall();
  }
});

// A context that records the font of each `fillText` and measures a text in proportion to the font size: 0.5 unit for each character and each pixel.
function recordFills(fills) {
  const proto = FNode.prototype;
  const original = proto.getContext;
  proto.getContext = function getContext() {
    const ctx = new Proxy({ calls: [], font: "10px sans-serif" }, {
      get: (t, k) => (k === "measureText" ? (s) => ({ width: String(s).length * 0.5 * parseFloat(/(\d+(?:\.\d+)?)px/.exec(t.font)[1]) }) : k === "fillText" ? (text) => fills.push([text, t.font]) : k in t ? t[k] : () => {}),
      set: (t, k, v) => { t[k] = v; return true; },
    });
    this.ctx = ctx;
    return ctx;
  };
  return () => { proto.getContext = original; };
}

// 9.19c, section 6: the texts are 12 CSS px high. The canvas has device pixels and no transform, so the font is 12 * dpr px.
// The places stay the same at each pixel ratio, because the widths are measured at 12 units.
test("9.19c the texts of the canvas are 12 CSS px high at every pixel ratio, and the places do not change", async () => {
  const seen = {};
  for (const dpr of [1, 1.25, 2]) {
    install();
    window.devicePixelRatio = dpr;
    const fills = [];
    const restore = recordFills(fills);
    try {
      const { host } = await draw({ texts: ["up", "down", "up"], name: "a row name", options: { show_values: "always" } });
      const sizes = new Set(fills.map(([, font]) => parseFloat(/(\d+(?:\.\d+)?)px/.exec(font)[1])));
      assert.deepEqual([...sizes], [12 * dpr], `dpr ${dpr}: every text has the size 12 * dpr`);
      assert.ok(fills.some(([text]) => text === "a row name") && fills.some(([, font]) => font.startsWith("500 ")), "the row name and a box text are drawn");
      seen[dpr] = JSON.parse(host.dataset.timeline).box;
    } finally {
      restore();
      uninstall();
    }
  }
  assert.deepEqual(seen[2], seen[1], "the plot box is the same at dpr 2");
  assert.deepEqual(seen[1.25], seen[1], "the plot box is the same at dpr 1.25");
  assert.equal(seen[1][0], Math.round(10 * 0.5 * 12 + 20.5), "the left edge is the width of the name at 12 px plus 20.5");
});

// 9.19c, section 6 and 9.29: the hook lists the ticks with a grid line and the boxes of the axis labels.
test("9.29 the hook data-timeline holds the ticks and the boxes of the labels of the time axis", async () => {
  install();
  try {
    const { host } = await draw({ texts: ["up", "down", "up"] });
    const hook = JSON.parse(host.dataset.timeline);
    const [L, top, plotW, plotH] = hook.box;
    assert.ok(hook.ticks.length > 0 && hook.labels.length > 0, "the range of three minutes has ticks and labels");
    for (const [v, x] of hook.ticks) {
      assert.ok(Number.isFinite(v) && x >= L - 0.5 && x <= L + plotW + 0.5, `the tick ${v} is in the plot`);
    }
    for (const [text, x, left, right, y] of hook.labels) {
      assert.equal(typeof text, "string");
      assert.ok(left < x && x < right, "the label has a box around x");
      assert.equal(y, top + plotH + 9, "the top of the glyph is 9 px under the plot");
    }
  } finally {
    uninstall();
  }
});
