// SPDX-License-Identifier: MIT OR Apache-2.0
// The shared part of the property tests and of the fuzz tool of the syntax tree of the query editor (F42).
// It holds the corpus loader, the seeded random source, the mutations and the checks of the properties.
// A check returns a list of faults. An empty list means that the property holds. A check never throws:
// an exception of the module under test is a fault with the text "throws".
// The tree is never a security check. These checks test the structure only.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { AST_REASONS, MAX_AST_INPUT_BYTES, TOKEN, utf8Length, walk } from "../../../web/js/editor/ast/nodes.js";
import { parseJson } from "../../../web/js/editor/ast/json_tree.js";
import { tokensClickHouse } from "../../../web/js/editor/ast/tokens_ch.js";
import { tokensPostgres } from "../../../web/js/editor/ast/tokens_pg.js";
import { parseSql } from "../../../web/js/editor/ast/sql_tree.js";

/** The root of the repository. */
export const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");

const FIXTURES = join(ROOT, "tests", "fixtures", "ast");
const KINDS = new Set(Object.values(TOKEN));
const BRACKETS = "()[]{}";

/** Reads a JSON file of the corpus directory. */
function readFixture(name) {
  return JSON.parse(readFileSync(join(FIXTURES, name), "utf8"));
}

/** Makes the text of a hostile case from its parts: a list of [text, times]. */
export function expandCase(c) {
  return c.parts.map(([text, times]) => text.repeat(times)).join("");
}

/** Loads the corpus: the real SQL texts, the real JSON texts and the hostile cases with their text. */
export function loadCorpus() {
  const sql = readFixture("corpus_sql.json").entries;
  const json = readFixture("corpus_json.json").entries;
  const hostile = readFixture("hostile.json").cases.map((c) => ({ name: c.name, text: expandCase(c) }));
  return { sql, json, hostile };
}

/** A seeded random source (mulberry32). It gives numbers in [0, 1) and the helpers `int` and `pick`. */
export function makeRng(seed) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  next.int = (n) => Math.floor(next() * n);
  next.pick = (list) => list[Math.floor(next() * list.length)];
  return next;
}

// The pieces that a mutation inserts: brackets, quotes, comment marks, macros, variables, keywords and hostile characters.
const PIECES = [
  "(", ")", "[", "]", "{", "}", "'", "\"", "`", "$$", "$", "${", "${a:csv}", "$a", "$__timeFilter(", "$__interval_ms", "$1",
  "/*", "*/", "--", "-- c\n", "#", "\n", "\r\n", "\r", "\t", " ", ",", ";", ":", "::", "->", "->>", "||", ".", "\\", "\\'", "''",
  "SELECT ", "FROM ", "WHERE ", "WITH ", "AS ", "UNION ALL ", "GROUP BY ", "ORDER BY ", "ARRAY JOIN ", "LEFT JOIN ", "ON ", "LIMIT ",
  "SETTINGS ", "FORMAT ", "{p:UInt8}", "{p:Array(", "E'", "$t$", "ARRAY[", "WINDOW ", "OVER (",
  "\"a\":", "\"$match\":", "null", "true", "1e5", "-1", "\"\\u00e9\"", "\"\\ud800\"", "\\u",
  "\u0000", "\u0001", "\u007f", "\u2028", "\ufeff", "\u00e9", "\u20ac", "\ud83d\ude00", "\ud800", "\udc00",
];

/** Applies one random mutation to a text. `pool` is a list of other texts that a splice can take a slice from. */
export function mutate(rng, text, pool) {
  const n = text.length;
  const at = n ? rng.int(n + 1) : 0;
  const len = Math.min(n - at, 1 + rng.int(8));
  switch (rng.int(10)) {
    case 0:
      return text.slice(0, at) + text.slice(at + len);
    case 1:
    case 2:
      return text.slice(0, at) + rng.pick(PIECES) + text.slice(at);
    case 3:
      return text.slice(0, at);
    case 4: {
      const b = rng.int(n + 1);
      const [x, y] = at <= b ? [at, b] : [b, at];
      return text.slice(0, y) + text.slice(x, y) + text.slice(y);
    }
    case 5: {
      const other = pool.length ? rng.pick(pool) : text;
      const from = other.length ? rng.int(other.length) : 0;
      return text.slice(0, at) + other.slice(from, from + 1 + rng.int(200)) + text.slice(at);
    }
    case 6:
      return text.slice(0, at) + rng.pick(PIECES).repeat(1 + rng.int(40)) + text.slice(at);
    case 7:
      return text.slice(0, at) + String.fromCharCode(rng.int(0x10000)) + text.slice(at + len);
    case 8:
      return text.slice(at);
    default: {
      const k = 1 + rng.int(4);
      let out = text;
      for (let i = 0; i < k; i++) out = out.slice(0, rng.int(out.length + 1)) + rng.pick(PIECES) + out.slice(rng.int(out.length + 1));
      return out;
    }
  }
}

