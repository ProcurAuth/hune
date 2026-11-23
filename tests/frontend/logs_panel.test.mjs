// SPDX-License-Identifier: MIT OR Apache-2.0
// The logs panel (9.19h, 7.16e, threat T104): FE-81 (the level rules of Grafana 12.4.2), the order, the time, the caps of lines and
// characters, hostile lines that stay text, the details row and its keys, and the time of 10,000 lines.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { FakeNode, installFakeDom } from "./fake_dom.mjs";
import {
  LEVEL_TEXT, MAX_LOG_CHARS, MAX_LOG_LINES, MAX_LOG_LINE_CHARS, buildLogs, cutText, detailsOf, levelOfBody, levelOfValue, render, stripAnsi,
} from "../../web/js/panels/logs.js";

const here = (p) => new URL(p, import.meta.url);
const vectors = JSON.parse(readFileSync(here("./vectors/FE-81-logs.json"), "utf8"));
const source = readFileSync(here("../../web/js/panels/logs.js"), "utf8");

installFakeDom();
// The fake DOM has no `after`: the details row goes after its line.
FakeNode.prototype.after ??= function after(node) {
  const list = this.parent.children;
  list.splice(list.indexOf(this) + 1, 0, node);
  node.parent = this.parent;
};

// --- the fields of a frame (9.12) -------------------------------------------------------------------------------------------------

const T0 = 1794211200000;
const timeField = (ms, name = "timestamp") => ({ name, type: "DateTime64(3)", kind: "time", values: Float64Array.from(ms, (v) => (v === null ? NaN : v)), nulls: Uint8Array.from(ms, (v) => (v === null ? 1 : 0)), config: {} });
const textField = (name, values) => ({ name, type: "String", kind: "text", values, config: {} });
const numberField = (name, values) => ({ name, type: "UInt8", kind: "number", values: Float64Array.from(values, (v) => (v === null ? NaN : v)), nulls: Uint8Array.from(values, (v) => (v === null ? 1 : 0)), config: {} });
const frame = (n, body = (i) => `line ${i}`, level = (i) => ["info", "error", "warn"][i % 3]) => [
  timeField(Array.from({ length: n }, (_, i) => T0 + i * 60000)),
  textField("body", Array.from({ length: n }, (_, i) => body(i))),
  textField("level", Array.from({ length: n }, (_, i) => level(i))),
];
const valueOf = (v) => (v && typeof v === "object" && "number" in v ? Number(v.number) : v);

// --- FE-81: the level rules -------------------------------------------------------------------------------------------------------

test("FE-81: the level of a row is the level of Grafana 12.4.2 (the value of the level field, else the first level word of the body)", () => {
  assert.ok(vectors.rows.length >= 200 && vectors.bodies.length >= 200);
  for (const v of vectors.rows) {
    const value = valueOf(v.value);
    const got = (typeof value === "string" && value !== "") || (typeof value === "number" && value !== 0 && !Number.isNaN(value)) ? levelOfValue(value) : levelOfBody(v.body);
    assert.equal(got, v.level, `value ${JSON.stringify(v.value)}, body ${JSON.stringify(v.body)}`);
  }
  for (const v of vectors.bodies) assert.equal(levelOfBody(v.body), v.level, JSON.stringify(v.body.slice(0, 60)));
  assert.deepEqual(LEVEL_TEXT, vectors.text);
});

test("FE-81: buildLogs gives the same level as the vectors, through the level field and through the body", () => {
  const rows = vectors.rows.filter((r) => typeof r.value === "string" || r.value === null);
  const fields = [timeField(rows.map((_, i) => T0 + i)), textField("body", rows.map((r) => r.body)), textField("level", rows.map((r) => r.value))];
  const m = buildLogs(fields, { order: "oldest_first" }, "utc");
  assert.deepEqual(m.lines.map((l) => l.level), rows.map((r) => r.level));
  // A number field named level: 0 and null give the level of the body, as in Grafana.
  const n = buildLogs([timeField([T0, T0 + 1, T0 + 2, T0 + 3]), textField("body", ["a", "b", "an error", "x"]), numberField("level", [7, 3, 0, null])], { order: "oldest_first" }, "utc");
  assert.deepEqual(n.lines.map((l) => l.level), ["debug", "error", "error", "unknown"]);
  // `detected_level` serves when no field is named `level`; another name such as `severity` does not.
  const d = buildLogs([timeField([T0]), textField("body", ["x"]), textField("detected_level", ["warn"])], {}, "utc");
  assert.equal(d.lines[0].level, "warning");
  const s = buildLogs([timeField([T0]), textField("body", ["x"]), textField("severity", ["error"])], {}, "utc");
  assert.equal(s.lines[0].level, "unknown");
  // A name of the prototype of an object is no level.
  for (const key of ["constructor", "toString", "__proto__", "hasOwnProperty", "length"]) assert.equal(levelOfValue(key), "unknown", key);
});

