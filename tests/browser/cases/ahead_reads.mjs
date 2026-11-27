// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B410 and B411 (9.6, 9.8): the reads of the first route start with the session request.
// B410: while the answer of `GET api/session` is held, the page has already sent `GET api/dashboards/<uid>` (one request) and has the answer. The
//       first paint stays held and the page shows nothing. After the release the dashboard shows, and the page sent no second request.
// B411: the same hold with a 401 as the answer of the dashboard. The page of the 401 does not show while the session is held. After the release it
//       shows one time. The list route starts `GET api/folders` and `GET api/dashboards` in the same way (B410 checks it with the list).
// B412: the module of a panel type loads before the answer of its query (9.11). A panel near the viewport asks for its module while its query is
//       held. A panel far below the viewport, and a panel in a collapsed section, ask for none.
// B413: the editor starts its style sheet, `GET api/sources` and the module of its view at the same time (10.2). While the answer of the sources is
//       held, the page has asked for the style sheet and the module of the panel editor. The first paint stays held, and the editor shows after the release.
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { sleep } from "../lib/h.mjs";

const isSession = (r) => r.method === "GET" && /\/api\/session$/.test(r.url);
const isDoc = (r) => r.method === "GET" && /\/api\/dashboards\/nodes$/.test(r.url);
const isFolders = (r) => r.method === "GET" && /\/api\/folders$/.test(r.url);
const isList = (r) => r.method === "GET" && /\/api\/dashboards(\?[^/]*)?$/.test(r.url);

/** Wait until `pred(page.requests)` is true; the case reads the requests that the page records. */
async function until(p, pred, what) {
  for (let i = 0; i < 300; i += 1) {
    if (pred(p.requests)) return;
    await sleep(50);
  }
  throw new Error(`timeout: ${what}`);
}

const held = (p) => p.eval(`({ hold: document.documentElement.hasAttribute("data-hune-hold"), state: document.documentElement.getAttribute("data-hune-state"), children: document.getElementById("app-main").childElementCount })`);

export const cases = [
  {
    id: "B410",
    name: "the reads of the first route start before the session answer, and the first view still waits for it",
    async run(t) {
      // The dashboard route.
      const p = await t.page();
      const hold = p.holdWhen(isSession);
      await p.send("Page.navigate", { url: t.dash("nodes") });
      await until(p, (rs) => rs.some(isDoc) && rs.some((r) => isDoc(r) && r.status === 200), "the answer of the dashboard while the session is held");
      t.eq(p.requests.filter(isSession).every((r) => r.status === null), true, "the session request is still held");
      const waiting = await held(p);
      t.eq(waiting.hold, true, "the first paint stays held");
      t.eq(waiting.state, "busy", "the state stays busy");
      t.eq(waiting.children, 0, "the page shows nothing before the session answer");
      hold.release();
      await p.waitSel(p.tid("panel-1"));
      await p.idle();
      t.eq(p.requests.filter(isDoc).length, 1, "one request for the dashboard");
      t.eq(p.requests.filter(isSession).length, 1, "one request for the session");
      t.eq((await held(p)).hold, false, "the first paint is released");

      // The list route.
      const l = await t.page();
      const holdList = l.holdWhen(isSession);
      await l.send("Page.navigate", { url: t.url("") });
      await until(l, (rs) => rs.some((r) => isFolders(r) && r.status === 200) && rs.some((r) => isList(r) && r.status === 200), "the answers of the list while the session is held");
      const waitingList = await held(l);
      t.eq(waitingList.hold, true, "the first paint of the list stays held");
      t.eq(waitingList.children, 0, "the list shows nothing before the session answer");
      holdList.release();
      await l.waitSel(`${l.tid("list")} article`);
      await l.idle();
      t.eq(l.requests.filter(isFolders).length, 1, "one request for the folders");
      t.eq(l.requests.filter(isList).length, 1, "one request for the dashboards of the list");
    },
  },
  {
    id: "B411",
    name: "a 401 of the dashboard that came before the session answer shows its page one time, after the session answer",
    async run(t) {
      const p = await t.page();
      t.allow(p, /Failed to load resource/);
      const refuse = p.fulfillWhen(isDoc, {
        status: 401,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ error: { code: "unauthorized", message: "No sign-in.", retryable: false } }),
      });
      const hold = p.holdWhen(isSession);
      await p.send("Page.navigate", { url: t.dash("nodes") });
      await until(p, (rs) => rs.some((r) => isDoc(r) && r.status === 401), "the 401 of the dashboard while the session is held");
      await sleep(200);
      t.eq(await p.exists(p.tid("unauthorized")), false, "no page of the 401 before the session answer");
      t.eq((await held(p)).hold, true, "the first paint stays held");
      hold.release();
      await p.waitSel(p.tid("unauthorized"));
      await p.idle();
      t.eq(await p.count(p.tid("unauthorized")), 1, "the page of the 401 shows one time");
      t.eq((await held(p)).hold, false, "the page of the 401 releases the first paint");
      refuse();
    },
  },
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
