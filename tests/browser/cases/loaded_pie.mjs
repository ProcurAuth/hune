// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B400 (9.19a, 9.29): the loaded pie chart sizes its circle from the real box of its canvas, with the fraction of a pixel.
// The hook `data-geometry` of `.pie__canvas` holds "cx cy r inner" in CSS px. At a width where the box has a fraction, cx and cy must be
// half of the real rect, and r must be min(width, height) / 2 - 8. The old code read the rounded clientWidth and clientHeight.
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const IDS = [1, 2, 3];

function pieDoc() {
  const legend = (placement) => ({ show: true, placement, mode: "list", values: [] });
  return doc("pie400", "Pie 400", [
    section("Pies", [
      panel(1, "piechart", "Right", SQL.bars, { grid: { x: 0, y: 0, w: 7, h: 8 }, options: { legend: legend("right") } }),
      panel(2, "piechart", "Bottom", SQL.bars, { grid: { x: 7, y: 0, w: 7, h: 8 }, options: { legend: legend("bottom"), pie_type: "donut" } }),
      panel(3, "piechart", "Off", SQL.bars, { grid: { x: 14, y: 0, w: 7, h: 8 }, options: { legend: { show: false, placement: "right", mode: "list", values: [] } } }),
    ]),
  ]);
}

const READ = `(() => IDS.map((id) => {
  const c = document.querySelector('[data-testid="panel-' + id + '"] .pie__canvas');
  const r = c.getBoundingClientRect();
  const [cx, cy, radius, inner] = c.dataset.geometry.split(" ").map(Number);
  return { id, w: r.width, h: r.height, cx, cy, radius, inner };
}))()`.replace("IDS", JSON.stringify(IDS));

export const cases = [
  {
    id: "B400",
    name: "the loaded pie sizes its circle from the real box with the fraction of a pixel (9.19a, 9.29)",
    async run(t) {
      const r = await t.api.create("transit", pieDoc());
      t.assert(r.status === 201, `create pie400: ${r.status} ${r.text.slice(0, 300)}`);
      let fractional = 0;
      const wrong = [];
      for (const [width, dsf] of [[1000, 1], [1283, 1], [1183, 1.1], [1011, 1.25], [1366, 0.9]]) {
        const cssW = Math.round(width / dsf);
        const cssH = Math.round(900 / dsf);
        const p = await t.page({ width: cssW, height: cssH });
        await p.send("Emulation.setDeviceMetricsOverride", { width: cssW, height: cssH, deviceScaleFactor: dsf, mobile: false });
        await p.goto(t.dash("pie400"));
        await p.idle();
        await waitPanelsReady(p, IDS);
        await p.eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))");
        for (const m of await p.eval(READ)) {
          const near = Math.abs(m.w - Math.round(m.w)) > 0.1 || Math.abs(m.h - Math.round(m.h)) > 0.1;
          if (near) fractional += 1;
          const want = { cx: m.w / 2, cy: m.h / 2, radius: Math.max(10, Math.min(m.w, m.h) / 2 - 8) };
          for (const k of Object.keys(want)) {
            if (Math.abs(m[k] - want[k]) > 0.02) wrong.push(`${width}@${dsf} panel ${m.id}: ${k} ${m[k]} for the box ${m.w} x ${m.h}, expected ${want[k].toFixed(3)}`);
          }
          const hole = m.id === 2 ? m.radius * 0.55 : 0;
          if (Math.abs(m.inner - hole) > 0.002) wrong.push(`${width}@${dsf} panel ${m.id}: inner ${m.inner}, expected ${hole.toFixed(3)}`);
        }
        await p.close();
      }
      t.assert(fractional > 0, "the case found no box with a fraction: the widths of the case do not test the rule");
      t.eq(wrong, [], "pie geometry against the real box");
    },
  },
];
