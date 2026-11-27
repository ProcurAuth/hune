// The bar gauge panel (9.19, 4.33): Hune code draws the bars in the DOM. The layout follows Grafana 12.4.2 (measured).
// Modes: basic (one colour), gradient (the threshold colours over the scale, up to the value), lcd (cells).
// A horizontal bar has its name above it when its item is 40 px high or more, else at its left.

import { fillOf, resolveFill, thresholdColor } from "../colors.js";
import { el } from "../ui.js";
import { canvasColor } from "./common.js";
import { legibleOn, reduceItems, stepPercent, unitSizeOf } from "./items.js";
import { floorUnit, innerSize, inputKey } from "./box.js";

const SOFT = 6; // percent of the scale
const CELL = 10; // the size of an LCD cell (px), and the gap between two cells
const CELL_GAP = 2;
const SPACING = { horizontal: 10, vertical: 2 }; // px between two items
const TOP_FROM = 40; // an item of this height (px) or more has its name above the bar
const MAX_VALUE_W = 150; // the widest value box (px), with its padding
const VALUE_PAD = 10; // horizontal padding (px) of the value box
const VALUE_SHARE = 0.2; // part of the item width that the value box may take, up to MAX_VALUE_W
const TOP_FLOOR = 0.3186; // in a tile with the name above: the value box is not narrower than the text at this font size (em), plus the padding
const FILL_WIDTH = 0.975; // part of the room that a value text at the full size may use (measured in Grafana)
const MIN_ITEM = { horizontal: 10, vertical: 16 };

/**
 * The sizes of a horizontal bar of an item `w` x `h` px (pure). `w100` is the width of the value text at a font size of 100 px
 * (with its unit at the full size) and `nameW100` the width of the name at 100 px. Gives { place, nameSize, nameBoxH, nameW,
 * barH, valueSize, valueMaxW, scaleLen } where `scaleLen` is the length (px) of 100 % of the scale.
 */
export function horizontalLayout({ w, h, w100, nameW100 }) {
  const place = h >= TOP_FROM ? "top" : "left";
  let nameSize;
  let nameBoxH = 0;
  let nameW = 0;
  let barH = h;
  if (place === "top") {
    nameBoxH = Math.min(h * 0.45, 50);
    nameSize = nameBoxH / 1.5;
    barH = h - nameBoxH;
  } else {
    nameSize = Math.max(12, h * 0.4);
    nameW = Math.min((nameW100 * nameSize) / 100 + 10, w * 0.4);
  }
  const room = w - nameW;
  let valueMaxW = Math.min(MAX_VALUE_W, Math.max(50, w * VALUE_SHARE));
  if (place === "top") valueMaxW = Math.max(valueMaxW, 2 * (VALUE_PAD / 2) + (w100 * TOP_FLOOR) / 1);
  valueMaxW = Math.min(valueMaxW, MAX_VALUE_W);
  const byWidth = ((valueMaxW - 2 * VALUE_PAD) * 100 * FILL_WIDTH) / (w100 || 1);
  const valueSize = Math.max(6, Math.min(barH, byWidth));
  return { place, nameSize, nameBoxH, nameW, barH, valueSize, valueMaxW, scaleLen: Math.max(0, room - valueMaxW) };
}

/**
 * The colour stops of the thresholds over the scale `min` to `max`, as [colour, percent of the scale] (pure).
 * The first step starts at 0. A step outside the scale is placed at 0 or at 100. The order of the steps is kept.
 * `dark` false gives the light shades of the named colours (colors.js resolveFill).
 */
export function thresholdStops(config, min, max, dark = true) {
  const t = config.thresholds;
  if (!t || !t.steps || t.steps.length === 0 || !(max > min) || !config.color || config.color.mode !== "thresholds") return null;
  let last = 0;
  return t.steps.map((step, i) => {
    const pos = i === 0 ? 0 : Math.max(last, stepPercent(t, step.value, min, max));
    last = pos;
    return [resolveFill(step.color, dark), pos];
  });
}