// --- the columns, the order, the time ---------------------------------------------------------------------------------------------

test("the first time field is the time, the first text field is the body, and a frame without one of them gives the message", () => {
  // The level field comes first: it is also the body, as in Grafana.
  const m = buildLogs([textField("level", ["error"]), timeField([T0]), textField("body", ["b"])], { show_time: true }, "utc");
  assert.equal(m.lines[0].body, "error");
  assert.equal(m.lines[0].level, "error");
  assert.ok(buildLogs([textField("body", ["b"])], {}, "utc").error.includes("time column and a text column"));
  assert.ok(buildLogs([timeField([T0]), numberField("n", [1])], {}, "utc").error);
});

test("the order: newest first by default, oldest first with the option; equal times keep the frame order; no time comes last", () => {
  const fields = [timeField([T0 + 2, null, T0, T0 + 2, T0 + 1]), textField("body", ["a", "b", "c", "d", "e"])];
  assert.deepEqual(buildLogs(fields, {}, "utc").lines.map((l) => l.body), ["a", "d", "e", "c", "b"]);
  assert.deepEqual(buildLogs(fields, { order: "newest_first" }, "utc").lines.map((l) => l.body), ["a", "d", "e", "c", "b"]);
  assert.deepEqual(buildLogs(fields, { order: "oldest_first" }, "utc").lines.map((l) => l.body), ["c", "e", "a", "d", "b"]);
});

test("the time shows with milliseconds in the zone of the dashboard, only with show_time; a null time is empty", () => {
  const fields = [timeField([T0 + 123, null]), textField("body", ["a", "b"])];
  assert.deepEqual(buildLogs(fields, {}, "utc").lines.map((l) => l.time), [null, null]);
  assert.deepEqual(buildLogs(fields, { show_time: true }, "utc").lines.map((l) => l.time), ["2026-11-09 08:00:00.123", ""]);
  assert.equal(buildLogs(fields, { show_time: true }, "Asia/Tokyo").lines[0].time, "2026-11-09 17:00:00.123");
});

test("wrap: a body keeps its line ends; without wrap they leave the line; an ANSI colour code leaves the text in both", () => {
  const fields = [timeField([T0]), textField("body", ["nl1\nnl2\r\nnl3\rx \u001b[31mred\u001b[0m"])];
  assert.equal(buildLogs(fields, {}, "utc").lines[0].body, "nl1nl2nl3x red");
  assert.equal(buildLogs(fields, { wrap: true }, "utc").lines[0].body, "nl1\nnl2\r\nnl3\rx red");
});

test("the level column is as wide as the longest level text, and a null body is an empty line", () => {
  assert.equal(buildLogs(frame(3), {}, "utc").levelChars, 5);
  assert.equal(buildLogs(frame(3, undefined, () => "info"), {}, "utc").levelChars, 4);
  assert.equal(buildLogs(frame(3, () => null, () => "x"), {}, "utc").levelChars, 0);
  assert.equal(buildLogs(frame(1, () => null), {}, "utc").lines[0].body, "");
});

// --- the caps ---------------------------------------------------------------------------------------------------------------------

test("caps: 10,000 lines at most, the newest first; one more row gives the cut `lines`", () => {
  assert.equal(MAX_LOG_LINES, 10000);
  const at = buildLogs(frame(10000), {}, "utc");
  assert.equal(at.lines.length, 10000);
  assert.equal(at.cut, null);
  const over = buildLogs(frame(10001), {}, "utc");
  assert.equal(over.lines.length, 10000);
  assert.equal(over.cut, "lines");
  assert.equal(over.total, 10001);
  assert.equal(over.lines[0].body, "line 10000");
});

