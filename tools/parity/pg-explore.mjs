#!/usr/bin/env node
// Helper of pg_vectors.py. It runs PostgreSQL queries in the Explore page of Grafana.
//
// The front end of Grafana replaces the built-in variable $__interval and computes the interval. The
// backend does not. So the vectors with $__interval need the front end. The helper opens one Explore
// pane for each SQL text. It reads the request `POST /api/ds/query` and its answer. It prints one JSON
// line for each SQL text, with the SQL that the front end sent, the interval, the text
// `executedQueryString` and the error.
//
// Use:  node tools/parity/pg-explore.mjs --url URL --uid UID --sqls JSON [--range JSON]
//       The value of --sqls is a JSON array of texts. The value of --range is {"from":"ms","to":"ms"}.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Browser, sleep } from "../../tests/browser/lib/cdp.mjs";

function fail(message) {
  console.error(`pg-explore: ${message}`);
  process.exit(2);
}

const o = { url: "http://127.0.0.1:3010", uid: "pgcheck", sqls: null, range: { from: "1794211200000", to: "1794232800000" } };
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  const next = () => {
    if (i + 1 >= process.argv.length) fail(`${a} needs a value`);
    return process.argv[++i];
  };
  if (a === "--url") o.url = next().replace(/\/$/, "");
  else if (a === "--uid") o.uid = next();
  else if (a === "--sqls") o.sqls = JSON.parse(next());
  else if (a === "--range") o.range = JSON.parse(next());
  else fail(`unknown argument ${a}`);
}
if (!Array.isArray(o.sqls)) fail("--sqls is necessary and must be a JSON array");

const profileDir = mkdtempSync(join(tmpdir(), "pg-explore-"));
const browser = await Browser.launch({ profileDir, width: 1500, height: 900 });
try {
  const page = await (await browser.newContext()).newPage({ width: 1500, height: 900 });
  for (const sql of o.sqls) {
    page.requests.length = 0;
    page.byId.clear();
    const query = {
      refId: "A",
      datasource: { type: "grafana-postgresql-datasource", uid: o.uid },
      rawSql: sql,
      format: "table",
      rawQuery: true,
      editorMode: "code",
    };
    const panes = { a: { datasource: o.uid, queries: [query], range: o.range } };
    const url = `${o.url}/explore?schemaVersion=1&orgId=1&panes=${encodeURIComponent(JSON.stringify(panes))}`;
    await page.send("Page.navigate", { url });
    let r = null;
    for (let i = 0; i < 200; i++) {
      r = page.requests.filter((x) => x.url.includes("/api/ds/query") && x.status !== null).at(-1);
      if (r) break;
      await sleep(100);
    }
    if (!r) fail(`no request for ${JSON.stringify(sql)}`);
    await sleep(300);
    let body = null;
    for (let i = 0; i < 20 && body === null; i++) {
      try {
        body = (await page.send("Network.getResponseBody", { requestId: r.id })).body;
      } catch {
        await sleep(100);
      }
    }
    const sent = JSON.parse(r.body);
    const answer = JSON.parse(body);
    const q = sent.queries[0];
    const res = answer.results?.A;
    console.log(
      JSON.stringify({
        sql,
        sent_sql: q.rawSql,
        intervalMs: q.intervalMs,
        mdp: q.maxDataPoints,
        from: sent.from,
        to: sent.to,
        executed: res?.frames?.[0]?.schema?.meta?.executedQueryString ?? null,
        error: res?.error ?? null,
      }),
    );
  }
} finally {
  await browser.close();
  rmSync(profileDir, { recursive: true, force: true });
}
