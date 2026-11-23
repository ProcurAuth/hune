// SPDX-License-Identifier: MIT OR Apache-2.0
// The fuzz tool of the syntax tree of the query editor (F42, job `nightly`). It runs the properties of ast_props.test.mjs on mutations of the corpus.
// Use: node tools/ast_fuzz.mjs --target sql_ch|sql_pg|json|format [--seconds N] [--seed S] [--failures K] [--file PATH]
//   --target   the parser to test: sql_ch (ClickHouse), sql_pg (PostgreSQL), json (the MongoDB pipeline), or format (the formatters).
//   --seconds  the run time. The default is 60.
//   --seed     the seed of the random source. The default is 1. The same seed gives the same inputs.
//   --failures the number of failing inputs to collect before the tool stops. The default is 1. A larger number finds more than one fault in one run.
//   --file     check one file with the properties of the target, and exit. Use it to repeat a crash.
// Each input comes from a text of the corpus (tests/fixtures/ast/) with one to four mutations. A failing input is saved in
// target/fuzz-ast/<target>/crash-<hash>.txt and printed with its seed and its number. The tool exits with 1 at the first failure.
// A watchdog stops an input that runs for 10 seconds or more, saves it as hang-<hash>.txt and exits with 1.
// The tool needs no network and no dependency. The target `format` is skipped (exit 0, with a message) while format.js does not exist.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { ROOT, formatFaults, hashText, jsonFaults, loadCorpus, loadFormatter, makeRng, mutate, sqlFaults } from "./ast_props_lib.mjs";

const TARGETS = ["sql_ch", "sql_pg", "json", "format"];
const HANG_MS = 10000;
// The limit of one input. The target `format` runs 3 formatters and the tokenizers twice on each input, so its limit is higher.
const SLOW_MS = { sql_ch: 3000, sql_pg: 3000, json: 3000, format: 20000 };
const MAX_UNITS = 300000;

/** The faults of a text for a target. `fmt` is the module of the formatter. */
function faultsOf(target, text, fmt) {
  switch (target) {
    case "sql_ch":
      return sqlFaults(text, "clickhouse");
    case "sql_pg":
      return sqlFaults(text, "postgres");
    case "json":
      return jsonFaults(text);
    default:
      return ["clickhouse", "postgres", "json"].flatMap((kind) => formatFaults(fmt, text, kind).map((f) => `[${kind}] ${f}`));
  }
}

/** Parses the command line. It gives null and prints the use when an argument is wrong. */
function parseArgs(argv) {
  const out = { target: null, seconds: 60, seed: 1, file: null, failures: 1 };
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    const value = argv[i + 1];
    if (value === undefined) return null;
    if (key === "--target") out.target = value;
    else if (key === "--seconds") out.seconds = Number(value);
    else if (key === "--seed") out.seed = Number(value);
    else if (key === "--file") out.file = value;
    else if (key === "--failures") out.failures = Number(value);
    else return null;
  }
  if (!TARGETS.includes(out.target) || !Number.isFinite(out.seconds) || out.seconds < 0 || !Number.isInteger(out.seed) || !Number.isInteger(out.failures) || out.failures < 1) return null;
  return out;
}

/** Saves a failing input and gives its path. */
function save(target, prefix, text) {
  const dir = join(ROOT, "target", "fuzz-ast", target);
  mkdirSync(dir, { recursive: true });
  const path = join(dir, `${prefix}-${hashText(text)}.txt`);
  writeFileSync(path, text);
  return path;
}

/** The pool of texts for a target: the real texts and the hostile cases of the corpus. */
function makePool(target) {
  const corpus = loadCorpus();
  const sql = corpus.sql.map((e) => e.text);
  const json = corpus.json.map((e) => e.text);
  const hostile = corpus.hostile.map((e) => e.text);
  // A target reads its own kind most of the time, and the other kind as a hostile input.
  const own = target === "json" ? json : target === "format" ? [...sql, ...json] : sql;
  const other = target === "json" ? sql : target === "format" ? [] : json;
  return { own: own.filter((t) => t.length <= 20000), other: other.filter((t) => t.length <= 20000), hostile, all: [...sql, ...json, ...hostile].filter((t) => t.length <= 20000) };
}

/** The input number `i` of a seed: a text of the pool with one to four mutations. The same seed and number give the same text. */
function makeInput(pool, seed, i) {
  const rng = makeRng((seed * 1000003 + i) >>> 0);
  const roll = rng.int(100);
  const source = roll < 70 ? pool.own : roll < 85 ? pool.other : pool.hostile.filter((t) => t.length <= 20000);
  let text = source.length ? rng.pick(source) : "";
  const steps = 1 + rng.int(4);
  for (let k = 0; k < steps; k++) text = mutate(rng, text, pool.all);
  return text.length > MAX_UNITS ? text.slice(0, MAX_UNITS) : text;
}

/**
 * The loop of the worker. It runs inputs until the time ends or one fails. It writes the current input into the shared
 * buffer and counts in the shared counter, so the main thread can find a hang and save its input.
 */
