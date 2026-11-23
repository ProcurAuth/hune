// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-07: colours, palette and thresholds (9.16).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { NAMED, NAMED_LIGHT, PALETTE, QCHART, QCHART_COUNT, qchartColor, relativeLuminance, fillOf, resolveFill, resolveColor, paletteColor, thresholdColor, fieldColor, readableTextOn, gradientFrom } from "../../web/js/colors.js";
import { isColorValid } from "../../web/js/editor/validators.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-07-colors.json", import.meta.url)));

test("FE-07 named tokens and resolve", () => {
  assert.equal(Object.keys(NAMED).length, 30);
  for (const [k, val] of Object.entries(v.named)) assert.equal(NAMED[k].toUpperCase(), val.toUpperCase(), k);
  for (const [k, val] of Object.entries(v.resolve)) assert.equal(resolveColor(k), val, k);
  assert.ok(isColorValid("text") && isColorValid("transparent") && isColorValid("#aabbccdd") && isColorValid("rgba(1, 2, 3, 0.5)"));
  assert.ok(!isColorValid("blu") && !isColorValid("rgb(1,2,3)") && !isColorValid("#abcde"));
});

test("FE-07 the classic palette of Grafana (the reference of 19.20a) has 64 entries, in its order, and wraps", () => {
  assert.equal(PALETTE.length, 64);
  assert.equal(new Set(PALETTE.map((c) => resolveColor(c).toUpperCase())).size, 64, "no colour twice");
  const hues = ["green", "yellow", "blue", "orange", "red", "purple"];
  assert.deepEqual(PALETTE.slice(0, 6), ["green", "semi-dark-yellow", "blue", "orange", "red", "purple"]);
  assert.deepEqual(PALETTE.slice(6, 12), hues.map((h) => `dark-${h}`));
  assert.deepEqual(PALETTE.slice(12, 18), hues.map((h) => `super-light-${h}`));
  for (const [i, val] of Object.entries(v.palette)) assert.equal(paletteColor(Number(i)).toUpperCase(), val.toUpperCase(), i);
});

test("FE-07 thresholds", () => {
  for (const c of v.thresholds) {
    const value = c.v === "NaN" ? NaN : c.v;
    assert.equal(thresholdColor(c.t, value, c.min, c.max), c.expect, JSON.stringify(c));
  }
});

test("FE-07 colour modes", () => {
  for (const c of v.modes) {
    const got = fieldColor(c.config, c.v, c.ctx);
    assert.equal(got === null ? null : got.toUpperCase(), c.expect === null ? null : c.expect.toUpperCase());
  }
});

test("FE-07 text on a colour and the gradient", () => {
  for (const [bg, fg] of Object.entries(v.text_on)) assert.equal(readableTextOn(bg), fg, bg);
  for (const [c, g] of Object.entries(v.gradient)) assert.equal(gradientFrom(c).toLowerCase(), g.toLowerCase());
});

// The light shades of Grafana 12.4.2 (pixel probes of table cells in the light theme, wave G2).
const LIGHT_PROBED = {
  "super-light-red": "#FF7383", "light-red": "#F2495C", red: "#E02F44", "semi-dark-red": "#C4162A", "dark-red": "#AD0317",
  "super-light-orange": "#FFB357", "light-orange": "#FF9830", orange: "#FF780A", "semi-dark-orange": "#FA6400", "dark-orange": "#E55400",
  "super-light-yellow": "#FFEE52", "light-yellow": "#FADE2A", yellow: "#F2CC0C", "semi-dark-yellow": "#E0B400", "dark-yellow": "#CC9D00",
  "super-light-green": "#96D98D", "light-green": "#73BF69", green: "#56A64B", "semi-dark-green": "#37872D", "dark-green": "#19730E",
  "super-light-blue": "#8AB8FF", "light-blue": "#5794F2", blue: "#3274D9", "semi-dark-blue": "#1F60C4", "dark-blue": "#1250B0",
  "super-light-purple": "#CA95E5", "light-purple": "#B877D9", purple: "#A352CC", "semi-dark-purple": "#8F3BB8", "dark-purple": "#7C2EA3",
};

test("FE-07 the light fill table has the 30 shades of Grafana, and the dark table stays", () => {
  assert.equal(Object.keys(NAMED_LIGHT).length, 30);
  for (const [k, hex] of Object.entries(LIGHT_PROBED)) assert.equal(NAMED_LIGHT[k].toUpperCase(), hex, k);
  // The light shade of a token is the dark-theme hex of the next darker step of its hue; `dark-*` has a shade of its own.
  for (const hue of ["red", "orange", "yellow", "green", "blue", "purple"]) {
    assert.equal(NAMED_LIGHT[`super-light-${hue}`], NAMED[`light-${hue}`]);
    assert.equal(NAMED_LIGHT[`light-${hue}`], NAMED[hue]);
    assert.equal(NAMED_LIGHT[hue], NAMED[`semi-dark-${hue}`]);
    assert.equal(NAMED_LIGHT[`semi-dark-${hue}`], NAMED[`dark-${hue}`]);
    assert.notEqual(NAMED_LIGHT[`dark-${hue}`], NAMED[`dark-${hue}`]);
  }
});

