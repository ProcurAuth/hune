// SPDX-License-Identifier: MIT OR Apache-2.0
// Contract of the dashboard grid and of the panel frame CSS (9.8, 9.11, 9.26). The geometry is read by the browser
// cases B02 and B27: these tests keep it from drifting. The page has no DOM here, so the CSS is read as text.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { readingOrder } from "../../web/js/grid.js";
import { csvCell, csvFileName, csvText, frameDataset, inspectAllowed, inspectModel } from "../../web/js/panel_frame.js";

const css = readFileSync(new URL("../../web/css/dashboard.css", import.meta.url), "utf8");
const tokens = readFileSync(new URL("../../web/css/tokens.css", import.meta.url), "utf8");
const vendorTokens = readFileSync(new URL("../../web/css/vendor/chdash/00-tokens.css", import.meta.url), "utf8");

// The rules of the frame: from the grid to the end of the file (the panel types have their own files).
const frameCss = css.slice(css.indexOf("/* Sections and grid"));

function rule(selector) {
  const at = frameCss.indexOf(`${selector} {`);
  assert.ok(at >= 0, `rule ${selector} exists`);
  return frameCss.slice(at, frameCss.indexOf("}", at));
}

test("grid: 24 columns, a row unit and a gap that come from the tokens of 9.8", () => {
  const grid = rule(".dash__grid");
  assert.match(grid, /repeat\(24, minmax\(0, 1fr\)\)/);
  assert.match(grid, /grid-auto-rows: var\(--grid-row\)/);
  assert.match(grid, /gap: var\(--grid-gap\)/);
  assert.match(tokens, /--grid-row: 30px/);
  assert.match(tokens, /--grid-gap: var\(--space-8\)/);
  assert.match(vendorTokens, /--space-8: 8px/);
});

