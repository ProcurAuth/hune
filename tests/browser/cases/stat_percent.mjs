// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B222 (4.29, 9.19): the percent change and the text sizes of a stat, in both themes, against the values that
// Grafana 12.4.2 gives (tests/frontend/vectors/FE-82-stat-percent.json). The sizes follow from the box of each tile, as the rules of 9.19 say.
import { doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const UID = "b222-stat-percent";
const sql = (values) => [`SELECT toDateTime(1794211200, 'UTC') + number * 60 AS time, [${values.join(", ")}][number + 1] AS v FROM numbers(${values.length})`];
const g = (x, y, w, h) => ({ x, y, w, h });
const steps = (base, above) => ({ defaults: { color: { mode: "thresholds", fixed: null }, thresholds: { mode: "absolute", steps: [{ value: null, color: base }, { value: 110, color: above }] } }, overrides: [] });
const RED_ABOVE_110 = steps("blue", "red");
const stat = (id, title, values, grid, options = {}, fields) => panel(id, "stat", title, sql(values), { grid, options: { percent_change: true, ...options }, fields });

const DOC = doc(UID, "Stat percent change", [
  section(null, [
    stat(1, "Up", [100, 120], g(0, 0, 6, 6)),
    stat(2, "Down", [120, 100], g(6, 0, 6, 6)),
    stat(3, "Zero", [100, 100], g(12, 0, 6, 6)),
    stat(4, "Nothing", [0, 0], g(18, 0, 6, 6)),
    stat(5, "Inverted", [100, 120], g(0, 6, 6, 6), { percent_change_color: "inverted" }),
    stat(6, "Same", [100, 120], g(6, 6, 6, 6), { percent_change_color: "same_as_value" }, RED_ABOVE_110),
    stat(7, "Filled", [120, 100], g(12, 6, 6, 6), { color_mode: "background" }, steps("dark-blue", "dark-blue")),
    stat(8, "Off", [100, 120], g(18, 6, 6, 6), { percent_change: false }),
    stat(9, "Wide", [100, 120], g(0, 12, 24, 3), { text_mode: "value_and_name" }),
    stat(10, "Sizes", [100, 120], g(0, 15, 6, 6), { text_mode: "value_and_name", title_size: 20, value_size: 40 }),
    stat(11, "Name", [100, 120], g(6, 15, 6, 6), { text_mode: "name" }),
    stat(12, "None", [100, 120], g(12, 15, 6, 6), { text_mode: "none" }),
    stat(13, "Rows", [100, 120], g(18, 15, 6, 6), { reduce: { calc: "last_not_null", fields: null, values: true } }),
  ]),
]);

// Grafana 12.4.2: the named colours `green` and `red` of each theme (FE-82).
const GREEN = { dark: "rgb(115, 191, 105)", light: "rgb(86, 166, 75)" };
const RED = { dark: "rgb(242, 73, 92)", light: "rgb(224, 47, 68)" };

// The tiles of the page: the box, the texts, the sizes and the colours.
const PROBE = `(() => {
  const out = {};
  const px = (v) => parseFloat(v) || 0;
  for (const tile of document.querySelectorAll('[data-testid^="stat-item-"]')) {
    const id = tile.dataset.testid.slice(10);
    const box = tile.getBoundingClientRect();
    const cs = getComputedStyle(tile);
    const rel = (e) => { const r = e.getBoundingClientRect(); return { x: r.left - box.left, y: r.top - box.top, w: r.width, h: r.height }; };
    const value = tile.querySelector(".stat__value");
    const title = tile.querySelector(".stat__title");
    const pct = tile.querySelector(".stat__pct");
    const item = {
      w: box.width, h: box.height, pad: px(cs.paddingTop), layout: tile.dataset.layout, align: tile.dataset.align, ink: cs.color,
      rowGap: px(getComputedStyle(tile.querySelector(".stat__box")).rowGap),
      value: { shown: !value.hidden, text: value.textContent, size: px(getComputedStyle(value).fontSize), color: getComputedStyle(value).color, box: rel(value) },
      title: { shown: !title.hidden, size: px(getComputedStyle(title).fontSize), box: rel(title.firstChild) },
      pct: null,
    };
    if (pct) {
      const ps = getComputedStyle(pct);
      const svg = pct.querySelector("svg");
      const span = pct.querySelector("span");
      item.pct = {
        shown: !pct.hidden, text: span.textContent, size: px(ps.fontSize), color: ps.color, gap: px(ps.columnGap), top: px(ps.marginTop), lineHeight: px(ps.lineHeight),
        align: ps.alignItems, box: rel(pct), textBox: rel(span),
        arrow: svg ? { w: svg.getBoundingClientRect().width, h: svg.getBoundingClientRect().height, name: svg.querySelector("use").getAttribute("href"), stroke: getComputedStyle(svg).stroke } : null,
        // Only an arrow and a span of text: the text of the change is a text node.
        kids: [...pct.children].map((k) => k.tagName.toLowerCase()), spanNodes: [...span.childNodes].map((n) => n.nodeType),
      };
    }
    (out[id] = out[id] || []).push(item);
  }
  return out;
})()`;

const near = (a, b, tol = 0.6) => Math.abs(a - b) <= tol;

export const cases = [
  {
    id: "B222",
    name: "the percent change and the text sizes of a stat: the texts, the arrows, the colours of each mode, the sizes and the places of Grafana 12.4.2, in both themes (4.29, 9.19)",
    async run(t) {
      const r = await t.api.create("transit", DOC);
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 400)}`);
      for (const theme of ["dark", "light"]) {
        const p = await t.page({ width: 1400, height: 1100 });
        await p.emulateColorScheme(theme);
        await p.goto(t.url(`d/${UID}/x`));
        await p.idle();
        await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]);
        t.eq(await p.eval("document.documentElement.dataset.theme"), theme, `the page uses the ${theme} theme`);
        // A font can arrive after the first draw: wait until two reads agree.
        let tiles = await p.eval(PROBE);
        for (let i = 0; i < 20; i++) {
          await p.eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))");
          const again = await p.eval(PROBE);
          if (JSON.stringify(again) === JSON.stringify(tiles)) break;
          tiles = again;
        }
        const one = (id) => tiles[id][0];
        const T = (s) => `${theme}: ${s}`;

        // The texts and the arrows (FE-82: P01, P02, P03, P05).
        t.eq(one(1).value.text, "120", T("the value of a rise"));
        t.eq(one(1).pct.text, "20%", T("100 then 120 gives 20%"));
        t.assert(one(1).pct.arrow && one(1).pct.arrow.name.endsWith("arrow-up"), T(`a rise has an arrow up: ${JSON.stringify(one(1).pct.arrow)}`));
        t.eq(one(2).pct.text, "-16.7%", T("120 then 100 gives -16.7%"));
        t.assert(one(2).pct.arrow && one(2).pct.arrow.name.endsWith("arrow-down"), T("a fall has an arrow down"));
        t.eq(one(3).pct.text, "0%", T("no change gives 0%"));
        t.eq(one(3).pct.arrow, null, T("no change has no arrow"));
        t.eq(one(4).pct.shown, false, T("0 then 0 shows no change"));
        for (const id of [1, 2, 3]) {
          t.eq(JSON.stringify(one(id).pct.kids), JSON.stringify(one(id).pct.arrow ? ["svg", "span"] : ["span"]), T(`panel ${id}: an arrow and a text only`));
          t.eq(JSON.stringify(one(id).pct.spanNodes), "[3]", T(`panel ${id}: the text is a text node`));
        }
        // An item of each row has no change, and a stat with the option off has none.
        t.eq(tiles[13].length, 2, T("two rows give two tiles"));
        t.assert(tiles[13].every((x) => x.pct === null), T("an item of each row has no change"));
        t.eq(one(8).pct, null, T("percent_change false shows no change"));

        // The colours of each mode (FE-82). The change of these tiles is 24 px or more: Grafana's colours stay in both themes.
        t.assert(one(1).pct.size >= 24, T(`the change of panel 1 is a large text: ${one(1).pct.size}`));
        t.eq(one(1).pct.color, GREEN[theme], T("standard: a rise is green"));
        t.eq(one(2).pct.color, RED[theme], T("standard: a fall is red"));
        t.eq(one(3).pct.color, RED[theme], T("standard: no change is red"));
        t.eq(one(5).pct.color, RED[theme], T("inverted: a rise is red"));
        t.eq(one(6).value.color, RED[theme], T("the value above 110 is red"));
        t.eq(one(6).pct.color, one(6).value.color, T("same_as_value: the change has the colour of the value"));
        t.eq(one(7).pct.color, one(7).ink, T("a filled tile gives its ink to the change"));
        t.eq(one(1).pct.arrow.stroke, one(1).pct.color, T("the arrow has the colour of the text"));

        // The sizes in a stacked tile (Grafana: 72.5, 29, 24.17, 9.67 and -7.25 px in a tile of 253 x 140).
        for (const id of [1, 2, 3, 4, 5, 6, 7]) {
          const x = one(id);
          t.eq(x.layout, "stacked", T(`panel ${id} is stacked`));
          const value = ((x.h - 2 * x.pad) * 0.75) / 1.2;
          t.assert(near(x.value.size, value, 0.3), T(`panel ${id}: the value gets 0.75 of the height (${x.value.size}, want ${value})`));
          if (!x.pct.shown) continue;
          t.assert(near(x.pct.size, x.value.size / 2.5, 0.05), T(`panel ${id}: the change is 1/2.5 of the value (${x.pct.size})`));
          t.assert(near(x.pct.gap, x.pct.size / 3, 0.05) && near(x.pct.top, -x.pct.size / 4, 0.05), T(`panel ${id}: the gap and the lift (${x.pct.gap}, ${x.pct.top})`));
          if (x.pct.arrow) t.assert(near(x.pct.arrow.w, x.value.size / 3, 0.1) && near(x.pct.arrow.h, x.value.size / 3, 0.1), T(`panel ${id}: the arrow is 1/3 of the value`));
          // Under the value (after the gap of 0.75ch of a centred box), centred like it (no name).
          t.assert(near(x.pct.box.y, x.value.box.y + x.value.box.h + x.rowGap - x.pct.size / 4, 1), T(`panel ${id}: the change is under the value (${x.pct.box.y})`));
          t.assert(near(x.pct.box.x + x.pct.box.w / 2, x.w / 2, 1.5), T(`panel ${id}: the change is centred`));
        }
        // A tile of 178 px has the height of the stacked tiles of the measure: the places of Grafana 12.4.2 (S13: 9.67 and 122.16 px; S05: 45.41,
        // 69.41 and 113.41 px from the top of the tile).
        if (near(one(1).h, 178, 0.01)) {
          t.assert(near(one(1).value.box.y, 9.67, 0.5) && near(one(1).pct.box.y, 122.16, 0.5), T(`the places of Grafana in a tile of 178 px: ${one(1).value.box.y}, ${one(1).pct.box.y}`));
          const f = one(10);
          t.assert(near(f.title.box.y, 45.41, 0.5) && near(f.value.box.y, 69.41, 0.5) && near(f.pct.box.y, 113.41, 0.5), T(`fixed sizes: the places of Grafana: ${f.title.box.y}, ${f.value.box.y}, ${f.pct.box.y}`));
        } else t.note(`${theme}: the stacked tiles are ${one(1).h} px high, not 178: the places of Grafana are not compared`);
        t.assert(near(one(8).value.size, ((one(8).h - 2 * one(8).pad)) / 1.2, 0.3), T("with no change, the value takes the whole height"));

        // A wide tile with a name: the name at the left, the value in the middle, the change at the right, on the base line, line height 3.
        const w = one(9);
        t.eq(w.layout, "wide", T("panel 9 is wide"));
        t.assert(near(w.title.box.x, w.pad, 1), T(`the name starts at the padding (${w.title.box.x})`));
        t.assert(near(w.pct.box.x + w.pct.box.w, w.w - w.pad, 1), T(`the change ends at the padding (${w.pct.box.x + w.pct.box.w})`));
        t.assert(w.value.box.x > w.title.box.x + w.title.box.w && w.value.box.x + w.value.box.w < w.pct.box.x, T("the value is between the name and the change"));
        t.eq(w.pct.align, "baseline", T("the change stands on the base line"));
        t.assert(near(w.pct.lineHeight, 3 * w.pct.size, 0.1) && w.pct.top === 0, T(`line height 3 and no lift (${w.pct.lineHeight}, ${w.pct.top})`));
        t.assert(near(w.pct.size, Math.max(w.value.size / 2.5, 12), 0.05), T("the change of a wide tile is 1/2.5 of the value, 12 px at least"));

        // Fixed sizes (Grafana S05: 20, 40, 16, 13.33, 5.33 and -4 px).
        const s = one(10);
        t.eq([s.title.size, s.value.size, s.pct.size].join(), "20,40,16", T("title_size 20 and value_size 40 give 20, 40 and 16 px"));
        t.assert(near(s.pct.arrow.w, 13.333, 0.05) && near(s.pct.gap, 5.333, 0.05) && near(s.pct.top, -4, 0.05), T("the arrow, the gap and the lift follow the fixed value"));

        // The text mode name: the name in the big slot, with the change under it. The text mode none: the change alone, 0.8 of the box.
        t.eq(one(11).value.text, "v", T("the name of the field is in the big slot"));
        t.eq(one(11).pct.text, "20%", T("the name shows its change"));
        const n = one(12);
        t.eq(n.value.shown, false, T("the mode none shows no value"));
        const fit = Math.min(((n.h - 2 * n.pad) * 0.8) / 1.2, ((n.w - 2 * n.pad) * 0.8 * 0.985 * 100) / ((n.pct.textBox.w / n.pct.size) * 100));
        t.assert(near(n.pct.size, fit, 0.5), T(`the change fills 0.8 of the box (${n.pct.size}, want ${fit})`));
        t.assert(near(n.pct.arrow.w, n.pct.size * 0.8, 0.1), T("its arrow is 0.8 of its size"));
      }
    },
  },
];
