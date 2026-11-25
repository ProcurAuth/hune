// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B43 and B229 (16.10, 10.16): drag and resize of the panels in edit mode, and what the edit mode leaves on the frames, against the real server.
import { draftOf, enterEdit, exitEdit, makeSeed, saveDialog, waitDraft } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const mouse = (p, type, x, y) => p.send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1 });

// The pixel size of the grid cells and the corner of the grid of the first section.
const metrics = (p) => p.eval(`(() => {
  const grid = document.querySelector('.edit-grid .dash__grid');
  const r = grid.getBoundingClientRect();
  const s = getComputedStyle(grid);
  const gap = parseFloat(s.columnGap);
  return { left: r.left, top: r.top, col: (r.width + gap) / 24, row: parseFloat(s.gridAutoRows) + parseFloat(s.rowGap) };
})()`);

// A drag in small steps, so the page sees the cells one after the other.
async function move(p, from, to, steps = 12) {
  await p.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: from[0], y: from[1] });
  await mouse(p, "mousePressed", ...from);
  for (let i = 1; i <= steps; i++) {
    await mouse(p, "mouseMoved", from[0] + ((to[0] - from[0]) * i) / steps, from[1] + ((to[1] - from[1]) * i) / steps);
    await sleep(30);
  }
  await mouse(p, "mouseMoved", ...to);
}

const gridOf = (d, id) => JSON.stringify(d.document.sections[0].panels.find((x) => x.id === id).grid);
const xOf = (d, id) => d.document.sections[0].panels.find((x) => x.id === id).grid.x;
const wOf = (d, id) => d.document.sections[0].panels.find((x) => x.id === id).grid.w;

