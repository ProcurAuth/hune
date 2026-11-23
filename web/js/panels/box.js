// The inner box of a panel part, with the fraction of a pixel (9.19a).
//
// clientWidth and clientHeight round to an integer. In a grid column with a fraction, or at a browser zoom that is not
// 100 %, the rounded value can be larger than the real box. A child of that size is then larger than its box by less
// than one pixel, and a box with `overflow: auto` shows a scroll bar for it. This module reads the real box and rounds
// DOWN to the layout unit of the browser (1/64 px), so the children that a panel sizes from it always fit.

/** The layout unit of the browser engines (1/64 CSS px). */
export const LAYOUT_UNIT = 1 / 64;

/** The largest multiple of the layout unit that is not larger than `value` (pure). */
export function floorUnit(value) {
  if (!(value > 0)) return 0;
  // The small tolerance keeps a value that is already a multiple of the unit (for example 335.828125) as it is.
  return Math.floor(value / LAYOUT_UNIT + 1e-6) * LAYOUT_UNIT;
}

// A scroll bar is at least a few pixels wide. A difference of 0 or 1 px between offset and client sizes comes from
// two independent roundings, not from a scroll bar.
function barSize(diff) {
  return diff >= 2 ? diff : 0;
}

/**
 * The content box of `el` in CSS px: its real width and height, less the borders, the padding and the scroll bars,
 * rounded down to the layout unit. The result is never larger than the space that a child has.
 */
export function innerSize(el) {
  const rect = el.getBoundingClientRect();
  const cs = getComputedStyle(el);
  const px = (name) => parseFloat(cs[name]) || 0;
  const bx = px("borderLeftWidth") + px("borderRightWidth");
  const by = px("borderTopWidth") + px("borderBottomWidth");
  const padX = px("paddingLeft") + px("paddingRight");
  const padY = px("paddingTop") + px("paddingBottom");
  const barY = barSize(el.offsetWidth - el.clientWidth - Math.round(bx));
  const barX = barSize(el.offsetHeight - el.clientHeight - Math.round(by));
  return {
    w: floorUnit(rect.width - bx - padX - barY),
    h: floorUnit(rect.height - by - padY - barX),
  };
}
