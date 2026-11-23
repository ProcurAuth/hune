// The draft in localStorage (10.8). Every access is in a try block: a failure never stops the page.

import { draftKey, readRecord, resumeNotice } from "./draft.js";
import { absoluteTime, notify } from "../ui.js";

export function readStored(uid) {
  try {
    return window.localStorage.getItem(draftKey(uid));
  } catch (error) {
    return null;
  }
}

// Returns true when the draft is kept.
export function writeStored(uid, record) {
  try {
    window.localStorage.setItem(draftKey(uid), JSON.stringify(record));
    return true;
  } catch (error) {
    return false;
  }
}

export function removeStored(uid) {
  try {
    window.localStorage.removeItem(draftKey(uid));
  } catch (error) {
    // Nothing to remove.
  }
}

// The key of the draft of this uid, for the `storage` event of another tab.
export { draftKey };

// The notice of a draft that the page removes (10.8). A draft of an older format has its own text: the people save or discard a draft before an upgrade.
function discardedNotice(reason) {
  if (reason === "subject") return;
  const text = reason === "format" ? "A draft of an older format was removed. Save or discard your drafts before an upgrade." : "A draft that is not valid was removed.";
  notify({ level: "warning", key: "draft-removed", text });
}

// Reads the stored draft for the editor. Returns { draft } or { none: true }. A draft that cannot be used is removed (10.8).
export function loadDraft(uid, subject) {
  const raw = readStored(uid);
  if (raw === null) return { none: true };
  const read = readRecord(raw, uid, subject);
  if (read.draft) return { draft: read.draft };
  removeStored(uid);
  discardedNotice(read.discard);
  return { none: true };
}

// The notice of a dashboard that has a draft: "Resume" and "Discard" (10.8). `raw` is the stored text.
export function showResumeNotice(api, raw, onResume) {
  const uid = api.uid;
  const read = readRecord(raw, uid, api.session.subject);
  if (!read.draft) {
    removeStored(uid);
    discardedNotice(read.discard);
    return;
  }
  const text = resumeNotice(read.draft, api.loaded.etag, (ms) => absoluteTime(new Date(ms).toISOString()));
  notify({
    level: "info",
    key: "draft",
    text,
    actions: [
      { label: "Resume", onClick: (node) => { node.remove(); onResume(); } },
      { label: "Discard", onClick: (node) => { removeStored(uid); node.remove(); } },
    ],
  });
}