test("caps: a body shows 10,000 characters at most with no half of a pair; the bodies show 2,000,000 characters at most", () => {
  assert.equal(MAX_LOG_LINE_CHARS, 10000);
  assert.equal(MAX_LOG_CHARS, 2000000);
  assert.equal(cutText("a".repeat(10000)), "a".repeat(10000));
  assert.equal(cutText("a".repeat(10001)), `${"a".repeat(10000)}…`);
  const pair = `${"a".repeat(9999)}\u{1F600}b`;
  assert.equal(cutText(pair), `${"a".repeat(9999)}…`);
  // 201 bodies of 10,000 characters: 200 lines (2,000,000 characters), then the cut `chars`.
  const m = buildLogs(frame(201, () => "x".repeat(20000)), {}, "utc");
  assert.equal(m.cut, "chars");
  assert.ok(m.lines.reduce((s, l) => s + l.body.length, 0) <= MAX_LOG_CHARS);
  assert.equal(m.lines.length, Math.floor(MAX_LOG_CHARS / 10001));
});

// --- the DOM: text only -----------------------------------------------------------------------------------------------------------

const HOSTILE = ["<img src=x onerror=alert(1)>", "<script>alert(1)</script>", "&amp; &lt;", "rtl ‮abc", "javascript:alert(1)", "\u0000\u0007bell", '"><svg onload=alert(1)>'];

/** A click on a line: the list holds the one listener (a delegate), and the fake DOM has no bubbling. */
const click = (list, line) => {
  for (const fn of list.listeners.get("click") || []) fn({ type: "click", target: line });
};

function draw(fields, options = {}, key = 7) {
  const host = new FakeNode("div");
  const panel = { id: key, type: "logs", options };
  const handle = render(host, { panel, fields, names: fields.map((f) => f.name), zone: "utc", key });
  return { host, handle, list: host.children[0] };
}

