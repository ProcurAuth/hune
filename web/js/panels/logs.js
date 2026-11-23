// The logs panel (9.19h): one line for each row of the frame, with the time, the level and the body (7.16e), and a details row with the
// other fields of a line. A log line is untrusted text (threat T104): it goes in the DOM with textContent only, and the caps below bound the
// lines and the characters that the page writes. The page builds the model first (`buildLogs`, no DOM access), then the DOM.
import { formatTime } from "../zone.js";
import { emptyState } from "../ui.js";

/** The lines that the panel draws at most (9.19h, section 4). */
export const MAX_LOG_LINES = 10000;
/** The characters (UTF-16 code units) of one body or one detail value that the panel shows at most. A longer text ends with "…". */
export const MAX_LOG_LINE_CHARS = 10000;
/** The characters of all the bodies of the panel. The line that passes this cap and the lines after it are not drawn. */
export const MAX_LOG_CHARS = 2000000;

// The level words of Grafana 12.4.2 (LogLevel), in its order, and the level of each word.
const WORDS = {
  emerg: "critical", fatal: "critical", alert: "critical", crit: "critical", critical: "critical",
  warn: "warning", warning: "warning", err: "error", eror: "error", error: "error",
  info: "info", information: "info", informational: "info", notice: "info", dbug: "debug", debug: "debug", trace: "trace", unknown: "unknown",
};
// The syslog severities 0 to 7 of a level value that is a number, or a text of digits with no 0 before another digit.
const SYSLOG = { 0: "critical", 1: "critical", 2: "critical", 3: "error", 4: "warning", 5: "info", 6: "info", 7: "debug" };
// One pass finds the first level word of a body: a word of the list with a word boundary on each side, in any case.
const SCAN = new RegExp(`\\b(?:${Object.keys(WORDS).join("|")})\\b`, "i");
// An ANSI colour sequence (ESC [ digits and ; m) at one place: Grafana removes it from the text of a line.
const ANSI = /\x1b\[[0-9;]*m/y;
const NEWLINES = /\r\n|\n|\r/g;
/** The text of the level column for each level. `unknown` shows no text. */
export const LEVEL_TEXT = { critical: "CRIT", error: "ERROR", warning: "WARN", info: "INFO", debug: "DEBUG", trace: "TRACE", unknown: "" };

/** The level of a value of the level field (Grafana getLogLevelFromKey): a level word in any case, or a syslog severity. */
export function levelOfValue(v) {
  const key = String(v).toLowerCase();
  if (Object.hasOwn(WORDS, key)) return WORDS[key];
  // A text is a severity only when it is the decimal text of its own integer ("3", not "03" or "3x").
  if (typeof v === "string" && v.length !== String(parseInt(v, 10)).length) return "unknown";
  return Object.hasOwn(SYSLOG, v) ? SYSLOG[v] : "unknown";
}

/** The level of a body with no usable level value (Grafana getLogLevel): the level of the first level word, or `unknown`. */
export function levelOfBody(text) {
  const m = SCAN.exec(text);
  return m ? WORDS[m[0].toLowerCase()] : "unknown";
}

/**
 * The text with no ANSI colour sequence, cut after `max` + 1 code units: the same text as a removal in the whole text, then the cut. The
 * walk stops at the cut, so a body of 32 MiB costs only its start (9.19h, section 4). A sequence across the cut point also goes.
 */
export function stripAnsi(text, max = MAX_LOG_LINE_CHARS) {
  let out = "";
  let i = 0;
  while (i < text.length && out.length <= max) {
    const room = max + 1 - out.length;
    const e = text.slice(i, i + room).indexOf("\x1b");
    if (e < 0) return out + text.slice(i, i + room);
    out += text.slice(i, i + e);
    ANSI.lastIndex = i + e;
    if (ANSI.test(text)) i = ANSI.lastIndex;
    else {
      out += "\x1b";
      i += e + 1;
    }
  }
  return out;
}

/** The first `max` code units of a text, with no half of a surrogate pair, and "…" after a cut. */
export function cutText(text, max = MAX_LOG_LINE_CHARS) {
  if (text.length <= max) return text;
  let end = max;
  const c = text.charCodeAt(end - 1);
  if (c >= 0xd800 && c <= 0xdbff) end--;
  return `${text.slice(0, end)}…`;
}

// The value of a field at a row, as the frame of 9.12 holds it: a text, a number, a boolean or null.
function valueAt(f, i) {
  if (f.nulls && f.nulls[i] === 1) return null;
  const v = f.values[i];
  return v === undefined ? null : v;
}

/**
 * The model of a logs panel (9.19h, 7.16e). `fields` are the fields of the frame, `options` the options of 4.33f, `zone` the zone of the
 * dashboard. It gives `{ error }` when the frame has no time field or no text field, else
 * `{ lines, total, cut, levelChars }`: `lines` in the order of the option `order`, each `{ row, time, level, body }`; `total` the rows of the
 * frame; `cut` is `null`, `"lines"` or `"chars"` when a cap stops the lines; `levelChars` the longest level text.
 */
export function buildLogs(fields, options = {}, zone = "browser") {
  const timeField = fields.find((f) => f.kind === "time");
  const bodyField = fields.find((f) => f.kind === "text");
  if (!timeField || !bodyField) return { error: "The data needs a time column and a text column to show log lines." };
  const levelField = fields.find((f) => f.name === "level") || fields.find((f) => f.name === "detected_level") || null;
  const total = timeField.values.length;
  const desc = options.order !== "oldest_first";
  const wrap = options.wrap === true;
  const t = timeField.values;
  // A stable sort by time: rows with the same time keep the order of the frame, and a row with no time comes last in both orders.
  const order = Array.from({ length: total }, (_, i) => i);
  order.sort((a, b) => {
    const na = Number.isNaN(t[a]);
    const nb = Number.isNaN(t[b]);
    if (na || nb) return na - nb;
    return desc ? t[b] - t[a] : t[a] - t[b];
  });
  const lines = [];
  let chars = 0;
  let cut = null;
  let levelChars = 0;
  for (const row of order) {
    if (lines.length === MAX_LOG_LINES) {
      cut = "lines";
      break;
    }
    const raw = valueAt(bodyField, row);
    const text = cutText(raw === null ? "" : stripAnsi(String(raw)));
    chars += text.length;
    if (chars > MAX_LOG_CHARS) {
      cut = "chars";
      break;
    }
    // A level value that is null, "", 0, NaN, false or true gives the level of the body, as in Grafana.
    const v = levelField ? valueAt(levelField, row) : null;
    const level = (typeof v === "string" && v !== "") || (typeof v === "number" && v !== 0 && !Number.isNaN(v)) ? levelOfValue(v) : levelOfBody(text);
    levelChars = Math.max(levelChars, LEVEL_TEXT[level].length);
    lines.push({ row, time: options.show_time === true ? formatTime(t[row], zone, "YYYY-MM-DD HH:mm:ss.SSS") : null, level, body: wrap ? text : text.replace(NEWLINES, "") });
  }
  return { lines, total, cut, levelChars, timeField, bodyField };
}

/** The fields of the details row of a line: each field but the time field and the body field, with a value at the row, in the frame order. */
export function detailsOf(fields, model, row) {
  const out = [];
  for (const f of fields) {
    if (f === model.timeField || f === model.bodyField) continue;
    const v = valueAt(f, row);
    if (v !== null) out.push([f.name, cutText(String(v))]);
  }
  return out;
}

const span = (doc, cls, text) => {
  const s = doc.createElement("span");
  s.className = cls;
  s.textContent = text;
  return s;
};

// The note of a cap: the panel shows the first lines in its order.
const capNote = (m) => `The panel shows the first ${m.lines.length.toLocaleString("en-US")} of ${m.total.toLocaleString("en-US")} lines (${m.cut === "lines" ? "the limit of lines" : "the limit of characters"}).`;

// Draws a panel of type `logs` in `host` (9.19h). The text of a line, a level and a detail goes in with textContent only (T104).
export function render(host, ctx) {
  const { panel, fields, zone, key } = ctx;
  const o = panel.options || {};
  const doc = host.ownerDocument || document;
  const m = buildLogs(fields, o, zone);
  if (m.error) {
    const empty = emptyState({ icon: null, title: "Cannot show log lines", text: m.error, inline: true, testid: `logs-${key}-shape` });
    host.append(empty);
    return { destroy: () => empty.remove() };
  }
  const details = o.details !== false;
  const list = doc.createElement("div");
  list.className = `logs${o.wrap === true ? " logs--wrap" : ""}`;
  list.setAttribute("role", "list");
  list.dataset.testid = `logs-${key}`;
  // The test hook of 9.29: the count of the lines, the rows of the frame and the cap.
  list.dataset.logs = JSON.stringify({ lines: m.lines.length, total: m.total, cut: m.cut });
  // The level column is as wide as the longest level text of the panel, 1 character at least (Grafana 12.4.2).
  list.style.setProperty("--logs-level", `${Math.max(1, m.levelChars)}ch`);
  if (!details) list.tabIndex = 0;
  if (m.cut) {
    const note = doc.createElement("p");
    note.className = "logs__note";
    note.setAttribute("role", "note");
    note.textContent = capNote(m);
    list.append(note);
  }
  const els = [];
  for (const line of m.lines) {
    const row = doc.createElement("div");
    row.className = "logs__line";
    row.dataset.i = String(els.length);
    if (details) {
      row.setAttribute("role", "button");
      row.setAttribute("aria-expanded", "false");
      row.tabIndex = els.length === 0 ? 0 : -1;
    } else row.setAttribute("role", "listitem");
    if (line.time !== null) row.append(span(doc, "logs__time", line.time));
    row.append(span(doc, `logs__level logs__level--${line.level}`, LEVEL_TEXT[line.level]), span(doc, "logs__body", line.body));
    els.push(row);
    list.append(row);
  }
  const open = new Map();
  // The details row of a line opens below it and closes on a second click, Enter or Space (9.19h, section 3).
  const toggle = (row) => {
    const i = Number(row.dataset.i);
    const was = open.get(i);
    if (was) {
      was.remove();
      open.delete(i);
      row.setAttribute("aria-expanded", "false");
      return;
    }
    const box = doc.createElement("div");
    box.className = "logs__details";
    box.dataset.testid = `logs-${key}-details`;
    const items = detailsOf(fields, m, m.lines[i].row);
    box.append(span(doc, "logs__dhead", "Fields"));
    if (items.length === 0) box.append(span(doc, "logs__dnone", "The line has no other field."));
    else {
      const dl = doc.createElement("dl");
      for (const [name, value] of items) {
        const dt = doc.createElement("dt");
        dt.textContent = name;
        const dd = doc.createElement("dd");
        dd.textContent = value;
        dl.append(dt, dd);
      }
      box.append(dl);
    }
    row.after(box);
    open.set(i, box);
    row.setAttribute("aria-expanded", "true");
  };
  const lineOf = (target) => (target && target.dataset && target.dataset.i !== undefined ? target : target && target.parentElement && target.parentElement.dataset.i !== undefined ? target.parentElement : null);
  // One tab stop in the list: the arrow keys, Home and End move the focus from line to line (9.26).
  const move = (from, to) => {
    if (!to || to === from) return;
    from.tabIndex = -1;
    to.tabIndex = 0;
    to.focus();
  };
  // A click that ends a selection of text keeps the details as they are, so that a person can copy a part of a line.
  const onClick = (e) => {
    const row = lineOf(e.target);
    const picked = typeof getSelection === "function" && String(getSelection()) !== "";
    if (row && !picked) toggle(row);
  };
  const onKey = (e) => {
    const row = lineOf(e.target);
    if (!row) return;
    const i = Number(row.dataset.i);
    const to = { ArrowDown: els[i + 1], ArrowUp: els[i - 1], Home: els[0], End: els[els.length - 1] }[e.key];
    if (e.key === "Enter" || e.key === " ") toggle(row);
    else if (to !== undefined) move(row, to);
    else return;
    e.preventDefault();
  };
  if (details) {
    list.addEventListener("click", onClick);
    list.addEventListener("keydown", onKey);
  }
  host.append(list);
  return { destroy: () => list.remove() };
}
