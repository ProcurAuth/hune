// Shim of ClickHouse Dash app_ui_sql.js (origin "hune", 9.24). app_highlight.js registers itself with useHighlighter.
// The editor (section 10) reads the highlighter from here.

let highlighter = null;

const sqlBlock = {
  useHighlighter(fn) {
    highlighter = fn;
  },
  highlighter() {
    return highlighter;
  },
};

export { sqlBlock };
