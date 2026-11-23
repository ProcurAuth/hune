// Data tables, one component for every page (css/10-components/table.css):
// <table class="dataTable [dataTable--compact]">.
//
//   Header      11.5 px / 700, --muted on --thead-bg, sentence case, sticky
//               inside the table's scroll container.
//   Density     --row-regular (32 px, the default) or --row-compact (26 px,
//               .dataTable--compact).
//   Cells       .num: right-aligned tabular figures (no mono); .mono only for
//               ids and hashes; a cut-off value keeps its full text in title.
//   Selection   tr.is-selected: an accent bar on the left plus --row-hover.
//   Row numbers td/th.dataTable__rowNum (results and previews only).
//   Sorting     a button in the th, the state in aria-sort only:
//               sortHeader(th, ...) for DOM tables. The glyph
//               is CSS: up / down when sorted, the idle glyph on hover.
//   Keyboard    rovingRows(container, ...): Up / Down,
//               Page Up / Down, Home / End move between rows, Enter / Space
//               open one.

// hune: E20. The helpers that Hune never calls are removed: the HTML sort header, the cells (bars, shares,
// copy buttons, title text, row heights), the keyboard menu target and the scroll cues (engine-patches.md).
// The imports app_ui_copy, app_util, h and shell go with them.
import { dom } from "./app_dom.js";

const { $, $$ } = dom;

const ARIA = { asc: "ascending", desc: "descending" };
const ariaSort = (dir) => ARIA[dir] || "none";

// ------------------------------------------------------------- sorting

// Marks a header sorted (dir "asc" | "desc") or not (""); the glyph follows.
function setSort(th, dir) {
  if (th) th.setAttribute("aria-sort", ariaSort(dir));
}

// Turns `th` into a sortable header: its content moves into a
// .dataTable__sort button; onSort(key, event) runs on click (Enter / Space
// on the button). Returns the button.
function sortHeader(th, { key, dir = "", onSort = null, title = "" } = {}) {
  if (!th) return null;
  let button = $(":scope > .dataTable__sort", th);
  if (!button) {
    button = document.createElement("button");
    button.type = "button";
    button.className = "dataTable__sort";
    button.append(...th.childNodes);
    th.appendChild(button);
  }
  th.classList.add("is-sortable");
  if (key != null) th.dataset.sortKey = String(key);
  if (title) button.title = title;
  setSort(th, dir);
  if (onSort && !th.dataset.sortBound) {
    th.dataset.sortBound = "1";
    th.addEventListener("click", (event) => {
      // Other controls in the header (an info icon) keep their clicks.
      const control = event.target instanceof Element ? event.target.closest("button, a, [tabindex]") : null;
      if (control && control !== button && th.contains(control)) return;
      onSort(th.dataset.sortKey, event);
    });
  }
  return button;
}

// hune: E20, sortHeadHtml, bindSort and the cell helpers are removed.

// ------------------------------------------------------------ keyboard

const MOVE_KEYS = new Set(["ArrowDown", "ArrowUp", "PageDown", "PageUp", "Home", "End"]);
const OPEN_KEYS = new Set(["Enter", " "]);

// The index a key moves to from `at` among `count` rows (`page` per screen).
function targetIndex(key, at, count, page) {
  if (!count) return -1;
  const from = at < 0 ? (key === "ArrowUp" || key === "End" || key === "PageUp" ? count : -1) : at;
  const step = Math.max(1, page | 0);
  let next = from;
  if (key === "ArrowDown") next = from + 1;
  else if (key === "ArrowUp") next = from - 1;
  else if (key === "PageDown") next = from + step;
  else if (key === "PageUp") next = from - step;
  else if (key === "Home") next = 0;
  else if (key === "End") next = count - 1;
  return Math.max(0, Math.min(count - 1, next));
}

