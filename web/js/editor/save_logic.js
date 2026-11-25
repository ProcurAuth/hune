// The logic of a save (10.9, 10.10, 10.13): the attempt with its save_id, and what the page does with each answer.
// Pure module: no DOM access.

import { makeSaveId } from "./draft.js";
import { READ_ONLY_ERROR_TEXT } from "../stores.js";

// A save attempt keeps one save_id. A change of the draft ends the attempt (10.9, rule 5).
export class SaveAttempt {
  constructor(fill) {
    this.fill = fill;
    this.saveId = null;
  }

  // The save_id of this attempt: made when the draft has none, kept for "Retry".
  id() {
    if (!this.saveId) this.saveId = makeSaveId(this.fill);
    return this.saveId;
  }

  // A change of the draft after a failed save.
  reset() {
    this.saveId = null;
  }
}

/**
 * What the page does with an error of a save (the table of 10.9).
 * Result: { kind, text } where kind is conflict, invalid, reused, retry, unauthenticated or show.
 */
export function classifySaveError(error) {
  const code = error && error.code;
  const status = error && error.status;
  if (code === "conflict") return { kind: "conflict", text: "" };
  if (code === "invalid_document") return { kind: "invalid", text: "The document has errors." };
  if (code === "idempotency_key_reused") return { kind: "reused", text: "Select Save again." };
  if (status === 401 || code === "unauthenticated") return { kind: "unauthenticated", text: "" };
  if (code === "storage_unavailable" || code === "network_error" || code === "request_timeout" || status === 0) {
    return { kind: "retry", text: "Saving is not possible now. Your draft is kept in this browser." };
  }
  if (code === "read_only_store") return { kind: "show", text: READ_ONLY_ERROR_TEXT };
  if (code === "forbidden" || code === "editing_disabled") return { kind: "show", text: error.message };
  if (code === "payload_too_large") return { kind: "show", text: "The dashboard is too large." };
  if (code === "storage_full") return { kind: "show", text: "The store is full. Tell the operator." };
  return { kind: "show", text: `${code || "error"}: ${error && error.message}` };
}

// The text of a successful save (10.9).
export function savedText(answer) {
  return answer && answer.noop ? "Nothing changed." : `Saved as version ${answer.version}.`;
}

// The kind of a conflict (10.10): "move" when the etag did not change and the folder did, else "changed". A folder is an id, or `null` for the root:
// a dashboard moved to the root has `current_folder` null; an answer with no member `current_folder` is never a move.
export function conflictKind(error, baseEtag, pageFolder) {
  const extra = (error && error.extra) || {};
  if (extra.current_etag === baseEtag && extra.current_folder !== undefined && extra.current_folder !== (pageFolder ?? null)) return "move";
  return "changed";
}

// A delete that is repeated with the same save_id: a 404 counts as success (10.13).
export function deleteRetryIsDone(error) {
  return !!error && error.code === "not_found";
}

// The file name of a draft download (10.10): <uid>-draft-<YYYYMMDD-HHmmss>.json in the zone of the browser.
export function draftFileName(uid, date) {
  const p = (n) => String(n).padStart(2, "0");
  return `${uid}-draft-${date.getFullYear()}${p(date.getMonth() + 1)}${p(date.getDate())}-${p(date.getHours())}${p(date.getMinutes())}${p(date.getSeconds())}.json`;
}

// Grafana files stop an import (10.12 step 2). `value` is the parsed JSON.
export function isGrafanaFile(value) {
  if (!value || typeof value !== "object") return false;
  if (typeof value.apiVersion === "string" && value.apiVersion.startsWith("dashboard.grafana.app/")) return true;
  if ("schemaVersion" in value && "panels" in value) return true;
  return !!(value.dashboard && typeof value.dashboard === "object" && "panels" in value.dashboard);
}

// The body of a validate_only request: the exact text of the tab in the member `document` (10.4).
export function validateBody(documentText) {
  return `{"document":${documentText}}`;
}
