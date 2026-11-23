// FE-09, wave H item P02: the data link tokens of Grafana 12 (9.18). The expected texts are the hrefs that Grafana 12.4.2 gave on
// 2026-11-22 for the same values (a temporary dashboard on the local Grafana, read in Chrome), unless a comment says otherwise.
import test from "node:test";
import assert from "node:assert/strict";
import { buildLink, expandUrl, numericOf, queryParam } from "../../web/js/links.js";
import { linkHref } from "../../web/js/dash_links.js";

const SPECIALS = " $,:@;!'()*&=#/?+~%\"<>[]{}|\\^`é€";
const vars = [
  { name: "env", kind: "custom", values: ["a b", "c"], all: false },
  { name: "tb", kind: "text", values: [SPECIALS], all: false },
  { name: "k", kind: "constant", values: ["a b$,:@;!'()*&=é"], all: false },
  { name: "iv", kind: "interval", values: ["10m"], all: false },
  { name: "al", kind: "custom", values: [], all: true },
  { name: "am", kind: "query", values: ["x"], all: true },
  { name: "qv", kind: "query", values: ["q r"], all: false },
  { name: "none", kind: "query", values: [], all: false },
];
const ctx = { from: 1795305600000, to: 1795327200000, variables: vars };
const x = (template, more = {}) => expandUrl(template, { ...ctx, ...more });

test("P02 ${<name>:queryparam}: the form of Grafana for each kind of variable", () => {
  assert.equal(x("/x?${env:queryparam}"), "/x?var-env=a%20b&var-env=c");
  assert.equal(x("${tb:queryparam}"), "var-tb=%20$,:@;%21%27%28%29%2A%26%3D%23%2F%3F%2B~%25%22%3C%3E%5B%5D%7B%7D%7C%5C%5E%60%C3%A9%E2%82%AC");
  // A constant has no URL state in Grafana: `@ : $ , ;` stay encoded.
  assert.equal(x("${k:queryparam}"), "var-k=a%20b%24%2C%3A%40%3B%21%27%28%29%2A%26%3D%C3%A9");
  assert.equal(x("${iv:queryparam}"), "var-iv=10m");
  assert.equal(x("${al:queryparam}|${am:queryparam}"), "var-al=$__all|var-am=$__all");
  assert.equal(x("${qv:queryparam}"), "var-qv=q%20r");
  // No value: one empty value (the URL state of Grafana for an empty selection, read in its code).
  assert.equal(x("${none:queryparam}"), "var-none=");
  // An unknown variable and an unknown format keep the token.
  assert.equal(x("${nope:queryparam}|${env:raw}|${__from:queryparam}"), "${nope:queryparam}|${env:raw}|${__from:queryparam}");
  // A value that came from the URL with a line end.
  assert.equal(queryParam({ name: "tb", kind: "text", values: ["a\r\nb"], all: false }), "var-tb=a%0D%0Ab");
});

test("P02 ${__value.time}: whole epoch milliseconds of the first time field, empty for null", () => {
  // DateTime64(9) 2026-11-22 01:00:00.123456789 and 1969-12-31 23:59:58.9985 as the frame holds them (seconds x 1000).
  assert.equal(x("${__value.time}", { valueTime: Number("1795309200.123456789") * 1000 }), "1795309200123");
  assert.equal(x("${__value.time}", { valueTime: Number("-1.0015") * 1000 }), "-1002");
  assert.equal(x("${__value.time}", { valueTime: Number("1795312800.5") * 1000 }), "1795312800500");
  // Not a Grafana measure: a product with a float error below the whole value (2039) keeps its millisecond.
  assert.equal(Number("2190455526.002") * 1000 < 2190455526002, true);
  assert.equal(x("${__value.time}", { valueTime: Number("2190455526.002") * 1000 }), "2190455526002");
  // A null time and a frame with no time field give an empty text. A panel link (no cell) keeps the token.
  assert.equal(x("/t=${__value.time}", { valueTime: null }), "/t=");
  assert.equal(x("/t=${__value.time}"), "/t=${__value.time}");
});

test("P02 ${__value.numeric}: the display number of Grafana", () => {
  const f = (kind, unit) => ({ kind, config: unit ? { unit } : {} });
  const n = (field, v) => x("${__value.numeric}", { valueNumeric: numericOf(field, v) });
  assert.equal(n(f("text"), "a b"), "NaN");
  assert.equal(n(f("text"), "42"), "NaN");
  assert.equal(n(f("text"), "  7 "), "NaN");
  assert.equal(n(f("number"), 12.5), "12.5");
  assert.equal(n(f("number"), -0.5), "-0.5");
  assert.equal(n(f("number"), 1e21), "1e+21");
  assert.equal(n(f("number"), null), "NaN");
  assert.equal(n(f("number_text"), parseFloat("18446744073709551615")), "18446744073709552000");
  assert.equal(n(f("bool"), true), "1");
  assert.equal(n(f("bool"), false), "0");
  assert.equal(n(f("time"), Number("1795309200.123456789") * 1000), "1795309200123");
  assert.equal(n(f("time"), Number("-1.0015") * 1000), "-1002");
  // Read in the display processor of Grafana: a text field with a unit gives its number; the unit `string` gives NaN.
  assert.equal(n(f("text", "short"), "42"), "42");
  assert.equal(n(f("number", "string"), 42), "NaN");
  assert.equal(x("/n=${__value.numeric}"), "/n=${__value.numeric}");
});

