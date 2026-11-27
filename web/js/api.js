// The API client: one function for each read route of section 6, with abort, the error body of 6.7 and the
// X-Hune-Build notice (6.4, 9.6). All URLs are relative: the browser resolves them against <base href>.

import { boot } from "./config.js";
import { notify } from "./ui.js";

// An error of the API, or of the network (status 0, code "network_error").
export class ApiError extends Error {
  constructor({ status, code, message, retryable = false, extra = {}, requestId = null }) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.retryable = retryable;
    this.extra = extra;
    this.requestId = requestId;
  }

  // True for an abort: the caller ignores it.
  get aborted() {
    return this.code === "aborted";
  }
}

export function isAbort(error) {
  return !!error && (error.name === "AbortError" || error.code === "aborted");
}

const hooks = {
  onUnauthorized: null,
  onBuildMismatch: null,
};

// main.js sets the hook that shows the full page of a 401.
export function setUnauthorizedHandler(fn) {
  hooks.onUnauthorized = fn;
}

let buildNoticeShown = false;

export function showBuildNotice() {
  if (buildNoticeShown) return;
  buildNoticeShown = true;
  notify({
    level: "info",
    key: "build",
    text: "A new version of Hune is available. Reload the page.",
    action: { label: "Reload", onClick: () => window.location.reload() },
  });
}

function checkBuild(response) {
  const build = response.headers.get("X-Hune-Build");
  if (build && boot.build && build !== boot.build) showBuildNotice();
}

// A read that starts ahead (`startAhead`) keeps its effects (the build notice, the hook of a 401, the notice of the access service)
// under its signal until `use` runs them.
const waiting = new WeakMap();

function effect(signal, run) {
  const list = waiting.get(signal);
  if (list) list.push(run);
  else run();
}

// Starts reads before the page needs them (9.6). `make(signal)` sends the requests and gives a promise. `abort()` ends a read that nobody
// takes, with no effect. `use(signal)` gives the promise to the page: an error shows then, never before.
export function startAhead(make) {
  const controller = new AbortController();
  const effects = [];
  waiting.set(controller.signal, effects);
  const promise = make(controller.signal);
  promise.catch(() => {});
  return {
    abort: () => controller.abort(),
    use(signal) {
      signal.addEventListener("abort", () => controller.abort());
      waiting.delete(controller.signal);
      for (const run of effects.splice(0)) run();
      return promise;
    },
  };
}

async function readError(response) {
  const requestId = response.headers.get("X-Request-Id");
  let body = null;
  try {
    body = await response.json();
  } catch (error) {
    body = null;
  }
  const info = body && typeof body === "object" && body.error && typeof body.error === "object" ? body.error : null;
  if (!info) {
    return new ApiError({
      status: response.status,
      code: response.status >= 500 ? "internal_error" : "bad_request",
      message: `The server answered with status ${response.status}.`,
      requestId,
    });
  }
  const { code, message, retryable, ...extra } = info;
  return new ApiError({
    status: response.status,
    code: typeof code === "string" ? code : "internal_error",
    message: typeof message === "string" ? message : `The server answered with status ${response.status}.`,
    retryable: retryable === true,
    extra,
    requestId,
  });
}

