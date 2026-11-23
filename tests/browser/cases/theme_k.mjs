// Browser case B91 (9.25, 9.26): the colour theme of the chrome is the theme of ClickHouse Dash, in the dark, the light and the system mode.
// The expected values come from the copied file css/vendor/chdash/00-tokens.css (a byte copy of the tokens of ClickHouse Dash), read at
// the start of the case. The case reads the computed colours of the chrome tokens on the page, and it checks the contrast of the text,
// of the edge of a control and of the focus ring (WCAG 2.2 AA: 4.5:1 for text, 3:1 for components).
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { sleep } from "../lib/h.mjs";

const VENDOR = join(dirname(fileURLToPath(import.meta.url)), "../../../web/css/vendor/chdash/00-tokens.css");

/** The custom properties of one block of the copied file. */
function block(css, start) {
  const i = css.indexOf(start);
  if (i < 0) throw new Error(`block ${start} is not in the copied tokens`);
  const j = css.indexOf("\n}\n", i);
  return Object.fromEntries([...css.slice(i, j).matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}

function expectedThemes() {
  const css = readFileSync(VENDOR, "utf8");
  const root = block(css, ":root {\n  color-scheme: dark;");
  const dark = { ...root, ...block(css, 'html[data-theme="dark"] {') };
  const light = { ...root, ...block(css, 'html[data-theme="light"] {') };
  return { dark, light };
}

// The chrome tokens that carry the colour theme (the map of the themed tokens in web/css/tokens.css). A value that is a var() is resolved by the browser.
const COLOUR = ["--bg", "--panel", "--raised", "--text", "--muted", "--border", "--border-strong", "--hover-tint", "--input-bg", "--button-bg-hover", "--button-border",
  "--button-border-hover", "--control-border", "--control-border-hover", "--row-hover", "--selected-bg", "--selected-edge", "--accent-fill", "--accent-fill-hover", "--accent-text",
  "--danger-fill", "--danger", "--warning", "--success", "--info", "--on-fill", "--focus-ring-color"];

// Hune roles that must follow a token of ClickHouse Dash: [Hune token, the token of ClickHouse Dash that it names].
const ROLES = [["--primary", "--accent-fill"], ["--primary-hover", "--accent-fill-hover"], ["--on-primary", "--on-fill"], ["--header-bg", "--panel"], ["--surface-menu", "--panel"],
  ["--menu-item-hover", "--row-hover"], ["--tb-bg", "--button-bg"], ["--tb-border", "--button-border"], ["--tb-bg-hover", "--button-bg-hover"], ["--accent-tint", "--selected-bg"],
  ["--status-error-bg", "--danger-fill"], ["--text-faint", "--muted"], ["--editor-border", "--control-border"]];

/** Page script: the computed colour of each token (through a probe element), the box-shadow of the overlay, and the colours of real chrome. */
const PROBE = (names, literals) => `(() => {
  const probe = document.createElement("i");
  document.body.append(probe);
  const colour = (value) => { probe.style.color = ""; probe.style.color = value; return getComputedStyle(probe).color; };
  const out = { theme: document.documentElement.dataset.theme, tokens: {}, expected: {} };
  for (const n of ${JSON.stringify(names)}) out.tokens[n] = colour("var(" + n + ")");
  for (const [n, v] of Object.entries(${JSON.stringify(literals)})) out.expected[n] = colour(v);
  probe.style.boxShadow = "var(--shadow-overlay)";
  out.shadow = getComputedStyle(probe).boxShadow;
  probe.style.boxShadow = "";
  probe.remove();
  const css = (sel, prop) => { const e = document.querySelector(sel); return e ? getComputedStyle(e)[prop] : null; };
  out.real = { body: css("body", "backgroundColor"), bodyText: css("body", "color"), header: css("#app-header", "backgroundColor"), headerEdge: css("#app-header", "borderBottomColor") };
  return out;
})()`;

const lin = (c) => { const s = c / 255; return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4; };
const parse = (s) => { const m = /rgba?\(([^)]*)\)/.exec(s); const p = m[1].split(/[ ,\/]+/).filter(Boolean).map(Number); return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1]; };
const over = (fg, bg) => [0, 1, 2].map((i) => fg[i] * fg[3] + bg[i] * (1 - fg[3]));
const lum = (c) => 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
const ratio = (a, b) => { const x = lum(a); const y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };

