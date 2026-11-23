// SPDX-License-Identifier: MIT OR Apache-2.0
// Pure functions of the parity capture tool (15.14a). This module has no browser, no network and no file access
// (except `insideGitCheckout`), so that a unit test can run each function.
//
// The tool `capture.mjs` records the SQL, the interval and the data of each panel in Grafana and in Hune. This
// module normalises the SQL, compares both sides and gives each difference one cause.

import { createHash } from "node:crypto";
import { existsSync, realpathSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

// The causes of 15.14a, and `other` for each difference that no other cause explains.
export const CAUSES = ["escape", "interval", "format", "other"];

// A blank of the lexer of 7.5: space, TAB, LF, CR, FF, VT and the Unicode spaces of the table.
const BLANKS = "[ \\t\\n\\r\\f\\v\\u0085\\u00A0\\u180E\\u2000-\\u200D\\u2028\\u2029\\u202F\\u205F\\u2060\\u3000\\uFEFF]";
const BLANK_RUN = new RegExp(`${BLANKS}+`, "g");
const BLANK_END = new RegExp(`^${BLANKS}+|${BLANKS}+$`, "g");

// The lexer of 7.5, reduced to what the normaliser needs: it finds each comment and each literal. A span is
// `{ start, end }` in UTF-16 units. The browser copy of the lexer has no heredoc, so the tool has its own.
const IS_BLANK = new RegExp(`^${BLANKS}$`);
const ID_START = /[A-Za-z_]/;
const ID_PART = /[A-Za-z0-9_]/;

function spansOf(text) {
  const out = [];
  const n = text.length;
  let i = 0;
  while (i < n) {
    const c = text[i];
    const d = text[i + 1];
    if (c === "-" && d === "-") {
      let j = i;
      while (j < n && text[j] !== "\n" && text[j] !== "\r") j++;
      out.push({ start: i, end: j });
      i = j;
    } else if (c === "#" && d !== undefined && (IS_BLANK.test(d) || d === "!")) {
      let j = i;
      while (j < n && text[j] !== "\n" && text[j] !== "\r") j++;
      out.push({ start: i, end: j });
      i = j;
    } else if (c === "/" && d === "*") {
      let depth = 1;
      let j = i + 2;
      while (j < n && depth > 0) {
        if (text[j] === "/" && text[j + 1] === "*") {
          depth++;
          j += 2;
        } else if (text[j] === "*" && text[j + 1] === "/") {
          depth--;
          j += 2;
        } else j++;
      }
      out.push({ start: i, end: j });
      i = j;
    } else if (c === "'" || c === '"' || c === "`") {
      let j = i + 1;
      while (j < n) {
        if (text[j] === "\\") j += 2;
        else if (text[j] === c) {
          if (text[j + 1] === c) j += 2;
          else break;
        } else j++;
      }
      j = Math.min(n, j + 1);
      out.push({ start: i, end: j });
      i = j;
    } else if (c === "$") {
      // A heredoc: $tag$ ... $tag$ or $$ ... $$.
      let j = i + 1;
      if (ID_START.test(text[j] ?? "")) while (ID_PART.test(text[j] ?? "")) j++;
      if (text[j] === "$") {
        const tag = text.slice(i, j + 1);
        const close = text.indexOf(tag, j + 1);
        const end = close < 0 ? n : close + tag.length;
        out.push({ start: i, end });
        i = end;
      } else i++;
    } else i++;
  }
  return out;
}

/** The parts of a SQL text: `{ code: boolean, text }`. A part that is not code is a comment or a literal. */
export function parts(text) {
  const out = [];
  let at = 0;
  for (const span of spansOf(text)) {
    if (span.start > at) out.push({ code: true, text: text.slice(at, span.start) });
    out.push({ code: false, text: text.slice(span.start, span.end) });
    at = span.end;
  }
  if (at < text.length) out.push({ code: true, text: text.slice(at) });
  return out;
}

/**
 * The normal form of 15.14a: each run of blanks outside literals and comments becomes one space, and the blanks at
 * both ends of the text go. A literal or a comment stays as it is.
 */
export function normaliseSql(text) {
  const joined = parts(String(text)).map((p) => (p.code ? p.text.replace(BLANK_RUN, " ") : p.text)).join("");
  return joined.replace(BLANK_END, "");
}

// The tokens of a normalised text: each comment or literal is one token. A code run gives numbers, words, spaces
// and single marks.
function tokens(text) {
  const out = [];
  for (const p of parts(text)) {
    if (!p.code) out.push(p.text);
    else out.push(...(p.text.match(/\d+(?:\.\d+)?|[A-Za-z_]\w*|\s+|[^\s\w]/g) ?? []));
  }
  return out;
}

/** The interval in seconds that the macros of 7.4 use: `max(1, floor(I / 1000))`. */
export function intervalSeconds(intervalMs) {
  return Math.max(1, Math.floor(intervalMs / 1000));
}

/**
 * True when the two normalised texts differ only in number tokens that follow from the interval. A token that is
 * the interval in milliseconds on the left side must be the interval in milliseconds on the right side. The same
 * rule holds for the interval in seconds. The two texts must have the same number of tokens.
 */
export function explainedByInterval(left, right, leftMs, rightMs) {
  if (!Number.isFinite(leftMs) || !Number.isFinite(rightMs)) return false;
  const a = tokens(left);
  const b = tokens(right);
  if (a.length !== b.length) return false;
  const pairs = new Map([
    [String(leftMs), String(rightMs)],
    [String(intervalSeconds(leftMs)), String(intervalSeconds(rightMs))],
  ]);
  for (let i = 0; i < a.length; i++) {
    if (a[i] === b[i]) continue;
    if (pairs.get(a[i]) !== b[i]) return false;
  }
  return true;
}

/** The first difference of two texts: the common start and end are cut. Each window has 200 characters at most. */
export function differenceWindow(left, right) {
  let s = 0;
  while (s < left.length && s < right.length && left[s] === right[s]) s++;
  let e = 0;
  while (e < left.length - s && e < right.length - s && left[left.length - 1 - e] === right[right.length - 1 - e]) e++;
  const cut = (t) => {
    const w = t.slice(Math.max(0, s - 20), t.length - e);
    return w.length > 200 ? w.slice(0, 200) + "..." : w;
  };
  return { offset: s, grafana: cut(left), hune: cut(right), end_grafana: left.length - e, end_hune: right.length - e };
}

/** The SHA-256 of a text as 64 lower-case hex digits. This is the `sql_sha256` of the log line `query.run` (14). */
export function sha256(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

/** The text with each number token that is the interval of `fromMs` (in ms or in s) replaced by the one of `toMs`. */
export function substituteInterval(text, fromMs, toMs) {
  const pairs = new Map([
    [String(fromMs), String(toMs)],
    [String(intervalSeconds(fromMs)), String(intervalSeconds(toMs))],
  ]);
  return tokens(text).map((t) => pairs.get(t) ?? t).join("");
}

/**
 * Compare with the hash of the SQL that Hune sent, when the text of Hune is not available. The hash covers the
 * exact bytes, so a difference in blanks gives a difference. The function tests two texts: the SQL of Grafana, and the
 * SQL of Grafana with its interval numbers replaced by the numbers of the interval of Hune.
 */
export function compareSqlByHash({ grafana, huneHash, grafanaIntervalMs = null, huneIntervalMs = null }) {
  if (grafana === null || grafana === undefined || !huneHash) {
    return { status: "different", cause: "other", detail: { mode: "hash", reason: grafana == null ? "no SQL from Grafana" : "no hash from Hune" } };
  }
  if (sha256(grafana) === huneHash) return { status: "equal", cause: null, detail: { mode: "hash" } };
  if (grafanaIntervalMs !== null && huneIntervalMs !== null && grafanaIntervalMs !== huneIntervalMs) {
    if (sha256(substituteInterval(grafana, grafanaIntervalMs, huneIntervalMs)) === huneHash) {
      return { status: "different", cause: "interval", detail: { mode: "hash", explained: true } };
    }
  }
  return {
    status: "different",
    cause: "other",
    detail: { mode: "hash", reason: "the hash of Hune differs, and the text of Hune is not available to find the cause" },
  };
}

/**
 * Compare the executed SQL of one query on both sides.
 * Input: the two texts, the two intervals in ms (a number or null), and `values`, the variable values of the run.
 * Output: `{ status: "equal" | "different", cause, detail }`. The cause is one of CAUSES.
 */
export function compareSql({ grafana, hune, grafanaIntervalMs = null, huneIntervalMs = null, values = [] }) {
  if (grafana === null || grafana === undefined || hune === null || hune === undefined) {
    return { status: "different", cause: "other", detail: { reason: grafana == null ? "no SQL from Grafana" : "no SQL from Hune" } };
  }
  const g = normaliseSql(grafana);
  const v = normaliseSql(hune);
  if (g === v) return { status: "equal", cause: null, detail: null };
  const window = differenceWindow(g, v);
  if (grafanaIntervalMs !== huneIntervalMs && grafanaIntervalMs !== null && huneIntervalMs !== null) {
    if (explainedByInterval(g, v, grafanaIntervalMs, huneIntervalMs)) {
      return { status: "different", cause: "interval", detail: { ...window, explained: true } };
    }
    return {
      status: "different",
      cause: "other",
      detail: { ...window, reason: "the interval differs and the texts still differ when the interval is masked" },
    };
  }
  // A variable value in the difference: `escape` for a value with a quote or a backslash, else `format`.
  const hit = values.filter(
    (x) => x !== "" && (overlaps(g, x, window.offset, window.end_grafana) || [x, ...escapedForms(x)].some((f) => overlaps(v, f, window.offset, window.end_hune))),
  );
  if (hit.length) {
    const cause = hit.some((x) => /['\\]/.test(x)) ? "escape" : "format";
    return { status: "different", cause, detail: window };
  }
  return { status: "different", cause: "other", detail: window };
}

// True when `value` occurs in `text` at a place that overlaps the range [start, end) or lies at most 3 characters
// from it. The margin covers the quote marks and the comma that a variable format puts around a value.
function overlaps(text, value, start, end) {
  for (let at = text.indexOf(value); at >= 0; at = text.indexOf(value, at + 1)) {
    if (at < end + 3 && at + value.length > start - 3) return true;
  }
  return false;
}

function escapedForms(value) {
  return [value.replaceAll("\\", "\\\\").replaceAll("'", "\\'")];
}

// --- Data -----------------------------------------------------------------------------------------------------------

/** One table from a Grafana frame list, or null when the frames are not one frame. */
export function tableOfGrafana(frames) {
  if (!Array.isArray(frames) || frames.length !== 1) return null;
  const f = frames[0];
  const fields = f.schema?.fields ?? [];
  const values = f.data?.values ?? [];
  return {
    columns: fields.map((x, i) => ({ name: x.name, kind: x.type === "time" ? "time" : x.type, values: values[i] ?? [] })),
  };
}

/** The table of a Hune frame (7.16). */
export function tableOfHune(frame) {
  if (!frame || !Array.isArray(frame.columns)) return null;
  return {
    columns: frame.columns.map((c, i) => ({ name: c.name, kind: c.kind, values: frame.values?.[i] ?? [] })),
  };
}

// A cell as text with a type tag, so that the two sides compare. A time is milliseconds; Grafana gives milliseconds
// and Hune gives seconds (7.16).
function cell(kind, side, value) {
  if (value === null || value === undefined) return "null";
  if (kind === "time") return `t:${side === "hune" ? Math.round(Number(value) * 1000) : Number(value)}`;
  if (kind === "number" || kind === "number_text") return `n:${numberText(value)}`;
  if (typeof value === "number") return `n:${numberText(value)}`;
  if (typeof value === "boolean") return `b:${value}`;
  return `s:${value}`;
}

function numberText(value) {
  const n = Number(value);
  if (Number.isNaN(n) || !Number.isFinite(n)) return String(value).toLowerCase();
  return String(n);
}

const NUMERIC = new Set(["number", "number_text", "int64", "uint64", "float64", "int32", "uint32", "float32", "int16", "uint16", "int8", "uint8"]);

/**
 * Compare the data of one query.
 * `mode` is "exact" for a table (same columns, same rows, same cells) or "multiset" for a time series. A Grafana
 * time series frame is wide and the Hune frame is long (7.16), so the multiset mode compares only the numeric
 * values and the distinct times, without order and without the nulls.
 * Output: `{ status: "equal" | "different" | "not_compared", mode, detail }`.
 */
export function compareData(g, v, mode) {
  if (!g || !v) return { status: "not_compared", mode, detail: { reason: !g ? "no frame from Grafana" : "no frame from Hune" } };
  if (mode === "exact") {
    const gn = g.columns.map((c) => c.name);
    const vn = v.columns.map((c) => c.name);
    if (JSON.stringify(gn) !== JSON.stringify(vn)) return { status: "different", mode, detail: { reason: "columns", grafana: gn, hune: vn } };
    const gr = g.columns[0]?.values.length ?? 0;
    const vr = v.columns[0]?.values.length ?? 0;
    if (gr !== vr) return { status: "different", mode, detail: { reason: "rows", grafana: gr, hune: vr } };
    const rowsOf = (t, side) => Array.from({ length: gr }, (_, r) => JSON.stringify(t.columns.map((c) => cell(c.kind, side, c.values[r]))));
    const ga = rowsOf(g, "grafana");
    const va = rowsOf(v, "hune");
    for (let r = 0; r < gr; r++) {
      if (ga[r] === va[r]) continue;
      const gc = JSON.parse(ga[r]);
      const vc = JSON.parse(va[r]);
      const c = gc.findIndex((x, i) => x !== vc[i]);
      const sameRows = JSON.stringify([...ga].sort()) === JSON.stringify([...va].sort());
      return {
        status: "different",
        mode,
        detail: { reason: sameRows ? "row order" : "cell", row: r, column: gn[c], grafana: gc[c], hune: vc[c] },
      };
    }
    return { status: "equal", mode, detail: { rows: gr } };
  }
  const digest = (t, side) => {
    const nums = [];
    const times = new Set();
    for (const c of t.columns) {
      const isTime = c.kind === "time";
      const isNum = NUMERIC.has(String(c.kind).toLowerCase());
      if (!isTime && !isNum) continue;
      for (const x of c.values) {
        if (x === null || x === undefined) continue;
        if (isTime) times.add(cell("time", side, x));
        else nums.push(numberText(x));
      }
    }
    nums.sort();
    return { nums, times: [...times].sort() };
  };
  const a = digest(g, "grafana");
  const b = digest(v, "hune");
  if (a.times.join() !== b.times.join()) return { status: "different", mode, detail: { reason: "times", grafana: a.times.length, hune: b.times.length } };
  if (a.nums.join() !== b.nums.join()) return { status: "different", mode, detail: { reason: "values", grafana: a.nums.length, hune: b.nums.length } };
  return { status: "equal", mode, detail: { times: a.times.length, values: a.nums.length } };
}

// --- Panels and report ------------------------------------------------------------------------------------------------

/**
 * The record of Hune that pairs with the query of Grafana that has the ref `refId` (4.15a, 15.14c). `panel` is the
 * panel of Hune. When its requests had a `ref`, `refs` maps each ref to the record of its request. A ref that Hune
 * has no request for gives `null`. A panel whose requests have no `ref` has one query (the importer drops the
 * refId of such a panel, 4.42): its record pairs with the first query of Grafana, `index` 0.
 */
export function pairByRef(panel, refId, index = 0) {
  if (!panel) return null;
  if (!panel.explicit_ref) return index === 0 ? panel : null;
  return panel.refs.get(refId) ?? null;
}

/**
 * Compare one query of one panel. `g` and `v` hold what the tool captured:
 *   g: { sql, interval_ms, max_data_points, from_ms, to_ms, table, format }
 *   v: { sql or sql_hash, interval_ms, max_data_points, from_ms, to_ms, table }
 * Output: the record of the report for this query.
 */
export function comparePanelQuery(g, v, values = []) {
  const input = {
    grafana: g?.sql ?? null,
    grafanaIntervalMs: g?.interval_ms ?? null,
    huneIntervalMs: v?.interval_ms ?? null,
  };
  const sql = v?.sql_hash ? compareSqlByHash({ ...input, huneHash: v.sql_hash }) : compareSql({ ...input, hune: v?.sql ?? null, values });
  const item = (a, b) => ({ status: a === b ? "equal" : "different", cause: a === b ? null : "interval", grafana: a ?? null, hune: b ?? null });
  const interval = item(g?.interval_ms, v?.interval_ms);
  const maxDataPoints = item(g?.max_data_points, v?.max_data_points);
  const range = {
    status: g?.from_ms === v?.from_ms && g?.to_ms === v?.to_ms ? "equal" : "different",
    cause: g?.from_ms === v?.from_ms && g?.to_ms === v?.to_ms ? null : "other",
    grafana: [g?.from_ms ?? null, g?.to_ms ?? null],
    hune: [v?.from_ms ?? null, v?.to_ms ?? null],
  };
  const mode = g?.format === 1 ? "exact" : "multiset";
  let data = compareData(g?.table ?? null, v?.table ?? null, mode);
  if (g?.error || v?.error) {
    // An error of one tool is a difference of its own, whatever the frames hold.
    data = { status: "different", mode, detail: { reason: "error", grafana: g?.error ?? null, hune: v?.error ?? null }, cause: "other" };
  }
  // The data of two queries with the same SQL cannot differ because of the interval: the cause is then `other`.
  if (data.status === "different" && !data.cause) data.cause = sql.cause === "interval" ? "interval" : "other";
  const different = [sql, interval, maxDataPoints, range, data].some((x) => x.status === "different");
  return { sql, interval, max_data_points: maxDataPoints, range, data, status: different ? "different" : "equal" };
}

/** The totals of a list of panel records: the number of records for each status and for each cause of the SQL. */
export function totals(records) {
  const t = { queries: records.length, equal: 0, different: 0, sql: { equal: 0, different: 0 }, sql_causes: {}, causes: {} };
  for (const c of CAUSES) {
    t.causes[c] = 0;
    t.sql_causes[c] = 0;
  }
  for (const r of records) {
    t[r.status]++;
    t.sql[r.sql.status]++;
    if (r.sql.cause) t.sql_causes[r.sql.cause]++;
    for (const key of ["sql", "interval", "max_data_points", "range", "data"]) {
      const cause = r[key].cause;
      if (cause) t.causes[cause] = (t.causes[cause] ?? 0) + 1;
    }
  }
  return t;
}

/** Make the Markdown text of the report from its JSON. The text holds no data value. */
export function renderMarkdown(report) {
  const L = [];
  L.push("# Parity capture report", "");
  L.push(`Tool: ${report.tool} (15.14a). Created: ${report.created_at}.`, "");
  L.push("## Run", "");
  L.push("| Item | Value |", "|---|---|");
  for (const [k, v] of Object.entries(report.run)) L.push(`| ${k} | ${typeof v === "object" ? "`" + JSON.stringify(v) + "`" : v} |`);
  L.push("", "## Totals", "");
  L.push(`Queries compared: ${report.totals.queries}. Equal: ${report.totals.equal}. Different: ${report.totals.different}.`, "");
  L.push(`SQL texts: ${report.totals.sql.equal} equal, ${report.totals.sql.different} different.`, "");
  L.push("| Cause | SQL | All checks |", "|---|---|---|");
  for (const [c, n] of Object.entries(report.totals.causes)) L.push(`| ${c} | ${report.totals.sql_causes[c]} | ${n} |`);
  L.push("", "The column `All checks` adds the SQL, the interval, `maxDataPoints`, the range and the data checks of each query.", "");
  L.push("## Panels", "");
  L.push("| Panel | Title | Type | Query | SQL | Interval (G / V) | maxDataPoints (G / V) | Width px (G / V) | Data |", "|---|---|---|---|---|---|---|---|---|");
  for (const p of report.panels) {
    const sql = p.sql.status === "equal" ? "equal" : `different: ${p.sql.cause}${p.sql.detail?.explained ? " (explained)" : ""}`;
    const data = p.data.status === "equal" ? `equal (${p.data.mode})` : p.data.status === "different" ? `different: ${p.data.cause} (${p.data.detail?.reason})` : "not compared";
    const iv = `${p.interval.grafana ?? "-"} / ${p.interval.hune ?? "-"}`;
    const mdp = `${p.max_data_points.grafana ?? "-"} / ${p.max_data_points.hune ?? "-"}`;
    const title = String(p.title ?? "").replaceAll("|", "\\|");
    const w = `${p.panel_width_px?.grafana ?? "-"} / ${p.panel_width_px?.hune ?? "-"}`;
    L.push(`| ${p.id} | ${title} | ${p.type ?? ""} | ${p.ref_id ?? ""} | ${sql} | ${iv} | ${mdp} | ${w} | ${data} |`);
  }
  const notes = report.panels.filter((p) => p.notes?.length || p.errors?.length);
  if (notes.length) {
    L.push("", "## Notes", "");
    for (const p of notes) {
      for (const n of p.notes ?? []) L.push(`- Panel ${p.id}: ${n}`);
      for (const e of p.errors ?? []) L.push(`- Panel ${p.id}: error. ${String(e).slice(0, 300)}`);
    }
  }
  const diffs = report.panels.filter((p) => p.sql.status === "different");
  if (diffs.length) {
    L.push("", "## Differences of the SQL", "");
    for (const p of diffs) {
      const d = p.sql.detail ?? {};
      L.push(`### Panel ${p.id} ${p.title ?? ""}`, "", `Cause: ${p.sql.cause}.`);
      if (d.explained) L.push("The difference is only the interval numbers: the texts are equal when the interval of one side replaces the other.");
      if (d.mode === "hash") L.push("Mode: hash of the SQL of Hune. The text of Hune is not available.");
      if (d.reason) L.push(`Reason: ${d.reason}.`);
      if (d.grafana !== undefined) {
        L.push("", "Grafana:", "", "```text", d.grafana, "```", "", "Hune:", "", "```text", d.hune, "```");
      }
      L.push("");
    }
  }
  if (report.missing.length) {
    L.push("", "## Panels not captured", "");
    for (const m of report.missing) L.push(`- Panel ${m.id} (${m.title ?? ""}): ${m.reason}.`);
  }
  L.push("");
  return L.join("\n");
}

// --- Arguments and paths -----------------------------------------------------------------------------------------------

/** A directory is in a git checkout when it or an ancestor holds a `.git` entry (15.2). The nearest ancestor that exists is resolved. */
export function insideGitCheckout(path) {
  let p = resolve(path);
  while (!existsSync(p)) {
    const up = dirname(p);
    if (up === p) break;
    p = up;
  }
  p = realpathSync(p);
  for (;;) {
    if (existsSync(join(p, ".git"))) return true;
    const up = dirname(p);
    if (up === p) return false;
    p = up;
  }
}

/** The fixed ranges of 15.14a. D is 00:00 UTC of the day of `now`. It returns `{ from_ms, to_ms }` or null. */
export function namedRange(name, now = Date.now()) {
  const D = Math.floor(now / 86400000) * 86400000;
  const span = { R1: 3600000, R2: 86400000, R3: 7 * 86400000 }[name];
  return span === undefined ? null : { from_ms: D - span, to_ms: D };
}

/** A time as a whole number of milliseconds. It takes digits, or an ISO text that ends with Z. It returns null if it fails. */
export function parseTime(text) {
  if (/^\d{10,}$/.test(text)) return Number(text);
  if (/^\d{4}-\d{2}-\d{2}T[\d:.]+Z$/.test(text)) {
    const t = Date.parse(text);
    return Number.isNaN(t) ? null : t;
  }
  return null;
}

/** A `--var name=value` argument. It returns `[name, value]` or null. */
export function parseVar(text) {
  const i = text.indexOf("=");
  if (i < 1) return null;
  return [text.slice(0, i), text.slice(i + 1)];
}

/**
 * The query string of the dashboard URL: absolute from and to, then each `var-` value (15.14a). Hune reads the times as
 * milliseconds. Grafana 12.4.2 ignores a `from` and a `to` in milliseconds on a dashboard URL and shows its own range, so the
 * Grafana form is `iso`: UTC times as `2026-11-13T00:00:00.000Z` (measured 2026-11-15).
 */
export function stateQuery(from_ms, to_ms, vars, iso = false) {
  const time = (ms) => (iso ? new Date(ms).toISOString() : String(ms));
  const p = [`from=${time(from_ms)}`, `to=${time(to_ms)}`];
  for (const [n, v] of vars) p.push(`var-${encodeURIComponent(n)}=${encodeURIComponent(v)}`);
  return p.join("&");
}

// --- Variable queries -----------------------------------------------------------------------------------------------------

const PLACEHOLDER = /\$(?:\{[A-Za-z0-9_]+(?::[A-Za-z0-9_]+)?\}|[A-Za-z0-9_]+)/g;

/**
 * Pair the SQL of a Grafana variable request with the query variables of the dashboard (15.14a).
 * `variables` is a list of `{ name, sql }` with the SQL as the author wrote it. `requestSql` is the SQL that Grafana sent.
 * A variable matches when its normalised SQL equals the normalised request, or when the request fits its SQL with a
 * placeholder replaced by any text. The result is the name when exactly one variable matches, else null with the
 * count of matches.
 */
export function pairVariable(variables, requestSql) {
  const request = normaliseSql(requestSql);
  const hits = [];
  for (const v of variables) {
    const template = normaliseSql(v.sql);
    if (template === request) {
      hits.push(v.name);
      continue;
    }
    const pattern = template
      .split(PLACEHOLDER)
      .map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
      .join("[\\s\\S]*?");
    if (new RegExp(`^${pattern}$`).test(request)) hits.push(v.name);
  }
  return { name: hits.length === 1 ? hits[0] : null, matches: hits.length };
}