test("FE-07 resolveFill and fillOf: the light theme only changes a named token", () => {
  assert.equal(resolveFill("green"), "#73BF69");
  assert.equal(resolveFill("green", true), "#73BF69");
  assert.equal(resolveFill("green", false), "#56A64B");
  assert.equal(resolveFill("#73BF69", false), "#73BF69", "a hex of the document is not a token");
  assert.equal(resolveFill("rgb(1, 2, 3)", false), "rgb(1, 2, 3)");
  assert.equal(resolveFill("text", false), "var(--text)");
  assert.equal(resolveFill("transparent", false), "transparent");
  const tok = { color: "#73BF69", mark: "var(--viz-green)" };
  const lit = { color: "#73BF69", mark: "#73BF69" };
  assert.equal(fillOf(tok, true), "#73BF69");
  assert.equal(fillOf(tok, false), "#56A64B");
  assert.equal(fillOf(lit, false), "#73BF69");
  assert.equal(fillOf({ color: null, mark: null }, false), null);
  assert.equal(fillOf({ color: "var(--text)", mark: "var(--text)" }, false), "var(--text)");
});

test("FE-07 gradient of a cell: the light theme raises the lightness by 7 points (Grafana)", () => {
  assert.equal(gradientFrom("#FF7383", false), "linear-gradient(120deg, rgb(255, 151, 154), #FF7383)");
  assert.equal(gradientFrom("#F2495C", false), "linear-gradient(120deg, rgb(244, 106, 110), #F2495C)");
  assert.equal(gradientFrom("#FFB357", false), "linear-gradient(120deg, rgb(255, 206, 123), #FFB357)");
  assert.equal(gradientFrom("#F2495C", true), gradientFrom("#F2495C"), "the dark theme is the default");
});

// ---- The series palette of ClickHouse Dash ----

const TOKENS = readFileSync(new URL("../../web/css/vendor/chdash/00-tokens.css", import.meta.url), "utf8");
const slotsOf = (css) => [...css.matchAll(/--qchart-(\d+|other): (#[0-9a-f]{6});/g)].map((m) => [m[1], m[2]]);
const tokenBlock = (name) => {
  // The block `html[data-theme="<name>"] { ... }` of the copied tokens.
  const at = TOKENS.indexOf(`html[data-theme="${name}"] {`);
  return TOKENS.slice(at, TOKENS.indexOf("\n}", at));
};
const panelOf = (name) => /--panel: (#[0-9a-f]{6});/.exec(tokenBlock(name))[1];
const pageOf = (name) => /--bg: (#[0-9a-f]{6});/.exec(tokenBlock(name))[1];
const ratio = (a, b) => (Math.max(relativeLuminance(a), relativeLuminance(b)) + 0.05) / (Math.min(relativeLuminance(a), relativeLuminance(b)) + 0.05);

test("the series palette is the 18 tokens --qchart-1 to --qchart-18 of ClickHouse Dash, in both themes", () => {
  assert.equal(QCHART_COUNT, 18);
  for (const theme of ["dark", "light"]) {
    const slots = slotsOf(tokenBlock(theme));
    assert.equal(slots.length, 19, `${theme}: 18 slots and other`);
    assert.deepEqual(slots.slice(0, 18).map(([, c]) => c), [...QCHART[theme]], theme);
    assert.deepEqual(slots[18], ["other", QCHART.other[theme]], theme);
    assert.equal(new Set(QCHART[theme]).size, 18, `${theme}: no colour twice`);
  }
});

test("each colour of the series palette has 3:1 with the panel in both themes (and with the page)", () => {
  for (const theme of ["dark", "light"]) {
    const panel = panelOf(theme);
    const page = pageOf(theme);
    assert.equal(panel, theme === "dark" ? "#13161a" : "#ffffff");
    for (const [i, c] of [...QCHART[theme], QCHART.other[theme]].entries()) {
      assert.ok(ratio(c, panel) >= 3, `${theme} qchart-${i + 1} ${c} on the panel ${panel}: ${ratio(c, panel).toFixed(2)}`);
      assert.ok(ratio(c, page) >= 3, `${theme} qchart-${i + 1} ${c} on the page ${page}: ${ratio(c, page).toFixed(2)}`);
    }
  }
});

test("the mode palette gives slot i mod 18 in the theme of the context, negative gives other, a named colour stays", () => {
  assert.equal(qchartColor(0), QCHART.dark[0]);
  assert.equal(qchartColor(17, false), QCHART.light[17]);
  assert.equal(qchartColor(18), QCHART.dark[0]);
  assert.equal(qchartColor(41, false), QCHART.light[5]);
  assert.equal(qchartColor(-1), QCHART.other.dark);
  assert.equal(qchartColor(Number.NaN, false), QCHART.other.light);
  assert.equal(qchartColor(2.9), QCHART.dark[2]);
  // The explicit colours are not touched: a fixed colour, a threshold step and a mapping keep their token.
  assert.equal(fieldColor({ color: { mode: "fixed", fixed: "red" } }, 1, { seriesIndex: 3 }), NAMED.red);
});