/** Gives a short hash of a text for the name of a crash file. */
export function hashText(text) {
  return createHash("sha1").update(text, "utf8").digest("hex").slice(0, 12);
}

/** The faults of a token list: the tokens cover the text with no gap and no overlap, with known kinds and a boolean `closed`. */
export function tokenFaults(text, tokens) {
  const out = [];
  let at = 0;
  for (const t of tokens) {
    if (!KINDS.has(t.kind)) out.push(`unknown kind ${t.kind} at ${t.start}`);
    if (t.start !== at) {
      out.push(`gap or overlap at ${at} (token starts at ${t.start})`);
      return out;
    }
    if (!(t.end > t.start)) {
      out.push(`empty token at ${t.start}`);
      return out;
    }
    if (typeof t.closed !== "boolean") out.push(`closed is not a boolean at ${t.start}`);
    at = t.end;
  }
  if (at !== text.length) out.push(`the tokens end at ${at}, the text at ${text.length}`);
  return out;
}

/**
 * The faults of a result for a text. `type` is the expected type of the root ("script" or "doc").
 * Checks: the tokens, the root, the order and the nesting of the nodes, the pair of the brackets, the folds and the cap.
 */
export function resultFaults(text, r, type, brackets = BRACKETS) {
  const out = [];
  if (!r || typeof r !== "object" || !r.root || !Array.isArray(r.tokens)) return ["the result has no root or no tokens"];
  out.push(...tokenFaults(text, r.tokens));
  if (r.capped !== null) {
    if (!r.capped || !AST_REASONS.includes(r.capped.reason)) out.push(`capped has a wrong reason: ${JSON.stringify(r.capped)}`);
    else if (!Number.isInteger(r.capped.at) || r.capped.at < 0 || r.capped.at > text.length) out.push(`capped.at ${r.capped.at} is out of the text`);
  }
  if (utf8Length(text) > MAX_AST_INPUT_BYTES && !(r.capped && r.capped.reason === "too_large")) out.push("over the input cap, but capped is not too_large");
  const root = r.root;
  if (root.type !== type || root.start !== 0 || root.end !== text.length) out.push(`the root is ${root.type} ${root.start}-${root.end}, not ${type} 0-${text.length}`);
  const owners = new Map();
  const own = (offset) => owners.set(offset, (owners.get(offset) ?? 0) + 1);
  let reported = 0;
  const report = (m) => {
    if (reported++ < 8) out.push(m);
  };
  walk(root, (n) => {
    if (typeof n.type !== "string" || !Array.isArray(n.children)) {
      report(`a node with a wrong shape at ${n.start}`);
      return false;
    }
    if (!(n.start <= n.end) || n.start < 0 || n.end > text.length) report(`${n.type} ${n.start}-${n.end} is out of the text`);
    let prev = n.start;
    for (const c of n.children) {
      if (c.start < prev) report(`${c.type} ${c.start}-${c.end} starts before ${prev} in ${n.type} ${n.start}-${n.end}`);
      if (c.end > n.end) report(`${c.type} ${c.start}-${c.end} ends after ${n.type} ${n.start}-${n.end}`);
      prev = c.end;
    }
    if (n.open !== undefined) {
      own(n.open);
      if (!(n.open >= n.start) || !BRACKETS.includes(text[n.open])) report(`${n.type}: open ${n.open} is not a bracket`);
      if (n.close === undefined && n.error !== "unclosed" && r.capped === null) report(`${n.type} at ${n.open} has no close and no error unclosed`);
    }
    if (n.close !== undefined) {
      own(n.close);
      if (!(n.close < n.end) || !BRACKETS.includes(text[n.close]) || n.open === undefined || !(n.close > n.open)) report(`${n.type}: close ${n.close} is wrong`);
      if (n.error === "unclosed") report(`${n.type} at ${n.open} has a close and the error unclosed`);
    }
    if (n.type === "error" && n.error === "unmatched") own(n.start);
    if (n.foldable === true && !text.slice(n.start, n.end).includes("\n")) report(`${n.type} ${n.start}-${n.end} is foldable on one line`);
    return undefined;
  });
  if (r.capped === null) {
    for (const t of r.tokens) {
      if (t.kind === TOKEN.PUNCT && t.end - t.start === 1 && brackets.includes(text[t.start]) && owners.get(t.start) !== 1) report(`the bracket at ${t.start} is owned ${owners.get(t.start) ?? 0} times`);
    }
    for (const [offset, count] of owners) if (count !== 1) report(`the offset ${offset} is owned ${count} times`);
  }
  return out;
}

/** The faults of `parseSql` for a text and a dialect ("clickhouse" or "postgres"). */
export function sqlFaults(text, dialect) {
  let r;
  try {
    r = parseSql(text, dialect);
  } catch (e) {
    return [`throws: ${e && e.message}`];
  }
  return resultFaults(text, r, "script");
}

/** The faults of `parseJson` for a text. */
export function jsonFaults(text) {
  let r;
  try {
    r = parseJson(text);
  } catch (e) {
    return [`throws: ${e && e.message}`];
  }
  return resultFaults(text, r, "doc", "[]{}");
}

