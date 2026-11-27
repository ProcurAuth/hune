// The stat panel (9.19, 4.29). The layout follows the layout of Grafana 12.4.2, measured on its demo panels:
// one box for each item (a "tile"), a padding of 12 px (8 px under 100 px of height), and two layouts.
// A wide tile (width above 2.5 times its height) puts the name at the left and the value at the right.
// A stacked tile puts the name above the value. The value is fitted to its box, and its unit is smaller.

import { fieldColorValue, fillOf, resolveFill } from "../colors.js";
import { el, icon } from "../ui.js";
import { chartUi } from "../vendor/chdash/app_ui_chart.js";
import { fitFontSize, inkOn, legibleOn, reduceItems, statFill, unitSizeOf } from "./items.js";
import { canvasColor } from "./common.js";
import { floorUnit, innerSize } from "./box.js";
import { percentDir, percentHue, percentOf, percentSizes, percentText } from "./stat_percent.js";

const TEXT_CONTRAST = 4.5; // the least contrast of a value colour with the panel (WCAG AA text)
const LARGE_CONTRAST = 3; // the same for a large text (WCAG 1.4.3): 24 px and more at weight 500
const LARGE_PX = 24;
const LINE = 1.2; // line height (em) of the value and of the name
const FILL_WIDTH = 0.985; // part of the box width that the text with its unit at full size may use (measured in Grafana)
const WIDE_RATIO = 2.5; // a tile is wide above this ratio of width to height
const MAX_TITLE = 30; // the largest size (px) of a name
const GAP = 3; // px between two tiles
const MIN_SIZE = 6;
const MIN_TILE = { column: 24, row: 56 }; // the least size (px) of a tile: the panel scrolls before a tile gets smaller

/** The padding of a tile: 12 px, or 8 px for a tile of 100 px or less in height (Grafana). */
export const tilePadding = (h) => (h > 100 ? 12 : 8);

/**
 * The size of each text of a tile (pure). `w100` and `t100` are the widths (px) of the value text (with its unit at the full size)
 * and of the name at a font size of 100 px; `t100` is 0 for a tile with no name. `chart` says that the tile has a graph.
 * `pct`: the tile has a percent change (4.29), so a value with no graph gets 0.75 of the height. `titleFix`, `valueFix`: `title_size` and
 * `value_size` (px) or null; a fixed name takes no room from the value. `p100`: the change text at 100 px in the text mode `none`, else 0.
 * Gives { wide, padding, valueSize, titleSize, unitSize, chartHeight, center, pct }: `pct` is { size, icon, gap, lift } or null.
 */
export function tileLayout({ w, h, w100, t100 = 0, chart = false, justify = "auto", pct = false, titleFix = null, valueFix = null, p100 = 0 }) {
  const padding = tilePadding(h);
  const maxW = w - 2 * padding;
  const maxH = h - 2 * padding;
  const hasTitle = t100 > 0;
  const wide = w / h > WIDE_RATIO;
  const center = justify === "center" || !hasTitle;
  const withChart = chart && (!wide || h > 50);
  const k = pct && !withChart ? 0.75 : 1;
  const fit = (x, bw, bh, max) => fitFontSize(x, bw, bh, { fillW: FILL_WIDTH, fillH: 1, lineHeight: LINE, min: MIN_SIZE, max });
  let valueSize;
  let titleSize = 0;
  let chartHeight = 0;
  if (wide) {
    const share = hasTitle && titleFix === null ? 0.3 : withChart ? 0.4 : 1;
    valueSize = valueFix ?? fit(w100, maxW * share, maxH * (withChart ? 0.5 : k));
    if (hasTitle) titleSize = titleFix ?? Math.min(valueSize * 0.7, MAX_TITLE);
    if (withChart) chartHeight = Math.floor(h / 2);
  } else {
    if (hasTitle) titleSize = titleFix ?? fit(t100, maxW, maxH * 0.15, MAX_TITLE);
    valueSize = valueFix ?? fit(w100, maxW, (maxH - (titleFix === null ? titleSize * LINE : 0)) * k);
    if (withChart) chartHeight = Math.max(0, Math.floor(h - (valueSize + titleSize) * LINE));
  }
  const unitSize = unitSizeOf(valueSize);
  let sizes = null;
  if (pct) {
    // Text mode `none`: the change fills 0.8 of the box, its arrow is 0.8 of its size; the gap and the lift are those of a one-digit value.
    // A wide tile puts the change on the base line of the value, with no lift.
    const s = percentSizes(p100 > 0 ? valueFix ?? (maxH * k) / LINE : valueSize);
    const size = p100 > 0 ? fit(p100, maxW * 0.8, maxH * 0.8) : s.size;
    sizes = { size, icon: p100 > 0 ? size * 0.8 : s.icon, gap: s.gap, lift: wide ? 0 : s.lift };
  }
  return { wide, padding, valueSize, titleSize, unitSize, chartHeight, center, pct: sizes };
}

