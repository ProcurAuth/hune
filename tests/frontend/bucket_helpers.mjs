// SPDX-License-Identifier: MIT OR Apache-2.0
// Helpers of the tests of bucket_model.js (FE-42 to FE-46): vectors, fields and the data of the measure.
import { readFileSync } from "node:fs";

/** The JSON of a file of `tests/frontend/vectors/`. */
export const read = (name) => JSON.parse(readFileSync(new URL(`./vectors/${name}`, import.meta.url)));

/** The default `color` object of a new heatmap (9.19e, section 11). */
export const COLOR = { mode: "scheme", scheme: "oranges", fill: "dark-orange", scale: "exponential", exponent: 0.5, steps: 64, reverse: false, min: null, max: null };

/** The options of cells that a new heatmap has. */
export const CELLS = { hide_le: 0, hide_ge: null, color: COLOR };

/** The options of a new histogram (9.19f, section 7) that a test changes. */
export const HIST = { bucket_count: 30, bucket_size: null, bucket_offset: 0, combine: false };

/** A cell of a vector: `null` is a null, and the texts `inf` and `-inf` are the infinities (JSON has no such numbers). */
const cell = (v) => (v === null ? NaN : v === "inf" ? Infinity : v === "-inf" ? -Infinity : v);

/** The values of a column of a vector as a `Float64Array`. */
export const column = (list) => Float64Array.from(list, cell);

let measured;
/** The 20,000 rows of the measure: `{ times, v, from, to }` (the range of 5 hours that the dashboards had). */
export function measure() {
  if (!measured) {
    const d = read("FE-42-data-20000.json");
    const times = new Float64Array(d.n);
    const v = new Float64Array(d.n);
    for (let i = 0; i < d.n; i++) {
      times[i] = d.t0 + 1000 * Math.floor((i * d.span_s) / d.n);
      v[i] = d.milli[i] / 1000;
    }
    measured = { times, v, from: d.t0, to: d.t0 + 5 * 3600e3 };
  }
  return measured;
}

/** A field of the kind `number`. A null becomes NaN and gets the flag in `nulls`. */
export function numberField(name, list, config = {}) {
  return { name, kind: "number", values: column(list), nulls: Uint8Array.from(list, (v) => (v === null ? 1 : 0)), config };
}

/** A field of the kind `time` in milliseconds. */
export function timeField(list) {
  return { name: "time", kind: "time", values: column(list), nulls: Uint8Array.from(list, (v) => (v === null ? 1 : 0)), config: {} };
}

/** A field of the kind `number_text`. */
export function textField(name, list, config = {}) {
  return { name, kind: "number_text", values: list.slice(), num: Float64Array.from(list, (s) => parseFloat(s)), nulls: Uint8Array.from(list, (s) => (s === null ? 1 : 0)), config };
}

/** The fields and the names of a frame of a vector: `{ names, time, cols }` (`time` null: no time column). */
export function frameFields(frame, kind = "number", hide = []) {
  const fields = [];
  if (frame.time) fields.push(timeField(frame.time));
  frame.cols.forEach((list, i) => {
    const config = hide.includes(frame.names[i]) ? { hide_from: { viz: true } } : {};
    fields.push(kind === "number_text" ? textField(frame.names[i], list, config) : numberField(frame.names[i], list, config));
  });
  return { fields, names: fields.map((f) => f.name) };
}

/** A fake DOM node for the tests of the texts: a text is a text node, and an element never comes from data. */
export class FakeNode {
  constructor(tag) {
    this.tag = tag;
    this.children = [];
  }

  set textContent(value) {
    this.children = value === "" ? [] : [{ tag: "#text", text: String(value) }];
  }

  get textContent() {
    return this.children.map((c) => c.text).join("");
  }

  set innerHTML(_) {
    throw new Error("innerHTML");
  }
}
