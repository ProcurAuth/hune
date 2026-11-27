// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B410 and B411 (9.6, 9.8): the reads of the first route start with the session request.
// B410: while the answer of `GET api/session` is held, the page has already sent `GET api/dashboards/<uid>` (one request) and has the answer. The
//       first paint stays held and the page shows nothing. After the release the dashboard shows, and the page sent no second request.
// B411: the same hold with a 401 as the answer of the dashboard. The page of the 401 does not show while the session is held. After the release it
//       shows one time. The list route starts `GET api/folders` and `GET api/dashboards` in the same way (B410 checks it with the list).
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
];