/**
 * The CSS gradient of a bar that is filled up to `fillPct` percent of the scale, or null (pure).
 * The colours keep their place on the scale: the bar shows the part of the scale up to the value.
 * Each colour holds over its step. A soft edge of `SOFT` percent of the scale ends just at the next step.
 * A step that the bar does not reach adds nothing: the bar has no sliver of the next colour.
 */
export function barGradient(config, min, max, fillPct, direction, dark = true) {
  const stops = thresholdStops(config, min, max, dark);
  if (!stops || !(fillPct > 0)) return null;
  const at = (p) => `${((p / fillPct) * 100).toFixed(2)}%`;
  const parts = [`${stops[0][0]} 0%`];
  for (let i = 1; i < stops.length; i++) {
    const [color, pos] = stops[i];
    const [prev, prevPos] = stops[i - 1];
    if (pos >= fillPct) break; // the bar ends before this step: its colour and the soft edge stay out of the bar
    const edge = Math.min(SOFT, (pos - prevPos) / 2);
    if (edge > 0) parts.push(`${prev} ${at(pos - edge)}`);
    else parts.push(`${prev} ${at(pos)}`);
    parts.push(`${color} ${at(pos)}`);
  }
  if (parts.length < 2) return null; // one colour only: the caller draws it plain
  return `linear-gradient(${direction}, ${parts.join(", ")})`;
}

// The colour of the cell that stands for the value `at` (LCD mode).
function cellColor(cfg, item, color, at, min, max, dark) {
  if (item.display.mapped && item.color) return color;
  if (cfg.color && cfg.color.mode === "thresholds") {
    const c = thresholdColor(cfg.thresholds, at, min, max);
    return c ? resolveFill(c, dark) : "var(--text)";
  }
  return color;
}

