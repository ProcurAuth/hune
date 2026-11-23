// The colour theme of the chrome is the theme of ClickHouse Dash (9.25, ID-K50). The CSS is read as text: the page has no DOM here.
// The browser case B91 reads the computed colours.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (p) => readFileSync(new URL(`../../web/${p}`, import.meta.url), "utf8");
const tokens = read("css/tokens.css");
const vendor = read("css/vendor/chdash/00-tokens.css");

// The custom properties set in one block of a file.
function block(css, start) {
  const i = css.indexOf(start);
  assert.ok(i >= 0, `block ${start}`);
  return Object.fromEntries([...css.slice(i, css.indexOf("\n}\n", i)).matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}
const root = block(vendor, ":root {\n  color-scheme: dark;");
const THEMES = { dark: { ...root, ...block(vendor, 'html[data-theme="dark"] {') }, light: { ...root, ...block(vendor, 'html[data-theme="light"] {') } };

function rgba(value, over) {
  const hex = /^#([0-9a-f]{6})$/i.exec(value);
  if (hex) return [0, 2, 4].map((i) => parseInt(hex[1].slice(i, i + 2), 16));
  const m = /^rgba?\(([^)]*)\)$/.exec(value);
  const p = m[1].split(",").map(Number);
  const a = p.length > 3 ? p[3] : 1;
  return [0, 1, 2].map((i) => p[i] * a + over[i] * (1 - a));
}
const lum = (c) => {
  const [r, g, b] = c.map((v) => { const s = v / 255; return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4; });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const ratio = (a, b) => {
  const x = lum(a);
  const y = lum(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
};

// The tokens that layer 1 owns. tokens.css must not set them again: the theme is the one of ClickHouse Dash.
const OWNED = ["--bg", "--panel", "--raised", "--text", "--muted", "--border", "--border-strong", "--hover-tint", "--input-bg", "--editor-solid-bg", "--button-bg-hover",
  "--button-border", "--button-border-hover", "--control-border", "--control-border-hover", "--row-hover", "--row-divider", "--shadow-overlay", "--accent-fill", "--accent-fill-hover",
  "--accent-text", "--selected-bg", "--danger", "--warning", "--success", "--info", "--danger-fill", "--on-fill", "--focus-ring-color"];

test("tokens.css leaves the surfaces, the text, the borders, the states and the accent to the copied tokens", () => {
  for (const name of OWNED) assert.doesNotMatch(tokens, new RegExp(`^\\s*${name}:`, "m"), `${name} is set in tokens.css`);
  assert.ok(tokens.includes("ID-K50"), "the file names the decision");
});

test("the Hune roles are names of copied tokens, not values", () => {
  const roles = {
    "--primary": "--accent-fill", "--primary-hover": "--accent-fill-hover", "--on-primary": "--on-fill", "--header-bg": "--panel", "--surface-menu": "--panel",
    "--menu-item-hover": "--row-hover", "--tb-bg": "--button-bg", "--tb-border": "--button-border", "--tb-bg-hover": "--button-bg-hover", "--tb-border-hover": "--button-border-hover",
    "--accent-tint": "--selected-bg", "--status-error-bg": "--danger-fill", "--text-faint": "--muted", "--shadow-menu": "--shadow-overlay", "--shadow-tooltip": "--shadow-overlay",
  };
  for (const [name, target] of Object.entries(roles)) assert.match(tokens, new RegExp(`^\\s*${name}: var\\(${target}\\);`, "m"), `${name} names ${target}`);
});

test("the three tokens that differ between the themes have the four contexts", () => {
  for (const name of ["--text-strong", "--link", "--select-arrow"]) assert.equal((tokens.match(new RegExp(`^\\s*${name}:`, "gm")) || []).length, 4, name);
});

test("component CSS has no literal colour (the print block of base.css and the named colours of dashboard.css are the two exceptions)", () => {
  for (const file of ["css/base.css", "css/components.css", "css/picker.css", "css/list.css", "css/editor.css", "css/dashboard.css"]) {
    let text = read(file);
    text = text.replace(/\/\*[\s\S]*?\*\//g, "");
    text = text.replace(/@media print \{[\s\S]*$/, "");
    text = text.split("\n").filter((l) => !l.includes("--viz-")).join("\n");
    assert.doesNotMatch(text, /#[0-9a-fA-F]{3,8}\b/, `${file}: a hex colour`);
    assert.doesNotMatch(text, /\brgba?\(/, `${file}: an rgb colour`);
    assert.doesNotMatch(text, /\bhsla?\(/, `${file}: an hsl colour`);
  }
});

test("each text token of the copied theme has 4.5:1 and each edge of a control and the focus ring have 3:1 in both themes", () => {
  for (const [name, t] of Object.entries(THEMES)) {
    const surf = {};
    for (const s of ["--bg", "--panel", "--raised", "--input-bg"]) surf[s] = rgba(t[s] === "var(--raised)" ? t["--raised"] : t[s], [0, 0, 0]);
    const resolve = (v) => (/^var\((--[a-z0-9-]+)\)$/.test(v) ? t[/^var\((--[a-z0-9-]+)\)$/.exec(v)[1]] : v);
    for (const [sname, sv] of Object.entries(surf)) {
      for (const tok of ["--text", "--muted"]) assert.ok(ratio(rgba(resolve(t[tok]), sv), sv) >= 4.5, `${name} ${tok} on ${sname}`);
      for (const tok of ["--control-border", "--control-border-hover", "--focus-ring-color", "--editor-border"]) {
        const v = tok === "--editor-border" ? t["--control-border"] : resolve(t[tok]); // tokens.css makes the editor frame the edge of a field
        assert.ok(ratio(rgba(v, sv), sv) >= 3, `${name} ${tok} on ${sname}`);
      }
    }
    for (const f of ["--accent-fill", "--accent-fill-hover", "--danger-fill", "--danger-fill-hover"]) assert.ok(ratio(rgba(t["--on-fill"], [0, 0, 0]), rgba(t[f], [0, 0, 0])) >= 4.5, `${name} --on-fill on ${f}`);
    for (const tok of ["--accent-text", "--danger", "--warning", "--success", "--info"]) assert.ok(ratio(rgba(t[tok], [0, 0, 0]), surf["--panel"]) >= 4.5, `${name} ${tok} on --panel`);
  }
});

test("the light named colours keep 3:1 on the page of the copied light theme (the page is darker than the Grafana page)", () => {
  const light = tokens.slice(tokens.lastIndexOf('html[data-theme="light"] {\n  --viz-super'));
  const values = [...light.matchAll(/--viz-([a-z-]+): (#[0-9a-f]{6});/g)];
  assert.equal(values.length, 30);
  const page = rgba(THEMES.light["--bg"], [0, 0, 0]);
  for (const [, name, c] of values) assert.ok(ratio(rgba(c, [0, 0, 0]), page) >= 3, `${name} ${c} on the page`);
});

test("the states of the chrome follow ClickHouse Dash: edge of a popup, selected state, head row, primary hover", () => {
  const components = read("css/components.css");
  assert.match(components, /\.menu \{[^}]*border: 1px solid var\(--border-strong\)/);
  assert.match(components, /\.select__popup \{[^}]*border: 1px solid var\(--border-strong\)/);
  assert.match(components, /\[data-tooltip\]::after \{[^}]*border: 1px solid var\(--border-strong\)/);
  assert.match(components, /\.theme-select__menu \{[^}]*border: 1px solid var\(--border-strong\)/);
  assert.match(components, /\.menu__item\[aria-checked="true"\][^{]*\{ background: var\(--selected-bg\); box-shadow: inset 0 0 0 1px var\(--selected-edge\)/);
  assert.match(components, /\.btn--tb-primary:hover \{ background: var\(--primary-hover\); \}/);
  assert.match(read("css/list.css"), /\.list-colhead \{[^}]*background: var\(--thead-bg\)/);
  assert.match(read("css/list.css"), /\.list-tagmenu__item\[aria-selected="true"\] \{ background: var\(--selected-bg\)/);
  assert.match(read("css/dashboard.css"), /\.listbox__item\[aria-selected="true"\] \{ background: var\(--selected-bg\)/);
  assert.match(read("css/panel-table.css"), /\.filter-popup \{[^}]*border: 1px solid var\(--border-strong\)/);
});
