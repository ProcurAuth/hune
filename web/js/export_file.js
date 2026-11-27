// The download of a text as a file (10.12): the export of a dashboard and the files of the editor (the history, the diff, the conflict).

import * as api from "./api.js";
import { el, notify } from "./ui.js";

// Saves a text as a file in the browser.
export function saveTextFile(name, text) {
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  const link = el("a", { href: url, download: name });
  document.body.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export async function exportDashboard(uid) {
  try {
    const { text } = await api.dashboardDownload(uid);
    saveTextFile(`${uid}.json`, text);
  } catch (error) {
    notify({ level: "error", text: error.message });
  }
}
