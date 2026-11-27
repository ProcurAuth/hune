// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B412 and B413 (9.11, 10.2): loads that start early.
// B412: the module of a panel type loads before the answer of its query (9.11). A panel near the viewport asks for its module while its query is
//       held. A panel far below the viewport, and a panel in a collapsed section, ask for none.
// B413: the editor starts its style sheet, `GET api/sources` and the module of its view at the same time (10.2). While the answer of the sources is
//       held, the page has asked for the style sheet and the module of the panel editor. The first paint stays held, and the editor shows after the release.
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { sleep } from "../lib/h.mjs";

/** Wait until `pred(page.requests)` is true; the case reads the requests that the page records. */
async function until(p, pred, what) {
  for (let i = 0; i < 300; i += 1) {
    if (pred(p.requests)) return;
    await sleep(50);
  }
  throw new Error(`timeout: ${what}`);
}

const held = (p) => p.eval(`({ hold: document.documentElement.hasAttribute("data-hune-hold") })`);

export const cases = [
  {
    id: "B412",
    name: "the module of a panel type loads while the query is held; a far panel and a collapsed section load none",
    async run(t) {
      const near = panel(1, "timeseries", "Near", SQL.series, { grid: { x: 0, y: 0, w: 24, h: 8 } });
      const far = panel(2, "barchart", "Far", SQL.bars, { grid: { x: 0, y: 0, w: 24, h: 8 } });
      const hidden = panel(3, "piechart", "Hidden", SQL.bars, { grid: { x: 0, y: 0, w: 24, h: 8 }, format: "table" });
      const filler = Array.from({ length: 12 }, (_, i) => panel(10 + i, "stat", `Filler ${i}`, SQL.total, { grid: { x: 0, y: i * 8, w: 24, h: 8 } }));
      const body = doc("ahead412", "Ahead reads", [section(null, [near]), section("Filler", filler), section("Far", [far]), section("Hidden", [hidden], true)]);
      const made = await t.api.create("transit", body);
      t.assert(made.status === 201, `create: ${made.status} ${made.text.slice(0, 300)}`);
      const p = await t.page({ width: 1280, height: 900 });
      const hold = p.holdWhen((r) => /\/panels\/[^/]+\/query/.test(r.url));
      await p.goto(t.dash("ahead412"));
      const module = (name) => (r) => r.method === "GET" && new RegExp(`/js/panels/${name}\\.js$`).test(r.url);
      await until(p, (rs) => rs.some((r) => /\/panels\/1\/query/.test(r.url)), "the query of the near panel (held)");
      await until(p, (rs) => rs.some((r) => module("timeseries")(r) && r.status === 200), "the module of the near panel while its query is held");
      await sleep(300);
      t.eq(p.requests.some(module("barchart")), false, "no module for the panel far below the viewport");
      t.eq(p.requests.some(module("piechart")), false, "no module for the panel of a collapsed section");
      hold.release();
      await p.waitFor(`document.querySelector('[data-testid="panel-1"]').dataset.state === "ready"`, { what: "the near panel", timeout: 30000 });
      t.eq(p.requests.filter(module("timeseries")).length, 1, "one request for the module");
      t.eq(p.requests.some(module("barchart")), false, "still no module for the far panel");
      t.eq(p.requests.some(module("piechart")), false, "still no module for the collapsed section");
    },
  },
  {
    id: "B413",
    name: "the style sheet, the sources and the module of the panel editor load at the same time, and the editor shows after all three",
    async run(t) {
      const p = await t.page({ width: 1280, height: 900 });
      const hold = p.holdWhen((r) => r.method === "GET" && /\/api\/sources$/.test(r.url));
      await p.send("Page.navigate", { url: t.dash("nodes", "refresh=off&edit=1&editPanel=1") });
      const seen = (re) => (rs) => rs.some((r) => r.method === "GET" && re.test(r.url) && r.status === 200);
      await until(p, (rs) => rs.some((r) => /\/api\/sources$/.test(r.url)), "the request of the sources (held)");
      await until(p, seen(/\/css\/editor\.css$/), "the style sheet of the editor while the sources are held");
      await until(p, seen(/\/js\/editor\/panel_editor\.js$/), "the module of the panel editor while the sources are held");
      await sleep(200);
      t.eq(await p.exists(".peditor"), false, "the panel editor is not in the page before the sources answer");
      t.eq((await held(p)).hold, true, "the first paint stays held");
      hold.release();
      await p.waitSel(".peditor");
      await p.idle();
      t.eq(p.requests.filter((r) => /\/api\/sources$/.test(r.url)).length, 1, "one request for the sources");
      t.eq((await held(p)).hold, false, "the first paint is released with the editor in the page");
    },
  },
];