test("T104: a hostile line, level and detail stay text: textContent only, no markup, no event attribute", () => {
  const fields = [timeField(HOSTILE.map((_, i) => T0 + i)), textField("body", HOSTILE), textField("level", HOSTILE), textField("host", HOSTILE)];
  const { list } = draw(fields, { order: "oldest_first", show_time: true });
  const lines = list.querySelectorAll(".logs__line");
  assert.equal(lines.length, HOSTILE.length);
  lines.forEach((line, i) => {
    assert.equal(line.querySelector(".logs__body").textContent, HOSTILE[i].replace(/\r\n|\n|\r/g, ""));
    assert.deepEqual([...line.attrs.keys()].sort(), ["aria-expanded", "role"]);
  });
  // The details of a hostile row: names and values are text nodes of dt and dd.
  click(list, lines[0]);
  const dd = list.querySelectorAll("dd").map((n) => n.textContent);
  assert.deepEqual(dd, [HOSTILE[0], HOSTILE[0]]);
  for (const re of [/innerHTML/, /outerHTML/, /insertAdjacentHTML/, /document\.write/, /createContextualFragment/, /DOMParser/, /\beval\s*\(/, /\bFunction\s*\(/, /setAttribute\(\s*["']on/, /\.href\s*=/, /\.src\s*=/]) {
    assert.ok(!re.test(source), `logs.js matches ${re}`);
  }
});

test("the DOM: one line for each row, the hook, the level class and text, the time only with show_time, the note of a cap", () => {
  const { list } = draw(frame(4), {});
  assert.equal(list.dataset.testid, "logs-7");
  assert.deepEqual(JSON.parse(list.dataset.logs), { lines: 4, total: 4, cut: null });
  const first = list.querySelectorAll(".logs__line")[0];
  assert.equal(first.querySelectorAll(".logs__time").length, 0);
  assert.equal(first.querySelector(".logs__level").className, "logs__level logs__level--info");
  assert.equal(first.querySelector(".logs__level").textContent, "INFO");
  const withTime = draw(frame(2), { show_time: true }).list;
  assert.equal(withTime.querySelector(".logs__time").textContent, "2026-11-09 08:01:00.000");
  assert.equal(draw(frame(2), { wrap: true }).list.className, "logs logs--wrap");
  const cut = draw(frame(10001), {}).list;
  assert.equal(cut.querySelector(".logs__note").textContent, "The panel shows the first 10,000 of 10,001 lines (the limit of lines).");
  assert.equal(cut.querySelectorAll(".logs__line").length, 10000);
});

test("the shape message: a frame with no text field shows the empty state, with no line", () => {
  const { host } = draw([timeField([T0]), numberField("n", [1])]);
  assert.equal(host.querySelectorAll(".logs__line").length, 0);
  assert.ok(host.querySelector('[data-testid="logs-7-shape"]').textContent.includes("Cannot show log lines"));
});

// --- the details row and the keys -------------------------------------------------------------------------------------------------

test("details: a click opens the fields of the line under it (no time, no body, no null, frame order) and a second click closes it", () => {
  const fields = [timeField([T0, T0 + 1]), textField("body", ["b0", "b1"]), textField("level", ["error", null]), numberField("code", [500, null]), textField("host", ["h0", "h1"])];
  const { list } = draw(fields, { order: "oldest_first" });
  const [l0, l1] = list.querySelectorAll(".logs__line");
  click(list, l0);
  assert.equal(l0.getAttribute("aria-expanded"), "true");
  const box = list.children[1];
  assert.equal(box.className, "logs__details");
  assert.deepEqual(box.querySelectorAll("dt").map((n) => n.textContent), ["level", "code", "host"]);
  assert.deepEqual(box.querySelectorAll("dd").map((n) => n.textContent), ["error", "500", "h0"]);
  click(list, l1);
  assert.deepEqual(list.querySelectorAll("dt").map((n) => n.textContent), ["level", "code", "host", "host"]);
  click(list, l0);
  assert.equal(l0.getAttribute("aria-expanded"), "false");
  assert.equal(list.querySelectorAll(".logs__details").length, 1);
  assert.deepEqual(detailsOf(fields, buildLogs(fields, {}, "utc"), 1), [["host", "h1"]]);
  // A line with no other field says so.
  const bare = draw([timeField([T0]), textField("body", ["x"])]).list;
  click(bare, bare.querySelectorAll(".logs__line")[0]);
  assert.equal(bare.querySelector(".logs__dnone").textContent, "The line has no other field.");
});

test("keys: one tab stop; arrows, Home and End move it; Enter and Space toggle the details; details off makes plain list items", () => {
  const { list } = draw(frame(5), {});
  const lines = list.querySelectorAll(".logs__line");
  assert.deepEqual(lines.map((l) => l.tabIndex), [0, -1, -1, -1, -1]);
  const key = (line, k) => {
    let prevented = false;
    for (const fn of list.listeners.get("keydown")) fn({ key: k, target: line, preventDefault: () => { prevented = true; } });
    return prevented;
  };
  assert.ok(key(lines[0], "ArrowDown"));
  assert.equal(document.activeElement, lines[1]);
  assert.deepEqual(lines.map((l) => l.tabIndex), [-1, 0, -1, -1, -1]);
  key(lines[1], "End");
  assert.equal(document.activeElement, lines[4]);
  key(lines[4], "ArrowDown");
  assert.equal(document.activeElement, lines[4]);
  key(lines[4], "Home");
  assert.equal(document.activeElement, lines[0]);
  key(lines[0], "ArrowUp");
  assert.equal(document.activeElement, lines[0]);
  assert.ok(key(lines[0], "Enter"));
  assert.equal(lines[0].getAttribute("aria-expanded"), "true");
  assert.ok(key(lines[0], " "));
  assert.equal(lines[0].getAttribute("aria-expanded"), "false");
  assert.ok(!key(lines[0], "ArrowRight"), "the side arrows scroll the list");
  const off = draw(frame(2), { details: false }).list;
  assert.equal(off.tabIndex, 0);
  assert.equal(off.listeners.get("click"), undefined);
  assert.deepEqual(off.querySelectorAll(".logs__line").map((l) => [l.getAttribute("role"), l.getAttribute("aria-expanded")]), [["listitem", null], ["listitem", null]]);
});

// --- the time -------------------------------------------------------------------------------------------------------------------

/** The minimum of 7 runs after 2 warm-ups, in ms. */
function timeOf(fn) {
  for (let i = 0; i < 2; i++) fn();
  let best = Infinity;
  for (let i = 0; i < 7; i++) {
    const t = performance.now();
    fn();
    best = Math.min(best, performance.now() - t);
  }
  return best;
}

test("time: 10,000 lines build and draw in 250 ms or less; 2,000,000 characters with no level word scan in 150 ms or less", () => {
  const fields = frame(10000, (i) => `request ${i} took ${i % 97} ms from host-${i % 13} with status ${200 + (i % 5)}`);
  const ms = timeOf(() => draw(fields, { show_time: true }));
  assert.ok(ms <= 250, `10,000 lines: ${ms.toFixed(1)} ms`);
  const long = frame(200, () => "z".repeat(9990) + " ok", () => null);
  const scan = timeOf(() => buildLogs(long, {}, "utc"));
  assert.ok(scan <= 150, `2,000,000 characters: ${scan.toFixed(1)} ms`);
});

// Security review 3, finding F4 (9.19h, sections 2 and 4): the ANSI codes leave a body with an early stop at the cut of 10,000 characters.
// The text is the text of a removal in the whole body and then the cut, also for a code across the cut point.
test("the ANSI strip stops at the cut and gives the text of a strip of the whole body, then the cut", () => {
  const whole = (s, max = MAX_LOG_LINE_CHARS) => cutText(s.replace(/\x1b\[[0-9;]*m/g, ""), max);
  const a = (n) => "a".repeat(n);
  const vectors = [
    "", "plain", "\u001b[31mred\u001b[0m", "\u001b[", "\u001b\u001b[1m", "[31m", "x\u001b[1;2;3", "\u001b[0m".repeat(30000),
    a(10000), a(10001), a(9999) + "\u001b[0m", a(9999) + "\u001b[0mb", a(10000) + "\u001b[0mb", a(9998) + "\u001b[31mbcd",
    a(9999) + "\u001b[3", a(9999) + "\u001b[3" + "m" + "z".repeat(5), a(9995) + "\u001b[" + "1;".repeat(10) + "m" + "z".repeat(10),
    a(9999) + "\u001bb", a(10000) + "\u001b", a(9999) + "😀", a(9999) + "\ud83d\u001b[0m\ude00", a(9998) + "\u001b[0m😀x",
    ("\u001b[31mERROR\u001b[0m " + "y".repeat(48)).repeat(400),
  ];
  for (const s of vectors) assert.equal(cutText(stripAnsi(s)), whole(s), JSON.stringify(s.slice(-40)));
  for (const s of vectors.slice(0, 12)) {
    assert.equal(buildLogs([timeField([T0]), textField("body", [s])], { wrap: true }, "utc").lines[0].body, whole(s));
  }
  // Short texts and small cuts: each mix of the parts below.
  const parts = ["a", "\u001b", "[", "3", ";", "m", "\u001b[31m", "\ud83d", "\ude00"];
  let seed = 7;
  const next = (n) => ((seed = (seed * 1103515245 + 12345) % 2147483648) % n);
  for (let i = 0; i < 20000; i++) {
    let s = "";
    for (let k = next(12); k > 0; k--) s += parts[next(parts.length)];
    const max = 1 + next(8);
    assert.equal(cutText(stripAnsi(s, max), max), whole(s, max), `${JSON.stringify(s)} at ${max}`);
  }
});

test("time: one body of 32 MiB full of ANSI codes builds in 20 ms or less", () => {
  const body = ("\u001b[31mERROR\u001b[0m " + "y".repeat(48)).repeat(Math.ceil((32 * 1048576) / 64));
  const fields = [timeField([T0]), textField("body", [body])];
  let line = null;
  const ms = timeOf(() => { line = buildLogs(fields, {}, "utc").lines[0]; });
  assert.ok(ms <= 20, `32 MiB: ${ms.toFixed(1)} ms`);
  assert.equal(line.body.length, MAX_LOG_LINE_CHARS + 1);
  assert.equal(line.level, "error");
});