async function workerLoop() {
  const { target, seed, seconds, shared, control, maxFailures } = workerData;
  const text16 = new Uint16Array(shared);
  const ctl = new Int32Array(control); // [0] counter of inputs, [1] length of the current input
  const fmt = target === "format" ? await loadFormatter() : null;
  const pool = makePool(target);
  const end = Date.now() + seconds * 1000;
  // The regression pass: each hostile case of the corpus once, as it is.
  const regression = loadCorpus().hostile;
  let count = 0;
  let nodes = 0;
  let failed = 0;
  const seenCrash = new Set();
  const run = (text, label) => {
    const n = Math.min(text.length, text16.length);
    for (let k = 0; k < n; k++) text16[k] = text.charCodeAt(k);
    Atomics.store(ctl, 1, n);
    Atomics.add(ctl, 0, 1);
    const t0 = performance.now();
    let faults;
    try {
      faults = faultsOf(target, text, fmt);
    } catch (e) {
      faults = [`throws: ${e && e.message}`];
    }
    const ms = performance.now() - t0;
    if (!faults.length && ms > SLOW_MS[target]) faults = [`slow: ${Math.round(ms)} ms for ${text.length} units`];
    if (faults.length) {
      const path = save(target, "crash", text);
      if (!seenCrash.has(path)) {
        seenCrash.add(path);
        failed += 1;
        parentPort.postMessage({ type: "fail", label, faults, path, count, units: text.length });
      }
      return failed < maxFailures;
    }
    count += 1;
    nodes += text.length;
    return true;
  };
  for (const h of regression) {
    if (!run(h.text, `regression ${h.name}`)) break;
  }
  for (let i = 0; Date.now() < end && failed < maxFailures; i++) {
    if (!run(makeInput(pool, seed, i), `seed ${seed} input ${i}`)) break;
    if (i % 200 === 0) parentPort.postMessage({ type: "progress", count });
  }
  parentPort.postMessage({ type: "done", count, units: nodes, regression: regression.length, failed });
}

/** The main thread: it starts the worker, watches it, and prints the counts. */
async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (!args) {
    console.error("Usage: node tools/ast_fuzz.mjs --target sql_ch|sql_pg|json|format [--seconds N] [--seed S] [--failures K] [--file PATH]");
    process.exit(2);
  }
  const { target } = args;
  if (target === "format") {
    const fmt = await loadFormatter();
    if (!fmt) {
      console.log("SKIPPED: web/js/editor/ast/format.js does not exist yet, so the target format has nothing to test.");
      return;
    }
  }
  if (args.file) {
    const text = readFileSync(args.file, "utf8");
    const fmt = target === "format" ? await loadFormatter() : null;
    const faults = faultsOf(target, text, fmt);
    console.log(faults.length ? `FAIL ${args.file}:\n  ${faults.join("\n  ")}` : `ok ${args.file}`);
    process.exit(faults.length ? 1 : 0);
  }
  const shared = new SharedArrayBuffer(MAX_UNITS * 2);
  const control = new SharedArrayBuffer(8);
  const ctl = new Int32Array(control);
  const worker = new Worker(fileURLToPath(import.meta.url), { workerData: { target, seed: args.seed, seconds: args.seconds, shared, control, maxFailures: args.failures } });
  let finished = false;
  let lastCount = -1;
  let lastChange = Date.now();
  const watchdog = setInterval(() => {
    const count = Atomics.load(ctl, 0);
    if (count !== lastCount) {
      lastCount = count;
      lastChange = Date.now();
      return;
    }
    if (Date.now() - lastChange < HANG_MS) return;
    finished = true;
    clearInterval(watchdog);
    const n = Atomics.load(ctl, 1);
    const text16 = new Uint16Array(shared, 0, n);
    let text = "";
    for (let k = 0; k < n; k += 8192) text += String.fromCharCode(...text16.subarray(k, Math.min(n, k + 8192)));
    const path = save(target, "hang", text);
    console.error(`HANG: an input of ${n} units runs for ${HANG_MS / 1000} s or more (target ${target}, seed ${args.seed}). Saved: ${path}`);
    worker.terminate().finally(() => process.exit(1));
  }, 500);
  worker.on("message", (m) => {
    if (m.type === "fail") {
      console.error(`FAIL target ${target}, ${m.label}, ${m.units} units, ${m.count} inputs ok before it:\n  ${m.faults.join("\n  ")}\nSaved: ${m.path}\nRepeat: node tools/ast_fuzz.mjs --target ${target} --file ${m.path}`);
      process.exitCode = 1;
    } else if (m.type === "done") {
      finished = true;
      clearInterval(watchdog);
      console.log(`done: target ${target}, seed ${args.seed}, ${args.seconds} s, ${m.count} inputs ok (${m.regression} hostile cases first), ${m.units} units, ${m.failed} failing input(s)`);
      if (m.failed) process.exitCode = 1;
    }
  });
  worker.on("error", (e) => {
    finished = true;
    clearInterval(watchdog);
    console.error(`ERROR in the fuzz worker: ${e && e.stack}`);
    process.exitCode = 1;
  });
  worker.on("exit", () => {
    clearInterval(watchdog);
    if (!finished) process.exitCode = 1;
  });
}

if (isMainThread) await main();
else await workerLoop();
