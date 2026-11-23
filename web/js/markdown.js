// The Markdown parser of the text panel (9.19b): lines to a tree, or the name of the limit that the text breaks. Pure: no DOM, no HTML.
// It is linear: a step is one visit of a line by a block pass or of a character by an inline pass. It makes 6 steps for each character at most.
import { checkUrl } from "./links.js";

// The limits of the format (4.35) and of the page (9.19b).
export const MAX_TEXT_LINES = 2000;
export const MAX_TEXT_LINE_BYTES = 8192;
export const MAX_TEXT_BYTES = 65536;
export const MD_MAX_DEPTH = 6;
export const MD_MAX_INLINE_DEPTH = 6;
export const MD_MAX_NODES = 4000;
export const MD_MAX_COLUMNS = 20;
export const MD_MAX_URL_BYTES = 2048;
export const MD_MAX_EXPANDED_BYTES = 262144;

/** The message of the state `text-limit` (9.11) for each limit. */
export const MD_MESSAGES = {
  size: "This text is too large: 2,000 lines and 65,536 bytes at most.",
  depth: "This text has lists and quotes more than 6 levels deep. Reduce the depth.",
  inline: "This text has emphasis and links more than 6 levels deep. Reduce the depth.",
  nodes: "This text has more than 4,000 elements. Shorten the text.",
  columns: "A table of this text has more than 20 columns.",
  expanded: "The text with the values of the variables has more than 262,144 bytes. Select fewer values.",
};

class Limit {
  constructor(which) {
    this.which = which;
  }
}

let nodes = 0;
let steps = 0;
const mk = (n) => {
  if (++nodes > MD_MAX_NODES) throw new Limit("nodes");
  return n;
};

