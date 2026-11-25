// The preview of a panel that waits for its first result (9.11): the DOM of the tree of shapes that panel_wait.js gives (previewOf).
// A node is { t, c, v, a, k }: the tag, the class, the custom properties, the attributes and the children. The custom properties are set with
// the CSSOM and the shapes are classes: the page never writes a style attribute in its markup (9.29). SVG nodes use the SVG namespace.

import { el } from "./ui.js";
import { previewOf } from "./panel_wait.js";

const SVG_NS = "http://www.w3.org/2000/svg";

// Makes the element of one node and its children.
export function drawNode(n) {
  const e = n.t === "svg" || n.t === "path" ? document.createElementNS(SVG_NS, n.t) : document.createElement(n.t);
  if (n.c) e.setAttribute("class", n.c);
  for (const [name, value] of Object.entries(n.v)) e.style.setProperty(name, value);
  for (const [name, value] of Object.entries(n.a)) e.setAttribute(name, value);
  for (const child of n.k) e.append(drawNode(child));
  return e;
}

// The element `panel__skeleton` of a panel, or null for a panel type that has no preview (a text). It is out of the flow and hidden from assistive
// technology. The CSS shows it for a first load only.
export function buildPreview(panel) {
  const tree = previewOf(panel);
  if (!tree) return null;
  return el("div", { class: "panel__skeleton", "aria-hidden": "true", dataset: { kind: tree.a["data-kind"] } }, drawNode(tree));
}