// One request. The answer body is JSON. A 304 gives { notModified: true }.
export async function request(method, path, { body, signal, headers = {}, raw = false, timeoutMs = 0 } = {}) {
  let timedOut = false;
  let timer = 0;
  if (timeoutMs > 0) {
    // A write that gets no answer within timeoutMs ends with the code request_timeout (10.9).
    const own = new AbortController();
    if (signal) signal.addEventListener("abort", () => own.abort());
    timer = setTimeout(() => {
      timedOut = true;
      own.abort();
    }, timeoutMs);
    signal = own.signal;
  }
  try {
    return await send(method, path, { body, signal, headers, raw });
  } catch (error) {
    if (timedOut && error && error.code === "aborted") throw new ApiError({ status: 0, code: "request_timeout", message: "Hune did not answer in time.", retryable: true });
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

async function send(method, path, { body, signal, headers, raw }) {
  const init = { method, signal, headers: { Accept: "application/json", ...headers }, credentials: "same-origin" };
  if (body !== undefined) {
    init.headers["Content-Type"] = "application/json";
    init.body = typeof body === "string" ? body : JSON.stringify(body);
  }
  let response;
  try {
    response = await fetch(path, init);
  } catch (error) {
    if (isAbort(error)) throw new ApiError({ status: 0, code: "aborted", message: "The request was cancelled." });
    throw new ApiError({ status: 0, code: "network_error", message: "Hune does not answer.", retryable: true });
  }
  effect(signal, () => checkBuild(response));
  if (response.status === 304) return { notModified: true };
  if (!response.ok) {
    const error = await readError(response);
    if (response.status === 401) {
      effect(signal, () => {
        if (hooks.onUnauthorized) hooks.onUnauthorized(error);
      });
    }
    if (error.code === "pdp_unavailable") {
      effect(signal, () => notify({ level: "error", key: "pdp", text: "The access decision service does not answer. Try again later." }));
    }
    throw error;
  }
  if (raw) return response;
  if (response.status === 204) return null;
  try {
    const data = await response.json();
    const etag = response.headers.get("ETag");
    if (etag && data && typeof data === "object" && !Array.isArray(data)) Object.defineProperty(data, "_etagHeader", { value: etag.replace(/"/g, ""), enumerable: false });
    return data;
  } catch (error) {
    if (isAbort(error)) throw new ApiError({ status: 0, code: "aborted", message: "The request was cancelled." });
    throw new ApiError({ status: response.status, code: "source_bad_response", message: "The answer is not valid JSON." });
  }
}

export function query(params) {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) if (value !== undefined && value !== null && value !== "") search.set(key, String(value));
  const text = search.toString();
  return text ? `?${text}` : "";
}

export const enc = encodeURIComponent;

// 6.10
export const session = (signal) => request("GET", "api/session", { signal });

// 6.11
export const folders = (signal) => request("GET", "api/folders", { signal });

// 6.12 with the pagination of 6.24: one page.
export const dashboardsPage = (params, signal) => request("GET", `api/dashboards${query(params)}`, { signal });

// 6.12: one page of the list with the members that the route knows (folder, q, tag, limit, after). `tag` is one tag: the route refuses a
// repeated `tag` (6.2), so the page sends the first tag of a dashboard link and tests the other tags itself (9.8a).
export const listDashboards = ({ folder, q, tag, limit, after } = {}, signal) => dashboardsPage({ folder, q, tag, limit, after }, signal);

// 6.12: all the pages (limit 500, then each next).
export async function dashboards(signal, params = {}) {
  const items = [];
  let after = null;
  for (let guard = 0; guard < 1000; guard += 1) {
    const page = await dashboardsPage({ limit: 500, ...params, after }, signal);
    items.push(...page.items);
    if (!page.next) break;
    after = page.next;
  }
  return items;
}

// 6.14
export const dashboard = (uid, signal) => request("GET", `api/dashboards/${enc(uid)}`, { signal });

// 6.14 download=true: the canonical bytes (or the stored bytes of an invalid entry) as text.
export async function dashboardDownload(uid, signal) {
  const response = await request("GET", `api/dashboards/${enc(uid)}${query({ download: "true" })}`, { signal, raw: true });
  return { text: await response.text(), etag: (response.headers.get("ETag") || "").replace(/"/g, "") };
}

// 6.20: the stored query of one panel. The body follows 6.20.
export const panelQuery = (uid, panelId, body, signal) =>
  request("POST", `api/dashboards/${enc(uid)}/panels/${enc(panelId)}/query`, { body, signal });

// 6.21
export const variableOptions = (uid, name, body, signal) =>
  request("POST", `api/dashboards/${enc(uid)}/variables/${enc(name)}/options`, { body, signal });

// 6.21a: the stored query of one annotation, by its index. The body has no SQL.
export const annotationQuery = (uid, index, body, signal) =>
  request("POST", `api/dashboards/${enc(uid)}/annotations/${enc(index)}/query`, { body, signal });
