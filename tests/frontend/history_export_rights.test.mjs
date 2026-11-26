// SPDX-License-Identifier: MIT OR Apache-2.0
// The flags `access.history` and `access.export` hide the entries of the page (6.12, 9.9, 10.11, 10.14).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const read = (p) => readFileSync(new URL(`../../web/${p}`, import.meta.url), "utf8");

test("the menu More actions has History only with access.history and Export only with access.export", () => {
  const js = read("js/dashboard.js");
  assert.match(js, /if \(loaded\.access && loaded\.access\.history === true\) items\.push\(\{ label: "History"/);
  assert.match(js, /if \(loaded\.access && loaded\.access\.export === true\) items\.push\(\{ label: "Export"/);
  assert.ok(!/items\.push\(\{ label: "History"/.test(js.replace(/if \(loaded\.access && loaded\.access\.history === true\) items\.push\(\{ label: "History"/, "")), "no other History entry");
  assert.ok(!/items\.push\(\{ label: "Export"/.test(js.replace(/if \(loaded\.access && loaded\.access\.export === true\) items\.push\(\{ label: "Export"/, "")), "no other Export entry");
});

test("a history URL opened by hand without access.history shows the refusal page and starts no page", () => {
  const js = read("js/dashboard.js");
  const at = js.indexOf('parseSearch(window.location.search).editview === "history"');
  assert.ok(at > 0, "the check is in mount");
  const piece = js.slice(at, at + 260);
  assert.match(piece, /!\(answer\.access && answer\.access\.history === true\)/);
  assert.match(piece, /errorPage\(ctx, \{ status: 403/);
  assert.match(piece, /return \{ destroy\(\) \{\} \};/);
  // The check comes before the page is built.
  assert.ok(at < js.indexOf("buildPage(initial"), "the refusal comes before the page");
});

test("the history page exports a version only with access.export and compares with the newest version without it", () => {
  const js = read("js/editor/history.js");
  assert.match(js, /const canExport = Boolean\(E\.base\.access && E\.base\.access\.export === true\);/);
  assert.match(js, /if \(canExport\) acts\.append\(button\("Export this version"/);
  const fn = js.slice(js.indexOf("async function currentText()"), js.indexOf("const getText"));
  assert.match(fn, /if \(canExport\) return \(await api\.dashboardDownload/);
  assert.match(fn, /editorRoutes\.history\(E\.uid, \{ limit: 1 \}/);
  assert.match(fn, /editorRoutes\.versionDownload\(E\.uid, head\.version/);
});

test("the list answer gives the flag of a card to cardRights and the menu has no empty button", () => {
  const js = read("js/list.js");
  assert.match(js, /export: Boolean\(item\.access && item\.access\.export === true\)/);
  assert.match(js, /if \(entries && entries\.length\) \{/);
});
