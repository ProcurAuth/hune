// The header bar (9.6) and the theme selector (9.25): the logic with no DOM, the boot script in a fake window, the boot block and the css rules.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { THEME_CHOICES, THEME_MOTION_MS, buttonLabel, menuTarget, normalizeChoice, resolveTheme } from "../../web/js/theme.js";
import { STEPS, maxQuery, stepsAt } from "../../web/js/header_layout.js";
import { parseBoot } from "../../web/js/config.js";

const read = (path) => readFileSync(new URL(`../../web/${path}`, import.meta.url), "utf8");

test("theme choices: the order of the menu, the default and the resolved theme", () => {
  assert.deepEqual(THEME_CHOICES, ["system", "dark", "light"]);
  for (const bad of [null, undefined, "", "auto", "Dark", 3, {}]) assert.equal(normalizeChoice(bad), "system", String(bad));
  for (const good of THEME_CHOICES) assert.equal(normalizeChoice(good), good);
  assert.equal(resolveTheme("system", true), "dark");
  assert.equal(resolveTheme("system", false), "light");
  assert.equal(resolveTheme("light", true), "light");
  assert.equal(resolveTheme("dark", false), "dark");
  assert.equal(resolveTheme("nonsense", true), "dark", "a bad value is the choice system");
  assert.equal(buttonLabel("dark"), "Theme: Dark");
  assert.equal(buttonLabel("x"), "Theme: System");
});

test("theme menu keys: arrows wrap, Home and End go to the ends, other keys do not move", () => {
  assert.equal(menuTarget("ArrowDown", -1, 3), 0);
  assert.equal(menuTarget("ArrowDown", 0, 3), 1);
  assert.equal(menuTarget("ArrowDown", 2, 3), 0);
  assert.equal(menuTarget("ArrowUp", -1, 3), 2);
  assert.equal(menuTarget("ArrowUp", 0, 3), 2);
  assert.equal(menuTarget("Home", 2, 3), 0);
  assert.equal(menuTarget("End", 0, 3), 2);
  assert.equal(menuTarget("a", 0, 3), -1);
  assert.equal(menuTarget("ArrowDown", 0, 0), -1);
});

test("the motion of the menu is the token --dur-base and stays under the 200 ms of 9.26", () => {
  assert.ok(THEME_MOTION_MS > 0 && THEME_MOTION_MS < 200);
  assert.match(read("css/vendor/chdash/00-tokens.css"), new RegExp(`--dur-base: ${THEME_MOTION_MS}ms`));
});

// Runs boot_theme.js in a fake window and gives the pieces that a test needs.
function boot({ stored = undefined, dark = false, blocked = false } = {}) {
  const attrs = new Map();
  const listeners = { document: new Map(), window: new Map() };
  const store = new Map(stored === undefined ? [] : [["hune.theme", stored]]);
  const media = { matches: dark, listeners: [], addEventListener(type, fn) { this.listeners.push(fn); } };
  const on = (bucket) => (type, fn) => bucket.set(type, [...(bucket.get(type) || []), fn]);
  const window = {
    localStorage: { getItem: (k) => { if (blocked) throw new Error("blocked"); return store.has(k) ? store.get(k) : null; }, setItem: (k, v) => store.set(k, v) },
    matchMedia: () => media,
    addEventListener: on(listeners.window),
    // The hold release of boot_theme.js (9.6): the fake timer keeps its function, a test runs it with `release()`.
    setTimeout: (fn, ms) => { timers.push({ fn, ms }); return timers.length; },
  };
  const timers = [];
  const events = [];
  const document = {
    documentElement: { getAttribute: (k) => (attrs.has(k) ? attrs.get(k) : null), setAttribute: (k, v) => attrs.set(k, String(v)), removeAttribute: (k) => attrs.delete(k) },
    addEventListener: on(listeners.document),
    dispatchEvent: (event) => { events.push(event.type); for (const fn of listeners.document.get(event.type) || []) fn(event); },
  };
  vm.runInNewContext(read("js/boot_theme.js"), { window, document, Event: class { constructor(type) { this.type = type; } } });
  return { attrs, store, media, events, document, timers, fire: (type, key) => { for (const fn of listeners.window.get(type) || []) fn({ key }); } };
}

