// The marks of the annotations on a time series (9.19d): the canvas, the card, the keys and the status text. The code is code of Hune.
// The page writes each text of a mark with `textContent` only. The server has already refused markup and control characters (7.37).

import { cardPlace, cardText, hitTest, hookOf, marks, span, stepKey, tableRows, TRIANGLE } from "../annotations.js";
import { parseCss, resolveFill } from "../colors.js";
import { formatTime } from "../zone.js";
import { isDarkTheme } from "./common.js";

// The look of 9.19d: the line (2 px, dash 5 and 5) and the opacity of the fill of a region.
const LINE = 2;
const DASH = [5, 5];
const FILL = 0.1;

/**
 * The CSS colour that the canvas gets for a colour value of 4.19 in a theme, or null when nothing draws (`transparent`, or a value that
 * does not parse). The page never puts the colour in markup. `host` gives the variable `--text` for the token `text`.
 */
export function ink(host, c, dark) {
  if (c === "text") return getComputedStyle(host).getPropertyValue("--text").trim() || null;
  const p = c === "transparent" ? null : parseCss(resolveFill(c, dark));
  return p ? `rgb(${p.r} ${p.g} ${p.b} / ${p.a})` : null;
}

/** Builds the card (the head, the body and the tags) with the document `doc`. `id` is the key of the panel instance. */
export function buildCard(doc, id) {
  const part = (cls) => Object.assign(doc.createElement("div"), { className: `annot__${cls}` });
  const head = part("head");
  const body = part("body");
  const tags = part("tags");
  const card = part("card");
  card.setAttribute("role", "tooltip");
  card.setAttribute("data-testid", `annot-${id}-card`);
  card.hidden = true;
  card.append(head, body, tags);
  return { card, head, body, tags };
}

/** Writes the texts of a mark in the card. Each text is a text node, and a tag is a chip: no element comes from data. */
export function fillCard(doc, parts, text) {
  parts.head.textContent = text.head;
  parts.body.textContent = text.body;
  parts.tags.replaceChildren(...text.tags.map((t) => Object.assign(doc.createElement("span"), { className: "annot__tag", textContent: t })));
}

/** Builds the second table of "View data" (9.11): the title "Annotations" and one row for each mark. `rows` come from `tableRows`. */
export function buildTable(doc, rows) {
  const cell = (tag, text) => Object.assign(doc.createElement(tag), { textContent: text });
  const row = (tag, cells) => {
    const tr = doc.createElement("tr");
    tr.append(...cells.map((c) => cell(tag, c)));
    return tr;
  };
  const head = doc.createElement("thead");
  head.append(row("th", ["Time", "End", "Title", "Text", "Tags"]));
  const table = Object.assign(doc.createElement("table"), { className: "dataTable dataTable--compact" });
  const body = doc.createElement("tbody");
  body.append(...rows.map((r) => row("td", r)));
  table.append(head, body);
  const box = Object.assign(doc.createElement("div"), { className: "annot__table" });
  box.setAttribute("data-testid", "view-data-annotations");
  box.append(Object.assign(doc.createElement("h4"), { textContent: "Annotations" }), table);
  return box;
}

/**
 * Adds the marks to a time series that is drawn in `host`. `ctx.annotationsOf()` gives the records of the annotations that apply to the
 * panel. The chart calls `paint` after each draw (the engine option `onDraw`). `update` draws again after new data. Returns the handle.
 */
