// Layout normalization (10.5, FE-16). Pure module: no DOM access.
// A panel has grid = { x, y, w, h }. Functions return new arrays and never change their input.

export function overlaps(a, b) {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

// The order of 10.5: by y, then x, then id.
export function orderPanels(panels) {
  return [...panels].sort((p, q) => p.grid.y - q.grid.y || p.grid.x - q.grid.x || p.id - q.id);
}

// The bottom of a section: the first free row unit under all panels.
export function bottomOf(panels) {
  return panels.reduce((m, p) => Math.max(m, p.grid.y + p.grid.h), 0);
}

/**
 * Vertical compaction (10.5). The panels come back in their input order, each with its new `y`.
 * 1. start at the current y; 2. while y > 0 and the rectangle at y - 1 overlaps no placed panel, y - 1;
 * 3. while the panel overlaps a placed panel P, y = P.y + P.h; 4. place it.
 */
export function normalizeSection(panels) {
  const placed = [];
  const ys = new Map();
  for (const p of orderPanels(panels)) {
    const { x, w, h } = p.grid;
    let y = p.grid.y;
    const hits = (at) => placed.find((q) => overlaps({ x, y: at, w, h }, q.grid));
    while (y > 0 && !hits(y - 1)) y -= 1;
    for (let hit = hits(y); hit; hit = hits(y)) y = hit.grid.y + hit.grid.h;
    ys.set(p.id, y);
    placed.push({ id: p.id, grid: { x, y, w, h } });
  }
  return panels.map((p) => (ys.get(p.id) === p.grid.y ? p : { ...p, grid: { ...p.grid, y: ys.get(p.id) } }));
}

// True when "Move up" (dir = -1) or "Move down" (dir = 1) is possible for the panel.
export function canMove(panels, id, dir) {
  const order = orderPanels(panels);
  const at = order.findIndex((p) => p.id === id);
  return at >= 0 && at + dir >= 0 && at + dir < order.length;
}

// The panel exchanges x and y with the previous or the next panel, then the section is normalized (10.5).
// It returns null when the move is not possible.
export function movePanel(panels, id, dir) {
  if (!canMove(panels, id, dir)) return null;
  const order = orderPanels(panels);
  const at = order.findIndex((p) => p.id === id);
  const a = order[at];
  const b = order[at + dir];
  const swapped = panels.map((p) => {
    if (p.id === a.id) return { ...p, grid: { ...p.grid, x: b.grid.x, y: b.grid.y } };
    if (p.id === b.id) return { ...p, grid: { ...p.grid, x: a.grid.x, y: a.grid.y } };
    return p;
  });
  return normalizeSection(swapped);
}

// A change of "Position": set the grid of one panel, then normalize.
export function setPosition(panels, id, grid) {
  return normalizeSection(panels.map((p) => (p.id === id ? { ...p, grid: { ...p.grid, ...grid } } : p)));
}

// The range check of 4.14 for the fields of "Position". It returns a list of texts.
export function positionErrors(grid) {
  const errors = [];
  const int = (name, v, lo, hi) => {
    if (!Number.isInteger(v) || v < lo || v > hi) errors.push(`${name} must be a whole number from ${lo} to ${hi}.`);
  };
  int("x", grid.x, 0, 23);
  int("y", grid.y, 0, 10000);
  int("w", grid.w, 1, 24);
  int("h", grid.h, 1, 200);
  if (Number.isInteger(grid.x) && Number.isInteger(grid.w) && grid.x + grid.w > 24) errors.push("x + w must be 24 or less.");
  return errors;
}

// --- Drag and resize on the grid (10.16) --------------------------------------------------------------------------------------
// The functions below are the model of a drag and of a resize. The page gives a target cell, and they give back the layout
// of the section and the cell of the panel after the compaction (the cell of the placeholder). They change `x`, `y`, `w` and
// `h` only, and they use the ranges of 4.14. The rules come from measures of Grafana 12.4.2 (gap F15 of milestone F):
// the layout of a drag depends on the path of the pointer, so the page gives each new cell to `dragPanel` with the layout
// that the last call returned.

export const GRID_COLUMNS = 24;
export const MAX_ROW = 10000;
export const MAX_HEIGHT = 200;

const clamp = (v, lo, hi) => Math.min(Math.max(v, lo), hi);

// The target cell of a move, kept in the section: 0 <= x <= 24 - w and 0 <= y <= 10000. A value that is not a number
// gives the current value.
export function clampMove(grid, x, y) {
  const fix = (v, fallback) => (Number.isFinite(v) ? Math.round(v) : fallback);
  return { x: clamp(fix(x, grid.x), 0, GRID_COLUMNS - grid.w), y: clamp(fix(y, grid.y), 0, MAX_ROW) };
}

// The target size of a resize: 1 <= w <= 24 - x and 1 <= h <= 200.
export function clampSize(grid, w, h) {
  const fix = (v, fallback) => (Number.isFinite(v) ? Math.round(v) : fallback);
  return { w: clamp(fix(w, grid.w), 1, GRID_COLUMNS - grid.x), h: clamp(fix(h, grid.h), 1, MAX_HEIGHT) };
}

// A work copy of the panels: one flat object for each panel, in the input order.
const toItems = (panels) => panels.map((p) => ({ id: p.id, x: p.grid.x, y: p.grid.y, w: p.grid.w, h: p.grid.h, moved: false }));

// Sorted by row, then column. The sort is stable: two panels at the same cell keep the input order.
const byRowColumn = (items) => [...items].sort((a, b) => a.y - b.y || a.x - b.x);

const firstMeeting = (items, rect) => items.find((o) => o.id !== rect.id && overlaps(o, rect));

// Compaction: the panels go up one by one, in the order by row and column, until a placed panel stops them. A panel that
// meets a placed panel goes below it, and the later panels that meet it go below it too.
function lowerAfter(sorted, item, to) {
  item.y += 1;
  for (let i = sorted.findIndex((o) => o.id === item.id) + 1; i < sorted.length; i++) {
    const other = sorted[i];
    if (other.y > item.y + item.h) break;
    if (other.id !== item.id && overlaps(item, other)) lowerAfter(sorted, other, to + item.h);
  }
  item.y = to;
}

function compactItems(items) {
  const sorted = byRowColumn(items);
  const placed = [];
  for (const item of sorted) {
    item.y = Math.min(placed.reduce((m, q) => Math.max(m, q.y + q.h), 0), item.y);
    while (item.y > 0 && !firstMeeting(placed, item)) item.y -= 1;
    for (let hit = firstMeeting(placed, item); hit; hit = firstMeeting(placed, item)) lowerAfter(sorted, item, hit.y + hit.h);
    item.y = Math.max(item.y, 0);
    item.moved = false;
    placed.push(item);
  }
}

// Move one panel to the column `x` (when given) and the row `y`, then move the panels that it meets, the nearest first.
// `byUser` is true for the panel that the user drags.
function moveItem(items, item, x, y, byUser) {
  if (item.x === x && item.y === y) return;
  const before = item.y;
  if (x !== undefined) item.x = x;
  item.y = y;
  item.moved = true;
  let order = byRowColumn(items);
  if (before >= y) order = order.reverse();
  for (const other of order) {
    if (other.id === item.id || !overlaps(item, other) || other.moved) continue;
    pushAway(items, item, other, byUser);
  }
}

// `other` leaves the place of `item`. When the user drags `item` and there is room above `other`, `other` goes there
// (the two panels swap). Else `other` goes below `item`.
function pushAway(items, item, other, byUser) {
  if (byUser) {
    const room = { id: "room", x: other.x, y: Math.max(item.y - other.h, 0), w: other.w, h: other.h };
    const block = firstMeeting(items, room);
    if (!block) {
      moveItem(items, other, undefined, room.y, false);
      return;
    }
    if (block.y + block.h > item.y) {
      moveItem(items, other, undefined, item.y + 1, false);
      return;
    }
  }
  moveItem(items, other, undefined, other.y + 1, false);
}

const gridOf = (it) => ({ x: it.x, y: it.y, w: it.w, h: it.h });

function build(panels, items, id) {
  const byId = new Map(items.map((it) => [it.id, it]));
  const out = panels.map((p) => {
    const it = byId.get(p.id);
    return it.x === p.grid.x && it.y === p.grid.y && it.w === p.grid.w && it.h === p.grid.h ? p : { ...p, grid: { ...p.grid, ...gridOf(it) } };
  });
  return { panels: out, grid: gridOf(byId.get(id)) };
}

/**
 * One step of a drag: the panel `id` goes to the cell (x, y). The target is kept in the section. The panels that the
 * moved panel meets give way, and the section is compacted upwards (the gravity of the grid). The page calls it for each
 * new cell, with the panels that the last call returned. It returns { panels, grid }: the new panels in the input order,
 * and the cell of the moved panel after the compaction. It returns null when the id is not in the panels.
 */
export function dragPanel(panels, id, x, y) {
  const items = toItems(panels);
  const item = items.find((it) => it.id === id);
  if (!item) return null;
  const to = clampMove(item, x, y);
  moveItem(items, item, to.x, to.y, true);
  compactItems(items);
  return build(panels, items, id);
}

/**
 * One step of a resize: the panel `id` gets w columns and h rows. The size is kept in the section. The panels that the new
 * size meets go below it. It returns { panels, grid }, or null when the id is not in the panels.
 */
export function resizePanel(panels, id, w, h) {
  const items = toItems(panels);
  const item = items.find((it) => it.id === id);
  if (!item) return null;
  const size = clampSize(item, w, h);
  item.w = size.w;
  item.h = size.h;
  compactItems(items);
  return build(panels, items, id);
}
