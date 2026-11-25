// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B102 to B109 (4.33a, 9.19b, 10.3, 10.6, 15.6): the text panel in Markdown.
// B102: the look (computed values in both themes, the region, no query, empty content, transparent, variables in the title and the content).
// B103: 40 hostile payloads (no element, attribute, request, dialog or CSP violation outside the rules). Chrome only: the runner drives Chrome.
// B104: variables in a text (forms, All, escapes, places, hostile values, a change with no reload, the size after the replacement).
// B105: limits and failures (the messages, the panels at the limit, 12 hostile families at 65,536 bytes, long tasks, the editor preview).
// B106: the editor (add with no source, the group "Text", the preview, the errors, Tab, the Query tab, the type dialog, revert, save and reload).
// B107: layouts and accessibility (four widths, zoom levels, viewPanel, the menu, print, keys, contrast, focus).
// B108: the format and the server (200, 422 with code and pointer, 400 for a query and a preview, bad_reference, authzen, no change for old documents).
// B109: the import of tests/fixtures/grafana/text_panels.json (15.6).
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { doc, panel, section } from "../lib/docs.mjs";
import { addItem, draftOf, enterEdit, openPanelEditor, saveDialog, waitDraft, waitNotice } from "../lib/ed.mjs";
import { panelQueries, sleep, waitPanelsReady } from "../lib/h.mjs";

const HOSTILE = JSON.parse(readFileSync(new URL("../../frontend/vectors/FE-29-markdown-hostile.json", import.meta.url), "utf8")).vectors;
const FAMILIES = await import("../../frontend/markdown_properties.mjs").then((m) => m.FAMILIES);

const tp = (id, title, lines, o = {}) => ({
  id, type: "text", title, description: o.description ?? "", transparent: o.transparent ?? false,
  grid: o.grid || { x: 0, y: 0, w: 12, h: 8 }, options: { mode: "markdown", content: lines }, links: o.links || [],
});
const customVar = (name, o = {}) => ({ kind: "custom", name, label: null, hide: "none", options: [{ text: "Alpha", value: "a" }, { text: "Beta", value: "b" }, { text: "Gamma", value: "c" }], multi: true, include_all: true, all_value: o.all ?? null, default: ["a"] });
const textVar = (name, def = "") => ({ kind: "text", name, label: null, hide: "none", value_type: "text", default: def });
const ensure = async (t, d) => {
  if ((await t.api.dashboard(d.uid)).status === 200) return;
  const r = await t.api.create("transit", d);
  t.assert(r.status === 201, `create ${d.uid}: ${r.status} ${r.text.slice(0, 400)}`);
};
const one = (id, p, o = {}) => doc(id, `Text ${id}`, [section(null, Array.isArray(p) ? p : [p])], o);

// What the page shows: the root of a text panel, as an object that the cases read.
const AUDIT = `(() => {
  const ALLOWED = new Set("div p h1 h2 h3 h4 h5 h6 em strong code pre blockquote ul ol li hr br a table thead tbody tr th td".split(" "));
  const checkUrl = (u) => !u.startsWith("//") && !u.startsWith("/\\\\") && (/^[/?#]/.test(u) || u.startsWith("http://") || u.startsWith("https://"));
  window.__audit = (root) => {
    const bad = [];
    const hrefs = [];
    for (const e of [root, ...root.querySelectorAll("*")]) {
      const tag = e.tagName.toLowerCase();
      if (!ALLOWED.has(tag)) bad.push("element " + tag);
      for (const a of e.attributes) {
        if (e === root) { if (!["class", "role", "tabindex", "aria-label", "data-testid"].includes(a.name)) bad.push("root attribute " + a.name); continue; }
        const ok = (tag === "a" && (a.name === "href" || a.name === "rel")) || (a.name === "class" && (tag === "th" || tag === "td") && /^md-[cr]$/.test(a.value));
        if (!ok) bad.push("attribute " + a.name + " on " + tag);
        if (a.name === "rel" && a.value !== "noopener noreferrer") bad.push("rel " + a.value);
        if (a.name === "href") { hrefs.push(a.value); if (!checkUrl(a.value) || /[^\\x21-\\x7e]/.test(a.value)) bad.push("href " + a.value); }
      }
      if (tag === "a" && !e.hasAttribute("rel")) bad.push("a without rel");
    }
    return { bad, hrefs, text: root.textContent, shape: [root, ...root.querySelectorAll("*")].map((e) => e.tagName.toLowerCase()).join(" ") };
  };
})();`;

async function openText(t, uid, query = "", opts = {}) {
  const p = await t.page(opts);
  await p.send("Page.addScriptToEvaluateOnNewDocument", { source: AUDIT });
  await p.goto(t.dash(uid, query));
  await p.idle();
  return p;
}
/** Wait until each text panel has drawn: its state is `ready` or `text-limit`. */
const waitSettled = async (p, ids) => {
  await p.waitFor(`${JSON.stringify(ids)}.every((id) => { const e = document.querySelector('[data-testid="panel-' + id + '"]'); return e && (e.dataset.state === "ready" || e.dataset.state === "text-limit"); })`, { what: `text panels ${ids}` });
  await p.idle();
};
const rootOf = (id) => `document.querySelector('[data-testid="text-${id}"]')`;

// --- B102 -----------------------------------------------------------------------------------------------------------------------------

const ALL_CONSTRUCTS = [
  "# H1", "## H2", "### H3", "#### H4", "##### H5", "###### H6", "",
  "A paragraph with *em*, **strong**, `code` and [a link](https://example.org/x).", "",
  "- item one", "  - nested bullet", "    - third level", "", "1. one", "2. two", "", "> quote", "", "---", "",
  "```", "code block", "```", "", "| a | b |", "|---|:-:|", "| 1 | 2 |",
];