test("frame CSS has no literal colour, no viewport sum of pixels and no hard-coded gap", () => {
  assert.doesNotMatch(frameCss, /#[0-9a-fA-F]{3,8}\b/);
  assert.doesNotMatch(frameCss, /rgba?\(/);
  assert.doesNotMatch(frameCss, /100vh - 220px/);
  assert.doesNotMatch(frameCss, /gap: \d+px/);
  assert.doesNotMatch(frameCss, /height: 32px/);
});

test("panel frame: no padding, so that the inner width is the width of the box minus the border (9.8)", () => {
  assert.doesNotMatch(rule(".panel"), /padding/);
});

test("the stacked layout is for screens up to 768 px only, never for print, and it wins over the inline grid numbers", () => {
  const at = frameCss.indexOf("@media screen and (max-width: 768px)");
  assert.ok(at >= 0, "the media query of narrow screens exists");
  const block = frameCss.slice(at, frameCss.indexOf("/* Print:", at));
  assert.match(block, /grid-column: 1 \/ -1 !important/);
  assert.match(block, /grid-row: auto !important/);
  assert.match(block, /var\(--panel-h, 4\)/);
  // no rule outside the media query changes the 24 columns
  assert.equal((frameCss.match(/grid-template-columns: minmax\(0, 1fr\);/g) || []).length, 1);
});

test("print: the dashboard has a landscape page, no panel menu, and a panel stays on one page", () => {
  assert.match(frameCss, /@page dashboard \{ size: A4 landscape/);
  const at = frameCss.indexOf("@media print");
  const block = frameCss.slice(at);
  assert.match(block, /\.panel__menu/);
  assert.match(block, /break-inside: avoid/);
});

test("the menu button shows on hover, on focus and while its menu is open, and always on a touch screen", () => {
  assert.match(frameCss, /\.panel__head \.panel__menu \{[^}]*opacity: 0;/);
  assert.match(frameCss, /\.panel:hover \.panel__menu[^{]*\.panel__menu:focus-visible[^{]*\.panel__menu\[aria-expanded="true"\] \{ opacity: 1; \}/);
  assert.match(frameCss, /@media \(hover: none\) \{ \.panel__menu \{ opacity: 1; \} \}/);
});

test("loading: the frame CSS has no animation, and the animations of the wait are in panel-loading.css and stop under reduced motion", () => {
  assert.equal((frameCss.match(/@keyframes/g) || []).length, 0);
  assert.doesNotMatch(frameCss, /\.panel__loader i|\.panel__skeleton/);
  const wait = readFileSync(new URL("../../web/css/panel-loading.css", import.meta.url), "utf8");
  assert.deepEqual([...wait.matchAll(/@keyframes ([\w-]+)/g)].map((m) => m[1]), ["panel-pulse", "panel-skel"]);
  assert.deepEqual(wait.match(/animation: [^;]*;/g).filter((a) => !/^animation: none;/.test(a) && !/^animation: panel-(pulse|skel) /.test(a)), []);
  assert.match(wait, /\.panel--wait > \.panel__head > \.panel__loader, \.panel--progress > \.panel__head > \.panel__loader \{ display: flex; \}/);
  assert.doesNotMatch(wait, /\.panel--progress \.panel__head::after|\.panel--wait \.panel__content:empty::before/);
});

test("readingOrder sorts by y, then x, then id, and keeps the input", () => {
  const panels = [
    { id: 5, grid: { x: 12, y: 0 } },
    { id: 2, grid: { x: 0, y: 8 } },
    { id: 4, grid: { x: 0, y: 0 } },
    { id: 3, grid: { x: 12, y: 0 } },
    { id: 1, grid: { x: 0, y: 8 } },
  ];
  assert.deepEqual(readingOrder(panels).map((p) => p.id), [4, 3, 5, 1, 2]);
  assert.deepEqual(panels.map((p) => p.id), [5, 2, 4, 3, 1]);
});

test("an empty overlay is not drawn, so that the skeleton of a loading panel shows", () => {
  assert.match(frameCss, /\.panel__overlay:empty \{ display: none; \}/);
});

test("tablet: two columns up to 768 px for a panel up to half of the grid, one column up to 480 px (F2)", () => {
  assert.match(frameCss, /@media screen and \(max-width: 768px\) \{\s*\.dash__grid \{ grid-template-columns: repeat\(2, minmax\(0, 1fr\)\)/);
  assert.match(frameCss, /\[data-span="half"\] \{ grid-column: span 1 !important; \}/);
  assert.match(frameCss, /@media screen and \(max-width: 480px\)/);
  const js = readFileSync(new URL("../../web/js/panel_frame.js", import.meta.url), "utf8");
  assert.match(js, /dataset\.span = g\.w <= 12 \? "half" : "full"/);
});

test("one inset for the title and the body, and one icon size in the title bar (F3, F4)", () => {
  assert.match(rule(".panel__head"), /padding: 0 var\(--space-8\) 0 var\(--space-12\)/);
  assert.match(rule(".panel__content"), /padding: var\(--panel-pad\);/);
  assert.match(frameCss, /\.panel__head \.icon \{ width: var\(--icon-md\); height: var\(--icon-md\); \}/);
});

test("print gives the range and the variable values as text, and drops controls (F1)", () => {
  const block = frameCss.slice(frameCss.indexOf("@media print"));
  assert.match(block, /\.dash__print-meta \{ display: block/);
  assert.match(block, /\.vars-bar, \.chartCore__legendMode \{ display: none !important; \}/);
  assert.match(block, /\.panel__title \{[^}]*white-space: normal/);
});

test("View data has one scroll area: the dialog is a column and the table region scrolls (F4)", () => {
  assert.match(frameCss, /\.dialog\[open\]:has\(\.view-data\) \{ display: flex; flex-direction: column; overflow: hidden; \}/);
  assert.match(frameCss, /\.view-data \{ flex: 1 1 auto; min-height: 0; max-height: none; overflow: auto; \}/);
});

test("Grafana geometry: the header of a section is 30 px with 8 px below, the panels are one gutter to the right (F3)", () => {
  const head = rule(".section__head");
  assert.match(head, /min-height: var\(--grid-row\)/);
  assert.match(head, /padding: var\(--space-4\) var\(--space-4\) var\(--space-4\) 0/);
  assert.match(head, /margin: 0 0 var\(--grid-gap\)/);
  assert.match(frameCss, /\.section__head ~ \.dash__grid \{ margin-left: var\(--space-24\); \}/);
  assert.match(frameCss, /\.section__head:has\(> \.section__toggle\[aria-expanded="false"\]\) \{ margin-bottom: 0; \}/);
  assert.match(rule(".section"), /margin-bottom: var\(--grid-gap\)/);
});

test("the title does not stretch: a spacer pushes the menu, and the info icon is a 32 px box after the title (F1)", () => {
  assert.match(rule(".panel__title"), /flex: 0 1 auto/);
  assert.match(frameCss, /\.panel__spacer \{ flex: 1 1 auto;/);
  assert.match(frameCss, /\.panel__head \.panel__menu \{[^}]*width: var\(--space-20\); height: var\(--space-24\)/);
});

test("the hook of a transparent panel exists and keeps the edge on hover (F6)", () => {
  assert.match(frameCss, /\.panel\[data-transparent\] \{ border-color: transparent; background: transparent; \}/);
  assert.match(frameCss, /\.panel\[data-transparent\]:hover[^{]*\{ border-color: var\(--border\); \}/);
});

test("the frame sets data-transparent exactly when the panel has transparent = true (4.13, X5)", () => {
  const base = { id: 7, type: "stat" };
  assert.deepEqual(frameDataset({ ...base, transparent: true }), { testid: "panel-7", state: "idle", type: "stat", transparent: "" });
  for (const value of [false, undefined, null, "true", 1]) {
    assert.deepEqual(frameDataset({ ...base, transparent: value }), { testid: "panel-7", state: "idle", type: "stat" }, String(value));
  }
  // The element uses this object for its dataset, so the attribute is present and empty: `.panel[data-transparent]`.
  const js = readFileSync(new URL("../../web/js/panel_frame.js", import.meta.url), "utf8");
  assert.match(js, /dataset: frameDataset\(panel, inst\),/);
});

test("a query error shows a warning icon left of the title with the message as tooltip, and the panel menu items have icons (F4, F5)", () => {
  const js = readFileSync(new URL("../../web/js/panel_frame.js", import.meta.url), "utf8");
  assert.match(js, /paintBadge\(message, `Error: \$\{message\}`\)/);
  assert.match(js, /statusIcon\.setAttribute\("data-tooltip", tip\)/);
  assert.match(js, /el\("header", \{ class: "panel__head" \}, statusIcon\)/);
  for (const name of ["eye", "table", "download"]) assert.match(js, new RegExp(`icon: "${name}"`));
});

// F21: the CSV of "View data" (9.11).
test("CSV: quote rule, row end, header row", () => {
  assert.equal(csvCell("plain"), "plain");
  assert.equal(csvCell('say "hi"'), '"say ""hi"""');
  assert.equal(csvCell("a,b"), '"a,b"');
  assert.equal(csvCell("line\nbreak"), '"line\nbreak"');
  const fields = [{ kind: "text" }, { kind: "number" }];
  const cells = [["a", "1.5"], ["b,c", "2"]];
  const text = csvText(["Name", "Value"], fields, (i, r) => cells[r][i], 2);
  assert.equal(text, 'Name,Value\r\na,1.5\r\n"b,c",2\r\n');
});

test("CSV: a text cell that starts with a formula character gets a quote prefix (D9)", () => {
  for (const c of ["=SUM(A1)", "+1+1", "-cmd|' /C calc'!A0", "@SUM(1)", "\tTAB", "\rCR"]) assert.ok(csvCell(c).replace(/^"/, "").startsWith("'"), JSON.stringify(c));
  assert.equal(csvCell("=1,2"), `"'=1,2"`);
  // The rule applies to the header too, and a quote prefix comes before the quote rule.
  assert.equal(csvText(["=x", "ok"], [{ kind: "text" }, { kind: "text" }], () => "v", 0), "'=x,ok\r\n");
  // Not at the start: no prefix.
  assert.equal(csvCell("a=b"), "a=b");
});

test("CSV: a number is written as a number, a text in a number field is not", () => {
  assert.equal(csvCell("-5", true), "-5");
  assert.equal(csvCell("-1.25 K", true), "-1.25 K");
  assert.equal(csvCell("+3", true), "+3");
  assert.equal(csvCell("-5.2%", true), "-5.2%");
  assert.equal(csvCell("-1.5 MB/s", true), "-1.5 MB/s");
  // The same text in a text field gets the prefix.
  assert.equal(csvCell("-5", false), "'-5");
  // A mapping text or a text with an operator in a number field is not a number.
  assert.equal(csvCell("-", true), "'-");
  assert.equal(csvCell("=SUM(A1)", true), "'=SUM(A1)");
  assert.equal(csvCell("-1+cmd|x", true), "'-1+cmd|x");
  assert.equal(csvCell("-5 =cmd", true), "'-5 =cmd");
});

test("CSV: the name of the file", () => {
  const at = new Date(Date.UTC(2026, 10, 9, 8, 5, 7));
  assert.equal(csvFileName("Latency of api", at), "Latency of api-data-20261109-080507.csv");
  assert.equal(csvFileName("a/b:c*d\u00e9", at), "a_b_c_d_-data-20261109-080507.csv");
  assert.equal(csvFileName("", at), "panel-data-20261109-080507.csv");
  assert.equal(csvFileName("x\ud83d\ude00y", at), "x_y-data-20261109-080507.csv");
});

test("variables bar: the rows of the list and the empty state follow the measures of wave VF; its row has no button", () => {
  assert.match(css, /\.vars-bar \.select__popup \{ min-width: 170px/);
  assert.match(css, /min-height: 38px/);
  // The controls of the page are in the header: the row under it holds the variables and the dashboard links (9.8a), and it does not show
  // without one of them.
  const components = readFileSync(new URL("../../web/css/components.css", import.meta.url), "utf8");
  assert.match(components, /\.dash-toolbar:not\(:has\(> \.vars-bar:not\(:empty\):not\(\.vars-bar--off\), > \.dash-links:not\(:empty\):not\(\.dash-links--off\)\)\) \{ display: none; \}/);
  assert.ok(!components.includes("dash-toolbar__actions"));
  assert.match(css, /\.empty__title \{[^}]*font-size: 18px/);
});

// F22: the item "Inspect" of the panel menu (9.11).
test("Inspect: the item shows only when the session says that the server allows it", () => {
  assert.equal(inspectAllowed({ query_inspect: true }), true);
  for (const session of [null, undefined, {}, { query_inspect: false }, { query_inspect: "true" }, { query_inspect: 1 }]) assert.equal(inspectAllowed(session), false, JSON.stringify(session));
  const js = readFileSync(new URL("../../web/js/panel_frame.js", import.meta.url), "utf8");
  // The item is behind the flag, and no other item uses the flag.
  assert.match(js, /\.\.\.\(inspectAllowed\(app\.session\) && !noted \? \[\{ icon: "code", label: "Inspect", onSelect: inspectQuery, testid: `panel-\$\{key\}-inspect` \}\] : \[\]\)/);
  assert.equal(js.split("inspectAllowed(app.session)").length - 1, 1);
});

test("Inspect: the model reads the member sql of the answer and nothing else is trusted", () => {
  const frame = { query_id: "0192abcd", interval_ms: 60000, rows: 2, sql: "SELECT 1\nWHERE x = 'a'" };
  assert.deepEqual(inspectModel(frame), { sql: "SELECT 1\nWHERE x = 'a'", queryId: "0192abcd", intervalMs: 60000, rows: 2 });
  // An answer with no sql member: the server did not give it.
  assert.equal(inspectModel({ query_id: "q", rows: 0 }).sql, null);
  // A member that is not a text counts as absent.
  for (const sql of [null, 5, {}, ["SELECT 1"], true]) assert.equal(inspectModel({ query_id: "q", sql }).sql, null, JSON.stringify(sql));
  // No answer yet.
  assert.equal(inspectModel(null), null);
  assert.equal(inspectModel(undefined), null);
  assert.equal(inspectModel("SELECT 1"), null);
  // A number member that is not an integer is left out.
  assert.deepEqual(inspectModel({ interval_ms: "60000", rows: 1.5, query_id: 7 }), { sql: null, queryId: "", intervalMs: null, rows: null });
});

test("Inspect: the SQL goes into the page as text, never as markup, and the dialog scrolls and wraps", () => {
  const js = readFileSync(new URL("../../web/js/panel_frame.js", import.meta.url), "utf8");
  const start = js.indexOf("function inspectQuery()");
  assert.ok(start > 0);
  const body = js.slice(start, js.indexOf("menuButton.addEventListener", start));
  assert.doesNotMatch(body, /innerHTML|insertAdjacentHTML|outerHTML/);
  assert.match(body, /text: model\.sql \}/);
  assert.match(css, /\.inspect__sql \{[^}]*overflow: auto;[^}]*white-space: pre-wrap;[^}]*overflow-wrap: anywhere/);
});

test("the frame shows a repeat note only for an object note; the text note of the history and of the editor keeps its hint (9.8c, 10.11)", () => {
  const js = readFileSync(new URL("../../web/js/panel_frame.js", import.meta.url), "utf8");
  assert.match(js, /if \(info\.p && info\.p\.note && typeof info\.p\.note === "object"\) \{\s*showNote\(info\.p\.note\);/);
  assert.match(js, /const noted = Boolean\(record\(\) && record\(\)\.note && typeof record\(\)\.note === "object"\);/);
  // The version runtime of the history gives a text note, and the hint shows it.
  const history = readFileSync(new URL("../../web/js/editor/history.js", import.meta.url), "utf8");
  assert.match(history, /const NOTE = "Data not shown";/);
  assert.match(js, /rec\.note \|\| "Run the query to see the data\."/);
});

test("a panel type that the page cannot draw yet shows a fixed message in the error state, never the text of an exception", () => {
  const js = readFileSync(new URL("../../web/js/panel_frame.js", import.meta.url), "utf8");
  const draw = js.slice(js.indexOf("async function draw(info)"), js.indexOf("function framesSelect"));
  // The check is the first step of draw(), before the data is prepared, and it uses the existing error state of the frame.
  assert.match(draw, /lastInfo = info;\s*\/\/[^\n]*\n\s*if \(!Object\.hasOwn\(LOADERS, panel\.type\)\) \{\s*showError\("This panel type is not available in this version of the page\.", null, info\.retryId\);\s*return;\s*\}/);
  assert.ok(draw.indexOf("Object.hasOwn(LOADERS") < draw.indexOf("preparePanelFrames"));
  // The text of an exception shows only for the loaders that exist: the call of a loader that does not exist is not reachable.
  assert.equal(draw.split("LOADERS[panel.type]()").length - 1, 1);
  // Each entry of LOADERS names a module that exists (a type that is added to the table has its module).
  const table = js.slice(js.indexOf("const LOADERS = {"), js.indexOf("};", js.indexOf("const LOADERS = {")));
  for (const m of table.matchAll(/import\("(\.\/panels\/[a-z_]+\.js)"\)/g)) assert.ok(readFileSync(new URL(`../../web/js/${m[1]}`, import.meta.url), "utf8").length > 0, m[1]);
});
