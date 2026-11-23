// SPDX-License-Identifier: MIT OR Apache-2.0
// A small fake DOM for the tests of the editor forms (web/js/ui.js and the controls of web/js/editor/forms/controls.js).
// It keeps what the forms use: elements, attributes, the dataset, events, a selector engine for `querySelector` and a focus.
// The text of a node goes in with `textContent` only, and `innerHTML` throws, as the page rules ask (9.1).

export class FakeNode {
  constructor(tag) {
    this.tag = tag;
    this.attrs = new Map();
    this.children = [];
    this.parent = null;
    this.dataset = {};
    this.listeners = new Map();
    this.className = "";
    this.hidden = false;
    this.disabled = false;
    this.checked = false;
    this.value = "";
    this.style = { setProperty() {} };
    this._text = null;
  }

  get textContent() {
    return this._text !== null ? this._text : this.children.map((c) => c.textContent).join("");
  }

  set textContent(v) {
    this.children = [];
    this._text = String(v);
  }

  set innerHTML(_) { throw new Error("innerHTML"); }
  set outerHTML(_) { throw new Error("outerHTML"); }
  get firstChild() { return this.children[0] || null; }
  setAttribute(n, v) { this.attrs.set(n, String(v)); }
  getAttribute(n) { return this.attrs.has(n) ? this.attrs.get(n) : null; }
  removeAttribute(n) { this.attrs.delete(n); }
  hasAttribute(n) { return this.attrs.has(n); }
  addEventListener(t, fn) { this.listeners.set(t, [...(this.listeners.get(t) || []), fn]); }

  append(...nodes) {
    for (const n of nodes) {
      const node = typeof n === "string" ? new FakeText(n) : n;
      node.parent = this;
      this._text = null;
      this.children.push(node);
    }
  }

  appendChild(n) { this.append(n); return n; }

  removeChild(n) {
    this.children = this.children.filter((c) => c !== n);
    n.parent = null;
    return n;
  }

  replaceChildren(...nodes) {
    this.children = [];
    this._text = null;
    this.append(...nodes);
  }

  remove() { if (this.parent) this.parent.removeChild(this); }
  focus() { document.activeElement = this; }
  dispatch(type, extra = {}) { for (const fn of this.listeners.get(type) || []) fn({ type, target: this, ...extra }); }

  // The simple selectors that the forms use: tag, .class, [attr], [attr=v], [attr$=v], :not([attr]), :scope and the combinators " " and ">".
  matches(compound, scope) {
    const re = /^([a-z0-9]+)?((?:\.[\w-]+|\[[\w-]+(?:\$?=(?:"[^"]*"|[^\]]*))?\]|:not\(\[[\w-]+\]\)|:scope)*)$/;
    const m = re.exec(compound);
    if (!m) throw new Error(`selector not supported: ${compound}`);
    if (m[1] && m[1] !== this.tag) return false;
    for (const part of m[2].match(/\.[\w-]+|\[[^\]]*\]|:not\(\[[\w-]+\]\)|:scope/g) || []) {
      if (part === ":scope") { if (this !== scope) return false; continue; }
      if (part.startsWith(".")) { if (!this.className.split(/\s+/).includes(part.slice(1))) return false; continue; }
      if (part.startsWith(":not(")) { if (this.hasValue(part.slice(6, -2))) return false; continue; }
      const a = /^\[([\w-]+)(?:(\$?=)(?:"([^"]*)"|(.*)))?\]$/.exec(part);
      const have = this.attrValue(a[1]);
      if (have === null) return false;
      const want = a[3] !== undefined ? a[3] : a[4];
      if (a[2] === "=" && have !== want) return false;
      if (a[2] === "$=" && !have.endsWith(want)) return false;
    }
    return true;
  }

  attrValue(name) {
    if (name.startsWith("data-")) {
      const key = name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
      return this.dataset[key] !== undefined ? String(this.dataset[key]) : null;
    }
    return this.attrs.has(name) ? this.attrs.get(name) : null;
  }

  hasValue(name) {
    if (name === "disabled") return !!this.disabled;
    return this.attrValue(name) !== null;
  }

  all(out = []) {
    for (const c of this.children) if (c instanceof FakeNode) { out.push(c); c.all(out); }
    return out;
  }

  querySelectorAll(selector) {
    // The compounds with the relation to the one before: " " (a descendant) or ">" (a child).
    const parts = [];
    let rel = "desc";
    for (const token of selector.trim().replace(/\s*>\s*/g, " > ").split(/\s+/)) {
      if (token === ">") rel = "child";
      else {
        parts.push({ sel: token, rel });
        rel = "desc";
      }
    }
    const test = (node, k) => {
      if (!node.matches(parts[k].sel, this)) return false;
      if (k === 0) return true;
      if (parts[k].rel === "child") return node.parent instanceof FakeNode && test(node.parent, k - 1);
      for (let p = node.parent; p instanceof FakeNode; p = p.parent) if (test(p, k - 1)) return true;
      return false;
    };
    return this.all().filter((n) => test(n, parts.length - 1));
  }

  querySelector(selector) { return this.querySelectorAll(selector)[0] || null; }
  getBoundingClientRect() { return { left: 0, top: 0, width: 100, height: 20 }; }
}

class FakeText {
  constructor(text) { this.text = String(text); this.parent = null; }
  get textContent() { return this.text; }
}

export function installFakeDom() {
  globalThis.Node = FakeNode;
  globalThis.document = {
    createElement: (tag) => new FakeNode(tag),
    createElementNS: (_ns, tag) => new FakeNode(tag),
    createTextNode: (text) => new FakeText(text),
    activeElement: null,
    getElementById: () => null,
    addEventListener() {},
    removeEventListener() {},
  };
}

/** The first node with `data-testid` equal to `id` under `root`, or null. */
export const byId = (root, id) => root.querySelector(`[data-testid="${id}"]`);

/** Type a text in a field and run the debounce at once. */
export function type(scope, root, id, text) {
  const node = byId(root, id);
  if (!node) throw new Error(`no control ${id}`);
  node.value = text;
  node.dispatch("input");
  scope.flush();
  return node;
}

/** Choose a value of a select. */
export function choose(root, id, value) {
  const node = byId(root, id);
  if (!node) throw new Error(`no control ${id}`);
  node.value = value;
  node.dispatch("change");
  return node;
}

/** Click a button, or change a check box or a switch (it flips). */
export function click(root, id) {
  const node = byId(root, id);
  if (!node) throw new Error(`no control ${id}`);
  if (node.tag === "input") {
    node.checked = !node.checked;
    node.dispatch("change");
  } else node.dispatch("click");
  return node;
}

/** The texts of the visible error lines under `root`. */
export const errors = (root) => root.querySelectorAll(".field-error").filter((n) => !n.hidden && n.textContent !== "").map((n) => n.textContent);