const LOOK = `(() => {
  const root = document.querySelector('[data-testid="text-1"]');
  const tok = (prop, name) => { const s = document.createElement("span"); s.style[prop] = "var(--" + name + ")"; document.body.append(s); const v = getComputedStyle(s)[prop]; s.remove(); return v; };
  const cs = (sel) => getComputedStyle(root.querySelector(sel));
  const px = (v) => Math.round(parseFloat(v) * 100) / 100;
  const out = { errors: [] };
  const eq = (name, got, want) => { if (JSON.stringify(got) !== JSON.stringify(want)) out.errors.push(name + ": " + JSON.stringify(got) + " != " + JSON.stringify(want)); };
  const r = getComputedStyle(root);
  eq("root font", [r.fontSize, r.lineHeight, r.fontWeight, r.color, r.overflowY, r.overflowX], ["14px", "22px", "400", tok("color", "text"), "auto", "auto"]);
  eq("root family", r.fontFamily.includes("Inter"), true);
  const heads = { h1: [28, 32, 12.6, 400], h2: [24, 28, 10.8, 400], h3: [22, 24, 9.9, 400], h4: [18, 22, 8.1, 400], h5: [16, 22, 7.2, 400], h6: [14, 22, 6.3, 500] };
  for (const [h, [fs, lh, mb, fw]] of Object.entries(heads)) { const c = cs(h); eq(h, [px(c.fontSize), px(c.lineHeight), px(c.marginBottom), px(c.marginTop), Number(c.fontWeight)], [fs, lh, mb, 0, fw]); }
  for (const sel of ["p", "ul", "ol", "blockquote", "pre", "table"]) { const c = cs(sel); eq(sel + " margin", [px(c.marginBottom), px(c.marginTop)], [16, 0]); }
  eq("strong", Number(cs("strong").fontWeight), 500);
  eq("em", cs("em").fontStyle, "italic");
  eq("ul", [px(cs("ul").paddingLeft), cs("ul").listStyleType, cs("ul ul").listStyleType, cs("ul ul ul").listStyleType, cs("ol").listStyleType], [24, "disc", "circle", "square", "decimal"]);
  eq("nested ul margin", [px(cs("li > ul").marginBottom), px(cs("li > ul").marginTop)], [0, 0]);
  const bq = cs("blockquote");
  eq("blockquote", [px(bq.paddingLeft), px(bq.borderLeftWidth), bq.borderLeftColor], [16, 5, tok("color", "muted")]);
  const hr = cs("hr");
  eq("hr", [px(hr.marginTop), px(hr.marginBottom), px(hr.borderTopWidth), hr.borderTopColor], [16, 16, 1, tok("color", "grid-line")]);
  const code = root.querySelector("p code") && getComputedStyle(root.querySelector("p code"));
  eq("code", [code.fontSize, px(code.paddingTop), px(code.paddingLeft), px(code.marginLeft), px(code.borderTopWidth), code.borderTopColor, px(code.borderTopLeftRadius), code.backgroundColor, code.fontFamily.includes("mono") || code.fontFamily.includes("Mono") || code.fontFamily.includes("monospace")], ["12px", 2, 5, 2, 1, tok("color", "grid-line"), 6, tok("backgroundColor", "panel"), true]);
  const pre = cs("pre");
  eq("pre", [pre.fontSize, px(pre.lineHeight), px(pre.padding.split(" ")[0]), px(pre.borderTopWidth), px(pre.borderTopLeftRadius), pre.backgroundColor, pre.whiteSpace, pre.overflowY], ["12px", 18.86, 10, 1, 6, tok("backgroundColor", "panel"), "pre-wrap", "auto"]);
  const pc = getComputedStyle(root.querySelector("pre code"));
  eq("pre code", [px(pc.borderTopWidth), px(pc.paddingLeft), pc.backgroundColor], [0, 0, "rgba(0, 0, 0, 0)"]);
  const td = cs("td");
  eq("td", [px(td.borderTopWidth), td.borderTopColor, px(td.paddingTop), px(td.paddingLeft)], [1, tok("color", "grid-line"), 4, 8]);
  const th = cs("th");
  eq("th", [Number(th.fontWeight), th.backgroundColor], [500, tok("backgroundColor", "surface-raised")]);
  eq("align", [cs("td.md-c").textAlign, cs("table").borderCollapse], ["center", "collapse"]);
  const a = cs("a");
  eq("link", [a.color, a.textDecorationLine], [tok("color", "link"), "underline"]);
  const body = getComputedStyle(root.parentElement);
  eq("body padding", [px(body.paddingTop), px(body.paddingLeft)], [8, 8]);
  return out;
})()`;

async function lookOnce(t, p) {
  const r = await p.eval(LOOK);
  t.eq(r.errors, [], "the computed values equal the table of the look");
}

// --- the cases ------------------------------------------------------------------------------------------------------------------------

