// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-29 (the hostile vectors) and FE-30 (the variables in a text): the DOM that panels/text.js builds from the tree (9.19b, 16.10a).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { checkUrl } from "../../web/js/links.js";
import { MD_MAX_EXPANDED_BYTES, MD_MESSAGES, parseMarkdown } from "../../web/js/markdown.js";
import { buildDom, createText } from "../../web/js/panels/text.js";
import { RuntimeWithNames as Runtime } from "./helpers.mjs";
import { checkDom, expandLines, fakeDocument, shapeOf, textOf } from "./markdown_helpers.mjs";

const read = (name) => JSON.parse(readFileSync(new URL(`./vectors/${name}`, import.meta.url)));
const hostile = read("FE-29-markdown-hostile.json");
const vars = read("FE-30-text-vars.json");

const build = (lines, expand) => {
  const r = parseMarkdown(lines);
  assert.ok(r.ok, r.limit);
  const built = buildDom(fakeDocument, r.tree, expand);
  assert.ok(built.root, built.limit);
  return built.root;
};

test("FE-29 has 80 hostile vectors, the 40 payloads of B103 first", () => {
  assert.equal(hostile.vectors.length, 80);
  assert.ok(hostile.vectors.slice(0, 40).every((c) => c.name.startsWith("payload ")));
});

test("FE-29: each vector has only the elements, the attributes and the destinations of 9.19b", () => {
  for (const c of hostile.vectors) {
    const root = build(c.lines);
    const hrefs = checkDom(root, checkUrl);
    assert.deepEqual(hrefs, c.hrefs, c.name);
    assert.equal(textOf(root), c.text, c.name);
    assert.equal(shapeOf(root), c.shape, c.name);
    for (const h of hrefs) assert.ok(!/^(javascript|data|vbscript):/i.test(h) && !h.startsWith("//"), `${c.name}: ${h}`);
  }
});

test("FE-29: an HTML payload shows as its own characters", () => {
  for (const c of hostile.vectors.filter((x) => x.lines.length === 1 && /^<[a-zA-Z!?]/.test(x.lines[0]) && !x.lines[0].includes("`"))) assert.equal(textOf(build(c.lines)), c.lines[0], c.name);
});

test("the builder refuses a tree that holds an unsafe destination, an unknown kind and a heading level of 7", () => {
  const bad = (node) => assert.throws(() => buildDom(fakeDocument, [node]), TypeError);
  bad({ k: "a", href: "javascript:alert(1)", c: [{ k: "t", v: "x" }] });
  bad({ k: "a", href: "//evil.example", c: [] });
  bad({ k: "a", href: "http://a/b c", c: [] });
  bad({ k: "img", src: "http://a/i.png" });
  bad({ k: "h", level: 7, c: [] });
  bad({ k: "script", c: [] });
});

// --- FE-30 ------------------------------------------------------------------------------------------------------------------

function runtimeFor(c) {
  const variables = Object.entries(c.variables).map(([name, def]) => (def.kind === "text" ? { name, label: "", kind: "text", default: "" } : { name, label: "", multi: def.multi, include_all: def.include_all, all_value: def.all_value, default: [], ...def }));
  const panel = { id: 1, type: "text", title: "T", description: "", transparent: false, grid: { x: 0, y: 0, w: 12, h: 8 }, options: { mode: "markdown", content: ["x"] }, links: [] };
  const doc = { format_version: 3, uid: "d", title: "D", description: "", tags: [], cursor_sync: "off", time: { from: "now-6h", to: "now", timezone: "utc", refresh: null }, variables, sections: [{ title: null, collapsed: false, panels: [panel] }] };
  const rt = new Runtime({ uid: "d", doc, etag: "e", api: {}, widthOf: () => 500, range: { fromText: "now-6h", toText: "now" } });
  for (const [name, s] of Object.entries(c.selections)) rt.selections.set(name, s);
  return rt;
}

test("FE-30: the forms of the table of 9.19b", () => {
  const plain = vars.cases.filter((c) => c.expect !== "");
  assert.ok(plain.length >= 35);
  for (const c of plain) {
    const rt = runtimeFor(c);
    const root = build(c.lines, (t) => rt.interpolateContent(t));
    assert.equal(textOf(root), c.expect, c.name);
    if (c.hrefs) assert.deepEqual(checkDom(root, checkUrl), c.hrefs, c.name);
    else checkDom(root, checkUrl);
    rt.destroy();
  }
});