export const cases = [
  {
    id: "B43",
    name: "editor: a panel moves by its title bar and changes size by its corner; the draft, Save and Escape follow 10.5",
    async run(t) {
      const uid = "ed43";
      const seed = await makeSeed(t, uid);
      const p = await t.open(t.dash(uid));
      await waitPanelsReady(p, [1, 2, 3]);
      await enterEdit(p);
      const m = await metrics(p);
      const box = await p.eval(`(() => { const r = document.querySelector('[data-testid="panel-3"]').getBoundingClientRect(); return [r.left, r.top]; })()`);
      t.assert(await p.exists(p.tid("panel-3-resize")), "each panel has a resize handle");
      t.eq(await p.eval(`getComputedStyle(document.querySelector('[data-testid="panel-3"] .panel__head')).cursor`), "move", "the title bar shows the cursor move");
      t.eq(await p.eval(`getComputedStyle(document.querySelector('[data-testid="panel-3-resize"]')).cursor`), "se-resize", "the corner shows the cursor se-resize");

      // Escape cancels a drag. The draft is not created.
      const grab = [box[0] + 24, box[1] + 20];
      const left = [grab[0] - 4 * m.col, grab[1]];
      await move(p, grab, left);
      await p.waitSel(".drag-placeholder");
      t.eq(await p.attr(".drag-placeholder", "aria-hidden"), "true", "the placeholder is hidden from a screen reader");
      await p.press("Escape");
      await mouse(p, "mouseReleased", ...left);
      await sleep(400);
      t.assert(!(await p.exists(".drag-placeholder")), "no placeholder after Escape");
      t.eq(await draftOf(p, uid), null, "Escape leaves no draft");

      // Drag panel 3 four columns to the left: it takes x = 12, and panel 1, which it meets, goes below it.
      await move(p, grab, left);
      await p.waitSel(".drag-placeholder");
      t.eq(await p.eval(`document.querySelector('.drag-placeholder').style.gridColumn`), "13 / span 8", "the placeholder is on the cell of the drop");
      t.eq(await p.attr(p.tid("panel-3"), "data-drag"), "move", "the moving panel is marked");
      await mouse(p, "mouseReleased", ...left);
      const moved = await waitDraft(p, uid, (d) => xOf(d, 3) === 12);
      t.eq(gridOf(moved, 3), JSON.stringify({ x: 12, y: 0, w: 8, h: 8 }), "panel 3 after the drop");
      t.eq(gridOf(moved, 1), JSON.stringify({ x: 0, y: 8, w: 16, h: 8 }), "panel 1 gave way below panel 3");
      const strip = (doc) => JSON.stringify(doc.sections.map((s) => s.panels.map(({ grid, ...rest }) => rest)));
      t.eq(strip(moved.document), strip(seed.doc), "nothing but grid changed");
      t.assert(!(await p.eval(`document.querySelector('[data-testid="save-button"]').disabled`)), "Save is active");
      t.assert(!(await p.exists(".drag-placeholder")), "no placeholder after the drop");

      // Resize panel 3 by its corner: two columns and one row more.
      await p.waitSel(p.tid("panel-3-resize"));
      const h = await p.eval(`(() => { const r = document.querySelector('[data-testid="panel-3-resize"]').getBoundingClientRect(); return [r.left + 20, r.top + 20]; })()`);
      const end = [h[0] + 2 * m.col, h[1] + m.row];
      await move(p, h, end);
      await p.waitSel(".drag-placeholder");
      t.eq(await p.eval(`document.querySelector('.drag-placeholder').style.gridColumn`), "13 / span 10", "the placeholder has the new width");
      await mouse(p, "mouseReleased", ...end);
      const resized = await waitDraft(p, uid, (d) => wOf(d, 3) === 10);
      t.eq(gridOf(resized, 3), JSON.stringify({ x: 12, y: 0, w: 10, h: 9 }), "panel 3 after the resize");

      // The arrow keys: Alt + ArrowLeft on the menu button of panel 3 moves it by one column.
      await p.focus(p.tid("panel-3-menu"));
      await p.press("ArrowLeft", { modifiers: 1 });
      await waitDraft(p, uid, (d) => xOf(d, 3) === 11);

      // Save keeps the layout.
      await saveDialog(p, "layout");
      await p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null`, { what: "edit mode left after the save" });
      const saved = await t.api.dashboard(uid);
      const panel = saved.json.document.sections[0].panels.find((x) => x.id === 3);
      t.eq(JSON.stringify(panel.grid), JSON.stringify({ x: 11, y: 0, w: 10, h: 9 }), "the saved document has the new grid");
    },
  },
  {
    id: "B229",
    name: "editor: the resize handles, the drag mark and the key shortcuts of a panel are there in edit mode only: none in the view, one for each panel in edit mode, none after the switch goes off or after Discard in the exit dialog, one for each panel after Revert, and again after a new entry (10.16)",
    async run(t) {
      const uid = "ed229";
      await makeSeed(t, uid);
      const p = await t.open(t.dash(uid));
      await waitPanelsReady(p, [1, 2, 3]);
      // What the edit mode puts on a frame: the handle (`.panel__resize`), the mark of the title bar (`data-drag-handle`) and `aria-keyshortcuts`.
      // The view grid uses the same frames again, so the count covers the whole document.
      const marks = () => p.eval(`(() => {
        const handles = [...document.querySelectorAll(".panel__resize")];
        return {
          handles: handles.length,
          visible: handles.filter((e) => e.getBoundingClientRect().width > 0 && getComputedStyle(e).display !== "none").length,
          dragMarks: document.querySelectorAll("[data-drag-handle]").length,
          keys: document.querySelectorAll("[aria-keyshortcuts]").length,
          editGrid: document.querySelectorAll(".edit-grid").length,
        };
      })()`);
      const NONE = { handles: 0, visible: 0, dragMarks: 0, keys: 0, editGrid: 0 };
      t.eq(await marks(), NONE, "the view has no handle, no drag mark and no key shortcut");

      // Edit mode: one handle, one drag mark and one set of key shortcuts for each panel, and each handle shows.
      await enterEdit(p);
      const frames = await p.count('.edit-grid [data-testid^="panel-"][data-state]');
      t.assert(frames >= 3, `the draft grid has the panels: ${frames}`);
      t.eq(await marks(), { handles: frames, visible: frames, dragMarks: frames, keys: frames, editGrid: 1 }, "edit mode: one handle, one drag mark and one set of keys for each panel");

      // The switch goes off with no change: the edit mode ends, and nothing stays on the frames of the view.
      await exitEdit(p);
      await p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null && document.querySelector(".edit-grid") === null`, { what: "the edit mode ends" });
      await p.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-state]').length >= 3`, { what: "the view frames" });
      t.eq(await marks(), NONE, "after the switch goes off: no handle, no drag mark and no key shortcut");
      t.eq(await p.eval(`document.querySelectorAll('[data-testid^="panel-"][data-state] [data-drag-handle], [data-testid^="panel-"][data-state][aria-keyshortcuts]').length`), 0, "the frames of the view have neither the drag mark nor the key shortcuts");

      // A new entry gives them again, one for each panel (no handle is made twice).
      await enterEdit(p);
      t.eq(await marks(), { handles: frames, visible: frames, dragMarks: frames, keys: frames, editGrid: 1 }, "a second entry: again one handle for each panel");

      // A change, then the switch off and "Discard" in the exit dialog: the edit mode ends, and nothing stays.
      const change = async () => {
        await p.focus(p.tid("panel-3-menu"));
        await p.press("ArrowLeft", { modifiers: 1 });
        await p.waitFor(`!document.querySelector('[data-testid="discard-button"]').disabled`, { what: "the draft has a change" });
      };
      await change();
      await exitEdit(p);
      await p.waitSel(p.tid("exit-dialog"));
      await p.clickText(`${p.tid("exit-dialog")} button`, "Discard");
      await p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null && document.querySelector(".edit-grid") === null`, { what: "the edit mode ends after Discard" });
      await p.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-state]').length >= 3`, { what: "the view frames" });
      t.eq(await marks(), NONE, "after Discard in the exit dialog: no handle, no drag mark and no key shortcut");

      // A third entry, a change, and "Revert all changes": the edit mode goes on, with one handle for each panel and none made twice.
      await enterEdit(p);
      await change();
      await p.click(p.tid("discard-button"));
      await p.clickText("dialog button, .dialog button", "Discard");
      await p.waitFor(`document.querySelector('[data-testid="discard-button"]').disabled`, { what: "the draft is reverted" });
      t.eq(await marks(), { handles: frames, visible: frames, dragMarks: frames, keys: frames, editGrid: 1 }, "after Revert: the edit mode goes on, one handle for each panel");
      await exitEdit(p);
      await p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null && document.querySelector(".edit-grid") === null`, { what: "the edit mode ends after Revert" });
      t.eq(await marks(), NONE, "after Revert and the switch off: no handle, no drag mark and no key shortcut");
    },
  },
];