export const cases = [
  {
    id: "B102",
    name: "the look of a text panel: the table of 9.19b in both themes, a region with the name, no query, an empty body, transparent, variables in the title and the content",
    async run(t) {
      await ensure(t, one("txt102", [
        tp(1, "Constructs", ALL_CONSTRUCTS, { grid: { x: 0, y: 0, w: 24, h: 40 } }),
        tp(2, "Empty", [], { grid: { x: 0, y: 40, w: 8, h: 6 } }),
        tp(3, "Transparent", ["text"], { transparent: true, grid: { x: 8, y: 40, w: 8, h: 6 } }),
        tp(4, "Env $env", ["Env: $env"], { grid: { x: 16, y: 40, w: 8, h: 6 } }),
      ], { variables: [{ ...customVar("env"), multi: false, include_all: false, options: [{ text: "Production", value: "prod" }], default: ["prod"] }] }));
      for (const scheme of ["dark", "light"]) {
        const p = await t.page({ width: 1280, height: 900 });
        await p.emulateColorScheme(scheme);
        await p.send("Page.addScriptToEvaluateOnNewDocument", { source: AUDIT });
        await p.goto(t.dash("txt102"));
        await p.idle();
        await waitPanelsReady(p, [1, 2, 3, 4]);
        t.eq(await p.eval("document.documentElement.dataset.theme"), scheme, "theme");
        await lookOnce(t, p);
        t.eq(panelQueries(p), [], "a text panel sends no query");
        t.eq(await p.eval(`['1','2','3','4'].map((i) => document.querySelector('[data-testid="panel-' + i + '"]').dataset.state)`), ["ready", "ready", "ready", "ready"], "state ready");
        t.eq(await p.eval(`['1','2','3','4'].map((i) => document.querySelector('[data-testid="panel-' + i + '"]').dataset.type)`), ["text", "text", "text", "text"], "data-type");
        t.eq(await p.eval(`(() => { const r = ${rootOf(1)}; return [r.getAttribute("role"), r.getAttribute("aria-label"), r.getAttribute("tabindex")]; })()`), ["region", "Constructs", "0"], "the scroll box is a region with the name");
        t.eq(await p.eval(`${rootOf(2)}.children.length`), 0, "an empty content shows an empty body");
        t.eq(await p.eval(`getComputedStyle(document.querySelector('[data-testid="panel-3"]')).backgroundColor`), "rgba(0, 0, 0, 0)", "a transparent panel has no fill");
        t.eq(await p.eval(`document.querySelector('[data-testid="panel-4"] .panel__title').textContent`), "Env Production", "the title has the option text");
        t.eq(await p.eval(`${rootOf(4)}.textContent`), "Env: prod", "the content has the value");
        t.eq(await p.eval(`window.__audit(${rootOf(1)}).bad`), [], "elements and attributes");
        t.eq(await p.eval(`window.__audit(${rootOf(1)}).hrefs`), ["https://example.org/x"], "the link");
      }
    },
  },
  {
    id: "B103",
    name: "40 hostile payloads show as text: only the elements and attributes of 9.19b, links that pass 4.23 with rel, no dialog, no request, no CSP violation (T53, T56)",
    async run(t) {
      // The recording server: a request to it is a beacon. The payload of the image points to it.
      const seen = [];
      const rec = createServer((req, res) => {
        seen.push(req.url);
        res.writeHead(200, { "Content-Type": "image/png" });
        res.end();
      });
      await new Promise((r) => rec.listen(0, "127.0.0.1", r));
      const base = `http://127.0.0.1:${rec.address().port}`;
      try {
        const vectors = HOSTILE.slice(0, 40).map((v) => ({
          ...v,
          lines: v.lines.map((l) => l.replace("http://evil.example/beacon.png", `${base}/beacon.png`)),
          hrefs: v.hrefs.map((h) => h.replace("http://evil.example/beacon.png", `${base}/beacon.png`)),
          text: v.text,
        }));
        const panels = vectors.map((v, i) => tp(i + 1, `Payload ${i + 1}`, v.lines, { grid: { x: (i % 2) * 12, y: Math.floor(i / 2) * 4, w: 12, h: 4 } }));
        await ensure(t, one("txt103", panels));
        const p = await openText(t, "txt103");
        await waitPanelsReady(p, vectors.map((_, i) => i + 1));
        for (const [i, v] of vectors.entries()) {
          const r = await p.eval(`window.__audit(${rootOf(i + 1)})`);
          t.eq(r.bad, [], `payload ${i + 1}: elements, attributes and destinations`);
          t.eq(r.hrefs, v.hrefs, `payload ${i + 1}: destinations`);
          t.eq(r.text, v.text, `payload ${i + 1}: the payload shows as text`);
        }
        await sleep(800);
        t.eq(p.dialogs.length, 0, "no dialog");
        t.eq(seen, [], "the recording server saw no request");
        t.eq(p.requests.filter((r) => !r.url.startsWith(t.server.origin) && !r.url.startsWith("data:") && !r.url.startsWith("blob:")).map((r) => r.url), [], "no request to another host");
        t.eq(await p.eval(`document.querySelectorAll('.md img, .md iframe, .md script, .md style, .md svg, .md form, .md input, .md [style], .md [onclick], .md [onerror]').length`), 0, "no element or attribute of the attacks");
      } finally {
        await new Promise((r) => rec.close(r));
      }
    },
  },
  {
    id: "B104",
    name: "variables in a text: the forms, All with and without all_value, escapes, the places, hostile values, a change with no reload, the size after the replacement (9.19b)",
    async run(t) {
      const content = [
        "- $v", "- ${v}", "- ${v:text}", "- ${v:csv}", "- ${v:pipe}", "- ${v:glob}", "- \\$v", "- $nope", "- $__from", "- [[v]]", "- `$v`",
        "- w $w", "- ${w:csv}", "- ${w:text}", "",
        "```", "$v", "```", "", "| h |", "|---|", "| ${v:csv} |", "", "[link $v](/x?q=$v)",
      ];
      await ensure(t, one("txt104", tp(1, "Vars", content, { grid: { x: 0, y: 0, w: 24, h: 30 } }), { variables: [customVar("v"), customVar("w", { all: "ALL" }), textVar("t", "z")] }));
      const read = async (p) => p.eval(`(() => ({ items: [...${rootOf(1)}.querySelectorAll("li")].map((e) => e.textContent), pre: ${rootOf(1)}.querySelector("pre").textContent, td: ${rootOf(1)}.querySelector("td").textContent, link: [${rootOf(1)}.querySelector("a").textContent, ${rootOf(1)}.querySelector("a").getAttribute("href")], audit: window.__audit(${rootOf(1)}) }))()`);
      let p = await openText(t, "txt104", "var-v=a&var-v=b&var-w=a");
      await waitPanelsReady(p, [1]);
      let r = await read(p);
      t.eq(r.items, ["a, b", "a, b", "Alpha + Beta", "a,b", "a|b", "{a,b}", "$v", "$nope", "$__from", "[[v]]", "a, b", "w a", "a", "Alpha"], "several values");
      t.eq([r.pre, r.td, r.link], ["a, b", "a,b", ["link a, b", "/x?q=$v"]], "code block, cell and link: the destination stays");
      t.eq(r.audit.bad, [], "elements");
      p = await openText(t, "txt104", "var-v=%24__all&var-w=%24__all");
      await waitPanelsReady(p, [1]);
      r = await read(p);
      t.eq([r.items[0], r.items[3], r.items[2], r.items[11], r.items[12], r.items[13]], ["a, b, c", "a,b,c", "All", "w ALL", "ALL", "All"], "All with and without all_value");
      // Hostile values: the shape is the shape of a harmless value.
      const base = await (async () => { const q = await openText(t, "txt104", "var-v=a&var-t=z"); await waitPanelsReady(q, [1]); return (await read(q)).audit.shape; })();
      for (const value of ["<b>x</b>", "*x*", "[x](javascript:alert(1))", "`", "|", "$", "x".repeat(1024), "<img src=x onerror=alert(1)>"]) {
        const q = await openText(t, "txt104", `var-v=a&var-t=${encodeURIComponent(value)}`);
        await waitPanelsReady(q, [1]);
        const a = await read(q);
        t.eq(a.audit.shape, base, `a value keeps the shape: ${value.slice(0, 20)}`);
        t.eq(a.audit.bad, [], "elements");
        t.eq(q.dialogs.length, 0, "no dialog");
      }
      // A text variable in the content: the change of the control changes the text with no reload.
      await ensure(t, one("txt104b", tp(1, "T", ["Value: $t", "", "*em* $t"]), { variables: [textVar("t", "one")] }));
      p = await openText(t, "txt104b");
      await waitPanelsReady(p, [1]);
      await p.eval(`window.__marker = 1; window.__shape = window.__audit(${rootOf(1)}).shape`);
      const input = `${p.tid("variable-t")} input`;
      await p.fill(input, "two <b>x</b>");
      await p.press("Enter");
      await p.waitFor(`${rootOf(1)}.textContent.includes("two <b>x</b>")`, { what: "the new value" });
      t.eq(await p.eval(`[window.__marker, window.__audit(${rootOf(1)}).shape === window.__shape]`), [1, true], "no reload, and the same elements");
      t.eq(panelQueries(p), [], "no query");
      // A selection that gives more than 262,144 bytes shows the message.
      await ensure(t, one("txt104c", tp(1, "Big", [Array.from({ length: 300 }, () => "$t").join(" ")]), { variables: [textVar("t", "x")] }));
      p = await openText(t, "txt104c", `var-t=${"x".repeat(1024)}`);
      await p.waitFor(`document.querySelector('[data-testid="text-1-note"]')`, { what: "the note" });
      t.eq(await p.eval(`[document.querySelector('[data-testid="panel-1"]').dataset.state, document.querySelector('[data-testid="text-1-note"]').textContent, document.querySelector('[data-testid="text-1-note"]').getAttribute("role"), !!${rootOf(1)}]`), ["text-limit", "The text with the values of the variables has more than 262,144 bytes. Select fewer values.", "note", false], "the message of the expanded limit");
    },
  },
  {
    id: "B105",
    name: "limits and failures: one panel for each limit shows its message and no text, the panels at the limit show their text, 12 hostile families at 65,536 bytes cause no task above 200 ms, the preview shows the same messages (9.19b)",
    async run(t) {
      const rep = (n, f) => Array.from({ length: n }, f);
      const big = (last) => [...rep(7, () => "a".repeat(8191)), "a".repeat(last)];
      const cases = [
        [3, "depth 7", ["> > > > > > > a"], "depth"],
        [4, "inline depth 7", ["*a **b *c **d *e **f *g* f** e* d** c* b** a*"], "inline"],
        [5, "4,001 nodes", [...rep(1999, () => "# a"), "# *a*"], "nodes"],
        [6, "21 columns", [`|${"a|".repeat(21)}`, `|${"-|".repeat(21)}`], "columns"],
        [7, "2,000 lines", rep(2000, () => "a"), null],
        [8, "depth 6", ["> > > > > > a"], null],
        [9, "20 columns", [`|${"a|".repeat(20)}`, `|${"-|".repeat(20)}`], null],
        [10, "65,536 bytes", big(8192), null],
      ];
      const MESSAGES = {
        size: "This text is too large: 2,000 lines and 65,536 bytes at most.",
        depth: "This text has lists and quotes more than 6 levels deep. Reduce the depth.",
        inline: "This text has emphasis and links more than 6 levels deep. Reduce the depth.",
        nodes: "This text has more than 4,000 elements. Shorten the text.",
        columns: "A table of this text has more than 20 columns.",
      };
      await ensure(t, one("txt105", cases.map(([id, title, lines], i) => tp(id, title, lines, { grid: { x: (i % 4) * 6, y: Math.floor(i / 4) * 4, w: 6, h: 4 } }))));
      let p = await t.page({ width: 1280, height: 900 });
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: AUDIT + `window.__long = []; new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__long.push(e.duration); }).observe({ entryTypes: ["longtask"] });` });
      await p.goto(t.dash("txt105"));
      await p.idle();
      await waitSettled(p, cases.map((c) => c[0]));
      for (const [id, title, , limit] of cases) {
        const state = await p.eval(`document.querySelector('[data-testid="panel-${id}"]').dataset.state`);
        if (limit) {
          t.eq(state, "text-limit", `${title}: the state`);
          t.eq(await p.eval(`[document.querySelector('[data-testid="text-${id}-note"]').textContent, document.querySelector('[data-testid="text-${id}-note"]').getAttribute("role"), !!${rootOf(id)}]`), [MESSAGES[limit], "note", false], `${title}: the message and no text`);
        } else {
          t.eq([state, await p.eval(`!!${rootOf(id)}`)], ["ready", true], `${title}: the text shows`);
        }
      }
      // 12 hostile families at 65,536 bytes: a valid document (lines of 8,000 characters at most), a message or a text, no long task.
      const chunk = (s) => s.split("\n").flatMap((l) => (l.length > 8000 ? l.match(/[^]{1,8000}/g) : [l]));
      const fam = Object.entries(FAMILIES).map(([name, make], i) => {
        let lines = chunk(make(65536 - 300));
        while (lines.join("\n").length > 65536) lines = lines.slice(0, -1);
        return tp(i + 1, name, lines.slice(0, 2000), { grid: { x: (i % 4) * 6, y: Math.floor(i / 4) * 4, w: 6, h: 4 } });
      });
      await ensure(t, one("txt105f", fam));
      p = await t.page({ width: 1280, height: 900 });
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: AUDIT + `window.__long = []; new PerformanceObserver((l) => { for (const e of l.getEntries()) window.__long.push(e.duration); }).observe({ entryTypes: ["longtask"] });` });
      await p.goto(t.dash("txt105f"));
      await p.idle();
      await waitSettled(p, fam.map((f) => f.id));
      const states = await p.eval(`[...document.querySelectorAll('[data-type="text"]')].map((e) => e.dataset.state)`);
      t.assert(states.length === 12 && states.every((s) => s === "ready" || s === "text-limit"), "each family shows a text or a message: " + JSON.stringify(states));
      const longest = Math.max(0, ...(await p.eval("window.__long")));
      t.assert(longest <= 200, `a task of ${longest} ms`);
      t.note(`12 families: ${states.filter((s) => s === "text-limit").length} messages, longest task ${Math.round(longest)} ms`);
      // The preview of the editor shows the same message.
      await enterEdit(p);
      await openPanelEditor(p, 3);
      await p.click(p.tid("tab-panel"));
      // The format refuses 2,001 lines and 65,537 bytes, so the document never has them: the draft of the editor does (it is not checked until a save).
      const type = async (value) => {
        await p.eval(`(() => { const a = document.querySelector('[data-testid="text-content"]'); a.value = ${JSON.stringify(value)}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
        await sleep(450);
        return p.eval(`[document.querySelector('[data-testid="text-error"]').textContent, (document.querySelector('.peditor [data-testid="text-1-note"], .peditor [data-testid="text-3-note"]') || {}).textContent || null, document.querySelector('.peditor [data-state]').dataset.state]`);
      };
      t.eq(await type(rep(2001, () => "a").join("\n")), [MESSAGES.size, MESSAGES.size, "text-limit"], "2,001 lines: the error and the preview");
      t.eq(await type([...rep(8, () => "a".repeat(8191)), "a".repeat(10)].join("\n")), [MESSAGES.size, MESSAGES.size, "text-limit"], "65,537 bytes or more: the same message");
      t.eq(await type("> > > > > > > a"), [MESSAGES.depth, MESSAGES.depth, "text-limit"], "depth 7");
      t.eq((await type("ok"))[2], "ready", "a text with no fault shows again");
    },
  },
  {
    id: "B106",
    name: "the editor: Add text panel with no source, the group Text, the preview after 300 ms, the errors, Tab, the Query tab, the groups, the dialog of a type change both ways, Revert, a save and a reload (10.2, 10.3, 10.6)",
    async run(t) {
      const TA = '[data-testid="text-content"]';
      const edit = (p, expr) => p.eval(`(() => { const a = document.querySelector('[data-testid="text-content"]'); return ${expr}; })()`);
      const setValue = async (p, value) => {
        await edit(p, `(a.value = ${JSON.stringify(value)}, a.dispatchEvent(new Event("input", { bubbles: true })))`);
      };
      const stub = { status: 200, headers: { "Content-Type": "application/json" }, body: '{"items":[]}' };
      // 1. No source: "Add text panel" works, "Add panel" does not.
      await ensure(t, one("txt106a", [tp(1, "T", ["x"])]));
      let p = await t.page({ width: 1400, height: 900 });
      p.fulfillWhen((r) => r.method === "GET" && r.url.endsWith("/api/sources"), stub);
      await p.goto(t.dash("txt106a"));
      await p.idle();
      await waitPanelsReady(p, [1]);
      await enterEdit(p);
      await addItem(p, "add-panel");
      await waitNotice(p, /No source is available to you\./);
      await addItem(p, "add-text");
      await p.waitSel(p.tid("panel-editor"));
      const d0 = await waitDraft(p, "txt106a", (d) => d.document.sections[0].panels.length === 2);
      t.eq(d0.document.sections[0].panels[1], { id: 2, type: "text", title: "New text", description: "", transparent: false, grid: { x: 0, y: 8, w: 12, h: 8 }, options: { mode: "markdown", content: ["# Title", "", "Write text in Markdown."] }, links: [] }, "the new text panel of 10.6");

      // 2. The editor with the real sources.
      await ensure(t, one("txt106", [tp(1, "Notes", ["# Hello", "", "Text with $env."]), panel(2, "stat", "Stat", ["SELECT 1 AS one"], { grid: { x: 12, y: 0, w: 12, h: 8 } })], { variables: [{ ...customVar("env"), multi: false, include_all: false, default: ["a"] }] }));
      p = await t.page({ width: 1400, height: 900 });
      await p.goto(t.dash("txt106"));
      await p.idle();
      await waitPanelsReady(p, [1, 2]);
      t.allow(p, /Failed to load resource/);
      await enterEdit(p);
      await openPanelEditor(p, 1);
      t.eq(await p.eval(`[...document.querySelectorAll('.ogroup__title')].map((e) => e.textContent)`), ["Panel options", "Visualisation", "Text", "Panel links", "Position"], "the groups of a text panel");
      t.eq(await p.eval(`[...document.querySelectorAll('.typelist__item')].map((e) => e.dataset.type)`), ["table", "timeseries", "stat", "barchart", "piechart", "xychart", "bargauge", "gauge", "state_timeline", "status_history", "heatmap", "histogram", "logs", "geomap", "text"], "15 types (9.19c; heatmap and histogram since F31; gauge since wave H, P12; logs since F33; geomap since F37)");
      t.eq(await edit(p, `[a.tagName, a.rows, a.getAttribute("spellcheck"), a.getAttribute("autocomplete"), a.getAttribute("autocapitalize"), a.getAttribute("wrap"), getComputedStyle(a).fontFamily.toLowerCase().includes("mono"), a.value]`), ["TEXTAREA", 12, "true", "off", "off", "soft", true, "# Hello\n\nText with $env."], "the field");
      t.eq(await p.text(p.tid("text-count")), "24 of 65,536 bytes, 3 of 2,000 lines", "the count");
      t.eq(await p.text(p.tid("text-help")), "Markdown. HTML shows as text. Images do not show. A link starts with /, ?, #, http:// or https://. $name shows the values of a variable.", "the help");
      // The preview changes 300 ms after the last key, with the draft.
      await p.fill(TA, "# Draft\n\n- one\n- two");
      t.assert(!(await p.text(".peditor .md")).includes("Draft"), "no change before the 300 ms");
      await p.waitFor(`document.querySelector(".peditor .md h1")?.textContent === "Draft"`, { what: "the preview" });
      t.eq(await p.eval(`[...document.querySelectorAll(".peditor .md li")].map((e) => e.textContent)`), ["one", "two"], "the preview is the Markdown preview");
      const d1 = await waitDraft(p, "txt106", (d) => d.document.sections[0].panels[0].options.content.length === 4);
      t.eq(d1.document.sections[0].panels[0].options.content, ["# Draft", "", "- one", "- two"], "the lines of the content");
      await p.fill(TA, "a\nb");
      t.eq((await waitDraft(p, "txt106", (d) => d.document.sections[0].panels[0].options.content.length === 2)).document.sections[0].panels[0].options.content, ["a", "b"], "a line end splits the text (the browser changes each CR LF of a paste to LF; the unit test of 10.3 covers the function)");
      // The errors.
      const err = async (value) => {
        await setValue(p, value);
        return p.eval(`(() => { const e = document.querySelector('[data-testid="text-error"]'); return [e.hidden ? "" : e.textContent, document.querySelector('[data-testid="text-content"]').getAttribute("aria-describedby").split(" ").includes(e.id)]; })()`);
      };
      t.eq(await err("ok\nbad\u0001"), ["Line 2 has a control character.", true], "a control character");
      t.eq(await err("x".repeat(8193)), ["Line 1 has more than 8,192 bytes.", true], "a long line");
      t.eq((await err(Array.from({ length: 2001 }, () => "a").join("\n")))[0], "This text is too large: 2,000 lines and 65,536 bytes at most.", "too many lines");
      t.eq((await err(Array.from({ length: 9 }, () => "a".repeat(8000)).join("\n")))[0], "This text is too large: 2,000 lines and 65,536 bytes at most.", "too many bytes");
      t.eq((await err("> > > > > > > x"))[0], "This text has lists and quotes more than 6 levels deep. Reduce the depth.", "a limit of 9.19b");
      t.eq((await err("fine"))[0], "", "no error");
      // The key Tab moves the focus out of the field.
      await p.focus(TA);
      await p.press("Tab");
      t.assert(await p.eval(`document.activeElement.tagName !== "TEXTAREA"`), "Tab leaves the field");
      // The tab "Query" has the note and no field.
      await p.click(p.tid("tab-query"));
      t.eq(await p.eval(`[document.querySelector('[data-testid="text-no-query"]').textContent, !!document.querySelector('[data-testid="run-button"]'), !!document.querySelector('.qarea input, .qarea select, .qarea textarea')]`), ["A text panel has no query.", false, false], "the tab Query");
      await p.click(p.tid("tab-panel"));
      t.assert((await p.text(".psummary")).includes("Lines"), "the tab Panel shows the lines");
      // The dialog of a change of the type, both ways. "Cancel" changes nothing.
      await p.click(p.tid("change-type")); // the group "Visualisation" is closed at the first draw (10.3h, rule 6)
      await p.click('.typelist__item[data-type="table"]');
      await p.waitSel(p.tid("type-change-dialog"));
      t.assert((await p.text(p.tid("type-change-dialog"))).includes("The text of this panel leaves the draft."), "the text of the dialog");
      await p.clickText(`${p.tid("type-change-dialog")} button`, "Cancel");
      await p.waitGone(p.tid("type-change-dialog"));
      await sleep(600);
      t.eq((await draftOf(p, "txt106")).document.sections[0].panels[0].type, "text", "Cancel changes nothing");
      await p.click(p.tid("change-type")); // the group "Visualisation" is closed at the first draw (10.3h, rule 6)
      await p.click('.typelist__item[data-type="table"]');
      await p.waitSel(p.tid("type-change-dialog"));
      await p.clickText(`${p.tid("type-change-dialog")} button`, "Change");
      const d2 = await waitDraft(p, "txt106", (d) => d.document.sections[0].panels[0].type === "table");
      const changed = d2.document.sections[0].panels[0];
      t.eq([changed.data.query.sql, changed.transforms, "options" in changed, "repeat" in changed], [["SELECT 1"], [], true, false], "the panel gets the query of a new panel");
      await p.click(p.tid("change-type")); // the group "Visualisation" is closed at the first draw (10.3h, rule 6)
      await p.click('.typelist__item[data-type="text"]');
      await p.waitSel(p.tid("type-change-dialog"));
      t.assert((await p.text(p.tid("type-change-dialog"))).includes("The query, the transformations and the field options of this panel leave the draft."), "the text of the dialog to text");
      await p.clickText(`${p.tid("type-change-dialog")} button`, "Change");
      const d3 = await waitDraft(p, "txt106", (d) => d.document.sections[0].panels[0].type === "text");
      t.eq(["data", "transforms", "fields"].map((k) => k in d3.document.sections[0].panels[0]), [false, false, false], "a text panel has no data, transforms or fields");
      // The JSON tab shows each error with its pointer.
      await p.click(p.tid("tab-json"));
      const area = p.tid("panel-json");
      const json = await p.eval(`document.querySelector('[data-testid="panel-json"]').value`);
      await p.eval(`(() => { const a = document.querySelector('[data-testid="panel-json"]'); a.value = ${JSON.stringify(json.replace('"markdown"', '"html"'))}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
      await p.waitFor(`/\\/options\\/mode/.test((document.querySelector('[data-testid="panel-json-errors"]') || {}).textContent || "")`, { what: "the error of the mode", timeout: 8000 });
      t.assert((await p.text(p.tid("panel-json-errors"))).includes("bad_format"), "the code of the error");
      await p.eval(`(() => { const a = document.querySelector('[data-testid="panel-json"]'); a.value = ${JSON.stringify(json)}; a.dispatchEvent(new Event("input", { bubbles: true })); })()`);
      // Revert restores the saved panel.
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("panel-editor"));
      await p.click(p.tid("discard-button"));
      await p.clickText("dialog button", "Discard");
      await p.waitFor(`document.querySelector('[data-testid="text-1"] h1')?.textContent === "Hello"`, { what: "the saved panel" });
      // A save and a reload give the same document.
      await openPanelEditor(p, 1);
      await p.click(p.tid("tab-panel"));
      await p.fill(TA, "# Saved\n\nBody `x` and [l](/d/y)");
      await sleep(450);
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("panel-editor"));
      await saveDialog(p, "text panel");
      await p.waitFor(`document.querySelector('[data-testid="text-1"] h1')?.textContent === "Saved" && !document.querySelector('[data-testid="save-button"]:not([disabled])')`, { what: "the saved page", timeout: 15000 }).catch(() => {});
      const stored = await t.api.dashboard("txt106");
      t.eq(stored.json.document.sections[0].panels[0].options, { mode: "markdown", content: ["# Saved", "", "Body `x` and [l](/d/y)"] }, "the stored text");
      t.eq(Object.keys(stored.json.document.sections[0].panels[0]), ["id", "type", "title", "description", "transparent", "grid", "options", "links"], "the canonical order of 4.38");
      await p.reload();
      await p.idle();
      await waitPanelsReady(p, [1]);
      t.eq(await p.text('[data-testid="text-1"] h1'), "Saved", "after a reload");
    },
  },
  {
    id: "B107",
    name: "layouts and accessibility: four widths and the zoom levels (no page scroll, long words break, a wide table and a long code block scroll), viewPanel, the menu, print, the keys, contrast and focus in both themes (9.19b, 9.26)",
    async run(t) {
      const wide = `| ${Array.from({ length: 14 }, (_, i) => `column${i}`).join(" | ")} |`;
      await ensure(t, one("txt107", [
        tp(1, "Layout", [
          "# Heading with a very long word " + "x".repeat(120), "", "A paragraph with a [link](/d/other) and " + "y".repeat(160) + ".", "",
          wide, `|${"---|".repeat(14)}`, `| ${Array.from({ length: 14 }, (_, i) => `value-number-${i}`).join(" | ")} |`, "",
          "```", "z".repeat(300), "```", "", "- item with [another](https://example.org/a)",
        ], { grid: { x: 0, y: 0, w: 12, h: 12 } }),
        tp(2, "Small", ["short"], { grid: { x: 12, y: 0, w: 12, h: 4 } }),
      ]));
      const probe = `(() => {
        const root = document.querySelector('[data-testid="text-1"]');
        const rb = root.getBoundingClientRect();
        const out = { page: Math.max(document.documentElement.scrollWidth - document.documentElement.clientWidth, document.getElementById("app-main").scrollWidth - document.getElementById("app-main").clientWidth), outside: [] };
        for (const e of root.querySelectorAll("p, h1, li, a")) { const r = e.getBoundingClientRect(); if (r.right > rb.right + 1.5) out.outside.push(e.tagName + " " + Math.round(r.right - rb.right)); }
        out.scrolls = root.scrollWidth > root.clientWidth;
        out.pre = (() => { const p = root.querySelector("pre"); return p.scrollWidth > p.clientWidth || p.getBoundingClientRect().right <= rb.right + 1.5; })();
        out.floor = document.querySelector('[data-testid="panel-2"]').getBoundingClientRect().height;
        return out;
      })()`;
      const found = [];
      const sizes = [[1920, 1], [1280, 1], [768, 1], [390, 1], [Math.round(1280 / 0.75), 0.75], [Math.round(1280 / 0.9), 0.9], [Math.round(1280 / 1.1), 1.1], [Math.round(1280 / 1.25), 1.25]];
      for (const scheme of ["dark", "light"]) {
        for (const [w, dsf] of sizes) {
          const p = await t.page({ width: w, height: 900 });
          await p.send("Emulation.setDeviceMetricsOverride", { width: w, height: Math.round(900 / dsf), deviceScaleFactor: dsf, mobile: false });
          await p.emulateColorScheme(scheme);
          await p.send("Page.addScriptToEvaluateOnNewDocument", { source: AUDIT });
          await p.goto(t.dash("txt107"));
          await p.idle();
          await waitPanelsReady(p, [1, 2]);
          const r = await p.eval(probe);
          if (r.page > 0) found.push(`${w}@${dsf} ${scheme}: page scroll ${r.page}`);
          if (r.outside.length) found.push(`${w}@${dsf} ${scheme}: outside ${r.outside.join(",")}`);
          if (!r.pre) found.push(`${w}@${dsf} ${scheme}: the code block neither fits nor scrolls`);
          if (w <= 768 && r.floor < 159.5) found.push(`${w}@${dsf}: a panel of 4 rows is ${r.floor} px high, under the floor of 160`);
          await p.close();
        }
      }
      t.eq(found, [], "layouts");
      // The page: viewPanel, the menu, print, the keys, contrast and focus.
      for (const scheme of ["dark", "light"]) {
        const p = await t.page({ width: 1280, height: 900 });
        await p.emulateColorScheme(scheme);
        await p.send("Page.addScriptToEvaluateOnNewDocument", { source: AUDIT });
        await p.goto(t.dash("txt107"));
        await p.idle();
        await waitPanelsReady(p, [1, 2]);
        await p.click(p.tid("panel-1-menu"));
        const items = await p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((e) => e.textContent)`);
        t.assert(items.includes("View") && !items.includes("View data") && !items.includes("Inspect") && !items.includes("Download CSV"), "the menu has View and no data item: " + JSON.stringify(items));
        await p.press("Escape");
        // The keys reach the scroll box and each link.
        await p.eval("document.body.focus(); document.activeElement && document.activeElement.blur()");
        const seen = [];
        for (let i = 0; i < 30; i++) {
          await p.press("Tab");
          seen.push(await p.eval(`(() => { const a = document.activeElement; return a === document.querySelector('[data-testid="text-1"]') ? "root" : a.tagName === "A" && a.closest('[data-testid="text-1"]') ? "a:" + a.getAttribute("href") : null; })()`));
        }
        t.eq([...new Set(seen.filter(Boolean))], ["root", "a:/d/other", "a:https://example.org/a"], "the Tab order: the scroll box, then each link");
        // Contrast and focus.
        const c = await p.eval(`(() => {
          const lum = (s) => { const m = s.match(/[\\d.]+/g).map(Number); const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(m[0]) + 0.7152 * f(m[1]) + 0.0722 * f(m[2]); };
          const ratio = (a, b) => { const x = lum(a); const y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
          const root = document.querySelector('[data-testid="text-1"]');
          const panelBg = getComputedStyle(document.querySelector('[data-testid="panel-1"]')).backgroundColor;
          const bg = (e) => { for (let n = e; n; n = n.parentElement) { const v = getComputedStyle(n).backgroundColor; if (v !== "rgba(0, 0, 0, 0)" && v !== "transparent") return v; } return panelBg; };
          const col = (sel) => getComputedStyle(root.querySelector(sel)).color;
          root.focus();
          const ring = getComputedStyle(root);
          return {
            link: ratio(col("a"), panelBg), text: ratio(col("p"), panelBg), code: ratio(col("pre code"), bg(root.querySelector("pre"))), cell: ratio(col("td"), panelBg),
            lines: ratio(getComputedStyle(root.querySelector("td")).borderTopColor, panelBg), ring: ratio(ring.outlineColor, panelBg), ringWidth: ring.outlineWidth,
          };
        })()`);
        t.assert(c.link >= 4.5 && c.text >= 4.5 && c.code >= 4.5 && c.cell >= 4.5, `${scheme}: text contrast ${JSON.stringify(c)}`);
        t.assert(c.ring >= 3, `${scheme}: focus ring contrast ${JSON.stringify(c)}`);
        t.note(`${scheme}: the table lines use the token --grid-line, as the table panel does (an alpha colour: no contrast rule)`);
        // viewPanel shows the text at full size.
        const v = await t.page({ width: 1280, height: 900 });
        await v.emulateColorScheme(scheme);
        await v.goto(t.dash("txt107", "viewPanel=1"));
        await v.idle();
        await v.waitSel('.panel--view [data-testid="text-1"]');
        const full = await v.eval(`(() => { const r = document.querySelector('.panel--view [data-testid="text-1"]').getBoundingClientRect(); return [r.width > 900, r.height > 500]; })()`);
        t.eq(full, [true, true], `${scheme}: viewPanel`);
        // Print shows the text.
        await v.send("Emulation.setEmulatedMedia", { media: "print" });
        t.eq(await v.eval(`(() => { const r = document.querySelector('.panel--view [data-testid="text-1"]'); const s = getComputedStyle(r); return [s.display !== "none", s.overflowY, r.textContent.includes("Heading")]; })()`), [true, "visible", true], `${scheme}: print`);
        await v.send("Emulation.setEmulatedMedia", { media: "" });
      }
      t.note("axe-core is not run: it needs the network (as in B82)");
    },
  },
  {
    id: "B108",
    name: "the format and the server: 200 and canonical bytes, 422 with the code and the pointer of 4.33a, 400 for a query and a preview, bad_reference for a reuse, a reader without source.query, no change for a document with no text panel (4.33a, 6.20, 6.22)",
    async run(t) {
      const good = one("txt108", [tp(1, "Notes", ["# Runbook", "", "Region: $region."]), panel(2, "stat", "Stat", ["SELECT 1 AS one"], { grid: { x: 12, y: 0, w: 12, h: 8 } })]);
      const made = await t.api.create("transit", good);
      t.eq(made.status, 201, "create");
      const saved = await t.api.save("txt108", made.etag, good, "again");
      t.assert(saved.status === 200, `save ${saved.status} ${saved.text.slice(0, 200)}`);
      const got = (await t.api.dashboard("txt108")).json.document;
      t.eq(Object.keys(got.sections[0].panels[0]), ["id", "type", "title", "description", "transparent", "grid", "options", "links"], "the canonical members");
      t.eq(Object.keys(got.sections[0].panels[0].options), ["mode", "content"], "the canonical options");
      const text1 = (await t.api.download("txt108")).text;
      const again = await t.api.save("txt108", (await t.api.dashboard("txt108")).etag, JSON.parse(text1), "same");
      t.eq((await t.api.download("txt108")).text, text1, "the same bytes after a save of the same document");
      void again;
      // 422 with the code and the pointer.
      const bad = (mutate) => { const d = one("txt108x", [tp(1, "T", ["x"]), panel(2, "stat", "S", ["SELECT 1"], { grid: { x: 12, y: 0, w: 12, h: 8 } })]); mutate(d.sections[0].panels, d); return d; };
      const P = "/sections/0/panels/0";
      const cases = [
        ["mode html", (ps) => { ps[0].options.mode = "html"; }, "bad_format", `${P}/options/mode`],
        ["mode code", (ps) => { ps[0].options.mode = "code"; }, "bad_format", `${P}/options/mode`],
        ["mode absent", (ps) => { delete ps[0].options.mode; }, "missing_member", `${P}/options`],
        ["content absent", (ps) => { delete ps[0].options.content; }, "missing_member", `${P}/options`],
        ["content not an array", (ps) => { ps[0].options.content = "text"; }, "wrong_type", `${P}/options/content`],
        ["a line that is not text", (ps) => { ps[0].options.content = [1]; }, "wrong_type", `${P}/options/content/0`],
        ["a line with U+000A", (ps) => { ps[0].options.content = ["a\nb"]; }, null, `${P}/options/content/0`],
        ["a line with U+0001", (ps) => { ps[0].options.content = ["a\u0001"]; }, null, `${P}/options/content/0`],
        ["a line of 8193 bytes", (ps) => { ps[0].options.content = ["a".repeat(8193)]; }, "too_large", `${P}/options/content/0`],
        ["2001 lines", (ps) => { ps[0].options.content = Array.from({ length: 2001 }, () => "a"); }, "too_large", `${P}/options/content`],
        ["65537 bytes", (ps) => { ps[0].options.content = [...Array.from({ length: 8 }, () => "a".repeat(8191)), "a".repeat(10)]; }, "too_large", `${P}/options/content`],
        ["data in a text panel", (ps) => { ps[0].data = { query: { source: "prod", format: "table", sql: ["SELECT 1"], min_interval: null, max_data_points: null } }; }, "not_permitted", `${P}/data`],
        ["transforms in a text panel", (ps) => { ps[0].transforms = []; }, "not_permitted", `${P}/transforms`],
        ["fields in a text panel", (ps) => { ps[0].fields = { defaults: {}, overrides: [] }; }, "not_permitted", `${P}/fields`],
        ["repeat in a text panel", (ps) => { ps[0].repeat = { variable: "v", direction: "h" }; }, "not_permitted", `${P}/repeat`],
        ["an unknown member of options", (ps) => { ps[0].options.extra = 1; }, null, `${P}/options/extra`],
        ["a reuse of a text panel", (ps) => { ps[1].data = { reuse: { panel: 1, with_transforms: true } }; }, "bad_reference", "/sections/0/panels/1/data/reuse/panel"],
        ["the type texts", (ps) => { ps[0].type = "texts"; }, "bad_format", `${P}/type`],
      ];
      const wrong = [];
      for (const [label, mutate, code, pointer] of cases) {
        const r = await t.api.create("transit", bad(mutate));
        const errors = (r.json && r.json.error && r.json.error.errors) || [];
        const hit = errors.find((e) => e.pointer === pointer && (code === null || e.code === code));
        if (r.status !== 422 || !hit) wrong.push(`${label}: ${r.status} ${JSON.stringify(errors.slice(0, 3).map((e) => [e.code, e.pointer]))}`);
      }
      t.eq(wrong, [], "the invalid documents");
      // 400 for a panel query and a preview of a text panel.
      const q = await t.api.call("POST", "api/dashboards/txt108/panels/1/query", { body: { etag: (await t.api.dashboard("txt108")).etag, from_ms: 1, to_ms: 2, max_data_points: 10, variables: {} } });
      t.eq([q.status, q.json && q.json.error && q.json.error.code, q.json && q.json.error && q.json.error.message], [400, "bad_request", "This panel has no query."], "the query of a text panel");
      const pv = await t.api.call("POST", "api/sources/prod/preview", { body: { uid: "txt108", from_ms: 1, to_ms: 2, max_data_points: 10, dashboard_variables: [], variables: {}, panel: got.sections[0].panels[0] } });
      t.eq([pv.status, pv.json && pv.json.error && pv.json.error.code, pv.json && pv.json.error && pv.json.error.message], [400, "bad_request", "This panel has no query."], "the preview of a text panel");
      // A document with no text panel keeps its bytes: the seed dashboard is as the file of the volume.
      const { readFileSync } = await import("node:fs");
      t.eq((await t.api.download("nodes")).text, readFileSync(`${t.server.volume}/transit/nodes.json`, "utf8"), "a document with no text panel keeps its bytes");
      // The mode authzen: a subject with dashboard.read and no source.query loads a dashboard of text panels only.
      const a = await t.authzen();
      const made2 = await a.call("admin", "POST", "api/dashboards", { body: { folder: "transit", message: "", document: one("txt108a", [tp(1, "Notes", ["# Only text", "", "Body"])]) }, headers: { "Idempotency-Key": crypto.randomUUID() } });
      t.eq(made2.status, 201, "admin creates a dashboard of text panels");
      t.eq((await a.call("author", "GET", "api/dashboards/txt108a")).status, 200, "the author reads it");
      const p = await t.page();
      await p.setHeaders(a.headers("author"));
      await p.goto(a.server.url + "d/txt108a");
      await p.idle();
      await p.waitSel(p.tid("text-1"));
      t.eq(await p.eval(`[document.querySelector('[data-testid="panel-1"]').dataset.state, document.querySelector('[data-testid="text-1"] h1').textContent]`), ["ready", "Only text"], "the panel shows");
    },
  },
  {
    id: "B109",
    name: "the import of tests/fixtures/grafana/text_panels.json: the report lists the items of 15.6, the server loads the dashboard, each panel shows the text of its row (15.6)",
    async run(t) {
      const { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } = await import("node:fs");
      const { tmpdir } = await import("node:os");
      const { spawnSync } = await import("node:child_process");
      const { join } = await import("node:path");
      const { BINARY, ROOT } = await import("../lib/env.mjs");
      const s = await t.ownServer("imp109");
      // The report holds SQL, so the importer refuses a path inside a git checkout (15.18): the input and the report are in the temporary folder.
      const work = mkdtempSync(join(tmpdir(), "hune-b109-"));
      const fx = join(ROOT, "tests/fixtures/grafana");
      mkdirSync(join(work, "dashboards"), { recursive: true });
      cpSync(join(fx, "text_panels.json"), join(work, "dashboards/text-panels.json"));
      for (const f of ["folders.json", "health.json", "plugin-clickhouse.json"]) cpSync(join(fx, "export", f), join(work, f));
      writeFileSync(join(work, "search.json"), JSON.stringify([{ uid: "text-panels", folderUid: "fe1x0abc2kq9" }]));
      const run = (extra) => spawnSync(BINARY, ["import-grafana", "--in", work, "--source-map", "x=prod", "--folders", join(fx, "export/folders.map.json"), "--report", join(work, "report"), "-c", s.configPath, ...extra], { encoding: "utf8", env: {} });
      // The dry run lists the items. The operator accepts each one of them in the file of acceptance.
      const dry = run([]);
      t.eq(dry.status, 1, "a dry run with unaccepted items exits with 1: " + dry.stdout.slice(-300));
      const report = JSON.parse(readFileSync(join(work, "report.json"), "utf8"));
      const items = report.dashboards[0].items;
      t.eq(items.map((i) => [i.id.split(":")[1], i.class]), [
        ["/panels/1/options/mode", "unsupported"], ["/panels/2/options/code", "dropped"], ["/panels/2/options/mode", "changed"], ["/panels/3/options/content", "changed"],
        ["/panels/4/options/content", "changed"], ["/panels/5/options/content", "changed"], ["/panels/6/repeat", "unsupported"],
        // The `custom` variable `region` has no member `allowCustomValue`: Grafana reads it as true, Hune keeps the variable closed (15.6).
        ["/templating/list/0/allowCustomValue", "changed"],
      ], "the items of the rows of 15.6");
      const accept = join(work, "accept.txt");
      writeFileSync(accept, `# accepted for the case B109\n${items.map((i) => i.id).join("\n")}\n`);
      await s.stop();
      const wrote = run(["--accept", accept, "--write"]);
      t.eq(wrote.status, 0, "the import with the acceptance: " + wrote.stdout.slice(-400) + wrote.stderr.slice(-300));
      await s.restart();
      const got = await fetch(`${s.url}api/dashboards/text-panels`).then((r) => r.json());
      const panels = got.document.sections.flatMap((x) => x.panels);
      t.assert(panels.length >= 7 && panels.every((p) => p.type === "text"), "text panels: " + JSON.stringify(panels.map((p) => p.type)));
      t.eq(panels.find((p) => p.title === "Mode html").options.content, ["```", "<p style=\"color:red\">Hello <b>HTML</b></p>", "<img src=x onerror=alert(1)>", "```"], "the html becomes a code block");
      t.eq(panels.every((p) => !("repeat" in p) && !("data" in p) && p.options.mode === "markdown"), true, "no repeat, no data, mode markdown");
      const p = await t.page();
      await p.goto(`${s.url}d/text-panels`);
      await p.idle();
      await p.waitFor(`document.querySelectorAll('[data-type="text"][data-state="ready"]').length >= 7`, { what: "the text panels" });
      const shows = await p.eval(`(() => { const by = (title) => [...document.querySelectorAll('[data-type="text"]')].find((f) => f.querySelector(".panel__title")?.textContent.startsWith(title)); const root = (f) => f.querySelector(".md"); return {
        html: root(by("Mode html")).querySelector("pre code").textContent,
        image: [root(by("Table and image")).textContent.includes("!alt"), root(by("Table and image")).querySelector("a")?.getAttribute("href"), !!root(by("Table and image")).querySelector("img"), !!root(by("Table and image")).querySelector("table")],
        raw: !!root(by("HTML in markdown")).querySelector("b, script"),
      }; })()`);
      t.eq(shows.html.includes("<img src=x onerror=alert(1)>"), true, "the HTML shows as text in a code block");
      t.eq(shows.image, [true, "http://127.0.0.1:1/i.png", false, true], "the image shows as ! and a link, with the table");
      t.eq(shows.raw, false, "HTML in a markdown content shows as text");
      t.eq(p.requests.filter((r) => r.url.includes("127.0.0.1:1/")).length, 0, "no image request");
      rmSync(work, { recursive: true, force: true });
    },
  },
];