export function render(host, ctx) {
  const { panel } = ctx;
  const o = panel.options;
  const items = reduceItems(ctx, o.reduce);
  const vertical = o.orientation === "vertical";
  const orient = vertical ? "vertical" : "horizontal";
  // The copy of rows (9.8e): row_copy.js gives one text line for each item, the name and the value with one space.
  const root = el("div", { class: `bargauge bargauge--${orient}`, dataset: { testid: `bargauge-${panel.id}`, mode: o.display_mode, copyRows: "1" } });
  const lcd = [];
  const bars = []; // the fills of the basic and the gradient mode, painted again when the theme changes
  const entries = [];
  for (const item of items) {
    const cfg = item.field.config;
    const min = Number.isFinite(item.min) ? item.min : 0;
    let max = Number.isFinite(item.max) ? item.max : 100;
    if (!(max > min)) max = min + 1;
    const v = item.value;
    const fill = v === null ? 0 : Math.max(0, Math.min(1, (v - min) / (max - min)));
    const tc = v !== null ? thresholdColor(cfg.thresholds, v, min, max) : null;
    const nodata = v === null && item.text === "";
    // A value that is null has the colour of the base step (as Grafana draws "No data").
    const baseColor = v === null ? thresholdColor(cfg.thresholds, -Infinity, min, max) : null;
    const track = el("div", {
      class: `bargauge__track${o.show_unfilled ? " bargauge__track--unfilled" : ""}`,
      role: "meter",
      "aria-label": item.name,
      "aria-valuemin": String(min),
      "aria-valuemax": String(max),
      "aria-valuenow": v === null ? null : String(v),
      "aria-valuetext": item.text,
    });
    // The colour of the item in a theme: the colour of the display (a named colour has its own light shade, as Grafana), else the
    // threshold step, else the text colour.
    const colorIn = (dark) => fillOf(item.display, dark) || (tc ? resolveFill(tc, dark) : baseColor ? resolveFill(baseColor, dark) : "var(--text)");
    if (o.display_mode === "lcd") {
      lcd.push({ track, cfg, item, colorIn, fill, min, max });
    } else {
      const bar = el("div", { class: "bargauge__fill" });
      const pct = fill * 100;
      if (fill === 0) bar.hidden = true;
      track.append(bar);
      bars.push({ bar, cfg, item, colorIn, pct, min, max, fill });
    }
    const label = el("span", { class: "bargauge__name", text: item.name, title: item.name });
    const probe = el("span", { class: "bargauge__probe", text: item.name, "aria-hidden": "true" });
    const valueEl = el("span", { class: `bargauge__value${nodata ? " bargauge__value--none" : ""}` });
    const inner = el("span", { class: "bargauge__text" });
    if (nodata) inner.append(el("span", { class: "bargauge__num", text: "No data" }));
    else if (item.parts) {
      if (item.parts.prefix) inner.append(el("span", { class: "bargauge__unit", text: item.parts.prefix }));
      inner.append(el("span", { class: "bargauge__num", text: item.parts.text }));
      if (item.parts.suffix) inner.append(el("span", { class: "bargauge__unit", text: item.parts.suffix }));
    } else inner.append(el("span", { class: "bargauge__num", text: item.text }));
    valueEl.append(inner);
    const float = vertical && !o.show_unfilled;
    const itemEl = el("div", { class: `bargauge__item${float ? " bargauge__item--float" : ""}`, dataset: { copyRow: "1" } }, label, track, valueEl, probe);
    entries.push({ itemEl, label, probe, valueEl, inner, track, colorIn, float, fill });
    root.append(itemEl);
  }
  host.append(root);

  // The fills depend on the theme: the light theme draws the named colours with their own shades.
  function paintBars() {
    const dark = document.documentElement.dataset.theme !== "light";
    for (const b of bars) {
      const gradient = o.display_mode === "gradient" && !(b.item.display.mapped && b.item.color) ? barGradient(b.cfg, b.min, b.max, b.pct, vertical ? "to top" : "to right", dark) : null;
      b.bar.style.background = "";
      b.bar.style.backgroundImage = "";
      if (gradient) b.bar.style.backgroundImage = gradient;
      else b.bar.style.background = b.colorIn(dark);
    }
  }

  // The colour of a value text: the colour of the item, moved toward the text colour when it reads badly on the panel.
  function paintValues() {
    const surface = canvasColor(host, "var(--panel)");
    const dark = document.documentElement.dataset.theme !== "light";
    for (const e of entries) {
      const c = e.colorIn(dark);
      const size = parseFloat(e.valueEl.style.fontSize) || 0;
      e.valueEl.style.color = c === "var(--text)" ? "" : legibleOn(c, surface, size >= 24 ? 3 : 4.5);
    }
  }

  // LCD cells: as many as the track holds, 10 px with a gap of 2 px (Grafana). A lit cell has the colour of the step at its place; the cells
  // above the value are the same colours at a low opacity.
  function buildCells(force = false) {
    const dark = document.documentElement.dataset.theme !== "light";
    // All reads first, then all writes: the layout runs once, and the cells of a track do not change the size of another track.
    const lengths = lcd.map((c) => (vertical ? c.track.clientHeight : c.track.clientWidth));
    lcd.forEach((c, i) => {
      const length = lengths[i];
      if (!(length > 0)) return;
      const n = Math.max(1, Math.floor((length + CELL_GAP) / (CELL + CELL_GAP)));
      if (!force && c.track.childElementCount === n && c.track.dataset.cells === String(n)) return;
      c.track.dataset.cells = String(n);
      const cells = [];
      for (let k = 0; k < n; k++) {
        const at = k / n;
        const lit = k / n <= c.fill && c.fill > 0;
        const color = cellColor(c.cfg, c.item, c.colorIn(dark), c.min + at * (c.max - c.min), c.min, c.max, dark);
        const cell = el("span", { class: `bargauge__cell${lit ? " is-lit" : ""}` });
        cell.style.setProperty("--cell", color);
        cell.dataset.at = at.toFixed(3);
        cells.push(cell);
      }
      c.track.replaceChildren(...cells);
    });
  }

  // The width of the value text (with its unit at the full size) and, with `name`, of the name at a font size of 100 px. The size of a text
  // does not depend on the other items, so all items write, then all items read (one layout), then all items write the result.
  const measure100 = (name) => {
    for (const e of entries) {
      e.valueEl.style.fontSize = "100px";
      e.valueEl.style.maxWidth = "none";
      if (name) e.probe.style.fontSize = "100px";
    }
    return entries.map((e) => [e.inner.getBoundingClientRect().width, name ? e.probe.getBoundingClientRect().width : 0]);
  };

  // Size: the box of each item, then the texts. The measure reads only the box and the widths of the texts at 100 px, so the same inputs
  // give the same result: skip it when they are the same as at the last measure.
  let sized = null;
  function layout() {
    // The real box, not the rounded clientWidth: a rounded-up size makes a scroll bar of less than 1 px (9.19a).
    const { w, h } = innerSize(root);
    if (!(w > 0) || !(h > 0)) return;
    const key = inputKey(w, h);
    if (key !== sized) {
      measureItems(w, h);
      sized = key;
    }
    // The fill is a length in px: a part of the scale length of the item.
    const bs = new Map(bars.map((b) => [b.bar, b]));
    entries.forEach((e) => {
      const bar = e.track.firstElementChild;
      const b = bs.get(bar);
      if (!b) return;
      if (vertical) bar.style.height = `${b.pct.toFixed(3)}%`;
      else bar.style.width = `${(e.scaleLen * b.fill).toFixed(3)}px`;
    });
    paintValues();
    buildCells();
  }

  function measureItems(w, h) {
    const n = Math.max(1, entries.length);
    const gaps = SPACING[orient] * (n - 1);
    root.classList.add("bargauge--measure");
    if (!vertical) {
      const itemH = Math.max(MIN_ITEM.horizontal, floorUnit((h - gaps) / n));
      entries.forEach((e, i) => {
        e.itemEl.style.height = `${itemH}px`;
        e.itemEl.style.marginBottom = i < n - 1 ? `${SPACING.horizontal}px` : "";
      });
      const w100 = measure100(true);
      entries.forEach((e, i) => {
        const lay = horizontalLayout({ w, h: itemH, w100: w100[i][0], nameW100: w100[i][1] });
        e.itemEl.dataset.place = lay.place;
        e.label.style.fontSize = `${lay.nameSize}px`;
        e.label.style.height = lay.place === "top" ? `${lay.nameBoxH}px` : "";
        e.label.style.width = lay.place === "left" ? `${lay.nameW}px` : "";
        e.valueEl.style.fontSize = `${lay.valueSize.toFixed(4)}px`;
        e.valueEl.style.setProperty("--bargauge-unit", `${unitSizeOf(lay.valueSize).toFixed(4)}px`);
        e.valueEl.style.maxWidth = `${lay.valueMaxW}px`;
        e.itemEl.style.setProperty("--bargauge-bar-h", `${lay.barH}px`);
        e.scaleLen = lay.scaleLen;
      });
    } else {
      const itemW = Math.max(MIN_ITEM.vertical, floorUnit((w - gaps) / n));
      entries.forEach((e, i) => {
        e.itemEl.style.width = `${itemW}px`;
        e.itemEl.style.marginRight = i < n - 1 ? `${SPACING.vertical}px` : "";
      });
      const w100 = measure100(false);
      entries.forEach((e, i) => {
        const size = Math.max(6, Math.min(h * 0.1, (itemW * 100 * FILL_WIDTH) / (w100[i][0] || 1)));
        e.valueEl.style.maxWidth = "";
        e.valueEl.style.fontSize = `${size.toFixed(4)}px`;
        e.valueEl.style.setProperty("--bargauge-unit", `${unitSizeOf(size).toFixed(4)}px`);
      });
    }
    root.classList.remove("bargauge--measure");
  }

  paintBars();
  layout();
  const observer = new ResizeObserver(layout);
  observer.observe(host);
  const onFont = () => layout();
  if (document.fonts) {
    if (document.fonts.ready) document.fonts.ready.then(layout);
    if (document.fonts.addEventListener) document.fonts.addEventListener("loadingdone", onFont);
  }
  const onTheme = () => {
    paintBars();
    buildCells(true);
    paintValues();
  };
  document.addEventListener("hune-theme", onTheme);
  return {
    destroy() {
      document.removeEventListener("hune-theme", onTheme);
      if (document.fonts && document.fonts.removeEventListener) document.fonts.removeEventListener("loadingdone", onFont);
      observer.disconnect();
      root.remove();
    },
  };
}