/**
 * The direction and the size of the tiles of a panel (pure). `orientation` is the option of the document, with the meaning of
 * Grafana: `horizontal` stacks the tiles one below the other (each tile is a wide bar), `vertical` puts them side by side, and
 * `auto` puts them side by side when the box is wider than it is high.
 */
export function tileGrid(n, w, h, orientation) {
  const row = orientation === "auto" ? w > h : orientation === "vertical";
  const dir = row ? "row" : "column";
  if (n <= 1) return { dir, tileW: w, tileH: h };
  const gaps = GAP * (n - 1);
  // Each share rounds down to the layout unit: n tiles and their gaps never take more than the box (9.19a).
  if (row) return { dir, tileW: Math.max(MIN_TILE.row, floorUnit((w - gaps) / n)), tileH: h };
  return { dir, tileW: w, tileH: Math.max(MIN_TILE.column, floorUnit((h - gaps) / n)) };
}

// The arrow of a change: the sprite arrow, cut to the box of its strokes so that it fills its size, as in Grafana (CSS `.stat__pct`).
function arrow(dir) {
  const a = icon(`arrow-${dir}`);
  a.setAttribute("viewBox", "4 4 16 16");
  for (const k of ["width", "height"]) a.firstChild.setAttribute(k, "24");
  return a;
}

// The text of the big slot: number, with the unit (prefix, suffix) in smaller spans.
function fillValue(valueEl, text, parts) {
  const inner = el("span", { class: "stat__text" });
  if (parts) {
    if (parts.prefix) inner.append(el("span", { class: "stat__unit", text: parts.prefix }));
    inner.append(el("span", { class: "stat__num", text: parts.text }));
    if (parts.suffix) inner.append(el("span", { class: "stat__unit", text: parts.suffix }));
  } else inner.append(el("span", { class: "stat__num", text }));
  valueEl.replaceChildren(inner);
  return inner;
}

