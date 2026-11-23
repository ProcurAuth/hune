// The DOM of the text panel (9.19b): DOM nodes from the tree of markdown.js, with createElement, createTextNode and setAttribute only.
// The element names come from a constant table. The text goes in with createTextNode. The attributes are `href` (checked), `rel` and `class`.
import { MD_MAX_EXPANDED_BYTES, MD_MESSAGES, isSafeHref, parseMarkdown } from "../markdown.js";

// The 24 names of rule 2 of 9.19b, and the root. The kind of the node is the key: no name comes from the text.
const TAG = {
  root: "div", p: "p", quote: "blockquote", ul: "ul", ol: "ol", li: "li", hr: "hr", em: "em", strong: "strong", code: "code", br: "br", a: "a", pre: "pre",
  table: "table", thead: "thead", tbody: "tbody", tr: "tr", th: "th", td: "td", h1: "h1", h2: "h2", h3: "h3", h4: "h4", h5: "h5", h6: "h6",
};
const ALIGN = { c: "md-c", r: "md-r" };
const utf8 = new TextEncoder();

const Expanded = {};

/** Build the DOM of a tree: `{ root }`, or `{ limit: "expanded" }` when the text, with the values that `expand(text)` gives, is too large. */
export function buildDom(doc, tree, expand = (text) => text) {
  let bytes = 0;
  const leaf = (v, lit) => {
    const out = lit || !v.includes("$") ? v : expand(v);
    if (typeof out !== "string") throw Expanded; // expand gives null when the text passes the budget (runtime.js)
    bytes += utf8.encode(out).length;
    if (bytes > MD_MAX_EXPANDED_BYTES) throw Expanded;
    return doc.createTextNode(out);
  };
  const make = (kind) => {
    const name = TAG[kind];
    if (!name) throw new TypeError(`unknown kind ${kind}`);
    return doc.createElement(name);
  };
  const add = (parent, list) => {
    for (const n of list) parent.appendChild(one(n));
    return parent;
  };
  const code = (v) => {
    const e = make("code");
    e.appendChild(leaf(v, false));
    return e;
  };
  const row = (cells, kind) => {
    const tr = make("tr");
    for (const c of cells) {
      const cell = add(make(kind), c.c);
      if (ALIGN[c.al]) cell.setAttribute("class", ALIGN[c.al]);
      tr.appendChild(cell);
    }
    return tr;
  };
  const one = (n) => {
    switch (n.k) {
      case "t": return leaf(n.v, n.lit === true);
      case "h": return add(make(`h${n.level}`), n.c);
      case "code": return code(n.v);
      case "pre": {
        const pre = make("pre");
        pre.appendChild(code(n.lines.join("\n")));
        return pre;
      }
      case "a": {
        // The parser checked the destination. The builder checks it again: an unsafe tree gives an error, never a link.
        if (!isSafeHref(n.href)) throw new TypeError("unsafe destination");
        const a = add(make("a"), n.c);
        a.setAttribute("href", n.href);
        a.setAttribute("rel", "noopener noreferrer");
        return a;
      }
      case "li": {
        // A tight item (a paragraph first, then lists only) shows the text of the paragraph with no `p`.
        const li = make("li");
        const tight = n.c.length > 0 && n.c[0].k === "p" && n.c.every((b, i) => i === 0 || b.k === "ul" || b.k === "ol");
        n.c.forEach((b, i) => (tight && i === 0 ? add(li, b.c) : li.appendChild(one(b))));
        return li;
      }
      case "table": {
        const table = make("table");
        const body = make("tbody");
        for (const r of n.rows) body.appendChild(row(r, "td"));
        const head = make("thead");
        head.appendChild(row(n.head, "th"));
        table.appendChild(head);
        table.appendChild(body);
        return table;
      }
      default: return n.c ? add(make(n.k), n.c) : make(n.k);
    }
  };
  try {
    const root = make("root");
    root.setAttribute("class", "md");
    return { root: add(root, tree) };
  } catch (e) {
    if (e === Expanded) return { limit: "expanded" };
    throw e;
  }
}

/** The text of a panel: it parses `options.content` once. `draw(doc, expand)` gives `{ root }` or `{ limit, message }`. `key(expand)` changes with the values that the text names. */
export function createText(panel) {
  const content = panel.options && Array.isArray(panel.options.content) && panel.options.content.every((l) => typeof l === "string") ? panel.options.content : [];
  const parsed = parseMarkdown(content);
  // The placeholders that the text holds. The key reads them and never expands the whole text.
  const refs = [...new Set(content.join("\n").match(/\$\{[A-Za-z0-9_]+(?::[A-Za-z0-9_]+)?\}|\$[A-Za-z0-9_]+/g) || [])];
  const limit = (key) => ({ limit: key, message: MD_MESSAGES[key] });
  return {
    parsed,
    draw(doc, expand) {
      if (!parsed.ok) return limit(parsed.limit);
      const built = buildDom(doc, parsed.tree, expand);
      return built.limit ? limit(built.limit) : built;
    },
    fail: () => limit("expanded"),
    key: (expand) => refs.map((r) => expand(r)).join("\0"),
  };
}
