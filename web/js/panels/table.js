// The table panel (9.20).

import { darkenSpin, fillOf, gradientFrom, resolveFill, thresholdColor } from "../colors.js";
import { grafanaValue } from "../frame.js";
import { makeDisplay } from "../display.js";
import { buildLink, numericOf } from "../links.js";
import { footerAllowed, footerLayout, footerName, distinctTexts, initialSort, inspectText, INSPECT_CAP, legibleColor, rowOrder, subsetField } from "../table_model.js";
import { reduceField } from "../reduce.js";
import { barGradient, thresholdStops } from "./bargauge.js";
import { clear, copyText, el, icon, notify, openDialog, openMenu, uniqueId } from "../ui.js";
import { table as chTable } from "../vendor/chdash/app_ui_table.js";
import { calcUnit, cellInk } from "./items.js";
import { floorUnit, innerSize } from "./box.js";

const ROW_PX = 36;
const HEAD_PX = 34; // the header row (Grafana parity, R2); a body row is ROW_PX
const VIRTUAL_FROM = 500;
const OVERSCAN = 20;
const MIN_COL = 150; // the least width of an automatic column (Grafana parity, R4; the spec of 9.20 said 80)
const DISTINCT_CAP = 5000;
const POPUP_W = 248;
const POPUP_GAP = 4;

// The text of a `json` cell (9.20): a text that is valid JSON gives its compact form, else the text stays as it is.
export function jsonCellText(text) {
  if (typeof text !== "string") return text;
  try {
    return JSON.stringify(JSON.parse(text));
  } catch {
    return text;
  }
}

const GAUGE_CELL = 12; // px of an LCD cell with its gap (10 + 2)

/**
 * The look of a `gauge` cell (pure): the filled part (0 to 1) of the scale `min` to `max`. A null value, and a scale with `max` not
 * above `min`, give an empty bar.
 */
export function gaugeFill(value, min, max) {
  if (typeof value !== "number" || Number.isNaN(value) || !(Number.isFinite(min) && Number.isFinite(max) && max > min)) return 0;
  return Math.max(0, Math.min(1, (value - min) / (max - min)));
}

/** The drawer "Inspect value" of a cell (9.20). The text goes in with textContent only. The copy keeps the drawer open. */
export function openInspect(name, value) {
  const { text, cut } = inspectText(value);
  const copy = () => {
    copyText(text).then((ok) => notify({ level: "info", text: ok ? "The value is in the clipboard." : "The browser did not copy the value." }));
    return false;
  };
  return openDialog({
    title: "Inspect value",
    subtitle: name,
    drawer: true,
    testid: "inspect-value",
    buttons: [{ label: "Copy to Clipboard", onClick: copy }, { label: "Close" }],
    build(body) {
      if (cut) body.append(el("p", { class: "inspect__meta", text: `The value has more than ${INSPECT_CAP.toLocaleString("en-US")} characters. The drawer shows the start.` }));
      body.append(el("pre", { class: "inspect__value", tabIndex: 0, role: "region", "aria-label": `Value of ${name}`, dataset: { testid: "inspect-value-text" }, text }));
    },
  });
}

