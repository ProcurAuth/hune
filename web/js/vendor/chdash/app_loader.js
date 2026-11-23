// Shim of ClickHouse Dash app_loader.js (origin "hune", 9.24). Hune has no lazy module groups and no WebAssembly kernel.
// loadGroup() gives a promise that is already fulfilled, so each copy takes its JavaScript path.

const page = Object.freeze({ name: "", entry: "", files: [], lazy: {} });

function loadGroup() {
  return Promise.resolve({});
}

function url(file) {
  return String(file);
}

function expose() {}

export const loader = Object.freeze({ page, url, loadGroup, expose });