test("P02 ${__series.name}, ${__timezone} and ${__dashboard.uid}", () => {
  assert.equal(x("${__series.name}|${__dashboard.uid}", { seriesName: "Q1", dashboardUid: "hune-p02-links" }), "Q1|hune-p02-links");
  assert.equal(x("${__timezone}", { timezone: "utc" }), "utc");
  assert.equal(x("${__timezone}", { timezone: "Europe/Paris" }), "Europe/Paris");
  // Not a Grafana measure: the format `uriencode` keeps `/` and `+`.
  assert.equal(x("${__timezone}", { timezone: "Etc/GMT+5" }), "Etc/GMT+5");
  assert.equal(x("${__timezone}", { timezone: "browser" }), Intl.DateTimeFormat().resolvedOptions().timeZone);
  assert.equal(x("${__series.name}${__timezone}${__dashboard.uid}"), "${__series.name}${__timezone}${__dashboard.uid}");
});

test("P02 the two links of the OTel dashboard of the ClickHouse plugin", () => {
  const otel = { ...ctx, variables: [{ name: "serviceName", kind: "query", values: [], all: true }, { name: "trace_id", kind: "query", values: [], all: true }], rawValue: "abc123" };
  const a = buildLink({ title: "t", url: "/d/otel/otel-traces?${__url_time_range}&${serviceName:queryparam}&var-trace_id=${__value.raw}" }, otel);
  assert.equal(a.href, "/d/otel/otel-traces?from=1795305600000&to=1795327200000&var-serviceName=$__all&var-trace_id=abc123");
  const b = buildLink({ title: "t", url: "/d/otel/otel-traces?${__url_time_range}&${trace_id:queryparam}&var-serviceName=${__value.raw}" }, { ...otel, rawValue: "svc a" });
  assert.equal(b.href, "/d/otel/otel-traces?from=1795305600000&to=1795327200000&var-trace_id=$__all&var-serviceName=svc%20a");
});

test("P02 hostile values: no scheme, no raw line end, no new parameter, no second expansion", () => {
  const evil = (value) => [{ name: "v", kind: "text", values: [value], all: false }];
  // A value cannot be the scheme: the text of the token starts with `var-`, and the check of the final URL refuses it.
  assert.equal(buildLink({ title: "t", url: "${v:queryparam}" }, { variables: evil("javascript:alert(1)") }).href, null);
  assert.equal(buildLink({ title: "t", url: "/a?${v:queryparam}" }, { variables: evil("javascript:alert(1)") }).href, "/a?var-v=javascript:alert%281%29");
  assert.equal(buildLink({ title: "t", url: "${__timezone}" }, { timezone: "javascript:alert(1)" }).href, null);
  assert.equal(buildLink({ title: "t", url: "${__series.name}" }, { seriesName: "//evil.example" }).href, null);
  // A line end, `&`, `=` and `#` stay encoded: the value cannot cut the URL or add a parameter.
  assert.equal(expandUrl("/a?${v:queryparam}", { variables: evil("x\r\nSet-Cookie: a=1&admin=1#f") }), "/a?var-v=x%0D%0ASet-Cookie:%20a%3D1%26admin%3D1%23f");
  // A value with a token stays text: the replacement makes one pass.
  assert.equal(expandUrl("/a?${v:queryparam}&u=${__dashboard.uid}", { variables: evil("${__dashboard.uid}"), dashboardUid: "d1" }), "/a?var-v=$%7B__dashboard.uid%7D&u=d1");
});

test("P02 a very long value: the text is complete and the time grows linearly (minimum of 7 runs after 2 warm-ups)", () => {
  const run = (n) => {
    const variables = [{ name: "v", kind: "text", values: ["a b&".repeat(n)], all: false }];
    return expandUrl("/a?${v:queryparam}", { variables });
  };
  assert.equal(run(50000).length, "/a?var-v=".length + 50000 * "a%20b%26".length);
  const best = (n) => {
    for (let k = 0; k < 2; k++) run(n);
    let low = Infinity;
    for (let k = 0; k < 7; k++) {
      const t0 = performance.now();
      run(n);
      low = Math.min(low, performance.now() - t0);
    }
    return low;
  };
  const small = best(50000);
  const big = best(200000);
  assert.ok(big < small * 4 * 2.5, `50,000: ${small} ms, 200,000: ${big} ms`);
});

test("P02 a dashboard link of the kind `link` also takes ${<name>:queryparam} (9.8a)", () => {
  const link = { kind: "link", title: "t", url: "https://wiki.example.test/r?${env:queryparam}", target_blank: false, include_vars: false, keep_time: false };
  assert.equal(linkHref(link, { ...ctx, fromText: "now-6h", toText: "now" }), "https://wiki.example.test/r?var-env=a%20b&var-env=c");
});
