// Drag and resize of the panels in edit mode (10.16). A panel moves when the user drags its title bar, and it changes size when
// the user drags its bottom-right corner. The module shows the preview on the page (the placeholder and the panels that give way)
// and gives the new layout of the section to the editor, which puts it in the draft like the form "Position" (10.5).
// The model of the layout is in layout.js (pure). This module reads and writes the DOM only.

import { el } from "../ui.js";
import { dragPanel, normalizeSection, resizePanel, GRID_COLUMNS, MAX_HEIGHT } from "./layout.js";

// A pointer that moves less than this many pixels is a click on the title, not a drag.
const START_DISTANCE = 4;
// Near the top or the bottom of the window, a drag scrolls the page.
const EDGE = 56;
const MAX_SCROLL = 24;
const FLIP_MS = 200;

// At this width the grid is a flow (9.8), so the panels have no cell to drag to. The form "Position" stays.
export const NARROW_QUERY = "(max-width: 768px)";

const INTERACTIVE = "button, a[href], input, select, textarea, [tabindex], [data-tooltip]";

// The arrow keys of 10.16: Alt + arrow moves a panel by one cell, Alt + Shift + arrow changes its size by one unit.
const ARROWS = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };

const sameGrid = (a, b) => a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;

const place = (node, g) => {
  node.style.gridColumn = `${g.x + 1} / span ${g.w}`;
  node.style.gridRow = `${g.y + 1} / span ${g.h}`;
};

// The sizes of the cells of a grid element in pixels, from its computed style (9.8: 24 columns, 30 px rows, 8 px gap).
export function gridMetrics(gridEl) {
  const style = getComputedStyle(gridEl);
  const gap = parseFloat(style.columnGap) || 0;
  const rowGap = parseFloat(style.rowGap) || gap;
  const unit = parseFloat(style.gridAutoRows) || 30;
  const width = gridEl.getBoundingClientRect().width;
  const col = (width - gap * (GRID_COLUMNS - 1)) / GRID_COLUMNS;
  return { gap, rowGap, colPitch: col + gap, rowPitch: unit + rowGap, rowUnit: unit };
}

// The cell for a position of the top left corner of the moving panel, in pixels from the corner of the grid. The step of the
// cell is one pitch, and the cell turns over one gap later than a plain rounding (the rule of the grid of Grafana, measured).
export function cellAt(left, top, m) {
  return { x: Math.max(0, Math.round((left - m.gap) / m.colPitch)), y: Math.max(0, Math.round((top - m.rowGap) / m.rowPitch)) };
}

// The size in cells for a size of the resized panel in pixels.
export function sizeAt(width, height, m) {
  return { w: Math.round((width + m.gap) / m.colPitch), h: Math.round((height + m.rowGap) / m.rowPitch) };
}

/**
 * Makes the panels of a grid view draggable.
 *  view: the grid of grid.js ({ el, frames }).
 *  panelsOf(sectionIndex): the panels of that section in the draft, as they are now.
 *  commit(sectionIndex, panels, info): puts the panels in the draft. info = { id, kind: "move" | "resize" | "key" }.
 *  announce(text): a text for a live region.
 * It returns { destroy }.
 */