const isSpace = (c) => c === undefined || /[ \t\n\xa0]/.test(c);
const PUNCT = /[\p{P}\p{S}]/u;
const isPunct = (c) => c !== undefined && PUNCT.test(c);
const ASCII_PUNCT = /[!-/:-@[-`{-~]/;
const DEST = /[\x21-\x27\x2a-\x7e]/; // a character of a link destination: printable ASCII, no parenthesis
const URL_CHAR = /[\x21-\x3b\x3d-\x7e]/; // a character of an autolink: printable ASCII, no `<`
const ind = (s) => /^ */.exec(s)[0].length;
const blank = (s) => s.trim() === "";

/** True for a destination of 9.19b: the check of 4.23 with case-sensitive prefixes, printable ASCII, 2,048 characters at most. */
export function isSafeHref(url) {
  return typeof url === "string" && url.length > 0 && url.length <= MD_MAX_URL_BYTES && /^([/?#]|https?:\/\/)[\x21-\x7e]*$/.test(url) && checkUrl(url);
}

// Merge the text leaves side by side (a placeholder must not break at a mark that did not match), and remove the empty ones. A `lit` leaf stays alone.
function tidy(list) {
  const out = [];
  for (const n of list) {
    if (n.k === "t") {
      if (n.v === "") continue;
      const p = out[out.length - 1];
      if (p && p.k === "t" && !p.lit && !n.lit) {
        p.v += n.v;
        continue;
      }
    } else if (n.c) n.c = tidy(n.c);
    out.push(n);
  }
  return out;
}

function inline(text) {
  const out = [];
  const stack = []; // the open runs of `*` and `_`
  const brackets = []; // the open `[`
  const count = { "*": 0, _: 0 };
  let links = 0;
  let i = 0;
  let from = 0;
  const flush = (to) => to > from && out.push(mk({ k: "t", v: text.slice(from, to) }));
  const eat = (node, to) => out.push(node) && (i = from = to); // add a node and move over the text that it took
  const wrap = (at, k, extra) => {
    const c = out.splice(at);
    steps += c.length;
    const n = mk({ k, c, d: 1 + Math.max(0, ...c.map((x) => x.d || 0)), ...extra });
    if (n.d > MD_MAX_INLINE_DEPTH) throw new Limit("inline");
    out.push(n);
  };
  // One pass finds each run of backticks and the next run of the same length.
  const runs = [...text.matchAll(/`+/g)].map((m) => ({ p: m.index, n: m[0].length, next: -1 }));
  steps += text.length;
  const last = new Map();
  for (let r = runs.length - 1; r >= 0; r--) {
    runs[r].next = last.has(runs[r].n) ? last.get(runs[r].n) : -1;
    last.set(runs[r].n, r);
  }
  let ri = 0;
  while (i < text.length) {
    steps++;
    const c = text[i];
    if (c === "\\" && i + 1 < text.length && (ASCII_PUNCT.test(text[i + 1]) || text[i + 1] === "\n")) {
      flush(i);
      eat(text[i + 1] === "\n" ? mk({ k: "br" }) : mk({ k: "t", v: text[i + 1], lit: true }), i + 2);
    } else if (c === "\n") {
      let e = i;
      while (e > from && text[e - 1] === " ") e--;
      flush(e);
      steps += i - e;
      eat(i - e >= 2 ? mk({ k: "br" }) : mk({ k: "t", v: " ", lit: true }), i + 1);
    } else if (c === "`") {
      while (ri < runs.length && runs[ri].p < i) ri++;
      const r = runs[ri];
      if (!r || r.p !== i) {
        while (text[i] === "`") i++; // the rest of a run that an escape cut: text
      } else if (r.next >= 0) {
        flush(i);
        const e = runs[r.next];
        let body = text.slice(r.p + r.n, e.p).replace(/\n/g, " ");
        if (body.length > 1 && body[0] === " " && body[body.length - 1] === " " && body.trim() !== "") body = body.slice(1, -1);
        eat(mk({ k: "code", v: body }), e.p + e.n);
        ri = r.next + 1;
      } else i = r.p + r.n;
    } else if (c === "*" || c === "_") {
      let e = i;
      while (text[e] === c) e++;
      let n = e - i;
      steps += n;
      const before = text[i - 1];
      const after = text[e];
      const left = !isSpace(after) && (!isPunct(after) || isSpace(before) || isPunct(before));
      const right = !isSpace(before) && (!isPunct(before) || isSpace(after) || isPunct(after));
      const open = c === "*" ? left : left && (!right || isPunct(before));
      const close = c === "*" ? right : right && (!left || isPunct(after));
      flush(i);
      from = e;
      i = e;
      // A run that closes matches the nearest open run of its character. The open runs between them stay as text.
      while (n > 0 && close && count[c] > 0) {
        while (stack[stack.length - 1].ch !== c) {
          count[stack.pop().ch]--;
          steps++;
        }
        const o = stack[stack.length - 1];
        const k = n >= 2 && o.n >= 2 ? 2 : 1;
        o.n -= k;
        o.node.v = o.node.v.slice(k);
        n -= k;
        while (brackets.length && brackets[brackets.length - 1].idx > o.idx) brackets.pop(); // a `[` inside the pair stays as text
        wrap(o.idx + 1, k === 2 ? "strong" : "em");
        if (o.n === 0) {
          stack.pop();
          count[c]--;
        }
      }
      if (n > 0) {
        const t = mk({ k: "t", v: c.repeat(n) });
        out.push(t);
        if (open) {
          stack.push({ ch: c, n, node: t, idx: out.length - 1 });
          count[c]++;
        }
      }
    } else if (c === "[") {
      flush(i);
      brackets.push({ idx: out.length, links });
      eat(mk({ k: "t", v: "[" }), i + 1);
    } else if (c === "]") {
      const b = brackets.pop();
      let done = false;
      if (b && text[i + 1] === "(" && b.links === links) {
        // The destination ends at the first character that is not printable ASCII, or is a parenthesis: no character is in two scans.
        let e = i + 2;
        while (e < text.length && e - i - 2 <= MD_MAX_URL_BYTES && DEST.test(text[e])) e++;
        steps += e - i;
        const dest = text.slice(i + 2, e);
        if (text[e] === ")" && isSafeHref(dest) && (i > from || out.length > b.idx + 1)) {
          flush(i);
          while (stack.length && stack[stack.length - 1].idx > b.idx) count[stack.pop().ch]--;
          out.splice(b.idx, 1);
          wrap(b.idx, "a", { href: dest });
          links++;
          i = from = e + 1;
          done = true;
        }
      }
      if (!done) i++;
    } else if (c === "h" && brackets.length === 0 && (text.startsWith("http://", i) || text.startsWith("https://", i)) && !/[A-Za-z0-9]/.test(text[i - 1] || " ")) {
      // An autolink loses the marks of the sentence at its end, and a `)` with no `(`.
      let e = i;
      let open = 0;
      let close = 0;
      for (; e < text.length && URL_CHAR.test(text[e]); e++) {
        if (text[e] === "(") open++;
        else if (text[e] === ")") close++;
      }
      steps += e - i;
      for (;;) {
        const ch = text[e - 1];
        if (ch === ")" && close > open) close--;
        else if (!(ch && "?!.,:;*_~'\"".includes(ch))) break;
        e--;
      }
      const u = text.slice(i, e);
      if (u.length > (u.startsWith("https") ? 8 : 7) && isSafeHref(u)) {
        flush(i);
        out.push(mk({ k: "a", href: u, c: [mk({ k: "t", v: u, lit: true })], d: 1 })); // its text is its destination: no placeholder
        links++;
        from = e;
      }
      i = Math.max(e, i + 1);
    } else i++;
  }
  flush(text.length);
  return tidy(out);
}