async function open(t, url, { scheme, choice }) {
  const p = await t.page({ width: 1280, height: 900 });
  t.allow(p, /Failed to load resource.*404/); // the page of an error asks for a dashboard that does not exist
  await p.emulateColorScheme(scheme);
  await p.send("Page.addScriptToEvaluateOnNewDocument", { source: `try { localStorage.setItem("hune.theme", ${JSON.stringify(choice)}); } catch (e) {}` });
  await p.goto(url);
  await p.idle();
  await sleep(150);
  return p;
}

async function checkMode(t, label, url, { scheme, choice, theme }, expected) {
  const p = await open(t, url, { scheme, choice });
  const names = [...new Set([...COLOUR, ...ROLES.flat()])];
  const r = await p.eval(PROBE(names, Object.fromEntries(COLOUR.map((n) => [n, expected[n]]))));
  t.eq(r.theme, theme, `${label}: data-theme`);
  for (const n of COLOUR) t.eq(r.tokens[n], r.expected[n], `${label}: ${n} is the value of ClickHouse Dash (${expected[n]})`);
  for (const [hune, chdash] of ROLES) t.eq(r.tokens[hune], r.tokens[chdash], `${label}: ${hune} follows ${chdash}`);
  t.assert(r.shadow !== "none" && r.shadow.length > 10, `${label}: the shadow of an overlay: ${r.shadow}`);
  t.eq(r.real.body, r.tokens["--bg"], `${label}: the page has the colour --bg`);
  t.eq(r.real.bodyText, r.tokens["--text"], `${label}: the page has the colour --text`);
  t.eq(r.real.header, r.tokens["--panel"], `${label}: the header has the colour --panel`);

  // Contrast. A translucent token is mixed over the surface that it sits on.
  const c = Object.fromEntries(Object.entries(r.tokens).map(([k, v]) => [k, parse(v)]));
  const surfaces = { "--bg": c["--bg"], "--panel": c["--panel"], "--raised": c["--raised"], "--input-bg": c["--input-bg"] };
  const solid = (fg, bg) => over(fg, bg);
  for (const [sname, sv] of Object.entries(surfaces)) {
    const bg = sv.slice(0, 3);
    for (const tok of ["--text", "--muted"]) t.assert(ratio(solid(c[tok], bg), bg) >= 4.5, `${label}: ${tok} on ${sname} has ${ratio(solid(c[tok], bg), bg).toFixed(2)}:1, 4.5:1 needed`);
    for (const tok of ["--control-border", "--control-border-hover", "--focus-ring-color"]) t.assert(ratio(solid(c[tok], bg), bg) >= 3, `${label}: ${tok} on ${sname} has ${ratio(solid(c[tok], bg), bg).toFixed(2)}:1, 3:1 needed`);
  }
  for (const f of ["--accent-fill", "--accent-fill-hover", "--danger-fill"]) {
    const bg = c[f].slice(0, 3);
    t.assert(ratio(c["--on-fill"].slice(0, 3), bg) >= 4.5, `${label}: --on-fill on ${f} has ${ratio(c["--on-fill"].slice(0, 3), bg).toFixed(2)}:1, 4.5:1 needed`);
  }
  const panel = c["--panel"].slice(0, 3);
  for (const tok of ["--accent-text", "--danger", "--warning", "--success"]) t.assert(ratio(c[tok].slice(0, 3), panel) >= 4.5, `${label}: ${tok} on --panel has ${ratio(c[tok].slice(0, 3), panel).toFixed(2)}:1, 4.5:1 needed`);
  await p.close();
}

export const cases = [
  {
    id: "B91",
    name: "the chrome has the colour theme of ClickHouse Dash in dark, light and system, with the contrast of WCAG 2.2 AA (9.25, 9.26)",
    async run(t) {
      const expected = expectedThemes();
      // The error page and a dashboard page both carry the header and the chrome. The error page needs no data.
      const url = t.url("d/b91-no-such-dashboard");
      await checkMode(t, "dark", url, { scheme: "light", choice: "dark", theme: "dark" }, expected.dark);
      await checkMode(t, "light", url, { scheme: "dark", choice: "light", theme: "light" }, expected.light);
      await checkMode(t, "system, dark", url, { scheme: "dark", choice: "system", theme: "dark" }, expected.dark);
      await checkMode(t, "system, light", url, { scheme: "light", choice: "system", theme: "light" }, expected.light);
    },
  },
];
