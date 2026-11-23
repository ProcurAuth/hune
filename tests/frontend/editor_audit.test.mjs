// The bridge from the panel editor to the preview (10.3, 4.13): the switch "Transparent background" writes the draft, the draft gives the
// attribute of the frame, and no rule of the editor hides the effect (decision ID-K179, owner request of 2026-11-14).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PANEL_TYPES, updatePanel } from "../../web/js/editor/draft.js";
import { frameDataset } from "../../web/js/panel_frame.js";

const read = (path) => readFileSync(new URL(`../../web/${path}`, import.meta.url), "utf8");
const editorCss = read("css/editor.css");
const frameCss = read("css/dashboard.css");
const tableCss = read("css/panel-table.css");
const editorJs = read("js/editor/panel_editor.js");

/** The text of the rule of a selector (the first rule that starts with it), or "". */
const ruleOf = (css, selector) => {
  const at = css.indexOf(`${selector} {`);
  return at < 0 ? "" : css.slice(at, css.indexOf("}", at) + 1);
};

const docOf = (types) => ({
  sections: [{ title: null, collapsed: false, panels: types.map((type, i) => ({ id: i + 1, type, title: type, description: "", transparent: false, grid: { x: 0, y: i * 8, w: 24, h: 8 } })) }],
});

test("the draft carries the switch: transparent true then false, for each of the 11 types, and only on the panel that changes", () => {
  assert.equal(PANEL_TYPES.length, 15);
  let doc = docOf(PANEL_TYPES);
  PANEL_TYPES.forEach((type, i) => {
    const id = i + 1;
    doc = updatePanel(doc, id, (x) => ({ ...x, transparent: true }));
    const panels = doc.sections[0].panels;
    assert.equal(panels[i].transparent, true, `${type}: the draft has transparent = true`);
    // The frame of the preview reads this record: the attribute is there for this panel only.
    assert.deepEqual(frameDataset(panels[i]), { testid: `panel-${id}`, state: "idle", type, transparent: "" }, type);
    for (let k = i + 1; k < panels.length; k++) assert.equal("transparent" in frameDataset(panels[k]), false, `${type}: the panel ${k + 1} keeps its fill`);
  });
  PANEL_TYPES.forEach((type, i) => {
    doc = updatePanel(doc, i + 1, (x) => ({ ...x, transparent: false }));
    assert.equal("transparent" in frameDataset(doc.sections[0].panels[i]), false, `${type}: the attribute leaves with the switch`);
  });
});

test("the editor writes the switch to the draft and draws the preview again from the draft", () => {
  // The switch applies `transparent: checked` through apply(), which changes the draft and then calls drawTop().
  assert.match(editorJs, /testid: "panel-transparent",\s*onChange: \(checked\) => apply\(\(x\) => \(\{ \.\.\.x, transparent: checked \}\)\)/);
  const apply = editorJs.slice(editorJs.indexOf("function apply(fn"), editorJs.indexOf("function drawTop()"));
  assert.match(apply, /E\.change\(doc\);\s*drawTop\(\);/);
  // drawTop() builds a new frame from the panel of the draft (not from the saved panel) and marks it as the preview.
  const draw = editorJs.slice(editorJs.indexOf("function drawTop()"), editorJs.indexOf("const onPanel"));
  assert.match(draw, /createPanelFrame\(\{ panel: p, /);
  assert.match(draw, /const p = panel\(\);/);
  assert.match(draw, /classList\.add\("panel--view", "panel--editing"\)/);
  // The switch is in the group "Panel options" of the type text and of the other types.
  assert.equal(editorJs.split('optionGroup({ key: "panel", title: "Panel options"').length - 1, 2);
});

test("no rule of the editor gives the preview a fill or an edge of its own (the root cause of ID-K179)", () => {
  // The rule of the preview frame keeps the place only: the fill, the edge and the overlay come from the frame (`.panel`), also in the editor.
  const rule = ruleOf(editorCss, ".peditor__top .panel.panel--editing");
  assert.match(rule, /position: absolute; inset: 0; height: auto;/);
  assert.doesNotMatch(rule, /background|border/);
  assert.doesNotMatch(editorCss, /\.peditor__top[^{]*\{[^}]*(background|border-color)\s*:\s*transparent/);
  assert.doesNotMatch(editorCss, /panel--editing[^{]*\{[^}]*(background|border-color)\s*:/);
  // The frame has the fill and the edge of the theme, and the hook of a transparent panel removes them.
  assert.match(ruleOf(frameCss, ".panel"), /border: 1px solid var\(--border\);[^}]*background: var\(--surface-panel\);/);
  assert.match(frameCss, /\.panel\[data-transparent\] \{ border-color: transparent; background: transparent; \}/);
  assert.match(frameCss, /\.panel\[data-transparent\] \.panel__overlay[^{]*\{ background: transparent; \}/);
  // A table keeps the sticky header on the page colour, and its cells do not paint over a transparent frame.
  assert.match(tableCss, /\.panel\[data-transparent\] \.vtable > thead > tr > th \{ background: var\(--bg\); \}/);
  assert.match(tableCss, /\.panel\[data-transparent\] \.vtable > tbody > tr > td:not\(\.cell--bg\) \{ background: transparent; \}/);
});

test("the title bar and the menu of a frame have no fill of their own that a transparent frame would keep", () => {
  for (const selector of [".panel__head", ".panel__title", ".panel__body", ".panel__content"]) {
    const at = frameCss.indexOf(`${selector} {`);
    if (at < 0) continue;
    assert.doesNotMatch(frameCss.slice(at, frameCss.indexOf("}", at)), /background/, selector);
  }
});
