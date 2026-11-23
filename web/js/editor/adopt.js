// The frames of the base grid in the draft grid (10.2a, rule 17; decision ID-K301).
// A panel of the draft that is the panel of the base document, with the same data, keeps its frame. The draft grid takes the element of
// the base frame, with its plot, its canvas and its last result. The frame returns to its place in the base grid when the draft grid
// closes. Thus the switch "Edit", the panel editor, "Back" and "Forward" make no new chart and send no query.
// The frame keeps the runtime of the base document: its record has the data that the record of the draft mirrors.

import { frameDataset } from "../panel_frame.js";

/**
 * The adopter of one draft grid.
 * @param {object} o
 * @param {object} o.api the editor API of the page (dashboard.js): `grid`, `runtime` and `pageApi` of the base document
 * @param {object} o.draft the runtime of the draft (draft_runtime.js)
 * @param {object} o.page the page object of the edit mode: the panel menu, the zoom and the marks of a frame in the draft grid
 */
export function frameAdopter({ api, draft, page }) {
  const taken = new Map(); // the key of an instance -> { frame, mark, style, span, menu, head }

  // True when the draft instance shows what the base frame shows: the same stored panel, the same copy and the same data.
  function fits(frame, panel, inst) {
    const own = api.runtime && api.runtime.panels.get(inst.key);
    const rec = draft.panels.get(inst.key);
    if (!frame || typeof frame.setPage !== "function" || !frame.el.parentNode || !own || !rec) return false;
    if (own.variable !== rec.variable || own.value !== rec.value) return false;
    if (JSON.stringify(api.runtime.stored.get(panel.id)) !== JSON.stringify(panel)) return false;
    const want = frameDataset(panel, inst);
    const has = frame.el.dataset;
    if (want.testid !== has.testid || want.repeatN !== has.repeatN || want.repeatCount !== has.repeatCount) return false;
    // A panel with a query mirrors the base record. A text panel has no data. A reuse panel needs a source that mirrors the base.
    const reuse = panel.data && panel.data.reuse;
    const source = reuse ? draft.panels.get(reuse.panel) : null;
    return rec.mirrored || panel.type === "text" || Boolean(source && source.mirrored);
  }

  return {
    /** The base frame for a draft instance, out of the base grid and in the page of the edit mode, or null: then the draft grid makes a frame. */
    take(panel, inst) {
      const frame = api.grid ? api.grid.frames.get(inst.key) : null;
      if (taken.has(inst.key) || !fits(frame, panel, inst)) return null;
      const node = frame.el;
      const menu = node.querySelector(".panel__menu");
      const mark = document.createComment(`frame ${inst.key}`);
      taken.set(inst.key, { frame, mark, style: node.getAttribute("style"), span: node.dataset.span, menu, head: menu ? menu.parentNode : null });
      node.replaceWith(mark);
      // The base grid stops its frames while the editor shows (dashboard.js, pauseBase). This frame shows in the draft grid.
      frame.resume();
      // The page of the edit mode gives the menu of the editor, no zoom and no mark (9.19d, rule 22).
      frame.setPage(page);
      return frame;
    },
    /**
     * Returns each frame to its place in the base grid, with its grid place, its menu and the page of the view. `grid` is the draft grid: its map
     * loses the frames, so that its destroy does not destroy them.
     */
    giveBack(grid) {
      for (const [key, t] of taken) {
        if (grid && grid.frames.get(key) === t.frame) grid.frames.delete(key);
        const node = t.frame.el;
        if (t.style === null) node.removeAttribute("style");
        else node.setAttribute("style", t.style);
        if (t.span === undefined) delete node.dataset.span;
        else node.dataset.span = t.span;
        if (t.menu && t.menu.parentNode !== t.head) t.head.append(t.menu);
        t.mark.replaceWith(node);
        t.frame.setPage(api.pageApi);
        t.frame.suspend();
      }
      taken.clear();
    },
  };
}