test("FE-30: a hostile value gives the same tree as a harmless value (P6)", () => {
  const hostileCases = vars.cases.filter((c) => c.expect === "");
  assert.equal(hostileCases.length, 12);
  for (const c of hostileCases) {
    const value = c.selections.t.values[0];
    const rt = runtimeFor(c);
    const mine = build(c.lines, (t) => rt.interpolateContent(t));
    const base = build(c.lines, (t) => t.replace(/\$\{?t\}?/g, "z"));
    assert.equal(shapeOf(mine), shapeOf(base), c.name);
    assert.deepEqual(checkDom(mine, checkUrl), checkDom(base, checkUrl), c.name);
    assert.ok(textOf(mine).includes(value), c.name);
    rt.destroy();
  }
});

test("FE-30: a value is never read as Markdown or HTML, in each place", () => {
  const rt = runtimeFor({ variables: { t: { kind: "text" } }, selections: { t: { all: false, values: ["<img src=x onerror=alert(1)>**x**[a](javascript:alert(1))"] } } });
  const root = build(["a $t", "", "`$t`", "", "```", "$t", "```", "", "| h |", "|---|", "| $t |"], (t) => rt.interpolateContent(t));
  checkDom(root, checkUrl);
  assert.equal(shapeOf(root), "div p p code pre code table thead tr th tbody tr td");
  rt.destroy();
});

test("FE-30: the text after the replacement has 262,144 bytes at most", () => {
  const big = "x".repeat(1024);
  const rt = runtimeFor({ variables: { t: { kind: "text" } }, selections: { t: { all: false, values: [big] } } });
  const make = (n) => parseMarkdown([Array.from({ length: n }, () => "$t").join(" ")]);
  const ok = buildDom(fakeDocument, make(255).tree, (t) => rt.interpolateContent(t));
  assert.ok(ok.root);
  const over = buildDom(fakeDocument, make(257).tree, (t) => rt.interpolateContent(t));
  assert.equal(over.limit, "expanded");
  assert.ok(MD_MESSAGES.expanded.includes("262,144"));
  assert.ok(MD_MAX_EXPANDED_BYTES === 262144);
  rt.destroy();
});

test("a variable of the content changes the key, an unrelated one does not; the parser runs once", () => {
  const rt = runtimeFor({ variables: { v: { kind: "custom", multi: true, include_all: true, all_value: null, options: [{ value: "a", text: "a" }, { value: "b", text: "b" }] }, w: { kind: "text" } }, selections: { v: { all: false, values: ["a"] }, w: { all: false, values: ["1"] } } });
  const view = createText({ options: { mode: "markdown", content: ["# $v", "", "no variable here"] } });
  const expand = (t) => rt.interpolateContent(t);
  const k1 = view.key(expand);
  rt.selections.set("w", { all: false, values: ["2"] });
  assert.equal(view.key(expand), k1);
  rt.selections.set("v", { all: false, values: ["b"] });
  assert.notEqual(view.key(expand), k1);
  assert.equal(createText({ options: { mode: "markdown", content: ["none"] } }).key(expand), "");
  const a = view.draw(fakeDocument, expand);
  const b = view.draw(fakeDocument, expand);
  assert.ok(a.root && b.root && view.parsed.tree.length === 2);
  rt.destroy();
});

test("a text panel has a record in the state ready, with no query and an empty data", () => {
  const rt = runtimeFor({ variables: {}, selections: {} });
  const rec = rt.panels.get(1);
  assert.equal(rec.state, "ready");
  assert.deepEqual(rec.panel.data, {});
  assert.equal(rec.names.size, 0);
  assert.equal(rt.order.length, 0);
  rt.destroy();
});

test("a limit gives a message and no text", () => {
  const view = createText({ options: { mode: "markdown", content: ["> > > > > > > deep"] } });
  const r = view.draw(fakeDocument, (t) => t);
  assert.equal(r.root, undefined);
  assert.equal(r.limit, "depth");
  assert.equal(r.message, MD_MESSAGES.depth);
  assert.equal(createText({ options: { mode: "markdown", content: "no array" } }).draw(fakeDocument, (t) => t).root.children.length, 0);
});

test("expandLines reads the three forms of a line", () => {
  assert.deepEqual(expandLines(["a", { line: "b", times: 2 }, { fill: ["c", 3], times: 2 }]), ["a", "b", "b", "ccc", "ccc"]);
});
