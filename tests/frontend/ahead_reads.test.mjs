// SPDX-License-Identifier: MIT OR Apache-2.0
// The reads of the first route start before the session answer (9.6, 9.8). Their effects wait until the page takes the answer.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as api from "../../web/js/api.js";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "../..");
const code = (path) => readFileSync(join(ROOT, path), "utf8").replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");

// A fake fetch. `handler(url, init)` gives { status, body, headers }. The calls list keeps the order of the requests.
function fakeFetch(handler) {
  const calls = [];
  globalThis.fetch = (url, init) => {
    calls.push({ url, init });
    return new Promise((resolve, reject) => {
      if (init.signal) init.signal.addEventListener("abort", () => reject(Object.assign(new Error("aborted"), { name: "AbortError" })));
      Promise.resolve(handler(url, init)).then((r) => resolve(new Response(r.body === undefined ? null : JSON.stringify(r.body), { status: r.status || 200, headers: r.headers || {} })));
    });
  };
  return calls;
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 5));

test("the read sends its request at once, with the same URL, headers and credentials as a normal read", async () => {
  const calls = fakeFetch(() => ({ body: { uid: "nodes" } }));
  const read = api.startAhead((signal) => api.dashboard("nodes", signal));
  assert.equal(calls.length, 1, "the request starts when the read starts");
  const page = new AbortController();
  assert.deepEqual(await read.use(page.signal), { uid: "nodes" });
  await api.dashboard("nodes", page.signal);
  assert.equal(calls.length, 2);
  assert.equal(calls[0].url, calls[1].url);
  assert.deepEqual(calls[0].init.headers, calls[1].init.headers);
  assert.equal(calls[0].init.credentials, calls[1].init.credentials);
  assert.equal(calls[0].init.method, calls[1].init.method);
});

test("a 401 calls the hook once, when the page takes the answer, and never before", async () => {
  fakeFetch(() => ({ status: 401, body: { error: { code: "unauthorized", message: "no" } } }));
  let hooked = 0;
  api.setUnauthorizedHandler(() => {
    hooked += 1;
  });
  const read = api.startAhead((signal) => api.dashboard("nodes", signal));
  await tick();
  assert.equal(hooked, 0, "the answer waits: no page shows before the first view");
  const page = new AbortController();
  await assert.rejects(read.use(page.signal), (error) => error.status === 401);
  assert.equal(hooked, 1, "the hook runs once, when the page takes the read");
  api.setUnauthorizedHandler(null);
});

test("a read that nobody takes has no effect on the page, and its abort cancels the request", async () => {
  const calls = fakeFetch(() => ({ status: 401, body: {} }));
  let hooked = 0;
  api.setUnauthorizedHandler(() => {
    hooked += 1;
  });
  const read = api.startAhead((signal) => api.dashboard("nodes", signal));
  read.abort();
  await tick();
  assert.equal(hooked, 0);
  assert.equal(calls[0].init.signal.aborted, true);
  api.setUnauthorizedHandler(null);
});

test("the abort of the page signal aborts the request, and a normal read has its effect at once", async () => {
  const calls = fakeFetch(() => new Promise(() => {}));
  const read = api.startAhead((signal) => api.dashboard("nodes", signal));
  const page = new AbortController();
  const taken = read.use(page.signal);
  page.abort();
  await assert.rejects(taken, (error) => api.isAbort(error));
  assert.equal(calls[0].init.signal.aborted, true);
  let hooked = 0;
  api.setUnauthorizedHandler(() => {
    hooked += 1;
  });
  fakeFetch(() => ({ status: 401, body: {} }));
  await assert.rejects(api.dashboard("nodes"), (error) => error.status === 401);
  assert.equal(hooked, 1, "a read that is not ahead calls the hook at its answer");
  api.setUnauthorizedHandler(null);
});

test("an answer that comes after the page took the read has its effect at once", async () => {
  let release;
  fakeFetch(() => new Promise((resolve) => {
    release = () => resolve({ status: 401, body: {} });
  }));
  let hooked = 0;
  api.setUnauthorizedHandler(() => {
    hooked += 1;
  });
  const read = api.startAhead((signal) => api.dashboard("nodes", signal));
  const taken = read.use(new AbortController().signal);
  assert.equal(hooked, 0);
  release();
  await assert.rejects(taken, (error) => error.status === 401);
  assert.equal(hooked, 1);
  api.setUnauthorizedHandler(null);
});

test("main.js starts the reads of the route with the module, drops them on a failed session and gives them to the first view only", () => {
  const main = code("web/js/main.js");
  assert.match(main, /api\.startAhead\(\(signal\) => api\.dashboard\(route\.uid, signal\)\)/);
  assert.match(main, /api\.startAhead\(\(signal\) => Promise\.all\(\[api\.folders\(signal\), api\.dashboards\(signal\)\]\)\)/);
  // The reads start in preloadRoute, which runs before the session request.
  assert.ok(main.indexOf("preloadRoute();") < main.indexOf("await api.session()"));
  // The first view takes the reads once, for the same route and the same uid. Another route ends them.
  assert.match(main, /if \(held\.kind === route\.kind && held\.uid === route\.uid\) return held\.read;\s*held\.read\.abort\(\);/);
  assert.match(main, /const held = ahead;\s*ahead = null;/);
  assert.match(main, /mod\.mount\(\{ \.\.\.ctx, uid: route\.uid, slug: route\.slug, ahead: reads \}\)/);
  assert.match(main, /mod\.mount\(\{ \.\.\.ctx, afterLoad: entryHook\(isEntry\), ahead: reads \}\)/);
  // A failed session (the 401 and the page "no answer") ends the reads. A new start replaces them.
  assert.match(main, /dropAhead\(\);\s*if \(error && error\.status === 401\) return;/);
  assert.match(main, /function preloadRoute\(\) \{\s*dropAhead\(\);/);
  // The busy state and the order of the first view do not change: the view still waits for the session, the font and the icons.
  assert.ok(main.indexOf("await Promise.all([fontWait, spriteReady]);") < main.indexOf("await show();"));
});

test("the pages take the reads one time, and send their own request without them", () => {
  const dash = code("web/js/dashboard.js");
  assert.match(dash, /ctx\.ahead \? ctx\.ahead\.use\(controller\.signal\) : api\.dashboard\(uid, controller\.signal\)/);
  const list = code("web/js/list.js");
  assert.match(list, /const taken = reads;\s*reads = null;/);
  assert.match(list, /taken \? taken\.use\(controller\.signal\) : Promise\.all\(\[api\.folders\(controller\.signal\), api\.dashboards\(controller\.signal\)\]\)/);
});
