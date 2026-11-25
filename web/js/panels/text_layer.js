// A layer of real text over each chart canvas, so that a person can select the axes, the titles and the labels that the chart draws, and copy them.
// A canvas draws text as pixels: the browser cannot select it. This module records each `fillText` call of a canvas during one draw (the text, the
// position, the font and the transform), and it puts the same texts as transparent DOM text at the same places (the technique of the text layer of
// a PDF viewer). It changes no drawing code: it wraps `fillText` and `clearRect` of the 2D context one time (installTextLayer).
// The pure functions are the model and a test can run them without a browser: parseFont, scaledFont, placeText, readingOrder, signature, textItems.

// The most texts that one canvas puts in its layer. The texts that come after the cap, in reading order, are not in the layer (the chart still draws them).
export const MAX_TEXTS = 400;
// A draw records at most twice the cap before it stops to measure the next texts: a chart with thousands of labels never makes the draw slow.
const RECORD_LIMIT = MAX_TEXTS * 2;

// The size in pixels of a canvas font ("500 11px Inter, sans-serif"), or null when the font has no size in pixels.
export function parseFont(font) {
  const match = /(\d+(?:\.\d+)?)px/.exec(String(font));
  return match ? Number.parseFloat(match[1]) : null;
}

// The font with its size multiplied by k (the scale of the transform of the canvas, divided by the pixel ratio).
export function scaledFont(font, k) {
  return String(font).replace(/(\d+(?:\.\d+)?)px/, (_, size) => `${Math.round(Number.parseFloat(size) * k * 100) / 100}px`);
}

// The place of one text in CSS pixels relative to the canvas. `m` is the transform [a, b, c, d, e, f] of the canvas context at the call, `ratio` is
// canvas pixels for one CSS pixel. The result is the point of the text origin (left, top), the angle in radians, the shift (dx, dy) in the turned
// frame that the alignment and the baseline need, and the size of the box (w, h) in CSS pixels.
export function placeText({ x, y, m, ratio, width, ascent, descent, size, align, baseline }) {
  const [a, b, c, d, e, f] = m;
  const k = Math.hypot(a, b) / ratio;
  const left = (a * x + c * y + e) / ratio;
  const top = (b * x + d * y + f) / ratio;
  const fallback = (size || 12) * 1.2;
  const asc = Number.isFinite(ascent) ? ascent : fallback * 0.8;
  const desc = Number.isFinite(descent) ? descent : fallback * 0.2;
  const w = width * k;
  const h = (asc + desc) * k;
  let dx = 0;
  if (align === "center") dx = -w / 2;
  else if (align === "right" || align === "end") dx = -w;
  let dy = -asc * k;
  if (baseline === "middle") dy = -h / 2;
  else if (baseline === "top") dy = 0;
  else if (baseline === "hanging") dy = -0.1 * h;
  else if (baseline === "bottom" || baseline === "ideographic") dy = -h;
  return { left, top, angle: Math.atan2(b, a), dx, dy, w, h, scale: k };
}

// The items in reading order: from the top row to the bottom row, and from left to right in a row. Rows are 4 px high.
export function readingOrder(items) {
  return items.slice().sort((p, q) => Math.round(p.top / 4) - Math.round(q.top / 4) || p.left - q.left);
}

// A short text that changes when a text, a place or a font changes. A draw that gives the same signature keeps the layer as it is.
export function signature(items, canvasWidth, canvasHeight) {
  let out = `${canvasWidth}x${canvasHeight}`;
  for (const t of items) out += `|${t.text}@${Math.round(t.left * 10)},${Math.round(t.top * 10)},${Math.round(t.angle * 100)},${t.font}`;
  return out;
}

// The texts of one draw as placed items, in reading order, cut to the cap. `records` come from the wrapper of fillText.
export function textItems(records, ratio) {
  const placed = records.map((r) => ({
    text: r.text,
    font: scaledFont(r.font, Math.hypot(r.m[0], r.m[1]) / ratio),
    ...placeText({ x: r.x, y: r.y, m: r.m, ratio, width: r.width, ascent: r.asc, descent: r.desc, size: parseFont(r.font), align: r.align, baseline: r.baseline }),
  }));
  return readingOrder(placed).slice(0, MAX_TEXTS);
}

// The element that holds the layer of a canvas, or null when the canvas gets none: a canvas that does not take the pointer (a layer of marks) or
// a canvas in a parent that is not positioned. The layer is a child of the parent of the canvas, after the other children, so it is over them.
export function layerHost(canvas) {
  const parent = canvas.parentElement;
  if (!parent) return null;
  const view = canvas.ownerDocument.defaultView;
  if (view.getComputedStyle(canvas).pointerEvents === "none") return null;
  if (view.getComputedStyle(parent).position === "static") return null;
  return parent;
}

