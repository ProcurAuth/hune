// SPDX-License-Identifier: MIT OR Apache-2.0
// Helpers of the tests FE-70 to FE-75 (more transforms, 9.21a): fields from compact column specs, and a plain view of the result.
import { applyMore } from "../../web/js/transform_more.js";

// A column spec: { name, kind, values, labels?, ref?, prefix?, config? }. A time is in milliseconds. A null cell is `null`.
export function fieldOf(c) {
  const n = c.values.length;
  const f = { name: c.name, type: c.kind === "text" ? "string" : c.kind, kind: c.kind, values: null, labels: c.labels || null, config: c.config || {} };
  // JSON has no number that is not finite: the text "NaN", "Infinity" or "-Infinity" in a vector gives that number.
  const special = { NaN, Infinity, "-Infinity": -Infinity };
  if (c.kind === "number" || c.kind === "time") {
    f.values = Float64Array.from(c.values, (v) => (v === null ? NaN : typeof v === "string" ? special[v] : v));
    f.nulls = Uint8Array.from(c.values, (v) => (v === null ? 1 : 0));
  } else if (c.kind === "number_text") {
    f.values = c.values.map((v) => (v === null ? null : String(v)));
    f.num = Float64Array.from(c.values, (v) => (v === null ? NaN : Number(v)));
    f.nulls = Uint8Array.from(c.values, (v) => (v === null ? 1 : 0));
  } else {
    f.values = c.values.slice();
  }
  if (c.ref !== undefined) f.ref = c.ref;
  if (c.prefix !== undefined) f.prefix = c.prefix;
  return f;
}

export const framesOf = (list) => list.map((cols) => cols.map(fieldOf));

// A cell for a comparison: a number that is not finite is a text (JSON has no value for it), and -0 is 0.
function cellOf(f, i) {
  if (f.kind === "number" || f.kind === "time") {
    if (f.nulls[i]) return null;
    const v = f.values[i];
    if (Number.isNaN(v)) return "NaN";
    if (!Number.isFinite(v)) return v > 0 ? "Infinity" : "-Infinity";
    return v === 0 ? 0 : v;
  }
  return f.values[i] === undefined ? null : f.values[i];
}

/** The plain view of a list of frames: [[{ n, k, v, l?, c?, r?, p? }]]. `n` is the name that a transform set, else the name. */
export function plainFrames(frames) {
  return frames.map((fs) => fs.map((f) => {
    const o = { n: f.config.display_name || f.name, k: f.kind, v: Array.from({ length: f.values.length }, (_, i) => cellOf(f, i)) };
    if (f.labels) o.l = f.labels;
    const { display_name: _name, ...config } = f.config;
    if (Object.keys(config).length) o.c = config;
    if (f.ref !== undefined && f.ref !== null) o.r = f.ref;
    if (f.prefix !== undefined && f.prefix !== null) o.p = f.prefix;
    return o;
  }));
}

/** Apply the transforms in order to the frames of a vector. It returns the plain frames, or { error } for an error of a transform. */
export function run(vectorFrames, transforms) {
  try {
    let frames = framesOf(vectorFrames);
    for (const t of Array.isArray(transforms) ? transforms : [transforms]) frames = applyMore(frames, t);
    return plainFrames(frames);
  } catch (e) {
    return { error: e.message };
  }
}

// ---- the vector files and the measure of Grafana (tests/frontend/vectors) ----------------------------------------------------

import { readFileSync } from "node:fs";
import { framesToFields } from "../../web/js/frame.js";

/** The parsed JSON file `name` of tests/frontend/vectors. */
export const vectors = (name) => JSON.parse(readFileSync(new URL(`./vectors/${name}`, import.meta.url), "utf8"));

/** The cases of the measure of Grafana 12.4.2 (docs/08, 19.20j). */
export const measure = vectors("measure-f28.json").cases;

const KIND = { s: ["text", "string"], f: ["number", "number"], i: ["number", "number"], t: ["time", "time"], b: ["bool", "boolean"] };
const frameJson = (spec) => ({ columns: spec.cols.map(([name, t]) => ({ name, type: KIND[t][1], kind: KIND[t][0] })), values: spec.cols.map((c, i) => spec.rows.map((r) => r[i])) });
const finite = (v) => (typeof v === "number" ? (Number.isFinite(v) ? v || 0 : null) : v);
const cells = (f) => Array.from({ length: f.values.length }, (_, r) => (f.nulls ? (f.nulls[r] ? null : f.kind === "number_text" ? f.num[r] : f.values[r]) : f.values[r])).map(finite);

/**
 * Run the transforms of a measure case. It gives the frames with a field list ([{ n, v }]), as the file keeps the result of Grafana:
 * a frame with no field is left out, and a number that is not finite is null (JSON has no value for it). An error gives { error }.
 */
export function measureRun(c) {
  try {
    let frames = Object.values(c.frames).map((spec) => framesToFields(frameJson(spec)));
    for (const t of c.hune_tr) frames = applyMore(frames, t);
    return frames.filter((fs) => fs.length).map((fs) => fs.map((f) => ({ n: f.config.display_name || f.name, v: cells(f) })));
  } catch (e) {
    return { error: e.message };
  }
}

/** The smallest time in ms of `runs` calls of `fn`, and the last result. */
export function best(fn, runs = 7) {
  let min = Infinity;
  let out;
  for (let i = 0; i < runs; i++) {
    const t0 = performance.now();
    out = fn();
    min = Math.min(min, performance.now() - t0);
  }
  return { ms: min, out };
}
