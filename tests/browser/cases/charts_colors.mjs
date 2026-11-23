// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B62 (16.10): the colour modes of the second set (the Grafana-parity work: F26): the one series palette (`palette`) and the continuous modes.
// The colours of the continuous modes are those that Grafana 12.4.2 draws (paper 10.8, V5, tests/frontend/vectors/FE-23-color-modes.json).
// The colours of `palette` are the one series palette of ClickHouse Dash, with a dark and a light variant.
// The removed mode `palette_by_name` gives `bad_format` (4.19).
import { doc, panel, section } from "../lib/docs.mjs";
import { reveal } from "../lib/h.mjs";

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const g = (x, y, w, h) => ({ x, y, w, h });

// The series names of panels 1 and 2: panel 2 lists them in reverse order. In the mode `palette`, a series takes the slot of its place.
const NAMES = ["a", "c", "db", "api", "web-1", "cache", "über"];
// The series palette, slots 0 to 6 of each theme (`--qchart-1` to `--qchart-7`).
const PALETTE7 = {
  dark: ["#4296fb", "#e86a34", "#29ae81", "#d28f09", "#de669a", "#4ba435", "#9e8cf4"],
  light: ["#1a73d5", "#c14802", "#088963", "#9c6900", "#b84379", "#227702", "#644fb1"],
};
// The series palette: slots 0 to 3 of each theme (`--qchart-1` to `--qchart-4`).
const SLOTS = { dark: ["#4296fb", "#e86a34", "#29ae81", "#d28f09"], light: ["#1a73d5", "#c14802", "#088963", "#9c6900"] };
const STOPS = {
  dark: ["#73BF69", "#B4CB4C", "#E2C03D", "#F39045", "#F2495C"],
  light: ["#56A64B", "#A0B52E", "#D5AC20", "#E5792A", "#E02F44"],
};

const rgbOf = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
const parseRgb = (text) => (text.match(/\d+(\.\d+)?/g) || []).slice(0, 3).map(Number);
const near = (a, b, tol) => a.length === 3 && b.length === 3 && a.every((v, i) => Math.abs(v - b[i]) <= tol);

