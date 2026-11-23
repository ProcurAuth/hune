// Shim of ClickHouse Dash app_results_values.js (origin "hune", 9.24). It exports only NUMERIC_RE, the number test of app_results_sort.js.
// hune: E28. A run of digits matches in one way only, so the time is linear (the upstream pattern took the square of the length).

export const NUMERIC_RE = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
