// SPDX-License-Identifier: MIT OR Apache-2.0
// FE-09: data links: tokens, encoding, final URL check (9.18).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { expandUrl, checkUrl, buildLink } from "../../web/js/links.js";

const v = JSON.parse(readFileSync(new URL("./vectors/FE-09-links.json", import.meta.url)));
const withRow = (ctx) => ({ ...ctx, rowValue: (name) => ctx.row[name] });

test("FE-09 tokens and encoding", () => {
  for (const c of v.expand) assert.equal(expandUrl(c.template, withRow(c.ctx)), c.expect, c.template);
});

test("FE-09 final URL check", () => {
  for (const [url, ok] of Object.entries(v.check)) assert.equal(checkUrl(url), ok, JSON.stringify(url));
});

test("FE-09 a value cannot make an unsafe URL", () => {
  for (const c of v.build) {
    const out = buildLink(c.link, withRow(c.ctx));
    assert.equal(out.href, c.href);
    assert.equal(out.target_blank, c.link.target_blank);
  }
});

test("F13 the title of a link shows after the replacement of the variables (9.13)", () => {
  const link = { title: "Open ${env}", url: "/d/x?e=${env}", target_blank: false };
  const ctx = { from: 1, to: 2, variables: [{ name: "env", values: ["prod"], all: false }], interpolate: (t) => t.replace("${env}", "prod") };
  assert.equal(buildLink(link, ctx).title, "Open prod");
  assert.equal(buildLink(link, ctx).href, "/d/x?e=prod");
  // No interpolate function: the stored title.
  assert.equal(buildLink(link, { from: 1, to: 2, variables: [] }).title, "Open ${env}");
});