test("boot_theme.js: each choice sets data-theme and data-theme-choice before the first paint", () => {
  const cases = [
    [undefined, false, "light", "system"], [undefined, true, "dark", "system"], ["system", true, "dark", "system"],
    ["dark", false, "dark", "dark"], ["light", true, "light", "light"], ["bogus", true, "dark", "system"],
  ];
  for (const [stored, dark, theme, choice] of cases) {
    const b = boot({ stored, dark });
    assert.equal(b.attrs.get("data-theme"), theme, `${stored} ${dark}`);
    assert.equal(b.attrs.get("data-theme-choice"), choice, `${stored} ${dark}`);
  }
});

test("boot_theme.js: blocked storage gives the default, and the event hune-theme applies a new choice", () => {
  assert.equal(boot({ blocked: true, dark: true }).attrs.get("data-theme"), "dark");
  const b = boot({ stored: "light" });
  assert.equal(b.attrs.get("data-theme"), "light");
  b.store.set("hune.theme", "dark");
  b.document.dispatchEvent({ type: "hune-theme" });
  assert.equal(b.attrs.get("data-theme"), "dark");
  assert.equal(b.attrs.get("data-theme-choice"), "dark");
});

test("boot_theme.js: the system and another tab change the theme, and the canvases are told", () => {
  const b = boot({ stored: "system", dark: false });
  assert.equal(b.attrs.get("data-theme"), "light");
  b.media.matches = true;
  b.media.listeners.forEach((fn) => fn());
  assert.equal(b.attrs.get("data-theme"), "dark");
  assert.ok(b.events.includes("hune-theme"), "a change that the control did not make sends the event");
  b.events.length = 0;
  b.store.set("hune.theme", "light");
  b.fire("storage", "hune.theme");
  assert.equal(b.attrs.get("data-theme"), "light");
  assert.deepEqual(b.events, ["hune-theme"]);
  b.events.length = 0;
  b.fire("storage", "other.key");
  assert.deepEqual(b.events, [], "another key changes nothing");
});

test("the boot block: version line and title, and a bad block gives empty values", () => {
  assert.deepEqual(parseBoot('{"build":"abc","version":"0.1.0:1f2e3d4c5","version_title":"Hune 0.1.0, commit 1f2e3d4c5"}'), { build: "abc", version: "0.1.0:1f2e3d4c5", versionTitle: "Hune 0.1.0, commit 1f2e3d4c5" });
  assert.deepEqual(parseBoot('{"build":"abc"}'), { build: "abc", version: "", versionTitle: "" });
  assert.deepEqual(parseBoot('{"build":7,"version":[]}'), { build: "", version: "", versionTitle: "" });
  for (const bad of ["", "not json", "null"]) assert.deepEqual(parseBoot(bad), { build: "", version: "", versionTitle: "" }, bad);
});

test("the shell carries the version line in the boot block and the header shows it as text, never as HTML", () => {
  const shell = read("shell.html");
  assert.match(shell, /id="hune-boot">\{"build":"\{\{build\}\}","version":"\{\{version_text\}\}","version_title":"\{\{version_title\}\}"\}</);
  const main = read("js/main.js");
  assert.match(main, /text: boot\.version/);
  assert.match(main, /title: boot\.versionTitle/);
  assert.match(main, /testid: "app-version"/);
  assert.match(main, /"aria-describedby": "brand-version"/);
});

test("header steps: the widths of the steps and what applies at a width", () => {
  assert.deepEqual(Object.keys(STEPS), ["versionMin", "labelsMax", "listFiltersMax", "listMenuMax", "iconsMax", "menuTimeMax", "addIconMax", "revertMenuMax", "compactMax", "nameMax", "tightMax"]);
  assert.equal(maxQuery(640), "(max-width: 640px)");
  const at = (w) => stepsAt(w);
  assert.deepEqual(at(1920), { version: true, labels: true, listFilterText: true, listMenu: false, icons: false, timeInMenu: false, tight: false });
  assert.equal(at(1281).version, true);
  assert.equal(at(1280).version, false, "the version line shows above 1280 px only");
  assert.equal(at(1101).labels, true);
  assert.equal(at(1100).labels, false, "up to 1100 px the labels Edit and Save hide");
  assert.equal(at(1180).listFilterText, false);
  assert.equal(at(900).listMenu, true);
  assert.equal(at(901).listMenu, false);
  assert.equal(at(768).icons, true);
  assert.equal(at(769).icons, false);
  assert.equal(at(900).timeInMenu, true, "up to 900 px the time shift and the zoom out are in the menu");
  assert.equal(at(901).timeInMenu, false);
  assert.equal(at(480).tight, true);
  assert.equal(at(481).tight, false);
});

