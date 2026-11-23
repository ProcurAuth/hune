// Tests of the URL contract (9.5).
import test from "node:test";
import assert from "node:assert/strict";
import { slugify, dashboardPath, parseSearch, buildSearch, panelIdFromParam, listSearch } from "../../web/js/url_state.js";

test("slug folds ASCII, joins runs with one dash and cuts at 64", () => {
  assert.equal(slugify("Nodes"), "nodes");
  assert.equal(slugify("  Weight & Score (pts) -- daily  "), "weight-score-pts-daily");
  assert.equal(slugify("Ünï"), "n");
  assert.equal(slugify("!!!"), "");
  assert.equal(slugify("a".repeat(100)).length, 64);
  assert.equal(slugify(`${"a".repeat(63)} b`), "a".repeat(63));
});

test("dashboard path with and without slug", () => {
  assert.equal(dashboardPath("nodes", "Nodes"), "d/nodes/nodes");
  assert.equal(dashboardPath("x1", "!!!"), "d/x1");
});

test("panel id accepts both forms", () => {
  assert.equal(panelIdFromParam("7"), 7);
  assert.equal(panelIdFromParam("panel-7"), 7);
  assert.equal(panelIdFromParam("panel-"), null);
  assert.equal(panelIdFromParam("x"), null);
  // The id is a whole number from 1 with no leading zero (FE-27, the closed choice).
  for (const text of ["0", "00", "007", "panel-0", "panel-07"]) assert.equal(panelIdFromParam(text), null, text);
  assert.equal(parseSearch("?viewPanel=007").viewPanel, null);
  assert.equal(parseSearch("?editPanel=0").editPanel, null);
});

test("read: plus is a space, repeated var, unknown names dropped", () => {
  const s = parseSearch("?orgId=1&from=now-6h&to=now&var-env=prod&var-node=a&var-node=b&refresh=1m&viewPanel=panel-2&q=a+b&edit=1&editview=json");
  assert.equal(s.from, "now-6h");
  assert.deepEqual(s.vars.get("node"), ["a", "b"]);
  assert.equal(s.viewPanel, 2);
  assert.equal(s.q, "a b");
  assert.equal(s.edit, true);
  assert.equal(s.editview, "json");
  assert.equal(s.refresh, "1m");
});

test("write: fixed order and encoding, drops unknown", () => {
  const s = parseSearch("?viewPanel=3&refresh=30s&var-b=2&var-a=1&to=now&from=now-1h&orgId=1&version=current");
  const out = buildSearch(s, ["a", "b"]);
  assert.equal(out, "?from=now-1h&to=now&var-a=1&var-b=2&refresh=30s&viewPanel=3&version=current");
  assert.equal(buildSearch({ vars: new Map([["w", ["a b&c"]]]) }, ["w"]), "?var-w=a%20b%26c");
  assert.equal(buildSearch({}, []), "");
});

test("list search", () => {
  assert.equal(listSearch(""), "");
  assert.equal(listSearch("a b"), "?q=a%20b");
});

test("editview: json, history and settings; another value is dropped (9.5)", () => {
  for (const name of ["json", "history", "settings"]) assert.equal(parseSearch(`?editview=${name}`).editview, name);
  for (const name of ["", "other", "SETTINGS", "settings "]) assert.equal(parseSearch(`?editview=${encodeURIComponent(name)}`).editview, null);
  const s = parseSearch("?from=now-1h&to=now&edit=1&editview=settings");
  assert.equal(buildSearch(s, []), "?from=now-1h&to=now&edit=1&editview=settings");
});

// D02 (9.5): a link of Grafana 12.4.2 after the nginx map of 15.17. The measure of 2026-11-22 gave these URLs.
test("D02: a Grafana 12 URL gives epoch milliseconds, the first timezone applies, orgId is dropped, and the page writes timezone after to", () => {
  const s = parseSearch("?timezone=Europe%2FParis&from=2026-11-09T08:00:00.000Z&to=2026-11-09T14:00:00.000Z&orgId=1&timezone=utc");
  assert.equal(s.from, "1794211200000");
  assert.equal(s.to, "1794232800000");
  assert.equal(s.timezone, "Europe/Paris");
  assert.equal(buildSearch(s, []), "?from=1794211200000&to=1794232800000&timezone=Europe%2FParis");
  const c = parseSearch("?from=20261109T080000&to=2026-11-09T14:00:00&var-a=1&timezone=browser&refresh=1m&viewPanel=2");
  assert.equal(buildSearch(c, ["a"]), "?from=1794211200000&to=1794232800000&timezone=browser&var-a=1&refresh=1m&viewPanel=2");
  assert.equal(parseSearch("?from=2026-11-09T08:00:00.123Z").from, "1794211200123");
  // The forms that Grafana reads as 1970 stay as they are: the page refuses them with the notice of 9.5. `+` reads as a space.
  assert.equal(parseSearch("?from=2026-11-09T08:00:00%2B01:00").from, "2026-11-09T08:00:00+01:00");
  assert.equal(parseSearch("?from=2026-11-09T08:00:00+01:00").from, "2026-11-09T08:00:00 01:00");
  assert.equal(parseSearch("?from=2026-11-09&to=2026-11-10").to, "2026-11-10");
  assert.equal(parseSearch("?from=2026-11-09+08:00:00").from, "2026-11-09 08:00:00");
  // No timezone gives null, and the page writes no timezone. An empty name is a name: the page refuses it with the notice.
  assert.equal(parseSearch("?from=now-6h").timezone, null);
  assert.equal(parseSearch("?timezone=&timezone=utc").timezone, "");
  assert.equal(buildSearch({ from: "now-1h", to: "now", timezone: null, vars: new Map(), refresh: "1m" }, []), "?from=now-1h&to=now&refresh=1m");
  // The last from and the last to apply, as for the other names of the query.
  assert.equal(parseSearch("?from=now-1h&from=2026-11-09T08:00:00").from, "1794211200000");
});

test("D02: a hostile URL keeps its text, the parse stays linear, and the page writes each value encoded", () => {
  const long = "9".repeat(1e6);
  const s = parseSearch(`?from=${long}&to=${"2026-11-09T08:00:00.000Z".repeat(1000)}&timezone=${encodeURIComponent("<b>x</b>&a=b")}`);
  assert.equal(s.from, long);
  assert.equal(s.to.length, 24000);
  assert.equal(s.timezone, "<b>x</b>&a=b");
  assert.equal(buildSearch({ timezone: s.timezone }, []), "?timezone=%3Cb%3Ex%3C%2Fb%3E%26a%3Db");
  const run = () => {
    const t0 = performance.now();
    parseSearch(`?from=${long}&timezone=${long}`);
    return performance.now() - t0;
  };
  run();
  run();
  const best = Math.min(...Array.from({ length: 7 }, run));
  assert.ok(best < 100, `${best.toFixed(2)} ms`);
});
