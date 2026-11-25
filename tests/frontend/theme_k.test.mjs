// SPDX-License-Identifier: MIT OR Apache-2.0
// The colour theme of the chrome: the copied tokens of ClickHouse Dash (layer 1), and look.css, which sets the tokens of the calm look
// again in the same four contexts (9.25). The CSS is read as text: the page has no DOM here.
// The browser case B91 reads the computed colours.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (p) => readFileSync(new URL(`../../web/${p}`, import.meta.url), "utf8");
const tokens = read("css/tokens.css");
const vendor = read("css/vendor/chdash/00-tokens.css");
const look = read("css/look.css");
const shell = read("shell.html");

// The custom properties set in one block of a file.
function block(css, start) {
  const i = css.indexOf(start);
  assert.ok(i >= 0, `block ${start}`);
  return Object.fromEntries([...css.slice(i, css.indexOf("\n}\n", i)).matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}
const root = block(vendor, ":root {\n  color-scheme: dark;");
const lookRoot = block(look, ":root {\n  --bg:");
const lookDark = block(look, 'html[data-theme="dark"] {');
const lookLight = block(look, 'html[data-theme="light"] {');
const lookShape = block(look, ":root {\n  --r-control");
const lookMediaLight = block(look, "@media (prefers-color-scheme: light) {\n  :root {");
// A token whose value is var(--other) names a token of the same element: the helper resolves it against the merged theme.
function settle(theme) {
  const out = { ...theme };
  for (let pass = 0; pass < 2; pass++) {
    for (const [k, v] of Object.entries(out)) {
      const m = /^var\((--[a-z0-9-]+)\)$/.exec(v);
      if (m && out[m[1]] !== undefined) out[k] = out[m[1]];
    }
  }
  return out;
}
const THEMES = {
  dark: settle({ ...root, ...block(vendor, 'html[data-theme="dark"] {'), ...lookShape, ...lookRoot, ...lookDark }),
  light: settle({ ...root, ...block(vendor, 'html[data-theme="light"] {'), ...lookShape, ...lookRoot, ...lookLight }),
};

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
  for (const file of ["css/base.css", "css/components.css", "css/picker.css", "css/list.css", "css/editor.css", "css/dashboard.css", "css/look.css"]) {
    let text = read(file);
    // look.css is the one file of values: its token blocks are before the marker, and its component rules are after it.
    if (file === "css/look.css") text = text.slice(text.indexOf("/* --- Shared components"));
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
    for (const s of ["--bg", "--panel", "--raised", "--input-bg"]) surf[s] = rgba(t[s], [0, 0, 0]);
    const resolve = (v) => (/^var\((--[a-z0-9-]+)\)$/.test(v) ? t[/^var\((--[a-z0-9-]+)\)$/.exec(v)[1]] : v);
    for (const [sname, sv] of Object.entries(surf)) {
      for (const tok of ["--text", "--muted"]) assert.ok(ratio(rgba(resolve(t[tok]), sv), sv) >= 4.5, `${name} ${tok} on ${sname}`);
      for (const tok of ["--control-border", "--control-border-hover", "--focus-ring-color", "--editor-border"]) {
        const v = tok === "--editor-border" ? t["--control-border"] : resolve(t[tok]); // tokens.css makes the editor frame the edge of a field
        assert.ok(ratio(rgba(v, sv), sv) >= 3, `${name} ${tok} on ${sname}`);
      }
    }
    for (const f of ["--accent-fill", "--accent-fill-hover"]) assert.ok(ratio(rgba(t["--on-fill"], [0, 0, 0]), rgba(t[f], [0, 0, 0])) >= 4.5, `${name} --on-fill on ${f}`);
    for (const f of ["--danger-fill", "--danger-fill-hover"]) assert.ok(ratio(rgba(t["--on-danger"], [0, 0, 0]), rgba(t[f], [0, 0, 0])) >= 4.5, `${name} --on-danger on ${f}`);
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
  assert.match(components, /\.menu__item\[aria-checked="true"\][^{]*\{ background: var\(--selected-bg\); box-shadow: inset 0 0 0 1px var\(--selected-edge\)/);
  assert.match(components, /\.btn--tb-primary:hover \{ background: var\(--primary-hover\); \}/);
  assert.match(components, /\.menu--joined \.menu__item\[aria-checked="true"\], \.menu--joined \.menu__item\.is-current \{[^}]*background: var\(--selected-bg\)/);
  assert.match(components, /\.selector\.is-joined-open, \.selector\.is-joined-open:hover:not\(:disabled\) \{[^}]*border-bottom-color: transparent/);
  assert.match(read("css/dashboard.css"), /\.listbox__item\[aria-selected="true"\] \{ background: var\(--selected-bg\)/);
  assert.match(read("css/panel-table.css"), /\.filter-popup \{[^}]*border: 1px solid var\(--border-strong\)/);
});

test("look.css loads after tokens.css and before base.css, and it sets the same tokens in its four contexts", () => {
  assert.ok(shell.indexOf("css/tokens.css") < shell.indexOf("css/look.css"));
  assert.ok(shell.indexOf("css/look.css") < shell.indexOf("css/base.css"));
  const names = (o) => Object.keys(o).sort().join(" ");
  assert.equal(names(lookLight), names(lookMediaLight), "the forced light block and the light system block differ");
  assert.equal(names(lookDark), names(lookLight), "the forced dark block and the forced light block differ");
  assert.equal(names(lookRoot), names(lookDark), "the dark root block and the forced dark block differ");
});

test("look.css leaves the tokens of the rules of 9.26 to layer 1: the edge of a field, the focus ring and the danger fill", () => {
  for (const name of ["--control-border", "--control-border-hover", "--focus-ring-color", "--danger-fill"]) {
    for (const b of [lookRoot, lookDark, lookLight, lookMediaLight]) assert.equal(b[name], undefined, `${name} is set in look.css`);
  }
});

// The 29 colours of the tag chips (list.js). The chip text is the colour mixed with the ink base, and the fill is a share of the colour.
const palette = [...read("js/list.js").match(/const TAG_PALETTE = \[([^\]]*)\]/)[1].matchAll(/#[0-9a-f]{6}/g)].map((m) => m[0]);
const mixRgb = (a, b, share) => a.map((v, i) => v * share + b[i] * (1 - share));
test("the 29 tag colours give a chip text with 4.5:1 on the tint, on each surface that holds a chip, in both themes", () => {
  assert.equal(palette.length, 29);
  const fill = Number.parseFloat(look.match(/--tag-fill-share:\s*([0-9.]+)%/)[1]) / 100;
  for (const [name, t] of Object.entries(THEMES)) {
    const base = rgba(t["--tag-ink-base"], [0, 0, 0]);
    const share = Number.parseFloat(t["--tag-ink-share"]) / 100;
    const surfaces = ["--panel", "--bg", "--raised", "--selected-bg"].map((s) => rgba(t[s], [0, 0, 0]));
    for (const c of palette) {
      const tc = rgba(c, [0, 0, 0]);
      const ink = mixRgb(tc, base, share);
      for (const s of surfaces) assert.ok(ratio(ink, mixRgb(tc, s, fill)) >= 4.5, `${name} ${c}`);
    }
  }
});

// The JSON area of the edit mode (10.4) uses the palette of VS Code Light+ and Dark+. A text colour has 4.5:1 on the surface of the editor, on the band of
// the line of the caret and on the box of a bracket. The colours of Light+ that fail are darker here (the number, the second bracket colour and the
// bracket with no pair). The selection is a state that lasts a moment, as in VS Code: the text colours keep 2.5:1 on it (the colour of a bracket with
// no pair excepted: it stands out by its hue).
test("the JSON tokens have 4.5:1 for the text on the editor, on the band of the caret line and on the box of a bracket, in both themes", () => {
  const texts = ["--json-key", "--json-str", "--json-num", "--json-const", "--json-punct", "--json-bracket-1", "--json-bracket-2", "--json-bracket-3", "--json-bracket-bad", "--json-lineno", "--json-lineno-active"];
  for (const [name, t] of Object.entries(THEMES)) {
    const get = (k) => {
      const v = (name === "dark" ? { ...lookRoot, ...lookDark } : { ...lookLight })[k];
      assert.ok(v !== undefined, `${name} ${k} is set`);
      return v;
    };
    const bg = rgba(t["--editor-solid-bg"], [0, 0, 0]);
    const band = rgba(get("--json-line-active"), bg);
    const match = rgba(get("--json-match-bg"), bg);
    const selection = rgba(get("--json-selection"), bg);
    for (const tok of texts) {
      const c = rgba(get(tok), bg);
      assert.ok(ratio(c, bg) >= 4.5, `${name} ${tok} on the editor: ${ratio(c, bg).toFixed(2)}`);
      if (tok !== "--json-lineno" && tok !== "--json-lineno-active") {
        assert.ok(ratio(c, band) >= 4.5, `${name} ${tok} on the band: ${ratio(c, band).toFixed(2)}`);
        assert.ok(ratio(c, match) >= 4.5 || !tok.startsWith("--json-bracket"), `${name} ${tok} on a box: ${ratio(c, match).toFixed(2)}`);
        assert.ok(ratio(c, selection) >= 2.5 || tok === "--json-bracket-bad", `${name} ${tok} on the selection: ${ratio(c, selection).toFixed(2)}`);
      }
    }
    assert.ok(ratio(rgba(get("--json-squiggle"), bg), bg) >= 3, `${name} the wavy line has 3:1`);
  }
});

test("the accent has 3:1 against the white line of the logo, and the fixed accent of the favicon has 4.5:1", () => {
  const white = [255, 255, 255];
  for (const [name, t] of Object.entries(THEMES)) assert.ok(ratio(white, rgba(t["--accent-fill"], [0, 0, 0])) >= 3, `${name} accent and the logo line`);
  const favicon = read("favicon.svg").match(/rx="6" fill="(#[0-9a-f]{6})"/)[1];
  assert.ok(ratio(white, rgba(favicon, [0, 0, 0])) >= 4.5);
  assert.equal(favicon, THEMES.light["--accent-fill"]);
});

// The comment of the SQL editor over the band of the current block (4 % of the text colour, as in editor.css) and over the editor itself: 4.5:1 in
// both themes. The copied tokens mix the muted colour at 90 % for the comment, which is too pale over the band in the light theme (4.36:1).
function commentColour(theme, under) {
  const v = theme["--sql-comment"];
  const m = /^color-mix\(in srgb, var\((--[a-z-]+)\) (\d+)%, transparent\)$/.exec(v);
  if (m) return mixRgb(rgba(theme[m[1]], under), under, Number(m[2]) / 100);
  return rgba(v, under);
}
test("the comment of the SQL editor has 4.5:1 over the editor and over the band of the block, in both themes", () => {
  for (const [name, t] of Object.entries(THEMES)) {
    const bg = rgba(t["--editor-solid-bg"], [0, 0, 0]);
    const band = mixRgb(rgba(t["--text"], bg), bg, 0.04);
    assert.ok(ratio(commentColour(t, bg), bg) >= 4.5, `${name} the comment on the editor: ${ratio(commentColour(t, bg), bg).toFixed(2)}`);
    assert.ok(ratio(commentColour(t, band), band) >= 4.5, `${name} the comment on the band: ${ratio(commentColour(t, band), band).toFixed(2)}`);
  }
});
