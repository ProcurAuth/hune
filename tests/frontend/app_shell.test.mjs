// SPDX-License-Identifier: MIT OR Apache-2.0
// The layout of the page (9.6): the header is on top at every width and the content scrolls in one container under it, so the scrollbar of the
// page starts under the header. The browser case B272 measures this in a real page. These tests check the files of web/ and the pure logic.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { scrollSpeed } from "../../web/js/editor/drag.js";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../..");
const read = (path) => readFileSync(join(ROOT, path), "utf8");
const code = (path) => read(path).replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");

test("the document does not scroll: the body fills the window and #app-main is the one scroll container", () => {
  const css = read("web/css/base.css");
  assert.match(css, /html, body \{ height: 100%; \}/);
  const body = css.match(/\nbody \{([^}]*)\}/)[1];
  assert.match(body, /height: 100dvh;/);
  assert.match(body, /overflow: hidden;/);
  assert.ok(!/min-height: 100vh/.test(body), "the body has no minimum height that lets the document grow");
  assert.match(css, /#app-main \{ position: relative; flex: 1 1 auto; min-width: 0; min-height: 0; overflow: auto; padding: var\(--page-pad\); \}/);
});

test("the header is always on top at every width and never scrolls away", () => {
  const css = read("web/css/base.css");
  assert.match(css, /#app-header \{\s*position: relative; flex: none; z-index: var\(--z-header\);/);
  assert.ok(!/#app-header \{ position: static; \}/.test(css), "no width lets the header scroll");
  assert.ok(!/position: sticky; top: 0; z-index: var\(--z-header\)/.test(css), "the header does not stick: it is not in the scroll container");
});

test("the variables row sticks at the top of the container, which is under the header", () => {
  const css = read("web/css/components.css");
  assert.match(css, /\.dash-toolbar \{\s*position: sticky; top: calc\(-1 \* var\(--page-pad\)\); z-index: var\(--z-sticky-top\);/);
  assert.ok(!/\.dash-toolbar \{[^}]*top: var\(--header-h\)/.test(css));
});

test("main.js measures the header and the scrollbar of the content, and keeps the scroll of each path for Back", () => {
  const js = code("web/js/main.js");
  assert.match(js, /setProperty\("--header-h", `\$\{headerEl\.offsetHeight\}px`\)/);
  assert.match(js, /setProperty\("--scrollbar-w", `\$\{Math\.max\(0, main\.offsetWidth - main\.clientWidth\)\}px`\)/);
  assert.match(js, /watch\.observe\(main\)/);
  assert.match(js, /scrollMemory\.set\(shownPath, main\.scrollTop\)/);
  assert.match(js, /main\.scrollTop = wantsRestore \? scrollMemory\.get\(window\.location\.pathname\) \|\| 0 : 0;/);
  assert.match(js, /window\.addEventListener\("popstate", \(\) => \{\s*restoreScroll = true;\s*show\(\);/);
});

test("what is fixed to the window edge leaves the scrollbar of the content free", () => {
  const css = read("web/css/editor.css");
  assert.match(css, /\.peditor__options \{ position: fixed; top: calc\(var\(--header-h\) \+ var\(--page-pad\)\); right: var\(--scrollbar-w, 0px\);/);
});

test("the lazy load of the panels and the drag scroll use the container, not the window", () => {
  assert.match(code("web/js/grid.js"), /root: document\.getElementById\("app-main"\), rootMargin: "100% 0px"/);
  const drag = code("web/js/editor/drag.js");
  assert.ok(!/window\.scrollBy|window\.scrollY/.test(drag), "no window scroll in drag.js");
  assert.match(drag, /scroller\.scrollTop = before \+ speed;/);
});

test("scrollSpeed: 0 in the middle, negative at the top edge zone, positive at the bottom edge zone, at most 24", () => {
  assert.equal(scrollSpeed(400, 44, 900), 0);
  assert.equal(scrollSpeed(44 + 56, 44, 900), 0, "the first pixel outside the zone");
  assert.equal(scrollSpeed(44, 44, 900), -24, "at the top edge of the container");
  assert.ok(scrollSpeed(60, 44, 900) < 0 && scrollSpeed(60, 44, 900) > -24);
  assert.equal(scrollSpeed(900, 44, 900), 24, "at the bottom edge");
  assert.equal(scrollSpeed(1200, 44, 900), 24, "below the container: never above the maximum");
  assert.equal(scrollSpeed(-50, 44, 900), -24, "above the container: never below the minimum");
  assert.equal(scrollSpeed(50, 44, 900), -Math.ceil(((44 + 56 - 50) / 56) * 24));
  assert.equal(scrollSpeed(20, 44, 900), -24, "a pointer on the header scrolls up at the maximum");
});
