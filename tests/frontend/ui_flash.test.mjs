// SPDX-License-Identifier: MIT OR Apache-2.0
// The rules of the worker ui-flash (9.6, 9.9, 9.11, 9.25): one stable render on a reload, a header with fixed buttons, no Share feature.
// The browser cases B58 and B59 check the behaviour in a real page. These tests check the files of web/ and the docs, so a change that
// breaks a rule fails before the browser suite runs.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../..");
const read = (path) => readFileSync(join(ROOT, path), "utf8");
const code = (path) => read(path).replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");

test("the shell holds its first paint, and boot_theme.js releases it after 2 s at the latest", () => {
  const shell = read("web/shell.html");
  assert.match(shell, /<html [^>]*data-theme="light"[^>]*data-hune-hold>/);
  // The theme script is a classic script and comes before the module (it sets data-theme before the first paint).
  assert.ok(shell.indexOf("js/boot_theme.js") < shell.indexOf('type="module"'));
  assert.ok(!/<script(?![^>]*\bsrc=)(?![^>]*type="application\/json")/.test(shell), "no inline script (9.4)");
  const boot = read("web/js/boot_theme.js");
  assert.match(boot, /HOLD_MS = 2000/);
  assert.match(boot, /removeAttribute\("data-hune-hold"\)/);
  const css = read("web/css/base.css");
  assert.match(css, /html\[data-hune-hold\] body > :not\(noscript\) \{ visibility: hidden; \}/);
});

test("main.js releases the first paint after the last route, and the pages of an error release it at once", () => {
  const js = code("web/js/main.js");
  assert.match(js, /function reveal\(\)/);
  assert.match(js, /removeAttribute\("data-hune-hold"\)/);
  // A route that another route replaced (the entry redirect of 9.7) does not release the paint.
  assert.match(js, /if \(token === routeToken\) reveal\(\);/);
  // The pages "not signed in" and "no answer" and a failed start release it.
  assert.ok((js.match(/\breveal\(\);/g) || []).length >= 4);
  // The page waits for the font and the sprite, and it measures the header before it reveals.
  assert.match(js, /Promise\.all\(\[fontWait, spriteReady\]\)/);
  assert.match(js, /if \(headerMeasure\) headerMeasure\(\);/);
  // The module of the route loads while the session request runs.
  assert.match(js, /function preloadRoute\(\)/);
});

test("the icons are inline symbols with a prefix, never an external reference", () => {
  const ui = code("web/js/ui.js");
  assert.match(ui, /SPRITE_PREFIX = "hune-icon-"/);
  assert.match(ui, /use\.setAttribute\("href", `#\$\{SPRITE_PREFIX\}\$\{name\}`\)/);
  assert.ok(!/icons\.svg/.test(ui), "ui.js does not name the file");
  const main = code("web/js/main.js");
  assert.match(main, /assetUrl\("icons\.svg"\)/);
  assert.match(main, /node\.setAttribute\("id", SPRITE_PREFIX \+ symbol\.getAttribute\("id"\)\)/);
});

test("the font does not swap after the first paint", () => {
  const css = read("web/css/fonts.css");
  assert.match(css, /font-display: block;/);
  assert.ok(!/font-display: swap/.test(css));
  assert.match(read("web/shell.html"), /rel="preload" href="static\/\{\{build\}\}\/fonts\/inter\/InterVariable\.woff2"/);
});

