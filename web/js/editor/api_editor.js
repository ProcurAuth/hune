// The routes that only the editor calls: the history, the sources, the write routes and the preview.
// The page does not load this file. A static test refuses these names outside editor/ (9.30). The read routes and the
// request function stay in ../api.js.

import { request, query, enc } from "../api.js";

// 6.17
export const history = (uid, params, signal) => request("GET", `api/dashboards/${enc(uid)}/history${query(params || {})}`, { signal });
export const historyVersion = (uid, n, signal) => request("GET", `api/dashboards/${enc(uid)}/history/${enc(n)}`, { signal });

// 6.22
export const sources = (signal) => request("GET", "api/sources", { signal });

// --- write routes (6.13 to 6.19, 6.11, 6.22) -------------------------------------------------------------------
// The headers of 6.3: Content-Type (set by request), If-Match with double quotes, Idempotency-Key (the save_id).

const WRITE_TIMEOUT_MS = 30000;
const quoted = (etag) => `"${etag}"`;
const write = (method, path, { body, etag, saveId, signal }) =>
  request(method, path, {
    body,
    signal,
    timeoutMs: WRITE_TIMEOUT_MS,
    headers: { ...(etag ? { "If-Match": quoted(etag) } : {}), ...(saveId ? { "Idempotency-Key": saveId } : {}) },
  });

// 6.13: { folder, message, document }.
export const createDashboard = (body, saveId, signal) => write("POST", "api/dashboards", { body, saveId, signal });

// 6.15: { message, document }.
export const saveDashboard = (uid, body, etag, saveId, signal) => write("PUT", `api/dashboards/${enc(uid)}`, { body, etag, saveId, signal });

// 6.15 validate_only: `documentText` is the exact text of the document. A saved dashboard uses PUT, else POST with a folder.
export function validateDocument({ uid = null, folder = null }, documentText, signal) {
  if (uid) return request("PUT", `api/dashboards/${enc(uid)}?validate_only=true`, { body: `{"document":${documentText}}`, signal });
  return request("POST", "api/dashboards?validate_only=true", { body: `{"folder":${JSON.stringify(folder)},"document":${documentText}}`, signal });
}

// 6.16
export const deleteDashboard = (uid, etag, saveId, signal) => write("DELETE", `api/dashboards/${enc(uid)}`, { etag, saveId, signal });

// 6.18: { version, message }.
export const restoreVersion = (uid, body, etag, saveId, signal) => write("POST", `api/dashboards/${enc(uid)}/restore`, { body, etag, saveId, signal });

// 6.19: { folder, message }.
export const moveDashboard = (uid, body, etag, saveId, signal) => write("POST", `api/dashboards/${enc(uid)}/move`, { body, etag, saveId, signal });

// 6.17 download=true of a version: the stored bytes as text.
export async function versionDownload(uid, n, signal) {
  const response = await request("GET", `api/dashboards/${enc(uid)}/history/${enc(n)}${query({ download: "true" })}`, { signal, raw: true });
  return response.text();
}

// 6.11
export const createFolder = (body, signal) => request("POST", "api/folders", { body, signal });
export const renameFolder = (id, title, signal) => request("PUT", `api/folders/${enc(id)}`, { body: { title }, signal });
export const deleteFolder = (id, signal) => request("DELETE", `api/folders/${enc(id)}`, { signal });

// 6.22: the body follows 6.22 (uid or folder, from_ms, to_ms, max_data_points, dashboard_variables, variables, panel or variable).
export const preview = (sourceId, body, signal) => request("POST", `api/sources/${enc(sourceId)}/preview`, { body, signal });