export const cases = [
  {
    id: "B62",
    name: "colour modes: the one series palette gives each series the slot of its place and follows the theme; palette_by_name is refused; continuous modes show the stops, null shows the first stop; both themes",
    async run(t) {
      const names = NAMES;
      const col = (name, k) => `toFloat64(number + ${k}) AS \`${name}\``;
      const sql = (list) => ["SELECT toDateTime(1794211200 + number * 60) AS t, " + list.map((n, k) => col(n, k)).join(", "), "FROM numbers(20)"];
      const palette = { defaults: { color: { mode: "palette", fixed: null } }, overrides: [] };
      const ramp = (name, v) => `toFloat64(${v}) AS ${name}`;
      const rampSql = ["SELECT toDateTime(1794211200 + number * 60) AS t, " + [ramp("s0", 0), ramp("s1", 25), ramp("s2", 50), ramp("s3", 75), ramp("s4", 100), "toFloat64OrNull('x') AS sn"].join(", "), "FROM numbers(20)"];
      const continuous = { defaults: { min: 0, max: 100, color: { mode: "continuous_gr_yl_rd", fixed: null } }, overrides: [] };
      const statSql = ["SELECT toFloat64(0) AS lo, toFloat64(100) AS hi, toFloat64OrNull('x') AS nothing"];
      const d = doc("color-modes", "Colour modes", [
        section(null, [
          panel(1, "timeseries", "Series palette", sql(names), { grid: g(0, 0, 12, 8), fields: palette, options: { legend: { show: true, placement: "bottom", mode: "list", values: [] } } }),
          panel(2, "timeseries", "Series palette, reverse order", sql([...names].reverse()), { grid: g(12, 0, 12, 8), fields: palette }),
          panel(3, "timeseries", "Continuous", rampSql, { grid: g(0, 8, 12, 8), fields: continuous }),
          panel(5, "timeseries", "Default colours", sql(["p0", "p1", "p2", "p3"]), { grid: g(0, 16, 12, 8), options: { legend: { show: true, placement: "bottom", mode: "list", values: [] } } }),
          panel(6, "timeseries", "Fixed colour on the second series", sql(["q0", "q1", "q2", "q3"]), {
            grid: g(12, 16, 12, 8),
            fields: { defaults: {}, overrides: [{ match: { by: "name", value: "q1" }, set: { color: { mode: "fixed", fixed: "purple" } } }] },
            options: { legend: { show: true, placement: "bottom", mode: "list", values: [] } },
          }),
          panel(4, "stat", "Stat", statSql, {
            grid: g(12, 8, 12, 4),
            fields: continuous,
            options: { reduce: { calc: "last", fields: null, values: false }, color_mode: "background", graph_mode: "none", text_mode: "value_and_name", orientation: "horizontal" },
          }),
        ]),
      ]);
      await ensure(t, d);

      // The removed mode palette_by_name gives bad_format: the document is not saved.
      const removed = doc("color-modes-removed", "Removed colour mode", [
        section(null, [panel(1, "timeseries", "Removed", sql(["a", "b"]), { grid: g(0, 0, 12, 8), fields: { defaults: { color: { mode: "palette_by_name", fixed: null } }, overrides: [] } })]),
      ]);
      const refused = await t.api.create("transit", removed);
      t.assert(refused.status >= 400 && refused.status < 500, `palette_by_name is refused (status ${refused.status})`);
      t.assert(/bad_format/.test(refused.text) && /palette/.test(refused.text), `the error is bad_format and names the palette (${refused.text.slice(0, 300)})`);

      for (const theme of ["dark", "light"]) {
        const p = await t.page();
        await p.emulateColorScheme(theme);
        await p.goto(t.dash("color-modes"));
        await p.idle();
        await reveal(p, [1, 2, 3, 4, 5, 6]);
        const dataTheme = await p.eval("document.documentElement.dataset.theme");
        t.eq(dataTheme, theme, `the page uses the ${theme} theme`);
        const swatches = (id) => p.eval(`(() => {
          const out = {};
          for (const b of document.querySelectorAll('[data-testid="panel-${id}"] .chartCore__legendItem')) {
            out[b.querySelector("span").textContent] = getComputedStyle(b.querySelector("i")).backgroundColor;
          }
          return out;
        })()`);

        // The mode palette: a series takes the slot of its place, so the order of the series changes the colours.
        const first = await swatches(1);
        const second = await swatches(2);
        t.eq(Object.keys(first).sort(), [...names].sort(), `${theme}: panel 1 has each series`);
        const reversed = [...names].reverse();
        names.forEach((name, k) => {
          t.assert(near(parseRgb(first[name]), rgbOf(PALETTE7[theme][k]), 1), `${theme}: ${name} (place ${k}) is ${PALETTE7[theme][k]} (got ${first[name]})`);
          const k2 = reversed.indexOf(name);
          t.assert(near(parseRgb(second[name]), rgbOf(PALETTE7[theme][k2]), 1), `${theme}: ${name} (place ${k2} in the reverse order) is ${PALETTE7[theme][k2]} (got ${second[name]})`);
        });

        // The default colour of a time series (no colour set) is the mode `palette`: slot i of the series palette in the theme of the page.
        const dflt = await swatches(5);
        ["p0", "p1", "p2", "p3"].forEach((name, k) => {
          t.assert(near(parseRgb(dflt[name]), rgbOf(SLOTS[theme][k]), 1), `${theme}: the default colour of ${name} is ${SLOTS[theme][k]} (got ${dflt[name]})`);
        });
        // An explicit colour keeps its value: a fixed named colour is not the palette; the other series keep their slot.
        const fixed = await swatches(6);
        t.assert(!near(parseRgb(fixed.q1), rgbOf(SLOTS[theme][1]), 8), `${theme}: the series with a fixed colour does not take the palette (got ${fixed.q1})`);
        t.assert(near(parseRgb(fixed.q0), rgbOf(SLOTS[theme][0]), 1) && near(parseRgb(fixed.q2), rgbOf(SLOTS[theme][2]), 1), `${theme}: the other series keep their slots (got ${fixed.q0}, ${fixed.q2})`);

        // A continuous mode: the colour of the last value, at 0, 25, 50, 75 and 100 per cent of the scale; null gives the first stop.
        const ramp3 = await swatches(3);
        ["s0", "s1", "s2", "s3", "s4"].forEach((name, k) => {
          t.assert(near(parseRgb(ramp3[name]), rgbOf(STOPS[theme][k]), 1), `${theme}: ${name} is ${STOPS[theme][k]} (got ${ramp3[name]})`);
        });
        t.assert(near(parseRgb(ramp3.sn), rgbOf(STOPS[theme][0]), 1), `${theme}: a series with no value has the first stop (got ${ramp3.sn})`);

        // A stat with the mode: the tile of 0 is green, the tile of 100 is red, a value that is null has the first stop (green).
        const tiles = await p.eval(`[...document.querySelectorAll('[data-testid^="stat-item-4"]')].map((e) => ({ text: e.textContent, bg: getComputedStyle(e).backgroundImage }))`);
        t.assert(tiles.length === 3, `${theme}: the stat has three tiles (${tiles.length})`);
        const mean = (bg) => {
          const nums = (bg.match(/rgba?\(([^)]+)\)/g) || []).map((c) => parseRgb(c));
          return [0, 1, 2].map((i) => nums.reduce((s, n) => s + n[i], 0) / Math.max(1, nums.length));
        };
        const [lo, hi, none] = tiles.map((x) => mean(x.bg));
        t.assert(lo[1] > lo[0] && hi[0] > hi[1], `${theme}: the tile of 0 is green and the tile of 100 is red: ${JSON.stringify([lo, hi])}`);
        t.assert(none[1] > none[0], `${theme}: a tile with no value has the first stop (green): ${JSON.stringify(none)}`);
      }
    },
  },
];