test("header steps: the css media queries use the widths of header_layout.js", () => {
  const base = read("css/base.css");
  const components = read("css/components.css");
  const list = read("css/list.css");
  const has = (css, px) => css.includes(`@media (max-width: ${px}px)`);
  assert.ok(has(base, STEPS.versionMin - 1), "the version line");
  assert.ok(has(base, STEPS.nameMax), "the brand name");
  assert.ok(has(base, STEPS.tightMax), "the phone step");
  assert.ok(has(components, STEPS.labelsMax) && components.includes(`@media (min-width: ${STEPS.labelsMax + 1}px)`), "the labels");
  assert.ok(has(components, STEPS.iconsMax) && has(components, STEPS.menuTimeMax) && has(components, STEPS.nameMax) && has(components, STEPS.addIconMax) && has(components, STEPS.revertMenuMax) && has(components, STEPS.compactMax) && has(components, STEPS.tightMax), "the icon steps");
  assert.ok(has(list, STEPS.listFiltersMax) && has(list, STEPS.listMenuMax) && has(list, STEPS.tightMax), "the list steps");
});

test("css: the theme menu moves 6 px with --dur-base, joins its button, and has no motion with prefers-reduced-motion", () => {
  const css = read("css/components.css");
  assert.match(css, /\.theme-select__menu \{[^}]*transform: translateY\(-6px\) scaleY\(0\.98\)/);
  assert.match(css, /transition: opacity var\(--dur-base\) var\(--ease\), transform var\(--dur-base\) var\(--ease\), visibility 0s linear var\(--dur-base\)/);
  assert.match(css, /\.theme-select--open \.theme-select__button, \.theme-select--closing \.theme-select__button \{[^}]*border-bottom-left-radius: 0/);
  const reduced = css.slice(css.indexOf("@media (prefers-reduced-motion: reduce) {\n  .theme-select__menu"));
  assert.match(reduced.slice(0, 120), /\.theme-select__menu \{ transition: none; transform: none; \}/);
  // The icon of the choice shows by data-theme-choice, so the first paint has the right icon.
  assert.match(css, /html\[data-theme-choice="dark"\] \.theme-select__current > \.theme-icon--dark/);
  assert.match(css, /html\[data-theme-choice="light"\] \.theme-select__current > \.theme-icon--light/);
});

test("css: the header is one flex row of 44 px with no second row, and the version line is never cut", () => {
  const base = read("css/base.css");
  assert.match(base, /:root \{ --header-base-h: 44px; \}/);
  assert.match(base, /#app-header \{[^}]*display: flex; flex-wrap: nowrap;[^}]*height: var\(--header-base-h\)/);
  assert.ok(!/data-layout/.test(base), "no stacked layout");
  assert.ok(!/grid-template-areas/.test(base.slice(base.indexOf("#app-header {"), base.indexOf("#app-main"))), "the header is not a grid");
  assert.match(base, /\.brand__version \{[^}]*white-space: nowrap/);
  assert.ok(!/\.brand__version \{[^}]*text-overflow/.test(base));
  // The crumbs are small: 12 px, a line of 16 px, an ellipsis, and a thin chevron.
  assert.match(base, /\.crumbs \{[^}]*font-size: var\(--fs-sm\)[^}]*line-height: 16px/);
  assert.match(base, /\.crumbs__text \{[^}]*text-overflow: ellipsis/);
  assert.match(read("js/main.js"), /icon\("chevron-right", 12\)/);
  assert.match(read("js/main.js"), /icon\("logo", 20\)/);
});

test("boot_theme.js releases the hold of the first paint after 2 s (9.6)", () => {
  const b = boot();
  b.attrs.set("data-hune-hold", "");
  assert.deepEqual(b.timers.map((t) => t.ms), [2000]);
  b.timers[0].fn();
  assert.equal(b.attrs.has("data-hune-hold"), false);
});