const fenceOf = (s) => {
  const n = ind(s);
  const c = s[n];
  if (n > 3 || (c !== "`" && c !== "~")) return null;
  let e = n;
  while (s[e] === c) e++;
  if (e - n < 3 || (c === "`" && s.indexOf("`", e) >= 0)) return null;
  return { c, n: e - n, indent: n };
};

const headingOf = (s) => {
  const n = ind(s);
  if (n > 3 || s[n] !== "#") return null;
  let e = n;
  while (s[e] === "#") e++;
  if (e - n > 6 || (e < s.length && s[e] !== " " && s[e] !== "\t")) return null;
  let t = s.slice(e).trim();
  let k = t.length;
  while (k > 0 && t[k - 1] === "#") k--;
  if (k === 0) t = "";
  else if (k < t.length && (t[k - 1] === " " || t[k - 1] === "\t")) t = t.slice(0, k).trim();
  return { level: e - n, text: t };
};

const hrOf = (s) => {
  const n = ind(s);
  const c = s[n];
  if (n > 3 || (c !== "-" && c !== "*" && c !== "_")) return false;
  let k = 0;
  for (let p = n; p < s.length; p++) {
    if (s[p] === c) k++;
    else if (s[p] !== " " && s[p] !== "\t") return false;
  }
  return k >= 3;
};

// An item of a list. `width` is the indent that the next lines of the item need.
const itemOf = (s) => {
  const n = ind(s);
  if (n > 3) return null;
  let e = n;
  let ordered = false;
  if (s[e] === "-" || s[e] === "*" || s[e] === "+") e++;
  else {
    while (e < s.length && e - n < 9 && s[e] >= "0" && s[e] <= "9") e++;
    if (e === n || (s[e] !== "." && s[e] !== ")")) return null;
    ordered = true;
    e++;
  }
  if (e < s.length && s[e] !== " ") return null;
  let sp = 0;
  while (s[e + sp] === " ") sp++;
  const empty = e + sp >= s.length;
  if (sp > 4 || empty) sp = 1;
  return { ordered, one: ordered && Number(s.slice(n, e - 1)) === 1, width: e + sp, first: s.slice(e + sp), empty };
};

// The cells of a row of a table. `\|` is a `|` character.
const cellsOf = (s) => {
  steps += s.length;
  let t = s.trim();
  if (t[0] === "|") t = t.slice(1);
  if (t.endsWith("|") && !t.endsWith("\\|")) t = t.slice(0, -1);
  const cells = [];
  let cur = "";
  for (let i = 0; i < t.length; i++) {
    if (t[i] === "\\" && t[i + 1] === "|") {
      cur += "|";
      i++;
    } else if (t[i] === "|") {
      cells.push(cur.trim());
      cur = "";
    } else cur += t[i];
  }
  cells.push(cur.trim());
  return cells;
};

// The alignment of each column ("", "c" or "r") of a delimiter row, else null.
const alignOf = (s) => {
  if (!s.includes("-")) return null;
  const al = [];
  for (const c of cellsOf(s)) {
    if (!/^:?-+:?$/.test(c)) return null;
    al.push(c[0] === ":" && c.endsWith(":") ? "c" : c.endsWith(":") ? "r" : "");
  }
  return al;
};

const quoteLine = (s) => ind(s) <= 3 && s[ind(s)] === ">";

