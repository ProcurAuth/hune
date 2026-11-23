// Shim of ClickHouse Dash app_dom.js (origin "hune", 9.24). It exports only the names that the copied modules import:
// `dom` ($, $$). The edit E20 of app_ui_table.js removed the last imports of `h` and `shell` (engine-patches.md).
// No DOM access happens at import time.

function $(selector, root) {
  return (root || document).querySelector(selector);
}

function $$(selector, root) {
  return Array.from((root || document).querySelectorAll(selector));
}

const dom = Object.freeze({ $, $$ });

export { dom };
