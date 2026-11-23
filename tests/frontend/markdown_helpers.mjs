// Helpers of the Markdown tests (9.19b, 16.10a). They are not part of the page.
import { parseMarkdown } from "../../web/js/markdown.js";

/** The kinds of node of the tree. `cell` is a cell of a table and `t` is a text leaf. */
export const MD_KINDS = ["p", "h", "quote", "ul", "ol", "li", "hr", "pre", "table", "cell", "t", "em", "strong", "code", "a", "br"];

const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");

/** A tree as text, with the escape of `&`, `<` and `"`. A tight item (9.19b) shows its paragraph with no `p`. */
export function serialize(n) {
  if (Array.isArray(n)) return n.map(serialize).join("");
  switch (n.k) {
    case "t": return esc(n.v);
    case "br": return "<br>";
    case "hr": return "<hr>";
    case "code": return `<code>${esc(n.v)}</code>`;
    case "pre": return `<pre><code>${esc(n.lines.join("\n"))}</code></pre>`;
    case "h": return `<h${n.level}>${serialize(n.c)}</h${n.level}>`;
    case "a": return `<a href="${esc(n.href)}">${serialize(n.c)}</a>`;
    case "li": return `<li>${n.c.length > 0 && n.c[0].k === "p" && n.c.slice(1).every((b) => b.k === "ul" || b.k === "ol") ? serialize(n.c[0].c) + serialize(n.c.slice(1)) : serialize(n.c)}</li>`;
    case "table": return `<table><thead><tr>${n.head.map((c) => serialize(c)).join("")}</tr></thead><tbody>${n.rows.map((r) => `<tr>${r.map((c) => serialize(c)).join("")}</tr>`).join("")}</tbody></table>`;
    case "cell": return `<c${n.al ? ` ${n.al}` : ""}>${serialize(n.c)}</c>`;
    default: return `<${n.k}>${serialize(n.c)}</${n.k}>`;
  }
}

/** The text of a source as a serialised tree, or `limit:<name>`. A source is one text: its line ends split it into lines. */
export function md(source) {
  const r = parseMarkdown(source.split("\n"));
  return r.ok ? serialize(r.tree) : `limit:${r.limit}`;
}

// A fake `document` for the builder of panels/text.js: it keeps the elements, the attributes and the text leaves, with no parsing of HTML.
class FakeElement {
  constructor(tag) {
    this.tag = tag;
    this.attrs = new Map();
    this.children = [];
  }

  setAttribute(name, value) {
    this.attrs.set(name, String(value));
  }

  appendChild(child) {
    this.children.push(child);
    return child;
  }
}

export const fakeDocument = {
  createElement: (tag) => new FakeElement(tag),
  createTextNode: (text) => ({ text: String(text) }),
};

/** Each element of a fake DOM, in document order. */
export function elements(root) {
  const out = [];
  const visit = (n) => {
    if (!(n instanceof FakeElement)) return;
    out.push(n);
    n.children.forEach(visit);
  };
  visit(root);
  return out;
}

/** The text of a fake DOM: the text leaves, side by side. */
export function textOf(n) {
  return n instanceof FakeElement ? n.children.map(textOf).join("") : n.text;
}

/** The sequence of element names of a fake DOM (the shape). */
export const shapeOf = (root) => elements(root).map((e) => e.tag).join(" ");

/** The 24 names of rule 2 of 9.19b and the root. */
export const ALLOWED_TAGS = new Set("div p h1 h2 h3 h4 h5 h6 em strong code pre blockquote ul ol li hr br a table thead tbody tr th td".split(" "));

/** The checks of rules 2 to 6 of 9.19b on a fake DOM. It returns the destinations of the links. */
export function checkDom(root, checkUrl) {
  const hrefs = [];
  for (const e of elements(root)) {
    if (!ALLOWED_TAGS.has(e.tag)) throw new Error(`element ${e.tag}`);
    for (const [name, value] of e.attrs) {
      const ok = (e.tag === "a" && (name === "href" || name === "rel")) || (name === "class" && (e === root || e.tag === "th" || e.tag === "td"));
      if (!ok) throw new Error(`attribute ${name} on ${e.tag}`);
      if (name === "rel" && value !== "noopener noreferrer") throw new Error(`rel ${value}`);
      if (name === "class" && !["md", "md-c", "md-r"].includes(value)) throw new Error(`class ${value}`);
      if (name === "href") {
        if (!checkUrl(value) || /[^\x21-\x7e]/.test(value)) throw new Error(`href ${value}`);
        hrefs.push(value);
      }
    }
    if (e.tag === "a" && (!e.attrs.has("href") || !e.attrs.has("rel"))) throw new Error("a without href or rel");
  }
  return hrefs;
}

/** The lines of a vector: a text, `{ line, times }` or `{ fill: [char, count], times }`. */
export function expandLines(items) {
  const out = [];
  for (const item of items) {
    if (typeof item === "string") out.push(item);
    else {
      const text = item.fill ? item.fill[0].repeat(item.fill[1]) : item.line;
      for (let i = 0; i < (item.times || 1); i++) out.push(text);
    }
  }
  return out;
}
