// Loads a module of js/editor/ on demand (9.2, 10.2). The editor is a later part of the product: when the
// file does not exist, the page shows the notice "The editor is not available in this build.".

import { assetUrl } from "./config.js";
import { showBuildNotice } from "./api.js";
import { notify } from "./ui.js";

const cache = new Map();

// Returns the module namespace, or null after a notice.
export async function loadEditorModule(name) {
  if (cache.has(name)) return cache.get(name);
  try {
    const mod = await import(`./editor/${name}.js`);
    cache.set(name, mod);
    return mod;
  } catch (error) {
    // A 404 of a static file can mean that a new build runs (9.6). The page asks one small file to find out.
    let stale = false;
    try {
      const probe = await fetch(assetUrl("favicon.svg"), { method: "HEAD" });
      stale = probe.status === 404;
    } catch (probeError) {
      stale = false;
    }
    if (stale) showBuildNotice();
    else notify({ level: "warning", key: "editor", text: "The editor is not available in this build." });
    return null;
  }
}