export function attach(host, chart, ctx) {
  const doc = host.ownerDocument;
  const plotEl = host.querySelector(".chartCore__plot");
  const id = ctx.key;
  const fmt = (ms) => formatTime(ms, ctx.zone);
  const canvas = Object.assign(doc.createElement("canvas"), { className: "annot__marks" });
  canvas.setAttribute("aria-hidden", "true");
  const parts = buildCard(doc, id);
  const keys = Object.assign(doc.createElement("div"), { className: "annot__keys", tabIndex: 0 });
  keys.setAttribute("role", "group");
  keys.setAttribute("data-testid", `annot-${id}-keys`);
  const status = Object.assign(doc.createElement("div"), { className: "visually-hidden" });
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  status.setAttribute("data-testid", `annot-${id}-status`);
  plotEl.append(canvas, parts.card, keys, status);
  let shown = []; // the marks in the visible range at the last draw
  let plot = null;
  let xOf = null;
  let active = -1; // the mark of the keyboard
  let open = null; // the mark of the card

  const hide = () => {
    open = null;
    parts.card.hidden = true;
  };
  const mute = () => {
    chart.hideTooltip();
    requestAnimationFrame(() => open && chart.hideTooltip());
  };
  // The card of a mark at the pointer `px` (px of the plot). The engine tooltip hides, also after its own frame, so one card shows at a time.
  const show = (m, px) => {
    open = m;
    const text = cardText(m, fmt);
    fillCard(doc, parts, text);
    parts.card.hidden = false;
    mute();
    const at = cardPlace(plot, px, parts.card.offsetWidth, parts.card.offsetHeight);
    parts.card.style.transform = `translate(${Math.round(at.x)}px, ${Math.round(at.y)}px)`;
    return text;
  };

  function paint() {
    const L = chart.layout();
    if (!L) return;
    const dark = isDarkTheme();
    const info = marks(ctx.annotationsOf(), L.xLo, L.xHi);
    shown = info.list;
    plot = { left: L.left, top: L.top, w: L.plotW, h: L.plotH };
    xOf = L.xOf;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(L.width * dpr);
    canvas.height = Math.round(L.height * dpr);
    canvas.style.width = `${L.width}px`;
    canvas.style.height = `${L.height}px`;
    const c = canvas.getContext("2d");
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    const inks = new Map();
    const inkOf = (v) => {
      if (!inks.has(v)) inks.set(v, ink(host, v, dark));
      return inks.get(v);
    };
    const right = plot.left + plot.w;
    const bottom = plot.top + plot.h;
    const line = (x) => {
      if (x < plot.left || x > right) return;
      c.beginPath();
      c.moveTo(x, plot.top);
      c.lineTo(x, bottom);
      c.stroke();
    };
    c.lineWidth = LINE;
    c.setLineDash(DASH);
    for (const m of shown) {
      const color = inkOf(m.color);
      if (!color) continue;
      const [a, b] = span(m, xOf);
      c.strokeStyle = color;
      if (m.end !== null) {
        c.globalAlpha = FILL;
        c.fillStyle = color;
        c.fillRect(Math.max(plot.left, a), plot.top, Math.max(0, Math.min(right, b) - Math.max(plot.left, a)), plot.h);
        c.globalAlpha = 1;
        line(b);
      }
      line(a);
    }
    c.setLineDash([]);
    for (const m of shown) {
      const color = inkOf(m.color);
      if (!color) continue;
      const [a, b] = span(m, xOf);
      c.fillStyle = color;
      if (m.end === null) {
        if (a < plot.left || a > right) continue;
        c.beginPath();
        c.moveTo(a - TRIANGLE.w / 2, bottom);
        c.lineTo(a + TRIANGLE.w / 2, bottom);
        c.lineTo(a, bottom - TRIANGLE.h);
        c.fill();
      } else {
        const x0 = Math.max(plot.left, a);
        c.fillRect(x0, bottom - TRIANGLE.h, Math.max(1, Math.min(right, b) - x0), TRIANGLE.h);
      }
    }
    keys.setAttribute("aria-label", `Annotations, ${shown.length}`);
    host.dataset.annotations = JSON.stringify(hookOf(info, shown, xOf, plot, inkOf));
    // A draw that removes the mark of the card hides the card.
    if (open && !shown.some((m) => m.t === open.t && m.index === open.index && m.title === open.title)) hide();
    if (active >= shown.length) active = -1;
  }

  // The pointer moves in the zone: the card shows for the mark under it. A pointer outside the zone hides the card.
  const onMove = (ev) => {
    if (!plot || !shown.length) return;
    const r = plotEl.getBoundingClientRect();
    const px = ev.clientX - r.left;
    const m = hitTest(shown, xOf, plot, px, ev.clientY - r.top);
    if (!m) hide();
    else if (m !== open) show(m, px);
    else mute();
  };
  const onKey = (ev) => {
    if (ev.key === "Escape") {
      hide();
      return;
    }
    if (ev.target !== keys) return;
    const next = stepKey(shown.length, active, ev.key);
    if (next < 0) return;
    ev.preventDefault();
    active = next;
    const m = shown[next];
    const text = show(m, span(m, xOf)[0]);
    status.textContent = `${text.head}. ${text.body}`;
  };
  plotEl.addEventListener("pointermove", onMove);
  plotEl.addEventListener("pointerleave", hide);
  plotEl.addEventListener("keydown", onKey);
  window.addEventListener("scroll", hide, true);
  return {
    paint,
    // New data: the card hides and the marks draw again (9.19d, rule 13).
    update() {
      hide();
      paint();
    },
    destroy() {
      window.removeEventListener("scroll", hide, true);
      canvas.remove();
      parts.card.remove();
      keys.remove();
      status.remove();
    },
  };
}

/** The rows for the table of "View data" of a panel: all the marks that apply, in the order of the time. */
export const allRows = (records, zone) => tableRows(marks(records, -Infinity, Infinity).list, (ms) => formatTime(ms, zone));
