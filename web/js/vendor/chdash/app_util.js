// hune: E35. The page and the editor call only escapeHtml (app_chart_core.js, app_ui_chart.js, app_highlight.js) and rafOnce
// (app_chart_core.js). The other helpers, their constants and the import of app_format.js are removed (engine-patches.md, Removals).

function escapeHtml(text) {
  return String(text)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

// fn once on the next animation frame however often it is asked for (the
// last arguments win); .cancel() drops it.
function rafOnce(fn) {
  let frame = 0;
  let args = [];
  const scheduled = (...next) => {
    args = next;
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      fn(...args);
    });
  };
  scheduled.cancel = () => { if (frame) cancelAnimationFrame(frame); frame = 0; };
  scheduled.pending = () => frame !== 0;
  return scheduled;
}

// hune: E35. The other members of `util` are removed.
export const util = {
  rafOnce,
  escapeHtml,
};
