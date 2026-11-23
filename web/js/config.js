// Boot data and URL helpers shared by all modules (9.3, 9.6).
// The shell holds the build hash and the version line in the JSON block "hune-boot". The base path comes from <base href>.

const text = (value) => (typeof value === "string" ? value : "");

// Reads the text of the boot block. A missing or bad block gives empty values.
export function parseBoot(source) {
  try {
    const data = JSON.parse(source || "{}");
    return { build: text(data.build), version: text(data.version), versionTitle: text(data.version_title) };
  } catch (error) {
    return { build: "", version: "", versionTitle: "" };
  }
}

function readBoot() {
  const node = typeof document === "undefined" ? null : document.getElementById("hune-boot");
  return parseBoot(node ? node.textContent : "");
}

export const boot = readBoot();

// The state that main.js fills at start: the answer of GET api/session (6.10).
export const app = { session: null };

// The URL of a static file, relative to <base href> (9.3).
export function assetUrl(path) {
  return `static/${boot.build}/${path}`;
}

// The path of <base href>, always with a final "/".
export function basePath() {
  const path = new URL(document.baseURI).pathname;
  return path.endsWith("/") ? path : `${path}/`;
}

// The path of the current location relative to the base path, without query and fragment.
export function relativePath(pathname = window.location.pathname) {
  const base = basePath();
  return pathname.startsWith(base) ? pathname.slice(base.length) : pathname.replace(/^\/+/, "");
}