// Writes the items as the spans of the layer of a canvas. The layer is made once and kept: it is replaced child by child only when the texts change.
export function syncLayer(canvas, host, items) {
  const doc = canvas.ownerDocument;
  let layer = canvas.__huneTextLayer;
  if (!layer || !layer.isConnected) {
    layer = doc.createElement("div");
    layer.className = "textLayer";
    layer.setAttribute("aria-hidden", "true");
    layer.dataset.testid = "text-layer";
    canvas.__huneTextLayer = layer;
  }
  // The layer is the last child of the host, so it is over the overlay that takes the pointer of a chart.
  if (host.lastElementChild !== layer) host.appendChild(layer);
  layer.style.left = `${canvas.offsetLeft}px`;
  layer.style.top = `${canvas.offsetTop}px`;
  layer.style.width = `${canvas.clientWidth}px`;
  layer.style.height = `${canvas.clientHeight}px`;
  const spans = [];
  for (const t of items) {
    const span = doc.createElement("span");
    span.textContent = t.text;
    span.style.font = t.font;
    span.style.lineHeight = `${Math.max(1, Math.round(t.h * 100) / 100)}px`;
    span.style.transform = `translate(${Math.round(t.left * 100) / 100}px, ${Math.round(t.top * 100) / 100}px) rotate(${Math.round(t.angle * 1e4) / 1e4}rad) translate(${Math.round(t.dx * 100) / 100}px, ${Math.round(t.dy * 100) / 100}px)`;
    spans.push(span);
  }
  layer.replaceChildren(...spans);
  return layer;
}

// The text that a selection over the layers gives for a copy: the selected part of each text, one text on each line, in reading order. The browser
// joins the texts of absolutely placed boxes with no line end, so a copy of an axis would give "100 ms200 ms". It returns null when the selection is
// not inside the layers only (then the copy of the browser applies). `doc` is the document.
export function selectedLayerText(doc) {
  const selection = doc.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return null;
  const layerOf = (node) => {
    const element = node && node.nodeType === 3 ? node.parentElement : node;
    return element && element.closest ? element.closest(".textLayer") : null;
  };
  if (!layerOf(selection.anchorNode) || !layerOf(selection.focusNode)) return null;
  const range = selection.getRangeAt(0);
  const parts = [];
  for (const layer of doc.querySelectorAll(".textLayer")) {
    if (!range.intersectsNode(layer)) continue;
    for (const span of layer.children) {
      if (!range.intersectsNode(span)) continue;
      const node = span.firstChild;
      const text = node ? node.data : "";
      const from = range.startContainer === node ? range.startOffset : 0;
      const to = range.endContainer === node ? range.endOffset : text.length;
      if (to > from) parts.push(text.slice(from, to));
    }
  }
  return parts.join("\n");
}

const MARK = Symbol.for("hune.textLayer");

// Wraps `fillText` and `clearRect` of the 2D context, one time for the page. `win` is the global object (a test gives a fake one).
// `schedule` runs the end of a draw: the default is a microtask, which runs when the code of the draw returns.
export function installTextLayer({ win = globalThis, schedule = queueMicrotask, host = layerHost, sync = syncLayer } = {}) {
  const Context = win.CanvasRenderingContext2D;
  if (!Context || Context.prototype[MARK]) return false;
  const proto = Context.prototype;
  const fillText = proto.fillText;
  const clearRect = proto.clearRect;
  const states = new WeakMap();

  function commit(ctx, st) {
    st.open = false;
    const canvas = ctx.canvas;
    if (!canvas || !canvas.isConnected) {
      st.list = [];
      return;
    }
    if (st.host === undefined) {
      st.host = host(canvas);
      if (st.host === null) {
        st.list = [];
        return;
      }
    }
    if (st.host === null) return;
    const ratio = canvas.clientWidth ? canvas.width / canvas.clientWidth : 1;
    const items = textItems(st.list, ratio || 1);
    const sig = signature(items, canvas.width, canvas.height);
    if (sig === st.sig && canvas.__huneTextLayer && canvas.__huneTextLayer.isConnected) return;
    st.sig = sig;
    sync(canvas, st.host, items);
  }

  proto.fillText = function (text, x, y, ...rest) {
    try {
      let st = states.get(this);
      if (!st) {
        st = { open: false, list: [], sig: "", host: undefined };
        states.set(this, st);
      }
      if (st.host !== null) {
        if (!st.open) {
          st.open = true;
          st.list = [];
          const ctx = this;
          schedule(() => commit(ctx, st));
        }
        const s = String(text);
        if (s.trim() !== "" && st.list.length < RECORD_LIMIT) {
          const m = this.getTransform();
          const metrics = this.measureText(s);
          st.list.push({ text: s, x, y, font: this.font, align: this.textAlign, baseline: this.textBaseline, m: [m.a, m.b, m.c, m.d, m.e, m.f], width: metrics.width, asc: metrics.fontBoundingBoxAscent, desc: metrics.fontBoundingBoxDescent });
        }
      }
    } catch (error) {
      // The layer is a help for the person. A fault here never stops the drawing.
    }
    return fillText.call(this, text, x, y, ...rest);
  };

  proto.clearRect = function (x, y, w, h) {
    try {
      const st = states.get(this);
      if (st && st.open) {
        const m = this.getTransform();
        if (w * Math.hypot(m.a, m.b) >= this.canvas.width - 1 && h * Math.hypot(m.c, m.d) >= this.canvas.height - 1) st.list = [];
      }
    } catch (error) {
      // See above.
    }
    return clearRect.call(this, x, y, w, h);
  };

  const doc = win.document;
  if (doc && !doc.__huneTextLayerCopy) {
    doc.__huneTextLayerCopy = true;
    doc.addEventListener("copy", (event) => {
      const text = selectedLayerText(doc);
      if (text === null || !event.clipboardData) return;
      event.clipboardData.setData("text/plain", text);
      event.preventDefault();
    }, true);
  }

  Object.defineProperty(proto, MARK, { value: true });
  return true;
}