/** The non-blank tokens of a text as `kind:closed:text` strings, for a formatter kind ("clickhouse", "postgres" or "json"). */
export function significantTokens(text, kind) {
  const tokens = kind === "json" ? parseJson(text).tokens : kind === "postgres" ? tokensPostgres(text) : tokensClickHouse(text);
  return tokens.filter((t) => t.kind !== TOKEN.BLANK).map((t) => `${t.kind}:${t.closed ? 1 : 0}:${text.slice(t.start, t.end)}`);
}

/** The tokens of a text for a formatter kind, with the blanks. */
function allTokens(text, kind) {
  return kind === "json" ? parseJson(text).tokens : kind === "postgres" ? tokensPostgres(text) : tokensClickHouse(text);
}

/** Loads the module of the formatter, or gives null when the file is not there yet. */
export async function loadFormatter() {
  const file = join(ROOT, "web", "js", "editor", "ast", "format.js");
  if (!existsSync(file)) return null;
  return import(pathToFileURL(file).href);
}

/** The text that a formatter call gives. The call can give a string or an object with the text in `text`, `output` or `formatted`. */
export function formatText(result, input) {
  if (typeof result === "string") return result;
  if (result && typeof result === "object") {
    for (const key of ["text", "output", "formatted", "result"]) if (typeof result[key] === "string") return result[key];
  }
  throw new Error(`the formatter result has no text: ${JSON.stringify(result)?.slice(0, 120)} for input of ${input.length} units`);
}

/** Runs the formatter of a kind on a text and gives the raw result. */
export function runFormatRaw(fmt, text, kind) {
  return kind === "json" ? fmt.formatJson(text) : fmt.formatSql(text, kind);
}

/** Runs the formatter of a kind on a text and gives the text. */
export function runFormat(fmt, text, kind) {
  return formatText(runFormatRaw(fmt, text, kind), text);
}

// The reasons of a refusal of the formatter (the header of format.js).
const REFUSALS = new Set(["capped_too_large", "capped_too_deep", "capped_too_many_nodes", "output_too_large", "nothing_to_format", "error_node", "unclosed", "no_statement", "self_check"]);

/**
 * The faults of the shape of a formatter result: a refusal leaves the text unchanged and gives a known reason,
 * and `replace` turns the input into the text. A `self_check` refusal is a bug of the formatter: it is a fault too.
 */
function resultShapeFaults(raw, input) {
  const out = [];
  if (!raw || typeof raw !== "object") return out;
  if (raw.reason) {
    if (!REFUSALS.has(raw.reason)) out.push(`unknown refusal reason ${raw.reason}`);
    if (raw.reason === "self_check") out.push("the self check of the formatter failed (a bug of the formatter: it saw a token change)");
    if (raw.text !== input) out.push(`a refusal (${raw.reason}) changed the text`);
    if (raw.changed === true) out.push(`a refusal (${raw.reason}) has changed: true`);
  }
  if (raw.replace && typeof raw.text === "string") {
    const { from, to, text } = raw.replace;
    if (input.slice(0, from) + text + input.slice(to) !== raw.text) out.push("replace does not turn the input into the text");
  }
  if (typeof raw.changed === "boolean" && typeof raw.text === "string" && raw.changed !== (raw.text !== input)) out.push(`changed is ${raw.changed} but the text ${raw.text === input ? "is" : "is not"} the input`);
  return out;
}

/**
 * The faults of the formatter for a text and a kind: the same non-blank tokens, the idempotence,
 * and no line comment that gets code after it on its own line.
 */
export function formatFaults(fmt, text, kind) {
  const out = [];
  let once;
  try {
    const raw = runFormatRaw(fmt, text, kind);
    out.push(...resultShapeFaults(raw, text));
    once = formatText(raw, text);
  } catch (e) {
    return [`throws: ${e && e.message}`];
  }
  const a = significantTokens(text, kind);
  const b = significantTokens(once, kind);
  if (a.length !== b.length || a.some((x, i) => x !== b[i])) {
    let i = 0;
    while (i < a.length && a[i] === b[i]) i++;
    out.push(`the tokens change at non-blank token ${i}: ${JSON.stringify(a[i])} -> ${JSON.stringify(b[i])}`);
  }
  let twice;
  try {
    twice = runFormat(fmt, once, kind);
  } catch (e) {
    return [...out, `throws on its own output: ${e && e.message}`];
  }
  if (twice !== once) out.push("not idempotent: format(format(x)) differs from format(x)");
  for (const t of allTokens(once, kind)) {
    if (t.kind !== TOKEN.COMMENT) continue;
    const body = once.slice(t.start, t.end);
    if (body.startsWith("/*")) continue;
    if (t.end < once.length && once[t.end] !== "\n" && once[t.end] !== "\r") {
      out.push(`a line comment at ${t.start} is followed by code on its line`);
      break;
    }
  }
  return out;
}