export function render(host, ctx) {
  const { panel, fields, names, zone } = ctx;
  const showHeader = panel.options.show_header !== false;
  const count = fields[0] ? fields[0].values.length : 0;
  const displays = fields.map((f) => makeDisplay(f, { zone }));
  const textCache = fields.map(() => new Array(count));
  const textAt = (fi, row) => {
    const cache = textCache[fi];
    if (cache[row] === undefined) cache[row] = displays[fi](row).text;
    return cache[row];
  };
  const shown = fields.map((f, i) => i).filter((i) => !fields[i].config.hidden);
  // For the tokens of 9.18: the first time field, and the ref of the frame.
  const timeField = fields.find((f) => f.kind === "time");
  const seriesName = (fields.find((f) => f.ref) || {}).ref || "A";
  let sort = initialSort(panel.options.sort, names);
  const filters = new Map(); // field index -> Set of allowed display texts
  let order = [];
  let filterPopup = null;
  // The text colour of a "color_text" cell, made legible on the panel of the current theme (see legibleColor).
  const inkCache = new Map();
  let surface = null;
  const inkOf = (color) => {
    let out = inkCache.get(color);
    if (out === undefined) {
      if (!surface) surface = getComputedStyle(scroll);
      out = legibleColor(color, surface.backgroundColor, surface.color);
      inkCache.set(color, out);
    }
    return out;
  };

  const scroll = el("div", { class: "vtable__scroll", tabIndex: 0, role: "region", "aria-label": `${panel.title || "Table"}, table`, dataset: { testid: `table-${panel.id}` } });
  const wrap = panel.options.wrap === true;
  const tableEl = el("table", { class: `dataTable vtable${wrap ? " vtable--wrap" : ""}`, "aria-rowcount": String(count + 1) });
  const colgroup = el("colgroup");
  const thead = el("thead");
  const tbody = el("tbody");
  const tfoot = el("tfoot", { class: "vtable__foot" });
  tableEl.append(colgroup);
  if (showHeader) tableEl.append(thead);
  tableEl.append(tbody);
  scroll.append(tableEl);
  host.append(scroll);

  const isNumeric = (f) => f.kind === "number" || f.kind === "number_text";
  const alignOf = (f) => {
    const a = f.config.align;
    return a === "auto" ? (isNumeric(f) ? "right" : "left") : a;
  };

  // --- widths -----------------------------------------------------------------------------------------

  // The columns as Grafana lays them out (R4): a field with a width keeps it; the other columns share the rest of the width of
  // the view in equal parts, with MIN_COL at least. The text of the cells does not change a width. When the sum is wider
  // than the view, the table scrolls on the x axis. The width of the view is the width without the vertical scroll bar.
  function layoutColumns() {
    // The real width of the view, not the rounded clientWidth: a rounded-up width gives a scroll bar of less than 1 px (9.19a).
    const avail = innerSize(scroll).w;
    const fixed = shown.reduce((sum, i) => sum + (fields[i].config.width || 0), 0);
    const autos = shown.filter((i) => !fields[i].config.width).length;
    const share = autos ? Math.max(MIN_COL, floorUnit((avail - fixed) / autos)) : 0;
    clear(colgroup);
    let total = 0;
    for (const i of shown) {
      const w = fields[i].config.width || share;
      total += w;
      colgroup.append(el("col", { style: { width: `${w}px` } }));
    }
    tableEl.style.width = `${total}px`;
    // The width of the view: the message "no row" keeps this width when the table is wider and scrolls.
    scroll.style.setProperty("--vtable-view", `${avail}px`);
  }

  // --- header -----------------------------------------------------------------------------------------

  function sortDir(index) {
    const s = sort.find((k) => k.index === index);
    return s ? (s.desc ? "desc" : "asc") : "";
  }

  function onSort(index) {
    const now = sort.length === 1 && sort[0].index === index ? sort[0] : null;
    if (!now) sort = [{ index, desc: false }];
    else if (!now.desc) sort = [{ index, desc: true }];
    else sort = [];
    recompute();
  }

  // The header is built once. A sort or a filter changes only its marks, so the focus stays on the button that the
  // keyboard user pressed.
  const headCells = new Map(); // field index -> { th, filterButton }

  function renderHeader() {
    clear(thead);
    headCells.clear();
    const tr = el("tr");
    for (const i of shown) {
      const f = fields[i];
      const th = el("th", { scope: "col", dataset: { field: names[i], align: alignOf(f) } });
      th.append(el("span", { class: "vtable__label", text: names[i] }));
      chTable.sortHeader(th, { key: String(i), dir: sortDir(i), onSort: () => onSort(i), title: `Sort by ${names[i]}` });
      th.style.textAlign = alignOf(f);
      let filterButton = null;
      if (f.config.filterable) {
        th.classList.add("has-filter");
        filterButton = el("button", { type: "button", class: "vtable__filter", "aria-label": `Filter ${names[i]}`, "aria-pressed": "false", "aria-haspopup": "dialog", "aria-expanded": "false", dataset: { testid: `filter-${panel.id}-${i}` } }, icon("filter", 16));
        filterButton.addEventListener("click", (e) => {
          e.stopPropagation();
          openFilter(i, filterButton);
        });
        th.append(filterButton);
      }
      headCells.set(i, { th, filterButton });
      tr.append(th);
    }
    thead.append(tr);
    markHeader();
  }

  function markHeader() {
    for (const [i, { th, filterButton }] of headCells) {
      chTable.setSort(th, sortDir(i));
      if (!filterButton) continue;
      const filtered = filters.has(i);
      filterButton.classList.toggle("is-filtered", filtered);
      filterButton.setAttribute("aria-pressed", String(filtered));
      filterButton.setAttribute("aria-label", `Filter ${names[i]}${filtered ? " (filtered)" : ""}`);
    }
  }

  // --- column filter ----------------------------------------------------------------------------------

  function closeFilter(restore = true) {
    if (!filterPopup) return;
    const { node, anchor, onDown, onMove } = filterPopup;
    node.remove();
    document.removeEventListener("pointerdown", onDown, true);
    window.removeEventListener("resize", onMove);
    scroll.removeEventListener("scroll", onMove);
    filterPopup = null;
    anchor.setAttribute("aria-expanded", "false");
    if (restore) anchor.focus();
  }

  function openFilter(index, anchor) {
    const same = filterPopup && filterPopup.anchor === anchor;
    closeFilter(false);
    if (same) {
      anchor.focus();
      return;
    }
    const all = distinctTexts(count, index, textAt);
    const limited = all.slice(0, DISTINCT_CAP);
    const allowed = new Set(filters.has(index) ? filters.get(index) : all);
    const searchId = uniqueId("filter-search");
    const search = el("input", { type: "search", class: "input", id: searchId, "aria-label": `Search the values of ${names[index]}`, placeholder: "Search" });
    const list = el("ul", { class: "filter-list" });
    const summary = el("span", { class: "filter-summary", "aria-live": "polite" });
    const boxes = [];
    const updateSummary = () => {
      summary.textContent = `${allowed.size} of ${all.length} selected`;
    };
    for (const text of limited) {
      const box = el("input", { type: "checkbox", checked: allowed.has(text) });
      box.addEventListener("change", () => {
        if (box.checked) allowed.add(text);
        else allowed.delete(text);
        updateSummary();
      });
      const label = el("label", { class: "check filter-item" }, box, el("span", { text: text === "" ? "(empty)" : text }));
      boxes.push({ text, box, node: label });
      list.append(el("li", {}, label));
    }
    updateSummary();
    search.addEventListener("input", () => {
      const needle = search.value.toLowerCase();
      for (const b of boxes) b.node.parentElement.hidden = needle !== "" && !b.text.toLowerCase().includes(needle);
    });
    const visibleBoxes = () => boxes.filter((b) => !b.node.parentElement.hidden);
    const node = el("div", { class: "filter-popup", role: "dialog", "aria-label": `Filter ${names[index]}`, on: { keydown: (e) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeFilter(); } } } },
      search, list,
      summary,
      el("div", { class: "filter-actions" },
        el("button", { type: "button", class: "btn btn--sm", text: "Select all", on: { click: () => { visibleBoxes().forEach((b) => { b.box.checked = true; allowed.add(b.text); }); updateSummary(); } } }),
        el("button", { type: "button", class: "btn btn--sm", text: "Clear", on: { click: () => { visibleBoxes().forEach((b) => { b.box.checked = false; allowed.delete(b.text); }); updateSummary(); } } }),
        el("button", { type: "button", class: "btn btn--sm btn--primary", text: "Apply", on: { click: () => {
          // Texts beyond the cap stay allowed.
          for (const t of all.slice(DISTINCT_CAP)) allowed.add(t);
          if (all.every((t) => allowed.has(t))) filters.delete(index);
          else filters.set(index, new Set(allowed));
          closeFilter(true);
          recompute();
        } } })));
    const onDown = (event) => {
      if (!node.contains(event.target) && !anchor.contains(event.target)) closeFilter(false);
    };
    // The popup is fixed to the window: it closes when its column moves.
    const onMove = () => closeFilter(false);
    document.addEventListener("pointerdown", onDown, true);
    window.addEventListener("resize", onMove);
    scroll.addEventListener("scroll", onMove);
    node.style.visibility = "hidden";
    document.body.append(node);
    const box = anchor.getBoundingClientRect();
    const height = node.offsetHeight;
    const width = Math.min(POPUP_W, window.innerWidth - 2 * POPUP_GAP);
    const below = box.bottom + POPUP_GAP;
    const top = below + height <= window.innerHeight - POPUP_GAP ? below : Math.max(POPUP_GAP, box.top - POPUP_GAP - height);
    // The popup lines its right edge with the button, as the button is at the right of its header cell.
    const left = Math.max(POPUP_GAP, Math.min(box.right - width, window.innerWidth - width - POPUP_GAP));
    node.style.top = `${top}px`;
    node.style.left = `${left}px`;
    node.style.visibility = "";
    filterPopup = { node, anchor, onDown, onMove };
    anchor.setAttribute("aria-expanded", "true");
    search.focus();
  }

  // --- rows -------------------------------------------------------------------------------------------

  // The `gauge` cell (9.20): a bar over the width of the cell and the display text over it. The text has the ink of the page; a copy in the
  // ink of the fill is clipped to the filled part, so that each letter is readable on the colour under it.
  function gaugeCell(td, f, d, content, dark, fill) {
    const cfg = f.config;
    const cell = cfg.cell;
    const min = Number.isFinite(cfg.min) ? cfg.min : NaN;
    const max = Number.isFinite(cfg.max) ? cfg.max : NaN;
    const part = gaugeFill(d.numeric, min, max);
    const box = el("div", { class: "cell-gauge", dataset: { mode: cell.mode }, role: "meter", "aria-valuemin": String(min), "aria-valuemax": String(max),
      "aria-valuenow": d.numeric === null ? null : String(d.numeric), "aria-valuetext": d.text });
    const bar = el("div", { class: "cell-gauge__bar" });
    let ends = [fill || "var(--text)"];
    if (cell.mode === "lcd") {
      bar.classList.add("cell-gauge__bar--lcd");
      const cells = Math.max(1, Math.floor((tdWidth(f) + 2) / GAUGE_CELL));
      const colors = [];
      for (let k = 0; k < cells; k++) {
        const at = k / cells;
        const lit = part > 0 && at <= part;
        const c = cellColorAt(cfg, d, fill, min + at * (max - min), min, max, dark);
        const span = el("span", { class: `cell-gauge__cell${lit ? " is-lit" : ""}` });
        span.style.setProperty("--cell", c);
        if (lit) colors.push(c);
        bar.append(span);
      }
      ends = colors.length ? colors : ends;
    } else if (part > 0) {
      const fillEl = el("div", { class: "cell-gauge__fill" });
      fillEl.style.width = `${(part * 100).toFixed(3)}%`;
      const stops = cell.mode === "gradient" && !(d.mapped && d.color) ? thresholdStops(cfg, min, max, dark) : null;
      const gradient = stops ? barGradient(cfg, min, max, part * 100, "to right", dark) : null;
      if (gradient) {
        fillEl.style.backgroundImage = gradient;
        ends = stops.filter(([, pos]) => pos < part * 100 || pos === 0).map(([c]) => c);
      } else fillEl.style.background = fill || "var(--text)";
      bar.append(fillEl);
    }
    const text = el("span", { class: "cell-gauge__text" }, content);
    box.append(bar, text);
    if (part > 0 && fill) {
      const ink = cellInk(ends[0], ends);
      const on = el("span", { class: "cell-gauge__text cell-gauge__text--on", "aria-hidden": "true", text: d.text });
      on.style.color = ink;
      on.style.clipPath = `inset(0 ${((1 - part) * 100).toFixed(3)}% 0 0)`;
      box.append(on);
    }
    td.append(box);
  }

  // The width (px) of the column of the field, for the number of cells of an LCD bar.
  function tdWidth(f) {
    const i = fields.indexOf(f);
    const col = colgroup.children[shown.indexOf(i)];
    return col ? parseFloat(col.style.width) - 2 * 6 - 1 : 120;
  }

  function cellColorAt(cfg, d, fill, at, min, max, dark) {
    if (d.mapped && d.color) return fill;
    if (cfg.color && cfg.color.mode === "thresholds") {
      const c = thresholdColor(cfg.thresholds, at, min, max);
      return c ? resolveFill(c, dark) : "var(--text)";
    }
    return fill || "var(--text)";
  }

  function cellFor(fi, row) {
    const f = fields[fi];
    const d = displays[fi](row);
    const td = el("td", { class: isNumeric(f) ? "num" : "", title: d.text });
    td.style.textAlign = alignOf(f);
    const cell = f.config.cell || { type: "auto", mode: null };
    let text = d.text;
    // The light theme draws a named colour with its own shade (colors.js NAMED_LIGHT, as Grafana); the document keeps the token.
    const dark = document.documentElement.dataset.theme !== "light";
    const fill = d.color ? fillOf(d, dark) : null;
    if (cell.type === "json") {
      text = d.empty || d.mapped ? d.text : jsonCellText(typeof d.raw === "string" ? d.raw : d.text);
      td.title = text;
      td.classList.add("cell--json");
    }
    if (cell.type === "color_text" && fill) {
      td.style.color = inkOf(fill);
      td.classList.add("cell--text");
    }
    if (cell.type === "color_background" && fill) {
      const gradient = cell.mode === "gradient";
      td.style.background = gradient ? gradientFrom(fill, dark) : fill;
      td.style.color = cellInk(fill, gradient ? [fill, darkenSpin(fill, dark) || fill] : [fill]);
      td.classList.add("cell--bg");
    }
    let content = document.createTextNode(text);
    const links = f.config.links || [];
    if (links.length) {
      const lctx = {
        ...ctx.linkContext(),
        rawValue: d.raw,
        displayText: d.text,
        fieldName: names[fi],
        valueTime: timeField ? grafanaValue(timeField, row) : null,
        valueNumeric: numericOf(f, grafanaValue(f, row)),
        seriesName,
        rowValue: (name) => {
          const at = names.indexOf(name) >= 0 ? names.indexOf(name) : fields.findIndex((x) => x.name === name);
          return at >= 0 ? String(grafanaValue(fields[at], row)) : undefined;
        },
      };
      const built = links.map((l) => buildLink(l, lctx));
      const good = built.filter((b) => b.href !== null);
      for (const b of built) if (b.href === null) console.warn(`The link "${b.title}" of the field ${names[fi]} has a URL that is not permitted.`);
      if (good.length === 1) {
        const a = el("a", { class: "cell-link", text, dataset: { external: "true" } });
        a.href = good[0].href;
        if (good[0].target_blank) {
          a.target = "_blank";
          a.rel = "noopener noreferrer";
        }
        content = a;
      } else if (good.length > 1) {
        const b = el("button", { type: "button", class: "cell-link", text, "aria-haspopup": "menu", "aria-expanded": "false" });
        b.addEventListener("click", () => openMenu(b, good.map((g) => ({ label: g.title, href: g.href, blank: g.target_blank, external: true }))));
        content = b;
      }
    }
    if (cell.type === "gauge") {
      td.classList.add("cell--gauge");
      gaugeCell(td, f, d, content, dark, fill);
    } else td.append(content);
    // The button "Inspect value" (9.20): the CSS shows it on hover and on focus.
    if ((f.config.set || {}).inspect) td.append(el("button", { type: "button", class: `cell-inspect${alignOf(f) === "right" ? " cell-inspect--left" : ""}`, "aria-label": "Inspect value", dataset: { field: fi } }, icon("eye", 16)));
    return td;
  }

  function rowEl(row, position) {
    const tr = el("tr", { tabIndex: -1, dataset: { row: String(row) }, "aria-rowindex": String(position + 2) });
    for (const fi of shown) tr.append(cellFor(fi, row));
    return tr;
  }

  let range = { first: -1, last: -1 };
  // The height of each row of `order` (36 px, or the measured height of a wrapped row), as a prefix sum: row k starts at prefix[k].
  let extra = new Float32Array(0);
  let prefix = new Float64Array(1);
  function rebuildPrefix() {
    prefix = new Float64Array(order.length + 1);
    for (let k = 0; k < order.length; k++) prefix[k + 1] = prefix[k] + ROW_PX + extra[k];
  }
  // The last row that starts at `y` or above.
  function rowAt(y) {
    let lo = 0;
    let hi = order.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (prefix[mid] <= y) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  }
  let topSpacer = null;
  let bottomSpacer = null;

  function paintRows(force = false) {
    const virtual = order.length >= VIRTUAL_FROM;
    let first = 0;
    let last = order.length - 1;
    if (virtual) {
      const view = scroll.clientHeight || 400;
      const top = Math.max(0, scroll.scrollTop - (showHeader ? HEAD_PX : 0));
      first = Math.max(0, rowAt(top) - OVERSCAN);
      last = Math.min(order.length - 1, rowAt(top + view) + OVERSCAN);
      if (!force && first >= range.first && last <= range.last && range.first >= 0 && Math.abs(first - range.first) < OVERSCAN / 2) return;
    }
    range = { first, last };
    // The focused row (and cell) stays focused when the window of rows changes.
    const active = document.activeElement;
    let keep = null;
    if (active && tbody.contains(active)) {
      const tr = active.closest("tr[data-row]");
      if (tr) keep = { row: tr.dataset.row, cell: active === tr ? -1 : Array.prototype.indexOf.call(tr.children, active) };
    }
    const frag = document.createDocumentFragment();
    topSpacer = null;
    bottomSpacer = null;
    if (virtual && first > 0) frag.append((topSpacer = spacer(prefix[first])));
    for (let k = first; k <= last; k++) frag.append(rowEl(order[k], k));
    if (virtual && last < order.length - 1) frag.append((bottomSpacer = spacer(prefix[order.length] - prefix[last + 1])));
    tbody.replaceChildren(frag);
    if (virtual && wrap) measureRows(first, last);
    if (keep) {
      const tr = tbody.querySelector(`tr[data-row="${keep.row}"]`);
      const target = tr ? (keep.cell >= 0 ? tr.children[keep.cell] : tr) : null;
      if (target) {
        if (!target.hasAttribute("tabindex")) target.tabIndex = -1;
        target.focus({ preventScroll: true });
      }
    }
    roving.refresh();
    if (!order.length) tbody.replaceChildren(noRows());
  }

  // Wrapped rows (9.20): from 500 rows the page measures the rows that it draws, and counts 36 px for the others.
  function measureRows(first, last) {
    let changed = false;
    const rows = tbody.querySelectorAll("tr[data-row]");
    rows.forEach((tr, i) => {
      const k = first + i;
      const delta = tr.offsetHeight - ROW_PX;
      if (Math.abs(delta - extra[k]) > 0.5) {
        extra[k] = delta;
        changed = true;
      }
    });
    if (!changed) return;
    rebuildPrefix();
    if (topSpacer) setSpacer(topSpacer, prefix[first]);
    if (bottomSpacer) setSpacer(bottomSpacer, prefix[order.length] - prefix[last + 1]);
  }

  // The state "no row" inside the table: the query gave no row, or the column filters hide every row.
  function noRows() {
    const filtered = filters.size > 0;
    const inner = el("div", { class: "vtable__none-inner" },
      el("span", { class: "vtable__none-title", text: filtered ? "No row matches the filters" : "No data" }),
      filtered
        ? el("button", { type: "button", class: "link-button", text: "Clear the filters", on: { click: () => { filters.clear(); recompute(); scroll.focus(); } } })
        : el("span", { class: "vtable__none-text", text: "The query returned no row." }));
    return el("tr", { class: "vtable__empty" }, el("td", { colspan: String(Math.max(1, shown.length)), class: "vtable__none" }, inner));
  }

  function setSpacer(tr, height) {
    tr.style.height = `${height}px`;
    tr.firstChild.style.height = `${height}px`;
  }

  function spacer(height) {
    const tr = el("tr", { class: "vtable__spacer", "aria-hidden": "true" }, el("td", { colspan: String(Math.max(1, shown.length)) }));
    setSpacer(tr, height);
    tr.firstChild.style.padding = "0";
    tr.firstChild.style.border = "0";
    return tr;
  }

  // --- footer -------------------------------------------------------------------------------------------

  // The text of a footer line: the calculation over the rows that stay after the column filters, in the unit, the decimals and the
  // mappings of the field. A count has no unit, and `diff_percent` has the unit percent. A text column shows only some calculations.
  function footerText(fi, calc) {
    const field = fields[fi];
    if (!footerAllowed(field.kind, calc)) return "";
    const r = reduceField(subsetField(field, order), calc);
    const unit = calcUnit(calc);
    const shownField = unit ? { ...field, config: { ...field.config, unit } } : field;
    const exact = field.kind === "number_text" && typeof r.value === "string" ? r.value : null;
    return makeDisplay(shownField, { zone }).valueDisplay(exact !== null ? r.num : r.value, exact).text;
  }

  function renderFooter() {
    const columns = shown.map((i) => ({ calcs: (fields[i].config.footer && fields[i].config.footer.reducers) || [] }));
    const layout = footerLayout(columns);
    if (!layout.lines) {
      tfoot.remove();
      return;
    }
    if (!tfoot.isConnected) tableEl.append(tfoot);
    const reference = columns.find((c) => c.calcs.length);
    const tr = el("tr");
    shown.forEach((fi, j) => {
      const calcs = columns[j].calcs;
      const td = el("td", { class: isNumeric(fields[fi]) ? "num" : "" });
      for (let l = 0; l < layout.lines; l++) {
        const calc = layout.names ? (j === 0 ? reference.calcs[l] : calcs[l]) : calcs[l];
        const line = el("div", { class: "vtable__foot-line" });
        if (calc && (!layout.names || j === 0)) line.append(el("span", { class: "vtable__foot-name", text: footerName(calc) }));
        if (calc && !(layout.names && j === 0)) line.append(el("span", { class: "vtable__foot-value", text: footerText(fi, calc) }));
        td.append(line);
      }
      tr.append(td);
    });
    tfoot.replaceChildren(tr);
  }

  function recompute() {
    order = rowOrder({
      count,
      fields,
      sort,
      filters: [...filters].map(([index, allowedSet]) => ({ index, allowed: allowedSet })),
      textAt,
    });
    markHeader();
    extra = new Float32Array(order.length);
    rebuildPrefix();
    renderFooter();
    scroll.scrollTop = 0;
    paintRows(true);
  }

  const roving = chTable.rovingRows(tbody, { rows: "tr[data-row]", cells: true, key: (row) => row.dataset.row });
  // The drawer "Inspect value" (9.20): a click on the button of a cell, or Enter on its cell. A decimal gives its exact text.
  tbody.addEventListener("click", (e) => {
    const b = e.target.closest(".cell-inspect");
    if (!b) return;
    const td = b.parentElement;
    const back = document.activeElement === td ? td : b;
    const f = fields[b.dataset.field];
    const row = +td.parentElement.dataset.row;
    // The focus goes back to the button or the cell. The hidden button can take it only after its cell.
    openInspect(names[b.dataset.field], f.kind === "number_text" ? f.values[row] : grafanaValue(f, row)).addEventListener("close", () => {
      td.tabIndex = -1;
      td.focus();
      back.focus();
    });
  });
  tbody.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" || !e.target.matches("td")) return;
    const b = e.target.querySelector(".cell-inspect");
    e.preventDefault();
    if (b) b.click();
    else toggleExpanded(e.target);
  });
  // The whole text of a cell that is cut (as in Grafana 12.4.2, which shows it over the cell): a click on the cell, or Enter on it, puts the text over
  // the cell, wrapped, with the width of the cell and the colours of the cell. A second click, Escape, a scroll or a click elsewhere closes it.
  let expanded = null;
  function closeExpanded(refocus = false) {
    if (!expanded) return;
    const { node, td } = expanded;
    expanded = null;
    node.remove();
    document.removeEventListener("pointerdown", outside, true);
    if (refocus && td.isConnected) td.focus({ preventScroll: true });
  }
  function outside(e) {
    if (expanded && !expanded.node.contains(e.target)) closeExpanded();
  }
  const isCut = (td) => {
    if (td.scrollWidth > td.clientWidth + 1) return true;
    const inner = td.querySelector(".cell-link, .cell-gauge__text");
    return Boolean(inner) && inner.scrollWidth > inner.clientWidth + 1;
  };
  function openExpanded(td) {
    const text = td.title;
    if (!text) return;
    const box = td.getBoundingClientRect();
    const host = scroll.getBoundingClientRect();
    const style = getComputedStyle(td);
    const node = el("div", { class: "cell-expand", role: "group", "aria-label": "Full text of the cell", tabIndex: -1, dataset: { testid: "cell-expand" }, text });
    // The fill of the cell can be a translucent tint (the hover of the row): it lies over the colour of the panel, so the cells below do not show through.
    const tint = style.backgroundColor === "rgba(0, 0, 0, 0)" ? null : style.backgroundColor;
    const solid = tint ? `linear-gradient(${tint}, ${tint}), var(--panel)` : "var(--panel)";
    node.style.cssText = `left:${box.left - host.left + scroll.scrollLeft}px;top:${box.top - host.top + scroll.scrollTop}px;min-width:${box.width}px;max-width:${Math.max(box.width, Math.min(scroll.clientWidth, 560))}px;min-height:${box.height}px;background:${solid};color:${style.color};text-align:${style.textAlign};`;
    // A click on the box closes it, unless the user selects text in it.
    node.addEventListener("click", () => {
      if (String(window.getSelection && window.getSelection()).length === 0) closeExpanded(true);
    });
    scroll.append(node);
    expanded = { node, td };
    document.addEventListener("pointerdown", outside, true);
  }
  function toggleExpanded(td) {
    const same = expanded && expanded.td === td;
    closeExpanded();
    if (!same && isCut(td)) openExpanded(td);
  }
  tbody.addEventListener("click", (e) => {
    const td = e.target.closest("td");
    if (!td || e.target.closest(".cell-inspect, a, button")) return;
    if (String(window.getSelection && window.getSelection()).length > 0) return;
    toggleExpanded(td);
  });
  scroll.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && expanded) {
      e.stopPropagation();
      closeExpanded(true);
    }
  });
  let frame = 0;
  scroll.addEventListener("scroll", () => {
    closeExpanded();
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      paintRows();
    });
  });
  const observer = new ResizeObserver(() => {
    closeExpanded();
    layoutColumns();
    paintRows(true);
  });
  observer.observe(scroll);
  // A change of theme changes the legible colour of the text colours.
  const onTheme = () => {
    inkCache.clear();
    paintRows(true);
  };
  document.addEventListener("hune-theme", onTheme);
  layoutColumns();
  if (showHeader) renderHeader();
  recompute();

  return {
    destroy() {
      closeExpanded();
      observer.disconnect();
      roving.destroy();
      closeFilter(false);
      document.removeEventListener("hune-theme", onTheme);
      if (frame) cancelAnimationFrame(frame);
      scroll.remove();
    },
  };
}
