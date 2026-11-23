// The percent change of a stat (4.29, 9.19; wave H, item P11), as Grafana 12.4.2 shows it (measured, FE-82). Pure module.

import { reduceField } from "../reduce.js";

// 3 significant digits, the group separator and the sign of a negative value (also of -0), as Grafana in an English browser.
const FORMAT = new Intl.NumberFormat("en-US", { style: "percent", maximumSignificantDigits: 3 });

/** The change from the first to the last usable value, in percent of the first (`diff_percent` of 9.19). No usable value gives 0, as Grafana. */
export function percentOf(field) {
  const r = reduceField(field, "diff_percent");
  return r.num === null ? 0 : r.num;
}

/** The text ("20%", "-0%", "∞%"), or null for NaN (a first and a last value of 0): then no change shows. */
export const percentText = (p) => (Number.isNaN(p) ? null : FORMAT.format(p / 100));

/** The arrow: "up", "down", or null for 0 and -0. */
export const percentDir = (p) => (p > 0 ? "up" : p < 0 ? "down" : null);

/** The colour of the modes `standard` and `inverted`: green for a good change, else red (also for 0, as Grafana). */
export const percentHue = (p, mode) => (p * (mode === "inverted" ? -1 : 1) > 0 ? "green" : "red");

/** The sizes (px) for a value of `v` px: the text, the arrow, the gap and the lift (the negative top margin in a stacked tile). */
export function percentSizes(v) {
  const size = Math.max(v / 2.5, 12);
  return { size, icon: Math.max(v / 3, 10), gap: Math.max(size / 3, 4), lift: size > 12 ? size / 4 : 0 };
}
