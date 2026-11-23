// The hook script of the visual parity capture (15.14c). The tool injects it into each page before the page code runs.
// It wraps the 2D context and the Path2D of the page, and it records what each canvas draws since its last clear.
// It records only. It changes nothing in a drawing call.
(() => {
  if (window.__cv) return;
  const proto = CanvasRenderingContext2D.prototype;
  const P2D = Path2D.prototype;
  const state = new WeakMap(); // canvas -> { ops: [], path: [], clip: null, stack: [] }
  const get = (canvas) => {
    let s = state.get(canvas);
    if (!s) state.set(canvas, (s = { ops: [], path: [], clip: null, stack: [] }));
    return s;
  };
  const num = (v) => Math.round(v * 1000) / 1000;
  const paint = (ctx) => (typeof ctx.fillStyle === "string" ? ctx.fillStyle : "gradient");
  const brush = (ctx) => (typeof ctx.strokeStyle === "string" ? ctx.strokeStyle : "gradient");
  const rectsOf = (path) => path.map((r) => r.map(num));
  // A Path2D keeps its own list of rectangles in a hidden member.
  for (const name of ["rect", "roundRect"]) {
    const original = P2D[name];
    if (!original) continue;
    P2D[name] = function (x, y, w, h, ...rest) {
      (this.__r || (this.__r = [])).push([x, y, w, h]);
      return original.call(this, x, y, w, h, ...rest);
    };
    const own = proto[name];
    if (!own) continue;
    proto[name] = function (x, y, w, h, ...rest) {
      get(this.canvas).path.push([x, y, w, h]);
      return own.call(this, x, y, w, h, ...rest);
    };
  }
  const addPath = P2D.addPath;
  P2D.addPath = function (other, ...rest) {
    if (other && other.__r) (this.__r || (this.__r = [])).push(...other.__r);
    return addPath.call(this, other, ...rest);
  };
  const begin = proto.beginPath;
  proto.beginPath = function () {
    get(this.canvas).path = [];
    return begin.call(this);
  };
  const reset = (canvas) => {
    const s = get(canvas);
    s.ops = [];
    s.path = [];
    s.clip = null;
    s.stack = [];
  };
  // The clip rectangle that applies to a drawing call: the intersection of the rectangles of the `clip` calls since the last `save`.
  const save = proto.save;
  proto.save = function () {
    const s = get(this.canvas);
    s.stack.push(s.clip);
    return save.call(this);
  };
  const restore = proto.restore;
  proto.restore = function () {
    const s = get(this.canvas);
    if (s.stack.length) s.clip = s.stack.pop();
    return restore.call(this);
  };
  const clip = proto.clip;
  proto.clip = function (...args) {
    const s = get(this.canvas);
    const path = args[0] && typeof args[0] === "object" && "addPath" in args[0] ? args[0] : null;
    const rects = path ? path.__r || [] : s.path;
    if (rects.length === 1) {
      const [x, y, w, h] = rects[0];
      const c = s.clip;
      s.clip = c ? [Math.max(x, c[0]), Math.max(y, c[1]), Math.min(x + w, c[0] + c[2]) - Math.max(x, c[0]), Math.min(y + h, c[1] + c[3]) - Math.max(y, c[1])].map(num) : [x, y, w, h].map(num);
    }
    return clip.apply(this, args);
  };
  const clear = proto.clearRect;
  proto.clearRect = function (x, y, w, h) {
    const c = this.canvas;
    if (x <= 0 && y <= 0 && w >= c.width && h >= c.height) reset(c);
    return clear.call(this, x, y, w, h);
  };
  const fill = proto.fill;
  proto.fill = function (...args) {
    const path = args[0] && typeof args[0] === "object" && "addPath" in args[0] ? args[0] : null;
    const rects = path ? path.__r || [] : get(this.canvas).path;
    get(this.canvas).ops.push({ k: "fill", color: paint(this), alpha: num(this.globalAlpha), rects: rectsOf(rects), clip: get(this.canvas).clip });
    return fill.apply(this, args);
  };
  const stroke = proto.stroke;
  proto.stroke = function (...args) {
    const path = args[0] && typeof args[0] === "object" && "addPath" in args[0] ? args[0] : null;
    const rects = path ? path.__r || [] : get(this.canvas).path;
    get(this.canvas).ops.push({ k: "stroke", color: brush(this), alpha: num(this.globalAlpha), width: num(this.lineWidth), dash: this.getLineDash().map(num), rects: rectsOf(rects), clip: get(this.canvas).clip });
    return stroke.apply(this, args);
  };
  const fillRect = proto.fillRect;
  proto.fillRect = function (x, y, w, h) {
    get(this.canvas).ops.push({ k: "fillRect", color: paint(this), alpha: num(this.globalAlpha), rects: [[x, y, w, h].map(num)], clip: get(this.canvas).clip });
    return fillRect.call(this, x, y, w, h);
  };
  const fillText = proto.fillText;
  proto.fillText = function (text, x, y, ...rest) {
    get(this.canvas).ops.push({ k: "text", text: String(text), x: num(x), y: num(y), color: paint(this), align: this.textAlign, baseline: this.textBaseline, font: this.font });
    return fillText.call(this, text, x, y, ...rest);
  };
  const resize = (name) => {
    const d = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, name);
    Object.defineProperty(HTMLCanvasElement.prototype, name, {
      configurable: true,
      get: d.get,
      set(v) {
        reset(this);
        d.set.call(this, v);
      },
    });
  };
  resize("width");
  resize("height");
  /** The records of the canvases that a root element holds: [{ box, size, ops }]. */
  window.__cv = (root) =>
    [...root.querySelectorAll("canvas")].map((c) => {
      const b = c.getBoundingClientRect();
      return { box: [b.left, b.top, b.width, b.height].map(num), size: [c.width, c.height], ops: get(c).ops };
    });
})();
