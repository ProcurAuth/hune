// Tests of the write routes of editor/api_editor.js (6.3, 6.6, 6.15 to 6.19, 6.11, 6.22) with a fake fetch.
import test from "node:test";
import assert from "node:assert/strict";
import * as api from "../../web/js/api.js";
import * as routes from "../../web/js/editor/api_editor.js";

function fakeFetch(handler) {
  const calls = [];
  globalThis.fetch = async (url, init) => {
    calls.push({ url, init });
    const r = await handler(url, init, calls.length);
    return new Response(r.body === undefined ? null : typeof r.body === "string" ? r.body : JSON.stringify(r.body), { status: r.status || 200, headers: r.headers || {} });
  };
  return calls;
}

test("save: PUT with a quoted If-Match, the Idempotency-Key and a JSON body", async () => {
  const calls = fakeFetch(() => ({ body: { version: 2 } }));
  const answer = await routes.saveDashboard("nodes", { message: "m", document: { uid: "nodes" } }, "abc123", "0e0e0e0e-0e0e-4e0e-8e0e-0e0e0e0e0e0e");
  assert.equal(answer.version, 2);
  const { url, init } = calls[0];
  assert.equal(url, "api/dashboards/nodes");
  assert.equal(init.method, "PUT");
  assert.equal(init.headers["If-Match"], '"abc123"');
  assert.equal(init.headers["Idempotency-Key"], "0e0e0e0e-0e0e-4e0e-8e0e-0e0e0e0e0e0e");
  assert.equal(init.headers["Content-Type"], "application/json");
  assert.deepEqual(JSON.parse(init.body), { message: "m", document: { uid: "nodes" } });
});

test("create, delete, restore, move send the right route and headers", async () => {
  const calls = fakeFetch(() => ({ body: {} }));
  await routes.createDashboard({ folder: "f", message: "", document: {} }, "k1");
  await routes.deleteDashboard("u", "e1", "k2");
  await routes.restoreVersion("u", { version: 3, message: "" }, "e2", "k3");
  await routes.moveDashboard("u", { folder: "g", message: "" }, "e3", "k4");
  assert.deepEqual(calls.map((c) => [c.init.method, c.url]), [["POST", "api/dashboards"], ["DELETE", "api/dashboards/u"], ["POST", "api/dashboards/u/restore"], ["POST", "api/dashboards/u/move"]]);
  assert.equal(calls[0].init.headers["If-Match"], undefined);
  assert.equal(calls[1].init.headers["If-Match"], '"e1"');
  assert.equal(calls[1].init.body, undefined);
  assert.deepEqual(calls.map((c) => c.init.headers["Idempotency-Key"]), ["k1", "k2", "k3", "k4"]);
});

test("validate_only sends the exact text of the document (10.4)", async () => {
  const calls = fakeFetch(() => ({ body: { valid: true } }));
  await routes.validateDocument({ uid: "u" }, '{"a": 1, "a": 2}');
  await routes.validateDocument({ folder: "f" }, '{"uid": "x"}');
  assert.equal(calls[0].url, "api/dashboards/u?validate_only=true");
  assert.equal(calls[0].init.method, "PUT");
  assert.equal(calls[0].init.body, '{"document":{"a": 1, "a": 2}}');
  assert.equal(calls[0].init.headers["If-Match"], undefined);
  assert.equal(calls[1].url, "api/dashboards?validate_only=true");
  assert.equal(calls[1].init.body, '{"folder":"f","document":{"uid": "x"}}');
});

test("folders and preview routes; a 204 gives null", async () => {
  const calls = fakeFetch((url, init) => (init.method === "DELETE" ? { status: 204 } : { body: { id: "x" } }));
  await routes.createFolder({ id: "x", title: "X" });
  await routes.renameFolder("x", "Y");
  assert.equal(await routes.deleteFolder("x"), null);
  await routes.preview("prod", { uid: "u" });
  assert.deepEqual(calls.map((c) => [c.init.method, c.url]), [["POST", "api/folders"], ["PUT", "api/folders/x"], ["DELETE", "api/folders/x"], ["POST", "api/sources/prod/preview"]]);
  assert.deepEqual(JSON.parse(calls[1].init.body), { title: "Y" });
});

test("a conflict keeps its extra members; a write that gets no answer ends with request_timeout", async () => {
  fakeFetch(() => ({ status: 409, body: { error: { code: "conflict", message: "m", retryable: false, current_etag: "e9", current_version: 8, current_folder: "ops", saved_at: "t", saved_by: "u" } } }));
  await assert.rejects(routes.saveDashboard("u", {}, "e", "k"), (error) => error.code === "conflict" && error.extra.current_etag === "e9" && error.extra.current_folder === "ops" && error.status === 409);
  globalThis.fetch = (url, init) => new Promise((resolve, reject) => init.signal.addEventListener("abort", () => reject(Object.assign(new Error("a"), { name: "AbortError" }))));
  await assert.rejects(api.request("PUT", "api/x", { body: {}, timeoutMs: 20 }), (error) => error.code === "request_timeout");
  fakeFetch(() => { throw new TypeError("network"); });
  globalThis.fetch = async () => { throw new TypeError("network"); };
  await assert.rejects(routes.saveDashboard("u", {}, "e", "k"), (error) => error.code === "network_error");
});

test("invalid_document keeps the list of errors with pointer, line and column", async () => {
  fakeFetch(() => ({ status: 422, body: { error: { code: "invalid_document", message: "m", retryable: false, errors: [{ pointer: "/title", code: "out_of_range", message: "x", line: null, column: null }] } } }));
  await assert.rejects(routes.validateDocument({ uid: "u" }, "{}"), (error) => error.extra.errors[0].pointer === "/title");
});

test("listDashboards: one page with the members folder, q, tag, limit and after (6.12); a member that is not set is not sent", async () => {
  const calls = fakeFetch(() => ({ body: { items: [], next: null } }));
  await api.listDashboards({ tag: "chain", limit: 500 });
  await api.listDashboards({ folder: "transit", q: "a b", tag: "x", limit: 50, after: "cursor" });
  await api.listDashboards({ tag: null, limit: 500, after: null });
  await api.listDashboards();
  assert.deepEqual(calls.map((c) => c.url), [
    "api/dashboards?tag=chain&limit=500",
    "api/dashboards?folder=transit&q=a+b&tag=x&limit=50&after=cursor",
    "api/dashboards?limit=500",
    "api/dashboards",
  ]);
  assert.ok(calls.every((c) => c.init.method === "GET"));
});