export function render(host, ctx) {
  const { panel, fields } = ctx;
  const o = panel.options;
  const items = reduceItems(ctx, o.reduce);
  const noValueText = fields[0] && fields[0].config.no_value;
  const root = el("div", { class: "stat", dataset: { testid: `stat-${panel.id}`, colorMode: o.color_mode } });
  const timeField = fields.find((f) => f.kind === "time");
  const many = items.length >= 2;
  const mode = o.text_mode;
  const showName = mode === "value_and_name" || (mode === "auto" && many);
  const nameAlone = mode === "name";
  const showValue = mode !== "name" && mode !== "none";
  const justify = o.justify || "auto";
  const tiles = [];
  for (const item of items) {
    const isNull = item.display.empty;
    const fallback = isNull && !noValueText && item.display.text === "";
    const valueText = fallback ? "No data" : item.text;
    const big = nameAlone ? item.name : valueText;
    const titleEl = el("div", { class: "stat__title" }, el("span", { class: "stat__title-text", text: item.name }));
    const valueEl = el("div", { class: "stat__value statTile__value" });
    const hasTitle = showName && !nameAlone;
    if (!hasTitle) titleEl.hidden = true;
    const textEl = fillValue(valueEl, showValue || nameAlone ? big : "", nameAlone || fallback ? null : item.parts);
    if (!showValue && !nameAlone) valueEl.hidden = true;
    // The percent change of a reduced item (4.29). NaN shows nothing, but the value keeps the size that a change gives, as in Grafana.
    const pct = o.percent_change === true && item.row === undefined ? percentOf(item.field) : null;
    const dir = pct === null ? null : percentDir(pct);
    const pctEl = pct === null ? null : el("div", { class: "stat__pct", hidden: Number.isNaN(pct) }, dir ? arrow(dir) : null, el("span", { text: percentText(pct) || "" }));
    const textBox = el("div", { class: "stat__box" }, titleEl, valueEl, pctEl);
    const box = el("div", { class: "stat__item", title: item.name, dataset: { testid: `stat-item-${panel.id}` } });
    let line = null;
    let lineValues = null;
    if (o.graph_mode === "area" && timeField && item.row === undefined && (item.field.kind === "number" || item.field.kind === "number_text")) {
      const src = item.field.kind === "number_text" ? item.field.num : item.field.values;
      const values = Array.from(src, (v, i) => (item.field.nulls && item.field.nulls[i] ? null : v));
      const finite = values.filter((v) => Number.isFinite(v));
      if (finite.length >= 2) {
        let lo = Math.min(...finite);
        let hi = Math.max(...finite);
        if (lo === hi) {
          lo -= 1;
          hi += 1;
        }
        line = el("div", { class: "stat__graph", "aria-hidden": "true" });
        lineValues = { values, lo, hi };
        box.classList.add("stat__item--graph");
      }
    }
    if (line) box.append(line);
    box.append(textBox);
    tiles.push({ item, box, textBox, titleEl, valueEl, textEl, line, lineValues, hasTitle: hasTitle && !titleEl.hidden, fallback, pct, pctEl });
  }
  root.append(...tiles.map((t) => t.box));
  host.append(root);

  const pctMode = o.percent_change_color || "standard";
  const large = (e) => (parseFloat(e.style.fontSize) || 0) >= LARGE_PX;
  // The colour of an item: the colour of its display, else (a value that is null) the base step of the thresholds, as Grafana.
  const colorOf = (t, dark) => {
    const shown = t.item.display;
    const own = fillOf(shown, dark);
    if (own || !shown.empty || (shown.mapped && shown.raw !== null)) return own;
    const cfg = t.item.field.config;
    const c = fieldColorValue(cfg, -Infinity, { min: cfg.min, max: cfg.max, seriesIndex: 0, dark });
    return c === null ? null : resolveFill(c, dark);
  };

  // Colours: they depend on the theme (a colour that is hard to read on the panel is moved toward the text colour).
  function paintColors() {
    const surface = canvasColor(host, "var(--panel)");
    const dark = document.documentElement.dataset.theme !== "light";
    for (const t of tiles) {
      // A named colour has its own shade in the light theme (colors.js NAMED_LIGHT, as Grafana); the document keeps the token.
      const color = colorOf(t, dark);
      t.large = large(t.valueEl);
      t.box.style.background = "";
      t.box.style.color = "";
      t.valueEl.style.color = "";
      t.box.classList.remove("stat__item--filled");
      let lineColor = color || "var(--accent-text)";
      if (color && o.color_mode === "value") {
        lineColor = legibleOn(color, surface, t.large ? LARGE_CONTRAST : TEXT_CONTRAST);
        t.valueEl.style.color = lineColor;
      } else if (color && o.color_mode === "background" && color !== "transparent") {
        // A plain colour gives Grafana's gradient and ink (items.js statFill); a token or a colour with alpha stays flat.
        const fill = statFill(color, dark);
        const ink = fill ? fill.ink : inkOn(color);
        t.box.style.background = fill ? fill.gradient : color;
        t.box.style.color = ink;
        t.box.classList.add("stat__item--filled");
        lineColor = ink;
      } else if (color && o.color_mode === "none") lineColor = "var(--muted)";
      t.lineColor = lineColor;
      // The change has the ink of a filled tile or the value colour (`same_as_value`), else green or red with the contrast rule of the value.
      if (t.pctEl) {
        t.pctLarge = large(t.pctEl);
        const own = t.box.classList.contains("stat__item--filled") || pctMode === "same_as_value";
        t.pctEl.style.color = own ? t.valueEl.style.color : legibleOn(resolveFill(percentHue(t.pct, pctMode), dark), surface, t.pctLarge ? LARGE_CONTRAST : TEXT_CONTRAST);
      }
    }
  }

  function drawLine(t, w) {
    if (!t.line) return;
    t.line.style.setProperty("--stat-line", t.lineColor);
    if (t.drawnWidth === w && t.drawnColor === t.lineColor) return;
    t.drawnWidth = w;
    t.drawnColor = t.lineColor;
    chartUi.sparkline.draw(t.line, t.lineValues.values, { xs: Array.from(timeField.values), area: true, min: t.lineValues.lo, max: t.lineValues.hi });
  }

  // Size: the direction and the box of each tile, then the font of each text.
  function fit() {
    // The real box, not the rounded clientWidth: a rounded-up size makes a scroll bar of less than 1 px (9.19a).
    const { w, h } = innerSize(root);
    if (!(w > 0) || !(h > 0)) return;
    const grid = tileGrid(tiles.length, w, h, o.orientation || "auto");
    root.dataset.orientation = grid.dir;
    root.style.flexDirection = grid.dir;
    root.classList.add("stat--measure");
    // The reads of one kind run together after the writes of that kind: one layout for all tiles, not one for each tile.
    // A tile has its own box, so another tile does not change what a tile reads. The order in a tile stays the same:
    // the value, then the name (with the value at 100 px), then the change (with both at 100 px).
    tiles.forEach((t, index) => {
      const last = index === tiles.length - 1;
      t.box.style.width = `${grid.tileW}px`;
      t.box.style.height = `${grid.tileH}px`;
      t.box.style.flex = "0 0 auto";
      t.box.style.marginRight = !last && grid.dir === "row" ? `${GAP}px` : "";
      t.box.style.marginBottom = !last && grid.dir === "column" ? `${GAP}px` : "";
      // The widths of the texts at 100 px: the value with its unit at the full size, and the name.
      t.valueEl.style.fontSize = "100px";
    });
    const w100 = tiles.map((t) => (t.valueEl.hidden ? 0 : t.textEl.offsetWidth));
    for (const t of tiles) if (t.hasTitle) t.titleEl.style.fontSize = "100px";
    const t100 = tiles.map((t) => (t.hasTitle ? t.titleEl.firstChild.offsetWidth : 0));
    const withPct = (t) => t.pctEl && mode === "none" && !t.pctEl.hidden;
    for (const t of tiles) if (withPct(t)) t.pctEl.style.fontSize = "100px";
    const p100 = tiles.map((t) => (withPct(t) ? t.pctEl.lastChild.offsetWidth : 0));
    tiles.forEach((t, i) => {
      const lay = tileLayout({ w: grid.tileW, h: grid.tileH, w100: w100[i], t100: t100[i], chart: !!t.line, justify, pct: !!t.pctEl, titleFix: o.title_size ?? null, valueFix: o.value_size ?? null, p100: p100[i] });
      if (lay.pct) {
        const s = lay.pct;
        t.pctEl.style.fontSize = `${s.size.toFixed(4)}px`;
        t.pctEl.style.gap = `${s.gap.toFixed(4)}px`;
        t.pctEl.style.marginTop = `${-s.lift.toFixed(4)}px`;
        t.pctEl.style.setProperty("--stat-arrow", `${s.icon.toFixed(4)}px`);
      }
      t.box.style.padding = `${lay.padding}px`;
      t.box.dataset.layout = lay.wide ? "wide" : "stacked";
      t.box.dataset.align = lay.center ? "center" : "start";
      t.valueEl.style.fontSize = `${lay.valueSize.toFixed(4)}px`;
      t.valueEl.style.setProperty("--stat-unit", `${lay.unitSize.toFixed(4)}px`);
      if (t.hasTitle) t.titleEl.style.fontSize = `${lay.titleSize.toFixed(4)}px`;
      if (t.line) {
        t.line.style.width = `${grid.tileW}px`;
        t.line.style.height = `${lay.chartHeight}px`;
        t.line.hidden = !(lay.chartHeight > 0);
        drawLine(t, grid.tileW);
      }
    });
    root.classList.remove("stat--measure");
    // The least contrast of a value colour and of a change colour depends on the size of the text (large text: 3:1).
    if (tiles.some((t) => (o.color_mode === "value" && t.large !== large(t.valueEl)) || (t.pctEl && t.pctLarge !== large(t.pctEl)))) paintColors();
    for (const t of tiles) if (t.line) drawLine(t, grid.tileW);
  }

  paintColors();
  fit();
  const observer = new ResizeObserver(fit);
  observer.observe(host);
  // The font Inter can arrive after the first fit: fit again when a font has loaded (the widths change).
  const onFont = () => fit();
  if (document.fonts) {
    if (document.fonts.ready) document.fonts.ready.then(fit);
    if (document.fonts.addEventListener) document.fonts.addEventListener("loadingdone", onFont);
  }
  const onTheme = () => {
    paintColors();
    fit();
  };
  document.addEventListener("hune-theme", onTheme);
  return {
    destroy() {
      observer.disconnect();
      document.removeEventListener("hune-theme", onTheme);
      if (document.fonts && document.fonts.removeEventListener) document.fonts.removeEventListener("loadingdone", onFont);
      root.remove();
    },
  };
}