export function attachDrag({ view, panelsOf, commit, announce = () => {} }) {
  const root = view.el;
  const narrow = window.matchMedia(NARROW_QUERY);
  const calm = window.matchMedia("(prefers-reduced-motion: reduce)");
  const cleanups = [];
  const sectionOf = new Map(); // panel id -> { section, gridEl }
  // Only the first copy of a panel moves: a copy n from 2 is a view, and so is a copy in a copy of a section (10.5a, 9.8c). The first copy of a
  // panel with direction `h` is in a band: the grid element is the section grid that holds the band.
  for (const node of root.querySelectorAll(".dash__grid")) {
    const match = /^section-grid-(\d+)$/.exec(node.id);
    if (!match) continue;
    for (const child of node.querySelectorAll("[data-panel-id]")) {
      if (child.dataset.panelKey === child.dataset.panelId) sectionOf.set(Number(child.dataset.panelId), { section: Number(match[1]), gridEl: node });
    }
  }
  // The element that the section grid places for a panel: the frame, or the band that holds it (the first copy of `h`).
  const layoutOf = (id, frame) => (view.layoutEls && view.layoutEls.get(id)) || frame.el;
  // The panel nodes take the displayed place, with the moves of 9.8c (a panel below a repeat goes down by its extra height).
  const placeAll = (id, g, display) => {
    if (view.placeNode) view.placeNode(id, g, display);
    else place(view.frames.get(id).el, g);
  };
  const displayOf = (panels) => (view.displayOf ? view.displayOf(panels) : null);

  let drag = null;

  function begin(kind, event, id) {
    if (drag || narrow.matches) return;
    if (event.pointerType === "mouse" && event.button !== 0) return;
    // A control inside the title bar keeps its own action. An ancestor with a tabindex (the region of the editor, 10.2a) does not count.
    if (kind === "move" && event.target instanceof Element) {
      const control = event.target.closest(INTERACTIVE);
      if (control && event.currentTarget.contains(control)) return;
    }
    const where = sectionOf.get(id);
    const frame = view.frames.get(id);
    if (!where || !frame) return;
    const panels = panelsOf(where.section);
    const me = panels.find((p) => p.id === id);
    if (!me) return;
    const metrics = gridMetrics(where.gridEl);
    const gridRect = where.gridEl.getBoundingClientRect();
    const layout = layoutOf(id, frame);
    const rect = layout.getBoundingClientRect();
    // A panel with direction `h` changes `y` and `h` only: the page keeps x = 0 and w = 24 (10.5a).
    const horizontal = Boolean(view.directionOf && view.directionOf(id) === "h");
    drag = {
      kind, id, frame, layout, horizontal, panels, cur: panels, grid: me.grid, metrics, started: false,
      section: where.section, gridEl: where.gridEl, pointerId: event.pointerId, holder: event.currentTarget,
      // The pointer and the panel in pixels from the corner of the grid. The scroll of the page changes the corner, so every move reads it again.
      sx: event.clientX - gridRect.left, sy: event.clientY - gridRect.top,
      left: rect.left - gridRect.left, top: rect.top - gridRect.top, width: rect.width, height: rect.height,
      clientX: event.clientX, clientY: event.clientY, cell: null, placeholder: null, frameId: 0,
    };
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch (error) {
      // A pointer that cannot be captured still gives its events to the page while it stays over the panel.
    }
    event.currentTarget.addEventListener("pointermove", onMove);
    event.currentTarget.addEventListener("pointerup", onUp);
    event.currentTarget.addEventListener("pointercancel", onCancel);
    event.currentTarget.addEventListener("lostpointercapture", onCancel);
    document.addEventListener("keydown", onEscape, true);
  }

  function start() {
    const d = drag;
    d.started = true;
    const node = d.layout;
    d.placeholder = el("div", { class: "drag-placeholder", "aria-hidden": "true" });
    place(d.placeholder, view.boxOf ? view.boxOf(d.id, d.grid, displayOf(d.panels)) : d.grid);
    d.gridEl.append(d.placeholder);
    node.dataset.drag = d.kind;
    root.dataset.drag = d.kind;
    if (d.kind === "resize") {
      node.style.justifySelf = "start";
      node.style.alignSelf = "start";
      node.style.width = `${d.width}px`;
      node.style.height = `${d.height}px`;
    }
    d.frameId = requestAnimationFrame(scrollLoop);
  }

  // The page scrolls while the pointer is near the top or the bottom edge of the window.
  function scrollLoop() {
    const d = drag;
    if (!d || !d.started) return;
    const h = window.innerHeight;
    const speed = d.clientY < EDGE ? -Math.ceil(((EDGE - d.clientY) / EDGE) * MAX_SCROLL) : d.clientY > h - EDGE ? Math.ceil(((d.clientY - (h - EDGE)) / EDGE) * MAX_SCROLL) : 0;
    if (speed) {
      const before = window.scrollY;
      window.scrollBy(0, speed);
      if (window.scrollY !== before) update();
    }
    d.frameId = requestAnimationFrame(scrollLoop);
  }

  // The pointer moved, or the page scrolled: the panel follows the pointer, and the cell may change.
  function update() {
    const d = drag;
    const gridRect = d.gridEl.getBoundingClientRect();
    const dx = d.clientX - gridRect.left - d.sx;
    const dy = d.clientY - gridRect.top - d.sy;
    const node = d.layout;
    let result = null;
    if (d.kind === "move") {
      node.style.transform = `translate(${dx}px, ${dy}px)`;
      const cell = cellAt(d.left + dx, d.top + dy, d.metrics);
      if (d.horizontal) cell.x = 0;
      const key = `${cell.x},${cell.y}`;
      if (key !== d.cell) {
        d.cell = key;
        result = dragPanel(d.cur, d.id, cell.x, cell.y);
      }
    } else {
      const m = d.metrics;
      const maxWidth = (GRID_COLUMNS - d.grid.x) * m.colPitch - m.gap;
      const width = Math.min(Math.max(d.width + dx, m.colPitch - m.gap), maxWidth);
      const height = Math.min(Math.max(d.height + dy, m.rowPitch - m.rowGap), MAX_HEIGHT * m.rowPitch - m.rowGap);
      node.style.width = `${width}px`;
      node.style.height = `${height}px`;
      const size = sizeAt(width, height, m);
      if (d.horizontal) size.w = GRID_COLUMNS;
      const key = `${size.w}x${size.h}`;
      if (key !== d.cell) {
        d.cell = key;
        result = resizePanel(d.cur, d.id, size.w, size.h);
      }
    }
    if (result) show(result);
  }

  // Shows a layout: the placeholder takes the cell of the moving panel, and the other panels take their new cells with a short slide.
  function show(result) {
    const d = drag;
    const moved = [];
    const display = displayOf(result.panels);
    for (const p of result.panels) {
      const old = d.cur.find((q) => q.id === p.id);
      if (p.id === d.id || sameGrid(old.grid, p.grid)) continue;
      const frame = view.frames.get(p.id);
      if (!frame) continue;
      const node = layoutOf(p.id, frame);
      moved.push({ node, id: p.id, grid: p.grid, from: node.getBoundingClientRect() });
    }
    for (const m of moved) {
      m.node.style.transition = "none";
      m.node.style.transform = "";
    }
    for (const m of moved) placeAll(m.id, m.grid, display);
    // A panel below a region that repeats moves with the extra height of the region, also when its own cell did not change.
    if (display) for (const p of result.panels) if (p.id !== d.id && !moved.some((m) => m.id === p.id) && view.frames.get(p.id)) placeAll(p.id, p.grid, display);
    const box = view.boxOf ? view.boxOf(d.id, result.grid, display) : result.grid;
    place(d.placeholder, box);
    // The moving panel stays in its first cell, and its size is the pointer position (resize): the grid never makes room for it.
    if (d.kind === "resize") place(d.layout, box);
    d.cur = result.panels;
    if (calm.matches) {
      for (const m of moved) m.node.style.transition = "";
      return;
    }
    for (const m of moved) {
      const to = m.node.getBoundingClientRect();
      m.node.style.transform = `translate(${m.from.left - to.left}px, ${m.from.top - to.top}px)`;
    }
    void d.gridEl.offsetWidth;
    for (const m of moved) {
      m.node.style.transition = `transform ${FLIP_MS}ms var(--ease, ease)`;
      m.node.style.transform = "";
      const done = () => {
        m.node.style.transition = "";
        m.node.removeEventListener("transitionend", done);
      };
      m.node.addEventListener("transitionend", done);
    }
  }

  function onMove(event) {
    const d = drag;
    if (!d || event.pointerId !== d.pointerId) return;
    d.clientX = event.clientX;
    d.clientY = event.clientY;
    if (!d.started) {
      const gridRect = d.gridEl.getBoundingClientRect();
      if (Math.hypot(event.clientX - gridRect.left - d.sx, event.clientY - gridRect.top - d.sy) < START_DISTANCE) return;
      start();
    }
    event.preventDefault();
    update();
  }

  // Puts the DOM back as the page built it: no inline style of the drag, no placeholder. The panels that slid go back to their cells.
  function restore(d) {
    cancelAnimationFrame(d.frameId);
    if (d.placeholder) d.placeholder.remove();
    delete root.dataset.drag;
    const display = displayOf(d.panels);
    for (const p of d.panels) {
      const frame = view.frames.get(p.id);
      if (!frame) continue;
      const node = layoutOf(p.id, frame);
      const s = node.style;
      placeAll(p.id, p.grid, display);
      s.transform = "";
      s.transition = "";
      if (p.id === d.id) {
        delete frame.el.dataset.drag;
        delete node.dataset.drag;
        s.width = "";
        s.height = "";
        s.justifySelf = "";
        s.alignSelf = "";
      }
    }
  }

  function release(d) {
    const holder = d.holder;
    holder.removeEventListener("pointermove", onMove);
    holder.removeEventListener("pointerup", onUp);
    holder.removeEventListener("pointercancel", onCancel);
    holder.removeEventListener("lostpointercapture", onCancel);
    document.removeEventListener("keydown", onEscape, true);
    try {
      holder.releasePointerCapture(d.pointerId);
    } catch (error) {
      // The pointer is already free.
    }
    drag = null;
  }

  function onUp(event) {
    const d = drag;
    if (!d || event.pointerId !== d.pointerId) return;
    release(d);
    if (!d.started) return;
    // A click that follows a drag must not open the title as a link or a menu.
    const stopClick = (e) => e.stopPropagation();
    d.holder.addEventListener("click", stopClick, { capture: true, once: true });
    setTimeout(() => d.holder.removeEventListener("click", stopClick, true), 0);
    restore(d);
    const changed = d.cur.some((p, i) => !sameGrid(p.grid, d.panels[i].grid));
    if (!changed) return;
    commit(d.section, normalizeSection(d.cur), { id: d.id, kind: d.kind });
    const now = d.cur.find((p) => p.id === d.id).grid;
    announce(d.kind === "move" ? `Panel moved to column ${now.x + 1}, row ${now.y + 1}.` : `Panel resized to ${now.w} columns and ${now.h} rows.`);
  }

  function onCancel(event) {
    const d = drag;
    if (!d || (event.pointerId !== undefined && event.pointerId !== d.pointerId)) return;
    release(d);
    if (d.started) restore(d);
  }

  function onEscape(event) {
    if (event.key !== "Escape" || !drag) return;
    event.preventDefault();
    event.stopPropagation();
    const d = drag;
    release(d);
    if (d.started) restore(d);
  }

  // The arrow keys: the same model, one step, then the draft.
  function onKey(event) {
    if (!event.altKey || event.ctrlKey || event.metaKey || !ARROWS[event.key] || narrow.matches) return;
    const node = event.target instanceof Element ? event.target.closest(".panel") : null;
    if (!node || event.target.closest("input, textarea, select")) return;
    const id = Number(node.dataset.panelId);
    const where = sectionOf.get(id);
    // A copy n from 2 is a view: the keys of this section do not act on it (10.5a).
    if (!where || node.dataset.panelKey !== node.dataset.panelId) return;
    const panels = panelsOf(where.section);
    const me = panels.find((p) => p.id === id);
    if (!me) return;
    event.preventDefault();
    const [dx, dy] = ARROWS[event.key];
    // A panel with direction `h` keeps x = 0 and w = 24 (10.5a).
    const horizontal = Boolean(view.directionOf && view.directionOf(id) === "h");
    const result = event.shiftKey ? resizePanel(panels, id, horizontal ? GRID_COLUMNS : me.grid.w + dx, me.grid.h + dy) : dragPanel(panels, id, horizontal ? 0 : me.grid.x + dx, me.grid.y + dy);
    if (!result || result.panels.every((p, i) => sameGrid(p.grid, panels[i].grid))) {
      announce("The panel cannot go there.");
      return;
    }
    commit(where.section, normalizeSection(result.panels), { id, kind: "key" });
    const now = result.panels.find((p) => p.id === id).grid;
    announce(event.shiftKey ? `Panel resized to ${now.w} columns and ${now.h} rows.` : `Panel moved to column ${now.x + 1}, row ${now.y + 1}.`);
  }

  for (const id of sectionOf.keys()) {
    const frame = view.frames.get(id);
    if (!frame) continue;
    const head = frame.el.querySelector(".panel__head");
    if (head) {
      head.dataset.dragHandle = "move";
      head.addEventListener("pointerdown", (e) => begin("move", e, id));
    }
    const handle = el("span", { class: "panel__resize", "aria-hidden": "true", dataset: { testid: `panel-${id}-resize` } });
    handle.addEventListener("pointerdown", (e) => begin("resize", e, id));
    frame.el.append(handle);
    frame.el.setAttribute("aria-keyshortcuts", "Alt+ArrowLeft Alt+ArrowRight Alt+ArrowUp Alt+ArrowDown Alt+Shift+ArrowLeft Alt+Shift+ArrowRight Alt+Shift+ArrowUp Alt+Shift+ArrowDown");
  }
  root.addEventListener("keydown", onKey);
  cleanups.push(() => root.removeEventListener("keydown", onKey));

  return {
    destroy() {
      if (drag) {
        const d = drag;
        release(d);
        if (d.started) restore(d);
      }
      for (const off of cleanups) off();
    },
  };
}
