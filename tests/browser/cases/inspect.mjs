// Browser case B46 (16.10, 9.11): the item "Inspect" of the panel menu and the key `server.query_inspect`.
// The case starts two servers of its own. With `query_inspect = true` the menu has "Inspect", and its dialog shows
// the expanded SQL. With `query_inspect = false` the item is absent. All SQL is synthetic.
import { doc, panel, section } from "../lib/docs.mjs";
import { SEED_FROM, SEED_TO, api } from "../lib/env.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const UID = "inspect-test";
const SQL = ["SELECT $__interval_s AS step,", "       41 + 1 AS answer"];

async function create(t, server) {
  const d = doc(UID, "Inspect", [section(null, [panel(1, "table", "Answer", SQL, { grid: { x: 0, y: 0, w: 12, h: 8 } })])]);
  const r = await api(server).create("transit", d);
  t.assert(r.status === 201, `create ${UID}: ${r.status} ${r.text.slice(0, 400)}`);
}

async function openMenu(t, server) {
  const p = await t.open(`${server.url}d/${UID}?from=${SEED_FROM}&to=${SEED_TO}`);
  await waitPanelsReady(p, [1]);
  await p.click(p.tid("panel-1-menu"));
  await p.waitSel(p.tid("panel-1-menu-popup"));
  return p;
}

export const cases = [
  {
    id: "B46",
    name: "inspect: with query_inspect true the menu has Inspect and the dialog shows the SQL; with false the item is absent",
    async run(t) {
      // (a) The key is true.
      const on = await t.ownServer("inspect-on", { queryInspect: true });
      await create(t, on);
      t.eq((await api(on).session()).json.query_inspect, true, "session.query_inspect of the first server");
      const p = await openMenu(t, on);
      t.assert(await p.exists(p.tid("panel-1-view-data")), "the item View data is in the menu");
      t.assert(await p.exists(p.tid("panel-1-inspect")), "the item Inspect is in the menu (query_inspect = true)");
      t.eq(await p.text(p.tid("panel-1-inspect")), "Inspect", "the label of the item");
      await p.click(p.tid("panel-1-inspect"));
      await p.waitSel(p.tid("inspect-1"));
      await p.waitSel(p.tid("inspect-1-sql"));
      const sql = await p.text(p.tid("inspect-1-sql"));
      t.assert(/SELECT\s+\d+\s+AS step,\s+41 \+ 1 AS answer/.test(sql), `the dialog shows the expanded SQL: ${JSON.stringify(sql)}`);
      t.assert(!sql.includes("$__"), `no macro is left in the SQL: ${JSON.stringify(sql)}`);
      t.assert(await p.exists(p.tid("inspect-copy")), "the button Copy SQL (inspect-copy) is in the dialog");
      t.assert(await p.exists(p.tid("inspect-1-meta")), "the dialog has its meta line");
      t.assert(!(await p.exists(p.tid("inspect-1-none"))), "the dialog has no text for a missing SQL");
      await p.press("Escape");
      await p.waitGone(p.tid("inspect-1"));

      // (b) The key is false.
      const off = await t.ownServer("inspect-off", { queryInspect: false });
      await create(t, off);
      t.eq((await api(off).session()).json.query_inspect, false, "session.query_inspect of the second server");
      const q = await openMenu(t, off);
      t.assert(await q.exists(q.tid("panel-1-view-data")), "the item View data is in the menu");
      t.assert(!(await q.exists(q.tid("panel-1-inspect"))), "the item Inspect is absent (query_inspect = false)");
      // The answer of the server has no member `sql` either.
      const r = await api(off).call("POST", `api/dashboards/${UID}/panels/1/query`, {
        body: { etag: (await api(off).dashboard(UID)).etag, from_ms: SEED_FROM, to_ms: SEED_TO, max_data_points: 1200, variables: {} },
      });
      t.assert(r.status === 200, `the query answers: ${r.status} ${r.text.slice(0, 200)}`);
      t.assert(!("sql" in r.json), "the answer of a server with query_inspect = false has no member sql");
    },
  },
];