// rovingRows(container, options): row navigation for a table or a list.
//
// DOM mode (rows: a selector, default "tbody tr[tabindex]"): one row is in
// the tab order (tabindex 0, the selected one or the first), the others -1;
// the keys move focus between rows. Rows rendered later are picked up.
//   onMove(row, event)  optional, after focus moved
//   onOpen(row, event)  Enter / Space on a row
//   cells: true         Right / Left step into the row's cells (and back),
//                       Up / Down keep the column: a grid. The focused cell
//                       is what a keyboard context menu acts on.
//   key(row)            a row's identity across re-renders (virtual tables
//                       rebuild their rows): focus comes back to the row
//                       (and cell) with the same key after a re-render.
//
// Index mode (count given: virtual lists whose rows are not all in the DOM;
// the container keeps focus): the keys call onMove(index, event) and
// onOpen(index, event); current() is the index the keys move from, page()
// the rows per screen.
//
// Keys with Alt / Ctrl / Meta and keys typed in a form field are left alone.
// Returns { refresh(), destroy() }.
function rovingRows(container, {
  rows = "tbody tr[tabindex]", onMove = null, onOpen = null, count = null, current = null, page = null,
  selected = ".is-selected", cells = false, key = null,
} = {}) {
  if (!container) return { refresh() {}, destroy() {} };
  const indexMode = typeof count === "function";
  const list = () => [...$$(rows, container)].filter((row) => !row.hidden);
  const rowOf = (el) => {
    const row = el instanceof Element ? el.closest(rows) : null;
    return row && container.contains(row) ? row : null;
  };
  // Row numbers are part of the row, not a cell to stop on.
  const cellsOf = (row) => [...row.children].filter((cell) => cell.matches("td, th") && !cell.hidden && !cell.classList.contains("dataTable__rowNum"));
  // The last focused row (and cell) by key, to restore after a re-render.
  let focusKey = null;
  let focusCell = -1;
  let focusInside = false;

  function setRoving(active) {
    for (const row of list()) {
      const value = row === active ? 0 : -1;
      if (row.tabIndex !== value) row.tabIndex = value;
    }
  }

  function focusEl(el, scroll = true) {
    if (!el) return;
    if (el.tabIndex < 0 && !el.hasAttribute("tabindex")) el.tabIndex = -1;
    el.focus({ preventScroll: !scroll });
    if (scroll) el.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }

  function refresh() {
    if (indexMode) return;
    const all = list();
    if (!all.length) return;
    const active = document.activeElement;
    // A virtual re-render dropped the focused row: focus its new element.
    if (focusInside && typeof key === "function" && focusKey != null && (!active || active === document.body || !container.contains(active))) {
      const again = all.find((row) => String(key(row)) === focusKey);
      if (again) {
        setRoving(again);
        const cell = focusCell >= 0 ? cellsOf(again)[focusCell] : null;
        focusEl(cell || again, false);
        return;
      }
    }
    const current = all.find((row) => row.contains(active))
      || all.find((row) => row.tabIndex === 0 && row.matches(selected))
      || all.find((row) => row.matches(selected))
      || all.find((row) => row.tabIndex === 0)
      || all[0];
    setRoving(current);
  }

  function pageSize(all) {
    if (typeof page === "function") return page();
    const first = all[0];
    const height = first ? first.getBoundingClientRect().height : 0;
    const view = container.clientHeight || window.innerHeight;
    return height > 0 ? Math.max(1, Math.floor(view / height) - 1) : 10;
  }

  function moveRow(row, next, event, cellIndex = -1) {
    if (!next) return;
    event.preventDefault();
    setRoving(next);
    const cell = cellIndex >= 0 ? cellsOf(next)[cellIndex] : null;
    focusEl(cell || next);
    if (next !== row) onMove?.(next, event);
  }

  function onKeydown(event) {
    if (event.altKey || event.ctrlKey || event.metaKey || event.defaultPrevented) return;
    const target = event.target instanceof Element ? event.target : null;
    if (target && target.closest("input, textarea, select, [contenteditable='true']")) return;
    if (indexMode) {
      const n = count();
      if (MOVE_KEYS.has(event.key)) {
        const next = targetIndex(event.key, typeof current === "function" ? current() : -1, n, typeof page === "function" ? page() : 10);
        if (next < 0) return;
        event.preventDefault();
        onMove?.(next, event);
      } else if (OPEN_KEYS.has(event.key) && onOpen) {
        if (target && target !== container && target.closest("button, a")) return;
        const at = typeof current === "function" ? current() : -1;
        if (at < 0) return;
        event.preventDefault();
        onOpen(at, event);
      }
      return;
    }
    const row = rowOf(target);
    if (!row) return;
    const onRow = target === row;
    const cell = !onRow && cells && target.parentElement === row ? target : null;
    // A button or link inside a row keeps its own keys.
    if (!onRow && !cell) return;
    if (cell) {
      const rowCells = cellsOf(row);
      const at = rowCells.indexOf(cell);
      if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
        event.preventDefault();
        const next = rowCells[at + (event.key === "ArrowRight" ? 1 : -1)];
        if (next) focusEl(next);
        else if (event.key === "ArrowLeft") focusEl(row);
        return;
      }
      if (event.key === "Home" || event.key === "End") {
        event.preventDefault();
        focusEl(event.key === "Home" ? rowCells[0] : rowCells[rowCells.length - 1]);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        focusEl(row);
        return;
      }
      if (MOVE_KEYS.has(event.key)) {
        const all = list();
        moveRow(row, all[targetIndex(event.key, all.indexOf(row), all.length, pageSize(all))], event, at);
      }
      return;
    }
    if (cells && event.key === "ArrowRight") {
      const first = cellsOf(row)[0];
      if (first) {
        event.preventDefault();
        focusEl(first);
      }
      return;
    }
    if (MOVE_KEYS.has(event.key)) {
      const all = list();
      moveRow(row, all[targetIndex(event.key, all.indexOf(row), all.length, pageSize(all))], event);
    } else if (OPEN_KEYS.has(event.key) && onOpen) {
      event.preventDefault();
      onOpen(row, event);
    }
  }

  function onFocusin(event) {
    if (indexMode) return;
    const row = rowOf(event.target);
    if (!row) return;
    focusInside = true;
    focusKey = typeof key === "function" ? String(key(row)) : null;
    focusCell = event.target !== row && event.target.parentElement === row ? cellsOf(row).indexOf(event.target) : -1;
    setRoving(row);
  }

  function onFocusout(event) {
    if (indexMode) return;
    const left = event.target;
    // Focus moved elsewhere. This is not a re-render that removes the row. Forget the row.
    setTimeout(() => {
      if (left instanceof Element && left.isConnected && !container.contains(document.activeElement)) focusInside = false;
    }, 0);
  }

  container.addEventListener("keydown", onKeydown);
  container.addEventListener("focusin", onFocusin);
  container.addEventListener("focusout", onFocusout);
  let observer = null;
  if (!indexMode && typeof MutationObserver === "function") {
    let queued = false;
    observer = new MutationObserver(() => {
      if (queued) return;
      queued = true;
      queueMicrotask(() => { queued = false; refresh(); });
    });
    observer.observe(container, { childList: true, subtree: true });
  }
  refresh();
  return {
    refresh,
    destroy() {
      container.removeEventListener("keydown", onKeydown);
      container.removeEventListener("focusin", onFocusin);
      container.removeEventListener("focusout", onFocusout);
      observer?.disconnect();
    },
  };
}

// hune: E20, keyboardMenuTarget and scrollCues are removed.
export const table = Object.freeze({
  ariaSort, setSort, sortHeader,
  rovingRows, targetIndex,
});
