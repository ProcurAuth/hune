// Shim of ClickHouse Dash app_wasm.js (origin "hune", 9.24, 9.27). v1 has no WebAssembly kernel.
// `ops` is empty, `get()` gives null and `load()` gives a promise of null, so each copy takes its JavaScript path.

const ops = {};

export const wasm = {
  supported: false,
  ops,
  md: {},
  get: () => null,
  load: () => Promise.resolve(null),
  stats: () => ({}),
  urls: {},
  maxInputUnits: 0,
};
