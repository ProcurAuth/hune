// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser case B211 (9.18): the data link tokens of Grafana 12 in a table, and a click on a link with tokens.
// The first link has the form of the two links of the OTel dashboard of the ClickHouse plugin: it opens the same dashboard with
// `${__url_time_range}`, the variables in the form `${<name>:queryparam}` and the value of the cell. The hrefs are the hrefs that
// Grafana 12.4.2 gives for the same values (measured on 2026-11-22). A text variable with `javascript:alert(1)` gives no link.
import { doc, panel, section } from "../lib/docs.mjs";
import { waitPanelsReady } from "../lib/h.mjs";

const UID = "linktokens211";
const link = (title, url) => ({ title, url, target_blank: false });
const TOKENS = "t=${__value.time}&n=${__value.numeric}&s=${__series.name}&z=${__timezone}&u=${__dashboard.uid}";
const SQL = ["SELECT 'a b' AS name, 12.5 AS v, toDateTime64('2026-11-22 01:00:00.123456789', 9, 'UTC') AS t, 'w' AS w"];
const OPEN = `/d/${UID}/link-tokens?\${__url_time_range}&\${env:queryparam}&\${svc:queryparam}&var-q=\${__value.raw}`;

export function tokensDoc() {
  return doc(UID, "Link tokens", [
    section(null, [
      panel(1, "table", "Tokens", SQL, {
        grid: { x: 0, y: 0, w: 24, h: 6 },
        fields: {
          defaults: {},
          overrides: [
            { match: { by: "name", value: "name" }, set: { links: [link("Open", OPEN)] } },
            { match: { by: "name", value: "v" }, set: { links: [link("Number", `/x?\${k:queryparam}&${TOKENS}`)] } },
            { match: { by: "name", value: "t" }, set: { links: [link("Bad", "${q:queryparam}")] } },
            { match: { by: "name", value: "w" }, set: { links: [link("Text", `/y?${TOKENS}`)] } },
          ],
        },
      }),
    ]),
  ], {
    time: { from: "1795305600000", to: "1795327200000", timezone: "utc", refresh: null },
    variables: [
      { kind: "custom", name: "env", label: null, hide: "none", options: [{ text: "a b", value: "a b" }, { text: "c", value: "c" }], multi: true, include_all: false, all_value: null, default: ["a b", "c"] },
      { kind: "custom", name: "svc", label: null, hide: "none", options: [{ text: "x", value: "x" }], multi: true, include_all: true, all_value: null, default: ["$__all"] },
      { kind: "constant", name: "k", label: null, hide: "variable", value: "a b$,:@;" },
      { kind: "text", name: "q", label: null, hide: "none", value_type: "text", default: "javascript:alert(1)" },
    ],
  });
}

const HREFS = `(() => {
  const heads = [...document.querySelectorAll('[data-testid="table-1"] th')].map((h) => h.textContent.trim());
  const row = document.querySelector('[data-testid="table-1"] tbody tr');
  const out = {};
  [...row.children].forEach((td, i) => { const a = td.querySelector("a"); out[heads[i]] = a ? a.getAttribute("href") : null; });
  return out;
})()`;

export const cases = [
  {
    id: "B211",
    name: "data link tokens of Grafana 12: queryparam, value time and numeric, series, zone, uid, and a click",
    async run(t) {
      const r = await t.api.create("transit", tokensDoc());
      t.assert(r.status === 201, `create: ${r.status} ${r.text.slice(0, 300)}`);
      const p = await t.open(t.url(`d/${UID}/link-tokens`));
      await waitPanelsReady(p, [1]);
      const cells = await p.eval(HREFS);
      t.eq(cells.name, `/d/${UID}/link-tokens?from=1795305600000&to=1795327200000&var-env=a%20b&var-env=c&var-svc=$__all&var-q=a%20b`, "the link of the OTel form");
      // A constant keeps `@ : $ , ;` encoded. The time has whole milliseconds. A number gives its value, a text gives NaN.
      t.eq(cells.v, "/x?var-k=a%20b%24%2C%3A%40%3B&t=1795309200123&n=12.5&s=A&z=utc&u=linktokens211", "the link of the number cell");
      t.eq(cells.w, "/y?t=1795309200123&n=NaN&s=A&z=utc&u=linktokens211", "the link of the text cell");
      // The text variable holds `javascript:alert(1)`: the URL starts with `var-q=`, so the check of 9.18 gives no link.
      t.eq(cells.t, null, "no link for a URL that fails the check");
      t.assert(await p.eval(`!document.querySelector('a[href*="javascript"]')`), "no anchor with a script URL");
      // The click opens the same dashboard in the same tab with the values of the link.
      await p.click('[data-testid="table-1"] a[href*="var-q="]');
      await p.waitFor(`new URLSearchParams(location.search).get("var-q") === "a b"`, { what: "the navigation of the link" });
      await waitPanelsReady(p, [1]);
      const search = new URLSearchParams(await p.eval("location.search"));
      t.eq(search.getAll("var-env"), ["a b", "c"], "the values of env after the click");
      t.eq(search.get("var-svc"), "$__all", "All of svc after the click");
      t.eq(await p.eval("location.pathname"), `/d/${UID}/link-tokens`, "the path after the click");
      // After the click the text variable holds `a b`: the URL `var-q=a%20b` still fails the check of 9.18.
      t.eq((await p.eval(HREFS)).t, null, "no link after the click");
    },
  },
];