test("the header sticks at every width, and only a phone on its side lets it scroll", () => {
  const css = read("web/css/base.css");
  assert.match(css, /#app-header \{\s*position: sticky; top: 0;/);
  const media = [...css.matchAll(/@media \(max-width: 760px\)([^{]*)\{\s*#app-header \{ position: static; \}/g)];
  assert.equal(media.length, 1);
  assert.match(media[0][1], /max-height: 600px/);
});

test("the controls of the header have fixed widths in every state (9.9)", () => {
  const css = read("web/css/components.css");
  const picker = read("web/css/picker.css");
  assert.match(picker, /\.picker--time > \.picker__button \{ flex: none; width: var\(--tb-range-w\); \}/);
  assert.match(css, /\.picker__run \{ width: var\(--tb-icon-w\); padding: 0; \}/);
  assert.match(picker, /\.picker__interval \{ width: var\(--tb-int-w\);/);
  assert.ok(!/picker__interval:has\(\.picker__interval-text:empty\)/.test(css + picker), "the interval button has one width with and without text");
  // With no interval the button shows a dash before its chevron: the text is never empty.
  assert.match(read("web/js/pickers.js"), /text\.textContent = select\.value === "off" \? "-" : select\.value;/);
  assert.match(css, /#app-header \{ --tb-range-w: 220px; --tb-edit-w: 76px; --tb-add-w: 68px; --tb-addmenu-w: 28px; --tb-icon-w: 36px; --tb-int-w: 58px; \}/);
  assert.match(css, /\.edit-switch \{ flex: none; width: var\(--tb-edit-w\);/);
  assert.match(css, /\.add-main \{ width: var\(--tb-add-w\);/);
  assert.match(css, /\.add-menu \{ width: var\(--tb-addmenu-w\);/);
  assert.ok(!css.includes("--tb-save-w") && !css.includes("header-edit-slot"), "no reserved slot for Save and Revert");
  // The title bar has no spinner of its own: the loader is the three dots of the body (9.11, 9.26).
  assert.ok(!/panel__state|stateIcon/.test(read("web/css/dashboard.css") + code("web/js/panel_frame.js")), "no spinner in the title bar of a panel");
});

test("the edit mode has no row of its own and no reserved slot: its group comes after the switch in the controls row", () => {
  const dash = code("web/js/dashboard.js");
  assert.match(dash, /header\.setActions\(timeControl\.el, editSwitch\)/);
  assert.match(dash, /header\.setMore\(more\)/);
  assert.ok(!/setEditSlots|toolbarSlots|header-edit-row|editSlot/.test(dash));
  const main = code("web/js/main.js");
  assert.ok(!/header-edit-row|editRow|setEditSlots/.test(main), "no row of the edit mode");
  assert.match(main, /headerEl\.append\(brand, startSlot, placeNav, actionsSlot, themeSlot, moreSlot\)/);
  const css = read("web/css/base.css");
  assert.ok(!/\.header-edit(?!-slot)|data-layout="stacked"|"edit edit edit edit"/.test(css));
  assert.ok(!/\.header-edit(?!-slot)|\.header-keep|\.header-slot/.test(read("web/css/components.css")));
});

test("a panel keeps its frame and shows no loader or preview before 300 ms, and a reload never dims (9.11)", () => {
  const js = code("web/js/panel_frame.js");
  assert.match(js, /const PROGRESS_MS = 300;/);
  assert.match(js, /const WAIT_MS = 300;/);
  assert.match(js, /classList\.add\("panel--wait"\)/);
  assert.match(js, /classList\.add\("panel--progress"\)/);
  const css = read("web/css/panel-loading.css");
  // The content of a reload stays as it is: no dim rule, in any file of the page.
  for (const file of ["web/css/dashboard.css", "web/css/panel-loading.css", "web/css/components.css", "web/css/base.css"]) {
    assert.ok(!/\.panel--progress[^{]*\.panel__content[^{]*\{[^}]*opacity/.test(read(file)), `no dim of the content in ${file}`);
  }
  // The dots show 300 ms after the wait starts (wait) or after the loading starts (progress), and only then.
  assert.match(css, /\.panel__loader \{[^}]*display: none;/);
  assert.match(css, /\.panel--wait > \.panel__head > \.panel__loader, \.panel--progress > \.panel__head > \.panel__loader \{ display: flex; \}/);
  // The preview shows for a panel with nothing on the screen only (data-wait="first"), and after the wait class.
  assert.match(css, /\.panel__skeleton \{[^}]*display: none;/);
  assert.match(css, /\.panel--wait\[data-wait="first"\] \.panel__skeleton \{ display: block; \}/);
  // The "No data" text of a panel that reloads stays: the old rule that hid it is gone.
  assert.ok(!/visibility: hidden/.test(css), "no hidden overlay");
  const dash = read("web/css/dashboard.css");
  assert.ok(!/\.panel\[data-state="(queued|loading)"\] \.panel__content/.test(dash));
  assert.ok(!/\.panel--progress \.panel__head::after|\.panel--wait \.panel__content:empty::before/.test(css + dash), "no bar and no grey block");
});

test("the loader is three pulsing dots in the title bar, out of the flow, and the preview is out of the flow too (9.26)", () => {
  const js = code("web/js/panel_frame.js");
  assert.match(js, /el\("div", \{ class: "panel__loader", "aria-hidden": "true" \}, el\("i"\), el\("i"\), el\("i"\)\)/);
  assert.match(js, /head\.append\(loader\);/, "the dots are in the title bar");
  assert.match(js, /el\("div", \{ class: "panel__body" \}, content, skeleton, overlay\)/, "the overlay of a state covers the preview");
  assert.match(js, /el\("div", \{ class: "panel__skeleton", "aria-hidden": "true"/);
  const css = read("web/css/panel-loading.css");
  assert.match(css, /\.panel__loader \{ position: absolute; top: 0; bottom: 0; right: calc\(var\(--space-8\) \+ var\(--space-20\) \+ var\(--space-4\)\); display: none;[^}]*pointer-events: none; \}/);
  assert.match(css, /\.panel__loader i \{ width: 5px; height: 5px; border-radius: 50%; background: var\(--accent-fill\);[^}]*animation: panel-pulse var\(--dur-pulse\) ease-in-out infinite; \}/);
  assert.match(css, /\.panel__loader i:nth-child\(2\) \{ animation-delay: 0\.15s; \}/);
  assert.match(css, /\.panel__loader i:nth-child\(3\) \{ animation-delay: 0\.3s; \}/);
  assert.match(css, /@keyframes panel-pulse /);
  assert.match(css, /position: absolute; inset: var\(--panel-pad\); display: none; pointer-events: none;/);
  // Reduced motion: three still, dimmed dots and still shapes. Print: nothing shows.
  assert.match(css, /@media \(prefers-reduced-motion: reduce\) \{\s*\.panel__loader i \{ animation: none; opacity: 0\.6; transform: none; \}\s*\.panel__skeleton i \{ animation: none; opacity: 0\.8; \}\s*\}/);
  assert.match(css, /@media print \{ \.panel__skeleton \{ display: none !important; \} \}/);
  assert.match(read("web/css/dashboard.css").slice(read("web/css/dashboard.css").indexOf("@media print")), /\.panel__loader \{ display: none !important; \}/);
  // The CSS names tokens only: no colour literal.
  assert.ok(!/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/.test(css), "no colour literal");
  assert.match(read("web/shell.html"), /css\/panel-annotations\.css">\s*<link rel="stylesheet" href="static\/\{\{build\}\}\/css\/panel-loading\.css">/);
});

test("Hune has no Share feature: no button, no menu item, no copy code, no CSS, no route", () => {
  for (const file of ["web/js/dashboard.js", "web/js/panel_frame.js", "web/js/main.js", "web/js/config.js", "web/css/components.css", "web/css/dashboard.css", "web/css/base.css"]) {
    const src = code(file);
    assert.ok(!/share/i.test(src), `${file} still names the Share feature`);
  }
  assert.ok(!/absoluteUrl/.test(code("web/js/config.js")));
  assert.ok(!/absoluteUrl/.test(code("web/js/dashboard.js")));
  // The copy helper stays for "Copy SQL" of the Inspect dialog and for the editor.
  assert.match(code("web/js/panel_frame.js"), /copyText\(model\.sql\)/);
  // "View panel" is a URL state and stays.
  assert.match(code("web/js/panel_frame.js"), /label: "View"/);
  assert.match(code("web/js/dashboard.js"), /viewPanel: \(id\) => dispatch\("view-panel", id\)/);
  // The spec has no Share button any more.
  const spec = read("docs/04-frontend-editor.md");
  assert.ok(!/data-testid="share-button"/.test(spec));
  assert.ok(!/"Share"/.test(spec.replace(/Hune has no "Share" button and no "Share" menu item/, "").replace(/The menu has no "Share" item/, "")));
});

test("el() writes false for the enumerated attributes and skips false for the others", async () => {
  const made = [];
  globalThis.document = { createElement: () => { const attrs = new Map(); const node = { setAttribute: (k, v) => attrs.set(k, v), attrs, append() {}, addEventListener() {} }; made.push(node); return node; }, createTextNode: (t) => t };
  globalThis.Node = class {};
  const { el } = await import("../../web/js/ui.js");
  const node = el("textarea", { spellcheck: false, draggable: false, contenteditable: false, translate: false, "aria-hidden": false, title: "x" });
  assert.equal(node.attrs.get("spellcheck"), "false");
  assert.equal(node.attrs.get("draggable"), "false");
  assert.equal(node.attrs.get("contenteditable"), "false");
  assert.equal(node.attrs.get("translate"), "false");
  assert.equal(node.attrs.has("aria-hidden"), false);
  assert.equal(node.attrs.get("title"), "x");
  delete globalThis.document;
  delete globalThis.Node;
});
