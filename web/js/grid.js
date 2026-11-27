// Sections and the grid (9.8): a CSS grid of 24 columns, a row unit of 30 px and a gap of 8 px.
// The panels load when they come near the viewport (IntersectionObserver, rootMargin "100% 0px").
// A panel or a section that repeats (4.13a) gives more than one item: the plan of repeat.js has the copies (9.8c).

import { GRID_COLUMNS, bandOf, buildPlan, displayedRows, instanceKey, keyFromText, mentionsVariable, sectionSignature } from "./repeat.js";
import { formatRangeText } from "./time.js";
import { clear, el, icon } from "./ui.js";

// The panels of a section in reading order: by y, then x, then id (the order of the scheduler, 9.8). The grid numbers
// place each panel, so the order changes nothing on a wide screen. It gives the order of the stacked layout of a narrow
// screen and the order of the tab key.
export function readingOrder(panels) {
  return [...panels].sort((a, b) => a.grid.y - b.grid.y || a.grid.x - b.grid.x || a.id - b.id);
}

const place = (node, g) => {
  node.style.gridColumn = `${g.x + 1} / span ${g.w}`;
  node.style.gridRow = `${g.y + 1} / span ${g.h}`;
};

// makeFrame(panel, instance) -> frame ({ el, update, destroy, ... }). rt is the runtime. `instance` is the instance of the plan (repeat.js): its key,
// its displayed place and the value of its copy. A caller that has no repeat can ignore it.
// options.sectionExtras(index, section) gives nodes for the section header (the editor, 10.2). The copies n of a section, from 2, have none.
export function createGrid({ doc, rt, makeFrame, sectionExtras = null }) {
  const root = el("div", { class: "dash" });
  const frames = new Map(); // the key of an instance -> its frame
  const layoutEls = new Map(); // the key of an instance -> the element that the section grid places: the frame, or the band that holds it
  const copies = new Map(); // the key of a section copy -> { sectionEl, gridEl, toggle, titleEl, setCount, section, copy }
  const groups = []; // for each section of the document: { anchor, sig, keys, copyKeys, nodes }
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        const key = keyFromText(entry.target.dataset.panelKey);
        if (entry.isIntersecting && frames.has(key) && frames.get(key).warm) frames.get(key).warm();
        rt.setVisible(key, entry.isIntersecting);
      }
    },
    // The root is the scroll container under the header (9.6), so the margin of one window height counts from its edges.
    { root: document.getElementById("app-main"), rootMargin: "100% 0px" },
  );
  // A runtime with no plan (the history, 10.11) shows the panels as stored.
  const planNow = () => rt.plan || buildPlan(doc, () => undefined, () => "", { repeat: false });
  let watching = true;
  let suspended = false; // true while the page shows another view: a frame that sync makes then waits (panel_frame.js, suspend)

  // Print only (dashboard.css): the title, the time range and the variable values as text. The header and the
  // variables bar are not printed, and a printed page without them does not say what it shows.
  const printMeta = el("p", { class: "dash__print-meta", "aria-hidden": "true" });
  root.append(el("p", { class: "dash__print-title", "aria-hidden": "true", text: doc.title || "" }), printMeta);
  const writePrintMeta = () => {
    // The draft runtime of the editor has no range or no selections: the print line is for the viewer only.
    if (!rt.range || !rt.variables || !rt.selections || typeof rt.selections.get !== "function") return;
    const parts = [formatRangeText(rt.range.fromText, rt.range.toText, rt.zone)];
    for (const v of rt.variables) {
      if (v.hide === "variable") continue;
      const sel = rt.selections.get(v.name);
      if (!sel) continue;
      const text = sel.all ? "All" : (sel.values || []).join(", ");
      parts.push(`${v.label || v.name}: ${text || "none"}`);
    }
    const text = parts.join(" | ");
    if (printMeta.textContent !== text) printMeta.textContent = text;
  };
  writePrintMeta();
  const stopMeta = rt.on(writePrintMeta);
  window.addEventListener("beforeprint", writePrintMeta);

  // The title of a section shows after the replacement of the variables (9.13). In a copy of a repeated section the repeat variable has the value of
  // the copy (9.8c). The draft runtime of the editor has no `interpolate`.
  const sectionTitle = (section, copy) => {
    if (typeof rt.interpolateWith === "function") return rt.interpolateWith(section.title, copy.variable && copy.value !== null ? { variable: copy.variable, value: copy.value, text: copy.text } : null);
    return typeof rt.interpolate === "function" ? rt.interpolate(section.title) : section.title;
  };

  // The accessible name of a section copy: the title, and the option text after a comma when the title has no placeholder of the repeat variable (9.26).
  const sectionName = (section, copy) => {
    const name = section.title === null ? "Panels" : sectionTitle(section, copy);
    return copy.variable && copy.text !== null && !mentionsVariable(section.title, copy.variable) ? `${name}, ${copy.text}` : name;
  };

  // The section element of one copy of a section.
  function buildCopy(sp, copy) {
    const index = sp.index;
    const section = doc.sections[index];
    const suffix = copy.n <= 1 ? "" : `-r${copy.n}`;
    const expanded = () => rt.expanded.get(copy.key) !== false;
    const gridEl = el("div", { class: "dash__grid", id: `section-grid-${index}${suffix}`, hidden: !expanded() });
    const count = section.panels.length;
    const countEl = el("span", { class: "section__count muted" });
    let headerEl = null;
    let toggle = null;
    let titleEl = null;
    const setCount = () => {
      countEl.textContent = !expanded() ? `(${count} panel${count === 1 ? "" : "s"})` : "";
    };
    if (section.title !== null) {
      titleEl = el("span", { class: "section__title", text: sectionTitle(section, copy) });
      toggle = el("button", { type: "button", class: "section__toggle", "aria-expanded": String(expanded()), "aria-controls": gridEl.id }, icon(expanded() ? "chevron-down" : "chevron-right", 14), titleEl, countEl);
      toggle.addEventListener("click", () => setExpanded(copy.key, !expanded()));
      headerEl = el("h2", { class: "section__head" }, toggle);
      if (sectionExtras && copy.n <= 1) for (const node of sectionExtras(index, section)) headerEl.append(node);
    }
    const sectionEl = el("section", { class: "section", dataset: { testid: copy.testid } }, ...(headerEl ? [headerEl] : []), gridEl);
    sectionEl.setAttribute("aria-label", sectionName(section, copy));
    setCount();
    copies.set(copy.key, { sectionEl, gridEl, toggle, titleEl, setCount, section, copy });
    return { sectionEl, gridEl };
  }

  // The frame of one instance. `parent` is the element that holds it.
  function addFrame(inst, parent, group) {
    const frame = makeFrame(inst.panel, inst);
    frames.set(inst.key, frame);
    if (suspended) frame.suspend();
    group.keys.push(inst.key);
    frame.el.dataset.panelId = String(inst.id);
    frame.el.dataset.panelKey = String(inst.key);
    parent.append(frame.el);
    if (watching) observer.observe(frame.el);
    return frame;
  }

  // The nodes of one section of the document: one section element for each copy (9.8c). It returns the section elements.
  function buildGroup(sp, group) {
    const nodes = [];
    for (const copy of sp.copies) {
      const { sectionEl, gridEl } = buildCopy(sp, copy);
      group.copyKeys.push(copy.key);
      nodes.push(sectionEl);
      if (copy.note) {
        gridEl.append(el("p", { class: "section__note", role: "note", dataset: { testid: `section-${sp.index}-note` }, text: copy.note.message }));
        continue;
      }
      for (const item of copy.items) {
        if (item.kind === "band") {
          const band = el("div", { class: "repeat-band", dataset: { testid: `repeat-band-${item.id}` } });
          band.style.gridColumn = "1 / span 24";
          band.style.gridRow = `${item.y + 1} / span ${item.rows}`;
          band.style.gridTemplateColumns = `repeat(${item.c}, minmax(0, 1fr))`;
          gridEl.append(band);
          for (const inst of item.instances) {
            const frame = addFrame(inst, band, group);
            frame.el.style.gridColumn = "";
            frame.el.style.gridRow = `span ${inst.grid.h}`;
            frame.el.dataset.span = item.c >= 2 ? "half" : "full";
            layoutEls.set(inst.key, band);
          }
          continue;
        }
        const frame = addFrame(item.instance, gridEl, group);
        place(frame.el, item.instance.grid);
        layoutEls.set(item.instance.key, frame.el);
      }
    }
    return nodes;
  }

  function updateFrames(group) {
    for (const key of group.keys) frames.get(key).update();
  }

  const plan = planNow();
  plan.sections.forEach((sp) => {
    const group = { anchor: document.createComment("section"), sig: sectionSignature(sp), keys: [], copyKeys: [], nodes: [] };
    group.nodes = buildGroup(sp, group);
    groups.push(group);
    root.append(...group.nodes, group.anchor);
    updateFrames(group);
  });

  // A new plan (a new list of values, 9.8c): a section whose plan changed is drawn again. The other sections stay.
  function sync() {
    const next = rt.plan;
    if (!next) return;
    next.sections.forEach((sp, i) => {
      const group = groups[i];
      const sig = sectionSignature(sp);
      if (!group || sig === group.sig) return;
      for (const key of group.keys) {
        const frame = frames.get(key);
        if (frame) {
          observer.unobserve(frame.el);
          frame.destroy();
        }
        frames.delete(key);
        layoutEls.delete(key);
      }
      for (const key of group.copyKeys) copies.delete(key);
      for (const node of group.nodes) node.remove();
      group.keys = [];
      group.copyKeys = [];
      group.sig = sig;
      group.nodes = buildGroup(sp, group);
      group.anchor.before(...group.nodes);
      updateFrames(group);
    });
  }

  // --- the place of a panel that repeats, for the drag of the editor (10.5a) ---------------------------------------------------------------

  const entryOf = (id) => (rt.plan && rt.plan.entries.panels.get(id)) || null;

  // The displayed first row of each panel of a layout, with the moves of 9.8c. It returns null when the plan has no repeat.
  function displayOf(panels) {
    if (!rt.plan || !rt.plan.entries.panels.size) return null;
    return displayedRows(panels.map((p) => ({ id: p.id, grid: p.grid, entry: entryOf(p.id) })));
  }

  // The box of the region that a panel takes in its section grid: the band of `h`, the copies of `v`, or the panel itself.
  function boxOf(id, g, display) {
    const entry = entryOf(id);
    const y = display && display.has(id) ? display.get(id) : g.y;
    if (!entry) return { x: g.x, y, w: g.w, h: g.h };
    const ok = entry.state === "ok";
    if (entry.direction === "h") return { x: 0, y, w: GRID_COLUMNS, h: ok ? bandOf(entry.k, entry.maxPerRow, g.h).rows : g.h };
    return { x: g.x, y, w: g.w, h: ok ? g.h * entry.k : g.h };
  }

  // Puts the element of a panel and of its copies in the grid of its section.
  function placeNode(id, g, display) {
    const el1 = layoutEls.get(id);
    if (!el1) return;
    const entry = entryOf(id);
    const box = boxOf(id, g, display);
    if (entry && entry.state === "ok" && entry.direction === "v") {
      for (let n = 1; n <= entry.k; n++) {
        const node = layoutEls.get(instanceKey(id, n));
        if (node) place(node, { x: g.x, y: box.y + (n - 1) * g.h, w: g.w, h: g.h });
      }
      return;
    }
    place(el1, box);
    if (entry && entry.state === "ok" && entry.direction === "h") for (const child of el1.children) child.style.gridRow = `span ${g.h}`;
  }

  // In view mode a collapse changes the page only, not the document and not the URL (9.8). Each copy of a section has its own state.
  function setExpanded(key, expanded) {
    const s = copies.get(key);
    if (!s || !s.toggle) return;
    rt.setExpanded(key, expanded);
    s.gridEl.hidden = !expanded;
    s.toggle.setAttribute("aria-expanded", String(expanded));
    s.toggle.replaceChild(icon(expanded ? "chevron-down" : "chevron-right", 14), s.toggle.firstChild);
    s.setCount();
    // The charts of the section draw in this task, not at the next resize callback (10.2a, rule 17).
    if (expanded) document.dispatchEvent(new Event("hune-shown"));
  }

  // The texts that hold variables: the section titles and the titles of the panels. The page calls it when a selection changes.
  function paintTexts() {
    for (const s of copies.values()) {
      if (!s.toggle) continue;
      const title = sectionTitle(s.section, s.copy);
      if (s.titleEl && s.titleEl.textContent !== title) s.titleEl.textContent = title;
      s.sectionEl.setAttribute("aria-label", sectionName(s.section, s.copy));
    }
    for (const f of frames.values()) if (typeof f.paintTexts === "function") f.paintTexts();
  }

  return {
    el: root,
    frames,
    layoutEls,
    displayOf,
    boxOf,
    placeNode,
    directionOf: (id) => (entryOf(id) ? entryOf(id).direction : null),
    sync,
    setExpanded,
    paintTexts,
    // The frames of this grid wait while the page shows the view of one panel or the editor, and run again after (9.8).
    suspendFrames() {
      suspended = true;
      for (const f of frames.values()) f.suspend();
    },
    resumeFrames() {
      suspended = false;
      for (const f of frames.values()) f.resume();
    },
    // The editor draws its own grid: the observer of this one stops and starts again.
    pauseObserver() {
      watching = false;
      observer.disconnect();
    },
    resumeObserver() {
      watching = true;
      for (const f of frames.values()) observer.observe(f.el);
    },
    hide(hidden) {
      root.hidden = hidden;
    },
    destroy() {
      stopMeta();
      window.removeEventListener("beforeprint", writePrintMeta);
      observer.disconnect();
      for (const f of frames.values()) f.destroy();
      clear(root);
    },
  };
}