function blocks(lines, depth) {
  if (depth > MD_MAX_DEPTH) throw new Limit("depth");
  const out = [];
  let i = 0;
  while (i < lines.length) {
    steps++;
    const s = lines[i];
    if (blank(s)) {
      i++;
      continue;
    }
    const f = fenceOf(s);
    if (f) {
      // A code block ends at a closing fence, or at the end of the container.
      const body = [];
      for (i++; i < lines.length; i++) {
        steps++;
        const g = fenceOf(lines[i]);
        if (g && g.c === f.c && g.n >= f.n && lines[i].slice(g.indent + g.n).trim() === "") break;
        body.push(lines[i].slice(Math.min(f.indent, ind(lines[i]))));
      }
      i++;
      out.push(mk({ k: "pre", lines: body }));
      continue;
    }
    const h = headingOf(s);
    if (h || hrOf(s)) {
      out.push(h ? mk({ k: "h", level: h.level, c: inline(h.text) }) : mk({ k: "hr" }));
      i++;
      continue;
    }
    if (quoteLine(s)) {
      const inner = [];
      for (; i < lines.length && quoteLine(lines[i]); i++) {
        steps++;
        const t = lines[i].slice(ind(lines[i]) + 1);
        inner.push(t[0] === " " ? t.slice(1) : t);
      }
      out.push(mk({ k: "quote", c: blocks(inner, depth + 1) }));
      continue;
    }
    const it = itemOf(s);
    if (it) {
      // An item takes the next lines that have its indent, and the blank lines that such a line follows.
      const list = mk({ k: it.ordered ? "ol" : "ul", c: [] });
      while (i < lines.length) {
        const m = itemOf(lines[i]);
        if (!m || m.ordered !== it.ordered || hrOf(lines[i])) break;
        const body = [m.first];
        i++;
        for (let j = i; j < lines.length; steps++) {
          if (blank(lines[j])) j++;
          else if (ind(lines[j]) >= m.width) {
            for (; i < j; i++) body.push("");
            body.push(lines[i].slice(m.width));
            j = ++i;
          } else break;
        }
        list.c.push(mk({ k: "li", c: blocks(body, depth + 1) }));
        while (i < lines.length && blank(lines[i])) i++;
      }
      out.push(list);
      continue;
    }
    if (i + 1 < lines.length && s.includes("|")) {
      const al = alignOf(lines[i + 1]);
      const head = al ? cellsOf(s) : null;
      if (al && al.length === head.length) {
        if (head.length > MD_MAX_COLUMNS) throw new Limit("columns");
        const rows = [];
        for (i += 2; i < lines.length && !blank(lines[i]) && lines[i].includes("|"); i++) {
          steps++;
          const r = cellsOf(lines[i]);
          while (r.length < head.length) r.push("");
          rows.push(r.slice(0, head.length));
        }
        const row = (r) => r.map((c, x) => mk({ k: "cell", al: al[x], c: inline(c) }));
        out.push(mk({ k: "table", head: row(head), rows: rows.map(row) }));
        continue;
      }
    }
    // A paragraph ends at a blank line, and before a fence, a heading, a rule, a quote, a bullet item or an ordered item that starts with 1.
    const para = [s.replace(/^ +/, "")];
    for (i++; i < lines.length && !blank(lines[i]); i++) {
      const g = lines[i];
      const m = itemOf(g);
      if (fenceOf(g) || headingOf(g) || hrOf(g) || quoteLine(g) || (m && !m.empty && (!m.ordered || m.one))) break;
      steps++;
      para.push(g.replace(/^ +/, ""));
    }
    let text = para.join("\n");
    let end = text.length;
    while (end > 0 && text[end - 1] === " ") end--; // no hard break at the end
    out.push(mk({ k: "p", c: inline(text.slice(0, end)) }));
  }
  return out;
}

const utf8 = new TextEncoder();

/** Parse the lines of a text panel: `{ ok: true, tree, stats }`, or `{ ok: false, limit, stats }` (`limit` is a key of `MD_MESSAGES`). */
export function parseMarkdown(lines) {
  nodes = 0;
  steps = 0;
  const stats = () => ({ steps, nodes });
  let chars = lines.length - 1;
  for (const l of lines) chars += l.length;
  // The size check comes first. The length is a lower bound of the bytes. The size of a line is a rule of the format: a long line is no risk here.
  if (lines.length > MAX_TEXT_LINES || chars > MAX_TEXT_BYTES || utf8.encode(lines.join("\n")).length > MAX_TEXT_BYTES) {
    return { ok: false, limit: "size", stats: stats() };
  }
  try {
    const tree = blocks(lines.map((l) => (l.includes("\t") ? l.replace(/^[ \t]+/, (m) => m.replace(/\t/g, "    ")) : l)), 0); // a tab at the start counts as 4 spaces
    return { ok: true, tree, stats: stats() };
  } catch (e) {
    if (e instanceof Limit) return { ok: false, limit: e.which, stats: stats() };
    throw e;
  }
}
